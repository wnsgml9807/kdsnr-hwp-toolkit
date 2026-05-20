//! `Hnc::Type::DrawingType::{Rgb, Cmyk, ScRgb, Hsl}` — Color 가 보관하는
//! 4 종류 POD value 타입의 raw layout.
//!
//! 각 타입의 정확한 byte size 는 `Color` 의 inline ctor (raw) 와 `Color::Swap`
//! 에서 도출:
//!
//! | type   | size | raw asm 인용 |
//! |--------|------|------------|
//! | `Rgb`  | 3B   | `Color(Rgb const&)` @ `0x14c7b4`: `ldrh w8,[x1]; ldrb w9,[x1,#0x2]` |
//! | `Cmyk` | 4B   | `Color(Cmyk const&)` @ `0x14c7d0`: `ldr w8,[x1]; str w8,[x0]` |
//! | `ScRgb`| 12B  | `Color(ScRgb const&)` @ `0x14c800`: `ldr x8,[x1]; ldr w9,[x1,#0x8]` |
//! | `Hsl`  | 12B  | `Color(Hsl const&)` @ `0x14c838`: 동일 패턴 |
//!
//! # 본 R-1.5.4 단계 scope
//!
//! Color 의 ctor 시그니처에 필요한 POD layout 만 정의. `operator<` /
//! `operator==` / `operator!=` 는 Color::operator< / operator== 의 sub-call 로
//! 호출되므로 그쪽 dispatch port 시점 (별도 세션) 에 추가.

/// `Hnc::Type::DrawingType::Rgb` — 3 bytes (r, g, b).
///
/// raw layout: `[0x00] = r (u8), [0x01] = g (u8), [0x02] = b (u8)`. align 1.
///
/// `Color(Rgb const&)` (raw `0x14c7b4`) 의 복사 패턴:
/// ```asm
/// 14c7b4: ldrh w8, [x1]            ; w8 = (g<<8)|r   (2 bytes at offset 0)
/// 14c7b8: ldrb w9, [x1, #0x2]      ; w9 = b           (1 byte at offset 2)
/// 14c7bc: strb w9, [x0, #0x2]      ; Color[0x02] = b
/// 14c7c0: strh w8, [x0]            ; Color[0x00..0x02] = (g<<8)|r → r at [0], g at [1]
/// 14c7c4: str wzr, [x0, #0xc]      ; type = 0 (Rgb)
/// 14c7c8: str xzr, [x0, #0x10]     ; effect = null
/// ```
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const _: () = assert!(std::mem::size_of::<Rgb>() == 3);

/// `Hnc::Type::DrawingType::Cmyk` — 4 bytes (c, m, y, k).
///
/// raw operator< (`0x26c34`) 가 byte-by-byte unsigned lexicographic compare —
/// `cmp w8, w9` (`b.hs`) 패턴 4회 반복. 따라서 packed [u8; 4].
///
/// raw ctor 의 `ldr w8, [x1]; str w8, [x0]` 는 4B 효율 복사일 뿐 — semantic 은
/// 4 separate u8.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmyk {
    pub c: u8,
    pub m: u8,
    pub y: u8,
    pub k: u8,
}

const _: () = assert!(std::mem::size_of::<Cmyk>() == 4);

/// `Hnc::Type::DrawingType::ScRgb` — 12 bytes (r, g, b 의 3 f32, scRgb = scale).
///
/// raw `Color(ScRgb const&)` (`0x14c800`):
/// ```asm
/// 14c800: ldr  x8, [x1]            ; q[0..7] = r, g (2 f32 packed)
/// 14c804: ldr  w9, [x1, #0x8]      ; q[8..11] = b (1 f32)
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScRgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

const _: () = assert!(std::mem::size_of::<ScRgb>() == 12);
const _: () = assert!(std::mem::align_of::<ScRgb>() == 4);

/// `Hnc::Type::DrawingType::Hsl` — 12 bytes. raw ctor 패턴이 ScRgb 와 동일.
///
/// 정확한 field 의미 (h:hue / s:sat / l:luminance) 는 raw 의 `operator==`
/// 별 RE 필요. 본 단계는 12B layout 만 확정.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    /// raw [0x00..0x04]
    pub h: f32,
    /// raw [0x04..0x08]
    pub s: f32,
    /// raw [0x08..0x0c]
    pub l: f32,
}

const _: () = assert!(std::mem::size_of::<Hsl>() == 12);
const _: () = assert!(std::mem::align_of::<Hsl>() == 4);

