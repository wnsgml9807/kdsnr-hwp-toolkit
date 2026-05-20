---
name: project-g-phase-d-plan
description: G phase (HYBRID_LAYOUT_PLAN Phase D) — rhwp paragraph layout 을 kdsnr-layout 으로 교체. 9 step 분산
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# G (HYBRID_LAYOUT_PLAN Phase D) 진입 plan — 2026-05-18

목표: rhwp 의 paragraph layout (`compose_lines` + typeset 글자 단위 advance) 을 우리 kdsnr-layout (한컴 byte-eq port `ColCompositor::ComposeLayout`) 으로 교체.

**Why**: 단기 visible fix (master page / picture mime / line startPt) 로 IoU 16.68→24.7% 까지 끌어올림. 나머지 격차 = 글자 위치/advance/break 의 미세 차이. byte-eq layout 으로만 100% pixel 가능.

**How to apply**: 각 step ≈ 1 세션. 9 step 분산 진행. 매 세션 빌드 통과 + 가능하면 milestone 측정.

## 9 step 분산 plan

| step | 내용 | 산출물 |
|------|------|--------|
| G-2-1 | RunProperty/ParaProperty/BodyProperty 어댑터 가능성 audit | mapping table |
| G-2-2 | rhwp ResolvedCharStyle → kdsnr-layout RunProperty 어댑터 | helper fn |
| G-2-3 | rhwp ResolvedParaStyle → kdsnr-layout ParaProperty 어댑터 | helper fn |
| G-2-4 | font metric source → CharItemViewConstructorMetrics 어댑터 (HFT or CoreText) | helper fn |
| G-3   | 단일 paragraph 의 char seq → Composition + CharItemView seq build | helper fn |
| G-3-test | 한 paragraph 입력 → ColCompositor::ComposeLayout → 결과 dump | probe binary |
| G-3-verify | Phase C harness — 결과 vs HWPX stored line_segs 비교 | verification |
| G-4   | 결과 Glyph tree → SvgSurface dispatch | wire |
| G-5   | 측정 vs GT (Phase D 완료) | final score |

## G-2-1 audit (현 세션)

### kdsnr-layout side
- `RunProperty`: PropertyBag (HashMap) 기반. keys: `FONT_SIZE`, `FONT_SIZE_ADJUST`, `BOLD_FLAG`, `ITALIC_FLAG`, `METRIC_96B` (line 338, [glyph.rs:2014](kdsnr-hwp-toolkit/layout-decoder/rust/src/glyph.rs#L2014))
- `font_table`: 4-slot (`Latin/CJK/script/symbol`) — `CharItemView::GetRealFont` 가 char class 로 슬롯 선택
- `CharItemView::from_constructor_metrics(char_code, run_prop?, para_prop?, body_prop?, reset_or_size, metrics)`
- `CharItemViewConstructorMetrics { metric_3c, ascent, descent, metric_4c, metric_50 }`

### rhwp side
- `ResolvedCharStyle`: font_size/font_families[7]/raw_font_families[7]/bold/italic/letter_spacing/ratio/text_color/...
- `ResolvedParaStyle`: alignment/line_spacing/margin_left/margin_right/indent/spacing_before/spacing_after/tab_stops/...
- font metric source: rhwp `font_runtime_metrics.rs` (ttf-parser, hmtx) + `font_metrics_data.rs` (HANCOM_FONT_METRICS 테이블)

### 매핑 (G-2-2 ~ G-2-4 작업)
1. ResolvedCharStyle.font_size → bag[FONT_SIZE]
2. ResolvedCharStyle.bold → bag[BOLD_FLAG]
3. ResolvedCharStyle.italic → bag[ITALIC_FLAG]
4. ResolvedCharStyle.font_families[0..7] → FontTable 4-slot 매핑 (한글/Latin/symbol 매핑 규칙)
5. font metric: 어떤 source 사용? 옵션:
   - (a) rhwp font_runtime_metrics::measure_char_advance_em → ascent/descent
   - (b) hft-decoder Glyph.em / advance → ascent/descent 추정
   - (c) CoreText FFI (font_metric_coretext.rs)
   - (d) 한컴 GetRealFont 의 raw decompile port

(d) 가 가장 정공법이지만 큰 작업. (a)/(c) 부터 시도 + 점차 (d) 로.

## 다음 세션 entry

G-2-2 부터. rhwp ResolvedCharStyle → kdsnr-layout RunProperty 어댑터 작성.
파일 위치 추정: `kdsnr-hwp-toolkit/render-engine/rust/examples/` 안 새 probe binary 또는
`layout-decoder/rust/src/` 안 새 adapter module.

## G-3 smoke (working flow 확인, 2026-05-18)

`layout-decoder/rust/examples/probe_g_paragraph.rs` 작성:
- "AB\r" (3 chars) → CharItemView seq → MutComp
- ppt_compose_layout(comp, type=1, Break(0, 2), p5=-1, p6=0, output)
- 결과: 9 items append:
  ```
  [0-1] Glue (pre-pad 2개)        ← stage 7 pre-pads
  [2]   null Append (predecessor)  ← stage 8 from-1=-1 invalid
  [3-5] CharItemView A/B/CR        ← stage 8 main loop
  [6]   null Append (successor)    ← stage 10 to+1=3 OOR
  [7-8] Glue (post-pad 2개)        ← stage 13 post-pads
  ```
- 한컴 ColCompositor::ComposeLayout 의 raw 13-stage 패턴 정확
- CharItemView width/ascent/descent = 0 (RunProperty/font metric 미지정)

→ **kdsnr-layout 직접 호출 flow 작동 확인**. 다음 = RunProperty 채우기 + font metric 연결 = byte-eq 시작점.

## G-2-2 metric flow (2026-05-18)

`probe_g_paragraph.rs` 확장 — RunProperty + CharItemViewConstructorMetrics 채워 ppt_compose_layout 호출.

helper `build_char_item_view(char, font_size, width, ascent, descent)`:
- RunProperty bag: FONT_SIZE = font_size
- CharItemViewConstructorMetrics: ascent/descent/metric_4c=width 채움
- CharItemView::from_constructor_metrics 호출

Run 2 결과:
```
CharItemView char='A'  width=10.00  ascent=8.00  descent=2.00
              total_height=13.33  line_height=20.80
```
- total_height = (ascent+descent) * dpi/72 변환 + offset
- line_height = font_size * 1.2 * dpi/72 = 13 * 1.2 * 96/72 = **20.80** ✅
- compute_metrics 가 한컴 raw `FUN_002ef798` line 243-297 의 paragraph_class != 5/6 분기 byte-eq 호출

→ metric flow 전체 검증. RunProperty + metric 정확 입력 시 한컴 동일한 derived 값 산출.

## 다음 세션 entry (refined)

- G-3-real: 진짜 rhwp HWPX → 1 paragraph → CharItemView seq → wire
  - HFT cache 기반 글자별 width (advance) 추출
  - CoreText 기반 ascent/descent (또는 HFT em 비율 추정)
  - ResolvedCharStyle → RunProperty 어댑터 fn
- G-3-verify: 결과 line break / total_height 가 HWPX stored line_segs 와 일치하는지 비교
- G-3-allocation: glyph x position (현재 compose_layout output 의 CharItemView 가 위치 정보 미보유 — Layout::Allocate / Glyph::allocate_bounds 단계 audit 필요)

## G-3-real working (2026-05-18)

`render-engine/rust/examples/probe_g_real.rs` — 실제 HWPX 1 paragraph wire.

흐름:
1. `rhwp::parser::hwpx::parse_hwpx(data)` → Document
2. 첫 non-empty paragraph 선택 (math hwpx → "9.    두 상수 , 에 대하여 함수", 18 chars)
3. `paragraph.char_shapes[0].char_shape_id` → `doc_info.char_shapes[id].base_size` = 1350 HWPUNIT = **13.50pt**
4. `char_shape.font_ids[0]` (한글 lang) → `doc_info.font_faces[0][font_id].name` = **"함초롬바탕"**
5. 글자별 HFT advance lookup (한글 ratio 0.3~2.0 만 신뢰, ASCII 추정) → width
6. CharItemView::from_constructor_metrics + RunProperty(font_size=13.50)
7. ppt_compose_layout(comp, 1, Break(0, 17), -1, 0, output)

결과:
- output 24 items = pre 2 + null + main 18 + null + post 2 ✅
- CharItemView width sum = 160.92 pt = **16092 HWPUNIT**
- stored line_segs[0].segment_width = **31748 HWPUNIT** (column 너비 — 비교 의미 X)
- stored line_segs[0].line_height = **1350 HWPUNIT** = font_size 그대로
- 우리 line_height = **2160 HWPUNIT** (font_size * 1.2 * 96/72)

→ **flow 작동, 글자 width 채움. line_height multiplier (1.2 factor) 차이가 한컴 native 인지 다음 step 에서 검증 필요**.

## 다음 (G-3-verify)

`compute_metrics` 의 `line_height = font_size * 1.2 * dpi/72` 가 한컴 raw decompile 의 정확한 매핑인지 확인.
- option a) raw asm dump 재검토 (1.2 factor 의 의미)
- option b) RunProperty 의 LINE_HEIGHT key 가 있다면 그것이 우선?
- option c) px 단위 출력 자체가 잘못 — 한컴 HWPUNIT 그대로 유지해야?

