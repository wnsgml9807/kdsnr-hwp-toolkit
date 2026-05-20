//! `Hnc::Shape::Text::BodyProperty` — text body property bag wrapper.
//!
//! 위치: `libHncDrawingEngine.dylib`.
//!
//! `BodyProperty` 는 `Hnc::Property::PropertyBag` 위에 typed accessor 를 제공하는
//! façade — 각 getter 는 `PropertyKey(int_id)` 로 bag 에서 `Property` sub-class 의
//! value field (Property+0xc) 를 조회 + typed cast.
//!
//! # raw 32B layout (확정 from C2 ctor `0x2e3030` + Set* methods)
//!
//! ```text
//! offset   field                    type                    크기   set 메서드
//! 0x00     bag                      PropertyBag             8B     (ctor 가 PropertyBag::PropertyBag(false))
//! 0x08     scene3d_ctrl             *ControlBlock<Scene3D>  8B     SetScene3D (raw 0x2e5668)
//! 0x10     sp3d_ctrl                *ControlBlock<Sp3D>     8B     SetSp3D (raw 0x2e5710)
//! 0x18     preset_warp              *PresetWarp (DeepPtr)   8B     SetPresetWarp (raw 0x2e100c → 0x2e5790)
//! ```
//!
//! 총 32B (= 0x20). PresetWarp 자체는 64B (raw `new(0x40)` @ `0x2e30c4`).
//!
//! # Method list (이 module 의 port scope)
//!
//! ## Scalar getters (27) — `[bag.impl][PropertyKey] → typed value`
//!
//! | symbol | raw addr | key | type | helper |
//! |--------|----------|-----|------|--------|
//! | GetVert | 0x2d2c6c | 0x89e | u32 | 0x67d0e4 |
//! | GetAutoTxRotType | 0x2d2cd8 | 0x8b0 | u32 | 0x67d654 |
//! | GetUpright | 0x2e0a2c | 0x8b0 → 0x8a9 | bool (conditional) | 0x67d654 + 0x662d4c |
//! | GetPresetWarpType | 0x2e0fa0 | 0x8ae | u32 | 0x67d484 |
//! | GetRotation | 0x2e3f74 | 0x898 | i32 (Degree) | 0x6628c8 |
//! | GetSpaceFirstLastPara | 0x2e404c | 0x899 | bool | 0x662d4c |
//! | GetHorzOverflow | 0x2e4128 | 0x89a | u32 | 0x67ce2c |
//! | GetVertOverflow | 0x2e4204 | 0x89b | u32 | 0x67cf14 |
//! | GetAnchor | 0x2e42e0 | 0x89c | u32 | 0x67cffc |
//! | GetAnchorCenter | 0x2e43bc | 0x89d | bool | 0x662d4c |
//! | GetWrap | 0x2e4498 | 0x89f | u32 | 0x67d1cc |
//! | GetLeftInset | 0x2e4574 | 0x8a0 | f32 | 0x65616c |
//! | GetTopInset | 0x2e4658 | 0x8a1 | f32 | 0x65616c |
//! | GetRightInset | 0x2e473c | 0x8a2 | f32 | 0x65616c |
//! | GetBottomInset | 0x2e4820 | 0x8a3 | f32 | 0x65616c |
//! | GetNumCol | 0x2e4be0 | 0x8a4 | u64 | 0x67d2b4 |
//! | GetSpaceCol | 0x2e4cb8 | 0x8a5 | f32 | 0x65616c |
//! | GetRtlCol | 0x2e4d9c | 0x8a6 | bool | 0x662d4c |
//! | GetFromWordArt | 0x2e4e78 | 0x8a7 | bool | 0x662d4c |
//! | GetForceAntiAlias | 0x2e4f54 | 0x8a8 | bool | 0x662d4c |
//! | GetCompatibleLineSpace | 0x2e50a0 | 0x8aa | bool | 0x662d4c |
//! | GetAutoFit | 0x2e517c | 0x8ab | u32 | 0x67d39c |
//! | GetNormalFitFontScale | 0x2e5258 | 0x8ac | f32 | 0x65616c |
//! | GetNormalFitLineReduction | 0x2e533c | 0x8ad | f32 | 0x65616c |
//! | GetAutoTxRotAngle | 0x2e5568 | 0x8b1 | i32 (Degree) | 0x6628c8 |
//!
//! ## Composite getters (L-5c-3d)
//!
//! - `GetInset` @ `0x2e4904` — Margin (4× f32 HVA: `Margin {left, top, right, bottom}`)
//! - `GetFlatText` @ `0x2e5420` — `*const pair<bool, f32>` (raw 반환 ptr 그대로)
//! - `GetPresetWarp` @ `0x2e0b08` — `&self.preset_warp` (2-instr trivial)
//!
//! ## Bag-forwarding (2)
//!
//! - `Contains(PropertyKey)` @ `0x2e3ed0` — tail-call to `PropertyBag::Contains`.
//! - `IsSaveable(PropertyKey, bool)` @ `0x2e3ed4` — Contains + GetState 조합:
//!   `Contains && (GetState in {1, 5} OR (writeAll && GetState == 2))`.
//!
//! # Deferred (다음 세션)
//!
//! - `GetScene3D` @ `0x2e5640` (sret + SharePtr<Scene3D> copy ctor + refcount++ + tail-call 0x64a1d4)
//! - `GetSp3D` @ `0x2e56e8` (sret + SharePtr<Sp3D> copy ctor + refcount++ + tail-call 0x64aa18)
//! - `operator==` @ `0x2e3ddc` (Scene3D/Sp3D/PresetWarp ptr 비교 + PropertyBag::operator== 위임)
//! - `operator!=` @ `0x2d20c8` (tail-call eq + xor 1)
//! - `Clone` @ `0x2d2c04` (sret alloc 0x20 + C2 ctor + DeepPtr wrap)
//! - `CollectProperty` @ `0x2e5a7c` (PropertyBag::Merge + Scene3D/Sp3D setter via 0x1cc15c/0x1cc2a4)
//! - `Union`/`Swap` (bag 합치기 + Scene3D/Sp3D/PresetWarp 교환)
//! - ctor `C1` @ `0x2d1d98` + `C2` @ `0x2e3030`, dtor `D1` @ `0x2d1e38` + `D2` @ `0x2e3c6c`
//! - PresetWarp 자체 64B layout
//! - 모두 Scene3D/Sp3D 타입의 byte-eq port 후 진행

use crate::property_bag::{PropertyBag, PropertyBagImpl};
use crate::property_key::PropertyKey;
use crate::share_ptr::ControlBlock;
use std::ptr;

// ─────────────────────────────────────────────────────────────────────────────
// PropertyKey integer constants — body property bag 의 key id 들.
// raw asm 의 `mov w8, #0xNNN; str w8, [sp]` 패턴에서 추출.
// ─────────────────────────────────────────────────────────────────────────────

