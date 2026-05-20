//! `probe_g_drawdirect_multi` — Stage 4-B: 한 문항/페이지 전체 draw_direct path.
//!
//! probe_g_drawdirect (single glyph) 의 wire pattern 을 multi-paragraph 에 적용.
//! 한 문항 (paragraph 들) 의 모든 글자를 draw_direct outer flow 통해 emit.
//!
//! 흐름:
//!   1. parse_hwpx → section.paragraphs
//!   2. 각 paragraph × line_seg × char 별:
//!     a. ci 빌드 (sentinel ControlBlock)
//!     b. deps.current_char/baseline_x/y/face update
//!     c. draw_direct(ci, alloc, flag, bw, deps) 호출 → outer flow byte-eq 활성화
//!     d. deps.surface_fill_path 가 HFT path → SvgSurface buffer
//!   3. SVG → PNG

use kdsnr_render::{
    blip_glyph::Allocation,
    brush::Brush,
    bw_mode::BWMode,
    char_item_view::{CharItemView, RunProperty},
    char_item_view_draw_direct::{draw_direct, BrushKind, DrawDirectDeps},
    flag::Flag,
    share_ptr::ControlBlock,
    svg_surface::SvgSurface,
};
use std::sync::Arc;
use std::ptr;
use std::fmt::Write;

fn hft_cache() -> Arc<kdsnr_hft::HftCache> {
    let mut cache = kdsnr_hft::HftCache::new();
    let _n = kdsnr_hft::embedded::load_into(&mut cache).expect("HFT load");
    use kdsnr_hft::alias::FaceCategory;
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hangul);
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Hanja);
    cache.add_alias("함초롬바탕", "HGSMJ", FaceCategory::Symbol);
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hangul);
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Hanja);
    cache.add_alias("함초롬돋움", "HGGGT", FaceCategory::Symbol);
    Arc::new(cache)
}

fn sentinel_ctrl() -> *mut ControlBlock<u8> {
    Box::into_raw(Box::new(ControlBlock::<u8> {
        obj: 0xCAFEBABEu64 as *mut u8,
        refcount: 1,
    }))
}

/// raw `CharItemView` 빌더 — sentinel ControlBlock 으로 brush dispatch 활성화.
/// (땜질 wire — render::CharItemView 의 raw ctor port 까지 부담스러우므로.
/// draw_direct outer flow Stage 1-9 가 byte-eq 호출되는지 검증 목적.)
fn build_minimal_ci(character: u16) -> (CharItemView, Vec<*mut u8>) {
    let brush_cb = Box::into_raw(Box::new(ControlBlock::<Brush> {
        obj: 0xC0FFEE00u64 as *mut Brush,
        refcount: 1,
    }));
    let mut rp = RunProperty::new_empty();
    rp.brush = brush_cb;
    let rp_box = Box::into_raw(Box::new(rp));
    let rp_cb = Box::into_raw(Box::new(ControlBlock::<RunProperty> {
        obj: rp_box,
        refcount: 1,
    }));
    let mut ci = CharItemView::new_empty();
    ci.character = character;
    ci.run_property = rp_cb;
    let leaks: Vec<*mut u8> = vec![
        brush_cb as *mut u8,
        rp_box as *mut u8,
        rp_cb as *mut u8,
    ];
    (ci, leaks)
}

struct SvgSurfaceDrawDeps<'a> {
    surface: &'a mut SvgSurface,
    cache: &'a kdsnr_hft::HftCache,
    current_char: u32,
    current_baseline_x_px: f32,
    current_baseline_y_px: f32,
    current_font_size_px: f32,
    current_face: String,
    current_color: (u8, u8, u8),
    sentinels: Vec<*mut ControlBlock<u8>>,
    /// stat
    fill_path_calls: u32,
    underline_calls: u32,
    no_path_skips: u32,
}

impl<'a> SvgSurfaceDrawDeps<'a> {
    fn new(surface: &'a mut SvgSurface, cache: &'a kdsnr_hft::HftCache) -> Self {
        Self {
            surface, cache,
            current_char: 0,
            current_baseline_x_px: 0.0,
            current_baseline_y_px: 0.0,
            current_font_size_px: 0.0,
            current_face: String::new(),
            current_color: (0, 0, 0),
            sentinels: Vec::new(),
            fill_path_calls: 0,
            underline_calls: 0,
            no_path_skips: 0,
        }
    }
    fn alloc_sentinel(&mut self) -> *mut ControlBlock<u8> {
        let s = sentinel_ctrl();
        self.sentinels.push(s);
        s
    }
}

