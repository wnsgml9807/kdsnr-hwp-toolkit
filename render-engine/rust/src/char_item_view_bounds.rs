//! `Hnc::Shape::Text::CharItemView::GetBounds` + `GetSourceRect` outer (L-5c-RE-6a).
//!
//! ## raw 출처
//!
//! - `__ZN3Hnc5Shape4Text12CharItemView9GetBoundsEPKNS0_5ThemeERKNS1_10AllocationEPNS1_5GlyphE`
//!   @ `0x2f9008`, **40B** (= 0x28) — trivial wrapper
//! - `__ZN3Hnc5Shape4Text12CharItemView13GetSourceRectERKNS1_10AllocationEPKNS0_5ThemeE`
//!   @ `0x2f9030`, ~2564B (514 decompile lines) — full source rect computation
//!
//! ## GetBounds (raw 0x2f9008, 40B)
//!
//! ```c
//! void CharItemView::GetBounds(Theme*, Allocation&, Glyph*) {
//!   if (theme != in_x3) {
//!     *out = 0;            // zero-init the 16B output rect (3 8B clears)
//!     return;
//!   }
//!   GetSourceRect(allocation, theme);
//! }
//! ```
//!
//! - `param_1 != in_x3`: theme 가 caller-provided 인지 self-internal 인지 분기.
//!   본 outer port 는 단순 check + 위임.
//! - `*out_sret = 0; *(out+0xc)=0; *(out+4)=0` = 16B output rect zero init
//! - else: `GetSourceRect` tail-call
//!
//! ## GetSourceRect (raw 0x2f9030, ~2564B)
//!
//! 본 outer port 는 trait callback 으로 위임. byte-eq port 는 다음 세션 L-5c-RE-6b.
//! 의존: CalcDrawVariables (이미 ported ✅) + Render::Path::GetBounds + Effects 적용.

use crate::char_item_view::CharItemView;
use crate::blip_glyph::Allocation;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct BoundsRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl BoundsRect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
}

pub trait GetBoundsDeps {
    /// raw `GetSourceRect(allocation, theme)` (= `0x2f9030`, ~2564B) 호출.
    /// 본 callback 의 byte-eq port 는 L-5c-RE-6b (CalcDrawVariables + Render::Path::GetBounds
    /// + Effects 적용).
    unsafe fn get_source_rect(
        &mut self,
        ci: &CharItemView,
        allocation: &Allocation,
    ) -> BoundsRect;
}

/// raw `CharItemView::GetBounds(Theme*, Allocation&, Glyph*)` (`0x2f9008`, 40B) byte-eq.
///
/// `theme_provided` 가 raw 의 `param_1 != in_x3` 분기 결과. 본 outer port 는 raw 의
/// trivial wrapper 만 (의미적: theme 가 user-provided 면 GetSourceRect 호출, 아니면
/// zero rect 반환).
///
/// # Safety
/// `ci` 는 valid CharItemView.
pub unsafe fn get_bounds(
    ci: &CharItemView,
    allocation: &Allocation,
    theme_provided: bool,
    deps: &mut dyn GetBoundsDeps,
) -> BoundsRect {
    if !theme_provided {
        // raw 0x2f9014: *out = 0 (16B zero init)
        return BoundsRect::ZERO;
    }
    // raw 0x2f902c: tail-call GetSourceRect
    deps.get_source_rect(ci, allocation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct TestDeps {
        called: RefCell<u32>,
        returns: BoundsRect,
    }
    impl GetBoundsDeps for TestDeps {
        unsafe fn get_source_rect(
            &mut self,
            _: &CharItemView,
            _: &Allocation,
        ) -> BoundsRect {
            *self.called.borrow_mut() += 1;
            self.returns
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn no_theme_returns_zero_rect() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                called: RefCell::new(0),
                returns: BoundsRect {
                    x: 1.0,
                    y: 2.0,
                    w: 3.0,
                    h: 4.0,
                },
            };
            let r = get_bounds(&ci, &alloc, false, &mut deps);
            assert_eq!(r, BoundsRect::ZERO);
            assert_eq!(*deps.called.borrow(), 0);
        }
    }

    #[test]
    fn theme_provided_calls_source_rect() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let expected = BoundsRect {
                x: 10.0,
                y: 20.0,
                w: 30.0,
                h: 40.0,
            };
            let mut deps = TestDeps {
                called: RefCell::new(0),
                returns: expected,
            };
            let r = get_bounds(&ci, &alloc, true, &mut deps);
            assert_eq!(r, expected);
            assert_eq!(*deps.called.borrow(), 1);
        }
    }
}
