---
name: Dock 리팩토링 2026-04-24 (Phase 1 완료)
description: docsStore → Source Controller 패턴 재편. Phase 2 (PDF 백엔드 이관) 는 다음 세션.
type: project
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
2026-04-24 에 프론트 `docsStore` + `sseHandlers` 1300줄 분기를 Source Controller 패턴으로 재편 완료.

**Why:** 새 소스 추가 시 코드가 9 store action + 컴포넌트 여러 분기 + sseHandlers 분기로 퍼져서 유지보수 어려웠음. 향후 Notion / Gmail / Calendar 추가 대비 예측 가능한 레시피 필요.

## 구조

```
src/dock/
├── types.ts, errors.ts, store.ts, controller.ts, create.ts, hooks.ts, index.ts
└── sources/
    ├── hwp/     (controller + store + listener + tools)
    ├── pdf/     (controller + store)
    └── google/  (controller + store + listener + picker + tools, 3 type 커버)

src/lib/auth/pipes.ts           WorkOS Pipes 범용 (drive/notion/gmail/calendar)
src/hooks/usePipesConnected.ts  service 인자 받는 훅
src/lib/api/{chat,tools}.ts     snake_case 경계 변환
src/electron/bridge.ts          window.hwp 타입 + 래퍼
```

**삭제**: `stores/docsStore.ts`, `stores/hwpStore.ts`, `lib/driveToken.ts`, `hooks/useDriveConnected.ts`.

## 핵심 설계

- DockEntry 최소: `{type, id, name, status}`. id 는 `{type}_{4hex}` — LLM 에 fileId 등 내부 식별자 노출 안 됨.
- SourceController 인터페이스: `pickAndAttach / detach / resolve / executeTool / useWatcher / getPromptMeta / rehydrate?`.
- `_new` 툴은 `dock/create.ts` 별도 — DockController 계약(doc_id 필수)과 분리.
- sseHandlers.onToolCallClient 는 10줄 라우터: `isCreateTool ? runCreateTool : dockController.executeTool`.
- Persistence: DockStore + GoogleStore localStorage. HWP/PDF 는 새로고침 시 자동 detach (fullPath / blob URL 복원 불가).
- COM 크래시: HwpController 만 disconnected 처리 + engineRecovery done 감지 시 재open 시도. PDF/Drive 무영향.

## Phase 2 보류 (다음 세션)

**PDF 백엔드 이관.**

- 현재: 브라우저 blob URL. 새로고침 시 증발, `pdf_read` 가 `start_page/end_page` 무시하고 전체 PDF 를 `__pdf__` 로 전송 (버그).
- 이관 후: Supabase Storage + `docs` 테이블 + `/api/docs/*` + 서버 pypdf slice.
- 수정 범위: `PdfController` 파일 하나 (attach/cleanup/resolve/executeTool 내부만) + 백엔드 신규. 다른 소스 / DockController / sseHandlers 는 건드릴 필요 없음 — 리팩토링이 이 격리를 가능케 함.

## 문서

`davinci/frontend/context/overview/dock.md` — 전체 아키텍처, 소스 추가 레시피, Phase 2 계획 상세.

## 주의

- Electron 재배포 불필요: main.js / preload.js 그대로 유지. preload 의 ALLOWED_APP_HOSTS 는 localhost/127.0.0.1/kndsr-davinci.vercel.app 만 허용 — Windows 로컬 테스트 시 Mac IP 로 접근하면 `window.hwp` 노출 안 됨 (Windows 쪽에서도 Next dev 띄워야 HWP 브릿지 작동).
- 백엔드는 `chat_prompt_builder.py` 1곳만 변경 (Dock 목록 주의 문구). Cloud Run push 시 반영.
