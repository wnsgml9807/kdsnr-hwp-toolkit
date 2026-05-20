//! Runtime singletons + helper classes — Phase B-8a foundational types.
//!
//! `Hnc::Shape::*` 의 layout engine 이 의존하는 global / context singleton 들.
//! `PptCompositor::ComposeLayout` 와 `CharItemView` 의 constructor 에서 자주 호출.
//!
//! ## 2026-05-15 14번째 세션 종료 기준 — 핵심 type 모두 1:1 포팅 완료
//!
//! - `ShapeEngine` (singleton): `get_logical_dpi()` 등 RE 검증.
//! - `RunProperty` + `Font` + `FontTable` + `Theme`: `CharItemView::GetRealFont` 가 의존하는
//!   property bag + 4-slot 폰트 테이블 + theme fallback. raw `FUN_002f0234` (992B) 1:1.
//! - `realize_font(char, run_property, theme) -> Option<RealFontMeta>`: `GetRealFont` 의
//!   layout 영향분 1:1. fallback face `"HCR Dotum"` (raw `REAL_FONT_FALLBACK_NAME` doc 참조).

// ============================================================
// ShapeEngine — global singleton with dpi/scale info
// ============================================================

/// `Hnc::Shape::ShapeEngine` — global singleton.
///
/// `ShapeEngine::GetInstance()` (0x1de540) 는 C++11 magic static pattern:
/// - guard at `0x79f578` (acquire/release 인 atomic byte)
/// - instance ptr at `0x79f6b8`
/// - 처음 호출 시 `cxa_guard_acquire` → constructor (`ShapeEngine::ShapeEngine` @0x1de250)
///   → `cxa_guard_release`. 이후엔 fast-path 로 ptr 만 반환.
///
/// 한컴 원본 layout (0x28+ bytes):
/// - `+0x00`: `bool started` (Start() 호출 후 1) — 1 byte
/// - `+0x04`: `float logical_dpi` (GetLogicalDpi 반환값). Layout 코드에서 가장 빈번한 access:
///   `(val * *(float *)(GetInstance() + 4)) / 72.0` — pt→px 변환.
/// - `+0x08`: `SharePtr<Theme>` (default theme)
/// - `+0x10`: `Catalog*`
/// - `+0x18..+0x40`: `CHncStringW` path
/// - `+0x20`: `bool enable_x_box` (default 1)
/// - `+0x24`: `float resolution` (default 1.0, `0x3f800000`)
///
/// **Rust 포팅**: thread-local `RefCell<Option<ShapeEngine>>`. `get_instance()` 호출 시
/// lazy init (한컴 magic static 과 동등). `start(dpi)` 가 호출 안 되면 96.0 default.
#[derive(Debug, Clone, Copy)]
pub struct ShapeEngine {
    /// `+0x00` — Start() 가 호출 됐는지 flag.
    pub started: bool,
    /// `+0x04` — logical DPI. Layout 의 pt→px 변환 핵심 값.
    pub logical_dpi: f32,
    /// `+0x20` — XBox 지원 enable.
    pub enable_x_box: bool,
    /// `+0x24` — resolution scaler. `0x3f800000` (=1.0) default.
    pub resolution: f32,
}

impl Default for ShapeEngine {
    fn default() -> Self {
        // 한컴 constructor (0x1de250): started=false, +4 미설정 (=0).
        // Start() 가 +4 를 set. layout 호출 전 Start() 가 항상 선행되므로 0 으론 안 떨어짐.
        // Rust 의 fallback default: 96 (screen at 1x). prod 에선 `start(dpi)` 호출 필수.
        Self {
            started: false,
            logical_dpi: 96.0,
            enable_x_box: true,
            resolution: 1.0,
        }
    }
}

thread_local! {
    /// `Hnc::Shape::ShapeEngine` singleton 의 thread-local 인스턴스.
    /// 한컴 magic static 1:1.
    static SHAPE_ENGINE: std::cell::RefCell<ShapeEngine> = std::cell::RefCell::new(ShapeEngine::default());
}

