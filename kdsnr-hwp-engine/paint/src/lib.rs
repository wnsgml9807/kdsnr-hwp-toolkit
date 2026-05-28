//! Paginated layout to backend-independent paint operations.
//!
//! [`lower`] turns a [`PaginationResult`] plus its source [`DocumentModel`] into
//! one [`PaintPage`] per page: a flat, ordered list of paint operations in
//! absolute page coordinates (HWPUNIT). A backend (see the render crate) replays
//! these into an image. The operations carry the resolved content — text runs
//! with their font and stored baseline, table cell boxes, object boxes — read
//! from Hancom's stored geometry, not recomputed.
//!
//! Glyph-exact text (per-character advance from Hancom font outlines) is the
//! backend's font concern; this stage emits each line's text, font, and stored
//! origin, which fixes the line box exactly.

use std::collections::HashMap;

use kdsnr_hwp_core::{Rect, SourceRef};
use kdsnr_hwp_doc::{
    Anchor, AnchorAlign, AnchorRel, CellInfo, CellVAlign, DocumentModel, ObjectContent, ObjectInfo,
    ParagraphModel, SectionModel, TableInfo,
};
pub use kdsnr_hwp_doc::{Align, BorderEdge, BorderFillInfo, BorderStyle};
use kdsnr_hwp_layout::{BlockKind, PaginatedItem, PaginationResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const GRID: Color = Color { r: 120, g: 120, b: 120, a: 255 };
    pub const PLACEHOLDER: Color = Color { r: 235, g: 235, b: 235, a: 255 };

    /// HWP ColorRef (0x00BBGGRR) → opaque Color.
    pub fn from_ref(c: u32) -> Color {
        Color { r: (c & 0xFF) as u8, g: ((c >> 8) & 0xFF) as u8, b: ((c >> 16) & 0xFF) as u8, a: 255 }
    }
}

/// ColorRef shade → fill color; 0xFFFFFFFF ("none") yields None.
fn shade_of(c: u32) -> Option<Color> {
    (c != 0xFFFFFFFF).then(|| Color::from_ref(c))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaintDocument {
    pub pages: Vec<PaintPage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaintPage {
    pub paper: Rect,
    /// Paint operations in back-to-front order.
    pub ops: Vec<PaintOp>,
    /// Index range of body-content ops within `ops` (excludes the page
    /// background and header/footer/master furniture). Used to crop a question
    /// preview to its content alone.
    pub content_range: std::ops::Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaintOp {
    /// Solid fill (page background, cell shading).
    FillRect { rect: Rect, color: Color },
    /// Rectangle outline (table cell, object placeholder).
    StrokeRect { rect: Rect, color: Color, width: i32 },
    /// A single straight border edge (cell/char/paragraph border).
    Line { x1: i32, y1: i32, x2: i32, y2: i32, color: Color, width: i32, style: BorderStyle },
    /// Embedded raster placed in a box (object picture). `data` is the source
    /// file bytes, `ext` the format hint (png/jpg/bmp/gif).
    Image { rect: Rect, data: std::rc::Rc<Vec<u8>>, ext: String },
    /// One stored text line: its baseline origin and runs of constant font.
    TextLine(TextLine),
}

/// Emit a border edge as a `Line` op when visible.
fn push_edge(ops: &mut Vec<PaintOp>, e: &BorderEdge, x1: i32, y1: i32, x2: i32, y2: i32) {
    if e.style != BorderStyle::None {
        ops.push(PaintOp::Line {
            x1, y1, x2, y2,
            color: Color::from_ref(e.color),
            width: e.width.raw().max(1),
            style: e.style,
        });
    }
}

/// A single stored line of text, positioned at its Hancom baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub source: SourceRef,
    /// Left edge of the line (page x, HWPUNIT).
    pub x: i32,
    /// Baseline y (page y, HWPUNIT).
    pub baseline: i32,
    /// Stored line top (page y, HWPUNIT) — baseline minus baseline distance.
    /// Carried for diagnostics: lets a GT diff separate line-top placement from
    /// the baseline-within-line offset.
    pub top: i32,
    /// Stored line height (`vertsize`, HWPUNIT).
    pub line_height: i32,
    /// Stored segment width (`horzsize`, HWPUNIT) — the laid-out line width.
    pub seg_width: i32,
    /// Paragraph alignment, for placing text within the segment.
    pub align: Align,
    /// Last line of its paragraph (justify renders the last line left).
    pub is_last_line: bool,
    pub runs: Vec<TextRun>,
    /// Treat-as-char objects (e.g. inline equations) that sit in this line, each
    /// at a visual char index. Their ops are relative to the object box top-left;
    /// the renderer translates them to the char's x at the line top.
    pub inline_objects: Vec<InlineObject>,
}

/// A treat-as-char object placed within a text line at a visual char index. The
/// renderer resolves the char's x from glyph advances and translates `ops`.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineObject {
    /// Visual char index within the line where the object sits (run order: the
    /// object precedes the visible char at this index).
    pub char_index: usize,
    /// Box width (HWPUNIT) the object occupies in the line's glyph flow, so the
    /// following text advances past it instead of overlapping.
    pub advance: i32,
    /// Object content ops, relative to the box top-left (0,0).
    pub ops: Vec<PaintOp>,
    /// Content baseline offset from the box top (HWPUNIT), for objects that align
    /// to the text line baseline (inline equations). `None` → box top sits at the
    /// line top (default for images/tables).
    pub baseline: Option<i32>,
}

/// A run of characters sharing one font and shape metrics within a line.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub font: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Color,
    /// 장평 (width ratio) %.
    pub ratio: u16,
    /// 자간 (letter spacing) %.
    pub spacing: i16,
    /// 상대 크기 %.
    pub rel_sz: u16,
    /// 글자 위치 (세로 오프셋) — 글자 크기의 %, 양수 = 위로.
    pub char_offset: i16,
    /// 음영 (글자 배경) 색. None = 없음.
    pub shade: Option<Color>,
    pub strikeout: bool,
    pub strike_shape: u8,
    pub strike_color: Color,
    pub underline_color: Color,
    pub underline_shape: u8,
    /// 글자 테두리/배경.
    pub border: kdsnr_hwp_doc::BorderFillInfo,
    /// Stored width (HWPUNIT) of each tab in `text`, in order; consumed as tab advance.
    pub tab_widths: Vec<i32>,
    /// Face is a Hancom HFT font: advance + outline come from the HFT decoder.
    pub is_hft: bool,
}

