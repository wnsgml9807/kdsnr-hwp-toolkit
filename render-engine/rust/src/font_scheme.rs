//! `Hnc::Shape::FontScheme` — 24B 1:1 byte-equivalent port.
//!
//! libHncDrawingEngine_arm64 의 `FontScheme` 는 OOXML `<a:fontScheme>` 의
//! majorFont/minorFont 보유 + name.
//!
//! # raw 24B layout (확정 from `FontScheme::FontScheme` @ `0x169dbc`)
//!
//! ```text
//! offset  field    타입            의미
//! 0x00    name     CHncStringW    8B (refcounted)
//! 0x08    major    FontSet*       8B (auto_ptr transferred, owning)
//! 0x10    minor    FontSet*       8B (auto_ptr transferred, owning)
//! ```
//!
//! 총 24B / 8B align.
//!
//! # raw `FontScheme::FontScheme(auto_ptr<FontSet> major, auto_ptr<FontSet> minor)` @ `0x169dbc`
//!
//! ```asm
//! 169dd0: bl  CHncStringW::CHncStringW()    ; name default init
//! 169dd4-169de0: transfer 2 FontSet* (auto_ptr release pattern)
//! 169de4: stp x8, x9, [x0, #0x8]            ; [self+0x8] = major, [self+0x10] = minor
//! ```
//!
//! # 본 단계 scope
//!
//! Theme(bool) ctor 가 EMPTY FontScheme 을 inline 으로 생성 (24B alloc +
//! CHncStringW default + 2 null FontSet*). 본 단계는 그 inline form 도 지원:
//! `new_empty()` 으로 2 null FontSet 가진 FontScheme 생성.
//!
//! # raw `~FontScheme()` @ `0x169e2c` (RE 미상세, dtor 패턴 표준)
//!
//! 1. ~FontSet on minor (if non-null) + free
//! 2. ~FontSet on major (if non-null) + free
//! 3. tail call ~CHncStringW on name

use crate::font_set::FontSet;
use crate::string_w::CHncStringW;
use std::alloc::Layout;
use std::ptr;

/// raw 24B `Hnc::Shape::FontScheme`.
#[repr(C)]
pub struct FontScheme {
    /// raw +0x00: name CHncStringW.
    pub name: CHncStringW,
    /// raw +0x08: major FontSet* (owning, nullable).
    pub major: *mut FontSet,
    /// raw +0x10: minor FontSet* (owning, nullable).
    pub minor: *mut FontSet,
}

pub const FONT_SCHEME_SIZE_BYTES: usize = 24;
pub const FONT_SCHEME_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<FontScheme>() == FONT_SCHEME_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<FontScheme>() == FONT_SCHEME_ALIGN_BYTES);

impl FontScheme {
    /// raw `FontScheme::FontScheme(auto_ptr<FontSet>, auto_ptr<FontSet>)` (`0x169dbc`).
    ///
    /// # Safety
    /// `major` / `minor` 는 `Box::into_raw(Box<FontSet>)` 등동등 ownership.
    pub unsafe fn new(major: *mut FontSet, minor: *mut FontSet) -> Self {
        FontScheme {
            name: CHncStringW::default(),
            major,
            minor,
        }
    }

    /// Theme(true) ctor 의 inline-empty FontScheme — name nil + 2 null FontSet.
    ///
    /// raw 의 Theme ctor `0x1eb9f0-0x1eba18` 에서 alloc 24B + CHncStringW default
    /// + `stp xzr, xzr, [x27, #0x8]` 으로 만드는 패턴과 동등.
    pub fn new_empty() -> Self {
        FontScheme {
            name: CHncStringW::default(),
            major: ptr::null_mut(),
            minor: ptr::null_mut(),
        }
    }

    /// raw `GetName() const` (`0x16a0d8`).
    #[inline]
    pub fn get_name(&self) -> &CHncStringW {
        &self.name
    }

    /// raw `SetName(CHncStringW const&)` (`0x16a0dc`).
    pub fn set_name(&mut self, s: &CHncStringW) {
        self.name = s.clone();
    }

    /// raw `GetMajorFont() const` (`0x16a0e0`).
    #[inline]
    pub fn get_major_font(&self) -> *mut FontSet {
        self.major
    }

    /// raw `GetMinorFont() const` (`0x16a440`).
    #[inline]
    pub fn get_minor_font(&self) -> *mut FontSet {
        self.minor
    }

