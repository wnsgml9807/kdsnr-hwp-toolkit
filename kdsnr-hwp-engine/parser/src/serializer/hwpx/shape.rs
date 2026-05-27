//! 그리기 개체 (도형) 직렬화 — Rectangle / Line / Container 뼈대.
//!
//! Stage 5 (#182): 대표 도형 3종(Rectangle, Line, Container)의 `<hp:rect>`, `<hp:line>`,
//! `<hp:container>` 요소 뼈대를 구현한다. 완전한 속성 커버리지는 별도 이슈로 이월.
//!
//! 속성·자식 순서는 한컴 OWPML 공식 (hancom-io/hwpx-owpml-model, Apache 2.0) 기준.
//!
//! ## 범위 한정
//!
//! - Stage 5 에서는 **도형 뼈대 출력** 기능만 제공 (section.rs dispatcher 연결은 #186).
//! - Arc / Polygon / Curve / Group 등은 향후 이슈에서 확장.
//! - DrawingObjAttr (선/채우기 세부 속성) 은 최소 기본값 출력.

#![allow(dead_code)]

use std::io::Write;

use quick_xml::Writer;

use crate::model::paragraph::Paragraph;
use crate::model::shape::{
    CommonObjAttr, DrawingObjAttr, GroupShape, HorzAlign, HorzRelTo, LineShape, PolygonShape,
    RectangleShape, ShapeComponentAttr, TextBox, TextWrap, VertAlign, VertRelTo,
};
use crate::model::style::FillType;
use crate::model::table::VerticalAlign;

use super::context::SerializeContext;
use super::utils::{empty_tag, end_tag, paragraph_hwpx_id, start_tag, start_tag_attrs, text};
use super::SerializeError;

// =====================================================================
// <hp:rect>
// =====================================================================

/// `<hp:rect>` 직렬화 진입점. Rectangle IR → XML.
///
/// Hanword 12+ refuses `<hp:rect>` that lacks the standard shape-component
/// children (lineShape, fillBrush). This emits the full skeleton observed
/// in Hancom-saved samples (offset / orgSz / curSz / flip / rotationInfo /
/// renderingInfo / lineShape / fillBrush) followed by the standard sz / pos
/// / outMargin trailer. Values are derived from `rect.drawing.border_line`
/// and `rect.drawing.fill` when present; otherwise sensible defaults
/// (black 1pt solid line, no fill) are used.
pub fn write_rect<W: Write>(
    w: &mut Writer<W>,
    rect: &RectangleShape,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let c = &rect.common;
    let id_str = c.instance_id.to_string();
    let group_level = rect.drawing.shape_attr.group_level;
    let is_group_child = group_level > 0 || (c.instance_id == 0 && rect.drawing.inst_id != 0);
    let inst_id = if rect.drawing.inst_id == 0 {
        c.instance_id
    } else {
        rect.drawing.inst_id
    }
    .to_string();
    let z_order = c.z_order.to_string();
    let tw = if is_group_child {
        "TOP_AND_BOTTOM"
    } else if c.treat_as_char && matches!(c.text_wrap, TextWrap::InFrontOfText) {
        "TOP_AND_BOTTOM"
    } else {
        text_wrap_str(c.text_wrap)
    };
    let group_level_s = if is_group_child {
        group_level.max(1).to_string()
    } else {
        "0".to_string()
    };
    let numbering_type = if c.attr & 0x0400_0000 != 0 {
        "PICTURE"
    } else {
        "NONE"
    };
    let ratio = rect.round_rate.to_string();

    start_tag_attrs(
        w,
        "hp:rect",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", numbering_type),
            ("textWrap", tw),
            ("textFlow", "BOTH_SIDES"),
            ("lock", "0"),
            ("dropcapstyle", "None"),
            ("href", ""),
            ("groupLevel", &group_level_s),
            ("instid", &inst_id),
            ("ratio", &ratio),
        ],
    )?;

    // Order observed in Hancom-saved samples:
    //   offset, orgSz, curSz, flip, rotationInfo, renderingInfo,
    //   lineShape, fillBrush, [shadow], sz, pos, outMargin
    write_offset_xy(w, &rect.drawing.shape_attr)?;
    if is_group_child {
        write_org_zero_curr_sizes(w, &rect.drawing.shape_attr)?;
    } else {
        write_org_curr_sizes(w, c, &rect.drawing.shape_attr)?;
    }
    write_flip(w)?;
    write_rotation_info_from_shape_attr(
        w,
        c,
        &rect.drawing.shape_attr,
        rect.drawing.shape_attr.flip & 0x0008_0000 != 0,
    )?;
    write_rendering_info(w, &rect.drawing.shape_attr)?;
    write_line_shape(w, &rect.drawing.border_line)?;
    if rect.drawing.fill.fill_type != FillType::None {
        write_fill_brush(w, &rect.drawing.fill)?;
    }
    write_shadow(w, &rect.drawing)?;
    // hp:drawText — inner text content, if the rect carries a text_box. The
    // (가)/(나) labels in social fillblank rectangles live here. Hancom
    // sample order: shadow → drawText → hc:pt0..3.
    if let Some(tb) = rect.drawing.text_box.as_ref() {
        write_draw_text(
            w,
            tb,
            draw_text_last_width(c, &rect.drawing.shape_attr),
            ctx,
        )?;
    }
    // hc:pt0..pt3 are local coordinates in the original shape coordinate
    // space. curSz/scale place that local rectangle into the current box.
    let ow = rect.drawing.shape_attr.original_width.max(1) as i32;
    let oh = rect.drawing.shape_attr.original_height.max(1) as i32;
    let xs = [0, ow, ow, 0];
    let ys = [0, 0, oh, oh];
    write_rect_corners(w, &xs, &ys)?;
    if !is_group_child {
        let protect_size = color_ref_hwpx(rect.drawing.border_line.color) == "#FF0000"
            && rect.drawing.border_line.width == 2;
        write_sz(w, c, protect_size)?;
        write_pos(w, c)?;
        write_out_margin(w, c)?;
        if !c.description.is_empty() {
            write_shape_comment_text(w, &c.description)?;
        } else if !ctx.in_master_page {
            write_shape_comment_text(w, "사각형입니다.")?;
        }
    }

    end_tag(w, "hp:rect")?;
    Ok(())
}

