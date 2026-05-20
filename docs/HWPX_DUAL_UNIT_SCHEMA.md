# HWPX Dual-Unit Schema — 한컴 12+ 거대 탭 / 들여쓰기 2× 스케일 버그 근본 해결

본 문서는 2026-05-11 추적·해결된 `rhwp` HWPX serializer 의 dual-unit 미구현 이슈를 다룬다. 이 한 가지 원인이 분리되어 보이던 세 증상 (거대 탭, 들여쓰기 2× 확대, 셀 내부 문단의 어색한 줄바꿈) 의 공통 뿌리였음을 기록한다.

---

## 1. 한 컴 HWPX 의 dual-unit 직렬화 규약

### 1.1 두 개의 길이 단위

HWPX 스키마는 두 종류의 길이 단위를 정의한다.

| 단위 | 정의 | 사용 위치 |
|---|---|---|
| **HwpUnit** | 절대 단위. `1 inch = 7200 HwpUnit`. | 레거시 (HWP 5.0 binary 와 동일 스케일) |
| **HwpUnitChar** | 글자 기준 상대 단위. **물리적으로 동일한 거리를 HwpUnit 의 1/2 값으로 표현**. | 한 컴 2016+ 가 추가한 새 단위 |

`HwpUnitChar` 라는 이름과 달리 실측상 폰트 크기와 무관하다 — 단순히 **HwpUnit 의 절반 스케일**이다. 즉 동일 물리 거리를 두 표기로 적으면 정확히 `HwpUnitChar = HwpUnit / 2`.

### 1.2 `<hp:switch>` 페어로 두 단위 동시 표현

한 컴 12+ 가 발행하는 HWPX 는 모든 numeric distance 속성 (paragraph margin, line spacing, tab stop) 을 두 단위로 동시에 적는다. `<hp:switch>` / `<hp:case>` / `<hp:default>` 컨트롤로 묶어 readers 가 자기가 이해하는 분기를 선택한다.

```xml
<hp:switch>
  <hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar">
    <hh:tabItem pos="2020" type="LEFT" leader="NONE" unit="HWPUNIT"/>
  </hp:case>
  <hp:default>
    <hh:tabItem pos="4040" type="LEFT" leader="NONE"/>
  </hp:default>
</hp:switch>
```

핵심 규칙:
- **`<hp:case>` 분기**: `required-namespace` 가 `…/HwpUnitChar`. 한 컴 12+ 가 이걸 읽는다. `pos` 값은 **물리 거리의 HwpUnitChar 표기 (= HwpUnit/2)**. `unit="HWPUNIT"` 속성이 명시되어 있어도 값 자체는 HwpUnitChar 의 절반 스케일 의미로 해석된다.
- **`<hp:default>` 분기**: 구버전 fallback. `pos` 값은 **HwpUnit 표기 (절대)**. `unit` 속성 생략.
- 두 값의 비는 항상 `case = default / 2`. 정확히 이 비율이 아니면 한 컴 12+ HWPX validator 가 거부한다 (실험적으로 확인).

paraPr margin 도 같은 형태로 묶인다. margin 자식 요소는 `hc:` 접두어를 사용한다 (`<hc:intent>`, `<hc:left>` 등) — `hh:` 가 아니다.

```xml
<hp:switch>
  <hp:case hp:required-namespace=".../HwpUnitChar">
    <hh:margin>
      <hc:intent value="-2750" unit="HWPUNIT"/>
      <hc:left value="0" unit="HWPUNIT"/>
      ...
    </hh:margin>
    <hh:lineSpacing type="PERCENT" value="130" unit="HWPUNIT"/>
  </hp:case>
  <hp:default>
    <hh:margin>
      <hc:intent value="-5500" unit="HWPUNIT"/>
      ...
    </hh:margin>
    <hh:lineSpacing type="PERCENT" value="130" unit="HWPUNIT"/>
  </hp:default>
</hp:switch>
```

`<hh:lineSpacing>` 의 `value` 는 type 에 따라 다르다:
- `type="PERCENT"`: 단위 없는 백분율 → 양쪽 분기 동일 값.
- `type="FIXED" | "AT_LEAST" | "BETWEEN_LINES"`: HwpUnit/HwpUnitChar 거리 → 양쪽 분기 N/2 vs N.

