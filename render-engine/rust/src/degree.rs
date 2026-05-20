//! `Hnc::Util::Degree` (`libHncFoundation.dylib`, 4B = single f32) — 0~360 degree wrapper.
//!
//! raw asm 위치: `work/hft_re/foundation_re/Degree_all.asm` (otool dump from
//! `libHncFoundation.dylib` @ `0x12364..0x1285c`, 19 T symbols representing 11
//! unique methods with C1/C2/D1/D2 in-charge/not-in-charge ctor/dtor pairs).
//!
//! # Object layout
//! - `+0x0`: `f32 value` (single field, total 4 bytes)
//!
//! # Method list
//! | symbol | size | 의미 |
//! |--------|------|------|
//! | `Degree()` (default) | 8B | `str wzr, [x0]` — value=0 |
//! | `Degree(float)` | 96B | Constrain + store |
//! | `~Degree()` | 4B | no-op POD |
//! | `Constrain(float)` static | 88B | 0~360 modulo with sign fix |
//! | `operator=(float)` | 96B | Constrain + store (= Degree(float)) |
//! | `operator+=(Degree const&)` | 108B | this.value += other.value; Constrain |
//! | `operator+=(float)` | 104B | this.value += v; Constrain |
//! | `operator-=(Degree const&)` | 108B | this.value -= other.value; Constrain |
//! | `operator-=(float)` | 104B | this.value -= v; Constrain |
//! | `operator-()` const | 108B | sret = Degree(-this.value) (Constrain) |
//! | `Swap(Degree&)` | 16B | swap two f32 values |
//! | `GetValue() const` | 8B | return this.value |
//! | `ToRadian() const` | 36B | value * π/180 via f64 intermediate |
//! | `Normalize(Degree const& step)` | 148B | quantize to multiple of step, with rounding |
//! | `FlipWidth()` | 56B | special mirror around 180 |
//! | `ToDegree(float radians)` static | 68B | radians * 180/π + Constrain |
//!
//! # Constrain magic
//! raw `Constrain(deg)`:
//! 1. if `0.0 <= deg < 360.0`: return deg (skip mod calc).
//! 2. else: signed magic-multiply div by 360, then `deg -= q * 360`, finally
//!    `if deg < 0: deg += 360`.
//!
//! Magic constant `0xB60B60B7` (32-bit) is the signed reciprocal of 360 for
//! `smull + asr` divide-by-360 trick. The Rust port reproduces the **exact**
//! magic multiply sequence so the bit-level result matches even at f32 edge cases.
//!
//! Semantic equivalent: `deg.rem_euclid(360.0)`. But the raw uses int truncation
//! + magic multiply, not floating modulo — so byte-eq port must follow the magic
//! sequence.

use std::cmp::Ordering;

/// `Hnc::Util::Degree` — 4B single-float wrapper for angles in degrees.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct Degree {
    /// `+0x0`: degree value, normalized to [0, 360) by `Constrain`.
    pub value: f32,
}

impl Degree {
    /// `Degree()` default ctor (`0x12364` sz=8). raw: `str wzr, [x0]; ret` — value=0.
    pub const fn new() -> Self {
        Self { value: 0.0 }
    }

    /// `Degree(float deg)` ctor (`0x1236c` sz=96). raw: `Constrain(deg)` then store.
    pub fn from_float(deg: f32) -> Self {
        Self { value: Self::constrain(deg) }
    }