## G-3-allocation audit (2026-05-18)

`work/hft_re/layout_re/decompiles_v2/` 안 Allocate vfunc 11 개:
- `Composition__Allocate_002fe7bc` (32 bytes — wrapper, parent.vfunc[4] tail-call)
- `CharItemView__Allocate_002f5d48`
- `ItemView__Allocate_003167cc`
- `MonoGlyph__Allocate_002d0584`
- `BlipGlyph__Allocate_002d13e8`
- `DebugGlyph__Allocate_0030b968`
- `WidgetGlyph__Allocate_0030b020`
- `PlaceCenter__Allocate_003307e4`
- `PlaceMargin__Allocate_00330dc0`
- `Placement__Allocate_003312b4`
- `Superpose__Allocate_00339850`

port 상태 (kdsnr-layout/src/glyph.rs + composition.rs):
- `Composition::allocate` = parent forward ✅
- `allocate_bounds` impl ✅ (Tile/Box/Glue 등 subclass)
- 단 실제 wire (compose_layout 후 → Layout::Allocate 호출 → glyph 의 final position 채움) 미구현

## 다음 세션 G phase 진입점

1. G-3-allocation wire: `ppt_compose_layout` 후 → Composition 의 children 에 대해 `Glyph::allocate_bounds(avail, out_bounds)` 호출 → 각 glyph 의 bbox 채움
2. avail = column width (예: 423 px), 결과 = 각 글자 의 x position
3. probe_g_real 확장 → bbox 출력
4. 우리 결과 vs HWPX stored line_segs[0].segment_width 비교
5. byte-eq 여부 검증

이게 G phase 의 milestone. 한 세션 분량.

## rhwp multi-column / footnote 한계 (2026-05-18)

korean S18-21 visible 분석: 우측 컬럼 children=7 (좌측 16). GT 는 우측 컬럼 가득 + footnote 영역. rhwp 의 multi-column flow 한컴과 다름 + footnote control 우리 split hwpx 에 누락. 이 모두 paginator 의 byte-eq port 영역 (G phase 의 부분).

## G-3-allocation wire 작동 (2026-05-18)

`render-engine/rust/examples/probe_g_real.rs` 확장 — `ppt_compose_layout` 후 각 CharItemView 에 누적 x 로 sub-Allocation 만들어 `CharItemView::allocate(alloc, &mut ext)` 호출 → bbox 산출.

흐름:
1. output 24 items 중 CharItemView 18 개 추출
2. `cursor_x = 0` 시작, 각 글자 별:
   - `alloc.x = Allotment(cursor_x, civ.width, 0.0)` (origin=begin)
   - `alloc.y = Allotment(baseline_y=0, civ.line_height, civ.vertical_anchor_ratio)`
   - `civ.allocate(&alloc, &mut ext)` → ext.{left,top,right,bottom}
   - `cursor_x += civ.width`