/// `Hnc::Shape::Text::BodyProperty` 의 PropertyKey int_id 상수 모음.
///
/// raw asm 의 `mov w8, #0xNNN` 직접 인용. **byte-identical** 보장.
pub mod key {
    /// raw `0x2e3f74 GetRotation`: `mov w8, #0x898`. Value type: i32 Degree.
    pub const ROTATION: u32 = 0x898;
    /// raw `0x2e404c GetSpaceFirstLastPara`: `mov w8, #0x899`. Value: bool.
    pub const SPACE_FIRST_LAST_PARA: u32 = 0x899;
    /// raw `0x2e4128 GetHorzOverflow`: `mov w8, #0x89a`. Value: HorzOverflowType (u32).
    pub const HORZ_OVERFLOW: u32 = 0x89a;
    /// raw `0x2e4204 GetVertOverflow`: `mov w8, #0x89b`. Value: VertOverflowType (u32).
    pub const VERT_OVERFLOW: u32 = 0x89b;
    /// raw `0x2e42e0 GetAnchor`: `mov w8, #0x89c`. Value: AnchorType (u32).
    pub const ANCHOR: u32 = 0x89c;
    /// raw `0x2e43bc GetAnchorCenter`: `mov w8, #0x89d`. Value: bool.
    pub const ANCHOR_CENTER: u32 = 0x89d;
    /// raw `0x2d2c6c GetVert`: `mov w8, #0x89e`. Value: TextDirectionType (u32).
    pub const VERT: u32 = 0x89e;
    /// raw `0x2e4498 GetWrap`: `mov w8, #0x89f`. Value: WrapType (u32).
    pub const WRAP: u32 = 0x89f;
    /// raw `0x2e4574 GetLeftInset`: `mov w8, #0x8a0`. Value: f32.
    pub const LEFT_INSET: u32 = 0x8a0;
    /// raw `0x2e4658 GetTopInset`: `mov w8, #0x8a1`. Value: f32.
    pub const TOP_INSET: u32 = 0x8a1;
    /// raw `0x2e473c GetRightInset`: `mov w8, #0x8a2`. Value: f32.
    pub const RIGHT_INSET: u32 = 0x8a2;
    /// raw `0x2e4820 GetBottomInset`: `mov w8, #0x8a3`. Value: f32.
    pub const BOTTOM_INSET: u32 = 0x8a3;
    /// raw `0x2e4be0 GetNumCol`: `mov w8, #0x8a4`. Value: u64 (raw `ldr x19`).
    pub const NUM_COL: u32 = 0x8a4;
    /// raw `0x2e4cb8 GetSpaceCol`: `mov w8, #0x8a5`. Value: f32.
    pub const SPACE_COL: u32 = 0x8a5;
    /// raw `0x2e4d9c GetRtlCol`: `mov w8, #0x8a6`. Value: bool.
    pub const RTL_COL: u32 = 0x8a6;
    /// raw `0x2e4e78 GetFromWordArt`: `mov w8, #0x8a7`. Value: bool.
    pub const FROM_WORD_ART: u32 = 0x8a7;
    /// raw `0x2e4f54 GetForceAntiAlias`: `mov w8, #0x8a8`. Value: bool.
    pub const FORCE_ANTI_ALIAS: u32 = 0x8a8;
    /// raw `0x2e0a90 GetUpright (branch path)`: `mov w8, #0x8a9`. Value: bool.
    ///
    /// **주의**: GetUpright 의 fast path 는 0x8b0 (AutoTxRotType) 을 먼저 검사,
    /// 0 이면 false 반환. 0 이 아니면 0x8a9 fetch.
    pub const UPRIGHT: u32 = 0x8a9;
    /// raw `0x2e50a0 GetCompatibleLineSpace`: `mov w8, #0x8aa`. Value: bool.
    pub const COMPATIBLE_LINE_SPACE: u32 = 0x8aa;
    /// raw `0x2e517c GetAutoFit`: `mov w8, #0x8ab`. Value: AutoFitType (u32).
    pub const AUTO_FIT: u32 = 0x8ab;
    /// raw `0x2e5258 GetNormalFitFontScale`: `mov w8, #0x8ac`. Value: f32.
    pub const NORMAL_FIT_FONT_SCALE: u32 = 0x8ac;
    /// raw `0x2e533c GetNormalFitLineReduction`: `mov w8, #0x8ad`. Value: f32.
    pub const NORMAL_FIT_LINE_REDUCTION: u32 = 0x8ad;
    /// raw `0x2e0fa0 GetPresetWarpType`: `mov w8, #0x8ae`. Value: WarpShapeType (u32).
    pub const PRESET_WARP_TYPE: u32 = 0x8ae;
    /// raw `0x2e5420 GetFlatText`: `mov w8, #0x8af`. Value: pair<bool, f32> (raw 반환 ptr).
    pub const FLAT_TEXT: u32 = 0x8af;
    /// raw `0x2d2cd8 GetAutoTxRotType`: `mov w8, #0x8b0`. Value: AutoTxRotType (u32).
    pub const AUTO_TX_ROT_TYPE: u32 = 0x8b0;
    /// raw `0x2e5568 GetAutoTxRotAngle`: `mov w8, #0x8b1`. Value: i32 Degree.
    pub const AUTO_TX_ROT_ANGLE: u32 = 0x8b1;
}

// ─────────────────────────────────────────────────────────────────────────────
// Enum value type aliases — body property 의 enum 값 타입들.
// raw 의 정확한 enum 매핑은 코드 보지 못함, 본 단계는 u32 newtype 으로 typed wrap.
// ─────────────────────────────────────────────────────────────────────────────

/// `Hnc::Shape::Text::BodyProperty::TextDirectionType` — Vert 의 enum (u32).
pub type TextDirectionType = u32;

/// `Hnc::Shape::Text::BodyProperty::HorzOverflowType` — u32.
pub type HorzOverflowType = u32;

/// `Hnc::Shape::Text::BodyProperty::VertOverflowType` — u32.
pub type VertOverflowType = u32;

/// `Hnc::Shape::Text::BodyProperty::AnchorType` — u32.
pub type AnchorType = u32;

/// `Hnc::Shape::Text::BodyProperty::WrapType` — u32.
pub type WrapType = u32;

/// `Hnc::Shape::Text::BodyProperty::AutoFitType` — u32.
pub type AutoFitType = u32;

/// `Hnc::Shape::Text::BodyProperty::WarpShapeType` — PresetWarp 의 enum (u32).
pub type WarpShapeType = u32;

/// `Hnc::Shape::Text::BodyProperty::AutoTxRotType` — auto text rotation enum (u32).
pub type AutoTxRotType = u32;

// ─────────────────────────────────────────────────────────────────────────────
// Composite return value types
// ─────────────────────────────────────────────────────────────────────────────

/// `Hnc::Shape::Text::Margin` — 16B HVA-eligible struct returned by `GetInset`.
///
/// raw `0x2e49f0..0x2e49fc`: return registers loaded as
/// `s0 = s8 (LEFT)`, `s1 = s9 (TOP)`, `s2 = s10 (RIGHT)`, `s3 = s11 (BOTTOM)`.
/// AAPCS HVA: 4 × f32 returned in v0..v3.
///
/// ## Layout
///
/// ```text
/// offset   field    type   크기
/// 0x00     left     f32    4B (key 0x8a0)
/// 0x04     top      f32    4B (key 0x8a1)
/// 0x08     right    f32    4B (key 0x8a2)
/// 0x0c     bottom   f32    4B (key 0x8a3)
/// ```
///
/// 총 16B / 4B align (#[repr(C)] guarantee).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    /// raw register s0 = s8 (loaded with `Hnc::Shape::Text::BodyProperty::GetLeftInset`).
    pub left: f32,
    /// raw register s1 = s9 (loaded with `Hnc::Shape::Text::BodyProperty::GetTopInset`).
    pub top: f32,
    /// raw register s2 = s10 (loaded with `Hnc::Shape::Text::BodyProperty::GetRightInset`).
    pub right: f32,
    /// raw register s3 = s11 (loaded with `Hnc::Shape::Text::BodyProperty::GetBottomInset`).
    pub bottom: f32,
}

pub const MARGIN_SIZE_BYTES: usize = 16;
pub const MARGIN_ALIGN_BYTES: usize = 4;

const _: () = assert!(std::mem::size_of::<Margin>() == MARGIN_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<Margin>() == MARGIN_ALIGN_BYTES);

/// `std::pair<bool, f32>` — 8B struct returned by reference from `GetFlatText`.
///
/// raw `0x2e5420 GetFlatText` 는 helper 가 반환한 Property+0xc 주소를 그대로
/// 반환 (no `ldr`). caller 는 그 주소에서 `(bool, f32)` 읽음.
///
/// ## Layout (libc++ `std::__1::pair<bool, float>`)
///
/// ```text
/// offset   field    type    크기
/// 0x00     first    u8       1B (bool)
/// 0x01     _pad     [u8; 3]  3B
/// 0x04     second   f32      4B
/// ```
///
/// 총 8B / 4B align.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FlatTextPair {
    /// raw `+0x00` (bool flag — std::pair::first).
    pub first: u8,
    /// 3B padding to f32 alignment.
    pub _pad: [u8; 3],
    /// raw `+0x04` (f32 value — std::pair::second).
    pub second: f32,
}

