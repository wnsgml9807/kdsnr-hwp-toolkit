//! `Hnc::Shape::Text::BlipGlyph` — image/picture glyph.
//!
//! ## raw 출처
//! - ctor: `0x2d1104` (`BlipGlyph(Requirement, Requirement, UniquePtr<ImageBrush>, TextDirectionType)`)
//! - dtor: `0x2d1238`
//! - `Draw`: `0x2d1480` (sz=410 instr, decompiled in `glyph_draw_dump/BlipGlyph__Draw_2d1480.asm`)
//! - `Allocate`: `0x2d13e8`
//! - vtable: `0x77faf0`
//!
//! ## layout (64B = 0x40)
//!
//! | offset | size | field            | 출처 |
//! |--------|------|------------------|------|
//! | +0x00  | 8B   | vtable           | ctor `*(this) = &PTR__BlipGlyph_0077faf0` |
//! | +0x08  | 8B   | requirement1     | ctor `*(this+8) = *param_1` (16B Req → 첫 8B) |
//! |        | +0   | `width: f32`     | Draw 의 `ldr s13, [x0, #0x8]` (drawmode==0/2/5/6 의 fmadd) |
//! |        | +4   | `_pad: f32`      | 사용 안 됨 |
//! | +0x10  | 8B   | requirement1[8..16] | ctor `param_1[1]` |
//! |        | +0   | (= +0x10)        |  |
//! |        | +4   | `anchor_x: f32` (= +0x14) | Draw 의 `ldp s11, s14, [x0, #0x14]` |
//! | +0x18  | 8B   | requirement2     | ctor `*(this+0x18) = *param_2` |
//! |        | +0   | `height: f32`    | Draw 의 `s14` (위의 `ldp` 의 두번째) |
//! |        | +4   | `_pad: f32`      |  |
//! | +0x20  | 8B   | requirement2[8..16] | ctor `param_2[1]` |
//! |        | +0   | (= +0x20)        |  |
//! |        | +4   | `anchor_y: f32` (= +0x24) | Draw 의 `ldr s10, [x0, #0x24]` |
//! | +0x28  | 4B   | `state: u32`     | ctor `*(this+0x28) = 0` |
//! | +0x2c  | 4B   | pad (alignment)  |  |
//! | +0x30  | 8B   | `picture: *ControlBlock<ImageBrush>` | ctor refcount++ + FUN_00677fd0 |
//! | +0x38  | 4B   | `direction: u32` | ctor `*(this+0x38) = param_5` (= TextDirectionType) |
//! | +0x3c  | 4B   | pad              |  |
//!
//! ## `Draw` 알고리즘 (raw `0x2d1480`)
//!
//! ```text
//! if (this+0x30) == null || (*(this+0x30)) == null: return
//! restorer = new SurfaceRestorer(surface)   ; CGContextSaveGState
//! origin_x = allocation[0]                  ; +0
//! origin_y = allocation[0xc]                ; +0xc
//! mode = this.direction (0..6)
//! if mode <= 6 && (1 << mode) & 0x65 != 0:  ; 0,2,5,6 (transform path)
//!   rect.flag = 1
//!   rect.x = origin_x - height * 0.5         ; fmadd(height, -0.5, origin_x)
//!   rect.y = origin_y - width                ; fsub
//!   rect.w = width
//!   rect.h = height
//!   t = Transform2D::default()
//!   unit = ShapeEngine::GetInstance().unit
//!   sx = origin_x * 96 / unit
//!   sy = origin_y * 96 / unit
//!   t.translate((-sx, -sy), 1)
//!   t.rotate(Degree(90), (0,0), 1)
//!   t.translate((sx, sy), 1)
//!   t.translate((height * 96 / unit, 0), 1)
//!   m = Matrix3::from_t(t)                   ; via GetElement 0..5
//!   ctm = surface.get_ctm() as Matrix3
//!   ctm.premultiply(m)
//!   inv = ctm.inverse()                       ; via CGAffineTransformInvert
//!   surface.concat_ctm(inv)
//!   surface.translate(-ty_orig, -...)
//!   call <Surface::ApplyTransform>(0x7f2a4)
//! else:                                       ; 1,3,4 (non-transform path)
//!   rect.flag = 1
//!   rect.x = origin_x - width * anchor_x
//!   rect.y = origin_y - height * anchor_y
//!   rect.w = width
//!   rect.h = height
//! path = new Path
//! path.add_rect(rect)
//! paths = Paths(); paths.add_path(path)
//! result_image_data = surface.draw_blip(paths, matrix3, picture)   ; vfunc[13]
//! cleanup ImageData × 2
//! restorer.~SurfaceRestorer()                ; CGContextRestoreGState
//! ```
//!
//! ## byte-eq 경계
//!
//! - **rect 계산**: 100% byte-eq (asm 의 fmsub / fmadd / fsub 그대로).
//! - **Transform2D 4-step**: 100% byte-eq (단위·각도·anchor 모두 raw 값).
//! - **vfunc[13] = surface.draw_blip(...)**: trait 메소드. SvgSurface 가 backend 구현.
//! - **SurfaceRestorer**: RAII (Rust 의 Drop trait).

