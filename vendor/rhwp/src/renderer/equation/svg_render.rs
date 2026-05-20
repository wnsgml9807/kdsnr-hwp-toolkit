//! 수식 SVG 렌더러
//!
//! LayoutBox를 SVG 요소로 변환한다.
//! 생성된 SVG 조각은 `<g>` 요소 내부에 포함된다.

use super::ast::MatrixStyle;
use super::layout::*;
use super::symbols::{DecoKind, FontStyleKind};

/// 수식 전용 font-family 폴백 체인
/// 한컴 hwp 의 본문 식은 변수에 Times-Italic 계열을 사용한다 (그리스/수학 기호는 STIX 필요).
/// 따라서 Times → STIX → 기타 순으로 폴백한다. Latin Modern Math 는 Mac 기본 미설치라 제외.
const EQ_FONT_FALLBACK: &str =
    "'Times New Roman', 'Times', 'STIX Two Text', 'STIX Two Math', serif";

/// HWPX 의 `font="…"` 값에 대응되는 실제 TTF family 정규명.
/// usvg/fontdb 는 case-sensitive 매칭이라 입력값(`HYhwpEQ`) 가 TTF 내부
/// family(`HyhwpEQ`) 와 case 가 다르면 매치 실패한다. 그래서 입력값이 아닌
/// 정규명**만** 출력한다 — 입력값을 family list 의 1순위에 두면 매치 실패로 인해
/// usvg 가 다음 family(=정규명) 로 넘어가지만, 그 시점에서 italic variant 부재 시
/// 추가 fallback 이 동작하지 않아 변수가 직립으로 출력된다 (검증된 usvg 동작).
fn canonical_eq_font(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "hyhwpeq" => Some("HyhwpEQ"),
        "hancomeqn" => Some("HancomEQN"),
        "haan symbol" | "haansymbol" | "hansymbol" => Some("Haan Symbol"),
        _ => None,
    }
}

/// HyhwpEQ 의 italic 변수 PUA 매핑.
///
/// HyhwpEQ TTF 는 italic variant 가 없는 단일 weight 폰트지만, PUA 영역에 italic
/// 소문자 알파벳 글리프가 들어있다 (시각 카탈로그 + Hancom export PDF ToUnicode
/// CMap 으로 확인):
/// - U+E0E5..U+E0FE = italic a-z (소문자 26자)
///
/// 검증 예: 'k'(0x6B) → 0xE0EF, 'x'(0x78) → 0xE0FC, 'y'(0x79) → 0xE0FD —
/// Hancom Q22 PDF 가 사용한 italic 글리프와 일치.
///
/// 대문자 (A-Z) 는 한컴 hwpeq 엔진도 ASCII 그대로 출력해 직립 — PUA 변환 안 함.
/// (E0CB-E0E4 영역은 italic 대문자가 아니라 blackboard bold 글리프이므로
/// 매핑 시 ℙ, 𝕊 같은 의도하지 않은 글리프가 출력된다.)
///
/// 한컴 GUI 출력은 변수 소문자에 자동 italic 적용 (LaTeX 와 동일). usvg 는 italic
/// variant 부재 시 family chain 의 다음 폰트로 fallback 못 하므로 (`font-style:italic`
/// CSS 가 무시됨), 우리는 italic 텍스트의 ASCII 소문자만 직접 PUA 코드로 변환해서
/// HyhwpEQ 의 italic 글리프를 명시적으로 사용한다.
fn map_ascii_to_italic_pua(text: &str) -> String {
    text.chars().map(map_ascii_char_to_italic_pua).collect()
}

/// 단일 글자 italic PUA 매핑. layout 측정과 emit 가 동일 PUA codepoint 의 hmtx 를
/// 보도록 layout.rs 에서도 사용한다.
pub(crate) fn map_ascii_char_to_italic_pua(c: char) -> char {
    let cp = c as u32;
    match cp {
        0x61..=0x7A => char::from_u32(0xE0E5 + (cp - 0x61)).unwrap_or(c),
        _ => c,
    }
}