/// Debug-only: render a single equation `script` into its own page, fitting the
/// engine's natural box to `font_size` (no object scaling). For inspecting
/// equation layout in isolation. `font_size` is in HWPUNIT (e.g. 1000 = 10pt).
pub fn debug_equation_page(script: &str, font: &str, font_size: u32) -> PaintPage {
    use kdsnr_hwp_equation::lower_equation_primitives;
    let base = if font_size > 0 { font_size as f64 } else { 1000.0 };
    let frag = lower_equation_primitives(script, base);
    let pad = (base * 0.5).round() as i32;
    let w = frag.natural_width.round() as i32;
    let h = frag.natural_height.round() as i32;
    let mut ops = Vec::new();
    emit_equation(
        script,
        font,
        0x000000,
        font_size,
        pad,
        pad,
        w,
        h,
        SourceRef::Page(kdsnr_hwp_core::PageId(0)),
        &mut ops,
    );
    let content_range = 0..ops.len();
    PaintPage {
        paper: Rect::new(0, 0, w + pad * 2, h + pad * 2),
        ops,
        content_range,
    }
}

/// Lower a pagination result into per-page paint operations.
pub fn lower(document: &DocumentModel, pagination: &PaginationResult) -> PaintDocument {
    let pages = pagination
        .pages
        .iter()
        .map(|page| {
            let section = document.sections.get(page.section.0);
            let lookup = section.map(paragraph_lookup).unwrap_or_default();
            // Anchor reference frames: paper edge and the page area inside margins.
            let frame = Frame {
                paper: page.paper,
                page: section.map(|s| s.page_area).unwrap_or(page.paper),
            };
            let mut ops = Vec::new();
            // Page background.
            ops.push(PaintOp::FillRect {
                rect: page.paper,
                color: Color::WHITE,
            });
            // Master furniture is drawn first (behind), then body, then header
            // and footer furniture on top, matching paint order.
            // A master column-divider line is stored full-height, but only spans
            // the page's multi-column region: above it (e.g. a full-width title
            // block on page 1) there is one column and no divider. Clip the
            // divider's top to where right-column content begins on this page.
            let gap_xs = column_gap_centers(&page.columns);
            let multicol_top = page
                .items
                .iter()
                .filter(|it| it.column >= 1)
                .map(|it| it.rect.y.raw())
                .min();
            for placement in &page.furniture {
                let start = ops.len();
                for item in &placement.items {
                    lower_item(item, &lookup, &frame, &mut ops);
                }
                if placement.role == kdsnr_hwp_layout::FurnitureRole::MasterPage {
                    if let Some(top) = multicol_top {
                        clip_column_dividers(&mut ops[start..], &gap_xs, top);
                    }
                }
            }
            // Body content begins here (after the page background and furniture).
            let body_start = ops.len();
            // Paragraph border boxes: fill behind the body, edges on top.
            let boxes = paragraph_border_boxes(&page.items, &lookup);
            for b in &boxes {
                if let Some(c) = b.border.fill {
                    ops.push(PaintOp::FillRect { rect: Rect::new(b.x, b.y, b.w, b.h), color: Color::from_ref(c) });
                }
            }
            for item in &page.items {
                lower_item(item, &lookup, &frame, &mut ops);
            }
            for b in &boxes {
                let (x, y, w, h) = (b.x, b.y, b.w, b.h);
                // A box split across a column/page keeps its side edges through the
                // break but leaves the break itself open: the upper fragment draws
                // no bottom edge, the lower fragment no top edge (Hancom RE'd from
                // the GT — a split passage box shows continuous verticals and no
                // horizontal at the column transition).
                if !b.open_top {
                    push_edge(&mut ops, &b.border.top, x, y, x + w, y);
                }
                if !b.open_bottom {
                    push_edge(&mut ops, &b.border.bottom, x, y + h, x + w, y + h);
                }
                push_edge(&mut ops, &b.border.left, x, y, x, y + h);
                push_edge(&mut ops, &b.border.right, x + w, y, x + w, y + h);
            }
            let content_range = body_start..ops.len();
            PaintPage {
                paper: page.paper,
                ops,
                content_range,
            }
        })
        .collect();
    PaintDocument { pages }
}

/// Midpoint x of each gap between consecutive columns — where a column divider sits.
fn column_gap_centers(columns: &[kdsnr_hwp_layout::PageColumn]) -> Vec<i32> {
    columns
        .windows(2)
        .map(|w| (w[0].rect.x.raw() + w[0].rect.width.raw() + w[1].rect.x.raw()) / 2)
        .collect()
}

/// Raise the top endpoint of any near-vertical, tall divider line sitting in a
/// column gap to `top`, so it does not extend above the page's multi-column
/// region (into a single-column area such as a page-1 title block).
fn clip_column_dividers(ops: &mut [PaintOp], gap_xs: &[i32], top: i32) {
    const X_TOL: i32 = 600; // ~6pt: divider centred on the gap
    for op in ops {
        if let PaintOp::Line { x1, y1, x2, y2, .. } = op {
            let vertical = (*x1 - *x2).abs() < 50;
            let tall = (*y1 - *y2).abs() > 20_000;
            let near_gap = gap_xs.iter().any(|g| (*x1 - *g).abs() < X_TOL);
            if vertical && tall && near_gap && top < (*y1).max(*y2) {
                if *y1 < top {
                    *y1 = top;
                }
                if *y2 < top {
                    *y2 = top;
                }
            }
        }
    }
}

