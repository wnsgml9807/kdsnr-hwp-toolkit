//! Derive the atom→slot table from a subject template.
//!
//! Each subject template carries one example paragraph per "slot" (balmun,
//! seonji, data box, bogi box, ...). We classify every top-level template
//! paragraph with the shared `classify_atom` and record, per atom, the
//! canonical (para_shape, style, char_shape) triple plus a representative
//! paragraph (needed to clone box shells). `apply_atom` then retargets each
//! source paragraph to its atom's slot.

use std::collections::HashMap;

use crate::model::document::Document;
use crate::model::paragraph::Paragraph;
use crate::model::style::HeadType;
use crate::split::{classify_atom, first_table, is_bogi_table, AtomKind, Subject};

/// A template slot: the paragraph-level styling an atom adopts in the output.
#[derive(Debug, Clone)]
pub struct Slot {
    pub para_shape_id: u16,
    pub style_id: u8,
    /// First char_shape of the slot (the unified body font for the atom).
    pub char_shape_id: u32,
    /// For balmun: a distinct char_shape used after the leading "N." run.
    pub body_char_shape_id: Option<u32>,
    /// Representative template paragraph (box shell clone source).
    pub template_paragraph: Option<Paragraph>,
    /// Science/social bogi: keep the source table instead of cloning the shell.
    pub preserve_source_table: bool,
    /// Science/social: flatten tiny 1x1 marker tables inside cells back to text.
    pub flatten_cell_marker_tables: bool,
}

/// The atom→slot table for one subject.
#[derive(Debug, Clone, Default)]
pub struct SubjectCatalog {
    pub atom_to_slot: HashMap<AtomKind, Slot>,
}

fn char_shape_first(p: &Paragraph) -> u32 {
    p.char_shapes.first().map(|c| c.char_shape_id).unwrap_or(0)
}

/// First run whose char_shape differs from the paragraph's first run — the
/// balmun template's body style (the leading "N." run uses the first style).
fn body_char_shape(p: &Paragraph, first: u32) -> Option<u32> {
    p.char_shapes
        .iter()
        .map(|c| c.char_shape_id)
        .find(|&id| id != first)
}

/// Build the atom→slot table from a subject template. `preserve_source_bogi`
/// keeps the source bogi table verbatim (and flattens cell marker tables)
/// instead of cloning the template's box shell.
pub fn build_catalog(
    template: &Document,
    subject: Subject,
    preserve_source_bogi: bool,
) -> SubjectCatalog {
    let empty: Vec<Paragraph> = Vec::new();
    let body = template
        .sections
        .first()
        .map(|s| &s.paragraphs)
        .unwrap_or(&empty);

    let mut by_atom: HashMap<AtomKind, Vec<&Paragraph>> = HashMap::new();
    let mut prev = None;
    for p in body {
        let atom = if subject == Subject::Korean && is_numbered_paragraph(template, p) {
            AtomKind::Balmun
        } else {
            classify_atom(p, subject, prev)
        };
        if matches!(atom, AtomKind::Empty | AtomKind::Unknown) {
            continue;
        }
        prev = Some(atom);
        by_atom.entry(atom).or_default().push(p);
    }

    let mut atom_to_slot = HashMap::new();
    for (atom, paras) in by_atom {
        // Most-common (para_shape, style, char) triple for this atom.
        let mut counts: HashMap<(u16, u8, u32), usize> = HashMap::new();
        for p in &paras {
            *counts
                .entry((p.para_shape_id, p.style_id, char_shape_first(p)))
                .or_default() += 1;
        }
        let chosen = counts
            .iter()
            .max_by_key(|(_, c)| **c)
            .map(|(k, _)| *k)
            .unwrap_or((0, 0, 0));

        // Representative paragraph for cloning: for bogi boxes prefer a clean
        // bogi-marked wrapper (table only, no fused text); otherwise the first
        // paragraph matching the chosen triple.
        let rep = if atom == AtomKind::BogiBox {
            paras
                .iter()
                .copied()
                .find(|p| first_table(p).is_some_and(is_bogi_table) && p.text.trim().is_empty())
                .or_else(|| {
                    paras
                        .iter()
                        .copied()
                        .find(|p| first_table(p).is_some_and(is_bogi_table))
                })
                .unwrap_or(paras[0])
        } else {
            paras
                .iter()
                .copied()
                .find(|p| (p.para_shape_id, p.style_id, char_shape_first(p)) == chosen)
                .unwrap_or(paras[0])
        };
        let mut slot_char_shape = chosen.2;
        let mut balmun_body_char_shape = body_char_shape(rep, chosen.2);
        if atom == AtomKind::Balmun && balmun_body_char_shape.is_none() {
            if let Some(number_cs) = numbering_head_char_shape(template, rep) {
                slot_char_shape = number_cs;
                balmun_body_char_shape = Some(chosen.2);
            }
        }

        atom_to_slot.insert(
            atom,
            Slot {
                para_shape_id: chosen.0,
                style_id: chosen.1,
                char_shape_id: slot_char_shape,
                body_char_shape_id: balmun_body_char_shape,
                template_paragraph: Some(rep.clone()),
                preserve_source_table: preserve_source_bogi && atom == AtomKind::BogiBox,
                flatten_cell_marker_tables: preserve_source_bogi,
            },
        );
    }

    SubjectCatalog { atom_to_slot }
}

fn numbering_head_char_shape(template: &Document, p: &Paragraph) -> Option<u32> {
    let para_shape = template
        .doc_info
        .para_shapes
        .get(p.para_shape_id as usize)?;
    if para_shape.head_type != HeadType::Number || para_shape.numbering_id == 0 {
        return None;
    }
    let numbering = template
        .doc_info
        .numberings
        .get(para_shape.numbering_id.saturating_sub(1) as usize)?;
    let char_shape_id = numbering
        .heads
        .get(para_shape.para_level as usize)
        .map(|head| head.char_shape_id)?;
    (char_shape_id != u32::MAX).then_some(char_shape_id)
}

fn is_numbered_paragraph(template: &Document, p: &Paragraph) -> bool {
    template
        .doc_info
        .para_shapes
        .get(p.para_shape_id as usize)
        .is_some_and(|para_shape| para_shape.head_type == HeadType::Number)
}