impl ShapeEngine {
    /// 한컴 `ShapeEngine::GetInstance()` 1:1.
    ///
    /// 한컴 C++11 magic static (guard at 0x79f578, ptr at 0x79f6b8). Rust 모델은
    /// thread-local `RefCell` — 첫 호출 시 default 로 init.
    pub fn get_instance() -> Self {
        SHAPE_ENGINE.with(|s| *s.borrow())
    }

    /// 한컴 `ShapeEngine::Start(SharePtr<FormsLoader>, float dpi, ...)` 의 핵심 부분.
    ///
    /// 1:1 으로는:
    /// - `*(float *)(this + 4) = param_2;`  // logical_dpi
    /// - `*this = 1;`                       // started = true
    /// - 그 외 Theme/Catalog/Path init (layout 에 직접 영향 없음)
    pub fn start(dpi: f32) {
        SHAPE_ENGINE.with(|s| {
            let mut inst = s.borrow_mut();
            inst.logical_dpi = dpi;
            inst.started = true;
        });
    }

    /// 한컴 `ShapeEngine::SetResolution(float)` @0x1de538.
    pub fn set_resolution(resolution: f32) {
        SHAPE_ENGINE.with(|s| s.borrow_mut().resolution = resolution);
    }

    /// 한컴 `ShapeEngine::GetResolution()` @0xf7f1c.
    pub fn get_resolution() -> f32 {
        Self::get_instance().resolution
    }

    /// 한컴 `ShapeEngine::GetLogicalDpi()` @0x18c0a4.
    pub fn get_logical_dpi() -> f32 {
        Self::get_instance().logical_dpi
    }

    /// 한컴 `ShapeEngine::IsStarted()` @0x1de4f8.
    pub fn is_started() -> bool {
        Self::get_instance().started
    }

    /// 한컴 패턴 `(val * GetInstance()[+4]) / 72.0` (pt→px).
    pub fn pt_to_pixels(pt: f32) -> f32 {
        (pt * Self::get_instance().logical_dpi) / 72.0
    }

    /// 테스트 리셋용 (thread-local 만 영향).
    #[doc(hidden)]
    pub fn _reset_for_test() {
        SHAPE_ENGINE.with(|s| *s.borrow_mut() = ShapeEngine::default());
    }

    // ─── Compatibility alias (기존 호출자 호환) ───
    /// `dpi_scale` alias — `logical_dpi` 와 동일.
    #[deprecated(note = "use `logical_dpi` directly")]
    pub fn dpi_scale_compat(&self) -> f32 { self.logical_dpi }
}

// ============================================================
// Font / FontTable / Theme — RunProperty 의 4-slot 폰트 테이블 + Theme 컨텍스트
// ============================================================

/// `Hnc::Shape::Text::Font` — RunProperty 의 face-name 슬롯에 들어가는 단일 폰트 객체.
///
/// `CharItemView::GetRealFont` (`0x2f0234`) 가 반환한 SharePtr<Font> 의 face name 이
/// `CharItemView` ctor 의 `pfVar3+2` (56B realfont meta 의 CHncStringW) 에 `Assign` 됨.
/// 본 모델은 face name 만 보존 (다른 필드는 layout 에 영향 없음).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Font {
    /// raw `*(Font + ?offset?)` 의 `CHncStringW` face name. ex. `"HCR Dotum"`, `"Arial"`.
    pub face_name: String,
}

impl Font {
    pub fn new<S: Into<String>>(face_name: S) -> Self {
        Self {
            face_name: face_name.into(),
        }
    }
}

