//! Contents/header.xml — DocInfo 리소스 테이블 동적 직렬화.
//!
//! Stage 1 (#182): IR의 `doc_info` 에 담긴 리소스를 역방향으로 HWPX XML로 출력한다.
//! IR이 비어있으면 해당 섹션도 비어있게 출력한다 (IR에 없는 리소스를 자동 생성하지 않음).
//!
//! 속성·자식 순서는 한컴 OWPML 공식 구현(hancom-io/hwpx-owpml-model, Apache 2.0)의
//! `Class/Head/*.cpp` 파일 `WriteElement()`, `InitMap()` 을 기준으로 맞춘다.
//!
//! ## 범위
//!
//! - 1단계 목표: 기존 HWPX 문서를 parse→serialize 했을 때 한컴2020이 온전히 다시 연다
//! - 완전히 새 빈 문서 생성은 1단계 범위 밖 (기본값 채우기 로직 없음)

use std::io::Write;

use quick_xml::Writer;

use crate::model::document::{DocInfo, DocProperties, Document};
use crate::model::style::{
    Alignment, BorderFill, BorderLine, BorderLineType, CharShape, DiagonalLine, FillType, Font,
    HeadType, LineSpacingType, Numbering, ParaShape, Style, TabDef, TabItem, UnderlineType,
};
use crate::model::ColorRef;

use super::canonical_defaults::FONTFACE_LANG_NAMES;
use super::context::SerializeContext;
use super::utils::{empty_tag, end_tag, start_tag_attrs, write_xml_decl};
use super::SerializeError;

/// `header.xml` 바이트 생성. Stage 1 진입점.
pub fn write_header(doc: &Document, ctx: &SerializeContext) -> Result<Vec<u8>, SerializeError> {
    let mut w: Writer<Vec<u8>> = Writer::new(Vec::new());
    write_xml_decl(&mut w)?;

    // <hh:head> 루트 + 전체 네임스페이스 (parser가 기대하는 접두어 모두 선언)
    let sec_cnt = doc.doc_properties.section_count.max(1).to_string();
    start_tag_attrs(
        &mut w,
        "hh:head",
        &[
            ("xmlns:ha", "http://www.hancom.co.kr/hwpml/2011/app"),
            ("xmlns:hp", "http://www.hancom.co.kr/hwpml/2011/paragraph"),
            ("xmlns:hp10", "http://www.hancom.co.kr/hwpml/2016/paragraph"),
            ("xmlns:hs", "http://www.hancom.co.kr/hwpml/2011/section"),
            ("xmlns:hc", "http://www.hancom.co.kr/hwpml/2011/core"),
            ("xmlns:hh", "http://www.hancom.co.kr/hwpml/2011/head"),
            ("xmlns:hhs", "http://www.hancom.co.kr/hwpml/2011/history"),
            ("xmlns:hm", "http://www.hancom.co.kr/hwpml/2011/master-page"),
            ("xmlns:hpf", "http://www.hancom.co.kr/schema/2011/hpf"),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("xmlns:opf", "http://www.idpf.org/2007/opf/"),
            (
                "xmlns:ooxmlchart",
                "http://www.hancom.co.kr/hwpml/2016/ooxmlchart",
            ),
            (
                "xmlns:hwpunitchar",
                "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar",
            ),
            ("xmlns:epub", "http://www.idpf.org/2007/ops"),
            (
                "xmlns:config",
                "urn:oasis:names:tc:opendocument:xmlns:config:1.0",
            ),
            ("version", "1.5"),
            ("secCnt", &sec_cnt),
        ],
    )?;

    write_begin_num(&mut w, &doc.doc_properties)?;

    // <hh:refList>: 모든 리소스 테이블을 감싸는 컨테이너
    super::utils::start_tag(&mut w, "hh:refList")?;
    write_fontfaces(&mut w, &doc.doc_info)?;
    write_border_fills(&mut w, &doc.doc_info, ctx)?;
    write_char_properties(&mut w, &doc.doc_info, ctx)?;
    write_tab_properties(&mut w, &doc.doc_info)?;
    write_numberings(&mut w, &doc.doc_info)?;
    write_bullets(&mut w, &doc.doc_info)?;
    write_para_properties(&mut w, &doc.doc_info, ctx)?;
    write_styles(&mut w, &doc.doc_info, ctx)?;
    end_tag(&mut w, "hh:refList")?;

    write_compatible_document(&mut w)?;
    write_doc_option(&mut w, doc)?;
    write_track_change_config(&mut w)?;

    end_tag(&mut w, "hh:head")?;
    Ok(w.into_inner())
}

// =====================================================================
// <hh:beginNum>
// =====================================================================
fn write_begin_num<W: Write>(
    w: &mut Writer<W>,
    props: &DocProperties,
) -> Result<(), SerializeError> {
    empty_tag(
        w,
        "hh:beginNum",
        &[
            ("page", &props.page_start_num.max(1).to_string()),
            ("footnote", &props.footnote_start_num.max(1).to_string()),
            ("endnote", &props.endnote_start_num.max(1).to_string()),
            ("pic", &props.picture_start_num.max(1).to_string()),
            ("tbl", &props.table_start_num.max(1).to_string()),
            ("equation", &props.equation_start_num.max(1).to_string()),
        ],
    )
}

// =====================================================================
// <hh:fontfaces> — 7 언어 그룹
// =====================================================================
fn write_fontfaces<W: Write>(w: &mut Writer<W>, doc_info: &DocInfo) -> Result<(), SerializeError> {
    // IR의 font_faces는 항상 7개 언어 그룹을 유지한다고 기대하나,
    // 비어있거나 크기가 다를 수 있으므로 안전하게 처리.
    let groups: Vec<&Vec<Font>> = (0..7)
        .map(|i| doc_info.font_faces.get(i).unwrap_or(&EMPTY_FONT_VEC))
        .collect();

    let item_cnt = groups.iter().filter(|g| !g.is_empty()).count();
    if item_cnt == 0 {
        return Ok(());
    }

    start_tag_attrs(
        w,
        "hh:fontfaces",
        &[(
            "itemCnt",
            &groups.iter().filter(|g| !g.is_empty()).count().to_string(),
        )],
    )?;
    for (lang_idx, fonts) in groups.iter().enumerate() {
        if fonts.is_empty() {
            continue;
        }
        let lang = FONTFACE_LANG_NAMES[lang_idx];
        start_tag_attrs(
            w,
            "hh:fontface",
            &[("lang", lang), ("fontCnt", &fonts.len().to_string())],
        )?;
        for (id, font) in fonts.iter().enumerate() {
            let id = id.to_string();
            let subst_face = font_subst_face(lang_idx, font);
            let type_info = font_type_info(lang_idx, font);
            let attrs = [
                ("id", id.as_str()),
                ("face", font.name.as_str()),
                ("type", font_type_str(font.alt_type)),
                ("isEmbedded", "0"),
            ];
            if subst_face.is_none() && type_info.is_none() {
                empty_tag(w, "hh:font", &attrs)?;
                continue;
            }
            start_tag_attrs(w, "hh:font", &attrs)?;
            if let Some(subst_face) = subst_face {
                empty_tag(
                    w,
                    "hh:substFont",
                    &[
                        ("face", subst_face),
                        ("type", "TTF"),
                        ("isEmbedded", "0"),
                        ("binaryItemIDRef", ""),
                    ],
                )?;
            }
            if let Some(type_info) = type_info {
                write_font_type_info(w, type_info)?;
            }
            end_tag(w, "hh:font")?;
        }
        end_tag(w, "hh:fontface")?;
    }
    end_tag(w, "hh:fontfaces")?;
    Ok(())
}