pub const FLAT_TEXT_PAIR_SIZE_BYTES: usize = 8;
pub const FLAT_TEXT_PAIR_ALIGN_BYTES: usize = 4;

const _: () = assert!(std::mem::size_of::<FlatTextPair>() == FLAT_TEXT_PAIR_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<FlatTextPair>() == FLAT_TEXT_PAIR_ALIGN_BYTES);

// ─────────────────────────────────────────────────────────────────────────────
// BodyProperty struct
// ─────────────────────────────────────────────────────────────────────────────

/// `Hnc::Shape::Text::BodyProperty` — text body property bag (raw 32B).
///
/// Layout 1:1 with C2 ctor `0x2e3030`:
/// - `+0x00` bag: PropertyBag (8B, refcounted)
/// - `+0x08` scene3d_ctrl: SharePtr<Scene3D> 의 ControlBlock pointer (8B)
/// - `+0x10` sp3d_ctrl: SharePtr<Sp3D> 의 ControlBlock pointer (8B)
/// - `+0x18` preset_warp: DeepPtr<PresetWarp> 의 owning raw pointer (8B)
///
/// **현재 단계 (L-5c-3c)**: scalar getter port 가 목적. ctor/dtor + Scene3D/Sp3D
/// /PresetWarp 의 typed 접근자는 후속 세션.
#[repr(C)]
pub struct BodyProperty {
    /// raw `+0x00..+0x08`: `Hnc::Property::PropertyBag` (= ControlBlock<PropertyBagImpl>*).
    ///
    /// **참고**: PropertyBag 는 `repr(transparent)` 가 아니라 named struct 라
    /// 첫 8B 가 정확히 `ctrl: *mut ControlBlock<PropertyBagImpl>` — byte-eq.
    pub bag: PropertyBag,
    /// raw `+0x08..+0x10`: SharePtr<Scene3D> 의 ctrl pointer (placeholder until L-5c-4+).
    pub scene3d_ctrl: *mut ControlBlock<u8>,
    /// raw `+0x10..+0x18`: SharePtr<Sp3D> 의 ctrl pointer (placeholder).
    pub sp3d_ctrl: *mut ControlBlock<u8>,
    /// raw `+0x18..+0x20`: DeepPtr<PresetWarp> 의 owning raw pointer (64B alloc).
    pub preset_warp: *mut u8,
}

pub const BODY_PROPERTY_SIZE_BYTES: usize = 32;
pub const BODY_PROPERTY_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<BodyProperty>() == BODY_PROPERTY_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<BodyProperty>() == BODY_PROPERTY_ALIGN_BYTES);

impl BodyProperty {
    /// Common prelude shared by all scalar getters:
    /// raw `ldr x8, [x0]; cbz x8, null_branch; ldr x0, [x8]; else mov x0, #0`.
    ///
    /// Returns the underlying `*const PropertyBagImpl` (or null when bag.ctrl is null
    /// or its obj field is null).
    ///
    /// # Safety
    /// `self.bag.ctrl` 가 valid heap-alloc `ControlBlock<PropertyBagImpl>` 거나 null.
    #[inline]
    unsafe fn bag_impl_ptr(&self) -> *const PropertyBagImpl {
        // raw `0x2d2c88: ldr x8, [x0]` — load PropertyBag.ctrl (= ControlBlock*)
        let ctrl = self.bag.ctrl;
        if ctrl.is_null() {
            // raw `0x2d2c98: mov x0, #0x0`
            ptr::null()
        } else {
            // raw `0x2d2c90: ldr x0, [x8]` — load ControlBlock.obj (= PropertyBagImpl*)
            (*ctrl).obj
        }
    }

    /// Generic GetValue → u32 (`ldr w19, [x0]` pattern, raw `0x2d2ca4`).
    ///
    /// Used by GetVert/GetAutoTxRotType/GetPresetWarpType/GetHorzOverflow/
    /// GetVertOverflow/GetAnchor/GetWrap/GetAutoFit.
    ///
    /// # Safety
    /// self.bag 가 valid + key 가 bag 에 존재 + Property 가 value-field PEnum/PInt.
    #[inline]
    unsafe fn get_u32_by_key(&self, key_id: u32) -> u32 {
        // raw: stack-alloc PropertyKey(int_id = key_id)
        let key = PropertyKey::from_int(key_id);
        // raw prelude
        let bag_ptr = self.bag_impl_ptr();
        // raw `bl 0x67d0e4` (or family)
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw `ldr w19, [x0]`
        let val = *(addr as *const u32);
        // raw `bl PropertyKey::~PropertyKey` — Rust 의 Drop 으로 자동 (key going out of scope)
        val
    }

    /// Generic GetValue → bool (`ldrb w19, [x0]` pattern, raw `0x2e4084`).
    ///
    /// Used by GetSpaceFirstLastPara/GetAnchorCenter/GetRtlCol/GetFromWordArt/
    /// GetForceAntiAlias/GetCompatibleLineSpace + GetUpright's inner stage.
    ///
    /// # Safety
    /// self.bag 가 valid + key 가 bag 에 존재 + Property 가 PBool.
    #[inline]
    unsafe fn get_bool_by_key(&self, key_id: u32) -> bool {
        let key = PropertyKey::from_int(key_id);
        let bag_ptr = self.bag_impl_ptr();
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw `ldrb w19, [x0]` — 1-byte load
        let val = *(addr as *const u8);
        val != 0
    }

    /// Generic GetValue → f32 (`ldr s8, [x0]` pattern, raw `0x2e45b0`).
    ///
    /// Used by GetLeftInset/GetTopInset/GetRightInset/GetBottomInset/GetSpaceCol/
    /// GetNormalFitFontScale/GetNormalFitLineReduction.
    ///
    /// # Safety
    /// self.bag 가 valid + key 가 bag 에 존재 + Property 가 PFloat.
    #[inline]
    unsafe fn get_f32_by_key(&self, key_id: u32) -> f32 {
        let key = PropertyKey::from_int(key_id);
        let bag_ptr = self.bag_impl_ptr();
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw `ldr s8, [x0]; fmov s0, s8` — 32-bit f32 load → return register
        let val = *(addr as *const f32);
        val
    }

    /// Generic GetValue → i32 Degree (`ldr x19, [x0]` — but only low 32 bits matter
    /// since Degree is u32 sized — raw `mov x19, x0; ... ldp ... ret`).
    ///
    /// Used by GetRotation/GetAutoTxRotAngle. raw 는 `ldr x` (64-bit) 로 load
    /// 하지만 PEnum/PInt value 는 32-bit slot — 상위 32 비트는 garbage 또는
    /// adjacent struct 영역. 본 port 는 32-bit 만 의미 있음 (PInt/Degree 가 i32).
    ///
    /// # Safety
    /// self.bag 가 valid + key 가 bag 에 존재 + Property 가 PInt/PDegree.
    #[inline]
    unsafe fn get_i32_by_key(&self, key_id: u32) -> i32 {
        let key = PropertyKey::from_int(key_id);
        let bag_ptr = self.bag_impl_ptr();
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw 는 mov x19, x0; ... return x0 = x19 — but the value at +0xc is 32-bit
        let val = *(addr as *const i32);
        val
    }

    /// Generic GetValue → u64 (`ldr x19, [x0]` pattern, raw `0x2e4c18`).
    ///
    /// Used by GetNumCol — `unsigned long` (u64) in the C++ source.
    ///
    /// # Safety
    /// self.bag 가 valid + key 가 bag 에 존재 + Property 가 PUInt64.
    #[inline]
    unsafe fn get_u64_by_key(&self, key_id: u32) -> u64 {
        let key = PropertyKey::from_int(key_id);
        let bag_ptr = self.bag_impl_ptr();
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw `ldr x19, [x0]` — 64-bit load
        let val = *(addr as *const u64);
        val
    }

