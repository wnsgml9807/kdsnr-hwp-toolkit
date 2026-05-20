//! `Hnc::Shape::Text::EquationGlyph` (가칭) — `<hp:equation>` 처리 placeholder
//! + pass-through (L-5c-RE-eq-1).
//!
//! ## 본 module 의 범위
//!
//! 한컴 dylib 의 수식 렌더링 엔진은 별도 plugin (`HncEqEdit.dylib`) 으로 격리:
//! - 1 export = `HncGetEqEditPluginProxy`
//! - 240 KB text, 868 internal functions, 43 EqNode AST class
//! - P0 input 928 script 분석 → 18~20 / 43 Node 발동, 발동 영역 ~100 KB raw
//!
//! byte-eq port 는 별도 큰 작업 (산정 28~32 세션). 본 module 은 그 전 단계로:
//! 1. `<hp:equation>` 의 메타데이터 보존 (script, width/height, baseline, textColor, font)
//! 2. SvgSurface 에 placeholder/pass-through emit (3가지 모드)
//! 3. 향후 byte-eq port 또는 syntax-based 재구현의 entry point
//!
//! ## EquationRenderMode
//!
//! - **Placeholder**: dashed bbox + `[수식]` 라벨 (debug / 초기 단계)
//! - **ScriptInline**: `<text>` 로 hp:script 원문 그대로 (편집 가능, pixel-eq 아님)
//! - **SvgInline**: caller 가 미리 만든 SVG 조각 inline 삽입 (rhwp 결과 wire 용)
//!
//! ## raw HWPX 구조 (참고)
//!
//! ```xml
//! <hp:equation id="..." baseLine="85" textColor="#000000" baseUnit="1100" font="HYhwpEQ">
//!   <hp:script>y=log _{`6`} x`</hp:script>
//!   <hp:sz width="4226" widthRelTo="ABSOLUTE" height="1350" heightRelTo="ABSOLUTE"/>
//!   <hp:pos treatAsChar="1" .../>
//!   <hp:outMargin left="56" right="56" top="0" bottom="0"/>
//! </hp:equation>
//! ```
//!
//! - `script`: HWP equation syntax (LaTeX-like with `^{}`, `_{}`, `sqrt {}`, `vec {}` 등)
//! - `width/height`: HWPUNIT (1/7200 inch). pt = HWPUNIT / 100
//! - `baseLine`: HWPUNIT, baseline offset from top
//! - `textColor`: `#rrggbb`
//! - `baseUnit`: 1100 = 11pt (standard)
//! - `font`: 보통 "HYhwpEQ" (Hancom 전용 수식 폰트)

use std::fmt::Write;

/// `<hp:equation>` 1개의 메타데이터 + script + (옵션) 미리 렌더된 SVG.
#[derive(Debug, Clone)]
pub struct EquationDescriptor {
    /// HWPX `<hp:script>` 원문 (HWP equation syntax).
    pub script: String,
    /// `<hp:sz width="..."/>` HWPUNIT (1/7200 inch).
    pub width_hwpunit: i32,
    /// `<hp:sz height="..."/>` HWPUNIT.
    pub height_hwpunit: i32,
    /// `<hp:equation baseLine="..."/>` HWPUNIT, top → baseline offset.
    pub baseline_hwpunit: i32,
    /// `<hp:equation textColor="..."/>` (`#rrggbb` or word color).
    pub text_color: String,
    /// `<hp:equation baseUnit="..."/>` 100*pt (1100 = 11pt).
    pub base_unit: i32,
    /// `<hp:equation font="..."/>` 보통 "HYhwpEQ".
    pub font_name: String,
    /// 미리 렌더된 SVG 조각 (rhwp `EquationNode.svg_content` 등 외부 source).
    /// 없으면 None — 본 module 이 placeholder/script emit fallback.
    pub pre_rendered_svg: Option<String>,
}

