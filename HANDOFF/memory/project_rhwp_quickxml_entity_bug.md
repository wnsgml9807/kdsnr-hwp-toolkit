---
name: rhwp HWPX 파서 quick-xml entity reference 누락 버그
description: quick-xml 0.39+ 가 &lt; &gt; &amp; 등을 Event::GeneralRef 로 분리하는데 rhwp 파서가 무시. 수정 위치/원리
type: project
originSessionId: 10442048-766c-4f41-8be5-551e46033447
---
quick-xml 0.39+ 부터 `&lt;` `&gt;` `&amp;` `&apos;` `&quot;` `&#NN;` 같은 character/general
entity reference 를 별도 `Event::GeneralRef` 이벤트로 emit. 이전 버전들은 `Event::Text`
안에 통합되어 있어 자동 처리됐다.

rhwp 의 hwpx 파서 (`read_text_content_with_tabs` in
`vendor/rhwp/src/parser/hwpx/section.rs`) 는 `Event::GeneralRef` 를 처리 안 하고
`_ => {}` 로 무시 → `<hp:t>&lt;</hp:t>` 같은 텍스트의 `<` 가 silently dropped.

영향: 보기 박스 라벨 `<보 기>`, 본문의 `<보기>` 등 문서 전반의 ASCII bracket 누락.

**Why:** quick-xml 의 메이저 버전 변경 시 silent breakage. 단위테스트로 직접 잡지
못했던 이유는 보통 한국어 본문에 `<` `>` 가 드물고, 우리는 보기 박스 빌드 시점에야
보였기 때문.

**How to apply:**
- `read_text_content_with_tabs` 의 match arm 에 `Event::GeneralRef(ref r)` 케이스 추가:
  raw entity name(`r.decode()`) 를 검사해 `lt`→`<`, `gt`→`>`, `amp`→`&`, `apos`→`'`,
  `quot`→`"`, `#NN`/`#xHH`→유니코드 codepoint 로 resolve 후 `text.push_str`.
- 다른 read_event 루프에서도 텍스트를 모으는 위치가 있다면 동일 패치 필요.
- quick-xml 업데이트 시 항상 entity 이벤트 처리 변경 여부 확인.

**디버그 단서**:
- `rhwp dump` 출력의 cell text 에 `<` `>` 누락 → 파서 단계 탈락 확정
- SVG 출력에 `&lt;`/`&gt;` 0건 (`grep -c '&lt' file.svg`) → render 가 아니라 파서 책임