fn textbox_vert_align_str(v: VerticalAlign) -> &'static str {
    use VerticalAlign::*;
    match v {
        Top => "TOP",
        Center => "CENTER",
        Bottom => "BOTTOM",
    }
}

fn write_draw_text<W: Write>(
    w: &mut Writer<W>,
    tb: &TextBox,
    last_width_value: u32,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let last_width = last_width_value.to_string();
    start_tag_attrs(
        w,
        "hp:drawText",
        &[("lastWidth", &last_width), ("name", ""), ("editable", "0")],
    )?;

    start_tag_attrs(
        w,
        "hp:subList",
        &[
            ("id", ""),
            ("textDirection", "HORIZONTAL"),
            ("lineWrap", "BREAK"),
            ("vertAlign", textbox_vert_align_str(tb.vertical_align)),
            ("linkListIDRef", "0"),
            ("linkListNextIDRef", "0"),
            ("textWidth", "0"),
            ("textHeight", "0"),
            ("hasTextRef", "0"),
            ("hasNumRef", "0"),
        ],
    )?;

    for (pi, para) in tb.paragraphs.iter().enumerate() {
        write_inner_paragraph(w, pi, para, ctx)?;
    }

    end_tag(w, "hp:subList")?;

    let ml = tb.margin_left.to_string();
    let mr = tb.margin_right.to_string();
    let mt = tb.margin_top.to_string();
    let mb = tb.margin_bottom.to_string();
    empty_tag(
        w,
        "hp:textMargin",
        &[("left", &ml), ("right", &mr), ("top", &mt), ("bottom", &mb)],
    )?;

    end_tag(w, "hp:drawText")?;
    Ok(())
}

fn draw_text_last_width(common: &CommonObjAttr, shape_attr: &ShapeComponentAttr) -> u32 {
    if common.width != 0 {
        common.width
    } else if shape_attr.rotation_center.x > 0 {
        (shape_attr.rotation_center.x as u32).saturating_mul(2)
    } else {
        shape_attr.current_width.max(shape_attr.original_width)
    }
}

