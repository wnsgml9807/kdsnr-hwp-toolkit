//! `e2e_bench_all` — work/e2e/ 전체 hwpx/png 페어 일괄 측정.
//!
//! e2e_bench 의 batch 버전. work/e2e/<subdir>/<stem>.hwpx + <stem>.png 페어를
//! 모두 찾아 page 0 비교 → CSV 출력 + 평균 score 집계.
//!
//! ## 사용
//!
//! ```bash
//! cargo run --release --example e2e_bench_all -- [--root work/e2e] [--out work/e2e/_bench_output] [--hft DIR]
//! ```
//!
//! 출력:
//! - stdout: CSV `subdir,stem,gt_w,gt_h,score_pct,mismatch,avg_delta,svg_bytes`
//! - stderr: 진행 로그
//! - `<out>/<stem>_page00_{ours,heatmap}.png` per pair
//!
//! ## 정공법 정책
//!
//! 본 batch 는 "rhwp baseline" 평균 score 산정용. 우리 SvgSurface adapter 가
//! e2e wire 되면 같은 binary 의 다른 backend 로 동일 측정 반복 → 개선 폭 검증.

use kdsnr_render::pixel_diff_harness::{make_heatmap_rgba, score_pages, DiffOptions};
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!("usage: e2e_bench_all [--root DIR] [--out DIR] [--hft DIR]");
    std::process::exit(2);
}

struct Args {
    root: PathBuf,
    out_dir: PathBuf,
    hft_dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().collect();
    let mut root = PathBuf::from("work/e2e");
    let mut out_dir = PathBuf::from("work/e2e/_bench_output");
    let mut hft_dir: Option<PathBuf> = None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--root" => {
                root = PathBuf::from(argv.get(i + 1).unwrap_or_else(|| usage()));
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
        root,
        out_dir,
        hft_dir,
    }
}

fn read_png_rgba(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
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
        png::ColorType::Grayscale => {
            let n = (w * h) as usize;
            let mut out = Vec::with_capacity(n * 4);
            for i in 0..n {
                let g = buf[i];
                out.extend_from_slice(&[g, g, g, 0xff]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let n = (w * h) as usize;
            let mut out = Vec::with_capacity(n * 4);
            for i in 0..n {
                let g = buf[i * 2];
                out.extend_from_slice(&[g, g, g, buf[i * 2 + 1]]);
            }
            out
        }
        _ => return None,
    };
    Some((rgba, w, h))
}

fn write_png_rgba(path: &Path, rgba: &[u8], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let Ok(mut writer) = encoder.write_header() else { return };
    let _ = writer.write_image_data(rgba);
}

fn build_usvg_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        db.load_system_fonts();
        // 한컴 자체 폰트 (HYhwpEQ, HancomEQN, symbol, HANSymbol 등) 는 macOS 시스템
        // font path 에 등록되지 않고 Hancom 앱 내부에 번들됨. 수식 PUA glyph (U+E0xx)
        // 를 정상 렌더하려면 이 디렉토리도 explicit load 해야 한다.
        for app_dir in [
            "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install",
            "/Applications/Hancom Office Hangul.app/Contents/Resources/Hnc/Shared/TTF/Install",
        ] {
            if std::path::Path::new(app_dir).exists() {
                db.load_fonts_dir(app_dir);
            }
        }
    }
    opt
}

/// SVG → RGBA pixmap. target_w/target_h 가 None 이면 SVG native size 그대로 raster.
/// 지정되면 isotropic scale + letterbox (anisotropic stretch 금지).
fn rasterize_svg(svg: &str, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    let opt = build_usvg_options();
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let svg_size = tree.size();
    let scale = (target_w as f32 / svg_size.width())
        .min(target_h as f32 / svg_size.height());
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    let mut pixmap = tiny_skia::Pixmap::new(target_w, target_h)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.take())
}

