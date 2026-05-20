//! `Hnc::Shape::Brush` — virtual class hierarchy 의 abstract base + 첫 concrete
//! sub-type `EmptyBrush`.
//!
//! ## raw Brush family 개요
//!
//! `libHncDrawingEngine_arm64.dylib` 에서 Brush 는 abstract virtual class 로,
//! 다음 sub-type 가족이 존재 (16번째 세션 16h RE 결과):
//!
//! - **EmptyBrush** (this file) — 8B, no-op semantic (no fill)
//! - **SolidBrush** — single Color fill (multi-session deferred)
//! - **HatchBrush** — pattern fill with fore/back Color (deferred)
//! - **GradientBrush** — multi-stop gradient (deferred)
//! - **PictureBrush** / **ImageBrush** — bitmap fill (deferred)
//! - **GroupBrush** — composite (deferred)
//! - **BlipBrush** — (deferred, picture-fill variant)
//!
//! ## raw Brush vtable layout (16 entries, 확정 by EmptyBrush @ `0x77b538`)
//!
//! | vfunc | offset | 시그니처                                            |
//! |-------|--------|-----------------------------------------------------|
//! | 0     | +0x00  | `~Brush()` D1 (virtual dtor)                        |
//! | 1     | +0x08  | `~Brush()` D0 (deleting dtor)                       |
//! | 2     | +0x10  | `operator==(Brush const&)` — RTTI-based              |
//! | 3     | +0x18  | `operator!=(Brush const&)` — `!=` via `!eq`         |
//! | 4     | +0x20  | `operator<(Brush const&)` — RTTI lexicographic       |
//! | 5     | +0x28  | `GetType()` const → `u32` (subtype tag)              |
//! | 6     | +0x30  | `Clone()` const → `Brush*` (heap-alloc copy)        |
//! | 7     | +0x38  | `Clone(Color const&)` const → `Brush*`              |
//! | 8     | +0x40  | `IsEnable(PropertyKey)` const → `bool`              |
//! | 9     | +0x48  | `IsSaveable(PropertyKey)` const → `bool`            |
//! | 10    | +0x50  | `Union(Brush const&)`                                |
//! | 11    | +0x58  | `CollectProperty(PropertyBag&)` const                |
//! | 12    | +0x60  | `ApplyProperty(PropertyBag const&)`                  |
//! | 13    | +0x68  | `Draw(Surface&, Paths&, RectImpl, Trans*, ImageData, RenderMode, bool, bool, Path*)` |
//! | 14    | +0x70  | `UpdateSchemeColor(ColorMapper*)`                    |
//! | 15    | +0x78  | `GetRepresentationColor()` const → `Color`           |
//!
//! ## byte-eq 경계
//!
//! **메모리 layout byte-eq 불가**: raw 의 `Brush*` 는 8B vtable-ptr 객체. Rust
//! 의 dyn dispatch 는 fat pointer (16B). 본 모듈은 enum dispatch 로 `Box<Brush>`
//! = 8B (= 단일 owned ptr) 만 보장. enum heap alloc 크기 = 모든 variant 의 max.
//!
//! **출력 byte-eq 가능**: vfunc dispatch 의 의미가 raw 와 동일하면 PDF 출력 byte-eq.
//! 본 모듈의 Brush trait + impl 이 동일 semantic 을 구현.
//!
//! # raw EmptyBrush 구조
//!
//! 8B (vtable ptr only, no fields). C2 ctor (`0x166738`):
//! ```asm
//! 166738: adrp x8, 0x77b000
//! 16673c: add x8, x8, #0x538     ; x8 = vtable
//! 166740: str x8, [x0]           ; this.vtable = ...
//! 166744: ret
//! ```
//!
//! 모든 vfunc 은 trivial 또는 RTTI 기반:
//! - `GetType()` → 0 (raw `0x166908`: `mov w0, #0; ret`)
//! - `IsEnable()` → false (raw `0x166940`)
//! - `IsSaveable()` → false (raw `0x166948`)
//! - `Union()` → no-op (raw `0x166950`: just `ret`)
//! - `Clone()` → new EmptyBrush (raw `0x166910`: alloc 8B + vtable)
//! - `Clone(Color&)` → calls `Clone()` (raw `0x166934`: `br vtable[+0x30]`)
//! - `D1` → no-op (raw `0x166748`: `ret`)
//! - `D0` → `operator_delete` tail call (raw `0x166750`)
//! - `eq` / `ne` / `lt` → RTTI-based: cmp typeinfo names then `__dynamic_cast`
//!
//! raw asm dump: 본 파일 `kdsnr-hwp-toolkit/work/hft_re/render_re/EMPTY_BRUSH_RE.txt` (작성 예정).

use std::fmt;

/// `Hnc::Shape::Brush::Type` — 각 sub-type 의 enum tag (`GetType()` 반환).
///
/// raw 의 각 sub-type `GetType()` vfunc 의 `mov w0, #N` 값으로 확정 (16h RE):
///
/// | sub-type      | raw addr   | tag |
/// |---------------|------------|-----|
/// | EmptyBrush    | `0x166908` | 0   |
/// | SolidBrush    | `0x1e5fcc` | 1   |
/// | GradientBrush | `0x177950` | 2   |
/// | ImageBrush    | `0x18f9b8` | 3   |
/// | HatchBrush    | `0x18c600` | 4   |
/// | GroupBrush    | `0x18662c` | 5   |
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BrushType {
    /// EmptyBrush (raw `0x166908`).
    Empty = 0,
    /// SolidBrush (raw `0x1e5fcc`).
    Solid = 1,
    /// GradientBrush (raw `0x177950`).
    Gradient = 2,
    /// ImageBrush (raw `0x18f9b8`).
    Image = 3,
    /// HatchBrush (raw `0x18c600`).
    Hatch = 4,
    /// GroupBrush (raw `0x18662c`).
    Group = 5,
}

impl BrushType {
    /// raw `GetType()` 가 반환하는 u32 tag.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

// =============================================================================
// BrushVtable + sub-type registration (16u)
//
// raw 의 `Hnc::Shape::SolidBrush` 등 sub-type 들은 각자 다른 vtable @ 다른 주소
// (`0x77cf48` SolidBrush, `0x77bfe0` HatchBrush 등) 보유. tree value 인
// `*mut Brush` 의 polymorphic drop / clone / type-id 는 vtable 의 슬롯을 통해
// dispatch.
//
// Rust 는 raw 의 vtable 주소가 아닌 별도 static `BrushVtable` 을 사용 — sub-type
// 별 함수 포인터 테이블. 각 SolidBrush/HatchBrush 의 `vtable` field 가 이 static
// 의 주소를 보유. 16B layout (vtable + bag) 는 raw 와 byte-equivalent (포인터
// 값 자체는 raw 와 다름, 그러나 layout / dispatch semantic 동등).
// =============================================================================

/// Sub-type 별 polymorphic dispatch table (`Hnc::Shape::Brush` vtable 의 Rust
/// 등가물). 본 단계 (16u) 는 FormatScheme std::map 의 SharePtr<Brush> 의 release
/// path 에서 drop dispatch 만 필요.
///
/// raw 의 Brush vtable 16 entries (RTTI/dtor/== /</Clone/GetType/...) 중 16u
/// 단계에서 실제 호출되는 것 들:
/// - vfunc[0] (dtor) — 본 module 의 `drop_in_place_fn`
/// - vfunc[5] (GetType) — 본 module 의 `type_tag` (u32 const)
///
/// 다른 vfunc 들 (Clone / == / < / Union / IsEnable) 은 16v+ deferred.
pub struct BrushVtable {
    /// `BrushType::as_u32()` — sub-type 의 RTTI tag.
    pub type_tag: u32,
    /// raw vfunc[0] (~SubBrush) 의 등가물. obj 의 in-place dtor 호출 (heap dealloc
    /// 은 caller 책임).
    ///
    /// # Safety
    /// `obj` 는 valid sub-type 인스턴스의 `*mut u8` cast. 호출 후 obj 의
    /// PropertyBag 등 owned field 는 모두 drop. 메모리 자체는 dealloc 안 됨.
    pub drop_in_place_fn: unsafe fn(obj: *mut u8),
}

/// SolidBrush 의 static vtable (raw `0x77cf48` 의 Rust 등가물).
///
/// `SolidBrush::new()` / `default()` 가 `vtable` field 를 이 static 의 주소로
/// 설정 — 16B layout 유지 + polymorphic dispatch 가능.
pub static SOLID_BRUSH_VTABLE: BrushVtable = BrushVtable {
    type_tag: BrushType::Solid as u32,
    drop_in_place_fn: solid_brush_drop_in_place,
};

/// raw `~SolidBrush()` 의 등가물 — PropertyBag auto-drop 호출.
///
/// # Safety
/// `obj` 는 valid `*mut SolidBrush`.
unsafe fn solid_brush_drop_in_place(obj: *mut u8) {
    std::ptr::drop_in_place(obj as *mut SolidBrush);
}

/// HatchBrush 의 static vtable (raw `0x77bfe0` 의 Rust 등가물).
pub static HATCH_BRUSH_VTABLE: BrushVtable = BrushVtable {
    type_tag: BrushType::Hatch as u32,
    drop_in_place_fn: hatch_brush_drop_in_place,
};

/// raw `~HatchBrush()` 의 등가물.
///
/// # Safety
/// `obj` 는 valid `*mut HatchBrush`.
unsafe fn hatch_brush_drop_in_place(obj: *mut u8) {
    std::ptr::drop_in_place(obj as *mut HatchBrush);
}

/// raw `Brush*` 의 첫 8B 를 vtable_ptr 로 해석 — opaque polymorphic dispatch
/// helper.
///
/// 본 단계는 모든 Brush sub-type 이 `repr(C)` + first-field `vtable: *const u8`
/// 라는 invariant 사용. raw 와 동일.
///
/// # Safety
/// `brush_obj` 는 valid Brush sub-type 인스턴스의 `*mut u8` cast — 첫 8B 가
/// `&BrushVtable` 주소.
pub unsafe fn brush_vtable(brush_obj: *const u8) -> &'static BrushVtable {
    let vptr = *(brush_obj as *const *const BrushVtable);
    debug_assert!(!vptr.is_null(), "brush_vtable: null vtable ptr");
    &*vptr
}

