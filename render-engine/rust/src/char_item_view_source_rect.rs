//! `Hnc::Shape::Text::CharItemView::GetSourceRect` outer (L-5c-RE-6b-2).
//!
//! ## raw 출처
//!
//! - `__ZN3Hnc5Shape4Text12CharItemView13GetSourceRectERKNS1_10AllocationEPKNS0_5ThemeE`
//! - 주소: `0x2f9030`, ~2564B
//! - decompile: 514 lines
//!
//! ## 함수 의미
//!
//! 본 글자의 실제 visual bounding rect (effects 포함) 계산. `GetBounds` 의 핵심 워커.
//! Effects 가 있으면 그 만큼 rect 를 확장.
//!
//! ## 핵심 control flow (raw 0x2f9030-0x2f9a30)
//!
//! 1. CalcDrawVariables (이미 ported as `calc_draw_variables`) 호출 →
//!    `(format_origin_x, format_origin_y, total_width, total_height, ...)`
//! 2. Render::Path::GetBounds (path 가 cache 됐으면 path bounds 도 union)
//! 3. RunProperty 에서 Effects 가져오기 (GetRealTextEffects, 이미 ported as
//!    `get_real_text_effects`)
//! 4. Effects 의 각 effect (Shadow/Glow/Reflection/OuterShadow) 의 size delta 를 rect 에 union
//!    - Shadow: distance + blur 만큼 rect 확장
//!    - Glow: radius 만큼 union
//!    - Reflection: bottom 으로 distance + height 확장
//!    - OuterShadow: similar to Shadow
//! 5. transformation 적용 (Vert / 회전 / scale)
//! 6. allocation x/y offset 합산
//! 7. result rect 반환
//!
//! ## 본 port scope (L-5c-RE-6b-2)
//!
//! - ✅ CalcDrawVariables 호출 + base rect 계산 (이미 ported)
//! - ✅ Path bounds union (callback)
//! - ✅ Effects iteration outer + 각 effect 의 size delta 적용 (callback)
//! - ✅ Transformation 적용 (callback)
//! - ✅ Allocation offset 합산
//! - ⏸️ Effects 각 type 의 정확한 size delta 계산은 별도 RE (Shadow/Glow/Reflection 의
//!       blur radius/distance 등). 본 outer port 는 callback 으로 받음.

use crate::blip_glyph::Allocation;
use crate::char_item_view::CharItemView;
use crate::char_item_view_bounds::BoundsRect;

/// EffectKind — Effects::GetEffectMap 의 type 별 size delta 적용 분기.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    /// raw PropertyKey 0x3ee
    Shadow,
    /// raw PropertyKey 0xbb9
    Glow,
    /// raw PropertyKey 0xbba
    OuterShadow,
    /// raw PropertyKey 0xbbb
    Reflection,
}

impl EffectKind {
    pub fn property_key(self) -> u32 {
        match self {
            EffectKind::Shadow => 0x3ee,
            EffectKind::Glow => 0xbb9,
            EffectKind::OuterShadow => 0xbba,
            EffectKind::Reflection => 0xbbb,
        }
    }
}

pub trait GetSourceRectDeps {
    /// raw `CalcDrawVariables` 결과 — `(origin_x, origin_y, width, height)`.
    /// 이미 ported as `calc_draw_variables`. 본 outer 는 callback 으로 받음 (이미 cache 됐을 수도).
    unsafe fn calc_draw_variables(
        &mut self,
        ci: &CharItemView,
        allocation: &Allocation,
    ) -> (f32, f32, f32, f32);

    /// raw `Render::Path::GetBounds(path)` 호출 — path 가 cache 됐을 때 path bounds 반환.
    /// 캐시 없으면 None.
    unsafe fn render_path_bounds(&mut self, ci: &CharItemView) -> Option<BoundsRect>;

    /// raw `Effects` enumeration — 활성 effect 의 (kind, size_delta) 쌍 반환.
    /// `size_delta` 는 (left, top, right, bottom) margin 으로 본 rect 를 확장.
    /// 빈 Vec 이면 effect 없음.
    unsafe fn enumerate_effects(
        &mut self,
        ci: &CharItemView,
    ) -> Vec<(EffectKind, [f32; 4])>;