/// SVG 의 native page 크기로 raster (페이지 비율 보존). 시각 검증용 ours.png 저장에
/// 사용한다. ScreenDPI 보정으로 GT PDF(보통 300 dpi) 와 비슷한 해상도를 확보.
fn rasterize_svg_native(svg: &str) -> Option<(Vec<u8>, u32, u32)> {
    let opt = build_usvg_options();
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let svg_size = tree.size();
    // SVG 좌표는 user-unit (CSS px @ 96dpi 기준). 300 dpi 등가 해상도로 출력하려면
    // 300/96 = 3.125 배. 다만 페이지가 너무 커서 메모리 부담 우려 시 3.0 으로 고정.
    let scale: f32 = 300.0 / 96.0;
    let w = (svg_size.width() * scale).ceil() as u32;
    let h = (svg_size.height() * scale).ceil() as u32;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some((pixmap.take(), w, h))
}

#[derive(Debug, Clone)]
struct PairResult {
    subdir: String,
    stem: String,
    gt_w: u32,
    gt_h: u32,
    score_pct: f32,
    mismatched: u64,
    avg_delta: f32,
    svg_bytes: usize,
    err: Option<String>,
}

fn collect_pairs(root: &Path) -> Vec<(PathBuf, PathBuf, String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return out };
    let mut subdirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| !s.starts_with('_') && !s.starts_with('.'))
                .unwrap_or(false)
        })
        .collect();
    subdirs.sort();
    for sub in subdirs {
        let subname = sub
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let Ok(rd) = std::fs::read_dir(&sub) else { continue };
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        files.sort();
        for f in &files {
            if f.extension().and_then(|s| s.to_str()) != Some("hwpx") {
                continue;
            }
            let Some(stem) = f.file_stem().and_then(|s| s.to_str()) else { continue };
            let png = sub.join(format!("{stem}.png"));
            if !png.exists() {
                continue;
            }
            out.push((f.clone(), png, subname.clone(), stem.to_string()));
        }
    }
    out
}

fn process_pair(hwpx: &Path, gt_png: &Path, out_dir: &Path, subdir: &str, stem: &str) -> PairResult {
    let mut res = PairResult {
        subdir: subdir.to_string(),
        stem: stem.to_string(),
        gt_w: 0,
        gt_h: 0,
        score_pct: 0.0,
        mismatched: 0,
        avg_delta: 0.0,
        svg_bytes: 0,
        err: None,
    };
    let Some((gt_rgba, gt_w, gt_h)) = read_png_rgba(gt_png) else {
        res.err = Some("gt png decode fail".into());
        return res;
    };
    res.gt_w = gt_w;
    res.gt_h = gt_h;

    let data = match std::fs::read(hwpx) {
        Ok(d) => d,
        Err(e) => {
            res.err = Some(format!("hwpx read: {e}"));
            return res;
        }
    };
    let doc = match rhwp::wasm_api::HwpDocument::from_bytes(&data) {
        Ok(d) => d,
        Err(e) => {
            res.err = Some(format!("rhwp parse: {e}"));
            return res;
        }
    };
    let svg = match doc.render_page_svg_native(0) {
        Ok(s) => s,
        Err(e) => {
            res.err = Some(format!("rhwp render: {e}"));
            return res;
        }
    };
    res.svg_bytes = svg.len();

    let Some(our_rgba) = rasterize_svg(&svg, gt_w, gt_h) else {
        res.err = Some("rasterize fail".into());
        return res;
    };

    let opts = DiffOptions::default();
    let score = match score_pages(&our_rgba, &gt_rgba, gt_w, gt_h, &opts) {
        Ok(s) => s,
        Err(e) => {
            res.err = Some(format!("score: {e}"));
            return res;
        }
    };
    res.score_pct = score.score_pct;
    res.mismatched = score.mismatched_pixels;
    res.avg_delta = score.avg_delta;

    // PNG + heatmap
    let prefix = format!("{subdir}__{stem}");
    // ours.png 는 SVG native 페이지 전체 크기로 저장 (페이지 비율 일관). 시각 검증용.
    if let Some((native_rgba, nw, nh)) = rasterize_svg_native(&svg) {
        write_png_rgba(
            &out_dir.join(format!("{prefix}_ours.png")),
            &native_rgba,
            nw,
            nh,
        );
    } else {
        // fallback: GT size 로 letterbox 된 raster 사용
        write_png_rgba(
            &out_dir.join(format!("{prefix}_ours.png")),
            &our_rgba,
            gt_w,
            gt_h,
        );
    }
    // heatmap 은 GT 와 같은 좌표계에서만 의미. GT size 유지.
    if let Ok(heat) = make_heatmap_rgba(&our_rgba, &gt_rgba, gt_w, gt_h, &opts) {
        write_png_rgba(
            &out_dir.join(format!("{prefix}_heatmap.png")),
            &heat,
            gt_w,
            gt_h,
        );
    }
    res
}

