//! `Hnc::Shape::Text::Hit` — POD hit-test result, in/out param to `Pick(... Hit&, int)`.
//!
//! 위치: `libHncDrawingEngine_arm64.dylib`. ctor/dtor 가 export 되지 않음 — POD-like with
//! inline default initialization.
//!
//! # Sizeof + layout 도출 근거 (raw asm 분석)
//!
//! `__ZN3Hnc5Shape4Text12CharItemView4PickERKNS1_10AllocationEPKNS0_5ThemeERNS1_3HitEi`
//! (@ 0x2f9a34) 의 raw asm 정독 결과 — x19 = Hit& (= x3 of caller frame). 관찰된 접근:
//!
//! ```text
//! 002f9c10  ldp   s1, s0, [x19]        // READ  : 2 × f32 from offsets 0x00 / 0x04 (hit point in)
//! 002f9c5c  strb  wzr, [x19, #0x10]    // WRITE : byte 0 at offset 0x10
//! 002f9cd0  strb  w8,  [x19, #0x10]    // WRITE : computed byte at offset 0x10
//! 002f9cd4  str   x20, [x19, #0x8]     // WRITE : 8B ptr at offset 0x08 (x20 = CharItemView* self)
//! ```
//!
//! 따라서 직접 관찰된 fields:
//! - **offset 0x00**: `f32` — INPUT  : hit-test 좌표 X (Hancom client space)
//! - **offset 0x04**: `f32` — INPUT  : hit-test 좌표 Y
//! - **offset 0x08**: `Glyph*` (8B ptr) — OUTPUT : hit 된 leaf glyph (CharItemView 의 self)
//! - **offset 0x10**: `bool`  (1B) — OUTPUT : leading/trailing edge flag
//!                                            (raw 의 `w8 = (cmp/orn 결과) & 0x1` 의 산출물)
//!
//! offset 0x11..0x17 는 본 dylib 의 export 함수들에서 직접 접근이 관찰되지 않음 — 단,
//! 자연정렬 (8B alignment) 을 만족시키기 위해 offset 0x10 의 bool 뒤로 padding/추가 슬롯이 있음.
//!
//! 안전한 sizeof: 8B-aligned 인 ptr field 0x08 가 있으므로 align = 8B. 마지막 byte field 가
//! 0x10 이고 alignment 8B 가 가산되면 최소 sizeof = 0x18 = **24B**.
//!
//! # Rust 매핑
//!
//! C++ 원본은 ctor 가 export 되지 않으므로 caller 가 stack 에 sub sp, sp, sizeof(Hit) 만 잡고
//! 직접 fields 를 채워서 Pick 에 전달함 (in-place initialization). Rust 에서는 `Hit::new_at`
//! constructor 로 hit point 를 지정하고, Pick 호출 후 outputs 를 읽도록 노출.
//!
//! `#[repr(C)]` 로 C++ ABI 와 동일한 layout 보장. Padding 은 explicit field 로 채우지 않고
//! Rust compiler 의 자동 padding 에 위임 (align(8) 으로 sizeof 24B 보장).
//!
//! # 주의 — Glyph* 표현
//!
//! `pub leaf: *const ()` 로 정의 — 임의 Glyph trait object 를 가리킬 수 없는 raw pointer.
//! kdsnr-layout 의 `Glyph` trait 은 `dyn` trait object (16B fat pointer) 이므로 byte-equivalent
//! 가 깨질 수 있음. R-5 (Pick 포팅) 단계에서 `Box<dyn Glyph>` 의 `*const ()` 변환 정책 결정 필요.
//! 본 단계는 layout 만 확정 — 사용자 코드는 raw u64 로 다루도록 권장.

#![allow(clippy::module_name_repetitions)]

