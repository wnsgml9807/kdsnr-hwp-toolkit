//! Template composition: extract a question's source paragraphs and inject
//! them into a clean per-subject template, so the output's structural frame
//! (page setup, header/footer, columns, styles) comes from the known-good
//! template rather than a fragile slice of the original document.
//!
//! Pipeline per question:
//!   1. `merge_styles` — bring the source's catalogs into the template, get
//!      source→template id maps.
//!   2. for each source paragraph: classify (on source ids) → `remap_paragraph`
//!      (into template id-space) → `apply_atom` (adopt the template slot).
//!   3. `inject` — keep the template's structural carrier paragraph and append
//!      the composed paragraphs.

pub mod apply_atom;
pub mod inject;
pub mod merge_styles;
pub mod postprocess;
pub mod rewrite;
pub mod slot_catalog;

pub use apply_atom::apply_atom;
pub use inject::inject;
pub use merge_styles::{merge_styles, IdMaps};
pub use rewrite::remap_paragraph;
pub use slot_catalog::{build_catalog, Slot, SubjectCatalog};

use std::sync::OnceLock;

use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::paragraph::{CharShapeRef, ColumnBreakType, Paragraph, ParagraphItem};
use crate::model::style::HeadType;
use crate::parse_document;
use crate::split::{classify_atom, AtomKind, Subject};

const MATH_HWPX: &[u8] = include_bytes!("templates/math.hwpx");
const SCIENCE_HWPX: &[u8] = include_bytes!("templates/science.hwpx");
const SOCIAL_HWPX: &[u8] = include_bytes!("templates/social.hwpx");
const KOREAN_HWPX: &[u8] = include_bytes!("templates/korean.hwpx");

/// Per-subject composition knobs. Every other step of the pipeline is
/// subject-independent; these are the only template-driven differences.
#[derive(Clone, Copy)]
struct ComposePolicy {
    /// Show the section master page on the first page. The Korean template hides
    /// it for a cover page we drop, so its "국어 영역" band and column dividers
    /// would otherwise vanish from page 1.
    show_master_on_first_page: bool,
    /// Empty the template footers. The science/social templates ship a footer
    /// holding a decorative sample table.
    clear_footers: bool,
    /// Keep the source bogi table verbatim instead of cloning the template shell,
    /// and flatten 1x1 marker tables in cells back to text.
    preserve_source_bogi: bool,
    /// Preserve the template carrier paragraph's visible blank advance before
    /// content. Korean sets begin directly at the set header.
    keep_carrier_spacer: bool,
}

impl ComposePolicy {
    fn for_subject(subject: Subject) -> Self {
        match subject {
            Subject::Korean => ComposePolicy {
                show_master_on_first_page: true,
                clear_footers: false,
                preserve_source_bogi: false,
                keep_carrier_spacer: false,
            },
            Subject::Math => ComposePolicy {
                show_master_on_first_page: false,
                clear_footers: false,
                preserve_source_bogi: false,
                keep_carrier_spacer: true,
            },
            Subject::Science | Subject::Social => ComposePolicy {
                show_master_on_first_page: false,
                clear_footers: true,
                preserve_source_bogi: true,
                keep_carrier_spacer: true,
            },
        }
    }
}

/// Parsed subject template, cached for the process lifetime.
fn template_doc(subject: Subject) -> &'static Document {
    static MATH: OnceLock<Document> = OnceLock::new();
    static SCIENCE: OnceLock<Document> = OnceLock::new();
    static SOCIAL: OnceLock<Document> = OnceLock::new();
    static KOREAN: OnceLock<Document> = OnceLock::new();
    let (cell, bytes, name) = match subject {
        Subject::Math => (&MATH, MATH_HWPX, "math"),
        Subject::Science => (&SCIENCE, SCIENCE_HWPX, "science"),
        Subject::Social => (&SOCIAL, SOCIAL_HWPX, "social"),
        Subject::Korean => (&KOREAN, KOREAN_HWPX, "korean"),
    };
    cell.get_or_init(|| {
        parse_document(bytes).unwrap_or_else(|e| panic!("bundled {name} template parse: {e:?}"))
    })
}