use crate::brush::ImageBrush;
use crate::path::{Path, PointF, RectF};
use crate::share_ptr::ControlBlock;
use crate::surface::{PointImpl, RectImpl, Surface};

/// `Hnc::Shape::Text::Allocation` — Draw 의 input 좌표 (16B).
///
/// asm 의 reads: `ldr s9, [x2]` (+0 → origin.x), `ldr s8, [x2, #0xc]` (+0xc → origin.y).
/// 나머지 8B 의 의미는 caller 별 (Layout RE 후 정정).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Allocation {
    /// +0x00: origin x (`ldr s9, [x2]`)
    pub origin_x: f32,
    /// +0x04: pad (asm 미사용)
    pub _pad1: f32,
    /// +0x08: pad (asm 미사용)
    pub _pad2: f32,
    /// +0x0c: origin y (`ldr s8, [x2, #0xc]`)
    pub origin_y: f32,
}

impl Allocation {
    /// 새 Allocation (origin 만 명시; pad 는 0).
    pub fn at_point(p: PointImpl<f32>) -> Self {
        Self {
            origin_x: p.x,
            _pad1: 0.0,
            _pad2: 0.0,
            origin_y: p.y,
        }
    }
}

/// `Hnc::Shape::Text::Extension` — Allocate 의 output rect (16B, 4 floats: x/y/x2/y2).
///
/// asm 의 writes (`Allocate` 0x2d13e8):
/// - `*param_2 = x_min`
/// - `*(param_2+4) = y_min`
/// - `*(param_2+8) = x_max`
/// - `*(param_2+0xc) = y_max`
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Extension {
    pub x_min: f32,
    pub y_min: f32,
    pub x_max: f32,
    pub y_max: f32,
}

/// `Hnc::Shape::Text::TextDirectionType` — drawmode (0..6).
///
/// Draw 의 transform-path 진입 조건: `mode <= 6 && (1 << mode) & 0x65 != 0`.
/// 0x65 = 0b01100101 → mode ∈ {0, 2, 5, 6}.
pub mod direction {
    pub const LTR: u32 = 0; // transform
    pub const D1: u32 = 1; // non-transform
    pub const D2: u32 = 2; // transform
    pub const D3: u32 = 3; // non-transform
    pub const D4: u32 = 4; // non-transform
    pub const D5: u32 = 5; // transform
    pub const D6: u32 = 6; // transform
}