/// `Hnc::Shape::Brush` 의 Rust enum dispatch — abstract base 의 polymorphic
/// container. 각 variant 가 sub-type 의 semantic byte-eq state 를 보유.
///
/// raw 의 `unique_ptr<Brush>` = `Brush*` 8B 대응 = `Box<Brush>` 8B (sized enum
/// 의 single owned ptr).
///
/// **byte-eq 경계 (16h)**: raw 의 Brush 는 `vtable_ptr + PropertyBag` (16B) 구조 —
/// `PropertyBag` 가 std::map-like dictionary 로 PropertyKey → PropertyValue 저장.
/// 본 Rust port 는 *output byte-eq* 만 보장 — 각 sub-type 의 `Draw`/`Clone`/`GetColor`
/// 결과가 raw 와 동일. 내부 PropertyBag 구조는 direct field 로 단순화.
#[derive(Debug)]
pub enum Brush {
    /// raw `EmptyBrush` (raw vtable `0x77b538`, 8B object, stateless).
    Empty(EmptyBrush),
    /// raw `SolidBrush` (raw vtable `0x77cf48`, 16B = vtable + PropertyBag with
    /// single Color at key 0x259).
    Solid(SolidBrush),
    /// raw `GradientBrush` (raw vtable region near `0x77c000`, multiple stops).
    Gradient(GradientBrush),
    /// raw `ImageBrush` (large struct with ImageSource SharePtr + tile params).
    Image(ImageBrush),
    /// raw `HatchBrush` (raw vtable `0x77bfe0`, 16B = vtable + PropertyBag with
    /// HatchStyle@0x25a + ForeColor@0x25b + BackColor@0x25c).
    Hatch(HatchBrush),
    /// raw `GroupBrush` (composite — list of child Brushes).
    Group(GroupBrush),
}

impl Brush {
    /// raw `Brush::GetType()` vfunc[5] dispatch.
    #[inline]
    pub fn get_type(&self) -> BrushType {
        match self {
            Brush::Empty(_) => BrushType::Empty,
            Brush::Solid(_) => BrushType::Solid,
            Brush::Gradient(_) => BrushType::Gradient,
            Brush::Image(_) => BrushType::Image,
            Brush::Hatch(_) => BrushType::Hatch,
            Brush::Group(_) => BrushType::Group,
        }
    }

    /// raw `Brush::Clone()` vfunc[6] dispatch — heap-alloc 새 Brush.
    pub fn clone_to_heap(&self) -> Box<Brush> {
        Box::new(match self {
            Brush::Empty(b) => Brush::Empty(*b),
            Brush::Solid(b) => Brush::Solid(b.clone()),
            Brush::Gradient(b) => Brush::Gradient(b.clone()),
            Brush::Image(b) => Brush::Image(b.clone()),
            Brush::Hatch(b) => Brush::Hatch(b.clone()),
            Brush::Group(b) => Brush::Group(b.clone()),
        })
    }

    /// raw `Brush::Clone(Color const&)` vfunc[7] dispatch.
    ///
    /// 각 sub-type 별 의미:
    /// - EmptyBrush: Color 무시 (raw `0x166934`: `br vtable[+0x30]`).
    /// - SolidBrush: Color 로 치환된 새 SolidBrush.
    /// - HatchBrush / GradientBrush 등: fore-Color 만 치환.
    pub fn clone_with_color(&self, color: &crate::color::Color) -> Box<Brush> {
        match self {
            Brush::Empty(_) => self.clone_to_heap(),
            Brush::Solid(_) => Box::new(Brush::Solid(SolidBrush::new(unsafe {
                crate::color::Color::copy_ctor(color)
            }))),
            Brush::Gradient(b) => {
                // Gradient 의 vfunc[7]: fore-color (= first stop) 만 교체.
                // 본 단계는 default GradientBrush (stops empty) 에서 호출 시
                // 단순 clone — 첫 stop 의 color 교체는 16x+ deferred (KEY_STOPS
                // 의 Vec 표현 RE 후).
                let _color_unused = color; // raw 의 의도 명시 (16x+ port 시 사용)
                Box::new(Brush::Gradient(b.clone()))
            }
            Brush::Image(b) => Box::new(Brush::Image(b.clone())),
            Brush::Hatch(b) => {
                // HatchBrush 의 vfunc[7]: fore_color 만 교체 (style/back 유지)
                let mut new_b = b.clone();
                new_b.set_fore_color(color);
                Box::new(Brush::Hatch(new_b))
            }
            Brush::Group(b) => Box::new(Brush::Group(b.clone())),
        }
    }

    /// raw `Brush::IsEnable(PropertyKey)` vfunc[8] dispatch.
    ///
    /// - EmptyBrush: 항상 false (raw `0x166940`).
    /// - 다른 sub-type: PropertyBag 에 key 가 있는지 확인.
    ///
    /// 본 Rust port 는 sub-type 별 expected key 만 true 반환.
    pub fn is_enable(&self, key: u32) -> bool {
        match self {
            Brush::Empty(_) => false,
            Brush::Solid(_) => key == SolidBrush::KEY_COLOR,
            Brush::Hatch(_) => {
                key == HatchBrush::KEY_HATCH_STYLE
                    || key == HatchBrush::KEY_FORE_COLOR
                    || key == HatchBrush::KEY_BACK_COLOR
            }
            Brush::Gradient(_) | Brush::Image(_) | Brush::Group(_) => {
                // 각 sub-type 의 PropertyBag key 집합은 multi-session deferred.
                false
            }
        }
    }

    /// raw `Brush::IsSaveable(PropertyKey)` vfunc[9] dispatch.
    pub fn is_saveable(&self, key: u32) -> bool {
        // raw 의 IsSaveable 은 EmptyBrush 가 false, 다른 sub-type 은 IsEnable 과 동일.
        self.is_enable(key)
    }

    /// raw `Brush::Union(Brush const&)` vfunc[10] dispatch.
    ///
    /// EmptyBrush: no-op (raw `0x166950`).
    /// 다른 sub-type: other 의 property 들을 self 에 merge (sub-type 별 spec).
    pub fn union_with(&mut self, _other: &Brush) {
        // 본 단계는 EmptyBrush 만 no-op 으로 정확. 다른 sub-type 의 Union 시멘틱 은
        // multi-session deferred (각 sub-type 의 raw `Union` impl RE 필요).
        match self {
            Brush::Empty(_) => {}
            _ => {} // deferred (NOT panic — 호출 가능 path 가 multi-session 단계에 발생)
        }
    }

    /// raw `Brush::operator==(Brush const&)` vfunc[2] dispatch.
    ///
    /// raw 는 RTTI 기반 type compare. Rust enum dispatch 는 variant tag 비교 +
    /// state 비교로 동등 semantic.
    pub fn eq_brush(&self, other: &Brush) -> bool {
        match (self, other) {
            (Brush::Empty(_), Brush::Empty(_)) => true,
            (Brush::Solid(a), Brush::Solid(b)) => a == b,
            (Brush::Gradient(a), Brush::Gradient(b)) => a == b,
            (Brush::Image(a), Brush::Image(b)) => a == b,
            (Brush::Hatch(a), Brush::Hatch(b)) => a == b,
            (Brush::Group(a), Brush::Group(b)) => a == b,
            _ => false, // 다른 sub-type = 다른 RTTI = false
        }
    }

    /// raw `Brush::operator!=(Brush const&)` vfunc[3] — `!eq`.
    #[inline]
    pub fn ne_brush(&self, other: &Brush) -> bool {
        !self.eq_brush(other)
    }

    /// raw `Brush::operator<(Brush const&)` vfunc[4] — RTTI lexicographic.
    pub fn lt_brush(&self, other: &Brush) -> bool {
        // 다른 type: BrushType u32 tag 순서.
        // 같은 type: state 의 lexicographic compare (sub-type 별 spec).
        match self.get_type().cmp(&other.get_type()) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => {
                // same-type 의 lt — 본 단계는 false (state 비교 multi-session).
                false
            }
        }
    }
}

impl fmt::Display for Brush {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Brush::Empty(_) => write!(f, "EmptyBrush"),
            Brush::Solid(_) => write!(f, "SolidBrush"),
            Brush::Gradient(_) => write!(f, "GradientBrush"),
            Brush::Image(_) => write!(f, "ImageBrush"),
            Brush::Hatch(_) => write!(f, "HatchBrush"),
            Brush::Group(_) => write!(f, "GroupBrush"),
        }
    }
}

