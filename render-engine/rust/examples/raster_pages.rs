//! `raster_pages` — hwpx 파일의 모든 페이지를 PNG 로 rasterize 한다.
//!
//! e2e_bench_all 의 rasterize_svg_native 와 동일한 isotropic 300 DPI 출력.
//!
//! ## 사용
//! ```bash
//! cargo run --release --example raster_pages -- <hwpx_path> <out_dir>
//! ```

use std::path::PathBuf;

fn build_usvg_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    let db = opt.fontdb_mut();
    // 2026-05-18 (root cause fix): simplecss/usvg 가 SVG @font-face 의 data URI 를
    // 처리 안 함 → SVG 임베드 base64 무용 → resvg 는 미리 로딩된 fontdb 매칭만 사용.
    // 시스템 한컴 Office /Install 의 HANBatang.ttf (family="HCR Batang") 가 fontdb 에
    // 등록되면 우리 assets/HCRBatang.ttf 와 동일 family 충돌 → 시스템 face 가 매칭됨
    // (다른 outline). 따라서 시스템 한컴 Office TTF 폴더 전체 load 제거 + 필수 face
    // (HYHWPEQ.TTF 수식) 만 명시 추가.
    //
    // LOAD ORDER (fontdb first-match 우선 가정):
    //   1. toolkit assets/fonts (HCR Batang/Dotum + Bold) — 우리 의도한 face
    //   2. toolkit vendor/rhwp/ttfs/hancom/All + Hwp (HBATANG, HDOTUM, H2HDRM 등)
    //   3. 시스템 한컴 Office 의 HYHWPEQ.TTF 만 (수식 — vendor 부재)
    //   4. (optional) load_system_fonts — 영문 시스템 폴백
    // assets/fonts 첫번째 → fontdb 에 "HCR Batang" 등록 → 후속 같은 family 등록 안 우선.

    // 1. assets/fonts 첫번째
    if let Ok(exe) = std::env::current_exe() {
        for n in 0..10 {
            if let Some(anc) = exe.ancestors().nth(n) {
                let assets = anc.join("assets/fonts");
                if assets.is_dir() {
                    eprintln!("[1] HCR fonts loaded from: {}", assets.display());
                    db.load_fonts_dir(&assets);
                    break;
                }
            }
        }
    }
    // 2. vendor (HBATANG, HDOTUM, H2HDRM, HYPORM, HMKMM 등)
    if let Ok(exe) = std::env::current_exe() {
        for n in 0..10 {
            if let Some(anc) = exe.ancestors().nth(n) {
                let mut loaded_any = false;
                for sub in &[
                    "vendor/rhwp/ttfs/hancom/All",
                    "vendor/rhwp/ttfs/hancom/Hwp",
                    "vendor/rhwp/ttfs/hancom/flat",
                ] {
                    let p = anc.join(sub);
                    if p.is_dir() {
                        eprintln!("[2] toolkit fonts loaded from: {}", p.display());
                        db.load_fonts_dir(&p);
                        loaded_any = true;
                    }
                }
                if loaded_any { break; }
            }
        }
    }
    // 3. system Hancom Office 의 HYHWPEQ.TTF 만 (vendor 부재). 다른 HANBatang.ttf 등은 제외.
    for hyhwpeq in [
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install/HYHWPEQ.TTF",
        "/Applications/Hancom Office Hangul.app/Contents/Resources/Hnc/Shared/TTF/Install/HYHWPEQ.TTF",
    ] {
        if std::path::Path::new(hyhwpeq).exists() {
            eprintln!("[3] HYHWPEQ loaded from: {}", hyhwpeq);
            let _ = db.load_font_file(hyhwpeq);
            break;
        }
    }
    // 4. system fonts (Apple 시스템 영문 + 폴백). RHWP_FONT_ISOLATE 면 skip.
    if std::env::var("RHWP_FONT_ISOLATE").is_err() {
        db.load_system_fonts();
        eprintln!("[4] system fonts loaded");
    } else {
        eprintln!("FONT ISOLATE mode: system fonts NOT loaded");
    }
    // Also allow absolute path from env (fallback).
    if let Ok(extra) = std::env::var("RHWP_EXTRA_FONT_DIR") {
        if std::path::Path::new(&extra).exists() {
            db.load_fonts_dir(&extra);
        }
    }
    // Diagnostic: confirm HCR Batang is queryable by family name.
    {
        use usvg::fontdb::{Query, Family, Style, Weight, Stretch};
        let q = Query {
            families: &[Family::Name("HCR Batang")],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        };
        match db.query(&q) {
            Some(id) => {
                let face = db.face(id).unwrap();
                eprintln!("FONTDB Query('HCR Batang' Normal) -> matched: family={:?} source={:?}",
                    face.families.first().map(|(s,_)| s.as_str()).unwrap_or("?"),
                    face.source);
            }
            None => eprintln!("FONTDB Query('HCR Batang' Normal) -> NO MATCH!"),
        }
        // Also try Postscript name "HCRBatang"
        let q2 = Query {
            families: &[Family::Name("HCRBatang")],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        };
        match db.query(&q2) {
            Some(id) => eprintln!("FONTDB Query('HCRBatang' no space) -> matched id={:?}", id),
            None => eprintln!("FONTDB Query('HCRBatang' no space) -> NO MATCH"),
        }
        // List all faces whose family contains "Batang"
        let mut batang_faces = 0;
        for face in db.faces() {
            for (family, _) in &face.families {
                if family.contains("Batang") || family.contains("바탕") {
                    eprintln!("FONTDB face: family={:?} weight={:?} style={:?} source={:?}",
                        family, face.weight, face.style, face.source);
                    batang_faces += 1;
                    break;
                }
            }
        }
        eprintln!("FONTDB total Batang faces: {}", batang_faces);
    }
    opt
}

