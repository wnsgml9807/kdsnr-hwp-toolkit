//! `probe_doc_pages` — 원본 시험지 hwpx 의 페이지 수 + 각 페이지 bbox 확인.
//!
//! 입력: hwpx 파일 경로
//! 출력: page index, page bbox, body bbox, header children count, footer children count

use std::path::PathBuf;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        eprintln!("usage: probe_doc_pages <hwpx>");
        std::process::exit(2);
    }
    let hwpx = PathBuf::from(&argv[1]);
    let data = std::fs::read(&hwpx).expect("read hwpx");
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&data).expect("rhwp parse");

    // 페이지 수 - build_page_render_tree 가 실패할 때까지 시도
    let mut page_idx: u32 = 0;
    loop {
        match doc.build_page_render_tree(page_idx) {
            Ok(tree) => {
                let h_children = count_children_by_type(&tree.root, "Header");
                let f_children = count_children_by_type(&tree.root, "Footer");
                let mp_children = count_children_by_type(&tree.root, "MasterPage");
                let body_children = count_children_by_type(&tree.root, "Body");
                eprintln!("page[{}]: bbox=({:.1}, {:.1}) header_kids={} footer_kids={} master_kids={} body_kids={}",
                    page_idx,
                    tree.root.bbox.width, tree.root.bbox.height,
                    h_children, f_children, mp_children, body_children);
            }
            Err(e) => {
                eprintln!("page[{}]: FAIL ({}) — stop", page_idx, e);
                break;
            }
        }
        page_idx += 1;
        if page_idx > 50 { eprintln!("STOP at 50"); break; }
    }
    eprintln!("=> total pages: {}", page_idx);
}

fn count_children_by_type(node: &rhwp::renderer::render_tree::RenderNode, type_name: &str) -> usize {
    use rhwp::renderer::render_tree::RenderNodeType as T;
    let mut total = 0;
    for c in &node.children {
        let kind = match &c.node_type {
            T::Header => "Header",
            T::Footer => "Footer",
            T::MasterPage => "MasterPage",
            T::Body { .. } => "Body",
            T::Column(_) => "Column",
            _ => continue,
        };
        if kind == type_name {
            total += count_descendants(c);
        }
    }
    total
}

fn count_descendants(node: &rhwp::renderer::render_tree::RenderNode) -> usize {
    node.children.len() + node.children.iter().map(count_descendants).sum::<usize>()
}
