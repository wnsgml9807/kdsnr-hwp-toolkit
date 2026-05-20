---
name: project-rhwp-inline-rect-double-emit
description: rhwp table_layout.rs 가 cell paragraph 안 inline Shape (가/나/다 박스) 주변 text 를 paragraph_layout 후 다시 emit → 동일 텍스트 두 y 값에 이중 렌더. social Q03 발문 박스 마지막 줄 글자 겹침 등. table_layout.rs:1700-1722 의 text_before TextRunNode push 제거로 fix.
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

## 증상

- social Q03 발문 박스 마지막 줄 "하여 사회의 쇠퇴와..." 글자가 두 번 그려져 겹침 (strikethrough 같은 시각).
- 같은 패턴 추정: science Q11/Q17, social Q04/Q05 등 inline Shape (treat_as_char=1) 보유 셀.
- OLD bench (2026-05-18) 도 동일 증상 → 장기 잠재 버그.

## probe 매트릭스

| 변경 | 결과 |
|---|---|
| rect 제거 | overlap 사라짐 |
| treat_as_char=1 → 0 | overlap 사라짐 |
| textWrap=TOP_AND_BOTTOM → SQUARE/IN_FRONT_OF_TEXT | overlap 유지 |
| drawText 제거 | overlap 유지 |
| `flowWithText=0 → 1` | overlap 유지 |

→ rect 가 inline (treat_as_char=1) 으로 cell 에 들어가는 자체에서 발생. textWrap/drawText 관계 없음.

## root cause

`vendor/rhwp/src/renderer/layout/table_layout.rs` `layout_table_cells` 안:

- `layout_composed_paragraph` 가 cell paragraph 의 inline 텍스트 (rect 앞/뒤 포함) 를 정상 emit 완료.
- 그 다음 `para.controls` loop 가 `Control::Shape(_)` 에 대해 **다시** text_before 를 추출해 TextRunNode push (line 1677-1724 in original) + layout_cell_shape 호출.
- 같은 텍스트가 두 y/x 로 두 번 렌더됨. SVG 확인: y=349.91 vs y=350.57 두 위치에 26 path 씩 (= 같은 문자 26 개 두 번).

## 패치

`vendor/rhwp/src/renderer/layout/table_layout.rs` 의 Shape 인라인 분기에서 `cell_node.children.push(text_node)` 만 제거. inline_x 누적은 유지 (Shape 들 사이 가로 위치 추적용).

```rust
// 2026-05-19: text_before emit 제거. paragraph_layout 가 이미
// 셀 paragraph 의 text 를 inline 으로 렌더했음. 여기서 또 emit 하면
// 동일 텍스트가 다른 y/x 로 이중 렌더 (사이언스 Q03 (가) 박스 줄 겹침).
// inline_x 누적만 유지 (Shape 들 사이 가로 위치 추적).
if !text_before.is_empty() {
    let char_style_id = composed.lines.first()
        .and_then(|l| l.runs.first())
        .map(|r| r.char_style_id).unwrap_or(0);
    let lang_index = composed.lines.first()
        .and_then(|l| l.runs.first())
        .map(|r| r.lang_index).unwrap_or(0);
    let ts = resolved_to_text_style(styles, char_style_id, lang_index);
    let text_w = estimate_text_width(&text_before, &ts);
    inline_x += text_w;
}
```

## 검증

- social Q03 발문 박스 마지막 줄 정상 표시 (overlap 사라짐) ✅
- 4 과목 × 5 문항 17 PNG 재렌더 0 errors ✅
- 단 science Q11/Q17, social Q04/Q05 잔존 회귀는 별개 패턴 (이번 fix 적용 안 됨)

## How to apply

- `Control::Shape(_)` 인라인 분기에서 text_before emit 절대 추가 금지. paragraph_layout 가 단일 정답.
- 비슷한 패턴 (Picture/Equation 인라인 분기) 도 확인 필요.
- layout_table_cells 와 table_cell_content.rs 모두 cell paragraph 처리 — 어느 경로가 active 인지 확인 후 동일 정정 검토.

## 관련

- [[feedback-rhwp-frontal-assault]] — rhwp 책임이면 직접 패치
- [[project-cell-height-resolve-regression]] — 같은 세션의 또 다른 cell-related fix