    /// `Hnc::Util::Degree::Constrain(float)` static (`0x12434` sz=88).
    ///
    /// raw asm 흐름:
    /// ```text
    /// fcmp s0, #0.0                ; deg vs 0
    /// mov w8, #0x43b40000           ; w8 = float(360.0) = 0x43b40000
    /// fmov s1, w8                   ; s1 = 360.0
    /// fccmp s0, s1, #0x0, pl        ; if pl (deg>=0): compare deg vs 360
    /// b.le 0x1248c                  ; if (deg<0) OR (deg<=360): goto end (skip mod)
    ///                                 ; Actually fccmp sets Z/N based on deg vs 360 when pl,
    ///                                 ; else flags=#0 (LE). So b.le triggers when deg in [0,360]
    ///                                 ; OR deg<0 (but fcmp before set N=1 for deg<0 → flags
    ///                                 ; not overwritten by fccmp).
    /// ; --- mod branch ---
    /// fcvtzs x8, s0                 ; x8 = (i64) deg (truncate toward zero)
    /// mov w9, #0x60b7
    /// movk w9, #0xb60b, lsl #16     ; w9 = 0xb60b60b7 (signed magic for /360)
    /// smull x9, w8, w9              ; x9 = (i64)w8 * w9
    /// lsr x9, x9, #32               ; x9 = high 32 bits of product (unsigned)
    /// add w8, w9, w8                ; w8 = high32 + n
    /// asr w9, w8, #8                ; w9 = w8 >> 8 (arithmetic)
    /// add w8, w9, w8, lsr #31       ; w8 = w9 + (w8 >> 31 unsigned) = q (= floor(n/360))
    /// scvtf s1, w8                  ; s1 = (f32) q
    /// mov w8, #-0x3c4c0000          ; w8 = 0xc3b40000 = float(-360.0)
    /// fmov s2, w8                   ; s2 = -360.0
    /// fmadd s0, s1, s2, s0          ; s0 = q * (-360.0) + deg = deg - q*360
    /// mov w8, #0x43b40000           ; w8 = 360.0
    /// fmov s1, w8
    /// fadd s1, s0, s1               ; s1 = s0 + 360
    /// fcmp s0, #0.0
    /// fcsel s0, s1, s0, mi          ; if (s0 < 0): s0 = s0 + 360
    /// end:
    ///   (caller: str s0, [x0])
    /// ret
    /// ```
    ///
    /// Semantic: returns `deg mod 360` in `[0, 360)`. byte-eq via exact magic-multiply
    /// trick (deviation: f32 edge values near 360 may diverge from naive
    /// `rem_euclid`).
    pub fn constrain(deg: f32) -> f32 {
        // raw early-exit: 0 <= deg < 360 → return as-is.
        // The fccmp pattern: fcmp sets flags from (deg vs 0); fccmp then either
        // re-compares (if pl, i.e. deg>=0) with 360, or sets flags=0 (le case).
        // b.le triggers on (Z=1 || N!=V). After fcmp: N=1 if deg<0; after fccmp:
        // if deg>=0, recomp deg vs 360 (so b.le if deg<=360); if deg<0, flags
        // forced to #0x0 (N=0,Z=0,C=0,V=0 → LE because Z=0,N=0,V=0: N==V → not LT,
        // not LE actually... need careful: LE = Z || (N != V)). With flags=0:
        // Z=0, N=0, V=0 → N==V, !Z → GT, so b.le not taken. So if deg<0, we
        // proceed to mod calc. If 0<=deg<=360, b.le taken → skip.
        // Hmm — boundary deg=360 exactly is also skipped → result = 360.0 (not normalized).
        // We mirror raw exactly:
        if deg >= 0.0 && deg <= 360.0 {
            return deg;
        }
        Self::constrain_mod(deg)
    }

    /// Inner: signed magic-multiply mod 360 + sign fix.
    fn constrain_mod(deg: f32) -> f32 {
        // fcvtzs x8, s0 → 64-bit signed truncate. ARMv8 saturates on overflow.
        let n_i64: i64 = if deg.is_nan() {
            0
        } else if deg >= (i64::MAX as f32) {
            i64::MAX
        } else if deg <= (i64::MIN as f32) {
            i64::MIN
        } else {
            deg as i64
        };
        // smull x9, w8, w9 — uses low 32 bits of n_i64.
        let w8: i32 = n_i64 as i32;
        let magic: i32 = 0xB60B60B7u32 as i32; // signed = -0x49F49F49
        let prod: i64 = (w8 as i64) * (magic as i64);
        // lsr x9, x9, #32 — unsigned right shift, take high 32 bits.
        let high32: u32 = (prod as u64 >> 32) as u32;
        // add w8, w9, w8 — 32-bit add (wrap).
        let sum: i32 = (high32 as i32).wrapping_add(w8);
        // asr w9, w8, #8 → arithmetic right shift.
        let asr8: i32 = sum >> 8;
        // add w8, w9, w8, lsr #31 → q = asr8 + (sum as u32 >> 31) as i32.
        let q: i32 = asr8.wrapping_add(((sum as u32) >> 31) as i32);
        // s1 = (f32) q (scvtf signed convert).
        let q_f: f32 = q as f32;
        // s0 = q_f * (-360.0) + deg.
        let r: f32 = q_f.mul_add(-360.0, deg);
        // if r < 0: r += 360.
        if r < 0.0 {
            r + 360.0
        } else {
            r
        }
    }

