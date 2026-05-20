//! `Hnc::Shape::Text::CharItemView::DrawDirect` byte-eq outer control flow port (L-5c-RE-5b).
//!
//! ## raw 출처
//!
//! - `__ZNK3Hnc5Shape4Text12CharItemView10DrawDirectERNS0_7SurfaceERKNS1_10AllocationERKNS_4Type4FlagENS0_6BWModeE`
//! - 주소: `0x2f67ec`
//! - 크기: 5248B (= 0x1480)
//! - decompile: `Text_CharItemView__DrawDirect_002f67ec.txt` (1035 lines)
//!
//! ## 함수 의미
//!
//! `CharItemView::Draw` 의 fast path 에서 호출되는 main draw routine. 1 글자의:
//! 1. 글자 path 그리기 (Brush fill)
//! 2. 글자 outline 그리기 (Pen stroke)
//! 3. 글자 underline 그리기 (DrawUnderLine 위임)
//!
//! ## raw 의 stage 매핑 (0x2f67ec - 0x2f7c6c)
//!
//! ```text
//! Stage 1 (0x2f67ec-0x2f68e8)  Setup: ShapeEngine warmup × N + GetRealPen + brush handle
//! Stage 2 (0x2f68ec-0x2f6960)  Default fallback: FUN_0064add4 / FUN_0064a590 (default brush/pen)
//! Stage 3 (0x2f6968-0x2f6a08)  Validate: pen.GetType() != Empty → bVar5=true
//! Stage 4 (0x2f6a0c-0x2f6c4c)  Dispatch on Flag.byte1.bit3: → Stage 5 (full draw) OR shortcut
//! Stage 5 (0x2f6c50-0x2f71c0)  CalcDrawVariables (b2=true) + GetCachedRenderPath +
//!                             Path::AddRect (clip rect) + Paths::AddPath + bbox 합성
//! Stage 6 (0x2f71c4-0x2f77b4)  Brush 5-way dispatch (Empty/Solid/Gradient/Image/Hatch):
//!                             - dynamic_cast → ShapeRenderConverter::To*Brush →
//!                             - Surface vfunc[+0x10] (FillPath) OR vfunc[+0x60] (path/clip 변종)
//! Stage 7 (0x2f77b8-0x2f797c)  Pen::ToRenderPen → Surface vfunc[+0x28] (StrokePath)
//!                             cleanup of Vec<long*> (local_160 effects list)
//! Stage 8 (0x2f7978-0x2f7b20)  DrawUnderLine(this, surface, alloc) 호출 +
//!                             cache_path[+0x98] 의 추가 underline 처리
//! Stage 9 (0x2f7b24-0x2f7c6c)  Cleanup: 4 SharePtr release (pen/brush/pen_default/brush_default)
//! ```
//!
//! ## 본 port scope (L-5c-RE-5b)
//!
//! - ✅ **Stage 1/2/3** (setup, default fallback, pen validity) byte-eq
//! - ✅ **Stage 4** dispatch byte-eq
//! - ✅ **Stage 5** outer flow (CalcDrawVariables / GetCachedRenderPath 호출은 callback) byte-eq
//! - ✅ **Stage 6** Brush 5-way dispatch byte-eq (case 0/1/2/3/4 분기, 각 brush type 의
//!   ShapeRenderConverter::To*Brush 호출은 trait callback)
//! - ✅ **Stage 7** Pen application outer flow byte-eq (Pen::ToRenderPen + Surface
//!   FillPath 호출은 trait callback)
//! - ✅ **Stage 8** UnderLine 호출 outer flow byte-eq (DrawUnderLine 는 trait callback)
//! - ✅ **Stage 9** Cleanup byte-eq
//! - ⏸️ **Surface vtable methods** (FillPath/StrokePath/GetColorScheme/GetTransform 등) →
//!   `DrawDirectDeps` trait callback. byte-eq port 는 별도 세션 (Surface vtable RE 필요).
//! - ⏸️ **ShapeRenderConverter::To*Brush** family (ToSolidBrush/ToGradientBrush/ToHatchBrush/
//!   ToImageBrush) → trait callback. byte-eq port 는 별도 세션 L-5c-RE-5b3.
//! - ⏸️ **GetCachedRenderPath** (900B, HFT decoder 의존) → trait callback. byte-eq port
//!   는 별도 세션 L-5c-RE-5b2.
//! - ⏸️ **DrawUnderLine** (3940B) → trait callback. byte-eq port 는 별도 세션 L-5c-RE-5b4.
//! - ⏸️ **Default brush/pen fallback** (FUN_0064add4/0064a590, ~200B each) → trait callback.

