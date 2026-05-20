# Harness — 측정 도구 + 검증 절차

## 디렉토리 구조

```
kdsnr-hwp-toolkit/
├── src/kdsnr_hwp_toolkit/      # Python SDK (splitter / lineseg_gen / codec / render)
├── vendor/rhwp/                 # Rust HWPX 렌더러 (vendored, fork)
│   └── target/release/rhwp      # binary
├── layout-decoder/rust/         # kdsnr-layout (한컴 raw asm port, 25357 줄)
│   ├── src/ppt_compose_break.rs       # 1058 줄
│   ├── src/ppt_compose_layout.rs      # 3726 줄
│   └── src/composition.rs             # 3512 줄
├── hft-decoder/                 # HFT 폰트 디코더 (완성, 6 family 모두)
├── work/                        # 측정/디버그 산출물
│   ├── tool_component_eq.py     # EQ_SCORE 측정 (goal metric)
│   ├── tool_inject_hwpsaved_lineseg.py  # GT lineseg inject (검증용)
│   ├── tool_render_tree_diff.py # render tree JSON diff
│   ├── tool_pixel_diff_strong.py # PNG pixel diff
│   ├── tool_hwpx_xml_diff.py    # hwpx XML diff
│   ├── _render_4subj_5q/        # 4 케이스 측정 디렉토리
│   │   ├── _hwpsaved/           # ours hwpx + 한컴 saved hwpx + GT PNG
│   │   ├── _injected/           # GT lineseg inject 한 hwpx
│   │   ├── _rt_saved_fresh/     # 한컴 saved 의 render tree JSON
│   │   ├── _rt_injected/        # inject hwpx 의 render tree JSON
│   │   ├── _rt_ours_envgate/    # env-gate ON 한 우리 render tree JSON
│   │   ├── _eq_injected/        # inject EQ_SCORE 결과
│   │   └── _eq_envgate/         # env-gate ON EQ_SCORE 결과
│   └── e2e/                     # 12 페어 (.hwpx + .hwp + .png) Phase C 검증 set
└── HANDOFF/                     # ← 이 문서
```

## 측정 도구

### 1. EQ_SCORE — goal metric

```bash
python3 work/tool_component_eq.py <ours.json> <hwpsaved.json> --out <out_dir>
```

- 입력: render tree JSON (rhwp dump-render-tree 출력)
- 출력: `summary.txt` (EQ_SCORE 한 줄 + type 별 매칭), `components.csv` (paired diff), `unmatched.txt`
- Body 안 component 만 계산. region offset 자동 cancel.
- **goal**: `EQ_SCORE ≥ 99%` (project_component_eq_metric.md)

### 2. render tree dump

```bash
RHWP_BIN=vendor/rhwp/target/release/rhwp
HFT_DIR="/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts"
"$RHWP_BIN" dump-render-tree <hwpx> -p 0 -o <out.json> --hft-path "$HFT_DIR"

# env-gate ON (한컴 byte-eq line break wire 작동)
RHWP_USE_KDSNR_LAYOUT=1 RHWP_USE_KDSNR_BREAK_APPLY=1 \
  "$RHWP_BIN" dump-render-tree ...
```

### 3. GT lineseg inject (검증 mode)

```bash
python3 work/tool_inject_hwpsaved_lineseg.py
# 또는: python3 work/tool_inject_hwpsaved_lineseg.py <ours.hwpx> <hwpsaved.hwpx> <out.hwpx>
```

- 한컴 saved hwpx 의 모든 `<hp:linesegarray>` 를 ours hwpx 에 1:1 raw XML swap
- paragraph 1:1 매핑 보장 (4 케이스 모두 paragraph 수 동일 확인됨)
- inject 결과 = byte-eq spec 의 upper bound

### 4. PNG 렌더 (API SDK)

```python
import sys
sys.path.insert(0, 'src')
from kdsnr_hwp_toolkit.render.preview import render_question_png
render_question_png('input.hwpx', 'output.png')
```

내부: rhwp → PDF → PNG (DPI 200) → crop (좌측 컬럼 + 헤더 divider 검출)

### 5. 4 케이스 splitter 재생성

`work/_render_4subj_5q.py` — 4 과목 × 5 문항 split + render. 일부만 필요 시 `/tmp/regen_4cases.py` 참고.

원본 입력: `templet/original/{social_test_input_2,science_input_example_2,...}.hwpx`

## 표준 측정 절차

코드 변경 후 EQ_SCORE 검증:

```bash
# 1. (코드 변경 후) 4 케이스 재 splitter
python3 /tmp/regen_4cases.py  # → work/_render_4subj_5q/_patched/{tag}.hwpx

# 2. render tree dump (env-gate OFF, A path only)
for tag in social_Q04 social_Q05 science_Q11 science_Q17; do
  vendor/rhwp/target/release/rhwp dump-render-tree \
    work/_render_4subj_5q/_patched/${tag}.hwpx \
    -p 0 -o work/_render_4subj_5q/_rt_patched/_rt_${tag}.json \
    --hft-path "/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts"
done

# 3. EQ_SCORE 측정 (vs 한컴 saved render tree)
for tag in social_Q04 social_Q05 science_Q11 science_Q17; do
  python3 work/tool_component_eq.py \
    work/_render_4subj_5q/_rt_patched/_rt_${tag}.json \
    work/_render_4subj_5q/_rt_saved_fresh/_rt_saved_${tag}.json \
    --out work/_render_4subj_5q/_eq_patched/${tag}
done
```

