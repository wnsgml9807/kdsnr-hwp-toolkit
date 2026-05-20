//! `Hnc::Shape::BWMode` — 4B u32 enum, w/ raw lookup tables for fill/outline conversion.
//!
//! 위치: `libHncDrawingEngine_arm64.dylib`
//! - `ShapeRenderConverter::ToFillRenderMode(BWMode)`    @ 0x1b9368
//! - `ShapeRenderConverter::ToOutlineRenderMode(BWMode)` @ 0x1b9408
//! - Lookup tables (each 11 × u32):
//!   - `0x7508a0` — fill table (file offset = vaddr; `__TEXT.__const` 의 base 0x740f50 보다 위, 동일 매핑)
//!   - `0x750874` — outline table
//!
//! # Raw asm — `ToFillRenderMode`
//!
//! ```text
//! 001b9368  sub   w8, w0, #0x2                    // w8 = BWMode - 2
//! 001b936c  cmp   w8, #0xa                        // (BWMode - 2) > 0xa ?
//! 001b9370  b.hi  0x1b9384                        // out-of-range → return 0
//! 001b9374  adrp  x9, 0x750000
//! 001b9378  add   x9, x9, #0x8a0                  // x9 = 0x7508a0
//! 001b937c  ldr   w0, [x9, w8, sxtw #2]           // w0 = table[(BWMode-2) * 4]
//! 001b9380  ret
//! 001b9384  mov   w0, #0x0
//! 001b9388  ret
//! ```
//!
//! # Raw asm — `ToOutlineRenderMode`
//!
//! ```text
//! 001b9408  sub   w8, w0, #0x2
//! 001b940c  cmp   w8, #0xa
//! 001b9410  b.hi  0x1b9424
//! 001b9414  adrp  x9, 0x750000
//! 001b9418  add   x9, x9, #0x874                  // x9 = 0x750874
//! 001b941c  ldr   w0, [x9, w8, sxtw #2]
//! 001b9420  ret
//! 001b9424  mov   w0, #0x0
//! 001b9428  ret
//! ```
//!
//! # Lookup table 값 (raw 파일 dump 으로 확인)
//!
//! ```text
//! BWMode=2:  Fill=1   Outline=1
//! BWMode=3:  Fill=2   Outline=2
//! BWMode=4:  Fill=3   Outline=3
//! BWMode=5:  Fill=4   Outline=4
//! BWMode=6:  Fill=5   Outline=4
//! BWMode=7:  Fill=1   Outline=6
//! BWMode=8:  Fill=5   Outline=6
//! BWMode=9:  Fill=6   Outline=6
//! BWMode=10: Fill=5   Outline=5
//! BWMode=11: Fill=0   Outline=0
//! BWMode=12: Fill=0   Outline=0
//! ```
//!
//! BWMode 0/1 및 13+ 은 lookup 범위 밖 → 항상 0. enum 값의 의미 (어떤 BWMode 가 무엇인지) 는
//! 별도 caller RE 필요 — 본 단계에서는 raw value 전송만 보장.

#![allow(non_camel_case_types)]

/// `Hnc::Shape::BWMode` — 4B u32. by-value (w0 register) 로 전달.
///
/// 11 valid values 가 lookup table 에 매핑됨: BWMode ∈ [2..12]. 다른 값은 lookup 결과 0.
///
/// 구체적 enum 값의 의미 (예: BWMode=2 가 "AsItIs" 인지, "FullColor" 인지 등) 는 본 단계에서
/// 미확정 — caller RE 가 필요. `repr(u32)` + 명시 디스크리미넌트로 raw 값 보존.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BWMode {
    /// BWMode = 0 — lookup 범위 밖, 항상 RenderMode 0.
    V0 = 0,
    /// BWMode = 1 — lookup 범위 밖.
    V1 = 1,
    /// BWMode = 2 — Fill=1, Outline=1.
    V2 = 2,
    /// BWMode = 3 — Fill=2, Outline=2.
    V3 = 3,
    /// BWMode = 4 — Fill=3, Outline=3.
    V4 = 4,
    /// BWMode = 5 — Fill=4, Outline=4.
    V5 = 5,
    /// BWMode = 6 — Fill=5, Outline=4.
    V6 = 6,
    /// BWMode = 7 — Fill=1, Outline=6.
    V7 = 7,
    /// BWMode = 8 — Fill=5, Outline=6.
    V8 = 8,
    /// BWMode = 9 — Fill=6, Outline=6.
    V9 = 9,
    /// BWMode = 10 — Fill=5, Outline=5.
    V10 = 10,
    /// BWMode = 11 — Fill=0, Outline=0.
    V11 = 11,
    /// BWMode = 12 — Fill=0, Outline=0.
    V12 = 12,
}

