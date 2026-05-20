---
name: 디버그 산출물 위치
description: /tmp 금지. 사용자가 IDE 에서 확인할 수 있는 work/debug/ 안에 저장
type: feedback
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
디버그용 PNG/PDF/로그 산출물은 `/tmp` 에 두지 말 것.

**Why:** 사용자가 `/tmp` 경로의 파일을 IDE 에서 직접 열어 확인하기 불편하다. work/ 안이라야 finder/탐색기/IDE 에서 바로 보임.

**How to apply:** kdsnr-hwp-toolkit 작업 시 임시 산출물 (시각 비교용 PNG, 진단 PDF, dump 로그 등) 은 `kdsnr-hwp-toolkit/work/debug/` 폴더에 저장. before/after 같은 비교 산출물도 같은 폴더에 식별 가능한 이름 (예: `q20_before_fix.png`, `q20_after_fix.png`). 빌드 산출물은 work/e2e/ 또는 work/GT/ 등 기존 위치.
