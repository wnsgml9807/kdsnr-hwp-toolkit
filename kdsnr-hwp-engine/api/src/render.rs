//! Preview rendering: `Document` → paginated pages → SVG/PNG/PDF files.
//!
//! Chain: `normalize` → `measure_document` → `paginate_document` → `lower` →
//! `page_to_svg`. The engine SVG is self-contained (glyphs as `<path>`, images
//! as data URIs), so PNG (resvg) and PDF (svg2pdf) convert without a font DB.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdsnr_hwp_doc::normalize;
use kdsnr_hwp_font::FontResolver;
use kdsnr_hwp_layout::{measure_document, paginate_document};
use kdsnr_hwp_paint::{lower, PaintDocument, PaintPage};
use kdsnr_hwp_parser::model::document::Document;
use kdsnr_hwp_render::{page_body_svg, page_to_svg};

use crate::{split_set_to_question, ApiError};

/// Output formats for a rendered page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Svg,
    Png,
    Pdf,
}

impl MediaType {
    pub fn ext(self) -> &'static str {
        match self {
            MediaType::Svg => "svg",
            MediaType::Png => "png",
            MediaType::Pdf => "pdf",
        }
    }
}

/// What each rendered sheet represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewType {
    /// One sheet per laid-out page.
    Page,
    /// Split the set into questions first, then render each question's pages.
    Question,
}

impl PreviewType {
    /// Filename kind tag: `p` for pages, `q` for questions.
    fn kind(self) -> char {
        match self {
            PreviewType::Page => 'p',
            PreviewType::Question => 'q',
        }
    }
}

/// Build the font resolver. Substitute fonts + maps come from `FONT_DIR` (the
/// deploy path), falling back to the in-tree `.fonts` isolate for local dev.
/// HFT-typed faces load their outlines from the Hancom install (`HANCOM_DIR`)
/// when present; otherwise they fall back to TTF substitutes.
pub fn build_resolver() -> Result<FontResolver, ApiError> {
    let fonts = font_dir()
        .ok_or_else(|| ApiError::Render("폰트 폴더를 찾을 수 없습니다 (FONT_DIR 설정 필요).".into()))?;
    let hftinfo = fonts.join("hftinfo.dat");
    let fontmap = fonts.join("FontMap.dat");
    let extra_map = fonts.join("extra_fontmap.ini");
    let mut maps: Vec<&Path> = Vec::new();
    if fontmap.exists() {
        maps.push(&fontmap);
    }
    if extra_map.exists() {
        maps.push(&extra_map);
    }
    let mut r = FontResolver::with_dirs(&[&fonts], &hftinfo, &maps).map_err(ApiError::Render)?;
    let hft_dir = hancom_dir().join("Fonts");
    if hft_dir.exists() {
        // Lazy: index files + aliases now, decode per-face at render time.
        let _ = r.set_hft_dir_lazy(&hft_dir);
        if let Some(cache) = glyph_cache_dir() {
            r.set_glyph_cache_dir(cache);
        }
    }
    Ok(r)
}

/// Hancom Office shared resource root (`HANCOM_PATH`), the source for HFT outlines
/// and font collection. Distinct from `FONT_DIR` (the managed font folder).
/// Defaults to the per-OS install path.
pub(crate) fn hancom_dir() -> PathBuf {
    std::env::var("HANCOM_PATH").map(PathBuf::from).unwrap_or_else(|_| {
        match std::env::consts::OS {
            "macos" => PathBuf::from(
                "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared",
            ),
            "windows" => PathBuf::from("C:/Program Files (x86)/Hnc/Office/Shared"),
            _ => PathBuf::new(),
        }
    })
}

/// Persistent decoded-glyph cache directory: `GLYPH_CACHE_DIR`, else the per-OS
/// user cache directory. Decoded faces survive across processes. Deliberately a
/// user-writable cache (not the installed package): site-packages is often
/// read-only and is wiped on upgrade.
fn glyph_cache_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("GLYPH_CACHE_DIR") {
        return Some(PathBuf::from(d));
    }
    let sub = "kdsnr-hwp-toolkit/glyphcache";
    match std::env::consts::OS {
        "windows" => std::env::var("LOCALAPPDATA").ok().map(|p| PathBuf::from(p).join(sub)),
        "macos" => {
            std::env::var("HOME").ok().map(|h| PathBuf::from(h).join("Library/Caches").join(sub))
        }
        _ => std::env::var("XDG_CACHE_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".cache")))
            .map(|c| c.join(sub)),
    }
}

thread_local! {
    /// Per-thread cached resolver, built on first use and reused across calls.
    /// HFT faces load lazily (per document) and accumulate here, so each face is
    /// decoded at most once per thread for the whole process. `RefCell` allows the
    /// per-document `ensure_hft_faces` mutation.
    static RESOLVER: RefCell<Option<Rc<RefCell<FontResolver>>>> = const { RefCell::new(None) };
}