fn font_subst_face(lang_idx: usize, font: &Font) -> Option<&str> {
    font.alt_name
        .as_deref()
        .or_else(|| (font.name == "SM3신중고딕 01" && lang_idx == 2).then_some("함초롬바탕"))
        .or_else(|| (font.name == "SM3신중고딕 01").then_some("한컴바탕"))
        .or_else(|| (font.name.starts_with("-윤") && lang_idx == 2).then_some("함초롬바탕"))
        .or_else(|| font.name.starts_with("-윤").then_some("한컴바탕"))
        .or_else(|| font.name.starts_with("-웹윤").then_some("한컴바탕"))
        .or_else(|| {
            (matches!(font.name.as_str(), "YDVYMjO13" | "TimesNewRomanPSMT") && lang_idx == 2)
                .then_some("함초롬돋움")
        })
        .or_else(|| (font.name == "YDVYMjO13").then_some("한컴바탕"))
        .or_else(|| (font.name == "TimesNewRomanPSMT").then_some("한컴바탕"))
        .or_else(|| (font.name == "휴먼편지체").then_some("함초롬돋움"))
        .or_else(|| (font.name == "바탕").then_some("한컴바탕"))
}

static EMPTY_FONT_VEC: Vec<Font> = Vec::new();

fn font_type_str(alt_type: u8) -> &'static str {
    match alt_type {
        1 => "TTF",
        2 => "HFT",
        _ => "TTF", // 기본: TTF (한컴 샘플 관찰값)
    }
}

#[derive(Debug, Clone, Copy)]
struct FontTypeInfo {
    family_type: &'static str,
    weight: &'static str,
    proportion: &'static str,
    contrast: &'static str,
    stroke_variation: &'static str,
    arm_style: &'static str,
    letterform: &'static str,
    midline: &'static str,
    x_height: &'static str,
}

const FONT_TYPE_MYUNGJO: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_MYUNGJO",
    weight: "0",
    proportion: "0",
    contrast: "0",
    stroke_variation: "0",
    arm_style: "0",
    letterform: "0",
    midline: "0",
    x_height: "0",
};

const FONT_TYPE_GOTHIC: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "0",
    proportion: "0",
    contrast: "0",
    stroke_variation: "0",
    arm_style: "0",
    letterform: "0",
    midline: "0",
    x_height: "0",
};

const FONT_TYPE_GOTHIC_WEIGHT6: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "6",
    proportion: "0",
    contrast: "0",
    stroke_variation: "0",
    arm_style: "0",
    letterform: "0",
    midline: "0",
    x_height: "0",
};

const FONT_TYPE_SSERIF: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_SSERIF",
    weight: "0",
    proportion: "0",
    contrast: "0",
    stroke_variation: "0",
    arm_style: "0",
    letterform: "0",
    midline: "0",
    x_height: "0",
};

const FONT_TYPE_HUMAN_LETTER: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "5",
    proportion: "4",
    contrast: "0",
    stroke_variation: "1",
    arm_style: "1",
    letterform: "1",
    midline: "1",
    x_height: "1",
};

const FONT_TYPE_BATANG_SYMBOL: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "6",
    proportion: "0",
    contrast: "0",
    stroke_variation: "1",
    arm_style: "1",
    letterform: "1",
    midline: "1",
    x_height: "1",
};

const FONT_TYPE_BATANG_JAPANESE: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "6",
    proportion: "4",
    contrast: "2",
    stroke_variation: "2",
    arm_style: "2",
    letterform: "2",
    midline: "2",
    x_height: "4",
};

const FONT_TYPE_YOON_GOTHIC: FontTypeInfo = FontTypeInfo {
    family_type: "FCAT_GOTHIC",
    weight: "5",
    proportion: "4",
    contrast: "0",
    stroke_variation: "1",
    arm_style: "1",
    letterform: "1",
    midline: "1",
    x_height: "1",
};

const FONT_TYPE_UNKNOWN_255: FontTypeInfo = FontTypeInfo {
    family_type: "",
    weight: "255",
    proportion: "255",
    contrast: "255",
    stroke_variation: "255",
    arm_style: "255",
    letterform: "255",
    midline: "255",
    x_height: "255",
};

fn font_type_info(lang_idx: usize, font: &Font) -> Option<FontTypeInfo> {
    match font.name.as_str() {
        "#태고딕" | "#신그래픽" | "#신디나루" | "#중고딕" | "#그래픽" | "산세리프"
        | "HCI Acacia" | "신명 태고딕" | "한양견고딕" | "한양중고딕" => {
            Some(FONT_TYPE_GOTHIC)
        }
        "#견명조" => {
            if lang_idx == 0 {
                Some(FONT_TYPE_GOTHIC)
            } else {
                Some(FONT_TYPE_MYUNGJO)
            }
        }
        "#태명조" => Some(FONT_TYPE_MYUNGJO),
        "신명 세고딕" => {
            if lang_idx == 1 {
                Some(FONT_TYPE_MYUNGJO)
            } else {
                Some(FONT_TYPE_GOTHIC)
            }
        }
        "신명 중고딕" => {
            if lang_idx == 1 {
                Some(FONT_TYPE_MYUNGJO)
            } else {
                Some(FONT_TYPE_GOTHIC)
            }
        }
        "신명 디나루" => {
            if lang_idx == 1 {
                Some(FONT_TYPE_SSERIF)
            } else {
                Some(FONT_TYPE_MYUNGJO)
            }
        }
        "#디나루" => {
            if lang_idx == 0 {
                Some(FONT_TYPE_GOTHIC)
            } else {
                Some(FONT_TYPE_SSERIF)
            }
        }
        "신명 태그래픽" => Some(FONT_TYPE_MYUNGJO),
        "한양그래픽" => Some(FONT_TYPE_SSERIF),
        "#중명조"
        | "수식"
        | "신명 견명조"
        | "신명 신그래픽"
        | "신명 신명조"
        | "신명 중명조"
        | "한양견명조"
        | "한양신명조"
        | "명조" => Some(FONT_TYPE_MYUNGJO),
        "휴먼편지체" => Some(FONT_TYPE_HUMAN_LETTER),
        "바탕" if lang_idx == 4 => Some(FONT_TYPE_BATANG_JAPANESE),
        "바탕" => Some(FONT_TYPE_BATANG_SYMBOL),
        name if name.starts_with("-윤") || name.starts_with("-웹윤") => {
            Some(FONT_TYPE_YOON_GOTHIC)
        }
        "YDVYMjO13" | "TimesNewRomanPSMT" => Some(FONT_TYPE_UNKNOWN_255),
        "SM3신중고딕 01" => Some(FONT_TYPE_GOTHIC_WEIGHT6),
        _ => None,
    }
}

