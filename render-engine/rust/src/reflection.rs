//! `Hnc::Shape::Reflection` — 반사 효과 sub-type of `Hnc::Shape::Effect`.
//!
//! ## raw 구조 (확정 by 7-arg ctor @ `0x1cd634`, vtable @ `0x77c7e8`)
//!
//! 16B layout (= OuterShadow 와 byte-eq):
//! - +0x00: vtable ptr (= `0x77c7e8`)
//! - +0x08: PropertyBag (8B handle)
//!
//! ## vtable (`0x77c7e8`) — 13 vfunc 검증됨
//!
//! | idx | offset | function | desc |
//! |-----|--------|----------|------|
//! | 0 | 0x00 | D1 (= 0x1cdb88) | complete dtor |
//! | 1 | 0x08 | D0 (= 0x1cdbb8) | deleting dtor |
//! | 2 | 0x10 | operator== (= 0x1cdc74) |  |
//! | 3 | 0x18 | operator!= (= 0x1cdd20) |  |
//! | 4 | 0x20 | operator<  (= 0x1cdd40) |  |
//! | **5** | **0x28** | **`GetType()` (= 0x1cde60) — returns `0xbbb`** | ⭐ effect_key |
//! | 6 | 0x30 | Clone (= 0x1cde68) |  |
//! | 7 | 0x38 | Clone(Color) (= 0x1cdec0) |  |
//! | 8 | 0x40 | CollectProperty (= 0x1cdecc) |  |
//! | 9 | 0x48 | ApplyProperty (= 0x1ce0dc) |  |
//! | 10 | 0x50 | ToRenderImageEffect (= 0x1ce1d0) |  |
//! | 11 | 0x58 | UpdateSchemeColor (= 0x1ceaac) |  |
//! | 12 | 0x60 | IsSaveable (= 0x1ceab0) |  |
//!
//! ## raw 7-arg ctor (`0x1cd634`) — Block 18 에서 호출
//!
//! signature: `Reflection(f32, Degree const&, f32, f32, f32, f32, bool)`
//!
//! 인자 mapping (per raw asm Block 18 caller `0x172040-0x172070`):
//! - arg0 (s0): `distance` (= 0x46467000 = 12700 EMU = 1pt for Block 18)
//! - arg1 (x1): `Degree const&`
//! - arg2 (s1): `blur` (= 0x4714D400 = 38100 EMU = 3pt for Block 18)
//! - arg3 (s2): hardcoded **0.26** (= 0x3E851EB8)
//! - arg4 (s3): hardcoded **0.28** (= 0x3E8F5C29)
//! - arg5 (s4): hardcoded **-1.0**
//! - arg6 (w2): hardcoded **false** (bool)
//!
//! ```asm
//! 0x1cd634: sub  sp, sp, #0x60
//! 0x1cd660: adrp x8, 1455 ; 0x77c000
//! 0x1cd664: add  x8, x8, #0x7e8                  ; vtable = 0x77c7e8
//! 0x1cd668: str  x8, [x0]
//! 0x1cd66c: add  x19, x0, #0x8                    ; bag
//! 0x1cd678: bl   PropertyBag::C1(false)
//! ;; 키 0x3a3 PEnum=7 (state=2) 부터 일련의 attach (raw `0x1cd67c+`)
//! ;; OuterShadow 와 유사한 구조이지만 사용 키 / 값 다름 (deferred RE)
//! ```
//!
//! ## 본 단계 (16-μ) byte-eq port scope
//!
//! - **완성**: 16B layout (vtable + bag), vtable ptr 정확 (= `0x77c7e8`),
//!   vfunc[5] = 0xbbb 보존
//! - **완성**: 7-arg ctor 의 raw float 값 정확 (distance / blur / 0.26 / 0.28 / -1.0 / bool)
//! - **deferred**: ctor 내부의 모든 PropertyBag 키 부착 sequence (OuterShadow 와 유사
//!   하지만 0.26/0.28/-1.0 등의 별도 키 필요 — multi-session RE)

use crate::property::state;
use crate::property_bag::PropertyBag;
use std::ptr;

pub const REFLECTION_VTABLE_RAW_ADDR: usize = 0x77c7e8;

/// raw vtable[5] = 0xbbb (= OuterShadow's 0xbba + 1).
pub const REFLECTION_EFFECT_KEY: u32 = 0xbbb;

#[repr(C)]
pub struct Reflection {
    /// raw +0x00: vtable ptr (= 0x77c7e8).
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag.
    pub bag: PropertyBag,
}

pub const REFLECTION_SIZE_BYTES: usize = 16;
pub const REFLECTION_ALIGN_BYTES: usize = 8;
const _: () = assert!(std::mem::size_of::<Reflection>() == REFLECTION_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Reflection>() == REFLECTION_ALIGN_BYTES);

