//! `Hnc::Shape::Path` — Shape::Path (Render::Path 와 별개의 high-level path).
//!
//! ## raw 출처 (libHncDrawingEngine.dylib arm64)
//!
//! - 4 ctor variants:
//!   - `Path(Style, bool, SizeF, bool, float)` @ `0x1096cc` sz=36B (default-like)
//!   - `Path(RectF, Style, bool, bool)` @ `0x1681d8` sz=104B
//!   - `Path(LogicalPosition*, LogicalPosition*, Style, bool, SizeF, bool)` @ `0x151b80` sz=296B
//!   - `Path(CHncStringW, Style, bool, SizeF, bool, bool)` @ `0x1b00a8` sz=4488B (formula)
//! - Copy ctor `Path(Path const&)` @ `0x16ab38 / 0x1b12b0` (small + large variants)
//! - Dtor `~Path()` @ `0x16ab3c / 0x1b14f0` sz=112B
//! - Element ops:
//!   - `Start(LogicalPosition*)` @ `0x10973c` sz=416B
//!   - `AddLine(LogicalPosition*)` @ `0x1098e0` sz=420B
//!   - `AddBezier(LogicalPosition*, LogicalPosition*, LogicalPosition*)` @ `0x10ff44` sz=428B
//!   - `AddQuadraticBezier(LogicalPosition*, LogicalPosition*)` @ `0x1b15b8` sz=456B
//!   - `Close()` @ `0x109a84` sz=28B
//! - Container ops:
//!   - `GetAt(usize)` @ `0xd090c` sz=120B
//!   - `GetCount()` @ `0xd08fc` sz=16B
//!   - `Insert(usize, Segment*)` @ `0x114874` sz=44B
//!   - `Remove(usize)` @ `0x115a04` sz=96B
//!   - `Begin/End` @ `0xbdf94/0xbdf9c` sz=8B
//! - Property:
//!   - `GetStyle/GetLight` @ `0x14afa0/0x14afa8` sz=8B
//!   - `GetSize/SetSize` @ `0xd08c4/0xd0bd0` sz=56B/12B
//!   - `GetStrokeStyle/SetStrokeStyle` @ `0x1b1570/0x1b1578` sz=8B
//!   - `GetExtrusionOk` @ `0x1b1580` sz=8B
//!   - `IsEmpty` @ `0x1b1560` sz=16B
//!
//! ## raw layout (48B, raw `0x1096cc` ctor 분석)
//!
//! | offset | size | field | init |
//! |--------|------|-------|------|
//! | +0x00  | 4B   | `style: u32` (Render::Path::Style) | param 1 |
//! | +0x04  | 1B   | `stroke_style: u8` | param 2 |
//! | +0x05  | 1B   | `extrusion_ok: u8` | param 4 |
//! | +0x06-07 | 2B | padding | — |
//! | +0x08  | 8B   | `size: SizeF` (`{width: f32, height: f32}`) | `*(SizeF*)x3` |
//! | +0x10  | 4B   | `light: f32` | param 5 (s0) |
//! | +0x14-17 | 4B | padding | — |
//! | +0x18  | 8B   | (zero — `str xzr`) | 0 |
//! | +0x20  | 16B  | vector begin/end (`stp xzr, xzr`) | 0/0 |
//!
//! raw 의 vector 는 `std::vector<Segment*>` (libc++ 3-ptr 24B). offset 추정:
//! - +0x18: begin (8B, zero init)
//! - +0x20: end (8B)
//! - +0x28: cap_end (8B)
//!
//! ## 본 port 의 단순화 (semantic-eq Vec<Box<Segment>>)
//!
//! raw 의 vector<Segment*> 는 새 Segment* 를 push 하면 ownership 가 vector 로 이전.
//! 본 port 는 `Vec<Box<Segment>>` 로 동등 (Box = unique ownership, drop 시 자동 free).
//! GetCount/GetAt/Insert/Remove 가 동일 동작.

use crate::logical_position::LogicalPosition;
use crate::shape_segment::{Segment, SegmentStyle};

/// `Hnc::Shape::Render::Path::Style` u32 enum (raw `[x0, #0]` field).
///
/// raw 의 실제 variant 매핑은 path.rs 의 `Style` 와 동일 (placeholder u32).
pub type Style = u32;

/// `Hnc::Type::SizeImpl<float>` — 8B {width, height}.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SizeF {
    pub width: f32,
    pub height: f32,
}

impl SizeF {
    pub const fn new(w: f32, h: f32) -> Self {
        Self { width: w, height: h }
    }
}

