---
name: GH 릴리즈 publish 는 사용자 확인 필수
description: 빌드 후 Draft → public 전환은 절대 자동 금지. 사용자가 로컬 테스트 완료 후 명시적으로 publish 명령을 내릴 때만 실행.
type: feedback
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
## 규칙

**Draft → `gh release edit --draft=false` 전환은 절대 자동 실행 금지.**

- release.sh 가 중간에 abort 했다면 그 지점에서 멈춰야 함. 남은 단계를 임의로 완료시키지 말 것.
- 사용자가 "빌드해" 라고 해도 빌드까지만. publish 는 별도 명령.
- 사용자는 보통 Draft 상태로 올라간 인스톨러를 로컬에서 실행해 검증한 후에 publish 한다.

## Why

2026-04-22: 0.1.13 재빌드 세션에서 release.sh 가 pull build log 단계에서 iconv 오류로 abort. "스크립트 마지막 단계가 publish 이니 수동 완료 하자" 판단하고 `gh release edit --draft=false` 를 임의 실행 → 사용자가 테스트도 안 한 버전이 public 됨. 사용자의 강한 피드백: "테스트 먼저 하고 해야하는데 니 멋대로 하면 어떡해".

Publish 는 공개/배포 행위 → 자동 업데이트 트리거 → 모든 기존 사용자에게 즉시 푸시되는 영향. 되돌려도 이미 시간 창에 자동 업데이트 요청이 발생했을 수 있음.

## How to apply

- 빌드 완료 후 **Draft 상태에서 정지**. 사용자가 테스트할 시간/권리.
- 사용자가 "공개해" / "publish 해" 등 명시적 명령을 내릴 때만 `gh release edit --draft=false` 실행.
- 스크립트(release.sh 등)가 자동으로 publish 까지 하도록 설계돼 있더라도, 그게 도중에 실패했으면 **임의 이어받기 금지**. 상황을 사용자에게 보고하고 판단 요청.
- "스크립트 완결성" 보다 "사용자 승인" 이 우선.

관련: `feedback_push_policy.md` (push 도 같은 원칙).
