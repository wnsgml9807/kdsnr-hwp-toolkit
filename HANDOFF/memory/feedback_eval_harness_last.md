---
name: eval-harness-last
description: kdsnr-hwp-toolkit 의 pixel-diff 평가 harness 는 모든 byte-eq port 가 끝난 뒤 맨 마지막에 구축. 그 전엔 byte-eq = 정답으로 신뢰.
metadata:
  type: feedback
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# Eval harness 는 맨 마지막 (2026-05-17 결정)

**Rule**: kdsnr-hwp-toolkit 의 pixel-diff 평가 harness (Phase I-2/I-3) 는 모든 구현 (parser/layout/render/paginator/Surface) 이 끝난 뒤 **맨 마지막** 에 만든다. 중간에 만들어 점수 보면서 작업하지 않음.

**Why**:
- byte-eq port 의 의미는 "raw decompile 1:1 = 정의상 정답". 평가로 검증할 필요가 없는 작업.
- harness 점수를 보면서 작업하면 "diff 줄이는 쪽으로" 의사결정이 휠 위험. 휴리스틱·visual tweak 유혹이 생긴다.
- byte-eq 가 안 맞으면 그건 port 가 틀린 거지 정답이 틀린 게 아니다. 점수보다 raw asm 재확인이 본질.
- [pixel-eq + maximal byte-eq 목표](feedback_rhwp_byte_equivalent_goal.md) 와 정합. 우리는 "보면서 맞춰가는" 게 아니라 "1:1 port 면 자동으로 맞는다" 를 신뢰함.

**How to apply**:
- Phase I-2/I-3 는 Stage 4 (마지막). Stage 1-3 동안은 harness 만들지 않는다.
- 그 동안 검증은 unit test (rust test) + raw asm 인용 doc comment + 한컴 IR sizeof/layout 만으로 한다.
- 누가 "지금 baseline 점수만 한번 보자" 제안해도 거절. 그 점수로 우선순위 흔드는 것 자체가 정공법 위반.
- 단, 한컴 PDF reference (work/GT/ 12개) 는 이미 확보 — 마지막 harness 구축 시 즉시 사용 가능.

**관련**:
- [full byte-eq pipeline plan](project_full_byteeq_plan.md) — 본 정책 반영된 Stage 1-4 로드맵
- [정공법·완벽 구현](feedback_no_time_optimization.md)
- [pixel-eq + maximal byte-eq goal](feedback_rhwp_byte_equivalent_goal.md)
- [e2e validation set](project_e2e_validation_set.md) — work/e2e + work/GT 입력/정답 위치
