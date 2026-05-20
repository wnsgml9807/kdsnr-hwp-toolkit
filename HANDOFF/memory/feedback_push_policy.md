---
name: push 정책
description: 코드 변경 후 자동 push 금지. 매번 push 여부를 사용자에게 확인.
type: feedback
---

변경 때마다 자동으로 git push하지 않는다. commit 후 push 할지 말지 사용자에게 물어본다.

**Why:** 사용자가 변경 내용을 확인하고 싶어하고, 불필요한 배포를 방지하기 위해.

**How to apply:** git commit까지는 자유롭게 하되, push는 "push할까요?" 로 확인 후 진행.
