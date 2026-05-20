---
name: project-e2e-first-measurement
description: "2026-05-18 e2e bench 첫 측정 — rhwp baseline 94% 평균, 다음 결정 (수식 vs SvgSurface wire)의 근거"
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# e2e bench 첫 실측 결과 (2026-05-18)

**Why**: Stage 5 pixel-diff harness 완성 직후, 한컴 GT vs 우리 파이프라인(현재는 rhwp baseline)의 첫 측정. 다음 작업 (수식 byte-eq port vs SvgSurface wire) 결정의 근거 데이터.

**How to apply**: 다음 byte-eq port 작업 ROI 판단에 사용. 본 baseline 을 넘어야 의미 있는 진전.

## 실행 환경

- bench binary: `kdsnr-hwp-toolkit/render-engine/rust/examples/e2e_bench.rs` (단일)
  + `e2e_bench_all.rs` (batch)
- 파이프라인: hwpx → `rhwp::HwpDocument::render_page_svg_native(page)` → SVG string → `usvg::Tree::from_str` → `resvg::render` → tiny-skia Pixmap (target = GT 크기) → `kdsnr_render::pixel_diff_harness::score_pages` (default `DiffOptions`: aa_threshold=2)
- GT: `work/e2e/<subj>__<sample>/<stem>.png` (한컴 PDF→PNG 사전 산출, 1399px 폭 다수)
- 페이지 비교: page 0 only (multi-page hwpx 라도 첫 페이지만)
- HFT cache: `hft-decoder/rust/test-data/` (HGMJ/ENSMJ/HCHGGGT/HJSMJ 4 families)
- 산출물 디렉토리: `work/e2e/_bench_output/` (per pair `<key>_ours.png` + `<key>_heatmap.png`, 총 217 파일)

## 점수 (106 페어, page 0)

- **전체 평균: 93.99%** (CSV escape 누락 4개 제외한 102 페어 기준 94.13%)
- 최고: `math__math_input_sample/Q06 — 99.21%`
- 최저: `social__social_test_input_2/Q08 — 85.91%`

| 과목 | n | 평균 | 최저 | 최고 | stdev |
|------|---|------|------|------|-------|
| math | 60 | 95.71% | 92.33% | 99.21% | 1.56 |
| science | 12 | 96.03% | 93.30% | 97.53% | 1.23 |
| social | 30 | 90.19% | 85.91% | 98.13% | 2.37 |

점수 구간 분포:
```
[85, 90) : 14 ##############
[90, 93) : 18 ##################
[93, 95) : 18 ##################
[95, 97) : 36 ####################################
[97, 99) : 15 ###############
[99,100) :  1 #
```

## mismatch 의 nature (해석)

- **`avg_delta ≈ 150-170`** 전 페이지 공통. anti-alias 만이면 30 이하여야 함 → 픽셀이 "완전히 다른 색" mismatch.
- 추정: 글리프 1-2 px 위치 어긋남 + 글리프 모양 차이 + 박스/도형 stroke 폭 차이가 누적.
- science 가 균일하고 높음 → 표/단순 layout 은 rhwp 가 거의 한컴.
- social 이 90% 평균 → 박스/도표/특수 글꼴 영역 baseline 자체가 낮음.
- math 95.7% baseline 은 의외 — **rhwp 수식 출력이 [[reference-rhwp-svg-renderer]] 메모와 달리 실측상 의외로 양호**. [[project-glyph-draw-state]] 의 수식 byte-eq port (28-32 세션) ROI 가 낮아질 가능성.

## 다음 결정점

1. **option A** — SvgSurface adapter e2e wire (L-5 chain → Document layer): rhwp 의 svg.rs 대신 우리 byte-eq port + SvgSurface 가 SVG 를 emit 하도록 wire. 본 baseline 을 넘어 95-97% 달성하면 우리 work 가 의미 있음. (예상 작업: kdsnr-layout → kdsnr-render wire + 페이지 driver, 미정 N 세션)
2. **option C** — 수식 byte-eq port (28-32 세션): math 5% gap 정복. 본 측정에선 ROI 가 가장 낮아 보임. **본 heatmap 분석으로 더더욱 낮아짐 (math glyph_in 4.8%, max cluster 399px). 연기 강력 권장**.

## 후속 heatmap 분석 (2026-05-18 동일 세션)

**스크립트**: `work/e2e/_bench_output/analyze_heatmaps.py` (per-row density + short/long run length + glyph_in mask) + `analyze_clusters.py` (connected component bbox top-N).

### 카테고리 비율 (108 페어 평균, mismatch 픽셀 내 비율)

