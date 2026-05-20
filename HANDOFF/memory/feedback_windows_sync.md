---
name: Windows Electron 동기화·실행 자동화
description: 어시스턴트가 SSH+schtasks 로 Mac에서 Windows Electron sync/start/stop 모두 자동 실행 가능.
type: feedback
originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---
## 역할 분담

- **싱크**: `./dev/win-sync.sh` — SSH → robocopy 로 Mac → Windows.
- **Electron 실행/종료**: `./dev/win-electron-start.sh` / `win-electron-stop.sh` — 어시스턴트가 직접 실행 가능.
- **COM API 테스트 (사용자)**: HWP/한컴 GUI 기반이라 사용자가 직접.

**Why:** SSH 세션은 Windows Services session 0 에 속해서 거기서 직접 spawn 한 GUI 는 사용자 데스크톱(Console session 1)에 안 뜬다. 이전엔 사용자가 직접 실행하는 걸로 분담했지만, 2026-04-27 검증으로 `schtasks /Run /IT` 우회가 동작 확인됨 — 어시스턴트가 launch 까지 자동화 가능.

**How to apply:** 사용자가 "electron 띄워" 같은 요청을 하면 `./dev/win-electron-start.sh` 실행. 종료는 `./dev/win-electron-stop.sh`. 단 한컴/COM 통합 테스트 단계는 GUI 조작이 필요해서 여전히 사용자 영역.

## win-electron-start.sh 동작

1. SSH → `taskkill /F /IM electron.exe` (기존 정리)
2. SSH → `schtasks /Create ... /IT` 로 인터랙티브 작업 등록
3. SSH → `schtasks /Run` 으로 발동 → Console session 1 에 cmd spawn
4. cmd 가 setx 로 영구 등록된 DAVINCI_URL/ELECTRON_DEV 자동 로드 후 `npm run electron:start`

## Sync (`./dev/win-sync.sh`)

내부적으로 SSH 로 robocopy:
- src: `C:\Mac\Home\Desktop\새 폴더\...\frontend` (공유 폴더 마운트)
- dst: `C:\davinci-frontend`
- desktop/ 은 `/MIR`, 루트 파일 3개 (package.json, forge.config.js, electron-builder.yml)
- 실행 중 .exe 는 `/XF` 로 스킵
- robocopy rc<8 정상, rc≥8 만 실제 에러

## Windows 측 1회성 영구 세팅 (사용자가 한 번만)

```cmd
powershell -Command "Set-ExecutionPolicy -Scope CurrentUser RemoteSigned -Force"
setx DAVINCI_URL "http://macbookpro:3000/s/chat"
setx ELECTRON_DEV "1"
```

이후 어시스턴트가 SSH 로 spawn 한 cmd 가 자동 로드.

## Electron OAuth — dev 모드 callback 호스트

Windows Electron 이 macbookpro:3000 을 로드하므로 OAuth callback 도 macbookpro 로 돌아와야 함. 둘 다 필요:

1. **WorkOS dashboard** → Authentication → Redirect URIs 에 `http://macbookpro:3000/callback` 등록
2. **frontend/.env.local** 에 `NEXT_PUBLIC_WORKOS_REDIRECT_URI=http://macbookpro:3000/callback`

이후 `mac-frontend-start.sh` 재기동.

세션 쿠키 캐시 때문에 ERR_CONNECTION_REFUSED 가 뜨면 Windows 의 `%APPDATA%\davinci` 삭제 후 재시작.

## 금기

- Z: 드라이브 공유 폴더에서 electron-forge / npm install 직접 실행 금지
- COM API 테스트(한컴 자동화)를 어시스턴트가 직접 실행하지 말 것 — GUI 조작 필요
- 커맨드 복사/실행/결과회수 3개를 한 줄로 합치지 말 것
