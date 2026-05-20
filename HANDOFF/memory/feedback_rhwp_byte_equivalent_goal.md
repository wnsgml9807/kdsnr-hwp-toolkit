---
name: pixel-equivalent-svg-maximal-byte-eq-logic
description: "⭐ 최종 목표: 우리 SVG 출력을 한컴 출력과 픽셀 단위 100% 일치. 모든 layout/render/parser/paginator 로직은 한컴 byte-eq 1:1 port. SVG export 는 rhwp svg.rs (2830줄, 한컴 spec 1:1) 사용 — 별도 SvgSurface 어댑터 port 불필요. R-2~R-4/R-6 SKIP."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# 목표 정의 (2026-05-17 갱신, Option B 선회 + rhwp SVG 활용 정정)

## ⭐ 최종 목표

**우리 SVG 출력 ≡ 한컴 출력 (픽셀 단위 100% 일치)**.

- 출력 포맷: **SVG** (PDF 가 아님)
- 비교 방법: 우리 SVG → PNG 렌더 vs 한컴 출력 → PNG 렌더 → ImageMagick `compare` → **diff pixel = 0**
- 검증 대상: toolkit input sample 파일들 (templet/original 9 + work/e2e/ 106 + work/0513 등 = 307 hwpx)

**byte-equivalent 가 아닌 이유**: SVG/PDF 내부 byte 구조 (객체 ID 순서, 압축 옵션, stream 구조, font subset 형식) 는 visual 에 무관. 거기까지 byte-eq 추적은 ROI 0.

**pixel-equivalent 가 시각 무관 byte-eq 보다 강한 이유**: 글리프 좌표, fill 색, path 모양, z-order, font glyph shape 같은 **시각 결정 요소는 모두 byte-eq 보장**. 같은 logic → 같은 좌표/색/모양 → 같은 픽셀.

## Why

- **사용자 핵심 use case**: KSAT 문항지 자동 생성. 한컴 GT 와 시각 비교 검증 / OCR 자동화 / dataset 라벨링이 픽셀 단위 일치를 요구.
- **휴리스틱 도입 시 누적 부정합**: "이 정도면 충분" 은 edge case 에서 깨짐 → 사용 불가.
- **byte-eq 아닌 visual-eq 만 추구하면**: 우리 custom logic 비중이 늘어남 → 견고함 약화. pixel-eq 는 visual-eq 와 동일한 결과를 byte-eq logic 위에서 얻음.

## How to apply

### byte-eq 대상 (한컴 1:1 port, 휴리스틱 0%)

1. **HWP/HWPX parser**: `libHncDoc.dylib` (또는 해당 파서 dylib) 의 파싱 함수 → Ghidra RE → Rust 1:1 port → `kdsnr-parser` crate.
2. **Layout (조판)**: `libHncDrawingEngine.dylib::Hnc::Shape::Text::*` — Composition/Compositor/Tile/Align/Box/Glyph 의 Request/Allocate (vfunc 3-4) — 진행 중 `kdsnr-layout` (24K줄, 516 tests).
3. **Render 데이터**: 같은 dylib 의 Theme/Color/ColorScheme/FormatScheme/FontSet/ObjectDefaults — 진행 중 `kdsnr-render` (23K줄, 609 tests).
4. **Glyph Draw vfunc 5-8**: Draw/Pick/Bounds/Undraw — `kdsnr-layout` 의 R-5 단계 (미시작, 3-5 세션 추정).
5. **Pagination**: `Hnc::Shape::Page::*` (또는 해당 네임스페이스) 의 페이지 분할 + measurement — 신규 `kdsnr-paginator` crate.
6. **Font (HFT)**: `kdsnr-hft` 완료 (6 family ✓).

### SVG export adapter = 직접 작성 필요 (rhwp svg.rs 그대로 안 됨) ⭐

**사용자 실측 (2026-05-17)**: rhwp 출력 ≠ 한컴 출력. rhwp svg.rs (`vendor/rhwp/src/
renderer/svg.rs`, 2830줄) 는 rhwp 자체의 SVG 백엔드일 뿐 한컴 pixel-eq 보장 없음.
자세한 내용은 [[reference_rhwp_svg_renderer]] 참조.