결과 (math hwpx 첫 paragraph, 18 chars, "9.<tab>두 상수 , 에 대하여 함수"):
```
[ 0] char='9'    cursor_x=  0.00  w= 6.75  ext=(  0.00, -16.20)-( 18.00,  5.40)
[ 1] char='.'    cursor_x=  6.75  w= 5.40  ext=(  6.75, -16.20)-( 24.75,  5.40)
[ 2] char='\t'   cursor_x= 12.15  w= 3.65  ext=( 12.15, -16.20)-( 30.15,  5.40)
[ 3] char='두'    cursor_x= 15.80  w=13.50  ext=( 15.80, -16.20)-( 33.79,  5.40)
...
[17] char='수'    cursor_x=147.42  w=13.50  ext=(147.42, -17.28)-(165.42,  4.32)

overall: width=16542 HWPUNIT, line_height=2268 HWPUNIT
```

핵심 관찰:
- **ext.right - cursor_x = 18.00pt = total_height (font cell)** ≠ width (advance).
  bbox = cell box, advance = next-x 결정. 한컴 spec 그대로.
- text advance sum = **160.92pt = 16092 HWPUNIT** (모든 width 합)
- stored line_segs[0].segment_width = **31748 HWPUNIT** (column 전체 너비)
  → 16092 만 텍스트, 나머지 15656 = 오른쪽 빈 공간 (line wrap 안 일어남, paragraph 한 줄)

⚠ **`line_height` discrepancy** (G-3-verify 작업):
- our line_height = **2160 HWPUNIT** (= font_size × 1.2 × 96/72 = 21.6pt)
- stored line_height = **1350 HWPUNIT** (= base_size 그대로, font_size × 1.0)
- 1.2 factor 가 한컴 `CharItemView::compute_metrics` raw 의 정확한 매핑인지 vs paragraph layout (PptCompositor) 단계에서 cancel 되는지 검증 필요

## 다음 (G-3-verify)

option a) `compute_metrics` raw asm 의 1.2 factor 의미 재검토 (paragraph 의 line_spacing? font_metric default?)
option b) PptCompositor::ComposeLayout 의 line_height 결정 흐름 추적 (CharItemView.line_height 가 paragraph_line_height 로 over-ride 되는지)
option c) stored line_height (= base_size) 가 한컴 paginator 의 line layout 출력인지 vs char metric 인지

검증 방법: 한컴 raw decompile `FUN_002ef798` line 243-297 (compute_metrics 호출부) + `PptCompositor::ComposeLayout` 의 line_height 결정 stage 비교.

## G-3-verify 첫 결과 (2026-05-18)

probe_g_real 에 ParaShape.line_spacing 출력 추가. math hwpx 첫 paragraph:

| 항목 | 값 |
|------|-----|
| font_size | 13.50pt (= 1350 HWPUNIT) |
| ParaShape.line_spacing | 165 (Percent) |
| ParaShape.line_spacing_v2 | 0 |
| stored line_segs[0].line_height | **1350** HWPUNIT (= base_size 그대로) |
| our CharItemView.line_height (compute_metrics) | **2160** HWPUNIT (= font_size × 1.2 × 96/72) |
| 165% 적용 시 예상 | 2227.5 HWPUNIT (1350 × 1.65) |

**결론**: stored `line_segs[i].line_height` 는 char cell 높이 (= base_size) 만 저장. paragraph 의 실제 줄간격은 `line_segs[i+1].vertical_pos − line_segs[i].vertical_pos` 로 표현. ParaShape.line_spacing 도 stored line_height 에 직접 안 들어감.

CharItemView 의 `+0x58 line_height = font_size × 1.2 × dpi/72` 는 raw `FUN_002ef798` line 243-297 의 byte-eq port 정확 — 그러나 이는 **char cell 의 line height** 일 뿐 paragraph line_seg.line_height 와 의미가 다름.

byte-eq 흐름 추정:
1. CharItemView 의 +0x58 (char cell line_height) 가 paragraph 의 line 단위 anchor 계산에 사용
2. line_seg 생성 시 PptCompositor 의 별도 stage 가 +0x58 을 stored line_height (= base 또는 다른 값) 로 변환
3. 줄간격 (165%) 는 line_seg 의 vertical_pos delta 로 적용

## 다음 (다음 세션 G phase 진입)

- G-3-verify-deep: `PptCompositor::ComposeLayout` 의 line_seg 생성 stage port — stored line_height (= base) 와 vertical_pos delta (= 165% × base) 의 byte-eq 산출
- 후보 raw: `PptCompositor::ComposeBreak` (`0x307d2c-0x308060`) 의 line wrap + line_seg 작성 부분
- 시간 가능하면 G-4 (SvgSurface dispatch) 부터 wire 해서 visible 결과 측정 후 G-3-verify-deep 로 돌아오기

## G-4 SvgSurface dispatch wire 작동 (2026-05-18)

`probe_g_real.rs` 에 SvgSurface dispatch 추가. milestone 도달:
**kdsnr-layout → SvgSurface → resvg PNG 의 e2e 흐름 작동 확인**.

흐름:
1. ppt_compose_layout output 의 CharItemView 18개
2. 각 글자: cursor_x_pt 누적 (kdsnr-layout 의 width 사용)
3. SvgSurface::draw_driver_string 시도 → HFT cache 에 "함초롬바탕" alias 없어 0 hit
4. fallback: SVG `<text font-family="HBatang">` 로 surface.buffer 에 직접 append (한컴 TTF 명시)
5. usvg + tiny_skia 로 PNG rasterize, fontdb 에 한컴 TTF 전체 로드

결과:
- emit: HFT path=0, SVG `<text>`=17
- SVG 2035 bytes
- PNG (1028×1489) 에 "9. 두 상수 , 에 대하여 함수" visible

영향:
- kdsnr-layout 의 layout 결과 → SVG/PNG 의 dispatch 흐름 검증 완료
- 글자 위치는 우리 width × cursor_x 누적 = 한컴 native advance 와 일치 안 함 (HFT alias 누락 + Hangul width 가 font_size 그대로 사용)
- visible 위치 정밀도는 향후 작업

## 다음 세션 G phase entry (refined)

| 단계 | 작업 | priority |
|------|------|----------|
| G-4-multi | probe → 전체 paragraph (page) 흐름. ParaShape / line wrap (compose_layout BreakType) / multi-column 통합 | high (visible 효과) |
| G-3-verify-deep | line_seg byte-eq port (line_height + vertical_pos delta) | high (byte-eq 정공법) |
| G-3-paginator | rhwp paginator → kdsnr-layout paginator port | huge |
| G-5 | wire_real_gt 대체 측정 vs GT | after G-4-multi |

