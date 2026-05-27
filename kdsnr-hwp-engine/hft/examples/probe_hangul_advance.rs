//! Probe: is the HFT advance uniformly a full em for Hangul syllables? A drift
//! here would corrupt the dominant body chars in the advance metric.

use kdsnr_hwp_hft::HftCache;

fn main() {
    let dir = std::env::var("HANCOM_HFT_DIR").unwrap_or_else(|_| {
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts".to_string()
    });
    let mut cache = HftCache::new();
    cache.load_dir(&dir).expect("load");
    cache.add_hancom_fontmap_aliases();

    for face in ["신명 중명조", "한양신명조", "신명 중고딕", "신명 세고딕", "신명 신그래픽"] {
        let mut n = 0;
        let mut full = 0;
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        let mut examples = Vec::new();
        for cp in 0xAC00u32..=0xD7A3 {
            if let Some(g) = cache.get(face, cp) {
                let r = g.advance as f64 / g.em as f64;
                n += 1;
                if (r - 1.0).abs() < 1e-9 { full += 1; }
                else if examples.len() < 8 {
                    examples.push((char::from_u32(cp).unwrap(), g.advance, g.em, r));
                }
                min = min.min(r);
                max = max.max(r);
            }
        }
        println!("{face}: syllables={n} full_em={full} min={min:.4} max={max:.4}");
        for (c, a, e, r) in &examples {
            println!("   non-full: {c} adv={a} em={e} ratio={r:.4}");
        }
    }
}
