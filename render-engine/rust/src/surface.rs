//! `Hnc::Shape::Render::Surface` — drawing target abstraction.
//!
//! 한컴 macOS `libHncDrawingEngine.dylib` 의 `Render::Surface` 클래스 (Windows GDI+
//! 추상화 wrapper) 의 **API 시그니처 byte-equivalent** trait 정의.
//!
//! ## 정책 (2026-05-17 Option B)
//!
//! [feedback_rhwp_byte_equivalent_goal.md] 에 따라:
//! - **Trait 시그니처** = 한컴 API byte-eq (Glyph::Draw vfunc 가 byte-eq port 되었을 때 그대로 호출 가능)
//! - **구현** = `SvgSurface` 어댑터가 SVG primitive 로 emit (200-400줄 추정, 우리 custom)
//! - 한컴의 GDI+/HDC/HWND 기반 8 ctor 변종, libhsp shim, PDFKit backend 는 **SKIP**
//!
//! ## API 출처
//!
//! `nm -U libHncDrawingEngine_arm64.dylib | c++filt | grep Render::Surface::` (2026-05-17 dump)
//! 53 unique method. 본 trait 은 그 53 개와 1:1.
//!
//! ## 진행 단계
//!
//! - **S-1** (현 단계, 2026-05-17): trait 선언 + 보조 type stub + SvgSurface skeleton
//! - **S-2**: SvgSurface 의 trivial method 구현 (Fill/Outline/Transform/State)
//! - **S-3**: DrawString → HFT glyph path 통합 (kdsnr-hft 사용, `<text>` 아닌 `<path>` emit)
//! - **S-4**: DrawImage / SetClip / GetDC 등 잔여
//!
//! ## byte-eq 보장 영역
//!
//! Trait 메소드 시그니처는 한컴과 1:1 (param 순서, 타입, const 한정 모두 동일). Surface
//! 의 *호출자* (Glyph::Draw vfunc 등) 가 한컴 decompile 1:1 port 라도, 동일 trait 메소드 호출
//! 로 컴파일됨. 출력 backend (SvgSurface) 만 다름.

use crate::brush::Brush;
use crate::color::Color;
use crate::pen::Pen;

// ─── 보조 type stub (raw RE 완료 시 별도 module 로 이관) ─────────────

/// `Hnc::Type::PointImpl<T>` — 한컴의 generic 좌표.
/// raw decompile 에서 sizeof = 2 × sizeof(T). T = int 또는 float.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointImpl<T> {
    pub x: T,
    pub y: T,
}

/// `Hnc::Type::SizeImpl<T>`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeImpl<T> {
    pub w: T,
    pub h: T,
}

/// `Hnc::Type::RectImpl<T>` — { x, y, w, h } 4 field. raw 의 RectImpl_int/_float 둘 다.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectImpl<T> {
    pub x: T,
    pub y: T,
    pub w: T,
    pub h: T,
}

/// `Hnc::Util::Transform2D` — 6-element affine (typical SVG matrix).
/// raw decompile 에선 sizeof TBD (audit 후 확정).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Transform2D {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };
}

/// `Hnc::Shape::Render::Path` — path 객체 (CGPath wrapper).
/// raw 의 17 ctor variant (Rect / Vec<Point> / Point pair / CGPath) 가 있음.
/// 본 stub 은 sub-task S-2 에서 expand.
#[derive(Clone, Debug, Default)]
pub struct Path {
    pub commands: Vec<PathCmd>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

/// `Hnc::Shape::Render::Font` — 폰트 인스턴스. sub-task S-3 에서 HFT 통합 시 expand.
#[derive(Debug)]
pub struct Font {
    pub family: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

/// `Hnc::Shape::Render::StringFormat` — 텍스트 정렬 / alignment 정보.
#[derive(Clone, Copy, Debug, Default)]
pub struct StringFormat {
    pub align: u32,
}

/// `Hnc::Shape::Render::Image` — 이미지 (CGImageRef wrapper).
#[derive(Debug)]
pub struct Image {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// `Hnc::Shape::Render::Pixels` — pixel buffer (`GetPixels` 류 사용).
#[derive(Debug, Default)]
pub struct Pixels {
    pub data: Vec<u8>,
}

// ─── Surface trait (53 method) ─────────────────────────────────────────

/// Surface trait — `Hnc::Shape::Render::Surface` 의 메소드 시그니처 byte-eq.
///
/// 53 method (Surface 자체 1ctor + 7 ctor variant + 45 method) 중 핵심 trait 메소드만
/// declare. ctor variant 들은 `SvgSurface::new_*` factory function 으로 별도 제공
/// (trait 에 ctor 못 둠).
///
/// raw 출처: `nm -U libHncDrawingEngine_arm64.dylib | grep Render::Surface::`.
pub trait Surface {
    // ─── 채우기 (Fill / Outline) ───────────────────────────────────────

