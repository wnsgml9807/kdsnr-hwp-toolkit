//! `wire_probe` — Phase B-1 첫 wire 시도.
//!
//! rhwp PageRenderTree → 우리 SvgSurface adapter. parser/layout/pagination 은 rhwp 가
//! 만든 RenderNode tree 를 빌리고, SVG emit 만 우리 SvgSurface 가 담당.
//!
//! ## 흐름
//!
//! ```text
//!   hwpx ──► rhwp parse + paginate ──► PageRenderTree (page 0)
//!                                              │
//!                                              ▼
//!                              wire_probe::traverse(&node, &mut svg)
//!                                              │
//!                              dispatch on node_type:
//!                                ├─ TextRun     → svg.draw_string_point
//!                                ├─ Rectangle   → svg.fill_rect_float + outline_rect_float
//!                                ├─ Line        → svg.outline_path
//!                                ├─ Image       → svg.draw_image_f
//!                                ├─ TableCell   → 4 borders + recurse
//!                                ├─ Table/Page/Body/Header/Footer/Column/MasterPage → recurse
//!                                └─ (others)    → skip + count
//!                                              │
//!                                              ▼
//!                                       svg.finish() → SVG string
//!                                              │
//!                                              ▼
//!                                       resvg → PNG
//!                                              │
//!                                              ▼
//!                                       pixel_diff vs GT
//!
//! ```
//!
//! ## 1차 매핑 정공법 정책
//!
//! - rhwp 의 raw_font_family (한국어 face 이름) 를 HFT canonical name 으로 mapping
//!   table 매핑 (정확치 미상 — alias 미실측 → 일부 글리프 missing 가능)
//! - color_ref → kdsnr_render::Color::from_rgb
//! - bbox 절대 좌표 그대로 사용 (rhwp 가 dpi 적용 후 px 좌표)
//! - 매핑 없는 노드 type 은 skip + count → missing list 로 보고

use kdsnr_render::brush::{Brush, EmptyBrush, SolidBrush};
use kdsnr_render::color::Color;
use kdsnr_render::surface::{Path, PathCmd};
use kdsnr_render::pen::Pen;
use kdsnr_render::pixel_diff_harness::{make_heatmap_rgba, score_pages, DiffOptions};
use kdsnr_render::surface::{Font, Image, PointImpl, RectImpl, StringFormat, Surface};
use kdsnr_render::svg_surface::SvgSurface;
use std::collections::HashMap;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;

// ─── 노드 type 별 카운터 ──────────────────────────────────────────────

#[derive(Debug, Default)]
struct Counts {
    text_run: u32,
    rectangle: u32,
    line: u32,
    image: u32,
    table_cell: u32,
    table: u32,
    page: u32,
    body: u32,
    column: u32,
    header: u32,
    footer: u32,
    master_page: u32,
    text_line: u32,
    page_background: u32,
    // skip
    path: u32,
    ellipse: u32,
    equation: u32,
    group: u32,
    text_box: u32,
    form_object: u32,
    footnote_marker: u32,
    footnote_area: u32,
    placeholder: u32,
    raw_svg: u32,
    // text 매핑 통계
    text_skipped_no_font: u32,
    text_rendered: u32,
    image_skipped_no_data: u32,
    image_rendered: u32,
}

// ─── HFT canonical name mapping (rhwp raw_font_family → HFT canonical) ─

/// rhwp 의 raw_font_family / font_family 를 HFT cache 의 canonical name 으로 mapping.
///
/// **주의**: 본 mapping 은 실측 alias 없이 추정. hft-decoder/rust/test-data 에 4 family
/// 만 있어 정확한 face → canonical 매핑이 어려움. 다음 hardcoded mapping 으로 1차 시도.
fn map_font_family(rhwp_family: &str) -> Option<String> {
    let f = rhwp_family.trim();
    if f.is_empty() {
        return Some("HGMJ".to_string());
    }
    // 한글 한컴 face → 후보 HFT canonical
    let candidates: &[(&str, &str)] = &[
        ("함초롬바탕", "HGMJ"),
        ("신명 중명조", "HGMJ"),
        ("한양신명조", "HGMJ"),
        ("바탕", "HGMJ"),
        ("Haansoft Batang", "HGMJ"),
        ("HY중고딕", "HCHGGGT"),
        ("함초롬돋움", "HCHGGGT"),
        ("돋움", "HCHGGGT"),
        ("Haansoft Dotum", "HCHGGGT"),
        ("한컴고딕", "HCHGGGT"),
        ("맑은 고딕", "HCHGGGT"),
        ("HY견고딕", "HCHGGGT"),
        ("한양견고딕", "HCHGGGT"),
        ("HY헤드라인M", "HCHGGGT"),
        ("HJSMJ", "HJSMJ"),
        ("ENSMJ", "ENSMJ"),
        ("HGMJ", "HGMJ"),
        ("HCHGGGT", "HCHGGGT"),
    ];
    for (face, canonical) in candidates {
        if f.contains(face) || f.eq_ignore_ascii_case(face) {
            return Some(canonical.to_string());
        }
    }
    // 영문 face (Times, Arial 등) → ENSMJ 시도
    if f.chars().all(|c| c.is_ascii()) {
        return Some("ENSMJ".to_string());
    }
    // unknown 한글 face → HGMJ fallback
    Some("HGMJ".to_string())
}

