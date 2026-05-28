//! Normalized document model.
//!
//! [`normalize`] turns parser output into a model the layout stages consume:
//! sections with page/body rectangles, and paragraphs whose characters carry a
//! resolved font (size, face, style) and whose paragraph context carries line
//! spacing. Stored Hancom line segments are preserved for verification.

use kdsnr_hwp_core::{EngineUnit, Insets, ParagraphId, Rect, SectionId};

use kdsnr_hwp_parser::model::control::Control;
use kdsnr_hwp_parser::model::document::{Document, Section};
use kdsnr_hwp_parser::model::header_footer::{HeaderFooterApply, MasterPage};
use kdsnr_hwp_parser::model::page::{ColumnDef, PageAreas};
use kdsnr_hwp_parser::model::paragraph::{ColumnBreakType, LineSeg as ParserLineSeg, Paragraph};
use kdsnr_hwp_parser::model::shape::{CommonObjAttr, TextWrap, VertRelTo};
use kdsnr_hwp_parser::model::style::LineSpacingType;
use kdsnr_hwp_parser::model::table::{Table, VerticalAlign};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentModel {
    pub sections: Vec<SectionModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionModel {
    pub id: SectionId,
    pub page_rect: Rect,
    /// Page area inside the margins (the `PAGE` anchor reference frame).
    pub page_area: Rect,
    pub body_rect: Rect,
    pub header_rect: Rect,
    pub footer_rect: Rect,
    pub footnote_rect: Rect,
    /// Column layout applied to the body area.
    pub columns: ColumnLayout,
    pub paragraphs: Vec<ParagraphModel>,
    /// Running header definitions for this section (applied per page by parity).
    pub headers: Vec<HeaderFooter>,
    /// Running footer definitions (page numbers, etc.).
    pub footers: Vec<HeaderFooter>,
    /// Background (master) page templates layered behind the body.
    pub master_pages: Vec<MasterPageModel>,
    /// Endnote content, in document order, flattened across all endnotes
    /// (each endnote's paragraphs in turn). Endnotes are placed after the body
    /// at the document/section end; their stored line segments are endnote-local
    /// (each endnote's `vertpos` restarts), so pagination flows them by height.
    pub endnotes: Vec<ParagraphModel>,
}

/// Which pages a piece of page furniture applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageApply {
    #[default]
    Both,
    Even,
    Odd,
}

/// A header or footer definition: its page-parity scope and its own content.
/// Paragraphs carry their stored line segments, like the body.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderFooter {
    pub apply: PageApply,
    pub paragraphs: Vec<ParagraphModel>,
}

/// A master (background) page template for the section.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterPageModel {
    pub apply: PageApply,
    /// Extension master page (applies from a later page onward).
    pub is_extension: bool,
    /// Drawn on top of an existing master rather than replacing it.
    pub overlap: bool,
    /// OPTIONAL_PAGE target page (1-based; ext_flags-3 when ext_flags>=4). None otherwise.
    pub target_page: Option<usize>,
    /// LAST_PAGE master (ext_flags==3): applies only to the last page.
    pub is_last_page: bool,
    pub paragraphs: Vec<ParagraphModel>,
}

/// Normalized column definition for the section body. Defaults to a single
/// column spanning the body.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnLayout {
    pub count: u16,
    pub same_width: bool,
    /// Uniform gap between columns (HWPUNIT), used when per-column gaps are absent.
    pub gap: i32,
    /// Per-column widths (HWPUNIT for absolute, weights for proportional).
    pub widths: Vec<i32>,
    /// Per-column trailing gaps (HWPUNIT).
    pub gaps: Vec<i32>,
    /// `widths` carry proportional weights rather than absolute HWPUNIT.
    pub proportional: bool,
    pub right_to_left: bool,
}

impl Default for ColumnLayout {
    fn default() -> Self {
        ColumnLayout {
            count: 1,
            same_width: true,
            gap: 0,
            widths: Vec::new(),
            gaps: Vec::new(),
            proportional: false,
            right_to_left: false,
        }
    }
}

/// Break applied before a paragraph. Mirrors the parser `ColumnBreakType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakBefore {
    #[default]
    None,
    Section,
    MultiColumn,
    Page,
    Column,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParagraphModel {
    pub id: ParagraphId,
    pub text: String,
    /// One entry per UTF-16 code unit of `text`.
    pub chars: Vec<ResolvedChar>,
    /// Gapped UTF-16 position of each visible char: an inline object marker that
    /// is excluded from `text` still advances Hancom's stored offsets by 8, so a
    /// line segment's `text_start` (gapped) maps to a visible index through this.
    /// Empty when the paragraph has no inline objects (gapped == visible).
    pub char_offsets: Vec<u32>,
    pub spacing: LineSpacing,
    /// Break forced before this paragraph (page/column/section).
    pub break_before: BreakBefore,
    /// Tables anchored in this paragraph, with their stored grid geometry.
    pub tables: Vec<TableInfo>,
    /// Pictures, shapes, and equations anchored in this paragraph.
    pub objects: Vec<ObjectInfo>,
    /// Stored Hancom line segments, for the verification gate.
    pub stored_line_segs: Vec<StoredLineSeg>,
    /// Paragraph-shape first-line indent (`<hc:intent>`, HWPUNIT). Signed:
    /// negative outdents the first line (hanging indent). Stored `column_start`
    /// already carries the left margin; paint adds this to the first line only.
    pub first_line_indent: EngineUnit,
    /// Paragraph space before/after (paraPr margin `prev`/`next`, HWPUNIT). The
    /// inter-paragraph gap pagination adds between blocks when filling a column.
    pub space_before: EngineUnit,
    pub space_after: EngineUnit,
    /// Horizontal alignment for placing text within the line segment.
    pub align: Align,
    /// Paragraph border/background (paraPr `<border>`). Consecutive paragraphs
    /// sharing `border_fill_id` form one connected box when `border_connect`.
    pub border: BorderFillInfo,
    pub border_fill_id: u16,
    /// `<hh:border connect>` (paraShape attr1 bit 28): connect this border with
    /// adjacent same-`border_fill_id` paragraphs into one region. When false, the
    /// paragraph draws its own closed box.
    pub border_connect: bool,
    /// Border offsets (text→border gap): left/right/top/bottom, HWPUNIT.
    pub border_offsets: Insets,
    /// Generated heading number/bullet prefix (e.g. "1.", "가."), rendered at the
    /// paragraph's first line. None when the paragraph has no active heading.
    pub auto_number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Justify,
    Left,
    Right,
    Center,
    Distribute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Picture,
    Shape,
    Equation,
}