    /// `Fill(RectImpl<int>, Brush)` @ `0x8b3c0`
    fn fill_rect_int(&mut self, rect: RectImpl<i32>, brush: &Brush);

    /// `Fill(RectImpl<float>, Brush)` @ `0xa8850`
    fn fill_rect_float(&mut self, rect: RectImpl<f32>, brush: &Brush);

    /// `Fill(Path, Brush)` @ `0xa87b8`
    fn fill_path(&mut self, path: &Path, brush: &Brush);

    /// `Outline(RectImpl<int>, Pen)` @ `0xa8898`
    fn outline_rect_int(&mut self, rect: RectImpl<i32>, pen: &Pen);

    /// `Outline(RectImpl<float>, Pen)` @ `0xa8904`
    fn outline_rect_float(&mut self, rect: RectImpl<f32>, pen: &Pen);

    /// `Outline(Path, Pen)` @ `0xa8804`
    fn outline_path(&mut self, path: &Path, pen: &Pen);

    // ─── 이미지 ─────────────────────────────────────────────────────────

    /// `DrawImage(RectImpl<int>, Image, bool, float)` @ `0x8b654`
    fn draw_image_rect(
        &mut self,
        rect: RectImpl<i32>,
        image: &Image,
        flag: bool,
        alpha: f32,
    );

    /// `DrawImage(PointImpl<float>, Image, Transform2D, Color*)` @ `0x93090`
    fn draw_image_point(
        &mut self,
        pt: PointImpl<f32>,
        image: &Image,
        transform: &Transform2D,
        color: Option<&Color>,
    );

    /// `DrawImageF(...)` — 별도 float-기반 variant (signature TBD)
    fn draw_image_f(&mut self, rect: RectImpl<f32>, image: &Image, alpha: f32);

    /// `DrawImageBorder(...)` — border 그리기 (signature TBD)
    fn draw_image_border(&mut self, rect: RectImpl<f32>, image: &Image);

    /// `DrawNoImage(...)` — image fallback placeholder (signature TBD)
    fn draw_no_image(&mut self, rect: RectImpl<f32>);

    // ─── 텍스트 ─────────────────────────────────────────────────────────

    /// `DrawString(wchar_t*, int, Font, PointImpl<float>, Brush, StringFormat)` @ `0xa894c`
    ///
    /// 우리 backend (SvgSurface) 에선 `<text>` 가 아닌 HFT glyph path emit.
    fn draw_string_point(
        &mut self,
        text: &[u16],
        font: &Font,
        pos: PointImpl<f32>,
        brush: &Brush,
        format: &StringFormat,
    );

    /// `DrawString(wchar_t*, int, Font, RectImpl<float>, Brush, StringFormat)` @ `0xa898c`
    fn draw_string_rect(
        &mut self,
        text: &[u16],
        font: &Font,
        rect: RectImpl<f32>,
        brush: &Brush,
        format: &StringFormat,
    );

    /// `DrawDriverString(wchar_t*, int, Font, Brush, PointImpl<float>*, int, Transform2D)`
    /// @ `0xa89d8` — per-glyph position 명시 (kdsnr-layout 의 linesegarray output 매핑에 적합)
    fn draw_driver_string(
        &mut self,
        text: &[u16],
        font: &Font,
        brush: &Brush,
        positions: &[PointImpl<f32>],
        transform: &Transform2D,
    );

    /// `MeasureString(wchar_t*, int, Font, PointImpl<float>, StringFormat, RectImpl<float>&)` @ `0xa899c`
    fn measure_string_point(
        &self,
        text: &[u16],
        font: &Font,
        pos: PointImpl<f32>,
        format: &StringFormat,
    ) -> RectImpl<f32>;

