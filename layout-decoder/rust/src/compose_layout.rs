//! `ColCompositor::ComposeLayout` — Hancom paragraph layout composition.
//!
//! 1:1 port of `Hnc::Shape::Text::ColCompositor::ComposeLayout`
//! (`FUN_00305d60`, size 1340).
//!
//! ## ⚠️ 미해결: null SharePtr Append 의 보존 여부
//!
//! 한컴은 predecessor / successor 조건 미충족 시 `Append(null SharePtr)` 를 명시적으로
//! 호출. `Composition::Insert` (Append 의 delegation 대상) 디코드 확인 결과:
//! ```c
//! plVar6 = operator_new(0x18);  // 새 list 노드 생성
//! plVar6[2] = *param_2;          // payload = SharePtr->ptr (null 이라도 그대로 저장)
//! count++;                       // count 무조건 증가
//! ```
//! 즉 null Append 도 count++ 효과 있음. 출력 container 가 `Composition` 이면 null entry 가
//! children 에 추가됨 → 후속 index 접근에 영향. byte-equivalent 보존 위해 Phase B-5
//! (Composition 포팅) 후 null-Append 경로 재검토 필요. 현재 구현은 "skip null" — 출력
//! container 가 Tile/Box (no Append override → no-op) 면 동등, Composition 이면 다를 수 있음.
//!
//! ## 함수 시그니처
//!
//! ```c
//! ColCompositor::ComposeLayout(
//!     ColCompositor *this,            // this (ColCompositor[+8] = col natural size)
//!     Composition const *composition,
//!     Composition::Type type,         // & 0xFFFE 마스킹: 2 → vertical, else horizontal
//!     Break const &break_range,       // {from, to} 두 int
//!     int unused1, int unused2,       // body 에서 사용 안 됨
//!     SharePtr<Glyph> &output         // 결과를 Append 받는 컨테이너
//! )
//! ```
//!
//! ## 구조 (모든 stage 가 항상 실행)
//!
//! ```text
//! 1. Pre-pads     : 2 boundary glues at start
//! 2. Predecessor  : children[from-1] (조건: from >= 1 && from < to)
//!                   조건 미충족 시 null SharePtr Append (no-op in base/container)
//! 3. Main loop    : children[i] for i in [from..=to]
//!                   from > to 일 때만 LAB_0030616c (extra glue, col_natural 적용)
//! 4. Successor    : children[to+1] (조건: to + 1 < count)
//!                   조건 미충족 시 null SharePtr Append (no-op)
//! 5. Post-pads    : 2 boundary glues at end
//! ```
//!
//! ## Glue byte-level 정수값 (모두 한컴 원본 1:1)
//!
//! | Glue                              | X axis Requirement      | Y axis Requirement      | penalty |
//! |-----------------------------------|-------------------------|-------------------------|---------|
//! | Vert pre-pad 1                    | (-1e8, 0, 0, 0)         | (0, 0, 0, **1.0**)      | 1000    |
//! | Vert pre-pad 2 / post-pad 1 / 2   | (-1e8, 0, 0, 0)         | (0, 0, 0, **0.0**)      | 1000    |
//! | Vert extra (LAB_0030616c)         | (col_w, **1e8**, 0, 0)  | (-1e8, 0, 0, 0)         | 0       |
//! | Horiz pre-pad 1 / 2 / post-pad 1  | (0, 0, 0, 0)            | (-1e8, 0, 0, 0)         | 1000    |
//! | Horiz post-pad 2                  | (0, 0, 0, **1.0**)      | (-1e8, 0, 0, 0)         | 1000    |
//! | Horiz extra (LAB_0030616c)        | (-1e8, 0, 0, 0)         | (col_w, **1e8**, 0, 0)  | 0       |
//!
//! `-1e8` = `Requirement::INVALID_NATURAL` (cross axis "size determined by content").
//!
//! ## Composition::ComposeGlyph (`FUN_002ff824`, sz=380)
//!
//! 입력 item 의 Compose 결과 처리:
//! - replacement 반환 → 사용
//! - replacement 없고 can_break=true → input 자신 사용 (Rust 에선 clone)
//! - 그 외 → null
//!
//! Glyph::Compose base (`FUN_00315998`, sz=20): `*can_break = (bt < 2); *out = null;`
//!
//! ## bt 값 (BreakType) — raw ARM64 어셈블리로 검증
//!
//! - Predecessor (`children[from-1]`): bt = **3 (Penalty)**. 0x306074 `mov w2, #0x3`.
//!   Penalty 면 `can_break=false` → 대부분 item 에서 null 반환 → 실질적 no-op Append.
//!   특수 break marker 가 Compose override 시에만 출력됨.
//! - Main loop (`children[from..=to]`): bt = **0 (Normal)**. 0x30624c `mov w2, #0x0`.
//! - Successor (`children[to+1]`): bt = **1 (Hint)**. 0x306490 `mov w2, #0x1`.
//!
//! ## DAT 상수 (검증된 raw bytes)
//!
//! - `_DAT_00741090` (8B) = `(0.0f, 0.0f)` — horiz post-pad 2 의 X.natural/stretch
//! - `_UNK_00741098` (8B) = `(0.0f, 1.0f)` — horiz post-pad 2 의 X.shrink/alignment
//! - `_DAT_00741f20` (8B) = `(-1e8f, 0.0f)` — horiz extra X.natural/stretch
//! - `_UNK_00741f28` (8B) = `(0.0f, 0.0f)` — horiz extra X.shrink/alignment
//! - `_DAT_00741f40` (8B) = `(1e8f, 0.0f)` — vert extra X.stretch/shrink
//! - `_UNK_00741f48` (8B) = `(0.0f, -1e8f)` — vert extra X.alignment/Y.natural
//! - `_DAT_00742de0` (8B) = `(1e8f, 0.0f)` — horiz extra Y.stretch/shrink

