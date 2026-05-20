---
name: rhwp SvgRenderer — 참고용만 (한컴 1:1 아님)
description: vendor/rhwp/src/renderer/svg.rs 2830줄은 rhwp 자체 SVG 백엔드일 뿐 한컴 출력과 픽셀 일치 보장 없음. 사용자 실측 확인: rhwp 출력 ≠ 한컴 출력. 우리 byte-eq layout 의 결과 → SVG emit 어댑터는 직접 작성 필요. rhwp svg.rs 는 mechanical mapping 참고용으로만 사용 (font embed, gradient defs 등).
metadata:
  type: reference
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# rhwp SvgRenderer (2026-05-17 정정)

**위치**: `kdsnr-hwp-toolkit/vendor/rhwp/src/renderer/svg.rs` (2830줄)

## ⚠️ 정정 (2026-05-17)

이전 메모리에 "한컴 spec 1:1" 이라고 적었던 것은 **잘못된 단순화**. 실제로는:

- rhwp svg.rs 는 **rhwp 자체** 의 SVG 백엔드 (rhwp layout 결과 → SVG 변환).
- **사용자 실측 확인**: rhwp 출력 ≠ 한컴 출력. 한컴과 픽셀 단위 일치 보장 없음.
- 우리가 byte-eq port (kdsnr-render) 하는 이유 = rhwp 의 parser/layout/renderer 가
  한컴과 다른 출력을 내기 때문.
- 따라서 **SVG export 도 rhwp 거 그대로 사용 안 됨** — 우리 byte-eq layout 결과를
  SVG 로 emit 할 때는 우리 자체 어댑터가 정공법.

## rhwp svg.rs 의 가치 (참고용만)

✅ 다음은 **mechanical mapping 참고**용:
- 4 FontEmbedMode: None / Style (@font-face local()) / Subset (base64) / Full (base64)
- 그라데이션 defs / 클립 patterns / 화살표 marker / 이미지 효과 filter 의 SVG 변환 패턴
- font subset extraction logic
- defs 내 중복 방지 ID 관리

❌ 다음은 **그대로 쓰면 우리 byte-eq 결과가 깨짐** — 우리 어댑터에서 직접 작성:
- 텍스트 layout → SVG path emit (glyph 좌표 결정 logic)
- shape transform 변환 (좌표계, anchor, rotation)
- 페이지 절대 좌표 매핑 (linesegarray 위치 환산)

## 동급 자매 백엔드 (rhwp 자체)

- `svg.rs` — SVG 출력 (본 reference)
- `svg_layer.rs` (335줄), `svg_fragment.rs` (278줄) — partial/layer SVG export
- `pdf.rs` (215줄) — PDF (svg2pdf 활용)
- `canvas.rs` (739줄) — HTML5 Canvas
- `html.rs` — HTML DOM
- `web_canvas.rs` — WebAssembly Canvas

모두 rhwp 자체 layout 결과를 받는 백엔드 — **우리 byte-eq layout 결과와는 별개**.

## byte-eq pipeline 영향

✅ **SVG export adapter 직접 작성 필요** (~500-2000 LOC 추정):
- 우리 byte-eq layout 의 출력 (CharItemView::Draw 의 path / brush / pen / underline 등)
  → SVG primitive (`<path>` / `<rect>` / `<line>` / `<image>` / `<defs>`) 로 emit
- font 출력: HFT decoder ([[reference_hft_decoder_complete]]) 로 glyph path 추출 후
  `<path>` emit (NOT `<text>` element — pixel-eq 위해)
- 한컴의 좌표/색/모양 결정 → SVG 좌표/색/모양 mechanical 1:1 변환

## How to apply

- 이전 메모리의 "rhwp PaintOp wire 만 필요" 는 **잘못** — 정정.
- Stage 4 = "SVG export adapter (직접 작성) + pixel-diff harness".
- rhwp svg.rs 는 mechanical mapping (특히 font embed, gradient defs) 참고용으로만.
- `kdsnr-render` crate 의 `svg_surface.rs` 가 우리 어댑터의 출발점 — 확장 필요.

## 사용자 실측

"내가 실제 확인한 바로는 [rhwp 출력이] 전혀 한컴과 동일하지 않았거든."
→ rhwp 가 한컴 1:1 아니라는 사실의 결정적 근거.
