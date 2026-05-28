//! `<hp:tbl>` 표 직렬화.
//!
//! Stage 3 (#182): `Control::Table` IR → `<hp:tbl>` + `<hp:tr>` + `<hp:tc>` + `<hp:subList>` + 문단 재귀.
//!
//! 속성·자식 순서는 한컴 OWPML 공식 (hancom-io/hwpx-owpml-model, Apache 2.0)
//! `Class/Para/TableType.cpp` 의 `WriteElement()`, `InitMap()` 기준:
//!
//! ### `<hp:tbl>` 속성 순서 (부모 AbstractShapeObjectType + 자신)
//! id, zOrder, numberingType, textWrap, textFlow, lock, dropcapstyle,
//! pageBreak, repeatHeader, rowCnt, colCnt, cellSpacing, borderFillIDRef, noAdjust
//!
//! ### `<hp:tbl>` 자식 순서
//! sz, pos, outMargin, (caption, shapeComment, parameterset, metaTag — 옵셔널),
//! inMargin, (cellzoneList — 옵셔널), tr (루프), (label — 옵셔널)
//!
//! ### `<hp:tc>` 속성 순서
//! name, header, hasMargin, protect, editable, dirty, borderFillIDRef
//!
//! ### `<hp:tc>` 자식 순서
//! subList, cellAddr, cellSpan, cellSz, cellMargin
//!
//! ## 중요: table.attr 비트 연산 금지
//!
//! HWPX에서 `table.attr` 는 0인 경우가 많으므로 비트 연산으로 `textWrap/textFlow/pageBreak` 등을
//! 추출하면 안 된다. 반드시 `table.common.text_wrap`, `table.page_break` 등 파싱된 IR 필드를 사용.

use std::io::Write;

use quick_xml::Writer;

use crate::model::control::Control;
use crate::model::paragraph::Paragraph;
use crate::model::shape::{
    Caption, CaptionDirection, CommonObjAttr, HorzAlign, HorzRelTo, TextWrap, VertAlign, VertRelTo,
};
use crate::model::table::{Cell, Table, TableLabel, TablePageBreak, VerticalAlign};

use super::context::SerializeContext;
use super::utils::{empty_tag, end_tag, paragraph_hwpx_id, start_tag, start_tag_attrs};
use super::SerializeError;

/// `<hp:tbl>` 직렬화.
pub fn write_table<W: Write>(
    w: &mut Writer<W>,
    table: &Table,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    // borderFillIDRef 참조 등록 (assert_all_refs_resolved 검증 대상)
    ctx.border_fill_ids.reference(table.border_fill_id);
    for zone in &table.zones {
        ctx.border_fill_ids.reference(zone.border_fill_id);
    }
    for cell in &table.cells {
        ctx.border_fill_ids.reference(cell.border_fill_id);
    }

    // --- <hp:tbl> 시작 태그 + 속성 ---
    let id_str = table.common.instance_id.to_string();
    let z_order = table.common.z_order.to_string();
    let text_wrap = text_wrap_str(table.common.text_wrap);
    let text_flow = text_flow_str(table.common.text_wrap);
    let lock = bool01(false);
    let page_break = table_page_break_str(table.page_break);
    let repeat_header = bool01(table.repeat_header);
    let no_adjust = bool01(table.raw_table_record_attr & 0x0000_0008 != 0);
    let row_cnt = table.row_count.to_string();
    let col_cnt = table.col_count.to_string();
    let cell_spacing = table.cell_spacing.to_string();
    let border_fill_id_ref = hwpx_border_fill_ref(table.border_fill_id, ctx).to_string();

    start_tag_attrs(
        w,
        "hp:tbl",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", "TABLE"),
            ("textWrap", text_wrap),
            ("textFlow", text_flow),
            ("lock", lock),
            ("dropcapstyle", "None"),
            ("pageBreak", page_break),
            ("repeatHeader", repeat_header),
            ("rowCnt", &row_cnt),
            ("colCnt", &col_cnt),
            ("cellSpacing", &cell_spacing),
            ("borderFillIDRef", &border_fill_id_ref),
            ("noAdjust", no_adjust),
        ],
    )?;

    // --- 자식: sz, pos, outMargin, inMargin, tr[] ---
    write_sz(w, &table.common, table.common.attr & 0x0010_0000 != 0)?;
    write_pos(w, &table.common)?;
    write_out_margin(w, table)?;
    if let Some(caption) = table.caption.as_ref() {
        write_caption(w, caption, ctx)?;
    }
    write_in_margin(w, table)?;

    // tr[]: 행 단위 반복. 각 행에 속한 셀 (cell.row == r) 을 col 오름차순으로 출력.
    for row_idx in 0..table.row_count {
        start_tag(w, "hp:tr")?;
        let mut row_cells: Vec<&Cell> = table.cells.iter().filter(|c| c.row == row_idx).collect();
        row_cells.sort_by_key(|c| c.col);
        for cell in row_cells {
            write_cell(w, cell, ctx)?;
        }
        end_tag(w, "hp:tr")?;
    }
    if let Some(label) = table.label.as_ref() {
        write_label(w, label)?;
    }

    end_tag(w, "hp:tbl")?;
    Ok(())
}