use crate::glyph::{Glue, Glyph};
use crate::value_types::{BreakType, Requirement, Requisition};

// ============================================================
// Break — `Hnc::Shape::Text::Break const&` 의 첫 두 int 필드만 사용
// ============================================================

/// Layout break range — Hancom `Hnc::Shape::Text::Break`.
/// - `+0x00` = from, `+0x04` = to (`Break const&` 의 첫 두 int 필드).
/// - `+0x10` = `flags` — tagged flags word. `Composition::CreateItem` (`FUN_003000a8`) 가
///   `(flags & ~3) | force | 2` 로 갱신: bit0 = force 로 만들어진 line, bit1 = 항상 set.
///   (Hancom struct 의 `+0x08` 영역은 미파악·미사용이라 모델하지 않음 — `flags` 의 실제
///   Rust 오프셋은 `#[repr(C)]` 상 8 이지만 논리적으로 Hancom `+0x10` 에 대응.)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Break {
    pub from: i32,
    pub to: i32,
    pub flags: u64,
}

impl Break {
    pub const fn new(from: i32, to: i32) -> Self { Self { from, to, flags: 0 } }
}

// ============================================================
// Glue 상수 — 한컴 byte-pattern 1:1
// ============================================================

const INVALID_NATURAL: f32 = -1e8;
const STRETCH_HUGE: f32 = 1e8;
const PENALTY_BOUNDARY: i32 = 1000;
const PENALTY_NONE: i32 = 0;

/// Vertical pre-pad 1 (`Type == 2` 경로 의 첫 Glue). Y.alignment = 1.0 (bottom-anchor).
pub(crate) fn vert_pre_pad_1() -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        Requirement::new(0.0, 0.0, 0.0, 1.0),
        PENALTY_BOUNDARY,
    ))
}

/// Vertical pre-pad 2 / post-pad 1 / post-pad 2 — 셋 다 byte-identical.
pub(crate) fn vert_zero_align_glue() -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        Requirement::new(0.0, 0.0, 0.0, 0.0),
        PENALTY_BOUNDARY,
    ))
}

/// Horizontal pre-pad 1 / 2 / post-pad 1 — 셋 다 byte-identical.
pub(crate) fn horiz_left_pad() -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(0.0, 0.0, 0.0, 0.0),
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        PENALTY_BOUNDARY,
    ))
}