    /// raw `FontScheme::FontScheme(const FontScheme&)` (`0x6321a8`) 1:1.
    ///
    /// 알고리즘:
    /// 1. CHncStringW name copy ctor (refcount++).
    /// 2. major: src.major 가 null 이면 null; non-null 이면 `new FontSet(*src.major)`
    ///    (FontSet copy ctor at raw `0x633c40`).
    /// 3. minor: 동일.
    ///
    /// **byte-eq 완전 도달**: FontSet copy ctor (`0x633c40`) 도 본 세션에 1:1 port됨.
    /// 3 TextFont 분기 + supplemental vector 의 SharePtr clone 모두 byte-eq.
    ///
    /// # Safety
    /// `src` 는 valid `*const FontScheme`. self 는 zeroed/uninit slot 으로 가정 (heap alloc).
    pub unsafe fn copy_from_raw(this: *mut FontScheme, src: *const FontScheme) {
        // raw `6321c0: bl CHncStringW copy ctor` — name field at offset 0
        let name_clone = (*src).name.clone();

        // raw `6321c4-6321e4`: major field
        let src_major = (*src).major;
        let new_major: *mut FontSet = if src_major.is_null() {
            // raw `63221c-632224: mov x22, #0; str` — null path
            ptr::null_mut()
        } else {
            // raw `6321cc-6321e4`: alloc 48B + FontSet copy ctor
            (*src_major).clone_to_heap()
        };

        // raw `6321e8-632204` (or `632228-632234`): minor field
        let src_minor = (*src).minor;
        let new_minor: *mut FontSet = if src_minor.is_null() {
            ptr::null_mut()
        } else {
            (*src_minor).clone_to_heap()
        };

        ptr::write(
            this,
            FontScheme {
                name: name_clone,
                major: new_major,
                minor: new_minor,
            },
        );
    }

    /// raw `FontScheme::Clone() const` (`0x16a030`) — sret 1:1.
    ///
    /// `alloc 24B + copy_from_raw(new, this)` 패턴.
    ///
    /// # Safety
    /// 반환 ptr 은 `raw_delete` 또는 `Box::from_raw` 로 해제.
    pub unsafe fn clone_to_heap(&self) -> *mut FontScheme {
        let layout = Layout::new::<FontScheme>();
        let new_p = std::alloc::alloc(layout) as *mut FontScheme;
        if new_p.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self::copy_from_raw(new_p, self as *const FontScheme);
        new_p
    }

    /// raw heap-alloc 해제 — Theme dtor 호출 패턴 1:1.
    ///
    /// # Safety
    /// `p` 는 `clone_to_heap` 또는 heap alloc 으로 얻은 ptr 또는 null.
    pub unsafe fn raw_delete(p: *mut FontScheme) {
        if p.is_null() {
            return;
        }
        ptr::drop_in_place(p);
        std::alloc::dealloc(p as *mut u8, Layout::new::<FontScheme>());
    }

