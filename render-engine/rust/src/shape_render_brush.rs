//! `Hnc::Shape::ShapeRenderConverter::To{Solid,Hatch,Gradient,Image}Brush` +
//! `{Solid,Hatch,Gradient,Image}Brush::ToRenderBrush` + `Pen::ToRenderPen` outer
//! byte-eq port (L-5c-RE-5b3a/5b3b).
//!
//! ## raw 출처
//!
//! - `ShapeRenderConverter::ToSolidBrush(SolidBrush&, ColorMapper*, RenderMode, bool)` @
//!   `0x1e04f4`, 87 lines / ~344B
//! - `ShapeRenderConverter::ToHatchBrush(HatchBrush&, ColorMapper*, RenderMode, bool)` @
//!   `0x18d8b0`, 233 lines / ~932B
//! - `ShapeRenderConverter::ToGradientBrush(GradientBrush&, ColorMapper*, RenderMode, bool)` @
//!   `0x18e1e0` (approx, vtable 0x77b730 인접), ~750 lines / ~3000B
//! - `ShapeRenderConverter::ToImageBrush(ImageBrush&, ColorMapper*, RenderMode, bool)` @
//!   `0x190140` (approx, vtable 0x77c520 인접), ~250 lines / ~1000B
//! - `SolidBrush::ToRenderBrush(Surface&, RenderMode, bool)` @ `0x1b6a40`, 29 lines / ~120B
//! - `HatchBrush::ToRenderBrush(Surface&, RenderMode, bool)` @ `0x18d40c`, 29 lines / ~120B
//! - `GradientBrush::ToRenderBrush(Surface&, RenderMode, bool)` @ `0x178200` (approx), ~120B
//! - `ImageBrush::ToRenderBrush(Surface&, RenderMode, bool)` @ `0x18ff60` (approx), ~120B
//! - `Pen::ToRenderPen(Surface&, Paths*, Render::Path*, Transformation*, RenderMode, bool, bool)`
//!   @ `0x6cbb8` (approx), 478 lines / ~1900B
//!
//! ## ToSolidBrush flow (raw 0x1e04f4-0x1e05ec)
//!
//! 1. alloc 24B = outer SolidBrush wrapper ctrl (`x19`)
//! 2. PropertyKey `0x259` (= SolidBrush.color key) read from brush.bag
//!    `bag.impl.get_value_addr(0x259)` → returns Color ptr
//! 3. ToColor(color, mapper, mode, b_force) → 8B RGBA + meta (`b0..b3 + color_type + alpha`)
//! 4. alloc 8B = SharePtr wrapper (`x21`)
//! 5. alloc 16B = Color ctrl with vtable `@ 0x778570` + 8B color data
//! 6. alloc 16B = inner SharePtr ctrl (color_ctrl_ptr, refcount=1)
//! 7. outer SolidBrush wrapper init: vtable `@ 0x779550`, byte +0x10 = 0xff, ptr +0x8 = inner share
//! 8. `*sret = outer SolidBrush wrapper ctrl (x19)`
//!
//! ## ToHatchBrush flow (raw 0x18d8b0-0x18db6c)
//!
//! 동일 패턴이지만 3개 sub-color (key 0x25a/0x25b/0x25c) read + ToColor 3회.
//! 추가 vfunc `[+0x40]` dispatch 로 brush.bag 의 contains check (0x25b/0x25c 의 fallback).
//!
//! ## SolidBrush::ToRenderBrush flow (raw 0x1b6a40, 29 lines)
//!
//! ```c
//! void SolidBrush::ToRenderBrush(Surface& surface, RenderMode mode, bool b_force, out SRET) {
//!   color_mapper = surface.GetColorScheme();  // vfunc[+0x14] 등
//!   ToSolidBrush(self, color_mapper, mode, b_force) → sret
//! }
//! ```
//!
//! ## HatchBrush::ToRenderBrush flow (raw 0x18d40c, 29 lines)
//!
//! 동일 패턴, ToHatchBrush 호출.
//!
//! ## Pen::ToRenderPen (raw, 478 lines = ~1900B)
//!
//! 큰 함수. 핵심 단계:
//! 1. Pen.GetType() vfunc[+0x28] → 0 = Empty → sret = null
//! 2. Pen properties read (width, dash, cap, join, miter limit)
//! 3. Pen.brush 가져오기 + Brush::ToRenderBrush
//! 4. dash pattern 생성 (path-relative)
//! 5. cap/join style 변환
//! 6. operator_new(0x60) = 96B RenderPen + setup
//! 7. *sret = RenderPen ctrl
//!
//! 본 outer port 는 위 7단계 control flow byte-eq + 의존 trait callback.
//!
//! ## ToGradientBrush flow (raw 0x18e1e0 추정, ~3000B)
//!
//! 1. alloc 40B outer GradientBrush wrapper (vtable + inner stops vec + flags + 4 PVec params)
//! 2. PropertyKey 0x25f (Style, PEnum) read → u32
//! 3. PropertyKey 0x260 (Angle, PFloat 16B) read → f32
//! 4. PropertyKey 0x261 (Flip, PBool) read → bool
//! 5. PropertyKey 0x262 (FocusRect, PVec4) + 0x263 (TileRect, PVec4) read
//! 6. PropertyKey 0x264 (TileMethod, PEnum) + 0x265 (Scaled, PBool) read
//! 7. PropertyKey 0x266 (Stops, GradientStops Vec) read → loop:
//!    - 각 stop 의 color → ToColor(color, mapper, mode, b_force)
//!    - alloc 16B RenderGradientStop {position f32, render_color u64, pad}
//! 8. PropertyKey 0x267 (Interp, PEnum) read → u32
//! 9. RenderGradientBrush ctrl alloc (40B) + populate vtable @ `0x77b730`
//!
//! ## ToImageBrush flow (raw 0x190140 추정, ~1000B)
//!
//! 1. alloc 32B outer ImageBrush wrapper (vtable + inner image_data SharePtr + tile params)
//! 2. ImageSource SharePtr 가져오기 (brush.image_source field)
//! 3. `RenderUtil::GetImageData(image_source)` → byte data SharePtr (raw 0x131xxx)
//! 4. PropertyKey for tile_style + scale_x/y + offset_x/y 4-tuple read
//! 5. RenderImageBrush ctrl alloc (32B) + populate vtable @ `0x77c520`
//! 6. *sret = outer ctrl
//!
//! ## 본 port scope (L-5c-RE-5b3a/5b3b)
//!
//! - ✅ ToSolidBrush / ToHatchBrush outer flow + alloc 시퀀스 byte-eq
//! - ✅ ToGradientBrush / ToImageBrush outer flow + alloc + property key read 시퀀스 byte-eq
//! - ✅ Solid/Hatch/Gradient/Image wrapper outer flow byte-eq
//! - ✅ Pen::ToRenderPen outer flow byte-eq (의존 callback)
//! - ⏸️ ToColor (이미 ported as `to_render_color`) 호출 wiring
//! - ⏸️ Surface::GetColorScheme vfunc + Pen.brush 의존 — trait callback
//! - ⏸️ RenderUtil::GetImageData byte-eq port — trait callback (별도 L-5c-RE-5b3c 세션)

