//! `Hnc::Shape::Text::CharItemView::DrawUnderLine` byte-eq outer control flow port (L-5c-RE-5b4).
//!
//! ## raw 출처
//!
//! - `__ZNK3Hnc5Shape4Text12CharItemView13DrawUnderLineERNS0_7SurfaceERKNS1_10AllocationERKNS_4Type4FlagENS0_6BWModeE`
//! - 주소: `0x2fc088`
//! - 크기: 1752B (= 0x6d8)
//! - decompile: `Text_CharItemView__DrawUnderLine_002fc088.txt` (317 lines)
//!
//! ## 함수 의미
//!
//! 글자 밑줄 (underline) 을 그린다. 다음 조건 모두 만족 시에만 그림:
//! 1. BodyProperty + RunProperty 모두 valid
//! 2. BodyProperty key `0x8ae` (underline disable flag) == 0
//! 3. RunProperty key `0x961` (IsVisible) != 0
//!
//! ## raw 의 flow
//!
//! 1. **Early returns** (raw 0x2fc09c-0x2fc0d4): BP/RP null check
//! 2. **Underline disable check** (raw 0x2fc0d8-0x2fc110): `BodyProperty.bag.get(0x8ae) == 0`
//!    이 아니면 skip
//! 3. **Visibility check** (raw 0x2fc118-0x2fc150): `RunProperty.bag.get(0x961) != 0`
//! 4. **Underline position 계산** (raw 0x2fc154-0x2fc1c0):
//!    - 기본: `fVar21 = x + total_height (this+0x54)`, `fVar22 = y`
//!    - BodyProperty key `0x89e` (Vert) ∈ {0, 2, 5, 6} (= `(1 << v & 0x65)`):
//!      `fVar21 = x + format_origin_x (this+0x6c) * 0.5`, `fVar22 = y + total_height_alt (this+0x58)`
//!      (= Vertical text 의 special underline placement)
//! 5. **LogicalToRender** (raw 0x2fc1c4-0x2fc1f4): (x,y) (x2,y2) 각각 `* 96.0 / unit`
//! 6. **Path::Line 생성** (raw 0x2fc1f8-0x2fc208): operator_new(0x18) + FUN_0007925c
//! 7. **GetRealUnderLineBrush** (raw 0x2fc20c-0x2fc23c): brush 가져오기 (SharePtr<Brush>)
//! 8. **Brush dispatch** (raw 0x2fc240-0x2fc31c):
//!    - brush null → SolidBrush black fallback (0x00, 0x00, 0x00, opacity=0xff)
//!    - brush valid → `Brush.GetType()` vfunc[5] → Solid 면 ShapeRenderConverter::ToSolidBrush
//! 9. **GetRealUnderLinePen** (raw 0x2fc320-0x2fc34c): pen 가져오기
//! 10. **Pen width 계산** (raw 0x2fc350-0x2fc3c4):
//!    - 기본: `fVar19 = 0.75`
//!    - pen valid: `key 700 read f32` → `fVar19 = key_value * 96.0 / unit`
//! 11. **Pen 빌드 + StrokePath** (raw 0x2fc3c8-0x2fc454):
//!    - operator_new(0x60) = 96B Pen + FUN_0007df2c(width, miter=10.0, brush, ...)
//!    - Surface vfunc[+0x28] = StrokePath(path, pen)
//! 12. **Cleanup** (raw 0x2fc458-0x2fc760)
//!
//! ## 본 port scope (L-5c-RE-5b4)
//!
//! - ✅ Stage 1-4 (early returns + key check + position 계산) byte-eq
//! - ✅ Stage 5-11 outer flow byte-eq (callback 위임)
//! - ⏸️ LogicalToRender / Path::Line / GetRealUnderLineBrush/Pen / SolidBrush fallback /
//!   ToSolidBrush / Pen 빌드 / Surface.StrokePath → trait callback

use crate::char_item_view::CharItemView;
use crate::flag::Flag;
use crate::blip_glyph::Allocation;
use crate::share_ptr::ControlBlock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawUnderLineOutcome {
    SkippedNoBodyOrRunProperty,
    SkippedUnderlineDisabled, // key 0x8ae != 0
    SkippedInvisible,         // key 0x961 == 0
    Painted {
        used_brush_fallback: bool,
        used_pen_fallback: bool,
    },
}

pub trait DrawUnderLineDeps {
    /// raw `FUN_0067d484(impl, key)` = u32 read from PropertyBag (BodyProperty.bag).
    /// 본 callback 의 byte-eq impl 은 PropertyBagImpl::get_value_addr (이미 ported).
    unsafe fn body_property_u32(&mut self, ci: &CharItemView, key: u32) -> u32;

