---
name: rhwp vertAlign 처리 한컴과 차이
description: rhwp 가 sublist vertAlign 을 엄격히 적용 — 한컴은 line≈cell 일 때 자동 중앙 정렬하는데 rhwp 는 안 함. 보기 박스 inset 라벨 처방의 핵심
type: project
originSessionId: 10442048-766c-4f41-8be5-551e46033447
---
rhwp 의 cell sublist `vertAlign` 처리는 한컴 자체 렌더러와 다르다.

**한컴**: rowSpan>1 셀에서 `line_height ≈ cell_height` 인 경우, `vertAlign="TOP"` 이라도
자동으로 셀 중앙 정렬을 적용 (실증).

**rhwp**: `vertAlign` 을 엄격히 따름. `TOP` 이면 셀 상단(y=0)부터 그림.

**Why 중요:**
보기 박스(`<보 기>` 라벨이 top border 에 inset 되는 한컴 클래식 디자인)는
라벨 셀 rowSpan=2 + line_height≈cell_height 트릭에 의존한다. 한컴 원본 hwpx 의
sublist 는 vertAlign="TOP" 이지만 한컴 PDF 는 라벨이 vertical center 에 그려져
top border 가로선과 동일선상에 위치 → inset 효과.
rhwp 로 한컴 원본 hwpx 를 export 하면 라벨이 top 정렬되어 가로선보다 위로 떠 박스와
분리된 형태로 망가진다 (검증: `/tmp/kor_rhwp-1.png`).

**How to apply:**
- HWPX 빌더에서 rowSpan 셀의 라벨/제목을 한컴 PDF 처럼 정중앙에 배치하려면
  sublist `vertAlign="CENTER"` 를 명시한다 (한컴 spec 의 "TOP" 그대로 사용 금지).
- 위치는 `box_templates.py` 의 `build_bogi_box_paragraph` cell_header.
- 한컴 spec 을 그대로 따르기 전에, **한컴 원본 hwpx 를 rhwp 로 export 했을 때
  깨지는지 먼저 확인**한다. 깨지면 그 부분이 rhwp 의 한계이고 우리가 spec 을
  수정해 보정해야 한다.

**확장 원칙**: rhwp 의 vertAlign 외에도 line_height/spacing/lineWrap/border 등
한컴-rhwp 처리 차이 가능성. 비주얼 회귀가 의심되면 한컴 원본 → rhwp export 로
isolate 해서 우리 파이프라인 책임인지 rhwp 책임인지 분리할 것.
