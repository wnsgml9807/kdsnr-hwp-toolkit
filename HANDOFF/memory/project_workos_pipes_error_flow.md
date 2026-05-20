---
name: WorkOS Pipes OAuth 거부 플로우 제약
description: Pipes authorize 엔드포인트는 거부 시 콜백 메커니즘이 없음 — 재조사 방지
type: project
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
WorkOS Pipes `POST /data-integrations/{slug}/authorize` 는 거부/에러 시 우리 앱으로 돌아올 방법이 **원천 없음**. 2026-04-22 실측/소스 검증 완료.

**Why:** 사용자가 Google 동의 화면에서 "거부"를 누르면 Google이 WorkOS 콜백으로 `?error=access_denied&state=...` 리다이렉트 → WorkOS 서버는 `HTTP 200` + 정적 HTML 에러 페이지("Connection Error")로 응답하고 끝. `Location` 헤더 없음, `return_to` 리다이렉트 없음, 페이지 내 `<script>`·`postMessage`·`opener`·`meta refresh`·링크 모두 없음, `x-frame-options: SAMEORIGIN`.

**확인한 것 (재조사 불필요):**
- 공식 authorize 바디 파라미터: `user_id` / `organization_id` / `return_to` 3개뿐 (workos-python auto-generated SDK `_resource.py` — OpenAPI 에서 자동 생성)
- `return_to_on_error`, `cancel_url`, `error_redirect_uri`, `state` 모두 **없음**
- cross-origin popup 이라 opener 에서 `popup.location.*` 읽기 불가 (SecurityError)
- `popup.closed` 폴링이 거부를 감지할 수 있는 **유일한 신호**

**How to apply:**
- Pipes OAuth 거부 UX 개선 요청이 또 나오면 이 제약 먼저 인용. 같은 조사 반복 금지.
- "에러 URL 감지해서 자동 리다이렉트" 같은 아이디어는 모두 cross-origin 벽에서 막힘 — 시간 낭비.
- 진짜 거부 플로우가 필요하면 **Pipes 우회 → Google OAuth 직접 구현** 만이 답 (redirect_uri 를 우리 도메인으로 설정하면 `error=access_denied` 파라미터를 백엔드에서 직접 받을 수 있음). drive.py 재작성 필요.
- 현재 구현(`driveOAuthPopup.ts`)의 `popup.closed` 폴링 + 에러 토스트가 Pipes 범위 내 최선.
- 실측 재현: `curl -sS -o /dev/null -D - "https://api.workos.com/data-integrations/google-drive/{flow_id}/callback?error=access_denied&state={state}"` → 200 정적 HTML.
