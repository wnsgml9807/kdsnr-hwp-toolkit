//! `probe_g_drawdirect` — Stage 4 정공법 Draw vfunc path 첫 활성화.
//!
//! 흐름:
//!   1. render::CharItemView::new_empty() 로 minimal char view 생성
//!   2. char_item_view_draw_direct::draw_direct(ci, alloc, flag, bw, &mut deps) 호출
//!   3. SvgSurfaceDrawDeps 가 callback 으로 HFT path lookup + SvgSurface fill_path 호출
//!   4. SVG → PNG
//!
//! ControlBlock<u8> 포인터는 sentinel 으로만 사용 (raw C++ shared_ptr 의 ABI 보존).
//! 실제 state (current char, hft cache, brush, surface ref) 는 deps struct 자체에 보관.
//! 한컴 raw 의 outer flow stage 1-9 byte-eq, inner dispatch (HFT path 조회 + Surface fill_path)
//! 는 우리 native Rust 방식. raw inner 의 byte-eq port 는 별도 (L-5c-RE-5b2 GetCachedRenderPath
//! full RE).

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
    Arc::new(cache)
}

/// Sentinel ControlBlock — non-null, but obj 도 non-null (sentinel value).
/// draw_direct 의 outer flow 가 ctrl 의 null 검사 + obj.is_null() 검사만 함.
/// 본 helper 는 두 null 검사 모두 false 반환하는 sentinel 생성.
fn sentinel_ctrl() -> *mut ControlBlock<u8> {
    Box::into_raw(Box::new(ControlBlock::<u8> {
        obj: 0xCAFEBABEu64 as *mut u8,
        refcount: 1,
    }))
}

/// SvgSurface 와 HFT cache 를 들고 있는 deps impl. callback 들이 자체 state 사용.
struct SvgSurfaceDrawDeps<'a> {
    surface: &'a mut SvgSurface,
    cache: &'a kdsnr_hft::HftCache,
    /// 현재 그릴 글자 (raw `CharItemView::character` 와 동일).
    /// callback `get_cached_render_path` 가 사용.
    current_char: u32,
    /// 현재 글자의 baseline (px). callback `surface_fill_path` 가 사용.
    current_baseline_x_px: f32,
    current_baseline_y_px: f32,
    /// 현재 폰트 size (px). emit_glyph_paths 의 transform scale 계산용.
    current_font_size_px: f32,
    /// 현재 face (HFT lookup key).
    current_face: String,
    /// 색상 (Brush::Solid 의 color).
    current_color: (u8, u8, u8),
    /// 임시 sentinel 들 (callback 가 반환). cleanup 시 free.
    sentinels: Vec<*mut ControlBlock<u8>>,
}

