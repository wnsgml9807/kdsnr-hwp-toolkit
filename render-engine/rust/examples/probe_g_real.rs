//! `probe_g_real` — G phase 의 실제 HWPX 1 paragraph wire smoke.
//!
//! 흐름:
//!   1. rhwp::parser::hwpx::parse_hwpx 로 Document 얻음
//!   2. sections[0].paragraphs 중 첫 non-empty 선택
//!   3. 첫 char_shape 의 base_size → font_size_pt
//!   4. 글자별 HFT advance lookup (한글), ASCII 추정
//!   5. CharItemView seq build → ppt_compose_layout 호출
//!   6. output 의 CharItemView width sum vs stored line_segs[0].segment_width 비교
//!
//! 이번 단계의 byte-eq 보장 X — 단순화 (모든 글자 같은 RunProperty,
//! font_size 만, ascent/descent 추정, lang category 미고려). flow 자체 작동 + dump.

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

// HFT cache 는 embedded
fn hft_cache() -> Arc<kdsnr_hft::HftCache> {
    let mut cache = kdsnr_hft::HftCache::new();
    let _n = kdsnr_hft::embedded::load_into(&mut cache).expect("HFT load");
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

/// 한 글자의 width (pt 단위) 계산.
/// - HFT hit (한글) → advance/em * font_size_pt
/// - ASCII / miss → 추정 (space 0.27em, digit 0.5em, ASCII 0.4em, 그 외 0.5em)
fn char_width_pt(cache: &kdsnr_hft::HftCache, face: &str, cp: u32, font_size_pt: f32) -> f32 {
    if let Some(g) = cache.get(face, cp) {
        if g.em > 0 && g.advance > 0 {
            let ratio = g.advance as f32 / g.em as f32;
            // HFT 의 ASCII advance 가 비정상 (0.022-0.061em) — 한글 advance 만 신뢰
            if ratio > 0.3 && ratio < 2.0 {
                return ratio * font_size_pt;
            }
        }
    }
    // 추정 fallback
    if cp == 0x20 || cp == 0xa0 || cp == 0x09 { font_size_pt * 0.27 }
    else if (0x30..=0x39).contains(&cp) { font_size_pt * 0.50 }
    else if cp < 0x80 { font_size_pt * 0.40 }
    else { font_size_pt * 1.0 }  // 한글/기타
}

fn build_char_item_view_pt(char_code: u16, font_size_pt: f32, width_pt: f32) -> CharItemView {
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
        char_code,
        run_property,
        None,
        None,
        font_size_pt,
        metrics,
    )
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let hwpx_path = argv.get(1).cloned().unwrap_or_else(||
        "../../templet/original/math_input_sample_2.hwpx".to_string()
    );

    let data = std::fs::read(&hwpx_path).expect("read hwpx");
    let doc = rhwp::parser::hwpx::parse_hwpx(&data).expect("parse_hwpx");
    println!("HWPX parsed: {} sections, {} char_shapes",
        doc.sections.len(),
        doc.doc_info.char_shapes.len());

    let section = &doc.sections[0];

    // 첫 non-empty paragraph 선택
    let (pidx, para) = section.paragraphs.iter().enumerate()
        .find(|(_, p)| !p.text.is_empty() && !p.line_segs.is_empty())
        .expect("non-empty paragraph not found");

    let chars: Vec<u16> = para.text.encode_utf16().collect();
    println!("\n=== paragraph [{}] ({} chars, {} line_segs) ===", pidx, chars.len(), para.line_segs.len());
    let preview: String = para.text.chars().take(40).collect();
    println!("  preview: {:?}", preview);

    // ParaShape 의 line_spacing 출력 — line_seg.line_height 1350 출처 검증
    let ps_id = para.para_shape_id as usize;
    if let Some(ps) = doc.doc_info.para_shapes.get(ps_id) {
        println!("  ParaShape[{}]: line_spacing={} type={:?} line_spacing_v2={}",
            ps_id, ps.line_spacing, ps.line_spacing_type, ps.line_spacing_v2);
    }

    // 첫 char_shape 의 base_size 사용 (단순화)
    let first_cs_id = para.char_shapes.first().map(|cs| cs.char_shape_id).unwrap_or(0);
    let cs = doc.doc_info.char_shapes.get(first_cs_id as usize).expect("char_shape");
    let font_size_pt = cs.base_size as f32 / 100.0;  // HWPUNIT 100 = 1pt
    println!("  first char_shape: id={}, base_size={} HWPUNIT, font_size={:.2}pt",
        first_cs_id, cs.base_size, font_size_pt);

    // 한글 face (lang 0)
    let font_id = cs.font_ids[0];
    let hangul_face = doc.doc_info.font_faces.get(0)
        .and_then(|faces| faces.get(font_id as usize))
        .map(|f| f.name.clone())
        .unwrap_or_default();
    println!("  hangul face: {:?}", hangul_face);

    let cache = hft_cache();
    println!("  hft cache: families={}, alias={}, has_font('{}')={}, has_alias('{}')={}",
        cache.family_count(), cache.alias_count(),
        hangul_face, cache.has_font(&hangul_face),
        hangul_face, cache.has_alias(&hangul_face));
    // 첫 한글 글자로 lookup 테스트
    if let Some(c) = para.text.chars().find(|c| (*c as u32) >= 0xAC00 && (*c as u32) <= 0xD7A3) {
        let cp = c as u32;
        let g = cache.get(&hangul_face, cp);
        println!("  lookup '{}' (U+{:04X}): {}",
            c, cp,
            match g {
                Some(g) => format!("d.len={} advance={} em={}", g.d.len(), g.advance, g.em),
                None => "None".to_string(),
            });
    }

    // CharItemView seq build
    let mut total_width_pt = 0f32;
    let children: Vec<Box<dyn Glyph>> = chars.iter().map(|&c| {
        let w = char_width_pt(&cache, &hangul_face, c as u32, font_size_pt);
        total_width_pt += w;
        let view = build_char_item_view_pt(c, font_size_pt, w);
        Box::new(CivItem(view)) as Box<dyn Glyph>
    }).collect();

    println!();
    println!("  per-char width sum (pt): {:.2}", total_width_pt);
    println!("  per-char width sum → HWPUNIT (×100): {:.0}", total_width_pt * 100.0);

    // stored line_segs[0] 비교
    let ls0 = &para.line_segs[0];
    println!();
    println!("  stored line_segs[0]:");
    println!("    vertical_pos: {} HWPUNIT", ls0.vertical_pos);
    println!("    line_height: {} HWPUNIT", ls0.line_height);
    println!("    text_height: {} HWPUNIT", ls0.text_height);
    println!("    baseline_distance: {} HWPUNIT", ls0.baseline_distance);
    println!("    segment_width: {} HWPUNIT", ls0.segment_width);

    let comp = MutComp { children };
    let mut output = AppendRecorder::default();
    ppt_compose_layout(
        &comp, 1, Break::new(0, (chars.len() as i32) - 1),
        -1, 0, &mut output,
    );

    // output 의 CharItemView 의 width sum
    let mut out_width_pt = 0f32;
    let mut out_count = 0;
    for item in &output.items {
        if let Some(g) = item {
            if let Some(civ) = g.as_any().downcast_ref::<CharItemView>() {
                out_width_pt += civ.width;
                out_count += 1;
            }
        }
    }
    println!();
    println!("  ppt_compose_layout output:");
    println!("    total items appended: {}", output.items.len());
    println!("    CharItemView count: {}", out_count);
    println!("    CharItemView width sum (pt): {:.2}", out_width_pt);

    // (선택) output line_height (CharItemView 첫 항목)
    for item in &output.items {
        if let Some(g) = item {
            if let Some(civ) = g.as_any().downcast_ref::<CharItemView>() {
                println!("    first CharItemView line_height: {:.2}pt (= {:.0} HWPUNIT)",
                    civ.line_height, civ.line_height * 100.0);
                break;
            }
        }
    }

    // ── G-3-allocation wire ────────────────
    // output 의 각 CharItemView 에 대해 누적 x 위치로 sub-Allocation 만들고
    // CharItemView::allocate(alloc, &mut ext) 호출 → 각 글자의 bbox 산출.
    //
    // y = baseline 위치 (단순화: 0 으로 두고 ascent/descent 가 ext.top/bottom 결정).
    // sub-allocation:
    //   x.origin = cursor_x (현재 글자 시작 좌표)
    //   x.span = civ.width (또는 civ.total_height 가 advance 면 그것)
    //   x.alignment = 0.0 (origin 이 begin)
    //   y.origin = baseline_y (= 0)
    //   y.span = civ.line_height
    //   y.alignment = civ.vertical_anchor_ratio (origin 이 ascent 만큼 위)
    println!();
    println!("  ── G-3-allocation wire: per-glyph bbox ──");
    let mut cursor_x_pt = 0f32;
    let baseline_y_pt = 0f32;
    let mut overall_min_x = f32::INFINITY;
    let mut overall_max_x = f32::NEG_INFINITY;
    let mut overall_min_y = f32::INFINITY;
    let mut overall_max_y = f32::NEG_INFINITY;
    let mut glyph_idx = 0;
    let mut civ_clones: Vec<CharItemView> = Vec::new();
    for item in &output.items {
        if let Some(g) = item {
            if let Some(civ) = g.as_any().downcast_ref::<CharItemView>() {
                civ_clones.push(civ.clone());
            }
        }
    }
    let civ_total = civ_clones.len();
    for civ in civ_clones.iter_mut() {
        let alloc = Allocation::new(
            Allotment::new(cursor_x_pt, civ.width, 0.0),
            Allotment::new(baseline_y_pt, civ.line_height, civ.vertical_anchor_ratio),
        );
        let mut ext = Extension::default();
        civ.allocate(&alloc, &mut ext);
        let c = char::from_u32(civ.char_code as u32).unwrap_or('?');
        let c_disp = match civ.char_code {
            0x0d => "\\r".to_string(),
            0x0a => "\\n".to_string(),
            0x20 => "SP".to_string(),
            _ => format!("{:?}", c),
        };
        if glyph_idx < 8 || glyph_idx >= civ_total.saturating_sub(2) {
            println!(
                "    [{:>2}] char={:<6} cursor_x={:>7.2}pt  w={:>5.2}  ext=({:>7.2},{:>7.2})-({:>7.2},{:>7.2})  th={:.2}",
                glyph_idx, c_disp, cursor_x_pt, civ.width,
                ext.left, ext.top, ext.right, ext.bottom, civ.total_height
            );
        } else if glyph_idx == 8 {
            println!("    ...");
        }
        overall_min_x = overall_min_x.min(ext.left);
        overall_max_x = overall_max_x.max(ext.right);
        overall_min_y = overall_min_y.min(ext.top);
        overall_max_y = overall_max_y.max(ext.bottom);
        cursor_x_pt += civ.width;
        glyph_idx += 1;
    }

    println!();
    println!("  ── overall bbox (모든 glyph union) ──");
    println!("    min_x = {:.2}pt   max_x = {:.2}pt   width = {:.2}pt ({:.0} HWPUNIT)",
        overall_min_x, overall_max_x, overall_max_x - overall_min_x,
        (overall_max_x - overall_min_x) * 100.0);
    println!("    min_y = {:.2}pt   max_y = {:.2}pt   height = {:.2}pt ({:.0} HWPUNIT)",
        overall_min_y, overall_max_y, overall_max_y - overall_min_y,
        (overall_max_y - overall_min_y) * 100.0);

    println!();
    println!("  ── stored 와 비교 ──");
    println!("    glyph union width = {:>5.0} HWPUNIT", (overall_max_x - overall_min_x) * 100.0);
    println!("    stored segment_width (column 너비) = {:>5} HWPUNIT", ls0.segment_width);
    println!("    stored line_height = {:>5} HWPUNIT", ls0.line_height);
    println!("    our line_height    = {:>5.0} HWPUNIT", (overall_max_y - overall_min_y) * 100.0);
    println!();
    println!("  주의: stored segment_width 는 column 전체 너비. glyph union width 는");
    println!("  실제 ink 영역 — 짧으면 정상 (오른쪽에 빈 공간).");

    // ── G-4: SvgSurface dispatch wire ────────────────
    // 각 CharItemView 를 SvgSurface 에 emit. 한글 글자: HFT path emit 시도 →
    // miss 면 SVG <text> fallback (system font, font-family="HBatang") 으로 buffer 에 직접 append.
    // unit: px (96 DPI 기준, pt × 96/72).
    // baseline_y: stored line_segs[0].vertical_pos (HWPUNIT/100 = pt) + ascent_pt.
    //
    // canvas: GT 페이지 (272 × 394 mm) = 1028 × 1489 px @ 96 DPI 와 동일.
    use std::fmt::Write;
    println!();
    println!("  ── G-4: SvgSurface dispatch wire ──");
    let dpi = 96.0_f32;
    let pt_to_px = dpi / 72.0;
    let canvas_w_px = 1028.0_f32;
    let canvas_h_px = 1489.0_f32;
    let mut surface = SvgSurface::new(canvas_w_px, canvas_h_px)
        .with_hft_cache(cache.clone());

    let stored_vpos_pt = ls0.vertical_pos as f32 / 100.0;  // HWPUNIT → pt
    let baseline_y_px = stored_vpos_pt * pt_to_px + font_size_pt * 0.8 * pt_to_px;
    let margin_left_pt = 50.0_f32;  // 임시 page margin
    let baseline_x_offset_px = margin_left_pt * pt_to_px;

    let font = Font {
        family: hangul_face.clone(),
        size: font_size_pt * pt_to_px,
        bold: false,
        italic: false,
    };
    let brush = Brush::Solid(SolidBrush::new(
        Color::from_rgb(0, 0, 0, std::ptr::null_mut())
    ));

    // HFT cache 에 hangul_face 있는지 확인
    let hft_face_ok = cache.has_font(&hangul_face) || cache.has_alias(&hangul_face);
    let fallback_family = "HBatang";  // 한컴 TTF
    let font_size_px = font_size_pt * pt_to_px;

    let mut emitted_hft = 0;
    let mut emitted_text = 0;

    let mut cursor_x_pt = 0f32;
    for civ in civ_clones.iter() {
        let codepoints: Vec<u16> = vec![civ.char_code];
        let x_px = baseline_x_offset_px + cursor_x_pt * pt_to_px;
        // control 문자 (\r, \n, \t) 는 emit skip
        if matches!(civ.char_code, 0x0d | 0x0a | 0x09) {
            cursor_x_pt += civ.width;
            continue;
        }
        let positions = vec![PointImpl { x: x_px, y: baseline_y_px }];
        // HFT lookup 시도 (그 글자 cp 에 대해)
        let cp32 = civ.char_code as u32;
        if hft_face_ok && cache.get(&hangul_face, cp32).map(|g| !g.d.is_empty()).unwrap_or(false) {
            surface.draw_driver_string(&codepoints, &font, &brush, &positions, &Transform2D::IDENTITY);
            emitted_hft += 1;
        } else {
            // SVG <text> fallback. SvgSurface.buffer 에 직접 append.
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
                x_px, baseline_y_px, fallback_family, font_size_px, escaped
            ).unwrap();
            emitted_text += 1;
        }
        cursor_x_pt += civ.width;
    }
    println!("    emit: HFT path={}, SVG <text>={}", emitted_hft, emitted_text);

    let svg = surface.finish();
    let out_path = std::path::PathBuf::from("../../work/debug/probe_g_real.svg");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out_path, &svg).expect("write svg");
    println!("    SVG 출력: {} ({} bytes)", out_path.display(), svg.len());

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
                println!("    PNG 출력: {}", png_path.display());
            }
        }
        Err(e) => println!("    usvg parse err: {}", e),
    }
}
