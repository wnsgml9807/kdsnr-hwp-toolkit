//! End-to-end measurement over a real original, from stored layout geometry.

use kdsnr_hwp_doc::normalize;
use kdsnr_hwp_layout::{measure_document, BlockKind};
use kdsnr_hwp_parser::parse_document;

fn original(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templet/original")
        .join(name)
}

#[test]
fn measures_every_paragraph_block() {
    let data = std::fs::read(original("math_input_sample_2.hwpx")).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);

    let measured = measure_document(&model);

    assert_eq!(measured.sections.len(), model.sections.len());

    for (m_sec, d_sec) in measured.sections.iter().zip(&model.sections) {
        // Exactly one paragraph block per paragraph; tables add extra blocks.
        let para_blocks = m_sec
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph)
            .count();
        assert_eq!(para_blocks, d_sec.paragraphs.len());
        assert!(m_sec.blocks.len() >= d_sec.paragraphs.len());

        // Every block stacks contiguously from the body top with a measured size.
        // A paragraph's flow advance is its text box plus any object band above
        // and below it; an absolute (Paper/Page-anchored) block sits in the flow
        // cursor but does not advance it.
        let mut expected_y = m_sec.body_rect.y.raw();
        for block in &m_sec.blocks {
            assert_eq!(block.bounds.y.raw(), expected_y);
            assert!(block.bounds.height.raw() >= 0);
            assert!(block.bounds.width.raw() > 0);
            if !block.absolute {
                expected_y += block.leading_band.raw()
                    + block.bounds.height.raw()
                    + block.trailing_band.raw();
            }
        }
    }

    let total_blocks: usize = measured.sections.iter().map(|s| s.blocks.len()).sum();
    assert!(total_blocks > 0);
}

#[test]
fn measures_tables_in_social_original() {
    let data = std::fs::read(original("social_test_input_2.hwpx")).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);

    let model_tables: usize = model
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .map(|p| p.tables.len())
        .sum();
    // A treat-as-char table flows inline (its space is already in the paragraph's
    // line segments); only a floating table reserves its own measured block.
    let floating_tables: usize = model
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.tables)
        .filter(|t| t.anchor.is_some())
        .count();
    let table_blocks: usize = measured
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter(|b| b.kind == BlockKind::Table)
        .count();
    eprintln!(
        "social: model_tables={model_tables} floating={floating_tables} table_blocks={table_blocks}"
    );
    assert!(model_tables > 0, "social original has tables");
    assert_eq!(
        table_blocks, floating_tables,
        "only floating tables become measured blocks; treat-as-char tables flow inline"
    );

    // Every measured table block (floating) has a positive measured height.
    for block in measured
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter(|b| b.kind == BlockKind::Table)
    {
        assert!(block.bounds.height.raw() > 0);
    }
}