    // ─────────────────────────────────────────────────────────────────────
    // Scalar getters — 1:1 with raw asm.
    // ─────────────────────────────────────────────────────────────────────

    /// `GetVert() const` (`0x2d2c6c` sz=88B). Key `0x89e`. Returns TextDirectionType (u32).
    ///
    /// raw 의 flow:
    /// 1. stack alloc PropertyKey(int_id = 0x89e, str_ptr = null)
    /// 2. bag_impl_ptr = self.bag.ctrl ? self.bag.ctrl.obj : null
    /// 3. addr = helper_0x67d0e4(bag_impl_ptr, &key)
    /// 4. w19 = *(u32*)addr
    /// 5. ~PropertyKey
    /// 6. return w19
    ///
    /// # Safety
    /// `self.bag` 가 valid + bag 에 key 0x89e 가 존재 + value 가 PEnum.
    pub unsafe fn get_vert(&self) -> TextDirectionType {
        self.get_u32_by_key(key::VERT)
    }

    /// `GetAutoTxRotType() const` (`0x2d2cd8` sz=104B). Key `0x8b0`. Returns AutoTxRotType.
    pub unsafe fn get_auto_tx_rot_type(&self) -> AutoTxRotType {
        self.get_u32_by_key(key::AUTO_TX_ROT_TYPE)
    }

    /// `GetUpright() const` (`0x2e0a2c` sz=220B). Key `0x8b0` → `0x8a9`. Conditional bool.
    ///
    /// raw flow:
    /// 1. fetch AutoTxRotType (key 0x8b0)
    /// 2. if AutoTxRotType == 0: return false (raw `mov w19, #0x0; ret`)
    /// 3. else: fetch bool at key 0x8a9 (Upright)
    pub unsafe fn get_upright(&self) -> bool {
        // raw 0x2e0a2c..0x2e0a74: get_u32(AUTO_TX_ROT_TYPE)
        let auto_tx_rot_type = self.get_u32_by_key(key::AUTO_TX_ROT_TYPE);
        // raw 0x2e0a74: cbz w20, branch  ; if 0 → 0x2e0a90 (the upright fetch)
        //               else fall through to w19=0; return 0
        if auto_tx_rot_type == 0 {
            // raw 0x2e0a78: `mov w19, #0x0; return x19`
            false
        } else {
            // raw 0x2e0a90..: `mov w8, #0x8a9; ...; ldrb w19, [x0]`
            self.get_bool_by_key(key::UPRIGHT)
        }
    }

    /// `GetPresetWarpType() const` (`0x2e0fa0` sz=104B). Key `0x8ae`. Returns WarpShapeType.
    pub unsafe fn get_preset_warp_type(&self) -> WarpShapeType {
        self.get_u32_by_key(key::PRESET_WARP_TYPE)
    }

    /// `GetRotation() const` (`0x2e3f74` sz=104B). Key `0x898`. Returns i32 Degree.
    ///
    /// raw 는 `mov x19, x0` (64-bit) 후 `mov x0, x19; ret` — but the typed value
    /// field at Property+0xc is 32-bit Degree (per `Hnc::Util::Degree` layout).
    /// 본 port 는 i32 로 노출.
    pub unsafe fn get_rotation(&self) -> i32 {
        self.get_i32_by_key(key::ROTATION)
    }

    /// `GetSpaceFirstLastPara() const` (`0x2e404c` sz=104B). Key `0x899`. Bool.
    pub unsafe fn get_space_first_last_para(&self) -> bool {
        self.get_bool_by_key(key::SPACE_FIRST_LAST_PARA)
    }

    /// `GetHorzOverflow() const` (`0x2e4128` sz=104B). Key `0x89a`. Returns HorzOverflowType.
    pub unsafe fn get_horz_overflow(&self) -> HorzOverflowType {
        self.get_u32_by_key(key::HORZ_OVERFLOW)
    }

    /// `GetVertOverflow() const` (`0x2e4204` sz=104B). Key `0x89b`. Returns VertOverflowType.
    pub unsafe fn get_vert_overflow(&self) -> VertOverflowType {
        self.get_u32_by_key(key::VERT_OVERFLOW)
    }

    /// `GetAnchor() const` (`0x2e42e0` sz=104B). Key `0x89c`. Returns AnchorType.
    pub unsafe fn get_anchor(&self) -> AnchorType {
        self.get_u32_by_key(key::ANCHOR)
    }

    /// `GetAnchorCenter() const` (`0x2e43bc` sz=104B). Key `0x89d`. Bool.
    pub unsafe fn get_anchor_center(&self) -> bool {
        self.get_bool_by_key(key::ANCHOR_CENTER)
    }

    /// `GetWrap() const` (`0x2e4498` sz=104B). Key `0x89f`. Returns WrapType.
    pub unsafe fn get_wrap(&self) -> WrapType {
        self.get_u32_by_key(key::WRAP)
    }

    /// `GetLeftInset() const` (`0x2e4574` sz=120B). Key `0x8a0`. f32.
    pub unsafe fn get_left_inset(&self) -> f32 {
        self.get_f32_by_key(key::LEFT_INSET)
    }

    /// `GetTopInset() const` (`0x2e4658` sz=120B). Key `0x8a1`. f32.
    pub unsafe fn get_top_inset(&self) -> f32 {
        self.get_f32_by_key(key::TOP_INSET)
    }

    /// `GetRightInset() const` (`0x2e473c` sz=120B). Key `0x8a2`. f32.
    pub unsafe fn get_right_inset(&self) -> f32 {
        self.get_f32_by_key(key::RIGHT_INSET)
    }

    /// `GetBottomInset() const` (`0x2e4820` sz=120B). Key `0x8a3`. f32.
    pub unsafe fn get_bottom_inset(&self) -> f32 {
        self.get_f32_by_key(key::BOTTOM_INSET)
    }

    /// `GetNumCol() const` (`0x2e4be0` sz=104B). Key `0x8a4`. u64.
    pub unsafe fn get_num_col(&self) -> u64 {
        self.get_u64_by_key(key::NUM_COL)
    }

    /// `GetSpaceCol() const` (`0x2e4cb8` sz=120B). Key `0x8a5`. f32.
    pub unsafe fn get_space_col(&self) -> f32 {
        self.get_f32_by_key(key::SPACE_COL)
    }

    /// `GetRtlCol() const` (`0x2e4d9c` sz=104B). Key `0x8a6`. Bool.
    pub unsafe fn get_rtl_col(&self) -> bool {
        self.get_bool_by_key(key::RTL_COL)
    }

    /// `GetFromWordArt() const` (`0x2e4e78` sz=104B). Key `0x8a7`. Bool.
    pub unsafe fn get_from_word_art(&self) -> bool {
        self.get_bool_by_key(key::FROM_WORD_ART)
    }

    /// `GetForceAntiAlias() const` (`0x2e4f54` sz=104B). Key `0x8a8`. Bool.
    pub unsafe fn get_force_anti_alias(&self) -> bool {
        self.get_bool_by_key(key::FORCE_ANTI_ALIAS)
    }

    /// `GetCompatibleLineSpace() const` (`0x2e50a0` sz=104B). Key `0x8aa`. Bool.
    pub unsafe fn get_compatible_line_space(&self) -> bool {
        self.get_bool_by_key(key::COMPATIBLE_LINE_SPACE)
    }

    /// `GetAutoFit() const` (`0x2e517c` sz=104B). Key `0x8ab`. Returns AutoFitType.
    pub unsafe fn get_auto_fit(&self) -> AutoFitType {
        self.get_u32_by_key(key::AUTO_FIT)
    }

    /// `GetNormalFitFontScale() const` (`0x2e5258` sz=120B). Key `0x8ac`. f32.
    pub unsafe fn get_normal_fit_font_scale(&self) -> f32 {
        self.get_f32_by_key(key::NORMAL_FIT_FONT_SCALE)
    }

