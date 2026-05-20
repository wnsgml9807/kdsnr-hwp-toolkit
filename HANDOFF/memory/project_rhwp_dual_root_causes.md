---
name: rhwp HWPX serializer 의 두 가지 root cause 패치 (2026-05-11)
description: 한 컴 12+ dual-unit `<hp:switch>` 미구현 + inline-only 필터로 인한 control ordering 어긋남. 두 별도 버그를 같은 세션에 해결. 자세한 분석은 toolkit docs 참조.
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
## 핵심 사실

`vendor/rhwp/src/serializer/hwpx/` 의 두 가지 별도 root cause:

### 1. Dual-unit `<hp:switch>` 미구현 (`header.rs`)

- 한 컴 12+ 의 HWPX 스키마는 모든 수치 distance (margin, tabItem pos, lineSpacing FIXED 계열) 를 `<hp:switch><hp:case HwpUnitChar val=N/2><hp:default val=N></hp:switch>` 페어로 출력해야 함.
- 페어 없이 default 만 단독 emit 하면 한 컴 12+ 가 default 를 HwpUnitChar 로 잘못 해석 → 2× 스케일 거대 탭 / 들여쓰기 확대.
- Fix: `write_tab_item_switch` 신규 + `write_para_pr` 의 margin/lineSpacing 을 hp:switch 페어로 wrap. margin 자식은 `hc:` 접두어.

### 2. Inline-only filter 기반 slot indexing 으로 control ordering 어긋남 (`section.rs`)

- `render_run_content` / `render_runs_split_by_char_shapes` 가 `slots = controls.filter(is_inline_slot)` 로 만든 뒤 PARA_TEXT 의 8-unit gap 마다 `slots[i]` 출력.
- non-inline control (ColDef `0x02 'cold'`, SectionDef, Header, Footnote, AutoNum 등) 이 paragraph 에 있으면 그 컨트롤의 8-unit gap 에 다음 inline 컨트롤이 잘못 emit → equation 등 인라인 객체가 직전 텍스트보다 한 자리 앞당겨짐.
- Fix: `slots` 필터 제거 → `all_controls` 전체 iterate, inline 이면 render, non-inline 이면 silent skip (위치는 그대로 8-unit 진행).

## Why (각각 별도 root cause)

- **1번**: rhwp 가 원래 한 컴 2020 (구버전) 타깃이었고 한 컴 12+ 의 새 스키마를 모름. 한 컴 2020 은 default-only 도 관용 처리, 12+ 는 엄격 해석.
- **2번**: 단순 코드 가정 오류. parser 는 `controls[]` 를 PARA_TEXT 순서대로 올바르게 채우지만 serializer 가 필터링하면서 위치 정보 손실. `slots` index 와 binary 의 8-unit gap index 가 어긋남.

## How to apply

- rhwp upstream upgrade 후 검증 필수: `grep -c '<hp:switch' Contents/header.xml` 다수, `<hh:intent` 없음 (모두 `<hc:intent>` 페어 안), Q29_3 같은 cell-인 equation paragraph 의 ordering 점검.
- 새 numeric distance 속성 추가 시 hp:switch 페어 패턴 따를 것.
- inline 가 아닌 새 컨트롤 종류 추가 시 `is_hwpx_inline_slot` 만 보지 말고 ordering drain 로직 (`section.rs`) 도 확인.
- 상세 분석: `kdsnr-hwp-toolkit/docs/HWPX_DUAL_UNIT_SCHEMA.md` (§1-7 dual-unit, §8 ordering).
- splitter.py 에서 `_pair_tabpr_stops` / `_wrap_paraPr_margin_in_switch` / `_merge_visually_continuous_cell_paragraphs` 모두 제거됨 (위 두 fix 로 불필요).
