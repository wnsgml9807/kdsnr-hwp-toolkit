//! `Hnc::Shape::Transformation` (28B) + `Hnc::Type::RectImpl<float>` (20B variant).
//!
//! ## 출처 (CalcDrawVariables `0x2f4368` byte-eq RE)
//!
//! `CalcDrawVariables` 의 output 4종 (PointF, RectImpl<float>, Transformation, int& mode) 중
//! RectF / Transformation 의 actual byte layout 을 raw asm trace + caller 의 stack frame
//! 측정 (caller `0x2f3998..0x2f39c8`) 으로 확정:
//!
//! - **arg5 RectF&** = `sp+0x8c..0xa0` = **20 byte** (Hnc 의 RectImpl<float> 변종)
//! - **arg6 Transformation&** = `sp+0x70..0x8c` = **28 byte**
//! - **arg7 StringFormat&** = `sp+0x68..0x70` = **8 byte** (impl_ptr only)
//!
//! `RectImpl<float>` 가 16B (`{x,y,w,h}`) 가 아니라 **20B** 임 — first 4 byte header
//! (`flag + u16 + byte`) + 4 f32 (matrix-like values). 한컴 의 Rect 가 단순 좌표가
//! 아니라 panose / transform metadata 를 함께 가지는 specialized 형태.
//!
//! ## byte writes (CalcDrawVariables 마지막 단계)
//!
//! ```text
//! ; output RectF (x21, 20B):
//! 0x2f48a8  strb  w9, [x21]            ; +0: byte (= 1)
//! 0x2f48b0  sturh w9, [x21, #0x1]      ; +1..+2: u16 unaligned (panose lower)
//! 0x2f48b8  strb  w9, [x21, #0x3]      ; +3: byte (panose upper)
//! 0x2f48bc  stp   s12, s14, [x21, #0x4]; +4..+0xb: 2 f32
//! 0x2f48c0  stp   s11, s13, [x21, #0xc]; +0xc..+0x13: 2 f32
//!
//! ; output Transformation (x9 = arg6 saved on sp+0x8, 28B):
//! 0x2f48cc  str   q0, [x9]             ; +0..+0xf: 16 byte from sp+0x20
//! 0x2f48d4  stur  q0, [x9, #0xc]       ; +0xc..+0x1b: 16 byte from sp+0x2c (overlap)
//! ```

use std::mem::{align_of, size_of};

/// `Hnc::Shape::Transformation` — 28 byte byte-eq layout.
///
/// 의미 (CalcDrawVariables 의 fill 패턴 기반 추론):
/// - `flag0` = 0 — invalidation / initial state
/// - `flag1` = 1 — "valid format" 표식
/// - `panose` (3 byte) = font panose metadata (또는 0 if `has_explicit_format` false)
/// - `m0..m3` (4 f32) = 2×2 transform matrix elements (혹은 scale/rotation/위치 조합)
/// - `degree_raw` = `Hnc::Util::Degree` 의 raw 4-byte representation (보통 90° = `0x42b40000` 또는 0)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transformation {
    /// +0..+3: `flag0` 4-byte slot (header byte at +0, rest pad).
    pub header0: [u8; 4],
    /// +4..+7: `flag1` byte (+4) + 3 panose byte (+5..+7).
    pub header1: [u8; 4],
    /// +8: f32 m0 (CalcDrawVariables 마지막 s12).
    pub m0: f32,
    /// +0xc: f32 m1 (CalcDrawVariables 마지막 s14).
    pub m1: f32,
    /// +0x10: f32 m2 (CalcDrawVariables 마지막 s11).
    pub m2: f32,
    /// +0x14: f32 m3 (CalcDrawVariables 마지막 s13).
    pub m3: f32,
    /// +0x18: u32 Degree raw value (4-byte fixed-point IEEE 754).
    pub degree_raw: u32,
}

pub const TRANSFORMATION_SIZE_BYTES: usize = 28;
pub const TRANSFORMATION_ALIGN_BYTES: usize = 4;

const _: () = assert!(size_of::<Transformation>() == TRANSFORMATION_SIZE_BYTES);
const _: () = assert!(align_of::<Transformation>() == TRANSFORMATION_ALIGN_BYTES);

impl Transformation {
    /// Default-init (모든 0).
    pub const ZERO: Self = Self {
        header0: [0; 4],
        header1: [0; 4],
        m0: 0.0,
        m1: 0.0,
        m2: 0.0,
        m3: 0.0,
        degree_raw: 0,
    };

    /// +0: `flag0` byte slot.
    #[inline]
    pub fn flag0(&self) -> u8 {
        self.header0[0]
    }

    /// +4: `flag1` byte slot.
    #[inline]
    pub fn flag1(&self) -> u8 {
        self.header1[0]
    }

    /// +5..+7: 3-byte panose.
    #[inline]
    pub fn panose(&self) -> [u8; 3] {
        [self.header1[1], self.header1[2], self.header1[3]]
    }

    /// CalcDrawVariables 의 stack-buffer copy 패턴 byte-eq 시뮬레이션:
    /// `sp+0x20..0x3c` 28B contiguous block 의 contents 그대로.
    pub fn write_raw(
        &mut self,
        flag0: u8,
        flag1: u8,
        panose: [u8; 3],
        m0: f32,
        m1: f32,
        m2: f32,
        m3: f32,
        degree_raw: u32,
    ) {
        self.header0 = [flag0, 0, 0, 0];
        self.header1 = [flag1, panose[0], panose[1], panose[2]];
        self.m0 = m0;
        self.m1 = m1;
        self.m2 = m2;
        self.m3 = m3;
        self.degree_raw = degree_raw;
    }
}

