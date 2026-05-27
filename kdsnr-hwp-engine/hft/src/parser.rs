//! HFT file parser — chunk + descriptor extraction.

use std::convert::TryInto;

/// A single descriptor inside a chunk (on-disk 22-byte header + glyph section).
#[derive(Debug, Clone)]
pub struct Descriptor {
    pub offset: usize,
    pub record_size: u32,
    pub type_id: u8,     // low nibble of flags
    pub is_bitmap: bool, // flags bit4 (=0x10)
    pub range_start: u16,
    pub range_end: u16,
    pub count: u16,
    pub em: u16,
    pub width: u16,
    pub height: u16,
    pub bytes_per_row: u16,
    pub stride: u32, // bytes per glyph (for fixed-stride bitmap data)
    pub inner_table: Vec<u8>,
    pub inner_header: u32,
    pub glyph_data: Vec<u8>,
}

/// A chunk in the HFT body (14-byte header + descriptors).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub offset: usize,
    pub size: u32,
    pub chunk_code: u16,
    pub desc_count: u16,
    pub descriptors: Vec<Descriptor>,
}

/// A per-glyph advance-width table. Stored in a dedicated chunk whose
/// `chunk_code` is the table's first char code (`range_start`) and whose word
/// at +6 is the last char code (`range_end`); the `u16` body at +0xa holds one
/// advance per code in `range_start..=range_end`, in the font's em units.
///
/// The outline blob carries only an ink bounding box, so this is the sole
/// source of the proportional Latin/punctuation advance. Hangul/symbol fonts
/// omit it (those glyphs advance a full em).
#[derive(Debug, Clone)]
pub struct WidthTable {
    pub range_start: u16,
    pub range_end: u16,
    /// One advance per code `range_start..=range_end`, em-units.
    pub widths: Vec<u16>,
}

impl WidthTable {
    /// Advance (em-units) for `char_code`, if covered by this table.
    pub fn advance(&self, char_code: u32) -> Option<u16> {
        if char_code < self.range_start as u32 || char_code > self.range_end as u32 {
            return None;
        }
        self.widths.get((char_code - self.range_start as u32) as usize).copied()
    }
}

/// Parsed HFT file: header + chunk linked list + advance-width tables.
#[derive(Debug, Clone)]
pub struct HftFile {
    pub raw_len: usize,
    pub chunks: Vec<Chunk>,
    pub width_tables: Vec<WidthTable>,
}

impl HftFile {
    pub fn find_descriptor(&self, em: u16, type_filter: Option<u8>) -> Option<&Descriptor> {
        for ch in &self.chunks {
            for d in &ch.descriptors {
                if d.em == em && type_filter.map_or(true, |t| d.type_id == t) {
                    return Some(d);
                }
            }
        }
        None
    }

    /// Advance (em-units) for `char_code` from any width table, or `None` when
    /// no table covers it (caller advances a full em).
    pub fn advance_width(&self, char_code: u32) -> Option<u16> {
        self.width_tables.iter().find_map(|t| t.advance(char_code))
    }
}

#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Truncated,
    InvalidMagic,
}

