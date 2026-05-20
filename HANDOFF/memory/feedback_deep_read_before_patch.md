---
name: feedback-deep-read-before-patch
description: 코드 patch 전 해당 모듈 + 관련 파일 전수로 읽기. 부분 보고 찔끔 고치는 식 금지.
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# 깊게 전수로 읽고 작업

**규칙**: 코드 patch 전에 해당 모듈 + 관련 파일을 **전수로 읽음**. 부분만 보고 fix 시도 → 찔끔 fix → 회귀/재패치 반복 금지.

**Why:** 사용자가 "코드를 찔끔 보고 고치고 찔끔 고치고 하지 말고, 깊게 전수로 읽어" (2026-05-20). 도구 manipulation 반복 (cheat) + 찔끔 patch 반복으로 진척 없음. 진정한 정공법 = 전체 logic 파악 후 단일 정확 patch.

**How to apply:**
- patch 대상 모듈 전수 Read (50줄/2000줄 무관, 한 번에 다 읽음)
- 관련 caller / callee 도 같이 읽음
- 한컴 식 RE 같은 경우 multiple sample data 로 검증한 후 patch (1 sample probe → patch 금지)
- patch 후 회귀 발견 시 일부 롤백 말고 전체 logic 다시 봄

관련: [[feedback-no-time-optimization]], [[feedback-probe-driven]].