/// Horizontal post-pad 2 — X.alignment = 1.0 (right-anchor).
pub(crate) fn horiz_right_pad() -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(0.0, 0.0, 0.0, 1.0),
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        PENALTY_BOUNDARY,
    ))
}

/// Vertical LAB_0030616c — empty range 시 placeholder. X.natural = col_natural,
/// X.stretch = 1e8 (huge flexibility). Y invalid.
fn vert_extra_glue(col_natural: f32) -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(col_natural, STRETCH_HUGE, 0.0, 0.0),
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        PENALTY_NONE,
    ))
}

/// Horizontal LAB_0030616c — empty range 시 placeholder. Y.natural = col_natural,
/// Y.stretch = 1e8. X invalid.
fn horiz_extra_glue(col_natural: f32) -> Glue {
    Glue::new(Requisition::new(
        Requirement::new(INVALID_NATURAL, 0.0, 0.0, 0.0),
        Requirement::new(col_natural, STRETCH_HUGE, 0.0, 0.0),
        PENALTY_NONE,
    ))
}

// ============================================================
// Composition::ComposeGlyph 1:1 포팅
// ============================================================

/// `Hnc::Shape::Text::Composition::ComposeGlyph` (`FUN_002ff824`, sz=380).
///
/// 한컴 원본:
/// ```c
/// void Composition::ComposeGlyph(out, this, &input, bt) {
///   if (*input == null) { *out = null; return; }
///   glyph = **input;
///   if (glyph == null) { *out = null; return; }
///   bool can_break;
///   glyph->Compose(&local, glyph, bt, &can_break);
///   if (local != null && *local != null) {
///     *out = local;  // replacement
///   } else if (can_break) {
///     *out = *input;  // use input itself
///   } else {
///     *out = null;
///   }
/// }
/// ```
///
/// 주의: `this` (Composition*) 는 body 에서 사용 안 됨 → free function 으로 포팅.
pub(crate) fn composition_compose_glyph(item: &dyn Glyph, bt: BreakType) -> Option<Box<dyn Glyph>> {
    let result = item.compose(bt);
    if result.replacement.is_some() {
        result.replacement
    } else if result.can_break {
        Some(item.clone_glyph())
    } else {
        None
    }
}

// ============================================================
// ColCompositor::ComposeLayout 1:1 포팅
// ============================================================