impl<'a> Drop for SvgSurfaceDrawDeps<'a> {
    fn drop(&mut self) {
        for s in self.sentinels.drain(..) {
            unsafe { let _ = Box::from_raw(s); }
        }
    }
}

impl<'a> DrawDirectDeps for SvgSurfaceDrawDeps<'a> {
    unsafe fn shape_engine_warmup(&mut self) {}
    unsafe fn default_brush_fallback(&mut self) -> *mut ControlBlock<u8> { ptr::null_mut() }
    unsafe fn default_pen_fallback(&mut self) -> *mut ControlBlock<u8> { ptr::null_mut() }
    unsafe fn pen_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 { 0 }
    unsafe fn brush_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 { BrushKind::Solid as u32 }

    unsafe fn get_cached_render_path(
        &mut self, _ci: &CharItemView, _alloc: &Allocation,
    ) -> *mut ControlBlock<u8> {
        if let Some(g) = self.cache.get(&self.current_face, self.current_char) {
            if !g.d.is_empty() {
                return self.alloc_sentinel();
            }
        }
        self.no_path_skips += 1;
        ptr::null_mut()
    }

    unsafe fn to_render_brush(
        &mut self, kind: BrushKind, _: *mut ControlBlock<u8>,
    ) -> *mut ControlBlock<u8> {
        if matches!(kind, BrushKind::Solid) { self.alloc_sentinel() } else { ptr::null_mut() }
    }

    unsafe fn surface_fill_path(&mut self, _: *mut ControlBlock<u8>, _: *mut ControlBlock<u8>) {
        if let Some(g) = self.cache.get(&self.current_face, self.current_char) {
            if !g.d.is_empty() && g.em > 0 {
                let s = self.current_font_size_px / g.em as f32;
                let (r, gn, b) = self.current_color;
                let fill_hex = format!("#{:02x}{:02x}{:02x}", r, gn, b);
                writeln!(
                    &mut self.surface.buffer,
                    "<path transform=\"matrix({:.6} 0 0 {:.6} {:.3} {:.3})\" d=\"{}\" fill=\"{}\"/>",
                    s, -s, self.current_baseline_x_px, self.current_baseline_y_px,
                    g.d, fill_hex
                ).unwrap();
                self.fill_path_calls += 1;
            }
        }
    }

    unsafe fn to_render_pen(&mut self, _: *mut ControlBlock<u8>) -> *mut ControlBlock<u8> {
        ptr::null_mut()
    }
    unsafe fn surface_stroke_path(&mut self, _: *mut ControlBlock<u8>, _: *mut ControlBlock<u8>) {}
    unsafe fn draw_underline(&mut self, _: &CharItemView, _: &Allocation, _: &Flag) {
        self.underline_calls += 1;
    }
}

