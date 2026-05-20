//! `Hnc::Shape::Text::PptCompositor` — paragraph-level compositor.
//!
//! Phase B-8b: 3 helper 메소드 1:1 포팅 (`PptCompositor::ComposeLayout` 의 의존성).
//!
//! ## 포팅된 helper (raw asm 으로 bt 검증)
//!
//! | 함수                        | 주소     | bt | 반환 의미                              |
//! |-----------------------------|----------|----|----------------------------------------|
//! | `IsFirstLineOnPara`         | 0x306ffc | 0  | (children[idx] 가 CR 이거나 idx<0) ? T |
//! | `GetParaItemView`           | 0x3071d8 | 0  | [idx..] 범위 첫 CR CharItemView 포인터 |
//! | `GetFirstCharItemViewOnPara`| 0x30794c | 0  | children[idx] 의 CharItemView 포인터   |
//!
//! 모두 `Composition::ComposeGlyph(bt=0)` 호출 후 `dynamic_cast<CharItemView*>` 검사.
//!
//! ## 한컴 원본 vs Rust 차이
//!
//! C++ 원본: SharePtr<Glyph> + RTTI dynamic_cast.
//! Rust: `&dyn Glyph` + `Any::downcast_ref<CharItemView>()`.

use crate::compose_layout::composition_compose_glyph;
use crate::glyph::{CharItemView, Glyph};
use crate::value_types::BreakType;

/// `Hnc::Shape::Text::PptCompositor::IsFirstLineOnPara(Composition const*, int) const`
/// (`FUN_00306ffc`, sz=436).
///
/// 의미: idx 가 음수이거나, idx 위치의 item 이 CR (`char_code == 0x0d`) 면 true.
/// 그 외 false (composition null 포함).
///
/// 한컴 원본:
/// ```c
/// bool IsFirstLineOnPara(Composition *c, int idx) {
///   if (c == null) return false;
///   if (idx < 0) return true;
///   if (idx >= c.count) throw "GetAt";
///   item = c.children[idx];
///   composed = Composition::ComposeGlyph(item, bt=0);
///   if (composed != null && composed.inner != null) {
///     view = dynamic_cast<CharItemView*>(composed.inner);
///     if (view == null) return false;
///     return view.char_code == 0x0d;
///   }
///   return false;
/// }
/// ```
pub fn is_first_line_on_para(composition: &dyn Glyph, idx: i32) -> bool {
    if idx < 0 {
        return true;
    }
    let count = composition.get_count();
    if (idx as usize) >= count {
        // 한컴 원본은 throw "GetAt" — Rust 에선 false 로 graceful (caller 가 미리 체크해야).
        // TODO: 실제 byte-equivalent 위해서는 panic 또는 throw mechanism 필요할 수도.
        return false;
    }
    let item = match composition.get_component(idx as usize) {
        Some(g) => g,
        None => return false,
    };
    let composed = match composition_compose_glyph(item, BreakType::Normal) {
        Some(g) => g,
        None => return false,
    };
    composed
        .as_any()
        .downcast_ref::<CharItemView>()
        .map(|view| view.char_code == 0x0d)
        .unwrap_or(false)
}

/// `Hnc::Shape::Text::PptCompositor::GetParaItemView(Composition const*, int) const`
/// (`FUN_003071d8`, sz=508).
///
/// 의미: idx 부터 count-1 까지 순회하며, **첫 CR (`char_code == 0x0d`) CharItemView 의
/// clone 반환**. 못 찾으면 None.
///
/// 한컴 원본은 raw pointer (CharItemView*) 반환 — Rust 에선 `Box<CharItemView>` 으로 owned.
///
/// 한컴 원본:
/// ```c
/// long GetParaItemView(Composition *c, int idx) {
///   if (c == null || idx >= c.count) return 0;
///   for (i = idx; i < c.count; i++) {
///     item = c.children[i];
///     composed = Composition::ComposeGlyph(item, bt=0);
///     view = dynamic_cast<CharItemView*>(composed.inner);
///     if (view != null && view.char_code == 0x0d) return view;
///   }
///   return 0;
/// }
/// ```
pub fn get_para_item_view(composition: &dyn Glyph, idx: i32) -> Option<Box<CharItemView>> {
    let count = composition.get_count() as i32;
    if idx >= count {
        return None;
    }

    let start = idx.max(0) as usize;
    let end = count as usize;

    for i in start..end {
        let item = composition.get_component(i)?;
        let composed = match composition_compose_glyph(item, BreakType::Normal) {
            Some(g) => g,
            None => continue,
        };

        // dynamic_cast<CharItemView*> 후 char_code == 0xd 검사
        if let Some(view) = composed.as_any().downcast_ref::<CharItemView>() {
            if view.char_code == 0x0d {
                return Some(Box::new(view.clone()));
            }
        }
    }
    None
}

