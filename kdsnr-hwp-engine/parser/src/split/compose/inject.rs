//! Inject composed paragraphs into the template's section body.
//!
//! A section's first paragraph is the structural carrier — it holds the
//! `SectionDef` (page setup, master-page refs) and `ColumnDef` controls and no
//! visible text in the math/science/social templates. We keep that carrier and
//! append the composed question paragraphs after it, so the output's section
//! frame comes entirely from the template. Trailing sections (templates are
//! single-section) are emptied.

use crate::model::document::Document;
use crate::model::paragraph::Paragraph;

/// Replace the merged template's body with `[structural carrier] + composed`.
pub fn inject(mut merged: Document, composed: Vec<Paragraph>) -> Document {
    if let Some(sec) = merged.sections.first_mut() {
        let mut body = Vec::with_capacity(composed.len() + 1);
        if let Some(carrier) = sec.paragraphs.first() {
            body.push(carrier.clone());
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
