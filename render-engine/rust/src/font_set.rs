//! `Hnc::Shape::FontSet` — 48B 1:1 byte-equivalent port.
//!
//! OOXML `<a:fontScheme>` 의 majorFont/minorFont 각각이 FontSet 1 개씩 보유.
//! Latin/ComplexScript/EastAsian 의 3 핵심 TextFont + 가변 SupplementalFont list.
//!
//! # raw 48B layout (확정 from `FontSet::FontSet` @ `0x169418` + 모든 accessor +
//! `AddSupplementalFont` + `~FontSet`)
//!
//! ```text
//! offset  field            타입                          의미
//! 0x00    latin            TextFont*                     8B owning ptr (auto_ptr.inner)
//! 0x08    complex_script   TextFont*                     8B owning ptr
//! 0x10    east_asian       TextFont*                     8B owning ptr
//! 0x18    sup_begin        SharePtr<SupplementalFont>*   8B vector.begin
//! 0x20    sup_end          SharePtr<SupplementalFont>*   8B vector.end
//! 0x28    sup_cap_end      SharePtr<SupplementalFont>*   8B vector.cap_end
//! ```
//!
//! 총 48B / 8B align.
//!
//! # raw `FontSet::FontSet(auto_ptr<TextFont> latin, auto_ptr<TextFont> cs, auto_ptr<TextFont> ea)` @ `0x169418`
//!
//! 1. Transfer 3 auto_ptr inner: `*(arg1)` → self[0], `*arg1 = nullptr`. Repeat for arg2, arg3.
//! 2. Init vec: `[+0x18..+0x30]` = (null, null, null) — `str xzr, [x22, #0x18]!; stp xzr, xzr, [x0, #0x20]`.
//! 3. `bl 0x6340a4(self+0x18, 20)` — vector::reserve(20) on the supplemental list.
//! 4. Null-check 3 TextFont* (latin/cs/ea); if any null, error path (throw).
//!
//! # raw `~FontSet()` @ `0x1696dc`
//!
//! 1. vector dtor on sup list (`bl 0x634354` — destroy each SharePtr + free buffer).
//! 2. Free east_asian (if non-null): ~CHncStringW(panose) + ~CHncStringW(typeface) + delete.
//! 3. Free complex_script: 동일.
//! 4. Free latin: 동일.
//!
//! # accessors (raw `0x169bf4..0x169c70`)
//!
//! - `GetLatin() const`: `ldr x0, [x0]; ret`.
//! - `GetComplexScript() const`: `ldr x0, [x0, #0x8]; ret`.
//! - `GetEastAsian() const`: `ldr x0, [x0, #0x10]; ret`.
//! - `Begin() const`: `ldr x0, [x0, #0x18]; ret`.
//! - `End() const`: `ldr x0, [x0, #0x20]; ret`.
//! - `AddSupplementalFont(SharePtr<SupplementalFont>)`:
//!   ```asm
//!   169c18-169c24: null/inner checks
//!   169c2c-169c34: x0 = vec.end, x9 = vec.cap_end, cmp
//!   169c38-169c4c: if end<cap: push_back ControlBlock*; refcount++
//!   169c50-169c58: else: vec.__push_back_with_realloc (call 0x6341d4)
//!   ```
//!
//! # 본 R-1.5.5 단계 scope
//!
//! - 48B layout + Field offsets 검증
//! - ctor (3 TextFont* ownership transfer + reserve(20) on sup vec)
//! - dtor (sup vec destroy + 3 TextFont free)
//! - accessors (GetLatin/CS/EA + Begin/End)
//! - AddSupplementalFont (capacity 충분 시의 inline push_back; realloc 경로는
//!   deferred — 본 byte-eq 가 Theme ctor 에서 도달 안 함)
//!
//! # 의도적 deferred
//!
//! - `operator==` / `operator!=` / `operator<` (`0x16976c..0x169a8c`) — TextFont 의
//!   비교 연산자 + SupplementalFont 비교 종속.
//! - Vector realloc path (`bl 0x6341d4`): capacity 초과 시 buffer 재할당.
//! - SupplementalFont 자체의 layout RE — FontSet byte-eq 에 본 sub-object 인스턴스 없음
//!   (Theme ctor 가 supplemental 안 추가).