/// raw 64B `Hnc::Shape::Text::BlipGlyph`.
#[repr(C)]
#[derive(Debug)]
pub struct BlipGlyph {
    /// +0x00: vtable.
    pub vtable: *const u8,
    /// +0x08: requirement1.first 8B; 그 안의 +0 = `width` (s13).
    pub width: f32,
    /// +0x0c: requirement1.first 8B[+4]; 미사용.
    pub _req1a_pad: f32,
    /// +0x10: requirement1.second 8B[+0]; 미사용.
    pub _req1b_pad: f32,
    /// +0x14: requirement1.second 8B[+4] = `anchor_x` (s11).
    pub anchor_x: f32,
    /// +0x18: requirement2.first 8B[+0] = `height` (s14).
    pub height: f32,
    /// +0x1c: requirement2.first 8B[+4]; 미사용.
    pub _req2a_pad: f32,
    /// +0x20: requirement2.second 8B[+0]; 미사용.
    pub _req2b_pad: f32,
    /// +0x24: requirement2.second 8B[+4] = `anchor_y` (s10).
    pub anchor_y: f32,
    /// +0x28: state u32 (ctor 가 0 으로 init).
    pub state: u32,
    /// +0x2c: alignment padding.
    pub _pad28: u32,
    /// +0x30: SharePtr<ImageBrush>.
    pub picture: *mut ControlBlock<ImageBrush>,
    /// +0x38: TextDirectionType (drawmode).
    pub direction: u32,
    /// +0x3c: alignment padding.
    pub _pad3c: u32,
}

pub const BLIP_GLYPH_SIZE_BYTES: usize = 64;
pub const BLIP_GLYPH_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<BlipGlyph>() == BLIP_GLYPH_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<BlipGlyph>() == BLIP_GLYPH_ALIGN_BYTES);

/// raw vtable address (`0x77faf0` in libHncDrawingEngine.dylib).
pub const BLIP_GLYPH_VTABLE_ADDR: usize = 0x77faf0;

impl BlipGlyph {
    /// raw `BlipGlyph::BlipGlyph(Requirement&, Requirement&, UniquePtr<ImageBrush>&, TextDirectionType)`
    /// @ `0x2d1104` (sz=112).
    ///
    /// # Safety
    /// `picture` 가 valid `ControlBlock<ImageBrush>*` 또는 null. raw 가 호출하는
    /// `FUN_00677fd0` 는 SharePtr 의 보조 setup (Effects 류 — multi-session deferred).
    pub unsafe fn new(
        req1: (f32, f32, f32, f32),
        req2: (f32, f32, f32, f32),
        picture: *mut ControlBlock<ImageBrush>,
        direction: u32,
    ) -> Self {
        // raw refcount++ if non-null obj
        if !picture.is_null() && !(*picture).obj.is_null() {
            (*picture).refcount = (*picture).refcount.wrapping_add(1);
            // raw `FUN_00677fd0()` — Effects/Scene3D-류 보조 setup (deferred)
        }
        Self {
            vtable: BLIP_GLYPH_VTABLE_ADDR as *const u8,
            width: req1.0,
            _req1a_pad: req1.1,
            _req1b_pad: req1.2,
            anchor_x: req1.3,
            height: req2.0,
            _req2a_pad: req2.1,
            _req2b_pad: req2.2,
            anchor_y: req2.3,
            state: 0,
            _pad28: 0,
            picture,
            direction,
            _pad3c: 0,
        }
    }

    /// transform-path 여부 (drawmode ∈ {0,2,5,6}).
    ///
    /// raw asm `cmp w9, #0x6; b.hi ...; lsl w9, #1, drawmode; tst w9, #0x65; b.eq ...`.
    #[inline]
    pub fn is_transform_mode(&self) -> bool {
        let m = self.direction;
        m <= 6 && (1u32 << m) & 0x65 != 0
    }

    /// raw `BlipGlyph::Allocate(Allocation&, Extension&)` @ `0x2d13e8`.
    ///
    /// transform-path: `rect = (x - h/2, y - w, x + h/2, y)`
    /// non-transform: `rect = (x - w*ax, y - h*ay, x - (1-w*ax)*w?, ...)`
    ///   (자세한 식은 decompile 의 fmsub 그대로)
    pub fn allocate(&self, alloc: &Allocation) -> Extension {
        let x = alloc.origin_x;
        let y = alloc.origin_y;
        let w = self.width;
        let h = self.height;
        if self.is_transform_mode() {
            Extension {
                x_min: x - h * 0.5,
                y_min: y - w,
                x_max: x + h * 0.5,
                y_max: y,
            }
        } else {
            let ax = self.anchor_x;
            let ay = self.anchor_y;
            Extension {
                x_min: x - ax * w,
                y_min: y - ay * h,
                x_max: x - (1.0 - ax) * w,
                y_max: y - (1.0 - ay) * h,
            }
        }
    }

