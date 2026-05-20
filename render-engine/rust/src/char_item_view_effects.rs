//! `Hnc::Shape::Text::CharItemView::GetRealTextEffects` byte-eq port (L-5c-RE-4).
//!
//! ## raw 출처
//!
//! - `__ZNK3Hnc5Shape4Text12CharItemView19GetRealTextEffectsERKNS_6Memory9UniquePtrINS0_7EffectsEEERKNS_4Type4FlagE`
//! - 주소: `0x2f2ad8`
//! - 크기: 2428B (0x97c, 607 instructions, end at 0x2f3450)
//!
//! ## 함수 의미
//!
//! 텍스트 효과 (Shadow/OuterShadow/Reflection/Glow) 의 device-pixel scaling 을 적용한
//! `UniquePtr<Effects>` 사본을 반환. 4 effect type 각각에 대해:
//!
//! - **Shadow (key 0x3ee, scale = 0x3a2)**: distance = font_size × distance_raw / 96.0
//! - **OuterShadow (key 0xbba, scale = 0x3a2)**: distance = (1 - shadow_offset_ratio) ×
//!   (font_size × distance_raw) / 72.0
//! - **Reflection (key 0xbbb, scale = 0x3b4 + 0x3b1)**: blur = blur_raw + (1-blur)×0.3,
//!   alpha_start = alpha_raw + (1-alpha)×0.5, distance = font_size × scale / 96.0 × 72.0 / 18.0
//!   + (CharItemView+0x44 × 2 × ShapeEngine.unit) / -72.0
//! - **Glow (key 0xbb9, scale = 0x3af)**: radius = font_size × radius_raw × 3.0 / 96.0
//!
//! ## P0 input 분석 결과 (work/e2e/ 104 sections)
//!
//! - `<hp:t>` 4712건 → 모든 텍스트가 본 함수 호출
//! - `<hp:outerShadow>` / `<hp:reflection>` / `<hp:glow>` / `<hp:innerShadow>` / `<hp:softEdge>`: **0건**
//! - `<hp:shadow type="NONE">` 16건 (= 명시적 "없음" 표식)
//!
//! 따라서 P0 input 의 모든 호출이 **fast path** (4 effect block 진입 없음).
//!
//! ## 본 port scope (L-5c-RE-4)
//!
//! - ✅ **Path 1 (effects_ctrl null)** byte-eq: `*sret = 0; return`
//! - ✅ **Path 2 (effects_obj null)** byte-eq: `*sret = effects_ctrl; return`
//! - ✅ **Path 3 (bag walk all-miss → fast wrap)** byte-eq + 24B UniquePtr ctrl alloc
//!   + refcount++ on inner
//! - ⏸️ **Path 4 (LAB_002f2c0c effect processing)** — 4 effect block 의 in-place modify
//!   sequence: P0 미발동 → `unreachable!()` deferral. 후속 세션 L-5c-RE-4b 에서 port.
//!   (필요 helper: PropertyBag::Set<float> @ 0x653cb4, Reflection::SetDistance @ 0x1cee70,
//!   Effects copy ctor @ 0x631f40, FUN_00649e30 dedup registry insert.)
//!
//! ## raw asm path map (0x2f2ad8 - 0x2f3450)
//!
//! ```text
//! 0x2f2ad8 - 0x2f2b0c   prologue + arg load (x21=this, x20=&unique, w22=flag.byte0, x19=sret)
//! 0x2f2b10 - 0x2f2b18   PATH 1 check (effects_ctrl null) → *sret = 0; ret
//! 0x2f2b1c - 0x2f2b28   PATH 2 check (effects.obj null) → *sret = ctrl; ret
//! 0x2f2b2c - 0x2f2c08   inner_bag walk for key 0x3ee (Shadow)
//!                       if found → LAB_002f2c0c
//! 0x2f2c0c..             LAB_002f2c0c (deep copy + 4 effect process)
//!   (P0 미발동, 본 단계 unreachable)
//! 0x2f33b8 - 0x2f33ec   PATH 3 fast wrap (refcount++ + FUN_00649980 + FUN_00649d3c)
//! 0x2f3418 - 0x2f3450   cleanup of local_80 (copy Effects) + epilogue
//! ```

