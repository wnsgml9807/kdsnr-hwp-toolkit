//! Exam-paper split logic.
//!
//! This module intentionally lives next to the parser because question
//! boundaries should be derived from the parsed source document, before any
//! Python-side template/layout code can disturb source paragraph geometry.
//!
//! Current scope:
//! - subject detection and Korean rejection
//! - question/set boundary detection
//! - wrapper/meta table unwrapping and mixed paragraph splitting needed by
//!   the observed `templet/original` exam inputs
//! - source paragraph-range slicing into question-local `Document`s
//! - per-unit preservation contract stats for layout-risk features
//!
//! Deliberate non-goal for this pass:
//! - template restyling

use std::fmt;

use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::image::Picture;
use crate::model::paragraph::{CharShapeRef, Paragraph, ParagraphItem};
use crate::model::shape::{ShapeObject, TextWrap};
use crate::model::table::Table;

const UNSUPPORTED_KOREAN_MESSAGE: &str = "국어 과목은 아직 지원하지 않습니다";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Math,
    Science,
    Social,
    Korean,
}

impl Subject {
    pub fn as_str(self) -> &'static str {
        match self {
            Subject::Math => "math",
            Subject::Science => "science",
            Subject::Social => "social",
            Subject::Korean => "korean",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Balmun,
    Seonji,
    Middle,
    SetHeader,
    Jimun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomKind {
    Balmun,
    BalmunCont,
    EqBlock,
    DataBox,
    BogiBox,
    Seonji,
    Seonji2Row,
    PicBlock,
    SetHeader,
    Jimun,
    JimunDialog,
    JimunBracket,
    JimunDataBox,
    JimunInlineTable,
    Jungryak,
    AuthorCredit,
    Footnote,
    Empty,
    Unknown,
}

impl AtomKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AtomKind::Balmun => "balmun",
            AtomKind::BalmunCont => "balmun_cont",
            AtomKind::EqBlock => "eq_block",
            AtomKind::DataBox => "data_box",
            AtomKind::BogiBox => "bogi_box",
            AtomKind::Seonji => "seonji",
            AtomKind::Seonji2Row => "seonji_2row",
            AtomKind::PicBlock => "pic_block",
            AtomKind::SetHeader => "set_header",
            AtomKind::Jimun => "jimun",
            AtomKind::JimunDialog => "jimun_dialog",
            AtomKind::JimunBracket => "jimun_bracket",
            AtomKind::JimunDataBox => "jimun_data_box",
            AtomKind::JimunInlineTable => "jimun_inline_table",
            AtomKind::Jungryak => "jungryak",
            AtomKind::AuthorCredit => "author_credit",
            AtomKind::Footnote => "footnote",
            AtomKind::Empty => "empty",
            AtomKind::Unknown => "unknown",
        }
    }
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Balmun => "balmun",
            Role::Seonji => "seonji",
            Role::Middle => "middle",
            Role::SetHeader => "set_header",
            Role::Jimun => "jimun",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetectedUnit {
    pub label: String,
    pub para_indices: Vec<usize>,
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone)]
pub struct QuestionDocument {
    pub label: String,
    pub document: Document,
    pub source_para_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct UnitContract {
    pub label: String,
    pub source_para_indices: Vec<usize>,
    pub normalized_paragraphs: usize,
    pub roles: Vec<Role>,
    pub atoms: Vec<AtomKind>,
    pub tables: usize,
    pub cells: usize,
    pub pictures: usize,
    pub equations: usize,
    pub table_border_fill_ids: Vec<u16>,
    pub cell_border_fill_ids: Vec<u16>,
    pub picture_z_orders: Vec<i32>,
    pub picture_bin_data_ids: Vec<u16>,
    pub picture_wrap_modes: Vec<&'static str>,
    pub floating_pictures: usize,
    pub inline_pictures: usize,
    pub square_wrap_paragraphs: usize,
    pub choice_image_tables: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError {
    EmptyDocument,
    UnsupportedKorean,
    UnknownSubject,
    NoQuestionUnits,
}

impl fmt::Display for SplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SplitError::EmptyDocument => write!(f, "문서에 본문 구역이 없습니다"),
            SplitError::UnsupportedKorean => write!(f, "{UNSUPPORTED_KOREAN_MESSAGE}"),
            SplitError::UnknownSubject => write!(f, "시험지 과목을 자동 인식하지 못했습니다"),
            SplitError::NoQuestionUnits => write!(f, "문항을 인식하지 못했습니다"),
        }
    }
}

impl std::error::Error for SplitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParaKind {
    Empty,
    Balmun,
    Seonji,
    SetHeader,
    Other,
}

#[derive(Debug, Clone)]
struct NormalizedParagraph {
    source_idx: Option<usize>,
    is_memo: bool,
    paragraph: Paragraph,
}

pub fn detect_subject(doc: &Document) -> Result<Subject, SplitError> {
    let text = document_text(doc);
    detect_subject_from_text(&text)
}

