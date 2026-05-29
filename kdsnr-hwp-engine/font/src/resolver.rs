//! face name → concrete TTF. See `docs/FONT_MODEL.md` §3.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::fontmap::FontMap;
use crate::hftinfo::Substitution;
use crate::script::{script_of, Script};
use crate::ttf::TtfFont;

/// Concrete TTFs for one family name: Regular and (if present) Bold.
#[derive(Clone)]
struct FamilyEntry {
    regular: PathBuf,
    bold: Option<PathBuf>,
}

pub struct FontResolver {
    subst: Substitution,
    /// Hancom face→font fallback map (`FontMap.dat`), empty when absent.
    fontmap: FontMap,
    /// family-name(lower) → Regular/Bold TTF paths.
    families: HashMap<String, FamilyEntry>,
    cache: RefCell<HashMap<PathBuf, Rc<TtfFont>>>,
    /// face+script → resolved family entry (or None), memoized.
    resolved: RefCell<HashMap<(String, Script), Option<FamilyEntry>>>,
    /// Decoded Hancom HFT glyphs (outline + advance) for HFT-typed faces. Empty
    /// until `load_hft_dir` is called; HFT-typed chars fall back to TTF when so.
    hft: kdsnr_hwp_hft::HftCache,
}

/// hftinfo generic family → concrete TTF family-name candidates.
fn generic_candidates(family: &str) -> Vec<String> {
    let f = family.to_lowercase();
    match f.as_str() {
        "batang" => vec!["한컴바탕".into(), "haansoft batang".into(), "바탕".into()],
        "dotum" => vec!["한컴돋움".into(), "haansoft dotum".into(), "돋움".into()],
        "gulim" => vec!["굴림".into(), "gulim".into()],
        "gungsuh" => vec!["궁서".into(), "gungsuh".into()],
        _ => vec![f],
    }
}

impl FontResolver {
    pub fn new(font_dir: &Path, hftinfo_path: &Path) -> Result<Self, String> {
        Self::with_dirs(&[font_dir], hftinfo_path, &[])
    }

    /// Build from several font roots (scanned and merged) plus the hftinfo
    /// substitution table and zero or more `FontMap.dat`-format fallback maps
    /// (merged in order; later maps supplement earlier ones).
    pub fn with_dirs(
        font_dirs: &[&Path],
        hftinfo_path: &Path,
        fontmap_paths: &[&Path],
    ) -> Result<Self, String> {
        let subst = Substitution::load_path_or_builtin(hftinfo_path);
        let mut fontmap = FontMap::builtin();
        for p in fontmap_paths {
            if let Ok(m) = FontMap::load_path(p) {
                fontmap.merge(m);
            }
        }
        let mut families = HashMap::new();
        for d in font_dirs {
            merge_families(&mut families, scan_families(d));
        }
        Ok(Self {
            subst,
            fontmap,
            families,
            cache: RefCell::new(HashMap::new()),
            resolved: RefCell::new(HashMap::new()),
            hft: kdsnr_hwp_hft::HftCache::new(),
        })
    }

    /// Load Hancom HFT fonts from `dir` (all `.HFT` + `hftinfo.dat` aliases) so
    /// HFT-typed faces resolve their own outline + advance. Returns the glyph
    /// count loaded. Safe to skip — HFT chars then fall back to TTF.
    pub fn load_hft_dir(&mut self, dir: &Path) -> Result<usize, String> {
        let n = self.hft.load_dir(dir)?;
        self.hft.add_hancom_fontmap_aliases();
        Ok(n)
    }

    /// Register an HFT directory for lazy loading: index filenames + aliases, but
    /// decode no glyphs. Call `ensure_hft_faces` with a document's faces before
    /// rendering it. Avoids decoding all ~390 faces when a document uses a few.
    pub fn set_hft_dir_lazy(&mut self, dir: &Path) -> Result<(), String> {
        self.hft.set_lazy_dir(dir)?;
        self.hft.add_hancom_fontmap_aliases();
        Ok(())
    }

    /// Decode (once) the HFT files backing each face. No-op for faces without an
    /// HFT file (they fall back to TTF). Pairs with `set_hft_dir_lazy`.
    pub fn ensure_hft_faces<I: IntoIterator<Item = String>>(&mut self, faces: I) {
        for f in faces {
            self.hft.ensure_face(&f);
        }
    }