fn main() {
    let args = parse_args();
    std::fs::create_dir_all(&args.out_dir).ok();

    if let Some(hft_dir) = &args.hft_dir {
        let mut cache = rhwp::kdsnr_hft::HftCache::new();
        match cache.load_dir(hft_dir) {
            Ok(n) => {
                eprintln!("HFT load: {n} glyphs ({} families)", cache.family_count());
                let arc = std::sync::Arc::new(cache);
                rhwp::renderer::font_runtime_metrics::set_global_hft_cache(arc);
            }
            Err(e) => eprintln!("HFT load FAIL: {e}"),
        }
    }

    let pairs = collect_pairs(&args.root);
    eprintln!("found {} pair(s) under {}", pairs.len(), args.root.display());

    // CSV header
    println!("subdir,stem,gt_w,gt_h,score_pct,mismatched,avg_delta,svg_bytes,err");

    let mut results = Vec::with_capacity(pairs.len());
    for (i, (hwpx, png, subdir, stem)) in pairs.iter().enumerate() {
        eprintln!(
            "[{}/{}] {} / {}",
            i + 1,
            pairs.len(),
            subdir,
            stem
        );
        let res = process_pair(hwpx, png, &args.out_dir, subdir, stem);
        println!(
            "{},{},{},{},{:.2},{},{:.2},{},{}",
            csv_escape(&res.subdir),
            csv_escape(&res.stem),
            res.gt_w,
            res.gt_h,
            res.score_pct,
            res.mismatched,
            res.avg_delta,
            res.svg_bytes,
            res.err.as_deref().unwrap_or("")
        );
        results.push(res);
    }

    // 집계
    let valid: Vec<&PairResult> = results.iter().filter(|r| r.err.is_none()).collect();
    let avg = if valid.is_empty() {
        0.0
    } else {
        valid.iter().map(|r| r.score_pct).sum::<f32>() / valid.len() as f32
    };
    let worst = valid
        .iter()
        .min_by(|a, b| a.score_pct.partial_cmp(&b.score_pct).unwrap())
        .map(|r| (r.subdir.clone(), r.stem.clone(), r.score_pct));
    let best = valid
        .iter()
        .max_by(|a, b| a.score_pct.partial_cmp(&b.score_pct).unwrap())
        .map(|r| (r.subdir.clone(), r.stem.clone(), r.score_pct));

    eprintln!();
    eprintln!("=== aggregate ===");
    eprintln!("  pairs       : {} (valid {} / err {})", results.len(), valid.len(), results.len() - valid.len());
    eprintln!("  avg score   : {avg:.2}%");
    if let Some((sd, st, s)) = worst {
        eprintln!("  worst       : {sd}/{st}  {s:.2}%");
    }
    if let Some((sd, st, s)) = best {
        eprintln!("  best        : {sd}/{st}  {s:.2}%");
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let esc = s.replace('"', "\"\"");
        format!("\"{esc}\"")
    } else {
        s.to_string()
    }
}
