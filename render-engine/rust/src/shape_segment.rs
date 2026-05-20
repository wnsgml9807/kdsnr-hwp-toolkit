//! `Hnc::Shape::Segment` — Shape::Path 의 element 단위 (Start / Line / Bezier / etc.).
//!
//! ## raw 출처 (libHncDrawingEngine.dylib arm64)
//!
//! - `Segment::CreateStart(LogicalPosition*)` @ `0x1b1588` sz=48B
//! - `Segment::CreateLine(LogicalPosition*)`  @ `0x114818` sz=52B
//! - `Segment::CreateBezier(LogicalPosition*, LogicalPosition*, LogicalPosition*)` @ `0x1168d4` sz=68B
//! - `Segment::Segment(Style, LogicalPosition*, LogicalPosition*, LogicalPosition*)` @ `0x1d1bfc` sz=20B
//! - `Segment::SetLastPosition(LogicalPosition)` @ `0x1b1aac` sz=148B
//!
//! ## raw layout (32B, raw `mov w0, #0x20` per CreateLine/CreateStart)
//!
//! | offset | size | field |
//! |--------|------|-------|
//! | +0x00  | 4B   | `style: u32` (= Style enum) |
//! | +0x04  | 4B   | (padding/alignment) |
//! | +0x08  | 8B   | `pos0: *LogicalPosition` (primary anchor) |
//! | +0x10  | 8B   | `pos1: *LogicalPosition` (cp1, for Bezier; null for Start/Line) |
//! | +0x18  | 8B   | `pos2: *LogicalPosition` (cp2/end, for Bezier; null otherwise) |
//!
//! raw CreateLine 의 sequence (0x114830-0x11483c):
//! ```text
//! mov w0, #0x20; bl __Znwm     ; alloc 32B
//! stp xzr, xzr, [x0, #0x10]    ; zero +0x10..0x20 (pos1, pos2 = null)
//! str x19, [x0, #8]             ; pos0 = arg
//! mov w8, #1; str w8, [x0]      ; style = 1 (Line)
//! ```
//!
//! raw CreateStart (0x1b15a0-0x1b15ac):
//! ```text
//! str wzr, [x0]                 ; style = 0 (Start)
//! stp xzr, xzr, [x0, #0x10]
//! str x19, [x0, #8]
//! ```

use crate::logical_position::LogicalPosition;
use std::ptr;

/// `Hnc::Shape::Segment::Style` — element kind (u32 enum, raw `[x0, #0]`).
///
/// raw 의 dispatch (Path::Start sets 0, Path::AddLine sets 1, Path::AddBezier sets 3):
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentStyle {
    /// raw style=0. subpath 시작 (한 점만).
    Start = 0,
    /// raw style=1. 직선 연결 (한 점만, 직전 점에서 line 그음).
    Line = 1,
    /// raw style=2. quadratic Bezier (2점: cp1, end).
    QuadraticBezier = 2,
    /// raw style=3. cubic Bezier (3점: cp1, cp2, end).
    Bezier = 3,
    /// raw style=4. close subpath (점 없음, 또는 보조 사용).
    Close = 4,
}

impl SegmentStyle {
    /// raw style field 값 (u32). RenderPathToPath 가 `[x0, #0]` 로 직접 사용.
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// `Hnc::Shape::Segment` — 32B element of Shape::Path.
#[repr(C)]
#[derive(Debug)]
pub struct Segment {
    /// `+0x00` — style (u32).
    pub style: u32,
    /// `+0x04` — padding.
    pub(crate) _pad: u32,
    /// `+0x08` — primary position (pos0). `null` 만 들어가는 일 거의 없음.
    pub pos0: *mut LogicalPosition,
    /// `+0x10` — pos1 (cp1 for Bezier, otherwise null).
    pub pos1: *mut LogicalPosition,
    /// `+0x18` — pos2 (cp2/end for Bezier, otherwise null).
    pub pos2: *mut LogicalPosition,
}

pub const SEGMENT_SIZE_BYTES: usize = 32;
pub const SEGMENT_ALIGN_BYTES: usize = 8;

const _: () = assert!(
    std::mem::size_of::<Segment>() == SEGMENT_SIZE_BYTES,
    "Segment size mismatch"
);

impl Segment {
    /// raw `Segment::CreateStart(LogicalPosition*)` @ `0x1b1588` sz=48B.
    ///
    /// ```asm
    /// mov w0, #0x20; bl __Znwm
    /// str wzr, [x0]                 ; style = 0
    /// stp xzr, xzr, [x0, #0x10]    ; pos1, pos2 = null
    /// str x19, [x0, #8]             ; pos0 = arg
    /// ret
    /// ```
    ///
    /// # Safety
    /// `pos` 는 valid `*mut LogicalPosition` 이거나 null.
    pub unsafe fn create_start(pos: *mut LogicalPosition) -> Box<Self> {
        Box::new(Self {
            style: SegmentStyle::Start.as_u32(),
            _pad: 0,
            pos0: pos,
            pos1: ptr::null_mut(),
            pos2: ptr::null_mut(),
        })
    }