    /// Persistent decoded-glyph cache directory: a face decoded once is reused by
    /// later processes (no re-decode). Pairs with `ensure_hft_faces_with_progress`.
    pub fn set_glyph_cache_dir(&mut self, dir: std::path::PathBuf) {
        self.hft.set_cache_dir(dir);
    }

    /// Like `ensure_hft_faces` but uses the on-disk glyph cache and reports decode
    /// progress (glyph units) via `progress(done, total)`. The callback fires only
    /// when faces must actually be decoded (cold cache).
    pub fn ensure_hft_faces_with_progress(
        &mut self,
        faces: &[String],
        progress: &mut dyn FnMut(usize, usize),
    ) {
        self.hft.ensure_faces_cached(faces, progress);
    }

    /// HFT glyph advance for `face`/`ch` as an em fraction (advance ÷ em-box), or
    /// `None` when no HFT font carries the glyph (caller falls back to TTF).
    pub fn hft_advance_em(&self, face: &str, ch: char) -> Option<f64> {
        self.hft
            .get(face, ch as u32)
            .filter(|g| g.em != 0)
            .map(|g| g.advance as f64 / g.em as f64)
    }

    /// HFT glyph outline for `face`/`ch`: the SVG path `d` (design-box
    /// coordinates, y-up, baseline at the font descent above the box bottom) plus
    /// the design-box height to scale by (`size / design_h`). `None` when no HFT
    /// font carries a non-empty outline (caller falls back to the TTF substitute).
    /// This is Hancom's own glyph shape — used for HFT-typed faces (신명/한양/…
    /// body text, 신그래픽/태고딕 display titles) so they render in the real face
    /// instead of a class substitute.
    pub fn hft_outline(&self, face: &str, ch: char) -> Option<(String, u16)> {
        self.hft
            .get(face, ch as u32)
            .filter(|g| g.design_h != 0 && !g.d.is_empty())
            .map(|g| (g.d.clone(), g.design_h))
    }

    /// Whether any HFT fonts are loaded.
    pub fn has_hft(&self) -> bool {
        self.hft.glyph_count() > 0
    }

    pub fn family_count(&self) -> usize {
        self.families.len()
    }

    /// Resolved Regular TTF path for `face`/`script` (diagnostics).
    pub fn debug_resolve_path(&self, face: &str, script: Script) -> Option<PathBuf> {
        self.resolve_entry(face, script).map(|e| e.regular)
    }

    fn lookup_family(&self, name: &str) -> Option<FamilyEntry> {
        self.families.get(&name.to_lowercase()).cloned()
    }

    /// Resolve a document face name (for `ch`'s script) to a family entry.
    fn resolve_entry(&self, face: &str, script: Script) -> Option<FamilyEntry> {
        let key = (face.to_string(), script);
        if let Some(hit) = self.resolved.borrow().get(&key) {
            return hit.clone();
        }
        let e = self.resolve_entry_uncached(face, script);
        self.resolved.borrow_mut().insert(key, e.clone());
        e
    }

    fn resolve_entry_uncached(&self, face: &str, script: Script) -> Option<FamilyEntry> {
        // 1. TTF-native.
        if let Some(e) = self.lookup_family(face) {
            return Some(e);
        }
        // 2. FontMap.dat fallback: first installed font in the face's candidate
        // chain (the face's own name is candidate 0, already tried above). This
        // matches Hancom's resolution order: e.g. 한양신명조 → [HY신명조, 한양신명조,
        // 신명조, BATANG class(Batang, 바탕, BatangChe, …)]. With MS Batang present,
        // the proportional "Batang" (the font Hancom's GT embeds) wins over the
        // fixed-width BatangChe that follows it.
        for cand in self.fontmap.candidates(face).into_iter().skip(1) {
            if let Some(e) = self.lookup_family(&cand) {
                return Some(e);
            }
        }
        // 3. Substitute via hftinfo Family Category (per script).
        let fam = self
            .subst
            .substitute(face, script)
            .or_else(|| {
                // 4. Bare name → retry with "신명 " prefix.
                if !face.starts_with("신명") && !face.starts_with("한양") {
                    self.subst.substitute(&format!("신명 {face}"), script)
                } else {
                    None
                }
            });
        if let Some(fam) = fam {
            for cand in generic_candidates(fam) {
                if let Some(e) = self.lookup_family(&cand) {
                    return Some(e);
                }
            }
        }
        None
    }

