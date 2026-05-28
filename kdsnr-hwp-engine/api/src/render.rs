//! Preview rendering: `Document` → paginated pages → SVG/PNG/PDF files.
//!
//! Chain: `normalize` → `measure_document` → `paginate_document` → `lower` →
//! `page_to_svg`. The engine SVG is self-contained (glyphs as `<path>`, images
//! as data URIs), so PNG (resvg) and PDF (svg2pdf) convert without a font DB.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kdsnr_hwp_core::{Rect, SectionId, SourceRef};
use kdsnr_hwp_doc::normalize;
use kdsnr_hwp_font::FontResolver;
use kdsnr_hwp_layout::{measure_document, paginate_document};
use kdsnr_hwp_paint::{lower, PaintDocument, PaintOp, PaintPage};
use kdsnr_hwp_parser::model::document::Document;
use kdsnr_hwp_parser::{detect_units_auto, DetectedUnit};
use kdsnr_hwp_render::{ops_svg, ops_svg_nested, page_to_svg};

use crate::ApiError;

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
            // Hancom Office installs under `Hnc\Office <year>\HOffice<ver>\Shared`
            // (e.g. Office 2024 → HOffice130). Discover it; fall back to the
            // legacy layout if nothing matches.
            "windows" => find_windows_hancom_shared()
                .unwrap_or_else(|| PathBuf::from("C:/Program Files (x86)/Hnc/Office/Shared")),
            _ => PathBuf::new(),
        }
    })
}