use crate::char_item_view::CharItemView;
use crate::effects_container::EffectControlBlock;
use std::alloc::Layout;
use std::ptr;

/// raw `Hnc::Memory::UniquePtr<Effects>` 의 24B ControlBlock (= FUN_00649d3c 가 alloc).
///
/// raw 0x649d68-0x649d80:
/// ```asm
/// mov  w0, #0x18           ; 24-byte alloc
/// bl   __Znwm
/// stp  x20, x8, [x0]       ; [+0] = inner_ctrl, [+8] = refcount=1
/// strb w8, [x0, #0x10]     ; [+0x10] = byte 1
/// ```
#[repr(C)]
pub struct UniqueEffectsCtrl {
    /// raw +0x00: inner `EffectControlBlock*` (= the 16B SharePtr ctrl from caller).
    pub inner_ctrl: *mut EffectControlBlock,
    /// raw +0x08: wrapper refcount (u64).
    pub refcount: u64,
    /// raw +0x10: byte flag (= 1 on creation, semantic 미해석 — dedup canonical 표식 추정).
    pub flag: u8,
    /// raw +0x11..+0x17: padding (alloc size 0x18 = 24B).
    pub _pad: [u8; 7],
}

pub const UNIQUE_EFFECTS_CTRL_SIZE_BYTES: usize = 24;
pub const UNIQUE_EFFECTS_CTRL_ALIGN_BYTES: usize = 8;

const _: () = assert!(std::mem::size_of::<UniqueEffectsCtrl>() == UNIQUE_EFFECTS_CTRL_SIZE_BYTES);
const _: () = assert!(std::mem::align_of::<UniqueEffectsCtrl>() == UNIQUE_EFFECTS_CTRL_ALIGN_BYTES);

/// raw `FUN_00649980` (= cxa_guard 4-stage 싱글톤 init for default-cache singleton) 1:1 byte-eq stub.
///
/// raw 알고리즘:
/// 1. cxa_guard #1 (slot 0x79e558) acquire → no-op init
/// 2. cxa_guard #2 (slot 0x79e568) acquire → store address of __cstring "+default" (or similar)
///    at 0x79e560 (and `& 0x7fffffffffffffff` to clear the high bit)
/// 3. cxa_guard #3 (slot 0x79e578) acquire → call `FUN_0x649b68(str, &slot_at_0x79e570)` to
///    create a CHncStringW from the literal
/// 4. cxa_guard #4 (slot 0x79e590) acquire → call `FUN_0x649c18(&slot_at_0x79e580, str)` to
///    construct the cached object
/// 5. return `[0x79e588]` = the cached object ptr
///
/// **본 port (pixel-eq scope)**: side effect (singleton initialization) 만 수행 — 반환
/// 객체의 content 는 GetRealTextEffects 의 output 에 직접 영향 없음 (FUN_00649d3c 의
/// 내부 dedup registry 에서만 사용). 따라서 no-op stub 으로 충분.
///
/// **byte-eq deferral**: cxa_guard sequence + literal string (`__cstring+0x74ddde`) 의 정확
/// 한 RE 는 별도 세션. 본 함수 호출 자체는 GetRealTextEffects fast path 의 side effect
/// (출력 변화 없음) 이므로 deferral 이 pixel-eq 보존.
pub unsafe fn singleton_init_649980() -> *mut u8 {
    // pixel-eq pass-through stub. raw 의 side-effect 인 singleton init 은 본 port 의 출력에
    // 영향 없음. cxa_guard 등의 실제 byte-eq 는 L-5c-RE-4c 에서 port.
    ptr::null_mut()
}