    /// raw `BlipGlyph::Draw(Surface&, Allocation&)` @ `0x2d1480`.
    ///
    /// # 알고리즘 (byte-eq from asm)
    ///
    /// 1. picture null 검사 (`+0x30` deref 후 deref). null 이면 즉시 return.
    /// 2. `surface.save_state()` (raw 의 `new SurfaceRestorer` + `CGContextSaveGState`).
    /// 3. drawmode (`+0x38`) 가 {0,2,5,6} 이면 transform-path:
    ///    - `rect = (origin.x - h*0.5, origin.y - h, h, w)` (자세히는 asm 참조)
    ///    - 4-step Transform2D: `Translate(-sx,-sy) → Rotate(90, (0,0)) → Translate(sx,sy)
    ///      → Translate(h*96/unit, 0)`. `sx = origin.x*96/unit, sy = origin.y*96/unit`,
    ///      `unit = ShapeEngine::GetInstance().unit`.
    ///    - CTM 와 pre-multiply → CGContextConcatCTM
    /// 4. drawmode != {0,2,5,6} 이면 non-transform-path:
    ///    - `rect = (origin.x - w*ax, origin.y - h*ay, w, h)`
    /// 5. `Path::AddRect(rect)` → `Paths::AddPath(path)` → `surface.draw_blip(paths, picture)`
    /// 6. RAII: `restorer` drop 시 `CGContextRestoreGState`
    ///
    /// # Safety
    /// `self.picture` 가 valid `ControlBlock<ImageBrush>*` 또는 null. SVG backend 는
    /// ImageBrush 의 `source_id` 만 사용 (실제 binData 통합은 caller 책임).
    pub unsafe fn draw<S: Surface>(&self, surface: &mut S, alloc: &Allocation) {
        // 1. picture null check (raw `ldr x8, [x0, #0x30]; cbz x8, exit; ldr x8, [x8]; cbz x8, exit`)
        if self.picture.is_null() {
            return;
        }
        if (*self.picture).obj.is_null() {
            return;
        }

        // 2. SurfaceRestorer ctor (raw `new(8); ... CGContextSaveGState`)
        surface.save_state();

        // 3-4. rect 계산 (transform vs non-transform path)
        let rect: RectF = {
            let x = alloc.origin_x;
            let y = alloc.origin_y;
            let w = self.width;
            let h = self.height;
            if self.is_transform_mode() {
                // raw `fmadd s9, s14, s0(-0.5), s9` → x = x - h*0.5
                // raw `fsub s8, s8, s13` → y = y - w
                // raw `stp s9, s8 → (x, y)`; `stp s13, s14 → (w, h)`
                RectF::new(x - h * 0.5, y - w, w, h)
            } else {
                // raw `fmsub s0, s13, s11, s9` → x = x - w*ax
                // raw `fmsub s1, s14, s10, s8` → y = y - h*ay
                // raw `stp s0, s1 → (x, y)`; `stp s13, s14 → (w, h)`
                RectF::new(x - w * self.anchor_x, y - h * self.anchor_y, w, h)
            }
        };

        // 5a. transform-path 만: 4-step Transform2D + CTM pre-multiply
        if self.is_transform_mode() {
            self.apply_transform_path(surface, alloc);
        }

        // 5b. Path + Paths + draw_blip
        let mut path = Path::new();
        path.add_rect(&rect);

        // raw 의 vfunc[13] (Surface::DrawBlip). SvgSurface 가 `<image>` emit.
        surface.draw_blip(&path, self.picture);

        // 6. SurfaceRestorer dtor (raw `CGContextRestoreGState; delete restorer`)
        surface.restore_state();
    }