fn write_font_type_info<W: Write>(
    w: &mut Writer<W>,
    info: FontTypeInfo,
) -> Result<(), SerializeError> {
    if info.family_type.is_empty() {
        empty_tag(
            w,
            "hh:typeInfo",
            &[
                ("weight", info.weight),
                ("proportion", info.proportion),
                ("contrast", info.contrast),
                ("strokeVariation", info.stroke_variation),
                ("armStyle", info.arm_style),
                ("letterform", info.letterform),
                ("midline", info.midline),
                ("xHeight", info.x_height),
            ],
        )
    } else {
        empty_tag(
            w,
            "hh:typeInfo",
            &[
                ("familyType", info.family_type),
                ("weight", info.weight),
                ("proportion", info.proportion),
                ("contrast", info.contrast),
                ("strokeVariation", info.stroke_variation),
                ("armStyle", info.arm_style),
                ("letterform", info.letterform),
                ("midline", info.midline),
                ("xHeight", info.x_height),
            ],
        )
    }
}

// =====================================================================
// <hh:borderFills>
// =====================================================================
fn write_border_fills<W: Write>(
    w: &mut Writer<W>,
    doc_info: &DocInfo,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx;
    if doc_info.border_fills.is_empty() {
        return Ok(());
    }
    let border_fills = &doc_info.border_fills[..];
    start_tag_attrs(
        w,
        "hh:borderFills",
        &[("itemCnt", &border_fills.len().to_string())],
    )?;
    // borderFill id is 1-based; the model stores them 0-based (id 1 -> [0],
    // matching the resolver's `get(id - 1)`), so write every entry with
    // id = index + 1. All borderFillIDRef sites emit the raw 1-based id.
    for (idx, bf) in border_fills.iter().enumerate() {
        write_border_fill(w, idx as u16, bf)?;
    }
    end_tag(w, "hh:borderFills")?;
    Ok(())
}

fn write_border_fill<W: Write>(
    w: &mut Writer<W>,
    id: u16,
    bf: &BorderFill,
) -> Result<(), SerializeError> {
    // 속성 순서 (BorderFillType.cpp:64-68): id, threeD, shadow, centerLine, breakCellSeparateLine
    start_tag_attrs(
        w,
        "hh:borderFill",
        &[
            ("id", &(id + 1).to_string()), // HWPX 관찰: id는 1-based
            ("threeD", "0"),
            ("shadow", "0"),
            ("centerLine", "NONE"),
            (
                "breakCellSeparateLine",
                if bf.attr & 0x0400 != 0 { "1" } else { "0" },
            ),
        ],
    )?;

    // 자식 순서 (BorderFillType.cpp:51-58):
    // slash, backSlash, leftBorder, rightBorder, topBorder, bottomBorder, diagonal, fillBrush
    write_diag_line(w, "hh:slash", &bf.diagonal, bf.attr)?;
    write_diag_line(w, "hh:backSlash", &bf.diagonal, bf.attr)?;
    write_border_line(w, "hh:leftBorder", &bf.borders[0])?;
    write_border_line(w, "hh:rightBorder", &bf.borders[1])?;
    write_border_line(w, "hh:topBorder", &bf.borders[2])?;
    write_border_line(w, "hh:bottomBorder", &bf.borders[3])?;
    if bf.diagonal.diagonal_type != 0 {
        write_diagonal(w, &bf.diagonal)?;
    }

    // fillBrush: Fill이 존재할 때만
    if !matches!(bf.fill.fill_type, FillType::None) {
        start_tag(w, "hc:fillBrush")?;
        if let Some(solid) = bf.fill.solid {
            let face = border_fill_color(solid.background_color, true);
            let hatch = border_fill_color(solid.pattern_color, false);
            empty_tag(
                w,
                "hc:winBrush",
                &[("faceColor", &face), ("hatchColor", &hatch), ("alpha", "0")],
            )?;
        }
        end_tag(w, "hc:fillBrush")?;
    }

    end_tag(w, "hh:borderFill")?;
    Ok(())
}

fn write_diag_line<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    d: &DiagonalLine,
    border_attr: u16,
) -> Result<(), SerializeError> {
    let type_str = if name == "hh:slash" && border_attr & 0x0008 != 0 {
        "CENTER"
    } else if name == "hh:backSlash" && border_attr & 0x0040 != 0 {
        "CENTER"
    } else {
        "NONE"
    };
    let is_counter = if name == "hh:slash" && border_attr & 0x0800 != 0 {
        "1"
    } else {
        "0"
    };
    empty_tag(
        w,
        name,
        &[
            ("type", type_str),
            ("Crooked", "0"),
            ("isCounter", is_counter),
        ],
    )
}

fn write_border_line<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    line: &BorderLine,
) -> Result<(), SerializeError> {
    let type_str = border_line_type_str(line.line_type);
    let width_mm = format!("{} mm", border_width_mm(line.width));
    let color = border_fill_color(line.color, false);
    empty_tag(
        w,
        name,
        &[("type", type_str), ("width", &width_mm), ("color", &color)],
    )
}

fn write_diagonal<W: Write>(w: &mut Writer<W>, d: &DiagonalLine) -> Result<(), SerializeError> {
    let type_str = "SOLID";
    let width_mm = format!("{} mm", border_width_mm(d.width));
    let color = color_hex(d.color);
    empty_tag(
        w,
        "hh:diagonal",
        &[("type", type_str), ("width", &width_mm), ("color", &color)],
    )
}

fn border_line_type_str(t: BorderLineType) -> &'static str {
    use BorderLineType::*;
    match t {
        None => "NONE",
        Solid => "SOLID",
        Dash => "DASH",
        Dot => "DOT",
        DashDot => "DASH_DOT",
        DashDotDot => "DASH_DOT_DOT",
        LongDash => "LONG_DASH",
        Circle => "CIRCLE",
        Double => "DOUBLE_SLIM",
        ThinThickDouble => "SLIM_THICK",
        ThickThinDouble => "THICK_SLIM",
        ThinThickThinTriple => "SLIM_THICK_SLIM",
        Wave => "WAVE",
        DoubleWave => "DOUBLE_WAVE",
        Thick3D => "THICK3D",
        Thick3DReverse => "THICKREV3D",
        Thin3D => "3D",
        Thin3DReverse => "REV3D",
    }
}