/// Serialize the paint document's components as JSON, for measurement against a
/// ground-truth PDF. Coordinates are HWPUNIT (1/7200 inch); the consumer
/// converts. Each page lists its text lines (start x, baseline, fonts, sizes,
/// text) and its non-text boxes (object placeholders, table cell grid) so a diff
/// tool can find moved, resized, mis-fonted, or missing components.
pub fn components_json(document: &PaintDocument) -> String {
    let mut out = String::from("{\"pages\":[");
    for (pi, page) in document.pages.iter().enumerate() {
        if pi > 0 {
            out.push(',');
        }
        out.push_str("{\"page\":");
        out.push_str(&(pi + 1).to_string());
        out.push_str(",\"paper\":{\"w\":");
        out.push_str(&page.paper.width.raw().to_string());
        out.push_str(",\"h\":");
        out.push_str(&page.paper.height.raw().to_string());
        out.push_str("},\"text\":[");
        let mut first_text = true;
        for op in &page.ops {
            if let PaintOp::TextLine(line) = op {
                let text: String = line.runs.iter().map(|r| r.text.as_str()).collect();
                if !first_text {
                    out.push(',');
                }
                first_text = false;
                out.push_str("{\"x\":");
                out.push_str(&line.x.to_string());
                out.push_str(",\"baseline\":");
                out.push_str(&line.baseline.to_string());
                out.push_str(",\"top\":");
                out.push_str(&line.top.to_string());
                out.push_str(",\"line_height\":");
                out.push_str(&line.line_height.to_string());
                out.push_str(",\"size_pt\":");
                out.push_str(&line.runs.first().map(|r| r.size_pt).unwrap_or(0.0).to_string());
                out.push_str(",\"font\":\"");
                out.push_str(&json_escape(line.runs.first().map(|r| r.font.as_str()).unwrap_or("")));
                out.push_str("\",\"bold\":");
                out.push_str(if line.runs.first().map(|r| r.bold).unwrap_or(false) { "true" } else { "false" });
                out.push_str(",\"text\":\"");
                out.push_str(&json_escape(&text));
                out.push_str("\"}");
            }
        }
        out.push_str("],\"boxes\":[");
        let mut first_box = true;
        for op in &page.ops {
            let (role, rect) = match op {
                PaintOp::StrokeRect { rect, .. } => ("box", rect),
                // Skip the page-background fill; report only placeholder fills.
                PaintOp::FillRect { rect, color } if *color == Color::PLACEHOLDER => ("object", rect),
                _ => continue,
            };
            if !first_box {
                out.push(',');
            }
            first_box = false;
            out.push_str("{\"role\":\"");
            out.push_str(role);
            out.push_str("\",\"x\":");
            out.push_str(&rect.x.raw().to_string());
            out.push_str(",\"y\":");
            out.push_str(&rect.y.raw().to_string());
            out.push_str(",\"w\":");
            out.push_str(&rect.width.raw().to_string());
            out.push_str(",\"h\":");
            out.push_str(&rect.height.raw().to_string());
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Index every paragraph reachable as a paint source in a section by its id
/// index: body, endnotes, and header/footer/master furniture all use disjoint
/// index ranges, so one map resolves any item's source.
fn paragraph_lookup(section: &SectionModel) -> HashMap<usize, &ParagraphModel> {
    let mut map: HashMap<usize, &ParagraphModel> = HashMap::new();
    let lists = section
        .paragraphs
        .iter()
        .chain(section.endnotes.iter())
        .chain(section.headers.iter().flat_map(|h| h.paragraphs.iter()))
        .chain(section.footers.iter().flat_map(|f| f.paragraphs.iter()))
        .chain(section.master_pages.iter().flat_map(|m| m.paragraphs.iter()));
    for p in lists {
        map.insert(p.id.index, p);
    }
    map
}

/// A connected paragraph border box (union of consecutive same-id paragraphs,
/// expanded by the border offsets).
struct BorderBox {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    border: kdsnr_hwp_doc::BorderFillInfo,
    /// The box continues from a previous column/page (this fragment is not the
    /// box's true top) — its top edge stays open at the break.
    open_top: bool,
    /// The box continues into a following column/page (not its true bottom) — its
    /// bottom edge stays open at the break.
    open_bottom: bool,
}

/// A paragraph carries a visible border box (a fill or any visible edge) under a
/// real border_fill_id.
fn bordered_visible(p: &ParagraphModel) -> bool {
    let b = &p.border;
    p.border_fill_id != 0
        && (b.fill.is_some()
            || b.left.visible()
            || b.right.visible()
            || b.top.visible()
            || b.bottom.visible())
}

/// A body item that is a bordered paragraph → (paragraph, its border_fill_id).
fn bordered_paragraph<'a>(
    item: &PaginatedItem,
    lookup: &HashMap<usize, &'a ParagraphModel>,
) -> Option<(&'a ParagraphModel, u16)> {
    if item.kind != BlockKind::Paragraph {
        return None;
    }
    let SourceRef::Paragraph(id) = item.source else { return None };
    let p = *lookup.get(&id.index)?;
    bordered_visible(p).then_some((p, p.border_fill_id))
}

/// Group bordered paragraphs into boxes. Consecutive paragraphs sharing a
/// `border_fill_id` join one connected region only when the later one carries
/// `border_connect` (`<hh:border connect>`); without it each paragraph is its own
/// closed box. A region split across columns/pages keeps its sides through the
/// break and leaves the break itself open (no horizontal at the column/page
/// boundary), since that edge is interior to the connected region.
fn paragraph_border_boxes(
    items: &[PaginatedItem],
    lookup: &HashMap<usize, &ParagraphModel>,
) -> Vec<BorderBox> {
    // Paragraph `idx` is a bordered paragraph of border `id`.
    let is_box = |idx: usize, id: u16| -> bool {
        lookup
            .get(&idx)
            .map(|p| p.border_fill_id == id && bordered_visible(p))
            .unwrap_or(false)
    };
    // Paragraph `idx` connects upward into border `id` (same border, and its
    // `connect` flag joins it to the previous paragraph).
    let connects_up = |idx: usize, id: u16| -> bool {
        lookup
            .get(&idx)
            .map(|p| p.border_fill_id == id && bordered_visible(p) && p.border_connect)
            .unwrap_or(false)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let Some((p, id)) = bordered_paragraph(&items[i], lookup) else {
            i += 1;
            continue;
        };
        // Group origin column: a connected box that flows into the next column
        // (or page) breaks into a separate box per column, so only extend while
        // the item shares this column's left x and keeps connecting upward.
        let col_x = items[i].rect.x.raw();
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        let (mut first_idx, mut last_idx) = (usize::MAX, 0usize);
        let mut j = i;
        while j < items.len() {
            match (items[j].source, bordered_paragraph(&items[j], lookup)) {
                (SourceRef::Paragraph(pid), Some((pp, idj)))
                    if idj == id
                        && (items[j].rect.x.raw() - col_x).abs() < 1000
                        && (j == i || pp.border_connect) =>
                {
                    let r = items[j].rect;
                    x0 = x0.min(r.x.raw());
                    y0 = y0.min(r.y.raw());
                    x1 = x1.max(r.x.raw() + r.width.raw());
                    y1 = y1.max(r.y.raw() + r.height.raw());
                    first_idx = first_idx.min(pid.index);
                    last_idx = last_idx.max(pid.index);
                    j += 1;
                }
                _ => break,
            }
        }
        // The connected region continues past this fragment when the paragraph
        // just before its first connects up into it, or the one just after its
        // last connects up into the region. The break then stays open: the
        // fragment continued from above draws no top edge, the one continued below
        // no bottom edge. Document-order neighbours catch column and page splits.
        let open_top = first_idx > 0 && connects_up(first_idx, id) && is_box(first_idx - 1, id);
        let open_bottom = connects_up(last_idx + 1, id);
        let o = &p.border_offsets;
        let (bx, by) = (x0 - o.left.raw(), y0 - o.top.raw());
        out.push(BorderBox {
            x: bx,
            y: by,
            w: (x1 + o.right.raw()) - bx,
            h: (y1 + o.bottom.raw()) - by,
            border: p.border,
            open_top,
            open_bottom,
        });
        i = j;
    }
    out
}

/// Reference frames for resolving a floating object's `<hp:pos>` anchor: the
/// paper edge and the page area inside the margins.
#[derive(Clone, Copy)]
struct Frame {
    paper: Rect,
    page: Rect,
}

/// Align an object's `size` within `[base, base+ref_len]` per `align`, then add
/// the offset (subtracted for far-edge alignment, so a positive offset insets).
fn align_axis(align: AnchorAlign, base: i32, ref_len: i32, size: i32, offset: i32) -> i32 {
    match align {
        AnchorAlign::Center => base + (ref_len - size) / 2 + offset,
        AnchorAlign::Right | AnchorAlign::Bottom => base + ref_len - size - offset,
        // Left/Top/None: offset from the near edge.
        _ => base + offset,
    }
}

/// Resolve a floating object's box top-left.
///
/// Each axis aligns the object's size within its reference span per the stored
/// `horz_align`/`vert_align`, then applies the offset:
/// - Paper/Page: absolute spans (paper edge, page area inside margins).
/// - Column/Para: the flowed reference at `(rel_x, rel_y)`. Horizontally the
///   span is `flow_w` (the anchoring paragraph's segment width = its column/para
///   content width), so a CENTER/RIGHT object lands at the column centre/right
///   instead of collapsing to the left. Vertically the object hangs from the
///   paragraph top (`rel_y + v_offset`); column/para vertical alignment is TOP in
///   practice and a reliable flow height is not available.
fn anchor_xy(a: &Anchor, w: i32, h: i32, rel_x: i32, rel_y: i32, flow_w: i32, frame: &Frame) -> (i32, i32) {
    let ho = a.h_offset.raw();
    let x = match a.horz_rel {
        AnchorRel::Paper => align_axis(a.horz_align, frame.paper.x.raw(), frame.paper.width.raw(), w, ho),
        AnchorRel::Page => align_axis(a.horz_align, frame.page.x.raw(), frame.page.width.raw(), w, ho),
        AnchorRel::Column | AnchorRel::Para => align_axis(a.horz_align, rel_x, flow_w, w, ho),
    };
    let vo = a.v_offset.raw();
    let y = match a.vert_rel {
        AnchorRel::Paper => align_axis(a.vert_align, frame.paper.y.raw(), frame.paper.height.raw(), h, vo),
        AnchorRel::Page => align_axis(a.vert_align, frame.page.y.raw(), frame.page.height.raw(), h, vo),
        AnchorRel::Column | AnchorRel::Para => rel_y + vo,
    };
    (x, y)
}

/// The anchoring reference width for a paragraph's Column/Para floating objects:
/// its stored segment width (the column/para content width). Falls back to the
/// page area width when no segment is stored.
fn para_flow_width(para: &ParagraphModel, frame: &Frame) -> i32 {
    para
        .stored_line_segs
        .first()
        .map(|s| s.segment_width.raw())
        .filter(|w| *w > 0)
        .unwrap_or_else(|| frame.page.width.raw())
}

fn lower_item(
    item: &PaginatedItem,
    lookup: &HashMap<usize, &ParagraphModel>,
    frame: &Frame,
    ops: &mut Vec<PaintOp>,
) {
    match item.kind {
        BlockKind::Paragraph => {
            if let SourceRef::Paragraph(id) = item.source {
                if let Some(para) = lookup.get(&id.index) {
                    // Map the item's stored top back to a paragraph origin so
                    // each line's stored vertpos lands at its page position.
                    // `fragment_range` is half-open; `emit_paragraph_lines` takes an
                    // inclusive last line, so convert. (A run split at a column break
                    // must not paint the next column's first line, whose stored
                    // vertpos resets to the column top.)
                    let (first, end) = item.fragment_range;
                    let last = end.saturating_sub(1);
                    let first_vp = para
                        .stored_line_segs
                        .get(first)
                        .map(|s| s.vertical_pos.raw())
                        .unwrap_or(0);
                    let origin_x = item.rect.x.raw();
                    let origin_y = item.rect.y.raw() - first_vp;
                    // A PARA/COLUMN-anchored floating object rides with its
                    // paragraph: its `<hp:pos>` vertical offset is measured from the
                    // paragraph's flowed top, not the body top. `origin_y` is the
                    // body top (so `origin_y + vertpos` lands each text line); the
                    // object base is the paragraph's first-line top
                    // (`origin_y + first_vp` = this item's flowed `rect.y`), matching
                    // the floating-table and cell-object paths.
                    let obj_y = origin_y + first_vp;
                    let flow_w = para_flow_width(para, frame);
                    // Anchored objects ride with their paragraph; emit once (first
                    // fragment). Behind-text layer below the text, others above.
                    if first == 0 {
                        emit_objects(&para.objects, origin_x, obj_y, flow_w, true, item.source, frame, ops);
                    }
                    emit_paragraph_lines(para, origin_x, origin_y, first, last, item.source, frame, ops);
                    if first == 0 {
                        emit_objects(&para.objects, origin_x, obj_y, flow_w, false, item.source, frame, ops);
                        // Para/Column-anchored floating tables that reserve no band
                        // ride with the paragraph (Square/Tight/Through wrap): they
                        // hang from the paragraph's flowed top like floating objects.
                        // (Paper/Page-anchored and band-reserving tables are placed
                        // as their own blocks instead — see measure_blocks.)
                        for table in &para.tables {
                            if let Some(a) = &table.anchor {
                                let rides = !matches!(a.vert_rel, AnchorRel::Paper | AnchorRel::Page)
                                    && !table.reserves_vertical_band;
                                if rides {
                                    let (tx, ty) = anchor_xy(
                                        a,
                                        table.width.raw(),
                                        table.height.raw(),
                                        origin_x,
                                        obj_y,
                                        flow_w,
                                        frame,
                                    );
                                    emit_table(table, tx, ty, item.source, frame, ops);
                                }
                            }
                        }
                    }
                }
            }
        }
        BlockKind::Table => {
            if let SourceRef::Control(cid) = item.source {
                if let Some(para) = lookup.get(&cid.paragraph.index) {
                    if let Some(table) = para.tables.get(cid.index) {
                        // A floating (Paper/Page-anchored) furniture table sits at
                        // its absolute anchor; an inline table uses its flowed rect.
                        let (tx, ty) = match &table.anchor {
                            Some(a) => anchor_xy(
                                a,
                                table.width.raw(),
                                table.height.raw(),
                                item.rect.x.raw(),
                                item.rect.y.raw(),
                                para_flow_width(para, frame),
                                frame,
                            ),
                            None => (item.rect.x.raw(), item.rect.y.raw()),
                        };
                        emit_table(table, tx, ty, item.source, frame, ops);
                    }
                }
            }
        }
        BlockKind::Picture | BlockKind::Shape | BlockKind::Equation => {
            // Object pixels/geometry are a later fidelity step; mark the box.
            ops.push(PaintOp::FillRect {
                rect: item.rect,
                color: Color::PLACEHOLDER,
            });
            ops.push(PaintOp::StrokeRect {
                rect: item.rect,
                color: Color::GRID,
                width: 100,
            });
        }
    }
}

/// Emit text lines `[first, last]` of a paragraph at the given origin. The line
/// x is `origin_x + column_start`; the baseline is `origin_y + vertpos +
/// baseline`. Each line's text is sliced by stored `text_start` and split into
/// runs of constant font.
/// Emit a paragraph's anchored objects (pictures/shapes) at their box. The box
/// top-left is the object's anchor: absolute for a Paper/Page reference (the
/// furniture/floating case), else the paragraph content origin plus the stored
/// offset (PARA/COLUMN). The object's reserved space is already in the stored
/// line geometry, so this only paints, never re-flows.
#[allow(clippy::too_many_arguments)]
fn emit_objects(
    objects: &[ObjectInfo],
    origin_x: i32,
    origin_y: i32,
    flow_w: i32,
    behind: bool,
    source: SourceRef,
    frame: &Frame,
    ops: &mut Vec<PaintOp>,
) {
    for obj in objects {
        // Object z-order: behind-text below the text, others above. This pass = one layer.
        if obj.behind_text != behind {
            continue;
        }
        // Treat-as-char objects flow in their text line; emitted there, not here.
        if obj.treat_as_char {
            continue;
        }
        let (w, h) = (obj.width.raw(), obj.height.raw());
        let (x, y) = anchor_xy(&obj.anchor, w, h, origin_x, origin_y, flow_w, frame);
        emit_object_content(obj, x, y, source, frame, ops);
    }
}

/// Emit an object's content (image/rect/line/equation + drawText) with its box
/// top-left at `(x, y)`. Shared by floating placement (anchor x,y) and inline
/// placement (relative x,y, then translated to the in-line position).
/// Returns the content baseline offset from the box top (HWPUNIT) when meaningful
/// for inline alignment (equations); `None` for box-aligned objects.
fn emit_object_content(obj: &ObjectInfo, x: i32, y: i32, source: SourceRef, frame: &Frame, ops: &mut Vec<PaintOp>) -> Option<i32> {
    let (w, h) = (obj.width.raw(), obj.height.raw());
    let mut inline_baseline = None;
    match &obj.content {
        ObjectContent::Image { data, ext } => {
            ops.push(PaintOp::Image {
                rect: Rect::new(x, y, w, h),
                data: data.clone(),
                ext: ext.clone(),
            });
        }
        ObjectContent::Rect { fill, border_color, border_width, border } => {
            if let Some(c) = fill {
                ops.push(PaintOp::FillRect {
                    rect: Rect::new(x, y, w, h),
                    color: Color::from_ref(*c),
                });
            }
            // A NONE outline is an invisible guide rect; Hancom draws nothing.
            if *border != BorderStyle::None && *border_width > 0 {
                let edge = BorderEdge {
                    style: *border,
                    width: kdsnr_hwp_core::EngineUnit::new(*border_width),
                    color: *border_color,
                };
                push_edge(ops, &edge, x, y, x + w, y);
                push_edge(ops, &edge, x, y + h, x + w, y + h);
                push_edge(ops, &edge, x, y, x, y + h);
                push_edge(ops, &edge, x + w, y, x + w, y + h);
            }
        }
        ObjectContent::Line { x1, y1, x2, y2, color, width, style } => {
            if *style != BorderStyle::None {
                ops.push(PaintOp::Line {
                    x1: x + x1, y1: y + y1, x2: x + x2, y2: y + y2,
                    color: Color::from_ref(*color),
                    width: (*width).max(1),
                    style: *style,
                });
            }
        }
        ObjectContent::Equation { script, font, color, font_size } => {
            inline_baseline = Some(emit_equation(script, font, *color, *font_size, x, y, w, h, source, ops));
        }
        ObjectContent::None => {}
    }
    // drawText: text laid out inside the object box (header/footer labels, text
    // boxes). Drawn after the shape fill/border, within the same layer.
    if let Some(tb) = &obj.text_box {
        emit_text_box(tb, x, y, w, h, source, frame, ops);
    }
    inline_baseline
}

/// Render a drawing object's text box (drawText) inside its box: paragraphs at
/// the inner-margin corner, vertically aligned like a table cell.
fn emit_text_box(
    tb: &kdsnr_hwp_doc::ObjectTextBox,
    x: i32,
    y: i32,
    _w: i32,
    h: i32,
    source: SourceRef,
    frame: &Frame,
    ops: &mut Vec<PaintOp>,
) {
    let content_h = tb
        .paragraphs
        .iter()
        .flat_map(|p| &p.stored_line_segs)
        .map(|s| s.vertical_pos.raw() + s.line_height.raw())
        .max()
        .unwrap_or(0);
    let content_x = x + tb.margin.left.raw();
    let content_y = cell_content_top(
        tb.vertical_align,
        y,
        h,
        tb.margin.top.raw(),
        tb.margin.bottom.raw(),
        content_h,
    );
    for para in &tb.paragraphs {
        if para.stored_line_segs.is_empty() {
            continue;
        }
        let last = para.stored_line_segs.len() - 1;
        emit_paragraph_lines(para, content_x, content_y, 0, last, source, frame, ops);
    }
}

/// Lower an equation script to glyph primitives and emit them scaled into the
/// object box. Text glyphs (HYHWPEQ PUA codes) become `TextRun`s in the equation
/// font, which the render backend resolves via the font crate (HANCOM_FONT_DIR);
/// rule/box primitives become lines. `font_size` is the equation's base size; the
/// natural box is then scaled uniformly to the stored object height.
#[allow(clippy::too_many_arguments)]
/// Returns the equation baseline offset (HWPUNIT from the box top), so an inline
/// caller can align it to the text line baseline.
#[allow(clippy::too_many_arguments)]
fn emit_equation(
    script: &str,
    font: &str,
    color: u32,
    font_size: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    source: SourceRef,
    ops: &mut Vec<PaintOp>,
) -> i32 {
    use kdsnr_hwp_equation::{lower_equation_primitives, EquationPrimitive};
    let base = if font_size > 0 { font_size as f64 } else { 1000.0 };
    let frag = lower_equation_primitives(script, base);
    if frag.natural_height <= 0.0 {
        return h;
    }
    // Render at the base font size: the natural box is already in baseUnit-based
    // units, so no rescaling. The stored object height/width are line-layout metrics
    // (the equation reserves that line space and may exceed it in ink for stacked
    // fractions/subscripts) — scaling ink to them shrinks tall equations (e.g. the
    // log₍½₎ inequality) below the surrounding text. GT renders at the base size.
    let at = |v: f64| v.round() as i32;
    let col = Color::from_ref(color);
    // The equation is left-aligned at its natural width; any surplus stored box width
    // is trailing margin. Alignment tabs (`&`, U+0009) are zero-width, not drawn.
    let _ = (w, h);
    for prim in &frag.primitives {
        match prim {
            EquationPrimitive::Text { x: px, baseline, text, font_size: fs, x_scale, .. } => {
                if text == "\u{0009}" {
                    continue; // alignment tab marker — not drawn
                }
                let glyph_h = at(*fs);
                ops.push(PaintOp::TextLine(TextLine {
                    source,
                    x: x + at(*px),
                    baseline: y + at(*baseline),
                    top: y + at(*baseline) - glyph_h,
                    line_height: glyph_h,
                    seg_width: 0,
                    align: Align::Left,
                    is_last_line: true,
                    // HYhwpEQ has no Hangul/CJK glyph (the box labels 가/나/다 etc.);
                    // those render from the Hangul font, like the surrounding text.
                    runs: vec![equation_run(
                        text.clone(),
                        if text.chars().any(is_cjk) { "함초롬바탕" } else { font },
                        glyph_h,
                        *x_scale,
                        col,
                    )],
                    inline_objects: Vec::new(),
                }));
            }
            EquationPrimitive::Line { x1, y1, x2, y2, stroke_width, .. } => {
                ops.push(PaintOp::Line {
                    x1: x + at(*x1), y1: y + at(*y1),
                    x2: x + at(*x2), y2: y + at(*y2),
                    color: col,
                    width: at(*stroke_width).max(1),
                    style: BorderStyle::Solid,
                });
            }
            EquationPrimitive::Rectangle { x: rx, y: ry, width, height, stroke_width, .. } => {
                let edge = BorderEdge {
                    style: BorderStyle::Solid,
                    width: kdsnr_hwp_core::EngineUnit::new(at(*stroke_width).max(1)),
                    color,
                };
                let (x0, y0) = (x + at(*rx), y + at(*ry));
                let (x1, y1) = (x0 + at(*width), y0 + at(*height));
                push_edge(ops, &edge, x0, y0, x1, y0);
                push_edge(ops, &edge, x0, y1, x1, y1);
                push_edge(ops, &edge, x0, y0, x0, y1);
                push_edge(ops, &edge, x1, y0, x1, y1);
            }
            EquationPrimitive::Guide { .. } => {}
        }
    }
    at(frag.natural_baseline)
}

/// Hangul syllables/jamo and CJK ideographs — equation glyphs that HYhwpEQ lacks
/// and that must resolve from the Hangul font instead.
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{1100}'..='\u{11FF}'   // Hangul Jamo
        | '\u{3130}'..='\u{318F}' // Hangul Compatibility Jamo
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{3400}'..='\u{9FFF}' // CJK ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK compat ideographs
    )
}

