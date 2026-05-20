//! `Hnc::Util::Matrix3` (`libHncFoundation.dylib`, 36B = 9 × f32 row-major).
//!
//! raw asm 위치: `work/hft_re/foundation_re/Matrix3_all.asm` (otool dump from
//! `libHncFoundation.dylib` @ `0x12c0c..0x132b7`, 22 T symbols representing 17
//! unique methods with C1/C2 ctor pairs).
//!
//! # Object layout
//! row-major 3×3 matrix, 9 contiguous `f32` (total 36B):
//!
//! | offset | field | index |
//! |--------|-------|-------|
//! | `+0x00` | `m00` (row 0 col 0) | `m[0]` |
//! | `+0x04` | `m01` (row 0 col 1) | `m[1]` |
//! | `+0x08` | `m02` (row 0 col 2) | `m[2]` |
//! | `+0x0c` | `m10` (row 1 col 0) | `m[3]` |
//! | `+0x10` | `m11` (row 1 col 1) | `m[4]` |
//! | `+0x14` | `m12` (row 1 col 2) | `m[5]` |
//! | `+0x18` | `m20` (row 2 col 0) | `m[6]` |
//! | `+0x1c` | `m21` (row 2 col 1) | `m[7]` |
//! | `+0x20` | `m22` (row 2 col 2) | `m[8]` |
//!
//! # Method list
//!
//! | symbol | raw addr | size | 의미 |
//! |--------|----------|------|------|
//! | `Matrix3()` default ctor (C1/C2 동일) | 0x12c0c, 0x12c40 | 20B | identity matrix |
//! | `Assign(f0..f8)` | 0x12c24 | 24B | 9 float 쓰기 |
//! | `Matrix3(Matrix3 const&)` copy (C1/C2 동일) | 0x12c58, 0x12c78 | 32B | zero-init + copy |
//! | `Matrix3(f0..f8)` 9-float ctor (C1/C2 동일) | 0x12c98, 0x12cb4 | 28B | = Assign |
//! | `operator=` | 0x12cd0 | 84B | self-check + 9 scalar copy |
//! | `operator==` | 0x12d24 | 160B | 9 fcmp + early-exit |
//! | `operator!=` | 0x12dc4 | 160B | 9 fcmp + early-exit (negated) |
//! | `operator/(f32) const` | 0x12e64 | 40B | sret. inv=1/x; 8-lane fmul + scalar fmul m22 |
//! | `Swap(Matrix3&)` | 0x12e8c | 144B | 9 scalar swap |
//! | `IsIdentity() const` | 0x12f20 | 128B | identity check |
//! | `Identity() const` static | 0x12fa0 | 24B | = new() |
//! | `Inverse()` | 0x12fb8 | 208B | adj/det. det==0 → identity 대체 |
//! | `Determinant() const` | 0x13088 | 60B | 3×3 cofactor expansion |
//! | `Adjoint() const` | 0x130c4 | 116B | sret. 3×3 adjoint |
//! | `PreMultiply(Matrix3 const&)` | 0x13138 | 192B | self = other * self |
//! | `AppendMultiply(Matrix3 const&)` | 0x131f8 | 192B | self = self * other |
//!
//! # default ctor / Identity 상수
//!
//! raw 는 `__TEXT,__const @ 0x000c8280` 의 16 byte 상수
//! `3f800000 00000000 00000000 00000000` (= `[1.0, 0.0, 0.0, 0.0]` as `[f32; 4]`)
//! 를 두 번 stp 한 후 `[+0x20] = 1.0` 으로 identity matrix 를 만든다:
//! `[1, 0, 0, 0, 1, 0, 0, 0, 1]`.
//!
//! # multiply accumulation order (byte-eq critical)
//!
//! raw PreMultiply / AppendMultiply 는 NEON 2-lane/4-lane FMUL+FMLA 시퀀스로
//! 9 개 result element 를 동일한 패턴으로 계산한다:
//!
//! ```text
//! result[i][j] = ((COL[1] * ROW[1])          [FMUL — separate round]
//!                 + COL[0] * ROW[0])          [FMLA — fma round]
//!                 + COL[2] * ROW[2]           [FMLA — fma round]
//! ```
//!
//! - PreMultiply (result = other * self): ROW = other.row[i], COL = self.col[j]
//! - AppendMultiply (result = self * other): ROW = self.row[i], COL = other.col[j]
//!
//! IEEE 754: 곱셈은 commutative 라 ROW/COL 위치 순서는 byte-eq 무관. 덧셈 순서
//! 는 결정적 (중간 column/row 가 먼저 fmul, 그 후 첫 fma, 그 후 마지막 fma).

/// `Hnc::Util::Matrix3` — 9 × f32 row-major 3×3 matrix.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Matrix3 {
    /// 9 contiguous f32, row-major:
    /// `[m00, m01, m02, m10, m11, m12, m20, m21, m22]`.
    pub m: [f32; 9],
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

