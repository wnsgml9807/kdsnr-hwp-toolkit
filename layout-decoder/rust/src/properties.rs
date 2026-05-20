//! `Hnc::Property::*` — Phase B-8a part 2.
//!
//! 한컴 `Hnc::Property::PropertyKey` + `PropertyBag` + typed `Get<T>` helpers 의 1:1 포팅.
//!
//! ## ARM64 asm 검증 결과 (FUN_0067d0e4 외 5종)
//!
//! `FUN_0067d0e4` (uint), `FUN_0067ffc4` (int), `FUN_006800ac` (int), `FUN_006805d0` (int[2]),
//! `FUN_0065616c` (float), `FUN_00662d4c` (char) — **6종 모두 어셈블리 byte-identical**.
//! C++ template `Hnc::Property::PropertyBag::Get<T>` 의 서로 다른 instantiation 이지만
//! deduplicated 되어 동일 body. 차이는 caller 의 `*ptr` 해석 type 뿐.
//!
//! ## BST 검색 알고리즘 (asm 분석)
//!
//! ```text
//! PropertyBag::Get(PropertyKey const&):
//!   1. x22 = *(bag + 0x10)         # root of BST
//!   2. if x22 == 0: throw "GetValue"
//!   3. x21 = bag + 0x10            # parent tracker (initially same as &root)
//!   4. loop:
//!        if key < x22.key: descend left  (x8 = x22[+0x8])
//!        else:             descend right (x8 = x22)
//!        x21 = (left ? x21 : x22)
//!        x22 = *x8
//!        if x22 == 0: break
//!   5. if x21 == bag+0x10: throw  # never found
//!   6. if key < x21.key: throw    # found key > search → not equal
//!   7. return &(x21[+0x30])[+0xc] # value pointer + 0xc offset
//! ```
//!
//! BST node layout:
//! - +0..+8: parent / unused
//! - +8..+10: right child pointer
//! - +0x10: ?
//! - +0x20: PropertyKey (embedded, 16 bytes)
//! - +0x30: value object pointer
//! - value object at +0xc: actual data
//!
//! ## PropertyKey layout (16 bytes)
//!
//! - +0: key value (u32, e.g. `0x89e`, `0x8fc`)
//! - +4: extra/version (often 0)
//! - +8: padding (u64, set to 0)
//!
//! 한컴 코드:
//! ```c
//! local_f0 = (long *)CONCAT44(local_f0._4_4_, 0x89e);  // low 32 = key
//! uStack_e8 = 0;                                         // next 8 bytes = 0
//! ```
//!
//! Float-encoded key (`3.22999e-42`) 는 i32 reinterpreted (`0xtxxx_xxxx` 으로 들어옴).
//! Rust 에선 raw u32 로 처리.

use std::collections::{BTreeMap, HashMap};

// ============================================================
// PropertyKey
// ============================================================

/// `Hnc::Property::PropertyKey` — 16-byte property identifier.
///
/// 한컴 원본 layout (constructor 패턴 + destructor 존재 — 추후 string ref 가능성):
/// - +0: u32 (primary key value)
/// - +4: u32 (extra/version, 보통 0)
/// - +8: u64 (padding/extra, 보통 0)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct PropertyKey {
    /// `+0` — primary key value (`0x89e`, `0x8fc`, etc.).
    pub value: u32,
    /// `+4` — version/namespace (보통 0).
    pub extra: u32,
    /// `+8` — padding (한컴 destructor 가 활용할 수 있는 자리, stub 에선 0).
    pub padding: u64,
}

impl PropertyKey {
    /// 가장 일반적인 PropertyKey 생성 (단순 u32 key).
    pub const fn new(value: u32) -> Self {
        Self { value, extra: 0, padding: 0 }
    }

    /// 한컴 디코드의 `(float)constant` PropertyKey 표현 대응.
    /// `3.22999e-42` 등은 i32 reinterpreted bit pattern.
    pub fn from_float_bits(f: f32) -> Self {
        Self::new(f.to_bits())
    }
}

// ============================================================
// PropertyBag trait
// ============================================================

