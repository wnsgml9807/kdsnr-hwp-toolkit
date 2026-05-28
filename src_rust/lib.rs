//! Python extension (`kdsnr_hwp_toolkit._native`): a thin PyO3 wrapper over the
//! `kdsnr-hwp-api` core. The single working type is `Document`; every entry point
//! runs the corruption guard and raises `ValueError` on tool-damaged input.
//! Dataset export lives Python-side over the exposed content.

use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use kdsnr_hwp_api as api;
use kdsnr_hwp_api::render::{export_preview as core_export_preview, MediaType, PreviewType};
use kdsnr_hwp_api::{FileType, SourceFormat};

fn value_err<E: std::fmt::Display>(e: E) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// The engine IR for one HWP/HWPX document. Construct via `import_file`; carries
/// the source-container tag and a filename stem used when naming preview outputs.
#[pyclass(name = "Document")]
#[derive(Clone)]
struct Document {
    inner: api::Document,
    source_format: SourceFormat,
    /// Filename stem for previews; set from the source path or a split label.
    stem: Option<String>,
    /// Question label, when this document is a split unit.
    label: Option<String>,
}

#[pymethods]
impl Document {
    /// Source container the document was imported from: `"hwp" | "hwpx" | "unknown"`.
    #[getter]
    fn source_format(&self) -> &'static str {
        self.source_format.as_str()
    }

    /// Question label when this is a split unit, else `None`.
    #[getter]
    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    /// Number of body sections.
    #[getter]
    fn section_count(&self) -> usize {
        self.inner.sections.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Document(source_format='{}', sections={}{})",
            self.source_format.as_str(),
            self.inner.sections.len(),
            self.label.as_deref().map(|l| format!(", label='{l}'")).unwrap_or_default(),
        )
    }
}

/// Read and parse a file to a `Document`. The container is detected from the
/// bytes (not the extension). Raises `ValueError` if the file is missing,
/// unparseable, or tool-corrupted.
#[pyfunction]
fn import_file(path: PathBuf) -> PyResult<Document> {
    let (inner, source_format) = api::import_file(&path).map_err(value_err)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).map(String::from);
    Ok(Document { inner, source_format, stem, label: None })
}

/// Save `doc` to `path`. `file_type` is `"hwp"`, `"hwpx"`, or `None` to infer
/// from the path extension. Returns the written path.
#[pyfunction]
#[pyo3(signature = (doc, path, file_type=None))]
fn save_file(doc: &Document, path: PathBuf, file_type: Option<&str>) -> PyResult<String> {
    let ft = match file_type {
        Some(s) => Some(parse_file_type(s)?),
        None => None,
    };
    // HWPX→HWP serialization is not yet supported (next version): block saving an
    // HWPX-origin document as HWP, whether the target is given explicitly or via
    // the path extension. HWP-origin saves and HWPX targets are unaffected.
    let target_is_hwp = match ft {
        Some(FileType::Hwp) => true,
        Some(FileType::Hwpx) => false,
        None => path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("hwp")),
    };
    if target_is_hwp && matches!(doc.source_format, SourceFormat::Hwpx) {
        return Err(PyValueError::new_err(HWPX_TO_HWP_UNSUPPORTED));
    }
    let out = api::save_file(&doc.inner, &path, ft).map_err(value_err)?;
    Ok(out.display().to_string())
}

/// Tag a document as HWPX. The IR is shared between formats, so this only flips
/// the format tag; the HWPX container is produced when `save_file` runs.
#[pyfunction]
fn hwp_to_hwpx(doc: &Document) -> Document {
    Document {
        inner: doc.inner.clone(),
        source_format: SourceFormat::Hwpx,
        stem: doc.stem.clone(),
        label: doc.label.clone(),
    }
}

const HWPX_TO_HWP_UNSUPPORTED: &str =
    "[KDSNR-HWP-TOOLKIT] HWPX→HWP 변환은 다음 버전에서 지원 예정입니다.";

/// Convert an HWPX-origin document to HWP. Not yet supported — raises
/// `ValueError` (planned for a future version). HWP→HWPX and same-format saves
/// are unaffected.
#[pyfunction]
fn hwpx_to_hwp(_doc: &Document) -> PyResult<Document> {
    Err(PyValueError::new_err(HWPX_TO_HWP_UNSUPPORTED))
}

/// Split a problem-set document into per-question `Document`s, in order. Each is
/// a complete document ready to render or save, carrying its question label.
#[pyfunction]
fn split_set_to_question(doc: &Document) -> PyResult<Vec<Document>> {
    let units = api::split_set_to_question(&doc.inner).map_err(value_err)?;
    let parent = doc.stem.clone();
    Ok(units
        .into_iter()
        .enumerate()
        .map(|(i, (label, inner))| {
            let stem = match &parent {
                Some(s) => format!("{s}_q{:02}", i + 1),
                None => format!("q{:02}", i + 1),
            };
            Document { inner, source_format: SourceFormat::Hwpx, stem: Some(stem), label: Some(label) }
        })
        .collect())
}

