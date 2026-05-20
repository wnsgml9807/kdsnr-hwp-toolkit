//! `Hnc::Property::PropertyKey` — 16B `(int_id, string_id)` key.
//!
//! libHncFoundation 의 `PropertyKey` 는 std::map<PropertyKey, SharePtr<Property>>
//! 의 key. raw 의 두 가지 식별 modes:
//! - int-keyed (`PropertyKey(int)`): int_id = value, str_ptr = null
//! - string-keyed (`PropertyKey(CHncStringW const&)` 또는 `(wchar_t const*)`):
//!   int_id = 0, str_ptr = heap-alloc CHncStringW
//!
//! # raw 16B layout (확정 from `PropertyKey(int)` `0x4e2dc`)
//!
//! ```text
//! offset   field          type            크기
//! 0x00     int_id         u32             4B (4B pad to 0x08)
//! 0x08     str_ptr        CHncStringW*    8B
//! ```
//!
//! str_ptr 가 non-null 이면 heap-allocated 8B `CHncStringW` box 를 소유.
//!
//! # raw `PropertyKey(int)` @ `0x4e2dc`
//!
//! ```text
//! 0x4e2dc: str  w1, [x0]            ; [+0x00] = int (4B)
//! 0x4e2e0: str  xzr, [x0, #0x8]     ; [+0x08] = 0 (str_ptr null)
//! 0x4e2e4: ret
//! ```
//!
//! # raw `PropertyKey(CHncStringW const&)` @ `0x4e2f4`
//!
//! ```text
//! str  wzr, [x0]                   ; [+0x00] = 0 (int_id = 0)
//! bl   new(8)                      ; alloc 8B for CHncStringW box
//! ... CHncStringW copy ctor (refcount logic for global empty/immortal)
//! str  x20, [x19, #0x8]            ; [+0x08] = new CHncStringW*
//! ```
//!
//! # raw `~PropertyKey()` @ `0x4e4d4`
//!
//! ```text
//! ldr  x20, [x0, #0x8]             ; x20 = str_ptr
//! cbz  x20, exit                   ; if null, exit
//! ldr  x0, [x20]                   ; load wide buffer
//! ldr  w8, [x0, #-0xc]!            ; refcount at offset -0xc of buffer (pre-decrement)
//! cmn  w8, #0x2                    ; -2 = immortal
//! b.eq skip_decrement
//! bl   InterlockedDecrement
//! ; if refcount == 0: delete buffer-0xc
//! ; then delete CHncStringW box (x20)
//! ```

use crate::string_w::CHncStringW;
use std::ptr;

/// raw 16B `Hnc::Property::PropertyKey`.
///
/// **address stability**: PropertyKey 는 std::map 의 key 로 in-place 저장됨.
/// raw 의 layout/copy semantic 1:1.
#[repr(C)]
#[derive(Debug)]
pub struct PropertyKey {
    /// raw +0x00: int identifier (u32 + 4B pad).
    pub int_id: u32,
    /// 4B padding (raw 에선 압축; Rust 에선 align 강제).
    pub _pad: u32,
    /// raw +0x08: heap-allocated CHncStringW box ptr (null when int-keyed).
    pub str_ptr: *mut CHncStringW,
}

pub const PROPERTY_KEY_SIZE_BYTES: usize = 16;
pub const PROPERTY_KEY_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<PropertyKey>() == PROPERTY_KEY_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<PropertyKey>() == PROPERTY_KEY_ALIGN_BYTES);

impl PropertyKey {
    /// raw `PropertyKey::PropertyKey(int)` @ `0x4e2dc`.
    ///
    /// ```text
    /// str w1, [x0]              ; [+0x00] = id
    /// str xzr, [x0, #0x8]       ; [+0x08] = null
    /// ```
    #[inline]
    pub fn from_int(id: u32) -> Self {
        PropertyKey {
            int_id: id,
            _pad: 0,
            str_ptr: ptr::null_mut(),
        }
    }

    /// raw `PropertyKey::PropertyKey(CHncStringW const&)` @ `0x4e2f4`.
    ///
    /// int_id = 0, str_ptr = heap-alloc CHncStringW (copy of src).
    ///
    /// **현재 단계**: CHncStringW 가 Rust 의 owning copy semantics 를 가지므로
    /// `Box::new(src.clone())` 로 same effect — raw 의 refcount sharing 은
    /// `CHncStringW::clone` 내부에서 처리.
    pub fn from_string(src: &CHncStringW) -> Self {
        let boxed = Box::new(src.clone());
        PropertyKey {
            int_id: 0,
            _pad: 0,
            str_ptr: Box::into_raw(boxed),
        }
    }

