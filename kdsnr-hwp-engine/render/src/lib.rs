//! SVG backend: replay paint operations into an SVG document.
//!
//! No layout decisions belong here. The backend converts stored HWPUNIT
//! coordinates to pixels at a chosen DPI and emits one SVG element per paint
//! operation. Text is emitted as resolved glyph outlines (Hancom HFT outlines for
//! HFT-typed faces, TTF outlines otherwise) at the stored 자간/장평 advances, via
//! the `FontResolver`. There is no font-name `<text>` fallback: rendering is
//! glyph-exact, so a `FontResolver` is required (the deployment gate guarantees
//! every drawn face has a file).

use kdsnr_hwp_core::{EngineResult, Rect};
use kdsnr_hwp_font::{advance_of, CharMetrics, FontResolver};
use kdsnr_hwp_paint::{Align, BorderStyle, Color, PaintOp, PaintPage, TextLine};

/// HWPUNIT is 1/7200 inch; pixels at `dpi` are `hwpunit * dpi / 7200`.
fn scale_for(dpi: f64) -> f64 {
    dpi / 7200.0
}

/// HFT glyph baseline height above the design-box bottom, as a fraction of the
/// glyph size (= design descent ÷ design-box height). Hancom HFT outlines are
/// design-box-relative; cap feet sit ~0.10 of the box above its bottom across the
/// 신명/한양 faces, so this drops the box onto the line baseline (TTF outlines are
/// baseline-relative, i.e. this offset is 0).
const HFT_BASELINE_FRAC: f64 = 0.10;

fn fmt(v: f64) -> String {
    // Trim to 2 decimals without trailing-zero noise.
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Format a glyph-transform scale factor with enough precision for both ranges:
/// TTF outlines are em-normalised so the scale is ~size (e.g. 15), while HFT
/// outlines are in design-box units (0..~1000) so the scale is tiny (~0.015).
/// `fmt`'s 2-decimal rounding would turn 0.0153 into 0.02 (a 30% over-scale that
/// vertically stretches HFT glyphs), so scale keeps 6 significant figures.
fn fmt_scale(v: f64) -> String {
    let s = format!("{:.6}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".to_string() } else { s.to_string() }
}

/// File extension → data-URI mime type.
fn mime_for(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "wmf" => "image/wmf",
        _ => "image/png",
    }
}

/// Standard base64 (RFC 4648) of raw bytes, for data URIs.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Render a single page to a standalone SVG string at 96 DPI with resolved glyph
/// outlines.
pub fn render_svg_page(page: &PaintPage, fonts: &FontResolver) -> EngineResult<String> {
    Ok(page_to_svg(page, 96.0, fonts))
}

/// Render a page to SVG: resolved glyph outlines (HFT/TTF) at 자간/장평 advances.
pub fn page_to_svg(page: &PaintPage, dpi: f64, fonts: &FontResolver) -> String {
    let s = scale_for(dpi);
    let w = fmt(page.paper.width.raw() as f64 * s);
    let h = fmt(page.paper.height.raw() as f64 * s);
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n"
    ));
    for op in &page.ops {
        render_op(&mut out, op, s, fonts);
    }
    out.push_str("</svg>\n");
    out
}

/// Render a given list of paint ops onto a canvas of `view` in user units
/// `(x, y, w, h)`. With `white_bg`, an opaque white rect backs `view`; without it
/// the canvas is transparent (used to probe the ops' ink bounds). Coordinates are
/// absolute (the ops carry page coords), so `view` may be any sub-rect. The
/// question crop passes only one unit's ops, so neighbouring content cannot bleed
/// into the canvas.
pub fn ops_svg(
    dpi: f64,
    fonts: &FontResolver,
    ops: &[PaintOp],
    view: (f64, f64, f64, f64),
    white_bg: bool,
) -> String {
    let s = scale_for(dpi);
    let (vx, vy, vw, vh) = (fmt(view.0), fmt(view.1), fmt(view.2), fmt(view.3));
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{vw}\" height=\"{vh}\" viewBox=\"{vx} {vy} {vw} {vh}\">\n"
    ));
    if white_bg {
        out.push_str(&format!(
            "  <rect x=\"{vx}\" y=\"{vy}\" width=\"{vw}\" height=\"{vh}\" fill=\"#ffffff\"/>\n"
        ));
    }
    for op in ops {
        render_op(&mut out, op, s, fonts);
    }
    out.push_str("</svg>\n");
    out
}

