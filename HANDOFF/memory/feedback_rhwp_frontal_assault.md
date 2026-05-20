---
name: rhwp 정면돌파 정책
description: rhwp 라이브러리에 책임이 명확히 드러나면 우회로 만들지 말고 라이브러리를 직접 고친다
type: feedback
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
rhwp 측에 버그/한계가 명확히 입증되면 파이프라인 쪽 우회로 처리하지 말고
rhwp 자체를 직접 패치한다.

**Why:** 우회로가 누적되면 매 input 마다 새로운 quirk 가 생기고, 결국
"input 마다 노가다" 의 근본 원인이 된다. 사용자는 정공법·완벽 구현을 명시적으로
요구함 (feedback_no_time_optimization).

**How to apply:**
- "그냥 책임 있다" 가 아니라 reproducible 증상 + dump 로 입증된 경우에만 rhwp 손댐
- 입증되지 않은 단계에서는 rhwp 무죄 추정
- 손댈 경우 우회로/예외 처리는 동시에 제거 (이중 방어 금지)
- 알려진 quirk 는 project_rhwp_quickxml_entity_bug.md, project_rhwp_vertalign_quirk.md 참고