fn write_inner_paragraph<W: Write>(
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
    let runs_xml = super::section::render_runs_split_by_char_shapes(para, ctx)
        .replace("<hp:t/><hp:ctrl>", "<hp:ctrl>");
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
                ("horzsize", "12964"),
                ("flags", "393216"),
            ],
        )?;
    } else {
        for seg in &para.line_segs {
            let text_start_s =
                super::section::hwpx_lineseg_textpos(para, seg.text_start).to_string();
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
    end_tag(w, "hp:p")?;
    Ok(())
}

fn write_rect_corners<W: Write>(
    w: &mut Writer<W>,
    xs: &[i32; 4],
    ys: &[i32; 4],
) -> Result<(), SerializeError> {
    for i in 0..4 {
        let x = xs[i].to_string();
        let y = ys[i].to_string();
        let name = match i {
            0 => "hc:pt0",
            1 => "hc:pt1",
            2 => "hc:pt2",
            _ => "hc:pt3",
        };
        empty_tag(w, name, &[("x", &x), ("y", &y)])?;
    }
    Ok(())
}

// =====================================================================
// <hp:line>
// =====================================================================

/// `<hp:line>` 직렬화 진입점. LineShape IR → XML.
pub fn write_line<W: Write>(
    w: &mut Writer<W>,
    line: &LineShape,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx; // hp:line currently has no inner text emission
    let c = &line.common;
    let id_str = c.instance_id.to_string();
    let inst_id = if line.drawing.inst_id == 0 {
        c.instance_id
    } else {
        line.drawing.inst_id
    }
    .to_string();
    let z_order = c.z_order.to_string();
    let tw = text_wrap_str(c.text_wrap);
    let srb = bool01(line.started_right_or_bottom);

    let numbering_type = if c.attr & 0x0400_0000 != 0 {
        "PICTURE"
    } else {
        "NONE"
    };

    start_tag_attrs(
        w,
        "hp:line",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", numbering_type),
            ("textWrap", tw),
            ("textFlow", "BOTH_SIDES"),
            ("lock", "0"),
            ("dropcapstyle", "None"),
            ("href", ""),
            ("groupLevel", "0"),
            ("instid", &inst_id),
            ("isReverseHV", srb),
        ],
    )?;

    // Order observed in Hancom-saved samples (mirrors write_rect):
    //   offset, orgSz, curSz, flip, rotationInfo, renderingInfo,
    //   lineShape, fillBrush, shadow, startPt, endPt, sz, pos, outMargin
    write_offset_xy(w, &line.drawing.shape_attr)?;
    write_line_org_curr_sizes(w, line)?;
    write_flip(w)?;
    write_rotation_info(w, c, c.attr & 0x0400_0000 != 0)?;
    write_rendering_info(w, &line.drawing.shape_attr)?;
    write_line_shape(w, &line.drawing.border_line)?;
    if line.drawing.fill.fill_type != FillType::None {
        write_fill_brush(w, &line.drawing.fill)?;
    }
    write_shadow(w, &line.drawing)?;
    write_line_points(w, line)?;
    write_sz(w, c, false)?;
    write_pos(w, c)?;
    write_out_margin(w, c)?;
    if !c.description.is_empty() {
        write_shape_comment_text(w, &c.description)?;
    }

    end_tag(w, "hp:line")?;
    Ok(())
}

// =====================================================================
// <hp:polygon>
// =====================================================================

/// `<hp:polygon>` 직렬화 진입점. PolygonShape IR -> XML.
pub fn write_polygon<W: Write>(
    w: &mut Writer<W>,
    poly: &PolygonShape,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx;
    let c = &poly.common;
    let id_str = c.instance_id.to_string();
    let inst_id = if poly.drawing.inst_id == 0 {
        c.instance_id
    } else {
        poly.drawing.inst_id
    }
    .to_string();
    let z_order = c.z_order.to_string();
    let tw = text_wrap_str(c.text_wrap);

    start_tag_attrs(
        w,
        "hp:polygon",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", "NONE"),
            ("textWrap", tw),
            ("textFlow", "BOTH_SIDES"),
            ("lock", "0"),
            ("dropcapstyle", "None"),
            ("href", ""),
            ("groupLevel", "0"),
            ("instid", &inst_id),
        ],
    )?;

    write_offset_xy(w, &poly.drawing.shape_attr)?;
    write_org_curr_sizes(w, c, &poly.drawing.shape_attr)?;
    write_flip(w)?;
    write_rotation_info(w, c, false)?;
    write_rendering_info(w, &poly.drawing.shape_attr)?;
    write_polygon_line_shape(w, &poly.drawing.border_line)?;
    if poly.drawing.fill.fill_type != FillType::None {
        write_fill_brush(w, &poly.drawing.fill)?;
    }
    write_shadow(w, &poly.drawing)?;
    for pt in &poly.points {
        let x = pt.x.to_string();
        let y = pt.y.to_string();
        empty_tag(w, "hc:pt", &[("x", &x), ("y", &y)])?;
    }
    write_sz(w, c, false)?;
    write_pos(w, c)?;
    write_out_margin(w, c)?;

    end_tag(w, "hp:polygon")?;
    Ok(())
}

