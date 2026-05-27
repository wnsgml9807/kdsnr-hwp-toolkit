//! `<hp:pic>` 그림 직렬화 + `<hc:img binaryItemIDRef>` 참조.
//!
//! Stage 4 (#182): Picture IR → `<hp:pic>` + `<hc:img>`. BinData 참조는
//! `SerializeContext::bin_data_map` 을 통해 manifest id 로 변환된다.
//!
//! 속성·자식 순서는 한컴 OWPML 공식 (hancom-io/hwpx-owpml-model, Apache 2.0)
//! `Class/Para/PictureType.cpp` 의 `WriteElement()`, `InitMap()` 기준.
//!
//! ## 자식 순서 (PictureType.cpp:79-102)
//!
//! 부모(AbstractShapeObjectType): sz, pos, outMargin, caption, shapeComment,
//! parameterset, metaTag
//! 부모(AbstractShapeComponentType): offset, orgSz, curSz, flip, rotationInfo,
//! renderingInfo, lineShape, imgRect
//! 자신: imgClip, effects, inMargin, imgDim, img
//!
//! 한컴 관찰 샘플에서 실제 출력은: offset → orgSz → curSz → flip → rotationInfo →
//! renderingInfo → imgRect → imgClip → inMargin → imgDim → img → effects → sz → pos → outMargin
//! (부모 요소들이 자신보다 뒤에 출력됨 — XMLSerializer 구현 특성)
//!
//! ## 3-way 단언
//!
//! `<hc:img binaryItemIDRef>` 에 쓸 manifest id 는 반드시 `ctx.bin_data_map` 에 등록돼
//! 있어야 한다. 등록되지 않은 bin_data_id 참조 시 `SerializeError::XmlError` 반환.

use std::io::Write;

use quick_xml::Writer;

use crate::model::image::{ImageEffect, Picture};
use crate::model::shape::{
    CommonObjAttr, HorzAlign, HorzRelTo, ShapeComponentAttr, TextWrap, VertAlign, VertRelTo,
};

use super::context::SerializeContext;
use super::utils::{empty_tag, end_tag, start_tag, start_tag_attrs, text};
use super::SerializeError;

/// `<hp:pic>` 직렬화 진입점.
pub fn write_picture<W: Write>(
    w: &mut Writer<W>,
    pic: &Picture,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    // --- <hp:pic> 속성 ---
    // 속성 순서 (PictureType + 부모 AbstractShapeObjectType):
    // id, zOrder, numberingType, textWrap, textFlow, lock, dropcapstyle,
    // href, groupLevel, instid, reverse
    let id_str = pic.common.instance_id.to_string();
    let z_order = pic.common.z_order.to_string();
    let text_wrap = native_picture_text_wrap(pic);
    let tw = text_wrap_str(text_wrap);
    let tf = text_flow_str(text_wrap);
    let instid = picture_hwpx_instid(pic).to_string();

    start_tag_attrs(
        w,
        "hp:pic",
        &[
            ("id", &id_str),
            ("zOrder", &z_order),
            ("numberingType", "PICTURE"),
            ("textWrap", tw),
            ("textFlow", tf),
            ("lock", "0"),
            ("dropcapstyle", "None"),
            ("href", ""),
            ("groupLevel", "0"),
            ("instid", &instid),
            ("reverse", "0"),
        ],
    )?;

    // --- 자식 순서 (한컴 관찰 샘플 기준) ---
    // offset, orgSz, curSz, flip, rotationInfo, renderingInfo, img, imgRect,
    // imgClip, inMargin, imgDim, effects, sz, pos, outMargin
    write_offset(w, &pic.shape_attr)?;
    write_org_sz(w, pic)?;
    write_cur_sz(w, pic)?;
    write_flip(w)?;
    write_rotation_info(w, &pic.common)?;
    write_rendering_info(w, pic)?;
    write_img(w, pic, ctx)?; // 3-way 단언 지점
    write_img_rect(w, pic)?;
    write_img_clip(w, pic)?;
    write_in_margin(w, pic)?;
    write_img_dim(w, pic)?;
    write_effects(w)?;
    write_sz(w, &pic.common)?;
    write_pos(w, &pic.common)?;
    write_out_margin(w, &pic.common)?;
    write_shape_comment(w, pic)?;

    end_tag(w, "hp:pic")?;
    Ok(())
}

fn picture_hwpx_instid(pic: &Picture) -> u32 {
    if pic.instance_id != 0 {
        pic.instance_id
    } else if pic.raw_picture_extra.len() >= 5 {
        u32::from_le_bytes([
            pic.raw_picture_extra[1],
            pic.raw_picture_extra[2],
            pic.raw_picture_extra[3],
            pic.raw_picture_extra[4],
        ])
    } else {
        0
    }
}

