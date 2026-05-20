//! Contents/section{N}.xml — Section 본문 직렬화
//!
//! Stage 2 (#182): 기존 템플릿 기반 구조를 유지하되, `<hp:p>` 와 `<hp:run>` 의 속성을
//! IR에서 가져와 동적으로 생성한다. `secPr`/`pagePr`/`grid` 등 섹션 정의는 템플릿 보존
//! (IR에 대응 필드가 더 담길 때까지 점진적으로 동적화 예정).
//!
//! Stage #177 (2026-04-18): `<hp:lineseg>` 직렬화를 IR 기반으로 전환.
//! `Paragraph.line_segs` 의 6개 필드(line_height, text_height, baseline_distance,
//! line_spacing, column_start/segment_width, tag)를 그대로 출력하여 **원본 lineseg 값
//! 보존**. rhwp 는 자신의 문서에서 새로 부정확한 값을 생산하지 않는다.
//!
//! IR 매핑 관행:
//!   - `section.paragraphs` 여러 개 = 하드 문단 경계 (`<hp:p>` 여러 개)
//!   - `paragraph.text` 내 `\n` = 소프트 라인브레이크 (`<hp:lineBreak/>`, 같은 문단 내)
//!   - `paragraph.text` 내 `\t` = 탭 (`<hp:tab width=... leader="0" type="1"/>`)
//!   - `paragraph.para_shape_id` → `<hp:p paraPrIDRef>`
//!   - `paragraph.style_id` → `<hp:p styleIDRef>`
//!   - `paragraph.column_type` → `<hp:p pageBreak/columnBreak>`
//!   - `paragraph.char_shapes[0].char_shape_id` → 첫 `<hp:run charPrIDRef>`
//!   - `paragraph.line_segs[i]` → 각 `<hp:lineseg>` 속성 (6개 필드 그대로 출력)

use crate::model::control::{Control, Equation};
use crate::model::document::{Document, Section};
use crate::model::paragraph::{ColumnBreakType, LineSeg, Paragraph};
use crate::model::shape::{HorzAlign, HorzRelTo, ShapeObject, TextWrap, VertAlign, VertRelTo};

use super::context::SerializeContext;
use super::picture::write_picture;
use super::shape::{write_line, write_rect};
use super::table::write_table;
use super::utils::xml_escape;
use super::SerializeError;

const EMPTY_SECTION_XML: &str = include_str!("templates/empty_section0.xml");
const TEXT_SLOT: &str = "<hp:t/>";
const LINESEG_SLOT_OPEN: &str = "<hp:linesegarray>";
const LINESEG_SLOT_CLOSE: &str = "</hp:linesegarray>";
const PARA_CLOSE: &str = "</hp:p></hs:sec>";

// 템플릿 내 첫 <hp:p> 태그의 실제 문자열 (id="3121190098" 랜덤 해시 포함).
// 템플릿은 정적이므로 이 문자열이 고정 위치에 있음이 보장됨.
const TEMPLATE_FIRST_P_TAG: &str = r#"<hp:p id="3121190098" paraPrIDRef="0" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0">"#;
// 템플릿 내 <hp:run charPrIDRef="0"> 직후에 TEXT_SLOT 이 오는 패턴.
const TEMPLATE_RUN_BEFORE_TEXT: &str = r#"<hp:run charPrIDRef="0"><hp:t/>"#;

/// 레퍼런스 기준 줄 레이아웃 파라미터.
const VERT_STEP: u32 = 1600; // vertsize(1000) + spacing(600)
const LINE_FLAGS: u32 = 393216;
const HORZ_SIZE: u32 = 42520;
/// 탭 기본 폭 (한컴이 열면서 재계산하지만 초기값으로 필요).
const TAB_DEFAULT_WIDTH: u32 = 4000;

