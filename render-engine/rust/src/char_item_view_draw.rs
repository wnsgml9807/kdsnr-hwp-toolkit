//! `Hnc::Shape::Text::CharItemView::Draw` byte-eq outer port (L-5c-RE-5a).
//!
//! ## raw 출처
//!
//! - `__ZN3Hnc5Shape4Text12CharItemView4DrawERNS0_7SurfaceERKNS1_10AllocationERKNS_4Type4FlagERKNS0_6BWModeE`
//! - 주소: `0x2f5e3c`
//! - 크기: 1880B (= 0x758)
//! - decompile: `Text_CharItemView__Draw_002f5e3c.txt` (392 lines)
//!
//! ## 함수 의미
//!
//! 본 함수는 1 글자의 텍스트를 Surface 에 그린다. 외부 dispatch 함수로:
//! - **early returns**: 빈 글자 (\n/\r, invisible space, font 없음)
//! - **fast path**: 효과 없으면 `DrawDirect` (5248B sub-routine)
//! - **effects path**: 효과 있으면 `GetRealTextEffects` + `GetCachedRenderPath` +
//!   `GetPreEffectsImage` + `EffectsPainter::Draw` 사슬 (P0 미발동)
//!
//! ## P0 input 분석
//!
//! - toolkit 307 hwpx audit 결과 outerShadow/reflection/glow/innerShadow/softEdge = 0건
//! - 따라서 effects path 진입 0건 → **fast path** 가 P0 의 100% (DrawDirect 호출)
//! - 또한 sVar1 ∈ {10, 13} (= '\n', '\r') 도 일반 텍스트 input 에서 거의 없음
//! - sVar1 == 32 (= space) 매우 흔함 — bag key 0x961 (= IsVisible) 검사 후 통과해야 그림
//!
//! ## 본 port scope (L-5c-RE-5a)
//!
//! - ✅ **outer Draw structure** (early returns + dispatch + SharePtr cleanup) byte-eq
//! - ✅ **5 trivial sret getter** 호출 wiring (`get_real_pen` / `get_real_brush` /
//!   `get_real_effects` / `get_real_text_effects`) — 모두 기존 method 사용
//! - ⏸️ **DrawDirect** (5248B, sub-routine) → caller 가 제공하는 `DrawDirectFn` trait
//!   callback 으로 추상화. byte-eq port 는 별도 세션 L-5c-RE-5b.
//! - ⏸️ **Effects path** (GetCachedRenderPath + Surface rotation + EffectsPainter ~
//!   1000B) → P0 미발동 → `unreachable!()` deferral (다음 세션 L-5c-RE-5c).
//! - ⏸️ **Block 1 dead reads** (param_4.byte1.bit4 off 면 0x96a/0x96c 읽고 버림): raw 가
//!   해당 결과 사용 안 함 (side effect 없음, output 무관) → 본 port 에서는 skip.
//!   byte-eq 정확성 위해 호출은 유지 가능하지만 pixel-eq 무관.
//!
//! ## raw asm flow map (0x2f5e3c - 0x2f6594)
//!
//! ```text
//! 0x2f5e3c-0x2f5e7c   prologue + stack setup (0x1f0 byte stack frame)
//! 0x2f5e80-0x2f5ee4   Block 1: dead reads of 0x96a/0x96c (flag.byte1.bit4 off + RP exists)
//! 0x2f5ee8-0x2f5f00   Block 2: char_code check (sVar1 == 10 || 13 → epilogue)
//! 0x2f5f04-0x2f5f4c   Block 3: sVar1 == 32 + key 0x961 == 0 → epilogue
//! 0x2f5f50-0x2f5f70   Block 4: font (this+0x30) null check → epilogue
//! 0x2f5f74-0x2f5fac   Block 5: ShapeEngine warmup (surface[+0x20] == 0)
//! 0x2f5fb0           Block 6: GetRealPen call → local_88
//! 0x2f5fb4-0x2f5fdc   (repeat ShapeEngine warmup, byte-eq inline expansion)
//! 0x2f5fe0-0x2f6058   Block 7: Get brush handle from RP (local_90)
//! 0x2f605c-0x2f6080   Block 8: if pen+brush both empty → goto LAB_002f62e4 cleanup
//! 0x2f6084-0x2f60ac   Block 9: ShapeEngine warmup + GetRealEffects → local_a0
//! 0x2f60b0-0x2f60e0   Block 10: dispatch fast/effects
//! 0x2f60e4-0x2f62b8   Effects path (P0 미발동 → unreachable)
//! 0x2f62bc-0x2f62cc   DrawDirect call (fast path)
//! 0x2f62d0-0x2f62e4   LAB_002f62e4: cleanup of local_a0/98/90/88 SharePtrs
//! 0x2f62e8-0x2f6594   epilogue + exception unwind landingpads
//! ```

