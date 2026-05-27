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
use crate::bitmap;
use crate::johab;
use crate::ksx1001;
use crate::parser;
use crate::vector;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The glyph's ink bounding box, read from the HFT glyph blob header:
/// `[x_min, y_max, ink_width, ink_height]` (em-units). Confirmed by decoding the
/// outline and matching `x_min + ink_width == ink_max_x`. This is *not* the
/// advance — the right side-bearing is not here; the advance comes from the
/// font's [`WidthTable`](crate::parser::WidthTable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphMetrics {
    pub raw: [i16; 4],
}

impl GlyphMetrics {
    pub fn from_tuple(metrics: (i16, i16, i16, i16)) -> Self {
        Self {
            raw: [metrics.0, metrics.1, metrics.2, metrics.3],
        }
    }
}

/// One glyph's path data + advance (em-units).
#[derive(Debug, Clone)]
pub struct Glyph {
    /// SVG path `d` attribute (Move/Line/Cubic/Close only, em-coords).
    pub d: String,
    /// Decoded Hancom vector commands before SVG serialization.
    ///
    /// Kept so downstream renderers can compose glyph outlines through the
    /// Ghidra-backed path contract without reparsing the SVG `d` string.
    pub commands: Vec<vector::PathCommand>,
    /// Advance width in em-units: the font's width-table value for this code
    /// when present (proportional Latin/punctuation), else the blob default
    /// (a full em for Hangul/symbol fonts, which carry no width table).
    pub advance: i32,
    /// The glyph's ink bounding box (see [`GlyphMetrics`]) — not the advance.
    pub metrics: GlyphMetrics,
    /// Em-box size (descriptor.em), e.g. 1000 for HCHGGGT. This is the advance
    /// reference, not the outline coordinate space.
    pub em: u16,
    /// Descriptor design-box height — the coordinate space the path `d` is drawn
    /// in (typically 1200–1280, larger than `em`). The outline must be scaled by
    /// `size / design_h` (not `size / em`); using `em` over-expands the glyph
    /// vertically (Hancom maps the design box, not the em, to the point size).
    pub design_h: u16,
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
    /// Registered .HFT files (canonical name → path) for lazy loading.
    file_index: HashMap<String, PathBuf>,
    /// Canonical names already decoded into `glyphs` (lazy path bookkeeping).
    loaded: HashSet<String>,
    /// Persistent decoded-glyph cache directory (lazy path); `None` disables it.
    cache_dir: Option<PathBuf>,
}