/// Compose one question's paragraphs into a fresh subject template document.
pub fn compose_question(
    src: &Document,
    subject: Subject,
    unit_paragraphs: &[Paragraph],
) -> Document {
    let policy = ComposePolicy::for_subject(subject);
    let template = template_doc(subject);
    let catalog = build_catalog(template, subject, policy.preserve_source_bogi);
    let (mut merged, maps) = merge_styles(template, src);

    let mut composed = Vec::with_capacity(unit_paragraphs.len());
    let mut prev_atom = None;
    let mut keep_next_korean_set_spacer = false;
    let mut korean_next_question = (subject == Subject::Korean)
        .then(|| korean_set_range(unit_paragraphs).map(|(from, _)| from))
        .flatten();
    for p in unit_paragraphs {
        // Classify on the source ids (before remapping) — the classifier keys
        // on source-specific para_shape/style ids.
        let atom = classify_atom(p, subject, prev_atom);
        let keep_korean_set_spacer = subject == Subject::Korean
            && keep_next_korean_set_spacer
            && atom == AtomKind::Empty
            && is_korean_set_header_spacer(p);
        if matches!(atom, AtomKind::Empty | AtomKind::Unknown) && !keep_korean_set_spacer {
            continue;
        }
        if atom == AtomKind::SetHeader {
            keep_next_korean_set_spacer = true;
            prev_atom = Some(atom);
        } else if keep_korean_set_spacer {
            keep_next_korean_set_spacer = false;
            prev_atom = Some(AtomKind::Jimun);
        } else {
            keep_next_korean_set_spacer = false;
            prev_atom = Some(atom);
        }

        let mut q = p.clone();
        remap_paragraph(&mut q, &maps);
        strip_source_page_controls(&mut q);
        if subject == Subject::Korean && atom == AtomKind::SetHeader {
            clean_korean_set_header(&mut q);
        }
        if subject == Subject::Korean && atom == AtomKind::Balmun {
            korean_next_question = normalize_korean_balmun_number(&mut q, korean_next_question);
        }
        match catalog.atom_to_slot.get(&atom) {
            Some(slot) => composed.push(apply_atom(q, atom, slot)),
            // No template slot for this atom: keep the remapped source paragraph
            // so its content is never dropped.
            None => {
                q.column_type = ColumnBreakType::None;
                q.raw_break_type = 0;
                composed.push(q);
            }
        }
    }

    for p in &mut composed {
        restack_linesegs(p);
    }
    if subject == Subject::Korean {
        disable_korean_auto_numbering(&mut merged, &mut composed);
    }

    let mut out = inject(
        merged,
        composed,
        policy.show_master_on_first_page,
        policy.keep_carrier_spacer,
    );
    postprocess::strip_default_tab_stop(&mut out);
    if policy.clear_footers {
        postprocess::clear_footers(&mut out);
    }
    out
}

fn is_korean_set_header_spacer(p: &Paragraph) -> bool {
    p.text.trim().is_empty()
        && p.controls.is_empty()
        && p.line_segs
            .first()
            .is_some_and(|seg| seg.text_height + seg.line_spacing > 0)
}

fn strip_source_page_controls(p: &mut Paragraph) {
    filter_controls(p, |c| {
        !matches!(
            c,
            Control::SectionDef(_)
                | Control::ColumnDef(_)
                | Control::Header(_)
                | Control::Footer(_)
                | Control::PageNumberPos(_)
                | Control::PageHide(_)
        )
    });
}

fn filter_controls(p: &mut Paragraph, keep: impl Fn(&Control) -> bool) {
    let mut index_map = vec![None; p.controls.len()];
    let mut controls = Vec::with_capacity(p.controls.len());
    let mut ctrl_data_records = Vec::with_capacity(p.ctrl_data_records.len());
    let mut changed = false;

    for (idx, control) in p.controls.iter().enumerate() {
        if keep(control) {
            index_map[idx] = Some(controls.len());
            controls.push(control.clone());
            ctrl_data_records.push(p.ctrl_data_records.get(idx).cloned().flatten());
        } else {
            changed = true;
        }
    }

    if !changed {
        return;
    }

    let old_text = p.text.clone();
    let old_offsets = p.char_offsets.clone();
    let mut items = Vec::new();
    for item in ordered_items(p) {
        match item {
            ParagraphItem::Text(text) => items.push(ParagraphItem::Text(text)),
            ParagraphItem::Control(idx) => {
                if let Some(Some(new_idx)) = index_map.get(idx) {
                    items.push(ParagraphItem::Control(*new_idx));
                }
            }
        }
    }

    p.controls = controls;
    p.ctrl_data_records = ctrl_data_records;
    p.items = coalesce_items(items);
    rebuild_flow_metrics(p);
    remap_positions(p, &old_text, &old_offsets);
}

