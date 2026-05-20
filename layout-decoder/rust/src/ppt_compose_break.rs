//! `Hnc::Shape::Text::PptCompositor::ComposeBreak` (`FUN_00307af4`, 1520B) 1:1 포팅.
//!
//! `Compositor::compose_break` 의 PptCompositor override. ColCompositor 는 별도 알고리즘
//! (`compose_break.rs`), Simple/Array 는 `simple_compose_break`/`array_compose_break`.
//! PptCompositor 만 paragraph 의 ParaProperty/Bullet/BodyProperty 를 읽어 **first-line
//! indent** 를 계산하고, penalty-aware greedy line-breaking 을 수행한다.
//!
//! ## raw C++ signature
//!
//! ```c
//! ulong PptCompositor::ComposeBreak(
//!     PptCompositor *this,            // x0 — this (state 없음, body 미사용)
//!     vector<float> const &param_1,   // x1 — widths
//!     vector<float> const &param_2,   // x2 — stretches (미사용)
//!     vector<float> const &param_3,   // x3 — shrinks   (미사용)
//!     vector<int>   const &param_4,   // x4 — penalties
//!     vector<float> const &param_5,   // x5 — heights
//!     Composition   const *param_6,   // x6 — composition
//!     int param_7,                    // x7 — from
//!     int param_8,                    // stack — to
//!     vector<int>         &param_9)    // stack — output (break char index 들)
//! ```
//!
//! 반환 = output 에 쓴 break entry 개수 (= line count). raw 는 `output` 을 그 길이로
//! resize. Rust 모델은 `Vec<u32>` 반환 — `breaks.len()` 이 곧 line count.
//!
//! ## 알고리즘 개요
//!
//! ### Phase 1 — first-line indent `fVar28` (+ `fVar30`, `fVar31`)
//!
//! raw 0x307b1c-308064 + 0x3080a4-e0. `GetParaItemView(composition, to)` 의 ParaProperty
//! 에서 3개 float key 를 읽는다 (raw `mov w9,#0x901 / #0x8ff / #0x900`):
//! - `fVar32` (= `a`)  = `bag.get_float(0x901)`
//! - `fVar30` (= `b`)  = `bag.get_float(0x8ff)` — Phase 2 의 **subsequent-line indent**
//! - `fVar31`          = `bag.get_float(0x900)` — Phase 2 의 **line margin**
//!
//! ParaProperty 가 없으면 셋 다 0. `IsFirstLineOnPara(composition, from)` 이 false 면
//! `fVar28 = a + b`. true 면 bullet 기반 ascent `c` (= `fVar29`) 를 구해서 case 분석:
//!
//! ```text
//! if abs(a) >= c:  a >= 0       → a + b
//!                  a < 0, c <= 0 → a + b
//!                  a < 0, c > 0  → b
//! else (abs(a)<c): a >= 0       → b + c
//!                  a < 0        → a + b + c
//! ```
//!
//! `c` 계산 (raw 0x307cc8-0x308064): bullet 이 valid 하고 `Bullet::GetType() != 0` 이면
//! `char_view = GetFirstCharItemViewOnPara(composition, from + 1)` 의 render path
//! (`+0x98`) vtable[3] (`FirstLineMetrics`) 와 BodyProperty (`+0x28`) 의 `GetVert` 로:
//! `body_property` null → `default_ascent`, else → `pick_for_paragraph_class(GetVert())`.
//! (raw `6 < uVar5` / `(1<<vert)&0x65` 분기 = `FirstLineMetrics::pick_for_paragraph_class`.)
//!
//! ### Phase 2 — penalty-aware greedy line breaking
//!
//! raw 0x307dc4-0x308058. line 별로:
//! - `line_idx = min(out_count, heights.len()-1)`, `indent = (out_count==0)?fVar28:fVar30`
//! - `avail = heights[line_idx] - indent - fVar31`
//! - `widths[pos], widths[pos+1], ...` 누적 (`acc`). `acc > avail` 면 overflow.
//!   누적 도중 `pos + j == n` 도달하면 line 이 다 들어감 → `break_idx = n-1, next = n`.
//! - overflow 시 `pen = penalties[pos+j]` 로 분기:
//!   - `pen >= 2`: `pos+j` 부터 앞으로 `penalties[scan] < 2` 인 첫 위치 scan →
//!     `break_idx = scan-1, next = scan` (못 찾으면 `n-1, n`).
//!   - `pen == 0, j > 0`: `pos+j` 부터 뒤로 backward scan #1 (`penalties ∈ {1,10,50}`),
//!     없으면 backward scan #2 (`penalties == 1`), 없으면 `break_idx = pos+j-1, next = pos+j`.
//!   - `pen == 1, j > 0`: `break_idx = pos+j-1, next = pos+j`.
//!   - `pen ∈ {0,1}, j == 0` (단일 char 가 line 초과): `next = pos+1`. `pen==0` 은
//!     `(next < n-1) || penalties[next] == -1000` 면 `break_idx = pos` emit, 아니면
//!     break 없이 `pos` 만 전진. `pen==1` 은 `next < n-1` 면 `break_idx = pos` emit,
//!     아니면 (`penalties[pos] == -1000` 분기 — pen==1 이라 dead) break 없이 전진.
//!
//! ## raw decompile 인용 (Phase 1 핵심)
//!
//! ```c
//! puVar19 = *(undefined8 **)(*local_88 + 0x18);          // ParaProperty 의 PropertyBag
//! pfVar8 = (float *)FUN_0065616c(*puVar19,&local_c0);     // PropertyBag::Get<float>(key)
//! fVar32 = *pfVar8;                                       // key 0x901
//! ...
//! iVar4 = IsFirstLineOnPara(pPVar9,param_6,param_7);
//! if (iVar4 == 0) goto LAB_00307dc0;                      // fVar28 = fVar32 + fVar30
//! local_90 = *(long **)(*local_88 + 8);                   // ParaProperty.+0x08 = Bullet
//! ... (**(code **)(*(long *)*local_90 + 0x30))() ...      // Bullet::GetType()
//! lVar6 = GetFirstCharItemViewOnPara(pPVar9,param_6,param_7 + 1);
//! plVar25 = *(long **)(lVar6 + 0x98);                     // render path SharePtr
//! (**(code **)(*plVar2 + 0x18))(plVar2,&local_c0);        // render path vtable[3]
//! local_c8 = *(undefined8 **)(lVar6 + 0x28);              // BodyProperty SharePtr
//! uVar5 = BodyProperty::GetVert((BodyProperty *)*local_c8);
//! if (6 < uVar5) ... if ((1 << (uVar5 & 0x1f) & 0x65U) == 0) ... // pick default/special
//! ```
//!
//! ## raw decompile 인용 (Phase 2 overflow 분기)
//!
//! ```c
//! if ((heights[iVar11] - fVar32) - fVar31 < fVar29) {     // overflow
//!   uVar12 = lVar14 + (uVar12 & 0xffffffff);              // new_pos = pos + j
//!   uVar5 = penalties[new_pos];
//!   if (1 < uVar5) goto LAB_00307eb8;                     // pen >= 2 → forward scan
//!   if (uVar5 != 1) { /* pen == 0 */
//!     if (iVar10 < iVar11) { ... goto LAB_00307f34; }     // j > 0 → backward scans
//!     uVar13 = uVar12 + 1;
//!     if (((int)uVar13 < (int)(uVar26-1)) || (penalties[uVar13] == -1000)) goto LAB_00307eec;
//!     goto LAB_00307efc;                                  // break 없이 전진
//!   }
//!   /* pen == 1 */
//!   if (iVar10 < iVar11) goto LAB_00307fd4;               // j > 0
//!   uVar13 = uVar12 + 1;
//!   if ((int)uVar13 < (int)(uVar26-1)) goto LAB_00307eec;
//!   if (penalties[pos] == -1000) goto LAB_00307fd4;
//!   goto LAB_00307efc;
//! }
//! ```
//!
//! ## 정공법 메모
//!
//! - `stretches`/`shrinks` (param_2/param_3) 는 raw body 에서 미참조 → `_` prefix.
//! - raw `FUN_0065616c` (`PropertyBag::Get<float>`) 는 key 부재 시 `out_of_range` throw.
//!   ParaProperty ctor (`0x31abf8`) 가 0x8ff/0x900/0x901 을 항상 채우므로 (text_property.rs
//!   참조) 본 모델은 `unwrap_or(0.0)` — invariant 하에 fallback 미도달.
//! - raw `iVar4` (output cap) = 호출처 `Composition::Repair` 가 output 을 `widths.len()`
//!   로 resize 한 값 → `cap = widths.len()`. raw 의 `if (iVar4 <= iVar21)` 는 push 전
//!   `out_count >= cap` 체크로 1:1.
//! - **잠재 OOB**: pen==0,j==0 분기의 `penalties[next]` 는 `next == n` 일 수 있음
//!   (`pos == n-1` 에서 단일 char overflow). raw 는 `vector` over-capacity 에 의존하는
//!   latent bug. 본 모델은 `.get()` 으로 OOB → `None` → "`-1000` 아님" 처리 (조건의 의도
//!   = forced-break marker 검출, one-past-end 는 forced break 가 아님).