fn rasterize_svg_native(svg: &str) -> Option<(Vec<u8>, u32, u32)> {
    let opt = build_usvg_options();
    let tree = usvg::Tree::from_str(svg, &opt).ok()?;
    let svg_size = tree.size();
    let scale: f32 = 300.0 / 96.0;
    let w = (svg_size.width() * scale) as u32;
    let h = (svg_size.height() * scale) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some((pixmap.data().to_vec(), w, h))
}

fn write_png_rgba(path: &std::path::Path, rgba: &[u8], w: u32, h: u32) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: raster_pages <hwpx_path> <out_dir>");
        std::process::exit(2);
    }
    let hwpx = PathBuf::from(&args[1]);
    let out_dir = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out_dir)?;

    // Load Hancom HFT cache so sinmyung-series face names emit real Hancom
    // vector path instead of Haansoft Batang TTF substitute.
    let hft_dir = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts";
    let hftinfo = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts/hftinfo.dat";
    let hft_arc = if std::path::Path::new(hft_dir).exists() {
        let mut cache = rhwp::kdsnr_hft::HftCache::new();
        let _ = cache.load_aliases(hftinfo);
        match cache.load_dir(hft_dir) {
            Ok(n) => eprintln!("HFT cache: {n} glyphs, {} families, {} aliases",
                cache.family_count(), cache.alias_count()),
            Err(e) => eprintln!("HFT load fail: {e}"),
        }
        let arc = std::sync::Arc::new(cache);
        rhwp::renderer::font_runtime_metrics::set_global_hft_cache(arc.clone());
        Some(arc)
    } else { None };

    let data = std::fs::read(&hwpx)?;
    let mut doc = rhwp::wasm_api::HwpDocument::from_bytes(&data)
        .map_err(|e| format!("rhwp parse: {e}"))?;
    if let Some(arc) = hft_arc { doc.set_hft_cache(arc); }
    let n_pages = doc.page_count();
    eprintln!("pages: {}", n_pages);

    // Build font paths for @font-face subset embedding. SVG output will contain
    // base64-encoded subset of HCR Batang / HCR Dotum / Hancom HFT TTFs so the
    // rasterizer (resvg/tiny-skia) and any client (Chrome/Edge/Safari) use the
    // exact same glyph outlines — independent of system font installation.
    let mut font_paths: Vec<std::path::PathBuf> = Vec::new();
    // 1. toolkit 내부 vendor/rhwp/ttfs (한컴 Office 폰트들 — 의존 없음)
    if let Ok(exe) = std::env::current_exe() {
        for n in 0..10 {
            if let Some(anc) = exe.ancestors().nth(n) {
                let mut added = false;
                for sub in &[
                    "vendor/rhwp/ttfs/hancom/All",
                    "vendor/rhwp/ttfs/hancom/Hwp",   // HYPORM.TTF, HMKMM.TTF, EN* 영어계
                    "vendor/rhwp/ttfs/hancom/flat",
                ] {
                    let p = anc.join(sub);
                    if p.is_dir() {
                        font_paths.push(p);
                        added = true;
                    }
                }
                if added { break; }
            }
        }
    }
    // 2. toolkit assets/fonts (HCR Batang/Dotum)
    if let Ok(exe) = std::env::current_exe() {
        for n in 0..10 {
            if let Some(anc) = exe.ancestors().nth(n) {
                let assets = anc.join("assets/fonts");
                if assets.is_dir() {
                    font_paths.push(assets);
                    break;
                }
            }
        }
    }
    // 3. 시스템 한컴 Office (있으면 보조; vendor 우선이라 매칭 거의 안 됨)
    for d in &[
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Hwp",
        "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/All",
    ] {
        if std::path::Path::new(d).exists() {
            font_paths.push(std::path::PathBuf::from(d));
        }
    }
    eprintln!("SVG font embed paths: {} dirs", font_paths.len());

    for i in 0..n_pages {
        let svg = doc.render_page_svg_with_fonts(
            i,
            rhwp::renderer::svg::FontEmbedMode::Subset,
            &font_paths,
        ).map_err(|e| format!("page {i} svg: {e}"))?;
        let (rgba, w, h) = rasterize_svg_native(&svg)
            .ok_or("rasterize fail")?;
        let png_path = out_dir.join(format!("page_{:02}.png", i + 1));
        let svg_path = out_dir.join(format!("page_{:02}.svg", i + 1));
        write_png_rgba(&png_path, &rgba, w, h)?;
        std::fs::write(&svg_path, svg)?;
        let (hit, miss) = rhwp::renderer::kdsnr_hft_global::stats();
        eprintln!("  → {} ({}x{})  HFT hit={hit} miss={miss}", png_path.display(), w, h);
    }
    rhwp::renderer::kdsnr_hft_global::dump_miss_report();
    Ok(())
}