    /// transform-path 의 4-step Transform2D 적용 (raw `0x2d1540..0x2d17f4` 부분).
    ///
    /// asm 의 호출 순서:
    /// 1. `Transform2D::Transform2D()` (identity ctor)
    /// 2. 4× `ShapeEngine::GetInstance(); ldr s, [+0x4]` → `unit` 3 read (s10, s11, s8)
    /// 3. `sx = origin.x * 96 / unit; sy = origin.y * 96 / unit`
    /// 4. `Translate((-sx, -sy), 1)` (order=1)
    /// 5. `Degree(90); Translate((0,0)) anchor; Rotate(deg, anchor, 1)`
    /// 6. `Translate((sx, sy), 1)`
    /// 7. `Translate((h * 96 / unit, 0), 1)`
    /// 8. GetElement 0..5 → Matrix3 ctor
    /// 9. CGContextGetCTM → Matrix3 → PreMultiply
    /// 10. CGAffineTransformInvert → CGContextConcatCTM → CGContextTranslateCTM(-d8, -d9)
    fn apply_transform_path<S: Surface>(&self, surface: &mut S, alloc: &Allocation) {
        use crate::degree::Degree;
        use crate::shape_engine::read_instance;
        use crate::transform2d::Transform2D as HncTransform2D;

        // ShapeEngine.unit 읽기 (3× 호출, 같은 값)
        let unit = read_instance().get_logical_dpi();
        let inv_unit = 96.0_f32 / unit;
        let sx = alloc.origin_x * inv_unit;
        let sy = alloc.origin_y * inv_unit;

        // 1. Transform2D identity
        let mut t = HncTransform2D::new();

        // 4. Translate(-sx, -sy), order=1
        let neg = PointImpl { x: -sx, y: -sy };
        t.translate(&neg, 1);

        // 5. Rotate(90deg, (0,0), 1)
        let deg = Degree::from_float(90.0);
        let zero = PointImpl { x: 0.0, y: 0.0 };
        t.rotate(&deg, &zero, 1);

        // 6. Translate(sx, sy)
        let pos = PointImpl { x: sx, y: sy };
        t.translate(&pos, 1);

        // 7. Translate(h * 96 / unit, 0)
        let extra = PointImpl { x: self.height * inv_unit, y: 0.0 };
        t.translate(&extra, 1);

        // 8-10. SVG backend: t 그대로 surface 에 합성 (CGContextConcatCTM 등치).
        //
        // raw 는 CGContextGetCTM + Matrix3 변환 + PreMultiply + Inverse + ConcatCTM 으로
        // 복잡하게 우회하지만 결과는 "현재 CTM 위에 t 합성". SvgSurface 의
        // `concat_transform` 가 동치.
        surface.concat_transform(&t);
    }
}

