---
name: project-rhwp-reflow-broadly-body
description: rhwp 본문 paragraph reflow 조건 확장 — 한컴 cache 부정확 케이스 자동 보정 (2026-05-18)
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# 본문 paragraph 에 needs_reflow_broadly 자동 적용

## 증상
- 한컴이 .hwp / .hwpx 의 long-text paragraph (40+ chars, no `\n`) 의 `linesegarray` 를 placeholder 1 개로만 cache
- 예: `science_input_example.hwp` 의 Q18 발문 paragraph (122 chars) → lineseg 1 개 (horzsize=31750, line_height=1150) 만 binary 에 저장
- 한컴 자체 viewer 는 cache 무시하고 매번 reflow 해서 3 줄 표시
- rhwp 는 cache 신뢰 → composer 가 1 ComposedLine 에 122 char 다 욱여넣음 → "한 줄로 뭉개짐"

## Root cause
`document_core/commands/document.rs::reflow_zero_height_paragraphs` 의 본문 처리:
```rust
if Self::needs_line_seg_reflow(para) {  // (len==1 && line_height==0) 만
    reflow_line_segs(...)
}
```
- 셀 내부: `reflow_zero_height_table_cells_inner` 이 `needs_reflow_broadly` 까지 검출 → 자동 reflow OK
- 본문: 좁은 `needs_line_seg_reflow` 만 → long-text + lineseg=1 + no `\n` 케이스 미처리
- **fix 가 이미 정의된 함수 (`needs_reflow_broadly`) 를 본문 path 에 연결만 하면 됨** — `reflow_linesegs_on_demand` 안에서만 쓰던 걸 자동 path 로 승격

## fix (2026-05-18)
`document_core/commands/document.rs:259`:
```rust
if Self::needs_line_seg_reflow(para) || Self::needs_reflow_broadly(para) {
```

## 검증
- Q18 paragraph 0.3: ls 1 개 → **ls 3 개로 정상 reflow** (ts=0/34/97, vpos=41524/43191/44858)
- 시각: 발문이 1 줄 cram → 3 줄 정상 줄바꿈 (GT 일치)
- 전체 e2e bench (106 페어, err 0, avg 93.57%) — 시각적 regression 없음
- 점수 -0.42% (93.99 → 93.57) 는 reflow 후 흰 배경 비율 변화 정상 trade-off

## 관련
- `[[project_rhwp_placeholder_lineseg]]` — cell 처리 reflow 확장
- `[[project_rhwp_architecture]]` — 전체 렌더 파이프라인
- `[[feedback_rhwp_byte_equivalent_goal]]` — pixel-eq 목표