impl BWMode {
    /// Convert from raw u32 (e.g. from C++ caller). Returns `None` for unknown values.
    pub const fn from_u32(v: u32) -> Option<BWMode> {
        match v {
            0 => Some(BWMode::V0),
            1 => Some(BWMode::V1),
            2 => Some(BWMode::V2),
            3 => Some(BWMode::V3),
            4 => Some(BWMode::V4),
            5 => Some(BWMode::V5),
            6 => Some(BWMode::V6),
            7 => Some(BWMode::V7),
            8 => Some(BWMode::V8),
            9 => Some(BWMode::V9),
            10 => Some(BWMode::V10),
            11 => Some(BWMode::V11),
            12 => Some(BWMode::V12),
            _ => None,
        }
    }

    /// Raw u32 value.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// `Hnc::Shape::RenderMode` — 4B u32, return type of ToFill/ToOutlineRenderMode.
///
/// 값 0..6 만 lookup table 에서 관찰됨. 의미 (Fill / Outline / Image / Gradient 등) 는 추가 RE 필요.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum RenderMode {
    /// 0 — invalid / out-of-range BWMode 의 default. 또한 BWMode=11/12 도 매핑.
    V0 = 0,
    /// 1
    V1 = 1,
    /// 2
    V2 = 2,
    /// 3
    V3 = 3,
    /// 4
    V4 = 4,
    /// 5
    V5 = 5,
    /// 6
    V6 = 6,
}

impl RenderMode {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    pub const fn from_u32(v: u32) -> RenderMode {
        match v {
            1 => RenderMode::V1,
            2 => RenderMode::V2,
            3 => RenderMode::V3,
            4 => RenderMode::V4,
            5 => RenderMode::V5,
            6 => RenderMode::V6,
            _ => RenderMode::V0,
        }
    }
}

// ===== Lookup tables =====
//
// Raw dump from `libHncDrawingEngine_arm64.dylib` at file offset = virtual address:
//
//   $ python3 -c "
//   with open('libHncDrawingEngine_arm64.dylib','rb') as f:
//       f.seek(0x7508a0)
//       print([int.from_bytes(f.read(4),'little') for _ in range(11)])
//   "
//   → [1, 2, 3, 4, 5, 1, 5, 6, 5, 0, 0]   # fill
//   → [1, 2, 3, 4, 4, 6, 6, 6, 5, 0, 0]   # outline (offset 0x750874)

/// `ToFillRenderMode` lookup table @ 0x7508a0, 11 × u32.
/// Index = BWMode - 2 (valid for BWMode ∈ [2..12]).
const FILL_TABLE: [u32; 11] = [
    1, // BWMode=2  → 1
    2, // BWMode=3  → 2
    3, // BWMode=4  → 3
    4, // BWMode=5  → 4
    5, // BWMode=6  → 5
    1, // BWMode=7  → 1
    5, // BWMode=8  → 5
    6, // BWMode=9  → 6
    5, // BWMode=10 → 5
    0, // BWMode=11 → 0
    0, // BWMode=12 → 0
];

/// `ToOutlineRenderMode` lookup table @ 0x750874, 11 × u32.
const OUTLINE_TABLE: [u32; 11] = [
    1, // BWMode=2  → 1
    2, // BWMode=3  → 2
    3, // BWMode=4  → 3
    4, // BWMode=5  → 4
    4, // BWMode=6  → 4
    6, // BWMode=7  → 6
    6, // BWMode=8  → 6
    6, // BWMode=9  → 6
    5, // BWMode=10 → 5
    0, // BWMode=11 → 0
    0, // BWMode=12 → 0
];