/// A list of paint ops rendered as a nested `<svg>` child positioned at `(at_x,
/// at_y)` (user units) within a parent SVG. `view` is the source sub-rect (user
/// units, absolute page coords); the child is `view`-sized at 1:1 (no scaling)
/// and clips to it. Used to stitch a crop unit's fragments (across columns/pages)
/// into one vertical strip.
pub fn ops_svg_nested(
    dpi: f64,
    fonts: &FontResolver,
    ops: &[PaintOp],
    view: (f64, f64, f64, f64),
    at_x: f64,
    at_y: f64,
) -> String {
    let s = scale_for(dpi);
    let (vx, vy, vw, vh) = (fmt(view.0), fmt(view.1), fmt(view.2), fmt(view.3));
    let mut out = String::new();
    out.push_str(&format!(
        "<svg x=\"{}\" y=\"{}\" width=\"{vw}\" height=\"{vh}\" viewBox=\"{vx} {vy} {vw} {vh}\">\n",
        fmt(at_x),
        fmt(at_y),
    ));
    out.push_str(&format!(
        "  <rect x=\"{vx}\" y=\"{vy}\" width=\"{vw}\" height=\"{vh}\" fill=\"#ffffff\"/>\n"
    ));
    for op in ops {
        render_op(&mut out, op, s, fonts);
    }
    out.push_str("</svg>\n");
    out
}

/// Render one paint op to SVG. Factored so inline objects (equations woven into
/// a text line) can be rendered at a computed offset via translated ops.
fn render_op(out: &mut String, op: &PaintOp, s: f64, fonts: &FontResolver) {
    {
        match op {
            PaintOp::FillRect { rect, color } => {
                out.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
                    fmt(rect.x.raw() as f64 * s),
                    fmt(rect.y.raw() as f64 * s),
                    fmt(rect.width.raw() as f64 * s),
                    fmt(rect.height.raw() as f64 * s),
                    hex(*color),
                ));
            }
            PaintOp::StrokeRect { rect, color, width } => {
                out.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
                    fmt(rect.x.raw() as f64 * s),
                    fmt(rect.y.raw() as f64 * s),
                    fmt(rect.width.raw() as f64 * s),
                    fmt(rect.height.raw() as f64 * s),
                    hex(*color),
                    fmt((*width as f64 * s).max(0.5)),
                ));
            }
            PaintOp::Line { x1, y1, x2, y2, color, width, style } => {
                push_border_line(out, *x1, *y1, *x2, *y2, *color, *width, *style, s);
            }
            PaintOp::Image { rect, data, ext } => {
                out.push_str(&format!(
                    "  <image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"data:{};base64,{}\"/>\n",
                    fmt(rect.x.raw() as f64 * s),
                    fmt(rect.y.raw() as f64 * s),
                    fmt(rect.width.raw() as f64 * s),
                    fmt(rect.height.raw() as f64 * s),
                    mime_for(ext),
                    base64(data),
                ));
            }
            PaintOp::TextLine(line) => push_text_line_glyphs(out, line, s, fonts),
        }
    }
}

/// Translate a paint op's coordinates by `(dx, dy)` HWPUNIT (for placing an
/// inline object's relative ops at its in-line position).
fn translate_op(op: &PaintOp, dx: i32, dy: i32) -> PaintOp {
    let mv = |r: &Rect| Rect::new(r.x.raw() + dx, r.y.raw() + dy, r.width.raw(), r.height.raw());
    match op {
        PaintOp::FillRect { rect, color } => PaintOp::FillRect { rect: mv(rect), color: *color },
        PaintOp::StrokeRect { rect, color, width } => {
            PaintOp::StrokeRect { rect: mv(rect), color: *color, width: *width }
        }
        PaintOp::Line { x1, y1, x2, y2, color, width, style } => PaintOp::Line {
            x1: x1 + dx, y1: y1 + dy, x2: x2 + dx, y2: y2 + dy,
            color: *color, width: *width, style: *style,
        },
        PaintOp::Image { rect, data, ext } => {
            PaintOp::Image { rect: mv(rect), data: data.clone(), ext: ext.clone() }
        }
        PaintOp::TextLine(l) => {
            let mut l = l.clone();
            l.x += dx;
            l.baseline += dy;
            l.top += dy;
            PaintOp::TextLine(l)
        }
    }
}