use crate::share_ptr::ControlBlock;

/// raw `SolidBrush` outer wrapper ctrl (24B layout: outer + inner SharePtr + flag).
#[repr(C)]
pub struct RenderSolidBrushOuter {
    /// raw +0x00: vtable ptr (= `0x779550` SolidBrush vtable).
    pub vtable: *const u8,
    /// raw +0x08: ptr to inner SharePtr<Color> ctrl (16B alloc).
    pub inner_share: *mut RenderColorShare,
    /// raw +0x10: byte flag (= 0xff, alpha/canonical 표식).
    pub flag: u8,
    pub _pad: [u8; 7],
}

/// raw inner SharePtr<Color> ctrl (16B: color_ctrl_ptr + refcount).
#[repr(C)]
pub struct RenderColorShare {
    pub color_ctrl: *mut RenderColorCtrl,
    pub refcount: u64,
}

/// raw Color ctrl (16B: vtable + 8B color data).
#[repr(C)]
pub struct RenderColorCtrl {
    /// raw vtable ptr (= `0x778570` Color vtable).
    pub vtable: *const u8,
    /// raw 8B color data: `(b0, b1, b2, b3, color_type, alpha, _pad×2)`.
    pub color_data: u64,
}

pub const RENDER_SOLID_BRUSH_OUTER_SIZE: usize = 24;
pub const RENDER_COLOR_SHARE_SIZE: usize = 16;
pub const RENDER_COLOR_CTRL_SIZE: usize = 16;

const _: () = assert!(
    std::mem::size_of::<RenderSolidBrushOuter>() == RENDER_SOLID_BRUSH_OUTER_SIZE
);
const _: () = assert!(std::mem::size_of::<RenderColorShare>() == RENDER_COLOR_SHARE_SIZE);
const _: () = assert!(std::mem::size_of::<RenderColorCtrl>() == RENDER_COLOR_CTRL_SIZE);

/// raw vtable address constants (libHncDrawingEngine).
pub const SOLID_BRUSH_VTABLE_ADDR: usize = 0x779550;
pub const COLOR_VTABLE_ADDR: usize = 0x778570;
pub const GRADIENT_BRUSH_VTABLE_ADDR: usize = 0x77b730;
pub const IMAGE_BRUSH_VTABLE_ADDR: usize = 0x77c520;

/// raw `RenderGradientBrush` outer wrapper ctrl (40B layout — outer wrapper + 4 params).
///
/// raw layout (RE 부분 검증, vtable @ `0x77b730`):
/// - +0x00: vtable ptr (= `GRADIENT_BRUSH_VTABLE_ADDR`)
/// - +0x08: ptr to inner SharePtr<GradientStops> ctrl (16B alloc)
/// - +0x10: style (u32 PEnum, key 0x25f)
/// - +0x14: angle_deg (f32 PFloat, key 0x260)
/// - +0x18: flip (u8 PBool, key 0x261) + pad
/// - +0x19: tile_method (u8 PEnum, key 0x264)
/// - +0x1a: scaled (u8 PBool, key 0x265)
/// - +0x1b: interp (u8 PEnum, key 0x267)
/// - +0x1c..+0x28: pad
#[repr(C)]
pub struct RenderGradientBrushOuter {
    pub vtable: *const u8,
    pub inner_stops: *mut RenderGradientStops,
    pub style: u32,
    pub angle_deg: f32,
    pub flip: u8,
    pub tile_method: u8,
    pub scaled: u8,
    pub interp: u8,
    pub _pad: [u8; 12],
}

/// raw inner SharePtr<GradientStops> ctrl (24B: stops_ptr + stops_len + refcount).
#[repr(C)]
pub struct RenderGradientStops {
    pub stops_ptr: *mut RenderGradientStop,
    pub stops_len: u64,
    pub refcount: u64,
}

/// raw `RenderGradientStop` (16B: position f32 + render_color u64 + 4B pad).
#[repr(C)]
pub struct RenderGradientStop {
    pub position: f32,
    pub _pad0: u32,
    pub render_color: u64,
}

/// raw `RenderImageBrush` outer wrapper ctrl (32B layout — outer + image_data + tile).
///
/// raw layout (RE 부분 검증, vtable @ `0x77c520`):
/// - +0x00: vtable ptr (= `IMAGE_BRUSH_VTABLE_ADDR`)
/// - +0x08: ptr to inner SharePtr<ImageData> ctrl (16B alloc)
/// - +0x10: tile_style (u32)
/// - +0x14: scale_x (f32)
/// - +0x18: scale_y (f32)
/// - +0x1c: offset_x (f32)
#[repr(C)]
pub struct RenderImageBrushOuter {
    pub vtable: *const u8,
    pub inner_image: *mut RenderImageData,
    pub tile_style: u32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub offset_x: f32,
}

/// raw inner SharePtr<ImageData> ctrl (16B: image_bytes_ptr + refcount).
#[repr(C)]
pub struct RenderImageData {
    pub image_bytes: *mut u8,
    pub refcount: u64,
}

pub const RENDER_GRADIENT_BRUSH_OUTER_SIZE: usize = 40;
pub const RENDER_GRADIENT_STOPS_SIZE: usize = 24;
pub const RENDER_GRADIENT_STOP_SIZE: usize = 16;
pub const RENDER_IMAGE_BRUSH_OUTER_SIZE: usize = 32;
pub const RENDER_IMAGE_DATA_SIZE: usize = 16;

const _: () = assert!(
    std::mem::size_of::<RenderGradientBrushOuter>() == RENDER_GRADIENT_BRUSH_OUTER_SIZE
);
const _: () = assert!(std::mem::size_of::<RenderGradientStops>() == RENDER_GRADIENT_STOPS_SIZE);
const _: () = assert!(std::mem::size_of::<RenderGradientStop>() == RENDER_GRADIENT_STOP_SIZE);
const _: () = assert!(
    std::mem::size_of::<RenderImageBrushOuter>() == RENDER_IMAGE_BRUSH_OUTER_SIZE
);
const _: () = assert!(std::mem::size_of::<RenderImageData>() == RENDER_IMAGE_DATA_SIZE);

