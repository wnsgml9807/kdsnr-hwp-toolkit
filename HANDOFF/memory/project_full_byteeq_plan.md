---
name: full-byte-eq-pipeline-plan-pixel-eq-option-b
description: 한컴 PDF 와 pixel-equivalent 출력을 위해 parser/layout/render/paginator 전부 byte-eq port. SvgSurface 어댑터 (200-400줄) 만 custom. R-2~R-4/R-6 SKIP. 2026-05-17 선회 결정.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# Full byte-eq pipeline plan (Option B)

목표: kdsnr-hwp-toolkit 이 한컴 PDF 와 **pixel-equivalent** 출력. 그 안의 모든 logic 은 한컴 byte-eq port, 외부 의존성은 SvgSurface 어댑터 + svg2pdf 만.

## 최종 아키텍처

```
HWP/HWPX bytes
   ↓
[kdsnr-parser]  ← byte-eq port from Hancom HWP/HWPX parser (신규)
   ↓
Document IR (Hancom 의 내부 IR 과 byte-eq)
   ↓
[kdsnr-render 데이터 layer]  ← byte-eq, 진행 중 (Theme/Color/FormatScheme 등)
   ↓
ResolvedStyle
   ↓
[kdsnr-layout]  ← byte-eq, 진행 중 (Composition/Compositor/Tile/Align)
   ↓
PositionedElements (linesegarray + glyph positions)
   ↓
[kdsnr-paginator]  ← byte-eq port from Hancom page breaker (신규)
   ↓
PaginatedDocument
   ↓
[Glyph::Draw vfunc 5-8]  ← byte-eq, 미시작 (R-5)
   ↓ Surface API 호출
[SvgSurface adapter]  ← 우리 custom (200-400줄, 유일한 non-byte-eq)
   ↓ SVG primitive emit
SVG string
   ↓
[svg2pdf 0.13]  ← 외부 library
   ↓
PDF bytes (Hancom 과 byte 다르지만 pixel 동일)
```

## Phase 분류

### Phase L (Layout — kdsnr-layout, 진행 중)

| Sub | 작업 | 현재 | 남은 | 세션 |
|-----|------|------|------|------|
| L-1 | 17 Glyph vfunc 3-4 (Request/Allocate) | ✅ 완료 | - | 0 |
| L-2 | LayoutFactory 50+ Create* method | △ HBox/VBox 만 | 50 | 3-4 |
| L-3 | ParaProperty/RunProperty/BodyProperty 225 getter/setter | △ partial | 대부분 | 2-3 |
| L-4 | Composition::DecideBreaks/Update/잔여 25 method | △ core 만 | 25 | 1-2 |
| L-5 | Glyph vfunc 5-8 (Draw/Pick/Bounds/Undraw) — Surface API 호출만 emit | ❌ 0% | 17 Glyph × 4 vfunc = 68 | **3-5** |
| L-6 | 잔여 Glyph 클래스 (PictureBullet/BulletRender 의 Draw 등) | △ shell | 일부 | 1-2 |
| **L 합계** | | | | **10-16** |

### Phase R (Render — kdsnr-render, 진행 중)

| Sub | 작업 | 현재 | 남은 | 세션 |
|-----|------|------|------|------|
| R-1 | Flag/BWMode/Hit/Theme types | ✅ 완료 | - | 0 |
| R-1.5 | Color/ColorScheme/FormatScheme/FontSet/ObjectDefaults | ✅ 거의 끝 (609 tests) | 일부 | 0 |
| R-1.6 | Brush 6 sub-type inner (Solid/Hatch/Gradient/Picture/Group/Blip) | △ EmptyBrush 만 | 6 sub-type | 2-3 |
| R-1.7 | OuterShadow/Reflection PropertyBag full state | △ shell | full | 1 |
| R-1.8 | ObjectDefaults::CreateDefault (DefaultProperty vtable 종속) | △ shell | full | 1-2 |
| R-1.9 | Pen 7 PEnum sub-class 분리 | △ vtable addr | sub-class impl | 0.5 |
| R-1.10 | FontSet copy ctor | △ partial | full | 0.5 |
| R-1.11 | std::map RB-tree 잔여 (ColorScheme 외) | △ | 잔여 | 1 |
| ~~R-2~~ | ~~Surface 8 ctor (Windows GDI+ 추상화)~~ | **SKIP** | - | - |
| ~~R-3~~ | ~~Surface 100+ method byte-eq~~ | **SKIP** | - | - |
| ~~R-4~~ | ~~libhsp GDI shim → CoreGraphics~~ | **SKIP** | - | - |
| ~~R-6~~ | ~~한컴 PDF writer (PDFKit) byte-eq~~ | **SKIP** | - | - |
| **R 합계** | | | | **6-8** |