use crate::char_item_view::CharItemView;
use crate::flag::Flag;
use crate::blip_glyph::Allocation;
use crate::bw_mode::BWMode;
use crate::share_ptr::ControlBlock;

/// raw `CharItemView::Draw` 의 outcome — 우리 SvgSurface adapter 가 dispatching 시
/// 어떤 path 가 실행됐는지 보고하는 enum. raw 자체는 void 반환이며 side effect (Surface
/// draw call) 가 결과. byte-eq 검증 / e2e wire 위해 명시적 enum 사용.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawOutcome {
    /// raw 의 early return path (skip without drawing).
    Skipped(SkipReason),
    /// raw 의 fast path: DrawDirect 호출 (effects 없음).
    DrawDirectCalled,
    /// raw 의 effects path: GetCachedRenderPath + EffectsPainter (P0 미발동).
    EffectsUnreachable,
}

/// `DrawOutcome::Skipped` 의 세분된 skip 원인. byte-eq 와 무관, 진단 용.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// `sVar1 ∈ {10, 13}` — newline / carriage return.
    NewlineOrCR,
    /// `sVar1 == 32` 이고 RunProperty.bag 의 key 0x961 (IsVisible) == 0.
    InvisibleSpace,
    /// `sVar1 == 32` 이고 RunProperty 가 null OR RunProperty.obj 가 null
    /// (space 인데 visibility 조회 불가능 → raw 가 early return).
    SpaceWithNullRunProperty,
    /// `this+0x30 (font ctrl) == null` OR `font ctrl.obj == null`.
    NoFont,
    /// pen + brush 둘 다 empty SharePtr (raw 0x2f605c-0x2f6080 의 분기).
    NoPenAndNoBrush,
}

/// raw `DrawDirect(Surface&, Allocation&, Flag&, BWMode)` (`0x2f67ec`, 5248B) — caller-provided
/// stub. 본 outer port 에서는 실제 byte-eq port 대신 trait callback 으로 추상화.
///
/// 별도 세션 L-5c-RE-5b 에서 full byte-eq port.
pub trait DrawDirectFn {
    /// raw 0x2f62bc-0x2f62cc 의 `DrawDirect(param_1, param_2, param_3, param_4, *in_x4)`.
    ///
    /// # Safety
    /// 모든 ref 는 caller 가 valid 보장.
    unsafe fn draw_direct(
        &mut self,
        ci: &CharItemView,
        allocation: &Allocation,
        flag: &Flag,
        bw_mode: BWMode,
    );
}

