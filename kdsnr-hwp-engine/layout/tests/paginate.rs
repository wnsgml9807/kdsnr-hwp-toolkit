//! End-to-end pagination over real originals, from stored layout geometry.

use kdsnr_hwp_doc::normalize;
use kdsnr_hwp_layout::{measure_document, paginate_document, pagination_to_json};
use kdsnr_hwp_parser::parse_document;

fn original(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templet/original")
        .join(name)
}

fn paginate(name: &str) -> kdsnr_hwp_layout::PaginationResult {
    let data = std::fs::read(original(name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    paginate_document(&measured).expect("paginate")
}

/// Run both paginators on one original (greedy via the env gate). Returns
/// `(heuristic, greedy)`. The env var is process-global, so the greedy probe
/// tests are `#[ignore]` and meant to run single-threaded
/// (`-- --ignored --test-threads=1 --nocapture`).
fn paginate_both(
    name: &str,
) -> (
    kdsnr_hwp_layout::PaginationResult,
    kdsnr_hwp_layout::PaginationResult,
) {
    let data = std::fs::read(original(name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    // Greedy is the default now; force heuristic explicitly for the comparison.
    std::env::remove_var("KDSNR_PAGINATE_GREEDY");
    std::env::set_var("KDSNR_PAGINATE_HEURISTIC", "1");
    let heuristic = paginate_document(&measured).expect("paginate heuristic");
    std::env::remove_var("KDSNR_PAGINATE_HEURISTIC");
    let greedy = paginate_document(&measured).expect("paginate greedy");
    (heuristic, greedy)
}

/// Probe: across all originals, count stored linesegs where text_height differs
/// from line_height, and where text_height is zero while line_height is not (a
/// blank spacer line). Tells whether switching the flow advance from line_height
/// to text_height is safe. Also checks vp-delta == text_height+spacing within a
/// column. `cargo test ... lineseg_height_audit -- --ignored --nocapture`
#[test]
#[ignore]
fn lineseg_height_audit() {
    for name in [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "science_input_example_2.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
        "국어_박스, 밑줄, 묶음.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let (mut th_ne_lh, mut th0_lh_pos, mut total, mut delta_eq_th, mut delta_checked) =
            (0u32, 0u32, 0u32, 0u32, 0u32);
        for sec in &model.sections {
            for p in &sec.paragraphs {
                let segs = &p.stored_line_segs;
                for (i, s) in segs.iter().enumerate() {
                    total += 1;
                    let (th, lh, sp) = (
                        s.text_height.raw(),
                        s.line_height.raw(),
                        s.line_spacing.raw(),
                    );
                    if th != lh {
                        th_ne_lh += 1;
                    }
                    if th == 0 && lh > 0 {
                        th0_lh_pos += 1;
                    }
                    if let Some(n) = segs.get(i + 1) {
                        let delta = n.vertical_pos.raw() - s.vertical_pos.raw();
                        if delta > 0 {
                            delta_checked += 1;
                            if delta == th + sp {
                                delta_eq_th += 1;
                            }
                        }
                    }
                }
            }
        }
        eprintln!(
            "{name}: segs={total} th!=lh:{th_ne_lh} th==0&lh>0:{th0_lh_pos} | vpDelta==th+sp:{delta_eq_th}/{delta_checked}"
        );
    }
}

/// Probe: for every top-level paragraph, dump its stored linesegs (vp, lh, th,
/// sp), every object's attributes (kind, size, treat_as_char/in_flow/behind,
/// anchor vert_rel/align + v_offset), and the boundary gap from this paragraph's
/// last line vp to the next paragraph's first line vp — so the "+8164" boundary
/// height can be matched to an object property. `DIAG=<file>`.
/// `cargo test ... diag_boundary_height -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_boundary_height() {
    let name = std::env::var("DIAG").unwrap_or_else(|_| "social_test_input_2.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    for sec in &model.sections {
        let paras = &sec.paragraphs;
        for (pi, p) in paras.iter().enumerate() {
            let n = p.stored_line_segs.len();
            let last_vp = p.stored_line_segs.last().map(|s| s.vertical_pos.raw());
            let last_th = p.stored_line_segs.last().map(|s| s.text_height.raw()).unwrap_or(0);
            let last_sp = p.stored_line_segs.last().map(|s| s.line_spacing.raw()).unwrap_or(0);
            let next_first = paras
                .get(pi + 1)
                .and_then(|q| q.stored_line_segs.first())
                .map(|s| s.vertical_pos.raw());
            // Boundary gap and the modelled advance (th+sp).
            let (gap, modelled) = match (last_vp, next_first) {
                (Some(lv), Some(nf)) => (Some(nf - lv), last_th + last_sp),
                _ => (None, last_th + last_sp),
            };
            // Only print paragraphs with objects, or a boundary gap that exceeds
            // the modelled last-line advance (the unsolved case).
            let interesting = !p.objects.is_empty()
                || gap.map(|g| g != modelled && g > 0).unwrap_or(false);
            if !interesting {
                continue;
            }
            eprintln!(
                "p{pi}: segs={n} lastVp={last_vp:?} nextFirstVp={next_first:?} gap={gap:?} modelled(th+sp)={modelled} extra={:?}",
                gap.map(|g| g - modelled)
            );
            for (li, s) in p.stored_line_segs.iter().enumerate() {
                eprintln!(
                    "    ls[{li}] vp={} lh={} th={} sp={} hpos={}",
                    s.vertical_pos.raw(),
                    s.line_height.raw(),
                    s.text_height.raw(),
                    s.line_spacing.raw(),
                    s.column_start.raw()
                );
            }
            for (oi, o) in p.objects.iter().enumerate() {
                eprintln!(
                    "    obj[{oi}] {:?} {}x{} tac={} in_flow={} behind={} mTop={} mBot={} v_off={} anchor(vrel={:?} valign={:?} v_off={}) | reserved(h+mT+mB)={}",
                    o.kind,
                    o.width.raw(),
                    o.height.raw(),
                    o.treat_as_char,
                    o.in_flow,
                    o.behind_text,
                    o.margin.top.raw(),
                    o.margin.bottom.raw(),
                    o.v_offset.raw(),
                    o.anchor.vert_rel,
                    o.anchor.vert_align,
                    o.anchor.v_offset.raw(),
                    o.height.raw() + o.margin.top.raw() + o.margin.bottom.raw(),
                );
            }
            for (ti, t) in p.tables.iter().enumerate() {
                eprintln!(
                    "    tbl[{ti}] {}x{} rows={} anchor={:?}",
                    t.width.raw(),
                    t.height.raw(),
                    t.rows,
                    t.anchor,
                );
            }
        }
    }
}

/// Validation: run the real greedy paginator and, for every body paragraph
/// item, compare its column-relative top (`rect.y - body_top`) to the block's
/// stored first-line vertpos for that run. Where the greedy reproduces Hancom's
/// fill these match exactly. Prints the first divergences per file. `DIAG=<file>`
/// limits to one. `cargo test ... diag_greedy_vs_stored -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_greedy_vs_stored() {
    use kdsnr_hwp_core::SourceRef;
    let only = std::env::var("DIAG").ok();
    for name in [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
        "국어_박스, 밑줄, 묶음.hwpx",
    ] {
        if let Some(o) = &only {
            if o != name {
                continue;
            }
        }
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let measured = measure_document(&model);
        // Stored first-line vertpos per paragraph block (by source paragraph id).
        use std::collections::HashMap;
        let mut stored: HashMap<(usize, usize), Vec<i32>> = HashMap::new();
        for sec in &measured.sections {
            for b in &sec.blocks {
                if let SourceRef::Paragraph(p) = b.source {
                    stored.insert(
                        (p.section.0, p.index),
                        b.line_tops.iter().map(|t| t.raw()).collect(),
                    );
                }
            }
        }
        std::env::set_var("KDSNR_PAGINATE_GREEDY", "1");
        let g = paginate_document(&measured).expect("greedy");
        std::env::remove_var("KDSNR_PAGINATE_GREEDY");
        let (mut checked, mut hit, mut shown) = (0u32, 0u32, 0u32);
        for page in &g.pages {
            let body_top = page.body.y.raw();
            for item in &page.items {
                let SourceRef::Paragraph(p) = item.source else { continue; };
                let Some(tops) = stored.get(&(p.section.0, p.index)) else { continue; };
                let Some(first_top) = tops.get(item.fragment_range.0) else { continue; };
                let greedy_rel = item.rect.y.raw() - body_top;
                checked += 1;
                if greedy_rel == *first_top {
                    hit += 1;
                } else if shown < 12 {
                    shown += 1;
                    eprintln!(
                        "  {name} p{}: greedy_rel={greedy_rel} stored={first_top} diff={} (col{}, frag {}..{})",
                        p.index, greedy_rel - first_top, item.column, item.fragment_range.0, item.fragment_range.1
                    );
                }
            }
        }
        eprintln!("{name}: greedy-vs-stored col-relative top {hit}/{checked} exact, {} pages", g.pages.len());
    }
}

/// Probe: walk the raw parsed top-level paragraphs and print, for each that
/// holds a floating (non-treat-as-char) shape/picture/table, the paragraph's
/// first stored vertpos plus the object's wrap/anchor/size/outMargin — so the
/// footprint-model misses can be matched to a wrap/anchor sub-rule. `DIAG=<file>`.
/// `cargo test ... diag_object_wrap -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_object_wrap() {
    use kdsnr_hwp_parser::model::control::Control;
    use kdsnr_hwp_parser::model::shape::CommonObjAttr;
    let name = std::env::var("DIAG").unwrap_or_else(|_| "social_input_sample.hwpx".into());
    let only_vp: Option<i32> = std::env::var("VP").ok().and_then(|s| s.parse().ok());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let show = |pi: usize, vp: i32, tag: &str, c: &CommonObjAttr| {
        eprintln!(
            "  p{pi} firstVp={vp} {tag} {}x{} tac={} wrap={:?} vrel={:?} valign={:?} vOff={} outM(t={},b={}) | reserve={}",
            c.width, c.height, c.treat_as_char, c.text_wrap, c.vert_rel_to, c.vert_align,
            c.vertical_offset, c.margin.top, c.margin.bottom,
            c.vertical_offset as i32 + c.margin.top as i32 + c.height as i32 + c.margin.bottom as i32,
        );
    };
    for sec in &doc.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            let vp = p.line_segs.first().map(|s| s.vertical_pos).unwrap_or(-1);
            if let Some(want) = only_vp {
                if vp != want {
                    continue;
                }
            }
            for c in &p.controls {
                match c {
                    Control::Shape(s) if !s.common().treat_as_char => show(pi, vp, "shape", s.common()),
                    Control::Picture(pic) if !pic.common.treat_as_char => show(pi, vp, "pic", &pic.common),
                    Control::Table(t) if !t.common.treat_as_char => show(pi, vp, "table", &t.common),
                    Control::Equation(e) if !e.common.treat_as_char => show(pi, vp, "eq", &e.common),
                    _ => {}
                }
            }
        }
    }
}

