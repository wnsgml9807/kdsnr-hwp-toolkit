//! Normalization over a real original.

use kdsnr_hwp_doc::{normalize, LineSpacingKind};
use kdsnr_hwp_parser::parse_document;

fn original(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templet/original")
        .join(name)
}

#[test]
fn normalizes_math_original() {
    let data = std::fs::read(original("math_input_sample_2.hwpx")).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);

    assert!(!model.sections.is_empty());
    let sec = &model.sections[0];
    assert!(sec.body_rect.width.raw() > 0, "body rect has width");
    assert!(sec.body_rect.height.raw() > 0, "body rect has height");
    assert!(!sec.paragraphs.is_empty());

    let para = sec
        .paragraphs
        .iter()
        .find(|p| !p.text.is_empty() && !p.stored_line_segs.is_empty())
        .expect("a non-empty paragraph");

    // One resolved char per UTF-16 code unit.
    assert_eq!(para.chars.len(), para.text.encode_utf16().count());
    let first = &para.chars[0];
    assert!(first.font_size_pt > 0.0, "char has a resolved font size");
    assert!(!first.font_face.is_empty(), "char has a resolved face");

    // Math originals use percent line spacing.
    assert_eq!(para.spacing.kind, LineSpacingKind::Percent);
    assert!(para.spacing.value >= 100);
}
