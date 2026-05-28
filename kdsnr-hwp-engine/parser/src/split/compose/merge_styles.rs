//! Cross-document catalog merge for template composition.
//!
//! Merges a source document's `DocInfo` style/resource catalogs into a
//! template document, deduping logically-equal entries, and returns `IdMaps`
//! translating every source id to its template-side id. `compose::rewrite`
//! consumes the maps to retarget the source paragraphs that get injected into
//! the template.
//!
//! Id conventions (from the engine's resolvers in `kdsnr-hwp-doc`):
//! char_shapes / para_shapes / styles / fonts / tab_defs are 0-based vector
//! indices; border_fills and numberings are 1-based (id 0 = none). Dedup
//! resolves a source entry's internal refs into template id-space first, then
//! compares the remapped entry by value.

use std::collections::HashMap;

use crate::model::document::Document;
use crate::model::style::{BorderFill, CharShape, Font, Numbering, ParaShape, Style};

/// Source-id → template-id translation for every merged catalog.
#[derive(Debug, Default, Clone)]
pub struct IdMaps {
    /// Per-language (한글/영문/한자/일어/기타/기호/사용자) font index maps.
    pub fonts: [HashMap<u16, u16>; 7],
    /// char_shape index (0-based; `CharShapeRef.char_shape_id` is u32).
    pub char_shapes: HashMap<u32, u32>,
    /// para_shape index (0-based).
    pub para_shapes: HashMap<u16, u16>,
    /// style id (0-based; `Paragraph.style_id` is u8).
    pub styles: HashMap<u8, u8>,
    /// border_fill id (1-based; 0 = none).
    pub border_fills: HashMap<u16, u16>,
    /// tab_def id (0-based).
    pub tab_defs: HashMap<u16, u16>,
    /// numbering id (1-based; 0 = none).
    pub numberings: HashMap<u16, u16>,
    /// bullet index (0-based).
    pub bullets: HashMap<u16, u16>,
    /// BinData storage id.
    pub bin_data: HashMap<u16, u16>,
}

impl IdMaps {
    pub fn font(&self, lang: usize, id: u16) -> u16 {
        self.fonts
            .get(lang)
            .and_then(|m| m.get(&id))
            .copied()
            .unwrap_or(id)
    }
    pub fn char_shape(&self, id: u32) -> u32 {
        self.char_shapes.get(&id).copied().unwrap_or(id)
    }
    pub fn para_shape(&self, id: u16) -> u16 {
        self.para_shapes.get(&id).copied().unwrap_or(id)
    }
    pub fn style(&self, id: u8) -> u8 {
        self.styles.get(&id).copied().unwrap_or(id)
    }
    /// border_fill is 1-based: id 0 (none) always maps to 0.
    pub fn border_fill(&self, id: u16) -> u16 {
        if id == 0 {
            return 0;
        }
        self.border_fills.get(&id).copied().unwrap_or(id)
    }
    pub fn tab_def(&self, id: u16) -> u16 {
        self.tab_defs.get(&id).copied().unwrap_or(id)
    }
    /// numbering is 1-based: id 0 (none) always maps to 0.
    pub fn numbering(&self, id: u16) -> u16 {
        if id == 0 {
            return 0;
        }
        self.numberings.get(&id).copied().unwrap_or(id)
    }
    pub fn bin_data(&self, id: u16) -> u16 {
        self.bin_data.get(&id).copied().unwrap_or(id)
    }
}

/// Two fonts are interchangeable when they name the same face with the same
/// substitution metadata (raw bytes ignored).
fn font_eq(a: &Font, b: &Font) -> bool {
    a.name == b.name
        && a.alt_type == b.alt_type
        && a.alt_name == b.alt_name
        && a.default_name == b.default_name
}

/// Style dedup ignores `next_style_id` (a forward ref resolved in a second
/// pass) and compares the visible identity + remapped shape refs.
fn style_dedup_eq(a: &Style, b: &Style) -> bool {
    a.local_name == b.local_name
        && a.english_name == b.english_name
        && a.style_type == b.style_type
        && a.para_shape_id == b.para_shape_id
        && a.char_shape_id == b.char_shape_id
}