    /// raw `PropertyKey::PropertyKey(wchar_t const*)` @ `0x4e3c4` 의 Rust equivalent.
    ///
    /// null → empty string (raw 의 default empty global).
    pub fn from_wide_chars(s: &[u16]) -> Self {
        let mut hnc = CHncStringW::default();
        if !s.is_empty() {
            hnc = CHncStringW::from_wide(s);
        }
        let boxed = Box::new(hnc);
        PropertyKey {
            int_id: 0,
            _pad: 0,
            str_ptr: Box::into_raw(boxed),
        }
    }

    /// raw 의 int-keyed 인지 (str_ptr == null + int_id != 0).
    ///
    /// 실제 raw 는 단순히 `str_ptr == null` 로 구분.
    #[inline]
    pub fn is_int_keyed(&self) -> bool {
        self.str_ptr.is_null()
    }

    /// raw PropertyKey 의 copy ctor / move semantics 를 모방 — int_id + CHncStringW
    /// box clone (CHncStringW 의 immortal/refcount 처리는 inner `clone` 에 위임).
    ///
    /// `PropertyBag::Attach` (raw `0x4cb0c`) 의 local copy 단계 (`w8 = [x1]; ldr x0,
    /// [x1+0x8]; bl 0x6e984`) 1:1 mirror.
    pub fn clone_op(&self) -> Self {
        let str_ptr = if self.str_ptr.is_null() {
            ptr::null_mut()
        } else {
            unsafe {
                let cloned = (*self.str_ptr).clone();
                Box::into_raw(Box::new(cloned))
            }
        };
        PropertyKey {
            int_id: self.int_id,
            _pad: self._pad,
            str_ptr,
        }
    }

    /// raw `int_id` 를 u32 로 반환 (int-keyed only).
    #[inline]
    pub fn int_value(&self) -> u32 {
        self.int_id
    }

    /// string key 접근 (null 가능). caller 가 lifetime 책임.
    ///
    /// # Safety
    /// `self.str_ptr` 가 valid 이거나 null.
    pub unsafe fn string_value(&self) -> Option<&CHncStringW> {
        if self.str_ptr.is_null() {
            None
        } else {
            Some(&*self.str_ptr)
        }
    }
}

/// `wcscmp` over CHncStringW wide-buffer slices, returning the raw signed delta
/// `a[i] - b[i]` at the first differing position (or `0` if all match).
///
/// raw 의 `wcscmp(a.data, b.data)` 는 null-terminated 가정. CHncStringW 는
/// `as_wide()` 가 explicit length 슬라이스 — `length()` 가 0 이거나 짧으면
/// terminator 가 length boundary 와 일치. 본 함수는 짧은 쪽이 다 끝났는데도
/// 일치하는 경우 길이 차이 (`len_a - len_b`) 를 반환 — `wcscmp` 의 null
/// terminator 비교를 simulate.
fn wide_compare(a: &[u16], b: &[u16]) -> i32 {
    let n = a.len().min(b.len());
    for i in 0..n {
        let av = a[i] as i32;
        let bv = b[i] as i32;
        if av != bv {
            return av - bv;
        }
    }
    // 짧은 쪽의 null terminator vs 긴 쪽의 다음 character.
    // raw 의 wcscmp 는 짧은 쪽이 0 (null) → 0 - longer[n] = negative.
    if a.len() < b.len() {
        return 0 - (b[n] as i32);
    } else if a.len() > b.len() {
        return (a[n] as i32) - 0;
    }
    0
}

impl PropertyKey {
    /// raw `operator==(PropertyKey const&) const` @ `0x4e58c` 1:1.
    ///
    /// ```text
    /// x8 = self.str_ptr; w10 = (x8 != 0)
    /// x9 = other.str_ptr; w11 = (x9 != 0)
    /// w10 = w10 ^ w11             ; XOR: 두 null-flag 동일하면 0
    /// if w10 != 0: return 0       ; 한 쪽만 null → not equal
    /// if x8 == 0 (both null): return self.int_id == other.int_id
    /// if x8 == x9: return 1       ; same CHncStringW box ptr
    /// return wcscmp(*x8, *x9) == 0
    /// ```
    pub fn eq_op(&self, other: &PropertyKey) -> bool {
        let self_str_null = self.str_ptr.is_null();
        let other_str_null = other.str_ptr.is_null();
        // raw 의 XOR: 한 쪽만 null 이면 not equal
        if self_str_null != other_str_null {
            return false;
        }
        if self_str_null {
            // both null → int compare
            return self.int_id == other.int_id;
        }
        // both non-null
        if self.str_ptr == other.str_ptr {
            return true;
        }
        unsafe {
            let a = (*self.str_ptr).as_wide();
            let b = (*other.str_ptr).as_wide();
            wide_compare(a, b) == 0
        }
    }