/// Validation: for every adjacent pair of top-level paragraphs that lies within
/// the same column (next first vp > this first vp, i.e. no reset), check the
/// hypothesised footprint model reproduces the stored boundary gap exactly:
///   gap == max(sum(th+sp over lines), object_reserve) + space_after(cur) + space_before(next)
/// where object_reserve = max over in-flow Para-anchored floating objects of
/// (anchor.v_offset + margin.top + height + margin.bottom). Reports every miss
/// so the model can be proven complete before it is implemented. `DIAG=<file>`.
/// `cargo test ... diag_footprint_model -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_footprint_model() {
    use kdsnr_hwp_doc::AnchorRel;
    let only = std::env::var("DIAG").ok();
    let files = [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "science_input_example_2.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
        "국어_박스, 밑줄, 묶음.hwpx",
    ];
    for name in files {
        if let Some(o) = &only {
            if o != name {
                continue;
            }
        }
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let (mut checked, mut hit, mut misses) = (0u32, 0u32, Vec::new());
        // Object vertical reserve per top-level paragraph, read from the raw doc:
        // only TOP_AND_BOTTOM Para-anchored floating objects reserve a full-width
        // band. Square/Tight/Through let text flow beside (no vertical push);
        // Behind/InFront overlap; Paper/Page anchors are positioned in another
        // frame (handled separately). Keyed [section][paragraph], aligned to the
        // model's top-level paragraph order.
        use kdsnr_hwp_parser::model::control::Control;
        use kdsnr_hwp_parser::model::shape::{CommonObjAttr, TextWrap, VertRelTo};
        // (band height, band top vOffset) for a TOP_AND_BOTTOM Para-anchored
        // floating object; (0, _) otherwise.
        let band = |c: &CommonObjAttr| -> (i32, i32) {
            if c.text_wrap == TextWrap::TopAndBottom
                && !c.treat_as_char
                && c.vert_rel_to == VertRelTo::Para
            {
                (
                    c.vertical_offset as i32 + c.margin.top as i32 + c.height as i32 + c.margin.bottom as i32,
                    c.vertical_offset as i32,
                )
            } else {
                (0, 0)
            }
        };
        // Per paragraph: the tallest band and its vOffset.
        let reserve_map: Vec<Vec<(i32, i32)>> = doc
            .sections
            .iter()
            .map(|s| {
                s.paragraphs
                    .iter()
                    .map(|p| {
                        p.controls
                            .iter()
                            .map(|c| match c {
                                Control::Shape(sh) => band(sh.common()),
                                Control::Picture(pic) => band(&pic.common),
                                Control::Table(t) => band(&t.common),
                                Control::Equation(e) => band(&e.common),
                                _ => (0, 0),
                            })
                            .max()
                            .unwrap_or((0, 0))
                    })
                    .collect()
            })
            .collect();
        let _ = AnchorRel::Para; // (model anchor kept for other probes)
        let text_flow = |p: &kdsnr_hwp_doc::ParagraphModel| -> i32 {
            p.stored_line_segs
                .iter()
                .map(|s| (s.text_height.raw() + s.line_spacing.raw()).max(0))
                .sum()
        };
        let has_real_text = |p: &kdsnr_hwp_doc::ParagraphModel| -> bool {
            p.stored_line_segs.iter().any(|s| s.segment_width.raw() > 0)
        };
        // A paragraph split across a column (an internal vp reset between its own
        // linesegs) cannot be measured by a single first-line gap; that is a break
        // the greedy fill makes, not a footprint failure.
        let internally_split = |p: &kdsnr_hwp_doc::ParagraphModel| -> bool {
            p.stored_line_segs
                .windows(2)
                .any(|w| w[1].vertical_pos.raw() < w[0].vertical_pos.raw())
        };
        for (si, sec) in model.sections.iter().enumerate() {
            let paras = &sec.paragraphs;
            for (pi, p) in paras.iter().enumerate() {
                let Some(this_first) = p.stored_line_segs.first().map(|s| s.vertical_pos.raw()) else { continue; };
                let Some(next) = paras.get(pi + 1) else { continue; };
                let Some(next_first) = next.stored_line_segs.first().map(|s| s.vertical_pos.raw()) else { continue; };
                let gap = next_first - this_first;
                let tf = text_flow(p);
                // A gap shorter than this paragraph's own text means the next
                // paragraph sits in a different column at a higher absolute vp
                // (a column break fell between or inside them); not a footprint.
                if gap <= 0 || gap < tf || internally_split(p) || internally_split(next) {
                    continue;
                }
                let (band_p, voff_p) = reserve_map.get(si).and_then(|s| s.get(pi)).copied().unwrap_or((0, 0));
                let (band_next, voff_next) = reserve_map.get(si).and_then(|s| s.get(pi + 1)).copied().unwrap_or((0, 0));
                // A full-width band at the paragraph top (vOff==0) with real text
                // forces that text below the band (additive: band then text). Any
                // other band (vOff>0, or an empty object-marker paragraph) sits
                // around the first line, so the box bottom is just max(band, text).
                let additive = |band: i32, voff: i32, real: bool| band > 0 && voff == 0 && real;
                // Extent from THIS paragraph's first text line to its box bottom.
                let below = if additive(band_p, voff_p, has_real_text(p)) {
                    0 // band is above the first line; first-line-to-bottom is just text
                } else {
                    (band_p - tf).max(0) // band extends below the (overlapping) text
                };
                // Leading offset of the NEXT paragraph's first text line within its
                // box (text pushed below a top band).
                let above_next = if additive(band_next, voff_next, has_real_text(next)) {
                    band_next
                } else {
                    0
                };
                let predicted = tf + below + p.space_after.raw() + next.space_before.raw() + above_next;
                checked += 1;
                if predicted == gap {
                    hit += 1;
                } else if misses.len() < 25 {
                    misses.push(format!(
                        "p{pi}: gap={gap} pred={predicted} (text={tf} below={below} sa={} sb_next={} above_next={above_next}) diff={}",
                        p.space_after.raw(),
                        next.space_before.raw(),
                        gap - predicted
                    ));
                }
            }
        }
        eprintln!("{name}: footprint model {hit}/{checked} exact");
        for m in &misses {
            eprintln!("    MISS {m}");
        }
    }
}

