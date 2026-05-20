---
name: UI 작업 지침
description: 다빈치 프론트엔드 UI 작업 시 따라야 할 원칙과 피드백 누적
type: feedback
---

## 스타일 통일 원칙

극단적으로 크기를 오가지 말 것. 한 번에 적정선을 잡아야 함.
**Why:** 사용자가 "너무 작아" → 과하게 키움 → "너무 커" 반복을 싫어함.
**How to apply:** 변경 시 기존 대비 1~2단계(2~4px)만 조정.

## 색상 규칙

cream.600 (애매한 중간톤), cream.900 (과도한 진함) 사용 금지.
본문은 cream.700~800, 보조는 cream.400~500으로 통일.
흐릿한 글씨(cream.500 이하) 남발 금지 — 읽을 수 있어야 함.

## 레이아웃

- 2단 마스터-디테일 선호 (구성원, 조직, 도구 모두 이 패턴)
- 3열도 가능하나 비율 1:1:1 균등 분할 선호
- 카드 안에 카드 넣지 말 것 (설정 항목은 구분선 행으로)
- 조건부 열 표시/숨김 하지 말 것 — 항상 3열 유지

## 입력 컴포넌트

모든 Input/Select 스타일 통일: h=34px, px=12px, fontSize=14px, fontWeight=500, borderRadius=8px.
focus 시 daesung.400 테두리. transition 0.15s.

## 다이얼로그

DavinciDialog 공통 컴포넌트 사용. borderRadius=16px, boxShadow, X 닫기 버튼.
Chakra Dialog.Root 직접 사용 금지.

## 사이드바

ServiceSidebar 스타일 기준: 15px 메뉴, 17px 아이콘, 10px borderRadius, sidebar.active 배경.

## 이름 컬럼

이름 너비 120px 고정. flex로 늘리지 말 것.

## 새 채팅 버튼

cream.100 배경 + cream.200 테두리 + cream.700 텍스트. 뉴트럴 톤.
daesung pill 버튼은 촌스러움. 투명+텍스트만도 촌스러움.
