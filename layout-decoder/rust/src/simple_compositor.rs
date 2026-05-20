//! `Hnc::Shape::Text::SimpleCompositor` 의 vfunc body 1:1 포팅.
//!
//! RTTI: `SimpleCompositor` vtable `@ 0x7804a0`, object vptr `@ 0x7804b0`.
//! ctor `0x303de0` / `0x303df0` (`SimpleCompositor()`) — raw 는 **vptr 만 set, field 없음**:
//! ```c
//! *(undefined ***)this = &PTR__SimpleCompositor_007804b0;
//! return;
//! ```
//! → `SimpleCompositor` 는 state 없는 빈 struct.
//!
//! vfunc (object-vptr-relative):
//! - `+0x18 ComposeNumbering` (`0x303e30`) / `+0x20 ComposeBullet` (`0x303e34`) — raw `ret`
//!   no-op (`return param_1`). → `Compositor` trait default 사용.
//! - `+0x28 ComposeBreak` (`0x303e38`, 328B) — `simple_compose_break`, 본 파일.
//! - `+0x30 ComposeLayout` (`0x303f80`, 2440B) — `simple_compose_layout`, 본 파일.
//!   `ArrayCompositor::ComposeLayout` (`0x304d5c`) 와 decompile 完全 동일 (LAB_ 주소만
//!   차이) → 두 subclass 가 본 함수를 공유.

use crate::compose_layout::{
    composition_compose_glyph, horiz_left_pad, horiz_right_pad, vert_pre_pad_1,
    vert_zero_align_glue, Break,
};
use crate::glyph::Glyph;
use crate::value_types::BreakType;

// ============================================================
// SimpleCompositor::ComposeBreak  (FUN_00303e38, size 328)
// ============================================================

