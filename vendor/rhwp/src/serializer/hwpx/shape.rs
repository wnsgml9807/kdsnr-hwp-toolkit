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
    CommonObjAttr, DrawingObjAttr, HorzAlign, HorzRelTo, LineShape, RectangleShape, TextBox,
    TextWrap, VertAlign, VertRelTo,
};
use crate::model::table::VerticalAlign;

use super::context::SerializeContext;
use super::utils::{empty_tag, end_tag, start_tag, start_tag_attrs};
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
    let z_order = c.z_order.to_string();
    let tw = text_wrap_str(c.text_wrap);

    start_tag_attrs(
        w,
        "hp:rect",
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
            ("instid", &id_str),
            ("ratio", "0"),
        ],
    )?;

    // Order observed in Hancom-saved samples:
    //   offset, orgSz, curSz, flip, rotationInfo, renderingInfo,
    //   lineShape, fillBrush, [shadow], sz, pos, outMargin
    write_offset_xy(w, c)?;
    write_org_curr_sizes(w, c)?;
    write_flip(w)?;
    write_rotation_info(w)?;
    write_rendering_info(w)?;
    write_line_shape(w, &rect.drawing.border_line)?;
    write_fill_brush(w, &rect.drawing.fill)?;
    write_shadow(w, &rect.drawing)?;
    // hp:drawText — inner text content, if the rect carries a text_box. The
    // (가)/(나) labels in social fillblank rectangles live here. Hancom
    // sample order: shadow → drawText → hc:pt0..3.
    if let Some(tb) = rect.drawing.text_box.as_ref() {
        write_draw_text(w, tb, c, ctx)?;
    }
    // hc:pt0..pt3 — rectangle corner coordinates in local space, derived
    // from orgSz. Binary IR stores raw x_coords/y_coords that may be stale
    // (resize ops update orgSz/curSz but not the corner array), so using
    // those directly puts the visible rectangle in the wrong place — the
    // drawText then floats outside the corner-defined area. Hancom-saved
    // samples always have pt0=(0,0), pt2=(orgSz.w, orgSz.h).
    let ow = c.width as i32;
    let oh = c.height as i32;
    let xs = [0, ow, ow, 0];
    let ys = [0, 0, oh, oh];
    write_rect_corners(w, &xs, &ys)?;
    write_sz(w, c)?;
    write_pos(w, c)?;
    write_out_margin(w, c)?;

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
    common: &CommonObjAttr,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    let last_width = common.width.to_string();
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

fn write_inner_paragraph<W: Write>(
    w: &mut Writer<W>,
    pi: usize,
    para: &Paragraph,
    ctx: &mut SerializeContext,
) -> Result<(), SerializeError> {
    use crate::model::paragraph::ColumnBreakType;
    ctx.para_shape_ids.reference(para.para_shape_id);
    ctx.style_ids.reference(para.style_id as u16);
    if let Some(cs_ref) = para.char_shapes.first() {
        ctx.char_shape_ids.reference(cs_ref.char_shape_id);
    }
    let pi_str = pi.to_string();
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
    let cs = para
        .char_shapes
        .first()
        .map(|r| r.char_shape_id)
        .unwrap_or(0);
    let cs_str = cs.to_string();
    start_tag_attrs(w, "hp:run", &[("charPrIDRef", &cs_str)])?;
    let run_inner = super::section::render_run_content(para, ctx);
    if !run_inner.is_empty() {
        use std::io::Write as _;
        w.get_mut()
            .write_all(run_inner.as_bytes())
            .map_err(|e| SerializeError::XmlError(e.to_string()))?;
    } else {
        empty_tag(w, "hp:t", &[])?;
    }
    end_tag(w, "hp:run")?;

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
    let z_order = c.z_order.to_string();
    let tw = text_wrap_str(c.text_wrap);
    let sx = line.start.x.to_string();
    let sy = line.start.y.to_string();
    let ex = line.end.x.to_string();
    let ey = line.end.y.to_string();
    let srb = bool01(line.started_right_or_bottom);

    start_tag_attrs(
        w,
        "hp:line",
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
            ("instid", &id_str),
            ("startX", &sx),
            ("startY", &sy),
            ("endX", &ex),
            ("endY", &ey),
            ("isReverseHV", srb),
        ],
    )?;

    // Order observed in Hancom-saved samples (mirrors write_rect):
    //   offset, orgSz, curSz, flip, rotationInfo, renderingInfo,
    //   lineShape, fillBrush, sz, pos, outMargin
    write_offset_xy(w, c)?;
    write_org_curr_sizes(w, c)?;
    write_flip(w)?;
    write_rotation_info(w)?;
    write_rendering_info(w)?;
    write_line_shape(w, &line.drawing.border_line)?;
    write_fill_brush(w, &line.drawing.fill)?;
    write_shadow(w, &line.drawing)?;
    write_sz(w, c)?;
    write_pos(w, c)?;
    write_out_margin(w, c)?;

    end_tag(w, "hp:line")?;
    Ok(())
}

