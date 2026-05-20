---
name: 부분 조사 기반 해석·임의 수정 금지
description: 시스템의 일부만 보고 "실패/버그/오동작" 단정 금지. 요청 없는 코드 수정·설계 변경 제안 금지.
type: feedback
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
**Why:** 2026-04-23 세션에서 두 번 같은 실수 반복.
1. `hwp_view_image` 호출 결과가 DB 엔 메타만 남은 걸 보고 "이미지 못 봤음" 단정 → 실제론 `__image__` base64 가 provider 블록으로 LLM 에 전달됐음. `_append_tool_result_to_messages` 가 pop 후 DB 저장하는 구조를 알았어야 함.
2. `pdf_read` 도 동일 패턴에서 "67자만 반환, 실패/hallucination" 으로 단정 → 실제론 `__pdf__` 로 전체 PDF 를 gpt-5.4 에 전달. 같은 실수 반복.

그 위에 "pdf_read 의 start_page/end_page 가 무시됨 → slicing 구현 or 스키마 설명 수정" 같은 **허락 안 한 autonomous 수정 방향 제시** 도 지적받음. 진단을 넘어서 처방까지 내가 결정하지 말라는 의미.

**How to apply:**

- 시스템 일부(DB · 로그 · API 응답 한 구석)만 보고 "실패/버그/미구현" 단정 금지. 반드시 **end-to-end 구현 경로**(프론트 → SSE → 백엔드 → DB) 전체를 확인한 뒤 판단.
- 특히 tool result 가 DB 에 짧게 남아있으면 `__image__`/`__pdf__`/미디어 블록이 pop 됐을 가능성부터 의심. `chat_tool_loop.py` + `chat_tools.py` + 해당 도구의 client 구현 (`sseHandlers.ts` 등) 셋 다 본 후 말하기.
- 진단은 보고하되, **처방(수정 방향·리팩터링 제안)** 은 사용자가 요청할 때만. "버그 발견했으니 고칠 방향" 식으로 먼저 제시 금지.
- 대화 중 사용자가 지적한 후에야 틀림을 인정하는 패턴을 반복하지 말 것. 발언 전에 전수 조사.
