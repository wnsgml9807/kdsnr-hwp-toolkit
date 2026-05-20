---
name: rhwp HWPX placeholder lineseg 정공법
description: 한컴 placeholder lineseg 의 self-encoded segment_width 를 parse 시점 자동 보정에서 활용하는 정공법. 우회 4 곳 제거 완료
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
한컴 HWPX 는 본문/셀 단락 일부를 **placeholder lineseg 1 개**로 저장하고 `horzsize` 속성에 wrap 폭 (본문 = 단 폭, 셀 = cell_inner_width) 을 self-encode 한다. 예: Q08 의 셀 단락 `<hp:lineseg textpos="0" vertsize="1150" horzsize="27499" .../>` 의 27499 HU = cell.w(29199) − padding(850×2). 한컴 자체도 렌더 시점에 매번 다시 wrap 한다.

## rhwp 의 함정

`DocumentCore::reflow_zero_height_paragraphs` (parse 직후 자동 보정) 는 검출 조건 `needs_line_seg_reflow` 가 `line_height==0` 만 보기 때문에 placeholder (line_height>0) 를 통과시킨다. 결과: 셀 단락 + 일부 본문이 한 줄짜리 IR 로 남아 글자 겹침 / 셀 경계 침범 / justify 가 단어 간격을 비정상적으로 늘림.

## 정공법 (2026-05-12)

`reflow_zero_height_paragraphs` 의 검출/폭 확장:
- 검출: `needs_line_seg_reflow || needs_reflow_broadly` (placeholder 패턴 포함)
- 본문 단락 폭: col_width − margins (기존)
- 셀 단락 폭: `cp.line_segs[0].segment_width` (한컴 self-encoded). 새 helper `reflow_zero_height_table_cells` 가 재귀 처리

이후 render-time 우회를 모두 제거:
- `table_layout.rs:1218-1222` 매-셀 clone+reflow → IR 직접 compose
- `table_layout.rs:698` (`calc_cell_paragraphs_content_height_with_width`) 내부 reflow 제거
- `height_measurer.rs:543, 734` 두 경로 clone+reflow 제거
- `preview.py` 의 `--reflow` 플래그 제거

**Why:** 우회는 px 단위 cell_inner_width 를 HU→px round-trip 으로 계산해 미세 손실 + padding 해석 차이로 wrap point 가 어긋났다. IR 의 self-encoded segment_width 를 HU 단위로 그대로 쓰는 게 한컴 원본과 정확히 일치. 회귀 검증: 102 unit 중 49 변경, 모두 한컴 의도 (정상 wrap, 정상 justify 간격) 에 더 부합.

**How to apply:** rhwp 의 셀/본문 paragraph 렌더 관련 작업 시 (1) parse 시점 IR 신뢰. (2) 매-호출 reflow 우회 부활시키지 말 것 — px 변환 round-trip 손실 + idempotent 가정 깨짐. (3) placeholder 검출은 `needs_reflow_broadly` 가 권위. line_height==0 만 보는 좁은 검출은 함정. (4) 셀 폭 동적 변경 시나리오에서는 IR 무효화 책임이 COM API/edit path 에 있음 (현 toolkit use case 에선 발생 안 함).
