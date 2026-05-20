//! `Hnc::Shape::Render::Path` — byte-eq port (L-5c-4 framework).
//!
//! raw 출처: `libHncDrawingEngine.dylib`
//!  - C2 default ctor: `0xa110c`  / C1: `0xa1184`  (b to C2)
//!  - dtor D2: `0xa1768` / D1: `0xa181c`
//!  - Swap: `0xa18d0` (then inline impl_swap @ `0xa192c`)
//!  - Setter/getter cluster: `0xa1bfc..0xa1c44` (12 inline accessors)
//!
//! ## 외부 layout (24B, raw 검증)
//!
//! ```text
//! offset  size  field            init (default ctor)
//! ──────  ────  ───────────────  ─────────────────────
//!   0      4    style: u32       0
//!   4      1    stroke_style: u8 1   (raw: strh w8, [x0,#4] with w8=0x0101)
//!   5      1    extrusion_ok: u8 1
//!   6-7    2    (padding)        -
//!   8      4    light: f32       0.0
//!  12      1    close_contains:u8 0
//! 13-15    3    (padding)        -
//!  16      8    impl_ptr: *mut PathImpl
//! ```
//!
//! ## 내부 layout (PathImpl, semantic-eq)
//!
//! raw PathImpl = 24B `std::vector<SharePtr<Subpath>>` (begin/end/cap 8B 씩).
//! 본 port 는 pixel-eq policy 에 따라 storage 는 `Vec<*mut ControlBlock<Subpath>>`
//! 로 idiomatic Rust + semantic 동등. raw 가 reserve(20) 으로 시작.
//!
//! ## 본 module 의 완성 범위 (L-5c-4a + L-5c-4b, 60 tests)
//!
//! - Path struct 24B byte-eq layout
//! - default ctor (raw 0xa110c) + dtor (raw 0xa1768)
//! - 5 setter + 5 getter (raw 0xa1bfc..0xa1c44)
//! - Swap (raw 0xa18d0 외부 5 field swap + 내부 impl swap)
//! - Subpath enum (Move/Line/Bezier/Begin/Close) — 5 raw subpath family 매핑
//! - PathImpl::add_line (raw 0x792dc), add_close (raw 0x799f4), add_begin (raw 0x797c0),
//!   add_bezier (raw 0x798c0)
//! - geometry ctor 4 variant: from_rect_f/i (raw 0xa1260/0xa1188),
//!   from_line_f/i (raw 0xa15d0/0xa14e8), from_polyline_f/i (raw 0xa1410/0xa1338)
//! - public Add* method: add_line/add_line_i/add_polyline/add_polyline_i,
//!   add_rect/add_rect_i, add_bezier/add_bezier_i/add_bezier_chain,
//!   start, close
//! - Clone (raw 0xa1a9c) — deep copy via Subpath::Copy
//! - Transform (raw 0xa2340) — IsIdentity short-circuit + per-point Transform2D::apply
//! - Outline / Expand / Union — raw stub 그대로 (0xa2388/0xa2390/0xa2398)
//! - Flatten — placeholder no-op (raw helper 0x7d860 RE 후 Bezier→polyline 변환)
//! - GetPointCount/GetPoints/GetTypes — subpath traversal
//! - GetBounds — placeholder min/max (raw helper 0x72c34 정확 logic 후속)
//!
//! ## 보류 (L-5c-4c, 별도 helper RE 필요)
//!
//! - AddArc(RectF, Degree, Degree) — raw helper 0x7aa44 RE 필요
//! - AddEllipse(RectF/RectI) — raw helper 0x7a930 RE 필요
//! - AddCurve(slice, tension) — raw helper 0x79fa4 RE 필요 (smoothed Bezier chain)
//! - Flatten(Bezier→polyline 변환) — raw helper 0x7d860 RE 필요
//! - GetBounds 정확 logic (Pen 두께 + Transform 처리) — raw helper 0x72c34 RE 필요
//! - GetStartPoint/GetLastPoint — subpath virtual vfunc dispatch 정밀 port 필요
//!
//! ## 보류 (L-5c-4d, CoreGraphics/HFT 의존 — S-4 backend 와 같이)
//!
//! - Path(CGPath*) ctor
//! - AddString(wchar*, FontFamily, size, PointF, StringFormat) — text → CGPath via HFT
//! - IsVisible(Surface, PointF/I) — CGPathContainsPoint
//! - IsOutlineVisible(Surface, Pen*, PointF/I) — CGPathCreateCopyByStrokingPath + contains

/// `Hnc::Type::PointImpl<float>` — 8B (x: f32, y: f32). raw 직접 매핑.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PointF {
    pub x: f32,
    pub y: f32,
}

impl PointF {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// `Hnc::Type::PointImpl<int>` — 8B (x: i32, y: i32). raw 직접 매핑.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PointI {
    pub x: i32,
    pub y: i32,
}

impl PointI {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    /// raw helper `scvtf` int→float conversion (per-component).
    pub fn to_f(self) -> PointF {
        PointF::new(self.x as f32, self.y as f32)
    }
}

/// `Hnc::Type::RectImpl<float>` — 16B (origin: PointF, size: SizeF).
///
/// raw layout 검증: 0x78c00 의 `ldp s0,s1, [x20,#4]` (origin.x,y) + `ldr s2, [x20,#0xc]`
/// (size.w) → origin @ +4, size @ +0xc. 본 port 는 paddingless `#[repr(C)]` 로 동일 효과.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RectF {
    pub _pad_origin_alignment: u32,  // raw 의 ldp 가 +4 부터 시작하니 +0..3 은 사용 안함
    pub origin: PointF,              // +4..11
    pub size_w: f32,                 // +12..15
    pub size_h: f32,                 // +16..19
}

impl RectF {
    /// 편의 ctor — raw layout 직접 채움.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            _pad_origin_alignment: 0,
            origin: PointF::new(x, y),
            size_w: w,
            size_h: h,
        }
    }
}

/// `Hnc::Type::RectImpl<int>` — 16B 변형 (origin: PointI, size: SizeI).
///
/// raw 0x789c0 의 `ldur d1, [x20,#0x4]` (origin x,y as 2x i32) + `ldr w21, [x20,#0x10]`
/// (size.h) + `ldr s0, [x20,#0xc]` (size.w as i32).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RectI {
    pub _pad_origin_alignment: u32,
    pub origin: PointI,    // +4..11
    pub size_w: i32,       // +12..15
    pub size_h: i32,       // +16..19
}

impl RectI {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            _pad_origin_alignment: 0,
            origin: PointI::new(x, y),
            size_w: w,
            size_h: h,
        }
    }
}

/// `Hnc::Shape::Render::Path::Style` — path drawing style enum (placeholder).
///
/// raw 의 u32 enum. CharItemView::Draw 분석 후 정확한 variant 매핑 예정 (L-5c-10).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style(pub u32);

/// `Hnc::Shape::Render::Path` — 외부 24B class.
#[repr(C)]
pub struct Path {
    /// offset 0 — `style: Style` (u32).
    pub style: Style,
    /// offset 4 — `stroke_style: bool` (default true). raw: strh w8=0x0101 → low byte.
    pub stroke_style: u8,
    /// offset 5 — `extrusion_ok: bool` (default true). raw: high byte of 0x0101.
    pub extrusion_ok: u8,
    /// offset 6-7 — padding.
    _pad1: [u8; 2],
    /// offset 8 — `light: f32` (default 0.0).
    pub light: f32,
    /// offset 12 — `close_contains: bool` (default false).
    pub close_contains: u8,
    /// offset 13-15 — padding.
    _pad2: [u8; 3],
    /// offset 16 — `impl: *mut PathImpl` (default new PathImpl with reserve(20)).
    pub impl_ptr: *mut PathImpl,
}

/// `Hnc::Shape::Render::Subpath` — single path command.
///
/// raw 의 polymorphic Subpath family (vtable adrp base = 0x793000):
/// - LineSubpath (vtable @ +0x960, 32B): p1/p2/type. type=0=Move, type=2=Line
/// - BezierSubpath (vtable @ +0x9a8, 40B): 4 control points (cubic)
/// - StartSubpath (vtable @ +0xa38, 8B): explicit "begin new subpath" marker
/// - CloseSubpath (vtable @ +0xa98, 8B): "close current subpath" marker
///
/// 본 port 는 Rust enum 으로 semantic-eq.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Subpath {
    /// raw LineSubpath (32B) with type=0. 첫 AddLine call 의 implicit "Move(0,0)" marker.
    /// raw 에서 p1=p2=(0,0) 로 초기화.
    Move { p1: PointF, p2: PointF },
    /// raw LineSubpath (32B) with type=2. 실제 line segment.
    Line { p1: PointF, p2: PointF },
    /// raw BezierSubpath (40B). 4 control points (cubic Bezier).
    Bezier { p1: PointF, p2: PointF, p3: PointF, p4: PointF },
    /// raw StartSubpath (8B). explicit "begin new subpath" marker
    /// (Path::start() / 0x797c0 helper 에서 push).
    Begin,
    /// raw CloseSubpath (8B). "close current subpath" marker
    /// (Path::close() / 0x799f4 helper 에서 push).
    Close,
}