/// The horizontal advance (HWPUNIT) an inline equation occupies: its natural
/// content width scaled to the stored object height. Used instead of the stored
/// box width so following text hugs the equation (the box may carry trailing
/// alignment margin the layout no longer spreads into the content).
fn equation_content_advance(script: &str, font_size: u32, _h: i32) -> Option<i32> {
    use kdsnr_hwp_equation::lower_equation_primitives;
    let base = if font_size > 0 { font_size as f64 } else { 1000.0 };
    let frag = lower_equation_primitives(script, base);
    if frag.natural_height <= 0.0 {
        return None;
    }
    // Rendered at the base size (scale 1), so the advance is the natural width.
    Some(frag.natural_width.round() as i32)
}

/// A plain text run for an equation glyph (no shade/border/strike), at the given
/// font and glyph height (HWPUNIT → pt). Glyphs resolve via the font crate.
fn equation_run(text: String, font: &str, glyph_h: i32, x_scale: f64, color: Color) -> TextRun {
    TextRun {
        text,
        font: font.to_string(),
        size_pt: glyph_h as f32 / 100.0,
        bold: false,
        italic: false,
        underline: false,
        color,
        // 장평: x_scale<1 narrows the glyph (used to stretch a sign in height only).
        ratio: (x_scale * 100.0).round().clamp(1.0, 1000.0) as u16,
        spacing: 0,
        rel_sz: 100,
        char_offset: 0,
        shade: None,
        strikeout: false,
        strike_shape: 0,
        strike_color: Color::BLACK,
        underline_color: Color::BLACK,
        underline_shape: 0,
        border: kdsnr_hwp_doc::BorderFillInfo::default(),
        tab_widths: Vec::new(),
        is_hft: false,
    }
}