// =====================================================================
// Shared shape-component children (used by rect/line/ellipse/etc.)
// =====================================================================

fn write_offset_xy<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let x = c.horizontal_offset.to_string();
    let y = c.vertical_offset.to_string();
    empty_tag(w, "hp:offset", &[("x", &x), ("y", &y)])
}

fn write_org_curr_sizes<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
) -> Result<(), SerializeError> {
    let width = c.width.to_string();
    let height = c.height.to_string();
    // orgSz = same as curSz when no separate original recorded (typical for
    // shapes; pictures override with binary's original).
    empty_tag(w, "hp:orgSz", &[("width", &width), ("height", &height)])?;
    empty_tag(w, "hp:curSz", &[("width", &width), ("height", &height)])
}

fn write_flip<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    empty_tag(w, "hp:flip", &[("horizontal", "0"), ("vertical", "0")])
}

fn write_rotation_info<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    empty_tag(
        w,
        "hp:rotationInfo",
        &[
            ("angle", "0"),
            ("centerX", "0"),
            ("centerY", "0"),
            ("rotateimage", "0"),
        ],
    )
}

fn write_rendering_info<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    start_tag(w, "hp:renderingInfo")?;
    for name in &["hc:transMatrix", "hc:scaMatrix", "hc:rotMatrix"] {
        empty_tag(
            w,
            name,
            &[
                ("e1", "1"),
                ("e2", "0"),
                ("e3", "0"),
                ("e4", "0"),
                ("e5", "1"),
                ("e6", "0"),
            ],
        )?;
    }
    end_tag(w, "hp:renderingInfo")?;
    Ok(())
}

fn color_ref_hwpx(color: u32) -> String {
    if color == 0xFFFFFFFF {
        "none".to_string()
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
    // Width=0 → fall back to 1pt thin (HWPUNIT 33 ≈ 0.12mm) so Hanword
    // doesn't render a zero-thickness invisible border.
    let width_val = if bl.width <= 0 { 33 } else { bl.width };
    let width = width_val.to_string();
    empty_tag(
        w,
        "hp:lineShape",
        &[
            ("color", &color),
            ("width", &width),
            ("style", "SOLID"),
            ("endCap", "FLAT"),
            ("headStyle", "NORMAL"),
            ("tailStyle", "NORMAL"),
            ("headfill", "1"),
            ("tailfill", "1"),
            ("headSz", "MEDIUM_MEDIUM"),
            ("tailSz", "MEDIUM_MEDIUM"),
            ("outlineStyle", "NORMAL"),
            ("alpha", "0"),
        ],
    )
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
                &[
                    ("faceColor", &face),
                    ("hatchColor", &hatch),
                    ("hatchStyle", "NONE"),
                    ("alpha", "0"),
                ],
            )?;
        }
        // Gradient/Image/None: emit a no-fill winBrush so the element
        // remains schema-valid; full gradient/image emission is future work.
        _ => {
            empty_tag(
                w,
                "hc:winBrush",
                &[
                    ("faceColor", "none"),
                    ("hatchColor", "none"),
                    ("hatchStyle", "NONE"),
                    ("alpha", "0"),
                ],
            )?;
        }
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
    common: &CommonObjAttr,
) -> Result<(), SerializeError> {
    let id_str = common.instance_id.to_string();
    let z_order = common.z_order.to_string();
    let tw = text_wrap_str(common.text_wrap);

    start_tag_attrs(
        w,
        "hp:container",
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
            ("instid", &id_str),
        ],
    )?;

    write_sz(w, common)?;
    write_pos(w, common)?;
    write_out_margin(w, common)?;

    Ok(())
}

pub fn write_container_close<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    end_tag(w, "hp:container")
}

// =====================================================================
// 공통 자식 요소 (sz / pos / outMargin)
// =====================================================================

fn write_sz<W: Write>(w: &mut Writer<W>, c: &CommonObjAttr) -> Result<(), SerializeError> {
    let width = c.width.to_string();
    let height = c.height.to_string();
    empty_tag(
        w,
        "hp:sz",
        &[
            ("width", &width),
            ("widthRelTo", "ABSOLUTE"),
            ("height", &height),
            ("heightRelTo", "ABSOLUTE"),
            ("protect", "0"),
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
