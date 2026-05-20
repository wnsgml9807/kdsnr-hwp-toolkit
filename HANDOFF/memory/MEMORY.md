# Project Memory

## ⭐ 작업 진행 상황

- **[TODO 모음](to-do.md)** — 다음 세션 + 보류된 이슈 + 운영 자동화 항목 통합

## Dataset Cleaning (edu-final-dataset.jsonl)

- Full guide: `data_extractor/CLEANING_GUIDE.md`
- Data file: `data_extractor/output/edu-final-dataset.jsonl`
- JSONL format, 0-indexed sets, each with passage/annotated_passage/questions[]

### Key Rules
- `passage`: markers only (no `<u>`, no `<mark-A>`)
- `annotated_passage`: markers + `<u>` underlines + `<mark-A>` tags
- `<box>`: first occurrence only in passage, both passage & annotated_passage
- `<보기>`: wrapper tag in material field
- SVG: inline in both passage & annotated_passage; use `<tspan>` not `<sub>` inside SVG
- [가]/[나] → [A]/[B] with `<mark-A>`/`<mark-B>` in annotated_passage only
- Underline range: sentence-level for 지시형, word-level for 어휘형; PDF is final authority
- Tilde: use fullwidth ～ (U+FF5E), not halfwidth ~ (U+007E)

## Design System

- [Design System & Dev Style](project_design_system.md) — UI 색상/타이포/컴포넌트 패턴, Pretendard+Nanum Myeongjo

## Kichul DB & Tool Architecture

- [Architecture & DB redesign](project_embedding_migration.md) — kichul_items 통합, 도구 4종, react-pdf 뷰어

## kdsnr-hwp-toolkit (현 작업)

- [재설계 핵심 계약](project_toolkit_redesign.md) — flap 레거시. atom→unified, 박스 내용물만 src 보존, 박스 shell 은 unified 에서 가져옴
- [rhwp 출력 보정 헬퍼 3종](project_toolkit_rhwp_quirks.md) — sanitize_style_refs / replace_meta_in_zip / picture+equation 보정 필수

## HWPX 문항 생성 파이프라인