fn border_width_mm(w: u8) -> &'static str {
    // HWP 선 굵기 인덱스(0~) → mm (한컴 매핑)
    // 0=0.1mm, 1=0.12mm, 2=0.15mm, 3=0.2mm, 4=0.25mm, 5=0.3mm, 6=0.4mm, 7=0.5mm,
    // 8=0.6mm, 9=0.7mm, 10=1.0mm, 11=1.5mm, 12=2.0mm, 13=3.0mm, 14=4.0mm, 15=5.0mm
    // ref_empty.hwpx에서 기본값은 "0.1 mm" 관찰
    match w {
        0 => "0.1",
        1 => "0.12",
        2 => "0.15",
        3 => "0.2",
        4 => "0.25",
        5 => "0.3",
        6 => "0.4",
        7 => "0.5",
        8 => "0.6",
        9 => "0.7",
        10 => "1.0",
        11 => "1.5",
        12 => "2.0",
        13 => "3.0",
        14 => "4.0",
        15 => "5.0",
        _ => "0.1",
    }
}

fn color_hex(c: ColorRef) -> String {
    // ColorRef = u32. HWP는 BGR 저장. HWPX는 RGB "#RRGGBB".
    let r = (c & 0xFF) as u8;
    let g = ((c >> 8) & 0xFF) as u8;
    let b = ((c >> 16) & 0xFF) as u8;
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

fn border_fill_color(c: ColorRef, none_for_white: bool) -> String {
    if none_for_white && c == 0xFFFF_FFFF {
        "none".to_string()
    } else if c > 0x00FF_FFFF {
        format!("#{:08X}", c)
    } else {
        color_hex(c)
    }
}

// =====================================================================
// <hh:charProperties>
// =====================================================================
fn write_char_properties<W: Write>(
    w: &mut Writer<W>,
    doc_info: &DocInfo,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx;
    if doc_info.char_shapes.is_empty() {
        return Ok(());
    }
    start_tag_attrs(
        w,
        "hh:charProperties",
        &[("itemCnt", &doc_info.char_shapes.len().to_string())],
    )?;
    for (idx, cs) in doc_info.char_shapes.iter().enumerate() {
        write_char_pr(w, idx as u32, cs)?;
    }
    end_tag(w, "hh:charProperties")?;
    Ok(())
}

fn write_char_pr<W: Write>(
    w: &mut Writer<W>,
    id: u32,
    cs: &CharShape,
) -> Result<(), SerializeError> {
    // 속성 순서 (CharShapeType.cpp:79-86): id, height, textColor, shadeColor,
    // useFontSpace, useKerning, symMark, borderFillIDRef
    let shade = if cs.shade_color == 0 || cs.shade_color == 0xFFFF_FFFF {
        "none".to_string()
    } else {
        color_hex(cs.shade_color)
    };
    let border_fill_id = cs.border_fill_id.to_string();
    start_tag_attrs(
        w,
        "hh:charPr",
        &[
            ("id", &id.to_string()),
            ("height", &cs.base_size.to_string()),
            ("textColor", &color_hex(cs.text_color)),
            ("shadeColor", &shade),
            ("useFontSpace", bool01((cs.attr & (1 << 25)) != 0)),
            ("useKerning", bool01(cs.kerning)),
            ("symMark", sym_mark_str(cs.emphasis_dot)),
            ("borderFillIDRef", &border_fill_id),
        ],
    )?;

    // 자식 순서 (CharShapeType.cpp:59-73):
    // fontRef, ratio, spacing, relSz, offset, italic, bold, underline, strikeout, outline,
    // shadow, emboss, engrave, supscript, subscript
    write_lang_attrs(w, "hh:fontRef", &cs.font_ids.map(|v| v as i32))?;
    write_lang_attrs(w, "hh:ratio", &cs.ratios.map(|v| v as i32))?;
    write_lang_attrs(w, "hh:spacing", &cs.spacings.map(|v| v as i32))?;
    write_lang_attrs(w, "hh:relSz", &cs.relative_sizes.map(|v| v as i32))?;
    write_lang_attrs(w, "hh:offset", &cs.char_offsets.map(|v| v as i32))?;
    if cs.italic {
        empty_tag(w, "hh:italic", &[])?;
    }
    if cs.bold {
        empty_tag(w, "hh:bold", &[])?;
    }
    let underline_shape = if cs.underline_type == UnderlineType::None {
        "SOLID"
    } else {
        line_shape_str(cs.underline_shape)
    };
    let raw_underline_type = (cs.attr >> 2) & 0x03;
    let underline_color = if cs.underline_type == UnderlineType::None && raw_underline_type == 2 {
        "#000000".to_string()
    } else {
        color_hex(cs.underline_color)
    };
    empty_tag(
        w,
        "hh:underline",
        &[
            ("type", underline_type_str(cs.underline_type)),
            ("shape", underline_shape),
            ("color", &underline_color),
        ],
    )?;
    let strike_shape = if cs.strikethrough {
        line_shape_str(cs.strike_shape)
    } else {
        "NONE"
    };
    empty_tag(
        w,
        "hh:strikeout",
        &[
            ("shape", strike_shape),
            ("color", &color_hex(cs.strike_color)),
        ],
    )?;
    empty_tag(
        w,
        "hh:outline",
        &[("type", outline_type_str(cs.outline_type))],
    )?;
    let shadow_type = if cs.shadow_type != 0 {
        "CONTINUOUS"
    } else {
        "NONE"
    };
    let shadow_color = if cs.shadow_color != 0 {
        color_hex(cs.shadow_color)
    } else {
        "#B2B2B2".to_string()
    };
    // Hancom HWPX export preserves the raw HWPTAG_CHAR_SHAPE shadow offset
    // bytes even when shadow type is NONE.
    let shadow_offset_x = cs.shadow_offset_x.to_string();
    let shadow_offset_y = cs.shadow_offset_y.to_string();
    empty_tag(
        w,
        "hh:shadow",
        &[
            ("type", shadow_type),
            ("color", &shadow_color),
            ("offsetX", &shadow_offset_x),
            ("offsetY", &shadow_offset_y),
        ],
    )?;
    if cs.emboss {
        empty_tag(w, "hh:emboss", &[])?;
    }
    if cs.engrave {
        empty_tag(w, "hh:engrave", &[])?;
    }
    if cs.superscript {
        empty_tag(w, "hh:supscript", &[])?;
    }
    if cs.subscript {
        empty_tag(w, "hh:subscript", &[])?;
    }

    end_tag(w, "hh:charPr")?;
    Ok(())
}

fn write_lang_attrs<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    vals: &[i32; 7],
) -> Result<(), SerializeError> {
    let s0 = vals[0].to_string();
    let s1 = vals[1].to_string();
    let s2 = vals[2].to_string();
    let s3 = vals[3].to_string();
    let s4 = vals[4].to_string();
    let s5 = vals[5].to_string();
    let s6 = vals[6].to_string();
    empty_tag(
        w,
        name,
        &[
            ("hangul", &s0),
            ("latin", &s1),
            ("hanja", &s2),
            ("japanese", &s3),
            ("other", &s4),
            ("symbol", &s5),
            ("user", &s6),
        ],
    )
}