/// Probe: per top-level paragraph, the para-shape space before/after (÷2 to
/// HWPUNIT) vs the stored vp gap to the next paragraph's first line. Confirms
/// inter-paragraph spacing is the greedy drift. `DIAG=<file>`.
/// `cargo test ... diag_para_spacing -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_para_spacing() {
    let name = std::env::var("DIAG").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    for sec in &model.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            if !(12..=20).contains(&pi) {
                continue;
            }
            let last = p.stored_line_segs.last();
            let first_vp = p.stored_line_segs.first().map(|s| s.vertical_pos.raw());
            let last_bottom = last.map(|s| s.vertical_pos.raw() + s.text_height.raw() + s.line_spacing.raw());
            eprintln!(
                "p{pi}: segs={} firstVp={:?} lastBottom(th+sp)={:?}",
                p.stored_line_segs.len(),
                first_vp,
                last_bottom
            );
        }
    }
    // Histogram of spacing_after across all top-level paragraphs.
    use std::collections::BTreeMap;
    let mut hist: BTreeMap<i32, u32> = BTreeMap::new();
    let mut total_after = 0i64;
    for ps_id in doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .map(|p| p.para_shape_id)
    {
        if let Some(ps) = doc.doc_info.para_shapes.get(ps_id as usize) {
            *hist.entry(ps.spacing_after / 2).or_default() += 1;
            total_after += (ps.spacing_after / 2) as i64;
        }
    }
    let mut hist_b: BTreeMap<i32, u32> = BTreeMap::new();
    let mut total_before = 0i64;
    for ps_id in doc
        .sections
        .iter()
        .flat_map(|s| s.paragraphs.iter())
        .map(|p| p.para_shape_id)
    {
        if let Some(ps) = doc.doc_info.para_shapes.get(ps_id as usize) {
            *hist_b.entry(ps.spacing_before / 2).or_default() += 1;
            total_before += (ps.spacing_before / 2) as i64;
        }
    }
    eprintln!("spacing_after/2 histogram (value: count): {hist:?}  sum={total_after}");
    eprintln!("spacing_before/2 histogram (value: count): {hist_b:?}  sum={total_before}");
}

