//! PDF 렌더러 (Task #21)
//!
//! SVG 렌더러의 출력을 svg2pdf + pdf-writer로 PDF를 생성한다.
//! 단일/다중 페이지 모두 지원. 네이티브 전용 (WASM 미지원).

/// 폰트 데이터베이스를 초기화 (시스템 폰트 + 프로젝트 폰트 로드).
///
/// 폰트 로드 우선순위:
/// 1. 시스템 폰트
/// 2. binary 위치 기준 `<crate_root>/ttfs/{hancom/flat,hwp,windows,}` — cwd 무관
/// 3. cwd 기준 동일 디렉토리 (개발 환경 fallback)
/// 4. macOS 한컴오피스 번들 (HyhwpEQ, Haan Symbol 등)
/// 5. WSL Windows 폰트
/// 6. `RHWP_FONT_DIR` 환경변수 (`:` 구분, 외부 호출자가 명시적으로 가리키는 경로)
#[cfg(not(target_arch = "wasm32"))]
fn create_fontdb() -> usvg::fontdb::Database {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();

    let project_dirs = ["ttfs/hancom/flat", "ttfs/hwp", "ttfs/windows", "ttfs"];

    // binary 위치 기준 (cwd 무관). exe = `<crate_root>/target/release/rhwp` 가정.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(crate_root) = exe.ancestors().nth(3) {
            for sub in &project_dirs {
                let p = crate_root.join(sub);
                if p.exists() {
                    fontdb.load_fonts_dir(&p);
                }
            }
        }
    }

    // cwd 기준 (legacy / dev 호환)
    for dir in &project_dirs {
        if std::path::Path::new(dir).exists() {
            fontdb.load_fonts_dir(dir);
        }
    }

    // macOS: 한컴오피스 번들 내 폰트
    // - Install: 함초롬바탕/돋움 (HCR Batang/Dotum), 한컴바탕/돋움 (Haansoft Batang)
    // - Hwp:     HY견명조/견고딕/엽서M, 휴먼명조/고딕, 양재 시리즈
    // - All:     HY헤드라인M (H2HDRM), HY나무/바다/동녘/강/산/수평선/태백/울릉도,
    //            한컴바탕확장 (FZSong_Super), 한컴 윤고딕/윤체/백제/소망/쿨재즈/바겐세일/솔잎,
    //            HBatang/HDotum (Install 중복본)
    //   → 셋 다 로드해야 HWPX 가 직접 지정한 face 가 fontdb 에서 해석된다.
    for dir in &[
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Hwp",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/All",
    ] {
        if std::path::Path::new(dir).exists() {
            fontdb.load_fonts_dir(dir);
        }
    }

    // WSL Windows 폰트
    if std::path::Path::new("/mnt/c/Windows/Fonts").exists() {
        fontdb.load_fonts_dir("/mnt/c/Windows/Fonts");
    }

    // 외부 명시 경로 (배포 환경에서 호출자가 위치 지정)
    if let Ok(extra) = std::env::var("RHWP_FONT_DIR") {
        for p in extra.split(':') {
            if !p.is_empty() && std::path::Path::new(p).exists() {
                fontdb.load_fonts_dir(p);
            }
        }
    }

    fontdb.set_serif_family("바탕");
    fontdb.set_sans_serif_family("맑은 고딕");
    fontdb.set_monospace_family("D2Coding");
    fontdb
}

/// SVG에서 없는 한글 폰트명에 fallback 추가
#[cfg(not(target_arch = "wasm32"))]
fn add_font_fallbacks(svg: &str) -> String {
    svg.replace(
        "font-family=\"휴먼명조\"",
        "font-family=\"휴먼명조, 바탕, serif\"",
    )
    .replace(
        "font-family=\"HCI Poppy\"",
        "font-family=\"HCI Poppy, 맑은 고딕, sans-serif\"",
    )
}

/// 단일 SVG를 PDF로 변환.
///
/// 우리 SVG 는 96 DPI 기준 좌표계 (rhwp 렌더러 기본). svg2pdf 의
/// `PageOptions::default()` 는 dpi=72 로 가정하여 1px = 1pt 변환 →
/// page size 가 96/72 = 4/3 배 부풀어 한 컴 PDF 와 mismatch. `dpi=96` 으로
/// 명시 지정해 1px = 0.75pt 정확히 환산. (예: 1028 px → 771 pt = A4 폭)
#[cfg(not(target_arch = "wasm32"))]
pub fn svg_to_pdf(svg_content: &str) -> Result<Vec<u8>, String> {
    let fontdb = create_fontdb();
    let mut options = usvg::Options::default();
    options.fontdb = std::sync::Arc::new(fontdb);
    let svg_with_fallback = add_font_fallbacks(svg_content);
    let tree = usvg::Tree::from_str(&svg_with_fallback, &options)
        .map_err(|e| format!("SVG 파싱 실패: {}", e))?;
    let page_opts = svg2pdf::PageOptions { dpi: 96.0 };
    let pdf = svg2pdf::to_pdf(&tree, svg2pdf::ConversionOptions::default(), page_opts)
        .map_err(|e| format!("PDF 변환 실패: {:?}", e))?;
    Ok(pdf)
}

