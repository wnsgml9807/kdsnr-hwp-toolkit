//! `e2e_bench` — first end-to-end pixel-diff measurement.
//!
//! Stage 5 의 첫 실측. 본 bench 는 **rhwp 의 SVG 출력** 을 baseline 으로 한컴 PDF→PNG GT 와
//! 비교한다. (우리 SvgSurface 어댑터 출력이 아니다 — 그건 R-5 chain 이 e2e wire 된 뒤 별도.)
//!
//! ## 흐름
//!
//! ```text
//!   hwpx ──► rhwp::HwpDocument::render_page_svg_native(page) ──► svg string
//!                                                                    │
//!                                                                    ▼
//!                                                          usvg::Tree::from_str
//!                                                                    │
//!                                                                    ▼
//!                                                          resvg::render (tiny-skia)
//!                                                                    │
//!                                                                    ▼
//!                                                              our.rgba8 (w×h)
//!                                                                    │
//!                                                                    ├── score_pages ──► PageScore
//!   gt.png ──► png::Decoder ──► gt.rgba8 (w×h) ───────────────────────┘
//! ```
//!
//! ## 사용
//!
//! ```bash
//! cargo run --release --example e2e_bench -- <hwpx> <gt.png> [--page N] [--out DIR]
//! ```
//!
//! 출력:
//! - stdout: score summary (matched/total + score% + avg_delta)
//! - `<out>/<stem>_page<N>_ours.png` — rhwp baseline rasterized PNG
//! - `<out>/<stem>_page<N>_heatmap.png` — mismatch heatmap (red=mismatch)
//!
//! ## 정공법 정책
//!
//! 본 bench 는 **첫 측정** 이 목적. rhwp 의 svg 는 "참고용만, 한컴 1:1 아님" (memory:
//! reference_rhwp_svg_renderer.md) 이므로 점수는 **rhwp baseline %** — 이게 곧 우리
//! SvgSurface adapter 가 wire 되었을 때 *최소한* 넘어야 할 score 다.

use kdsnr_render::pixel_diff_harness::{make_heatmap_rgba, score_pages, DiffOptions};
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "usage: e2e_bench <hwpx> <gt.png> [--page N] [--out DIR] [--hft DIR]"
    );
    eprintln!();
    eprintln!("options:");
    eprintln!("  --page N      한 페이지만 비교 (0-based, default 0)");
    eprintln!("  --out DIR     결과 PNG 출력 디렉토리 (default work/e2e/_bench_output)");
    eprintln!("  --hft DIR     HFT 폰트 캐시 디렉토리 (rhwp text advance 측정)");
    std::process::exit(2);
}

struct Args {
    hwpx: PathBuf,
    gt_png: PathBuf,
    page: u32,
    out_dir: PathBuf,
    hft_dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 3 {
        usage();
    }
    let hwpx = PathBuf::from(&argv[1]);
    let gt_png = PathBuf::from(&argv[2]);
    let mut page: u32 = 0;
    let mut out_dir = PathBuf::from("work/e2e/_bench_output");
    let mut hft_dir: Option<PathBuf> = None;
    let mut i = 3;
    while i < argv.len() {
        match argv[i].as_str() {
            "--page" => {
                page = argv
                    .get(i + 1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| {
                        eprintln!("--page 인자 누락/parse 실패");
                        usage();
                    });
                i += 2;
            }
            "--out" => {
                out_dir = PathBuf::from(argv.get(i + 1).unwrap_or_else(|| usage()));
                i += 2;
            }
            "--hft" => {
                hft_dir = Some(PathBuf::from(argv.get(i + 1).unwrap_or_else(|| usage())));
                i += 2;
            }
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }
    Args {
        hwpx,
        gt_png,
        page,
        out_dir,
        hft_dir,
    }
}

fn read_png_rgba(path: &Path) -> (Vec<u8>, u32, u32) {
    let file = std::fs::File::open(path)
        .unwrap_or_else(|e| panic!("PNG open fail: {} — {e}", path.display()));
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("PNG header read fail");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("PNG decode fail");
    let (w, h) = (info.width, info.height);
    // RGBA 강제 변환 — png crate 은 RGB 인 경우 그대로 주므로 패딩 필요.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let n = (w * h) as usize;
            let mut out = Vec::with_capacity(n * 4);
            for i in 0..n {
                out.push(buf[i * 3]);
                out.push(buf[i * 3 + 1]);
                out.push(buf[i * 3 + 2]);
                out.push(0xff);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let n = (w * h) as usize;
            let mut out = Vec::with_capacity(n * 4);
            for i in 0..n {
                let g = buf[i * 2];
                out.push(g);
                out.push(g);
                out.push(g);
                out.push(buf[i * 2 + 1]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let n = (w * h) as usize;
            let mut out = Vec::with_capacity(n * 4);
            for i in 0..n {
                let g = buf[i];
                out.push(g);
                out.push(g);
                out.push(g);
                out.push(0xff);
            }
            out
        }
        other => panic!("unsupported PNG color type: {other:?}"),
    };
    (rgba, w, h)
}

fn write_png_rgba(path: &Path, rgba: &[u8], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path)
        .unwrap_or_else(|e| panic!("PNG create fail: {} — {e}", path.display()));
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header write fail");
    writer.write_image_data(rgba).expect("PNG data write fail");
}

/// SVG string → RGBA buffer at given (target_w, target_h).
///
/// usvg 로 parse → resvg 로 tiny-skia Pixmap 에 rasterize → premultiplied RGBA 추출.
/// target 사이즈가 SVG viewBox 와 다르면 transform 으로 scale 적용.
fn rasterize_svg(svg: &str, target_w: u32, target_h: u32) -> Vec<u8> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).expect("usvg parse fail");

    let svg_size = tree.size();
    let sx = target_w as f32 / svg_size.width();
    let sy = target_h as f32 / svg_size.height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    let mut pixmap = tiny_skia::Pixmap::new(target_w, target_h).expect("Pixmap alloc fail");
    // 흰 배경 fill — rhwp svg 는 transparent bg, GT 한컴 PDF 는 흰배경.
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia 는 premultiplied RGBA. score_pages 는 일반 RGBA 비교라 그대로 사용.
    pixmap.take()
}