## G-4-multi 작동 (2026-05-18)

`probe_g_multi.rs` 신규 — section 전체 (40 paragraph × 79 line_seg × 1587 글자) e2e visible PNG.

흐름:
1. 각 paragraph 의 각 line_seg [text_start .. next_text_start) 글자
2. kdsnr-layout 의 ppt_compose_layout + cursor_x_pt 누적
3. SvgSurface emit (HFT path 331 + SVG `<text>` fallback 1256)
4. baseline_y = (margin_top + ls.vertical_pos/100) × pt_to_px (rhwp 의 `paper_area.y + hwpunit_to_px(...)` 와 동일 공식)

결과 PNG (work/debug/probe_g_multi.png):
- ✅ 모든 line 의 y 위치 plausible (40 paragraph 가 페이지 상반 분포)
- ❌ glyph 겹침 + 박스 다수 — visible 깨짐

깨진 원인 분석:
1. **paper_area margin 가짜**: `margin_top_pt=50` fixed. 실제는 `page_def.margin_top` 사용 필요 (보통 1500 HWPUNIT)
2. **char_shapes char_pos 별 변동**: 한 paragraph 안 multi-shape (수식/볼드/한자) → 첫 shape 만 사용한 단순화 → font_size 차이 무시
3. **control chars**: 수식 mark/사진 mark (`\x05`, `\x06`, `\x07`, `\x00`) 도 일반 글자처럼 render 시도 → 박스
4. **HFT alias 함초롬바탕 등 후속 폰트 누락**: 1256/1587 = **79% SVG `<text>` fallback** (system font → advance 우리 cursor 와 매치 안 됨)

## 다음 세션 G phase entry (v2)

| 단계 | 작업 | priority |
|------|------|----------|
| G-4-multi-fix | (1) page_def.margin 정확 (2) char_shapes char_pos 적용 (3) control char skip (4) HFT alias 확장 | next |
| G-3-verify-deep | line_seg byte-eq port | high |
| G-3-paginator | rhwp paginator → kdsnr-layout paginator port | huge |
| G-5 | wire_real_gt 대체 측정 vs GT | after G-4-multi-fix |

## 2026-05-19 진단 — Q11/Q17 회귀의 직접 진입점

`work/tool_render_tree_diff.py` 로 4 케이스 진단 (자세히는 [[project-splitter-wrapper-vp-fix]]):
- Q04/Q05: render tree 100% / 99% matched, dy=0 → **회귀 아님** (rhwp 가 같게 그림)
- **Q11/Q17: matched 47%/54%, |dy|max=24.3px** — 진짜 layout 차이
  - 두 hwpx 의 rect 정의·폰트·lineseg.horzsize 동일
  - charPr ID 만 재할당 (ID 5 vs 8 둘 다 "신명 중명조" — 동일 폰트 다른 ID)
  - rhwp 가 두 hwpx 를 다르게 wrap → wrap 위치 / line height 차이
  - 결과: 말풍선 안 텍스트 마지막 줄이 box 바닥 넘침 (Q11)
  - rhwp 가 stderr 에 `LAYOUT_OVERFLOW_DRAW overflow=3.7px` 직접 출력

→ **fix layer = rhwp 의 line break / line height 결정 = `PptCompositor::ComposeBreak` (1520B asm) byte-eq port**.

## G-3-verify-deep entry 정리 (2026-05-19)

`work/hft_re/layout_re/decompiles_v2/` 안 핵심 4 decompile:

| 파일 | 줄수 | size | 역할 |
|---|---|---|---|
| `PptCompositor__ComposeBreak_00307af4.txt` | 332 | 1520B | **paragraph line break 결정** (Q11/Q17 root cause) |
| `PptCompositor__ComposeLayout_00308248.txt` | 1782 | — | paragraph 전체 layout 진입점 |
| `ColCompositor__ComposeBreak_0030590c.txt` | 318 | — | column 단위 break |
| `ColCompositor__ComposeLayout_00305d60.txt` | 534 | — | column 단위 layout |

signature (PptCompositor::ComposeBreak):
```cpp
ulong ComposeBreak(
    vector<float> const& positions,   // param_1
    vector<float> const& widths,      // param_2
    vector<float> const& heights,     // param_3
    vector<int>   const& indices,     // param_4
    vector<float> const& params,      // param_5
    Composition const* composition,   // param_6
    int line_index,                   // param_7
    int paragraph_index,              // param_8
    vector<int>& break_positions      // param_9 (output)
);
```

핵심 의존:
- `GetParaItemView(this, comp, para_idx)` → 이미 port (composition.rs)
- `IsFirstLineOnPara(comp, line_idx)` → 이미 port
- `GetFirstCharItemViewOnPara(comp, para_idx)` → 이미 port
- `BodyProperty::GetVert(body)` → 이미 port (L-5c-3c)
- `Hnc::Property::PropertyKey` ctor/dtor → 이미 사용
- PropertyKey 상수 `0x900`, `3.22719e-42`, `3.22999e-42` (= property keys) → BodyProperty 와 같은 키 family

## 다음 세션 entry (2026-05-19, refined)

**Step G-3-verify-deep-1**: PptCompositor::ComposeBreak byte-eq port 시작.
- 위치: `kdsnr-hwp-toolkit/layout-decoder/rust/src/composition.rs` 또는 신규 `ppt_compose_break.rs`
- raw asm 332 line decompile 1:1 port
- input/output 시그니처 fix
- test: probe binary 로 Q11 의 말풍선 paragraph 입력 → break positions 출력
- 검증: rhwp 의 현재 break positions 와 다른 점 확인 → 한컴 byte-eq 확보 시 wrap 결과 변경

이번 turn 후 1-2 세션 분량.

## 2026-05-19 — port 가 이미 작성됐고 wire 만 남았다

진입 audit 한 결과:

- **port 코드 존재**:
  - `layout-decoder/rust/src/ppt_compose_break.rs` — **1058 줄** (PptCompositor::ComposeBreak 1520B asm 의 1:1 port)
  - `layout-decoder/rust/src/ppt_compose_layout.rs` — **3726 줄** (PptCompositor::ComposeLayout 1782 line decompile 의 port)
  - `layout-decoder/rust/src/compose_break.rs` — 369 줄 (ColCompositor::ComposeBreak)
