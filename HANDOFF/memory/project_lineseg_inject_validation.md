---
name: project-lineseg-inject-validation
description: lineseg byte-eq port 정공법 검증 (2026-05-20). Q11 lineseg inject → 100% EQ_SCORE.
metadata:
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# lineseg inject 검증 — 정공법 spec (2026-05-20)

## 검증 결과

`work/tool_inject_hwpsaved_lineseg.py` 로 ours hwpx 의 `<hp:linesegarray>` 만 한컴 saved 의 동일 순서 array 로 raw XML swap → rhwp 렌더 → EQ_SCORE 측정 (Body-only, region offset cancel).

| Case | EQ_SCORE | TextRun full | 비고 |
|---|---|---|---|
| science_Q11 | **100.00%** | 100% | ✅ goal 달성 입증 |
| science_Q17 | 98.25% | 96.7% (59/61) | TextRun 잔여 2 |
| social_Q05 | 95.03% | 90.5% (86/95) | TextRun 잔여 9 |
| social_Q04 | 84.51% | 72.5% (29/40) | TextRun 잔여 11 |

**결정적 발견**: lineseg byte-eq → 모든 비-TextRun 컴포넌트 (Body/Cell/Column/Image/Line/Rect/Table/TextLine) **100% 매치**. 잔여는 오직 TextRun (char advance).

## 헛다리 인정 (지금까지 시간 낭비)

- `lineseg_gen.py` 의 `fill_lines/condense/tokenize/break` logic 만 만지작거림
- attribute-level byte-diff 를 spec 으로 쓰지 않고 EQ_SCORE 점수만 보면서 추정
- calibration / fill_lines / hft-decoder 등 도구 만지작
- 도구로 fuzzy=full_ok 처리 cheat 도 시도 (rollback)

## 정공법 = lineseg_gen.py 의 byte-eq port

### Q11 byte-diff spec (38 linesegarray, 25 mismatch — port 시 따라야 할 logic 카테고리)

| 카테고리 | diff | 우리 (ours) | 한컴 (saved) | 필요한 logic |
|---|---|---|---|---|
| **horzsize -1/-2 (미세)** | 14 | eff_w 1-2 큼 | eff_w − 1or2 | paragraph margin left/right + indent 1 HWPUNIT 정확화 |
| **horzsize → 0 (빈 para)** | 11 | cell_width 채움 | 0 | text/run 없는 paragraph → horzsize=0 |
| **textpos break 위치** | 6 | 어절 break | +2~+8 char 더 잘림 | 한컴식 break logic RE (조 어절/한글-mid) |
| **vertpos ls[0] +2** | 1 | 0 | 2 | paragraph margin_top +2 HWPUNIT (`space-before/2` 같은 spacing) |
| **vertpos 큰 차이 (P30/P31)** | 2 | section 누적 | 절대 vp | **별도 layer** (page/column break — lineseg_gen 밖) |
| **vertsize/textheight/baseline/spacing** | 5 | para 단일 font 1300 | line max char height 1150 | **이미 patch 적용 (메모리: project-lineseg-line-max-height)**, regenerate 필요 |
| **flags first/last swap** | 3 | 1441792 (first line) | 1441792 (last line) | paragraph-end bit 위치 = last line (한컴), first line (우리) |
| **horzpos** | 0 | 동일 | 동일 | OK |

### 가장 큰 victory potential

- **horzsize 0 처리 + horzsize -1/-2** = 25 diff 중 25 (전부). 이 2 logic 만 port 해도 Q11 의 lineseg mismatch 25 → ~10 즉시 감소.
- 다음 = flags bit swap, vertpos +2.
- 가장 어려운 lever (한컴식 break) = textpos 6 diff 만 영향.

## 다음 step 우선순위

