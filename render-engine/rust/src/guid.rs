//! `Hnc::Type::Guid` — 16B Windows GUID (RFC 4122 UUID 와 호환).
//!
//! 위치: `libHncFoundation_arm64.dylib`. Export 심볼 (8 함수 + Generator):
//!
//! ```text
//! 00011520 T  __ZN3Hnc4Type4Guid9Generator8CreateIDEv               // Guid::Generator::CreateID()
//! 0001152c T  __ZN3Hnc4Type4GuidC1Ev                                 // Guid::Guid()  complete ctor
//! 00011534 T  __ZN3Hnc4Type4GuidD1Ev                                 // Guid::~Guid() complete dtor
//! 00011538 T  __ZN3Hnc4Type4Guid9Generator8CreateIDERK11CHncStringW  // CreateID(const CHncStringW&)
//! 00011a6c T  __ZN3Hnc4Type4GuidC2Ev                                 // Guid::Guid()  base ctor
//! 00011a74 T  __ZN3Hnc4Type4GuidC2ERKS1_                             // Guid::Guid(const Guid&) base
//! 00011a80 T  __ZN3Hnc4Type4GuidC1ERKS1_                             // Guid::Guid(const Guid&) complete
//! 00011a8c T  __ZN3Hnc4Type4GuidD2Ev                                 // Guid::~Guid() base
//! 00011a90 T  __ZNK3Hnc4Type4GuideqERKS1_                            // operator==(const Guid&) const
//! 00011aa8 T  __ZNK3Hnc4Type4GuidneERKS1_                            // operator!=(const Guid&) const
//! 00011ac0 T  __ZNK3Hnc4Type4GuidltERKS1_                            // operator<(const Guid&) const
//! 00011b0c T  __ZNK3Hnc4Type4Guid9GetStringEv                         // GetString() const → CHncStringW
//! ```
//!
//! # Layout (CreateID(const CHncStringW&) raw asm 으로부터 확정)
//!
//! Windows `GUID` struct 와 1:1:
//!
//! ```c
//! typedef struct _GUID {
//!     ULONG    Data1;       // 4B  @ offset 0x00
//!     USHORT   Data2;       // 2B  @ offset 0x04
//!     USHORT   Data3;       // 2B  @ offset 0x06
//!     UCHAR    Data4[8];    // 8B  @ offset 0x08..0x0F
//! } GUID;                   // 16B total
//! ```
//!
//! CreateID 의 parsing 부분:
//! ```text
//! 00011650  str   w0, [x19]          // *(Guid + 0x00) = u32 Data1
//! 00011664  strh  w0, [x19, #0x4]    // *(Guid + 0x04) = u16 Data2
//! 00011678  strh  w0, [x19, #0x6]    // *(Guid + 0x06) = u16 Data3
//! 0001168c  strb  w0, [x19, #0x8]    // *(Guid + 0x08) = u8  Data4[0]
//! 000116a0  strb  w0, [x19, #0x9]    // ... Data4[1]
//! 000116b4  strb  w0, [x19, #0xa]    // ... Data4[2]
//! ; ... continues through Data4[7] at offset 0x0F
//! ```
//!
//! # 정렬 / sizeof
//!
//! `operator==` 가 `ldp x8, x9, [x0]` (8B pair load) 를 사용 — 자연정렬 8B 가 안전.
//! repr(C) 의 u32 alignment 가 4B 라서 macOS arm64 의 unaligned access 허용 정책상 동작은
//! 하지만, byte-equivalent 보장을 위해 `align(8)` 강제. sizeof = 16B 유지.

#![allow(clippy::module_name_repetitions)]

use std::fmt;

/// `Hnc::Type::Guid` — 16B Windows GUID / RFC 4122 UUID 호환.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct Guid {
    /// offset 0x00, 4B, little-endian native u32 (raw asm `str w0, [x19]` 로 그대로 저장).
    pub data1: u32,
    /// offset 0x04, 2B u16.
    pub data2: u16,
    /// offset 0x06, 2B u16.
    pub data3: u16,
    /// offset 0x08, 8B 의 8 × u8 array.
    pub data4: [u8; 8],
}

