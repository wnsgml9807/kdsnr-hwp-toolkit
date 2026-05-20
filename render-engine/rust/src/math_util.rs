//! `Hnc::Util::MathUtil` — foundation 의 작은 numeric helper 집합.
//!
//! 본 module 은 ShapeRenderConverter (drawing engine) 가 `MathUtil::Round` 을
//! 호출하기에 필요해서 R-1 단계에서 우선 port.
//!
//! 본 단계 scope: `Round(double) -> i64` 만. 나머지 MathUtil 함수는 필요 시점에.
//!
//! # 원본 위치
//! `libHncFoundation.dylib` (arm64 slice). symbol:
//! `__ZN3Hnc4Util8MathUtil5RoundEd` @ `0x12a38`.

/// `Hnc::Util::MathUtil::Round(double) -> i64` — raw `0x12a38` (28B).
///
/// ```asm
/// 12a38: fcmp d0, #0.0
/// 12a3c: fmov d1, #0.5
/// 12a40: fmov d2, #-0.5
/// 12a44: fcsel d1, d2, d1, lt   ; d1 = (d0 < 0) ? -0.5 : 0.5
/// 12a48: fadd d0, d1, d0        ; d0 = d0 + d1
/// 12a4c: fcvtzs x0, d0          ; x0 = (i64)trunc(d0)
/// 12a50: ret
/// ```
///
/// 의미: "half away from zero" rounding (banker's rounding 아님).
/// - `Round(0.5)  = 1` (0.5 + 0.5 = 1.0)
/// - `Round(-0.5) = -1` (-0.5 + -0.5 = -1.0 → trunc to -1)
/// - `Round(1.4)  = 1` (1.4 + 0.5 = 1.9 → trunc 1)
/// - `Round(-1.4) = -1` (-1.4 + -0.5 = -1.9 → trunc -1)
///
/// raw 의 `fcmp ... b.lt` 는 NaN 의 경우 unordered → !lt → `+0.5` 분기 → fadd
/// 가 여전히 NaN → fcvtzs(NaN) = 0 (ARM "invalid op" default). 본 Rust 포팅은
/// `f64::is_sign_negative()` 가 아니라 raw 의 `<` 비교를 그대로 사용.
pub fn round_to_i64(d: f64) -> i64 {
    // raw fcsel pl semantic: d1 = (d < 0.0) ? -0.5 : 0.5.
    // NaN 의 경우 fcmp 결과 unordered, b.lt 안 taken → d1 = +0.5.
    let bias = if d < 0.0 { -0.5_f64 } else { 0.5_f64 };
    let biased = bias + d;
    // raw fcvtzs: float → signed integer with truncation toward zero.
    // - NaN → 0
    // - +Inf → INT64_MAX, -Inf → INT64_MIN
    // - Overflow → INT64_MAX or INT64_MIN saturated
    if biased.is_nan() {
        return 0;
    }
    if biased >= i64::MAX as f64 {
        return i64::MAX;
    }
    if biased <= i64::MIN as f64 {
        return i64::MIN;
    }
    biased.trunc() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_away_from_zero() {
        assert_eq!(round_to_i64(0.5), 1, "0.5 + 0.5 = 1.0 → trunc 1");
        assert_eq!(round_to_i64(-0.5), -1, "-0.5 + -0.5 = -1.0 → trunc -1");
        assert_eq!(round_to_i64(1.5), 2);
        assert_eq!(round_to_i64(-1.5), -2);
    }

    #[test]
    fn round_quarter_truncates() {
        assert_eq!(round_to_i64(1.4), 1, "1.4 + 0.5 = 1.9 → trunc 1");
        assert_eq!(round_to_i64(1.6), 2, "1.6 + 0.5 = 2.1 → trunc 2");
        assert_eq!(round_to_i64(-1.4), -1, "-1.4 + -0.5 = -1.9 → trunc -1");
        assert_eq!(round_to_i64(-1.6), -2);
    }

    #[test]
    fn round_zero_positive_bias() {
        // raw fcmp 0.0,#0.0 → eq → b.lt NOT taken → d1 = +0.5
        assert_eq!(round_to_i64(0.0), 0, "0 + 0.5 = 0.5 → trunc 0");
        assert_eq!(round_to_i64(-0.0), 0, "-0 (== +0) → bias 0.5 → 0");
    }

    #[test]
    fn round_integers_unchanged_in_range() {
        assert_eq!(round_to_i64(100.0), 100);
        assert_eq!(round_to_i64(-100.0), -100);
        assert_eq!(round_to_i64(255.0), 255);
    }

    #[test]
    fn round_nan_returns_zero() {
        // raw: fcmp NaN < 0 → unordered → b.lt NOT taken → bias=+0.5 → NaN+0.5=NaN
        // fcvtzs(NaN) = 0
        assert_eq!(round_to_i64(f64::NAN), 0);
    }
}