/// `Hnc::Shape::Text::RunProperty` 의 4-slot 폰트 테이블 — raw `RunProperty +0x28..+0x48`.
///
/// raw `GetRealFont` (`0x2f0234`) 가 `char_class` (= [`crate::glyph::char_item_view_char_class`])
/// 으로 분기:
/// - class ∈ {2,3,4,5} → `cjk` 슬롯 (`RunProperty +0x30`)
/// - class ∈ {8,9,10,33} → `script` 슬롯 (`RunProperty +0x38`)
/// - class == 32 → `symbol` 슬롯 (`RunProperty +0x40`)
/// - class ≥ 34 OR else → `latin` 슬롯 (`RunProperty +0x28`)
///
/// 각 슬롯 SharePtr 이 null 이거나 가리키는 obj 가 null 이면 → `FontScheme` substitution 또는
/// hardcoded `"HCR Dotum"` fallback (raw decompile line 25, 36).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontTable {
    /// raw `+0x28` — Latin / default 폰트.
    pub latin: Option<Font>,
    /// raw `+0x30` — CJK (한국·중국·일본) 폰트.
    pub cjk: Option<Font>,
    /// raw `+0x38` — script (Arabic/Hebrew/Thai 등) 폰트.
    pub script: Option<Font>,
    /// raw `+0x40` — symbol (= 특수문자) 폰트.
    pub symbol: Option<Font>,
}

/// raw `GetRealFont` 의 char-class → font slot 분기 1:1 포팅.
///
/// raw decompile lines 39-134 of `0x2f0234`:
/// ```text
/// uVar3 = FUN_002f0ad0(char_code);                  // == char_item_view_char_class
/// if ((uint)uVar3 < 0x22) {
///   if ((1L << (uVar3 & 0x3f) & 0x3cU)        != 0) → +0x30 (CJK)
///   else if ((1L << (uVar3 & 0x3f) & 0x200000700U) != 0) → +0x38 (script)
///   else if ((uVar3 & 0xffffffff) == 0x20)               → +0x40 (symbol)
///   else                                                  → fallthrough to +0x28 (Latin)
/// }
/// else                                                    → +0x28 (Latin)
/// ```
///
/// 마스크 `0x3c = bits {2,3,4,5}`. 마스크 `0x200000700 = bits {8,9,10,33}` (33 = 0x21, but
/// loop guard is `< 0x22` = `< 34`, so 33 is included).
pub fn select_font_slot(char_class: u32) -> FontSlot {
    if char_class >= 0x22 {
        FontSlot::Latin
    } else if (1u64 << (char_class & 0x3f)) & 0x3c != 0 {
        FontSlot::Cjk
    } else if (1u64 << (char_class & 0x3f)) & 0x200000700u64 != 0 {
        FontSlot::Script
    } else if char_class == 0x20 {
        FontSlot::Symbol
    } else {
        FontSlot::Latin
    }
}

/// `select_font_slot` 의 출력. `FontTable` 의 4 슬롯 중 어느 것을 사용할지 식별.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlot {
    Latin,
    Cjk,
    Script,
    Symbol,
}

impl FontSlot {
    /// `FontTable` 의 해당 슬롯 참조.
    pub fn pick<'a>(self, table: &'a FontTable) -> Option<&'a Font> {
        match self {
            FontSlot::Latin => table.latin.as_ref(),
            FontSlot::Cjk => table.cjk.as_ref(),
            FontSlot::Script => table.script.as_ref(),
            FontSlot::Symbol => table.symbol.as_ref(),
        }
    }
}

/// `Hnc::Shape::Theme` — 폰트 scheme / color scheme 보유.
///
/// `CharItemView::GetRealFont` (`0x2f0234`) 가 `Theme::GetFontScheme(true)` 으로 가져온
/// FontScheme 을 `FUN_002f10bc` 에 넘겨 폰트 substitution (예: 비어 있거나 unknown 폰트를
/// theme major/minor font 로 치환). 본 모델은 substitution 을 단순화하여 fallback face
/// name `"HCR Dotum"` 만 보존 — substitution 정책은 후속 RE 에서 확장.
#[derive(Debug, Clone, Default)]
pub struct Theme {
    /// `FontScheme.major_latin` 후보 face name (theme 의 heading 폰트). 비어 있으면 미사용.
    pub major_latin: Option<String>,
    /// `FontScheme.minor_latin` 후보 face name (theme 의 body 폰트). 비어 있으면 미사용.
    pub minor_latin: Option<String>,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }
}