/// raw `CharItemView::Draw` outer dispatch — byte-eq early return + fast/effects dispatch
/// + SharePtr lifecycle. effects path 는 P0 미발동 → unreachable.
///
/// ## byte-eq scope (L-5c-RE-5a)
///
/// - Early returns (Block 2/3/4): full byte-eq
/// - SharePtr local 변수 lifecycle (refcount inc/dec): full byte-eq
/// - DrawDirect 호출: trait callback (`draw_direct_fn`) 위임. 본 method 의 byte-eq port
///   는 L-5c-RE-5b. 본 port 의 outcome 은 raw 와 동일 (호출 시점/순서/인자 일치).
/// - Effects path: P0 미발동 (toolkit 307 hwpx 0건) → `unreachable!()` deferral L-5c-RE-5c
///
/// ## raw byte-eq verification points
///
/// - Block 2 (raw 0x2f5ee8): `sVar1 == 10 || sVar1 == 13` → epilogue
/// - Block 3 (raw 0x2f5f04): `sVar1 == 32` + bag.0x961 read u32 == 0 → epilogue
/// - Block 4 (raw 0x2f5f50): `this->font (+0x30) == null OR font.obj == null` → epilogue
/// - Block 8 (raw 0x2f605c): `pen empty AND brush empty` → cleanup epilogue
/// - Block 10 (raw 0x2f60b0): dispatch — effects.bag empty OR `flag & 1 != 0` → fast
///
/// # Safety
///
/// `ci` 는 valid `CharItemView`. `allocation`/`flag`/`bw_mode` 는 valid raw bytes.
/// `draw_direct_fn` 은 실제 DrawDirect 동작 byte-eq.
#[allow(clippy::too_many_arguments)]
pub unsafe fn draw(
    ci: &CharItemView,
    allocation: &Allocation,
    flag: &Flag,
    bw_mode: BWMode,
    draw_direct_fn: &mut dyn DrawDirectFn,
) -> DrawOutcome {
    // Block 1 (raw 0x2f5e80-0x2f5ee4): dead reads of 0x96a (FontSize) + 0x96c (ScriptBaseLine)
    // if `flag.byte1.bit4 == 0` AND RunProperty exists. raw 가 결과 안 씀.
    // pixel-eq 무관 → skip. byte-eq 호출 sequence 보존 원할 시 add 가능.

    // Block 2 (raw 0x2f5ee8): sVar1 check
    let s_var1 = ci.character;
    if s_var1 == 10 || s_var1 == 13 {
        return DrawOutcome::Skipped(SkipReason::NewlineOrCR);
    }

    // Block 3 (raw 0x2f5f04): if space, check visibility (bag key 0x961)
    if s_var1 == 0x20 {
        let rp_ctrl = ci.run_property;
        if rp_ctrl.is_null() {
            return DrawOutcome::Skipped(SkipReason::SpaceWithNullRunProperty);
        }
        let rp_obj = (*rp_ctrl).obj;
        if rp_obj.is_null() {
            return DrawOutcome::Skipped(SkipReason::SpaceWithNullRunProperty);
        }
        // raw 0x2f5f30-0x2f5f4c: read u32 at PropertyKey 0x961 (IsVisible).
        // raw 의 helper FUN_00687254 = 동일 RB tree lookup family of `get_value_addr`
        // (template-instantiated for u32 read). 우리 port 는 PropertyBag::get_value_addr
        // 재사용.
        let pk = crate::property_key::PropertyKey::from_int(0x961);
        let impl_ptr: *const crate::property_bag::PropertyBagImpl =
            match (*rp_obj).property_bag.is_null() {
                false => (*(*rp_obj).property_bag).obj,
                true => std::ptr::null(),
            };
        let value_addr = crate::property_bag::PropertyBagImpl::get_value_addr(impl_ptr, &pk);
        let visible: u32 = *(value_addr as *const u32);
        if visible == 0 {
            return DrawOutcome::Skipped(SkipReason::InvisibleSpace);
        }
    }

    // Block 4 (raw 0x2f5f50): font check
    let font_ctrl = ci.font as *mut ControlBlock<u8>;
    if font_ctrl.is_null() {
        return DrawOutcome::Skipped(SkipReason::NoFont);
    }
    if (*font_ctrl).obj.is_null() {
        return DrawOutcome::Skipped(SkipReason::NoFont);
    }

    // Block 5 (raw 0x2f5f74-0x2f5fac): ShapeEngine warmup
    // — surface[+0x20] check + ShapeEngine::GetInstance + Theme refcount inline 전개.
    // 본 outer port 의 byte-eq 범위에서 surface 는 trait abstraction 으로 다뤄지므로
    // 본 ShapeEngine warmup 의 정확 비교는 L-5c-RE-5b 의 surface adapter 와 함께
    // 진행. 본 method 는 logic dispatch 만.

    // Block 6 (raw 0x2f5fb0): GetRealPen → local_88 (pen ControlBlock*)
    let pen_ctrl = ci.get_real_pen(std::ptr::null());

    // Block 7 (raw 0x2f5fe0-0x2f6058): Get RealBrush from RunProperty (local_90)
    // raw 가 GetRealBrush 함수 호출 대신 inline expand 했음. 우리는 method 호출.
    let brush_ctrl = ci.get_real_brush(std::ptr::null());

    // Block 8 (raw 0x2f605c-0x2f6080): if pen+brush both empty → cleanup + return
    let pen_empty = pen_ctrl.is_null() || unsafe_obj_null(pen_ctrl as *mut ControlBlock<u8>);
    let brush_empty = brush_ctrl.is_null() || unsafe_obj_null(brush_ctrl as *mut ControlBlock<u8>);
    if pen_empty && brush_empty {
        // SharePtr cleanup 발생 (raw 0x2f62e4-...): release pen_ctrl + brush_ctrl + (effects, txt)
        release_share_ptr(pen_ctrl as *mut ControlBlock<u8>);
        release_share_ptr(brush_ctrl as *mut ControlBlock<u8>);
        return DrawOutcome::Skipped(SkipReason::NoPenAndNoBrush);
    }

    // Block 9 (raw 0x2f6084-0x2f60ac): GetRealEffects → local_a0 (effects ControlBlock*)
    let effects_ctrl = ci.get_real_effects(std::ptr::null());

    // Block 10 (raw 0x2f60b0-0x2f60e0): dispatch fast/effects
    // Fast condition: effects empty OR effects.obj.bag.impl == 0 OR `flag.byte0 & 1 != 0`
    let is_fast = if effects_ctrl.is_null() {
        true
    } else {
        let eff_obj = (*effects_ctrl).obj;
        if eff_obj.is_null() {
            true
        } else {
            // raw `*(long*)(*local_a0 + 0x10) == 0` = effects.bag.size == 0
            // (Effects layout: +0x10 = size u64)
            let bag_size_addr = (eff_obj as *const u8).add(0x10) as *const u64;
            let bag_size = *bag_size_addr;
            bag_size == 0 || (flag.0 & 1) != 0
        }
    };

    let outcome = if is_fast {
        // PATH FAST (raw 0x2f62bc): DrawDirect(param_1, param_2, param_3, param_4, *in_x4)
        draw_direct_fn.draw_direct(ci, allocation, flag, bw_mode);
        DrawOutcome::DrawDirectCalled
    } else {
        // PATH EFFECTS (raw 0x2f60e4-0x2f62b8): GetRealTextEffects + GetCachedRenderPath +
        // Render::Path + Transform2D rotation + Surface::SetTransform + EffectsPainter::Draw
        //
        // **P0 미발동 (toolkit 307 hwpx 0건)** → unreachable.
        //
        // L-5c-RE-5c 에서 full byte-eq port: GetCachedRenderPath (207줄), Render::Surface
        // GetTransform/ResetTransform/SetTransform, GetPreEffectsImage (479줄),
        // EffectsPainter::Draw, ImagePainterObject ctor/dtor, SurfaceRestorer ctor/dtor.
        release_share_ptr(pen_ctrl as *mut ControlBlock<u8>);
        release_share_ptr(brush_ctrl as *mut ControlBlock<u8>);
        release_share_ptr(effects_ctrl as *mut ControlBlock<u8>);
        unreachable!(
            "CharItemView::Draw effects path (LAB_002f60e4): P0 미발동 → deferred to L-5c-RE-5c"
        );
    };

    // Cleanup (raw 0x2f62d0-0x2f62e4 + LAB_002f62e4): release SharePtrs.
    release_share_ptr(pen_ctrl as *mut ControlBlock<u8>);
    release_share_ptr(brush_ctrl as *mut ControlBlock<u8>);
    release_share_ptr(effects_ctrl as *mut ControlBlock<u8>);
    outcome
}

