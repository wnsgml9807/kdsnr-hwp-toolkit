//! 직렬화 컨텍스트 — 1-pass 스캔으로 ID 풀을 구성하고 2-pass 쓰기에서 참조 정합성을 단언.
//!
//! ## 배경
//!
//! HWPX 직렬화에서 가장 큰 함정은 **한 파일(section.xml)에서 쓴 ID가 다른 파일(header.xml)에
//! 등록되지 않은** 상태로 출력되는 경우다. 예: `<hp:run charPrIDRef="3">` 를 썼는데
//! header의 `<hh:charPr id="3">` 가 누락되면 한컴2020이 조용히 스타일을 엉키게 렌더링한다.
//!
//! `SerializeContext`는 이를 구조적으로 방지한다:
//! 1. **1-pass**: Document IR을 훑어 모든 ID를 `registered`에 등록
//! 2. **2-pass**: 각 writer가 ID를 사용할 때 `reference`에 기록
//! 3. **단언**: `assert_all_refs_resolved()` 가 `referenced - registered` 가 공집합임을 확인
//!
//! Stage 0 에서는 뼈대 구조만 둔다. 실제 스캔 로직은 Stage 1~4에서 writer가 추가될 때 함께 확장한다.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::model::control::Control;
use crate::model::document::Document;
use crate::model::paragraph::Paragraph;
use crate::model::shape::ShapeObject;
use crate::serializer::SerializeError;

/// 양방향 ID 풀 — 등록된 ID와 참조된 ID를 추적한다.
#[derive(Debug, Default)]
pub struct IdPool<T: Copy + Eq + std::hash::Hash> {
    registered: HashSet<T>,
    referenced: HashSet<T>,
}

impl<T: Copy + Eq + std::hash::Hash> IdPool<T> {
    pub fn new() -> Self {
        Self {
            registered: HashSet::new(),
            referenced: HashSet::new(),
        }
    }

    /// header/DocInfo에서 정의되는 ID를 등록.
    pub fn register(&mut self, id: T) {
        self.registered.insert(id);
    }

    /// section/기타 writer가 ID를 참조할 때 호출.
    pub fn reference(&mut self, id: T) {
        self.referenced.insert(id);
    }

    pub fn is_registered(&self, id: &T) -> bool {
        self.registered.contains(id)
    }

    /// `referenced - registered`: 참조됐으나 등록되지 않은 ID.
    pub fn unresolved(&self) -> Vec<T> {
        self.referenced
            .difference(&self.registered)
            .copied()
            .collect()
    }

    pub fn registered_count(&self) -> usize {
        self.registered.len()
    }
}

/// HWPX manifest + ZIP entry용 BinData 엔트리.
#[derive(Debug, Clone)]
pub struct BinDataEntry {
    /// content.hpf 의 `opf:item id` (예: "image1")
    pub manifest_id: String,
    /// ZIP 엔트리 경로 (예: "BinData/image1.png")
    pub href: String,
    /// MIME 타입 (예: "image/png")
    pub media_type: String,
    /// IR 상의 bin_data_id (storage_id) — 매핑 역추적용
    pub bin_data_id: u16,
}