/// Renderable content of an object, resolved from its control. Geometry-only;
/// the object box (size/position) lives on [`ObjectInfo`].
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectContent {
    /// Embedded raster; `ext` is the source extension (png/jpg/bmp/gif), a mime hint.
    Image { data: std::rc::Rc<Vec<u8>>, ext: String },
    /// Filled/stroked rectangle spanning the object box. `border` is None for a
    /// `style="NONE"` outline (an invisible guide rect Hancom does not draw).
    Rect { fill: Option<u32>, border_color: u32, border_width: i32, border: BorderStyle },
    /// Straight line; endpoints are offsets within the object box (HWPUNIT).
    /// `style` is None for a `style="NONE"` (invisible) line.
    Line { x1: i32, y1: i32, x2: i32, y2: i32, color: u32, width: i32, style: BorderStyle },
    /// Equation: the Hancom script plus its font/size/color. Lowered to glyph
    /// primitives at paint time; glyphs resolve via the font crate (HANCOM_FONT_DIR).
    Equation { script: String, font: String, color: u32, font_size: u32 },
    /// Not yet renderable (drawText, group, ole, …).
    None,
}

/// Text-box content of a drawing object (drawText): paragraphs laid out inside
/// the object box, with the box's inner margins and vertical alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTextBox {
    pub paragraphs: Vec<ParagraphModel>,
    pub vertical_align: CellVAlign,
    pub margin: Insets,
}

/// Anchor reference frame for a floating object's position (`<hp:pos>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorRel {
    /// Physical paper edge.
    Paper,
    /// Page area inside the margins.
    Page,
    /// Text column.
    Column,
    /// The anchoring paragraph.
    Para,
}

/// Alignment of a floating object within its reference frame, per axis. The
/// offset adds to the aligned position (`None` = offset is the raw coordinate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorAlign {
    None,
    Left,
    Center,
    Right,
    Top,
    Bottom,
}

/// A floating object's placement: reference frame, alignment, and offset per
/// axis (`<hp:pos>`). Paper/Page anchors are absolute; Column/Para ride the
/// flow. Absent for treat-as-char (inline) objects, which follow the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub vert_rel: AnchorRel,
    pub horz_rel: AnchorRel,
    pub vert_align: AnchorAlign,
    pub horz_align: AnchorAlign,
    pub v_offset: EngineUnit,
    pub h_offset: EngineUnit,
}

/// A picture, shape, or equation anchored in a paragraph, with its stored size
/// and flow disposition.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectInfo {
    pub kind: ObjectKind,
    /// Index into the paragraph's controls, for the source reference.
    pub control_index: usize,
    pub width: EngineUnit,
    pub height: EngineUnit,
    pub margin: Insets,
    /// Offset of the object box from its anchor reference (HWPUNIT).
    pub h_offset: EngineUnit,
    pub v_offset: EngineUnit,
    /// Placement anchor (`<hp:pos>`): reference frame + alignment per axis.
    pub anchor: Anchor,
    /// Resolved renderable content.
    pub content: ObjectContent,
    /// drawText content: paragraphs drawn inside the shape box (header/footer/
    /// master labels). None when the shape has no text box.
    pub text_box: Option<ObjectTextBox>,
    /// BEHIND_TEXT wrap: draw beneath the body text. Others draw above it.
    pub behind_text: bool,
    /// Inline object that occupies a line like a character.
    pub treat_as_char: bool,
    /// For a treat-as-char object: its visual character index in the paragraph
    /// (count of visible chars before it in run order), so paint/render place it
    /// at its in-line position instead of the paragraph origin. None otherwise.
    pub inline_pos: Option<u32>,
    /// Object reserves vertical space in the body flow (inline, or floating
    /// anchored to the paragraph with a non-overlap wrap). Page/paper-anchored
    /// objects are positioned absolutely and do not.
    pub in_flow: bool,
    /// TOP_AND_BOTTOM wrap: the object reserves a full-width vertical band that
    /// text cannot sit beside, so it pushes the flow down by its band height
    /// (`v_offset + margin.top + height + margin.bottom`). Square/Tight/Through
    /// wraps let text flow beside (no vertical push); Behind/InFront overlap.
    pub reserves_vertical_band: bool,
}

/// A table's grid. The table box height (`<hp:sz>`) is Hancom's laid-out
/// footprint and is always stored, even when individual cells leave their
/// `cellSz` height at zero (auto). Measurement trusts this total and the stored
/// per-cell heights, deriving from content only as a last resort.
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    pub rows: u16,
    pub cols: u16,
    pub width: EngineUnit,
    /// Stored table box height (`<hp:sz height>`, HWPUNIT). Zero only when the
    /// format omits it.
    pub height: EngineUnit,
    pub cells: Vec<CellInfo>,
    /// Floating placement anchor (`<hp:pos>`), or None for a treat-as-char table
    /// that flows inline on its own line.
    pub anchor: Option<Anchor>,
    /// For a treat-as-char table: its run-order visual char index in the
    /// paragraph, so paint places it in the text line. None for a floating table.
    pub inline_pos: Option<u32>,
    /// Outer margin (`<hp:outMargin>`, HWPUNIT): the gap reserved around the table
    /// box in the flow, part of a floating table's vertical band.
    pub margin: Insets,
    /// TOP_AND_BOTTOM wrap: the table reserves a full-width vertical band (band =
    /// `anchor.v_offset + margin.top + height + margin.bottom`). See the object
    /// flag of the same name.
    pub reserves_vertical_band: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellInfo {
    pub row: u16,
    pub col: u16,
    pub row_span: u16,
    pub col_span: u16,
    pub width: EngineUnit,
    /// Stored cell box height (HWPUNIT). Zero when the format omits it; then
    /// measurement derives the height from cell content.
    pub height: EngineUnit,
    /// Effective inner margin: the cell's own when it sets one, else the table's
    /// default. Cell line segments are stored relative to this inset, so it is
    /// the cell content origin.
    pub padding: Insets,
    /// Vertical placement of the cell's content block within the cell box.
    pub vertical_align: CellVAlign,
    /// Resolved cell borders and background fill.
    pub border: BorderFillInfo,
    pub paragraphs: Vec<ParagraphModel>,
}

/// A cell's content height: the bottom of its tallest stored line (cumulative
/// `vertpos + line_height` across the cell's paragraphs) plus the inner top and
/// bottom margins — the room the cell needs for its text.
fn cell_content_height(cell: &CellInfo) -> i32 {
    let inner = cell
        .paragraphs
        .iter()
        .flat_map(|p| &p.stored_line_segs)
        .map(|s| s.vertical_pos.raw() + s.line_height.raw())
        .max()
        .unwrap_or(0);
    if inner == 0 {
        return 0;
    }
    inner + cell.padding.top.raw() + cell.padding.bottom.raw()
}

