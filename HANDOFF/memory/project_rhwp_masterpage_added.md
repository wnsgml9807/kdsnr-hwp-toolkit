---
name: project-rhwp-masterpage-added
description: rhwp 의 hwpx loader 에 Contents/masterpageN.xml 파싱 추가 — 한컴 시험지 헤더/페이지번호/divider source 확보 (2026-05-18)
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# rhwp masterpage 파싱 추가 (2026-05-18)

**한컴 시험지 (수능/모의고사) 의 헤더/divider/페이지번호 = master page 안에 정의**.

## 발견 흐름

- wire_real_gt visible 결손 95% 가 헤더/divider/페이지번호 누락
- rhwp 의 build_header 가 `page_content.active_header == None` 시 children 0
- pagination/rendering.rs:968 의 master page select logic 은 이미 있음 (자동 active 설정)
- 하지만 rhwp 의 `parse_hwpx` 가 **`Contents/masterpageN.xml` 자체를 안 읽음**
- → `section.section_def.master_pages` 가 비어있음 → select skip → 헤더 영역 통째 누락

## 패치

`vendor/rhwp/src/parser/hwpx/mod.rs::parse_hwpx`:
- Chart loading 다음에 masterpageN.xml 0~31 까지 시도해서 읽음
- 각 file 을 `section::parse_hwpx_master_page(xml)` 로 파싱
- 모든 section 의 `section_def.master_pages` 에 push (단순 정책: 전체 share)

`vendor/rhwp/src/parser/hwpx/section.rs`:
- 새 함수 `parse_hwpx_master_page(xml) -> Result<MasterPage, HwpxError>` 추가
- root `<masterPage type=EVEN/ODD/BOTH ...>` → `mp.apply_to`
- `<hp:subList textWidth=... textHeight=...>` 의 attr 보존
- subList 안 `<hp:p>` → `parse_sublist_paragraphs(reader, b"subList")` 재사용

## 결과 (12 페어 wire_real_gt)

| metric              | 패치 전 | 패치 후 | Δ |
|---------------------|--------|--------|---|
| ink IoU dilate-2px  | 16.68% | **19.00%** | +2.3%p |
| ink IoU dilate-5px  | 26.90% | **29.90%** | +3%p |
| ink-only strict     | 1.12%  | 2.00%  | +0.9%p |
| full (w/ bg)        | 96.59% | 96.58% | -0.01%p |

## visible 효과 (점수보다 큼)

✅ "수학 영역"/"사회탐구 영역" 헤더 텍스트 + 정렬 (math 우측 정확)
✅ 페이지 상단 가로 underline
✅ 페이지번호 박스 (e.g. "2 / 4") — body 우측 하단

## 잔여 결손

⚠ "사회탐구 영역" 좌측 배치 (GT 우측) — master page 종류 다를 수도
⚠ 페이지 우측 세로 divider — master page 의 table border? 미렌더
⚠ Picture (math 도형) 누락
⚠ Equation 안 그리스 문자 → `?` 박스 (PUA / equation 텍스트)

## Phase B1: LineShape startPt/endPt 파싱 추가 — 2026-05-18

`vendor/rhwp/src/parser/hwpx/section.rs::parse_shape_object`:
- `<hc:startPt x=... y=.../>` / `<hc:endPt x=... y=.../>` element 처리 추가
- LineShape 생성 시 채움 (`start: start_pt, end: end_pt`)
- bug 원인: rhwp 가 startPt/endPt element 자체를 skip → LineShape.start = end = (0,0) default → SVG `M514 143 L514 143` (시작=끝, 보이지 않음)

효과 (한 fix 큰 점프):
| metric | 이전 | 이후 | Δ |
|---|---|---|---|
| IoU dilate-2px | 19.07% | **24.73%** | +5.7%p |
| IoU dilate-5px | 29.98% | **36.68%** | +6.7%p |
| ink strict | 1.98% | **5.45%** | 2.75× |
| worst pair | 4.85% | 10.32% | 2.13× |

visible: 페이지 우측 세로 divider 모든 페어 등장 (GT 와 위치 일치). 한컴 시험지의 정형화된 본문 분리 column 가시화.

## Phase C: Picture mime fix (kdsnr-render 패치) — 2026-05-18

`render-engine/rust/src/svg_surface.rs`:
- `image_data_uri(data)` helper 추가 — magic byte 로 mime 자동 결정 (PNG/JPEG/GIF/BMP/TIFF/WebP)
- `draw_image_rect` / `draw_image_point` / `draw_image_f` 3개 함수가 hardcode `image/png` → helper 호출로 통일
- bug 원인: 한컴 hwpx 의 `BinData/image*.jpg` (JPEG) 가 우리 SVG 에 `data:image/png;base64,...` 으로 emit → resvg mime/data mismatch → silent skip

효과:
- math Q28_3 의 입체 삼각형 + α/β/ℓ/A/B/C/H 라벨 완전 복구 (이전 = 빈 사각형 영역)
- 점수 작은 변화 (19.00% → 19.07%) but visible 큰 효과

## 다음 priority

1. 우측 세로 divider 누락 진단 (master page 안 어떤 element?)
2. "사회탐구 영역" alignment (좌측 → 우측)
3. Equation 안 PUA / 그리스 문자
4. G (kdsnr-layout wire, 장기)

## Phase D: Equation HYHWPEQ.TTF load (2026-05-18)

`render-engine/rust/examples/wire_real_gt.rs::rasterize_svg`:
- usvg fontdb 에 `/Applications/Hancom Office HWP.app/.../HYHWPEQ.TTF` 명시 load
- 한컴 본문 TTF 폴더 (TTF/Install/) 전체도 load
- 결과: equation 안의 PUA 그리스 문자 (α/β/θ/ℓ) 모두 정확 렌더 (이전 = ? 박스)
- math Q28_3 visible: GT 와 거의 동일

## Phase E: tab type=3 + compute_char_positions wire (2026-05-18)

`vendor/rhwp/src/renderer/layout/text_measurement.rs`:
- match tab_type 의 `_ => 왼쪽` 분기에서 `3` 제거. `1 | 3 => 오른쪽 정렬` 로 패치
- 영향: inline_tabs 경로 + custom_tabs 경로 + compute_char_positions 의 양쪽 분기
- module visibility: `pub mod text_measurement` (외부 접근 가능)

`render-engine/rust/examples/wire_real_gt.rs`:
- TextRun emit 시 `rhwp::renderer::layout::text_measurement::compute_char_positions(text, style)` 직접 호출
- per-char advance = `max(computed, hft_or_estimate)` — tab 위치 보존 + 글자 자체 너비 보장
- 결과: "사회탐구 영역" 우측 정렬 ✅ (GT 와 visible 일치), 본문 spacing 정상 유지

측정 (12 페어, 누적):
| metric | 세션 처음 | 현재 (v8) | Δ |
|---|---|---|---|
| IoU dilate-2px | 16.68% | **24.99%** | +8.31%p |
| IoU dilate-5px | 26.90% | **37.08%** | +10.18%p |
| ink strict | 1.16% | **5.69%** | 4.9× |
| best pair | 26.84% | **41.08%** | +14.24%p |

visible (math Q28_3 + social Q17): GT 와 거의 동일 페이지 형식 — 헤더/divider/페이지번호/도형/그리스 문자/본문 spacing 모두 정확. 남은 미세 차이 = rhwp typeset 정밀도 한계 (G phase 로만 fix).
