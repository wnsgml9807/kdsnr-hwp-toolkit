//! `Hnc::Shape::Text::RunProperty` — per-run text properties container.
//!
//! Used by `CharItemView` to hold font/brush/pen/effects + property bag for
//! font-size, bold, italic, baseline, strike/underline style, language, etc.
//!
//! ## raw layout (80B, from C2 ctor @ `0x3324cc`)
//!
//! ```text
//! offset  field                   타입               의미
//! +0x00   brush                   SharePtr<Brush>    primary fill paint
//! +0x08   pen                     SharePtr<Pen>      primary stroke paint
//! +0x10   effects                 SharePtr<Effects>  shadow/glow/reflection
//! +0x18   underline_brush         SharePtr<Brush>    underline 색
//! +0x20   underline_pen           SharePtr<Pen>      underline stroke
//! +0x28   latin_font              SharePtr<TextFont> 라틴 문자용 폰트 (GetLatinFont)
//! +0x30   east_asian_font         SharePtr<TextFont> 한글/한자/일본어 폰트 (GetEastAsianFont)
//! +0x38   complex_script_font     SharePtr<TextFont> 아랍/태국/Devanagari 폰트 (GetComplexScriptFont)
//! +0x40   symbol_font             SharePtr<TextFont> 기호 폰트 (GetSymbolFont)
//! +0x48   property_bag            PropertyBag (8B)   font_size, bold, italic, etc.
//! ```
//!
//! 총 80B / 8B align.
//!
//! ## SharePtr 동작 (모든 +0x00..0x48 의 9 slot 공통)
//!
//! 각 slot 은 `SharePtr<T>` = 8B pointer to `ControlBlock<T>` (refcount + payload ptr).
//! - null SharePtr: slot ptr = `nullptr`
//! - 정상: ControlBlock 의 refcount++ / -- 로 lifetime 관리
//!
//! ## raw getter pattern (예: GetLatinFont @ `0x2f0f24`)
//!
//! ```text
//! ldr x9, [x0, #0x28]    ; x9 = this->latin_font (ControlBlock*)
//! str x9, [x8]            ; *out (SRET) = x9
//! cbz x9, ret             ; if null, just return
//! ldr x8, [x9]            ; x8 = ControlBlock->payload (TextFont*)
//! cbz x8, ret             ; if payload null, return without ref-bump
//! ldr x8, [x9, #0x8]      ; x8 = refcount
//! add x8, x8, #0x1
//! str x8, [x9, #0x8]      ; refcount++
//! b 0x679938              ; tail call to (probably SharePtr::AddRef helper)
//! ```
//!
//! 4 font getter 가 같은 패턴, offset 만 다름 (+0x28, +0x30, +0x38, +0x40).
//!
//! ## PropertyBag 키 (raw ctor 에서 등록 순서, partial — full list 후속 RE)
//!
//! - `0x964` (FontSize key): float, init to font_size arg
//! - `0x965`: float (related to font_size, maybe ScaleFactor)
//! - `0x967`: bool/enum (Bold? Italic?)
//! - `0x968`: bool/enum
//! - `0x969`: bool/enum
//! - `0x96A`: bool/enum
//! - `0x961`, `0x962`, `0x963`: language/script-related
//!
//! 본 port 의 현재 단계는 PropertyBag 만 보유 — 개별 키 별 getter 는 추후 RE.

use crate::property_bag::PropertyBag;

/// `Hnc::Shape::Text::RunProperty` — 80B 1:1 byte-eq layout.
///
/// 본 단계는 layout + font slot getter 만. ctor / dtor / PropertyBag key getter 는 후속.
#[repr(C)]
pub struct RunProperty {
    /// `+0x00` — primary brush SharePtr (= ControlBlock<Brush>*, nullable).
    pub brush: *mut (),
    /// `+0x08` — primary pen SharePtr.
    pub pen: *mut (),
    /// `+0x10` — effects SharePtr.
    pub effects: *mut (),
    /// `+0x18` — underline brush SharePtr.
    pub underline_brush: *mut (),
    /// `+0x20` — underline pen SharePtr.
    pub underline_pen: *mut (),
    /// `+0x28` — latin font SharePtr.
    pub latin_font: *mut (),
    /// `+0x30` — east asian font SharePtr.
    pub east_asian_font: *mut (),
    /// `+0x38` — complex script font SharePtr.
    pub complex_script_font: *mut (),
    /// `+0x40` — symbol font SharePtr.
    pub symbol_font: *mut (),
    /// `+0x48` — property bag (font size, bold, italic, etc.).
    pub property_bag: PropertyBag,
}

