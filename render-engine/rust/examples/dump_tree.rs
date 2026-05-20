//! `dump_tree` — rhwp PageRenderTree 의 JSON dump.
//!
//! Header / Column 같은 노드의 정체와 자식 구조 진단용.
//!
//! 사용:
//! ```bash
//! cargo run --release --example dump_tree -- <hwpx> [--page N]
//! ```

use std::path::PathBuf;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 2 {
        eprintln!("usage: dump_tree <hwpx> [--page N]");
        std::process::exit(2);
    }
    let hwpx = PathBuf::from(&argv[1]);
    let page: u32 = argv.iter().position(|a| a == "--page")
        .and_then(|i| argv.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(0);

    let data = std::fs::read(&hwpx).expect("read hwpx");
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&data).expect("rhwp parse");
    let tree = doc.build_page_render_tree(page).expect("build tree");

    eprintln!("page bbox: x={} y={} w={} h={}",
              tree.root.bbox.x, tree.root.bbox.y,
              tree.root.bbox.width, tree.root.bbox.height);
    println!("{}", tree.root.to_json());
}