/// Render previews to files under `save_path`. `preview_type` is `"page"` (one
/// sheet per laid-out page) or `"question"` (split first, then render each
/// question). `media_types` selects `"svg" | "png" | "pdf"`; `dpi` is the PNG
/// raster resolution (vector-accurate; SVG/PDF ignore it).
///
/// Returns paths grouped by media type: outer list follows `media_types` order,
/// each inner list holds every path for that extension (all docs/pages flattened,
/// in render order).
#[pyfunction]
#[pyo3(signature = (docs, save_path, preview_type="page", media_types=None, dpi=200.0, progress=None))]
fn export_preview(
    py: Python<'_>,
    docs: Vec<Document>,
    save_path: PathBuf,
    preview_type: &str,
    media_types: Option<Vec<String>>,
    dpi: f32,
    progress: Option<PyObject>,
) -> PyResult<Vec<Vec<String>>> {
    let pt = parse_preview_type(preview_type)?;
    let media_types = media_types.unwrap_or_else(|| vec!["png".into()]);
    let media: Vec<MediaType> =
        media_types.iter().map(|s| parse_media_type(s)).collect::<PyResult<_>>()?;
    // The engine SVG is laid out at 96 DPI; the raster scale is dpi/96 and resvg
    // renders the vector tree at that scale (so dpi is true resolution, not upscaling).
    let scale = dpi / 96.0;

    let pairs: Vec<(String, api::Document)> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.stem.clone().unwrap_or_else(|| format!("doc{}", i + 1)), d.inner.clone()))
        .collect();

    // Progress forwarded to an optional Python callable `progress(phase, done, total)`,
    // phase ∈ {"glyph","render"}. The glyph phase fires only on a cold glyph cache.
    let mut glyph_cb = |done: usize, total: usize| {
        if let Some(cb) = &progress {
            let _ = cb.call1(py, ("glyph", done, total));
        }
    };
    let mut render_cb = |done: usize, total: usize| {
        if let Some(cb) = &progress {
            let _ = cb.call1(py, ("render", done, total));
        }
    };
    let out =
        core_export_preview(&pairs, &save_path, pt, &media, scale, &mut glyph_cb, &mut render_cb)
            .map_err(value_err)?;
    Ok(out
        .into_iter()
        .map(|group| group.into_iter().map(|p| p.display().to_string()).collect())
        .collect())
}

/// True if the document's stored layout looks tool-corrupted (does not raise).
#[pyfunction]
fn is_corrupt(doc: &Document) -> bool {
    api::is_corrupt(&doc.inner)
}

/// Check the font directory for every face the documents need; on Windows/macOS
/// collect any missing files from an installed Hancom Office. Returns a dict:
/// `font_dir`, `os`, `required` (count), `collected` and `missing` as lists of
/// `(face, file)`. Does not raise — the caller decides (the wrapper raises a
/// `ValueError` listing `missing`).
#[pyfunction]
fn prepare_fonts(py: Python<'_>, docs: Vec<Document>) -> PyResult<PyObject> {
    let inner: Vec<api::Document> = docs.iter().map(|d| d.inner.clone()).collect();
    let report = api::fonts::collect_fonts(&inner);
    let d = PyDict::new_bound(py);
    d.set_item("font_dir", report.font_dir.display().to_string())?;
    d.set_item("os", report.os)?;
    d.set_item("required", report.required)?;
    d.set_item("collected", report.collected)?;
    d.set_item("missing", report.missing)?;
    Ok(d.into())
}

fn parse_file_type(s: &str) -> PyResult<FileType> {
    match s.to_ascii_lowercase().as_str() {
        "hwp" => Ok(FileType::Hwp),
        "hwpx" => Ok(FileType::Hwpx),
        _ => Err(PyValueError::new_err(format!("unknown file_type '{s}' (expected 'hwp'|'hwpx')"))),
    }
}

fn parse_preview_type(s: &str) -> PyResult<PreviewType> {
    match s.to_ascii_lowercase().as_str() {
        "page" => Ok(PreviewType::Page),
        "question" => Ok(PreviewType::Question),
        _ => Err(PyValueError::new_err(format!(
            "unknown preview_type '{s}' (expected 'page'|'question')"
        ))),
    }
}

fn parse_media_type(s: &str) -> PyResult<MediaType> {
    match s.to_ascii_lowercase().as_str() {
        "svg" => Ok(MediaType::Svg),
        "png" => Ok(MediaType::Png),
        "pdf" => Ok(MediaType::Pdf),
        _ => Err(PyValueError::new_err(format!(
            "unknown media_type '{s}' (expected 'svg'|'png'|'pdf')"
        ))),
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Document>()?;
    m.add_function(wrap_pyfunction!(import_file, m)?)?;
    m.add_function(wrap_pyfunction!(save_file, m)?)?;
    m.add_function(wrap_pyfunction!(hwp_to_hwpx, m)?)?;
    m.add_function(wrap_pyfunction!(hwpx_to_hwp, m)?)?;
    m.add_function(wrap_pyfunction!(split_set_to_question, m)?)?;
    m.add_function(wrap_pyfunction!(export_preview, m)?)?;
    m.add_function(wrap_pyfunction!(is_corrupt, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_fonts, m)?)?;
    Ok(())
}
