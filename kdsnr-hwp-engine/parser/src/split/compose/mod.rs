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
pub mod rewrite;
pub mod slot_catalog;

pub use apply_atom::apply_atom;
pub use inject::inject;
pub use merge_styles::{merge_styles, IdMaps};
pub use rewrite::remap_paragraph;
pub use slot_catalog::{build_catalog, Slot, SubjectCatalog};

use std::sync::OnceLock;

use crate::model::document::Document;
use crate::model::paragraph::{ColumnBreakType, Paragraph};
use crate::parse_document;
use crate::split::{classify_atom, AtomKind, Subject};

const MATH_HWPX: &[u8] = include_bytes!("../../../../../templet/math.hwpx");
const SCIENCE_HWPX: &[u8] = include_bytes!("../../../../../templet/science.hwpx");
const SOCIAL_HWPX: &[u8] = include_bytes!("../../../../../templet/social.hwpx");

/// Parsed subject template, cached for the process lifetime.
fn template_doc(subject: Subject) -> &'static Document {
    static MATH: OnceLock<Document> = OnceLock::new();
    static SCIENCE: OnceLock<Document> = OnceLock::new();
    static SOCIAL: OnceLock<Document> = OnceLock::new();
    let (cell, bytes, name) = match subject {
        Subject::Math => (&MATH, MATH_HWPX, "math"),
        Subject::Science => (&SCIENCE, SCIENCE_HWPX, "science"),
        Subject::Social => (&SOCIAL, SOCIAL_HWPX, "social"),
        Subject::Korean => (&MATH, MATH_HWPX, "math"), // korean is gated upstream
    };
    cell.get_or_init(|| {
        parse_document(bytes).unwrap_or_else(|e| panic!("bundled {name} template parse: {e:?}"))
    })
}

/// Compose one question's paragraphs into a fresh subject template document.
pub fn compose_question(src: &Document, subject: Subject, unit_paragraphs: &[Paragraph]) -> Document {
    let template = template_doc(subject);
    let catalog = build_catalog(template, subject);
    let (merged, maps) = merge_styles(template, src);

    let mut composed = Vec::with_capacity(unit_paragraphs.len());
    let mut prev_atom = None;
    for p in unit_paragraphs {
        // Classify on the source ids (before remapping) — the classifier keys
        // on source-specific para_shape/style ids.
        let atom = classify_atom(p, subject, prev_atom);
        if matches!(atom, AtomKind::Empty | AtomKind::Unknown) {
            continue;
        }
        prev_atom = Some(atom);

        let mut q = p.clone();
        remap_paragraph(&mut q, &maps);
        match catalog.atom_to_slot.get(&atom) {
            Some(slot) => composed.push(apply_atom(q, atom, slot)),
            // No template slot for this atom: keep the source paragraph (already
            // remapped, segments intact) so its content is never dropped.
            None => {
                q.column_type = ColumnBreakType::None;
                q.raw_break_type = 0;
                composed.push(q);
            }
        }
    }

    inject(merged, composed)
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
