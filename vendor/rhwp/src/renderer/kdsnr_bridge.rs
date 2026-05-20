//! `kdsnr_bridge` — rhwp IR ↔ kdsnr-layout/render raw ABI adapter.
//!
//! 책임:
//! - rhwp 의 IR (`Paragraph`, `ComposedLine`, `ComposedTextRun`, `CharShape`,
//!   `ParaShape`, `LineSeg`) 을 kdsnr-layout 의 raw ABI (`CharItemView`,
//!   `RunProperty`, `ParaProperty`, `BodyProperty`, `PropertyBag`) 로 변환.
//! - `kdsnr_layout::ppt_compose_layout` 등 byte-eq 알고리즘 호출 wrapper.
//! - 결과 metric 을 rhwp `paragraph_layout.rs` / `svg.rs` 등이 사용 가능한
//!   형태로 환원.
//!
//! 정공법 정책 [feedback_no_time_optimization]: stub/MVP 금지. 변환 불가능한
//! 필드는 RE 진행 후 채움. 임시 default 사용 시 본 모듈 상단에 LIMITATION 으로
//! 명시.
//!
//! ## 단위 정책
//! - rhwp IR: HWPUNIT 우세 (1 inch = 7200 HU)
//! - kdsnr-layout CharItemView: 픽셀 (f32, 96 DPI 기본)
//! - kdsnr-layout PropertyBag: 한컴 raw 단위 (line spacing 은 HWPUNIT,
//!   font_size 는 pt 의 f32)

#![cfg(not(target_arch = "wasm32"))]

use kdsnr_layout::{
    ppt_compose_layout, properties::keys as pkeys, BodyProperty, Break, CharItemView, Glyph,
    HashMapPropertyBag, ParaProperty, PropertyValue, RunProperty,
};

use crate::model::style::{
    Alignment as RhwpAlignment, CharShape, LineSpacingType as RhwpLineSpacingType, ParaShape,
};
use crate::renderer::composer::{ComposedLine, ComposedTextRun};
use crate::renderer::style_resolver::{ResolvedCharStyle, ResolvedParaStyle};

// ─────────────────────────────────────────────────────────────────────
// 단위 변환
// ─────────────────────────────────────────────────────────────────────

/// HWPUNIT → px. 1 inch = 7200 HWPUNIT.
#[inline]
pub fn hwpunit_to_px(val: i32, dpi: f64) -> f32 {
    (val as f64 * dpi / 7200.0) as f32
}

/// HWPUNIT → pt. 1 pt = 20 HWPUNIT (한컴 CharShape::base_size 단위).
#[inline]
pub fn hwpunit_to_pt(val: i32) -> f32 {
    val as f32 / 20.0
}

// ─────────────────────────────────────────────────────────────────────
// enum 매핑
// ─────────────────────────────────────────────────────────────────────

/// rhwp `Alignment` → PptCompositor `ALIGNMENT_TYPE` (key 0x8fc) 값.
///
/// 한컴 raw stage_5_alignment_switch dispatch:
/// - 0 = Justify (양쪽 정렬, alignment_type=0 → HUGE_STRETCH)
/// - 1 = Left
/// - 2 = Right
/// - 3 = Center
/// - 4 = Distribute (양쪽 + char-spread)
/// - 5 = Split (나눔 정렬)
pub fn alignment_to_pbag(a: RhwpAlignment) -> i32 {
    match a {
        RhwpAlignment::Justify => 0,
        RhwpAlignment::Left => 1,
        RhwpAlignment::Right => 2,
        RhwpAlignment::Center => 3,
        RhwpAlignment::Distribute => 4,
        RhwpAlignment::Split => 5,
    }
}

/// rhwp `LineSpacingType` → PptCompositor `SPACING_TYPE` (key 0x8fd) 값.
///
/// 한컴 raw: 0 Percent, 1 Fixed, 2 SpaceOnly (AtLeast), 3 Minimum.
pub fn line_spacing_type_to_pbag(t: RhwpLineSpacingType) -> i32 {
    match t {
        RhwpLineSpacingType::Percent => 0,
        RhwpLineSpacingType::Fixed => 1,
        RhwpLineSpacingType::SpaceOnly => 2,
        RhwpLineSpacingType::Minimum => 3,
    }
}

// ─────────────────────────────────────────────────────────────────────
// adapter: ParaShape → ParaProperty
// ─────────────────────────────────────────────────────────────────────