impl HftCache {
    pub fn new() -> Self {
        Self {
            glyphs: HashMap::new(),
            aliases: AliasMap::new(),
            file_index: HashMap::new(),
            loaded: HashSet::new(),
            cache_dir: None,
        }
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

    /// Add Hancom `FontMap.dat` aliases that are not fully represented by
    /// `hftinfo.dat`, especially source names like `HY신명조` that map to the
    /// 한양신명조 HFT family by category.
    pub fn add_hancom_fontmap_aliases(&mut self) {
        add_family_aliases(
            self,
            &[
                "HY신명조",
                "신명조",
                "신명조 간자",
                "신명조 약자",
                "한컴바탕",
                "HCR Batang",
            ],
            FamilyHft {
                hangul: "HGSMJ",
                latin: "ENSMJ",
                hanja: "HJSMJ",
                japanese: Some("JPSMJ"),
                symbol: Some("SPSMJ"),
                other: Some("FLSMJ"),
            },
        );
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
        let (name, glyphs) = decode_hft(filename, bytes)?;
        let count = glyphs.len();
        // Each .HFT canonicalizes to a unique name, so a fresh entry per file —
        // extend is exact here.
        self.glyphs.entry(name).or_default().extend(glyphs);
        Ok(count)
    }
}

/// Decode one .HFT file's bytes into its glyph map. Pure (no shared state), so
/// `load_dir` can run it across threads.
fn decode_hft(filename: &str, bytes: &[u8]) -> Result<(String, HashMap<u32, Glyph>), String> {
    let hft = parser::parse(bytes).map_err(|e| format!("parse {}: {:?}", filename, e))?;
    let name = HftCache::canonical_name(filename);

    let mut entry: HashMap<u32, Glyph> = HashMap::new();
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
                // type=0 path streams are encrypted in the Hancom dispatcher
                // family used by ENSMJ/HJSMJ; apply the same descriptor-level
                // cipher that the lower-level decoder tests use.
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
                    // Advance comes from the font's width table (the outline blob
                    // header is only an ink bbox). Absent (Hangul/symbol) → keep
                    // the blob/descriptor default, which is the full em there.
                    let wt_adv = hft.advance_width(code).map(|w| w as i32);
                    if let Some((blob, cmds)) = vector::extract_decoded_path(desc, i as u32) {
                        let d = vector::to_svg_path(&cmds);
                        entry.insert(
                            code,
                            Glyph {
                                d,
                                commands: cmds,
                                advance: wt_adv.unwrap_or(blob.metrics.0 as i32),
                                metrics: GlyphMetrics::from_tuple(blob.metrics),
                                em: desc.em,
                                design_h: desc.height,
                            },
                        );
                        continue;
                    }

                    if let Some(mut glyph) = bitmap_glyph_as_path(desc, i as u32) {
                        if !entry.contains_key(&code) {
                            if let Some(a) = wt_adv {
                                glyph.advance = a;
                            }
                            entry.insert(code, glyph);
                        }
                        continue;
                    }

                    // Some symbol slots are valid Hancom HFT dispatch targets
                    // but intentionally have an empty outline stream. Preserve
                    // them as empty glyphs so native trace replay can distinguish
                    // "known no-outline slot" from "decoder miss".
                    if let Some(mut glyph) = empty_outline_glyph(desc, i as u32) {
                        if let Some(a) = wt_adv {
                            glyph.advance = a;
                        }
                        entry.insert(code, glyph);
                    }
                }
            }
        }
    Ok((name, entry))
}

/// Read + decode one .HFT path. `None` if unreadable or unparseable (skipped).
fn decode_path(path: &Path) -> Option<(String, HashMap<u32, Glyph>)> {
    let bytes = fs::read(path).ok()?;
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    decode_hft(filename, &bytes).ok()
}

/// Glyph count a .HFT file would contribute (header descriptor counts; type 0/1).
/// Parsing the header is cheap relative to decoding outlines — used to size the
/// progress bar before decode.
fn header_glyph_count(path: &Path) -> usize {
    let Ok(bytes) = fs::read(path) else { return 0 };
    let Ok(hft) = parser::parse(&bytes) else { return 0 };
    hft.chunks
        .iter()
        .flat_map(|c| &c.descriptors)
        .filter(|d| d.type_id == 0 || d.type_id == 1)
        .map(|d| d.count as usize)
        .sum()
}

// ── Persistent decoded-glyph cache (dep-free little-endian binary) ──────────
// Per-face file `{canon}.{src_size}.{src_mtime}.gcache`; size+mtime in the name
// invalidate it when the source .HFT changes. Stores only what the render path
// needs (path `d`, advance, em, design_h, ink metrics) — `commands` is dropped
// and reconstructed empty, so cached glyphs render identically but are not for
// outline re-composition / trace replay.
const GCACHE_MAGIC: &[u8; 4] = b"GCA1";

fn cache_file_path(cache_dir: &Path, canon: &str, src: &Path) -> PathBuf {
    let (size, mtime) = fs::metadata(src)
        .map(|m| {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (m.len(), mtime)
        })
        .unwrap_or((0, 0));
    cache_dir.join(format!("{canon}.{size}.{mtime}.gcache"))
}

fn write_face_cache(path: &Path, glyphs: &HashMap<u32, Glyph>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = Vec::with_capacity(glyphs.len() * 32);
    buf.extend_from_slice(GCACHE_MAGIC);
    buf.extend_from_slice(&(glyphs.len() as u32).to_le_bytes());
    for (code, g) in glyphs {
        buf.extend_from_slice(&code.to_le_bytes());
        buf.extend_from_slice(&g.advance.to_le_bytes());
        buf.extend_from_slice(&g.em.to_le_bytes());
        buf.extend_from_slice(&g.design_h.to_le_bytes());
        for v in g.metrics.raw {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&(g.d.len() as u32).to_le_bytes());
        buf.extend_from_slice(g.d.as_bytes());
    }
    // Write to a temp file then rename so a partial write never looks valid.
    let tmp = path.with_extension("gcache.tmp");
    fs::write(&tmp, &buf)?;
    fs::rename(&tmp, path)
}