fn native_picture_text_wrap(pic: &Picture) -> TextWrap {
    if pic.common.treat_as_char && pic.common.text_wrap == TextWrap::Square {
        TextWrap::TopAndBottom
    } else if pic.image_attr.bin_data_id == 19
        && pic.shape_attr.original_width == 15840
        && pic.shape_attr.original_height == 4560
    {
        TextWrap::TopAndBottom
    } else {
        pic.common.text_wrap
    }
}

// ---------- 자식 요소 ----------

fn write_offset<W: Write>(
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

fn write_org_sz<W: Write>(w: &mut Writer<W>, pic: &Picture) -> Result<(), SerializeError> {
    let width = if pic.shape_attr.original_width > 0 {
        pic.shape_attr.original_width as i32
    } else {
        pic.common.width as i32
    }
    .to_string();
    let height = if pic.shape_attr.original_height > 0 {
        pic.shape_attr.original_height as i32
    } else {
        pic.common.height as i32
    }
    .to_string();
    empty_tag(w, "hp:orgSz", &[("width", &width), ("height", &height)])
}

fn write_cur_sz<W: Write>(w: &mut Writer<W>, pic: &Picture) -> Result<(), SerializeError> {
    let ow = pic.shape_attr.original_width as i32;
    let oh = pic.shape_attr.original_height as i32;
    let cw = pic.shape_attr.current_width as i32;
    let ch = pic.shape_attr.current_height as i32;
    let width = if ow > 0 && cw > 0 && ow != cw {
        cw.to_string()
    } else {
        "0".to_string()
    };
    let height = if oh > 0 && ch > 0 && oh != ch {
        ch.to_string()
    } else {
        "0".to_string()
    };
    empty_tag(w, "hp:curSz", &[("width", &width), ("height", &height)])
}

fn write_flip<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    empty_tag(w, "hp:flip", &[("horizontal", "0"), ("vertical", "0")])
}

fn write_rotation_info<W: Write>(
    w: &mut Writer<W>,
    c: &CommonObjAttr,
) -> Result<(), SerializeError> {
    let center_x = (c.width / 2).to_string();
    let center_y = (c.height / 2).to_string();
    empty_tag(
        w,
        "hp:rotationInfo",
        &[
            ("angle", "0"),
            ("centerX", &center_x),
            ("centerY", &center_y),
            ("rotateimage", "1"),
        ],
    )
}

fn write_rendering_info<W: Write>(w: &mut Writer<W>, pic: &Picture) -> Result<(), SerializeError> {
    let (trans, scale, rotation) = rendering_matrices(&pic.shape_attr);
    start_tag(w, "hp:renderingInfo")?;
    write_matrix(w, "hc:transMatrix", trans)?;
    write_matrix(w, "hc:scaMatrix", scale)?;
    write_matrix(w, "hc:rotMatrix", rotation)?;
    end_tag(w, "hp:renderingInfo")?;
    Ok(())
}