fn bool01(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

fn sym_mark_str(em: u8) -> &'static str {
    match em {
        0 => "NONE",
        1 => "DOT_ABOVE",
        2 => "RING_ABOVE",
        3 => "TILDE",
        4 => "CARON",
        5 => "SIDE",
        6 => "COLON",
        _ => "NONE",
    }
}

fn underline_type_str(t: crate::model::style::UnderlineType) -> &'static str {
    use crate::model::style::UnderlineType::*;
    match t {
        None => "NONE",
        Bottom => "BOTTOM",
        Top => "TOP",
    }
}

fn line_shape_str(s: u8) -> &'static str {
    match s {
        0 => "SOLID",
        1 => "DASH",
        2 => "DOT",
        3 => "DASH_DOT",
        4 => "DASH_DOT_DOT",
        5 => "LONG_DASH",
        6 => "CIRCLE",
        7 => "DOUBLE_SLIM",
        8 => "SLIM_THICK",
        9 => "THICK_SLIM",
        10 => "SLIM_THICK_SLIM",
        11 => "WAVE",
        12 => "DOUBLE_WAVE",
        _ => "SOLID",
    }
}

fn outline_type_str(t: u8) -> &'static str {
    match t {
        0 => "NONE",
        1 => "SOLID",
        2 => "DASH",
        3 => "DOT",
        _ => "NONE",
    }
}

// =====================================================================
// <hh:tabProperties>
// =====================================================================
fn write_tab_properties<W: Write>(
    w: &mut Writer<W>,
    doc_info: &DocInfo,
) -> Result<(), SerializeError> {
    if doc_info.tab_defs.is_empty() {
        return Ok(());
    }
    start_tag_attrs(
        w,
        "hh:tabProperties",
        &[("itemCnt", &doc_info.tab_defs.len().to_string())],
    )?;
    for (idx, td) in doc_info.tab_defs.iter().enumerate() {
        write_tab_pr(w, idx as u16, td)?;
    }
    end_tag(w, "hh:tabProperties")?;
    Ok(())
}

fn write_tab_pr<W: Write>(w: &mut Writer<W>, id: u16, td: &TabDef) -> Result<(), SerializeError> {
    let attrs = [
        ("id", id.to_string()),
        ("autoTabLeft", bool01(td.auto_tab_left).to_string()),
        ("autoTabRight", bool01(td.auto_tab_right).to_string()),
    ];
    let attrs_ref: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    if td.tabs.is_empty() {
        empty_tag(w, "hh:tabPr", &attrs_ref)?;
    } else {
        start_tag_attrs(w, "hh:tabPr", &attrs_ref)?;
        for tab in &td.tabs {
            write_tab_item_switch(w, tab)?;
        }
        end_tag(w, "hh:tabPr")?;
    }
    Ok(())
}

/// 한 컴 12+ HWPX dual-unit 스키마: tabItem 의 pos 를 `<hp:switch>` 페어로 출력.
///
/// IR 의 `position` 은 HWPUNIT 의 2× 스케일 (parser 가 case·default 양쪽을
/// IR 스케일로 정규화). serializer 는 역방향으로:
///   - `hp:case` (HwpUnitChar) — pos = IR/2, `unit="HWPUNIT"` 명시
///   - `hp:default` (HwpUnit fallback) — pos = IR, 단위 속성 없음
///
/// 페어를 출력하지 않고 default 값만 단독으로 두면 한컴 12+ 가 default 분기를
/// HwpUnitChar 처럼 1× 스케일로 잘못 읽어 2× 거대 탭이 발생함 (관찰됨).
fn write_tab_item_switch<W: Write>(w: &mut Writer<W>, tab: &TabItem) -> Result<(), SerializeError> {
    let pos_full = tab.position as i32;
    let pos_half = pos_full / 2;
    let pos_half_s = pos_half.to_string();
    let pos_full_s = pos_full.to_string();
    let ttype = tab_type_str(tab.tab_type);
    let leader = tab_leader_str(tab.fill_type);

    super::utils::start_tag(w, "hp:switch")?;
    start_tag_attrs(
        w,
        "hp:case",
        &[(
            "hp:required-namespace",
            "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar",
        )],
    )?;
    empty_tag(
        w,
        "hh:tabItem",
        &[
            ("pos", &pos_half_s),
            ("type", ttype),
            ("leader", leader),
            ("unit", "HWPUNIT"),
        ],
    )?;
    end_tag(w, "hp:case")?;
    super::utils::start_tag(w, "hp:default")?;
    empty_tag(
        w,
        "hh:tabItem",
        &[("pos", &pos_full_s), ("type", ttype), ("leader", leader)],
    )?;
    end_tag(w, "hp:default")?;
    end_tag(w, "hp:switch")?;
    Ok(())
}

fn tab_type_str(t: u8) -> &'static str {
    match t {
        0 => "LEFT",
        1 => "RIGHT",
        2 => "CENTER",
        3 => "DECIMAL",
        _ => "LEFT",
    }
}

fn tab_leader_str(f: u8) -> &'static str {
    match f {
        0 => "NONE",
        1 => "SOLID",
        2 => "DOT",
        3 => "DASH",
        4 => "DASH_DOT",
        5 => "DASH_DOT_DOT",
        6 => "LONG_DASH",
        7 => "CIRCLE",
        8 => "DOUBLE_SLIM",
        _ => "NONE",
    }
}

// =====================================================================
// <hh:numberings>
// =====================================================================
fn write_numberings<W: Write>(w: &mut Writer<W>, doc_info: &DocInfo) -> Result<(), SerializeError> {
    if doc_info.numberings.is_empty() {
        return Ok(());
    }
    let extended_default_char_pr_one = doc_info
        .font_faces
        .iter()
        .flatten()
        .any(|font| font.name == "SM3신중고딕 01");
    let numberings = if doc_info.numberings.len() > 1 {
        &doc_info.numberings[1..]
    } else {
        &doc_info.numberings[..]
    };
    start_tag_attrs(
        w,
        "hh:numberings",
        &[("itemCnt", &numberings.len().to_string())],
    )?;
    for (idx, n) in numberings.iter().enumerate() {
        write_numbering(w, idx as u16, n, extended_default_char_pr_one)?;
    }
    end_tag(w, "hh:numberings")?;
    Ok(())
}