/// HFT advance lookup (fallback estimates).
fn char_width_pt(cache: &kdsnr_hft::HftCache, face: &str, cp: u32, font_size_pt: f32) -> f32 {
    if let Some(g) = cache.get(face, cp) {
        if g.em > 0 && g.advance > 0 {
            let ratio = g.advance as f32 / g.em as f32;
            if ratio > 0.3 && ratio < 2.0 { return ratio * font_size_pt; }
        }
    }
    if cp == 0x20 || cp == 0xa0 || cp == 0x09 { font_size_pt * 0.27 }
    else if (0x30..=0x39).contains(&cp) { font_size_pt * 0.50 }
    else if cp < 0x80 { font_size_pt * 0.40 }
    else { font_size_pt * 1.0 }
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

    let dpi = 96.0_f32;
    let pt_to_px = dpi / 72.0;
    let canvas_w_px = 1028.0_f32;
    let canvas_h_px = 1489.0_f32;
    let mut surface = SvgSurface::new(canvas_w_px, canvas_h_px)
        .with_hft_cache(cache.clone());

    let margin_left_pt = 50.0_f32;
    let margin_top_pt = 50.0_f32;

    let bw = BWMode::V0;
    let flag = Flag(0x00000800u64);
    let alloc: Allocation = unsafe { std::mem::zeroed() };

    let mut deps = SvgSurfaceDrawDeps::new(&mut surface, &cache);
    let mut stats_glyph = 0u32;

    for para in section.paragraphs.iter() {
        if para.text.is_empty() || para.line_segs.is_empty() { continue; }
        let chars: Vec<u16> = para.text.encode_utf16().collect();
        let n = chars.len() as u32;
        let cs_id = para.char_shapes.first().map(|cs| cs.char_shape_id).unwrap_or(0);
        let cs = match doc.doc_info.char_shapes.get(cs_id as usize) { Some(c) => c, None => continue };
        let font_size_pt = cs.base_size as f32 / 100.0;
        let font_id = cs.font_ids[0];
        let face = doc.doc_info.font_faces.get(0)
            .and_then(|f| f.get(font_id as usize))
            .map(|f| f.name.clone())
            .unwrap_or_default();

        for (li, lseg) in para.line_segs.iter().enumerate() {
            let next_start = para.line_segs.get(li + 1).map(|l| l.text_start).unwrap_or(n);
            let start = lseg.text_start as usize;
            let end = (next_start as usize).min(chars.len());
            if start >= end { continue; }

            let baseline_y_pt = lseg.vertical_pos as f32 / 100.0
                + lseg.baseline_distance as f32 / 100.0;
            let column_start_pt = lseg.column_start as f32 / 100.0;
            let mut cursor_x_pt = 0f32;

            for &c in &chars[start..end] {
                let w_pt = char_width_pt(&cache, &face, c as u32, font_size_pt);
                if matches!(c, 0x0d | 0x0a) {
                    cursor_x_pt += w_pt;
                    continue;
                }

                let (ci, leaks) = build_minimal_ci(c);
                deps.current_char = c as u32;
                deps.current_baseline_x_px = (margin_left_pt + column_start_pt + cursor_x_pt) * pt_to_px;
                deps.current_baseline_y_px = (margin_top_pt + baseline_y_pt) * pt_to_px;
                deps.current_font_size_px = font_size_pt * pt_to_px;
                deps.current_face = face.clone();
                deps.current_color = (0, 0, 0);

                unsafe {
                    let _ = draw_direct(&ci, &alloc, &flag, bw, &mut deps);
                }
                // cleanup leaked sentinels
                for p in leaks {
                    unsafe { let _ = Box::from_raw(p as *mut ControlBlock<u8>); }
                }
                std::mem::forget(ci);  // CharItemView 의 _trailing/_image_painter 가 raw bytes — drop side effect 없음
                stats_glyph += 1;
                cursor_x_pt += w_pt;
            }
        }
    }

    println!();
    println!("stats:");
    println!("  glyphs draw_direct dispatched: {}", stats_glyph);
    println!("  fill_path callbacks: {}", deps.fill_path_calls);
    println!("  underline callbacks: {}", deps.underline_calls);
    println!("  HFT miss (no path) skips: {}", deps.no_path_skips);
    drop(deps);

    let svg = surface.finish();
    let out_path = std::path::PathBuf::from("../../work/debug/probe_g_drawdirect_multi.svg");
    if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent).ok(); }
    std::fs::write(&out_path, &svg).expect("write svg");
    println!();
    println!("SVG: {} ({} bytes)", out_path.display(), svg.len());

    let png_path = out_path.with_extension("png");
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let hwp_ttf_dir = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install";
    if let Ok(rd) = std::fs::read_dir(hwp_ttf_dir) {
        for e in rd.flatten() {
            if e.path().extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("ttf")) == Some(true) {
                let _ = opt.fontdb_mut().load_font_file(&e.path());
            }
        }
    }
    let hyhwpeq = "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/TTF/Install/HYHWPEQ.TTF";
    if std::path::Path::new(hyhwpeq).exists() { opt.fontdb_mut().load_font_file(hyhwpeq).ok(); }
    let hancom_fonts = "../../work/fonts/hancom";
    if let Ok(rd) = std::fs::read_dir(hancom_fonts) {
        for e in rd.flatten() {
            if e.path().extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("ttf")) == Some(true) {
                let _ = opt.fontdb_mut().load_font_file(&e.path());
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
                println!("PNG: {}", png_path.display());
            }
        }
        Err(e) => println!("usvg err: {}", e),
    }
}