    /// raw `Hnc::Shape::FontScheme::CreateDefaultFontSet()` @ `0x16a144` (190 instr) 1:1.
    ///
    /// 호출 사이트: `Theme::GetMajorFont/GetMinorFont` 의 lazy-init path.
    /// 인자 없음, 반환 `FontSet*` (raw heap-alloc).
    ///
    /// # raw 알고리즘 (high-level, 3-block + final assembly)
    ///
    /// ## Block 1 — latin TextFont
    /// 1. CHncStringW("HNC_GO_B_HINT_GS") at `sp+0x30` (raw `0x16a168`)
    /// 2. CHncStringW("") at `sp+0x28` (raw `0x16a178`)
    /// 3. `new(0x18)` → x20 = TextFont* (raw `0x16a17c-184`)
    /// 4. typeface (= +0x00) ← CHncStringW C1 copy from sp+0x30 (raw `0x16a18c`)
    /// 5. `strh w8={0x0001}, [x20+0x8]` → charset=1, pitch_family=0 (raw `0x16a190-94`)
    /// 6. `strb wzr, [x20+0xa]` → is_panose_enabled=false (raw `0x16a198`)
    /// 7. panose (= +0x10) ← CHncStringW C1 copy from sp+0x28 (raw `0x16a1a4`)
    /// 8. `stur x20, [x29-0x28]` → latin auto_ptr slot (raw `0x16a1a8`)
    /// 9. Drop sp+0x28, sp+0x30 (raw `0x16a1ac-b8`)
    ///
    /// ## Block 2 — cs TextFont (identical content to latin)
    /// 동일한 패턴 — typeface="HNC_GO_B_HINT_GS", charset=1, panose=""
    /// x21 = 2nd TextFont*. 저장: `str x21, [sp+0x30]` (raw `0x16a208`)
    ///
    /// ## Block 3 — ea TextFont
    /// typeface="", charset=1, panose=""
    /// x22 = 3rd TextFont*.
    ///
    /// ## Final assembly (raw `0x16a278-294`)
    /// - 3 auto_ptr slots 배치: sp+0x10 = latin (x20), sp+0x8 = cs (x21), sp+0x0 = ea (x22)
    /// - x8 = sp+0x20 (sret slot for FontSet*)
    /// - `bl FontSet::Create(latin, cs, ea)` (raw `0x16a298`)
    /// - `ldr x8, [sp+0x20]; str x8, [x19]` → 결과를 outer sret 으로 (raw `0x16a29c-a0`)
    /// - Drop 3 auto_ptr slots (now null after Create transferred ownership) (raw `0x16a2a4-2f4`)
    ///
    /// # 본 port 의 byte-eq scope
    /// - TextFont layout (24B) + flag bytes (charset=1, pitch_family=0, is_panose_enabled=false) ✓
    /// - String constants ("HNC_GO_B_HINT_GS" / "") UTF-16LE ✓
    /// - FontSet ownership transfer via `FontSet::new` ✓
    /// - Return as `Box::into_raw(Box<FontSet>)` → matching raw `*mut FontSet`
    ///
    /// # Safety
    /// 반환 ptr 은 `Box::from_raw` 또는 동등 free 로 해제 필요.
    pub unsafe fn create_default_font_set() -> *mut FontSet {
        use crate::string_w::CHncStringW;
        use crate::text_font::TextFont;

        // raw `0x757c48`: L"HNC_GO_B_HINT_GS"
        const HNC_GO_B_HINT_GS: &str = "HNC_GO_B_HINT_GS";
        // raw `0x75794c`: L"" (empty string)
        const EMPTY: &str = "";

        // ----- Block 1: latin TextFont
        // raw 0x16a168/178: 2 CHncStringW locals
        let latin_typeface = CHncStringW::from_str(HNC_GO_B_HINT_GS);
        let latin_panose = CHncStringW::from_str(EMPTY);
        // raw 0x16a17c-1a4: alloc 24B + init flags + 2 CHncStringW copy ctor
        let latin = TextFont::new_boxed(
            &latin_typeface,
            1,     // charset (raw `mov w8, #0x1; strh w8, [x20+0x8]` low byte)
            0,     // pitch_family (raw same store, high byte)
            false, // is_panose_enabled (raw `strb wzr, [x20+0xa]`)
            &latin_panose,
        );
        // CHncStringW locals 의 Drop 은 Rust scope 종료 시 자동 (raw 의 D1 호출 대응).

        // ----- Block 2: cs TextFont (identical content)
        let cs_typeface = CHncStringW::from_str(HNC_GO_B_HINT_GS);
        let cs_panose = CHncStringW::from_str(EMPTY);
        let cs = TextFont::new_boxed(&cs_typeface, 1, 0, false, &cs_panose);

        // ----- Block 3: ea TextFont (empty typeface)
        let ea_typeface = CHncStringW::from_str(EMPTY);
        let ea_panose = CHncStringW::from_str(EMPTY);
        let ea = TextFont::new_boxed(&ea_typeface, 1, 0, false, &ea_panose);

        // ----- Final: FontSet::Create(latin, cs, ea) 으로 ownership 이양
        // raw 0x16a298: bl FontSet::Create (auto_ptr transfer)
        let fs_box = FontSet::new(latin, cs, ea);
        // raw 0x16a29c-a0: 결과를 sret 으로
        Box::into_raw(fs_box)
    }
}

