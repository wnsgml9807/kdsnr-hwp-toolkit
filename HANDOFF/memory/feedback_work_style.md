---
name: 작업 스타일 & 인수인계 매뉴얼
description: 이 프로젝트에서 일하는 방식, 환경 설정, 커맨드 패턴, 코드/문서 스타일 규칙.
type: feedback
---

## 작업 환경

### 이중 머신 구조
- **Mac (개발)**: 코드 편집, 서버 실행 (백엔드 FastAPI + 프론트 Next.js)
- **Windows (실행)**: 한글 COM API 실행, Electron 데스크톱 앱, COM 실험
- 공유 폴더: Mac의 데스크톱이 Windows에서 `Z:\Desktop\`으로 접근 가능

### 동기화
Mac에서 수정 → Windows로 robocopy:
```cmd
robocopy "Z:\Desktop\새 폴더\Project\KSAT Agent\Agent_Streamlit\davinci\frontend\desktop\hwp" "C:\davinci-frontend\desktop\hwp" /MIR
```
이 커맨드를 매번 써야 한다. 파일 경로 외우기.

### 서버 실행
백엔드: `cd davinci/backend && uvicorn app.main:app --reload --host 0.0.0.0 --port 8080`
프론트: `cd davinci/frontend && npm run dev`
포트 충돌 시: `lsof -ti:8080 | xargs kill -9` 후 재시작.

### COM 실험 패턴
1단계 (복사): robocopy + 테스트 파일/문서 복사
2단계 (실행): `cd C:\davinci-frontend\desktop && python hwp\test_xxx.py && copy xxx_result.txt "Z:\Desktop\"`
항상 두 커맨드로 나눠서 준다. 한 줄로 합쳐서.
실험 끝나면 테스트 스크립트와 결과 파일 반드시 삭제.


## 코드 스타일

### 주석
- 깔끔하고 담백하게. 기능만.
- "멋진 말 넣으려고 하지 말고."
- 한국어 주석 사용.
- 불필요한 docstring/타입 주석 추가하지 않기.

### 코드
- 추정값 절대 금지. COM 속성명이든 뭐든 반드시 검증 후 삽입.
- "틀리면 말고~" 식 접근 금지. 확인 안 된 건 TODO로 남기고 실험 후 추가.
- 패치 말고 전면 재작성. 주먹구구식 수정 금지.
- 로직은 유지하되 구조를 깔끔하게.

### 스키마 설계 원칙
- Data(is)와 Action(do) 분리.
- 읽기/쓰기 포맷 통일. hwp_item_read에서 읽은 값을 그대로 쓰기에 넣으면 동작.
- 4필드 패턴: apply_exist_*_style / apply_new_*_style.
- 각 도구는 자기 레벨의 스타일만 반환:
  - hwp_para_read → para_style 상세
  - hwp_item_read → char_style/eq_style 상세

### 네이밍
- 필드명은 대칭적으로. apply_exist_char_style / apply_exist_para_style / apply_exist_eq_style.
- COM 액션 이름과 ParameterSet 이름이 다를 수 있음 주의 (CharShape vs ParagraphShape).


## 문서 스타일

### 기술 문서
- 담백하고 기능적인 톤. 전문 용어 쓰려고 하지 않기.
- 의도와 목적, 사용법, 원리를 포함.
- 시행착오를 상세히 기록 (같은 실수 반복 방지).

### 설계 문서 (memory/)
- 시행착오를 "왜 이 구조인가" 형태로 기록.
- COM 실험 결과는 검증된 것만 기록.
- 추측이 아닌 사실만.

### Skills 문서 (hwp-tools.md)
- LLM이 읽는 문서. 예시 중심.
- 반환값 예시를 실제 구조화된 포맷으로 상세히.
- 금지 사항을 명확히.


## 의사결정 방식

- 혼자 결정하지 않고 질문한다.
- 설계 변경이 필요하면 먼저 제안하고 확인받는다.
- 임의로 추상화하거나 용어를 만들지 않는다.
- 심각도를 임의 평가하지 않는다. 증상/원인만 객관적으로.

### 워크플로우 (새 아이템 타입 추가 시)
1. 공식 문서 조사 (davinci/hwpx/docs/ PDF 3개)
2. COM 실험 스크립트 → Windows 실행
3. 검증 결과 기록
4. schema.py 모델 추가
5. parser.py 파싱 로직
6. bridge.py COM 래퍼
7. api.py 액션 추가
8. skills + tools/__init__.py 업데이트
9. DB seed: `cd backend && python -m sql.scripts.seed_skills`
10. 동기화 + 테스트


## 자주 쓰는 커맨드 모음

### Mac
```bash
# 백엔드 실행
cd davinci/backend && uvicorn app.main:app --reload --host 0.0.0.0 --port 8080

# 프론트 실행
cd davinci/frontend && npm run dev

# 포트 정리
lsof -ti:8080 | xargs kill -9; lsof -ti:3000 | xargs kill -9

# DB seed
cd davinci/backend && python -m sql.scripts.seed_skills
```

### Windows
```cmd
:: 동기화
robocopy "Z:\Desktop\새 폴더\Project\KSAT Agent\Agent_Streamlit\davinci\frontend\desktop\hwp" "C:\davinci-frontend\desktop\hwp" /MIR

:: COM 실험 (복사 + 실행 + 결과 복사를 한 줄로)
cd C:\davinci-frontend\desktop && python hwp\test_xxx.py && copy xxx_result.txt "Z:\Desktop\"

:: Electron 앱
cd C:\davinci-frontend\desktop && npx electron-forge start

:: 한글 문서 열어서 테스트
copy "Z:\...\문서.hwpx" "C:\davinci-frontend\desktop\" && start "" "C:\davinci-frontend\desktop\문서.hwpx"
```