impl Subpath {
    /// raw type field (offset 0x18 of LineSubpath) 값. Bezier/Begin/Close 는 type 없음.
    pub fn raw_type_field(&self) -> u32 {
        match self {
            Subpath::Move { .. } => 0,
            Subpath::Line { .. } => 2,
            Subpath::Bezier { .. } => 0,
            Subpath::Begin => 0,
            Subpath::Close => 0,
        }
    }
}

/// `Hnc::Shape::Render::PathImpl` — 내부 storage.
///
/// raw 24B `std::vector<SharePtr<Subpath>>` (begin/end/cap × 8B).
/// 본 port 는 semantic-eq Rust `Vec<Subpath>` (single ownership;
/// raw 의 SharePtr<Subpath> refcount 는 Clone/Cow 시점에 의미가 있고
/// raw add_line 시점엔 refcount=2 (vector + local) 가 즉시 1 로 떨어져
/// vector 단독 소유 = Rust Vec 단독 소유와 동등).
#[repr(C)]
pub struct PathImpl {
    pub subpaths: Vec<Subpath>,
}

/// Transform2D 의 apply 를 우리 PointF 에 적용. 내부 conversion 만.
fn apply_transform_to_point(t: &crate::transform2d::Transform2D, p: &mut PointF) {
    let mut pi = crate::surface::PointImpl::<f32> { x: p.x, y: p.y };
    t.apply(&mut pi);
    p.x = pi.x;
    p.y = pi.y;
}

impl PathImpl {
    /// raw default state: empty vector with reserve(20) (raw `bl 0x62fae4(impl, 20)`).
    ///
    /// reserve 는 capacity 만 늘리고 len 은 0. semantic-eq Rust Vec.
    pub fn new() -> Self {
        Self { subpaths: Vec::with_capacity(20) }
    }

    /// raw `AddLine(impl, p1, p2)` @ `0x792dc` (~ 300B).
    ///
    /// 의미: PathImpl 에 Line segment 추가.
    /// - 빈 vector 이면 **Start(p1=(0,0), p2=(0,0)) marker 를 먼저 push**
    /// - 그 다음 항상 Line(p1, p2) push
    ///
    /// raw 검증:
    /// - 0x79304: `ldp x8, x25, [x0]` → begin/end
    /// - 0x79310: `cmp x25, x8; b.eq 0x793bc` → empty 분기
    /// - branch 1 (non-empty): new Subpath(32B) with vtable+p1+p2+type=2, wrap CB, push
    /// - branch 2 (empty): new Subpath(32B) with vtable+0+0+type=0 (Start), wrap CB, push,
    ///   then continue to also push the Line subpath (joined paths)
    pub fn add_line(&mut self, p1: PointF, p2: PointF) {
        if self.subpaths.is_empty() {
            self.subpaths.push(Subpath::Move {
                p1: PointF::new(0.0, 0.0),
                p2: PointF::new(0.0, 0.0),
            });
        }
        self.subpaths.push(Subpath::Line { p1, p2 });
    }

    /// raw `Subpath_Start_push(impl)` @ `0x797c0` (~ 84B).
    ///
    /// 새 8B StartSubpath (vtable @ +0xa38, no fields) 만들어 push.
    /// 의미: "begin new subpath" 마커. AddLine 의 implicit Move(0,0) 와는 별개.
    pub fn add_begin(&mut self) {
        self.subpaths.push(Subpath::Begin);
    }

    /// raw `AddBezier(impl, p1, p2, p3, p4)` @ `0x798c0` (~ 230B).
    ///
    /// 의미: PathImpl 에 cubic Bezier subpath 1개 push.
    /// raw 에 empty check 없음 — 항상 새 BezierSubpath 만들고 push.
    /// (LineSubpath 와 다르게 implicit Start 안 생김)
    pub fn add_bezier(&mut self, p1: PointF, p2: PointF, p3: PointF, p4: PointF) {
        self.subpaths.push(Subpath::Bezier { p1, p2, p3, p4 });
    }

    /// raw `AddClose(impl)` @ `0x799f4` (~ 100B).
    ///
    /// 의미: PathImpl 에 Close marker 1개 push. 빈 vector 도 그냥 Close 추가
    /// (raw 가 empty check 없이 바로 alloc + push).
    pub fn add_close(&mut self) {
        self.subpaths.push(Subpath::Close);
    }
}

impl Default for PathImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl Path {
    /// `Path::Path()` default ctor — raw `0xa110c` (C2) / `0xa1184` (C1=b to C2).
    ///
    /// raw 순서:
    /// 1. str wzr, [x0]           → style = 0
    /// 2. mov w8, #0x101; strh w8, [x0,#4] → stroke_style=1, extrusion_ok=1
    /// 3. str wzr, [x0,#8]        → light = 0.0
    /// 4. strb wzr, [x0,#0xc]     → close_contains = 0
    /// 5. mov w0, #0x18; bl __Znwm → alloc 24B PathImpl
    /// 6. stp xzr, xzr, [x0,#8]; str xzr, [x0] → impl all-zero (3× null ptr)
    /// 7. mov w1, #0x14; bl 0x62fae4 → reserve(20)
    /// 8. str x19, [x20,#0x10]    → self.impl_ptr = alloc
    pub fn new() -> Self {
        Self::with_impl(PathImpl::new())
    }

    /// 내부 helper — meta 필드 default init + 주어진 PathImpl 을 Box 로 owned.
    /// raw 의 모든 ctor 가 동일한 meta init 시퀀스 (0xa110c..0xa113c 와 동일) 를
    /// 공유하므로 DRY.
    fn with_impl(impl_data: PathImpl) -> Self {
        Self {
            style: Style(0),
            stroke_style: 1,
            extrusion_ok: 1,
            _pad1: [0; 2],
            light: 0.0,
            close_contains: 0,
            _pad2: [0; 3],
            impl_ptr: Box::into_raw(Box::new(impl_data)),
        }
    }

    /// `Path(RectImpl<float>&)` ctor — raw C2 `0xa1260` / C1 `0xa12cc`.
    ///
    /// raw 흐름:
    /// 1. meta init (default ctor 와 동일)
    /// 2. alloc 24B PathImpl
    /// 3. call helper `0x78c00(impl, &rect)`
    ///    - reserve(20)
    ///    - AddLine((x,y), (x+w,y))       — top edge
    ///    - AddLine((x+w,y), (x+w,y+h))   — right edge
    ///    - AddLine((x+w,y+h), (x,y+h))   — bottom edge
    ///    - AddClose                      — close (implicit left edge)
    /// 4. self.impl_ptr = impl
    ///
    /// 결과 subpath sequence (5 entries due to implicit Start in first AddLine):
    /// `[Start(0,0,0,0), Line(top), Line(right), Line(bottom), Close]`
    pub fn from_rect_f(rect: &RectF) -> Self {
        let mut impl_data = PathImpl::new();
        let x = rect.origin.x;
        let y = rect.origin.y;
        let w = rect.size_w;
        let h = rect.size_h;
        impl_data.add_line(PointF::new(x, y), PointF::new(x + w, y));
        impl_data.add_line(PointF::new(x + w, y), PointF::new(x + w, y + h));
        impl_data.add_line(PointF::new(x + w, y + h), PointF::new(x, y + h));
        impl_data.add_close();
        Self::with_impl(impl_data)
    }

    /// `Path(RectImpl<int>&)` ctor — raw C2 `0xa1188` / C1 `0xa11f4`.
    ///
    /// raw 흐름은 from_rect_f 와 동일하나, 입력이 int 라서 helper `0x789c0` 가
    /// 각 좌표에 `scvtf` (int→float) 를 적용. semantic-eq 로 `as f32` 변환 후
    /// from_rect_f 와 동일 결과.
    pub fn from_rect_i(rect: &RectI) -> Self {
        Self::from_rect_f(&RectF {
            _pad_origin_alignment: 0,
            origin: rect.origin.to_f(),
            size_w: rect.size_w as f32,
            size_h: rect.size_h as f32,
        })
    }