fn emit_paragraph_lines(
    para: &ParagraphModel,
    origin_x: i32,
    origin_y: i32,
    first: usize,
    last: usize,
    source: SourceRef,
    frame: &Frame,
    ops: &mut Vec<PaintOp>,
) {
    let units: Vec<u16> = para.text.encode_utf16().collect();
    let segs = &para.stored_line_segs;
    // A line segment's `text_start` is a gapped UTF-16 offset (an inline object
    // marker, absent from `text`, still advances it by 8). Map it to a visible
    // index so text slicing and inline placement share one coordinate system.
    let to_vis = |g: usize| -> usize {
        if para.char_offsets.is_empty() {
            g
        } else {
            para.char_offsets.partition_point(|&o| (o as usize) < g)
        }
    };
    for j in first..=last.min(segs.len().saturating_sub(1)) {
        let seg = &segs[j];
        let start = to_vis(seg.text_start as usize).min(units.len());
        let end = to_vis(
            segs.get(j + 1).map(|n| n.text_start as usize).unwrap_or(usize::MAX),
        )
        .min(units.len());
        let runs = line_runs(para, &units, start, end);
        // Treat-as-char objects and tables whose visible char index falls in this
        // line. Each reserves its box width as advance so following text flows
        // past it; a table alone on its line keeps the line even with no text runs.
        let is_last_seg = j == segs.len() - 1;
        let in_line = |pos: usize| pos >= start && (pos < end || (is_last_seg && pos <= end));
        let mut inline_objects = Vec::new();
        for obj in &para.objects {
            if !obj.treat_as_char {
                continue;
            }
            if let Some(pos) = obj.inline_pos {
                let pos = pos as usize;
                if in_line(pos) {
                    let mut rel = Vec::new();
                    let baseline = emit_object_content(obj, 0, 0, source, frame, &mut rel);
                    // An inline equation advances by its content width, not the stored
                    // box width: the box may carry trailing alignment margin (`&`
                    // equations) that Hancom does not advance past inline — the next
                    // text hugs the equation (GT). Other objects use their box width.
                    let advance = match &obj.content {
                        ObjectContent::Equation { script, font_size, .. } => {
                            equation_content_advance(script, *font_size, obj.height.raw())
                                .unwrap_or_else(|| obj.width.raw())
                        }
                        _ => obj.width.raw(),
                    };
                    inline_objects.push(InlineObject {
                        char_index: pos - start,
                        advance,
                        ops: rel,
                        baseline,
                    });
                }
            }
        }
        for table in &para.tables {
            // Floating tables are painted at their anchor as a block, not inline.
            if table.anchor.is_some() {
                continue;
            }
            if let Some(pos) = table.inline_pos {
                let pos = pos as usize;
                if in_line(pos) {
                    let mut rel = Vec::new();
                    emit_table(table, 0, 0, source, frame, &mut rel);
                    inline_objects.push(InlineObject {
                        char_index: pos - start,
                        advance: table.width.raw(),
                        ops: rel,
                        baseline: None,
                    });
                }
            }
        }
        if runs.is_empty() && inline_objects.is_empty() {
            continue;
        }
        let top = origin_y + seg.vertical_pos.raw();
        // Stored `column_start` (horzpos) already carries the paragraph left
        // margin. `<hc:intent>` is signed (HWP 들여쓰기/내어쓰기): positive indents
        // the FIRST line right (들여쓰기); negative is a hanging indent (내어쓰기) —
        // the first line stays at the margin and the CONTINUATION lines shift
        // right by |intent| (so a marker + tab sits at the margin and wrapped text
        // aligns under it). The segment's right edge is fixed, so the line's fill
        // width shrinks by whatever the start shifts right.
        let fli = para.first_line_indent.raw();
        let indent = if fli >= 0 {
            if j == 0 { fli } else { 0 }
        } else if j == 0 {
            0
        } else {
            -fli
        };
        let line_x = origin_x + seg.column_start.raw() + indent;
        let baseline = top + seg.baseline_distance.raw();
        // Heading number/bullet prefix on the paragraph's first line: placed in
        // the outdent to the left of the text (ends at the text start).
        if j == 0 {
            if let (Some(num), Some(c0)) = (&para.auto_number, para.chars.first()) {
                let size_hwp = (c0.font_size_pt * 100.0) as f64 * c0.rel_sz as f64 / 100.0;
                let est_w = (num.chars().count() as f64 * size_hwp * 0.5) as i32;
                ops.push(PaintOp::TextLine(TextLine {
                    source,
                    x: line_x - est_w,
                    baseline,
                    top,
                    line_height: seg.line_height.raw(),
                    seg_width: 0,
                    align: Align::Left,
                    is_last_line: true,
                    runs: vec![number_run(num.clone(), c0)],
                    inline_objects: Vec::new(),
                }));
            }
        }
        ops.push(PaintOp::TextLine(TextLine {
            source,
            x: line_x,
            baseline,
            top,
            line_height: seg.line_height.raw(),
            seg_width: seg.segment_width.raw() - indent,
            align: para.align,
            is_last_line: is_last_seg,
            runs,
            inline_objects,
        }));
    }
}