pub fn detect_subject_from_text(text: &str) -> Result<Subject, SplitError> {
    let compact = compact_text(text);
    if contains_unsupported_korean_marker(text, &compact) {
        return Err(SplitError::UnsupportedKorean);
    }
    if compact.contains("과학탐구") || compact.contains("통합과학") {
        return Ok(Subject::Science);
    }
    if compact.contains("수학영역") || (compact.contains("5지선다형") && compact.contains("단답형"))
    {
        return Ok(Subject::Math);
    }
    if compact.contains("사회탐구") {
        return Ok(Subject::Social);
    }
    let social_markers = [
        "사회문제",
        "개인과사회",
        "문화",
        "가치함축",
        "사회현상",
        "당위법칙",
    ];
    if social_markers
        .iter()
        .filter(|marker| compact.contains(**marker))
        .count()
        >= 2
    {
        return Ok(Subject::Social);
    }
    Err(SplitError::UnknownSubject)
}

pub fn detect_units(doc: &Document, subject: Subject) -> Result<Vec<DetectedUnit>, SplitError> {
    let section = doc.sections.first().ok_or(SplitError::EmptyDocument)?;
    let memo_mask = build_memo_mask(doc);
    let units = match subject {
        Subject::Korean => detect_korean_sets(&section.paragraphs, &memo_mask),
        Subject::Math | Subject::Science | Subject::Social => {
            detect_questions(&section.paragraphs, &memo_mask)
        }
    };
    if units.is_empty() {
        Err(SplitError::NoQuestionUnits)
    } else {
        Ok(disambiguate_labels(units))
    }
}

pub fn split_document_units(
    doc: &Document,
) -> Result<(Subject, Vec<QuestionDocument>), SplitError> {
    let subject = detect_subject(doc)?;
    if subject == Subject::Korean {
        return Err(SplitError::UnsupportedKorean);
    }
    let normalized = normalize_body_for_split(doc, subject)?;
    let units = detect_units_in_normalized(&normalized, subject)?;
    let out = units
        .into_iter()
        .map(|unit| QuestionDocument {
            label: unit.label.clone(),
            source_para_indices: unit
                .para_indices
                .iter()
                .filter_map(|&idx| normalized.get(idx).and_then(|p| p.source_idx))
                .collect(),
            document: slice_document_from_normalized(doc, &normalized, &unit.para_indices),
        })
        .collect();
    Ok((subject, out))
}

pub fn split_document_contract(doc: &Document) -> Result<(Subject, Vec<UnitContract>), SplitError> {
    let subject = detect_subject(doc)?;
    if subject == Subject::Korean {
        return Err(SplitError::UnsupportedKorean);
    }
    split_document_contract_inner(doc, subject)
}

fn split_document_contract_inner(
    doc: &Document,
    subject: Subject,
) -> Result<(Subject, Vec<UnitContract>), SplitError> {
    let normalized = normalize_body_for_split(doc, subject)?;
    let units = detect_units_in_normalized(&normalized, subject)?;
    let contracts = units
        .into_iter()
        .map(|unit| {
            let paragraphs: Vec<Paragraph> = unit
                .para_indices
                .iter()
                .filter_map(|&idx| normalized.get(idx).map(|p| p.paragraph.clone()))
                .collect();
            let mut stats = UnitContractStats::default();
            let mut atoms = Vec::with_capacity(paragraphs.len());
            let mut prev_atom = None;
            for paragraph in &paragraphs {
                let atom = classify_atom(paragraph, subject, prev_atom);
                if !matches!(atom, AtomKind::Empty | AtomKind::Unknown) {
                    prev_atom = Some(atom);
                }
                atoms.push(atom);
                collect_unit_contract_stats_for_paragraph(paragraph, &mut stats);
            }
            UnitContract {
                label: unit.label,
                source_para_indices: unit
                    .para_indices
                    .iter()
                    .filter_map(|&idx| normalized.get(idx).and_then(|p| p.source_idx))
                    .collect(),
                normalized_paragraphs: paragraphs.len(),
                roles: unit.roles,
                atoms,
                tables: stats.tables,
                cells: stats.cells,
                pictures: stats.pictures,
                equations: stats.equations,
                table_border_fill_ids: stats.table_border_fill_ids,
                cell_border_fill_ids: stats.cell_border_fill_ids,
                picture_z_orders: stats.picture_z_orders,
                picture_bin_data_ids: stats.picture_bin_data_ids,
                picture_wrap_modes: stats.picture_wrap_modes,
                floating_pictures: stats.floating_pictures,
                inline_pictures: stats.inline_pictures,
                square_wrap_paragraphs: stats.square_wrap_paragraphs,
                choice_image_tables: stats.choice_image_tables,
            }
        })
        .collect();
    Ok((subject, contracts))
}

fn detect_units_in_normalized(
    normalized: &[NormalizedParagraph],
    subject: Subject,
) -> Result<Vec<DetectedUnit>, SplitError> {
    let paragraphs: Vec<Paragraph> = normalized.iter().map(|p| p.paragraph.clone()).collect();
    let memo_mask: Vec<bool> = normalized.iter().map(|p| p.is_memo).collect();
    let units = match subject {
        Subject::Korean => detect_korean_sets(&paragraphs, &memo_mask),
        Subject::Math | Subject::Science | Subject::Social => {
            detect_questions(&paragraphs, &memo_mask)
        }
    };
    if units.is_empty() {
        Err(SplitError::NoQuestionUnits)
    } else {
        Ok(disambiguate_labels(units))
    }
}