    /// `Path(PointImpl<float>&, PointImpl<float>&)` ctor — raw C2 `0xa15d0` / C1 `0xa1644`.
    ///
    /// raw 흐름:
    /// 1. meta init
    /// 2. alloc 24B PathImpl + reserve(20)
    /// 3. AddLine(p1, p2)
    ///   - empty 분기로 Start(0,0,0,0) 먼저 push, 그 다음 Line(p1, p2) push
    ///
    /// 결과 subpath: `[Start(0,0,0,0), Line(p1, p2)]`
    pub fn from_line_f(p1: PointF, p2: PointF) -> Self {
        let mut impl_data = PathImpl::new();
        impl_data.add_line(p1, p2);
        Self::with_impl(impl_data)
    }

    /// `Path(PointImpl<int>&, PointImpl<int>&)` ctor — raw C2 `0xa14e8` / C1 `0xa155c`.
    ///
    /// from_line_f 의 int 변형. helper 0x79184 에서 `scvtf` 로 int→float 변환 후 동일.
    pub fn from_line_i(p1: PointI, p2: PointI) -> Self {
        Self::from_line_f(p1.to_f(), p2.to_f())
    }

    /// `Path(Vec<PointImpl<float>>&)` ctor — raw C2 `0xa1410` / C1 `0xa147c`.
    ///
    /// helper `0x790dc` 가 vector 의 연속 pair 마다 AddLine 호출. polyline 생성.
    /// 빈 vector → no subpaths.
    pub fn from_polyline_f(points: &[PointF]) -> Self {
        let mut impl_data = PathImpl::new();
        let n = points.len();
        if n >= 2 {
            for i in 0..n - 1 {
                impl_data.add_line(points[i], points[i + 1]);
            }
        }
        Self::with_impl(impl_data)
    }

    /// `Path(Vec<PointImpl<int>>&)` ctor — raw C2 `0xa1338` / C1 `0xa13a4`.
    ///
    /// from_polyline_f 의 int 변형. helper 에서 `scvtf` per-element 후 동일.
    pub fn from_polyline_i(points: &[PointI]) -> Self {
        let pts_f: Vec<PointF> = points.iter().map(|p| p.to_f()).collect();
        Self::from_polyline_f(&pts_f)
    }

    // ─── 5 inline getter (raw 0xa1bfc, 0xa1c0c, 0xa1c1c, 0xa1c2c, 0xa1c3c) ───

    /// `GetStyle()` — raw `0xa1bfc`: `ldr w0, [x0]; ret`.
    #[inline]
    pub fn get_style(&self) -> Style {
        self.style
    }

    /// `GetStrokeStyle()` — raw `0xa1c0c`: `ldrb w0, [x0,#4]; ret`.
    #[inline]
    pub fn get_stroke_style(&self) -> bool {
        self.stroke_style != 0
    }

    /// `GetExtrusionOk()` — raw `0xa1c1c`: `ldrb w0, [x0,#5]; ret`.
    #[inline]
    pub fn get_extrusion_ok(&self) -> bool {
        self.extrusion_ok != 0
    }

    /// `GetLight()` — raw `0xa1c2c`: `ldr s0, [x0,#8]; ret`.
    #[inline]
    pub fn get_light(&self) -> f32 {
        self.light
    }

    /// `GetCloseContains()` — raw `0xa1c3c`: `ldrb w0, [x0,#0xc]; ret`.
    #[inline]
    pub fn get_close_contains(&self) -> bool {
        self.close_contains != 0
    }

    // ─── 5 inline setter (raw 0xa1c04, 0xa1c14, 0xa1c24, 0xa1c34, 0xa1c44) ───

    /// `SetStyle(Style)` — raw `0xa1c04`: `str w1, [x0]; ret`.
    #[inline]
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// `SetStrokeStyle(bool)` — raw `0xa1c14`: `strb w1, [x0,#4]; ret`.
    #[inline]
    pub fn set_stroke_style(&mut self, v: bool) {
        self.stroke_style = v as u8;
    }

    /// `SetExtrusionOk(bool)` — raw `0xa1c24`: `strb w1, [x0,#5]; ret`.
    #[inline]
    pub fn set_extrusion_ok(&mut self, v: bool) {
        self.extrusion_ok = v as u8;
    }

    /// `SetLight(f32)` — raw `0xa1c34`: `str s0, [x0,#8]; ret`.
    #[inline]
    pub fn set_light(&mut self, light: f32) {
        self.light = light;
    }

    /// `SetCloseContains(bool)` — raw `0xa1c44`: `strb w1, [x0,#0xc]; ret`.
    #[inline]
    pub fn set_close_contains(&mut self, v: bool) {
        self.close_contains = v as u8;
    }

    // ─── public Add* methods (thin wrappers over PathImpl) ──────────

    /// `AddLine(PointImpl<float>&, PointImpl<float>&)` — raw `0xa1c88`:
    /// `ldr x0,[x0,#0x10]; b 0x792dc`. Pure tail-call.
    #[inline]
    pub fn add_line(&mut self, p1: PointF, p2: PointF) {
        self.impl_mut().add_line(p1, p2);
    }

    /// `AddLine(PointImpl<int>&, PointImpl<int>&)` — raw `0xa1c4c`:
    /// scvtf int→float per-coord, then call 0x792dc.
    #[inline]
    pub fn add_line_i(&mut self, p1: PointI, p2: PointI) {
        self.add_line(p1.to_f(), p2.to_f());
    }

    /// `AddLine(__wrap_iter<PointImpl<float>>, __wrap_iter<PointImpl<float>>)` —
    /// raw `0xa1c90`. polyline: 연속 pair `(points[i], points[i+1])` 마다 AddLine
    /// 호출. `points.len() < 2` 이면 no-op.
    pub fn add_polyline(&mut self, points: &[PointF]) {
        let n = points.len();
        if n < 2 {
            return;
        }
        for i in 0..n - 1 {
            self.add_line(points[i], points[i + 1]);
        }
    }

    /// `AddLine(__wrap_iter<PointImpl<int>>, __wrap_iter<PointImpl<int>>)` —
    /// int 변형. scvtf per-element.
    pub fn add_polyline_i(&mut self, points: &[PointI]) {
        let n = points.len();
        if n < 2 {
            return;
        }
        for i in 0..n - 1 {
            self.add_line_i(points[i], points[i + 1]);
        }
    }

    /// `AddRect(RectImpl<float>&)` — raw `0xa1e88`.
    ///
    /// raw 와 동일 시퀀스: 4-line + close (from_rect_f 와 동일 logic on existing impl).
    pub fn add_rect(&mut self, rect: &RectF) {
        let x = rect.origin.x;
        let y = rect.origin.y;
        let w = rect.size_w;
        let h = rect.size_h;
        let impl_data = self.impl_mut();
        impl_data.add_line(PointF::new(x, y), PointF::new(x + w, y));
        impl_data.add_line(PointF::new(x + w, y), PointF::new(x + w, y + h));
        impl_data.add_line(PointF::new(x + w, y + h), PointF::new(x, y + h));
        impl_data.add_close();
    }

    /// `AddRect(RectImpl<int>&)` — raw `0xa1df4`. int → scvtf → float path.
    pub fn add_rect_i(&mut self, rect: &RectI) {
        self.add_rect(&RectF {
            _pad_origin_alignment: 0,
            origin: rect.origin.to_f(),
            size_w: rect.size_w as f32,
            size_h: rect.size_h as f32,
        });
    }

    /// `AddBezier(PointImpl<float>×4)` — raw `0xa1d58`: tail-call 0x798c0.
    #[inline]
    pub fn add_bezier(&mut self, p1: PointF, p2: PointF, p3: PointF, p4: PointF) {
        self.impl_mut().add_bezier(p1, p2, p3, p4);
    }

    /// `AddBezier(PointImpl<int>×4)` — raw `0xa1cfc`: scvtf per-coord, then call 0x798c0.
    #[inline]
    pub fn add_bezier_i(&mut self, p1: PointI, p2: PointI, p3: PointI, p4: PointI) {
        self.add_bezier(p1.to_f(), p2.to_f(), p3.to_f(), p4.to_f());
    }

    /// `AddBezier(__wrap_iter, __wrap_iter)` — raw `0xa1d60`. cubic Bezier chain:
    /// 3-step sliding window over points. `points.len() < 4` 이면 no-op.
    ///
    /// raw 0xa1d68 의 `cmp x8, #0x20` (8 = 4 points = 32 bytes) check.
    /// Loop: 매 iteration 마다 4 consecutive points 로 AddBezier 호출, 다음 iter 는
    /// 3 points (24 byte) 진행.
    pub fn add_bezier_chain(&mut self, points: &[PointF]) {
        if points.len() < 4 {
            return;
        }
        let mut i = 0;
        while i + 3 < points.len() {
            self.add_bezier(points[i], points[i + 1], points[i + 2], points[i + 3]);
            i += 3;
        }
    }