impl Guid {
    /// `Hnc::Type::Guid::Guid()` C2 @ 0x11a6c (and C1 @ 0x1152c — byte-identical).
    ///
    /// Raw asm:
    /// ```text
    /// 00011a6c  stp  xzr, xzr, [x0]    // [x0..x0+0x10] = 0  (zero 16B)
    /// 00011a70  ret
    /// ```
    pub const fn new() -> Self {
        Guid {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0u8; 8],
        }
    }

    /// `Hnc::Type::Guid::Guid(const Guid&)` C2 @ 0x11a74 (and C1 @ 0x11a80 — byte-identical).
    ///
    /// Raw asm:
    /// ```text
    /// 00011a74  ldr  q0, [x1]    // load 16B from other
    /// 00011a78  str  q0, [x0]    // store to self
    /// 00011a7c  ret
    /// ```
    pub fn copy_from(other: &Guid) -> Self {
        *other // Rust Copy trait equivalent to raw asm 의 16B copy.
    }

    /// `Hnc::Type::Guid::~Guid()` D2/D1 @ 0x11a8c / 0x11534.
    ///
    /// Raw asm: `ret` (no-op). Rust 의 Drop 도 trivial.
    pub fn drop_explicit(self) {
        let _ = self;
    }

    /// `Hnc::Type::Guid::operator==(const Guid&) const` @ 0x11a90.
    ///
    /// Raw asm:
    /// ```text
    /// 00011a90  ldp   x8, x9,  [x0]            // self.{lo, hi} 8B each
    /// 00011a94  ldp   x10, x11, [x1]           // other.{lo, hi}
    /// 00011a98  cmp   x8, x10                  // compare lo
    /// 00011a9c  ccmp  x9, x11, #0x0, eq        // if lo equal, compare hi
    /// 00011aa0  cset  w0, eq                   // return both halves equal
    /// 00011aa4  ret
    /// ```
    ///
    /// 즉 16B 전체 비트 비교 (= memcmp).
    pub fn eq_guid(&self, other: &Guid) -> bool {
        let s = self.as_u64_pair();
        let o = other.as_u64_pair();
        s.0 == o.0 && s.1 == o.1
    }

    /// `Hnc::Type::Guid::operator!=(const Guid&) const` @ 0x11aa8.
    ///
    /// Raw asm: same as `==` but `cset w0, ne`.
    pub fn ne_guid(&self, other: &Guid) -> bool {
        !self.eq_guid(other)
    }

    /// `Hnc::Type::Guid::operator<(const Guid&) const` @ 0x11ac0.
    ///
    /// Raw asm:
    /// ```text
    /// 00011ac0  ldr   x8, [x0]              // x8 = self.lo (8B as LE u64)
    /// 00011ac4  ldr   x9, [x1]              // x9 = other.lo
    /// 00011ac8  rev   x8, x8                // byte-swap → x8 = big-endian view of self.lo
    /// 00011acc  rev   x9, x9                // x9 = big-endian view of other.lo
    /// 00011ad0  cmp   x8, x9
    /// 00011ad4  b.ne  0x11af8               // if differ → branch to "compare hi half"
    /// 00011ad8  ldr   x8, [x0, #0x8]        // x8 = self.hi (LE u64)
    /// 00011adc  ldr   x9, [x1, #0x8]        // x9 = other.hi
    /// 00011ae0  rev   x8, x8
    /// 00011ae4  rev   x9, x9
    /// 00011ae8  cmp   x8, x9
    /// 00011aec  b.ne  0x11af8
    /// 00011af0  lsr   w0, wzr, #31          // equal → return 0 (false)
    /// 00011af4  ret
    /// 00011af8  cmp   x8, x9                // (last differing pair)
    /// 00011afc  mov   w8, #-0x1
    /// 00011b00  cneg  w8, w8, hs            // w8 = (self_be >= other_be) ? +1 : -1
    /// 00011b04  lsr   w0, w8, #31           // w0 = sign bit = 1 iff w8 was negative (self_be < other_be)
    /// 00011b08  ret
    /// ```
    ///
    /// 즉 byte 단위 lexicographic compare from offset 0 → offset 15 (memcmp 의 `<`).
    /// 통상적 UUID lexicographic order (high-byte-first), Windows `IsEqualGUID` / RFC 4122 와 일관.
    pub fn lt_guid(&self, other: &Guid) -> bool {
        // 16B raw byte 비교. Rust 의 `[u8; 16]::cmp` 가 memcmp 와 동일.
        let s_bytes = self.as_bytes();
        let o_bytes = other.as_bytes();
        s_bytes < o_bytes
    }

    /// 내부: Guid 를 (u64 lo, u64 hi) LE pair 로 본다 (offset 0..7, 8..15).
    fn as_u64_pair(&self) -> (u64, u64) {
        // SAFETY: Guid 가 #[repr(C, align(8))] 16B 이므로 (u64, u64) 와 layout 호환.
        unsafe {
            let ptr = self as *const Guid as *const u64;
            (ptr.read(), ptr.add(1).read())
        }
    }

    /// 16B raw byte view. memcmp 호환 lexicographic 순서 (offset 0 가 high-priority).
    pub fn as_bytes(&self) -> [u8; 16] {
        // SAFETY: repr(C, align(8)) 16B → 직접 byte view 안전.
        unsafe { std::mem::transmute_copy(self) }
    }

    /// `Hnc::Type::Guid::Generator::CreateID()` (libHncFoundation @ `0x11520`).
    ///
    /// raw asm:
    /// ```text
    /// 00011520: mov x0, x8            ; sret slot → x0 (first arg of CoCreateGuid)
    /// 00011524: stp xzr, xzr, [x8]    ; zero 16B initial
    /// 00011528: b 0xa9550             ; tail-call libhsp.dylib._CoCreateGuid
    /// ```
    ///
    /// `_CoCreateGuid` (Windows COM API)는 macOS 한컴 빌드에서 libhsp shim 으로
    /// CoreFoundation 의 `CFUUIDCreate` 를 호출하는 macOS 표준 RFC 4122 v4 UUID
    /// 생성기.
    ///
    /// **byte-equivalence note**: GUID 의 byte 내용은 본질적으로 RNG-driven 이라
    /// 두 run 간 다름. raw 한컴과 본 Rust port 는 "valid v4 UUID 생성" 까지만 동등 —
    /// byte-by-byte 같은 출력은 보장 불가 (raw 도 마찬가지). 본 함수는 (1) 16B
    /// 영역 zero-init, (2) macOS CFUUIDCreate-style v4 UUID 생성.
    ///
    /// 본 구현은 외부 dylib link 없이 `getentropy(2)` (BSD 표준 syscall, macOS 지원)
    /// 으로 16 random bytes 를 받아 RFC 4122 v4 variant bits 적용.
    pub fn create_id() -> Self {
        let mut g = Guid::new();
        // raw 의 zero-init 은 위 Guid::new 가 처리.
        // 그 후 CoCreateGuid 가 16B 영역에 v4 UUID 작성.
        let mut bytes = [0u8; 16];
        unsafe {
            // macOS arc4random_buf(buf, len) — entropy 보장 + non-blocking.
            extern "C" {
                fn arc4random_buf(buf: *mut std::ffi::c_void, nbytes: usize);
            }
            arc4random_buf(bytes.as_mut_ptr() as *mut std::ffi::c_void, 16);
        }
        // RFC 4122 v4 variant bits:
        //   bytes[6] = (bytes[6] & 0x0F) | 0x40   ; version = 4
        //   bytes[8] = (bytes[8] & 0x3F) | 0x80   ; variant = RFC 4122
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;

        // GUID layout: Data1 (4B LE), Data2 (2B LE), Data3 (2B LE), Data4 (8B raw).
        g.data1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        g.data2 = u16::from_le_bytes([bytes[4], bytes[5]]);
        g.data3 = u16::from_le_bytes([bytes[6], bytes[7]]);
        g.data4 = [
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ];
        g
    }

    /// 검증/테스트용: nil UUID (zero-initialized) 인지.
    pub const fn is_nil(&self) -> bool {
        self.data1 == 0
            && self.data2 == 0
            && self.data3 == 0
            && self.data4[0] == 0
            && self.data4[1] == 0
            && self.data4[2] == 0
            && self.data4[3] == 0
            && self.data4[4] == 0
            && self.data4[5] == 0
            && self.data4[6] == 0
            && self.data4[7] == 0
    }
}

