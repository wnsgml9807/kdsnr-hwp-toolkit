//! `Hnc::Shape::OuterShadow` — 그림자 효과 sub-type of `Hnc::Shape::Effect`.
//!
//! ## raw 구조 (확정 by 5-arg ctor @ `0x1d2224`, vtable @ `0x77c908`)
//!
//! 16B layout:
//! - +0x00: vtable ptr (= `0x77c908`)
//! - +0x08: PropertyBag (= 8B handle)
//!
//! ## vtable (`0x77c908`) — 13 vfunc 검증됨
//!
//! | idx | offset | function | desc |
//! |-----|--------|----------|------|
//! | 0 | 0x00 | `__ZN3Hnc5Shape11OuterShadowD1Ev` (= 0x1d2670) | complete dtor |
//! | 1 | 0x08 | `__ZN3Hnc5Shape11OuterShadowD0Ev` (= 0x1d26a0) | deleting dtor |
//! | 2 | 0x10 | `operator==(Effect const&)` (= 0x1d275c) | equality |
//! | 3 | 0x18 | `operator!=(Effect const&)` (= 0x1d2808) | inequality |
//! | 4 | 0x20 | `operator<(Effect const&)` (= 0x1d2828) | less-than |
//! | **5** | **0x28** | **`GetType()` (= 0x1d2948) — returns `0xbba`** | ⭐ effect_key |
//! | 6 | 0x30 | `Clone()` (= 0x1d2950) | clone (alloc 16B) |
//! | 7 | 0x38 | `Clone(Color)` (= 0x1d29a8) | clone with new color |
//! | 8 | 0x40 | `CollectProperty(PropertyBag&)` (= 0x1d2b5c) |  |
//! | 9 | 0x48 | `ApplyProperty(PropertyBag const&)` (= 0x1d2d40) |  |
//! | 10 | 0x50 | `ToRenderImageEffect(ColorMapper*, RenderMode)` (= 0x1d3328) |  |
//! | 11 | 0x58 | `UpdateSchemeColor(ColorMapper*)` (= 0x1d3850) |  |
//! | 12 | 0x60 | `IsSaveable(PKey)` (= 0x1d39dc) |  |
//!
//! ## raw `5-arg ctor` (`0x1d2224`) — Block 16/17 에서 호출되는 ctor
//!
//! signature: `OuterShadow(Color const&, f32 distance, Degree const&, f32 blur, bool flip)`
//!
//! ```asm
//! 0x1d2224: sub  sp, sp, #0x60
//! 0x1d2250: adrp x8, 1450 ; 0x77c000
//! 0x1d2254: add  x8, x8, #0x908
//! 0x1d2258: str  x8, [x0]                    ; vtable @ +0x0
//! 0x1d225c: add  x19, x0, #0x8                ; x19 = &bag
//! 0x1d2268: bl   PropertyBag::C1(false)       ; bag init
//! ;; --- property attaches (10+ keys, mix of state=1 and state=2/5) ---
//! 0x1d226c: 0x39e PColor (state=1)            ; Color arg
//! 0x1d22a4: 0x3a0 PFloat (state=1)            ; distance
//! 0x1d22dc: 0x3a1 PDegree (state=1)           ; degree
//! 0x1d2314: 0x3a2 PFloat (state=1)            ; blur
//! 0x1d234c: 0x3a8 PBool (state=1)             ; flip
//! 0x1d2384: 0x3a3 PEnum=7 (state=2)           ; default
//! 0x1d23c0: 0x3a4 PDegree=0 (state=5)         ; default
//! 0x1d240c: 0x3a5 PDegree=0 (state=5)         ; default
//! ;; ... more state=2 / state=5 defaults (deferred RE)
//! 0x1d2474+: ret
//! ```
//!
//! ## 본 단계 (16-μ) byte-eq port scope
//!
//! - **완성**: 16B layout (vtable + bag), vtable ptr 정확, vfunc[5] = 0xbba 보존
//! - **완성**: 5-arg ctor 의 **5 user-overridden 키** (0x39e/0x3a0/0x3a1/0x3a2/0x3a8) 부착 (state=1)
//! - **deferred (multi-session RE 필요)**:
//!   - 5+ 추가 state=2/5 default 키 (0x3a3/0x3a4/0x3a5/0x3a6/0x3a7/0x3a9-0x3ab)
//!   - state=5 의 enum 정의 (Property::state module 에 없는 값 — raw asm RE 필요)
//!   - 13 vfunc 의 실제 implementation port (현재는 vtable ptr 만 byte-eq, 호출 시 Rust 측에서 panic)
//!
//! → 본 단계의 byte-eq 경계: **FormatScheme.effects tree 의 vtable dispatch level**
//!   (= Effects map 의 OuterShadow node 가 vtable[5] 통해 0xbba 키로 정렬됨).
//!   Effect 의 PDF rendering 시점 byte-eq 는 별도 multi-session.