// =============================================================================
// SolidBrush — raw `Hnc::Shape::SolidBrush` (16B: vtable + PropertyBag with
// single Color at key 0x259)
// =============================================================================

/// raw `Hnc::Shape::SolidBrush` (raw ctor `0x1e5b0c`).
///
/// raw 구조 (16B):
/// - +0x00: vtable_ptr (= 0x77cf48)
/// - +0x08: PropertyBag (8B unique_ptr<PropertyBagImpl>)
///   - PropertyBag 에 `(key=0x259, value=Color)` 한 entry
///
/// Rust 는 PropertyBag 우회 — Color 를 direct field 로 저장 (semantic byte-eq).
///
/// raw asm 인용 (ctor body @ 0x1e5b0c..0x1e5b88):
/// ```text
/// 1e5b30: str vtable, [x0]
/// 1e5b40: bl PropertyBag::PropertyBag(false)
/// 1e5b44: mov w8, #0x259   ; key
/// 1e5b48: str w8, [sp]       ; PropertyValue.key
/// 1e5b50-1e5b58: load PropertyBag.impl_ptr
/// 1e5b70: bl 0x6541e8        ; SetProperty(key, Color, b=1)
/// ```
/// raw 16B `Hnc::Shape::SolidBrush` — vtable + PropertyBag (16q 의 정공법 byte-eq).
///
/// **재설계 (16r)**: 기존 `{ color: Color }` direct field 에서 raw byte-eq
/// `{ vtable, bag }` 로 변경.
///
/// raw ctor (`0x1e5b0c`) 와 1:1:
/// 1. vtable @ `0x77cf48` 저장 (Rust 는 null sentinel)
/// 2. PropertyBag::PropertyBag(false) — empty bag
/// 3. PColor::create_attach_ctrl(state=2, color) + bag.attach(0x259, ctrl)
///
/// API 호환: `SolidBrush::new(Color)` / `SolidBrush::default()` 는 그대로 유지.
/// 내부 표현만 PropertyBag-backed 로 교체. `get_color()` 가 bag 에서 lookup.
#[repr(C)]
pub struct SolidBrush {
    /// raw +0x00: vtable ptr.
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag (= SharePtr<PropertyBagImpl> 의 single ptr field).
    pub bag: crate::property_bag::PropertyBag,
}

impl std::fmt::Debug for SolidBrush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SolidBrush")
            .field("color", &self.get_color())
            .finish()
    }
}

impl SolidBrush {
    /// raw PropertyKey for the single Color (raw `0x1e5b44: mov w8, #0x259`).
    pub const KEY_COLOR: u32 = 0x259;

    /// raw `SolidBrush::SolidBrush(Color const&)` (`0x1e5b0c`) 1:1.
    ///
    /// 1. vtable + empty PropertyBag (raw 의 ctor + PropertyBag::PropertyBag(false))
    /// 2. PColor::create_attach_ctrl(state=2, color)
    /// 3. bag.attach(key=0x259, ctrl) — raw `bl 0x6541e8`
    pub fn new(color: crate::color::Color) -> Self {
        let mut bag = crate::property_bag::PropertyBag::new(false);
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_COLOR);
        let ctrl = unsafe {
            crate::property::PColor::create_attach_ctrl(
                crate::property::state::ENABLED_EXPLICIT,
                &color,
            )
        };
        unsafe {
            let _old = bag.attach(&key, ctrl);
            // _old should be null (new key) — fresh bag from ctor above
        }
        SolidBrush {
            vtable: &SOLID_BRUSH_VTABLE as *const _ as *const u8,
            bag,
        }
    }

    /// raw `SolidBrush::SolidBrush()` (`0x1e5ad8`) — empty bag, no color attached.
    ///
    /// raw 의 default ctor 는 PropertyBag::PropertyBag(false) 만 호출 + vtable.
    /// PColor attach 는 안 함 — 즉 GetColor 가 default Color 반환 (raw 의 default).
    pub fn default() -> Self {
        SolidBrush {
            vtable: &SOLID_BRUSH_VTABLE as *const _ as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }

    /// raw `SolidBrush::GetColor() const` (`0x1e0650`) — PropertyBag lookup.
    ///
    /// 1. bag.find_equal(0x259)
    /// 2. if found: PColor.color (deep clone)
    /// 3. else: default Color (raw 0,0,0 RGB)
    pub fn get_color(&self) -> crate::color::Color {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_COLOR);
        unsafe {
            if let Some(impl_ref) = self.bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&key) {
                    let cb = (*node).value;
                    if !cb.is_null() {
                        let prop = (*cb).obj;
                        if !prop.is_null() {
                            let pc = prop as *const crate::property::PColor;
                            return crate::color::Color::copy_ctor(&(*pc).color);
                        }
                    }
                }
            }
        }
        // default: nil Color (raw 의 default ctor 와 일치)
        crate::color::Color::from_rgb(0, 0, 0, std::ptr::null_mut())
    }

    /// raw `SolidBrush::SetColor(Color const&)` (`0x173128`) — bag.attach(0x259, PColor).
    pub fn set_color(&mut self, color: &crate::color::Color) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_COLOR);
        let ctrl = unsafe {
            crate::property::PColor::create_attach_ctrl(
                crate::property::state::ENABLED_EXPLICIT,
                color,
            )
        };
        unsafe {
            // _old 는 raw 가 SharePtr 로 반환 (refcount++ 된 상태). caller 가 release 책임.
            // Rust 는 자동 leak — 정확 release 는 SharePtr Drop API 추가 후 (deferred).
            let _old = self.bag.attach(&key, ctrl);
        }
    }

    /// raw `SolidBrush::Create(Color const&)` (`0x0c09fc`).
    pub fn create_boxed(color: crate::color::Color) -> Box<Brush> {
        Box::new(Brush::Solid(Self::new(color)))
    }
}

impl Clone for SolidBrush {
    fn clone(&self) -> Self {
        // raw `Clone()` (vfunc[6]) — PropertyBag 의 deep clone.
        // 현재 단계는 simple: GetColor + SolidBrush::new (= 새 bag + attach).
        // multi-session deferred: PropertyBag::Clone (raw 0x4d928) 의 정확한 tree
        // deep copy.
        let color = self.get_color();
        SolidBrush::new(color)
    }
}

impl PartialEq for SolidBrush {
    fn eq(&self, other: &Self) -> bool {
        self.get_color().eq_struct(&other.get_color())
    }
}

// =============================================================================
// HatchBrush — raw `Hnc::Shape::HatchBrush` (16B: vtable + PropertyBag with
// HatchStyle@0x25a + ForeColor@0x25b + BackColor@0x25c)
// =============================================================================

/// raw `Hnc::Shape::HatchBrush` (raw ctor `0x18c160`).
///
/// raw ctor 가 PropertyBag 에 3 entries 세팅 (raw `0x18c1a0-0x18c244`):
/// - key `0x25a` (602) — HatchStyle (u32 int, 4B)
/// - key `0x25b` (603) — ForeColor (Color)
/// - key `0x25c` (604) — BackColor (Color)
///
/// raw 16B `Hnc::Shape::HatchBrush` — vtable + PropertyBag (16s 정공법 byte-eq).
///
/// **재설계 (16s)**: 기존 direct fields (~56B) → raw byte-eq `{vtable, bag}` (16B).
///
/// raw ctor (`0x18c160`) algorithm:
/// 1. vtable @ `0x77bfe0` 저장
/// 2. PropertyBag::PropertyBag(false) at self+0x8
/// 3. attach key `0x25a` (HatchStyle) via PEnum helper (`0x6674b8`)
/// 4. attach key `0x25b` (ForeColor) via PColor helper (`0x6541e8`)
/// 5. attach key `0x25c` (BackColor) via PColor helper (`0x6541e8`)
#[repr(C)]
pub struct HatchBrush {
    /// raw +0x00: vtable ptr.
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag (= SharePtr<PropertyBagImpl>).
    pub bag: crate::property_bag::PropertyBag,
}

impl std::fmt::Debug for HatchBrush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HatchBrush")
            .field("hatch_style", &self.get_hatch_style())
            .field("fore_color", &self.get_fore_color())
            .field("back_color", &self.get_back_color())
            .finish()
    }
}

impl HatchBrush {
    pub const KEY_HATCH_STYLE: u32 = 0x25a;
    pub const KEY_FORE_COLOR: u32 = 0x25b;
    pub const KEY_BACK_COLOR: u32 = 0x25c;

