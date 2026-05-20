//! `Hnc::Shape::Editor::PathUtil` — Render::Path → Shape::Path 변환 helpers.
//!
//! ## 출처 (libHncDrawingEngine.dylib arm64)
//!
//! - `FUN_0x7b674` (stripped, ~3060B): Render::PathImpl 의 segments 를
//!   flat PointF 시퀀스로 평탄화. RenderPathToPath 의 첫 호출.
//! - `FUN_0x7c26c` (stripped, ~3148B): 같은 segments 를 CGPath element type 시퀀스로
//!   평탄화 (각 점 별 type byte). RenderPathToPath 의 두 번째 호출.
//! - `PathUtil::RenderPathToPath` @ `0x10f5f4` (1840B): 두 평탄화 결과를 사용해
//!   `Shape::Path` 를 재구성. **L-5c-RE-1**, 본 모듈의 최종 entry.
//! - `PathUtil::ToPath` 4 overloads @ `0x10309c / 0x10fd24 / 0x10fe20 / 0x10374c`:
//!   RenderPathToPath 의 thin wrapper (출력 단순화). **L-5c-RE-2** 대상.
//!
//! ## RE 정공법 정책 (lib.rs 명시 + feedback_no_time_optimization.md)
//!
//! 본 모듈은 **byte-eq logic** (좌표 결정, type code, push 순서) 을 1:1 보존.
//! storage 만 idiomatic Rust (Vec, enum) 로 대체 — output 은 항상 raw asm 과
//! bit-equivalent (PointF u64 bit pattern, u8 type code).
//!
//! ## CGPath element type codes (raw 와 매칭)
//!
//! raw 가 `cmp w8, #0x3 / #0x1 / #0x0` 로 dispatch (PathUtil::RenderPathToPath @ 0x10f7d4-0x10f7ec):
//! - `0` = MoveToPoint (subpath 시작)
//! - `1` = AddLineToPoint
//! - `3` = AddCurveToPoint (cubic Bezier, 3 점 묶음)
//!
//! ## FUN_0x7b674 dispatch (vfunc[3] → 4 cases)
//!
//! jump table @ `0x74318f` = `[0x0, 0x24, 0x2, 0x13]` (raw 4-byte table):
//! - vfunc[3]=0 → case Begin (0x7b738): `mov w26, #1` (flag set, no push)
//! - vfunc[3]=1 → case Close (0x7b7c8): `str d8, [out_end], #8` (cached start point push)
//! - vfunc[3]=2 → case Line  (0x7b740): dynamic_cast LineSegment, push p1 (if flag) + p2
//! - vfunc[3]=3 → case Bezier(0x7b784): dynamic_cast BezierSegment, push p1 (if flag) + 3 cps
//!
//! 출력: `std::vector<u64>` (= packed PointF). 각 push = 8B.
//! 후처리: `flag |= !is_last` (loop tail). flag bit 0 은 normal path 에서 never cleared.
//!
//! ## FUN_0x7c26c dispatch (parallel to FUN_0x7b674)
//!
//! jump table @ `0x743193` = `[0x0, 0x12, 0x4, 0xb]`:
//! - case Begin (0x7c324): `mov w23, #1` (flag set, no push)
//! - case Close (0x7c36c): push **1** (`mov w8, #1; strb w8, [out_end], #1`) — LineTo cached
//! - case Line  (0x7c334): if flag: push **0** (Move), push **1** (Line). Else: push **1**
//! - case Bezier(0x7c350): if flag: push **0**, **3**, **3**, **3**. Else: push **3**, **3**, **3**
//!
//! 출력: `std::vector<u8>`. 각 push = 1B.
//! 후처리: 같은 `flag |= !is_last` 로직.
//!
//! ## 우리 Rust port 의 enum 매핑 (path.rs 의 Subpath)
//!
//! raw Subpath family vtable 의 vfunc[3] 결과는 직접 측정 안 됨. semantic 매핑:
//! - `Subpath::Begin` (raw StartSubpath @ vtable+0xa38) → vfunc[3]=0
//! - `Subpath::Close` (raw CloseSubpath @ vtable+0xa98) → vfunc[3]=1
//! - `Subpath::Move {..}` (raw LineSubpath type=0 @ vtable+0x960) → vfunc[3]=2
//!   (Move 와 Line 은 같은 클래스, type field 만 차이. vfunc[3] 은 class 별이므로 둘 다 2)
//! - `Subpath::Line {..}` (raw LineSubpath type=2) → vfunc[3]=2
//! - `Subpath::Bezier {..}` (raw BezierSubpath @ vtable+0x9a8) → vfunc[3]=3

use crate::path::{PathImpl, PointF, Subpath};

/// CGPath element type code (byte-eq raw).
pub mod type_code {
    /// `0` = MoveToPoint (subpath 시작).
    pub const MOVE: u8 = 0;
    /// `1` = AddLineToPoint.
    pub const LINE: u8 = 1;
    /// `3` = AddCurveToPoint (cubic Bezier cp/end).
    pub const BEZIER: u8 = 3;
}