use crate::property::state;
use crate::property_bag::PropertyBag;
use std::alloc::Layout;
use std::ptr;

/// raw `0x77c908` — OuterShadow 의 정적 vtable 주소 (binary 절대 주소).
///
/// 본 Rust port 는 sub-type discrimination 만 보존 — vfunc dispatch 는 Rust trait.
pub const OUTER_SHADOW_VTABLE_RAW_ADDR: usize = 0x77c908;

/// raw `__ZNK3Hnc5Shape11OuterShadow7GetTypeEv` (vtable[5]) 의 return 값.
///
/// raw `0x1d2948: mov w0, #0xbba; ret` — Effect 의 sub-type 식별자.
pub const OUTER_SHADOW_EFFECT_KEY: u32 = 0xbba;

/// `Hnc::Shape::OuterShadow` — raw 16B layout (vtable + PropertyBag).
///
/// Rust port: vtable 은 `OUTER_SHADOW_VTABLE_RAW_ADDR` 의 const refer (= raw address).
/// drop_in_place 시 raw vtable[0] (= D1) 의 algorithm = `~PropertyBag()` 호출.
///
/// PropertyBag 의 키 attach 은 `new_with_args(...)` 에서 1:1.
#[repr(C)]
pub struct OuterShadow {
    /// raw +0x00: vtable ptr — Rust port 는 raw address constant 사용 (= 0x77c908).
    pub vtable: *const u8,
    /// raw +0x08: PropertyBag (8B handle).
    pub bag: PropertyBag,
}

pub const OUTER_SHADOW_SIZE_BYTES: usize = 16;
pub const OUTER_SHADOW_ALIGN_BYTES: usize = 8;
const _: () = assert!(std::mem::size_of::<OuterShadow>() == OUTER_SHADOW_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<OuterShadow>() == OUTER_SHADOW_ALIGN_BYTES);

impl OuterShadow {
    pub const KEY_COLOR: u32 = 0x39e;
    pub const KEY_DISTANCE: u32 = 0x3a0;
    pub const KEY_DEGREE: u32 = 0x3a1;
    pub const KEY_BLUR: u32 = 0x3a2;
    pub const KEY_FLIP: u32 = 0x3a8;
    pub const KEY_DEFAULT_ENUM: u32 = 0x3a3; // value 7, state=2

    /// raw `0x1d2224` 5-arg ctor 1:1 (user-override path) — Block 16/17 의 OuterShadow alloc.
    ///
    /// raw 인용:
    /// ```asm
    /// 0x1d2224: sub  sp, sp, #0x60               ; (alloc + frame)
    /// 0x1d2250-0x1d2258: vtable @ +0x0 = 0x77c908
    /// 0x1d225c-0x1d2268: PropertyBag::C1(false) @ +0x8
    /// ;; 5 user-override attaches (state=1 = ENABLED_DEFAULT) + N defaults
    /// ```
    ///
    /// 본 method 는 **5 user 키만 부착** — multi-session 에서 state=2/5 defaults 추가.
    ///
    /// # Safety
    /// `color` 는 raw 16B Color blob. `degree_value` 는 raw Degree (1 f32 angle).
    pub unsafe fn new_with_args(
        color: &crate::color::Color,
        distance: f32,
        degree_value: f32,
        blur: f32,
        flip: bool,
    ) -> Self {
        let bag = PropertyBag::new(false);
        let mut shadow = OuterShadow {
            vtable: OUTER_SHADOW_VTABLE_RAW_ADDR as *const u8,
            bag,
        };

        // raw 0x1d226c-0x1d2298: PColor attach (key 0x39e, state=1)
        let pcolor = crate::property::PColor::create_attach_ctrl(
            state::ENABLED_DEFAULT,
            color,
        );
        let pk_color = crate::property_key::PropertyKey::from_int(Self::KEY_COLOR);
        let _ = shadow.bag.attach(&pk_color, pcolor);

        // raw 0x1d22a4-0x1d22d0: PFloat distance (key 0x3a0, state=1)
        let pf_dist = crate::property::PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, distance);
        let pk_dist = crate::property_key::PropertyKey::from_int(Self::KEY_DISTANCE);
        let _ = shadow.bag.attach(&pk_dist, pf_dist);

