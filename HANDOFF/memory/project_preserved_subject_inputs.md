---
name: project-preserved-subject-inputs
description: 2026-05-20 SDK split/render pipeline 실행 후 과목별 input 샘플과 산출물을 work/preserved_subject_inputs 에 보존.
metadata:
  type: project
  originSessionId: current
---

# 과목별 input 샘플 보존 (2026-05-20)

## 위치

`kdsnr-hwp-toolkit/work/preserved_subject_inputs/`

- `manifest.json` — 보존된 input, split 출력, preview 목록
- `math/input/math_input_sample_2.hwp`
- `math/input/math_input_sample_2.pdf` (GT PDF)
- `science/input/science_input_example_2.hwp`
- `science/input/science_input_example_2.pdf` (GT PDF)
- `social/input/social_test_input_2.hwp`
- `korean_unsupported/input/국어_박스, 밑줄, 묶음.hwpx`
- `korean_unsupported/input/국어_박스, 밑줄, 묶음.pdf` (GT PDF)

## 실행 결과

도구:

```bash
../.venv/bin/python tools/preserve_subject_samples.py \
  --out-dir work/preserved_subject_inputs \
  --preview-count 3
```

지원 과목 pipeline 결과:

| subject | input | split count | preview |
|---|---|---:|---|
| math | `math_input_sample_2.hwp` | 14 | Q09, Q10, Q11 PDF/PNG/SVG |
| science | `science_input_example_2.hwp` | 6 | Q01, Q07, Q11 PDF/PNG/SVG |
| social | `social_test_input_2.hwp` | 20 | Q01, Q02, Q03 PDF/PNG/SVG |

국어는 현재 API 정책대로 미지원:

```text
국어 과목은 아직 지원하지 않습니다
```

## 검증

- `tools/preserve_subject_samples.py` compile OK
- `split_set_to_question` 으로 수학/과학/사회 split 성공
- 각 과목 앞 3문항 `render_pdf` + `pdf_to_question_png` + `render_svg` 성공
- 국어 unsupported error 확인

## How to apply

- 재현 가능한 supported-subject input 세트가 필요하면 `work/preserved_subject_inputs/*/input/` 를 사용한다.
- split 결과는 `work/preserved_subject_inputs/<subject>/split/`.
- 빠른 시각 확인은 `work/preserved_subject_inputs/<subject>/preview/`.
- 새 세트로 갱신할 때는 `tools/preserve_subject_samples.py` 의 `SUPPORTED_SAMPLES` 만 바꿔서 재실행한다.