impl Default for Matrix3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Matrix3 {
    /// `Matrix3()` default ctor (`0x12c0c` sz=20B). raw 는 `__const@0xc8280` 의
    /// 16B `[1.0, 0, 0, 0]` 두 개 + `[+0x20]=1.0` 로 identity 를 생성. 결과는
    /// `[1, 0, 0, 0, 1, 0, 0, 0, 1]`.
    pub const fn new() -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }
    }

    /// `Identity()` static (`0x12fa0` sz=24B). raw 는 default ctor 와 동일한
    /// instruction sequence (sret x0 에 동일 상수 쓰기).
    pub const fn identity() -> Self {
        Self::new()
    }

    /// `Assign(f0..f8)` (`0x12c24` sz=24B). raw 는 stp 로 9 floats 쓰기.
    /// 8 args via s0..s7, 9th arg `f8` via `[sp]`.
    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        &mut self,
        f0: f32, f1: f32, f2: f32,
        f3: f32, f4: f32, f5: f32,
        f6: f32, f7: f32, f8: f32,
    ) {
        self.m = [f0, f1, f2, f3, f4, f5, f6, f7, f8];
    }

    /// `Matrix3(f0..f8)` 9-float ctor (`0x12c98` sz=28B). raw 의 instruction
    /// sequence 는 `Assign` 과 동일.
    #[allow(clippy::too_many_arguments)]
    pub fn from_floats(
        f0: f32, f1: f32, f2: f32,
        f3: f32, f4: f32, f5: f32,
        f6: f32, f7: f32, f8: f32,
    ) -> Self {
        Self { m: [f0, f1, f2, f3, f4, f5, f6, f7, f8] }
    }

    /// `Matrix3(Matrix3 const&)` copy ctor (`0x12c58` sz=32B). raw 는 zero-init
    /// 후 9 element copy 인데 zero-init 직후 모두 overwrite 라 효과는 단순 copy.
    pub fn from_other(other: &Self) -> Self {
        Self { m: other.m }
    }

    /// `operator=(Matrix3 const&)` (`0x12cd0` sz=84B). raw 는 self-check 후
    /// 9 scalar copy. self == other 면 no-op.
    pub fn assign_from(&mut self, other: &Self) {
        if !core::ptr::eq(self, other) {
            self.m = other.m;
        }
    }

    /// `operator==(Matrix3 const&) const` (`0x12d24` sz=160B). raw 는 9 fcmp 를
    /// 순차로 수행하고 첫 mismatch 에서 false 반환. NaN 비교 시 `b.ne` 가 unordered
    /// 도 트리거하여 false 반환 → Rust `f32::PartialEq` 와 동일 동작.
    pub fn equals(&self, other: &Self) -> bool {
        for i in 0..9 {
            if self.m[i] != other.m[i] {
                return false;
            }
        }
        true
    }

    /// `operator!=(Matrix3 const&) const` (`0x12dc4` sz=160B). raw 는 `==` 의
    /// negation 인데 NaN unordered 도 true 로 반환 → Rust `!=` 와 동일.
    pub fn not_equals(&self, other: &Self) -> bool {
        for i in 0..9 {
            if self.m[i] != other.m[i] {
                return true;
            }
        }
        false
    }

    /// `operator/(float) const` (`0x12e64` sz=40B). raw 는 `s0_inv = 1.0/x`
    /// 한 번 계산 후 9 element 에 fmul. NEON 4-lane fmul (lane scalar broadcast)
    /// 로 8 element + scalar fmul 로 m22.
    ///
    /// byte-eq: scalar `inv * self.m[i]` 9 회. 4-lane SIMD 가 각 lane 독립
    /// rounding 이라 scalar 와 결과 동일.
    pub fn div(&self, denom: f32) -> Self {
        let inv = 1.0_f32 / denom;
        let mut out = [0.0f32; 9];
        for i in 0..9 {
            out[i] = self.m[i] * inv;
        }
        Self { m: out }
    }

    /// `Swap(Matrix3&)` (`0x12e8c` sz=144B). raw 는 9 element 쌍-방향 scalar swap.
    pub fn swap(&mut self, other: &mut Self) {
        for i in 0..9 {
            let tmp = self.m[i];
            self.m[i] = other.m[i];
            other.m[i] = tmp;
        }
    }

    /// `IsIdentity() const` (`0x12f20` sz=128B). raw 는 9 fcmp 로 identity 패턴
    /// (`m00=1, m01=0, m02=0, m10=0, m11=1, m12=0, m20=0, m21=0, m22=1`) 확인.
    pub fn is_identity(&self) -> bool {
        self.m[M00] == 1.0 && self.m[M01] == 0.0 && self.m[M02] == 0.0
            && self.m[M10] == 0.0 && self.m[M11] == 1.0 && self.m[M12] == 0.0
            && self.m[M20] == 0.0 && self.m[M21] == 0.0 && self.m[M22] == 1.0
    }

    /// `Determinant() const` (`0x13088` sz=60B). raw 의 3×3 cofactor expansion.
    ///
    /// raw FMA 시퀀스 (s0..s7 = local cofactor temps):
    /// ```text
    /// s5 = -m21*m12 + m11*m22                 ; cof00 [fma]
    /// s0 = -m20*m12 + m10*m22                 ; cof01 [fma]
    /// s0 = -m01*s0 + m00*s5                   ; ... [fma]
    /// s1 = -m20*m11 + m10*m21                 ; cof02 [fma]
    /// s0 = s0 + m02*s1                        ; final det [fma]
    /// ```
    pub fn determinant(&self) -> f32 {
        let cof00 = self.m[M11].mul_add(self.m[M22], -(self.m[M21] * self.m[M12]));
        let cof01 = self.m[M10].mul_add(self.m[M22], -(self.m[M20] * self.m[M12]));
        let acc = self.m[M00].mul_add(cof00, -(self.m[M01] * cof01));
        let cof02 = self.m[M10].mul_add(self.m[M21], -(self.m[M20] * self.m[M11]));
        self.m[M02].mul_add(cof02, acc)
    }

    /// `Adjoint() const` (`0x130c4` sz=116B). raw 는 sret 으로 새 Matrix3 반환.
    /// adj[r][c] = cofactor(c, r) (transposed cofactor matrix).
    ///
    /// raw FMA 시퀀스:
    /// ```text
    /// adj[0][0] = -m21*m12 + m11*m22         ; cof00
    /// adj[0][1] = -m21*m02 + m01*m22 NEG... [trace]
    /// ```
    ///
    /// raw 의 정확한 시퀀스 trace 결과:
    /// - `[+0x00]` = `-m21*m12 + m11*m22`
    /// - `[+0x04]` = `-m01*m22 + (-(m21*m02))` → `-m01*m22 - m21*m02` ... hmm
    ///
    /// 실제 정확한 trace 는 source 내 inline 구현 참조.
    pub fn adjoint(&self) -> Self {
        // raw 0x130c4..0x13134 시퀀스 1:1 trace.
        // s3=m21, s0=m22, s1=m11, s2=m12, s4=m20, s6=m10, s7=m02 (Hncm row 1 col 2 = old m12 vs new m12...
        // 헷갈리기 쉬워 raw 라인별로 직접 명시.

        // 0x130c4..130d0: load
        let s3 = self.m[M21];
        let s0 = self.m[M22];
        let s1 = self.m[M11];
        let s2 = self.m[M12];
        let s4 = self.m[M20];

        // 0x130d0..0x130d8: cof00 → [+0x00]
        let s5 = s1.mul_add(s0, -(s3 * s2)); // m11*m22 - m21*m12
        // 0x130d8..0x130e8: load m10/m12 m00/m01, compute [+0x04]
        let s6 = self.m[M10];
        let s7 = self.m[M12];
        // Actually re-check raw: `ldp s6, s7, [x0, #0x8]` loads s6=[x0+8]=m02, s7=[x0+c]=m10
        //   The raw doc above used different vars. Let me redo from scratch carefully.
        // raw is hard to follow with so many regs. Use the literal raw code in the implementation
        // below — re-derived properly.
        let _ = (s6, s7, s4, s5);
        // (See properly-derived implementation right below; comments above are wrong.)

        // ===== Properly-derived from raw 0x130c4..0x13134 ===========================
        // Load aliases (matching raw register usage):
        //   s3 = m21, s0 = m22, s1 = m11, s2 = m12, s4 = m20
        //   s6 = m02 (ldp s6,s7 [x0,#8]: s6=[+8]=m02, s7=[+c]=m10)
        //   s7 = m10
        //   s18 = m00 (ldp s18,s17 [x0]: s18=[+0]=m00, s17=[+4]=m01)
        //   s17 = m01
        //
        // raw instructions and resulting stores (x8 = sret):
        //   fnmul s5, s3, s2          → s5 = -m21*m12
        //   fmadd s5, s1, s0, s5      → s5 = m11*m22 + (-m21*m12) = m11*m22 - m21*m12
        //   ldp s6, s7, [x0, #0x8]
        //   fnmul s16, s3, s6         → s16 = -m21*m02
        //   ldp s18, s17, [x0]
        //   fnmadd s16, s17, s0, s16  → s16 = -s16 - m01*m22 = m21*m02 - m01*m22
        //                                 (NOTE: ARM fnmadd: Sd = -Sa - Sn*Sm)
        //                                 → adj[0][1] = m21*m02 - m01*m22
        //                                 (Wait — adj[0][1] should be -(m01*m22 - m02*m21) =
        //                                  m02*m21 - m01*m22. ✓ same)
        //   stp s5, s16, [x8]         → x8[+0]=s5=adj[0][0], x8[+4]=s16=adj[0][1]
        //
        //   fnmul s5, s1, s6          → s5 = -m11*m02
        //   fmadd s5, s17, s2, s5     → s5 = m01*m12 + (-m11*m02) = m01*m12 - m02*m11
        //                                 → adj[0][2] = m01*m12 - m02*m11 ✓
        //   fnmul s16, s4, s2         → s16 = -m20*m12
        //   fnmadd s16, s7, s0, s16   → s16 = -s16 - m10*m22 = m20*m12 - m10*m22
        //                                 → adj[1][0] = m12*m20 - m10*m22 ✓ (= -cof01_signed)
        //   stp s5, s16, [x8, #0x8]   → adj[0][2], adj[1][0]
        //
        //   fnmul s5, s4, s6          → s5 = -m20*m02
        //   fmadd s0, s18, s0, s5     → s0 = m00*m22 + (-m20*m02) = m00*m22 - m02*m20
        //                                 → adj[1][1] = m00*m22 - m02*m20 ✓
        //   fnmul s5, s7, s6          → s5 = -m10*m02
        //   fnmadd s2, s18, s2, s5    → s2 = -s5 - m00*m12 = m10*m02 - m00*m12
        //                                 → adj[1][2] = m02*m10 - m00*m12 ✓
        //   stp s0, s2, [x8, #0x10]   → adj[1][1], adj[1][2]
        //
        //   fnmul s0, s4, s1          → s0 = -m20*m11
        //   fmadd s0, s7, s3, s0      → s0 = m10*m21 + (-m20*m11) = m10*m21 - m20*m11
        //                                 → adj[2][0] = m10*m21 - m11*m20 ✓
        //   fnmul s2, s4, s17         → s2 = -m20*m01
        //   fnmadd s2, s18, s3, s2    → s2 = -s2 - m00*m21 = m20*m01 - m00*m21
        //                                 → adj[2][1] = m01*m20 - m00*m21 ✓
        //   stp s0, s2, [x8, #0x18]   → adj[2][0], adj[2][1]
        //
        //   fnmul s0, s7, s17         → s0 = -m10*m01
        //   fmadd s0, s18, s1, s0     → s0 = m00*m11 + (-m10*m01) = m00*m11 - m01*m10
        //                                 → adj[2][2] = m00*m11 - m01*m10 ✓
        //   str s0, [x8, #0x20]       → adj[2][2]
        //
        // 따라서 byte-eq 시퀀스:

        let m00 = self.m[M00];
        let m01 = self.m[M01];
        let m02 = self.m[M02];
        let m10 = self.m[M10];
        let m11 = self.m[M11];
        let m12 = self.m[M12];
        let m20 = self.m[M20];
        let m21 = self.m[M21];
        let m22 = self.m[M22];

        // adj[0][0] = m11*m22 - m21*m12   (fma round)
        let a00 = m11.mul_add(m22, -(m21 * m12));
        // adj[0][1] = m21*m02 - m01*m22  via fnmadd: -((-m21*m02)) - m01*m22 = m21*m02 - m01*m22
        //   raw: s16 = -m21*m02 [fmul], then s16 = -s16 - m01*m22 [fnmadd round]
        //   = -(-(m21*m02)) - m01*m22 [round] = m21*m02 - m01*m22
        //   byte-eq Rust: fma(m01, -m22, m21*m02) = m01*(-m22) + m21*m02 [fma round]
        //                = m21*m02 - m01*m22 (same single round)
        let a01 = m01.mul_add(-m22, m21 * m02);
        // adj[0][2] = m01*m12 - m02*m11  via fnmul + fmadd
        let a02 = m01.mul_add(m12, -(m11 * m02));
        // adj[1][0] = m12*m20 - m10*m22  via fnmul + fnmadd
        //   raw: s16 = -m20*m12, then s16 = -s16 - m10*m22 = m20*m12 - m10*m22
        //   byte-eq: fma(m10, -m22, m20*m12) = -m10*m22 + m20*m12 [single round]
        let a10 = m10.mul_add(-m22, m20 * m12);
        // adj[1][1] = m00*m22 - m02*m20
        let a11 = m00.mul_add(m22, -(m20 * m02));
        // adj[1][2] = m02*m10 - m00*m12  via fnmul + fnmadd
        let a12 = m00.mul_add(-m12, m10 * m02);
        // adj[2][0] = m10*m21 - m20*m11
        let a20 = m10.mul_add(m21, -(m20 * m11));
        // adj[2][1] = m01*m20 - m00*m21  via fnmul + fnmadd
        let a21 = m00.mul_add(-m21, m20 * m01);
        // adj[2][2] = m00*m11 - m01*m10
        let a22 = m00.mul_add(m11, -(m10 * m01));

        Self { m: [a00, a01, a02, a10, a11, a12, a20, a21, a22] }
    }

    /// `Inverse()` (`0x12fb8` sz=208B). raw 는 in-place: det 계산 → det==0 이면
    /// identity 로 덮어쓰고 return, 아니면 `inv = adjoint * (1/det)` 를 in-place 저장.
    ///
    /// raw 의 inverse 계산 시퀀스:
    /// - cof00 = m11*m22 - m21*m12  (fma)
    /// - cof01_alt = m10*m22 - m20*m12  (fma)
    /// - acc = m00*cof00 - m01*cof01_alt  (fma)
    /// - cof02 = m10*m21 - m20*m11  (fma)
    /// - det = acc + m02*cof02  (fma)
    /// - if det == 0: identity 로 덮어쓰고 return
    /// - else: adjoint 의 9 element 를 `inv = 1.0/det` 로 fmul 후 저장
    pub fn inverse(&mut self) {
        // det 계산 (raw 0x12fb8..0x12ff0 - Determinant 와 동일 시퀀스)
        let cof00 = self.m[M11].mul_add(self.m[M22], -(self.m[M21] * self.m[M12]));
        let cof01_alt = self.m[M10].mul_add(self.m[M22], -(self.m[M20] * self.m[M12]));
        let acc = self.m[M00].mul_add(cof00, -(self.m[M01] * cof01_alt));
        let cof02 = self.m[M10].mul_add(self.m[M21], -(self.m[M20] * self.m[M11]));
        let det = self.m[M02].mul_add(cof02, acc);

        if det == 0.0 {
            // raw 0x12ff8..0x13010: identity 로 덮어쓰기
            *self = Self::new();
            return;
        }

        // raw 0x13014..0x13084: adj * (1/det), in-place.
        //
        // raw 의 instruction 시퀀스를 1:1 trace:
        //   s7=m00, s16=m01, s18=m02, s17=m10, s2=m11, s6=m12, s19=m20, s3=m21, s5=m22
        //
        // computed cofactors (with sign):
        //   s0 = m11*m22 - m21*m12  (= cof00, adj[0][0])
        //   s20 = m10*m22 - m20*m12 (= -adj[1][0] before fneg, i.e. (m10*m22 - m20*m12))
        //
        // raw 시퀀스 (with FNEG to flip s20):
        //   fneg s21, s3            ; s21 = -m21
        //   fneg s19, s19           ; s19 = -m20 (in-place fneg)
        //   fmul s21, s18, s21      ; s21 = m02 * (-m21) = -m02*m21
        //   fnmadd s21, s16, s5, s21 ; s21 = -s21 - m01*m22 = m02*m21 - m01*m22 = adj[0][1]
        //   fnmul s22, s2, s18      ; s22 = -m11*m02
        //   fmadd s22, s16, s6, s22 ; s22 = m01*m12 + (-m11*m02) = m01*m12 - m02*m11 = adj[0][2]
        //   fneg s20, s20           ; s20 = -(m10*m22 - m20*m12) = m12*m20 - m10*m22 = adj[1][0]
        //   fmul s23, s18, s19      ; s23 = m02 * (-m20) = -m02*m20
        //   fmadd s5, s7, s5, s23   ; s5 = m00*m22 + (-m02*m20) = m00*m22 - m02*m20 = adj[1][1]
        //   fnmul s18, s17, s18     ; s18 = -m10*m02
        //   fnmadd s6, s7, s6, s18  ; s6 = -s18 - m00*m12 = m10*m02 - m00*m12 = adj[1][2]
        //   fmul s18, s16, s19      ; s18 = m01 * (-m20) = -m01*m20
        //   fnmadd s3, s7, s3, s18  ; s3 = -s18 - m00*m21 = m01*m20 - m00*m21 = adj[2][1]
        //   fnmul s16, s17, s16     ; s16 = -m10*m01
        //   fmadd s7, s7, s2, s16   ; s7 = m00*m11 + (-m10*m01) = m00*m11 - m01*m10 = adj[2][2]
        //
        // 그리고 v0 lanes = (s0=adj[0][0], s21=adj[0][1], s22=adj[0][2], s20=adj[1][0])
        //       v5 lanes = (s5=adj[1][1], s6=adj[1][2], s1_cof02=adj[2][0], s3=adj[2][1])
        //   *fdiv s4, 1.0, det        ; det_inv = 1/det
        //   *fmul.4s v0, v0, v4[0]    ; lane-wise mul by det_inv
        //   *fmul.4s v2, v5, v4[0]    ; lane-wise mul by det_inv
        //   *fmul s1, s4, s7          ; s1 = det_inv * adj[2][2]
        //   *stp q0, q2, [x0] ; str s1, [x0, #0x20]

        let m00 = self.m[M00];
        let m01 = self.m[M01];
        let m02 = self.m[M02];
        let m10 = self.m[M10];
        let m11 = self.m[M11];
        let m12 = self.m[M12];
        let m20 = self.m[M20];
        let m21 = self.m[M21];
        let m22 = self.m[M22];

        // cof01_alt 는 m10*m22 - m20*m12; adj[1][0] = -(cof01_alt) → m12*m20 - m10*m22
        // 위에서 cof01_alt 를 이미 fma 로 계산했음 (값 동일하나 raw 의 별도 fneg 만큼 한 round 더?
        // 아니다 — fneg 는 IEEE bit flip, rounding 없음. 따라서 -cof01_alt 가 byte-eq).
        let a00 = cof00;                                       // already computed
        // adj[0][1] = fma(m01, -m22, m02*m21)
        let a01 = m01.mul_add(-m22, m02 * m21);
        // adj[0][2] = fma(m01, m12, -(m11*m02))
        let a02 = m01.mul_add(m12, -(m11 * m02));
        let a10 = -cof01_alt;                                  // = m12*m20 - m10*m22, byte-eq via fneg
        // adj[1][1] = fma(m00, m22, -(m02*m20))
        let a11 = m00.mul_add(m22, -(m02 * m20));
        // adj[1][2] = fma(m00, -m12, m10*m02)
        let a12 = m00.mul_add(-m12, m10 * m02);
        let a20 = cof02;                                       // adj[2][0] = m10*m21 - m20*m11
        // adj[2][1] = fma(m00, -m21, m20*m01)
        let a21 = m00.mul_add(-m21, m20 * m01);
        // adj[2][2] = fma(m00, m11, -(m10*m01))
        let a22 = m00.mul_add(m11, -(m10 * m01));

        let inv = 1.0_f32 / det;
        self.m = [
            a00 * inv, a01 * inv, a02 * inv,
            a10 * inv, a11 * inv, a12 * inv,
            a20 * inv, a21 * inv, a22 * inv,
        ];
    }

    /// `PreMultiply(Matrix3 const&)` (`0x13138` sz=192B). self = other * self.
    ///
    /// raw 의 9 element 모두 동일 패턴:
    /// ```text
    /// result[i][j] = ((self.m[1][j] * other.m[i][1]) [FMUL: separate round]
    ///                 + self.m[0][j] * other.m[i][0]) [FMLA: fma round]
    ///                 + self.m[2][j] * other.m[i][2]  [FMLA: fma round]
    /// ```
    pub fn pre_multiply(&mut self, other: &Self) {
        // self.col[j] for j=0,1,2:
        //   col0 = (self.m00, self.m10, self.m20)
        //   col1 = (self.m01, self.m11, self.m21)
        //   col2 = (self.m02, self.m12, self.m22)
        // other.row[i] for i=0,1,2:
        //   row0 = (other.m00, other.m01, other.m02)
        //   row1 = (other.m10, other.m11, other.m12)
        //   row2 = (other.m20, other.m21, other.m22)
        // result[i][j] = ((col[j][1] * row[i][1]) + col[j][0] * row[i][0]) + col[j][2] * row[i][2]
        let s = self.m;
        let o = other.m;

        // row 0 (i=0): row = (o[M00], o[M01], o[M02])
        let r00 = (s[M10] * o[M01]) // col0[1]*row0[1]
            .add_fma(s[M00], o[M00])  // + col0[0]*row0[0] (fma)
            .add_fma(s[M20], o[M02]); // + col0[2]*row0[2] (fma)
        let r01 = (s[M11] * o[M01])
            .add_fma(s[M01], o[M00])
            .add_fma(s[M21], o[M02]);
        let r02 = (s[M12] * o[M01])
            .add_fma(s[M02], o[M00])
            .add_fma(s[M22], o[M02]);

        // row 1 (i=1): row = (o[M10], o[M11], o[M12])
        let r10 = (s[M10] * o[M11])
            .add_fma(s[M00], o[M10])
            .add_fma(s[M20], o[M12]);
        let r11 = (s[M11] * o[M11])
            .add_fma(s[M01], o[M10])
            .add_fma(s[M21], o[M12]);
        let r12 = (s[M12] * o[M11])
            .add_fma(s[M02], o[M10])
            .add_fma(s[M22], o[M12]);

        // row 2 (i=2): row = (o[M20], o[M21], o[M22])
        let r20 = (s[M10] * o[M21])
            .add_fma(s[M00], o[M20])
            .add_fma(s[M20], o[M22]);
        let r21 = (s[M11] * o[M21])
            .add_fma(s[M01], o[M20])
            .add_fma(s[M21], o[M22]);
        let r22 = (s[M12] * o[M21])
            .add_fma(s[M02], o[M20])
            .add_fma(s[M22], o[M22]);

        self.m = [r00, r01, r02, r10, r11, r12, r20, r21, r22];
    }

    /// `AppendMultiply(Matrix3 const&)` (`0x131f8` sz=192B). self = self * other.
    ///
    /// raw 는 PreMultiply 와 동일한 NEON 시퀀스인데 x0/x1 의 역할이 swap 되어
    /// "row source" = self, "col source" = other 가 된다. accumulation order 는 동일.
    pub fn append_multiply(&mut self, other: &Self) {
        // self.row[i] / other.col[j] 가 source.
        // result[i][j] = ((other.col[j][1] * self.row[i][1])
        //                  + other.col[j][0] * self.row[i][0])
        //                  + other.col[j][2] * self.row[i][2]
        let s = self.m;
        let o = other.m;

        // row 0 (i=0): self.row0 = (s[M00], s[M01], s[M02])
        let r00 = (o[M10] * s[M01])
            .add_fma(o[M00], s[M00])
            .add_fma(o[M20], s[M02]);
        let r01 = (o[M11] * s[M01])
            .add_fma(o[M01], s[M00])
            .add_fma(o[M21], s[M02]);
        let r02 = (o[M12] * s[M01])
            .add_fma(o[M02], s[M00])
            .add_fma(o[M22], s[M02]);

        // row 1 (i=1): self.row1 = (s[M10], s[M11], s[M12])
        let r10 = (o[M10] * s[M11])
            .add_fma(o[M00], s[M10])
            .add_fma(o[M20], s[M12]);
        let r11 = (o[M11] * s[M11])
            .add_fma(o[M01], s[M10])
            .add_fma(o[M21], s[M12]);
        let r12 = (o[M12] * s[M11])
            .add_fma(o[M02], s[M10])
            .add_fma(o[M22], s[M12]);

        // row 2 (i=2): self.row2 = (s[M20], s[M21], s[M22])
        let r20 = (o[M10] * s[M21])
            .add_fma(o[M00], s[M20])
            .add_fma(o[M20], s[M22]);
        let r21 = (o[M11] * s[M21])
            .add_fma(o[M01], s[M20])
            .add_fma(o[M21], s[M22]);
        let r22 = (o[M12] * s[M21])
            .add_fma(o[M02], s[M20])
            .add_fma(o[M22], s[M22]);

        self.m = [r00, r01, r02, r10, r11, r12, r20, r21, r22];
    }
}