use crate::glyph::{first_line_ascent_from_render_path, Glyph};
use crate::ppt_compositor::{
    get_first_char_item_view_on_para, get_para_item_view, is_first_line_on_para,
};
use crate::properties::{PropertyBag, PropertyKey};

/// raw `mov w9,#0x901` — Phase 1 첫 번째 float key (`fVar32` / `a`).
const KEY_901: u32 = 0x901;
/// raw `mov w9,#0x8ff` — Phase 1 두 번째 float key (`fVar30` / `b`, subsequent-line indent).
const KEY_8FF: u32 = 0x8ff;
/// raw `mov w9,#0x900` — Phase 1 세 번째 float key (`fVar31`, line margin).
const KEY_900: u32 = 0x900;

/// raw `0x4000000000402` — backward scan #1 의 penalty 비트마스크. bits {1, 10, 50}.
/// raw: `if (uVar5 < 0x33 && (1L << (uVar5 & 0x3f) & 0x4000000000402U) != 0)`.
const PENALTY_BREAK_MASK: u64 = 0x0004_0000_0000_0402;

/// 한 line 의 break 결정. raw 의 `LAB_00307eec` (output write 후 전진) vs `LAB_00307efc`
/// (write 없이 전진) 분기.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineDecision {
    /// raw `LAB_00307eec` → `LAB_00307efc`: `output[out_count++] = break_idx`, `pos = next_pos`.
    Emit { break_idx: i32, next_pos: i32 },
    /// raw `LAB_00307efc` 직행: output write 없이 `pos = next_pos`.
    SkipEmit { next_pos: i32 },
}