fn main() {
    let args = parse_args();

    eprintln!("=== e2e_bench ===");
    eprintln!("hwpx   : {}", args.hwpx.display());
    eprintln!("gt png : {}", args.gt_png.display());
    eprintln!("page   : {}", args.page);
    eprintln!("out    : {}", args.out_dir.display());

    // 1. GT PNG load → 크기 기준점
    let (gt_rgba, gt_w, gt_h) = read_png_rgba(&args.gt_png);
    eprintln!("gt size: {} x {}", gt_w, gt_h);

    // 2. (옵션) HFT cache 주입
    if let Some(hft_dir) = &args.hft_dir {
        let mut cache = rhwp::kdsnr_hft::HftCache::new();
        match cache.load_dir(hft_dir) {
            Ok(n) => {
                eprintln!("HFT load: {n} glyphs ({} families)", cache.family_count());
                let arc = std::sync::Arc::new(cache);
                rhwp::renderer::font_runtime_metrics::set_global_hft_cache(arc);
            }
            Err(e) => eprintln!("HFT load FAIL: {e} (continuing without HFT)"),
        }
    }

    // 3. rhwp 로 hwpx parse + 페이지 SVG 렌더
    let data = std::fs::read(&args.hwpx).expect("hwpx read fail");
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&data).expect("rhwp parse fail");
    let pc = doc.page_count();
    eprintln!("rhwp page count: {pc}");
    if args.page >= pc {
        panic!("--page {} >= page_count {}", args.page, pc);
    }
    let svg = doc
        .render_page_svg_native(args.page)
        .expect("rhwp render_page_svg_native fail");
    eprintln!("svg size: {} bytes", svg.len());

    // 4. SVG → RGBA at GT dims
    let our_rgba = rasterize_svg(&svg, gt_w, gt_h);

    // 5. 결과 PNG 저장 (디버깅용)
    let stem = args
        .hwpx
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page");
    let ours_path = args
        .out_dir
        .join(format!("{stem}_page{:02}_ours.png", args.page));
    write_png_rgba(&ours_path, &our_rgba, gt_w, gt_h);
    eprintln!("wrote ours: {}", ours_path.display());

    // 6. pixel-diff score
    let opts = DiffOptions::default();
    let score = score_pages(&our_rgba, &gt_rgba, gt_w, gt_h, &opts)
        .expect("score_pages fail");

    // 7. heatmap
    let heatmap = make_heatmap_rgba(&our_rgba, &gt_rgba, gt_w, gt_h, &opts)
        .expect("heatmap fail");
    let heat_path = args
        .out_dir
        .join(format!("{stem}_page{:02}_heatmap.png", args.page));
    write_png_rgba(&heat_path, &heatmap, gt_w, gt_h);
    eprintln!("wrote heatmap: {}", heat_path.display());

    // 8. summary
    println!();
    println!("=== PageScore ===");
    println!("  total      : {}", score.total_pixels);
    println!("  matched    : {}", score.matched_pixels);
    println!("  mismatched : {}", score.mismatched_pixels);
    println!("  score      : {:.2}%", score.score_pct);
    println!("  avg_delta  : {:.2}", score.avg_delta);
}