#[derive(Default)]
struct UnitContractStats {
    tables: usize,
    cells: usize,
    pictures: usize,
    equations: usize,
    table_border_fill_ids: Vec<u16>,
    cell_border_fill_ids: Vec<u16>,
    picture_z_orders: Vec<i32>,
    picture_bin_data_ids: Vec<u16>,
    picture_wrap_modes: Vec<&'static str>,
    floating_pictures: usize,
    inline_pictures: usize,
    square_wrap_paragraphs: usize,
    choice_image_tables: usize,
}

fn collect_unit_contract_stats_for_paragraph(paragraph: &Paragraph, stats: &mut UnitContractStats) {
    if paragraph_has_square_wrap_linesegs(paragraph) {
        stats.square_wrap_paragraphs += 1;
    }
    for control in &paragraph.controls {
        collect_unit_contract_stats_for_control(control, stats);
    }
}

fn collect_unit_contract_stats_for_control(control: &Control, stats: &mut UnitContractStats) {
    match control {
        Control::Table(table) => {
            stats.tables += 1;
            stats.table_border_fill_ids.push(table.border_fill_id);
            stats.cells += table.cells.len();
            if is_choice_image_table(table) {
                stats.choice_image_tables += 1;
            }
            for cell in &table.cells {
                stats.cell_border_fill_ids.push(cell.border_fill_id);
                for paragraph in &cell.paragraphs {
                    collect_unit_contract_stats_for_paragraph(paragraph, stats);
                }
            }
        }
        Control::Picture(pic) => collect_unit_contract_stats_for_picture(pic, stats),
        Control::Equation(_) => stats.equations += 1,
        Control::Shape(shape) => collect_unit_contract_stats_for_shape(shape, stats),
        Control::Header(header) => {
            for paragraph in &header.paragraphs {
                collect_unit_contract_stats_for_paragraph(paragraph, stats);
            }
        }
        Control::Footer(footer) => {
            for paragraph in &footer.paragraphs {
                collect_unit_contract_stats_for_paragraph(paragraph, stats);
            }
        }
        Control::Footnote(footnote) => {
            for paragraph in &footnote.paragraphs {
                collect_unit_contract_stats_for_paragraph(paragraph, stats);
            }
        }
        Control::Endnote(endnote) => {
            for paragraph in &endnote.paragraphs {
                collect_unit_contract_stats_for_paragraph(paragraph, stats);
            }
        }
        Control::HiddenComment(comment) => {
            for paragraph in &comment.paragraphs {
                collect_unit_contract_stats_for_paragraph(paragraph, stats);
            }
        }
        _ => {}
    }
}

fn collect_unit_contract_stats_for_picture(pic: &Picture, stats: &mut UnitContractStats) {
    stats.pictures += 1;
    stats.picture_z_orders.push(pic.common.z_order);
    stats.picture_bin_data_ids.push(pic.image_attr.bin_data_id);
    stats
        .picture_wrap_modes
        .push(text_wrap_as_str(pic.common.text_wrap));
    if pic.common.treat_as_char {
        stats.inline_pictures += 1;
    } else {
        stats.floating_pictures += 1;
    }
}

fn collect_unit_contract_stats_for_shape(shape: &ShapeObject, stats: &mut UnitContractStats) {
    match shape {
        ShapeObject::Picture(pic) => collect_unit_contract_stats_for_picture(pic, stats),
        ShapeObject::Group(group) => {
            for child in &group.children {
                collect_unit_contract_stats_for_shape(child, stats);
            }
        }
        _ => {}
    }
}

fn text_wrap_as_str(wrap: TextWrap) -> &'static str {
    match wrap {
        TextWrap::Square => "square",
        TextWrap::Tight => "tight",
        TextWrap::Through => "through",
        TextWrap::TopAndBottom => "top_and_bottom",
        TextWrap::BehindText => "behind_text",
        TextWrap::InFrontOfText => "in_front_of_text",
    }
}

fn paragraph_has_square_wrap_linesegs(paragraph: &Paragraph) -> bool {
    let mut first = None;
    for seg in &paragraph.line_segs {
        let width = seg.segment_width;
        if width <= 0 {
            continue;
        }
        match first {
            None => first = Some(width),
            Some(prev) if prev != width => return true,
            _ => {}
        }
    }
    false
}

fn is_choice_image_table(table: &crate::model::table::Table) -> bool {
    if table.row_count > 2 || table.col_count < 5 {
        return false;
    }
    let mut markers = 0usize;
    let mut has_picture = false;
    for cell in &table.cells {
        let text = cell
            .paragraphs
            .iter()
            .map(paragraph_text)
            .collect::<String>();
        if text
            .chars()
            .any(|ch| matches!(ch, '①' | '②' | '③' | '④' | '⑤'))
        {
            markers += 1;
        }
        for paragraph in &cell.paragraphs {
            if paragraph
                .controls
                .iter()
                .any(|control| matches!(control, Control::Picture(_)))
            {
                has_picture = true;
            }
        }
    }
    markers >= 3 && has_picture
}