/// 여러 SVG 페이지를 단일 다중 페이지 PDF로 생성
#[cfg(not(target_arch = "wasm32"))]
pub fn svgs_to_pdf(svg_pages: &[String]) -> Result<Vec<u8>, String> {
    if svg_pages.is_empty() {
        return Err("페이지가 없습니다".to_string());
    }
    if svg_pages.len() == 1 {
        return svg_to_pdf(&svg_pages[0]);
    }

    use pdf_writer::{Finish, Pdf, Ref};
    use std::collections::HashMap;

    let fontdb = create_fontdb();
    let mut options = usvg::Options::default();
    options.fontdb = std::sync::Arc::new(fontdb);

    let mut alloc = Ref::new(1);
    let catalog_ref = alloc.bump();
    let page_tree_ref = alloc.bump();

    // 각 페이지의 SVG를 파싱하여 chunk + page 정보 수집
    struct PageData {
        chunk: pdf_writer::Chunk,
        svg_ref: Ref,
        width: f32,
        height: f32,
    }

    let mut page_datas: Vec<PageData> = Vec::new();

    for svg in svg_pages {
        let svg_with_fallback = add_font_fallbacks(svg);
        let tree = usvg::Tree::from_str(&svg_with_fallback, &options)
            .map_err(|e| format!("SVG 파싱 실패: {}", e))?;

        let (chunk, svg_ref) = svg2pdf::to_chunk(&tree, svg2pdf::ConversionOptions::default())
            .map_err(|e| format!("SVG→chunk 변환 실패: {:?}", e))?;

        let dpi_ratio = 72.0 / 96.0; // 96 DPI → 72 pt
        let w = tree.size().width() * dpi_ratio;
        let h = tree.size().height() * dpi_ratio;

        page_datas.push(PageData {
            chunk,
            svg_ref,
            width: w,
            height: h,
        });
    }

    // 각 chunk를 재번호화하고 페이지 참조 수집
    let mut page_refs: Vec<Ref> = Vec::new();
    let mut renumbered_chunks: Vec<pdf_writer::Chunk> = Vec::new();
    let mut svg_refs_remapped: Vec<Ref> = Vec::new();

    for pd in &page_datas {
        let page_ref = alloc.bump();
        let content_ref = alloc.bump();
        page_refs.push(page_ref);

        // chunk 재번호화
        let mut map = HashMap::new();
        let renumbered = pd
            .chunk
            .renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));

        let remapped_svg_ref = map.get(&pd.svg_ref).copied().unwrap_or(pd.svg_ref);
        svg_refs_remapped.push(remapped_svg_ref);
        renumbered_chunks.push(renumbered);
    }

    // PDF 생성
    let mut pdf = Pdf::new();
    pdf.catalog(catalog_ref).pages(page_tree_ref);
    pdf.pages(page_tree_ref)
        .count(page_refs.len() as i32)
        .kids(page_refs.iter().copied());

    // 각 페이지 생성
    let svg_name = pdf_writer::Name(b"S1");

    for (i, pd) in page_datas.iter().enumerate() {
        let page_ref = page_refs[i];
        let content_ref = alloc.bump();
        let svg_ref = svg_refs_remapped[i];

        let mut page = pdf.page(page_ref);
        page.media_box(pdf_writer::Rect::new(0.0, 0.0, pd.width, pd.height));
        page.parent(page_tree_ref);
        page.contents(content_ref);

        let mut resources = page.resources();
        resources.x_objects().pair(svg_name, svg_ref);
        resources.finish();
        page.finish();

        // 컨텐츠 스트림: SVG XObject를 페이지 크기에 맞게 배치
        let mut content = pdf_writer::Content::new();
        content.transform([pd.width, 0.0, 0.0, pd.height, 0.0, 0.0]);
        content.x_object(svg_name);

        pdf.stream(content_ref, &content.finish());
    }

    // 모든 chunk를 PDF에 추가
    for chunk in &renumbered_chunks {
        pdf.extend(chunk);
    }

    // 문서 정보
    let info_ref = alloc.bump();
    pdf.document_info(info_ref)
        .producer(pdf_writer::TextStr("rhwp"));

    Ok(pdf.finish())
}
