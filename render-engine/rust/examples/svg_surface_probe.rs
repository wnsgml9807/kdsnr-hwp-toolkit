//! `svg_surface_probe` — SvgSurface 단독 단위 검증.
//!
//! wire 본격 시도 전 SvgSurface 가 실제로 SVG primitive 들을 emit 하는지 확인.
//!
//! probe 항목:
//! - `fill_rect_float` (solid brush red) — 빨강 사각형
//! - `outline_rect_float` (pen) — 검정 박스 테두리
//! - `draw_string_point` (HFT cache 필수) — 텍스트
//! - `draw_image_f` — placeholder image rect
//! - `save_state` / `restore_state` — group
//!
//! 출력:
//! - `work/e2e/_probe_output/svg_surface_probe.svg`
//! - `work/e2e/_probe_output/svg_surface_probe.png` (resvg 변환)
//! - stdout: SVG buffer 의 첫 줄 + finish() 결과 byte 수

use kdsnr_render::brush::{Brush, SolidBrush};
use kdsnr_render::color::Color;
use kdsnr_render::pen::Pen;
use kdsnr_render::surface::{Font, Image, PointImpl, RectImpl, StringFormat, Surface};
use kdsnr_render::svg_surface::SvgSurface;
use kdsnr_render::transform2d::Transform2D;
use std::path::PathBuf;
use std::sync::Arc;

fn write_png_rgba(path: &std::path::Path, rgba: &[u8], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path).expect("png create");
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

fn rasterize_svg(svg: &str, w: u32, h: u32) -> Vec<u8> {
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &opt).expect("usvg parse");
    let svg_size = tree.size();
    let sx = w as f32 / svg_size.width();
    let sy = h as f32 / svg_size.height();
    let t = tiny_skia::Transform::from_scale(sx, sy);
    let mut pix = tiny_skia::Pixmap::new(w, h).expect("pixmap");
    pix.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, t, &mut pix.as_mut());
    pix.take()
}

fn main() {
    let out_dir = PathBuf::from("work/e2e/_probe_output");
    std::fs::create_dir_all(&out_dir).ok();

    // 1) HFT cache 로드 (draw_string 용)
    let hft_dir = PathBuf::from("hft-decoder/rust/test-data");
    let mut cache = kdsnr_hft::HftCache::new();
    let n = cache.load_dir(&hft_dir).expect("HFT load");
    eprintln!("HFT load: {n} glyphs ({} families)", cache.family_count());

    // 2) SvgSurface 생성 + HFT 주입
    let canvas_w = 600.0_f32;
    let canvas_h = 400.0_f32;
    let mut svg = SvgSurface::new(canvas_w, canvas_h).with_hft_cache(Arc::new(cache));

    // 3) Surface API 호출
    // ─ 빨강 사각형 (solid brush)
    let red = Color::from_rgb(220, 60, 60, std::ptr::null_mut());
    let red_brush = Brush::Solid(SolidBrush::new(red));
    svg.fill_rect_float(
        RectImpl { x: 20.0, y: 20.0, w: 120.0, h: 80.0 },
        &red_brush,
    );

    // ─ 검정 박스 outline (pen + 명시적 black brush)
    let black = Color::from_rgb(0, 0, 0, std::ptr::null_mut());
    let mut pen = Pen::new_default();
    pen.brush = Box::new(Brush::Solid(SolidBrush::new(Color::from_rgb(0, 0, 0, std::ptr::null_mut()))));
    pen.set_thickness(1.5);
    svg.outline_rect_float(
        RectImpl { x: 180.0, y: 20.0, w: 200.0, h: 100.0 },
        &pen,
    );

    // ─ 텍스트 (draw_string_point) — HFT canonical name "HCHGGGT" (함초롬돋움)
    let text: Vec<u16> = "Hello 안녕 1234".encode_utf16().collect();
    let font = Font {
        family: "HCHGGGT".to_string(),
        size: 32.0,
        bold: false,
        italic: false,
    };
    let black_brush = Brush::Solid(SolidBrush::new(black));
    svg.draw_string_point(
        &text,
        &font,
        PointImpl { x: 20.0, y: 200.0 },
        &black_brush,
        &StringFormat::default(),
    );

    // ─ 텍스트 두 번째 — HGMJ (함초롬바탕 추정)
    let text2: Vec<u16> = "abc 가나다 0123".encode_utf16().collect();
    let font2 = Font {
        family: "HGMJ".to_string(),
        size: 24.0,
        bold: false,
        italic: false,
    };
    svg.draw_string_point(
        &text2,
        &font2,
        PointImpl { x: 20.0, y: 250.0 },
        &black_brush,
        &StringFormat::default(),
    );

    // ─ placeholder image rect (data 없음 → "#missing-img" href, 시각 표시 안 됨)
    let image = Image { data: vec![], width: 100, height: 80 };
    svg.draw_image_f(
        RectImpl { x: 420.0, y: 150.0, w: 150.0, h: 100.0 },
        &image,
        1.0,
    );

    // ─ 파란 사각형 + 그 안의 transform (save/concat/restore) — group balance 검증
    let blue_brush = Brush::Solid(SolidBrush::new(Color::from_rgb(80, 80, 240, std::ptr::null_mut())));
    svg.save_state();
    let mut translate = Transform2D::new();
    translate.set_x_offset(50.0);
    translate.set_y_offset(300.0);
    svg.concat_transform(&translate);
    svg.fill_rect_float(
        RectImpl { x: 0.0, y: 0.0, w: 100.0, h: 60.0 },
        &blue_brush,
    );
    svg.restore_state();

    // 4) finish + 저장
    let svg_str = svg.finish();
    let svg_path = out_dir.join("svg_surface_probe.svg");
    std::fs::write(&svg_path, &svg_str).expect("svg write");
    eprintln!("SVG ({} bytes): {}", svg_str.len(), svg_path.display());

    // 5) resvg 로 PNG
    let png_w = canvas_w as u32;
    let png_h = canvas_h as u32;
    let rgba = rasterize_svg(&svg_str, png_w, png_h);
    let png_path = out_dir.join("svg_surface_probe.png");
    write_png_rgba(&png_path, &rgba, png_w, png_h);
    eprintln!("PNG: {}", png_path.display());

    // 6) 진단: SVG buffer 의 첫 5줄 출력
    println!("=== svg buffer first 10 lines ===");
    for (i, line) in svg.buffer.lines().take(10).enumerate() {
        println!("  [{:>2}] {}", i, line);
    }
    println!();
    println!("total svg bytes: {}", svg_str.len());
    println!("buffer ops (line count): {}", svg.buffer.lines().count());

    // 7) 검은 픽셀 비율 (PNG 에 글자/도형 그려졌는지)
    let mut dark = 0u32;
    let mut colored = 0u32;
    let n = rgba.len() / 4;
    for i in 0..n {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        let b = rgba[i * 4 + 2];
        if r < 60 && g < 60 && b < 60 {
            dark += 1;
        } else if r < 240 || g < 240 || b < 240 {
            colored += 1;
        }
    }
    println!();
    println!("=== PNG pixel diagnosis ===");
    println!("total px : {n}");
    println!("dark (<60): {dark}  ({:.2}%)", 100.0 * dark as f32 / n as f32);
    println!("colored  : {colored}  ({:.2}%)", 100.0 * colored as f32 / n as f32);
    println!("white-ish: {}  ({:.2}%)", n as u32 - dark - colored, 100.0 * (n as u32 - dark - colored) as f32 / n as f32);
}