/// `FUN_0x7b674` semantic port — Render::PathImpl 의 segments 를
/// flat PointF 시퀀스로 평탄화.
///
/// ## raw asm 출처 (libHncDrawingEngine.dylib @ 0x7b674-0x7c150)
///
/// - prologue: 0x7b674-0x7b6c8 (frame 0x90, w26=1 init, d8=0 init)
/// - empty check: 0x7b698-0x7b6a0 (vector size==0 → exit)
/// - main loop: 0x7b6c8-0x7c0fc (loop_idx x27 from 0)
/// - vfunc[3] dispatch: 0x7b6fc-0x7b734 (jump table @ 0x74318f)
/// - loop tail: 0x7c0fc-0x7c12c (refcount-- + dealloc)
/// - exit: 0x7c130-0x7c150 (frame restore + ret)
///
/// ## byte-eq output guarantee
///
/// 각 PointF 는 raw `ldr/str` 의 8B 와 bit-equivalent (`f32 x, f32 y` little-endian).
/// 순서도 raw 와 동일.
pub fn flatten_path_points(impl_obj: &PathImpl) -> Vec<PointF> {
    let n = impl_obj.subpaths.len();
    if n == 0 {
        // raw 0x7b698-0x7b6a0: subs x8, x8, x9; b.eq 0x7c130 — empty → 즉시 exit
        return Vec::new();
    }
    // raw 0x7b6c0: movi d8, #0
    let mut cached_d8 = PointF::new(0.0, 0.0);
    // raw 0x7b6c4: mov w26, #1
    let mut flag: u32 = 1;
    // 예상 출력 크기: 평균 2 points/segment (line) ~ 4 (bezier)
    let mut out = Vec::with_capacity(n * 2);

    for (i, sp) in impl_obj.subpaths.iter().enumerate() {
        let is_last = i == n - 1;
        match sp {
            // raw case 0 (vfunc[3]=0) @ 0x7b738: `mov w26, #1; b 0x7c0fc`
            // → flag = 1 unconditionally, no push, no d8 update
            Subpath::Begin => {
                flag = 1;
            }
            // raw case 1 (vfunc[3]=1) @ 0x7b7c8: push d8 (cached subpath start)
            // → 후처리에서 flag |= !is_last
            Subpath::Close => {
                out.push(cached_d8);
                flag |= if !is_last { 1 } else { 0 };
            }
            // raw case 2 (vfunc[3]=2) @ 0x7b740: LineSegment dynamic_cast
            // - flag=1 path: push p1 (LineSegment+8), push p2 (LineSegment+0x10)
            // - flag=0 path: push p2 only (raw at 0x7b8fc → 0x7bfc0, then `mov w26, #0`)
            // - d8 update (raw 0x7b8f0): `ldr d8, [x24, #8]` = LineSegment+8 = p1
            // Move 와 Line 둘 다 같은 raw vtable (LineSubpath) 이므로 동일 처리
            Subpath::Move { p1, p2 } | Subpath::Line { p1, p2 } => {
                if flag & 1 != 0 {
                    out.push(*p1);
                    out.push(*p2);
                    cached_d8 = *p1;
                    flag |= if !is_last { 1 } else { 0 };
                } else {
                    out.push(*p2);
                    // raw 0x7c0f4: mov w26, #0 (flag cleared in flag=0 path)
                    flag = 0;
                }
            }
            // raw case 3 (vfunc[3]=3) @ 0x7b784: BezierSegment dynamic_cast
            // - flag=1 path: push p1, p2, p3, p4 (offsets +8, +0x10, +0x18, +0x20)
            // - flag=0 path: push p2, p3, p4 only
            // - d8 = p1
            Subpath::Bezier { p1, p2, p3, p4 } => {
                if flag & 1 != 0 {
                    out.push(*p1);
                    out.push(*p2);
                    out.push(*p3);
                    out.push(*p4);
                    cached_d8 = *p1;
                    flag |= if !is_last { 1 } else { 0 };
                } else {
                    out.push(*p2);
                    out.push(*p3);
                    out.push(*p4);
                    flag = 0;
                }
            }
        }
    }
    out
}

/// `FUN_0x7c26c` semantic port — Render::PathImpl 의 segments 를
/// CGPath element type byte 시퀀스로 평탄화.
///
/// ## raw asm 출처 (libHncDrawingEngine.dylib @ 0x7c26c-0x7ceb8)
///
/// - prologue: 0x7c26c-0x7c2b4 (frame 0x70, w23=1 init)
/// - 동일한 loop + dispatch 구조. jump table @ 0x743193 = [0x0, 0x12, 0x4, 0xb]
///
/// ## type codes (byte-eq raw)
///
/// raw 의 case 별 push 값:
/// - Begin: no push
/// - Close: push **1** (raw 0x7c378 `mov w8, #1; strb w8`)
/// - Line: if flag: push **0** (0x7c344 `strb wzr`) + push **1** (0x7c590 `mov w8, #1; strb`).
///   Else: push **1** (직접 0x7c588 으로 점프).
/// - Bezier: if flag: push **0** + push **3** × 3.
///   Else: push **3** × 3.
///
/// ## 출력 길이 = `flatten_path_points` 결과 길이
///
/// 두 함수는 parallel arrays — RenderPathToPath 가 `points[i]` 와 `types[i]` 를 함께 소비.
pub fn flatten_path_types(impl_obj: &PathImpl) -> Vec<u8> {
    let n = impl_obj.subpaths.len();
    if n == 0 {
        return Vec::new();
    }
    let mut flag: u32 = 1;
    let mut out = Vec::with_capacity(n * 2);

    for (i, sp) in impl_obj.subpaths.iter().enumerate() {
        let is_last = i == n - 1;
        match sp {
            Subpath::Begin => {
                flag = 1;
            }
            // Close → push 1 (LineTo cached_start)
            Subpath::Close => {
                out.push(type_code::LINE);
                flag |= if !is_last { 1 } else { 0 };
            }
            Subpath::Move { .. } | Subpath::Line { .. } => {
                if flag & 1 != 0 {
                    out.push(type_code::MOVE);
                    out.push(type_code::LINE);
                    flag |= if !is_last { 1 } else { 0 };
                } else {
                    out.push(type_code::LINE);
                    flag = 0;
                }
            }
            Subpath::Bezier { .. } => {
                if flag & 1 != 0 {
                    out.push(type_code::MOVE);
                    out.push(type_code::BEZIER);
                    out.push(type_code::BEZIER);
                    out.push(type_code::BEZIER);
                    flag |= if !is_last { 1 } else { 0 };
                } else {
                    out.push(type_code::BEZIER);
                    out.push(type_code::BEZIER);
                    out.push(type_code::BEZIER);
                    flag = 0;
                }
            }
        }
    }
    out
}

