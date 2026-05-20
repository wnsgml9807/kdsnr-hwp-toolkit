# Hybrid Layout Plan — rhwp 의 layout/render 를 한컴 1:1 포트로 대체

**최종 목표**: rhwp 의 출력이 macOS 한컴 PDF 와 **byte-equivalent**.

수단: `libHncDrawingEngine.dylib::Hnc::Shape::Text::*` (그리고 의존 lib) 의 layout 계열 함수를 Ghidra 로 decompile → 알고리즘·상수·분기 그대로 Rust 로 1:1 포팅 → `kdsnr-layout` 크레이트로 패키징 → rhwp 의 기존 layout/composer 휴리스틱을 점진적 폐기.

## 원칙

1. **알고리즘은 한컴에서 추출.** 우리 직관·교과서 알고리즘으로 짜지 않는다. 모든 식·상수·분기는 decompile 출처가 있어야 한다.
2. **포팅은 변수명·식 구조까지 1:1.** decompile 의 `iVar24`, `pcVar18`, `DAT_007415b0` 등을 그대로 변수명에 살리고, 함수 주소 (`FUN_0030590c`) 를 doc 주석에 남긴다.
3. **검증은 산출물 비교.** HWPX 를 입력으로 받아 우리 포트가 동일한 `<hp:linesegarray>` (vpos/lh/baseline/textwidth) 를 생성하는지 비교. 다르면 우리 포트가 틀린 것.
4. **HWPX `linesegarray` 신뢰 (replay) 는 쇼트컷.** 채택 안 함. HWP binary 입력에서는 그 값이 없고, byte-equivalent 가 목표이므로 우리 포트가 그 배열을 *재현* 해야 한다.

## 단계

### Phase A — RE coverage

A1. `Hnc::Shape::Text::*` 네임스페이스의 모든 함수 enumerate (Ghidra symbol tree).
A2. 현재 추출 완료된 decompile 과 대조 (`kdsnr-hwp-toolkit/work/hft_re/layout_re/decompiles/`, `/tmp/hsp_pdf_re/layout_sys/`).
A3. 누락 함수 추출:
   - `LayoutFactory::CreateHGlue`, `CreateHBox`, `CreateVBox`, `CreateHStrut`, `CreateCenter`
   - `Glyph` 파생 클래스의 `Compose` / `Decompose` / `GetWidth` / `GetHeight` 등 가상함수
   - `ColCompositor` 의 `ComposeNumbering` (이미 일부 있음) / `ComposeBullet` / 기타 helper
   - `Composition::DecideBreaks` / `Update` / 페이지 break 분기
   - `CharItemView` 의 layout-time 사용 메소드
   - `Break` 클래스 전체 (현재 일부)
A4. East-Asian penalty 상수 추출:
   - `DAT_007415b0`, `b8`, `c0`, `c8` 위치의 data segment 에서 실제 값 dump
   - 어떤 BreakClass 쌍에 어떤 penalty 가 매핑되는지 lookup table 의 row/col 의미 RE

### Phase B — 1:1 포팅

B1. `ColCompositor::ComposeBreak` 의 현재 `compose_break.rs` (교과서 Knuth-Plass) 를 **decompile 그대로** 다시 짠다. 한컴의 비용함수·badness·penalty 적용 위치를 정확히 살린다.
B2. `ColCompositor::ComposeLayout` 도 동일.
B3. `LayoutFactory` 의 모든 `Create*` 가상함수를 한컴 vtable 구조 그대로 포팅.
B4. `ParaProperty` 의 getter/setter 모두 포팅 (현재 일부).
B5. `Glyph` 클래스 hierarchy 와 가상함수 dispatch 를 enum + match 패턴으로 포팅 (Rust 식 가상함수 흉내).
B6. `Composition::ComposeGlyph` 및 helper.

### Phase C — 검증 harness

C1. HWPX 입력 → 파싱 (rhwp 의 parser 사용) → `kdsnr-layout` 으로 `linesegarray` 생성.
C2. 원본 HWPX 의 `linesegarray` 와 필드별 diff. 임계치 0 (정확 일치). 첫 diff 위치 + 차이값을 사람이 읽기 쉽게 출력.
C3. 입력 set: `kdsnr-hwp-toolkit/templet/original/` 의 korean/math/social/science 전 과목.

### Phase D — rhwp 통합

D1. rhwp 에 `kdsnr-layout` 의존 추가 (이미 Cargo.toml 에 path dep 등록 완료).
D2. `renderer/composer.rs` 의 `compose_lines` 가 stored linesegarray 를 슬라이스만 하던 코드를, **`kdsnr-layout` 으로 새로 생성한 linesegarray 를 사용** 하도록 변경. HWPX 의 stored 값은 보조 검증용으로만.
D3. `paragraph_layout.rs` 의 y-누적·tab/leader/numbering 로직을 `kdsnr-layout` 산출 (Glyph tree + 위치) 그대로 읽어 emit 으로 변환.
D4. `table_layout.rs` 의 cell 처리도 `kdsnr-layout` 에 cell 단위 호출 위임.

### Phase E — 페이지·페이지간 (out of layout scope)

E1. 다단·페이지 break·헤더/푸터·각주 area 배치는 rhwp 의 `pagination.rs` 가 계속 담당.
E2. `kdsnr-layout` 은 column 한 칸 (또는 cell 한 칸) 의 layout 만 책임진다.

### Phase F — cleanup

F1. rhwp 의 `composer.rs` / `paragraph_layout.rs` / `table_layout.rs` 의 기존 휴리스틱 경로 제거.
F2. height_measurer 도 `kdsnr-layout` 로 통합.

## 산출물 위치

- decompile 원본: `kdsnr-hwp-toolkit/work/hft_re/layout_re/decompiles/`
- 추가 추출은 같은 폴더에 저장. `Hnc_namespace_part_address.txt` 형식.
- Coverage audit 표: `kdsnr-hwp-toolkit/work/hft_re/layout_re/COVERAGE.md` (Phase A1 산출물)
- 알고리즘 노트: `kdsnr-hwp-toolkit/work/hft_re/layout_re/LAYOUT_RE.md` (계속 보강)
- 포팅 코드: `kdsnr-hwp-toolkit/layout-decoder/rust/src/*.rs`
- 검증 harness: `kdsnr-hwp-toolkit/layout-decoder/rust/tests/` + 별도 binary

## 현재 상태

| 항목                                  | 상태       |
|---------------------------------------|------------|
| layout_re/decompiles 일부 (~60)        | ✓ 추출됨    |
| LAYOUT_RE.md spec                      | ✓ 초안     |
| kdsnr-layout 스켈레톤                  | ✓ 컴파일됨 |
| compose_break.rs                       | △ 교과서 버전 (재포팅 필요) |
| compose_layout.rs                      | △ 추측 기반 (재포팅 필요) |
| LayoutFactory 전 메소드                | △ 일부만   |
| East-Asian penalty 상수                | ✗ 미추출   |
| 검증 harness                          | ✗ 미구축   |
| rhwp 통합                             | ✗ 미시작   |