    /// `GetNormalFitLineReduction() const` (`0x2e533c` sz=120B). Key `0x8ad`. f32.
    pub unsafe fn get_normal_fit_line_reduction(&self) -> f32 {
        self.get_f32_by_key(key::NORMAL_FIT_LINE_REDUCTION)
    }

    /// `GetAutoTxRotAngle() const` (`0x2e5568` sz=104B). Key `0x8b1`. i32 Degree.
    pub unsafe fn get_auto_tx_rot_angle(&self) -> i32 {
        self.get_i32_by_key(key::AUTO_TX_ROT_ANGLE)
    }

    // ─────────────────────────────────────────────────────────────────────
    // Composite getters
    // ─────────────────────────────────────────────────────────────────────

    /// `GetInset() const` (`0x2e4904` sz=272B). Composite Margin {left, top, right, bottom}.
    ///
    /// raw flow (4 sequential f32 getter calls + HVA return):
    /// ```text
    /// s8  = GetLeftInset()     (key 0x8a0, helper 0x65616c)
    /// s9  = GetTopInset()      (key 0x8a1)
    /// s10 = GetRightInset()    (key 0x8a2)
    /// s11 = GetBottomInset()   (key 0x8a3)
    /// fmov s0, s8 ; s1=s9 ; s2=s10 ; s3=s11
    /// ret    (HVA return: v0..v3)
    /// ```
    ///
    /// AAPCS HVA: `Margin` 가 4 × f32 라서 v0..v3 레지스터로 반환. Rust 의
    /// `#[repr(C)] struct Margin { f32 × 4 }` returned by value 도 동일 ABI.
    ///
    /// # Safety
    /// `self.bag` 가 valid + 4 키 (0x8a0..0x8a3) 모두 bag 에 존재 + 모두 PFloat.
    pub unsafe fn get_inset(&self) -> Margin {
        // raw 0x2e4920..0x2e4948: s8 = GetLeftInset
        let s8 = self.get_f32_by_key(key::LEFT_INSET);
        // raw 0x2e4954..0x2e497c: s9 = GetTopInset
        let s9 = self.get_f32_by_key(key::TOP_INSET);
        // raw 0x2e4988..0x2e49b0: s10 = GetRightInset
        let s10 = self.get_f32_by_key(key::RIGHT_INSET);
        // raw 0x2e49bc..0x2e49e4: s11 = GetBottomInset
        let s11 = self.get_f32_by_key(key::BOTTOM_INSET);
        // raw 0x2e49f0..0x2e49fc: fmov s0/s1/s2/s3 = s8/s9/s10/s11
        Margin {
            left: s8,
            top: s9,
            right: s10,
            bottom: s11,
        }
    }

    /// `GetFlatText() const` (`0x2e5420` sz=108B). Key `0x8af`. Returns *raw pointer*
    /// to `pair<bool, f32>` in the Property's value slot.
    ///
    /// raw 의 특이점: helper (`0x67d56c` — byte-identical 10th instantiation of the
    /// 0x67d0e4 family) 가 반환한 Property+0xc 주소를 그대로 caller 에게 반환 —
    /// `ldr` 명령이 없음. caller 가 직접 `(bool, f32)` 를 읽음.
    ///
    /// ```text
    /// 0x2e5454: bl 0x67d56c       ; addr = helper(impl, &key)
    /// 0x2e5458: mov x19, x0       ; save addr
    /// 0x2e5460: bl ~PropertyKey
    /// 0x2e5464: mov x0, x19       ; return addr
    /// ```
    ///
    /// 본 port 는 byte-eq 의미를 보존하기 위해 `*const FlatTextPair` 반환.
    ///
    /// # Safety
    /// `self.bag` 가 valid + key `0x8af` 가 bag 에 존재 + value 가 `pair<bool,f32>`-shaped.
    /// 반환된 포인터는 bag 의 Property 가 살아있는 동안만 valid.
    pub unsafe fn get_flat_text(&self) -> *const FlatTextPair {
        // raw 의 PropertyKey alloc
        let key = PropertyKey::from_int(key::FLAT_TEXT);
        // raw 의 bag_impl_ptr resolve
        let bag_ptr = self.bag_impl_ptr();
        // raw `bl 0x67d56c` (byte-identical family member)
        let addr = PropertyBagImpl::get_value_addr(bag_ptr, &key);
        // raw `mov x19, x0; mov x0, x19; ret` — return the address as-is.
        // No `ldr` — raw returns pointer to the typed value at Property+0xc.
        addr as *const FlatTextPair
    }

    /// `GetFlatText() const` 의 편의 wrapper — pointer dereference 해서 값 반환.
    ///
    /// raw 는 pointer 만 반환하지만, 안전한 Rust API 제공.
    ///
    /// # Safety
    /// `self.bag` 가 valid + key `0x8af` 가 bag 에 존재.
    pub unsafe fn get_flat_text_value(&self) -> FlatTextPair {
        *self.get_flat_text()
    }

    // ─────────────────────────────────────────────────────────────────────
    // Reference-returning getter (no helper call)
    // ─────────────────────────────────────────────────────────────────────

    /// `GetPresetWarp() const` (`0x2e0b08` sz=8B). Returns `&self.preset_warp` slot address.
    ///
    /// raw 전체 (2 instr):
    /// ```text
    /// add x0, x0, #0x18
    /// ret
    /// ```
    ///
    /// 즉 BodyProperty+0x18 의 주소 반환 — `preset_warp` field 의 slot 주소.
    /// Caller 는 그 주소에서 `*mut PresetWarp` 를 dereference 할 수 있다.
    ///
    /// Rust port: `&self.preset_warp` 으로 동등.
    pub fn get_preset_warp(&self) -> &*mut u8 {
        // raw `add x0, x0, #0x18`: BodyProperty + 0x18 = &self.preset_warp
        &self.preset_warp
    }

    /// Raw pointer variant of `get_preset_warp` — byte-eq 1:1 mirror (returns the
    /// address itself, not a Rust reference).
    pub fn get_preset_warp_ptr(&self) -> *const *mut u8 {
        // raw `add x0, x0, #0x18; ret` — return address of slot.
        &self.preset_warp as *const _
    }

    // ─────────────────────────────────────────────────────────────────────
    // Bag-forwarding methods
    // ─────────────────────────────────────────────────────────────────────

    /// `Contains(PropertyKey) const` (`0x2e3ed0` sz=4B). Pure tail-call:
    /// `b PropertyBag::Contains`.
    pub fn contains(&self, key: &PropertyKey) -> bool {
        self.bag.contains(key)
    }

