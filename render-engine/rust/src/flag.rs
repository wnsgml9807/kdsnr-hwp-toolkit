//! `Hnc::Type::Flag` — 8B (u64) bit-flag container.
//!
//! 위치: `libHncFoundation_arm64.dylib`
//! Export 심볼 (10 함수): 0x113c8 .. 0x1151f
//!
//! ```text
//! 00011288 T __ZN3Hnc2IO6Stream4LoadERKNS_4Type6ModuleEPKwS7_   (선행 - 무관)
//! 000113c8 T __ZN3Hnc4Type4FlagC2Ev          // Flag::Flag()                     base ctor
//! 000113d0 T __ZN3Hnc4Type4FlagC1Ev          // Flag::Flag()                     complete ctor
//! 000113d8 T __ZN3Hnc4Type4FlagD2Ev          // Flag::~Flag()                    base dtor
//! 000113dc T __ZN3Hnc4Type4FlagD1Ev          // Flag::~Flag()                    complete dtor
//! 000113e0 T __ZNK3Hnc4Type4FlageqERKS1_     // bool operator==(const Flag&) const
//! 000113f8 T __ZNK3Hnc4Type4FlagneERKS1_     // bool operator!=(const Flag&) const
//! 00011410 T __ZNK3Hnc4Type4FlagltERKS1_     // bool operator<(const Flag&) const
//! 000114ac T __ZN3Hnc4Type4FlagoRERKS1_      // Flag& operator|=(const Flag&)
//! 000114c0 T __ZNK3Hnc4Type4FlagorERKS1_     // Flag  operator|(const Flag&) const
//! 000114d4 T __ZN3Hnc4Type4Flag4SwapERS1_    // void Swap(Flag&)
//! 000114e8 T __ZNK3Hnc4Type4Flag8IsAllOffEv  // bool IsAllOff() const
//! 0001151c (end of region; next symbol 0x11520 is unrelated Guid::Generator::CreateID)
//! ```
//!
//! # 비트 의미 (operator< / IsAllOff / operator== mask 로부터 역추출)
//!
//! - **bit 0 (LSB, mask 0x1)** = "meta" flag. operator< / IsAllOff 에서 특별취급.
//! - **bit 1..62 (mask 0x7FFF_FFFF_FFFF_FFFE)** = 62 개 user-defined flags. operator== 의 mask
//!   `0x7FFFFFFFFFFFFFFF` 는 bit 0..62 즉 meta + 62 user flag, 즉 사실상 bit 63 만 제외.
//! - **bit 63 (MSB, mask 0x8000_0000_0000_0000)** = operator== 의 비교 mask 비트. `eor` 결과를
//!   `tst x8, #0x7fffffffffffffff` 로 마스킹하므로 bit 63 의 차이는 무시됨.
//!
//! 결과적으로 sizeof(Flag) = 8B, 내부는 단일 `u64` raw 비트.
//!
//! # Rust 매핑
//!
//! `Flag(pub u64)` — tuple struct. 모든 method 가 `*(u64*)&self` 를 직접 다루므로 raw u64
//! 노출이 raw asm 의 의도. C++ 의 `&self` (x0 register) 는 self 의 첫 8B 를 가리키며,
//! Rust 의 `&self` 도 `repr(transparent)` 효과로 동일한 메모리 레이아웃.

#![allow(clippy::needless_doctest_main)]

use std::fmt;

/// `Hnc::Type::Flag` — 8B bit-flag container.
///
/// Layout 1:1 with C++ (single `u64` field, no padding, no vtable).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Flag(pub u64);

impl Flag {
    /// `Hnc::Type::Flag::Flag()` — base ctor (C2) @ 0x113c8.
    ///
    /// Raw asm:
    /// ```text
    /// 000113c8  str  xzr, [x0]     // *(u64*)self = 0
    /// 000113cc  ret
    /// ```
    ///
    /// Complete ctor (C1) @ 0x113d0 is byte-identical:
    /// ```text
    /// 000113d0  str  xzr, [x0]
    /// 000113d4  ret
    /// ```
    pub const fn new() -> Self {
        Flag(0)
    }

