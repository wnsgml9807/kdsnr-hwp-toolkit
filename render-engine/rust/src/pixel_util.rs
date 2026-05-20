//! `Hnc::Util::PixelUtil` — pixel/color conversion helpers.
//!
//! 본 단계 scope: `HslToRgb` 만. 다른 PixelUtil 함수 (RgbToHsl 등) 는 필요 시.
//!
//! # 원본 위치
//! `libHncFoundation.dylib` (arm64). symbol:
//! `__ZN3Hnc4Util9PixelUtil8HslToRgbERhS2_S2_NS0_6DegreeEffff`
//! @ `0x13724` (~220B, NEON-heavy).

use crate::degree::Degree;
use crate::math_util::round_to_i64;

/// `Hnc::Util::PixelUtil::HslToRgb(u8& r, u8& g, u8& b, Degree h, float s, float l, float s_scale, float l_scale)`.
///
/// raw `0x13724` (~220B).
///
/// # raw asm trace 정공법
///
/// 본 port 는 raw asm 의 모든 fcmp / fcsel / fmadd 를 register-level 1:1 재현.
/// register lane 의 의미를 잃지 않기 위해 변수명에 raw register suffix 추가.
///
/// ## Lane setup (raw 13724-bc):
///
/// 3 개의 hue-shifted lane 을 만든 후 (h >= 240) 와 (h >= 120) 두 단계 fcsel
/// 로 piecewise linear 의 6 sector trapezoidal hue function 을 계산.
///
/// 최종 (h >= 120 select 후) lane 의미 - h ∈ [0..360) 에서:
///
/// | h sector | s3 lane     | s5 lane     | s4 lane     | (R, G, B 채널?)         |
/// |----------|------------|------------|------------|------------------------|
/// | 0..60    | (120-h)/60 | h/60       | 0          | h=0 → (2,0,0) →R       |
/// | 60..120  | (120-h)/60 | h/60       | 0          | h=60 → (1,1,0) →R+G    |
/// | 120..180 | 0          | (240-h)/60 | (h-120)/60 | h=120 → (0,2,0) → G    |
/// | 180..240 | 0          | (240-h)/60 | (h-120)/60 | h=180 → (0,1,1) → G+B  |
/// | 240..300 | (h-240)/60 | 0          | (360-h)/60 | h=240 → (0,0,2) → B    |
/// | 300..360 | (h-240)/60 | 0          | (360-h)/60 | h=300 → (1,0,1) → R+B  |
///
/// → s3 lane → R, s5 lane → G, s4 lane → B  (h=120 → G, h=240 → B 검증)
///
/// ## Saturation blend (raw 137ec-fc):
/// `out = 2s * clamped_lane + (1 - s)`. s=0 → 1 (white); s=1 → 2*lane (gray)
///
/// ## Lightness blend (raw 13810-28):
/// - `l >= 0.5`: `out = (1-l) * sat_out + 2l - 1`
/// - `l <  0.5`: `out = l * sat_out`
/// (raw 의 fcmp s1, #0.5 + fcsel pl)
///
/// ## Final clamp + scale (raw 13844-fc):
/// `out_byte = round(clamp(out, 0, 1) * 255)`
///
/// # raw 의 register → 출력 매핑 (strb 순서):
/// - `strb [x0=out_r] = round(s1 * 255)` where s1 = clamp(s4_blend) → s4 → R lane (= s3 piece)
/// - `strb [x1=out_g] = round(s0 * 255)` where s0 = clamp(s3_blend) → s3 → G lane (= s5 piece)
/// - `strb [x2=out_b] = round(s3 * 255)` where s3 = clamp(s2_blend) → s2 → B lane (= s4 piece)
///
/// (raw 의 변수 이름 swap 이 헷갈리지만 최종 매핑은 위와 같음.)
pub fn hsl_to_rgb(
    out_r: &mut u8,
    out_g: &mut u8,
    out_b: &mut u8,
    hue: &Degree,
    s: f32,
    l: f32,
    s_scale: f32,
    l_scale: f32,
) {
    // raw 13724-2c:
    //   s4 = ldr [x3] = h
    //   s2 = s0*s2 = s * s_scale
    //   s1 = s1*s3 = l * l_scale
    let h = hue.get_value();
    let s_eff = s * s_scale;
    let l_eff = l * l_scale;

    // raw 13730-3c constants:
    //   0x42f00000 = 120.0
    //   0x43700000 = 240.0
    //   0x42700000 = 60.0
    //   0x43b40000 = 360.0
    //   0xc3700000 = -240.0  (raw mov #-0x3c900000 sign-trick)
    //   0xc2f00000 = -120.0  (raw mov #-0x3d100000)
    let c60: f32 = f32::from_bits(0x42700000);
    let c120: f32 = f32::from_bits(0x42f00000);
    let c240: f32 = f32::from_bits(0x43700000);
    let c360: f32 = f32::from_bits(0x43b40000);
    let neg_c240: f32 = f32::from_bits(0xc3700000); // -240.0
    let neg_c120: f32 = f32::from_bits(0xc2f00000); // -120.0

    // raw 13740: fcmp s4, s3  (s3 = 240) → h vs 240
    let h_ge_240 = h >= c240;

    // raw 13744-58: lane_A = (h - 240) / 60
    let lane_a = (h + neg_c240) / c60;
    // raw 1375c-68: lane_B = (360 - h) / 60
    let lane_b = (c360 - h) / c60;
    // raw 1376c: s6 = 0
    // raw 13770-84: lane_C = (240 - h) / 60
    let lane_c = (c240 - h) / c60;
    // raw 13788-94: lane_D = (h - 120) / 60
    let lane_d = (h + neg_c120) / c60;

    // raw 13798-a0: fcsel pl (h >= 240)
    //   s5 = h>=240 ? lane_B : lane_D
    //   s7 = h>=240 ? 0      : lane_C
    //   s3 = h>=240 ? lane_A : 0
    let p240_s5 = if h_ge_240 { lane_b } else { lane_d };
    let p240_s7 = if h_ge_240 { 0.0 } else { lane_c };
    let p240_s3 = if h_ge_240 { lane_a } else { 0.0 };

    // raw 137a4-bc:
    //   lane_E = (120 - h) / 60
    //   lane_F = h / 60
    let lane_e = (c120 - h) / c60;
    let lane_f = h / c60;

    // raw 137c0-cc: fcmp s4, s0  (s0=120 at this point) → h vs 120
    //   s4 = h>=120 ? p240_s5 : 0
    //   s5 = h>=120 ? p240_s7 : lane_F
    //   s3 = h>=120 ? p240_s3 : lane_E
    let h_ge_120 = h >= c120;
    let lane_s4 = if h_ge_120 { p240_s5 } else { 0.0 };
    let lane_s5 = if h_ge_120 { p240_s7 } else { lane_f };
    let lane_s3 = if h_ge_120 { p240_s3 } else { lane_e };
    //
    // CHECK at h=0 (NOT h_ge_120, NOT h_ge_240):
    //   lane_s4 = 0
    //   lane_s5 = lane_F = h/60 = 0
    //   lane_s3 = lane_E = (120-h)/60 = 2
    // CHECK at h=120 (h_ge_120 true, NOT h_ge_240):
    //   lane_s4 = p240_s5 = lane_D = (120-120)/60 = 0
    //   lane_s5 = p240_s7 = lane_C = (240-120)/60 = 2
    //   lane_s3 = p240_s3 = 0
    // CHECK at h=240 (both true):
    //   lane_s4 = p240_s5 = lane_B = (360-240)/60 = 2
    //   lane_s5 = p240_s7 = 0
    //   lane_s3 = p240_s3 = lane_A = (240-240)/60 = 0
    //
    // → lane_s3 spikes at h=0 (R), lane_s5 spikes at h=120 (G), lane_s4 spikes at h=240 (B)

    // raw 137d0-e8: clamp each lane to <= 1.0 (no lower-bound clamp here)
    //   s0 = 1.0 (overwrites 120 from earlier)
    let one: f32 = 1.0;
    let cs3 = if lane_s3 > one { one } else { lane_s3 };
    let cs5 = if lane_s5 > one { one } else { lane_s5 };
    let cs4 = if lane_s4 > one { one } else { lane_s4 };

    // raw 137ec-fc: saturation blend
    //   s6 = 2*s_eff
    //   s2 (overwrite) = 1 - s_eff
    //   sat_s3 = 2s*cs3 + (1-s)
    //   sat_s5 = 2s*cs5 + (1-s)
    //   sat_s2 = 2s*cs4 + (1-s)  ← note: cs4 lane goes into "sat_s2" slot
    let two_s = s_eff + s_eff;
    let one_minus_s = one - s_eff;
    let sat_s3 = two_s * cs3 + one_minus_s; // R-lane saturation result
    let sat_s5 = two_s * cs5 + one_minus_s; // G-lane saturation result
    let sat_s2 = two_s * cs4 + one_minus_s; // B-lane saturation result (note variable swap)

    // raw 13800-28: lightness blend
    //   s4 = 0.5
    //   fcmp s1, s4  → l vs 0.5
    //
    //   pl branch (l >= 0.5):
    //     s4 (overwrite) = 1 - l
    //     s6 (overwrite) = 2l
    //     s7  = (1-l)*sat_s3 + 2l;  s7 -= 1
    //     s17 = (1-l)*sat_s5 + 2l;  s17 -= 1
    //     s4  = (1-l)*sat_s2 + 2l;  s6 (overwrite!) = s4 - 1
    //
    //   lt branch (l < 0.5):
    //     s4 (new) = l * sat_s3
    //     s3 (overwrite!) = l * sat_s5
    //     s1 (overwrite!) = l * sat_s2
    //
    //   Final fcsel pl (l >= 0.5):
    //     s3 = pl ? s17  : (l*sat_s5)   ← s3 register = G output pre-clamp
    //     s4 = pl ? s7   : (l*sat_s3)   ← s4 register = R output pre-clamp
    //     s2 = pl ? s6   : (l*sat_s2)   ← s2 register = B output pre-clamp
    let l_ge_half = l_eff >= 0.5;
    let one_minus_l = one - l_eff;
    let two_l = l_eff + l_eff;

    let pl_r = one_minus_l * sat_s3 + two_l + (-1.0_f32); // (1-l)*sat_s3 + 2l - 1
    let pl_g = one_minus_l * sat_s5 + two_l + (-1.0_f32);
    let pl_b = one_minus_l * sat_s2 + two_l + (-1.0_f32);

    let lt_r = l_eff * sat_s3;
    let lt_g = l_eff * sat_s5;
    let lt_b = l_eff * sat_s2;

    // raw: s4 (final, → out_r) = pl ? pl_r : lt_r
    //      s3 (final, → out_g) = pl ? pl_g : lt_g
    //      s2 (final, → out_b) = pl ? pl_b : lt_b
    let final_r_unclamped = if l_ge_half { pl_r } else { lt_r };
    let final_g_unclamped = if l_ge_half { pl_g } else { lt_g };
    let final_b_unclamped = if l_ge_half { pl_b } else { lt_b };

    // raw 13844-90: clamp each to [0..1]
    //   sequence: clamp s4 → s1, clamp s3 → s0, clamp s2 → s3
    //   if v > 1: → 1
    //   else if v >= 0: → v
    //   else: → 0
    //
    //   ⚠️ raw 의 fcmp + b.gt + fcmp + b.pl 의 NaN 처리: NaN > 1 → unordered → NOT taken;
    //   NaN >= 0 → unordered → NOT taken → 0 으로 떨어짐.
    fn clamp_unit(v: f32) -> f32 {
        if v > 1.0 {
            1.0
        } else if v >= 0.0 {
            v
        } else {
            0.0
        }
    }
    let cr = clamp_unit(final_r_unclamped);
    let cg = clamp_unit(final_g_unclamped);
    let cb = clamp_unit(final_b_unclamped);

    // raw 13894-fc: scale by 255, round, strb
    //   d2 = 255.0 (double)
    //   strb [x0] = round(cr * 255); strb [x1] = round(cg * 255); strb [x2] = round(cb * 255)
    let scale255: f64 = 255.0;
    *out_r = round_to_i64(cr as f64 * scale255) as u8;
    *out_g = round_to_i64(cg as f64 * scale255) as u8;
    *out_b = round_to_i64(cb as f64 * scale255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hsl(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
        let mut r = 0u8;
        let mut g = 0u8;
        let mut b = 0u8;
        let degree = Degree::from_float(h);
        hsl_to_rgb(&mut r, &mut g, &mut b, &degree, s, l, 1.0, 1.0);
        (r, g, b)
    }

    #[test]
    fn black_l_zero() {
        let (r, g, b) = hsl(0.0, 1.0, 0.0);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn white_l_one() {
        let (r, g, b) = hsl(0.0, 1.0, 1.0);
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn gray_s_zero_l_half() {
        // s=0 → all lanes blend to (1-s)=1 then clamp; lightness 0.5 → 0.5
        // round(0.5*255) = 128
        let (r, g, b) = hsl(0.0, 0.0, 0.5);
        assert_eq!((r, g, b), (128, 128, 128));
    }

    #[test]
    fn primary_red_h0() {
        let (r, g, b) = hsl(0.0, 1.0, 0.5);
        assert_eq!((r, g, b), (255, 0, 0), "h=0 → primary red");
    }

    #[test]
    fn primary_green_h120() {
        let (r, g, b) = hsl(120.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 255, 0), "h=120 → primary green");
    }

    #[test]
    fn primary_blue_h240() {
        let (r, g, b) = hsl(240.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 0, 255), "h=240 → primary blue");
    }

    #[test]
    fn yellow_h60() {
        let (r, g, b) = hsl(60.0, 1.0, 0.5);
        assert_eq!((r, g, b), (255, 255, 0));
    }

    #[test]
    fn cyan_h180() {
        let (r, g, b) = hsl(180.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 255, 255));
    }

    #[test]
    fn magenta_h300() {
        let (r, g, b) = hsl(300.0, 1.0, 0.5);
        assert_eq!((r, g, b), (255, 0, 255));
    }

    #[test]
    fn dark_red_l_quarter() {
        // h=0, s=1, l=0.25 → r=l*2*1 = 0.5 → 128, g=0, b=0
        let (r, g, b) = hsl(0.0, 1.0, 0.25);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
        // r = l_eff * sat_s3 where sat_s3 = 2*1*1 + 0 = 2; l*sat = 0.25*2 = 0.5
        // round(0.5*255) = 128
        assert_eq!(r, 128);
    }
}
