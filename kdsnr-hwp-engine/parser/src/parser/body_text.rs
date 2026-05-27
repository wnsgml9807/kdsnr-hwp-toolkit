//! BodyText 섹션 파싱
//!
//! BodyText/Section{N} 스트림의 레코드를 파싱하여 Section(문단 목록)으로 변환.
//! 레코드의 level 필드로 부모-자식 관계를 결정한다.
//!
//! 레코드 트리 구조 예시:
//! ```text
//! PARA_HEADER (level 0)
//!   PARA_TEXT (level 1)
//!   PARA_CHAR_SHAPE (level 1)
//!   PARA_LINE_SEG (level 1)
//!   CTRL_HEADER (level 1)  ← secd, cold, tbl, 등
//!     PAGE_DEF (level 2)
//!     FOOTNOTE_SHAPE (level 2)
//!     ...
//! ```

use super::byte_reader::ByteReader;
use super::record::Record;
use super::tags;

use crate::model::control::{Control, FieldType, UnknownControl};
use crate::model::document::{RawRecord, Section, SectionDef};
use crate::model::footnote::FootnoteShape;
use crate::model::header_footer::{HeaderFooterApply, MasterPage};
use crate::model::page::{
    BindingMethod, ColumnDef, ColumnDirection, ColumnType, PageBorderFill, PageDef,
};
use crate::model::paragraph::{
    CharShapeRef, ColumnBreakType, FieldRange, LineSeg, Paragraph, ParagraphItem, RangeTag,
};

/// BodyText 파싱 에러
#[derive(Debug)]
pub enum BodyTextError {
    RecordError(String),
    ParseError(String),
}

impl std::fmt::Display for BodyTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyTextError::RecordError(e) => write!(f, "BodyText 레코드 오류: {}", e),
            BodyTextError::ParseError(e) => write!(f, "BodyText 파싱 오류: {}", e),
        }
    }
}

impl std::error::Error for BodyTextError {}

/// 섹션 레코드 데이터를 파싱하여 Section으로 변환
///
/// data: 압축 해제된(배포용은 복호화+해제된) 레코드 바이트 스트림
pub fn parse_body_text_section(data: &[u8]) -> Result<Section, BodyTextError> {
    let records = Record::read_all(data).map_err(|e| BodyTextError::RecordError(e.to_string()))?;

    let mut section = Section::default();
    let mut idx = 0;

    while idx < records.len() {
        if records[idx].tag_id == tags::HWPTAG_PARA_HEADER && records[idx].level == 0 {
            let base_level = records[idx].level;
            let start = idx;
            idx += 1;

            // 자식 레코드 수집 (level > base_level).
            //
            // HWP 바이너리는 마지막 본문 문단 뒤에 확장 바탕쪽을
            // LIST_HEADER(level = base + 1)로 이어 붙이는 경우가 있다.
            // 이것을 마지막 문단의 자식으로 삼으면 header/footer/endnote
            // 계열 내용이 본문 section0.xml로 새어 나간다.
            while idx < records.len() && records[idx].level > base_level {
                if records[idx].tag_id == tags::HWPTAG_LIST_HEADER
                    && records[idx].level == base_level + 1
                {
                    break;
                }
                idx += 1;
            }

            let para_records = &records[start..idx];
            let paragraph = parse_paragraph(para_records)?;

            // 구역 정의 추출
            for ctrl in &paragraph.controls {
                if let Control::SectionDef(sd) = ctrl {
                    section.section_def = (**sd).clone();
                }
            }

            section.paragraphs.push(paragraph);
        } else {
            idx += 1;
        }
    }

    // 확장 바탕쪽 파싱: 마지막 문단 이후의 LIST_HEADER (level=1)
    // HWP 바이너리에서 확장 바탕쪽(마지막 쪽, 임의 쪽)은 Section 스트림 끝에 저장되지만,
    // level=1로 태그되어 마지막 문단의 자식으로 오인됨.
    // 전체 레코드를 재스캔하여 마지막 PARA_HEADER(level=0) 이후의 LIST_HEADER(level=1)를 추출.
    {
        let all_records = Record::read_all(data).unwrap_or_default();
        let last_para0_idx = all_records
            .iter()
            .rposition(|r| r.tag_id == tags::HWPTAG_PARA_HEADER && r.level == 0);
        if let Some(lp) = last_para0_idx {
            // 마지막 문단의 본래 자식 레코드 범위 결정 (PARA_TEXT, PARA_CHAR_SHAPE 등)
            // LIST_HEADER(level=1)가 나타나면 그 이후는 확장 바탕쪽
            let mut scan = lp + 1;
            while scan < all_records.len() {
                if all_records[scan].tag_id == tags::HWPTAG_LIST_HEADER
                    && all_records[scan].level == 1
                {
                    // 확장 바탕쪽 발견
                    let tail: Vec<RawRecord> = all_records[scan..]
                        .iter()
                        .map(|r| RawRecord {
                            tag_id: r.tag_id,
                            level: r.level,
                            data: r.data.clone(),
                        })
                        .collect();
                    let ext_mps = parse_master_pages_from_raw(&tail);
                    section.section_def.master_pages.extend(ext_mps);
                    break;
                }
                scan += 1;
            }
        }

        let memo_lists = parse_memo_lists_from_records(&all_records);
        if !memo_lists.is_empty() {
            attach_memo_lists_to_fields(&mut section, memo_lists);
        }
    }

    Ok(section)
}

