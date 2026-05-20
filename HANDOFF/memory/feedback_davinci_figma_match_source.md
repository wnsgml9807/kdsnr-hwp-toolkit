---
name: feedback-davinci-figma-match-source
description: 다빈치 Figma 라이브러리 컴포넌트는 시각적 추측 금지 — .tsx 코드 정독해서 픽셀 값 그대로 빼다박기
metadata: 
  node_type: memory
  type: feedback
  originSessionId: f2fed247-4476-42bd-8889-0933385a093f
---

다빈치 Figma 라이브러리에 컴포넌트 만들 때 "대충 비슷하게" 만들지 말고, 다빈치 프론트엔드의 해당 .tsx 코드를 먼저 정독해서 padding/radius/fontSize/weight/color 토큰을 그대로 빼다박는다. 사용자 표현으로 "조금씩 달라지는" 일이 반복되면 안 됨.

**Why**: 도구 카드·박스 등에서 시각적 추측으로 그렸더니 실제 다빈치와 어설프게 달라져서 사용자가 여러 번 지적함 — "어설프게 하지 말고", "왜 자꾸 이렇게 조금씩 달라질까?", "정확히 빼다박아 놔". 라이브러리 의미가 없어진다.

**How to apply**:
- 컴포넌트 만들기 전 해당 다빈치 소스 (예: `davinci/frontend/src/app/_components/InlineToolCard.tsx`, `ChatInput.tsx`, `ModelSelector.tsx`, `PlatformHeader.tsx`, `ServiceSidebar.tsx`, `ChatBody.tsx`, `DeskPanel.tsx`, `ChatConversationList.tsx`, `DocsDock.tsx`, `StepperView.tsx`, `settings/_components/SettingsUI.tsx`, `settings/_components/DavinciDialog.tsx`) 를 Read 로 펼쳐서 정확한 픽셀/토큰 값 확보.
- 사이즈·색상은 토큰 이름까지 일치시킨다 (예: `bg=daesung.500` → `paint("color/brand/default")`, `radius=10px` → `radius/md`).
- 다빈치에 없는 값(라이브러리 확장: 라디오 카드·슬라이더·무지개 색 등)은 별개. 다빈치 코드에 있는 컴포넌트는 그대로.
- 한 번에 끝낼 욕심 부리지 말고 컴포넌트 1~2개 단위로 짧게 빌드 → 스크린샷 확인 → 다음.
- 영역·전체 화면 페이지는 사용자가 따로 보존 지시한 영역이므로 건드리지 않음(2026-05-15 기준). 그 안의 부속 컴포넌트만 별도 페이지에 만들 때도 코드 그대로 옮긴다.
- 관련: [[project_davinci_figma_library]] · [[feedback_no_subjective_judgment]] · [[feedback_ui_guidelines]]