/// helper trait for fluent fma chaining: `(a * b).add_fma(c, d) = fma(c, d, a*b)`
trait FmaChain {
    fn add_fma(self, a: f32, b: f32) -> f32;
}

impl FmaChain for f32 {
    #[inline(always)]
    fn add_fma(self, a: f32, b: f32) -> f32 {
        a.mul_add(b, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident() -> Matrix3 { Matrix3::identity() }

    #[test]
    fn default_is_identity() {
        let m = Matrix3::new();
        assert_eq!(m.m, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(m.is_identity());
    }

    #[test]
    fn identity_factory_same_as_new() {
        assert_eq!(Matrix3::identity().m, Matrix3::new().m);
    }

    #[test]
    fn assign_writes_all_nine() {
        let mut m = Matrix3::new();
        m.assign(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        assert_eq!(m.m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn from_floats_matches_assign() {
        let m = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        assert_eq!(m.m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn from_other_copies_all() {
        let a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = Matrix3::from_other(&a);
        assert_eq!(a.m, b.m);
    }

    #[test]
    fn assign_from_self_is_noop() {
        let mut a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let raw_ptr_check = a.m;
        // self-assign through &mut
        let ptr: *mut Matrix3 = &mut a;
        unsafe {
            (*ptr).assign_from(&*ptr);
        }
        assert_eq!(a.m, raw_ptr_check);
    }

    #[test]
    fn assign_from_other_copies() {
        let mut a = ident();
        let b = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        a.assign_from(&b);
        assert_eq!(a.m, b.m);
    }

    #[test]
    fn equals_matches_element() {
        let a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = a;
        let mut c = a;
        c.m[5] = 99.0;
        assert!(a.equals(&b));
        assert!(!a.equals(&c));
        assert!(!a.not_equals(&b));
        assert!(a.not_equals(&c));
    }

    #[test]
    fn equals_nan_returns_false() {
        let mut a = ident();
        a.m[0] = f32::NAN;
        let b = ident();
        // raw: any fcmp with NaN triggers b.ne (unordered) → returns 0
        // Rust f32::eq: returns false for NaN
        assert!(!a.equals(&b));
        assert!(a.not_equals(&b));
    }

    #[test]
    fn div_scales_all_nine() {
        let m = Matrix3::from_floats(2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0);
        let d = m.div(2.0);
        // raw: inv = 1/2 = 0.5 (exact); then m[i]*inv = m[i]/2 (exact for these values)
        assert_eq!(d.m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn div_uses_reciprocal_round() {
        // 3.0 is not exactly representable as 1/3, so div(3) ≠ exact division.
        // We just verify that the result matches `m[i] * (1/3)` byte-for-byte.
        let m = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let d = m.div(3.0);
        let inv = 1.0_f32 / 3.0;
        for i in 0..9 {
            assert_eq!(d.m[i].to_bits(), (m.m[i] * inv).to_bits(),
                "i={i}: div uses 1/x reciprocal then mul, not direct division");
        }
    }

    #[test]
    fn swap_exchanges_all() {
        let mut a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let mut b = Matrix3::from_floats(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
        a.swap(&mut b);
        assert_eq!(a.m, [9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
        assert_eq!(b.m, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    }

    #[test]
    fn is_identity_for_identity() {
        assert!(ident().is_identity());
    }

    #[test]
    fn is_identity_false_for_non_identity() {
        let mut m = ident();
        m.m[5] = 0.001;
        assert!(!m.is_identity());
    }

    #[test]
    fn determinant_identity_is_one() {
        assert_eq!(ident().determinant(), 1.0);
    }

    #[test]
    fn determinant_known_matrix() {
        // [[1,2,3],[0,4,5],[1,0,6]]
        // det = 1*(4*6 - 5*0) - 2*(0*6 - 5*1) + 3*(0*0 - 4*1)
        //     = 24 - 2*(-5) + 3*(-4) = 24 + 10 - 12 = 22
        let m = Matrix3::from_floats(1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 1.0, 0.0, 6.0);
        assert_eq!(m.determinant(), 22.0);
    }

    #[test]
    fn adjoint_identity_is_identity() {
        let adj = ident().adjoint();
        assert_eq!(adj.m, ident().m);
    }

    #[test]
    fn adjoint_times_original_is_det_times_identity() {
        let m = Matrix3::from_floats(1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 1.0, 0.0, 6.0);
        let adj = m.adjoint();
        // m * adj = det(m) * I = 22 * I
        // Use a fresh matrix to multiply.
        let mut prod = m;
        prod.append_multiply(&adj);
        let expected = 22.0_f32;
        for i in 0..9 {
            let want = if i == 0 || i == 4 || i == 8 { expected } else { 0.0 };
            assert!((prod.m[i] - want).abs() < 1e-4, "i={i}: {} vs {}", prod.m[i], want);
        }
    }

    #[test]
    fn inverse_identity_is_identity() {
        let mut m = ident();
        m.inverse();
        assert_eq!(m.m, ident().m);
    }

    #[test]
    fn inverse_known_matrix_round_trip() {
        let original = Matrix3::from_floats(1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 1.0, 0.0, 6.0);
        let mut inv = original;
        inv.inverse();
        // inv * original should be identity (within float tolerance)
        let mut prod = inv;
        prod.append_multiply(&original);
        for i in 0..9 {
            let want = if i == 0 || i == 4 || i == 8 { 1.0 } else { 0.0 };
            assert!((prod.m[i] - want).abs() < 1e-5, "i={i}: {} vs {}", prod.m[i], want);
        }
    }

    #[test]
    fn inverse_singular_falls_back_to_identity() {
        // [[1,2,3],[2,4,6],[1,1,1]] — row1 = 2*row0, det=0
        let mut m = Matrix3::from_floats(1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 1.0, 1.0);
        m.inverse();
        assert_eq!(m.m, ident().m, "singular matrix → identity");
    }

    #[test]
    fn append_multiply_by_identity_is_noop() {
        let original = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let mut m = original;
        m.append_multiply(&ident());
        // self * I = self.  But due to FMA accumulation order with zeros in I,
        // result should be exactly byte-eq to self.
        // For each result element r[i][j] = ((I.row1[j] * self.row[i][1]) [fmul]
        //                                     + I.row0[j] * self.row[i][0]) [fma]
        //                                     + I.row2[j] * self.row[i][2] [fma]
        // For j=0: I.col0 = (1, 0, 0). Sequence: (0 * s[i][1]) + 1*s[i][0] + 0*s[i][2]
        //   = (0 + s[i][0]) + 0 = s[i][0]  ✓ byte-eq
        // For j=1: I.col1 = (0, 1, 0). Sequence: (1*s[i][1]) [fmul] + 0*s[i][0] [fma] + 0*s[i][2] [fma]
        //   = s[i][1] + 0 + 0 = s[i][1]  ✓
        // For j=2: I.col2 = (0, 0, 1). Sequence: (0*s[i][1]) + 0*s[i][0] + 1*s[i][2]
        //   = 0 + 0 + s[i][2] = s[i][2]  ✓
        for i in 0..9 {
            assert_eq!(m.m[i], original.m[i], "i={i}");
        }
    }

    #[test]
    fn pre_multiply_by_identity_is_noop() {
        let original = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let mut m = original;
        m.pre_multiply(&ident());
        for i in 0..9 {
            assert_eq!(m.m[i], original.m[i], "i={i}");
        }
    }

    #[test]
    fn append_multiply_matches_textbook() {
        // Use small integers to avoid rounding.
        let a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = Matrix3::from_floats(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
        let mut prod = a;
        prod.append_multiply(&b);
        // a * b:
        // r00 = 1*9 + 2*6 + 3*3 = 9+12+9 = 30
        // r01 = 1*8 + 2*5 + 3*2 = 8+10+6 = 24
        // r02 = 1*7 + 2*4 + 3*1 = 7+8+3 = 18
        // r10 = 4*9 + 5*6 + 6*3 = 36+30+18 = 84
        // r11 = 4*8 + 5*5 + 6*2 = 32+25+12 = 69
        // r12 = 4*7 + 5*4 + 6*1 = 28+20+6 = 54
        // r20 = 7*9 + 8*6 + 9*3 = 63+48+27 = 138
        // r21 = 7*8 + 8*5 + 9*2 = 56+40+18 = 114
        // r22 = 7*7 + 8*4 + 9*1 = 49+32+9 = 90
        assert_eq!(prod.m, [30.0, 24.0, 18.0, 84.0, 69.0, 54.0, 138.0, 114.0, 90.0]);
    }

    #[test]
    fn pre_multiply_matches_textbook() {
        let a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = Matrix3::from_floats(9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0);
        let mut prod = a;
        prod.pre_multiply(&b);
        // pre_multiply: self = b * a (other * self)
        // b * a:
        // r00 = 9*1 + 8*4 + 7*7 = 9+32+49 = 90
        // r01 = 9*2 + 8*5 + 7*8 = 18+40+56 = 114
        // r02 = 9*3 + 8*6 + 7*9 = 27+48+63 = 138
        // r10 = 6*1 + 5*4 + 4*7 = 6+20+28 = 54
        // r11 = 6*2 + 5*5 + 4*8 = 12+25+32 = 69
        // r12 = 6*3 + 5*6 + 4*9 = 18+30+36 = 84
        // r20 = 3*1 + 2*4 + 1*7 = 3+8+7 = 18
        // r21 = 3*2 + 2*5 + 1*8 = 6+10+8 = 24
        // r22 = 3*3 + 2*6 + 1*9 = 9+12+9 = 30
        assert_eq!(prod.m, [90.0, 114.0, 138.0, 54.0, 69.0, 84.0, 18.0, 24.0, 30.0]);
    }

    #[test]
    fn append_multiply_non_commutative_with_pre() {
        // verify pre * append duality:
        //   a.pre_multiply(&b) ⇔ result = b * a
        //   a.append_multiply(&b) ⇔ result = a * b
        let a = Matrix3::from_floats(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
        let b = Matrix3::from_floats(2.0, 0.0, 1.0, 0.0, 3.0, 0.0, 1.0, 0.0, 2.0);
        let mut pre = a;
        pre.pre_multiply(&b);  // = b * a
        let mut app = b;
        app.append_multiply(&a);  // = b * a
        assert_eq!(pre.m, app.m);
    }

    #[test]
    fn struct_layout_is_36_bytes() {
        assert_eq!(core::mem::size_of::<Matrix3>(), 36);
        assert_eq!(core::mem::align_of::<Matrix3>(), 4);
    }
}
