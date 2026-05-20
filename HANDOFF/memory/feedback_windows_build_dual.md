---
name: Electron x64 빌드 절차
description: hwp_server.exe + convert_to_pdf.exe + Electron 인스톨러 x64 전용 빌드. bat 인코딩/EOL, robocopy 함정, 릴리즈 publish까지.
type: feedback
originSessionId: 3748c570-c0c4-48c2-94fb-72362744eca3
---
빌드는 **x64 전용**. ARM64 빌드 하지 않음.

**Why:** 2026-04-17 v0.1.10 빌드가 `convert_to_pdf.exe` 누락 + ARM Python 혼입 위험으로 깨짐. 아래 절차는 v0.1.11 빌드에서 실전 검증됨.

**How to apply:** Electron 인스톨러 빌드 시 이 절차 그대로. 중간에 실패하면 하단 "함정 모음"부터 먼저 확인.

---

## 토큰 분리

- `desktop/updater-token.json` — read-only PAT (앱 다운로드용, gitignore)
- `desktop/build-token.json` — write PAT (빌드 업로드용, gitignore)
- Windows 환경변수 `GH_TOKEN`에 write 토큰 `setx`로 영구 등록

## 빌드 절차

```
1. 동기화:
robocopy "C:\Mac\Home\Desktop\새 폴더\Project\KSAT Agent\Agent_Streamlit\davinci\frontend" "C:\davinci-frontend" /E /MIR /XD node_modules .next dist dist-electron __pycache__ .git /XF build-win.bat build.log

2. 동기화 검증 (SSH 가능):
ssh wnsgml@localhost -p 2222 "findstr \"키워드\" C:\davinci-frontend\desktop\hwp\api\_xxx.py"

3. 빌드 (C:\davinci-frontend\desktop에서):
C:\Python312-x64\python.exe -m PyInstaller --onefile --clean --name hwp_server hwp_server_entry.py --distpath hwp --workpath build_tmp_x64 --specpath . -y
&& C:\Python312-x64\python.exe -m PyInstaller --onefile --clean --name convert_to_pdf hwp\convert_to_pdf.py --distpath hwp --workpath build_tmp_cvt --specpath . -y
&& cd /d C:\davinci-frontend && npx electron-builder --win --config electron-builder.yml --publish always

4. 릴리즈 노트 + publish:
gh release edit vX.X.X --repo wnsgml9807/kdsnr-davinci-frontend --draft=false --notes "- 변경 1\n- 변경 2"
```

## 빌드 후 체크리스트

- GitHub 릴리즈에 `davinci-setup-vX.X.X-x64.exe`, `.blockmap`, `latest.yml` 3종 모두 있는지 확인 (`latest.yml` 없으면 자동 업데이트 안 됨)
- 릴리즈 노트는 사용자 친화적 한국어 (개발 용어 금지)
- 스킬 문서 업데이트 시 `cd davinci/backend && python -m sql.scripts.seed_skills`

---

## 함정 모음 (실전 삽질에서 배운 것)

### bat 파일 자체 규칙

- **인코딩: CP949 필수.** UTF-8로 저장하면 Windows cmd의 기본 코드페이지(949)와 안 맞아 한글 경로 파싱 실패. `chcp 65001`도 100% 해결 안 됨.
- **EOL: CRLF 필수.** LF이면 cmd가 멀티라인 `if (...)` 블록을 오파싱해서 `'defined'`, `'!!'` 같은 토큰별 에러 쏟아냄.
- **Edit 도구로 수정 금지.** Claude의 Edit 툴은 EOL을 LF로 정규화하거나 `>nul`을 `>/dev/null`로 손상시킴. 수정할 땐 반드시 Python으로 `.encode('cp949')` + `\r\n` 명시해서 Write.

### robocopy /MIR 함정

- **실행 중인 bat + 로그 반드시 `/XF`로 제외.** `/MIR`은 source에 없는 파일을 extra로 판단해 지움. 현재 실행 중인 `build-win.bat`이나 stdout 리다이렉트 중인 `build.log`가 삭제되면 cmd 파일 핸들이 깨져 bat가 다음 단계로 넘어가지 못하고 조용히 죽음.
- **로그 리다이렉트는 sync 대상 밖으로.** `> C:\davinci-frontend\build.log` 말고 `> C:\build.log`.

### Python 경로

- **반드시 `C:\Python312-x64\python.exe`로 full path 고정.** 맨 `pyinstaller` 호출하면 PATH 첫 번째 Python을 씀 → ARM Python 섞일 위험.

### PyInstaller 엔트리

- hwp_server: `hwp_server_entry.py` (sys.path 보정 있음). `run_server.py` 쓰지 마.
- convert_to_pdf: `hwp\convert_to_pdf.py` 직접. 엔트리 래퍼 없음.
- `--clean` 필수 (캐시 오염 방지).
- `--distpath hwp`로 바로 출력해 copy 단계 생략.

### SSH one-liner 한글 경로

- SSH로 Windows cmd에 한글 경로를 직접 넘기면 cmd가 active codepage(949)로 UTF-8 바이트를 오해석.
- 해결: bat 내부 robocopy에 한글 경로를 CP949로 박아두고, SSH에서는 `C:\davinci-frontend\build-win.bat`만 호출.

### 릴리즈

- `electron-builder --publish always`가 올리는 릴리즈는 **Draft 상태**. `gh release edit v... --draft=false`로 publish 해야 자동 업데이트가 감지함.
- 이전 버전 재빌드가 필요하면 릴리즈+태그 같이 삭제: `gh release delete vX.X.X --cleanup-tag --yes`.

---

## 기타

- `__pycache__` robocopy에서 제외됨 → 코드 변경 후 수동 삭제 필요할 수 있음
- SSH로 동기화 검증 가능: `ssh wnsgml@localhost -p 2222`
- scp로 bat만 빠르게 업데이트 가능: `scp -P 2222 build-win.bat wnsgml@localhost:C:/davinci-frontend/build-win.bat`