/// rhwp `ParaShape` → kdsnr `ParaProperty` (PropertyBag 채움).
///
/// PptCompositor 의 stage 5/6/9/10 에서 lookup 하는 키들을 모두 채운다.
///
/// 채우는 키:
/// - `PARAGRAPH_CLASS` (0x89e) — 기본 0 (Repair compositor 가 dispatch 결정)
/// - `ALIGNMENT_TYPE` (0x8fc) — `alignment_to_pbag`
/// - `SPACING_TYPE` (0x8fd) — `line_spacing_type_to_pbag`
/// - `LINE_HEIGHT_TYPE1` (0x907) — `line_spacing` HWPUNIT 그대로 (한컴 stage_9 가
///   factor 으로 사용)
/// - `LINE_HEIGHT_TYPE3` (0x909) — 동일
/// - `VERTICAL_ANCHOR` (0x900) — `vertical_align` (0=Top, 1=Center, 2=Bottom)
pub fn para_shape_to_property(ps: &ParaShape) -> ParaProperty {
    let mut bag = HashMapPropertyBag::new();
    bag.insert(pkeys::PARAGRAPH_CLASS, PropertyValue::Uint(0));
    bag.insert(
        pkeys::ALIGNMENT_TYPE,
        PropertyValue::Int(alignment_to_pbag(ps.alignment)),
    );
    bag.insert(
        pkeys::SPACING_TYPE,
        PropertyValue::Int(line_spacing_type_to_pbag(ps.line_spacing_type)),
    );
    bag.insert(
        pkeys::LINE_HEIGHT_TYPE1,
        PropertyValue::Int(ps.line_spacing),
    );
    bag.insert(
        pkeys::LINE_HEIGHT_TYPE3,
        PropertyValue::Int(ps.line_spacing),
    );
    bag.insert(
        pkeys::VERTICAL_ANCHOR,
        PropertyValue::Int(ps.vertical_align as i32),
    );

    let mut pp = ParaProperty::new();
    pp.property_bag = bag;
    pp
}

/// `ResolvedParaStyle` (px 변환 후) → `ParaProperty`.
///
/// rhwp 의 layout 코드는 ResolvedXxxStyle 만 다룸 (raw IR 직접 접근 X). 그래서 본
/// 경로가 실제 wire-in 시 사용됨.
///
/// LIMITATION: `line_spacing` 의 단위가 `line_spacing_type` 에 따라 다름:
/// - `Percent` → 비율 값 (예: 160.0 = 160%)
/// - `Fixed` → px 값 (rhwp 변환)
/// - `SpaceOnly` / `Minimum` → px
///
/// PptCompositor stage_9 는 한컴 raw HWPUNIT 기대. 본 함수는 PptCompositor 의
/// 본문 사용에 맞춰 raw 형식 그대로 i32 캐스트만 적용. 실제 단위 매핑 차이
/// (px vs HWPUNIT) 는 wire-in 후 측정으로 확정.
pub fn resolved_para_style_to_property(rps: &ResolvedParaStyle) -> ParaProperty {
    let mut bag = HashMapPropertyBag::new();
    bag.insert(pkeys::PARAGRAPH_CLASS, PropertyValue::Uint(0));
    bag.insert(
        pkeys::ALIGNMENT_TYPE,
        PropertyValue::Int(alignment_to_pbag(rps.alignment)),
    );
    bag.insert(
        pkeys::SPACING_TYPE,
        PropertyValue::Int(line_spacing_type_to_pbag(rps.line_spacing_type)),
    );
    let ls_i32 = rps.line_spacing as i32;
    bag.insert(pkeys::LINE_HEIGHT_TYPE1, PropertyValue::Int(ls_i32));
    bag.insert(pkeys::LINE_HEIGHT_TYPE3, PropertyValue::Int(ls_i32));

    let mut pp = ParaProperty::new();
    pp.property_bag = bag;
    pp
}

/// `ResolvedCharStyle` (px 변환 후) → `RunProperty`.
///
/// font_size: f64 px → pt (96 DPI: 1pt = 96/72 px).
pub fn resolved_char_style_to_property(rcs: &ResolvedCharStyle) -> RunProperty {
    let mut bag = HashMapPropertyBag::new();
    let font_size_pt = (rcs.font_size * 72.0 / 96.0) as f32;
    bag.insert(pkeys::FONT_SIZE, PropertyValue::Float(font_size_pt));
    bag.insert(
        pkeys::BOLD_FLAG,
        PropertyValue::Char(if rcs.bold { 1 } else { 0 }),
    );
    bag.insert(
        pkeys::ITALIC_FLAG,
        PropertyValue::Char(if rcs.italic { 1 } else { 0 }),
    );
    RunProperty::from_bag(bag)
}