    /// `Hnc::Type::Flag::~Flag()` — base/complete dtor (D2/D1) @ 0x113d8 / 0x113dc.
    ///
    /// Raw asm:
    /// ```text
    /// 000113d8  ret    // D2 — pure no-op
    /// 000113dc  ret    // D1 — pure no-op (no sub-object, no vtable)
    /// ```
    ///
    /// Rust 의 `Drop` 도 trivial 이므로 `Drop` impl 없음 = 동일.
    /// Copy 가능하다는 것도 raw 의 의도와 일치.
    pub fn drop_explicit(self) {
        // no-op; matches raw `ret`.
        let _ = self;
    }

    /// `Hnc::Type::Flag::operator==(const Flag&) const` @ 0x113e0.
    ///
    /// Raw asm:
    /// ```text
    /// 000113e0  ldr   x8, [x1]                       // x8 = rhs.val
    /// 000113e4  ldr   x9, [x0]                       // x9 = self.val
    /// 000113e8  eor   x8, x9, x8                     // x8 = self ^ rhs
    /// 000113ec  tst   x8, #0x7fffffffffffffff        // test low 63 bits (mask out bit 63)
    /// 000113f0  cset  w0, eq                         // ret (low63(self) == low63(rhs))
    /// 000113f4  ret
    /// ```
    ///
    /// Bit 63 (`0x8000_0000_0000_0000`) 은 무시 (XOR 후 마스킹). 나머지 63 bits 가 일치하면 true.
    #[allow(clippy::should_implement_trait)]
    pub fn eq_flag(&self, rhs: &Flag) -> bool {
        const MASK_LOW63: u64 = 0x7FFF_FFFF_FFFF_FFFF;
        ((self.0 ^ rhs.0) & MASK_LOW63) == 0
    }

    /// `Hnc::Type::Flag::operator!=(const Flag&) const` @ 0x113f8.
    ///
    /// Raw asm:
    /// ```text
    /// 000113f8  ldr   x8, [x1]
    /// 000113fc  ldr   x9, [x0]
    /// 00011400  eor   x8, x9, x8
    /// 00011404  tst   x8, #0x7fffffffffffffff
    /// 00011408  cset  w0, ne                         // ret (low63 differ)
    /// 0001140c  ret
    /// ```
    pub fn ne_flag(&self, rhs: &Flag) -> bool {
        const MASK_LOW63: u64 = 0x7FFF_FFFF_FFFF_FFFF;
        ((self.0 ^ rhs.0) & MASK_LOW63) != 0
    }