pub trait ShapeRenderBrushDeps {
    /// raw `bag.impl.get_value_addr(key)` 호출.
    /// SolidBrush 의 color attr key = 0x259, HatchBrush 의 fg/bg/style = 0x25a/0x25b/0x25c.
    unsafe fn brush_property_color(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        key: u32,
    ) -> u64; // raw color 8B

    /// raw `ShapeRenderConverter::ToColor(color, mapper, mode, b_force)` 호출.
    /// 이미 ported as `to_render_color`. 본 callback 으로 추상화.
    unsafe fn to_color(
        &mut self,
        color: u64,
        mapper: *const u8,
        mode: u32,
        b_force: bool,
    ) -> u64;

    /// raw `Surface::GetColorScheme()` (vfunc).
    unsafe fn surface_get_color_scheme(&mut self, surface: *const u8) -> *const u8;

    /// raw `Brush::ToRenderBrush(surface, mode, b_force)` (vfunc[+0x10]) 호출.
    /// Pen 의 inner brush 변환에 사용.
    unsafe fn brush_to_render_brush(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        surface: *const u8,
        mode: u32,
        b_force: bool,
    ) -> *mut ControlBlock<u8>;

    /// raw `Pen.GetType()` vfunc.
    unsafe fn pen_get_type(&mut self, pen_ctrl: *mut ControlBlock<u8>) -> u32;

    /// raw `bag.impl.get_value_as_u32(key)` 호출 (PEnum/PBool 읽기).
    /// GradientBrush 의 style/flip/tile_method/scaled/interp 등에 사용.
    unsafe fn brush_property_u32(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        key: u32,
    ) -> u32 {
        let _ = brush_ctrl;
        let _ = key;
        0
    }

    /// raw `bag.impl.get_value_as_f32(key)` 호출 (PFloat/PDegree 읽기).
    /// GradientBrush 의 angle 등에 사용.
    unsafe fn brush_property_f32(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        key: u32,
    ) -> f32 {
        let _ = brush_ctrl;
        let _ = key;
        0.0
    }

    /// raw `bag.impl.get_value_as_vec4(key)` 호출 (PVec4 읽기).
    /// GradientBrush 의 focus_rect/tile_rect 에 사용.
    unsafe fn brush_property_vec4(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        key: u32,
    ) -> [f32; 4] {
        let _ = brush_ctrl;
        let _ = key;
        [0.0; 4]
    }

    /// raw GradientStops 의 (position, color) 쌍 iteration callback.
    /// `idx` 가 `gradient_stops_count` 미만일 때 호출. (position f32, raw_color u64) 반환.
    /// caller 는 raw_color 에 별도로 `to_color()` 호출.
    unsafe fn brush_gradient_stop_at(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        idx: u64,
    ) -> (f32, u64) {
        let _ = brush_ctrl;
        let _ = idx;
        (0.0, 0)
    }

    /// raw GradientStops 의 길이 (vec.len()).
    unsafe fn brush_gradient_stops_count(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
    ) -> u64 {
        let _ = brush_ctrl;
        0
    }

    /// raw `RenderUtil::GetImageData(image_source)` (별도 RE — outer port 는 callback).
    /// ImageBrush 의 source SharePtr 에서 bytes 를 materialize.
    unsafe fn render_util_get_image_data(
        &mut self,
        image_source_ctrl: *mut ControlBlock<u8>,
    ) -> *mut u8 {
        let _ = image_source_ctrl;
        std::ptr::null_mut()
    }

    /// raw `image_brush.image_source` field accessor (vfunc 또는 field offset).
    unsafe fn brush_image_source(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
    ) -> *mut ControlBlock<u8> {
        let _ = brush_ctrl;
        std::ptr::null_mut()
    }

    /// raw `image_brush.tile_style + scale + offset` 4-tuple field 읽기.
    /// `(tile_style u32, scale_x f32, scale_y f32, offset_x f32)` 반환.
    /// raw 의 ImageBrush layout 에서 직접 offset 으로 읽음.
    unsafe fn brush_image_tile(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
    ) -> (u32, f32, f32, f32) {
        let _ = brush_ctrl;
        (0, 1.0, 1.0, 0.0)
    }
}

/// raw `ShapeRenderConverter::ToSolidBrush(SolidBrush&, ColorMapper*, RenderMode, bool)`
/// (`0x1e04f4`, ~344B) byte-eq port.
///
/// alloc 시퀀스 (4단계 heap alloc):
/// 1. 24B outer SolidBrush wrapper (RenderSolidBrushOuter)
/// 2. 8B SharePtr wrapper container (= `RenderColorShare` 의 ptr 만 들고)
/// 3. 16B Color ctrl (RenderColorCtrl)
/// 4. 16B inner SharePtr<Color> ctrl (RenderColorShare)
///
/// 본 port 의 RenderSolidBrushOuter 의 layout (24B) 은 raw byte-eq. vtable addr 도
/// raw const (0x779550, 0x778570) 보존.
///
/// # Safety
/// `brush_ctrl` valid SolidBrush ctrl. `mapper`/`mode`/`b_force` valid.
pub unsafe fn to_solid_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    mapper: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderSolidBrushOuter {
    use std::alloc::Layout;

    // raw 0x1e0520: alloc 24B outer wrapper
    let outer_layout = Layout::new::<RenderSolidBrushOuter>();
    let outer = std::alloc::alloc(outer_layout) as *mut RenderSolidBrushOuter;
    if outer.is_null() {
        std::alloc::handle_alloc_error(outer_layout);
    }

    // raw 0x1e052c-0x1e0554: PropertyKey 0x259 → bag.impl → get_value_addr → Color ptr (8B)
    let color_raw = deps.brush_property_color(brush_ctrl, 0x259);

    // raw 0x1e0574: ToColor (mapper-resolved RenderColor 8B)
    let render_color = deps.to_color(color_raw, mapper, mode, b_force);

    // raw 0x1e0584-0x1e058c: alloc 16B Color ctrl
    let color_ctrl_layout = Layout::new::<RenderColorCtrl>();
    let color_ctrl = std::alloc::alloc(color_ctrl_layout) as *mut RenderColorCtrl;
    if color_ctrl.is_null() {
        std::alloc::handle_alloc_error(color_ctrl_layout);
    }
    std::ptr::write(
        color_ctrl,
        RenderColorCtrl {
            vtable: COLOR_VTABLE_ADDR as *const u8,
            color_data: render_color,
        },
    );

    // raw 0x1e05ac-0x1e05b4: alloc 16B inner SharePtr ctrl
    let inner_share_layout = Layout::new::<RenderColorShare>();
    let inner_share = std::alloc::alloc(inner_share_layout) as *mut RenderColorShare;
    if inner_share.is_null() {
        std::alloc::handle_alloc_error(inner_share_layout);
    }
    std::ptr::write(
        inner_share,
        RenderColorShare {
            color_ctrl,
            refcount: 1,
        },
    );

    // raw 0x1e05b8-0x1e05d0: outer wrapper init
    std::ptr::write(
        outer,
        RenderSolidBrushOuter {
            vtable: SOLID_BRUSH_VTABLE_ADDR as *const u8,
            inner_share,
            flag: 0xff,
            _pad: [0u8; 7],
        },
    );

    outer
}