### 1.3 단독 분기 출력의 함정 (= 본 버그의 핵심)

`<hp:switch>` 페어 없이 plain `<hh:tabItem pos="N"/>` 만 출력하면 한 컴 12+ 는 그것을 **HwpUnitChar 표기로 잘못 해석**하여 `N` 을 1× 스케일 그대로 화면에 반영한다. IR (그리고 HWP binary) 가 HwpUnit 2× 스케일이라 화면은 의도 거리의 정확히 **2배** 가 된다.

즉:
- 정상 (페어 출력): `case=N/2` 를 읽어 `N/2 × 2 = N` 의 물리 거리 렌더링 = HWP binary 동일.
- 비정상 (단독 default): `pos=N` 을 HwpUnitChar 로 잘못 해석해 `N × 2 = 2N` 거리 렌더링 = 의도 2배.

자료 출처: rhwp parser 의 case 처리 코드가 case 값을 `× 2` 하여 IR 로 정규화 (`vendor/rhwp/src/parser/hwpx/header.rs` 의 `parse_para_shape_switch` 와 `parse_tab_def` — `val * 2`, `position *= 2` 주석에 "HwpUnitChar 값은 실제 HWPUNIT (1× 스케일) 이므로 HWP 바이너리와 동일한 2× 스케일로 변환"). serializer 가 default-only 로 출력하면 reader 입장에선 "case 가 없으니 default 를 case 처럼 읽어야 하나?" 라는 모호함이 발생하고 한 컴 12+ 는 그걸 1× 스케일로 처리해 2× 스케일 확대로 이어진다.

---

## 2. IR (Intermediate Representation) 스케일 계약

rhwp 의 IR (`ParaShape.indent`, `TabItem.position`, 등) 은 **항상 HwpUnit 의 2× 스케일** 로 둔다. parser 가 case 와 default 두 분기를 모두 IR 스케일로 정규화해서 저장한다.

| 입력 분기 | parser 동작 | IR 저장값 |
|---|---|---|
| `<hp:default> pos="N"/>` | 그대로 사용 | `N` |
| `<hp:case (HwpUnitChar)> pos="M"/>` | `× 2` 후 사용 | `2M` |
| `<hh:tabItem pos="K"/>` (no switch) | 단위 불명 → 그대로 (legacy 호환) | `K` |
| HWP 5.0 binary 의 stop position | 그대로 (이미 2× 스케일) | `K` |

→ serializer 는 항상 IR 을 출력 시 `case = IR/2`, `default = IR` 로 페어링.

`PERCENT` 타입 lineSpacing 만 예외 — 백분율이라 단위 무관 → case·default 동일 값.

---

## 3. rhwp 의 동작 원리 (관련 부분)

### 3.1 Parser → IR

- `vendor/rhwp/src/parser/hwpx/header.rs` — HWPX header.xml 의 paraPr / tabPr 를 IR 로 파싱.
  - `parse_para_shape_switch` (line ~594): `<hp:switch>` 안에서 `case` (HwpUnitChar) 우선, 없으면 `default`. case 의 margin·lineSpacing(FIXED계열) 값은 `× 2` 정규화.
  - `parse_tab_def` (line ~968): `<hp:case>` tabItem pos `× 2`, `<hp:default>` tabItem pos 그대로.
- `vendor/rhwp/src/parser/doc_info.rs` — HWP 5.0 binary 의 DocInfo 레코드 파싱. 이미 HwpUnit 2× 스케일이라 변환 없음.

### 3.2 Serializer (HWP → HWPX 방향)

- `vendor/rhwp/src/serializer/hwpx/header.rs` — IR → HWPX header.xml 생성.
  - `write_tab_pr` → `write_tab_item_switch` (신규, 2026-05-11): 각 tabItem 을 `<hp:switch>` 페어로 emit.
  - `write_para_pr`: margin + lineSpacing 을 `<hp:switch>` 페어로 emit. margin 자식은 `hc:` 접두어. case 분기 값은 IR/2.
  - helper: `write_margin_block`, `write_line_spacing`.

### 3.3 IR ↔ HWPX round-trip 안정성

parser 와 serializer 의 스케일 변환이 정확히 역상이라 IR → HWPX → IR 라운드트립 시 값이 유지된다. case-only / default-only / no-switch 어느 형태로 들어와도 IR 은 2× 스케일로 정규화되고, 출력은 항상 양쪽 페어.