// =============================================================================
// operator==/!=/< for each DrawingType POD
// =============================================================================
//
// raw addresses:
// - Rgb::operator==   @ 0x29aa4
// - Rgb::operator!=   @ 0x26ef0
// - Rgb::operator<    @ 0x270d8
// - Cmyk::operator==  @ 0x26b9c
// - Cmyk::operator!=  @ 0x26be8
// - Cmyk::operator<   @ 0x26c34
// - ScRgb::operator== @ 0x29ae0
// - ScRgb::operator!= @ 0x29b24
// - ScRgb::operator<  @ 0x29b68  ← **Hancom의 inverted bug**: 반대 semantic
// - Hsl::operator==   @ 0x297e4
// - Hsl::operator!=   @ 0x29828
// - Hsl::operator<    @ 0x2986c

impl Rgb {
    /// raw `Rgb::operator==` (`0x29aa4`): byte-wise equality (3 byte).
    pub fn eq_struct(&self, other: &Rgb) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }

    /// raw `Rgb::operator!=` (`0x26ef0`).
    #[inline]
    pub fn ne_struct(&self, other: &Rgb) -> bool {
        !self.eq_struct(other)
    }

    /// raw `Rgb::operator<` (`0x270d8`): byte-wise unsigned lex compare.
    ///
    /// ```asm
    /// 270d8-: ldrb w8, [x0]; ldrb w9, [x1]; cmp w8, w9
    /// 270e4: b.hs skip; mov w0, #1; ret              ; this[0] < other[0] → 1
    /// 270f0: cmp w9, w8; b.hs skip; mov w0, #0; ret  ; this[0] > other[0] → 0
    /// 27100-: same for offset 1
    /// 27128-: cmp byte 2; cset w0, lo                 ; this[2] < other[2] → 1
    /// ```
    pub fn lt_struct(&self, other: &Rgb) -> bool {
        if self.r < other.r {
            return true;
        }
        if self.r > other.r {
            return false;
        }
        if self.g < other.g {
            return true;
        }
        if self.g > other.g {
            return false;
        }
        self.b < other.b
    }
}

impl Cmyk {
    /// raw `Cmyk::operator==` (`0x26b9c`): 4-byte equality.
    pub fn eq_struct(&self, other: &Cmyk) -> bool {
        self.c == other.c && self.m == other.m && self.y == other.y && self.k == other.k
    }

    /// raw `Cmyk::operator!=` (`0x26be8`).
    #[inline]
    pub fn ne_struct(&self, other: &Cmyk) -> bool {
        !self.eq_struct(other)
    }

    /// raw `Cmyk::operator<` (`0x26c34`): byte-wise unsigned lex compare (4 bytes).
    pub fn lt_struct(&self, other: &Cmyk) -> bool {
        if self.c < other.c {
            return true;
        }
        if self.c > other.c {
            return false;
        }
        if self.m < other.m {
            return true;
        }
        if self.m > other.m {
            return false;
        }
        if self.y < other.y {
            return true;
        }
        if self.y > other.y {
            return false;
        }
        self.k < other.k
    }
}

impl ScRgb {
    /// raw `ScRgb::operator==` (`0x29ae0`): 3-float equality.
    ///
    /// fcmp 의 NaN 처리: NaN == NaN → false (raw 의 `b.eq` 가 unordered 일 때 NOT taken).
    pub fn eq_struct(&self, other: &ScRgb) -> bool {
        self.r == other.r && self.g == other.g && self.b == other.b
    }

    /// raw `ScRgb::operator!=` (`0x29b24`).
    #[inline]
    pub fn ne_struct(&self, other: &ScRgb) -> bool {
        !self.eq_struct(other)
    }

    /// raw `ScRgb::operator<` (`0x29b68`) — **Hancom의 inverted bug 그대로 1:1 port**.
    ///
    /// Raw asm 의 `mov w0, #0` / `mov w0, #1` 위치가 Cmyk/Rgb/Hsl 의 normal 패턴과
    /// 반전됨 — 결과적으로 본 함수는 `self > other` 를 반환 (이름은 `<` 인데).
    ///
    /// ```asm
    /// 29b74: b.pl 0x29b80      ; if self.r >= other.r (or unordered): skip
    /// 29b78: mov w0, #0; ret   ; else (self.r < other.r): return 0  ← INVERTED
    /// 29b80: fcmp s1, s0       ; s1=other.r, s0=self.r
    /// 29b84: b.pl 0x29b90      ; if other.r >= self.r (equal): skip
    /// 29b88: mov w0, #1; ret   ; else (other.r < self.r = self.r > other.r): return 1  ← INVERTED
    /// ... (offset 4 동일 패턴)
    /// 29bd0: fcmp s1, s0       ; last element
    /// 29bd4: cset w0, mi       ; w0 = 1 if other.b < self.b (= self.b > other.b)
    /// ```
    ///
    /// Color::operator< 의 ScRgb 분기는 이 inverted 함수를 swap 후 호출 →
    /// 두 inversion 이 cancel 되어 결과적으로 잘못된 색 ordering 산출 (Hancom bug
    /// 그대로 byte-eq 보존).
    pub fn lt_struct(&self, other: &ScRgb) -> bool {
        // raw 의 inverted semantic: self > other 일 때 true
        if self.r < other.r {
            return false; // raw 29b78
        }
        if self.r > other.r {
            return true; // raw 29b88
        }
        // self.r == other.r
        if self.g < other.g {
            return false;
        }
        if self.g > other.g {
            return true;
        }
        self.b > other.b // raw 29bd4: cset mi (= other.b < self.b)
    }
}