### Phase P (Parser — kdsnr-parser, 신규)

신규 crate `kdsnr-hwp-toolkit/parser/rust/`. 한컴의 HWP binary + HWPX 파싱 함수를 Ghidra RE → 1:1 port.

| Sub | 작업 | 세션 |
|-----|------|------|
| P-A1 | dylib 식별 (libHncDoc / libHncOfficeFramework / ?) — 어디에 HWP/HWPX 파싱 있나 | 0.5 |
| P-A2 | HWP binary 파서 함수 enumerate (Ghidra symbol tree) | 1 |
| P-A3 | HWPX (XML) 파서 함수 enumerate | 1 |
| P-A4 | Document IR struct sizeof + layout audit | 1-2 |
| P-B1 | HWP binary parser 1:1 port (header / docinfo / bodytext / sections) | 4-6 |
| P-B2 | HWPX parser 1:1 port (XML → 동일 Document IR) | 2-3 |
| P-B3 | 두 파서가 산출하는 Document IR byte-eq 검증 (round-trip + 한컴 IR dump 비교) | 1-2 |
| **P 합계** | | **10-15** |

### Phase G (Paginator — kdsnr-paginator, 신규)

신규 crate. 한컴의 paginate 함수 (`Hnc::Shape::Page::*` 또는 해당) RE → port.

| Sub | 작업 | 세션 |
|-----|------|------|
| G-A | dylib + 함수 enumerate | 1 |
| G-B | page break decision logic 1:1 port | 2-3 |
| G-C | header/footer/page number 처리 1:1 port | 1-2 |
| G-D | section/column break 1:1 port | 1-2 |
| **G 합계** | | **5-8** |

### Phase S (SvgSurface adapter — custom)

신규 crate `kdsnr-hwp-toolkit/svg-surface/rust/` 또는 `render-engine/rust/src/svg_surface.rs`.

| Sub | 작업 | 세션 |
|-----|------|------|
| S-1 | Surface trait 정의 (한컴 API 시그니처 byte-eq) | 0.5 |
| S-2 | SvgSurface 구현 (각 method → SVG primitive emit) | 1-2 |
| S-3 | DrawText → HFT glyph path emit 통합 | 1 |
| S-4 | Transform / Clip / Image 처리 | 1 |
| **S 합계** | | **3-4.5** |

### Phase I (Integration + Validation)

| Sub | 작업 | 세션 |
|-----|------|------|
| I-1 | 새 pipeline crate (`kdsnr-pipeline`) — Parser → Render → Layout → Paginator → SvgSurface | 1-2 |
| I-2 | pixel-diff harness (한컴 PDF + 우리 PDF → poppler PNG → ImageMagick compare) | 1-2 |
| I-3 | e2e 12 sample iteration — diff 발견 → root cause → 어느 byte-eq layer 수정 | 3-5 |
| I-4 | HFT 1-폰트 PoC → 387 폰트 scale | 2-3 |
| **I 합계** | | **7-12** |

## 총 세션 추정

```
Phase L (layout 보강)        : 10-16
Phase R (render 보강)        :  6-8
Phase P (parser 신규)        : 10-15
Phase G (paginator 신규)     :  5-8
Phase S (SvgSurface)         :  3-4.5
Phase I (통합 + 검증)        :  7-12
                              ─────────
Total                        : 41-63 세션
```

## 진행 순서 (사용자 합의 시 권장)

> **정책 (2026-05-17 결정)**: 평가 harness (Phase I-2/I-3) 는 **맨 마지막**. 그 전엔 byte-eq port 의 정확성을 그 자체로 신뢰 (raw decompile 1:1 = 정의상 정답). 휴리스틱·visual tweak 도입 금지 (=evaluation 가설로 코드 짜는 짓 자체를 차단).
> [eval-harness-last 정책](feedback_eval_harness_last.md) 참조.