impl TableInfo {
    /// Per-row heights recovered from stored geometry. Each row's base height is
    /// the tallest non-spanning cell in it, where a cell needs `max(cellSz,
    /// content height + padding)` — Hancom often stores a tiny uniform `cellSz`
    /// placeholder (e.g. 282) while the real row height lives only in the table
    /// box total, so trusting `cellSz` alone collapses every row but one. Any gap
    /// between the row bases and the stored box height (`<hp:sz>`) is spread over
    /// the auto rows (those with no base), or, when every row is sized, evenly
    /// over all rows — never dumped onto the last row, which would crush the rest.
    /// Returns `None` when no box height is stored (caller derives from content).
    pub fn stored_row_heights(&self) -> Option<Vec<i32>> {
        if self.height.raw() <= 0 {
            return None;
        }
        let n = self.rows as usize;
        let mut rows = vec![0i32; n];
        for c in &self.cells {
            if c.row_span.max(1) != 1 {
                continue;
            }
            if let Some(slot) = rows.get_mut(c.row as usize) {
                *slot = (*slot).max(c.height.raw().max(cell_content_height(c)));
            }
        }
        let gap = self.height.raw() - rows.iter().sum::<i32>();
        if gap > 0 {
            let auto: Vec<usize> = (0..n).filter(|&i| rows[i] == 0).collect();
            let targets: Vec<usize> = if auto.is_empty() { (0..n).collect() } else { auto };
            if let Some(&last) = targets.last() {
                let each = gap / targets.len() as i32;
                for &i in &targets {
                    rows[i] += each;
                }
                rows[last] += gap - each * targets.len() as i32;
            }
        }
        Some(rows)
    }
}

/// Vertical alignment of a table cell's content block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellVAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

/// Rendered border line style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
}

/// One resolved border edge (style + width in HWPUNIT + ColorRef).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BorderEdge {
    pub style: BorderStyle,
    pub width: EngineUnit,
    pub color: u32,
}

impl BorderEdge {
    pub fn visible(&self) -> bool {
        self.style != BorderStyle::None
    }
}

/// A resolved `<hh:borderFill>`: four edges plus an optional solid fill color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BorderFillInfo {
    pub left: BorderEdge,
    pub right: BorderEdge,
    pub top: BorderEdge,
    pub bottom: BorderEdge,
    /// Solid background ColorRef; None when no fill or "none".
    pub fill: Option<u32>,
}

/// Border width index → mm (HWP spec table 27).
const BORDER_WIDTH_MM: [f64; 16] = [
    0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
];

/// mm → HWPUNIT (1 inch = 25.4 mm = 7200 HWPUNIT).
fn mm_to_hwp(mm: f64) -> i32 {
    (mm * 7200.0 / 25.4).round() as i32
}

fn edge_of(b: &kdsnr_hwp_parser::model::style::BorderLine) -> BorderEdge {
    use kdsnr_hwp_parser::model::style::BorderLineType as T;
    let style = match b.line_type {
        T::None => BorderStyle::None,
        T::Dash | T::LongDash => BorderStyle::Dashed,
        T::Dot => BorderStyle::Dotted,
        T::Double | T::ThinThickDouble | T::ThickThinDouble | T::ThinThickThinTriple => {
            BorderStyle::Double
        }
        _ => BorderStyle::Solid,
    };
    let mm = BORDER_WIDTH_MM.get(b.width as usize).copied().unwrap_or(0.1);
    BorderEdge { style, width: EngineUnit::new(mm_to_hwp(mm)), color: b.color }
}

/// Resolve a `borderFillIDRef` (1-based; 0 = none) into render-ready edges/fill.
pub fn resolve_border_fill(doc: &Document, id: u16) -> BorderFillInfo {
    use kdsnr_hwp_parser::model::style::FillType;
    // borderFillIDRef is 1-based; `border_fills` is stored 0-based (id 1 → [0]).
    if id == 0 {
        return BorderFillInfo::default();
    }
    let Some(bf) = doc.doc_info.border_fills.get((id - 1) as usize) else {
        return BorderFillInfo::default();
    };
    // Solid fill, dropping the "none" sentinel (0xFFFFFFFF).
    let fill = (bf.fill.fill_type == FillType::Solid)
        .then(|| bf.fill.solid.as_ref().map(|s| s.background_color))
        .flatten()
        .filter(|&c| c != 0xFFFFFFFF);
    // borders order: [left, right, top, bottom].
    BorderFillInfo {
        left: edge_of(&bf.borders[0]),
        right: edge_of(&bf.borders[1]),
        top: edge_of(&bf.borders[2]),
        bottom: edge_of(&bf.borders[3]),
        fill,
    }
}

/// A character with its font and shape metrics resolved for its script slot.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedChar {
    pub code_unit: u16,
    pub font_size_pt: f32,
    pub font_face: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 장평 (width ratio) %.
    pub ratio: u16,
    /// 자간 (letter spacing) %.
    pub spacing: i16,
    /// 상대 크기 %.
    pub rel_sz: u16,
    /// 글자색 ColorRef (0x00BBGGRR; 0 = 검정).
    pub color: u32,
    /// 글자 위치 (세로 오프셋) — 글자 크기의 %, 양수 = 위로.
    pub char_offset: i16,
    /// 음영색 ColorRef (0xFFFFFFFF = 없음).
    pub shade: u32,
    pub strikeout: bool,
    pub strike_shape: u8,
    pub strike_color: u32,
    pub underline_color: u32,
    pub underline_shape: u8,
    /// 글자 테두리/배경 (charPr borderFillIDRef). diagonal-only → 빈 값.
    pub border: BorderFillInfo,
    /// Stored width of a tab char (U+0009) from `<hp:tab width>` (HWPUNIT); 0 otherwise.
    pub tab_width: i32,
    /// The face is a Hancom HFT font (`<hh:font type="HFT">`): glyph outline and
    /// advance come from the `.HFT` decoder, not a substitute TTF.
    pub is_hft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSpacingKind {
    Percent,
    Fixed,
    SpaceOnly,
    Minimum,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSpacing {
    pub kind: LineSpacingKind,
    /// Raw ParaShape value: percent for [`LineSpacingKind::Percent`], HWPUNIT otherwise.
    pub value: i32,
}

/// Mirror of the parser `LineSeg` / HWPX `<hp:lineseg>` record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoredLineSeg {
    pub text_start: u32,
    pub vertical_pos: EngineUnit,
    pub line_height: EngineUnit,
    pub text_height: EngineUnit,
    pub baseline_distance: EngineUnit,
    pub line_spacing: EngineUnit,
    pub column_start: EngineUnit,
    pub segment_width: EngineUnit,
    pub tag: u32,
}

fn rect_from_areas(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect::new(left, top, (right - left).max(0), (bottom - top).max(0))
}

fn break_before(t: ColumnBreakType) -> BreakBefore {
    match t {
        ColumnBreakType::None => BreakBefore::None,
        ColumnBreakType::Section => BreakBefore::Section,
        ColumnBreakType::MultiColumn => BreakBefore::MultiColumn,
        ColumnBreakType::Page => BreakBefore::Page,
        ColumnBreakType::Column => BreakBefore::Column,
    }
}

