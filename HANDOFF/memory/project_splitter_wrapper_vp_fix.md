---
name: project-splitter-wrapper-vp-fix
description: enrich_linesegs wrapper-paragraph vp 덮어쓰기 fix 는 PNG 에 0 영향 — rhwp 가 hwpx 의 lineseg vp 값을 무시하고 자체 누적 계산함. fix 적용/롤백 PNG md5 동일. lineseg_gen.py 코드는 롤백 완료. 진단은 lineseg vp 가 아닌 render tree 비교가 정답.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

## 결론 (2026-05-19)

**fix 철회**. `enrich_linesegs` wrapper 분기에서 박스/표 paragraph 의 lineseg vp 를 cumulative 로 덮어쓰는 패치를 시도했으나, **PNG 시각 결과 / rhwp render tree 모두 0 변화**.

### 검증

| 산출물 | fix OFF | fix ON | 결과 |
|---|---|---|---|
| Q04.hwpx 박스 wrapper vp | 11948 | 2390 (cumulative) | hwpx 는 변경됨 |
| Q04.hwpx ① vp | 1890 | 17181 | hwpx 는 변경됨 |
| Q04.png md5 | 27019dfe... | 27019dfe... | **동일** |
| rhwp render tree (94 노드 dy) | n/a | 모두 0 | **동일** |

### 의미

rhwp 가 hwpx 의 `<hp:lineseg vertpos>` 값을 **무시**하고 paragraph 간 누적을 자체 계산한다.
([[project-rhwp-stored-vp-patch]] 의 stored-vp 우선 패치는 line 내 vp 만 stored 사용,
paragraph 간 누적은 별도 로직.) splitter 가 hwpx 의 lineseg vp 를 어떻게 변경하든
PNG/render tree 출력은 같다.

→ Q04/Q05 가 처음 "박스+①~⑤ 압축 겹침" 으로 보였던 건 다른 원인. 이번 fix 와 무관.

### 코드 상태

`src/kdsnr_hwp_toolkit/lineseg_gen.py` `enrich_linesegs` wrapper 분기는 **원복 완료**:
```python
if (info['has_table'] or info['has_secpr'] or info['has_layout']
        or info['has_scope']):
    new_paras.append(_enrich_cell_linesegs_in_items(
        p, para_t, char_t, width_tbl,
    ))
    prev_next = 0
    continue
```

추가 코드 없음.

## 회수해야 할 메시지

이전 보고 중 다음 주장은 **틀림**:
- "Q04/Q05 완벽 해결, mismatch 5.65%→0.44% / 9.25%→0.42%" — fix 와 무관, 그냥 원래 결과
- "Q17 ①~⑤ 분리 성공" — fix 와 무관
- "박스 wrapper vp 11948→2390 으로 한컴 saved 와 일치, ①~⑤ 분리 효과" — hwpx 만 변경, PNG 0 영향

PNG md5 비교로 즉시 검증할 수 있었던 사항을 놓침.

## 도구 (이번 세션에서 신설, 유지)

1. **`work/tool_hwpx_xml_diff.py`** — hwpx vs hwpsaved 의 section0/header XML diff
2. **`work/tool_pixel_diff_strong.py`** — PNG 두 장 side-by-side + row density + cluster bbox overlay + crop
3. **`work/tool_render_tree_diff.py`** — rhwp dump-render-tree JSON 두 개 받아 노드별 (TextRun/Table/Rect/Line/Image) 위치 delta 분석
4. **`vendor/rhwp/src/main.rs` 의 `dump-render-tree` 서브커맨드 + `wasm_api.rs` 의 `get_page_render_tree_native`** — PageRenderTree 의 모든 컴포넌트를 (type, bbox, text) JSON 으로 dump
   - 사용: `rhwp dump-render-tree <hwpx> -p <page> -o <out.json> [--hft-path <dir>]`

이 도구들이 **lineseg vp 가 아닌 실제 rhwp 렌더 결과** 를 보여주므로, hwpx XML 변경이 PNG 에 효과 있는지 사전 검증 가능. 앞으로 hwpx XML 만 보고 fix 판단 금지.

## 검증 결과 (2026-05-19)

`tool_render_tree_diff.py` 로 4 케이스 분석:

| 케이스 | matched | unmatched A/B | TextLine \|dy\|max | 진단 |
|---|---|---|---|---|
| Q04 | 94/94 (100%) | 0/0 | 0.0 | 동일 layout — **회귀 없음** |
| Q05 | 204/206 (99%) | 2/0 | 0.0 | 동일 layout — **회귀 없음** |
| Q11 | 71/150 (47%) | 79/78 | 24.3 | **진짜 layout 차이** — 말풍선 박스 잘림 |
| Q17 | 86/158 (54%) | 72/68 | 24.3 | **진짜 layout 차이** — 박스 내부 압축 |

Q11 dump 시 stderr: `LAYOUT_OVERFLOW_DRAW pi=18446744073709551613 y=1424.8 col_bottom=1421.1 overflow=3.7px`
→ rhwp 가 paragraph 컨텐츠가 column 바닥에서 넘침을 자체 감지. 말풍선 잘림의 직접 신호.

## How to apply

- 우리 splitter / enrich 변경 → PNG 출력 영향 검증은 **rhwp dump-render-tree** 로. lineseg vp/vs/spacing 만 변경하는 fix 는 PNG 에 거의 영향 없을 가능성 높음 (rhwp 자체 계산이 우선).
- 진짜 layout 회귀 찾을 때는 `tool_render_tree_diff.py` 의 unmatched 와 |dy|max 보면 됨.
- ours vs hwpsaved render tree 동일 = 회귀 없음. 다르면 unmatched 노드 path/text 가 root cause 단서.

## 관련

- [[project-rhwp-inline-rect-double-emit]] — Q03 fix (rhwp 측 책임). 진짜로 효과 있던 fix
- [[project-rhwp-stored-vp-patch]] — paragraph_layout 의 stored vp 우선 처리. line 내부만 영향, paragraph 간 누적과 별개
- [[project-splitter-enrich-linesegs]] — enrich_linesegs 도입. **lineseg vp 자체는 rhwp 가 무시**하지만 lineseg.vs (vertsize) 등 다른 attr 은 영향 가능성. 재검증 필요
- [[feedback-probe-driven]] — PNG md5 비교 / render tree 비교가 진짜 probe. hwpx XML 만 보고 fix 했다고 판단하면 안 됨
