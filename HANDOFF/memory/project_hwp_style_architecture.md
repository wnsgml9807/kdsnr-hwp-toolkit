---
name: HWP 스타일 복사 아키텍처
description: Display/Apply 경로 분리, XML 직접 복사, 프리셋 도구 분리. 2026-04-15 재설계.
type: project
---

## 아키텍처 (2026-04-15 재설계)

**Display (LLM용):** raw dict → `to_basic()`/`to_full()` → pt 단위 schema → JSON
**Apply (exist):** 소스 XML → 요소 추출 → 대상 XML에 삽입 (변환 없음, 완벽 재현)
**Apply (new):** LLM pt 값 → `pt_to_hwpunit()` → XML

**Why:** 이전에 `_resolve_para_style()`이 raw dict(HWP 단위)를 반환 → edit 함수가 pt로 가정하고 `pt_to_hwpunit()` 적용 → ×100 이중 변환 버그. Indent -2260 → -226000, BreakNonLatinWord/Condense/TabDef 누락.

## 도구 분리

| 도구 | 역할 |
|------|------|
| `hwp_edit_para_style` | 문단 서식 (exist 풀좌표 or new 직접) |
| `hwp_edit_char_style` | 글자 서식 (exist 풀좌표 or new 직접) |
| `hwp_apply_preset` | 프리셋 적용 (para+char 한번에, cross-doc 지원) |

- 프리셋 이름을 `hwp_edit_para_style`에 넣으면 ValidationError → "hwp_apply_preset 사용하세요"
- `ParagraphDef`(hwp_insert)에서도 동일하게 str 프리셋 거부

## 크로스 문서 리매핑

ParaShape/CharShape XML 직접 복사 시:
- BorderFill (1-based) 리매핑
- TabDef (0-based) 리매핑
- FontFace 리매핑 (`_remap_fontfaces` + `_remap_fontid_in_charshape`)

프리셋 크로스 문서 시:
- STYLE + ParaShape + CharShape 모두 대상에 심기
- 이름 충돌: 같은이름+같은속성 → 재사용, 같은이름+다른속성 → name-1, name-2 넘버링
- 소스 캐시의 `presets_map`으로 Style Id 기반 검색 (Name 불일치 우회)

## resolve 함수 쌍

| 함수 | 용도 | 반환 |
|------|------|------|
| `_resolve_para_style(ref)` | Display | raw dict (HWP 단위) |
| `_resolve_para_style_source(ref)` | Apply | `(source_xml \| None, ps_id)` |
| `_resolve_char_style(ref)` | Display | raw dict |
| `_resolve_char_style_source(ref)` | Apply | `(source_xml \| None, cs_id)` |

## COM 테스트

테스트 파일: `/Users/wnsgml/Documents/hwp/test/`
- `test_cross_doc_style_issue01.py` — 크로스 문서 스타일 복사 검증 (ParaShape + CharShape + FONTID 대조)
- `test_apply_preset_cross_doc.py` — 크로스 문서 프리셋 적용 검증
- `sys.path.insert(0, os.getcwd())` 사용 (테스트 디렉토리 hwp/ 패키지 충돌 방지)

**How to apply:** 스타일 관련 코드 수정 시 이 아키텍처를 따를 것. display/apply 경로를 섞지 않기.
