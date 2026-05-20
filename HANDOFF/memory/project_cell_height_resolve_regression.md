---
name: project-cell-height-resolve-regression
description: 2026-05-19 cell_height.resolve 회귀 — splitter enrich_doc 가 placeholder cellSz (예 282) 를 content_bottom+margin (예 1586) 으로 부풀리면 rhwp 가 (가)(나)(다) 박스 마지막 행 잘림. enrich_doc 에서 cell_height.resolve 호출 제거. linesegs.fill_missing + inline_correction 만 유지.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

## 증상
- 사이언스 Q14: (가)(나)(다) 3 박스 각각 마지막 행 (AAG / CCU / 검은색) 잘림.
- 원인: NEW splitter (enrich_doc 포함) 가 만든 hwpx 의 cellSz height 가 OLD 282 → NEW 1586 으로 부풀려짐.
- OLD hwpx + NEW rhwp = 정상 렌더 → rhwp 책임 아님. splitter 출력의 차이.
- 부풀린 cellSz 와 vertAlign=CENTER 조합에서 rhwp 가 행 높이/내용을 제대로 못 맞춤 (정확한 rhwp 코드 위치는 미식별, 우회로 충분).

## 패치 — `src/kdsnr_hwp_toolkit/layout/enrich.py`

```python
def enrich_doc(doc):
    doc = inline_correction.apply(doc)
    doc = linesegs.fill_missing(doc)
    doc = inline_correction.apply(doc)
    return doc
```
(cell_height.resolve 호출 제거).

## Why
- 한컴 원본 placeholder cellSz (예 282, 단순한 lh+ls 보다 작음) 는 rhwp 런타임 reflow 가 content 기반으로 정확히 재계산.
- cell_height.resolve 가 `required_h = max(old_h, content_bottom + top_m + bottom_m)` 식으로 cellSz 를 키우면 rhwp 의 reflow path 와 충돌.
- Q28 거대공백 fix 의 핵심은 linesegs.fill_missing (paragraph linesegs_xml 채움) 이지 cell_height 가 아님 → 제거해도 Q28 효과 유지.

## How to apply
- splitter 후 cell.height 손대지 말 것. paragraph linesegs 만 채우면 rhwp 가 알아서 reflow.
- 신규 회귀 의심 시 우선 `OLD hwpx (이전 work/e2e/_excluded_no_gt_pdf 등) + NEW rhwp` probe 로 rhwp 책임/splitter 책임 양분.

## 검증 (2026-05-19)
- 4 과목 × 5 문항 (korean 2 + math/science/social 각 5) 재렌더 모두 OK.
- 과학 Q14 (가)(나)(다) 3 박스 4 행 정상 표시.
- 수학 Q28_2 발문 직후 수식 정상 (huge gap 없음, 이전 enrich_linesegs 효과 유지).
- 출력: `kdsnr-hwp-toolkit/work/_render_4subj_5q/`.

## 관련 메모리
- [[project-splitter-enrich-linesegs]] — 원래 enrich_linesegs (lineseg 채움) motivation
- [[feedback-rhwp-byte-equivalent-goal]] — pixel-eq 목표
