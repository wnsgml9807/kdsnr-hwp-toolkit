//! `Hnc::Shape::Text::RenderUtil` — drawing-time helpers.
//!
//! raw asm 위치: `work/hft_re/drawing_re/RenderUtil_*.asm` (otool dump from
//! `libHncDrawingEngine.dylib`).
//!
//! # Method list (이 module 에 1:1 port)
//!
//! | symbol | raw addr | size | 의미 |
//! |--------|----------|------|------|
//! | `ToMatrix3(Transform2D const&)` | 0x2d1ae4 | 200B | Transform2D → Matrix3 변환 |
//! | `LogicalToRender(Point&)` | 0x332274 | 288B | ShapeEngine DPI 기반 좌표 변환 (보류 — ShapeEngine 의존) |
//! | `DrawLine / DrawRect / DrawFillRect / DrawCross / DrawOrigin` | various | small | Surface wrapper (R-3 SKIP scope) |
//!
//! `Render::SurfaceRestorer` 는 CoreGraphics `_CGContextSaveGState/RestoreGState`
//! wrapper 라 SvgSurface backend 에서는 무관. byte-eq port 대상 아님.

use crate::matrix3::Matrix3;
use crate::shape_engine;
use crate::surface::PointImpl;
use crate::transform2d::Transform2D;

/// `Hnc::Shape::Text::RenderUtil::ToMatrix3(Transform2D const&)` (`0x2d1ae4` sz=200B).
///
/// raw 의 흐름:
/// 1. `GetElement(0)` → s8  (= m00)
/// 2. `GetElement(2)` → s9  (= m01)
/// 3. `GetElement(4)` → s10 (= m02)
/// 4. `GetElement(1)` → s11 (= m10)
/// 5. `GetElement(3)` → s12 (= m11)
/// 6. `GetElement(5)` → s5  (= m12)
/// 7. 9th stack arg = `1.0` (m22)
/// 8. `Matrix3(s8, s9, s10, s11, s12, s5, 0.0, 0.0, 1.0)` (9-float ctor)
///
/// 즉 Transform2D 의 2×3 affine 부분 (m00..m12) + 강제 bottom row `(0, 0, 1)`.
/// 일반적인 Transform2D 의 internal matrix 와 동등하지만, m20/m21/m22 가
/// 의도된 (0,0,1) 이 아닌 임의 값일 경우 그것을 무시하고 강제 표준화.
///
/// raw 의 `GetElement` table 순서가 `[m00, m10, m01, m11, m02, m12]` 이라
/// 호출 순서가 raw 의 register 매핑과 정확히 매칭되도록 같은 index 호출.
pub fn to_matrix3(t: &Transform2D) -> Matrix3 {
    let s8 = t.get_element(0);   // m00
    let s9 = t.get_element(2);   // m01
    let s10 = t.get_element(4);  // m02
    let s11 = t.get_element(1);  // m10
    let s12 = t.get_element(3);  // m11
    let s5 = t.get_element(5);   // m12
    Matrix3::from_floats(
        s8, s9, s10,
        s11, s12, s5,
        0.0, 0.0, 1.0,
    )
}

