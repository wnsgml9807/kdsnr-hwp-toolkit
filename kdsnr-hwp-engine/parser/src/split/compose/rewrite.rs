//! Retarget a source paragraph's catalog ids into template id-space.
//!
//! After `merge_styles` brings the source's styles into the template and
//! produces `IdMaps`, every paragraph copied out of the source must have its
//! ids rewritten so they point at the merged template catalogs. This walks the
//! paragraph and all nested controls (tables, pictures, shapes, notes).

use super::merge_styles::IdMaps;
use crate::model::control::Control;
use crate::model::paragraph::Paragraph;
use crate::model::shape::ShapeObject;
use crate::model::table::Table;

/// Remap every catalog id inside `p` and its nested controls, in place.
pub fn remap_paragraph(p: &mut Paragraph, maps: &IdMaps) {
    p.para_shape_id = maps.para_shape(p.para_shape_id);
    p.style_id = maps.style(p.style_id);
    for cs in &mut p.char_shapes {
        cs.char_shape_id = maps.char_shape(cs.char_shape_id);
    }
    for c in &mut p.controls {
        remap_control(c, maps);
    }
}

fn remap_paragraphs(ps: &mut [Paragraph], maps: &IdMaps) {
    for p in ps {
        remap_paragraph(p, maps);
    }
}

fn remap_control(c: &mut Control, maps: &IdMaps) {
    match c {
        Control::Table(t) => remap_table(t, maps),
        Control::Picture(pic) => {
            pic.image_attr.bin_data_id = maps.bin_data(pic.image_attr.bin_data_id);
            if let Some(cap) = pic.caption.as_mut() {
                remap_paragraphs(&mut cap.paragraphs, maps);
            }
        }
        Control::Shape(s) => remap_shape(s, maps),
        Control::Header(h) => remap_paragraphs(&mut h.paragraphs, maps),
        Control::Footer(f) => remap_paragraphs(&mut f.paragraphs, maps),
        Control::Footnote(f) => remap_paragraphs(&mut f.paragraphs, maps),
        Control::Endnote(e) => remap_paragraphs(&mut e.paragraphs, maps),
        Control::HiddenComment(hc) => remap_paragraphs(&mut hc.paragraphs, maps),
        Control::CharOverlap(co) => {
            for id in &mut co.char_shape_ids {
                *id = maps.char_shape(*id);
            }
        }
        // Equations carry their own font metadata; remaining controls hold no
        // doc-catalog references (or are structural and come from the template).
        _ => {}
    }
}

fn remap_table(t: &mut Table, maps: &IdMaps) {
    t.border_fill_id = maps.border_fill(t.border_fill_id);
    for z in &mut t.zones {
        z.border_fill_id = maps.border_fill(z.border_fill_id);
    }
    for cell in &mut t.cells {
        cell.border_fill_id = maps.border_fill(cell.border_fill_id);
        remap_paragraphs(&mut cell.paragraphs, maps);
    }
    if let Some(cap) = t.caption.as_mut() {
        remap_paragraphs(&mut cap.paragraphs, maps);
    }
}

fn remap_shape(s: &mut ShapeObject, maps: &IdMaps) {
    match s {
        ShapeObject::Group(g) => {
            for child in &mut g.children {
                remap_shape(child, maps);
            }
            if let Some(cap) = g.caption.as_mut() {
                remap_paragraphs(&mut cap.paragraphs, maps);
            }
        }
        ShapeObject::Picture(pic) => {
            pic.image_attr.bin_data_id = maps.bin_data(pic.image_attr.bin_data_id);
            if let Some(cap) = pic.caption.as_mut() {
                remap_paragraphs(&mut cap.paragraphs, maps);
            }
        }
        other => {
            if let Some(d) = other.drawing_mut() {
                if let Some(tb) = d.text_box.as_mut() {
                    remap_paragraphs(&mut tb.paragraphs, maps);
                }
                if let Some(cap) = d.caption.as_mut() {
                    remap_paragraphs(&mut cap.paragraphs, maps);
                }
            }
        }
    }
}