// =====================================================================
// Shared shape-component children (used by rect/line/ellipse/etc.)
// =====================================================================

fn write_offset_xy<W: Write>(
    w: &mut Writer<W>,
    shape_attr: &ShapeComponentAttr,
) -> Result<(), SerializeError> {
    let x = signed_hwp_u32(shape_attr.offset_x);
    let y = signed_hwp_u32(shape_attr.offset_y);
    empty_tag(w, "hp:offset", &[("x", &x), ("y", &y)])
}

fn signed_hwp_u32(value: i32) -> String {
    (value as u32).to_string()
}

fn write_org_curr_sizes<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
    shape_attr: &ShapeComponentAttr,
) -> Result<(), SerializeError> {
    let has_shape_component_size = shape_attr.original_width != 0
        || shape_attr.original_height != 0
        || shape_attr.current_width != 0
        || shape_attr.current_height != 0;
    let cur_width = if has_shape_component_size {
        shape_attr.current_width.max(1)
    } else {
        c.width
    };
    let cur_height = if has_shape_component_size {
        shape_attr.current_height.max(1)
    } else {
        c.height
    };
    let org_width = if has_shape_component_size {
        shape_attr.original_width.max(1)
    } else {
        c.width
    };
    let org_height = if has_shape_component_size {
        shape_attr.original_height.max(1)
    } else {
        c.height
    };
    let ow = org_width.to_string();
    let oh = org_height.to_string();
    let cw = cur_width.to_string();
    let ch = cur_height.to_string();
    empty_tag(w, "hp:orgSz", &[("width", &ow), ("height", &oh)])?;
    empty_tag(w, "hp:curSz", &[("width", &cw), ("height", &ch)])
}

fn write_org_zero_curr_sizes<W: Write>(
    w: &mut Writer<W>,
    shape_attr: &ShapeComponentAttr,
) -> Result<(), SerializeError> {
    let ow = shape_attr.original_width.max(1).to_string();
    let oh = shape_attr.original_height.max(1).to_string();
    empty_tag(w, "hp:orgSz", &[("width", &ow), ("height", &oh)])?;
    empty_tag(w, "hp:curSz", &[("width", "0"), ("height", "0")])
}

fn write_line_org_curr_sizes<W: Write>(
    w: &mut Writer<W>,
    line: &LineShape,
) -> Result<(), SerializeError> {
    let shape_attr = &line.drawing.shape_attr;
    let org_width = shape_attr.original_width.max(1).to_string();
    let org_height = shape_attr.original_height.max(1).to_string();
    let cur_width = if shape_attr.current_width == shape_attr.original_width
        && shape_attr.original_width == 100
        && line.common.width <= 1
    {
        0
    } else {
        shape_attr.current_width
    }
    .to_string();
    let cur_height = if shape_attr.current_height == shape_attr.original_height
        && shape_attr.original_height == 100
        && line.common.height <= 1
    {
        0
    } else {
        shape_attr.current_height
    }
    .to_string();
    empty_tag(
        w,
        "hp:orgSz",
        &[("width", &org_width), ("height", &org_height)],
    )?;
    empty_tag(
        w,
        "hp:curSz",
        &[("width", &cur_width), ("height", &cur_height)],
    )
}

fn write_line_points<W: Write>(w: &mut Writer<W>, line: &LineShape) -> Result<(), SerializeError> {
    let sx = line.start.x.to_string();
    let sy = line.start.y.to_string();
    let ex = line.end.x.to_string();
    let ey = line.end.y.to_string();
    empty_tag(w, "hc:startPt", &[("x", &sx), ("y", &sy)])?;
    empty_tag(w, "hc:endPt", &[("x", &ex), ("y", &ey)])
}

fn write_flip<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    empty_tag(w, "hp:flip", &[("horizontal", "0"), ("vertical", "0")])
}

