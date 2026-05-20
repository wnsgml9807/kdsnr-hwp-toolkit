---
name: COM 테스트 워크플로우
description: Windows COM 테스트 실행 방법, SSH 가능, 결과 파일, sys.path 주의사항.
type: feedback
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
## 테스트 실행

SSH: `ssh wnsgml@localhost -p 2222` — **비-HWP 스크립트 (robocopy, 파일 조작) 전용**.
HWP COM 사용 테스트는 사용자가 Windows 세션에서 직접 실행 (GUI 기반, SSH 창에서 띄우면 창 제어 실패).
→ 상세: `feedback_hwp_test_execution.md`

테스트 파일 위치: `/Users/wnsgml/Documents/hwp/test/` (Mac) = `C:\Mac\Home\Documents\hwp\test\` (Windows)
HWP 테스트 파일: `C:\Users\wnsgml\Desktop\test_hwp\`
이슈 파일: `davinci/debug_notes/issue/issue_01/` (1번.hwp, 통합과학_양식.hwp)

**실행 커맨드 (사용자가 Windows 셸에서 실행):**
```
cd C:\davinci-frontend\desktop && C:\Python312-x64\python.exe -X utf8 C:\Mac\Home\Documents\hwp\test\test_xxx.py
```

**결과 파일:** 테스트 스크립트가 `_result.txt`로 저장. Mac에서 Read로 확인.
원본 파일은 **tempdir에 복사**해서 사용. 원본 수정 금지.

## sys.path 주의사항

`C:\Mac\Home\Documents\hwp\`에 `__init__.py`가 있어서 Python이 이걸 `hwp` 패키지로 인식.

**반드시 `sys.path.insert(0, os.getcwd())` 사용:**
```python
sys.path.insert(0, os.getcwd())  # CWD = C:\davinci-frontend\desktop
```

`os.path.dirname(__file__)` 기반 경로는 잘못된 hwp 패키지를 import함.

**Why:** 2026-04-15 크로스 문서 스타일 테스트에서 구 코드가 계속 실행되는 원인이 이것이었음.

## 동기화 후 코드 검증

robocopy가 `__pycache__`를 제외하므로, 이전 .pyc 캐시가 남아있을 수 있음.
```
ssh wnsgml@localhost -p 2222 "findstr \"키워드\" C:\davinci-frontend\desktop\hwp\api\_xxx.py"
```
의심되면: `rd /s /q C:\davinci-frontend\desktop\hwp\api\__pycache__`