/// `Hnc::Shape::Text::PptCompositor::ComposeBreak` 1:1 포팅.
///
/// `cap` (raw `iVar4`) 은 호출처 `Composition::Repair` 가 output 을 `widths.len()` 로
/// resize 한 값 → `widths.len()`. 반환 `Vec<u32>` 의 길이 = raw return value (line count).
#[allow(clippy::too_many_arguments)]
pub fn ppt_compose_break(
    widths: &[f32],
    _stretches: &[f32],
    _shrinks: &[f32],
    penalties: &[i32],
    heights: &[f32],
    composition: &dyn Glyph,
    from: i32,
    to: i32,
) -> Vec<u32> {
    // raw 0x307b1c-38: n = widths.len(); penalties.len() != n → return 0.
    let n = widths.len();
    if penalties.len() != n {
        return Vec::new();
    }

    // ── Phase 1: first-line indent ───────────────────────────────────────────
    let (fvar28, fvar30, fvar31) = compute_phase1_indents(composition, from, to);

    // ── Phase 2: penalty-aware greedy line breaking ──────────────────────────
    let n_i32 = n as i32;
    // raw iVar4 — output cap. Composition::Repair 가 output 을 widths.len() 로 resize.
    let cap = n_i32;
    // raw iVar3 = heights.size() - 1. invariant: heights.len() >= 1 (Repair 가 resize;
    // raw 도 빈 heights 면 heights[-1] OOB).
    let max_h = heights.len() as i32 - 1;

    let mut breaks: Vec<u32> = Vec::new();
    let mut pos: i32 = 0; // raw uVar12
    let mut out_count: i32 = 0; // raw iVar21

    // raw 0x307dd0-d4: `if (0 < (int)uVar26)` — n <= 0 면 loop 진입 안 함.
    if n_i32 > 0 {
        loop {
            // raw 0x307e10-1c: line_idx = min(out_count, max_h);
            //                  indent = (out_count == 0) ? fVar28 : fVar30.
            let line_idx = out_count.min(max_h);
            let indent = if out_count == 0 { fvar28 } else { fvar30 };

            // raw 0x307e2c-38: avail = heights[line_idx] - indent - fVar31.
            let avail = heights[line_idx as usize] - indent - fvar31;

            let i10 = pos; // raw iVar10
            let mut acc = 0.0_f32; // raw fVar29 (Phase 2 에서 width 누적기로 재사용)
            let mut j: i32 = 0; // raw lVar14

            // raw 0x307e48-74: width 누적 do-while.
            let decision = loop {
                // raw 0x307e50-54: fVar29 += widths[pos + j].
                acc += widths[(i10 + j) as usize];
                // raw 0x307e58-5c: `b.gt` — acc > avail → overflow.
                if avail < acc {
                    break handle_overflow(i10, j, n_i32, penalties);
                }
                // raw 0x307e64-70: j++; while (pos + j != n).
                j += 1;
                if i10 + j == n_i32 {
                    // raw 0x307e74 → 0x307ee0: line 이 다 들어감.
                    break LineDecision::Emit {
                        break_idx: n_i32 - 1,
                        next_pos: n_i32,
                    };
                }
            };

            match decision {
                LineDecision::Emit {
                    break_idx,
                    next_pos,
                } => {
                    // raw LAB_00307eec: `if (iVar4 <= iVar21) goto LAB_00307fe8;`
                    if out_count >= cap {
                        break;
                    }
                    // raw: `output[iVar21] = uVar12; iVar21++;`
                    breaks.push(break_idx as u32);
                    out_count += 1;
                    // raw LAB_00307efc: `uVar12 = uVar13; if (uVar26 <= uVar12) goto done;`
                    pos = next_pos;
                    if next_pos >= n_i32 {
                        break;
                    }
                }
                LineDecision::SkipEmit { next_pos } => {
                    // raw LAB_00307efc 직행 (output write / cap 체크 없음).
                    pos = next_pos;
                    if next_pos >= n_i32 {
                        break;
                    }
                }
            }
        }
    }

    // raw LAB_00307ff4: output 을 min(out_count, cap) 로 resize. push 시 cap 을 지켰으므로
    // breaks.len() == out_count == min(out_count, cap) — 추가 처리 불필요.
    breaks
}