/// `Hnc::Shape::Path` — 48B high-level path.
///
/// raw 의 `Path(Style, bool, SizeF, bool, float)` ctor 가 만드는 layout.
/// Render::Path 와는 별개 class (별개 namespace `Hnc::Shape::` vs `Hnc::Shape::Render::`).
#[repr(C)]
#[derive(Debug)]
pub struct Path {
    /// `+0x00` — style (u32).
    pub style: Style,
    /// `+0x04` — stroke_style (bool).
    pub stroke_style: u8,
    /// `+0x05` — extrusion_ok (bool).
    pub extrusion_ok: u8,
    /// `+0x06-07` — padding.
    _pad1: [u8; 2],
    /// `+0x08` — size (8B).
    pub size: SizeF,
    /// `+0x10` — light (f32).
    pub light: f32,
    /// `+0x14-17` — padding.
    _pad2: [u8; 4],
    /// `+0x18-0x30` — segments (raw `vector<Segment*>` 24B).
    /// 본 port 는 idiomatic Vec<Box<Segment>>. byte-eq output (GetCount/GetAt) 보장.
    pub segments: Vec<Box<Segment>>,
}

pub const SHAPE_PATH_SIZE_BYTES: usize = 48;
pub const SHAPE_PATH_ALIGN_BYTES: usize = 8;

impl Path {
    /// raw `Path::Path(Style, bool, SizeF const&, bool, float)` @ `0x1096cc` sz=36B.
    ///
    /// raw asm:
    /// ```text
    /// str  w1, [x0]               ; style
    /// strb w2, [x0, #4]           ; stroke_style
    /// strb w4, [x0, #5]           ; extrusion_ok
    /// ldr  x8, [x3]; str x8, [x0, #8]   ; size (8B copy)
    /// str  s0, [x0, #0x10]        ; light
    /// str  xzr, [x0, #0x18]       ; vector cap_end = 0
    /// stp  xzr, xzr, [x0, #0x20]  ; vector begin/end = 0
    /// ret
    /// ```
    pub fn new(style: Style, stroke_style: bool, size: SizeF, extrusion_ok: bool, light: f32) -> Self {
        Self {
            style,
            stroke_style: stroke_style as u8,
            extrusion_ok: extrusion_ok as u8,
            _pad1: [0; 2],
            size,
            light,
            _pad2: [0; 4],
            segments: Vec::new(),
        }
    }

    /// raw `Path::Create(Style, bool, SizeF const&, bool, float)` @ `0x10f590` sz=100B.
    ///
    /// raw 는 `Path*` 반환 (heap alloc). 본 port 는 `Box<Self>`.
    pub fn create(style: Style, stroke_style: bool, size: SizeF, extrusion_ok: bool, light: f32) -> Box<Self> {
        Box::new(Self::new(style, stroke_style, size, extrusion_ok, light))
    }

    /// raw `Path::GetCount()` @ `0xd08fc` sz=16B.
    ///
    /// raw: `ldp x8, x9, [x0, #0x20]; sub x0, x9, x8; lsr x0, x0, #3; ret`
    /// (= (end - begin) / 8).
    pub fn get_count(&self) -> usize {
        self.segments.len()
    }

    /// raw `Path::IsEmpty()` @ `0x1b1560` sz=16B.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// raw `Path::GetStyle() const` @ `0x14afa0` sz=8B. `ldr w0, [x0]`.
    pub fn get_style(&self) -> Style {
        self.style
    }

    /// raw `Path::GetLight() const` @ `0x14afa8` sz=8B. `ldr s0, [x0, #0x10]`.
    pub fn get_light(&self) -> f32 {
        self.light
    }

    /// raw `Path::GetStrokeStyle()` @ `0x1b1570` sz=8B.
    pub fn get_stroke_style(&self) -> bool {
        self.stroke_style != 0
    }

    /// raw `Path::SetStrokeStyle(bool)` @ `0x1b1578` sz=8B.
    pub fn set_stroke_style(&mut self, v: bool) {
        self.stroke_style = v as u8;
    }

    /// raw `Path::GetExtrusionOk()` @ `0x1b1580` sz=8B.
    pub fn get_extrusion_ok(&self) -> bool {
        self.extrusion_ok != 0
    }

    /// raw `Path::SetSize(SizeF const&)` @ `0xd0bd0` sz=12B. `ldr x8, [x1]; str x8, [x0, #8]`.
    pub fn set_size(&mut self, size: SizeF) {
        self.size = size;
    }

