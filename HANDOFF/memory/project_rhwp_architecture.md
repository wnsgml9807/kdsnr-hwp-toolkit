---
name: rhwp 렌더 파이프라인 아키텍처 맵
description: 14k 줄 layout 코드의 단계별 구조 + ParaShape/탭/셀마진 흐름 + 알려진 inconsistency (잔여 증상 진단의 출발점)
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
# rhwp 렌더 파이프라인 (HWPX bytes → SVG → PDF)

작업 위치: `kdsnr-hwp-toolkit/vendor/rhwp/src/`

## 1. 7 단계 파이프라인

각 단계는 mutable 또는 read-only.

| # | 단계 | 진입점 | 입력 → 출력 |
|---|------|--------|-------------|
| 1 | Parse | `document_core/commands/document.rs:38` `DocumentCore::from_bytes` | bytes → `Document` IR |
| 2 | Validate + Reflow | `document.rs:48-65` `validate_linesegs` / `reflow_zero_height_paragraphs` / `normalize_hwpx_paragraphs` | Document → Document (mut) |
| 3 | Style Resolve | `renderer/style_resolver.rs:284` `resolve_styles(doc_info, dpi)` | DocInfo → `ResolvedStyleSet` |
| 4 | Compose | `renderer/composer.rs:104,113` `compose_section / compose_paragraph` | Paragraph + Styles → `ComposedParagraph` (lines, tab_extended) |
| 5 | Paginate | `document_core/queries/rendering.rs:775,877-908` `paginate` → `HeightMeasurer.measure_section` + `Paginator.paginate_with_measured` | Composed → `PaginationResult` + `MeasuredTable[]` |
| 6 | Build PageTree | `rendering.rs:1524,1624` `build_page_tree(page) → layout_engine.build_render_tree` | Composed + Styles + Measured → `PageRenderTree` |
| 7 | SVG Emit | `rendering.rs:57,67-75` `render_page_svg_native → SvgRenderer.render_tree` | Tree → SVG string |

PDF 는 7 출력 SVG 를 `renderer/pdf.rs:svg_to_pdf` 가 svg2pdf 로 변환. svg2pdf 는 `PageOptions { dpi: 96.0 }` 명시 필요 (디폴트 72 면 4/3 거대).

## 2. 핵심 IR (model/)

**Paragraph** (`model/paragraph.rs:7`)
- `text: String` — 본문 문자열 (UTF-16LE → Rust). `\t` 는 char 1 개로 보이지만 HWPX 내부에선 **8 UTF-16 코드유닛** 차지
- `char_offsets: Vec<u32>` — char index → UTF-16 절대 위치 매핑. gap (offsets[i] - offsets[i-1]) > prev_char_unit_size + 4 이면 그 사이에 컨트롤이 있음
- `char_shapes: Vec<CharShapeRef>` — `start_pos` (UTF-16) → `char_shape_id`. iterator `rev().find(|cs| cs.start_pos <= utf16_pos)` 로 char 별 cs 결정
- `controls: Vec<Control>` — Table/Picture/Shape/Equation 등. `treat_as_char=1` 이면 inline (텍스트 흐름에 박힘)
- `line_segs: Vec<LineSeg>` — 줄별 vpos/lh/sw/baseline. parser/composer/layout 모두 사용
- `tab_extended: Vec<[u16; 7]>` — `\t` 별 inline tab 정보 (width=ext[0], type=ext[2])
- `para_shape_id: u16` → `DocInfo.para_shapes[id]`

**ResolvedParaStyle** (`style_resolver.rs ~172`) — `Vec<f64>` 단위 (px 변환됨)
- `alignment`, `margin_left/right`, `indent` (= indent_first_line), `spacing_before/after`
- `line_spacing` / `line_spacing_type`
- `default_tab_width`, `tab_stops: Vec<TabStop>`