fn write_rotation_info<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
    rotate_image: bool,
) -> Result<(), SerializeError> {
    let center_x = (c.width / 2).to_string();
    let center_y = (c.height / 2).to_string();
    let rotate_image = bool01(rotate_image);
    empty_tag(
        w,
        "hp:rotationInfo",
        &[
            ("angle", "0"),
            ("centerX", &center_x),
            ("centerY", &center_y),
            ("rotateimage", rotate_image),
        ],
    )
}

fn write_rotation_info_from_shape_attr<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
    shape_attr: &ShapeComponentAttr,
    rotate_image: bool,
) -> Result<(), SerializeError> {
    let center_x = if shape_attr.rotation_center.x != 0 || shape_attr.rotation_center.y != 0 {
        shape_attr.rotation_center.x.to_string()
    } else {
        (c.width / 2).to_string()
    };
    let center_y = if shape_attr.rotation_center.x != 0 || shape_attr.rotation_center.y != 0 {
        shape_attr.rotation_center.y.to_string()
    } else {
        (c.height / 2).to_string()
    };
    let rotate_image = bool01(rotate_image);
    empty_tag(
        w,
        "hp:rotationInfo",
        &[
            ("angle", "0"),
            ("centerX", &center_x),
            ("centerY", &center_y),
            ("rotateimage", rotate_image),
        ],
    )
}

fn write_rendering_info<W: Write>(
    w: &mut Writer<W>,
    shape_attr: &ShapeComponentAttr,
) -> Result<(), SerializeError> {
    start_tag(w, "hp:renderingInfo")?;
    if write_raw_rendering_matrices(w, shape_attr)? {
        end_tag(w, "hp:renderingInfo")?;
        return Ok(());
    }
    let (trans, scale, rotation) = rendering_matrices(shape_attr);
    write_matrix(w, "hc:transMatrix", trans)?;
    write_matrix(w, "hc:scaMatrix", scale)?;
    write_matrix(w, "hc:rotMatrix", rotation)?;
    end_tag(w, "hp:renderingInfo")?;
    Ok(())
}

fn write_raw_rendering_matrices<W: Write>(
    w: &mut Writer<W>,
    shape_attr: &ShapeComponentAttr,
) -> Result<bool, SerializeError> {
    let raw = &shape_attr.raw_rendering;
    if raw.len() < 2 + 48 {
        return Ok(false);
    }
    let cnt = u16::from_le_bytes([raw[0], raw[1]]) as usize;
    let Some(trans) = read_matrix6(raw, 2) else {
        return Ok(false);
    };
    write_matrix(w, "hc:transMatrix", trans)?;
    for i in 0..cnt {
        let scale_offset = 2 + 48 + i * 96;
        let rot_offset = scale_offset + 48;
        let Some(scale) = read_matrix6(raw, scale_offset) else {
            return Ok(true);
        };
        write_matrix(w, "hc:scaMatrix", scale)?;
        let Some(rotation) = read_matrix6(raw, rot_offset) else {
            return Ok(true);
        };
        write_matrix(w, "hc:rotMatrix", rotation)?;
    }
    Ok(true)
}

fn rendering_matrices(shape_attr: &ShapeComponentAttr) -> ([f64; 6], [f64; 6], [f64; 6]) {
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let raw = &shape_attr.raw_rendering;
    if raw.len() < 2 + 48 {
        return (
            [
                1.0,
                0.0,
                shape_attr.render_tx,
                0.0,
                1.0,
                shape_attr.render_ty,
            ],
            [
                shape_attr.render_sx,
                0.0,
                0.0,
                0.0,
                shape_attr.render_sy,
                0.0,
            ],
            identity,
        );
    }

    let cnt = u16::from_le_bytes([raw[0], raw[1]]) as usize;
    let Some(trans) = read_matrix6(raw, 2) else {
        return (identity, identity, identity);
    };
    if cnt == 0 {
        return (trans, identity, identity);
    }
    let scale_offset = 2 + 48;
    let rot_offset = scale_offset + 48;
    let scale = read_matrix6(raw, scale_offset).unwrap_or(identity);
    let rotation = read_matrix6(raw, rot_offset).unwrap_or(identity);
    (trans, scale, rotation)
}

fn read_matrix6(raw: &[u8], offset: usize) -> Option<[f64; 6]> {
    if raw.len() < offset + 48 {
        return None;
    }
    let mut out = [0.0; 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let start = offset + i * 8;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&raw[start..start + 8]);
        *slot = f64::from_le_bytes(bytes);
    }
    Some(out)
}

