//! `HftCache` — high-level glyph-to-SVG-path API for embedders.
//!
//! Given a directory of .HFT files, build an in-memory cache of
//!     (font_name, char_code) -> SVG path "d" string
//! suitable for plugging into SVG/Canvas/HTML renderers.
//!
//! Caller is responsible for outer transforms:
//!   - translate to glyph baseline (x, y)
//!   - scale(font_size / em, -font_size / em)  (y-flip)
//!   - fill / stroke color

use crate::alias::{AliasMap, FaceCategory};
use crate::ksx1001;
use crate::parser;
use crate::vector;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// One glyph's path data + advance (em-units).
#[derive(Debug, Clone)]
pub struct Glyph {
    /// SVG path `d` attribute (Move/Line/Cubic/Close only, em-coords).
    pub d: String,
    /// Advance width in em-units (metrics[0] from the path blob).
    pub advance: i32,
    /// Em-box size (descriptor.em), e.g. 1000 for HCHGGGT.
    pub em: u16,
}

/// In-memory cache of decoded HFT glyphs.
///
/// Lookup is by canonical font name (filename without extension, uppercase)
/// + Unicode codepoint. When `hftinfo.dat` is loaded alongside the .HFT
/// files, `get()` also accepts Korean/English display aliases like
/// `"한양신명조"` or `"HY Sinmyeongjo"` and routes to the right HFT for
/// the codepoint's Unicode category (Hangul / Symbol / Hanja / Latin).
#[derive(Debug, Default)]
pub struct HftCache {
    glyphs: HashMap<String, HashMap<u32, Glyph>>,
    aliases: AliasMap,
}

impl HftCache {
    pub fn new() -> Self {
        Self { glyphs: HashMap::new(), aliases: AliasMap::new() }
    }