/// Normalize the first `ColumnDef` control found in a section's paragraphs.
/// Sections without one keep the single-column default.
fn column_layout(section: &kdsnr_hwp_parser::model::document::Section) -> ColumnLayout {
    for para in &section.paragraphs {
        for control in &para.controls {
            if let Control::ColumnDef(def) = control {
                return column_layout_from_def(def);
            }
        }
    }
    ColumnLayout::default()
}

fn column_layout_from_def(def: &ColumnDef) -> ColumnLayout {
    ColumnLayout {
        count: def.column_count.max(1),
        same_width: def.same_width,
        gap: def.spacing as i32,
        widths: def.widths.iter().map(|w| *w as i32).collect(),
        gaps: def.gaps.iter().map(|g| *g as i32).collect(),
        proportional: def.proportional_widths,
        right_to_left: matches!(
            def.direction,
            kdsnr_hwp_parser::model::page::ColumnDirection::RightToLeft
        ),
    }
}

fn page_apply(a: HeaderFooterApply) -> PageApply {
    match a {
        HeaderFooterApply::Both => PageApply::Both,
        HeaderFooterApply::Even => PageApply::Even,
        HeaderFooterApply::Odd => PageApply::Odd,
    }
}

/// Normalize a furniture paragraph list (header/footer/master content). These
/// paragraphs live outside the body flow, so they get synthetic indices.
fn furniture_paragraphs(
    doc: &Document,
    section: SectionId,
    base: usize,
    paras: &[Paragraph],
) -> Vec<ParagraphModel> {
    paras
        .iter()
        .enumerate()
        .map(|(i, p)| normalize_paragraph(doc, section, base + i, p, &mut std::collections::HashMap::new()))
        .collect()
}

/// Collect header and footer definitions from a section's controls.
fn header_footers(
    doc: &Document,
    section_id: SectionId,
    section: &Section,
) -> (Vec<HeaderFooter>, Vec<HeaderFooter>) {
    let mut headers = Vec::new();
    let mut footers = Vec::new();
    for para in &section.paragraphs {
        for control in &para.controls {
            match control {
                // Each header/footer gets a distinct synthetic index base so its
                // paragraphs don't collide in the paint lookup (a section has
                // several headers — odd/even/both — with different content).
                Control::Header(h) => {
                    let base = 100_000 + headers.len() * 1000;
                    headers.push(HeaderFooter {
                        apply: page_apply(h.apply_to),
                        paragraphs: furniture_paragraphs(doc, section_id, base, &h.paragraphs),
                    });
                }
                Control::Footer(f) => {
                    let base = 200_000 + footers.len() * 1000;
                    footers.push(HeaderFooter {
                        apply: page_apply(f.apply_to),
                        paragraphs: furniture_paragraphs(doc, section_id, base, &f.paragraphs),
                    });
                }
                _ => {}
            }
        }
    }
    (headers, footers)
}

/// Collect endnote content from a paragraph list in document order, descending
/// into table cells (endnote references can sit inside cells). Each endnote's
/// own paragraphs are normalized and appended; endnotes are flowed after the
/// body, so they share the body's synthetic-index space (base 400_000).
fn collect_endnotes(
    doc: &Document,
    section_id: SectionId,
    paras: &[Paragraph],
    out: &mut Vec<ParagraphModel>,
) {
    for para in paras {
        for control in &para.controls {
            match control {
                Control::Endnote(e) => {
                    for p in &e.paragraphs {
                        let idx = 400_000 + out.len();
                        out.push(normalize_paragraph(doc, section_id, idx, p, &mut std::collections::HashMap::new()));
                    }
                }
                Control::Table(t) => {
                    for cell in &t.cells {
                        collect_endnotes(doc, section_id, &cell.paragraphs, out);
                    }
                }
                _ => {}
            }
        }
    }
}

fn master_page_models(
    doc: &Document,
    section_id: SectionId,
    masters: &[MasterPage],
) -> Vec<MasterPageModel> {
    masters
        .iter()
        .enumerate()
        .map(|(mi, m)| {
            // Parser convention: ext_flags==3 = LAST_PAGE, >=4 = OPTIONAL_PAGE (page = ext_flags-3).
            let (target_page, is_last_page) = match m.ext_flags {
                3 => (None, true),
                n if n >= 4 => (Some((n - 3) as usize), false),
                _ => (None, false),
            };
            // Distinct index base per master so its paragraphs don't collide.
            let base = 300_000 + mi * 1000;
            MasterPageModel {
                apply: page_apply(m.apply_to),
                is_extension: m.is_extension,
                overlap: m.overlap,
                target_page,
                is_last_page,
                paragraphs: furniture_paragraphs(doc, section_id, base, &m.paragraphs),
            }
        })
        .collect()
}

fn spacing_kind(t: LineSpacingType) -> LineSpacingKind {
    match t {
        LineSpacingType::Percent => LineSpacingKind::Percent,
        LineSpacingType::Fixed => LineSpacingKind::Fixed,
        LineSpacingType::SpaceOnly => LineSpacingKind::SpaceOnly,
        LineSpacingType::Minimum => LineSpacingKind::Minimum,
    }
}

fn stored_seg(s: &ParserLineSeg) -> StoredLineSeg {
    StoredLineSeg {
        text_start: s.text_start,
        vertical_pos: EngineUnit::new(s.vertical_pos),
        line_height: EngineUnit::new(s.line_height),
        text_height: EngineUnit::new(s.text_height),
        baseline_distance: EngineUnit::new(s.baseline_distance),
        line_spacing: EngineUnit::new(s.line_spacing),
        column_start: EngineUnit::new(s.column_start),
        segment_width: EngineUnit::new(s.segment_width),
        tag: s.tag,
    }
}

/// Table width = sum of first-row cell widths.
fn table_width(t: &Table) -> i32 {
    let first_row: i32 = t
        .cells
        .iter()
        .filter(|c| c.row == 0)
        .map(|c| c.width as i32)
        .sum();
    first_row.max(t.cells.iter().map(|c| c.width as i32).max().unwrap_or(0))
}

