//! 런타임 폰트 metric 측정 — TTF/OTF 의 hmtx 테이블에서 직접 글자 폭을 측정.
//!
//! 동기: 기존 `font_metrics_data` 의 하드코딩된 메트릭 테이블 + 휴리스틱 fallback
//! (CJK=font_size, Latin=font_size×0.5, narrow_punctuation=font_size×0.3 등) 은
//! 한컴 양식 PDF 와 글자 폭 차이를 만들어 wrap point 가 어긋난다.
//!
//! 본 모듈은 fontdb 에 로드된 모든 폰트 (시스템 + ttfs/hancom/flat + 한컴오피스 + WSL)
//! 의 `Face` 에 직접 접근하여 글자별 `hor_advance` 를 em 단위로 반환한다.
//! 호출자는 `em * font_size_px` 로 픽셀 폭을 얻는다.
//!
//! 휴리스틱 0 원칙: 매칭 실패 시 `None` 만 반환한다. 호출자가 추정하지 않는다.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// HFT glyph cache (raid 23). `--hft-path` 활성 시 채워지며, 한글 음절/자모의
/// advance lookup 을 TTF fontdb 대신 HFT blob.metrics 기반으로 한다. layout
/// 단계와 SvgRenderer::draw_text emit 단계가 동일한 advance 를 사용해야
/// 줄 내 누적 오차가 사라진다 (raid 23 v2 → v3 fix).
static GLOBAL_HFT_CACHE: OnceLock<Mutex<Option<Arc<kdsnr_hft::HftCache>>>> = OnceLock::new();

fn hft_cache_slot() -> &'static Mutex<Option<Arc<kdsnr_hft::HftCache>>> {
    GLOBAL_HFT_CACHE.get_or_init(|| Mutex::new(None))
}

/// HFT cache 설정. 이미 캐싱된 advance 결과는 invalidate.
pub fn set_global_hft_cache(new_cache: Arc<kdsnr_hft::HftCache>) {
    if let Ok(mut slot) = hft_cache_slot().lock() {
        *slot = Some(new_cache);
    }
    // measure_char_advance_em 의 cache 도 비워 새 HFT advance 가 반영되게.
    if let Ok(mut c) = cache().lock() {
        c.clear();
    }
}

/// HFT cache 에서 글자의 advance (em 단위) 를 가져온다. miss 면 None.
///
/// cache.get 내부에서 hftinfo.dat alias 까지 시도하므로 별도 blanket
/// fallback 을 두지 않는다. alias 매핑이 없는 face_family (TTF 전용)
/// 는 자연스럽게 None 으로 떨어져 TTF advance 측정으로 흘러간다.
fn hft_advance_em(family: &str, ch: char) -> Option<f64> {
    let slot = hft_cache_slot().lock().ok()?;
    let cache = slot.as_ref()?;
    let code = ch as u32;
    let glyph = cache.get(family, code)?;
    let em = glyph.em as f64;
    if em <= 0.0 {
        return None;
    }
    let adv = glyph.advance as f64 / em;
    if std::env::var("RHWP_HFT_DEBUG").is_ok() {
        eprintln!(
            "HFT advance hit: family={:?} ch={:?} code=U+{:04X} → {:.4} em",
            family, ch, code, adv
        );
    }
    Some(adv)
}

/// 글로벌 fontdb 인스턴스 (한 번 초기화 후 재사용).
/// pdf.rs 의 `create_fontdb` 와 동일한 로드 정책을 사용한다.
fn fontdb() -> &'static usvg::fontdb::Database {
    static DB: OnceLock<usvg::fontdb::Database> = OnceLock::new();
    DB.get_or_init(build_fontdb)
}

/// fontdb 빌드 — pdf.rs::create_fontdb 와 동일 로드 정책.
///
/// 우선순위: 시스템 → binary 위치 기준 ttfs → cwd 기준 ttfs → macOS 한컴오피스 →
/// WSL Windows → `RHWP_FONT_DIR` 환경변수.
fn build_fontdb() -> usvg::fontdb::Database {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();

    let project_dirs = ["ttfs/hancom/flat", "ttfs/hwp", "ttfs/windows", "ttfs"];

    if let Ok(exe) = std::env::current_exe() {
        if let Some(crate_root) = exe.ancestors().nth(3) {
            for sub in &project_dirs {
                let p = crate_root.join(sub);
                if p.exists() {
                    db.load_fonts_dir(&p);
                }
            }
        }
    }

    for dir in &project_dirs {
        if std::path::Path::new(dir).exists() {
            db.load_fonts_dir(dir);
        }
    }

    // pdf.rs::create_fontdb 와 동일하게 Install / Hwp / All 셋 다 로드.
    // (HY헤드라인M 등 /All/ 전용 face 가 측정 단계에서도 매칭되어야 layout 계산이 일치한다.)
    for dir in &[
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Hwp",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/All",
    ] {
        if std::path::Path::new(dir).exists() {
            db.load_fonts_dir(dir);
        }
    }

    // toolkit assets — HCR Batang/Dotum (한컴 official HancomFont.zip 2017).
    // GT PDF embed 실측: 신명/한양 series HFT face 는 HCR Batang TTF 로 substitute.
    // layout text_measurement 가 HCR Batang advance 사용해야 줄바꿈 위치 GT 와 일치.
    if let Ok(exe) = std::env::current_exe() {
        for n in 0..8 {
            if let Some(anc) = exe.ancestors().nth(n) {
                let assets = anc.join("assets/fonts");
                if assets.is_dir() {
                    db.load_fonts_dir(&assets);
                    break;
                }
            }
        }
    }

    if std::path::Path::new("/mnt/c/Windows/Fonts").exists() {
        db.load_fonts_dir("/mnt/c/Windows/Fonts");
    }

    if let Ok(extra) = std::env::var("RHWP_FONT_DIR") {
        for p in extra.split(':') {
            if !p.is_empty() && std::path::Path::new(p).exists() {
                db.load_fonts_dir(p);
            }
        }
    }

    db
}