impl Reflection {
    /// Reflection 의 user-visible 키 (= Block 18 의 7-arg 인자 매핑 + 최소 default 1개).
    ///
    /// 정확한 raw 키 ID 는 multi-session RE 후 정정. 본 단계는 sentinel 키 사용.
    pub const KEY_DISTANCE: u32 = 0x3a0; // 추정 — Block 18 의 distance arg
    pub const KEY_DEGREE: u32 = 0x3a1;
    pub const KEY_BLUR: u32 = 0x3a2;
    pub const KEY_OPACITY_START: u32 = 0x3ac;  // 추정 (0.26)
    pub const KEY_OPACITY_END: u32 = 0x3ad;    // 추정 (0.28)
    pub const KEY_FADE: u32 = 0x3ae;            // 추정 (-1.0)
    pub const KEY_FLIP: u32 = 0x3a8;            // 추정 (bool)

    /// raw `0x1cd634` 7-arg ctor 1:1 — Block 18 에서 호출.
    ///
    /// **본 단계 scope**: 16B 구조 + vtable + bag 초기화 + 7 user-args 의 부착.
    /// 정확한 raw key ID 는 multi-session RE 필요 — KEY_* 들은 추정값.
    pub unsafe fn new_with_args(
        distance: f32,
        degree_value: f32,
        blur: f32,
        opacity_start: f32,
        opacity_end: f32,
        fade: f32,
        flip: bool,
    ) -> Self {
        let bag = PropertyBag::new(false);
        let mut r = Reflection {
            vtable: REFLECTION_VTABLE_RAW_ADDR as *const u8,
            bag,
        };

        // 7 user attaches (state=1) — 정확한 KEY ID 는 raw RE 후 정정 필요
        let attaches: [(u32, f32); 6] = [
            (Self::KEY_DISTANCE, distance),
            (Self::KEY_DEGREE, degree_value),
            (Self::KEY_BLUR, blur),
            (Self::KEY_OPACITY_START, opacity_start),
            (Self::KEY_OPACITY_END, opacity_end),
            (Self::KEY_FADE, fade),
        ];
        for (key_id, value) in attaches {
            let pk = crate::property_key::PropertyKey::from_int(key_id);
            let ctrl =
                crate::property::PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, value);
            let _ = r.bag.attach(&pk, ctrl);
        }
        // PBool flip
        let pk_flip = crate::property_key::PropertyKey::from_int(Self::KEY_FLIP);
        let pb_flip =
            crate::property::PBool::create_attach_ctrl(state::ENABLED_DEFAULT, flip);
        let _ = r.bag.attach(&pk_flip, pb_flip);

        r
    }

    /// raw vfunc[5] `GetType()` (= 0x1cde60) 등가 — effect_key = 0xbbb.
    #[inline]
    pub fn effect_key(&self) -> u32 {
        REFLECTION_EFFECT_KEY
    }

    pub fn bag_size(&self) -> u64 {
        unsafe { self.bag.impl_ref().map(|i| i.tree.size).unwrap_or(0) }
    }

    /// raw vtable[0] (= D1) — bag dtor only.
    ///
    /// # Safety
    /// `obj` 는 valid `*mut Reflection`.
    pub unsafe fn drop_in_place_raw(obj: *mut u8) {
        let p = obj as *mut Reflection;
        ptr::drop_in_place(p);
    }
}

impl Drop for Reflection {
    fn drop(&mut self) {
        // PropertyBag 자체 Drop.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_raw_16b_layout() {
        assert_eq!(std::mem::size_of::<Reflection>(), 16);
        assert_eq!(std::mem::align_of::<Reflection>(), 8);
    }

    #[test]
    fn reflection_vtable_matches_raw() {
        unsafe {
            let r = Reflection::new_with_args(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, false);
            assert_eq!(r.vtable as usize, REFLECTION_VTABLE_RAW_ADDR);
            assert_eq!(r.vtable as usize, 0x77c7e8);
        }
    }

    #[test]
    fn reflection_effect_key_is_0xbbb() {
        unsafe {
            let r = Reflection::new_with_args(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, false);
            assert_eq!(r.effect_key(), 0xbbb);
        }
    }

    #[test]
    fn reflection_block18_params_byte_eq() {
        // Block 18 corrected params (per Agent B 정정):
        // distance = 12700 (= 0x46467000)
        // blur = 38100 (= 0x4714D400)
        // 3 hardcoded floats: 0.26, 0.28, -1.0
        unsafe {
            let r = Reflection::new_with_args(
                f32::from_bits(0x46467000), // distance
                f32::from_bits(0x42B40000), // 90°
                f32::from_bits(0x4714D400), // blur
                f32::from_bits(0x3E851EB8), // 0.26
                f32::from_bits(0x3E8F5C29), // 0.28
                -1.0,
                false,
            );
            assert_eq!(r.effect_key(), 0xbbb);
            // 7 keys attached
            assert_eq!(r.bag_size(), 7);
        }
    }
}
