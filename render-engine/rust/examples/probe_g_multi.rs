//! `probe_g_multi` — section 의 모든 paragraph × 모든 line_seg 렌더.
//!
//! probe_g_real (single paragraph) 의 확장: rhwp 의 stored line_segs 를
//! "어느 글자가 어느 줄, 어느 vertical_pos 에 있는가" 의 source of truth 로 사용하고,
//! kdsnr-layout 의 compose_layout + allocate 로 per-glyph cursor_x 계산하여
//! SvgSurface 로 emit.
//!
//! 이 단계의 byte-eq 보장 X — paginator 의 vertical_pos / column_start 는
//! rhwp 결과 그대로 빌려옴. 글자 위치 (horizontal) 만 kdsnr-layout 으로 계산.
//! G phase 의 "글자 위치 byte-eq" milestone 로 가는 중간 단계.

use kdsnr_layout::{
    glyph::{CharItemView, CharItemViewConstructorMetrics, ComposeResult, Glyph},
    value_types::{Allocation, Allotment, BreakType, Extension},
    compose_layout::Break,
    ppt_compose_layout,
    runtime::RunProperty,
    properties::{HashMapPropertyBag, PropertyValue, keys},
};
use kdsnr_render::{
    brush::{Brush, SolidBrush},
    color::Color,
    surface::{Font, Surface, PointImpl, Transform2D},
    svg_surface::SvgSurface,
};
use std::sync::Arc;
use std::fmt::Write;

fn hft_cache() -> Arc<kdsnr_hft::HftCache> {
    let mut cache = kdsnr_hft::HftCache::new();
    let _n = kdsnr_hft::embedded::load_into(&mut cache).expect("HFT load");
    // 사용자 alias 확장 (hftinfo.dat 미포함 한컴 후속 폰트들)
    use kdsnr_hft::alias::FaceCategory;
    // 함초롬바탕 → 한양신명조 (둘 다 명조 계열, 한컴 office 미포함 V6+ 폰트 매핑)
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hangul);
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hanja);
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Symbol);
    cache.add_alias("HamChoRomBatang", "HGSMJ", FaceCategory::Hangul);
    // 함초롬돋움 → 한양견고딕
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hangul);
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hanja);
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Symbol);
    cache.add_alias("HamChoRomDotum", "HGGGT", FaceCategory::Hangul);
    Arc::new(cache)
}

// ── Glyph wrappers ────────────────
#[derive(Debug, Clone)]
struct CivItem(CharItemView);

impl Glyph for CivItem {
    fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn compose(&self, _bt: BreakType) -> ComposeResult {
        ComposeResult {
            replacement: Some(Box::new(self.0.clone())),
            can_break: false,
        }
    }
}

#[derive(Debug)]
struct MutComp { children: Vec<Box<dyn Glyph>> }

impl Glyph for MutComp {
    fn clone_glyph(&self) -> Box<dyn Glyph> { unimplemented!() }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn get_count(&self) -> usize { self.children.len() }
    fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
        self.children.get(idx).map(|b| b.as_ref())
    }
}

#[derive(Debug, Default)]
struct AppendRecorder { items: Vec<Option<Box<dyn Glyph>>> }

impl Glyph for AppendRecorder {
    fn clone_glyph(&self) -> Box<dyn Glyph> { unimplemented!() }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn append(&mut self, child: Option<Box<dyn Glyph>>) {
        self.items.push(child);
    }
}

// ── helpers ────────────────
fn make_run_property(font_size_pt: f32) -> RunProperty {
    let bag = HashMapPropertyBag::new()
        .with(keys::FONT_SIZE, PropertyValue::Float(font_size_pt));
    RunProperty::from_bag(bag)
}

fn char_width_pt(cache: &kdsnr_hft::HftCache, face: &str, cp: u32, font_size_pt: f32) -> f32 {
    if let Some(g) = cache.get(face, cp) {
        if g.em > 0 && g.advance > 0 {
            let ratio = g.advance as f32 / g.em as f32;
            if ratio > 0.3 && ratio < 2.0 {
                return ratio * font_size_pt;
            }
        }
    }
    if cp == 0x20 || cp == 0xa0 || cp == 0x09 { font_size_pt * 0.27 }
    else if (0x30..=0x39).contains(&cp) { font_size_pt * 0.50 }
    else if cp < 0x80 { font_size_pt * 0.40 }
    else { font_size_pt * 1.0 }
}

