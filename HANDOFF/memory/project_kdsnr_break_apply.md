---
name: project-kdsnr-break-apply
description: G-W-3 KDSNR_BREAK apply gate (RHWP_USE_KDSNR_BREAK_APPLY) — byte-eq line break 결과를 line_breaks 변수에 실제 적용. 2026-05-19 케이스별 효과 측정.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# G-W-3 KDSNR_BREAK apply 효과 측정 (2026-05-19)

`ppt_compose_break` (한컴 byte-eq line break) 결과를 rhwp `LineBreakResult` 로 변환 → `fill_lines` 대신 사용. env gate = `RHWP_USE_KDSNR_BREAK_APPLY=1` (opt-in, default off).

구현: `vendor/rhwp/src/renderer/composer/line_breaking.rs` 의 `compute_kdsnr_breaks` 헬퍼 + `reflow_line_segs` 안 apply gate 분기.

## Hancom-saved (`_rt_saved_*.json`) 대비 매칭 (render tree diff)

`work/_render_4subj_5q/_hwpsaved/_rt_saved_*.json` = Hancom 이 .hwpx 를 재저장한 뒤 rhwp 로 render tree dump → GT-proxy.

| 케이스 | baseline | brkOFF (kdsnr-W only) | **brkapply** |
|---|---|---|---|
| science Q11 | matched **71**, dy 24.3 | 73, dy 19.4 | **140**, dy 12.3 ⭐ |
| science Q17 | 86 | - | **112** ✅ |
| social Q05 | **204**, dy 0.0 (이미 완벽) | 120, dy 28.0 ❌ | 114 ❌ |

**Q11/Q17**: stored line_segs 가 Hancom 과 어긋났던 케이스. byte-eq apply 가 도움.
**Q05**: stored line_segs 가 이미 Hancom 완벽 매치. reflow 가 망침 (apply 이전 단계인 kdsnr-W 자체에서 회귀).

## Why: token vs char granularity

현재 `compute_kdsnr_breaks` 는 token-level (rhwp `BreakToken` = word/space 단위) 로 ppt_compose_break 에 전달. Korean 의 경우 한 token 안에 여러 글자가 들어가서 ppt_compose_break 가 intra-token break 를 못 만듦. **per-char tokenization + Hancom char_class penalty 필요** for full byte-eq.

penalty mapping (현재):
- Text=2 (intra-token break 금지)
- Space=1 (break OK)
- Tab=1 (break OK)
- LineBreak=0 (force break)

PENALTY_BREAK_MASK bits {1, 10, 50} 만 break 허용. 한컴은 char class 별 penalty 가 따로 있을 가능성 — backward RE 필요.

## Pixel diff 의 함정

Q17 직접 pixel diff (GT PDF rasterize vs ours):
- baseline 2.56% mismatch
- kdsnr-W 2.91% (worse)
- brkapply 2.90% (worse)

**그러나** Hancom-saved RT 대비는 brkapply 가 압도적 우위. 이는 pixel diff 가 HFT glyph 모양 차이까지 포함하기 때문. byte-eq 진단은 render tree diff 가 정답. [[feedback-eval-harness-last]] 원칙대로 점수로 steering 금지.

## How to apply

기본 OFF. dev 측정 시:
```
RHWP_USE_KDSNR_LAYOUT=1 RHWP_USE_KDSNR_BREAK_APPLY=1 ./rhwp dump-render-tree ... 
```

Wire 사이트 (compose_paragraph_with_reflow):
- `document.rs:70` (section traversal)
- `table_layout.rs:675` (height measure)
- `table_layout.rs:1190` (actual cell layout)
- `table_layout.rs:2051` (remaining height)
- `table_cell_content.rs:556`
- `shape_layout.rs:1213/1301/1667`

## per-char 시도 결과 (2026-05-19, same session)

**Korean per-char expand** (char_widths 누락 시 token width / n 균등): Q11 회귀 (140→71). 균등 분할은 byte-eq 안 됨 — Korean char width 가 각자 다른데 누적 sum 이 drift.

**Latin per-char expand** (char_widths 채워짐): Q17 회귀 (112→84). intra-word break (e.g. "wor|ld") 발생.

**결론** (현재 코드 상태): Korean 어절 모드 default 는 char_widths 비어 있어 자동으로 token-level 처리. Latin 도 강제 token-level. 미래에 Korean 글자 모드 (korean_break_unit=1) 활성화될 때만 per-char 적용.

진짜 per-char byte-eq: 정확한 per-char width 계산 (estimate_text_width_unrounded char 단위) + Hancom char_class penalty 테이블 RE 필요. 별도 세션.

## Q11 잔여 dy=12.3 원인 (다음 step)

"11." 문항번호 paragraph 의 Rect 와 TextRun 의 vertical alignment 차이:
- 우리: Rect (70,140), TextRun (70,140) — text at top of rect
- Hancom: Rect (70,128), TextRun (85,153) — Rect 가 12px 위, text 가 Rect top 에서 25px 아래

paragraph border_spacing 또는 first-line baseline 결정 로직 byte-eq 추적 필요.

## reflow_line_segs preserve_dims fix (2026-05-19)

reflow_line_segs 의 `make_line_seg` 가 stored 의 line_height/text_height/baseline_distance/line_spacing 를 무조건 재계산. 한컴이 이미 계산한 값을 가지고 있는 경우 (paragraph 가 hwpsaved 출발) 우리 재계산이 약간 어긋나서 y 좌표 +12px 회귀 (lineSpacing 140% interpretation 차이).

**Fix**: stored 가 valid (line_height>0 AND text_height>0 AND baseline_distance>0) 면 stored 4개 dimension 모두 preserve. segment_width 만 재계산. line break 위치는 여전히 ppt_compose_break (env=APPLY) / fill_lines 결과 사용.

### 효과

| 케이스 | baseline | brkapply 회복 전 | **brkapply preserve_dims** |
|---|---|---|---|
| science Q11 | 71 | 140 | **140** (유지) |
| science Q17 | 86 | 112 | **112** (유지) |
| social Q05 | **204** | 114 (-90 회귀) | **201** (-3, 거의 회복) |

Q11 "11." position:
- BL: (70,128,20x17)
- KD before fix: (70,140,23x24) — y +12, h +7 회귀
- **KD after fix: (70,128,23x17)** — y/h 정확 매치 (w 3px 차이는 kdsnr_w 폭 byte-eq 잔영)
- HC: (70,128,20x17)

## 다음 step

1. paragraph spacing (before/after) byte-eq
2. 진짜 per-char compose_break (정확한 widths + Hancom char_class) — Q11 잔여 mismatch
3. glyph rendering byte-eq (HFT advance)

관련: [[project-g-phase-d-plan]], [[project-wire-probe-state]], [[feedback-rhwp-byte-equivalent-goal]].