impl Hsl {
    /// raw `Hsl::operator==` (`0x297e4`): 3-float equality.
    pub fn eq_struct(&self, other: &Hsl) -> bool {
        self.h == other.h && self.s == other.s && self.l == other.l
    }

    /// raw `Hsl::operator!=` (`0x29828`).
    #[inline]
    pub fn ne_struct(&self, other: &Hsl) -> bool {
        !self.eq_struct(other)
    }

    /// raw `Hsl::operator<` (`0x2986c`): 3-float lex compare (normal `<` semantic).
    ///
    /// ```asm
    /// 29874: fcmp s0, s1; b.pl skip; mov w0, #1; ret  ; self.h < other.h → 1
    /// 29884: fcmp s1, s0; b.pl skip; mov w0, #0; ret  ; self.h > other.h → 0
    /// ... offset 4 동일
    /// 298c8: fcmp s0, s1; csel w0, #1, wzr, mi        ; self.l < other.l → 1
    /// ```
    pub fn lt_struct(&self, other: &Hsl) -> bool {
        if self.h < other.h {
            return true;
        }
        if self.h > other.h {
            return false;
        }
        if self.s < other.s {
            return true;
        }
        if self.s > other.s {
            return false;
        }
        self.l < other.l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_raw() {
        assert_eq!(std::mem::size_of::<Rgb>(), 3);
        assert_eq!(std::mem::size_of::<Cmyk>(), 4);
        assert_eq!(std::mem::size_of::<ScRgb>(), 12);
        assert_eq!(std::mem::size_of::<Hsl>(), 12);
    }

    #[test]
    fn rgb_field_offsets() {
        let v = Rgb { r: 1, g: 2, b: 3 };
        let p = &v as *const Rgb as usize;
        assert_eq!(&v.r as *const _ as usize - p, 0);
        assert_eq!(&v.g as *const _ as usize - p, 1);
        assert_eq!(&v.b as *const _ as usize - p, 2);
    }

    #[test]
    fn cmyk_field_offsets() {
        let v = Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 4,
        };
        let p = &v as *const Cmyk as usize;
        assert_eq!(&v.c as *const _ as usize - p, 0);
        assert_eq!(&v.m as *const _ as usize - p, 1);
        assert_eq!(&v.y as *const _ as usize - p, 2);
        assert_eq!(&v.k as *const _ as usize - p, 3);
    }

    #[test]
    fn scrgb_field_offsets() {
        let v = ScRgb {
            r: 0.5,
            g: 1.0,
            b: 1.5,
        };
        let p = &v as *const ScRgb as usize;
        assert_eq!(&v.r as *const _ as usize - p, 0);
        assert_eq!(&v.g as *const _ as usize - p, 4);
        assert_eq!(&v.b as *const _ as usize - p, 8);
    }

    // ====== operator tests ======

    #[test]
    fn rgb_eq_basic() {
        let a = Rgb { r: 1, g: 2, b: 3 };
        let b = Rgb { r: 1, g: 2, b: 3 };
        let c = Rgb { r: 1, g: 2, b: 4 };
        assert!(a.eq_struct(&b));
        assert!(!a.eq_struct(&c));
        assert!(!a.ne_struct(&b));
        assert!(a.ne_struct(&c));
    }

    #[test]
    fn rgb_lt_lex_order() {
        let a = Rgb { r: 0, g: 0, b: 0 };
        let b = Rgb { r: 0, g: 0, b: 1 };
        let c = Rgb { r: 0, g: 1, b: 0 };
        let d = Rgb { r: 1, g: 0, b: 0 };
        assert!(a.lt_struct(&b));
        assert!(a.lt_struct(&c));
        assert!(a.lt_struct(&d));
        assert!(b.lt_struct(&c));
        assert!(c.lt_struct(&d));
        // not strict less
        assert!(!a.lt_struct(&a));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn rgb_lt_matches_raw_pattern_byte_compare() {
        // raw 0x270d8: this[0] < other[0] → 1
        // 즉 normal lex ordering
        let a = Rgb {
            r: 0x3a,
            g: 0x3c,
            b: 0x84,
        };
        let b = Rgb {
            r: 0xfa,
            g: 0xf3,
            b: 0xdb,
        };
        assert!(a.lt_struct(&b));
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn cmyk_eq_basic() {
        let a = Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 4,
        };
        let b = Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 4,
        };
        let c = Cmyk {
            c: 1,
            m: 2,
            y: 3,
            k: 5,
        };
        assert!(a.eq_struct(&b));
        assert!(a.ne_struct(&c));
    }

