---
name: HWPX 작업 방법론 및 사용자 피드백
description: HWPX 문항 생성 시 크기/폰트/스타일 결정 방법론, 사용자가 강조한 포인트들. 향후 국어/사회 Builder 작업 시 동일 원칙 적용.
type: feedback
---

## 크기/스타일 결정 원칙

### 원본 PDF에서 실측하라
- 그래프 이미지 크기를 임의로 정하지 말고, 실제 수능 시험지 PDF에서 PyMuPDF로 이미지 크기를 추출해서 기준으로 삼았음 (수학 그래프: ~77mm × 70.5mm → 21000 HU)
- `pdftoppm`으로 PDF를 PNG 렌더링해서 시각 비교

**Why:** 사용자가 크기를 여러 차례 조정 요청 (70% → 60% → 원복) 후, 결국 "원본 PDF에서 실측"으로 해결. 감으로 잡지 말고 실측이 정확.
**How to apply:** 새 과목 Builder 작업 시 반드시 해당 과목 시험지 PDF에서 이미지/테이블 크기 실측 후 적용.

### 한글(Hangul)에서 직접 열어서 확인
- 코드 수정 후 반드시 `test_format_and_build.py` 실행 → `.hwpx` 파일을 한글에서 열어 시각 확인
- XML 구조만 보고는 렌더링 결과를 예측할 수 없음 (특히 정렬, 폰트, 마진)

**Why:** paraPrIDRef="24"가 CENTER인 줄 알았지만 실제로는 DISTRIBUTE_SPACE였던 사례. 코드만으로는 알 수 없었음.
**How to apply:** HWPX 관련 변경은 항상 빌드 → 한글 열기 → 사용자 확인 루프.

## 사용자가 짚은 구체적 포인트

### 1. "마진이 너무 넓다" ≠ 이미지 크기
- 사용자가 "마진이 넓다"고 할 때, 이미지 자체의 크기가 아니라 matplotlib의 subplots_adjust (여백) 문제였음
- 이미지 크기를 줄이는 것과 렌더링 여백을 줄이는 것은 별개

### 2. 그래프 세로는 고정이 아니라 가변
- 정사각형(5.5×5.5)으로 시작했으나, 사용자가 "꼭 정사각형 아니어도 된다. 가로 고정 + 세로 가변"으로 방향 제시
- 다만 무제한 세로는 안 됨 → aspect ratio cap 1.2 (사용자 지정)

### 3. 곡선/점/폰트 크기 반복 조정
- 사용자 피드백 순서: 곡선 너무 굵다 → 좀 더 얇게 → 점도 절반 → 마진 늘려서 라벨 잘리지 않게
- 개별 요소를 한꺼번에 바꾸지 말고, 사용자가 요청하는 순서대로 하나씩 조정

### 4. 원본 템플릿 구조를 정확히 따르라
- "5지선다형" 헤더가 `<hp:rect>` (사각형 도형)인데 `<hp:tbl>` (테이블)로 만들면 안 됨
- charPr도 원본과 정확히 일치해야 함 — 사용자가 "폰트가 다르다"며 두 번 수정 요청
  - 1차: `_CHAR_PR_QNUM` (HY견명조) → 틀림
  - 2차: `_CHAR_PR_TEXT` (신명 중명조) → 여전히 틀림 (굴림체여야 함)
  - 3차: charPr 10 (굴림 볼드) → 정답

**Why:** HWPX header.xml에서 charPr/fontRef 매핑을 꼼꼼히 추적하지 않아 3번 수정.
**How to apply:** 원본 템플릿의 정확한 charPr → fontRef → font face 매핑을 먼저 확인한 뒤 코드 작성.

### 5. `\u00a0` (non-breaking space) 주의
- 한글에서 `\u00a0`이 □ (박스)로 표시됨 → 일반 공백이나 `<hp:fwSpace/>`로 대체해야 함

### 6. textWrap과 정렬의 관계
- `treatAsChar="1"` + `paraPrIDRef="4"` (CENTER)만으로는 가운데 정렬 안 됨
- `textWrap="NONE"`도 함께 설정해야 CENTER가 작동

## 개발 워크플로

1. 원본 시험지 PDF에서 대상 요소 크기/스타일 실측
2. header.xml에서 charPr/paraPr/fontRef 매핑 확인
3. 원본 HWPX (section_prefix.xml 등)에서 XML 구조 파악
4. format_units.py 수정
5. test_format_and_build.py 실행 → 한글에서 열기
6. 사용자 피드백 → 반복