use crate::share_ptr::{ControlBlock, SharePtr};
use crate::text_font::TextFont;
use std::alloc::Layout;
use std::ptr;

/// `Hnc::Shape::SupplementalFont` — opaque placeholder (RE 미실시).
///
/// 본 R-1.5.5 단계의 FontSet byte-eq 에 인스턴스 발생 안 함. 추후 RE 시점에
/// 실제 layout 추가.
pub struct SupplementalFont {
    /// Placeholder (raw 의 실제 layout 까지 zero-sized).
    _opaque: [u8; 0],
}

/// raw 48B `Hnc::Shape::FontSet`.
///
/// **owning** semantics: latin/cs/ea 의 TextFont* 는 ctor 시 ownership transfer
/// 받음 (raw 의 auto_ptr release). drop 시 자동 해제.
#[repr(C)]
pub struct FontSet {
    /// raw +0x00: latin TextFont* (owning).
    pub latin: *mut TextFont,
    /// raw +0x08: complex_script TextFont* (owning).
    pub complex_script: *mut TextFont,
    /// raw +0x10: east_asian TextFont* (owning).
    pub east_asian: *mut TextFont,
    /// raw +0x18: sup vector.begin.
    pub sup_begin: *mut *mut ControlBlock<SupplementalFont>,
    /// raw +0x20: sup vector.end.
    pub sup_end: *mut *mut ControlBlock<SupplementalFont>,
    /// raw +0x28: sup vector.cap_end.
    pub sup_cap_end: *mut *mut ControlBlock<SupplementalFont>,
}

pub const FONT_SET_SIZE_BYTES: usize = 48;
pub const FONT_SET_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<FontSet>() == FONT_SET_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<FontSet>() == FONT_SET_ALIGN_BYTES);

/// raw ctor 의 reserve(20) — 초기 supplemental vector capacity.
pub const SUP_VEC_INITIAL_CAPACITY: usize = 20;

impl FontSet {
    /// raw `FontSet::FontSet(auto_ptr<TextFont> latin, auto_ptr<TextFont> cs, auto_ptr<TextFont> ea)` (`0x169418`).
    ///
    /// auto_ptr 의 release 패턴: `*arg = inner; *arg_in = null`. Rust 에선
    /// `*mut TextFont` 를 직접 받고 caller 가 ownership 포기.
    ///
    /// # Safety
    /// - `latin`, `cs`, `ea` 모두 `TextFont::new_boxed` (또는 호환 heap alloc)
    ///   으로 얻은 valid ptr 이어야 함.
    /// - null 인자는 raw 의 error path 와 동일 — 본 메소드는 null 이어도 일단 받아
    ///   드리고 drop 시 raw_delete(null) = no-op. raw 의 throw 와 다름 (Rust 안전 default).
    pub unsafe fn new(
        latin: *mut TextFont,
        complex_script: *mut TextFont,
        east_asian: *mut TextFont,
    ) -> Box<Self> {
        // raw 의 ctor flow (3 transfer + reserve(20))
        let mut boxed = Box::new(FontSet {
            latin,
            complex_script,
            east_asian,
            sup_begin: ptr::null_mut(),
            sup_end: ptr::null_mut(),
            sup_cap_end: ptr::null_mut(),
        });
        boxed.reserve_supplemental(SUP_VEC_INITIAL_CAPACITY);
        boxed
    }

    /// raw `std::vector::reserve(n)` — n 개 SharePtr<SupplementalFont> entry 의
    /// buffer 미리 alloc. 본 메소드는 raw 의 `0x6340a4(vec, n)` 호출과 동등.
    ///
    /// # Safety
    /// `self` 의 sup_begin 이 null (empty vec) 이어야 함. 본 메소드는 ctor 의
    /// 초기 reserve 패턴만 가정.
    unsafe fn reserve_supplemental(&mut self, n: usize) {
        // 현재 capacity (in entries)
        let cur_cap = if self.sup_begin.is_null() {
            0
        } else {
            (self.sup_cap_end as usize - self.sup_begin as usize) / 8
        };
        if cur_cap >= n {
            return; // no-op
        }
        // raw alloc: n * 8 bytes (8B per ControlBlock*)
        let new_size = n * 8;
        let layout = Layout::from_size_align(new_size, 8).expect("FontSet reserve layout");
        let new_buf = std::alloc::alloc(layout) as *mut *mut ControlBlock<SupplementalFont>;
        if new_buf.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // Empty vec: end = begin = new_buf
        self.sup_begin = new_buf;
        self.sup_end = new_buf;
        self.sup_cap_end = new_buf.add(n);
    }