// ─── ColorRef u32 → kdsnr_render::Color ───────────────────────────────

fn color_ref_to_color(cref: u32) -> Color {
    // rhwp ColorRef = 0x00BBGGRR (BGR little-endian) — `helpers::color_ref_to_css` 참조
    let r = (cref & 0xff) as u8;
    let g = ((cref >> 8) & 0xff) as u8;
    let b = ((cref >> 16) & 0xff) as u8;
    Color::from_rgb(r, g, b, std::ptr::null_mut())
}

fn solid_brush(cref: u32) -> Brush {
    Brush::Solid(SolidBrush::new(color_ref_to_color(cref)))
}

fn pen_for(stroke_color: u32, width: f64) -> Pen {
    let mut pen = Pen::new_default();
    pen.brush = Box::new(solid_brush(stroke_color));
    pen.set_thickness(width as f32);
    pen
}

fn empty_pen() -> Pen {
    let mut pen = Pen::new_default();
    pen.brush = Box::new(Brush::Empty(EmptyBrush::new()));
    pen
}

// ─── traverse + dispatch ───────────────────────────────────────────────

fn traverse(
    node: &rhwp::renderer::render_tree::RenderNode,
    svg: &mut SvgSurface,
    bin_data: &HashMap<usize, Vec<u8>>,
    cache: &kdsnr_hft::HftCache,
    counts: &mut Counts,
) {
    use rhwp::renderer::render_tree::RenderNodeType as T;
    if !node.visible {
        return;
    }
    let bbox = node.bbox;
    let rect_f32 = RectImpl {
        x: bbox.x as f32,
        y: bbox.y as f32,
        w: bbox.width as f32,
        h: bbox.height as f32,
    };

    match &node.node_type {
        T::Page(_) => {
            counts.page += 1;
            // 자식만 traverse (페이지 자체 그리기 없음)
        }
        T::PageBackground(_pbg) => {
            counts.page_background += 1;
            // 단순화: 흰 배경. 한컴 PDF GT 는 흰배경이므로 fill 안 해도 됨.
        }
        T::MasterPage => { counts.master_page += 1; }
        T::Header => { counts.header += 1; }
        T::Footer => { counts.footer += 1; }
        T::Body { .. } => { counts.body += 1; }
        T::Column(_) => { counts.column += 1; }
        T::FootnoteArea => { counts.footnote_area += 1; }
        T::TextLine(_) => { counts.text_line += 1; }
        T::TextRun(tr) => {
            counts.text_run += 1;
            // raw_font_family 우선, 없으면 font_family
            let face = if !tr.style.raw_font_family.is_empty() {
                &tr.style.raw_font_family
            } else {
                &tr.style.font_family
            };
            // DIAG: 첫 5개 face name 추적
            if counts.text_run <= 5 {
                eprintln!("  [text {}] face=\"{}\" font_family=\"{}\" size={} text=\"{}\"",
                          counts.text_run, face, tr.style.font_family,
                          tr.style.font_size,
                          tr.text.chars().take(20).collect::<String>());
            }
            // HFT cache 의 alias map (hftinfo.dat) 이 face name → canonical 매핑.
            // 따라서 face name 을 그대로 cache.get() 에 넘기면 자동 lookup.
            // 우리 map_font_family 의 hardcoded mapping 은 alias miss 시 fallback.
            let text16: Vec<u16> = tr.text.encode_utf16().collect();
            if text16.is_empty() {
                return;
            }
            // 1순위: face name 그대로 (alias 작동), 2순위: font_family, 3순위: hardcoded
            let mut family_used = face.clone();
            let mut any_hit = false;
            for cp in text16.iter().take(5) {
                if cache.get(&family_used, *cp as u32).is_some() {
                    any_hit = true;
                    break;
                }
            }
            if !any_hit && !tr.style.font_family.is_empty() {
                let f2 = tr.style.font_family.clone();
                for cp in text16.iter().take(5) {
                    if cache.get(&f2, *cp as u32).is_some() {
                        family_used = f2.clone();
                        any_hit = true;
                        break;
                    }
                }
            }
            if !any_hit {
                if let Some(canonical) = map_font_family(face) {
                    for cp in text16.iter().take(5) {
                        if cache.get(&canonical, *cp as u32).is_some() {
                            family_used = canonical.clone();
                            any_hit = true;
                            break;
                        }
                    }
                }
            }
            // baseline y = bbox.y + baseline
            let baseline_y = bbox.y as f32 + tr.baseline as f32;
            let font = Font {
                family: family_used,
                size: tr.style.font_size as f32,
                bold: tr.style.bold,
                italic: tr.style.italic,
            };
            if counts.text_run <= 5 {
                let mut hits = 0usize;
                let mut miss_samples: Vec<u32> = Vec::new();
                for cp in text16.iter().take(20) {
                    if cache.get(&font.family, *cp as u32).is_some() {
                        hits += 1;
                    } else if miss_samples.len() < 3 {
                        miss_samples.push(*cp as u32);
                    }
                }
                eprintln!("    → resolved=\"{}\"  hits {}/{}  miss_samples={:?}",
                          font.family, hits, text16.len().min(20), miss_samples);
            }
            let brush = solid_brush(tr.style.color);
            svg.draw_string_point(
                &text16,
                &font,
                PointImpl { x: bbox.x as f32, y: baseline_y },
                &brush,
                &StringFormat::default(),
            );
            let mut any_hit = false;
            for cp in text16.iter().take(20) {
                if cache.get(&font.family, *cp as u32).is_some() {
                    any_hit = true;
                    break;
                }
            }
            if any_hit { counts.text_rendered += 1; }
            else { counts.text_skipped_no_font += 1; }
        }
        T::Rectangle(rect_node) => {
            counts.rectangle += 1;
            // fill
            if let Some(fc) = rect_node.style.fill_color {
                svg.fill_rect_float(rect_f32, &solid_brush(fc));
            }
            // stroke
            if let Some(sc) = rect_node.style.stroke_color {
                let pen = pen_for(sc, rect_node.style.stroke_width.max(0.25));
                svg.outline_rect_float(rect_f32, &pen);
            }
        }
        T::Line(line_node) => {
            counts.line += 1;
            // single segment path
            let path = make_line_path(
                line_node.x1 as f32, line_node.y1 as f32,
                line_node.x2 as f32, line_node.y2 as f32,
            );
            let pen = pen_for(line_node.style.color, line_node.style.width.max(0.25));
            svg.outline_path(&path, &pen);
        }
        T::Image(img_node) => {
            counts.image += 1;
            // rhwp ImageNode.data: Option<Vec<u8>>
            let Some(data) = &img_node.data else {
                counts.image_skipped_no_data += 1;
                return;
            };
            if data.is_empty() {
                counts.image_skipped_no_data += 1;
                return;
            }
            counts.image_rendered += 1;
            let image = Image {
                data: data.clone(),
                width: bbox.width as u32,
                height: bbox.height as u32,
            };
            svg.draw_image_f(rect_f32, &image, 1.0);
        }
        T::Table(_tn) => {
            counts.table += 1;
            // 자식 (TableCell) 만 traverse
        }
        T::TableCell(_tc) => {
            counts.table_cell += 1;
            // 셀 자체 border 는 rhwp 가 별도 LineNode/RectangleNode 자식으로 emit.
            // bbox 만 dump 했다가 children 으로 내려감.
        }
        T::Ellipse(_) => { counts.ellipse += 1; }
        T::Path(_) => { counts.path += 1; }
        T::Equation(_) => { counts.equation += 1; }
        T::Group(_) => { counts.group += 1; }
        T::TextBox => { counts.text_box += 1; }
        T::FormObject(_) => { counts.form_object += 1; }
        T::FootnoteMarker(_) => { counts.footnote_marker += 1; }
        T::Placeholder(_) => { counts.placeholder += 1; }
        T::RawSvg(raw) => {
            counts.raw_svg += 1;
            // svg buffer 에 그대로 push — well-formed 가정
            svg.buffer.push_str(&raw.svg);
            svg.buffer.push('\n');
        }
    }

    // children recurse
    for child in &node.children {
        traverse(child, svg, bin_data, cache, counts);
    }

    let _ = bin_data;
}