fn table_info(doc: &Document, section: SectionId, p_idx: usize, t: &Table) -> TableInfo {
    let cells = t
        .cells
        .iter()
        .map(|c| {
            // A cell stores line segments relative to its inner margin. When the
            // cell sets no own margin it inherits the table default, so the
            // effective inset (content origin) must pick the right one.
            let m = if c.apply_inner_margin { &c.padding } else { &t.padding };
            CellInfo {
                row: c.row,
                col: c.col,
                row_span: c.row_span.max(1),
                col_span: c.col_span.max(1),
                width: EngineUnit::new(c.width as i32),
                height: EngineUnit::new(c.height as i32),
                padding: Insets {
                    left: EngineUnit::new(m.left as i32),
                    right: EngineUnit::new(m.right as i32),
                    top: EngineUnit::new(m.top as i32),
                    bottom: EngineUnit::new(m.bottom as i32),
                },
                vertical_align: match c.vertical_align {
                    VerticalAlign::Top => CellVAlign::Top,
                    VerticalAlign::Center => CellVAlign::Center,
                    VerticalAlign::Bottom => CellVAlign::Bottom,
                },
                border: resolve_border_fill(doc, c.border_fill_id),
                paragraphs: c
                    .paragraphs
                    .iter()
                    .map(|p| normalize_paragraph(doc, section, p_idx, p, &mut std::collections::HashMap::new()))
                    .collect(),
            }
        })
        .collect();
    TableInfo {
        rows: t.row_count,
        cols: t.col_count,
        width: EngineUnit::new(table_width(t)),
        height: EngineUnit::new(t.common.height as i32),
        cells,
        // A treat-as-char table flows inline; a floating one carries its anchor.
        anchor: (!t.common.treat_as_char).then(|| anchor_of(&t.common)),
        // Set by normalize_paragraph from the run order.
        inline_pos: None,
        margin: Insets {
            left: EngineUnit::new(t.outer_margin_left as i32),
            right: EngineUnit::new(t.outer_margin_right as i32),
            top: EngineUnit::new(t.outer_margin_top as i32),
            bottom: EngineUnit::new(t.outer_margin_bottom as i32),
        },
        reserves_vertical_band: t.common.text_wrap
            == kdsnr_hwp_parser::model::shape::TextWrap::TopAndBottom,
    }
}

/// Object reserves vertical body-flow space when it is inline (treat-as-char)
/// or floats relative to the paragraph with a wrap that pushes text away.
/// Page/paper-anchored objects are positioned absolutely and reserve nothing.
fn object_in_flow(common: &CommonObjAttr) -> bool {
    if common.treat_as_char {
        return true;
    }
    let overlaps = matches!(
        common.text_wrap,
        TextWrap::BehindText | TextWrap::InFrontOfText
    );
    common.vert_rel_to == VertRelTo::Para && !overlaps
}

/// Map a parser anchor (`<hp:pos>`) to the model's placement anchor.
fn anchor_of(common: &CommonObjAttr) -> Anchor {
    use kdsnr_hwp_parser::model::shape::{HorzAlign, HorzRelTo, VertAlign, VertRelTo as VR};
    let vert_rel = match common.vert_rel_to {
        VR::Paper => AnchorRel::Paper,
        VR::Page => AnchorRel::Page,
        VR::Para => AnchorRel::Para,
    };
    let horz_rel = match common.horz_rel_to {
        HorzRelTo::Paper => AnchorRel::Paper,
        HorzRelTo::Page => AnchorRel::Page,
        HorzRelTo::Column => AnchorRel::Column,
        HorzRelTo::Para => AnchorRel::Para,
    };
    let vert_align = match common.vert_align {
        VertAlign::Top => AnchorAlign::Top,
        VertAlign::Center => AnchorAlign::Center,
        VertAlign::Bottom => AnchorAlign::Bottom,
        // Inside/Outside fold to None: offset is the raw coordinate.
        _ => AnchorAlign::None,
    };
    let horz_align = match common.horz_align {
        HorzAlign::Left => AnchorAlign::Left,
        HorzAlign::Center => AnchorAlign::Center,
        HorzAlign::Right => AnchorAlign::Right,
        _ => AnchorAlign::None,
    };
    Anchor {
        vert_rel,
        horz_rel,
        vert_align,
        horz_align,
        v_offset: EngineUnit::new(common.vertical_offset as i32),
        h_offset: EngineUnit::new(common.horizontal_offset as i32),
    }
}

fn object_info(
    kind: ObjectKind,
    control_index: usize,
    common: &CommonObjAttr,
    content: ObjectContent,
    text_box: Option<ObjectTextBox>,
) -> ObjectInfo {
    ObjectInfo {
        kind,
        control_index,
        width: EngineUnit::new(common.width as i32),
        height: EngineUnit::new(common.height as i32),
        margin: Insets {
            left: EngineUnit::new(common.margin.left as i32),
            right: EngineUnit::new(common.margin.right as i32),
            top: EngineUnit::new(common.margin.top as i32),
            bottom: EngineUnit::new(common.margin.bottom as i32),
        },
        h_offset: EngineUnit::new(common.horizontal_offset as i32),
        v_offset: EngineUnit::new(common.vertical_offset as i32),
        anchor: anchor_of(common),
        content,
        text_box,
        behind_text: common.text_wrap == kdsnr_hwp_parser::model::shape::TextWrap::BehindText,
        treat_as_char: common.treat_as_char,
        inline_pos: None,
        in_flow: object_in_flow(common),
        reserves_vertical_band: common.text_wrap
            == kdsnr_hwp_parser::model::shape::TextWrap::TopAndBottom,
    }
}

/// The drawing attributes (border/fill/text box) of a shape variant that has them.
fn shape_drawing(
    shape: &kdsnr_hwp_parser::model::shape::ShapeObject,
) -> Option<&kdsnr_hwp_parser::model::shape::DrawingObjAttr> {
    use kdsnr_hwp_parser::model::shape::ShapeObject;
    match shape {
        ShapeObject::Line(s) => Some(&s.drawing),
        ShapeObject::Rectangle(s) => Some(&s.drawing),
        ShapeObject::Ellipse(s) => Some(&s.drawing),
        ShapeObject::Arc(s) => Some(&s.drawing),
        ShapeObject::Polygon(s) => Some(&s.drawing),
        ShapeObject::Curve(s) => Some(&s.drawing),
        _ => None,
    }
}

/// Extract a shape's text box (drawText) as renderable paragraphs in the box.
fn shape_text_box(
    doc: &Document,
    section: SectionId,
    p_idx: usize,
    shape: &kdsnr_hwp_parser::model::shape::ShapeObject,
) -> Option<ObjectTextBox> {
    let tb = shape_drawing(shape)?.text_box.as_ref()?;
    if tb.paragraphs.is_empty() {
        return None;
    }
    let paragraphs = tb
        .paragraphs
        .iter()
        .map(|p| normalize_paragraph(doc, section, p_idx, p, &mut std::collections::HashMap::new()))
        .collect();
    use kdsnr_hwp_parser::model::table::VerticalAlign;
    Some(ObjectTextBox {
        paragraphs,
        vertical_align: match tb.vertical_align {
            VerticalAlign::Top => CellVAlign::Top,
            VerticalAlign::Center => CellVAlign::Center,
            VerticalAlign::Bottom => CellVAlign::Bottom,
        },
        margin: Insets {
            left: EngineUnit::new(tb.margin_left as i32),
            right: EngineUnit::new(tb.margin_right as i32),
            top: EngineUnit::new(tb.margin_top as i32),
            bottom: EngineUnit::new(tb.margin_bottom as i32),
        },
    })
}

/// Resolve a picture's embedded raster from the document's bin-data store.
fn picture_content(doc: &Document, pic: &kdsnr_hwp_parser::model::image::Picture) -> ObjectContent {
    let id = pic.image_attr.bin_data_id;
    match doc.bin_data_content.iter().find(|b| b.id == id) {
        Some(b) => ObjectContent::Image {
            data: std::rc::Rc::new(b.data.clone()),
            ext: b.extension.to_ascii_lowercase(),
        },
        None => ObjectContent::None,
    }
}

