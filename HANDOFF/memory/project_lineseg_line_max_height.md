---
name: project-lineseg-line-max-height
description: lineseg_gen.py 의 line_text_h 결정 logic byte-eq port (2026-05-19). paragraph 단일 font size → line 안 chars 의 max char_shape.height.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# lineseg_gen.py line metric byte-eq port (2026-05-19)

## 진단 (Probe)

Q11 paragraph 0.1 "11. 다음은 ... 대화이다.":
- charPr 동일 (ours id=37/63, saved id=27/30 — id 번호만 다르고 height/ratio 등 property 동일).
  - id=37/27: height=1300 ratio=100% (pos=0~2 "11.")
  - id=63/30: height=1150 ratio=95% (pos=3~끝)
- 한컴 saved stored linesegs:
  - ls[0]: lh=1300 (line 1: chars 0-34, max char height = 1300 — 처음 3 chars 가 height=1300)
  - ls[1]: lh=1150 (line 2: chars 35-, 모두 height=1150)
- ours hwpx (splitter output) stored linesegs:
  - ls[0]: lh=1300 ✅
  - ls[1]: **lh=1300** ❌ (paragraph 단일 font size 사용 — 잘못)

rhwp 가 stored lineseg.lh 신뢰 → ours ls[1] lh=1300 으로 line_step=24.27px. saved 21.47px. 차이 2.8px → 모든 후속 component (Line/Table/Cell/TextLine/TextRun) +2.8 systematic offset 누적.

## Fix

`src/kdsnr_hwp_toolkit/lineseg_gen.py`:

1. `generate_linesegs` 시그니처에 `per_char_heights: Optional[list[int]]` 추가.
2. line iteration 안에서 `line_text_h = max(per_char_heights[line.text_start:line.text_end])` (height > 0 만).
3. baseline 도 `line_text_h * baseline_ratio` 재계산. spacing 도 `line_text_h * (line_spacing_pct-100)/100` 재계산.
4. `generate_linesegs_for_paragraph` 에서 `raw_cs_map + char_t` 로 per_char_heights 빌드 후 전달.

검산: line_text_h=1150, line_spacing_pct=140 → baseline `round(1150*0.85)=978`, spacing `(1150*40+50)//100=460` — 한컴 saved.ls[1] (bl=978, ls=460) 와 완전 일치 ✅.

## 효과 (2026-05-19 측정)

| Case | baseline | patched | 진척 |
|---|---|---|---|
| science_Q11 | 79.82% | **85.32%** | +5.5% |
| science_Q17 | 65.79% | **70.18%** | +4.4% |
| social_Q05 | 95.03% | 95.03% | 변동 0 (입력 hwpx 가 Hancom-touched, 영향 없음) |

Q11 Body offset dy=+2.80px → **0**. Line/Cell/Table/Image/Rect/TextLine 모두 100% full_ok 도달. 잔여 fail = TextRun width/advance 차이 (별도 lever).

## Step 2: char width byte-eq (em × ratio)

`CharWidthTable.register_doc_metric(doc_cpr_id, char_metric)`:
- HFT probe 결과 한국어 폰트는 **fixed-em advance** (모든 한글 metric (em, em, 0, 0))
- 한컴 식: **width = em × ratio_hangul/100** (spacing 은 char 간 자간, advance 자체에 미적용)
- doc charPr id 직접 사용 (calibration unified template 의존성 제거)
- `generate_linesegs_for_paragraph` 에서 `map_to_unified` 호출 제거, raw doc cs id 사용

## 다음 lever

ts 차이: Q11 ours **33** vs saved **35** (2 chars). width 식 정확, 잔여 = `fill_lines` greedy 의 한컴식 RE.
- 어절 단위 break vs char 단위
- LINE_START_FORBIDDEN/END_FORBIDDEN 적용 차이
- 한컴 compose_break 정공법 port

관련: [[project-q11-dy-offset-root-cause]], [[project-component-eq-metric]], [[project-full-byteeq-plan]].