/// `flatten_path_points` + `flatten_path_types` 동시 실행 (parallel iteration).
///
/// 두 함수의 출력은 길이가 같음 (parallel arrays). 본 헬퍼는 단일 loop 로 묶어
/// 호출 cost 를 줄임. RenderPathToPath 가 두 결과를 즉시 함께 소비하므로
/// 분리/통합 모두 byte-eq output 동일.
pub fn flatten_path(impl_obj: &PathImpl) -> (Vec<PointF>, Vec<u8>) {
    let points = flatten_path_points(impl_obj);
    let types = flatten_path_types(impl_obj);
    debug_assert_eq!(
        points.len(),
        types.len(),
        "FUN_0x7b674 / FUN_0x7c26c 출력 길이 불일치 — parallel array invariant 위반"
    );
    (points, types)
}

// ============================================================================
// PathUtil::RenderPathToPath (L-5c-RE-1) — 1840B byte-eq port
// ============================================================================

use crate::logical_position::LogicalPosition;
use crate::path::Path as RenderPath;
use crate::shape_engine;
use crate::shape_path::Path as ShapePath;
use crate::shape_path::SizeF;

/// `RenderPathToPath` 의 x8 SRET aux output struct — 20B layout.
///
/// ## raw layout (caller 의 sp+0x8 에서 관찰)
///
/// raw 의 caller (`ToPath` wrapper @ 0x10309c) 가 `add x8, sp, #0x8` 으로 SRET pointer 지정.
/// RenderPathToPath 내부에서 다음과 같이 채워짐:
/// - `+0x00` (1B): `valid: bool` — `strb wzr/w8, [x19]` (start: 0, end: 1 if bbox ok)
/// - `+0x01..0x04` (3B): padding (raw 가 garbage 로 둠, 본 port 는 zero-init)
/// - `+0x04` (8B): `point: PointF` — `stp s2, s0, [x19, #0x4]` (bbox origin scaled)
/// - `+0x0c` (8B): `size: SizeF` — `stur d0, [x19, #0xc]` (bbox extent scaled)
///
/// ## caller 의 사용 (raw `ToPath` @ 0x10309c-0x103154)
/// ```text
/// ldur d0, [sp, #0x14] ; d0 = aux.size (sp+0x8+0xc)
/// str d0, [x22, #0x8]  ; Shape::Path.size = aux.size
/// ldur d0, [sp, #0xc]  ; d0 = aux.point (sp+0x8+0x4)
/// str d0, [x19]        ; *out_PointImpl = aux.point
/// ```
///
/// ## 본 port 의 의미
///
/// `valid=true` 일 때 `point/size` 는 변환된 CGPath 의 tight bounding box (logical 단위).
/// `valid=false` 이면 caller 가 *out_PointImpl/Path->size 를 zero 로 둠 (raw 의 fallthrough).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPathAuxOutput {
    /// `+0x00` — valid flag (1B). false 이면 point/size 무효.
    pub valid: bool,
    /// `+0x01..+0x04` — padding (raw 가 garbage 인 부분, 본 port 는 zero-init).
    pub(crate) _pad: [u8; 3],
    /// `+0x04` — CGPath bbox origin (scaled to logical units).
    pub point: crate::path::PointF,
    /// `+0x0c` — CGPath bbox extent (width, height; scaled to logical units).
    pub size: SizeF,
}

pub const RENDER_PATH_AUX_OUTPUT_SIZE_BYTES: usize = 20;

const _: () = assert!(
    std::mem::size_of::<RenderPathAuxOutput>() == RENDER_PATH_AUX_OUTPUT_SIZE_BYTES,
    "RenderPathAuxOutput size mismatch"
);

impl Default for RenderPathAuxOutput {
    fn default() -> Self {
        Self {
            valid: false,
            _pad: [0; 3],
            point: crate::path::PointF::new(0.0, 0.0),
            size: SizeF::default(),
        }
    }
}

impl RenderPathAuxOutput {
    pub fn new_invalid() -> Self {
        Self::default()
    }
}

/// raw constant `0x42c00000` = 96.0f — pixel-to-inch divisor (96 DPI).
const PIXEL_TO_INCH_DIVISOR: f32 = 96.0;

