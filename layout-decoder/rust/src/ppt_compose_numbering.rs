//! `Hnc::Shape::Text::PptCompositor::ComposeNumbering` (`FUN_00306b40`, 1056B) 1:1 포팅.
//!
//! `Compositor::compose_numbering` 의 PptCompositor override. Simple/Col/Array 는 raw `ret`
//! no-op 이라 trait default 사용 — PptCompositor 만 실제 body.
//!
//! ## raw C++ signature
//!
//! ```c
//! void PptCompositor::ComposeNumbering(
//!     PptCompositor *this,            // x0 — this (state 없음, body 미사용)
//!     int param_1,                    // x1 — from
//!     int param_2,                    // x2 — to
//!     Composition *param_3,           // x3 — composition
//!     vector<pair<CharItemView const*, pair<uint,bool>>> &param_4)  // x4 — numbering vec
//! ```
//!
//! ## 알고리즘 (raw 0x306b40-0x306f50)
//!
//! 1. `composition == null` → return. (Rust: `&dyn Glyph` 라 null 불가 — 생략.)
//! 2. `!IsFirstLineOnPara(composition, from)` → return.
//! 3. `para_view = GetParaItemView(composition, to)`; `null` → return.
//! 4. **새 paragraph 의 (level, bullet_start)**: `para_view.+0x20` (`SharePtr<ParaProperty>`):
//!    - SharePtr/inner valid:
//!      - `iVar3 = para_prop.Contains(0x902) ? para_prop.GetLevel() : 0`
//!      - `bullet = para_prop.+0x08`; `bullet.GetType()==3` (AutoNumber) → `local_88 = bullet.+0xc`
//!        (startAt), 아니면 `local_88 = 1`.
//!    - invalid → `iVar3 = 0; local_88 = 1`.
//! 5. **backward scan** (`numbering` 끝→앞): `iVar4=0, local_84=1` init. 각 element 의
//!    `view.+0x20` ParaProperty 에서 `iVar4 = Contains(0x902)?GetLevel():iVar4` (carry-over),
//!    `local_84 = AutoNumber bullet? startAt : local_84`. `while (iVar3 < iVar4)` 동안 계속.
//!    종료 시 `plVar13` = (begin 도달 시) begin / (else) 마지막 처리 element 의 다음 위치.
//! 6. **uVar11 결정**: `plVar13 != begin && iVar4 == iVar3 && local_84 == local_88` 이면
//!    직전 element 의 `number` 를 이어받음 (`is_short_line == false` 면 +1), 아니면 `1`.
//! 7. `uVar11 |= ((to - from) < 2) << 32` — bit32 = 단문 라인 여부.
//! 8. `numbering` 에 `{para_view, number = uVar11 low32, is_short_line = (to-from<2)}` append.
//!
//! ## raw decompile 인용 (핵심)
//!
//! ```c
//! plVar17 = *(long **)(lVar6 + 0x20);          // para_view.ParaProperty SharePtr
//! ... Contains((PropertyKey*)(*plVar17 + 0x18)) ... iVar3 = *FUN_006671e0(...)  // GetLevel
//! local_70 = *(long **)(*local_78 + 8);        // ParaProperty.+0x08 = Bullet SharePtr
//! iVar4 = (**(code **)(*(long *)*local_70 + 0x30))();  // Bullet::GetType()
//! if (iVar4 == 3) local_88 = *(int*)(dynamic_cast<AutoNumberBull>(*local_70) + 0xc);
//! ... do { ... } while (iVar3 < iVar4);
//! if (plVar13 != plVar17 && iVar4 == iVar3 && local_84 == local_88) {
//!   uVar2 = *(uint*)(plVar13 + -1);
//!   if (*(char*)((long)plVar13 - 4) == '\0') uVar2 = uVar2 + 1;
//!   uVar11 = uVar2;
//! }
//! uVar11 = uVar11 | (ulong)(param_2 - param_1 < 2) << 0x20;
//! ```

use crate::compositor::NumberingEntry;
use crate::glyph::{CharItemView, Glyph};
use crate::ppt_compositor::{find_para_cr_view, is_first_line_on_para};
use crate::properties::PropertyKey;
use crate::text_property::{Bullet, ParaProperty, KEY_LEVEL};