    /// raw `operator!=(PropertyKey const&) const` @ `0x4e610` — `!= ` = `eor #0x1` on eq.
    #[inline]
    pub fn ne_op(&self, other: &PropertyKey) -> bool {
        !self.eq_op(other)
    }

    /// raw `operator<(PropertyKey const&) const` @ `0x4e6a4` 1:1.
    ///
    /// **Ordering**: int-keyed (str=null) < string-keyed (str=non-null), then
    /// within int int_id 비교, within string wcscmp.
    ///
    /// ```text
    /// case 1: self.str=null  & other.str!=null → return 1   ; int < string
    /// case 2: self.str!=null & other.str=null  → return 0
    /// case 3: both null                        → int_id < other.int_id
    /// case 4: both non-null                    → wcscmp < 0
    /// ```
    pub fn lt_op(&self, other: &PropertyKey) -> bool {
        let self_str_null = self.str_ptr.is_null();
        let other_str_null = other.str_ptr.is_null();
        match (self_str_null, other_str_null) {
            (true, false) => true,            // int < string
            (false, true) => false,
            (true, true) => self.int_id < other.int_id,
            (false, false) => unsafe {
                // raw 의 wcscmp 결과의 sign bit 검사 → return (wcscmp < 0)
                let a = (*self.str_ptr).as_wide();
                let b = (*other.str_ptr).as_wide();
                wide_compare(a, b) < 0
            },
        }
    }
}

impl PartialEq for PropertyKey {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.eq_op(other)
    }
}

impl Eq for PropertyKey {}

impl PartialOrd for PropertyKey {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropertyKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if self.eq_op(other) {
            Ordering::Equal
        } else if self.lt_op(other) {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    }
}

