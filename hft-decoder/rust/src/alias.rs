//! Parse `hftinfo.dat` to build a face-name → HFT-canonical alias map.
//!
//! `hftinfo.dat` (UTF-16 LE) is shipped alongside the .HFT files in
//! `Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts/`. It contains
//! several `[Font Definition - {Hangul,Latin,Hanja,Japanese,Symbol,User,Other}]`
//! sections; each line is:
//!
//!   `<KoreanDisplayName>=<FILENAME>.HFT,,,,<vendor>,<lang>,<EnglishName>`
//!
//! We collect both the Korean display name and the English alias as keys,
//! pointing to the canonical filename stem (uppercased — same as
//! `HftCache::canonical_name`). Category and font-class metadata is preserved
//! so the consumer can pick the right HFT per Unicode category (Hangul vs
//! Symbol vs Hanja) when a font_family has multiple HFT siblings.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Category of an HFT entry from `hftinfo.dat`. Roughly maps to the Unicode
/// block the file covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceCategory {
    Hangul,
    Latin,
    Hanja,
    Japanese,
    Symbol,
    User,
    Other,
}

#[derive(Debug, Clone)]
pub struct FaceEntry {
    /// Canonical HFT name (filename stem, uppercased).
    pub hft: String,
    pub category: FaceCategory,
}

/// Maps face-name aliases (Korean display, English alias) to HFT entries.
#[derive(Debug, Default, Clone)]
pub struct AliasMap {
    /// key = uppercased trimmed face name; value = list of categories under
    /// which that name appears (one font_face may have a Hangul AND a Symbol
    /// HFT — pick by Unicode category at lookup time).
    aliases: HashMap<String, Vec<FaceEntry>>,
}

fn normalize_key(s: &str) -> String {
    // Hancom display names sometimes include a leading '#' (e.g. "#견고딕");
    // keep it as-is — strip only whitespace + uppercase ASCII. We do NOT
    // unicode-fold so that e.g. "한양신명조" stays distinct from "한양 신명조".
    s.trim().to_uppercase()
}