/// raw type byte mask: `w8 = w27 & 0x7` @ `0x10f7d4` — low 3 bits = element type,
/// high bits = smooth-corner flag (bit 0x80 = signed-negative byte).
const TYPE_MASK: u8 = 0x7;

/// `Hnc::Shape::Editor::PathUtil::RenderPathToPath` @ `0x10f5f4` sz=1840B.
///
/// ## raw 시그니처
/// ```cpp
/// void RenderPathToPath(
///     Hnc::Shape::Path& out,
///     Hnc::Shape::Render::Path const& src,
///     float scale,
///     bool b1,   // type==0 (Move) 처리 여부: true → out.Start() 호출
///     bool b2,   // post-loop "snap last segment to last point" 여부 (b2=true 시 활성)
///     bool b3    // true → smoothing branch (AppleSmooth Bezier @0x79fa4 후 재귀 호출)
/// );
/// ```
///
/// ## raw branch 구조
/// - `b3=true` (0x10f69c): 새 Render::Path 만들어 `0x79fa4` (smooth Bezier, tension=0.2)
///   적용 후 자신을 재귀 호출 (`b3=false`).
/// - `b3=false` (0x10f71c): **본 함수의 핵심**. flatten + 직접 변환.
///   - `FUN_0x7b674` (flatten_path_points) 호출 → `Vec<PointF>`
///   - `FUN_0x7c26c` (flatten_path_types) 호출 → `Vec<u8>`
///   - 각 (point, type) 페어를 type 별로 dispatch:
///     - `type & 7 == 0` (Move): `b1=true` 면 `out.Start(scaled)`. 아니면 skip
///     - `type & 7 == 1` (Line): `out.AddLine(scaled)`
///     - `type & 7 == 3` (Bezier): 추가로 2점 더 읽어 `out.AddBezier(cp1, cp2, end)` (3점 consume)
///   - 좌표 변환: `scaled = pt * ShapeEngine.unit / 96.0`
///   - type 의 high bit (0x80, signed-negative) = smooth-corner 마커:
///     last segment 의 +0x2 word 에 1 저장 (0x10f94c-0x10f960 / 0x10f96c-0x10f980)
///   - post-loop (`b2=true && count >= 17`): last segment 의 endpoint 를
///     last point 와 비교, 임계값 초과 시 `SetLastPosition` 호출
/// - 마지막 (모든 branch): `_CGPathRelease` + 외부 출력 struct 채움 (x19 SRET, 21B+)
///
/// ## 본 port 의 현재 범위 (L-5c-RE-1a, 본 commit)
///
/// - `b3=false` branch 의 main loop (type dispatch)
/// - 좌표 scale (ShapeEngine.unit / 96.0)
/// - Start / Line / Bezier dispatch
///
/// ## 보류 (다음 commit, L-5c-RE-1b)
///
/// - `b3=true` smoothing branch (0x79fa4 AppleSmooth helper RE 필요, ~300B)
/// - post-loop "snap last position" (0x10f988-0x10faa8, ~200B):
///   `Segment::SetLastPosition` 호출 + threshold 비교
/// - type high-bit smooth-corner marker (0x10f94c-0x10f980): last segment +0x2 word set
/// - x19 SRET auxiliary output (path-empty flag + RectF) — caller 의 별도 출력. void return
///   인데 x8(SRET) 사용 — 컴파일러 ABI 특수 케이스
/// - `_CGPathRelease` / `_CGPathIsEmpty` / `_CGPathGetPathBoundingBox` 호출 (macOS 의존)
///
/// ## byte-eq output guarantee (현재 범위 한정)
///
/// 본 port 의 main loop 는 raw 와 같은 순서로 `out.Start/AddLine/AddBezier` 호출,
/// 좌표 변환도 같은 scale 적용. 따라서 out.segments 의 (style, LogicalPosition coords)
/// 시퀀스가 byte-eq.
///
/// # Arguments
/// - `out`: 채워질 Shape::Path (mutable reference)
/// - `src`: 원본 Render::Path (const reference)
/// - `_scale`: unused in b3=false branch (raw 의 s8). b3=true smoothing 에 쓰임
/// - `b1`: Move type 처리 여부
/// - `_b2`: post-loop "snap last position" 활성 — 미구현
/// - `b3`: true 이면 smoothing branch (현재 미구현 — panic)
pub fn render_path_to_path(
    out: &mut ShapePath,
    src: &RenderPath,
    _scale: f32,
    b1: bool,
    _b2: bool,
    b3: bool,
) {
    if b3 {
        // raw 0x10f698 b3=true branch — smoothing 미구현
        todo!("RenderPathToPath b3=true branch (smoothing via 0x79fa4 AppleSmooth) not yet ported");
    }

    // raw 0x10f71c 진입 (b3=false branch)
    //
    // x21 = src (Render::Path const&), x21+0x10 = src.impl_ptr (PathImpl*)
    // PathImpl* 가 null 이면 비정상 — 본 port 는 panic 대신 빈 결과 처리
    let impl_ptr = src.impl_ptr;
    if impl_ptr.is_null() {
        return;
    }
    let impl_obj = unsafe { &*impl_ptr };

    // raw 0x10f730: bl 0x7b674 — flatten_path_points
    // raw 0x10f744: bl 0x7c26c — flatten_path_types
    let points = flatten_path_points(impl_obj);
    let types = flatten_path_types(impl_obj);
    debug_assert_eq!(points.len(), types.len());

    // raw 0x10f74c: subs x8, x8, x9; b.eq 0x10f988 — empty (count==0) 시 post-loop 로 점프
    if points.is_empty() {
        // 본 port 는 post-loop 미구현 — empty 면 그냥 return
        return;
    }

    // raw 0x10f764-0x10f768: v9 = dup.2s(0x42c00000) = (96.0, 96.0) constant for fdiv
    // raw 의 unit fetch:
    //   bl ShapeEngine::GetInstance; ldur d0, [x0, #4]
    // 본 port 는 동일하게 singleton 의 unit 사용 (caching 가능하지만 일단 매 iter 호출 매칭)
    let unit = shape_engine::read_instance().get_logical_dpi();

    let count = points.len();
    let mut i = 0usize;
    // raw main loop @ 0x10f77c-0x10f984
    while i < count {
        let pt = points[i];
        // raw 0x10f7b4-0x10f7c8: v0 = (pt.x * unit / 96, pt.y * unit / 96)
        let scaled_pt = scale_point(pt, unit);

        // raw 0x10f7d0: ldrb w27 = types[i] (signed byte semantic)
        let type_byte = types[i];
        // raw 0x10f7d4: and w8, w27, #0x7 — low 3 bits
        let type_low = type_byte & TYPE_MASK;

        match type_low {
            // raw 0x10f83c Bezier branch
            3 => {
                // raw 0x10f83c: add x22, x25, #2; cmp x28, x22; b.ls 0x10f770
                // i+2 가 범위 밖이면 loop 계속 (= skip)
                if i + 2 >= count {
                    i += 1;
                    continue;
                }
                let pt_i1 = points[i + 1];
                let pt_i2 = points[i + 2];
                let scaled_cp1 = scaled_pt; // pts[i] = first control point
                let scaled_cp2 = scale_point(pt_i1, unit);
                let scaled_end = scale_point(pt_i2, unit);

                // raw 0x10f8ec-0x10f938: alloc 3 LogicalPositions + AddBezier
                out.add_bezier(
                    LogicalPosition::from_point(scaled_cp1),
                    LogicalPosition::from_point(scaled_cp2),
                    LogicalPosition::from_point(scaled_end),
                );

                // raw 0x10f93c: mov x25, x22 (= i + 2). loop ++ → i += 3
                i += 3;
                // TODO (L-5c-RE-1b): high-bit smooth-corner marker on last segment
                continue;
            }
            // raw 0x10f818 Line branch
            1 => {
                // raw 0x10f818-0x10f834: alloc LogicalPosition + AddLine
                out.add_line(LogicalPosition::from_point(scaled_pt));
                i += 1;
                // TODO (L-5c-RE-1b): high-bit smooth-corner marker
                continue;
            }
            // raw 0x10f7e8 Start (Move) branch
            0 => {
                // raw 0x10f7ec: ccmp w23, #0, #4, eq — b1==true 시에만 Start 실행
                if b1 {
                    out.start(LogicalPosition::from_point(scaled_pt));
                }
                // b1=false 면 skip (raw 는 0x10f964 으로 점프해 diagnostic 만)
                i += 1;
                continue;
            }
            _ => {
                // raw 의 ccmp 로 처리 (any other type → skip).
                // flatten_path_types 가 0/1/3 만 emit 하므로 이 case 는 실제 도달 안 함.
                i += 1;
            }
        }
    }
    // TODO (L-5c-RE-1b): post-loop "snap last position" (b2=true && count >= 17)
    // TODO (L-5c-RE-1b): x19 SRET auxiliary output (path-empty flag + bbox)
    // TODO (L-5c-RE-1c): CGPath cleanup (raw 0x10fab0-0x10fae4) — macOS dep, 본 port 불필요
}