    /// `Close()` — raw `0xa207c`: tail-call to 0x799f4 with impl_ptr.
    #[inline]
    pub fn close(&mut self) {
        self.impl_mut().add_close();
    }

    /// `Start()` — raw `0xa2074`: tail-call to 0x797c0 with impl_ptr.
    /// 명시적 begin-new-subpath 마커 push.
    #[inline]
    pub fn start(&mut self) {
        self.impl_mut().add_begin();
    }

    /// `Clone() const` — raw `0xa1a9c` (~ 300B).
    ///
    /// raw 흐름:
    /// 1. Call helper `0x79b7c(self.impl_ptr, sret)` — deep-clone PathImpl (모든 subpath 복사)
    /// 2. If cloned impl is null → set sret=null (return)
    /// 3. else: alloc new 24B Path, default-init, copy 5 meta fields
    /// 4. Replace new Path's default impl with cloned impl
    ///
    /// 본 port 는 Subpath enum 이 Copy → 단순 `Vec::clone()` 으로 deep copy 동등.
    pub fn clone_path(&self) -> Path {
        let cloned_impl = unsafe { PathImpl { subpaths: (*self.impl_ptr).subpaths.clone() } };
        Self {
            style: self.style,
            stroke_style: self.stroke_style,
            extrusion_ok: self.extrusion_ok,
            _pad1: [0; 2],
            light: self.light,
            close_contains: self.close_contains,
            _pad2: [0; 3],
            impl_ptr: Box::into_raw(Box::new(cloned_impl)),
        }
    }

    // ─── Transform / Flatten / Outline / Expand / Union ──────────────



    /// `Transform(Transform2D&)` — raw `0xa2340` (~ 60B).
    ///
    /// raw 흐름:
    /// 1. Transform2D::IsIdentity() check → true 면 즉시 return (no-op)
    /// 2. else: tail-call helper `0x7d72c(impl, transform)` — 모든 subpath 의
    ///    각 point 에 matrix 곱셈
    pub fn transform(&mut self, t: &crate::transform2d::Transform2D) {
        if t.is_identity() {
            return;
        }
        let impl_data = self.impl_mut();
        for s in impl_data.subpaths.iter_mut() {
            match s {
                Subpath::Move { p1, p2 } | Subpath::Line { p1, p2 } => {
                    apply_transform_to_point(t, p1);
                    apply_transform_to_point(t, p2);
                }
                Subpath::Bezier { p1, p2, p3, p4 } => {
                    apply_transform_to_point(t, p1);
                    apply_transform_to_point(t, p2);
                    apply_transform_to_point(t, p3);
                    apply_transform_to_point(t, p4);
                }
                Subpath::Begin | Subpath::Close => {}
            }
        }
    }

    /// `Flatten()` — raw `0xa2380`: tail-call `0x7d860`. 곡선을 짧은 line 시퀀스로
    /// 근사 변환. raw helper 0x7d860 RE 후 byte-eq port 예정 (L-5c-4b 후속).
    /// 본 port 는 placeholder no-op (Bezier 변환 알고리즘 미구현).
    pub fn flatten(&mut self) {
        // TODO L-5c-4b-iii: 0x7d860 helper RE 후 Bezier → polyline 변환 구현
    }

    /// `Outline()` — raw `0xa2388`: **stub** (`mov w0,#0; ret`).
    /// raw 가 의도적 no-op (return 0). 본 port 도 no-op.
    pub fn outline(&mut self) {
        // raw 가 stub. 의미: outline 처리 안 함.
    }

    /// `Expand(width, ...)` — raw `0xa2390`: **stub** (`str xzr,[x8]; ret`).
    /// raw 가 sret slot 에 null 쓰고 return → 새 Path 생성 안 함. 본 port 도 None.
    pub fn expand(
        &self,
        _width: f32,
        _outside: bool,
        _inside: bool,
        _close: bool,
        _line_cap: u32,
    ) -> Option<Path> {
        None
    }

    /// `Union(f32)` — raw `0xa2398`: **stub** (`ret`).
    /// raw 가 pure no-op. 본 port 도 no-op.
    pub fn union(&mut self, _flatness: f32) {
        // raw stub.
    }

    // ─── simple Get* methods ────────────────────────────────────────

    /// `GetPointCount()` — raw `0xa2178`.
    ///
    /// raw 흐름: 임시 `vector<PointF>` 만들어 GetPoints 호출 → vec.size() 반환.
    /// 본 port 는 직접 subpath traversal 로 계산 (semantic-eq).
    ///
    /// 각 subpath 당 point 수:
    /// - Move: 2 (raw 의 GetFirstPoint/GetLastPoint 가 subpath 의 2 points 반환)
    /// - Line: 2
    /// - Bezier: 4
    /// - Begin: 0
    /// - Close: 0
    pub fn get_point_count(&self) -> usize {
        let impl_data = unsafe { &*self.impl_ptr };
        impl_data
            .subpaths
            .iter()
            .map(|s| match s {
                Subpath::Move { .. } => 2,
                Subpath::Line { .. } => 2,
                Subpath::Bezier { .. } => 4,
                Subpath::Begin => 0,
                Subpath::Close => 0,
            })
            .sum()
    }

    /// `GetPoints(vector<PointF>&)` — raw `0xa2168`: tail-call 0x7b674.
    /// subpath 마다 point 들을 sequential 추가.
    pub fn get_points(&self) -> Vec<PointF> {
        let impl_data = unsafe { &*self.impl_ptr };
        let mut out = Vec::with_capacity(self.get_point_count());
        for s in &impl_data.subpaths {
            match s {
                Subpath::Move { p1, p2 } | Subpath::Line { p1, p2 } => {
                    out.push(*p1);
                    out.push(*p2);
                }
                Subpath::Bezier { p1, p2, p3, p4 } => {
                    out.push(*p1);
                    out.push(*p2);
                    out.push(*p3);
                    out.push(*p4);
                }
                Subpath::Begin | Subpath::Close => {}
            }
        }
        out
    }