/// Probe: side-by-side per-block start (page, column) for both paginators on one
/// `DIAG=<file>` original, listing only the blocks where they differ.
/// `cargo test ... diag_greedy_diff -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_greedy_diff() {
    let name = std::env::var("DIAG").unwrap_or_else(|_| "국어_박스, 밑줄, 묶음.hwpx".into());
    let (h, g) = paginate_both(&name);
    use std::collections::BTreeMap;
    let firsts = |r: &kdsnr_hwp_layout::PaginationResult| {
        let mut m: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let cpp = r.pages.first().map(|p| p.columns.len().max(1)).unwrap_or(1);
        for (pi, page) in r.pages.iter().enumerate() {
            for item in &page.items {
                let key = format!("{:?}:{:?}", item.source, item.kind);
                m.entry(key).or_insert((pi, pi * cpp + item.column));
            }
        }
        m
    };
    let hf = firsts(&h);
    let gf = firsts(&g);
    eprintln!(
        "{name}: heuristic {} pages, greedy {} pages",
        h.pages.len(),
        g.pages.len()
    );
    for (k, hv) in &hf {
        let gv = gf.get(k).copied().unwrap_or((999, 999));
        if Some(hv) != gf.get(k) {
            eprintln!("  DIFF {k}: heuristic(pg{},col{}) greedy(pg{},col{})", hv.0, hv.1, gv.0, gv.1);
        }
    }
}

/// Probe: dump measured block heights against the column height (`span`) for one
/// original, plus each block's first stored line top. Tells whether greedy's
/// per-page drift comes from block heights that pack differently than Hancom's
/// stored column resets. `DIAG=<file>` selects the original.
/// `cargo test -p kdsnr-hwp-layout --test paginate diag_greedy_fill -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_greedy_fill() {
    let name = std::env::var("DIAG").unwrap_or_else(|_| "science_input_example_2.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    for (si, sec) in measured.sections.iter().enumerate() {
        let span = sec.body_rect.height.raw();
        eprintln!(
            "section {si}: span(body h)={span} cols/page={} blocks={}",
            sec.columns.len(),
            sec.blocks.len()
        );
        let mut accum = 0i32;
        let mut col = 0usize;
        let mut seen = false;
        let verbose = std::env::var_os("VERBOSE").is_some();
        let cpp = sec.columns.len().max(1);
        for (bi, b) in sec.blocks.iter().enumerate() {
            let kind = format!("{:?}", b.kind);
            let h = b.bounds.height.raw();
            let first_top = b.line_tops.first().map(|t| t.raw()).unwrap_or(-1);
            let (sb, sa) = (b.space_before.raw(), b.space_after.raw());
            if seen {
                use kdsnr_hwp_doc::BreakBefore::*;
                match b.break_before {
                    Page | Section => {
                        let page = col / cpp;
                        col = (page + 1) * cpp;
                        accum = 0;
                        eprintln!("    [forced {:?} break -> col{col}]", b.break_before);
                    }
                    Column | MultiColumn => {
                        col += 1;
                        accum = 0;
                        eprintln!("    [forced {:?} break -> col{col}]", b.break_before);
                    }
                    None => {}
                }
            }
            if accum > 0 {
                accum += sb;
            }
            seen = true;
            // Greedy column fill, mirroring paginate_greedy (incl. spacing).
            if b.line_tops.is_empty() {
                if accum > 0 && accum + h > span {
                    col += 1;
                    accum = 0;
                }
                eprintln!(
                    "  b{bi:>3} {:<28} {kind:<9} h={h:>7} L0 sb={sb} sa={sa} -> col{col} y={accum} (top={first_top})",
                    format!("{:?}", b.source)
                );
                accum += h + sa;
            } else {
                let prev_col = col;
                let start_y = accum;
                let mut detail = String::new();
                for (li, frag) in b.fragments.iter().enumerate() {
                    let fh = frag.raw();
                    if accum > 0 && accum + fh > span {
                        col += 1;
                        accum = 0;
                    }
                    let stored = b.line_tops.get(li).map(|t| t.raw()).unwrap_or(-1);
                    if verbose {
                        detail.push_str(&format!(
                            "\n      L{li} h={fh} greedy(col{col},y{accum}) stored_vp={stored}"
                        ));
                    }
                    accum += fh;
                }
                accum += sa;
                let spans = if col != prev_col { format!(" SPANS {prev_col}->{col}") } else { String::new() };
                eprintln!(
                    "  b{bi:>3} {:<28} {kind:<9} h={h:>7} L={:<3} sb={sb} sa={sa} -> col{prev_col} y={start_y}{spans} (top={first_top}){detail}",
                    format!("{:?}", b.source),
                    b.fragments.len()
                );
            }
        }
        eprintln!("  end: col={col} accum={accum}");
    }
}

/// Probe: compare the greedy paginator to the stored-position heuristic over all
/// originals. The heuristic reproduces Hancom's stored layout where its reset
/// rules hold, so per-page item counts and column assignments are the ground
/// truth here. Prints page-count and placement divergence for inspection.
/// `cargo test -p kdsnr-hwp-layout --test paginate greedy_vs_heuristic -- --ignored --test-threads=1 --nocapture`
#[test]
#[ignore]
fn greedy_vs_heuristic() {
    for name in [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "science_input_example_2.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
        "국어_박스, 밑줄, 묶음.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let (h, g) = paginate_both(name);
        // Per (source, kind) block, the column index of its first item, in each
        // paginator. A mismatch is a different column/page break decision.
        use std::collections::BTreeMap;
        let first_cols = |r: &kdsnr_hwp_layout::PaginationResult| {
            let mut m: BTreeMap<String, usize> = BTreeMap::new();
            for (pi, page) in r.pages.iter().enumerate() {
                for item in &page.items {
                    let key = format!("{:?}:{:?}", item.source, item.kind);
                    m.entry(key)
                        .or_insert(pi * page.columns.len().max(1) + item.column);
                }
            }
            m
        };
        let hc = first_cols(&h);
        let gc = first_cols(&g);
        let mut diff = 0usize;
        for (k, hv) in &hc {
            if let Some(gv) = gc.get(k) {
                if hv != gv {
                    diff += 1;
                }
            }
        }
        eprintln!(
            "{name}: heuristic {} page(s), greedy {} page(s); block start-column diffs: {}/{}",
            h.pages.len(),
            g.pages.len(),
            diff,
            hc.len()
        );
    }
}

#[test]
fn paginates_math_original() {
    let result = paginate("math_input_sample_2.hwpx");
    assert!(!result.pages.is_empty(), "produced at least one page");

    // Pages number sequentially from zero.
    for (i, page) in result.pages.iter().enumerate() {
        assert_eq!(page.page.0, i);
        assert!(!page.columns.is_empty());
        // Every placed item references a real column and sits at or below its
        // top (the stored vertpos is column-relative, so y >= column top) and at
        // or right of its left (a treat-as-char table carries a stored `horzpos`
        // indent, so x >= column left). A block whose lines span a column
        // boundary is anchored at its start column, so its full height may
        // extend past the column bottom.
        for item in &page.items {
            let col = page
                .columns
                .iter()
                .find(|c| c.index == item.column)
                .expect("item references a page column");
            assert!(item.rect.x.raw() >= col.rect.x.raw());
            assert!(item.rect.y.raw() >= col.rect.y.raw());
        }
    }
    // Ground truth: math_input_sample_2.pdf is 7 pages.
    assert_eq!(result.pages.len(), 7, "page count matches the GT PDF");
    eprintln!("math: {} page(s)", result.pages.len());
}