/// 문단 레코드 그룹에서 Paragraph 구성
///
/// records[0] = PARA_HEADER, records[1..] = 자식 레코드
pub fn parse_paragraph(records: &[Record]) -> Result<Paragraph, BodyTextError> {
    if records.is_empty() || records[0].tag_id != tags::HWPTAG_PARA_HEADER {
        return Err(BodyTextError::ParseError("PARA_HEADER 레코드 없음".into()));
    }

    let mut para = parse_para_header(&records[0].data);
    let base_level = records[0].level;

    let mut i = 1;
    while i < records.len() {
        let record = &records[i];

        // 직접 자식만 처리 (level == base_level + 1)
        if record.level != base_level + 1 {
            i += 1;
            continue;
        }

        match record.tag_id {
            tags::HWPTAG_PARA_TEXT => {
                let (text, offsets, field_ranges, tab_ext, items) = parse_para_text(&record.data);
                para.text = text;
                para.char_offsets = offsets;
                para.field_ranges = field_ranges;
                para.tab_extended = tab_ext;
                para.items = items;
                para.has_para_text = true;
            }
            tags::HWPTAG_PARA_CHAR_SHAPE => {
                para.char_shapes = parse_para_char_shape(&record.data);
            }
            tags::HWPTAG_PARA_LINE_SEG => {
                para.line_segs = parse_para_line_seg(&record.data);
            }
            tags::HWPTAG_PARA_RANGE_TAG => {
                para.range_tags = parse_para_range_tag(&record.data);
            }
            tags::HWPTAG_CTRL_HEADER => {
                // 컨트롤의 자식 레코드 범위 수집
                let ctrl_start = i;
                i += 1;
                while i < records.len() && records[i].level > base_level + 1 {
                    i += 1;
                }
                let ctrl_records = &records[ctrl_start..i];
                let mut control = parse_ctrl_header(ctrl_records);

                // CTRL_DATA 레코드 추출 (라운드트립 보존용)
                // 중첩 CTRL_HEADER 이전까지만 검색하여 내부 컨트롤의 CTRL_DATA 혼입 방지
                let ctrl_data = ctrl_records[1..]
                    .iter()
                    .take_while(|r| r.tag_id != tags::HWPTAG_CTRL_HEADER)
                    .find(|r| r.tag_id == tags::HWPTAG_CTRL_DATA)
                    .map(|r| r.data.clone());

                // CTRL_DATA에서 필드 이름 추출 → Field.ctrl_data_name에 설정
                if let Control::Field(ref mut field) = control {
                    if let Some(ref cd) = ctrl_data {
                        field.ctrl_data_name = parse_ctrl_data_field_name(cd);
                    }
                }

                // CTRL_DATA에서 책갈피 이름 추출 (HWP 스펙: 책갈피 이름은 HWPTAG_CTRL_DATA의 ParameterSet에 저장)
                if let Control::Bookmark(ref mut bm) = control {
                    if let Some(ref cd) = ctrl_data {
                        if let Some(name) = parse_ctrl_data_field_name(cd) {
                            bm.name = name;
                        }
                    }
                }

                para.controls.push(control);
                para.ctrl_data_records.push(ctrl_data);
                continue; // i는 이미 전진됨
            }
            _ => {}
        }

        i += 1;
    }

    Ok(para)
}

/// PARA_HEADER 바이너리 데이터 파싱
///
/// 레이아웃 (최소 12바이트, 실제로 22~24바이트):
/// - u32: nChars (bit 31은 플래그)
/// - u32: controlMask
/// - u16: paraShapeId
/// - u8:  styleId
/// - u8:  breakType (bits 0-2)
/// - [이후 10~12바이트: numCharShapes, numRangeTags, numLineSegs, instanceId 등]
fn parse_para_header(data: &[u8]) -> Paragraph {
    let mut r = ByteReader::new(data);
    let mut para = Paragraph::default();

    let n_chars_raw = r.read_u32().unwrap_or(0);
    para.char_count = n_chars_raw & 0x7FFFFFFF;
    para.char_count_msb = n_chars_raw & 0x80000000 != 0;

    para.control_mask = r.read_u32().unwrap_or(0);
    para.para_shape_id = r.read_u16().unwrap_or(0);
    para.style_id = r.read_u8().unwrap_or(0);

    // 단 나누기 종류 (표 61: 비트 플래그)
    // 0x01 = 구역 나누기, 0x02 = 다단 나누기, 0x04 = 쪽 나누기, 0x08 = 단 나누기
    let break_val = r.read_u8().unwrap_or(0);
    para.raw_break_type = break_val;
    para.column_type = if break_val & 0x04 != 0 {
        ColumnBreakType::Page
    } else if break_val & 0x08 != 0 {
        ColumnBreakType::Column
    } else if break_val & 0x01 != 0 {
        ColumnBreakType::Section
    } else if break_val & 0x02 != 0 {
        ColumnBreakType::MultiColumn
    } else {
        ColumnBreakType::None
    };

    // 12바이트 이후 추가 데이터 보존 (라운드트립용)
    if data.len() > 12 {
        para.raw_header_extra = data[12..].to_vec();
    }

    para
}