    #[test]
    fn cmyk_lt_lex_order_4_bytes() {
        let a = Cmyk {
            c: 0,
            m: 0,
            y: 0,
            k: 0,
        };
        let b = Cmyk {
            c: 0,
            m: 0,
            y: 0,
            k: 1,
        };
        let c = Cmyk {
            c: 0,
            m: 1,
            y: 0,
            k: 0,
        };
        let d = Cmyk {
            c: 1,
            m: 0,
            y: 0,
            k: 0,
        };
        assert!(a.lt_struct(&b));
        assert!(b.lt_struct(&c));
        assert!(c.lt_struct(&d));
        assert!(!d.lt_struct(&a));
    }

    #[test]
    fn hsl_lt_normal_semantic() {
        let a = Hsl {
            h: 0.0,
            s: 0.0,
            l: 0.0,
        };
        let b = Hsl {
            h: 0.0,
            s: 0.0,
            l: 0.5,
        };
        let c = Hsl {
            h: 0.0,
            s: 0.5,
            l: 0.0,
        };
        let d = Hsl {
            h: 1.0,
            s: 0.0,
            l: 0.0,
        };
        assert!(a.lt_struct(&b));
        assert!(b.lt_struct(&c));
        assert!(c.lt_struct(&d));
        assert!(!a.lt_struct(&a));
    }

    #[test]
    fn hsl_eq_basic() {
        let a = Hsl {
            h: 120.0,
            s: 0.5,
            l: 0.75,
        };
        let b = Hsl {
            h: 120.0,
            s: 0.5,
            l: 0.75,
        };
        let c = Hsl {
            h: 120.0,
            s: 0.5,
            l: 0.76,
        };
        assert!(a.eq_struct(&b));
        assert!(a.ne_struct(&c));
    }

    #[test]
    fn scrgb_lt_inverted_per_raw_asm() {
        // raw `0x29b68` 의 inverted semantic: self > other 일 때 true.
        let a = ScRgb {
            r: 0.5,
            g: 0.0,
            b: 0.0,
        };
        let b = ScRgb {
            r: 1.0,
            g: 0.0,
            b: 0.0,
        };
        // 정상 semantic 이면 a < b → true; raw inverted 이므로 → false.
        assert!(!a.lt_struct(&b), "raw inverted: a.r < b.r should give false");
        // 반대: b > a → 1
        assert!(b.lt_struct(&a), "raw inverted: b.r > a.r should give true");
    }

    #[test]
    fn scrgb_lt_equal_returns_false_for_first_then_compares_next() {
        // self.r == other.r, then second elem compared (same inverted semantic)
        let a = ScRgb {
            r: 0.5,
            g: 0.3,
            b: 0.0,
        };
        let b = ScRgb {
            r: 0.5,
            g: 0.4,
            b: 0.0,
        };
        // a.g < b.g → inverted → false
        assert!(!a.lt_struct(&b));
        // b.g > a.g → inverted → true
        assert!(b.lt_struct(&a));
    }

    #[test]
    fn scrgb_lt_last_element_via_cset_mi() {
        // raw 29bd4: cset w0, mi (= other.b < self.b)
        // → self.b > other.b 일 때 true
        let a = ScRgb {
            r: 0.5,
            g: 0.3,
            b: 0.8, // larger
        };
        let b = ScRgb {
            r: 0.5,
            g: 0.3,
            b: 0.2,
        };
        assert!(a.lt_struct(&b), "self.b > other.b → inverted true");
        assert!(!b.lt_struct(&a));
    }

    #[test]
    fn scrgb_lt_self_equal_returns_false() {
        let a = ScRgb {
            r: 0.5,
            g: 0.3,
            b: 0.2,
        };
        assert!(!a.lt_struct(&a));
    }

    #[test]
    fn scrgb_eq_basic() {
        let a = ScRgb {
            r: 0.5,
            g: 0.0,
            b: 1.0,
        };
        let b = ScRgb {
            r: 0.5,
            g: 0.0,
            b: 1.0,
        };
        let c = ScRgb {
            r: 0.5,
            g: 0.0,
            b: 1.1,
        };
        assert!(a.eq_struct(&b));
        assert!(a.ne_struct(&c));
    }

    #[test]
    fn hsl_field_offsets() {
        let v = Hsl {
            h: 120.0,
            s: 0.5,
            l: 0.75,
        };
        let p = &v as *const Hsl as usize;
        assert_eq!(&v.h as *const _ as usize - p, 0);
        assert_eq!(&v.s as *const _ as usize - p, 4);
        assert_eq!(&v.l as *const _ as usize - p, 8);
    }
}