/// Locate `…\Hnc\Office*\HOffice*\Shared` under the common Program Files roots.
#[cfg(target_os = "windows")]
fn find_windows_hancom_shared() -> Option<PathBuf> {
    for root in [
        "C:/Program Files (x86)/Hnc",
        "C:/Program Files/Hnc",
    ] {
        let Ok(offices) = std::fs::read_dir(root) else { continue };
        for office in offices.flatten().filter(|e| e.path().is_dir()) {
            let Ok(versions) = std::fs::read_dir(office.path()) else { continue };
            for ver in versions.flatten() {
                let shared = ver.path().join("Shared");
                if shared.is_dir() {
                    return Some(shared);
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn find_windows_hancom_shared() -> Option<PathBuf> {
    None
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
    // `doc` is already memo-stripped by the caller (so memos never render and so
    // unit detection and the painted pages share one paragraph indexing).
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
/// Uniform blank-white margin (points) padded around a unit's cropped ink.
const CROP_MARGIN_PT: f64 = 18.0;
/// Sourceless ops (borders, fills, images, lines carry no paragraph source) join
/// a unit's column fragment when their vertical centre falls within this slack of
/// the unit's text span there — enough to catch a border box or figure hugging
/// the text, without reaching into a neighbouring unit.
const SOURCELESS_PAD: i32 = 700; // HWPUNIT (~0.1 inch)

/// HWPUNIT → crop user units (1/7200 inch → px at `CROP_DPI`).
fn u(hwp: i32) -> f64 {
    hwp as f64 * CROP_DPI / 7200.0
}

/// Trim a float to a compact SVG coordinate string.
fn n(v: f64) -> String {
    format!("{v:.3}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// One contiguous fragment of a crop unit: the unit's paint ops that land on one
/// page+column, in original paint order (so fills stay behind text and borders on
/// top). Absolute page coords.
struct Frag {
    page: usize,
    col: usize,
    ops: Vec<PaintOp>,
}

/// A detected crop unit — one question (math/science/social) or one Korean set —
/// as the ordered fragments it spans across columns and pages.
struct CropUnit {
    frags: Vec<Frag>,
}

/// Ink bounding box (absolute crop user units `(x, y, w, h)`) of `ops`, or `None`
/// if they draw no ink. Renders only `ops` onto a transparent page-sized canvas
/// and scans for non-transparent pixels, so the box hugs the unit's own ink and
/// nothing outside the selected ops can widen it.
fn ops_ink_bbox(
    ops: &[PaintOp],
    page: &PaintPage,
    fonts: &FontResolver,
) -> Result<Option<(f64, f64, f64, f64)>, ApiError> {
    let view = (0.0, 0.0, u(page.paper.width.raw()), u(page.paper.height.raw()));
    let (pw, ph) = (view.2.ceil().max(1.0) as u32, view.3.ceil().max(1.0) as u32);
    let svg = ops_svg(CROP_DPI, fonts, ops, view, false);
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

/// Numeric layout box (crop user-unit corners `(x0, y0, x1, y1)`) of `ops` from
/// their stored geometry — text lines as their full line box (`top..top+line_height`),
/// other ops as their rect/segment. Unlike the pixel ink box this keeps each line's
/// leading, so stacking fragments by these edges reproduces the document's line
/// spacing exactly at a column/page seam (baseline-to-baseline = one line height).
fn ops_layout_box(ops: &[PaintOp]) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let mut any = false;
    for op in ops {
        let (ax0, ay0, ax1, ay1) = match op {
            PaintOp::TextLine(l) => (l.x, l.top, l.x + l.seg_width, l.top + l.line_height),
            PaintOp::FillRect { rect, .. }
            | PaintOp::StrokeRect { rect, .. }
            | PaintOp::Image { rect, .. } => (
                rect.x.raw(),
                rect.y.raw(),
                rect.x.raw() + rect.width.raw(),
                rect.y.raw() + rect.height.raw(),
            ),
            PaintOp::Line { x1, y1, x2, y2, .. } => {
                ((*x1).min(*x2), (*y1).min(*y2), (*x1).max(*x2), (*y1).max(*y2))
            }
        };
        x0 = x0.min(ax0);
        y0 = y0.min(ay0);
        x1 = x1.max(ax1);
        y1 = y1.max(ay1);
        any = true;
    }
    any.then(|| (u(x0), u(y0), u(x1), u(y1)))
}

/// Page body columns, left to right (HWPUNIT). Falls back to the whole page when
/// the page carries no explicit columns.
fn page_columns(page: &PaintPage) -> Vec<Rect> {
    if page.columns.is_empty() {
        vec![page.paper]
    } else {
        let mut cols = page.columns.clone();
        cols.sort_by_key(|c| c.x.raw());
        cols
    }
}

/// Index of the column whose x-range contains `x`, else the nearest column by
/// center (full-width content lands in the column it starts in).
fn column_of(x: i32, cols: &[Rect]) -> usize {
    for (i, c) in cols.iter().enumerate() {
        if x >= c.x.raw() && x < c.x.raw() + c.width.raw() {
            return i;
        }
    }
    cols.iter()
        .enumerate()
        .min_by_key(|(_, c)| (x - (c.x.raw() + c.width.raw() / 2)).abs())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// A sourceless op's `(column, y_top, y_bottom)` (HWPUNIT) for attributing it to
/// a unit fragment by geometry. `None` for a text line (attributed by paragraph
/// source instead).
fn sourceless_extent(op: &PaintOp, cols: &[Rect]) -> Option<(usize, i32, i32)> {
    let (cx, y0, y1) = match op {
        PaintOp::FillRect { rect, .. }
        | PaintOp::StrokeRect { rect, .. }
        | PaintOp::Image { rect, .. } => (
            rect.x.raw() + rect.width.raw() / 2,
            rect.y.raw(),
            rect.y.raw() + rect.height.raw(),
        ),
        PaintOp::Line { x1, y1, x2, y2, .. } => ((x1 + x2) / 2, (*y1).min(*y2), (*y1).max(*y2)),
        PaintOp::TextLine(_) => return None,
    };
    Some((column_of(cx, cols), y0, y1))
}

/// Build crop units from the painted document and the model-detected unit ranges
/// (`detect_units_auto` on the same memo-stripped document the pages were painted
/// from, so its section-0 paragraph indices line up with the painted text lines).
/// A unit's paragraphs select its text lines; sourceless ops (borders, fills,
/// images) join by geometry within the unit's text span in their column. Because
/// a question's paragraphs end at its choices, content between questions (section
/// labels like "5지선다형", trailing blank space) belongs to no unit and is
/// dropped. Ops keep their paint order and split into one fragment per page+column
/// the unit occupies, in reading order.
fn build_crop_units(painted: &PaintDocument, units: &[DetectedUnit]) -> Vec<CropUnit> {
    let mut out = Vec::new();
    for unit in units {
        let want: std::collections::HashSet<usize> = unit.para_indices.iter().copied().collect();
        let mut frags: Vec<Frag> = Vec::new();
        for (pi, page) in painted.pages.iter().enumerate() {
            let cols = page_columns(page);
            let content = &page.ops[page.content_range.clone()];
            // Pass 1: this unit's text lines and their per-column vertical span.
            let mut span: Vec<(i32, i32)> = vec![(i32::MAX, i32::MIN); cols.len()];
            let mut text_col: Vec<Option<usize>> = vec![None; content.len()];
            for (i, op) in content.iter().enumerate() {
                if let PaintOp::TextLine(line) = op {
                    // A line belongs to the unit when its paragraph is in `want`.
                    // Block-level tables emit cell text under `Control` (anchored to
                    // a paragraph); inline/riding tables and plain text under
                    // `Paragraph`. Resolve both to the owning paragraph.
                    let para = match line.source {
                        SourceRef::Paragraph(id) => Some(id),
                        SourceRef::Control(cid) => Some(cid.paragraph),
                        _ => None,
                    };
                    if let Some(id) = para {
                        if id.section == SectionId(0) && want.contains(&id.index) {
                            let c = column_of(line.x, &cols);
                            text_col[i] = Some(c);
                            span[c].0 = span[c].0.min(line.top);
                            span[c].1 = span[c].1.max(line.top + line.line_height);
                        }
                    }
                }
            }
            // Pass 2: assign ops to a column fragment in paint order — unit text by
            // source, sourceless ops by geometry within that column's text span.
            let mut frag_ops: Vec<Vec<PaintOp>> = vec![Vec::new(); cols.len()];
            for (i, op) in content.iter().enumerate() {
                let col = if let Some(c) = text_col[i] {
                    Some(c)
                } else if matches!(op, PaintOp::TextLine(_)) {
                    None
                } else {
                    sourceless_extent(op, &cols).and_then(|(c, oy0, oy1)| {
                        let (smin, smax) = span[c];
                        if smax < smin {
                            return None;
                        }
                        let cy = (oy0 + oy1) / 2;
                        (cy >= smin - SOURCELESS_PAD && cy <= smax + SOURCELESS_PAD).then_some(c)
                    })
                };
                if let Some(c) = col {
                    frag_ops[c].push(op.clone());
                }
            }
            for (c, ops) in frag_ops.into_iter().enumerate() {
                if span[c].1 >= span[c].0 && !ops.is_empty() {
                    frags.push(Frag { page: pi, col: c, ops });
                }
            }
        }
        frags.sort_by_key(|f| (f.page, f.col));
        if !frags.is_empty() {
            out.push(CropUnit { frags });
        }
    }
    out
}

/// Crop window for one fragment: horizontal bounds from the ink (tight), vertical
/// bounds from the layout box (keeps line leading so seams join seamlessly).
struct FragView {
    fi: usize,
    x0: f64,
    x1: f64,
    y_top: f64,
    y_bottom: f64,
}

/// Render each fragment and stitch them top-to-bottom into one white-backed SVG.
/// Every fragment shares one horizontal window (the union of fragment ink, at
/// absolute page-x), so text columns and box borders line up across the seam; the
/// vertical extent is each fragment's layout box, so consecutive fragments meet at
/// exactly one line height (no glyphs butting, no double gap). A uniform blank
/// margin wraps the strip. `None` if every fragment is blank.
fn render_crop_unit(
    painted: &PaintDocument,
    unit: &CropUnit,
    fonts: &FontResolver,
) -> Result<Option<String>, ApiError> {
    let m = CROP_MARGIN_PT * CROP_DPI / 72.0;
    let mut fvs: Vec<FragView> = Vec::new();
    for (fi, frag) in unit.frags.iter().enumerate() {
        let page = &painted.pages[frag.page];
        // Render 1 (ink probe): tight horizontal bounds + blank-fragment skip.
        let Some((ix, iy, iw, ih)) = ops_ink_bbox(&frag.ops, page, fonts)? else {
            continue;
        };
        if iw <= 0.0 || ih <= 0.0 {
            continue;
        }
        // Layout box: vertical edges from stored line metrics (ink-expanded so a
        // tall inline equation or descender is never clipped).
        let (_, ly0, _, ly1) = ops_layout_box(&frag.ops).unwrap_or((ix, iy, ix + iw, iy + ih));
        fvs.push(FragView {
            fi,
            x0: ix,
            x1: ix + iw,
            y_top: ly0.min(iy),
            y_bottom: ly1.max(iy + ih),
        });
    }
    if fvs.is_empty() {
        return Ok(None);
    }
    // Each fragment is normalised to its own left edge (a column's content-left),
    // so columns from different page-x positions stack flush-left as one continuous
    // column; the strip width is the widest fragment.
    let content_w = fvs.iter().map(|f| f.x1 - f.x0).fold(0.0_f64, f64::max).max(1.0);
    let content_h: f64 = fvs.iter().map(|f| f.y_bottom - f.y_top).sum();
    let total_w = content_w + 2.0 * m;
    let total_h = content_h + 2.0 * m;
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n",
        w = n(total_w),
        h = n(total_h),
    ));
    svg.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#ffffff\"/>\n",
        n(total_w),
        n(total_h),
    ));
    // Render 2 (final): each fragment over the shared horizontal window at its own
    // height, abutted so the layout-box edges meet — seamless line spacing.
    let mut y_off = m;
    for f in &fvs {
        let h = f.y_bottom - f.y_top;
        let view = (f.x0, f.y_top, content_w, h);
        svg.push_str(&ops_svg_nested(CROP_DPI, fonts, &unit.frags[f.fi].ops, view, m, y_off));
        y_off += h;
    }
    svg.push_str("</svg>\n");
    Ok(Some(svg))
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

/// Render previews of `docs` to files under `save_path`. In `Page` mode each
/// laid-out page is one sheet (`{stem}_p01..`). In `Question` mode the document
/// is scanned page by page for unit markers — question numbers, or Korean set
/// headers when present — and each unit is cropped to its content (stitched
/// across column/page breaks) into one image (`{stem}_q01..`). Works on a full
/// set or an already-split document alike; no re-splitting.
///
/// Returns paths grouped by media type: the outer list follows `media_types`
/// order, each inner list holds every path for that extension (all docs and
/// units/pages, flattened, in render order).
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

    // Strip editing memos once, up front: memos must never render, and the crop's
    // unit detection must see the same paragraph indexing as the painted pages.
    let stripped: Vec<Document> = docs
        .iter()
        .map(|(_, d)| {
            let mut d = d.clone();
            kdsnr_hwp_parser::strip_memos(&mut d);
            d
        })
        .collect();

    // First pass: paint every document whole (decodes glyphs, with the glyph bar).
    let mut painted_docs: Vec<(&str, PaintDocument)> = Vec::new();
    for ((stem, _), doc) in docs.iter().zip(&stripped) {
        painted_docs.push((stem.as_str(), paint(doc, &resolver, glyph_progress)?));
    }

    let fonts = resolver.borrow();
    let write_all = |svg: &str, stem: &str, index: usize, out: &mut Vec<Vec<PathBuf>>| -> Result<(), ApiError> {
        for (slot, &media) in media_types.iter().enumerate() {
            out[slot].push(write_page(svg, media, scale, save_path, stem, kind, index)?);
        }
        Ok(())
    };

    match preview_type {
        PreviewType::Page => {
            let total: usize = painted_docs.iter().map(|(_, p)| p.pages.len()).sum();
            let mut done = 0;
            render_progress(done, total);
            for (stem, painted) in &painted_docs {
                for (i, page) in painted.pages.iter().enumerate() {
                    let svg = page_to_svg(page, 96.0, &fonts);
                    write_all(&svg, stem, i + 1, &mut out)?;
                    done += 1;
                    render_progress(done, total);
                }
            }
        }
        PreviewType::Question => {
            let doc_units: Vec<(&str, &PaintDocument, Vec<CropUnit>)> = painted_docs
                .iter()
                .zip(&stripped)
                .map(|((stem, painted), sdoc)| {
                    let units = detect_units_auto(sdoc);
                    (*stem, painted, build_crop_units(painted, &units))
                })
                .collect();
            let total: usize = doc_units.iter().map(|(_, _, u)| u.len()).sum();
            let mut done = 0;
            render_progress(done, total);
            for (stem, painted, units) in &doc_units {
                let mut index = 1;
                for unit in units {
                    if let Some(svg) = render_crop_unit(painted, unit, &fonts)? {
                        write_all(&svg, stem, index, &mut out)?;
                        index += 1;
                    }
                    done += 1;
                    render_progress(done, total);
                }
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

    /// Manual (needs FONT_DIR): crop each unit of a set and write PNGs to a temp
    /// dir for visual inspection. Run with
    /// `FONT_DIR=… cargo test -p kdsnr-hwp-api crop_units_render -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn crop_units_render() {
        for (file, mode) in [
            ("math_input_sample.hwpx", PreviewType::Question),
            ("social_test_input_2.hwpx", PreviewType::Question),
            ("korean.hwpx", PreviewType::Question),
            ("social_input_sample.hwpx", PreviewType::Question),
            ("social_input_sample.hwpx", PreviewType::Page),
        ] {
            let (doc, _) = import_file(&original(file)).expect("import");
            let tag = if matches!(mode, PreviewType::Page) { "page" } else { "q" };
            let dir = std::env::temp_dir().join("kdsnr_crop_units").join(format!("{file}.{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            let out = export_preview(
                &[(file.trim_end_matches(".hwpx").into(), doc)],
                &dir,
                mode,
                &[MediaType::Png],
                2.0,
                &mut |_d, _t| {},
                &mut |_d, _t| {},
            )
            .expect("export");
            let n: usize = out.iter().map(|g| g.len()).sum();
            eprintln!("{file} [{tag}]: {n} -> {}", dir.display());
        }
    }
}