    /// raw transformation 적용 (Vert / 회전 / scale). 적용 후 rect 반환.
    /// 변경 없으면 입력 그대로.
    unsafe fn apply_transformation(
        &mut self,
        ci: &CharItemView,
        rect: BoundsRect,
    ) -> BoundsRect;
}

/// raw `CharItemView::GetSourceRect(Allocation&, Theme*)` (`0x2f9030`, ~2564B) outer byte-eq.
///
/// 흐름:
/// 1. CalcDrawVariables → base rect (origin_x, origin_y, width, height)
/// 2. Render::Path bounds union (if cached)
/// 3. Effects iteration: 각 effect 의 size_delta 로 rect 확장 (union)
/// 4. Transformation 적용
/// 5. Allocation offset 합산
/// 6. 결과 rect 반환
///
/// # Safety
/// `ci` 는 valid CharItemView. `allocation` valid Allocation reference.
pub unsafe fn get_source_rect(
    ci: &CharItemView,
    allocation: &Allocation,
    deps: &mut dyn GetSourceRectDeps,
) -> BoundsRect {
    // Stage 1: CalcDrawVariables → base rect
    let (origin_x, origin_y, width, height) = deps.calc_draw_variables(ci, allocation);
    let mut rect = BoundsRect {
        x: origin_x,
        y: origin_y,
        w: width,
        h: height,
    };

    // Stage 2: Render::Path bounds union (path cache 있으면)
    if let Some(path_rect) = deps.render_path_bounds(ci) {
        rect = union_rect(rect, path_rect);
    }

    // Stage 3: Effects iteration — 각 effect 의 size_delta 적용
    let effects = deps.enumerate_effects(ci);
    for (_kind, delta) in &effects {
        // delta = [left, top, right, bottom] margin
        rect.x -= delta[0];
        rect.y -= delta[1];
        rect.w += delta[0] + delta[2];
        rect.h += delta[1] + delta[3];
    }

    // Stage 4: Transformation 적용 (Vert / 회전 / scale)
    rect = deps.apply_transformation(ci, rect);

    // Stage 5: Allocation offset 합산 (raw 의 allocation.origin_x/y 가 absolute place)
    rect.x += allocation.origin_x;
    rect.y += allocation.origin_y;

    rect
}