/// raw `GetRealFont` (`0x2f0234`) 의 fallback 폰트 face name.
///
/// raw decompile line 25: `CHncStringW::CHncStringW(aCStack_38, L"HCR Dotum")`.
/// 모든 분기 (RunProperty 없음 / 슬롯 비어 있음) 의 default.
pub const REAL_FONT_FALLBACK_NAME: &str = "HCR Dotum";

/// `CharItemView::GetRealFont` (`0x2f0234`, 992B) 의 layout 영향분 1:1 포팅.
///
/// 입력:
/// - `char_code`: 측정 대상 UTF-16 code unit (`this+0x08`).
/// - `run_property`: `*(this + 0x18)` — `Some(rp)` 이면 `rp.font_table` 슬롯 dispatch.
///   `None` 이면 raw line 34-37 의 "no RunProperty" path → output empty.
/// - `_theme`: substitution 시 사용 (현 모델은 fallback `"HCR Dotum"` 만).
///
/// 출력 face name 결정 순서 (raw 동일):
/// 1. `select_font_slot(char_class)` 으로 후보 슬롯 선택,
/// 2. 슬롯 SharePtr 이 `Some` 이면 해당 `Font::face_name`,
/// 3. 없거나 `FUN_002f0fc4()` 가 비-zero (=substitution 요청) 이면 `Theme::major/minor_latin`,
/// 4. 그래도 없으면 `REAL_FONT_FALLBACK_NAME` (`"HCR Dotum"`).
///
/// raw decompile line 167-198 의 `FUN_002f0fc4` 분기 (substitution 트리거)는 현 모델에서
/// `FontTable` 슬롯이 비어 있을 때만 active — `Some(font)` 면 substitution skip. 후속 RE 에서
/// FontScheme 의 trigger 조건 확장 가능.
pub fn realize_font(char_code: u16, run_property: Option<&RunProperty>, theme: &Theme) -> Option<RealFontMeta> {
    // raw line 34-37: RunProperty 없으면 output empty (= None).
    let run = run_property?;
    let table = run.font_table.as_ref()?;

    // raw line 39: char_class = FUN_002f0ad0(char_code).
    let char_class = crate::glyph::char_item_view_char_class(char_code) as u32;
    // raw line 40-165: char_class → 4 slot 중 하나 dispatch.
    let slot = select_font_slot(char_class);

    let face_name = if let Some(font) = slot.pick(table) {
        // raw line 172: 슬롯 valid → 그 face name 사용.
        font.face_name.clone()
    } else {
        // raw line 167-198: 슬롯 invalid → FontScheme substitution. 현 모델은 theme.major →
        // theme.minor → "HCR Dotum" fallback 순.
        theme
            .major_latin
            .clone()
            .or_else(|| theme.minor_latin.clone())
            .unwrap_or_else(|| REAL_FONT_FALLBACK_NAME.to_string())
    };

    Some(RealFontMeta { face_name })
}

/// `GetRealFont` 출력 — 56B realfont meta (`pfVar3`) 의 layout-affecting 부분.
///
/// raw `CharItemView::CharItemView` line 3585-3603:
/// ```text
/// pfVar3 = operator_new(0x38);                  // 56 bytes
/// *pfVar3 = size_px;                            // [0] size in pixels
/// pfVar3[1] = style_flag;                       // [1] bold/italic 비트 플래그
/// CHncStringW::CHncStringW(pfVar3 + 2);         // [2..5] empty CHncStringW
/// pfVar3[4..13] = 0;
/// CHncStringW::Assign(pfVar3 + 2);              // [2..5] := GetRealFont 의 face name
/// ```
///
/// `size_px` / `style_flag` 는 RunProperty 에서 derive 되므로 `RealFontMeta` 는 face name 만
/// 보존. ctor 가 직접 두 값을 합쳐 realfont 구조를 만들고 `FUN_000764fc` 의 첫 param 으로 넘김.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealFontMeta {
    /// raw `pfVar3[2..5]` 의 `CHncStringW` 내용 — `FUN_000761f4` (global metric) 가 face name
    /// 으로 사용.
    pub face_name: String,
}

