//! `Hnc::Shape::LogicalPosition` — Shape::Path 의 좌표 단위 (logical inch / 96-DPI invariant).
//!
//! ## raw 출처 (libHncDrawingEngine.dylib arm64)
//!
//! - `LogicalPosition(PointF)` @ `0x198e8c` sz=244B — 가장 자주 쓰이는 ctor
//! - `LogicalPosition(float, float)` @ `0x198f90` sz=240B — pair-float ctor
//! - `LogicalPosition(Variable*, Variable*)` @ `0x198f80` sz=8B — formula-based ctor
//! - `LogicalPosition(CHncStringW, CHncStringW)` @ `0x199308` sz=340B — 이름 기반 (수식)
//! - Copy ctor `0x19945c` sz=300B, dtor `0x199588` sz=96B
//! - `operator==` `0x1995e8` sz=116B, `operator!=` `0x199688` sz=124B
//! - `Swap` `0x199704` sz=36B
//! - `IsVariable` `0x199788` sz=40B
//! - `MappingGuides(Guides)` `0x1997b0` sz=344B
//! - `GetX/GetY` (Shape namespace) `0x1402f4 / 0x140dfc` sz=8B (getter)
//!
//! ## raw layout (16B)
//!
//! | offset | size | field |
//! |--------|------|-------|
//! | +0x00  | 8B   | `x_var: *Variable` |
//! | +0x08  | 8B   | `y_var: *Variable` |
//!
//! Variable (16B) layout:
//! | +0x00  | 4B   | `value: f32` |
//! | +0x04  | 4B   | (uninitialized — raw 가 `str s9, [x19]` 만 함) |
//! | +0x08  | 8B   | `zero` (= 0, raw `str xzr, [x19, #8]`) |
//!
//! ## 본 port 의 단순화 (semantic-eq, byte-eq output)
//!
//! raw 의 Variable indirection 은 "formula 값 (수식)" 을 위한 것. PathUtil::RenderPathToPath
//! 가 만드는 LogicalPosition 은 항상 **literal float 좌표** — Variable 의 formula 기능 불필요.
//!
//! 본 port 는 `LogicalPosition { x: f32, y: f32 }` 로 16B 그대로 보존 (= raw 와 같은 size/align,
//! field 의미만 다름). GetX/GetY 출력은 byte-eq.
//!
//! raw 의 ShapeEngine::GetInstance().unit 스케일링 (0x198fcc-0x198fe8):
//! ```text
//! fcmp s10, s0     ; s10 = GetInstance().unit, s0 = GetInstance().unit (같은 instance)
//! b.eq             ; 같으므로 항상 skip (no rescaling)
//! ; 따라서 default 동작 = literal value 저장
//! ```
//! Singleton 이므로 두 GetInstance 가 항상 같은 값 → scaling no-op → byte-eq.

use crate::path::PointF;

/// `Hnc::Shape::LogicalPosition` — 16B literal logical position.
///
/// raw 는 `(*Variable, *Variable)` 16B 구조. 본 port 는 두 f32 + 8B padding 으로
/// size/align 동일, 의미 단순화.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalPosition {
    /// `+0x00` — x value (literal logical coord).
    pub x: f32,
    /// `+0x04` — (raw 의 Variable 가 4B padding 후 8B 가짐. 본 port 는 padding 으로).
    _pad1: u32,
    /// `+0x08` — (raw 의 y_var 자리). 본 port 는 y value 로 활용.
    pub y_low: u32,
    /// `+0x0c` — (raw 의 y_var 의 high half). 본 port 는 unused.
    _pad2: u32,
}

pub const LOGICAL_POSITION_SIZE_BYTES: usize = 16;
pub const LOGICAL_POSITION_ALIGN_BYTES: usize = 4;

const _: () = assert!(
    std::mem::size_of::<LogicalPosition>() == LOGICAL_POSITION_SIZE_BYTES,
    "LogicalPosition size mismatch"
);

impl LogicalPosition {
    /// `LogicalPosition(float, float)` @ raw `0x198f90` sz=240B.
    ///
    /// raw 는 Variable* 2 개 alloc 후 각 +0 에 ShapeEngine 스케일 적용한 f32 저장.
    /// 본 port 는 literal float 그대로 저장 (singleton scaling no-op 으로 동치).
    pub fn from_xy(x: f32, y: f32) -> Self {
        Self {
            x,
            _pad1: 0,
            y_low: y.to_bits(),
            _pad2: 0,
        }
    }

