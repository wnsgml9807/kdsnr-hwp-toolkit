---
name: 다빈치 설정 페이지 아키텍처
description: /settings 전체 구조, 권한 체계, 도구 시스템, 프롬프트 관리 현황
type: project
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
## 설정 페이지 구조

```
/settings
├─ 사용자 설정 (visible: all)
│   ├─ 내 프로필 — 아바타+이름+뱃지, 과목/조직, 사용 시간/토큰 그래프 (admin+)
│   └─ 채팅 도구 — 3열: 세트 | 도구+토글 | 상세+설정 (zustand store)
│
├─ 조직 관리 (visible: master)
│   └─ 조직 현황 — 2단: 목록 | 상세+구성원 편집 (좌우 분할 다이얼로그)
│
├─ 구성원 관리 (visible: admin)
│   ├─ 구성원 현황 — 2단: 목록(필터+검색) | 상세(드롭다운 폼)
│   └─ 사용 기록 조회 — 조직별/사용자별(토큰+시간 컬럼)/개발자 로그
│
└─ 서비스 관리 (visible: all, dynamic)
    └─ 서비스별 상세
```

## 권한 체계

- master: DB에서만 설정. 조직 CRUD, 전체 구성원 열람.
- admin: master가 설정. 자기 조직 구성원만 열람. 역할은 user/admin까지만 변경 가능.
- user: 프로필, 도구, 서비스만 접근.

## 도구 시스템

- DB: tool_sets → tool_configs → user_tool_set_settings + user_tool_settings
- 세트별 on/off가 아닌 도구별 on/off (2열 헤더에 전체 토글)
- config_schema (JSON Schema) → ConfigEditor 자동 렌더링
- 채팅 시 get_disabled_tool_names RPC로 비활성 도구 필터링
- zustand toolStore로 상태 관리

## 프롬프트 관리

- backend/app/prompts/ 디렉토리에 md 파일
  - system.md: 기본 소개 (한 줄)
  - _common.md: 말투, 자기 정보 비공개, 도구 내부 체계 비공개, 화면 표시 안내
  - kichul_tools.md: 기출 도구 사용 가이드
- 과목별 프롬프트 분기 제거 → 단일 SYSTEM_PROMPT
- chat_prompts.py에서 3개 md 로드 → 결합

## 공통 컴포넌트

- SettingsUI.tsx: PageHeader, TableCard, TableHeader, ColHeader, EmptyState, SearchInput, NativeSelect, SelectRow, InfoRow, DisplayRow, InputRow, ModelToggle, PrimaryButton, DangerButton, RoleBadge, StatusDot, PanelHeader, PanelFooter
- DavinciDialog.tsx: Body, Footer, PrimaryBtn, DangerBtn, CancelBtn, GhostBtn

## 문서

- frontend/context/overview/permissions.md: 권한 규칙
- frontend/context/overview/design-system.md: 디자인 시스템 규격
- backend/context/overview/tool-system.md: 도구 아키텍처

## Context Window 링

- 모델별 context_window 값 하드코딩 (chat_config.py)
- done SSE 이벤트에 context.used/limit 전달
- ModelSelector 옆 ContextRing: 사용량 % 원형, 호버 시 "남은 대화 길이 N%"

## Zoom 기능

- Electron: webFrame.setZoomFactor() (preload.js에 API 노출)
- PlatformHeader에 줌 컨트롤 (0.5~1.5, 0.1 단위)
- localStorage에 저장, 기본값 0.85
