---
name: 자동 배포 파이프라인
description: frontend→Vercel, backend→Cloud Run 자동 배포. push만 하면 됨.
type: feedback
---

push하면 자동 배포된다:
- **frontend** (davinci/frontend) → GitHub push → Vercel 자동 배포
- **backend** (davinci/backend) → GitHub push → Google Cloud Run 자동 배포

**Why:** 별도 배포 명령 불필요. 커밋+푸시가 곧 배포.

**How to apply:** 작업 완료 후 push하면 끝. 수동 빌드/배포 안내하지 말 것.
