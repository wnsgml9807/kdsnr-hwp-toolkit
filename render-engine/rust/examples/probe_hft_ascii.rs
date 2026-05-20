//! `probe_hft_ascii` — HFT family 별 ASCII / Latin codepoint 보유 검증.
//!
//! Latin TTF fallback (system font) 폐기 + HFT 사용 위한 정공법 확인.
//! face name 으로 HFT cache lookup → ASCII 0x20-0x7e 의 hit 비율 추출.

use std::sync::Arc;

fn main() {
    let mut cache = kdsnr_hft::HftCache::new();
    let n = kdsnr_hft::embedded::load_into(&mut cache).expect("HFT load");
    eprintln!("HFT embedded: {n} glyphs, {} families, {} aliases",
              cache.family_count(), cache.alias_count());

    // 측정 대상: 실제 hwpx 에서 본 face names + 한컴 Latin 대표 family
    let faces = [
        // 한컴 본문 face (HFT 의 한글 family)
        "신명 신그래픽", "신명 중명조", "한양신명조", "한양견명조",
        "함초롬바탕", "함초롬돋움", "한컴고딕", "한컴바탕",
        "맑은 고딕", "Haansoft Batang",
        // 한컴 Latin 대표 family (canonical name)
        "ENSMJ", "ENMJ", "ENGT", "ENGGT", "ENGMJ",
        "HCENSMJ", "HCENGGT", "HCENGMJ",
        // ASCII chars 들이 있을 만한 HFT
        "HGMJ", "HCHGSMJ", "HCHGGGT", "HCHJJGT",
    ];

    println!("face,ascii_letter_hit,digit_hit,space_hit,period_hit,sample_advance");
    for face in &faces {
        let mut letter_hit = 0;
        for cp in (b'A'..=b'Z').chain(b'a'..=b'z') {
            if cache.get(face, cp as u32).is_some() { letter_hit += 1; }
        }
        let mut digit_hit = 0;
        for cp in b'0'..=b'9' {
            if cache.get(face, cp as u32).is_some() { digit_hit += 1; }
        }
        let space_hit = cache.get(face, 0x20).is_some();
        let period_hit = cache.get(face, b'.' as u32).is_some();
        let sample_advance = cache.get(face, b'A' as u32)
            .map(|g| format!("{:.2}em ({}adv/{}em)", g.advance as f32 / g.em as f32, g.advance, g.em))
            .unwrap_or_else(|| "-".to_string());
        println!("{},{}/52,{}/10,{},{},{}",
                 face, letter_hit, digit_hit, space_hit, period_hit, sample_advance);
    }

    // 추가: 한글 본문 face 가 ASCII 도 가지는가?
    println!();
    eprintln!("=== ASCII 가 한글 face 안에도 있는지 ===");
    let test_face = "신명 신그래픽";
    for cp in [0x20u32, 0x21, 0x28, 0x29, 0x2e, 0x30, 0x39, 0x41, 0x5a, 0x61, 0x7a] {
        let hit = cache.get(test_face, cp).is_some();
        let ch = char::from_u32(cp).unwrap_or('?');
        eprintln!("  {} {:04X} ({:?}) hit={}", test_face, cp, ch, hit);
    }

    // 한글 글자 advance 도 확인 (advance 값의 단위 검증)
    eprintln!();
    eprintln!("=== 한글 글자 advance (HCHGSMJ) ===");
    for cp in [0xAC00u32, 0xB098, 0xB2E4, 0xB77C, 0xB9C8, 0xBC14, 0xC0AC, 0xC544] {
        let ch = char::from_u32(cp).unwrap_or('?');
        if let Some(g) = cache.get("HCHGSMJ", cp) {
            eprintln!("  '{}' U+{:04X}  advance={} em={} ratio={:.3}",
                      ch, cp, g.advance, g.em, g.advance as f32 / g.em as f32);
        } else {
            eprintln!("  '{}' U+{:04X}  MISS", ch, cp);
        }
    }
    eprintln!();
    eprintln!("=== ASCII advance ratio (HCENSMJ) — 한글 대비 ===");
    for cp in [0x20u32, 0x2e, 0x30, 0x35, 0x41, 0x4d, 0x57, 0x61, 0x69, 0x6d, 0x77] {
        let ch = char::from_u32(cp).unwrap_or('?');
        if let Some(g) = cache.get("HCENSMJ", cp) {
            eprintln!("  '{}' U+{:04X}  advance={} em={} ratio={:.3}",
                      ch, cp, g.advance, g.em, g.advance as f32 / g.em as f32);
        } else {
            eprintln!("  '{}' U+{:04X}  MISS", ch, cp);
        }
    }
    let _ = Arc::new(cache);
}
