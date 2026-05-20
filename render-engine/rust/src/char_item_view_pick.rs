//! `Hnc::Shape::Text::CharItemView::Pick` outer (L-5c-RE-6b-1).
//!
//! ## raw 출처
//!
//! - `__ZN3Hnc5Shape4Text12CharItemView4PickERKNS1_10AllocationENS_5Type5PointEPNS1_5GlyphE`
//! - 주소: `0x2f9a34`, 388B
//! - decompile: ~97 lines
//!
//! ## 함수 의미
//!
//! Hit-test: 주어진 point (x,y) 가 본 CharItemView 의 visual region 에 닿는지 판정.
//! 닿으면 self (또는 child) ptr 을 sret 으로 반환, 아니면 null.
//!
//! ## 핵심 control flow (raw 0x2f9a34-0x2f9bb8)
//!
//! 1. (point 가 GetSourceRect 결과 rect 내부인지) bounds check
//!    - `GetSourceRect(allocation, theme)` → BoundsRect
//!    - `point.x ∈ [rect.x, rect.x+rect.w]` AND `point.y ∈ [rect.y, rect.y+rect.h]` ?
//! 2. 본 글자의 visibility flag check (RunProperty 의 IsVisible 0x961)
//! 3. inside → glyph self ptr 반환
//! 4. outside → 자식 vfunc traversal (vfunc[+0x80] = GetCount, vfunc[+0x88] = GetComponent,
//!    각 자식의 vfunc[+0x38] = Pick) — 자식 hit 시 그 ptr 반환
//! 5. 모두 miss → null
//!
//! ## 본 port scope (L-5c-RE-6b-1)
//!
//! - ✅ bounds check outer flow byte-eq (GetSourceRect 결과 받아서 inside? 판정)
//! - ✅ 자식 vfunc traversal byte-eq
//! - ⏸️ GetSourceRect 자체는 별도 trait callback (L-5c-RE-6b-2 에서 byte-eq)
//! - ⏸️ RunProperty IsVisible 체크는 callback

use crate::blip_glyph::Allocation;
use crate::char_item_view::CharItemView;
use crate::char_item_view_bounds::BoundsRect;

/// Pick 결과: 어떤 glyph 가 hit 되었는지.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickOutcome {
    /// 본 CharItemView 자체가 hit.
    SelfHit,
    /// 자식 idx 가 hit.
    ChildHit { idx: u32 },
    /// 아무도 hit 안 함.
    Miss,
}

pub trait PickDeps {
    /// raw `GetSourceRect(allocation, theme)` 호출 (L-5c-RE-6b-2).
    unsafe fn get_source_rect(
        &mut self,
        ci: &CharItemView,
        allocation: &Allocation,
    ) -> BoundsRect;

    /// raw `is_visible()` (RunProperty PropertyKey 0x961 check). false 면 self-hit 무시.
    unsafe fn is_visible(&mut self, ci: &CharItemView) -> bool;

    /// raw `vfunc[+0x80] = GetCount()` (자식 개수).
    unsafe fn glyph_get_count(&mut self, ci: &CharItemView) -> u32;

    /// raw `vfunc[+0x38] = child.Pick(allocation, point, glyph_out)` → child hit 시 true.
    unsafe fn child_pick(
        &mut self,
        ci: &CharItemView,
        idx: u32,
        allocation: &Allocation,
        point: (f32, f32),
    ) -> bool;
}