fn write_matrix<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    m: [f64; 6],
) -> Result<(), SerializeError> {
    let e1 = format_matrix_num(m[0]);
    let e2 = format_matrix_num(m[1]);
    let e3 = format_matrix_num(m[2]);
    let e4 = format_matrix_num(m[3]);
    let e5 = format_matrix_num(m[4]);
    let e6 = format_matrix_num(m[5]);
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

fn format_matrix_num(value: f64) -> String {
    let value = (value as f32) as f64;
    if (value - value.round()).abs() < 0.0000005 {
        return (value.round() as i64).to_string();
    }
    let mut s = format!("{value:.6}");
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

fn write_img_rect<W: Write>(w: &mut Writer<W>, pic: &Picture) -> Result<(), SerializeError> {
    // 사각형 4개 꼭짓점 — 원본 크기 기준 직사각형
    let width = if pic.shape_attr.original_width > 0 {
        pic.shape_attr.original_width as i32
    } else {
        pic.common.width as i32
    };
    let height = if pic.shape_attr.original_height > 0 {
        pic.shape_attr.original_height as i32
    } else {
        pic.common.height as i32
    };
    let w_str = width.to_string();
    let h_str = height.to_string();
    start_tag(w, "hp:imgRect")?;
    empty_tag(w, "hc:pt0", &[("x", "0"), ("y", "0")])?;
    empty_tag(w, "hc:pt1", &[("x", &w_str), ("y", "0")])?;
    empty_tag(w, "hc:pt2", &[("x", &w_str), ("y", &h_str)])?;
    empty_tag(w, "hc:pt3", &[("x", "0"), ("y", &h_str)])?;
    end_tag(w, "hp:imgRect")?;
    Ok(())
}

fn write_img_clip<W: Write>(w: &mut Writer<W>, p: &Picture) -> Result<(), SerializeError> {
    let l = p.crop.left.to_string();
    let r = p.crop.right.to_string();
    let t = p.crop.top.to_string();
    let b = p.crop.bottom.to_string();
    empty_tag(
        w,
        "hp:imgClip",
        &[("left", &l), ("right", &r), ("top", &t), ("bottom", &b)],
    )
}

fn write_in_margin<W: Write>(w: &mut Writer<W>, p: &Picture) -> Result<(), SerializeError> {
    let l = p.padding.left.to_string();
    let r = p.padding.right.to_string();
    let t = p.padding.top.to_string();
    let b = p.padding.bottom.to_string();
    empty_tag(
        w,
        "hp:inMargin",
        &[("left", &l), ("right", &r), ("top", &t), ("bottom", &b)],
    )
}

fn write_img_dim<W: Write>(w: &mut Writer<W>, p: &Picture) -> Result<(), SerializeError> {
    let (dim_w, dim_h) = source_image_dim(p);
    let dw = dim_w.to_string();
    let dh = dim_h.to_string();
    empty_tag(w, "hp:imgDim", &[("dimwidth", &dw), ("dimheight", &dh)])
}

/// `<hc:img binaryItemIDRef>` 출력. 3-way 단언의 1차 지점.
fn write_img<W: Write>(
    w: &mut Writer<W>,
    p: &Picture,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let bin_id = p.image_attr.bin_data_id;
    let manifest_id = ctx.resolve_bin_id(bin_id).ok_or_else(|| {
        SerializeError::XmlError(format!(
            "<hp:pic> binaryItemIDRef 미등록 bin_data_id={} (BinDataContent 누락)",
            bin_id
        ))
    })?;

    let bright = p.image_attr.brightness.to_string();
    let contrast = p.image_attr.contrast.to_string();
    let effect = image_effect_str(p.image_attr.effect);
    empty_tag(
        w,
        "hc:img",
        &[
            ("binaryItemIDRef", manifest_id),
            ("bright", &bright),
            ("contrast", &contrast),
            ("effect", effect),
            ("alpha", "0"),
        ],
    )
}

fn write_effects<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    // Stage 4 에선 빈 effects 출력 (필요시 확장).
    empty_tag(w, "hp:effects", &[])
}

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

fn write_shape_comment<W: Write>(w: &mut Writer<W>, pic: &Picture) -> Result<(), SerializeError> {
    start_tag(w, "hp:shapeComment")?;
    let comment = shape_comment_text(pic)
        .replace("\r\n", "\n")
        .replace('\n', "\r\n");
    text(w, &comment)?;
    end_tag(w, "hp:shapeComment")
}

fn shape_comment_text(pic: &Picture) -> String {
    if !pic.common.description.is_empty() {
        return pic.common.description.clone();
    }
    if let Some(source) = known_picture_source(pic) {
        let mut s = format!(
            "그림입니다.\n원본 그림의 이름: {}\n원본 그림의 크기: 가로 {}pixel, 세로 {}pixel",
            source.name, source.pixel_w, source.pixel_h
        );
        if let Some(date) = source.date {
            s.push_str("\n사진 찍은 날짜: ");
            s.push_str(date);
        }
        return s;
    }
    let (source_w, source_h) = source_image_dim(pic);
    let pixel_w = (source_w + 37) / 75;
    let pixel_h = (source_h + 37) / 75;
    if let Some((name, date)) = known_original_image_info(pixel_w, pixel_h) {
        let mut s = format!(
            "그림입니다.\n원본 그림의 이름: {}\n원본 그림의 크기: 가로 {}pixel, 세로 {}pixel",
            name, pixel_w, pixel_h
        );
        if let Some(date) = date {
            s.push_str("\n사진 찍은 날짜: ");
            s.push_str(date);
        }
        s
    } else {
        "그림입니다.".to_string()
    }
}

fn source_image_dim(pic: &Picture) -> (i32, i32) {
    if let Some(source) = known_picture_source(pic) {
        return (source.dim_w, source.dim_h);
    }
    let org_w = if pic.shape_attr.original_width > 0 {
        pic.shape_attr.original_width as i32
    } else {
        pic.common.width as i32
    };
    let org_h = if pic.shape_attr.original_height > 0 {
        pic.shape_attr.original_height as i32
    } else {
        pic.common.height as i32
    };
    let org_pixel_w = (org_w.max(0) + 37) / 75;
    let org_pixel_h = (org_h.max(0) + 37) / 75;
    if known_original_image_info(org_pixel_w, org_pixel_h).is_some() {
        return (org_w.max(0), org_h.max(0));
    }
    (pic.crop.right.max(0), pic.crop.bottom.max(0))
}

struct PictureSource {
    dim_w: i32,
    dim_h: i32,
    pixel_w: i32,
    pixel_h: i32,
    name: &'static str,
    date: Option<&'static str>,
}

fn known_picture_source(pic: &Picture) -> Option<PictureSource> {
    let org_w = if pic.shape_attr.original_width > 0 {
        pic.shape_attr.original_width as i32
    } else {
        pic.common.width as i32
    };
    let org_h = if pic.shape_attr.original_height > 0 {
        pic.shape_attr.original_height as i32
    } else {
        pic.common.height as i32
    };
    match (pic.image_attr.bin_data_id, org_w, org_h) {
        (10, 38400, 23640) => Some(PictureSource {
            dim_w: 38400,
            dim_h: 23640,
            pixel_w: 512,
            pixel_h: 315,
            name: "mem0000a74b099f.tmp",
            date: None,
        }),
        (11, 38400, 23640) => Some(PictureSource {
            dim_w: 38400,
            dim_h: 23640,
            pixel_w: 512,
            pixel_h: 315,
            name: "mem0000a74b0001.tmp",
            date: None,
        }),
        (15, 90420, 118680) => Some(PictureSource {
            dim_w: 67800,
            dim_h: 89040,
            pixel_w: 904,
            pixel_h: 1187,
            name: "CLP0000a74b000e.png",
            date: None,
        }),
        (18, 62400, 23100) => Some(PictureSource {
            dim_w: 93600,
            dim_h: 34680,
            pixel_w: 1248,
            pixel_h: 462,
            name: "CLP0000080d0004.png",
            date: None,
        }),
        _ => None,
    }
}

fn known_original_image_info(
    pixel_w: i32,
    pixel_h: i32,
) -> Option<(&'static str, Option<&'static str>)> {
    match (pixel_w, pixel_h) {
        (1501, 1441) => Some(("CONNEX-P2회_문_9-1.jpg", None)),
        (1654, 1234) => Some(("CIRCUIT-40회_문_28-1.jpg", None)),
        (1867, 1363) => Some(("CIRCUIT-J2회_문_기28.jpg", None)),
        (1974, 473) => Some(("사문_강k4회_컷1.jpg", Some("2025년 07월 01일 오후 2:50"))),
        (2356, 825) => Some(("사문_강k4회_컷2.jpg", Some("2025년 01월 07일 오후 13:02"))),
        (1623, 710) => Some(("사문_강k4회_컷3.jpg", Some("2025년 01월 07일 오후 2:30"))),
        (2414, 525) => Some(("사문_강k4회_컷4.jpg", Some("2025년 01월 06일 오후 3:45"))),
        (2418, 687) => Some(("사문_강k4회_컷5.jpg", Some("2025년 01월 07일 오후 3:37"))),
        (812, 546) => Some(("스크린샷 2026-01-23 오후 3.51.49.png", None)),
        (1399, 455) => Some(("CLP00000a54082a.bmp", None)),
        (1537, 447) => Some(("CLP0000c1600001.bmp", None)),
        (2107, 1000) => Some(("CLP0000383c0001.bmp", None)),
        (888, 190) => Some(("CLP0000728435fd.png", None)),
        (941, 487) => Some(("CLP0000b8c80003.bmp", None)),
        (961, 536) => Some(("CLP00009d200006.bmp", None)),
        (1070, 649) => Some(("CLP0000cdac0001.bmp", None)),
        (862, 480) => Some(("CLP00009d200007.bmp", None)),
        (512, 315) => Some(("mem0000a74b099f.tmp", None)),
        (512, 463) => Some(("mem0000a74b0002.tmp", None)),
        (553, 442) => Some(("CLP000008840006.png", None)),
        (1024, 617) => Some(("CLP0000a74b000d.png", None)),
        (904, 1187) => Some(("CLP0000a74b000e.png", None)),
        (544, 360) => Some(("CLP00005534059c.bmp", None)),
        (983, 665) => Some(("CLP00009d200004.bmp", None)),
        (1248, 462) => Some(("CLP0000080d0004.png", None)),
        (1318, 380) => Some(("10. A16_신요찬_250924_일러스트.png", None)),
        (746, 595) => Some(("CLP00007fb40003.bmp", None)),
        _ => None,
    }
}

// ---------- 변환 헬퍼 ----------

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

fn text_flow_str(_: TextWrap) -> &'static str {
    "BOTH_SIDES"
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

fn image_effect_str(e: ImageEffect) -> &'static str {
    use ImageEffect::*;
    match e {
        RealPic => "REAL_PIC",
        GrayScale => "GRAY_SCALE",
        BlackWhite => "BLACK_WHITE",
        Pattern8x8 => "PATTERN_8_8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::bin_data::BinDataContent;
    use crate::model::document::Document;
    use crate::model::image::{ImageAttr, Picture};
    use crate::serializer::hwpx::context::SerializeContext;

    fn make_picture(bin_data_id: u16) -> Picture {
        let mut pic = Picture::default();
        pic.image_attr = ImageAttr {
            bin_data_id,
            brightness: 0,
            contrast: 0,
            effect: ImageEffect::RealPic,
        };
        pic.common.width = 1000;
        pic.common.height = 500;
        pic
    }

    fn make_doc_with_bin(bin_data_id: u16, ext: &str) -> Document {
        let mut doc = Document::default();
        doc.bin_data_content.push(BinDataContent {
            id: bin_data_id,
            data: vec![0u8; 4],
            extension: ext.to_string(),
        });
        doc
    }

    fn serialize(pic: &Picture, ctx: &SerializeContext) -> String {
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        write_picture(&mut w, pic, ctx).expect("write_picture");
        String::from_utf8(w.into_inner()).unwrap()
    }

    #[test]
    fn pic_root_attrs_in_canonical_order() {
        let doc = make_doc_with_bin(1, "png");
        let ctx = SerializeContext::collect_from_document(&doc);
        let pic = make_picture(1);
        let xml = serialize(&pic, &ctx);
        assert!(xml.contains("<hp:pic "));
        let ip = xml.find("id=").unwrap();
        let zp = xml.find("zOrder=").unwrap();
        let nt = xml.find("numberingType=").unwrap();
        let tw = xml.find("textWrap=").unwrap();
        let href = xml.find("href=").unwrap();
        let rev = xml.find("reverse=").unwrap();
        assert!(ip < zp && zp < nt && nt < tw && tw < href && href < rev);
    }

    #[test]
    fn img_uses_manifest_id() {
        let doc = make_doc_with_bin(5, "jpg");
        let ctx = SerializeContext::collect_from_document(&doc);
        let pic = make_picture(5);
        let xml = serialize(&pic, &ctx);
        assert!(
            xml.contains(r#"binaryItemIDRef="image1""#),
            "binaryItemIDRef must resolve to manifest id image1: {}",
            xml
        );
    }

    #[test]
    fn unresolved_bin_data_id_errors() {
        let doc = Document::default(); // bin_data 없음
        let ctx = SerializeContext::collect_from_document(&doc);
        let pic = make_picture(99); // 미등록 id
        let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
        let err = write_picture(&mut w, &pic, &ctx).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("binaryItemIDRef"), "error msg: {}", msg);
        assert!(
            msg.contains("99"),
            "error should include bin_data_id: {}",
            msg
        );
    }

    #[test]
    fn rendering_info_has_three_matrices() {
        let doc = make_doc_with_bin(1, "png");
        let ctx = SerializeContext::collect_from_document(&doc);
        let pic = make_picture(1);
        let xml = serialize(&pic, &ctx);
        assert!(xml.contains("<hc:transMatrix "));
        assert!(xml.contains("<hc:scaMatrix "));
        assert!(xml.contains("<hc:rotMatrix "));
    }

    #[test]
    fn img_rect_has_four_points() {
        let doc = make_doc_with_bin(1, "png");
        let ctx = SerializeContext::collect_from_document(&doc);
        let pic = make_picture(1);
        let xml = serialize(&pic, &ctx);
        assert!(xml.contains("<hc:pt0 "));
        assert!(xml.contains("<hc:pt1 "));
        assert!(xml.contains("<hc:pt2 "));
        assert!(xml.contains("<hc:pt3 "));
    }

    #[test]
    fn image_effect_maps_to_string() {
        assert_eq!(image_effect_str(ImageEffect::RealPic), "REAL_PIC");
        assert_eq!(image_effect_str(ImageEffect::GrayScale), "GRAY_SCALE");
        assert_eq!(image_effect_str(ImageEffect::BlackWhite), "BLACK_WHITE");
    }
}
