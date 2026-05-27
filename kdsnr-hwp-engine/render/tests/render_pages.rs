//! End-to-end render: hwpx -> parse -> normalize -> measure -> paginate ->
//! lower -> SVG. Structural assertions plus, when run with `--ignored`, SVG
//! files written to the toolkit debug dir for visual comparison against GT.

use kdsnr_hwp_doc::normalize;
use kdsnr_hwp_font::{advance_of, CharMetrics, FontResolver};
use kdsnr_hwp_layout::{measure_document, paginate_document};
use kdsnr_hwp_paint::{components_json, lower, PaintOp};
use kdsnr_hwp_parser::parse_document;
use kdsnr_hwp_render::page_to_svg;

fn original(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templet/original")
        .join(name)
}

fn resolver() -> Option<FontResolver> {
    // Isolate mode: the bundled `.fonts` directory is the sole font source — no
    // system / Hancom-app fallback, so resolution is identical on any platform
    // (Linux deploy included). `.fonts` holds the canonical TTFs + FontMap.dat +
    // extra_fontmap.ini + hftinfo.dat.
    let fonts = std::env::var("FONT_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.fonts"));
    if !fonts.exists() {
        return None;
    }
    let hi = fonts.join("hftinfo.dat");
    let fontmap = fonts.join("FontMap.dat");
    let extra_map = fonts.join("extra_fontmap.ini");
    let mut maps: Vec<&std::path::Path> = Vec::new();
    if fontmap.exists() {
        maps.push(&fontmap);
    }
    if extra_map.exists() {
        maps.push(&extra_map);
    }
    let mut r = FontResolver::with_dirs(&[&fonts], &hi, &maps).ok()?;
    // HFT-typed faces (신명 중명조, 한양신명조, …) take their advance + outline
    // from Hancom's own .HFT fonts under `HANCOM_PATH`/Fonts (the Hancom Office
    // shared root; default: the macOS install); silently skipped when absent.
    let hancom = std::env::var("HANCOM_PATH").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(
            "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared",
        )
    });
    let hft_dir = hancom.join("Fonts");
    if hft_dir.exists() {
        let _ = r.load_hft_dir(&hft_dir);
    }
    Some(r)
}

fn debug_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../work/debug/render")
}

/// The `.fonts` isolate directory: the sole, deterministic font source.
fn fonts_dir() -> std::path::PathBuf {
    std::env::var("FONT_DIR")
        .map(std::path::PathBuf::from)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.fonts"))
}