/// The shared resolver for this thread, built (lazily registered) on first use.
fn cached_resolver() -> Result<Rc<RefCell<FontResolver>>, ApiError> {
    RESOLVER.with(|cell| {
        if let Some(r) = cell.borrow().as_ref() {
            return Ok(r.clone());
        }
        let r = Rc::new(RefCell::new(build_resolver()?));
        *cell.borrow_mut() = Some(r.clone());
        Ok(r)
    })
}

pub(crate) fn font_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FONT_DIR") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.fonts");
    dev.exists().then_some(dev)
}

/// Run the layout chain and lower a document to paint pages, decoding the HFT
/// faces this document uses into the (lazy, disk-cached) resolver first.
/// `progress(done, total)` reports glyph-decode progress on a cold cache.
fn paint(
    doc: &Document,
    resolver: &Rc<RefCell<FontResolver>>,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<PaintDocument, ApiError> {
    let model = normalize(doc);
    let faces: Vec<String> = kdsnr_hwp_doc::required_faces(&model).into_iter().collect();
    resolver.borrow_mut().ensure_hft_faces_with_progress(&faces, progress);
    let measured = measure_document(&model);
    let pagination = paginate_document(&measured).map_err(|e| ApiError::Render(format!("{e:?}")))?;
    Ok(lower(&model, &pagination))
}

/// Rasterize an engine SVG to PNG at `scale` (1.0 == the SVG's pixel size).
fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, ApiError> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| ApiError::Render(format!("svg parse: {e}")))?;
    let size = tree.size();
    let w = (size.width() * scale).ceil().max(1.0) as u32;
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| ApiError::Render(format!("pixmap alloc {w}x{h}")))?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| ApiError::Render(format!("png encode: {e}")))
}

/// Convert an engine SVG to a single-page PDF.
fn svg_to_pdf(svg: &str) -> Result<Vec<u8>, ApiError> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| ApiError::Render(format!("svg parse: {e}")))?;
    svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default())
        .map_err(|e| ApiError::Render(format!("pdf convert: {e}")))
}

/// DPI for the crop probe/raster; user units == pixels at this DPI.
const CROP_DPI: f64 = 96.0;
/// Uniform margin (points) added around the body ink box in a question crop.
const CROP_MARGIN_PT: f64 = 12.0;