/// `Hnc::Shape::Text::PptCompositor::GetFirstCharItemViewOnPara(Composition const*, int) const`
/// (`FUN_0030794c`, sz=384).
///
/// 의미: idx 위치의 item 을 ComposeGlyph 후 `dynamic_cast<CharItemView*>` 결과 반환.
/// idx<0 또는 cast 실패 시 None.
///
/// 한컴 원본:
/// ```c
/// CharItemView *GetFirstCharItemViewOnPara(Composition *c, int idx) {
///   if (c == null || idx < 0) return 0;
///   if (idx >= c.count) throw "GetAt";
///   item = c.children[idx];
///   composed = Composition::ComposeGlyph(item, bt=0);
///   return dynamic_cast<CharItemView*>(composed.inner);
/// }
/// ```
pub fn get_first_char_item_view_on_para(
    composition: &dyn Glyph,
    idx: i32,
) -> Option<Box<CharItemView>> {
    if idx < 0 {
        return None;
    }
    let count = composition.get_count();
    if (idx as usize) >= count {
        // 한컴: throw "GetAt". Rust: None.
        return None;
    }
    let item = composition.get_component(idx as usize)?;
    let composed = composition_compose_glyph(item, BreakType::Normal)?;
    composed
        .as_any()
        .downcast_ref::<CharItemView>()
        .map(|v| Box::new(v.clone()))
}

/// `GetFirstCharItemViewOnPara` 의 mutable variant — composition 의 children[idx] 가
/// `CharItemView` 일 때 in-place mutable 참조 반환.
///
/// **존재 이유**: `PptCompositor::ComposeBullet` (`0x307468:180-204`) 는 결과
/// `BulletRenderGlyph` SharePtr 를 target CharItemView 의 `+0x98` (render_path) 슬롯에
/// **재할당** 한다:
/// ```c
/// puVar4 = *(undefined8 **)(lVar9 + 0x98);  // 기존 render_path SharePtr
/// // ... refcount 정리 ...
/// *(long **)(lVar9 + 0x98) = plVar10;       // 신규 SharePtr 저장
/// ```
/// 즉 mutable 접근 필수. 한컴의 SharePtr<Glyph> 체인은 inner ptr 가 안정적이라 dynamic_cast
/// 가 곧 composition 내부 CharItemView 자체이지만, Rust port 는 `composition_compose_glyph`
/// 가 input 을 clone 하므로 (`compose_layout.rs:201`) compose 경로로는 mutation 가 손실.
///
/// **차이점 (raw 와 의도적으로 다른 부분)**: 한컴은 children 의 SharePtr<Glyph> 가 가리키는
/// inner 가 CharItemView 이므로 ComposeGlyph 거쳐도 같은 ptr 가 반환된다. Rust 는 clone-on-
/// compose 라 같은 효과를 위해 `Composition::get_component_mut(idx).as_any_mut()
/// .downcast_mut::<CharItemView>()` 으로 **직접** 접근. composition.items[idx] 가 직접
/// CharItemView 가 아닌 경우 (wrapper Glyph) None — 이 경우 ComposeBullet 의 효과를 캐스트로
/// 표현할 수 없으므로 caller 가 책임.
pub fn get_first_char_item_view_on_para_mut(
    composition: &mut dyn Glyph,
    idx: i32,
) -> Option<&mut CharItemView> {
    if idx < 0 {
        return None;
    }
    let count = composition.get_count();
    if (idx as usize) >= count {
        return None;
    }
    let item = composition.get_component_mut(idx as usize)?;
    item.as_any_mut().downcast_mut::<CharItemView>()
}