impl Drop for PropertyKey {
    /// raw `~PropertyKey()` @ `0x4e4d4`.
    ///
    /// str_ptr 이 non-null 이면 CHncStringW 의 wide buffer refcount-- + free,
    /// 그 다음 CHncStringW box 자체를 `delete`.
    ///
    /// Rust port: `Box::from_raw` + auto-drop of CHncStringW (which handles its
    /// own buffer refcount via `Drop`).
    fn drop(&mut self) {
        if !self.str_ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.str_ptr));
            }
            self.str_ptr = ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<PropertyKey>(), 16);
        assert_eq!(std::mem::align_of::<PropertyKey>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let pk = PropertyKey::from_int(0x259);
        let p = &pk as *const PropertyKey as usize;
        assert_eq!(&pk.int_id as *const _ as usize - p, 0x00);
        assert_eq!(&pk.str_ptr as *const _ as usize - p, 0x08);
    }

    #[test]
    fn from_int_yields_null_string() {
        let pk = PropertyKey::from_int(0x259);
        assert_eq!(pk.int_id, 0x259);
        assert!(pk.str_ptr.is_null());
        assert!(pk.is_int_keyed());
    }

    #[test]
    fn from_string_yields_zero_int() {
        let s = CHncStringW::from_str("MyProperty");
        let pk = PropertyKey::from_string(&s);
        assert_eq!(pk.int_id, 0);
        assert!(!pk.str_ptr.is_null());
        assert!(!pk.is_int_keyed());
    }

    #[test]
    fn drop_int_keyed_is_noop() {
        for _ in 0..100 {
            let _ = PropertyKey::from_int(42);
        }
    }

    #[test]
    fn drop_string_keyed_releases_heap() {
        for _ in 0..100 {
            let s = CHncStringW::from_str("Test");
            let _ = PropertyKey::from_string(&s);
        }
    }

    #[test]
    fn many_distinct_int_keys() {
        let keys: Vec<_> = (0..1000u32).map(PropertyKey::from_int).collect();
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(k.int_id, i as u32);
            assert!(k.is_int_keyed());
        }
    }

    #[test]
    fn string_value_access() {
        let s = CHncStringW::from_str("StyleKey");
        let pk = PropertyKey::from_string(&s);
        unsafe {
            let v = pk.string_value().expect("str non-null");
            assert_eq!(v.length(), 8);
        }
    }

    #[test]
    fn int_value_access() {
        let pk = PropertyKey::from_int(601);
        assert_eq!(pk.int_value(), 601);
    }

    #[test]
    fn pad_field_zero() {
        let pk = PropertyKey::from_int(1);
        assert_eq!(pk._pad, 0);
    }

    // ---------- ordering tests (raw 0x4e58c/0x4e610/0x4e6a4) ----------

    #[test]
    fn eq_both_int_same_id() {
        let a = PropertyKey::from_int(601);
        let b = PropertyKey::from_int(601);
        assert!(a.eq_op(&b));
        assert!(!a.ne_op(&b));
    }

    #[test]
    fn eq_both_int_different_id() {
        let a = PropertyKey::from_int(601);
        let b = PropertyKey::from_int(602);
        assert!(!a.eq_op(&b));
        assert!(a.ne_op(&b));
    }

    #[test]
    fn eq_int_vs_string_never_equal() {
        let a = PropertyKey::from_int(0);
        let s = CHncStringW::from_str("X");
        let b = PropertyKey::from_string(&s);
        // raw XOR check: 한 쪽만 null → not equal
        assert!(!a.eq_op(&b));
        assert!(!b.eq_op(&a));
    }

    #[test]
    fn eq_both_string_same_content() {
        let s1 = CHncStringW::from_str("Color");
        let s2 = CHncStringW::from_str("Color");
        let a = PropertyKey::from_string(&s1);
        let b = PropertyKey::from_string(&s2);
        assert!(a.eq_op(&b));
    }

    #[test]
    fn eq_both_string_different_content() {
        let s1 = CHncStringW::from_str("Color");
        let s2 = CHncStringW::from_str("Width");
        let a = PropertyKey::from_string(&s1);
        let b = PropertyKey::from_string(&s2);
        assert!(!a.eq_op(&b));
    }

    #[test]
    fn lt_int_below_int() {
        let a = PropertyKey::from_int(100);
        let b = PropertyKey::from_int(200);
        assert!(a.lt_op(&b));
        assert!(!b.lt_op(&a));
    }

    #[test]
    fn lt_int_before_string() {
        let a = PropertyKey::from_int(99999);
        let s = CHncStringW::from_str("a");
        let b = PropertyKey::from_string(&s);
        // raw: (self.str=null & other.str!=null) → return 1
        assert!(a.lt_op(&b));
        assert!(!b.lt_op(&a));
    }

    #[test]
    fn lt_string_string_alpha() {
        let s1 = CHncStringW::from_str("a");
        let s2 = CHncStringW::from_str("b");
        let a = PropertyKey::from_string(&s1);
        let b = PropertyKey::from_string(&s2);
        assert!(a.lt_op(&b));
        assert!(!b.lt_op(&a));
    }

    #[test]
    fn lt_irreflexive() {
        let pk = PropertyKey::from_int(42);
        assert!(!pk.lt_op(&pk));
    }

    #[test]
    fn ord_traits_consistent() {
        use std::cmp::Ordering;
        let a = PropertyKey::from_int(1);
        let b = PropertyKey::from_int(2);
        assert_eq!(a.cmp(&b), Ordering::Less);
        assert_eq!(b.cmp(&a), Ordering::Greater);
        let c = PropertyKey::from_int(1);
        assert_eq!(a.cmp(&c), Ordering::Equal);
    }

    #[test]
    fn ord_with_string_keys() {
        use std::cmp::Ordering;
        let s = CHncStringW::from_str("X");
        let str_keyed = PropertyKey::from_string(&s);
        let int_keyed = PropertyKey::from_int(99);
        assert_eq!(int_keyed.cmp(&str_keyed), Ordering::Less);
        assert_eq!(str_keyed.cmp(&int_keyed), Ordering::Greater);
    }

    #[test]
    fn same_string_pointer_optimization() {
        // raw `cmp x8, x9; b.eq 0x4e604; mov w0, #0x1` — same ptr returns equal
        let s = CHncStringW::from_str("Shared");
        let pk1 = PropertyKey::from_string(&s);
        // Force same str_ptr by leaking — pk2 가 pk1 의 box 를 가리키게 한다.
        let same_ptr = pk1.str_ptr;
        let pk2 = PropertyKey {
            int_id: 0,
            _pad: 0,
            str_ptr: same_ptr,
        };
        assert!(pk1.eq_op(&pk2));
        // 둘 다 동일 ptr → drop 시 double-free. pk2 의 ptr 을 null 로 reset.
        std::mem::forget(pk2);
    }

    #[test]
    fn wide_compare_basic() {
        assert!(wide_compare(&[], &[]) == 0);
        assert!(wide_compare(&[65, 66], &[65, 66]) == 0);
        assert!(wide_compare(&[65, 66], &[65, 67]) < 0);
        assert!(wide_compare(&[65, 67], &[65, 66]) > 0);
    }

    #[test]
    fn wide_compare_length_diff_treats_null_terminator() {
        // shorter < longer prefix
        assert!(wide_compare(&[65], &[65, 66]) < 0);
        assert!(wide_compare(&[65, 66], &[65]) > 0);
    }
}