fn write_label<W: Write>(w: &mut Writer<W>, label: &TableLabel) -> Result<(), SerializeError> {
    let top_margin = label.top_margin.to_string();
    let left_margin = label.left_margin.to_string();
    let box_width = label.box_width.to_string();
    let box_length = label.box_length.to_string();
    let box_margin_hor = label.box_margin_hor.to_string();
    let box_margin_ver = label.box_margin_ver.to_string();
    let label_cols = label.label_cols.to_string();
    let label_rows = label.label_rows.to_string();
    let page_width = label.page_width.to_string();
    let page_height = label.page_height.to_string();
    empty_tag(
        w,
        "hp:label",
        &[
            ("topmargin", &top_margin),
            ("leftmargin", &left_margin),
            ("boxwidth", &box_width),
            ("boxlength", &box_length),
            ("boxmarginhor", &box_margin_hor),
            ("boxmarginver", &box_margin_ver),
            ("labelcols", &label_cols),
            ("labelrows", &label_rows),
            ("landscape", &label.landscape),
            ("pagewidth", &page_width),
            ("pageheight", &page_height),
        ],
    )
}

fn write_caption<W: Write>(
    w: &mut Writer<W>,
    caption: &Caption,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let side = match caption.direction {
        CaptionDirection::Left => "LEFT",
        CaptionDirection::Right => "RIGHT",
        CaptionDirection::Top => "TOP",
        CaptionDirection::Bottom => "BOTTOM",
    };
    let full_sz = bool01(caption.include_margin);
    let width = caption.width.to_string();
    let gap = caption.spacing.to_string();
    let last_width = caption.max_width.to_string();
    start_tag_attrs(
        w,
        "hp:caption",
        &[
            ("side", side),
            ("fullSz", full_sz),
            ("width", &width),
            ("gap", &gap),
            ("lastWidth", &last_width),
        ],
    )?;
    start_tag_attrs(
        w,
        "hp:subList",
        &[
            ("id", ""),
            ("textDirection", "HORIZONTAL"),
            ("lineWrap", "BREAK"),
            ("vertAlign", "TOP"),
            ("linkListIDRef", "0"),
            ("linkListNextIDRef", "0"),
            ("textWidth", "0"),
            ("textHeight", "0"),
            ("hasTextRef", "0"),
            ("hasNumRef", "0"),
        ],
    )?;
    for (pi, para) in caption.paragraphs.iter().enumerate() {
        write_caption_paragraph(w, pi, para, ctx)?;
    }
    end_tag(w, "hp:subList")?;
    end_tag(w, "hp:caption")
}