- [HWPX Pipeline](project_hwpx_pipeline.md) — 전 과목 빌더 (수학/사회/국어)
- [Korean HWPX](project_korean_hwpx_redo.md) — 국어 빌더 완성, DB 등록 대기 (to-do.md)
- [HWPX 방법론](feedback_hwpx_methodology.md) — 원본 실측, header.xml 매핑, 시각 검증
- [HWPX 출력 위치](feedback_hwpx_output_dir.md) — /tmp 금지, flap-hwp-parser/work/sdk_* 에 저장
- [디버그 산출물 위치](feedback_debug_output_location.md) — kdsnr-hwp-toolkit/work/debug/ 에 저장
- [전 과목 입력 원본 위치](reference_toolkit_input_originals.md) — flap-hwp-parser/templet/original/ (korean/math/science/social hwpx+pdf)
- ⭐ [과목별 input 샘플 보존](project_preserved_subject_inputs.md) — `work/preserved_subject_inputs/`: math/science/social split 성공 + 각 3문항 PDF/PNG/SVG preview, korean unsupported 샘플/GT 보존
- ⭐ [hft-decoder 완성](reference_hft_decoder_complete.md) — kdsnr-hwp-toolkit/hft-decoder/ 6 family 모두 ✅ (cargo test 5/5). byte-eq port 진행률 산정에서 제외
- ⚠️ [rhwp SvgRenderer 참고용만](reference_rhwp_svg_renderer.md) — vendor/rhwp/src/renderer/svg.rs 2830줄은 rhwp 자체 백엔드일 뿐 한컴 1:1 아님 (사용자 실측). 우리 SVG export adapter 직접 작성 필요 (~500-2000 LOC), rhwp svg.rs 는 mechanical mapping 참고용
- [rhwp vertAlign quirk](project_rhwp_vertalign_quirk.md) — rhwp 가 vertAlign 엄격 적용. 한컴 spec TOP→rhwp 에선 CENTER 명시 필요
- [rhwp HWPX entity 누락 버그](project_rhwp_quickxml_entity_bug.md) — quick-xml 0.39+ Event::GeneralRef 처리 안 해서 &lt; &gt; 누락. section.rs 패치 위치
- [rhwp serializer dual root causes](project_rhwp_dual_root_causes.md) — (1) hp:switch dual-unit 미구현 → 한컴 12+ 2× 스케일 거대탭, (2) inline-only 필터로 control ordering 어긋남 → equation 위치 밀림. 패치 완료. 상세는 toolkit `docs/HWPX_DUAL_UNIT_SCHEMA.md`
- [rhwp placeholder lineseg 정공법](project_rhwp_placeholder_lineseg.md) — 한컴 placeholder horzsize 가 self-encoded cell_inner_width. reflow_zero_height_paragraphs 확장 + render-time reflow 우회 4 곳 제거 완료
- ⭐ [rhwp 본문 reflow_broadly 자동 적용](project_rhwp_reflow_broadly_body.md) — 한컴이 long-text paragraph 의 lineseg 를 1 개만 cache. 본문 path 에 needs_reflow_broadly OR 추가 (1 줄) → Q18 발문 1 줄 cram → 3 줄 정상 (2026-05-18). ★ 안전망이며 진짜 root cause 는 [[project-splitter-enrich-linesegs]]
- ⭐⭐ [splitter pipeline 끝에 enrich_linesegs 호출 (2026-05-18)](project_splitter_enrich_linesegs.md) — splitter 가 발문/본문 paragraph linesegs_xml="" 비움 → rhwp self-accumulate y 와 다음 paragraph absolute vpos 불일치 → Q28 수식 위 +109 px 공백, Q18 발문 1줄 cram + 표 셀 가/나/다 한줄 압축. enrich_linesegs(out_doc) 1 줄 추가로 전 12 페어 96.60→97.27%, Q28_2 99.19%
- ⭐ [cell_height.resolve 회귀 제거 (2026-05-19)](project_cell_height_resolve_regression.md) — enrich_doc 가 cellSz 282→1586 부풀려서 사이언스 Q14 (가)(나)(다) 마지막 행 잘림. enrich_doc 에서 cell_height.resolve 호출 제거. linesegs.fill_missing 만 유지 (Q28 효과 그대로)
- ⭐ [rhwp inline rect 이중 emit fix (2026-05-19)](project_rhwp_inline_rect_double_emit.md) — table_layout.rs:1677-1724 가 paragraph_layout 직후 cell paragraph 안 inline Shape 주변 text 를 다시 emit. social Q03 발문 박스 마지막 줄 텍스트 이중 렌더. text_before TextRunNode push 제거 (inline_x 누적만 유지)
- ⚠️ [splitter wrapper vp fix 철회 (2026-05-19)](project_splitter_wrapper_vp_fix.md) — enrich_linesegs wrapper-vp 덮어쓰기 fix 시도했으나 **rhwp 가 hwpx lineseg vp 를 무시**해서 PNG 0 영향 (md5 동일). 코드 롤백. 진단은 lineseg vp 가 아닌 render tree 비교가 정답. 3 신설 도구: `work/tool_hwpx_xml_diff.py`, `work/tool_pixel_diff_strong.py`, `work/tool_render_tree_diff.py` + rhwp `dump-render-tree` 서브커맨드 (vendor/rhwp/src/main.rs + wasm_api.rs `get_page_render_tree_native`). Q04/Q05 render tree 동일 (회귀 없음), Q11/Q17 dy=24.3px 진짜 차이
- ⭐ [rhwp paragraph_layout stored-vp 우선 패치 (2026-05-19)](project_rhwp_stored_vp_patch.md) — 모든 paragraph 가 stored lineseg.vp 기반 line 위치 (use_stored_vp). 단 stored_h==sum (자기일치) 이라 paragraph 간 누적은 변화 없음. +128.6 GT diff 진짜 root cause = pagination engine 의 wrap=TopAndBottom 표 누적 (별도)
- [rhwp 렌더 파이프라인 아키텍처](project_rhwp_architecture.md) — 7 단계 흐름, IR/ParaShape/탭 type/셀마진 흐름 + 알려진 dual-encoding inconsistency. 잔여 증상 진단 출발점
- ⭐ [rhwp visible 결손 3-fix (2026-05-18)](project_rhwp_masterpage_added.md) — (A) masterpage 파싱 (B) Picture mime byte detect (C) LineShape startPt/endPt 파싱. IoU dilate-2px **16.68 → 24.73% (+8%p)**, strict ink **1.16 → 5.45% (4.7×)**. visible: 헤더/페이지번호/세로 divider/도형 모두 복구
- ⭐⭐ [G phase plan + G-W-1 wire entry](project_g_phase_d_plan.md) — port 는 이미 작성됨 (`ppt_compose_break.rs` 1058줄, `ppt_compose_layout.rs` 3726줄). `kdsnr_bridge.rs` 어댑터도 `compose_line` wrapper 가짐. 그러나 **호출처 0개 = dead code**. 다음 step = `layout_composed_paragraph`(3209줄) line-loop 에 `compose_line` wire 작업 (RHWP_USE_KDSNR_LAYOUT env gate)
- [HFT alias 확장 (2026-05-18)](project_hft_alias_extension.md) — HftCache::add_alias API + HancomFont.zip 명시 load (HCRBatang/HCRDotum). 함초롬바탕 face hit 0/14 → 14/14
- ⭐ [HFT cache 로드 = 글씨체 GT 일치 (2026-05-18)](project_render_with_hft.md) — raster_pages.rs 에 set_global_hft_cache + doc.set_hft_cache 호출. try_emit_hft_paths 가 신명 series → 한컴 vector path 직접 emit. 셀 안 글씨 굵음→얇은 명조. 단 advance 는 substitute TTF 기준 → 줄바꿈 위치 잔존
- [layout RE 상태 (16-μ 까지)](project_layout_re_state.md) — **543 layout + 837 render-engine = 1380 tests pass + 1 ignored** (2026-05-17). L-5a/b + L-5c-0/1/2 (Degree/Matrix3/Transform2D from libHncFoundation) + L-5c-3a/b/c/d (RenderUtil/ShapeEngine/BodyProperty 27 getter + Margin/FlatTextPair) 완료.
- ⭐ [Glyph Draw vfunc 5-8 port 상태 (L-5)](project_glyph_draw_state.md) — L-5a/b/c-1/2/3 + L-5c-RE-1/2/3/4/5a/5b/5b2/5b2-ttf/5b4/5b3a/5b3b/5e/6a/b/c + **Stage 4 (SVG adapter brush/pen/equation) + Stage 5 (pixel-diff harness skeleton)** 완료 (2026-05-18). P0 chain ≈ **94%**, 종합 ≈ **96%**. 1332 tests pass. 수식 byte-eq port (HncEqEdit dylib) 산정 = **28~32 세션** (별도 작업, harness 측정 후 결정). 다음 = e2e wire + pixel-diff 측정.
- [rendering phase grand plan (R-2~R-4/R-6 SKIP)](project_rendering_phase_plan.md) — 2026-05-17 Option B 선회 반영. byte-eq logic 까지만, 출력 backend 는 SvgSurface 어댑터로 대체.
- ⭐ **[full byte-eq pipeline plan (Option B)](project_full_byteeq_plan.md)** — pixel-eq 목표, parser/layout/render/paginator 모두 byte-eq port, SvgSurface adapter (200-400줄) 만 custom. 28-44 세션 추정.
- ⭐ **[e2e bench 첫 실측 + heatmap 분석 (2026-05-18)](project_e2e_first_measurement.md)** — rhwp baseline **93.99%** (106 페어). heatmap: mismatch 91% 가 글리프 영역 밖 + 57% 가 long run (≥5px). single biggest ROI = 이미지/도표 위치 (social Q08 단일 cluster 64K px = page mismatch 27.9%). **수식 byte-eq port ROI ≪ 추정치** (math glyph_in 4.8%, max cluster 399px). 다음 byte-eq port 후보 = PictureGlyph / TableGlyph / paginator vertical position.
- ⭐ **[wire 시도 + 진짜 한컴 GT + honest metric (2026-05-18)](project_wire_probe_state.md)** — rhwp PageRenderTree → 우리 SvgSurface adapter. 12 페어, **3 metric**: full=96.60% (inflated, 흰배경 매치) / strict ink-only=1.16% (1px 어긋남도 mismatch) / **IoU dilate-2px=16.75%** (시각 perception). 한컴 HFT 380+ binary embed (180MB). Latin TTF fallback (visible 도약). 진짜 bottleneck = 글자 위치/advance/모양 정확도 (GT/ours ink 양 비슷, 위치 다름). 다음 = Latin HFT 사용 / EquationNode 직접 파싱 / Header-Divider hwpx XML 파싱.
- ⭐⭐ [KDSNR_BREAK apply gate (2026-05-19)](project_kdsnr_break_apply.md) — `compute_kdsnr_breaks` 헬퍼 + `RHWP_USE_KDSNR_BREAK_APPLY` env gate. Hancom-saved RT 대비 **Q11 matched 71→140**, dy 24.3→12.3px ⭐. 그러나 Q05 (stored line_segs 이미 완벽 매치) 는 reflow 자체로 회귀 (204→114). per-char tokenization + char class penalty 가 진짜 byte-eq 필요
- ⭐⭐ [Component EQ_SCORE goal metric (2026-05-19)](project_component_eq_metric.md) — `work/tool_component_eq.py` + render tree JSON 스타일 속성 확장. pixel-eq goal 의 공식 metric. **Q05 baseline 95.43%**, Q11/Q17 baseline ~25%. kdsnr+brkapply 는 matched count 늘리나 full_eq 약간 낮음. 매 byte-eq port 후 측정
- ⭐ [Q11 dy=+2.8 systematic offset root cause (2026-05-19)](project_q11_dy_offset_root_cause.md) — splitter hwpx 의 stored lineseg.lh 가 한컴 saved 와 다름 (ls[1] lh=1300 vs 1150, tag bit 20 다름). rhwp 가 stored lh 신뢰해서 누적 +2.8 px. 해결: rhwp 가 stored lh 무시하고 한컴식 line metric 자체 계산 (compute_line_height byte-eq port) — paragraph_layout 정공법
- ⭐⭐ [lineseg_gen line_text_h byte-eq port (2026-05-19)](project_lineseg_line_max_height.md) — `lineseg_gen.py:generate_linesegs` line_text_h 결정을 paragraph 단일 font_size → **line 안 chars 의 max char_shape.height** 로 변경. baseline/spacing 도 line_text_h 기반 재계산. Q11 dy=+2.80 → 0 완전 제거. Q11 79.82→85.32%, Q17 65.79→70.18%
- ⭐⭐⭐ [lineseg inject 검증 + byte-diff spec (2026-05-20)](project_lineseg_inject_validation.md) — `tool_inject_hwpsaved_lineseg.py` 로 ours 의 linesegarray 만 hwpsaved 로 swap → **Q11 100% EQ_SCORE 달성**. 비-TextRun 컴포넌트 모두 100% 매치. 잔여 = TextRun (char advance). Q11 38 lineseg attribute byte-diff: horzsize 25 diff (빈 para=0 + eff_w -1/-2), textpos 6, flags swap 3, line metric 5. lineseg_gen.py byte-eq port 의 spec 확정
- [Glyph vfunc index audit](project_glyph_vfunc_audit.md) — raw vtable dump 검증: vfunc[3]=Request(Requisition&), vfunc[4]=Allocate(Allocation,Extension). Ghidra "Glue::Request" mis-name 정정
- [Composition port 상세 상태](project_composition_port_state.md) — 함수별 port 진행도, raw asm 위치, 다음 진입점, 정공법 정책 강조
- [e2e 검증 세트 (.hwpx + 한컴 .hwp 12쌍)](project_e2e_validation_set.md) — Phase C byte-equivalent 검증의 input + reference. work/e2e/ 하 12 항목
- [분할 파이프라인 손상](project_split_pipeline_broken.md) — work/e2e 이후 시점부터 분할 hwpx 가 한컴 호환 깨짐. 보수 필요

