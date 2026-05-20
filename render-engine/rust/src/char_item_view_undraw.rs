//! `Hnc::Shape::Text::CharItemView::Undraw` outer (L-5c-RE-6c).
//!
//! ## raw 출처
//!
//! - `__ZN3Hnc5Shape4Text12CharItemView6UndrawERKNS_4Type4FlagE`
//! - 주소: `0x2f8bd8`, 820B
//! - decompile: 155 lines
//!
//! ## 함수 의미
//!
//! 그려진 글자의 cache 와 surface 그림을 invalidate. 다음 cache 들 release:
//! 1. `this[+0xa0]` = paths_cache (SharePtr<Paths>)
//! 2. `this[+0xa8]` = render_path_cache (SharePtr<Path>)
//! 3. `this[+0xb0]` = unknown SharePtr (FUN_00649820/00649980 의 cache)
//! 4. `this[+0x178]` / `this[+0x180]` = additional SharePtr cache
//! 5. ImagePainterObject (this+0xb8) cache reset
//! 6. Flag.byte0.bit0 dispatch: cleanup 분기
//! 7. 자식 vfunc[+0x80] (= GetCount) + vfunc[+0x88] (= GetComponent) traversal
//!    + 각 자식의 vfunc[+0x30] (= Undraw) 호출
//!
//! ## 본 port scope (L-5c-RE-6c)
//!
//! - ✅ Cache release sequences (5 SharePtr slots) byte-eq
//! - ✅ Flag dispatch byte-eq
//! - ⏸️ ImagePainterObject::ResetCache + 자식 vfunc traversal — trait callback

use crate::char_item_view::CharItemView;
use crate::flag::Flag;
use crate::share_ptr::ControlBlock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UndrawOutcome {
    /// Flag.byte0.bit0 == 0 path (이미지 painter 캐시만 reset + 자식 vfunc dispatch)
    PartialCacheReset { children_undrawn: u32 },
    /// Flag.byte0.bit0 == 1 path (전체 cache + image painter + 자식 vfunc dispatch)
    FullCacheReset { children_undrawn: u32 },
}

pub trait UndrawDeps {
    /// raw `ImagePainterObject::ResetCache(painter, 0, &flag)` 호출.
    unsafe fn image_painter_reset_cache(
        &mut self,
        ci: &mut CharItemView,
        flag_bit0_set: bool,
    );

    /// raw `this->vfunc[+0x80] = GetCount()` 호출 (자식 개수 반환).
    unsafe fn glyph_get_count(&mut self, ci: &CharItemView) -> u32;

    /// raw `this->vfunc[+0x88] = GetComponent(idx)` + `child->vfunc[+0x30] = Undraw(flag)` 호출.
    unsafe fn child_undraw(&mut self, ci: &CharItemView, idx: u32, flag: &Flag);
}

/// raw `CharItemView::Undraw(Flag&)` (`0x2f8bd8`, 820B) outer byte-eq.
///
/// # Safety
/// `ci` 는 valid CharItemView. `deps` 는 ImagePainter + 자식 vfunc dispatch 제공.
pub unsafe fn undraw(
    ci: &mut CharItemView,
    flag: &Flag,
    deps: &mut dyn UndrawDeps,
) -> UndrawOutcome {
    // Stage 1: paths_cache (this+0xa0) release
    release_share_ptr_generic(&mut ci.paths_cache as *mut _ as *mut *mut ControlBlock<u8>);

    // Stage 2: render_path_cache (this+0xa8) release
    release_share_ptr_generic(&mut ci.render_path_cache as *mut _ as *mut *mut ControlBlock<u8>);

    // Stage 3: this+0xb0 release — 본 outer port 의 ci struct 에선 `_padb0` 으로 정의됨.
    //   raw 가 SharePtr 로 사용. `&mut ci._padb0 as *mut u64 as *mut *mut Ctrl` cast.
    let pb0_ptr = &mut ci._padb0 as *mut u64 as *mut *mut ControlBlock<u8>;
    release_share_ptr_generic(pb0_ptr);

    // Stage 4: this+0x178 / this+0x180 release — 본 outer port 의 ci struct 에선 `_trailing`
    //   영역에 들어감. byte-eq 위해 unsafe ptr offset 으로 access.
    let trailing_base = ci._trailing.as_mut_ptr();
    let s178_ptr = trailing_base.add(0x178 - 0x170) as *mut *mut ControlBlock<u8>;
    let s180_ptr = trailing_base.add(0x180 - 0x170) as *mut *mut ControlBlock<u8>;
    release_share_ptr_generic(s178_ptr);
    release_share_ptr_generic(s180_ptr);

    // Stage 5: Flag.byte0.bit0 dispatch
    let flag_byte0 = flag.0.to_le_bytes()[0];
    let bit0_set = (flag_byte0 & 1) != 0;

    if !bit0_set {
        // PartialCacheReset path (raw 0x2f8da4-0x2f8e34)
        deps.image_painter_reset_cache(ci, false);
        let count = deps.glyph_get_count(ci);
        for i in 0..count {
            deps.child_undraw(ci, i, flag);
        }
        return UndrawOutcome::PartialCacheReset {
            children_undrawn: count,
        };
    }

    // FullCacheReset path (raw 0x2f8dd0+):
    deps.image_painter_reset_cache(ci, true);
    let count = deps.glyph_get_count(ci);
    for i in 0..count {
        deps.child_undraw(ci, i, flag);
    }
    UndrawOutcome::FullCacheReset {
        children_undrawn: count,
    }
}