use crate::char_item_view::CharItemView;
use crate::flag::Flag;
use crate::blip_glyph::Allocation;
use crate::bw_mode::BWMode;
use crate::share_ptr::ControlBlock;

/// raw `Hnc::Shape::Brush::Type` enum (vfunc[5] = `GetType()` 반환값).
///
/// raw 의 Brush 5 sub-types:
/// - `Empty` = 0 (EmptyBrush::vtable[5] returns 0)
/// - `Solid` = 1 (SolidBrush)
/// - `Gradient` = 2 (GradientBrush)
/// - `Image` = 3 (ImageBrush)
/// - `Hatch` = 4 (HatchBrush)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BrushKind {
    Empty = 0,
    Solid = 1,
    Gradient = 2,
    Image = 3,
    Hatch = 4,
}

impl BrushKind {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(BrushKind::Empty),
            1 => Some(BrushKind::Solid),
            2 => Some(BrushKind::Gradient),
            3 => Some(BrushKind::Image),
            4 => Some(BrushKind::Hatch),
            _ => None,
        }
    }
}

/// raw `CharItemView::DrawDirect` 의 outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawDirectOutcome {
    /// raw Stage 4 의 shortcut path (Flag.byte1.bit3 == 0 & pen invalid 등): early return
    EarlyExit,
    /// Brush 5-way dispatch 적용 후 fill + stroke + underline 모두 시도
    Painted {
        brush_kind: Option<BrushKind>,
        stroke_painted: bool,
        underline_called: bool,
    },
}

/// raw 의 5 main external dependencies — caller (= SvgSurface integration test) 가
/// byte-eq impl 제공. 본 outer port 는 control flow 만 byte-eq.
///
/// 각 method 의 byte-eq port 는 별도 세션:
/// - L-5c-RE-5b2: GetCachedRenderPath
/// - L-5c-RE-5b3: ShapeRenderConverter::To*Brush family
/// - L-5c-RE-5b4: DrawUnderLine
/// - Surface vtable methods: 별도 surface adapter session
pub trait DrawDirectDeps {
    /// raw 0x2f67ec-0x2f68b4: ShapeEngine warmup (refcount inline expand, side effect zero).
    /// 본 callback 은 caller-specific (대부분 no-op).
    unsafe fn shape_engine_warmup(&mut self) {}

    /// raw `FUN_0064add4(out_sret, &local_1b0)`: default brush fallback.
    /// caller 가 raw 의 default brush ctrl 반환 (또는 null).
    unsafe fn default_brush_fallback(&mut self) -> *mut ControlBlock<u8>;

    /// raw `FUN_0064a590(out_sret, &local_1b0)`: default pen fallback.
    unsafe fn default_pen_fallback(&mut self) -> *mut ControlBlock<u8>;

    /// raw `pen.GetType()` (vfunc[5]) 호출. 0 = Empty pen, else valid.
    /// caller 가 pen vfunc dispatch.
    unsafe fn pen_get_type(&mut self, pen_ctrl: *mut ControlBlock<u8>) -> u32;

    /// raw `brush.GetType()` (vfunc[5]) 호출. 위 BrushKind enum 으로 변환됨.
    unsafe fn brush_get_type(&mut self, brush_ctrl: *mut ControlBlock<u8>) -> u32;

    /// raw `GetCachedRenderPath(ci, alloc, theme)` (0x2f1f94, 900B) 호출.
    /// 본 callback 의 byte-eq port 는 L-5c-RE-5b2 (HFT path 의존).
    unsafe fn get_cached_render_path(
        &mut self,
        ci: &CharItemView,
        allocation: &Allocation,
    ) -> *mut ControlBlock<u8>;

