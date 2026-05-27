//! Acceptance test for face→TTF resolution + the 자간/장평 advance formula,
//! mirroring `work/advance_fit.py`. Uses the local Hancom install; skips (does
//! not fail) when the fonts / hftinfo.dat are not present on this machine.

use std::path::Path;

use kdsnr_hwp_font::{advance_of, CharMetrics, FontResolver, Script};

fn font_dir() -> Option<std::path::PathBuf> {
    if let Ok(d) = std::env::var("HANCOM_FONT_DIR") {
        let p = std::path::PathBuf::from(d);
        if p.exists() {
            return Some(p);
        }
    }
    let mac = Path::new("/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF");
    mac.exists().then(|| mac.to_path_buf())
}

fn hftinfo() -> Option<std::path::PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../kdsnr-hwp-engine-inventory/hft/fonts/hftinfo.dat");
    p.exists().then_some(p)
}

fn resolver() -> Option<FontResolver> {
    let fd = font_dir()?;
    let hi = hftinfo()?;
    FontResolver::new(&fd, &hi).ok()
}

// charPr0 of korean.hwpx: 장평 95 / 자간 -5 / 11.5pt.
fn body_metrics() -> CharMetrics {
    CharMetrics { face_resolved: true, ratio: 95, spacing: -5, rel_sz: 100, base_size: 1150, bold: false, is_hft: false }
}

#[test]
fn hangul_advance_matches_gt() {
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    // 신명 중명조 (HFT-display) → substitutes to 한컴바탕 (Batang); '가' is full-em.
    let f = r.resolve("신명 중명조", '가').expect("resolve 신명 중명조 -> TTF");
    let em = f.advance_em('가').expect("가 advance");
    assert!((em - 1.0).abs() < 0.02, "Hangul em ~1.0, got {em}");
    // Frida GT dx for body 가 = 1032; formula must land within ~1%.
    let adv = advance_of(&r, "신명 중명조", '가', &body_metrics()).unwrap();
    assert!((1020..=1045).contains(&adv), "가 advance {adv} not ≈ GT 1032");
}

#[test]
fn space_is_half_width() {
    let Some(r) = resolver() else { return };
    // Space special rule: half-width (~0.5 em), independent of the font glyph.
    let adv = advance_of(&r, "신명 중명조", ' ', &body_metrics()).unwrap();
    // (0.5*0.95 - 0.05) * 1150 ≈ 489; GT natural space ≈ 505–516.
    assert!((460..=540).contains(&adv), "space advance {adv} not half-width-ish");
}

#[test]
fn latin_glyph_of_hangul_face_resolves() {
    let Some(r) = resolver() else { return };
    // 'A' takes the Latin script slot; 신명 태고딕 Latin → Arial (per hftinfo).
    let f = r.resolve("신명 태고딕", 'A');
    assert!(f.is_some(), "Latin 'A' should resolve via Latin substitution");
    let em = f.unwrap().advance_em('A').unwrap();
    assert!(em > 0.4 && em < 1.0, "'A' em in proportional range, got {em}");
}

#[test]
fn ttf_native_face_used_directly() {
    let Some(r) = resolver() else { return };
    // 함초롬바탕 is a real TTF family → used directly (no substitution).
    let f = r.resolve_face("함초롬바탕", Script::Hangul);
    assert!(f.is_some(), "함초롬바탕 should resolve as TTF-native");
}