    /// `~Degree()` (`0x12490` sz=4): no-op POD.
    pub const fn drop_noop(self) {}

    /// `operator=(float)` (`0x12498` sz=96): same as `Degree(float)` then store.
    pub fn assign_float(&mut self, deg: f32) {
        self.value = Self::constrain(deg);
    }

    /// `operator+=(Degree const&)` (`0x124f8` sz=108):
    /// `s0 = this->value + other->value; Constrain; store`.
    pub fn add_assign_degree(&mut self, other: &Degree) {
        let sum = self.value + other.value;
        self.value = Self::constrain(sum);
    }

    /// `operator+=(float)` (`0x1256c` sz=104).
    pub fn add_assign_float(&mut self, v: f32) {
        let sum = self.value + v;
        self.value = Self::constrain(sum);
    }

    /// `operator-=(Degree const&)` (`0x125d4` sz=108).
    pub fn sub_assign_degree(&mut self, other: &Degree) {
        let diff = self.value - other.value;
        self.value = Self::constrain(diff);
    }

    /// `operator-=(float)` (`0x12640` sz=104).
    pub fn sub_assign_float(&mut self, v: f32) {
        let diff = self.value - v;
        self.value = Self::constrain(diff);
    }

    /// `operator-() const` (`0x126a8` sz=108) — sret returns `Degree(-this.value)`.
    ///
    /// raw asm 흐름:
    /// ```text
    /// ldr s1, [x0]                  ; s1 = this->value
    /// fneg s0, s1                   ; s0 = -this->value
    /// fcmp s1, #0.0                 ; s1 vs 0
    /// mov w9, #-0x3c4c0000           ; w9 = -360.0
    /// fmov s2, w9
    /// fccmp s1, s2, #0x8, le         ; if le: compare s1 vs -360
    /// b.pl 0x12708                   ; if pl (s1 >= -360 AND s1 <= 0): skip mod calc
    ///                                  ; → output is just -s1.
    /// ; --- mod branch (s1 > 0 OR s1 < -360) ---
    /// fcvtzs x9, s0                  ; s0 = -s1 here; truncate
    /// ... same magic-multiply mod 360 + +360 fix ...
    /// str s0, [x8]                   ; sret
    /// ```
    ///
    /// 즉 `0 <= s1 <= 360` 면 단순 `-s1` 반환. 아니면 `Constrain(-s1)`.
    /// (the special case `s1 in [0, 360]` 은 -s1 in [-360, 0], constrain 후 [0, 360].
    /// 단순 negate 와 다른 동작이지만, raw 가 그렇다 — 정공법 1:1.)
    pub fn neg(&self) -> Degree {
        let s1 = self.value;
        let neg = -s1;
        if s1 >= 0.0 && s1 <= 360.0 {
            // raw: skip mod, output = -s1.
            return Degree { value: neg };
        }
        Degree { value: Self::constrain(neg) }
    }

    /// `Swap(Degree&)` (`0x12710` sz=16).
    pub fn swap(&mut self, other: &mut Degree) {
        std::mem::swap(&mut self.value, &mut other.value);
    }

    /// `GetValue() const` (`0x12564` sz=8) — return f32.
    pub fn get_value(&self) -> f32 {
        self.value
    }

