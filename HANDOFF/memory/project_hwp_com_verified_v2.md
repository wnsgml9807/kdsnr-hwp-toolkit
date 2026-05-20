---
name: HWP COM 검증 결과 v2
description: COM 연결/SetTextFile/FullName/.tmp 동작에 대한 v1~v11 테스트 검증 결과 종합.
type: project
---

## COM 연결 (test_com_connection v1~v11)

- Dispatch = 같은 프로세스 공유 (PID 동일, v3 #03). launch() 중복 호출 안전.
- ROT↔Dispatch = 동일 객체 (v5 #07 양방향 교차 확인).
- 참조 해제(None+gc)로 프로세스 종료 불가 (v3 #04). 좀비는 Python 프로세스 종료 시 같이 사라짐.
- 강제 종료 후 → `com_error: RPC 서버를 사용할 수 없습니다` → is_connected=False → launch 복구 정상 (v3 #01, v11 #06).
- 30초 idle 후 연결 유지 (v3 #07).

## ensure_connected 전략

```
1. is_connected() → True면 리턴
2. ROT에서 기존 한글 찾기 → 있으면 연결
3. 없으면 launch() (새 인스턴스 생성)
```

activate()에서 사용. list_documents/list_external_documents는 ROT only 유지.

## SetTextFile

- `SetTextFile(data, format, option)` — data는 **XML 문자열** (파일 경로 아님, COM 문서 확인).
- 반환값 1 = 성공. 수정된 XML도 정상 반영됨 (v9 #01~05 전부 성공).
- 속도: avg 24ms (v9 #06).
- FullName이 .tmp로 변경됨 — COM 제약, SaveAs로도 복원 안 됨 (조건부 복원되기도 함).
- 네이티브 Undo 불가 — SetTextFile/SaveAs→Open 둘 다 동일 (v10 #03~06).

## .tmp 방어

- `list_external_documents`: .tmp FullName을 `_original_path`로 매핑 (v11 #01 검증).
- `syncWithExternalDocs` (프론트): .tmp 경로를 싱크 비교에서 제외.
- `DocsDock`: 파일 찾기 메뉴에서 .tmp 필터링.

## find_table_in_xml

- HEADER/FOOTER/FOOTNOTE 내 TABLE 제외 (SECTION 서브 블록 skip).
- 중첩 TABLE 포함 순차 카운팅 (파서와 동일).

**Why:** 셀 삽입이 잘못된 TABLE에 들어가던 근본 원인 2가지.

**How to apply:** table_id 관련 로직 수정 시 파서와 find_table_in_xml의 카운팅이 일치하는지 확인.
