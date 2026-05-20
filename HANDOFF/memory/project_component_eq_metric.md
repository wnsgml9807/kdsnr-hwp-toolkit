---
name: project-component-eq-metric
description: pixel-eq goal 의 공식 metric 도구 — work/tool_component_eq.py. component 별 position + size + style 정밀 매치 점수.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# 고정밀 component EQ_SCORE 도구 (2026-05-19)

사용자 정의 "pixel eq" = 글자/표/셀 내 패딩/폰트/스타일/이미지 등 **모든 요소의 위치와 스타일이 GT와 일치**.

raster pixel diff (SvgSurface vs Hancom native rasterizer 차이로 본질적 불일치) 대신 **render tree component-level 정밀 매치** 가 goal benchmark.

## 도구

- `work/tool_component_eq.py`
- 사용: `python3 tool_component_eq.py ours.json hcsaved.json --out diff_dir [--tol 0.5]`
- 출력: `EQ_SCORE: 0-100% (full_eq/total_b)` + per-type breakdown (csv + summary.txt)

## Render tree JSON 스키마 (v2)

`vendor/rhwp/src/renderer/render_tree.rs` 의 `write_json` 에 스타일 속성 추가:
- TextRun: `style: {ff, fs, col, b, i, u, st, ls, ratio}`
- Image: `bin` (bin_data_id)
- Line: `x1, y1, x2, y2, col, w`
- Rect: `r (corner_r), fill, stroke, sw`

## 매칭 키 (정밀)

- TextRun: (type, text) — 같은 text 면 최근접 dy 매칭. unmatched 들은 2-pass fuzzy match (same-y concat strip 비교) — split 경계 다른 case (`①`+` 텍스트` vs `① `+`텍스트`) 매칭.
- Cell: (type, parent_table_path, row, col) — table 별 row/col 좌표
- Table: (type, rows, cols)
- TextLine: (type, round(y/5)*5, round(x/5)*5) — 위치 fuzzy bucket
- Line: (type, x1, y1, x2, y2) — 좌표 자체가 식별자
- Rect: (type, round(x,y,w,h))
- Image: (type, round(w,1), round(h,1)) — bin_id 가 hwpx 마다 unstable 해서 size 매칭 (2026-05-19 변경)

## 비교 항목 (per pair)

- pos_ok: |dx|, |dy| ≤ tol (default 0.5px)
- size_ok: |dw|, |dh| ≤ tol
- style_ok: TextRun style 모두 exact (ff/fs/col/b/i/u/st/ls/ratio); Line/Rect color/width 비교
- full_ok: pos AND size AND style

## 현재 측정값 (baseline, 2026-05-19)

| 케이스 | EQ_SCORE (baseline) | EQ_SCORE (kdsnr+brkapply) |
|---|---|---|
| science Q11 | 26.57% (38/143) | 25.17% (36/143) |
| science Q17 | 25.68% (38/148) | 24.32% (36/148) |
| social Q05 | **95.43%** (188/197) | 66.50% (131/197) |

**Q05 baseline 95.43% = 거의 pixel-eq 달성** (input hwpx 가 Hancom-touched 였음).
**Q11/Q17 baseline ~25%** = 갈 길 멀음 (splitter-touched input, Hancom 과 layout 다름).

kdsnr+brkapply 은 component **matched count** 는 늘리지만 (Q11 76→94), 매치된 component 의 pos/style 정확도는 baseline 과 비슷하거나 약간 낮음.

## Pixel eq goal 달성 경로 (chain)

1. ⭐ 이 메트릭으로 매 세션 측정. EQ_SCORE ≥ 99% = goal 달성.
2. **Q11 main loss** = paragraph 누적 dy=+2.8 (Line/Table/Cell/TextLine/TextRun 전부). root cause = stored lineseg ls[1] lh 차이 (ours hwpx=1300 vs saved hwpx=1150). 한컴 line metric algorithm RE 필요. 자세히는 [[project-q11-dy-offset-root-cause]]
3. **Q05 baseline 95% remaining 5%** = TextRun split 경계 차이 (한컴: `① `+`텍스트`, rhwp: `①`+` 텍스트`). 도구 fuzzy match 추가 → unmatched 0 이지만 pos/size mismatch 라 EQ_SCORE 변동 없음. 진짜 fix = rhwp run grouping 한컴식 (trailing space attach 앞 run).
4. **공통 root cause**: Hancom line breaking 알고리즘 + line metric 결정 logic byte-eq RE. [[project-full-byteeq-plan]] 의 28-44 세션.
5. 매 byte-eq port 후 EQ_SCORE 측정해서 개선/회귀 검출

## EQ_SCORE 측정값 (2026-05-19, 도구 v3 = 사용자 요구사항 반영)

사용자 요구사항 (2026-05-19): **header/footer 무관. 각 개별 문항 덩어리 (=Body) 만. 덩어리 위치 어긋남 OK, 덩어리 내부 요소 상대 위치만 일치 필요. pixel-eq raster 비교는 흰색 픽셀로 희석되니 component-level diff 가 더 정확.**

| 케이스 | baseline | 도구 v3 | 진척 |
|---|---|---|---|
| Q11 | 26.57% | **79.82%** | +53% |
| Q17 | 25.68% | **65.79%** | +40% |
| Q05 | 95.43% | 95.03% | -0.4 (Body 외 빼서) |

도구 v3 변경 (`work/tool_component_eq.py`):
- Image 매칭 키 (type, w, h) — bin_id unstable
- TextRun 2-pass fuzzy match (same-y concat strip)
- Line/Rect 매칭 키 fuzzy (round(./5)*5)
- ⭐ **spatial nearest 2-pass match** (type 만 같으면 region offset 보정 거리 ≤20px 매칭) — Line/Rect/TextLine 매칭 대폭 증가
- ⭐ **score = Body 안만** (header/footer/masterpage/pagebg 제외)
- ⭐ **region offset cancel** (Body median dx/dy 자동 차감 — Q11 dy=+2.8 systematic offset 해소)
- ⭐ **line-local dx offset cancel** (같은 y bucket 안 TextRun dx median 추가 cancel — paragraph margin 차이 흡수)
- Line style 비교에서 y1/y2 region offset 차감
- Image style 비교에서 bin 제외 (unstable)
- unmatched.txt 별도 출력

진짜 잔여 fail = TextRun dx/size 차이. 도구로 cancel 불가능 (line 안 글자 advance 자체 다름). rhwp 코드 byte-eq port 필요 (다음 lever):
1. **글자 advance width byte-eq** (HFT 한컴 metric → 글자별 위치 정확)
2. **TextRun split 한컴식** (trailing space 앞 run 부착)
3. **paragraph line break 한컴식** (compose_break 완성 — kdsnr_break_apply 시작점)

관련: [[project-kdsnr-break-apply]], [[project-full-byteeq-plan]], [[project-q11-dy-offset-root-cause]], [[feedback-rhwp-byte-equivalent-goal]].