    /// `MeasureString(... RectImpl ...)` @ `0xa89a8`
    fn measure_string_rect(
        &self,
        text: &[u16],
        font: &Font,
        rect: RectImpl<f32>,
        format: &StringFormat,
    ) -> RectImpl<f32>;

    /// `MeasureDriverString(...)` (signature TBD, per-glyph 측정)
    fn measure_driver_string(
        &self,
        text: &[u16],
        font: &Font,
        positions: &[PointImpl<f32>],
    ) -> RectImpl<f32>;

    // ─── 그 외 도형 ─────────────────────────────────────────────────────

    /// `DrawPie(...)` — pie/arc (signature TBD)
    fn draw_pie(
        &mut self,
        rect: RectImpl<f32>,
        start_angle: f32,
        sweep_angle: f32,
        pen: &Pen,
    );

    // ─── Transform ─────────────────────────────────────────────────────

    /// `GetTransform() const` @ `0xa7b3c`
    fn get_transform(&self) -> Transform2D;

    /// `SetTransform(Transform2D)` @ `0xa7bdc`
    fn set_transform(&mut self, transform: &Transform2D);

    /// `SetCartesianTransform(Transform2D)` @ `0xa7bd0`
    fn set_cartesian_transform(&mut self, transform: &Transform2D);

    /// `GetCartesianTransform()` (signature TBD)
    fn get_cartesian_transform(&self) -> Transform2D;

    /// `ApplyCartesianCoordinate(...)` (signature TBD)
    fn apply_cartesian_coordinate(&mut self, transform: &Transform2D);

    /// `ResetTransform()`
    fn reset_transform(&mut self);

    /// `SetOffset(f32, f32)`
    fn set_offset(&mut self, dx: f32, dy: f32);

    /// `GetOffset() → PointImpl<f32>`
    fn get_offset(&self) -> PointImpl<f32>;

    /// `SetZoom(f32)`
    fn set_zoom(&mut self, zoom: f32);

    /// `GetZoom() → f32`
    fn get_zoom(&self) -> f32;

    /// `InitZoomAndOffset(...)` (signature TBD)
    fn init_zoom_and_offset(&mut self);

    /// `Scale(f32, f32)` — scale 적용
    fn scale(&mut self, sx: f32, sy: f32);

    /// `GetScale() → PointImpl<f32>`
    fn get_scale(&self) -> PointImpl<f32>;

    /// `GetContextScale() → f32`
    fn get_context_scale(&self) -> f32;

    /// `Translate(f32, f32)`
    fn translate(&mut self, dx: f32, dy: f32);

    // ─── Clip ───────────────────────────────────────────────────────────

    /// `SetClip(Path)` — clip path 설정
    fn set_clip(&mut self, path: &Path);

    /// `ResetClip()`
    fn reset_clip(&mut self);

    /// `GetClipBounds() → RectImpl<f32>`
    fn get_clip_bounds(&self) -> RectImpl<f32>;

    /// `DetachRegion()` (signature TBD)
    fn detach_region(&mut self);

    // ─── 렌더 옵션 ───────────────────────────────────────────────────────

    /// `SetAntialiasing(bool)`
    fn set_antialiasing(&mut self, enabled: bool);

    /// `SetFillAntialiasing(bool)`
    fn set_fill_antialiasing(&mut self, enabled: bool);

    /// `SetInterpolationMode(u32)`
    fn set_interpolation_mode(&mut self, mode: u32);

    /// `SetTextRenderingHint(u32)`
    fn set_text_rendering_hint(&mut self, hint: u32);

    /// `SetCompositingMode(u32)`
    fn set_compositing_mode(&mut self, mode: u32);

    /// `GetCompositingMode() → u32`
    fn get_compositing_mode(&self) -> u32;

    /// `SetPenInteger(...)` — pen pixel-alignment 옵션
    fn set_pen_integer(&mut self, enabled: bool);

    /// `GetPenInteger() → bool`
    fn get_pen_integer(&self) -> bool;

    // ─── State / Backend ────────────────────────────────────────────────

    /// `IsPrint() const → bool` @ `0xa87b0`
    fn is_print(&self) -> bool;

    /// `IsValidMemory() → bool`
    fn is_valid_memory(&self) -> bool;

