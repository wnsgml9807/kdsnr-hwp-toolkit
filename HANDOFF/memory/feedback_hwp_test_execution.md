---
name: HWP/한컴 테스트 실행 분담
description: HWP COM 테스트는 사용자가 직접 실행. 어시스턴트는 커맨드만 제공.
type: feedback
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
HWP/한컴 관련 테스트 (probe_e2e.py, drift, 그 외 COM 사용 스크립트) 는 사용자가 Windows 에서 직접 실행한다. 어시스턴트가 ssh 로 돌리지 말고 커맨드만 전달.

**Why:** 한컴오피스가 GUI 기반이라 SSH 세션에서 띄우면 동작 이상 / 창 제어 불가. 사용자 세션에서 실행해야 COM 연결과 윈도우 제어가 정상.

**How to apply:**
- HWP 관련 테스트 필요 → 실행 커맨드를 복사 붙여넣기 가능한 형태로 제시하고 결과 기다림
- robocopy 등 비-HWP 동기화/빌드는 SSH 로 돌려도 됨
- 결과는 사용자가 붙여넣은 텍스트 또는 `C:\Mac\Home\Documents\hwp\test\e2e\results\*.txt` 를 Read 로 읽어서 확인