fn clean_korean_set_header(p: &mut Paragraph) {
    let text = p.text.clone();
    let start = find_set_header_start(&text).unwrap_or(0);
    let clean = text[start..].to_string();
    set_text_only(p, &clean);
}

fn korean_set_range(paragraphs: &[Paragraph]) -> Option<(u32, u32)> {
    paragraphs
        .iter()
        .find_map(|p| parse_set_header_range(&p.text))
}

fn normalize_korean_balmun_number(p: &mut Paragraph, next: Option<u32>) -> Option<u32> {
    if let Some(current) = leading_question_number(&p.text) {
        return Some(current + 1);
    }
    let current = next?;
    prefix_korean_balmun_number(p, current);
    Some(current + 1)
}

fn prefix_korean_balmun_number(p: &mut Paragraph, number: u32) {
    let old_text = p.text.clone();
    let trim_chars = old_text.chars().take_while(|ch| ch.is_whitespace()).count();
    let trim_units: u32 = old_text
        .chars()
        .take(trim_chars)
        .map(char_utf16_width)
        .sum();
    let body: String = old_text.chars().skip(trim_chars).collect();
    let prefix = format!("{number}. ");
    let prefix_units = utf16_len(&prefix);
    let new_text = format!("{prefix}{body}");

    for cs in &mut p.char_shapes {
        cs.start_pos = if cs.start_pos <= trim_units {
            0
        } else {
            cs.start_pos - trim_units + prefix_units
        };
    }
    if p.char_shapes.is_empty() {
        p.char_shapes.push(CharShapeRef {
            start_pos: 0,
            char_shape_id: 0,
        });
    } else {
        p.char_shapes[0].start_pos = 0;
    }
    p.char_shapes.sort_by_key(|cs| cs.start_pos);
    p.char_shapes.dedup_by_key(|cs| cs.start_pos);

    for line in &mut p.line_segs {
        line.text_start = if line.text_start <= trim_units {
            0
        } else {
            line.text_start - trim_units + prefix_units
        };
    }
    for range in &mut p.range_tags {
        range.start = if range.start <= trim_units {
            0
        } else {
            range.start - trim_units + prefix_units
        };
        range.end = if range.end <= trim_units {
            prefix_units
        } else {
            range.end - trim_units + prefix_units
        };
    }
    p.text = new_text.clone();
    p.items = vec![ParagraphItem::Text(new_text)];
    p.char_offsets = char_offsets_for(&p.text);
    p.char_count = utf16_len(&p.text) + 1;
    p.has_para_text = true;
    p.tab_extended.clear();
}

fn disable_korean_auto_numbering(doc: &mut Document, paragraphs: &mut [Paragraph]) {
    let mut clones = std::collections::HashMap::<u16, u16>::new();
    for p in paragraphs {
        if leading_question_number(&p.text).is_none() {
            continue;
        }
        let old_id = p.para_shape_id;
        let Some(shape) = doc.doc_info.para_shapes.get(old_id as usize) else {
            continue;
        };
        if shape.head_type != HeadType::Number {
            continue;
        }
        if let Some(&new_id) = clones.get(&old_id) {
            p.para_shape_id = new_id;
            continue;
        }
        let mut clone = shape.clone();
        clone.raw_data = None;
        clone.head_type = HeadType::None;
        clone.numbering_id = 0;
        clone.para_level = 0;
        clone.attr1 &= !(0x03 << 23);
        clone.attr1 &= !(0x07 << 25);
        doc.doc_info.para_shapes.push(clone);
        let new_id = (doc.doc_info.para_shapes.len() - 1) as u16;
        clones.insert(old_id, new_id);
        p.para_shape_id = new_id;
    }
}