**Cell** (`model/table.rs:85`)
- `padding: Padding` — `apply_inner_margin=true` 일 때 cell-specific, false 면 table 의 default `padding`
- `width/height: HwpUnit`, `vertical_align`, `paragraphs: Vec<Paragraph>`

## 3. ParaShape 소비 — 경로별 drift

| 필드 | `layout_composed_paragraph` | `layout_inline_table_paragraph` |
|------|---------------------------|---------------------------------|
| alignment | ✓ start_x 계산 | ✓ start_x 계산 |
| margin_left/right | ✓ | ✓ |
| **indent (first-line)** | ✓ 876-879 | ✗ **누락** |
| spacing_before/after | ✓ | ✓ |
| line_spacing / type | ✓ corrected_line_height 거침 | △ raw line_segs 만 사용 |
| default_tab_width | ✓ composer 로 전달 | ✗ **누락** (composed.tab_extended 만 봄) |
| tab_stops | ✓ composer 로 전달 | ✗ **누락** |

**해석:** inline-table 경로는 page-level 경로와 별개로 진화. ParaShape 의 절반만 소비. **이게 본문 처음 들여쓰기 누락의 근본 원인**.

## 4. 셀 마진 흐름

`table_layout.rs:1177` `resolve_cell_padding(cell, table)` → pad_left/right/top/bottom (px).
`table_layout.rs:1190-1192`:
```
inner_x = cell_x + pad_left
inner_width = cell_w - pad_left - pad_right
inner_height = cell_h - pad_top - pad_bottom
```
`inner_area = LayoutRect { x: inner_x, y: text_y_start, width: inner_width, height: inner_height }` (1347).

inner_area 가 cell 내부 단락 layout 에 전달됨. **셀 padding 은 적용된다 (이론적으로).** 만약 시각 결과에서 오른쪽 여백이 무시되어 보인다면, 원인 후보:
- (a) `apply_inner_margin=false` 이고 table.padding 이 0 으로 잘못 파싱
- (b) cell.width 자체가 cell.padding 포함값이라 inner_width 가 더 줄어들어야
- (c) layout_inline_table_paragraph 가 `available_width = col_area.width - margin_left - margin_right` (228) 만 보고 right_margin 을 col_area 의 절대 우측 끝으로 잡아서 오버플로우

GT 와 차이를 진단하려면 cell.padding + table.padding 값을 probe 해봐야 한다.

## 5. 탭 type 이중 인코딩 (HWPX vs binary HWP)

**HWPX 파서** (`parser/hwpx/section.rs:293-306,543-555`): `<hp:tab type="X"/>` 의 attribute 값을 그대로 `ext[2] = X` 저장.

HWPX 코멘트 기준 매핑: `0=LEFT, 1=RIGHT, 2=CENTER, 3=DECIMAL`.

**Binary HWP 파서** (`parser/body_text.rs:273-287`): `ext[2]` 의 **고바이트** = type+1 (1=LEFT, 2=RIGHT, 3=CENTER, 4=DECIMAL). 저바이트 = fill_type.

**즉, 같은 `ext[2]` 필드가 포맷마다 다른 비트에 type 을 둔다.** Round-trip 시 위험.

**소비 측 inconsistency:**
- `text_measurement.rs:247` — `inline_type = ext[2]` (전체 u16). match: 1=RIGHT, 2=CENTER, _=LEFT
- `paragraph_layout.rs:70` — `inline_type = (ext[2] >> 8) & 0xFF` (고바이트). match: 0|1=LEFT skip, 2|3=RIGHT/CENTER continue