fn make_line_path(x1: f32, y1: f32, x2: f32, y2: f32) -> Path {
    let mut p = Path::default();
    p.commands.push(PathCmd::MoveTo(x1, y1));
    p.commands.push(PathCmd::LineTo(x2, y2));
    p
}

// ─── PNG IO ────────────────────────────────────────────────────────────

fn read_png_rgba(path: &StdPath) -> (Vec<u8>, u32, u32) {
    let f = std::fs::File::open(path).expect("png open");
    let mut r = png::Decoder::new(f).read_info().expect("png hdr");
    let mut buf = vec![0u8; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).expect("png next");
    let (w, h) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let n = (w * h) as usize;
            let mut o = Vec::with_capacity(n * 4);
            for i in 0..n {
                o.push(buf[i * 3]); o.push(buf[i * 3 + 1]); o.push(buf[i * 3 + 2]); o.push(0xff);
            }
            o
        }
        _ => panic!("unsupported color type {:?}", info.color_type),
    };
    (rgba, w, h)
}

fn write_png_rgba(path: &StdPath, rgba: &[u8], w: u32, h: u32) {
    if let Some(p) = path.parent() { std::fs::create_dir_all(p).ok(); }
    let f = std::fs::File::create(path).expect("png create");
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut wr = e.write_header().expect("png hdr");
    wr.write_image_data(rgba).expect("png data");
}