/// `Hnc::Shape::Text::SimpleCompositor::ComposeBreak` 1:1 포팅.
///
/// raw C++ signature (decompile):
/// ```c
/// ulong SimpleCompositor::ComposeBreak(
///     vector<float> const& param_1,   // x1 — widths (각 item 의 자연 width)
///     vector<float> const& param_2,   // x2 — stretches  (body 미사용)
///     vector<float> const& param_3,   // x3 — shrinks    (body 미사용)
///     vector<int>   const& param_4,   // x4 — penalties  (body 미사용)
///     vector<float> const& param_5,   // x5 — heights = 라인별 가용 width
///     Composition const*   param_6,   // x6 — Composition* (body 미사용)
///     int param_7,                    // x7 — from        (body 미사용)
///     int param_8,                    // stack0 — to      (body 미사용)
///     vector<int>&  param_9)          // stack1 — &output (in/out, pre-sized)
/// ```
///
/// raw asm 첫 줄에서 `ldr x0,[x29,#0x18]` 으로 x0(this) 를 즉시 param_9 로 덮어씀 → **`this`
/// 미사용** (SimpleCompositor 는 state 없음). body 에서 실제 읽는 인자는 `widths`(x1) +
/// `heights`(x5) + `output`(param_9) 뿐.
///
/// ## output cap = `widths.len()`
///
/// raw 는 `n_w = widths.size()`, `n_out = output.size()` 를 별도로 읽지만, **유일한 호출처
/// `Composition::Repair` 에서 widths(`local_98`) 와 output(`local_110`) 이 항상 같은 크기
/// `uVar23` 으로 동시 resize** (`repair.txt` line 153-206: 6개 vector 를 `uVar23` 으로 일괄
/// resize, 이후 line 337 에서 `heights` 만 추가 resize). 따라서 `n_out == n_w == widths.len()`.
/// 본 포팅은 이 invariant 를 사용해 `n = widths.len()` 하나로 처리한다.
///
/// ## 알고리즘 (greedy line-fill, raw 0x303e38-0x303f7c)
///
/// `n_w <= 0` → output 비우고 0 반환. 그 외:
/// - `uVar15` = output write index (0..), `w1` = 현재 시작 char (0..)
/// - 매 line:
///   - `w1 == n-1` → 마지막 char: `w2=n-1, w3=n`
///   - else: `avail = heights[min(heights.len()-1, uVar15)]`, `w1` 부터 width 누적,
///     `acc > avail` 인 첫 char `u1` 발견 시: `u1!=w1` → `w2=u1-1, w3=u1`; `u1==w1` (첫
///     char 가 이미 초과) → `w2=w1, w3=w1+1`. 끝까지 fit → `w2=n-1, w3=n`.
///   - `output[uVar15]=w2`, `uVar15++`, `w1=w3`. `w3 >= n` 이면 종료.
/// 반환 = 최종 output 길이 (= line count).
///
/// raw decompile 인용 (핵심 분기):
/// ```c
/// if ((int)uVar13 < 1) { ... }                  // n_w < 1 → 빈 경로
/// else { do { ... inner accumulate ... } while ((int)param_4 < (int)uVar13); }
/// // inner: fVar18 += widths[w1+pc]; if (heights[iVar2] < fVar18) break;
/// // 0x303eac: iVar2 = min(iVar5, uVar15)  (iVar5 = heights.size()-1)
/// ```
pub fn simple_compose_break(widths: &[f32], heights: &[f32]) -> Vec<u32> {
    // raw 0x303e48-64: n_w = widths.size(); subs w12,w11,#1; b.lt 0x303f4c (n_w<=0).
    let n_w: i32 = widths.len() as i32;
    if n_w < 1 {
        // raw 0x303f4c-7c: n_w<=0 → n_out(=n_w)==0 이므로 output 길이 0 반환.
        //   (w15=0; w10 = (0<n_out)?0:n_out; n_out==0 → w10=0; x19=0; 0>=0 → resize 없이 0).
        return Vec::new();
    }

    // raw 0x303e70-7c: iVar5 = heights.size() - 1  (라인별 가용 width 배열 인덱스 상한).
    let i_var5: i32 = heights.len() as i32 - 1;

    // raw 0x303e68-6c: uVar15(write idx)=0, w1(current pos)=0.
    let mut u_var15: i32 = 0;
    let mut w1: i32 = 0;
    // raw 0x303e84: uVar16 = w9 & ~(w9 ASR 31) = max(n_out, 0). n_out==n_w>0 → uVar16=n_w.
    let u_var16: i32 = n_w; // max(n_out, 0), n_out == n_w > 0

    let mut out: Vec<u32> = Vec::new();

    loop {
        // raw 0x303e88-8c: 기본값 w2 = n_w-1, w3 = n_w.
        let w2: i32;
        let w3: i32;

        // raw 0x303e90-94: w1 == n_w-1 → b.eq 0x303f04 (w2=n_w-1, w3=n_w 유지).
        if w1 == n_w - 1 {
            w2 = n_w - 1;
            w3 = n_w;
        } else if w1 >= n_w {
            // raw 0x303e98-9c: w1 >= n_w → b.ge 0x303efc (방어적; w1∈[0,n_w-1] 에선 미발생).
            w3 = w1 + 1;
            w2 = w1;
        } else {
            // raw 0x303ea0-edc: inner accumulate loop.
            // raw 0x303ea8-ac: iVar2 = (iVar5 < uVar15) ? iVar5 : uVar15  (= min).
            let i_var2 = if i_var5 < u_var15 { i_var5 } else { u_var15 };
            // raw 0x303eb0: s0 = heights[iVar2].
            let avail = heights[i_var2 as usize];
            // raw 0x303eb8: fVar18 = 0.0.
            let mut acc: f32 = 0.0;
            // raw 0x303ebc: 내부 trip count = n_w - w1.
            let trip = n_w - w1;
            let mut found_u1: Option<i32> = None;
            // raw 0x303ec0-d8: for pc in 0..trip { acc += widths[w1+pc]; if avail < acc break; }
            let mut pc: i32 = 0;
            while pc != trip {
                acc += widths[(w1 + pc) as usize];
                // raw 0x303ec8-cc: fcmp s1,s0; b.gt 0x303ef0  → avail < acc.
                if avail < acc {
                    // raw 0x303ef0: uVar1 = w1 + pc.
                    found_u1 = Some(w1 + pc);
                    break;
                }
                pc += 1;
            }
            match found_u1 {
                Some(u1) => {
                    // raw 0x303ef4-f8: uVar1 != uVar8(=w1) ?
                    if u1 != w1 {
                        // raw 0x303ee8: w2 = uVar1 - 1; b 0x303f04 (w3 = uVar1).
                        w2 = u1 - 1;
                        w3 = u1;
                    } else {
                        // raw 0x303efc-f00: w3 = w1+1, w2 = w1.
                        w3 = w1 + 1;
                        w2 = w1;
                    }
                }
                None => {
                    // raw 0x303edc-ec: inner loop 완주 → x3 = n_w.
                    //   cmp w3(n_w),w2(w1); b.eq 0x303efc 는 n_w==w1 일 때만 (여기선 w1<n_w-1<n_w
                    //   이라 미발생) → 항상 sub w2,w3,#1: w2 = n_w-1, w3 = n_w.
                    w2 = n_w - 1;
                    w3 = n_w;
                }
            }
        }

        // raw 0x303f04-08: uVar15 == uVar16 → b.eq 0x303f24 (종료, uVar16 유지).
        if u_var15 == u_var16 {
            break;
        }
        // raw 0x303f0c: output[uVar15] = w2.
        out.push(w2 as u32);
        // raw 0x303f10-14: uVar15++; w1 = w3.
        u_var15 += 1;
        w1 = w3;
        // raw 0x303f18-1c: w3 < n_w → loop; else fall (uVar16 = uVar15).
        if w3 >= n_w {
            // raw 0x303f20: uVar16 = uVar15.
            break;
        }
    }

    // raw 0x303f24-7c: final = min(uVar16, n_out). uVar16 <= n_w 이고 우리 `out` 은 정확히
    //   uVar16 개를 push 했으므로 (loop 종료 시 out.len()==uVar16) final == out.len().
    //   raw 의 truncate/grow 분기는 항상 no-op → `out` 그대로 반환.
    out
}