/// HyhwpEQ Greek 글리프 매핑 (PUA 전용).
///
/// HyhwpEQ TTF 의 cmap 에는 Greek codepoint(U+0391..U+03C9) 가 직접 등록되어
/// 있지 않다 → fontdb 가 매치 실패해서 다른 폰트(Apple Symbols 등) 로 fallback,
/// 굵은 sans-serif π 같은 의도하지 않은 외형 발생.
///
/// HyhwpEQ 의 실제 Greek 글리프는 PUA 영역에 들어있다 (시각 카탈로그 + Hancom
/// PDF 의 [E0AC]=π 검증):
/// - U+E085..U+E09C = 대문자 Α-Ω (24자, 0x03A2 reserved 자리 한 번 건너뜀)
/// - U+E09D..U+E0B5 = 소문자 α-ω (25자, ς 포함)
///
/// 이 매핑은 italic 여부와 무관하게 항상 적용한다 (Greek 글리프 자체가 hwpeq
/// 의 italic-style 외형으로 디자인됨).
fn map_greek_to_pua(text: &str) -> String {
    text.chars().map(map_greek_char_to_pua).collect()
}

/// 단일 글자 Greek PUA 매핑. layout 측정과 emit 가 동일 PUA codepoint 의 hmtx 를
/// 보도록 layout.rs 에서도 사용한다.
pub(crate) fn map_greek_char_to_pua(c: char) -> char {
    let cp = c as u32;
    match cp {
        0x0391..=0x03A1 => char::from_u32(0xE085 + (cp - 0x0391)).unwrap_or(c),
        0x03A3..=0x03A9 => char::from_u32(0xE085 + (cp - 0x0391) - 1).unwrap_or(c),
        0x03B1..=0x03C9 => char::from_u32(0xE09D + (cp - 0x03B1)).unwrap_or(c),
        _ => c,
    }
}

/// 텍스트에 italic + Greek PUA 변환을 모두 적용한다.
fn remap_text(text: &str, italic: bool) -> String {
    let g = map_greek_to_pua(text);
    if italic {
        map_ascii_to_italic_pua(&g)
    } else {
        g
    }
}

/// HWPX `<hp:equation font="…">` 값을 받아 font-family 속성 문자열을 만든다.
///
/// 정책: HWPX 가 명시한 폰트는 정규명으로 변환해서 family 1순위에 둔다. italic
/// 변수는 `map_ascii_to_italic_pua` 로 PUA 변환 후 HyhwpEQ 글리프 직사용.
fn build_eq_font_family_attr(font_name: &str) -> String {
    let trimmed = font_name.trim();
    let mut families: Vec<String> = Vec::new();
    if !trimmed.is_empty() {
        // 알려진 한컴 폰트면 정규명만 사용. 모르는 폰트면 입력 그대로.
        let primary = canonical_eq_font(trimmed)
            .map(|s| s.to_string())
            .unwrap_or_else(|| escape_xml(trimmed));
        families.push(format!("'{}'", primary));
    }
    families.push("'Times New Roman'".into());
    families.push("'Times'".into());
    families.push("'STIX Two Text'".into());
    families.push("'STIX Two Math'".into());
    families.push("serif".into());
    format!(" font-family=\"{}\"", families.join(", "))
}

/// 수식을 SVG 조각 문자열로 렌더링
///
/// 진입점 default: italic=true. HWPX `<hp:equation>` 메타엔 italic 속성이 없지만,
/// 한컴 hwpeq 의 default 동작은 LaTeX와 동일하게 변수에 italic 적용이고
/// `rm`/`RM` 명령으로 명시될 때만 직립으로 전환된다 (Q22_hwpsaved.hwpx 검증:
/// charPr italic="1" 가 0개이지만 한컴 GUI 출력은 변수 italic — hwpeq 엔진 default).
///
/// `font_name` 은 HWPX 의 `<hp:equation font="…">` 값. 빈 문자열이면 폴백만 사용.
pub fn render_equation_svg(
    layout: &LayoutBox,
    color: &str,
    base_font_size: f64,
    font_name: &str,
) -> String {
    let mut svg = String::new();
    let family_attr = build_eq_font_family_attr(font_name);
    render_box(
        &mut svg,
        layout,
        0.0,
        0.0,
        color,
        base_font_size,
        true,
        false,
        &family_attr,
    );
    svg
}