fn write_caption_paragraph<W: Write>(
    w: &mut Writer<W>,
    _pi: usize,
    para: &Paragraph,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    use crate::model::paragraph::ColumnBreakType;
    ctx.para_shape_ids.reference(para.para_shape_id);
    ctx.style_ids.reference(para.style_id as u16);
    if let Some(cs_ref) = para.char_shapes.first() {
        ctx.char_shape_ids.reference(cs_ref.char_shape_id);
    }
    let pi_str = paragraph_hwpx_id(para).to_string();
    let ppr = para.para_shape_id.to_string();
    let sp = para.style_id.to_string();
    let page_break = if matches!(para.column_type, ColumnBreakType::Page) {
        "1"
    } else {
        "0"
    };
    let column_break = if matches!(para.column_type, ColumnBreakType::Column) {
        "1"
    } else {
        "0"
    };
    start_tag_attrs(
        w,
        "hp:p",
        &[
            ("id", &pi_str),
            ("paraPrIDRef", &ppr),
            ("styleIDRef", &sp),
            ("pageBreak", page_break),
            ("columnBreak", column_break),
            ("merged", "0"),
        ],
    )?;
    let runs_xml = super::section::render_runs_split_by_char_shapes_with_markpen(para, ctx);
    use std::io::Write as _;
    w.get_mut()
        .write_all(runs_xml.as_bytes())
        .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    start_tag(w, "hp:linesegarray")?;
    if para.line_segs.is_empty() {
        empty_tag(
            w,
            "hp:lineseg",
            &[
                ("textpos", "0"),
                ("vertpos", "0"),
                ("vertsize", "1000"),
                ("textheight", "1000"),
                ("baseline", "850"),
                ("spacing", "600"),
                ("horzpos", "0"),
                ("horzsize", "0"),
                ("flags", "393216"),
            ],
        )?;
    } else {
        for seg in &para.line_segs {
            let text_start_s = seg.text_start.to_string();
            let vpos_s = seg.vertical_pos.to_string();
            let lh_s = seg.line_height.to_string();
            let th_s = seg.text_height.to_string();
            let bl_s = seg.baseline_distance.to_string();
            let ls_s = seg.line_spacing.to_string();
            let col_start_s = seg.column_start.to_string();
            let seg_w_s = seg.segment_width.to_string();
            let flags_s = seg.tag.to_string();
            empty_tag(
                w,
                "hp:lineseg",
                &[
                    ("textpos", &text_start_s),
                    ("vertpos", &vpos_s),
                    ("vertsize", &lh_s),
                    ("textheight", &th_s),
                    ("baseline", &bl_s),
                    ("spacing", &ls_s),
                    ("horzpos", &col_start_s),
                    ("horzsize", &seg_w_s),
                    ("flags", &flags_s),
                ],
            )?;
        }
    }
    end_tag(w, "hp:linesegarray")?;
    end_tag(w, "hp:p")
}

fn write_sz<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
    protect: bool,
) -> Result<(), SerializeError> {
    let width = c.width.to_string();
    let height = c.height.to_string();
    let protect = bool01(protect);
    empty_tag(
        w,
        "hp:sz",
        &[
            ("width", &width),
            ("widthRelTo", "ABSOLUTE"),
            ("height", &height),
            ("heightRelTo", "ABSOLUTE"),
            ("protect", protect),
        ],
    )
}

fn write_pos<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let treat = bool01(c.treat_as_char);
    let vert_offset = c.vertical_offset.to_string();
    let horz_offset = c.horizontal_offset.to_string();
    empty_tag(
        w,
        "hp:pos",
        &[
            ("treatAsChar", treat),
            ("affectLSpacing", "0"),
            ("flowWithText", "1"),
            ("allowOverlap", "0"),
            ("holdAnchorAndSO", "0"),
            ("vertRelTo", vert_rel_to_str(c.vert_rel_to)),
            ("horzRelTo", horz_rel_to_str(c.horz_rel_to)),
            ("vertAlign", vert_align_str(c.vert_align)),
            ("horzAlign", horz_align_str(c.horz_align)),
            ("vertOffset", &vert_offset),
            ("horzOffset", &horz_offset),
        ],
    )
}

fn write_out_margin<W: Write>(w: &mut Writer<W>, t: &Table) -> Result<(), SerializeError> {
    let left = t.outer_margin_left.to_string();
    let right = t.outer_margin_right.to_string();
    let top = t.outer_margin_top.to_string();
    let bottom = t.outer_margin_bottom.to_string();
    empty_tag(
        w,
        "hp:outMargin",
        &[
            ("left", &left),
            ("right", &right),
            ("top", &top),
            ("bottom", &bottom),
        ],
    )
}

