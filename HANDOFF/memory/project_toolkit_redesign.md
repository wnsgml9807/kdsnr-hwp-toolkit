---
name: kdsnr-hwp-toolkit 재설계 핵심 계약
description: toolkit 리팩토링의 비협상 계약 — input atom → unified 템플릿 스타일 강제, 단 박스 내용물만 src 보존
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
flap-hwp-parser 는 레거시. 모든 신규 작업은 kdsnr-hwp-toolkit 에서.

**핵심 계약 (비협상)**

1. src 에서 atom 추출 → unified 템플릿에 임베딩된 스타일을 입혀 정리
2. 단 한 가지 예외: src 의 **자료박스/보기박스 내용물 (박스 안의 콘텐츠)** 은
   src 포맷 그대로 보존
3. 박스 **껍데기/shell** (border, geometry, label 등) 은 unified 템플릿이 이미
   제공하므로 코드에서 새로 만들지 않음. 템플릿에서 가져옴.
4. 그 외 모든 영역 (발문/선지/연속/수식/그림/표 슬롯 등) 은 unified 템플릿
   role style 강제. src 스타일 무시.

**Why:** 사용자 명시 (2026-05-11). flap 의 "input 마다 노가다" 근본 원인은
박스 안과 밖의 소유권 경계가 코드로 강제되지 않아서 → 매 input 마다 styling
divergence 가 생기는 것. 계약을 코드 invariant 로 박아야 함.

**How to apply:**
- core.policy.policy_for_role 가 splitter / hwpx_roles / hwpx_boxes 모두에서
  강제 통과 경로여야 함. 옆길 금지.
- 박스 shell 빌더는 unified.hwpx 에서 추출한 shell 을 재사용. 코드로 shell
  geometry 를 다시 작성하면 안 됨 (현재 hwpx_boxes.py 의 BOGI_BOX_*, DATA_BOX_*
  상수들 = anti-pattern).
- 박스 내용물에 char/para style 변형이 들어가면 무조건 버그.
- 회귀 fixture 는 없음. 첫 번째 작업의 출력이 곧 fixture.
