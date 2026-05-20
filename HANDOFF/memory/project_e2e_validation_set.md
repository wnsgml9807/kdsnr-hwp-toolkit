---
name: 분할 정상본 + 한컴 reference .hwp 세트 (work/e2e)
description: 마지막 정상 분할 시점의 hwpx + 사용자 한컴 변환본 hwp 12개 위치. layout RE byte-equivalent 검증의 reference.
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
byte-equivalent 검증 (Phase C) 의 입력/정답 세트.

## 위치

- 루트 (입력 + layout 정답 + 렌더 정답): `kdsnr-hwp-toolkit/work/e2e/<set>/Qxx.{hwpx,hwp,png}`
- **한컴 PDF export 정답**: `kdsnr-hwp-toolkit/work/GT/<set>/Qxx.pdf` (12개, e2e 와 1:1 매핑) — Phase I-2 pixel-diff harness reference

## 사용 가능한 12쌍

- `korean__국어_박스, 밑줄, 묶음 복사본/S01-03.{hwpx, hwp, png}`
- `korean__국어_박스, 밑줄, 묶음 복사본/S18-21.{hwpx, hwp, png}`
- `math__math_input_sample_2/Q28_2.{hwpx, hwp, png}`
- `math__math_input_sample_2/Q28_3.{hwpx, hwp, png}`
- `math__math_input_sample_2/Q29_2.{hwpx, hwp, png}`
- `science__science_input_example/Q20.{hwpx, hwp, png}`
- `science__science_input_example_2/Q17.{hwpx, hwp, png}`
- `science__science_input_example_2/Q25.{hwpx, hwp, png}`
- `social__social_test_input_2/Q13.{hwpx, hwp, png}`
- `social__social_test_input_2/Q15.{hwpx, hwp, png}`
- `social__social_test_input_2/Q17.{hwpx, hwp, png}`
- `social__social_test_input_2/Q20.{hwpx, hwp, png}`

## 분할 파이프라인 상태

- **`work/e2e/` 생성 시점 = 마지막으로 분할 파이프라인이 정상 동작하던 때**.
- 그 이후 시점 (`work/0513/pipeline_v2/` 등) 의 hwpx 는 **한컴에서 안 열림** — 분할 결과 손상.
- 사용자가 e2e 의 hwpx 들을 한컴 macOS 로 직접 열어 `.hwp` 로 변환해 둠 → reference 로 사용 가능.

## 검증 시 사용

- 입력: `*.hwpx` (분할된 정상본)
- 한컴 layout 정답: `*.hwp` (paragraph header 에 한컴이 만든 linesegarray 포함 — `kdsnr-hwp-toolkit/layout-decoder/rust` 의 출력과 byte 비교 대상)
- 시각 정답 (PNG): `work/e2e/<set>/*.png` (한컴이 렌더한 비트맵)
- **시각 정답 (PDF)**: `work/GT/<set>/*.pdf` — 한컴 GUI 의 PDF export 결과. Phase I-2 pixel-diff harness 에서 우리 출력 PDF 와 poppler/ImageMagick 으로 비교

## 첫 검증 대상 추천

`korean__국어_박스, 밑줄, 묶음 복사본/S01-03.{hwpx, hwp}` — 텍스트만 (이미지/표 없음), layout 변수 가장 적음. 첫 byte-equivalent 비교에 적합.

**Why:** 다음 검증 세션이 "어디 hwpx 를 input 으로, 어디 hwp 를 정답으로 쓸지" 매번 재탐색하지 않도록 위치 + 추천 순서 박제.

**How to apply:** Phase C harness 작성 시 이 12쌍 중 단순한 것부터 차례로 통과시킴. e2e 외 (`pipeline_v2/`, `pipeline/` 등) 의 분할물은 손상이라 사용 금지.