fn write_in_margin<W: Write>(w: &mut Writer<W>, t: &Table) -> Result<(), SerializeError> {
    let left = t.padding.left.to_string();
    let right = t.padding.right.to_string();
    let top = t.padding.top.to_string();
    let bottom = t.padding.bottom.to_string();
    empty_tag(
        w,
        "hp:inMargin",
        &[
            ("left", &left),
            ("right", &right),
            ("top", &top),
            ("bottom", &bottom),
        ],
    )
}

fn write_cell<W: Write>(
    w: &mut Writer<W>,
    cell: &Cell,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let name = cell.field_name.as_deref().unwrap_or("");
    let header = bool01(cell.is_header);
    let has_margin = bool01(cell.apply_inner_margin);
    let border_ref = hwpx_border_fill_ref(cell.border_fill_id, ctx).to_string();

    start_tag_attrs(
        w,
        "hp:tc",
        &[
            ("name", name),
            ("header", header),
            ("hasMargin", has_margin),
            ("protect", "0"),
            ("editable", "0"),
            ("dirty", "0"),
            ("borderFillIDRef", &border_ref),
        ],
    )?;

    // 자식 순서: subList, cellAddr, cellSpan, cellSz, cellMargin
    write_sub_list(w, cell, ctx)?;
    write_cell_addr(w, cell)?;
    write_cell_span(w, cell)?;
    write_cell_sz(w, cell)?;
    write_cell_margin(w, cell)?;

    end_tag(w, "hp:tc")?;
    Ok(())
}