/// Phase 1 — `(fVar28, fVar30, fVar31)` 계산. raw 0x307b40-0x308064 + 0x3080a4-e0.
///
/// - `fVar28` — first-line indent (Phase 2 의 line 0 indent).
/// - `fVar30` — key 0x8ff (Phase 2 의 subsequent-line indent).
/// - `fVar31` — key 0x900 (Phase 2 의 line margin).
///
/// ParaProperty 가 없으면 `(0, 0, 0)` (raw `LAB_00307c1c` → `LAB_00307dc0`).
fn compute_phase1_indents(composition: &dyn Glyph, from: i32, to: i32) -> (f32, f32, f32) {
    // raw 0x307b68-c14: para_view = GetParaItemView(composition, to). null 이거나
    //   ParaProperty SharePtr 가 invalid 면 local_88 = null.
    let para_view = get_para_item_view(composition, to);
    let para_prop = match para_view.as_ref().and_then(|pv| pv.para_property.as_ref()) {
        // raw LAB_00307c1c → LAB_00307dc0: fVar32 = fVar30 = fVar31 = 0, fVar28 = 0.
        None => return (0.0, 0.0, 0.0),
        Some(pp) => pp,
    };

    // raw 0x307bf8-cb8: 3개 float key 를 PropertyBag::Get<float> 로 읽음.
    // raw 의 FUN_0065616c 는 key 부재 시 throw — ctor invariant 하 unwrap_or(0.0) 미도달.
    let a = para_prop
        .property_bag
        .get_float(PropertyKey::new(KEY_901))
        .unwrap_or(0.0); // fVar32
    let b = para_prop
        .property_bag
        .get_float(PropertyKey::new(KEY_8FF))
        .unwrap_or(0.0); // fVar30
    let fvar31 = para_prop
        .property_bag
        .get_float(PropertyKey::new(KEY_900))
        .unwrap_or(0.0); // fVar31

    // raw 0x307cc0-c4: `if (IsFirstLineOnPara(composition, from) == 0) goto LAB_00307dc0;`
    if !is_first_line_on_para(composition, from) {
        // raw LAB_00307dc0: fVar28 = fVar32 + fVar30.
        return (a + b, b, fvar31);
    }

    // raw 0x307cc8-0x308064: bullet 기반 ascent c (= fVar29) 계산.
    let mut c = 0.0_f32;
    // raw 0x307cd0-14: local_90 = ParaProperty.+0x08 (Bullet SharePtr); valid && GetType()!=0.
    if let Some(bullet) = para_prop.get_bullet() {
        if bullet.get_type() != 0 {
            // raw 0x307d18-28: char_view = GetFirstCharItemViewOnPara(composition, from + 1).
            if let Some(char_view) = get_first_char_item_view_on_para(composition, from + 1) {
                // raw 0x307d2c-44: render_path = char_view.+0x98 (SharePtr<RenderPath>).
                if let Some(render_path) = char_view.render_path.as_deref() {
                    // raw 0x307d48-db8 + 0x308060-64:
                    //   render path vtable[3] → Requisition buffer.
                    //   BodyProperty (char_view.+0x28) null → default_ascent.
                    //   else → GetVert() 로 default/special 선택 (= pick_for_paragraph_class):
                    //     `6 < vert` OR `(1<<vert)&0x65 == 0` → default, else → special.
                    c = match &char_view.body_property {
                        None => first_line_ascent_from_render_path(render_path, 7),
                        Some(bp) => first_line_ascent_from_render_path(render_path, bp.get_vert()),
                    };
                }
            }
        }
    }

    // raw 0x3080a4-e0: 최종 case 분석. a = fVar32, b = fVar30, c = fVar29.
    let fvar28 = if a.abs() >= c {
        // raw 0x3080ac `b.pl` — ABS(fVar32) >= fVar29.
        if a >= 0.0 {
            // raw 0x3080c8 `b.ge` → LAB_00307dc0.
            a + b
        } else if c <= 0.0 {
            // raw 0x3080d0 `b.le` → LAB_00307dc0.
            a + b
        } else {
            // raw 0x3080d4: fmov s0,s8 → fVar28 = fVar30.
            b
        }
    } else {
        // raw: ABS(fVar32) < fVar29.
        if a >= 0.0 {
            // raw 0x3080dc: fadd s0,s8,s11 → fVar28 = fVar30 + fVar29.
            b + c
        } else {
            // raw 0x3080b8-bc: fadd s0,s10,s8; fadd s0,s0,s11 → fVar28 = fVar32 + fVar30 + fVar29.
            a + b + c
        }
    };

    (fvar28, b, fvar31)
}

