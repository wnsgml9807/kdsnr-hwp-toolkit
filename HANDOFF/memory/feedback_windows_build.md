---
name: Windows 빌드 인프라 (SSH, 자동 업데이트)
description: Parallels VM SSH 접속, 자동 업데이트 GH_TOKEN 설정. 실제 빌드 명령은 feedback_windows_build_dual.md 참조.
type: feedback
originSessionId: 3748c570-c0c4-48c2-94fb-72362744eca3
---
빌드 명령/함정은 [Electron x64 빌드 절차](feedback_windows_build_dual.md) 참조. 이 파일은 **인프라/SSH/자동 업데이트** 전담.

## SSH 설정 (완료, 동작 확인)

- 접속: `ssh -p 2222 wnsgml@localhost`
- Parallels 공유 네트워크 + 포트 포워딩 (2222 TCP → VM:22)
- Parallels Pro Edition 필요 (포트 포워딩)
- OpenSSH Server + ED25519 키 인증
- 키 위치: `C:\ProgramData\ssh\administrators_authorized_keys`

**Why:** Mac에서 직접 Windows 빌드/테스트 실행 가능.

**How to apply:** `ssh -p 2222 wnsgml@localhost "명령어"` 로 직접 실행. 빌드 트리거는 `ssh -p 2222 wnsgml@localhost "C:\davinci-frontend\build-win.bat > C:\build.log 2>&1"` 권장 (로그를 sync 밖에 저장).

## 자동 업데이트 (electron-updater)

- **의존성:** `electron-updater` (package.json dependencies)
- **publish 대상:** GitHub Releases (`wnsgml9807/kdsnr-davinci-frontend`, private repo)
- **GH_TOKEN 필요:** Windows 시스템 환경변수로 설정 (최초 1회)
  ```
  setx GH_TOKEN "<write-token>"
  ```
- **빌드 시:** `--publish always` → exe + latest.yml이 GitHub Releases에 Draft로 업로드
- **Draft → publish 전환 필수:** `gh release edit vX.X.X --repo wnsgml9807/kdsnr-davinci-frontend --draft=false --notes "..."` 안 하면 런타임에서 신규 버전을 감지 못 함
- **런타임:** 앱 시작 5초 후 업데이트 체크 → 백그라운드 다운로드 → 다이얼로그 → 재시작
- **app-update.yml:** electron-builder가 빌드 시 자동 생성, GH_TOKEN 포함 → private repo 접근 가능
- **최초 배포:** 자동 업데이트 미포함 버전 → 수동 설치 1회 필요

## 파일 위치 (현행)

- exe 빌드 출력: `C:\davinci-frontend\desktop\hwp\hwp_server.exe`, `convert_to_pdf.exe` (pyinstaller `--distpath hwp`로 바로 출력)
- 인스톨러 출력: `C:\davinci-frontend\dist-electron\davinci-setup-vX.X.X-x64.exe`
- electron-builder 설정: `desktop/hwp/*.exe` ASAR unpack
- 빌드 로그 권장 위치: `C:\build.log` (sync 대상 밖)

## 구버전 빌드 취소 / 재빌드

깨진 릴리즈 버전이 올라간 경우:
```
gh release delete vX.X.X --cleanup-tag --yes --repo wnsgml9807/kdsnr-davinci-frontend
```
→ package.json 버전 bump → 재빌드. 기존 사용자는 다음 버전으로 자동 점프.