#[test]
fn fragment_ranges_chain_per_block() {
    let result = paginate("social_test_input_2.hwpx");
    assert!(!result.pages.is_empty());

    // For each source block, the fragment ranges of its items must chain:
    // 0..a, a..b, ..., in document order across pages/columns.
    use std::collections::HashMap;
    let mut next_expected: HashMap<String, usize> = HashMap::new();
    for page in &result.pages {
        for item in &page.items {
            let key = format!("{:?}:{:?}", item.source, item.kind);
            let expect = next_expected.entry(key).or_insert(0);
            assert_eq!(
                item.fragment_range.0, *expect,
                "fragment range start is contiguous within a block"
            );
            assert!(item.fragment_range.1 > item.fragment_range.0);
            *expect = item.fragment_range.1;
        }
    }
    // Ground truth: social_test_input_2.pdf is 4 pages. The stored-position
    // (reset-counting) paginator recovers this exactly.
    assert_eq!(result.pages.len(), 4, "page count matches the GT PDF");
    eprintln!("social: {} page(s)", result.pages.len());
}

#[test]
fn page_count_matches_gt() {
    // Page counts the stored-position paginator recovers exactly, vs the GT PDF
    // page count. Guards the column-reset rule (incl. full-column inline tables)
    // and the endnote flow.
    assert_eq!(paginate("math_input_sample.hwpx").pages.len(), 20);
    assert_eq!(paginate("math_input_sample_2.hwpx").pages.len(), 7);
    assert_eq!(paginate("social_input_sample.hwpx").pages.len(), 5);
    assert_eq!(paginate("social_test_input_2.hwpx").pages.len(), 4);
    // science_input_example_2 is content-in-full-column-tables (13 tables stored
    // at vertpos 0); the `top == prev_top` rule recovers its 7 pages. Its 6
    // endnotes share the last page.
    assert_eq!(paginate("science_input_example_2.hwpx").pages.len(), 7);
    // science.hwpx: 6 body pages + a 7th page for 25 endnotes that overflow the
    // full last body page. Also guards the horzpos reset rule: without it, four
    // same-vertpos two-segment lines would inflate the body to 8 pages.
    assert_eq!(paginate("science.hwpx").pages.len(), 7);
    // korean.hwpx is excluded: its GT PDF is a near-empty 1-page export (10 chars
    // of extractable text), while the .hwpx holds 4 full columns (2 pages) of
    // real content. The PDF is not a usable ground truth.
}

/// Manual probe: page count per original. Run with
/// `cargo test -p kdsnr-hwp-layout --test paginate -- --ignored --nocapture`.
#[test]
#[ignore]
fn page_count_survey() {
    for name in [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "science_input_example_2.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let result = paginate(name);
        eprintln!("{name}: {} page(s)", result.pages.len());
    }
}

#[test]
#[ignore]
fn diagnose_structure() {
    use kdsnr_hwp_doc::ObjectKind;
    let name = std::env::var("DIAG").unwrap_or_else(|_| "science_input_example_2.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    fn walk(paras: &[kdsnr_hwp_doc::ParagraphModel], depth: usize, acc: &mut [usize; 4]) {
        for p in paras {
            for o in &p.objects {
                acc[0] += 1;
                if o.in_flow {
                    acc[1] += 1;
                }
                let k = match o.kind {
                    ObjectKind::Picture => "pic",
                    ObjectKind::Shape => "shape",
                    ObjectKind::Equation => "eq",
                };
                eprintln!(
                    "{:indent$}obj {k} {}x{} tac={} in_flow={}",
                    "",
                    o.width.raw(),
                    o.height.raw(),
                    o.treat_as_char,
                    o.in_flow,
                    indent = depth * 2
                );
            }
            for t in &p.tables {
                acc[2] += 1;
                eprintln!(
                    "{:indent$}table {}x{} rows={} cells={}",
                    "",
                    t.width.raw(),
                    "",
                    t.rows,
                    t.cells.len(),
                    indent = depth * 2
                );
                for c in &t.cells {
                    acc[3] += 1;
                    walk(&c.paragraphs, depth + 1, acc);
                }
            }
        }
    }
    for sec in &model.sections {
        eprintln!(
            "section {:?}: body={:?} cols={} paras={}",
            sec.id,
            (sec.body_rect.width.raw(), sec.body_rect.height.raw()),
            sec.columns.count,
            sec.paragraphs.len()
        );
        let mut acc = [0usize; 4];
        walk(&sec.paragraphs, 1, &mut acc);
        eprintln!(
            "  totals: objs={} in_flow={} tables={} cells={}",
            acc[0], acc[1], acc[2], acc[3]
        );
    }
}

/// Compare Hancom's stored page boundaries (lineseg tag bit0 = first-line-of-page)
/// against the engine's page count, for every original.
#[test]
#[ignore]
fn stored_page_breaks() {
    fn count_first_of_page(paras: &[kdsnr_hwp_doc::ParagraphModel], acc: &mut (usize, usize)) {
        for p in paras {
            for ls in &p.stored_line_segs {
                acc.0 += 1;
                if ls.tag & 0x01 != 0 {
                    acc.1 += 1;
                }
            }
            for t in &p.tables {
                for c in &t.cells {
                    count_first_of_page(&c.paragraphs, acc);
                }
            }
        }
    }
    for name in [
        "korean.hwpx",
        "math_input_sample.hwpx",
        "math_input_sample_2.hwpx",
        "science.hwpx",
        "science_input_example_2.hwpx",
        "social_input_sample.hwpx",
        "social_test_input_2.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let mut acc = (0usize, 0usize);
        let paras: Vec<_> = model.sections.iter().flat_map(|s| s.paragraphs.clone()).collect();
        count_first_of_page(&paras, &mut acc);
        let max_vp = model
            .sections
            .iter()
            .flat_map(|s| &s.paragraphs)
            .flat_map(|p| &p.stored_line_segs)
            .map(|ls| ls.vertical_pos.raw())
            .max()
            .unwrap_or(0);
        let body_h = model.sections.first().map(|s| s.body_rect.height.raw()).unwrap_or(0);
        // Engine's measured total stacked content height (single-column proxy).
        let measured = measure_document(&model);
        let engine_total: i32 = measured
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .map(|b| b.bounds.height.raw())
            .sum();
        let engine = paginate(name).pages.len();
        eprintln!(
            "{name}: first_of_page={} hancom_last_vp={} body_h={} engine_total_h={} | engine={engine}",
            acc.1, max_vp, body_h, engine_total
        );
    }
}

