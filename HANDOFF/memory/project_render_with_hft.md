---
name: project-render-with-hft
description: 2026-05-18 갱신. 한컴 GT PDF embed 실측 = HCR Batang TTF. 우리 fix = 신명/한양 HFT series → HCR Batang TTF substitute + SVG @font-face subset embed. 셀 baseline dy=-7 → ±0~+2 해소. 잔존 = ours 글자가 시각상 미세 굵음 + 일부 advance/wrap 차이 (정확한 root cause = rhwp 의 글자 emit 좌표/scale 의 한 픽셀 단위 정밀 추적 필요, 미해소).
metadata:
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

## 확정된 root cause (2026-05-18 측정)

한컴 GT PDF 의 embed font 실측 (pdffonts):
- social Q17: `HYporM` + `HCRBatang`
- social Q19: `HCRBatang` + `Haansoft-Batang`
- social Q20: `HCRBatang` 단일
- science Q18: `Haansoft-Batang` + `HyhwpEQ` + `HCRBatang` + `HMKMM`
- math Q28_2: `HyhwpEQ`

→ **HFT type face (신명/한양 시리즈) 는 한컴 native PDF export 시 HCR Batang TTF 로 substitute**. 우리 fix 도 동일 정책.

## HCR Batang TTF 위치 (2017 official)

- `~/Downloads/HancomFont.zip` 안에 HCRBatang.ttf / HCRBatang-Bold.ttf / HCRDotum.ttf / HCRDotum-Bold.ttf (한컴 2017 official, em=1000, asc=1.07em hhea / glyph asc=0.74em)
- 한컴 Office HWP.app 의 `HANBatang.ttf` 의 family name 도 "HCR Batang" (사실상 동일 폰트)
- toolkit 영구 위치: `kdsnr-hwp-toolkit/assets/fonts/HCR*.ttf` (work 는 gitignore 이므로 assets 로)

## 적용 fix (이번 세션)

1. **style_resolver.rs:523-569**: 신명/한양 series → `"HCR Batang"` (이전 `"Haansoft Batang"` 매핑은 부분만 정확)
2. **kdsnr_hft_global.rs:33-48,82-86**: `is_substituted_hft_face()` 추가. advance_em 도 신명 series 면 None → fontdb HCR Batang advance 사용 (layout 측정 일관)
3. **svg.rs:190-196**: `try_emit_hft_paths` 가 신명 series raw face 면 None → TTF text emit fallback
4. **svg.rs:2773-2810,2820-2840**: `@font-face` subset embed 의 MIME 을 `font/ttf` + format `truetype` 으로 정정 (TTF 인 경우; 이전 `font/opentype` 잘못)
5. **font_runtime_metrics.rs:106-118 + raster_pages.rs**: `assets/fonts` 를 fontdb 에 명시 load. raster_pages 가 `render_page_svg_with_fonts(FontEmbedMode::Subset, &font_paths)` 호출
6. **raster_pages.rs**: `RHWP_PAD_DUMP / RHWP_FACE_DUMP / RHWP_HFT_MISS_DUMP / RHWP_EMBED_DUMP` 환경변수로 dump 가능

## 측정 결과 (12 페어, 2026-05-18 fix 적용 후)

- IoU dilate-2: **16.75 → 36.24** (+19.5%p, 2.16x)
- full%: 96.60 → 96.86
- 셀 baseline dy: GT 표 본문 line 모두 ±0~+2 px (이전 -7 일관)
- line count: Q17 GT 14 = ours 14 (이전 14 vs 17)

## 잔존 증상 (사용자 시각 확인)