/// `Hnc::Shape::ShapeRenderConverter::ToFillRenderMode(BWMode)` @ 0x1b9368.
///
/// raw flow:
/// - `w8 = w0 - 2`; if `w8 > 0xa` → return 0;
/// - else `return table_at_0x7508a0[w8]`
///
/// Rust 의 `u32` 의 wrapping sub 가 raw `sub w8, w0, 2` 와 일치하지만, BWMode 가 0/1 일 때
/// w8 가 큰 음수 (signed) = 큰 unsigned 가 되어 `cmp w8, 0xa; b.hi` 가 정확히 out-of-range 로 분기.
/// 본 함수는 그 분기를 정확히 재현 — `bw_raw < 2 || bw_raw > 12` 모두 0 반환.
pub fn to_fill_render_mode(bw: BWMode) -> RenderMode {
    let bw_raw = bw.as_u32();
    // 0x1b9368: sub w8, w0, #2 — wrapping sub on u32.
    let idx = bw_raw.wrapping_sub(2);
    // 0x1b936c-0x1b9370: cmp w8, 0xa ; b.hi 0x1b9384 — branch if unsigned > 10.
    if idx > 0xa {
        // 0x1b9384: mov w0, 0
        return RenderMode::V0;
    }
    // 0x1b937c: ldr w0, [x9, w8, sxtw #2] — sign-extended shift-left 2.
    // idx 는 항상 [0..10] 범위 이므로 sxtw 의 signed extension 은 영향 없음.
    let raw = FILL_TABLE[idx as usize];
    RenderMode::from_u32(raw)
}

/// `Hnc::Shape::ShapeRenderConverter::ToOutlineRenderMode(BWMode)` @ 0x1b9408.
///
/// Same structure as ToFillRenderMode, with `OUTLINE_TABLE` (base 0x750874).
pub fn to_outline_render_mode(bw: BWMode) -> RenderMode {
    let bw_raw = bw.as_u32();
    let idx = bw_raw.wrapping_sub(2);
    if idx > 0xa {
        return RenderMode::V0;
    }
    let raw = OUTLINE_TABLE[idx as usize];
    RenderMode::from_u32(raw)
}

/// Raw-u32 variant (전달 받는 caller 가 BWMode enum 으로 안전 변환 못 하는 경우 — out-of-range
/// 값 0/1/13+ 도 raw asm 의 정확한 동작 재현). Lookup 분기까지 raw asm 과 byte-equivalent.
pub fn to_fill_render_mode_u32(bw_raw: u32) -> u32 {
    let idx = bw_raw.wrapping_sub(2);
    if idx > 0xa {
        return 0;
    }
    FILL_TABLE[idx as usize]
}

pub fn to_outline_render_mode_u32(bw_raw: u32) -> u32 {
    let idx = bw_raw.wrapping_sub(2);
    if idx > 0xa {
        return 0;
    }
    OUTLINE_TABLE[idx as usize]
}