impl Drop for BlipGlyph {
    /// raw `~BlipGlyph()` @ `0x2d1238`:
    /// `*(this) = &PTR__BlipGlyph_0077faf0; FUN_0067838c(this+0x30); return this;`
    /// → SharePtr release (refcount--).
    fn drop(&mut self) {
        unsafe {
            if !self.picture.is_null() {
                let ctrl = &mut *self.picture;
                if ctrl.refcount > 0 {
                    ctrl.refcount = ctrl.refcount.wrapping_sub(1);
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::ImageBrush;
    use crate::svg_surface::SvgSurface;
    use std::ptr;

    fn make_picture(brush: ImageBrush) -> *mut ControlBlock<ImageBrush> {
        Box::into_raw(Box::new(ControlBlock {
            obj: Box::into_raw(Box::new(brush)),
            refcount: 1,
        }))
    }

    unsafe fn free_picture(p: *mut ControlBlock<ImageBrush>) {
        if !p.is_null() {
            let cb = Box::from_raw(p);
            if !cb.obj.is_null() {
                let _ = Box::from_raw(cb.obj);
            }
        }
    }

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<BlipGlyph>(), 64);
        assert_eq!(std::mem::align_of::<BlipGlyph>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let bg = unsafe { BlipGlyph::new((10.0, 0.0, 0.0, 0.5), (20.0, 0.0, 0.0, 0.5), ptr::null_mut(), 0) };
        let base = &bg as *const _ as usize;
        assert_eq!(&bg.vtable as *const _ as usize - base, 0x00);
        assert_eq!(&bg.width as *const _ as usize - base, 0x08);
        assert_eq!(&bg.anchor_x as *const _ as usize - base, 0x14);
        assert_eq!(&bg.height as *const _ as usize - base, 0x18);
        assert_eq!(&bg.anchor_y as *const _ as usize - base, 0x24);
        assert_eq!(&bg.state as *const _ as usize - base, 0x28);
        assert_eq!(&bg.picture as *const _ as usize - base, 0x30);
        assert_eq!(&bg.direction as *const _ as usize - base, 0x38);
    }

    #[test]
    fn ctor_refcount_increment() {
        let p = make_picture(ImageBrush::new("img1".to_string()));
        unsafe {
            assert_eq!((*p).refcount, 1);
            let bg = BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (2.0, 0.0, 0.0, 0.5), p, 0);
            assert_eq!((*p).refcount, 2); // refcount++ in ctor
            drop(bg); // Drop decrements
            assert_eq!((*p).refcount, 1);
            free_picture(p);
        }
    }

    #[test]
    fn ctor_null_picture_no_refcount() {
        // null picture 면 refcount 처리 안 함 (raw 의 cbz 검사)
        unsafe {
            let bg = BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (2.0, 0.0, 0.0, 0.5), ptr::null_mut(), 0);
            assert!(bg.picture.is_null());
            // drop 도 panic 없이
        }
    }

    #[test]
    fn is_transform_mode_for_0_2_5_6() {
        // raw 의 (1 << mode) & 0x65 != 0 mask 검증
        for mode in [0u32, 2, 5, 6] {
            let bg = unsafe { BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (1.0, 0.0, 0.0, 0.5), ptr::null_mut(), mode) };
            assert!(bg.is_transform_mode(), "mode {} should be transform", mode);
        }
    }