    /// raw `GetLatin() const` (`0x169bf4`): `ldr x0, [x0]; ret`.
    #[inline]
    pub fn get_latin(&self) -> *mut TextFont {
        self.latin
    }

    /// raw `GetComplexScript() const` (`0x169bfc`): `ldr x0, [x0, #0x8]; ret`.
    #[inline]
    pub fn get_complex_script(&self) -> *mut TextFont {
        self.complex_script
    }

    /// raw `GetEastAsian() const` (`0x169c04`): `ldr x0, [x0, #0x10]; ret`.
    #[inline]
    pub fn get_east_asian(&self) -> *mut TextFont {
        self.east_asian
    }

    /// raw `Begin() const` (`0x169c68`): `ldr x0, [x0, #0x18]; ret`.
    #[inline]
    pub fn sup_iter_begin(&self) -> *mut *mut ControlBlock<SupplementalFont> {
        self.sup_begin
    }

    /// raw `End() const` (`0x169c70`): `ldr x0, [x0, #0x20]; ret`.
    #[inline]
    pub fn sup_iter_end(&self) -> *mut *mut ControlBlock<SupplementalFont> {
        self.sup_end
    }

    /// supplemental list 의 element 개수.
    #[inline]
    pub fn sup_len(&self) -> usize {
        if self.sup_begin.is_null() {
            0
        } else {
            (self.sup_end as usize - self.sup_begin as usize) / 8
        }
    }

    /// raw `FontSet::FontSet(const FontSet&)` (`0x633c40`, 158 줄) 1:1 port.
    ///
    /// 알고리즘:
    /// 1. 3 TextFont* (latin/cs/ea):
    ///    - null → null
    ///    - non-null → alloc 24B + `TextFont::copy_from_raw` (typeface clone +
    ///      3B fields + panose clone)
    /// 2. supplemental vector (`std::vector<SharePtr<SupplementalFont>>`):
    ///    - init {begin/end/cap_end} = null
    ///    - reserve(src.size) (raw `0x6340a4`)
    ///    - 각 src entry 의 SharePtr clone (refcount++) → push
    ///
    /// raw asm 인용:
    /// ```text
    /// 633c40-633d14: 3 TextFont* clone (각 ~80B)
    /// 633d40-633d50: this+0x10 = ea_clone; vector init zeros
    /// 633d54-633d64: vector reserve(num_src_elem)
    /// 633d68-633d7c: range insert (각 entry SharePtr clone)
    /// ```
    ///
    /// # Safety
    /// `this` 는 uninit 48B heap slot. `src` 는 valid `*const FontSet`.
    pub unsafe fn copy_from_raw(this: *mut FontSet, src: *const FontSet) {
        // raw `633c60-633c98`: latin clone or null
        let src_latin = (*src).latin;
        let new_latin: *mut TextFont = if src_latin.is_null() {
            ptr::null_mut()
        } else {
            (*src_latin).clone_to_heap()
        };

        // raw `633ca0-633cd8`: cs clone or null
        let src_cs = (*src).complex_script;
        let new_cs: *mut TextFont = if src_cs.is_null() {
            ptr::null_mut()
        } else {
            (*src_cs).clone_to_heap()
        };

        // raw `633cdc-633d14`: ea clone or null
        let src_ea = (*src).east_asian;
        let new_ea: *mut TextFont = if src_ea.is_null() {
            ptr::null_mut()
        } else {
            (*src_ea).clone_to_heap()
        };

        // raw `633d40-633d50`: vector init zeros
        ptr::write(
            this,
            FontSet {
                latin: new_latin,
                complex_script: new_cs,
                east_asian: new_ea,
                sup_begin: ptr::null_mut(),
                sup_end: ptr::null_mut(),
                sup_cap_end: ptr::null_mut(),
            },
        );

        // raw `633d54-633d64: reserve(src.size)`
        let src_size_bytes =
            ((*src).sup_end as usize).wrapping_sub((*src).sup_begin as usize);
        let src_num = src_size_bytes / 8;
        if src_num == 0 {
            return; // empty vector — done
        }
        (*this).reserve_supplemental(src_num);

        // raw `633d68-633d7c: range insert` — 각 src entry clone (SharePtr refcount++)
        let mut src_p = (*src).sup_begin;
        let mut dst_p = (*this).sup_end;
        for _ in 0..src_num {
            let cb = *src_p; // ControlBlock<SupplementalFont>*
            if !cb.is_null() {
                // raw SharePtr clone: refcount++
                (*cb).refcount = (*cb).refcount.wrapping_add(1);
            }
            ptr::write(dst_p, cb);
            src_p = src_p.add(1);
            dst_p = dst_p.add(1);
        }
        (*this).sup_end = dst_p;
    }