/// Inspect actual furniture content (apply scope + paragraph text) to judge
/// whether what we extract/place is real, not just plausible counts.
#[test]
#[ignore]
fn dump_furniture_content() {
    let name = std::env::var("DUMP").unwrap_or_else(|_| "korean.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let snippet = |paras: &[kdsnr_hwp_doc::ParagraphModel]| -> String {
        paras
            .iter()
            .map(|p| p.text.clone())
            .collect::<Vec<_>>()
            .join("|")
            .chars()
            .take(60)
            .collect()
    };
    for (si, s) in model.sections.iter().enumerate() {
        eprintln!("{name} sec{si}: page_h={} body={:?} header_rect={:?} footer_rect={:?}",
            s.page_rect.height.raw(),
            (s.body_rect.y.raw(), s.body_rect.height.raw()),
            (s.header_rect.y.raw(), s.header_rect.height.raw()),
            (s.footer_rect.y.raw(), s.footer_rect.height.raw()));
        for (i, h) in s.headers.iter().enumerate() {
            eprintln!("  header[{i}] apply={:?} paras={} text={:?}", h.apply, h.paragraphs.len(), snippet(&h.paragraphs));
        }
        for (i, f) in s.footers.iter().enumerate() {
            eprintln!("  footer[{i}] apply={:?} paras={} text={:?}", f.apply, f.paragraphs.len(), snippet(&f.paragraphs));
        }
        for (i, m) in s.master_pages.iter().enumerate() {
            eprintln!("  master[{i}] apply={:?} ext={} overlap={} paras={} text={:?}",
                m.apply, m.is_extension, m.overlap, m.paragraphs.len(), snippet(&m.paragraphs));
        }
    }
}

/// Report page furniture (header/footer/master) extracted + placed per file.
#[test]
#[ignore]
fn dump_furniture() {
    use kdsnr_hwp_layout::FurnitureRole;
    for name in [
        "korean.hwpx",
        "math_input_sample_2.hwpx",
        "science_input_example_2.hwpx",
        "social_test_input_2.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let (mut h, mut f, mut m) = (0usize, 0usize, 0usize);
        for s in &model.sections {
            h += s.headers.len();
            f += s.footers.len();
            m += s.master_pages.len();
        }
        let result = paginate(name);
        let mut ph = 0;
        let mut pf = 0;
        let mut pm = 0;
        for page in &result.pages {
            for fu in &page.furniture {
                match fu.role {
                    FurnitureRole::Header => ph += fu.items.len(),
                    FurnitureRole::Footer => pf += fu.items.len(),
                    FurnitureRole::MasterPage => pm += fu.items.len(),
                }
            }
        }
        eprintln!(
            "{name}: defs[hdr={h} ftr={f} master={m}] | placed items across {} pages: hdr={ph} ftr={pf} master={pm}",
            result.pages.len()
        );
    }
}

/// Inspect raw lineseg `flags` bits across a file: does HWPX preserve the
/// first-line-of-page (bit0) / first-line-of-column (bit1) markers, and at the
/// vertpos resets the engine detects?
#[test]
#[ignore]
fn dump_lineseg_flags() {
    use std::collections::BTreeMap;
    for name in [
        "math_input_sample_2.hwpx",
        "social_test_input_2.hwpx",
        "science_input_example_2.hwpx",
        "korean.hwpx",
    ] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let mut distinct: BTreeMap<u32, usize> = BTreeMap::new();
        let mut bit0 = 0usize; // first-line-of-page
        let mut bit1 = 0usize; // first-line-of-column
        let mut total = 0usize;
        // Also: at each vertpos reset, what is the flag of the resetting line?
        let mut reset_lines = 0usize;
        let mut reset_with_bit1 = 0usize;
        let mut prev_top = i32::MIN;
        let mut prev_adv = 0i32;
        let mut seen = false;
        for sec in &model.sections {
            for p in &sec.paragraphs {
                for s in &p.stored_line_segs {
                    total += 1;
                    *distinct.entry(s.tag).or_default() += 1;
                    if s.tag & 0x01 != 0 {
                        bit0 += 1;
                    }
                    if s.tag & 0x02 != 0 {
                        bit1 += 1;
                    }
                    let t = s.vertical_pos.raw();
                    if seen && (t < prev_top || (t == prev_top && prev_adv > 0)) {
                        reset_lines += 1;
                        if s.tag & 0x02 != 0 {
                            reset_with_bit1 += 1;
                        }
                    }
                    prev_top = t;
                    prev_adv = s.line_height.raw() + s.line_spacing.raw();
                    seen = true;
                }
            }
        }
        eprintln!(
            "{name}: total={total} bit0(page)={bit0} bit1(col)={bit1} | engine_resets={reset_lines} resets_with_bit1={reset_with_bit1}",
        );
        let top: Vec<String> = distinct
            .iter()
            .rev()
            .take(6)
            .map(|(k, v)| format!("0x{k:08x}:{v}"))
            .collect();
        eprintln!("    distinct flags (top): {}", top.join(" "));
    }
}

/// Dump top-level table anchor attributes (treat_as_char / vert_rel_to /
/// text_wrap / height / offset) to find the in-flow vs floating signal.
#[test]
#[ignore]
fn dump_table_anchors() {
    use kdsnr_hwp_parser::model::control::Control;
    let name = std::env::var("DUMP").unwrap_or_else(|_| "science_input_example_2.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    for sec in &doc.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            for c in &p.controls {
                if let Control::Table(t) = c {
                    let co = &t.common;
                    eprintln!(
                        "{name} p{pi}: table {}x{} tac={} vrel={:?} wrap={:?} voff={} rows={}",
                        co.width, co.height, co.treat_as_char, co.vert_rel_to, co.text_wrap,
                        co.vertical_offset, t.row_count
                    );
                }
            }
        }
    }
}

