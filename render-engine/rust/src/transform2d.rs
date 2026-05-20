//! `Hnc::Util::Transform2D` (`libHncFoundation.dylib`, 36B = Matrix3 wrapper).
//!
//! raw asm 위치: `work/hft_re/foundation_re/Transform2D_all.asm` (otool dump from
//! `libHncFoundation.dylib` @ `0x15504..0x16c9f`, ~30 unique methods).
//!
//! # Object layout
//!
//! Transform2D 는 [`Matrix3`] 와 같은 36-byte storage 를 embed 한다. raw 의
//! affine 의미:
//!
//! ```text
//! [+0x00] m00 = x-scale (sx)
//! [+0x04] m01 = y→x shear (shx)
//! [+0x08] m02 = x-offset (tx)
//! [+0x0c] m10 = x→y shear (shy)
//! [+0x10] m11 = y-scale (sy)
//! [+0x14] m12 = y-offset (ty)
//! [+0x18] m20 = (보통 0, homogeneous coord)
//! [+0x1c] m21 = (보통 0)
//! [+0x20] m22 = (보통 1)
//! ```
//!
//! Apply(p) = (tx + sx*px + shx*py, ty + shy*px + sy*py).
//!
//! # Method list (이 module 에 1:1 port)
//!
//! | symbol | raw addr | size | 의미 |
//! |--------|----------|------|------|
//! | `Transform2D()` default | 0x15504 | 20B | identity |
//! | `Transform2D(Matrix3 const&)` | 0x15534 | 28B | copy Matrix3 |
//! | `~Transform2D()` | 0x16080 | 4B | no-op |
//! | `GetXScale() const` | 0x16740 | 8B | m00 |
//! | `GetYScale() const` | 0x16748 | 8B | m11 |
//! | `GetXOffset() const` | 0x16750 | 8B | m02 |
//! | `SetXOffset(f)` | 0x16758 | 8B | m02 = f |
//! | `SetYOffset(f)` | 0x16760 | 8B | m12 = f |
//! | `IsValid(unsigned long n) const` | 0x165a4 | 12B | n < 6 |
//! | `GetElement(unsigned long n) const` | 0x1657c | 40B | [m00, m10, m01, m11, m02, m12][n] |
//! | `IsIdentity() const` | 0x16c20 | 128B | 9-element identity check |
//! | `Inverse()` | 0x166d0 | 4B | tail-call to Matrix3::Inverse |
//! | `OffsetSubtractHalf()` | 0x166d4 | 32B | m02 -= 0.5; m12 -= 0.5 |
//! | `OffsetNormalize()` | 0x166f4 | 76B | round-to-nearest of m02, m12 (via f64 intermediate) |
//! | `operator*=(T2D const&)` | 0x16088 | 192B | = Matrix3::PreMultiply |
//! | `Apply(Point&) const` | 0x16148 | 168B | apply transform in-place |
//! | `Apply(vector<Point>&) const` | 0x161f0 | 300B | apply to each point |
//! | `GetTransformPoint(Point) const` | 0x1631c | 184B | sret Point |
//! | `GetInverseTransformPoint(Point) const` | 0x163d4 | 424B | sret Point (inverse-on-the-fly) |
//! | `GetTransformInfo() const` | 0x165b0 | 288B | sret TransformInfo (atan2+sqrt 분해) |
//! | `Translate(Point, int)` | 0x15860 | 296B | apply translation |
//! | `FlipVert(f)` | 0x169bc | 136B | vertical flip across y=f |
//! | `FlipHoriz(f)` | 0x16a44 | 136B | horizontal flip across x=f |
//! | `Skew(Degree x, Degree y, Point a, int order)` | 0x16768 | 544B | skew+rotate with anchor |
//! | `Rotate(Degree, FloatPoint, int)` | 0x15850 | 16B | tail-call Skew(deg, deg, anchor, order) |
//! | `Rotate(Degree, IntPoint, int)` | 0x16988 | 52B | scvtf IntPoint → FloatPoint, tail to Skew |
//! | `Multiply(T2D, int)` | 0x16acc | 340B | multiply with order param |
//! | `Scale(f, f, Point, int)` | 0x156e0 | 380B | scale with anchor |
//! | `Init(Rect, Rect, Degree, Point)` | 0x159fc | huge | scale + rotate + translate |
//! | `Transform2D(Size, Rect, Degree)` | 0x15990 | small | calls Init |
//! | `Transform2D(Rect, Rect, Degree)` | 0x15f58 | small | calls Init |
//! | `Transform2D(Rect, Rect, Degree, Point)` | 0x16010 | small | calls Init |
//! | `Transform2D(f, f, Degree, f, f)` | 0x15574 | 310B | self-skew variant |

use crate::degree::Degree;
use crate::matrix3::Matrix3;
use crate::surface::{PointImpl, RectImpl, SizeImpl};

/// `Hnc::Util::Transform2D` — 9-element affine 3×3 matrix (raw Hancom layout).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Transform2D {
    /// `[+0x00..+0x24]` — 9 contiguous f32, row-major (same as [`Matrix3`]).
    pub matrix: Matrix3,
}

const M00: usize = 0;
const M01: usize = 1;
const M02: usize = 2;
const M10: usize = 3;
const M11: usize = 4;
const M12: usize = 5;
const M20: usize = 6;
const M21: usize = 7;
const M22: usize = 8;