1. ~~**글자 자체 미세 굵음/narrow**~~ → **해소 (2026-05-18 후속 turn)**. 진범 네 개:
   - (1) **find_font_file 매핑 누락** — "HCR Batang Bold" (HCRBatang-Bold.ttf), "Haansoft Batang" (HBatang.TTF) 등이 known_font_filenames 에 없어서 subset embed 실패 → `local()` 폴백 → 시스템 폰트로 다른 모양 렌더. svg.rs:2653 fix.
   - (2) **scale(0.9,1) 가로 squeeze** — char_positions 에 ratio 이미 곱해져 있는데 `<text transform="scale(ratio,1)">` 에서 또 곱해 glyph 가 narrow squeeze. GT 한컴 native 는 wide glyph + narrow advance. svg.rs:2163-2173 + 2204-2214 fix (scale transform 제거).
   - (3) **substitute face 가 HCR Batang (얇음)** — 12 페어 GT PDF pdffonts dump 결과 science 본문 = **Haansoft-Batang** (HBatang.TTF, 굵은 stroke). HCR Batang 으로 substitute 하면 ours 가 GT 보다 명확히 얇음. style_resolver.rs:573 신명/한양 series → "Haansoft Batang" 로 변경.
   - (4) **find_font_file Haansoft Batang 폴백에 HCRBatang.ttf 포함** — 의도는 HBatang.TTF 폴백이었지만 `vec!["HBatang.TTF", ..., "HCRBatang.ttf"]` 의 마지막 후보가 extra_paths[0]=assets/fonts/ 에서 즉시 매칭 → SVG @font-face data URI 가 사실 HCRBatang.ttf 데이터로 임베드. svg.rs:2659 폴백 제거.
2. **셀 안 글자 wrap 패턴 다름** — **별도 root cause 확인 (2026-05-18 후속 turn)**: hwpx 의 본문 셀 paragraph 가 lineseg `textpos=5` (혼합 전 까지 1번째 줄) 를 정의. ours render 는 이 lineseg 따름. 한컴 native render 는 lineseg 무시하고 셀 가용 width 가득까지 push ("혼합 전 수용액" 1줄). 즉 rhwp 의 cell-paragraph 가 lineseg 를 너무 곧이곧대로 따름.
3. **별개 root cause**:
   - science Q17 자료(그래프) element 누락 (picture/shape draw)
   - science Q25 박스 너무 큼 (picture/container size)

## 다음 세션 진단 출발점

남은 잔존 = **본문 셀 paragraph 의 wrap 위치**. ours render 가 hwpx lineseg textpos 를 그대로 따라 wrap 결정. 한컴 native 는 cell 가용 width 까지 push.

진단/fix 시작 위치:
- hwpx 본문 셀 paragraph dump: `unzip -p Q20.hwpx Contents/section0.xml | grep -A 5 '혼합 전 수용액의 부피'` → lineseg textpos 확인
- rhwp 의 cell paragraph lineseg 신뢰 분기: layout/paragraph_layout.rs 또는 layout/text_measurement.rs:697 의 compute_char_positions 호출 흐름에서 cached lineseg vs self-calc 선택 로직
- 한컴 spec: lineseg 는 paragraph cache 일 뿐, render 는 self-calc 가 정공법 (특히 cell paragraph)

측정 도구:
- `RHWP_PAD_DUMP=1` (cell padding 검증, OK)
- `RHWP_EMBED_DUMP=1` (font codepoint 검증, OK)
- SVG 의 cluster x 좌표 차이 (sed 로 추출, 11.62 px advance/혼한글 등)

## 연계 메모

- [[project-splitter-enrich-linesegs]] — splitter 의 lineseg 보충 (이전 fix)
- [[reference-hft-decoder-complete]] — HFT 디코더 6 family
- [[project-hft-alias-extension]] — alias 확장
- [vendor/rhwp/src/renderer/style_resolver.rs:510-569](kdsnr-hwp-toolkit/vendor/rhwp/src/renderer/style_resolver.rs)
- [vendor/rhwp/src/renderer/kdsnr_hft_global.rs](kdsnr-hwp-toolkit/vendor/rhwp/src/renderer/kdsnr_hft_global.rs)
- [vendor/rhwp/src/renderer/svg.rs](kdsnr-hwp-toolkit/vendor/rhwp/src/renderer/svg.rs)
- [kdsnr-hwp-toolkit/assets/fonts/](kdsnr-hwp-toolkit/assets/fonts/)