fn classify_atom(paragraph: &Paragraph, subject: Subject, prev_atom: Option<AtomKind>) -> AtomKind {
    if is_empty_atom_paragraph(paragraph) {
        return AtomKind::Empty;
    }

    let text = paragraph_text(paragraph);
    let trimmed = text.trim();
    let table = first_table(paragraph);

    if let Some(table) = table {
        if is_bogi_table(table) {
            return AtomKind::BogiBox;
        }
        if subject == Subject::Korean {
            if set_header_match(&text).is_some() {
                return AtomKind::SetHeader;
            }
            if matches!(
                prev_atom,
                Some(
                    AtomKind::Jimun
                        | AtomKind::JimunDialog
                        | AtomKind::JimunBracket
                        | AtomKind::JimunDataBox
                        | AtomKind::JimunInlineTable
                )
            ) {
                if table.row_count == 1 && table.col_count == 1 {
                    return AtomKind::JimunDataBox;
                }
                return AtomKind::JimunInlineTable;
            }
        }
        return AtomKind::DataBox;
    }

    if has_picture(paragraph) && trimmed.is_empty() {
        return AtomKind::PicBlock;
    }

    if subject == Subject::Korean {
        if set_header_match(&text).is_some() {
            return AtomKind::SetHeader;
        }
        if text.contains("(중략)") {
            return AtomKind::Jungryak;
        }
        if text.trim_start().starts_with('*') {
            return AtomKind::Footnote;
        }
        if balmun_number(trimmed).is_some()
            || (paragraph.para_shape_id, paragraph.style_id) == (43, 3)
        {
            return AtomKind::Balmun;
        }
        if starts_with_choice_marker(trimmed) {
            return AtomKind::Seonji;
        }
        if text.trim_start().starts_with(['“', '"']) {
            return AtomKind::JimunDialog;
        }
        if paragraph.para_shape_id == 47 {
            return AtomKind::JimunBracket;
        }
        return AtomKind::Jimun;
    }

    if balmun_number(trimmed).is_some() {
        return AtomKind::Balmun;
    }
    if starts_with_choice_marker(trimmed) {
        if has_explicit_line_break(paragraph) {
            return AtomKind::Seonji2Row;
        }
        return AtomKind::Seonji;
    }
    if has_equation(paragraph) && !contains_hangul_or_latin(&text) {
        return AtomKind::EqBlock;
    }
    if subject == Subject::Math
        && (paragraph.para_shape_id, paragraph.style_id) == (6, 2)
        && has_equation(paragraph)
    {
        return AtomKind::EqBlock;
    }
    if (has_equation(paragraph) || !trimmed.is_empty())
        && matches!(
            prev_atom,
            Some(
                AtomKind::EqBlock
                    | AtomKind::DataBox
                    | AtomKind::BogiBox
                    | AtomKind::Balmun
                    | AtomKind::BalmunCont
                    | AtomKind::PicBlock
            )
        )
    {
        return AtomKind::BalmunCont;
    }
    AtomKind::Unknown
}

fn is_empty_atom_paragraph(paragraph: &Paragraph) -> bool {
    paragraph_text(paragraph).trim().is_empty()
        && first_table(paragraph).is_none()
        && !has_picture(paragraph)
        && !has_equation(paragraph)
}

fn first_table(paragraph: &Paragraph) -> Option<&Table> {
    paragraph.controls.iter().find_map(|control| match control {
        Control::Table(table) => Some(table.as_ref()),
        _ => None,
    })
}

fn has_picture(paragraph: &Paragraph) -> bool {
    paragraph
        .controls
        .iter()
        .any(|control| matches!(control, Control::Picture(_)))
}

fn has_equation(paragraph: &Paragraph) -> bool {
    paragraph
        .controls
        .iter()
        .any(|control| matches!(control, Control::Equation(_)))
}

fn is_bogi_table(table: &Table) -> bool {
    if table.row_count < 3 || table.col_count < 3 {
        return false;
    }
    let text = table
        .cells
        .iter()
        .flat_map(|cell| &cell.paragraphs)
        .map(paragraph_text)
        .collect::<String>();
    compact_text(&text).contains("보기")
}

fn starts_with_choice_marker(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| matches!(ch, '①' | '②' | '③' | '④' | '⑤'))
}

fn has_explicit_line_break(paragraph: &Paragraph) -> bool {
    paragraph.text.contains('\n') || paragraph.text.contains('\u{000a}')
}

fn contains_hangul_or_latin(text: &str) -> bool {
    text.chars()
        .any(|ch| ch.is_ascii_alphabetic() || ('가'..='힣').contains(&ch))
}