    /// `GetBounds(Pen*, Transform2D*) const` — raw `0xa21e4` (~ 16B):
    /// tail-call `0x72c34(impl, pen.impl, transform)`. axis-aligned bounding box.
    ///
    /// 본 port 는 subpath 순회로 min/max 계산 (semantic-eq). Pen/Transform 처리는
    /// L-5c-4b 후속에서 (Pen 두께 → bounds 확장, Transform → bounds 변환).
    /// 빈 path → empty RectF (0,0,0,0).
    pub fn get_bounds(&self) -> RectF {
        let impl_data = unsafe { &*self.impl_ptr };
        if impl_data.subpaths.is_empty() {
            return RectF::new(0.0, 0.0, 0.0, 0.0);
        }
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut any = false;
        let mut accept = |p: PointF| {
            // Move(0,0) marker 는 의미 없는 origin 으로 bounds 왜곡 가능. raw 가 어떻게
            // 처리하는지는 helper 0x72c34 RE 후 확정 (L-5c-4b 후속). 본 placeholder 는
            // 모든 point 포함.
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
            any = true;
        };
        for s in &impl_data.subpaths {
            match s {
                Subpath::Move { p1, p2 } | Subpath::Line { p1, p2 } => {
                    accept(*p1);
                    accept(*p2);
                }
                Subpath::Bezier { p1, p2, p3, p4 } => {
                    accept(*p1);
                    accept(*p2);
                    accept(*p3);
                    accept(*p4);
                }
                Subpath::Begin | Subpath::Close => {}
            }
        }
        if !any {
            return RectF::new(0.0, 0.0, 0.0, 0.0);
        }
        RectF::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// `GetTypes(vector<u8>&)` — raw `0xa2170`: tail-call 0x7c26c.
    /// subpath 의 type 바이트 시퀀스 반환. SVG path command 식별자에 해당.
    ///
    /// raw 와 정확한 enum 매핑은 L-5c-4b 후속에서 0x7c26c 정독 후 확정. 본 port 는
    /// semantic placeholder (실제 enum 값은 CharItemView::Draw 분석 시 fix).
    pub fn get_types(&self) -> Vec<u8> {
        let impl_data = unsafe { &*self.impl_ptr };
        impl_data
            .subpaths
            .iter()
            .map(|s| match s {
                Subpath::Move { .. } => 0u8,
                Subpath::Line { .. } => 2u8,
                Subpath::Bezier { .. } => 3u8,  // SVG-style cubic curve marker
                Subpath::Begin => 4u8,           // placeholder
                Subpath::Close => 5u8,           // placeholder (SVG Z = 0x80 in some encodings)
            })
            .collect()
    }

    /// 내부 helper — `*mut PathImpl` → `&mut PathImpl`. raw 의 `ldr x0,[x0,#0x10]`
    /// 와 의미 동등. impl_ptr 가 null 일 가능성은 ctor 후 정상 사용에서는 0.
    #[inline]
    fn impl_mut(&mut self) -> &mut PathImpl {
        // SAFETY: impl_ptr 는 Path ctor (default/from_rect/from_line) 에서 항상
        // valid Box::into_raw 결과. dtor 전까지 유효.
        unsafe { &mut *self.impl_ptr }
    }

    /// `GetImpl()` — raw `0x884bc`: `ldr x0, [x0,#0x10]; ret`.
    ///
    /// 외부에 PathImpl 포인터 그대로 반환 (raw 와 동일 의미).
    #[inline]
    pub fn get_impl_ptr(&self) -> *mut PathImpl {
        self.impl_ptr
    }

    /// `Swap(Path&)` — raw `0xa18d0` (외부 5 field) + `0xa192c` (내부 impl_ptr move).
    ///
    /// raw 순서:
    /// 1. style swap (4B)
    /// 2. stroke_style swap (1B at +4)
    /// 3. extrusion_ok swap (1B at +5)
    /// 4. light swap (4B at +8)
    /// 5. close_contains swap (1B at +0xc)
    /// 6. impl_ptr swap (jump to 0xa192c)
    ///
    /// 0xa192c 의 raw 는 분기 cleanup logic 포함하나 semantic 은 단순 ptr swap +
    /// 이전 owner 의 Vec destructor 실행. Rust Box swap 이 byte-eq semantic.
    pub fn swap(&mut self, other: &mut Path) {
        std::mem::swap(&mut self.style, &mut other.style);
        std::mem::swap(&mut self.stroke_style, &mut other.stroke_style);
        std::mem::swap(&mut self.extrusion_ok, &mut other.extrusion_ok);
        std::mem::swap(&mut self.light, &mut other.light);
        std::mem::swap(&mut self.close_contains, &mut other.close_contains);
        std::mem::swap(&mut self.impl_ptr, &mut other.impl_ptr);
    }
}

impl Drop for Path {
    /// `~Path()` — raw `0xa1768` (D2) / `0xa181c` (D1).
    ///
    /// 1. self.impl_ptr 가 null 이 아니면 PathImpl destructor (vector destroy) 실행
    /// 2. operator delete(impl_ptr, 24)
    fn drop(&mut self) {
        if !self.impl_ptr.is_null() {
            // SAFETY: impl_ptr is owned by self, allocated via Box::into_raw in `new()`
            // (or moved in via Swap). Box::from_raw + drop = byte-eq semantic of
            // PathImpl::~PathImpl() (Vec destructor) + operator delete(24).
            unsafe {
                let _ = Box::from_raw(self.impl_ptr);
            }
        }
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    // ─── layout ──────────────────────────────────────────────────────

    #[test]
    fn path_size_is_24_bytes() {
        assert_eq!(size_of::<Path>(), 24);
    }

    #[test]
    fn path_alignment_is_8() {
        assert_eq!(align_of::<Path>(), 8);
    }

    #[test]
    fn path_field_offsets_match_raw() {
        assert_eq!(offset_of!(Path, style), 0);
        assert_eq!(offset_of!(Path, stroke_style), 4);
        assert_eq!(offset_of!(Path, extrusion_ok), 5);
        assert_eq!(offset_of!(Path, light), 8);
        assert_eq!(offset_of!(Path, close_contains), 12);
        assert_eq!(offset_of!(Path, impl_ptr), 16);
    }

    // ─── ctor defaults ───────────────────────────────────────────────

    #[test]
    fn default_ctor_matches_raw_initial_state() {
        let p = Path::new();
        assert_eq!(p.style.0, 0);
        assert_eq!(p.stroke_style, 1, "raw mov w8,#0x101; strh w8,[x0,#4] low byte");
        assert_eq!(p.extrusion_ok, 1, "raw mov w8,#0x101; strh w8,[x0,#4] high byte");
        assert_eq!(p.light, 0.0);
        assert_eq!(p.close_contains, 0);
        assert!(!p.impl_ptr.is_null());
    }

    #[test]
    fn default_ctor_pathimpl_starts_empty_with_capacity_20() {
        let p = Path::new();
        unsafe {
            assert_eq!((*p.impl_ptr).subpaths.len(), 0);
            assert_eq!((*p.impl_ptr).subpaths.capacity(), 20,
                "raw 의 bl 0x62fae4 (impl, 20) = reserve(20)");
        }
    }

    // ─── getters ─────────────────────────────────────────────────────

    #[test]
    fn getters_round_trip_with_default_state() {
        let p = Path::new();
        assert_eq!(p.get_style().0, 0);
        assert_eq!(p.get_stroke_style(), true);
        assert_eq!(p.get_extrusion_ok(), true);
        assert_eq!(p.get_light(), 0.0);
        assert_eq!(p.get_close_contains(), false);
    }

    // ─── setters ─────────────────────────────────────────────────────

    #[test]
    fn set_style_writes_4_bytes_at_offset_0() {
        let mut p = Path::new();
        p.set_style(Style(0xdeadbeef));
        assert_eq!(p.style.0, 0xdeadbeef);
        assert_eq!(p.get_style().0, 0xdeadbeef);
    }

    #[test]
    fn set_stroke_style_writes_1_byte_at_offset_4() {
        let mut p = Path::new();
        p.set_stroke_style(false);
        assert_eq!(p.stroke_style, 0);
        assert!(!p.get_stroke_style());
        p.set_stroke_style(true);
        assert_eq!(p.stroke_style, 1);
        assert!(p.get_stroke_style());
    }

    #[test]
    fn set_extrusion_ok_writes_1_byte_at_offset_5() {
        let mut p = Path::new();
        p.set_extrusion_ok(false);
        assert_eq!(p.extrusion_ok, 0);
        assert!(!p.get_extrusion_ok());
        // 인접 stroke_style 보존 검증 (raw 의 strb 만 +5 에 write)
        assert_eq!(p.stroke_style, 1);
    }

    #[test]
    fn set_light_writes_4_bytes_at_offset_8() {
        let mut p = Path::new();
        p.set_light(3.14_f32);
        assert_eq!(p.light, 3.14_f32);
        assert_eq!(p.get_light(), 3.14_f32);
    }

    #[test]
    fn set_close_contains_writes_1_byte_at_offset_c() {
        let mut p = Path::new();
        p.set_close_contains(true);
        assert_eq!(p.close_contains, 1);
        assert!(p.get_close_contains());
        // 인접 fields (light @ +8) 보존
        assert_eq!(p.light, 0.0);
    }

    #[test]
    fn setters_do_not_touch_impl_ptr() {
        let mut p = Path::new();
        let original_impl = p.impl_ptr;
        p.set_style(Style(1));
        p.set_stroke_style(false);
        p.set_extrusion_ok(false);
        p.set_light(2.5);
        p.set_close_contains(true);
        assert_eq!(p.impl_ptr, original_impl);
    }

    // ─── Swap ────────────────────────────────────────────────────────

    #[test]
    fn swap_exchanges_all_5_fields_and_impl_ptr() {
        let mut a = Path::new();
        let mut b = Path::new();
        a.set_style(Style(0xaaaa));
        a.set_stroke_style(false);
        a.set_extrusion_ok(false);
        a.set_light(11.0);
        a.set_close_contains(true);

        b.set_style(Style(0xbbbb));
        b.set_stroke_style(true);
        b.set_extrusion_ok(true);
        b.set_light(22.0);
        b.set_close_contains(false);

        let a_impl_before = a.impl_ptr;
        let b_impl_before = b.impl_ptr;

        a.swap(&mut b);

        assert_eq!(a.style.0, 0xbbbb);
        assert_eq!(a.stroke_style, 1);
        assert_eq!(a.extrusion_ok, 1);
        assert_eq!(a.light, 22.0);
        assert_eq!(a.close_contains, 0);
        assert_eq!(a.impl_ptr, b_impl_before);

        assert_eq!(b.style.0, 0xaaaa);
        assert_eq!(b.stroke_style, 0);
        assert_eq!(b.extrusion_ok, 0);
        assert_eq!(b.light, 11.0);
        assert_eq!(b.close_contains, 1);
        assert_eq!(b.impl_ptr, a_impl_before);
    }

    #[test]
    fn swap_does_not_leak_or_double_free() {
        // 두 Path 의 impl 이 정상 swap 후에도 drop 시 single-free 검증.
        // sanitizer 없이도 valgrind 류 검사에서 leak 없으려면 swap 이 ptr
        // 단순 교환만 해야.
        let mut a = Path::new();
        let mut b = Path::new();
        a.swap(&mut b);
        drop(a);
        drop(b);
    }

    // ─── dtor ────────────────────────────────────────────────────────

    #[test]
    fn dtor_frees_impl_when_non_null() {
        // miri 또는 ASan 으로 검증. drop 호출 자체 panic-free 면 pass.
        let p = Path::new();
        drop(p);
    }

    // ─── PathImpl::add_line / add_close ─────────────────────────────

    #[test]
    fn add_line_to_empty_pushes_start_then_line() {
        let mut impl_data = PathImpl::new();
        impl_data.add_line(PointF::new(1.0, 2.0), PointF::new(3.0, 4.0));
        assert_eq!(impl_data.subpaths.len(), 2);
        assert_eq!(impl_data.subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0),
            p2: PointF::new(0.0, 0.0),
        });
        assert_eq!(impl_data.subpaths[1], Subpath::Line {
            p1: PointF::new(1.0, 2.0),
            p2: PointF::new(3.0, 4.0),
        });
    }

