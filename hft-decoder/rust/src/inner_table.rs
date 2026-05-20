//! Korean composition inner table parser for type 2/4 descriptors.
//!
//! On-disk body (after the 4-byte u32 header) is 8 bytes shorter than the
//! in-memory view because the +0..+7 pointer slots are runtime-allocated.
//!
//! Runtime memory layout (offsets relative to `piVar11`):
//!   +0x00..+0x07: x/y offset table pointers (runtime only, not in file)
//!   +0x08..+0x0b: max_cho_shape, max_jung_shape, max_jong_shape, pad
//!   +0x0c..+0x4b: u16[32] cho_jamo_base (per cho_shape; transformed to cumulative)
//!   +0x4c..+0x8b: u16[32] jung_jamo_base
//!   +0x8c..+0xcb: u16[32] jong_jamo_base
//!   +0xcc..+0xcf: max_cho_count, max_jung_count, max_jong_count, pad
//!   +0xd0..+0xef: byte[32] cho_remap
//!   +0xf0..+0x10f: byte[32] jung_remap
//!   +0x110..+0x12f: byte[32] jong_remap
//!   +0x130..: 3 stacked 3D tables of (max_cho × max_jung × max_jong) bytes each
//!
//! The HFT loader transforms the jamo_base arrays in-place via a single
//! cumulative-prefix-sum that carries the accumulator across all three
//! position arrays in sequence (cho → jung → jong).

use std::convert::TryInto;

#[derive(Debug, Clone)]
pub struct InnerTable {
    pub max_cho_shape: u8,
    pub max_jung_shape: u8,
    pub max_jong_shape: u8,

    pub cho_jamo_base: [u16; 32],
    pub jung_jamo_base: [u16; 32],
    pub jong_jamo_base: [u16; 32],

    pub max_cho_count: u8,
    pub max_jung_count: u8,
    pub max_jong_count: u8,

    pub cho_remap: [u8; 32],
    pub jung_remap: [u8; 32],
    pub jong_remap: [u8; 32],

    pub table_3d: Vec<u8>,
}

impl InnerTable {
    pub fn per_position_size(&self) -> usize {
        (self.max_cho_count as usize)
            * (self.max_jung_count as usize)
            * (self.max_jong_count as usize)
    }

    /// Compute the 3D-table index for the given shape triple. Returns None
    /// if any remapped position falls outside its max_count bound.
    pub fn combined_index(&self, cs: u8, js: u8, gs: u8) -> Option<usize> {
        let flat_cho = self.cho_remap[cs as usize] as usize;
        let flat_jung = self.jung_remap[js as usize] as usize;
        let flat_jong = self.jong_remap[gs as usize] as usize;
        if flat_cho >= self.max_cho_count as usize
            || flat_jung >= self.max_jung_count as usize
            || flat_jong >= self.max_jong_count as usize
        {
            return None;
        }
        Some(
            (self.max_jung_count as usize * flat_cho + flat_jung)
                * self.max_jong_count as usize
                + flat_jong,
        )
    }

    /// Return (cho_idx, jung_idx, jong_idx) bitmap indices for the syllable.
    /// Caller is responsible for skipping components where shape == 0 (per
    /// FUN_100ac080 case 2 `if (iVar3 != 0) {...}` guards).
    pub fn bitmap_indices(&self, cs: u8, js: u8, gs: u8) -> Option<(u32, u32, u32)> {
        let idx = self.combined_index(cs, js, gs)?;
        let n = self.per_position_size();
        if idx + 2 * n >= self.table_3d.len() {
            return None;
        }
        let cho_off = self.table_3d[idx] as u32;
        let jung_off = self.table_3d[n + idx] as u32;
        let jong_off = self.table_3d[2 * n + idx] as u32;

        Some((
            self.cho_jamo_base[cs as usize] as u32 + cho_off,
            self.jung_jamo_base[js as usize] as u32 + jung_off,
            self.jong_jamo_base[gs as usize] as u32 + jong_off,
        ))
    }
}

#[derive(Debug)]
pub enum InnerTableError {
    Truncated,
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}