fn render_box(
    svg: &mut String,
    lb: &LayoutBox,
    parent_x: f64,
    parent_y: f64,
    color: &str,
    fs: f64,
    italic: bool,
    bold: bool,
    font_family: &str,
) {
    let x = parent_x + lb.x;
    let y = parent_y + lb.y;

    match &lb.kind {
        LayoutKind::Row(children) => {
            for child in children {
                render_box(svg, child, x, y, color, fs, italic, bold, font_family);
            }
        }
        LayoutKind::Text(text) => {
            let text_x = x;
            let text_y = y + lb.baseline;
            let fi = fs;
            // CJK/한글 텍스트는 이탤릭 없이 렌더링 (수학 변수명만 이탤릭).
            // FontStyle::Roman(`rm` 적용)으로 italic=false 가 전달된 경우에도 이탤릭을 적용하지 않는다.
            let has_cjk = text.chars().any(|c| {
                matches!(c,
                    '\u{3000}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{AC00}'..='\u{D7AF}'
                )
            });
            let italic_active = !has_cjk && italic;
            let esc = escape_xml(&remap_text(text, italic_active));
            let weight_attr = if bold { " font-weight=\"bold\"" } else { "" };
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}{}>{}</text>\n",
                text_x, text_y, fi, color, weight_attr, font_family, esc,
            ));
        }
        LayoutKind::Number(text) => {
            let text_x = x;
            let text_y = y + lb.baseline;
            let esc = escape_xml(&remap_text(text, false));
            let fi = fs;
            let style_attr = if bold { " font-weight=\"bold\"" } else { "" };
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}{}>{}</text>\n",
                text_x, text_y, fi, color, style_attr, font_family, esc,
            ));
        }
        LayoutKind::Symbol(text) => {
            let text_x = x + lb.width / 2.0;
            let text_y = y + lb.baseline;
            let esc = escape_xml(&remap_text(text, false));
            let fi = fs;
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\" text-anchor=\"middle\"{}>{}</text>\n",
                text_x, text_y, fi, color, font_family, esc,
            ));
        }
        LayoutKind::MathSymbol(text) => {
            let text_x = x;
            let text_y = y + lb.baseline;
            let esc = escape_xml(&remap_text(text, false));
            // 적분 기호: layout에서 BIG_OP_SCALE이 적용된 높이를 font-size로 사용
            let fi = if super::layout::is_integral_symbol(text) {
                lb.height
            } else {
                fs
            };
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                text_x, text_y, fi, color, font_family, esc,
            ));
        }
        LayoutKind::Function(name) => {
            let text_x = x;
            let text_y = y + lb.baseline;
            let esc = escape_xml(&remap_text(name, false));
            let fi = fs;
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                text_x, text_y, fi, color, font_family, esc,
            ));
        }
        LayoutKind::Fraction { numer, denom } => {
            // 분자
            render_box(svg, numer, x, y, color, fs, italic, bold, font_family);
            // 분수선 — baseline에서 axis_height 위에 배치
            let line_y = y + lb.baseline - fs * super::layout::AXIS_HEIGHT;
            let line_thick = fs * 0.04;
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                x + fs * 0.05, line_y,
                x + lb.width - fs * 0.05, line_y,
                color, line_thick,
            ));
            // 분모
            render_box(svg, denom, x, y, color, fs, italic, bold, font_family);
        }
        LayoutKind::Atop { top, bottom } => {
            render_box(svg, top, x, y, color, fs, italic, bold, font_family);
            render_box(svg, bottom, x, y, color, fs, italic, bold, font_family);
        }
        LayoutKind::Sqrt { index, body } => {
            // √ 기호
            let sign_h = lb.height;
            let body_left = x + body.x - fs * 0.1;
            let sign_x = x;
            // V 모양 경로
            let v_top = y;
            let v_mid_x = body_left - fs * 0.15;
            let v_mid_y = y + sign_h;
            let v_start_x = v_mid_x - fs * 0.3;
            let v_start_y = y + sign_h * 0.6;
            let tick_x = v_start_x - fs * 0.1;
            let tick_y = v_start_y - fs * 0.05;

            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                tick_x, tick_y,
                v_start_x, v_start_y,
                v_mid_x, v_mid_y,
                body_left, v_top,
                x + lb.width, v_top,
                color, fs * 0.04,
            ));

            // 인덱스 (있으면)
            if let Some(idx) = index {
                render_box(
                    svg,
                    idx,
                    sign_x,
                    y,
                    color,
                    fs * super::layout::SCRIPT_SCALE,
                    false,
                    false,
                    font_family,
                );
            }

            // 본체
            render_box(svg, body, x, y, color, fs, italic, bold, font_family);
        }
        LayoutKind::Superscript { base, sup } => {
            render_box(svg, base, x, y, color, fs, italic, bold, font_family);
            render_box(
                svg,
                sup,
                x,
                y,
                color,
                fs * super::layout::SCRIPT_SCALE,
                italic,
                bold,
                font_family,
            );
        }
        LayoutKind::Subscript { base, sub } => {
            render_box(svg, base, x, y, color, fs, italic, bold, font_family);
            render_box(
                svg,
                sub,
                x,
                y,
                color,
                fs * super::layout::SCRIPT_SCALE,
                italic,
                bold,
                font_family,
            );
        }
        LayoutKind::SubSup { base, sub, sup } => {
            render_box(svg, base, x, y, color, fs, italic, bold, font_family);
            render_box(
                svg,
                sub,
                x,
                y,
                color,
                fs * super::layout::SCRIPT_SCALE,
                italic,
                bold,
                font_family,
            );
            render_box(
                svg,
                sup,
                x,
                y,
                color,
                fs * super::layout::SCRIPT_SCALE,
                italic,
                bold,
                font_family,
            );
        }
        LayoutKind::BigOp { symbol, sub, sup } => {
            let op_fs = fs * super::layout::BIG_OP_SCALE;
            let is_integral = super::layout::is_integral_symbol(symbol);
            let esc = escape_xml(symbol);

            if is_integral {
                // 적분: 기호는 왼쪽, 첨자는 오른쪽 위/아래 (nolimits)
                let op_x = x;
                let op_y = y + op_fs * 0.8;
                svg.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                    op_x, op_y, op_fs, color, font_family, esc,
                ));
            } else {
                // ∑, ∏ 등: 기호는 중앙, 첨자는 위/아래 (limits)
                let sup_h = sup.as_ref().map(|b| b.height + fs * 0.05).unwrap_or(0.0);
                let op_x = x + (lb.width - estimate_op_width(symbol, op_fs)) / 2.0;
                let op_y = y + sup_h + op_fs * 0.8;
                svg.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                    op_x, op_y, op_fs, color, font_family, esc,
                ));
            }
            // 위/아래 첨자: LayoutBox의 자식 좌표로 배치 — 본 본체의 italic 컨텍스트 유지
            if let Some(sup_box) = sup {
                render_box(
                    svg,
                    sup_box,
                    x,
                    y,
                    color,
                    fs * super::layout::SCRIPT_SCALE,
                    italic,
                    bold,
                    font_family,
                );
            }
            if let Some(sub_box) = sub {
                render_box(
                    svg,
                    sub_box,
                    x,
                    y,
                    color,
                    fs * super::layout::SCRIPT_SCALE,
                    italic,
                    bold,
                    font_family,
                );
            }
        }
        LayoutKind::Limit { is_upper, sub } => {
            let name = if *is_upper { "Lim" } else { "lim" };
            let fi = fs;
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                x,
                y + fi * 0.8,
                fi,
                color,
                font_family,
                name,
            ));
            // lim 밑 첨자도 italic 컨텍스트 유지 (변수 x → t 등이 italic 으로 출력).
            // 한컴 hwpeq 의 lim 밑 글씨는 일반 첨자(0.7)보다 작은 LIMIT_SCRIPT_SCALE 사용.
            if let Some(sub_box) = sub {
                render_box(
                    svg,
                    sub_box,
                    x,
                    y,
                    color,
                    fs * super::layout::LIMIT_SCRIPT_SCALE,
                    italic,
                    bold,
                    font_family,
                );
            }
        }
        LayoutKind::Matrix { cells, style } => {
            // 괄호
            let bracket_chars = match style {
                MatrixStyle::Paren => ("(", ")"),
                MatrixStyle::Bracket => ("[", "]"),
                MatrixStyle::Vert => ("|", "|"),
                MatrixStyle::Plain => ("", ""),
            };
            if !bracket_chars.0.is_empty() {
                draw_stretch_bracket(
                    svg,
                    bracket_chars.0,
                    x,
                    y,
                    fs * 0.3,
                    lb.height,
                    color,
                    fs,
                    font_family,
                );
                draw_stretch_bracket(
                    svg,
                    bracket_chars.1,
                    x + lb.width - fs * 0.3,
                    y,
                    fs * 0.3,
                    lb.height,
                    color,
                    fs,
                    font_family,
                );
            }
            // 셀 내용
            for row in cells {
                for cell in row {
                    render_box(svg, cell, x, y, color, fs, italic, bold, font_family);
                }
            }
        }
        LayoutKind::Rel { arrow, over, under } => {
            render_box(svg, over, x, y, color, fs, italic, bold, font_family);
            render_box(svg, arrow, x, y, color, fs, italic, bold, font_family);
            if let Some(u) = under {
                render_box(svg, u, x, y, color, fs, italic, bold, font_family);
            }
        }
        LayoutKind::EqAlign { rows } => {
            for (left, right) in rows {
                render_box(svg, left, x, y, color, fs, italic, bold, font_family);
                render_box(svg, right, x, y, color, fs, italic, bold, font_family);
            }
        }
        LayoutKind::Paren { left, right, body } => {
            // 텍스트 높이 파렌(`(`, `)`)은 폰트 글리프로 렌더, 그 외는 path. (Task #283)
            // Brace ({/}) needs a wider paren_w so the middle bump is
            // visible; layout_paren reserves the matching width.
            let paren_w_of = |s: &str| -> f64 {
                if s == "{" || s == "}" {
                    fs * 0.5
                } else {
                    fs * 0.333
                }
            };
            let left_w = if left.is_empty() {
                0.0
            } else {
                paren_w_of(left)
            };
            let right_w = if right.is_empty() {
                0.0
            } else {
                paren_w_of(right)
            };
            let use_glyph = lb.height <= fs * 1.2;
            // 왼쪽 괄호
            if !left.is_empty() {
                if use_glyph && (left == "(" || left == ")") {
                    svg.push_str(&format!(
                        "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                        x, y + lb.baseline, fs, color, font_family, escape_xml(left),
                    ));
                } else {
                    draw_stretch_bracket(
                        svg,
                        left,
                        x,
                        y,
                        left_w,
                        lb.height,
                        color,
                        fs,
                        font_family,
                    );
                }
            }
            // 본체
            render_box(svg, body, x, y, color, fs, italic, bold, font_family);
            // 오른쪽 괄호
            if !right.is_empty() {
                let right_x = x + lb.width - right_w;
                if use_glyph && (right == "(" || right == ")") {
                    svg.push_str(&format!(
                        "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\"{}>{}</text>\n",
                        right_x, y + lb.baseline, fs, color, font_family, escape_xml(right),
                    ));
                } else {
                    draw_stretch_bracket(
                        svg,
                        right,
                        right_x,
                        y,
                        right_w,
                        lb.height,
                        color,
                        fs,
                        font_family,
                    );
                }
            }
        }
        LayoutKind::Decoration { kind, body } => {
            render_box(svg, body, x, y, color, fs, italic, bold, font_family);
            let deco_y = y + fs * 0.05;
            let mid_x = x + body.x + body.width / 2.0;
            draw_decoration(svg, *kind, mid_x, deco_y, body.width, color, fs);
        }
        LayoutKind::FontStyle { style, body } => {
            let (new_italic, new_bold) = match style {
                FontStyleKind::Roman => (false, false),
                FontStyleKind::Italic => (true, bold),
                FontStyleKind::Bold => (italic, true),
            };
            render_box(
                svg,
                body,
                x,
                y,
                color,
                fs,
                new_italic,
                new_bold,
                font_family,
            );
        }
        LayoutKind::Space(_) | LayoutKind::Newline | LayoutKind::Empty => {}
        LayoutKind::BoxFrame { body } => {
            // 사각형 frame + 내부 body 렌더. stroke 두께는 fs 비례.
            let stroke_w = (fs * 0.05).max(0.5);
            svg.push_str(&format!(
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" \
                 fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                x, y, lb.width, lb.height, color, stroke_w,
            ));
            render_box(svg, body, x, y, color, fs, italic, bold, font_family);
        }
    }
}