// ============================================================
// SimpleCompositor::ComposeLayout  (FUN_00303f80, size 2440)
//   = ArrayCompositor::ComposeLayout (FUN_00304d5c) — decompile 完全 동일
// ============================================================

/// `Hnc::Shape::Text::SimpleCompositor::ComposeLayout` (`FUN_00303f80`) 1:1 포팅.
/// `ArrayCompositor::ComposeLayout` (`FUN_00304d5c`) 와 decompile 完全 동일 (LAB_ 주소만
/// 차이) → 두 subclass 공유.
///
/// raw C++ signature:
/// ```c
/// void SimpleCompositor::ComposeLayout(
///     SimpleCompositor *this,         // x0 — this (state 없음, body 미사용)
///     Composition const *param_1,     // x1 — composition (children list)
///     Composition::Type param_3,      // x2 — type. `(type & 0xFFFE) == 2` → vertical
///     Break const &param_4,           // x3 — {from, to}
///     int param_5, int param_6,       // x4, x5 — body 미사용
///     SharePtr<Glyph> &param_7)       // x6 — output container (Append 받는 쪽)
/// ```
///
/// ## `ColCompositor::ComposeLayout` (`compose_layout::compose_layout`) 와의 차이
///
/// 구조 (Pre-pad → Predecessor → Main loop → Successor → Post-pad) 와 모든 Glue
/// byte-pattern 은 동일하나 **두 가지가 다름**:
/// 1. **Predecessor 조건**: raw 0x304220-0x304230 — `0 < from` (= `from >= 1`) **만** 검사.
///    ColCompositor 의 `from < to` 추가 조건이 **없다**. `from >= 1` 이면 무조건
///    `children[from-1]` 을 ComposeGlyph(Penalty) → Append.
/// 2. **Main loop 미실행 시 extra glue 없음**: raw 0x304368-0x304490 — `from <= to` 면
///    main loop, 아니면 **아무것도 안 함** (ColCompositor 의 `LAB_0030616c` extra-glue 경로가
///    없음). 따라서 `col_natural_size` 인자 자체가 불필요.
///
/// ## bt 값 (raw asm 검증)
///
/// - Predecessor: `mov w2,#0x3` @ 0x304280 → bt = 3 (Penalty).
/// - Main loop:   `mov w2,#0x0` @ 0x304538 → bt = 0 (Normal).
/// - Successor:   `mov w2,#0x1` @ 0x304408 / decompile `ComposeGlyph(...,1)` → bt = 1 (Hint).
///
/// ## Glue byte-pattern (raw 0x303fc0-0x304884 의 `operator_new(0x30)` + 필드 write)
///
/// - vert pre-pad: `vert_pre_pad_1` (`+0x24`=`0x3e83f800000` → Y.align=1.0, penalty=1000),
///   `vert_zero_align_glue` (`+0x24`=`0x3e800000000` → Y.align=0.0, penalty=1000).
/// - horiz pre-pad: `horiz_left_pad` ×2 (`+0x18`=-1e8 = Y.natural, `+0x24`=`0x3e800000000`).
/// - vert post-pad: `vert_zero_align_glue` ×2.
/// - horiz post-pad: `horiz_left_pad`, `horiz_right_pad` (후자는 `+0x08`=`_DAT_00741090`
///   =(0,0), `+0x10`=`_UNK_00741098`=(0,1.0) → X.align=1.0).
///   → `compose_layout.rs` 의 검증된 helper 와 byte-identical.
///
/// ## out-of-range 처리
///
/// raw 는 `count <= idx` 시 `___cxa_throw` ("GetAt" `out_of_range`). 본 포팅은 기존
/// `compose_layout::compose_layout` 와 동일하게 `get_component` 의 `None` (= graceful
/// null Append) 로 처리 — 정상 break range (`0 <= from,to < count`) 에선 throw 미발생.
pub fn simple_compose_layout(
    composition: &dyn Glyph,
    composition_type: u32,
    break_range: Break,
    output: &mut dyn Glyph,
) {
    // raw 0x303fbc-c0: param_3 = param_3 & 0xfffffffe; cmp #2 → vertical.
    let is_vertical = (composition_type & 0xFFFE) == 2;

    // ── Stage 1: Pre-pads (2 boundary glues) — raw 0x303fc4-0x304204 ──
    if is_vertical {
        output.append_some(Box::new(vert_pre_pad_1()));
        output.append_some(Box::new(vert_zero_align_glue()));
    } else {
        output.append_some(Box::new(horiz_left_pad()));
        output.append_some(Box::new(horiz_left_pad()));
    }

    let from = break_range.from;
    let to = break_range.to;

    // ── Stage 2: Predecessor (children[from-1]) — raw 0x304220-0x304340 ──
    //
    // raw 0x304220 `if (0 < iVar11) goto LAB_00304230` / 0x30431c `if (iVar11 < 1) goto
    // LAB_00304310` — 조건은 **`from >= 1` 뿐** (ColCompositor 와 달리 `from < to` 없음).
    // `from < 1` → LAB_00304310: `local_58=0; Append(&local_58)` = 명시적 null Append.
    if from >= 1 {
        // raw 0x304230: uVar1 = from - 1. 0x304240 `if (count <= from-1) throw "GetAt"`.
        let pred_idx = (from - 1) as usize;
        // raw 0x304280 `mov w2,#0x3` → ComposeGlyph(children[from-1], Penalty).
        let c = composition
            .get_component(pred_idx)
            .and_then(|pred| composition_compose_glyph(pred, BreakType::Penalty));
        output.append(c);
    } else {
        // raw LAB_00304310 — predecessor skip 시 명시적 null Append.
        output.append_null();
    }

    // ── Stage 3: Main loop (children[from..=to]) — raw 0x304368-0x304600 ──
    //
    // raw 0x304368 `if (from <= to) goto LAB_00304490` — `from <= to` 면 main loop,
    // 아니면 **아무것도 안 함** (ColCompositor 의 extra-glue 경로 없음).
    // LAB_00304490 do-while: uVar12 = from; while (uVar12 < to) uVar12++ → from..=to 포함.
    if from <= to {
        for i in from..=to {
            // raw 0x304538 `mov w2,#0x0` → ComposeGlyph(children[i], Normal).
            let c = composition
                .get_component(i as usize)
                .and_then(|item| composition_compose_glyph(item, BreakType::Normal));
            output.append(c);
        }
    }

    // ── Stage 4: Successor (children[to+1]) — raw 0x3043ec-0x304624 ──
    //
    // raw 0x3043ec: count = composition->GetCount() (vfunc +0x80). 0x304400
    // `if (to < count - 1)` → ComposeGlyph(children[to+1], Hint); else null Append.
    let count = composition.get_count() as i32;
    if to < count - 1 {
        let succ_idx = (to + 1) as usize;
        // raw 0x304408 `mov w2,#0x1` → ComposeGlyph(children[to+1], Hint).
        let c = composition
            .get_component(succ_idx)
            .and_then(|succ| composition_compose_glyph(succ, BreakType::Hint));
        output.append(c);
    } else {
        // raw 0x304618: successor skip 시 명시적 null Append.
        output.append_null();
    }

    // ── Stage 5: Post-pads (2 boundary glues) — raw 0x304624-0x304884 ──
    if is_vertical {
        output.append_some(Box::new(vert_zero_align_glue()));
        output.append_some(Box::new(vert_zero_align_glue()));
    } else {
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

    #[test]
    fn empty_widths_returns_empty() {
        // raw: n_w < 1 → 빈 경로, output 길이 0.
        assert_eq!(simple_compose_break(&[], &[100.0]), Vec::<u32>::new());
    }

    #[test]
    fn single_char_is_last_char() {
        // n_w=1 → w1==n_w-1==0 → w2=0, w3=1. out=[0]. w3>=n_w 종료.
        assert_eq!(simple_compose_break(&[10.0], &[100.0]), vec![0]);
    }

    #[test]
    fn all_fit_one_line() {
        // 3 chars × 10, avail 100. w1=0 (!=2): inner accumulate 10,20,30 모두 <=100 →
        // found_u1=None → w2=n_w-1=2, w3=n_w=3. out=[2]. w3>=3 종료.
        assert_eq!(simple_compose_break(&[10.0, 10.0, 10.0], &[100.0]), vec![2]);
    }

    #[test]
    fn greedy_break_when_overflow() {
        // 4 chars × 10, avail 25. heights=[25] (len 1 → iVar5=0).
        // line0: uVar15=0, iVar2=min(0,0)=0, avail=25. acc: 10,20,30>25 at pc=2 → u1=2.
        //   u1!=w1(0) → w2=1, w3=2. out=[1]. uVar15=1, w1=2.
        // line1: w1=2 (!=3): iVar2=min(0,1)=0, avail=25. acc: 10,20 (pc 0,1), pc=2 → trip=n_w-w1=2
        //   so pc runs 0,1 only: acc=10,20 both <=25 → found_u1=None → w2=n_w-1=3, w3=n_w=4.
        //   out=[1,3]. uVar15=2, w1=4. w3>=4 종료.
        assert_eq!(
            simple_compose_break(&[10.0, 10.0, 10.0, 10.0], &[25.0]),
            vec![1, 3]
        );
    }

    #[test]
    fn first_char_already_overflows() {
        // 3 chars × 30, avail 10. line0: w1=0, acc=30>10 at pc=0 → u1=0. u1==w1 →
        //   w2=w1=0, w3=w1+1=1. out=[0]. uVar15=1, w1=1.
        // line1: w1=1 (!=2): iVar2=min(0,1)=0, avail=10. acc=30>10 at pc=0 → u1=1. u1==w1 →
        //   w2=1, w3=2. out=[0,1]. uVar15=2, w1=2.
        // line2: w1=2==n_w-1 → w2=2, w3=3. out=[0,1,2]. w3>=3 종료.
        assert_eq!(
            simple_compose_break(&[30.0, 30.0, 30.0], &[10.0]),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn per_line_avail_from_heights_index() {
        // heights 가 line 별로 다른 가용 width: heights=[5, 100] (iVar5=1).
        // 4 chars × 10.
        // line0: uVar15=0, iVar2=min(1,0)=0, avail=heights[0]=5. acc=10>5 at pc=0 → u1=0.
        //   u1==w1 → w2=0, w3=1. out=[0]. uVar15=1, w1=1.
        // line1: w1=1 (!=3): iVar2=min(1,1)=1, avail=heights[1]=100. trip=n_w-w1=3.
        //   acc: 10,20,30 all <=100 → found_u1=None → w2=n_w-1=3, w3=n_w=4. out=[0,3].
        //   uVar15=2, w1=4. w3>=4 종료.
        assert_eq!(
            simple_compose_break(&[10.0, 10.0, 10.0, 10.0], &[5.0, 100.0]),
            vec![0, 3]
        );
    }

    // ── simple_compose_layout 테스트 ──────────────────────────

    use crate::glyph::ComposeResult;

    /// Append 호출 순서/종류를 기록하는 테스트 container.
    #[derive(Debug)]
    struct RecordingContainer {
        kinds: Vec<&'static str>,
    }
    impl Glyph for RecordingContainer {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(Self { kinds: self.kinds.clone() })
        }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn append(&mut self, child: Option<Box<dyn Glyph>>) {
            self.kinds.push(match child {
                Some(_) => "some",
                None => "null",
            });
        }
    }

    /// children Vec + count 노출하는 테스트 composition.
    #[derive(Debug)]
    struct MockComposition {
        children: Vec<Box<dyn Glyph>>,
    }
    impl Glyph for MockComposition {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(Self {
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

    /// Compose: can_break = bt < 2, no replacement. → composition_compose_glyph 가
    /// can_break 면 self clone 반환.
    #[derive(Debug, Clone)]
    struct SimpleItem;
    impl Glyph for SimpleItem {
        fn clone_glyph(&self) -> Box<dyn Glyph> { Box::new(self.clone()) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn compose(&self, bt: BreakType) -> ComposeResult {
            ComposeResult { replacement: None, can_break: (bt as u32) < 2 }
        }
    }

    fn mock(n: usize) -> MockComposition {
        MockComposition {
            children: (0..n).map(|_| Box::new(SimpleItem) as Box<dyn Glyph>).collect(),
        }
    }
    fn rec() -> RecordingContainer {
        RecordingContainer { kinds: Vec::new() }
    }

    #[test]
    fn layout_predecessor_runs_when_from_ge_1_even_if_from_not_lt_to() {
        // 핵심 차이: SimpleCompositor 는 predecessor 가 `from >= 1` 만 검사.
        // from=2, to=2 (from == to, NOT from < to): ColCompositor 라면 null pred 지만
        // SimpleCompositor 는 children[1] 을 ComposeGlyph(Penalty)+Append.
        //   Penalty(bt=3) → SimpleItem can_break = 3<2 = false → composition_compose_glyph
        //   → None → append(None) = "null". 그래도 pred "stage" 는 실행됨 (1 append).
        // 구성: pre(2 some) + pred(1) + main[2..=2](1) + succ(to=2,count=5,2<4 → 1) + post(2 some)
        let c = mock(5);
        let mut out = rec();
        simple_compose_layout(&c, 0, Break::new(2, 2), &mut out);
        assert_eq!(out.kinds.len(), 7);
        // pred (index 2): Penalty → None → "null".
        assert_eq!(out.kinds[2], "null");
        // main children[2]: Normal → can_break → Some.
        assert_eq!(out.kinds[3], "some");
    }

    #[test]
    fn layout_no_extra_glue_when_from_gt_to() {
        // from=3, to=1 (from > to): main loop 미실행 + extra glue 없음.
        // 구성: pre(2) + pred(from=3>=1 → children[2] Penalty → "null") + (main 없음) +
        //   succ(to=1, count=5, 1<4 → children[2] Hint → SimpleItem can_break=1<2=true →
        //   Some) + post(2).  총 6 appends.
        let c = mock(5);
        let mut out = rec();
        simple_compose_layout(&c, 0, Break::new(3, 1), &mut out);
        assert_eq!(out.kinds.len(), 6, "no extra glue → 2+1+0+1+2 = 6");
        assert_eq!(out.kinds, vec!["some", "some", "null", "some", "some", "some"]);
    }

    #[test]
    fn layout_null_predecessor_when_from_is_zero() {
        // from=0 → predecessor null Append (raw LAB_00304310).
        let c = mock(5);
        let mut out = rec();
        simple_compose_layout(&c, 0, Break::new(0, 1), &mut out);
        // pre(2) + null pred(1) + main[0..=1](2) + succ(to=1,count=5 → 1) + post(2) = 8
        assert_eq!(out.kinds.len(), 8);
        assert_eq!(out.kinds[2], "null");
    }

    #[test]
    fn layout_null_successor_when_to_is_last() {
        // to = count-1 → successor null Append.
        let c = mock(5);
        let mut out = rec();
        simple_compose_layout(&c, 0, Break::new(1, 4), &mut out);
        // pre(2) + pred(children[0] Penalty → "null") + main[1..=4](4) + null succ(1) + post(2) = 10
        assert_eq!(out.kinds.len(), 10);
        assert_eq!(out.kinds[2], "null"); // pred (Penalty → None)
        assert_eq!(out.kinds[7], "null"); // succ (to+1 == count → null Append)
    }

    #[test]
    fn layout_main_loop_inclusive_range() {
        // from=1, to=3 → main loop children[1,2,3] (3개, inclusive).
        let c = mock(6);
        let mut out = rec();
        simple_compose_layout(&c, 0, Break::new(1, 3), &mut out);
        // pre(2) + pred(1) + main(3) + succ(to=3,count=6,3<5 → 1) + post(2) = 9
        assert_eq!(out.kinds.len(), 9);
        // main children[1..=3] 은 Normal → can_break → "some" (index 3,4,5).
        assert_eq!(&out.kinds[3..6], &["some", "some", "some"]);
    }

    #[test]
    fn layout_vertical_vs_horizontal_glue_count_same() {
        // vertical (type & 0xFFFE == 2) 과 horizontal 둘 다 동일 stage 수 (Glue 종류만 차이).
        let c = mock(3);
        let mut out_v = rec();
        let mut out_h = rec();
        simple_compose_layout(&c, 2, Break::new(0, 1), &mut out_v); // vertical
        simple_compose_layout(&c, 0, Break::new(0, 1), &mut out_h); // horizontal
        assert_eq!(out_v.kinds.len(), out_h.kinds.len());
        // pre(2) + null pred(from=0) + main[0..=1](2) + succ(to=1,count=3,1<2 → 1) + post(2) = 8
        assert_eq!(out_v.kinds.len(), 8);
    }

    #[test]
    fn layout_type_low_bit_set_is_vertical() {
        // type=3 → (3 & 0xFFFE) == 2 → vertical 경로. stage 수는 동일.
        let c = mock(0);
        let mut out = rec();
        simple_compose_layout(&c, 3, Break::new(0, -1), &mut out);
        // pre(2) + null pred(from=0) + (main 없음: from=0 > to=-1) + null succ(count=0) + post(2) = 6
        assert_eq!(out.kinds.len(), 6);
    }
}