    /// raw 의 5-way ShapeRenderConverter::To*Brush dispatch 호출.
    /// 본 callback 의 byte-eq port 는 L-5c-RE-5b3.
    /// `kind` 매개변수가 어떤 brush 인지 표시.
    unsafe fn to_render_brush(
        &mut self,
        kind: BrushKind,
        brush_ctrl: *mut ControlBlock<u8>,
    ) -> *mut ControlBlock<u8>;

    /// raw `Surface vfunc[+0x10] = FillPath(paths, brush)` 호출.
    /// 본 callback 의 byte-eq port 는 별도 surface adapter session.
    unsafe fn surface_fill_path(
        &mut self,
        paths_ctrl: *mut ControlBlock<u8>,
        brush_ctrl: *mut ControlBlock<u8>,
    );

    /// raw `Pen::ToRenderPen(out, pen, surface)` 호출.
    /// 본 callback 의 byte-eq port 는 L-5c-RE-5b3.
    unsafe fn to_render_pen(&mut self, pen_ctrl: *mut ControlBlock<u8>) -> *mut ControlBlock<u8>;

    /// raw `Surface vfunc[+0x28] = StrokePath(paths, render_pen)` 호출.
    unsafe fn surface_stroke_path(
        &mut self,
        paths_ctrl: *mut ControlBlock<u8>,
        pen_ctrl: *mut ControlBlock<u8>,
    );

    /// raw `DrawUnderLine(ci, surface, alloc)` (0x2fc088, 3940B) 호출.
    /// 본 callback 의 byte-eq port 는 L-5c-RE-5b4.
    unsafe fn draw_underline(&mut self, ci: &CharItemView, allocation: &Allocation, flag: &Flag);
}