pub const RUN_PROPERTY_SIZE_BYTES: usize = 80;
pub const RUN_PROPERTY_ALIGN_BYTES: usize = 8;

const _: () = assert!(
    std::mem::size_of::<RunProperty>() == RUN_PROPERTY_SIZE_BYTES,
    "RunProperty size mismatch"
);

const _: () = assert!(
    std::mem::align_of::<RunProperty>() == RUN_PROPERTY_ALIGN_BYTES,
    "RunProperty align mismatch"
);

/// `RunProperty::FontSlot` — 4 font slot enum 매핑.
///
/// `text_real_font::script_to_slot` 결과와 1:1 매핑.
///
/// ## 매핑 출처 (GetRealFont @ `0x2f0234` asm 확인)
///
/// - bits 2-5 (`0x3c`) → slot **1 = EastAsian** (`[x22, #0x30]`, raw `0x2f0388`)
/// - bits 8,9,10,33 (`0x0000_0002_0000_0700`) → slot **2 = ComplexScript** (`[x22, #0x38]`, raw `0x2f02fc`)
/// - script == 32 → slot **3 = Symbol** (`[x22, #0x40]`, raw `0x2f04e4`)
/// - else → slot **0 = Latin** (`[x22, #0x28]`, raw `0x2f0560`)
///
/// **주의**: EastAsian (slot 1) 과 ComplexScript (slot 2) 의 슬롯 번호가
/// RunProperty 구조체 layout 순서 (Latin/EA/CS/Sym @ +0x28/0x30/0x38/0x40) 와
/// 동일. 즉 slot index = (field offset - 0x28) / 8.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSlot {
    /// slot 0 — Latin (`RunProperty+0x28`).
    Latin = 0,
    /// slot 1 — EastAsian (`RunProperty+0x30`).
    EastAsian = 1,
    /// slot 2 — ComplexScript (`RunProperty+0x38`).
    ComplexScript = 2,
    /// slot 3 — Symbol (`RunProperty+0x40`).
    Symbol = 3,
}

impl FontSlot {
    /// `script_to_slot` u32 결과를 enum 으로 변환. invalid 값은 None.
    pub fn from_slot_index(idx: u32) -> Option<Self> {
        match idx {
            0 => Some(FontSlot::Latin),
            1 => Some(FontSlot::EastAsian),
            2 => Some(FontSlot::ComplexScript),
            3 => Some(FontSlot::Symbol),
            _ => None,
        }
    }
}

impl RunProperty {
    /// raw `GetLatinFont() const` @ `0x2f0f24` (44B).
    ///
    /// 반환: latin_font SharePtr (raw `ControlBlock*`, nullable).
    /// raw 가 SRET 으로 ControlBlock 의 refcount++ 한 복사본 반환.
    /// 본 port 는 raw pointer 그대로 반환 — 호출자가 SharePtr 래핑 책임.
    pub fn get_latin_font(&self) -> *mut () {
        self.latin_font
    }

    /// raw `GetEastAsianFont() const` @ `0x2f0f4c` (44B).
    pub fn get_east_asian_font(&self) -> *mut () {
        self.east_asian_font
    }

    /// raw `GetComplexScriptFont() const` @ `0x2f0f74` (44B).
    pub fn get_complex_script_font(&self) -> *mut () {
        self.complex_script_font
    }

    /// raw `GetSymbolFont() const` @ `0x2f0f9c` (44B).
    pub fn get_symbol_font(&self) -> *mut () {
        self.symbol_font
    }

    /// slot index 기반 dispatch — `script_to_slot` 결과를 직접 받아 해당 폰트 반환.
    ///
    /// raw `GetRealFont` (`0x2f0234`) 내부의 dispatch 와 등가.
    /// slot 0 → Latin, 1 → EastAsian, 2 → ComplexScript, 3 → Symbol.
    pub fn get_font_for_slot(&self, slot: FontSlot) -> *mut () {
        match slot {
            FontSlot::Latin => self.latin_font,
            FontSlot::EastAsian => self.east_asian_font,
            FontSlot::ComplexScript => self.complex_script_font,
            FontSlot::Symbol => self.symbol_font,
        }
    }