    #[test]
    fn add_line_to_nonempty_pushes_only_line() {
        let mut impl_data = PathImpl::new();
        impl_data.add_line(PointF::new(0.0, 0.0), PointF::new(1.0, 0.0));
        // now has 2 entries (Start + Line)
        impl_data.add_line(PointF::new(1.0, 0.0), PointF::new(1.0, 1.0));
        assert_eq!(impl_data.subpaths.len(), 3);
        assert_eq!(impl_data.subpaths[2], Subpath::Line {
            p1: PointF::new(1.0, 0.0),
            p2: PointF::new(1.0, 1.0),
        });
    }

    #[test]
    fn add_close_pushes_close_marker() {
        let mut impl_data = PathImpl::new();
        impl_data.add_close();
        assert_eq!(impl_data.subpaths.len(), 1);
        assert_eq!(impl_data.subpaths[0], Subpath::Close);
    }

    #[test]
    fn subpath_raw_type_field_matches_raw_asm_constants() {
        assert_eq!(Subpath::Move { p1: PointF::default(), p2: PointF::default() }.raw_type_field(), 0,
            "raw 0x793d0 str wzr [x0,#0x18]");
        assert_eq!(Subpath::Line { p1: PointF::default(), p2: PointF::default() }.raw_type_field(), 2,
            "raw 0x79334 mov w8,#0x2; str w8,[x0,#0x18]");
        assert_eq!(Subpath::Bezier {
            p1: PointF::default(), p2: PointF::default(),
            p3: PointF::default(), p4: PointF::default(),
        }.raw_type_field(), 0, "BezierSubpath 는 type 필드 없음");
        assert_eq!(Subpath::Begin.raw_type_field(), 0);
        assert_eq!(Subpath::Close.raw_type_field(), 0);
    }

    // ─── PointF / PointI / RectF / RectI layout ─────────────────────

    #[test]
    fn pointf_layout_8b_repr_c() {
        assert_eq!(size_of::<PointF>(), 8);
        assert_eq!(align_of::<PointF>(), 4);
        assert_eq!(offset_of!(PointF, x), 0);
        assert_eq!(offset_of!(PointF, y), 4);
    }

    #[test]
    fn pointi_to_f_converts_per_component() {
        let pi = PointI::new(-3, 7);
        let pf = pi.to_f();
        assert_eq!(pf.x, -3.0);
        assert_eq!(pf.y, 7.0);
    }

    #[test]
    fn rectf_layout_origin_at_offset_4() {
        // raw 0x78c00 의 ldp s0,s1 [x20,#4] 가 origin 을 읽음 → origin @ +4
        assert_eq!(offset_of!(RectF, origin), 4);
        assert_eq!(offset_of!(RectF, size_w), 12);
        assert_eq!(offset_of!(RectF, size_h), 16);
    }

    // ─── from_rect_f ────────────────────────────────────────────────

