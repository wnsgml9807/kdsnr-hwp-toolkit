//! HWPX 파일 파서 모듈
//!
//! HWPX(XML 기반 HWP) 파일을 파싱하여 Document 모델로 변환한다.
//! HWPX는 ZIP 패키지 내 XML 파일로 구성된 KS X 6101:2024 표준 포맷이다.
//!
//! ## 파싱 순서
//! 1. ZIP 컨테이너 열기 (reader)
//! 2. content.hpf → 섹션 파일 목록 추출 (content)
//! 3. header.xml → DocInfo 변환 (header)
//! 4. section*.xml → Section 변환 (section)
//! 5. BinData → 이미지 로딩

pub mod content;
pub mod header;
pub mod reader;
pub mod section;
pub mod utils;

use crate::model::bin_data::{BinData, BinDataContent, BinDataType};
use crate::model::document::{Document, FileHeader, HwpVersion, Section};

/// HWPX 파싱 에러
#[derive(Debug)]
pub enum HwpxError {
    /// ZIP 컨테이너 오류
    ZipError(String),
    /// XML 파싱 오류
    XmlError(String),
    /// 필수 파일 누락
    MissingFile(String),
    /// 데이터 변환 오류
    ConversionError(String),
}

impl std::fmt::Display for HwpxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HwpxError::ZipError(e) => write!(f, "ZIP 오류: {}", e),
            HwpxError::XmlError(e) => write!(f, "XML 파싱 오류: {}", e),
            HwpxError::MissingFile(e) => write!(f, "필수 파일 누락: {}", e),
            HwpxError::ConversionError(e) => write!(f, "변환 오류: {}", e),
        }
    }
}

impl std::error::Error for HwpxError {}

impl From<zip::result::ZipError> for HwpxError {
    fn from(e: zip::result::ZipError) -> Self {
        HwpxError::ZipError(e.to_string())
    }
}

impl From<quick_xml::Error> for HwpxError {
    fn from(e: quick_xml::Error) -> Self {
        HwpxError::XmlError(e.to_string())
    }
}

/// HWPX 파일 바이트 데이터를 파싱하여 Document IR로 변환
pub fn parse_hwpx(data: &[u8]) -> Result<Document, HwpxError> {
    // 1. ZIP 컨테이너 열기
    let mut reader = reader::HwpxReader::open(data)?;

    // 2. content.hpf → 섹션 파일 목록 + BinData 목록
    let content_xml = reader.read_file("Contents/content.hpf")?;
    let package_info = content::parse_content_hpf(&content_xml)?;

    // 3. header.xml → DocInfo, DocProperties
    let header_xml = reader.read_file("Contents/header.xml")?;
    let (mut doc_info, doc_properties) = header::parse_hwpx_header(&header_xml)?;

    // BinData 목록을 DocInfo에 등록
    for (i, item) in package_info.bin_data_items.iter().enumerate() {
        let ext = item.href.rsplit('.').next().unwrap_or("dat").to_string();
        doc_info.bin_data_list.push(BinData {
            data_type: BinDataType::Embedding,
            storage_id: (i + 1) as u16,
            extension: Some(ext),
            ..Default::default()
        });
    }

    // 4. section*.xml → Section 변환
    let mut sections = Vec::new();
    for section_href in &package_info.section_files {
        let section_xml = reader.read_file(section_href)?;
        match section::parse_hwpx_section(&section_xml) {
            Ok(section) => sections.push(section),
            Err(e) => {
                eprintln!("경고: {} 파싱 실패: {}", section_href, e);
                sections.push(Section::default());
            }
        }
    }

    // 5. BinData 이미지 로딩
    let mut bin_data_content = Vec::new();
    for (i, item) in package_info.bin_data_items.iter().enumerate() {
        match reader.read_file_bytes(&item.href) {
            Ok(data) => {
                let ext = item.href.rsplit('.').next().unwrap_or("dat").to_string();
                bin_data_content.push(BinDataContent {
                    id: (i + 1) as u16,
                    data,
                    extension: ext,
                });
            }
            Err(e) => {
                eprintln!("경고: BinData '{}' 로드 실패: {}", item.href, e);
            }
        }
    }

    // 5-1. Chart/*.xml (OOXML 차트) 로딩 — bin_data_id = 60000+N, extension="ooxml_chart"
    // section 파서에서 <hp:chart chartIDRef="Chart/chartN.xml">를 만나면 동일 ID의 OleShape 생성
    for n in 1..=64u16 {
        let path = format!("Chart/chart{}.xml", n);
        match reader.read_file_bytes(&path) {
            Ok(data) => {
                bin_data_content.push(BinDataContent {
                    id: 60000 + n,
                    data,
                    extension: "ooxml_chart".to_string(),
                });
            }
            Err(_) => break,
        }
    }

    // 6. Contents/masterpageN.xml 파싱 — 한컴 시험지 헤더/divider/페이지번호의 source.
    //
    // rhwp 의 원래 hwpx loader 는 master page XML 자체를 읽지 않아 시험지 페이지
    // 의 헤더 영역 ("수학 영역" 등) + 우측 divider + 페이지번호 박스가 통째로
    // 누락됐다. 본 로딩이 그 결손 해결의 1차 진입점.
    //
    // 한컴 시험지는 N개 master page (페이지별, EVEN/ODD/BOTH) 를 가진다. 가장
    // 단순한 attach 정책: 모든 master page 를 모든 section 의 section_def 에
    // 푸시. 실제 active 선택은 pagination 단계가 type + page_number 매칭.
    let mut master_pages: Vec<crate::model::header_footer::MasterPage> = Vec::new();
    for n in 0..32u32 {
        let path = format!("Contents/masterpage{}.xml", n);
        match reader.read_file(&path) {
            Ok(xml) => match section::parse_hwpx_master_page(&xml) {
                Ok(mp) => master_pages.push(mp),
                Err(e) => eprintln!("masterpage{} 파싱 실패: {}", n, e),
            },
            Err(_) => break, // 더 이상 파일 없음
        }
    }
    if !master_pages.is_empty() {
        for section in &mut sections {
            section.section_def.master_pages = master_pages.clone();
            // 직렬화되는 secd 는 section_def 필드가 아니라 문단 내
            // Control::SectionDef 인스턴스다(<hp:secPr> 파싱 시점에 clone 되어
            // 바탕쪽을 아직 모름). hwp 직렬화에서 바탕쪽이 빠지지 않도록 그쪽에도 채운다.
            for para in &mut section.paragraphs {
                for ctrl in &mut para.controls {
                    if let crate::model::control::Control::SectionDef(sd) = ctrl {
                        sd.master_pages = master_pages.clone();
                    }
                }
            }
        }
    }

    // Document 조립
    let model_header = FileHeader {
        version: HwpVersion {
            major: 5,
            minor: 1,
            build: 0,
            revision: 0,
        },
        flags: 0,
        compressed: false,
        encrypted: false,
        distribution: false,
        raw_data: None,
    };

    let doc = Document {
        header: model_header,
        doc_properties,
        doc_info,
        sections,
        preview: None,
        bin_data_content,
        extra_streams: Vec::new(),
        legacy_equation_width_unit: false,
    };

    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hwpx_invalid_data() {
        let result = parse_hwpx(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hwpx_not_zip() {
        // CFB/HWP 데이터로 시도
        let result = parse_hwpx(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
        assert!(result.is_err());
    }
}
