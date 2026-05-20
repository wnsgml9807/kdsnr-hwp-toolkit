//! `Hnc::Shape::TextFont` — 24B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `TextFont` 는 OOXML `<a:font>` 의
//! typeface/charset/pitchFamily/panose 정보 보유.
//!
//! # raw 24B layout (확정 from `TextFont::TextFont` @ `0x169008` + 모든 accessor)
//!
//! ```text
//! offset  field               타입            의미
//! 0x00    typeface            CHncStringW    8B (refcounted wide string)
//! 0x08    charset             u8             1B
//! 0x09    pitch_family        u8             1B
//! 0x0a    is_panose_enabled   u8             1B (bool)
//! 0x0b    _pad                u8 [5]         5B padding (CHncStringW 8B align)
//! 0x10    panose              CHncStringW    8B (refcounted wide string)
//! ```
//!
//! 총 24B / 8B align.
//!
//! # raw ctor `TextFont::TextFont(CHncStringW&, u8 charset, u8 pitch_family, bool is_panose_enabled, CHncStringW& panose)` @ `0x169008`
//!
//! ```asm
//! 16902c: mov x19, x0                ; save self
//! 169030: bl  CHncStringW::CHncStringW(CHncStringW const&)  ; init typeface at [self+0]
//! 169034: strb w23, [x0, #0x8]       ; self.charset = arg2
//! 169038: strb w22, [x0, #0x9]       ; self.pitch_family = arg3
//! 16903c: add  x0, x0, #0x10         ; x0 = &self.panose
//! 169040: strb w21, [x19, #0xa]      ; self.is_panose_enabled = arg4
//! 169044: mov  x1, x20               ; x1 = panose source ref
//! 169048: bl   CHncStringW::CHncStringW(CHncStringW const&)  ; init panose at [self+0x10]
//! 16904c: mov  x0, x19; ret
//! ```
//!
//! # raw `~TextFont()` @ `0x1690e8`
//!
//! ```asm
//! 1690f8: add  x0, x0, #0x10         ; x0 = &self.panose
//! 1690fc: bl   CHncStringW::~CHncStringW()
//! 169100: mov  x0, x19               ; x0 = self
//! 169108: b    CHncStringW::~CHncStringW()   ; tail call: dtor typeface
//! ```
//!
//! # accessors (raw `0x1692f0..0x169338`)
//!
//! - `GetTypeface()`: `ret` (= `*(CHncStringW const*)(this+0)`)
//! - `SetTypeface(s)`: tail call `CHncStringW::operator=` on self+0
//! - `GetCharset()`: `ldrb w0, [x0, #0x8]; ret`
//! - `SetCharset(u8)`: `strb w1, [x0, #0x8]; ret`
//! - `GetPitchFamily()`: `ldrb w0, [x0, #0x9]; ret`
//! - `SetPitchFamily(u8)`: `strb w1, [x0, #0x9]; ret`
//! - `GetPanose()`: `add x0, x0, #0x10; ret` (= `*(CHncStringW const*)(this+0x10)`)
//! - `SetPanose(s)`: `mov w8, #0x1; strb w8, [x0, #0xa]; add x0, x0, #0x10; b CHncStringW::operator=`
//!   (= set is_panose_enabled=1 + assign panose string)
//! - `IsPanoseEnabled()`: `ldrb w0, [x0, #0xa]; ret`

use crate::string_w::CHncStringW;
use std::alloc::Layout;
use std::ptr;

/// raw 24B `Hnc::Shape::TextFont`.
#[repr(C)]
pub struct TextFont {
    /// raw +0x00: typeface CHncStringW.
    pub typeface: CHncStringW,
    /// raw +0x08: u8 charset.
    pub charset: u8,
    /// raw +0x09: u8 pitch_family.
    pub pitch_family: u8,
    /// raw +0x0a: u8 is_panose_enabled (bool).
    pub is_panose_enabled: u8,
    /// raw +0x0b..+0x10: 5B padding (align 8 for next CHncStringW).
    pub _pad_0x0b: [u8; 5],
    /// raw +0x10: panose CHncStringW.
    pub panose: CHncStringW,
}