/// Solid fill color of a drawing fill, or None when not a solid fill.
fn fill_color(fill: &kdsnr_hwp_parser::model::style::Fill) -> Option<u32> {
    use kdsnr_hwp_parser::model::style::FillType;
    (fill.fill_type == FillType::Solid)
        .then(|| fill.solid.as_ref().map(|s| s.background_color))
        .flatten()
}

/// Decode a shape border-line style from `ShapeBorderLine.attr` (low byte, set
/// by the hwpx `lineShape style`: 0=NONE, 1=SOLID, 2=DASH/LONG_DASH, 3=DOT, …).
fn shape_border_style(line: &kdsnr_hwp_parser::model::style::ShapeBorderLine) -> BorderStyle {
    // The line type is the low 6 bits (0x3F); bits 6-7 carry other flags. HWP
    // shapes set those flags on a NONE line (color stored but not drawn), so
    // masking the full byte made `& 0xFF != 0` fall through to Solid and paint a
    // phantom border. Match the HWPX serializer, which reads `attr & 0x3F`.
    match line.attr & 0x3F {
        0 => BorderStyle::None,
        2 | 6 => BorderStyle::Dashed,
        3 => BorderStyle::Dotted,
        8..=11 => BorderStyle::Double,
        _ => BorderStyle::Solid,
    }
}

/// Resolve a drawing shape's renderable content (rect fill/border, line endpoints).
fn shape_content(shape: &kdsnr_hwp_parser::model::shape::ShapeObject) -> ObjectContent {
    use kdsnr_hwp_parser::model::shape::ShapeObject;
    match shape {
        ShapeObject::Rectangle(r) => ObjectContent::Rect {
            fill: fill_color(&r.drawing.fill),
            border_color: r.drawing.border_line.color,
            border_width: r.drawing.border_line.width,
            border: shape_border_style(&r.drawing.border_line),
        },
        ShapeObject::Line(l) => ObjectContent::Line {
            x1: l.start.x,
            y1: l.start.y,
            x2: l.end.x,
            y2: l.end.y,
            color: l.drawing.border_line.color,
            width: l.drawing.border_line.width,
            style: shape_border_style(&l.drawing.border_line),
        },
        _ => ObjectContent::None,
    }
}

/// Resolve the char-shape id active at `utf16_index` from a paragraph's
/// `char_shapes` ranges (each `CharShapeRef` applies from its `start_pos`).
fn char_shape_at(refs: &[kdsnr_hwp_parser::model::paragraph::CharShapeRef], utf16_index: u32) -> u32 {
    let mut chosen = refs.first().map(|r| r.char_shape_id).unwrap_or(0);
    for r in refs {
        if r.start_pos <= utf16_index {
            chosen = r.char_shape_id;
        } else {
            break;
        }
    }
    chosen
}

/// Normalize parser output into a [`DocumentModel`].
/// Every font face name the model actually draws with: per-char faces plus
/// equation fonts, recursing into table cells and shape text boxes, across the
/// body, page furniture, master pages, and endnotes. The deployment font check
/// compares this set against the bundled `.fonts` directory.
pub fn required_faces(model: &DocumentModel) -> std::collections::BTreeSet<String> {
    let mut faces = std::collections::BTreeSet::new();
    for sec in &model.sections {
        collect_faces(&sec.paragraphs, &mut faces);
        for hf in sec.headers.iter().chain(&sec.footers) {
            collect_faces(&hf.paragraphs, &mut faces);
        }
        for mp in &sec.master_pages {
            collect_faces(&mp.paragraphs, &mut faces);
        }
        collect_faces(&sec.endnotes, &mut faces);
    }
    faces
}

fn collect_faces(paras: &[ParagraphModel], out: &mut std::collections::BTreeSet<String>) {
    for p in paras {
        for c in &p.chars {
            if !c.font_face.is_empty() {
                out.insert(c.font_face.clone());
            }
        }
        for o in &p.objects {
            if let ObjectContent::Equation { font, .. } = &o.content {
                if !font.is_empty() {
                    out.insert(font.clone());
                }
            }
            if let Some(tb) = &o.text_box {
                collect_faces(&tb.paragraphs, out);
            }
        }
        for t in &p.tables {
            for cell in &t.cells {
                collect_faces(&cell.paragraphs, out);
            }
        }
    }
}

/// Deployment font check: verify that `.fonts` (at `fonts_dir`) holds a file for
/// every face `model` needs. Returns `Err` with the operator error table (header
/// + `idx | 문서명 | 폰트명 | 폰트파일명`) when any file is missing, `Ok` otherwise.
/// `doc_name` is the source document name shown in the 문서명 column.
pub fn check_fonts(
    model: &DocumentModel,
    doc_name: &str,
    fonts_dir: &std::path::Path,
) -> Result<(), String> {
    let manifest = kdsnr_hwp_font::FontManifest::load(fonts_dir);
    let faces: Vec<String> = required_faces(model).into_iter().collect();
    let missing = manifest.missing_for(doc_name, &faces);
    match kdsnr_hwp_font::format_missing_table(&missing) {
        Some(table) => Err(table),
        None => Ok(()),
    }
}