/// 점 좌표를 logical (96-DPI invariant) 단위로 변환.
///
/// raw 식: `scaled = pt * engine.unit / 96.0` (정확히는 SIMD 2-lane fmul + fdiv).
///
/// raw 의 SIMD 흐름:
/// ```text
/// v0 = (pt.x, pt.y)        ; mov.s v0[1], v1[0]
/// v1 = (unit, unit)        ; ld1.s {v1}[1], [engine+4]
/// v0 = v0 * v1             ; fmul.2s
/// v9 = (96.0, 96.0)        ; dup.2s of 0x42c00000
/// v0 = v0 / v9             ; fdiv.2s
/// ```
#[inline]
fn scale_point(pt: PointF, unit: f32) -> PointF {
    PointF::new(
        pt.x * unit / PIXEL_TO_INCH_DIVISOR,
        pt.y * unit / PIXEL_TO_INCH_DIVISOR,
    )
}

#[cfg(test)]
mod render_path_to_path_tests {
    use super::*;
    use crate::path::Path as RenderPath;
    use crate::shape_path::{Path as ShapePath, SizeF};

    fn make_empty_out() -> ShapePath {
        ShapePath::new(0, false, SizeF::default(), false, 0.0)
    }

    #[test]
    fn empty_src_produces_empty_out() {
        let src = RenderPath::new(); // empty PathImpl
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, false, false, false);
        assert_eq!(out.get_count(), 0);
    }

    #[test]
    fn single_line_b1_false_skips_moves_produces_two_lines() {
        // Render::Path::add_line raw 0x792dc — empty 면 Move(0,0) + Line(96,0)→(192,0) 둘 다 push.
        // flatten with flag=1:
        //   Move{(0,0),(0,0)}: points [(0,0),(0,0)], types [0,1]
        //   Line{(96,0),(192,0)}: points [(96,0),(192,0)], types [0,1]
        // 총 4 points + 4 types [0,1,0,1].
        // b1=false → type=0 skip, type=1 = AddLine. 결과: 2 AddLine.
        let mut src = RenderPath::new();
        src.add_line(PointF::new(96.0, 0.0), PointF::new(192.0, 0.0));
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, false, false, false);
        assert_eq!(out.get_count(), 2);
        assert_eq!(out.get_at(0).unwrap().style, 1); // Line
        assert_eq!(out.get_at(1).unwrap().style, 1); // Line
        unsafe {
            // first AddLine: scaled (0, 0)
            assert_eq!((*out.get_at(0).unwrap().pos0).get_x(), 0.0);
            // second AddLine: scaled (192/96, 0) = (2, 0)
            assert_eq!((*out.get_at(1).unwrap().pos0).get_x(), 2.0);
        }
    }

    #[test]
    fn single_line_b1_true_produces_start_line_start_line() {
        let mut src = RenderPath::new();
        src.add_line(PointF::new(96.0, 0.0), PointF::new(192.0, 0.0));
        let mut out = make_empty_out();
        // b1=true → Start + Line + Start + Line = 4 segments
        render_path_to_path(&mut out, &src, 1.0, true, false, false);
        assert_eq!(out.get_count(), 4);
        assert_eq!(out.get_at(0).unwrap().style, 0); // Start
        assert_eq!(out.get_at(1).unwrap().style, 1); // Line
        assert_eq!(out.get_at(2).unwrap().style, 0); // Start
        assert_eq!(out.get_at(3).unwrap().style, 1); // Line
    }

    #[test]
    fn empty_render_path_produces_empty_shape_path() {
        let src = RenderPath::new();
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, true, false, false);
        assert_eq!(out.get_count(), 0);
    }

    #[test]
    #[should_panic(expected = "smoothing")]
    fn b3_true_panics_smoothing_not_yet_ported() {
        let src = RenderPath::new();
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, false, false, true);
    }

    #[test]
    fn scale_point_divides_by_96() {
        let scaled = scale_point(PointF::new(96.0, 192.0), 1.0);
        assert_eq!(scaled.x, 1.0);
        assert_eq!(scaled.y, 2.0);
    }

    #[test]
    fn scale_point_respects_unit() {
        // unit=2.0 → output = pt * 2 / 96
        let scaled = scale_point(PointF::new(96.0, 0.0), 2.0);
        assert_eq!(scaled.x, 2.0);
        assert_eq!(scaled.y, 0.0);
    }

    #[test]
    fn bezier_consumes_three_points_at_correct_offsets() {
        // Render::Path 에 add_bezier → Bezier{p1, p2, p3, p4} subpath 1개 push
        // flatten with flag=1: 4 points [p1, p2, p3, p4] + 4 types [0, 3, 3, 3]
        // loop:
        //   i=0 type=0 b1=false → skip
        //   i=1 type=3 → AddBezier(p2, p3, p4). i += 3 → i=4. exit.
        // out = 1 Bezier segment
        let mut src = RenderPath::new();
        src.add_bezier(
            PointF::new(0.0, 0.0),
            PointF::new(96.0, 0.0),
            PointF::new(96.0, 96.0),
            PointF::new(0.0, 96.0),
        );
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, false, false, false);
        assert_eq!(out.get_count(), 1);
        let seg = out.get_at(0).unwrap();
        assert_eq!(seg.style, 3); // Bezier
        unsafe {
            // cp1 = p2 = (96,0) → scaled (1, 0)
            assert_eq!((*seg.pos0).get_x(), 1.0);
            assert_eq!((*seg.pos0).get_y(), 0.0);
            // cp2 = p3 = (96, 96) → (1, 1)
            assert_eq!((*seg.pos1).get_x(), 1.0);
            assert_eq!((*seg.pos1).get_y(), 1.0);
            // end = p4 = (0, 96) → (0, 1)
            assert_eq!((*seg.pos2).get_x(), 0.0);
            assert_eq!((*seg.pos2).get_y(), 1.0);
        }
    }

    #[test]
    fn bezier_with_b1_true_produces_start_plus_bezier() {
        let mut src = RenderPath::new();
        src.add_bezier(
            PointF::new(0.0, 0.0),
            PointF::new(96.0, 0.0),
            PointF::new(96.0, 96.0),
            PointF::new(0.0, 96.0),
        );
        let mut out = make_empty_out();
        render_path_to_path(&mut out, &src, 1.0, true, false, false);
        // i=0 type=0 b1=true → Start(0,0)
        // i=1 type=3 → AddBezier(...). i=4. exit.
        // 2 segments: Start + Bezier
        assert_eq!(out.get_count(), 2);
        assert_eq!(out.get_at(0).unwrap().style, 0);
        assert_eq!(out.get_at(1).unwrap().style, 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::PathImpl;

    fn p(x: f32, y: f32) -> PointF {
        PointF::new(x, y)
    }

    fn make_impl(subs: Vec<Subpath>) -> PathImpl {
        let mut impl_obj = PathImpl::new();
        impl_obj.subpaths = subs;
        impl_obj
    }

    // --- flatten_path_points: empty ---

    #[test]
    fn flatten_points_empty_returns_empty() {
        let impl_obj = make_impl(vec![]);
        assert_eq!(flatten_path_points(&impl_obj), Vec::<PointF>::new());
    }

    #[test]
    fn flatten_types_empty_returns_empty() {
        let impl_obj = make_impl(vec![]);
        assert_eq!(flatten_path_types(&impl_obj), Vec::<u8>::new());
    }

    // --- single Line (initial flag=1, is_last=true) ---

    #[test]
    fn single_line_emits_start_and_end_points() {
        let impl_obj = make_impl(vec![Subpath::Line {
            p1: p(1.0, 2.0),
            p2: p(3.0, 4.0),
        }]);
        // flag=1 (initial) AND is_last=true → flag |= 0 → flag stays 1, but already pushed p1+p2
        let points = flatten_path_points(&impl_obj);
        assert_eq!(points, vec![p(1.0, 2.0), p(3.0, 4.0)]);
    }

    #[test]
    fn single_line_emits_move_then_line_type() {
        let impl_obj = make_impl(vec![Subpath::Line {
            p1: p(1.0, 2.0),
            p2: p(3.0, 4.0),
        }]);
        let types = flatten_path_types(&impl_obj);
        assert_eq!(types, vec![type_code::MOVE, type_code::LINE]);
    }

    // --- single Bezier ---

    #[test]
    fn single_bezier_emits_four_points() {
        let impl_obj = make_impl(vec![Subpath::Bezier {
            p1: p(0.0, 0.0),
            p2: p(1.0, 1.0),
            p3: p(2.0, 2.0),
            p4: p(3.0, 3.0),
        }]);
        let points = flatten_path_points(&impl_obj);
        assert_eq!(
            points,
            vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0), p(3.0, 3.0)]
        );
    }

    #[test]
    fn single_bezier_emits_move_then_three_bezier_types() {
        let impl_obj = make_impl(vec![Subpath::Bezier {
            p1: p(0.0, 0.0),
            p2: p(1.0, 1.0),
            p3: p(2.0, 2.0),
            p4: p(3.0, 3.0),
        }]);
        let types = flatten_path_types(&impl_obj);
        assert_eq!(
            types,
            vec![
                type_code::MOVE,
                type_code::BEZIER,
                type_code::BEZIER,
                type_code::BEZIER
            ]
        );
    }

    // --- parallel arrays invariant ---

    #[test]
    fn parallel_arrays_same_length_single_line() {
        let impl_obj = make_impl(vec![Subpath::Line {
            p1: p(1.0, 2.0),
            p2: p(3.0, 4.0),
        }]);
        let (pts, types) = flatten_path(&impl_obj);
        assert_eq!(pts.len(), types.len());
    }

    #[test]
    fn parallel_arrays_same_length_bezier() {
        let impl_obj = make_impl(vec![Subpath::Bezier {
            p1: p(0.0, 0.0),
            p2: p(1.0, 1.0),
            p3: p(2.0, 2.0),
            p4: p(3.0, 3.0),
        }]);
        let (pts, types) = flatten_path(&impl_obj);
        assert_eq!(pts.len(), types.len());
        assert_eq!(pts.len(), 4);
    }

    // --- multi-segment, flag mechanism ---

    #[test]
    fn two_lines_flag_stays_set() {
        // flag |= !is_last — for i=0 (not last), flag stays 1; for i=1 (last), flag stays 1
        // Both Lines emit [start, end] = 2 points each
        let impl_obj = make_impl(vec![
            Subpath::Line {
                p1: p(0.0, 0.0),
                p2: p(1.0, 1.0),
            },
            Subpath::Line {
                p1: p(2.0, 2.0),
                p2: p(3.0, 3.0),
            },
        ]);
        let points = flatten_path_points(&impl_obj);
        assert_eq!(
            points,
            vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0), p(3.0, 3.0)]
        );
    }

    #[test]
    fn two_lines_types_are_alternating() {
        let impl_obj = make_impl(vec![
            Subpath::Line {
                p1: p(0.0, 0.0),
                p2: p(1.0, 1.0),
            },
            Subpath::Line {
                p1: p(2.0, 2.0),
                p2: p(3.0, 3.0),
            },
        ]);
        let types = flatten_path_types(&impl_obj);
        assert_eq!(
            types,
            vec![
                type_code::MOVE,
                type_code::LINE,
                type_code::MOVE,
                type_code::LINE,
            ]
        );
    }

    // --- Close uses cached d8 (= last LineSegment.p1) ---

    #[test]
    fn close_pushes_cached_d8_from_previous_line() {
        // Line {p1=(5,5), p2=(6,6)} → d8 = (5,5)
        // Close → push d8 = (5,5)
        let impl_obj = make_impl(vec![
            Subpath::Line {
                p1: p(5.0, 5.0),
                p2: p(6.0, 6.0),
            },
            Subpath::Close,
        ]);
        let points = flatten_path_points(&impl_obj);
        assert_eq!(points, vec![p(5.0, 5.0), p(6.0, 6.0), p(5.0, 5.0)]);
    }

    #[test]
    fn close_emits_line_type_not_move() {
        // Close maps to type 1 (LineTo cached_start) — raw 0x7c378 `mov w8, #1`
        let impl_obj = make_impl(vec![
            Subpath::Line {
                p1: p(1.0, 2.0),
                p2: p(3.0, 4.0),
            },
            Subpath::Close,
        ]);
        let types = flatten_path_types(&impl_obj);
        assert_eq!(
            types,
            vec![type_code::MOVE, type_code::LINE, type_code::LINE]
        );
    }

    // --- Begin (StartSubpath) ---

    #[test]
    fn begin_alone_emits_nothing() {
        let impl_obj = make_impl(vec![Subpath::Begin]);
        let (pts, types) = flatten_path(&impl_obj);
        assert!(pts.is_empty());
        assert!(types.is_empty());
    }

    #[test]
    fn begin_resets_flag_before_line() {
        // Begin alone doesn't push. Then Line pushes 2 points + types [Move, Line].
        let impl_obj = make_impl(vec![
            Subpath::Begin,
            Subpath::Line {
                p1: p(1.0, 2.0),
                p2: p(3.0, 4.0),
            },
        ]);
        let (pts, types) = flatten_path(&impl_obj);
        assert_eq!(pts, vec![p(1.0, 2.0), p(3.0, 4.0)]);
        assert_eq!(types, vec![type_code::MOVE, type_code::LINE]);
    }

    // --- Move (raw LineSubpath type=0, treated identically to Line) ---

    #[test]
    fn move_variant_same_as_line() {
        // Move 와 Line 은 같은 raw vtable (LineSubpath) — vfunc[3] 같은 값 반환
        let line = make_impl(vec![Subpath::Line {
            p1: p(1.0, 2.0),
            p2: p(3.0, 4.0),
        }]);
        let mv = make_impl(vec![Subpath::Move {
            p1: p(1.0, 2.0),
            p2: p(3.0, 4.0),
        }]);
        assert_eq!(flatten_path_points(&line), flatten_path_points(&mv));
        assert_eq!(flatten_path_types(&line), flatten_path_types(&mv));
    }

    // --- realistic: rectangle via 4 Lines + Close ---

    #[test]
    fn rectangle_via_four_lines_and_close_emits_polygon() {
        // 사각형: (0,0)→(10,0)→(10,10)→(0,10)→Close(back to (0,0))
        let impl_obj = make_impl(vec![
            Subpath::Line { p1: p(0.0, 0.0), p2: p(10.0, 0.0) },
            Subpath::Line { p1: p(10.0, 0.0), p2: p(10.0, 10.0) },
            Subpath::Line { p1: p(10.0, 10.0), p2: p(0.0, 10.0) },
            Subpath::Line { p1: p(0.0, 10.0), p2: p(0.0, 0.0) },
            Subpath::Close,
        ]);
        let (pts, types) = flatten_path(&impl_obj);
        // 각 Line 마다 [start, end] = 2 points (flag stays 1 throughout). Close = 1 point (d8 = last Line.p1 = (0,10)).
        assert_eq!(
            pts,
            vec![
                p(0.0, 0.0), p(10.0, 0.0),
                p(10.0, 0.0), p(10.0, 10.0),
                p(10.0, 10.0), p(0.0, 10.0),
                p(0.0, 10.0), p(0.0, 0.0),
                p(0.0, 10.0),  // Close pushes cached d8 = last Line.p1
            ]
        );
        assert_eq!(
            types,
            vec![
                type_code::MOVE, type_code::LINE,
                type_code::MOVE, type_code::LINE,
                type_code::MOVE, type_code::LINE,
                type_code::MOVE, type_code::LINE,
                type_code::LINE,
            ]
        );
    }

    // --- mixed Line + Bezier ---

    #[test]
    fn line_then_bezier_emits_correct_sequence() {
        let impl_obj = make_impl(vec![
            Subpath::Line { p1: p(0.0, 0.0), p2: p(1.0, 0.0) },
            Subpath::Bezier {
                p1: p(1.0, 0.0),
                p2: p(2.0, 0.5),
                p3: p(3.0, 1.5),
                p4: p(4.0, 2.0),
            },
        ]);
        let (pts, types) = flatten_path(&impl_obj);
        assert_eq!(
            pts,
            vec![
                p(0.0, 0.0), p(1.0, 0.0),                    // Line
                p(1.0, 0.0), p(2.0, 0.5), p(3.0, 1.5), p(4.0, 2.0),  // Bezier
            ]
        );
        assert_eq!(
            types,
            vec![
                type_code::MOVE, type_code::LINE,
                type_code::MOVE, type_code::BEZIER, type_code::BEZIER, type_code::BEZIER,
            ]
        );
    }

    // --- byte-eq guarantee: PointF bit pattern ---

    #[test]
    fn pointf_emitted_with_exact_bit_pattern() {
        // raw `str x8, [end]` 는 8B (PointF 의 두 f32 little-endian).
        // PointF 의 x, y 필드 ordering 이 그대로 유지되는지 확인.
        let p1 = p(1.5_f32, 2.5_f32);
        let impl_obj = make_impl(vec![Subpath::Line { p1, p2: p(3.0, 4.0) }]);
        let pts = flatten_path_points(&impl_obj);
        assert_eq!(pts[0].x, 1.5);
        assert_eq!(pts[0].y, 2.5);
        // bit-pattern test
        let bits1 = pts[0].x.to_bits();
        let bits2 = pts[0].y.to_bits();
        assert_eq!(bits1, 1.5_f32.to_bits());
        assert_eq!(bits2, 2.5_f32.to_bits());
    }
}