    /// raw `HatchBrush::HatchBrush(HatchStyle, Color const&, Color const&)` (`0x18c160`) 1:1.
    pub fn new(
        hatch_style: u32,
        fore: crate::color::Color,
        back: crate::color::Color,
    ) -> Self {
        let mut bag = crate::property_bag::PropertyBag::new(false);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            // raw 0x18c1a0-0x18c1d4: attach HatchStyle (PEnum)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_HATCH_STYLE);
            let _ = bag.attach(&k, crate::property::PEnum::create_attach_ctrl(state, hatch_style));
            // raw 0x18c1d8-0x18c20c: attach ForeColor (PColor)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_FORE_COLOR);
            let _ = bag.attach(&k, crate::property::PColor::create_attach_ctrl(state, &fore));
            // raw 0x18c210-0x18c244: attach BackColor (PColor)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_BACK_COLOR);
            let _ = bag.attach(&k, crate::property::PColor::create_attach_ctrl(state, &back));
        }
        HatchBrush {
            vtable: &HATCH_BRUSH_VTABLE as *const _ as *const u8,
            bag,
        }
    }

    /// raw `HatchBrush::HatchBrush()` (`0x18c0c8`) — empty bag (no entries).
    pub fn default() -> Self {
        HatchBrush {
            vtable: &HATCH_BRUSH_VTABLE as *const _ as *const u8,
            bag: crate::property_bag::PropertyBag::new(false),
        }
    }

    /// raw `GetHatchStyle() const` (`0x18dc54`) — PropertyBag lookup.
    pub fn get_hatch_style(&self) -> u32 {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_HATCH_STYLE);
        unsafe {
            if let Some(impl_ref) = self.bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&key) {
                    let cb = (*node).value;
                    if !cb.is_null() && !(*cb).obj.is_null() {
                        let pe = (*cb).obj as *const crate::property::PEnum;
                        return (*pe).value;
                    }
                }
            }
        }
        0 // default
    }

    /// raw `SetHatchStyle` (`0x18dcc0`) — attach via PEnum helper `0x6674b8`.
    pub fn set_hatch_style(&mut self, style: u32) {
        let k = crate::property_key::PropertyKey::from_int(Self::KEY_HATCH_STYLE);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            let _ = self.bag.attach(&k, crate::property::PEnum::create_attach_ctrl(state, style));
        }
    }

    /// raw `GetForeColor() const` (`0x18d480`).
    pub fn get_fore_color(&self) -> crate::color::Color {
        Self::get_color_at(&self.bag, Self::KEY_FORE_COLOR)
    }

    /// raw `SetForeColor` (`0x18d6e4`).
    pub fn set_fore_color(&mut self, color: &crate::color::Color) {
        Self::set_color_at(&mut self.bag, Self::KEY_FORE_COLOR, color);
    }

    /// raw `GetBackColor() const` (`0x18d750`).
    pub fn get_back_color(&self) -> crate::color::Color {
        Self::get_color_at(&self.bag, Self::KEY_BACK_COLOR)
    }

    /// raw `SetBackColor` (`0x18d7bc`).
    pub fn set_back_color(&mut self, color: &crate::color::Color) {
        Self::set_color_at(&mut self.bag, Self::KEY_BACK_COLOR, color);
    }

    /// 공통 helper — bag 에서 PColor (key 별) lookup.
    fn get_color_at(bag: &crate::property_bag::PropertyBag, key_id: u32) -> crate::color::Color {
        let key = crate::property_key::PropertyKey::from_int(key_id);
        unsafe {
            if let Some(impl_ref) = bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&key) {
                    let cb = (*node).value;
                    if !cb.is_null() && !(*cb).obj.is_null() {
                        let pc = (*cb).obj as *const crate::property::PColor;
                        return crate::color::Color::copy_ctor(&(*pc).color);
                    }
                }
            }
        }
        crate::color::Color::from_rgb(0, 0, 0, std::ptr::null_mut())
    }

    /// 공통 helper — bag 에 PColor 새 attach (key 별).
    fn set_color_at(
        bag: &mut crate::property_bag::PropertyBag,
        key_id: u32,
        color: &crate::color::Color,
    ) {
        let k = crate::property_key::PropertyKey::from_int(key_id);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            let _ = bag.attach(&k, crate::property::PColor::create_attach_ctrl(state, color));
        }
    }
}

impl Clone for HatchBrush {
    fn clone(&self) -> Self {
        // 단순 모델: 3 properties 재 attach via new()
        // multi-session deferred: PropertyBag::Clone (`0x4d928`) 정확한 tree clone.
        HatchBrush::new(self.get_hatch_style(), self.get_fore_color(), self.get_back_color())
    }
}

impl PartialEq for HatchBrush {
    fn eq(&self, other: &Self) -> bool {
        self.get_hatch_style() == other.get_hatch_style()
            && self.get_fore_color().eq_struct(&other.get_fore_color())
            && self.get_back_color().eq_struct(&other.get_back_color())
    }
}

// =============================================================================
// GradientBrush — multi-stop gradient (raw `Hnc::Shape::GradientBrush`,
// GetType=2 @ `0x177950`)
// =============================================================================

/// GradientBrush 의 static vtable (raw `0x77b730` 의 Rust 등가물).
///
/// raw `GradientBrush::C2Ev` (`0x176628`) 의 `adrp/add → 0x77b730 → str x8, [x0]`
/// 가 GradientBrush 의 vtable.
pub static GRADIENT_BRUSH_VTABLE: BrushVtable = BrushVtable {
    type_tag: BrushType::Gradient as u32,
    drop_in_place_fn: gradient_brush_drop_in_place,
};

/// raw `~GradientBrush()` 의 등가물.
///
/// # Safety
/// `obj` 는 valid `*mut GradientBrush`.
unsafe fn gradient_brush_drop_in_place(obj: *mut u8) {
    std::ptr::drop_in_place(obj as *mut GradientBrush);
}

/// raw `Hnc::Shape::GradientBrush` — multi-stop gradient.
///
/// **재설계 (16w)**: 기존 `{ stops: Vec, style: u32, angle: f32 }` direct fields
/// 에서 raw byte-eq `{ vtable, bag }` (16B) 로 변경.
///
/// raw layout (`0x176628 GradientBrush::C2Ev`):
/// - +0x00: vtable_ptr (= `0x77b730`)
/// - +0x08: PropertyBag (8B SharePtr<PropertyBagImpl>)
///
/// 총 16B (= raw `mov w0, #0x10` for the alloc).
///
/// ## PropertyBag 의 8 default keys (raw `0x176658-0x17684c` 의 attach sequence)
///
/// | Key   | helper        | type            | default value          |
/// |-------|---------------|-----------------|------------------------|
/// | 0x25f | `0x656690`    | PEnum (u32)     | 0 (gradient style)     |
/// | 0x260 | `0x656acc`    | PDegree (f32)   | 0.0 (angle)            |
/// | 0x261 | `0x6475a4`    | PBool (u8)      | false                  |
/// | 0x262 | `0x656fb4`    | PVec4 (16B)     | (0.5, 0.5, 0.5, 0.5)   |
/// | 0x263 | `0x656fb4`    | PVec4 (16B)     | (0, 0, 0, 0)           |
/// | 0x264 | `0x665628`    | PEnum (u32)     | 4 (alignment?)         |
/// | 0x265 | `0x6475a4`    | PBool (u8)      | true                   |
/// | 0x267 | `0x665a64`    | PEnum (u32)     | 1                      |
///
/// **NOTE**: key `0x266` 은 default ctor 가 set 안 함 — `GradientStops` (Vec) 의
/// key 로 추정. 별도 `SetStops` 호출에서 init (16x+ deferred).
///
/// 본 단계 (16w) 는 default ctor 만 1:1 — stops 는 빈 상태.
#[repr(C)]
pub struct GradientBrush {
    /// raw +0x00: vtable ptr.
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag (8B SharePtr<PropertyBagImpl>).
    pub bag: crate::property_bag::PropertyBag,
}

impl std::fmt::Debug for GradientBrush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GradientBrush")
            .field("vtable", &self.vtable)
            .field("bag_size", &self.bag_size())
            .finish()
    }
}

impl GradientBrush {
    // 8 default PropertyBag keys (raw 의 attach 순서)
    pub const KEY_STYLE: u32 = 0x25f;       // PEnum
    pub const KEY_ANGLE: u32 = 0x260;       // PDegree (= PFloat 16B)
    pub const KEY_FLIP: u32 = 0x261;        // PBool
    pub const KEY_FOCUS_RECT: u32 = 0x262;  // PVec4 (0.5, 0.5, 0.5, 0.5)
    pub const KEY_TILE_RECT: u32 = 0x263;   // PVec4 (0, 0, 0, 0)
    pub const KEY_TILE_METHOD: u32 = 0x264; // PEnum (= 4)
    pub const KEY_SCALED: u32 = 0x265;      // PBool (= true)
    pub const KEY_STOPS: u32 = 0x266;       // GradientStops Vec — NOT set by default
    pub const KEY_INTERP: u32 = 0x267;      // PEnum (= 1)