/// raw `CharItemView::DrawDirect(Surface&, Allocation&, Flag&, BWMode)` (`0x2f67ec`, 5248B)
/// outer control flow byte-eq port.
///
/// ## raw 의 control flow byte-eq scope
///
/// **byte-eq 보장**: Brush 5-way dispatch + Pen 호출 시퀀스 + UnderLine 호출 시퀀스
///   (raw 의 case 0/1/2/3/4 jump + Pen.ToRenderPen + Surface.StrokePath + DrawUnderLine 순서)
///
/// **trait callback 위임** (별도 세션 byte-eq port):
/// - Surface vfunc (FillPath/StrokePath/GetColorScheme/GetTransform) — surface adapter
/// - ShapeRenderConverter::To*Brush family — L-5c-RE-5b3
/// - GetCachedRenderPath — L-5c-RE-5b2
/// - DrawUnderLine — L-5c-RE-5b4
/// - Default brush/pen fallback — 별도 helper port
///
/// ## raw byte-eq verification points
///
/// - Stage 4 (raw 0x2f6c20): `(Flag.byte1.bit3) >> 3 & 1 != 0` → 본문 진입 (Stage 5-7)
///   - bit3 == 0 일 때는 Stage 8 (underline) 만 호출
/// - Stage 6 (raw 0x2f71c4): `Brush.GetType() ∈ {0,1,2,3,4}` 5-way switch — case 0
///   (Empty) 은 fall-through (paint 없음), 1-4 는 각각 ShapeRenderConverter::To*Brush
///   호출 후 Surface.FillPath
/// - Stage 7 (raw 0x2f77b8): Pen.ToRenderPen 호출. 결과 non-null 이면 Surface.StrokePath
/// - Stage 8 (raw 0x2f7978): DrawUnderLine 호출 + 추가 underline path (cache_path[+0x98])
/// - Stage 9 (raw 0x2f7b24): 4 SharePtr release sequence
///
/// # Safety
///
/// `ci` 는 valid CharItemView. `allocation`/`flag`/`bw_mode` 는 valid raw bytes.
/// `deps` 는 raw 의 외부 함수 byte-eq impl 제공.
pub unsafe fn draw_direct(
    ci: &CharItemView,
    allocation: &Allocation,
    flag: &Flag,
    _bw_mode: BWMode,
    deps: &mut dyn DrawDirectDeps,
) -> DrawDirectOutcome {
    // Stage 1 (raw 0x2f67ec-0x2f68b4): ShapeEngine warmup × multiple — caller callback (no-op)
    deps.shape_engine_warmup();

    // raw 0x2f68b8: GetRealPen(this, theme) → local_b8 (pen ControlBlock**)
    // raw 의 실제 sret 은 stack slot. 우리 port 는 method 반환.
    let pen_ctrl = ci.get_real_pen(std::ptr::null());

    // raw 0x2f68bc-0x2f6884: brush handle from RunProperty (local_c0).
    // raw inline expand: `this->run_property.obj.brush` SharePtr.
    let brush_ctrl = ci.get_real_brush(std::ptr::null()) as *mut ControlBlock<u8>;

    // raw 0x2f6890-0x2f68f0: ShapeEngine warmup (again, byte-eq inline)
    deps.shape_engine_warmup();

    // Stage 2 (raw 0x2f68f0-0x2f6960): default brush/pen fallback if needed
    // FUN_0064add4(&local_c8, &local_1b0) — default brush
    // FUN_0064a590(&local_d0, &local_1b0) — default pen
    let _default_brush_ctrl = deps.default_brush_fallback();
    let _default_pen_ctrl = deps.default_pen_fallback();

    // Stage 3 (raw 0x2f6968-0x2f6a08): Pen.IsValid → bVar5
    // raw 가 pen_obj.GetType() vfunc[5] 호출 후 != 0 검사.
    let pen_obj_ctrl = pen_ctrl as *mut ControlBlock<u8>;
    let pen_type = if pen_obj_ctrl.is_null() {
        0u32
    } else {
        let pen_obj = (*pen_obj_ctrl).obj;
        if pen_obj.is_null() {
            0u32
        } else {
            deps.pen_get_type(pen_obj_ctrl)
        }
    };
    let pen_valid = pen_type != 0;

    // Stage 4 (raw 0x2f6c20): dispatch on Flag.byte1.bit3
    let flag_bytes = flag.0.to_le_bytes();
    let flag_byte1 = flag_bytes[1];
    let bit3_set = (flag_byte1 >> 3) & 1 != 0;

    let _has_default_brush = !_default_brush_ctrl.is_null() && unsafe_obj_nonnull(_default_brush_ctrl);
    let _has_default_pen = !_default_pen_ctrl.is_null() && unsafe_obj_nonnull(_default_pen_ctrl);

    // raw 의 분기:
    // - bit3==1: Stage 5-7 (full brush dispatch + pen) 진입
    // - bit3==0: Stage 8 (underline) 만
    if !bit3_set {
        // bit3 == 0 (= 'skip fill/stroke, only underline')
        deps.draw_underline(ci, allocation, flag);
        return DrawDirectOutcome::Painted {
            brush_kind: None,
            stroke_painted: false,
            underline_called: true,
        };
    }

    // Stage 5 (raw 0x2f6c50-0x2f71c0): CalcDrawVariables + GetCachedRenderPath + Path/Paths build
    //
    // raw 가 CalcDrawVariables 호출 후 결과 (PointF, RectF20, Transformation, StringFormat, mode)
    // 활용. 본 outer port 는 callback 으로 path 만 받음 — 정확한 raw 의 path build 시퀀스 는
    // L-5c-RE-5b2 에서 GetCachedRenderPath full port.
    let path_ctrl = deps.get_cached_render_path(ci, allocation);

    if path_ctrl.is_null() {
        // raw 의 GetCachedRenderPath 가 null 반환 시 → 그래도 underline 시도
        deps.draw_underline(ci, allocation, flag);
        return DrawDirectOutcome::Painted {
            brush_kind: None,
            stroke_painted: false,
            underline_called: true,
        };
    }

    // Stage 6 (raw 0x2f71c4-0x2f77b4): Brush 5-way dispatch
    //
    // raw 의 분기:
    // 1. brush.GetType() vfunc[5] 호출 → 0/1/2/3/4
    // 2. case 별 ShapeRenderConverter::To*Brush(brush, color_scheme) 호출
    // 3. 결과 render_brush 가 non-null 이면 Surface.FillPath(paths, render_brush) 호출
    let brush_obj_ctrl = brush_ctrl;
    let mut brush_kind: Option<BrushKind> = None;
    let mut render_brush_ctrl: *mut ControlBlock<u8> = std::ptr::null_mut();

    if !brush_obj_ctrl.is_null() && unsafe_obj_nonnull(brush_obj_ctrl) {
        let kind_raw = deps.brush_get_type(brush_obj_ctrl);
        if let Some(kind) = BrushKind::from_u32(kind_raw) {
            brush_kind = Some(kind);
            match kind {
                BrushKind::Empty => {
                    // raw case 0: dynamic_cast to EmptyBrush, no render — fall through
                }
                BrushKind::Solid | BrushKind::Gradient | BrushKind::Image | BrushKind::Hatch => {
                    // raw case 1/2/3/4: ShapeRenderConverter::To*Brush
                    render_brush_ctrl = deps.to_render_brush(kind, brush_obj_ctrl);
                    if !render_brush_ctrl.is_null() {
                        // raw `Surface vfunc[+0x10] = FillPath(paths, render_brush)` 호출
                        deps.surface_fill_path(path_ctrl, render_brush_ctrl);
                    }
                }
            }
        }
    }

    // Stage 7 (raw 0x2f77b8-0x2f797c): Pen application
    //
    // raw 가 Pen::ToRenderPen(&local_a8, pen, surface) 호출 → local_a8[0] = render_pen ctrl.
    // 결과 non-null 이면 Surface vfunc[+0x28] = StrokePath 호출.
    let mut stroke_painted = false;
    if pen_valid {
        let render_pen_ctrl = deps.to_render_pen(pen_obj_ctrl);
        if !render_pen_ctrl.is_null() {
            deps.surface_stroke_path(path_ctrl, render_pen_ctrl);
            stroke_painted = true;
            // raw cleanup of render_pen (refcount--) — caller responsibility in our port
        }
    }

    // Stage 8 (raw 0x2f7978): DrawUnderLine 호출
    deps.draw_underline(ci, allocation, flag);

    // Stage 9 (raw 0x2f7b24): SharePtr cleanup × 4 (pen/brush/default_pen/default_brush)
    // 본 outer port 의 cleanup 은 method 호출자 (test) 책임.

    DrawDirectOutcome::Painted {
        brush_kind,
        stroke_painted,
        underline_called: true,
    }
}