/// A text run carrying a heading number/bullet, styled from the paragraph's
/// first character (font/size/color/ratio/spacing).
fn number_run(text: String, c: &kdsnr_hwp_doc::ResolvedChar) -> TextRun {
    TextRun {
        text,
        font: c.font_face.clone(),
        size_pt: c.font_size_pt,
        bold: c.bold,
        italic: c.italic,
        underline: false,
        color: Color::from_ref(c.color),
        ratio: c.ratio,
        spacing: c.spacing,
        rel_sz: c.rel_sz,
        char_offset: 0,
        shade: None,
        strikeout: false,
        strike_shape: 0,
        strike_color: Color::BLACK,
        underline_color: Color::BLACK,
        underline_shape: 0,
        border: kdsnr_hwp_doc::BorderFillInfo::default(),
        tab_widths: Vec::new(),
        is_hft: c.is_hft,
    }
}

/// Split a line's UTF-16 range into runs of constant font, decoding each back to
/// a string.
/// Formatting that defines a run boundary — a contiguous span sharing it.
#[derive(Clone, PartialEq)]
struct RunFmt {
    font: String,
    size_pt: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    ratio: u16,
    spacing: i16,
    rel_sz: u16,
    char_offset: i16,
    color: u32,
    shade: u32,
    strikeout: bool,
    strike_shape: u8,
    strike_color: u32,
    underline_color: u32,
    underline_shape: u8,
    border: kdsnr_hwp_doc::BorderFillInfo,
    is_hft: bool,
}