/// `Hnc::Type::RectImpl<float>` (20B variant — CalcDrawVariables 의 arg5 specialization).
///
/// **주의**: 동명 type `surface::RectImpl<T>` (16B `{x,y,w,h}`) 와 다름.
/// 한컴 demangler 가 동일한 mangled name 으로 보여주지만 실제 byte 는 20B
/// 이며 panose + 4 matrix element 를 보유. Surface API 의 RectImpl 와 별개로 정의.
///
/// caller stack-frame 검증 (raw `0x2f39b0`: `add x5, sp, #0x8c`, next arg `0x2f39ac add x4, sp, #0xa0`)
/// → size = 0xa0 - 0x8c = 0x14 = 20 byte ✓.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RectF20 {
    /// +0..+3: header (byte flag + u16 panose_lo unaligned + byte panose_hi).
    pub header: [u8; 4],
    /// +4: f32 m0 (raw `stp s12,s14 [x21, #4]` first lane).
    pub m0: f32,
    /// +8: f32 m1 (raw `stp s12,s14 [x21, #4]` second lane).
    pub m1: f32,
    /// +0xc: f32 m2 (raw `stp s11,s13 [x21, #0xc]` first lane).
    pub m2: f32,
    /// +0x10: f32 m3 (raw `stp s11,s13 [x21, #0xc]` second lane).
    pub m3: f32,
}

pub const RECTF20_SIZE_BYTES: usize = 20;
pub const RECTF20_ALIGN_BYTES: usize = 4;

const _: () = assert!(size_of::<RectF20>() == RECTF20_SIZE_BYTES);
const _: () = assert!(align_of::<RectF20>() == RECTF20_ALIGN_BYTES);

impl RectF20 {
    pub const ZERO: Self = Self {
        header: [0; 4],
        m0: 0.0,
        m1: 0.0,
        m2: 0.0,
        m3: 0.0,
    };

    /// CalcDrawVariables 의 RectF write 패턴 byte-eq: flag + u16 + byte + 4 f32.
    pub fn write_raw(
        &mut self,
        flag: u8,
        panose_lo: u16,
        panose_hi: u8,
        m0: f32,
        m1: f32,
        m2: f32,
        m3: f32,
    ) {
        let lo = panose_lo.to_le_bytes();
        self.header = [flag, lo[0], lo[1], panose_hi];
        self.m0 = m0;
        self.m1 = m1;
        self.m2 = m2;
        self.m3 = m3;
    }

    /// header byte +0 (flag).
    #[inline]
    pub fn flag(&self) -> u8 {
        self.header[0]
    }

    /// header byte +1..+2 as u16 LE.
    #[inline]
    pub fn panose_lo(&self) -> u16 {
        u16::from_le_bytes([self.header[1], self.header[2]])
    }

    /// header byte +3.
    #[inline]
    pub fn panose_hi(&self) -> u8 {
        self.header[3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transformation_size_and_align() {
        assert_eq!(size_of::<Transformation>(), 28);
        assert_eq!(align_of::<Transformation>(), 4);
    }

    #[test]
    fn rectf20_size_and_align() {
        assert_eq!(size_of::<RectF20>(), 20);
        assert_eq!(align_of::<RectF20>(), 4);
    }

    #[test]
    fn transformation_field_offsets() {
        let t = Transformation::ZERO;
        let base = &t as *const _ as usize;
        assert_eq!(&t.header0 as *const _ as usize - base, 0x00);
        assert_eq!(&t.header1 as *const _ as usize - base, 0x04);
        assert_eq!(&t.m0 as *const _ as usize - base, 0x08);
        assert_eq!(&t.m1 as *const _ as usize - base, 0x0c);
        assert_eq!(&t.m2 as *const _ as usize - base, 0x10);
        assert_eq!(&t.m3 as *const _ as usize - base, 0x14);
        assert_eq!(&t.degree_raw as *const _ as usize - base, 0x18);
    }

    #[test]
    fn rectf20_field_offsets() {
        let r = RectF20::ZERO;
        let base = &r as *const _ as usize;
        assert_eq!(&r.header as *const _ as usize - base, 0x00);
        assert_eq!(&r.m0 as *const _ as usize - base, 0x04);
        assert_eq!(&r.m1 as *const _ as usize - base, 0x08);
        assert_eq!(&r.m2 as *const _ as usize - base, 0x0c);
        assert_eq!(&r.m3 as *const _ as usize - base, 0x10);
    }

    #[test]
    fn rectf20_write_raw_byte_pattern() {
        let mut r = RectF20::ZERO;
        r.write_raw(1, 0x1234, 0x5a, 1.0, 2.0, 3.0, 4.0);
        // little-endian: 0x1234 = bytes [0x34, 0x12]
        assert_eq!(r.header, [1, 0x34, 0x12, 0x5a]);
        assert_eq!(r.m0, 1.0);
        assert_eq!(r.m3, 4.0);
        assert_eq!(r.flag(), 1);
        assert_eq!(r.panose_lo(), 0x1234);
        assert_eq!(r.panose_hi(), 0x5a);
    }

    #[test]
    fn transformation_write_raw_byte_pattern() {
        let mut t = Transformation::ZERO;
        t.write_raw(0, 1, [0xaa, 0xbb, 0xcc], 1.0, 2.0, 3.0, 4.0, 0x42b40000);
        assert_eq!(t.header0, [0, 0, 0, 0]);
        assert_eq!(t.header1, [1, 0xaa, 0xbb, 0xcc]);
        assert_eq!(t.m0, 1.0);
        assert_eq!(t.m3, 4.0);
        assert_eq!(t.degree_raw, 0x42b40000);
        assert_eq!(t.flag0(), 0);
        assert_eq!(t.flag1(), 1);
        assert_eq!(t.panose(), [0xaa, 0xbb, 0xcc]);
    }
}