/// PARA_TEXT 바이너리 데이터에서 텍스트 추출
///
/// HWP의 텍스트는 UTF-16LE로 저장되며, 0x0000~0x001F 범위는 컨트롤 문자.
/// - 확장 컨트롤 문자: 8 code unit (16바이트) 차지
/// - 인라인 컨트롤 문자: 1 code unit (2바이트) 차지
fn parse_para_text(
    data: &[u8],
) -> (
    String,
    Vec<u32>,
    Vec<FieldRange>,
    Vec<[u16; 7]>,
    Vec<ParagraphItem>,
) {
    let mut text = String::new();
    let mut char_offsets: Vec<u32> = Vec::new();
    let mut field_ranges: Vec<FieldRange> = Vec::new();
    let mut tab_extended: Vec<[u16; 7]> = Vec::new();
    let mut items: Vec<ParagraphItem> = Vec::new();
    let mut item_text = String::new();
    let mut pos = 0;
    // 확장 컨트롤(extended) 카운터 → controls[] 인덱스와 1:1 대응
    let mut ctrl_idx: usize = 0;
    // text 문자열 내 문자 수 (바이트가 아닌 char 카운트)
    let mut char_count: usize = 0;
    // 현재 열린 필드 범위 스택 (중첩 필드 지원)
    let mut field_stack: Vec<(usize, usize)> = Vec::new(); // (start_char_idx, control_idx)

    let flush_item_text = |items: &mut Vec<ParagraphItem>, item_text: &mut String| {
        if !item_text.is_empty() {
            items.push(ParagraphItem::Text(std::mem::take(item_text)));
        }
    };

    while pos + 1 < data.len() {
        let code_unit_pos = (pos / 2) as u32; // UTF-16 코드 유닛 인덱스
        let ch = u16::from_le_bytes([data[pos], data[pos + 1]]);

        if ch == 0 {
            pos += 2;
        } else if ch == 0x0009 {
            // 탭: inline 컨트롤 (8 code unit = 16바이트)
            char_offsets.push(code_unit_pos);
            text.push('\t');
            item_text.push('\t');
            char_count += 1;
            // TAB 확장 데이터 보존 (code unit 1~7: 탭 너비, 종류 등)
            let mut ext = [0u16; 7];
            for k in 0..7 {
                let bp = pos + 2 + k * 2;
                if bp + 1 < data.len() {
                    ext[k] = u16::from_le_bytes([data[bp], data[bp + 1]]);
                }
            }
            tab_extended.push(ext);
            pos += 16;
        } else if ch == 0x000A {
            // 줄 끝: char 컨트롤 (1 code unit = 2바이트)
            char_offsets.push(code_unit_pos);
            text.push('\n');
            item_text.push('\n');
            char_count += 1;
            pos += 2;
        } else if ch == 0x000D {
            // 문단 끝
            break;
        } else if is_extended_ctrl_char(ch) {
            // 확장/인라인 컨트롤 문자: 8 code unit = 16바이트
            if ch == 0x0003 {
                // FIELD_BEGIN: 확장 컨트롤 → controls[]에 대응
                flush_item_text(&mut items, &mut item_text);
                items.push(ParagraphItem::Control(ctrl_idx));
                field_stack.push((char_count, ctrl_idx));
                ctrl_idx += 1;
            } else if ch == 0x0004 {
                // FIELD_END: 인라인 컨트롤 → controls[]에 대응하지 않음
                if let Some((start_idx, field_ctrl_idx)) = field_stack.pop() {
                    field_ranges.push(FieldRange {
                        start_char_idx: start_idx,
                        end_char_idx: char_count,
                        control_idx: field_ctrl_idx,
                    });
                }
            } else if is_extended_only_ctrl_char(ch) {
                // extended 컨트롤 (CTRL_HEADER 있음) → ctrl_idx 증가
                flush_item_text(&mut items, &mut item_text);
                items.push(ParagraphItem::Control(ctrl_idx));
                ctrl_idx += 1;
            }
            // inline 컨트롤 (4-9, 19-20 중 0x04 제외): ctrl_idx 증가 없음
            // 자동번호(0x12) / 새번호(0x12): 텍스트에 공백 placeholder 추가
            // → apply_auto_numbers_to_composed에서 "  " (연속 2공백)으로 번호 삽입
            if ch == 0x0012 {
                char_offsets.push(code_unit_pos);
                text.push(' ');
                item_text.push(' ');
                char_count += 1;
            }
            pos += 16;
        } else if ch < 0x0020 {
            // 문자 컨트롤 (1 code unit = 2바이트)
            match ch {
                0x0018 => {
                    char_offsets.push(code_unit_pos);
                    text.push('\u{00A0}'); // 묶음 빈칸
                    item_text.push('\u{00A0}');
                    char_count += 1;
                }
                0x0019 => {
                    char_offsets.push(code_unit_pos);
                    text.push(' '); // 고정폭 빈칸
                    item_text.push(' ');
                    char_count += 1;
                }
                0x001E => {
                    char_offsets.push(code_unit_pos);
                    text.push('\u{00A0}'); // 묶음 빈칸
                    item_text.push('\u{00A0}');
                    char_count += 1;
                }
                0x001F => {
                    char_offsets.push(code_unit_pos);
                    text.push('\u{2007}'); // 고정폭 빈칸 (FIGURE SPACE)
                    item_text.push('\u{2007}');
                    char_count += 1;
                }
                _ => {}
            }
            pos += 2;
        } else {
            // 일반 문자 (서로게이트 페어 처리)
            if (0xD800..=0xDBFF).contains(&ch) && pos + 3 < data.len() {
                let low = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
                if (0xDC00..=0xDFFF).contains(&low) {
                    let code_point = 0x10000 + ((ch as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                    if let Some(c) = char::from_u32(code_point) {
                        char_offsets.push(code_unit_pos);
                        text.push(c);
                        item_text.push(c);
                        char_count += 1;
                    }
                    pos += 4;
                    continue;
                }
            }
            if let Some(c) = char::from_u32(ch as u32) {
                char_offsets.push(code_unit_pos);
                text.push(c);
                item_text.push(c);
                char_count += 1;
            }
            pos += 2;
        }
    }

    flush_item_text(&mut items, &mut item_text);
    (text, char_offsets, field_ranges, tab_extended, items)
}

/// extended 컨트롤 문자 여부 (CTRL_HEADER 레코드가 있는 컨트롤)
///
/// HWP 5.0 제어 문자 분류 (표 6):
///   extended: 1-3, 11-12, 14-18, 21-23
///   inline: 4-9, 19-20
fn is_extended_only_ctrl_char(ch: u16) -> bool {
    matches!(ch, 1..=3 | 11..=12 | 14..=18 | 21..=23)
}

/// 16바이트 컨트롤 문자 여부 (8 code unit 차지)
///
/// HWP 5.0 제어 문자 분류 (표 6):
///   char (1 code unit = 2바이트): 0, 10, 13, 24-31
///   inline (8 code unit = 16바이트): 4-9, 19-20
///   extended (8 code unit = 16바이트): 1-3, 11-12, 14-18, 21-23
///
/// 탭(9), 줄 끝(10), 문단 끝(13)은 호출 전에 별도 처리된다.
fn is_extended_ctrl_char(ch: u16) -> bool {
    matches!(ch, 1..=8 | 11..=12 | 14..=23)
}

/// PARA_CHAR_SHAPE 바이너리 데이터 파싱
///
/// 각 항목: [u32 start_pos] + [u32 char_shape_id] (8바이트)
fn parse_para_char_shape(data: &[u8]) -> Vec<CharShapeRef> {
    let mut refs = Vec::new();
    let mut r = ByteReader::new(data);

    while r.remaining() >= 8 {
        let start_pos = r.read_u32().unwrap_or(0);
        let char_shape_id = r.read_u32().unwrap_or(0);
        refs.push(CharShapeRef {
            start_pos,
            char_shape_id,
        });
    }

    refs
}

/// PARA_LINE_SEG 바이너리 데이터 파싱
///
/// 각 항목: 36바이트 (u32 + i32×7 + u32)
fn parse_para_line_seg(data: &[u8]) -> Vec<LineSeg> {
    let mut segs = Vec::new();
    let mut r = ByteReader::new(data);

    while r.remaining() >= 36 {
        segs.push(LineSeg {
            text_start: r.read_u32().unwrap_or(0),
            vertical_pos: r.read_i32().unwrap_or(0),
            line_height: r.read_i32().unwrap_or(0),
            text_height: r.read_i32().unwrap_or(0),
            baseline_distance: r.read_i32().unwrap_or(0),
            line_spacing: r.read_i32().unwrap_or(0),
            column_start: r.read_i32().unwrap_or(0),
            segment_width: r.read_i32().unwrap_or(0),
            tag: r.read_u32().unwrap_or(0),
        });
    }

    segs
}

/// PARA_RANGE_TAG 바이너리 데이터 파싱
///
/// 각 항목: 12바이트 (u32 × 3)
fn parse_para_range_tag(data: &[u8]) -> Vec<RangeTag> {
    let mut result = Vec::new();
    let mut r = ByteReader::new(data);

    while r.remaining() >= 12 {
        result.push(RangeTag {
            start: r.read_u32().unwrap_or(0),
            end: r.read_u32().unwrap_or(0),
            tag: r.read_u32().unwrap_or(0),
        });
    }

    result
}

/// 레코드 목록에서 문단 리스트 추출 (재귀 파싱용)
///
/// TABLE 셀, 머리말/꼬리말, 각주/미주 등에서 문단 목록을 파싱할 때 사용.
pub fn parse_paragraph_list(records: &[Record]) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();
    let mut idx = 0;

    while idx < records.len() {
        if records[idx].tag_id == tags::HWPTAG_PARA_HEADER {
            let base_level = records[idx].level;
            let start = idx;
            idx += 1;
            while idx < records.len() && records[idx].level > base_level {
                idx += 1;
            }
            if let Ok(para) = parse_paragraph(&records[start..idx]) {
                paragraphs.push(para);
            }
        } else {
            idx += 1;
        }
    }

    paragraphs
}

fn parse_memo_lists_from_records(records: &[Record]) -> Vec<(u32, Vec<Paragraph>)> {
    let mut memo_lists = Vec::new();
    let mut idx = 0usize;
    while idx < records.len() {
        let record = &records[idx];
        if record.tag_id != tags::HWPTAG_LIST_HEADER
            || record.level != 1
            || !is_zero_area_list_header(&record.data)
        {
            idx += 1;
            continue;
        }

        let leading_number = if idx > 0
            && records[idx - 1].tag_id == 93
            && records[idx - 1].level == record.level
            && records[idx - 1].data.len() >= 4
        {
            Some(u32::from_le_bytes([
                records[idx - 1].data[0],
                records[idx - 1].data[1],
                records[idx - 1].data[2],
                records[idx - 1].data[3],
            ]))
        } else {
            None
        };

        let start = idx + 1;
        let mut end = start;
        let mut trailing_number = None;
        while end < records.len() {
            let r = &records[end];
            if r.level <= record.level {
                if r.tag_id == 93 && r.data.len() >= 4 {
                    trailing_number = Some(u32::from_le_bytes([
                        r.data[0], r.data[1], r.data[2], r.data[3],
                    ]));
                    break;
                }
                if r.tag_id == tags::HWPTAG_LIST_HEADER {
                    break;
                }
            }
            end += 1;
        }

        let memo_number = leading_number.or(trailing_number);
        if let Some(number) = memo_number {
            let paragraphs = parse_paragraph_list(&records[start..end]);
            memo_lists.push((number, paragraphs));
            idx = end + 1;
        } else {
            idx += 1;
        }
    }
    memo_lists
}

fn is_zero_area_list_header(data: &[u8]) -> bool {
    if data.len() < 16 {
        return false;
    }
    let text_width = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let text_height = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    text_width == 0 && text_height == 0
}

fn attach_memo_lists_to_fields(section: &mut Section, mut memo_lists: Vec<(u32, Vec<Paragraph>)>) {
    for para in &mut section.paragraphs {
        attach_memo_lists_to_paragraph(para, &mut memo_lists);
    }
}

fn attach_memo_lists_to_paragraph(
    para: &mut Paragraph,
    memo_lists: &mut Vec<(u32, Vec<Paragraph>)>,
) {
    for control in &mut para.controls {
        attach_memo_lists_to_control(control, memo_lists);
    }
}

fn attach_memo_lists_to_paragraphs(
    paragraphs: &mut [Paragraph],
    memo_lists: &mut Vec<(u32, Vec<Paragraph>)>,
) {
    for para in paragraphs {
        attach_memo_lists_to_paragraph(para, memo_lists);
    }
}

fn attach_memo_lists_to_control(
    control: &mut Control,
    memo_lists: &mut Vec<(u32, Vec<Paragraph>)>,
) {
    match control {
        Control::Field(field) if field.field_type == FieldType::Memo => {
            let number = memo_number_from_command(&field.command).unwrap_or(field.memo_index);
            if let Some(pos) = memo_lists.iter().position(|(n, _)| *n == number) {
                field.memo_paragraphs = memo_lists.remove(pos).1;
            }
        }
        Control::Table(table) => {
            for cell in &mut table.cells {
                attach_memo_lists_to_paragraphs(&mut cell.paragraphs, memo_lists);
            }
            if let Some(caption) = &mut table.caption {
                attach_memo_lists_to_paragraphs(&mut caption.paragraphs, memo_lists);
            }
        }
        Control::Shape(shape) => {
            if let Some(drawing) = shape.drawing_mut() {
                if let Some(text_box) = &mut drawing.text_box {
                    attach_memo_lists_to_paragraphs(&mut text_box.paragraphs, memo_lists);
                }
                if let Some(caption) = &mut drawing.caption {
                    attach_memo_lists_to_paragraphs(&mut caption.paragraphs, memo_lists);
                }
            }
            if let crate::model::shape::ShapeObject::Group(group) = shape.as_mut() {
                for child in &mut group.children {
                    let mut child_control = Control::Shape(Box::new(child.clone()));
                    attach_memo_lists_to_control(&mut child_control, memo_lists);
                    if let Control::Shape(updated_child) = child_control {
                        *child = *updated_child;
                    }
                }
                if let Some(caption) = &mut group.caption {
                    attach_memo_lists_to_paragraphs(&mut caption.paragraphs, memo_lists);
                }
            }
        }
        Control::Picture(pic) => {
            if let Some(caption) = &mut pic.caption {
                attach_memo_lists_to_paragraphs(&mut caption.paragraphs, memo_lists);
            }
        }
        _ => {}
    }
}

fn memo_number_from_command(command: &str) -> Option<u32> {
    command.split('/').nth(2)?.parse().ok()
}

/// CTRL_HEADER 레코드 그룹 파싱
///
/// records[0] = CTRL_HEADER, records[1..] = 자식 레코드
/// ctrl_id(처음 4바이트)로 컨트롤 종류를 식별한다.
fn parse_ctrl_header(records: &[Record]) -> Control {
    if records.is_empty() || records[0].tag_id != tags::HWPTAG_CTRL_HEADER {
        return Control::Unknown(UnknownControl { ctrl_id: 0 });
    }

    let data = &records[0].data;
    if data.len() < 4 {
        return Control::Unknown(UnknownControl { ctrl_id: 0 });
    }

    let ctrl_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let ctrl_data = &data[4..];
    let child_records = &records[1..];

    match ctrl_id {
        tags::CTRL_SECTION_DEF => {
            let section_def = parse_section_def(ctrl_data, child_records);
            Control::SectionDef(Box::new(section_def))
        }
        tags::CTRL_COLUMN_DEF => {
            let column_def = parse_column_def_ctrl(ctrl_data);
            Control::ColumnDef(column_def)
        }
        _ => {
            // 표, 도형, 그림, 머리말/꼬리말 등은 control.rs에서 처리
            super::control::parse_control(ctrl_id, ctrl_data, child_records)
        }
    }
}

/// 구역 정의 파싱 ('secd' 컨트롤)
///
/// ctrl_data: CTRL_HEADER의 ctrl_id 이후 데이터
/// child_records: 자식 레코드 (PAGE_DEF, FOOTNOTE_SHAPE, PAGE_BORDER_FILL)
fn parse_section_def(ctrl_data: &[u8], child_records: &[Record]) -> SectionDef {
    let mut sd = SectionDef::default();
    let mut r = ByteReader::new(ctrl_data);

    sd.flags = r.read_u32().unwrap_or(0);
    sd.column_spacing = r.read_i16().unwrap_or(0);
    let _vertical_align = r.read_u16().unwrap_or(0);
    let _horizontal_align = r.read_u16().unwrap_or(0);
    sd.default_tab_spacing = r.read_u32().unwrap_or(0);
    sd.outline_numbering_id = r.read_u16().unwrap_or(0);
    sd.page_num = r.read_u16().unwrap_or(0);
    sd.picture_num = r.read_u16().unwrap_or(0);
    sd.table_num = r.read_u16().unwrap_or(0);
    sd.equation_num = r.read_u16().unwrap_or(0);

    // 파싱된 필드 이후 추가 바이트 보존 (라운드트립용)
    let consumed = 4 + 2 + 2 + 2 + 4 + 2 + 2 + 2 + 2 + 2; // = 24 bytes
    if ctrl_data.len() > consumed {
        sd.raw_ctrl_extra = ctrl_data[consumed..].to_vec();
    }

    // 숨기기 플래그 (flags에서 추출)
    sd.hide_header = sd.flags & 0x0100 != 0;
    sd.hide_footer = sd.flags & 0x0200 != 0;
    sd.hide_master_page = sd.flags & 0x0004 != 0; // bit 2 (HWP5 스펙, 첫쪽 바탕쪽 감춤)
    sd.hide_border = sd.flags & 0x0800 != 0;
    sd.hide_fill = sd.flags & 0x1000 != 0;
    sd.hide_empty_line = sd.flags & 0x00080000 != 0; // bit 19: 빈 줄 감추기
    sd.page_num_type = ((sd.flags >> 20) & 0x03) as u8; // bit 20-21: 쪽 번호 종류 (0=이어서, 1=홀수, 2=짝수)

    // 자식 레코드에서 PAGE_DEF, FOOTNOTE_SHAPE, PAGE_BORDER_FILL 파싱
    let mut footnote_count = 0u32;
    let mut border_fill_count = 0u32;
    for record in child_records {
        match record.tag_id {
            tags::HWPTAG_PAGE_DEF => {
                sd.page_def = parse_page_def(&record.data);
            }
            tags::HWPTAG_FOOTNOTE_SHAPE => {
                let fs = parse_footnote_shape_record(&record.data);
                if footnote_count == 0 {
                    sd.footnote_shape = fs;
                } else {
                    sd.endnote_shape = fs;
                }
                footnote_count += 1;
            }
            tags::HWPTAG_PAGE_BORDER_FILL => {
                let pbf = parse_page_border_fill(&record.data);
                if border_fill_count == 0 {
                    sd.page_border_fill = pbf;
                } else {
                    sd.extra_page_border_fills.push(pbf);
                }
                border_fill_count += 1;
            }
            _ => {
                // 인식하지 못한 자식 레코드 보존 (바탕쪽 LIST_HEADER, 문단 등)
                sd.extra_child_records
                    .push(crate::model::document::RawRecord {
                        tag_id: record.tag_id,
                        level: record.level,
                        data: record.data.clone(),
                    });
            }
        }
    }

    // extra_child_records에서 바탕쪽 (LIST_HEADER) 파싱
    sd.master_pages = parse_master_pages_from_raw(&sd.extra_child_records);

    sd
}

/// extra_child_records에서 바탕쪽 LIST_HEADER를 파싱한다.
///
/// LIST_HEADER(tag 66)가 나타나면 바탕쪽으로 파싱.
/// 순서: 1번째=양쪽(Both), 2번째=홀수(Odd), 3번째=짝수(Even)
fn parse_master_pages_from_raw(raw_records: &[RawRecord]) -> Vec<MasterPage> {
    let mut master_pages = Vec::new();

    // RawRecord를 Record로 변환
    let records: Vec<Record> = raw_records
        .iter()
        .map(|r| Record {
            tag_id: r.tag_id,
            level: r.level,
            size: r.data.len() as u32,
            data: r.data.clone(),
        })
        .collect();

    // 바탕쪽 LIST_HEADER 위치 수집 (level 2만 — 하위 레벨은 도형 내부 텍스트박스)
    let top_level = records
        .iter()
        .filter(|r| r.tag_id == tags::HWPTAG_LIST_HEADER)
        .map(|r| r.level)
        .min()
        .unwrap_or(0);
    let list_header_positions: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.tag_id == tags::HWPTAG_LIST_HEADER && r.level == top_level)
        .map(|(i, _)| i)
        .collect();

    if list_header_positions.is_empty() {
        return master_pages;
    }

    let apply_order = [
        HeaderFooterApply::Even,
        HeaderFooterApply::Odd,
        HeaderFooterApply::Both,
    ];

    for (mp_idx, &start) in list_header_positions.iter().enumerate() {
        let apply_to = apply_order
            .get(mp_idx)
            .copied()
            .unwrap_or(HeaderFooterApply::Both);

        // LIST_HEADER 데이터 파싱
        let list_data = &records[start].data;
        let raw_list_header = list_data.to_vec();
        let mut r = ByteReader::new(list_data);

        // 표준 LIST_HEADER 프리픽스: para_count(2) + attr(4) + width_ref(2) = 8바이트
        let _para_count = r.read_u16().unwrap_or(0);
        let _list_attr = r.read_u32().unwrap_or(0);
        let _width_ref = r.read_u16().unwrap_or(0);

        // 바탕쪽 정보 (표 139, 10바이트)
        let text_width = r.read_u32().unwrap_or(0);
        let text_height = r.read_u32().unwrap_or(0);
        let text_ref = r.read_u8().unwrap_or(0);
        let num_ref = r.read_u8().unwrap_or(0);

        // 영역 0×0 LIST_HEADER는 MEMO/주석 컨트롤의 텍스트 박스가 오분류된 것.
        // 실제 바탕쪽은 반드시 text_width > 0 || text_height > 0.
        if text_width == 0 && text_height == 0 {
            continue;
        }

        // 확장 플래그 (byte 18-19, 표 139 이후)
        let ext_flags = r.read_u16().unwrap_or(0);

        let overlap = false;
        let is_extension = ext_flags >= 3
            || master_pages
                .iter()
                .any(|m: &MasterPage| m.apply_to == apply_to);

        // 이 LIST_HEADER에 속하는 문단 레코드 범위 결정
        let end = if mp_idx + 1 < list_header_positions.len() {
            list_header_positions[mp_idx + 1]
        } else {
            records.len()
        };

        // LIST_HEADER 다음 레코드부터 문단 파싱
        let para_records = &records[start + 1..end];
        let paragraphs = parse_paragraph_list(para_records);

        master_pages.push(MasterPage {
            apply_to,
            is_extension,
            overlap,
            ext_flags,
            paragraphs,
            text_width,
            text_height,
            text_ref,
            num_ref,
            raw_list_header,
        });
    }

    master_pages
}