/// pdf 렌더 단계에서 동일 fontdb 인스턴스를 가져갈 수 있도록 expose.
pub fn shared_fontdb() -> &'static usvg::fontdb::Database {
    fontdb()
}

/// 측정 캐시 — `(family_lower, bold, italic, ch)` → advance (em 단위).
fn cache() -> &'static Mutex<HashMap<(String, bool, bool, char), Option<f64>>> {
    static C: OnceLock<Mutex<HashMap<(String, bool, bool, char), Option<f64>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// fontdb 에서 (family, bold, italic) 와 가장 일치하는 face 의 ID 를 찾는다.
fn resolve_face_id(family: &str, bold: bool, italic: bool) -> Option<usvg::fontdb::ID> {
    use usvg::fontdb::{Family, Query, Stretch, Style, Weight};
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let style = if italic { Style::Italic } else { Style::Normal };
    let owned = family.to_string();
    let query = Query {
        families: &[Family::Name(&owned)],
        weight,
        stretch: Stretch::Normal,
        style,
    };
    fontdb().query(&query)
}

/// 한 글자의 advance width 를 em 단위로 측정.
///
/// 반환값을 `font_size_px` 와 곱하면 픽셀 폭이 된다.
///
/// fontdb 에 해당 family 가 없거나 ttf 파싱 실패 또는 글자 glyph 누락 시 `None`.
/// 호출자는 추정하지 말고 다른 폰트를 시도하거나 측정 실패를 명시적으로 처리해야 한다.
pub fn measure_char_advance_em(family: &str, bold: bool, italic: bool, ch: char) -> Option<f64> {
    let key = (family.to_ascii_lowercase(), bold, italic, ch);
    if let Ok(c) = cache().lock() {
        if let Some(v) = c.get(&key) {
            return *v;
        }
    }

    // HFT 우선: 한글 음절/자모는 HFT 의 정확한 advance 사용. layout-emit sync.
    let result = hft_advance_em(family, ch).or_else(|| measure_uncached(family, bold, italic, ch));

    if let Ok(mut c) = cache().lock() {
        c.insert(key, result);
    }
    result
}

fn measure_uncached(family: &str, bold: bool, italic: bool, ch: char) -> Option<f64> {
    let face_id = resolve_face_id(family, bold, italic)?;
    let db = fontdb();
    db.with_face_data(face_id, |data, face_index| -> Option<f64> {
        let face = ttf_parser::Face::parse(data, face_index).ok()?;
        let glyph_id = face.glyph_index(ch)?;
        let adv_units = face.glyph_hor_advance(glyph_id)? as f64;
        let upem = face.units_per_em() as f64;
        if upem <= 0.0 {
            return None;
        }
        Some(adv_units / upem)
    })?
}

/// 폰트 행 높이 (em 단위) — ascender + |descender| + line_gap.
pub fn measure_face_metrics_em(family: &str, bold: bool, italic: bool) -> Option<FaceMetricsEm> {
    let face_id = resolve_face_id(family, bold, italic)?;
    let db = fontdb();
    db.with_face_data(face_id, |data, face_index| -> Option<FaceMetricsEm> {
        let face = ttf_parser::Face::parse(data, face_index).ok()?;
        let upem = face.units_per_em() as f64;
        if upem <= 0.0 {
            return None;
        }
        Some(FaceMetricsEm {
            ascender: face.ascender() as f64 / upem,
            descender: face.descender() as f64 / upem,
            line_gap: face.line_gap() as f64 / upem,
            units_per_em: upem,
        })
    })?
}

#[derive(Debug, Clone, Copy)]
pub struct FaceMetricsEm {
    pub ascender: f64,
    pub descender: f64,
    pub line_gap: f64,
    pub units_per_em: f64,
}

/// 디버그용 — 등록된 모든 폰트 family 목록.
#[allow(dead_code)]
pub fn list_loaded_families() -> Vec<String> {
    let mut names: Vec<String> = fontdb()
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}