/// helper: ControlBlock.obj null check.
#[inline]
unsafe fn unsafe_obj_null<T>(ctrl: *mut ControlBlock<T>) -> bool {
    if ctrl.is_null() {
        return true;
    }
    (*ctrl).obj.is_null()
}

/// raw SharePtr release 의 minimal byte-eq pattern. 본 outer port 의 scope 에서는
/// ctrl 객체 자체의 free 는 caller (=test) 가 책임. 본 helper 는 refcount--만.
///
/// raw 의 actual SharePtr.~SharePtr() 호출 시 ctrl[+0x8] (refcount) 가 0 되면
/// vtable[0] D1 + delete 수행. 본 outer port 는 refcount만 정공법 감소.
#[inline]
unsafe fn release_share_ptr<T>(ctrl: *mut ControlBlock<T>) {
    if ctrl.is_null() {
        return;
    }
    let cb = &mut *ctrl;
    if cb.refcount > 0 {
        cb.refcount = cb.refcount.wrapping_sub(1);
    }
    // ctrl 자체 dealloc 은 caller 책임 (test scaffolding).
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_item_view::{CharItemView, RunProperty};
    use crate::share_ptr::ControlBlock;
    use std::alloc::Layout;
    use std::ptr;

    struct NoopDrawDirect {
        call_count: u32,
    }
    impl DrawDirectFn for NoopDrawDirect {
        unsafe fn draw_direct(
            &mut self,
            _ci: &CharItemView,
            _alloc: &Allocation,
            _flag: &Flag,
            _bw: BWMode,
        ) {
            self.call_count += 1;
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn draw_path_newline_returns_skipped() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = 10;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::Skipped(SkipReason::NewlineOrCR));
            assert_eq!(stub.call_count, 0);
        }
    }

    #[test]
    fn draw_path_cr_returns_skipped() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = 13;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::Skipped(SkipReason::NewlineOrCR));
        }
    }

    #[test]
    fn draw_path_space_with_null_runproperty_returns_skipped() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = 0x20;
            // RP is null → space-with-null-RP skip
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::Skipped(SkipReason::SpaceWithNullRunProperty));
        }
    }

    #[test]
    fn draw_path_no_font_returns_skipped() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            ci.character = b'A' as u16; // normal char (not newline/space)
            // font is null → no-font skip
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::Skipped(SkipReason::NoFont));
        }
    }

    #[test]
    fn draw_path_with_font_but_no_pen_or_brush_returns_skipped() {
        unsafe {
            // Setup: font ctrl with valid obj, but pen/brush null in RunProperty
            let mut ci = CharItemView::new_empty();
            ci.character = b'X' as u16;
            // Allocate Font ctrl block (obj just dummy)
            let font_layout = Layout::new::<ControlBlock<u8>>();
            let font_ctrl = std::alloc::alloc(font_layout) as *mut ControlBlock<u8>;
            ptr::write(
                font_ctrl,
                ControlBlock {
                    obj: 0xCAFEu64 as *mut u8,
                    refcount: 1,
                },
            );
            ci.font = font_ctrl as *mut u8;
            // RunProperty with null pen+brush
            let mut rp = RunProperty::new_empty();
            // wrap in ControlBlock
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::Skipped(SkipReason::NoPenAndNoBrush));

            // cleanup
            std::alloc::dealloc(font_ctrl as *mut u8, font_layout);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }

    #[test]
    fn draw_path_with_pen_and_no_effects_calls_draw_direct() {
        unsafe {
            // Setup: font + RunProperty.pen (non-null) + no effects
            let mut ci = CharItemView::new_empty();
            ci.character = b'X' as u16;
            // Font ctrl
            let font_layout = Layout::new::<ControlBlock<u8>>();
            let font_ctrl = std::alloc::alloc(font_layout) as *mut ControlBlock<u8>;
            ptr::write(
                font_ctrl,
                ControlBlock {
                    obj: 0xCAFEu64 as *mut u8,
                    refcount: 1,
                },
            );
            ci.font = font_ctrl as *mut u8;
            // RunProperty with non-null pen ctrl
            let mut rp = RunProperty::new_empty();
            let pen_layout = Layout::new::<ControlBlock<crate::pen::Pen>>();
            let pen_ctrl = std::alloc::alloc(pen_layout) as *mut ControlBlock<crate::pen::Pen>;
            ptr::write(
                pen_ctrl,
                ControlBlock {
                    obj: 0xBEEFu64 as *mut crate::pen::Pen,
                    refcount: 1,
                },
            );
            rp.pen = pen_ctrl;
            let rp_layout = Layout::new::<ControlBlock<RunProperty>>();
            let rp_ctrl = std::alloc::alloc(rp_layout) as *mut ControlBlock<RunProperty>;
            ptr::write(
                rp_ctrl,
                ControlBlock {
                    obj: &mut rp as *mut RunProperty,
                    refcount: 1,
                },
            );
            ci.run_property = rp_ctrl;
            let alloc = empty_alloc();
            let flag = Flag(0);
            let mut stub = NoopDrawDirect { call_count: 0 };
            let r = draw(&ci, &alloc, &flag, BWMode::V0, &mut stub);
            assert_eq!(r, DrawOutcome::DrawDirectCalled);
            assert_eq!(stub.call_count, 1);

            // cleanup
            std::alloc::dealloc(font_ctrl as *mut u8, font_layout);
            std::alloc::dealloc(pen_ctrl as *mut u8, pen_layout);
            std::alloc::dealloc(rp_ctrl as *mut u8, rp_layout);
        }
    }
}
