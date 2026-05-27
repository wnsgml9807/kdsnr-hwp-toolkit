//! Per-character advance (자간/장평). See `docs/FONT_MODEL.md` §2.3.
//!
//! `advance = round((glyph_em × ratio/100 + spacing/100) × base_size × relSz/100)`,
//! `glyph_em` = 0.5 for space (half-width), else the resolved TTF hmtx.

use crate::resolver::FontResolver;

/// `CharShape` shape params for one char's script slot (percent units).
#[derive(Debug, Clone, Copy)]
pub struct CharMetrics {
    pub face_resolved: bool,
    pub ratio: u16,
    pub spacing: i16,
    pub rel_sz: u16,
    pub base_size: i32,
    pub bold: bool,
    /// Face is an HFT font: take the glyph em-width from the HFT width table
    /// rather than a substitute TTF's hmtx.
    pub is_hft: bool,
}

pub const SPACE_EM: f64 = 0.5;

/// Codepoints Hancom's glyph advance (`FUN_00082d98`) maps to glyph 0xffff, i.e.
/// zero advance: C0 controls (`<0x20`), DEL + C1 (`0x7f..=0x9f`), and ZWSP
/// (`0x200b`). Matches the RE'd `measure_string_advance` glyph-substitution rule.
fn is_zero_advance(ch: char) -> bool {
    let c = ch as u32;
    c < 0x20 || (0x7f..=0x9f).contains(&c) || c == 0x200b
}

/// Glyph em-width for `ch`: zero for Hancom's zero-advance set, a fixed half-em
/// for space, the HFT width-table value for an HFT-typed face, else the resolved
/// TTF's hmtx (glyph fallback when the per-script font lacks it).
pub fn glyph_em(resolver: &FontResolver, face: &str, ch: char, bold: bool, is_hft: bool) -> Option<f64> {
    // Hancom gives controls/DEL-C1/ZWSP no advance (RE: FUN_00082d98 → glyph 0xffff).
    if is_zero_advance(ch) {
        return Some(0.0);
    }
    // Space is a fixed half-em — Hancom computes it from a default rule, not the
    // font's own space glyph (raw hmtx/HFT space regressed science). Applies to
    // HFT faces too. See project_hwpx_advance_layout_re.
    if ch == ' ' {
        return Some(SPACE_EM);
    }
    // HFT-typed face: advance comes from the .HFT width table (advance ÷ em).
    // Falls back to TTF when no HFT font carries the glyph.
    if is_hft {
        if let Some(em) = resolver.hft_advance_em(face, ch) {
            return Some(em);
        }
    }
    resolver.resolve_glyph(face, ch, bold).and_then(|(f, _)| f.advance_em(ch))
}

pub fn advance_hwpunit(em: f64, m: &CharMetrics) -> i32 {
    let size = m.base_size as f64 * m.rel_sz as f64 / 100.0;
    let frac = em * (m.ratio as f64 / 100.0) + (m.spacing as f64 / 100.0);
    (frac * size).round() as i32
}

pub fn advance_of(resolver: &FontResolver, face: &str, ch: char, m: &CharMetrics) -> Option<i32> {
    Some(advance_hwpunit(glyph_em(resolver, face, ch, m.bold, m.is_hft)?, m))
}
