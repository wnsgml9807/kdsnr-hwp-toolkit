//! Model-centric conversion/render API over the KDSNR HWP/HWPX engine.
//!
//! The single working type is the engine's `Document` IR (one model for both
//! formats; the source format is metadata). Every entry point runs a corruption
//! guard first and rejects documents whose stored layout was damaged by a
//! non-Hancom tool. The Python extension module wraps these functions; dataset
//! export (ShareGPT/OpenAI) lives Python-side over the exposed content.
//!
//! See `project_engine_api_redesign` for the full design and `API_SPEC.md`.

pub mod fonts;
pub mod render;

use std::path::{Path, PathBuf};

pub use kdsnr_hwp_parser::model::document::Document;
use kdsnr_hwp_parser::model::paragraph::Paragraph;
use kdsnr_hwp_parser::parser::{detect_format, FileFormat};
use kdsnr_hwp_parser::split::{split_document_units, SplitError};
use kdsnr_hwp_parser::serializer::{serialize_hwp, serialize_hwpx};
use kdsnr_hwp_parser::parse_document;

/// The message surfaced (as a Python `ValueError`) when a document is rejected
/// as tool-corrupted. Verbatim per product copy.
pub const CORRUPT_MESSAGE: &str =
    "[KDSNR-HWP-TOOLKIT] 한컴이 아닌 다른 툴에 의해 변형되거나 편집되어 손상된 문서입니다. 변환이 불가능합니다.";

#[derive(Debug)]
pub enum ApiError {
    /// File read / write failed.
    Io(String),
    /// The bytes could not be parsed as HWP or HWPX.
    Parse(String),
    /// The document parsed but its stored layout is damaged (see `CORRUPT_MESSAGE`).
    Corrupt,
    /// Serialization to HWP/HWPX failed.
    Serialize(String),
    /// The save format could not be inferred from the path extension.
    UnknownFormat(String),
    /// Document set could not be split into questions.
    Split(String),
    /// Korean subject is not yet supported for question split / per-question
    /// (crop) preview.
    UnsupportedKorean,
    /// Required font files are missing and could not be collected: `(face, file)`.
    FontsMissing(Vec<(String, String)>),
    /// Preview rendering failed.
    Render(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Io(e) => write!(f, "io error: {e}"),
            ApiError::Parse(e) => write!(f, "parse failed: {e}"),
            ApiError::Corrupt => f.write_str(CORRUPT_MESSAGE),
            ApiError::Serialize(e) => write!(f, "serialize failed: {e}"),
            ApiError::UnknownFormat(e) => write!(f, "unknown file format: {e}"),
            ApiError::Split(e) => write!(f, "split failed: {e}"),
            ApiError::UnsupportedKorean => {
                f.write_str("국어 과목은 문항별 분할과 미리보기를 지원하지 않습니다. (다음 버전 예정)")
            }
            ApiError::FontsMissing(rows) => {
                f.write_str(
                    "[KDSNR-HWP-TOOLKIT] 일부 폰트 파일을 찾을 수 없습니다. 폰트 폴더(FONT_DIR)를 확인하세요.\n# 누락 폰트",
                )?;
                for (i, (face, file)) in rows.iter().enumerate() {
                    write!(f, "\n  {} : {face} -> {file}", i + 1)?;
                }
                Ok(())
            }
            ApiError::Render(e) => write!(f, "render failed: {e}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// The source container a `Document` was imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Hwp,
    Hwpx,
    Unknown,
}

impl SourceFormat {
    fn from_detected(f: FileFormat) -> Self {
        match f {
            FileFormat::Hwp => SourceFormat::Hwp,
            FileFormat::Hwpx => SourceFormat::Hwpx,
            _ => SourceFormat::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SourceFormat::Hwp => "hwp",
            SourceFormat::Hwpx => "hwpx",
            SourceFormat::Unknown => "unknown",
        }
    }
}

/// Target container for `save_file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Hwp,
    Hwpx,
}

impl FileType {
    /// Infer from a path extension (case-insensitive). `None` when unrecognized.
    fn from_extension(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str())?.to_ascii_lowercase().as_str() {
            "hwp" => Some(FileType::Hwp),
            "hwpx" => Some(FileType::Hwpx),
            _ => None,
        }
    }
}

/// Corruption guard: HWPX written by a non-Hancom tool can leave a paragraph's
/// stored line segments un-enriched — every line collapsed to `vertical_pos = 0`.
/// Hancom re-flows such a paragraph at render time, but our renderer honours the
/// stored positions, so the lines would overlap. We reject rather than render
/// garbage. The signal is precise (validated: flags the damaged science inputs,
/// zero false positives across the pristine originals): a paragraph with **two
/// or more** line segments whose `vertical_pos` are **all zero**. (A single
/// first line at 0 is normal; a justified two-segment line shares a non-zero
/// vertpos; a column-split paragraph has one 0 among non-zeros — none trip this.)
fn paragraph_is_corrupt(p: &Paragraph) -> bool {
    let segs = &p.line_segs;
    segs.len() >= 2 && segs.iter().all(|s| s.vertical_pos == 0)
}

/// True if any (top-level) paragraph in the document is corrupt.
pub fn is_corrupt(doc: &Document) -> bool {
    doc.sections
        .iter()
        .any(|sec| sec.paragraphs.iter().any(paragraph_is_corrupt))
}