impl RunFmt {
    fn of(c: &kdsnr_hwp_doc::ResolvedChar) -> RunFmt {
        RunFmt {
            font: c.font_face.clone(),
            is_hft: c.is_hft,
            size_pt: c.font_size_pt,
            bold: c.bold,
            italic: c.italic,
            underline: c.underline,
            ratio: c.ratio,
            spacing: c.spacing,
            rel_sz: c.rel_sz,
            char_offset: c.char_offset,
            color: c.color,
            shade: c.shade,
            strikeout: c.strikeout,
            strike_shape: c.strike_shape,
            strike_color: c.strike_color,
            underline_color: c.underline_color,
            underline_shape: c.underline_shape,
            border: c.border,
        }
    }
}

fn line_runs(para: &ParagraphModel, units: &[u16], start: usize, end: usize) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut buf: Vec<u16> = Vec::new();
    let mut tab_buf: Vec<i32> = Vec::new();
    let mut cur: Option<RunFmt> = None;
    let flush =
        |runs: &mut Vec<TextRun>, buf: &mut Vec<u16>, tab_buf: &mut Vec<i32>, cur: &Option<RunFmt>| {
            if buf.is_empty() {
                return;
            }
            if let Some(f) = cur {
                let text = String::from_utf16_lossy(buf);
                // Keep a tab-only run for its advance (trim drops '\t', so test it separately).
                if !text.trim().is_empty() || text.contains(' ') || !tab_buf.is_empty() {
                    runs.push(TextRun {
                        text,
                        font: f.font.clone(),
                        size_pt: f.size_pt,
                        bold: f.bold,
                        italic: f.italic,
                        underline: f.underline,
                        color: Color::from_ref(f.color),
                        ratio: f.ratio,
                        spacing: f.spacing,
                        rel_sz: f.rel_sz,
                        char_offset: f.char_offset,
                        shade: shade_of(f.shade),
                        strikeout: f.strikeout,
                        strike_shape: f.strike_shape,
                        strike_color: Color::from_ref(f.strike_color),
                        underline_color: Color::from_ref(f.underline_color),
                        underline_shape: f.underline_shape,
                        border: f.border,
                        tab_widths: tab_buf.clone(),
                        is_hft: f.is_hft,
                    });
                }
            }
            buf.clear();
            tab_buf.clear();
        };
    for i in start..end {
        let key = para.chars.get(i).map(RunFmt::of);
        if cur != key {
            flush(&mut runs, &mut buf, &mut tab_buf, &cur);
            cur = key;
        }
        buf.push(units[i]);
        if units[i] == 0x0009 {
            tab_buf.push(para.chars.get(i).map(|c| c.tab_width).unwrap_or(0));
        }
    }
    flush(&mut runs, &mut buf, &mut tab_buf, &cur);
    runs
}