// ─────────────────────────────────────────────────────────────────────
// adapter: CharShape → RunProperty
// ─────────────────────────────────────────────────────────────────────

/// rhwp `CharShape` → kdsnr `RunProperty`.
///
/// 채우는 키:
/// - `FONT_SIZE` (0x96a) — `base_size` HWPUNIT → pt (Float)
/// - `BOLD_FLAG` (0x967) — `bold` (Char 0/1)
/// - `ITALIC_FLAG` (0x968) — `italic` (Char 0/1)
///
/// LIMITATION: font_table 은 None (rhwp 의 ResolvedStyleSet 폰트 폴백 chain 사용
/// 중이므로 우리 측은 빈 슬롯). 향후 raw `GetRealFont` 포팅 완료 시 채움.
pub fn char_shape_to_property(cs: &CharShape) -> RunProperty {
    let mut bag = HashMapPropertyBag::new();
    bag.insert(
        pkeys::FONT_SIZE,
        PropertyValue::Float(hwpunit_to_pt(cs.base_size)),
    );
    bag.insert(
        pkeys::BOLD_FLAG,
        PropertyValue::Char(if cs.bold { 1 } else { 0 }),
    );
    bag.insert(
        pkeys::ITALIC_FLAG,
        PropertyValue::Char(if cs.italic { 1 } else { 0 }),
    );
    RunProperty::from_bag(bag)
}

// ─────────────────────────────────────────────────────────────────────
// adapter: BodyProperty (default)
// ─────────────────────────────────────────────────────────────────────

/// 가로쓰기 본문용 기본 `BodyProperty`. `VERTICAL` (key 0x89e) = 0.
///
/// LIMITATION: rhwp 모델에는 body-level vertical 플래그가 없으므로 가로쓰기로
/// 가정. 세로쓰기 (한컴 vertical layout) 지원 시 caller 가 별도 BodyProperty
/// 전달.
pub fn default_horizontal_body_property() -> BodyProperty {
    let mut bp = BodyProperty::new();
    let mut bag = HashMapPropertyBag::new();
    bag.insert(pkeys::PARAGRAPH_CLASS, PropertyValue::Uint(0));
    bp.property_bag = bag;
    bp
}

// ─────────────────────────────────────────────────────────────────────
// adapter: ComposedTextRun → Vec<CharItemView>
// ─────────────────────────────────────────────────────────────────────

/// 한 `ComposedTextRun` 을 문자 단위 `CharItemView` 리스트로 분해.
///
/// metric 측정 — rhwp 의 `font_runtime_metrics` 호출:
/// - `measure_face_metrics_em(family, bold, italic)` → ascender / descender em
///   (OS/2 또는 hhea 의 실측값. fontdb 에 face 없으면 None → fallback 0.85/0.15 em).
/// - `measure_char_advance_em(family, bold, italic, ch)` → advance em
///   (HFT cache 우선. fontdb hmtx fallback. miss → 0.5 em).
///
/// caller 가 미리 준비:
/// - `rp` : 이 run 의 RunProperty (`resolved_char_style_to_property` 결과)
/// - `pp` : paragraph 의 ParaProperty
/// - `bp` : 본문 BodyProperty (`default_horizontal_body_property` 권장)
/// - `font_family`, `bold`, `italic`, `font_size_px` : metric lookup 인자
/// G-W-3b: ParaProperty 보유한 단일 dummy CharItemView 컨테이너 생성.
///
/// `ppt_compose_break` 의 phase 1 indent 계산 (`GetParaItemView(composition, to)`)
/// 가 이 컨테이너 첫 CharItemView 의 ParaProperty 에서 key 0x901/0x8ff/0x900 을 읽음.
///
/// para_style 가 None 이면 indent 모두 0 (rhwp 기존 동작과 동일).
pub fn build_para_composition(para_style: Option<&ResolvedParaStyle>) -> CharItemContainer {
    let pp = match para_style {
        Some(ps) => resolved_para_style_to_property(ps),
        None => ParaProperty::default(),
    };
    let mut civ = CharItemView::new(0);
    civ.para_property = Some(pp);
    CharItemContainer::from_civs(vec![civ])
}

pub fn run_to_civs(
    run: &ComposedTextRun,
    rp: &RunProperty,
    pp: &ParaProperty,
    bp: &BodyProperty,
    font_family: &str,
    bold: bool,
    italic: bool,
    font_size_px: f32,
) -> Vec<CharItemView> {
    run_to_civs_with_raw(run, rp, pp, bp, font_family, "", bold, italic, font_size_px)
}