/// Parse + corruption guard. Internal chokepoint shared by `import_file`.
fn import_bytes(data: &[u8]) -> Result<(Document, SourceFormat), ApiError> {
    let format = SourceFormat::from_detected(detect_format(data));
    let doc = parse_document(data).map_err(|e| ApiError::Parse(format!("{e:?}")))?;
    if is_corrupt(&doc) {
        return Err(ApiError::Corrupt);
    }
    Ok((doc, format))
}

/// Read a file and parse it to the `Document` IR. The container (HWP/HWPX) is
/// detected from the bytes, not the extension. Rejects tool-corrupted documents
/// with `ApiError::Corrupt`.
pub fn import_file(path: &Path) -> Result<(Document, SourceFormat), ApiError> {
    let data = std::fs::read(path).map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))?;
    import_bytes(&data)
}

/// Serialize `doc` to `path`. `file_type` `None` infers from the path extension.
/// Returns the written path.
pub fn save_file(
    doc: &Document,
    path: &Path,
    file_type: Option<FileType>,
) -> Result<PathBuf, ApiError> {
    let ft = match file_type {
        Some(ft) => ft,
        None => FileType::from_extension(path)
            .ok_or_else(|| ApiError::UnknownFormat(path.display().to_string()))?,
    };
    let bytes = match ft {
        FileType::Hwpx => serialize_hwpx(doc),
        FileType::Hwp => serialize_hwp(doc),
    }
    .map_err(|e| ApiError::Serialize(e.to_string()))?;
    std::fs::write(path, &bytes).map_err(|e| ApiError::Io(format!("{}: {e}", path.display())))?;
    Ok(path.to_path_buf())
}

/// Split a problem-set document into per-question `Document`s. Each carries its
/// detected label and is a complete document (header/styles preserved), ready to
/// render or save. The shared IR means HWP and HWPX sources split identically.
pub fn split_set_to_question(doc: &Document) -> Result<Vec<(String, Document)>, ApiError> {
    let (_subject, units) = split_document_units(doc).map_err(|e| match e {
        SplitError::UnsupportedKorean => ApiError::UnsupportedKorean,
        other => ApiError::Split(format!("{other:?}")),
    })?;
    Ok(units.into_iter().map(|q| (q.label, q.document)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templet/original").join(name)
    }

    #[test]
    fn rejects_tool_corrupted_inputs() {
        for name in ["science_input_example.hwpx", "science_input_example_2.hwpx"] {
            match import_file(&original(name)) {
                Err(ApiError::Corrupt) => {}
                other => panic!("{name}: expected Corrupt, got {other:?}"),
            }
        }
    }

    #[test]
    fn accepts_pristine_originals() {
        for name in [
            "science.hwpx", "math.hwpx", "math_input_sample.hwpx", "math_input_sample_2.hwpx",
            "social.hwpx", "social_input_sample.hwpx", "korean.hwpx",
        ] {
            import_file(&original(name))
                .unwrap_or_else(|e| panic!("{name}: pristine doc rejected: {e:?}"));
        }
    }

    #[test]
    fn file_type_inferred_from_extension() {
        assert_eq!(FileType::from_extension(Path::new("a.hwp")), Some(FileType::Hwp));
        assert_eq!(FileType::from_extension(Path::new("a.HWPX")), Some(FileType::Hwpx));
        assert_eq!(FileType::from_extension(Path::new("a.txt")), None);
    }

    /// Probe (run with `--ignored`): does HWP binary save round-trip? For each
    /// pristine original, save to HWP, re-import, and report parse/guard outcome.
    /// Decides whether `save_file(.., Hwp)` is reliable enough to offer silently.
    #[test]
    #[ignore]
    fn probe_hwp_round_trip() {
        let names = [
            "science.hwpx", "math.hwpx", "math_input_sample.hwpx", "math_input_sample_2.hwpx",
            "social.hwpx", "social_input_sample.hwpx", "korean.hwpx",
        ];
        let mut ok = 0;
        for name in names {
            let (doc, _) = import_file(&original(name)).expect("import");
            let out = std::env::temp_dir().join(format!("kdsnr_hwp_rt_{name}.hwp"));
            let r = save_file(&doc, &out, Some(FileType::Hwp));
            match r.and_then(|_| import_file(&out)) {
                Ok((_d, fmt)) => {
                    ok += 1;
                    eprintln!("  OK   {name}: re-import fmt={:?}", fmt);
                }
                Err(e) => eprintln!("  FAIL {name}: {e}"),
            }
            let _ = std::fs::remove_file(&out);
        }
        eprintln!("HWP round-trip: {ok}/{} survived save+reimport+guard", names.len());
    }

    #[test]
    fn hwpx_round_trip_survives_corruption_guard() {
        // Save a pristine doc back to HWPX and re-import: must parse and pass the
        // guard (the serializer must not produce all-vertpos-0 paragraphs).
        let (doc, _) = import_file(&original("korean.hwpx")).expect("import");
        let out = std::env::temp_dir().join("kdsnr_api_roundtrip.hwpx");
        save_file(&doc, &out, None).expect("save");
        let (_doc2, fmt) = import_file(&out).expect("re-import");
        assert_eq!(fmt, SourceFormat::Hwpx);
        let _ = std::fs::remove_file(&out);
    }
}