// ============================================================
// RunProperty — run-level character property
// ============================================================

/// `Hnc::Shape::Text::RunProperty` — character run 의 속성 (font, size, style 등).
///
/// CharItemView 의 `+0x18` 필드 (SharePtr<RunProperty>) 가 가리킴.
/// 한컴 원본은 PropertyBag 기반: instance `+0x48` slot 이 PropertyBag 포인터.
/// 한컴 `FUN_0065616c(propBag, &key)` = `bag.GetFloat(&key)`,
///       `FUN_00662d4c(propBag, &key)` = `bag.GetChar(&key)`.
///
/// `CharItemView::CharItemView` (constructor `FUN_002ef798`) 의 line 94-167 가 RunProperty
/// 의 5개 키를 조회:
/// - `0x96a` (`FONT_SIZE`): float, default 10.0
/// - `0x96c` (`FONT_SIZE_ADJUST`): float, 0이 아니면 size = (size*2)/3
/// - `0x967` (`BOLD_FLAG`): char (0 또는 1)
/// - `0x968` (`ITALIC_FLAG`): char (0 또는 1)
/// - `0x96b` (`METRIC_96B`): float, dpi/72 변환되어 fVar14 (offset) 로 사용
///
/// 모든 메소드는 한컴 PropertyBag.Get* 호출의 1:1 wrapper.
#[derive(Debug, Clone, Default)]
pub struct RunProperty {
    /// 한컴 `+0x48` PropertyBag.
    pub bag: crate::properties::HashMapPropertyBag,

    /// 한컴 `+0x28..+0x48` 의 4-slot 폰트 테이블. `CharItemView::GetRealFont` 가 char-class
    /// 으로 슬롯을 골라 face name 을 추출 (`Latin/CJK/script/symbol`).
    ///
    /// `None` → "RunProperty 가 font 슬롯을 갖지 않음" — raw line 34-37 (GetRealFont) 에서
    /// 본문 dispatch 를 skip 하고 fallback 으로 빠지는 path 와 동등 (output `RealFontMeta` 가
    /// `None` 으로 떨어져 CharItemView ctor 가 측정 skip).
    pub font_table: Option<FontTable>,
}

impl RunProperty {
    /// 빈 RunProperty (모든 키 default).
    pub fn empty() -> Self {
        Self {
            bag: crate::properties::HashMapPropertyBag::new(),
            font_table: None,
        }
    }

    /// `font_size` 만 set 된 RunProperty. 테스트 편의용.
    pub fn new(font_size: f32) -> Self {
        let bag = crate::properties::HashMapPropertyBag::new()
            .with(crate::properties::keys::FONT_SIZE,
                  crate::properties::PropertyValue::Float(font_size));
        Self { bag, font_table: None }
    }

    /// `bag` 을 직접 set.
    pub fn from_bag(bag: crate::properties::HashMapPropertyBag) -> Self {
        Self { bag, font_table: None }
    }

    /// `font_table` 을 함께 set 한 RunProperty.
    pub fn with_font_table(mut self, table: FontTable) -> Self {
        self.font_table = Some(table);
        self
    }

    /// 한컴 line 104: `FUN_0065616c(propBag, &local_c0={0x96a, 0})` → 첫 float.
    /// 한컴 fallback: `if (size <= 0) size = 10.0`.
    pub fn get_font_size(&self) -> f32 {
        use crate::properties::PropertyBag;
        let raw = self.bag.get_float(crate::properties::keys::FONT_SIZE).unwrap_or(10.0);
        if raw > 0.0 { raw } else { 10.0 }
    }