- **adapter 존재**:
  - `vendor/rhwp/src/renderer/kdsnr_bridge.rs` — rhwp IR ↔ kdsnr-layout 어댑터
  - `pub fn compose_line(civs, composition_type) -> CharItemContainer` 진입점
  - `pub fn run_to_civs`, `pub fn resolved_char_style_to_property`, `pub fn para_shape_to_property`, `pub fn default_horizontal_body_property` 등 conversion helper
- **wire 안 됨**:
  - grep `kdsnr_bridge|use.*kdsnr_bridge` in vendor/rhwp/src/ 결과 = `mod.rs:19:pub mod kdsnr_bridge;` 1 줄만. **호출처 0 개** = dead code

→ **G phase 의 진짜 빈 칸은 port 가 아니라 wire**. paragraph_layout.rs (3209 줄) 의 `layout_composed_paragraph` 함수의 line loop (line 905~) 에서 각 `comp_line.runs` 를 처리할 때 `kdsnr_bridge::compose_line` 호출해서 한컴 byte-eq glyph 위치를 받아 TextRun bbox 에 반영해야 함.

## G-W-1 PoC 시작 + 새 발견 (2026-05-19 늦은 시간)

진행:
1. `paragraph_layout.rs:982` 의 RHWP_KDSNR_STYLE probe 옆에 **RHWP_USE_KDSNR_LAYOUT env-gate** 추가
2. line-loop 안에서 `kdsnr_bridge::run_to_civs` + `compose_line(civs, 1)` 호출 + width dump
3. rhwp 측 `estimate_text_width` sum 도 dump (직접 비교)
4. `dump-render-tree` CLI 에 `doc.set_hft_cache(arc)` 추가 (export-svg 와 동등)

Q11 PoC 결과 (HFT cache 428339 글리프 로드):
| line | kdsnr_w | rhwp_text_w | 차이 |
|---|---|---|---|
| "이번 실험에서..." | 339.12px | 289.00px | **+17%** |
| "새롭게 생성되었다가..." | 331.68px | 284.00px | +17% |
| "①A\t②C..." | 208.43px | 357.00px | **-42%** (탭 처리 차이) |
| 일반적으로 한글 line | +14~17% kdsnr |

→ **진짜 root cause 확정**: `kdsnr_bridge::run_to_civs` 가 사용하는 `font_runtime_metrics::measure_char_advance_em` 이 `EmbeddedTextMeasurer::estimate_text_width` 와 다른 값을 반환. HFT cache 활성화해도 차이 그대로.

ppt_compose_layout 알고리즘 자체는 정확하나 input metric source 가 두 갈래로 어긋남. wire 가 효과 있으려면 두 source 통일 (또는 한컴 byte-eq metric 으로 둘 다 교체) 필요.

다음 step entry:
1. `measure_char_advance_em` (HFT + ttf-parser) vs `measure_char_width_embedded` (HANCOM_FONT_METRICS table) 두 path 의 차이 진단
2. 한컴 byte-eq metric = 어느 source?
   - 한컴 native: `CharItemView::compute_metrics` (이미 port, `FUN_002ef798` line 243-297)
   - 그 안에서 width 는 어떻게 가져옴? (font face metric ascender/descender 만 사용? glyph advance 는 외부?)
3. 한 source 로 통일 후 wire bbox 갱신

## 추가 시도 (2026-05-19 늦은 시간) — run_to_civs_with_raw

`vendor/rhwp/src/renderer/kdsnr_bridge.rs` 에 **`run_to_civs_with_raw`** 신설:
- raw face name 우선 (HFT lookup → "신명 중명조" 등)
- substituted name fallback (HFT 시도 다시)
- fontdb hmtx 최후 fallback
- half-width punctuation 보정 (0.5em 강제)
- HWPUNIT quantize (`px * 75 → int → /75`)

→ rhwp 의 `measure_char_width_embedded_with_raw` 와 거의 1:1 path.

`paragraph_layout.rs` 의 PoC 도 `rcs.raw_font_family` 전달하도록 변경. Q11 재측정:

| line | kdsnr_w (raw v2) | rhwp_text_w | 차이 |
|---|---|---|---|
| "이번 실험에서..." | 339.09 | 289.00 | 여전히 +17% |
| "11. 다음은..." | 389.87 | 343.00 | +14% |

→ quantize/raw_face/HFT 우선 적용해도 **여전히 동일 17% 차이**. 즉 차이의 원인은 raw vs substituted 가 아니거나, 또는 HFT cache 의 advance 결과가 fontdb 결과와 동일하나 estimate_text_width 가 다른 source (HANCOM_FONT_METRICS table) 를 우선 사용.

다음 turn 의 진짜 entry:
1. **HFT cache 의 advance_em 결과를 직접 dump** — 한글 글자 'A' 등의 em 값이 어떤지
2. `estimate_text_width` 의 char_width 함수의 path 추적 — 정확히 어떤 source 사용?
3. `font_metrics_data::measure_char_width_lookup` 에서 HANCOM_FONT_METRICS 사용 흐름
4. 두 함수의 path 가 같은데 결과 다르다면 caching 또는 quantize 차이

이번 turn 까지의 큰 milestone:
- PoC env-gate + compose_line wire 작동 (paragraph_layout 안 wire 진입점)
- dump-render-tree CLI 에 doc.set_hft_cache 통합
- run_to_civs_with_raw 신설 — rhwp path 와 거의 동등
- **font metric source 가 진짜 bottleneck 임을 정량으로 입증**

## 🎯 2026-05-19 — PATH_DIAG + ratio fix MILESTONE

### PATH_DIAG 결과 (Q11 한글 첫 글자)

`paragraph_layout.rs` 의 PoC 에 3 갈래 path 동시 측정 추가:
| 글자 | raw_face | sub_face | HFT(raw) | HFT(sub) | measure_em | etw_1px | font_size |
|---|---|---|---|---|---|---|---|
| '과' | "신명 신그래픽" | "HCR Batang" | **None** | **None** | 0.97 | 31.0 | 36.0 |
| '학' | "신명 중명조" | "HCR Batang" | **None** | **None** | 0.97 | 13.0 | 15.33 |
| '이' | "신명 중고딕" | "HCR Batang" | **None** | **None** | 0.97 | 10.0 | 12.0 |