/// Emit a border edge: solid/dashed/dotted as one stroked line, double as two.
#[allow(clippy::too_many_arguments)]
fn push_border_line(
    out: &mut String, x1: i32, y1: i32, x2: i32, y2: i32, color: Color, width: i32,
    style: BorderStyle, s: f64,
) {
    let w = (width as f64 * s).max(0.4);
    let dash = match style {
        BorderStyle::Dashed => format!(" stroke-dasharray=\"{},{}\"", fmt(w * 3.0), fmt(w * 2.0)),
        BorderStyle::Dotted => format!(" stroke-dasharray=\"{},{}\"", fmt(w), fmt(w * 1.5)),
        _ => String::new(),
    };
    let (x1, y1, x2, y2) = (x1 as f64 * s, y1 as f64 * s, x2 as f64 * s, y2 as f64 * s);
    let line = |out: &mut String, off: f64| {
        let (dx, dy) = if (y1 - y2).abs() < (x1 - x2).abs() { (0.0, off) } else { (off, 0.0) };
        out.push_str(&format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>\n",
            fmt(x1 + dx), fmt(y1 + dy), fmt(x2 + dx), fmt(y2 + dy), hex(color), fmt(w), dash,
        ));
    };
    if style == BorderStyle::Double {
        line(out, -w);
        line(out, w);
    } else {
        line(out, 0.0);
    }
}

struct Glyph {
    run: usize,
    d: Option<String>,
    sx: f64,
    sy: f64,
    dy: f64,
    fill: String,
    faux_bold: bool,
    adv: f64,
    is_space: bool,
}

/// Line-fill result: visible glyph count (trailing spaces excluded), the
/// alignment start offset, and per-space / per-gap stretch added during justify.
struct LineFill {
    vis_end: usize,
    start_delta: f64,
    extra_space: f64,
    extra_gap: f64,
}

/// Place a line's glyphs within `seg_width` per alignment. Trailing spaces are
/// the word break that wraps to the next line — invisible, so they are dropped
/// from the fill (`vis_end`) and never consume width or take justify slack; the
/// last visible glyph then meets the segment's right edge. Justify spreads slack
/// over word-spaces when present (Hancom widens 어절 gaps, keeping char spacing
/// even), else over all character gaps; positive slack stretches, negative
/// condenses.
fn line_fill(cells: &[(f64, bool)], seg_width: f64, align: Align, is_last: bool) -> LineFill {
    let vis_end = cells.iter().rposition(|&(_, sp)| !sp).map_or(0, |i| i + 1);
    if vis_end == 0 {
        return LineFill { vis_end: 0, start_delta: 0.0, extra_space: 0.0, extra_gap: 0.0 };
    }
    let total: f64 = cells[..vis_end].iter().map(|&(a, _)| a).sum();
    let slack = seg_width - total;
    let n = vis_end;
    let n_space = cells[..vis_end].iter().filter(|&&(_, sp)| sp).count();
    let (mut start_delta, mut extra_space, mut extra_gap) = (0.0, 0.0, 0.0);
    match align {
        Align::Left => {}
        Align::Right => start_delta = slack,
        Align::Center => start_delta = slack / 2.0,
        // Justify fills the segment width by spreading (slack > 0) or condensing
        // (slack < 0) over word-spaces, else over all char gaps. The last line is
        // left-aligned when it fits, but an over-full last line is still condensed
        // to the segment width — Hancom never lets a line exceed it (this is the
        // fit-to-cell behaviour that keeps cell text inside its box).
        Align::Justify if !is_last || slack < 0.0 => {
            if n_space > 0 {
                extra_space = slack / n_space as f64;
            } else if n > 1 {
                extra_gap = slack / (n - 1) as f64;
            }
        }
        Align::Justify => {}
        Align::Distribute if n > 1 => extra_gap = slack / (n - 1) as f64,
        Align::Distribute => {}
    }
    LineFill { vis_end, start_delta, extra_space, extra_gap }
}