fn remap_char_shape(src: &CharShape, maps: &IdMaps) -> CharShape {
    let mut cs = src.clone();
    cs.raw_data = None;
    for lang in 0..7 {
        cs.font_ids[lang] = maps.font(lang, src.font_ids[lang]);
    }
    cs.border_fill_id = maps.border_fill(src.border_fill_id);
    cs
}

fn remap_border_fill(src: &BorderFill, maps: &IdMaps) -> BorderFill {
    let mut bf = src.clone();
    bf.raw_data = None;
    if let Some(img) = bf.fill.image.as_mut() {
        img.bin_data_id = maps.bin_data(img.bin_data_id);
    }
    bf
}

fn remap_numbering(src: &Numbering, maps: &IdMaps) -> Numbering {
    let mut nb = src.clone();
    nb.raw_data = None;
    for head in nb.heads.iter_mut() {
        head.char_shape_id = maps.char_shape(head.char_shape_id);
    }
    nb
}

fn remap_para_shape(src: &ParaShape, maps: &IdMaps) -> ParaShape {
    let mut ps = src.clone();
    ps.raw_data = None;
    ps.tab_def_id = maps.tab_def(src.tab_def_id);
    ps.numbering_id = maps.numbering(src.numbering_id);
    ps.border_fill_id = maps.border_fill(src.border_fill_id);
    ps
}