**우리 SVG export adapter 작성** (~500-2000 LOC 추정):
- 우리 byte-eq layout 의 출력 (CharItemView::Draw 의 path / brush / pen / underline 등)
  → SVG primitive (`<path>` / `<rect>` / `<line>` / `<image>` / `<defs>`) emit
- font 출력: HFT decoder ([[reference_hft_decoder_complete]]) 로 glyph path 추출 후
  `<path>` emit (NOT `<text>` element — pixel-eq 위해)
- 한컴의 좌표/색/모양 결정 → SVG 좌표/색/모양 mechanical 1:1 변환

**rhwp svg.rs 는 mechanical mapping 참고용으로만**:
- font subset extraction logic
- @font-face local() 처리
- 그라데이션/클립/패턴 defs 의 SVG 변환 패턴
- defs ID 중복 방지

`kdsnr-render` crate 의 `svg_surface.rs` 가 우리 어댑터의 출발점 — 확장 필요.

### SKIP 대상 (visual 무관)

| 영역 | 원래 계획 | SKIP 이유 |
|---|---|---|
| R-2 | Surface 8 ctor (GDI+ Graphics / HDC / HWND / 등) | Windows API target. 우리는 SVG 1 target |
| R-3 | Surface 100+ method GDI+ 호출 sequence | byte-eq GDI 호출 시퀀스가 visual 무관. logic 만 byte-eq |
| R-4 | libhsp GDI shim → CoreGraphics | Mac 전용 출력. svg2pdf 사용 |
| R-6 | 한컴 PDF writer (PDFKit/자체) | svg2pdf 가 SVG → PDF 변환 담당 |

이 4 영역 SKIP 으로 **~13-22 세션 절감**.

### 검증 방법 (pixel-diff harness, Stage 5)

1. **우리 출력**: kdsnr-render layout → rhwp PaintOp emit → rhwp svg.rs → **SVG**
2. **한컴 출력**: 한컴 GT (PDF 또는 직접 SVG export 가능 시)
3. **비교**: 둘 다 같은 viewer/DPI 로 PNG 렌더 → ImageMagick `compare` → **diff pixel = 0**
4. **input sample**: toolkit input 파일들 (templet/original 9 + work/e2e/ 106 + work/0513 등 = 307 hwpx). 자세한 sample 정책은 [[reference_toolkit_input_originals]] 참조.
5. **차이 발견 시**: 어느 byte-eq layer 가 mismatch 인지 추적 → 그 함수 raw asm 재정독 → port 수정.
6. **HWPX `<hp:linesegarray>` 비교**: 우리 `kdsnr-layout` 산출 linesegarray vs HWPX stored linesegarray byte 일치 (B-port 보조 검증).

### rhwp 의 운명

[project_rhwp_architecture.md] / [project_rhwp_dual_root_causes.md] / [project_rhwp_placeholder_lineseg.md] 등의 rhwp 패치는 **interim 자산**:
- kdsnr-parser/paginator 가 완성되기 전 까지 rhwp 의 parser/paginator 사용
- 완성 후 rhwp 전체 stack 폐기 (interim 패치들도 폐기)
- 최종 구조: `kdsnr-parser` → `kdsnr-layout` + `kdsnr-render` → SvgSurface → svg2pdf

## 관련 정책

- [정공법·완벽 구현](feedback_no_time_optimization.md): "이정도면 충분" 휴리스틱 금지 — 본 정책의 핵심
- [rhwp 정면돌파](feedback_rhwp_frontal_assault.md): rhwp 책임 입증 시 직접 패치 (interim 단계용)
- [HFT toolkit isolation](feedback_hft_toolkit_isolation.md): HFT 결과물은 toolkit 내부 sub-module
- [probe 기반 사실 확인](feedback_probe_driven.md): 추측 금지 (Ghidra decompile / raw asm 만 신뢰)
- [full byte-eq plan](project_full_byteeq_plan.md): 새 전략의 phase 별 plan

## 본 정책 갱신 이력

- 2026-05-14 ~ 2026-05-16: byte-eq PDF output 목표. R-2~R-6 까지 모두 byte-eq port 계획.
- **2026-05-17 (이 갱신)**: pixel-eq output 으로 선회. byte-eq 는 visual 결정 logic 까지만. R-2~R-4/R-6 SKIP. SvgSurface 어댑터로 SVG 출력 후 svg2pdf.