    /// `ToRadian() const` (`0x12724` sz=36).
    ///
    /// raw: `(double)value * (double)π/180.0; (float) result`.
    /// magic const `0x3f91df46a2529d39` = f64(π/180) ≈ 0.017453292519943295.
    pub fn to_radian(&self) -> f32 {
        let d: f64 = self.value as f64;
        // f64 bit pattern from raw `mov x8, #0x9d39; movk #0xa252,16; movk #0xdf46,32; movk #0x3f91,48`.
        let pi_180_bits: u64 = 0x3f91_df46_a252_9d39;
        let pi_180: f64 = f64::from_bits(pi_180_bits);
        (d * pi_180) as f32
    }

    /// `Normalize(Degree const& step)` (`0x1274c` sz=148).
    ///
    /// raw asm:
    /// ```text
    /// ; this->value rounded to nearest multiple of step->value.
    /// s0 = this->value
    /// s1 = step->value
    /// s2 = 0.5; s2 = step * 0.5
    /// s0 = s0 + s2                ; this + step/2 (round-half)
    /// s0 = s0 / s1                ; / step
    /// x8 = (i32) trunc(s0)         ; fcvtzs
    /// s0 = (f32) x8                ; scvtf
    /// s0 = step * s0               ; quantized value
    /// ... Constrain(s0) ...
    /// ; final: if (s0 == 360.0): s0 = 0.0 (raw `fcmp s0, s1; fcsel s0, s1, s0, eq` with s1=0)
    /// str s0, [x0]
    /// ```
    pub fn normalize(&mut self, step: &Degree) {
        let s = step.value;
        let half = s * 0.5;
        let q = ((self.value + half) / s).trunc();
        let quantized = s * q;
        let constrained = Self::constrain(quantized);
        // raw final guard: if constrained == 360.0, set to 0.0.
        let final_val = if constrained == 360.0 { 0.0 } else { constrained };
        self.value = final_val;
    }

    /// `FlipWidth()` (`0x127e0` sz=56).
    ///
    /// raw asm:
    /// ```text
    /// s0 = this->value
    /// s1 = 180.0 (= 0x43340000)
    /// fcmp s0, s1                  ; s0 vs 180
    /// d2 = 0
    /// fccmp s0, s2, #0x8, mi        ; if mi (s0 < 180): compare s0 vs 0; flags=8 (Z=1) if eq
    /// s2 = 360.0
    /// s2 = 360 - s0                 ; s2 = 360.0 - s0
    /// s0 = -s0                      ; fneg
    /// fcsel s0, s0, s2, ge          ; if ge (s0 >= 0 OR s0 >= 180): pick -s0, else 360-s0
    /// s0 = s0 + 180.0
    /// store
    /// ```
    ///
    /// 의미: width-axis mirror (예: 90° → 90° (270 → -270+180 = -90 → 90 어쩌고...
    /// raw 그대로 port — 분석은 후속 필요).
    pub fn flip_width(&mut self) {
        let s0 = self.value;
        let s1: f32 = 180.0;
        // raw: ge branch picks -s0 when (s0 >= 180 OR s0 >= 0 after second fccmp).
        // Reproducing the fcsel: condition is "ge" from the combined flags.
        // First fcmp(s0, 180): N=1 if s0<180. fccmp triggers only if mi (s0<180):
        //   compares s0 vs 0, setting flags. If s0>=180, flags retain (Z=0,N=0,V=0,C=1).
        // ge condition: N==V (no signed less-than). With flags(N=0,V=0): GE true.
        //   With s0<180 case: fccmp(s0,0) → N=1 if s0<0 else 0; V=0.
        //   GE: N==V → if s0>=0 (N=0=V), pick -s0; if s0<0 (N=1 != V=0), pick 360-s0.
        // Combined: pick = (s0 >= 180) || (s0 >= 0 && s0 < 180) = (s0 >= 0). Else 360-s0.
        let pick = if s0 >= 0.0 { -s0 } else { 360.0 - s0 };
        self.value = pick + s1;
    }

