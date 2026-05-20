# HANDOFF — 새 AI 진입 가이드

작업 인계용. 다음 순서로 읽음:

1. **`context.md`** — 현재 작업 상황 (goal, 진척, blocker, path 후보 비교, 측정 결과)
2. **`harness.md`** — 측정 도구 + 검증 절차 (inject script, dump-render-tree, tool_component_eq.py 등)
3. **`memory/MEMORY.md`** — 사용자 자동 memory 인덱스. 작업 history + 정책 + 도구 + 진행 상황 78 개 메모리
4. **`memory/feedback_deep_read_before_patch.md`** — 가장 중요한 작업 정책. **patch 전 전수 read**, 찔끔 patch 금지
5. **`memory/feedback_no_time_optimization.md`** — 정공법 정책. MVP/우회/타협 금지
6. **`memory/project_lineseg_inject_validation.md`** — 가장 최근 (2026-05-20) 핵심 발견. lineseg inject 로 Q11 100% 달성 입증 + byte-diff spec

## Goal (Stop hook 강제)

```
한컴 export(GT) 와 rhwp 렌더링이 pixel eq
EQ_SCORE ≥ 99% across all test cases (project_component_eq_metric.md)
```

현재 4 케이스 평균 94.45% (Q11 100% inject mode only, Q17 98.25%, Q05 95.03%, Q04 84.51%). **미달성 상태**.

## 핵심 환경

- 작업 디렉토리: `/Users/wnsgml/Desktop/새 폴더/Project/KSAT Agent/Agent_Streamlit/kdsnr-hwp-toolkit/`
- 사용자 OS: macOS 24.6.0
- macOS 한컴 HWP 12.30.0 설치됨 (`/Applications/Hancom Office HWP.app/`)
- HFT 폰트 cache: `/Applications/Hancom Office HWP.app/Contents/Resources/Hnc/Shared/Fonts`

## 절대 금지

- COM API (Windows) — 사용자 명시적 금지
- 도구 만지작 (fuzzy=full_ok cheat 등) — 사용자 명시적 금지
- 찔끔 patch — 사용자 명시적 금지 (`feedback_deep_read_before_patch.md`)
- 한컴 GUI 없는 환경 가정 — macOS 한컴 응답 확인됨

## 사용자 작업 스타일

- 정공법 우선. 타협 금지.
- 깊게 전수 read 후 단일 정확 patch
- 측정 도구로 사전 검증 + 사후 측정
- 메모리에 진행 상황 박제 (project_*) + 정책 박제 (feedback_*)
