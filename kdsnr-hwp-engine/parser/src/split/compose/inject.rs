//! Inject composed paragraphs into the template's section body.
//!
//! The template section's first paragraph is the structural carrier: it holds
//! the `SectionDef` (page setup, master-page refs), `ColumnDef`, and header/
//! footer controls. We keep that carrier and append the composed paragraphs
//! after it so the output frame comes entirely from the template.

use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::paragraph::{Paragraph, ParagraphItem};

fn zero_line_seg(seg: &mut crate::model::paragraph::LineSeg) {
    seg.text_start = 0;
    seg.vertical_pos = 0;
    seg.line_height = 0;
    seg.text_height = 0;
    seg.baseline_distance = 0;
    seg.line_spacing = 0;
    seg.column_start = 0;
    seg.segment_width = 0;
}

/// Clean the carrier for injection: keep frame controls and drop printable
/// cover objects/text. The carrier's visible advance is subject policy.
fn clean_carrier(carrier: &Paragraph, keep_spacer: bool) -> Paragraph {
    let mut c = carrier.clone();
    let mut controls = Vec::new();
    let mut ctrl_data = Vec::new();
    for (i, ctl) in c.controls.iter().enumerate() {
        if matches!(
            ctl,
            Control::Table(_) | Control::Picture(_) | Control::Shape(_)
        ) {
            continue;
        }
        controls.push(ctl.clone());
        ctrl_data.push(c.ctrl_data_records.get(i).cloned().flatten());
    }
    c.controls = controls;
    c.ctrl_data_records = ctrl_data;
    c.items = (0..c.controls.len()).map(ParagraphItem::Control).collect();
    c.text.clear();
    c.char_offsets.clear();
    c.char_shapes.truncate(1);
    c.range_tags.clear();
    c.field_ranges.clear();
    c.tab_extended.clear();
    if keep_spacer {
        if let Some(seg) = c.line_segs.first_mut() {
            seg.text_start = 0;
        }
    } else if let Some(seg) = c.line_segs.first_mut() {
        zero_line_seg(seg);
    }
    c.line_segs.truncate(1);
    c.char_count = c.controls.len() as u32 + 1;
    c
}

/// Replace the merged template's body with `[structural carrier] + composed`.
/// When `show_master_on_first_page`, clear the section's first-page master-page
/// suppression so the running band and dividers show from page 1.
pub fn inject(
    mut merged: Document,
    composed: Vec<Paragraph>,
    show_master_on_first_page: bool,
    keep_carrier_spacer: bool,
) -> Document {
    if let Some(sec) = merged.sections.first_mut() {
        if show_master_on_first_page {
            sec.section_def.hide_master_page = false;
        }
        let mut body = Vec::with_capacity(composed.len() + 1);
        if let Some(carrier) = sec.paragraphs.first() {
            body.push(clean_carrier(carrier, keep_carrier_spacer));
        }
        body.extend(composed);
        sec.paragraphs = body;
        sec.raw_stream = None;
    }
    for sec in merged.sections.iter_mut().skip(1) {
        sec.paragraphs.clear();
        sec.raw_stream = None;
    }
    merged.doc_info.raw_stream_dirty = true;
    merged
}