// ===== sizeof 정적 검증 =====
const _: () = assert!(std::mem::size_of::<BWMode>() == 4, "BWMode must be u32-sized");
const _: () = assert!(std::mem::size_of::<RenderMode>() == 4, "RenderMode must be u32-sized");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bwmode_size_is_u32() {
        assert_eq!(std::mem::size_of::<BWMode>(), 4);
        assert_eq!(std::mem::size_of::<RenderMode>(), 4);
    }

    #[test]
    fn bwmode_as_u32_roundtrip() {
        for v in 0u32..=12 {
            let bw = BWMode::from_u32(v).expect("valid");
            assert_eq!(bw.as_u32(), v);
        }
        assert!(BWMode::from_u32(13).is_none());
        assert!(BWMode::from_u32(u32::MAX).is_none());
    }

    // ---- raw lookup table ground truth (from dylib dump) ----
    #[test]
    fn fill_table_matches_raw_dump() {
        let expected: [u32; 11] = [1, 2, 3, 4, 5, 1, 5, 6, 5, 0, 0];
        assert_eq!(FILL_TABLE, expected);
    }

    #[test]
    fn outline_table_matches_raw_dump() {
        let expected: [u32; 11] = [1, 2, 3, 4, 4, 6, 6, 6, 5, 0, 0];
        assert_eq!(OUTLINE_TABLE, expected);
    }

    // ---- ToFillRenderMode raw asm equivalence ----
    #[test]
    fn fill_bwmode_2_returns_1() {
        assert_eq!(to_fill_render_mode(BWMode::V2), RenderMode::V1);
    }

    #[test]
    fn fill_bwmode_6_returns_5() {
        assert_eq!(to_fill_render_mode(BWMode::V6), RenderMode::V5);
    }

    #[test]
    fn fill_bwmode_9_returns_6() {
        assert_eq!(to_fill_render_mode(BWMode::V9), RenderMode::V6);
    }

    #[test]
    fn fill_bwmode_11_returns_0() {
        assert_eq!(to_fill_render_mode(BWMode::V11), RenderMode::V0);
    }

    #[test]
    fn fill_bwmode_0_returns_0_out_of_range() {
        // raw: w8 = 0 - 2 = 0xfffffffe (u32). cmp w8, 0xa → unsigned greater → branch → return 0.
        assert_eq!(to_fill_render_mode(BWMode::V0), RenderMode::V0);
    }

    #[test]
    fn fill_bwmode_1_returns_0_out_of_range() {
        // w8 = 1 - 2 = 0xffffffff. unsigned > 10 → return 0.
        assert_eq!(to_fill_render_mode(BWMode::V1), RenderMode::V0);
    }

    #[test]
    fn fill_u32_variant_handles_out_of_range_13plus() {
        // BWMode = 13: w8 = 11 > 10 → return 0.
        assert_eq!(to_fill_render_mode_u32(13), 0);
        // BWMode = 100: w8 = 98 > 10 → return 0.
        assert_eq!(to_fill_render_mode_u32(100), 0);
        // BWMode = u32::MAX: w8 = u32::MAX-2 > 10 → return 0.
        assert_eq!(to_fill_render_mode_u32(u32::MAX), 0);
    }

    #[test]
    fn fill_u32_variant_all_valid_match_table() {
        let expected: [u32; 11] = [1, 2, 3, 4, 5, 1, 5, 6, 5, 0, 0];
        for i in 0..11u32 {
            assert_eq!(to_fill_render_mode_u32(i + 2), expected[i as usize]);
        }
    }

    // ---- ToOutlineRenderMode ----
    #[test]
    fn outline_bwmode_5_returns_4() {
        assert_eq!(to_outline_render_mode(BWMode::V5), RenderMode::V4);
    }

    #[test]
    fn outline_bwmode_7_returns_6() {
        assert_eq!(to_outline_render_mode(BWMode::V7), RenderMode::V6);
    }

    #[test]
    fn outline_bwmode_10_returns_5() {
        assert_eq!(to_outline_render_mode(BWMode::V10), RenderMode::V5);
    }

    #[test]
    fn outline_bwmode_12_returns_0() {
        assert_eq!(to_outline_render_mode(BWMode::V12), RenderMode::V0);
    }

    #[test]
    fn outline_bwmode_0_1_out_of_range() {
        assert_eq!(to_outline_render_mode(BWMode::V0), RenderMode::V0);
        assert_eq!(to_outline_render_mode(BWMode::V1), RenderMode::V0);
    }

    #[test]
    fn outline_u32_variant_all_valid_match_table() {
        let expected: [u32; 11] = [1, 2, 3, 4, 4, 6, 6, 6, 5, 0, 0];
        for i in 0..11u32 {
            assert_eq!(to_outline_render_mode_u32(i + 2), expected[i as usize]);
        }
    }

    // ---- Cross-check: fill vs outline differ at expected positions ----
    #[test]
    fn fill_outline_differ_at_bwmode_6() {
        // Fill=5, Outline=4
        assert_eq!(to_fill_render_mode(BWMode::V6).as_u32(), 5);
        assert_eq!(to_outline_render_mode(BWMode::V6).as_u32(), 4);
    }

    #[test]
    fn fill_outline_differ_at_bwmode_7() {
        // Fill=1, Outline=6
        assert_eq!(to_fill_render_mode(BWMode::V7).as_u32(), 1);
        assert_eq!(to_outline_render_mode(BWMode::V7).as_u32(), 6);
    }

    #[test]
    fn fill_outline_differ_at_bwmode_8() {
        // Fill=5, Outline=6
        assert_eq!(to_fill_render_mode(BWMode::V8).as_u32(), 5);
        assert_eq!(to_outline_render_mode(BWMode::V8).as_u32(), 6);
    }

    #[test]
    fn fill_outline_same_at_bwmode_2_3_4_5_10_11_12() {
        for &bw in &[BWMode::V2, BWMode::V3, BWMode::V4, BWMode::V5, BWMode::V10, BWMode::V11, BWMode::V12] {
            assert_eq!(to_fill_render_mode(bw), to_outline_render_mode(bw),
                "BWMode={:?}", bw);
        }
    }
}