    /// `ToDegree(float radians)` static (`0x12818` sz=68).
    ///
    /// raw: `(double)radians * (double)(180/π); (float); Constrain`.
    /// magic const `0x404ca5dc1a63c1f8` = f64(180/π) ≈ 57.29577951308232.
    pub fn to_degree(radians: f32) -> Degree {
        let r: f64 = radians as f64;
        let inv_bits: u64 = 0x404c_a5dc_1a63_c1f8;
        let inv: f64 = f64::from_bits(inv_bits);
        let deg: f32 = (r * inv) as f32;
        Degree::from_float(deg)
    }
}

// PartialOrd / Ord 는 raw 에 없음 — Degree 값 비교는 호출자가 GetValue() 후 직접.
// raw operator==/!= 도 정의되지 않음.

impl PartialOrd for Degree {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ctor_is_zero() {
        // raw 0x12364: `str wzr, [x0]; ret`.
        let d = Degree::new();
        assert_eq!(d.value, 0.0);
    }

    #[test]
    fn from_float_in_range_passes_through() {
        // raw Constrain 의 early-exit: 0..=360 그대로.
        assert_eq!(Degree::from_float(0.0).value, 0.0);
        assert_eq!(Degree::from_float(45.0).value, 45.0);
        assert_eq!(Degree::from_float(180.0).value, 180.0);
        assert_eq!(Degree::from_float(359.999).value, 359.999);
        assert_eq!(Degree::from_float(360.0).value, 360.0);
    }

    #[test]
    fn from_float_negative_wraps_to_positive() {
        // -90 → 270 (after magic mod + sign fix).
        assert!((Degree::from_float(-90.0).value - 270.0).abs() < 0.001);
        assert!((Degree::from_float(-180.0).value - 180.0).abs() < 0.001);
        assert!((Degree::from_float(-360.0).value - 0.0).abs() < 0.001);
    }

    #[test]
    fn from_float_over_360_wraps() {
        assert!((Degree::from_float(450.0).value - 90.0).abs() < 0.001);
        assert!((Degree::from_float(720.0).value - 0.0).abs() < 0.001);
        assert!((Degree::from_float(721.0).value - 1.0).abs() < 0.001);
    }

    #[test]
    fn get_value_returns_field() {
        // raw 0x12564: `ldr s0, [x0]; ret`.
        let d = Degree::from_float(123.0);
        assert_eq!(d.get_value(), 123.0);
    }

    #[test]
    fn assign_float_constrains_and_stores() {
        let mut d = Degree::new();
        d.assign_float(450.0);
        assert!((d.value - 90.0).abs() < 0.001);
    }

    #[test]
    fn add_assign_degree() {
        let mut d = Degree::from_float(300.0);
        let other = Degree::from_float(100.0);
        d.add_assign_degree(&other);
        // 300 + 100 = 400 → 40.
        assert!((d.value - 40.0).abs() < 0.001);
    }

    #[test]
    fn add_assign_float() {
        let mut d = Degree::from_float(350.0);
        d.add_assign_float(20.0);
        // 350 + 20 = 370 → 10.
        assert!((d.value - 10.0).abs() < 0.001);
    }

    #[test]
    fn sub_assign_degree_wraps() {
        let mut d = Degree::from_float(30.0);
        let other = Degree::from_float(50.0);
        d.sub_assign_degree(&other);
        // 30 - 50 = -20 → 340.
        assert!((d.value - 340.0).abs() < 0.001);
    }

    #[test]
    fn sub_assign_float() {
        let mut d = Degree::from_float(100.0);
        d.sub_assign_float(50.0);
        assert!((d.value - 50.0).abs() < 0.001);
    }

    #[test]
    fn neg_in_normal_range_returns_simple_neg() {
        // raw: 0 <= s1 <= 360 → output = -s1 (no Constrain).
        // 즉 Degree(90).neg() = Degree { value: -90.0 } (not constrained!).
        let d = Degree::from_float(90.0);
        let n = d.neg();
        assert_eq!(n.value, -90.0, "raw: skip Constrain when value in [0,360]");
    }