    /// raw `Path::Start(LogicalPosition*)` @ `0x10973c` sz=416B.
    ///
    /// raw 는 큰 함수 (vector push_back 인라인 + alloc + Segment::CreateStart 등) 인데
    /// semantic 은 단순: `segments.push(Segment::CreateStart(pos))`.
    ///
    /// 본 port 는 ownership 을 self.segments 로 이전.
    pub fn start(&mut self, pos: LogicalPosition) {
        // raw 는 LogicalPosition* (heap) 을 받음. 본 port 는 value 를 받아 내부에서 alloc.
        let lp_ptr = Box::into_raw(Box::new(pos));
        let seg = unsafe { Segment::create_start(lp_ptr) };
        self.segments.push(seg);
    }

    /// raw `Path::AddLine(LogicalPosition*)` @ `0x1098e0` sz=420B.
    ///
    /// semantic: `segments.push(Segment::CreateLine(pos))`.
    pub fn add_line(&mut self, pos: LogicalPosition) {
        let lp_ptr = Box::into_raw(Box::new(pos));
        let seg = unsafe { Segment::create_line(lp_ptr) };
        self.segments.push(seg);
    }

    /// raw `Path::AddBezier(LogicalPosition*, LogicalPosition*, LogicalPosition*)` @ `0x10ff44` sz=428B.
    pub fn add_bezier(&mut self, cp1: LogicalPosition, cp2: LogicalPosition, end: LogicalPosition) {
        let p1 = Box::into_raw(Box::new(cp1));
        let p2 = Box::into_raw(Box::new(cp2));
        let p3 = Box::into_raw(Box::new(end));
        let seg = unsafe { Segment::create_bezier(p1, p2, p3) };
        self.segments.push(seg);
    }

    /// raw `Path::Close()` @ `0x109a84` sz=28B.
    ///
    /// raw asm (28B = 7 instr):
    /// ```text
    /// (alloc 32B; style=4; pos0=null; pos1=null; pos2=null; push)
    /// ```
    /// 실제론 단순한 Close marker Segment 를 push.
    pub fn close(&mut self) {
        let seg = Box::new(Segment {
            style: SegmentStyle::Close.as_u32(),
            _pad: 0,
            pos0: std::ptr::null_mut(),
            pos1: std::ptr::null_mut(),
            pos2: std::ptr::null_mut(),
        });
        self.segments.push(seg);
    }

    /// raw `Path::GetAt(usize)` @ `0xd090c` sz=120B.
    ///
    /// raw 는 bounds-check 후 `(*Segment*)*((begin)[idx])` 반환. 본 port 는 `Option<&Segment>`.
    pub fn get_at(&self, index: usize) -> Option<&Segment> {
        self.segments.get(index).map(|b| b.as_ref())
    }
}