/// Stage 2 진입점. `ctx` 는 Stage 3+ 에서 파라미터 검증에 사용.
pub fn write_section(
    section: &Section,
    _doc: &Document,
    _index: usize,
    ctx: &mut SerializeContext,
) -> Result<Vec<u8>, SerializeError> {
    let mut vert_cursor: u32 = 0;

    let first_para = section.paragraphs.first();
    let (first_t, first_linesegs, first_advance) = match first_para {
        Some(p) => render_paragraph_parts(p, vert_cursor, ctx),
        None => render_paragraph_parts_for_text("", vert_cursor),
    };
    vert_cursor = first_advance;

    let mut out = EMPTY_SECTION_XML.replacen(TEXT_SLOT, &first_t, 1);
    out = replace_first_linesegs(&out, &first_linesegs);

    // 첫 문단 `<hp:p>` 태그를 IR 기반 속성으로 교체
    if let Some(p) = first_para {
        let new_p_tag = render_hp_p_open(p, 0);
        out = out.replacen(TEMPLATE_FIRST_P_TAG, &new_p_tag, 1);

        // 첫 문단의 텍스트용 <hp:run> 의 charPrIDRef 를 IR 기반으로 교체
        // 템플릿에서 TEXT_SLOT 이 있던 자리 바로 앞의 <hp:run charPrIDRef="0"> 패턴.
        let first_run_cs = first_run_char_shape_id(p);
        let new_run = format!(r#"<hp:run charPrIDRef="{}">"#, first_run_cs);
        let replacement = format!("{}{}", new_run, &first_t);
        // 이미 first_t 는 out 에 들어갔으므로 그 직전의 <hp:run charPrIDRef="0"> 만 변경
        let anchor = format!("{}{}", r#"<hp:run charPrIDRef="0">"#, &first_t);
        if out.contains(&anchor) {
            out = out.replacen(&anchor, &replacement, 1);
        }
    }

    // 추가 문단: `</hp:p></hs:sec>` 직전에 `<hp:p>` 요소를 삽입.
    if section.paragraphs.len() > 1 {
        let mut extra = String::new();
        for (idx, p) in section.paragraphs.iter().enumerate().skip(1) {
            let (t, linesegs, advance) = render_paragraph_parts(p, vert_cursor, ctx);
            vert_cursor = advance;
            let cs = first_run_char_shape_id(p);
            extra.push_str(&render_hp_p_open(p, idx as u32));
            extra.push_str(&format!(r#"<hp:run charPrIDRef="{}">"#, cs));
            extra.push_str(&t);
            extra.push_str(r#"</hp:run><hp:linesegarray>"#);
            extra.push_str(&linesegs);
            extra.push_str(r#"</hp:linesegarray></hp:p>"#);
        }
        out = out.replacen(PARA_CLOSE, &format!("</hp:p>{}</hs:sec>", extra), 1);
    }

    Ok(out.into_bytes())
}

/// IR의 Paragraph를 기반으로 `<hp:p>` 시작 태그를 생성.
///
/// `id` 는 문단 순서 기반(0, 1, 2, ...)로 할당한다. 한컴 샘플은 랜덤 해시도 쓰지만
/// 파서는 id 를 무시하므로 순차값으로 충분.
fn render_hp_p_open(p: &Paragraph, id: u32) -> String {
    let page_break = if matches!(p.column_type, ColumnBreakType::Page) {
        1
    } else {
        0
    };
    let column_break = if matches!(p.column_type, ColumnBreakType::Column) {
        1
    } else {
        0
    };
    format!(
        r#"<hp:p id="{}" paraPrIDRef="{}" styleIDRef="{}" pageBreak="{}" columnBreak="{}" merged="0">"#,
        id, p.para_shape_id, p.style_id, page_break, column_break,
    )
}

/// 문단 첫 run 의 charPrIDRef. IR의 `char_shapes[0].char_shape_id` 사용.
/// 비어있으면 0 (기본 글자모양) 반환.
fn first_run_char_shape_id(p: &Paragraph) -> u32 {
    p.char_shapes.first().map(|r| r.char_shape_id).unwrap_or(0)
}

/// Paragraph 하나를 (`<hp:t>` XML, lineseg XML, 다음 vert_cursor)로 변환.
///
/// `<hp:lineseg>` 출력 원칙 (#177):
/// - `para.line_segs` 가 비어있지 않으면 **IR 값 그대로 출력**
/// - 비어있을 때만 텍스트 내 `\n` 기반으로 fallback 생성 (빈 문단·`Document::default()` 호환)
fn render_paragraph_parts(
    para: &Paragraph,
    vert_start: u32,
    ctx: &mut SerializeContext,
) -> (String, String, u32) {
    let t_xml = render_run_content(para, ctx);

    if !para.line_segs.is_empty() {
        // IR 기반 출력 — 원본 lineseg 값 보존 (#177)
        let linesegs = render_lineseg_array_from_ir(&para.line_segs);
        let vert_end = next_vert_cursor_from_ir(&para.line_segs, vert_start);
        (t_xml, linesegs, vert_end)
    } else {
        // Fallback — IR에 line_segs 가 없으면 기존 생성 로직 유지
        let (linesegs, vert_end) = render_lineseg_array_fallback(&para.text, vert_start);
        (t_xml, linesegs, vert_end)
    }
}

/// IR 없이 텍스트만 있을 때 `<hp:t>` 와 fallback lineseg 생성.
/// `write_section` 이 `first_para == None` 인 경우를 위해 유지.
fn render_paragraph_parts_for_text(text: &str, vert_start: u32) -> (String, String, u32) {
    let t_xml = render_hp_t_content(text);
    let (linesegs, vert_end) = render_lineseg_array_fallback(text, vert_start);
    (t_xml, linesegs, vert_end)
}

/// `<hp:t>...</hp:t>` 본문 생성 — 탭/소프트브레이크/XML escape 포함.
fn render_hp_t_content(text: &str) -> String {
    render_hp_t_content_with_tabs(text, &[], &mut 0)
}

/// `render_hp_t_content` 의 IR 보존형 — `tab_extended` 슬라이스에서 각 `\t`
/// 의 width / leader / type 를 가져와 emit 한다. `tab_idx_cumulative` 는
/// 호출자가 여러 fragment 에 걸쳐 추적하는 누적 인덱스 (PARA_TEXT 의 N 번째
/// TAB 이 tab_extended[N] 에 대응).
///
/// HWP TAB extension 14바이트 레이아웃 (PARA_TEXT 의 0x0009 직후):
///   bytes 0-3 (u32): width (HWPUNIT)
///   byte 4   (u8):  leader (0=NONE, 1=DOT, 2=DASH, 3=LINE, 4=BOLD_DOT,
///                            5=DOUBLE_LINE, 6=DOTTED_LINE)
///   byte 5   (u8):  type (0=LEFT, 1=RIGHT, 2=CENTER, 3=DECIMAL — HWP 바이너리 기준)
///   bytes 6-13: reserved (auto-tab flag, dot pos 등 — 현재 미사용)
fn render_hp_t_content_with_tabs(
    text: &str,
    tab_extended: &[[u16; 7]],
    tab_idx_cumulative: &mut usize,
) -> String {
    let mut t_xml = String::from("<hp:t>");
    let mut buf = String::new();
    for c in text.chars() {
        match c {
            '\t' => {
                flush_buf(&mut t_xml, &mut buf);
                let (width, leader, ttype) =
                    if let Some(ext) = tab_extended.get(*tab_idx_cumulative) {
                        let w = (ext[0] as u32) | ((ext[1] as u32) << 16);
                        let leader = (ext[2] & 0xFF) as u8;
                        let ttype = ((ext[2] >> 8) & 0xFF) as u8;
                        (w, leader, ttype)
                    } else {
                        (TAB_DEFAULT_WIDTH, 0, 1)
                    };
                *tab_idx_cumulative += 1;
                t_xml.push_str(&format!(
                    r#"<hp:tab width="{}" leader="{}" type="{}"/>"#,
                    width, leader, ttype
                ));
            }
            '\n' => {
                flush_buf(&mut t_xml, &mut buf);
                t_xml.push_str("<hp:lineBreak/>");
            }
            c if (c as u32) < 0x20 => { /* 기타 제어문자 무시 */ }
            c => buf.push(c),
        }
    }
    flush_buf(&mut t_xml, &mut buf);
    t_xml.push_str("</hp:t>");
    t_xml
}

/// Emit one or more `<hp:run charPrIDRef="X">…</hp:run>` for a paragraph,
/// splitting at every char_shape transition so per-character styles
/// (underline, bold, italic, color) are preserved AND interleaving inline
/// controls (equations, sub-tables, fields) the same way `render_run_content`
/// does. Binary HWP stores `CharShapeRef.start_pos` in UTF-16 units; we walk
/// text char-by-char, emit any pending control slots in the gaps before each
/// char, and switch the active run when the resolved char_shape changes.
pub(crate) fn render_runs_split_by_char_shapes(
    para: &Paragraph,
    ctx: &mut SerializeContext,
) -> String {
    use crate::model::paragraph::CharShapeRef;
    let cs_refs: &[CharShapeRef] = &para.char_shapes;
    if cs_refs.len() <= 1 {
        let cs = cs_refs.first().map(|r| r.char_shape_id).unwrap_or(0);
        let inner = render_run_content(para, ctx);
        let inner = if inner.is_empty() {
            String::from("<hp:t/>")
        } else {
            inner
        };
        return format!(r#"<hp:run charPrIDRef="{}">{}</hp:run>"#, cs, inner);
    }

    let resolve_cs = |pos: u32| -> u32 {
        let mut active = cs_refs[0].char_shape_id;
        for r in cs_refs {
            if r.start_pos <= pos {
                active = r.char_shape_id;
            } else {
                break;
            }
        }
        active
    };

    // NOTE: iterate ALL controls (not filtered) so non-inline ones
    // (ColDef/SectionDef/Header/Footnote/AutoNum/etc.) advance the PARA_TEXT
    // position without consuming an inline slot. Previously we built a
    // filtered `slots` list and assumed each 8-unit gap = 1 slot, which
    // shifted every inline control before its preceding text whenever a
    // non-inline control appeared earlier in the paragraph (e.g. cells with
    // a leading `cold` ColDef caused equations to be emitted ahead of the
    // text they followed in the binary).
    let all_controls = &para.controls;

    // Emit state. We keep <hp:t> open while accumulating text; switch
    // runs require closing both <hp:t> and <hp:run>.
    let mut out = String::new();
    let mut buf = String::new();
    let mut t_open = false;
    let mut current_cs = cs_refs[0].char_shape_id;
    out.push_str(&format!(r#"<hp:run charPrIDRef="{}">"#, current_cs));

    let open_t = |out: &mut String, t_open: &mut bool| {
        if !*t_open {
            out.push_str("<hp:t>");
            *t_open = true;
        }
    };
    let flush_text = |out: &mut String, buf: &mut String, t_open: &mut bool| {
        if !buf.is_empty() {
            if !*t_open {
                out.push_str("<hp:t>");
                *t_open = true;
            }
            for c in buf.chars() {
                match c {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    _ => out.push(c),
                }
            }
            buf.clear();
        }
    };
    let close_t_if_open = |out: &mut String, t_open: &mut bool| {
        if *t_open {
            out.push_str("</hp:t>");
            *t_open = false;
        }
    };

    let mut switch_run = |out: &mut String,
                          buf: &mut String,
                          t_open: &mut bool,
                          current_cs: &mut u32,
                          new_cs: u32| {
        if new_cs == *current_cs {
            return;
        }
        flush_text(out, buf, t_open);
        close_t_if_open(out, t_open);
        out.push_str("</hp:run>");
        *current_cs = new_cs;
        out.push_str(&format!(r#"<hp:run charPrIDRef="{}">"#, new_cs));
    };

    let mut tab_idx_cumulative = 0usize;
    let mut ctrl_idx = 0usize;
    let mut expected_utf16_pos = 0u32;
    let mut phantom_slots_to_consume = 0usize;

    for (idx, c) in para.text.chars().enumerate() {
        let char_pos = para
            .char_offsets
            .get(idx)
            .copied()
            .unwrap_or(expected_utf16_pos);

        // Walk every 8-unit gap before this char position. Each gap is one
        // extended-control slot in PARA_TEXT (in `para.controls[]` order).
        // Render the next control if it's inline-renderable; otherwise just
        // skip it (still advancing the position) so subsequent inline slots
        // line up with their actual binary positions.
        while char_pos >= expected_utf16_pos.saturating_add(8) {
            if phantom_slots_to_consume > 0 {
                phantom_slots_to_consume -= 1;
            } else if ctrl_idx < all_controls.len() {
                let ctrl = &all_controls[ctrl_idx];
                if is_hwpx_inline_slot(ctrl) {
                    let slot_cs = resolve_cs(expected_utf16_pos);
                    switch_run(&mut out, &mut buf, &mut t_open, &mut current_cs, slot_cs);
                    flush_text(&mut out, &mut buf, &mut t_open);
                    close_t_if_open(&mut out, &mut t_open);
                    if matches!(ctrl, Control::Field(_)) {
                        phantom_slots_to_consume += 1;
                    } else {
                        render_control_slot(&mut out, ctrl, ctx);
                    }
                }
                // non-inline controls (ColDef, SectionDef, Header, Footnote,
                // AutoNum, …) silently consume their 8-unit slot.
                ctrl_idx += 1;
            }
            expected_utf16_pos = expected_utf16_pos.saturating_add(8);
        }

        let cs_here = resolve_cs(char_pos);
        switch_run(&mut out, &mut buf, &mut t_open, &mut current_cs, cs_here);

        match c {
            '\t' => {
                flush_text(&mut out, &mut buf, &mut t_open);
                open_t(&mut out, &mut t_open);
                let (width, leader, ttype) =
                    if let Some(ext) = para.tab_extended.get(tab_idx_cumulative) {
                        let w = (ext[0] as u32) | ((ext[1] as u32) << 16);
                        let leader = (ext[2] & 0xFF) as u8;
                        let ttype = ((ext[2] >> 8) & 0xFF) as u8;
                        (w, leader, ttype)
                    } else {
                        (0u32, 0u8, 1u8)
                    };
                tab_idx_cumulative += 1;
                out.push_str(&format!(
                    r#"<hp:tab width="{}" leader="{}" type="{}"/>"#,
                    width, leader, ttype
                ));
            }
            '\n' => {
                flush_text(&mut out, &mut buf, &mut t_open);
                open_t(&mut out, &mut t_open);
                out.push_str("<hp:lineBreak/>");
            }
            c if (c as u32) < 0x20 => { /* drop */ }
            c => buf.push(c),
        }

        let width = char_utf16_width(c);
        if char_pos >= expected_utf16_pos {
            expected_utf16_pos = char_pos.saturating_add(width);
        } else {
            expected_utf16_pos = expected_utf16_pos.saturating_add(width);
        }
    }

    // Drain any trailing controls not yet emitted (inline-only).
    while ctrl_idx < all_controls.len() {
        let ctrl = &all_controls[ctrl_idx];
        if is_hwpx_inline_slot(ctrl) && !matches!(ctrl, Control::Field(_)) {
            let slot_cs = resolve_cs(expected_utf16_pos);
            switch_run(&mut out, &mut buf, &mut t_open, &mut current_cs, slot_cs);
            flush_text(&mut out, &mut buf, &mut t_open);
            close_t_if_open(&mut out, &mut t_open);
            render_control_slot(&mut out, ctrl, ctx);
        }
        ctrl_idx += 1;
    }

    flush_text(&mut out, &mut buf, &mut t_open);
    if !t_open {
        // Run with no text content (just controls) — emit empty <hp:t/> for
        // schema validity.
        out.push_str("<hp:t/>");
    } else {
        out.push_str("</hp:t>");
    }
    out.push_str("</hp:run>");
    out
}

fn push_xml_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Paragraph의 본문 run 콘텐츠를 `<hp:t>`와 인라인 컨트롤 XML로 직렬화한다.
pub(crate) fn render_run_content(para: &Paragraph, ctx: &mut SerializeContext) -> String {
    let slot_count = inferred_control_slot_count(para);

    // Inline-renderable controls only — controls list may include items that
    // don't get embedded in a run's text flow (column/section pseudo-ctrls),
    // and may be MISSING items that DO occupy slots in PARA_TEXT but aren't
    // re-emitted (e.g. FIELD_END consumes 8 utf-16 units in PARA_TEXT but
    // is not stored in `controls[]`). The inline interleaving path below
    // tolerates both directions: extras at end go after the text, missing
    // slots are silently consumed when `char_offsets` shows a gap with no
    // matching control.
    // See `render_runs_split_by_char_shapes` for the rationale: iterate ALL
    // controls (not the filtered inline subset) so non-inline ones consume
    // their 8-unit PARA_TEXT slot without displacing later inline emissions.
    let all_controls = &para.controls;
    let any_inline = all_controls.iter().any(|c| is_hwpx_inline_slot(c));

    let _ = slot_count; // retained for diagnostic clarity if re-enabled
    let mut tab_idx_cumulative = 0usize;
    if !any_inline
        && para
            .char_offsets
            .iter()
            .enumerate()
            .all(|(i, p)| *p == i as u32)
    {
        return render_hp_t_content_with_tabs(
            &para.text,
            &para.tab_extended,
            &mut tab_idx_cumulative,
        );
    }

    let mut out = String::new();
    let mut text_buf = String::new();
    let mut ctrl_idx = 0usize;
    let mut expected_utf16_pos = 0u32;
    let mut phantom_slots_to_consume = 0usize;

    for (idx, c) in para.text.chars().enumerate() {
        let char_pos = para
            .char_offsets
            .get(idx)
            .copied()
            .unwrap_or(expected_utf16_pos);
        while char_pos >= expected_utf16_pos.saturating_add(8) {
            if phantom_slots_to_consume > 0 {
                phantom_slots_to_consume -= 1;
            } else if ctrl_idx < all_controls.len() {
                let ctrl = &all_controls[ctrl_idx];
                if is_hwpx_inline_slot(ctrl) {
                    flush_text_fragment_with_tabs(
                        &mut out,
                        &mut text_buf,
                        &para.tab_extended,
                        &mut tab_idx_cumulative,
                    );
                    if matches!(ctrl, Control::Field(_)) {
                        // HWP fields occupy BEGIN and END extended-character
                        // slots in PARA_TEXT but only one entry in `controls`.
                        // Mark the END slot phantom so it's silently consumed.
                        phantom_slots_to_consume += 1;
                    } else {
                        render_control_slot(&mut out, ctrl, ctx);
                    }
                }
                // non-inline controls silently consume their slot.
                ctrl_idx += 1;
            }
            expected_utf16_pos = expected_utf16_pos.saturating_add(8);
        }

        text_buf.push(c);
        let width = char_utf16_width(c);
        if char_pos >= expected_utf16_pos {
            expected_utf16_pos = char_pos.saturating_add(width);
        } else {
            expected_utf16_pos = expected_utf16_pos.saturating_add(width);
        }
    }

    flush_text_fragment_with_tabs(
        &mut out,
        &mut text_buf,
        &para.tab_extended,
        &mut tab_idx_cumulative,
    );
    while ctrl_idx < all_controls.len() {
        let ctrl = &all_controls[ctrl_idx];
        if is_hwpx_inline_slot(ctrl) && !matches!(ctrl, Control::Field(_)) {
            render_control_slot(&mut out, ctrl, ctx);
        }
        ctrl_idx += 1;
    }

    if out.is_empty() {
        render_hp_t_content("")
    } else {
        out
    }
}

fn flush_text_fragment_with_tabs(
    out: &mut String,
    text_buf: &mut String,
    tab_extended: &[[u16; 7]],
    tab_idx_cumulative: &mut usize,
) {
    if !text_buf.is_empty() {
        out.push_str(&render_hp_t_content_with_tabs(
            text_buf,
            tab_extended,
            tab_idx_cumulative,
        ));
        text_buf.clear();
    }
}

fn inferred_control_slot_count(para: &Paragraph) -> usize {
    let text_units: u32 = para.text.chars().map(char_utf16_width).sum();
    let from_char_count = para.char_count.saturating_sub(1).saturating_sub(text_units) / 8;

    let mut from_offsets = 0u32;
    let mut expected = 0u32;
    for (idx, c) in para.text.chars().enumerate() {
        let pos = para.char_offsets.get(idx).copied().unwrap_or(expected);
        if pos > expected {
            from_offsets += (pos - expected) / 8;
        }
        expected = pos.max(expected).saturating_add(char_utf16_width(c));
    }

    from_char_count.max(from_offsets) as usize
}

fn is_hwpx_inline_slot(control: &Control) -> bool {
    matches!(
        control,
        Control::Table(_)
            | Control::Shape(_)
            | Control::Picture(_)
            | Control::CharOverlap(_)
            | Control::Ruby(_)
            | Control::Equation(_)
            | Control::Field(_)
            | Control::Form(_)
    )
}

fn flush_text_fragment(out: &mut String, text_buf: &mut String) {
    if !text_buf.is_empty() {
        out.push_str(&render_hp_t_content(text_buf));
        text_buf.clear();
    }
}

fn render_control_slot(out: &mut String, control: &Control, ctx: &mut SerializeContext) {
    match control {
        Control::Equation(eq) => out.push_str(&render_equation(eq)),
        Control::Picture(pic) => {
            if let Some(xml) = capture_writer(|w| write_picture(w, pic, ctx)) {
                out.push_str(&xml);
            }
        }
        Control::Table(tbl) => {
            if let Some(xml) = capture_writer(|w| write_table(w, tbl, ctx)) {
                out.push_str(&xml);
            }
        }
        Control::Shape(shape) => render_shape_slot(out, shape.as_ref(), ctx),
        // Field/Form/Ruby/CharOverlap/Footnote/Endnote — Stage TBD.
        _ => {}
    }
}

fn render_shape_slot(out: &mut String, shape: &ShapeObject, ctx: &mut SerializeContext) {
    match shape {
        ShapeObject::Rectangle(r) => {
            if let Some(xml) = capture_writer(|w| write_rect(w, r, ctx)) {
                out.push_str(&xml);
            }
        }
        ShapeObject::Line(l) => {
            if let Some(xml) = capture_writer(|w| write_line(w, l, ctx)) {
                out.push_str(&xml);
            }
        }
        ShapeObject::Picture(p) => {
            if let Some(xml) = capture_writer(|w| write_picture(w, p, ctx)) {
                out.push_str(&xml);
            }
        }
        // Group/Arc/Polygon/Curve/Chart/Ole — Stage TBD.
        _ => {}
    }
}

/// 임시 quick_xml::Writer 에 씌워 호출하고, 성공 시 UTF-8 문자열로 반환.
/// 실패 (직렬화 에러) 시 None — 호출처에서 빈 출력으로 처리.
fn capture_writer<F>(f: F) -> Option<String>
where
    F: FnOnce(&mut quick_xml::Writer<&mut Vec<u8>>) -> Result<(), SerializeError>,
{
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = quick_xml::Writer::new(&mut buf);
        if f(&mut w).is_err() {
            return None;
        }
    }
    String::from_utf8(buf).ok()
}

fn render_equation(eq: &Equation) -> String {
    let c = &eq.common;
    let id = c.instance_id.to_string();
    let z_order = c.z_order.to_string();
    let version = xml_escape(&eq.version_info);
    let baseline = eq.baseline.to_string();
    let text_color = color_ref_to_hwpx(eq.color);
    let base_unit = eq.font_size.to_string();
    let font = xml_escape(&eq.font_name);
    let script = xml_escape(&eq.script);
    let width = c.width.to_string();
    let height = c.height.to_string();
    let treat = if c.treat_as_char { "1" } else { "0" };
    let vert_offset = c.vertical_offset.to_string();
    let horz_offset = c.horizontal_offset.to_string();
    let margin_left = c.margin.left.to_string();
    let margin_right = c.margin.right.to_string();
    let margin_top = c.margin.top.to_string();
    let margin_bottom = c.margin.bottom.to_string();

    format!(
        r#"<hp:equation id="{id}" zOrder="{z_order}" numberingType="EQUATION" textWrap="{}" textFlow="BOTH_SIDES" lock="0" dropcapstyle="None" instid="{id}" version="{version}" baseLine="{baseline}" textColor="{text_color}" baseUnit="{base_unit}" font="{font}"><hp:script>{script}</hp:script><hp:sz width="{width}" widthRelTo="ABSOLUTE" height="{height}" heightRelTo="ABSOLUTE"/><hp:pos treatAsChar="{treat}" affectLSpacing="0" flowWithText="1" allowOverlap="0" holdAnchorAndSO="0" vertRelTo="{}" horzRelTo="{}" vertAlign="{}" horzAlign="{}" vertOffset="{vert_offset}" horzOffset="{horz_offset}"/><hp:outMargin left="{margin_left}" right="{margin_right}" top="{margin_top}" bottom="{margin_bottom}"/></hp:equation>"#,
        text_wrap_to_hwpx(c.text_wrap),
        vert_rel_to_hwpx(c.vert_rel_to),
        horz_rel_to_hwpx(c.horz_rel_to),
        vert_align_to_hwpx(c.vert_align),
        horz_align_to_hwpx(c.horz_align),
    )
}

fn char_utf16_width(c: char) -> u32 {
    if c == '\t' {
        8
    } else if (c as u32) > 0xFFFF {
        2
    } else {
        1
    }
}

fn color_ref_to_hwpx(color: u32) -> String {
    if color == 0xFFFFFFFF {
        return "none".to_string();
    }

    let r = color & 0xFF;
    let g = (color >> 8) & 0xFF;
    let b = (color >> 16) & 0xFF;
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn text_wrap_to_hwpx(wrap: TextWrap) -> &'static str {
    match wrap {
        TextWrap::Square => "SQUARE",
        TextWrap::Tight => "TIGHT",
        TextWrap::Through => "THROUGH",
        TextWrap::TopAndBottom => "TOP_AND_BOTTOM",
        TextWrap::BehindText => "BEHIND_TEXT",
        TextWrap::InFrontOfText => "IN_FRONT_OF_TEXT",
    }
}

fn vert_rel_to_hwpx(rel: VertRelTo) -> &'static str {
    match rel {
        VertRelTo::Paper => "PAPER",
        VertRelTo::Page => "PAGE",
        VertRelTo::Para => "PARA",
    }
}

fn horz_rel_to_hwpx(rel: HorzRelTo) -> &'static str {
    match rel {
        HorzRelTo::Paper => "PAPER",
        HorzRelTo::Page => "PAGE",
        HorzRelTo::Column => "COLUMN",
        HorzRelTo::Para => "PARA",
    }
}

fn vert_align_to_hwpx(align: VertAlign) -> &'static str {
    match align {
        VertAlign::Top => "TOP",
        VertAlign::Center => "CENTER",
        VertAlign::Bottom => "BOTTOM",
        VertAlign::Inside => "INSIDE",
        VertAlign::Outside => "OUTSIDE",
    }
}

fn horz_align_to_hwpx(align: HorzAlign) -> &'static str {
    match align {
        HorzAlign::Left => "LEFT",
        HorzAlign::Center => "CENTER",
        HorzAlign::Right => "RIGHT",
        HorzAlign::Inside => "INSIDE",
        HorzAlign::Outside => "OUTSIDE",
    }
}

/// IR의 `line_segs` 를 그대로 XML로 직렬화 (6개 필드 전부 IR 값 사용).
///
/// rhwp 는 자신의 문서에서 비표준 lineseg 를 **새로 생산하지 않는다**.
/// 원본 한컴 파일의 lineseg 값이 파서에 의해 `Paragraph.line_segs` 에 담겼다면,
/// 저장 시 그 값을 훼손 없이 보존한다.
fn render_lineseg_array_from_ir(segs: &[LineSeg]) -> String {
    let mut out = String::new();
    for seg in segs {
        out.push_str(&format!(
            r#"<hp:lineseg textpos="{}" vertpos="{}" vertsize="{}" textheight="{}" baseline="{}" spacing="{}" horzpos="{}" horzsize="{}" flags="{}"/>"#,
            seg.text_start,
            seg.vertical_pos,
            seg.line_height,
            seg.text_height,
            seg.baseline_distance,
            seg.line_spacing,
            seg.column_start,
            seg.segment_width,
            seg.tag,
        ));
    }
    out
}

/// IR 기반 다음 문단의 vert_start 계산 — 마지막 lineseg 의 vpos + lh 사용.
fn next_vert_cursor_from_ir(segs: &[LineSeg], vert_start: u32) -> u32 {
    if let Some(last) = segs.last() {
        // vertical_pos 는 섹션 시작 기준 절대값일 수도, 문단 기준 상대값일 수도 있음.
        // 현재 rhwp 는 섹션 절대값이므로 그대로 + lh 로 다음 커서 산출.
        let next = (last.vertical_pos as i64) + (last.line_height.max(0) as i64);
        if next > vert_start as i64 {
            next as u32
        } else {
            vert_start + VERT_STEP
        }
    } else {
        vert_start + VERT_STEP
    }
}

/// Fallback — IR 에 line_segs 가 없는 경우에만 사용 (예: `Document::default()`).
/// 과거 동작을 보존하기 위해 기존 정적값으로 lineseg 생성.
fn render_lineseg_array_fallback(text: &str, vert_start: u32) -> (String, u32) {
    let mut linesegs = String::new();
    push_lineseg_static(&mut linesegs, 0, vert_start);
    let mut utf16_pos: u32 = 0;
    let mut lines_in_para: u32 = 0;
    for c in text.chars() {
        let u16_len = c.len_utf16() as u32;
        match c {
            '\t' | '\n' => {
                utf16_pos += u16_len;
                if c == '\n' {
                    lines_in_para += 1;
                    push_lineseg_static(
                        &mut linesegs,
                        utf16_pos,
                        vert_start + lines_in_para * VERT_STEP,
                    );
                }
            }
            c if (c as u32) < 0x20 => {}
            _ => utf16_pos += u16_len,
        }
    }
    let vert_end = vert_start + (lines_in_para + 1) * VERT_STEP;
    (linesegs, vert_end)
}

fn flush_buf(t_xml: &mut String, buf: &mut String) {
    if !buf.is_empty() {
        t_xml.push_str(&xml_escape(buf));
        buf.clear();
    }
}

/// Fallback 전용 static lineseg 생성기 — IR에 값이 없을 때만 사용.
/// 주: 이 함수의 출력은 "명세 상 정확한 값" 이 아닌 정적 자리표이므로,
/// 호출 후 문서는 `DocumentCore::from_bytes` 의 `reflow_zero_height_paragraphs`
/// 또는 사용자의 `reflow_linesegs_on_demand` 로 재계산되어야 한다.
fn push_lineseg_static(out: &mut String, textpos: u32, vertpos: u32) {
    out.push_str(&format!(
        r#"<hp:lineseg textpos="{}" vertpos="{}" vertsize="1000" textheight="1000" baseline="850" spacing="600" horzpos="0" horzsize="{}" flags="{}"/>"#,
        textpos, vertpos, HORZ_SIZE, LINE_FLAGS,
    ));
}

fn replace_first_linesegs(xml: &str, new_inner: &str) -> String {
    let open = xml
        .find(LINESEG_SLOT_OPEN)
        .expect("template has linesegarray");
    let inner_start = open + LINESEG_SLOT_OPEN.len();
    let close_rel = xml[inner_start..]
        .find(LINESEG_SLOT_CLOSE)
        .expect("template has closing linesegarray");
    let inner_end = inner_start + close_rel;
    let mut out = String::with_capacity(xml.len() + new_inner.len());
    out.push_str(&xml[..inner_start]);
    out.push_str(new_inner);
    out.push_str(&xml[inner_end..]);
    out
}

// `TEMPLATE_RUN_BEFORE_TEXT` 는 패턴 인식용 상수로만 쓰이므로 명시 참조.
#[allow(dead_code)]
fn _template_anchor_hint() {
    let _ = TEMPLATE_RUN_BEFORE_TEXT;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::paragraph::{CharShapeRef, Paragraph};

    fn make_doc_with_paragraph(para: Paragraph) -> (Document, Section) {
        let mut section = Section::default();
        section.paragraphs.push(para);
        let mut doc = Document::default();
        doc.sections.push(section.clone());
        (doc, section)
    }

    #[test]
    fn hp_p_attrs_reflect_para_shape_id_and_style_id() {
        let mut para = Paragraph::default();
        para.para_shape_id = 7;
        para.style_id = 3;
        para.text = "hi".to_string();
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let bytes = write_section(&section, &doc, 0, &mut ctx).unwrap();
        let xml = std::str::from_utf8(&bytes).unwrap();
        assert!(
            xml.contains(r#"paraPrIDRef="7""#),
            "<hp:p> must reflect para_shape_id=7: {}",
            &xml[..200.min(xml.len())]
        );
        assert!(
            xml.contains(r#"styleIDRef="3""#),
            "<hp:p> must reflect style_id=3"
        );
    }

    #[test]
    fn hp_run_reflects_first_char_shape_id() {
        let mut para = Paragraph::default();
        para.text = "hello".to_string();
        para.char_shapes.push(CharShapeRef {
            start_pos: 0,
            char_shape_id: 42,
        });
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let bytes = write_section(&section, &doc, 0, &mut ctx).unwrap();
        let xml = std::str::from_utf8(&bytes).unwrap();
        assert!(
            xml.contains(r#"<hp:run charPrIDRef="42"><hp:t>hello</hp:t>"#),
            "first run must use char_shape_id 42, xml excerpt around <hp:t>: {:?}",
            xml.find("<hp:t>")
                .map(|i| &xml[i.saturating_sub(50)..(i + 50).min(xml.len())])
        );
    }

    #[test]
    fn page_break_paragraph_emits_attr() {
        let mut para = Paragraph::default();
        para.text = "p1".to_string();
        para.column_type = crate::model::paragraph::ColumnBreakType::Page;
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let bytes = write_section(&section, &doc, 0, &mut ctx).unwrap();
        let xml = std::str::from_utf8(&bytes).unwrap();
        assert!(
            xml.contains(r#"pageBreak="1""#),
            "pageBreak must be 1 for Page column_type"
        );
        assert!(xml.contains(r#"columnBreak="0""#));
    }

    #[test]
    fn default_paragraph_keeps_zero_attrs() {
        let mut para = Paragraph::default();
        para.text = "x".to_string();
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let bytes = write_section(&section, &doc, 0, &mut ctx).unwrap();
        let xml = std::str::from_utf8(&bytes).unwrap();
        assert!(xml.contains(r#"paraPrIDRef="0""#));
        assert!(xml.contains(r#"styleIDRef="0""#));
        // char_shapes 가 비어있으면 fallback 0
        assert!(xml.contains(r#"<hp:run charPrIDRef="0">"#));
    }

    #[test]
    fn additional_paragraphs_use_their_own_char_shape() {
        let mut p1 = Paragraph::default();
        p1.text = "first".to_string();
        p1.char_shapes.push(CharShapeRef {
            start_pos: 0,
            char_shape_id: 5,
        });
        let mut p2 = Paragraph::default();
        p2.text = "second".to_string();
        p2.para_shape_id = 2;
        p2.char_shapes.push(CharShapeRef {
            start_pos: 0,
            char_shape_id: 6,
        });
        let mut section = Section::default();
        section.paragraphs.push(p1);
        section.paragraphs.push(p2);
        let mut doc = Document::default();
        doc.sections.push(section.clone());
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_section(&section, &doc, 0, &mut ctx).unwrap()).unwrap();
        // 두 번째 문단: paraPrIDRef=2, charPrIDRef=6
        assert!(xml.contains(r#"paraPrIDRef="2""#));
        assert!(
            xml.matches(r#"charPrIDRef="6""#).count() >= 1,
            "second paragraph must emit charPrIDRef=6"
        );
    }

    // ---------- #177 Stage 2: IR 기반 lineseg 출력 ----------

    use crate::model::paragraph::LineSeg;

    #[test]
    fn task177_lineseg_reflects_ir_values() {
        // IR에 담긴 lineseg 값이 XML 속성에 그대로 반영되는지 확인.
        let mut para = Paragraph::default();
        para.text = "hello".to_string();
        para.line_segs.push(LineSeg {
            text_start: 0,
            vertical_pos: 5000,
            line_height: 1200,
            text_height: 1100,
            baseline_distance: 900,
            line_spacing: 700,
            column_start: 100,
            segment_width: 50000,
            tag: 999,
        });
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_section(&section, &doc, 0, &mut ctx).unwrap()).unwrap();
        assert!(xml.contains(r#"<hp:lineseg textpos="0" vertpos="5000" vertsize="1200" textheight="1100" baseline="900" spacing="700" horzpos="100" horzsize="50000" flags="999"/>"#),
            "lineseg must reflect IR values exactly, got XML: {}",
            &xml[xml.find("<hp:lineseg").unwrap_or(0)..(xml.find("<hp:lineseg").unwrap_or(0) + 200).min(xml.len())]);
    }

    #[test]
    fn task177_multiple_linesegs_preserved_in_order() {
        let mut para = Paragraph::default();
        para.text = "three\nlines\nhere".to_string();
        for (i, (tp, vp, lh)) in [(0u32, 0i32, 1000), (6, 1500, 1200), (12, 3100, 1100)]
            .iter()
            .enumerate()
        {
            let _ = i;
            para.line_segs.push(LineSeg {
                text_start: *tp,
                vertical_pos: *vp,
                line_height: *lh,
                text_height: *lh,
                baseline_distance: 850,
                line_spacing: 600,
                column_start: 0,
                segment_width: 42520,
                tag: 393216,
            });
        }
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_section(&section, &doc, 0, &mut ctx).unwrap()).unwrap();
        // 3개 lineseg 모두 출력되고 각각의 vertsize 값이 IR 값과 일치
        assert_eq!(xml.matches("<hp:lineseg ").count(), 3);
        assert!(xml.contains(r#"textpos="0" vertpos="0" vertsize="1000""#));
        assert!(xml.contains(r#"textpos="6" vertpos="1500" vertsize="1200""#));
        assert!(xml.contains(r#"textpos="12" vertpos="3100" vertsize="1100""#));
    }

    #[test]
    fn task177_fallback_used_when_ir_empty() {
        // IR 의 line_segs 가 비어있으면 fallback 경로로 정적 값 출력.
        let mut para = Paragraph::default();
        para.text = "a\nb".to_string(); // 소프트브레이크 1개 → fallback 은 lineseg 2개 생성
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_section(&section, &doc, 0, &mut ctx).unwrap()).unwrap();
        // 정적 fallback: vertsize=1000, textheight=1000, baseline=850, spacing=600
        assert!(xml.contains(r#"vertsize="1000""#));
        assert!(xml.contains(r#"baseline="850""#));
    }

    #[test]
    fn task177_ir_lineseg_takes_precedence_over_text() {
        // text 의 \n 개수가 2개(lineseg 3개 기대)이지만 IR의 line_segs 는 1개만 있음.
        // IR 기반 출력이 우선 — 1개만 출력돼야 함.
        let mut para = Paragraph::default();
        para.text = "a\nb\nc".to_string(); // 3줄
        para.line_segs.push(LineSeg {
            text_start: 0,
            vertical_pos: 0,
            line_height: 2000, // IR 값
            text_height: 2000,
            baseline_distance: 1700,
            line_spacing: 300,
            column_start: 0,
            segment_width: 40000,
            tag: 0,
        });
        let (doc, section) = make_doc_with_paragraph(para);
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_section(&section, &doc, 0, &mut ctx).unwrap()).unwrap();
        // IR 에 1개만 있으므로 lineseg 도 1개만 출력 (rhwp 는 원본 보존)
        assert_eq!(xml.matches("<hp:lineseg ").count(), 1);
        assert!(
            xml.contains(r#"vertsize="2000""#),
            "IR value 2000 must be used, not fallback 1000"
        );
    }
}