### Stage 1 (1-2 세션) — 밑작업
1. ✅ **Phase P-A1** — parser dylib 식별 (libMajorDocGroup + libXMLDocGroup)
2. ✅ **Phase S-1** — Surface trait 정의
3. ✅ **Phase S-2** — SvgSurface Fill/Outline/Transform/Pie/Clip 구현

### Stage 2 (3-5 세션) — Surface backend + interim wire
1. ✅ **Phase S-3** (2026-05-17) — DrawString/DrawDriverString/MeasureString 5 method 구현. kdsnr-hft path dep + HftCache integration. `<path>` only (`<text>` 절대 금지). em-coord → SVG y-flip matrix. 18 신규 tests, **render 655 pass**
2. ✅ **Phase L-5a** (2026-05-17) — Glyph trait draw/undraw/get_bounds/pick 시그니처 raw 1:1 확장 + 14 method 정공법 port:
   - Glyph base 4 (Draw=ret/Undraw=container traversal/GetBounds=zero/Pick=false)
   - MonoGlyph (=Placement vtable 0x781168) 4 (Draw/Undraw/GetBounds/Pick = CalcPlacement+forward, Undraw 만 direct forward)
   - Box::Undraw (children Vec 순회)
   - DebugGlyph::Undraw 는 ZST 모델 limitation 으로 base default 와 동등 (DebugGlyph layout audit 후속)
   - kdsnr-render path dep 추가. bullet_render ripple 처리. 15 신규 tests, **layout 531 pass**
3. **Phase L-5b** (별도 세션, prerequisite tree) — Box::Draw/GetBounds/Pick (`FUN_002e6120` setup helper ~500B + Compositor::Allocate prerequisite) + CharItemView::Draw 묶음 (Brush/Pen/UnderLine/Direct/Diagnostics, Render::Path 17 ctor + Theme cache + PropertyBag effects prerequisite) + BlipGlyph::Draw + WidgetGlyph::Draw + DebugGlyph::Draw
4. **rhwp interim wire** — kdsnr-layout 을 rhwp 의 composer 자리에 임시 wire (rhwp 의 parser 는 interim 사용)

### Stage 3 (10-20 세션) — 큰 chunk port
1. **Phase P-A2/A3/A4** — Ghidra HWP/HWPX 파서 함수 enumerate + IR audit
2. **Phase P-B1/B2** — kdsnr-parser 본격 port (rhwp 의 parser 폐기 준비)
3. **Phase L-2/L-3/L-4** — layout 잔여 chunks
4. **Phase R-1.6~R-1.11** — render 잔여 chunks
5. **Phase G** — kdsnr-paginator port

### Stage 4 (5-10 세션) — 완성 + 평가
1. **Phase I-1** — 새 kdsnr-pipeline crate 로 통째 wire (rhwp 완전 폐기)
2. **Phase I-2** — pixel-diff harness 구축 (poppler + ImageMagick + work/GT/ 12개 한컴 PDF reference)
3. **Phase I-3** — pixel diff 측정 → root cause → byte-eq layer 수정 반복 → diff 0
4. **Phase I-4** — HFT 387 폰트 scale

## 관련 정책

- [pixel-eq + maximal byte-eq](feedback_rhwp_byte_equivalent_goal.md) — 본 plan 의 정책 근거
- [정공법·완벽 구현](feedback_no_time_optimization.md) — 휴리스틱 금지
- [HFT toolkit isolation](feedback_hft_toolkit_isolation.md) — HFT 결과물 위치

## 관련 메모리

- [layout RE 상태](project_layout_re_state.md) — Phase L 의 현재 상태 (16-μ session)
- [rendering phase grand plan](project_rendering_phase_plan.md) — 갱신 대상 (R-2~R-4/R-6 SKIP 반영)
- [Composition port 상세](project_composition_port_state.md) — Phase L 진행 상태
- [rhwp 렌더 파이프라인](project_rhwp_architecture.md) — interim rhwp 의 구조 (Stage 2 까지 사용)
- [e2e 검증 세트](project_e2e_validation_set.md) — Phase I 의 input