| 과목 | n | glyph_in | glyph_out | short_run | medium | long_run | peak_y_band |
|------|---|---------:|----------:|----------:|-------:|---------:|------------:|
| math | 60 | 4.8% | 95.2% | 5.1% | 42.0% | 52.9% | 30.0% |
| science | 12 | 22.1% | 77.9% | 12.6% | 22.6% | 64.8% | 41.0% |
| social | 30 | 11.1% | 88.9% | 7.0% | 31.0% | 62.0% | 24.4% |
| korean | 4 | 12.2% | 87.8% | 9.2% | 34.6% | 56.1% | 19.6% |
| **전체** | 108 | **8.8%** | 91.2% | **6.6%** | 36.4% | **57.0%** | 28.0% |

- `glyph_in` = mismatch ∩ (ours luminance < 128 픽셀) = 글리프 영역 안 mismatch
- `short_run` ≤ 2px, `long_run` ≥ 5px (수평 연속 mismatch)

### Cluster bbox 분석 (worst/best/korean 1개씩)

- **social Q08** (worst 85.91%): top cluster (96,72)-(1304,756), **1209×685 = 64687px → mismatch 27.9% 단독**. 거대 이미지/도표 misalignment. top 12 cluster 가 mismatch 45% 차지.
- **math Q06** (best 99.21%): max cluster 399px, 모두 area<1000. 글리프 단위 미세 차이만. 큰 misalignment 없음.
- **korean S01-03**: max cluster 6503px (131×218 박스 추정), area≥1000 cluster 20개. 박스/표 misalignment 많음.

### 핵심 함의

1. **mismatch 91%는 글리프 영역 밖** = ours 글자 위가 아니라 ours 가 비어있는 곳에서 mismatch. 즉 글꼴 자체 vs 한컴 차이 아니고, **layout 위치 어긋남으로 양쪽 모두 mismatch 처리** 되는 패턴.
2. **mismatch 57%는 long run (≥5px)** = 채워진 영역/박스/도형 misalignment dominant.
3. **single biggest ROI = 이미지/도표 영역 정확도** — social Q08 의 64K px 단일 cluster 처럼 큰 영역 1개가 page mismatch 의 1/4 차지 가능.
4. **두 번째 ROI = line baseline 정확도** — 폭 1000+ 높이 40 가로띠 cluster 들 (text line vertical position 1-2px 어긋남).
5. **수식 byte-eq port ROI ≪ 추정치** — math glyph_in 4.8%, max cluster 399px, area≥1000 cluster 0개.
6. **science glyph_in 22.1%** outlier — 과학 페이지 한컴 폰트 (HJSMJ 등) 와 rhwp ttfs/system font 차이일 가능성.

### 권장 다음 action (데이터 기반 우선순위)

1. **이미지/도형 (rhwp picture/binData) rendering 정확도** ← single biggest ROI
2. **page layout vertical position** ← 두 번째 ROI
3. (deferred) 수식 byte-eq port ← 측정된 ROI ≪ 4.8%
4. (deferred) 글꼴 자체 byte-eq ← 6.6% short run 만

옵션 A (SvgSurface adapter wire) 가 의미 있으려면 1번/2번을 해결할 byte-eq port 가 그 안에 있어야 함. 즉 다음 byte-eq port 후보는 **PictureGlyph / TableGlyph / paginator vertical position**.

## 산출물 위치

- `work/e2e/_bench_output/bench_log.txt` — 106 페어 CSV + 집계
- `work/e2e/_bench_output/mismatch_categories.csv` — 108 페어 카테고리 비율
- `work/e2e/_bench_output/mismatch_summary.json` — 과목별 + 전체 평균
- `work/e2e/_bench_output/<key>_ours.png` + `_heatmap.png` × 106
- `work/e2e/_bench_output/<key>_clusters_top15.png` × 3 (worst/best/korean)

## 한계 / 추후 보정

- page 0 only — multi-page hwpx 의 후속 페이지 미측정. e2e_bench_all 에 `--all-pages` 옵션 추가 필요.
- korean 4개 페어 (콤마 폴더명) CSV 파싱 누락 — 실행/PNG 생성은 성공, log 의 quoted 행 미파싱.
- aa_threshold=2 가 너무 관대할 수 있음. 0 으로 재측정 시 더 낮은 score 가 정확한 byte-eq 거리.
- `LAYOUT_OVERFLOW_DRAW` 경고 출력됨 (science 12/12, social Q07). rhwp 페이지넷션 미세 버그 — 본 score 에 영향 미미.