fn slice_document_from_normalized(
    doc: &Document,
    normalized: &[NormalizedParagraph],
    para_indices: &[usize],
) -> Document {
    let mut out = doc.clone();
    let initial_column_def = doc
        .sections
        .first()
        .and_then(initial_section_column_def)
        .cloned();
    if let Some(first) = out.sections.first_mut() {
        first.paragraphs = para_indices
            .iter()
            .filter_map(|&idx| normalized.get(idx).map(|p| p.paragraph.clone()))
            .collect();
        if let (Some(col), Some(first_para)) = (initial_column_def, first.paragraphs.first_mut()) {
            let already_has_initial_col = first_para.controls.iter().any(|ctrl| {
                matches!(
                    ctrl,
                    Control::ColumnDef(existing)
                        if existing.column_count == col.column_count
                            && existing.spacing == col.spacing
                            && existing.same_width == col.same_width
                )
            });
            if !already_has_initial_col {
                // The HWPX serializer emits section-level colPr from the first
                // ColumnDef it can see in the sliced document. A question slice
                // must therefore carry the source section's initial column
                // definition even when the defining paragraph lives before the
                // question boundary; otherwise cached lineSeg widths are
                // interpreted under the wrong column context.
                first_para.controls.insert(0, Control::ColumnDef(col));
                first_para.ctrl_data_records.insert(0, None);
            }
        }
        first.raw_stream = None;
    }
    for section in out.sections.iter_mut().skip(1) {
        section.paragraphs.clear();
        section.raw_stream = None;
    }
    out.doc_info.raw_stream_dirty = true;
    out
}

fn initial_section_column_def(
    section: &crate::model::document::Section,
) -> Option<&crate::model::page::ColumnDef> {
    section.paragraphs.iter().find_map(|paragraph| {
        paragraph.controls.iter().find_map(|control| {
            if let Control::ColumnDef(col) = control {
                Some(col)
            } else {
                None
            }
        })
    })
}

fn normalize_body_for_split(
    doc: &Document,
    subject: Subject,
) -> Result<Vec<NormalizedParagraph>, SplitError> {
    let section = doc.sections.first().ok_or(SplitError::EmptyDocument)?;
    let memo_mask = build_memo_mask(doc);
    let korean = subject == Subject::Korean;
    let mut body: Vec<NormalizedParagraph> = section
        .paragraphs
        .iter()
        .cloned()
        .enumerate()
        .map(|(idx, paragraph)| NormalizedParagraph {
            source_idx: Some(idx),
            is_memo: memo_mask.get(idx).copied().unwrap_or(false),
            paragraph,
        })
        .collect();

    loop {
        let (next, changed) = unwrap_wrappers_once(body, korean);
        body = next;
        if !changed {
            break;
        }
    }

    body = unwrap_meta_tables(body);
    Ok(split_fused_in_body(body))
}

fn split_fused_in_body(body: Vec<NormalizedParagraph>) -> Vec<NormalizedParagraph> {
    let mut out = Vec::new();
    for para in body {
        let sub = split_fused_paragraph(&para.paragraph);
        if sub.len() == 1 {
            out.push(para);
        } else {
            out.extend(sub.into_iter().map(|paragraph| NormalizedParagraph {
                source_idx: para.source_idx,
                is_memo: para.is_memo,
                paragraph,
            }));
        }
    }
    out
}