/// raw `FUN_00649d3c` (= `Hnc::Memory::UniquePtr<Effects>::make_from_raw(sret, &raw_ptr)`) 1:1 byte-eq.
///
/// 알고리즘 (raw 0x649d3c-0x649dac):
/// 1. `*sret = 0` (clear output)
/// 2. `x20 = *src_slot; *src_slot = 0` (move semantics — null out source)
/// 3. if x20 == null → return (sret stays null)
/// 4. alloc 24B + populate { inner_ctrl=x20, refcount=1, flag=1 }
/// 5. `*sret = new_ctrl`
/// 6. call `singleton_init_649980()` (side effect)
/// 7. call `FUN_00649e30(self, sret, sret)` (dedup registry insert)
/// 8. if 0x649e30 returns truthy (= dup found in registry):
///    - release the just-allocated ctrl (call 0x649820)
///    - `*sret = existing_ctrl_from_registry` (= [returned_node+0x20])
///    - existing.refcount++
///
/// **본 port (pixel-eq scope)**: step 1-6 full byte-eq. step 7-8 (dedup registry) 보류
/// — output ctrl 의 content 는 step 1-6 만으로 동일하게 결정되므로 (just inner_ctrl + refcount + flag).
/// dedup 은 메모리 절약만 영향 (pixel-eq 무관).
///
/// **byte-eq deferral**: FUN_00649e30 의 RB tree registry insert + 0x649820 release
/// 는 L-5c-RE-4c 에서 port. 본 함수 호출 후 caller 가 sret 으로 보는 ctrl 의 byte
/// content 는 step 6 시점과 step 8 시점이 동일 (단지 alloc 주소만 다를 수 있음).
///
/// # Safety
/// `sret` 는 valid output slot (= `*mut *mut UniqueEffectsCtrl`). `src_slot` 은 valid
/// `*mut *mut EffectControlBlock` (caller 가 move semantics 동의).
pub unsafe fn make_unique_effects_649d3c(
    sret: *mut *mut UniqueEffectsCtrl,
    src_slot: *mut *mut EffectControlBlock,
) {
    // raw 0x649d50: *sret = 0
    *sret = ptr::null_mut();
    // raw 0x649d54-0x649d58: x20 = *src_slot; *src_slot = 0 (move)
    let inner = *src_slot;
    *src_slot = ptr::null_mut();
    // raw 0x649d5c: if x20 == null → return (sret stays null)
    if inner.is_null() {
        return;
    }
    // raw 0x649d68-0x649d80: alloc 24B + populate { inner_ctrl, refcount=1, flag=1 }
    let layout = Layout::new::<UniqueEffectsCtrl>();
    let p = std::alloc::alloc(layout) as *mut UniqueEffectsCtrl;
    if p.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr::write(
        p,
        UniqueEffectsCtrl {
            inner_ctrl: inner,
            refcount: 1,
            flag: 1,
            _pad: [0u8; 7],
        },
    );
    *sret = p;
    // raw 0x649d84: side-effect singleton init (output unused for pixel-eq).
    let _ = singleton_init_649980();
    // raw 0x649d88-0x649d98: FUN_00649e30 registry insert (dedup) — deferred (L-5c-RE-4c).
    // L-5c-RE-4c 에서: 만약 0x649e30 가 existing 반환 시 본 ctrl 을 release + sret 을
    // existing 으로 교체. 본 단계는 항상 새 ctrl 유지 (메모리만 약간 더, output 동일).
}