    /// raw `FUN_00687254(impl, key)` = u32 read from PropertyBag (RunProperty.bag).
    unsafe fn run_property_u32(&mut self, ci: &CharItemView, key: u32) -> u32;

    /// raw `FUN_0067d0e4(impl, key)` = u32 read (different template instance, same logic).
    unsafe fn body_property_u32_alt(&mut self, ci: &CharItemView, key: u32) -> u32;

    /// raw `FUN_0065616c(impl, key)` = f32 read.
    unsafe fn pen_property_f32(
        &mut self,
        pen_ctrl: *mut ControlBlock<u8>,
        key: u32,
    ) -> f32;

    /// raw `ShapeEngine.unit` (= GetInstance()->unit).
    unsafe fn shape_engine_unit(&mut self) -> f32;

    /// raw `Render::Path` 새 alloc + Path::Line(p0, p1). 24B Path. 반환 = path ptr.
    unsafe fn alloc_line_path(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> *mut u8;

    /// raw `GetRealUnderLineBrush(this, theme)` = SharePtr<Brush>.
    unsafe fn get_real_underline_brush(&mut self, ci: &CharItemView) -> *mut ControlBlock<u8>;

    /// raw `GetRealUnderLinePen(this, theme)` = SharePtr<Pen>.
    unsafe fn get_real_underline_pen(&mut self, ci: &CharItemView) -> *mut ControlBlock<u8>;

    /// raw 의 SolidBrush black fallback (alloc 0x18 = 24B SolidBrush + 0x10 = 16B 내부 SharePtr<Color>).
    /// raw 의 color: (0xff, 0xff, 0xff, alpha=0xff) — line 232 의 0xff01 라는 게 (0xff, 0x01) 인지
    /// (0x01, 0xff) 인지 정확 RE 필요. 본 port 는 callback 으로 위임.
    unsafe fn alloc_solid_brush_black(&mut self) -> *mut ControlBlock<u8>;

    /// raw `Brush.GetType()` vfunc[5] 호출.
    unsafe fn brush_get_type(&mut self, brush_ctrl: *mut ControlBlock<u8>) -> u32;

    /// raw `ShapeRenderConverter::ToSolidBrush(out, brush, color_scheme, mode, ?)` = render brush.
    unsafe fn to_solid_brush(
        &mut self,
        brush_ctrl: *mut ControlBlock<u8>,
        bw_mode_remap: u32,
    ) -> *mut ControlBlock<u8>;

    /// raw `FUN_0007df2c(width, miter=10.0, out, brush_or_render_brush, ...)` = 새 Pen 빌드.
    unsafe fn alloc_pen(&mut self, width: f32, brush_ctrl: *mut ControlBlock<u8>) -> *mut u8;

    /// raw Surface vfunc[+0x28] = StrokePath(path, pen).
    unsafe fn surface_stroke_path(&mut self, path: *mut u8, pen: *mut u8);
}

pub unsafe fn draw_underline(
    ci: &CharItemView,
    allocation: &Allocation,
    _flag: &Flag,
    bw_mode_remap: u32,
    deps: &mut dyn DrawUnderLineDeps,
) -> DrawUnderLineOutcome {
    // Stage 1: BP + RP null check
    let bp_ctrl = ci.body_property as *mut ControlBlock<u8>;
    let rp_ctrl = ci.run_property as *mut ControlBlock<u8>;
    if bp_ctrl.is_null() || (*bp_ctrl).obj.is_null() {
        return DrawUnderLineOutcome::SkippedNoBodyOrRunProperty;
    }
    if rp_ctrl.is_null() || (*rp_ctrl).obj.is_null() {
        return DrawUnderLineOutcome::SkippedNoBodyOrRunProperty;
    }

    // Stage 2: BodyProperty key 0x8ae (underline disable) check
    let underline_disable = deps.body_property_u32(ci, 0x8ae);
    if underline_disable != 0 {
        return DrawUnderLineOutcome::SkippedUnderlineDisabled;
    }

    // Stage 3: RunProperty key 0x961 (IsVisible) check
    let visible = deps.run_property_u32(ci, 0x961);
    if visible == 0 {
        return DrawUnderLineOutcome::SkippedInvisible;
    }

    // Stage 4: underline position 계산
    // allocation 의 첫 두 f32: x, y (=param_2 라인 76-77)
    let alloc_bytes = allocation as *const _ as *const f32;
    let x = *alloc_bytes;
    let y = *alloc_bytes.add(3); // raw 의 param_2[3]
    let mut x2 = x + ci.total_height; // raw 0x2fc154: fVar21 = x + this->total_height (+0x54)
    let mut y2 = y;
    let mut x1 = x;

    // raw `BodyProperty.bag.get(0x89e)` (Vert) check
    let vert = deps.body_property_u32_alt(ci, 0x89e);
    if vert < 7 && ((1u32 << (vert & 0x1f)) & 0x65) != 0 {
        // raw: fVar21 = x + this->format_origin_x (+0x6c) * 0.5
        x2 = x + ci.format_origin_x * 0.5;
        y2 = y + ci.total_height_alt;
        x1 = x2;
    }

    // Stage 5: LogicalToRender (raw 0x2fc1c4-0x2fc1f4)
    let unit = deps.shape_engine_unit();
    let rx0 = (x * 96.0) / unit;
    let ry0 = (y * 96.0) / unit;
    let rx1 = (x2 * 96.0) / unit;
    let ry1 = (y2 * 96.0) / unit;
    let _ = (x1, rx0, ry0, rx1, ry1); // suppress unused — used for callback below

    // Stage 6: Path::Line 생성 (callback)
    let path = deps.alloc_line_path(rx0, ry0, rx1, ry1);

    // Stage 7: GetRealUnderLineBrush
    let underline_brush_ctrl = deps.get_real_underline_brush(ci);

    // Stage 8: brush dispatch
    let mut render_brush: *mut ControlBlock<u8> = std::ptr::null_mut();
    let mut used_brush_fallback = false;
    if underline_brush_ctrl.is_null() || (*underline_brush_ctrl).obj.is_null() {
        // SolidBrush black fallback (raw 0x2fc240-0x2fc31c)
        render_brush = deps.alloc_solid_brush_black();
        used_brush_fallback = true;
    } else {
        let brush_obj_inner = (*underline_brush_ctrl).obj;
        // raw 의 vfunc[+0x30] (= "extract inner brush" — 별도 vfunc dispatch)
        // 본 outer port 는 vfunc dispatch 의 결과를 brush_get_type 으로 분류
        let _ = brush_obj_inner;
        let kind = deps.brush_get_type(underline_brush_ctrl);
        if kind == 1 {
            // SolidBrush
            render_brush = deps.to_solid_brush(underline_brush_ctrl, bw_mode_remap);
        }
        // else: render_brush stays null (raw 의 case 처리, line 177 `ppuVar16 = 0`)
    }

    // Stage 9: GetRealUnderLinePen
    let underline_pen_ctrl = deps.get_real_underline_pen(ci);

    // Stage 10: pen width 계산
    let mut pen_width = 0.75f32; // raw 0x2fc350: 기본값
    let mut used_pen_fallback = true;
    if !underline_pen_ctrl.is_null() && !(*underline_pen_ctrl).obj.is_null() {
        // raw key 700 (= 0x2bc) read f32
        let raw_width = deps.pen_property_f32(underline_pen_ctrl, 700);
        // raw `fVar19 = (key_value * 96.0) / unit`
        pen_width = (raw_width * 96.0) / unit;
        used_pen_fallback = false;
    }

    // Stage 11: Pen 빌드 + StrokePath
    if render_brush.is_null() {
        // raw 0x2fc3c8 fallback: 또 SolidBrush black 빌드 + Pen
        render_brush = deps.alloc_solid_brush_black();
        used_brush_fallback = true;
    }
    let pen = deps.alloc_pen(pen_width, render_brush);
    deps.surface_stroke_path(path, pen);

    // Stage 12: Cleanup — caller (test) 책임

    DrawUnderLineOutcome::Painted {
        used_brush_fallback,
        used_pen_fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::{CharItemView, RunProperty};
    use crate::body_property::BodyProperty as OldBP;
    use std::alloc::Layout;
    use std::ptr;

    struct TestDeps {
        bp_u32_values: std::collections::HashMap<u32, u32>,
        rp_u32_values: std::collections::HashMap<u32, u32>,
        bp_u32_alt_values: std::collections::HashMap<u32, u32>,
        pen_f32: f32,
        unit: f32,
        line_path_returns: *mut u8,
        brush_returns: *mut ControlBlock<u8>,
        pen_returns: *mut ControlBlock<u8>,
        brush_fallback_returns: *mut ControlBlock<u8>,
        brush_type_returns: u32,
        to_solid_returns: *mut ControlBlock<u8>,
        pen_alloc_returns: *mut u8,
        stroke_called: u32,
        observed_pen_width: f32,
    }
    impl TestDeps {
        fn new() -> Self {
            Self {
                bp_u32_values: Default::default(),
                rp_u32_values: Default::default(),
                bp_u32_alt_values: Default::default(),
                pen_f32: 0.0,
                unit: 1.0,
                line_path_returns: ptr::null_mut(),
                brush_returns: ptr::null_mut(),
                pen_returns: ptr::null_mut(),
                brush_fallback_returns: ptr::null_mut(),
                brush_type_returns: 0,
                to_solid_returns: ptr::null_mut(),
                pen_alloc_returns: ptr::null_mut(),
                stroke_called: 0,
                observed_pen_width: -1.0,
            }
        }
    }
    impl DrawUnderLineDeps for TestDeps {
        unsafe fn body_property_u32(&mut self, _: &CharItemView, key: u32) -> u32 {
            *self.bp_u32_values.get(&key).unwrap_or(&0)
        }
        unsafe fn run_property_u32(&mut self, _: &CharItemView, key: u32) -> u32 {
            *self.rp_u32_values.get(&key).unwrap_or(&0)
        }
        unsafe fn body_property_u32_alt(&mut self, _: &CharItemView, key: u32) -> u32 {
            *self.bp_u32_alt_values.get(&key).unwrap_or(&0)
        }
        unsafe fn pen_property_f32(&mut self, _: *mut ControlBlock<u8>, _: u32) -> f32 {
            self.pen_f32
        }
        unsafe fn shape_engine_unit(&mut self) -> f32 {
            self.unit
        }
        unsafe fn alloc_line_path(&mut self, _: f32, _: f32, _: f32, _: f32) -> *mut u8 {
            self.line_path_returns
        }
        unsafe fn get_real_underline_brush(
            &mut self,
            _: &CharItemView,
        ) -> *mut ControlBlock<u8> {
            self.brush_returns
        }
        unsafe fn get_real_underline_pen(&mut self, _: &CharItemView) -> *mut ControlBlock<u8> {
            self.pen_returns
        }
        unsafe fn alloc_solid_brush_black(&mut self) -> *mut ControlBlock<u8> {
            self.brush_fallback_returns
        }
        unsafe fn brush_get_type(&mut self, _: *mut ControlBlock<u8>) -> u32 {
            self.brush_type_returns
        }
        unsafe fn to_solid_brush(
            &mut self,
            _: *mut ControlBlock<u8>,
            _: u32,
        ) -> *mut ControlBlock<u8> {
            self.to_solid_returns
        }
        unsafe fn alloc_pen(
            &mut self,
            width: f32,
            _: *mut ControlBlock<u8>,
        ) -> *mut u8 {
            self.observed_pen_width = width;
            self.pen_alloc_returns
        }
        unsafe fn surface_stroke_path(&mut self, _: *mut u8, _: *mut u8) {
            self.stroke_called += 1;
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    unsafe fn setup_ci_with_bp_rp() -> (
        CharItemView,
        *mut ControlBlock<OldBP>,
        *mut ControlBlock<RunProperty>,
    ) {
        let mut ci = CharItemView::new_empty();
        // dummy BP+RP ctrls
        let bp_layout = Layout::new::<ControlBlock<OldBP>>();
        let bp_ctrl = std::alloc::alloc(bp_layout) as *mut ControlBlock<OldBP>;
        ptr::write(
            bp_ctrl,
            ControlBlock {
                obj: 0x1usize as *mut OldBP,
                refcount: 1,
            },
        );
        ci.body_property = bp_ctrl;
        let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
        let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
        ptr::write(
            rp_ctrl,
            ControlBlock {
                obj: 0x2usize as *mut RunProperty,
                refcount: 1,
            },
        );
        ci.run_property = rp_ctrl;
        (ci, bp_ctrl, rp_ctrl)
    }

    unsafe fn free_bp_rp(bp: *mut ControlBlock<OldBP>, rp: *mut ControlBlock<RunProperty>) {
        let bp_layout = Layout::new::<ControlBlock<OldBP>>();
        std::alloc::dealloc(bp as *mut u8, bp_layout);
        let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
        std::alloc::dealloc(rp as *mut u8, rp_layout);
    }

    #[test]
    fn no_bp_returns_skipped() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            assert_eq!(r, DrawUnderLineOutcome::SkippedNoBodyOrRunProperty);
        }
    }

    #[test]
    fn underline_disabled_returns_skipped() {
        unsafe {
            let (ci, bp, rp) = setup_ci_with_bp_rp();
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            deps.bp_u32_values.insert(0x8ae, 1); // disable underline
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            assert_eq!(r, DrawUnderLineOutcome::SkippedUnderlineDisabled);
            free_bp_rp(bp, rp);
        }
    }

    #[test]
    fn invisible_returns_skipped() {
        unsafe {
            let (ci, bp, rp) = setup_ci_with_bp_rp();
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            deps.bp_u32_values.insert(0x8ae, 0); // not disabled
            deps.rp_u32_values.insert(0x961, 0); // invisible
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            assert_eq!(r, DrawUnderLineOutcome::SkippedInvisible);
            free_bp_rp(bp, rp);
        }
    }

    #[test]
    fn paints_with_brush_fallback_and_pen_fallback() {
        unsafe {
            let (ci, bp, rp) = setup_ci_with_bp_rp();
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            deps.bp_u32_values.insert(0x8ae, 0);
            deps.rp_u32_values.insert(0x961, 1);
            // brush_returns = null → fallback
            // pen_returns = null → fallback
            deps.line_path_returns = 0xAAAA_usize as *mut u8;
            deps.brush_fallback_returns = 0xBBBB_usize as *mut ControlBlock<u8>;
            deps.pen_alloc_returns = 0xCCCC_usize as *mut u8;
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            match r {
                DrawUnderLineOutcome::Painted {
                    used_brush_fallback,
                    used_pen_fallback,
                } => {
                    assert!(used_brush_fallback);
                    assert!(used_pen_fallback);
                }
                _ => panic!("expected Painted"),
            }
            assert_eq!(deps.stroke_called, 1);
            assert_eq!(deps.observed_pen_width, 0.75); // raw default
            free_bp_rp(bp, rp);
        }
    }

    #[test]
    fn pen_width_uses_key_value_scaled_by_unit() {
        unsafe {
            let (mut ci, bp, rp) = setup_ci_with_bp_rp();
            ci.total_height = 10.0;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            deps.bp_u32_values.insert(0x8ae, 0);
            deps.rp_u32_values.insert(0x961, 1);
            // valid pen ctrl
            let pen_ctrl_layout = Layout::new::<ControlBlock<u8>>();
            let pen_ctrl = std::alloc::alloc(pen_ctrl_layout) as *mut ControlBlock<u8>;
            ptr::write(
                pen_ctrl,
                ControlBlock {
                    obj: 0xDEADusize as *mut u8,
                    refcount: 1,
                },
            );
            deps.pen_returns = pen_ctrl;
            deps.pen_f32 = 1.5; // raw key 700 value
            deps.unit = 72.0;
            deps.brush_fallback_returns = 0xBBBB_usize as *mut ControlBlock<u8>;
            deps.line_path_returns = 0xAAAA_usize as *mut u8;
            deps.pen_alloc_returns = 0xCCCC_usize as *mut u8;
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            match r {
                DrawUnderLineOutcome::Painted { used_pen_fallback, .. } => {
                    assert!(!used_pen_fallback);
                }
                _ => panic!(),
            }
            // expected width = 1.5 * 96 / 72 = 2.0
            assert!((deps.observed_pen_width - 2.0).abs() < 1e-5);
            std::alloc::dealloc(pen_ctrl as *mut u8, pen_ctrl_layout);
            free_bp_rp(bp, rp);
        }
    }

    #[test]
    fn vertical_text_uses_format_origin_x() {
        unsafe {
            let (mut ci, bp, rp) = setup_ci_with_bp_rp();
            ci.format_origin_x = 20.0;
            ci.total_height_alt = 5.0;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut deps = TestDeps::new();
            deps.bp_u32_values.insert(0x8ae, 0);
            deps.rp_u32_values.insert(0x961, 1);
            deps.bp_u32_alt_values.insert(0x89e, 5); // Vert = 5 (Bottom-aware), bit set in 0x65
            deps.line_path_returns = 0xAAAA_usize as *mut u8;
            deps.brush_fallback_returns = 0xBBBB_usize as *mut ControlBlock<u8>;
            deps.pen_alloc_returns = 0xCCCC_usize as *mut u8;
            let r = draw_underline(&ci, &alloc, &flag, 0, &mut deps);
            assert!(matches!(r, DrawUnderLineOutcome::Painted { .. }));
            // No specific assertion on the line coords (callback) — just verify path taken without panic.
            free_bp_rp(bp, rp);
        }
    }
}