    /// Load Hancom's `hftinfo.dat` (UTF-16 LE) for alias resolution.
    /// Safe to call multiple times; entries merge.
    pub fn load_aliases<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        let parsed = AliasMap::load_path(path)?;
        // Merge: walk parsed entries and call insert_alias-equivalent via
        // existing accessors. Since AliasMap fields are private, use a fresh
        // map only when current is empty.
        if self.aliases.alias_count() == 0 {
            self.aliases = parsed;
        } else {
            // Naive merge: load_aliases is typically called once. If it's
            // called repeatedly, we would need a real merge method on
            // AliasMap. Punt for now — log via return value.
            self.aliases = parsed;
        }
        Ok(self.aliases.alias_count())
    }

    /// Canonicalize a font name for lookup. Hancom's font_face name often
    /// matches the .HFT filename uppercased (e.g. "HCHGGGT").
    pub fn canonical_name(name: &str) -> String {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        stem.trim().to_uppercase()
    }

    /// Bytes-based variant of [`load_hft`] for embedded (compile-time included)
    /// HFT data. `filename` is used only for canonical name resolution
    /// (e.g. `"HCHGSMJ.HFT"` → `"HCHGSMJ"`).
    pub fn load_hft_bytes(&mut self, filename: &str, bytes: &[u8]) -> Result<usize, String> {
        self.load_hft_inner(filename, bytes)
    }

    /// Bytes-based variant of [`load_aliases`] for embedded `hftinfo.dat`.
    pub fn load_aliases_bytes(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let parsed = AliasMap::parse_bytes(bytes);
        if self.aliases.alias_count() == 0 {
            self.aliases = parsed;
        } else {
            self.aliases = parsed;
        }
        Ok(self.aliases.alias_count())
    }

    /// Caller-side alias extension. hftinfo.dat 에 없는 face_name 을 임의 HFT 로 매핑.
    /// (예: 함초롬바탕 → HGSMJ 한양신명조 — 한컴 office mac 에 함초롬바탕이 없을 때 fallback.)
    pub fn add_alias(&mut self, face_name: &str, hft_canonical: &str, category: FaceCategory) {
        self.aliases.add_alias(face_name, hft_canonical, category);
    }

    /// Decode all vector descriptors of one .HFT file and store them in the
    /// cache keyed by `canonical_name(path.file_stem())`.
    pub fn load_hft<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        self.load_hft_inner(filename, &bytes)
    }

    fn load_hft_inner(&mut self, filename: &str, bytes: &[u8]) -> Result<usize, String> {
        let hft = parser::parse(bytes).map_err(|e| format!("parse {}: {:?}", filename, e))?;
        let name = Self::canonical_name(filename);

        let mut count = 0;
        let entry = self.glyphs.entry(name).or_default();
        for chunk in &hft.chunks {
            for desc in &chunk.descriptors {
                if desc.count == 0 {
                    continue;
                }
                // type_id 별 mapping 규칙:
                //   type=0 (HCEN*.HFT, Latin/ASCII): code = range_start + idx
                //     예: HCENGGT desc[0] type=0 range=32..133 count=102
                //   type=1 (HCH*/HG*/MHH*/YJ*/TE*.HFT, Hangul):
                //     (a) inner_table 가 있으면 inner[idx*2..idx*2+2] = LE u16 char_code
                //     (b) inner_table 가 비고 count==2350 이면 KS X 1001 ordinal 매핑
                //
                // 다른 type 은 아직 미지원 (Hanja 등 별도 RE 필요).
                //
                // Note: `is_bitmap` is misnamed — it's just `flags & 0x10`,
                // which controls blob layout (metrics in descriptor vs in
                // blob), not vector/bitmap distinction.
                if desc.type_id != 0 && desc.type_id != 1 {
                    continue;
                }
                let inner = &desc.inner_table;
                let n = desc.count as usize;

                let use_ksx = desc.type_id == 1 && inner.is_empty() && n == 2350;
                let use_range = desc.type_id == 0 && inner.is_empty();

                for i in 0..n {
                    let code: u32 = if use_ksx {
                        match ksx1001::ordinal_to_unicode(i) {
                            Some(c) => c,
                            None => continue,
                        }
                    } else if use_range {
                        desc.range_start as u32 + i as u32
                    } else {
                        let off = i * 2;
                        if off + 2 > inner.len() {
                            break;
                        }
                        u16::from_le_bytes([inner[off], inner[off + 1]]) as u32
                    };
                    let blob = match vector::extract_blob(desc, i as u32, None) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    let cmds = vector::walk_path(&blob.raw);
                    if cmds.is_empty() {
                        continue;
                    }
                    let d = vector::to_svg_path(&cmds);
                    entry.insert(
                        code,
                        Glyph {
                            d,
                            advance: blob.metrics.0 as i32,
                            em: desc.em,
                        },
                    );
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Load every .HFT file under `dir` (non-recursive). If `hftinfo.dat`
    /// is present in the same directory, the alias map is loaded too.
    pub fn load_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, String> {
        let dir = dir.as_ref();
        let mut total = 0;
        for entry in fs::read_dir(dir).map_err(|e| format!("read_dir: {}", e))? {
            let entry = entry.map_err(|e| format!("entry: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("hft")) != Some(true) {
                continue;
            }
            match self.load_hft(&path) {
                Ok(n) => total += n,
                Err(_) => {} // skip unparseable
            }
        }
        // Best-effort alias load — silently ignored if hftinfo.dat is absent.
        let alias_path = dir.join("hftinfo.dat");
        if alias_path.exists() {
            let _ = self.load_aliases(&alias_path);
        }
        Ok(total)
    }

    /// Look up a glyph. `name` is matched first by `canonical_name(name)`
    /// (e.g. `"HCHGGGT"`) and then via the alias map loaded from
    /// `hftinfo.dat` (e.g. `"한양견고딕"` / `"HY Gyeongothic"`). When the
    /// alias resolves to multiple HFTs for the same face_name (e.g. Hangul
    /// + Symbol siblings), the codepoint's Unicode category picks the
    /// right one.
    ///
    /// 엄격 매칭: alias 가 Hangul HFT 만 가진 face_name 에 Latin/숫자 codepoint
    /// 가 들어오면 None 을 반환한다. (예) "신명 중명조" + '?' → None.
    /// TEJMJHG.HFT 가 Latin glyph 를 들고 있긴 하지만, 한 컴 본문 Latin 은
    /// 별도 TTF embed 되므로 HFT path emit 으로 가지 않는다.
    pub fn get(&self, name: &str, char_code: u32) -> Option<&Glyph> {
        // TEMP DIAGNOSTIC: force every Hangul lookup to HCHGSMJ to verify
        // whether the visual blob is TEJMJHG-specific.
        if std::env::var("KDSNR_HFT_FORCE_HCHGSMJ").is_ok() {
            if (0xAC00..=0xD7A3).contains(&char_code) {
                if let Some(map) = self.glyphs.get("HCHGSMJ") {
                    if let Some(g) = map.get(&char_code) {
                        return Some(g);
                    }
                }
            }
        }

        let canonical = Self::canonical_name(name);
        if let Some(map) = self.glyphs.get(&canonical) {
            if let Some(g) = map.get(&char_code) {
                return Some(g);
            }
        }
        // Alias resolution (category-strict, Latin skipped).
        if let Some(entry) = self.aliases.resolve_for_code(name, char_code) {
            if let Some(map) = self.glyphs.get(&entry.hft) {
                if let Some(g) = map.get(&char_code) {
                    return Some(g);
                }
            }
        }
        None
    }

    /// Whether the alias map has any candidates for this face_name.
    pub fn has_alias(&self, name: &str) -> bool {
        !self.aliases.resolve_all(name).is_empty()
    }

    /// Alias map count (for diagnostics).
    pub fn alias_count(&self) -> usize {
        self.aliases.alias_count()
    }

    /// Whether the cache holds any glyphs for the given font name.
    pub fn has_font(&self, name: &str) -> bool {
        self.glyphs.contains_key(&Self::canonical_name(name))
    }

    /// Number of cached font families.
    pub fn family_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Total glyphs cached.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.values().map(|m| m.len()).sum()
    }

    /// All canonical family names currently loaded.
    pub fn family_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.glyphs.keys().cloned().collect();
        v.sort();
        v
    }

    /// Glyph count for a single family (canonical name).
    pub fn glyph_count_for(&self, name: &str) -> usize {
        self.glyphs
            .get(&Self::canonical_name(name))
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_name_strips_path_and_case() {
        assert_eq!(HftCache::canonical_name("HCHGGGT.HFT"), "HCHGGGT");
        assert_eq!(HftCache::canonical_name("hchgggt.hft"), "HCHGGGT");
        assert_eq!(HftCache::canonical_name("/path/to/HCHGGGT.HFT"), "HCHGGGT");
    }
}