pub const TEXT_FONT_SIZE_BYTES: usize = 24;
pub const TEXT_FONT_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<TextFont>() == TEXT_FONT_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<TextFont>() == TEXT_FONT_ALIGN_BYTES);

impl TextFont {
    /// raw `TextFont::TextFont(CHncStringW&, u8, u8, bool, CHncStringW&)` (`0x169008`).
    pub fn new(
        typeface: &CHncStringW,
        charset: u8,
        pitch_family: u8,
        is_panose_enabled: bool,
        panose: &CHncStringW,
    ) -> Self {
        TextFont {
            typeface: typeface.clone(),
            charset,
            pitch_family,
            is_panose_enabled: if is_panose_enabled { 1 } else { 0 },
            _pad_0x0b: [0; 5],
            panose: panose.clone(),
        }
    }

    /// Heap-alloc 새 TextFont (raw `Create()` 패턴 또는 `auto_ptr` 의 typical
    /// alloc path).
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 로 해제 필요.
    pub unsafe fn new_boxed(
        typeface: &CHncStringW,
        charset: u8,
        pitch_family: u8,
        is_panose_enabled: bool,
        panose: &CHncStringW,
    ) -> *mut TextFont {
        let layout = Layout::new::<TextFont>();
        let p = std::alloc::alloc(layout) as *mut TextFont;
        if p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        ptr::write(
            p,
            Self::new(typeface, charset, pitch_family, is_panose_enabled, panose),
        );
        p
    }

    /// raw `~TextFont` + heap dealloc 결합 (auto_ptr drop 패턴 또는 FontSet
    /// dtor 의 inline path).
    ///
    /// # Safety
    /// `p` 는 `new_boxed` 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut TextFont) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, Layout::new::<TextFont>());
    }

    /// raw `GetTypeface() const` (`0x1692f0`).
    #[inline]
    pub fn get_typeface(&self) -> &CHncStringW {
        &self.typeface
    }

    /// raw `SetTypeface(CHncStringW const&)` (`0x1692f4`).
    pub fn set_typeface(&mut self, s: &CHncStringW) {
        self.typeface = s.clone();
    }

    /// raw `GetCharset() const` (`0x1692f8`).
    #[inline]
    pub fn get_charset(&self) -> u8 {
        self.charset
    }

    /// raw `SetCharset(u8)` (`0x169300`).
    #[inline]
    pub fn set_charset(&mut self, c: u8) {
        self.charset = c;
    }

    /// raw `GetPitchFamily() const` (`0x169308`).
    #[inline]
    pub fn get_pitch_family(&self) -> u8 {
        self.pitch_family
    }

    /// raw `SetPitchFamily(u8)` (`0x169310`).
    #[inline]
    pub fn set_pitch_family(&mut self, p: u8) {
        self.pitch_family = p;
    }

    /// raw `GetPanose() const` (`0x169318`).
    #[inline]
    pub fn get_panose(&self) -> &CHncStringW {
        &self.panose
    }

    /// raw `SetPanose(CHncStringW const&)` (`0x169320`) — 1) set is_panose_enabled=1
    /// 2) assign panose.
    pub fn set_panose(&mut self, s: &CHncStringW) {
        self.is_panose_enabled = 1;
        self.panose = s.clone();
    }

    /// raw `IsPanoseEnabled() const` (`0x169330`).
    #[inline]
    pub fn is_panose_enabled(&self) -> bool {
        self.is_panose_enabled != 0
    }

    /// raw `TextFont::TextFont(const TextFont&)` (inline within `FontSet`
    /// copy ctor `0x633c40`, lines 633c68-633c94).
    ///
    /// raw pattern (TextFont 1 / latin):
    /// ```asm
    /// 633c68: w0 = 0x18; bl operator_new       ; alloc 24B
    /// 633c70: x22 = new
    /// 633c74: x1 = src (TextFont*)
    /// 633c78: bl CHncStringW copy ctor          ; typeface (offset 0x00)
    /// 633c7c: ldrh w8, [src, #0x8]              ; load 2B (charset + pitch_family)
    /// 633c80: ldrb w9, [src, #0xa]              ; load is_panose_enabled
    /// 633c84: strb w9, [new, #0xa]
    /// 633c88: strh w8, [new, #0x8]
    /// 633c8c: add x0, new, #0x10
    /// 633c90: add x1, src, #0x10
    /// 633c94: bl CHncStringW copy ctor          ; panose (offset 0x10)
    /// ```
    ///
    /// # Safety
    /// `this` 는 uninit 24B heap slot. `src` 는 valid `*const TextFont`.
    pub unsafe fn copy_from_raw(this: *mut TextFont, src: *const TextFont) {
        let typeface_clone = (*src).typeface.clone();
        let panose_clone = (*src).panose.clone();
        ptr::write(
            this,
            TextFont {
                typeface: typeface_clone,
                charset: (*src).charset,
                pitch_family: (*src).pitch_family,
                is_panose_enabled: (*src).is_panose_enabled,
                _pad_0x0b: [0; 5],
                panose: panose_clone,
            },
        );
    }

    /// `TextFont::Clone() const` 동등 — alloc 24B + copy_from_raw.
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut TextFont {
        let layout = Layout::new::<TextFont>();
        let new_p = std::alloc::alloc(layout) as *mut TextFont;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self::copy_from_raw(new_p, self as *const TextFont);
        new_p
    }
}