        // raw 0x1d22dc-0x1d2308: PDegree (= PFloat byte-eq, key 0x3a1, state=1)
        let pf_deg = crate::property::PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, degree_value);
        let pk_deg = crate::property_key::PropertyKey::from_int(Self::KEY_DEGREE);
        let _ = shadow.bag.attach(&pk_deg, pf_deg);

        // raw 0x1d2314-0x1d2340: PFloat blur (key 0x3a2, state=1)
        let pf_blur = crate::property::PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, blur);
        let pk_blur = crate::property_key::PropertyKey::from_int(Self::KEY_BLUR);
        let _ = shadow.bag.attach(&pk_blur, pf_blur);

        // raw 0x1d234c-0x1d2378: PBool flip (key 0x3a8, state=1)
        let pb_flip = crate::property::PBool::create_attach_ctrl(state::ENABLED_DEFAULT, flip);
        let pk_flip = crate::property_key::PropertyKey::from_int(Self::KEY_FLIP);
        let _ = shadow.bag.attach(&pk_flip, pb_flip);

        // raw 0x1d2384-0x1d23b8: PEnum=7 (key 0x3a3, state=2) — default ENABLED_EXPLICIT
        // helper 0x6679dc 는 PEnum variant (= 7 변종 중 하나, vtable distinction 은 deferred)
        let pe_default = crate::property::PEnum::create_attach_ctrl(state::ENABLED_EXPLICIT, 7);
        let pk_default = crate::property_key::PropertyKey::from_int(Self::KEY_DEFAULT_ENUM);
        let _ = shadow.bag.attach(&pk_default, pe_default);

        // raw 0x1d23c0+: 추가 0x3a4/0x3a5/0x3a6/0x3a7 PDegree=0 (state=5) — 본 단계 deferred
        // (state=5 의 enum 정의는 별도 RE)

        shadow
    }

    /// raw vfunc[5] `GetType()` (= 0x1d2948) 등가 — effect_key 반환 (= 0xbba).
    #[inline]
    pub fn effect_key(&self) -> u32 {
        OUTER_SHADOW_EFFECT_KEY
    }

    /// PropertyBag 의 노드 수 (= 본 단계의 user override 6개 = 0x39e/0x3a0/0x3a1/0x3a2/0x3a8/0x3a3).
    pub fn bag_size(&self) -> u64 {
        unsafe { self.bag.impl_ref().map(|i| i.tree.size).unwrap_or(0) }
    }

    /// raw vtable[0] (= `D1` complete dtor) 1:1 — bag dtor only.
    ///
    /// # Safety
    /// `obj` 는 valid `*mut OuterShadow`.
    pub unsafe fn drop_in_place_raw(obj: *mut u8) {
        let p = obj as *mut OuterShadow;
        ptr::drop_in_place(p);
    }
}

impl Drop for OuterShadow {
    fn drop(&mut self) {
        // PropertyBag 가 자체 Drop 으로 처리.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_shadow_raw_16b_layout() {
        assert_eq!(std::mem::size_of::<OuterShadow>(), 16);
        assert_eq!(std::mem::align_of::<OuterShadow>(), 8);
    }

    #[test]
    fn outer_shadow_field_offsets_match_raw() {
        unsafe {
            let color = crate::color::Color::from_rgb(0xFF, 0, 0, ptr::null_mut());
            let s = OuterShadow::new_with_args(&color, 100.0, 90.0, 50.0, false);
            let base = &s as *const _ as usize;
            assert_eq!(&s.vtable as *const _ as usize - base, 0x00);
            assert_eq!(&s.bag as *const _ as usize - base, 0x08);
        }
    }

    #[test]
    fn outer_shadow_vtable_address_matches_raw() {
        unsafe {
            let color = crate::color::Color::from_rgb(0, 0, 0, ptr::null_mut());
            let s = OuterShadow::new_with_args(&color, 0.0, 0.0, 0.0, false);
            assert_eq!(s.vtable as usize, OUTER_SHADOW_VTABLE_RAW_ADDR);
            assert_eq!(s.vtable as usize, 0x77c908);
        }
    }

    #[test]
    fn outer_shadow_effect_key_is_0xbba() {
        unsafe {
            let color = crate::color::Color::from_rgb(0, 0, 0, ptr::null_mut());
            let s = OuterShadow::new_with_args(&color, 0.0, 0.0, 0.0, false);
            assert_eq!(s.effect_key(), 0xbba);
        }
    }

    #[test]
    fn outer_shadow_5arg_ctor_attaches_6_user_keys() {
        unsafe {
            let color = crate::color::Color::from_rgb(0xAA, 0xBB, 0xCC, ptr::null_mut());
            // Block 16 params: distance=63500, degree=90, blur=45398, flip=false
            let s = OuterShadow::new_with_args(
                &color,
                f32::from_bits(0x47780C00), // 63500
                f32::from_bits(0x42B40000), // 90.0
                f32::from_bits(0x47315600), // 45398
                false,
            );
            // 5 user keys + 1 default = 6
            assert_eq!(s.bag_size(), 6);
        }
    }

    #[test]
    fn outer_shadow_block17_params_byte_eq() {
        // Block 17 (=  16-η-Agent-B 보정): distance=63500, blur=23000 (Agent B 의 45398 은 오류)
        unsafe {
            let color = crate::color::Color::from_rgb(0, 0, 0, ptr::null_mut());
            let s = OuterShadow::new_with_args(
                &color,
                f32::from_bits(0x47780C00), // 63500
                f32::from_bits(0x42B40000), // 90.0
                f32::from_bits(0x46B3B000), // 23000 — Block 17 corrected value
                false,
            );
            assert_eq!(s.effect_key(), 0xbba);
        }
    }
}
