---
name: Korean HWPX 국어 파이프라인 완료 현황
description: 국어 HWPX 빌더 전 기능 완성 (6가지 스타일 유형). ingest_pdf.py에 통합됨. DB 등록은 PDF 수집 후 진행 예정.
type: project
---

국어 HWPX 파이프라인 구현 완료. `ingest_pdf.py`에 통합됨.

**완성된 6가지 스타일 유형:**
1. 세트 헤더 (지시문): `[1~3] 다음 글은 ~`
2. 지문 부분: 밑줄(`<u>`), 박스(`<box>`), 마커(`<mark-A>[A]</mark-A>`)
3. 문항 발문
4. 선지 부분 — 개별 paragraph
5. 보기 부분 — 3×3 텍스트 테이블
6. 묶음 브라켓 ([가]/[나]) — 3×2 플로팅 테이블

**DB 등록 TODO:** 국어 PDF 수집 → `batch_ingest_korean.py` 작성 → 배치 등록

**How to apply:** format_units_korean.py + KoreanHwpxBuilder 동작 중. 추가 스타일 조정은 한글 시각 검증 후 상수만 수정.