fn write_sub_list<W: Write>(
    w: &mut Writer<W>,
    cell: &Cell,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    start_tag_attrs(
        w,
        "hp:subList",
        &[
            ("id", ""),
            (
                "textDirection",
                if cell.text_direction == 1 {
                    "VERTICAL"
                } else {
                    "HORIZONTAL"
                },
            ),
            ("lineWrap", "BREAK"),
            ("vertAlign", cell_vert_align_str(cell.vertical_align)),
            ("linkListIDRef", "0"),
            ("linkListNextIDRef", "0"),
            ("textWidth", "0"),
            ("textHeight", "0"),
            ("hasTextRef", "0"),
            ("hasNumRef", "0"),
        ],
    )?;

    // 셀 내부 문단 재귀 — 각 문단은 간단한 <hp:p><hp:run><hp:t>텍스트</hp:t></hp:run></hp:p> 구조
    for (_pi, para) in cell.paragraphs.iter().enumerate() {
        ctx.para_shape_ids.reference(para.para_shape_id);
        ctx.style_ids.reference(para.style_id as u16);
        if let Some(cs_ref) = para.char_shapes.first() {
            ctx.char_shape_ids.reference(cs_ref.char_shape_id);
        }

        let pi_str = paragraph_hwpx_id(para).to_string();
        let ppr = para.para_shape_id.to_string();
        let sp = para.style_id.to_string();
        // Derive page/column break from IR `column_type` (matches section.rs
        // top-level paragraph emission). Hardcoding "0" loses page breaks
        // inside cells when the source author actually wanted one.
        use crate::model::paragraph::ColumnBreakType;
        let page_break = if matches!(para.column_type, ColumnBreakType::Page) {
            "1"
        } else {
            "0"
        };
        let column_break = if matches!(para.column_type, ColumnBreakType::Column) {
            "1"
        } else {
            "0"
        };
        start_tag_attrs(
            w,
            "hp:p",
            &[
                ("id", &pi_str),
                ("paraPrIDRef", &ppr),
                ("styleIDRef", &sp),
                ("pageBreak", page_break),
                ("columnBreak", column_break),
                ("merged", "0"),
            ],
        )?;

        // Emit one or more <hp:run> elements split at every char_shape
        // transition. The binary HWP stores per-character-range char_shapes
        // (CharShapeRef.start_pos in UTF-16 units); a single-run emission
        // collapses the whole paragraph onto the first char_shape and loses
        // every inline style (underline, bold, color span, etc.). Hancom-saved
        // .hwpx files always emit one <hp:run> per char_shape range.
        let runs_xml = super::section::render_runs_split_by_char_shapes_with_markpen(para, ctx);
        use std::io::Write as _;
        w.get_mut()
            .write_all(runs_xml.as_bytes())
            .map_err(|e| SerializeError::XmlError(e.to_string()))?;

        // <hp:linesegarray> — emit binary IR linesegs if present, otherwise
        // fall back to a single seed lineseg sized to the cell.
        //
        // IMPORTANT: do not rewrite per-line horzsize to the cell width when
        // real HWP lineSeg records exist. In table cells, Hanword uses
        // lineSeg.horzpos/horzsize as the resolved layout cache: floating
        // pictures, row-local indents, and short visual lines may each have a
        // different width. Pairing original textpos/vpos with a rewritten
        // full-cell horzsize destroys that cache and causes stretched cell
        // text/wrong wrapping after HWP -> HWPX conversion.
        let cell_content_width =
            (cell.width as i32 - cell.padding.left as i32 - cell.padding.right as i32).max(0)
                as u32;
        let cw_s = cell_content_width.to_string();
        start_tag(w, "hp:linesegarray")?;
        if para.line_segs.is_empty() {
            empty_tag(
                w,
                "hp:lineseg",
                &[
                    ("textpos", "0"),
                    ("vertpos", "0"),
                    ("vertsize", "1000"),
                    ("textheight", "1000"),
                    ("baseline", "850"),
                    ("spacing", "600"),
                    ("horzpos", "0"),
                    ("horzsize", &cw_s),
                    ("flags", "393216"),
                ],
            )?;
        } else {
            // Binary IR 의 multi-line lineseg 를 **모두 emit** 한다. 과거에는 first
            // seed 1 개만 emit 하고 Hanword 12+ 가 자체 reflow 하길 기대했으나, 그
            // 정책은 rhwp 자신이 다시 read 할 때 (line_height>0 이라) reflow trigger
            // 가 안 돼 1 줄 cram 결함을 만들어 냈다 (sci Q20 ㄷ 항목 등). 한컴
            // viewer 도 multi-line cache 를 정상 처리한다는 점은 GT PDF 로 검증됨.
            //
            // Each cached line's horzpos/horzsize belongs with its textpos.
            // Preserve it unless the source record has no usable width.
            for seg in &para.line_segs {
                let ts_s = super::section::hwpx_lineseg_textpos(para, seg.text_start).to_string();
                let vpos_s = seg.vertical_pos.to_string();
                let lh_s = seg.line_height.to_string();
                let th_s = seg.text_height.to_string();
                let bl_s = seg.baseline_distance.to_string();
                let ls_s = seg.line_spacing.to_string();
                let col_start_s = seg.column_start.to_string();
                let seg_width = if paragraph_has_floating_top_bottom_picture(para) {
                    0
                } else {
                    seg.segment_width
                };
                let seg_width_s = seg_width.to_string();
                let flags_s = seg.tag.to_string();
                empty_tag(
                    w,
                    "hp:lineseg",
                    &[
                        ("textpos", &ts_s),
                        ("vertpos", &vpos_s),
                        ("vertsize", &lh_s),
                        ("textheight", &th_s),
                        ("baseline", &bl_s),
                        ("spacing", &ls_s),
                        ("horzpos", &col_start_s),
                        ("horzsize", &seg_width_s),
                        ("flags", &flags_s),
                    ],
                )?;
            }
        }
        end_tag(w, "hp:linesegarray")?;

        end_tag(w, "hp:p")?;
    }

    end_tag(w, "hp:subList")?;
    Ok(())
}

fn paragraph_has_floating_top_bottom_picture(para: &Paragraph) -> bool {
    para.text.is_empty()
        && para.controls.iter().any(|control| match control {
            Control::Picture(pic) => {
                !pic.common.treat_as_char && pic.common.text_wrap == TextWrap::TopAndBottom
            }
            _ => false,
        })
}

fn write_cell_text<W: Write>(w: &mut Writer<W>, text: &str) -> Result<(), SerializeError> {
    use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
    // <hp:t>text</hp:t>
    w.write_event(Event::Start(BytesStart::new("hp:t")))
        .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    if !text.is_empty() {
        w.write_event(Event::Text(BytesText::new(text)))
            .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    }
    w.write_event(Event::End(BytesEnd::new("hp:t")))
        .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    Ok(())
}