### 3.4 이전 상태 (버그)

수정 전 serializer 는 단일값을 그대로 `<hh:tabItem>` / `<hh:margin>` 자식으로만 emit 했다. 한컴 2020 (rhwp 가 원래 타깃) 은 이를 관용적으로 처리했지만 한 컴 12+ (사용자가 사용하는 버전) 는 단독 default 를 HwpUnitChar 로 해석해 2× 스케일 확대를 일으켰다.

---

## 4. 우리 파이프라인 동작 원리

`kdsnr-hwp-toolkit` 은 한 컴 HWP/HWPX 시험지 입력을 받아 문항 단위 .hwpx 로 분할·재조립한다.

```
입력 (.hwp/.hwpx)
  │
  ▼
hwp_to_hwpx (rhwp)             ← 입력이 .hwp 일 때 HWPX 로 변환
  │
  ▼
read → Document (IR)           ← codec.read 가 HWPX 파싱
  │
  ▼
unwrap_wrappers / unwrap_meta_tables / split_fused_paragraph  (extract)
  │
  ▼
detect_units → unit 별 paragraph index 범위
  │
  ▼
for each unit:
    template = read(template_bytes)        ← 과목별 templet hwpx
    merged_template, id_maps = merge_styles(template, src)
    sanitize_style_refs(merged_template)   ← rhwp 가 langID 를 paraPrIDRef 슬롯에 흘리는 quirk 보정
    classify + rewrite_paragraph + apply_atom
    replace_section_body(merged_template, transformed_paragraphs)
    out_doc = normalize_styles + strip_templet_tabstopval
    out_bytes = write(out_doc)                       ← rhwp serializer (HWPX emit)
    out_bytes = _replace_meta_in_zip(version.xml, settings.xml)  ← rhwp 가 너무 옛 메타 emit → templet 의 한컴 12+ 호환 메타로 교체
  │
  ▼
출력 (Q01.hwpx, Q02.hwpx, …)
```

### 4.1 splitter.py 에 남은 보정 (legitimate, NOT heuristics)

| 함수 | 역할 |
|---|---|
| `_sanitize_style_refs` | rhwp 가 `<hh:style>` 의 paraPrIDRef 등에 langID(1042) 를 흘림 → 유효 범위 밖이면 null sentinel 로 sweep |
| `_replace_meta_in_zip` | rhwp 의 version.xml / settings.xml 이 한컴 12+ 가 거부하는 옛 버전 → templet 의 canonical 메타로 교체 |
| `_strip_templet_tabstopval` | templet 의 section default tabStopVal=4000 이 src 기대치와 다름 → 제거 |
| `_refuse_jimun_inline_box` | 국어 inline box 재결합 (split→atom→re-fuse) |
| `_merge_leading_structural_into_next` | 국어 templet 의 빈 첫 줄 제거 |
| `_find_spacer_after_set_header` | 국어 set_header 다음 반줄 spacer 삽입 |

### 4.2 제거된 휴리스틱 (rhwp 패치로 불필요해진 것)

| 함수 (제거됨) | 원래 우회하려던 증상 | 근본 원인 |
|---|---|---|
| `_pair_tabpr_stops` | 거대 탭 (Q17/Q18 의 "교사 :" 양쪽) | rhwp 가 tabItem 을 hp:switch 페어 없이 default-only 로 emit → 한 컴 12+ 2× 스케일 |
| `_wrap_paraPr_margin_in_switch` + `_wrap_header_paraPr_switch` | 들여쓰기 / margin 2× 확대 | rhwp 가 paraPr margin 을 plain `<hh:intent>` 등으로 emit → 한 컴 12+ 가 받아들이지 않거나 잘못 해석 |
| `_merge_visually_continuous_cell_paragraphs` + `_LIST_MARKER_PREFIXES` | 셀 안 paragraph 의 어색한 줄바꿈 ("세계 / 최고" 따로 떨어짐 등) | 사실은 paraPr/tabPr 의 스케일 깨짐이 indent tab 위치를 어긋나게 만든 부작용. 두 paragraph 가 정상 페어 스케일을 만나면 GT 처럼 자연스럽게 흐른다. |

세 휴리스틱이 모두 동일한 dual-unit 미구현 의 별도 증상이었음.

