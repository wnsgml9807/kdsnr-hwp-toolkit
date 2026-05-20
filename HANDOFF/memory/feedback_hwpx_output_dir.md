---
name: HWPX 파이프라인 출력 위치
description: HWPX 파서/SDK 결과물은 /tmp 가 아니라 flap-hwp-parser/work/ 안 하위 폴더에 저장
type: feedback
originSessionId: 10442048-766c-4f41-8be5-551e46033447
---
HWPX 파서/SDK (`parse_hwpx_set_to_each_question` 등) 의 출력(hwpx + png 미리보기) 은
**`flap-hwp-parser/work/<subfolder>/` 에 저장**한다. `/tmp/...` 사용 금지.

**Why:** 사용자가 macOS Finder 로 직접 결과물을 열어 확인하는 워크플로우. `/tmp` 는 사용자
시야 밖이라 검증을 못 함.

**How to apply:**
- 출력 디렉터리는 `flap-hwp-parser/work/sdk_<용도>` 형식 (예: `sdk_sci_bogi`,
  `sdk_mat_bogi`, `sdk_sci_v2`).
- 임시 중간 산물 (디버그용 dump 등) 도 가능하면 `work/` 하위에. 정말 일회성일 때만 `/tmp`.
- 사용자에게 결과 보여줄 때 절대경로 또는 `work/...` 상대경로 링크로 안내.