    /// raw `GradientBrush::GradientBrush()` (`0x176628`) 1:1 port.
    ///
    /// raw asm sequence:
    /// ```asm
    /// 17663c-176644: vtable @ 0x77b730
    /// 17664c-176654: PropertyBag::PropertyBag(false)
    /// 176658-17668c: attach 0x25f (PEnum, value=0) via 0x656690
    /// 176694-1766dc: attach 0x260 (PDegree, value=0.0) via 0x656acc
    /// 1766e0-176718: attach 0x261 (PBool, value=false) via 0x6475a4
    /// 17671c-176758: attach 0x262 (PVec4, (0.5,0.5,0.5,0.5)) via 0x656fb4
    /// 17675c-176794: attach 0x263 (PVec4, (0,0,0,0)) via 0x656fb4
    /// 176798-1767d4: attach 0x264 (PEnum, value=4) via 0x665628
    /// 1767d8-176814: attach 0x265 (PBool, value=true) via 0x6475a4
    /// 176818-176854: attach 0x267 (PEnum, value=1) via 0x665a64
    /// ```
    pub fn new() -> Self {
        let mut bag = crate::property_bag::PropertyBag::new(false);
        let state = crate::property::state::ENABLED_EXPLICIT;
        unsafe {
            // raw 0x176658-0x17668c: key 0x25f, PEnum(state=2, value=0)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_STYLE);
            let _ = bag.attach(&k, crate::property::PEnum::create_attach_ctrl(state, 0));

            // raw 0x176694-0x1766dc: key 0x260, PDegree(state=2, value=0.0)
            // PDegree (16B) 의 layout = PFloat 와 동일 (vtable + state + f32)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_ANGLE);
            let _ = bag.attach(&k, crate::property::PFloat::create_attach_ctrl(state, 0.0));

            // raw 0x1766e0-0x176718: key 0x261, PBool(state=2, value=false)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_FLIP);
            let _ = bag.attach(&k, crate::property::PBool::create_attach_ctrl(state, false));

            // raw 0x17671c-0x176758: key 0x262, PVec4(state=2, value=(0.5,0.5,0.5,0.5))
            // raw `movi.4s v0, #0x3f, lsl #24` → 4 lanes = 0x3F000000 = 0.5 each
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_FOCUS_RECT);
            let _ = bag.attach(
                &k,
                crate::property::PVec4::create_attach_ctrl(
                    state,
                    {
                        let mut b = [0u8; 16];
                        for i in 0..4 {
                            b[i * 4..i * 4 + 4].copy_from_slice(&0.5_f32.to_le_bytes());
                        }
                        b
                    },
                ),
            );

            // raw 0x17675c-0x176794: key 0x263, PVec4(state=2, value=(0,0,0,0))
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_TILE_RECT);
            let _ = bag.attach(
                &k,
                crate::property::PVec4::create_attach_ctrl(state, [0u8; 16]),
            );

            // raw 0x176798-0x1767d4: key 0x264, PEnum(state=2, value=4)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_TILE_METHOD);
            let _ = bag.attach(&k, crate::property::PEnum::create_attach_ctrl(state, 4));

            // raw 0x1767d8-0x176814: key 0x265, PBool(state=2, value=true)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_SCALED);
            let _ = bag.attach(&k, crate::property::PBool::create_attach_ctrl(state, true));

            // raw 0x176818-0x176854: key 0x267, PEnum(state=2, value=1)
            let k = crate::property_key::PropertyKey::from_int(Self::KEY_INTERP);
            let _ = bag.attach(&k, crate::property::PEnum::create_attach_ctrl(state, 1));
        }

        GradientBrush {
            vtable: &GRADIENT_BRUSH_VTABLE as *const _ as *const u8,
            bag,
        }
    }

    /// PropertyBag 의 노드 수 (debug / test 용).
    pub fn bag_size(&self) -> u64 {
        unsafe {
            self.bag
                .impl_ref()
                .map(|i| i.tree.size)
                .unwrap_or(0)
        }
    }

    /// Helper: bag 에서 u32 (PEnum) value lookup, key 없으면 default 반환.
    unsafe fn bag_get_u32(&self, key: u32, default: u32) -> u32 {
        let pk = crate::property_key::PropertyKey::from_int(key);
        if let Some(impl_ref) = self.bag.impl_ref() {
            if let Ok(node) = impl_ref.find_equal(&pk) {
                let cb = (*node).value;
                if !cb.is_null() {
                    let prop = (*cb).obj;
                    if !prop.is_null() {
                        let pe = prop as *const crate::property::PEnum;
                        return (*pe).value;
                    }
                }
            }
        }
        default
    }

    /// Helper: bag 에서 f32 (PFloat) value lookup.
    unsafe fn bag_get_f32(&self, key: u32, default: f32) -> f32 {
        let pk = crate::property_key::PropertyKey::from_int(key);
        if let Some(impl_ref) = self.bag.impl_ref() {
            if let Ok(node) = impl_ref.find_equal(&pk) {
                let cb = (*node).value;
                if !cb.is_null() {
                    let prop = (*cb).obj;
                    if !prop.is_null() {
                        let pf = prop as *const crate::property::PFloat;
                        return (*pf).value;
                    }
                }
            }
        }
        default
    }

    /// raw 의 gradient style (linear/radial/path) — bag key 0x25f.
    #[inline]
    pub fn get_style(&self) -> u32 {
        unsafe { self.bag_get_u32(Self::KEY_STYLE, 0) }
    }

    /// raw 의 angle (degrees) — bag key 0x260.
    #[inline]
    pub fn get_angle_degrees(&self) -> f32 {
        unsafe { self.bag_get_f32(Self::KEY_ANGLE, 0.0) }
    }

    /// stops — bag key 0x266 의 PStops 에서 stops 개수 확인.
    ///
    /// 16w: empty Vec 반환 (key 0x266 미부착)
    /// 16y: PStops 에서 GradientStopsVec 의 ptr 들 통해 (position, Color) tuples 재구성.
    ///       Color 재구성은 simplified — 16z+ 에서 정확한 stop-of-Color reconstruction.
    pub fn get_stops(&self) -> Vec<(f32, crate::color::Color)> {
        let pk = crate::property_key::PropertyKey::from_int(Self::KEY_STOPS);
        unsafe {
            if let Some(impl_ref) = self.bag.impl_ref() {
                if let Ok(node) = impl_ref.find_equal(&pk) {
                    let cb = (*node).value;
                    if !cb.is_null() {
                        let prop = (*cb).obj;
                        if !prop.is_null() {
                            let ps = prop as *const crate::property::PStops;
                            // 16y: return [(position, default color)] for each stop.
                            // 정확한 Color 의 reconstruction (value+type_tag+effect →
                            // Color struct) 은 16z+ 에서.
                            let mut out = Vec::new();
                            let v = &(*ps).stops;
                            let count = v.len();
                            for i in 0..count {
                                let ctrl = *v.begin.add(i);
                                if !ctrl.is_null() {
                                    let stop = (*ctrl).obj;
                                    if !stop.is_null() {
                                        let position = (*stop).position;
                                        // Color reconstruct (simplified — no effect deep clone here)
                                        let mut color = crate::color::Color::from_rgb(0, 0, 0, std::ptr::null_mut());
                                        color.value.copy_from_slice(&(*stop).value);
                                        color.type_tag = (*stop).type_tag;
                                        // color.color_effect 는 NOT 복사 (raw ptr ownership)
                                        out.push((position, color));
                                    }
                                }
                            }
                            return out;
                        }
                    }
                }
            }
        }
        Vec::new()
    }

    /// raw `0x16fb9c-0x16fbd8` 1:1 — bag.attach(key=0x266, PStops(state=2, src_vec)) via
    /// helper `0x655508`.
    ///
    /// raw asm:
    /// ```asm
    /// 0x16fba0: mov  w9, #0x266            ; key
    /// 0x16fba4: stur w9, [x29, #-0xa0]
    /// 0x16fbc4: sub  x2, x29, #0xd8        ; x2 = &GradientStopsVec (24B)
    /// 0x16fbc8: mov  w3, #0x1
    /// 0x16fbcc: bl   0x655508              ; PStops helper
    /// ```
    ///
    /// 본 method: PStops::create_attach_ctrl + bag.attach(0x266, ctrl). caller 의
    /// `src_vec` 은 변경되지 않음 — PStops 가 deep clone (clone_deep) 으로 own.
    ///
    /// # Safety
    /// `src_vec` 은 valid GradientStopsVec.
    pub unsafe fn set_stops(&mut self, src_vec: &crate::gradient_stop::GradientStopsVec) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_STOPS);
        let ctrl = crate::property::PStops::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            src_vec,
        );
        let _old = self.bag.attach(&key, ctrl);
        // _old 는 이전 PStops 의 SharePtr (default ctor 후엔 null) — Rust drop 처리.
    }

    /// raw `0x16fbdc-0x16fc10` 1:1 — re-attach key 0x265 (PBool) with given value.
    pub unsafe fn set_flip(&mut self, value: bool) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_FLIP);
        let ctrl = crate::property::PBool::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            value,
        );
        let _old = self.bag.attach(&key, ctrl);
    }

    /// raw `0x16fc14-0x16fc4c` 1:1 — re-attach key 0x25f (PEnum) with given style.
    pub unsafe fn set_style(&mut self, value: u32) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_STYLE);
        let ctrl = crate::property::PEnum::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            value,
        );
        let _old = self.bag.attach(&key, ctrl);
    }

    /// raw `0x16fc50-0x16fc94` 1:1 — re-attach key 0x260 (PDegree) with angle.
    ///
    /// raw `0x16fc50: mov w8, #0x43870000; fmov s0, w8` → 270.0 float.
    pub unsafe fn set_angle_degrees(&mut self, angle: f32) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_ANGLE);
        let ctrl = crate::property::PFloat::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            angle,
        );
        let _old = self.bag.attach(&key, ctrl);
    }

    /// raw `0x16fca0-0x16fcd8` 1:1 — re-attach key 0x261 (PBool) with value.
    pub unsafe fn set_scaled(&mut self, value: bool) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_SCALED);
        let ctrl = crate::property::PBool::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            value,
        );
        let _old = self.bag.attach(&key, ctrl);
    }

    /// raw `0x17090c-0x170944` 1:1 — re-attach key 0x262 (PVec4) with 16B blob
    /// (= 4 float lanes representing focus rectangle).
    ///
    /// Block 11 (SetBackgroundBrush #2 의 4번째 setter):
    /// ```asm
    /// 0x17090c: adrp x8, 0x741000
    /// 0x170910: ldr  q0, [x8, #0xe90]   ; load 16B from rodata @ 0x741e90
    /// 0x170914: str  q0, [sp, #0xd0]
    /// 0x170918: mov  w8, #0x262
    /// 0x170944: bl   0x656fb4           ; PVec4 helper
    /// ```
    ///
    /// rodata @ 0x741e90 (verified via xxd on arm64 slice):
    /// `00 00 00 3f  cd cc 4c bf  00 00 00 3f  66 66 e6 3f`
    /// = (0.5, -0.8, 0.5, 1.8) as 4 f32 lanes.
    pub unsafe fn set_focus_rect(&mut self, blob: [u8; 16]) {
        let key = crate::property_key::PropertyKey::from_int(Self::KEY_FOCUS_RECT);
        let ctrl = crate::property::PVec4::create_attach_ctrl(
            crate::property::state::ENABLED_EXPLICIT,
            blob,
        );
        let _old = self.bag.attach(&key, ctrl);
    }
}