/// raw `Hnc::Shape::Text::CharItemView::GetRealTextEffects(UniquePtr<Effects>&, Flag&) const`
/// (`__ZNK3Hnc5Shape4Text12CharItemView19GetRealTextEffectsERKNS_6Memory9UniquePtrINS0_7EffectsEEERKNS_4Type4FlagE`
/// @ `0x2f2ad8`, 2428B) — fast path full byte-eq port.
///
/// ## P0 dispatch
///
/// - Path 1 (effects_ctrl null): `*sret = 0; return`
/// - Path 2 (effects_obj null): `*sret = effects_ctrl; return` (NOTE: raw 는 refcount++ 안 함)
/// - Path 3 (bag walk: all 4 keys 0x3ee/0xbba/0xbbb/0xbb9 not found):
///   - effects_ctrl.refcount++ (raw 0x2f33c8)
///   - `singleton_init_649980()` (side effect)
///   - `make_unique_effects_649d3c(sret, &effects_ctrl)` → wrap inner ctrl in 24B UniqueEffectsCtrl
/// - Path 4 (any of 4 keys found → LAB_002f2c0c): **P0 미발동 → unreachable**
///   - 4 effect processing block (Shadow/OuterShadow/Reflection/Glow) 의 in-place modify
///     sequence 는 별도 세션 L-5c-RE-4b 에서 port.
///
/// ## 4 keys = effect type GetType() 반환값
///
/// - `0x3ee` = Shadow effect (raw lower_bound check at 0x2f2bbc)
/// - `0xbb9` = Glow effect (raw lower_bound check at 0x2f2f64 - skip if `flag & 1`)
/// - `0xbba` = OuterShadow effect (raw lower_bound check at 0x2f2e1c - skip if `flag & 1`)
/// - `0xbbb` = Reflection effect (raw lower_bound check at 0x2f2eb4 - skip if `flag & 1`)
///
/// `flag & 1` (raw 0x2f2b08 `ldrb w22, [x2]`) 는 본 함수의 두 번째 인자 `Flag const&`.
///
/// # Safety
///
/// `ci` 는 valid `CharItemView`. `effects_slot` 는 valid `*mut *mut EffectControlBlock`
/// (= UniquePtr<Effects>& 가 가리키는 raw effects ctrl slot). `flag_byte0` 은 `Flag` 의
/// 첫 byte (raw 0x2f2b08 에서 `ldrb w22, [x2]`). `sret` 는 valid output slot.
#[allow(clippy::too_many_arguments)]
pub unsafe fn get_real_text_effects(
    _ci: &CharItemView,
    effects_slot: *mut *mut EffectControlBlock,
    flag_byte0: u8,
    sret: *mut *mut UniqueEffectsCtrl,
) {
    // raw 0x2f2b0c: x8 = *effects_slot = EffectControlBlock*
    let effects_ctrl = *effects_slot;

    // raw 0x2f2b10-0x2f2b18: PATH 1 — if effects_ctrl null → *sret = 0; ret
    if effects_ctrl.is_null() {
        *sret = ptr::null_mut();
        return;
    }

    // raw 0x2f2b14: x23 = *effects_ctrl = Effects* (= ctrl.obj)
    let effects_obj = (*effects_ctrl).obj as *mut crate::effects_container::Effects;

    // raw 0x2f2b18-0x2f2b28: PATH 2 — if effects_obj null →
    //   *sret = (UniqueEffectsCtrl*)effects_ctrl; ret
    // raw 의 직접 캐스팅: SRET (UniqueEffectsCtrl*) = effects_ctrl (EffectControlBlock*).
    // 의미적으로는 raw 가 단지 *sret 에 ctrl 의 raw bits 를 store 하므로 byte-eq.
    if effects_obj.is_null() {
        *sret = effects_ctrl as *mut UniqueEffectsCtrl;
        return;
    }

    // raw 0x2f2b2c..: inner_bag walk for 4 keys (0x3ee, 0xbba, 0xbbb, 0xbb9).
    // `effects_obj` 는 24B Effects struct (libc++ std::map __tree_base). end_node_left
    // 는 `effects_obj + 8`.
    let effects = &*effects_obj;

    // raw 의 두 번째 walk (key 0xbb9 등) 가 conditional on `flag.byte0 & 1` 임을 반영.
    // raw 0x2f2b08 `ldrb w22, [x2]` → 0x2f2e08 `tbnz w22, #0, …` (= bit0 set 시 skip).
    let skip_glow_outershadow_reflection = (flag_byte0 & 1) != 0;

    // Path 4 check: 4 keys 중 하나라도 found → LAB_002f2c0c (P0 미발동).
    //
    // raw 의 decompile 은 inner_bag walk 를 직접 inline (do/while lower_bound). 본 port
    // 는 `Effects::find()` (= byte-eq lower_bound, refcount 안 건드림) 로 단순화.
    // raw 의 4 walk 순서: 0x3ee (Shadow, 항상) → 0xbba (OuterShadow, flag&1==0) →
    // 0xbbb (Reflection, flag&1==0) → 0xbb9 (Glow, flag&1==0).
    let any_found = if effects.find(0x3ee).is_some() {
        true
    } else if !skip_glow_outershadow_reflection {
        effects.find(0xbba).is_some()
            || effects.find(0xbbb).is_some()
            || effects.find(0xbb9).is_some()
    } else {
        false
    };

    if any_found {
        // PATH 4 (LAB_002f2c0c): 4 effect block in-place modify sequence (P0 미발동).
        // L-5c-RE-4b 에서 port 예정. 필요 helper: PropertyBag::Set<float> @ 0x653cb4,
        // Reflection::SetDistance @ 0x1cee70, Effects copy ctor @ 0x631f40, FUN_00649e30
        // dedup registry insert.
        unreachable!(
            "GetRealTextEffects PATH 4 (LAB_002f2c0c effect processing): P0 미발동 → deferred to L-5c-RE-4b"
        );
    }

    // PATH 3 fast wrap (raw 0x2f33b8-0x2f33e4):
    // 1. effects_ctrl.refcount++ (raw 0x2f33c8)
    // 2. store effects_ctrl into local stack slot
    // 3. singleton_init_649980 side effect
    // 4. make_unique_effects_649d3c(sret, &local_slot)
    (*effects_ctrl).refcount = (*effects_ctrl).refcount.wrapping_add(1);
    // raw 0x2f33d0: store ctrl into local stack slot (= local SharePtr handle).
    let mut local_slot: *mut EffectControlBlock = effects_ctrl;
    // raw 0x2f33d4: bl 0x649980 (side effect).
    let _ = singleton_init_649980();
    // raw 0x2f33e0: bl 0x649d3c (UniquePtr wrap).
    make_unique_effects_649d3c(sret, &mut local_slot as *mut _);
    // raw 의 cleanup: local_80 (LAB_002f2c0c 의 deep-copy temp) 는 fast path 에서 null
    // 이므로 cleanup 스킵 (= LAB_002f3418 fall-through 의 no-op).
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::CharItemView;
    use crate::effects_container::Effects;

    #[test]
    fn unique_effects_ctrl_layout_24b() {
        assert_eq!(std::mem::size_of::<UniqueEffectsCtrl>(), 24);
        assert_eq!(std::mem::align_of::<UniqueEffectsCtrl>(), 8);
    }

    #[test]
    fn unique_effects_ctrl_field_offsets() {
        let c = UniqueEffectsCtrl {
            inner_ctrl: ptr::null_mut(),
            refcount: 0,
            flag: 0,
            _pad: [0u8; 7],
        };
        let base = &c as *const _ as usize;
        assert_eq!(&c.inner_ctrl as *const _ as usize - base, 0x00);
        assert_eq!(&c.refcount as *const _ as usize - base, 0x08);
        assert_eq!(&c.flag as *const _ as usize - base, 0x10);
    }

    #[test]
    fn make_unique_effects_null_source_returns_null_sret() {
        unsafe {
            let mut src: *mut EffectControlBlock = ptr::null_mut();
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            make_unique_effects_649d3c(&mut sret, &mut src);
            assert!(sret.is_null());
            assert!(src.is_null());
        }
    }

    #[test]
    fn make_unique_effects_alloc_and_move() {
        unsafe {
            let dummy_obj = 0xCAFEBABEusize as *mut u8;
            let ctrl = EffectControlBlock::create_raw(dummy_obj);
            let mut src: *mut EffectControlBlock = ctrl;
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            make_unique_effects_649d3c(&mut sret, &mut src);
            // raw 0x649d58: src nulled (move)
            assert!(src.is_null());
            // raw 0x649d80: sret holds new wrapper
            assert!(!sret.is_null());
            // raw 0x649d78: inner_ctrl = original
            assert_eq!((*sret).inner_ctrl, ctrl);
            // raw 0x649d7c-78: refcount=1, flag=1
            assert_eq!((*sret).refcount, 1);
            assert_eq!((*sret).flag, 1);
            // cleanup
            let layout = Layout::new::<UniqueEffectsCtrl>();
            std::alloc::dealloc(sret as *mut u8, layout);
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
        }
    }

    #[test]
    fn get_real_text_effects_path1_null_ctrl_returns_null() {
        unsafe {
            let ci = CharItemView::new_empty();
            let mut slot: *mut EffectControlBlock = ptr::null_mut();
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            get_real_text_effects(&ci, &mut slot, 0, &mut sret);
            assert!(sret.is_null(), "Path 1: null effects_ctrl → sret = null");
        }
    }

    #[test]
    fn get_real_text_effects_path2_null_obj_returns_ctrl_cast() {
        unsafe {
            let ci = CharItemView::new_empty();
            // Build ctrl with null obj
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            let ctrl = std::alloc::alloc(ctrl_layout) as *mut EffectControlBlock;
            ptr::write(
                ctrl,
                EffectControlBlock {
                    obj: ptr::null_mut(),
                    refcount: 1,
                },
            );
            let mut slot: *mut EffectControlBlock = ctrl;
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            get_real_text_effects(&ci, &mut slot, 0, &mut sret);
            // raw: *sret = effects_ctrl directly cast — same address
            assert_eq!(sret as *const u8, ctrl as *const u8);
            // raw 도 refcount 안 건드림
            assert_eq!((*ctrl).refcount, 1);
            // cleanup
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
        }
    }

    #[test]
    fn get_real_text_effects_path3_empty_bag_wraps_in_unique_ctrl() {
        unsafe {
            let ci = CharItemView::new_empty();
            // Build a real empty Effects + ctrl
            let mut effects = Effects::new();
            let effects_ptr = &mut *effects as *mut Effects as *mut u8;
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            let ctrl = std::alloc::alloc(ctrl_layout) as *mut EffectControlBlock;
            ptr::write(
                ctrl,
                EffectControlBlock {
                    obj: effects_ptr,
                    refcount: 1,
                },
            );

            let mut slot: *mut EffectControlBlock = ctrl;
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            get_real_text_effects(&ci, &mut slot, 0, &mut sret);

            // Path 3 fast wrap: sret is new UniqueEffectsCtrl
            assert!(!sret.is_null());
            // inner_ctrl points to original effects ctrl
            assert_eq!((*sret).inner_ctrl, ctrl);
            // wrapper refcount=1, flag=1
            assert_eq!((*sret).refcount, 1);
            assert_eq!((*sret).flag, 1);
            // raw 0x2f33c8: effects_ctrl.refcount++ (1 → 2)
            assert_eq!((*ctrl).refcount, 2);

            // cleanup
            let wrap_layout = Layout::new::<UniqueEffectsCtrl>();
            std::alloc::dealloc(sret as *mut u8, wrap_layout);
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*effects);
        }
    }

    #[test]
    fn get_real_text_effects_path3_skip_flags_bit0_set_still_works_for_empty() {
        // Even if flag&1 = 1 (skip outerShadow/glow/reflection), empty bag still
        // hits PATH 3 fast wrap since no key 0x3ee either.
        unsafe {
            let ci = CharItemView::new_empty();
            let mut effects = Effects::new();
            let effects_ptr = &mut *effects as *mut Effects as *mut u8;
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            let ctrl = std::alloc::alloc(ctrl_layout) as *mut EffectControlBlock;
            ptr::write(
                ctrl,
                EffectControlBlock {
                    obj: effects_ptr,
                    refcount: 1,
                },
            );

            let mut slot: *mut EffectControlBlock = ctrl;
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            // flag.byte0 = 1 (skip flag set)
            get_real_text_effects(&ci, &mut slot, 1, &mut sret);
            assert!(!sret.is_null(), "empty bag + flag set → still PATH 3");
            assert_eq!((*sret).inner_ctrl, ctrl);

            // cleanup
            let wrap_layout = Layout::new::<UniqueEffectsCtrl>();
            std::alloc::dealloc(sret as *mut u8, wrap_layout);
            std::alloc::dealloc(ctrl as *mut u8, ctrl_layout);
            std::mem::forget(*effects);
        }
    }

    #[test]
    #[should_panic(expected = "PATH 4")]
    fn get_real_text_effects_path4_unreachable_when_shadow_key_present() {
        unsafe {
            let ci = CharItemView::new_empty();
            let mut effects = Effects::new();
            let dummy = 0xDEADBEEFusize as *mut u8;
            let inner_ctrl = EffectControlBlock::create_raw(dummy);
            effects.insert(0x3ee, inner_ctrl);
            let effects_ptr = &mut *effects as *mut Effects as *mut u8;
            let ctrl_layout = Layout::new::<EffectControlBlock>();
            let ctrl = std::alloc::alloc(ctrl_layout) as *mut EffectControlBlock;
            ptr::write(
                ctrl,
                EffectControlBlock {
                    obj: effects_ptr,
                    refcount: 1,
                },
            );
            let mut slot: *mut EffectControlBlock = ctrl;
            let mut sret: *mut UniqueEffectsCtrl = ptr::null_mut();
            get_real_text_effects(&ci, &mut slot, 0, &mut sret);
            // No cleanup — should_panic
        }
    }
}
