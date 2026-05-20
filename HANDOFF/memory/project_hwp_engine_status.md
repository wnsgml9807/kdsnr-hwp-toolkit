---
name: HWP COM 엔진 현재 상태
description: v0.1.5 기준 엔진 성숙도, 도구 목록, 알려진 제한사항. 2026-04-15.
type: project
---

## 도구 목록 (v0.1.5)

**읽기 (8개):** hwp_info, hwp_read, hwp_read_table, hwp_read_layout, hwp_read_preset, hwp_read_memo, hwp_search_text, hwp_view_image
**편집 (7개):** hwp_edit_text, hwp_edit_equation, hwp_edit_char_style, hwp_edit_para_style, hwp_edit_object, hwp_edit_table, hwp_edit_cell
**구조 (4개):** hwp_insert, hwp_insert_table, hwp_delete, hwp_copy
**기타 (4개):** hwp_apply_preset, hwp_edit_layout, hwp_memo, hwp_preview, hwp_new

## 성숙도

| 영역 | 상태 | 비고 |
|------|------|------|
| 텍스트/수식 편집 | 안정 | 44건 실사용 에러 0 |
| COM 연결/복구 | 안정 | SendMessageTimeout 행 감지, 5초 자동 복구 |
| 크로스 문서 스타일 | 안정 | XML 직접 복사, COM 검증 완료 |
| 프리셋 적용 | 안정 | 크로스 문서 심기 + 이름 충돌 넘버링 |
| 표 읽기 | 안정 | 병합셀 포함 |
| 표 구조 변경 | 약함 | LLM 전략 문제 |
| hwp_new | 수정됨 | save_as 문서 전환 버그 해결 |

## 알려진 제한사항

- `hwp_edit_text` content 덮어쓰기 전용 (find/replace 미구현, 현재 불필요)
- MAX_ARGS_LENGTH = 3000 (content ~2000자 제한)
- 업데이트 체크: 이벤트 기반 (앱 시작 + 절전 복귀 + 창 포커스, 1시간 쿨다운)
- convert_to_pdf.exe: 별도 COM 인스턴스 (hwp_server와 독립)

## 에러 핸들링

- save/undo/redo: _original_path None 방어
- undo 실패 시 스택 복원
- COM Save/SaveAs: 반환값 False → RuntimeError
- worker rebuild: 이전 큐 drain
- _append_style_entry: 태그 미발견 → ValueError

**How to apply:** 새 도구 추가 시 registry.py + tools/__init__.py + server.py + main.js + preload.js + sseHandlers.ts + InlineToolCard.tsx + chat_tool_loop.py + hwp-tools.md + DB seed 필요.