pub fn normalize(doc: &Document) -> DocumentModel {
    let mut sections = Vec::with_capacity(doc.sections.len());
    // Heading-number counters, keyed by (numbering id, level), advanced in
    // document order across the whole document.
    let mut num_state: std::collections::HashMap<(u16, u8), u32> = std::collections::HashMap::new();

    for (sec_idx, section) in doc.sections.iter().enumerate() {
        let id = SectionId(sec_idx);
        let page_def = &section.section_def.page_def;
        let areas = PageAreas::from_page_def(page_def);
        let (page_w, page_h) = if page_def.landscape {
            (page_def.height as i32, page_def.width as i32)
        } else {
            (page_def.width as i32, page_def.height as i32)
        };
        let page_rect = Rect::new(0, 0, page_w, page_h);
        // PAGE anchor frame: the area inside the paper margins.
        let (ml, mr, mt, mb) = (
            page_def.margin_left as i32,
            page_def.margin_right as i32,
            page_def.margin_top as i32,
            page_def.margin_bottom as i32,
        );
        let page_area = Rect::new(ml, mt, (page_w - ml - mr).max(0), (page_h - mt - mb).max(0));
        // A page border (`textBorder="PAPER"`, e.g. offset 1417 from the paper
        // edge) is a decorative rectangle near the paper edge — its offset is well
        // inside the body margins, so it does NOT move the text frame. The body,
        // header, and footer stay at their margin positions; this keeps them
        // aligned with the PAPER-anchored exam rules (the column divider sits at the
        // body's horizontal center, the header rule just above the first body line).
        let body_rect = rect_from_areas(
            areas.body_area.left,
            areas.body_area.top,
            areas.body_area.right,
            areas.body_area.bottom,
        );
        let header_rect = rect_from_areas(
            areas.header_area.left,
            areas.header_area.top,
            areas.header_area.right,
            areas.header_area.bottom,
        );
        let footer_rect = rect_from_areas(
            areas.footer_area.left,
            areas.footer_area.top,
            areas.footer_area.right,
            areas.footer_area.bottom,
        );
        let footnote_rect = rect_from_areas(
            areas.footnote_area.left,
            areas.footnote_area.top,
            areas.footnote_area.right,
            areas.footnote_area.bottom,
        );

        let mut paragraphs = Vec::with_capacity(section.paragraphs.len());
        for (p_idx, para) in section.paragraphs.iter().enumerate() {
            paragraphs.push(normalize_paragraph(doc, id, p_idx, para, &mut num_state));
        }

        let (mut headers, mut footers) = header_footers(doc, id, section);
        let sdef = &section.section_def;
        if sdef.hide_header {
            headers.clear();
        }
        if sdef.hide_footer {
            footers.clear();
        }
        let master_pages = if sdef.hide_master_page {
            Vec::new()
        } else {
            master_page_models(doc, id, &sdef.master_pages)
        };

        let mut endnotes = Vec::new();
        collect_endnotes(doc, id, &section.paragraphs, &mut endnotes);

        sections.push(SectionModel {
            id,
            page_rect,
            page_area,
            body_rect,
            header_rect,
            footer_rect,
            footnote_rect,
            columns: column_layout(section),
            paragraphs,
            headers,
            footers,
            master_pages,
            endnotes,
        });
    }

    DocumentModel { sections }
}

fn normalize_paragraph(
    doc: &Document,
    section: SectionId,
    p_idx: usize,
    para: &kdsnr_hwp_parser::model::paragraph::Paragraph,
    num_state: &mut std::collections::HashMap<(u16, u8), u32>,
) -> ParagraphModel {
    let para_shape = doc.doc_info.para_shapes.get(para.para_shape_id as usize);
    let auto_number = para_shape.and_then(|ps| compute_auto_number(doc, ps, num_state));
    let spacing = para_shape
        .map(|ps| LineSpacing {
            kind: spacing_kind(ps.line_spacing_type),
            value: ps.line_spacing,
        })
        .unwrap_or(LineSpacing {
            kind: LineSpacingKind::Percent,
            value: 100,
        });
    // ParaShape margins are kept in Hancom's binary 2x HWPUNIT scale; render
    // units are plain HWPUNIT, so halve.
    let first_line_indent = EngineUnit::new(para_shape.map(|ps| ps.indent / 2).unwrap_or(0));
    // Paragraph space before/after (paraPr margin prev/next), 2x scale → halve.
    // Verified: next paragraph's first-line vertpos = this paragraph's last-line
    // bottom + space_after, so this is the inter-paragraph gap pagination needs.
    let space_before = EngineUnit::new(para_shape.map(|ps| ps.spacing_before / 2).unwrap_or(0));
    let space_after = EngineUnit::new(para_shape.map(|ps| ps.spacing_after / 2).unwrap_or(0));
    let border_fill_id = para_shape.map(|ps| ps.border_fill_id).unwrap_or(0);
    // paraShape attr1 bit 28: connect this border with adjacent same-borderFill
    // paragraphs into one region (the HWPX `<hh:border connect>`). When set, the
    // box is the outline of the connected run's union; otherwise each paragraph
    // is its own closed box.
    let border_connect = para_shape.map(|ps| ps.attr1 & (1 << 28) != 0).unwrap_or(false);
    // border_spacing = [left, right, top, bottom], plain HWPUNIT (outside switch).
    let border_offsets = para_shape
        .map(|ps| Insets {
            left: EngineUnit::new(ps.border_spacing[0] as i32),
            right: EngineUnit::new(ps.border_spacing[1] as i32),
            top: EngineUnit::new(ps.border_spacing[2] as i32),
            bottom: EngineUnit::new(ps.border_spacing[3] as i32),
        })
        .unwrap_or_default();

    // `char_shapes` start positions live in the control-inclusive UTF-16 space
    // (each inline control occupies 8 units), the same space as `char_offsets`.
    // `para.text` excludes control chars, so map each visible UTF-16 unit back to
    // that space before resolving its char shape — otherwise leading controls
    // (secPr/table/line) shift every run's color/font onto the wrong character.
    let unit_pos: Vec<u32> = para
        .text
        .chars()
        .enumerate()
        .flat_map(|(k, ch)| {
            let pos = para.char_offsets.get(k).copied().unwrap_or(k as u32);
            std::iter::repeat(pos).take(ch.len_utf16())
        })
        .collect();

    let mut chars = Vec::new();
    let mut tab_idx = 0usize;
    for (i, unit) in para.text.encode_utf16().enumerate() {
        let pos = unit_pos.get(i).copied().unwrap_or(i as u32);
        let cs_id = char_shape_at(&para.char_shapes, pos);
        let cs = doc.doc_info.char_shapes.get(cs_id as usize);
        // Per-script slot (surrogate halves → Symbol, fine for body text).
        let ch = char::from_u32(unit as u32).unwrap_or(' ');
        let si = kdsnr_hwp_font::script_of(ch).index();
        let resolved = match cs {
            Some(cs) => {
                let font = doc
                    .doc_info
                    .font_faces
                    .get(si)
                    .and_then(|faces| faces.get(cs.font_ids[si] as usize));
                let face = font.map(|f| f.name.clone()).unwrap_or_default();
                // alt_type: 1 = TTF, 2 = HFT (Hancom's own font format).
                let is_hft = font.map(|f| f.alt_type == 2).unwrap_or(false);
                ResolvedChar {
                    code_unit: unit,
                    font_size_pt: cs.base_size as f32 / 100.0,
                    font_face: face,
                    is_hft,
                    bold: cs.bold,
                    italic: cs.italic,
                    underline: cs.underline_type
                        != kdsnr_hwp_parser::model::style::UnderlineType::None,
                    ratio: cs.ratios[si] as u16,
                    spacing: cs.spacings[si] as i16,
                    rel_sz: cs.relative_sizes[si] as u16,
                    color: cs.text_color,
                    char_offset: cs.char_offsets[si] as i16,
                    shade: cs.shade_color,
                    strikeout: cs.strikethrough,
                    strike_shape: cs.strike_shape,
                    strike_color: cs.strike_color,
                    underline_color: cs.underline_color,
                    underline_shape: cs.underline_shape,
                    border: resolve_border_fill(doc, cs.border_fill_id),
                    tab_width: 0,
                }
            }
            None => ResolvedChar {
                code_unit: unit,
                font_size_pt: 10.0,
                font_face: String::new(),
                is_hft: false,
                bold: false,
                italic: false,
                underline: false,
                ratio: 100,
                spacing: 0,
                rel_sz: 100,
                color: 0,
                char_offset: 0,
                shade: 0xFFFFFFFF,
                strikeout: false,
                strike_shape: 0,
                strike_color: 0,
                underline_color: 0,
                underline_shape: 0,
                border: BorderFillInfo::default(),
                tab_width: 0,
            },
        };
        // Tab (U+0009): its advance is Hancom's stored width (tab_extended[n][0]).
        let mut resolved = resolved;
        if unit == 0x0009 {
            resolved.tab_width = para
                .tab_extended
                .get(tab_idx)
                .map(|e| e[0] as i32)
                .unwrap_or(0);
            tab_idx += 1;
        }
        chars.push(resolved);
    }

    // Visual char index of each inline control, from the run order: text runs
    // advance the count, controls mark a position (their object char is not in
    // the visible text). Lets paint place a treat-as-char object in its line.
    let mut control_pos: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    {
        use kdsnr_hwp_parser::model::paragraph::ParagraphItem;
        let mut vpos = 0u32;
        for item in &para.items {
            match item {
                ParagraphItem::Text(s) => vpos += s.chars().count() as u32,
                ParagraphItem::Control(idx) => {
                    control_pos.insert(*idx, vpos);
                }
            }
        }
    }

    let tables = para
        .controls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Control::Table(t) => {
                let mut ti = table_info(doc, section, p_idx, t);
                if t.common.treat_as_char {
                    ti.inline_pos = control_pos.get(&i).copied();
                }
                Some(ti)
            }
            _ => None,
        })
        .collect();

    let objects = para
        .controls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let mut obj = match c {
                Control::Picture(p) => {
                    object_info(ObjectKind::Picture, i, &p.common, picture_content(doc, p), None)
                }
                Control::Shape(s) => object_info(
                    ObjectKind::Shape,
                    i,
                    s.common(),
                    shape_content(s),
                    shape_text_box(doc, section, p_idx, s),
                ),
                Control::Equation(e) => object_info(
                    ObjectKind::Equation,
                    i,
                    &e.common,
                    ObjectContent::Equation {
                        script: e.script.clone(),
                        font: e.font_name.clone(),
                        color: e.color,
                        font_size: e.font_size,
                    },
                    None,
                ),
                _ => return None,
            };
            if obj.treat_as_char {
                obj.inline_pos = control_pos.get(&i).copied();
            }
            Some(obj)
        })
        .collect();

    ParagraphModel {
        id: ParagraphId {
            section,
            index: p_idx,
        },
        text: para.text.clone(),
        chars,
        // Carry the gapped offsets only when an inline object shifts them; an
        // empty vec means gapped == visible (the common no-object case).
        char_offsets: {
            let has_gap = para
                .char_offsets
                .iter()
                .enumerate()
                .any(|(i, &o)| o != i as u32);
            if has_gap { para.char_offsets.clone() } else { Vec::new() }
        },
        spacing,
        break_before: break_before(para.column_type),
        tables,
        objects,
        stored_line_segs: para.line_segs.iter().map(stored_seg).collect(),
        first_line_indent,
        space_before,
        space_after,
        align: para_shape.map(|ps| align_of(ps.alignment)).unwrap_or_default(),
        border: resolve_border_fill(doc, border_fill_id),
        border_fill_id,
        border_connect,
        border_offsets,
        auto_number,
    }
}

