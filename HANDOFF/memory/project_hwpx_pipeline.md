---
name: HWPX 문항 생성 파이프라인
description: LLM JSON → format_units → builder → HWPX 파일 생성. 전 과목 빌더+DB 등록 완성. 수학 1,916 + 사회 3,000 + 국어 525 = 5,441개.
type: project
---

## HWPX 라이브러리 구조

### 핵심 파일 (`davinci/backend/lib/hwpx/`)

| 파일 | 역할 |
|------|------|
| `builder.py` | HwpxBuilder(base), MathHwpxBuilder, SocialHwpxBuilder, KoreanHwpxBuilder |
| `format_units.py` | 수학: LLM JSON → HWPX XML (일반/보기/박스/그래프/단답형) |
| `format_units_korean.py` | 국어: LLM JSON → HWPX XML (세트 구조, 6가지 스타일) |
| `format_units_social.py` | 사회: LLM JSON → HWPX XML (passage/table/bogi/image 블록) |
| `graph_renderer.py` | matplotlib 그래프 PNG 렌더러 (GraphRenderer, GCurve, GDot 등) |
| `graph_tools.py` | LLM 그래프 도구 핸들러 + 레지스트리 |
| `latex_to_hwpeq.py` | LaTeX → HWP 수식 변환 |
| `ingest.py` | PDF 스캔/크롭 (수학 개별, 국어 세트별) |
| `ingest_pdf.py` | 통합 등록 파이프라인: PDF → 크롭 → LLM 구조화 → HWPX 빌드 → Storage → 임베딩 → DB |
| `parser.py` | HWPX 파일 파싱 (ParsedItem) |

### 테스트/배치 (`davinci/hwpx/test/`)
- `test_format_and_build.py` — 수학 E2E (9문항)
- `test_format_and_build_korean.py` — 국어 E2E
- `test_format_and_build_social.py` — 사회 E2E
- `test_pdf_to_hwpx_*.py` — 과목별 PDF→HWPX 테스트
- `test_ingest_pdf.py` — 단일 PDF 등록 테스트
- `batch_ingest_math.py` — 수학 배치 등록
- `batch_ingest_social.py` — 사회 배치 등록 (합본 PDF 4페이지 분할)
- `batch_ingest_korean.py` — 국어 배치 등록 (kice + kice_old)

### 템플릿
- `davinci/hwpx/templates/math/` — 수학 HWPX 템플릿
- `davinci/hwpx/templates/social/` — 사회 HWPX 템플릿
- `davinci/hwpx/templates/korean/` — 국어 HWPX 템플릿

## 과목별 현황

### 수학 (완성 + DB 등록 완료)
- 5종 문항: 일반, 보기(bogi), 박스(box), 그래프, 단답형(주관식)
- 1,916개 DB 등록 완료
- gpt-5.4-mini 사용

### 사회 (완성 + DB 등록 완료)
- blocks 기반: passage, table, bogi, image 자유 조합
- 7과목 150시험 3,000문항 등록 완료 (경제 340, 사회문화 480, 생활과윤리 460, 윤리와사상 340, 정치와법 700, 세계지리 340, 한국지리 340)
- 합본 PDF 4페이지 자동 분할 + 메타데이터 추출/추론
- gpt-5.4 사용

### 국어 (완성 + DB 등록 완료)
- 세트 구조: 지문 + 문항 묶음
- 6가지 스타일: normal/dialogue/ellipsis/title/vocab + 인라인 밑줄/마커/박스/보기/묶음 브라켓
- 525개 세트 등록 완료: kice 29 PDF 322세트 + kice_old 18 PDF 203세트
- gpt-5.4 사용

## 기술 결정사항
- LLM: gpt-5.4-mini (gpt-5.4와 동등, 비용/속도 우위)
- 임베딩: Google Gemini multimodal (텍스트+이미지 동시)
- Storage: Supabase Storage (PDF/PNG/HWPX/BinData)
- 병렬화: PDF간 asyncio.Semaphore + 항목간 asyncio.gather
- HTTP/2 이슈: 항목마다 새 Supabase 클라이언트 생성으로 해결
- Supabase: Pro 플랜 ($25/월), Micro 컴퓨트

**Why:** 기출 문항을 구조화된 HWPX로 생성/편집/내보내기 가능하게 함.
**How to apply:** 전 과목 등록 완료. 추가 등록 시 기존 batch_ingest 스크립트 참조.