    /// `FontSet::Clone() const` 동등 — alloc 48B + copy_from_raw.
    ///
    /// # Safety
    /// 반환 ptr 은 `Box::from_raw` 또는 `raw_delete` 로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut FontSet {
        let layout = Layout::new::<FontSet>();
        let new_p = std::alloc::alloc(layout) as *mut FontSet;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self::copy_from_raw(new_p, self as *const FontSet);
        new_p
    }

    /// heap-alloc FontSet* 해제.
    ///
    /// # Safety
    /// `p` 는 `clone_to_heap` 또는 heap alloc 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut FontSet) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, Layout::new::<FontSet>());
    }

    /// raw `AddSupplementalFont(SharePtr<SupplementalFont>)` (`0x169c0c`).
    ///
    /// 본 단계는 capacity 충분 시의 inline path 만 port (raw `0x169c2c-0x169c4c`).
    /// capacity 부족 시 raw 는 `0x6341d4` (vector realloc) 호출 — deferred.
    ///
    /// # Safety
    /// `sp` 는 valid SharePtr (또는 null). null 이거나 inner.obj==null 일 때
    /// 본 함수는 no-op (raw `cbz x8; cbz x9` path).
    pub unsafe fn add_supplemental_font(&mut self, sp: SharePtr<SupplementalFont>) {
        // raw `169c18: ldr x8, [x1]; cbz x8, exit` — sp.inner null check
        let cb = sp.as_raw();
        if cb.is_null() {
            return;
        }
        // raw `169c20: ldr x9, [x8]; cbz x9, exit` — sp.inner.obj null check
        if (*cb).obj.is_null() {
            return;
        }
        // raw `169c2c-169c34: end vs cap_end`
        if self.sup_end >= self.sup_cap_end {
            // raw realloc path — deferred (assertion).
            panic!("FontSet::add_supplemental_font: vector realloc deferred — increase initial capacity");
        }
        // raw `169c3c: str x8, [x0], #0x8` — *end++ = cb
        ptr::write(self.sup_end, cb);
        self.sup_end = self.sup_end.add(1);
        // raw `169c40-169c48: refcount++` — SharePtr.Clone semantic
        (*cb).refcount = (*cb).refcount.wrapping_add(1);
        // sp 는 본 fn return 시 drop (refcount--), 결과로 refcount net unchanged
        // → vector entry 가 own 한 채로 남음. Rust 는 sp drop 으로 1 decrement,
        // 우리는 직접 1 increment 했음 → net = 0 change. SharePtr.inner 가
        // 그대로 vector entry 에 살아남음. ✓
        drop(sp);
    }
}