/// 단 정의 파싱 ('cold' 컨트롤)
///
/// ctrl_data: CTRL_HEADER의 ctrl_id 이후 데이터
fn parse_column_def_ctrl(ctrl_data: &[u8]) -> ColumnDef {
    let mut cd = ColumnDef::default();
    let mut r = ByteReader::new(ctrl_data);

    // 표 140: UINT16 속성 (표 141 참조)
    let attr = r.read_u16().unwrap_or(0);
    cd.raw_attr = attr;
    // bit 0-1: 단 종류
    cd.column_type = match attr & 0x03 {
        1 => ColumnType::Distribute,
        2 => ColumnType::Parallel,
        _ => ColumnType::Normal,
    };
    // bit 2-9: 단 개수 (1-255)
    cd.column_count = ((attr >> 2) & 0xFF) as u16;
    // bit 10-11: 단 방향
    cd.direction = match (attr >> 10) & 0x03 {
        1 => ColumnDirection::RightToLeft,
        _ => ColumnDirection::LeftToRight,
    };
    // bit 12: 단 너비 동일 여부
    cd.same_width = attr & (1 << 12) != 0;

    // hwplib 기준: same_width 여부에 따라 바이트 순서가 다름
    if !cd.same_width && cd.column_count > 1 {
        // same_width=false: [attr2(2)] [col0_width(2) col0_gap(2)] [col1_width(2) col1_gap(2)] ...
        // 너비/간격 값은 비례값 (합계=32768), 절대 HWPUNIT이 아님
        let _attr2 = r.read_u16().unwrap_or(0);
        for _ in 0..cd.column_count {
            let w = r.read_i16().unwrap_or(0);
            let g = r.read_i16().unwrap_or(0);
            cd.widths.push(w);
            cd.gaps.push(g);
        }
        cd.proportional_widths = true;
    } else {
        // same_width=true: [gap(2)] [attr2(2)]
        cd.spacing = r.read_i16().unwrap_or(0);
        let _attr2 = r.read_u16().unwrap_or(0);
    }

    // 표 140: 단 구분선
    cd.separator_type = r.read_u8().unwrap_or(0);
    cd.separator_width = r.read_u8().unwrap_or(0);
    cd.separator_color = r.read_color_ref().unwrap_or(0);

    cd
}

