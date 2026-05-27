//! Vector path opcode parser for type 1 HFT descriptors (HCHGGGT-style).

use crate::cipher;
use crate::parser::Descriptor;
use std::convert::TryInto;

#[derive(Debug, Clone)]
pub struct PathBlob {
    pub metrics: (i16, i16, i16, i16),
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Move,
    Line,
    Cubic,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathCommand {
    pub kind: CommandKind,
    /// Absolute coords. Order:
    ///   Move/Line: [x, y]
    ///   Cubic:     [cx1, cy1, cx2, cy2, ex, ey]
    ///   Close:     []
    pub points: Vec<i32>,
}

#[derive(Debug)]
pub enum VectorError {
    NotType1,
    OutOfRange,
    Truncated,
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

fn read_i16(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes(buf[off..off + 2].try_into().unwrap())
}

/// Binary search the type 1 inner table for `char_code`. Returns the bitmap index.
pub fn lookup_type1(desc: &Descriptor, char_code: u16) -> Option<u32> {
    if desc.type_id != 1 {
        return None;
    }
    let inner = &desc.inner_table;
    let n = desc.count as usize;
    if inner.len() < n * 2 {
        return None;
    }
    let mut lo: isize = 0;
    let mut hi: isize = n as isize - 1;
    while lo <= hi {
        let mid = ((lo + hi) >> 1) as usize;
        let v = read_u16(inner, mid * 2);
        if v < char_code {
            lo = mid as isize + 1;
        } else if v > char_code {
            hi = mid as isize - 1;
        } else {
            return Some(mid as u32);
        }
    }
    None
}

/// Extract the path blob for the given glyph index. If `cipher` is `Some`,
/// the path bytes are decrypted before return.
pub fn extract_blob(
    desc: &Descriptor,
    idx: u32,
    cipher: Option<cipher::CipherKey>,
) -> Result<PathBlob, VectorError> {
    if idx >= desc.count as u32 {
        return Err(VectorError::OutOfRange);
    }
    let data = &desc.glyph_data;
    let off_table_pos = (idx as usize) * 4;
    if off_table_pos + 4 > data.len() {
        return Err(VectorError::Truncated);
    }
    let blob_off = read_u32(data, off_table_pos) as usize;

    let (metrics, body_off) = if !desc.is_bitmap {
        // bit4=0: 8-byte metrics in blob, then size
        if blob_off + 10 > data.len() {
            return Err(VectorError::Truncated);
        }
        let m = (
            read_i16(data, blob_off),
            read_i16(data, blob_off + 2),
            read_i16(data, blob_off + 4),
            read_i16(data, blob_off + 6),
        );
        let body_off = blob_off + 10;
        (m, body_off)
    } else {
        // bit4=1: metrics from descriptor, blob starts with size
        if blob_off + 2 > data.len() {
            return Err(VectorError::Truncated);
        }
        let m = (desc.width as i16, desc.height as i16, 0, 0);
        let body_off = blob_off + 2;
        (m, body_off)
    };
    let size = read_u16(data, body_off - 2) as usize;
    if body_off + size > data.len() {
        return Err(VectorError::Truncated);
    }
    let mut raw = data[body_off..body_off + size].to_vec();
    if let Some(key) = cipher {
        raw = cipher::decrypt(&raw, key);
    }
    Ok(PathBlob { metrics, raw })
}

/// Extract a vector blob and walk it with the descriptor's known Hancom
/// encoding variants. Some type=0 families (ENSMJ/HJSMJ) use the HJSMJ stream
/// cipher while sibling families (TEJMJEN, TETGTEN, ...) store raw path data.
pub fn extract_decoded_path(desc: &Descriptor, idx: u32) -> Option<(PathBlob, Vec<PathCommand>)> {
    let mut candidates = [None, None, None];
    let len = if desc.type_id == 1 {
        // Most HCH/TE type=1 families are raw, while Hanyang HG* siblings use
        // the same HJSMJ stream observed in type=0 vectors.
        candidates[0] = None;
        candidates[1] = Some(cipher::HJSMJ);
        2
    } else if let Some(key) = cipher::for_type(desc.type_id) {
        candidates[0] = Some(key);
        candidates[1] = None;
        2
    } else {
        candidates[0] = None;
        1
    };

    for cipher_key in candidates.into_iter().take(len) {
        let blob = match extract_blob(desc, idx, cipher_key) {
            Ok(blob) => blob,
            Err(_) => continue,
        };
        let cmds = walk_path(&blob.raw);
        if has_visible_segments(&cmds) {
            return Some((blob, cmds));
        }
    }
    None
}

fn has_visible_segments(commands: &[PathCommand]) -> bool {
    commands
        .iter()
        .any(|command| matches!(command.kind, CommandKind::Line | CommandKind::Cubic))
}

/// Walk a raw path opcode stream and emit a command list matching the
/// painter's marker/coord stream.
///
/// Verified against macOS native painter `FUN_000296ac` in libHncBaseDraw.dylib
/// (raid 22 of HFT RE). Key invariants:
///
/// - Per macOS painter line 53-54: `iVar8 = (int)param_2; iVar23 = (int)uVar25;`
///   are reset at the start of **every** iteration. So all conditional varint
///   reads (in cases 1/2/3, 5/6/7, 9/A/B) default to `initial_x/y` (typically 0),
///   NOT to the previous opcode's value.
///
/// - Marker semantics (libhsp `FUN_00091c30`):
///     1 = MoveTo            (consumed as `CGContextMoveToPoint`)
///     2 = LineTo            (consumed as `CGContextAddLineToPoint`)
///     3 = CurveTo (3-in-row, coords[i]=c1, [i+1]=c2, [i+2]=end)
///     4 = ClosePath
///
/// - Cubic varint count per opcode (macos_painter.txt line 250-441):
///     0x09: dx0 + local_24 + local_28 + local_18       (4 varints)
///     0x0A: dy0 + local_24 + local_28 + local_1c       (4 varints)
///     0x0B: dx0 + dy0 + local_24 + local_28 + local_1c + local_18 (6 varints)
///
/// - Cubic coordinates (chained deltas):
///     c1  = (x + dx0,         y + dy0)
///     c2  = (c1.x + local_24, c1.y + local_28)
///     end = (c2.x + local_1c, c2.y + local_18)
///
/// - `case 0/4` (Close) is emitted only when the previous emit was Line/Cubic
///   (`needs_close=true`). On Move (1/2/3) entry an implicit Close is emitted
///   first if `needs_close` is set. A final implicit Close is emitted if the
///   path ends while `needs_close` is still set.
pub fn walk_path(raw: &[u8]) -> Vec<PathCommand> {
    let mut pos = 0usize;
    let mut x: i32 = 0;
    let mut y: i32 = 0;
    // Subpath start (= last Move's absolute coord). Close resets `x, y` to this,
    // matching macOS painter FUN_000296ac line 73:
    //   _DAT_000f04a8 = CONCAT44((int)uVar24, (int)uVar18);
    // where (uVar18, uVar24) are updated only at the end of case 1/2/3 (Move).
    // Without this, multi-subpath glyphs (e.g. ㅁ / ㅂ 받침 = outer + inner hole)
    // anchor their second subpath at the previous subpath's end point.
    let mut subpath_start_x: i32 = 0;
    let mut subpath_start_y: i32 = 0;
    let mut cmds = Vec::new();
    let mut needs_close = false;

    /// Try to read a varint; returns None if the buffer is exhausted.
    fn try_varint(raw: &[u8], pos: usize) -> Option<(i32, usize)> {
        if pos >= raw.len() {
            return None;
        }
        let b = raw[pos] as i8 as i32;
        if (-123..=123).contains(&b) {
            return Some((b, pos + 1));
        }
        if b >= 124 {
            if pos + 1 >= raw.len() {
                return None;
            }
            return Some((b * 256 + raw[pos + 1] as i32 - 0x7b84, pos + 2));
        }
        if (-127..=-124).contains(&b) {
            if pos + 1 >= raw.len() {
                return None;
            }
            return Some((b * 256 - raw[pos + 1] as i32 + 0x7b84, pos + 2));
        }
        // b == -128: i16 LE follows
        if pos + 2 >= raw.len() {
            return None;
        }
        let lo = raw[pos + 1] as i32;
        let hi = raw[pos + 2] as i8 as i32;
        Some(((hi << 8) | lo, pos + 3))
    }

    while pos < raw.len() {
        let op = raw[pos];
        pos += 1;
        match op {
            0x00 | 0x04 => {
                if needs_close {
                    cmds.push(PathCommand {
                        kind: CommandKind::Close,
                        points: vec![],
                    });
                    needs_close = false;
                    x = subpath_start_x;
                    y = subpath_start_y;
                }
                if op == 0x00 {
                    break;
                }
            }
            0x01 | 0x02 | 0x03 => {
                // Move — implicit Close first if previous was Line/Cubic
                if needs_close {
                    cmds.push(PathCommand {
                        kind: CommandKind::Close,
                        points: vec![],
                    });
                    needs_close = false;
                    x = subpath_start_x;
                    y = subpath_start_y;
                }
                let mut dx = 0i32;
                let mut dy = 0i32;
                if op & 1 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dx = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                if op & 2 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dy = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                x += dx;
                y += dy;
                subpath_start_x = x;
                subpath_start_y = y;
                cmds.push(PathCommand {
                    kind: CommandKind::Move,
                    points: vec![x, y],
                });
            }
            0x05 | 0x06 | 0x07 => {
                let mut dx = 0i32;
                let mut dy = 0i32;
                if op & 1 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dx = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                if op & 2 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dy = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                x += dx;
                y += dy;
                cmds.push(PathCommand {
                    kind: CommandKind::Line,
                    points: vec![x, y],
                });
                needs_close = true;
            }
            0x09 | 0x0A | 0x0B => {
                let mut dx0 = 0i32;
                let mut dy0 = 0i32;
                if op & 1 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dx0 = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                if op & 2 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            dy0 = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                let d24;
                let d28;
                match try_varint(raw, pos) {
                    Some((v, p)) => {
                        d24 = v;
                        pos = p;
                    }
                    None => break,
                }
                match try_varint(raw, pos) {
                    Some((v, p)) => {
                        d28 = v;
                        pos = p;
                    }
                    None => break,
                }
                let mut d1c = 0i32;
                let mut d18 = 0i32;
                if op & 2 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            d1c = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                if op & 1 != 0 {
                    match try_varint(raw, pos) {
                        Some((v, p)) => {
                            d18 = v;
                            pos = p;
                        }
                        None => break,
                    }
                }
                let cx1 = x + dx0;
                let cy1 = y + dy0;
                let cx2 = cx1 + d24;
                let cy2 = cy1 + d28;
                let ex = cx2 + d1c;
                let ey = cy2 + d18;
                cmds.push(PathCommand {
                    kind: CommandKind::Cubic,
                    points: vec![cx1, cy1, cx2, cy2, ex, ey],
                });
                x = ex;
                y = ey;
                needs_close = true;
            }
            0x40 | 0x42 => {
                for _ in 0..2 {
                    match try_varint(raw, pos) {
                        Some((_, p)) => {
                            pos = p;
                        }
                        None => break,
                    }
                }
            }
            0x41 | 0x43 => {
                for _ in 0..5 {
                    match try_varint(raw, pos) {
                        Some((_, p)) => {
                            pos = p;
                        }
                        None => break,
                    }
                }
            }
            0x44 => {}
            0x22 | 0x23 => {
                pos += 1;
            }
            0x20 => {
                if pos >= raw.len() {
                    break;
                }
                let n = raw[pos] as usize;
                pos += 1 + 2 * n;
                if pos + 4 > raw.len() {
                    break;
                }
                let extra = raw[pos + 3] as usize;
                pos += 4 + 2 * extra;
            }
            0x21 => {
                if pos >= raw.len() {
                    break;
                }
                let n = raw[pos] as usize;
                pos += 1;
                for _ in 0..n {
                    if pos >= raw.len() {
                        break;
                    }
                    let ln = raw[pos] as usize;
                    pos += 1 + ln;
                }
            }
            _ => break,
        }
    }
    if needs_close {
        cmds.push(PathCommand {
            kind: CommandKind::Close,
            points: vec![],
        });
    }
    cmds
}

/// Render a command list as an SVG path `d` attribute string.
///
/// This mirrors libhsp.dylib `FUN_00091c30` (raid 20). All outlines are
/// Move + Line + Cubic + Close only.
pub fn to_svg_path(commands: &[PathCommand]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in commands {
        match c.kind {
            CommandKind::Move => parts.push(format!("M{},{}", c.points[0], c.points[1])),
            CommandKind::Line => parts.push(format!("L{},{}", c.points[0], c.points[1])),
            CommandKind::Cubic => parts.push(format!(
                "C{},{} {},{} {},{}",
                c.points[0], c.points[1], c.points[2], c.points[3], c.points[4], c.points[5]
            )),
            CommandKind::Close => parts.push("Z".to_string()),
        }
    }
    parts.join("")
}