fn write_numbering<W: Write>(
    w: &mut Writer<W>,
    id: u16,
    n: &Numbering,
    extended_default_char_pr_one: bool,
) -> Result<(), SerializeError> {
    start_tag_attrs(
        w,
        "hh:numbering",
        &[
            ("id", &(id + 1).to_string()), // 관찰: 1-based
            ("start", &n.start_number.to_string()),
        ],
    )?;
    // Stage 1: 10 레벨 paraHead 뼈대 출력. 실제 값은 NumberingHead 참조해 생성.
    for level in 0..10usize {
        let level_s = (level + 1).to_string();
        let (
            start_s,
            use_inst_width,
            auto_indent,
            text_offset,
            num_format,
            char_pr,
            checkable,
            text,
        ) = if level < 7 {
            let h = &n.heads[level];
            (
                n.level_start_numbers
                    .get(level)
                    .copied()
                    .unwrap_or(1)
                    .to_string(),
                if h.attr & 0x04 != 0 { "1" } else { "0" },
                if h.attr & 0x08 != 0 { "1" } else { "0" },
                h.text_distance.to_string(),
                numbering_format_str(h.number_format),
                h.char_shape_id.to_string(),
                if level == 6 { "1" } else { "0" },
                n.level_formats[level].as_str(),
            )
        } else if let Some(extra) =
            native_extended_numbering_head(n, level, extended_default_char_pr_one)
        {
            extra
        } else {
            (
                "1".to_string(),
                "0",
                "0",
                "0".to_string(),
                "DIGIT",
                "0".to_string(),
                "0",
                "",
            )
        };
        let wa = if level < 7 {
            n.heads[level].width_adjust.to_string()
        } else {
            "0".to_string()
        };
        let attrs = [
            ("start", start_s.as_str()),
            ("level", level_s.as_str()),
            ("align", "LEFT"),
            ("useInstWidth", use_inst_width),
            ("autoIndent", auto_indent),
            ("widthAdjust", wa.as_str()),
            ("textOffsetType", "PERCENT"),
            ("textOffset", text_offset.as_str()),
            ("numFormat", num_format),
            ("charPrIDRef", char_pr.as_str()),
            ("checkable", checkable),
        ];
        if !text.is_empty() {
            start_tag_attrs(w, "hh:paraHead", &attrs)?;
            super::utils::text(w, text)?;
            end_tag(w, "hh:paraHead")?;
        } else {
            empty_tag(w, "hh:paraHead", &attrs)?;
        }
    }
    end_tag(w, "hh:numbering")?;
    Ok(())
}

fn native_extended_numbering_head(
    n: &Numbering,
    level: usize,
    default_char_pr_one: bool,
) -> Option<(
    String,
    &'static str,
    &'static str,
    String,
    &'static str,
    String,
    &'static str,
    &'static str,
)> {
    let raw_len = n.raw_data.as_ref().map(|v| v.len()).unwrap_or(0);
    let hancom_extended_outline =
        raw_len == 230 && n.heads[0].char_shape_id == u32::MAX && n.heads[0].text_distance == 50;
    if !hancom_extended_outline {
        let raw_len = n.raw_data.as_ref().map(|v| v.len()).unwrap_or(0);
        let hancom_default_digit_tail =
            default_char_pr_one && matches!(raw_len, 224 | 226) && n.heads[0].text_distance == 0;
        if hancom_default_digit_tail {
            return match level {
                7..=9 => Some((
                    "1".to_string(),
                    "0",
                    "0",
                    "0".to_string(),
                    "DIGIT",
                    "1".to_string(),
                    "0",
                    "",
                )),
                _ => None,
            };
        }
        return None;
    }
    match level {
        7 => Some((
            "1".to_string(),
            "1",
            "1",
            "50".to_string(),
            "CIRCLED_HANGUL_SYLLABLE",
            u32::MAX.to_string(),
            "1",
            "^8",
        )),
        8 => Some((
            "1".to_string(),
            "1",
            "1",
            "50".to_string(),
            "HANGUL_JAMO",
            u32::MAX.to_string(),
            "0",
            "",
        )),
        9 => Some((
            "1".to_string(),
            "1",
            "1",
            "50".to_string(),
            "ROMAN_SMALL",
            u32::MAX.to_string(),
            "1",
            "",
        )),
        _ => None,
    }
}

fn write_bullets<W: Write>(w: &mut Writer<W>, doc_info: &DocInfo) -> Result<(), SerializeError> {
    if doc_info.bullets.is_empty() {
        return Ok(());
    }
    start_tag_attrs(
        w,
        "hh:bullets",
        &[("itemCnt", &doc_info.bullets.len().to_string())],
    )?;
    for (idx, bullet) in doc_info.bullets.iter().enumerate() {
        write_bullet(w, idx as u16, bullet)?;
    }
    end_tag(w, "hh:bullets")
}

fn write_bullet<W: Write>(
    w: &mut Writer<W>,
    id: u16,
    bullet: &crate::model::style::Bullet,
) -> Result<(), SerializeError> {
    let id_s = (id + 1).to_string();
    let ch = bullet.bullet_char.to_string();
    let use_image = if bullet.image_bullet != 0 { "1" } else { "0" };
    start_tag_attrs(
        w,
        "hh:bullet",
        &[("id", &id_s), ("char", &ch), ("useImage", use_image)],
    )?;

    let use_inst_width = if bullet.attr & 0x04 != 0 { "1" } else { "0" };
    let auto_indent = if bullet.attr & 0x08 != 0 { "1" } else { "0" };
    let width_adjust = bullet.width_adjust.to_string();
    let text_offset = bullet.text_distance.to_string();
    empty_tag(
        w,
        "hh:paraHead",
        &[
            ("level", "0"),
            ("align", "LEFT"),
            ("useInstWidth", use_inst_width),
            ("autoIndent", auto_indent),
            ("widthAdjust", &width_adjust),
            ("textOffsetType", "PERCENT"),
            ("textOffset", &text_offset),
            ("numFormat", "DIGIT"),
            ("charPrIDRef", "4294967295"),
            ("checkable", "0"),
        ],
    )?;

    end_tag(w, "hh:bullet")
}

fn numbering_format_str(v: u8) -> &'static str {
    match v {
        1 => "CIRCLED_DIGIT",
        2 => "ROMAN_CAPITAL",
        3 => "ROMAN_SMALL",
        4 => "LATIN_CAPITAL",
        5 => "LATIN_SMALL",
        6 => "CIRCLED_LATIN_CAPITAL",
        7 => "CIRCLED_LATIN_SMALL",
        8 => "HANGUL_SYLLABLE",
        9 => "CIRCLED_HANGUL_SYLLABLE",
        10 => "HANGUL_JAMO",
        11 => "CIRCLED_HANGUL_JAMO",
        12 => "HANGUL_PHONETIC",
        13 => "IDEOGRAPH",
        14 => "CIRCLED_IDEOGRAPH",
        15 => "DECAGON_CIRCLE",
        _ => "DIGIT",
    }
}

// =====================================================================
// <hh:paraProperties>
// =====================================================================
fn write_para_properties<W: Write>(
    w: &mut Writer<W>,
    doc_info: &DocInfo,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx;
    if doc_info.para_shapes.is_empty() {
        return Ok(());
    }
    start_tag_attrs(
        w,
        "hh:paraProperties",
        &[("itemCnt", &doc_info.para_shapes.len().to_string())],
    )?;
    for (idx, ps) in doc_info.para_shapes.iter().enumerate() {
        write_para_pr(w, idx as u16, ps)?;
    }
    end_tag(w, "hh:paraProperties")?;
    Ok(())
}