/// Emit a table's cell grid (boxes) and cell text at the table origin. Column
/// widths and row heights come from stored cell extents; cells are positioned by
/// summing the cells to their left/above.
fn emit_table(table: &TableInfo, ox: i32, oy: i32, source: SourceRef, frame: &Frame, ops: &mut Vec<PaintOp>) {
    let n_cols = table.cols as usize;
    let n_rows = table.rows as usize;
    if n_cols == 0 || n_rows == 0 {
        return;
    }
    // Column widths come from single-span cells (each gives one column's exact
    // width). A column-spanning cell must NOT distribute its width across the
    // columns it covers — splitting `width/span` and taking the max inflates a
    // narrow column (e.g. a gap) past its real size, pushing the table border out
    // beyond `table.width`. A spanning cell only fills columns no single-span cell
    // sized (rare), with the remaining width spread over those.
    let mut col_w = vec![0i32; n_cols];
    for c in &table.cells {
        if c.col_span.max(1) == 1 {
            if let Some(slot) = col_w.get_mut(c.col as usize) {
                *slot = (*slot).max(c.width.raw());
            }
        }
    }
    for c in &table.cells {
        let cs = c.col_span.max(1) as usize;
        if cs == 1 {
            continue;
        }
        let cols = c.col as usize..(c.col as usize + cs).min(n_cols);
        let uncovered: Vec<usize> = cols.clone().filter(|&i| col_w[i] == 0).collect();
        if uncovered.is_empty() {
            continue;
        }
        let known: i32 = cols.map(|i| col_w[i]).sum();
        let each = (c.width.raw() - known).max(0) / uncovered.len() as i32;
        for i in uncovered {
            col_w[i] = each;
        }
    }
    // Row heights come from the stored box height (the same recovery layout uses
    // for vertical accumulation), so auto cells (cellSz height=0) still get the
    // box's real height instead of collapsing to zero.
    let row_h = table.stored_row_heights().unwrap_or_else(|| vec![0i32; n_rows]);
    let col_x: Vec<i32> = prefix_sums(&col_w);
    let row_y: Vec<i32> = prefix_sums(&row_h);
    for c in &table.cells {
        emit_cell(c, ox, oy, &col_x, &col_w, &row_y, &row_h, source, frame, ops);
    }
}

fn prefix_sums(values: &[i32]) -> Vec<i32> {
    let mut out = Vec::with_capacity(values.len() + 1);
    let mut acc = 0;
    out.push(0);
    for v in values {
        acc += v;
        out.push(acc);
    }
    out
}

/// Top y of a cell's content block for its vertical alignment. The block (height
/// `content_h`) sits inside the inner-margin area; CENTER splits the leftover
/// space, BOTTOM rests on the bottom margin, never above the top margin.
fn cell_content_top(
    valign: CellVAlign,
    y: i32,
    h: i32,
    pad_t: i32,
    pad_b: i32,
    content_h: i32,
) -> i32 {
    match valign {
        CellVAlign::Top => y + pad_t,
        CellVAlign::Center => y + pad_t + ((h - pad_t - pad_b - content_h) / 2).max(0),
        CellVAlign::Bottom => y + (h - pad_b - content_h).max(pad_t),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_cell(
    cell: &CellInfo,
    ox: i32,
    oy: i32,
    col_x: &[i32],
    col_w: &[i32],
    row_y: &[i32],
    row_h: &[i32],
    source: SourceRef,
    frame: &Frame,
    ops: &mut Vec<PaintOp>,
) {
    let col = cell.col as usize;
    let row = cell.row as usize;
    let cs = cell.col_span.max(1) as usize;
    let rs = cell.row_span.max(1) as usize;
    let x = ox + col_x.get(col).copied().unwrap_or(0);
    let y = oy + row_y.get(row).copied().unwrap_or(0);
    let w: i32 = col_w.iter().skip(col).take(cs).sum();
    let h: i32 = row_h.iter().skip(row).take(rs).sum();
    // Cell background fill behind the text.
    if let Some(c) = cell.border.fill {
        ops.push(PaintOp::FillRect { rect: Rect::new(x, y, w, h), color: Color::from_ref(c) });
    }
    // Cell text: every paragraph's line segments share one origin at the cell's
    // inner-margin corner. Their vertpos is cumulative across the cell's
    // paragraphs (not restarted per paragraph), so all paragraphs use the same
    // origin and add their stored vertpos directly.
    let content_x = x + cell.padding.left.raw();
    // Cell vertical alignment: place the content block (its bottom = max vertpos+lh)
    // TOP/CENTER/BOTTOM within the inner-margin area. CENTER dominates (1118/1205 cells).
    let content_h = cell
        .paragraphs
        .iter()
        .flat_map(|p| &p.stored_line_segs)
        .map(|sg| sg.vertical_pos.raw() + sg.line_height.raw())
        .max()
        .unwrap_or(0);
    let content_y = cell_content_top(
        cell.vertical_align,
        y,
        h,
        cell.padding.top.raw(),
        cell.padding.bottom.raw(),
        content_h,
    );
    for para in &cell.paragraphs {
        // Nested table anchored in this cell paragraph: sits at the paragraph's
        // stored line top (TOP_AND_BOTTOM wrap), like a body table at its anchor.
        let anchor_top = content_y
            + para
                .stored_line_segs
                .first()
                .map_or(0, |s| s.vertical_pos.raw());
        // Floating nested tables sit at their anchor; treat-as-char ones flow
        // inline via emit_paragraph_lines (same as body tables).
        for nt in &para.tables {
            if nt.anchor.is_some() {
                emit_table(nt, content_x, anchor_top, source, frame, ops);
            }
        }
        let flow_w = para_flow_width(para, frame);
        emit_objects(&para.objects, content_x, anchor_top, flow_w, true, source, frame, ops);
        if !para.stored_line_segs.is_empty() {
            let last = para.stored_line_segs.len() - 1;
            emit_paragraph_lines(para, content_x, content_y, 0, last, source, frame, ops);
        }
        emit_objects(&para.objects, content_x, anchor_top, flow_w, false, source, frame, ops);
    }
    // Cell borders on top of the content.
    let b = &cell.border;
    push_edge(ops, &b.top, x, y, x + w, y);
    push_edge(ops, &b.bottom, x, y + h, x + w, y + h);
    push_edge(ops, &b.left, x, y, x, y + h);
    push_edge(ops, &b.right, x + w, y, x + w, y + h);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cell height 1000, content 200, top/bottom margin 50.
    const Y: i32 = 1000;
    const H: i32 = 1000;
    const PT: i32 = 50;
    const PB: i32 = 50;
    const CH: i32 = 200;

    #[test]
    fn top_align_sits_at_top_margin() {
        assert_eq!(cell_content_top(CellVAlign::Top, Y, H, PT, PB, CH), Y + PT);
    }

    #[test]
    fn center_align_splits_leftover() {
        // Inner area 900, content 200 -> centered down by (900-200)/2 = 350.
        assert_eq!(cell_content_top(CellVAlign::Center, Y, H, PT, PB, CH), Y + PT + 350);
    }

    #[test]
    fn bottom_align_rests_on_bottom_margin() {
        // y + h - pad_b - content = 1000 + 1000 - 50 - 200 = 1750.
        assert_eq!(cell_content_top(CellVAlign::Bottom, Y, H, PT, PB, CH), 1750);
    }

    #[test]
    fn center_overflow_clamps_to_top() {
        // Content taller than the area clamps to top (no negative offset).
        assert_eq!(cell_content_top(CellVAlign::Center, Y, H, PT, PB, 2000), Y + PT);
    }
}
