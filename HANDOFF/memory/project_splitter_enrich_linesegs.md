---
name: project-splitter-enrich-linesegs
description: "splitter pipeline 끝에 enrich_linesegs(out_doc) 호출 추가. 발문/본문 paragraph linesegs_xml 빔 → rhwp 가 self-accumulate y 와 다음 paragraph absolute vpos 사이 불일치로 거대 공백·발문 1줄 cram·표 셀 압축. 전 12 페어 평균 96.60→97.27%, Q28_2 99.19% best. Q18 발문/표 정상화."
metadata: 
  node_type: memory
  type: project
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

**적용 위치**: `src/kdsnr_hwp_toolkit/compose/splitter.py:split_paper_to_hwpx_units` 의 `_strip_templet_tabstopval(out_doc)` 다음, `write(out_doc)` 직전.

```python
out_doc = enrich_linesegs(out_doc)
out_bytes = write(out_doc)
```

**Why**: splitter 가 3 군데에서 `linesegs_xml=""` 비움 — src 의 lineseg vpos cache 가 templet slot 의 column 폭과 안 맞기 때문 (의도된 동작).

1. `extract/boundary.py:119, 132` `split_fused_paragraph`
2. `transform/applier.py:133` `_apply_role_style` text-only
3. `transform/applier.py:230` `_apply_balmun_style` (발문)

rhwp 는 segs=0 paragraph 의 height 를 self-accumulate 하는데, 그 paragraph 다음 paragraph 가 절대 vpos 를 보존한 경우 두 값 사이 불일치 → 거대 공백 발생 (Q28 수식 위 +109 px = 약 2616 HU).

`enrich_linesegs(doc)` 는 [lineseg_gen.py:942](src/kdsnr_hwp_toolkit/lineseg_gen.py#L942) 가 metric/calib 기반으로 모든 paragraph 의 linesegs 를 재생성. inline equation/picture 컨트롤도 `extract_text_and_inlines` 로 처리.

**How to apply**:

- splitter 출력 직전에 한 번 호출 (per-unit). 작은 calib JSON 매번 로드는 무시 가능.
- pure Python 변경이라 rust 빌드 불필요.
- 이전 [project_rhwp_reflow_broadly_body.md] 의 reflow_broadly 보조망은 유지하되, 이 fix 가 진짜 root cause 해결.

**시각 비교 (이전 → fix 후)**:

- Q28_2 (수학): 수식 위 +109 px 공백 → 0 px. score 96.60→99.19%
- Q18 (과학): 발문 1줄 cram + 표 셀 가/나/다 한줄 압축 → 정상 줄바꿈
- 전체 12 페어 평균: 96.60 → 97.27% (+0.67%p, 단 시각 perception 으로는 훨씬 큰 도약)

**연계 메모리**:
- [[project-rhwp-reflow-broadly-body]] — rhwp 측 안전망
- [[project-toolkit-redesign]] — splitter pipeline 전체
- [[feedback-rhwp-frontal-assault]] — 정공법 정책
