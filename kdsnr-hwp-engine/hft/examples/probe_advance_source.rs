//! Probe: settle the HFT 4-word glyph-header semantics by comparing the words
//! against the exact ink bbox decoded from the path commands. Determines whether
//! word[2] is the advance width or merely the ink width.

use kdsnr_hwp_hft::vector::CommandKind;
use kdsnr_hwp_hft::HftCache;

fn main() {
    let dir = std::env::var("HANCOM_HFT_DIR").unwrap_or_else(|_| {
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts".to_string()
    });
    let mut cache = HftCache::new();
    cache.load_hft(format!("{dir}/TEJMJEN.HFT")).expect("load TEJMJEN");

    println!("char\tem\tw0(lsb?)\tw2(adv?)\tw0+w2\tink_min_x\tink_max_x\tink_w");
    for ch in "A W a g . , ( ) 0 1 i m".split_whitespace() {
        let code = ch.chars().next().unwrap() as u32;
        let Some(g) = cache.get("TEJMJEN", code) else {
            println!("{ch}\t<none>");
            continue;
        };
        let (mut min_x, mut max_x) = (i32::MAX, i32::MIN);
        for c in &g.commands {
            let xs: &[i32] = match c.kind {
                CommandKind::Move | CommandKind::Line => &c.points[0..1],
                CommandKind::Cubic => &c.points[..], // x at 0,2,4 — scan all, harmless
                CommandKind::Close => &[],
            };
            for (i, v) in xs.iter().enumerate() {
                if c.kind == CommandKind::Cubic && i % 2 != 0 {
                    continue; // skip y
                }
                min_x = min_x.min(*v);
                max_x = max_x.max(*v);
            }
        }
        let m = g.metrics.raw;
        println!(
            "{ch}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            g.em, m[0], m[2], m[0] as i32 + m[2] as i32, min_x, max_x, max_x - min_x
        );
    }
}