**HWPX 입력 (raw 저장) 에서 동작:**
- type="1" (HWPX RIGHT) → ext[2]=0x0001 → text_measurement 는 case 1 → RIGHT 동작 ✓
- type="2" (HWPX CENTER) → ext[2]=0x0002 → text_measurement 는 case 2 → CENTER 동작 ✓
- text_measurement 는 **"raw u16 가 우연히 binary 의 고바이트 enum 과 같은 의미를 갖는 코딩 사고"** 로 작동. 코멘트 240-243 가 "Task #296 범위 외" 라고 의도적으로 인정.
- paragraph_layout.rs 의 고바이트 추출 경로 (`resolve_last_tab_pending`) 는 HWPX 입력에서 항상 0 만 봐서 cross-run RIGHT/CENTER pending 을 전혀 못 잡음.

**HWPX 의 type 의미가 진짜 0/1/2/3 = LEFT/RIGHT/CENTER/DECIMAL 이라면**, Q20 의 `<hp:tab width="1200" type="2"/>` 는 **CENTER** tab. 분수 레이블 정렬용으로는 CENTER 가 자연스럽지 않다 (GT 처럼 `*`/`**`/`***`/`****` 가 right-edge 정렬되려면 RIGHT 가 맞음). 그래서 의심: **HWPX 의 type 값 매핑이 실제 한컴 스펙과 다를 수 있음** — 확인 필요.

## 6. composer (`composer.rs`, `composer/line_breaking.rs`)

- page-level 단락에서만 호출됨
- inline-table 단락은 raw para.text 와 para.line_segs 만 보고 layout_inline_table_paragraph 가 직접 처리 → composer 단계 건너뜀
- composer 가 indent_first_line, default_tab_width, tab_stops 를 ParaShape 에서 읽어 line_breaking 에 전달. 그래서 page-level 경로만 들여쓰기가 적용된다.

## 7. 정공법 단서 — 잔여 3 증상에 대한 가설

### 증상 A: `*` 들 정렬 안 됨
- HWPX type=2 가 **CENTER 인지 RIGHT 인지** 가 핵심. HWPX 1.0 스펙 / 한컴 샘플 비교 필요.
- text_measurement 의 case 1 가지가 RIGHT 동작 (seg_w_after 만큼 좌측이동). type=2 가 진짜 RIGHT 라면 ext[2] 저장 시 1 로 normalize 해야 (또는 raw 2 인 상태에서 CENTER 가 아닌 RIGHT 로 해석).
- 또는 한컴 자체가 inline tab type=2 를 RIGHT 로 처리하는데 우리 mapping 만 틀렸을 가능성.

### 증상 B: 오른쪽 셀 여백 무시
- cell.padding / table.padding 의 실제 hwpunit 값을 probe 해봐야 한다.
- inner_area 가 layout_inline_table_paragraph 에 잘 전달되는지 + available_width 가 cell 내부 폭을 정확히 반영하는지 확인.
- 후보: layout_inline_table_paragraph 가 `col_area.width - margin_left - margin_right` 만 보고 right_margin 을 정확히 계산하지 못하는 케이스.

### 증상 C: 본문 들여쓰기 누락
- 가장 확실. **layout_inline_table_paragraph 가 ParaShape.indent 를 완전히 무시한다** (4 절 표 참조).
- 박스 안 첫 단락 "표는 갑국의..." 는 inline-table 없음 → page-level 이 아닌 **cell-internal layout_composed_paragraph** 경로. 그 경로도 indent 를 적용하는지 재확인 필요.
- 또는 paragraph style 자체에 indent 가 안 설정돼 있을 수 있음 (probe 필요).

## 8. 진단 우선순위

1. **probe ParaShape**: Q20 의 각 cell 단락 (p[0..6]) 의 `para_shape_id` 와 그에 대응하는 `ParaShape.indent / margin_left / alignment` 실측값
2. **probe Cell padding**: 박스 외곽 표 + 데이터 grid 의 `cell.padding` + `table.padding` 실측값
3. **probe HWPX tab type semantics**: Q20 의 ext[2] 값들 + GT 한컴이 type=2 를 어떻게 그리는지 비교 (한컴 PDF text-extract 로 좌표 측정)

코드 수정은 이 3 probe 결과 본 후에 결정.