fn set_text_only(p: &mut Paragraph, text: &str) {
    let first_cs = p
        .char_shapes
        .first()
        .map(|cs| cs.char_shape_id)
        .unwrap_or(0);
    p.text = text.to_string();
    p.items = if text.is_empty() {
        Vec::new()
    } else {
        vec![ParagraphItem::Text(text.to_string())]
    };
    p.controls.clear();
    p.ctrl_data_records.clear();
    p.char_offsets = char_offsets_for(text);
    p.char_count = utf16_len(text) + 1;
    p.char_shapes = vec![CharShapeRef {
        start_pos: 0,
        char_shape_id: first_cs,
    }];
    p.range_tags.clear();
    p.field_ranges.clear();
    p.tab_extended.clear();
    p.has_para_text = !text.is_empty();
    if let Some(first) = p.line_segs.first().cloned() {
        let mut line = first;
        line.text_start = 0;
        p.line_segs = vec![line];
    }
}

fn ordered_items(p: &Paragraph) -> Vec<ParagraphItem> {
    if !p.items.is_empty() {
        return p.items.clone();
    }
    let mut items = Vec::new();
    if !p.text.is_empty() {
        items.push(ParagraphItem::Text(p.text.clone()));
    }
    items.extend((0..p.controls.len()).map(ParagraphItem::Control));
    items
}

fn coalesce_items(items: Vec<ParagraphItem>) -> Vec<ParagraphItem> {
    let mut out: Vec<ParagraphItem> = Vec::new();
    for item in items {
        match (out.last_mut(), item) {
            (Some(ParagraphItem::Text(prev)), ParagraphItem::Text(text)) => prev.push_str(&text),
            (_, item) => out.push(item),
        }
    }
    out
}

fn rebuild_flow_metrics(p: &mut Paragraph) {
    let mut text = String::new();
    let mut char_offsets = Vec::new();
    let mut utf16_pos = 0u32;
    for item in &p.items {
        match item {
            ParagraphItem::Text(part) => {
                for ch in part.chars() {
                    char_offsets.push(utf16_pos);
                    text.push(ch);
                    utf16_pos += char_utf16_width(ch);
                }
            }
            ParagraphItem::Control(_) => utf16_pos += 8,
        }
    }
    p.text = text;
    p.char_offsets = char_offsets;
    p.char_count = utf16_pos + 1;
    p.has_para_text = !p.text.is_empty() || !p.controls.is_empty();
}

fn remap_positions(p: &mut Paragraph, old_text: &str, old_offsets: &[u32]) {
    let new_offsets = p.char_offsets.clone();
    for cs in &mut p.char_shapes {
        cs.start_pos = map_utf16_pos(cs.start_pos, old_text, old_offsets, &new_offsets);
    }
    p.char_shapes.sort_by_key(|cs| cs.start_pos);
    p.char_shapes.dedup_by_key(|cs| cs.start_pos);
    for line in &mut p.line_segs {
        line.text_start = map_utf16_pos(line.text_start, old_text, old_offsets, &new_offsets);
    }
    for range in &mut p.range_tags {
        range.start = map_utf16_pos(range.start, old_text, old_offsets, &new_offsets);
        range.end = map_utf16_pos(range.end, old_text, old_offsets, &new_offsets);
    }
}

fn map_utf16_pos(pos: u32, old_text: &str, old_offsets: &[u32], new_offsets: &[u32]) -> u32 {
    if old_offsets.is_empty() || new_offsets.is_empty() {
        return pos.min(text_end_utf16_from_offsets(old_text, new_offsets));
    }
    match old_offsets.iter().position(|old| *old >= pos) {
        Some(idx) => new_offsets
            .get(idx)
            .copied()
            .unwrap_or_else(|| text_end_utf16_from_offsets(old_text, new_offsets)),
        None => text_end_utf16_from_offsets(old_text, new_offsets),
    }
}

fn text_end_utf16_from_offsets(text: &str, offsets: &[u32]) -> u32 {
    let Some((&last_offset, last_char)) = offsets.last().zip(text.chars().last()) else {
        return 0;
    };
    last_offset + char_utf16_width(last_char)
}

fn char_offsets_for(text: &str) -> Vec<u32> {
    let mut pos = 0u32;
    text.chars()
        .map(|ch| {
            let current = pos;
            pos += char_utf16_width(ch);
            current
        })
        .collect()
}

fn utf16_len(text: &str) -> u32 {
    text.chars().map(char_utf16_width).sum()
}