/// 용지 설정 파싱 (HWPTAG_PAGE_DEF)
///
/// 레이아웃: u32 × 9 (크기+여백) + u32 attr
fn parse_page_def(data: &[u8]) -> PageDef {
    let mut pd = PageDef::default();
    let mut r = ByteReader::new(data);

    pd.width = r.read_u32().unwrap_or(59528);
    pd.height = r.read_u32().unwrap_or(84188);
    pd.margin_left = r.read_u32().unwrap_or(8504);
    pd.margin_right = r.read_u32().unwrap_or(8504);
    pd.margin_top = r.read_u32().unwrap_or(5669);
    pd.margin_bottom = r.read_u32().unwrap_or(4252);
    pd.margin_header = r.read_u32().unwrap_or(4252);
    pd.margin_footer = r.read_u32().unwrap_or(4252);
    pd.margin_gutter = r.read_u32().unwrap_or(0);
    pd.attr = r.read_u32().unwrap_or(0);

    // HWP binary page-def bit 0 is inverted from the intuitive name used in
    // older comments here. Hancom's HWP->HWPX export writes attr=0 as
    // landscape="WIDELY" and attr=1 as landscape="NARROWLY".
    pd.landscape = pd.attr & 0x01 == 0;
    pd.binding = match (pd.attr >> 1) & 0x03 {
        1 => BindingMethod::DuplexSided,
        2 => BindingMethod::TopFlip,
        _ => BindingMethod::SingleSided,
    };

    pd
}