fn rasterize_svg(svg: &str, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("usvg: {e}"))?;
    let sz = tree.size();
    let sx = w as f32 / sz.width();
    let sy = h as f32 / sz.height();
    let t = tiny_skia::Transform::from_scale(sx, sy);
    let mut pix = tiny_skia::Pixmap::new(w, h).ok_or("pixmap alloc")?;
    pix.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, t, &mut pix.as_mut());
    Ok(pix.take())
}

// ─── main ─────────────────────────────────────────────────────────────

fn usage() -> ! {
    eprintln!("usage: wire_probe <hwpx> <gt.png> [--page N] [--out DIR] [--hft DIR]");
    std::process::exit(2);
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.len() < 3 { usage(); }
    let hwpx = PathBuf::from(&argv[1]);
    let gt_png = PathBuf::from(&argv[2]);
    let mut page: u32 = 0;
    let mut out_dir = PathBuf::from("work/e2e/_wire_output");
    let mut hft_dir = PathBuf::from("hft-decoder/rust/test-data");
    let mut i = 3;
    while i < argv.len() {
        match argv[i].as_str() {
            "--page" => { page = argv[i+1].parse().unwrap(); i += 2; }
            "--out" => { out_dir = PathBuf::from(&argv[i+1]); i += 2; }
            "--hft" => { hft_dir = PathBuf::from(&argv[i+1]); i += 2; }
            other => { eprintln!("unknown {other}"); usage(); }
        }
    }
    std::fs::create_dir_all(&out_dir).ok();

    eprintln!("=== wire_probe ===");
    eprintln!("hwpx: {}", hwpx.display());
    eprintln!("gt  : {}", gt_png.display());

    // HFT load — embedded archive 우선 (binary 에 박힌 180MB). --hft path 명시
    // 시 fs 에서도 추가 로드 (alias 보강용).
    let mut cache = kdsnr_hft::HftCache::new();
    let n_emb = kdsnr_hft::embedded::load_into(&mut cache).expect("embedded HFT load");
    eprintln!("HFT embedded: {n_emb} glyphs ({} families, {} aliases)",
              cache.family_count(), cache.alias_count());
    if hft_dir.exists() {
        let n_extra = cache.load_dir(&hft_dir).unwrap_or(0);
        if n_extra > 0 {
            eprintln!("HFT extra (fs): {n_extra} glyphs (now {} families)", cache.family_count());
        }
    }
    let n = n_emb;
    let cache_arc = Arc::new(cache);
    let cache_for_lookup: &kdsnr_hft::HftCache = &cache_arc;
    // (rhwp 도 global cache 사용 가능 — wire_probe 는 우리 SvgSurface 만 쓰니 무관)

    // GT 크기
    let (gt_rgba, gt_w, gt_h) = read_png_rgba(&gt_png);
    eprintln!("gt size: {gt_w} x {gt_h}");

    // rhwp parse + render_tree
    let data = std::fs::read(&hwpx).expect("hwpx read");
    let doc = rhwp::wasm_api::HwpDocument::from_bytes(&data).expect("rhwp parse");
    let pc = doc.page_count();
    eprintln!("rhwp pages: {pc}");
    if page >= pc { panic!("--page {page} >= {pc}"); }
    let tree = doc.build_page_render_tree(page).expect("build tree");
    eprintln!("tree root type: {:?}",
              std::mem::discriminant(&tree.root.node_type));

    // page bbox → SvgSurface 크기
    let page_w = tree.root.bbox.width as f32;
    let page_h = tree.root.bbox.height as f32;
    eprintln!("page bbox: {} x {}", page_w, page_h);

    // SvgSurface 생성
    let mut svg = SvgSurface::new(page_w, page_h).with_hft_cache(Arc::clone(&cache_arc));

    // 흰 배경 — 한컴 PDF 가 흰배경이라 같은 가정
    svg.fill_rect_float(
        RectImpl { x: 0.0, y: 0.0, w: page_w, h: page_h },
        &Brush::Solid(SolidBrush::new(Color::from_rgb(255, 255, 255, std::ptr::null_mut()))),
    );

    // traverse
    let bin_data: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut counts = Counts::default();
    traverse(&tree.root, &mut svg, &bin_data, cache_for_lookup, &mut counts);

    let svg_str = svg.finish();
    let stem = hwpx.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let svg_path = out_dir.join(format!("{stem}_page{:02}_ours.svg", page));
    std::fs::write(&svg_path, &svg_str).expect("svg write");
    eprintln!("SVG: {} bytes → {}", svg_str.len(), svg_path.display());

    // resvg 로 PNG (GT 크기로)
    let our_rgba = match rasterize_svg(&svg_str, gt_w, gt_h) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("RASTERIZE FAIL: {e}");
            eprintln!("(svg saved for inspection)");
            print_counts(&counts);
            return;
        }
    };
    let png_path = out_dir.join(format!("{stem}_page{:02}_ours.png", page));
    write_png_rgba(&png_path, &our_rgba, gt_w, gt_h);
    eprintln!("PNG: {}", png_path.display());

    // pixel-diff
    let opts = DiffOptions::default();
    let score = score_pages(&our_rgba, &gt_rgba, gt_w, gt_h, &opts).expect("score");
    let heat = make_heatmap_rgba(&our_rgba, &gt_rgba, gt_w, gt_h, &opts).expect("heatmap");
    let heat_path = out_dir.join(format!("{stem}_page{:02}_heatmap.png", page));
    write_png_rgba(&heat_path, &heat, gt_w, gt_h);
    eprintln!("heatmap: {}", heat_path.display());

    println!();
    println!("=== PageScore (rhwp-tree → ours SvgSurface) ===");
    println!("  total      : {}", score.total_pixels);
    println!("  matched    : {}", score.matched_pixels);
    println!("  mismatched : {}", score.mismatched_pixels);
    println!("  score      : {:.2}%", score.score_pct);
    println!("  avg_delta  : {:.2}", score.avg_delta);

    print_counts(&counts);
}

