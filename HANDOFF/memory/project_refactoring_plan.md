---
name: Chat System Refactoring Plan
description: chat.py 분해, 도구 통합, Auth 정리, 에러 핸들링, 다중 인스턴스 대응 — 전체 리팩토링 설계
type: project
---

## 리팩토링 전체 계획 (2026-04-09 확정)

### Phase 1: _pending_tool_results 다중 인스턴스 대응
- Cloud Run session affinity 활성화 (즉시 해결)
- 장기: Supabase `pending_tool_results` 테이블로 이전 (Realtime 구독)

### Phase 2: chat.py 분해
- chat_session.py: ChatSession 데이터클래스
- chat_history.py: _restore_history + DB 히스토리 로드
- chat_prompt_builder.py: 시스템 프롬프트 조립
- chat_tool_loop.py: 도구 루프 (LLM 스트리밍 + 분류/실행/대기)
- chat_persistence.py: 메시지 저장, 제목, 감사로그, context 스냅샷
- chat.py: 오케스트레이터 (~80줄)

### Phase 3: 도구 정의 통합 (Single Source of Truth)
- backend/app/tools/registry.py: 도구 이름, display_name, icon, steps, client/server 구분 일원화
- chat_config.py TOOL_CARD_CONFIG 제거 (registry에서 생성)
- frontend TOOL_STEPS_MAP 제거 (백엔드에서 전달)
- sseHandlers.ts 하드코딩 도구 목록 제거 (동적 처리)

### Phase 4: Auth 정리
- AuthContext 11개 useState → 단일 상태 객체
- isAdmin/isMaster → computed
- 3초 타임아웃 → 500ms getSession() fallback
- Provider mounted 가드 유지 (Chakra SSR)

### Phase 5: 프론트엔드 정리
- catch (e) { void e; } → 토스트 UI
- Date.now() ID → crypto.randomUUID()
- __error__: 문자열 마커 → ChatMessage.isError 필드
- sseHandlers.ts any 타입 → 정확한 ChatState 타입
- content 변환 로직 중앙화

### 제약 사항
- .env는 커밋 안 함 (private repo)
- 100+ 동시 사용자, 다중 Cloud Run 인스턴스
- 도구는 빠르게 추가/변형될 예정