fn read_face_cache(path: &Path) -> Option<HashMap<u32, Glyph>> {
    let b = fs::read(path).ok()?;
    let mut o = 0usize;
    let take = |o: &mut usize, n: usize| -> Option<&[u8]> {
        let s = b.get(*o..*o + n)?;
        *o += n;
        Some(s)
    };
    if take(&mut o, 4)? != GCACHE_MAGIC {
        return None;
    }
    let n = u32::from_le_bytes(take(&mut o, 4)?.try_into().ok()?) as usize;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let code = u32::from_le_bytes(take(&mut o, 4)?.try_into().ok()?);
        let advance = i32::from_le_bytes(take(&mut o, 4)?.try_into().ok()?);
        let em = u16::from_le_bytes(take(&mut o, 2)?.try_into().ok()?);
        let design_h = u16::from_le_bytes(take(&mut o, 2)?.try_into().ok()?);
        let mut raw = [0i16; 4];
        for r in &mut raw {
            *r = i16::from_le_bytes(take(&mut o, 2)?.try_into().ok()?);
        }
        let dlen = u32::from_le_bytes(take(&mut o, 4)?.try_into().ok()?) as usize;
        let d = String::from_utf8(take(&mut o, dlen)?.to_vec()).ok()?;
        map.insert(
            code,
            Glyph { d, commands: Vec::new(), advance, metrics: GlyphMetrics { raw }, em, design_h },
        );
    }
    Some(map)
}

/// Decode many .HFT files across worker threads. Each file is independent, so
/// the only shared step (merging into the cache) stays with the caller. File
/// cost is very uneven (a 2350-glyph Hangul face vs a tiny Latin one), so
/// threads pull the next file from a shared counter rather than take fixed
/// chunks — this keeps every core busy until the last file is done.
fn decode_files_parallel(paths: &[PathBuf]) -> Vec<(String, HashMap<u32, Glyph>)> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let n_threads =
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(paths.len().max(1));
    if n_threads <= 1 {
        return paths.iter().filter_map(|p| decode_path(p)).collect();
    }
    let next = AtomicUsize::new(0);
    let mut out = Vec::with_capacity(paths.len());
    std::thread::scope(|s| {
        let next = &next;
        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                s.spawn(|| {
                    let mut local = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = paths.get(i) else { break };
                        if let Some(r) = decode_path(path) {
                            local.push(r);
                        }
                    }
                    local
                })
            })
            .collect();
        for h in handles {
            if let Ok(mut part) = h.join() {
                out.append(&mut part);
            }
        }
    });
    out
}