    #[test]
    fn neg_out_of_range_constrains() {
        // raw: s1 < -360 OR s1 > 360 (after assign from caller setting via Constrain
        // this shouldn't happen, but operator- 의 직접 호출 시).
        // Degree::from_float 는 항상 [0,360] 에 보장하므로 normal case 는 위 테스트.
        // 직접 value override 케이스.
        let d = Degree { value: 720.0 };
        let n = d.neg();
        // -720 → Constrain(-720) = 0.
        assert!((n.value - 0.0).abs() < 0.001);
    }

    #[test]
    fn swap_exchanges_values() {
        let mut a = Degree::from_float(10.0);
        let mut b = Degree::from_float(20.0);
        a.swap(&mut b);
        assert_eq!(a.value, 20.0);
        assert_eq!(b.value, 10.0);
    }

    #[test]
    fn to_radian_matches_pi_over_180() {
        // raw: f64 (π/180) = 0x3f91df46a2529d39.
        let d = Degree::from_float(180.0);
        let r = d.to_radian();
        assert!((r - std::f32::consts::PI).abs() < 1e-5);
        let d2 = Degree::from_float(90.0);
        assert!((d2.to_radian() - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let d3 = Degree::from_float(0.0);
        assert_eq!(d3.to_radian(), 0.0);
    }

    #[test]
    fn to_degree_static_converts_radians() {
        // raw: f64 (180/π) = 0x404ca5dc1a63c1f8.
        let d = Degree::to_degree(std::f32::consts::PI);
        assert!((d.value - 180.0).abs() < 0.001);
        let d2 = Degree::to_degree(std::f32::consts::FRAC_PI_2);
        assert!((d2.value - 90.0).abs() < 0.001);
        let d3 = Degree::to_degree(0.0);
        assert_eq!(d3.value, 0.0);
    }

    #[test]
    fn normalize_quantizes_to_step() {
        // 47.0 with step 30.0 → round-half: (47 + 15) / 30 = 2.066 → trunc=2 → 60.
        let mut d = Degree::from_float(47.0);
        d.normalize(&Degree::from_float(30.0));
        assert!((d.value - 60.0).abs() < 0.001);
        // 360 → 0 (raw final guard).
        let mut d2 = Degree { value: 358.0 };
        d2.normalize(&Degree::from_float(45.0));
        // (358 + 22.5) / 45 = 8.45 → trunc=8 → 360 → guard → 0.
        assert_eq!(d2.value, 0.0);
    }

    #[test]
    fn flip_width_inversion() {
        // raw 동작: s0 >= 0 → -s0 + 180. 즉 90 → -90+180 = 90.
        let mut d = Degree::from_float(90.0);
        d.flip_width();
        assert!((d.value - 90.0).abs() < 0.001);
        // 0 → 180.
        let mut d2 = Degree::from_float(0.0);
        d2.flip_width();
        assert!((d2.value - 180.0).abs() < 0.001);
        // 45 → -45+180 = 135.
        let mut d3 = Degree::from_float(45.0);
        d3.flip_width();
        assert!((d3.value - 135.0).abs() < 0.001);
    }

    #[test]
    fn constrain_static_matches_from_float() {
        // Constrain 정적 호출 = from_float 의 핵심.
        assert_eq!(Degree::constrain(0.0), 0.0);
        assert_eq!(Degree::constrain(180.0), 180.0);
        // raw early-exit: 360 그대로 (not 0).
        assert_eq!(Degree::constrain(360.0), 360.0);
        // 361 → mod.
        assert!((Degree::constrain(361.0) - 1.0).abs() < 0.001);
        assert!((Degree::constrain(-1.0) - 359.0).abs() < 0.001);
    }

    #[test]
    fn constrain_zero_is_zero() {
        assert_eq!(Degree::constrain(0.0), 0.0);
    }

    #[test]
    fn drop_noop_compiles() {
        // raw `~Degree() sz=4: ret`.
        let d = Degree::from_float(45.0);
        d.drop_noop();
    }
}