fn write_matrix<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    m: [f64; 6],
) -> Result<(), SerializeError> {
    let e1 = format_matrix_value(m[0]);
    let e2 = format_matrix_value(m[1]);
    let e3 = format_matrix_value(m[2]);
    let e4 = format_matrix_value(m[3]);
    let e5 = format_matrix_value(m[4]);
    let e6 = format_matrix_value(m[5]);
    empty_tag(
        w,
        name,
        &[
            ("e1", &e1),
            ("e2", &e2),
            ("e3", &e3),
            ("e4", &e4),
            ("e5", &e5),
            ("e6", &e6),
        ],
    )
}

fn format_matrix_value(v: f64) -> String {
    let v = (v as f32) as f64;
    if (v - v.round()).abs() < 0.0000005 {
        return (v.round() as i64).to_string();
    }
    let mut s = format!("{v:.6}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

fn color_ref_hwpx(color: u32) -> String {
    if color == 0xFFFFFFFF {
        "none".to_string()
    } else if color & 0xFF00_0000 != 0 {
        format!("#{color:08X}")
    } else {
        let r = color & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = (color >> 16) & 0xFF;
        format!("#{r:02X}{g:02X}{b:02X}")
    }
}

fn write_line_shape<W: Write>(
    w: &mut Writer<W>,
    bl: &crate::model::style::ShapeBorderLine,
) -> Result<(), SerializeError> {
    let color = color_ref_hwpx(bl.color);
    let line_type = bl.attr & 0x3F;
    // Width=0 → fall back only for visible lines. If line_type is NONE,
    // forcing a positive width plus SOLID makes hidden text-box borders appear.
    let width_val = if line_type > 0 && bl.width <= 0 {
        33
    } else {
        bl.width
    };
    let width = width_val.to_string();
    let style = if color == "#FF0000" && width_val == 2 {
        "NONE"
    } else {
        shape_line_style_str(line_type)
    };
    let outline = match bl.outline_style {
        1 => "OUTER",
        2 => "INNER",
        _ => "NORMAL",
    };
    let arrow_size = if bl.attr & 0xF000_0000 == 0xC000_0000 {
        "SMALL_SMALL"
    } else {
        "MEDIUM_MEDIUM"
    };
    empty_tag(
        w,
        "hp:lineShape",
        &[
            ("color", &color),
            ("width", &width),
            ("style", style),
            ("endCap", "FLAT"),
            ("headStyle", "NORMAL"),
            ("tailStyle", "NORMAL"),
            ("headfill", "1"),
            ("tailfill", "1"),
            ("headSz", arrow_size),
            ("tailSz", arrow_size),
            ("outlineStyle", outline),
            ("alpha", "0"),
        ],
    )
}

fn write_polygon_line_shape<W: Write>(
    w: &mut Writer<W>,
    bl: &crate::model::style::ShapeBorderLine,
) -> Result<(), SerializeError> {
    let color = color_ref_hwpx(bl.color);
    let style = shape_line_style_str(bl.attr & 0x3F);
    let outline = match bl.outline_style {
        1 => "OUTER",
        2 => "INNER",
        _ => "NORMAL",
    };
    empty_tag(
        w,
        "hp:lineShape",
        &[
            ("color", &color),
            ("width", "0"),
            ("style", style),
            ("endCap", "ROUND"),
            ("headStyle", "NORMAL"),
            ("tailStyle", "NORMAL"),
            ("headfill", "0"),
            ("tailfill", "0"),
            ("headSz", "SMALL_SMALL"),
            ("tailSz", "SMALL_SMALL"),
            ("outlineStyle", outline),
            ("alpha", "0"),
        ],
    )
}

fn shape_line_style_str(line_type: u32) -> &'static str {
    match line_type {
        0 => "NONE",
        1 => "SOLID",
        2 => "DASH",
        3 => "DOT",
        4 => "DASH_DOT",
        5 => "DASH_DOT_DOT",
        6 => "LONG_DASH",
        7 => "CIRCLE",
        8 => "DOUBLE_SLIM",
        9 => "SLIM_THICK",
        10 => "THICK_SLIM",
        11 => "SLIM_THICK_SLIM",
        _ => "SOLID",
    }
}

fn write_shadow<W: Write>(
    w: &mut Writer<W>,
    drawing: &crate::model::shape::DrawingObjAttr,
) -> Result<(), SerializeError> {
    let color = color_ref_hwpx(drawing.shadow_color);
    let ox = drawing.shadow_offset_x.to_string();
    let oy = drawing.shadow_offset_y.to_string();
    let alpha = drawing.shadow_alpha.to_string();
    // type=NONE when no shadow recorded; otherwise emit DROP-style.
    let type_str = if drawing.shadow_type == 0 {
        "NONE"
    } else {
        "DROP"
    };
    empty_tag(
        w,
        "hp:shadow",
        &[
            ("type", type_str),
            ("color", &color),
            ("offsetX", &ox),
            ("offsetY", &oy),
            ("alpha", &alpha),
        ],
    )
}

fn write_shape_comment<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    start_tag(w, "hp:shapeComment")?;
    end_tag(w, "hp:shapeComment")
}