impl EquationDescriptor {
    /// HWPUNIT (1/7200 inch) → SVG px (1px = 1/96 inch).
    pub fn width_px(&self) -> f32 {
        self.width_hwpunit as f32 * 96.0 / 7200.0
    }
    pub fn height_px(&self) -> f32 {
        self.height_hwpunit as f32 * 96.0 / 7200.0
    }
    pub fn baseline_px(&self) -> f32 {
        self.baseline_hwpunit as f32 * 96.0 / 7200.0
    }
}

/// 수식을 SVG 에 어떤 식으로 표시할지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquationRenderMode {
    /// dashed box + `[수식]` label. debug / 초기 단계.
    Placeholder,
    /// `<text>` 로 hp:script 원문 그대로. 픽셀 정확도 없음, debug 용.
    ScriptInline,
    /// `EquationDescriptor.pre_rendered_svg` 가 있으면 그대로 inline. 없으면 Placeholder.
    SvgInline,
}

/// 단일 equation 을 SVG 조각으로 emit. caller (SvgSurface) 가 결과 string 을 buffer 에 push.
///
/// `pos_x`, `pos_y` 는 SVG 좌표 (현재 transform 적용 후 픽셀). bbox 의 좌상단.
///
/// # 반환
///
/// SVG element string (예: `"<g transform=...>...</g>"`).
pub fn emit_equation(
    desc: &EquationDescriptor,
    pos_x: f32,
    pos_y: f32,
    mode: EquationRenderMode,
) -> String {
    let w = desc.width_px();
    let h = desc.height_px();

    match mode {
        EquationRenderMode::Placeholder => emit_placeholder(desc, pos_x, pos_y, w, h),
        EquationRenderMode::ScriptInline => emit_script_inline(desc, pos_x, pos_y, w, h),
        EquationRenderMode::SvgInline => {
            if let Some(svg) = desc.pre_rendered_svg.as_ref() {
                emit_svg_inline(svg, pos_x, pos_y, w, h)
            } else {
                emit_placeholder(desc, pos_x, pos_y, w, h)
            }
        }
    }
}

fn emit_placeholder(desc: &EquationDescriptor, x: f32, y: f32, w: f32, h: f32) -> String {
    let mut out = String::with_capacity(256);
    let _ = write!(
        out,
        r#"<g transform="translate({x:.3} {y:.3})"><rect x="0" y="0" width="{w:.3}" height="{h:.3}" fill="none" stroke="{c}" stroke-width="0.5" stroke-dasharray="2 2"/><text x="{tx:.3}" y="{ty:.3}" font-size="{fs:.2}" fill="{c}" text-anchor="middle">[수식]</text></g>
"#,
        x = x,
        y = y,
        w = w,
        h = h,
        c = desc.text_color,
        tx = w / 2.0,
        ty = h / 2.0 + 4.0,
        fs = (w.min(h) * 0.4).clamp(8.0, 14.0),
    );
    out
}

fn emit_script_inline(desc: &EquationDescriptor, x: f32, y: f32, w: f32, h: f32) -> String {
    let mut out = String::with_capacity(256 + desc.script.len() * 2);
    let escaped = escape_xml(&desc.script);
    // baseline 적용한 y (top → baseline)
    let bl = desc.baseline_px();
    let _ = write!(
        out,
        r#"<g transform="translate({x:.3} {y:.3})"><text x="0" y="{bl:.3}" font-family="{fnt}" font-size="{fs:.2}" fill="{c}">{txt}</text></g>
"#,
        x = x,
        y = y,
        bl = bl,
        fnt = desc.font_name,
        fs = (desc.base_unit as f32) / 100.0,
        c = desc.text_color,
        txt = escaped,
    );
    // h 미사용 (script-inline 은 font-size 결정)
    let _ = (w, h);
    out
}