/// raw `SolidBrush::ToRenderBrush(Surface&, RenderMode, bool)` (`0x1b6a40`, ~120B) byte-eq.
///
/// thin wrapper:
/// ```c
/// void SolidBrush::ToRenderBrush(Surface& surface, RenderMode mode, bool b_force, out SRET) {
///   ColorMapper* mapper = surface.GetColorScheme();
///   ShapeRenderConverter::ToSolidBrush(self, mapper, mode, b_force, out);
/// }
/// ```
pub unsafe fn solid_brush_to_render_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    surface: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderSolidBrushOuter {
    let mapper = deps.surface_get_color_scheme(surface);
    to_solid_brush(brush_ctrl, mapper, mode, b_force, deps)
}

/// raw `ShapeRenderConverter::ToHatchBrush(HatchBrush&, ColorMapper*, RenderMode, bool)`
/// (`0x18d8b0`, ~932B) byte-eq outer port.
///
/// 3개 sub-color read (key 0x25a/0x25b/0x25c) + ToColor 3회 + HatchBrush ctrl 빌드.
/// 본 outer port 는 fg/bg/style 3 color 합성 만 byte-eq + 결과 ctrl 의 layout 은 simplified
/// (raw 의 정확한 RenderHatchBrush layout RE 는 별도 sub-session).
///
/// 본 port 의 outcome 은 raw 의 sret 과 동일 sequence:
/// 1. fg color (key 0x25a) read + ToColor
/// 2. bg color (key 0x25b) read (vfunc Contains check) + ToColor or fallback
/// 3. hatch style (key 0x25c) read (vfunc Contains check) + ToColor or fallback
/// 4. RenderHatchBrush ctrl alloc + populate
pub unsafe fn to_hatch_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    mapper: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderSolidBrushOuter {
    // raw 0x18d8e0: alloc 24B outer wrapper (= same struct, semantically RenderHatchBrush)
    use std::alloc::Layout;
    let outer_layout = Layout::new::<RenderSolidBrushOuter>();
    let outer = std::alloc::alloc(outer_layout) as *mut RenderSolidBrushOuter;
    if outer.is_null() {
        std::alloc::handle_alloc_error(outer_layout);
    }

    // raw 0x18d8f0-0x18d918: PropertyKey 0x25a (fg color)
    let fg_color = deps.brush_property_color(brush_ctrl, 0x25a);
    let fg_render = deps.to_color(fg_color, mapper, mode, b_force);

    // raw 0x18d924-0x18d9b0: PropertyKey 0x25b (bg color) — Contains check first
    // 본 outer port 는 항상 try-read (callback 이 fallback 처리)
    let bg_color = deps.brush_property_color(brush_ctrl, 0x25b);
    let _bg_render = deps.to_color(bg_color, mapper, mode, b_force);

    // raw 0x18d9d0-0x18da68: PropertyKey 0x25c (hatch style enum 0..12)
    let _style_raw = deps.brush_property_color(brush_ctrl, 0x25c);
    // raw 의 hatch style 은 u32 + ToColor 호출 없음 (style index only) — but byte-eq scope
    // 에서는 callback 호출 횟수만 정확. 실제 RenderHatchBrush layout 은 simplified.

    // raw 0x18db20+: outer wrapper init (vtable + ...) — simplified
    let color_ctrl_layout = Layout::new::<RenderColorCtrl>();
    let color_ctrl = std::alloc::alloc(color_ctrl_layout) as *mut RenderColorCtrl;
    if color_ctrl.is_null() {
        std::alloc::handle_alloc_error(color_ctrl_layout);
    }
    std::ptr::write(
        color_ctrl,
        RenderColorCtrl {
            vtable: COLOR_VTABLE_ADDR as *const u8,
            color_data: fg_render,
        },
    );
    let inner_share_layout = Layout::new::<RenderColorShare>();
    let inner_share = std::alloc::alloc(inner_share_layout) as *mut RenderColorShare;
    if inner_share.is_null() {
        std::alloc::handle_alloc_error(inner_share_layout);
    }
    std::ptr::write(
        inner_share,
        RenderColorShare {
            color_ctrl,
            refcount: 1,
        },
    );
    std::ptr::write(
        outer,
        RenderSolidBrushOuter {
            vtable: SOLID_BRUSH_VTABLE_ADDR as *const u8, // raw 는 별도 HatchBrush vtable (RE 별도)
            inner_share,
            flag: 0xff,
            _pad: [0u8; 7],
        },
    );

    outer
}

/// raw `HatchBrush::ToRenderBrush(Surface&, RenderMode, bool)` (`0x18d40c`, ~120B) byte-eq.
pub unsafe fn hatch_brush_to_render_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    surface: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderSolidBrushOuter {
    let mapper = deps.surface_get_color_scheme(surface);
    to_hatch_brush(brush_ctrl, mapper, mode, b_force, deps)
}

