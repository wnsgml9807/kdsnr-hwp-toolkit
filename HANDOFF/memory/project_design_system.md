---
name: Davinci Design System & Development Style
description: UI/UX 디자인 시스템 — 색상(cream/daesung), 타이포(Pretendard/Nanum Myeongjo), 컴포넌트 패턴, PDF 뷰어, 마크다운 렌더링, 개발 컨벤션
type: project
---

# Davinci Design System & Development Style

## 색상 체계 (Color Tokens)

### Primary — daesung (강남대성 브랜드 teal)
- `daesung.500` (#2BACA2): 메인 액센트, 활성 상태, CTA 버튼
- `daesung.50`: 호버 배경, 뱃지 배경
- `daesung.600`: 강조 텍스트, 활성 탭 텍스트

### Neutral — cream (웜 그레이 계열)
- `cream.50` ~ `cream.100`: 미세 배경, 호버
- `cream.200`: 구분선 (borderColor), 비활성 배경
- `cream.300`: 테두리 (border), 카드 경계
- `cream.400`: 비활성 아이콘, placeholder 텍스트
- `cream.500`: 보조 텍스트, 비활성 라벨
- `cream.600`: 부제목, 아이콘 기본색
- `cream.700`: 중요 보조 텍스트
- `cream.800`: 본문 텍스트
- `cream.900`: 헤딩, 강조 텍스트, 로그인 버튼 배경

### 상태 색상
- 성공: `green.50` / `green.500` / `green.600`
- 경고/유사도: `red.50` / `red.600`, `orange.50` / `orange.600`
- 관리자: `blue.50` / `blue.600` (Admin), `purple.50` / `purple.600` (Master)

## 타이포그래피

### 폰트 패밀리
- **UI 텍스트**: `'Pretendard', sans-serif` — 버튼, 라벨, 네비게이션, 뱃지, 메타 정보
- **콘텐츠 본문**: `'Nanum Myeongjo', serif` — 지문, 발문, 선지 등 시험지 콘텐츠
- **body 기본값**: `'Noto Serif KR', serif` (Chakra theme)

### 폰트 규칙
- 버튼: `fontSize="15px"`, `fontWeight="700"`
- 뱃지/라벨: `fontSize="12~13px"`, `fontWeight="600~700"`
- 본문 텍스트: `fontSize="14~15px"`, `fontWeight="500"`
- 콘텐츠 (시험지): `fontSize="18px"`, `fontWeight={500~800}`, `lineHeight="1.8"`

## 컴포넌트 패턴

### 버튼
- **CTA**: `borderRadius="12px"`, `py="14px"`, `bg="daesung.500"`, `color="white"`, `fontWeight="700"`
- **보조 버튼 그룹**: 흰색 배경, `border="1px solid" borderColor="cream.300"`, 중앙 구분선
- **로그인**: `bg="cream.900"`, `color="white"`, `borderRadius="12px"`, `h="52px"`
- **Ghost 아이콘**: `variant="ghost"`, `borderRadius="10px"`, `color="cream.500"`

### 카드
- `borderRadius="14px"`, `border="1px solid" borderColor="cream.200"`, `bg="white"`

### 다이얼로그 (모달)
- 오버레이: `position="fixed"`, `bg="blackAlpha.400"`
- 컨텐츠: `borderRadius="16px"`, `boxShadow="0 20px 60px rgba(0,0,0,.15)"`

### PDF 뷰어 (ContentView.tsx — PdfViewer)
- react-pdf (wojtekmaj v10.4.1): `<Document>` + `<Page>` + TextLayer
- 플로팅 줌 리모콘: `position="sticky"`, 좌측 상단 고정 (top/ml 12px)
- 줌 스텝: 50% ~ 200%, 기본 100% (= 컨테이너 폭의 80%)
- 텍스트 선택 색상: `rgba(43, 172, 162, 0.35)` (daesung 계열)
- 텍스트 드래그 → DeskSelection → "질문하기" 플로팅 버튼 → 채팅 인용

### 마크다운 렌더링 (markdown.ts)
- `mdToHtml`: LaTeX를 먼저 추출(플레이스홀더) → 마크다운 처리 → KaTeX 복원
- LaTeX 지원: `$$...$$`, `\[...\]`, `\(...\)`, `$...$`
- `inlineMd`: 인라인 마크다운 (bold, italic, code, link) + LaTeX
- `renderAnnotated`: 시험지 마크업 (mark-A/B/가, box, u 태그)

## 개발 컨벤션

### 상태 관리
- **Zustand**: 서비스별 store (similarityStore, chatStore, deskStore)
- **localStorage**: job ID 복구용 persist

### API 패턴
- `apiFetch<T>()`: 제네릭 JSON fetch wrapper, 401 시 session-expired 이벤트
- SSE: artifact/tool_card/meta 등 이벤트 → sseHandlers.ts에서 라우팅
- Supabase Realtime: postgres_changes 구독

### 인증
- Supabase Auth + `onAuthStateChange` 단일 리스너
- `AuthContext`: session, user, role, subject, signOut

### 파일 구조
- `app/_components/`: 페이지 전용 (PlatformHeader, Sidebar, ShellLayout, DeskPanel, DeskSelection)
- `app/_components/artifacts/`: ContentView (시험지 렌더러 + PDF 뷰어)
- `stores/`: Zustand store + types.ts (DeskTab 등)
- `lib/`: 유틸리티 (api.ts, supabase.ts, markdown.ts)

### 코드 스타일
- 한국어 주석, 섹션 구분 주석 (`// ====...====`)
- 인라인 스타일 props (Chakra UI), CSS 파일은 전역만
- `fontFamily` 명시적 지정 (theme 기본값이 Noto Serif이므로)