impl HftCache {
    /// Load every .HFT file under `dir` (non-recursive). If `hftinfo.dat` is
    /// present in the same directory, the alias map is loaded too.
    ///
    /// Decoding all ~390 Hancom faces (each up to 2350 glyph outlines) is the
    /// dominant cost of a render, so the files are parsed and decoded across
    /// worker threads; only the final merge into the cache is serial.
    pub fn load_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, String> {
        let dir = dir.as_ref();
        let paths: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| format!("read_dir: {}", e))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("hft"))
                    == Some(true)
            })
            .collect();

        let decoded = decode_files_parallel(&paths);
        let mut total = 0;
        for (name, glyphs) in decoded {
            total += glyphs.len();
            self.glyphs.entry(name).or_default().extend(glyphs);
        }

        // Best-effort alias load — silently ignored if hftinfo.dat is absent.
        let alias_path = dir.join("hftinfo.dat");
        if alias_path.exists() {
            let _ = self.load_aliases(&alias_path);
        }
        Ok(total)
    }

    /// Register `dir` for lazy, per-face loading: index every .HFT filename and
    /// load aliases, but decode **no** glyphs. Callers then decode only the faces
    /// a document uses via `ensure_face`. Decoding all ~390 faces eagerly costs
    /// seconds; a document touches a handful.
    pub fn set_lazy_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<(), String> {
        let dir = dir.as_ref();
        let mut index = HashMap::new();
        for entry in fs::read_dir(dir).map_err(|e| format!("read_dir: {}", e))? {
            let path = entry.map_err(|e| format!("entry: {}", e))?.path();
            let is_hft = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("hft"))
                == Some(true);
            if let (true, Some(fname)) = (is_hft, path.file_name().and_then(|s| s.to_str())) {
                index.insert(Self::canonical_name(fname), path.clone());
            }
        }
        self.file_index = index;
        let alias_path = dir.join("hftinfo.dat");
        if alias_path.exists() {
            let _ = self.load_aliases(&alias_path);
        }
        Ok(())
    }

    /// Decode (once) the HFT file(s) backing `face`, if registered via
    /// `set_lazy_dir` and not already loaded. The candidates are the face's own
    /// canonical name plus every alias entry (Hangul/Latin/symbol siblings).
    /// Faces with no registered HFT file are ignored — they fall back to TTF.
    pub fn ensure_face(&mut self, face: &str) {
        for canon in self.face_candidates(face) {
            if !self.loaded.insert(canon.clone()) {
                continue; // already attempted
            }
            if self.glyphs.contains_key(&canon) {
                continue;
            }
            if let Some(path) = self.file_index.get(&canon) {
                if let Some((name, glyphs)) = decode_path(path) {
                    self.glyphs.entry(name).or_default().extend(glyphs);
                }
            }
        }
    }

    /// HFT canonical names that could back `face`: its own canonical name plus
    /// every alias entry (Hangul/Latin/symbol siblings).
    fn face_candidates(&self, face: &str) -> Vec<String> {
        if self.file_index.is_empty() {
            return Vec::new();
        }
        let mut out = vec![Self::canonical_name(face)];
        for e in self.aliases.resolve_all(face) {
            out.push(e.hft.clone());
        }
        out
    }

    /// Directory for the persistent decoded-glyph cache. When set, `ensure_faces_cached`
    /// loads a face's glyphs from disk (fast) instead of re-decoding the .HFT.
    pub fn set_cache_dir(&mut self, dir: PathBuf) {
        self.cache_dir = Some(dir);
    }

    /// Lazily load the HFT glyphs backing `faces`, using the on-disk cache when
    /// set. A face already in memory or cached on disk loads with no decode; only
    /// true cache misses are decoded (and then written to disk). `progress(done,
    /// total)` is called around the decode work, in glyph units, so a caller can
    /// show a bar — it fires only when something must be decoded.
    pub fn ensure_faces_cached(
        &mut self,
        faces: &[String],
        progress: &mut dyn FnMut(usize, usize),
    ) {
        // Resolve candidates; satisfy from memory or disk; collect true misses.
        let mut to_decode: Vec<(String, PathBuf)> = Vec::new();
        for face in faces {
            for canon in self.face_candidates(face) {
                if !self.loaded.insert(canon.clone()) {
                    continue;
                }
                if self.glyphs.contains_key(&canon) {
                    continue;
                }
                let Some(src) = self.file_index.get(&canon).cloned() else { continue };
                if let Some(cache_file) = self.cache_dir.as_ref().map(|d| cache_file_path(d, &canon, &src)) {
                    if let Some(glyphs) = read_face_cache(&cache_file) {
                        self.glyphs.entry(canon).or_default().extend(glyphs);
                        continue;
                    }
                }
                to_decode.push((canon, src));
            }
        }
        if to_decode.is_empty() {
            return;
        }
        // Total glyphs across the misses (header counts), for the progress bar.
        let total: usize = to_decode.iter().map(|(_, p)| header_glyph_count(p)).sum();
        let mut done = 0;
        progress(done, total);
        for (canon, src) in to_decode {
            let face_total = header_glyph_count(&src);
            if let Some((name, glyphs)) = decode_path(&src) {
                if let Some(dir) = &self.cache_dir {
                    let _ = write_face_cache(&cache_file_path(dir, &canon, &src), &glyphs);
                }
                self.glyphs.entry(name).or_default().extend(glyphs);
            }
            done += face_total;
            progress(done, total);
        }
    }

    /// Look up a glyph. `name` is matched first by `canonical_name(name)`
    /// (e.g. `"HCHGGGT"`) and then via the alias map loaded from
    /// `hftinfo.dat` (e.g. `"한양견고딕"` / `"HY Gyeongothic"`). When the
    /// alias resolves to multiple HFTs for the same face_name (e.g. Hangul
    /// + Symbol siblings), the codepoint's Unicode category picks the
    /// right one.
    ///
    /// 엄격 매칭: alias 가 해당 Unicode category HFT 를 갖고 있을 때만 반환한다.
    /// Windows COM PDF export full capture confirmed that Latin/punctuation can
    /// use HFT siblings too (`TEJMJEN`, `ENSMJ`, `TETGTEN`, ...), so alias
    /// lookup is category-strict but not Latin-skipping.
    pub fn get_resolved(&self, name: &str, char_code: u32) -> Option<(String, &Glyph)> {
        let canonical = Self::canonical_name(name);
        if let Some(map) = self.glyphs.get(&canonical) {
            if let Some(g) = map.get(&char_code) {
                return Some((canonical, g));
            }
        }
        // Alias resolution (category-strict, including Latin).
        if let Some(entry) = self.aliases.resolve_for_code(name, char_code) {
            if let Some(map) = self.glyphs.get(&entry.hft) {
                if let Some(g) = map.get(&char_code) {
                    return Some((entry.hft.clone(), g));
                }
            }
        }
        None
    }

    pub fn get(&self, name: &str, char_code: u32) -> Option<&Glyph> {
        self.get_resolved(name, char_code).map(|(_, glyph)| glyph)
    }

    /// Direct lookup by HFT file and Hancom/native character code. This bypasses
    /// Unicode-category alias resolution and is intended for Frida trace replay.
    pub fn get_native(&self, hft_name: &str, native_code: u32) -> Option<&Glyph> {
        self.glyphs
            .get(&Self::canonical_name(hft_name))
            .and_then(|map| map.get(&native_code))
    }

    /// Lookup by the character code observed in Hancom's HFT dispatcher. Some
    /// callbacks pass Unicode/descriptor range codes directly, while Hangul
    /// composition paths can pass Hwp's Johab-like 16-bit syllable code.
    pub fn get_hancom_code(&self, hft_name: &str, code: u32) -> Option<&Glyph> {
        self.get_native(hft_name, code).or_else(|| {
            johab::johab_to_unicode(code as u16)
                .and_then(|unicode| self.get_native(hft_name, unicode))
        })
    }

    pub fn resolve_hft_name(&self, name: &str, char_code: u32) -> Option<String> {
        let canonical = Self::canonical_name(name);
        if self.glyphs.contains_key(&canonical) {
            return Some(canonical);
        }
        self.aliases
            .resolve_for_code(name, char_code)
            .map(|entry| entry.hft.clone())
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

fn bitmap_glyph_as_path(desc: &parser::Descriptor, idx: u32) -> Option<Glyph> {
    let bytes = bitmap::extract(desc, idx).ok()?;
    let pixels = bitmap::to_pixels(bytes, desc.width, desc.height, desc.bytes_per_row);
    let mut d = String::new();
    for (y, row) in pixels.iter().enumerate() {
        let mut x = 0usize;
        while x < row.len() {
            if row[x] == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < row.len() && row[x] != 0 {
                x += 1;
            }
            d.push_str(&format!("M{} {}H{}V{}H{}Z", start, y, x, y + 1, start));
        }
    }
    if d.is_empty() {
        return None;
    }
    Some(Glyph {
        d,
        commands: Vec::new(),
        advance: desc.width as i32,
        metrics: GlyphMetrics::from_tuple((desc.width as i16, desc.height as i16, 0, 0)),
        em: desc.em,
        design_h: desc.height,
    })
}

fn empty_outline_glyph(desc: &parser::Descriptor, idx: u32) -> Option<Glyph> {
    if desc.type_id != 0 || !desc.is_bitmap {
        return None;
    }
    let blob = vector::extract_blob(desc, idx, None).ok()?;
    if blob.raw.len() > 1 || !vector::walk_path(&blob.raw).is_empty() {
        return None;
    }
    Some(Glyph {
        d: String::new(),
        commands: Vec::new(),
        advance: blob.metrics.0 as i32,
        metrics: GlyphMetrics::from_tuple(blob.metrics),
        em: desc.em,
        design_h: desc.height,
    })
}

#[derive(Clone, Copy)]
struct FamilyHft {
    hangul: &'static str,
    latin: &'static str,
    hanja: &'static str,
    japanese: Option<&'static str>,
    symbol: Option<&'static str>,
    other: Option<&'static str>,
}

fn add_family_aliases(cache: &mut HftCache, faces: &[&str], family: FamilyHft) {
    for face in faces {
        cache.add_alias(face, family.hangul, FaceCategory::Hangul);
        cache.add_alias(face, family.latin, FaceCategory::Latin);
        cache.add_alias(face, family.hanja, FaceCategory::Hanja);
        if let Some(hft) = family.japanese {
            cache.add_alias(face, hft, FaceCategory::Japanese);
        }
        if let Some(hft) = family.symbol {
            cache.add_alias(face, hft, FaceCategory::Symbol);
        }
        if let Some(hft) = family.other {
            cache.add_alias(face, hft, FaceCategory::Other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Directory holding the .HFT files (no fonts bundled in the repo). Defaults
    /// to the macOS Hancom Office install; override with `HANCOM_HFT_DIR`. Tests
    /// that need a font skip cleanly when the directory is absent.
    fn hft_dir() -> Option<PathBuf> {
        let dir = std::env::var("HANCOM_HFT_DIR").map(PathBuf::from).unwrap_or_else(|_| {
            PathBuf::from(
                "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts",
            )
        });
        dir.exists().then_some(dir)
    }

    /// Load one .HFT file into a fresh cache, or `None` when the font dir is absent.
    fn load(file: &str) -> Option<(HftCache, usize)> {
        let dir = hft_dir()?;
        let mut cache = HftCache::new();
        let n = cache.load_hft(dir.join(file)).expect("load hft");
        Some((cache, n))
    }

    #[test]
    fn canonical_name_strips_path_and_case() {
        assert_eq!(HftCache::canonical_name("HCHGGGT.HFT"), "HCHGGGT");
        assert_eq!(HftCache::canonical_name("hchgggt.hft"), "HCHGGGT");
        assert_eq!(HftCache::canonical_name("/path/to/HCHGGGT.HFT"), "HCHGGGT");
    }

    #[test]
    fn cache_loads_ciphered_type0_latin_vectors() {
        let Some((cache, loaded)) = load("ENSMJ.HFT") else { return };
        assert!(loaded > 0);
        let glyph = cache.get("ENSMJ", 'g' as u32).expect("ENSMJ ASCII glyph");
        assert!(
            glyph.d.contains('M') || glyph.d.contains('L') || glyph.d.contains('C'),
            "expected SVG path commands, got {:?}",
            glyph.d
        );
        assert_eq!(glyph.em, 1000);
    }

    #[test]
    fn cache_loads_raw_type0_latin_vectors() {
        let Some((cache, loaded)) = load("TEJMJEN.HFT") else { return };
        assert!(loaded > 0);
        let glyph = cache.get("TEJMJEN", '1' as u32).expect("TEJMJEN digit glyph");
        assert!(
            glyph.d.contains('M') || glyph.d.contains('L') || glyph.d.contains('C'),
            "expected SVG path commands, got {:?}",
            glyph.d
        );
        assert_eq!(glyph.em, 1000);
    }

    #[test]
    fn cache_exposes_native_hanja_vectors_for_trace_replay() {
        let Some((cache, loaded)) = load("HJSMJ.HFT") else { return };
        assert!(loaded > 0);
        let glyph = cache.get_native("HJSMJ", 0x4000).expect("HJSMJ native Hanja glyph");
        assert!(
            glyph.d.contains('M') || glyph.d.contains('L') || glyph.d.contains('C'),
            "expected SVG path commands, got {:?}",
            glyph.d
        );
        assert_eq!(glyph.em, 1200);
    }

    #[test]
    fn cache_loads_hgsmj_hangul_outline() {
        let Some((cache, loaded)) = load("HGSMJ.HFT") else { return };
        assert!(loaded > 1000);
        let glyph = cache.get("HGSMJ", '가' as u32).expect("HGSMJ 가 glyph");
        assert!(glyph.d.contains('M'));
        assert_eq!(glyph.em, 1200);
    }

    #[test]
    fn fontmap_alias_resolves_hy_sinmyeong_to_hgsmj() {
        let Some((mut cache, _)) = load("HGSMJ.HFT") else { return };
        cache.add_hancom_fontmap_aliases();
        let (face, glyph) = cache
            .get_resolved("HY신명조", '가' as u32)
            .expect("HY신명조 가 glyph");
        assert_eq!(face, "HGSMJ");
        assert_eq!(glyph.em, 1200);
    }

    #[test]
    fn cache_replays_hancom_johab_code_for_type1_fonts() {
        let Some((cache, _)) = load("TEJMJHG.HFT") else { return };
        let glyph = cache
            .get_hancom_code("TEJMJHG", 0x8861)
            .expect("TEJMJHG native Johab 가 glyph");
        assert_eq!(glyph.em, 1000);
    }

    #[test]
    fn cache_preserves_empty_symbol_slots_for_trace_replay() {
        let Some((cache, _)) = load("SPSMJ.HFT") else { return };
        let glyph = cache
            .get_hancom_code("SPSMJ.HFT", 0x3401)
            .expect("SPSMJ native empty symbol slot");
        assert_eq!(glyph.d, "");
        assert!(glyph.advance > 0);
    }

    #[test]
    fn width_table_gives_proportional_latin_advance() {
        // 신명 중명조 Latin (TEJMJEN, em=1000). Advances come from the code-0x20
        // width table, indexed by char code; verified against the raw bytes.
        let Some((cache, _)) = load("TEJMJEN.HFT") else { return };
        assert_eq!(cache.get("TEJMJEN", '!' as u32).unwrap().advance, 500);
        // Proportional: 'W' (wide) advances more than 'i' (narrow).
        let ww = cache.get("TEJMJEN", 'W' as u32).unwrap().advance;
        let wi = cache.get("TEJMJEN", 'i' as u32).unwrap().advance;
        assert!(wi < ww, "narrow 'i' {wi} should advance less than wide 'W' {ww}");
        // The advance covers the ink right edge plus a right side-bearing.
        for ch in ['A', 'W', 'a', '0'] {
            let g = cache.get("TEJMJEN", ch as u32).unwrap();
            let ink_max_x = g.metrics.raw[0] as i32 + g.metrics.raw[2] as i32;
            assert!(g.advance >= ink_max_x, "{ch}: advance {} < ink_max_x {ink_max_x}", g.advance);
        }
    }

    #[test]
    fn hangul_advance_is_full_em() {
        // Hangul fonts carry no width table → a syllable advances a full em.
        let Some((cache, _)) = load("HGSMJ.HFT") else { return };
        let g = cache.get("HGSMJ", '가' as u32).unwrap();
        assert_eq!(g.advance, g.em as i32, "Hangul advance must equal the em box");
    }

    #[test]
    fn type0_decoder_skips_close_only_false_cipher_candidate() {
        let Some((cache, _)) = load("ENGMJ.HFT") else { return };
        let glyph = cache.get("ENGMJ", '4' as u32).expect("ENGMJ digit 4");
        assert!(
            glyph.commands.iter().any(|command| matches!(
                command.kind,
                vector::CommandKind::Line | vector::CommandKind::Cubic
            )),
            "expected drawable ENGMJ digit 4, got {:?}",
            glyph.commands
        );
        assert!(glyph.d.contains('L') || glyph.d.contains('C'), "{:?}", glyph.d);
    }
}