/// Merge `src`'s catalogs into a clone of `template`. Returns the extended
/// template document and the source→template id translation maps.
pub fn merge_styles(template: &Document, src: &Document) -> (Document, IdMaps) {
    let mut out = template.clone();
    let mut maps = IdMaps::default();

    let tpl = &template.doc_info;
    let src_info = &src.doc_info;

    // ── BinData (storage id; dedup by embedded content bytes) ──
    let mut out_bin_list = tpl.bin_data_list.clone();
    let mut out_bin_content = template.bin_data_content.clone();
    for sb in &src_info.bin_data_list {
        let src_bytes = src
            .bin_data_content
            .iter()
            .find(|c| c.id == sb.storage_id)
            .map(|c| &c.data);
        let matched = out_bin_list.iter().find(|db| {
            let dst_bytes = out_bin_content
                .iter()
                .find(|c| c.id == db.storage_id)
                .map(|c| &c.data);
            db.data_type == sb.data_type && dst_bytes == src_bytes
        });
        if let Some(db) = matched {
            maps.bin_data.insert(sb.storage_id, db.storage_id);
            continue;
        }
        let new_id = out_bin_list
            .iter()
            .map(|b| b.storage_id)
            .max()
            .map_or(1, |m| m + 1);
        let mut nb = sb.clone();
        nb.raw_data = None;
        nb.storage_id = new_id;
        out_bin_list.push(nb);
        if let Some(content) = src.bin_data_content.iter().find(|c| c.id == sb.storage_id) {
            let mut nc = content.clone();
            nc.id = new_id;
            out_bin_content.push(nc);
        }
        maps.bin_data.insert(sb.storage_id, new_id);
    }

    // ── Fonts (per language, 0-based; dedup by face identity) ──
    let mut out_fonts: Vec<Vec<Font>> = tpl.font_faces.clone();
    for (lang, src_lang) in src_info.font_faces.iter().enumerate() {
        if out_fonts.len() <= lang {
            out_fonts.resize(lang + 1, Vec::new());
        }
        for (si, sf) in src_lang.iter().enumerate() {
            if let Some(di) = out_fonts[lang].iter().position(|df| font_eq(df, sf)) {
                maps.fonts[lang].insert(si as u16, di as u16);
            } else {
                let new = out_fonts[lang].len() as u16;
                out_fonts[lang].push(sf.clone());
                maps.fonts[lang].insert(si as u16, new);
            }
        }
    }

    // ── BorderFill (1-based; refs bin_data) ──
    let mut out_bf = tpl.border_fills.clone();
    for (si, sb) in src_info.border_fills.iter().enumerate() {
        let remapped = remap_border_fill(sb, &maps);
        if let Some(di) = out_bf.iter().position(|db| *db == remapped) {
            maps.border_fills.insert((si + 1) as u16, (di + 1) as u16);
        } else {
            let new = out_bf.len();
            out_bf.push(remapped);
            maps.border_fills.insert((si + 1) as u16, (new + 1) as u16);
        }
    }

    // ── CharShape (0-based; refs fonts + border_fill) ──
    let mut out_cs = tpl.char_shapes.clone();
    for (si, sc) in src_info.char_shapes.iter().enumerate() {
        let remapped = remap_char_shape(sc, &maps);
        if let Some(di) = out_cs.iter().position(|dc| *dc == remapped) {
            maps.char_shapes.insert(si as u32, di as u32);
        } else {
            let new = out_cs.len() as u32;
            out_cs.push(remapped);
            maps.char_shapes.insert(si as u32, new);
        }
    }

    // ── TabDef (0-based; no internal refs) ──
    let mut out_td = tpl.tab_defs.clone();
    for (si, st) in src_info.tab_defs.iter().enumerate() {
        if let Some(di) = out_td.iter().position(|dt| dt == st) {
            maps.tab_defs.insert(si as u16, di as u16);
        } else {
            let new = out_td.len() as u16;
            let mut e = st.clone();
            e.raw_data = None;
            out_td.push(e);
            maps.tab_defs.insert(si as u16, new);
        }
    }

    // ── Numbering (1-based; refs char_shape via heads) ──
    let mut out_nb = tpl.numberings.clone();
    for (si, sn) in src_info.numberings.iter().enumerate() {
        let remapped = remap_numbering(sn, &maps);
        if let Some(di) = out_nb.iter().position(|dn| *dn == remapped) {
            maps.numberings.insert((si + 1) as u16, (di + 1) as u16);
        } else {
            let new = out_nb.len();
            out_nb.push(remapped);
            maps.numberings.insert((si + 1) as u16, (new + 1) as u16);
        }
    }

    // ── Bullet (0-based; no internal refs) ──
    let mut out_bullets = tpl.bullets.clone();
    for (si, sbl) in src_info.bullets.iter().enumerate() {
        if let Some(di) = out_bullets.iter().position(|db| db == sbl) {
            maps.bullets.insert(si as u16, di as u16);
        } else {
            let new = out_bullets.len() as u16;
            let mut e = sbl.clone();
            e.raw_data = None;
            out_bullets.push(e);
            maps.bullets.insert(si as u16, new);
        }
    }

    // ── ParaShape (0-based; refs tab_def + numbering + border_fill) ──
    let mut out_ps = tpl.para_shapes.clone();
    for (si, sp) in src_info.para_shapes.iter().enumerate() {
        let remapped = remap_para_shape(sp, &maps);
        if let Some(di) = out_ps.iter().position(|dp| *dp == remapped) {
            maps.para_shapes.insert(si as u16, di as u16);
        } else {
            let new = out_ps.len() as u16;
            out_ps.push(remapped);
            maps.para_shapes.insert(si as u16, new);
        }
    }

    // ── Style (0-based; refs para_shape + char_shape; next_style_id resolved
    //    in a second pass since it forward-refs other styles) ──
    let mut out_st = tpl.styles.clone();
    let mut appended: Vec<usize> = Vec::new(); // out indices of newly appended styles
    for (si, ss) in src_info.styles.iter().enumerate() {
        let mut remapped = ss.clone();
        remapped.raw_data = None;
        remapped.para_shape_id = maps.para_shape(ss.para_shape_id);
        remapped.char_shape_id = maps.char_shape(ss.char_shape_id as u32) as u16;
        if let Some(di) = out_st.iter().position(|ds| style_dedup_eq(ds, &remapped)) {
            maps.styles.insert(si as u8, di as u8);
        } else {
            let new = out_st.len();
            out_st.push(remapped);
            appended.push(new);
            maps.styles.insert(si as u8, new as u8);
        }
    }
    // Second pass: retarget next_style_id on appended styles (src id-space).
    for &idx in &appended {
        let old_next = out_st[idx].next_style_id;
        out_st[idx].next_style_id = maps.style(old_next);
    }

    out.doc_info.bin_data_list = out_bin_list;
    out.bin_data_content = out_bin_content;
    out.doc_info.font_faces = out_fonts;
    out.doc_info.border_fills = out_bf;
    out.doc_info.char_shapes = out_cs;
    out.doc_info.tab_defs = out_td;
    out.doc_info.numberings = out_nb;
    out.doc_info.bullets = out_bullets;
    out.doc_info.para_shapes = out_ps;
    out.doc_info.styles = out_st;
    out.doc_info.raw_stream = None;
    out.doc_info.raw_stream_dirty = true;

    (out, maps)
}