### 4.3 여전히 정의되어 있지만 호출 안 하는 함수

`_distribute_to_justify`, `_shorttabs_to_space`, `_zero_inline_tab_widths`, `_drop_tabpr_for_cell_tabs` — 과거 probe 로 추가된 후처리. 현 상태에서 호출 안 됨. 향후 정말 필요 없음이 확정되면 추가 제거 가능.

---

## 5. 진단 / 재현 절차

### 5.1 한 컴이 GT 를 어떻게 출력하는지 확인

원본 .hwp 를 한 컴에서 열어 `다른 이름으로 저장` → HWPX. 이게 GT (ground truth). GT 의 header.xml 을 보면 항상 hp:switch 페어가 나온다.

```bash
unzip -q social_test_input_2.hwpx -d /tmp/gt
grep -c '<hp:switch' /tmp/gt/Contents/header.xml   # 수십 ~ 수백 개
```

### 5.2 우리 출력 검증

```bash
unzip -q work/e2e/.../Q17.hwpx -d /tmp/ours
# paraPr 안에 hp:switch 가 모두 들어있는지
python3 -c "
import re
hdr = open('/tmp/ours/Contents/header.xml').read()
paraprs = re.findall(r'<hh:paraPr\b.*?</hh:paraPr>', hdr, re.DOTALL)
print(f'paraPr 중 hp:switch 미포함 = {sum(1 for p in paraprs if \"<hp:switch\" not in p)} / {len(paraprs)}')
print(f'plain <hh:intent> 발생 = {hdr.count(\"<hh:intent\")}')   # 0 이어야 함
"
```

검증 통과 기준:
- `paraPr 중 hp:switch 미포함 = 0 / N`
- `plain <hh:intent> 발생 = 0`
- `<hc:intent>` 가 paraPr 갯수 × 2 (case + default 양쪽) 만큼 존재

### 5.3 GT 와 paraPr / tabPr 직접 비교

```python
import re, zipfile, io

def find_pr(xml, tag, id_):
    m = re.search(rf'<hh:{tag}\b[^>]*\bid="{id_}"[^>]*>.*?</hh:{tag}>', xml, re.DOTALL)
    return m.group(0) if m else None

gt = open('/tmp/gt/Contents/header.xml').read()
ours = open('/tmp/ours/Contents/header.xml').read()
# 동일 의미의 paraPr 끼리 비교 (id 는 다를 수 있음)
```

paraPr 의 `id` 는 templet 머지에 따라 다르지만, **참조하는 tabPr 의 stops + margin 의 case 값 + lineSpacing 값** 이 GT 와 일치해야 한다.

---

## 6. 향후 주의사항

### 6.1 rhwp upgrade 시

vendor/rhwp 를 upstream 새 버전으로 업데이트하면 우리 dual-unit 패치 (`write_tab_item_switch`, `write_para_pr` 의 switch 래핑, `write_margin_block`, `write_line_spacing`) 가 사라질 수 있다. 업데이트 후 검증 절차 (§5.2) 를 반드시 돌릴 것. 만약 upstream 이 이걸 자체 구현했다면 그쪽을 채택, 아니라면 우리 패치를 재적용한 fork 유지.

### 6.2 새 numeric 속성 추가 시

rhwp serializer 에 새로 numeric distance 속성을 emit 하려면 동일 패턴 적용:
1. IR 은 HwpUnit 2× 스케일 (parser 와 일치)
2. emit 시 `<hp:switch><hp:case>val/2</hp:case><hp:default>val</hp:default></hp:switch>`
3. case 분기에 `unit="HWPUNIT"` 명시

### 6.3 한 컴 buggy validator 우회 금지

이전에 `_wrap_paraPr_margin_in_switch` 코멘트에서 "case=2×default 등 다른 비율은 한 컴 12+ 가 거부한다" 가 확인됨. 정확히 `case = default / 2` 만 호환. 우회 패턴 (다른 비율, 단독 default 등) 시도 금지.

### 6.4 디버깅 우선 순위

