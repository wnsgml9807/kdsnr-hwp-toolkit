# Context — 현재 작업 상황 (2026-05-20)

## Goal

```
한컴 export(GT).PDF 와 rhwp 렌더링 PNG 가 pixel-eq
EQ_SCORE ≥ 99% across all test cases  (memory: project_component_eq_metric.md)
```

EQ_SCORE 정의 = Body-only component (Body/Cell/Column/Image/Line/Rect/Table/TextLine/TextRun) 의 full-eq 비율. region offset cancel. tool: `work/tool_component_eq.py`.

## 현재 진척 (2026-05-20 측정)

| Case | inject mode (lineseg 한컴 swap) | 우리 splitter+rhwp generate | env-gate ON (G-W-3a wire) |
|---|---|---|---|
| science_Q11 | **100.00%** ✅ | ~75% (Q11 baseline 79.82→85.32 patch 후) | 52.29% (회귀) |
| science_Q17 | 98.25% | ~70% | 14.91% (회귀) |
| social_Q05 | 95.03% | 95.03% | 67.40% (회귀) |
| social_Q04 | 84.51% | ~84% | 46.48% (회귀) |
| **평균** | 94.45% | ~80% | 45.27% |

**핵심 발견**:
1. **lineseg byte-eq inject 만으로 Q11 100% 달성** (`work/tool_inject_hwpsaved_lineseg.py` 검증) — 정공법의 spec
2. **env-gate ON (rhwp wire = line break 만 byte-eq) 은 회귀** — line metric (vertsize/baseline/spacing) 우리 stored 가 한컴 byte-eq 아니라서
3. **단일 작은 patch (flags swap) 도 회귀 위험** — Q04/Q05 의 paragraph 가 다른 path 거치므로 generate_linesegs 의 단일 logic 변경이 망침

## Byte-diff Spec (4 케이스 aggregate)

ours (splitter output) vs hwpsaved (한컴 saved) 의 lineseg attribute 차이:

| attr | Q04 | Q05 | Q11 | Q17 | 합 |
|---|---|---|---|---|---|
| horzsize | 1 | 1 | 25 | 23 | **50** |
| vertpos | 10 | 10 | 5 | 7 | 32 (Q04/Q05 의 10 = paragraph page 경계 넘김, 별도 layer) |
| textpos | 0 | 0 | 6 | 7 | 13 |
| spacing | 0 | 0 | 3 | 4 | 7 |
| horzpos | 0 | 0 | 0 | 5 | 5 |
| flags | 0 | 0 | 3 | 2 | 5 |
| baseline | 0 | 0 | 2 | 2 | 4 |
| vertsize/textheight | 0 | 0 | 1 | 2 | 3 |

**horzsize 50 breakdown**: `-1` 22개 (상수), `-2` 6개, 빈 paragraph (-1417/-25235/-2861) 11개.

## TextRun 잔여 (lineseg 무관 별도 layer)

Q04/Q05 의 EQ_SCORE 84.51/95.03% 는 *inject 후에도* 그대로. TextRun 11/9 개 mismatch:
- 번호 `①②③④⑤` width: dw -4~-17 (size_ok=False)
- 뒤 텍스트 dx -4~-17 (앞 번호 width 차이로 왼쪽 밀림)
- 빈 text run style: `ff:함초롬돋움≠함초롬바탕; fs:13.33≠15.31; ls:0.0≠-0.46; ratio:1.0≠0.95`
- 'ㄱ.'/'ㄷ.' dw -361/-199 (TextRun grouping)

별도 layer: char advance metric (HFT cache miss 일부), TextRun grouping (한컴이 더 길게 묶음), 빈 run style fallback.

## Path 후보 비교 (이번 세션 결론)

| path | 내용 | 분량 (수정 추정) | risk |
|---|---|---|---|
| **A** | lineseg_gen.py byte-eq port (Python) — paragraph caller 별 다른 logic, ppt_compose_break 1058줄 Python translate | 7~11 세션 | 작은 patch 도 paragraph caller 별 logic 다름 회귀 (이번 세션 입증) |
| **B** | rhwp 의 kdsnr-layout wire 완성 (Rust) — per-char tokenization, ParaProperty composition, segment_width 가드, line metric byte-eq generate | 5~7 세션 (메모리 추정) → 실측 결과 line metric byte-eq 도 필요 → 10+ 세션 | env-gate ON 회귀 (이번 세션 입증) — 추가 wire 필요 |
| **C** | macOS HWP AppleScript 자동화 — splitter output 을 한컴 GUI 자동 open+save → hwpsaved 직접 렌더 | 1~3 세션 | production 한컴 SW 의존 |
| **A+B** | A 의 작은 patch + B 의 wire 결합 | 4~6 세션 (가설) | 사용자 의도 (이번 세션 결정) |

**사용자 결정**: A+B 결합 (마지막 메시지). 단 작은 patch 회귀 입증 후 deep read 요구. 그 후 PNG 시각 비교 후 미완.

**검증된 사실**:
- macOS HWP 12.30.0 AppleScript 응답 확인 (`osascript -e 'tell application "Hancom Office HWP" to get version'` = `12.30.0`)
- 즉 C path 기술적으로 가능

## 이번 세션 시도/결과

1. ✅ Q11 byte-diff attribute level 전수 spec 추출 (38 linesegarray, 25 mismatch)
2. ✅ 4 케이스 aggregate spec (Q04/Q05 의 vertpos 큰 차이 = paragraph page 경계 — lineseg 무관)
3. ✅ env-gate ON 측정 (Q11 52.29% 회귀) — B path 단독으론 부족 입증
4. ❌ 작은 patch 시도 (flags swap + horz_size 0/-1) → Q04/Q05 회귀 +18/+15 → 롤백
5. ✅ Q11 ours vs hwpsaved PNG 시각 비교 — 한컴이 한글 mid break, 우리는 어절 단위 break
6. ✅ 메모리 `project_lineseg_inject_validation.md` 박제 + MEMORY.md 인덱스 업데이트

## 미해결 / 다음 step 후보

1. **C path** (한컴 GUI AppleScript) 실제 PoC — 가장 빠른 path. production 호환 확인 필요
2. **A path** 의 paragraph caller 별 logic 정확화 — `generate_linesegs_for_paragraph` 의 caller path 분석 필요 (어떤 paragraph 가 어떤 path 거치는지)
3. **B path** wire 완성 + line metric byte-eq generate — `make_line_seg` 가 stored 무시하고 한컴식 generate, `font_size_to_line_height` / `compute_line_spacing_hwp` 의 한컴 식 RE
4. **TextRun 잔여 layer** (별도) — char advance / grouping / 빈 run style. Q04 EQ_SCORE 99% 도달의 마지막 lever

## 다음 AI 가 결정해야 할 것

1. **path 선택**: A / B / C / A+B 중 어느 것
2. **goal 도달 기준 재확인**: 평균 99% vs 모든 케이스 99%. 메모리는 후자 (`EQ_SCORE ≥ 99% = goal 달성`)
3. **production 환경 제약**: 한컴 SW 가 production 에 있는지 (C path 가능 여부)

## 코드 변경 상태

- `lineseg_gen.py` line 586-602: **롤백 완료** (patch 시도 → 회귀 → 롤백). 현재 main branch 와 동일
- 코드 변경 없는 측정 결과만 메모리에 박제됨