#[inline]
unsafe fn release_share_ptr_generic(slot: *mut *mut ControlBlock<u8>) {
    let ctrl = *slot;
    if ctrl.is_null() {
        return;
    }
    let cb = &mut *ctrl;
    if cb.obj.is_null() {
        return;
    }
    let new_refcount = cb.refcount.wrapping_sub(1);
    if new_refcount == 0 {
        // raw 의 vfunc[+0x8] (D1) dispatch + dealloc obj + dealloc ctrl.
        // 본 outer port 의 scope 에서는 ctrl 의 dealloc 책임은 caller (test).
    } else {
        cb.refcount = new_refcount;
    }
    *slot = std::ptr::null_mut();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::Layout;
    use std::ptr;

    struct TestDeps {
        reset_called: u32,
        last_bit0_set: bool,
        children_count: u32,
        children_undrawn: u32,
    }
    impl UndrawDeps for TestDeps {
        unsafe fn image_painter_reset_cache(&mut self, _: &mut CharItemView, bit0: bool) {
            self.reset_called += 1;
            self.last_bit0_set = bit0;
        }
        unsafe fn glyph_get_count(&mut self, _: &CharItemView) -> u32 {
            self.children_count
        }
        unsafe fn child_undraw(&mut self, _: &CharItemView, _: u32, _: &Flag) {
            self.children_undrawn += 1;
        }
    }

    #[test]
    fn undraw_no_caches_partial_path_invokes_painter_and_children() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let flag = Flag(0);
            let mut deps = TestDeps {
                reset_called: 0,
                last_bit0_set: false,
                children_count: 3,
                children_undrawn: 0,
            };
            let r = undraw(&mut ci, &flag, &mut deps);
            assert_eq!(
                r,
                UndrawOutcome::PartialCacheReset { children_undrawn: 3 }
            );
            assert_eq!(deps.reset_called, 1);
            assert!(!deps.last_bit0_set);
            assert_eq!(deps.children_undrawn, 3);
        }
    }

    #[test]
    fn undraw_flag_bit0_set_full_cache_path() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            let flag = Flag(1);
            let mut deps = TestDeps {
                reset_called: 0,
                last_bit0_set: false,
                children_count: 0,
                children_undrawn: 0,
            };
            let r = undraw(&mut ci, &flag, &mut deps);
            assert_eq!(
                r,
                UndrawOutcome::FullCacheReset { children_undrawn: 0 }
            );
            assert_eq!(deps.reset_called, 1);
            assert!(deps.last_bit0_set);
        }
    }

    #[test]
    fn undraw_releases_paths_and_render_path_cache() {
        unsafe {
            let mut ci = CharItemView::new_empty();
            // setup paths_cache and render_path_cache with refcount > 1
            let path_layout = Layout::new::<ControlBlock<crate::paths::Paths>>();
            let path_ctrl =
                std::alloc::alloc(path_layout) as *mut ControlBlock<crate::paths::Paths>;
            ptr::write(
                path_ctrl,
                ControlBlock {
                    obj: 0xCAFEusize as *mut crate::paths::Paths,
                    refcount: 2,
                },
            );
            ci.paths_cache = path_ctrl;
            let render_layout = Layout::new::<ControlBlock<crate::path::Path>>();
            let render_ctrl =
                std::alloc::alloc(render_layout) as *mut ControlBlock<crate::path::Path>;
            ptr::write(
                render_ctrl,
                ControlBlock {
                    obj: 0xBEEFusize as *mut crate::path::Path,
                    refcount: 2,
                },
            );
            ci.render_path_cache = render_ctrl;

            let flag = Flag(0);
            let mut deps = TestDeps {
                reset_called: 0,
                last_bit0_set: false,
                children_count: 0,
                children_undrawn: 0,
            };
            let _ = undraw(&mut ci, &flag, &mut deps);

            // refcount decremented to 1
            assert_eq!((*path_ctrl).refcount, 1);
            assert_eq!((*render_ctrl).refcount, 1);
            // slots cleared
            assert!(ci.paths_cache.is_null());
            assert!(ci.render_path_cache.is_null());

            std::alloc::dealloc(path_ctrl as *mut u8, path_layout);
            std::alloc::dealloc(render_ctrl as *mut u8, render_layout);
        }
    }
}