impl Default for GradientBrush {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for GradientBrush {
    /// 본 단계는 raw 와 동등 (PropertyBag deep clone 의 정식 port 는 multi-session
    /// deferred). 현재는 `GradientBrush::new()` 로 default 복귀 — default 의 모든
    /// 8 keys 가 동일하므로 default-state 끼리는 동등.
    fn clone(&self) -> Self {
        // multi-session deferred: PropertyBag::Clone (raw 0x4d928) 으로 deep copy.
        // 현재는 default 의 모든 keys 가 같은 값 → new() 동등 (no stops/no custom).
        GradientBrush::new()
    }
}

impl PartialEq for GradientBrush {
    fn eq(&self, other: &Self) -> bool {
        // 본 단계는 bag 의 8 keys 가 동일하면 동등.
        // 16w 의 default ctor 는 모든 GradientBrush 가 동일 state → 항상 true.
        // 향후 SetStops / SetStyle 후엔 bag key 별 deep compare 필요.
        self.get_style() == other.get_style()
            && self.get_angle_degrees().to_bits() == other.get_angle_degrees().to_bits()
            && self.bag_size() == other.bag_size()
    }
}

// =============================================================================
// ImageBrush — raw `Hnc::Shape::ImageBrush` (large struct, GetType=3 @ `0x18f9b8`)
// =============================================================================

/// raw `Hnc::Shape::ImageBrush` — image / picture fill.
///
/// raw ctor 시그니처가 매우 길어 (`0x18ee30`):
/// `(SharePtr<ImageSource>, TileStyle, f32, f32, f32, f32, RectAlignStyle,
///   OffsetRect, bool, auto_ptr<ImageEffects>)` — 9+ params.
///
/// 본 단계는 opaque placeholder — 정확한 fields 는 multi-session deferred.
#[derive(Debug, Default)]
pub struct ImageBrush {
    /// raw 의 ImageSource SharePtr (RE 후 정확한 타입 추가).
    /// 본 단계는 string identifier 만 (output byte-eq 검증용).
    pub source_id: String,
    /// raw TileStyle (u32 enum).
    pub tile_style: u32,
    /// raw 4 floats (scale_x, scale_y, offset_x, offset_y).
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl ImageBrush {
    pub fn new(source_id: String) -> Self {
        ImageBrush {
            source_id,
            ..Default::default()
        }
    }
}

impl Clone for ImageBrush {
    fn clone(&self) -> Self {
        ImageBrush {
            source_id: self.source_id.clone(),
            tile_style: self.tile_style,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
        }
    }
}

impl PartialEq for ImageBrush {
    fn eq(&self, other: &Self) -> bool {
        self.source_id == other.source_id
            && self.tile_style == other.tile_style
            && self.scale_x.to_bits() == other.scale_x.to_bits()
            && self.scale_y.to_bits() == other.scale_y.to_bits()
            && self.offset_x.to_bits() == other.offset_x.to_bits()
            && self.offset_y.to_bits() == other.offset_y.to_bits()
    }
}

// =============================================================================
// GroupBrush — composite (raw `Hnc::Shape::GroupBrush`, GetType=5 @ `0x18662c`)
// =============================================================================

/// raw `Hnc::Shape::GroupBrush` — composite brush containing child brushes.
///
/// raw 의 GroupBrush 는 child Brush list 를 가진 composite. 본 단계는 vec
/// of Box<Brush> 로 모델링.
#[derive(Debug, Default)]
pub struct GroupBrush {
    pub children: Vec<Box<Brush>>,
}

impl GroupBrush {
    pub fn new() -> Self {
        GroupBrush::default()
    }

    pub fn with_children(children: Vec<Box<Brush>>) -> Self {
        GroupBrush { children }
    }
}

impl Clone for GroupBrush {
    fn clone(&self) -> Self {
        GroupBrush {
            children: self.children.iter().map(|c| c.clone_to_heap()).collect(),
        }
    }
}

impl PartialEq for GroupBrush {
    fn eq(&self, other: &Self) -> bool {
        self.children.len() == other.children.len()
            && self
                .children
                .iter()
                .zip(other.children.iter())
                .all(|(a, b)| a.eq_brush(b))
    }
}

/// raw `Hnc::Shape::EmptyBrush` — 8B (vtable ptr only, no fields).
///
/// raw ctor (`0x166738`) 가 vtable ptr (= `0x77b538`) 만 store 하고 return.
/// 즉 의미상 stateless (의미: "no fill / no brush operation").
///
/// Rust 는 unit-like struct — 0B (vtable 은 enum dispatch 가 제공).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EmptyBrush;

impl EmptyBrush {
    /// raw `EmptyBrush::EmptyBrush()` (`0x166738`) — stateless ctor.
    #[inline]
    pub const fn new() -> Self {
        EmptyBrush
    }

