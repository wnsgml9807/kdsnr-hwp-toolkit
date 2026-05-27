//! Pagination from stored layout geometry.
//!
//! Hancom's stored line segments carry each line's `vertpos` (its vertical
//! position within a column). The position climbs down a column and resets to
//! the top when content moves to the next column — and a page break is just a
//! column reset where no column remains. So the stored vertpos already encodes
//! every column and page break Hancom made.
//!
//! This paginator recovers that layout: it walks the measured blocks in
//! document order, treats each drop in a paragraph's stored line tops as a
//! column advance, and groups columns into pages (`cols_per_page` columns per
//! page, filled left to right). No height summing, no re-flow.
//!
//! Known gap: tables/objects whose content is not reflected in the surrounding
//! line tops (a tall table that fills columns on its own) are not yet advanced
//! across columns here; that needs table-content flow (tracked separately).

use kdsnr_hwp_core::{EngineResult, PageId, Rect, SourceRef};
use kdsnr_hwp_doc::{BreakBefore, PageApply};

use crate::{BlockKind, MeasuredBlock, MeasuredDocument, MeasuredFurniture, MeasuredSection};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaginationResult {
    pub pages: Vec<PaginatedPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginatedPage {
    pub page: PageId,
    pub section: kdsnr_hwp_core::SectionId,
    pub paper: Rect,
    pub body: Rect,
    pub header: Rect,
    pub footer: Rect,
    pub footnote: Rect,
    pub columns: Vec<PageColumn>,
    pub items: Vec<PaginatedItem>,
    /// Page furniture placed on this page (master behind, then header/footer),
    /// selected by page parity.
    pub furniture: Vec<PageFurniturePlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FurnitureRole {
    MasterPage,
    Header,
    Footer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFurniturePlacement {
    pub role: FurnitureRole,
    /// The furniture's content blocks placed at page coordinates.
    pub items: Vec<PaginatedItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageColumn {
    pub index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaginatedItem {
    pub source: SourceRef,
    pub kind: BlockKind,
    /// Column on its page (0-based) the item starts in.
    pub column: usize,
    /// Position on the page (HWPUNIT): column origin x, stored top y.
    pub rect: Rect,
    /// Half-open range of the block's fragments represented by this item.
    pub fragment_range: (usize, usize),
}

/// Place a block at column origin `x` and area top `base_y`: the y is the
/// stored top of its first line (column-relative), or the area top when the
/// block has no lines (a table).
fn place_item(block: &MeasuredBlock, x: i32, base_y: i32, column: usize) -> PaginatedItem {
    let y = match block.line_tops.first() {
        Some(top) => base_y + top.raw(),
        None => base_y,
    };
    PaginatedItem {
        source: block.source,
        kind: block.kind,
        column,
        rect: Rect::new(
            x + block.column_offset_x.raw(),
            y,
            block.bounds.width.raw(),
            block.bounds.height.raw(),
        ),
        fragment_range: (0, block.fragments.len().max(1)),
    }
}

fn furniture_applies(apply: PageApply, page_number: usize) -> bool {
    match apply {
        PageApply::Both => true,
        PageApply::Odd => page_number % 2 == 1,
        PageApply::Even => page_number % 2 == 0,
    }
}

/// Select the furniture definition for a page: a parity-specific (odd/even)
/// definition wins over a both-pages one.
fn pick_furniture(list: &[MeasuredFurniture], page_number: usize) -> Option<&MeasuredFurniture> {
    list.iter()
        .find(|f| {
            matches!(f.apply, PageApply::Odd | PageApply::Even)
                && furniture_applies(f.apply, page_number)
        })
        .or_else(|| list.iter().find(|f| f.apply == PageApply::Both))
}

/// Select the master page for a 1-based physical page. hwpx masters can be
/// page-specific (OPTIONAL_PAGE → exact page), parity (EVEN/ODD), both, or
/// last-page. Priority: exact target page > parity > both > last-page > the
/// nearest earlier OPTIONAL_PAGE carried forward (extension semantics, so an
/// optional-only section never leaves a page master-less).
fn pick_master(
    list: &[MeasuredFurniture],
    page_number: usize,
    last_page: usize,
) -> Option<&MeasuredFurniture> {
    list.iter()
        .find(|f| f.target_page == Some(page_number))
        .or_else(|| {
            list.iter().find(|f| {
                !f.is_extension
                    && matches!(f.apply, PageApply::Odd | PageApply::Even)
                    && furniture_applies(f.apply, page_number)
            })
        })
        .or_else(|| {
            list.iter()
                .find(|f| !f.is_extension && f.apply == PageApply::Both)
        })
        .or_else(|| {
            (page_number == last_page)
                .then(|| list.iter().find(|f| f.is_last_page))
                .flatten()
        })
        .or_else(|| {
            list.iter()
                .filter(|f| f.target_page.is_some_and(|t| t <= page_number))
                .max_by_key(|f| f.target_page.unwrap_or(0))
        })
}

fn place_furniture(
    furn: &MeasuredFurniture,
    role: FurnitureRole,
    base_y: i32,
) -> PageFurniturePlacement {
    let items = furn
        .blocks
        .iter()
        .filter(|b| !(b.bounds.height.raw() == 0 && b.line_tops.is_empty()))
        .map(|b| place_item(b, b.bounds.x.raw(), base_y, 0))
        .collect();
    PageFurniturePlacement { role, items }
}

/// Paginate a measured document into pages.
///
/// Two strategies share the same output and page-assembly code:
///   - default: `paginate_greedy` recomputes the column/page breaks by greedy
///     vertical fill of stored line heights against the column height — the RE'd
///     Hancom algorithm (`ComposeBreak` applied vertically). Byte-exact to the
///     stored layout on every validation original, and the only correct path for
///     generated/edited documents whose stored `vertpos` is absent or stale.
///   - `KDSNR_PAGINATE_HEURISTIC`: recover the breaks from stored line tops
///     (`paginate_heuristic`) instead — a re-render shortcut, lossy where stored
///     `vertpos` is ambiguous (it drops the HWPX break-flag bits). `KDSNR_PAGINATE_GREEDY`
///     is still honoured (and is now the default) for older call sites.
pub fn paginate_document(measured: &MeasuredDocument) -> EngineResult<PaginationResult> {
    let force_greedy = std::env::var_os("KDSNR_PAGINATE_GREEDY").is_some();
    if std::env::var_os("KDSNR_PAGINATE_HEURISTIC").is_some() && !force_greedy {
        paginate_heuristic(measured)
    } else {
        paginate_greedy(measured)
    }
}

/// Assign every block a starting column index by recovering Hancom's column and
/// page breaks from stored line tops, then group columns into pages.
fn paginate_heuristic(measured: &MeasuredDocument) -> EngineResult<PaginationResult> {
    let mut pages: Vec<PaginatedPage> = Vec::new();
    let mut next_page = 0usize;

    for section in &measured.sections {
        let cols_per_page = section.columns.len().max(1);

        // Column index at the start of each block. Within a column, stored line
        // tops strictly increase. A column boundary is therefore a line top that
        // does not advance past the previous one:
        //   - `top < prev_top`  — an ordinary column reset (also how Hancom
        //     records page and explicit breaks: a reset to the column top).
        //   - `top == prev_top` with a non-empty previous line *and* a column
        //     start that does not move right (`start <= prev_start`) —
        //     consecutive full-column inline tables/objects Hancom stores at the
        //     same `vertpos` (e.g. several full-height tables at `vertpos = 0`,
        //     each at the column's left edge). A same-`vertpos` line that moves
        //     *right* instead continues the same physical line (a justified or
        //     tab-split line in two segments) and is not a column boundary.
        let mut col_index = 0usize;
        let mut prev_top = i32::MIN;
        let mut prev_start = i32::MIN;
        let mut prev_advance = 0i32;
        let mut seen_line = false;
        // Column-relative top for a no-line block (table/object). A table is
        // anchored in a paragraph whose stored line height already spans the
        // table band (TOP_AND_BOTTOM wrap), so the table sits at that line's
        // top, not below it. Consecutive tables in one anchor stack from there.
        let mut col_y = 0i32;
        // Per block, the runs of consecutive lines that share a column. A block
        // whose lines span several columns (e.g. a paragraph of full-column
        // inline objects) yields one run per column, so each column's slice
        // lands on its own page instead of the whole block on the start page.
        // A run is `(column, first_line, last_line, y)`; for a block with no
        // stored lines (table/object) `first_line > last_line` marks the no-line
        // case and `y` is the column-relative top to place it at.
        let mut block_runs: Vec<Vec<(usize, usize, usize, i32)>> =
            Vec::with_capacity(section.blocks.len());

        for block in &section.blocks {
            // An explicit break classifies the boundary at this block's start:
            // a page/section break starts a fresh page (jump to its first
            // column), a column break starts the next column. Hancom records the
            // same move as a vertpos reset on the first line, so when an explicit
            // break is present we apply it here and skip the reset count once.
            let mut boundary_from_break = false;
            if seen_line {
                match block.break_before {
                    BreakBefore::Page | BreakBefore::Section => {
                        let page = col_index / cols_per_page;
                        col_index = (page + 1) * cols_per_page;
                        boundary_from_break = true;
                        col_y = 0;
                    }
                    BreakBefore::Column | BreakBefore::MultiColumn => {
                        col_index += 1;
                        boundary_from_break = true;
                        col_y = 0;
                    }
                    BreakBefore::None => {}
                }
            }

            let mut runs: Vec<(usize, usize, usize, i32)> = Vec::new();
            if block.line_tops.is_empty() {
                // Table/object block: placed at the running column cursor, then
                // advancing it by the block height so following content stacks.
                runs.push((col_index, 1, 0, col_y));
                col_y += block.bounds.height.raw();
            } else {
                let mut run_col = col_index;
                let mut run_start = 0usize;
                let mut first = true;
                for (i, (top, advance)) in
                    block.line_tops.iter().zip(block.fragments.iter()).enumerate()
                {
                    let t = top.raw();
                    let s = block.line_starts.get(i).map(|v| v.raw()).unwrap_or(0);
                    let reset = seen_line
                        && (t < prev_top || (t == prev_top && s <= prev_start && prev_advance > 0));
                    if reset && !(first && boundary_from_break) {
                        // Close the run only when it has lines (a reset on the
                        // block's first line just starts this block in a new
                        // column, with nothing to close).
                        if i > run_start {
                            runs.push((run_col, run_start, i - 1, 0));
                        }
                        col_index += 1;
                        run_col = col_index;
                        run_start = i;
                    }
                    prev_top = t;
                    prev_start = s;
                    prev_advance = advance.raw();
                    // Anchor cursor tracks the line top (the table band's top),
                    // not the bottom: a TOP_AND_BOTTOM table's height is already
                    // inside this line's stored height.
                    col_y = t;
                    seen_line = true;
                    first = false;
                }
                runs.push((run_col, run_start, block.line_tops.len() - 1, 0));
            }
            block_runs.push(runs);
        }

        // Endnotes are rendered after the body at the document/section end. They
        // continue in the body's last column from where its content ends and
        // spill into fresh columns. Their stored positions are endnote-local, so
        // they stack by measured height rather than by stored tops. The body's
        // fill in the last column is its final line's bottom (`vertpos` already
        // includes the heights of tables stacked above that line).
        let body_h = section.body_rect.height.raw().max(1);
        let body_bottom = if seen_line { prev_top + prev_advance } else { 0 };
        let mut endnote_place: Vec<(usize, i32)> = Vec::with_capacity(section.endnotes.len());
        {
            let mut en_col = col_index;
            let mut en_y = body_bottom;
            for block in &section.endnotes {
                let h = block.bounds.height.raw();
                if en_y > 0 && en_y + h > body_h {
                    en_col += 1;
                    en_y = 0;
                }
                endnote_place.push((en_col, en_y));
                en_y += h;
            }
        }
        let last_col = endnote_place
            .last()
            .map(|(c, _)| *c)
            .unwrap_or(col_index)
            .max(col_index);

        let total_cols = if section.blocks.is_empty() && section.endnotes.is_empty() {
            0
        } else {
            last_col + 1
        };
        let total_pages = total_cols.div_ceil(cols_per_page);
        let base = next_page;

        let column_x = |col_on_page: usize| {
            section
                .columns
                .get(col_on_page)
                .copied()
                .unwrap_or(section.body_rect)
                .x
                .raw()
        };

        let body_top = section.body_rect.y.raw();
        let mut page_items: Vec<Vec<PaginatedItem>> = vec![Vec::new(); total_pages];
        for (block, runs) in section.blocks.iter().zip(&block_runs) {
            // Skip hidden/empty content that occupies no height and no lines.
            if block.bounds.height.raw() == 0 && block.line_tops.is_empty() {
                continue;
            }
            for &(col, first_line, last_line, y) in runs {
                let page_in_section = col / cols_per_page;
                let col_on_page = col % cols_per_page;
                let x = column_x(col_on_page);
                // A no-line block (table/object) is placed whole at its column
                // cursor `y`; a line run spans its first..=last stored line tops.
                let item = if block.line_tops.is_empty() {
                    PaginatedItem {
                        source: block.source,
                        kind: block.kind,
                        column: col_on_page,
                        rect: Rect::new(
                            x + block.column_offset_x.raw(),
                            body_top + y,
                            block.bounds.width.raw(),
                            block.bounds.height.raw(),
                        ),
                        fragment_range: (0, block.fragments.len().max(1)),
                    }
                } else {
                    let y0 = block.line_tops[first_line].raw();
                    let y1 = block.line_tops[last_line].raw()
                        + block.fragments.get(last_line).map(|f| f.raw()).unwrap_or(0);
                    PaginatedItem {
                        source: block.source,
                        kind: block.kind,
                        column: col_on_page,
                        rect: Rect::new(x, body_top + y0, block.bounds.width.raw(), y1 - y0),
                        fragment_range: (first_line, last_line + 1),
                    }
                };
                if let Some(slot) = page_items.get_mut(page_in_section) {
                    slot.push(item);
                }
            }
        }
        // Place endnote blocks at their flowed column and stacked offset.
        for (block, &(col, y)) in section.endnotes.iter().zip(&endnote_place) {
            if block.bounds.height.raw() == 0 {
                continue;
            }
            let page_in_section = col / cols_per_page;
            let col_on_page = col % cols_per_page;
            let item = PaginatedItem {
                source: block.source,
                kind: block.kind,
                column: col_on_page,
                rect: Rect::new(
                    column_x(col_on_page),
                    section.body_rect.y.raw() + y,
                    block.bounds.width.raw(),
                    block.bounds.height.raw(),
                ),
                fragment_range: (0, block.fragments.len().max(1)),
            };
            if let Some(slot) = page_items.get_mut(page_in_section) {
                slot.push(item);
            }
        }

        let section_pages = assemble_section_pages(section, page_items, base, total_pages);
        pages.extend(section_pages);
        next_page = base + total_pages;
    }

    Ok(PaginationResult { pages })
}

/// A run of one block's content that lands in a single column: `[first_line,
/// last_line]` (inclusive) of its stored lines at column-relative top `y`. For a
/// no-line block (table/object) `first_line > last_line` marks the whole-block
/// case and `y` is its column-relative top.
#[derive(Clone, Copy)]
struct Run {
    col: usize,
    first_line: usize,
    last_line: usize,
    y: i32,
}

/// Recompute Hancom's column/page breaks by greedy vertical fill (the RE'd
/// algorithm: `Composition::Repair` runs `ComposeBreak` over line heights with
/// the column height as the composition width). Each line advances the column
/// cursor by its stored height; when the next line would overflow the column
/// height it starts the next column (left to right, wrapping to a new page every
/// `cols_per_page` columns). A paragraph straddling a column boundary yields one
/// run per column, so its border stays open across the break.
///
/// Unlike the heuristic, breaks come from the fill, not from stored `vertpos`
/// drops; the stored tops are only the validation ground truth (a recomputed
/// boundary should fall where stored `vertpos` resets).
/// The reserved top band of a column: the band a full-width banner spanning it
/// blocks off at the column top, or zero.
fn col_top(reserve: &std::collections::HashMap<usize, i32>, c: usize) -> i32 {
    reserve.get(&c).copied().unwrap_or(0)
}

/// Push the column cursor past a reserved top band when real content is placed.
/// A full-width banner reserves a band at the column top; real text is laid out
/// below it, while an empty paragraph (a blank marker line) sits within it at the
/// column top. So only `real` content that would start inside the band is bumped
/// to its bottom.
fn skip_top_band(
    accum: i32,
    col: usize,
    reserve: &std::collections::HashMap<usize, i32>,
    real: bool,
) -> i32 {
    let band = col_top(reserve, col);
    if real && accum < band {
        band
    } else {
        accum
    }
}

fn paginate_greedy(measured: &MeasuredDocument) -> EngineResult<PaginationResult> {
    let mut pages: Vec<PaginatedPage> = Vec::new();
    let mut next_page = 0usize;

    // The column-fill consumes, per paragraph, the full flow footprint Hancom
    // lays out: its text-line advances, the inter-paragraph spacing
    // (space_after(prev) + space_before(next), the paraShape margins), and a
    // Para-anchored TOP_AND_BOTTOM object's band above/below the text
    // (leading/trailing_band — the "b0 +8164" reserve). Verified to reproduce the
    // stored boundary gaps across the originals (see PAGINATION_RE_PLAN 1.1c).
    for section in &measured.sections {
        let cols_per_page = section.columns.len().max(1);
        let span = section.body_rect.height.raw().max(1);
        // Per-column top reserve: the height a full-width banner spanning that
        // column blocks off at its top. Seeded for the first page from the section
        // banner (an absolute one covers the first column too; a flowing one
        // reserves only the other columns, the first being handled by its own
        // block). Page banners found during the fill add reserves for later pages.
        let strip = section.top_banner_strip.raw();
        let mut col_reserve: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
        if strip > 0 {
            for c in 0..cols_per_page {
                if !(c == 0 && section.top_banner_flows) {
                    col_reserve.insert(c, strip);
                }
            }
        }

        // Global column index and the current column's filled height. A run's
        // `y` is the column cursor at its first line. The cursor starts at the
        // column top; real content is pushed past any reserved top band lazily
        // (`skip_top_band`), so an empty leading paragraph can sit within a banner.
        let mut col = 0usize;
        let mut accum = 0i32;
        let mut seen = false;
        let mut block_runs: Vec<Vec<Run>> = Vec::with_capacity(section.blocks.len());

        for block in &section.blocks {
            // An absolute (Paper/Page-anchored, or text-wrap) floating block is
            // positioned outside the flow: it does not stack or push text. But it
            // must still be placed so it paints — paint resolves its true position
            // from the anchor frame (`anchor_xy`), using this page assignment. Emit
            // it at the current column cursor without advancing the flow.
            if block.absolute {
                block_runs.push(vec![Run {
                    col,
                    first_line: 1,
                    last_line: 0,
                    y: accum,
                }]);
                continue;
            }

            // A forced break moves to the next column (or the next page's first
            // column) before any of this block's content is placed.
            let mut page_break_top = false;
            if seen {
                match block.break_before {
                    BreakBefore::Page | BreakBefore::Section => {
                        let page = col / cols_per_page;
                        col = (page + 1) * cols_per_page;
                        accum = 0;
                        page_break_top = true;
                    }
                    BreakBefore::Column | BreakBefore::MultiColumn => {
                        col += 1;
                        accum = 0;
                    }
                    BreakBefore::None => {}
                }
            }

            // A page banner: a full-width band block sitting at a page's first
            // column top spans the page's other columns, so reserve its band at
            // the top of each of them. (The first page is already seeded above.)
            if block.band_spans_columns
                && col % cols_per_page == 0
                && col >= cols_per_page
                && accum == 0
            {
                let band = block.leading_band.raw().max(block.trailing_band.raw());
                for c in (col + 1)..(col + cols_per_page) {
                    col_reserve.insert(c, band);
                }
            }

            // Space before this paragraph joins the running fill (the gap to the
            // previous block). Suppressed at a column top, where leading space is
            // discarded like glue at a break — except a forced Page/Section break,
            // where Hancom reserves space_before at the new page top.
            if accum > 0 {
                accum += block.space_before.raw();
            } else if page_break_top {
                accum += block.space_before.raw();
            }

            // Real content placed at a banner column top is pushed below the band.
            accum = skip_top_band(accum, col, &col_reserve, block.has_text);

            let mut runs: Vec<Run> = Vec::new();
            if block.line_tops.is_empty() {
                // Table/object: a whole, unsplittable box with its outer margins
                // (band reserve). Move it to the next column if box + margins would
                // overflow the current one (when the column already holds content).
                let h = block.bounds.height.raw();
                if accum > 0 && accum + h > span {
                    col += 1;
                    accum = skip_top_band(0, col, &col_reserve, true);
                }
                runs.push(Run { col, first_line: 1, last_line: 0, y: accum });
                accum += h + block.space_after.raw();
                seen = true;
            } else {
                // A full-width band above the first text line (a TOP_AND_BOTTOM
                // object at the paragraph top forcing text below it) joins the
                // fill before the lines and can itself overflow the column.
                let leading = block.leading_band.raw();
                if accum > 0 && accum + leading > span {
                    col += 1;
                    accum = skip_top_band(0, col, &col_reserve, block.has_text);
                }
                accum += leading;
                let mut run_start = 0usize;
                let mut run_y = accum;
                for (i, frag) in block.fragments.iter().enumerate() {
                    // A line stored as two segments at the same vertpos (a justified
                    // line split left/right) is one visual line: the second segment
                    // continues it horizontally and adds no vertical advance. It is
                    // told from a column reset (also same/lower vertpos) by its
                    // start moving right rather than resetting left.
                    let is_continuation = i > 0
                        && block.line_tops.get(i) == block.line_tops.get(i - 1)
                        && block.line_starts.get(i).map(|s| s.raw()).unwrap_or(0)
                            > block.line_starts.get(i - 1).map(|s| s.raw()).unwrap_or(0);
                    let advance = if is_continuation { 0 } else { frag.raw() };
                    // Overflow uses the line box (text_height): a line fits if its
                    // box clears the column bottom; the trailing line_spacing is
                    // glue that hangs past it. The cursor then advances by the full
                    // advance (box + spacing) to the next line. A single line
                    // taller than the column cannot be split, so only break when
                    // the column already holds content.
                    let box_h = if is_continuation {
                        0
                    } else {
                        block.line_boxes.get(i).map(|b| b.raw()).unwrap_or(advance)
                    };
                    if accum > 0 && accum + box_h > span {
                        if i > run_start {
                            runs.push(Run {
                                col,
                                first_line: run_start,
                                last_line: i - 1,
                                y: run_y,
                            });
                        }
                        col += 1;
                        accum = skip_top_band(0, col, &col_reserve, block.has_text);
                        run_start = i;
                        run_y = accum;
                    }
                    accum += advance;
                    seen = true;
                }
                runs.push(Run {
                    col,
                    first_line: run_start,
                    last_line: block.fragments.len() - 1,
                    y: run_y,
                });
                // The band's part below the last line, then inter-paragraph space.
                accum += block.trailing_band.raw() + block.space_after.raw();
            }
            block_runs.push(runs);
        }

        // Endnotes flow after the body, continuing in its last column from the
        // current fill and spilling into fresh columns by measured height.
        let mut endnote_place: Vec<(usize, i32)> = Vec::with_capacity(section.endnotes.len());
        {
            let mut en_col = col;
            let mut en_y = accum;
            for block in &section.endnotes {
                let h = block.bounds.height.raw();
                if en_y > 0 && en_y + h > span {
                    en_col += 1;
                    en_y = 0;
                }
                endnote_place.push((en_col, en_y));
                en_y += h;
            }
        }
        let last_col = endnote_place
            .last()
            .map(|(c, _)| *c)
            .unwrap_or(col)
            .max(col);

        let total_cols = if section.blocks.is_empty() && section.endnotes.is_empty() {
            0
        } else {
            last_col + 1
        };
        let total_pages = total_cols.div_ceil(cols_per_page);
        let base = next_page;

        let column_x = |col_on_page: usize| {
            section
                .columns
                .get(col_on_page)
                .copied()
                .unwrap_or(section.body_rect)
                .x
                .raw()
        };
        let body_top = section.body_rect.y.raw();
        let mut page_items: Vec<Vec<PaginatedItem>> = vec![Vec::new(); total_pages];

        for (block, runs) in section.blocks.iter().zip(&block_runs) {
            if block.bounds.height.raw() == 0 && block.line_tops.is_empty() {
                continue;
            }
            for run in runs {
                let page_in_section = run.col / cols_per_page;
                let col_on_page = run.col % cols_per_page;
                let x = column_x(col_on_page);
                let item = if block.line_tops.is_empty() {
                    PaginatedItem {
                        source: block.source,
                        kind: block.kind,
                        column: col_on_page,
                        rect: Rect::new(
                            x + block.column_offset_x.raw(),
                            body_top + run.y,
                            block.bounds.width.raw(),
                            block.bounds.height.raw(),
                        ),
                        fragment_range: (0, block.fragments.len().max(1)),
                    }
                } else {
                    // The run sits at the recomputed column cursor `y`; its height
                    // is the sum of its lines' stored advances. Paint anchors each
                    // line by the first line's stored vertpos delta, so only `y`
                    // (the run top) needs to be the recomputed fill position.
                    let h: i32 = block.fragments[run.first_line..=run.last_line]
                        .iter()
                        .map(|f| f.raw())
                        .sum();
                    PaginatedItem {
                        source: block.source,
                        kind: block.kind,
                        column: col_on_page,
                        rect: Rect::new(x, body_top + run.y, block.bounds.width.raw(), h),
                        fragment_range: (run.first_line, run.last_line + 1),
                    }
                };
                if let Some(slot) = page_items.get_mut(page_in_section) {
                    slot.push(item);
                }
            }
        }

        for (block, &(col, y)) in section.endnotes.iter().zip(&endnote_place) {
            if block.bounds.height.raw() == 0 {
                continue;
            }
            let page_in_section = col / cols_per_page;
            let col_on_page = col % cols_per_page;
            let item = PaginatedItem {
                source: block.source,
                kind: block.kind,
                column: col_on_page,
                rect: Rect::new(
                    column_x(col_on_page),
                    body_top + y,
                    block.bounds.width.raw(),
                    block.bounds.height.raw(),
                ),
                fragment_range: (0, block.fragments.len().max(1)),
            };
            if let Some(slot) = page_items.get_mut(page_in_section) {
                slot.push(item);
            }
        }

        let section_pages = assemble_section_pages(section, page_items, base, total_pages);
        pages.extend(section_pages);
        next_page = base + total_pages;
    }

    Ok(PaginationResult { pages })
}

/// Build the final pages for one section from its per-page item lists: attach
/// the column rectangles and select the master/header/footer furniture for each
/// page by parity. Shared by both paginators.
fn assemble_section_pages(
    section: &MeasuredSection,
    page_items: Vec<Vec<PaginatedItem>>,
    base: usize,
    total_pages: usize,
) -> Vec<PaginatedPage> {
    let columns: Vec<PageColumn> = section
        .columns
        .iter()
        .enumerate()
        .map(|(index, rect)| PageColumn { index, rect: *rect })
        .collect();

    let mut out = Vec::with_capacity(page_items.len());
    for (i, items) in page_items.into_iter().enumerate() {
        let page_id = base + i;
        // 1-based physical page number drives header/footer/master parity.
        let page_number = page_id + 1;
        let mut furniture = Vec::new();
        let master = pick_master(&section.master_pages, page_number, base + total_pages);
        if let Some(m) = master {
            furniture.push(place_furniture(
                m,
                FurnitureRole::MasterPage,
                section.body_rect.y.raw(),
            ));
        }
        // A dedicated first-page (cover) master owns that page's furniture, so the
        // running header/footer are not drawn over it. RE'd from the corpus: a
        // page-1 OPTIONAL_PAGE master carries the cover labels (e.g. social's
        // "사회·문화" + page number), and the section's ODD/EVEN header is suppressed
        // on page 1 — while pages with other page-specific masters keep it. The
        // `hideFirstHeader` flag is 0 here, so the cover master is the real signal.
        let cover_page = master.is_some_and(|m| m.target_page == Some(1));
        if !cover_page {
            if let Some(h) = pick_furniture(&section.headers, page_number) {
                furniture.push(place_furniture(h, FurnitureRole::Header, section.header_rect.y.raw()));
            }
            if let Some(f) = pick_furniture(&section.footers, page_number) {
                furniture.push(place_furniture(f, FurnitureRole::Footer, section.footer_rect.y.raw()));
            }
        }
        out.push(PaginatedPage {
            page: PageId(page_id),
            section: section.id,
            paper: section.page_rect,
            body: section.body_rect,
            header: section.header_rect,
            footer: section.footer_rect,
            footnote: section.footnote_rect,
            columns: columns.clone(),
            items,
            furniture,
        });
    }
    out
}

/// Serialize a pagination result as JSON for inspection. Dependency-free.
pub fn pagination_to_json(result: &PaginationResult) -> String {
    let mut out = String::from("{\"pages\":[");
    for (pi, page) in result.pages.iter().enumerate() {
        if pi > 0 {
            out.push(',');
        }
        out.push_str("{\"page\":");
        out.push_str(&page.page.0.to_string());
        out.push_str(",\"section\":");
        out.push_str(&page.section.0.to_string());
        out.push_str(",\"paper\":");
        push_rect(&mut out, page.paper);
        out.push_str(",\"body\":");
        push_rect(&mut out, page.body);
        out.push_str(",\"header\":");
        push_rect(&mut out, page.header);
        out.push_str(",\"footer\":");
        push_rect(&mut out, page.footer);
        out.push_str(",\"footnote\":");
        push_rect(&mut out, page.footnote);
        out.push_str(",\"columns\":[");
        for (ci, col) in page.columns.iter().enumerate() {
            if ci > 0 {
                out.push(',');
            }
            out.push_str("{\"index\":");
            out.push_str(&col.index.to_string());
            out.push_str(",\"rect\":");
            push_rect(&mut out, col.rect);
            out.push('}');
        }
        out.push_str("],\"items\":[");
        for (ii, item) in page.items.iter().enumerate() {
            if ii > 0 {
                out.push(',');
            }
            push_item(&mut out, item);
        }
        out.push_str("],\"furniture\":[");
        for (fi, f) in page.furniture.iter().enumerate() {
            if fi > 0 {
                out.push(',');
            }
            out.push_str("{\"role\":\"");
            out.push_str(match f.role {
                FurnitureRole::MasterPage => "master",
                FurnitureRole::Header => "header",
                FurnitureRole::Footer => "footer",
            });
            out.push_str("\",\"items\":[");
            for (ii, item) in f.items.iter().enumerate() {
                if ii > 0 {
                    out.push(',');
                }
                push_item(&mut out, item);
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn push_item(out: &mut String, item: &PaginatedItem) {
    out.push_str("{\"source\":\"");
    push_source(out, item.source);
    out.push_str("\",\"kind\":\"");
    out.push_str(match item.kind {
        BlockKind::Paragraph => "paragraph",
        BlockKind::Table => "table",
        BlockKind::Picture => "picture",
        BlockKind::Shape => "shape",
        BlockKind::Equation => "equation",
    });
    out.push_str("\",\"column\":");
    out.push_str(&item.column.to_string());
    out.push_str(",\"fragments\":[");
    out.push_str(&item.fragment_range.0.to_string());
    out.push(',');
    out.push_str(&item.fragment_range.1.to_string());
    out.push_str("],\"rect\":");
    push_rect(out, item.rect);
    out.push('}');
}

fn push_rect(out: &mut String, r: Rect) {
    out.push('{');
    out.push_str("\"x\":");
    out.push_str(&r.x.raw().to_string());
    out.push_str(",\"y\":");
    out.push_str(&r.y.raw().to_string());
    out.push_str(",\"w\":");
    out.push_str(&r.width.raw().to_string());
    out.push_str(",\"h\":");
    out.push_str(&r.height.raw().to_string());
    out.push('}');
}

fn push_source(out: &mut String, source: SourceRef) {
    match source {
        SourceRef::Section(s) => out.push_str(&format!("section:{}", s.0)),
        SourceRef::Page(p) => out.push_str(&format!("page:{}", p.0)),
        SourceRef::Paragraph(p) => out.push_str(&format!("paragraph:{}:{}", p.section.0, p.index)),
        SourceRef::Control(c) => out.push_str(&format!(
            "control:{}:{}:{}",
            c.paragraph.section.0, c.paragraph.index, c.index
        )),
        SourceRef::Style(s) => out.push_str(&format!("style:{}", s.0)),
    }
}