impl Drop for FontScheme {
    /// raw `~FontScheme()` (`0x169e2c`) — 2 FontSet cleanup + name auto-drop.
    fn drop(&mut self) {
        unsafe {
            if !self.minor.is_null() {
                drop(Box::from_raw(self.minor));
                self.minor = ptr::null_mut();
            }
            if !self.major.is_null() {
                drop(Box::from_raw(self.major));
                self.major = ptr::null_mut();
            }
            // name 자동 drop (Rust 의 field-drop 순서: minor, major, name).
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_layout_size_align() {
        assert_eq!(std::mem::size_of::<FontScheme>(), 24);
        assert_eq!(std::mem::align_of::<FontScheme>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let fs = FontScheme::new_empty();
        let p = &fs as *const FontScheme as usize;
        assert_eq!(&fs.name as *const _ as usize - p, 0x00);
        assert_eq!(&fs.major as *const _ as usize - p, 0x08);
        assert_eq!(&fs.minor as *const _ as usize - p, 0x10);
    }

    #[test]
    fn new_empty_has_null_fontsets() {
        let fs = FontScheme::new_empty();
        assert!(fs.major.is_null());
        assert!(fs.minor.is_null());
        assert!(fs.get_major_font().is_null());
        assert!(fs.get_minor_font().is_null());
    }

    #[test]
    fn new_with_fontsets_transfers_ownership() {
        unsafe {
            use crate::text_font::TextFont;
            let make_tf = || {
                let typeface = CHncStringW::default();
                let panose = CHncStringW::default();
                TextFont::new_boxed(&typeface, 0, 0, false, &panose)
            };
            let major_raw = Box::into_raw(FontSet::new(make_tf(), make_tf(), make_tf()));
            let minor_raw = Box::into_raw(FontSet::new(make_tf(), make_tf(), make_tf()));
            let fs = FontScheme::new(major_raw, minor_raw);
            assert_eq!(fs.major, major_raw);
            assert_eq!(fs.minor, minor_raw);
            // drop fs → frees both
            drop(fs);
        }
    }

    #[test]
    fn drop_empty_no_panic() {
        for _ in 0..50 {
            let fs = FontScheme::new_empty();
            drop(fs);
        }
    }

    #[test]
    fn name_default_is_empty() {
        let fs = FontScheme::new_empty();
        assert_eq!(fs.get_name().length(), 0);
    }

    #[test]
    fn set_name_round_trip() {
        let mut fs = FontScheme::new_empty();
        let new_name = CHncStringW::from_str("OfficeFontScheme");
        fs.set_name(&new_name);
        assert!(fs.get_name().length() > 0);
    }

    // ----- L-5c-5b1: CreateDefaultFontSet (raw 0x16a144) byte-eq port tests

    #[test]
    fn create_default_font_set_returns_non_null() {
        unsafe {
            let fs = FontScheme::create_default_font_set();
            assert!(!fs.is_null());
            // cleanup
            drop(Box::from_raw(fs));
        }
    }

    #[test]
    fn create_default_font_set_latin_typeface_is_hnc_go_b_hint_gs() {
        unsafe {
            let fs = FontScheme::create_default_font_set();
            let latin = (*fs).get_latin();
            assert!(!latin.is_null(), "latin TextFont 가 alloc 됨");
            let s = String::from_utf16_lossy((*latin).get_typeface().as_wide());
            assert_eq!(s, "HNC_GO_B_HINT_GS",
                "raw 0x757c48 의 wide literal 1:1");
            drop(Box::from_raw(fs));
        }
    }

    #[test]
    fn create_default_font_set_cs_typeface_is_hnc_go_b_hint_gs() {
        unsafe {
            let fs = FontScheme::create_default_font_set();
            let cs = (*fs).get_complex_script();
            assert!(!cs.is_null());
            let s = String::from_utf16_lossy((*cs).get_typeface().as_wide());
            assert_eq!(s, "HNC_GO_B_HINT_GS",
                "raw block 2: cs 도 같은 string literal");
            drop(Box::from_raw(fs));
        }
    }

    #[test]
    fn create_default_font_set_ea_typeface_is_empty() {
        unsafe {
            let fs = FontScheme::create_default_font_set();
            let ea = (*fs).get_east_asian();
            assert!(!ea.is_null());
            let s = String::from_utf16_lossy((*ea).get_typeface().as_wide());
            assert_eq!(s, "", "raw block 3: ea typeface 가 empty literal");
            drop(Box::from_raw(fs));
        }
    }

    #[test]
    fn create_default_font_set_flag_bytes_match_raw() {
        // raw 0x16a190-198: charset=1, pitch_family=0, is_panose_enabled=false
        unsafe {
            let fs = FontScheme::create_default_font_set();
            let latin = (*fs).get_latin();
            assert_eq!((*latin).get_charset(), 1, "raw `mov w8, #0x1; strh w8` low byte");
            assert_eq!((*latin).get_pitch_family(), 0, "raw same store, high byte = 0");
            assert!(!(*latin).is_panose_enabled(), "raw `strb wzr, [x20+0xa]`");
            drop(Box::from_raw(fs));
        }
    }

    #[test]
    fn create_default_font_set_panose_is_empty_for_all_three() {
        unsafe {
            let fs = FontScheme::create_default_font_set();
            for f in [(*fs).get_latin(), (*fs).get_complex_script(), (*fs).get_east_asian()] {
                let s = String::from_utf16_lossy((*f).get_panose().as_wide());
                assert_eq!(s, "", "all 3 TextFont 의 panose 가 raw 0x75794c (= empty)");
            }
            drop(Box::from_raw(fs));
        }
    }
}
