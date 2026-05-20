---
name: project-q11-dy-offset-root-cause
description: Q11 baseline dy=+2.8 systematic offset 의 root cause — splitter hwpx 의 stored lineseg.lh 자체가 한컴 saved 와 다름. rhwp 가 stored 값 사용해서 누적.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# Q11 systematic dy=+2.8 offset root cause (2026-05-19)

EQ_SCORE 26.57% (Q11 baseline) 의 가장 큰 lever 진단.

## 증상

`work/_eq_q11_baseline/components.csv` 분석:
- header divider (y=113.4): dy=0 ✅
- body 안 모든 Line/Table/Cell/TextLine/TextRun: **dy=+2.8 px 일정**
- footer divider (y=1357.0): dy=0 ✅

ours 의 body content 전체가 +2.8 아래로 일관되게 밀려있음. paragraph y 누적이 +2.8 어긋남.

## 진단

`rhwp dump -s 0 -p 1` 으로 두 hwpx 비교:

**ours hwpx (KSAT splitter output, `science_Q11.hwpx`)** pi=1 "11. 다음은 ...":
- ls[0]: vpos=2, lh=1300, th=1300, bl=1105, ls=520, tag=0x00160000
- ls[1]: vpos=1822, **lh=1300**, th=1300, bl=1105, ls=520, **tag=0x00060000**
- dump-pages: pi=1 h=42.2 (lines=34.7)

**saved hwpx (한컴 saved, `science_Q11_hwpsaved.hwpx`)** pi=1:
- ls[0]: vpos=2, lh=1300, th=1300, bl=1105, ls=520, tag=0x00060000
- ls[1]: vpos=1822, **lh=1150**, th=1150, bl=978, ls=460, **tag=0x00160000**
- dump-pages: pi=1 h=40.2 (lines=32.7)

차이 = pi=1 lines 34.7 - 32.7 = 2.0 px + sa 동일 (7.6) → Table vpos 차이 **210 HU = 2.8 px** ⭐

## Root cause

splitter 가 분할 hwpx 만들 때 paragraph 의 lineseg cache (`lh/th/bl/ls/tag`) 가 한컴 saved 결과와 다름. 특히:
- ls[1] (이어지는 line) 의 lh 가 1300 (== ls[0]) 로 채워짐, 한컴은 1150 (작은 폰트 last char 기준).
- tag bit position 20 (0x100000) 도 swap.

rhwp 는 stored lh 신뢰해서 paragraph height 계산하므로, paragraph h 가 한컴보다 2 px 큼. 누적으로 +2.8 px 어긋남.

[[project-splitter-enrich-linesegs]] 의 `enrich_linesegs(out_doc)` 가 paragraph linesegs 의 vp 만 채우고 lh/th/bl/tag 는 보정 안 함.

## 해결 경로 (multi-session)

3 가지 옵션:

1. ⭐ **rhwp 가 stored lh 무시하고 자기 계산** — paragraph_layout 의 line metric 결정 logic 을 한컴식으로 byte-eq port. lineseg.lh = max(char_height of run on this line) 같은 한컴 알고리즘 RE 필요. 한컴은 line break 후 line 안 last char 까지의 char shape 를 보고 lh 결정 (이 케이스: ls[1] 끝에 작은 폰트가 있어서 lh=1150 로 줄어듦). 정공법, 모든 hwpx 케이스에 효과.

2. **splitter enrich 가 lh/th/bl/tag 보정** — `compute_line_height` 함수를 한컴식으로 정확히 port 한 후 enrich_linesegs 에 추가. Q11 특정 해결.

3. **rhwp paragraph_layout 의 preserve_dims 분기 변경** — preserve_dims 조건을 더 엄격하게 (예: tag bit 20 검사). 임시 우회.

## 측정 도구

`python3 work/tool_component_eq.py ours.json saved.json --out diff_dir` 로 매 패치 후 측정.
fix 후 기대: Q11 EQ_SCORE 26.57% → ~38% (Line/TextLine/Cell/TextRun pos_ok 모두 회복).

관련: [[project-component-eq-metric]], [[project-splitter-enrich-linesegs]], [[project-kdsnr-break-apply]].