fn font_size_from_box(lb: &LayoutBox, base_fs: f64) -> f64 {
    // 박스 높이에서 폰트 크기 추정 (baseline 비율로)
    if lb.height > 0.0 {
        lb.height
    } else {
        base_fs
    }
}

fn estimate_op_width(text: &str, fs: f64) -> f64 {
    let mut w = 0.0;
    for ch in text.chars() {
        // operator 는 italic 아님. Greek operator (Δ 등) 는 PUA 매핑 후 measure
        // 해야 emit (`remap_text(.., italic=false)` 가 Greek→PUA 적용) 과 동일.
        let measured = map_greek_char_to_pua(ch);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(em) = crate::renderer::font_runtime_metrics::measure_char_advance_em(
                "HyhwpEQ", false, false, measured,
            ) {
                w += em * fs;
                continue;
            }
        }
        // fallback: heuristic 0.6.
        let _ = measured;
        w += fs * 0.6;
    }
    w
}

/// 늘림 괄호 렌더링
fn draw_stretch_bracket(
    svg: &mut String,
    bracket: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: &str,
    fs: f64,
    font_family: &str,
) {
    let mid_x = x + w / 2.0;
    let stroke_w = fs * 0.04;

    match bracket {
        "(" => {
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x + w * 0.2, y,
                x, y + h / 2.0,
                mid_x + w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        ")" => {
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - w * 0.2, y,
                x + w, y + h / 2.0,
                mid_x - w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        "[" => {
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x + w * 0.2, y,
                mid_x - w * 0.2, y,
                mid_x - w * 0.2, y + h,
                mid_x + w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        "]" => {
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - w * 0.2, y,
                mid_x + w * 0.2, y,
                mid_x + w * 0.2, y + h,
                mid_x - w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        "{" => {
            let qh = h / 4.0;
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x + w * 0.2, y,
                mid_x - w * 0.1, y,
                mid_x - w * 0.1, y + qh,
                mid_x - w * 0.1, y + qh * 2.0,
                mid_x - w * 0.3, y + qh * 2.0,
                mid_x - w * 0.1, y + qh * 2.0,
                mid_x - w * 0.1, y + qh * 3.0,
                mid_x - w * 0.1, y + h,
                mid_x + w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        "}" => {
            let qh = h / 4.0;
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - w * 0.2, y,
                mid_x + w * 0.1, y,
                mid_x + w * 0.1, y + qh,
                mid_x + w * 0.1, y + qh * 2.0,
                mid_x + w * 0.3, y + qh * 2.0,
                mid_x + w * 0.1, y + qh * 2.0,
                mid_x + w * 0.1, y + qh * 3.0,
                mid_x + w * 0.1, y + h,
                mid_x - w * 0.2, y + h,
                color, stroke_w,
            ));
        }
        "|" => {
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x, y, mid_x, y + h, color, stroke_w,
            ));
        }
        _ => {
            // 기타 문자 (⌈, ⌉, ⌊, ⌋ 등)은 텍스트로 렌더링
            let esc = escape_xml(bracket);
            svg.push_str(&format!(
                "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"{:.2}\" fill=\"{}\" text-anchor=\"middle\"{}>{}</text>\n",
                mid_x, y + h * 0.7, h, color, font_family, esc,
            ));
        }
    }
}

/// 장식 렌더링
fn draw_decoration(
    svg: &mut String,
    kind: DecoKind,
    mid_x: f64,
    y: f64,
    width: f64,
    color: &str,
    fs: f64,
) {
    let stroke_w = fs * 0.03;
    let half_w = width / 2.0;

    match kind {
        DecoKind::Hat => {
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w * 0.6, y + fs * 0.15,
                mid_x, y,
                mid_x + half_w * 0.6, y + fs * 0.15,
                color, stroke_w,
            ));
        }
        DecoKind::Bar | DecoKind::Overline => {
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w, y + fs * 0.05,
                mid_x + half_w, y + fs * 0.05,
                color, stroke_w,
            ));
        }
        DecoKind::Vec => {
            // 오른쪽 화살표
            let arrow_y = y + fs * 0.05;
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w, arrow_y,
                mid_x + half_w, arrow_y,
                color, stroke_w,
            ));
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x + half_w - fs * 0.1, arrow_y - fs * 0.06,
                mid_x + half_w, arrow_y,
                mid_x + half_w - fs * 0.1, arrow_y + fs * 0.06,
                color, stroke_w,
            ));
        }
        DecoKind::Tilde => {
            let ty = y + fs * 0.08;
            svg.push_str(&format!(
                "<path d=\"M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w * 0.6, ty,
                mid_x - half_w * 0.2, ty - fs * 0.08,
                mid_x, ty,
                mid_x + half_w * 0.2, ty + fs * 0.08,
                mid_x + half_w * 0.6, ty,
                color, stroke_w,
            ));
        }
        DecoKind::Dot => {
            svg.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>\n",
                mid_x,
                y + fs * 0.06,
                fs * 0.03,
                color,
            ));
        }
        DecoKind::DDot => {
            let gap = fs * 0.1;
            svg.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>\n",
                mid_x - gap,
                y + fs * 0.06,
                fs * 0.03,
                color,
            ));
            svg.push_str(&format!(
                "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"{}\"/>\n",
                mid_x + gap,
                y + fs * 0.06,
                fs * 0.03,
                color,
            ));
        }
        DecoKind::Underline | DecoKind::Under => {
            // 아래선은 y 위치를 body 아래로 옮김 (여기서는 위치만 표시)
            // 실제로는 body 높이를 알아야 하지만, 여기서는 근사치 사용
            let uy = y + fs * 1.1;
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w, uy, mid_x + half_w, uy, color, stroke_w,
            ));
        }
        _ => {
            // Check, Acute, Grave, Dyad, Arch, StrikeThrough 등 간략 처리
            svg.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\"/>\n",
                mid_x - half_w * 0.5, y + fs * 0.1,
                mid_x + half_w * 0.5, y + fs * 0.1,
                color, stroke_w,
            ));
        }
    }
}