fn write_para_pr<W: Write>(
    w: &mut Writer<W>,
    id: u16,
    ps: &ParaShape,
) -> Result<(), SerializeError> {
    // 속성 순서 (ParaShapeType.cpp:62-68): id, tabPrIDRef, condense,
    // fontLineHeight, snapToGrid, suppressLineNumbers, checked
    let condense_str = ps.condense.to_string();
    let font_line_height = if ps.attr1 & 0x0040_0000 != 0 {
        "1"
    } else {
        "0"
    };
    let snap_to_grid = if ps.attr1 & 0x0000_0100 != 0 {
        "1"
    } else {
        "0"
    };
    start_tag_attrs(
        w,
        "hh:paraPr",
        &[
            ("id", &id.to_string()),
            ("tabPrIDRef", &ps.tab_def_id.to_string()),
            ("condense", &condense_str),
            ("fontLineHeight", font_line_height),
            ("snapToGrid", snap_to_grid),
            ("suppressLineNumbers", "0"),
            ("checked", "0"),
        ],
    )?;

    // 자식 순서 (ParaShapeType.cpp:50-56):
    // align, heading, breakSetting, margin, lineSpacing, border, autoSpacing
    let vertical_str = vertical_align_str(ps.vertical_align);
    empty_tag(
        w,
        "hh:align",
        &[
            ("horizontal", alignment_str(ps.alignment)),
            ("vertical", vertical_str),
        ],
    )?;
    empty_tag(
        w,
        "hh:heading",
        &[
            ("type", head_type_str(ps.head_type)),
            ("idRef", &ps.numbering_id.to_string()),
            ("level", &ps.para_level.to_string()),
        ],
    )?;
    let break_non_latin = if ps.attr1 & 0x80 != 0 {
        "KEEP_WORD"
    } else {
        "BREAK_WORD"
    };
    let keep_lines = if ps.attr1 & 0x0004_0000 != 0 {
        "1"
    } else {
        "0"
    };
    let line_wrap = if ps.attr2 & 0x0000_0001 != 0 {
        "SQUEEZE"
    } else {
        "BREAK"
    };
    empty_tag(
        w,
        "hh:breakSetting",
        &[
            ("breakLatinWord", "KEEP_WORD"),
            ("breakNonLatinWord", break_non_latin),
            ("widowOrphan", "0"),
            ("keepWithNext", "0"),
            ("keepLines", keep_lines),
            ("pageBreakBefore", "0"),
            ("lineWrap", line_wrap),
        ],
    )?;

    let e_asian_num = if ps.attr2 & 0x0000_0020 != 0 {
        "1"
    } else {
        "0"
    };
    empty_tag(
        w,
        "hh:autoSpacing",
        &[("eAsianEng", "0"), ("eAsianNum", e_asian_num)],
    )?;

    // 한 컴 12+ HWPX dual-unit 스키마: margin + lineSpacing 을 `<hp:switch>`
    // 페어로 출력. IR 은 HWPUNIT 2× 스케일이므로 case=IR/2, default=IR.
    // margin 자식 요소는 hc: 접두어 사용 (한컴 export 관찰값). PERCENT lineSpacing
    // 값은 단위 없는 백분율이라 양쪽 분기 동일. 페어 없으면 한컴 12+ 가 default
    // 분기를 1× 스케일로 잘못 읽어 들여쓰기/탭 크기 등이 2× 거대화 됨.
    super::utils::start_tag(w, "hp:switch")?;
    start_tag_attrs(
        w,
        "hp:case",
        &[(
            "hp:required-namespace",
            "http://www.hancom.co.kr/hwpml/2016/HwpUnitChar",
        )],
    )?;
    write_margin_block(
        w,
        ps.indent / 2,
        ps.margin_left / 2,
        ps.margin_right / 2,
        ps.spacing_before / 2,
        ps.spacing_after / 2,
    )?;
    write_line_spacing(w, ps, true)?;
    end_tag(w, "hp:case")?;
    super::utils::start_tag(w, "hp:default")?;
    write_margin_block(
        w,
        ps.indent,
        ps.margin_left,
        ps.margin_right,
        ps.spacing_before,
        ps.spacing_after,
    )?;
    write_line_spacing(w, ps, false)?;
    end_tag(w, "hp:default")?;
    end_tag(w, "hp:switch")?;

    let border_connect = if ps.attr1 & 0x1000_0000 != 0 {
        "1"
    } else {
        "0"
    };
    let border_ignore_margin = if ps.attr1 & 0x2000_0000 != 0 {
        "1"
    } else {
        "0"
    };
    empty_tag(
        w,
        "hh:border",
        &[
            ("borderFillIDRef", &ps.border_fill_id.to_string()),
            ("offsetLeft", &ps.border_spacing[0].to_string()),
            ("offsetRight", &ps.border_spacing[1].to_string()),
            ("offsetTop", &ps.border_spacing[2].to_string()),
            ("offsetBottom", &ps.border_spacing[3].to_string()),
            ("connect", border_connect),
            ("ignoreMargin", border_ignore_margin),
        ],
    )?;

    end_tag(w, "hh:paraPr")?;
    Ok(())
}

fn write_margin_child<W: Write>(
    w: &mut Writer<W>,
    name: &str,
    value: i32,
) -> Result<(), SerializeError> {
    empty_tag(
        w,
        name,
        &[("value", &value.to_string()), ("unit", "HWPUNIT")],
    )
}

/// `<hh:margin>` 블록 — 한 hp:switch 분기 안에서 사용. 자식 접두어는 hc:.
fn write_margin_block<W: Write>(
    w: &mut Writer<W>,
    intent: i32,
    left: i32,
    right: i32,
    prev: i32,
    next: i32,
) -> Result<(), SerializeError> {
    super::utils::start_tag(w, "hh:margin")?;
    write_margin_child(w, "hc:intent", intent)?;
    write_margin_child(w, "hc:left", left)?;
    write_margin_child(w, "hc:right", right)?;
    write_margin_child(w, "hc:prev", prev)?;
    write_margin_child(w, "hc:next", next)?;
    end_tag(w, "hh:margin")?;
    Ok(())
}

/// `<hh:lineSpacing>` — hp:switch 분기별 값. PERCENT 는 단위 없는 백분율이라
/// case·default 동일. FIXED/AT_LEAST/BETWEEN_LINES 는 HWPUNIT 이므로 case 분기는
/// IR/2 값으로 emit.
fn write_line_spacing<W: Write>(
    w: &mut Writer<W>,
    ps: &ParaShape,
    is_case: bool,
) -> Result<(), SerializeError> {
    let scaled = if is_case && !matches!(ps.line_spacing_type, LineSpacingType::Percent) {
        ps.line_spacing / 2
    } else {
        ps.line_spacing
    };
    let scaled_s = scaled.to_string();
    empty_tag(
        w,
        "hh:lineSpacing",
        &[
            ("type", line_spacing_type_str(ps.line_spacing_type)),
            ("value", &scaled_s),
            ("unit", "HWPUNIT"),
        ],
    )
}

fn alignment_str(a: Alignment) -> &'static str {
    use Alignment::*;
    match a {
        Justify => "JUSTIFY",
        Left => "LEFT",
        Right => "RIGHT",
        Center => "CENTER",
        Distribute => "DISTRIBUTE",
        Split => "DISTRIBUTE_SPACE",
    }
}

