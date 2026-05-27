//! Probe: confirm the code-0x20 chunk is a per-glyph advance-width table across
//! Latin / Hangul / symbol HFT fonts, and pin down its header + indexing.

fn main() {
    let dir = std::env::var("HANCOM_HFT_DIR").unwrap_or_else(|_| {
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts".to_string()
    });
    for file in ["TEJMJEN.HFT", "ENSMJ.HFT", "TEJMJHG.HFT", "HGSMJ.HFT", "SPSMJ.HFT", "ENGMJ.HFT"] {
        let bytes = std::fs::read(format!("{dir}/{file}")).expect("read");
        println!("\n===== {file} len={} =====", bytes.len());
        // Walk chunks the same way the parser does (start 0x200, size-prefixed).
        let mut pos = 0x200usize;
        while pos + 4 <= bytes.len() {
            let sz = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            if sz == 0 || sz > 0x100_0000 || pos + sz > bytes.len() {
                break;
            }
            if sz >= 6 {
                let code = u16::from_le_bytes([bytes[pos + 4], bytes[pos + 5]]);
                let h6 = u16::from_le_bytes([bytes[pos + 6], bytes[pos + 7]]);
                let h8 = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
                print!("  chunk off={:#x} size={} code={:#06x} h6={} h8={}", pos, sz, code, h6, h8);
                if code == 0x20 {
                    // Width table: u16 values from +0xa to end.
                    let start = pos + 0xa;
                    let n = (pos + sz - start) / 2;
                    let w: Vec<u16> = (0..n)
                        .map(|i| u16::from_le_bytes([bytes[start + i * 2], bytes[start + i * 2 + 1]]))
                        .collect();
                    let head: Vec<u16> = w.iter().take(8).copied().collect();
                    let tail: Vec<u16> = w.iter().rev().take(4).rev().copied().collect();
                    println!("  -> WIDTH TABLE n={n} head={head:?} tail={tail:?}");
                } else {
                    println!();
                }
            }
            pos += sz;
        }
    }
}