fn parse_category(header: &str) -> Option<FaceCategory> {
    let h = header.trim_matches(|c: char| c == '[' || c == ']');
    let suffix = h.rsplit('-').next()?.trim();
    Some(match suffix {
        "Hangul" => FaceCategory::Hangul,
        "Latin" => FaceCategory::Latin,
        "Hanja" => FaceCategory::Hanja,
        "Japanese" => FaceCategory::Japanese,
        "Symbol" => FaceCategory::Symbol,
        "User" => FaceCategory::User,
        "Other" => FaceCategory::Other,
        _ => return None,
    })
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let mut start = 0;
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        start = 2;
    }
    let mut units = Vec::with_capacity((bytes.len() - start) / 2);
    let mut i = start;
    while i + 1 < bytes.len() {
        units.push(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

impl AliasMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `hftinfo.dat` content (UTF-16 LE) into an alias map. Lines that
    /// don't match the `<display>=<file>.HFT,...` shape are ignored silently.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        let text = decode_utf16le(bytes);
        let mut map = Self::new();
        let mut cat: Option<FaceCategory> = None;
        for raw_line in text.lines() {
            let line = raw_line.trim_start_matches('\u{FEFF}').trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') {
                cat = parse_category(line);
                continue;
            }
            // Only `Font Definition` sections produce entries.
            let category = match cat {
                Some(c) => c,
                None => continue,
            };
            let eq = match line.find('=') {
                Some(i) => i,
                None => continue,
            };
            let lhs = line[..eq].trim();
            let rhs = line[eq + 1..].trim();
            if lhs.is_empty() {
                continue;
            }
            let parts: Vec<&str> = rhs.split(',').collect();
            // Modern Hangul format: <file>.HFT,,,,vendor,lang,English
            //   parts[0] = primary HFT
            // Some entries use parts[1] (e.g. legacy "산돌 ..." lines have file in 2nd column).
            let primary = parts.get(0).copied().unwrap_or("").trim();
            let alt = parts.get(1).copied().unwrap_or("").trim();
            let english = parts.get(6).copied().unwrap_or("").trim();

            let hft_file = if !primary.is_empty() && primary.to_ascii_uppercase().ends_with(".HFT") {
                primary
            } else if !alt.is_empty() && alt.to_ascii_uppercase().ends_with(".HFT") {
                alt
            } else {
                continue;
            };
            let canonical = canonical_from_filename(hft_file);
            let entry = FaceEntry { hft: canonical.clone(), category };
            map.insert_alias(lhs, entry.clone());
            if !english.is_empty() && english != lhs {
                map.insert_alias(english, entry.clone());
            }
            // Also alias the canonical name itself, so cache.get("HCHGGGT", ...) works.
            map.insert_alias(&canonical, entry);
        }
        map
    }

    pub fn load_path<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let bytes = fs::read(path.as_ref()).map_err(|e| format!("read alias: {}", e))?;
        Ok(Self::parse_bytes(&bytes))
    }

    fn insert_alias(&mut self, name: &str, entry: FaceEntry) {
        let key = normalize_key(name);
        let v = self.aliases.entry(key).or_default();
        if !v.iter().any(|e| e.hft == entry.hft && e.category == entry.category) {
            v.push(entry);
        }
    }

    /// Public extension API — caller adds an alias mapping.
    /// Used for fonts NOT in hftinfo.dat (예: 함초롬바탕 같은 한컴 V6+ 폰트,
    /// 사용자 임의 별칭).
    ///
    /// `hft_canonical` is the target HFT canonical name (e.g. "HGSMJ" for
    /// 한양신명조 — must be a name that's actually been loaded via
    /// `HftCache::load_hft_bytes` etc.).
    pub fn add_alias(&mut self, face_name: &str, hft_canonical: &str, category: FaceCategory) {
        let entry = FaceEntry {
            hft: hft_canonical.trim().to_uppercase(),
            category,
        };
        self.insert_alias(face_name, entry);
    }

    /// Return all HFT candidates for the given face name (any category).
    pub fn resolve_all(&self, face_name: &str) -> &[FaceEntry] {
        match self.aliases.get(&normalize_key(face_name)) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }

    /// Pick the best HFT for a face_name given a Unicode codepoint.
    ///
    /// 한 컴 GT PDF 매칭 정책:
    /// - Hangul / Hanja / Symbol / Japanese: alias HFT (vector path) 사용
    /// - **Latin: alias 무시 → 호출자가 TTF text emit 으로 폴백**.
    ///   이유: GT PDF 가 본문 Latin 을 TTF Tj 로 임베드 (Haansoft-Batang /
    ///   HCRBatang). TEJMJEN 처럼 Hangul 폰트와 페어된 압축 Latin HFT 가
    ///   있지만, GT PDF 는 사용하지 않음 (raid 24 측정).
    /// - 단 face_name 이 HFT 캐논컬 이름인 경우 (예: `cache.get("HCENGGT", ch)`)
    ///   는 `cache.get` 내 canonical 경로에서 직접 매칭되므로 본 함수와 무관.
    pub fn resolve_for_code(&self, face_name: &str, code: u32) -> Option<&FaceEntry> {
        let want = category_for_code(code);
        if want == FaceCategory::Latin {
            return None;
        }
        let entries = self.resolve_all(face_name);
        entries.iter().find(|e| e.category == want)
    }

    /// Total alias keys (for diagnostics).
    pub fn alias_count(&self) -> usize {
        self.aliases.len()
    }
}

fn canonical_from_filename(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    stem.trim().to_uppercase()
}

/// Classify a Unicode codepoint into one of our `FaceCategory` slots, so the
/// alias resolver can pick the right HFT sibling.
pub fn category_for_code(code: u32) -> FaceCategory {
    match code {
        // ASCII + Latin-1 Supplement
        0x0020..=0x024F => FaceCategory::Latin,
        // Hangul Jamo + Compatibility Jamo + Syllables
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => FaceCategory::Hangul,
        // CJK Unified Ideographs (Hanja)
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF => FaceCategory::Hanja,
        // Hiragana/Katakana
        0x3040..=0x30FF | 0x31F0..=0x31FF => FaceCategory::Japanese,
        // Everything else (CJK Symbols/Punctuation, halfwidth/fullwidth forms,
        // Mathematical Operators, etc.) → Symbol.
        _ => FaceCategory::Symbol,
    }
}