    /// `Hnc::Type::Flag::operator<(const Flag&) const` @ 0x11410.
    ///
    /// Raw asm:
    /// ```text
    /// 00011410  ldr   x9,  [x0]                  // x9  = self.val
    /// 00011414  ldr   x10, [x1]                  // x10 = rhs.val
    /// 00011418  ands  x8,  x10, #0x1             // x8  = rhs.meta (bit 0)
    /// 0001141c  cset  w13, eq                    // w13 = (rhs.meta == 0)
    /// 00011420  tbnz  w9,  #0x0, 0x11434         // if self.meta != 0 → goto loop
    /// 00011424  cbz   x8,            0x11434     // if rhs.meta  == 0 → goto loop
    /// 00011428  mov   w8, #0x1                   // ; here: self.meta==0 AND rhs.meta!=0
    /// 0001142c  and   w0, w8, #0x1               // → return 1 (self < rhs)
    /// 00011430  ret
    /// 00011434  mov   x11, #0x0                  // i = 0
    /// 00011438  and   w14, w9,  #0x1             // w14 = self.meta
    /// 0001143c  mov   w8,  #0x1                  // w8 = 1
    /// 00011440  mov   w12, #0x2                  // bit_base = 2
    /// 00011444  tbz   w14, #0x0, 0x1144c         // if !(self_bit prev) → fall through
    /// 00011448  tbnz  w13, #0x0, 0x11490         // if (rhs_bit prev == 0) → return 0
    /// 0001144c  cmp   x11, #0x3e                 // saturate flag = (i < 0x3e)
    /// 00011450  cset  w8,  lo
    /// 00011454  add   x11, x11, #0x1             // i++
    /// 00011458  cmp   x11, #0x3f
    /// 0001145c  b.eq  0x114a0                    // if i == 0x3f → fall-through exit
    /// 00011460  sub   x13, x11, #0x1
    /// 00011464  lsl   x13, x12, x13              // bit_mask = 2 << (i - 1) = 1 << i
    /// 00011468  ands  x16, x9,  x13              // self_bit  = self & bit_mask
    /// 0001146c  cset  w14, ne                    // w14 = (self_bit != 0)
    /// 00011470  ands  x15, x10, x13              // rhs_bit   = rhs  & bit_mask
    /// 00011474  cset  w13, eq                    // w13 = (rhs_bit == 0)
    /// 00011478  cbnz  x16, 0x11444               // if self_bit != 0 → loop back
    /// 0001147c  cbz   x15, 0x11444               // if rhs_bit  == 0 → loop back
    /// 00011480  mov   w9, #0x1                   // ; self_bit==0 AND rhs_bit!=0
    /// 00011484  and   w8,  w8, w9                // → return (i_old < 0x3e)
    /// 00011488  and   w0,  w8, #0x1
    /// 0001148c  ret
    /// 00011490  mov   w9, #0x0                   // ; self_bit==1 AND rhs_bit==0
    /// 00011494  and   w8,  w8, w9                // → return 0
    /// 00011498  and   w0,  w8, #0x1
    /// 0001149c  ret
    /// 000114a0  and   w8,  w8, w9                // ; loop exhausted (i hit 0x3f)
    /// 000114a4  and   w0,  w8, #0x1              // → return 0 (w9 = self_low32 from initial ldr;
    /// 000114a8  ret                              //    AND w8(=0 since i_old=0x3e) → 0; net 0)
    /// ```
    ///
    /// 의미적 요약 (raw asm 의 충실한 재현 — 절대 단순화 금지):
    /// 1. self.meta=0 AND rhs.meta=1 → return true
    /// 2. self.meta=1 AND rhs.meta=0 → return false  (loop entry path 의 0x11490 trap)
    /// 3. self.meta == rhs.meta → loop bits 1..62 from low to high:
    ///    - 첫 차이 bit 발견:
    ///      - self_bit=0, rhs_bit=1 → return true (w8 = (i_old < 0x3e) — i_old 는 발견 직전 i)
    ///      - self_bit=1, rhs_bit=0 → return false
    ///    - 모든 bit 동일 → return false
    ///
    /// 주의: bit 63 은 검사하지 않음 (loop 종료 조건 i == 0x3f). operator== 의 mask 와 일관.
    #[allow(clippy::should_implement_trait)]
    pub fn lt_flag(&self, rhs: &Flag) -> bool {
        let self_val = self.0;
        let rhs_val = rhs.0;

        // Entry path: if (self.meta == 0 && rhs.meta != 0) → return true
        let self_meta = (self_val & 1) != 0;
        let rhs_meta = (rhs_val & 1) != 0;
        if !self_meta && rhs_meta {
            return true;
        }
        // Otherwise fall into bit-by-bit loop. The loop pre-checks (self.meta, rhs.meta) once
        // at top via the `tbz w14, #0, 0x1144c` / `tbnz w13, #0, 0x11490` pair using the
        // INITIAL w14=self.meta and w13=(rhs.meta==0). We replicate that semantic at entry
        // before iterating bits 1..62.
        if self_meta && !rhs_meta {
            // 0x11444 → 0x11448 (self.meta=1 AND rhs.meta=0) → 0x11490 path → return 0
            return false;
        }
        // else: meta bits are EITHER both 0 OR both 1. Iterate bits 1..62.
        //
        // Raw asm increments `i` from 0 BEFORE testing bit i (`bit_mask = 1<<i`), so the
        // tested bit positions are i = 1, 2, ..., 0x3e (62). Loop exits when i_new == 0x3f.
        let mut i: u64 = 0;
        loop {
            // 0x1144c: w8 = (i < 0x3e) (saturate flag, captured BEFORE increment)
            let saturate = i < 0x3e;
            // 0x11454: i++
            i += 1;
            // 0x11458-0x1145c: if i == 0x3f → exit loop → return 0
            if i == 0x3f {
                return false;
            }
            // 0x11460-0x11464: bit_mask = 1 << i (since x12=2, shift = i-1)
            let bit_mask: u64 = 1u64 << i;
            let self_bit = (self_val & bit_mask) != 0;
            let rhs_bit = (rhs_val & bit_mask) != 0;

            // 0x11478: if self_bit != 0 → loop back (continue), BUT at top check
            // tbnz w13, #0, 0x11490 — using w13 = (rhs_bit == 0). So if self_bit=1 AND rhs_bit=0
            // we trap into 0x11490 (return 0). If self_bit=1 AND rhs_bit=1, continue loop.
            // 0x1147c: if rhs_bit == 0 (and self_bit was 0) → loop back, but at top check
            // tbz w14, #0 with w14=0 → fall through to next bit.
            if self_bit && !rhs_bit {
                return false; // 0x11490 path
            }
            if !self_bit && rhs_bit {
                return saturate; // 0x11480 path: w8 (=i_old<0x3e) & 1
            }
            // else: bits equal → continue (effectively fall through cbnz/cbz back to 0x11444)
        }
    }