impl From<std::io::Error> for ParseError {
    fn from(e: std::io::Error) -> Self {
        ParseError::Io(e)
    }
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

fn read_i32(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

/// Parse an HFT file's bytes into chunks + descriptors.
pub fn parse(data: &[u8]) -> Result<HftFile, ParseError> {
    if data.len() < 0x200 {
        return Err(ParseError::Truncated);
    }

    let mut hft = HftFile {
        raw_len: data.len(),
        chunks: Vec::new(),
        width_tables: Vec::new(),
    };

    let mut pos = 0x200usize;
    while pos + 4 <= data.len() {
        let sz = read_u32(data, pos) as usize;
        if sz == 0 || sz > 0x100_0000 || pos + sz > data.len() {
            break;
        }
        if sz >= 14 {
            if let Some(wt) = parse_width_table(data, pos, sz) {
                hft.width_tables.push(wt);
            } else {
                hft.chunks.push(parse_chunk(data, pos, sz as u32)?);
            }
        }
        pos += sz;
    }

    Ok(hft)
}

/// Recognize an advance-width-table chunk and parse it, else `None`.
///
/// Layout: `chunk_code`(+4) = `range_start`, word(+6) = `range_end`, `u16` body
/// from +0xa. A chunk is a width table iff its char-code range exactly fills the
/// body: `range_start < range_end` and `(size-0xa)/2 == range_end-range_start+1`.
/// This cleanly excludes outline chunks (whose `chunk_code` is the em, e.g. 1000,
/// with word(+6)=1) and the 0x8000/0x3400 marker chunks (size 12, count 1).
fn parse_width_table(data: &[u8], off: usize, size: usize) -> Option<WidthTable> {
    if off + 0xa > data.len() || size < 0xc {
        return None;
    }
    let range_start = read_u16(data, off + 4);
    let range_end = read_u16(data, off + 6);
    if range_start >= range_end {
        return None;
    }
    let n = (size - 0xa) / 2;
    if n != (range_end - range_start + 1) as usize {
        return None;
    }
    let body = off + 0xa;
    if body + n * 2 > data.len() {
        return None;
    }
    let widths = (0..n).map(|i| read_u16(data, body + i * 2)).collect();
    Some(WidthTable { range_start, range_end, widths })
}

fn parse_chunk(data: &[u8], chunk_off: usize, chunk_size: u32) -> Result<Chunk, ParseError> {
    let chunk_code = read_u16(data, chunk_off + 4);
    let desc_count = read_u16(data, chunk_off + 8);
    let local_e = read_i32(data, chunk_off + 10);

    let mut chunk = Chunk {
        offset: chunk_off,
        size: chunk_size,
        chunk_code,
        desc_count,
        descriptors: Vec::with_capacity(desc_count as usize),
    };

    let chunk_end = chunk_off + chunk_size as usize;
    let mut cur = chunk_off as isize + local_e as isize;

    for _ in 0..desc_count {
        if cur < 0 || (cur as usize) + 22 > chunk_end.min(data.len()) {
            break;
        }
        let desc = parse_descriptor(data, cur as usize)?;
        let next = cur as usize + desc.record_size as usize;
        chunk.descriptors.push(desc);
        cur = next as isize;
    }

    Ok(chunk)
}

fn parse_descriptor(data: &[u8], off: usize) -> Result<Descriptor, ParseError> {
    let rec_sz = read_u32(data, off);
    let flags = read_u16(data, off + 4);
    let rs = read_u16(data, off + 6);
    let re = read_u16(data, off + 8);
    let cnt = read_u16(data, off + 10);
    let em = read_u16(data, off + 12);
    let int_at_18 = read_i32(data, off + 18);
    let type_id = (flags & 0xf) as u8;
    let is_bitmap = (flags & 0x10) != 0;
    let width = (int_at_18 & 0xFFFF) as u16;
    let height = ((int_at_18 >> 16) & 0xFFFF) as u16;

    let after_hdr = off + 22;
    let mut inner_size = 0u16;
    let mut inner_header = 0u32;
    let mut inner_body: Vec<u8> = Vec::new();

    if matches!(type_id, 1 | 2 | 4) && after_hdr + 4 <= data.len() {
        inner_header = read_u32(data, after_hdr);
        inner_size = (inner_header & 0xFFFF) as u16;
        if inner_size > 4 && after_hdr + inner_size as usize <= data.len() {
            inner_body = data[after_hdr + 4..after_hdr + inner_size as usize].to_vec();
        }
    }

    // record_size 가 파일 끝을 넘어가는 손상된 descriptor 가 있다 (Hancom
    // HGBT.HFT 등). slice 패닉을 막기 위해 record_end / glyph_section_off /
    // glyph_section_end 를 모두 data.len() 로 clamp 한 뒤 안전 범위에서만 잘라낸다.
    let rec_end = (off.saturating_add(rec_sz as usize)).min(data.len());
    let glyph_section_off_raw = if matches!(type_id, 1 | 2 | 4) {
        after_hdr.saturating_add(inner_size as usize)
    } else {
        after_hdr
    };
    let glyph_section_off = glyph_section_off_raw.min(data.len());
    let glyph_data = if glyph_section_off < rec_end {
        data[glyph_section_off..rec_end].to_vec()
    } else {
        Vec::new()
    };

    let bytes_per_row = if width > 0 { (width + 7) / 8 } else { 0 };
    let stride = if is_bitmap {
        (bytes_per_row as u32) * (height as u32)
    } else {
        0
    };

    Ok(Descriptor {
        offset: off,
        record_size: rec_sz,
        type_id,
        is_bitmap,
        range_start: rs,
        range_end: re,
        count: cnt,
        em,
        width,
        height,
        bytes_per_row,
        stride,
        inner_table: inner_body,
        inner_header,
        glyph_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn hft_dir() -> Option<PathBuf> {
        let dir = std::env::var("HANCOM_HFT_DIR").map(PathBuf::from).unwrap_or_else(|_| {
            PathBuf::from(
                "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts",
            )
        });
        dir.exists().then_some(dir)
    }

    #[test]
    fn parses_latin_width_table() {
        let Some(dir) = hft_dir() else { return };
        let bytes = std::fs::read(dir.join("TEJMJEN.HFT")).unwrap();
        let hft = parse(&bytes).unwrap();
        // ASCII width table: range 32..=133, 102 entries.
        let wt = hft
            .width_tables
            .iter()
            .find(|t| t.range_start == 32)
            .expect("ASCII width table");
        assert_eq!(wt.range_end, 133);
        assert_eq!(wt.widths.len(), 102);
        // Indexed by char code from range_start=32: first two entries are the
        // raw bytes 0x190 and 0x1f4 (space=400, '!'=500).
        assert_eq!(hft.advance_width(32), Some(400));
        assert_eq!(hft.advance_width('!' as u32), Some(500));
        // Proportional: wide 'W' advances more than narrow 'i'.
        assert!(hft.advance_width('W' as u32) > hft.advance_width('i' as u32));
        // Out-of-range (Hangul) → no width-table entry.
        assert_eq!(hft.advance_width('가' as u32), None);
    }

    #[test]
    fn hangul_font_has_no_width_table() {
        let Some(dir) = hft_dir() else { return };
        let bytes = std::fs::read(dir.join("HGSMJ.HFT")).unwrap();
        let hft = parse(&bytes).unwrap();
        assert!(hft.width_tables.is_empty(), "Hangul font should carry no width table");
    }
}