/// Validate the reset-counting page model: columns = (vertpos resets in the
/// top-level lineseg stream) + 1; pages = ceil(columns / cols_per_page).
#[test]
#[ignore]
fn reset_page_model() {
    for (name, gt) in [
        ("korean.hwpx", 1),
        ("math_input_sample.hwpx", 20),
        ("math_input_sample_2.hwpx", 7),
        ("science.hwpx", 7),
        ("science_input_example_2.hwpx", 7),
        ("social_input_sample.hwpx", 5),
        ("social_test_input_2.hwpx", 4),
    ] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let mut total_pages = 0usize;
        for sec in &model.sections {
            let cols = sec.columns.count.max(1) as usize;
            let mut resets = 0usize;
            let mut prev = i32::MIN;
            let mut any = false;
            for p in &sec.paragraphs {
                for ls in &p.stored_line_segs {
                    let vp = ls.vertical_pos.raw();
                    if any && vp < prev {
                        resets += 1;
                    }
                    prev = vp;
                    any = true;
                }
            }
            let columns = resets + 1;
            total_pages += (columns + cols - 1) / cols;
        }
        let mark = if total_pages == gt { "OK" } else { "XX" };
        eprintln!("{mark} {name}: model_pages={total_pages} GT={gt}");
    }
}

/// Report per-page item counts and the count of endnote-sourced items, to
/// confirm endnotes flow onto the expected (final) page.
#[test]
#[ignore]
fn dump_endnote_placement() {
    for name in ["science.hwpx", "science_input_example_2.hwpx"] {
        if !original(name).exists() {
            continue;
        }
        let data = std::fs::read(original(name)).expect("read");
        let doc = parse_document(&data).expect("parse");
        let model = normalize(&doc);
        let en_paras: Vec<usize> = model
            .sections
            .iter()
            .flat_map(|s| s.endnotes.iter().map(|p| p.id.index))
            .collect();
        let result = paginate(name);
        eprintln!("{name}: {} pages, {} endnote paragraphs", result.pages.len(), en_paras.len());
        for (pi, page) in result.pages.iter().enumerate() {
            let en_here = page
                .items
                .iter()
                .filter(|it| matches!(it.source, kdsnr_hwp_core::SourceRef::Paragraph(p) if p.index >= 400_000))
                .count();
            eprintln!("  page {pi}: {} items ({} endnote)", page.items.len(), en_here);
        }
    }
}

/// Replicate the paginator's exact block walk and report every reset, with the
/// block that triggered it, so the column total can be audited line by line.
#[test]
#[ignore]
fn trace_resets() {
    use kdsnr_hwp_doc::BreakBefore;
    let name = std::env::var("DUMP").unwrap_or_else(|_| "science.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    for sec in &measured.sections {
        let cols_per_page = sec.columns.len().max(1);
        let mut col_index = 0usize;
        let mut prev_top = i32::MIN;
        let mut prev_start = i32::MIN;
        let mut prev_advance = 0i32;
        let mut seen_line = false;
        eprintln!(
            "{name} sec {:?}: cols_per_page={cols_per_page} blocks={}",
            sec.id,
            sec.blocks.len()
        );
        for (bi, block) in sec.blocks.iter().enumerate() {
            let mut boundary_from_break = false;
            if seen_line {
                match block.break_before {
                    BreakBefore::Page | BreakBefore::Section => {
                        let page = col_index / cols_per_page;
                        col_index = (page + 1) * cols_per_page;
                        boundary_from_break = true;
                        eprintln!("  [b{bi}] {:?} break={:?} -> col {col_index}", block.kind, block.break_before);
                    }
                    BreakBefore::Column | BreakBefore::MultiColumn => {
                        col_index += 1;
                        boundary_from_break = true;
                        eprintln!("  [b{bi}] {:?} break={:?} -> col {col_index}", block.kind, block.break_before);
                    }
                    BreakBefore::None => {}
                }
            }
            let mut first = true;
            for (li, (top, advance)) in block.line_tops.iter().zip(block.fragments.iter()).enumerate() {
                let t = top.raw();
                let s = block.line_starts.get(li).map(|v| v.raw()).unwrap_or(0);
                let reset = seen_line
                    && (t < prev_top || (t == prev_top && s <= prev_start && prev_advance > 0));
                if reset && !(first && boundary_from_break) {
                    col_index += 1;
                    eprintln!(
                        "  [b{bi} L{li}] {:?} {:?} RESET prev_top={prev_top} top={t} start={s} prev_start={prev_start} prev_adv={prev_advance} -> col {col_index}",
                        block.kind, block.source
                    );
                }
                prev_top = t;
                prev_start = s;
                prev_advance = advance.raw();
                seen_line = true;
                first = false;
            }
        }
        let total_cols = if sec.blocks.is_empty() { 0 } else { col_index + 1 };
        eprintln!(
            "  => total_cols={total_cols} pages={}",
            total_cols.div_ceil(cols_per_page)
        );
    }
}

/// Dump every block in an index window with kind/height/first+last line top, so
/// table-driven column fills (no line_tops) can be seen alongside paragraphs.
#[test]
#[ignore]
fn dump_block_window() {
    let name = std::env::var("DUMP").unwrap_or_else(|_| "science.hwpx".into());
    let lo: usize = std::env::var("LO").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let hi: usize = std::env::var("HI").ok().and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    for sec in &measured.sections {
        eprintln!("{name} body_h={} cols={}", sec.body_rect.height.raw(), sec.columns.len());
        for (bi, b) in sec.blocks.iter().enumerate() {
            if bi < lo || bi > hi {
                continue;
            }
            let ft = b.line_tops.first().map(|t| t.raw());
            let lt = b.line_tops.last().map(|t| t.raw());
            eprintln!(
                "  b{bi} {:?} h={} brk={:?} lines={} top[{:?}..{:?}]",
                b.kind, b.bounds.height.raw(), b.break_before, b.line_tops.len(), ft, lt
            );
        }
    }
}