    /// 한컴 line 117: `FUN_0065616c(propBag, &local_c0={0x96c, 0})`.
    pub fn get_font_size_adjust(&self) -> f32 {
        use crate::properties::PropertyBag;
        self.bag.get_float(crate::properties::keys::FONT_SIZE_ADJUST).unwrap_or(0.0)
    }

    /// 한컴 line 131: `FUN_00662d4c(propBag, &local_c0={0x967, 0})` (`cVar1`).
    pub fn get_bold(&self) -> bool {
        use crate::properties::PropertyBag;
        self.bag.get_char(crate::properties::keys::BOLD_FLAG).unwrap_or(0) != 0
    }

    /// 한컴 line 142: `FUN_00662d4c(propBag, &local_c0={0x968, 0})` (`*pcVar4`).
    pub fn get_italic(&self) -> bool {
        use crate::properties::PropertyBag;
        self.bag.get_char(crate::properties::keys::ITALIC_FLAG).unwrap_or(0) != 0
    }

    /// 한컴 line 163: `FUN_0065616c(propBag, &local_c0={0x96b, 0})`.
    pub fn get_metric_96b(&self) -> f32 {
        use crate::properties::PropertyBag;
        self.bag.get_float(crate::properties::keys::METRIC_96B).unwrap_or(0.0)
    }

    /// 한컴 constructor line 88-122 의 effective font size 1:1.
    /// `(font_size_adjust != 0)` 이면 `(size * 2) / 3`.
    pub fn effective_font_size(&self) -> f32 {
        let base = self.get_font_size();
        if self.get_font_size_adjust() != 0.0 {
            (base + base) / 3.0
        } else {
            base
        }
    }