fn write_cell_addr<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let col = cell.col.to_string();
    let row = cell.row.to_string();
    empty_tag(w, "hp:cellAddr", &[("colAddr", &col), ("rowAddr", &row)])
}

fn write_cell_span<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let cs = cell.col_span.max(1).to_string();
    let rs = cell.row_span.max(1).to_string();
    empty_tag(w, "hp:cellSpan", &[("colSpan", &cs), ("rowSpan", &rs)])
}

fn write_cell_sz<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let w_s = cell.width.to_string();
    let h_s = cell.height.to_string();
    empty_tag(w, "hp:cellSz", &[("width", &w_s), ("height", &h_s)])
}

fn write_cell_margin<W: Write>(w: &mut Writer<W>, cell: &Cell) -> Result<(), SerializeError> {
    let l = signed_hwp_unit_string(cell.padding.left);
    let r = signed_hwp_unit_string(cell.padding.right);
    let t = signed_hwp_unit_string(cell.padding.top);
    let b = signed_hwp_unit_string(cell.padding.bottom);
    empty_tag(
        w,
        "hp:cellMargin",
        &[("left", &l), ("right", &r), ("top", &t), ("bottom", &b)],
    )
}

fn signed_hwp_unit_string(v: i16) -> String {
    if v < 0 {
        ((v as i32) as u32).to_string()
    } else {
        v.to_string()
    }
}

// ---------- enum 변환 헬퍼 ----------

fn bool01(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

fn text_wrap_str(w: TextWrap) -> &'static str {
    use TextWrap::*;
    match w {
        Square => "SQUARE",
        Tight => "TIGHT",
        Through => "THROUGH",
        TopAndBottom => "TOP_AND_BOTTOM",
        BehindText => "BEHIND_TEXT",
        InFrontOfText => "IN_FRONT_OF_TEXT",
    }
}

/// textFlow: TextWrap 에 따라 결정 (한컴 관찰값 기준).
fn text_flow_str(w: TextWrap) -> &'static str {
    use TextWrap::*;
    match w {
        Square | Tight | Through => "BOTH_SIDES",
        _ => "BOTH_SIDES",
    }
}

fn table_page_break_str(pb: TablePageBreak) -> &'static str {
    use TablePageBreak::*;
    match pb {
        None => "NONE",
        CellBreak => "TABLE",
        RowBreak => "CELL",
    }
}

fn hwpx_border_fill_ref(id: u16, _ctx: &SerializeContext) -> u16 {
    // borderFillIDRef is the raw 1-based id, matching the 1-based ids the
    // borderFills list now writes (no index-0 drop, no shift).
    id
}

fn vert_rel_to_str(v: VertRelTo) -> &'static str {
    use VertRelTo::*;
    match v {
        Paper => "PAPER",
        Page => "PAGE",
        Para => "PARA",
    }
}

fn horz_rel_to_str(h: HorzRelTo) -> &'static str {
    use HorzRelTo::*;
    match h {
        Paper => "PAPER",
        Page => "PAGE",
        Column => "COLUMN",
        Para => "PARA",
    }
}

fn vert_align_str(v: VertAlign) -> &'static str {
    use VertAlign::*;
    match v {
        Top => "TOP",
        Center => "CENTER",
        Bottom => "BOTTOM",
        Inside => "INSIDE",
        Outside => "OUTSIDE",
    }
}

fn horz_align_str(h: HorzAlign) -> &'static str {
    use HorzAlign::*;
    match h {
        Left => "LEFT",
        Center => "CENTER",
        Right => "RIGHT",
        Inside => "INSIDE",
        Outside => "OUTSIDE",
    }
}

