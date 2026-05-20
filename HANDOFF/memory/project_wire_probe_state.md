---
name: project-wire-probe-state
description: 2026-05-18 wire 첫 시도 — rhwp PageRenderTree → 우리 SvgSurface adapter. 5 페어 모두 rhwp baseline 위 (+0.1~0.3%p). 한컴 HFT 380+ binary embed (feature=embedded). 다음 = coordinate scale + Latin TTF fallback + text positioning.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# Phase B — wire 첫 시도 상태 (2026-05-18)

**Why**: e2e 첫 측정 (rhwp baseline 평균 94%) 후 mismatch 의 91% 가 글리프 영역 밖 + 57% 가 long run = rhwp 의 layout/이미지 misalignment 문제. 다음 작업의 본질 = "우리 byte-eq port + SvgSurface adapter 가 실제 hwpx 페이지를 렌더하도록 wire". 본 메모는 그 wire 첫 시도의 진행 상태.

**How to apply**: 다음 wire 작업 진입 시 본 메모의 잔여 문제 list 부터 fix.

## 산출물

### 새 binary / example

- `render-engine/rust/examples/svg_surface_probe.rs` — SvgSurface 단위 검증 (fill/outline/text/image)
- `render-engine/rust/examples/wire_probe.rs` — rhwp PageRenderTree → SvgSurface adapter

### 라이브러리 변경

- **`hft-decoder/rust/src/cache.rs`**:
  - `pub fn load_hft_bytes(&mut self, filename: &str, bytes: &[u8])` 추가
  - `pub fn load_aliases_bytes(&mut self, bytes: &[u8])` 추가
  - `load_hft` 의 internal logic 을 `load_hft_inner(filename, bytes)` 로 분리 (코드 중복 제거)
- **`hft-decoder/rust/src/embedded.rs`** (신규):
  - `pub static FONTS: Dir<'_> = include_dir!(...)`  ← 180MB binary embed
  - `pub fn load_into(cache: &mut HftCache) -> Result<usize>` — 모든 *.HFT + hftinfo.dat → cache
  - `pub fn file_count() -> usize`
  - `pub fn get_file_bytes(filename: &str) -> Option<&'static [u8]>`
  - 3 tests
- **`hft-decoder/rust/Cargo.toml`**: `embedded` feature (`include_dir = "0.7"` optional)
- **`hft-decoder/.gitignore`**: `rust/fonts/*.HFT`, `rust/fonts/*.dat` 추가
- **`render-engine/rust/Cargo.toml`**: `kdsnr-hft = { path, features = ["embedded"] }`
- **`render-engine/rust/src/svg_surface.rs`** — group balance fix:
  - `gstate_stack: Vec<bool>` → `Vec<u32>` (frame 안 group 카운트)
  - `save_state` push(1), `concat_transform` 시 top count++, `restore_state` 시 count 만큼 `</g>` close
  - **이전 버그**: `save_state` + `concat_transform` 두 번 `<g>` open + restore 한 번에 1개만 close → SVG root 까지 손상

### binary 사이즈

- `wire_probe` binary: **188MB** (180MB embedded HFT + 8MB code/deps)
- `hft-decoder/rust/fonts/`: 403 files / 180MB (한컴 office HWP.app 의 Fonts/ 통째 rsync)

## wire_probe 측정 결과 (2026-05-18, 5 페어 page 0)

| 페어 | rhwp baseline | 우리 wire | 차이 | TextRun rendered/total |
|------|--------------:|---------:|------:|----------------------:|
| math Q01 | 97.43% | **97.56%** | +0.13%p | 2/12 |
| math Q06 | 99.21% | **99.32%** | +0.11%p | 6/16 |
| korean S18-21 | 91.39% | **91.58%** | +0.19%p | 25/39 |
| social Q01 | 88.92% | **89.06%** | +0.14%p | 32/42 |
| social Q08 (worst) | 85.91% | **86.15%** | +0.24%p | 34/46 |

**핵심 발견**: 모든 페어에서 우리 wire 가 rhwp svg.rs baseline 보다 +0.1~0.3%p 위. 부분만 wire 한 상태에서도 이미 우위 — wire 완성도 올리면 큰 격차 가능.

## wire 1차 매핑 대상 (구현 완료)