fn char_utf16_width(ch: char) -> u32 {
    if ch == '\t' {
        8
    } else if (ch as u32) > 0xFFFF {
        2
    } else {
        1
    }
}

fn find_set_header_start(text: &str) -> Option<usize> {
    for (start, ch) in text.char_indices() {
        if ch == '[' && parse_set_header_after_open(&text[start + ch.len_utf8()..]).is_some() {
            return Some(start);
        }
    }
    None
}

fn parse_set_header_range(text: &str) -> Option<(u32, u32)> {
    for (start, ch) in text.char_indices() {
        if ch != '[' {
            continue;
        }
        let rest = &text[start + ch.len_utf8()..];
        if let Some(parsed) = parse_set_header_range_after_open(rest) {
            return Some(parsed);
        }
    }
    None
}

fn parse_set_header_after_open(text: &str) -> Option<()> {
    parse_set_header_range_after_open(text).map(|_| ())
}

fn parse_set_header_range_after_open(text: &str) -> Option<(u32, u32)> {
    let mut chars = text.chars().peekable();
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    let mut a = String::new();
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        a.push(chars.next()?);
    }
    if a.is_empty() {
        return None;
    }
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    if !matches!(chars.next()?, '~' | '～' | '∼' | '∽') {
        return None;
    }
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    let mut b = String::new();
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        b.push(chars.next()?);
    }
    if b.is_empty() {
        return None;
    }
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    (chars.next()? == ']').then_some((a.parse().ok()?, b.parse().ok()?))
}

fn leading_question_number(text: &str) -> Option<u32> {
    let s = text.trim_start();
    let mut digits = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.peek().copied() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            chars.next();
        } else {
            break;
        }
    }
    if digits.is_empty() {
        return None;
    }
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    if chars.next()? != '.' {
        return None;
    }
    if matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Rebase a paragraph's stored line tops to increase monotonically. A paragraph
/// that spanned a column in the source stores a top that resets (e.g. 91909→0);
/// the engine positions lines within a run by their `vertical_pos` delta, so a
/// reset would drop later lines ~a column-height up. Shift each reset to sit just
/// below the previous line. Equal tops (a justified split line) are kept.
fn restack_linesegs(p: &mut Paragraph) {
    let n = p.line_segs.len();
    if n < 2 {
        return;
    }
    let orig: Vec<i32> = p.line_segs.iter().map(|s| s.vertical_pos).collect();
    let mut shift = 0i32;
    for i in 1..n {
        if orig[i] < orig[i - 1] {
            let adv = p.line_segs[i - 1].text_height + p.line_segs[i - 1].line_spacing;
            shift += orig[i - 1] + adv - orig[i];
        }
        p.line_segs[i].vertical_pos = orig[i] + shift;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split::split_document_units;
    use std::path::PathBuf;

    fn original(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../templet/original")
            .join(name)
    }

    /// Every question from each supported subject composes into a template,
    /// serializes to HWPX, and re-parses cleanly with a structural carrier plus
    /// at least one composed paragraph.
    fn check_subject(file: &str, expect: Subject) {
        let bytes = std::fs::read(original(file)).expect("read input");
        let doc = parse_document(&bytes).expect("parse input");
        let (subject, questions) = split_document_units(&doc).expect("split");
        assert_eq!(subject, expect, "{file}");
        assert!(!questions.is_empty(), "{file}: no questions");
        for q in &questions {
            let sec = q.document.sections.first().expect("section");
            assert!(
                sec.paragraphs.len() >= 2,
                "{file} {}: expected carrier + content, got {}",
                q.label,
                sec.paragraphs.len()
            );
            let out = crate::serialize_hwpx(&q.document)
                .unwrap_or_else(|e| panic!("{file} {}: serialize: {e:?}", q.label));
            let re = parse_document(&out)
                .unwrap_or_else(|e| panic!("{file} {}: reparse: {e:?}", q.label));
            assert!(!re.sections.is_empty(), "{file} {}: empty reparse", q.label);
        }
    }

    #[test]
    fn compose_math() {
        check_subject("math_input_sample.hwpx", Subject::Math);
    }

    #[test]
    fn compose_social() {
        check_subject("social_input_sample.hwpx", Subject::Social);
    }
}