fn write_shape_comment_text<W: Write>(
    w: &mut Writer<W>,
    comment: &str,
) -> Result<(), SerializeError> {
    start_tag(w, "hp:shapeComment")?;
    text(w, comment)?;
    end_tag(w, "hp:shapeComment")
}

fn write_fill_brush<W: Write>(
    w: &mut Writer<W>,
    fill: &crate::model::style::Fill,
) -> Result<(), SerializeError> {
    use crate::model::style::FillType;
    start_tag(w, "hc:fillBrush")?;
    match fill.fill_type {
        FillType::Solid => {
            let face = fill
                .solid
                .as_ref()
                .map(|s| color_ref_hwpx(s.background_color))
                .unwrap_or_else(|| "none".to_string());
            let hatch = fill
                .solid
                .as_ref()
                .map(|s| color_ref_hwpx(s.pattern_color))
                .unwrap_or_else(|| "none".to_string());
            empty_tag(
                w,
                "hc:winBrush",
                &[("faceColor", &face), ("hatchColor", &hatch), ("alpha", "0")],
            )?;
        }
        FillType::None => {
            empty_tag(
                w,
                "hc:winBrush",
                &[
                    ("faceColor", "none"),
                    ("hatchColor", "none"),
                    ("alpha", "0"),
                ],
            )?;
        }
        // Gradient/Image: leave empty until those sample-backed fill modes are
        // mapped explicitly.
        _ => {}
    }
    end_tag(w, "hc:fillBrush")?;
    Ok(())
}

// =====================================================================
// <hp:container> — 묶음 개체 (GroupShape). Stage 5 뼈대만.
// =====================================================================

/// `<hp:container>` 뼈대 — 내부 자식 도형 루프는 dispatcher에서 처리.
pub fn write_container_open<W: Write>(
    w: &mut Writer<W>,
    group: &GroupShape,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let common = &group.common;
    let id_str = common.instance_id.to_string();
    let z_order = common.z_order.to_string();
    let tw = if common.treat_as_char && matches!(common.text_wrap, TextWrap::InFrontOfText) {
        "TOP_AND_BOTTOM"
    } else {
        text_wrap_str(common.text_wrap)
    };
    let numbering_type = if ctx.in_master_page || common.attr & 0x0400_0000 != 0 {
        "PICTURE"
    } else {
        "NONE"
    };
    let inst_id = container_hwpx_instid(group).to_string();

    start_tag_attrs(
        w,
        "hp:container",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", numbering_type),
            ("textWrap", tw),
            ("textFlow", "BOTH_SIDES"),
            ("lock", "0"),
            ("dropcapstyle", "None"),
            ("href", ""),
            ("groupLevel", "0"),
            ("instid", &inst_id),
        ],
    )?;

    write_offset_xy(w, &group.shape_attr)?;
    write_container_org_curr_sizes(w, common)?;
    write_flip(w)?;
    write_rotation_info(w, common, true)?;
    write_rendering_info(w, &group.shape_attr)?;

    Ok(())
}

fn write_container_org_curr_sizes<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
) -> Result<(), SerializeError> {
    let ow = c.width.to_string();
    let oh = c.height.to_string();
    empty_tag(w, "hp:orgSz", &[("width", &ow), ("height", &oh)])?;
    empty_tag(w, "hp:curSz", &[("width", "0"), ("height", "0")])
}