impl<'a> SvgSurfaceDrawDeps<'a> {
    fn new(surface: &'a mut SvgSurface, cache: &'a kdsnr_hft::HftCache) -> Self {
        Self {
            surface,
            cache,
            current_char: 0,
            current_baseline_x_px: 0.0,
            current_baseline_y_px: 0.0,
            current_font_size_px: 0.0,
            current_face: String::new(),
            current_color: (0, 0, 0),
            sentinels: Vec::new(),
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
    unsafe fn shape_engine_warmup(&mut self) {
        // raw side effect 없음 (refcount inline 만). no-op.
    }

    unsafe fn default_brush_fallback(&mut self) -> *mut ControlBlock<u8> {
        // 기본 brush 안 씀 (current_color 사용). null 반환 → raw outer flow 가 skip.
        ptr::null_mut()
    }

    unsafe fn default_pen_fallback(&mut self) -> *mut ControlBlock<u8> {
        ptr::null_mut()
    }

    unsafe fn pen_get_type(&mut self, _pen_ctrl: *mut ControlBlock<u8>) -> u32 {
        // 본 wire 는 stroke (outline) 미지원 → Empty pen.
        0
    }

    unsafe fn brush_get_type(&mut self, _brush_ctrl: *mut ControlBlock<u8>) -> u32 {
        // Solid brush 만 지원.
        BrushKind::Solid as u32
    }

    unsafe fn get_cached_render_path(
        &mut self,
        _ci: &CharItemView,
        _allocation: &Allocation,
    ) -> *mut ControlBlock<u8> {
        // HFT cache lookup. miss 시 null 반환 → raw outer flow 가 skip fill, underline 만.
        if let Some(g) = self.cache.get(&self.current_face, self.current_char) {
            if !g.d.is_empty() {
                // sentinel 반환 — 실제 path d-string 은 surface_fill_path 가 cache 다시 query.
                return self.alloc_sentinel();
            }
        }
        ptr::null_mut()
    }

    unsafe fn to_render_brush(
        &mut self,
        kind: BrushKind,
        _brush_ctrl: *mut ControlBlock<u8>,
    ) -> *mut ControlBlock<u8> {
        // Solid 만 sentinel 반환. 나머지는 null (skip).
        if matches!(kind, BrushKind::Solid) {
            self.alloc_sentinel()
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn surface_fill_path(
        &mut self,
        _path_ctrl: *mut ControlBlock<u8>,
        _brush_ctrl: *mut ControlBlock<u8>,
    ) {
        // raw 의 Surface vfunc[+0x10] = FillPath(paths, brush) 호출.
        //
        // 본 wire 는 HFT d-string 을 SVG <path> element 로 buffer 에 직접 emit.
        // (한컴 native 의 paths byte-eq 변환은 별도 작업.)
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
            }
        }
    }

    unsafe fn to_render_pen(&mut self, _pen_ctrl: *mut ControlBlock<u8>) -> *mut ControlBlock<u8> {
        // pen 미지원.
        ptr::null_mut()
    }

    unsafe fn surface_stroke_path(
        &mut self,
        _paths_ctrl: *mut ControlBlock<u8>,
        _pen_ctrl: *mut ControlBlock<u8>,
    ) {
        // pen Empty 이므로 호출 안 됨.
    }

    unsafe fn draw_underline(
        &mut self,
        _ci: &CharItemView,
        _allocation: &Allocation,
        _flag: &Flag,
    ) {
        // underline 미지원.
    }
}

fn main() {
    let cache = hft_cache();
    let canvas_w_px = 200.0_f32;
    let canvas_h_px = 80.0_f32;
    let mut surface = SvgSurface::new(canvas_w_px, canvas_h_px)
        .with_hft_cache(cache.clone());

    // 테스트: '가' (U+AC00) draw_direct path 활성화.
    let face = "신명 중명조"; // HFT alias 에 있는 face
    let test_char: char = '가';
    let font_size_px = 32.0;
    let baseline_x = 10.0;
    let baseline_y = 50.0;

    // raw CharItemView 빌드. ci.character 외에:
    //  - ci.run_property = ControlBlock<RunProperty> sentinel
    //  - RunProperty.brush = ControlBlock<Brush> sentinel (obj 도 non-null)
    // 이래야 draw_direct outer flow 의 stage 1 (GetRealBrush) 가 brush_ctrl non-null 반환 →
    // Stage 6 (Brush 5-way dispatch) 진입.
    let brush_cb = Box::into_raw(Box::new(ControlBlock::<Brush> {
        obj: 0xC0FFEE00u64 as *mut Brush,  // sentinel — deps.brush_get_type 이 그 안 deref 안 함
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
    ci.character = test_char as u16;
    ci.run_property = rp_cb;

    // minimal Allocation (zero — outer flow 가 actual value 안 봄)
    let alloc: Allocation = unsafe { std::mem::zeroed() };
    // Flag.byte1 bit3 = 1 (full draw enable). u32 layout: byte0|byte1|byte2|byte3
    let flag = Flag(0x00000800u64);  // bit11 = byte1 bit3 (full draw enable)
    let bw = BWMode::V0;

    let mut deps = SvgSurfaceDrawDeps::new(&mut surface, &cache);
    deps.current_char = test_char as u32;
    deps.current_baseline_x_px = baseline_x;
    deps.current_baseline_y_px = baseline_y;
    deps.current_font_size_px = font_size_px;
    deps.current_face = face.to_string();
    deps.current_color = (0, 0, 0);

    let outcome = unsafe { draw_direct(&ci, &alloc, &flag, bw, &mut deps) };
    println!("draw_direct outcome: {:?}", outcome);

    drop(deps);

    let svg = surface.finish();
    let out_path = std::path::PathBuf::from("../../work/debug/probe_g_drawdirect.svg");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&out_path, &svg).expect("write svg");
    println!("SVG 출력: {} ({} bytes)", out_path.display(), svg.len());

    // PNG rasterize
    let png_path = out_path.with_extension("png");
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
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
        Err(e) => println!("usvg err: {}", e),
    }
}