/// Phase 2 의 overflow 핸들러. raw 0x307e78-0x307fe4 의 모든 LAB 경로.
///
/// `i10` = 현재 line 시작 pos, `j` = overflow 가 난 누적 offset (`new_pos = i10 + j`),
/// `n` = widths.len(), `penalties` = penalty 배열.
fn handle_overflow(i10: i32, j: i32, n: i32, penalties: &[i32]) -> LineDecision {
    // raw 0x307e20-24 / 0x307e78: new_pos = pos + j; pen = penalties[new_pos].
    let new_pos = i10 + j;
    let pen = penalties[new_pos as usize];

    // raw 0x307e84-88: `if (1 < uVar5) goto LAB_00307eb8;` — pen >= 2 → forward scan.
    if pen >= 2 {
        // raw LAB_00307eb8.
        // raw 0x307eb8-bc: `if (new_pos >= n)` → break_idx = new_pos-1, next = new_pos.
        if new_pos >= n {
            return LineDecision::Emit {
                break_idx: new_pos - 1,
                next_pos: new_pos,
            };
        }
        // raw 0x307ec4-edc: scan = new_pos 부터 forward, `penalties[scan] < 2` 인 첫 위치.
        let mut scan = new_pos;
        loop {
            // raw 0x307ec8-d0: `if (penalties[scan] < 2)` (unsigned cmp).
            //   penalties[scan] 는 int 이지만 raw 는 `cmp w5,#0x2; b.cc` (unsigned) —
            //   음수면 huge → `< 2` false. `(0..2).contains` 가 동일.
            if (0..2).contains(&penalties[scan as usize]) {
                // raw 0x307f0c-18: break_idx = scan-1, next = scan.
                return LineDecision::Emit {
                    break_idx: scan - 1,
                    next_pos: scan,
                };
            }
            // raw 0x307ed4-dc: scan++; while (scan != n).
            scan += 1;
            if scan == n {
                // raw 0x307ee0-e8: break_idx = n-1, next = n.
                return LineDecision::Emit {
                    break_idx: n - 1,
                    next_pos: n,
                };
            }
        }
    }

    // raw 0x307e8c-90: `if (uVar5 != 1)` — pen == 0.
    if pen == 0 {
        // raw 0x307f24-28: `if (iVar10 < iVar11)` — pos < new_pos (j > 0).
        if i10 < new_pos {
            // ── backward scan #1 — penalties ∈ {1, 10, 50} (raw LAB_00307f34) ──
            // raw: uVar17 = new_pos 부터 뒤로, `lVar16 < uVar17` (= pos < uVar17) 동안.
            let mut u17 = new_pos;
            loop {
                let p = penalties[u17 as usize];
                // raw 0x307f38-44: `p < 0x33 && (1 << (p & 0x3f)) & PENALTY_BREAK_MASK != 0`.
                //   p 가 음수면 raw 의 unsigned `uVar5 < 0x33` 가 false → (0..0x33) 가 동일.
                if (0..0x33).contains(&p)
                    && (1u64 << (p as u64 & 0x3f)) & PENALTY_BREAK_MASK != 0
                {
                    // raw 0x307f48 → 0x307f7c: `if (new_pos > uVar17)` → emit, else break.
                    if u17 < new_pos {
                        return LineDecision::Emit {
                            break_idx: u17,
                            next_pos: u17 + 1,
                        };
                    }
                    break;
                }
                // raw 0x307f4c-54: uVar17--; while (pos < uVar17).
                u17 -= 1;
                if !(i10 < u17) {
                    break;
                }
            }
            // ── backward scan #2 — penalties == 1 (raw LAB_00307f8c) ──
            // raw: uVar18 = new_pos 부터 뒤로 (별도 카운터), `lVar16 < uVar18` 동안.
            let mut u18 = new_pos;
            loop {
                // raw 0x307f8c-94: `if (penalties[uVar18] == 1)`.
                if penalties[u18 as usize] == 1 {
                    // raw 0x307fc0-cc: `if (new_pos > uVar18)` → emit, else break (→ LAB_00307fd4).
                    if u18 < new_pos {
                        return LineDecision::Emit {
                            break_idx: u18,
                            next_pos: u18 + 1,
                        };
                    }
                    break;
                }
                // raw 0x307f98-a0: uVar18--; while (pos < uVar18).
                u18 -= 1;
                if !(i10 < u18) {
                    break;
                }
            }
            // raw LAB_00307fd4: break_idx = pos+j-1, next = pos+j.
            return LineDecision::Emit {
                break_idx: new_pos - 1,
                next_pos: new_pos,
            };
        }
        // raw 0x307f5c-78: pen == 0, j == 0 (new_pos == pos).
        let next = new_pos + 1;
        // raw: `if ((next < n-1) || (penalties[next] == -1000)) goto LAB_00307eec;`
        //   penalties[next] 는 next == n 일 수 있음 (latent OOB) — .get() 으로 graceful.
        if next < n - 1 || penalties.get(next as usize).copied() == Some(-1000) {
            return LineDecision::Emit {
                break_idx: new_pos,
                next_pos: next,
            };
        }
        // raw 0x307fe0-e4: goto LAB_00307efc — break 없이 전진.
        return LineDecision::SkipEmit { next_pos: next };
    }

    // raw: pen == 1.
    // raw 0x307e94-98: `if (iVar10 < iVar11) goto LAB_00307fd4;` — j > 0.
    if i10 < new_pos {
        // raw LAB_00307fd4: break_idx = pos+j-1, next = pos+j.
        return LineDecision::Emit {
            break_idx: new_pos - 1,
            next_pos: new_pos,
        };
    }
    // raw 0x307e9c-b0: pen == 1, j == 0.
    let next = new_pos + 1;
    // raw: `if ((int)uVar13 < (int)(uVar26-1)) goto LAB_00307eec;`
    if next < n - 1 {
        return LineDecision::Emit {
            break_idx: new_pos,
            next_pos: next,
        };
    }
    // raw 0x307fa8-b4 (LAB_00307fa8): `if (penalties[pos] == -1000) goto LAB_00307fd4;`
    //   pen == 1 이라 penalties[i10] (== penalties[new_pos] == pen) 은 1 — 이 분기는
    //   logically dead 이지만 raw 1:1 유지.
    if penalties[i10 as usize] == -1000 {
        return LineDecision::Emit {
            break_idx: new_pos - 1,
            next_pos: new_pos,
        };
    }
    // raw 0x307fb8-bc: goto LAB_00307efc — break 없이 전진.
    LineDecision::SkipEmit { next_pos: next }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::{CharItemView, ComposeResult, FirstLineMetrics};
    use crate::properties::{HashMapPropertyBag, PropertyValue};
    use crate::text_property::{BodyProperty, Bullet, ParaProperty};
    use crate::value_types::BreakType;

    // ── 테스트용 mock composition (ppt_compose_numbering.rs 패턴) ──────────────

    /// `Compose` 가 자신의 `CharItemView` clone 을 replacement 로 반환 — `get_para_item_view`
    /// / `is_first_line_on_para` / `get_first_char_item_view_on_para` 의 dynamic_cast 대응.
    #[derive(Debug)]
    struct ParaItem {
        view: CharItemView,
    }
    impl Glyph for ParaItem {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(ParaItem {
                view: self.view.clone(),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn compose(&self, _bt: BreakType) -> ComposeResult {
            ComposeResult {
                replacement: Some(Box::new(self.view.clone())),
                can_break: false,
            }
        }
    }

    #[derive(Debug)]
    struct MockComposition {
        children: Vec<Box<dyn Glyph>>,
    }
    impl Glyph for MockComposition {
        fn clone_glyph(&self) -> Box<dyn Glyph> {
            Box::new(MockComposition {
                children: self.children.iter().map(|c| c.clone_glyph()).collect(),
            })
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn get_count(&self) -> usize {
            self.children.len()
        }
        fn get_component(&self, idx: usize) -> Option<&dyn Glyph> {
            self.children.get(idx).map(|b| b.as_ref())
        }
    }

    /// CR 없는 빈 composition — `get_para_item_view` 가 None → Phase 1 = (0, 0, 0).
    fn dummy_comp() -> MockComposition {
        MockComposition { children: vec![] }
    }

    // ── Phase 0: 입력 검증 ───────────────────────────────────────────────────

    #[test]
    fn penalties_len_mismatch_returns_empty() {
        // raw 0x307b34-38: penalties.len() != widths.len() → return 0.
        let widths = [1.0, 1.0];
        let penalties = [0]; // len 1 != 2
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn empty_widths_returns_empty() {
        // n == 0 → Phase 2 loop 진입 안 함.
        let out = ppt_compose_break(&[], &[], &[], &[], &[10.0], &dummy_comp(), 0, 0);
        assert!(out.is_empty());
    }

    // ── Phase 2: 기본 greedy ─────────────────────────────────────────────────

    #[test]
    fn single_line_fits_all() {
        // widths 합 < avail → 한 줄에 다 들어감 → break_idx = n-1.
        let widths = [1.0, 1.0, 1.0];
        let penalties = [5, 5, 5];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![2]);
    }

    #[test]
    fn overflow_pen_ge_2_scans_forward() {
        // widths=[4,4,4], avail=10. j=2 에서 acc=12 > 10 overflow, new_pos=2.
        // penalties[2]=5 >= 2 → forward scan: scan=2(5,>=2), scan=3==n → break_idx=2,next=3.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [5, 5, 5];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![2]);
    }

    #[test]
    fn overflow_pen_eq_1_j_gt_0_breaks_before() {
        // widths=[4,4,4], avail=10. overflow new_pos=2, penalties[2]=1, j=2>0
        //   → break_idx = new_pos-1 = 1, next = 2. line1: widths[2]=4 fits → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    fn overflow_pen_eq_1_j_eq_0_single_char() {
        // widths=[20,5,5], avail=10. j=0 에서 acc=20>10 overflow, new_pos=0, penalties[0]=1.
        //   j==0, next=1, `next < n-1` (1<2) → break_idx=0, next=1.
        //   line1 pos=1: widths[1]+widths[2]=10, `10<10` false, pos+j==3 → break_idx=2.
        let widths = [20.0, 5.0, 5.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![0, 2]);
    }

    #[test]
    fn overflow_pen_eq_1_j_eq_0_at_end_skip_emits() {
        // widths=[5,5,20], avail=10. line0: acc 5,10 (10<10 false), j=2 acc=30>10 overflow
        //   new_pos=2, penalties[2]=1, j=2>0 → break_idx=1, next=2.
        // line1 pos=2: widths[2]=20>10 overflow new_pos=2, penalties[2]=1, j==0.
        //   next=3, `3 < n-1=2` false. penalties[i10=2]=1 != -1000 → SkipEmit(next=3).
        //   pos=3 >= n → 종료. break 없음 → out = [1].
        let widths = [5.0, 5.0, 20.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![1]);
    }

    #[test]
    fn overflow_pen_eq_0_j_gt_0_fallback() {
        // widths=[4,4,4], avail=10. overflow new_pos=2, penalties[2]=0, j=2>0.
        //   backward scan #1 ({1,10,50}): u17=2(0 no), u17=1(0 no) → 못 찾음.
        //   backward scan #2 (==1): u18=2(0 no), u18=1(0 no) → 못 찾음.
        //   LAB_00307fd4: break_idx = new_pos-1 = 1, next = 2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 0, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    fn overflow_pen_eq_0_j_gt_0_backward_scan_finds_break() {
        // widths=[3,3,3,3], avail=10. line0: acc 3,6,9 (각 ok), j=3 acc=12>10 overflow
        //   new_pos=3, penalties[3]=0, j=3>0 → backward scan #1:
        //   u17=3(0 no), u17=2(0 no), u17=1(penalties[1]=10 → (1<<10)&mask != 0 → match!)
        //   u17=1 < new_pos=3 → break_idx=1, next=2.
        // line1 pos=2: widths[2]+widths[3]=6, pos+j==4 → break_idx=3.
        let widths = [3.0, 3.0, 3.0, 3.0];
        let penalties = [0, 10, 0, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![1, 3]);
    }

    #[test]
    fn overflow_pen_eq_0_j_gt_0_all_zero_uses_fallback() {
        // 위 테스트와 동일 widths 지만 penalties 전부 0 → backward scan 둘 다 못 찾음 →
        //   LAB_00307fd4: break_idx = new_pos-1 = 2, next = 3 (penalty 10 케이스와 대비).
        let widths = [3.0, 3.0, 3.0, 3.0];
        let penalties = [0, 0, 0, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![2, 3]);
    }

    #[test]
    fn overflow_pen_eq_0_j_eq_0_skip_emits_when_not_near_end() {
        // widths=[20,3,3,3,3], avail=10. line0: j=0 acc=20>10 overflow new_pos=0,
        //   penalties[0]=0, j==0. next=1. `next < n-1` (1 < 4) → break_idx=0, next=1.
        let widths = [20.0, 3.0, 3.0, 3.0, 3.0];
        let penalties = [0, 0, 0, 0, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        // line0 break_idx=0,next=1. line1 pos=1: widths[1..4]=3,6,9 (ok), j=3 acc=12>10
        //   overflow new_pos=4, penalties[4]=0, j=3>0 → backward scan 못 찾음 →
        //   LAB_00307fd4 break_idx=3,next=4. line2 pos=4: widths[4]=3, pos+j==5 → break_idx=4.
        assert_eq!(out, vec![0, 3, 4]);
    }

    #[test]
    fn overflow_pen_eq_0_j_eq_0_at_end_skip_emits() {
        // widths=[3,3,20], avail=10. line0: acc 3,6 (ok), j=2 acc=26>10 overflow
        //   new_pos=2, penalties[2]=0, j=2>0 → backward scan 못 찾음 → break_idx=1,next=2.
        // line1 pos=2: widths[2]=20>10 overflow new_pos=2, penalties[2]=0, j==0.
        //   next=3. `next < n-1=1`? no. penalties.get(3) = None != Some(-1000) →
        //   SkipEmit(next=3). pos=3 >= n → 종료. out = [1].
        let widths = [3.0, 3.0, 20.0];
        let penalties = [0, 0, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        assert_eq!(out, vec![1]);
    }

    #[test]
    fn overflow_pen_eq_0_j_eq_0_forced_break_marker_emits() {
        // widths=[20,3,20], avail=10. line0: j=0 acc=20>10 overflow new_pos=0,
        //   penalties[0]=0, j==0. next=1. `next < n-1=1`? no (1<1 false).
        //   penalties.get(1) = Some(-1000) → break_idx=0, next=1.
        let widths = [20.0, 3.0, 20.0];
        let penalties = [0, -1000, 0];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        // line0 break_idx=0,next=1. line1 pos=1: widths[1]=3 ok, j=1 acc += widths[2]=20
        //   → 23>10 overflow new_pos=2, penalties[2]=0, j=1>0 → backward scan #1:
        //   u17=2(0 no), u17 -= 1 → 1, `i10(1) < 1`? no → break. scan #2 동일 → break.
        //   LAB_00307fd4: break_idx = new_pos-1 = 1, next = 2.
        // line2 pos=2: widths[2]=20>10 overflow new_pos=2, penalties[2]=0, j==0.
        //   next=3. `3 < n-1=2`? no. penalties.get(3)=None → SkipEmit(next=3). 종료.
        assert_eq!(out, vec![0, 1]);
    }

    // ── cap (output 한계) ────────────────────────────────────────────────────

    #[test]
    fn respects_output_cap() {
        // 모든 char 가 단일 char overflow → 각 line 1 char. cap = widths.len() = n.
        // out_count 이 cap 에 도달하면 더 push 안 함. 여기선 정상적으로 n-1 개 break + 종료.
        // widths 전부 20, avail=10 → 각 char overflow.
        let widths = [20.0, 20.0, 20.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        // line0 pos=0: acc=20>10 overflow new_pos=0 pen=1 j==0. next=1 `1<2` → break_idx=0,next=1.
        // line1 pos=1: acc=20>10 overflow new_pos=1 pen=1 j==0. next=2 `2<2`? no.
        //   penalties[1]=1 != -1000 → SkipEmit(next=2).
        // line2 pos=2: acc=20>10 overflow new_pos=2 pen=1 j==0. next=3 `3<2`? no.
        //   penalties[2]=1 != -1000 → SkipEmit(next=3). pos=3>=n → 종료.
        assert_eq!(out, vec![0]);
    }

    // ── Phase 1: first-line indent ──────────────────────────────────────────

    /// `to` index 에 para_property 를 가진 CR CharItemView 가 있는 composition.
    fn comp_with_para(para: ParaProperty, count: usize) -> MockComposition {
        let mut cr = CharItemView::new(0x0d);
        cr.para_property = Some(para);
        MockComposition {
            children: (0..count)
                .map(|_| Box::new(ParaItem { view: cr.clone() }) as Box<dyn Glyph>)
                .collect(),
        }
    }

    fn para_with_floats(k901: f32, k8ff: f32, k900: f32) -> ParaProperty {
        let mut bag = HashMapPropertyBag::new();
        bag.insert(PropertyKey::new(KEY_901), PropertyValue::Float(k901));
        bag.insert(PropertyKey::new(KEY_8FF), PropertyValue::Float(k8ff));
        bag.insert(PropertyKey::new(KEY_900), PropertyValue::Float(k900));
        ParaProperty {
            bullet: None,
            text_font: None,
            property_bag: bag,
        }
    }

    #[test]
    fn phase1_no_para_property_zero_indents() {
        // dummy_comp → get_para_item_view None → (0,0,0). avail = heights[0] = 10.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(
            &widths,
            &[],
            &[],
            &penalties,
            &heights,
            &dummy_comp(),
            0,
            0,
        );
        // avail=10: overflow new_pos=2 pen=1 j=2>0 → break_idx=1. line1: widths[2]=4 → break_idx=2.
        assert_eq!(out, vec![1, 2]);
    }

    #[test]
    fn phase1_not_first_line_indent_is_a_plus_b() {
        // from=0 이고 comp[0] = CR → is_first_line_on_para(comp, 0) = true... CR 이면 true.
        // not-first-line 을 만들려면 comp[0] 이 non-CR 이어야 함. 별도 구성.
        // 여기선 a=2, b=3, k900=0. from 위치가 CR 이 아니게 — comp[0]=non-CR, comp[1..]=CR.
        let para = para_with_floats(2.0, 3.0, 0.0);
        let mut non_cr = CharItemView::new(0x41); // 'A'
        non_cr.para_property = Some(para.clone());
        let mut cr = CharItemView::new(0x0d);
        cr.para_property = Some(para);
        let comp = MockComposition {
            children: vec![
                Box::new(ParaItem { view: non_cr }),
                Box::new(ParaItem { view: cr.clone() }),
                Box::new(ParaItem { view: cr }),
            ],
        };
        // get_para_item_view(comp, to=1) → idx 1 CR 발견 → para. is_first_line_on_para(comp, 0)
        //   → comp[0]='A' not CR → false → fVar28 = a + b = 5. fVar30 = b = 3.
        // widths=[4,4,4], heights=[20]. line0 indent=5: avail=20-5-0=15. acc 4,8,12 → pos+j==3
        //   → 한 줄 → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [20.0];
        let out = ppt_compose_break(&widths, &[], &[], &penalties, &heights, &comp, 0, 1);
        assert_eq!(out, vec![2]);
    }

    #[test]
    fn phase1_first_line_indent_affects_avail() {
        // from=-1 → is_first_line = true. bullet = None → c = 0.
        // a=3, b=5, k900=0. abs(a)=3 >= c=0, a>=0 → fVar28 = a+b = 8. fVar30 = b = 5.
        let para = para_with_floats(3.0, 5.0, 0.0);
        let comp = comp_with_para(para, 3);
        // widths=[4,4,4], heights=[14,14]. line0 indent=8: avail=14-8=6. j=0 acc=4 ok,
        //   j=1 acc=8>6 overflow new_pos=1 pen=1 j=1>0 → break_idx=0, next=1.
        // line1 pos=1 indent=fVar30=5: avail=14-5=9. j=0 acc=4 ok, j=1 acc=8 ok, pos+j==3
        //   → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [14.0, 14.0];
        let out = ppt_compose_break(&widths, &[], &[], &penalties, &heights, &comp, -1, 0);
        assert_eq!(out, vec![0, 2]);
    }

    #[test]
    fn phase1_margin_k900_subtracted_from_avail() {
        // a=0, b=0, k900=4 → fVar28 = a+b = 0 (abs(0)>=0, a>=0), fVar31 = 4.
        let para = para_with_floats(0.0, 0.0, 4.0);
        let comp = comp_with_para(para, 3);
        // widths=[4,4,4], heights=[10]. avail = 10 - 0 - 4 = 6. j=0 acc=4 ok, j=1 acc=8>6
        //   overflow new_pos=1 pen=1 j=1>0 → break_idx=0,next=1.
        // line1 pos=1: avail = heights[0] - fVar30(0) - fVar31(4) = 6. j=0 acc=4 ok,
        //   j=1 acc=8>6 overflow new_pos=2 pen=1 j=1>0 → break_idx=1,next=2.
        // line2 pos=2: widths[2]=4, pos+j==3 → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [10.0];
        let out = ppt_compose_break(&widths, &[], &[], &penalties, &heights, &comp, -1, 0);
        assert_eq!(out, vec![0, 1, 2]);
    }

    #[test]
    fn phase1_bullet_ascent_via_render_path() {
        // from=-1 → first line. bullet = Character (get_type()=1 != 0) → c 계산 경로.
        // char_view = get_first_char_item_view_on_para(comp, from+1=0) → comp[0] CR view.
        //   그 view 에 render_path + body_property 부착.
        // a=-2, b=1, k900=0. render_path default=10, special=20. body_property None
        //   → c = default_ascent = 10.
        // case: abs(a)=2 < c=10 → a<0 → fVar28 = a+b+c = -2+1+10 = 9.
        let para = ParaProperty {
            bullet: Some(Bullet::Character {
                chars: vec![0x2022],
            }),
            text_font: None,
            property_bag: {
                let mut bag = HashMapPropertyBag::new();
                bag.insert(PropertyKey::new(KEY_901), PropertyValue::Float(-2.0));
                bag.insert(PropertyKey::new(KEY_8FF), PropertyValue::Float(1.0));
                bag.insert(PropertyKey::new(KEY_900), PropertyValue::Float(0.0));
                bag
            },
        };
        let mut cr = CharItemView::new(0x0d);
        cr.para_property = Some(para);
        cr.render_path = Some(Box::new(FirstLineMetrics {
            default_ascent: 10.0,
            special_ascent: 20.0,
        }));
        // body_property None.
        let comp = MockComposition {
            children: vec![
                Box::new(ParaItem { view: cr.clone() }),
                Box::new(ParaItem { view: cr.clone() }),
                Box::new(ParaItem { view: cr }),
            ],
        };
        // fVar28 = 9, fVar30 = b = 1. widths=[4,4,4], heights=[15].
        //   line0 indent=9: avail=15-9=6. j=0 acc=4 ok, j=1 acc=8>6 overflow new_pos=1
        //   pen=1 (penalties=[1,1,1]) j=1>0 → break_idx=0,next=1.
        //   line1 pos=1 indent=fVar30=1: avail=15-1=14. acc 4,8 → pos+j==3 → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [15.0];
        let out = ppt_compose_break(&widths, &[], &[], &penalties, &heights, &comp, -1, 0);
        assert_eq!(out, vec![0, 2]);
    }

    #[test]
    fn phase1_bullet_ascent_special_slot_via_body_vert() {
        // body_property 의 GetVert() = 0 → pick_for_paragraph_class(0):
        //   `0 > 6` false, `(1<<0)&0x65` = 1 & 0x65 = 1 != 0 → special_ascent.
        // a=-2, b=1, k900=0. special=20 → c=20. abs(a)=2 < 20 → a<0 → fVar28 = -2+1+20 = 19.
        let para = ParaProperty {
            bullet: Some(Bullet::Character {
                chars: vec![0x2022],
            }),
            text_font: None,
            property_bag: {
                let mut bag = HashMapPropertyBag::new();
                bag.insert(PropertyKey::new(KEY_901), PropertyValue::Float(-2.0));
                bag.insert(PropertyKey::new(KEY_8FF), PropertyValue::Float(1.0));
                bag.insert(PropertyKey::new(KEY_900), PropertyValue::Float(0.0));
                bag
            },
        };
        let mut body = BodyProperty::new();
        body.property_bag.insert(
            PropertyKey::new(crate::text_property::KEY_VERT),
            PropertyValue::Uint(0),
        );
        let mut cr = CharItemView::new(0x0d);
        cr.para_property = Some(para);
        cr.render_path = Some(Box::new(FirstLineMetrics {
            default_ascent: 10.0,
            special_ascent: 20.0,
        }));
        cr.body_property = Some(body);
        let comp = MockComposition {
            children: vec![
                Box::new(ParaItem { view: cr.clone() }),
                Box::new(ParaItem { view: cr.clone() }),
                Box::new(ParaItem { view: cr }),
            ],
        };
        // fVar28 = 19, fVar30 = 1. widths=[4,4,4], heights=[25].
        //   line0 indent=19: avail=25-19=6 → j=0 acc=4 ok, j=1 acc=8>6 overflow new_pos=1
        //   pen=1 j=1>0 → break_idx=0,next=1. line1 indent=1: avail=24 → 한 줄 → break_idx=2.
        let widths = [4.0, 4.0, 4.0];
        let penalties = [1, 1, 1];
        let heights = [25.0];
        let out = ppt_compose_break(&widths, &[], &[], &penalties, &heights, &comp, -1, 0);
        assert_eq!(out, vec![0, 2]);
    }
}
