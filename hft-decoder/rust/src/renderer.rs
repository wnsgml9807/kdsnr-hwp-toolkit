//! End-to-end syllable renderer for HGMJ-style HFT fonts.

use crate::bitmap;
use crate::inner_table;
use crate::johab::{shape_class_invalid, unicode_to_johab, Johab};
use crate::parser::HftFile;

#[derive(Debug)]
pub enum RenderError {
    NotHangul,
    NoDescriptor,
    InnerTableTruncated,
    NoIndices,
}

/// Render a Hangul Syllables codepoint into a 2D pixel grid using the
/// HGMJ-style composition pipeline at the given em size tier (default 17).
pub fn render_syllable(
    hft: &HftFile,
    unicode_cp: u32,
    em: u16,
) -> Result<Vec<Vec<u8>>, RenderError> {
    let johab = unicode_to_johab(unicode_cp).ok_or(RenderError::NotHangul)?;
    let j = Johab::decompose(johab).ok_or(RenderError::NotHangul)?;
    let (cs, js, gs) = j.shape_classes();

    let desc = hft
        .find_descriptor(em, Some(2))
        .ok_or(RenderError::NoDescriptor)?;
    let inner =
        inner_table::parse(&desc.inner_table).map_err(|_| RenderError::InnerTableTruncated)?;

    let (cho_idx, jung_idx, jong_idx) = inner
        .bitmap_indices(cs, js, gs)
        .ok_or(RenderError::NoIndices)?;

    let mut base: Vec<Vec<u8>> = (0..desc.height).map(|_| vec![0u8; desc.width as usize]).collect();

    // Per FUN_100ac080 case 2: shape == 0 means skip that position.
    if cs != 0 && !shape_class_invalid(cs) && cho_idx < desc.count as u32 {
        if let Ok(b) = bitmap::extract(desc, cho_idx) {
            let pix = bitmap::to_pixels(b, desc.width, desc.height, desc.bytes_per_row);
            bitmap::or_blit(&mut base, &pix);
        }
    }
    if js != 0 && !shape_class_invalid(js) && jung_idx < desc.count as u32 {
        if let Ok(b) = bitmap::extract(desc, jung_idx) {
            let pix = bitmap::to_pixels(b, desc.width, desc.height, desc.bytes_per_row);
            bitmap::or_blit(&mut base, &pix);
        }
    }
    if gs != 0 && !shape_class_invalid(gs) && jong_idx < desc.count as u32 {
        if let Ok(b) = bitmap::extract(desc, jong_idx) {
            let pix = bitmap::to_pixels(b, desc.width, desc.height, desc.bytes_per_row);
            bitmap::or_blit(&mut base, &pix);
        }
    }
    Ok(base)
}