fn print_counts(c: &Counts) {
    println!();
    println!("=== Node counts ===");
    println!("  Page              : {}", c.page);
    println!("  Body              : {}", c.body);
    println!("  Header / Footer   : {} / {}", c.header, c.footer);
    println!("  MasterPage        : {}", c.master_page);
    println!("  Column            : {}", c.column);
    println!("  FootnoteArea      : {}", c.footnote_area);
    println!("  PageBackground    : {}", c.page_background);
    println!("  TextLine          : {}", c.text_line);
    println!("  TextRun           : {} (rendered: {}, skip-no-font: {})",
             c.text_run, c.text_rendered, c.text_skipped_no_font);
    println!("  Rectangle         : {}", c.rectangle);
    println!("  Line              : {}", c.line);
    println!("  Image             : {} (rendered: {}, skip-no-data: {})",
             c.image, c.image_rendered, c.image_skipped_no_data);
    println!("  Table / TableCell : {} / {}", c.table, c.table_cell);
    println!("  -- skipped types --");
    println!("  Path              : {}", c.path);
    println!("  Ellipse           : {}", c.ellipse);
    println!("  Equation          : {}", c.equation);
    println!("  Group             : {}", c.group);
    println!("  TextBox           : {}", c.text_box);
    println!("  FormObject        : {}", c.form_object);
    println!("  FootnoteMarker    : {}", c.footnote_marker);
    println!("  Placeholder       : {}", c.placeholder);
    println!("  RawSvg            : {}", c.raw_svg);
}