    /// raw `Segment::CreateLine(LogicalPosition*)` @ `0x114818` sz=52B.
    ///
    /// CreateStart 와 동일 layout, 단 `style=1`.
    ///
    /// # Safety
    /// `pos` 는 valid 또는 null.
    pub unsafe fn create_line(pos: *mut LogicalPosition) -> Box<Self> {
        Box::new(Self {
            style: SegmentStyle::Line.as_u32(),
            _pad: 0,
            pos0: pos,
            pos1: ptr::null_mut(),
            pos2: ptr::null_mut(),
        })
    }

    /// raw `Segment::CreateBezier(LogicalPosition*, LogicalPosition*, LogicalPosition*)` @ `0x1168d4` sz=68B.
    ///
    /// raw alloc 32B, style=3, pos0=cp1, pos1=cp2, pos2=end.
    ///
    /// # Safety
    /// 모든 ptr 는 valid 또는 null.
    pub unsafe fn create_bezier(
        cp1: *mut LogicalPosition,
        cp2: *mut LogicalPosition,
        end: *mut LogicalPosition,
    ) -> Box<Self> {
        Box::new(Self {
            style: SegmentStyle::Bezier.as_u32(),
            _pad: 0,
            pos0: cp1,
            pos1: cp2,
            pos2: end,
        })
    }

    /// 현재 style 를 enum 으로 반환.
    pub fn style_enum(&self) -> Option<SegmentStyle> {
        match self.style {
            0 => Some(SegmentStyle::Start),
            1 => Some(SegmentStyle::Line),
            2 => Some(SegmentStyle::QuadraticBezier),
            3 => Some(SegmentStyle::Bezier),
            4 => Some(SegmentStyle::Close),
            _ => None,
        }
    }

    /// raw `strh w9, [x8, #0x2]` 의 16-bit halfword write 모방.
    ///
    /// raw RenderPathToPath 의 smooth-corner high-bit branch (0x10f94c-0x10f980,
    /// 0x10fa9c-0x10faa8) 가 마지막 segment 의 byte+2..+3 에 0x0001 을 strh 한다.
    /// 본 port 는 style: u32 의 상위 2 바이트로 매핑 (little-endian).
    ///
    /// raw 의미: "이 segment 의 끝점은 스무드 코너" (다음 segment 와 연속성 보장).
    pub fn set_smooth_corner(&mut self) {
        // strh w9, [x8, #0x2] with w9 = 1: byte +2 = 0x01, byte +3 = 0x00
        // → u32 little-endian 상에서 bits 16-23 = 0x01, bits 24-31 = 0x00
        let low16 = self.style & 0xFFFF;
        self.style = low16 | 0x0001_0000;
    }