→ **HFT cache lookup 둘 다 None** (한컴 office 폰트 face 가 HFT cache 의 face name 과 매핑 안 됨).
→ `measure_char_advance_em` = 0.97em (fontdb hmtx fallback).
→ `estimate_text_width` 결과 = 0.85em (15.33pt × 0.85 = 13px).
→ **두 source 가 0.97 vs 0.85 = 14% 차이** = 우리 측정 17% 와 매치.

### 진짜 root cause = ratio 누락

`text_measurement.rs::style_params` 에서 `ratio = style.ratio if > 0.0`. `estimate_text_width` 가 `char_width × ratio` 적용. 우리 PoC 의 `run_to_civs_with_raw` 는 ratio 적용 누락.

### Fix: `run_to_civs_with_raw_ratio` 신설

`vendor/rhwp/src/renderer/kdsnr_bridge.rs` 에 `char_ratio` + `letter_spacing` 인자 확장:
```rust
let raw_px = (adv_em as f32) * font_size_px * char_ratio + letter_spacing_px;
let hwp = (raw_px * 75.0) as i32;
civ.width = hwp as f32 / 75.0;
```

`paragraph_layout.rs` PoC 도 `rcs.ratio` / `rcs.letter_spacing` 전달.

### 결과 — Q11 width 거의 byte-eq

| line | kdsnr_w | rhwp_text_w | 차이 |
|---|---|---|---|
| "학생의 대화이다." | 101.01 | 101.00 | **+0.01px** |
| "이번 실험에서..." | 288.41 | 289.00 | -0.2% |
| "새롭게 생성되었다가..." | 283.65 | 284.00 | -0.1% |
| "이유는 무엇일까요?" | 88.79 | 89.00 | -0.2% |
| "①A\t②C..." | 208.43 | 357.00 | **여전히 -42%** (탭 처리 차이) |

→ **일반 텍스트 line 은 byte-eq 도달** (0.01-0.7% 미세 차이만). 탭 포함 line 만 별도 처리 필요.

### 다음 step (G-W-2): bbox 교체

이제 metric 일치 확정 → compose_line 결과 width 로 line 의 cursor_x 누적 교체 가능. paragraph_layout 의 TextRun 노드 생성 부분에서 bbox.x 갱신:
1. compose_line(civs, 1) 결과 본문 슬라이스 의 각 CharItemView 의 width 추출
2. line 의 cursor_x 를 그 width 합으로 갱신
3. TextRun 의 bbox.x 와 bbox.width 도 byte-eq 위치
4. tool_render_tree_diff.py 로 Q11 matched 47% → 90%+ 확인

이건 paragraph_layout 의 깊은 변경. 1 세션 분량.

## 🎯 2026-05-19 — G-W-2 wire 적용 + 새 root cause 발견

### G-W-2 적용 위치

`vendor/rhwp/src/renderer/layout/paragraph_layout.rs:1703` 의 `full_width = estimate_text_width(&run.text, &text_style)` 를 env-gate 로 분기:

```rust
// G-W-2 wire: env-gated byte-eq char advance (kdsnr_bridge::run_to_civs_with_raw_ratio).
#[cfg(not(target_arch = "wasm32"))]
let kdsnr_w: Option<f64> = if std::env::var("RHWP_USE_KDSNR_LAYOUT").is_ok() && !run.text.contains('\t') {
    /* ... run_to_civs_with_raw_ratio 호출 + width sum ... */
} else { None };
kdsnr_w.unwrap_or_else(|| estimate_text_width(&run.text, &text_style))
```

### Q11 측정

wire-OFF vs wire-ON 비교 (render tree diff):
- matched 150/150 (100%)
- **TextRun |dx|max = 47.0 → 18.5px** 감소 (bbox.x 가 우리 kdsnr_w 로 갱신)
- TextLine |dy| = 0 (line vertical 위치 불변)

ours-wire-ON vs hwpsaved (회귀 fix 측정):
- matched 71/150 = 47% (**그대로**)
- TextRun |dx|max = 18.5px, **|dy|max = 24.3px 그대로**
- TextLine |dy|max = 24.3px 그대로

### 새 root cause: line break / wrap 위치

→ G-W-2 (width wire) 는 작동하나 Q11/Q17 의 dy=24.3px 차이는 **line break/wrap 위치 결정** 단계에서 발생. paragraph_layout 가 받는 ComposedParagraph 의 line break 가 이미 결정된 상태 — 즉 `compose_paragraph` (`vendor/rhwp/src/renderer/composer.rs`) 또는 그 caller 가 한컴과 다른 line break 위치 결정.

진짜 Q11/Q17 fix = `compose_paragraph` 또는 `compose_section` 단계에 `kdsnr_layout::ppt_compose_break` 통합. line wrap 위치를 한컴 byte-eq 로.

### 다음 step (G-W-3): compose_break wire

`vendor/rhwp/src/renderer/composer.rs` 의 line break 결정 path 에 `ppt_compose_break` (1058줄 port) 통합. 한컴 line wrap 위치와 byte-eq. 한 세션 분량.

## 🎯 2026-05-19 — G-W-3 entry audit + 본격 wire blocker 식별

### reflow_line_segs / fill_lines flow

`vendor/rhwp/src/renderer/composer/line_breaking.rs`:
- **`reflow_line_segs(para, available_width_px, styles, dpi)`** (line 621, 1001 line module)
  - tokenize_paragraph → BreakToken[]
  - inline ctrl width 보정 (`\u{0002}` placeholder → 실제 width)
  - **fill_lines (line 348)** — 핵심 break decision
  - LineBreakResult[] → LineSeg[] 변환
- **`fill_lines(tokens, text_chars, available_width_px, indent_px, default_tab_width, korean_break_unit) -> Vec<LineBreakResult>`**
  - rhwp 자체 알고리즘: HWPUNIT lw 누적, `lw > eff_w(first)` 면 break
  - last_break_token_idx 추적 → 줄 머리/꼬리 금칙

### G-W-3 wire 의 4 blocker