/// `Hnc::Shape::Text::Hit` — 24B POD with 8B alignment.
///
/// In/out parameter to `Glyph::Pick(Allocation&, Theme*, Hit&, int) -> bool`.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    /// offset 0x00 — f32, hit-test X coordinate (INPUT).
    pub x: f32,
    /// offset 0x04 — f32, hit-test Y coordinate (INPUT).
    pub y: f32,
    /// offset 0x08 — 8B Glyph* (OUTPUT). `0` (null) when no hit.
    ///
    /// raw asm 에서는 `str x20, [x19, #0x8]` 로 직접 8B store. Rust 에서는 `*const ()` (8B) 로
    /// 정렬 보존. R-5 단계에서 Glyph trait object 와의 mapping 정책 확정.
    pub leaf: *const (),
    /// offset 0x10 — 1B bool (OUTPUT). leading/trailing edge flag.
    /// raw 의 `strb` 산출물 (mask `& 0x1`).
    pub flag: bool,
    // offset 0x11..0x17 — automatic padding (Rust compiler) to satisfy 8B alignment.
}

impl Hit {
    /// Construct a Hit with input point coordinates, zero outputs.
    ///
    /// 한컴 원본은 ctor 가 inline (export 없음) — caller 가 stack 에 직접 fill. Rust 의 명시적
    /// constructor 는 이 패턴을 미러링.
    pub const fn new_at(x: f32, y: f32) -> Self {
        Hit {
            x,
            y,
            leaf: std::ptr::null(),
            flag: false,
        }
    }

    /// Zero-initialized Hit (all fields 0/false/null).
    pub const fn zero() -> Self {
        Hit {
            x: 0.0,
            y: 0.0,
            leaf: std::ptr::null(),
            flag: false,
        }
    }
}

impl Default for Hit {
    fn default() -> Self {
        Hit::zero()
    }
}

// ===== sizeof 정적 검증 =====
//
// 24B layout 보장 — `#[repr(C, align(8))]` + 자연 padding.
const _: () = assert!(std::mem::size_of::<Hit>() == 24, "Hit must be 24B");
const _: () = assert!(std::mem::align_of::<Hit>() == 8, "Hit must be 8B aligned");

// SAFETY: Hit 의 leaf 가 *const () 라서 기본적으로 !Send/!Sync 이지만, raw POD 으로 다룰 때만
// 사용되고 ownership semantics 가 없으므로 명시적으로 marker 가능. R-5 단계에서 thread safety
// 정책 확정 필요. 본 단계는 marker 미부여.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizeof_is_24b() {
        assert_eq!(std::mem::size_of::<Hit>(), 24);
        assert_eq!(std::mem::align_of::<Hit>(), 8);
    }

    #[test]
    fn field_offsets_match_raw_asm() {
        // raw asm 에서 관찰된 offsets:
        let h = Hit::zero();
        let base = &h as *const Hit as usize;
        let off_x = &h.x as *const f32 as usize - base;
        let off_y = &h.y as *const f32 as usize - base;
        let off_leaf = &h.leaf as *const *const () as usize - base;
        let off_flag = &h.flag as *const bool as usize - base;

        assert_eq!(off_x, 0x00, "x at offset 0");
        assert_eq!(off_y, 0x04, "y at offset 4");
        assert_eq!(off_leaf, 0x08, "leaf at offset 8");
        assert_eq!(off_flag, 0x10, "flag at offset 16 (0x10)");
    }

    #[test]
    fn new_at_initializes_input_zero_output() {
        let h = Hit::new_at(1.5, -2.25);
        assert_eq!(h.x, 1.5);
        assert_eq!(h.y, -2.25);
        assert!(h.leaf.is_null());
        assert!(!h.flag);
    }

    #[test]
    fn zero_default_consistent() {
        let a = Hit::zero();
        let b = Hit::default();
        assert_eq!(a.x, b.x);
        assert_eq!(a.y, b.y);
        assert_eq!(a.leaf, b.leaf);
        assert_eq!(a.flag, b.flag);
    }

    #[test]
    fn output_fields_writeable() {
        let mut h = Hit::new_at(0.0, 0.0);
        // Pick 의 output 시뮬레이션:
        let dummy_glyph: usize = 0xDEAD_BEEF_CAFE_BABE;
        h.leaf = dummy_glyph as *const ();
        h.flag = true;
        assert_eq!(h.leaf as usize, dummy_glyph);
        assert!(h.flag);
    }
}