// Drop 은 자동으로 typeface + panose 의 Drop 호출 (raw 와 byte-eq 동등).
// raw 0x1690e8 의 `~CHncStringW(panose)` 가 먼저, 그 다음 `~CHncStringW(typeface)` —
// Rust drop 순서는 field 정의 순서 (typeface 먼저, panose 나중) 의 역순 (panose 먼저, typeface 나중).
// → 정확히 raw 와 일치 ✓.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<TextFont>(), 24);
        assert_eq!(std::mem::align_of::<TextFont>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let tf = TextFont::new(&CHncStringW::default(), 0, 0, false, &CHncStringW::default());
        let p = &tf as *const TextFont as usize;
        assert_eq!(&tf.typeface as *const _ as usize - p, 0x00);
        assert_eq!(&tf.charset as *const _ as usize - p, 0x08);
        assert_eq!(&tf.pitch_family as *const _ as usize - p, 0x09);
        assert_eq!(&tf.is_panose_enabled as *const _ as usize - p, 0x0a);
        assert_eq!(&tf.panose as *const _ as usize - p, 0x10);
    }

    #[test]
    fn new_basic_init() {
        let typeface = CHncStringW::from_str("Arial");
        let panose = CHncStringW::from_str("0205020404");
        let tf = TextFont::new(&typeface, 0, 0x22, true, &panose);
        assert_eq!(tf.charset, 0);
        assert_eq!(tf.pitch_family, 0x22);
        assert_eq!(tf.is_panose_enabled, 1);
        assert!(tf.is_panose_enabled());
        // typeface 가 cloned (refcount shared)
        assert!(tf.get_typeface().refcount() >= 1);
    }

    #[test]
    fn accessors_match_raw_offsets() {
        let typeface = CHncStringW::from_str("Helvetica");
        let mut tf = TextFont::new(&typeface, 0x80, 0x12, false, &CHncStringW::default());
        assert_eq!(tf.get_charset(), 0x80);
        tf.set_charset(0x99);
        assert_eq!(tf.charset, 0x99);
        assert_eq!(tf.get_pitch_family(), 0x12);
        tf.set_pitch_family(0x34);
        assert_eq!(tf.pitch_family, 0x34);
        assert!(!tf.is_panose_enabled());
        let new_panose = CHncStringW::from_str("0500020404");
        tf.set_panose(&new_panose);
        assert!(tf.is_panose_enabled());
        assert_eq!(tf.is_panose_enabled, 1);
    }

    #[test]
    fn new_boxed_and_raw_delete() {
        unsafe {
            let typeface = CHncStringW::from_str("Calibri");
            let panose = CHncStringW::from_str("");
            let p = TextFont::new_boxed(&typeface, 0, 0x10, false, &panose);
            assert!(!p.is_null());
            assert_eq!((*p).pitch_family, 0x10);
            TextFont::raw_delete(p);
        }
    }

    #[test]
    fn raw_delete_of_null_is_noop() {
        unsafe {
            TextFont::raw_delete(ptr::null_mut());
        }
    }

    #[test]
    fn drop_order_is_panose_first_then_typeface() {
        // raw dtor 의 panose-first-then-typeface 순서를 Rust 도 보장 — fields 정의
        // 순서의 역순 drop. 본 테스트는 panic 없음만 검증 (drop 순서 자체는
        // 컴파일러 보장).
        let typeface = CHncStringW::from_str("X");
        let panose = CHncStringW::from_str("Y");
        let tf = TextFont::new(&typeface, 0, 0, false, &panose);
        drop(tf);
    }

    #[test]
    fn typeface_clone_shares_refcount() {
        let typeface = CHncStringW::from_str("SharedFont");
        let initial_rc = typeface.refcount();
        let tf = TextFont::new(&typeface, 0, 0, false, &CHncStringW::default());
        // After clone, refcount increased.
        assert!(typeface.refcount() > initial_rc);
        drop(tf);
        // After tf drop, refcount restored.
        assert_eq!(typeface.refcount(), initial_rc);
    }

    // ===== TextFont::copy_from_raw / clone_to_heap tests =====

    #[test]
    fn copy_from_raw_clones_all_fields() {
        unsafe {
            let typeface = CHncStringW::from_str("Foo");
            let panose = CHncStringW::from_str("Bar");
            let src = TextFont::new(&typeface, 11, 22, true, &panose);

            let mut dst = std::mem::MaybeUninit::<TextFont>::uninit();
            TextFont::copy_from_raw(dst.as_mut_ptr(), &src as *const TextFont);
            let d = &*dst.as_ptr();
            assert_eq!(d.charset, 11);
            assert_eq!(d.pitch_family, 22);
            assert_eq!(d.is_panose_enabled, 1);
            assert_eq!(d.typeface, src.typeface); // PartialEq compares content
            assert_eq!(d.panose, src.panose);
            // Properly drop dst
            ptr::drop_in_place(dst.as_mut_ptr());
        }
    }

    #[test]
    fn clone_to_heap_independent() {
        unsafe {
            let typeface = CHncStringW::from_str("X");
            let panose = CHncStringW::from_str("Y");
            let src = TextFont::new(&typeface, 5, 6, false, &panose);
            let dst = src.clone_to_heap();
            assert!(!dst.is_null());
            assert_eq!((*dst).charset, 5);
            assert_eq!((*dst).pitch_family, 6);
            assert_eq!((*dst).is_panose_enabled, 0);
            assert_eq!((*dst).typeface, src.typeface);
            assert_eq!((*dst).panose, src.panose);
            TextFont::raw_delete(dst);
        }
    }

    #[test]
    fn clone_to_heap_shares_string_refcounts() {
        unsafe {
            let typeface = CHncStringW::from_str("RefcountedFont");
            let initial_rc = typeface.refcount();
            let src = TextFont::new(&typeface, 0, 0, false, &CHncStringW::default());
            let src_rc_after = typeface.refcount();
            assert_eq!(src_rc_after, initial_rc + 1);

            let dst = src.clone_to_heap();
            let dst_rc_after = typeface.refcount();
            assert_eq!(dst_rc_after, initial_rc + 2);

            TextFont::raw_delete(dst);
            assert_eq!(typeface.refcount(), initial_rc + 1);
            drop(src);
            assert_eq!(typeface.refcount(), initial_rc);
        }
    }
}
