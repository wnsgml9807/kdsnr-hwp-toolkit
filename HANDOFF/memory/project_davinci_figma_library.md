---
name: project-davinci-figma-library
description: Davinci UI 컴포넌트 라이브러리 — Figma 파일 위치·구조·폰트 제약·남은 수동 작업
metadata: 
  node_type: memory
  type: project
  originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---

기획자(비개발자)가 Davinci 화면 시안을 직접 조작으로 만들도록 Figma 에 Davinci UI 컴포넌트 라이브러리를 구축. davinci-ui-kit/components/full.html 을 전수조사해 그대로 이식.

- **파일**: fileKey `BSExKS0NW4jLTUggfmG7Iu` — https://www.figma.com/design/BSExKS0NW4jLTUggfmG7Iu
- **팀**: kdsnr-davinci (planKey `team::1636927293257729891`), pro 플랜
- **MCP**: figma 원격 MCP 서버 (`mcp.figma.com`)
- **빌드 내용**: 토큰 4컬렉션 101변수(무지개 확장) + 텍스트 7 + 이펙트 3 / 아이콘 60 / 전체 변형 274
- **페이지 순서** (2026-05-15 갱신): 전체 화면(보존) · 영역(보존) · 색상·글자·간격 · 버튼 · 입력 · 선택 · 배지 · 리스트 · 카드 · 오버레이 · 채팅 · 패턴 · (legacy) 아이콘·버튼·배지·메뉴 · (legacy) 레이아웃·사이드바·목록
- **신규 페이지 컴포넌트** (변형 수): 버튼 67(기능 15 + 무지개 36 + 아이콘 12 + 특수) / 입력 12 / 선택 15(슬라이더·라디오 카드 포함) / 배지 19 / 리스트 19(통합 테이블 3종) / 카드 44(박스 7 + 사이즈·구성 + 도구 4 state + HWP 3 + 알림 + 통계 + 가격 + 빈상태 + 이미지타일·미디어·히어로·갤러리) / 오버레이 9 / 채팅 23(메시지 7 + Dock 5 + 모델 셀렉터 + 채팅 입력 5) / 패턴 11(탭·단계·검색·필터·페이지네이션·breadcrumb·KPI)
- 상태 원장: `/tmp/dsb-state-davinci-ds-2026-05-15.json` (컴포넌트 ID 맵 포함)

**Why**: 기존 davinci-ui-kit(loader.html) → Streamlit Studio → 결국 Figma 직접조작으로 피벗. 사용자가 "바이브코딩은 접근성 나쁨" 이라며 결정.

**How to apply**:
- 폰트 제약 확정 — Figma MCP(pro 플랜)는 Pretendard 를 못 씀. MCP `use_figma` 는 서버사이드라 Google Fonts + Apple 폰트만 접근, 로컬/조직 폰트 ✗. 조직 폰트 업로드는 Organization 플랜 전용. 그래서 UI 폰트는 **Gothic A1**(대체), serif 는 **NanumMyeongjo**(원본 일치). 텍스트 스타일 7개만 바꾸면 나중에 스왑 가능.
- 파일명은 플러그인 API 가 변경 불가 (`Setting the document name is currently not supported`) — 사용자가 Figma 에서 수동.
- `use_figma` 함수 안에서 `await` 쓰려면 그 함수도 `async` 여야 함 (안 그러면 "expecting ';'" SyntaxError).
- 아이콘은 `figma.createNodeFromSvg` + `createComponentFromNode` 로 원본 SVG 그대로 이식.
- **남은 사용자 수동 작업**: 파일명 Davinci UI 로 변경 · 팀장(권준희) 지정 · 라이브러리 publish (Assets 패널) · 기획자 editor 초대.
- **사용자 지시 (2026-05-15)**: 영역·전체 화면 페이지는 손대지 않음. 채팅·통신 부속은 채팅 페이지에 따로. 컴포넌트는 다빈치 .tsx 코드 그대로 빼다박기 ([[feedback_davinci_figma_match_source]]). 예시 이름은 권준희 + 남자 이름 다수.
- 사용자 선호: 커버·장식·자질구레한 설명 금지, 평범한 한국어, 담백하게. [[feedback_ui_guidelines]]