## HWP COM 편집 API

- [엔진 현재 상태](project_hwp_engine_status.md) — 도구 목록, 성숙도, 제한사항
- [스타일 복사 아키텍처](project_hwp_style_architecture.md) — Display/Apply 분리, XML 직접 복사, 프리셋 도구 분리
- [COM 검증 v2](project_hwp_com_verified_v2.md) — 연결 전략, SetTextFile, .tmp 방어, find_table 카운팅
- [Staleness + 네이티브 Undo 탐색](project_hwp_staleness_native_undo.md) — Modified 플래그 확정, Ctrl+Z 1회 포기. 구현은 to-do.md

## Davinci 플랫폼

- [Dock 리팩토링](project_dock_refactor.md) — Source Controller 패턴 (Phase 1 완료, Phase 2 in to-do)
- [Inference Parameters](project_davinci_inference.md) — Gemini fine-tuned 추론 파라미터
- [Refactoring Plan](project_refactoring_plan.md) — chat.py 분해, 도구 통합, Auth 정리
- [Settings Page](project_settings_page.md) — /settings 구조, 권한, 도구, 서비스 멤버 관리
- [WorkOS Pipes 거부 플로우 제약](project_workos_pipes_error_flow.md) — authorize 파라미터 3개뿐, 거부 시 정적 HTML 에러만 (재조사 금지)
- [Davinci UI Figma 라이브러리](project_davinci_figma_library.md) — 기획자용 컴포넌트 라이브러리 구축 완료, fileKey BSExKS0NW4jLTUggfmG7Iu. Pretendard→Gothic A1 제약. 남은 건 사용자 publish