| blocker | 설명 | 작업 분량 |
|---|---|---|
| (1) BreakToken ↔ CharItemView 변환 | rhwp 의 token (Text/Space/Tab/LineBreak/Inline) ↔ kdsnr 의 CharItemView seq. 한 token 이 여러 char | 1 step |
| (2) ParaProperty 보유한 Composition | `ppt_compose_break(composition: &dyn Glyph, ...)` → fVar28/30/31 (indent values) 가 필요. ParaProperty 의 key 0x901/0x8ff/0x900 (first-line indent / subsequent-line indent / line margin) | 1 step |
| (3) penalties 계산 | char-class penalty (0=normal, 1=break-friendly, 2=hard-break, -1000=non-break) — rhwp 의 BreakToken 종류 별 매핑 | small |
| (4) heights[] = column widths | line 별 available width (multi-line paragraph). rhwp 는 single avail. 단일값 broadcast 가능 | small |

### G-W-3 진입 plan (다음 1-2 세션)

**Step 1: 변환 helper**
- `kdsnr_bridge::tokens_to_civs(tokens, styles, dpi) -> (Vec<CharItemView>, Vec<i32>)` 신설
- BreakToken → CharItemView + penalty 동시 변환
- inline ctrl 의 byte-eq width 정확히

**Step 2: ParaProperty composition**
- `kdsnr_bridge::build_paragraph_composition(para_style, civs) -> CharItemContainer`
- ParaProperty 의 0x901/0x8ff/0x900 키 세팅

**Step 3: wire**
- `fill_lines` 직전에 env-gate 추가
- `ppt_compose_break(widths, _, _, penalties, heights, composition, 0, n-1) -> Vec<u32>`
- 결과 break positions → `Vec<LineBreakResult>` 변환

**Step 4: 검증**
- Q11/Q17 render tree diff: matched 47% → 90%+ 목표
- 일반 통과 케이스 (Q01/Q03 등) 회귀 없음 확인

총 1-2 세션 분량.

### 이번 turn 까지의 누적 milestone (2026-05-19)

- 도구 3개 + dump-render-tree CLI 신설
- Q04/Q05 회귀 아님, Q11/Q17 진짜 회귀 입증
- kdsnr_bridge wire 진입점 식별
- PATH_DIAG metric source 3갈래 진단 + ratio fix → width byte-eq
- **G-W-2 wire 적용 + 작동 입증** (TextRun |dx|max 47→18.5)
- **G-W-3 진입점 + 4 blocker 식별** (BreakToken 변환 / ParaProperty composition / penalties / heights)

## 🎯 2026-05-19 — G-W-3 PoC 시도 + paginate path 의 진짜 발견

### G-W-3 PoC 결과: fire 안 함

`reflow_line_segs` 의 fill_lines 직전에 env-gate + ppt_compose_break 호출 + dump 추가. 빌드 통과. 그러나 Q11 dump 시 KDSNR_BREAK 1 줄도 안 fire.

→ **`reflow_line_segs` 가 `dump-render-tree` / `export-svg` path 에서 호출 안 됨**.

### 진짜 paginate path 발견

`vendor/rhwp/src/renderer/composer.rs::compose_lines` (line 263):
- `para.line_segs.is_empty()` 면 1 line 전체로 처리
- 그 외 `line_segs.text_start` (UTF-16 위치) 로 line 의 텍스트 범위 분할

→ **paginate 가 hwpx 의 stored `<hp:lineseg textpos>` 를 그대로 trust**. reflow 는 `--reflow` flag 또는 빈 line_segs 일 때만 동작.

### Q11/Q17 root cause 재정의

이전 진단: rhwp 의 line break 알고리즘이 한컴과 다름.
**수정**: rhwp 는 line break 결정을 자체적으로 안 함. **splitter (kdsnr_hwp_toolkit) 가 emit 한 `<hp:lineseg textpos>` 가 곧 line break 위치**. Q11/Q17 의 line break 차이는 우리 splitter 가 한컴과 다른 textpos 를 emit 한 데서 옴.

### 두 갈래 fix path

| path | 작업 분량 | 정공법 |
|---|---|---|
| (A) splitter 의 enrich_linesegs 가 한컴 byte-eq textpos emit | huge (PyO3 binding for ppt_compose_break) | ✅ |
| (B) rhwp 의 compose_lines 에 env-gate + tokenize + ppt_compose_break → line_segs override | medium | ✅ |

(B) 가 더 빠른 milestone. (A) 가 splitter 의 진짜 fix (PNG renderer 와 무관). 둘 다 valid.

### 다음 turn entry (수정)

**Step G-W-3a (수정)**: `compose_lines` (composer.rs:263) 에 env-gate 추가.
- env-on 이면 `para.line_segs` 무시
- `tokenize_paragraph` + 우리 byte-eq char widths + `ppt_compose_break` 호출
- 결과 break positions → 가상 line_segs 만들어 compose_lines 진행
- 빈 ParaProperty composition 으로 시작 → 효과 측정 → ParaProperty 추가

이게 진짜 wire 진입점. 1-2 세션.

### 누적 진전 metric

- width wire (G-W-2): TextRun |dx|max 47→18.5 (Q11 matched 47% 그대로)
- **line break wire (G-W-3a)**: 진짜 fix path 확정, 다음 turn 시작

## 🎯 2026-05-19 — G-W-3a wire 적용 + 실제 효과 입증

### wire 구현

1. **`composer.rs` 에 helper 신설**:
   - `compose_paragraph_with_reflow(para, styles, dpi) -> ComposedParagraph`
     - env-gate `RHWP_USE_KDSNR_LAYOUT=1` 시 paragraph clone → `reflow_line_segs` 강제 호출
     - avail_width 는 `para.line_segs[0].segment_width` 에서 자동 추출 (HWPUNIT → px)
   - `compose_section_with_reflow(section, styles, dpi)` 도 추가
2. **wire 위치**: `document_core/commands/document.rs:70` 의 `from_bytes` 안 첫 compose_section 호출
3. **`reflow_line_segs::fill_lines` 직전에 KDSNR_BREAK PoC** (이전 turn 의 코드) — env-on 시 widths/penalties 추출 + `ppt_compose_break` 호출 + 결과 dump (line break 적용은 아직 안 함, 측정만)

### Q11 측정 결과