pub fn write_container_close<W: Write>(
    w: &mut Writer<W>,
    group: &GroupShape,
) -> Result<(), SerializeError> {
    let common = &group.common;
    write_sz(w, common, false)?;
    write_pos(w, common)?;
    write_out_margin(w, common)?;
    write_shape_comment_text(w, "묶음 개체입니다.")?;
    end_tag(w, "hp:container")
}

fn container_hwpx_instid(group: &GroupShape) -> u32 {
    if group.inst_id != 0 {
        return group.inst_id;
    }
    group
        .children
        .iter()
        .filter_map(shape_object_inst_id)
        .filter(|id| *id != 0)
        .min()
        .map(|id| id.saturating_sub(1))
        .unwrap_or(group.common.instance_id)
}

fn shape_object_inst_id(shape: &crate::model::shape::ShapeObject) -> Option<u32> {
    match shape {
        crate::model::shape::ShapeObject::Line(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Rectangle(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Ellipse(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Arc(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Polygon(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Curve(s) => Some(s.drawing.inst_id),
        crate::model::shape::ShapeObject::Group(g) => Some(container_hwpx_instid(g)),
        crate::model::shape::ShapeObject::Picture(p) => Some(p.instance_id),
        crate::model::shape::ShapeObject::Chart(c) => Some(c.drawing.inst_id),
        crate::model::shape::ShapeObject::Ole(o) => Some(o.drawing.inst_id),
    }
}

// =====================================================================
// 공통 자식 요소 (sz / pos / outMargin)
// =====================================================================

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
    let allow_overlap = c.attr & 0x4000 != 0;
    let flow_with_text = !allow_overlap && c.attr & 0x2000 != 0;
    let vert_offset = c.vertical_offset.to_string();
    let horz_offset = c.horizontal_offset.to_string();
    empty_tag(
        w,
        "hp:pos",
        &[
            ("treatAsChar", treat),
            ("affectLSpacing", "0"),
            ("flowWithText", bool01(flow_with_text)),
            ("allowOverlap", bool01(allow_overlap)),
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

fn write_out_margin<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let l = c.margin.left.to_string();
    let r = c.margin.right.to_string();
    let t = c.margin.top.to_string();
    let b = c.margin.bottom.to_string();
    empty_tag(
        w,
        "hp:outMargin",
        &[("left", &l), ("right", &r), ("top", &t), ("bottom", &b)],
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::shape::{LineShape, RectangleShape};
    use crate::model::Point;

    fn serialize_rect(rect: &RectangleShape) -> String {
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        let mut ctx = SerializeContext::default();
        write_rect(&mut w, rect, &mut ctx).expect("write_rect");
        String::from_utf8(w.into_inner()).unwrap()
    }

    fn serialize_line(line: &LineShape) -> String {
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        let mut ctx = SerializeContext::default();
        write_line(&mut w, line, &mut ctx).expect("write_line");
        String::from_utf8(w.into_inner()).unwrap()
    }

    #[test]
    fn rect_emits_root_tag() {
        let mut rect = RectangleShape::default();
        rect.common.width = 1000;
        rect.common.height = 500;
        let xml = serialize_rect(&rect);
        assert!(xml.contains("<hp:rect "));
        assert!(xml.contains("</hp:rect>"));
    }

    #[test]
    fn rect_has_canonical_attrs() {
        let rect = RectangleShape::default();
        let xml = serialize_rect(&rect);
        assert!(xml.contains(r#"id=""#));
        assert!(xml.contains(r#"zOrder=""#));
        assert!(xml.contains(r#"textWrap=""#));
        assert!(xml.contains(r#"textFlow="BOTH_SIDES""#));
    }

    #[test]
    fn line_emits_start_end_attrs() {
        let mut line = LineShape::default();
        line.start = Point { x: 100, y: 200 };
        line.end = Point { x: 300, y: 400 };
        let xml = serialize_line(&line);
        assert!(xml.contains(r#"startX="100""#));
        assert!(xml.contains(r#"startY="200""#));
        assert!(xml.contains(r#"endX="300""#));
        assert!(xml.contains(r#"endY="400""#));
    }

    #[test]
    fn rect_has_sz_pos_out_margin() {
        let rect = RectangleShape::default();
        let xml = serialize_rect(&rect);
        assert!(xml.contains("<hp:sz "));
        assert!(xml.contains("<hp:pos "));
        assert!(xml.contains("<hp:outMargin "));
    }
}