fn build_civ(char_code: u16, font_size_pt: f32, width_pt: f32) -> CharItemView {
    let ascent_pt = font_size_pt * 0.8;
    let descent_pt = font_size_pt * 0.2;
    let run_property = Some(make_run_property(font_size_pt));
    let metrics = CharItemViewConstructorMetrics {
        metric_3c: ascent_pt + descent_pt,
        ascent: ascent_pt,
        descent: descent_pt,
        metric_4c: width_pt,
        metric_50: 0.0,
    };
    CharItemView::from_constructor_metrics(
        char_code, run_property, None, None, font_size_pt, metrics,
    )
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let hwpx_path = argv.get(1).cloned().unwrap_or_else(||
        "../../templet/original/math_input_sample_2.hwpx".to_string()
    );

    let data = std::fs::read(&hwpx_path).expect("read hwpx");
    let doc = rhwp::parser::hwpx::parse_hwpx(&data).expect("parse_hwpx");
    let section = &doc.sections[0];
    println!("HWPX parsed: {} paragraphs in section[0]", section.paragraphs.len());

    let cache = hft_cache();

    // canvas (272 × 394 mm @ 96 DPI = 1028 × 1489 px)
    let dpi = 96.0_f32;
    let pt_to_px = dpi / 72.0;
    let canvas_w_px = 1028.0_f32;
    let canvas_h_px = 1489.0_f32;
    let mut surface = SvgSurface::new(canvas_w_px, canvas_h_px)
        .with_hft_cache(cache.clone());

    // 페이지 margin (단순): 좌 100 HWPUNIT = 1pt ... 한컴 default 는 보통 1500 HWPUNIT (15mm)
    // 정확 산출은 page_def.margin_left 가 답이지만 본 probe 는 임시 50pt margin.
    let margin_left_pt = 50.0_f32;
    let margin_top_pt = 50.0_f32;

    // fallback chain: 우선 face 그대로 (함초롬바탕, etc.) → HCR Batang (TTF family) → HBatang (한컴 office)
    // → serif. fontdb 가 첫 매칭 사용.
    let fallback_family_chain = "'함초롬바탕', 'HCR Batang', 'HBatang', serif";
    let mut stats_para = 0usize;
    let mut stats_line = 0usize;
    let mut stats_emit_hft = 0usize;
    let mut stats_emit_text = 0usize;

    // 사용된 face 명 + cache hit 통계 수집
    let mut face_stats: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();

    for (pidx, para) in section.paragraphs.iter().enumerate() {
        if para.text.is_empty() || para.line_segs.is_empty() {
            continue;
        }
        stats_para += 1;
        let chars: Vec<u16> = para.text.encode_utf16().collect();
        let n_chars = chars.len() as u32;

        // 첫 char_shape (단순화: 한 paragraph 안 모든 글자 같은 shape 가정)
        let first_cs_id = para.char_shapes.first().map(|cs| cs.char_shape_id).unwrap_or(0);
        let cs = match doc.doc_info.char_shapes.get(first_cs_id as usize) {
            Some(cs) => cs,
            None => continue,
        };
        let font_size_pt = cs.base_size as f32 / 100.0;

        let font_id = cs.font_ids[0];
        let hangul_face = doc.doc_info.font_faces.get(0)
            .and_then(|faces| faces.get(font_id as usize))
            .map(|f| f.name.clone())
            .unwrap_or_default();
        let hft_face_ok = cache.has_font(&hangul_face) || cache.has_alias(&hangul_face);
        let entry = face_stats.entry(hangul_face.clone()).or_insert((0, 0));
        if hft_face_ok { entry.0 += 1; } else { entry.1 += 1; }

        for (li, lseg) in para.line_segs.iter().enumerate() {
            stats_line += 1;
            // line 의 글자 범위 [text_start .. next_text_start | n_chars)
            let next_start = para.line_segs.get(li + 1)
                .map(|ls| ls.text_start)
                .unwrap_or(n_chars);
            let start = lseg.text_start as usize;
            let end = (next_start as usize).min(chars.len());
            if start >= end { continue; }
            let line_chars = &chars[start..end];

            // CharItemView seq build
            let mut civ_list: Vec<Box<dyn Glyph>> = Vec::new();
            for &c in line_chars {
                let w = char_width_pt(&cache, &hangul_face, c as u32, font_size_pt);
                let view = build_civ(c, font_size_pt, w);
                civ_list.push(Box::new(CivItem(view)));
            }
            let comp = MutComp { children: civ_list };
            let mut output = AppendRecorder::default();
            ppt_compose_layout(
                &comp, 1, Break::new(0, (line_chars.len() as i32) - 1),
                -1, 0, &mut output,
            );

            // 글자 별 emit (cursor_x_pt 누적)
            let baseline_y_pt = lseg.vertical_pos as f32 / 100.0
                + lseg.baseline_distance as f32 / 100.0;
            let column_start_pt = lseg.column_start as f32 / 100.0;
            let baseline_x_pt = margin_left_pt + column_start_pt;
            let baseline_y_px = (margin_top_pt + baseline_y_pt) * pt_to_px;

            let font = Font {
                family: hangul_face.clone(),
                size: font_size_pt * pt_to_px,
                bold: false,
                italic: false,
            };
            let brush = Brush::Solid(SolidBrush::new(
                Color::from_rgb(0, 0, 0, std::ptr::null_mut())
            ));

            let mut cursor_x_pt = 0f32;
            for item in &output.items {
                let Some(g) = item else { continue };
                let Some(civ) = g.as_any().downcast_ref::<CharItemView>() else { continue };
                let cp32 = civ.char_code as u32;
                if matches!(civ.char_code, 0x0d | 0x0a) {
                    cursor_x_pt += civ.width;
                    continue;
                }
                if civ.char_code == 0x09 {
                    cursor_x_pt += civ.width;
                    continue;
                }
                let x_px = (baseline_x_pt + cursor_x_pt) * pt_to_px;
                if hft_face_ok && cache.get(&hangul_face, cp32).map(|g| !g.d.is_empty()).unwrap_or(false) {
                    let codepoints = vec![civ.char_code];
                    let positions = vec![PointImpl { x: x_px, y: baseline_y_px }];
                    surface.draw_driver_string(&codepoints, &font, &brush, &positions, &Transform2D::IDENTITY);
                    stats_emit_hft += 1;
                } else {
                    let c = char::from_u32(cp32).unwrap_or(' ');
                    let escaped = match c {
                        '<' => "&lt;".to_string(),
                        '>' => "&gt;".to_string(),
                        '&' => "&amp;".to_string(),
                        _ => c.to_string(),
                    };
                    writeln!(
                        &mut surface.buffer,
                        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{:.2}\" fill=\"#000000\" xml:space=\"preserve\">{}</text>",
                        x_px, baseline_y_px, fallback_family_chain, font_size_pt * pt_to_px, escaped
                    ).unwrap();
                    stats_emit_text += 1;
                }
                cursor_x_pt += civ.width;
            }

            // 첫 5 paragraph 의 첫 line 만 dump
            if pidx < 5 && li == 0 {
                let preview: String = para.text.chars().take(30).collect();
                println!("  para[{}] line[{}] vpos={}pt cs={} chars={}..{} \"{}\"",
                    pidx, li, baseline_y_pt, first_cs_id, start, end, preview);
            }
        }
    }

    println!();
    println!("stats:");
    println!("  paragraphs rendered: {}", stats_para);
    println!("  line_segs rendered: {}", stats_line);
    println!("  HFT path emits: {}", stats_emit_hft);
    println!("  SVG <text> emits: {}", stats_emit_text);

    println!();
    println!("face stats (paragraph count by face, hft_hit / miss):");
    let mut face_vec: Vec<_> = face_stats.iter().collect();
    face_vec.sort_by(|a, b| (b.1.0 + b.1.1).cmp(&(a.1.0 + a.1.1)));
    for (face, (hit, miss)) in &face_vec {
        println!("  {:?}  hit={}  miss={}", face, hit, miss);
    }

    let svg = surface.finish();
    let out_path = std::path::PathBuf::from("../../work/debug/probe_g_multi.svg");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out_path, &svg).expect("write svg");
    println!();
    println!("SVG 출력: {} ({} bytes)", out_path.display(), svg.len());

    // PNG rasterize
    let png_path = out_path.with_extension("png");
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let hwp_ttf_dir = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install";
    if let Ok(rd) = std::fs::read_dir(hwp_ttf_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("ttf")) == Some(true) {
                let _ = opt.fontdb_mut().load_font_file(&path);
            }
        }
    }
    let hyhwpeq_path = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install/HYHWPEQ.TTF";
    if std::path::Path::new(hyhwpeq_path).exists() {
        opt.fontdb_mut().load_font_file(hyhwpeq_path).ok();
    }
    // 한컴 별도 download 폰트 (함초롬바탕/돋움) — work/fonts/hancom/
    let hancom_fonts_dir = "../../work/fonts/hancom";
    if let Ok(rd) = std::fs::read_dir(hancom_fonts_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("ttf")) == Some(true) {
                opt.fontdb_mut().load_font_file(&path).ok();
            }
        }
    }
    match usvg::Tree::from_str(&svg, &opt) {
        Ok(tree) => {
            let sz = tree.size();
            let sx = canvas_w_px / sz.width();
            let sy = canvas_h_px / sz.height();
            let t = tiny_skia::Transform::from_scale(sx, sy);
            if let Some(mut pix) = tiny_skia::Pixmap::new(canvas_w_px as u32, canvas_h_px as u32) {
                pix.fill(tiny_skia::Color::WHITE);
                resvg::render(&tree, t, &mut pix.as_mut());
                pix.save_png(&png_path).expect("save png");
                println!("PNG 출력: {}", png_path.display());
            }
        }
        Err(e) => println!("usvg parse err: {}", e),
    }
}