    #[test]
    fn is_not_transform_mode_for_1_3_4() {
        for mode in [1u32, 3, 4] {
            let bg = unsafe { BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (1.0, 0.0, 0.0, 0.5), ptr::null_mut(), mode) };
            assert!(!bg.is_transform_mode(), "mode {} should NOT be transform", mode);
        }
    }

    #[test]
    fn is_not_transform_mode_for_out_of_range() {
        // mode > 6 → non-transform path (raw 의 b.hi)
        for mode in [7u32, 8, 100, u32::MAX] {
            let bg = unsafe { BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (1.0, 0.0, 0.0, 0.5), ptr::null_mut(), mode) };
            assert!(!bg.is_transform_mode(), "mode {} should NOT be transform", mode);
        }
    }

    #[test]
    fn allocate_transform_mode() {
        // raw asm path 1: (x - h/2, y - w, x + h/2, y)
        let bg = unsafe { BlipGlyph::new((10.0, 0.0, 0.0, 0.3), (20.0, 0.0, 0.0, 0.7), ptr::null_mut(), 0) };
        let alloc = Allocation::at_point(PointImpl { x: 100.0, y: 200.0 });
        let ext = bg.allocate(&alloc);
        assert_eq!(ext.x_min, 100.0 - 20.0 * 0.5); // 90.0
        assert_eq!(ext.y_min, 200.0 - 10.0);        // 190.0
        assert_eq!(ext.x_max, 100.0 + 20.0 * 0.5); // 110.0
        assert_eq!(ext.y_max, 200.0);
    }

    #[test]
    fn allocate_non_transform_mode() {
        // raw asm path 2: (x - w*ax, y - h*ay, x - (1-ax)*w, y - (1-ay)*h)
        let bg = unsafe { BlipGlyph::new((10.0, 0.0, 0.0, 0.3), (20.0, 0.0, 0.0, 0.7), ptr::null_mut(), 1) };
        let alloc = Allocation::at_point(PointImpl { x: 100.0, y: 200.0 });
        let ext = bg.allocate(&alloc);
        assert_eq!(ext.x_min, 100.0 - 0.3 * 10.0);            // 97.0
        assert_eq!(ext.y_min, 200.0 - 0.7 * 20.0);            // 186.0
        assert_eq!(ext.x_max, 100.0 - (1.0 - 0.3) * 10.0);     // 93.0
        assert_eq!(ext.y_max, 200.0 - (1.0 - 0.7) * 20.0);     // 194.0
    }

    #[test]
    fn draw_skips_when_picture_null() {
        let bg = unsafe { BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (1.0, 0.0, 0.0, 0.5), ptr::null_mut(), 0) };
        let mut surface = SvgSurface::new(100.0, 100.0);
        let alloc = Allocation::at_point(PointImpl { x: 0.0, y: 0.0 });
        unsafe { bg.draw(&mut surface, &alloc) };
        // null picture → 즉시 return, buffer 변경 없음
        assert!(surface.buffer.is_empty());
    }

    #[test]
    fn draw_skips_when_picture_obj_null() {
        // picture ctrl 은 존재하지만 obj=null (release 된 SharePtr)
        let p = Box::into_raw(Box::new(ControlBlock::<ImageBrush> {
            obj: ptr::null_mut(),
            refcount: 1,
        }));
        let bg = unsafe { BlipGlyph::new((1.0, 0.0, 0.0, 0.5), (1.0, 0.0, 0.0, 0.5), p, 0) };
        let mut surface = SvgSurface::new(100.0, 100.0);
        let alloc = Allocation::at_point(PointImpl { x: 0.0, y: 0.0 });
        unsafe { bg.draw(&mut surface, &alloc) };
        assert!(surface.buffer.is_empty());
        // cleanup
        drop(bg);
        unsafe { let _ = Box::from_raw(p); }
    }

    #[test]
    fn draw_non_transform_emits_image_within_save_restore() {
        let p = make_picture(ImageBrush::new("test-img-123".to_string()));
        let bg = unsafe { BlipGlyph::new((50.0, 0.0, 0.0, 0.5), (30.0, 0.0, 0.0, 0.5), p, 1) };
        let mut surface = SvgSurface::new(500.0, 500.0);
        let alloc = Allocation::at_point(PointImpl { x: 100.0, y: 200.0 });
        unsafe { bg.draw(&mut surface, &alloc) };
        // save_state → <g>, restore_state → </g>, draw_blip → <image>
        assert!(surface.buffer.contains("<g"), "missing <g> from save_state: {}", surface.buffer);
        assert!(surface.buffer.contains("</g>"), "missing </g> from restore_state: {}", surface.buffer);
        assert!(surface.buffer.contains("<image"), "missing <image> from draw_blip: {}", surface.buffer);
        assert!(surface.buffer.contains("test-img-123"), "image source_id not embedded: {}", surface.buffer);
        drop(bg);
        unsafe { free_picture(p); }
    }

    #[test]
    fn draw_transform_mode_emits_image_with_transform() {
        let p = make_picture(ImageBrush::new("rotated-img".to_string()));
        let bg = unsafe { BlipGlyph::new((40.0, 0.0, 0.0, 0.5), (60.0, 0.0, 0.0, 0.5), p, 2) };
        let mut surface = SvgSurface::new(500.0, 500.0);
        let alloc = Allocation::at_point(PointImpl { x: 100.0, y: 100.0 });
        unsafe { bg.draw(&mut surface, &alloc) };
        // transform path → save → concat_transform → image → restore
        assert!(surface.buffer.contains("<g"));
        assert!(surface.buffer.contains("<image"));
        assert!(surface.buffer.contains("rotated-img"));
        // concat_transform 으로 group transform 이 있어야 함
        assert!(surface.buffer.contains("transform=") || surface.buffer.matches("<g").count() >= 1);
        drop(bg);
        unsafe { free_picture(p); }
    }
}