/// raw `CharItemView::Pick(Allocation&, Point, Glyph*)` (`0x2f9a34`, 388B) outer byte-eq.
///
/// # Safety
/// `ci` 는 valid CharItemView. `deps` 는 GetSourceRect/IsVisible/자식 vfunc dispatch 제공.
pub unsafe fn pick(
    ci: &CharItemView,
    allocation: &Allocation,
    point: (f32, f32),
    deps: &mut dyn PickDeps,
) -> PickOutcome {
    // Stage 1: bounds check via GetSourceRect
    let rect = deps.get_source_rect(ci, allocation);
    let (px, py) = point;
    let inside_x = px >= rect.x && px <= rect.x + rect.w;
    let inside_y = py >= rect.y && py <= rect.y + rect.h;

    // Stage 2: self visibility check
    if inside_x && inside_y && deps.is_visible(ci) {
        // raw 0x2f9ab8: return this (self hit)
        return PickOutcome::SelfHit;
    }

    // Stage 4: 자식 traversal
    let count = deps.glyph_get_count(ci);
    for i in 0..count {
        if deps.child_pick(ci, i, allocation, point) {
            return PickOutcome::ChildHit { idx: i };
        }
    }

    // Stage 5: miss
    PickOutcome::Miss
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDeps {
        rect: BoundsRect,
        is_visible_ret: bool,
        children_count: u32,
        child_pick_hit: Option<u32>,
    }
    impl PickDeps for TestDeps {
        unsafe fn get_source_rect(
            &mut self,
            _: &CharItemView,
            _: &Allocation,
        ) -> BoundsRect {
            self.rect
        }
        unsafe fn is_visible(&mut self, _: &CharItemView) -> bool {
            self.is_visible_ret
        }
        unsafe fn glyph_get_count(&mut self, _: &CharItemView) -> u32 {
            self.children_count
        }
        unsafe fn child_pick(
            &mut self,
            _: &CharItemView,
            idx: u32,
            _: &Allocation,
            _: (f32, f32),
        ) -> bool {
            Some(idx) == self.child_pick_hit
        }
    }

    fn empty_alloc() -> Allocation {
        unsafe { std::mem::zeroed() }
    }

    #[test]
    fn pick_inside_rect_visible_returns_self_hit() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
                is_visible_ret: true,
                children_count: 0,
                child_pick_hit: None,
            };
            let r = pick(&ci, &alloc, (50.0, 25.0), &mut deps);
            assert_eq!(r, PickOutcome::SelfHit);
        }
    }

    #[test]
    fn pick_outside_rect_no_children_returns_miss() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
                is_visible_ret: true,
                children_count: 0,
                child_pick_hit: None,
            };
            let r = pick(&ci, &alloc, (200.0, 200.0), &mut deps);
            assert_eq!(r, PickOutcome::Miss);
        }
    }

    #[test]
    fn pick_inside_but_invisible_falls_through_to_children() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
                is_visible_ret: false,
                children_count: 3,
                child_pick_hit: Some(1),
            };
            let r = pick(&ci, &alloc, (50.0, 25.0), &mut deps);
            assert_eq!(r, PickOutcome::ChildHit { idx: 1 });
        }
    }

    #[test]
    fn pick_inside_visible_takes_precedence_over_children() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
                is_visible_ret: true,
                children_count: 5,
                child_pick_hit: Some(2), // would otherwise win
            };
            let r = pick(&ci, &alloc, (10.0, 10.0), &mut deps);
            assert_eq!(r, PickOutcome::SelfHit);
        }
    }

    #[test]
    fn pick_outside_rect_child_at_idx_2_hits() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 },
                is_visible_ret: true,
                children_count: 5,
                child_pick_hit: Some(2),
            };
            let r = pick(&ci, &alloc, (100.0, 100.0), &mut deps);
            assert_eq!(r, PickOutcome::ChildHit { idx: 2 });
        }
    }

    #[test]
    fn pick_on_boundary_x_eq_rect_left_is_inside() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc();
            let mut deps = TestDeps {
                rect: BoundsRect { x: 10.0, y: 10.0, w: 90.0, h: 40.0 },
                is_visible_ret: true,
                children_count: 0,
                child_pick_hit: None,
            };
            // px=10.0 == rect.x → inside
            let r = pick(&ci, &alloc, (10.0, 25.0), &mut deps);
            assert_eq!(r, PickOutcome::SelfHit);
        }
    }
}