/// `PptCompositor::ComposeBullet` (`0x307468:42-61`) 의 ParaItemView 조회 — `param_2` 부터
/// 첫 CR (`char_code == 0x0d`) CharItemView 의 **composition 내부 참조** 반환.
///
/// raw decompile:
/// ```c
/// lVar3 = GetParaItemView(this, param_4, param_2);
/// if (lVar3 != 0) {
///     pplVar1 = (long **)(lVar3 + 0x20);  // ParaProperty SharePtr slot
///     // local_48 = *pplVar1 (refcount-aware assignment)
/// }
/// // 이후 lVar3 는 numbering vector lookup 의 key (pair.first)
/// ```
///
/// `lVar3` 가 composition 내부 CharItemView ptr 라는 점이 **핵심**. raw 의
/// `vector<pair<CharItemView const*, ...>>` lookup (`0x307468:165-172`) 가 이 ptr 를
/// equality key 로 쓴다. Rust 는 `&CharItemView` 의 raw ptr 캐스트로 동일 identity 보존:
/// `view as *const CharItemView as usize`.
///
/// **`get_para_item_view` 와의 차이**: 그쪽은 `composition_compose_glyph` 후 `Box<CharItemView>`
/// (clone) 반환. 본 helper 는 composition 내부 borrow 를 반환하므로 numbering vector 의
/// pointer identity 키로 사용 가능. 한컴 raw 는 SharePtr 가 inner ptr 보존하므로 이 차이가
/// 없다 — Rust port 만의 결함을 우회한 것이 아니라, clone semantic 의 부산물.
///
/// 반환: `Some(view)` (composition 내부 CharItemView 참조) 또는 `None` (CR 없음, idx 범위 초과,
/// items[i] 가 CharItemView 가 아님 등).
pub fn find_para_cr_view(
    composition: &dyn Glyph,
    idx: i32,
) -> Option<&CharItemView> {
    let count = composition.get_count() as i32;
    if idx >= count {
        return None;
    }
    let start = idx.max(0) as usize;
    let end = count as usize;
    for i in start..end {
        let item = composition.get_component(i)?;
        if let Some(view) = item.as_any().downcast_ref::<CharItemView>() {
            if view.char_code == 0x0d {
                return Some(view);
            }
        }
    }
    None
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::ComposeResult;

    /// 테스트용 mock composition.
    #[derive(Debug)]
    struct MockComp {
        children: Vec<Box<dyn Glyph>>,
    }

    impl Glyph for MockComp {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(MockComp {
                children: self.children.iter().map(|c| c.clone_glyph()).collect(),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn get_count(&self) -> usize { self.children.len() }
        fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
            self.children.get(idx).map(|b| b.as_ref())
        }
    }

    /// 테스트용 CharItemView wrapper — Glyph trait 의 compose 가 자신을 그대로 반환.
    /// 한컴 `CharItemView::Compose` 는 base impl 사용 (replacement=null, can_break=bt<2).
    /// Rust 에선 compose default 사용 → composition_compose_glyph 가 clone 반환.
    #[derive(Debug, Clone)]
    struct CharItemViewItem(CharItemView);

    impl Glyph for CharItemViewItem {
        fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn compose(&self, bt: BreakType) -> ComposeResult {
            // 한컴 Glyph::Compose base: `*can_break = (bt < 2); *out = null;`
            // 하지만 composition_compose_glyph 는 input.clone_glyph() 을 반환하므로
            // 우리는 CharItemView 자체로 wrap 한 clone 을 반환해야 dynamic_cast 가 작동.
            //
            // 실제 한컴은 SharePtr<Glyph> 의 inner pointer (CharItemView*) 가 입력 그대로
            // 반환되어 dynamic_cast 가 성공. Rust 에선 clone_glyph 가 Box<CharItemView>
            // 를 반환하도록 우회 — 즉 wrapper 가 아니라 inner 만 노출.
            let _ = bt;
            ComposeResult {
                replacement: Some(Box::new(self.0.clone())),
                can_break: false,  // bt 무관 (replacement 있으면 사용됨)
            }
        }
    }

    fn make_comp(chars: &[u16]) -> MockComp {
        MockComp {
            children: chars
                .iter()
                .map(|&c| {
                    Box::new(CharItemViewItem(CharItemView::new(c))) as Box<dyn Glyph>
                })
                .collect(),
        }
    }

    #[test]
    fn is_first_line_negative_idx_returns_true() {
        let comp = make_comp(&[]);
        assert!(is_first_line_on_para(&comp, -1));
        assert!(is_first_line_on_para(&comp, -10));
    }

    #[test]
    fn is_first_line_cr_at_idx_returns_true() {
        let comp = make_comp(&[0x41, 0x0d, 0x42]);  // 'A', CR, 'B'
        assert!(is_first_line_on_para(&comp, 1));  // CR at 1 → true
    }

    #[test]
    fn is_first_line_non_cr_returns_false() {
        let comp = make_comp(&[0x41, 0x0d, 0x42]);
        assert!(!is_first_line_on_para(&comp, 0));  // 'A' → not CR
        assert!(!is_first_line_on_para(&comp, 2));  // 'B' → not CR
    }

    #[test]
    fn is_first_line_out_of_range_returns_false() {
        let comp = make_comp(&[0x41]);
        // idx=1 = count → false (한컴은 throw, 우리는 graceful)
        assert!(!is_first_line_on_para(&comp, 1));
    }

    #[test]
    fn get_para_item_view_finds_first_cr() {
        let comp = make_comp(&[0x41, 0x42, 0x0d, 0x43, 0x0d]);
        // idx=0 → first CR at 2 → returns view with char_code = 0x0d
        let v = get_para_item_view(&comp, 0).expect("CR should be found");
        assert_eq!(v.char_code, 0x0d);
    }

    #[test]
    fn get_para_item_view_no_cr_returns_none() {
        let comp = make_comp(&[0x41, 0x42, 0x43]);
        assert!(get_para_item_view(&comp, 0).is_none());
    }

    #[test]
    fn get_para_item_view_idx_past_end_returns_none() {
        let comp = make_comp(&[0x0d]);
        assert!(get_para_item_view(&comp, 5).is_none());
    }

    #[test]
    fn get_para_item_view_starts_from_idx() {
        // 첫 CR 은 idx=0, 두 번째 CR 은 idx=3.
        // idx=2 부터 검색 시작 → idx=3 의 CR 반환 (idx=0 의 CR 은 무시).
        let comp = make_comp(&[0x0d, 0x41, 0x42, 0x0d]);
        let v = get_para_item_view(&comp, 2).expect("CR at idx=3");
        assert_eq!(v.char_code, 0x0d);
    }

    #[test]
    fn get_first_char_view_returns_cast_result() {
        let comp = make_comp(&[0x41, 0x42, 0x0d]);
        let v = get_first_char_item_view_on_para(&comp, 0).unwrap();
        assert_eq!(v.char_code, 0x41);

        let v = get_first_char_item_view_on_para(&comp, 2).unwrap();
        assert_eq!(v.char_code, 0x0d);
    }

    #[test]
    fn get_first_char_view_negative_idx_returns_none() {
        let comp = make_comp(&[0x41]);
        assert!(get_first_char_item_view_on_para(&comp, -1).is_none());
    }

    #[test]
    fn get_first_char_view_out_of_range_returns_none() {
        let comp = make_comp(&[0x41]);
        assert!(get_first_char_item_view_on_para(&comp, 5).is_none());
    }
}
