---
name: HWP staleness 감지 + 네이티브 Ctrl+Z 탐색 기록
description: 2026-04-19 세션에서 확정된 내용. 다음 세션 구현 대기.
type: project
originSessionId: 2026-04-19
---

## 상세 문서 (정보 손실 0)

**파일:** `davinci/frontend/desktop/hwp/docs/NATIVE_UNDO_AND_STALENESS_2026-04-19.md`

모든 테스트 결과, hash 값, 측정 수치, 확정 사항, 설계 결정, 구현 계획 포함. 다음 세션 시작 시 이 파일 정독 후 진행.

## 핵심 확정 (요약)

### staleness 감지 → `doc.Modified` 플래그 기반으로 확정

- HWP COM `IXHwpDocument.Modified` (read/write property) 사용 — HwpAutomation.pdf page 47 확인
- 우리 write 후 `doc.Modified = False` 리셋 → 다음 write 진입 시 `doc.Modified == True` 면 사용자 외부 편집
- **XML hash 방식 폐기** (Modified 가 O(1) + 완벽 정확)
- 11/0 PASS (test_staleness_modified_flag.py)
- **set_xml 은 Modified 안 건드림** 확정 (test_staleness_final_confirm.py [A])
- HAction 경로 (Paste/Cut 등) 는 Modified=True set → 일관성 위해 모든 write 도구 말미 리셋 권장

### Ctrl+Z 네이티브 undo 1회 복원 → 포기 확정

- SetTextFile (set_xml) 은 HWP 네이티브 undo 스택에 쌓이지 않음 (구조적 배제)
- SetTextFile insertfile 옵션, Insert 메서드, HAction.Execute("InsertFile"), SelectAll+변형, 연속 action — 모두 실패
- HWP 에 compound undo API 없음
- **→ 우리 `_undo_stack` (XML snapshot) 유지**

### 부차 긍정 발견

- **사용자 HAction 편집과 우리 set_xml 은 독립된 undo 스택** (test_undo_setxml.py [B])
- 우리 set_xml 이후 사용자 편집 시 `doc.Undo(1)` 로 **사용자 편집만 pin-point 되돌리기 가능** (우리 set_xml 유지)

### UX 설계 (Electron 레벨 Ctrl+Z 가로채기)

```
사용자 Ctrl+Z → Electron 가로챔 → Davinci 분기:
  doc.Modified == True: HWP 에 전달 (사용자 편집 취소)
  _undo_stack 비어있음: HWP 에 전달
  _undo_stack 있고 Modified == False: Davinci 커스텀 undo (set_xml 이전 XML)
```

사용자 체감: "Ctrl+Z 가 항상 자연스럽게 작동".

## 다음 세션 구현 Phase

1. **Phase 1** — HwpBridge + HwpSession 에 Modified 체크/리셋 주입
   - bridge 에 get_modified_flag / set_modified_flag 추가
   - `_check_staleness()` 헬퍼
   - `_xml_batch` 시작/끝에 주입
   - 기타 write 경로 (hwp_copy, hwp_apply_preset, hwp_edit_layout, hwp_memo, hwp_insert_table) 에도 주입
   - activate 시 강제 False 리셋
   - `_undo` 말미에 리셋

2. **Phase 2** — Electron Ctrl+Z 가로채기 구현
   - main.js: window-level keyDown hook (또는 globalShortcut)
   - IPC → renderer → hwp API 분기 호출

3. **Phase 3** — E2E 테스트

**How to apply:** 다음 세션 시작 시 `davinci/frontend/desktop/hwp/docs/NATIVE_UNDO_AND_STALENESS_2026-04-19.md` 를 전체 정독한 후 Phase 1 부터 순차 구현.

## 테스트 파일 위치

- Mac: `/Users/wnsgml/Documents/hwp/test/`
- Windows: `C:\Mac\Home\Documents\hwp\test\`
- 12개 테스트 + 결과 txt 모두 존재. 재검증 필요 시 그대로 실행 가능.
