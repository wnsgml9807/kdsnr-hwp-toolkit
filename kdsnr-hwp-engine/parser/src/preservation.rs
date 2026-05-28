//! HWP -> HWPX preservation contract.
//!
//! HWP binary and HWPX do not always place equivalent layout information on
//! the same node. This module normalizes the IR just before HWPX serialization
//! so downstream renderers see the same visual contract the source HWP implied.

use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::paragraph::Paragraph;
use crate::model::shape::ShapeObject;
use crate::model::style::{BorderFill, Numbering};
use crate::model::table::Table;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreservationStats {
    pub hancom_default_border_fill_inserted: usize,
    pub hancom_default_numbering_inserted: usize,
    pub table_cell_borders_promoted: usize,
    pub cell_justify_para_shapes_cloned: usize,
    pub cell_paragraphs_left_aligned: usize,
}

/// Apply the HWPX preservation contract in-place.
///
/// Contract currently covered:
/// - Insert Hancom's implicit default numbering resource for HWP origin documents
///   while preserving source layout records.
///
/// NOTE: the default-borderFill insertion was removed — Hancom's own HWPX export
/// keeps the source borderFill list and 1-based IDs unchanged (verified against
/// the original .hwpx corpus: math 9, social 17). Inserting a default at index 0
/// shifted every ID by one; pageBorderFill refs were not bumped, so the page got
/// a spurious full border. Keeping source IDs matches Hancom and renders cleanly.
pub fn apply_hwpx_preservation_contract(doc: &mut Document) -> PreservationStats {
    let mut stats = PreservationStats::default();
    insert_hancom_default_numbering_for_hwp(doc, &mut stats);
    stats
}

fn insert_hancom_default_numbering_for_hwp(doc: &mut Document, stats: &mut PreservationStats) {
    if doc.header.raw_data.is_none() {
        return;
    }
    if doc.doc_info.numberings.is_empty() {
        return;
    }
    if doc
        .doc_info
        .numberings
        .first()
        .is_some_and(|n| n.start_number == 0 && n.level_start_numbers.iter().all(|v| *v == 0))
    {
        return;
    }

    doc.doc_info.numberings.insert(0, Numbering::default());
    stats.hancom_default_numbering_inserted = 1;
}

#[allow(dead_code)] // retired (see apply_hwpx_preservation_contract); kept for reference
fn insert_hancom_default_border_fill_for_hwp(doc: &mut Document, stats: &mut PreservationStats) {
    if doc.header.raw_data.is_none() {
        return;
    }
    if doc.doc_info.border_fills.is_empty() {
        return;
    }

    doc.doc_info
        .border_fills
        .insert(0, hancom_default_border_fill());
    stats.hancom_default_border_fill_inserted = 1;

    for char_shape in &mut doc.doc_info.char_shapes {
        bump_border_ref(&mut char_shape.border_fill_id);
    }
    for para_shape in &mut doc.doc_info.para_shapes {
        bump_border_ref(&mut para_shape.border_fill_id);
    }
    for section in &mut doc.sections {
        // Hancom inserts an implicit default borderFill at header index 0, but
        // pageBorderFill IDs in exported HWPX stay on the source value.
        for paragraph in &mut section.paragraphs {
            bump_border_refs_in_paragraph(paragraph);
        }
    }
}

fn hancom_default_border_fill() -> BorderFill {
    BorderFill::default()
}

fn bump_border_ref(id: &mut u16) {
    if *id > 0 {
        *id = id.saturating_add(1);
    }
}

fn bump_border_refs_in_paragraph(paragraph: &mut Paragraph) {
    for control in &mut paragraph.controls {
        bump_border_refs_in_control(control);
    }
}

fn bump_border_refs_in_control(control: &mut Control) {
    match control {
        Control::Table(table) => bump_border_refs_in_table(table),
        Control::Header(header) => {
            for paragraph in &mut header.paragraphs {
                bump_border_refs_in_paragraph(paragraph);
            }
        }
        Control::Footer(footer) => {
            for paragraph in &mut footer.paragraphs {
                bump_border_refs_in_paragraph(paragraph);
            }
        }
        Control::Footnote(note) => {
            for paragraph in &mut note.paragraphs {
                bump_border_refs_in_paragraph(paragraph);
            }
        }
        Control::Endnote(note) => {
            for paragraph in &mut note.paragraphs {
                bump_border_refs_in_paragraph(paragraph);
            }
        }
        Control::HiddenComment(comment) => {
            for paragraph in &mut comment.paragraphs {
                bump_border_refs_in_paragraph(paragraph);
            }
        }
        Control::Shape(shape) => bump_border_refs_in_shape(shape),
        _ => {}
    }
}

fn bump_border_refs_in_table(table: &mut Table) {
    bump_border_ref(&mut table.border_fill_id);
    for zone in &mut table.zones {
        bump_border_ref(&mut zone.border_fill_id);
    }
    for cell in &mut table.cells {
        bump_border_ref(&mut cell.border_fill_id);
        for paragraph in &mut cell.paragraphs {
            bump_border_refs_in_paragraph(paragraph);
        }
    }
}

fn bump_border_refs_in_shape(shape: &mut ShapeObject) {
    match shape {
        ShapeObject::Line(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Rectangle(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Ellipse(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Arc(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Polygon(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Curve(s) => bump_border_refs_in_drawing(&mut s.drawing),
        ShapeObject::Group(group) => {
            for child in &mut group.children {
                bump_border_refs_in_shape(child);
            }
        }
        ShapeObject::Picture(pic) => {
            if let Some(caption) = &mut pic.caption {
                for paragraph in &mut caption.paragraphs {
                    bump_border_refs_in_paragraph(paragraph);
                }
            }
        }
        ShapeObject::Chart(chart) => {
            bump_border_refs_in_drawing(&mut chart.drawing);
            if let Some(caption) = &mut chart.caption {
                for paragraph in &mut caption.paragraphs {
                    bump_border_refs_in_paragraph(paragraph);
                }
            }
        }
        ShapeObject::Ole(ole) => {
            bump_border_refs_in_drawing(&mut ole.drawing);
            if let Some(caption) = &mut ole.caption {
                for paragraph in &mut caption.paragraphs {
                    bump_border_refs_in_paragraph(paragraph);
                }
            }
        }
    }
}

fn bump_border_refs_in_drawing(drawing: &mut crate::model::shape::DrawingObjAttr) {
    if let Some(text_box) = &mut drawing.text_box {
        for paragraph in &mut text_box.paragraphs {
            bump_border_refs_in_paragraph(paragraph);
        }
    }
}
