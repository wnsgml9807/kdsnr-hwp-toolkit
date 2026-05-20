---
name: rhwp 출력 보정 헬퍼 3종
description: kdsnr-hwp-toolkit 가 rhwp HWP→HWPX 출력을 한글에서 열고 정상 렌더하기 위해 필수로 적용해야 하는 보정들
type: project
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
rhwp 의 HWP→HWPX 직접 출력은 한글에서 열리지 않거나 깨져 렌더된다. 다음 3가지 보정을 항상 거쳐야 정상 동작:

1. **`_sanitize_style_refs`** (compose/splitter.py)
   - rhwp 가 `<hh:style>` 의 `paraPrIDRef` 슬롯에 langID (1042 = 한국어) 를 넣어 출력. 한글은 이 dangling ref 를 그대로 적용해 paragraph 의 실제 paraPr (intent/align/들여쓰기) 를 무시한다.
   - 유효 범위 밖 ref 를 `4294967295` (HWPX uint sentinel = "no override") 로 교체
   - merge_styles 직후, write 직전에 호출

2. **`_replace_meta_in_zip`** (compose/splitter.py)
   - rhwp 출력 version.xml 은 구버전 xmlVersion + 콤마 구분 appVersion → 한글 12+ 거부
   - templet hwpx 의 version.xml / settings.xml 로 교체

3. **`_fix_rhwp_picture_orgsz_keep_clip`** + **`_fix_rhwp_equation_attrs`** (adapters/native.py)
   - 그림 orgSz=0×0 → curSz 값으로 채움 (imgClip 은 보존)
   - equation `version`/`font` 속성 swap 버그 복원

**Why:** 이 보정 없이 빌드한 출력은 "intent=2000 으로 들여쓰기 명시했는데 한글에서 들여쓰기 안 되는" 등 시각 차이 발생. 진단 시 paraPr 데이터는 동일해 보여 원인 추적 어려움. 진짜 원인은 `<hh:style>` 의 dangling paraPrIDRef.

**How to apply:** 새 양식/파이프라인 추가 시 splitter 의 호출 위치 (merge_styles → sanitize → ... → write → replace_meta) 그대로 따라야 함. 진단 시 paraPr 가 같아 보이면 hh:style 의 paraPrIDRef 부터 의심.
