---
name: Toolkit 입력 원본 hwpx/hwp 위치
description: 전 과목 toolkit 파이프라인 입력 원본 (korean/math/science/social) 이 모여 있는 디렉토리. flap-hwp-parser 는 레거시라 코드 무시, 입력은 toolkit 으로 이전 완료.
type: reference
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
전 과목 입력 원본 위치 (2026-05-14 이전 완료):

`/Users/wnsgml/Desktop/새 폴더/Project/KSAT Agent/Agent_Streamlit/kdsnr-hwp-toolkit/templet/original/`

이전: `flap-hwp-parser/templet/original/` (이제 비어있음, .DS_Store 만 잔존)

수록 파일 (23개):
- korean.hwpx, 국어_박스, 밑줄, 묶음.hwpx (+ 복사본), 국어_박스, 밑줄, 묶음.pdf, 출제 기본 틀.hwp
- math.hwpx, math_input_sample{,_2}.{hwp,hwpx,pdf}
- science.{hwp,hwpx}, science_input_example{,_2}.{hwp,pdf}
- social.hwpx, social_input_sample.{hwpx,pdf}, social_test_input_2.{hwp,hwpx}
- korean.pdf, social_input_sample.pdf 등 일부에 대응 PDF 동봉 (한컴 GT 출력 ground truth)

How to apply:
- "전 과목 입력", "모든 input 다시 출력" 류 요청은 work/0513 이 아니라 이 디렉토리를 input 으로 본다.
- flap-hwp-parser 자체는 레거시. 코드 거들떠 보지 말 것.
- 출력은 toolkit `render_question_png` (preview.py) 로 hwpx → PDF → PNG.
