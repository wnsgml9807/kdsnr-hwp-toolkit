//! kdsnr-hft cache 글로벌 helper — rhwp 의 measurement 함수가 한컴 raw HFT advance 사용.
//!
//! `measure_char_width_embedded` 의 0순위 hook. lazy_static 으로 embedded HFT 자동 load
//! (한 번만, 첫 호출 시). 매핑 정책:
//! - face_name → HftCache::get(face, codepoint)
//! - cache 가 알아서 alias resolution (한컴 hftinfo.dat + 사용자 add_alias)
//! - hit 시 `advance / em` (em-unit) 반환. miss → None (caller fallback).

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
static HFT_CACHE: OnceLock<Arc<kdsnr_hft::HftCache>> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
pub static HIT_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(not(target_arch = "wasm32"))]
pub static MISS_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(not(target_arch = "wasm32"))]
static MISS_DUMP: OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, (u64, std::collections::HashSet<char>)>>,
> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
fn dump_miss(face: &str, c: char) {
    if std::env::var("RHWP_HFT_MISS_DUMP").is_err() {
        return;
    }
    let map = MISS_DUMP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut g = map.lock().unwrap();
    let entry = g
        .entry(face.to_string())
        .or_insert_with(|| (0, std::collections::HashSet::new()));
    entry.0 += 1;
    if entry.1.len() < 20 {
        entry.1.insert(c);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stats() -> (u64, u64) {
    (
        HIT_COUNT.load(Ordering::Relaxed),
        MISS_COUNT.load(Ordering::Relaxed),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn dump_miss_report() {
    if std::env::var("RHWP_HFT_MISS_DUMP").is_err() {
        return;
    }
    let Some(map) = MISS_DUMP.get() else {
        return;
    };
    let g = map.lock().unwrap();
    let mut v: Vec<(&String, &(u64, std::collections::HashSet<char>))> = g.iter().collect();
    v.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    eprintln!("\n=== HFT MISS by face (top 30) ===");
    for (face, (n, chars)) in v.iter().take(30) {
        let sample: String = chars.iter().take(20).collect();
        eprintln!("  {:>6}  face={face:?}  sample={sample:?}", n);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn add_default_aliases(c: &mut kdsnr_hft::HftCache) {
    // 사용자 alias 확장 — hftinfo.dat (한컴 V5) 에 없는 후속 폰트들
    use kdsnr_hft::alias::FaceCategory;
    c.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hangul);
    c.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hanja);
    c.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Symbol);
    c.add_alias("HamChoRomBatang", "HGSMJ", FaceCategory::Hangul);
    c.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hangul);
    c.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hanja);
    c.add_alias("함초롬돋움", "HGGGT", FaceCategory::Symbol);
    c.add_alias("HamChoRomDotum", "HGGGT", FaceCategory::Hangul);
    // 한컴 office TTF 본문 face → V5 HFT 매핑.
    // HBatang.TTF / HDotum.TTF 의 name table 에 Haansoft Batang/Gothic + 신명 시리즈 동시.
    c.add_alias("Haansoft Batang", "TEJMJHG", FaceCategory::Hangul);
    c.add_alias("Haansoft Batang", "TEJMJHG", FaceCategory::Hanja);
    c.add_alias("Haansoft Batang", "TEJMJHG", FaceCategory::Symbol);
    c.add_alias("HaansoftBatang", "TEJMJHG", FaceCategory::Hangul);
    c.add_alias("Haansoft Dotum", "HCHGGGT", FaceCategory::Hangul);
    c.add_alias("Haansoft Dotum", "HCHGGGT", FaceCategory::Hanja);
    c.add_alias("Haansoft Dotum", "HCHGGGT", FaceCategory::Symbol);
    c.add_alias("HaansoftDotum", "HCHGGGT", FaceCategory::Hangul);
    c.add_alias("Haansoft Gothic", "HCHGGGT", FaceCategory::Hangul);
    c.add_alias("Haansoft Gothic", "HCHGGGT", FaceCategory::Hanja);
    c.add_alias("Haansoft Gothic", "HCHGGGT", FaceCategory::Symbol);
    c.add_alias("HaansoftGothic", "HCHGGGT", FaceCategory::Hangul);
}

#[cfg(not(target_arch = "wasm32"))]
fn build_cache() -> Arc<kdsnr_hft::HftCache> {
    let mut c = kdsnr_hft::HftCache::new();
    let _ = kdsnr_hft::embedded::load_into(&mut c);
    add_default_aliases(&mut c);
    Arc::new(c)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn cache_arc() -> Arc<kdsnr_hft::HftCache> {
    HFT_CACHE.get_or_init(build_cache).clone()
}

#[cfg(not(target_arch = "wasm32"))]
fn cache() -> &'static kdsnr_hft::HftCache {
    HFT_CACHE.get_or_init(build_cache).as_ref()
}

/// 한컴 GT PDF 가 HCR Batang TTF 로 substitute 하는 것처럼 보였던 HFT face 이름들.
///
/// 이 목록은 진단용으로 유지한다. 실제 렌더 경로에서는 더 이상 차단하지 않는다.
/// SVG emit 이 raw HFT glyph path 를 쓰는 순간 advance 도 같은 raw HFT 를 써야
/// 한 줄 안 glyph들의 상대 위치가 한컴 export PDF 와 같은 기준 위에 놓인다.
pub fn is_substituted_hft_face(face_name: &str) -> bool {
    let n = face_name.trim();
    matches!(
        n,
        "한양중고딕"
            | "한양신명조"
            | "한양견고딕"
            | "한양견명조"
            | "한양그래픽"
            | "신명 태고딕"
            | "신명 태명조"
            | "신명 견고딕"
            | "신명 견명조"
            | "신명 중고딕"
            | "신명 세고딕"
            | "신명 세명조"
            | "신명 신명조"
            | "신명 신신명조"
            | "신명 중명조"
            | "신명 순명조"
            | "신명 신문명조"
            | "신명 디나루"
            | "신명 세나루"
            | "신명 신그래픽"
            | "신명 태그래픽"
    ) || n.starts_with('#')
}

/// `face_name` + `c` 의 HFT advance (em 단위) 반환. miss 시 None.
/// 한컴 raw HFT 가 갖는 advance — fontdb hmtx 보다 한컴 native 와 byte-eq 일치.
#[cfg(not(target_arch = "wasm32"))]
pub fn advance_em(face_name: &str, c: char) -> Option<f64> {
    // 2026-05-20: raw HFT glyph path emit 과 advance source 를 다시 통일.
    // 이전에는 일부 신명/한양 face 를 TTF substitute 로 간주해 HFT advance 를
    // 차단했지만, 그 상태에서는 SvgRenderer 가 raw HFT path 를 그리면서
    // paragraph_layout/kdsnr-layout 은 TTF advance 로 x 를 전진시키는 split-brain 이
    // 생긴다. 문항 내부 요소의 relative position 은 advance source 를 하나로
    // 고정해야 안정된다.
    let cp = c as u32;
    let g = match cache().get(face_name, cp) {
        Some(g) => g,
        None => {
            MISS_COUNT.fetch_add(1, Ordering::Relaxed);
            dump_miss(face_name, c);
            return None;
        }
    };
    if g.em == 0 || g.advance <= 0 {
        MISS_COUNT.fetch_add(1, Ordering::Relaxed);
        dump_miss(&format!("[bad em/adv] {face_name}"), c);
        return None;
    }
    let ratio = g.advance as f64 / g.em as f64;
    // ratio 0.3~2.0 만 신뢰 (HFT 의 ASCII advance 가 0.022-0.061em 인 비정상 케이스 제외)
    if ratio < 0.3 || ratio > 2.0 {
        MISS_COUNT.fetch_add(1, Ordering::Relaxed);
        dump_miss(&format!("[bad ratio={ratio:.2}] {face_name}"), c);
        return None;
    }
    HIT_COUNT.fetch_add(1, Ordering::Relaxed);
    Some(ratio)
}

#[cfg(target_arch = "wasm32")]
pub fn advance_em(_face_name: &str, _c: char) -> Option<f64> {
    None
}