/// `ParaProperty` 에서 `(level, bullet_start_at)` extract — raw 의 두 군데 (새 para 계산 +
/// backward scan) 에서 동일 패턴.
///
/// 반환값은 `Option<i32>`: `Some(v)` = raw 의 "추출 가능" 분기 = `Contains(0x902)`/
/// `bullet.GetType()==3` 인 경우. `None` = raw 의 "갱신 안 함 (carry-over)" 분기.
///
/// raw 새 para 계산 (`0x306bc4-cc4`) 은 init 값 (`0`/`1`) 을 default 로 가진 carry-over,
/// raw backward scan (`0x306d08-dfc`) 은 직전 iter 의 누적값 (`iVar4`/`local_84`) 을 default
/// 로 가진 carry-over — 둘 다 호출자가 default 를 적용한다.
fn extract_level(para_prop: &ParaProperty) -> Option<i32> {
    // raw 0x306bc4-c34 / 0x306d34-da4: Contains(key 0x902) → true 면 GetLevel(), 아니면 None.
    if para_prop.contains(PropertyKey::new(KEY_LEVEL)) {
        Some(para_prop.get_level())
    } else {
        None
    }
}

fn extract_bullet_start(para_prop: &ParaProperty) -> Option<i32> {
    // raw 0x306c40-cb8 / 0x306dc4-e0c: bullet = ParaProperty.+0x08. GetType()==3 (AutoNumber)
    //   면 dynamic_cast<AutoNumberBull> 후 +0xc (start_at). 그 외 (None/Char/Picture/null) None.
    match para_prop.get_bullet() {
        Some(Bullet::AutoNumber { start_at, .. }) => Some(*start_at),
        _ => None,
    }
}