impl Drop for Path {
    /// raw `~Path()` @ `0x16ab3c` sz=112B.
    ///
    /// raw: vector 의 각 Segment* 에 대해 그 LogicalPosition* 들 free + Segment* free.
    /// 본 port 는 `Vec<Box<Segment>>` drop 이 Segment 를 자동 free. Segment 의
    /// LogicalPosition raw ptr 은 수동 cleanup 필요.
    fn drop(&mut self) {
        for seg in self.segments.iter() {
            unsafe {
                if !seg.pos0.is_null() {
                    let _ = Box::from_raw(seg.pos0);
                }
                if !seg.pos1.is_null() {
                    let _ = Box::from_raw(seg.pos1);
                }
                if !seg.pos2.is_null() {
                    let _ = Box::from_raw(seg.pos2);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_basic_fields() {
        let p = Path::new(0, true, SizeF::new(10.0, 20.0), false, 0.5);
        assert_eq!(p.style, 0);
        assert!(p.get_stroke_style());
        assert!(!p.get_extrusion_ok());
        assert_eq!(p.size, SizeF::new(10.0, 20.0));
        assert_eq!(p.get_light(), 0.5);
        assert!(p.is_empty());
        assert_eq!(p.get_count(), 0);
    }

    #[test]
    fn field_offsets() {
        let p = Path::new(0, false, SizeF::default(), false, 0.0);
        let base = &p as *const _ as usize;
        assert_eq!(&p.style as *const _ as usize - base, 0x00);
        assert_eq!(&p.stroke_style as *const _ as usize - base, 0x04);
        assert_eq!(&p.extrusion_ok as *const _ as usize - base, 0x05);
        assert_eq!(&p.size as *const _ as usize - base, 0x08);
        assert_eq!(&p.light as *const _ as usize - base, 0x10);
        // segments at +0x18 (raw vector starts here)
        assert_eq!(&p.segments as *const _ as usize - base, 0x18);
    }

    #[test]
    fn create_returns_boxed() {
        let p = Path::create(1, true, SizeF::new(5.0, 5.0), true, 1.0);
        assert_eq!(p.style, 1);
        assert!(p.get_stroke_style());
        assert!(p.get_extrusion_ok());
    }

    #[test]
    fn start_adds_one_segment() {
        let mut p = Path::new(0, false, SizeF::default(), false, 0.0);
        p.start(LogicalPosition::from_xy(1.0, 2.0));
        assert_eq!(p.get_count(), 1);
        let seg = p.get_at(0).unwrap();
        assert_eq!(seg.style, 0);
        unsafe {
            assert_eq!((*seg.pos0).get_x(), 1.0);
            assert_eq!((*seg.pos0).get_y(), 2.0);
        }
    }

    #[test]
    fn add_line_appends_line_segment() {
        let mut p = Path::new(0, false, SizeF::default(), false, 0.0);
        p.start(LogicalPosition::from_xy(0.0, 0.0));
        p.add_line(LogicalPosition::from_xy(10.0, 0.0));
        assert_eq!(p.get_count(), 2);
        assert_eq!(p.get_at(0).unwrap().style, 0);
        assert_eq!(p.get_at(1).unwrap().style, 1);
    }

    #[test]
    fn add_bezier_appends_bezier_segment() {
        let mut p = Path::new(0, false, SizeF::default(), false, 0.0);
        p.add_bezier(
            LogicalPosition::from_xy(1.0, 1.0),
            LogicalPosition::from_xy(2.0, 2.0),
            LogicalPosition::from_xy(3.0, 3.0),
        );
        let seg = p.get_at(0).unwrap();
        assert_eq!(seg.style, 3);
        unsafe {
            assert_eq!((*seg.pos0).get_x(), 1.0);
            assert_eq!((*seg.pos1).get_x(), 2.0);
            assert_eq!((*seg.pos2).get_x(), 3.0);
        }
    }

    #[test]
    fn close_appends_close_segment() {
        let mut p = Path::new(0, false, SizeF::default(), false, 0.0);
        p.start(LogicalPosition::from_xy(0.0, 0.0));
        p.close();
        assert_eq!(p.get_count(), 2);
        let close_seg = p.get_at(1).unwrap();
        assert_eq!(close_seg.style, 4);
        assert!(close_seg.pos0.is_null());
    }

    #[test]
    fn rectangle_construction() {
        // 사각형: Start(0,0) → AddLine(10,0) → AddLine(10,10) → AddLine(0,10) → Close()
        let mut p = Path::new(0, false, SizeF::new(10.0, 10.0), false, 0.0);
        p.start(LogicalPosition::from_xy(0.0, 0.0));
        p.add_line(LogicalPosition::from_xy(10.0, 0.0));
        p.add_line(LogicalPosition::from_xy(10.0, 10.0));
        p.add_line(LogicalPosition::from_xy(0.0, 10.0));
        p.close();
        assert_eq!(p.get_count(), 5);
        assert_eq!(p.get_at(0).unwrap().style, 0); // Start
        assert_eq!(p.get_at(1).unwrap().style, 1); // Line
        assert_eq!(p.get_at(2).unwrap().style, 1);
        assert_eq!(p.get_at(3).unwrap().style, 1);
        assert_eq!(p.get_at(4).unwrap().style, 4); // Close
    }

    #[test]
    fn set_size_modifies_size() {
        let mut p = Path::new(0, false, SizeF::new(1.0, 1.0), false, 0.0);
        p.set_size(SizeF::new(99.0, 88.0));
        assert_eq!(p.size, SizeF::new(99.0, 88.0));
    }

    #[test]
    fn drop_releases_logical_positions() {
        // valgrind 없이는 leak 검증 어렵지만, 구조 검증
        let mut p = Path::new(0, false, SizeF::default(), false, 0.0);
        p.start(LogicalPosition::from_xy(0.0, 0.0));
        p.add_line(LogicalPosition::from_xy(1.0, 1.0));
        p.add_bezier(
            LogicalPosition::from_xy(2.0, 2.0),
            LogicalPosition::from_xy(3.0, 3.0),
            LogicalPosition::from_xy(4.0, 4.0),
        );
        drop(p);
        // no UB 또는 double-free 확인 (cargo miri 가 잡아줌)
    }
}