    fn font_at(&self, path: &Path) -> Option<Rc<TtfFont>> {
        if let Some(f) = self.cache.borrow().get(path) {
            return Some(f.clone());
        }
        let f = Rc::new(TtfFont::load(path).ok()?);
        self.cache.borrow_mut().insert(path.to_path_buf(), f.clone());
        Some(f)
    }

    /// Resolve the Regular TTF for `face` as used to draw `ch`.
    pub fn resolve(&self, face: &str, ch: char) -> Option<Rc<TtfFont>> {
        let e = self.resolve_entry(face, script_of(ch))?;
        self.font_at(&e.regular)
    }

    /// Resolve a face for a script without a specific char (diagnostics).
    pub fn resolve_face(&self, face: &str, script: Script) -> Option<Rc<TtfFont>> {
        let e = self.resolve_entry(face, script)?;
        self.font_at(&e.regular)
    }

    /// Resolve the TTF for `face`/`ch` honoring `bold`. Returns the font and
    /// whether bold must be synthesized (no real Bold variant in the family).
    pub fn resolve_styled(&self, face: &str, ch: char, bold: bool) -> Option<(Rc<TtfFont>, bool)> {
        let e = self.resolve_entry(face, script_of(ch))?;
        match (bold, &e.bold) {
            (true, Some(b)) => Some((self.font_at(b)?, false)),
            (true, None) => Some((self.font_at(&e.regular)?, true)),
            (false, _) => Some((self.font_at(&e.regular)?, false)),
        }
    }

    fn font_styled(&self, e: &FamilyEntry, bold: bool) -> Option<(Rc<TtfFont>, bool)> {
        match (bold, &e.bold) {
            (true, Some(b)) => Some((self.font_at(b)?, false)),
            (true, None) => Some((self.font_at(&e.regular)?, true)),
            (false, _) => Some((self.font_at(&e.regular)?, false)),
        }
    }

    /// Resolve the font that actually carries `ch`'s glyph. The per-script font
    /// is tried first; when it lacks the glyph (e.g. a symbol slot mapped to a
    /// Latin face for a CJK marker) fall back to the face's CJK/Latin fonts,
    /// matching GDI glyph fallback. Returns the font and bold-synthesis flag.
    pub fn resolve_glyph(&self, face: &str, ch: char, bold: bool) -> Option<(Rc<TtfFont>, bool)> {
        let mut scripts = vec![script_of(ch)];
        for s in [Script::Hangul, Script::Latin] {
            if !scripts.contains(&s) {
                scripts.push(s);
            }
        }
        let mut first: Option<(Rc<TtfFont>, bool)> = None;
        for sc in scripts {
            if let Some(e) = self.resolve_entry(face, sc) {
                if let Some((f, faux)) = self.font_styled(&e, bold) {
                    if f.advance_em(ch).is_some() {
                        return Some((f, faux));
                    }
                    first.get_or_insert((f, faux));
                }
            }
        }
        first
    }
}

/// Merge a scanned family map into `dst`; existing families (earlier dirs) win,
/// so the primary font dir takes precedence over later fallback dirs.
fn merge_families(dst: &mut HashMap<String, FamilyEntry>, src: HashMap<String, FamilyEntry>) {
    for (k, v) in src {
        dst.entry(k).or_insert(v);
    }
}

fn scan_families(dir: &Path) -> HashMap<String, FamilyEntry> {
    let mut fam: HashMap<String, FamilyEntry> = HashMap::new();
    let mut regular: HashMap<String, bool> = HashMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for ent in rd.flatten() {
            let p = ent.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());
            if !matches!(ext.as_deref(), Some("ttf") | Some("otf") | Some("ttc")) {
                continue;
            }
            let Ok(font) = TtfFont::load(&p) else { continue };
            let (is_reg, is_bold) = (font.is_regular(), font.is_bold());
            for nm in font.family_names() {
                let seen = fam.contains_key(&nm);
                let e = fam.entry(nm.clone()).or_insert_with(|| FamilyEntry {
                    regular: p.clone(),
                    bold: None,
                });
                if is_bold {
                    e.bold.get_or_insert_with(|| p.clone());
                }
                // Regular slot: take first seen, then upgrade to a true Regular.
                let prev_reg = *regular.get(&nm).unwrap_or(&false);
                if !seen || (is_reg && !prev_reg) {
                    e.regular = p.clone();
                    regular.insert(nm, is_reg);
                }
            }
        }
    }
    fam
}