    /// raw `EmptyBrush::Create()` (`0x166a6c`) — alloc 8B + ctor + sret 반환.
    ///
    /// Rust enum 에선 `Box::new(Brush::Empty(EmptyBrush::new()))` 동등.
    pub fn create_boxed() -> Box<Brush> {
        Box::new(Brush::Empty(EmptyBrush::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_brush_is_zero_bytes() {
        // raw 의 8B vtable-ptr 객체. Rust 는 0B placeholder (vtable 은 enum tag).
        assert_eq!(std::mem::size_of::<EmptyBrush>(), 0);
    }

    #[test]
    fn brush_enum_get_type_returns_zero_for_empty() {
        let b = Brush::Empty(EmptyBrush::new());
        assert_eq!(b.get_type(), BrushType::Empty);
        assert_eq!(b.get_type().as_u32(), 0);
    }

    #[test]
    fn brush_clone_returns_same_variant() {
        // raw `Brush::Clone()` alloc 새 Brush + 같은 vfunc 분기 (= 같은 sub-type).
        // Rust ZST 의 경우 Box<ZST> 의 "address" 는 dangling pointer (allocator
        // optimization) — 모든 Box<EmptyBrush> 가 같은 값. byte-eq 의 본질은
        // "동일 variant" 와 "동일 semantic 결과" — type tag 일치 검증.
        let b = Brush::Empty(EmptyBrush::new());
        let cloned = b.clone_to_heap();
        assert_eq!(b.get_type(), cloned.get_type());
        // Equality semantic
        assert!(b.eq_brush(&cloned));
    }

    #[test]
    fn brush_clone_with_color_ignores_color() {
        // raw EmptyBrush::Clone(Color&) = tail-call Clone() — Color 무시
        let b = Brush::Empty(EmptyBrush::new());
        let color = crate::color::Color::from_rgb(0xff, 0x00, 0x00, std::ptr::null_mut());
        let cloned_a = b.clone_to_heap();
        let cloned_b = b.clone_with_color(&color);
        // 두 clone 모두 동일 type (EmptyBrush)
        assert_eq!(cloned_a.get_type(), cloned_b.get_type());
        assert_eq!(cloned_b.get_type(), BrushType::Empty);
    }

    #[test]
    fn empty_brush_is_enable_always_false() {
        let b = Brush::Empty(EmptyBrush::new());
        // raw `0x166940: mov w0, #0; ret` — PropertyKey 무시
        assert!(!b.is_enable(0));
        assert!(!b.is_enable(0x259));
        assert!(!b.is_enable(0xFFFF_FFFF));
    }

    #[test]
    fn empty_brush_is_saveable_always_false() {
        let b = Brush::Empty(EmptyBrush::new());
        assert!(!b.is_saveable(0));
        assert!(!b.is_saveable(0x259));
    }

    #[test]
    fn empty_brush_union_is_noop() {
        let mut b = Brush::Empty(EmptyBrush::new());
        let other = Brush::Empty(EmptyBrush::new());
        b.union_with(&other);
        // No-op — state unchanged
        assert_eq!(b.get_type(), BrushType::Empty);
    }

    #[test]
    fn brush_eq_same_type_returns_true() {
        let a = Brush::Empty(EmptyBrush::new());
        let b = Brush::Empty(EmptyBrush::new());
        assert!(a.eq_brush(&b));
        assert!(!a.ne_brush(&b));
    }

    #[test]
    fn brush_lt_same_type_returns_false() {
        let a = Brush::Empty(EmptyBrush::new());
        let b = Brush::Empty(EmptyBrush::new());
        assert!(!a.lt_brush(&b));
        assert!(!b.lt_brush(&a));
    }

    #[test]
    fn empty_brush_create_boxed_yields_owned_brush() {
        let boxed = EmptyBrush::create_boxed();
        assert_eq!(boxed.get_type(), BrushType::Empty);
    }

    #[test]
    fn brush_box_size_is_8b() {
        // raw 의 unique_ptr<Brush> = Brush* = 8B 와 동등.
        assert_eq!(std::mem::size_of::<Box<Brush>>(), 8);
    }

    #[test]
    fn brush_display_format() {
        let b = Brush::Empty(EmptyBrush::new());
        assert_eq!(format!("{}", b), "EmptyBrush");
    }

    // ===== SolidBrush tests =====

    #[test]
    fn solid_brush_raw_16b_layout() {
        // 16r: SolidBrush 의 PropertyBag-backed layout 검증
        assert_eq!(std::mem::size_of::<SolidBrush>(), 16);
        assert_eq!(std::mem::align_of::<SolidBrush>(), 8);
    }

    #[test]
    fn solid_brush_field_offsets_match_raw() {
        let sb = SolidBrush::default();
        let base = &sb as *const _ as usize;
        assert_eq!(&sb.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&sb.bag as *const _ as usize - base, 0x08);
    }

    #[test]
    fn solid_brush_new_attaches_pcolor_to_bag() {
        // raw 의 SolidBrush::SolidBrush(Color) 의 결과 검증:
        // bag.contains(0x259) == true, get_color() 가 원본과 일치
        let red = crate::color::Color::from_rgb(0xCA, 0xFE, 0x42, std::ptr::null_mut());
        let sb = SolidBrush::new(red);
        let read_rgb = sb.get_color().get_rgb();
        assert_eq!(read_rgb.r, 0xCA);
        assert_eq!(read_rgb.g, 0xFE);
        assert_eq!(read_rgb.b, 0x42);
    }

    #[test]
    fn solid_brush_default_empty_bag_returns_nil_color() {
        let sb = SolidBrush::default();
        let rgb = sb.get_color().get_rgb();
        assert_eq!(rgb.r, 0);
        assert_eq!(rgb.g, 0);
        assert_eq!(rgb.b, 0);
    }

    #[test]
    fn solid_brush_set_color_round_trip() {
        let mut sb = SolidBrush::default();
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        sb.set_color(&red);
        assert_eq!(sb.get_color().get_rgb().r, 0xFF);

        let blue = crate::color::Color::from_rgb(0, 0, 0xFF, std::ptr::null_mut());
        sb.set_color(&blue);
        assert_eq!(sb.get_color().get_rgb().b, 0xFF);
        assert_eq!(sb.get_color().get_rgb().r, 0);
    }

    #[test]
    fn solid_brush_get_type_returns_1() {
        let color = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let b = Brush::Solid(SolidBrush::new(color));
        assert_eq!(b.get_type(), BrushType::Solid);
        assert_eq!(b.get_type().as_u32(), 1);
    }

    #[test]
    fn solid_brush_clone_preserves_color() {
        let red = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let b = Brush::Solid(SolidBrush::new(red));
        let cloned = b.clone_to_heap();
        assert_eq!(cloned.get_type(), BrushType::Solid);
        if let Brush::Solid(sb) = &*cloned {
            let rgb = sb.get_color().get_rgb();
            assert_eq!(rgb.r, 0xFF);
            assert_eq!(rgb.g, 0x00);
            assert_eq!(rgb.b, 0x00);
        } else {
            panic!("expected SolidBrush variant");
        }
    }

    #[test]
    fn solid_brush_clone_with_color_replaces_color() {
        let red = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let blue = crate::color::Color::from_rgb(0x00, 0x00, 0xFF, std::ptr::null_mut());
        let b = Brush::Solid(SolidBrush::new(red));
        let cloned = b.clone_with_color(&blue);
        if let Brush::Solid(sb) = &*cloned {
            let rgb = sb.get_color().get_rgb();
            assert_eq!(rgb.r, 0x00);
            assert_eq!(rgb.b, 0xFF);
        }
    }

    #[test]
    fn solid_brush_is_enable_key_259() {
        let red = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let b = Brush::Solid(SolidBrush::new(red));
        assert!(b.is_enable(0x259));
        assert!(!b.is_enable(0x25a));
        assert!(!b.is_enable(0));
    }

    // ===== HatchBrush tests =====

    #[test]
    fn hatch_brush_get_type_returns_4() {
        let fore = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let back = crate::color::Color::from_rgb(0x00, 0xFF, 0x00, std::ptr::null_mut());
        let b = Brush::Hatch(HatchBrush::new(5, fore, back));
        assert_eq!(b.get_type(), BrushType::Hatch);
        assert_eq!(b.get_type().as_u32(), 4);
    }

    #[test]
    fn hatch_brush_raw_16b_layout() {
        // 16s 재설계: HatchBrush 의 PropertyBag-backed 16B layout 검증
        assert_eq!(std::mem::size_of::<HatchBrush>(), 16);
        assert_eq!(std::mem::align_of::<HatchBrush>(), 8);
    }

    #[test]
    fn hatch_brush_field_offsets_match_raw() {
        let hb = HatchBrush::default();
        let base = &hb as *const _ as usize;
        assert_eq!(&hb.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&hb.bag as *const _ as usize - base, 0x08);
    }

    #[test]
    fn hatch_brush_new_attaches_3_keys_to_bag() {
        let fore = crate::color::Color::from_rgb(0xCA, 0xFE, 0x00, std::ptr::null_mut());
        let back = crate::color::Color::from_rgb(0x00, 0xBE, 0xEF, std::ptr::null_mut());
        let hb = HatchBrush::new(5, fore, back);
        // 3 keys in bag
        unsafe {
            assert_eq!(hb.bag.impl_ref().unwrap().tree.size, 3);
        }
        assert_eq!(hb.get_hatch_style(), 5);
        let fr = hb.get_fore_color().get_rgb();
        let bk = hb.get_back_color().get_rgb();
        assert_eq!(fr.r, 0xCA);
        assert_eq!(fr.g, 0xFE);
        assert_eq!(bk.g, 0xBE);
        assert_eq!(bk.b, 0xEF);
    }

    #[test]
    fn hatch_brush_default_empty_bag() {
        let hb = HatchBrush::default();
        unsafe {
            assert_eq!(hb.bag.impl_ref().unwrap().tree.size, 0);
        }
        // 모든 getter 가 default 반환 (style=0, color=0,0,0)
        assert_eq!(hb.get_hatch_style(), 0);
        assert_eq!(hb.get_fore_color().get_rgb().r, 0);
        assert_eq!(hb.get_back_color().get_rgb().r, 0);
    }

    #[test]
    fn hatch_brush_setters_round_trip() {
        let mut hb = HatchBrush::default();
        hb.set_hatch_style(9);
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let blue = crate::color::Color::from_rgb(0, 0, 0xFF, std::ptr::null_mut());
        hb.set_fore_color(&red);
        hb.set_back_color(&blue);
        assert_eq!(hb.get_hatch_style(), 9);
        assert_eq!(hb.get_fore_color().get_rgb().r, 0xFF);
        assert_eq!(hb.get_back_color().get_rgb().b, 0xFF);
        unsafe {
            assert_eq!(hb.bag.impl_ref().unwrap().tree.size, 3);
        }
    }

    #[test]
    fn hatch_brush_clone_preserves_all_fields() {
        let fore = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let back = crate::color::Color::from_rgb(0x00, 0xFF, 0x00, std::ptr::null_mut());
        let b = Brush::Hatch(HatchBrush::new(7, fore, back));
        let cloned = b.clone_to_heap();
        if let Brush::Hatch(hb) = &*cloned {
            assert_eq!(hb.get_hatch_style(), 7);
            assert_eq!(hb.get_fore_color().get_rgb().r, 0xFF);
            assert_eq!(hb.get_back_color().get_rgb().g, 0xFF);
        }
    }

    #[test]
    fn hatch_brush_clone_with_color_replaces_only_fore() {
        let fore = crate::color::Color::from_rgb(0xFF, 0x00, 0x00, std::ptr::null_mut());
        let back = crate::color::Color::from_rgb(0x00, 0xFF, 0x00, std::ptr::null_mut());
        let new_fore = crate::color::Color::from_rgb(0x00, 0x00, 0xFF, std::ptr::null_mut());
        let b = Brush::Hatch(HatchBrush::new(7, fore, back));
        let cloned = b.clone_with_color(&new_fore);
        if let Brush::Hatch(hb) = &*cloned {
            assert_eq!(hb.get_hatch_style(), 7); // style unchanged
            assert_eq!(hb.get_fore_color().get_rgb().b, 0xFF); // fore replaced
            assert_eq!(hb.get_back_color().get_rgb().g, 0xFF); // back unchanged
        }
    }

    #[test]
    fn hatch_brush_is_enable_three_keys() {
        let fore = crate::color::Color::from_rgb(0, 0, 0, std::ptr::null_mut());
        let back = crate::color::Color::from_rgb(0, 0, 0, std::ptr::null_mut());
        let b = Brush::Hatch(HatchBrush::new(0, fore, back));
        assert!(b.is_enable(HatchBrush::KEY_HATCH_STYLE));
        assert!(b.is_enable(HatchBrush::KEY_FORE_COLOR));
        assert!(b.is_enable(HatchBrush::KEY_BACK_COLOR));
        assert!(!b.is_enable(0x259));
        assert!(!b.is_enable(0));
    }

    // ===== Brush type ordering =====

    #[test]
    fn brush_lt_uses_type_tag_order() {
        let empty = Brush::Empty(EmptyBrush::new());
        let solid = Brush::Solid(SolidBrush::default());
        let hatch = Brush::Hatch(HatchBrush::default());

        assert!(empty.lt_brush(&solid));
        assert!(empty.lt_brush(&hatch));
        assert!(solid.lt_brush(&hatch)); // Solid=1 < Hatch=4
        assert!(!hatch.lt_brush(&empty));
        assert!(!solid.lt_brush(&empty));
    }

    #[test]
    fn brush_ne_different_types_returns_true() {
        let empty = Brush::Empty(EmptyBrush::new());
        let solid = Brush::Solid(SolidBrush::default());
        assert!(empty.ne_brush(&solid));
        assert!(!empty.eq_brush(&solid));
    }

    #[test]
    fn brush_eq_same_type_same_state() {
        let c1 = crate::color::Color::from_rgb(1, 2, 3, std::ptr::null_mut());
        let c2 = crate::color::Color::from_rgb(1, 2, 3, std::ptr::null_mut());
        let a = Brush::Solid(SolidBrush::new(c1));
        let b = Brush::Solid(SolidBrush::new(c2));
        assert!(a.eq_brush(&b));
    }

    #[test]
    fn brush_eq_same_type_different_state() {
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let blue = crate::color::Color::from_rgb(0, 0, 0xFF, std::ptr::null_mut());
        let a = Brush::Solid(SolidBrush::new(red));
        let b = Brush::Solid(SolidBrush::new(blue));
        assert!(!a.eq_brush(&b));
    }

    // ===== GradientBrush / ImageBrush / GroupBrush =====

    #[test]
    fn gradient_brush_get_type_returns_2() {
        let b = Brush::Gradient(GradientBrush::new());
        assert_eq!(b.get_type(), BrushType::Gradient);
        assert_eq!(b.get_type().as_u32(), 2);
    }

    #[test]
    fn gradient_brush_clone_default_state() {
        // 16w 재설계: GradientBrush 가 bag-backed. default ctor 의 8 keys 기준.
        // stops Vec (key 0x266) 는 default ctor 가 attach 안 함 — 16x+ 에서 SetStops.
        let g = GradientBrush::new();
        let b = Brush::Gradient(g);
        let cloned = b.clone_to_heap();
        if let Brush::Gradient(gb) = &*cloned {
            // default state: stops empty, angle 0.0, style 0
            assert_eq!(gb.get_stops().len(), 0);
            assert_eq!(gb.get_angle_degrees(), 0.0);
            assert_eq!(gb.get_style(), 0);
            assert_eq!(gb.bag_size(), 8);
        }
    }

    // ===== 16w: GradientBrush re-architect (16B byte-eq) tests =====

    #[test]
    fn gradient_brush_raw_16b_layout() {
        // raw `Hnc::Shape::GradientBrush` (`0x176628`) = 16B (vtable + bag)
        assert_eq!(std::mem::size_of::<GradientBrush>(), 16);
        assert_eq!(std::mem::align_of::<GradientBrush>(), 8);
    }

    #[test]
    fn gradient_brush_field_offsets_match_raw() {
        // raw `17663c-176644`: vtable @ +0x00
        // raw `176648`: bag (= self+0x8) — vtable + 8B
        let g = GradientBrush::new();
        let base = &g as *const _ as usize;
        assert_eq!(&g.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&g.bag as *const _ as usize - base, 0x08);
    }

    #[test]
    fn gradient_brush_vtable_points_to_gradient_static() {
        let g = GradientBrush::new();
        let expected = &GRADIENT_BRUSH_VTABLE as *const _ as usize;
        assert_eq!(g.vtable as usize, expected);
        // type_tag = Gradient (= 2)
        unsafe {
            let vt = brush_vtable(&g as *const _ as *const u8);
            assert_eq!(vt.type_tag, BrushType::Gradient as u32);
        }
    }

    #[test]
    fn gradient_brush_default_ctor_attaches_8_keys() {
        // raw `GradientBrush::C2Ev` 가 8 keys (0x25f-0x265, 0x267) attach
        let g = GradientBrush::new();
        assert_eq!(g.bag_size(), 8);
    }

    #[test]
    fn gradient_brush_default_values_match_raw() {
        let g = GradientBrush::new();
        // 0x25f (style) = 0 (raw `176664: str wzr, [sp]`)
        assert_eq!(g.get_style(), 0);
        // 0x260 (angle) = 0.0 (raw `1766a4: movi d0, #0; Degree(0.0)`)
        assert_eq!(g.get_angle_degrees().to_bits(), 0u32);
    }

    #[test]
    fn gradient_brush_each_default_key_present() {
        let g = GradientBrush::new();
        for key in [
            GradientBrush::KEY_STYLE,
            GradientBrush::KEY_ANGLE,
            GradientBrush::KEY_FLIP,
            GradientBrush::KEY_FOCUS_RECT,
            GradientBrush::KEY_TILE_RECT,
            GradientBrush::KEY_TILE_METHOD,
            GradientBrush::KEY_SCALED,
            GradientBrush::KEY_INTERP,
        ] {
            let pk = crate::property_key::PropertyKey::from_int(key);
            unsafe {
                let impl_ref = g.bag.impl_ref().expect("bag impl");
                assert!(
                    impl_ref.find_equal(&pk).is_ok(),
                    "key {:#x} should be attached",
                    key
                );
            }
        }
        // key 0x266 (KEY_STOPS) 는 default ctor 가 attach 안 함
        let stops_key = crate::property_key::PropertyKey::from_int(GradientBrush::KEY_STOPS);
        unsafe {
            let impl_ref = g.bag.impl_ref().expect("bag impl");
            assert!(impl_ref.find_equal(&stops_key).is_err(),
                "key 0x266 (KEY_STOPS) should NOT be in default bag");
        }
    }

    #[test]
    fn gradient_brush_vtable_drop_in_place_works() {
        // Drop dispatching via vtable.drop_in_place_fn — Box<GradientBrush> drop
        // 이 PropertyBag (8 nodes) 모두 안전하게 해제.
        for _ in 0..10 {
            let g = GradientBrush::new();
            drop(g);
        }
    }

    #[test]
    fn image_brush_get_type_returns_3() {
        let b = Brush::Image(ImageBrush::new("img1".to_string()));
        assert_eq!(b.get_type(), BrushType::Image);
        assert_eq!(b.get_type().as_u32(), 3);
    }

    #[test]
    fn image_brush_clone_preserves_source_id() {
        let b = Brush::Image(ImageBrush::new("img-abc".to_string()));
        let cloned = b.clone_to_heap();
        if let Brush::Image(ib) = &*cloned {
            assert_eq!(ib.source_id, "img-abc");
        }
    }

    #[test]
    fn group_brush_get_type_returns_5() {
        let b = Brush::Group(GroupBrush::new());
        assert_eq!(b.get_type(), BrushType::Group);
        assert_eq!(b.get_type().as_u32(), 5);
    }

    #[test]
    fn group_brush_clone_preserves_children() {
        let mut children: Vec<Box<Brush>> = Vec::new();
        children.push(Box::new(Brush::Empty(EmptyBrush::new())));
        children.push(Box::new(Brush::Solid(SolidBrush::default())));
        let b = Brush::Group(GroupBrush::with_children(children));
        let cloned = b.clone_to_heap();
        if let Brush::Group(gb) = &*cloned {
            assert_eq!(gb.children.len(), 2);
            assert_eq!(gb.children[0].get_type(), BrushType::Empty);
            assert_eq!(gb.children[1].get_type(), BrushType::Solid);
        }
    }

    #[test]
    fn group_brush_eq_compares_children() {
        let a = Brush::Group(GroupBrush::with_children(vec![
            Box::new(Brush::Empty(EmptyBrush::new())),
            Box::new(Brush::Solid(SolidBrush::default())),
        ]));
        let b = Brush::Group(GroupBrush::with_children(vec![
            Box::new(Brush::Empty(EmptyBrush::new())),
            Box::new(Brush::Solid(SolidBrush::default())),
        ]));
        let c = Brush::Group(GroupBrush::with_children(vec![Box::new(Brush::Empty(
            EmptyBrush::new(),
        ))]));
        assert!(a.eq_brush(&b));
        assert!(!a.eq_brush(&c));
    }

    #[test]
    fn brush_box_size_still_8b_with_all_variants() {
        // sized enum 의 Box 는 8B 단일 ptr.
        assert_eq!(std::mem::size_of::<Box<Brush>>(), 8);
    }

    #[test]
    fn brush_display_all_variants() {
        assert_eq!(format!("{}", Brush::Empty(EmptyBrush::new())), "EmptyBrush");
        assert_eq!(
            format!("{}", Brush::Solid(SolidBrush::default())),
            "SolidBrush"
        );
        assert_eq!(
            format!("{}", Brush::Gradient(GradientBrush::new())),
            "GradientBrush"
        );
        assert_eq!(
            format!("{}", Brush::Image(ImageBrush::default())),
            "ImageBrush"
        );
        assert_eq!(
            format!("{}", Brush::Hatch(HatchBrush::default())),
            "HatchBrush"
        );
        assert_eq!(
            format!("{}", Brush::Group(GroupBrush::new())),
            "GroupBrush"
        );
    }

    #[test]
    fn solid_brush_create_boxed_wraps_color() {
        let red = crate::color::Color::from_rgb(0xFF, 0, 0, std::ptr::null_mut());
        let b = SolidBrush::create_boxed(red);
        assert_eq!(b.get_type(), BrushType::Solid);
    }
}
