//! Apply a template slot's styling to a (already id-remapped) source paragraph.
//!
//! Three paths:
//!   - bogi box: clone the template's framed box shell and drop the source's
//!     box content into its content cell;
//!   - balmun: split the leading "N." run into the slot's number style and the
//!     rest into the body style;
//!   - everything else: restamp the paragraph's outer (para/style) and its
//!     default char-shape runs to the slot, preserving emphasis runs.
//!
//! Line segments are kept throughout: the engine lays out from stored segments
//! (it does not reflow), and the template shares the source's column geometry.

use super::slot_catalog::Slot;
use crate::model::control::Control;
use crate::model::paragraph::{CharShapeRef, ColumnBreakType, Paragraph};
use crate::model::table::Table;
use crate::split::AtomKind;

/// Transform a source paragraph into its output form using `slot`.
pub fn apply_atom(src: Paragraph, atom: AtomKind, slot: &Slot) -> Paragraph {
    match atom {
        AtomKind::BogiBox => apply_box(src, slot),
        AtomKind::Balmun => apply_balmun(src, slot),
        _ => apply_role(src, slot),
    }
}

/// Reset page/column break flags only; line segments are left intact.
fn reset_breaks(p: &mut Paragraph) {
    p.column_type = ColumnBreakType::None;
    p.raw_break_type = 0;
}

fn default_cs(p: &Paragraph) -> u32 {
    p.char_shapes.first().map(|c| c.char_shape_id).unwrap_or(0)
}

/// Non-box, non-balmun: template owns paragraph para/style and the default
/// char-shape; emphasis runs (a run whose id differs from the default) survive.
fn apply_role(mut src: Paragraph, slot: &Slot) -> Paragraph {
    let has_positioned_content = src.controls.iter().any(|c| {
        matches!(
            c,
            Control::Picture(_) | Control::Table(_) | Control::Shape(_) | Control::Equation(_)
        )
    });
    let def = default_cs(&src);
    for cs in &mut src.char_shapes {
        if cs.char_shape_id == def {
            cs.char_shape_id = slot.char_shape_id;
        }
    }
    // Positioned paragraphs keep the source paraPr because tabs, margins, and
    // equations/pictures/tables are authored as one horizontal layout.
    if !has_positioned_content {
        src.para_shape_id = slot.para_shape_id;
        src.style_id = slot.style_id;
    }
    reset_breaks(&mut src);
    src
}

/// Balmun: leading "N." adopts the slot's number style, the rest the body
/// style. Falls back to the role path when the slot has no distinct body style
/// or the text has no question number.
fn apply_balmun(src: Paragraph, slot: &Slot) -> Paragraph {
    let Some(body_cs) = slot.body_char_shape_id else {
        return apply_role(src, slot);
    };
    let Some(qnum_chars) = qnum_prefix_len(&src.text) else {
        return apply_role(src, slot);
    };
    let qnum_cs = slot.char_shape_id;
    let def = default_cs(&src);

    let mut runs = vec![CharShapeRef {
        start_pos: 0,
        char_shape_id: qnum_cs,
    }];
    if let Some(&body_start) = src.char_offsets.get(qnum_chars) {
        runs.push(CharShapeRef {
            start_pos: body_start,
            char_shape_id: body_cs,
        });
        // Preserve emphasis runs (밑줄/볼드 etc.) after the question number.
        for r in &src.char_shapes {
            if r.start_pos > body_start && r.char_shape_id != def {
                runs.push(CharShapeRef {
                    start_pos: r.start_pos,
                    char_shape_id: r.char_shape_id,
                });
            }
        }
        runs.sort_by_key(|r| r.start_pos);
        runs.dedup_by_key(|r| r.start_pos);
    }

    let mut out = src;
    out.char_shapes = runs;
    out.para_shape_id = slot.para_shape_id;
    out.style_id = slot.style_id;
    reset_breaks(&mut out);
    out
}

/// Bogi box. Science/social keep the source table (just restamp the wrapper);
/// math clones the template's 〈보기〉 shell and drops the source's box items
/// into its content cell.
fn apply_box(src: Paragraph, slot: &Slot) -> Paragraph {
    let src_has_table = src.controls.iter().any(|c| matches!(c, Control::Table(_)));

    if slot.preserve_source_table && src_has_table {
        let mut out = src;
        out.para_shape_id = slot.para_shape_id;
        out.style_id = slot.style_id;
        if let Some(first) = out.char_shapes.first_mut() {
            first.char_shape_id = slot.char_shape_id;
        }
        reset_breaks(&mut out);
        return out;
    }

    let Some(mut out) = slot.template_paragraph.clone() else {
        return apply_role(src, slot);
    };
    let Some(tbl_idx) = out
        .controls
        .iter()
        .position(|c| matches!(c, Control::Table(_)))
    else {
        return apply_role(src, slot);
    };

    let mut content = src_content_paragraphs(&src);
    if let Control::Table(tbl) = &mut out.controls[tbl_idx] {
        let content_idx = content_cell_index(tbl);
        let ref_meta = tbl
            .cells
            .get(content_idx)
            .and_then(|c| c.paragraphs.first())
            .map(|p| {
                (
                    p.para_shape_id,
                    p.style_id,
                    p.char_shapes.first().map(|x| x.char_shape_id),
                )
            });
        if let Some((pp, st, cs)) = ref_meta {
            for p in &mut content {
                p.para_shape_id = pp;
                p.style_id = st;
                if let (Some(cs), Some(first)) = (cs, p.char_shapes.first_mut()) {
                    first.char_shape_id = cs;
                }
            }
        }
        if let Some(cell) = tbl.cells.get_mut(content_idx) {
            cell.paragraphs = content;
        }
    }
    out
}

/// Source content to place in the box: the heaviest cell of the source table
/// (the actual ㄱ/ㄴ/ㄷ items), or the paragraph itself when it has no table.
fn src_content_paragraphs(src: &Paragraph) -> Vec<Paragraph> {
    for c in &src.controls {
        if let Control::Table(t) = c {
            if let Some(cell) = t.cells.iter().max_by_key(|c| cell_weight(&c.paragraphs)) {
                return cell.paragraphs.clone();
            }
        }
    }
    vec![src.clone()]
}

fn cell_weight(paras: &[Paragraph]) -> usize {
    paras
        .iter()
        .map(|p| p.text.chars().count() + 10 * p.controls.len())
        .sum()
}

/// Index of the template box cell that should hold the content (heaviest cell).
fn content_cell_index(tbl: &Table) -> usize {
    tbl.cells
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| cell_weight(&c.paragraphs))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Char length of a leading question-number prefix (`\s*\d+\s*\.\s*`), or None.
fn qnum_prefix_len(text: &str) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    let digit_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start {
        return None;
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() || chars[i] != '.' {
        return None;
    }
    i += 1; // consume '.'
            // A following digit means it was a decimal, not a question number.
    if i < chars.len() && chars[i].is_ascii_digit() {
        return None;
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    Some(i)
}