`Q01.hwpx` 가 한 컴에서 안 열리거나 시각 이상이 발생하면 다음 순서로 점검:
1. version.xml / settings.xml — `_replace_meta_in_zip` 가 정상 동작했는지
2. style refs sanitize — `_sanitize_style_refs` 가 정상 동작했는지
3. **dual-unit switch** (이 문서의 핵심) — `<hp:switch>` 가 paraPr / tabPr 에 모두 emit 되었는지
4. paraPr 의 `tabPrIDRef` 가 존재하는 tabPr 를 가리키는지
5. cell paragraph 의 `<hp:tab width>` 자체 (Hanword 12+ 는 이걸 무시하고 tabPr stop 에 snap)

---

## 7. 관련 파일 / 코드 위치

| 파일 | 역할 |
|---|---|
| `vendor/rhwp/src/serializer/hwpx/header.rs` | dual-unit 패치 핵심. `write_tab_item_switch`, `write_para_pr`, `write_margin_block`, `write_line_spacing` |
| `vendor/rhwp/src/parser/hwpx/header.rs` | dual-unit 파싱 (round-trip 반대편). `parse_para_shape_switch`, `parse_tab_def` |
| `vendor/rhwp/src/parser/doc_info.rs` | HWP binary → IR (이미 2× 스케일이라 변환 없음) |
| `vendor/rhwp/src/model/style.rs` | IR 정의 (ParaShape, TabDef, TabItem) |
| `src/kdsnr_hwp_toolkit/compose/splitter.py` | 파이프라인 main. dual-unit 휴리스틱 제거 후 형태 |
| `kdsnr-hwp-toolkit/templet/original/*.hwp{,x}` | 입력 + GT (한컴이 .hwp 를 .hwpx 로 저장한 버전) |

---

## 8. 추적 후 발견된 별도 버그 — Inline control ordering (2026-05-11)

같은 추적 과정에서 발견한 두 번째 rhwp serializer 버그. **dual-unit 과 무관한 별개 root cause** 였지만 같은 추적 세션에서 해결.

### 8.1 증상

수식 (inline equation) 이 직전 텍스트보다 먼저 렌더링되어 한 자리씩 앞당겨짐.

예 (Q29 조건 paragraph):

| 위치 | GT (한 컴) | rhwp 출력 (수정 전) |
|---|---|---|
| 1 | "쌍곡선 " | `[C]` |
| 2 | `[C]` | "쌍곡선 " |
| 3 | "와 직선 " | `[y=2x-4]` |
| 4 | `[y=2x-4]` | "와 직선 " |
| 5 | "가 " | `[x]` |
| 6 | `[x]` | "가 좌표가..." |

수식 마다 자기 뒤 텍스트와 자리가 바뀜 — paragraph 첫 텍스트 chunk 가 사라진 듯한 효과.

### 8.2 Root Cause

`vendor/rhwp/src/serializer/hwpx/section.rs` 의 `render_run_content` / `render_runs_split_by_char_shapes` 두 함수에 있던 잘못된 가정:

```rust
// (수정 전) 인라인-렌더 가능한 컨트롤만 필터링
let slots: Vec<&Control> = para.controls.iter()
    .filter(|c| is_hwpx_inline_slot(c))
    .collect();

// PARA_TEXT 의 8-unit gap 마다 slots[i] 하나씩 emit
while char_pos >= expected_utf16_pos.saturating_add(8) {
    ...
    render_control_slot(slots[slot_idx]);
    slot_idx += 1;
    expected_utf16_pos += 8;
}
```

**문제**: `is_hwpx_inline_slot` 가 인라인이 아닌 컨트롤 (ColDef `0x02 'cold'`, SectionDef, Header, Footnote, AutoNum 등) 을 필터링해버려, **그 컨트롤이 PARA_TEXT 에서 차지하는 8-unit gap 만큼 인라인 슬롯이 한 칸씩 앞당겨짐**.

구체적 예 (테이블 셀 안 paragraph):
- PARA_TEXT 바이너리: `[cold (8u)] [쌍곡선  (4u)] [eq1 (8u)] [와 직선  (5u)] [eq2 (8u)] [가  (2u)] [eq3 (8u)] [좌표가… (16u)]`
- `controls[]`: `[cold, eq1, eq2, eq3]`
- `slots[]` (filtered): `[eq1, eq2, eq3]` ← cold 제거
- char_offsets[0] = 8 (첫 텍스트 '쌍' 의 위치)
- serializer 가 8-unit gap 보고 "1 slot emit" → `slots[0] = eq1` 출력. 사실 그 자리에는 cold 가 있어야 했고 eq1 은 그 다음 gap (position 12) 에 들어가야 함.