/// Emit a line as resolved-TTF glyph outlines at 자간/장평 advances, placing the
/// run within its segment per the paragraph alignment (justify fills `seg_width`).
fn push_text_line_glyphs(out: &mut String, line: &TextLine, s: f64, r: &FontResolver) {
    let baseline_px = line.baseline as f64 * s;
    let mut glyphs: Vec<Glyph> = Vec::new();
    for (ri, run) in line.runs.iter().enumerate() {
        let m = CharMetrics {
            face_resolved: true,
            ratio: run.ratio,
            spacing: run.spacing,
            rel_sz: run.rel_sz,
            base_size: (run.size_pt * 100.0).round() as i32,
            bold: run.bold,
            is_hft: run.is_hft,
        };
        let size_hwp = m.base_size as f64 * m.rel_sz as f64 / 100.0;
        let sx = size_hwp * (m.ratio as f64 / 100.0) * s;
        let sy = size_hwp * s;
        // 글자 위치: 크기의 %, 양수 = 위로 (SVG y-down 이므로 baseline 에서 뺀다).
        let dy = -(run.char_offset as f64 / 100.0) * size_hwp * s;
        let fill = hex(run.color);
        let mut tab_i = 0usize;
        for ch in run.text.chars() {
            // A tab's advance is Hancom's stored width (no glyph).
            let adv = if ch == '\t' {
                let w = run.tab_widths.get(tab_i).copied().unwrap_or(0);
                tab_i += 1;
                w
            } else {
                advance_of(r, &run.font, ch, &m).unwrap_or((size_hwp * 0.5) as i32)
            };
            // Whitespace/control chars (tab, figure space, NBSP, …) advance only;
            // they never ink. Some fonts map them to a visible glyph (e.g. a
            // small circle for U+0009), so the outline must be suppressed here.
            // An HFT-typed face draws Hancom's own outline. The path is in the
            // glyph's design box (`0..design_h`, y-up, baseline `HFT_BASELINE_FRAC`
            // of the box above its bottom): scale by `size/design_h` (NOT size/em —
            // the design box, ~1.2–1.3× em, is what maps to the point size) and
            // drop the baseline so it sits on the line baseline like the TTF path
            // (em-normalised, baseline 0). Falls back to the TTF substitute when
            // the HFT glyph is absent.
            let (d, faux_bold, gsx, gsy, gdy) = if ch.is_whitespace() || ch.is_control() {
                (None, false, sx, sy, dy)
            } else if let Some((hd, design_h)) = m.is_hft.then(|| r.hft_outline(&run.font, ch)).flatten() {
                let e = design_h as f64;
                (Some(hd), false, sx / e, sy / e, dy + HFT_BASELINE_FRAC * sy)
            } else {
                let (d, faux) = r
                    .resolve_glyph(&run.font, ch, run.bold)
                    .map(|(f, faux)| (f.outline_svg_em(ch).filter(|d| !d.is_empty()), faux))
                    .unwrap_or((None, false));
                (d, faux, sx, sy, dy)
            };
            glyphs.push(Glyph {
                run: ri, d, sx: gsx, sy: gsy, dy: gdy, fill: fill.clone(), faux_bold,
                adv: adv as f64, is_space: ch == ' ',
            });
        }
    }
    // Interleave inline-object spacers into the glyph flow at their run-order
    // char index, each occupying its box width so the following text advances
    // past it. A spacer carries no outline (run = usize::MAX) and is skipped by
    // the run shade/underline bands.
    let mut obj_slot = vec![usize::MAX; line.inline_objects.len()];
    if !line.inline_objects.is_empty() {
        let n_text = glyphs.len();
        let mut order: Vec<usize> = (0..line.inline_objects.len()).collect();
        order.sort_by_key(|&k| line.inline_objects[k].char_index.min(n_text));
        let mut merged: Vec<Glyph> = Vec::with_capacity(n_text + order.len());
        let mut text = glyphs.into_iter();
        let mut oi = 0;
        for ci in 0..=n_text {
            while oi < order.len() && line.inline_objects[order[oi]].char_index.min(n_text) == ci {
                obj_slot[order[oi]] = merged.len();
                merged.push(Glyph {
                    run: usize::MAX, d: None, sx: 0.0, sy: 0.0, dy: 0.0,
                    fill: String::new(), faux_bold: false,
                    adv: line.inline_objects[order[oi]].advance as f64, is_space: false,
                });
                oi += 1;
            }
            if ci < n_text {
                merged.push(text.next().unwrap());
            }
        }
        glyphs = merged;
    }
    if glyphs.is_empty() {
        return;
    }
    let cells: Vec<(f64, bool)> = glyphs.iter().map(|g| (g.adv, g.is_space)).collect();
    let fill = line_fill(&cells, line.seg_width as f64, line.align, line.is_last_line);
    let glyphs = &glyphs[..fill.vis_end];
    if glyphs.is_empty() {
        return;
    }
    let n = glyphs.len();
    let (start_x, extra_space, extra_gap) =
        (line.x as f64 + fill.start_delta, fill.extra_space, fill.extra_gap);
    let mut cursor = start_x;
    let mut run_x0 = vec![f64::NAN; line.runs.len()];
    let mut run_x1 = vec![0.0; line.runs.len()];
    let mut paths = String::new();
    // Cursor x (HWPUNIT) at the start of each glyph, plus the end, so an inline
    // object at a visual char index resolves to its in-line x.
    let mut glyph_x: Vec<f64> = Vec::with_capacity(n + 1);
    for (i, g) in glyphs.iter().enumerate() {
        glyph_x.push(cursor);
        if g.run < run_x0.len() && run_x0[g.run].is_nan() {
            run_x0[g.run] = cursor;
        }
        if let Some(d) = &g.d {
            // Faux-bold (no Bold TTF in family): stroke the fill outline.
            // stroke-width is em-space (inside the scale transform) ≈ 3.5% em.
            let stroke = if g.faux_bold {
                format!(" stroke=\"{}\" stroke-width=\"0.035\"", g.fill)
            } else {
                String::new()
            };
            paths.push_str(&format!(
                "  <path d=\"{}\" transform=\"translate({},{}) scale({},{})\" fill=\"{}\"{}/>\n",
                d, fmt(cursor * s), fmt(baseline_px + g.dy), fmt_scale(g.sx), fmt_scale(-g.sy), g.fill, stroke,
            ));
        }
        cursor += g.adv;
        if g.is_space {
            cursor += extra_space;
        }
        if i + 1 < n {
            cursor += extra_gap;
        }
        if g.run < run_x1.len() {
            run_x1[g.run] = cursor;
        }
    }
    // 음영 + 글자 테두리 배경 — 글리프 뒤에 먼저.
    let run_band = |run: &kdsnr_hwp_paint::TextRun| {
        let size_hwp = (run.size_pt * 100.0) as f64 * run.rel_sz as f64 / 100.0;
        (baseline_px - size_hwp * 0.85 * s, size_hwp * 1.05 * s)
    };
    for (ri, run) in line.runs.iter().enumerate() {
        if run_x1[ri] <= run_x0[ri] {
            continue;
        }
        let (top, ht) = run_band(run);
        let bg = run.shade.or_else(|| run.border.fill.map(Color::from_ref));
        if let Some(c) = bg {
            out.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
                fmt(run_x0[ri] * s), fmt(top), fmt((run_x1[ri] - run_x0[ri]) * s), fmt(ht), hex(c),
            ));
        }
    }
    out.push_str(&paths);
    for (ri, run) in line.runs.iter().enumerate() {
        if run_x1[ri] <= run_x0[ri] {
            continue;
        }
        let size_hwp = (run.size_pt * 100.0) as f64 * run.rel_sz as f64 / 100.0;
        let x0 = fmt(run_x0[ri] * s);
        let x1 = fmt(run_x1[ri] * s);
        let w = fmt((size_hwp * 0.05 * s).max(0.5));
        let mut hline = |y: f64, c: &str| {
            out.push_str(&format!(
                "  <line x1=\"{x0}\" y1=\"{0}\" x2=\"{x1}\" y2=\"{0}\" stroke=\"{c}\" stroke-width=\"{w}\"/>\n",
                fmt(y),
            ));
        };
        if run.underline {
            hline(baseline_px + size_hwp * 0.18 * s, &hex(run.underline_color));
        }
        if run.strikeout {
            let y = baseline_px - size_hwp * 0.28 * s;
            let c = hex(run.strike_color);
            if run.strike_shape == 7 {
                // DOUBLE_SLIM: 두 줄.
                hline(y - size_hwp * 0.05 * s, &c);
                hline(y + size_hwp * 0.05 * s, &c);
            } else {
                hline(y, &c);
            }
        }
        // 글자 테두리 박스 (4면).
        let b = &run.border;
        if b.left.visible() || b.right.visible() || b.top.visible() || b.bottom.visible() {
            let (top, ht) = run_band(run);
            let (rx0, rx1, ry0, ry1) = (run_x0[ri] * s, run_x1[ri] * s, top, top + ht);
            let mut edge = |e: &kdsnr_hwp_paint::BorderEdge, x1: f64, y1: f64, x2: f64, y2: f64| {
                if e.visible() {
                    out.push_str(&format!(
                        "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
                        fmt(x1), fmt(y1), fmt(x2), fmt(y2),
                        hex(Color::from_ref(e.color)), fmt((e.width.raw() as f64 * s).max(0.4)),
                    ));
                }
            };
            edge(&b.top, rx0, ry0, rx1, ry0);
            edge(&b.bottom, rx0, ry1, rx1, ry1);
            edge(&b.left, rx0, ry0, rx0, ry1);
            edge(&b.right, rx1, ry0, rx1, ry1);
        }
    }
    // Inline objects (equations, treat-as-char tables) drawn at their reserved
    // slot: box top-left at (slot x, line top), except objects with a content
    // baseline (inline equations) drop so their baseline sits on the text line
    // baseline. ops are box-relative.
    for (k, io) in line.inline_objects.iter().enumerate() {
        let x = glyph_x.get(obj_slot[k]).copied().unwrap_or(cursor);
        let dy = match io.baseline {
            Some(b) => line.baseline - b,
            None => line.top,
        };
        let dx = x.round() as i32;
        for op in &io.ops {
            render_op(out, &translate_op(op, dx, dy), s, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Body text plus one trailing space; justify must ignore the trailing space.
    fn cells() -> Vec<(f64, bool)> {
        // "a b " — glyph a(10) space(5) glyph b(10) trailing-space(5).
        vec![(10.0, false), (5.0, true), (10.0, false), (5.0, true)]
    }

    #[test]
    fn justify_ignores_trailing_space() {
        // seg_width 40: body width (no trailing space) = 25, slack 15, 1 inner space.
        let f = line_fill(&cells(), 40.0, Align::Justify, false);
        assert_eq!(f.vis_end, 3, "끝 공백 글자는 레이아웃에서 제외");
        assert_eq!(f.start_delta, 0.0);
        assert_eq!(f.extra_space, 15.0, "slack 전부 내부 공백 1개로");
        assert_eq!(f.extra_gap, 0.0);
        // The last visible glyph b reaches exactly seg_width.
        let end = 10.0 + (5.0 + f.extra_space) + 10.0;
        assert_eq!(end, 40.0);
    }

    #[test]
    fn last_line_justify_left_aligned() {
        let f = line_fill(&cells(), 40.0, Align::Justify, true);
        assert_eq!(f.vis_end, 3);
        assert_eq!((f.start_delta, f.extra_space, f.extra_gap), (0.0, 0.0, 0.0));
    }

    #[test]
    fn last_line_justify_condenses_overflow() {
        // An over-full last/only line (body 25 > seg_width 20) is condensed to the
        // segment width over its inner space — the fit-to-cell behaviour. (Under-
        // full last lines stay left-aligned, per the test above.)
        let f = line_fill(&cells(), 20.0, Align::Justify, true);
        assert_eq!(f.vis_end, 3);
        assert_eq!(f.extra_space, -5.0, "slack -5 condensed onto the 1 inner space");
        // Last visible glyph meets exactly the segment width.
        let end = 10.0 + (5.0 + f.extra_space) + 10.0;
        assert_eq!(end, 20.0);
    }

    #[test]
    fn no_space_overflow_last_line_condenses_gaps() {
        // Hangul (no spaces) over-full last line condenses over the (n-1) gaps.
        let cells = vec![(10.0, false), (10.0, false), (10.0, false)];
        let f = line_fill(&cells, 24.0, Align::Justify, true);
        assert_eq!(f.extra_gap, -3.0, "slack -6 / (3-1) gaps");
        assert_eq!(f.extra_space, 0.0);
    }

    #[test]
    fn right_align_drops_trailing_space() {
        // Right align ignores the trailing space: body(25) meets the right edge(40), start_delta=15.
        let f = line_fill(&cells(), 40.0, Align::Right, false);
        assert_eq!(f.vis_end, 3);
        assert_eq!(f.start_delta, 15.0);
    }

    #[test]
    fn justify_no_space_spreads_gaps() {
        // No-space (Hangul) line: slack spreads over the (n-1) character gaps.
        let cells = vec![(10.0, false), (10.0, false), (10.0, false)];
        let f = line_fill(&cells, 36.0, Align::Justify, false);
        assert_eq!(f.vis_end, 3);
        assert_eq!(f.extra_gap, 3.0, "slack 6 / (3-1) gap");
        assert_eq!(f.extra_space, 0.0);
    }

    #[test]
    fn all_spaces_line_is_empty() {
        let cells = vec![(5.0, true), (5.0, true)];
        let f = line_fill(&cells, 40.0, Align::Justify, false);
        assert_eq!(f.vis_end, 0);
    }
}