/// XML 특수문자 이스케이프
fn escape_xml(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

/// 수식 color(0x00BBGGRR)를 SVG 색상 문자열(#rrggbb)로 변환
pub fn eq_color_to_svg(color: u32) -> String {
    let r = color & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = (color >> 16) & 0xFF;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::equation::layout::EqLayout;
    use crate::renderer::equation::parser::EqParser;
    use crate::renderer::equation::tokenizer::tokenize;

    fn render_eq(script: &str) -> String {
        let tokens = tokenize(script);
        let ast = EqParser::new(tokens).parse();
        let layout = EqLayout::new(20.0).layout(&ast);
        render_equation_svg(&layout, "#000000", 20.0, "")
    }

    #[test]
    fn test_simple_text_svg() {
        let svg = render_eq("abc");
        assert!(svg.contains("<text"));
        assert!(svg.contains("abc"));
    }

    #[test]
    fn test_fraction_svg() {
        let svg = render_eq("a over b");
        assert!(svg.contains("<text")); // 분자/분모 텍스트
        assert!(svg.contains("<line")); // 분수선
    }

    #[test]
    fn test_atop_svg_has_no_fraction_line() {
        let svg = render_eq("a atop b");
        assert!(svg.contains("<text"));
        assert!(!svg.contains("<line"));
        let y_values: Vec<&str> = svg
            .lines()
            .filter_map(|line| line.split(" y=\"").nth(1))
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert_eq!(
            y_values.len(),
            2,
            "ATOP은 위/아래 텍스트 2개를 렌더링해야 함: {}",
            svg
        );
        assert_ne!(
            y_values[0], y_values[1],
            "ATOP은 두 항을 세로로 배치해야 함: {}",
            svg
        );
    }

    #[test]
    fn test_paren_svg() {
        // 텍스트 높이 파렌은 글리프로 렌더 (Task #283)
        let svg = render_eq("LEFT ( a RIGHT )");
        assert!(svg.contains("<text")); // 내용 + 글리프 파렌
        assert!(!svg.contains("<path")); // path 파렌 아님
    }

    #[test]
    fn test_paren_stretch_svg() {
        // 스트레치 파렌(분수 감쌈)은 path 유지 (Task #283)
        let svg = render_eq("LEFT ( a over b RIGHT )");
        assert!(svg.contains("<path")); // 스트레치 괄호
        assert!(svg.contains("<line")); // 분수선
    }

    #[test]
    fn test_eq01_svg() {
        let svg = render_eq(
            "평점=입찰가격평가~배점한도 TIMES LEFT ( {최저입찰가격} over {해당입찰가격} RIGHT )",
        );
        assert!(svg.contains("평점"));
        assert!(svg.contains("×")); // TIMES → ×
        assert!(svg.contains("<line")); // 분수선
        assert!(svg.contains("<path")); // 괄호
    }

    // Task #488: rm/it 폰트 스타일 적용 검증

    #[test]
    fn test_default_text_is_italic() {
        // hwpeq 기본: 라틴 변수는 italic
        let svg = render_eq("K");
        assert!(
            svg.contains("font-style=\"italic\""),
            "기본 변수는 italic: {}",
            svg
        );
    }

    #[test]
    fn test_rm_disables_italic() {
        // rm K (직립체): italic 미적용
        let svg = render_eq("rm K");
        assert!(
            !svg.contains("font-style=\"italic\""),
            "rm 적용 시 italic 없음: {}",
            svg
        );
        assert!(svg.contains(">K<"));
    }

    #[test]
    fn test_rm_prefix_form_disables_italic() {
        // rmK (공백 없는 prefix 형태): italic 미적용
        let svg = render_eq("rmK");
        assert!(
            !svg.contains("font-style=\"italic\""),
            "rmK 적용 시 italic 없음: {}",
            svg
        );
        assert!(svg.contains(">K<"));
        // rm prefix 자체가 토큰으로 분리되었으므로 raw "rmK" 가 SVG 텍스트로 남지 않아야 함
        assert!(!svg.contains(">rmK<"));
    }

    #[test]
    fn test_rm_compound_chemical_symbol() {
        // rmCa: 두 글자 화학 기호도 한 토큰으로 묶여 italic 미적용
        let svg = render_eq("rmCa");
        assert!(!svg.contains("font-style=\"italic\""));
        assert!(svg.contains(">Ca<"));
    }

    #[test]
    fn test_it_keeps_italic() {
        // it K (이탤릭 명시): italic 적용
        let svg = render_eq("it K");
        assert!(svg.contains("font-style=\"italic\""));
        assert!(svg.contains(">K<"));
    }

    #[test]
    fn test_cjk_never_italic() {
        // 한글은 default italic=true 영역에서도 italic 미적용
        let svg = render_eq("평점");
        assert!(
            !svg.contains("font-style=\"italic\""),
            "CJK는 italic 미적용: {}",
            svg
        );
        assert!(svg.contains("평점"));
    }
}