/// `Hnc::Shape::Text::RenderUtil::LogicalToRender(Point&)` (`0x332274` sz=288B).
///
/// raw 의 흐름 (분기 없음, 단순 변환):
/// 1. `bl ShapeEngine::GetInstance` (first call)
/// 2. `ldur d0, [x0, #0x4]` — 8 bytes from engine+4 (= `unit` + low 4B of next field)
/// 3. `str q0, [sp, #0x10]` — save
/// 4. `dup.2s v0, w8` where w8 = `0x42c00000` (= float 96.0) → `v0 = (96.0, 96.0)`
/// 5. `fmul.2s v8, v1, v0` — `v8 = (p.x * 96, p.y * 96)`
/// 6. `bl ShapeEngine::GetInstance` (second call, same singleton)
/// 7. `add x8, x0, #0x4; ld1.s {v0}[1], [x8]` — overwrite `v0.s[1] = unit` (engine+4)
/// 8. effective `v0 = (unit, unit)` (lane 0 from saved q0 = unit, lane 1 overwritten = unit)
/// 9. `fdiv.2s v0, v8, v0` — `v0 = (p.x*96/unit, p.y*96/unit)`
/// 10. `str d0, [x19]` — write back to point
///
/// 즉 `p.x = p.x * 96.0 / unit; p.y = p.y * 96.0 / unit`. 양 축 동일 scale.
///
/// 96.0 은 raw 의 hardcoded constant. unit 은 `ShapeEngine::GetLogicalDpi()`.
pub fn logical_to_render(p: &mut PointImpl<f32>) {
    let unit = shape_engine::read_instance().unit;
    // raw: fmul (p * 96.0) [single round] → fdiv (... / unit) [single round]
    //   2-lane SIMD 가 양 축 동일 sequence → scalar 도 동일.
    // Rust 컴파일러는 mul_add 자동 합성 안 함 (`*` 와 `/` 는 명시적 두 instruction).
    let px_scaled = p.x * 96.0;
    let py_scaled = p.y * 96.0;
    p.x = px_scaled / unit;
    p.y = py_scaled / unit;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_matrix3_extracts_affine_2x3() {
        let m = Matrix3::from_floats(
            2.0, 3.0, 5.0,
            7.0, 11.0, 13.0,
            99.0, 99.0, 99.0,  // pathological bottom row
        );
        let t = Transform2D::from_matrix(&m);
        let out = to_matrix3(&t);
        // raw 강제 bottom row (0,0,1):
        assert_eq!(out.m, [2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn to_matrix3_identity_is_identity() {
        let t = Transform2D::new();
        let out = to_matrix3(&t);
        assert_eq!(out.m, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn to_matrix3_normalizes_bottom_row() {
        // Transform2D with non-standard bottom row gets normalized.
        let mut t = Transform2D::new();
        t.matrix.m[6] = 0.5;  // pathological
        t.matrix.m[7] = 0.7;
        t.matrix.m[8] = 2.0;
        let out = to_matrix3(&t);
        // raw forces (0, 0, 1) regardless
        assert_eq!(out.m[6], 0.0);
        assert_eq!(out.m[7], 0.0);
        assert_eq!(out.m[8], 1.0);
    }

    #[test]
    fn to_matrix3_typical_affine() {
        // Standard 2D affine: scale + translate
        let m = Matrix3::from_floats(
            2.0, 0.0, 10.0,   // sx=2, tx=10
            0.0, 3.0, 20.0,   // sy=3, ty=20
            0.0, 0.0, 1.0,
        );
        let t = Transform2D::from_matrix(&m);
        let out = to_matrix3(&t);
        assert_eq!(out.m, m.m);  // byte-eq copy
    }

    // Note: LogicalToRender tests share global ShapeEngine singleton state.
    // We restore unit=1.0 (default) after each test to avoid cross-test pollution.

    #[test]
    fn logical_to_render_default_unit_scales_by_96() {
        // Save+restore singleton state to avoid test pollution.
        let saved_unit = shape_engine::read_instance().unit;
        shape_engine::write_instance().set_unit(1.0);

        let mut p = PointImpl { x: 1.0, y: 2.0 };
        logical_to_render(&mut p);
        // unit = 1.0, scale = 96.0/1.0 = 96.0
        assert_eq!(p.x, 96.0);
        assert_eq!(p.y, 192.0);

        shape_engine::write_instance().set_unit(saved_unit);
    }

    #[test]
    fn logical_to_render_with_custom_unit() {
        let saved_unit = shape_engine::read_instance().unit;
        shape_engine::write_instance().set_unit(2.0);

        let mut p = PointImpl { x: 1.0, y: 4.0 };
        logical_to_render(&mut p);
        // scale = 96.0 / 2.0 = 48.0 (exact)
        assert_eq!(p.x, 48.0);
        assert_eq!(p.y, 192.0);

        shape_engine::write_instance().set_unit(saved_unit);
    }

    #[test]
    fn logical_to_render_zero_point_unchanged() {
        let saved_unit = shape_engine::read_instance().unit;
        shape_engine::write_instance().set_unit(1.0);

        let mut p = PointImpl { x: 0.0, y: 0.0 };
        logical_to_render(&mut p);
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);

        shape_engine::write_instance().set_unit(saved_unit);
    }
}