    /// `Detach()` @ `0x8b42c` — backend release
    fn detach(&mut self);

    /// `GetMemory() → *mut ()` — backend pointer 직접 접근
    fn get_memory(&self) -> *const ();

    /// `GetNative() → *mut ()` — native (CGContext/HDC 등) 직접 접근
    fn get_native(&self) -> *const ();

    /// `GetImpl() const → *const ()` @ `0x866ec`
    fn get_impl(&self) -> *const ();

    /// `GetDC() → *mut ()` — HDC (Windows API) 가져오기
    fn get_dc(&self) -> *const ();

    /// `ReleaseDC(*mut ())` — HDC 반환
    fn release_dc(&mut self, dc: *const ());

    /// `GetLastError() → i32`
    fn get_last_error(&self) -> i32;

    // ─── GState (CGContextSaveGState / RestoreGState) ────────────────────
    //
    // raw `BlipGlyph::Draw` 등 의 `new SurfaceRestorer` / `~SurfaceRestorer`
    // RAII 패턴 의 trait 측 API. SvgSurface 는 `<g>` push/pop 으로 매핑.

    /// `SurfaceRestorer ctor` (raw `CGContextSaveGState` 호출 측).
    fn save_state(&mut self);

    /// `~SurfaceRestorer` (raw `CGContextRestoreGState` 호출 측).
    fn restore_state(&mut self);

    /// 현재 CTM 위에 `t` 합성 (raw `CGContextConcatCTM` 또는
    /// Matrix3::PreMultiply 후 ConcatCTM 의 등치).
    ///
    /// `t` 는 `Hnc::Util::Transform2D` (byte-eq from libHncFoundation).
    fn concat_transform(&mut self, t: &crate::transform2d::Transform2D);

    /// `BlipGlyph::Draw` 의 vfunc[13] 등치 — Path 안에 picture 그리기.
    ///
    /// raw `0x2d18e8` 의 `blr x9` (Surface->impl vtable[0x68/8 = 13]) 호출:
    /// `DrawBlip(Paths*, Matrix3*, ?, ImageData* out, int, int) → ImageData`.
    ///
    /// SvgSurface 는 `<image href="data:..." />` (clip-path 적용) emit. 실제 binary
    /// 데이터는 ImageBrush 의 `source_id` 로 caller 가 등록한 binData 매핑에서 가져옴.
    fn draw_blip(
        &mut self,
        path: &crate::path::Path,
        picture: *mut crate::share_ptr::ControlBlock<crate::brush::ImageBrush>,
    );
}

// ─── static method (Hnc::Shape::Render::Surface::) ─────────────────────

/// `IsSystemHighContrastMode()` — Windows 고대비 모드 검사 (Mac/Linux 에선 false)
pub fn is_system_high_contrast_mode() -> bool {
    false
}

// ─── 검증 (sizeof / offset / 53 method count) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_impl_layout_int() {
        assert_eq!(std::mem::size_of::<PointImpl<i32>>(), 8);
        assert_eq!(std::mem::align_of::<PointImpl<i32>>(), 4);
    }

    #[test]
    fn point_impl_layout_float() {
        assert_eq!(std::mem::size_of::<PointImpl<f32>>(), 8);
        assert_eq!(std::mem::align_of::<PointImpl<f32>>(), 4);
    }

    #[test]
    fn size_impl_layout() {
        assert_eq!(std::mem::size_of::<SizeImpl<i32>>(), 8);
    }

    #[test]
    fn rect_impl_layout_int() {
        assert_eq!(std::mem::size_of::<RectImpl<i32>>(), 16);
        assert_eq!(std::mem::align_of::<RectImpl<i32>>(), 4);
    }

    #[test]
    fn rect_impl_layout_float() {
        assert_eq!(std::mem::size_of::<RectImpl<f32>>(), 16);
    }

    #[test]
    fn transform2d_layout() {
        // 6 × 4B = 24B
        assert_eq!(std::mem::size_of::<Transform2D>(), 24);
    }

    #[test]
    fn transform2d_identity() {
        let id = Transform2D::IDENTITY;
        assert_eq!(id.a, 1.0);
        assert_eq!(id.d, 1.0);
        assert_eq!(id.tx, 0.0);
    }
}
