---
name: Davinci inference parameters
description: Davinci 파이프라인 전 단계 추론 파라미터 확정 — 개요/재작성/지문 각각 구분
type: project
---

## Davinci 추론 파라미터 (rewrite 모드 기준)

### Step 1 — 개요 생성 (outline DPO)
- **temperature**: 1.0
- **top_p**: 0.95

### Step 2 — 사실관계 보정 (gemini-3.1-pro-preview)
- **temperature**: 0.3
- **top_p**: 0.9

### Step 3 — 개요 재작성 (outline DPO rewrite)
- **temperature**: 0.95
- **top_p**: 0.95

### Step 4 — 지문 생성 (passage DPO)
- **temperature**: 0.95
- **top_p**: 0.95

**Why:** OTP/법률행위/재무비율/바디우 등 다수 소재에서 반복 테스트하여 확정.
- 개요 생성은 1.0/0.95로 자유도를 높여 소재 다양성과 독해 포인트 깊이 확보.
- 보정은 0.3/0.9으로 사실 정확도 극대화.
- 재작성은 0.95/0.95로 AI 톤 오염 없이 DPO 학습 문체로 복원. (1.0에서는 메타적 서술 발생)
- 지문 생성은 0.95/0.95로 건조한 수능 톤 유지 + 개요 충실도 확보.

**How to apply:** `run_pipeline.py --mode rewrite` 기본 모드로 사용.