#[test]
#[ignore]
fn dump_vp_progression() {
    let name = std::env::var("DUMP").unwrap_or_else(|_| "korean.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let sec = &model.sections[0];
    eprintln!("{name}: body_h={} cols={}", sec.body_rect.height.raw(), sec.columns.count);
    let mut prev_vp = -1i32;
    for (pi, p) in sec.paragraphs.iter().enumerate() {
        if let Some(f) = p.stored_line_segs.first() {
            let l = p.stored_line_segs.last().unwrap();
            let reset = if f.vertical_pos.raw() < prev_vp { " <== RESET" } else { "" };
            eprintln!(
                "p{pi}: first_vp={} hpos={} .. last_vp={} (n={}, tbls={}, objs={}) brk={:?} sb={} sa={}{reset}",
                f.vertical_pos.raw(), f.column_start.raw(), l.vertical_pos.raw(),
                p.stored_line_segs.len(), p.tables.len(), p.objects.len(),
                p.break_before, p.space_before.raw(), p.space_after.raw()
            );
            prev_vp = l.vertical_pos.raw();
        } else {
            eprintln!("p{pi}: (no linesegs, tbls={}, objs={})", p.tables.len(), p.objects.len());
        }
    }
}

#[test]
#[ignore]
fn dump_block_kinds() {
    use kdsnr_hwp_layout::BlockKind;
    let name = std::env::var("DUMP").unwrap_or_else(|_| "korean.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    for sec in &measured.sections {
        let mut para_h = 0i32;
        let mut para_n = 0;
        let mut tbl_h = 0i32;
        let mut tbl_n = 0;
        let mut tbl_heights = vec![];
        let mut obj_h = 0i32;
        for b in &sec.blocks {
            match b.kind {
                BlockKind::Paragraph => {
                    para_h += b.bounds.height.raw();
                    para_n += 1;
                }
                BlockKind::Table => {
                    tbl_h += b.bounds.height.raw();
                    tbl_n += 1;
                    tbl_heights.push(b.bounds.height.raw());
                }
                _ => obj_h += b.bounds.height.raw(),
            }
        }
        eprintln!(
            "{name} sec {:?}: cols={} body_h={} | paras={para_n} para_h={para_h} | tables={tbl_n} tbl_h={tbl_h} {:?} | obj_h={obj_h}",
            sec.id,
            sec.columns.len(),
            sec.body_rect.height.raw(),
            tbl_heights
        );
    }
}

#[test]
#[ignore]
fn dump_linesegs() {
    let name = std::env::var("DUMP").unwrap_or_else(|_| "korean.hwpx".into());
    let data = std::fs::read(original(&name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    // First few top-level paragraphs with >1 stored lineseg.
    let mut shown = 0;
    for sec in &model.sections {
        for (pi, p) in sec.paragraphs.iter().enumerate() {
            if p.stored_line_segs.len() < 2 {
                continue;
            }
            eprintln!("--- para {pi}: {} linesegs ---", p.stored_line_segs.len());
            for (i, s) in p.stored_line_segs.iter().enumerate() {
                let next_vp = p
                    .stored_line_segs
                    .get(i + 1)
                    .map(|n| n.vertical_pos.raw());
                let delta = next_vp.map(|nv| nv - s.vertical_pos.raw());
                eprintln!(
                    "  ls[{i}] vp={} lh={} th={} bl={} sp={} hpos={} hsize={} | next_vp_delta={:?} (lh+sp={})",
                    s.vertical_pos.raw(),
                    s.line_height.raw(),
                    s.text_height.raw(),
                    s.baseline_distance.raw(),
                    s.line_spacing.raw(),
                    s.column_start.raw(),
                    s.segment_width.raw(),
                    delta,
                    s.line_height.raw() + s.line_spacing.raw()
                );
            }
            shown += 1;
            if shown >= 4 {
                return;
            }
        }
    }
}

#[test]
fn pagination_is_deterministic_and_json_inspectable() {
    let a = paginate("math_input_sample_2.hwpx");
    let b = paginate("math_input_sample_2.hwpx");
    assert_eq!(a.pages.len(), b.pages.len());
    assert_eq!(a, b, "pagination is stable across runs");

    let json = pagination_to_json(&a);
    assert!(json.starts_with("{\"pages\":["));
    assert!(json.contains("\"items\":["));
    assert!(json.contains("\"paper\":"));
    // Source count in JSON matches body items plus furniture items.
    let total_items: usize = a
        .pages
        .iter()
        .map(|p| {
            p.items.len() + p.furniture.iter().map(|f| f.items.len()).sum::<usize>()
        })
        .sum();
    assert_eq!(json.matches("\"source\":").count(), total_items);
}

#[test]
#[ignore]
fn probe_idx30_items() {
    use kdsnr_hwp_core::SourceRef;
    let target: usize = std::env::var("IDX").ok().and_then(|s| s.parse().ok()).unwrap_or(30);
    let name = "국어_박스, 밑줄, 묶음.hwpx";
    let data = std::fs::read(original(name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    // Inspect the measured block for idx30.
    for sec in &measured.sections {
        for b in &sec.blocks {
            if let SourceRef::Paragraph(id) = b.source {
                if id.index == target {
                    eprintln!("BLOCK idx{target}: nfrag={} line_tops={:?}", b.fragments.len(),
                        b.line_tops.iter().map(|t| t.raw()).collect::<Vec<_>>());
                    eprintln!("  fragments(adv)={:?}", b.fragments.iter().map(|f| f.raw()).collect::<Vec<_>>());
                    eprintln!("  line_boxes={:?} break_before={:?} absolute={}",
                        b.line_boxes.iter().map(|f| f.raw()).collect::<Vec<_>>(), b.break_before, b.absolute);
                    eprintln!("  body_rect={:?}", sec.body_rect);
                }
            }
        }
    }
    let g = paginate_document(&measured).expect("greedy");
    for (pi, page) in g.pages.iter().enumerate() {
        for it in &page.items {
            if let SourceRef::Paragraph(id) = it.source {
                if id.index == target {
                    eprintln!("p{pi} col={} rect={:?} frag_range={:?}", it.column, it.rect, it.fragment_range);
                }
            }
        }
    }
}

#[test]
#[ignore]
fn probe_ctrl_items() {
    use kdsnr_hwp_core::SourceRef;
    let pidx: usize = std::env::var("PIDX").ok().and_then(|s| s.parse().ok()).unwrap_or(34);
    let name = "국어_박스, 밑줄, 묶음.hwpx";
    let data = std::fs::read(original(name)).expect("read");
    let doc = parse_document(&data).expect("parse");
    let model = normalize(&doc);
    let measured = measure_document(&model);
    for sec in &measured.sections {
        for b in &sec.blocks {
            match b.source {
                SourceRef::Control(c) if c.paragraph.index == pidx => {
                    eprintln!("BLOCK Control(p{}:{}) kind={:?} absolute={} bounds={:?} break={:?} nfrag={}",
                        c.paragraph.index, c.index, b.kind, b.absolute, b.bounds, b.break_before, b.fragments.len());
                }
                SourceRef::Paragraph(id) if id.index == pidx => {
                    eprintln!("BLOCK Paragraph(idx{}) line_tops[0..2]={:?} nfrag={}", id.index,
                        b.line_tops.iter().take(2).map(|t| t.raw()).collect::<Vec<_>>(), b.fragments.len());
                }
                _ => {}
            }
        }
        eprintln!("body_rect={:?} columns={:?}", sec.body_rect, sec.columns);
    }
    let g = paginate_document(&measured).expect("greedy");
    for (pi, page) in g.pages.iter().enumerate() {
        for it in &page.items {
            let hit = match it.source {
                SourceRef::Control(c) => c.paragraph.index == pidx,
                SourceRef::Paragraph(id) => id.index == pidx,
                _ => false,
            };
            if hit {
                eprintln!("p{pi} {:?} col={} rect={:?} frag={:?}", it.source, it.column, it.rect, it.fragment_range);
            }
        }
    }
}