    /// `Segment::SetLastPosition(LogicalPosition const&)` @ raw `0x1b1aac` sz=148B.
    ///
    /// raw 흐름:
    /// ```text
    /// ldrb w8, [x0]              ; load this->style
    /// cmp w8, #0x3               ; style == Bezier?
    /// b.eq ret                   ; yes → no-op, return (Bezier 의 pos0 은 cp1 이므로 보호)
    /// ; else: swap pos0->x_var/y_var pointer slots with new LogicalPosition's slots,
    /// ; then delete the swapped-out old Variable* (with CHncStringW dtor)
    /// ```
    ///
    /// semantic: "pos0 위치를 lp 의 위치로 교체" (Bezier 제외, ownership transfer).
    ///
    /// 본 port (literal LogicalPosition): pos0 의 x,y 값을 lp 의 x,y 값으로 덮어쓰기.
    /// Bezier 인 경우 no-op (raw 와 byte-eq).
    ///
    /// # Safety
    /// pos0 가 valid `*mut LogicalPosition` 이어야 함 (null 이 아닐 것).
    pub unsafe fn set_last_position(&mut self, lp: &LogicalPosition) {
        if self.style == SegmentStyle::Bezier.as_u32() {
            return;
        }
        if self.pos0.is_null() {
            return;
        }
        (*self.pos0).x = lp.x;
        // y 는 bit-pattern 보존이므로 y_low 그대로 복사
        let y_bits = lp.get_y().to_bits();
        (*self.pos0).y_low = y_bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical_position::LogicalPosition;

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<Segment>(), 32);
        assert_eq!(std::mem::align_of::<Segment>(), 8);
    }

    #[test]
    fn create_start_has_style_0() {
        let mut lp = LogicalPosition::from_xy(1.0, 2.0);
        let seg = unsafe { Segment::create_start(&mut lp as *mut _) };
        assert_eq!(seg.style, 0);
        assert!(!seg.pos0.is_null());
        assert!(seg.pos1.is_null());
        assert!(seg.pos2.is_null());
        assert_eq!(seg.style_enum(), Some(SegmentStyle::Start));
    }

    #[test]
    fn create_line_has_style_1() {
        let mut lp = LogicalPosition::from_xy(3.0, 4.0);
        let seg = unsafe { Segment::create_line(&mut lp as *mut _) };
        assert_eq!(seg.style, 1);
        assert!(!seg.pos0.is_null());
        assert!(seg.pos1.is_null());
        assert!(seg.pos2.is_null());
        assert_eq!(seg.style_enum(), Some(SegmentStyle::Line));
    }

    #[test]
    fn create_bezier_has_style_3_and_3_positions() {
        let mut cp1 = LogicalPosition::from_xy(1.0, 1.0);
        let mut cp2 = LogicalPosition::from_xy(2.0, 2.0);
        let mut end = LogicalPosition::from_xy(3.0, 3.0);
        let seg = unsafe {
            Segment::create_bezier(
                &mut cp1 as *mut _,
                &mut cp2 as *mut _,
                &mut end as *mut _,
            )
        };
        assert_eq!(seg.style, 3);
        assert!(!seg.pos0.is_null());
        assert!(!seg.pos1.is_null());
        assert!(!seg.pos2.is_null());
        assert_eq!(seg.style_enum(), Some(SegmentStyle::Bezier));
    }

    #[test]
    fn create_start_reads_pos_value() {
        let mut lp = LogicalPosition::from_xy(5.0, 6.0);
        let seg = unsafe { Segment::create_start(&mut lp as *mut _) };
        unsafe {
            assert_eq!((*seg.pos0).get_x(), 5.0);
            assert_eq!((*seg.pos0).get_y(), 6.0);
        }
    }

    #[test]
    fn style_enum_returns_none_for_invalid() {
        // 가짜 segment 만들어서 invalid style 테스트
        let mut lp = LogicalPosition::from_xy(0.0, 0.0);
        let mut seg = unsafe { Segment::create_line(&mut lp as *mut _) };
        seg.style = 99;
        assert_eq!(seg.style_enum(), None);
    }

    #[test]
    fn field_offsets() {
        let mut lp = LogicalPosition::from_xy(0.0, 0.0);
        let seg = unsafe { Segment::create_start(&mut lp as *mut _) };
        let base = &*seg as *const _ as usize;
        assert_eq!(&seg.style as *const _ as usize - base, 0x00);
        assert_eq!(&seg.pos0 as *const _ as usize - base, 0x08);
        assert_eq!(&seg.pos1 as *const _ as usize - base, 0x10);
        assert_eq!(&seg.pos2 as *const _ as usize - base, 0x18);
    }
}