    /// `IsSaveable(PropertyKey, bool write_all) const` (`0x2e3ed4` sz=140B).
    ///
    /// raw flow (decompiled `0x2e3ed4..0x2e3f60`):
    /// ```text
    /// if (!bag.Contains(key)) return false;
    /// if (bag.GetState(key) == 1) return true;    // (raw `cmp w0, #0x1; b.eq ret_1`)
    /// if (bag.GetState(key) == 5) return true;    // (raw `cmp w0, #0x5; b.ne else`)
    /// if (!write_all) return false;
    /// if (bag.GetState(key) == 2) return true;
    /// return false;
    /// ```
    ///
    /// State 의미 (inferred from raw):
    /// - 1 = ENABLED_DEFAULT (always saveable)
    /// - 2 = ENABLED_EXPLICIT (saveable only with write_all)
    /// - 5 = ? (always saveable — likely "modified")
    pub fn is_saveable(&self, key: &PropertyKey, write_all: bool) -> bool {
        // raw `bl PropertyBag::Contains; cbz w0, exit_0`
        if !self.bag.contains(key) {
            return false;
        }
        // raw `bl PropertyBag::GetState; cmp w0, #0x1; b.eq ret_1`
        let state = self.bag.get_state(key);
        if state == 1 {
            return true;
        }
        // raw `bl PropertyBag::GetState; cmp w0, #0x5; b.ne else`
        // (raw 가 GetState 를 두 번 호출 — 컴파일러 의 inlining 결과)
        if self.bag.get_state(key) == 5 {
            return true;
        }
        // raw `cbz w21, exit_0` — if !write_all, exit
        if !write_all {
            return false;
        }
        // raw `bl PropertyBag::GetState; cmp w0, #0x2; b.ne exit_0; mov w0, #0x1`
        if self.bag.get_state(key) == 2 {
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::{state, PBool, PEnum, PFloat, Property};

    /// Helper: build a PropertyBag containing a single PEnum entry.
    unsafe fn make_pe_bag(key_id: u32, value: u32) -> PropertyBag {
        let mut bag = PropertyBag::new(false);
        let key = PropertyKey::from_int(key_id);
        let ctrl = PEnum::create_attach_ctrl(state::ENABLED_DEFAULT, value);
        let _prev = bag.attach(&key, ctrl);
        bag
    }

    /// Helper: build a PropertyBag containing a single PFloat entry.
    unsafe fn make_pf_bag(key_id: u32, value: f32) -> PropertyBag {
        let mut bag = PropertyBag::new(false);
        let key = PropertyKey::from_int(key_id);
        let ctrl = PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, value);
        let _prev = bag.attach(&key, ctrl);
        bag
    }

    /// Helper: build a PropertyBag containing a single PBool entry.
    unsafe fn make_pb_bag(key_id: u32, value: bool) -> PropertyBag {
        let mut bag = PropertyBag::new(false);
        let key = PropertyKey::from_int(key_id);
        let ctrl = PBool::create_attach_ctrl(state::ENABLED_DEFAULT, value);
        let _prev = bag.attach(&key, ctrl);
        bag
    }

    /// Build a BodyProperty with a fresh bag — Scene3D/Sp3D/PresetWarp = null.
    fn make_body_with_bag(bag: PropertyBag) -> BodyProperty {
        BodyProperty {
            bag,
            scene3d_ctrl: ptr::null_mut(),
            sp3d_ctrl: ptr::null_mut(),
            preset_warp: ptr::null_mut(),
        }
    }

    #[test]
    fn layout_matches_raw_32bytes() {
        // raw `0x2e3030` C2 ctor + 4 fields @ 0/8/0x10/0x18 → 32B total.
        assert_eq!(std::mem::size_of::<BodyProperty>(), 32);
        assert_eq!(std::mem::align_of::<BodyProperty>(), 8);
    }

    #[test]
    fn field_offsets_match_raw() {
        let bp = BodyProperty {
            bag: PropertyBag::new(false),
            scene3d_ctrl: ptr::null_mut(),
            sp3d_ctrl: ptr::null_mut(),
            preset_warp: ptr::null_mut(),
        };
        let base = &bp as *const _ as usize;
        assert_eq!(&bp.bag as *const _ as usize - base, 0x00);
        assert_eq!(&bp.scene3d_ctrl as *const _ as usize - base, 0x08);
        assert_eq!(&bp.sp3d_ctrl as *const _ as usize - base, 0x10);
        assert_eq!(&bp.preset_warp as *const _ as usize - base, 0x18);
    }

    #[test]
    fn key_constants_match_raw_asm() {
        // raw 의 mov w8, #0xNNN 와 1:1.
        assert_eq!(key::ROTATION, 0x898);
        assert_eq!(key::SPACE_FIRST_LAST_PARA, 0x899);
        assert_eq!(key::HORZ_OVERFLOW, 0x89a);
        assert_eq!(key::VERT_OVERFLOW, 0x89b);
        assert_eq!(key::ANCHOR, 0x89c);
        assert_eq!(key::ANCHOR_CENTER, 0x89d);
        assert_eq!(key::VERT, 0x89e);
        assert_eq!(key::WRAP, 0x89f);
        assert_eq!(key::LEFT_INSET, 0x8a0);
        assert_eq!(key::TOP_INSET, 0x8a1);
        assert_eq!(key::RIGHT_INSET, 0x8a2);
        assert_eq!(key::BOTTOM_INSET, 0x8a3);
        assert_eq!(key::NUM_COL, 0x8a4);
        assert_eq!(key::SPACE_COL, 0x8a5);
        assert_eq!(key::RTL_COL, 0x8a6);
        assert_eq!(key::FROM_WORD_ART, 0x8a7);
        assert_eq!(key::FORCE_ANTI_ALIAS, 0x8a8);
        assert_eq!(key::UPRIGHT, 0x8a9);
        assert_eq!(key::COMPATIBLE_LINE_SPACE, 0x8aa);
        assert_eq!(key::AUTO_FIT, 0x8ab);
        assert_eq!(key::NORMAL_FIT_FONT_SCALE, 0x8ac);
        assert_eq!(key::NORMAL_FIT_LINE_REDUCTION, 0x8ad);
        assert_eq!(key::PRESET_WARP_TYPE, 0x8ae);
        assert_eq!(key::FLAT_TEXT, 0x8af);
        assert_eq!(key::AUTO_TX_ROT_TYPE, 0x8b0);
        assert_eq!(key::AUTO_TX_ROT_ANGLE, 0x8b1);
    }

    // ─────────────────────────────────────────────────────────────────────
    // u32 getters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn get_vert_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::VERT, 3);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_vert(), 3);
        }
    }

    #[test]
    fn get_auto_tx_rot_type_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::AUTO_TX_ROT_TYPE, 7);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_auto_tx_rot_type(), 7);
        }
    }

    #[test]
    fn get_preset_warp_type_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::PRESET_WARP_TYPE, 42);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_preset_warp_type(), 42);
        }
    }

    #[test]
    fn get_horz_overflow_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::HORZ_OVERFLOW, 2);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_horz_overflow(), 2);
        }
    }

    #[test]
    fn get_vert_overflow_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::VERT_OVERFLOW, 1);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_vert_overflow(), 1);
        }
    }

    #[test]
    fn get_anchor_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::ANCHOR, 4);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_anchor(), 4);
        }
    }

    #[test]
    fn get_wrap_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::WRAP, 0);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_wrap(), 0);
        }
    }

    #[test]
    fn get_auto_fit_returns_pe_value() {
        unsafe {
            let bag = make_pe_bag(key::AUTO_FIT, 5);
            let bp = make_body_with_bag(bag);
            assert_eq!(bp.get_auto_fit(), 5);
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // bool getters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn get_space_first_last_para_true_false() {
        unsafe {
            let bag = make_pb_bag(key::SPACE_FIRST_LAST_PARA, true);
            assert!(make_body_with_bag(bag).get_space_first_last_para());
            let bag2 = make_pb_bag(key::SPACE_FIRST_LAST_PARA, false);
            assert!(!make_body_with_bag(bag2).get_space_first_last_para());
        }
    }

    #[test]
    fn get_anchor_center_returns_pb() {
        unsafe {
            let bag = make_pb_bag(key::ANCHOR_CENTER, true);
            assert!(make_body_with_bag(bag).get_anchor_center());
        }
    }

    #[test]
    fn get_rtl_col_returns_pb() {
        unsafe {
            let bag = make_pb_bag(key::RTL_COL, true);
            assert!(make_body_with_bag(bag).get_rtl_col());
        }
    }

    #[test]
    fn get_from_word_art_returns_pb() {
        unsafe {
            let bag = make_pb_bag(key::FROM_WORD_ART, true);
            assert!(make_body_with_bag(bag).get_from_word_art());
        }
    }

    #[test]
    fn get_force_anti_alias_returns_pb() {
        unsafe {
            let bag = make_pb_bag(key::FORCE_ANTI_ALIAS, false);
            assert!(!make_body_with_bag(bag).get_force_anti_alias());
        }
    }

    #[test]
    fn get_compatible_line_space_returns_pb() {
        unsafe {
            let bag = make_pb_bag(key::COMPATIBLE_LINE_SPACE, true);
            assert!(make_body_with_bag(bag).get_compatible_line_space());
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // f32 getters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn get_left_inset_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::LEFT_INSET, 1.5);
            assert_eq!(make_body_with_bag(bag).get_left_inset(), 1.5);
        }
    }

    #[test]
    fn get_top_inset_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::TOP_INSET, 2.5);
            assert_eq!(make_body_with_bag(bag).get_top_inset(), 2.5);
        }
    }

    #[test]
    fn get_right_inset_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::RIGHT_INSET, 3.5);
            assert_eq!(make_body_with_bag(bag).get_right_inset(), 3.5);
        }
    }

    #[test]
    fn get_bottom_inset_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::BOTTOM_INSET, 4.5);
            assert_eq!(make_body_with_bag(bag).get_bottom_inset(), 4.5);
        }
    }

    #[test]
    fn get_space_col_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::SPACE_COL, 7.25);
            assert_eq!(make_body_with_bag(bag).get_space_col(), 7.25);
        }
    }

    #[test]
    fn get_normal_fit_font_scale_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::NORMAL_FIT_FONT_SCALE, 0.85);
            assert_eq!(make_body_with_bag(bag).get_normal_fit_font_scale(), 0.85);
        }
    }

    #[test]
    fn get_normal_fit_line_reduction_returns_pf() {
        unsafe {
            let bag = make_pf_bag(key::NORMAL_FIT_LINE_REDUCTION, 0.5);
            assert_eq!(make_body_with_bag(bag).get_normal_fit_line_reduction(), 0.5);
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // i32 / u64 getters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn get_rotation_returns_pe_as_i32() {
        unsafe {
            // PEnum.value is u32 — interpret as i32. 90 degrees → 90.
            let bag = make_pe_bag(key::ROTATION, 90u32);
            assert_eq!(make_body_with_bag(bag).get_rotation(), 90);
            // Negative degree as i32 — store as u32 bit pattern -45 → 0xFFFFFFD3.
            let bag2 = make_pe_bag(key::ROTATION, (-45i32) as u32);
            assert_eq!(make_body_with_bag(bag2).get_rotation(), -45);
        }
    }

    #[test]
    fn get_auto_tx_rot_angle_returns_pe_as_i32() {
        unsafe {
            let bag = make_pe_bag(key::AUTO_TX_ROT_ANGLE, 180u32);
            assert_eq!(make_body_with_bag(bag).get_auto_tx_rot_angle(), 180);
        }
    }

    #[test]
    #[ignore = "PUInt64 (8B value at +0xc) not yet ported; PEnum/PFloat/PBool only have 4B at +0xc \
                so reading 8 bytes overflows the alloc. Re-enable after PUInt64 port."]
    fn get_num_col_returns_u64() {
        // raw `GetNumCol` does `ldr x19, [x0]` (8-byte load at Property+0xc).
        // 해당 위치에는 PUInt64 (Property + u64 value @ +0xc) 가 있어야 byte-eq.
        // 본 단계 (L-5c-3c) 는 PEnum/PFloat/PBool 만 사용 — PUInt64 ctor 가
        // property.rs 에 추가되면 그때 enable.
        unsafe {
            // Placeholder — would alloc a 24B PUInt64 and verify low 64 bits.
            let bag = make_pe_bag(key::NUM_COL, 3u32);
            let _val = make_body_with_bag(bag).get_num_col();
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Conditional getter: GetUpright
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn get_upright_returns_false_when_auto_tx_rot_type_is_zero() {
        unsafe {
            // raw 0x2e0a74: if AutoTxRotType == 0 → return false (Upright key not consulted).
            let bag = make_pe_bag(key::AUTO_TX_ROT_TYPE, 0);
            let bp = make_body_with_bag(bag);
            // Note: Upright key 0x8a9 is NOT in the bag, but we don't reach it.
            assert!(!bp.get_upright());
        }
    }

    #[test]
    fn get_upright_returns_upright_value_when_auto_tx_rot_type_nonzero() {
        unsafe {
            // raw 0x2e0a90+: AutoTxRotType != 0 → fetch UPRIGHT
            let mut bag = PropertyBag::new(false);
            let k1 = PropertyKey::from_int(key::AUTO_TX_ROT_TYPE);
            let _ = bag.attach(&k1, PEnum::create_attach_ctrl(state::ENABLED_DEFAULT, 1));

            let k2 = PropertyKey::from_int(key::UPRIGHT);
            let _ = bag.attach(&k2, PBool::create_attach_ctrl(state::ENABLED_DEFAULT, true));

            let bp = make_body_with_bag(bag);
            assert!(bp.get_upright());

            // and false case
            let mut bag2 = PropertyBag::new(false);
            let k1b = PropertyKey::from_int(key::AUTO_TX_ROT_TYPE);
            let _ = bag2.attach(&k1b, PEnum::create_attach_ctrl(state::ENABLED_DEFAULT, 2));

            let k2b = PropertyKey::from_int(key::UPRIGHT);
            let _ = bag2.attach(&k2b, PBool::create_attach_ctrl(state::ENABLED_DEFAULT, false));

            assert!(!make_body_with_bag(bag2).get_upright());
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Bag-forwarding: contains / is_saveable
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn contains_forwards_to_bag() {
        unsafe {
            let bag = make_pe_bag(key::VERT, 3);
            let bp = make_body_with_bag(bag);
            assert!(bp.contains(&PropertyKey::from_int(key::VERT)));
            assert!(!bp.contains(&PropertyKey::from_int(key::ROTATION)));
        }
    }

    #[test]
    fn is_saveable_false_when_not_contained() {
        unsafe {
            let bag = make_pe_bag(key::VERT, 3);
            let bp = make_body_with_bag(bag);
            assert!(!bp.is_saveable(&PropertyKey::from_int(key::ROTATION), true));
            assert!(!bp.is_saveable(&PropertyKey::from_int(key::ROTATION), false));
        }
    }

    #[test]
    fn is_saveable_true_for_state_1_regardless_of_write_all() {
        unsafe {
            // state ENABLED_DEFAULT = 1
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(key::VERT);
            let _ = bag.attach(&k, PEnum::create_attach_ctrl(state::ENABLED_DEFAULT, 0));
            let bp = make_body_with_bag(bag);
            assert!(bp.is_saveable(&k, true));
            assert!(bp.is_saveable(&k, false));
        }
    }

    #[test]
    fn is_saveable_true_for_state_5_regardless_of_write_all() {
        unsafe {
            // raw 의 special-case: state == 5 (likely "modified")
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(key::VERT);
            let _ = bag.attach(&k, PEnum::create_attach_ctrl(5, 0));
            let bp = make_body_with_bag(bag);
            assert!(bp.is_saveable(&k, true));
            assert!(bp.is_saveable(&k, false));
        }
    }

    #[test]
    fn is_saveable_state_2_requires_write_all() {
        unsafe {
            // state ENABLED_EXPLICIT = 2 → saveable iff write_all
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(key::VERT);
            let _ = bag.attach(&k, PEnum::create_attach_ctrl(state::ENABLED_EXPLICIT, 0));
            let bp = make_body_with_bag(bag);
            assert!(bp.is_saveable(&k, true));
            assert!(!bp.is_saveable(&k, false));
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // L-5c-3d Composite getters
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn margin_layout_is_16_bytes_4_align() {
        // raw `Hnc::Shape::Text::Margin` HVA layout: 4× f32 = 16B / align 4.
        assert_eq!(std::mem::size_of::<Margin>(), 16);
        assert_eq!(std::mem::align_of::<Margin>(), 4);
        // field offsets
        let m = Margin { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };
        let base = &m as *const _ as usize;
        assert_eq!(&m.left as *const _ as usize - base, 0);
        assert_eq!(&m.top as *const _ as usize - base, 4);
        assert_eq!(&m.right as *const _ as usize - base, 8);
        assert_eq!(&m.bottom as *const _ as usize - base, 12);
    }

    #[test]
    fn flat_text_pair_layout_is_8_bytes_4_align() {
        // libc++ pair<bool, f32>: bool@0, pad[1..4], f32@4 → 8B.
        assert_eq!(std::mem::size_of::<FlatTextPair>(), 8);
        assert_eq!(std::mem::align_of::<FlatTextPair>(), 4);
        let p = FlatTextPair { first: 0, _pad: [0; 3], second: 0.0 };
        let base = &p as *const _ as usize;
        assert_eq!(&p.first as *const _ as usize - base, 0);
        assert_eq!(&p.second as *const _ as usize - base, 4);
    }

    #[test]
    fn get_inset_composes_four_floats_into_margin() {
        unsafe {
            // raw: GetInset reads keys 0x8a0/0x8a1/0x8a2/0x8a3 sequentially.
            let mut bag = PropertyBag::new(false);
            let _ = bag.attach(
                &PropertyKey::from_int(key::LEFT_INSET),
                PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, 1.5),
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::TOP_INSET),
                PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, 2.5),
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::RIGHT_INSET),
                PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, 3.5),
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::BOTTOM_INSET),
                PFloat::create_attach_ctrl(state::ENABLED_DEFAULT, 4.5),
            );

            let bp = make_body_with_bag(bag);
            let m = bp.get_inset();
            assert_eq!(m.left, 1.5);
            assert_eq!(m.top, 2.5);
            assert_eq!(m.right, 3.5);
            assert_eq!(m.bottom, 4.5);
        }
    }

    #[test]
    fn get_inset_field_order_matches_register_assignment() {
        // raw 0x2e49f0..0x2e49fc: s0=s8 (LEFT), s1=s9 (TOP), s2=s10 (RIGHT), s3=s11 (BOTTOM).
        // Margin struct's field order must match (HVA ABI).
        unsafe {
            let mut bag = PropertyBag::new(false);
            // Use distinct values so a swap would be obvious.
            let _ = bag.attach(
                &PropertyKey::from_int(key::LEFT_INSET),
                PFloat::create_attach_ctrl(1, f32::from_bits(0x40000000)), // 2.0
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::TOP_INSET),
                PFloat::create_attach_ctrl(1, f32::from_bits(0x40400000)), // 3.0
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::RIGHT_INSET),
                PFloat::create_attach_ctrl(1, f32::from_bits(0x40800000)), // 4.0
            );
            let _ = bag.attach(
                &PropertyKey::from_int(key::BOTTOM_INSET),
                PFloat::create_attach_ctrl(1, f32::from_bits(0x40a00000)), // 5.0
            );

            let bp = make_body_with_bag(bag);
            let m = bp.get_inset();
            // Byte-level identical to raw register sequence
            assert_eq!(m.left.to_bits(), 0x40000000);
            assert_eq!(m.top.to_bits(), 0x40400000);
            assert_eq!(m.right.to_bits(), 0x40800000);
            assert_eq!(m.bottom.to_bits(), 0x40a00000);
        }
    }

    // FlatTextPair custom Property for testing — Property base + (bool, pad[3], f32).
    // PropertyBag::attach 가 ControlBlock<Property> 받으므로 같은 alloc pattern 사용.
    fn make_flat_text_ctrl(flag: bool, value: f32) -> *mut ControlBlock<Property> {
        // 16B alloc: Property header (8B vtable + 4B state + 4B pad) +
        //   pair<bool,f32> 8B at offset +0xc..+0x14 ... wait Property is 16B
        //   and value field at +0xc is only 4B. We need to alloc 24B to fit pair.
        // For test purposes, use PVec4 which has 16B value slot at +0x10.
        // But raw GetFlatText reads at Property+0xc (HelperFamily returns Property+0xc).
        // So pair occupies +0xc..+0x14 which requires Property base to be alloc'd 24B.
        //
        // Custom alloc: 24B for Property + pair, then write fields by hand.
        use std::alloc::{alloc, Layout};
        unsafe {
            let layout = Layout::from_size_align(24, 8).unwrap();
            let raw = alloc(layout) as *mut u8;
            // Zero-init
            std::ptr::write_bytes(raw, 0, 24);
            // Property header at +0..+0x10
            (raw as *mut crate::property::Property).write(crate::property::Property {
                vtable: std::ptr::null(),
                state: 1,
                _pad: 0,
            });
            // pair<bool, f32> at +0xc..+0x14 (overlapping Property's _pad + extra 4B)
            *raw.add(0xc) = flag as u8;
            *raw.add(0xd) = 0;
            *raw.add(0xe) = 0;
            *raw.add(0xf) = 0;
            *(raw.add(0x10) as *mut f32) = value;
            // Wrap in ControlBlock
            let ctrl_box = Box::new(crate::share_ptr::ControlBlock {
                obj: raw as *mut Property,
                refcount: 1,
            });
            Box::into_raw(ctrl_box)
        }
    }

    #[test]
    fn get_flat_text_returns_pointer_to_pair() {
        unsafe {
            let mut bag = PropertyBag::new(false);
            let _ = bag.attach(
                &PropertyKey::from_int(key::FLAT_TEXT),
                make_flat_text_ctrl(true, 7.5),
            );
            let bp = make_body_with_bag(bag);
            let ptr = bp.get_flat_text();
            assert!(!ptr.is_null());
            let pair = *ptr;
            assert_eq!(pair.first, 1);
            assert_eq!(pair.second, 7.5);
        }
    }

    #[test]
    fn get_flat_text_value_dereferences_pointer() {
        unsafe {
            let mut bag = PropertyBag::new(false);
            let _ = bag.attach(
                &PropertyKey::from_int(key::FLAT_TEXT),
                make_flat_text_ctrl(false, 3.25),
            );
            let bp = make_body_with_bag(bag);
            let pair = bp.get_flat_text_value();
            assert_eq!(pair.first, 0);
            assert_eq!(pair.second, 3.25);
        }
    }

    #[test]
    fn get_preset_warp_returns_address_of_slot() {
        // raw `add x0, x0, #0x18; ret` — returns BodyProperty + 0x18 address.
        let bp = BodyProperty {
            bag: PropertyBag::new(false),
            scene3d_ctrl: ptr::null_mut(),
            sp3d_ctrl: ptr::null_mut(),
            preset_warp: 0xDEAD_BEEF_usize as *mut u8,
        };
        let ref_ptr = bp.get_preset_warp();
        // Address of returned reference must be BodyProperty + 0x18.
        let base = &bp as *const _ as usize;
        let ref_addr = ref_ptr as *const _ as usize;
        assert_eq!(ref_addr - base, 0x18);
        // Dereferenced value matches preset_warp field
        assert_eq!(*ref_ptr as usize, 0xDEAD_BEEF);
    }

    #[test]
    fn get_preset_warp_ptr_matches_field_address() {
        let bp = BodyProperty {
            bag: PropertyBag::new(false),
            scene3d_ctrl: ptr::null_mut(),
            sp3d_ctrl: ptr::null_mut(),
            preset_warp: ptr::null_mut(),
        };
        let raw = bp.get_preset_warp_ptr();
        let base = &bp as *const _ as usize;
        assert_eq!(raw as usize - base, 0x18);
    }

    #[test]
    fn is_saveable_state_0_or_3_is_false() {
        unsafe {
            // state DEFAULT = 0
            let mut bag = PropertyBag::new(false);
            let k = PropertyKey::from_int(key::VERT);
            let _ = bag.attach(&k, PEnum::create_attach_ctrl(state::DEFAULT, 0));
            let bp = make_body_with_bag(bag);
            assert!(!bp.is_saveable(&k, true));
            assert!(!bp.is_saveable(&k, false));

            // state DISABLED = 3
            let mut bag2 = PropertyBag::new(false);
            let k2 = PropertyKey::from_int(key::VERT);
            let _ = bag2.attach(&k2, PEnum::create_attach_ctrl(state::DISABLED, 0));
            let bp2 = make_body_with_bag(bag2);
            assert!(!bp2.is_saveable(&k2, true));
            assert!(!bp2.is_saveable(&k2, false));
        }
    }
}