    /// raw `RunProperty::GetFontSize() const` (`__ZNK3Hnc5Shape4Text11RunProperty11GetFontSizeEv`
    /// @ `0x2ecb18`, 116B) 1:1 byte-eq.
    ///
    /// ```asm
    /// 0x2ecb2c  mov  w8, #0x96a
    /// 0x2ecb30  str  w8, [sp]              ; PropertyKey local = 0x96a
    /// 0x2ecb34  str  xzr, [sp, #0x8]
    /// 0x2ecb38  ldr  x8, [x0, #0x48]       ; bag handle (SharePtr -> ControlBlock<Impl>)
    /// 0x2ecb3c  cbz  x8, 0x2ecb48
    /// 0x2ecb40  ldr  x0, [x8]              ; ctrl.obj = PropertyBagImpl*
    /// 0x2ecb44  b    0x2ecb4c
    /// 0x2ecb48  mov  x0, #0x0
    /// 0x2ecb4c  mov  x1, sp                ; &key
    /// 0x2ecb50  bl   0x65616c              ; = get_value_addr
    /// 0x2ecb54  ldr  s8, [x0]              ; *(f32*)(Property + 0xc)
    /// ```
    ///
    /// `PropertyKey 0x96a` = FontSize. PropertyBag 가 null 이면 raw 가 `mov x0, #0`
    /// 후 get_value_addr 호출 → 그 helper 는 panic (out_of_range "bag is null"). raw
    /// asm 에서는 cxa exception throw 후 caller unwind 처리.
    ///
    /// # Safety
    /// `self.property_bag` 의 underlying impl 가 valid 또는 null.
    pub unsafe fn get_font_size(&self) -> f32 {
        let pk = crate::property_key::PropertyKey::from_int(0x96a);
        // raw 0x2ecb38-0x2ecb4c: bag handle → ctrl.obj → impl ptr (or null on missing ctrl/obj)
        let impl_ptr: *const crate::property_bag::PropertyBagImpl = match self.property_bag.impl_ref() {
            Some(im) => im as *const _,
            None => std::ptr::null(),
        };
        let value_addr = crate::property_bag::PropertyBagImpl::get_value_addr(impl_ptr, &pk);
        *(value_addr as *const f32)
    }

    /// raw `RunProperty::GetScriptBaseLine() const`
    /// (`__ZNK3Hnc5Shape4Text11RunProperty17GetScriptBaseLineEv` @ `0x2f0074`, 116B)
    /// 1:1 byte-eq.
    ///
    /// `RunProperty::GetFontSize` 과 완전 동일 구조, PropertyKey 만 `0x96c` 로 다름.
    ///
    /// `0x96c` = ScriptBaseLine (= sub/superscript baseline shift ratio).
    ///
    /// # Safety
    /// `self.property_bag` 의 underlying impl 가 valid 또는 null.
    pub unsafe fn get_script_base_line(&self) -> f32 {
        let pk = crate::property_key::PropertyKey::from_int(0x96c);
        let impl_ptr: *const crate::property_bag::PropertyBagImpl = match self.property_bag.impl_ref() {
            Some(im) => im as *const _,
            None => std::ptr::null(),
        };
        let value_addr = crate::property_bag::PropertyBagImpl::get_value_addr(impl_ptr, &pk);
        *(value_addr as *const f32)
    }