    /// `Hnc::Type::Flag::operator|=(const Flag&)` @ 0x114ac.
    ///
    /// Raw asm:
    /// ```text
    /// 000114ac  ldr   x8,  [x1]            // x8 = rhs.val
    /// 000114b0  ldr   x9,  [x0]            // x9 = self.val
    /// 000114b4  orr   x8,  x9, x8          // x8 = self | rhs
    /// 000114b8  str   x8,  [x0]            // self.val = x8
    /// 000114bc  ret                        // returns self (caller convention; x0 unchanged)
    /// ```
    pub fn or_assign(&mut self, rhs: &Flag) {
        self.0 |= rhs.0;
    }

    /// `Hnc::Type::Flag::operator|(const Flag&) const` @ 0x114c0.
    ///
    /// Raw asm (sret return via x8):
    /// ```text
    /// 000114c0  ldr   x9,  [x0]            // x9 = self.val
    /// 000114c4  ldr   x10, [x1]            // x10 = rhs.val
    /// 000114c8  orr   x9,  x10, x9         // x9 = rhs | self
    /// 000114cc  str   x9,  [x8]            // *result = x9        (x8 = sret slot)
    /// 000114d0  ret
    /// ```
    pub fn or_flag(&self, rhs: &Flag) -> Flag {
        Flag(self.0 | rhs.0)
    }

    /// `Hnc::Type::Flag::Swap(Flag&)` @ 0x114d4.
    ///
    /// Raw asm:
    /// ```text
    /// 000114d4  ldr   x8, [x0]    // x8 = self.val
    /// 000114d8  ldr   x9, [x1]    // x9 = other.val
    /// 000114dc  str   x9, [x0]    // self.val  = other.val
    /// 000114e0  str   x8, [x1]    // other.val = self.val (saved in x8)
    /// 000114e4  ret
    /// ```
    pub fn swap(&mut self, other: &mut Flag) {
        std::mem::swap(&mut self.0, &mut other.0);
    }

    /// `Hnc::Type::Flag::IsAllOff() const` @ 0x114e8.
    ///
    /// Raw asm:
    /// ```text
    /// 000114e8  ldr   x8,  [x0]                    // x8 = self.val
    /// 000114ec  tbnz  w8,  #0x0, 0x11518           // if self.meta (bit 0) set → return 0
    /// 000114f0  mov   x10, #0x0                    // i = 0
    /// 000114f4  mov   x9,  x10                     // x9 = i (saved before increment / check)
    /// 000114f8  cmp   x10, #0x3e
    /// 000114fc  b.eq  0x1150c                      // if i == 0x3e → exit loop
    /// 00011500  add   x10, x9,  #0x1               // i++  (next iteration uses this)
    /// 00011504  lsr   x11, x8,  x9                 // x11 = self >> i_old
    /// 00011508  tbz   w11, #0x1, 0x114f4           // if bit (i_old+1) of self == 0 → loop
    /// 0001150c  cmp   x9,  #0x3d                   // (exit) compare i_old to 0x3d
    /// 00011510  cset  w0,  hi                      // return (i_old > 0x3d)
    /// 00011514  ret
    /// 00011518  mov   w0,  #0x0                    // (meta path) return false
    /// 0001151c  ret
    /// ```
    ///
    /// 의미:
    /// - meta (bit 0) 가 set 이면 즉시 false.
    /// - 그렇지 않으면 bits 1..62 (총 62 비트) 를 LSB→MSB 로 스캔.
    /// - 첫 set bit 발견 시 i_old = (set bit 의 index - 1) 로 exit; 모든 비트가 0 이면 i_old = 0x3e.
    /// - 마지막 `(i_old > 0x3d)` → loop 완전 종료 (i_old = 0x3e) 시에만 true.
    /// - 즉 "meta=0 AND no bits set in 1..62" 이면 true.
    pub fn is_all_off(&self) -> bool {
        let val = self.0;
        // bit 0 (meta) set → false
        if (val & 1) != 0 {
            return false;
        }
        // scan bits 1..62. Raw asm structure: i starts at 0, tested bit position is (i+1),
        // loop exits when i reaches 0x3e (no bit set at positions 1..62).
        let mut i: u64 = 0;
        loop {
            // 0x114f8: if i == 0x3e → goto 0x1150c (exit)
            if i == 0x3e {
                // 0x1150c: cmp i (= 0x3e), 0x3d → cset hi (i > 0x3d) = 1
                return i > 0x3d;
            }
            // 0x11504: x11 = self >> i; 0x11508: tbz bit 1 of x11 (== bit (i+1) of self)
            let bit_test = (val >> i) & 0b10; // bit 1 of (val >> i) == bit (i+1) of val
            if bit_test != 0 {
                // 0x1150c: cmp i (current = i_old before increment), 0x3d → cset hi
                return i > 0x3d;
            }
            // 0x11500: i++ (loop)
            i += 1;
        }
    }
}