1. **horzsize 0 (빈 paragraph) 처리** — `lineseg_gen.py:generate_linesegs` 에서 text 비어 있으면 `horz_size=0` 출력
2. **horzsize -1/-2** — paragraph margin 또는 indent 1-2 HWPUNIT 차이 RE
3. **flags bit swap** — paragraph-end bit 를 last line 에 (현재 first line)
4. **vertpos ls[0] +2** — paragraph 첫 line space-before
5. **vertsize line max-h (이미 patch, regenerate)** — 모든 ours hwpx 재생성 후 inject diff 다시
6. **textpos break (가장 어려움)** — paragraph.condense 따라 어절 vs 한글-mid

## 도구/스크립트 박제

- `work/tool_inject_hwpsaved_lineseg.py` — raw XML linesegarray swap (검증 mode 도구)
- `/tmp/lineseg_diff_q11_v2.py` — 38 lineseg attribute-by-attribute diff (필요시 재실행)
- `work/_render_4subj_5q/_injected/` — inject 된 hwpx 4 개
- `work/_render_4subj_5q/_rt_injected/` — 그 render tree JSON
- `work/_render_4subj_5q/_rt_saved_fresh/` — fresh saved render tree JSON
- `work/_render_4subj_5q/_eq_injected/` — EQ_SCORE summary (4 케이스)

## 4 케이스 aggregate byte-diff spec (2026-05-20)

| attr | Q04 | Q05 | Q11 | Q17 | 합 |
|---|---|---|---|---|---|
| horzsize | 1 | 1 | 25 | 23 | **50** |
| vertpos | 10 | 10 | 5 | 7 | 32 |
| textpos | 0 | 0 | 6 | 7 | 13 |
| spacing | 0 | 0 | 3 | 4 | 7 |
| horzpos | 0 | 0 | 0 | 5 | 5 |
| flags | 0 | 0 | 3 | 2 | 5 |
| baseline | 0 | 0 | 2 | 2 | 4 |
| vertsize/textheight | 0 | 0 | 1 | 2 | 3 |

**케이스별 특성**:
- Q04/Q05: vertpos 큰 차이 (15807/33494 HWPUNIT ≈ 1 page) — **paragraph 가 page 경계 넘김** (splitter pagination 차이, lineseg 무관)
- Q11/Q17: 모든 attribute 광범위 mismatch — lineseg generate RE

**horzsize 50 breakdown**: `-1` 22개 (상수), `-2` 6개, 빈 paragraph (-1417/-25235/-2861) 11개. 22 paragraph 의 정확한 -1 = eff_w 계산 1 HWPUNIT 상수 차이.

## TextRun 잔여 = 진짜 99% goal 의 2 번째 layer

**inject 한 hwpx (lineseg = 한컴 native) 에서도 Q04 84.51%, Q05 95.03%**. 즉 path A/B/C (lineseg byte-eq) 어느 것이든 완료해도 Q04/Q05 의 TextRun 잔여 그대로.

Q04 의 TextRun fail 11 개 패턴:
- 번호 `①②③④⑤` 자체 width: dw -4/-10/-17 (size_ok=False)
- 그 뒤 텍스트 "  대중문화...": dx -4/-10/-17 (앞 번호 width 차이로 왼쪽 밀림)
- 빈 text run: style_diff (`ff:함초롬돋움≠함초롬바탕; fs:13.33≠15.31; ls:0.0≠-0.46; ratio:1.0≠0.95`)

Q17 의 TextRun fail 2 개: `'ㄱ.'`/`'ㄷ.'` dw -361/-199 (매우 큼, TextRun grouping 차이로 추정).

**99% goal = layer 1 (lineseg) + layer 2 (TextRun)**.

Layer 2 의 lever (별도 layer, 후속):
- char advance metric — 번호 ①②③ 의 한컴 width vs 우리 width 차이
- TextRun grouping — 우리가 run 을 일찍 끊고, 한컴은 더 길게 묶음
- 빈 run style fallback — font fallback 차이

관련: [[project-lineseg-line-max-height]], [[project-component-eq-metric]], [[project-q11-dy-offset-root-cause]], [[feedback-deep-read-before-patch]].