## Feedback

- [자동 배포](feedback_deploy.md) — Vercel + Cloud Run, push만 하면 됨
- [작업 스타일](feedback_work_style.md) — 이중 머신, 커맨드, 코드 스타일, 테스트 워크플로우
- [리포트 스타일](feedback_report_style.md) — 임의 추상화 금지, 지문 전문 인용
- [주관적 판단 금지](feedback_no_subjective_judgment.md) — 증상/원인만 객관적 기술
- [Windows 동기화](feedback_windows_sync.md) — robocopy, 공유 폴더 직접 실행 금지
- [Windows 빌드 인프라](feedback_windows_build.md) — SSH 22, GH_TOKEN, Draft→publish 전환, 깨진 릴리즈 회수
- [Electron 빌드 절차](feedback_windows_build_dual.md) — x64 전용, bat CP949+CRLF, /XF로 실행 중 파일 제외, 릴리즈 publish
- [UI 지침](feedback_ui_guidelines.md) — 크기 극단 금지, DavinciDialog, 색상 규칙
- [Push 정책](feedback_push_policy.md) — 자동 push 금지, 매번 확인 후 진행
- [정공법·완벽 구현 (타협 금지)](feedback_no_time_optimization.md) — MVP/우회 + "이정도면 충분" 타협 둘 다 금지. 깊은 모듈 회피 금지, 정공법으로 완성
- ⭐ [깊게 전수로 읽고 작업 (2026-05-20)](feedback_deep_read_before_patch.md) — patch 전 해당 모듈 + 관련 파일 전수 Read. 찔끔 보고 찔끔 patch → 회귀/재패치 반복 금지
- [rhwp 정면돌파 정책](feedback_rhwp_frontal_assault.md) — rhwp 책임 입증되면 우회 말고 라이브러리 직접 패치
- ⭐ [pixel-eq + maximal byte-eq logic](feedback_rhwp_byte_equivalent_goal.md) — **2026-05-17 갱신: byte-eq → pixel-eq 선회**. 좌표/색/모양 결정 logic 은 byte-eq, 출력 backend (Surface/PDF writer) 는 SvgSurface 어댑터로 대체
- ⭐ [eval harness 는 맨 마지막](feedback_eval_harness_last.md) — 모든 byte-eq port 끝난 뒤 Stage 4 에 pixel-diff harness 구축. 그 전엔 점수 보면서 작업 금지
- [HFT 디코더는 toolkit 모듈](feedback_hft_toolkit_isolation.md) — HFT RE 결과물은 rhwp 에 편입 금지. toolkit 내부 독립 sub-module 로
- [probe 기반 사실 확인 후 구현](feedback_probe_driven.md) — 추측으로 코드 작성 금지. 픽셀 측정·dump 로 검증된 사실에만 기반
- [릴리즈 publish 정책](feedback_release_publish_policy.md) — Draft→public 전환 자동 금지, 사용자 테스트 후 명시적 명령 필요
- [COM 테스트](feedback_test_output.md) — SSH 가능, sys.path 주의, __pycache__ 캐시 문제
- [HWP 테스트 실행](feedback_hwp_test_execution.md) — HWP/한컴 GUI 기반 → 사용자 실행, 어시스턴트는 커맨드만
- [부분 조사·임의 수정 금지](feedback_partial_view_and_unauthorized_edit.md) — DB 스냅만 보고 "실패" 단정 금지, end-to-end 확인 후 발언. 처방(수정안) 은 요청받을 때만
- [병렬 세션](feedback_parallel_sessions.md) — 같은 레포 여러 claude 세션 동시 실행, 파일 충돌 감지·자기 PID 식별·정리 절차
- [cargo 무한 로딩 해소](feedback_cargo_process_lock.md) — 백그라운드 죽은 cargo 가 target lock 점유 → pkill -9 후 재시도. 한 turn 에 cargo 다중 동시 실행 금지
- [다빈치 Figma 그대로 빼다박기](feedback_davinci_figma_match_source.md) — 시각적 추측 금지, .tsx 정독 후 픽셀 값·토큰 그대로