/// `run_to_civs` 의 raw face name 확장. rhwp 의 `measure_char_width_embedded_with_raw`
/// 와 동일 path: (1) raw face name 우선 HFT lookup → (2) substituted name → (3)
/// fontdb hmtx 폴백 → (4) HWPUNIT quantize (`px * 75 → int → /75`).
///
/// rhwp 의 `estimate_text_width` 와 동일 결과를 얻으려면 이 함수 사용.
pub fn run_to_civs_with_raw(
    run: &ComposedTextRun,
    rp: &RunProperty,
    pp: &ParaProperty,
    bp: &BodyProperty,
    font_family: &str,
    raw_font_family: &str,
    bold: bool,
    italic: bool,
    font_size_px: f32,
) -> Vec<CharItemView> {
    run_to_civs_with_raw_ratio(
        run,
        rp,
        pp,
        bp,
        font_family,
        raw_font_family,
        bold,
        italic,
        font_size_px,
        1.0,
        0.0,
    )
}

/// `run_to_civs_with_raw` 의 ratio/letter_spacing 인자 확장.
///
/// - `char_ratio`: ResolvedCharStyle.ratio (한컴 `<hh:ratio hangul="N">/100`).
///   한글 자간 가로 비율 (보통 0.85 ~ 1.0). estimate_text_width 와 동일하게 곱한다.
/// - `letter_spacing_px`: ResolvedCharStyle.letter_spacing (자간 추가 px).
///
/// `run_to_civs_with_raw` 는 ratio=1.0, letter_spacing=0.0 으로 호출.
pub fn run_to_civs_with_raw_ratio(
    run: &ComposedTextRun,
    rp: &RunProperty,
    pp: &ParaProperty,
    bp: &BodyProperty,
    font_family: &str,
    raw_font_family: &str,
    bold: bool,
    italic: bool,
    font_size_px: f32,
    char_ratio: f32,
    letter_spacing_px: f32,
) -> Vec<CharItemView> {
    // face metric 1회 lookup. fallback OS/2 표준 비율 (typo_ascender 0.85,
    // typo_descender 0.15) — face 없을 때만.
    let (asc_em, desc_em) = match crate::renderer::font_runtime_metrics::measure_face_metrics_em(
        font_family,
        bold,
        italic,
    ) {
        Some(m) => (m.ascender as f32, m.descender.abs() as f32),
        None => (0.85, 0.15),
    };
    let ascent = asc_em * font_size_px;
    let descent = desc_em * font_size_px;
    let total_height = ascent + descent;
    let var = if total_height > 0.0 {
        ascent / total_height
    } else {
        0.85
    };

    let mut out = Vec::with_capacity(run.text.chars().count());
    for c in run.text.chars() {
        let mut civ = CharItemView::new(c as u16);
        civ.run_property = Some(rp.clone());
        civ.para_property = Some(pp.clone());
        civ.body_property = Some(bp.clone());

        // rhwp `measure_char_width_embedded_with_raw` 와 동일 path:
        //   0순위: HFT raw_face_name → substituted_name
        //   1순위: fontdb hmtx (measure_char_advance_em — 내부적으로 HFT 도 시도)
        //   2순위: 0.5 em fallback
        let adv_em_raw: f64 = if !raw_font_family.is_empty() {
            crate::renderer::kdsnr_hft_global::advance_em(raw_font_family, c)
        } else {
            None
        }
        .or_else(|| crate::renderer::kdsnr_hft_global::advance_em(font_family, c))
        .or_else(|| {
            crate::renderer::font_runtime_metrics::measure_char_advance_em(
                font_family,
                bold,
                italic,
                c,
            )
        })
        .unwrap_or(0.5);

        // half-width punctuation correction (rhwp 와 동일 규칙)
        let is_halfwidth_punct = matches!(c, '\u{2018}'..='\u{2027}' | '\u{00B7}');
        let adv_em = if is_halfwidth_punct && adv_em_raw >= 1.0 {
            0.5
        } else {
            adv_em_raw
        };

        // ratio (한컴 자간) + letter_spacing 적용.
        // rhwp `estimate_text_width` 의 `style.ratio` 와 동일.
        let raw_px = (adv_em as f32) * font_size_px * char_ratio + letter_spacing_px;
        // HWPUNIT quantize: rhwp 의 estimate_text_width 와 동일 정확도.
        let hwp = (raw_px * 75.0) as i32;
        civ.width = hwp as f32 / 75.0;

        civ.ascent = ascent;
        civ.descent = descent;
        civ.total_height = total_height;
        // line_height 초기값 — PptCompositor stage_9 가 SPACING_TYPE 기반으로 재결정.
        civ.line_height = total_height;
        civ.vertical_anchor_ratio = var;
        out.push(civ);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────
// Composition 컨테이너 — ppt_compose_layout 호출용 Glyph
// ─────────────────────────────────────────────────────────────────────

/// `kdsnr_layout::Glyph` 를 구현하는 단순 컨테이너. `ppt_compose_layout` 의
/// `composition: &dyn Glyph` 인자 + `output: &mut dyn Glyph` 양쪽에 사용.
///
/// 내부에 `Box<dyn Glyph>` 리스트 보관 (CharItemView + Glue + 기타 stage 산물
/// 혼합 가능). `get_count` / `get_component` / `append` dispatch.
#[derive(Debug, Default)]
pub struct CharItemContainer {
    pub items: Vec<Box<dyn Glyph>>,
}

impl CharItemContainer {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn from_civs(items: Vec<CharItemView>) -> Self {
        Self {
            items: items
                .into_iter()
                .map(|civ| Box::new(civ) as Box<dyn Glyph>)
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// items 의 i 번째가 `CharItemView` 면 참조 반환. PptCompositor 출력
    /// 분석용 (line_height/vertical_anchor 등 추출).
    pub fn civ_at(&self, idx: usize) -> Option<&CharItemView> {
        self.items.get(idx)?.as_any().downcast_ref::<CharItemView>()
    }
}

impl Glyph for CharItemContainer {
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(Self {
            items: self.items.iter().map(|g| g.clone_glyph()).collect(),
        })
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn get_count(&self) -> usize {
        self.items.len()
    }
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        self.items.get(idx).map(|b| b.as_ref())
    }
    fn get_component_mut(&mut self, idx: usize) -> Option<&mut dyn Glyph> {
        self.items.get_mut(idx).map(|b| b.as_mut())
    }
    fn append(&mut self, child: Option<Box<dyn Glyph>>) {
        // 한컴 Box::Append: null SharePtr 도 placeholder slot. Glue 가 null 로 들어와도
        // count 증가 — 우리도 동일. None 의 경우 dummy debug-glyph 대신 별도 처리:
        // count 증가가 필요한 caller (PptCompositor stage_7/stage_13) 가 있으므로
        // None 은 placeholder Box (count 만 증가) 로 보관.
        if let Some(g) = child {
            self.items.push(g);
        } else {
            // placeholder: ZeroSizedPlaceholder
            self.items.push(Box::new(NullPlaceholder));
        }
    }
}

/// `append(None)` 호출 시 보관용 placeholder. 한컴 Box::Append 가 null SharePtr 도
/// 노드 add 하는 거 1:1 흉내. `get_count` 만 증가시키고 실제 layout 동작 없음.
#[derive(Debug, Clone)]
struct NullPlaceholder;

impl Glyph for NullPlaceholder {
    fn clone_glyph(&self) -> Box<dyn Glyph> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ─────────────────────────────────────────────────────────────────────
// ppt_compose_layout 호출 wrapper
// ─────────────────────────────────────────────────────────────────────

/// 한 `ComposedLine` 을 PptCompositor 의 13 stage 알고리즘으로 layout.
///
/// caller 가 미리 채워준 CharItemView list (`run_to_civs` 결과 모두 concat) 를
/// 받아 PptCompositor 출력 CharItemView 들을 반환 — 각 char 에 stage_8 (font
/// metric accumulation) / stage_9 (line height type lookup) / stage_10 (apply
/// top/bottom extra) / stage_11 (alignment ratio mutation) 결과가 박혀 있음.
///
/// `composition_type`: 1 = 가로쓰기 (기본). 2 = 세로쓰기.
///
/// 반환된 vec 의 [0..2] = stage_7 pre-pads (Glue), 본문, [..마지막 2] = stage_13
/// post-pads. caller 가 본문 슬라이스만 사용.
pub fn compose_line(civs: Vec<CharItemView>, composition_type: u32) -> CharItemContainer {
    let n = civs.len();
    let input = CharItemContainer::from_civs(civs);
    let mut output = CharItemContainer::new();
    if n == 0 {
        // 빈 range: stage 들이 default path. 호출 자체는 panic-free.
        ppt_compose_layout(
            &input,
            composition_type,
            Break::new(0, -1),
            -1,
            0,
            &mut output,
        );
    } else {
        ppt_compose_layout(
            &input,
            composition_type,
            Break::new(0, (n as i32) - 1),
            -1,
            0,
            &mut output,
        );
    }
    output
}