impl Default for Flag {
    fn default() -> Self {
        Flag::new()
    }
}

// Rust 의 `PartialEq` 가 `==` 연산자를 제공하지만, raw asm 의 mask semantics 와 일치시키기 위해
// 명시적으로 `eq_flag` 를 호출하도록 binding.
impl PartialEq for Flag {
    fn eq(&self, other: &Self) -> bool {
        self.eq_flag(other)
    }
}
impl Eq for Flag {}

// raw `operator<` 의 의미 (bit-by-bit LSB→MSB 비교) 를 PartialOrd 에도 노출.
impl PartialOrd for Flag {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Flag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.lt_flag(other) {
            std::cmp::Ordering::Less
        } else if other.lt_flag(self) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

impl std::ops::BitOr for Flag {
    type Output = Flag;
    fn bitor(self, rhs: Self) -> Flag {
        self.or_flag(&rhs)
    }
}
impl std::ops::BitOrAssign for Flag {
    fn bitor_assign(&mut self, rhs: Self) {
        self.or_assign(&rhs)
    }
}

// Hash impl 은 raw 에 없음. PartialEq 와 일관성을 위해 mask 적용된 low63 만 hash.
impl std::hash::Hash for Flag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        const MASK_LOW63: u64 = 0x7FFF_FFFF_FFFF_FFFF;
        (self.0 & MASK_LOW63).hash(state);
    }
}

impl fmt::Debug for Flag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Flag(0x{:016x})", self.0)
    }
}