| metric | G-W-2 만 (wire-OFF) | G-W-3a wire-ON | 변화 |
|---|---|---|---|
| matched | 71/150 (47%) | 71/150 (47%) | 그대로 |
| TextRun \|dy\|max | 24.3 | **12.3** | **절반 감소 ✓** |
| TextLine \|dy\|max | 24.3 | **19.4** | 작아짐 ✓ |
| TextRun avg dy | -1.2 | -4.9 | 분포 변화 |
| TextRun \|dx\|max | 18.5 | 18.5 | 동일 (G-W-2 효과) |

→ **line break wire 가 line vertical 위치 실제 변화 시킴**. 한컴 byte-eq 에 절반 가까워짐.

### 잔존 dy=12.3px 의 원인

1. **ppt_compose_break 가 dummy composition 사용** — `CharItemContainer::new()` (빈)
   - ParaProperty 없음 → phase 1 indent (fVar28/30/31) 모두 0
   - 첫 줄 indent 차이 = dy 잔존의 일부
2. **break positions 가 line_seg 에 적용 안 됨** — KDSNR_BREAK 는 dump 만
   - reflow_line_segs::fill_lines 는 자체 알고리즘으로 break positions 만들고 line_segs 갱신
   - 즉 우리 wire 효과 = reflow_line_segs 호출 자체 (rhwp 자체 알고리즘으로 stored line_segs 덮음)
3. **cell paragraph 가 reflow 안 거침** — `segment_width=0` 인 경우 skip
   - Q11 의 말풍선 안 paragraph 가 cell paragraph → stored line_segs 그대로 사용

### 다음 turn entry (G-W-3b)

1. **ParaProperty 보유 composition 만들기** — `kdsnr_bridge::build_para_composition(para_style)` 신설
   - key 0x901 (first-line indent), 0x8ff (subsequent-line indent), 0x900 (line margin) 세팅
2. **KDSNR_BREAK 결과를 fill_lines 결과 대체** — 실제 byte-eq line break positions 적용
3. **cell paragraph 도 reflow** — segment_width=0 일 때 cell width 추정 (cell.width 또는 paragraph 자체에서)
4. Q11/Q17 render_tree_diff 재측 → matched 47% → 90%+ 목표

### 누적 milestone (2026-05-19)

- 도구 3개 + dump-render-tree CLI 신설
- 잘못된 enrich_linesegs wrapper-vp fix 롤백
- Q04/Q05 회귀 아님, Q11/Q17 진짜 회귀 입증
- PATH_DIAG metric source 3갈래 진단
- ratio fix → width byte-eq (97-99%)
- G-W-2 wire (width) → TextRun |dx|max 47→18.5
- **G-W-3a wire (line break, partial) → TextRun |dy|max 24.3→12.3 (절반)**
- 다음 turn = G-W-3b (ParaProperty composition + cell paragraph + 실제 byte-eq break apply)

### 이번 turn 까지의 누적 milestone

- 도구 3개 + dump-render-tree CLI 신설 (이전 turn)
- Q04/Q05 회귀 아님, Q11/Q17 진짜 회귀 입증 (이전 turn)
- kdsnr_bridge wire 진입점 식별 + PoC env-gate 작동 (이전 turn)
- PATH_DIAG metric source 3갈래 진단 + ratio fix (이번 turn)
- **G-W-2 wire 적용 + 작동 입증 (이번 turn)**
- **새 root cause = line break/wrap 단계 확정 (이번 turn)**

## kdsnr-hft 후속 개선 (사용자 제안)

HFT cache 의 face name 이 "신명 중명조" / "HCR Batang" 어떤 것과도 매치 안 됨 → kdsnr_hft_global::advance_em 항상 None. 한컴 office 의 hftinfo.dat 의 alias 가 HFT cache 로딩 시 적용 안 됨. 또는 lookup 시 hftinfo.dat 의 face_name → HFT 의 internal face_id 매핑 누락.

별도 작업: `kdsnr_hft::HftCache::load_dir` 에 hftinfo.dat alias 통합. 현재 ratio fix 만으로 wire 가능하므로 G-W-2 진행 후 측정 결과 보고 결정.

## G phase 다음 step (재정의, 2026-05-19)

**Step G-W-1: paragraph_layout 의 line-loop 에 compose_line wire**

scope:
- `vendor/rhwp/src/renderer/layout/paragraph_layout.rs::layout_composed_paragraph` 의 905-line 시작 `for line_idx in start_line..end` loop
- 각 line 처리에서:
  1. `comp_line.runs` 각 run → `kdsnr_bridge::run_to_civs(run)` → `Vec<CharItemView>` 생성
  2. line 의 모든 run concat → `compose_line(civs, 1)` 호출
  3. 결과 CharItemContainer 의 본문 슬라이스 (`[2..-2]`) 의 각 CharItemView 의 width / line_height 추출
  4. 그 값으로 TextRun 의 bbox.x (누적), bbox.w (advance), 그리고 line 의 y delta (line_height) 계산
- 첫 시도는 환경변수 gate (예: `RHWP_USE_KDSNR_LAYOUT=1`) 로 toggle 가능하게 → A/B 검증 용이

검증:
- Q04/Q05 render tree dy=0 유지 (이미 정상)
- Q11/Q17 render tree matched 율 47% → 90%+ 향상 기대
- 변화 측정: `tool_render_tree_diff.py` 로 wire-ON vs wire-OFF 비교
- pixel 차이 측정: `tool_pixel_diff_strong.py` 로 hwpsaved 재렌더 vs 우리 PNG

예상 분량: 1 세션. 단 compose_line 의 input/output 정확성 확보 (특히 RunProperty FONT_SIZE 단위, line_height 변환) 가 한계 — 못 맞으면 sub-step.

## 잘 잡힌 진단 = 도구 의존

이번 진단의 핵심 사실들은 모두 `dump-render-tree` + `tool_render_tree_diff.py` 가 입증.
- Q04/Q05: 회귀 아님 (render tree 동일)
- Q11/Q17: 진짜 회귀 (matched 47-54%, dy=24.3px)
- enrich_linesegs wrapper-vp fix: PNG 0 영향 (md5 동일)
- kdsnr_bridge: dead code (호출처 0개)

앞으로 모든 fix 시도 = 이 도구들로 사전 검증 + 사후 측정. hwpx XML 또는 코드만 보고 "fix 됐다" 판단 금지.