fn cell_vert_align_str(v: VerticalAlign) -> &'static str {
    use VerticalAlign::*;
    match v {
        Top => "TOP",
        Center => "CENTER",
        Bottom => "BOTTOM",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::document::Document;
    use crate::model::paragraph::Paragraph;
    use crate::model::table::{Cell, Table};
    use crate::serializer::hwpx::context::SerializeContext;

    fn empty_table(rows: u16, cols: u16) -> Table {
        let mut t = Table::default();
        t.row_count = rows;
        t.col_count = cols;
        for r in 0..rows {
            for c in 0..cols {
                let mut cell = Cell::default();
                cell.col = c;
                cell.row = r;
                cell.col_span = 1;
                cell.row_span = 1;
                cell.width = 1000;
                cell.height = 300;
                cell.paragraphs.push(Paragraph::default());
                t.cells.push(cell);
            }
        }
        t.rebuild_grid();
        t
    }

    fn serialize(table: &Table) -> String {
        let doc = Document::default();
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        write_table(&mut w, table, &mut ctx).expect("write_table");
        String::from_utf8(w.into_inner()).unwrap()
    }

    #[test]
    fn tbl_root_attrs_in_canonical_order() {
        let t = empty_table(2, 3);
        let xml = serialize(&t);
        assert!(xml.contains("<hp:tbl "), "should emit <hp:tbl>: {}", xml);
        // id → zOrder → numberingType → textWrap → textFlow → lock → dropcapstyle →
        // pageBreak → repeatHeader → rowCnt → colCnt → cellSpacing → borderFillIDRef → noAdjust
        let ip = xml.find("id=").unwrap();
        let zp = xml.find("zOrder=").unwrap();
        let nt = xml.find("numberingType=").unwrap();
        let tw = xml.find("textWrap=").unwrap();
        let tf = xml.find("textFlow=").unwrap();
        let rc = xml.find("rowCnt=").unwrap();
        let cc = xml.find("colCnt=").unwrap();
        let bf = xml.find("borderFillIDRef=").unwrap();
        let na = xml.find("noAdjust=").unwrap();
        assert!(
            ip < zp && zp < nt && nt < tw && tw < tf && tf < rc && rc < cc && cc < bf && bf < na
        );
    }

    #[test]
    fn tr_count_matches_row_count() {
        let t = empty_table(4, 2);
        let xml = serialize(&t);
        assert_eq!(xml.matches("<hp:tr>").count(), 4);
    }

    #[test]
    fn tc_count_matches_cell_count() {
        let t = empty_table(2, 3);
        let xml = serialize(&t);
        assert_eq!(xml.matches("<hp:tc ").count(), 6);
    }

    #[test]
    fn cells_have_canonical_child_order() {
        let t = empty_table(1, 1);
        let xml = serialize(&t);
        // subList → cellAddr → cellSpan → cellSz → cellMargin
        let sl = xml.find("<hp:subList ").unwrap();
        let ca = xml.find("<hp:cellAddr ").unwrap();
        let cs = xml.find("<hp:cellSpan ").unwrap();
        let cz = xml.find("<hp:cellSz ").unwrap();
        let cm = xml.find("<hp:cellMargin ").unwrap();
        assert!(sl < ca && ca < cs && cs < cz && cz < cm);
    }

    #[test]
    fn cell_addr_reflects_coordinates() {
        let t = empty_table(2, 2);
        let xml = serialize(&t);
        assert!(xml.contains(r#"<hp:cellAddr colAddr="0" rowAddr="0"/>"#));
        assert!(xml.contains(r#"<hp:cellAddr colAddr="1" rowAddr="0"/>"#));
        assert!(xml.contains(r#"<hp:cellAddr colAddr="0" rowAddr="1"/>"#));
        assert!(xml.contains(r#"<hp:cellAddr colAddr="1" rowAddr="1"/>"#));
    }

    #[test]
    fn cell_span_defaults_to_one() {
        let t = empty_table(1, 1);
        let xml = serialize(&t);
        assert!(xml.contains(r#"<hp:cellSpan colSpan="1" rowSpan="1"/>"#));
    }

    #[test]
    fn border_fill_id_ref_registered_in_ctx() {
        let doc = Document::default();
        let mut ctx = SerializeContext::collect_from_document(&doc);
        let mut t = empty_table(1, 1);
        t.border_fill_id = 99;
        t.cells[0].border_fill_id = 99;
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        write_table(&mut w, &t, &mut ctx).unwrap();
        // 99 는 등록되지 않은 borderFill → unresolved
        assert!(ctx.border_fill_ids.unresolved().contains(&99u16));
    }
}