/// 1-pass 스캔으로 구축되는 직렬화 컨텍스트.
#[derive(Debug, Default)]
pub struct SerializeContext {
    pub char_shape_ids: IdPool<u32>,
    pub para_shape_ids: IdPool<u16>,
    pub border_fill_ids: IdPool<u16>,
    pub tab_pr_ids: IdPool<u16>,
    pub numbering_ids: IdPool<u16>,
    pub style_ids: IdPool<u16>,
    /// charPrIDRef -> CHAR style id. Hancom emits charStyleIDRef on selected
    /// text fragments when the fragment's char shape is backed by a CHAR
    /// style; the binary paragraph stores the char-shape range, while the
    /// style table carries the stable HWPX style id.
    pub char_style_by_char_shape: HashMap<u32, u16>,
    /// Master-page text materializes only Hancom's named header/footer CHAR
    /// styles, not every CHAR style backed by the run's char shape.
    pub master_page_char_style_by_char_shape: HashMap<u32, u16>,
    /// charPrIDRef -> CHAR style id for question numbers.
    pub question_number_char_style_by_char_shape: HashMap<u32, u16>,
    /// Default CHAR style id for source markers such as `(가)`/`(나)`.
    pub source_marker_char_style_id: Option<u16>,
    /// Native Korean prompt range style for literal bracket characters.
    pub bracket_range_char_style_id: Option<u16>,
    /// Native Korean prompt range style for `~숫자` fragments.
    pub range_suffix_char_style_id: Option<u16>,
    /// Native master-page style used for the confirmation subject label.
    pub confirmation_subject_char_style_id: Option<u16>,
    /// Char shapes with an explicit shade fill. Hancom treats selected shaded
    /// equation lead-in runs specially in HWPX output.
    pub shaded_char_shape_ids: HashSet<u32>,
    /// `bin_data_id` (IR) → manifest 엔트리 매핑
    pub bin_data_map: HashMap<u16, BinDataEntry>,
    /// Master page XML uses Hancom's raw borderFillIDRef for table controls,
    /// while body section table controls are serialized against the shifted
    /// HWPX borderFill table emitted in header.xml.
    pub in_master_page: bool,
}

impl SerializeContext {
    /// Document IR 전체를 1-pass 스캔하여 ID 풀을 채운다.
    ///
    /// Stage 0에서는 최소 등록(header.xml 리소스만)만 수행한다. Stage 1~4에서
    /// 각 writer가 추가되면서 `reference()` 호출과 스캔 범위가 확장된다.
    pub fn collect_from_document(doc: &Document) -> Self {
        let mut ctx = Self::default();

        // CharShape, ParaShape, TabDef, Style, Font IDs are array-indexed.
        // BorderFill and Numbering are emitted as HWP/HWPX 1-based IDs.
        for (idx, char_shape) in doc.doc_info.char_shapes.iter().enumerate() {
            ctx.char_shape_ids.register(idx as u32);
            if char_shape.shade_color != 0xFFFF_FFFF {
                ctx.shaded_char_shape_ids.insert(idx as u32);
            }
        }
        for (idx, _) in doc.doc_info.para_shapes.iter().enumerate() {
            ctx.para_shape_ids.register(idx as u16);
        }
        for (idx, _) in doc.doc_info.border_fills.iter().enumerate() {
            ctx.border_fill_ids.register(idx as u16 + 1);
        }
        for (idx, _) in doc.doc_info.tab_defs.iter().enumerate() {
            ctx.tab_pr_ids.register(idx as u16);
        }
        for (idx, _) in doc.doc_info.numberings.iter().enumerate() {
            ctx.numbering_ids.register(idx as u16 + 1);
        }
        for (idx, _) in doc.doc_info.styles.iter().enumerate() {
            ctx.style_ids.register(idx as u16);
        }
        for (idx, style) in doc.doc_info.styles.iter().enumerate() {
            if style.style_type == 1 {
                ctx.char_style_by_char_shape
                    .entry(style.char_shape_id as u32)
                    .or_insert(idx as u16);
                if is_master_page_materialized_char_style(&style.local_name) {
                    ctx.master_page_char_style_by_char_shape
                        .entry(style.char_shape_id as u32)
                        .or_insert(idx as u16);
                }
                if style.local_name == "명조10" && ctx.source_marker_char_style_id.is_none() {
                    ctx.source_marker_char_style_id = Some(idx as u16);
                }
                if style.local_name == "[ ]" && ctx.bracket_range_char_style_id.is_none() {
                    ctx.bracket_range_char_style_id = Some(idx as u16);
                }
                if style.local_name == "~숫자" && ctx.range_suffix_char_style_id.is_none() {
                    ctx.range_suffix_char_style_id = Some(idx as u16);
                }
                if style.local_name == "확인사항 선택과목"
                    && ctx.confirmation_subject_char_style_id.is_none()
                {
                    ctx.confirmation_subject_char_style_id = Some(idx as u16);
                }
                let target = style.local_name.contains("문항번호")
                    || !ctx
                        .question_number_char_style_by_char_shape
                        .contains_key(&(style.char_shape_id as u32));
                if target {
                    ctx.question_number_char_style_by_char_shape
                        .insert(style.char_shape_id as u32, idx as u16);
                }
            }
        }

        // BinData: Hancom assigns imageN by first visual reference order, not
        // necessarily by storage id. Keep unreferenced binaries after that in
        // storage order so the manifest remains complete.
        let mut bin_order = referenced_bin_order(doc);
        for bd in &doc.bin_data_content {
            if !bin_order.contains(&bd.id) {
                bin_order.push(bd.id);
            }
        }
        for (i, bin_id) in bin_order.into_iter().enumerate() {
            let Some(bd) = doc.bin_data_content.iter().find(|b| b.id == bin_id) else {
                continue;
            };
            let ext = if bd.extension.is_empty() {
                "bin"
            } else {
                bd.extension.as_str()
            };
            let manifest_id = format!("image{}", i + 1);
            let href = format!("BinData/{}.{}", manifest_id, ext);
            let media_type = mime_from_ext(ext);
            ctx.bin_data_map.insert(
                bd.id,
                BinDataEntry {
                    manifest_id,
                    href,
                    media_type: media_type.to_string(),
                    bin_data_id: bin_id,
                },
            );
        }

        ctx
    }

