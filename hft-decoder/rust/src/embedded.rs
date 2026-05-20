//! Embedded Hancom HFT archive — binary 에 embed 된 한컴 폰트 380+ family.
//!
//! ## 활성화
//!
//! Cargo feature `embedded` 가 켜져야 본 module 이 컴파일됨. 기본 비활성화.
//! 활성화 시 `hft-decoder/rust/fonts/` 의 모든 `*.HFT` + `hftinfo.dat` 가
//! binary 에 포함 → runtime 에 fs IO 없이 cache 채울 수 있음 (서버에 한컴
//! office 설치 불필요).
//!
//! ## 사용
//!
//! ```rust,ignore
//! let mut cache = kdsnr_hft::HftCache::new();
//! let n = kdsnr_hft::embedded::load_into(&mut cache).expect("load embedded");
//! println!("loaded {} glyphs", n);
//! ```
//!
//! ## 사이즈
//!
//! binary 에 ~180MB 추가. cargo build 시간 / binary size 부담.
//!
//! ## 라이센스
//!
//! 한컴 HFT 폰트는 한컴 office 라이센스에 종속. 본 embed 는 한컴 office
//! 라이센스 보유 사내 사용 가정. **외부 배포 금지**.

use crate::HftCache;
use include_dir::{include_dir, Dir};

/// 한컴 HFT 폴더 (180MB, 380+ HFT + hftinfo.dat) 의 컴파일 시점 embed.
///
/// `hft-decoder/rust/fonts/` 폴더 통째로 binary 에 박힘.
pub static FONTS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/fonts");

/// embedded HFT archive 의 모든 `*.HFT` 와 `hftinfo.dat` 를 cache 에 load.
///
/// 반환값: cache 된 총 glyph 수 (모든 HFT 합산). hftinfo.dat 가 폴더에 있으면
/// alias 도 자동 load.
pub fn load_into(cache: &mut HftCache) -> Result<usize, String> {
    let mut total = 0usize;
    for entry in FONTS.files() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.to_ascii_lowercase().ends_with(".hft") {
            match cache.load_hft_bytes(name, entry.contents()) {
                Ok(n) => total += n,
                Err(_) => {} // 일부 HFT 가 parser 실패할 수 있음 — silent skip
            }
        }
    }
    if let Some(alias) = FONTS.get_file("hftinfo.dat") {
        let _ = cache.load_aliases_bytes(alias.contents());
    }
    Ok(total)
}

/// embedded archive 안의 파일 수 (HFT + 기타 .dat).
pub fn file_count() -> usize {
    FONTS.files().count()
}

/// 특정 파일의 bytes 직접 추출 (예: `"hftinfo.dat"`).
pub fn get_file_bytes(filename: &str) -> Option<&'static [u8]> {
    FONTS.get_file(filename).map(|f| f.contents())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_has_files() {
        let n = file_count();
        assert!(n > 100, "expected 100+ embedded files, got {}", n);
    }

    #[test]
    fn embedded_has_hftinfo_dat() {
        assert!(get_file_bytes("hftinfo.dat").is_some());
    }

    #[test]
    fn embedded_load_into_cache_succeeds() {
        let mut cache = HftCache::new();
        let n = load_into(&mut cache).expect("load");
        assert!(n > 10_000, "expected 10k+ glyphs, got {}", n);
        assert!(cache.family_count() > 50);
        assert!(cache.alias_count() > 0);
    }
}