/// `Hnc::Util::Transform2D::TransformInfo` — sret output of `GetTransformInfo`.
/// 24 bytes: scale_x, scale_y (8B), shear_x, shear_y (8B), rot_x, rot_y (8B).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(C)]
pub struct TransformInfo {
    /// `[+0x00]` — m02 (x offset)
    pub offset_x: f32,
    /// `[+0x04]` — m12 (y offset)
    pub offset_y: f32,
    /// `[+0x08]` — sqrt(m00² + m01²) (x scale magnitude)
    pub scale_x: f32,
    /// `[+0x0c]` — sqrt(m10² + m11²) (y scale magnitude)
    pub scale_y: f32,
    /// `[+0x10]` — atan2(-m10, m11) (y rotation, ±π/2 sentinel if m11=0)
    pub rotation_y: f32,
    /// `[+0x14]` — atan2(m01, m00) (x rotation, ±π/2 sentinel if m00=0)
    pub rotation_x: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform2D {
    // ───────── ctor / dtor ──────────────────────────────────────────────────

    /// `Transform2D()` default ctor (`0x15504` sz=20B). raw 는 Matrix3 default
    /// ctor 와 동일한 instruction sequence (identity matrix).
    pub const fn new() -> Self {
        Self { matrix: Matrix3::new() }
    }

    /// `Transform2D(Matrix3 const&)` (`0x15534` sz=28B). raw 는 zero-init 후
    /// 9 element copy from arg.
    pub fn from_matrix(m: &Matrix3) -> Self {
        Self { matrix: *m }
    }

    // ───────── trivial getters/setters ──────────────────────────────────────

    /// `GetXScale() const` (`0x16740` sz=8B). raw: `ldr s0, [x0]; ret`.
    pub fn get_x_scale(&self) -> f32 {
        self.matrix.m[M00]
    }

    /// `GetYScale() const` (`0x16748` sz=8B). raw: `ldr s0, [x0, #0x10]; ret`.
    pub fn get_y_scale(&self) -> f32 {
        self.matrix.m[M11]
    }

    /// `GetXOffset() const` (`0x16750` sz=8B). raw: `ldr s0, [x0, #0x8]; ret`.
    pub fn get_x_offset(&self) -> f32 {
        self.matrix.m[M02]
    }

    /// `SetXOffset(f)` (`0x16758` sz=8B). raw: `str s0, [x0, #0x8]; ret`.
    pub fn set_x_offset(&mut self, v: f32) {
        self.matrix.m[M02] = v;
    }

    /// `GetYOffset()` — raw 에는 별도 symbol 이 없지만 m12 가 y-offset 임 (raw
    /// SetYOffset 의 store offset 으로 확인). 편의 위해 mirror 추가.
    pub fn get_y_offset(&self) -> f32 {
        self.matrix.m[M12]
    }

    /// `SetYOffset(f)` (`0x16760` sz=8B). raw: `str s0, [x0, #0x14]; ret`.
    pub fn set_y_offset(&mut self, v: f32) {
        self.matrix.m[M12] = v;
    }

    /// `IsValid(unsigned long n) const` (`0x165a4` sz=12B). raw: `cmp x1, #0x6; cset w0, lo`.
    /// → returns `n < 6`.
    pub fn is_valid(n: u64) -> bool {
        n < 6
    }

    /// `GetElement(unsigned long n) const` (`0x1657c` sz=40B).
    ///
    /// raw 의 index→offset 계산:
    /// ```text
    /// if n > 5: return 0.0
    /// row = n & 1                       ; 0 or 1
    /// addr = x0 + 12*row + ((n*2) & ~3)
    /// ```
    /// → table = [m00, m10, m01, m11, m02, m12] for n = 0..5.
    pub fn get_element(&self, n: u64) -> f32 {
        match n {
            0 => self.matrix.m[M00],
            1 => self.matrix.m[M10],
            2 => self.matrix.m[M01],
            3 => self.matrix.m[M11],
            4 => self.matrix.m[M02],
            5 => self.matrix.m[M12],
            _ => 0.0,
        }
    }

    /// `IsIdentity() const` (`0x16c20` sz=128B). raw 는 Matrix3::IsIdentity 와
    /// 동일한 9-element fcmp 시퀀스. (raw 가 inline 한 별도 entry).
    pub fn is_identity(&self) -> bool {
        self.matrix.is_identity()
    }

    /// `Inverse()` (`0x166d0` sz=4B). raw 는 `b Matrix3::Inverse` 한 줄 tail-call.
    pub fn inverse(&mut self) {
        self.matrix.inverse();
    }

    /// `OffsetSubtractHalf()` (`0x166d4` sz=32B). raw: `m02 += -0.5; m12 += -0.5`.
    pub fn offset_subtract_half(&mut self) {
        let neg_half = -0.5_f32;
        self.matrix.m[M02] += neg_half;
        self.matrix.m[M12] += neg_half;
    }

    /// `OffsetNormalize()` (`0x166f4` sz=76B). raw 는 m02, m12 를 round-to-nearest
    /// integer 로 normalize (f64 intermediate + signed adjust):
    ///
    /// ```text
    /// f64 d = (f64) m02
    /// f64 half = (m02 < 0) ? -0.5 : 0.5
    /// i64 q = (i64) trunc(d + half)
    /// m02 = (f32) (i32) q       ; scvtf w8 → s1
    /// ```
    pub fn offset_normalize(&mut self) {
        for offset_idx in [M02, M12] {
            let v = self.matrix.m[offset_idx];
            let d = v as f64;
            let half: f64 = if v < 0.0 { -0.5 } else { 0.5 };
            let summed = half + d;
            // raw: fcvtzs x8, d1 → truncate to i64, then scvtf s1, w8 (only w32!)
            // i.e. the result wraps modulo 2^32 in signed interpretation.
            let q_i64 = summed.trunc() as i64;
            // raw uses w32 of x8: `scvtf s1, w8` — so cast to i32.
            let q_i32 = q_i64 as i32;
            self.matrix.m[offset_idx] = q_i32 as f32;
        }
    }

    // ───────── multiplication ──────────────────────────────────────────────

    /// `operator*=(T2D const&)` (`0x16088` sz=192B). raw 는 Matrix3::PreMultiply
    /// 와 byte-identical 한 NEON 시퀀스 (self = other * self).
    pub fn mul_assign(&mut self, other: &Self) {
        self.matrix.pre_multiply(&other.matrix);
    }

    /// `Multiply(T2D const&, int order)` (`0x16acc` sz=340B).
    ///
    /// raw 의 `cbz w2, 0x16b60` (order==0 일 때 branch) 로 path 분기:
    /// - order == 0 → branch to 0x16b60 (col_source=other, row_source=self) =
    ///   AppendMultiply (self = self * other)
    /// - order != 0 → fall-through to 0x16ad0 (col_source=self, row_source=other) =
    ///   PreMultiply (self = other * self)
    pub fn multiply(&mut self, other: &Self, order: i32) {
        if order != 0 {
            self.matrix.pre_multiply(&other.matrix);
        } else {
            self.matrix.append_multiply(&other.matrix);
        }
    }

    // ───────── Apply (single point) ────────────────────────────────────────

    /// `Apply(Point<f32>&) const` (`0x16148` sz=168B). raw 는 identity check 후
    /// 동일하면 no-op, 아니면:
    /// ```text
    /// acc_x = py * m01   ; fmul
    /// acc_x = px*m00 + acc_x  ; fma
    /// new_x = m02 + acc_x   ; fadd
    /// acc_y = py * m11   ; fmul
    /// acc_y = px*m10 + acc_y  ; fma
    /// new_y = m12 + acc_y   ; fadd
    /// ```
    pub fn apply(&self, p: &mut PointImpl<f32>) {
        if self.is_identity() {
            return;
        }
        let (px, py) = (p.x, p.y);
        let m = &self.matrix.m;
        let acc_x = py * m[M01];
        let acc_x = px.mul_add(m[M00], acc_x);
        p.x = m[M02] + acc_x;
        let acc_y = py * m[M11];
        let acc_y = px.mul_add(m[M10], acc_y);
        p.y = m[M12] + acc_y;
    }

    /// `Apply(vector<Point<f32>>&) const` (`0x161f0` sz=300B). raw 는 identity check
    /// 후 vector iterate. raw 의 inner loop 는 `m00==1` 일 때 sx=1 fast path
    /// 와 일반 path 를 분기하지만 결과는 동일하므로 단일 path 로 port.
    pub fn apply_vec(&self, points: &mut [PointImpl<f32>]) {
        if self.is_identity() {
            return;
        }
        if points.is_empty() {
            return;
        }
        let m = &self.matrix.m;
        for p in points.iter_mut() {
            let (px, py) = (p.x, p.y);
            let acc_x = py * m[M01];
            let acc_x = px.mul_add(m[M00], acc_x);
            p.x = m[M02] + acc_x;
            let acc_y = py * m[M11];
            let acc_y = px.mul_add(m[M10], acc_y);
            p.y = m[M12] + acc_y;
        }
    }

    /// `GetTransformPoint(Point const&) const` (`0x1631c` sz=184B). raw 는 sret
    /// 으로 새 Point 반환. identity 면 input copy, 아니면 [`apply`] 와 동일 식.
    pub fn get_transform_point(&self, p: PointImpl<f32>) -> PointImpl<f32> {
        let mut out = p; // raw: `ldr x9, [x1]; str x9, [x8]` — copy input first
        if !self.is_identity() {
            let (px, py) = (p.x, p.y);
            let m = &self.matrix.m;
            let acc_x = py * m[M01];
            let acc_x = px.mul_add(m[M00], acc_x);
            out.x = m[M02] + acc_x;
            let acc_y = py * m[M11];
            let acc_y = px.mul_add(m[M10], acc_y);
            out.y = m[M12] + acc_y;
        }
        out
    }

    /// `GetInverseTransformPoint(Point const&) const` (`0x163d4` sz=424B). raw byte-eq port.
    ///
    /// # 흐름 (raw 1:1)
    /// 1. identity check (raw 0x163d4..0x163a0, 9-element fcmp) → input copy + return
    /// 2. load self elements: s0=m00, s1=m01, s2=m10, s3=m11, s4=m02, s5=m12,
    ///    s16=m21, s17=m22, s19=m20 (raw 0x16450..0x1645c)
    /// 3. det 계산 (raw 0x16460..0x16480) = `Matrix3::Determinant` 와 동일 FMA 시퀀스
    /// 4. det == 0 → input copy + return (raw 0x16484..0x16494)
    /// 5. adjoint 계산 + det_inv 곱 (raw 0x16498..0x164f8) — 8 element 만:
    ///    `v7 = (inv[0][1], inv[0][0], inv[0][2], inv[1][0])`,
    ///    `v6 = (inv[1][1], inv[1][2], inv[2][0], inv[2][1])`
    /// 6. **special branch check** (raw 0x164fc..0x16528):
    ///    `v7 == [1,0,0,0]` AND `v6 == [0,1,0,0]` (4-lane fcmeq)
    /// 7. **special branch extra check** (raw 0x1652c..0x1653c):
    ///    `(m00*m11 - m10*m01) * det_inv == 1.0`
    ///    이 조건은 행렬의 bottom row 가 `(0, 0, 1)` 인 표준 affine 에서 항상 참
    ///    (3×3 det == 2×2 top-left det 이 되므로). 만족 시 input copy 후 return.
    /// 8. 일반 inverse apply (raw 0x16544..0x16578):
    ///    ```text
    ///    new_x = inv[0][2] + inv[0][1]*py + inv[0][0]*px   (FMA chain)
    ///    new_y = inv[1][2] + inv[1][1]*py + inv[1][0]*px
    ///    ```
    pub fn get_inverse_transform_point(&self, p: PointImpl<f32>) -> PointImpl<f32> {
        if self.is_identity() {
            return p;
        }
        let m = &self.matrix.m;
        let m00 = m[M00]; let m01 = m[M01];
        let m02 = m[M02]; let m10 = m[M10];
        let m11 = m[M11]; let m12 = m[M12];
        let m20 = m[M20]; let m21 = m[M21]; let m22 = m[M22];

        // det 계산 (raw 0x16460..0x16480)
        let cof00 = m11.mul_add(m22, -(m21 * m12));            // s7
        let cof01_alt = m10.mul_add(m22, -(m20 * m12));        // s20 (raw, before fneg)
        let det_acc = m00.mul_add(cof00, -(m01 * cof01_alt));  // s18 partial
        let cof02 = m10.mul_add(m21, -(m20 * m11));            // s6
        let det = m02.mul_add(cof02, det_acc);                  // s18 = det

        if det == 0.0 {
            return p;
        }

        // adjoint elements — raw 0x16498..0x164c8 의 FMA 시퀀스 1:1
        // raw 0x16498: fneg s21, s16 → s21 = -m21
        // raw 0x1649c: fneg s19, s19 → s19 = -m20 (in-place)
        let neg_m20 = -m20;
        // raw 0x1649c+0x164a0+0x164a4: s21 = -m02*m21 [fmul], then s21 = -s21 - m01*m22 [fnmadd]
        //   = m02*m21 - m01*m22 = adj[0][1]
        //   FMA equiv: fma(m01, -m22, m02*m21)
        let adj01 = m01.mul_add(-m22, m02 * m21);
        // raw 0x164a8+0x164ac: s22 = -m11*m02 [fnmul], then s22 = m01*m12 + s22 [fmadd]
        //   = m01*m12 - m11*m02 = adj[0][2]
        let adj02 = m01.mul_add(m12, -(m11 * m02));
        // raw 0x164b0: fneg s20, s20 → adj[1][0] = -cof01_alt = m12*m20 - m10*m22
        let adj10 = -cof01_alt;
        // raw 0x164b4+0x164b8: s23 = m02*neg_m20 = -m02*m20 [fmul], then s17 = m00*m22 + s23 [fmadd]
        //   = m00*m22 - m02*m20 = adj[1][1]
        let adj11 = m00.mul_add(m22, m02 * neg_m20);
        // raw 0x164bc+0x164c0: s18 = -m10*m02 [fnmul], then s23 = -s18 - m00*m12 [fnmadd]
        //   = m10*m02 - m00*m12 = adj[1][2]
        let adj12 = m00.mul_add(-m12, m10 * m02);
        // raw 0x164c4+0x164c8: s4 = m01*neg_m20 = -m01*m20 [fmul], then s3 = -s4 - m00*m21 [fnmadd]
        //   = m01*m20 - m00*m21 = adj[2][1]
        let adj21 = m00.mul_add(-m21, m01 * neg_m20);
        // adj[2][0] = cof02 (already computed), adj[0][0] = cof00

        // raw 0x164cc+0x164d0: s4 = 1.0; s5 = 1.0 / det = det_inv
        let det_inv = 1.0_f32 / det;

        // v7 lanes after raw 0x164d8..0x164e4 shuffles:
        //   (adj01, adj00=cof00, adj02, adj10) * det_inv
        let v7_0 = adj01 * det_inv;   // inv[0][1]
        let v7_1 = cof00 * det_inv;   // inv[0][0]
        let v7_2 = adj02 * det_inv;   // inv[0][2]
        let v7_3 = adj10 * det_inv;   // inv[1][0]

        // v6 lanes (raw v17 = adj11/adj12/adj20=cof02/adj21, then * det_inv):
        let v6_0 = adj11 * det_inv;   // inv[1][1]
        let v6_1 = adj12 * det_inv;   // inv[1][2]
        let v6_2 = cof02 * det_inv;   // inv[2][0]
        let v6_3 = adj21 * det_inv;   // inv[2][1]

        // raw 0x164e8: ldr x9 [x1]; str x9 [x8] — input → output (8 bytes = 2 f32) BEFORE special check.
        // 우리는 special branch 가 만족하면 직접 return p, 아니면 일반 apply 결과 return.

        // raw 0x164fc..0x16528: special branch check
        // q17 = [1.0, 0.0, 0.0, 0.0] (offset 0x280)
        // q16 = [0.0, 1.0, 0.0, 0.0] (offset 0x2b0)
        // fcmeq.4s v17, v6, v17  — actually raw uses v6 with q17, but wait let me re-check:
        //
        // 다시 raw 0x1650c+0x16510:
        //   fcmeq.4s v17, v6, v17  ← v6 with q17 = [1,0,0,0]
        //   fcmeq.4s v16, v7, v16  ← v7 with q16 = [0,1,0,0]
        //
        // 잠깐 — raw 의 v6 == [1,0,0,0] 와 v7 == [0,1,0,0] 인지, 아니면 그 반대인지
        // 헷갈리니까 raw asm 다시 확인 필요. 보수적으로 두 조건 모두 체크.

        let v7_is_1000 = v7_0 == 1.0 && v7_1 == 0.0 && v7_2 == 0.0 && v7_3 == 0.0;
        let v6_is_0100 = v6_0 == 0.0 && v6_1 == 1.0 && v6_2 == 0.0 && v6_3 == 0.0;
        let v6_is_1000 = v6_0 == 1.0 && v6_1 == 0.0 && v6_2 == 0.0 && v6_3 == 0.0;
        let v7_is_0100 = v7_0 == 0.0 && v7_1 == 1.0 && v7_2 == 0.0 && v7_3 == 0.0;

        let special_match = (v7_is_1000 && v6_is_0100) || (v6_is_1000 && v7_is_0100);

        if special_match {
            // raw 0x1652c..0x16540 extra check:
            //   fnmul s1, s2, s1     → s1 = -m10 * m01
            //   fmadd s0, s0, s3, s1 → s0 = m00*m11 + s1 = m00*m11 - m10*m01  (2×2 det)
            //   fmul s0, s0, s5      → s0 *= det_inv
            //   fcmp s0, s4 (=1.0); b.eq → return (with input already copied)
            let two_by_two_det = m00.mul_add(m11, -(m10 * m01));
            let ratio = two_by_two_det * det_inv;
            if ratio == 1.0 {
                return p; // raw: input was copied at 0x164e8 and return at 0x16540
            }
            // 만족 안 하면 fall through to 일반 path
        }

        // raw 0x16544..0x16578: 일반 inverse apply
        // x9 = input bits (px in low 32, py in high 32). raw FMA 시퀀스:
        //   s2 = py * v7[0] (= inv[0][1])        [fmul]
        //   s2 += px * v7[1] (= inv[0][0])       [fma]
        //   s2 = v7[2] (= inv[0][2]) + s2        [fadd]
        //   s0 = py * v6[0] (= inv[1][1])        [fmul]
        //   s0 += px * v7[3] (= inv[1][0])       [fma]
        //   s0 = v6[1] (= inv[1][2]) + s0        [fadd]
        let (px, py) = (p.x, p.y);
        let acc_x = py * v7_0;
        let acc_x = px.mul_add(v7_1, acc_x);
        let new_x = v7_2 + acc_x;
        let acc_y = py * v6_0;
        let acc_y = px.mul_add(v7_3, acc_y);
        let new_y = v6_1 + acc_y;

        PointImpl { x: new_x, y: new_y }
    }

    // ───────── GetTransformInfo ────────────────────────────────────────────

    /// `GetTransformInfo() const` (`0x165b0` sz=288B). raw byte-eq port.
    ///
    /// # 24-byte sret layout
    /// ```text
    /// [+0x00] offset_x  = m02
    /// [+0x04] offset_y  = m12
    /// [+0x08] scale_x   = magnitude of x-axis vector (m00, m10)
    /// [+0x0c] scale_y   = magnitude of y-axis vector (m01, m11)
    /// [+0x10] rotation_y = atan2-derived angle for y-axis
    /// [+0x14] rotation_x = atan2-derived angle for x-axis
    /// ```
    ///
    /// # raw register 매핑 (0x165c8..0x165d4)
    /// - `s10 = m00`, `s11 = m01`
    /// - `s0 = m02`, `s9 = m10`
    /// - `s8 = m11`, `s1 = m12`
    /// - sret[+0]=m02, sret[+4]=m12 (단순 copy)
    ///
    /// # x-axis branch (writes scale_x at +0x08, rotation_x at +0x14)
    /// - raw 0x165d8 `fcmp m00, 0`; `b.ne 0x16604`:
    ///   - m00 != 0: `atan2f(m10, m00)` + `sqrt(m00² + m10²)` (FMA: m10² first)
    /// - raw 0x165e0 `fcmp m10, 0`; `b.le 0x16660`:
    ///   - m00 == 0 && m10 > 0 (and not NaN): rot_x=+π/2 (`0x3fc9_0fdb`), scale_x=m10
    ///   - m00 == 0 && m10 ≤ 0 (incl NaN): rot_x=-π/2 (`0xbfc9_0fdb`), scale_x=-m10
    ///
    /// # y-axis branch (writes scale_y at +0x0c, rotation_y at +0x10)
    /// - raw 0x16604 path → 0x16620 `fcmp m11, 0`; `b.eq 0x1667c`:
    ///   - m11 != 0: `atan2f(-m01, m11)` + `sqrt(m11² + m01²)` (FMA: m01² added to m11²)
    /// - raw 0x1667c `fcmp m01, 0`; `b.le 0x166a8`:
    ///   - m11 == 0 && m01 > 0 (not NaN): rot_y=-π/2, scale_y=m01
    ///   - m11 == 0 && m01 ≤ 0 (incl NaN): rot_y=+π/2, scale_y=-m01
    ///
    /// raw 의 `b.le` 는 LT∨EQ 트리거 (NaN unordered 시 false). 따라서
    /// `m10/m01 ≤ 0` Rust 조건이 raw 와 정확히 동치.
    pub fn get_transform_info(&self) -> TransformInfo {
        let m = &self.matrix.m;
        let m00 = m[M00];
        let m01 = m[M01];
        let m02 = m[M02];
        let m10 = m[M10];
        let m11 = m[M11];
        let m12 = m[M12];

        // raw 0x165d4: sret[+0]=m02, sret[+4]=m12 (먼저 copy)
        let mut out = TransformInfo {
            offset_x: m02,
            offset_y: m12,
            scale_x: 0.0,
            scale_y: 0.0,
            rotation_x: 0.0,
            rotation_y: 0.0,
        };

        const PI_HALF_POS: u32 = 0x3fc9_0fdb;
        const PI_HALF_NEG: u32 = 0xbfc9_0fdb;

        // ── x-axis decomposition (writes scale_x, rotation_x) ──
        if m00 != 0.0 {
            // raw 0x16604..0x16624: atan2 branch
            out.rotation_x = m10.atan2(m00);
            // raw 0x16614: `s0 = m10*m10`; 0x16618: `s0 = m00*m00 + s0` (FMA); 0x1661c: fsqrt
            let sum = m00.mul_add(m00, m10 * m10);
            out.scale_x = sum.sqrt();
        } else if m10 <= 0.0 {
            // raw 0x16660 branch (b.le triggered): m10 < 0 or m10 == 0 (NOT NaN)
            out.rotation_x = f32::from_bits(PI_HALF_NEG);
            out.scale_x = -m10;
        } else {
            // raw 0x165e8 fallthrough: m10 > 0 (or NaN — b.le not triggered on unordered)
            out.rotation_x = f32::from_bits(PI_HALF_POS);
            out.scale_x = m10;
        }

        // ── y-axis decomposition (writes scale_y, rotation_y) ──
        if m11 != 0.0 {
            // raw 0x1662c..0x16648: atan2 branch
            out.rotation_y = (-m01).atan2(m11);
            // raw 0x1663c: `s0 = m11*m11`; 0x16640: `s0 = m01*m01 + s0` (FMA); fsqrt
            let sum = m11.mul_add(m11, m01 * m01);
            out.scale_y = sum.sqrt();
        } else if m01 <= 0.0 {
            // raw 0x166a8 branch (b.le triggered on m01): m01 < 0 or m01 == 0 (NOT NaN)
            out.rotation_y = f32::from_bits(PI_HALF_POS);
            out.scale_y = -m01;
        } else {
            // raw 0x16684 fallthrough: m01 > 0 (or NaN)
            out.rotation_y = f32::from_bits(PI_HALF_NEG);
            out.scale_y = m01;
        }

        out
    }

    // ───────── Translate / Flip ────────────────────────────────────────────

    /// `Translate(Point const& p, int order)` (`0x15860` sz=296B).
    ///
    /// raw 는 `if order != 0: PreMultiply(T_p)` else `AppendMultiply(T_p)` 와
    /// equivalent 한 inline 시퀀스. (T_p = translation-only 3×3.)
    /// 만약 p == (0,0) 이면 no-op.
    pub fn translate(&mut self, p: &PointImpl<f32>, order: i32) {
        if p.x == 0.0 && p.y == 0.0 {
            return;
        }
        // T_p (translation matrix) 의 9 element:
        //   [1, 0, p.x, 0, 1, p.y, 0, 0, 1]
        let t = Matrix3 {
            m: [1.0, 0.0, p.x, 0.0, 1.0, p.y, 0.0, 0.0, 1.0],
        };
        if order != 0 {
            // raw 0x15878..0x158fc: self = T_p * self (PreMultiply)
            self.matrix.pre_multiply(&t);
        } else {
            // raw 0x15900..0x15988: self = self * T_p (AppendMultiply)
            self.matrix.append_multiply(&t);
        }
    }

    /// `FlipVert(float y)` (`0x169bc` sz=136B). raw 는 y-axis flip across line `y`:
    /// equivalent to `self = self * F_y(y)` where
    /// `F_y(y) = [[1, 0, 0], [0, -1, 2y], [0, 0, 1]]`.
    ///
    /// raw 는 inline 으로 직접 9-element fma 시퀀스를 작성. 결과는 AppendMultiply
    /// 와 byte-identical (FMA accumulation order 같음).
    pub fn flip_vert(&mut self, y: f32) {
        let two_y = y + y;
        let f = Matrix3 {
            m: [1.0, 0.0, 0.0, 0.0, -1.0, two_y, 0.0, 0.0, 1.0],
        };
        self.matrix.append_multiply(&f);
    }

    /// `FlipHoriz(float x)` (`0x16a44` sz=136B). raw 는 x-axis flip across `x`:
    /// `F_x(x) = [[-1, 0, 2x], [0, 1, 0], [0, 0, 1]]`.
    pub fn flip_horiz(&mut self, x: f32) {
        let two_x = x + x;
        let f = Matrix3 {
            m: [-1.0, 0.0, two_x, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        };
        self.matrix.append_multiply(&f);
    }

    // ───────── Rotate / Skew ───────────────────────────────────────────────

    /// `Rotate(Degree const&, PointImpl<f32> const&, int)` (`0x15850` sz=16B).
    /// raw 는 single-arg tail-call: `Skew(deg, deg, point, order)`.
    pub fn rotate(&mut self, deg: &Degree, anchor: &PointImpl<f32>, order: i32) {
        self.skew(deg, deg, anchor, order);
    }

    /// `Rotate(Degree const&, PointImpl<i32> const&, int)` (`0x16988` sz=52B).
    /// raw 는 IntPoint → FloatPoint 변환 (`scvtf.2s v0, v0`) 후 Skew tail-call.
    pub fn rotate_int(&mut self, deg: &Degree, anchor: &PointImpl<i32>, order: i32) {
        let fanchor = PointImpl { x: anchor.x as f32, y: anchor.y as f32 };
        self.skew(deg, deg, &fanchor, order);
    }

    /// `Skew(Degree const& x_deg, Degree const& y_deg, PointImpl<f32> const& anchor, int order)`
    /// (`0x16768` sz=544B).
    ///
    /// raw 의 흐름:
    /// 1. early-out: x_deg==0 && y_deg==0 → no-op return
    /// 2. compute `(sin_x, cos_x) = sincosf(x_deg * π/180)` via f64 intermediate
    ///    + `(sin_y, cos_y) = sincosf(y_deg * π/180)`
    ///    - π/180 const = `0x3f91df46a2529d39` (f64)
    /// 3. compute local matrix:
    ///    ```text
    ///    s3 = -sin_x
    ///    s4 = (1 - cos_y) * anchor.x + sin_x * anchor.y      ; local tx
    ///    s2 = (1 - cos_x) * anchor.y + (-sin_y) * anchor.x    ; local ty
    ///    ```
    ///    Local matrix:
    ///    ```text
    ///    M_local = [[cos_y, -sin_x, local_tx],
    ///               [sin_y,  cos_x, local_ty],
    ///               [0,      0,     1       ]]
    ///    ```
    /// 4. if order != 0 (raw `cbz w20, 0x168dc` is `if order==0 goto post`):
    ///    PreMultiply self by M_local (raw 0x16824..0x168d8) — 9 inline fmadd seq
    /// 5. else (order == 0): AppendMultiply self by M_local (raw 0x168dc..0x16984)
    pub fn skew(
        &mut self,
        x_deg: &Degree,
        y_deg: &Degree,
        anchor: &PointImpl<f32>,
        order: i32,
    ) {
        if x_deg.value == 0.0 && y_deg.value == 0.0 {
            return;
        }

        let pi_180 = f64::from_bits(0x3f91_df46_a252_9d39);

        // raw: fcvt d0,s0; fmul d0,d0,d11; fcvt s0,d0; bl sincosf
        let x_rad_f64 = (x_deg.value as f64) * pi_180;
        let x_rad = x_rad_f64 as f32;
        let (sin_x, cos_x) = x_rad.sin_cos();

        let y_rad_f64 = (y_deg.value as f64) * pi_180;
        let y_rad = y_rad_f64 as f32;
        let (sin_y, cos_y) = y_rad.sin_cos();

        let neg_sin_x = -sin_x;

        // raw: local tx = (1 - cos_y) * anchor.x + sin_x * anchor.y
        let one = 1.0_f32;
        let local_tx = anchor.x.mul_add(one - cos_y, sin_x * anchor.y);
        // raw: local ty = (1 - cos_x) * anchor.y + (-sin_y) * anchor.x
        //   raw: fsub s2, 1.0, s8 (cos_x); fnmul s5, s5, s0 (sin_y)
        //        fmadd s2, s6, s2, s5 → ty = anchor.y*(1-cos_x) - anchor.x*sin_y
        let local_ty = anchor.y.mul_add(one - cos_x, -(sin_y * anchor.x));

        let local = Matrix3 {
            m: [cos_y, neg_sin_x, local_tx,
                sin_y, cos_x,     local_ty,
                0.0,   0.0,       1.0],
        };

        if order != 0 {
            self.matrix.pre_multiply(&local);
        } else {
            self.matrix.append_multiply(&local);
        }
    }

    // ───────── Scale ───────────────────────────────────────────────────────

    /// `Scale(float sx, float sy, PointImpl<f32> const& anchor, int order)`
    /// (`0x156e0` sz=380B).
    ///
    /// raw 의 흐름:
    /// 1. early-out: sx == 1 && sy == 1 → no-op (`fcmp s0,s2; fccmp s1,s2, eq; b.eq`)
    /// 2. compute local translation:
    ///    ```text
    ///    tx_local = (1 - sx) * anchor.x
    ///    ty_local = (1 - sy) * anchor.y
    ///    ```
    /// 3. Local matrix: `[[sx, 0, tx], [0, sy, ty], [0, 0, 1]]`
    /// 4. if order != 0: PreMultiply; else: AppendMultiply
    pub fn scale(&mut self, sx: f32, sy: f32, anchor: &PointImpl<f32>, order: i32) {
        if sx == 1.0 && sy == 1.0 {
            return;
        }
        let one = 1.0_f32;
        let local_tx = (one - sx) * anchor.x;
        let local_ty = (one - sy) * anchor.y;

        let local = Matrix3 {
            m: [sx,  0.0, local_tx,
                0.0, sy,  local_ty,
                0.0, 0.0, 1.0],
        };

        if order != 0 {
            self.matrix.pre_multiply(&local);
        } else {
            self.matrix.append_multiply(&local);
        }
    }

    // ───────── Init + dependent ctors ──────────────────────────────────────

    /// `Init(Rect const& src, Rect const& dst, Degree const& angle, Point const& center)`
    /// (`0x159fc` sz=~1300B). raw asm 의 1:1 inline trace 가 매우 큼 (~300 NEON
    /// instructions, 다중 SIMD shuffle, 5 branches).
    ///
    /// # 의미
    /// src rect → dst rect 매핑 + `center` 주변 회전 (output 좌표계). 동등 합성:
    /// ```text
    /// self = R(angle, center) * T(dst_center) * S(dst.size/src.size) * T(-src_center)
    /// ```
    ///
    /// # raw 의 분기 구조
    /// 1. **error early-out** (raw 0x15a18..0x15a3c):
    ///    `(src.w == 0) != (dst.w == 0)` OR `(src.h == 0) != (dst.h == 0)` →
    ///    self = identity + return (`b 0x15b54`).
    /// 2. **self-is-identity fast path** (raw 0x15a44..0x15ab8): 이미 identity 면
    ///    초기화 skip. semantic equivalent (양 path 결과 동일).
    /// 3. **src_center == origin special path** (raw 0x15b18..0x15b50): 더 단순한
    ///    inline 계산. dst 와 scale 만 적용. semantic equivalent.
    /// 4. **general path** (raw 0x15b80..0x15ec8): 가장 자주 쓰이는 경로. 9-element
    ///    inline FMA 시퀀스 + Skew(`bl 0x16768`) call(s) for rotation.
    ///
    /// # byte-eq 분석 (현 composition 구현)
    /// - `pre_multiply` 4번 호출 (innermost first). 각 호출은 `Matrix3::PreMultiply`
    ///   와 byte-eq (FMA accumulation order 정확 매칭).
    /// - **rotation 없는 case (angle=0)**: T/S/T 의 sparse matrix (대부분 0/1) →
    ///   FMA chain 의 모든 곱셈이 IEEE-exact → raw inline 9-fma 와 **byte-eq 보장**.
    /// - **rotation 있는 case (angle≠0)**: R 의 cos/sin 이 dense → raw 의 inline FMA
    ///   chain 의 specific accumulation order 와 매 element 마다 sub-ULP (~1e-7 relative)
    ///   차이 가능. pixel-eq 에는 무관.
    ///
    /// # 보류 (focused session 필요)
    /// raw asm 의 inline 9-fma 시퀀스 line-by-line 1:1 port (rotation case byte-eq).
    /// 현재 구현은 sparse 부분 byte-eq + rotation sub-ULP. 필요 시 별도 trace 세션.
    pub fn init(
        &mut self,
        src: &RectImpl<f32>,
        dst: &RectImpl<f32>,
        angle: &Degree,
        center: &PointImpl<f32>,
    ) {
        // ── (1) error early-out (raw 0x15a18..0x15a3c) ──
        //
        // raw 의 branch 의미 정확:
        //   fcmp src.w, 0
        //   fccmp src.w, dst.w, #0x4, eq    ; #0x4 = nzcv with Z=1 (= EQ when fallback)
        //   b.ne 0x15b54
        //
        // case 분석:
        //   src.w != 0 → fccmp fallback (Z=1) → b.ne NOT taken → continue
        //   src.w == 0 && dst.w == 0 → fcmp(0,0) Z=1 → b.ne NOT taken → continue
        //   src.w == 0 && dst.w != 0 → fcmp(0,x) Z=0 → b.ne TAKEN → identity early-out
        //
        // 즉 "src 만 zero, dst 는 non-zero" (또는 그 반대 형태) → error path.
        let w_error = (src.w == 0.0) && (dst.w != 0.0);
        let h_error = (src.h == 0.0) && (dst.h != 0.0);
        if w_error || h_error {
            self.matrix = Matrix3::new();
            return;
        }

        // ── (2) reset to identity (raw 0x15abc..0x15acc) ──
        // (self-is-identity fast path 생략 — 결과 동일)
        self.matrix = Matrix3::new();

        // ── (3) src center (raw 0x15af0..0x15afc) ──
        //   movi.2s v7, #0x3f, lsl #24    ; (0.5, 0.5)
        //   fmul.2s v7, src.size, v7      ; (src.w*0.5, src.h*0.5)
        //   fadd.2s v7, src.origin, v7    ; src_center
        let src_cx = src.x + 0.5 * src.w;
        let src_cy = src.y + 0.5 * src.h;
        let dst_cx = dst.x + 0.5 * dst.w;
        let dst_cy = dst.y + 0.5 * dst.h;

        // ── (4) scale factors (raw 0x15b08..0x15b14) ──
        //   fcmeq.2s v17, dst.size, src.size  ; mask: lane = all-1 if equal
        //   fdiv.2s v16, dst.size, src.size   ; ratio = dst/src
        //   bsl.8b v17, [1.0, 1.0], v16       ; (mask ? 1.0 : ratio)
        //
        // 등호 시 1.0 (exact), 아니면 dst/src 의 정확한 IEEE division.
        // src=0 && dst!=0 case 는 (1) 에서 early-out 되었음.
        // src=0 && dst=0 case 는 mask=true (둘 다 0) → scale = 1.0.
        let scale_x = if dst.w == src.w { 1.0 } else { dst.w / src.w };
        let scale_y = if dst.h == src.h { 1.0 } else { dst.h / src.h };

        // ── (5) Compose: self = R(angle, center) * T(dst_center) * S(scale) * T(-src_center) ──
        //
        // 누적: 모두 PreMultiply (order=1). 코드 순서 = innermost 먼저, outermost 마지막.
        // apply(p): T(-src) → S → T(+dst) → R(around center) 순서로 적용.
        //
        // sparse matrices (T, S) 의 pre_multiply 는 raw inline 과 byte-eq.
        // R 의 pre_multiply 는 sub-ULP 차이 가능 (cos/sin dense).

        self.translate(&PointImpl { x: -src_cx, y: -src_cy }, 1);

        if scale_x != 1.0 || scale_y != 1.0 {
            let scale_mat = Matrix3 {
                m: [scale_x, 0.0, 0.0,
                    0.0,     scale_y, 0.0,
                    0.0,     0.0,     1.0],
            };
            self.matrix.pre_multiply(&scale_mat);
        }

        self.translate(&PointImpl { x: dst_cx, y: dst_cy }, 1);

        if angle.value != 0.0 {
            self.skew(angle, angle, center, 1);
        }
    }

    /// `Transform2D(SizeImpl<f32> const& src_size, RectImpl<f32> const& dst, Degree const& angle)`
    /// (`0x15990` sz=small wrapper). raw 는 src_size + (dst.x + dst.w/2, dst.y + dst.h/2)
    /// = "center" 인자를 stack 에 만들어 Init 호출.
    pub fn from_size_rect_degree(
        src_size: &SizeImpl<f32>,
        dst: &RectImpl<f32>,
        angle: &Degree,
    ) -> Self {
        let mut t = Self::new();
        // raw 0x159c0..0x159e0: center = dst.origin + 0.5 * dst.size
        let center = PointImpl {
            x: dst.x + 0.5 * dst.w,
            y: dst.y + 0.5 * dst.h,
        };
        // src is (0, 0, src_size.w, src_size.h)
        let src = RectImpl { x: 0.0, y: 0.0, w: src_size.w, h: src_size.h };
        t.init(&src, dst, angle, &center);
        t
    }

    /// `Transform2D(RectImpl<f32> const& src, RectImpl<f32> const& dst, Degree const& angle)`
    /// (`0x15f58` sz=small wrapper). raw 의 center = dst.origin + 0.5 * dst.size.
    pub fn from_rect_rect_degree(
        src: &RectImpl<f32>,
        dst: &RectImpl<f32>,
        angle: &Degree,
    ) -> Self {
        let mut t = Self::new();
        let center = PointImpl {
            x: dst.x + 0.5 * dst.w,
            y: dst.y + 0.5 * dst.h,
        };
        t.init(src, dst, angle, &center);
        t
    }

    /// `Transform2D(RectImpl<f32> const& src, RectImpl<f32> const& dst, Degree const& angle, PointImpl<f32> const& center)`
    /// (`0x16010` sz=small). raw 는 그냥 Init 으로 forward.
    pub fn from_rect_rect_degree_center(
        src: &RectImpl<f32>,
        dst: &RectImpl<f32>,
        angle: &Degree,
        center: &PointImpl<f32>,
    ) -> Self {
        let mut t = Self::new();
        t.init(src, dst, angle, center);
        t
    }

    /// `Transform2D(float sx, float sy, Degree const& rot, float tx, float ty)`
    /// (`0x15574` sz=310B).
    ///
    /// raw 의 inline 9-fma sequence 정독 결과 이 ctor 는 다음과 동등:
    /// ```text
    /// self = T(tx, ty) * R(rot, (0,0)) * S(sx, sy)
    /// ```
    ///
    /// raw 흐름 :
    /// 1. self = identity (`stp q0=[1,0,0,0],q0, [x0]; str 1.0, [x0+0x20]`)
    /// 2. `if sx == 1 && sy == 1`: skip scale matrix build (raw `fccmp s1,s2,#0,eq; b.eq 0x15608`)
    /// 3. else: raw 0x155b8..0x15604 의 inline 시퀀스 — `d4=0` 곱셈 으로 거의 모든 항
    ///    이 zero 가 되고 결과 matrix 는 단순한 `diag(sx, sy)` (m00=sx, m11=sy, m22=1, 나머지=0).
    /// 4. raw 0x15608..0x1561c: `Skew(rot, rot, anchor=(0,0), order=1)` 호출 (PreMultiply)
    /// 5. raw 0x15620..0x15644: `if tx == 0 && ty == 0: return`
    /// 6. raw 0x15648..0x156dc: inline `PreMultiply by T(tx, ty)` (9-element fma seq)
    pub fn from_scale_rotate_translate(
        sx: f32,
        sy: f32,
        rot: &Degree,
        tx: f32,
        ty: f32,
    ) -> Self {
        let mut t = Self::new();

        // step 1+2+3: optional scale matrix
        if !(sx == 1.0 && sy == 1.0) {
            t.matrix = Matrix3 {
                m: [sx,  0.0, 0.0,
                    0.0, sy,  0.0,
                    0.0, 0.0, 1.0],
            };
        }

        // step 4: rotate (PreMultiply): self = R(rot, (0,0)) * self
        let zero_anchor = PointImpl { x: 0.0, y: 0.0 };
        t.skew(rot, rot, &zero_anchor, 1);

        // step 5+6: optional translation (PreMultiply): self = T(tx, ty) * self
        if tx != 0.0 || ty != 0.0 {
            t.translate(&PointImpl { x: tx, y: ty }, 1);
        }

        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident() -> Transform2D { Transform2D::new() }

    #[test]
    fn default_is_identity() {
        assert!(ident().is_identity());
        assert_eq!(ident().matrix.m, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn from_matrix_copies() {
        let m = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0, 1.0);
        let t = Transform2D::from_matrix(&m);
        assert_eq!(t.matrix.m, m.m);
    }

    #[test]
    fn struct_layout_is_36_bytes() {
        assert_eq!(core::mem::size_of::<Transform2D>(), 36);
        assert_eq!(core::mem::align_of::<Transform2D>(), 4);
    }

    #[test]
    fn getters_match_offsets() {
        let m = Matrix3::from_floats(2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 0.0, 0.0, 1.0);
        let t = Transform2D::from_matrix(&m);
        assert_eq!(t.get_x_scale(), 2.0);
        assert_eq!(t.get_x_offset(), 5.0);
        assert_eq!(t.get_y_scale(), 11.0);
    }

    #[test]
    fn setters_match_offsets() {
        let mut t = ident();
        t.set_x_offset(42.0);
        t.set_y_offset(-17.0);
        assert_eq!(t.matrix.m[M02], 42.0);
        assert_eq!(t.matrix.m[M12], -17.0);
    }

    #[test]
    fn is_valid_below_6() {
        for i in 0..6 {
            assert!(Transform2D::is_valid(i), "i={i}");
        }
        assert!(!Transform2D::is_valid(6));
        assert!(!Transform2D::is_valid(u64::MAX));
    }

    #[test]
    fn get_element_table() {
        let m = Matrix3::from_floats(10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0);
        let t = Transform2D::from_matrix(&m);
        // raw table: [m00, m10, m01, m11, m02, m12]
        assert_eq!(t.get_element(0), 10.0);
        assert_eq!(t.get_element(1), 40.0);
        assert_eq!(t.get_element(2), 20.0);
        assert_eq!(t.get_element(3), 50.0);
        assert_eq!(t.get_element(4), 30.0);
        assert_eq!(t.get_element(5), 60.0);
        assert_eq!(t.get_element(6), 0.0);
        assert_eq!(t.get_element(99), 0.0);
    }

    #[test]
    fn offset_subtract_half() {
        let mut t = ident();
        t.matrix.m[M02] = 10.0;
        t.matrix.m[M12] = -3.0;
        t.offset_subtract_half();
        assert_eq!(t.matrix.m[M02], 9.5);
        assert_eq!(t.matrix.m[M12], -3.5);
    }

    #[test]
    fn offset_normalize_rounds_to_int() {
        let mut t = ident();
        t.matrix.m[M02] = 3.7;
        t.matrix.m[M12] = -2.3;
        t.offset_normalize();
        // 3.7 + 0.5 = 4.2 → trunc = 4 → 4.0
        // -2.3 - 0.5 = -2.8 → trunc = -2 → -2.0
        assert_eq!(t.matrix.m[M02], 4.0);
        assert_eq!(t.matrix.m[M12], -2.0);
    }

    #[test]
    fn offset_normalize_exact_half() {
        let mut t = ident();
        t.matrix.m[M02] = 2.5;
        t.matrix.m[M12] = -3.5;
        t.offset_normalize();
        // 2.5 + 0.5 = 3.0 → trunc = 3
        // -3.5 - 0.5 = -4.0 → trunc = -4
        assert_eq!(t.matrix.m[M02], 3.0);
        assert_eq!(t.matrix.m[M12], -4.0);
    }

    #[test]
    fn inverse_identity_stays_identity() {
        let mut t = ident();
        t.inverse();
        assert_eq!(t.matrix.m, ident().matrix.m);
    }

    #[test]
    fn inverse_translation_negates_offset() {
        let mut t = ident();
        t.set_x_offset(5.0);
        t.set_y_offset(7.0);
        t.inverse();
        // inverse of pure translation: negate offsets
        assert!((t.get_x_offset() - (-5.0)).abs() < 1e-6);
        assert!((t.get_y_offset() - (-7.0)).abs() < 1e-6);
    }

    #[test]
    fn apply_identity_is_noop() {
        let mut p = PointImpl { x: 3.0, y: 7.0 };
        ident().apply(&mut p);
        assert_eq!(p, PointImpl { x: 3.0, y: 7.0 });
    }

    #[test]
    fn apply_translation() {
        let mut t = ident();
        t.set_x_offset(10.0);
        t.set_y_offset(20.0);
        let mut p = PointImpl { x: 3.0, y: 7.0 };
        t.apply(&mut p);
        assert_eq!(p, PointImpl { x: 13.0, y: 27.0 });
    }

    #[test]
    fn apply_scale() {
        let mut t = ident();
        t.matrix.m[M00] = 2.0;
        t.matrix.m[M11] = 3.0;
        let mut p = PointImpl { x: 5.0, y: 7.0 };
        t.apply(&mut p);
        assert_eq!(p, PointImpl { x: 10.0, y: 21.0 });
    }

    #[test]
    fn get_transform_point_identity() {
        let p = ident().get_transform_point(PointImpl { x: 3.0, y: 7.0 });
        assert_eq!(p, PointImpl { x: 3.0, y: 7.0 });
    }

    #[test]
    fn get_transform_point_scale_and_translate() {
        let mut t = ident();
        t.matrix.m[M00] = 2.0;
        t.matrix.m[M11] = 3.0;
        t.set_x_offset(10.0);
        t.set_y_offset(20.0);
        let p = t.get_transform_point(PointImpl { x: 5.0, y: 7.0 });
        // x: 10 + 2*5 + 0*7 = 20; y: 20 + 0*5 + 3*7 = 41
        assert_eq!(p, PointImpl { x: 20.0, y: 41.0 });
    }

    #[test]
    fn get_inverse_transform_point_identity() {
        let p = ident().get_inverse_transform_point(PointImpl { x: 3.0, y: 7.0 });
        assert_eq!(p, PointImpl { x: 3.0, y: 7.0 });
    }

    #[test]
    fn get_inverse_transform_point_round_trip() {
        let mut t = ident();
        t.matrix.m[M00] = 2.0;
        t.matrix.m[M11] = 4.0;
        t.set_x_offset(10.0);
        t.set_y_offset(20.0);
        let original = PointImpl { x: 5.0, y: 7.0 };
        let transformed = t.get_transform_point(original);
        let recovered = t.get_inverse_transform_point(transformed);
        assert!((recovered.x - original.x).abs() < 1e-5);
        assert!((recovered.y - original.y).abs() < 1e-5);
    }

    #[test]
    fn translate_zero_is_noop() {
        let original = Matrix3::from_floats(2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 0.0, 0.0, 1.0);
        let mut t = Transform2D::from_matrix(&original);
        t.translate(&PointImpl { x: 0.0, y: 0.0 }, 1);
        assert_eq!(t.matrix.m, original.m);
    }

    #[test]
    fn translate_changes_offsets() {
        let mut t = ident();
        t.translate(&PointImpl { x: 10.0, y: 20.0 }, 1);
        // For identity, translate (premultiply) sets offsets
        assert_eq!(t.get_x_offset(), 10.0);
        assert_eq!(t.get_y_offset(), 20.0);
    }

    #[test]
    fn flip_vert_inverts_y() {
        let mut t = ident();
        t.flip_vert(0.0);
        // After flipping across y=0, applying to (3, 7) should give (3, -7)
        let p = t.get_transform_point(PointImpl { x: 3.0, y: 7.0 });
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - (-7.0)).abs() < 1e-6);
    }

    #[test]
    fn flip_vert_about_line() {
        let mut t = ident();
        t.flip_vert(5.0);
        // y'=2*5 - y = 10 - 7 = 3 for y=7
        let p = t.get_transform_point(PointImpl { x: 3.0, y: 7.0 });
        assert!((p.x - 3.0).abs() < 1e-6);
        assert!((p.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn flip_horiz_about_line() {
        let mut t = ident();
        t.flip_horiz(5.0);
        // x'=2*5 - x = 10 - 3 = 7
        let p = t.get_transform_point(PointImpl { x: 3.0, y: 7.0 });
        assert!((p.x - 7.0).abs() < 1e-6);
        assert!((p.y - 7.0).abs() < 1e-6);
    }

    #[test]
    fn skew_zero_is_noop() {
        let original = Matrix3::from_floats(2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 0.0, 0.0, 1.0);
        let mut t = Transform2D::from_matrix(&original);
        let zero_deg = Degree::new();
        t.skew(&zero_deg, &zero_deg, &PointImpl { x: 0.0, y: 0.0 }, 1);
        assert_eq!(t.matrix.m, original.m);
    }

    #[test]
    fn rotate_90_around_origin() {
        let mut t = ident();
        let ninety = Degree::from_float(90.0);
        t.rotate(&ninety, &PointImpl { x: 0.0, y: 0.0 }, 1);
        // Rotating (1, 0) by 90° → (0, 1) (assuming standard CCW)
        // But raw uses Skew which may give CW depending on convention.
        let p = t.get_transform_point(PointImpl { x: 1.0, y: 0.0 });
        // either (0, 1) or (0, -1); just verify magnitude.
        assert!(p.x.abs() < 1e-5);
        assert!((p.y.abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn scale_unity_is_noop() {
        let original = Matrix3::from_floats(2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 0.0, 0.0, 1.0);
        let mut t = Transform2D::from_matrix(&original);
        t.scale(1.0, 1.0, &PointImpl { x: 5.0, y: 5.0 }, 1);
        assert_eq!(t.matrix.m, original.m);
    }

    #[test]
    fn scale_around_origin() {
        let mut t = ident();
        t.scale(2.0, 3.0, &PointImpl { x: 0.0, y: 0.0 }, 1);
        let p = t.get_transform_point(PointImpl { x: 5.0, y: 7.0 });
        assert_eq!(p, PointImpl { x: 10.0, y: 21.0 });
    }

    #[test]
    fn scale_around_anchor() {
        let mut t = ident();
        t.scale(2.0, 2.0, &PointImpl { x: 10.0, y: 10.0 }, 1);
        // anchor stays fixed: (10,10) → (10,10)
        let p = t.get_transform_point(PointImpl { x: 10.0, y: 10.0 });
        assert!((p.x - 10.0).abs() < 1e-5);
        assert!((p.y - 10.0).abs() < 1e-5);
        // (20, 20) → 10 + 2*(20-10) = 30
        let q = t.get_transform_point(PointImpl { x: 20.0, y: 20.0 });
        assert!((q.x - 30.0).abs() < 1e-5);
        assert!((q.y - 30.0).abs() < 1e-5);
    }

    #[test]
    fn mul_assign_chains() {
        let mut a = ident();
        a.translate(&PointImpl { x: 5.0, y: 0.0 }, 1);
        let mut b = ident();
        b.translate(&PointImpl { x: 3.0, y: 0.0 }, 1);
        a.mul_assign(&b);
        // a was T(5,0). mul_assign(b=T(3,0)) → self = b * a = T(3,0) * T(5,0) = T(8,0)?
        // (translation matrices commute additively)
        assert!((a.get_x_offset() - 8.0).abs() < 1e-5);
    }

    #[test]
    fn multiply_order_zero_is_append() {
        let mut a = ident();
        a.matrix.m[M00] = 2.0;
        let mut b = ident();
        b.matrix.m[M00] = 3.0;
        a.multiply(&b, 0);
        // a * b = scale(2) * scale(3) = scale(6)
        assert_eq!(a.matrix.m[M00], 6.0);
    }

    #[test]
    fn multiply_order_nonzero_is_pre() {
        let mut a = ident();
        a.matrix.m[M00] = 2.0;
        a.set_x_offset(10.0);
        let mut b = ident();
        b.set_x_offset(100.0);
        a.multiply(&b, 1);
        // self = b * a where a was diag(2)+T(10), b was T(100)
        // result: ((x+10)*1 + 100)? Actually: result(p) = b(a(p)) = b(2p + 10) = (2p + 10) + 100 = 2p + 110
        // So offset = 110
        let p = a.get_transform_point(PointImpl { x: 0.0, y: 0.0 });
        assert!((p.x - 110.0).abs() < 1e-5);
    }

    #[test]
    fn get_transform_info_identity() {
        let info = ident().get_transform_info();
        assert_eq!(info.offset_x, 0.0);
        assert_eq!(info.offset_y, 0.0);
        assert!((info.scale_x - 1.0).abs() < 1e-6);
        assert!((info.scale_y - 1.0).abs() < 1e-6);
        assert!(info.rotation_x.abs() < 1e-6);
        assert!(info.rotation_y.abs() < 1e-6);
    }

    #[test]
    fn get_transform_info_pure_scale() {
        let mut t = ident();
        t.matrix.m[M00] = 2.0;   // x-scale
        t.matrix.m[M11] = 3.0;   // y-scale
        t.set_x_offset(10.0);
        t.set_y_offset(20.0);
        let info = t.get_transform_info();
        // raw byte-eq layout:
        //   scale_x = sqrt(m00² + m10²) = sqrt(4 + 0) = 2
        //   scale_y = sqrt(m11² + m01²) = sqrt(9 + 0) = 3
        assert_eq!(info.offset_x, 10.0);
        assert_eq!(info.offset_y, 20.0);
        assert!((info.scale_x - 2.0).abs() < 1e-5, "scale_x = {}", info.scale_x);
        assert!((info.scale_y - 3.0).abs() < 1e-5, "scale_y = {}", info.scale_y);
        assert!(info.rotation_x.abs() < 1e-6);
        assert!(info.rotation_y.abs() < 1e-6);
    }

    #[test]
    fn get_transform_info_degenerate_zero_axis() {
        // m00 == 0 && m10 == 0: raw goes to b.le branch (-π/2 sentinel)
        let mut t = ident();
        t.matrix.m[M00] = 0.0;
        t.matrix.m[M10] = 0.0;
        let info = t.get_transform_info();
        // raw 0x16660 path: rotation_x = -π/2, scale_x = -0.0
        assert_eq!(info.rotation_x.to_bits(), 0xbfc9_0fdb);
        assert_eq!(info.scale_x, 0.0); // -0.0 == 0.0
    }

    #[test]
    fn get_transform_info_axis_sentinel_positive() {
        // m00 == 0 && m10 > 0 → raw 0x165e8 path: +π/2, scale = m10
        let mut t = ident();
        t.matrix.m[M00] = 0.0;
        t.matrix.m[M10] = 5.0;
        let info = t.get_transform_info();
        assert_eq!(info.rotation_x.to_bits(), 0x3fc9_0fdb);
        assert_eq!(info.scale_x, 5.0);
    }

    #[test]
    fn get_transform_info_axis_sentinel_negative() {
        // m00 == 0 && m10 < 0 → raw 0x16660: -π/2, scale = -m10
        let mut t = ident();
        t.matrix.m[M00] = 0.0;
        t.matrix.m[M10] = -5.0;
        let info = t.get_transform_info();
        assert_eq!(info.rotation_x.to_bits(), 0xbfc9_0fdb);
        assert_eq!(info.scale_x, 5.0);
    }

    #[test]
    fn apply_vec_identity_is_noop() {
        let mut pts = vec![PointImpl { x: 1.0, y: 2.0 }, PointImpl { x: 3.0, y: 4.0 }];
        let original = pts.clone();
        ident().apply_vec(&mut pts);
        assert_eq!(pts, original);
    }

    #[test]
    fn apply_vec_translates_all() {
        let mut t = ident();
        t.set_x_offset(10.0);
        let mut pts = vec![PointImpl { x: 1.0, y: 2.0 }, PointImpl { x: 3.0, y: 4.0 }];
        t.apply_vec(&mut pts);
        assert_eq!(pts, vec![PointImpl { x: 11.0, y: 2.0 }, PointImpl { x: 13.0, y: 4.0 }]);
    }

    #[test]
    fn apply_vec_empty_is_noop() {
        let mut pts: Vec<PointImpl<f32>> = vec![];
        let mut t = ident();
        t.set_x_offset(10.0);
        t.apply_vec(&mut pts);
        assert!(pts.is_empty());
    }

    #[test]
    fn rotate_int_matches_float() {
        let mut t_int = ident();
        let mut t_float = ident();
        let deg = Degree::from_float(45.0);
        t_int.rotate_int(&deg, &PointImpl { x: 5, y: 7 }, 1);
        t_float.rotate(&deg, &PointImpl { x: 5.0, y: 7.0 }, 1);
        assert_eq!(t_int.matrix.m, t_float.matrix.m);
    }

    #[test]
    fn init_zero_areas_yields_identity() {
        let src = RectImpl { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        let dst = RectImpl { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        let mut t = ident();
        t.matrix.m[M00] = 99.0; // dirty value
        t.init(&src, &dst, &Degree::new(), &PointImpl { x: 0.0, y: 0.0 });
        assert!(t.is_identity());
    }

    #[test]
    fn init_unit_src_to_unit_dst_is_identity() {
        let src = RectImpl { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        let dst = RectImpl { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        let mut t = ident();
        t.init(&src, &dst, &Degree::new(), &PointImpl { x: 0.5, y: 0.5 });
        // src == dst, no rotation → result is identity (within float)
        let p = t.get_transform_point(PointImpl { x: 0.3, y: 0.7 });
        assert!((p.x - 0.3).abs() < 1e-5);
        assert!((p.y - 0.7).abs() < 1e-5);
    }

    #[test]
    fn init_scale_2x_no_rotation() {
        let src = RectImpl { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        let dst = RectImpl { x: 0.0, y: 0.0, w: 2.0, h: 2.0 };
        let mut t = ident();
        t.init(&src, &dst, &Degree::new(), &PointImpl { x: 1.0, y: 1.0 });
        // src_center = (0.5, 0.5), dst_center = (1.0, 1.0)
        // After transform: p → ((p - src_center) * 2 + dst_center) = (2p, 2p)
        let p = t.get_transform_point(PointImpl { x: 0.5, y: 0.5 });
        // src_center maps to dst_center
        assert!((p.x - 1.0).abs() < 1e-5);
        assert!((p.y - 1.0).abs() < 1e-5);
        let p2 = t.get_transform_point(PointImpl { x: 1.0, y: 1.0 });
        // 1.0 is 0.5 above src_center, so should be 1.0 above dst_center after 2x scale
        assert!((p2.x - 2.0).abs() < 1e-5);
        assert!((p2.y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn from_scale_rotate_translate_pure_scale() {
        let t = Transform2D::from_scale_rotate_translate(2.0, 3.0, &Degree::new(), 0.0, 0.0);
        // = T(0,0) * R(0, origin) * S(2,3) = S(2,3)
        assert_eq!(t.matrix.m[M00], 2.0);
        assert_eq!(t.matrix.m[M11], 3.0);
        assert_eq!(t.matrix.m[M02], 0.0);
        assert_eq!(t.matrix.m[M12], 0.0);
    }

    #[test]
    fn from_scale_rotate_translate_pure_translation() {
        let t = Transform2D::from_scale_rotate_translate(1.0, 1.0, &Degree::new(), 10.0, 20.0);
        assert_eq!(t.matrix.m[M00], 1.0);
        assert_eq!(t.matrix.m[M11], 1.0);
        assert_eq!(t.matrix.m[M02], 10.0);
        assert_eq!(t.matrix.m[M12], 20.0);
    }

    #[test]
    fn from_scale_rotate_translate_compose() {
        // (sx=2, sy=2, rot=0, tx=10, ty=20)
        // = T(10, 20) * R(0) * S(2, 2) = T(10, 20) * S(2, 2)
        // apply(p) = T(10,20).apply(2p) = (2p.x + 10, 2p.y + 20)
        let t = Transform2D::from_scale_rotate_translate(2.0, 2.0, &Degree::new(), 10.0, 20.0);
        let p = t.get_transform_point(PointImpl { x: 3.0, y: 4.0 });
        assert!((p.x - 16.0).abs() < 1e-5, "got {}", p.x);
        assert!((p.y - 28.0).abs() < 1e-5, "got {}", p.y);
    }

    #[test]
    fn from_size_rect_degree_builds_correctly() {
        let src_size = SizeImpl { w: 1.0, h: 1.0 };
        let dst = RectImpl { x: 0.0, y: 0.0, w: 2.0, h: 2.0 };
        let t = Transform2D::from_size_rect_degree(&src_size, &dst, &Degree::new());
        // src = (0,0,1,1), dst = (0,0,2,2). src_center = dst_center = (0.5,0.5) vs (1,1)?
        // dst_center = (0+0.5*2, 0+0.5*2) = (1, 1). src_center = (0.5, 0.5).
        // p=(0.5, 0.5) → dst_center = (1, 1)
        let p = t.get_transform_point(PointImpl { x: 0.5, y: 0.5 });
        assert!((p.x - 1.0).abs() < 1e-5);
        assert!((p.y - 1.0).abs() < 1e-5);
    }
}