    /// manifest·content.hpf 출력용 엔트리 목록 (삽입 순서 보존을 위해 `bin_data_id` 정렬).
    pub fn bin_data_entries(&self) -> Vec<BinDataEntry> {
        let mut v: Vec<_> = self.bin_data_map.values().cloned().collect();
        v.sort_by_key(|e| {
            e.manifest_id
                .strip_prefix("image")
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(u16::MAX)
        });
        v
    }

    pub fn master_page_bin_entry_count(&self, doc: &Document) -> usize {
        let mut ids = Vec::new();
        for section in &doc.sections {
            for master_page in &section.section_def.master_pages {
                for para in &master_page.paragraphs {
                    collect_para_bins(para, &mut ids);
                }
            }
        }
        ids.len()
    }

    /// `bin_data_id` → manifest id 조회 (Stage 4의 `<hc:img binaryItemIDRef="...">` 용).
    pub fn resolve_bin_id(&self, bin_data_id: u16) -> Option<&str> {
        self.bin_data_map
            .get(&bin_data_id)
            .map(|e| e.manifest_id.as_str())
    }

    /// 모든 참조가 해소되었는지 단언. 해소되지 않은 ID가 있으면 `SerializeError::XmlError` 반환.
    pub fn assert_all_refs_resolved(&self) -> Result<(), SerializeError> {
        let mut missing: Vec<String> = Vec::new();
        let cs = self.char_shape_ids.unresolved();
        if !cs.is_empty() {
            missing.push(format!("charPrIDRef: {:?}", cs));
        }
        let ps = self.para_shape_ids.unresolved();
        if !ps.is_empty() {
            missing.push(format!("paraPrIDRef: {:?}", ps));
        }
        let bf = self.border_fill_ids.unresolved();
        if !bf.is_empty() {
            missing.push(format!("borderFillIDRef: {:?}", bf));
        }
        let tp = self.tab_pr_ids.unresolved();
        if !tp.is_empty() {
            missing.push(format!("tabPrIDRef: {:?}", tp));
        }
        let nm = self.numbering_ids.unresolved();
        if !nm.is_empty() {
            missing.push(format!("numberingIDRef: {:?}", nm));
        }
        let st = self.style_ids.unresolved();
        if !st.is_empty() {
            missing.push(format!("styleIDRef: {:?}", st));
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(SerializeError::XmlError(format!(
                "미등록 ID 참조 발견: {}",
                missing.join("; ")
            )))
        }
    }
}

