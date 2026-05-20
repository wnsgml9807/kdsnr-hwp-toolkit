---
name: project-font-policy-vendor-ttfs
description: 2026-05-18. toolkit 자체 폰트 라이브러리 vendor/rhwp/ttfs/hancom/All (HBATANG.TTF 등 50+) + assets/fonts/HCR* 사용. 한컴 Office 설치 불필요. raster_pages 와 svg.rs find_font_file 양쪽이 이 path 를 우선 매칭.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

## toolkit 자체 폰트 라이브러리 위치

- **vendor/rhwp/ttfs/hancom/All/** — HBATANG.TTF (Haansoft Batang v1.21, 62687 glyphs), HDOTUM.TTF, H2HDRM.TTF (HY헤드라인M), HANBaek/HANCooljazz/HANSale/HANSol/HANSoma/HANYGO/HANYhead 등 50+ 종.
- **vendor/rhwp/ttfs/hancom/flat/** — 한컴 office TTF flat layout.
- **vendor/rhwp/ttfs/hwp/**, **vendor/rhwp/ttfs/windows/** — 보조 fallback.
- **assets/fonts/** — HCRBatang.ttf / HCRBatang-Bold.ttf / HCRDotum.ttf / HCRDotum-Bold.ttf (HancomFont 2017 official, 무료 배포본).

## 시스템 한컴 Office 와 차이

- 시스템: /Applications/Hancom Office HWP.app/.../HBatang.TTF — Version 1.30 (60679 glyphs, 31636844 bytes)
- vendor: vendor/rhwp/ttfs/hancom/All/HBATANG.TTF — Version 1.21 (62687 glyphs, 32361044 bytes)
- 같은 family "Haansoft Batang" 다른 minor version. 시각 결과 거의 동일 (검증 완료).

## 코드 위치

- [render-engine/rust/examples/raster_pages.rs:12-95](kdsnr-hwp-toolkit/render-engine/rust/examples/raster_pages.rs#L12) — build_usvg_options 가 vendor ttfs 우선 fontdb load. 시스템 한컴 Office 는 보조 (RHWP_FONT_ISOLATE=1 으로 skip 가능).
- [render-engine/rust/examples/raster_pages.rs:164-205](kdsnr-hwp-toolkit/render-engine/rust/examples/raster_pages.rs#L164) — main 의 font_paths 에 vendor ttfs 추가. SVG @font-face subset embed 도 vendor face 사용.
- [vendor/rhwp/src/renderer/svg.rs:2678-2738](kdsnr-hwp-toolkit/vendor/rhwp/src/renderer/svg.rs#L2678) — find_font_file. extra_paths (raster_pages 가 전달) 가 default search_dirs 보다 우선. vendor ttfs 가 첫번째라 system 의존 끊김.

## 검증 (RHWP_FONT_ISOLATE=1 모드)

```
$ RHWP_FONT_ISOLATE=1 raster_pages Q20.hwpx out/
FONT ISOLATE mode: system fonts NOT loaded
toolkit fonts loaded from: vendor/rhwp/ttfs/hancom/All
[font-embed] Haansoft Batang → 서브셋 49.4KB (102글자, 원본 31602.6KB)
FONTDB face: family="Haansoft Batang" source=vendor/rhwp/ttfs/hancom/All/HBATANG.TTF
```

pixel-diff (ours "혼" vs PIL HBatang.TTF) = 93517 < (vs HCRBatang.ttf = 101151) → ours 가 HBatang.TTF face 정확히 사용.

## 연계

- [[project-render-with-hft]] — render 진단 흐름 / squeeze fix / substitute fix
- [[feedback-rhwp-byte-equivalent-goal]] — pixel-eq 목표