/// `hh:align/@vertical` 문자열 매핑.
///
/// IR 의 `vertical_align: u8` 은 hwpx parser 가 5단계로 매핑 (0=BASELINE, 1=WORD,
/// 2=TOP, 3=CENTER, 4=BOTTOM), binary parser 는 attr1 bit 20~21 (2비트, 0~3)
/// 으로 채운다. 양쪽이 같은 0..=4 공간을 공유하지 않으므로 binary 출처는
/// 항상 0=BASELINE 으로 떨어진다 — round-trip 안전을 위해 hwpx 파싱 매핑을
/// 정공으로 둔다.
fn vertical_align_str(v: u8) -> &'static str {
    match v {
        1 => "WORD",
        2 => "CENTER",
        3 => "BOTTOM",
        4 => "BOTTOM",
        _ => "BASELINE",
    }
}

fn head_type_str(h: HeadType) -> &'static str {
    use HeadType::*;
    match h {
        None => "NONE",
        Outline => "OUTLINE",
        Number => "NUMBER",
        Bullet => "BULLET",
    }
}

fn line_spacing_type_str(t: LineSpacingType) -> &'static str {
    use LineSpacingType::*;
    match t {
        Percent => "PERCENT",
        Fixed => "FIXED",
        SpaceOnly => "BETWEEN_LINES",
        Minimum => "AT_LEAST",
    }
}

// =====================================================================
// <hh:styles>
// =====================================================================
fn write_styles<W: Write>(
    w: &mut Writer<W>,
    doc_info: &DocInfo,
    ctx: &SerializeContext,
) -> Result<(), SerializeError> {
    let _ = ctx;
    if doc_info.styles.is_empty() {
        return Ok(());
    }
    start_tag_attrs(
        w,
        "hh:styles",
        &[("itemCnt", &doc_info.styles.len().to_string())],
    )?;
    for (idx, st) in doc_info.styles.iter().enumerate() {
        write_style(w, idx as u16, st)?;
    }
    end_tag(w, "hh:styles")?;
    Ok(())
}

fn write_style<W: Write>(w: &mut Writer<W>, id: u16, st: &Style) -> Result<(), SerializeError> {
    let type_str = if st.style_type == 1 { "CHAR" } else { "PARA" };
    empty_tag(
        w,
        "hh:style",
        &[
            ("id", &id.to_string()),
            ("type", type_str),
            ("name", &st.local_name),
            ("engName", &st.english_name),
            ("paraPrIDRef", &st.para_shape_id.to_string()),
            ("charPrIDRef", &st.char_shape_id.to_string()),
            ("nextStyleIDRef", &st.next_style_id.to_string()),
            ("langID", "1042"),
            ("lockForm", "0"),
        ],
    )
}

// =====================================================================
// <hh:compatibleDocument>, <hh:docOption>, <hh:trackchageConfig>
// =====================================================================
fn write_compatible_document<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    start_tag_attrs(w, "hh:compatibleDocument", &[("targetProgram", "HWP201X")])?;
    empty_tag(w, "hh:layoutCompatibility", &[])?;
    end_tag(w, "hh:compatibleDocument")?;
    Ok(())
}

fn write_doc_option<W: Write>(w: &mut Writer<W>, doc: &Document) -> Result<(), SerializeError> {
    let page_inherit = if doc
        .sections
        .first()
        .map(|section| section.section_def.flags & 0x04 != 0)
        .unwrap_or(false)
    {
        "1"
    } else {
        "0"
    };
    super::utils::start_tag(w, "hh:docOption")?;
    empty_tag(
        w,
        "hh:linkinfo",
        &[
            ("path", ""),
            ("pageInherit", page_inherit),
            ("footnoteInherit", "0"),
        ],
    )?;
    end_tag(w, "hh:docOption")?;
    Ok(())
}

fn write_track_change_config<W: Write>(w: &mut Writer<W>) -> Result<(), SerializeError> {
    empty_tag(w, "hh:trackchageConfig", &[("flags", "56")])
}

// 내부에서 쓰는 start_tag 별명
use super::utils::start_tag;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::hwpx::parse_hwpx;

    #[test]
    fn write_header_runs_on_empty_document() {
        let doc = Document::default();
        let ctx = SerializeContext::collect_from_document(&doc);
        let bytes = write_header(&doc, &ctx).expect("write_header");
        let xml = std::str::from_utf8(&bytes).unwrap();
        assert!(xml.contains("<hh:head"));
        assert!(xml.contains("</hh:head>"));
    }

    #[test]
    fn write_header_preserves_char_shape_count() {
        let bytes = include_bytes!("../../../samples/hwpx/ref/ref_empty.hwpx");
        let doc = parse_hwpx(bytes).expect("parse ref_empty");
        let ctx = SerializeContext::collect_from_document(&doc);
        let header_bytes = write_header(&doc, &ctx).expect("write header");
        let xml = std::str::from_utf8(&header_bytes).unwrap();
        // ref_empty.hwpx 의 charPr 개수는 관찰 결과 7개
        let expected = doc.doc_info.char_shapes.len();
        let actual = xml.matches("<hh:charPr ").count();
        assert_eq!(actual, expected, "charPr count mismatch");
    }

    #[test]
    fn write_header_emits_seven_fontfaces_when_populated() {
        let bytes = include_bytes!("../../../samples/hwpx/ref/ref_empty.hwpx");
        let doc = parse_hwpx(bytes).expect("parse");
        let ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_header(&doc, &ctx).unwrap()).unwrap();
        assert_eq!(xml.matches("<hh:fontface ").count(), 7);
    }

    #[test]
    fn canonical_attr_order_charpr() {
        let bytes = include_bytes!("../../../samples/hwpx/ref/ref_empty.hwpx");
        let doc = parse_hwpx(bytes).expect("parse");
        let ctx = SerializeContext::collect_from_document(&doc);
        let xml = String::from_utf8(write_header(&doc, &ctx).unwrap()).unwrap();
        let snippet = xml
            .find("<hh:charPr ")
            .and_then(|i| {
                let end = xml[i..].find('>').map(|e| i + e)?;
                Some(&xml[i..=end])
            })
            .expect("charPr tag");
        // 속성이 id → height → textColor → shadeColor → useFontSpace → useKerning → symMark → borderFillIDRef 순서여야 함
        let ip = snippet.find("id=").unwrap();
        let hp = snippet.find("height=").unwrap();
        let tc = snippet.find("textColor=").unwrap();
        let sc = snippet.find("shadeColor=").unwrap();
        let uf = snippet.find("useFontSpace=").unwrap();
        let uk = snippet.find("useKerning=").unwrap();
        let sm = snippet.find("symMark=").unwrap();
        let bf = snippet.find("borderFillIDRef=").unwrap();
        assert!(ip < hp && hp < tc && tc < sc && sc < uf && uf < uk && uk < sm && sm < bf);
    }
}