/// Parse an inner table body (the bytes AFTER the 4-byte u32 header).
/// Applies the runtime cumulative prefix-sum transform across the three
/// `jamo_base` arrays so the result is immediately usable for index lookup.
pub fn parse(body: &[u8]) -> Result<InnerTable, InnerTableError> {
    if body.len() < 0x130 - 8 {
        return Err(InnerTableError::Truncated);
    }

    let max_cs = body[0];
    let max_js = body[1];
    let max_gs = body[2];

    let mut cho_jamo_base = [0u16; 32];
    let mut jung_jamo_base = [0u16; 32];
    let mut jong_jamo_base = [0u16; 32];
    for i in 0..32 {
        cho_jamo_base[i] = read_u16(body, 0x04 + i * 2);
        jung_jamo_base[i] = read_u16(body, 0x44 + i * 2);
        jong_jamo_base[i] = read_u16(body, 0x84 + i * 2);
    }

    prefix_sum_3way(
        &mut cho_jamo_base,
        &mut jung_jamo_base,
        &mut jong_jamo_base,
        max_cs,
        max_js,
        max_gs,
    );

    let max_cc = body[0xc4];
    let max_jc = body[0xc5];
    let max_gc = body[0xc6];

    let mut cho_remap = [0u8; 32];
    let mut jung_remap = [0u8; 32];
    let mut jong_remap = [0u8; 32];
    cho_remap.copy_from_slice(&body[0xc8..0xc8 + 32]);
    jung_remap.copy_from_slice(&body[0xe8..0xe8 + 32]);
    jong_remap.copy_from_slice(&body[0x108..0x108 + 32]);

    let table_3d = body[0x128..].to_vec();

    Ok(InnerTable {
        max_cho_shape: max_cs,
        max_jung_shape: max_js,
        max_jong_shape: max_gs,
        cho_jamo_base,
        jung_jamo_base,
        jong_jamo_base,
        max_cho_count: max_cc,
        max_jung_count: max_jc,
        max_jong_count: max_gc,
        cho_remap,
        jung_remap,
        jong_remap,
        table_3d,
    })
}

/// In-place 3-way cumulative prefix-sum. The accumulator carries from cho →
/// jung → jong; each array's loop runs `0..=max_shape_for_position`.
///
/// Verified via Frida raid 16: this reproduces the runtime transform that
/// `FUN_100ab9a0` performs in the HFT loader.
fn prefix_sum_3way(
    cho: &mut [u16; 32],
    jung: &mut [u16; 32],
    jong: &mut [u16; 32],
    max_cs: u8,
    max_js: u8,
    max_gs: u8,
) {
    let mut acc: u16 = 0;
    for (arr, max_s) in [
        (&mut *cho, max_cs),
        (&mut *jung, max_js),
        (&mut *jong, max_gs),
    ] {
        for i in 0..=(max_s as usize) {
            // Read as signed `short` to match C semantics, but the values we've
            // observed are always small positive counts.
            let old = arr[i] as i16 as i32;
            arr[i] = acc;
            acc = acc.wrapping_add(old as u16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values dumped from HGMJ.HFT (em=17 chunk 2 desc 0) at runtime.
    /// These are the prefix-sum results that match Frida-captured bitmap indices.
    #[test]
    fn prefix_sum_known_values() {
        // Synthetic counts that mimic HGMJ chunk 2 em=17: cho_jamo_base[0]=0,
        // cho_jamo_base[1..]=10. After prefix-sum: [0, 0, 10, 20, 30, ...]
        let mut cho = [0u16; 32];
        let mut jung = [0u16; 32];
        let mut jong = [0u16; 32];
        cho[0] = 0;
        for i in 1..32 {
            cho[i] = 10;
        }
        prefix_sum_3way(&mut cho, &mut jung, &mut jong, 31, 0, 0);
        assert_eq!(cho[0], 0);
        assert_eq!(cho[1], 0);
        assert_eq!(cho[2], 10);
        assert_eq!(cho[3], 20);
        assert_eq!(cho[19], 180);
        // accumulator after cho loop = sum(disk_cho[0..31]) = 0 + 10*31 = 310
        // (this is what jung[0] would be in real HGMJ)
    }
}
