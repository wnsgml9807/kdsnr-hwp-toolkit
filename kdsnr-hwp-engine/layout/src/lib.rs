//! Document measurement, pagination, and semantic layout tree.
//!
//! Measurement reads Hancom's **stored** layout geometry from the parsed
//! document — line segments (`vertsize`/`spacing`), cell box heights, object
//! extents — rather than recomputing it. A losslessly-converted hwpx always
//! carries this geometry, and reading it is byte-exact and font-independent.
//! From-scratch line composition lives in the preserved `text` crate (off this
//! path); see `docs/ENGINE_PURPOSE_AND_LAYERS.md`.

use kdsnr_hwp_core::{ControlId, EngineUnit, PageId, ParagraphId, Rect, SectionId, SourceRef};
use kdsnr_hwp_doc::{
    AnchorRel, BreakBefore, ColumnLayout, DocumentModel, HeaderFooter, MasterPageModel, PageApply,
    ParagraphModel, TableInfo,
};

mod paginate;

pub use paginate::{
    paginate_document, pagination_to_json, FurnitureRole, PageColumn, PageFurniturePlacement,
    PaginatedItem, PaginatedPage, PaginationResult,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageLayoutInfo {
    pub paper: Rect,
    pub body: Rect,
    pub header: Rect,
    pub footer: Rect,
    pub footnote: Rect,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeasuredDocument {
    pub sections: Vec<MeasuredSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredSection {
    pub id: SectionId,
    pub page_rect: Rect,
    pub body_rect: Rect,
    pub header_rect: Rect,
    pub footer_rect: Rect,
    pub footnote_rect: Rect,
    /// Body split into column areas (single entry when not multi-column).
    pub columns: Vec<Rect>,
    /// Vertical strip (HWPUNIT) a full-width TOP_AND_BOTTOM banner at the section
    /// top reserves at the top of every column on the first page. The banner spans
    /// all columns, so they start their content below it. Zero when there is none.
    pub top_banner_strip: EngineUnit,
    /// Whether that banner flows in document order (a Para-anchored object in the
    /// first paragraph): then the first column already reserves the strip through
    /// the block's own footprint and only the other first-page columns need it. A
    /// Paper/Page-anchored banner is absolute and flows nowhere, so every first-
    /// page column (including the first) starts below the strip.
    pub top_banner_flows: bool,
    pub blocks: Vec<MeasuredBlock>,
    /// Page furniture, measured from stored geometry; pagination places it per
    /// page by parity. Header/footer content goes in `header_rect`/`footer_rect`,
    /// master content behind the page.
    pub headers: Vec<MeasuredFurniture>,
    pub footers: Vec<MeasuredFurniture>,
    pub master_pages: Vec<MeasuredFurniture>,
    /// Endnote content blocks, measured from stored geometry in document order.
    /// Pagination flows these after the body (they carry no document-level
    /// vertical position, so they are stacked by height, not by stored tops).
    pub endnotes: Vec<MeasuredBlock>,
}

/// A measured page-furniture definition (header, footer, or master page).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredFurniture {
    pub apply: PageApply,
    pub is_extension: bool,
    pub overlap: bool,
    /// Master only: OPTIONAL_PAGE target page (1-based). None for header/footer.
    pub target_page: Option<usize>,
    /// Master only: LAST_PAGE master.
    pub is_last_page: bool,
    pub blocks: Vec<MeasuredBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredBlock {
    pub source: SourceRef,
    pub kind: BlockKind,
    /// Block box with measured height. Vertical position is the single-column
    /// stacking cursor; pagination assigns the final page position.
    pub bounds: Rect,
    /// Break forced before this block.
    pub break_before: BreakBefore,
    /// Splittable units stacked top to bottom (paragraph lines or table rows).
    /// Heights sum to `bounds.height`. Pagination splits only at these
    /// boundaries.
    pub fragments: Vec<EngineUnit>,
    /// Per paragraph line, the box height (`text_height`) that must fit in the
    /// column — the greedy fill's overflow test, vs `fragments` (the advance,
    /// `text_height + line_spacing`) which positions the next line. The trailing
    /// `line_spacing` is glue that hangs at a column bottom. Empty for non-
    /// paragraph blocks (they use `bounds.height`).
    pub line_boxes: Vec<EngineUnit>,
    /// Stored line tops (`vertpos`) per paragraph line, in document order; empty
    /// for non-paragraph blocks. A drop between consecutive tops marks a column
    /// boundary Hancom applied — pagination reads these to recover page/column
    /// breaks.
    pub line_tops: Vec<EngineUnit>,
    /// Stored line starts (`horzpos`, column-relative) per line, parallel to
    /// `line_tops`. Used to tell a column reset (a line at the same `vertpos`
    /// that starts a fresh column) from a same-`vertpos` segment that merely
    /// continues the line further right.
    pub line_starts: Vec<EngineUnit>,
    pub line_count: usize,
    /// Column-relative left offset (`horzpos`) applied at placement. Paragraph
    /// lines add their own `column_start` in paint, so this is zero for them; a
    /// treat-as-char table has no per-line starts, so its stored indent rides
    /// here and pagination adds it to the column origin.
    pub column_offset_x: EngineUnit,
    /// Paragraph space before/after (HWPUNIT): the inter-block gap the greedy
    /// paginator adds when filling a column. Zero for non-paragraph blocks.
    pub space_before: EngineUnit,
    pub space_after: EngineUnit,
    /// Extra flow height a Para-anchored TOP_AND_BOTTOM floating object adds to
    /// this paragraph beyond its text lines (HWPUNIT), for the greedy fill. The
    /// heuristic reads these effects from stored vertpos and ignores both.
    /// `leading_band` sits above the first text line (a full-width band at the
    /// paragraph top forces real text below it); `trailing_band` sits below the
    /// last text line (the band's part not overlapped by text pushes the next
    /// paragraph down). At most one is non-zero. Zero for non-paragraph blocks.
    pub leading_band: EngineUnit,
    pub trailing_band: EngineUnit,
    /// The band object is wider than one column, so it spans the page's other
    /// columns: when this block sits at a page's first-column top (a page banner),
    /// the greedy reserves the band at the top of that page's remaining columns.
    pub band_spans_columns: bool,
    /// Positioned absolutely (a Paper/Page-anchored floating table), so it does
    /// not participate in the column flow — the greedy fill neither stacks it nor
    /// lets it push text. Its vertical reserve, when it is a full-width top
    /// banner, is carried by the section's `top_banner_strip` instead.
    pub absolute: bool,
    /// The block carries real (non-empty) text or content. A full-width banner
    /// reserves a top band that real content is pushed below, while an empty
    /// paragraph (a blank marker line) sits within it at the column top. Always
    /// true for tables/objects.
    pub has_text: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Table,
    Picture,
    Shape,
    Equation,
}

/// Measured paragraph: total height, the vertical advance of each line, each
/// line's box height (`text_height`, the part that must fit in the column; the
/// trailing `line_spacing` is glue that hangs at a column bottom), and each
/// line's stored top (`vertpos`).
struct ParagraphMeasure {
    total: i32,
    lines: Vec<i32>,
    boxes: Vec<i32>,
    tops: Vec<i32>,
    starts: Vec<i32>,
}

/// Read a paragraph's height from its stored line segments. Each line advances
/// by `text_height + line_spacing` — the baseline-to-baseline flow advance,
/// which equals the stored `vertpos` delta between consecutive lines byte-for-
/// byte (verified across all originals). `line_height` is the line-box height
/// and can be far larger than the advance when a tall positioned object overlaps
/// the line without pushing the text flow down, so it is not the advance. The
/// advances sum to the paragraph's flowed height. The line tops (`vertpos`) are
/// kept for column/page break recovery. An empty paragraph measures to zero.
fn measure_paragraph(para: &ParagraphModel) -> ParagraphMeasure {
    let mut lines = Vec::with_capacity(para.stored_line_segs.len());
    let mut boxes = Vec::with_capacity(para.stored_line_segs.len());
    let mut tops = Vec::with_capacity(para.stored_line_segs.len());
    let mut starts = Vec::with_capacity(para.stored_line_segs.len());
    let mut total = 0i32;
    for seg in &para.stored_line_segs {
        let advance = (seg.text_height.raw() + seg.line_spacing.raw()).max(0);
        lines.push(advance);
        boxes.push(seg.text_height.raw().max(0));
        tops.push(seg.vertical_pos.raw());
        starts.push(seg.column_start.raw());
        total += advance;
    }
    ParagraphMeasure {
        total,
        lines,
        boxes,
        tops,
        starts,
    }
}

/// The extra flow height a Para-anchored TOP_AND_BOTTOM floating object adds to
/// a paragraph beyond its `text_flow` (sum of line advances), split into the
/// part above the first text line and the part below the last. The object's band
/// is `v_offset + margin.top + height + margin.bottom`. A full-width band at the
/// paragraph top (`v_offset == 0`) with real text forces that text below it
/// (leading band = the whole band; the box is band + text). Otherwise the text
/// sits at/around the top of the band and only the part not overlapped by text
/// extends past it (trailing band = `band - text_flow`), pushing the next
/// paragraph down. Square/Tight wraps reserve no band (text flows beside);
/// Paper/Page anchors are positioned in another frame and handled separately.
fn paragraph_object_bands(para: &ParagraphModel, text_flow: i32, column_width: i32) -> (i32, i32, bool) {
    let mut band = 0;
    let mut band_voff = 0;
    let mut spans = false;
    for o in &para.objects {
        // Inline (treat-as-char) objects are already inside their line's stored
        // text_height; only a floating Para-anchored TOP_AND_BOTTOM object adds a
        // band beyond the text lines.
        if o.reserves_vertical_band && !o.treat_as_char && o.anchor.vert_rel == AnchorRel::Para {
            let b = o.anchor.v_offset.raw() + o.margin.top.raw() + o.height.raw() + o.margin.bottom.raw();
            if b > band {
                band = b;
                band_voff = o.anchor.v_offset.raw();
                // Wider than its column means the band reaches the page's other
                // columns (a page-spanning banner).
                spans = o.width.raw() > column_width;
            }
        }
    }
    if band == 0 {
        return (0, 0, false);
    }
    let has_real_text = para.stored_line_segs.iter().any(|s| s.segment_width.raw() > 0);
    let (leading, trailing) = if band_voff == 0 && has_real_text {
        (band, 0) // leading: text pushed below the band
    } else {
        (0, (band - text_flow).max(0)) // trailing: band extends past the text
    };
    (leading, trailing, spans)
}

/// Full vertical extent of a paragraph's content: its text lines plus any
/// nested tables anchored in it. Used for table cells whose box height is not
/// stored; only the total matters. (Inline objects are already in the stored
/// line heights, so they are not added here.)
fn paragraph_content_height(para: &ParagraphModel) -> i32 {
    let mut h = measure_paragraph(para).total;
    for table in &para.tables {
        h += measure_table(table).total;
    }
    h
}

/// Measured table: total height plus each row height.
struct TableMeasure {
    total: i32,
    rows: Vec<i32>,
}

/// A table's height from stored geometry. The table box height (`<hp:sz>`) is
/// Hancom's laid-out footprint and is the authority for the total (validated
/// against GT: a 9700-HWPUNIT box renders 96.8pt tall). Per-row heights come
/// from the stored `cellSz` of the tallest non-spanning cell in the row; any
/// remainder between the stored total and the stored rows is absorbed by the
/// rows that leave their `cellSz` at zero (auto). When no total is stored, the
/// height is derived from cell content as a last resort.
fn measure_table(table: &TableInfo) -> TableMeasure {
    if let Some(rows) = table.stored_row_heights() {
        return TableMeasure {
            total: rows.iter().sum(),
            rows,
        };
    }
    // No stored box height: fall back to content plus the cells' padding.
    let mut rows = vec![0i32; table.rows as usize];
    for cell in &table.cells {
        if cell.row_span.max(1) != 1 {
            continue;
        }
        let mut content_h = 0i32;
        for para in &cell.paragraphs {
            content_h += paragraph_content_height(para);
        }
        content_h += cell.padding.top.raw() + cell.padding.bottom.raw();
        if let Some(slot) = rows.get_mut(cell.row as usize) {
            *slot = (*slot).max(content_h);
        }
    }
    TableMeasure {
        total: rows.iter().sum(),
        rows,
    }
}

/// Compute column areas from the body rectangle and the section column layout.
fn column_areas(body: Rect, columns: &ColumnLayout) -> Vec<Rect> {
    let count = columns.count.max(1) as usize;
    if count == 1 {
        return vec![body];
    }

    let bx = body.x.raw();
    let by = body.y.raw();
    let bw = body.width.raw();
    let bh = body.height.raw();

    let gaps: Vec<i32> = (0..count.saturating_sub(1))
        .map(|i| {
            columns
                .gaps
                .get(i)
                .copied()
                .filter(|g| *g > 0)
                .unwrap_or(columns.gap.max(0))
        })
        .collect();
    let total_gap: i32 = gaps.iter().sum();

    let widths: Vec<i32> = if columns.same_width || columns.widths.len() < count {
        let avail = (bw - total_gap).max(0);
        let w = avail / count as i32;
        vec![w; count]
    } else if columns.proportional {
        let avail = (bw - total_gap).max(0);
        let weight_sum: i32 = columns.widths.iter().take(count).map(|v| (*v).max(0)).sum::<i32>().max(1);
        columns
            .widths
            .iter()
            .take(count)
            .map(|v| avail * (*v).max(0) / weight_sum)
            .collect()
    } else {
        columns.widths.iter().take(count).map(|v| (*v).max(0)).collect()
    };

    let mut areas = Vec::with_capacity(count);
    let mut x = bx;
    for i in 0..count {
        let width = if i + 1 == count {
            (bx + bw - x).max(0)
        } else {
            widths.get(i).copied().unwrap_or(0)
        };
        areas.push(Rect::new(x, by, width, bh));
        x += width + gaps.get(i).copied().unwrap_or(0);
    }
    if columns.right_to_left {
        areas.reverse();
    }
    areas
}

/// Stack a paragraph list into measured blocks from stored geometry, laid out
/// at `origin` with the given block width. Paragraph heights come from stored
/// line segments (with their stored tops), tables from stored cell heights.
/// Objects add no separate stacked height: an inline object's height is already
/// in its line's stored `vertsize`, and a floating object is positioned
/// absolutely (its push on surrounding text is already in the stored vertpos).
fn measure_blocks(paras: &[ParagraphModel], origin_x: i32, origin_y: i32, width: i32) -> Vec<MeasuredBlock> {
    let mut cursor_y = origin_y;
    let mut blocks = Vec::with_capacity(paras.len());

    for para in paras {
        let id: ParagraphId = para.id;
        let pm = measure_paragraph(para);
        let line_count = para.stored_line_segs.len().max(1);
        // Real content that a banner band pushes below it: any text, inline
        // object, or table. An empty paragraph (a blank marker line) has none and
        // sits within the band at the column top. (An empty line still carries a
        // nonzero stored segment width, so width is not the emptiness signal.)
        let has_text = !para.text.trim().is_empty()
            || !para.objects.is_empty()
            || !para.tables.is_empty();
        let (leading_band, trailing_band, band_spans_columns) =
            paragraph_object_bands(para, pm.total, width);

        blocks.push(MeasuredBlock {
            source: SourceRef::Paragraph(id),
            kind: BlockKind::Paragraph,
            bounds: Rect::new(origin_x, cursor_y, width, pm.total),
            break_before: para.break_before,
            fragments: pm.lines.into_iter().map(EngineUnit::new).collect(),
            line_boxes: pm.boxes.into_iter().map(EngineUnit::new).collect(),
            line_tops: pm.tops.into_iter().map(EngineUnit::new).collect(),
            line_starts: pm.starts.into_iter().map(EngineUnit::new).collect(),
            line_count,
            column_offset_x: EngineUnit::new(0),
            space_before: para.space_before,
            space_after: para.space_after,
            leading_band: EngineUnit::new(leading_band),
            trailing_band: EngineUnit::new(trailing_band),
            band_spans_columns,
            absolute: false,
            has_text,
        });
        cursor_y += leading_band + pm.total + trailing_band;

        for (table_ordinal, table) in para.tables.iter().enumerate() {
            // A treat-as-char table's vertical space is already in the paragraph's
            // stored line segments (its marker line carries the table height), and
            // it is painted inline. Only a floating table reserves a block here.
            if table.anchor.is_none() {
                continue;
            }
            // A floating table consumes column flow height only when it reserves
            // a full-width band (TOP_AND_BOTTOM) and is anchored to the flow
            // (Para/Column). A Paper/Page anchor is absolute; a Square/Tight/
            // Through wrap lets text flow beside it (no net column height). Such a
            // table does not advance the column cursor.
            let frame_absolute = matches!(
                table.anchor.map(|a| a.vert_rel),
                Some(AnchorRel::Paper | AnchorRel::Page)
            );
            let flows_in_col = table.reserves_vertical_band && !frame_absolute;
            // A Para/Column-anchored table that reserves no band (Square/Tight/
            // Through wrap) hangs from its anchor paragraph like a floating
            // picture/shape. It is painted with the paragraph (lower_item), riding
            // its flowed top — not placed as its own block at the post-paragraph
            // cursor, which would drop it below the text it brackets.
            if !frame_absolute && !flows_in_col {
                continue;
            }
            let absolute = !flows_in_col;
            // A flowing band table reserves its outer margins above and below.
            let (tbl_sb, tbl_sa) = if flows_in_col {
                (table.margin.top.raw(), table.margin.bottom.raw())
            } else {
                (0, 0)
            };
            let tm = measure_table(table);
            let t_w = if table.width.raw() > 0 {
                table.width.raw()
            } else {
                width
            };
            // A treat-as-char table sits on its own line; that line's stored
            // segment carries the table's column-relative left (`horzpos`) and a
            // height equal to the table box, so it is told apart from text lines
            // by the closest `line_height`. Without this the table collapses to
            // the column left and a narrow box loses its stored indent.
            let table_x = para
                .stored_line_segs
                .iter()
                .min_by_key(|s| (s.line_height.raw() - tm.total).abs())
                .map(|s| s.column_start.raw())
                .unwrap_or(0);
            blocks.push(MeasuredBlock {
                // A paragraph can anchor several tables; the ordinal makes the
                // source resolve to the specific table for painting.
                source: SourceRef::Control(ControlId {
                    paragraph: id,
                    index: table_ordinal,
                }),
                kind: BlockKind::Table,
                bounds: Rect::new(origin_x, cursor_y, t_w, tm.total),
                break_before: BreakBefore::None,
                fragments: tm.rows.into_iter().map(EngineUnit::new).collect(),
                line_boxes: Vec::new(),
                line_tops: Vec::new(),
                line_starts: Vec::new(),
                line_count: 0,
                column_offset_x: EngineUnit::new(table_x),
                space_before: EngineUnit::new(tbl_sb),
                space_after: EngineUnit::new(tbl_sa),
                leading_band: EngineUnit::new(0),
                trailing_band: EngineUnit::new(0),
                band_spans_columns: false,
                absolute,
                has_text: true,
            });
            // A non-flowing table (absolute, or text-beside wrap) does not advance
            // the stacking cursor; a flowing band table consumes its box and outer
            // margins.
            if !absolute {
                cursor_y += tbl_sb + tm.total + tbl_sa;
            }
        }
    }

    blocks
}

/// Measure a header/footer definition into furniture at the given area.
fn measure_header_footer(hf: &HeaderFooter, area: Rect) -> MeasuredFurniture {
    MeasuredFurniture {
        apply: hf.apply,
        is_extension: false,
        overlap: false,
        target_page: None,
        is_last_page: false,
        blocks: measure_blocks(&hf.paragraphs, area.x.raw(), area.y.raw(), area.width.raw()),
    }
}

/// Measure a master page definition into furniture spanning the body area.
fn measure_master(master: &MasterPageModel, area: Rect) -> MeasuredFurniture {
    MeasuredFurniture {
        apply: master.apply,
        is_extension: master.is_extension,
        overlap: master.overlap,
        target_page: master.target_page,
        is_last_page: master.is_last_page,
        blocks: measure_blocks(&master.paragraphs, area.x.raw(), area.y.raw(), area.width.raw()),
    }
}

/// Measure every body block and page furniture in the document from stored
/// geometry. Page geometry and column areas are carried for pagination. No font
/// measurement is performed.
pub fn measure_document(document: &DocumentModel) -> MeasuredDocument {
    let mut sections = Vec::with_capacity(document.sections.len());

    for section in &document.sections {
        let body = section.body_rect;
        let columns = column_areas(body, &section.columns);
        // Paragraph block width follows the first column (uniform columns share
        // a width; the dominant case for body text).
        let measure_w = columns.first().copied().unwrap_or(body).width;
        let blocks = measure_blocks(&section.paragraphs, body.x.raw(), body.y.raw(), measure_w.raw());

        // A full-width TOP_AND_BOTTOM banner at the section top (a title bar)
        // spans all columns, so every column on the first page starts its content
        // below it. Two forms: a Para-anchored object in the first paragraph
        // (flows in the first column — its band is already in block 0's footprint),
        // or a Paper/Page-anchored table (absolute — flows nowhere, so the first
        // column needs the reserve too). Only meaningful when multi-column.
        let body_w = body.width.raw();
        let is_full_width = |w: i32| w * 10 >= body_w * 9;
        // Band of a full-width Para-anchored TOP_AND_BOTTOM object in the first
        // paragraph (the part of the page top it reserves across all columns —
        // just the object band, not the text that flows below it in the first
        // column).
        let para_band = section
            .paragraphs
            .first()
            .and_then(|p| {
                p.objects
                    .iter()
                    .filter(|o| {
                        o.reserves_vertical_band
                            && !o.treat_as_char
                            && o.anchor.vert_rel == AnchorRel::Para
                            && o.anchor.v_offset.raw() == 0
                            && is_full_width(o.width.raw())
                    })
                    .map(|o| o.anchor.v_offset.raw() + o.margin.top.raw() + o.height.raw() + o.margin.bottom.raw())
                    .max()
            });
        let (top_banner_strip, top_banner_flows) = if columns.len() <= 1 {
            (0, false)
        } else if let Some(band) = para_band {
            (band, true)
        } else {
            // Paper/Page-anchored full-width banner: its body-relative band bottom
            // (`v_offset + margin.top + height + margin.bottom − body_top`) is the
            // top all columns clear to. The banner may be a table or a shape/pic.
            let body_y = body.y.raw();
            let table_strip = section
                .paragraphs
                .iter()
                .flat_map(|p| &p.tables)
                .filter(|t| {
                    t.reserves_vertical_band
                        && matches!(
                            t.anchor.map(|a| a.vert_rel),
                            Some(AnchorRel::Paper | AnchorRel::Page)
                        )
                        && is_full_width(t.width.raw())
                })
                .map(|t| {
                    let voff = t.anchor.map(|a| a.v_offset.raw()).unwrap_or(0);
                    voff + t.margin.top.raw() + t.height.raw() + t.margin.bottom.raw() - body_y
                });
            let object_strip = section
                .paragraphs
                .iter()
                .flat_map(|p| &p.objects)
                .filter(|o| {
                    o.reserves_vertical_band
                        && !o.treat_as_char
                        && matches!(o.anchor.vert_rel, AnchorRel::Paper | AnchorRel::Page)
                        && is_full_width(o.width.raw())
                })
                .map(|o| {
                    o.anchor.v_offset.raw() + o.margin.top.raw() + o.height.raw()
                        + o.margin.bottom.raw()
                        - body_y
                });
            let strip = table_strip
                .chain(object_strip)
                .filter(|s| *s > 0)
                .max()
                .unwrap_or(0);
            (strip, false)
        };

        let headers = section
            .headers
            .iter()
            .map(|h| measure_header_footer(h, section.header_rect))
            .collect();
        let footers = section
            .footers
            .iter()
            .map(|f| measure_header_footer(f, section.footer_rect))
            .collect();
        let master_pages = section
            .master_pages
            .iter()
            .map(|m| measure_master(m, body))
            .collect();
        // Endnotes flow in the body's column width after the body content.
        let endnotes = measure_blocks(&section.endnotes, body.x.raw(), body.y.raw(), measure_w.raw());

        sections.push(MeasuredSection {
            id: section.id,
            page_rect: section.page_rect,
            body_rect: body,
            header_rect: section.header_rect,
            footer_rect: section.footer_rect,
            footnote_rect: section.footnote_rect,
            columns,
            top_banner_strip: EngineUnit::new(top_banner_strip),
            top_banner_flows,
            blocks,
            headers,
            footers,
            master_pages,
            endnotes,
        });
    }

    MeasuredDocument { sections }
}

// --- M5+ stubs ---

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageLayoutTree {
    pub page: Option<PageId>,
    pub layout: PageLayoutInfo,
    pub nodes: Vec<LayoutNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutNode {
    pub source: Option<SourceRef>,
    pub kind: LayoutNodeKind,
    pub rect: Rect,
    pub children: Vec<LayoutNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutNodeKind {
    Page,
    Background,
    Master,
    Header,
    Body,
    Footer,
    Footnote,
    Column,
    Paragraph,
    TextLine,
    TextRun,
    Table,
    TableCell,
    Picture,
    Shape,
    Equation,
    Group,
}