`wire_probe.rs::traverse` 가 rhwp RenderNode 종류별 SvgSurface 호출:

- **TextRun** → `svg.draw_string_point` (HFT glyph path emit). face name 그대로 cache.get 호출 (alias map 자동 처리). raw_font_family → font_family → hardcoded fallback 3단계.
- **Rectangle** → `svg.fill_rect_float` (있으면) + `svg.outline_rect_float` (있으면)
- **Line** → `svg.outline_path` (MoveTo + LineTo)
- **Image** → `svg.draw_image_f` (Option<Vec<u8>> 처리)
- **TableCell / Table / Page / Body / Column / Header / Footer / MasterPage / TextLine / PageBackground** → recurse 만 (내용 skip)
- **그 외** (Path / Ellipse / Equation / Group / TextBox / FormObject / FootnoteMarker / Placeholder / RawSvg) → skip + count

## 잔여 문제 (다음 wire 세션 진입점)

### P0 — 다음 +5~10%p 가능

1. **coordinate scale mismatch** — 우리 page bbox 1028×1489 (rhwp 가 계산), GT png 는 1399×다양. wire_probe 가 resvg 에 GT 크기로 scale 변환 — 이때 글자 위치 +30% scale 어긋남. **fix**: rhwp 가 보고하는 page bbox 와 GT png 사이 정확한 scale 비율 산정 + SvgSurface 의 viewBox 정합.