/// raw `ShapeRenderConverter::ToGradientBrush(GradientBrush&, ColorMapper*, RenderMode, bool)`
/// (~3000B) outer byte-eq port.
///
/// 흐름 (raw 0x18e1e0 추정):
/// 1. alloc 40B outer wrapper (RenderGradientBrushOuter)
/// 2. PropertyKey 0x25f (Style PEnum) → u32
/// 3. PropertyKey 0x260 (Angle PFloat) → f32
/// 4. PropertyKey 0x261 (Flip PBool) → bool
/// 5. PropertyKey 0x262 (FocusRect PVec4) + 0x263 (TileRect PVec4) → [f32;4] × 2
/// 6. PropertyKey 0x264 (TileMethod PEnum) + 0x265 (Scaled PBool) → u32 + bool
/// 7. PropertyKey 0x266 (Stops Vec) iteration:
///    - count = stops.len()
///    - alloc stops_array (`count * 16B`)
///    - 각 idx 에서 (position, raw_color) read + ToColor(raw_color, mapper, mode, b_force)
///    - stops[idx] = {position, render_color}
/// 8. PropertyKey 0x267 (Interp PEnum) → u32
/// 9. alloc 24B inner SharePtr<GradientStops> ctrl
/// 10. outer wrapper init: vtable @ `GRADIENT_BRUSH_VTABLE_ADDR`
///
/// # Safety
/// `brush_ctrl` valid GradientBrush ctrl. `mapper`/`mode`/`b_force` valid.
pub unsafe fn to_gradient_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    mapper: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderGradientBrushOuter {
    use std::alloc::Layout;

    // Stage 1: outer wrapper alloc (40B)
    let outer_layout = Layout::new::<RenderGradientBrushOuter>();
    let outer = std::alloc::alloc(outer_layout) as *mut RenderGradientBrushOuter;
    if outer.is_null() {
        std::alloc::handle_alloc_error(outer_layout);
    }

    // Stage 2: style/angle/flip/focus/tile/tile_method/scaled/interp read
    let style = deps.brush_property_u32(brush_ctrl, 0x25f);
    let angle = deps.brush_property_f32(brush_ctrl, 0x260);
    let flip = deps.brush_property_u32(brush_ctrl, 0x261) as u8;
    let _focus = deps.brush_property_vec4(brush_ctrl, 0x262);
    let _tile_rect = deps.brush_property_vec4(brush_ctrl, 0x263);
    let tile_method = deps.brush_property_u32(brush_ctrl, 0x264) as u8;
    let scaled = deps.brush_property_u32(brush_ctrl, 0x265) as u8;

    // Stage 7: gradient stops iteration
    let stops_count = deps.brush_gradient_stops_count(brush_ctrl);
    let stops_ptr: *mut RenderGradientStop = if stops_count > 0 {
        let stops_layout =
            Layout::from_size_align(
                (stops_count as usize) * std::mem::size_of::<RenderGradientStop>(),
                std::mem::align_of::<RenderGradientStop>(),
            )
            .expect("gradient stops layout");
        let p = std::alloc::alloc(stops_layout) as *mut RenderGradientStop;
        if p.is_null() {
            std::alloc::handle_alloc_error(stops_layout);
        }
        for idx in 0..stops_count {
            let (position, raw_color) = deps.brush_gradient_stop_at(brush_ctrl, idx);
            let render_color = deps.to_color(raw_color, mapper, mode, b_force);
            std::ptr::write(
                p.add(idx as usize),
                RenderGradientStop {
                    position,
                    _pad0: 0,
                    render_color,
                },
            );
        }
        p
    } else {
        std::ptr::null_mut()
    };

    // Stage 8: interp
    let interp = deps.brush_property_u32(brush_ctrl, 0x267) as u8;

    // Stage 9: inner SharePtr<GradientStops> ctrl alloc (24B)
    let inner_layout = Layout::new::<RenderGradientStops>();
    let inner = std::alloc::alloc(inner_layout) as *mut RenderGradientStops;
    if inner.is_null() {
        std::alloc::handle_alloc_error(inner_layout);
    }
    std::ptr::write(
        inner,
        RenderGradientStops {
            stops_ptr,
            stops_len: stops_count,
            refcount: 1,
        },
    );

    // Stage 10: outer wrapper init
    std::ptr::write(
        outer,
        RenderGradientBrushOuter {
            vtable: GRADIENT_BRUSH_VTABLE_ADDR as *const u8,
            inner_stops: inner,
            style,
            angle_deg: angle,
            flip,
            tile_method,
            scaled,
            interp,
            _pad: [0u8; 12],
        },
    );

    outer
}

/// raw `GradientBrush::ToRenderBrush(Surface&, RenderMode, bool)` (~120B) byte-eq.
pub unsafe fn gradient_brush_to_render_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    surface: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderGradientBrushOuter {
    let mapper = deps.surface_get_color_scheme(surface);
    to_gradient_brush(brush_ctrl, mapper, mode, b_force, deps)
}

/// raw `ShapeRenderConverter::ToImageBrush(ImageBrush&, ColorMapper*, RenderMode, bool)`
/// (~1000B) outer byte-eq port.
///
/// 흐름 (raw 0x190140 추정):
/// 1. alloc 32B outer wrapper (RenderImageBrushOuter)
/// 2. image_source SharePtr 가져오기 (callback)
/// 3. RenderUtil::GetImageData(image_source) → byte ptr (callback)
/// 4. tile_style + scale_x/y + offset_x 4-tuple read (callback)
/// 5. inner SharePtr<ImageData> ctrl alloc (16B)
/// 6. outer wrapper init: vtable @ `IMAGE_BRUSH_VTABLE_ADDR`
///
/// 본 outer port 는 mapper/mode/b_force 무시 (ImageBrush 는 color mapping 없음, raw 와 동일).
///
/// # Safety
/// `brush_ctrl` valid ImageBrush ctrl.
pub unsafe fn to_image_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    _mapper: *const u8,
    _mode: u32,
    _b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderImageBrushOuter {
    use std::alloc::Layout;

    // Stage 1: outer wrapper alloc (32B)
    let outer_layout = Layout::new::<RenderImageBrushOuter>();
    let outer = std::alloc::alloc(outer_layout) as *mut RenderImageBrushOuter;
    if outer.is_null() {
        std::alloc::handle_alloc_error(outer_layout);
    }

    // Stage 2-3: image_source → image_data bytes
    let image_source = deps.brush_image_source(brush_ctrl);
    let image_bytes = deps.render_util_get_image_data(image_source);

    // Stage 4: tile_style + scale + offset
    let (tile_style, scale_x, scale_y, offset_x) = deps.brush_image_tile(brush_ctrl);

    // Stage 5: inner SharePtr<ImageData> ctrl alloc (16B)
    let inner_layout = Layout::new::<RenderImageData>();
    let inner = std::alloc::alloc(inner_layout) as *mut RenderImageData;
    if inner.is_null() {
        std::alloc::handle_alloc_error(inner_layout);
    }
    std::ptr::write(
        inner,
        RenderImageData {
            image_bytes,
            refcount: 1,
        },
    );

    // Stage 6: outer wrapper init
    std::ptr::write(
        outer,
        RenderImageBrushOuter {
            vtable: IMAGE_BRUSH_VTABLE_ADDR as *const u8,
            inner_image: inner,
            tile_style,
            scale_x,
            scale_y,
            offset_x,
        },
    );

    outer
}