## 4 케이스 reference points

| Case | inject EQ | baseline EQ (우리 generate) | env-gate ON EQ | 비고 |
|---|---|---|---|---|
| science_Q11 | **100.00%** | ~75% | 52.29% | byte-eq 가능 입증 |
| science_Q17 | 98.25% | ~70% | 14.91% | TextRun 2 잔여 |
| social_Q05 | 95.03% | 95.03% | 67.40% | TextRun 9 잔여 (stored 이미 GT 와 거의 같음) |
| social_Q04 | 84.51% | ~84% | 46.48% | TextRun 11 잔여 |

inject 결과 = 모든 lineseg attribute 한컴 swap 시 도달 가능 ceiling.
env-gate 결과 = rhwp wire (line break 만 byte-eq) 의 현재 상태 (line metric 우리 stored 사용으로 회귀).

## 사용자 정책

- **No cheat**: 도구로 fuzzy=full_ok 처리, 임의 매칭 완화 등 금지. EQ tool 의 정확도 변경은 user 승인 후만
- **Deep read before patch** (`feedback_deep_read_before_patch.md`): patch 대상 모듈 + 관련 파일 전수 read. 찔끔 patch 금지
- **Probe-driven** (`feedback_probe_driven.md`): 추측으로 코드 작성 금지. 측정 도구로 사전/사후 검증
- **Eval harness last** (`feedback_eval_harness_last.md`): 점수만 보면서 작업 금지. byte-eq port 끝난 뒤 pixel-diff harness 구축

## 알려진 도구 한계

- **EQ tool 의 spatial nearest match**: 두 dataset 에서 type 별 가까운 component 매칭. 잘못 매칭될 수 있음 — 단 region offset cancel 후 분포 검토로 확인 가능
- **TextRun grouping 차이**: 우리는 char shape 별로 run 분할. 한컴은 더 길게 묶음. EQ tool 의 TextRun 매칭이 1:1 못하면 fail 발생 (Q17 의 'ㄱ.'/'ㄷ.' dw -361 사례)
- **render tree JSON 의 style 속성**: 일부 속성 (letter_spacing, ratio) 만 비교. font fallback chain 의 차이는 잡지 못할 수도

## 코드 path map

- `kdsnr-hwp-toolkit/src/kdsnr_hwp_toolkit/compose/splitter.py:719` — splitter 가 `enrich_doc(out_doc)` 호출
- `kdsnr-hwp-toolkit/src/kdsnr_hwp_toolkit/layout/linesegs.py:fill_missing` — paragraph.linesegs_xml 비어있는 것만 채움
- `kdsnr-hwp-toolkit/src/kdsnr_hwp_toolkit/lineseg_gen.py:510-604` — `generate_linesegs` (paragraph → LineSeg list)
- `kdsnr-hwp-toolkit/src/kdsnr_hwp_toolkit/lineseg_gen.py:867-961` — `generate_linesegs_for_paragraph` (caller)
- `kdsnr-hwp-toolkit/src/kdsnr_hwp_toolkit/lineseg_gen.py:968+` — `enrich_linesegs(doc)` 진입점

- `vendor/rhwp/src/renderer/kdsnr_bridge.rs:515 줄` — Rust IR ↔ kdsnr-layout adapter
- `vendor/rhwp/src/renderer/composer/line_breaking.rs:1223 줄` — `reflow_line_segs` + `compute_kdsnr_breaks` (apply gate)
- `vendor/rhwp/src/renderer/composer.rs:117-166` — `compose_paragraph_with_reflow` (env gate)
- `vendor/rhwp/src/renderer/layout/paragraph_layout.rs:1703` — `estimate_text_width` G-W-2 wire 위치
- `vendor/rhwp/src/main.rs:2727+` — `dump-render-tree` 서브커맨드

- `layout-decoder/rust/src/ppt_compose_break.rs` — 한컴 ColCompositor::ComposeBreak port (1058 줄)
- `layout-decoder/rust/src/ppt_compose_layout.rs` — 한컴 PptCompositor::ComposeLayout port (3726 줄)
- `layout-decoder/rust/src/composition.rs` — Composition + Glyph base (3512 줄)

## 메모리 cross-reference

핵심 메모리 (모두 `HANDOFF/memory/` 에 복사됨):

- `project_lineseg_inject_validation.md` — **가장 최근 (2026-05-20) 핵심**. inject 검증 + byte-diff spec
- `project_component_eq_metric.md` — EQ_SCORE goal metric 정의
- `project_g_phase_d_plan.md` — rhwp wire 진행 history (G-W-1 / W-2 / W-3a)
- `project_kdsnr_break_apply.md` — KDSNR_BREAK env gate 효과 측정
- `project_lineseg_line_max_height.md` — line max char height patch
- `project_q11_dy_offset_root_cause.md` — Q11 dy=+2.8 root cause
- `feedback_deep_read_before_patch.md` — patch 정책
- `feedback_no_time_optimization.md` — 정공법 정책
- `feedback_eval_harness_last.md` — eval 정책
- `feedback_probe_driven.md` — probe 정책