    /// `LogicalPosition(PointF)` @ raw `0x198e8c` sz=244B.
    ///
    /// raw 는 PointF 의 x,y 를 추출 → `from_xy` 와 동등.
    pub fn from_point(p: PointF) -> Self {
        Self::from_xy(p.x, p.y)
    }

    /// `LogicalPosition::GetX()` (`Hnc::Shape::` namespace) — return x value (f32).
    ///
    /// raw: literal 의 경우 Variable.value 직접 반환. 본 port 는 same.
    pub fn get_x(&self) -> f32 {
        self.x
    }

    /// `LogicalPosition::GetY()` — return y value (f32).
    pub fn get_y(&self) -> f32 {
        f32::from_bits(self.y_low)
    }

    /// `IsVariable()` @ raw `0x199788` sz=40B.
    ///
    /// raw 는 Variable 의 type tag 확인 후 true/false 반환. 본 port 는 literal 만 다루므로
    /// 항상 false. **TODO**: formula-based ctor 추가 시 보강.
    pub fn is_variable(&self) -> bool {
        false
    }

    /// `GetPosition(Guides const*) const` @ raw `0xbe010` sz=~500B (literal path만 ~76B).
    ///
    /// raw 흐름 (guides == NULL 분기, 0xbe0f8-0xbe16c):
    /// ```text
    /// ldr x8, [x20]           ; x8 = this->x_var (Variable*)
    /// ldr x9, [x8, #8]        ; x9 = x_var->name (CHncStringW*, 0 if literal)
    /// cbnz x9, throw          ; if Variable has name (= formula), throw "유효하지 않은 guides"
    /// ldr s8, [x8]            ; s8 = x_var->value (f32 literal)
    /// ; scaling (singleton no-op): fcmp s9, s0 → b.eq skip; else fdiv/fmul (skip)
    /// ; same for y_var
    /// stp s8, s9, [x19]       ; write PointF to *out (SRET via x8)
    /// ```
    ///
    /// 본 port (literal-only): `LogicalPosition.x, y` 를 직접 PointF 로 반환.
    /// raw 의 ShapeEngine scaling 은 singleton 이라 항상 no-op (= literal pass-through).
    ///
    /// `guides` 인자는 본 port 에서 무시 (Variable formula 미지원).
    pub fn get_position(&self, _guides: Option<&()>) -> PointF {
        PointF::new(self.x, self.get_y())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_size_align() {
        assert_eq!(std::mem::size_of::<LogicalPosition>(), 16);
        assert_eq!(std::mem::align_of::<LogicalPosition>(), 4);
    }

    #[test]
    fn from_xy_preserves_values() {
        let lp = LogicalPosition::from_xy(1.5, 2.5);
        assert_eq!(lp.get_x(), 1.5);
        assert_eq!(lp.get_y(), 2.5);
    }

    #[test]
    fn from_point_preserves_values() {
        let p = PointF::new(3.14, 6.28);
        let lp = LogicalPosition::from_point(p);
        assert_eq!(lp.get_x(), 3.14);
        assert_eq!(lp.get_y(), 6.28);
    }

    #[test]
    fn is_variable_false_for_literal() {
        let lp = LogicalPosition::from_xy(0.0, 0.0);
        assert!(!lp.is_variable());
    }

    #[test]
    fn negative_and_zero_values() {
        let lp = LogicalPosition::from_xy(-1.0, 0.0);
        assert_eq!(lp.get_x(), -1.0);
        assert_eq!(lp.get_y(), 0.0);
    }

    #[test]
    fn bit_pattern_preserved() {
        // byte-eq: f32 bit pattern preserved through storage
        let lp = LogicalPosition::from_xy(3.14159, -2.71828);
        assert_eq!(lp.get_x().to_bits(), 3.14159_f32.to_bits());
        assert_eq!(lp.get_y().to_bits(), (-2.71828_f32).to_bits());
    }

    #[test]
    fn get_position_returns_xy_as_pointf() {
        let lp = LogicalPosition::from_xy(1.5, -2.5);
        let p = lp.get_position(None);
        assert_eq!(p.x, 1.5);
        assert_eq!(p.y, -2.5);
    }

    #[test]
    fn get_position_ignores_guides_arg() {
        // 본 port 는 Variable 미지원 → guides 인자는 항상 무시
        let lp = LogicalPosition::from_xy(7.0, 8.0);
        let p_none = lp.get_position(None);
        let dummy = ();
        let p_some = lp.get_position(Some(&dummy));
        assert_eq!(p_none, p_some);
    }
}