/// Generate a paragraph's heading number/bullet prefix, advancing the per-level
/// counter. Returns None unless the paragraph activates a NUMBER/BULLET heading.
/// `^k` in the level's format template is replaced by the level-k counter.
fn compute_auto_number(
    doc: &Document,
    ps: &kdsnr_hwp_parser::model::style::ParaShape,
    state: &mut std::collections::HashMap<(u16, u8), u32>,
) -> Option<String> {
    use kdsnr_hwp_parser::model::style::HeadType;
    let numbering = match ps.head_type {
        HeadType::Number => doc.doc_info.numberings.get(ps.numbering_id.checked_sub(1)? as usize)?,
        // Bullet/Outline: not generated yet (bullet char glyph is a later step).
        _ => return None,
    };
    let lvl = (ps.para_level as usize).min(6); // 0-based level index (0 = level 1)
    let template = &numbering.level_formats[lvl];
    if template.is_empty() {
        return None;
    }
    // Advance this level's counter (start from its stored start, else 1).
    let start = numbering.level_start_numbers[lvl].max(1);
    let key = (ps.numbering_id, lvl as u8);
    let n = state.entry(key).or_insert(start);
    let current = *n;
    *n += 1;
    // Replace ^1..^(lvl+1): own level uses the new count; ancestors use their
    // last counter. Each level formats per its own numFormat code.
    let mut out = template.clone();
    for k in 1..=(lvl + 1) {
        let token = format!("^{k}");
        if !out.contains(&token) {
            continue;
        }
        let val = if k == lvl + 1 {
            current
        } else {
            state.get(&(ps.numbering_id, (k - 1) as u8)).copied().unwrap_or(1)
        };
        let fmt = numbering.heads[k - 1].number_format;
        out = out.replace(&token, &format_number(val, fmt));
    }
    Some(out)
}

/// Format a counter per an HWP number-format code (spec table 43); the sample-active set
/// is DIGIT, with Hangul/circled/roman supported for completeness.
fn format_number(n: u32, fmt: u8) -> String {
    const HANGUL: [char; 14] =
        ['가', '나', '다', '라', '마', '바', '사', '아', '자', '차', '카', '타', '파', '하'];
    const JAMO: [char; 14] =
        ['ㄱ', 'ㄴ', 'ㄷ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅅ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'];
    let cyc = |arr: &[char; 14]| arr[((n.max(1) - 1) % 14) as usize].to_string();
    match fmt {
        1 if (1..=20).contains(&n) => char::from_u32(0x245F + n).map(String::from).unwrap_or_default(), // ①..⑳
        3 => roman_small(n),
        8 => cyc(&HANGUL),
        9 if (1..=14).contains(&n) => char::from_u32(0x326E + n - 1).map(String::from).unwrap_or_else(|| cyc(&HANGUL)), // ㉮..
        10 => cyc(&JAMO),
        _ => n.to_string(), // DIGIT (0) and unsupported
    }
}

fn roman_small(mut n: u32) -> String {
    const VALS: [(u32, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
        (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut s = String::new();
    for (v, sym) in VALS {
        while n >= v {
            s.push_str(sym);
            n -= v;
        }
    }
    s
}

fn align_of(a: kdsnr_hwp_parser::model::style::Alignment) -> Align {
    use kdsnr_hwp_parser::model::style::Alignment;
    match a {
        Alignment::Left => Align::Left,
        Alignment::Right => Align::Right,
        Alignment::Center => Align::Center,
        Alignment::Distribute => Align::Distribute,
        Alignment::Split => Align::Justify,
        Alignment::Justify => Align::Justify,
    }
}