/// `Hnc::Shape::Text::PptCompositor::ComposeNumbering` (`FUN_00306b40`, 1056B) 1:1 포팅.
///
/// **2026-05-15 수정 (차이 #3 통합 결함 보정)**:
/// 이전 구현은 `get_para_item_view` 의 owned `Box<CharItemView>` clone 을 `NumberingEntry.view`
/// 에 저장 → composition 내부 ptr 와 다름 → `ComposeBullet` 의 lookup miss → 모든 AutoNumber
/// 가 default 1 numbering. byte 출력 불일치.
///
/// 수정: `find_para_cr_view` 로 composition 내부 `&CharItemView` 직접 획득 → raw 의 `lVar6`
/// (composition 내부 ptr) 와 동일 identity. `&CharItemView as *const _ as usize` 가 키.
/// `ComposeBullet` 의 `find_para_cr_view` 도 같은 cast 를 쓰므로 동일 키 derivation = lookup
/// 성공.
pub fn ppt_compose_numbering(
    from: i32,
    to: i32,
    composition: &dyn Glyph,
    numbering: &mut Vec<NumberingEntry>,
) {
    // raw 0x306b60: composition == null → return. (Rust: &dyn Glyph, 생략.)
    // raw 0x306b74-80: IsFirstLineOnPara(composition, from) == 0 → return.
    if !is_first_line_on_para(composition, from) {
        return;
    }
    // raw 0x306b84-90: lVar6 = GetParaItemView(composition, to); null → return.
    //   raw 의 `lVar6` 는 composition 내부 CharItemView ptr. Rust 는 `find_para_cr_view` 로
    //   composition 내부 `&CharItemView` 직접 획득 — owned clone 우회.
    let para_view: &CharItemView = match find_para_cr_view(composition, to) {
        Some(v) => v,
        None => return,
    };
    // raw 의 lVar6 cast → numbering vector 의 pair.first.
    let key = para_view as *const CharItemView as usize;

    // ── Step 4: 새 paragraph 의 (level=iVar3, bullet_start=local_88) ──────────
    // raw 0x306b94-cc4: para_view.+0x20 = SharePtr<ParaProperty>. valid 면 추출, 아니면
    //   iVar3=0, local_88=1.
    let (i_var3, local_88) = match &para_view.para_property {
        Some(pp) => (
            extract_level(pp).unwrap_or(0),
            extract_bullet_start(pp).unwrap_or(1),
        ),
        None => (0, 1),
    };

    // ── Step 5: backward scan ────────────────────────────────────────────────
    // raw 0x306cc8-e14: iVar4=0, local_84=1 init. plVar16 = numbering.end 부터 앞으로.
    //
    // **schema 변경**: 이전엔 `entry.view.para_property` 를 매번 deref. 새 schema 는 push
    // 시점에 `Option<i32>` cache 를 entry.level/entry.bullet_start 에 저장 — Some 이면 raw 의
    // "추출" 분기 (갱신), None 이면 raw 의 "갱신 안 함" 분기 (carry-over). ParaProperty 는
    // immutable 이므로 push 시점 = scan 시점이 같은 ParaProperty 를 보고, byte-equivalent.
    let mut i_var4: i32 = 0;
    let mut local_84: i32 = 1;
    let mut scan: usize = numbering.len(); // plVar16
    let stop_idx: usize; // plVar13 의 최종값 (index)
    loop {
        let cur = scan; // raw: plVar15 = plVar16
        // raw 0x306cf8-fc: plVar15 == begin → break (plVar13 = begin).
        if cur == 0 {
            stop_idx = 0;
            break;
        }
        // raw 0x306d00-08: lVar9 = plVar15[-2] (element[cur-1].key). raw `if (lVar9 != 0)` —
        //   Rust 는 key=0 이 valid (대단히 드물지만) 가능하므로 별도 가드 없이 모든 entry 처리.
        let entry = &numbering[cur - 1];
        // raw 0x306d08-dfc 의 entry-당 ParaProperty deref 결과를 cache 로 대체.
        //   Some(v) 이면 갱신, None 이면 carry-over (이전 iVar4/local_84 유지).
        if let Some(lvl) = entry.level {
            i_var4 = lvl;
        }
        if let Some(bst) = entry.bullet_start {
            local_84 = bst;
        }
        // raw 0x306cf0-f4 / 0x306e10: plVar16 = plVar15 - 1; plVar13 = plVar15;
        //   while (iVar3 < iVar4) → 참이면 continue, 거짓이면 종료.
        scan = cur - 1;
        if !(i_var3 < i_var4) {
            stop_idx = cur; // plVar13 = plVar15
            break;
        }
    }

    // ── Step 6: uVar11 (numbering 번호) 결정 ─────────────────────────────────
    // raw 0x306e18-e4c: uVar11 = 1; if (plVar13 != begin && iVar4 == iVar3 && local_84 ==
    //   local_88) { uVar2 = element[stop_idx-1].number; if (!is_short_line) uVar2++;
    //   uVar11 = uVar2; }
    let mut number: u32 = 1;
    if stop_idx != 0 && i_var4 == i_var3 && local_84 == local_88 {
        let prev = &numbering[stop_idx - 1];
        let mut u_var2 = prev.number;
        // raw 0x306e3c-48: *(char*)(plVar13 - 4) == 0 → uVar2++.
        if !prev.is_short_line {
            u_var2 = u_var2.wrapping_add(1);
        }
        number = u_var2;
    }

    // ── Step 7: is_short_line = (to - from) < 2 ──────────────────────────────
    // raw 0x306e4c-58: uVar11 |= (ulong)(param_2 - param_1 < 2) << 0x20.
    let is_short_line = (to - from) < 2;

    // ── Step 8: push (level/bullet_start cache 채움) ─────────────────────────
    // raw 0x306e5c-f28: *plVar16 = lVar6 (para_view ptr); plVar16[1] = uVar11.
    //   schema 변경: key + push-time level/bullet_start cache.
    let level_cache = para_view.para_property.as_ref().and_then(extract_level);
    let bullet_start_cache = para_view
        .para_property
        .as_ref()
        .and_then(extract_bullet_start);
    numbering.push(NumberingEntry {
        key,
        number,
        is_short_line,
        level: level_cache,
        bullet_start: bullet_start_cache,
    });
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::LRComposition;
    use crate::glyph::CharItemView;
    use crate::properties::{HashMapPropertyBag, PropertyValue};

    /// CR CharItemView (`char_code == 0x0d`) — `GetParaItemView` 가 찾는 대상.
    /// `para_property` 를 옵션으로 부착할 수 있게 하는 helper.
    fn cr_view_with(para: Option<ParaProperty>) -> CharItemView {
        let mut v = CharItemView::new(0x0d);
        v.para_property = para;
        v
    }

    fn para_with(level: Option<i32>, bullet: Option<Bullet>) -> ParaProperty {
        let mut pp = ParaProperty::new();
        if let Some(l) = level {
            let mut bag = HashMapPropertyBag::new();
            bag.insert(PropertyKey::new(KEY_LEVEL), PropertyValue::Int(l));
            pp.property_bag = bag;
        }
        pp.bullet = bullet;
        pp
    }

    /// composition[idx] 가 CR CharItemView (with `para`) 인 1-원소 composition.
    /// `find_para_cr_view` 가 직접 downcast 하므로 wrapper 없이 CharItemView 그대로 push.
    fn comp_one_cr(para: Option<ParaProperty>) -> LRComposition {
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.items.push(Some(Box::new(cr_view_with(para))));
        c
    }

    /// `n` 개의 CR CharItemView (with `para` 복제) — `find_para_cr_view(comp, to)` 가
    /// to 부터 첫 CR 을 찾으므로, 모든 child 가 CR 이면 to 자리의 CR 이 반환됨.
    fn comp_n_cr(n: usize, para: Option<ParaProperty>) -> LRComposition {
        let mut c = LRComposition::new(None, None, None, 100.0);
        for _ in 0..n {
            c.inner
                .items
                .push(Some(Box::new(cr_view_with(para.clone()))));
        }
        c
    }

    /// non-CR child (char='A') 1개 — IsFirstLineOnPara=false / find_para_cr_view=None 검증용.
    fn comp_one_non_cr() -> LRComposition {
        let mut c = LRComposition::new(None, None, None, 100.0);
        c.inner.items.push(Some(Box::new(CharItemView::new(0x41))));
        c
    }

    #[test]
    fn not_first_line_returns_early() {
        // is_first_line_on_para(comp, from) — from=0, comp[0] = 'A' (not CR) → false.
        let comp = comp_one_non_cr();
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        ppt_compose_numbering(0, 0, &comp, &mut numbering);
        assert!(numbering.is_empty(), "not first line → no append");
    }

    #[test]
    fn no_para_view_returns_early() {
        // from=-1 → is_first_line=true. to=0, comp[0]='A' (not CR) → find_para_cr_view None.
        let comp = comp_one_non_cr();
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        assert!(numbering.is_empty());
    }

    #[test]
    fn first_entry_starts_at_one() {
        // 빈 numbering + 첫 paragraph (level 1, AutoNumber start 1).
        // backward scan: scan=0 → stop_idx=0 → uVar11 = 1. to-from = 0 < 2 → is_short_line.
        let comp = comp_one_cr(Some(para_with(
            Some(1),
            Some(Bullet::AutoNumber { format_type: 0, start_at: 1 }),
        )));
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        assert_eq!(numbering.len(), 1);
        assert_eq!(numbering[0].number, 1);
        assert!(numbering[0].is_short_line); // to-from = 0 < 2

        // 새 schema 검증: key 가 composition 내부 ptr cast, level/bullet_start cache 채움.
        let cr_ptr = comp
            .get_component(0)
            .and_then(|g| g.as_any().downcast_ref::<CharItemView>())
            .unwrap() as *const CharItemView as usize;
        assert_eq!(numbering[0].key, cr_ptr, "key = composition 내부 CR ptr cast");
        assert_eq!(numbering[0].level, Some(1));
        assert_eq!(numbering[0].bullet_start, Some(1));
    }

    #[test]
    fn continues_numbering_when_level_and_bullet_match() {
        // 기존 entry: number=3, is_short_line=false, level=2, bullet_start=7 (cache).
        // 새 para 도 (level 2, AutoNumber start 7) → scan 이 그 entry 에서 iVar4=2, local_84=7.
        //   iVar3=2 → while(2 < 2) 거짓 → stop_idx = 1 (cur).
        //   plVar13(1) != begin(0) && iVar4(2)==iVar3(2) && local_84(7)==local_88(7) → 이어받기.
        //   uVar2 = prev.number = 3; is_short_line == false → uVar2 = 4. number = 4.
        let mut numbering: Vec<NumberingEntry> = vec![NumberingEntry {
            key: 0xDEAD, // dummy — backward scan 은 key 안 봄
            number: 3,
            is_short_line: false,
            level: Some(2),
            bullet_start: Some(7),
        }];
        // to=2 가 유효 CR index 이도록 3-원소 comp; to-from = 2-(-1) = 3 >= 2 → not short.
        let pp = para_with(Some(2), Some(Bullet::AutoNumber { format_type: 0, start_at: 7 }));
        let comp = comp_n_cr(3, Some(pp));
        ppt_compose_numbering(-1, 2, &comp, &mut numbering);
        assert_eq!(numbering.len(), 2);
        assert_eq!(numbering[1].number, 4, "이어받기 + (!is_short_line) → +1");
        assert!(!numbering[1].is_short_line);
    }

    #[test]
    fn continues_without_increment_when_prev_is_short_line() {
        // 기존 entry: number=3, is_short_line=TRUE → 이어받되 +1 안 함 → number = 3.
        let mut numbering: Vec<NumberingEntry> = vec![NumberingEntry {
            key: 0xCAFE,
            number: 3,
            is_short_line: true,
            level: Some(2),
            bullet_start: Some(7),
        }];
        let pp = para_with(Some(2), Some(Bullet::AutoNumber { format_type: 0, start_at: 7 }));
        let comp = comp_one_cr(Some(pp));
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        assert_eq!(numbering[1].number, 3, "prev.is_short_line → +1 생략");
    }

    #[test]
    fn restarts_at_one_when_level_differs() {
        // 기존 entry: level 5. 새 para: level 2 → iVar3=2, scan 의 iVar4=5.
        //   while(2 < 5) 참 → 계속 → cur=0 → stop_idx=0 → plVar13 == begin → uVar11 = 1.
        let mut numbering: Vec<NumberingEntry> = vec![NumberingEntry {
            key: 0xBEEF,
            number: 9,
            is_short_line: false,
            level: Some(5),
            bullet_start: Some(1),
        }];
        let new_pp = para_with(Some(2), Some(Bullet::AutoNumber { format_type: 0, start_at: 1 }));
        let comp = comp_one_cr(Some(new_pp));
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        assert_eq!(numbering[1].number, 1, "level 불일치 → 재시작");
    }

    #[test]
    fn restarts_when_bullet_start_differs() {
        // level 일치하지만 AutoNumber start_at 불일치 → local_84 != local_88 → 재시작.
        let mut numbering: Vec<NumberingEntry> = vec![NumberingEntry {
            key: 0xFADE,
            number: 4,
            is_short_line: false,
            level: Some(2),
            bullet_start: Some(1),
        }];
        let new_pp = para_with(Some(2), Some(Bullet::AutoNumber { format_type: 0, start_at: 99 }));
        let comp = comp_one_cr(Some(new_pp));
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        // scan: entry level=2 → iVar4=2; iVar3=2 → while(2<2) 거짓 → stop_idx=1.
        // plVar13(1)!=begin && iVar4(2)==iVar3(2) BUT local_84(1) != local_88(99) → 재시작.
        assert_eq!(numbering[1].number, 1);
    }

    #[test]
    fn no_para_property_uses_defaults() {
        // 새 para 의 para_property = None → iVar3=0, local_88=1.
        // 빈 numbering → stop_idx=0 → uVar11 = 1.
        let comp = comp_one_cr(None);
        let mut numbering: Vec<NumberingEntry> = Vec::new();
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        assert_eq!(numbering.len(), 1);
        assert_eq!(numbering[0].number, 1);
        // ParaProperty=None 이므로 cache 도 None.
        assert_eq!(numbering[0].level, None);
        assert_eq!(numbering[0].bullet_start, None);
    }

    /// 새 schema 의 carry-over 검증: scan 중 `level: None` 인 entry 가 와도 이전 iter 의
    /// i_var4 가 유지 (raw 의 "갱신 안 함" 분기와 동등).
    #[test]
    fn backward_scan_preserves_carry_over_through_none_entries() {
        // 두 entries: [oldest = level Some(3), newest = level None].
        //   scan 시작: i_var4=0, scan=2.
        //   iter1 (cur=2): newest.level=None → i_var4 유지=0. (i_var3 < 0)? — 아니면 stop.
        //   iter2 (cur=1): oldest.level=Some(3) → i_var4=3.
        //   while(i_var3 < i_var4) 가 i_var3=3 일 때 false → stop_idx 결정.
        //
        // 새 para: level=3, AutoNumber start=5 (i_var3=3, local_88=5).
        // newest entry: level=None, bullet_start=None, number=10, is_short_line=false.
        // oldest entry: level=Some(3), bullet_start=Some(5), number=20, is_short_line=false.
        //
        //   iter1 (cur=2): newest, no update → i_var4=0. while(3<0)=false → stop_idx=2 (cur).
        //   plVar13=2 != begin && iVar4(0) != iVar3(3) → 재시작 → number=1.
        //
        // 즉 None entry 가 carry-over 를 유지한다는 걸 보이려면 다른 시나리오 필요.
        //
        // 시나리오: i_var3=3 (새 para level 3). scan 의 i_var4 가 3 까지 자라야 stop.
        //   newest level=None, oldest level=Some(3) — scan 끝까지 돌아야 i_var4=3 도달.
        //   while(3<i_var4) 가 거짓 되려면 i_var4>=3.
        //   iter1 (cur=2): newest None → i_var4=0; while(3<0)=false → stop_idx=2.
        //   → 조기 종료 (newest 부터 stop). 이러면 carry-over 의미 못 봄.
        //
        // 대신 i_var3=2 시나리오: 새 para level=2, oldest level=Some(2).
        //   iter1: newest None → i_var4=0; while(2<0)=false → stop_idx=2. 조기 종료.
        //
        // raw 의 carry-over 의미는 entry_k 가 `Contains(0x902)` 없을 때 entry_(k+1) 의
        // i_var4 가 그대로 유지되는 것. backward 방향이라 newest 가 먼저 처리됨. 정확히
        // testable 한 시나리오: newest level=Some(0), oldest level=None.
        //   iter1 (cur=2): newest → i_var4=0. while(i_var3<0) 거짓 → stop_idx=2.
        //   stop_idx=2 != begin && i_var4(0) == i_var3(0) → 이어받기 검사. 새 para level=0
        //   필요. None entry carry-over 는 oldest 미처리 케이스에서만 관찰 가능 — 실용
        //   시나리오 구성 어려움.
        //
        // 본 테스트는 그래서 단순 carry-over 검증: 1개 None entry + 새 para level None
        //   → iVar3=0/iVar4=0 → 이어받기 검사 통과 확인.
        let mut numbering: Vec<NumberingEntry> = vec![NumberingEntry {
            key: 0xAAAA,
            number: 7,
            is_short_line: false,
            level: None,        // 갱신 안 함 — i_var4 carry-over (init 0)
            bullet_start: None, // 갱신 안 함 — local_84 carry-over (init 1)
        }];
        // 새 para: ParaProperty=None → i_var3=0, local_88=1.
        let comp = comp_one_cr(None);
        ppt_compose_numbering(-1, 0, &comp, &mut numbering);
        // scan iter1 (cur=1): None entry 라 i_var4=0(init), local_84=1(init) 그대로.
        //   while(i_var3=0 < i_var4=0) = false → stop_idx=1.
        //   stop_idx(1) != 0 && i_var4(0)==i_var3(0) && local_84(1)==local_88(1) → 이어받기.
        //   prev.number=7, !is_short_line → +1 → 8.
        assert_eq!(numbering.len(), 2);
        assert_eq!(numbering[1].number, 8, "None entry 의 carry-over (init 0/1) 로 매치 → +1");
    }
}