    #[test]
    fn from_rect_f_produces_5_subpaths_start_3lines_close() {
        let rect = RectF::new(10.0, 20.0, 100.0, 50.0);
        let p = Path::from_rect_f(&rect);
        let impl_data = unsafe { &*p.impl_ptr };
        assert_eq!(impl_data.subpaths.len(), 5);
        assert_eq!(impl_data.subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(0.0, 0.0)
        });
        assert_eq!(impl_data.subpaths[1], Subpath::Line {
            p1: PointF::new(10.0, 20.0), p2: PointF::new(110.0, 20.0)
        }, "top edge");
        assert_eq!(impl_data.subpaths[2], Subpath::Line {
            p1: PointF::new(110.0, 20.0), p2: PointF::new(110.0, 70.0)
        }, "right edge");
        assert_eq!(impl_data.subpaths[3], Subpath::Line {
            p1: PointF::new(110.0, 70.0), p2: PointF::new(10.0, 70.0)
        }, "bottom edge");
        assert_eq!(impl_data.subpaths[4], Subpath::Close);
    }

    #[test]
    fn from_rect_f_has_same_meta_default_as_default_ctor() {
        let p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(p.style.0, 0);
        assert_eq!(p.stroke_style, 1);
        assert_eq!(p.extrusion_ok, 1);
        assert_eq!(p.light, 0.0);
        assert_eq!(p.close_contains, 0);
    }

    // ─── from_rect_i ────────────────────────────────────────────────

    #[test]
    fn from_rect_i_byte_eq_with_from_rect_f_after_scvtf() {
        let ri = RectI::new(-5, 10, 20, 30);
        let p_int = Path::from_rect_i(&ri);
        let p_flt = Path::from_rect_f(&RectF::new(-5.0, 10.0, 20.0, 30.0));
        let impl_i = unsafe { &*p_int.impl_ptr };
        let impl_f = unsafe { &*p_flt.impl_ptr };
        assert_eq!(impl_i.subpaths, impl_f.subpaths,
            "raw helper 0x789c0 = 0x78c00 + scvtf prefix");
    }

    // ─── from_line_f / from_line_i ──────────────────────────────────

    #[test]
    fn from_line_f_produces_start_plus_line() {
        let p = Path::from_line_f(PointF::new(1.0, 2.0), PointF::new(5.0, 8.0));
        let impl_data = unsafe { &*p.impl_ptr };
        assert_eq!(impl_data.subpaths.len(), 2);
        assert_eq!(impl_data.subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(0.0, 0.0)
        });
        assert_eq!(impl_data.subpaths[1], Subpath::Line {
            p1: PointF::new(1.0, 2.0), p2: PointF::new(5.0, 8.0)
        });
    }

    #[test]
    fn from_line_i_byte_eq_with_from_line_f_after_scvtf() {
        let p_int = Path::from_line_i(PointI::new(-1, 2), PointI::new(3, -4));
        let p_flt = Path::from_line_f(PointF::new(-1.0, 2.0), PointF::new(3.0, -4.0));
        let impl_i = unsafe { &*p_int.impl_ptr };
        let impl_f = unsafe { &*p_flt.impl_ptr };
        assert_eq!(impl_i.subpaths, impl_f.subpaths);
    }

    // ─── public Add* method tests ───────────────────────────────────

    #[test]
    fn add_line_appends_start_plus_line_on_empty_path() {
        let mut p = Path::new();
        p.add_line(PointF::new(1.0, 2.0), PointF::new(3.0, 4.0));
        let impl_data = unsafe { &*p.impl_ptr };
        assert_eq!(impl_data.subpaths.len(), 2);
        assert_eq!(impl_data.subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(0.0, 0.0)
        });
        assert_eq!(impl_data.subpaths[1], Subpath::Line {
            p1: PointF::new(1.0, 2.0), p2: PointF::new(3.0, 4.0)
        });
    }

    #[test]
    fn add_line_i_byte_eq_with_add_line_after_scvtf() {
        let mut p_i = Path::new();
        let mut p_f = Path::new();
        p_i.add_line_i(PointI::new(-5, 7), PointI::new(11, -13));
        p_f.add_line(PointF::new(-5.0, 7.0), PointF::new(11.0, -13.0));
        unsafe {
            assert_eq!((*p_i.impl_ptr).subpaths, (*p_f.impl_ptr).subpaths);
        }
    }

    #[test]
    fn add_polyline_iterates_consecutive_pairs() {
        let mut p = Path::new();
        let points = [
            PointF::new(0.0, 0.0),
            PointF::new(1.0, 0.0),
            PointF::new(1.0, 1.0),
            PointF::new(0.0, 1.0),
        ];
        p.add_polyline(&points);
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        // 3 AddLine calls → Start (from first) + 3 Lines
        assert_eq!(subpaths.len(), 4);
        assert_eq!(subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(0.0, 0.0)
        });
        assert_eq!(subpaths[1], Subpath::Line {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(1.0, 0.0)
        });
        assert_eq!(subpaths[2], Subpath::Line {
            p1: PointF::new(1.0, 0.0), p2: PointF::new(1.0, 1.0)
        });
        assert_eq!(subpaths[3], Subpath::Line {
            p1: PointF::new(1.0, 1.0), p2: PointF::new(0.0, 1.0)
        });
    }

    #[test]
    fn add_polyline_with_fewer_than_2_points_is_noop() {
        let mut p = Path::new();
        p.add_polyline(&[]);
        p.add_polyline(&[PointF::new(1.0, 1.0)]);
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(subpaths.len(), 0,
            "raw 0xa1c94: cmp x1,x2; b.eq 0xa1cf8 (return) on empty/single");
    }

    #[test]
    fn add_polyline_i_byte_eq_with_add_polyline_after_scvtf() {
        let mut p_i = Path::new();
        let mut p_f = Path::new();
        let pi = [PointI::new(0, 0), PointI::new(10, 0), PointI::new(10, 10)];
        let pf = [PointF::new(0.0, 0.0), PointF::new(10.0, 0.0), PointF::new(10.0, 10.0)];
        p_i.add_polyline_i(&pi);
        p_f.add_polyline(&pf);
        unsafe {
            assert_eq!((*p_i.impl_ptr).subpaths, (*p_f.impl_ptr).subpaths);
        }
    }

    #[test]
    fn add_rect_matches_from_rect_when_called_on_fresh_path() {
        let rect = RectF::new(5.0, 10.0, 20.0, 30.0);
        let p_from = Path::from_rect_f(&rect);
        let mut p_add = Path::new();
        p_add.add_rect(&rect);
        unsafe {
            assert_eq!((*p_from.impl_ptr).subpaths, (*p_add.impl_ptr).subpaths);
        }
    }

    #[test]
    fn add_rect_appends_to_existing_path() {
        let mut p = Path::new();
        p.add_line(PointF::new(0.0, 0.0), PointF::new(1.0, 0.0));
        // path has 2 subpaths: Start + Line
        p.add_rect(&RectF::new(2.0, 0.0, 1.0, 1.0));
        // Add 3 lines + Close (no Start since not empty)
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(subpaths.len(), 2 + 4);  // 2 existing + 3 lines + Close
        assert_eq!(subpaths[5], Subpath::Close);
    }

    #[test]
    fn add_rect_i_byte_eq_with_add_rect_after_scvtf() {
        let mut p_i = Path::new();
        let mut p_f = Path::new();
        p_i.add_rect_i(&RectI::new(-3, 5, 10, 20));
        p_f.add_rect(&RectF::new(-3.0, 5.0, 10.0, 20.0));
        unsafe {
            assert_eq!((*p_i.impl_ptr).subpaths, (*p_f.impl_ptr).subpaths);
        }
    }

    // ─── from_polyline ──────────────────────────────────────────────

    #[test]
    fn from_polyline_f_chains_lines() {
        let pts = [
            PointF::new(0.0, 0.0),
            PointF::new(1.0, 0.0),
            PointF::new(1.0, 1.0),
            PointF::new(0.0, 1.0),
            PointF::new(0.0, 0.0),
        ];
        let p = Path::from_polyline_f(&pts);
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        // 4 AddLine calls (5 points → 4 segments). First call adds implicit Move.
        assert_eq!(subpaths.len(), 5);
        assert_eq!(subpaths[0], Subpath::Move {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(0.0, 0.0)
        });
        assert_eq!(subpaths[1], Subpath::Line {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(1.0, 0.0)
        });
        assert_eq!(subpaths[4], Subpath::Line {
            p1: PointF::new(0.0, 1.0), p2: PointF::new(0.0, 0.0)
        });
    }

    #[test]
    fn from_polyline_f_empty_and_single_point_produce_empty_path() {
        let p0 = Path::from_polyline_f(&[]);
        let p1 = Path::from_polyline_f(&[PointF::new(5.0, 5.0)]);
        unsafe {
            assert_eq!((*p0.impl_ptr).subpaths.len(), 0);
            assert_eq!((*p1.impl_ptr).subpaths.len(), 0,
                "raw 0x79110 b.eq exit when begin==end after first advance");
        }
    }

    #[test]
    fn from_polyline_i_byte_eq_with_from_polyline_f_after_scvtf() {
        let pi = [PointI::new(0, 0), PointI::new(2, 3), PointI::new(-1, 4)];
        let pf = [PointF::new(0.0, 0.0), PointF::new(2.0, 3.0), PointF::new(-1.0, 4.0)];
        let p_i = Path::from_polyline_i(&pi);
        let p_f = Path::from_polyline_f(&pf);
        unsafe {
            assert_eq!((*p_i.impl_ptr).subpaths, (*p_f.impl_ptr).subpaths);
        }
    }

    // ─── Bezier ─────────────────────────────────────────────────────

    #[test]
    fn add_bezier_pushes_one_subpath_no_implicit_start() {
        let mut p = Path::new();
        p.add_bezier(
            PointF::new(0.0, 0.0),
            PointF::new(1.0, 2.0),
            PointF::new(3.0, 4.0),
            PointF::new(5.0, 6.0),
        );
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(subpaths.len(), 1, "raw 0x798c0 에 empty check 없음 → Start 안 생김");
        assert_eq!(subpaths[0], Subpath::Bezier {
            p1: PointF::new(0.0, 0.0),
            p2: PointF::new(1.0, 2.0),
            p3: PointF::new(3.0, 4.0),
            p4: PointF::new(5.0, 6.0),
        });
    }

    #[test]
    fn add_bezier_i_byte_eq_with_add_bezier_after_scvtf() {
        let mut p_i = Path::new();
        let mut p_f = Path::new();
        p_i.add_bezier_i(
            PointI::new(0, 0), PointI::new(1, 2), PointI::new(3, 4), PointI::new(5, 6)
        );
        p_f.add_bezier(
            PointF::new(0.0, 0.0), PointF::new(1.0, 2.0), PointF::new(3.0, 4.0), PointF::new(5.0, 6.0)
        );
        unsafe {
            assert_eq!((*p_i.impl_ptr).subpaths, (*p_f.impl_ptr).subpaths);
        }
    }

    #[test]
    fn add_bezier_chain_3_step_sliding_window() {
        let mut p = Path::new();
        // 7 points → 2 beziers: (0,1,2,3) + (3,4,5,6) [overlap at index 3]
        let pts: Vec<PointF> = (0..7).map(|i| PointF::new(i as f32, 0.0)).collect();
        p.add_bezier_chain(&pts);
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(subpaths.len(), 2);
        assert_eq!(subpaths[0], Subpath::Bezier {
            p1: PointF::new(0.0, 0.0), p2: PointF::new(1.0, 0.0),
            p3: PointF::new(2.0, 0.0), p4: PointF::new(3.0, 0.0)
        });
        assert_eq!(subpaths[1], Subpath::Bezier {
            p1: PointF::new(3.0, 0.0), p2: PointF::new(4.0, 0.0),
            p3: PointF::new(5.0, 0.0), p4: PointF::new(6.0, 0.0)
        });
    }

    #[test]
    fn add_bezier_chain_fewer_than_4_points_is_noop() {
        let mut p = Path::new();
        p.add_bezier_chain(&[]);
        p.add_bezier_chain(&[PointF::new(0.0, 0.0); 3]);
        assert_eq!(unsafe { &*p.impl_ptr }.subpaths.len(), 0,
            "raw 0xa1d68 cmp x8,#0x20 (32 bytes = 4 points) check");
    }

    #[test]
    fn add_bezier_chain_exactly_4_points_one_bezier() {
        let mut p = Path::new();
        p.add_bezier_chain(&[
            PointF::new(0.0, 0.0), PointF::new(1.0, 1.0),
            PointF::new(2.0, 2.0), PointF::new(3.0, 3.0),
        ]);
        assert_eq!(unsafe { &*p.impl_ptr }.subpaths.len(), 1);
    }

    #[test]
    fn close_appends_close_marker() {
        let mut p = Path::new();
        p.add_line(PointF::new(0.0, 0.0), PointF::new(1.0, 1.0));
        p.close();
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(*subpaths.last().unwrap(), Subpath::Close);
    }

    // ─── Clone ───────────────────────────────────────────────────────

    #[test]
    fn clone_deep_copies_subpaths_and_meta() {
        let mut p1 = Path::from_rect_f(&RectF::new(0.0, 0.0, 10.0, 10.0));
        p1.set_style(Style(0x42));
        p1.set_stroke_style(false);
        p1.set_light(3.0);
        let p2 = p1.clone_path();
        // 메타 동일
        assert_eq!(p2.style, Style(0x42));
        assert_eq!(p2.stroke_style, 0);
        assert_eq!(p2.extrusion_ok, 1);
        assert_eq!(p2.light, 3.0);
        // subpaths 동일
        unsafe {
            assert_eq!((*p1.impl_ptr).subpaths, (*p2.impl_ptr).subpaths);
        }
        // 그러나 impl_ptr 자체는 별개 (deep copy)
        assert_ne!(p1.impl_ptr, p2.impl_ptr);
    }

    #[test]
    fn clone_mutation_does_not_affect_original() {
        let mut p1 = Path::from_line_f(PointF::new(0.0, 0.0), PointF::new(1.0, 1.0));
        let mut p2 = p1.clone_path();
        p2.add_line(PointF::new(5.0, 5.0), PointF::new(6.0, 6.0));
        let s1 = unsafe { &(*p1.impl_ptr).subpaths };
        let s2 = unsafe { &(*p2.impl_ptr).subpaths };
        assert_eq!(s1.len(), 2);
        assert_eq!(s2.len(), 3);  // p2 has 1 extra Line
    }

    // ─── Transform / Flatten / Outline / Expand / Union ──────────────

    #[test]
    fn transform_with_identity_is_noop() {
        let mut p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        let original = unsafe { (*p.impl_ptr).subpaths.clone() };
        p.transform(&crate::transform2d::Transform2D::default());
        let after = unsafe { &(*p.impl_ptr).subpaths };
        assert_eq!(*after, original, "raw 0xa2358 IsIdentity → return no-op");
    }

    #[test]
    fn transform_with_translation_shifts_all_points() {
        let mut p = Path::from_line_f(PointF::new(1.0, 2.0), PointF::new(3.0, 4.0));
        let mut t = crate::transform2d::Transform2D::default();
        // Translate by (10, 20). raw byte-eq translate via Init - skip and use direct mat
        // 사용 가능한 API: Translate(tx, ty, order). order=0 = append, ≠0 = pre.
        t.translate(&crate::surface::PointImpl::<f32> { x: 10.0, y: 20.0 }, 0);
        p.transform(&t);
        let subpaths = unsafe { &(*p.impl_ptr).subpaths };
        // Move(0,0) → (10,20), Line(1,2,3,4) → Line(11,22,13,24)
        assert_eq!(subpaths[0], Subpath::Move {
            p1: PointF::new(10.0, 20.0), p2: PointF::new(10.0, 20.0)
        });
        assert_eq!(subpaths[1], Subpath::Line {
            p1: PointF::new(11.0, 22.0), p2: PointF::new(13.0, 24.0)
        });
    }

    #[test]
    fn outline_is_raw_stub_noop() {
        let mut p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        let original = unsafe { (*p.impl_ptr).subpaths.clone() };
        p.outline();
        // raw 0xa2388 = `mov w0,#0; ret` → 변화 없음
        assert_eq!(unsafe { &(*p.impl_ptr).subpaths }, &original);
    }

    #[test]
    fn expand_is_raw_stub_returns_none() {
        let p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        assert!(p.expand(2.0, true, true, true, 0).is_none(),
            "raw 0xa2390: str xzr,[x8] → null path return");
    }

    #[test]
    fn union_is_raw_stub_noop() {
        let mut p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        let original = unsafe { (*p.impl_ptr).subpaths.clone() };
        p.union(0.5);
        assert_eq!(unsafe { &(*p.impl_ptr).subpaths }, &original);
    }

    #[test]
    fn flatten_currently_placeholder_noop() {
        let mut p = Path::from_line_f(PointF::new(0.0, 0.0), PointF::new(1.0, 1.0));
        let len_before = unsafe { (*p.impl_ptr).subpaths.len() };
        p.flatten();
        // 본 port 가 placeholder 라 변화 없음 (TODO L-5c-4b-iii)
        assert_eq!(unsafe { (*p.impl_ptr).subpaths.len() }, len_before);
    }

    // ─── start / get_point_count / get_points / get_types ───────────

    #[test]
    fn start_pushes_begin_marker() {
        let mut p = Path::new();
        p.start();
        let subpaths = &unsafe { &*p.impl_ptr }.subpaths;
        assert_eq!(subpaths.len(), 1);
        assert_eq!(subpaths[0], Subpath::Begin);
    }

    #[test]
    fn get_point_count_sums_per_subpath() {
        let mut p = Path::new();
        // Empty
        assert_eq!(p.get_point_count(), 0);
        // After add_line: Move (2) + Line (2) = 4
        p.add_line(PointF::new(0.0, 0.0), PointF::new(1.0, 1.0));
        assert_eq!(p.get_point_count(), 4);
        // + Bezier (4) = 8
        p.add_bezier(
            PointF::new(0.0, 0.0), PointF::new(1.0, 0.0),
            PointF::new(2.0, 0.0), PointF::new(3.0, 0.0),
        );
        assert_eq!(p.get_point_count(), 8);
        // + Close (0) = 8
        p.close();
        assert_eq!(p.get_point_count(), 8);
        // + Begin (0) = 8
        p.start();
        assert_eq!(p.get_point_count(), 8);
    }

    #[test]
    fn get_bounds_empty_path_returns_zero_rect() {
        let p = Path::new();
        let r = p.get_bounds();
        assert_eq!(r.origin, PointF::new(0.0, 0.0));
        assert_eq!(r.size_w, 0.0);
        assert_eq!(r.size_h, 0.0);
    }

    #[test]
    fn get_bounds_includes_all_points_modulo_move_zero() {
        // from_rect_f 의 시퀀스: Move(0,0,0,0) + 3 lines @ (10,20)-(110,70) + Close
        // Move(0,0) 가 bounds 에 (0,0) 포함시켜 좌상이 (0,0) 으로 확장됨.
        // raw helper 0x72c34 의 정확한 처리는 후속 RE 필요. 본 placeholder 동작 검증.
        let p = Path::from_rect_f(&RectF::new(10.0, 20.0, 100.0, 50.0));
        let r = p.get_bounds();
        assert_eq!(r.origin.x, 0.0);
        assert_eq!(r.origin.y, 0.0);
        assert_eq!(r.size_w, 110.0);
        assert_eq!(r.size_h, 70.0);
    }

    #[test]
    fn get_points_returns_sequence() {
        let mut p = Path::new();
        p.add_line(PointF::new(1.0, 2.0), PointF::new(3.0, 4.0));
        let pts = p.get_points();
        // Move(0,0,0,0) + Line(1,2,3,4) = [(0,0),(0,0),(1,2),(3,4)]
        assert_eq!(pts, vec![
            PointF::new(0.0, 0.0),
            PointF::new(0.0, 0.0),
            PointF::new(1.0, 2.0),
            PointF::new(3.0, 4.0),
        ]);
    }

    #[test]
    fn get_types_returns_subpath_marker_per_entry() {
        let mut p = Path::new();
        p.add_line(PointF::new(0.0, 0.0), PointF::new(1.0, 1.0));
        p.add_bezier(
            PointF::new(0.0, 0.0), PointF::new(1.0, 0.0),
            PointF::new(2.0, 0.0), PointF::new(3.0, 0.0),
        );
        p.close();
        p.start();
        let types = p.get_types();
        // Move (raw=0) + Line (raw=2) + Bezier (placeholder=3) + Close (placeholder=5) + Begin (placeholder=4)
        assert_eq!(types, vec![0, 2, 3, 5, 4]);
    }

    #[test]
    fn from_rect_f_capacity_remains_20_after_5_pushes() {
        // raw reserve(20) > 5 pushes 면 reallocation 없어야
        let p = Path::from_rect_f(&RectF::new(0.0, 0.0, 1.0, 1.0));
        let impl_data = unsafe { &*p.impl_ptr };
        assert!(impl_data.subpaths.capacity() >= 20);
    }

    #[test]
    fn dtor_handles_null_impl_ptr() {
        // Swap 후 한쪽이 null impl 을 갖는 시나리오는 raw cleanup 경로의 대상.
        // Rust 에서는 Drop 이 idempotent 해야.
        let mut p = Path::new();
        // SAFETY: 의도적 raw 시뮬레이션. 실제 메모리 leak 회피 위해 Box 해제 후 null.
        unsafe {
            let _ = Box::from_raw(p.impl_ptr);
            p.impl_ptr = std::ptr::null_mut();
        }
        drop(p);
    }
}