/// 각주/미주 모양 파싱 (HWPTAG_FOOTNOTE_SHAPE)
///
/// 스펙 문서는 26바이트로 기술하지만, 실제 레코드는 28바이트.
/// note_spacing과 separator_line_type 사이에 미문서화된 2바이트 필드가 있음.
fn parse_footnote_shape_record(data: &[u8]) -> FootnoteShape {
    let mut fs = FootnoteShape::default();
    let mut r = ByteReader::new(data);

    fs.attr = r.read_u32().unwrap_or(0);

    // attr에서 number_format, numbering, placement 추출
    let num_fmt = fs.attr & 0xFF;
    fs.number_format = match num_fmt {
        0 => crate::model::footnote::NumberFormat::Digit,
        1 => crate::model::footnote::NumberFormat::CircledDigit,
        2 => crate::model::footnote::NumberFormat::UpperRoman,
        3 => crate::model::footnote::NumberFormat::LowerRoman,
        4 => crate::model::footnote::NumberFormat::UpperAlpha,
        5 => crate::model::footnote::NumberFormat::LowerAlpha,
        6 => crate::model::footnote::NumberFormat::CircledUpperAlpha,
        7 => crate::model::footnote::NumberFormat::CircledLowerAlpha,
        8 => crate::model::footnote::NumberFormat::HangulSyllable,
        9 => crate::model::footnote::NumberFormat::CircledHangulSyllable,
        10 => crate::model::footnote::NumberFormat::HangulJamo,
        11 => crate::model::footnote::NumberFormat::CircledHangulJamo,
        12 => crate::model::footnote::NumberFormat::HangulDigit,
        13 => crate::model::footnote::NumberFormat::HanjaDigit,
        14 => crate::model::footnote::NumberFormat::CircledHanjaDigit,
        15 => crate::model::footnote::NumberFormat::HanjaGapEul,
        16 => crate::model::footnote::NumberFormat::HanjaGapEulHanja,
        _ => crate::model::footnote::NumberFormat::Digit,
    };
    fs.numbering = match (fs.attr >> 8) & 0x03 {
        1 => crate::model::footnote::FootnoteNumbering::RestartSection,
        2 => crate::model::footnote::FootnoteNumbering::RestartPage,
        _ => crate::model::footnote::FootnoteNumbering::Continue,
    };
    fs.placement = match (fs.attr >> 8) & 0x03 {
        1 => crate::model::footnote::FootnotePlacement::BelowText,
        2 => crate::model::footnote::FootnotePlacement::RightColumn,
        _ => crate::model::footnote::FootnotePlacement::EachColumn,
    };

    fs.user_char = char::from_u32(r.read_u16().unwrap_or(0) as u32).unwrap_or('\0');
    fs.prefix_char = char::from_u32(r.read_u16().unwrap_or(0) as u32).unwrap_or('\0');
    fs.suffix_char = char::from_u32(r.read_u16().unwrap_or(0) as u32).unwrap_or('\0');
    fs.start_number = r.read_u16().unwrap_or(1);
    fs.separator_length = r.read_i16().unwrap_or(0);
    fs.separator_margin_top = r.read_i16().unwrap_or(0);
    fs.separator_margin_bottom = r.read_i16().unwrap_or(0);
    fs.note_spacing = r.read_i16().unwrap_or(0);

    // 미문서화 2바이트 (스펙에는 없지만 실제 데이터에 존재)
    fs.raw_unknown = r.read_u16().unwrap_or(0);

    fs.separator_line_type = r.read_u8().unwrap_or(0);
    fs.separator_line_width = r.read_u8().unwrap_or(0);
    fs.separator_color = r.read_color_ref().unwrap_or(0);

    fs
}