fn emit_svg_inline(pre_svg: &str, x: f32, y: f32, _w: f32, _h: f32) -> String {
    let mut out = String::with_capacity(64 + pre_svg.len());
    let _ = write!(
        out,
        r#"<g transform="translate({x:.3} {y:.3})">{svg}</g>
"#,
        x = x,
        y = y,
        svg = pre_svg,
    );
    out
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_desc() -> EquationDescriptor {
        EquationDescriptor {
            script: "y= 6 ^{`x-1} +3`".to_string(),
            width_hwpunit: 5458,
            height_hwpunit: 1313,
            baseline_hwpunit: 87,
            text_color: "#000000".to_string(),
            base_unit: 1100,
            font_name: "HYhwpEQ".to_string(),
            pre_rendered_svg: None,
        }
    }

    #[test]
    fn descriptor_hwpunit_to_px_conversion() {
        let d = sample_desc();
        // 5458 * 96 / 7200 ≈ 72.77
        let w = d.width_px();
        assert!((w - 72.773).abs() < 0.01, "got {}", w);
        let h = d.height_px();
        assert!((h - 17.506).abs() < 0.01, "got {}", h);
    }

    #[test]
    fn placeholder_emits_dashed_rect_and_label() {
        let d = sample_desc();
        let svg = emit_equation(&d, 100.0, 200.0, EquationRenderMode::Placeholder);
        assert!(svg.contains("transform=\"translate(100.000 200.000)\""), "got: {}", svg);
        assert!(svg.contains("stroke-dasharray=\"2 2\""));
        assert!(svg.contains("[수식]"));
        assert!(svg.contains("fill=\"#000000\""));
    }

    #[test]
    fn script_inline_emits_raw_text_with_font() {
        let d = sample_desc();
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::ScriptInline);
        assert!(svg.contains("font-family=\"HYhwpEQ\""), "got: {}", svg);
        assert!(svg.contains("font-size=\"11.00\""));
        assert!(svg.contains("y= 6 ^{`x-1} +3`"));
    }

    #[test]
    fn script_inline_escapes_xml_special_chars() {
        let mut d = sample_desc();
        d.script = "a<b & c>d \"e\" 'f'".to_string();
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::ScriptInline);
        assert!(svg.contains("a&lt;b &amp; c&gt;d &quot;e&quot; &apos;f&apos;"), "got: {}", svg);
    }

    #[test]
    fn svg_inline_with_pre_rendered_uses_pre_svg() {
        let mut d = sample_desc();
        d.pre_rendered_svg = Some("<path d=\"M0 0 L10 10\" stroke=\"red\"/>".to_string());
        let svg = emit_equation(&d, 50.0, 60.0, EquationRenderMode::SvgInline);
        assert!(svg.contains("<path d=\"M0 0 L10 10\" stroke=\"red\"/>"), "got: {}", svg);
        assert!(svg.contains("transform=\"translate(50.000 60.000)\""));
    }

    #[test]
    fn svg_inline_without_pre_rendered_falls_back_to_placeholder() {
        let d = sample_desc(); // pre_rendered_svg = None
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::SvgInline);
        assert!(svg.contains("[수식]"), "expected placeholder fallback, got: {}", svg);
    }

    #[test]
    fn placeholder_with_custom_text_color() {
        let mut d = sample_desc();
        d.text_color = "#ff0000".to_string();
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::Placeholder);
        assert!(svg.contains("stroke=\"#ff0000\""));
        assert!(svg.contains("fill=\"#ff0000\""));
    }

    #[test]
    fn descriptor_with_baseline_offset_emits_text_below_top() {
        let d = sample_desc();
        // baseline_hwpunit = 87 → 87 * 96/7200 ≈ 1.16 px
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::ScriptInline);
        // text y= baseline_px ≈ 1.160
        assert!(svg.contains("y=\"1.160\""), "got: {}", svg);
    }

    #[test]
    fn small_equation_uses_minimum_font_size_in_placeholder() {
        let mut d = sample_desc();
        d.width_hwpunit = 100; // very small
        d.height_hwpunit = 100;
        let svg = emit_equation(&d, 0.0, 0.0, EquationRenderMode::Placeholder);
        // font-size should clamp at 8.00
        assert!(svg.contains("font-size=\"8.00\""), "got: {}", svg);
    }
}