/// rect union (둘을 모두 포함하는 최소 bounding rect).
fn union_rect(a: BoundsRect, b: BoundsRect) -> BoundsRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    BoundsRect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDeps {
        cdv: (f32, f32, f32, f32),
        path_bounds: Option<BoundsRect>,
        effects: Vec<(EffectKind, [f32; 4])>,
        transform_returns: Option<BoundsRect>,
        cdv_call_count: u32,
        path_call_count: u32,
        effects_call_count: u32,
        transform_call_count: u32,
    }
    impl TestDeps {
        fn new(cdv: (f32, f32, f32, f32)) -> Self {
            Self {
                cdv,
                path_bounds: None,
                effects: Vec::new(),
                transform_returns: None,
                cdv_call_count: 0,
                path_call_count: 0,
                effects_call_count: 0,
                transform_call_count: 0,
            }
        }
    }
    impl GetSourceRectDeps for TestDeps {
        unsafe fn calc_draw_variables(
            &mut self,
            _: &CharItemView,
            _: &Allocation,
        ) -> (f32, f32, f32, f32) {
            self.cdv_call_count += 1;
            self.cdv
        }
        unsafe fn render_path_bounds(&mut self, _: &CharItemView) -> Option<BoundsRect> {
            self.path_call_count += 1;
            self.path_bounds
        }
        unsafe fn enumerate_effects(
            &mut self,
            _: &CharItemView,
        ) -> Vec<(EffectKind, [f32; 4])> {
            self.effects_call_count += 1;
            self.effects.clone()
        }
        unsafe fn apply_transformation(
            &mut self,
            _: &CharItemView,
            rect: BoundsRect,
        ) -> BoundsRect {
            self.transform_call_count += 1;
            self.transform_returns.unwrap_or(rect)
        }
    }

    fn empty_alloc_at(x: f32, y: f32) -> Allocation {
        let mut alloc: Allocation = unsafe { std::mem::zeroed() };
        alloc.origin_x = x;
        alloc.origin_y = y;
        alloc
    }

    #[test]
    fn effect_kind_property_keys_match_raw() {
        assert_eq!(EffectKind::Shadow.property_key(), 0x3ee);
        assert_eq!(EffectKind::Glow.property_key(), 0xbb9);
        assert_eq!(EffectKind::OuterShadow.property_key(), 0xbba);
        assert_eq!(EffectKind::Reflection.property_key(), 0xbbb);
    }

    #[test]
    fn base_rect_no_path_no_effects_no_alloc_offset() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(0.0, 0.0);
            let mut deps = TestDeps::new((10.0, 20.0, 100.0, 50.0));
            let r = get_source_rect(&ci, &alloc, &mut deps);
            assert_eq!(r, BoundsRect { x: 10.0, y: 20.0, w: 100.0, h: 50.0 });
            assert_eq!(deps.cdv_call_count, 1);
            assert_eq!(deps.path_call_count, 1);
            assert_eq!(deps.effects_call_count, 1);
            assert_eq!(deps.transform_call_count, 1);
        }
    }

    #[test]
    fn path_bounds_union_extends_rect() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(0.0, 0.0);
            let mut deps = TestDeps::new((0.0, 0.0, 50.0, 50.0));
            deps.path_bounds = Some(BoundsRect { x: 30.0, y: 30.0, w: 100.0, h: 100.0 });
            let r = get_source_rect(&ci, &alloc, &mut deps);
            // union: x=0, y=0, right=max(50, 130)=130, bottom=max(50, 130)=130
            assert_eq!(r, BoundsRect { x: 0.0, y: 0.0, w: 130.0, h: 130.0 });
        }
    }

    #[test]
    fn shadow_effect_extends_rect_by_margin() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(0.0, 0.0);
            let mut deps = TestDeps::new((10.0, 10.0, 100.0, 100.0));
            // Shadow [left, top, right, bottom] = [5, 0, 5, 10]
            deps.effects = vec![(EffectKind::Shadow, [5.0, 0.0, 5.0, 10.0])];
            let r = get_source_rect(&ci, &alloc, &mut deps);
            // base (10,10,100,100) - left5,top0 → (5,10), w=100+5+5=110, h=100+0+10=110
            assert_eq!(r, BoundsRect { x: 5.0, y: 10.0, w: 110.0, h: 110.0 });
        }
    }

    #[test]
    fn multiple_effects_stack_their_deltas() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(0.0, 0.0);
            let mut deps = TestDeps::new((0.0, 0.0, 100.0, 100.0));
            deps.effects = vec![
                (EffectKind::Shadow, [2.0, 0.0, 2.0, 5.0]),
                (EffectKind::Glow, [3.0, 3.0, 3.0, 3.0]),
            ];
            let r = get_source_rect(&ci, &alloc, &mut deps);
            // x: 0 - 2 - 3 = -5
            // y: 0 - 0 - 3 = -3
            // w: 100 + (2+2) + (3+3) = 110
            // h: 100 + (0+5) + (3+3) = 111
            assert_eq!(r, BoundsRect { x: -5.0, y: -3.0, w: 110.0, h: 111.0 });
        }
    }

    #[test]
    fn allocation_offset_is_added_last() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(100.0, 200.0);
            let mut deps = TestDeps::new((10.0, 10.0, 50.0, 50.0));
            let r = get_source_rect(&ci, &alloc, &mut deps);
            assert_eq!(r, BoundsRect { x: 110.0, y: 210.0, w: 50.0, h: 50.0 });
        }
    }

    #[test]
    fn transformation_override_replaces_rect() {
        unsafe {
            let ci = CharItemView::new_empty();
            let alloc = empty_alloc_at(0.0, 0.0);
            let mut deps = TestDeps::new((10.0, 10.0, 50.0, 50.0));
            deps.transform_returns = Some(BoundsRect { x: 0.0, y: 0.0, w: 200.0, h: 200.0 });
            let r = get_source_rect(&ci, &alloc, &mut deps);
            assert_eq!(r, BoundsRect { x: 0.0, y: 0.0, w: 200.0, h: 200.0 });
        }
    }

    #[test]
    fn union_rect_helper_correctness() {
        let a = BoundsRect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        let b = BoundsRect { x: 20.0, y: 5.0, w: 5.0, h: 20.0 };
        let u = union_rect(a, b);
        assert_eq!(u, BoundsRect { x: 0.0, y: 0.0, w: 25.0, h: 25.0 });
    }
}