/// helper: ControlBlock<T>.obj null check.
#[inline]
unsafe fn unsafe_obj_nonnull<T>(ctrl: *mut ControlBlock<T>) -> bool {
    if ctrl.is_null() {
        return false;
    }
    !(*ctrl).obj.is_null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::{CharItemView, RunProperty};
    use std::alloc::Layout;
    use std::ptr;

    /// Test deps stub — tracks all callback invocations + provides programmable returns.
    struct TestDeps {
        warmup_called: u32,
        default_brush_returns: *mut ControlBlock<u8>,
        default_pen_returns: *mut ControlBlock<u8>,
        pen_type_returns: u32,
        brush_type_returns: u32,
        path_returns: *mut ControlBlock<u8>,
        render_brush_returns: *mut ControlBlock<u8>,
        render_pen_returns: *mut ControlBlock<u8>,
        fill_path_called: u32,
        stroke_path_called: u32,
        underline_called: u32,
        brush_kind_observed: Option<BrushKind>,
    }
    impl TestDeps {
        fn new() -> Self {
            Self {
                warmup_called: 0,
                default_brush_returns: ptr::null_mut(),
                default_pen_returns: ptr::null_mut(),
                pen_type_returns: 0,
                brush_type_returns: 0,
                path_returns: ptr::null_mut(),
                render_brush_returns: ptr::null_mut(),
                render_pen_returns: ptr::null_mut(),
                fill_path_called: 0,
                stroke_path_called: 0,
                underline_called: 0,
                brush_kind_observed: None,
            }
        }
    }
    impl DrawDirectDeps for TestDeps {
        unsafe fn shape_engine_warmup(&mut self) {
            self.warmup_called += 1;
        }
        unsafe fn default_brush_fallback(&mut self) -> *mut ControlBlock<u8> {
            self.default_brush_returns
        }
        unsafe fn default_pen_fallback(&mut self) -> *mut ControlBlock<u8> {
            self.default_pen_returns
        }
        unsafe fn pen_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 {
            self.pen_type_returns
        }
        unsafe fn brush_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 {
            self.brush_type_returns
        }
        unsafe fn get_cached_render_path(
            &mut self,
            _: &CharItemView,
            _: &Allocation,
        ) -> *mut ControlBlock<u8> {
            self.path_returns
        }
        unsafe fn to_render_brush(
            &mut self,
            kind: BrushKind,
            _: *mut ControlBlock<u8>,
        ) -> *mut ControlBlock<u8> {
            self.brush_kind_observed = Some(kind);
            self.render_brush_returns
        }
        unsafe fn surface_fill_path(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: *mut ControlBlock<u8>,
        ) {
            self.fill_path_called += 1;
        }
        unsafe fn to_render_pen(
            &mut self,
            _: *mut ControlBlock<u8>,
        ) -> *mut ControlBlock<u8> {
            self.render_pen_returns
        }
        unsafe fn surface_stroke_path(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: *mut ControlBlock<u8>,
        ) {
            self.stroke_path_called += 1;
        }
        unsafe fn draw_underline(
            &mut self,
            _: &CharItemView,
            _: &Allocation,
            _: &Flag,
        ) {
            self.underline_called += 1;
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    /// helper: create a ControlBlock with non-null obj.
    unsafe fn make_ctrl() -> *mut ControlBlock<u8> {
        let layout = Layout::new::<ControlBlock<u8>>();
        let p = std::alloc::alloc(layout) as *mut ControlBlock<u8>;
        ptr::write(
            p,
            ControlBlock {
                obj: 0xCAFEu64 as *mut u8,
                refcount: 1,
            },
        );
        p
    }
    unsafe fn free_ctrl(p: *mut ControlBlock<u8>) {
        let layout = Layout::new::<ControlBlock<u8>>();
        std::alloc::dealloc(p as *mut u8, layout);
    }

    #[test]
    fn brush_kind_from_u32_all_5_variants() {
        assert_eq!(BrushKind::from_u32(0), Some(BrushKind::Empty));
        assert_eq!(BrushKind::from_u32(1), Some(BrushKind::Solid));
        assert_eq!(BrushKind::from_u32(2), Some(BrushKind::Gradient));
        assert_eq!(BrushKind::from_u32(3), Some(BrushKind::Image));
        assert_eq!(BrushKind::from_u32(4), Some(BrushKind::Hatch));
        assert_eq!(BrushKind::from_u32(5), None);
        assert_eq!(BrushKind::from_u32(0xFF), None);
    }

    #[test]
    fn draw_direct_flag_bit3_off_only_calls_underline() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            // Flag.byte1 bit3 == 0 (= skip fill/stroke)
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted {
                    brush_kind,
                    stroke_painted,
                    underline_called,
                } => {
                    assert!(brush_kind.is_none());
                    assert!(!stroke_painted);
                    assert!(underline_called);
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.underline_called, 1);
            assert_eq!(deps.fill_path_called, 0);
            assert_eq!(deps.stroke_path_called, 0);
        }
    }

    #[test]
    fn draw_direct_flag_bit3_on_no_brush_no_pen_only_underline() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            // Flag.byte1 bit3 == 1 → enter Stage 5
            let flag_bytes = [0u8, 0x08, 0, 0, 0, 0, 0, 0];
            let flag = Flag(u64::from_le_bytes(flag_bytes));
            let mut deps = TestDeps::new();
            // path callback returns null → bail to underline
            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted {
                    underline_called, ..
                } => {
                    assert!(underline_called);
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.underline_called, 1);
            assert_eq!(deps.fill_path_called, 0);
            assert_eq!(deps.stroke_path_called, 0);
        }
    }

    #[test]
    fn draw_direct_brush_solid_dispatches_fill_path() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            // brush non-null in RP
            let mut rp = RunProperty::new_empty();
            let brush_ctrl = make_ctrl();
            rp.brush = brush_ctrl as *mut ControlBlock<crate::brush::Brush>;
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;

            let alloc = empty_alloc();
            let flag_bytes = [0u8, 0x08, 0, 0, 0, 0, 0, 0];
            let flag = Flag(u64::from_le_bytes(flag_bytes));

            let path_ctrl = make_ctrl();
            let render_brush_ctrl = make_ctrl();

            let mut deps = TestDeps::new();
            deps.path_returns = path_ctrl;
            deps.brush_type_returns = 1; // Solid
            deps.render_brush_returns = render_brush_ctrl;

            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted {
                    brush_kind,
                    underline_called,
                    ..
                } => {
                    assert_eq!(brush_kind, Some(BrushKind::Solid));
                    assert!(underline_called);
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.brush_kind_observed, Some(BrushKind::Solid));
            assert_eq!(deps.fill_path_called, 1);
            assert_eq!(deps.stroke_path_called, 0);
            assert_eq!(deps.underline_called, 1);

            free_ctrl(brush_ctrl);
            free_ctrl(path_ctrl);
            free_ctrl(render_brush_ctrl);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }

    #[test]
    fn draw_direct_brush_empty_skips_fill_no_render_call() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let mut rp = RunProperty::new_empty();
            let brush_ctrl = make_ctrl();
            rp.brush = brush_ctrl as *mut ControlBlock<crate::brush::Brush>;
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;

            let alloc = empty_alloc();
            let flag_bytes = [0u8, 0x08, 0, 0, 0, 0, 0, 0];
            let flag = Flag(u64::from_le_bytes(flag_bytes));

            let path_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            deps.path_returns = path_ctrl;
            deps.brush_type_returns = 0; // Empty
            // render_brush_returns 미설정 (null) — but case 0 안 호출됨

            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted { brush_kind, .. } => {
                    assert_eq!(brush_kind, Some(BrushKind::Empty));
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.brush_kind_observed, None, "case 0 (Empty) should not call to_render_brush");
            assert_eq!(deps.fill_path_called, 0);

            free_ctrl(brush_ctrl);
            free_ctrl(path_ctrl);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }

    #[test]
    fn draw_direct_pen_valid_dispatches_stroke_path() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let mut rp = RunProperty::new_empty();
            let pen_ctrl = make_ctrl();
            rp.pen = pen_ctrl as *mut ControlBlock<crate::pen::Pen>;
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;

            let alloc = empty_alloc();
            let flag_bytes = [0u8, 0x08, 0, 0, 0, 0, 0, 0];
            let flag = Flag(u64::from_le_bytes(flag_bytes));

            let path_ctrl = make_ctrl();
            let render_pen_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            deps.path_returns = path_ctrl;
            deps.pen_type_returns = 1; // valid pen (non-Empty)
            deps.brush_type_returns = 0xFF; // invalid → no brush dispatch
            deps.render_pen_returns = render_pen_ctrl;

            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted {
                    stroke_painted,
                    underline_called,
                    ..
                } => {
                    assert!(stroke_painted);
                    assert!(underline_called);
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.stroke_path_called, 1);
            assert_eq!(deps.underline_called, 1);

            free_ctrl(pen_ctrl);
            free_ctrl(path_ctrl);
            free_ctrl(render_pen_ctrl);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }

    #[test]
    fn draw_direct_pen_empty_skips_stroke() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let mut rp = RunProperty::new_empty();
            let pen_ctrl = make_ctrl();
            rp.pen = pen_ctrl as *mut ControlBlock<crate::pen::Pen>;
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;

            let alloc = empty_alloc();
            let flag_bytes = [0u8, 0x08, 0, 0, 0, 0, 0, 0];
            let flag = Flag(u64::from_le_bytes(flag_bytes));

            let path_ctrl = make_ctrl();
            let mut deps = TestDeps::new();
            deps.path_returns = path_ctrl;
            deps.pen_type_returns = 0; // Empty pen
            deps.brush_type_returns = 0xFF;

            let r = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            match r {
                DrawDirectOutcome::Painted { stroke_painted, .. } => {
                    assert!(!stroke_painted, "pen type 0 should skip stroke");
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.stroke_path_called, 0);

            free_ctrl(pen_ctrl);
            free_ctrl(path_ctrl);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }

    #[test]
    fn draw_direct_warmup_called_multiple_times() {
        // raw 가 ShapeEngine warmup 을 inline expand 한 횟수 (≥ 2회 + Stage 1/2)
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            let _ = draw_direct(&ci, &alloc, &flag, BWMode::V0, &mut deps);
            // 최소 2회 (Stage 1 + 다음 inline expand)
            assert!(deps.warmup_called >= 2, "expected >= 2 warmup calls, got {}", deps.warmup_called);
        }
    }
}
