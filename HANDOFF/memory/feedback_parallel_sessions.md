---
name: feedback_parallel_sessions
description: 같은 레포에 여러 claude 세션을 동시에 띄워 작업 — 파일 충돌·중복 작업 감지/정리 절차
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

사용자는 Cursor 에서 **같은 레포에 여러 claude 세션을 동시에** 띄워 작업하기도 한다 (12번째 세션에서 4개 동시 실행 확인). 같은 작업 범위면 파일 덮어쓰기·중복 RE 가 발생한다.

**Why:** 한 턴 안에 `<system-reminder>` 로 "파일이 수정됨" 통보가 계속 들어오는데 내가 수정한 게 아니면 = 다른 세션이 병행 작업 중. 모르고 같은 파일을 편집하면 서로 덮어쓴다.

**How to apply:**
- 내가 안 건드린 파일이 턴 도중 수정 통보되면 → `ps -Ao pid,ppid,etime,command | grep native-binary/claude` 로 동시 실행 세션 확인.
- 자기 PID 식별: 현재 shell `$$` 에서 `ps -o ppid= -p <pid>` 로 부모 체인을 타고 올라가 claude PID 매칭.
- 정리는 사용자 위임 시에만. 자기 PID 는 **반드시 제외**하고 나머지만 `kill -TERM`.
- 다른 세션 작업물은 함부로 되돌리지 말 것 — 먼저 RE 기반·정공법 준수 여부 검증 (12번째 세션: 다른 AI 의 11번째 세션 작업은 정상이었음, 독립 교차검증으로 확인). 망친 경우에만 되돌림.
- 충돌 회피: 같은 범위 병렬 금지. 한 세션이 한 범위를 단독 소유하도록 사용자와 조율.
