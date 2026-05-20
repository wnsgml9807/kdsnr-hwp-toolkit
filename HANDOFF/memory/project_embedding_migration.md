---
name: Kichul DB redesign & architecture decisions
description: 기출 DB kichul_items 통합 테이블 중심 아키텍처, 도구 4종(search/list/fetch/upload) 분리, PDF.js+react-pdf 뷰어, 인증 계획
type: project
---

## 핵심 아키텍처 결정

**서비스 구조:**
- 서비스 = 모듈형 앱 (유사도 검사, 향후 이메일 자동화 등)
- 채팅 = 빌트인 서비스, LLM 도구셋을 사용자별로 부여
- 도구셋은 개발자(관리자)가 계정 생성 시 지정
- service_access 체계로 도구 접근 제어

**기출 DB:**
- 단일 테이블 `kichul_items` (pdf_url, hwpx_url, text_content, embedding)
- `pdf_url`: 크롭/스티칭 PDF (Supabase Storage) — react-pdf로 텍스트 선택 가능
- `hwpx_url`: 독립 HWPX 파일 (Supabase Storage) — export/AI 편집용
- 임베딩: Google Gemini multimodal (텍스트+이미지 동시)
- 설계 사양: `davinci/context/kichul_db_redesign.md`
- DB/API README: `davinci/backend/sql/functions/kichul/README.md`

**기출 도구 (4종 × 3과목 = 12개):**
- `search_{subject}_kichul`: 의미 검색 (임베딩 기반, 필터 없음). query 필수, image_ref 선택.
- `list_{subject}_kichul`: 메타데이터 필터 조회 (year/month/source, 임베딩 없음)
- `fetch_{subject}_kichul`: text_content를 LLM context에 삽입 (Desk에 안 올림)
- `upload_{subject}_kichul`: PDF를 Desk artifact로 등록
- search/list 분리 이유: LLM이 search에 임의 필터 추가하여 검색 정확도 저하 방지

**프론트 뷰어:**
- react-pdf (wojtekmaj v10.4.1) — `<Document>` + `<Page>` + TextLayer
- 플로팅 줌 리모콘 (sticky, 좌측 상단), 텍스트 드래그 → "질문하기" 인용
- DeskTab에 `pdfUrl`, `hwpxUrl` 필드 추가, SSE artifact 이벤트로 전달

**인증:**
- 현재: 이메일+비밀번호, 관리자 승인
- 계획: Google OAuth 로그인 추가 (미착수)

## 현재 진행 상황 (2026-04-01)

**Phase 1 — 기출 DB 등록:**
- ✅ 수학: 64 PDF × 30문항 = 1,920개 등록 완료
- ⬜ 사회: 과목별 분리 전처리 필요
- ⬜ 국어: PDF 수집 + 배치 등록

**Phase 2 — 도구/로깅**: 도구 분리(search/list) 완료, TOOL_CARD_CONFIG 업데이트 완료
**Phase 3 — HWP MCP**: 설계 완료, 구현 미착수
**Phase 4 — Google 인증**: 미착수

**Why:** HTML 기반 기출 DB의 시각 정보 손실, HWP export 불가, 과목별 테이블 분산 문제 해결.
**How to apply:** Phase 1 수학 완료. 사회→국어 순서로 DB 등록 진행.