impl Default for Guid {
    fn default() -> Self {
        Guid::new()
    }
}

// PartialEq / Eq → raw operator== 의 16B 비교.
impl PartialEq for Guid {
    fn eq(&self, other: &Self) -> bool {
        self.eq_guid(other)
    }
}
impl Eq for Guid {}

// Ord / PartialOrd → raw operator< 의 memcmp 순서.
impl PartialOrd for Guid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Guid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(&other.as_bytes())
    }
}

impl std::hash::Hash for Guid {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Standard UUID display format: 8-4-4-4-12 hex.
        write!(
            f,
            "Guid({{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}})",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7],
        )
    }
}

// 정적 검증
const _: () = assert!(std::mem::size_of::<Guid>() == 16);
const _: () = assert!(std::mem::align_of::<Guid>() == 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizeof_is_16b_8align() {
        assert_eq!(std::mem::size_of::<Guid>(), 16);
        assert_eq!(std::mem::align_of::<Guid>(), 8);
    }

    #[test]
    fn field_offsets() {
        let g = Guid::new();
        let base = &g as *const Guid as usize;
        assert_eq!(&g.data1 as *const u32 as usize - base, 0x0);
        assert_eq!(&g.data2 as *const u16 as usize - base, 0x4);
        assert_eq!(&g.data3 as *const u16 as usize - base, 0x6);
        assert_eq!(g.data4.as_ptr() as usize - base, 0x8);
    }

    #[test]
    fn new_zero_init() {
        let g = Guid::new();
        assert_eq!(g.data1, 0);
        assert_eq!(g.data2, 0);
        assert_eq!(g.data3, 0);
        assert_eq!(g.data4, [0u8; 8]);
        assert!(g.is_nil());
    }

    #[test]
    fn copy_clones_16b() {
        let a = Guid {
            data1: 0xDEADBEEF,
            data2: 0xCAFE,
            data3: 0xBABE,
            data4: [1, 2, 3, 4, 5, 6, 7, 8],
        };
        let b = Guid::copy_from(&a);
        assert_eq!(a, b);
    }

    #[test]
    fn eq_compares_16b() {
        let a = Guid {
            data1: 0x12345678,
            data2: 0x9ABC,
            data3: 0xDEF0,
            data4: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22],
        };
        let b = a;
        assert!(a.eq_guid(&b));
        assert!(!a.ne_guid(&b));
    }

    #[test]
    fn eq_detects_difference_in_any_byte() {
        let a = Guid::new();
        for i in 0..16 {
            let mut bytes = [0u8; 16];
            bytes[i] = 0xFF;
            // SAFETY: 16B raw byte → Guid 변환 safe (repr).
            let b: Guid = unsafe { std::mem::transmute(bytes) };
            assert!(!a.eq_guid(&b), "differ at byte {}", i);
            assert!(a.ne_guid(&b));
        }
    }

    #[test]
    fn lt_memcmp_lexicographic_order() {
        // memcmp 의 byte-by-byte 비교 — first byte 가 priority.
        let zero = Guid::new();
        let one_first_byte = Guid {
            data1: 0x0000_0001,
            ..Guid::new()
        };
        // data1 = 1 → LE bytes [01, 00, 00, 00]. byte 0 of zero = 0, byte 0 of one = 1. one > zero.
        // Actually wait: data1 = 0x00000001 LE → memory layout [01, 00, 00, 00]. byte[0]=0x01.
        // zero.bytes[0] = 0x00. So zero < one.
        assert!(zero.lt_guid(&one_first_byte));
        assert!(!one_first_byte.lt_guid(&zero));
        assert!(!zero.lt_guid(&zero));
    }

    #[test]
    fn lt_byte0_priority_over_byte15() {
        // byte 0 가 더 priority — raw asm 의 first ldr+rev 가 lo half 를 먼저 비교하기 때문.
        let a = Guid {
            data1: 0x0000_0002,                  // byte[0]=0x02
            data4: [0, 0, 0, 0, 0, 0, 0, 0xFF],   // byte[15]=0xFF
            ..Guid::new()
        };
        let b = Guid {
            data1: 0x0000_0003,                  // byte[0]=0x03 (more)
            data4: [0, 0, 0, 0, 0, 0, 0, 0x00],   // byte[15]=0x00 (less)
            ..Guid::new()
        };
        // memcmp: a.byte[0]=0x02 < b.byte[0]=0x03 → a < b (byte 0 wins).
        assert!(a.lt_guid(&b));
        assert!(!b.lt_guid(&a));
    }

    #[test]
    fn lt_self_equal_returns_false() {
        let a = Guid {
            data1: 0xABCDEF01,
            data2: 0x1234,
            data3: 0x5678,
            data4: [9, 10, 11, 12, 13, 14, 15, 16],
        };
        assert!(!a.lt_guid(&a));
    }

    #[test]
    fn lt_diff_in_hi_half() {
        // lo half (offset 0..7) 동일, hi half (offset 8..15) 차이.
        let a = Guid {
            data1: 0x1111_2222,
            data2: 0x3333,
            data3: 0x4444,
            data4: [0, 0, 0, 0, 0, 0, 0, 0],
        };
        let b = Guid {
            data1: 0x1111_2222,
            data2: 0x3333,
            data3: 0x4444,
            data4: [1, 0, 0, 0, 0, 0, 0, 0], // byte[8] = 1
        };
        assert!(a.lt_guid(&b));
        assert!(!b.lt_guid(&a));
    }

    #[test]
    fn ord_consistency() {
        let a = Guid {
            data1: 1,
            ..Guid::new()
        };
        let b = Guid {
            data1: 2,
            ..Guid::new()
        };
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Less);
        assert_eq!(b.cmp(&a), std::cmp::Ordering::Greater);
        assert_eq!(a.cmp(&a), std::cmp::Ordering::Equal);
    }

    #[test]
    fn debug_format_uuid_style() {
        let g = Guid {
            data1: 0x12345678,
            data2: 0x9ABC,
            data3: 0xDEF0,
            data4: [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        };
        let s = format!("{:?}", g);
        assert_eq!(s, "Guid({12345678-9abc-def0-1122-334455667788})");
    }

    #[test]
    fn is_nil_distinguishes_zero_from_nonzero() {
        assert!(Guid::new().is_nil());
        assert!(!Guid {
            data1: 1,
            ..Guid::new()
        }
        .is_nil());
        assert!(!Guid {
            data4: [0, 0, 0, 0, 0, 0, 0, 1],
            ..Guid::new()
        }
        .is_nil());
    }

    #[test]
    fn as_bytes_matches_le_layout() {
        let g = Guid {
            data1: 0x04030201,  // LE: [01, 02, 03, 04]
            data2: 0x0605,       // LE: [05, 06]
            data3: 0x0807,       // LE: [07, 08]
            data4: [0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10],
        };
        let bytes = g.as_bytes();
        assert_eq!(bytes, [
            0x01, 0x02, 0x03, 0x04,  // data1 LE
            0x05, 0x06,              // data2 LE
            0x07, 0x08,              // data3 LE
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
        ]);
    }
}