/// Body ink bounding box in SVG user units `(x, y, w, h)`, or `None` if the page
/// body draws no ink. Rasterizes the body-only (transparent) SVG and scans for
/// non-transparent pixels, so the box hugs the actual glyph/object extent and
/// excludes page furniture and background.
fn body_ink_bbox(page: &PaintPage, fonts: &FontResolver) -> Result<Option<(f64, f64, f64, f64)>, ApiError> {
    let w = page.paper.width.raw() as f64 * CROP_DPI / 7200.0;
    let h = page.paper.height.raw() as f64 * CROP_DPI / 7200.0;
    let (pw, ph) = (w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32);
    let svg = page_body_svg(page, CROP_DPI, fonts, (0.0, 0.0, w, h), false);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|e| ApiError::Render(format!("svg parse: {e}")))?;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| ApiError::Render(format!("pixmap alloc {pw}x{ph}")))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let data = pixmap.data();
    let (mut x0, mut y0, mut x1, mut y1) = (pw, ph, 0u32, 0u32);
    let mut any = false;
    for y in 0..ph {
        for x in 0..pw {
            // Ignore near-transparent anti-alias fringe.
            if data[((y * pw + x) * 4 + 3) as usize] > 8 {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    Ok(any.then(|| (x0 as f64, y0 as f64, (x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64)))
}

/// Body content cropped to its ink box plus a uniform margin, as an SVG with a
/// white background. `None` when the page body is blank (skip the sheet).
fn crop_page_svg(page: &PaintPage, fonts: &FontResolver) -> Result<Option<String>, ApiError> {
    let pw = page.paper.width.raw() as f64 * CROP_DPI / 7200.0;
    let ph = page.paper.height.raw() as f64 * CROP_DPI / 7200.0;
    let Some((bx, by, bw, bh)) = body_ink_bbox(page, fonts)? else {
        return Ok(None);
    };
    let m = CROP_MARGIN_PT * CROP_DPI / 72.0;
    let x = (bx - m).max(0.0);
    let y = (by - m).max(0.0);
    let cw = (bx + bw + m).min(pw) - x;
    let ch = (by + bh + m).min(ph) - y;
    Ok(Some(page_body_svg(page, CROP_DPI, fonts, (x, y, cw, ch), true)))
}

/// Write one page in the requested media type to `save_path`, returning the path.
/// Filename: `{stem}_{kind}{index:02}.{ext}`.
fn write_page(
    svg: &str,
    media: MediaType,
    scale: f32,
    save_path: &Path,
    stem: &str,
    kind: char,
    index: usize,
) -> Result<PathBuf, ApiError> {
    let name = format!("{stem}_{kind}{index:02}.{}", media.ext());
    let path = save_path.join(name);
    let bytes = match media {
        MediaType::Svg => svg.as_bytes().to_vec(),
        MediaType::Png => svg_to_png(svg, scale)?,
        MediaType::Pdf => svg_to_pdf(svg)?,
    };
    std::fs::write(&path, &bytes).map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))?;
    Ok(path)
}

/// Render previews of `docs` to files under `save_path`, one sheet per page (or
/// per question's pages when `preview_type == Question`). Each doc is paired with
/// a filename stem.
///
/// Returns paths grouped by media type: the outer list follows `media_types`
/// order, each inner list holds every path for that extension (all docs and
/// pages, flattened, in render order).
pub fn export_preview(
    docs: &[(String, Document)],
    save_path: &Path,
    preview_type: PreviewType,
    media_types: &[MediaType],
    scale: f32,
    glyph_progress: &mut dyn FnMut(usize, usize),
    render_progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<Vec<PathBuf>>, ApiError> {
    // Font gate: never render with a missing/wrong face. Any caller (even without
    // the Python collection pre-step) fails here when a required font is absent.
    let docs_only: Vec<Document> = docs.iter().map(|(_, d)| d.clone()).collect();
    let missing = crate::fonts::missing_fonts(&docs_only);
    if !missing.is_empty() {
        return Err(ApiError::FontsMissing(missing));
    }

    std::fs::create_dir_all(save_path)
        .map_err(|e| ApiError::Io(format!("{}: {e}", save_path.display())))?;
    let resolver = cached_resolver()?;
    let mut out: Vec<Vec<PathBuf>> = media_types.iter().map(|_| Vec::new()).collect();
    let kind = preview_type.kind();

    // First pass: paint every sheet (decodes glyphs, with the glyph-cache bar).
    let mut painted_docs: Vec<(&str, Vec<PaintDocument>)> = Vec::new();
    for (stem, doc) in docs {
        // In question mode each split unit renders under `{stem}_q01..`; in page
        // mode the whole doc renders under `{stem}_p01..`.
        let sheets: Vec<PaintDocument> = match preview_type {
            PreviewType::Page => vec![paint(doc, &resolver, glyph_progress)?],
            PreviewType::Question => split_set_to_question(doc)?
                .iter()
                .map(|(_label, qdoc)| paint(qdoc, &resolver, glyph_progress))
                .collect::<Result<_, _>>()?,
        };
        painted_docs.push((stem.as_str(), sheets));
    }

    // Second pass: render each page to the requested media (with the render bar).
    let total_pages: usize = painted_docs.iter().flat_map(|(_, s)| s).map(|p| p.pages.len()).sum();
    let mut done = 0;
    render_progress(done, total_pages);
    let fonts = resolver.borrow();
    for (stem, sheets) in &painted_docs {
        let mut index = 1;
        for painted in sheets {
            for page in &painted.pages {
                // Page mode renders the whole sheet; question mode crops to the
                // body ink box (+margin), skipping a sheet with no body content.
                let svg = match preview_type {
                    PreviewType::Page => Some(page_to_svg(page, 96.0, &fonts)),
                    PreviewType::Question => crop_page_svg(page, &fonts)?,
                };
                done += 1;
                render_progress(done, total_pages);
                let Some(svg) = svg else { continue };
                for (slot, &media) in media_types.iter().enumerate() {
                    let p = write_page(&svg, media, scale, save_path, stem, kind, index)?;
                    out[slot].push(p);
                }
                index += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_file;

    fn original(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templet/original").join(name)
    }

    #[test]
    fn exports_pages_grouped_by_media() {
        if build_resolver().is_err() {
            eprintln!("skip: fonts not present");
            return;
        }
        let (doc, _) = import_file(&original("math_input_sample_2.hwpx")).expect("import");
        let dir = std::env::temp_dir().join("kdsnr_api_preview_test");
        let _ = std::fs::remove_dir_all(&dir);
        let out = export_preview(
            &[("set".into(), doc)],
            &dir,
            PreviewType::Page,
            &[MediaType::Svg, MediaType::Png],
            1.5,
            &mut |_d, _t| {},
            &mut |_d, _t| {},
        )
        .expect("export");
        assert_eq!(out.len(), 2, "two media groups");
        assert!(!out[0].is_empty(), "svg paths");
        assert_eq!(out[0].len(), out[1].len(), "one png per svg");
        for p in out.iter().flatten() {
            assert!(p.exists(), "wrote {}", p.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
