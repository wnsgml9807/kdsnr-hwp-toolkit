//! Bitmap glyph extraction + rendering for HGMJ-style HFT fonts.

use crate::parser::Descriptor;

#[derive(Debug)]
pub enum BitmapError {
    NotBitmap,
    OutOfRange,
    Truncated,
}

/// Extract raw bitmap bytes for a glyph at the given index.
pub fn extract(desc: &Descriptor, idx: u32) -> Result<&[u8], BitmapError> {
    if !desc.is_bitmap {
        return Err(BitmapError::NotBitmap);
    }
    if idx >= desc.count as u32 {
        return Err(BitmapError::OutOfRange);
    }
    let stride = desc.stride as usize;
    let start = (idx as usize) * stride;
    if start
        .checked_add(stride)
        .is_none_or(|end| end > desc.glyph_data.len())
    {
        return Err(BitmapError::Truncated);
    }
    Ok(&desc.glyph_data[start..start + stride])
}

/// Convert raw bitmap bytes to a 2D Vec<Vec<u8>> of 0/1 pixels (MSB-first row).
pub fn to_pixels(data: &[u8], width: u16, height: u16, bytes_per_row: u16) -> Vec<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let bpr = bytes_per_row as usize;
    let mut rows = Vec::with_capacity(h);
    for r in 0..h {
        let mut row = Vec::with_capacity(w);
        for c in 0..w {
            let byte_idx = r * bpr + c / 8;
            let bit_pos = 7 - (c % 8);
            let bit = if byte_idx < data.len() && (data[byte_idx] & (1 << bit_pos)) != 0 {
                1
            } else {
                0
            };
            row.push(bit);
        }
        rows.push(row);
    }
    rows
}

/// OR-blit `overlay` onto `base`. Both pixel grids must be the same shape.
pub fn or_blit(base: &mut Vec<Vec<u8>>, overlay: &[Vec<u8>]) {
    for r in 0..base.len().min(overlay.len()) {
        for c in 0..base[r].len().min(overlay[r].len()) {
            base[r][c] |= overlay[r][c];
        }
    }
}

/// Render a 2D pixel grid as ASCII (block for "on", space for "off").
pub fn to_ascii(pixels: &[Vec<u8>], on: char, off: char) -> String {
    let mut out = String::new();
    for row in pixels {
        for &v in row {
            out.push(if v != 0 { on } else { off });
        }
        out.push('\n');
    }
    out
}
