---
name: project-rhwp-stored-vp-patch
description: rhwp paragraph_layout/height_measurer 가 모든 paragraph 에 stored lineseg.vp 우선 path 적용. paragraph 안 line 위치 한컴 spec 일치. +128.6 GT diff 는 별개 (pagination 표 누적).
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

2026-05-19 deep dive 결과. rhwp 가 한컴 stored LineSeg.vertical_pos 를 무시하고 line_height + line_spacing 누적 → 한컴 GT 와 paragraph 안 line 위치 미세 어긋남.

## 패치

paragraph_layout.rs line 862-866:
```rust
let stored_vp_base = composed.lines.get(start_line).map(|l| l.vertical_pos).unwrap_or(0);
let last_line_idx_for_vp = end.saturating_sub(1).max(start_line);
let stored_vp_last = composed.lines.get(last_line_idx_for_vp)
    .map(|l| l.vertical_pos).unwrap_or(0);
let use_stored_vp = stored_vp_base != 0 || stored_vp_last != 0;
```
- 모든 paragraph 가 stored vp 사용 (이전: has_inline_tac_table 만)
- text_y = y_para_origin + (vp - stored_vp_base) [line 1048]
- y 누적 = y_para_origin + (last.vp - vp_base + last.lh) + last.ls [line 2823]

height_measurer.rs line 303-323:
```rust
let stored_h = if !para.line_segs.is_empty() {
    let first = &para.line_segs[0];
    let last = &para.line_segs[para.line_segs.len() - 1];
    if first.vertical_pos != 0 || last.vertical_pos != 0 {
        Some(hwpunit_to_px(last.vertical_pos + last.line_height + last.line_spacing - first.vertical_pos, self.dpi))
    } else { None }
} else { None };
stored_h.unwrap_or(sum)
```

## 결과 측정

- stored_h == sum (lineseg 자기일치: lh+ls == next.vp - prev.vp)
- paragraph 안 line 위치만 stored vp 따라 정확 (paragraph 간 누적은 그대로)
- pagination used_height 변화 없음 (height_measurer 결과 동일)

## +128.6 px 진짜 root cause (별도 작업)

dump-pages 의 used vs hwp_used 차이는 pagination engine 의 wrap=TopAndBottom 비-TAC 표 처리에서 paragraph_height + table_total_height 둘 다 누적 → 표 1 개당 ~70 px 중복.

위치: engine.rs:1422, 1469 (`st.current_height += table_total_height + caption_extra_for_current`).

한컴 stored vp gap 은 paragraph + table 합쳐진 단일 값. rhwp 누적 모델은 둘 별도 합산.

**Why:** rhwp 자체 누적 모델이 한컴 spec 과 다름. paragraph_layout 변경 후에도 pagination engine 은 동일 누적 식 사용.

**How to apply:** pagination engine 의 wrap=TopAndBottom 표 처리에서 stored vp gap 으로 누적 변경 시 시각 정확성 개선. 단 다른 케이스 회귀 위험 (caption, multi-table 등). 별도 세션 정공법 진행 필요. 관련 [[project-splitter-enrich-linesegs]].

## 2026-05-19 추가: main pagination path 식별

⚠️ **engine.rs (Paginator::paginate_with_measured) 는 fallback path** (env `RHWP_USE_PAGINATOR=1` 일 때만).
**main path = `typeset.rs::TypesetEngine`** (document_core/queries/rendering.rs:897 분기).

dump-pages 의 used_height 는 TypesetState.current_height 누적. typeset_paragraph + typeset_table_paragraph (→ typeset_block_table) 가 핵심 path.

wrap=TopAndBottom 비-TAC 표 처리는 typeset_block_table (typeset.rs:1545). 누적식:
- line 1567-1568: `host_spacing_total = ft.host_spacing.before + ft.host_spacing.after`, `table_total = ft.effective_height + host_spacing_total`
- line 1598-1599: fits 시 `place_table_with_text(..., table_total)` — paragraph text + 표 누적
- line 1614: 강제 배치 `st.current_height += ft.effective_height`

**다음 세션 작업**: typeset_block_table 의 wrap=TopAndBottom 비-TAC + vert_offset=0 에 대해 paragraph_height + table_total 합산이 stored gap 과 일치하도록 보정.

engine.rs 의 paragraph loop override (engine.rs:85-99) + table 누적 skip (engine.rs:1422 부근) 은 그대로 둠 (fallback path 에 효과). 단 main path 아니므로 visual 영향 없음.
