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
use kdsnr_hwp_parser::parse_document;
use kdsnr_hwp_parser::parser::{detect_format, FileFormat};
use kdsnr_hwp_parser::serializer::{serialize_hwp, serialize_hwpx};
use kdsnr_hwp_parser::split::{split_document_units, SplitError};

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
            ApiError::UnsupportedKorean => f.write_str(
                "국어 과목은 문항별 분할과 미리보기를 지원하지 않습니다. (다음 버전 예정)",
            ),
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
        match path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase()
            .as_str()
        {
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

/// One question extracted for AI/dataset consumption. In `text`, each equation
/// script sits at its inline position wrapped in STX (`\u{2}`) / ETX (`\u{3}`)
/// sentinels — the Python wrapper turns those spans into LaTeX. `images` is each
/// embedded raster's bytes and source extension.
pub struct QuestionItem {
    pub label: String,
    pub subject: String,
    pub text: String,
    pub images: Vec<(Vec<u8>, String)>,
}

/// Every embedded raster in the document (paragraph objects and table cells),
/// in document order, as `(bytes, ext)`. Resolved via `normalize`, which decodes
/// each picture's stored binary — no fonts needed.
fn collect_images(doc: &Document) -> Vec<(Vec<u8>, String)> {
    fn walk(para: &kdsnr_hwp_doc::ParagraphModel, out: &mut Vec<(Vec<u8>, String)>) {
        for obj in &para.objects {
            if let kdsnr_hwp_doc::ObjectContent::Image { data, ext } = &obj.content {
                out.push((data.as_ref().clone(), ext.clone()));
            }
        }
        for table in &para.tables {
            for cell in &table.cells {
                for p in &cell.paragraphs {
                    walk(p, out);
                }
            }
        }
    }
    let model = kdsnr_hwp_doc::normalize(doc);
    let mut out = Vec::new();
    for section in &model.sections {
        for para in &section.paragraphs {
            walk(para, &mut out);
        }
    }
    out
}

/// Extract a problem set's questions, ready to serialize as JSON for a language
/// model: plain text, the equation scripts it contains, and its embedded images.
pub fn extract_questions(doc: &Document) -> Result<Vec<QuestionItem>, ApiError> {
    let (subject, units) = split_document_units(doc).map_err(|e| match e {
        SplitError::UnsupportedKorean => ApiError::UnsupportedKorean,
        other => ApiError::Split(format!("{other:?}")),
    })?;
    let subject = subject.as_str().to_string();
    Ok(units
        .into_iter()
        .map(|q| QuestionItem {
            label: q.label,
            subject: subject.clone(),
            text: kdsnr_hwp_parser::document_text_eq_marked(&q.document),
            images: collect_images(&q.document),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../templet/original")
            .join(name)
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
            "science.hwpx",
            "math.hwpx",
            "math_input_sample.hwpx",
            "math_input_sample_2.hwpx",
            "social.hwpx",
            "social_input_sample.hwpx",
            "korean.hwpx",
        ] {
            import_file(&original(name))
                .unwrap_or_else(|e| panic!("{name}: pristine doc rejected: {e:?}"));
        }
    }

    #[test]
    fn file_type_inferred_from_extension() {
        assert_eq!(
            FileType::from_extension(Path::new("a.hwp")),
            Some(FileType::Hwp)
        );
        assert_eq!(
            FileType::from_extension(Path::new("a.HWPX")),
            Some(FileType::Hwpx)
        );
        assert_eq!(FileType::from_extension(Path::new("a.txt")), None);
    }

    #[test]
    fn hwpx_to_hwp_output_has_hancom_docinfo_trailer() {
        let (doc, _) = import_file(&original("social.hwpx")).expect("import");
        let bytes = kdsnr_hwp_parser::serializer::serialize_hwp(&doc).expect("serialize");
        let mut cfb = kdsnr_hwp_parser::parser::cfb_reader::CfbReader::open(&bytes).expect("cfb");

        let header = kdsnr_hwp_parser::parser::header::parse_file_header(
            &cfb.read_file_header().expect("header"),
        )
        .expect("parse header");
        assert_eq!(header.version.major, 5);
        assert_eq!(header.version.minor, 1);
        assert_eq!(header.version.build, 1);
        assert_eq!(header.version.revision, 0);
        assert!(header.flags.compressed);

        let doc_info = cfb.read_doc_info(true).expect("docinfo");
        let records =
            kdsnr_hwp_parser::parser::record::Record::read_all(&doc_info).expect("docinfo records");
        let tags: Vec<u16> = records.iter().map(|r| r.tag_id).collect();
        assert!(tags.contains(&kdsnr_hwp_parser::parser::tags::HWPTAG_DOC_DATA));
        assert!(tags.contains(&kdsnr_hwp_parser::parser::tags::HWPTAG_FORBIDDEN_CHAR));
        assert!(tags.contains(&kdsnr_hwp_parser::parser::tags::HWPTAG_COMPATIBLE_DOCUMENT));
        assert!(tags.contains(&kdsnr_hwp_parser::parser::tags::HWPTAG_LAYOUT_COMPATIBILITY));
        assert!(tags.contains(&kdsnr_hwp_parser::parser::tags::HWPTAG_TRACKCHANGE));
    }

    /// Probe (run with `--ignored`): does HWP binary save round-trip? For each
    /// pristine original, save to HWP, re-import, and report parse/guard outcome.
    /// Decides whether `save_file(.., Hwp)` is reliable enough to offer silently.
    #[test]
    #[ignore]
    fn probe_hwp_round_trip() {
        let names = [
            "science.hwpx",
            "math.hwpx",
            "math_input_sample.hwpx",
            "math_input_sample_2.hwpx",
            "social.hwpx",
            "social_input_sample.hwpx",
            "korean.hwpx",
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
        eprintln!(
            "HWP round-trip: {ok}/{} survived save+reimport+guard",
            names.len()
        );
    }

    /// Each split question composes into a template, serializes to HWPX, and
    /// re-imports past the corruption guard with a structural carrier plus
    /// content.
    fn assert_carrier_advance(file: &str, label: &str, doc: &Document, keep_spacer: bool) {
        let sec = doc.sections.first().expect("section");
        let carrier = sec.paragraphs.first().expect("carrier");
        let advance = carrier
            .line_segs
            .first()
            .map(|s| s.text_height + s.line_spacing)
            .unwrap_or(0);
        if keep_spacer {
            assert!(
                advance > 0,
                "{file} {label}: carrier spacer should be visible, got {advance}"
            );
        } else {
            assert_eq!(
                advance, 0,
                "{file} {label}: Korean carrier spacer should be zero-height"
            );
        }
    }

    fn assert_korean_set_header_spacer(file: &str, label: &str, doc: &Document) {
        let sec = doc.sections.first().expect("section");
        let header_idx = sec
            .paragraphs
            .iter()
            .position(|p| p.text.contains('[') && p.text.contains(']'))
            .unwrap_or_else(|| panic!("{file} {label}: no Korean set header"));
        let spacer = sec
            .paragraphs
            .get(header_idx + 1)
            .unwrap_or_else(|| panic!("{file} {label}: no paragraph after set header"));
        assert!(
            spacer.text.trim().is_empty(),
            "{file} {label}: expected spacer after set header, got {:?}",
            spacer.text
        );
        let advance = spacer
            .line_segs
            .first()
            .map(|s| s.text_height + s.line_spacing)
            .unwrap_or(0);
        assert!(
            advance > 0,
            "{file} {label}: set header spacer should have positive advance"
        );
    }

    fn check_compose(file: &str, carrier_spacer: Option<bool>) {
        let (doc, _) = import_file(&original(file)).expect("import");
        let questions = split_set_to_question(&doc).expect("split");
        assert!(!questions.is_empty(), "{file}: no questions");
        for (label, qdoc) in &questions {
            let sec = qdoc.sections.first().expect("section");
            assert!(
                sec.paragraphs.len() >= 2,
                "{file} {label}: carrier+content expected, got {}",
                sec.paragraphs.len()
            );
            if let Some(keep_spacer) = carrier_spacer {
                assert_carrier_advance(file, label, qdoc, keep_spacer);
            }
            if file == "korean.hwpx" {
                assert_korean_set_header_spacer(file, label, qdoc);
            }
            let out = std::env::temp_dir().join(format!("kdsnr_compose_{label}.hwpx"));
            save_file(qdoc, &out, Some(FileType::Hwpx))
                .unwrap_or_else(|e| panic!("{file} {label}: save: {e}"));
            let (re, _) =
                import_file(&out).unwrap_or_else(|e| panic!("{file} {label}: reimport: {e}"));
            if let Some(keep_spacer) = carrier_spacer {
                assert_carrier_advance(file, label, &re, keep_spacer);
            }
            if file == "korean.hwpx" {
                assert_korean_set_header_spacer(file, label, &re);
            }
            let _ = std::fs::remove_file(&out);
        }
    }

    #[test]
    fn compose_math_questions() {
        check_compose("math_input_sample.hwpx", Some(true));
    }

    #[test]
    fn compose_science_questions() {
        check_compose("science.hwpx", None);
    }

    #[test]
    fn compose_social_questions() {
        check_compose("social_input_sample.hwpx", None);
    }

    #[test]
    fn compose_korean_sets() {
        check_compose("korean.hwpx", Some(false));
    }

    /// Diagnostic: does the parsed model retain the deeply-nested content of
    /// the last paragraph (parser completeness), or did the parser drop it?
    #[test]
    #[ignore]
    fn diag_last_paragraph_controls() {
        use kdsnr_hwp_parser::model::control::Control;
        let bytes = std::fs::read(original("math_input_sample.hwp")).unwrap();
        let doc = parse_document(&bytes).unwrap();
        let sec = &doc.sections[0];
        eprintln!("section0 top-level paragraphs: {}", sec.paragraphs.len());
        fn count_paras(ps: &[Paragraph]) -> usize {
            let mut n = ps.len();
            for p in ps {
                for c in &p.controls {
                    n += match c {
                        Control::Table(t) => {
                            t.cells.iter().map(|cl| count_paras(&cl.paragraphs)).sum()
                        }
                        Control::Shape(s) => count_shape_paras(s),
                        Control::Header(h) => count_paras(&h.paragraphs),
                        Control::Footer(f) => count_paras(&f.paragraphs),
                        Control::Footnote(f) => count_paras(&f.paragraphs),
                        Control::Endnote(e) => count_paras(&e.paragraphs),
                        Control::HiddenComment(h) => count_paras(&h.paragraphs),
                        _ => 0,
                    };
                }
            }
            n
        }
        fn count_shape_paras(s: &kdsnr_hwp_parser::model::shape::ShapeObject) -> usize {
            use kdsnr_hwp_parser::model::shape::ShapeObject;
            match s {
                ShapeObject::Group(g) => g.children.iter().map(count_shape_paras).sum(),
                _ => 0,
            }
        }
        let last = sec.paragraphs.last().unwrap();
        eprintln!("last paragraph: {} controls", last.controls.len());
        use std::collections::BTreeMap;
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &last.controls {
            *kinds
                .entry(match c {
                    Control::Equation(_) => "Equation",
                    Control::Table(_) => "Table",
                    Control::Shape(_) => "Shape",
                    Control::Picture(_) => "Picture",
                    Control::SectionDef(_) => "SectionDef",
                    Control::ColumnDef(_) => "ColumnDef",
                    _ => "other",
                })
                .or_default() += 1;
        }
        eprintln!("last paragraph control kinds: {:?}", kinds);
        eprintln!(
            "total recursive paragraphs in section0: {}",
            count_paras(&sec.paragraphs)
        );
    }

    /// Deep-clear bisection: clear ALL raw passthrough recursively (mimics a
    /// composed HWPX-origin doc which has no HWP raw bytes anywhere), so every
    /// record is from-model. Byte-compare record sizes to genuine to find any
    /// remaining missing fixed field in the from-model path.
    #[test]
    #[ignore]
    fn dump_deepclear() {
        use kdsnr_hwp_parser::model::control::Control;
        use kdsnr_hwp_parser::model::shape::ShapeObject;
        fn clear_paras(ps: &mut [Paragraph]) {
            for p in ps {
                p.raw_header_extra.clear();
                for c in &mut p.controls {
                    match c {
                        Control::SectionDef(s) => s.raw_ctrl_extra.clear(),
                        Control::Table(t) => {
                            t.raw_ctrl_data.clear();
                            t.raw_table_record_extra.clear();
                            t.raw_table_record_attr = 0;
                            t.common.raw_extra.clear();
                            for cell in &mut t.cells {
                                clear_paras(&mut cell.paragraphs);
                            }
                        }
                        Control::Equation(e) => {
                            e.raw_ctrl_data.clear();
                            e.raw_eqedit_data.clear();
                            e.common.raw_extra.clear();
                        }
                        Control::Shape(s) => {
                            s.common_mut().raw_extra.clear();
                            clear_shape(s);
                        }
                        Control::Header(h) => {
                            h.raw_ctrl_extra.clear();
                            clear_paras(&mut h.paragraphs);
                        }
                        Control::Footer(f) => {
                            f.raw_ctrl_extra.clear();
                            clear_paras(&mut f.paragraphs);
                        }
                        Control::Footnote(f) => clear_paras(&mut f.paragraphs),
                        Control::Endnote(e) => clear_paras(&mut e.paragraphs),
                        Control::HiddenComment(h) => clear_paras(&mut h.paragraphs),
                        _ => {}
                    }
                }
            }
        }
        fn clear_shape(s: &mut ShapeObject) {
            if let ShapeObject::Group(g) = s {
                for ch in &mut g.children {
                    ch.common_mut().raw_extra.clear();
                    clear_shape(ch);
                }
            }
        }
        let bytes = std::fs::read(original("social.hwp")).unwrap();
        let mut doc = parse_document(&bytes).unwrap();
        doc.doc_info.raw_stream = None;
        doc.doc_info.raw_stream_dirty = true;
        doc.doc_properties.raw_data = None;
        for v in &mut doc.doc_info.font_faces {
            for f in v {
                f.raw_data = None;
            }
        }
        for e in &mut doc.doc_info.char_shapes {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.para_shapes {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.border_fills {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.tab_defs {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.numberings {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.bullets {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.styles {
            e.raw_data = None;
        }
        for s in &mut doc.sections {
            s.raw_stream = None;
            clear_paras(&mut s.paragraphs);
        }
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../work/debug/compose_verify");
        let out = dir.join("_social_deepclear.hwp");
        std::fs::write(&out, serialize_hwp(&doc).unwrap()).unwrap();
        eprintln!("wrote {}", out.display());
    }

    /// Bisection ladder (run with --ignored): from a genuine .hwp produce four
    /// variants that isolate which serialization layer breaks Hancom load.
    ///   _bisect_passthrough  : all raw kept (pure CFB/framework + recompress)
    ///   _bisect_docinfo_fm   : DocInfo from-model, body raw kept
    ///   _bisect_body_fm      : body from-model, DocInfo raw kept
    ///   _bisect_all_fm       : everything from-model (== composed path)
    #[test]
    #[ignore]
    fn dump_bisection_ladder() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../work/debug/compose_verify");
        let src = original("math_input_sample.hwp");
        let write = |doc: &Document, name: &str| {
            let out = dir.join(name);
            let bytes = serialize_hwp(doc).expect("serialize");
            std::fs::write(&out, &bytes).expect("write");
            eprintln!("wrote {} ({} bytes)", out.display(), bytes.len());
        };
        // passthrough
        let doc = parse_document(&std::fs::read(&src).unwrap()).unwrap();
        write(&doc, "_bisect_passthrough.hwp");
        // DocInfo from-model only
        let mut d = parse_document(&std::fs::read(&src).unwrap()).unwrap();
        d.doc_info.raw_stream = None;
        d.doc_info.raw_stream_dirty = true;
        write(&d, "_bisect_docinfo_fm.hwp");
        // body from-model only
        let mut d = parse_document(&std::fs::read(&src).unwrap()).unwrap();
        for s in &mut d.sections {
            s.raw_stream = None;
        }
        write(&d, "_bisect_body_fm.hwp");
        // everything from-model
        let mut d = parse_document(&std::fs::read(&src).unwrap()).unwrap();
        d.doc_info.raw_stream = None;
        d.doc_info.raw_stream_dirty = true;
        for s in &mut d.sections {
            s.raw_stream = None;
        }
        write(&d, "_bisect_all_fm.hwp");
    }

    #[test]
    #[ignore]
    fn dump_from_model_hwp() {
        let bytes = std::fs::read(original("math_input_sample.hwp")).expect("read");
        let mut doc = kdsnr_hwp_parser::parse_document(&bytes).expect("parse");
        doc.doc_info.raw_stream = None;
        doc.doc_info.raw_stream_dirty = true;
        // Clear per-record raw_data too, forcing from-model serialization of
        // every DocInfo catalog entry (mirrors an HWPX-sourced/composed doc,
        // which has no HWP raw bytes to pass through).
        doc.doc_properties.raw_data = None;
        for v in &mut doc.doc_info.font_faces {
            for f in v {
                f.raw_data = None;
            }
        }
        for e in &mut doc.doc_info.char_shapes {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.para_shapes {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.border_fills {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.tab_defs {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.numberings {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.bullets {
            e.raw_data = None;
        }
        for e in &mut doc.doc_info.styles {
            e.raw_data = None;
        }
        for s in &mut doc.sections {
            s.raw_stream = None;
        }
        let out = kdsnr_hwp_parser::serializer::serialize_hwp(&doc).expect("serialize");
        std::fs::write("/tmp/frommodel.hwp", &out).expect("write");
        eprintln!("wrote /tmp/frommodel.hwp ({} bytes)", out.len());
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
