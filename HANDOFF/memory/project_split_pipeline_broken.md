---
name: 분할 파이프라인 손상 — work/0513 이후 산출물 한컴 호환 깨짐
description: 문항 분할 파이프라인이 work/e2e 이후 시점에 손상되어, 분할 hwpx 가 한컴에서 안 열림. 보수 필요.
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
문항/지문 분할 파이프라인 (`kdsnr-hwp-toolkit/work/sdk/` 또는 관련 분할 스크립트) 이 어느 시점부터 손상됨.

## 증상

- `kdsnr-hwp-toolkit/work/0513/pipeline_v2/` 의 분할 hwpx (예: `math/Q01.hwpx`, `korean/S01-03.hwpx`) 가 한컴 macOS 에서 안 열림.
- 마지막 정상 분할 시점: `work/e2e/` 폴더 생성 시점. 그 안의 hwpx 는 한컴에서 정상 동작 (사용자가 .hwp 로 변환 완료).

## 원인 (미확인)

- 어느 시점에 분할 파이프라인 코드 변경으로 인해 한컴 호환성이 깨졌음.
- 후보 위치: `kdsnr-hwp-toolkit/work/sdk/` 의 분할 스크립트, 또는 toolkit 의 atom→unified 변환 로직.

## 해야 할 일

1. e2e 시점의 분할 코드와 현재 코드를 git 으로 diff.
2. 한컴 호환성 깨지는 지점을 찾아 정정.
3. 정정 후 `work/0513/pipeline_v2/` 또는 새 폴더로 재분할 → 한컴 열기 검증.

**Why:** Phase C (layout RE byte-equivalent 검증) 가 분할물에 의존하지 않도록 e2e 의 reference 12쌍을 우선 사용. 그러나 새로운 입력 패턴 검증이 필요할 땐 분할 파이프라인 자체가 정상이어야 함.

**How to apply:** 분할 결과를 사용하는 작업 시작 전, `work/e2e/` 의 것만 사용. e2e 외의 분할물은 손상 가정. 새 분할이 필요하면 먼저 파이프라인 보수.