fn paint_document(name: &str) -> kdsnr_hwp_paint::PaintDocument {
    let data = std::fs::read(original(name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    // Deployment gate: every face the document draws must have a file in `.fonts`.
    // Render aborts with the operator error table when any font file is missing.
    if let Err(table) = kdsnr_hwp_doc::check_fonts(&model, name, &fonts_dir()) {
        panic!("{table}");
    }
    let measured = measure_document(&model);
    let pagination = paginate_document(&measured).expect("paginate");
    lower(&model, &pagination)
}

/// Hanging indent: a paragraph's left margin applies to every line and a
/// negative first-line indent outdents line 0, so a later line sits right of
/// the first. (Stored segments hold `horzpos=0`; paint adds margin+indent.)
#[test]
fn hanging_indent_outdents_first_line() {
    use kdsnr_hwp_core::SourceRef;
    let painted = paint_document("korean.hwpx");
    let mut by_para: std::collections::HashMap<SourceRef, Vec<&kdsnr_hwp_paint::TextLine>> =
        std::collections::HashMap::new();
    for page in &painted.pages {
        for op in &page.ops {
            if let PaintOp::TextLine(l) = op {
                if matches!(l.source, SourceRef::Paragraph(_)) {
                    by_para.entry(l.source).or_default().push(l);
                }
            }
        }
    }
    let hanging = by_para
        .values()
        .filter(|ls| ls.len() >= 2)
        .any(|ls| ls[1..].iter().any(|l| l.x > ls[0].x + 200));
    assert!(hanging, "no paragraph shows a hanging indent (margin_left/intent dropped)");
}

/// Connected paragraph border box: a passage box spanning many paragraphs emits
/// one tall vertical edge (far taller than any single table cell row).
#[test]
fn paragraph_border_box_spans_paragraphs() {
    let painted = paint_document("korean.hwpx");
    let tall = painted.pages.iter().flat_map(|p| &p.ops).any(|op| {
        matches!(op, PaintOp::Line { x1, y1, x2, y2, .. }
            if (x1 - x2).abs() < 50 && (y1 - y2).abs() > 40_000)
    });
    assert!(tall, "no tall paragraph border box edge (passage box dropped)");
}

/// Tabs carry Hancom's stored `<hp:tab width>` as their advance (baked, like
/// linesegs) — not a half-em fallback. A tab-only run must survive run-flush.
#[test]
fn tabs_carry_stored_width() {
    let painted = paint_document("math.hwpx");
    let mut tab_runs = 0;
    let mut max_w = 0;
    for page in &painted.pages {
        for op in &page.ops {
            if let PaintOp::TextLine(l) = op {
                for run in &l.runs {
                    let n_tab = run.text.matches('\t').count();
                    // Every '\t' in a surviving run has a recorded width.
                    assert_eq!(run.tab_widths.len(), n_tab,
                        "tab_widths count mismatch in run {:?}", run.text);
                    if n_tab > 0 {
                        tab_runs += 1;
                        max_w = max_w.max(run.tab_widths.iter().copied().max().unwrap_or(0));
                    }
                }
            }
        }
    }
    assert!(tab_runs > 0, "math.hwpx has tabs but none survived to paint runs");
    assert!(max_w > 1000, "tab widths look like half-em fallback, not stored width (max={max_w})");
}

#[test]
fn lowers_pages_with_content() {
    let painted = paint_document("science.hwpx");
    assert_eq!(painted.pages.len(), 7);
    // Every page has a white background fill first.
    for page in &painted.pages {
        assert!(matches!(page.ops.first(), Some(PaintOp::FillRect { .. })));
    }
    // Body pages carry text lines; the endnote page (last) too.
    let text_lines = |p: &kdsnr_hwp_paint::PaintPage| {
        p.ops
            .iter()
            .filter(|o| matches!(o, PaintOp::TextLine(_)))
            .count()
    };
    assert!(text_lines(&painted.pages[0]) > 0, "first page has text");
    assert!(
        text_lines(painted.pages.last().unwrap()) >= 25,
        "endnote page has the 25 endnote lines"
    );
}

#[test]
fn svg_is_well_formed() {
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    let painted = paint_document("math_input_sample_2.hwpx");
    let svg = page_to_svg(&painted.pages[0], 96.0, &r);
    assert!(svg.starts_with("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    // Glyph rendering emits resolved outlines as <path>, not font-name <text>.
    assert!(svg.contains("<path"));
}

/// Dump SVGs for every sample to the debug dir, rendered through the canonical
/// glyph path (resolved HFT/TTF outlines at 자간/장평 advances). GT-paired samples
/// feed work/pixel_harness.py; components.json feeds the GT component diff.
#[test]
#[ignore]
fn dump_svgs() {
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    let dir = debug_dir();
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in [
        "science.hwpx",
        "math.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science_input_example.hwpx",
        "science_input_example_2.hwpx",
        "social.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
        "korean.hwpx",
        "국어_박스, 밑줄, 묶음.hwpx",
        "국어_박스, 밑줄, 묶음 복사본.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let painted = paint_document(name);
        let stem = name.trim_end_matches(".hwpx");
        for (i, page) in painted.pages.iter().enumerate() {
            let svg = page_to_svg(page, 96.0, &r);
            let path = dir.join(format!("{stem}_p{:02}.svg", i + 1));
            std::fs::write(&path, svg).expect("write svg");
        }
        // Component JSON for the GT diff harness.
        std::fs::write(dir.join(format!("{stem}.components.json")), components_json(&painted))
            .expect("write components");
        eprintln!("{name}: {} page svg(s) -> {}", painted.pages.len(), dir.display());
    }
}

/// Coarse engine sanity check: 자간/장평 advance vs stored `seg_width` (horzsize).
/// `seg_width` is the segment (column) width, so only column-filling lines have
/// `sum(advance) ≈ seg_width` (short/last lines fill less → low ratio). The top
/// decile captures filling lines; the ±10% band tolerates per-face metric drift
/// across the resolved Hancom TTF set (FontMap.dat now maps 신명/한양/함초롬 to
/// real faces). Precise validation is per-char implied/actual (font crate /
/// advance_fit.py), which is scale-invariant.
#[test]
fn glyph_advance_sum_matches_seg_width() {
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    let mut all = Vec::new();
    let mut resolved_only = Vec::new();
    let (mut chars, mut unresolved) = (0u64, 0u64);
    for name in ["korean.hwpx", "social_test_input_2.hwpx", "science_input_example.hwpx"] {
        if !original(name).exists() {
            continue;
        }
        for page in paint_document(name).pages {
            for op in &page.ops {
                let PaintOp::TextLine(line) = op else { continue };
                if line.seg_width <= 0 {
                    continue;
                }
                let mut sum = 0i64;
                let mut line_unresolved = false;
                for run in &line.runs {
                    let m = CharMetrics {
                        face_resolved: true,
                        ratio: run.ratio,
                        spacing: run.spacing,
                        rel_sz: run.rel_sz,
                        base_size: (run.size_pt * 100.0).round() as i32,
                        bold: run.bold,
                        is_hft: run.is_hft,
                    };
                    for ch in run.text.chars() {
                        chars += 1;
                        match advance_of(&r, &run.font, ch, &m) {
                            Some(a) => sum += a as i64,
                            None => {
                                unresolved += 1;
                                line_unresolved = true;
                            }
                        }
                    }
                }
                let ratio = sum as f64 / line.seg_width as f64;
                all.push(ratio);
                if !line_unresolved {
                    resolved_only.push(ratio);
                }
            }
        }
    }
    assert!(!all.is_empty(), "no text lines measured");
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    resolved_only.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |v: &[f64], p: f64| v[((v.len() as f64 * p) as usize).min(v.len() - 1)];
    let p90 = pct(&resolved_only, 0.90);
    eprintln!(
        "lines={} (resolved {}) chars={chars} unresolved={unresolved} ({:.1}%)\n  all: median={:.3} p90={:.3} | resolved-only: median={:.3} p90={:.3}",
        all.len(), resolved_only.len(),
        100.0 * unresolved as f64 / chars.max(1) as f64,
        pct(&all, 0.5), pct(&all, 0.9), pct(&resolved_only, 0.5), p90,
    );
    // Filling lines (top decile) match the stored segment width within ±10%.
    assert!((0.90..=1.10).contains(&p90), "advance model off on filling lines: p90 {p90:.4}");
}

/// Per-page master selection: social_test_input_2 has one OPTIONAL_PAGE master
/// per physical page (1..4). Each page must get its own master, not page 1's on
/// every page (the bug when all OPTIONAL_PAGE masters collapsed to apply=Both).
#[test]
fn optional_page_masters_differ_per_page() {
    use kdsnr_hwp_layout::FurnitureRole;
    let data = std::fs::read(original("social_test_input_2.hwpx")).expect("read");
    let model = normalize(&parse_document(&data).expect("parse"));
    let pag = paginate_document(&measure_document(&model)).expect("paginate");
    assert_eq!(pag.pages.len(), 4, "social_test_input_2 is 4 pages");
    let sig = |pg: &kdsnr_hwp_layout::PaginatedPage| {
        pg.furniture
            .iter()
            .filter(|f| matches!(f.role, FurnitureRole::MasterPage))
            .flat_map(|f| f.items.iter().map(|it| (it.rect.x.raw(), it.rect.y.raw())))
            .collect::<Vec<_>>()
    };
    let sigs: Vec<_> = pag.pages.iter().map(sig).collect();
    assert!(sigs.iter().all(|s| !s.is_empty()), "every page has a master");
    assert!(sigs[0] != sigs[1], "page 1 and 2 must use different OPTIONAL_PAGE masters");
}

#[test]
fn nested_tables_emit_grid() {
    use kdsnr_hwp_doc::TableInfo;
    let data = std::fs::read(original("science.hwpx")).expect("read");
    let model = normalize(&parse_document(&data).expect("parse"));
    // Recursively count tables in the model: top-level + nested-in-cells.
    fn count(t: &TableInfo) -> usize {
        1 + t.cells.iter()
            .flat_map(|c| &c.paragraphs)
            .flat_map(|p| &p.tables)
            .map(count).sum::<usize>()
    }
    let mut total_tables = 0usize;
    let mut nested = 0usize;
    for sec in &model.sections {
        for para in &sec.paragraphs {
            for t in &para.tables {
                let c = count(t);
                total_tables += c;
                nested += c - 1;
            }
        }
    }
    eprintln!("science tables total(incl nested)={total_tables} nested={nested}");
    assert!(nested > 0, "science should have nested tables in the model");
    // emit_cell recurses into each cell paragraph's nested tables. Count the
    // text lines stored in nested-table cells: the painter must emit at least
    // that many TextLines beyond what it would without recursing (exact
    // placement is Chapter-7 harness work; this guards the data path).
    fn nested_cell_lines(t: &TableInfo) -> usize {
        t.cells
            .iter()
            .flat_map(|c| &c.paragraphs)
            .flat_map(|p| &p.tables)
            .map(|nt| {
                nt.cells
                    .iter()
                    .flat_map(|c| &c.paragraphs)
                    .filter(|p| !p.stored_line_segs.is_empty())
                    .count()
                    + nested_cell_lines(nt)
            })
            .sum()
    }
    let nested_lines: usize = model
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .flat_map(|p| &p.tables)
        .map(nested_cell_lines)
        .sum();
    assert!(nested_lines > 0, "nested cells should carry text lines");
    // Smoke: painting a doc with nested tables succeeds and emits a grid. Exact
    // nested placement is Chapter-7 harness work; emit_cell recursing into
    // para.tables is the data path under guard here.
    let painted = paint_document("science.hwpx");
    let lines = painted
        .pages
        .iter()
        .flat_map(|p| &p.ops)
        .filter(|o| matches!(o, PaintOp::Line { .. }))
        .count();
    eprintln!("science nested_cell_lines={nested_lines} grid_lines={lines}");
    assert!(lines > 100, "table grid not emitted");
}

#[test]
fn objects_emit_images_and_shapes() {
    use kdsnr_hwp_paint::PaintOp;
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    for name in ["social_input_sample.hwpx", "science.hwpx", "social_test_input_2.hwpx"] {
        if !original(name).exists() { continue; }
        let painted = paint_document(name);
        let mut images = 0;
        for page in &painted.pages {
            for op in &page.ops {
                if matches!(op, PaintOp::Image { .. }) { images += 1; }
            }
        }
        eprintln!("{name}: image ops = {images}");
        if images > 0 {
            // The first page SVG must carry an <image> data URI.
            let svg = kdsnr_hwp_render::page_to_svg(&painted.pages[0], 96.0, &r);
            let any_img: bool = painted.pages.iter().any(|p| {
                kdsnr_hwp_render::page_to_svg(p, 96.0, &r).contains("<image")
            });
            assert!(any_img, "{name} has image ops but no <image> in SVG");
            let _ = svg;
            return; // proved end-to-end on at least one sample
        }
    }
    panic!("no sample produced image ops");
}

#[test]
fn equations_emit_glyph_runs() {
    use kdsnr_hwp_paint::PaintOp;
    // math.hwpx is equation-dense; emit_equation lowers each to glyph runs in the
    // equation font (resolved via FontResolver / FONT_DIR at render time).
    let painted = paint_document("math.hwpx");
    let mut eq_fonts = std::collections::BTreeSet::new();
    let mut glyph_runs = 0;
    let mut count = |l: &kdsnr_hwp_paint::TextLine, gr: &mut i32, f: &mut std::collections::BTreeSet<String>| {
        for r in &l.runs {
            if r.font.contains("EQ") || r.font.contains("hwpEQ") || r.font.contains("HYhwp") {
                *gr += 1;
                f.insert(r.font.clone());
            }
        }
    };
    for page in &painted.pages {
        for op in &page.ops {
            if let PaintOp::TextLine(l) = op {
                count(l, &mut glyph_runs, &mut eq_fonts);
                // Inline equations are woven into the line's objects.
                for io in &l.inline_objects {
                    for iop in &io.ops {
                        if let PaintOp::TextLine(el) = iop {
                            count(el, &mut glyph_runs, &mut eq_fonts);
                        }
                    }
                }
            }
        }
    }
    eprintln!("equation glyph runs={glyph_runs} fonts={eq_fonts:?}");
    assert!(glyph_runs > 0, "no equation glyph runs emitted in math.hwpx");
}

#[test]
fn auto_numbers_generated() {
    use kdsnr_hwp_doc::normalize;
    for name in ["social_input_sample.hwpx", "korean.hwpx"] {
        let data = std::fs::read(original(name)).unwrap();
        let model = normalize(&kdsnr_hwp_parser::parse_document(&data).unwrap());
        let nums: Vec<String> = model.sections.iter()
            .flat_map(|s| &s.paragraphs)
            .filter_map(|p| p.auto_number.clone())
            .collect();
        eprintln!("{name}: auto_numbers = {nums:?}");
        assert!(!nums.is_empty(), "{name} should generate heading numbers");
    }
}

/// Probe: dump every TextLine in the top band of a page with its y + text +
/// source, to find duplicated/overlapping furniture (e.g. social header drawn
/// twice). PROBE_DOC selects the doc; defaults to social_input_sample.
#[test]
#[ignore]
fn probe_top_band_textlines() {
    use kdsnr_hwp_core::SourceRef;
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "social_input_sample.hwpx".into());
    let painted = paint_document(&name);
    let page = &painted.pages[0];
    let band = page.paper.height.raw() / 6; // top sixth
    let mut lines: Vec<&kdsnr_hwp_paint::TextLine> = page
        .ops
        .iter()
        .filter_map(|op| match op {
            PaintOp::TextLine(l) if l.top <= band => Some(l),
            _ => None,
        })
        .collect();
    lines.sort_by_key(|l| (l.top, l.x));
    eprintln!("== {name} page1 top-band TextLines (top <= {band}) ==");
    for l in lines {
        let txt: String = l.runs.iter().flat_map(|r| r.text.chars()).take(40).collect();
        let src = match l.source {
            SourceRef::Paragraph(id) => format!("para idx={}", id.index),
            other => format!("{other:?}"),
        };
        eprintln!("  top={:>7} base={:>7} x={:>6} seg_w={:>6} [{src}] {txt:?}", l.top, l.baseline, l.x, l.seg_width);
    }
}

/// Probe: dump near-vertical and near-horizontal Line ops on page 1 (the column
/// divider + header rule), with endpoints, to check their extent vs GT.
#[test]
#[ignore]
fn probe_divider_lines() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "social_input_sample.hwpx".into());
    let painted = paint_document(&name);
    let page = &painted.pages[0];
    eprintln!("== {name} page1 paper={}x{} ==", page.paper.width.raw(), page.paper.height.raw());
    for op in &page.ops {
        if let PaintOp::Line { x1, y1, x2, y2, .. } = op {
            let dx = (x1 - x2).abs();
            let dy = (y1 - y2).abs();
            if dx < 50 && dy > 20_000 {
                eprintln!("  VLINE x={x1:>6} y {y1:>7}..{y2:>7} (len {dy})");
            } else if dy < 50 && dx > 20_000 {
                eprintln!("  HLINE y={y1:>6} x {x1:>7}..{x2:>7} (len {dx})");
            }
        }
    }
}

/// Header/footer/master text is drawText (text inside shapes); it must render.
/// Each furniture definition gets a distinct index base so the right content
/// resolves (a section has several headers with different text).
#[test]
fn furniture_text_renders() {
    use kdsnr_hwp_core::SourceRef;
    let painted = paint_document("science.hwpx");
    let furn_text = painted.pages.iter().flat_map(|p| &p.ops).any(|op| {
        matches!(op, PaintOp::TextLine(l)
            if matches!(l.source, SourceRef::Paragraph(id) if id.index >= 100_000)
                && l.runs.iter().any(|r| !r.text.trim().is_empty()))
    });
    assert!(furn_text, "no header/footer/master text rendered (drawText dropped)");
}

/// Probe: dump raw master/header/footer furniture structure for one sample.
#[test]
#[ignore]
fn probe_furniture_structure() {
    use kdsnr_hwp_parser::model::control::Control;
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "social_test_input_2.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    for (si, sec) in doc.sections.iter().enumerate() {
        let sd = &sec.section_def;
        let pg = &sd.page_def;
        eprintln!("== section {si} ==");
        eprintln!("  page {}x{} margins L{} R{} T{} B{} hdr{} ftr{}",
            pg.width, pg.height, pg.margin_left, pg.margin_right,
            pg.margin_top, pg.margin_bottom, pg.margin_header, pg.margin_footer);
        for (mi, m) in sd.master_pages.iter().enumerate() {
            eprintln!("  MASTER[{mi}] apply={:?} ext={} overlap={} extflags={} tw={} th={} paras={}",
                m.apply_to, m.is_extension, m.overlap, m.ext_flags, m.text_width, m.text_height, m.paragraphs.len());
            dump_paras(&m.paragraphs);
        }
        // headers/footers live as controls in the body paragraphs
        for p in &sec.paragraphs {
            for c in &p.controls {
                match c {
                    Control::Header(h) => { eprintln!("  HEADER apply={:?} tw={} th={} valign={:?} paras={}", h.apply_to, h.text_width, h.text_height, h.vertical_align, h.paragraphs.len()); dump_paras(&h.paragraphs); }
                    Control::Footer(f) => { eprintln!("  FOOTER apply={:?} tw={} th={} valign={:?} paras={}", f.apply_to, f.text_width, f.text_height, f.vertical_align, f.paragraphs.len()); dump_paras(&f.paragraphs); }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
fn dump_paras(paras: &[kdsnr_hwp_parser::model::paragraph::Paragraph]) {
    use kdsnr_hwp_parser::model::control::Control;
    for (pi, p) in paras.iter().enumerate() {
        let txt: String = p.text.chars().take(30).collect();
        let segs: Vec<String> = p.line_segs.iter().map(|s| format!("vp{}:h{}", s.vertical_pos, s.line_height)).collect();
        eprintln!("    para[{pi}] text={:?} segs=[{}]", txt, segs.join(","));
        for c in &p.controls {
            match c {
                Control::Shape(s) => { let co = s.common(); eprintln!("      SHAPE {:?} vrel={:?} valign={:?} hrel={:?} halign={:?} voff={} hoff={} w={} h={} wrap={:?} asChar={}",
                    shape_kind(s), co.vert_rel_to, co.vert_align, co.horz_rel_to, co.horz_align, co.vertical_offset, co.horizontal_offset, co.width, co.height, co.text_wrap, co.treat_as_char); }
                Control::Picture(pic) => { let co = &pic.common; eprintln!("      PICTURE vrel={:?} valign={:?} hrel={:?} voff={} hoff={} w={} h={} asChar={}",
                    co.vert_rel_to, co.vert_align, co.horz_rel_to, co.vertical_offset, co.horizontal_offset, co.width, co.height, co.treat_as_char); }
                Control::Table(t) => { let co = &t.common; eprintln!("      TABLE {}x{} vrel={:?} valign={:?} hrel={:?} halign={:?} voff={} hoff={} w={} h={} asChar={}",
                    t.row_count, t.col_count, co.vert_rel_to, co.vert_align, co.horz_rel_to, co.horz_align, co.vertical_offset, co.horizontal_offset, co.width, co.height, co.treat_as_char); }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
fn shape_kind(s: &kdsnr_hwp_parser::model::shape::ShapeObject) -> &'static str {
    use kdsnr_hwp_parser::model::shape::ShapeObject::*;
    match s { Line(_)=>"Line", Rectangle(_)=>"Rect", Ellipse(_)=>"Ellipse", Arc(_)=>"Arc", Polygon(_)=>"Polygon", Curve(_)=>"Curve", _=>"Other" }
}

/// Probe: report how many distinct document faces resolve to a concrete font
/// (vs fall through to a substitute) with the current resolver + FontMap.
#[test]
#[ignore]
fn probe_face_resolution() {
    use kdsnr_hwp_font::Script;
    let Some(r) = resolver() else { eprintln!("skip: no fonts"); return; };
    let faces = [
        "신명 신그래픽","신명 중고딕","신명 중명조","신명 견명조","한양견명조","한양신명조",
        "한양중고딕","한양견고딕","한양그래픽","함초롬바탕","함초롬돋움","바탕","명조","휴먼편지체",
        "HY헤드라인M","-윤고딕140","-윤명조120","나눔고딕","나눔명조","KoPubWorld돋움체 Light",
        "수식","HY엽서M","신명 디나루","한컴바탕",
    ];
    let (mut ok, mut miss) = (0, 0);
    for f in faces {
        match r.debug_resolve_path(f, Script::Hangul) {
            Some(p) => { ok += 1; eprintln!("  OK   {f:20} -> {}", p.file_name().unwrap().to_string_lossy()); }
            None => { miss += 1; eprintln!("  MISS {f}"); }
        }
    }
    eprintln!("resolved {ok}/{} faces", ok + miss);
}

/// Probe: how inline equations sit in the char stream (control-char markers?).
#[test]
#[ignore]
fn probe_inline_equation_stream() {
    use kdsnr_hwp_parser::model::control::Control;
    let data = std::fs::read(original("math_input_sample.hwpx")).expect("read");
    let doc = parse_document(&data).expect("parse");
    let mut shown = 0;
    for sec in &doc.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            let eqs = p.controls.iter().filter(|c| matches!(c, Control::Equation(_))).count();
            if eqs == 0 { continue; }
            // control-char codes in text (< 0x20)
            let ctrl_codes: Vec<u16> = p.text.encode_utf16().filter(|&u| u < 0x20).collect();
            let preview: String = p.text.chars().map(|c| if (c as u32) < 0x20 { '·' } else { c }).take(40).collect();
            eprintln!("para[{pi}] eqs={eqs} text_len={} ctrl_chars={:?} segs={} controls={}",
                p.text.encode_utf16().count(), ctrl_codes, p.line_segs.len(), p.controls.len());
            eprintln!("   text: {preview:?}");
            for c in &p.controls {
                if let Control::Equation(e) = c {
                    eprintln!("   EQ asChar={} script={:?}", e.common.treat_as_char, e.script.chars().take(30).collect::<String>());
                }
            }
            shown += 1;
            if shown >= 4 { return; }
        }
    }
}

/// Probe: are treat-as-char tables alone in their paragraph (block-like) or
/// interleaved with text on a line (true inline)?
#[test]
#[ignore]
fn probe_inline_tables() {
    use kdsnr_hwp_parser::model::control::Control;
    use kdsnr_hwp_parser::model::paragraph::ParagraphItem;
    for name in ["math_input_sample.hwpx","science_input_example.hwpx","social_test_input_2.hwpx","social_input_sample.hwpx","korean.hwpx","science.hwpx"] {
        if !original(name).exists() { continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let mut asch=0; let mut interleaved=0; let mut alone=0;
        for sec in &doc.sections {
            for p in &sec.paragraphs {
                for c in &p.controls {
                    if let Control::Table(t)=c {
                        if t.common.treat_as_char {
                            asch+=1;
                            let text_chars: usize = p.items.iter().map(|it| match it { ParagraphItem::Text(s)=>s.chars().filter(|c| !c.is_whitespace()).count(), _=>0 }).sum();
                            if text_chars>0 { interleaved+=1; } else { alone+=1; }
                        }
                    }
                }
            }
        }
        eprintln!("{name}: asChar tables={asch} (with non-ws text in para={interleaved}, alone={alone})");
    }
}

/// Probe: for paragraphs holding a treat-as-char table, dump stored linesegs vs
/// table height — does Hancom reserve a table-tall line inside the paragraph,
/// or keep the table out of the paragraph's line flow?
#[test]
#[ignore]
fn probe_inline_table_linesegs() {
    use kdsnr_hwp_parser::model::control::Control;
    let doc_p = |name: &str| parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
    for name in ["korean.hwpx","science.hwpx","math_input_sample.hwpx","social_test_input_2.hwpx"] {
        if !original(name).exists() { continue; }
        let doc = doc_p(name);
        let mut shown = 0;
        for (si, sec) in doc.sections.iter().enumerate() {
            for (pi, p) in sec.paragraphs.iter().enumerate() {
                let tbl: Vec<(i32,i32)> = p.controls.iter().filter_map(|c| match c {
                    Control::Table(t) if t.common.treat_as_char => Some((t.common.width as i32, t.common.height as i32)),
                    _ => None,
                }).collect();
                if tbl.is_empty() { continue; }
                let txt: String = p.text.chars().take(24).collect();
                eprintln!("--- {name} sec{si} para{pi}  tbl(w,h)={:?}  text={:?}", tbl, txt);
                for (li, s) in p.line_segs.iter().enumerate() {
                    eprintln!("    seg[{li}] vpos={} h={} text_start={} col_start={} seg_w={}", s.vertical_pos, s.line_height, s.text_start, s.column_start, s.segment_width);
                }
                shown += 1;
                if shown >= 6 { break; }
            }
            if shown >= 6 { break; }
        }
    }
}

/// Probe: dump the run-order layout of every paragraph that holds a treat-as-char
/// table together with text, to see whether the table sits mid-line.
#[test]
#[ignore]
fn probe_inline_table_runs() {
    use kdsnr_hwp_parser::model::control::Control;
    use kdsnr_hwp_parser::model::paragraph::ParagraphItem;
    for name in ["math_input_sample.hwpx","science_input_example.hwpx","social_test_input_2.hwpx","social_input_sample.hwpx","korean.hwpx","science.hwpx"] {
        if !original(name).exists() { continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        for (si, sec) in doc.sections.iter().enumerate() {
            for (pi, p) in sec.paragraphs.iter().enumerate() {
                let has_asch_tbl = p.controls.iter().any(|c| matches!(c, Control::Table(t) if t.common.treat_as_char));
                if !has_asch_tbl { continue; }
                let text_chars: usize = p.items.iter().map(|it| match it { ParagraphItem::Text(s)=>s.chars().filter(|c| !c.is_whitespace()).count(), _=>0 }).sum();
                if text_chars == 0 { continue; }
                eprintln!("=== {name} sec{si} para{pi}  (text_chars={text_chars}) ===");
                for it in &p.items {
                    match it {
                        ParagraphItem::Text(s) => {
                            let t: String = s.chars().take(40).collect();
                            eprintln!("   TEXT {:?}", t);
                        }
                        ParagraphItem::Control(idx) => {
                            match &p.controls[*idx] {
                                Control::Table(t) => eprintln!("   CTRL[{idx}] TABLE asChar={} rows={} cols={}", t.common.treat_as_char, t.row_count, t.col_count),
                                other => eprintln!("   CTRL[{idx}] {:?}", std::mem::discriminant(other)),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Measure our natural per-char advance sum against Hancom's stored seg_width on
/// lines Hancom packed full (non-last text lines). ratio ~1.0 = our advance
/// matches Hancom's; systematic >1 = we run wide (justify must condense).
#[test]
#[ignore]
fn measure_advance_vs_segwidth() {
    let Some(r) = resolver() else { eprintln!("skip: fonts"); return; };
    for name in ["korean.hwpx","social_test_input_2.hwpx","math_input_sample.hwpx","science.hwpx"] {
        if !original(name).exists() { continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        let mut ratios: Vec<f64> = Vec::new();
        for sec in &model.sections {
            for p in &sec.paragraphs {
                let segs = &p.stored_line_segs;
                let units: Vec<u16> = p.text.encode_utf16().collect();
                let to_vis = |g: usize| -> usize {
                    if p.char_offsets.is_empty() { g } else { p.char_offsets.partition_point(|&o| (o as usize) < g) }
                };
                for j in 0..segs.len() {
                    let is_last = j + 1 == segs.len();
                    if is_last { continue; } // only full lines
                    let start = to_vis(segs[j].text_start as usize).min(units.len());
                    let end = to_vis(segs.get(j+1).map(|n| n.text_start as usize).unwrap_or(usize::MAX)).min(units.len());
                    let seg_w = segs[j].segment_width.raw();
                    if seg_w <= 0 || end <= start { continue; }
                    let mut sum = 0i32;
                    for k in start..end {
                        let ch = char::from_u32(units[k] as u32).unwrap_or(' ');
                        if ch == '\u{0002}' || ch == '\n' { continue; }
                        let c = match p.chars.get(k) { Some(c) => c, None => continue };
                        if ch == '\t' { sum += c.tab_width; continue; }
                        let m = CharMetrics { face_resolved: true, ratio: c.ratio, spacing: c.spacing, rel_sz: c.rel_sz, base_size: (c.font_size_pt*100.0).round() as i32, bold: c.bold, is_hft: c.is_hft };
                        sum += advance_of(&r, &c.font_face, ch, &m).unwrap_or((m.base_size as f64*0.5) as i32);
                    }
                    // Inline objects (treat-as-char equations/tables) occupy box width
                    // in the line too — include so equation/table lines compare to 1.0.
                    for obj in &p.objects {
                        if obj.treat_as_char {
                            if let Some(pos) = obj.inline_pos { let pos = pos as usize; if pos>=start && pos<end { sum += obj.width.raw(); } }
                        }
                    }
                    for t in &p.tables {
                        if t.anchor.is_none() {
                            if let Some(pos) = t.inline_pos { let pos = pos as usize; if pos>=start && pos<end { sum += t.width.raw(); } }
                        }
                    }
                    ratios.push(sum as f64 / seg_w as f64);
                }
            }
        }
        if ratios.is_empty() { eprintln!("{name}: no full lines"); continue; }
        ratios.sort_by(|a,b| a.partial_cmp(b).unwrap());
        let n = ratios.len();
        let mean = ratios.iter().sum::<f64>()/n as f64;
        let med = ratios[n/2];
        let p10 = ratios[n/10]; let p90 = ratios[n*9/10];
        eprintln!("{name}: lines={n} our_sum/seg_w  mean={:.3} median={:.3} p10={:.3} p90={:.3}", mean, med, p10, p90);
    }
}


/// Probe: social inline-table vertical offset + cell para align/segment_width vs cell width.
#[test]
#[ignore]
fn probe_social_inline_and_cell() {
    use kdsnr_hwp_parser::model::control::Control;
    let doc = parse_document(&std::fs::read(original("social_test_input_2.hwpx")).unwrap()).unwrap();
    let mut shown = 0;
    for (pi, p) in doc.sections[0].paragraphs.iter().enumerate() {
        for c in &p.controls {
            if let Control::Table(t) = c {
                if t.common.treat_as_char {
                    let seg0 = p.line_segs.first();
                    eprintln!("INLINE-TBL para{pi}: vOff={} h={} w={} | seg0 vpos={:?} lh={:?} bdist={:?}",
                        t.common.vertical_offset, t.common.height, t.common.width,
                        seg0.map(|s|s.vertical_pos), seg0.map(|s|s.line_height), seg0.map(|s|s.baseline_distance));
                    // first cell with text: dump its para align + seg widths vs cell width
                    for cell in t.cells.iter().take(1) {
                        eprintln!("  cell w={} pad(l,r)={:?},{:?}", cell.width, cell.padding.left, cell.padding.right);
                        for (ci, cp) in cell.paragraphs.iter().enumerate().take(2) {
                            for (si, s) in cp.line_segs.iter().enumerate().take(2) {
                                eprintln!("    cellpara{ci} seg{si}: col_start={} seg_w={} text_start={} txt={:?}",
                                    s.column_start, s.segment_width, s.text_start, cp.text.chars().take(14).collect::<String>());
                            }
                        }
                    }
                    shown += 1;
                }
            }
        }
        if shown >= 4 { break; }
    }
}

/// Probe: social cell paragraph alignment + advance fit on non-last vs last lines.
#[test]
#[ignore]
fn probe_social_cell_justify() {
    let Some(r) = resolver() else { eprintln!("skip"); return; };
    let doc = parse_document(&std::fs::read(original("social_test_input_2.hwpx")).unwrap()).unwrap();
    let model = normalize(&doc);
    use kdsnr_hwp_parser::model::control::Control;
    let mut shown = 0;
    for (pi, _p) in doc.sections[0].paragraphs.iter().enumerate() {
        // re-find via model paragraph tables
        let mp = &model.sections[0].paragraphs[pi];
        for t in &mp.tables {
            if t.anchor.is_some() { continue; }
            for cell in &t.cells {
                for cp in &cell.paragraphs {
                    if cp.text.chars().filter(|c| !c.is_whitespace()).count() < 8 { continue; }
                    let units: Vec<u16> = cp.text.encode_utf16().collect();
                    let segs = &cp.stored_line_segs;
                    for j in 0..segs.len() {
                        let is_last = j+1==segs.len();
                        let start = segs[j].text_start as usize;
                        let end = segs.get(j+1).map(|n| n.text_start as usize).unwrap_or(units.len()).min(units.len());
                        if end<=start { continue; }
                        let mut sum=0i32;
                        for k in start..end {
                            let ch=char::from_u32(units[k] as u32).unwrap_or(' ');
                            if ch=='\u{0002}'||ch=='\n'{continue}
                            let c=&cp.chars[k];
                            if ch=='\t'{sum+=c.tab_width;continue}
                            let m=CharMetrics{face_resolved:true,ratio:c.ratio,spacing:c.spacing,rel_sz:c.rel_sz,base_size:(c.font_size_pt*100.0).round() as i32,bold:c.bold,is_hft:c.is_hft};
                            sum+=advance_of(&r,&c.font_face,ch,&m).unwrap_or(0);
                        }
                        let sw=segs[j].segment_width.raw();
                        let ratio = sum as f64/sw as f64;
                        eprintln!("p{pi} align={:?} line{j} last={} sum={} seg_w={} ratio={:.3}", cp.align, is_last, sum, sw, ratio);
                        // For over-wide lines, dump per-char advance by class to see
                        // what inflates the sum (Hangul=full-em, so suspect space/punct/Latin).
                        if ratio > 1.03 {
                            for k in start..end {
                                let ch=char::from_u32(units[k] as u32).unwrap_or(' ');
                                if ch=='\u{0002}'||ch=='\n'{continue}
                                let c=&cp.chars[k];
                                let m=CharMetrics{face_resolved:true,ratio:c.ratio,spacing:c.spacing,rel_sz:c.rel_sz,base_size:(c.font_size_pt*100.0).round() as i32,bold:c.bold,is_hft:c.is_hft};
                                let a = if ch=='\t' { c.tab_width } else { advance_of(&r,&c.font_face,ch,&m).unwrap_or(0) };
                                let em = kdsnr_hwp_font::glyph_em(&r,&c.font_face,ch,c.bold,c.is_hft).unwrap_or(-1.0);
                                eprintln!("    {:?} U+{:04X} face={:?} hft={} em={:.3} ratio={} spacing={} relsz={} sz_pt={:.1} adv={}", ch, ch as u32, c.font_face, c.is_hft, em, c.ratio, c.spacing, c.rel_sz, c.font_size_pt, a);
                            }
                        }
                    }
                    shown+=1;
                    if shown>=6 { return; }
                }
            }
        }
    }
}

/// Probe: social body/cell font faces, what TTF they resolve to, and Hangul advance_em.
#[test]
#[ignore]
fn probe_social_fonts() {
    use kdsnr_hwp_font::Script;
    let Some(r) = resolver() else { eprintln!("skip"); return; };
    let doc = parse_document(&std::fs::read(original("social_test_input_2.hwpx")).unwrap()).unwrap();
    let model = normalize(&doc);
    let mut seen = std::collections::BTreeSet::new();
    for p in &model.sections[0].paragraphs {
        for c in &p.chars {
            if seen.insert(c.font_face.clone()) {
                let path = r.debug_resolve_path(&c.font_face, Script::Hangul);
                let em = r.resolve(&c.font_face, '가').and_then(|f| f.advance_em('가'));
                eprintln!("face={:?} -> {:?}  가_em={:?}", c.font_face, path.map(|p| p.file_name().map(|f| f.to_string_lossy().to_string())), em);
            }
        }
        for t in &p.tables {
            for cell in &t.cells { for cp in &cell.paragraphs { for c in &cp.chars {
                if seen.insert(c.font_face.clone()) {
                    let path = r.debug_resolve_path(&c.font_face, Script::Hangul);
                    let em = r.resolve(&c.font_face, '가').and_then(|f| f.advance_em('가'));
                    eprintln!("CELL face={:?} -> {:?}  가_em={:?}", c.font_face, path.map(|p| p.file_name().map(|f| f.to_string_lossy().to_string())), em);
                }
            }}}
        }
    }
}

/// Probe: char metric (size/ratio/spacing) distribution per sample body text.
#[test]
#[ignore]
fn probe_char_metrics_dist() {
    for name in ["korean.hwpx","social_test_input_2.hwpx","science.hwpx"] {
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        let mut sizes=std::collections::BTreeMap::new();
        let mut ratios=std::collections::BTreeMap::new();
        let mut spacings=std::collections::BTreeMap::new();
        let mut tally=|p:&kdsnr_hwp_doc::ParagraphModel| {
            for c in &p.chars {
                *sizes.entry((c.font_size_pt*10.0) as i32).or_insert(0)+=1;
                *ratios.entry(c.ratio).or_insert(0)+=1;
                *spacings.entry(c.spacing).or_insert(0)+=1;
            }
        };
        for p in &model.sections[0].paragraphs {
            tally(p);
            for t in &p.tables { for cell in &t.cells { for cp in &cell.paragraphs { tally(cp); } } }
        }
        let top=|m:&std::collections::BTreeMap<i32,i32>| { let mut v:Vec<_>=m.iter().collect(); v.sort_by_key(|(_,c)| -**c); v.into_iter().take(3).map(|(k,c)|format!("{}:{}",k,c)).collect::<Vec<_>>().join(" ") };
        let topu=|m:&std::collections::BTreeMap<u16,i32>| { let mut v:Vec<_>=m.iter().collect(); v.sort_by_key(|(_,c)| -**c); v.into_iter().take(3).map(|(k,c)|format!("{}:{}",k,c)).collect::<Vec<_>>().join(" ") };
        let topi=|m:&std::collections::BTreeMap<i16,i32>| { let mut v:Vec<_>=m.iter().collect(); v.sort_by_key(|(_,c)| -**c); v.into_iter().take(3).map(|(k,c)|format!("{}:{}",k,c)).collect::<Vec<_>>().join(" ") };
        eprintln!("{name}: size(x10) [{}]  ratio [{}]  spacing [{}]", top(&sizes), topu(&ratios), topi(&spacings));
    }
}

/// Probe: dump codepoints of choice-marker paragraphs (start with ①..⑤ or digit+.)
#[test]
#[ignore]
fn probe_choice_codepoints() {
    let doc = parse_document(&std::fs::read(original("social_test_input_2.hwpx")).unwrap()).unwrap();
    let mut shown=0;
    for p in &doc.sections[0].paragraphs {
        let first = p.text.chars().next().unwrap_or(' ');
        if "①②③④⑤⑥".contains(first) || (first.is_ascii_digit() && p.text.chars().nth(1)==Some('.')) {
            let cps: Vec<String> = p.text.chars().take(8).map(|c| format!("U+{:04X}({})", c as u32, c)).collect();
            eprintln!("{:?}", cps);
            shown+=1; if shown>=8 { break; }
        }
    }
}

/// Probe: every table's width / anchor / column widths + first cell text, to
/// find a box whose border extends too far right (wrong width or position).
#[test]
#[ignore]
fn probe_table_widths() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "social_test_input_2.hwpx".into());
    let doc = parse_document(&std::fs::read(original(&name)).unwrap()).unwrap();
    let model = normalize(&doc);
    for (pi, p) in model.sections[0].paragraphs.iter().enumerate() {
        for t in &p.tables {
            let txt: String = t.cells.first().map(|c| c.paragraphs.iter().flat_map(|q| q.text.chars()).take(18).collect()).unwrap_or_default();
            let colws: Vec<i32> = t.cells.iter().filter(|c| c.row == 0).map(|c| c.width.raw()).collect();
            eprintln!("p{pi} tbl rows={} cols={} width={} anchor={:?} inline_pos={:?} row0_colw={:?} txt={:?}",
                t.rows, t.cols, t.width.raw(), t.anchor.map(|a| (a.horz_rel, a.horz_align, a.h_offset.raw())), t.inline_pos, colws, txt);
            // For multi-col tables: dump every cell + computed col_w sum vs table.width.
            if t.cols > 1 {
                let mut col_w = vec![0i32; t.cols as usize];
                for c in &t.cells {
                    let cs = c.col_span.max(1) as i32;
                    let per = c.width.raw() / cs;
                    for k in 0..cs as usize {
                        if let Some(s) = col_w.get_mut(c.col as usize + k) { *s = (*s).max(per); }
                    }
                    eprintln!("    cell r{} c{} cspan={} rspan={} w={}", c.row, c.col, c.col_span, c.row_span, c.width.raw());
                }
                eprintln!("    => col_w={:?} sum={} (table.width={})", col_w, col_w.iter().sum::<i32>(), t.width.raw());
            }
        }
    }
}

/// Probe: cell/box paragraphs whose first-line indent is negative (hanging) —
/// the left-protrusion suspects. Dumps fli vs stored column_start to see whether
/// Hancom already baked the indent into horzpos.
#[test]
#[ignore]
fn probe_neg_indent_cells() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "social_test_input_2.hwpx".into());
    let doc = parse_document(&std::fs::read(original(&name)).unwrap()).unwrap();
    let model = normalize(&doc);
    let mut dump = |where_: &str, p: &kdsnr_hwp_doc::ParagraphModel| {
        let fli = p.first_line_indent.raw();
        if fli >= 0 { return; }
        let cs: Vec<i32> = p.stored_line_segs.iter().map(|s| s.column_start.raw()).collect();
        eprintln!("{where_} fli={} align={:?} col_start={:?} auto={:?} text={:?}",
            fli, p.align, cs, p.auto_number, p.text.chars().take(20).collect::<String>());
    };
    for (pi, p) in model.sections[0].paragraphs.iter().enumerate() {
        dump(&format!("BODY p{pi}"), p);
        for t in &p.tables {
            for cell in &t.cells {
                for cp in &cell.paragraphs { dump(&format!("CELL p{pi}"), cp); }
            }
        }
        for o in &p.objects {
            if let Some(tb) = &o.text_box {
                for bp in &tb.paragraphs { dump(&format!("BOX p{pi}"), bp); }
            }
        }
    }
}

/// Probe: structure of the <보기> ㄱㄴㄷㄹ items — table cell vs body, auto_number, indent.
#[test]
#[ignore]
fn probe_bogi_markers() {
    let doc = parse_document(&std::fs::read(original("social_test_input_2.hwpx")).unwrap()).unwrap();
    let model = normalize(&doc);
    for (pi, p) in model.sections[0].paragraphs.iter().enumerate() {
        if p.text.contains("사회 전체의 이익") || p.text.starts_with("사회는 고정된") {
            eprintln!("BODY para{pi}: auto={:?} fli={} align={:?} text={:?} seg0(col_start={:?},seg_w={:?},tstart={:?})",
                p.auto_number, p.first_line_indent.raw(), p.align, p.text.chars().take(16).collect::<String>(),
                p.stored_line_segs.first().map(|s|s.column_start.raw()), p.stored_line_segs.first().map(|s|s.segment_width.raw()), p.stored_line_segs.first().map(|s|s.text_start));
        }
        for t in &p.tables {
            for cell in &t.cells {
                for cp in &cell.paragraphs {
                    if cp.text.contains("사회 전체의 이익") || cp.text.starts_with("사회는 고정된") {
                        eprintln!("CELL(para{pi}) cell_w={} pad_l={}: auto={:?} fli={} text={:?} seg0(col_start={:?},seg_w={:?})",
                            cell.width.raw(), cell.padding.left.raw(), cp.auto_number, cp.first_line_indent.raw(), cp.text.chars().take(16).collect::<String>(),
                            cp.stored_line_segs.iter().map(|s|s.column_start.raw()).collect::<Vec<_>>(), cp.stored_line_segs.iter().map(|s|s.segment_width.raw()).collect::<Vec<_>>());
                    }
                }
            }
        }
    }
}

/// Probe: actual painted cell TextRun ratio/spacing/size/font for the <보기> items.
#[test]
#[ignore]
fn probe_painted_cell_runs() {
    use kdsnr_hwp_core::SourceRef;
    let painted = paint_document("social_test_input_2.hwpx");
    let mut shown=0;
    for page in &painted.pages {
        for op in &page.ops {
            if let PaintOp::TextLine(l)=op {
                let t:String=l.runs.iter().flat_map(|r|r.text.chars()).collect();
                if t.contains("사회 전체의 이익") || t.contains("재창조") || t.contains("구성원들은") {
                    if let SourceRef::Control(_)=l.source {
                        for r in &l.runs {
                            eprintln!("CELLRUN font={:?} size_pt={} ratio={} spacing={} rel_sz={} txt={:?}", r.font, r.size_pt, r.ratio, r.spacing, r.rel_sz, r.text.chars().take(12).collect::<String>());
                        }
                        eprintln!("  line x={} seg_w={} align={:?} last={}", l.x, l.seg_width, l.align, l.is_last_line);
                        shown+=1;
                    }
                }
            }
        }
        if shown>=3 { break; }
    }
}

/// Probe: does seg.column_start already encode the first-line outdent? Compare
/// seg0 vs seg1 column_start for a wrapping hanging-indent paragraph.
#[test]
#[ignore]
fn probe_hanging_colstart() {
    for name in ["social_test_input_2.hwpx","korean.hwpx"] {
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        let mut shown=0;
        let mut dump=|p:&kdsnr_hwp_doc::ParagraphModel, where_:&str| {
            if p.first_line_indent.raw()!=0 && p.stored_line_segs.len()>=2 {
                let cs:Vec<i32>=p.stored_line_segs.iter().map(|s|s.column_start.raw()).collect();
                eprintln!("{name} {where_} fli={} cols={:?} text={:?}", p.first_line_indent.raw(), cs, p.text.chars().take(12).collect::<String>());
            }
        };
        for p in &model.sections[0].paragraphs {
            dump(p,"BODY");
            for t in &p.tables { for c in &t.cells { for cp in &c.paragraphs { dump(cp,"CELL"); shown+=1; } } }
            if shown>20 { break; }
        }
    }
}

/// Probe: across all originals, find any bordered-paragraph box (visible border,
/// border_fill_id != 0) whose run spans more than one column or page — the B
/// case where the border must stay open at the column break. Runs both
/// paginators. `cargo test -p kdsnr-hwp-render --test render_pages
/// probe_border_column_split -- --ignored --test-threads=1 --nocapture`
#[test]
#[ignore]
fn probe_border_column_split() {
    use kdsnr_hwp_core::SourceRef;
    let files = [
        "korean.hwpx", "math_input_sample.hwpx", "math_input_sample_2.hwpx",
        "science.hwpx", "science_input_example_2.hwpx", "social_input_sample.hwpx",
        "social_test_input_2.hwpx", "국어_박스, 밑줄, 묶음.hwpx",
    ];
    for greedy in [false, true] {
        eprintln!("=== paginator: {} ===", if greedy { "greedy" } else { "heuristic" });
        for name in files {
            if !original(name).exists() { continue; }
            let data = std::fs::read(original(name)).expect("read");
            let model = normalize(&parse_document(&data).expect("parse"));
            let lookup: std::collections::HashMap<usize, &kdsnr_hwp_doc::ParagraphModel> =
                model.sections.iter().flat_map(|s| &s.paragraphs).map(|p| (p.id.index, p)).collect();
            let bordered = |id: usize| -> Option<u16> {
                let p = lookup.get(&id)?;
                let b = &p.border;
                let vis = b.fill.is_some() || b.left.visible() || b.right.visible()
                    || b.top.visible() || b.bottom.visible();
                (vis && p.border_fill_id != 0).then_some(p.border_fill_id)
            };
            let measured = measure_document(&model);
            // Greedy is the default; force heuristic explicitly when requested.
            if !greedy { std::env::set_var("KDSNR_PAGINATE_HEURISTIC", "1"); }
            let pag = paginate_document(&measured).expect("paginate");
            std::env::remove_var("KDSNR_PAGINATE_HEURISTIC");
            // Walk every body item in page/document order; track each bordered
            // group's distinct (page, column) cells.
            // Replicate paint's consecutive-run grouping: a box is a maximal run
            // of adjacent same-border_fill items at one column-left; report when
            // such a run's items land in >1 (page,column) cell.
            let mut multi = 0;
            let mut total = 0;
            for (pi, page) in pag.pages.iter().enumerate() {
                let mut i = 0usize;
                while i < page.items.len() {
                    let SourceRef::Paragraph(id) = page.items[i].source else { i += 1; continue };
                    let Some(bid) = bordered(id.index) else { i += 1; continue };
                    let mut cells = vec![(pi, page.items[i].column)];
                    let (mut y0, mut y1) = (page.items[i].rect.y.raw(), page.items[i].rect.y.raw() + page.items[i].rect.height.raw());
                    let mut j = i + 1;
                    while j < page.items.len() {
                        let SourceRef::Paragraph(jd) = page.items[j].source else { break };
                        match bordered(jd.index) {
                            Some(b) if b == bid => {
                                cells.push((pi, page.items[j].column));
                                y0 = y0.min(page.items[j].rect.y.raw());
                                y1 = y1.max(page.items[j].rect.y.raw() + page.items[j].rect.height.raw());
                                j += 1;
                            }
                            _ => break,
                        }
                    }
                    total += 1;
                    let distinct: std::collections::BTreeSet<_> = cells.iter().copied().collect();
                    if distinct.len() > 1 {
                        multi += 1;
                        eprintln!("  {name}: box fill {bid} run [{i}..{j}) spans cells {:?} y {y0}..{y1}", distinct);
                    }
                    i = j;
                }
            }
            if multi == 0 {
                eprintln!("  {name}: no consecutive bordered box spans a column boundary ({total} boxes)");
            }
        }
    }
}

/// Probe: dump our painted long border-box edges (the 4 push_edge lines) per
/// page for `PROBE_DOC`, in PDF points (HWPUNIT/100), so each edge can be
/// sampled against the GT pixels. `cargo test -p kdsnr-hwp-render --test
/// render_pages probe_box_edges -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_box_edges() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    if std::env::var_os("GREEDY").is_some() { std::env::set_var("KDSNR_PAGINATE_GREEDY", "1"); }
    let painted = paint_document(&name);
    std::env::remove_var("KDSNR_PAGINATE_GREEDY");
    for (pi, page) in painted.pages.iter().enumerate() {
        for op in &page.ops {
            if let PaintOp::Line { x1, y1, x2, y2, .. } = op {
                let len = ((x1 - x2).abs()).max((y1 - y2).abs());
                if len > 30_000 {
                    let kind = if (x1 - x2).abs() < 50 { "V" } else if (y1 - y2).abs() < 50 { "H" } else { "?" };
                    eprintln!(
                        "  p{pi} {kind} ({:.1},{:.1})->({:.1},{:.1}) pt",
                        *x1 as f64 / 100.0, *y1 as f64 / 100.0, *x2 as f64 / 100.0, *y2 as f64 / 100.0
                    );
                }
            }
        }
    }
}

/// Probe: dump every body paragraph's border_fill_id, visible-edge flags, and
/// text for one `PROBE_DOC`, to see which paragraphs the painter groups into a
/// box. `cargo test -p kdsnr-hwp-render --test render_pages probe_para_borders
/// -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_para_borders() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let model = normalize(&parse_document(&data).expect("parse"));
    for (si, sec) in model.sections.iter().enumerate() {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            let b = &p.border;
            let e = |x: &kdsnr_hwp_doc::BorderEdge| if x.visible() { "1" } else { "0" };
            if p.border_fill_id != 0 || b.fill.is_some() {
                eprintln!(
                    "sec{si} p{pi} bf_id={} L{}R{}T{}B{} fill={:?} segs={} text={:?}",
                    p.border_fill_id, e(&b.left), e(&b.right), e(&b.top), e(&b.bottom),
                    b.fill.is_some(), p.stored_line_segs.len(),
                    p.text.chars().take(24).collect::<String>(),
                );
            }
        }
    }
}

/// Probe: find the painted TextLine(s) containing a query string and print their
/// SourceRef + position, to identify where a piece of text comes from (body
/// paragraph index, furniture, etc.). `QUERY=국어영역 cargo test ... probe_find_text -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_find_text() {
    use kdsnr_hwp_core::SourceRef;
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let q = std::env::var("QUERY").unwrap_or_else(|_| "국어영역".into());
    let painted = paint_document(&name);
    for (pi, page) in painted.pages.iter().enumerate() {
        let mut scan = |op: &PaintOp, where_: &str| {
            if let PaintOp::TextLine(l) = op {
                let t: String = l.runs.iter().flat_map(|r| r.text.chars()).collect();
                if t.contains(&q) {
                    let src = match l.source {
                        SourceRef::Paragraph(id) => format!("Paragraph(sec{},idx{})", id.section.0, id.index),
                        SourceRef::Control(c) => format!("Control(p{}:{})", c.paragraph.index, c.index),
                        other => format!("{other:?}"),
                    };
                    let cols: Vec<String> = l.runs.iter().map(|r| format!("{:?}=#{:02X}{:02X}{:02X}", r.text.chars().take(6).collect::<String>(), r.color.r, r.color.g, r.color.b)).collect();
                    eprintln!("  p{pi} {where_} src={src} x={} baseline={} top={} txt={:?} runs={:?}", l.x, l.baseline, l.top, t.chars().take(24).collect::<String>(), cols);
                }
            }
        };
        // lower() renders body + furniture into the same `ops`, so this catches
        // furniture text too (its source is the furniture paragraph id).
        for op in &page.ops { scan(op, "ops"); }
    }
}

/// Probe: dump every TextLine on a page within a top-coordinate window, sorted
/// by column (x) then top, to surface paragraphs colliding at the same y.
/// `PAGE=0 TOPMAX=26000 cargo test ... probe_top_lines -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_top_lines() {
    use kdsnr_hwp_core::SourceRef;
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let page = std::env::var("PAGE").ok().and_then(|s| s.parse().ok()).unwrap_or(0usize);
    let topmax: i32 = std::env::var("TOPMAX").ok().and_then(|s| s.parse().ok()).unwrap_or(26000);
    let painted = paint_document(&name);
    let Some(p) = painted.pages.get(page) else { return };
    let mut rows: Vec<(i32, i32, String, String)> = Vec::new();
    for op in &p.ops {
        if let PaintOp::TextLine(l) = op {
            if l.top > topmax { continue; }
            let t: String = l.runs.iter().flat_map(|r| r.text.chars()).collect();
            if t.trim().is_empty() { continue; }
            let src = match l.source {
                SourceRef::Paragraph(id) => format!("P(sec{},idx{})", id.section.0, id.index),
                SourceRef::Control(c) => format!("Ctrl(p{}:{})", c.paragraph.index, c.index),
                other => format!("{other:?}"),
            };
            rows.push((l.x, l.top, src, t.chars().take(28).collect()));
        }
    }
    rows.sort_by_key(|r| (r.0 / 1000 * 1000, r.1));
    for (x, top, src, t) in rows {
        eprintln!("  x={x:>6} top={top:>6} {src:<16} {t:?}");
    }
}

/// Probe: dump master-page paragraphs' objects (kind, anchor rel/align/offset,
/// treat_as_char) and any text-box inner text, to see how the running-title shape
/// is anchored in the model. `cargo test ... probe_master_objects -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_master_objects() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let model = normalize(&parse_document(&std::fs::read(original(&name)).expect("read")).expect("parse"));
    for (si, sec) in model.sections.iter().enumerate() {
        for (mi, m) in sec.master_pages.iter().enumerate() {
            eprintln!("sec{si} master[{mi}] apply={:?} paras={}", m.apply, m.paragraphs.len());
            for (pi, p) in m.paragraphs.iter().enumerate() {
                eprintln!("  para{pi} text={:?} objs={} tbls={}", p.text.chars().take(20).collect::<String>(), p.objects.len(), p.tables.len());
                for (oi, o) in p.objects.iter().enumerate() {
                    let tb: String = o.text_box.as_ref().map(|t| t.paragraphs.iter().flat_map(|q| q.text.chars()).take(16).collect()).unwrap_or_default();
                    eprintln!("    obj{oi} {:?} {}x{} tac={} anchor(vrel={:?} valign={:?} voff={} hrel={:?} halign={:?} hoff={}) tb={:?}",
                        o.kind, o.width.raw(), o.height.raw(), o.treat_as_char,
                        o.anchor.vert_rel, o.anchor.vert_align, o.anchor.v_offset.raw(),
                        o.anchor.horz_rel, o.anchor.horz_align, o.anchor.h_offset.raw(), tb);
                }
            }
        }
    }
}

/// Probe: dump the parser's body paragraphs in document order with text snippet
/// and any ColumnDef control (colCount), to map mid-section column-count regions.
/// `cargo test -p kdsnr-hwp-render --test render_pages probe_col_regions -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_col_regions() {
    use kdsnr_hwp_parser::model::control::Control;
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let doc = parse_document(&std::fs::read(original(&name)).expect("read")).expect("parse");
    for (si, sec) in doc.sections.iter().enumerate() {
        eprintln!("== section {si}: {} body paras ==", sec.paragraphs.len());
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            let cols: Vec<u16> = p
                .controls
                .iter()
                .filter_map(|c| match c {
                    Control::ColumnDef(cd) => Some(cd.column_count),
                    _ => None,
                })
                .collect();
            let txt: String = p.text.chars().take(28).collect();
            let mark = if cols.is_empty() { String::new() } else { format!(" [ColumnDef colCount={cols:?}]") };
            if !cols.is_empty() || pi < 14 {
                eprintln!("  p{pi}{mark} align? text={txt:?}");
            }
        }
    }
}

/// Probe: per-document face -> resolved file using the isolated .fonts dir.
/// Builds the canonical face->file manifest and flags any unresolved face.
#[test]
#[ignore]
fn probe_font_manifest() {
    use kdsnr_hwp_font::Script;
    let Some(r) = resolver() else { eprintln!("skip: .fonts missing"); return; };
    let docs = ["korean.hwpx","math_input_sample.hwpx","math_input_sample_2.hwpx","science.hwpx",
        "science_input_example.hwpx","science_input_example_2.hwpx","social_input_sample.hwpx","social_test_input_2.hwpx"];
    let mut missing = 0;
    for name in docs {
        if !original(name).exists() { continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        for face in kdsnr_hwp_doc::required_faces(&model) {
            // What the resolver actually picks, trying Hangul then Latin script.
            let file = [Script::Hangul, Script::Latin].iter().find_map(|&s| {
                r.debug_resolve_path(&face, s)
                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
            });
            match file {
                Some(f) => eprintln!("{name}\t{face}\t{f}"),
                None => { eprintln!("{name}\t{face}\t<MISSING>"); missing += 1; }
            }
        }
    }
    eprintln!("=== unresolved faces: {missing} ===");
}

/// Probe: render the operator missing-font error table for all samples using
/// the bundled .fonts manifest (demonstrates the deployment check output).
#[test]
#[ignore]
fn probe_font_error_table() {
    let fonts = fonts_dir();
    let manifest = kdsnr_hwp_font::FontManifest::load(&fonts);
    let docs = ["korean.hwpx","math_input_sample.hwpx","science.hwpx",
        "science_input_example.hwpx","social_input_sample.hwpx","social_test_input_2.hwpx"];
    let mut rows = Vec::new();
    for name in docs {
        if !original(name).exists() { continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        // Canonical face set (body + furniture + master pages + endnotes), same
        // as the render-time gate in `paint_document`.
        let faces: Vec<String> = kdsnr_hwp_doc::required_faces(&model).into_iter().collect();
        rows.extend(manifest.missing_for(name, &faces));
    }
    match kdsnr_hwp_font::format_missing_table(&rows) {
        Some(t) => eprintln!("{t}"),
        None => eprintln!("(all fonts present)"),
    }
}

/// Probe: science.hwpx floating objects (anchor + offsets), inline tables (dims
/// vs reserved line height), and header paragraphs — to root-cause the image-at-
/// top, inline-table skew/crush, and header overlap.
#[test]
#[ignore]
fn probe_lines() {
    let name = std::env::var("PROBE_DOC").unwrap_or_else(|_| "science.hwpx".into());
    let doc = parse_document(&std::fs::read(original(&name)).unwrap()).unwrap();
    let model = normalize(&doc);
    for sec in &model.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for o in &p.objects {
                if let kdsnr_hwp_doc::ObjectContent::Line { x1, y1, x2, y2, width, style, .. } = &o.content {
                    let a = &o.anchor;
                    eprintln!("para{pi} LINE ({x1},{y1})->({x2},{y2}) w={} h={} lw={width} style={style:?} vrel={:?} valign={:?} voff={} hrel={:?} halign={:?} hoff={}",
                        o.width.raw(), o.height.raw(), a.vert_rel, a.vert_align, o.v_offset.raw(), a.horz_rel, a.horz_align, o.h_offset.raw());
                }
            }
        }
    }
}

#[test]
#[ignore]
fn probe_science_objects() {
    let name = "science.hwpx";
    let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
    let model = normalize(&doc);
    for sec in &model.sections {
        eprintln!("== HEADERS {} ==", sec.headers.len());
        for h in &sec.headers {
            eprintln!("  -- header apply={:?}", h.apply);
            for (pi, p) in h.paragraphs.iter().enumerate() {
                let segw = p.stored_line_segs.first().map(|s| s.segment_width.raw()).unwrap_or(-1);
                eprintln!("  hdr para{pi} segw={} text={:?}", segw, p.text.chars().take(40).collect::<String>());
                for o in &p.objects {
                    let a=&o.anchor;
                    let tb: String = o.text_box.as_ref().map(|t| t.paragraphs.iter()
                        .flat_map(|p| p.text.chars()).take(14).collect()).unwrap_or_default();
                    eprintln!("     OBJ {:?} w={} h={} vrel={:?} valign={:?} voff={} hrel={:?} halign={:?} haoff={} tb={:?}",
                        o.kind, o.width.raw(), o.height.raw(), a.vert_rel, a.vert_align, a.v_offset.raw(),
                        a.horz_rel, a.horz_align, a.h_offset.raw(), tb);
                }
            }
        }
        eprintln!("== BODY floating objects & inline tables ==");
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for o in &p.objects {
                if o.treat_as_char { continue; }
                let a = &o.anchor;
                let tb: String = o.text_box.as_ref().map(|t| t.paragraphs.iter()
                    .flat_map(|p| p.text.chars()).take(12).collect()).unwrap_or_default();
                let segw = p.stored_line_segs.first().map(|s| s.segment_width.raw()).unwrap_or(-1);
                let cs = p.stored_line_segs.first().map(|s| s.column_start.raw()).unwrap_or(-1);
                if !tb.is_empty() || a.horz_align != kdsnr_hwp_doc::AnchorAlign::Left {
                    eprintln!("  para{pi} {:?} w={} hrel={:?} halign={:?} haoff={} parasegw={} cstart={} tb={:?}",
                        o.kind, o.width.raw(), a.horz_rel, a.horz_align, a.h_offset.raw(), segw, cs, tb);
                }
            }
            for t in &p.tables {
                if t.anchor.is_some() { continue; }
                let lh: Vec<i32> = p.stored_line_segs.iter().map(|s| s.line_height.raw()).collect();
                eprintln!("  para{pi} INLINE TBL w={} h={} rows={} cols={} inline_pos={:?} seglh={:?} txt={:?}",
                    t.width.raw(), t.height.raw(), t.rows, t.cols, t.inline_pos, lh,
                    p.text.chars().take(16).collect::<String>());
            }
        }
    }
}

/// Probe: dump pagination geometry (per-page item rects + furniture) for the
/// reported-broken docs, to find vertical mis-placement / page-break recovery
/// failures at their root.
#[test]
#[ignore]
fn probe_pagination_geom() {
    let docs = std::env::var("PROBE_DOCS").unwrap_or_else(|_|
        "science_input_example_2.hwpx,science_input_example.hwpx,science.hwpx".into());
    for name in docs.split(',') {
        if !original(name).exists() { eprintln!("MISSING {name}"); continue; }
        let doc = parse_document(&std::fs::read(original(name)).unwrap()).unwrap();
        let model = normalize(&doc);
        // Dump raw model segs for first few paragraphs.
        for sec in &model.sections {
            for (pi, p) in sec.paragraphs.iter().enumerate().take(5) {
                eprintln!("  MODEL para{pi} segs={} ntbl={} npic={} text={:?}",
                    p.stored_line_segs.len(), p.tables.len(), p.objects.len(),
                    p.text.chars().take(20).collect::<String>());
                for (li, s) in p.stored_line_segs.iter().enumerate().take(10) {
                    eprintln!("      seg[{li}] vp={} lh={} lsp={} cstart={} segw={} tag=0x{:x}",
                        s.vertical_pos.raw(), s.line_height.raw(), s.line_spacing.raw(),
                        s.column_start.raw(), s.segment_width.raw(), s.tag);
                }
            }
        }
        let measured = measure_document(&model);
        // Dump tall measured blocks with their raw stored line_tops.
        for sec in &measured.sections {
            for b in &sec.blocks {
                if b.bounds.height.raw() > 40000 {
                    let tops: Vec<i32> = b.line_tops.iter().map(|t| t.raw()).collect();
                    eprintln!("  TALL block {:?} {:?} h={} lines={} tops={:?}",
                        b.kind, b.source, b.bounds.height.raw(), b.line_count, tops);
                }
            }
        }
        let pag = paginate_document(&measured).expect("paginate");
        eprintln!("\n======== {name}: {} pages ========", pag.pages.len());
        for pg in &pag.pages {
            eprintln!("-- page {} sec{} paper(w={} h={}) body(x={} y={} w={} h={}) cols={}",
                pg.page.0, pg.section.0,
                pg.paper.width.raw(), pg.paper.height.raw(),
                pg.body.x.raw(), pg.body.y.raw(), pg.body.width.raw(), pg.body.height.raw(),
                pg.columns.len());
            for it in &pg.items {
                eprintln!("   item {:?} {:?} col{} rect(x={} y={} w={} h={}) frags={:?}",
                    it.kind, it.source, it.column, it.rect.x.raw(), it.rect.y.raw(),
                    it.rect.width.raw(), it.rect.height.raw(), it.fragment_range);
            }
            for f in &pg.furniture {
                eprintln!("   furn {:?}: {} items", f.role, f.items.len());
                for it in &f.items {
                    eprintln!("      {:?} rect(x={} y={} w={} h={})",
                        it.kind, it.rect.x.raw(), it.rect.y.raw(), it.rect.width.raw(), it.rect.height.raw());
                }
            }
        }
    }
}

/// Probe: for the integral eq2 script, lower it (engine class-ratio advances),
/// then re-measure each Text primitive's glyphs with the real HYhwpEQ TTF
/// advance to estimate the natural width under real metrics vs stored (6819).
#[test]
#[ignore]
fn probe_equation_real_advance() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let Some(r) = resolver() else { eprintln!("skip: .fonts missing"); return };
    let cases = [
        ("int _{ 0} ^{1} {} LEFT ( 8x ^{`3} +1 RIGHT )`dx`", 1100.0, 6819i32, 2800i32),
        ("y ^{`2} =8x", 1100.0, 3217, 1313),
        ("int _{ 0} ^{k} {}  f(x)`dx`", 1100.0, 4947, 2800),
    ];
    for (s, base, sw, sh) in cases {
        let f = lower_equation_primitives(s, base);
        // Engine glyph-advance total (class ratio) vs real HYhwpEQ TTF advance.
        let mut real_text_w = 0.0f64;
        let mut min_x = f64::MAX; let mut max_x = f64::MIN;
        for p in &f.primitives {
            if let EquationPrimitive::Text { x, text, font_size, dx, .. } = p {
                let mut rx = *x;
                for (gi, ch) in text.chars().enumerate() {
                    let adv_em = r.resolve_glyph("HYhwpEQ", ch, false).and_then(|(tf,_)| tf.advance_em(ch))
                        .or_else(|| r.hft_advance_em("HYhwpEQ", ch));
                    let a = adv_em.unwrap_or(0.5) * font_size * 0.9; // FUN_0003a934 *9/10
                    eprintln!("    ch U+{:04X} fs={:.0} engine_dx={:.0} real_em={:?} real_adv={:.0}",
                        ch as u32, font_size, dx.get(gi).copied().unwrap_or(0.0), adv_em.map(|e| (e*1000.0).round()/1000.0), a);
                    rx += a; real_text_w += a;
                }
                if *x < min_x { min_x = *x; }
                if rx > max_x { max_x = rx; }
            }
        }
        eprintln!("stored {sw}x{sh} | engine natural_w={:.0} | real-advance text span≈{:.0} (min_x {:.0}->max_x {:.0}) sum_real_text={:.0}",
            f.natural_width, max_x - min_x, min_x, max_x, real_text_w);
    }
}

/// Harness: across all original HWPX, compute each equation's natural box (engine)
/// vs Hancom's stored <hp:sz>, and report width/height error distribution. The
/// stored box is Hancom's GT layout result; convergence of natural→stored is the
/// non-pixel correctness metric for the equation layout port.
/// `cargo test -p kdsnr-hwp-render --test render_pages eq_corpus_box_error -- --ignored --nocapture`
#[test]
#[ignore]
fn eq_corpus_box_error() {
    use kdsnr_hwp_equation::lower_equation_primitives;
    use kdsnr_hwp_parser::model::control::Control;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templet/original");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x=="hwpx").unwrap_or(false))
        .collect();
    files.sort();
    let mut wre = Vec::new(); let mut hre = Vec::new(); let mut n = 0u32;
    let mut worst: Vec<(f64,String,i32,i32,f64,f64)> = Vec::new();
    fn scan(paras: &[kdsnr_hwp_parser::model::paragraph::Paragraph],
            out: &mut Vec<(String,u32,i32,i32)>) {
        for p in paras {
            for c in &p.controls {
                match c {
                    Control::Equation(e) => {
                        let w = e.common.width as i32; let h = e.common.height as i32;
                        let fs = if e.font_size>0 { e.font_size } else { 1000 };
                        if w>0 && h>0 { out.push((e.script.clone(), fs, w, h)); }
                    }
                    Control::Table(t) => for cell in &t.cells { scan(&cell.paragraphs, out); },
                    _ => {}
                }
            }
        }
    }
    for f in &files {
        let Ok(data) = std::fs::read(f) else { continue };
        let Ok(doc) = parse_document(&data) else { continue };
        let mut eqs = Vec::new();
        for sec in &doc.sections { scan(&sec.paragraphs, &mut eqs); }
        for (script, fs, sw, sh) in eqs {
            let frag = lower_equation_primitives(&script, fs as f64);
            if frag.natural_height <= 0.0 { continue; }
            let we = (frag.natural_width - sw as f64) / sw as f64;
            let he = (frag.natural_height - sh as f64) / sh as f64;
            wre.push(we); hre.push(he); n += 1;
            worst.push((we, script.chars().take(40).collect(), sw, sh, frag.natural_width, frag.natural_height));
        }
    }
    let stat = |v: &mut Vec<f64>| {
        v.sort_by(|a,b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f64>()/v.len() as f64;
        let absm = v.iter().map(|x| x.abs()).sum::<f64>()/v.len() as f64;
        let med = v[v.len()/2];
        (mean, absm, med, v[0], v[v.len()-1])
    };
    let buckets = |v: &[f64]| {
        let n = v.len() as f64;
        let c = |lo: f64, hi: f64| v.iter().filter(|x| x.abs() >= lo && x.abs() < hi).count() as f64 / n * 100.0;
        format!("≤2%:{:.0} 2-5%:{:.0} 5-10%:{:.0} 10-20%:{:.0} >20%:{:.0}",
            c(0.0,0.02), c(0.02,0.05), c(0.05,0.10), c(0.10,0.20), c(0.20,1e9))
    };
    let wb = buckets(&wre); let hb = buckets(&hre);
    let (wm,wam,wmed,wlo,whi) = stat(&mut wre);
    let (hm,ham,hmed,hlo,hhi) = stat(&mut hre);
    let pct = |v: &Vec<f64>, p: f64| v[((v.len() as f64 * p) as usize).min(v.len()-1)].abs();
    let mut wabs: Vec<f64> = wre.iter().map(|x| x.abs()).collect(); wabs.sort_by(|a,b| a.partial_cmp(b).unwrap());
    let mut habs: Vec<f64> = hre.iter().map(|x| x.abs()).collect(); habs.sort_by(|a,b| a.partial_cmp(b).unwrap());
    eprintln!("n={n} equations");
    eprintln!("WIDTH  err: mean={:+.3} mean|.|={:.3} median={:+.3} range=[{:+.3},{:+.3}]", wm,wam,wmed,wlo,whi);
    eprintln!("  |.| p50={:.3} p75={:.3} p90={:.3} p95={:.3} | {}", pct(&wabs,0.50),pct(&wabs,0.75),pct(&wabs,0.90),pct(&wabs,0.95), wb);
    eprintln!("HEIGHT err: mean={:+.3} mean|.|={:.3} median={:+.3} range=[{:+.3},{:+.3}]", hm,ham,hmed,hlo,hhi);
    eprintln!("  |.| p50={:.3} p75={:.3} p90={:.3} p95={:.3} | {}", pct(&habs,0.50),pct(&habs,0.75),pct(&habs,0.90),pct(&habs,0.95), hb);
    // Width-error mean|.| grouped by construct feature (overlapping), to target ROI.
    let feats: [(&str, &dyn Fn(&str)->bool); 11] = [
        ("plain", &|s:&str| !s.contains(['&','{']) && !s.to_uppercase().contains("SQRT") && !s.contains('^') && !s.contains('_')),
        ("over", &|s:&str| s.contains("over")),
        ("sup/sub", &|s:&str| s.contains('^')||s.contains('_')),
        ("sqrt", &|s:&str| s.to_uppercase().contains("SQRT")||s.to_uppercase().contains("ROOT")),
        ("LEFT(", &|s:&str| s.to_uppercase().contains("LEFT")),
        ("times", &|s:&str| s.to_uppercase().contains("TIMES")),
        ("amp&", &|s:&str| s.contains('&')),
        ("cases/align", &|s:&str| {let u=s.to_uppercase(); u.contains("CASES")||u.contains("EQALIGN")||u.contains("PILE")}),
        ("vec/bar", &|s:&str| {let u=s.to_uppercase(); u.contains("VEC")||u.contains("BAR")}),
        ("int/sum", &|s:&str| {let u=s.to_uppercase(); u.contains("INT ")||u.contains("SUM")||u.contains("PROD")}),
        ("lim", &|s:&str| s.to_uppercase().contains("LIM")),
    ];
    eprintln!("-- width err by feature (signed mean, mean|.|) --");
    for (name, f) in &feats {
        let v: Vec<f64> = worst.iter().filter(|w| f(&w.1)).map(|w| w.0).collect();
        if !v.is_empty() {
            let sm = v.iter().sum::<f64>()/v.len() as f64;
            let am = v.iter().map(|x|x.abs()).sum::<f64>()/v.len() as f64;
            eprintln!("  {:>12}: n={:<4} mean={:+.3} mean|.|={:.3}", name, v.len(), sm, am);
        }
    }
    // Sample plain equations (no scripts/groups) to calibrate the base advance.
    eprintln!("-- sample plain --");
    for (e,s,sw,_sh,nw,_nh) in worst.iter().filter(|w| {let s=&w.1; !s.contains(['&','{']) && !s.to_uppercase().contains("SQRT") && !s.contains('^') && !s.contains('_') && s.len()>3}).take(14) {
        eprintln!("  werr={:+.3} stored_w {sw} natural_w {:.0} {:?}", e, nw, s);
    }
    worst.sort_by(|a,b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
    eprintln!("-- worst 12 by |width err| --");
    for (e,s,sw,sh,nw,nh) in worst.iter().take(12) {
        eprintln!("  werr={:+.2} stored {sw}x{sh} natural {:.0}x{:.0} {:?}", e, nw, nh, s);
    }
}

/// Generator: dump HYhwpEQ glyph advances (em) for ASCII + PUA E000..E0FF + key
/// symbols, as the faithful GetTextExtentExPointW source for the equation port.
#[test]
#[ignore]
fn gen_hyhwpeq_advance_table() {
    let Some(r) = resolver() else { eprintln!("skip"); return };
    let mut rows: Vec<(u32, f64)> = Vec::new();
    // Broad BMP sweep: Latin-1 (×÷°), Greek, punctuation/super-sub, letterlike
    // (℃℉), arrows, and the full math-operator + PUA equation-glyph blocks. Only
    // codepoints the font actually carries are emitted.
    let ranges = [(0x20u32, 0x100u32), (0x0250, 0x0400), (0x2000, 0x2C00), (0xE000, 0xE100)];
    for (lo, hi) in ranges {
        for cp in lo..hi {
            if let Some(ch) = char::from_u32(cp) {
                if let Some(em) = r.resolve_glyph("HYhwpEQ", ch, false).and_then(|(t,_)| t.advance_em(ch)) {
                    rows.push((cp, (em*10000.0).round()/10000.0));
                }
            }
        }
    }
    eprintln!("HYHWPEQ glyphs with advance: {}", rows.len());
    rows.sort_by_key(|r| r.0);
    // Emit a checked-in Rust table: (codepoint, advance per 10000 em). The
    // equation crate looks this up as the faithful GetTextExtentExPointW source
    // (Hancom then applies ×9/10 for the text/char path; see FUN_0003a934).
    let mut src = String::new();
    src.push_str("//! GENERATED by render test `gen_hyhwpeq_advance_table` from .fonts/HYHWPEQ.TTF.\n");
    src.push_str("//! HYhwpEQ glyph advances (per-10000 em) — Hancom's GetTextExtentExPointW source.\n");
    src.push_str("//! Regenerate after a font change; do not hand-edit.\n\n");
    src.push_str("/// (codepoint, advance × 10000 / em), sorted by codepoint for binary search.\n");
    src.push_str(&format!("pub(crate) static HYHWPEQ_ADVANCE: [(u32, u16); {}] = [\n", rows.len()));
    for (cp, em) in &rows {
        src.push_str(&format!("    (0x{:04X}, {}),\n", cp, (em * 10000.0).round() as u16));
    }
    src.push_str("];\n");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../equation/src/hyhwpeq_advance.rs");
    std::fs::write(&out, src).unwrap();
    eprintln!("wrote {} entries to {}", rows.len(), out.display());
}

/// Dump every corpus equation as JSON {script, fs, stored_w, stored_h,
/// natural_w, glyphs:[(cp, baseline, dx_sum)]} for matching against the Frida
/// surface trace (Hancom's actual rendered pen extent). Output: work/debug/eq_corpus_glyphs.json
#[test]
#[ignore]
fn dump_eq_corpus_glyphs() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    use kdsnr_hwp_parser::model::control::Control;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templet/original");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x=="hwpx").unwrap_or(false)).collect();
    files.sort();
    fn scan(paras: &[kdsnr_hwp_parser::model::paragraph::Paragraph], out: &mut Vec<(String,u32,i32,i32)>) {
        for p in paras { for c in &p.controls { match c {
            Control::Equation(e) => { let w=e.common.width as i32; let h=e.common.height as i32;
                let fs=if e.font_size>0 {e.font_size} else {1000};
                if w>0&&h>0 { out.push((e.script.clone(),fs,w,h)); } }
            Control::Table(t)=>for cell in &t.cells { scan(&cell.paragraphs,out); }, _=>{} } } }
    }
    let mut json = String::from("[\n");
    for f in &files {
        let Ok(data)=std::fs::read(f) else {continue};
        let Ok(doc)=parse_document(&data) else {continue};
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let mut eqs=Vec::new();
        for sec in &doc.sections { scan(&sec.paragraphs,&mut eqs); }
        for (script,fs,sw,sh) in eqs {
            let frag=lower_equation_primitives(&script, fs as f64);
            if frag.natural_height<=0.0 {continue}
            let mut glyphs=String::new();
            for p in &frag.primitives { if let EquationPrimitive::Text{x,baseline,text,dx,..}=p {
                let adv:f64=dx.iter().sum();
                for ch in text.chars() {
                    glyphs.push_str(&format!("[{},{:.0},{:.0},{:.1}],", ch as u32, x, baseline, adv));
                }
            }}
            json.push_str(&format!("{{\"doc\":{:?},\"script\":{:?},\"fs\":{},\"sw\":{},\"sh\":{},\"nw\":{:.1},\"nh\":{:.1},\"glyphs\":[{}]}},\n",
                stem, script, fs, sw, sh, frag.natural_width, frag.natural_height, glyphs.trim_end_matches(',')));
        }
    }
    json.push_str("]\n");
    let out=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../work/debug/eq_corpus_glyphs.json");
    std::fs::create_dir_all(out.parent().unwrap()).ok();
    std::fs::write(&out,&json).unwrap();
    eprintln!("wrote {}", out.display());
}

/// Probe: verify the `&` alignment-tab render expansion. Lowers each corpus
/// equation containing one `&`, applies the same slack distribution emit_equation
/// uses (stored_w − scaled natural across the tabs), and reports whether the
/// rendered right edge reaches the stored common.width.
#[test]
#[ignore]
fn probe_eq_tab() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    use kdsnr_hwp_parser::model::control::Control;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templet/original");
    let data = std::fs::read(dir.join("math.hwpx")).unwrap();
    let doc = parse_document(&data).unwrap();
    let mut eqs = Vec::new();
    fn scan(paras: &[kdsnr_hwp_parser::model::paragraph::Paragraph], out: &mut Vec<(String,u32,i32,i32)>) {
        for p in paras { for c in &p.controls { match c {
            Control::Equation(e) => { if e.common.width>0 { out.push((e.script.clone(), if e.font_size>0 {e.font_size} else {1000}, e.common.width as i32, e.common.height as i32)); } }
            Control::Table(t)=>for cell in &t.cells { scan(&cell.paragraphs,out); }, _=>{} } } }
    }
    for sec in &doc.sections { scan(&sec.paragraphs, &mut eqs); }
    let mut shown = 0;
    for (script, fs, sw, sh) in eqs {
        if script.matches('&').count() != 1 { continue; }
        let frag = lower_equation_primitives(&script, fs as f64);
        if frag.natural_height <= 0.0 { continue; }
        let scale = sh as f64 / frag.natural_height;
        let tab_xs: Vec<f64> = frag.primitives.iter().filter_map(|p| match p {
            EquationPrimitive::Text { x, text, .. } if text == "\u{0009}" => Some(*x), _ => None }).collect();
        let slack = (sw as f64 - frag.natural_width * scale).max(0.0);
        let per_tab = if tab_xs.is_empty() { 0.0 } else { slack / tab_xs.len() as f64 };
        // rendered right edge = max over drawn glyphs of (x+adv)*scale + shift
        let mut right = 0.0_f64;
        for p in &frag.primitives {
            if let EquationPrimitive::Text { x, text, dx, .. } = p {
                if text == "\u{0009}" { continue; }
                let before = tab_xs.iter().filter(|&&t| t < *x).count();
                let sh = per_tab * before as f64;
                let adv: f64 = dx.iter().sum();
                right = right.max((x + adv) * scale + sh);
            }
        }
        eprintln!("tabs={} slack={:.0} rendered_right={:.0} stored_w={} (err={:+.3}) {:?}",
            tab_xs.len(), slack, right, sw, (right - sw as f64)/sw as f64, script.chars().take(34).collect::<String>());
        shown += 1;
        if shown >= 12 { break; }
    }
}

/// Probe: per-primitive width breakdown for a few scripts, to localize the
/// systematic +5% width-over. Prints each Text primitive's x, glyph advances,
/// and the implied gap (x - prev_end = inter-atom spacing inserted before it).
#[test]
#[ignore]
fn probe_eq_breakdown() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let scripts = ["f(x)`", "a-b`", "m-M``", "x GEQ 0", "t=0`", "f(a)+f(2a)`"];
    let fs = 1100.0_f64;
    for s in scripts {
        let frag = lower_equation_primitives(s, fs);
        eprintln!("\n=== {:?}  natural_w={:.1} ({:.4}em)", s, frag.natural_width, frag.natural_width/fs);
        let mut prev_end = 0.0_f64;
        for p in &frag.primitives {
            if let EquationPrimitive::Text { x, text, dx, font_size, .. } = p {
                let adv: f64 = dx.iter().sum();
                let gap = x - prev_end;
                eprintln!("  x={:7.1} gap={:+6.1} ({:+.3}em) text={:?} dx_sum={:.1} fs={:.0}",
                    x, gap, gap/fs, text, adv, font_size);
                prev_end = x + adv;
            }
        }
    }
}

#[test]
#[ignore]
fn probe_strut_height() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let fs = 1000.0_f64;
    // box body and progressively-stripped pieces (wrap in BOX to read the rect, or
    // bare to read natural h). Bare: measure y-span of glyphs+rules.
    let bits = [
        ("(가)", "(가)"),
        ("(가)~", "~(가)~"),
        ("sup1", "x ^{`}"),
        ("sup2", "x ^{` ^{`}}"),
        ("sup3", "x ^{` ^{` ^{`}}}"),
        ("sub2", "x _{` _{`}}"),
        ("strut_full", "x _{{}_{}}^{{}^{{}^{}}}"),
        ("body", "~(가)~ _{{}_{}}^{{}^{{}^{}}}"),
    ];
    for (label, script) in bits {
        let frag = lower_equation_primitives(script, fs);
        let (mut top, mut bot) = (f64::MAX, f64::MIN);
        for p in &frag.primitives {
            match p {
                EquationPrimitive::Text { baseline, font_size, text, .. } if !text.trim().is_empty() => {
                    top = top.min(baseline - 0.85 * font_size); bot = bot.max(*baseline);
                }
                EquationPrimitive::Line { y1, .. } => { top = top.min(*y1); bot = bot.max(*y1); }
                _ => {}
            }
        }
        eprintln!("{label:12} natural_h={:.0}({:.3}em) ink_span={:.0}({:.3}em) [{}]", frag.natural_height, frag.natural_height/fs, bot-top, (bot-top)/fs, script);
    }
}

#[test]
#[ignore]
fn probe_cjk_advance() {
    let Some(r) = resolver() else { eprintln!("skip"); return };
    for (fontname, ch) in [("함초롬바탕", '가'), ("함초롬바탕", '나'), ("함초롬바탕", '다'),
                           ("HYhwpEQ", '가'), ("함초롬바탕", '('), ("함초롬바탕", ')')] {
        match r.resolve_glyph(fontname, ch, false) {
            Some((t, _)) => {
                let bbox = t.glyph_bbox_em(ch);
                eprintln!("{fontname} {:?}: advance_em={:?} bbox_y={:?} (h={:?})", ch, t.advance_em(ch), bbox,
                    bbox.map(|(y0,y1)| y1-y0));
            }
            None => eprintln!("{fontname} {:?}: not resolved", ch),
        }
    }
}

#[test]
#[ignore]
fn probe_box_width() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let cases = [
        ("가", "h` LEFT ( x RIGHT ) & =x ^{`4} -2x ^{`2} + {BOX{~(가)~ _{{}_{}}^{{}^{{}^{}}}}}", 11662),
        ("나", "f` prime  LEFT ( x RIGHT ) & = {BOX{~(나)~ _{{}_{}}^{{}^{{}^{}}}}} & TIMES  LEFT ( x+1 RIGHT )", 12096),
        ("다", "g` LEFT ( 2 RIGHT ) & -g` LEFT ( -2 RIGHT ) & =` {BOX{~(다)~ _{{}_{}}^{{}^{{}^{}}}}}", 11742),
    ];
    let fs = 1000.0_f64;
    for (label, script, stored_w) in cases {
        let frag = lower_equation_primitives(script, fs);
        let mut rects: Vec<(f64,f64,f64,f64)> = Vec::new();
        for p in &frag.primitives {
            if let EquationPrimitive::Rectangle { x, y, width, height, .. } = p {
                rects.push((*x, *y, *width, *height));
            }
        }
        eprintln!("({label}) stored_eq_w={stored_w} natural_eq_w={:.0}", frag.natural_width);
        for (x,y,w,h) in &rects {
            eprintln!("    BOX rect: x={:.0} y={:.0} w={:.0}({:.3}em) h={:.0}({:.3}em)", x, y, w, w/fs, h, h/fs);
            // glyphs inside this box
            for p in &frag.primitives {
                if let EquationPrimitive::Text { x: gx, text, dx, font_size, .. } = p {
                    if *gx >= *x && *gx <= *x + *w && !text.trim().is_empty() {
                        let adv: f64 = dx.iter().sum();
                        eprintln!("       glyph x={:.0} adv={:.0}({:.3}em) fs={:.0} {:?}", gx, adv, adv/fs, font_size, text);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore]
fn probe_cases_layout() {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let script = "f(x)= {cases{eqalign{~ {x} over {x-1} ``#\n#\n~3x-a` ^{` ^{` ^{`}}}#\n}&eqalign{~(x<1) pile{it#it}#\n#\n~(1 LEQ  x LEQ  2)` ^{` ^{` ^{`}}}#\n}#~ {x-b} over {sqrt {2x} `-2 _{` _{` _{`}}}}&~(x>2) _{` _{` _{`}}}}}";
    let fs = 1000.0_f64;
    let frag = lower_equation_primitives(script, fs);
    eprintln!("natural w={:.1} h={:.1} baseline={:.1}", frag.natural_width, frag.natural_height, frag.natural_baseline);
    let mut items: Vec<(f64,f64,String,f64)> = Vec::new();
    for p in &frag.primitives {
        match p {
            EquationPrimitive::Text { x, baseline, text, font_size, .. } if !text.trim().is_empty() && text != "\u{0009}" => {
                items.push((*baseline, *x, text.clone(), *font_size));
            }
            EquationPrimitive::Line { x1, y1, x2, .. } => {
                items.push((*y1, *x1, format!("<rule {:.0}..{:.0}>", x1, x2), 0.0));
            }
            _ => {}
        }
    }
    items.sort_by(|a,b| a.0.partial_cmp(&b.0).unwrap().then(a.1.partial_cmp(&b.1).unwrap()));
    for (b,x,t,fsz) in &items {
        eprintln!("  baseline={:7.1}  x={:7.1}  fs={:5.0}  {:?}", b, x, fsz, t);
    }
}

/// Probe: HYhwpEQ ink bbox (em) for the big operators / radical / rule, the
/// glyph extent Hancom's symbol measure reads (FUN_0003ac9c).
#[test]
#[ignore]
fn probe_bigop_glyph_bbox() {
    let Some(r) = resolver() else { eprintln!("skip"); return };
    for (name, ch) in [
        ("integral E05B", '\u{E05B}'),
        ("sum E067", '\u{E067}'),
        ("prod E068", '\u{E068}'),
        ("radical E05C", '\u{E05C}'),
        ("rule E06D", '\u{E06D}'),
        ("arrow E06E", '\u{E06E}'),
        ("paren( E044", '\u{E044}'),
    ] {
        if let Some((t, _)) = r.resolve_glyph("HYhwpEQ", ch, false) {
            match t.glyph_bbox_em(ch) {
                Some((y0, y1)) => {
                    let (x0, x1) = t.glyph_xbbox_em(ch).unwrap_or((0.0, 0.0));
                    eprintln!("{name}: y[{y0:.4},{y1:.4}] h={:.4} x[{x0:.4},{x1:.4}] w={:.4} adv={:.4}", y1 - y0, x1 - x0, t.advance_em(ch).unwrap_or(0.0));
                }
                None => eprintln!("{name}: no bbox"),
            }
        } else {
            eprintln!("{name}: not resolved");
        }
    }
}

/// Generator: render a labelled grid of HYhwpEQ PUA glyphs so big operators
/// (∫ ∑ ∏), radical, and rule glyphs can be identified by codepoint.
#[test]
#[ignore]
fn gen_hyhwpeq_glyph_grid() {
    let Some(r) = resolver() else { eprintln!("skip"); return };
    let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1600" height="1400" viewBox="0 0 1600 1400"><rect width="1600" height="1400" fill="white"/>"#);
    let lo = std::env::var("LO").ok().and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"),16).ok()).unwrap_or(0xE040);
    let mut i = 0u32;
    for cp in lo..lo+160 {
        let ch = char::from_u32(cp).unwrap();
        let Some((tf,_)) = r.resolve_glyph("HYhwpEQ", ch, false) else { continue };
        let Some(d) = tf.outline_svg_em(ch) else { continue };
        let col = (i % 12) as f64; let row = (i / 12) as f64;
        let ox = 30.0 + col*130.0; let oy = 60.0 + row*120.0;
        // glyph: em-space y-up, scale 60px, flip y
        svg.push_str(&format!(r#"<g transform="translate({:.0},{:.0}) scale(60,-60)"><path d="{}" fill="black"/></g>"#, ox, oy, d));
        svg.push_str(&format!(r#"<text x="{:.0}" y="{:.0}" font-size="12" fill="red">{:04X}</text>"#, ox, oy+20.0, cp));
        i += 1;
    }
    svg.push_str("</svg>");
    let out = debug_dir().join("hyhwpeq_grid.svg");
    std::fs::write(&out, svg).unwrap();
    eprintln!("wrote {} glyphs from {:04X} to {}", i, lo, out.display());
}

/// Verify capitals resolve as HYhwpEQ regular-ASCII glyphs (the equation render
/// path now keeps `A`..`Z` as U+0041…, not PUA E000). Confirms each has an outline
/// + advance (no tofu) and contrasts with the old PUA E000 block.
#[test]
#[ignore]
fn probe_capital_glyph_resolution() {
    let Some(r) = resolver() else { eprintln!("skip"); return };
    let mut missing = Vec::new();
    for c in 'A'..='Z' {
        let reg = r.resolve_glyph("HYhwpEQ", c, false);
        let reg_out = reg.as_ref().and_then(|(t, _)| t.outline_svg_em(c)).map(|d| d.len()).unwrap_or(0);
        let reg_adv = reg.as_ref().and_then(|(t, _)| t.advance_em(c)).unwrap_or(0.0);
        let pua = char::from_u32(0xE000 + (c as u32 - 'A' as u32)).unwrap();
        let pua_adv = r.resolve_glyph("HYhwpEQ", pua, false).as_ref().and_then(|(t, _)| t.advance_em(pua)).unwrap_or(0.0);
        if reg.is_none() || reg_out == 0 {
            missing.push(c);
        }
        eprintln!("{c}: reg(U+{:04X}) outline_len={reg_out} adv={reg_adv:.4} | pua(E0{:02X}) adv={pua_adv:.4}", c as u32, c as u32 - 'A' as u32);
    }
    // lowercase + digit sanity (these stay PUA).
    for ch in ['a', 't', '\u{E0E5}', '\u{E034}'] {
        let out = r.resolve_glyph("HYhwpEQ", ch, false).and_then(|(t, _)| t.outline_svg_em(ch)).map(|d| d.len()).unwrap_or(0);
        eprintln!("ref {:?} (U+{:04X}) outline_len={out}", ch, ch as u32);
    }
    assert!(missing.is_empty(), "capitals without HYhwpEQ regular-ASCII outline: {missing:?}");
}

#[test]
#[ignore]
fn probe_rule_glyph_bbox() {
    let Some(r) = resolver() else { return };
    for cp in [0xE06Du32, 0xE05Bu32, 0xE046u32] {
        let ch = char::from_u32(cp).unwrap();
        if let Some((tf,_)) = r.resolve_glyph("HYhwpEQ", ch, false) {
            let adv = tf.advance_em(ch);
            let d = tf.outline_svg_em(ch).unwrap_or_default();
            eprintln!("U+{:04X} adv_em={:?} path_len={} d_head={:.80}", cp, adv, d.len(), d);
        }
    }
}

/// Dump SVGs for individual equation scripts (radicals etc.) in isolation, so the
/// √ sign / vinculum / radicand placement can be inspected without a full page.
#[test]
#[ignore]
fn dump_equation_svgs() {
    let Some(r) = resolver() else {
        eprintln!("skip: Hancom fonts not present");
        return;
    };
    let dir = debug_dir().join("eq");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let samples: &[(&str, &str)] = &[
        ("sqrt2", "sqrt {2}"),
        ("sqrt_x2", "sqrt {x ^2 +1}"),
        ("sqrt_frac", "sqrt {a over b}"),
        ("sqrt_tall", "sqrt {SUM _{k=1} ^n k}"),
        ("nested", "LEFT ( 2 ^{`2- sqrt {2}}` RIGHT ) ^{`2+ sqrt {2}} `"),
        ("eq_ab", "a = b"),
        ("plus_ab", "a + b"),
        ("vec_eq", "vec a = (3, p)"),
        ("vec_dot", "( vec t - vec a ) cdot vec b = 0"),
        ("vec_abs", "LEFT |  vec{`t`}  RIGHT | "),
        // ---- reported problem scripts ----
        ("m2_09_piece", "f(x)= {cases{eqalign{~ {x} over {x-1} ``#\n#\n~3x-a` ^{` ^{` ^{`}}}#\n}&eqalign{~(x<1) pile{it#it}#\n#\n~(1 LEQ  x LEQ  2)` ^{` ^{` ^{`}}}#\n}#~ {x-b} over {sqrt {2x} `-2 _{` _{` _{`}}}}&~(x>2) _{` _{` _{`}}}}}"),
        ("m2_11_bar", "{bar{rmBC}} &= {bar{rmCD}} `"),
        ("m2_11_rmbar", "rmbarAB&<rmbarCD"),
        ("m2_56_box", "h` LEFT ( x RIGHT ) & =x ^{`4} -2x ^{`2} + {BOX{~(가)~ _{{}_{}}^{{}^{{}^{}}}}}"),
        ("m1_20_log", "log _{ ` {1} over {2}  }  ( x+a  ) & LEQ  it -4(x-2) LEQ  {3} over {a} & TIMES  4 ^{`x}"),
        ("m2_11_barAB", "{bar{rm AB}} & ="),
        ("m2_12_box", "2f` LEFT ( x RIGHT ) & +g` prime  LEFT ( x RIGHT ) & =4x ^{`2} +4x"),
        ("m2_21_times", "80timesk`"),
        ("m2_13_bracket", "LEFT . LEFT ( - INF `,``f` LEFT ( k RIGHT )  RIGHT ] ` RIGHT ."),
        ("m2_29_bigparen", "g` LEFT ( x RIGHT ) & = LEFT ( x ^{`2} -4x RIGHT ) f` LEFT ( x RIGHT ) `"),
        ("m2_int_frac", "f` LEFT ( x RIGHT ) & = int _{```2} ^{x} {`} e ^{`t ^{`2} -4t+3} `dt"),
        ("m2_72_piece", "a _{`n+1} & = {cases{eqalign{~2a _{`n} ` ^{` ^{` ^{`}}}#\n}&~#``- {a _{`n}} over {4} ` _{` _{` _{`}}}&~}}"),
        ("m1_27op", "2f` LEFT ( x RIGHT ) & + LEFT ( x-1 RIGHT ) f` prime  LEFT ( x RIGHT ) & =4x ^{`2} +4x`"),
        ("m1_int_fracsup", "{100} over {pi } & TIMES  int _{0} ^{{pi } over {4}}  f  ( x  ) `sec ^{`2}  x`dx`"),
        ("m1_int_fracdenom", "f(x)= {2} over {LEFT ( x ^{`2} +1 RIGHT ) ^{`2}} &+ int _{  0} ^{2}  tf(t)`dt`"),
    ];
    for (name, script) in samples {
        let page = kdsnr_hwp_paint::debug_equation_page(script, "HYhwpEQ", 4000);
        let svg = page_to_svg(&page, 96.0, &r);
        let path = dir.join(format!("{name}.svg"));
        std::fs::write(&path, svg).expect("write svg");
        eprintln!("{name}: {} ops -> {}", page.ops.len(), path.display());
    }
}

/// Probe: stored equation `baseLine` (% of box height) vs our natural baseline
/// ratio — the inline vertical-alignment ground truth.
#[test]
#[ignore]
fn probe_equation_baseline_ratio() {
    use kdsnr_hwp_equation::lower_equation_primitives;
    use kdsnr_hwp_parser::model::control::Control;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templet/original");
    fn scan(paras: &[kdsnr_hwp_parser::model::paragraph::Paragraph], out: &mut Vec<(String,u32,i32,i32,i16)>) {
        for p in paras { for c in &p.controls { match c {
            Control::Equation(e) => { let w=e.common.width as i32; let h=e.common.height as i32;
                let fs=if e.font_size>0 {e.font_size} else {1000};
                if w>0&&h>0 { out.push((e.script.clone(),fs,w,h,e.baseline)); } }
            Control::Table(t)=>for cell in &t.cells { scan(&cell.paragraphs,out); }, _=>{} } } }
    }
    let mut errs=Vec::new();
    for f in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e|e.path())).filter(|p|p.extension().map(|x|x=="hwpx").unwrap_or(false)) {
        let Ok(data)=std::fs::read(&f) else {continue};
        let Ok(doc)=kdsnr_hwp_parser::parse_document(&data) else {continue};
        let mut eqs=Vec::new();
        for sec in &doc.sections { scan(&sec.paragraphs,&mut eqs); }
        for (script,fs,_sw,sh,bl) in eqs {
            if bl<=0 {continue;}
            let frag=lower_equation_primitives(&script,fs as f64);
            if frag.natural_height<=0.0 {continue;}
            let our_ratio = frag.natural_baseline/frag.natural_height*100.0;
            // Page-render scale = stored_h / natural_h; <1 shrinks the whole equation.
            let scale = sh as f64 / frag.natural_height;
            if (scale-1.0).abs()>0.08 || script.contains("log") {
                eprintln!("  HSCALE {:.3} stored_h={} natural_h={:.0} {:?}", scale, sh, frag.natural_height, script.chars().take(46).collect::<String>());
            }
            errs.push((our_ratio - bl as f64, bl, our_ratio, script.chars().take(40).collect::<String>()));
        }
    }
    let n=errs.len() as f64;
    let mean=errs.iter().map(|e|e.0).sum::<f64>()/n;
    let absm=errs.iter().map(|e|e.0.abs()).sum::<f64>()/n;
    eprintln!("n={} baseline ratio err (ours - stored, pct pts): mean={:+.1} mean|.|={:.1}", errs.len(), mean, absm);
    errs.sort_by(|a,b| b.0.abs().partial_cmp(&a.0.abs()).unwrap());
    for (e,bl,our,s) in errs.iter().take(12) { eprintln!("  err={:+.1} stored={} ours={:.1} {:?}", e, bl, our, s); }
}