/// `Hnc::Property::PropertyBag` 의 인터페이스.
///
/// 한컴 원본은 BST 기반 map. Rust 에선 generic trait 으로 추상화 (HashMap 등 다양한 backend
/// 가능). 각 typed getter 는 C++ `Get<T>` 의 6종 instantiation 에 대응.
pub trait PropertyBag {
    /// `Hnc::Property::PropertyBag::Contains(PropertyKey const&)` — bool 반환.
    fn contains(&self, key: PropertyKey) -> bool;

    /// `FUN_0067d0e4` — uint property lookup.
    fn get_uint(&self, key: PropertyKey) -> Option<u32>;

    /// `FUN_0067ffc4` / `FUN_006800ac` — int property lookup. (두 변종 asm 동일)
    fn get_int(&self, key: PropertyKey) -> Option<i32>;

    /// `FUN_006805d0` — int pair `{type, value}` property (line height variants).
    /// 한컴 원본: `int piVar13[2]` — `piVar13[0]` = type (0/1), `piVar13[1]` = value.
    fn get_int_pair(&self, key: PropertyKey) -> Option<(i32, i32)>;

    /// `FUN_006805d0` — same 8-byte payload as `get_int_pair` but interpreted as
    /// `{i32 mode, f32 factor}`. Keys like `0x90f` (`ParaProperty::GetBulletSize`)
    /// store `{mode, factor}` (`piVar12[0]` = mode, `(float)piVar12[1]` = factor in
    /// raw `FUN_002eaf54`). Provided as a separate accessor to keep callers type-safe.
    fn get_int_float(&self, key: PropertyKey) -> Option<(i32, f32)>;

    /// `FUN_0065616c` — float property lookup.
    fn get_float(&self, key: PropertyKey) -> Option<f32>;

    /// `FUN_00662d4c` — char/bool property lookup.
    fn get_char(&self, key: PropertyKey) -> Option<u8>;
}

// ============================================================
// PropertyValue (union-ish)
// ============================================================

/// 한 PropertyBag entry 의 typed value.
///
/// 실제 한컴은 BST node 의 value 객체가 generic (vtable 기반 polymorphic).
/// Rust 에선 enum 으로 표현.
///
/// `IntFloat` 는 raw `FUN_006805d0` 가 반환하는 8-byte `{ i32 mode, f32 factor }`
/// 페이로드용 — `ParaProperty::GetBulletSize` (key `0x90f`) 와 같이 mode + factor 를
/// 함께 들고 다니는 properties 에 사용된다. `(i32, i32)` 비트캐스트 대신 별도
/// variant 로 모델해 type-safety 를 유지한다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyValue {
    Uint(u32),
    Int(i32),
    IntPair(i32, i32),
    /// raw `FUN_006805d0` 반환 8-byte struct `{ i32 mode, f32 factor }`.
    /// 예: ParaProperty key `0x90f` (BulletSize): mode `1` = absolute pt, else factor mode.
    IntFloat(i32, f32),
    Float(f32),
    Char(u8),
}

// ============================================================
// HashMap-backed default impl (테스트 / stub 용)
// ============================================================

/// HashMap 기반 PropertyBag impl. 한컴 원본의 BST 대신 사용 (성능 동등, semantic 동등).
#[derive(Debug, Default, Clone)]
pub struct HashMapPropertyBag {
    entries: HashMap<PropertyKey, PropertyValue>,
}

impl HashMapPropertyBag {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, key: PropertyKey, value: PropertyValue) -> &mut Self {
        self.entries.insert(key, value);
        self
    }

    pub fn with(mut self, key: PropertyKey, value: PropertyValue) -> Self {
        self.insert(key, value);
        self
    }
}

impl PropertyBag for HashMapPropertyBag {
    fn contains(&self, key: PropertyKey) -> bool {
        self.entries.contains_key(&key)
    }

    fn get_uint(&self, key: PropertyKey) -> Option<u32> {
        match self.entries.get(&key)? {
            PropertyValue::Uint(v) => Some(*v),
            _ => None,
        }
    }

