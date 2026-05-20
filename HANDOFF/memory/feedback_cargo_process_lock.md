---
name: cargo-target-lock
description: cargo build/test 가 진행 안 되고 무한 대기 시 → 백그라운드에 죽지 않은 cargo 여러 개. pkill -9 로 정리 후 재시도.
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# cargo 무한 로딩 = 죽은 cargo 프로세스의 target lock 충돌

## 증상

- `cargo test --lib` 나 `cargo build --lib` 가 1분, 2분, 5분 동안 출력 없이 대기
- 컴파일이 단순 변경에도 끝없이 진행 안 됨
- 출력 파일이 0 bytes 로 멈춤

## 원인

`target/` 디렉토리의 build lock 을 죽지 않은 백그라운드 cargo 프로세스가 점유 중. 한 세션에 백그라운드 cargo 를 여러 번 실행하면 (run_in_background, 또는 timeout 으로 중단된 경우), 프로세스들이 좀비/대기 상태로 남아 새 cargo 가 락 획득 못 함.

**Why**: cargo 는 단일 cargo 인스턴스만 동시에 target 빌드 가능하도록 file lock 사용. 죽지 않은 프로세스가 락을 계속 잡으면 다른 cargo 가 무한 대기.

**How to apply**:

1. **증상 발견 시 즉시**: `ps auxw | grep -E "cargo|rustc" | grep -v grep` 으로 죽지 않은 프로세스 확인
2. **정리**: `pkill -9 -f cargo; pkill -9 -f rustc; sleep 2`
3. **재시도**: foreground 로 단일 cargo 만 실행

## 예방

- `run_in_background: true` 로 cargo 띄울 때 주의 — 한 번에 1 개만 활성화. 중단되면 즉시 pkill
- timeout 으로 cargo 중단 시 → 즉시 pkill (cargo 가 deadlock 상태로 남음)
- 5+ cargo 동시 실행 사례: 같은 turn 에 fg/bg/timeout 으로 3-5회 띄우면 발생 가능
- 빌드만 검증 (test 실행 X) 시 `cargo build --lib` 만 사용

## 관련

- [작업 스타일](feedback_work_style.md) — 일반 워크플로우
- [병렬 세션](feedback_parallel_sessions.md) — 다중 claude 세션 충돌 (cargo lock 별개 문제)