    /// 한컴 line 167: `fVar14 = (metric_96b * GetInstance()[+4]) / 72.0`.
    /// CharItemView::compute_metrics 의 `font_metric_offset` argument 도출에 사용.
    pub fn font_metric_offset_px(&self) -> f32 {
        let m96b = self.get_metric_96b();
        let dpi = ShapeEngine::get_instance().logical_dpi;
        (m96b * dpi) / 72.0
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_engine_default_dpi() {
        let se = ShapeEngine::default();
        assert_eq!(se.logical_dpi, 96.0);
    }

    #[test]
    fn shape_engine_default_started_false() {
        let se = ShapeEngine::default();
        assert!(!se.started);
        assert!(se.enable_x_box);
        assert_eq!(se.resolution, 1.0);
    }

    #[test]
    fn shape_engine_start_sets_dpi_and_started() {
        ShapeEngine::_reset_for_test();
        ShapeEngine::start(120.0);
        let se = ShapeEngine::get_instance();
        assert!(se.started);
        assert_eq!(se.logical_dpi, 120.0);
        assert!(ShapeEngine::is_started());
        assert_eq!(ShapeEngine::get_logical_dpi(), 120.0);
        // Reset for other tests
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn shape_engine_set_resolution() {
        ShapeEngine::_reset_for_test();
        assert_eq!(ShapeEngine::get_resolution(), 1.0);
        ShapeEngine::set_resolution(2.0);
        assert_eq!(ShapeEngine::get_resolution(), 2.0);
        ShapeEngine::_reset_for_test();
    }

    #[test]
    fn pt_to_pixels_conversion() {
        ShapeEngine::_reset_for_test();
        // 10pt at 96 DPI → 10 * 96 / 72 = 13.333...
        let px = ShapeEngine::pt_to_pixels(10.0);
        assert!((px - 13.333333).abs() < 0.001);
    }

    #[test]
    fn run_property_font_size() {
        let rp = RunProperty::new(12.0);
        assert_eq!(rp.get_font_size(), 12.0);
    }

    #[test]
    fn run_property_empty_bag_returns_default_font_size() {
        let rp = RunProperty::empty();
        // No value in bag → fallback to 10.0
        assert_eq!(rp.get_font_size(), 10.0);
    }

    #[test]
    fn run_property_zero_or_negative_size_falls_back_to_10() {
        use crate::properties::{keys, HashMapPropertyBag, PropertyValue};
        let bag = HashMapPropertyBag::new()
            .with(keys::FONT_SIZE, PropertyValue::Float(0.0));
        let rp = RunProperty::from_bag(bag);
        assert_eq!(rp.get_font_size(), 10.0);

        let bag2 = HashMapPropertyBag::new()
            .with(keys::FONT_SIZE, PropertyValue::Float(-5.0));
        let rp2 = RunProperty::from_bag(bag2);
        assert_eq!(rp2.get_font_size(), 10.0);
    }

    #[test]
    fn run_property_bold_italic() {
        use crate::properties::{keys, HashMapPropertyBag, PropertyValue};
        let bag = HashMapPropertyBag::new()
            .with(keys::BOLD_FLAG, PropertyValue::Char(1))
            .with(keys::ITALIC_FLAG, PropertyValue::Char(0));
        let rp = RunProperty::from_bag(bag);
        assert!(rp.get_bold());
        assert!(!rp.get_italic());
    }

    #[test]
    fn run_property_effective_font_size_with_adjust() {
        use crate::properties::{keys, HashMapPropertyBag, PropertyValue};
        // adjust != 0 → (size * 2) / 3
        let bag = HashMapPropertyBag::new()
            .with(keys::FONT_SIZE, PropertyValue::Float(12.0))
            .with(keys::FONT_SIZE_ADJUST, PropertyValue::Float(0.5));
        let rp = RunProperty::from_bag(bag);
        let eff = rp.effective_font_size();
        assert!((eff - 8.0).abs() < 1e-6);

        // adjust == 0 → raw
        let bag2 = HashMapPropertyBag::new()
            .with(keys::FONT_SIZE, PropertyValue::Float(14.0));
        let rp2 = RunProperty::from_bag(bag2);
        assert_eq!(rp2.effective_font_size(), 14.0);
    }

    #[test]
    fn run_property_font_metric_offset_px() {
        use crate::properties::{keys, HashMapPropertyBag, PropertyValue};
        let bag = HashMapPropertyBag::new()
            .with(keys::METRIC_96B, PropertyValue::Float(3.0));
        let rp = RunProperty::from_bag(bag);
        // metric_96b=3 * 96 / 72 = 4.0
        assert!((rp.font_metric_offset_px() - 4.0).abs() < 1e-6);
    }

    // ============================================================
    // select_font_slot / realize_font — GetRealFont 1:1 포팅 테스트
    // ============================================================

    #[test]
    fn select_font_slot_latin_default_for_class_zero_and_high() {
        // raw line 144-165 (LAB_002f0560): class ≥ 0x22 OR fallthrough else → Latin.
        assert_eq!(select_font_slot(0), FontSlot::Latin);
        assert_eq!(select_font_slot(1), FontSlot::Latin);
        // class 33 (0x21) IS in script mask 0x200000700 (bit 33) — Script, not Latin.
        // See `select_font_slot_script_for_classes_8_9_10_33` 테스트.
        assert_eq!(select_font_slot(0x22), FontSlot::Latin);   // 34 → ≥ 0x22 → Latin
        assert_eq!(select_font_slot(100), FontSlot::Latin);
    }

    #[test]
    fn select_font_slot_cjk_for_classes_2_3_4_5() {
        // raw line 41-103: mask 0x3c = bits {2,3,4,5} → CJK (+0x30).
        for class in [2, 3, 4, 5] {
            assert_eq!(select_font_slot(class), FontSlot::Cjk, "class={}", class);
        }
        // class 6, 7 NOT in mask → fallthrough → Latin.
        assert_eq!(select_font_slot(6), FontSlot::Latin);
        assert_eq!(select_font_slot(7), FontSlot::Latin);
    }

    #[test]
    fn select_font_slot_script_for_classes_8_9_10_33() {
        // raw line 73-102: mask 0x200000700 = bits {8,9,10,33} → Script (+0x38).
        for class in [8, 9, 10, 33] {
            assert_eq!(select_font_slot(class), FontSlot::Script, "class={}", class);
        }
        // class 11 NOT in mask → fallthrough.
        assert_eq!(select_font_slot(11), FontSlot::Latin);
    }

    #[test]
    fn select_font_slot_symbol_for_class_32() {
        // raw line 43: (uVar3 & 0xffffffff) != 0x20 → not symbol. So class == 32 → Symbol.
        assert_eq!(select_font_slot(32), FontSlot::Symbol);
        assert_eq!(select_font_slot(31), FontSlot::Latin);
    }

    fn run_with_table(table: FontTable) -> RunProperty {
        RunProperty::empty().with_font_table(table)
    }

    #[test]
    fn realize_font_returns_none_when_no_run_property() {
        // raw line 34-37: RunProperty 없으면 output empty.
        let theme = Theme::new();
        let result = realize_font(b'A' as u16, None, &theme);
        assert!(result.is_none());
    }

    #[test]
    fn realize_font_returns_none_when_no_font_table() {
        let theme = Theme::new();
        let rp = RunProperty::empty();
        let result = realize_font(b'A' as u16, Some(&rp), &theme);
        // 본 모델 결정: font_table=None → None. 후속 GetRealFont fallback 은 ctor 쪽에서
        // 별도 default 처리.
        assert!(result.is_none());
    }

    #[test]
    fn realize_font_picks_latin_slot_for_ascii() {
        let table = FontTable {
            latin: Some(Font::new("Arial")),
            cjk: Some(Font::new("Malgun Gothic")),
            script: None,
            symbol: None,
        };
        let rp = run_with_table(table);
        let theme = Theme::new();
        // 'A' (0x41) → char_class != {2,3,4,5,8,9,10,33,32} → Latin slot.
        let result = realize_font(b'A' as u16, Some(&rp), &theme).unwrap();
        assert_eq!(result.face_name, "Arial");
    }

    #[test]
    fn realize_font_picks_cjk_slot_for_hangul() {
        let table = FontTable {
            latin: Some(Font::new("Arial")),
            cjk: Some(Font::new("Malgun Gothic")),
            script: None,
            symbol: None,
        };
        let rp = run_with_table(table);
        let theme = Theme::new();
        // '한' (0xD55C) — Hangul syllable. char_class = 3 (per glyph.rs). → CJK slot.
        let result = realize_font(0xD55C, Some(&rp), &theme).unwrap();
        assert_eq!(result.face_name, "Malgun Gothic");
    }

    #[test]
    fn realize_font_falls_back_to_theme_when_slot_empty() {
        // raw line 167-198: 슬롯 비어 있으면 FontScheme substitution 으로 theme major/minor.
        let table = FontTable {
            latin: None,
            cjk: None,
            script: None,
            symbol: None,
        };
        let rp = run_with_table(table);
        let theme = Theme {
            major_latin: Some("Calibri".to_string()),
            minor_latin: Some("Times".to_string()),
        };
        let result = realize_font(b'A' as u16, Some(&rp), &theme).unwrap();
        assert_eq!(result.face_name, "Calibri", "theme.major_latin first");
    }

    #[test]
    fn realize_font_falls_back_to_hcr_dotum_when_no_theme() {
        let table = FontTable {
            latin: None,
            cjk: None,
            script: None,
            symbol: None,
        };
        let rp = run_with_table(table);
        let theme = Theme::new();
        let result = realize_font(b'A' as u16, Some(&rp), &theme).unwrap();
        // raw line 25: hardcoded fallback "HCR Dotum".
        assert_eq!(result.face_name, REAL_FONT_FALLBACK_NAME);
    }
}