/// 쪽 테두리/배경 파싱 (HWPTAG_PAGE_BORDER_FILL)
fn parse_page_border_fill(data: &[u8]) -> PageBorderFill {
    let mut pbf = PageBorderFill::default();
    let mut r = ByteReader::new(data);

    pbf.attr = r.read_u32().unwrap_or(0);
    pbf.spacing_left = r.read_i16().unwrap_or(0);
    pbf.spacing_right = r.read_i16().unwrap_or(0);
    pbf.spacing_top = r.read_i16().unwrap_or(0);
    pbf.spacing_bottom = r.read_i16().unwrap_or(0);
    pbf.border_fill_id = r.read_u16().unwrap_or(0);

    pbf
}

/// CTRL_DATA에서 필드 이름을 추출한다.
///
/// CTRL_DATA 레이아웃 (누름틀 필드):
///   바이트 0~9: 헤더 (paramset 등)
///   바이트 10~11: WORD - 필드 이름 길이 (글자 수)
///   바이트 12~: WCHAR[len] - 필드 이름 (UTF-16LE)
fn parse_ctrl_data_field_name(data: &[u8]) -> Option<String> {
    if data.len() < 12 {
        return None;
    }
    let name_len = u16::from_le_bytes([data[10], data[11]]) as usize;
    if name_len == 0 {
        return None;
    }
    let name_bytes = &data[12..];
    if name_bytes.len() < name_len * 2 {
        return None;
    }
    let wchars: Vec<u16> = name_bytes[..name_len * 2]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let name = String::from_utf16_lossy(&wchars);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests;