결과: eq1 이 잘못된 위치 (paragraph 시작, 텍스트 앞) 에 emit. 이후 eq2 / eq3 도 한 칸씩 앞당겨짐. 마지막 텍스트 chunk 가 trailing 으로 흡수되거나 사라지는 효과.

### 8.3 Fix

`render_run_content` / `render_runs_split_by_char_shapes` 모두 동일 패치 적용:

1. `slots` 필터 제거 — 대신 `all_controls = &para.controls` 전체 사용
2. drain 루프에서 `controls[ctrl_idx]` 가 `is_hwpx_inline_slot` 이면 emit, 아니면 **silent skip** (위치는 그대로 8-unit 진행)
3. trailing drain 도 같은 방식
4. `Field` 컨트롤은 BEGIN+END 두 슬롯 차지하지만 controls[] 에는 1개만 있어 `phantom_slots_to_consume` 메커니즘 유지

핵심 변경:

```rust
while char_pos >= expected_utf16_pos.saturating_add(8) {
    if phantom_slots_to_consume > 0 {
        phantom_slots_to_consume -= 1;
    } else if ctrl_idx < all_controls.len() {
        let ctrl = &all_controls[ctrl_idx];
        if is_hwpx_inline_slot(ctrl) {
            // emit inline control
            ...
        }
        // else: non-inline (cold/SectionDef/Header/Footnote/AutoNum/…) 은
        // 8-unit slot 을 소비만 하고 emit 안 함.
        ctrl_idx += 1;
    }
    expected_utf16_pos = expected_utf16_pos.saturating_add(8);
}
```

### 8.4 IR ↔ 바이너리 contract

```
para.controls[i] ←→ i-번째 extended control 이 PARA_TEXT 에서 등장한 순서
para.char_offsets[i] = i-번째 텍스트 char 의 PARA_TEXT 내 UTF-16 위치
```

parser (`vendor/rhwp/src/parser/body_text.rs`) 가 이 contract 를 유지. 수정 후 serializer 도 이 contract 를 정확히 활용 (필터링된 슬롯 인덱스가 아니라 원본 ctrl 인덱스로 진행).

### 8.5 영향 받는 컨트롤 종류

`is_hwpx_inline_slot` 가 false 반환하는 모든 extended control. 주요 케이스:
- `0x02 'cold'` (ColDef) — 셀 paragraph 의 첫 컨트롤로 자주 등장 (Q29_3 사례)
- `0x02 'secd'` (SectionDef) — section 첫 paragraph
- `0x0f 'hedr/footr'` (Header/Footer)
- `0x10 'fn/en'` (Footnote/Endnote)
- `0x11/0x12 'atnu/nwnu'` (AutoNum/NewNum)
- `0x15 'bokm'` (Bookmark)
- `0x16 'tdut/tcps'` (Dutbl/CharOverlap — overlap 은 inline 이지만 dutbl 는 별도)

### 8.6 검증

- Q29 조건 paragraph: `[C][쌍곡선][y=2x-4][와 직선][x][가 좌표가…]` (수정 전) → `[쌍곡선][C][와 직선][y=2x-4][가][x][좌표가…]` (수정 후, GT 와 정확히 일치).
- math_input_sample_2 전체 (53개 equation+text 혼합 paragraph) 에서 ordering 일치율 대폭 향상. 남은 차이의 대부분은 `<hp:t>` 분할 차이 (GT 는 tab 마다 별개 hp:t 로 분리, 우리는 통합 — 시각상 동일).

### 8.7 관련 파일

| 파일 | 변경 |
|---|---|
| `vendor/rhwp/src/serializer/hwpx/section.rs` | `render_run_content` + `render_runs_split_by_char_shapes` 의 drain 로직 |
| `vendor/rhwp/src/parser/body_text.rs` | 변경 없음 — parser 의 `para.controls` 순서가 이미 contract 와 일치 |

---

*최종 갱신: 2026-05-11. 두 가지 root cause 발견·해결: (1) dual-unit `<hp:switch>` 미구현, (2) inline-only 필터 기반 slot 인덱싱으로 인한 ordering 어긋남. 전 과목 (math/science/social/korean) 8개 입력, 106개 출력 unit 정상 재생성.*