// ===== sizeof / repr 정적 검증 =====
const _: () = assert!(std::mem::size_of::<Flag>() == 8, "Flag must be exactly 8B");
const _: () = assert!(std::mem::align_of::<Flag>() == 8, "Flag must be 8B aligned");

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ctor / dtor ----
    #[test]
    fn ctor_zeroes_raw_u64() {
        let f = Flag::new();
        assert_eq!(f.0, 0);
    }

    #[test]
    fn default_matches_ctor() {
        assert_eq!(Flag::default().0, Flag::new().0);
    }

    #[test]
    fn dtor_is_trivial_noop() {
        let f = Flag(0xDEAD_BEEF_CAFE_BABE);
        f.drop_explicit(); // no panic, no side effect
    }

    // ---- operator|= / |= ----
    #[test]
    fn or_assign_combines_bits() {
        let mut a = Flag(0b0101);
        let b = Flag(0b1010);
        a.or_assign(&b);
        assert_eq!(a.0, 0b1111);
    }

    #[test]
    fn or_assign_uses_full_u64() {
        // 모든 비트가 영향 받음 (bit 63 포함; mask 없음).
        let mut a = Flag(0x8000_0000_0000_0001);
        let b = Flag(0x4000_0000_0000_0002);
        a.or_assign(&b);
        assert_eq!(a.0, 0xC000_0000_0000_0003);
    }

    // ---- operator| (returning) ----
    #[test]
    fn or_flag_returns_new() {
        let a = Flag(0xF0F0);
        let b = Flag(0x0F0F);
        let c = a.or_flag(&b);
        assert_eq!(c.0, 0xFFFF);
        // a/b 불변
        assert_eq!(a.0, 0xF0F0);
        assert_eq!(b.0, 0x0F0F);
    }

    #[test]
    fn or_op_trait_matches_or_flag() {
        let a = Flag(0xAA);
        let b = Flag(0x55);
        assert_eq!((a | b).0, (a.or_flag(&b)).0);
    }

    // ---- operator== / != ----
    #[test]
    fn eq_ignores_bit63() {
        // raw: tst x8, #0x7fffffffffffffff — bit 63 차이 무시.
        let a = Flag(0x0000_0000_0000_0001);
        let b = Flag(0x8000_0000_0000_0001);
        assert!(a.eq_flag(&b));
        assert!(!a.ne_flag(&b));
        assert_eq!(a, b);
    }

    #[test]
    fn eq_compares_low63_bits() {
        let a = Flag(0x0000_0000_0000_0001);
        let b = Flag(0x0000_0000_0000_0002);
        assert!(!a.eq_flag(&b));
        assert!(a.ne_flag(&b));
        assert_ne!(a, b);
    }

    #[test]
    fn eq_zero_with_bit63_set_still_equal_to_zero() {
        let a = Flag(0);
        let b = Flag(0x8000_0000_0000_0000);
        assert!(a.eq_flag(&b));
    }

    // ---- operator< — meta paths ----
    #[test]
    fn lt_self_meta0_rhs_meta1_returns_true() {
        // raw entry: 0x1142c path → return 1
        let a = Flag(0x0000_0000_0000_0000); // meta=0
        let b = Flag(0x0000_0000_0000_0001); // meta=1
        assert!(a.lt_flag(&b));
    }

    #[test]
    fn lt_self_meta1_rhs_meta0_returns_false() {
        // raw: 0x11420 takes branch to 0x11434 → 0x11448 → 0x11490 → return 0
        let a = Flag(0x0000_0000_0000_0001);
        let b = Flag(0x0000_0000_0000_0000);
        assert!(!a.lt_flag(&b));
    }

    // ---- operator< — bit-by-bit loop paths ----
    #[test]
    fn lt_self_lt_rhs_at_bit1_returns_true() {
        // bit 0 동일 (둘 다 0). bit 1: self=0, rhs=1 → return saturate (i_old=0 < 0x3e) = true
        let a = Flag(0b00);
        let b = Flag(0b10);
        assert!(a.lt_flag(&b));
    }

    #[test]
    fn lt_self_gt_rhs_at_bit1_returns_false() {
        let a = Flag(0b10);
        let b = Flag(0b00);
        assert!(!a.lt_flag(&b));
    }

    #[test]
    fn lt_equal_low63_returns_false() {
        // 동일 → 모든 bit 동일 → loop exit (i==0x3f) → return 0
        let a = Flag(0x1234_5678_9ABC_DEF0);
        let b = Flag(0x1234_5678_9ABC_DEF0);
        assert!(!a.lt_flag(&b));
    }

    #[test]
    fn lt_ignores_bit63_difference() {
        // bit 63 은 loop 에서 검사되지 않음 (i == 0x3f 에서 exit)
        let a = Flag(0x0000_0000_0000_0000);
        let b = Flag(0x8000_0000_0000_0000);
        assert!(!a.lt_flag(&b));
        assert!(!b.lt_flag(&a));
    }

    #[test]
    fn lt_difference_at_bit62_returns_true_when_self_lower() {
        // bit 62 = 0x4000_0000_0000_0000.
        // i_old 시점은 i=0x3d 일 때 (i++ 후 i=0x3e, 그 시점 bit_mask=1<<0x3e=bit 62).
        // saturate = (0x3d < 0x3e) = true.
        let a = Flag(0x0000_0000_0000_0000);
        let b = Flag(0x4000_0000_0000_0000);
        assert!(a.lt_flag(&b));
    }

    #[test]
    fn lt_meta_equal_both_zero_self_has_lowest_set_bit_returns_false() {
        // 양쪽 meta=0. scan bits 1..62 LSB→MSB.
        // bit 2: self=1, rhs=0 → 다음 iteration 시작 시 (tbnz w13, 0) → return 0.
        // 즉 numeric (4 < 8) 이지만 raw asm 의 비트별 LSB→MSB 비교에서는 false.
        let a = Flag(0b0100);
        let b = Flag(0b1000);
        assert!(!a.lt_flag(&b));
        // 반대: rhs 가 낮은 set bit 를 가지면 self_bit=0, rhs_bit=1 → return true.
        assert!(b.lt_flag(&a));
    }

    #[test]
    fn lt_meta_equal_both_zero_bit1_strictly_less() {
        // bit 1 에서 self=0, rhs=1 → return saturate (true).
        let a = Flag(0b0000);
        let b = Flag(0b0010);
        assert!(a.lt_flag(&b));
    }

    #[test]
    fn lt_low_bit_set_in_self_dominates_higher_set_in_rhs() {
        // raw operator< 의 핵심 특성: LSB 우선. self 의 낮은 비트가 set 이면, rhs 의 더 높은
        // 비트들이 set 이어도 self < rhs 는 false.
        // a = 0b00000010 (bit 1)
        // b = 0b11111100 (bits 2..7)
        let a = Flag(0b00000010);
        let b = Flag(0b11111100);
        assert!(!a.lt_flag(&b)); // a 가 bit 1 에서 먼저 1 을 가짐 → false
        assert!(b.lt_flag(&a));  // 반대 방향: b 가 bit 2 에서 먼저 1 을 가지지만 a 는 bit 1
                                  // 에서 먼저 1 → b 의 입장에선 bit 1: self_b=0, rhs_a=1 → return true
    }

    #[test]
    fn lt_meta_equal_both_one_then_bits() {
        // meta=1 둘 다, bit 1 비교: self=0, rhs=1 → true
        let a = Flag(0b001);
        let b = Flag(0b011);
        assert!(a.lt_flag(&b));
    }

    // ---- Swap ----
    #[test]
    fn swap_exchanges_values() {
        let mut a = Flag(0xAA);
        let mut b = Flag(0x55);
        a.swap(&mut b);
        assert_eq!(a.0, 0x55);
        assert_eq!(b.0, 0xAA);
    }

    // ---- IsAllOff ----
    #[test]
    fn is_all_off_zero_returns_true() {
        assert!(Flag(0).is_all_off());
    }

    #[test]
    fn is_all_off_meta_set_returns_false() {
        // bit 0 (meta) 만 set → false
        assert!(!Flag(0b1).is_all_off());
    }

    #[test]
    fn is_all_off_bit_1_set_returns_false() {
        // bit 1 set, meta=0 → false (loop detects bit at position 1)
        assert!(!Flag(0b10).is_all_off());
    }

    #[test]
    fn is_all_off_bit_62_set_returns_false() {
        // 마지막 검사 비트 (position 62 = 1 << 62)
        assert!(!Flag(1u64 << 62).is_all_off());
    }

    #[test]
    fn is_all_off_bit_63_ignored_returns_true() {
        // bit 63 은 loop 범위 밖 (i==0x3e 에서 exit) — 검사하지 않음 → true
        assert!(Flag(1u64 << 63).is_all_off());
    }

    #[test]
    fn is_all_off_all_bits_set_meta1_returns_false() {
        assert!(!Flag(u64::MAX).is_all_off());
    }

    // ---- traits ----
    #[test]
    fn ord_consistency_with_lt() {
        let a = Flag(0b00);
        let b = Flag(0b10);
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Less);
        assert_eq!(b.cmp(&a), std::cmp::Ordering::Greater);
        assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);
    }

    #[test]
    fn bitor_trait_matches() {
        let a = Flag(0xF0);
        let b = Flag(0x0F);
        let c = a | b;
        assert_eq!(c.0, 0xFF);
    }

    #[test]
    fn bitor_assign_trait_matches() {
        let mut a = Flag(0xF0);
        a |= Flag(0x0F);
        assert_eq!(a.0, 0xFF);
    }

    // ---- repr / size invariants ----
    #[test]
    fn sizeof_is_8b() {
        assert_eq!(std::mem::size_of::<Flag>(), 8);
        assert_eq!(std::mem::align_of::<Flag>(), 8);
    }

    #[test]
    fn debug_format_is_hex_u64() {
        let s = format!("{:?}", Flag(0xCAFE_BABE_DEAD_BEEF));
        assert_eq!(s, "Flag(0xcafebabedeadbeef)");
    }
}