/// `Hnc::Shape::Text::ColCompositor::ComposeLayout` 1:1 포팅.
///
/// - `composition`: Composition (children list). `count = composition.get_count()`,
///   `composition.get_component(i)` 로 i 번째 child 접근.
/// - `composition_type`: `Composition::Type` enum 값. `(type & 0xFFFE) == 2` 이면 vertical.
/// - `break_range`: `{from, to}`.
/// - `output`: `Glyph` container (Append 호출 받는 쪽).
/// - `col_natural_size`: ColCompositor[+8] (constructor 의 첫 float arg). LAB_0030616c
///   (empty range) 시 extra glue 의 cross-axis 크기.
pub fn compose_layout(
    composition: &dyn Glyph,
    composition_type: u32,
    break_range: Break,
    output: &mut dyn Glyph,
    col_natural_size: f32,
) {
    // 한컴: `param_3 = param_3 & 0xfffffffe;` — 최하위 bit 마스킹
    let is_vertical = (composition_type & 0xFFFE) == 2;

    // ── Stage 1: Pre-pads (2 boundary glues) ──────────────────
    if is_vertical {
        output.append_some(Box::new(vert_pre_pad_1()));
        output.append_some(Box::new(vert_zero_align_glue()));
    } else {
        output.append_some(Box::new(horiz_left_pad()));
        output.append_some(Box::new(horiz_left_pad()));
    }

    let from = break_range.from;
    let to = break_range.to;

    // ── Stage 2: Predecessor (children[from-1]) ──────────────
    //
    // 한컴 조건 (asm 검증):
    // - `if (iVar9 < 1) goto LAB_003060d8;`           → null Append (from < 1)
    // - `if (param_4[1] <= iVar9) goto LAB_003060d8;` → null Append (to <= from)
    // - 그 외:                                          → ComposeGlyph + Append (bt=Penalty)
    //
    // **null Append**: Box::Append (FUN_00331810) 가 null SharePtr 도 link 노드 추가
    // (count++). 즉 child list 에 placeholder 가 들어감 — skip 하면 byte-equivalent 아님.
    //
    // bt = Penalty (3) — `mov w2, #0x3` @ 0x306074. can_break=false 라서 대부분 item 의
    // Compose 가 null 반환 → composition_compose_glyph 가 None → append(None) = null Append.
    if from >= 1 && from < to {
        let pred_idx = (from - 1) as usize;
        let c = composition
            .get_component(pred_idx)
            .and_then(|pred| composition_compose_glyph(pred, BreakType::Penalty));
        output.append(c);
    } else {
        // LAB_003060d8 — predecessor skip path. 한컴 명시적 null Append.
        output.append_null();
    }

    // ── Stage 3: Main loop OR extra glue ──────────────────────
    //
    // 한컴 조건:
    // - `if (param_4[1] < iVar9) goto LAB_0030616c;` → to < from 이면 extra glue
    // - else → LAB_003061a4 (main loop)
    //
    // Main loop: do { ... uVar12++; } while (uVar12 < to); — i in [from..=to] 포함.
    // 한컴 line 229-230: ComposeGlyph + Append, both null and non-null cases.
    if from <= to {
        for i in from..=to {
            let c = composition
                .get_component(i as usize)
                .and_then(|item| composition_compose_glyph(item, BreakType::Normal));
            output.append(c);
        }
    } else {
        // LAB_0030616c — extra glue (empty range)
        if is_vertical {
            output.append_some(Box::new(vert_extra_glue(col_natural_size)));
        } else {
            output.append_some(Box::new(horiz_extra_glue(col_natural_size)));
        }
    }

    // ── Stage 4: Successor (children[to+1]) ──────────────────
    //
    // 한컴 조건 (asm line 347, 408-410):
    // - `if (iVar9 < (int)uVar4 + -1)` (= `to < count - 1`) → ComposeGlyph + Append (bt=Hint)
    // - else → null Append (LAB_else 의 `local_68 = 0; Append(&local_68)`)
    let count = composition.get_count() as i32;
    if to + 1 < count {
        let succ_idx = (to + 1) as usize;
        let c = composition
            .get_component(succ_idx)
            .and_then(|succ| composition_compose_glyph(succ, BreakType::Hint));
        output.append(c);
    } else {
        // LAB_else — successor skip path. 한컴 명시적 null Append.
        output.append_null();
    }

    // ── Stage 5: Post-pads (2 boundary glues) ─────────────────
    if is_vertical {
        // 한컴 post-pad 1 과 2 byte-identical, 둘 다 vert_zero_align_glue 와 동일.
        output.append_some(Box::new(vert_zero_align_glue()));
        output.append_some(Box::new(vert_zero_align_glue()));
    } else {
        // 한컴 post-pad 1 은 horiz_left_pad, post-pad 2 는 horiz_right_pad (X.alignment=1.0).
        output.append_some(Box::new(horiz_left_pad()));
        output.append_some(Box::new(horiz_right_pad()));
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::ComposeResult;

    /// 테스트용 container — Append 호출 순서를 기록.
    #[derive(Debug)]
    struct RecordingContainer {
        appended_count: usize,
        appended_kinds: Vec<&'static str>,
    }

    impl Glyph for RecordingContainer {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(Self {
                appended_count: self.appended_count,
                appended_kinds: self.appended_kinds.clone(),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn append(&mut self, child: Option<Box<dyn Glyph>>) {
            // 한컴 Box::Append 1:1: null SharePtr 도 link 노드 추가 (count++).
            self.appended_count += 1;
            self.appended_kinds.push(match child {
                Some(_) => "some",
                None => "null",
            });
        }
    }

    /// 테스트용 composition — children Vec 와 count 노출.
    #[derive(Debug)]
    struct MockComposition {
        children: Vec<Box<dyn Glyph>>,
    }

    impl Glyph for MockComposition {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            let children: Vec<Box<dyn Glyph>> =
                self.children.iter().map(|c| c.clone_glyph()).collect();
            Box::new(Self { children })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn get_count(&self) -> usize { self.children.len() }
        fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
            self.children.get(idx).map(|b| b.as_ref())
        }
    }

    /// SimpleItem — Compose 에서 can_break=true, no replacement.
    /// 즉 composition_compose_glyph 가 이 자신을 clone 하여 반환.
    #[derive(Debug, Clone)]
    struct SimpleItem;

    impl Glyph for SimpleItem {
        fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn compose(&self, bt: BreakType) -> ComposeResult {
            ComposeResult {
                replacement: None,
                can_break: (bt as u32) < 2,
            }
        }
    }

    fn mock_with_n_items(n: usize) -> MockComposition {
        MockComposition {
            children: (0..n).map(|_| Box::new(SimpleItem) as Box<dyn Glyph>).collect(),
        }
    }

    fn new_recorder() -> RecordingContainer {
        RecordingContainer { appended_count: 0, appended_kinds: Vec::new() }
    }

    #[test]
    fn vertical_pre_pad_1_byte_pattern() {
        let g = vert_pre_pad_1();
        // X = (-1e8, 0, 0, 0)
        assert_eq!(g.req.x.natural, INVALID_NATURAL);
        assert_eq!(g.req.x.stretch, 0.0);
        assert_eq!(g.req.x.shrink, 0.0);
        assert_eq!(g.req.x.alignment, 0.0);
        // Y = (0, 0, 0, 1.0)
        assert_eq!(g.req.y.natural, 0.0);
        assert_eq!(g.req.y.stretch, 0.0);
        assert_eq!(g.req.y.shrink, 0.0);
        assert_eq!(g.req.y.alignment, 1.0);
        // penalty
        assert_eq!(g.req.penalty, 1000);
    }

    #[test]
    fn vertical_post_pad_byte_pattern() {
        let g = vert_zero_align_glue();
        // 한컴 vertical post-pad 1, 2 와 pre-pad 2 가 byte-identical.
        assert_eq!(g.req.x.natural, INVALID_NATURAL);
        assert_eq!(g.req.y.alignment, 0.0);
        assert_eq!(g.req.penalty, 1000);
    }

    #[test]
    fn horizontal_left_pad_byte_pattern() {
        let g = horiz_left_pad();
        // X = (0, 0, 0, 0), Y = (-1e8, 0, 0, 0)
        assert_eq!(g.req.x.natural, 0.0);
        assert_eq!(g.req.x.alignment, 0.0);
        assert_eq!(g.req.y.natural, INVALID_NATURAL);
        assert_eq!(g.req.penalty, 1000);
    }

    #[test]
    fn horizontal_right_pad_byte_pattern() {
        let g = horiz_right_pad();
        // X = (0, 0, 0, 1.0) — 차이점: alignment=1.0
        assert_eq!(g.req.x.alignment, 1.0);
        assert_eq!(g.req.y.natural, INVALID_NATURAL);
        assert_eq!(g.req.penalty, 1000);
    }

    #[test]
    fn vertical_extra_glue_byte_pattern() {
        let g = vert_extra_glue(123.45);
        // X = (col_w, 1e8, 0, 0), Y = (-1e8, 0, 0, 0)
        assert_eq!(g.req.x.natural, 123.45);
        assert_eq!(g.req.x.stretch, STRETCH_HUGE);
        assert_eq!(g.req.x.shrink, 0.0);
        assert_eq!(g.req.x.alignment, 0.0);
        assert_eq!(g.req.y.natural, INVALID_NATURAL);
        assert_eq!(g.req.penalty, 0);
    }

    #[test]
    fn horizontal_extra_glue_byte_pattern() {
        let g = horiz_extra_glue(67.89);
        assert_eq!(g.req.x.natural, INVALID_NATURAL);
        assert_eq!(g.req.y.natural, 67.89);
        assert_eq!(g.req.y.stretch, STRETCH_HUGE);
        assert_eq!(g.req.penalty, 0);
    }

    #[test]
    fn compose_glyph_returns_clone_when_can_break() {
        let item = SimpleItem;
        let result = composition_compose_glyph(&item, BreakType::Normal);
        assert!(result.is_some(), "Normal (bt=0) → can_break=true → return clone");
    }

    #[test]
    fn compose_glyph_returns_none_when_forced_break() {
        // SimpleItem 의 compose: can_break = bt < 2. bt=2 (Forced) 면 can_break=false.
        let item = SimpleItem;
        let result = composition_compose_glyph(&item, BreakType::Forced);
        assert!(result.is_none(), "Forced (bt=2) → can_break=false → return None");
    }

    #[test]
    fn compose_layout_empty_composition_horizontal() {
        // composition 비어있고 range = (0, -1) (from > to) → extra glue path
        // 한컴 1:1: Pre-pad(2) + null pred(1) + extra glue(1) + null succ(1) + Post-pad(2) = 7
        let composition = mock_with_n_items(0);
        let mut output = new_recorder();
        let break_range = Break::new(0, -1);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        assert_eq!(output.appended_count, 7);
        // null Append 위치 검증: index 2 (pred), index 4 (succ)
        assert_eq!(output.appended_kinds[2], "null");
        assert_eq!(output.appended_kinds[4], "null");
    }

    #[test]
    fn compose_layout_empty_composition_vertical() {
        let composition = mock_with_n_items(0);
        let mut output = new_recorder();
        let break_range = Break::new(0, -1);
        compose_layout(&composition, 2, break_range, &mut output, 50.0);
        // Same structure as horizontal: 7 appends with null pred + null succ
        assert_eq!(output.appended_count, 7);
    }

    #[test]
    fn compose_layout_single_item_range_horizontal() {
        // 1 item composition, range = (0, 0) → main loop runs once, null pred + null succ
        // Pre-pad(2) + null pred (from<1) + main(1) + null succ (to+1>=count) + Post-pad(2) = 7
        let composition = mock_with_n_items(1);
        let mut output = new_recorder();
        let break_range = Break::new(0, 0);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        assert_eq!(output.appended_count, 7);
    }

    #[test]
    fn compose_layout_with_predecessor_and_successor() {
        // 5 items, range = (1, 2)
        //   from=1: predecessor block 진입 (from >= 1 && from < to). bt=Penalty.
        //     SimpleItem 의 Compose: can_break = (bt < 2) = false → composition_compose_glyph 가
        //     None 반환 → output.append(None) = null Append (count++).
        //   main: children[1], children[2] — bt=Normal → can_break=true → Append(Some) (2)
        //   to=2, count=5, to+1=3 < 5 → succ at children[3]. bt=Hint → null (SimpleItem) → null Append
        // Total: pre-pad(2) + pred(1 null) + main(2) + succ(1) + post-pad(2) = 8
        let composition = mock_with_n_items(5);
        let mut output = new_recorder();
        let break_range = Break::new(1, 2);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        assert_eq!(output.appended_count, 8);
    }

    #[test]
    fn compose_layout_no_predecessor_when_from_is_zero() {
        // from = 0 → 한컴: iVar9 < 1 goto LAB_003060d8 (null Append)
        let composition = mock_with_n_items(5);
        let mut output = new_recorder();
        let break_range = Break::new(0, 1);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        // Pre(2) + null pred(1) + main(2: children[0,1]) + succ(1) + post(2) = 8
        assert_eq!(output.appended_count, 8);
        // pred (index 2) is null
        assert_eq!(output.appended_kinds[2], "null");
    }

    #[test]
    fn compose_layout_no_successor_when_to_is_last() {
        let composition = mock_with_n_items(5);
        let mut output = new_recorder();
        // range = (1, 4): main = [1,2,3,4], pred block 진입 (bt=Penalty → null),
        // no succ (4+1=5, !<5) → null Append
        let break_range = Break::new(1, 4);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        // Pre(2) + null pred(1) + main(4) + null succ(1) + post(2) = 10
        assert_eq!(output.appended_count, 10);
    }

    #[test]
    fn compose_layout_no_pred_when_from_equals_to() {
        // from = to: 한컴 조건 from < to 안 맞아 pred null Append
        let composition = mock_with_n_items(5);
        let mut output = new_recorder();
        let break_range = Break::new(2, 2);
        compose_layout(&composition, 0, break_range, &mut output, 100.0);
        // Pre(2) + null pred(1) + main(1: children[2]) + succ(1) + post(2) = 7
        assert_eq!(output.appended_count, 7);
    }

    #[test]
    fn compose_layout_extra_glue_for_inverted_range() {
        // from=3, to=1 (from > to) → main loop skipped, extra glue added
        let composition = mock_with_n_items(5);
        let mut output = new_recorder();
        let break_range = Break::new(3, 1);
        compose_layout(&composition, 0, break_range, &mut output, 250.0);
        // Pre(2) + null pred (from=3 >= 1 BUT from=3 NOT < to=1) + extra(1) +
        //   succ check: to=1, to+1=2 < count=5 → succ(1, SimpleItem null) + post(2) = 7
        assert_eq!(output.appended_count, 7);
    }

    /// bt 값 검증용 — Compose 호출 시 bt 를 RefCell 에 누적 기록. 모든 bt 에서 replacement
    /// 반환 (can_break 무관) 으로 predecessor/main/successor 가 다 호출되는 환경 시뮬레이션.
    #[derive(Debug)]
    struct TrackingItem {
        observed_bts: std::cell::RefCell<Vec<BreakType>>,
    }

    impl Glyph for TrackingItem {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(TrackingItem {
                observed_bts: std::cell::RefCell::new(self.observed_bts.borrow().clone()),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn compose(&self, bt: BreakType) -> ComposeResult {
            self.observed_bts.borrow_mut().push(bt);
            // Penalty 라도 replacement 가 있으면 ComposeGlyph 가 그걸 반환
            // → predecessor 호출 추적 가능
            ComposeResult {
                replacement: Some(Box::new(SimpleItem)),
                can_break: false,
            }
        }
    }

    #[test]
    fn predecessor_called_with_penalty_bt() {
        // 5 items, all TrackingItem. range = (1, 2).
        // - pred at children[0]: bt=3 (Penalty)
        // - main: children[1] bt=0 (Normal), children[2] bt=0 (Normal)
        // - succ: children[3] bt=1 (Hint)
        let children: Vec<Box<dyn Glyph>> = (0..5)
            .map(|_| Box::new(TrackingItem { observed_bts: std::cell::RefCell::new(Vec::new()) }) as Box<dyn Glyph>)
            .collect();
        let composition = MockComposition { children };
        let mut output = new_recorder();
        compose_layout(&composition, 0, Break::new(1, 2), &mut output, 100.0);

        // 각 item 이 본 bt 확인
        let item0 = composition.get_component(0).unwrap();
        let item1 = composition.get_component(1).unwrap();
        let item2 = composition.get_component(2).unwrap();
        let item3 = composition.get_component(3).unwrap();
        let item4 = composition.get_component(4).unwrap();

        let bts0 = unsafe { &*(item0 as *const dyn Glyph as *const TrackingItem) }.observed_bts.borrow().clone();
        let bts1 = unsafe { &*(item1 as *const dyn Glyph as *const TrackingItem) }.observed_bts.borrow().clone();
        let bts2 = unsafe { &*(item2 as *const dyn Glyph as *const TrackingItem) }.observed_bts.borrow().clone();
        let bts3 = unsafe { &*(item3 as *const dyn Glyph as *const TrackingItem) }.observed_bts.borrow().clone();
        let bts4 = unsafe { &*(item4 as *const dyn Glyph as *const TrackingItem) }.observed_bts.borrow().clone();

        assert_eq!(bts0, vec![BreakType::Penalty], "predecessor uses Penalty");
        assert_eq!(bts1, vec![BreakType::Normal], "main loop uses Normal");
        assert_eq!(bts2, vec![BreakType::Normal], "main loop uses Normal");
        assert_eq!(bts3, vec![BreakType::Hint], "successor uses Hint");
        assert_eq!(bts4, Vec::<BreakType>::new(), "out-of-range item not called");
    }

    #[test]
    fn vertical_type_with_low_bit_set() {
        // type = 3 → (3 & 0xFFFE) = 2 → vertical
        let composition = mock_with_n_items(0);
        let mut output = new_recorder();
        let break_range = Break::new(0, -1);
        compose_layout(&composition, 3, break_range, &mut output, 100.0);
        assert_eq!(output.appended_count, 7);  // 2 pre + null pred + 1 extra + null succ + 2 post
    }
}