impl Drop for FontSet {
    /// raw `~FontSet()` (`0x1696dc`).
    ///
    /// 1. sup vec destroy: 각 entry 의 SharePtr drop (refcount--) + free buffer.
    /// 2. east_asian / cs / latin TextFont* 각각 `~TextFont + operator_delete`.
    fn drop(&mut self) {
        unsafe {
            // 1. sup vec destroy (raw 0x1696f0-0x1696fc + 0x634354)
            //
            //   for each entry in [sup_begin..sup_end): SharePtr drop pattern
            //     (refcount-- + free CB + Box::drop T if 0)
            //   free buffer of size (sup_cap_end - sup_begin)
            if !self.sup_begin.is_null() {
                let mut p = self.sup_begin;
                while p < self.sup_end {
                    // raw SharePtr drop on entry: refcount--, free if 0
                    let cb = *p;
                    if !cb.is_null() {
                        let cb_ref = &mut *cb;
                        cb_ref.refcount = cb_ref.refcount.wrapping_sub(1);
                        if cb_ref.refcount == 0 {
                            if !cb_ref.obj.is_null() {
                                drop(Box::from_raw(cb_ref.obj));
                                cb_ref.obj = ptr::null_mut();
                            }
                            drop(Box::from_raw(cb));
                        }
                    }
                    p = p.add(1);
                }
                // free vec buffer
                let cap_bytes = self.sup_cap_end as usize - self.sup_begin as usize;
                if cap_bytes > 0 {
                    let layout = Layout::from_size_align(cap_bytes, 8)
                        .expect("FontSet vec buffer layout");
                    std::alloc::dealloc(self.sup_begin as *mut u8, layout);
                }
            }
            self.sup_begin = ptr::null_mut();
            self.sup_end = ptr::null_mut();
            self.sup_cap_end = ptr::null_mut();

            // 2. east_asian / cs / latin TextFont free (raw 의 역순)
            TextFont::raw_delete(self.east_asian);
            self.east_asian = ptr::null_mut();
            TextFont::raw_delete(self.complex_script);
            self.complex_script = ptr::null_mut();
            TextFont::raw_delete(self.latin);
            self.latin = ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string_w::CHncStringW;

    fn make_text_font(name: &str) -> *mut TextFont {
        unsafe {
            let typeface = CHncStringW::from_str(name);
            let panose = CHncStringW::default();
            TextFont::new_boxed(&typeface, 0, 0, false, &panose)
        }
    }

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<FontSet>(), 48);
        assert_eq!(std::mem::align_of::<FontSet>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let fs = unsafe { FontSet::new(make_text_font("L"), make_text_font("C"), make_text_font("E")) };
        let p = &*fs as *const FontSet as usize;
        assert_eq!(&fs.latin as *const _ as usize - p, 0x00);
        assert_eq!(&fs.complex_script as *const _ as usize - p, 0x08);
        assert_eq!(&fs.east_asian as *const _ as usize - p, 0x10);
        assert_eq!(&fs.sup_begin as *const _ as usize - p, 0x18);
        assert_eq!(&fs.sup_end as *const _ as usize - p, 0x20);
        assert_eq!(&fs.sup_cap_end as *const _ as usize - p, 0x28);
    }

    #[test]
    fn new_transfers_three_text_fonts_and_reserves_20() {
        unsafe {
            let l = make_text_font("Latin");
            let c = make_text_font("CS");
            let e = make_text_font("EA");
            let fs = FontSet::new(l, c, e);
            // 3 TextFont 가 own 됨
            assert_eq!(fs.get_latin(), l);
            assert_eq!(fs.get_complex_script(), c);
            assert_eq!(fs.get_east_asian(), e);
            // sup vec 가 reserve(20) 됨
            assert!(!fs.sup_begin.is_null());
            assert_eq!(fs.sup_begin, fs.sup_end); // empty (len=0)
            assert_eq!(fs.sup_len(), 0);
            // capacity = 20
            let cap_bytes = fs.sup_cap_end as usize - fs.sup_begin as usize;
            assert_eq!(cap_bytes, 160); // 20 × 8
        }
    }

    #[test]
    fn drop_releases_all_resources() {
        unsafe {
            for _ in 0..30 {
                let l = make_text_font("L");
                let c = make_text_font("C");
                let e = make_text_font("E");
                let fs = FontSet::new(l, c, e);
                drop(fs);
            }
        }
    }

    #[test]
    fn add_supplemental_font_pushes_to_vec() {
        unsafe {
            let mut fs = FontSet::new(
                make_text_font("L"),
                make_text_font("C"),
                make_text_font("E"),
            );
            assert_eq!(fs.sup_len(), 0);

            // SupplementalFont (placeholder ZST) 의 SharePtr 생성
            let sp1 = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let sp2 = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let sp3 = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });

            fs.add_supplemental_font(sp1);
            assert_eq!(fs.sup_len(), 1);
            fs.add_supplemental_font(sp2);
            assert_eq!(fs.sup_len(), 2);
            fs.add_supplemental_font(sp3);
            assert_eq!(fs.sup_len(), 3);
            // drop 시 vec entries 의 refcount-- 후 free.
        }
    }

    #[test]
    fn add_null_share_ptr_is_noop() {
        unsafe {
            let mut fs = FontSet::new(
                make_text_font("L"),
                make_text_font("C"),
                make_text_font("E"),
            );
            let null_sp = SharePtr::<SupplementalFont>::null();
            fs.add_supplemental_font(null_sp);
            assert_eq!(fs.sup_len(), 0);
        }
    }

    #[test]
    fn add_up_to_capacity_then_panic_on_realloc() {
        unsafe {
            let mut fs = FontSet::new(
                make_text_font("L"),
                make_text_font("C"),
                make_text_font("E"),
            );
            // Add 20 (= initial capacity)
            for _ in 0..20 {
                let sp = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
                fs.add_supplemental_font(sp);
            }
            assert_eq!(fs.sup_len(), 20);

            // 21번째는 panic (realloc deferred)
            let sp21 = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fs.add_supplemental_font(sp21);
            }));
            assert!(result.is_err(), "expected panic on capacity overflow");
        }
    }

    #[test]
    fn accessors_match_raw_offsets() {
        unsafe {
            let l = make_text_font("Latin");
            let c = make_text_font("CS");
            let e = make_text_font("EA");
            let fs = FontSet::new(l, c, e);
            assert_eq!(fs.get_latin(), l);
            assert_eq!(fs.get_complex_script(), c);
            assert_eq!(fs.get_east_asian(), e);
            assert_eq!(fs.sup_iter_begin(), fs.sup_begin);
            assert_eq!(fs.sup_iter_end(), fs.sup_end);
        }
    }

    #[test]
    fn supplemental_font_size_constants() {
        // SupplementalFont 는 opaque placeholder — 0B (RE 미완).
        assert_eq!(std::mem::size_of::<SupplementalFont>(), 0);
    }

    #[test]
    fn entry_size_is_8b_per_share_ptr() {
        // vec entry = 8B (SharePtr<T> = *mut ControlBlock<T>).
        assert_eq!(std::mem::size_of::<*mut ControlBlock<SupplementalFont>>(), 8);
    }

    #[test]
    fn sup_len_consistency_after_inserts() {
        unsafe {
            let mut fs = FontSet::new(
                make_text_font("L"),
                make_text_font("C"),
                make_text_font("E"),
            );
            for i in 0..15 {
                let sp = SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
                fs.add_supplemental_font(sp);
                assert_eq!(fs.sup_len(), i + 1);
            }
            // 모든 entry 가 valid SharePtr → drop 시 정상 cleanup
        }
    }

    // ===== FontSet::copy_from_raw + clone_to_heap tests =====

    #[test]
    fn copy_from_raw_with_all_text_fonts_null_and_empty_vec() {
        // Edge case: 3 null TextFont + empty supplemental vec
        unsafe {
            // Bypass normal ctor to set all-null state
            let src_layout = Layout::new::<FontSet>();
            let src = std::alloc::alloc_zeroed(src_layout) as *mut FontSet;
            // sup_begin/end/cap_end all null (zero-init via alloc_zeroed)
            assert!((*src).latin.is_null());
            assert!((*src).sup_begin.is_null());

            let dst_layout = Layout::new::<FontSet>();
            let dst = std::alloc::alloc(dst_layout) as *mut FontSet;
            FontSet::copy_from_raw(dst, src);

            assert!((*dst).latin.is_null());
            assert!((*dst).complex_script.is_null());
            assert!((*dst).east_asian.is_null());
            assert_eq!((*dst).sup_len(), 0);

            // Cleanup without calling Drop (since we used raw alloc and didn't
            // initialize properly for src). Manual dealloc only.
            std::alloc::dealloc(src as *mut u8, src_layout);
            // dst: Drop runs all-null path, then dealloc
            FontSet::raw_delete(dst);
        }
    }

    #[test]
    fn copy_from_raw_clones_three_text_fonts() {
        unsafe {
            let l = make_text_font("LatinFace");
            let c = make_text_font("CSFace");
            let e = make_text_font("EAFace");
            let src = FontSet::new(l, c, e);
            // src has 3 TextFont* + initial reserve(20) capacity, 0 entries

            let dst = (*src).clone_to_heap();

            // 3 TextFont* of dst are heap-allocated, distinct from src
            assert!(!(*dst).latin.is_null());
            assert!(!(*dst).complex_script.is_null());
            assert!(!(*dst).east_asian.is_null());
            assert_ne!((*dst).latin, src.latin);
            assert_ne!((*dst).complex_script, src.complex_script);
            assert_ne!((*dst).east_asian, src.east_asian);

            // Typefaces should byte-eq match (CHncStringW PartialEq compares content)
            let src_latin_typeface = (*src.latin).get_typeface();
            let dst_latin_typeface = (*(*dst).latin).get_typeface();
            assert_eq!(src_latin_typeface, dst_latin_typeface);
            assert_eq!(src_latin_typeface.length() as usize, "LatinFace".len());

            // Empty supplemental vec → dst's vec also empty (no reserve needed)
            assert_eq!((*dst).sup_len(), 0);

            FontSet::raw_delete(dst);
            // src auto-drops at end of scope
        }
    }

    #[test]
    fn copy_from_raw_clones_supplemental_with_refcount_increment() {
        unsafe {
            let l = make_text_font("L");
            let c = make_text_font("C");
            let e = make_text_font("E");
            let mut src = FontSet::new(l, c, e);

            // Add 3 supplemental fonts
            let sp1_orig =
                SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let cb1 = sp1_orig.as_raw();
            src.add_supplemental_font(sp1_orig); // refcount 1→2 inside add (net 1 after sp drop)

            let sp2_orig =
                SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let cb2 = sp2_orig.as_raw();
            src.add_supplemental_font(sp2_orig);

            let sp3_orig =
                SharePtr::<SupplementalFont>::from_object(SupplementalFont { _opaque: [] });
            let cb3 = sp3_orig.as_raw();
            src.add_supplemental_font(sp3_orig);

            // src.vec has 3 entries; each CB has refcount 1 (1 owning slot in src.vec)
            assert_eq!((*src).sup_len(), 3);
            assert_eq!((*cb1).refcount, 1);
            assert_eq!((*cb2).refcount, 1);
            assert_eq!((*cb3).refcount, 1);

            // Now clone
            let dst = (*src).clone_to_heap();

            // dst.vec should have 3 entries pointing to SAME CBs (shared ptr semantic)
            assert_eq!((*dst).sup_len(), 3);
            assert_eq!(*((*dst).sup_begin), cb1);
            assert_eq!(*((*dst).sup_begin.add(1)), cb2);
            assert_eq!(*((*dst).sup_begin.add(2)), cb3);

            // Each CB refcount now 2 (= 1 src + 1 dst)
            assert_eq!((*cb1).refcount, 2);
            assert_eq!((*cb2).refcount, 2);
            assert_eq!((*cb3).refcount, 2);

            // Drop dst — each refcount back to 1
            FontSet::raw_delete(dst);
            assert_eq!((*cb1).refcount, 1);
            assert_eq!((*cb2).refcount, 1);
            assert_eq!((*cb3).refcount, 1);

            // src auto-drops at end of scope → refcount 0 → free
        }
    }

    #[test]
    fn copy_from_raw_independent_text_fonts() {
        unsafe {
            let l = make_text_font("L");
            let c = make_text_font("C");
            let e = make_text_font("E");
            let src = FontSet::new(l, c, e);
            let dst = (*src).clone_to_heap();
            // Mutating src TextFont shouldn't affect dst (heap-independent).
            (*src.latin).set_charset(99);
            assert_eq!((*src.latin).get_charset(), 99);
            assert_eq!((*(*dst).latin).get_charset(), 0); // default
            FontSet::raw_delete(dst);
        }
    }
}