fn is_master_page_materialized_char_style(name: &str) -> bool {
    name == "시험회차년도"
        || name == "윗쪽번호"
        || name == "제2교시"
        || name.contains("홀짝")
        || name.contains("확인사항")
        || name.contains("(선택과목)")
}

fn referenced_bin_order(doc: &Document) -> Vec<u16> {
    let mut out = Vec::new();
    for section in &doc.sections {
        for master_page in &section.section_def.master_pages {
            for para in &master_page.paragraphs {
                collect_para_bins(para, &mut out);
            }
        }
        for para in &section.paragraphs {
            collect_para_bins(para, &mut out);
        }
    }
    out
}

fn push_unique(out: &mut Vec<u16>, id: u16) {
    if id != 0 && !out.contains(&id) {
        out.push(id);
    }
}

fn collect_para_bins(para: &Paragraph, out: &mut Vec<u16>) {
    for control in &para.controls {
        collect_control_bins(control, out);
    }
}

fn collect_control_bins(control: &Control, out: &mut Vec<u16>) {
    match control {
        Control::Picture(pic) => push_unique(out, pic.image_attr.bin_data_id),
        Control::Shape(shape) => collect_shape_bins(shape, out),
        Control::Table(table) => {
            for cell in &table.cells {
                for para in &cell.paragraphs {
                    collect_para_bins(para, out);
                }
            }
        }
        Control::Header(header) => {
            for para in &header.paragraphs {
                collect_para_bins(para, out);
            }
        }
        Control::Footer(footer) => {
            for para in &footer.paragraphs {
                collect_para_bins(para, out);
            }
        }
        Control::Footnote(note) => {
            for para in &note.paragraphs {
                collect_para_bins(para, out);
            }
        }
        Control::Endnote(note) => {
            for para in &note.paragraphs {
                collect_para_bins(para, out);
            }
        }
        _ => {}
    }
}

fn collect_shape_bins(shape: &ShapeObject, out: &mut Vec<u16>) {
    match shape {
        ShapeObject::Picture(pic) => push_unique(out, pic.image_attr.bin_data_id),
        ShapeObject::Group(group) => {
            for child in &group.children {
                collect_shape_bins(child, out);
            }
        }
        _ => {}
    }
}

fn mime_from_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "tmp" => "image/tmp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_has_no_registered_ids() {
        let doc = Document::default();
        let ctx = SerializeContext::collect_from_document(&doc);
        assert_eq!(ctx.char_shape_ids.registered_count(), 0);
        assert_eq!(ctx.para_shape_ids.registered_count(), 0);
        assert!(ctx.bin_data_map.is_empty());
    }

    #[test]
    fn empty_doc_passes_ref_resolution() {
        let doc = Document::default();
        let ctx = SerializeContext::collect_from_document(&doc);
        ctx.assert_all_refs_resolved().expect("empty doc must pass");
    }

    #[test]
    fn unresolved_char_pr_fails() {
        let doc = Document::default();
        let mut ctx = SerializeContext::collect_from_document(&doc);
        ctx.char_shape_ids.reference(42); // 등록되지 않은 ID 참조
        let err = ctx.assert_all_refs_resolved().unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("charPrIDRef"),
            "error message should name charPrIDRef: {}",
            msg
        );
        assert!(
            msg.contains("42"),
            "error message should include id 42: {}",
            msg
        );
    }

    #[test]
    fn id_pool_register_reference_roundtrip() {
        let mut pool: IdPool<u32> = IdPool::new();
        pool.register(1);
        pool.register(2);
        pool.reference(1);
        pool.reference(3); // 미등록
        assert!(pool.is_registered(&1));
        assert!(!pool.is_registered(&3));
        assert_eq!(pool.unresolved(), vec![3]);
    }

    #[test]
    fn mime_from_ext_covers_common_formats() {
        assert_eq!(mime_from_ext("png"), "image/png");
        assert_eq!(mime_from_ext("PNG"), "image/png");
        assert_eq!(mime_from_ext("jpg"), "image/jpeg");
        assert_eq!(mime_from_ext("unknown"), "application/octet-stream");
    }
}