fn split_fused_paragraph(paragraph: &Paragraph) -> Vec<Paragraph> {
    let items = ordered_items(paragraph);
    let block_count = items
        .iter()
        .filter(|item| match item {
            ParagraphItem::Control(idx) => paragraph
                .controls
                .get(*idx)
                .is_some_and(is_block_level_control),
            _ => false,
        })
        .count();
    if block_count == 0 || (block_count == 1 && items.len() == 1) {
        return vec![paragraph.clone()];
    }
    if block_count == 1 {
        let text = items
            .iter()
            .filter_map(|item| match item {
                ParagraphItem::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        let has_non_block_control = items.iter().any(|item| match item {
            ParagraphItem::Control(idx) => paragraph
                .controls
                .get(*idx)
                .is_some_and(|ctrl| !is_block_level_control(ctrl)),
            _ => false,
        });
        if text.trim().is_empty() && !has_non_block_control {
            return vec![paragraph.clone()];
        }
    }

    let mut out = Vec::new();
    let mut bucket = Vec::new();
    for item in items {
        let is_block = match item {
            ParagraphItem::Control(idx) => paragraph
                .controls
                .get(idx)
                .is_some_and(is_block_level_control),
            _ => false,
        };
        if is_block {
            if !bucket.is_empty() {
                out.push(paragraph_from_items(paragraph, &bucket));
                bucket.clear();
            }
            out.push(paragraph_from_items(paragraph, &[item]));
        } else {
            bucket.push(item);
        }
    }
    if !bucket.is_empty() {
        out.push(paragraph_from_items(paragraph, &bucket));
    }
    out
}

fn unwrap_meta_tables(body: Vec<NormalizedParagraph>) -> Vec<NormalizedParagraph> {
    let mut out = Vec::new();
    let mut changed = false;
    for para in body {
        let mut embedded = Vec::new();
        for item in ordered_items(&para.paragraph) {
            let ParagraphItem::Control(idx) = item else {
                continue;
            };
            let Some(Control::Table(table)) = para.paragraph.controls.get(idx) else {
                continue;
            };
            if table.cells.len() < 4 {
                continue;
            }
            for cell in &table.cells {
                let text = cell
                    .paragraphs
                    .iter()
                    .map(paragraph_text)
                    .collect::<String>();
                let stripped = text.trim_start_matches(['　', ' ']);
                if balmun_number(stripped).is_some() && stripped.chars().count() > 15 {
                    let source_idx = para.source_idx;
                    embedded.extend(cell.paragraphs.iter().cloned().map(|paragraph| {
                        NormalizedParagraph {
                            source_idx,
                            is_memo: para.is_memo,
                            paragraph,
                        }
                    }));
                }
            }
        }
        if embedded.is_empty() {
            out.push(para);
        } else {
            changed = true;
            out.extend(embedded);
        }
    }
    if changed {
        out
    } else {
        out
    }
}

fn unwrap_wrappers_once(
    body: Vec<NormalizedParagraph>,
    korean: bool,
) -> (Vec<NormalizedParagraph>, bool) {
    let mut out = Vec::new();
    let mut changed = false;
    for para in body {
        if let Some(table) = wrapper_table(&para.paragraph, korean) {
            let source_idx = para.source_idx;
            out.extend(table.cells.iter().flat_map(|cell| {
                cell.paragraphs
                    .iter()
                    .cloned()
                    .map(|paragraph| NormalizedParagraph {
                        source_idx,
                        is_memo: para.is_memo,
                        paragraph,
                    })
            }));
            changed = true;
        } else {
            out.push(para);
        }
    }
    (out, changed)
}

fn wrapper_table(paragraph: &Paragraph, korean: bool) -> Option<&crate::model::table::Table> {
    let items = ordered_items(paragraph);
    let mut table_idx = None;
    let mut meaningful_text = String::new();
    for item in items {
        match item {
            ParagraphItem::Text(text) => meaningful_text.push_str(&text),
            ParagraphItem::Control(idx) => {
                if paragraph
                    .controls
                    .get(idx)
                    .is_some_and(is_block_level_control)
                {
                    if table_idx.is_some() {
                        return None;
                    }
                    table_idx = Some(idx);
                } else {
                    return None;
                }
            }
        }
    }
    if !meaningful_text.trim().is_empty() {
        return None;
    }
    let idx = table_idx?;
    let Control::Table(table) = paragraph.controls.get(idx)? else {
        return None;
    };
    let has_boundary = table.cells.iter().any(|cell| {
        cell.paragraphs.iter().any(|p| {
            let kind = classify_paragraph(&paragraph_text(p), korean);
            kind == ParaKind::Balmun || (korean && kind == ParaKind::SetHeader)
        })
    });
    has_boundary.then_some(table.as_ref())
}

fn ordered_items(paragraph: &Paragraph) -> Vec<ParagraphItem> {
    if !paragraph.items.is_empty() {
        return paragraph.items.clone();
    }
    let mut items = Vec::new();
    if !paragraph.text.is_empty() {
        items.push(ParagraphItem::Text(paragraph.text.clone()));
    }
    items.extend(
        paragraph
            .controls
            .iter()
            .enumerate()
            .map(|(idx, _)| ParagraphItem::Control(idx)),
    );
    items
}

fn paragraph_from_items(template: &Paragraph, items: &[ParagraphItem]) -> Paragraph {
    let mut paragraph = template.clone();
    let mut text = String::new();
    let mut char_offsets = Vec::new();
    let mut controls = Vec::new();
    let mut ctrl_data_records = Vec::new();
    let mut new_items = Vec::new();
    let mut utf16_pos = 0u32;

    for item in items {
        match item {
            ParagraphItem::Text(part) => {
                let mut kept = String::new();
                for ch in part.chars() {
                    char_offsets.push(utf16_pos);
                    text.push(ch);
                    kept.push(ch);
                    utf16_pos += char_utf16_width(ch);
                }
                if !kept.is_empty() {
                    new_items.push(ParagraphItem::Text(kept));
                }
            }
            ParagraphItem::Control(idx) => {
                if let Some(ctrl) = template.controls.get(*idx) {
                    let new_idx = controls.len();
                    controls.push(ctrl.clone());
                    ctrl_data_records.push(template.ctrl_data_records.get(*idx).cloned().flatten());
                    new_items.push(ParagraphItem::Control(new_idx));
                    utf16_pos += 8;
                }
            }
        }
    }

    paragraph.text = text;
    paragraph.items = coalesce_items(new_items);
    paragraph.char_offsets = char_offsets;
    paragraph.controls = controls;
    paragraph.ctrl_data_records = ctrl_data_records;
    paragraph.char_count = utf16_pos + 1;
    paragraph.line_segs = template.line_segs.first().cloned().into_iter().collect();
    paragraph.range_tags.clear();
    paragraph.field_ranges.clear();
    paragraph.has_para_text = !paragraph.text.is_empty() || !paragraph.controls.is_empty();
    paragraph.char_shapes = vec![template
        .char_shapes
        .first()
        .cloned()
        .map(|mut cs| {
            cs.start_pos = 0;
            cs
        })
        .unwrap_or(CharShapeRef {
            start_pos: 0,
            char_shape_id: 0,
        })];
    paragraph
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

fn is_block_level_control(control: &Control) -> bool {
    matches!(control, Control::Table(_))
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

fn compact_text(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn contains_unsupported_korean_marker(text: &str, compact: &str) -> bool {
    const MARKERS: &[&str] = &[
        "국어 영역",
        "다음 글을 읽고",
        "다음 글을 읽고 물음에 답하시오",
        "윗글",
        "보기의 ⓐ",
    ];
    MARKERS.iter().any(|marker| text.contains(marker))
        || compact.contains("국어영역")
        || compact.contains("다음글을읽고")
}

fn document_text(doc: &Document) -> String {
    let mut out = String::new();
    for section in &doc.sections {
        for paragraph in &section.paragraphs {
            collect_paragraph_text(paragraph, &mut out, true);
        }
    }
    out
}

fn paragraph_text(paragraph: &Paragraph) -> String {
    let mut out = ordered_items(paragraph)
        .into_iter()
        .filter_map(|item| match item {
            ParagraphItem::Text(text) => Some(text),
            _ => None,
        })
        .collect::<String>();
    out.retain(|ch| !is_control_marker(ch));
    out
}

fn collect_paragraph_text(paragraph: &Paragraph, out: &mut String, include_nested: bool) {
    out.push_str(&paragraph_text(paragraph));
    if !include_nested {
        return;
    }
    for control in &paragraph.controls {
        collect_control_text(control, out);
    }
}

fn collect_control_text(control: &Control, out: &mut String) {
    match control {
        Control::Table(table) => {
            for cell in &table.cells {
                for paragraph in &cell.paragraphs {
                    collect_paragraph_text(paragraph, out, true);
                }
            }
        }
        Control::Header(header) => {
            for paragraph in &header.paragraphs {
                collect_paragraph_text(paragraph, out, true);
            }
        }
        Control::Footer(footer) => {
            for paragraph in &footer.paragraphs {
                collect_paragraph_text(paragraph, out, true);
            }
        }
        Control::Footnote(footnote) => {
            for paragraph in &footnote.paragraphs {
                collect_paragraph_text(paragraph, out, true);
            }
        }
        Control::Endnote(endnote) => {
            for paragraph in &endnote.paragraphs {
                collect_paragraph_text(paragraph, out, true);
            }
        }
        Control::HiddenComment(comment) => {
            for paragraph in &comment.paragraphs {
                collect_paragraph_text(paragraph, out, true);
            }
        }
        Control::Field(field) => out.push_str(&field.command),
        Control::Equation(eq) => out.push_str(&eq.script),
        _ => {}
    }
}

fn classify_paragraph(text: &str, korean: bool) -> ParaKind {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ParaKind::Empty;
    }
    if korean && set_header_match(text).is_some() {
        return ParaKind::SetHeader;
    }
    if balmun_number(trimmed).is_some() {
        return ParaKind::Balmun;
    }
    if trimmed
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, '①' | '②' | '③' | '④' | '⑤'))
    {
        return ParaKind::Seonji;
    }
    ParaKind::Other
}

fn balmun_number(text: &str) -> Option<u32> {
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
    digits.parse::<u32>().ok()
}

fn set_header_match(text: &str) -> Option<(u32, u32)> {
    for (start_byte, ch) in text.char_indices() {
        if ch != '[' {
            continue;
        }
        let rest = &text[start_byte + ch.len_utf8()..];
        if let Some(parsed) = parse_set_header_after_open(rest) {
            return Some(parsed);
        }
    }
    None
}

fn parse_set_header_after_open(text: &str) -> Option<(u32, u32)> {
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
    if chars.next()? != ']' {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn detect_questions(body: &[Paragraph], memo_mask: &[bool]) -> Vec<DetectedUnit> {
    let mut kinds: Vec<ParaKind> = body
        .iter()
        .map(|p| classify_paragraph(&paragraph_text(p), false))
        .collect();
    for (idx, kind) in kinds.iter_mut().enumerate() {
        if memo_mask.get(idx).copied().unwrap_or(false)
            && matches!(kind, ParaKind::Balmun | ParaKind::Seonji)
        {
            *kind = ParaKind::Other;
        }
    }

    let starts: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter_map(|(idx, kind)| (*kind == ParaKind::Balmun).then_some(idx))
        .collect();

    let mut units = Vec::new();
    for (unit_idx, &start) in starts.iter().enumerate() {
        let next_start = starts.get(unit_idx + 1).copied().unwrap_or(body.len());
        let mut last = start;
        let mut terminated = false;
        for j in start..next_start {
            match kinds[j] {
                ParaKind::Seonji => {
                    last = j;
                    terminated = true;
                }
                ParaKind::Balmun | ParaKind::Other => {
                    last = j;
                    if j != start && paragraph_text(&body[j]).contains("[4점]") {
                        terminated = true;
                    }
                }
                ParaKind::Empty if !terminated && has_visual_content(&body[j]) => {
                    last = j;
                }
                _ => {}
            }
        }
        let para_indices: Vec<usize> = (start..=last)
            .filter(|&idx| !memo_mask.get(idx).copied().unwrap_or(false))
            .collect();
        if para_indices.is_empty() {
            continue;
        }
        let roles = para_indices
            .iter()
            .map(|&idx| match kinds[idx] {
                ParaKind::Balmun => Role::Balmun,
                ParaKind::Seonji => Role::Seonji,
                _ => Role::Middle,
            })
            .collect();
        let q_num = balmun_number(&paragraph_text(&body[start])).unwrap_or((unit_idx + 1) as u32);
        units.push(DetectedUnit {
            label: format!("Q{q_num:02}"),
            para_indices,
            roles,
        });
    }
    units
}

fn detect_korean_sets(body: &[Paragraph], memo_mask: &[bool]) -> Vec<DetectedUnit> {
    let headers: Vec<(usize, u32, u32)> = body
        .iter()
        .enumerate()
        .filter(|(idx, _)| !memo_mask.get(*idx).copied().unwrap_or(false))
        .filter_map(|(idx, p)| set_header_match(&paragraph_text(p)).map(|(a, b)| (idx, a, b)))
        .collect();

    let mut units = Vec::new();
    for (unit_idx, &(start, from_q, to_q)) in headers.iter().enumerate() {
        let next_start = headers
            .get(unit_idx + 1)
            .map(|(idx, _, _)| *idx)
            .unwrap_or(body.len());
        let para_indices: Vec<usize> = (start..next_start)
            .filter(|&idx| !memo_mask.get(idx).copied().unwrap_or(false))
            .collect();
        let roles = para_indices
            .iter()
            .map(|&idx| {
                if idx == start {
                    Role::SetHeader
                } else {
                    match classify_paragraph(&paragraph_text(&body[idx]), true) {
                        ParaKind::Balmun => Role::Balmun,
                        ParaKind::Seonji => Role::Seonji,
                        _ => Role::Jimun,
                    }
                }
            })
            .collect();
        units.push(DetectedUnit {
            label: format!("S{from_q:02}-{to_q:02}"),
            para_indices,
            roles,
        });
    }
    units
}

fn has_visual_content(paragraph: &Paragraph) -> bool {
    paragraph.controls.iter().any(|c| {
        matches!(
            c,
            Control::Table(_) | Control::Picture(_) | Control::Shape(_)
        )
    })
}

fn build_memo_mask(doc: &Document) -> Vec<bool> {
    let Some(section) = doc.sections.first() else {
        return Vec::new();
    };
    section
        .paragraphs
        .iter()
        .map(|p| {
            let text = paragraph_text(p);
            let cs_id = p
                .char_shapes
                .iter()
                .find(|cs| cs.char_shape_id != u32::MAX)
                .map(|cs| cs.char_shape_id as usize)
                .unwrap_or(0);
            let non_black = doc
                .doc_info
                .char_shapes
                .get(cs_id)
                .is_some_and(|cs| cs.text_color != 0);
            let excel_style = doc
                .doc_info
                .styles
                .get(p.style_id as usize)
                .is_some_and(|style| {
                    let name = if style.local_name.is_empty() {
                        &style.english_name
                    } else {
                        &style.local_name
                    };
                    let lower = name.to_ascii_lowercase();
                    lower.starts_with("xl") && lower[2..].chars().all(|ch| ch.is_ascii_digit())
                });
            non_black || excel_style || is_review_request_text(&text)
        })
        .collect()
}

fn is_review_request_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.contains("주세요")
        || trimmed.contains("연구진")
        || trimmed.contains("출제진")
        || trimmed.contains("수정 사항")
}

fn disambiguate_labels(mut units: Vec<DetectedUnit>) -> Vec<DetectedUnit> {
    use std::collections::HashMap;
    let mut seen: HashMap<String, usize> = HashMap::new();
    for unit in &mut units {
        let count = seen.entry(unit.label.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            unit.label = format!("{}_{}", unit.label, count);
        }
    }
    units
}

fn is_control_marker(ch: char) -> bool {
    matches!(
        ch,
        '\u{0002}'
            | '\u{0003}'
            | '\u{0004}'
            | '\u{0005}'
            | '\u{0006}'
            | '\u{0007}'
            | '\u{0008}'
            | '\u{0009}'
            | '\u{000a}'
            | '\u{000b}'
            | '\u{000c}'
            | '\u{000d}'
            | '\u{0010}'
            | '\u{0011}'
            | '\u{0012}'
            | '\u{0013}'
            | '\u{0014}'
            | '\u{0015}'
            | '\u{0016}'
            | '\u{0017}'
            | '\u{0018}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Paragraph {
        Paragraph {
            text: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn detects_subject_markers() {
        assert_eq!(
            detect_subject_from_text("2026학년도 수학 영역 5지선다형 단답형").unwrap(),
            Subject::Math
        );
        assert_eq!(
            detect_subject_from_text("통합과학 탐구").unwrap(),
            Subject::Science
        );
        assert!(matches!(
            detect_subject_from_text("국어 영역 다음 글을 읽고"),
            Err(SplitError::UnsupportedKorean)
        ));
    }

    #[test]
    fn detects_question_units() {
        let doc = Document {
            sections: vec![crate::model::document::Section {
                paragraphs: vec![
                    p("1. 첫 번째 문항이다."),
                    p("자료 설명"),
                    p("① ㄱ ② ㄴ"),
                    p("2. 두 번째 문항이다."),
                    p("이에 대한 설명으로 옳은 것은?"),
                    p("① A ② B"),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let units = detect_units(&doc, Subject::Math).unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].label, "Q01");
        assert_eq!(units[0].para_indices, vec![0, 1, 2]);
        assert_eq!(units[1].label, "Q02");
        assert_eq!(units[1].para_indices, vec![3, 4, 5]);
    }
}
