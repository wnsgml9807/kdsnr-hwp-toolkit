//! Probe: exhaustively dump TEJMJEN.HFT structure to locate any per-glyph
//! advance/width table the glyph blob (= ink bbox only) does not carry.

use kdsnr_hwp_hft::parser;

fn main() {
    let dir = std::env::var("HANCOM_HFT_DIR").unwrap_or_else(|_| {
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts".to_string()
    });
    let file = std::env::args().nth(1).unwrap_or_else(|| "TEJMJEN.HFT".into());
    let bytes = std::fs::read(format!("{dir}/{file}")).expect("read");
    println!("file={file} len={}", bytes.len());

    // Raw 0x200 header dump (first 64 bytes).
    print!("header[0..64]:");
    for (i, b) in bytes[..64].iter().enumerate() {
        if i % 16 == 0 { print!("\n  {:04x}: ", i); }
        print!("{:02x} ", b);
    }
    println!();

    let hft = parser::parse(&bytes).expect("parse");
    for (ci, chunk) in hft.chunks.iter().enumerate() {
        println!(
            "\nchunk[{ci}] off={:#x} size={} code={:#x} desc_count={}",
            chunk.offset, chunk.size, chunk.chunk_code, chunk.desc_count
        );
        for (di, d) in chunk.descriptors.iter().enumerate() {
            // Raw 22-byte descriptor header (expose the bytes the parser skips:
            // off+14,16 and off+20,21).
            let h = &bytes[d.offset..d.offset + 22];
            let w14 = u16::from_le_bytes([h[14], h[15]]);
            let w16 = u16::from_le_bytes([h[16], h[17]]);
            let w20 = u16::from_le_bytes([h[20], h[21]]);
            // Bytes consumed by the offset table + blobs vs record_size: any
            // surplus is candidate trailing metadata (a width table?).
            let off_table = d.count as usize * 4;
            // Last blob end: max(offset[i]) walked is complex; instead show the
            // glyph_data length and the offset-table span.
            println!(
                "  desc[{di}] off={:#x} rec_size={} type={} is_bitmap={} range={}..{} count={} em={} \
                 wh=({},{})  hdr14={} hdr16={} hdr20={}  glyph_data_len={} off_table={}  inner_hdr={:#x} inner_len={}",
                d.offset, d.record_size, d.type_id, d.is_bitmap, d.range_start, d.range_end,
                d.count, d.em, d.width, d.height, w14, w16, w20,
                d.glyph_data.len(), off_table, d.inner_header, d.inner_table.len()
            );
            // For a type=0 Latin descriptor: dump the offset table + per-glyph
            // blob (8-byte bbox + size). Check whether successive blob offsets
            // leave room only for bbox+path (no width), and whether glyph_data
            // has trailing bytes after the last blob.
            if d.type_id == 0 && d.inner_table.is_empty() && d.count > 0 && d.count < 200 {
                let gd = &d.glyph_data;
                let mut max_end = 0usize;
                for i in 0..d.count as usize {
                    let op = i * 4;
                    if op + 4 > gd.len() { break; }
                    let blob_off = u32::from_le_bytes([gd[op], gd[op+1], gd[op+2], gd[op+3]]) as usize;
                    if blob_off + 10 > gd.len() { continue; }
                    let size = u16::from_le_bytes([gd[blob_off+8], gd[blob_off+9]]) as usize;
                    let end = blob_off + 10 + size;
                    max_end = max_end.max(end);
                }
                println!("    [type0] glyph_data_len={} last_blob_end={} trailing={}",
                    gd.len(), max_end, gd.len().saturating_sub(max_end));
            }
        }
    }
}