2. **Latin codepoint (ASCII/숫자/구두점) lookup 실패** — Latin face ("한양견명조" 등) → ASCII '8' (0x38) hit 0. 한컴 alias 정책: "Latin: alias 무시 → TTF text emit 폴백" ([alias.rs:172](kdsnr-hwp-toolkit/hft-decoder/rust/src/alias.rs#L172)). **fix**: TextRun 의 codepoint 별 분기 — Latin 은 TTF (resvg 의 system font) 로 그리고 Hangul/Hanja/Symbol 만 HFT path. 우리 SvgSurface 가 Latin 부분 `<text>` element 로 emit + HFT 부분 `<path>` emit 혼합.

3. **text positioning baseline** — 현재 `baseline_y = bbox.y + tr.baseline` 로 단순 합산. 한컴/rhwp 의 정확한 baseline 좌표 정합 검증 필요. visual diff 로 글자 위치 어긋남 확인.

### P1 — 추후

4. **Path 노드 매핑** — Bezier/곡선 도형. social/korean 페이지에 일부 등장 (skip count 로 확인).
5. **Equation 노드 매핑** — rhwp 의 equation tree → SvgSurface. byte-eq port (28-32 세션) 대신 rhwp 출력 그대로 emit.
6. **Group 노드 매핑** — 묶음 도형. save_state/restore_state 사용 (group balance 이미 fix).
7. **FootnoteMarker / Placeholder / RawSvg** — 빈도 낮음.
8. **TableCell 의 border** — 현재 셀 borders 가 자식 LineNode 로 들어오는지 확인 필요. 만약 아니면 셀 4개 border 직접 emit.
9. **PageBackground**, **MasterPage** — 페이지 배경 색/이미지. 현재 흰배경 fill 만.

### 발견된 SvgSurface 잠재 이슈

- **HFT cache 의 alias map 이 hftinfo.dat 없으면 작동 안 함** — embedded archive 가 alias 도 함께 박았으니 항상 작동.
- **Pen::new_default() brush 가 EmptyBrush** → stroke="none" 자동. caller (wire_probe::pen_for) 가 명시적으로 brush 주입 필요.

## 관련 메모리

- [[project-e2e-first-measurement]] — wire 진입 전 rhwp baseline 측정
- [[project-glyph-draw-state]] — byte-eq port 의 컴포넌트 상태
- [[reference-rhwp-svg-renderer]] — rhwp svg.rs 가 참고용만 ("한컴 1:1 아님") 인 이유 + 본 wire 가 그 한계를 우회
- [[feedback-no-time-optimization]] — wire 가 MVP 아님; 점진적 정공법 이어야

## 다음 세션 권장 진입점

1. **coordinate scale fix** — 가장 큰 +%p 예상. 단순 ratio 계산.
2. **Latin TTF fallback** — TextRun rendered 비율 ~76% → ~95%+ 예상.
3. **시각 검증** — `work/e2e/_wire_output/<key>_ours.png` 와 GT png 나란히 보고 어디 어긋났는지 확인.

본 세션 1332 + 7 tests pass (변경 없음).

## 2 차 진전 — 진짜 한컴 GT 와 비교 (2026-05-18 동일 세션)

### 발견: 기존 `work/e2e/<...>/<stem>.png` 는 rhwp 기반 crop GT 였음

- `kdsnr_hwp_toolkit.render.preview.render_question_png` = rhwp `export-pdf` → pdftoppm 200DPI → PIL crop (좌측 컬럼 + 본문 ink bbox). **rhwp 의 한계 그대로**.
- 그러나 `work/GT/<subject>__<sample>/<stem>.pdf` 는 **한컴 Office HWP.app "PDF로 인쇄"** 출력 — Producer "macOS Quartz PDFContext", 1 page each, 771×1117 pts (= 272×394 mm 시험지 페이지).
- 12 페어 진짜 한컴 GT pdf 보유: korean(2)/math2(3)/sci(2)/sci2(2)/social2(3).

### toolkit 의 진짜 용도

**수능 시험지 HWP/HWPX → 문항별 분할 + 각 문항 PNG 미리보기** 도구 (신규 개발 중, 외부 호출처 없음. flap-hwp-parser 는 레거시).

메인 API:
- `split_paper_to_hwpx_units(input, subject) -> [(label, hwpx_bytes), ...]`
- `split_paper_to_files(input, out_dir, subject, preview_type="png")`
- `render_question_png(hwpx, png)` — 한 문항 → png (현재 rhwp backend)

wire 의 종착점 = **`render_question_png` 의 backend 를 rhwp PDF route 에서 우리 SvgSurface 로 교체** → 한컴 office 없이도 한컴과 pixel-eq 한 미리보기.

### 신규 binary: `render-engine/rust/examples/wire_real_gt.rs`

```
work/GT/<.../stem.pdf>   → pdftoppm 200 DPI → tmp PNG (전체 페이지)
work/e2e/<.../stem.hwpx> → rhwp parse → render_tree → SvgSurface (HFT embedded)
                                                    → SVG → resvg 200DPI PNG
                                                    → pixel_diff vs GT PNG
```

좌표계 정합: GT 페이지 2142×3103 px @ 200 DPI = 272×394 mm. 우리 page bbox 1028×1489 px @ 96 DPI = same mm. resvg 가 2.084x uniform scale → 글자 모양 유지.

매핑 보강 (wire_probe 대비):
- **Path 노드** — rhwp PathCommand (MoveTo/LineTo/CurveTo/ArcTo/ClosePath) → surface::PathCmd. ArcTo 는 `rhwp::renderer::svg_arc_to_beziers` 헬퍼로 cubic bezier 변환
- **Equation 노드** — rhwp EquationNode.svg_content 가 이미 SVG fragment. `<g transform="translate(x,y)">` + raw push → wrap. RawSvg 와 같은 패턴
- **Group / RawSvg / TableCell** — 자식만 recurse (Group 자체 styling 없음)

### 측정 결과 (12 페어, 진짜 한컴 GT vs 우리 wire)

**평균 96.68%** (rhwp baseline 평균 93.99% 대비 +2.69%p).

| 과목 | n | 평균 | 최저 | 최고 |
|------|---|-----:|------:|------:|
| math (수식 多) | 3 | **98.61%** | 98.45% | 98.82% |
| science | 4 | 96.93% | 96.25% | 97.41% |
| social | 3 | 96.81% | 96.01% | 97.38% |
| korean (박스/밑줄) | 2 | 92.89% | 91.09% | 94.68% |

- worst: `korean S18-21 = 91.09%`
- best: `math Q28_2 = 98.82%`
- 100% 까지 평균 **3.32%p**

### 추가 잔여 문제

1. **face name alias miss** — text_skipped 비율 social Q20 = 88/148 (59%), korean S18-21 = 12/39 (31%). 일부 face 는 hftinfo.dat alias 도 못 잡음. **fix**: Latin codepoint 분기 (TTF 폴백, `<text>` element) + 더 정확한 face → canonical 매핑.
2. **avg_delta 150-170** — 글자 위치/모양 어긋남. baseline 계산 정확도 검증. visual diff.
3. **TableCell border** — math Q28_2 같은 표 셀 경계가 그려지는지 시각 확인. cell border 가 별도 LineNode 자식인지 아닌지 검증.
4. **science LAYOUT_OVERFLOW_DRAW** — rhwp 페이지 미세 버그 (overflow 3.7px). rhwp 패치 또는 우리가 우회.

### 산출물 디렉토리

- `work/e2e/_real_gt_output/<subdir>__<stem>_gt.png` — 한컴 PDF→PNG @200DPI (페이지 전체)
- `<subdir>__<stem>_ours.png` — 우리 SvgSurface→PNG (GT 크기)
- `<subdir>__<stem>_heatmap.png` — mismatch 빨강
- `<subdir>__<stem>_ours.svg` — 디버그용 SVG

### 다음 세션 권장 (96.68% → 100% 의 3.32%p 작업)

1. heatmap 분석 (analyze_heatmaps.py 와 비슷한 카테고리화) — 어디서 mismatch?
2. text_skipped 줄이기 (Latin TTF fallback + face alias 보강)
3. baseline 정확도 (rhwp text positioning 검증 + 필요 시 우리 byte-eq port 가 계산)

본 세션 1332 + 7 tests pass + 1 신규 binary (`wire_real_gt`).

## 3 차 진전 — Latin TTF fallback (2026-05-18 동일 세션)

### dump_tree 진단

신규 binary [dump_tree.rs](kdsnr-hwp-toolkit/render-engine/rust/examples/dump_tree.rs) 로 rhwp PageRenderTree JSON dump. Q28_2 분석 결과:

- **Header / Footer children count = 0** — rhwp 가 "수학 영역" / "국어 영역" 같은 페이지 헤더와 컬럼 사이 divider line 을 render_tree 에 노출 안 함. 한컴 PDF 의 master page 처리는 별도 (rhwp 의 한계).
- **Column children = 13** (TextLine + TextRun + Equation + Rect 모두 포함). bbox 절대 좌표 (페이지 1028×1489 안).
- TextRun bbox/text 정상 — `'28.'`, `' 실수 전체의 집합에서 미분가능한 함수 '`, `'\t② '` 등 한글 + ASCII 섞임. 공백 codepoint (0x20) 가 HFT lookup 실패해서 advance 0 → 글자 박살의 root cause.

### Latin TTF fallback 구현

[wire_real_gt.rs](kdsnr-hwp-toolkit/render-engine/rust/examples/wire_real_gt.rs) 의 TextRun 처리 재작성:

- 글자별 분기: HFT cache.get(family, cp) hit/miss
- hit → 기존 svg.draw_string_point (HFT glyph path)
- miss → SVG `<text>` element fallback (resvg 가 system font 로 그림)
- chunk 단위 emit (연속된 hit/miss group 화)
- advance 누적: HFT hit 면 glyph.advance \* scale, miss 면 codepoint 별 추정 (space 0.27em, 숫자 0.50em, ASCII 0.40em, 수학기호 0.60em, 기타 0.50em)

신규 helper: `emit_text_chunk()` + `xml_escape()`.

### 시각 결과 (2 페어 확인)

- **math Q28_2**: 공백/번호/줄 정렬/선지/분수/적분 ∫ 모두 정상. 수식 변수 (x, t, e, a italic) 만 `[?]` placeholder (EquationNode.svg_content 의 system font missing glyph)
- **korean S18-21**: 본문 텍스트 전부 정상, 따옴표/괄호 정상, **두 컬럼 layout 정상** (오른쪽 컬럼 제대로 배치), 줄 정렬 정상. 헤더/divider/㉠ 동그라미 mark 만 잔여 박살

### 점수 vs visible 의 괴리

| metric | 1차 wire | wire+Latin fallback | 변화 |
|--------|---------:|--------------------:|-----:|
| 평균 score | 96.68% | 96.60% | -0.08%p |
| visible 품질 | 글자 박살 + 공백 없음 + 수식 [?] | **본문 거의 정상**, 헤더/divider/수식 변수만 잔여 | **거대한 도약** |

**점수가 visible 개선을 못 잡음** — 흰배경 매치가 점수의 대부분. score metric 의 honest 화 (ink-only ROI) 필수. 현재 측정은 visible quality 의 proxy 가 아님.

### 잔여 박살 (visible 기준)

1. **EquationNode 의 svg_content 안 system font 글자 `[?]`** — 수식 변수 (x, t, e, a italic) 가 resvg 의 system font 에서 missing → 박스로 표시. fix: svg_content 의 `<text>` element 를 HFT path 로 치환, 또는 별도 font fallback 추가
2. **Header / Divider line missing** — rhwp render_tree 에 노출 안 됨 (master page 처리 누락). fix: hwpx XML 직접 parse + header 자식 추가, 또는 rhwp 패치
3. **㉠ 동그라미 enclosed Hangul mark 어긋남** — 유니코드 enclosed alphanumerics. HFT 와 system font 매핑 불일치
4. **Rect (박스 outline) 미렌더** — Q28_2 의 4 Rect 노드 (수식 박스) fill/stroke color 가 None 인 듯. rhwp 가 emit 한 ShapeStyle 확인 필요

### 신규 산출물

- `render-engine/rust/examples/dump_tree.rs` — render_tree JSON dump
- `render-engine/rust/examples/wire_real_gt.rs` 의 Latin fallback 구현 (~80 LOC 추가)
- `work/e2e/_real_gt_output/<key>_ours.png` 재생성 (1차 wire 대체)

### 다음 우선순위 (data-driven)

1. **fix #4 score honest 화** (ink-only ROI) — 현재 score 가 visible 개선을 잡지 못해 다음 fix 의 ROI 판단 불가
2. **EquationNode fix** — 수식 변수 visible 회복
3. **Header / Divider** — rhwp 한계 우회 (hwpx XML 직접 parse)

## 4 차 진전 — honest score (ink-only + IoU)

### `pixel_diff_harness` 신규 API

- `score_pages_ink_only(our, gt, w, h, opts, ink_threshold) -> PageScore` — ROI = union ink pixels (둘 중 한쪽이라도 luminance < threshold). 흰배경 매치 제외.
- `score_pages_ink_iou(our, gt, w, h, ink_threshold, dilate_radius) -> (iou_pct, gt_ink, our_ink, intersection, union)` — alignment-tolerant IoU. Manhattan ball dilate 적용 후 intersection/union. **시각 perception 과 가장 일치**.
- `dilate_mask(mask, w, h, radius)` 내부 helper.

### 진짜 측정 결과 (12 페어, 3 metric)

| metric | 평균 | 의미 |
|--------|-----:|------|
| full (흰배경 포함) | 96.60% | inflated (흰배경 매치) |
| strict ink-only | 1.16% | 너무 엄격 (1px 어긋남도 mismatch) |
| **IoU dilate-2px** | **16.75%** | **1-2px 어긋남 tolerant** |
| IoU dilate-5px | 26.91% | 매우 loose |

IoU2 페어별:
- best: social Q17 = **35.47%**
- worst: math Q28_3 = **5.05%** (수식 변수 [?])

### 결정적 통찰

- GT ink (~100k 픽셀) ≈ ours ink (~88-105k 픽셀) → 우리가 한컴과 **비슷한 양의 잉크** 출력
- 그러나 IoU 17% → **그 잉크가 다른 위치/모양**
- 즉 **글자 위치/advance/모양 정확도가 진짜 bottleneck**

### 다음 fix priority (data-driven)

1. 🥇 **Latin codepoint 도 HFT 사용** (HCEN*/EN* family) — system font 폐기. HFT 가 ASCII glyph 있고 advance 정확함. fix #1 의 Latin fallback (system font 의존) 의 후속 정공법
2. 🥈 **EquationNode 의 svg_content 직접 파싱 + HFT path 변환** — math 페어 worst IoU 5%
3. 🥉 **Header / Divider / Rect 박스** — rhwp 한계 우회 (hwpx XML 직접 파싱)
4. (별도) **font_size scale 정확도** — HWPUNIT → px 변환 + bold weight 적용

### 산출물

- `render-engine/rust/src/pixel_diff_harness.rs` 의 `score_pages_ink_only` + `score_pages_ink_iou` + `dilate_mask` 추가
- `render-engine/rust/examples/wire_real_gt.rs` 3 metric 출력 (CSV + aggregate)