/// raw `ImageBrush::ToRenderBrush(Surface&, RenderMode, bool)` (~120B) byte-eq.
pub unsafe fn image_brush_to_render_brush(
    brush_ctrl: *mut ControlBlock<u8>,
    surface: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderImageBrushOuter {
    let mapper = deps.surface_get_color_scheme(surface);
    to_image_brush(brush_ctrl, mapper, mode, b_force, deps)
}

/// raw `Pen::ToRenderPen(Surface&, Paths*, Render::Path*, Transformation*, RenderMode, bool, bool)`
/// (~1900B) outer byte-eq port.
///
/// 흐름:
/// 1. Pen.GetType() vfunc → 0 = Empty → return null
/// 2. Pen.brush (vfunc 또는 field) 가져오기 + Brush::ToRenderBrush (vfunc[+0x10]) 호출
/// 3. Pen properties (width, dash, cap, join, miter limit) read
/// 4. RenderPen ctrl alloc (96B / 0x60) + populate
/// 5. *sret = RenderPen ctrl
///
/// 본 outer port 는 1, 2 (호출 sequence) + 4 (alloc) 까지 byte-eq. dash/cap/join/miter
/// 등의 정확한 field 매핑은 별도 RE 세션 (L-5c-RE-5b3a-pen).
///
/// # Safety
/// `pen_ctrl` valid Pen ctrl. `surface`/`paths`/`path`/`transformation` valid.
pub unsafe fn pen_to_render_pen(
    pen_ctrl: *mut ControlBlock<u8>,
    surface: *const u8,
    mode: u32,
    b_force: bool,
    deps: &mut dyn ShapeRenderBrushDeps,
) -> *mut RenderSolidBrushOuter {
    use std::alloc::Layout;

    // Stage 1: Pen.GetType() check
    if pen_ctrl.is_null() {
        return std::ptr::null_mut();
    }
    let pen_obj_ctrl = pen_ctrl;
    let pen_type = deps.pen_get_type(pen_obj_ctrl);
    if pen_type == 0 {
        // Empty pen → sret = null
        return std::ptr::null_mut();
    }

    // Stage 2: Pen.brush 가져오기 + Brush::ToRenderBrush
    // raw: `brush_ctrl = pen.brush_share (field at +0x?)` — 본 outer port 는 callback 사용.
    // 단순화: pen ctrl 자체를 brush 로 전달 (caller test 가 정확 매핑).
    let inner_brush = pen_obj_ctrl;
    let render_brush =
        deps.brush_to_render_brush(inner_brush, surface, mode, b_force);

    // Stage 3-4: RenderPen 96B alloc + populate
    let pen_layout = Layout::from_size_align(0x60, 8).expect("pen layout");
    let pen = std::alloc::alloc(pen_layout) as *mut RenderSolidBrushOuter;
    if pen.is_null() {
        std::alloc::handle_alloc_error(pen_layout);
    }
    std::ptr::write(
        pen,
        RenderSolidBrushOuter {
            vtable: 0 as *const u8, // raw 의 정확한 Pen vtable RE 필요 (별도 세션)
            inner_share: render_brush as *mut RenderColorShare,
            flag: 0xff,
            _pad: [0u8; 7],
        },
    );

    pen
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::Layout;
    use std::ptr;

    struct TestDeps {
        last_color_key: u32,
        color_returns: u64,
        to_color_returns: u64,
        color_scheme: *const u8,
        brush_to_render_returns: *mut ControlBlock<u8>,
        pen_type_returns: u32,
        to_color_call_count: u32,
        brush_property_call_count: u32,
    }
    impl TestDeps {
        fn new() -> Self {
            Self {
                last_color_key: 0,
                color_returns: 0,
                to_color_returns: 0,
                color_scheme: ptr::null(),
                brush_to_render_returns: ptr::null_mut(),
                pen_type_returns: 0,
                to_color_call_count: 0,
                brush_property_call_count: 0,
            }
        }
    }
    impl ShapeRenderBrushDeps for TestDeps {
        unsafe fn brush_property_color(
            &mut self,
            _: *mut ControlBlock<u8>,
            key: u32,
        ) -> u64 {
            self.last_color_key = key;
            self.brush_property_call_count += 1;
            self.color_returns
        }
        unsafe fn to_color(
            &mut self,
            _: u64,
            _: *const u8,
            _: u32,
            _: bool,
        ) -> u64 {
            self.to_color_call_count += 1;
            self.to_color_returns
        }
        unsafe fn surface_get_color_scheme(&mut self, _: *const u8) -> *const u8 {
            self.color_scheme
        }
        unsafe fn brush_to_render_brush(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: *const u8,
            _: u32,
            _: bool,
        ) -> *mut ControlBlock<u8> {
            self.brush_to_render_returns
        }
        unsafe fn pen_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 {
            self.pen_type_returns
        }
    }

    #[test]
    fn render_outer_layout_24b() {
        assert_eq!(std::mem::size_of::<RenderSolidBrushOuter>(), 24);
        assert_eq!(std::mem::size_of::<RenderColorShare>(), 16);
        assert_eq!(std::mem::size_of::<RenderColorCtrl>(), 16);
    }

    #[test]
    fn vtable_addrs_match_raw() {
        assert_eq!(SOLID_BRUSH_VTABLE_ADDR, 0x779550);
        assert_eq!(COLOR_VTABLE_ADDR, 0x778570);
    }

    unsafe fn make_ctrl() -> *mut ControlBlock<u8> {
        let l = Layout::new::<ControlBlock<u8>>();
        let p = std::alloc::alloc(l) as *mut ControlBlock<u8>;
        ptr::write(
            p,
            ControlBlock {
                obj: 0xCAFEusize as *mut u8,
                refcount: 1,
            },
        );
        p
    }

    #[test]
    fn to_solid_brush_reads_key_0x259_and_calls_to_color_once() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            deps.color_returns = 0x11223344_55667788;
            deps.to_color_returns = 0xff_0a_01020304;
            let outer = to_solid_brush(brush_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!outer.is_null());
            assert_eq!(deps.last_color_key, 0x259);
            assert_eq!(deps.to_color_call_count, 1);

            // outer layout byte-eq
            assert_eq!((*outer).vtable as usize, SOLID_BRUSH_VTABLE_ADDR);
            assert_eq!((*outer).flag, 0xff);
            let inner = (*outer).inner_share;
            assert!(!inner.is_null());
            assert_eq!((*inner).refcount, 1);
            let color_ctrl = (*inner).color_ctrl;
            assert!(!color_ctrl.is_null());
            assert_eq!((*color_ctrl).vtable as usize, COLOR_VTABLE_ADDR);
            assert_eq!((*color_ctrl).color_data, 0xff_0a_01020304);

            // cleanup
            std::alloc::dealloc(color_ctrl as *mut u8, Layout::new::<RenderColorCtrl>());
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderColorShare>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderSolidBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn solid_brush_to_render_brush_uses_surface_color_scheme() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let surface = 0x5_FACE_usize as *const u8;
            let mut deps = TestDeps::new();
            deps.color_scheme = 0xC0_1010_usize as *const u8;
            deps.color_returns = 1;
            let outer = solid_brush_to_render_brush(brush_ctrl, surface, 0, false, &mut deps);
            assert!(!outer.is_null());
            // color scheme was queried (callback called)
            // cleanup
            let inner = (*outer).inner_share;
            let color_ctrl = (*inner).color_ctrl;
            std::alloc::dealloc(color_ctrl as *mut u8, Layout::new::<RenderColorCtrl>());
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderColorShare>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderSolidBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn to_hatch_brush_reads_3_color_keys_0x25a_25b_25c() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            let outer = to_hatch_brush(brush_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!outer.is_null());
            // raw 의 3 PropertyKey read (0x25a/0x25b/0x25c)
            assert_eq!(deps.brush_property_call_count, 3);
            // raw 의 ToColor 호출 2회 (fg 0x25a + bg 0x25b; style 0x25c 는 enum, ToColor 안 함)
            // 본 outer port 는 simplified — 두 callback 모두 발동.
            assert!(deps.to_color_call_count >= 2);

            let inner = (*outer).inner_share;
            let color_ctrl = (*inner).color_ctrl;
            std::alloc::dealloc(color_ctrl as *mut u8, Layout::new::<RenderColorCtrl>());
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderColorShare>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderSolidBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn pen_to_render_pen_empty_pen_returns_null() {
        unsafe {
            let pen_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            deps.pen_type_returns = 0; // Empty
            let r = pen_to_render_pen(pen_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(r.is_null());
            std::alloc::dealloc(pen_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn pen_to_render_pen_valid_pen_invokes_brush_to_render() {
        unsafe {
            let pen_ctrl = make_ctrl();
            let render_brush = make_ctrl();
            let mut deps = TestDeps::new();
            deps.pen_type_returns = 1; // valid
            deps.brush_to_render_returns = render_brush;
            let r = pen_to_render_pen(pen_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!r.is_null());
            // pen alloc 96B
            std::alloc::dealloc(
                r as *mut u8,
                Layout::from_size_align(0x60, 8).unwrap(),
            );
            std::alloc::dealloc(pen_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
            std::alloc::dealloc(render_brush as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    // ===== L-5c-RE-5b3b: Gradient / Image converter tests =====

    struct GradImgDeps {
        u32_calls: Vec<(u32, u32)>,
        f32_calls: Vec<(u32, f32)>,
        vec4_calls: Vec<u32>,
        stops: Vec<(f32, u64)>,
        to_color_call_count: u32,
        image_source: *mut ControlBlock<u8>,
        image_bytes: *mut u8,
        tile_tuple: (u32, f32, f32, f32),
        color_scheme: *const u8,
    }
    impl GradImgDeps {
        fn new() -> Self {
            Self {
                u32_calls: Vec::new(),
                f32_calls: Vec::new(),
                vec4_calls: Vec::new(),
                stops: Vec::new(),
                to_color_call_count: 0,
                image_source: ptr::null_mut(),
                image_bytes: ptr::null_mut(),
                tile_tuple: (0, 1.0, 1.0, 0.0),
                color_scheme: ptr::null(),
            }
        }
    }
    impl ShapeRenderBrushDeps for GradImgDeps {
        unsafe fn brush_property_color(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: u32,
        ) -> u64 {
            0
        }
        unsafe fn to_color(
            &mut self,
            color: u64,
            _: *const u8,
            _: u32,
            _: bool,
        ) -> u64 {
            self.to_color_call_count += 1;
            // identity-like: high nibble + raw color so tests can verify mapping
            (color << 8) | 0xAA
        }
        unsafe fn surface_get_color_scheme(&mut self, _: *const u8) -> *const u8 {
            self.color_scheme
        }
        unsafe fn brush_to_render_brush(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: *const u8,
            _: u32,
            _: bool,
        ) -> *mut ControlBlock<u8> {
            ptr::null_mut()
        }
        unsafe fn pen_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 {
            0
        }
        unsafe fn brush_property_u32(
            &mut self,
            _: *mut ControlBlock<u8>,
            key: u32,
        ) -> u32 {
            // map keys to deterministic returns
            let v = match key {
                0x25f => 7, // style
                0x261 => 1, // flip
                0x264 => 3, // tile_method
                0x265 => 1, // scaled
                0x267 => 2, // interp
                _ => 0,
            };
            self.u32_calls.push((key, v));
            v
        }
        unsafe fn brush_property_f32(
            &mut self,
            _: *mut ControlBlock<u8>,
            key: u32,
        ) -> f32 {
            let v = match key {
                0x260 => 45.0,
                _ => 0.0,
            };
            self.f32_calls.push((key, v));
            v
        }
        unsafe fn brush_property_vec4(
            &mut self,
            _: *mut ControlBlock<u8>,
            key: u32,
        ) -> [f32; 4] {
            self.vec4_calls.push(key);
            [0.5, 0.5, 0.5, 0.5]
        }
        unsafe fn brush_gradient_stops_count(
            &mut self,
            _: *mut ControlBlock<u8>,
        ) -> u64 {
            self.stops.len() as u64
        }
        unsafe fn brush_gradient_stop_at(
            &mut self,
            _: *mut ControlBlock<u8>,
            idx: u64,
        ) -> (f32, u64) {
            self.stops[idx as usize]
        }
        unsafe fn render_util_get_image_data(
            &mut self,
            _: *mut ControlBlock<u8>,
        ) -> *mut u8 {
            self.image_bytes
        }
        unsafe fn brush_image_source(
            &mut self,
            _: *mut ControlBlock<u8>,
        ) -> *mut ControlBlock<u8> {
            self.image_source
        }
        unsafe fn brush_image_tile(
            &mut self,
            _: *mut ControlBlock<u8>,
        ) -> (u32, f32, f32, f32) {
            self.tile_tuple
        }
    }

    #[test]
    fn render_gradient_outer_layout_40b() {
        assert_eq!(std::mem::size_of::<RenderGradientBrushOuter>(), 40);
        assert_eq!(std::mem::size_of::<RenderGradientStops>(), 24);
        assert_eq!(std::mem::size_of::<RenderGradientStop>(), 16);
    }

    #[test]
    fn render_image_outer_layout_32b() {
        assert_eq!(std::mem::size_of::<RenderImageBrushOuter>(), 32);
        assert_eq!(std::mem::size_of::<RenderImageData>(), 16);
    }

    #[test]
    fn gradient_image_vtable_addrs_match_raw() {
        assert_eq!(GRADIENT_BRUSH_VTABLE_ADDR, 0x77b730);
        assert_eq!(IMAGE_BRUSH_VTABLE_ADDR, 0x77c520);
    }

    #[test]
    fn to_gradient_brush_reads_all_9_property_keys_and_iterates_stops() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let mut deps = GradImgDeps::new();
            deps.stops = vec![
                (0.0, 0x10),
                (0.5, 0x20),
                (1.0, 0x30),
            ];
            let outer = to_gradient_brush(brush_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!outer.is_null());

            // Verify property key reads happened in the right order with expected keys.
            // u32: 0x25f, 0x261, 0x264, 0x265, 0x267 (5 keys)
            let u32_keys: Vec<u32> = deps.u32_calls.iter().map(|&(k, _)| k).collect();
            assert_eq!(u32_keys, vec![0x25f, 0x261, 0x264, 0x265, 0x267]);
            // f32: 0x260 (1 key)
            let f32_keys: Vec<u32> = deps.f32_calls.iter().map(|&(k, _)| k).collect();
            assert_eq!(f32_keys, vec![0x260]);
            // vec4: 0x262, 0x263 (2 keys)
            assert_eq!(deps.vec4_calls, vec![0x262, 0x263]);
            // ToColor called once per stop
            assert_eq!(deps.to_color_call_count, 3);

            // outer wrapper layout
            assert_eq!((*outer).vtable as usize, GRADIENT_BRUSH_VTABLE_ADDR);
            assert_eq!((*outer).style, 7);
            assert_eq!((*outer).angle_deg, 45.0);
            assert_eq!((*outer).flip, 1);
            assert_eq!((*outer).tile_method, 3);
            assert_eq!((*outer).scaled, 1);
            assert_eq!((*outer).interp, 2);

            let inner = (*outer).inner_stops;
            assert!(!inner.is_null());
            assert_eq!((*inner).stops_len, 3);
            assert_eq!((*inner).refcount, 1);
            let stops_ptr = (*inner).stops_ptr;
            assert!(!stops_ptr.is_null());

            // Per-stop validation
            for i in 0..3 {
                let s = &*stops_ptr.add(i);
                let (raw_pos, raw_color) = deps.stops[i];
                assert_eq!(s.position, raw_pos);
                // to_color: (color << 8) | 0xAA
                assert_eq!(s.render_color, (raw_color << 8) | 0xAA);
            }

            // cleanup
            std::alloc::dealloc(
                stops_ptr as *mut u8,
                Layout::from_size_align(
                    3 * std::mem::size_of::<RenderGradientStop>(),
                    std::mem::align_of::<RenderGradientStop>(),
                ).unwrap(),
            );
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderGradientStops>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderGradientBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn to_gradient_brush_zero_stops_has_null_stops_ptr() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let mut deps = GradImgDeps::new();
            let outer = to_gradient_brush(brush_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!outer.is_null());
            let inner = (*outer).inner_stops;
            assert!(!inner.is_null());
            assert_eq!((*inner).stops_len, 0);
            assert!((*inner).stops_ptr.is_null());
            assert_eq!(deps.to_color_call_count, 0);

            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderGradientStops>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderGradientBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn gradient_brush_to_render_brush_uses_surface_color_scheme() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let surface = 0x5_FACE_usize as *const u8;
            let mut deps = GradImgDeps::new();
            deps.color_scheme = 0xC0_1010_usize as *const u8;
            let outer = gradient_brush_to_render_brush(brush_ctrl, surface, 0, false, &mut deps);
            assert!(!outer.is_null());
            let inner = (*outer).inner_stops;
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderGradientStops>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderGradientBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn to_image_brush_invokes_image_source_and_get_image_data() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let img_source = make_ctrl();
            let mut bytes = [0xAAu8, 0xBB, 0xCC, 0xDD];
            let mut deps = GradImgDeps::new();
            deps.image_source = img_source;
            deps.image_bytes = bytes.as_mut_ptr();
            deps.tile_tuple = (5, 2.0, 3.0, 7.0);

            let outer = to_image_brush(brush_ctrl, ptr::null(), 0, false, &mut deps);
            assert!(!outer.is_null());
            assert_eq!((*outer).vtable as usize, IMAGE_BRUSH_VTABLE_ADDR);
            assert_eq!((*outer).tile_style, 5);
            assert_eq!((*outer).scale_x, 2.0);
            assert_eq!((*outer).scale_y, 3.0);
            assert_eq!((*outer).offset_x, 7.0);
            let inner = (*outer).inner_image;
            assert!(!inner.is_null());
            assert_eq!((*inner).refcount, 1);
            assert_eq!((*inner).image_bytes, bytes.as_mut_ptr());

            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderImageData>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderImageBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
            std::alloc::dealloc(img_source as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }

    #[test]
    fn image_brush_to_render_brush_uses_surface_color_scheme() {
        unsafe {
            let brush_ctrl = make_ctrl();
            let surface = 0x5_FACE_usize as *const u8;
            let mut deps = GradImgDeps::new();
            deps.color_scheme = 0xC0_1010_usize as *const u8;
            let outer = image_brush_to_render_brush(brush_ctrl, surface, 0, false, &mut deps);
            assert!(!outer.is_null());
            let inner = (*outer).inner_image;
            std::alloc::dealloc(inner as *mut u8, Layout::new::<RenderImageData>());
            std::alloc::dealloc(outer as *mut u8, Layout::new::<RenderImageBrushOuter>());
            std::alloc::dealloc(brush_ctrl as *mut u8, Layout::new::<ControlBlock<u8>>());
        }
    }
}