    fn get_int(&self, key: PropertyKey) -> Option<i32> {
        match self.entries.get(&key)? {
            PropertyValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    fn get_int_pair(&self, key: PropertyKey) -> Option<(i32, i32)> {
        match self.entries.get(&key)? {
            PropertyValue::IntPair(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    fn get_int_float(&self, key: PropertyKey) -> Option<(i32, f32)> {
        match self.entries.get(&key)? {
            PropertyValue::IntFloat(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    fn get_float(&self, key: PropertyKey) -> Option<f32> {
        match self.entries.get(&key)? {
            PropertyValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    fn get_char(&self, key: PropertyKey) -> Option<u8> {
        match self.entries.get(&key)? {
            PropertyValue::Char(v) => Some(*v),
            _ => None,
        }
    }
}

// ============================================================
// BTreeMap-backed impl (한컴 BST 와 ordered 동등)
// ============================================================

/// BTreeMap (ordered map) 기반 PropertyBag — 한컴 BST 와 의미 + 순회 순서 동등.
///
/// 한컴 BST 는 PropertyKey 의 `+0` u32 value 로 정렬됨 (asm 의 `cmp x9, x10` 패턴).
/// `BTreeMap` 도 `Ord` 기준 정렬 — `PropertyKey` 의 `derive Ord` 가 lexicographic
/// (`value`, `extra`, `padding`) 순서로 비교. `extra`/`padding` 이 모두 0인 정상 케이스에선
/// 한컴 BST 와 동일한 순서를 보장.
///
/// **언제 사용**: HashMap impl 과 의미적으로 동등 — layout output 의 byte-equivalence 에는
/// 영향 없음. 디버깅 시 iteration 순서 결정성이 필요할 때 사용.
#[derive(Debug, Default, Clone)]
pub struct BTreeMapPropertyBag {
    entries: BTreeMap<PropertyKey, PropertyValue>,
}

impl BTreeMapPropertyBag {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, key: PropertyKey, value: PropertyValue) -> &mut Self {
        self.entries.insert(key, value);
        self
    }

    pub fn with(mut self, key: PropertyKey, value: PropertyValue) -> Self {
        self.insert(key, value);
        self
    }

    /// BST 와 동등한 in-order iteration. 한컴 BST 의 in-order traversal 과 일치.
    pub fn iter_in_order(&self) -> impl Iterator<Item = (&PropertyKey, &PropertyValue)> {
        self.entries.iter()
    }
}

impl PropertyBag for BTreeMapPropertyBag {
    fn contains(&self, key: PropertyKey) -> bool { self.entries.contains_key(&key) }
    fn get_uint(&self, key: PropertyKey) -> Option<u32> {
        match self.entries.get(&key)? {
            PropertyValue::Uint(v) => Some(*v),
            _ => None,
        }
    }
    fn get_int(&self, key: PropertyKey) -> Option<i32> {
        match self.entries.get(&key)? {
            PropertyValue::Int(v) => Some(*v),
            _ => None,
        }
    }
    fn get_int_pair(&self, key: PropertyKey) -> Option<(i32, i32)> {
        match self.entries.get(&key)? {
            PropertyValue::IntPair(a, b) => Some((*a, *b)),
            _ => None,
        }
    }
    fn get_int_float(&self, key: PropertyKey) -> Option<(i32, f32)> {
        match self.entries.get(&key)? {
            PropertyValue::IntFloat(a, b) => Some((*a, *b)),
            _ => None,
        }
    }
    fn get_float(&self, key: PropertyKey) -> Option<f32> {
        match self.entries.get(&key)? {
            PropertyValue::Float(v) => Some(*v),
            _ => None,
        }
    }
    fn get_char(&self, key: PropertyKey) -> Option<u8> {
        match self.entries.get(&key)? {
            PropertyValue::Char(v) => Some(*v),
            _ => None,
        }
    }
}

// ============================================================
// Known PropertyKey constants from PptCompositor::ComposeLayout RE
// ============================================================

/// `PptCompositor::ComposeLayout` 에서 사용되는 PropertyKey 상수.
///
/// 각 키의 의미는 RE 진행 중 확정. 현재는 디코드에서 식별된 값 + 추정 의미.
pub mod keys {
    use super::PropertyKey;

    /// `0x89e` — paragraph class (uVar32 in ComposeLayout stage 1).
    /// `FUN_0067d0e4` 로 uint 추출. 값 5~6 이면 special 처리.
    pub const PARAGRAPH_CLASS: PropertyKey = PropertyKey::new(0x89e);

    /// `0x8fc` — paragraph alignment type (iVar34 in stage 5).
    /// 0~6 의 switch case.
    pub const ALIGNMENT_TYPE: PropertyKey = PropertyKey::new(0x8fc);

    /// `0x8fd` — spacing type variant (stage 9 switch).
    pub const SPACING_TYPE: PropertyKey = PropertyKey::new(0x8fd);

    /// `0x900` — vertical anchor (stage 6, uVar46).
    pub const VERTICAL_ANCHOR: PropertyKey = PropertyKey::new(0x900);

    /// `0x907` — line height type 1 (stage 9, fVar39 / iVar8).
    pub const LINE_HEIGHT_TYPE1: PropertyKey = PropertyKey::new(0x907);

    /// `0x909` — line height type 3 (stage 9, local_144 / iVar9).
    pub const LINE_HEIGHT_TYPE3: PropertyKey = PropertyKey::new(0x909);

    /// `0x96a` — font size (CharItemView constructor, stage 8 line 1056).
    pub const FONT_SIZE: PropertyKey = PropertyKey::new(0x96a);

    /// `0x899` — special bool flag (stage 9, uVar32).
    pub const SPECIAL_FLAG: PropertyKey = PropertyKey::new(0x899);

    /// `0x96c` — adjusted font size flag (CharItemView constructor).
    pub const FONT_SIZE_ADJUST: PropertyKey = PropertyKey::new(0x96c);

    /// `0x967` — bold flag (CharItemView constructor).
    pub const BOLD_FLAG: PropertyKey = PropertyKey::new(0x967);

    /// `0x968` — italic flag (CharItemView constructor).
    pub const ITALIC_FLAG: PropertyKey = PropertyKey::new(0x968);

    /// `0x96b` — additional metric (CharItemView constructor).
    pub const METRIC_96B: PropertyKey = PropertyKey::new(0x96b);

    /// `3.22999e-42` — float-encoded key for spacing-related lookup (stage 6).
    /// `0x00000901` bit pattern = 2305 — 추정상 line spacing.
    pub fn line_spacing_a() -> PropertyKey {
        PropertyKey::from_float_bits(3.22999e-42)
    }

    /// `3.22719e-42` — float-encoded key (stage 6).
    /// `0x00000900` 와 다른 bit pattern.
    pub fn line_spacing_b() -> PropertyKey {
        PropertyKey::from_float_bits(3.22719e-42)
    }

    /// `3.2398e-42` — float-encoded key (stage 9).
    pub fn line_height_extra() -> PropertyKey {
        PropertyKey::from_float_bits(3.2398e-42)
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_key_size() {
        assert_eq!(std::mem::size_of::<PropertyKey>(), 16);
    }

    #[test]
    fn property_key_ordering() {
        // 한컴 PropertyKey::operator< 호출 패턴 대응
        let k1 = PropertyKey::new(0x100);
        let k2 = PropertyKey::new(0x200);
        assert!(k1 < k2);
        assert!(!(k2 < k1));
        assert!(k1 == PropertyKey::new(0x100));
    }

    #[test]
    fn property_key_from_float_bits() {
        // 3.22999e-42 → bit pattern
        let k = PropertyKey::from_float_bits(3.22999e-42);
        // 정확한 bit pattern 은 RE 후 검증 — 일단 동일 float → 동일 bits 확인
        assert_eq!(k.value, 3.22999e-42_f32.to_bits());
    }

    #[test]
    fn hash_map_bag_basic() {
        let bag = HashMapPropertyBag::new()
            .with(keys::PARAGRAPH_CLASS, PropertyValue::Uint(5))
            .with(keys::ALIGNMENT_TYPE, PropertyValue::Int(2));

        assert!(bag.contains(keys::PARAGRAPH_CLASS));
        assert!(bag.contains(keys::ALIGNMENT_TYPE));
        assert!(!bag.contains(keys::SPACING_TYPE));

        assert_eq!(bag.get_uint(keys::PARAGRAPH_CLASS), Some(5));
        assert_eq!(bag.get_int(keys::ALIGNMENT_TYPE), Some(2));
    }

    #[test]
    fn hash_map_bag_type_mismatch_returns_none() {
        // PARAGRAPH_CLASS 는 Uint 인데 get_int 로 조회 → None
        let bag = HashMapPropertyBag::new()
            .with(keys::PARAGRAPH_CLASS, PropertyValue::Uint(5));
        assert_eq!(bag.get_int(keys::PARAGRAPH_CLASS), None);
        assert_eq!(bag.get_uint(keys::PARAGRAPH_CLASS), Some(5));
    }

    #[test]
    fn int_pair_for_line_height() {
        // stage 9 line height: {type, value} pair
        let bag = HashMapPropertyBag::new()
            .with(keys::LINE_HEIGHT_TYPE1, PropertyValue::IntPair(0, 200));
        let (ty, val) = bag.get_int_pair(keys::LINE_HEIGHT_TYPE1).unwrap();
        assert_eq!(ty, 0);
        assert_eq!(val, 200);
    }

    #[test]
    fn float_lookup_for_spacing() {
        // stage 6 의 line_spacing 변종
        let bag = HashMapPropertyBag::new()
            .with(keys::line_spacing_a(), PropertyValue::Float(12.5))
            .with(keys::line_spacing_b(), PropertyValue::Float(8.0));
        assert_eq!(bag.get_float(keys::line_spacing_a()), Some(12.5));
        assert_eq!(bag.get_float(keys::line_spacing_b()), Some(8.0));
    }

    #[test]
    fn char_lookup_for_bool_flag() {
        // stage 9 의 SPECIAL_FLAG (`FUN_00662d4c` 반환 char)
        let bag = HashMapPropertyBag::new()
            .with(keys::SPECIAL_FLAG, PropertyValue::Char(1));
        assert_eq!(bag.get_char(keys::SPECIAL_FLAG), Some(1));
    }

    #[test]
    fn known_keys_have_unique_values() {
        // 같은 키가 다른 의미로 매핑되지 않게 확인
        let all = [
            keys::PARAGRAPH_CLASS,
            keys::ALIGNMENT_TYPE,
            keys::SPACING_TYPE,
            keys::VERTICAL_ANCHOR,
            keys::LINE_HEIGHT_TYPE1,
            keys::LINE_HEIGHT_TYPE3,
            keys::FONT_SIZE,
            keys::SPECIAL_FLAG,
            keys::FONT_SIZE_ADJUST,
            keys::BOLD_FLAG,
            keys::ITALIC_FLAG,
            keys::METRIC_96B,
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "duplicate key at indices {i}, {j}");
            }
        }
    }

    // ────── BTreeMapPropertyBag (한컴 BST 동등) ──────

    #[test]
    fn btree_bag_semantic_equivalent_to_hashmap() {
        let h = HashMapPropertyBag::new()
            .with(keys::PARAGRAPH_CLASS, PropertyValue::Uint(5))
            .with(keys::FONT_SIZE, PropertyValue::Float(14.0))
            .with(keys::BOLD_FLAG, PropertyValue::Char(1));
        let b = BTreeMapPropertyBag::new()
            .with(keys::PARAGRAPH_CLASS, PropertyValue::Uint(5))
            .with(keys::FONT_SIZE, PropertyValue::Float(14.0))
            .with(keys::BOLD_FLAG, PropertyValue::Char(1));
        assert_eq!(h.get_uint(keys::PARAGRAPH_CLASS), b.get_uint(keys::PARAGRAPH_CLASS));
        assert_eq!(h.get_float(keys::FONT_SIZE), b.get_float(keys::FONT_SIZE));
        assert_eq!(h.get_char(keys::BOLD_FLAG), b.get_char(keys::BOLD_FLAG));
        assert_eq!(h.contains(keys::ITALIC_FLAG), b.contains(keys::ITALIC_FLAG));
    }

    #[test]
    fn btree_bag_in_order_iteration() {
        // 한컴 BST 의 in-order = key.value 오름차순
        // 0x89e(2206), 0x8fc(2300), 0x967(2407), 0x96a(2410)
        let b = BTreeMapPropertyBag::new()
            .with(keys::FONT_SIZE, PropertyValue::Float(14.0))      // 0x96a
            .with(keys::PARAGRAPH_CLASS, PropertyValue::Uint(5))    // 0x89e
            .with(keys::BOLD_FLAG, PropertyValue::Char(1))          // 0x967
            .with(keys::ALIGNMENT_TYPE, PropertyValue::Uint(3));    // 0x8fc
        let ordered: Vec<_> = b.iter_in_order().map(|(k, _)| k.value).collect();
        assert_eq!(ordered, vec![0x89e, 0x8fc, 0x967, 0x96a]);
    }
}