    /// 본 단계 테스트용 — 9 slot 전부 null + 빈 PropertyBag 으로 초기화.
    /// raw default ctor 없음 (모든 ctor 가 args 받음) — 본 함수는 test/scaffolding 전용.
    pub fn new_empty_for_test() -> Self {
        Self {
            brush: std::ptr::null_mut(),
            pen: std::ptr::null_mut(),
            effects: std::ptr::null_mut(),
            underline_brush: std::ptr::null_mut(),
            underline_pen: std::ptr::null_mut(),
            latin_font: std::ptr::null_mut(),
            east_asian_font: std::ptr::null_mut(),
            complex_script_font: std::ptr::null_mut(),
            symbol_font: std::ptr::null_mut(),
            property_bag: PropertyBag::new(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<RunProperty>(), 80);
        assert_eq!(std::mem::align_of::<RunProperty>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let rp = RunProperty::new_empty_for_test();
        let base = &rp as *const _ as usize;
        assert_eq!(&rp.brush as *const _ as usize - base, 0x00);
        assert_eq!(&rp.pen as *const _ as usize - base, 0x08);
        assert_eq!(&rp.effects as *const _ as usize - base, 0x10);
        assert_eq!(&rp.underline_brush as *const _ as usize - base, 0x18);
        assert_eq!(&rp.underline_pen as *const _ as usize - base, 0x20);
        assert_eq!(&rp.latin_font as *const _ as usize - base, 0x28);
        assert_eq!(&rp.east_asian_font as *const _ as usize - base, 0x30);
        assert_eq!(&rp.complex_script_font as *const _ as usize - base, 0x38);
        assert_eq!(&rp.symbol_font as *const _ as usize - base, 0x40);
        assert_eq!(&rp.property_bag as *const _ as usize - base, 0x48);
    }

    #[test]
    fn empty_runproperty_has_null_fonts() {
        let rp = RunProperty::new_empty_for_test();
        assert!(rp.get_latin_font().is_null());
        assert!(rp.get_east_asian_font().is_null());
        assert!(rp.get_complex_script_font().is_null());
        assert!(rp.get_symbol_font().is_null());
    }

    #[test]
    fn font_slot_dispatch() {
        let mut rp = RunProperty::new_empty_for_test();
        // Use sentinel ptr values to verify dispatch routes correctly
        let p_latin = 0x1000usize as *mut ();
        let p_cs = 0x2000usize as *mut ();
        let p_ea = 0x3000usize as *mut ();
        let p_sym = 0x4000usize as *mut ();
        rp.latin_font = p_latin;
        rp.complex_script_font = p_cs;
        rp.east_asian_font = p_ea;
        rp.symbol_font = p_sym;

        assert_eq!(rp.get_font_for_slot(FontSlot::Latin), p_latin);
        assert_eq!(rp.get_font_for_slot(FontSlot::ComplexScript), p_cs);
        assert_eq!(rp.get_font_for_slot(FontSlot::EastAsian), p_ea);
        assert_eq!(rp.get_font_for_slot(FontSlot::Symbol), p_sym);
    }

    #[test]
    fn font_slot_from_index() {
        // raw GetRealFont asm 매핑: 0=Latin, 1=EastAsian, 2=ComplexScript, 3=Symbol
        assert_eq!(FontSlot::from_slot_index(0), Some(FontSlot::Latin));
        assert_eq!(FontSlot::from_slot_index(1), Some(FontSlot::EastAsian));
        assert_eq!(FontSlot::from_slot_index(2), Some(FontSlot::ComplexScript));
        assert_eq!(FontSlot::from_slot_index(3), Some(FontSlot::Symbol));
        assert_eq!(FontSlot::from_slot_index(4), None);
        assert_eq!(FontSlot::from_slot_index(0xFFFF_FFFF), None);
    }

    #[test]
    fn get_font_size_reads_key_0x96a_from_bag() {
        unsafe {
            let mut rp = RunProperty::new_empty_for_test();
            // Build attach with key 0x96a, value 12.5 (FontSize)
            let pk = crate::property_key::PropertyKey::from_int(0x96a);
            let ctrl = crate::property::PFloat::create_attach_ctrl(
                crate::property::state::ENABLED_DEFAULT,
                12.5,
            );
            let _ = rp.property_bag.attach(&pk, ctrl);
            assert_eq!(rp.get_font_size(), 12.5);
        }
    }

    #[test]
    fn get_script_base_line_reads_key_0x96c_from_bag() {
        unsafe {
            let mut rp = RunProperty::new_empty_for_test();
            let pk = crate::property_key::PropertyKey::from_int(0x96c);
            let ctrl = crate::property::PFloat::create_attach_ctrl(
                crate::property::state::ENABLED_DEFAULT,
                0.33,
            );
            let _ = rp.property_bag.attach(&pk, ctrl);
            assert_eq!(rp.get_script_base_line(), 0.33);
        }
    }

    #[test]
    fn script_to_slot_integration() {
        use crate::text_real_font::script_to_slot;
        // script class 3 (Korean Hangul default) → slot 1 (EastAsian) — raw 0x2f0388 reads +0x30
        let slot_idx = script_to_slot(3);
        let slot = FontSlot::from_slot_index(slot_idx).unwrap();
        assert_eq!(slot, FontSlot::EastAsian);

        // script class 8 (Hebrew etc) → slot 2 (ComplexScript) — raw 0x2f02fc reads +0x38
        let slot_idx = script_to_slot(8);
        let slot = FontSlot::from_slot_index(slot_idx).unwrap();
        assert_eq!(slot, FontSlot::ComplexScript);

        // script class 0x20 (32, Symbol) → slot 3 (Symbol) — raw 0x2f04e4 reads +0x40
        let slot_idx = script_to_slot(0x20);
        let slot = FontSlot::from_slot_index(slot_idx).unwrap();
        assert_eq!(slot, FontSlot::Symbol);

        // script class 0 → slot 0 (Latin fallback) — raw 0x2f0560 reads +0x28
        let slot_idx = script_to_slot(0);
        let slot = FontSlot::from_slot_index(slot_idx).unwrap();
        assert_eq!(slot, FontSlot::Latin);
    }
}
