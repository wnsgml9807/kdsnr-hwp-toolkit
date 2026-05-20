---
name: HFT 디코더는 toolkit 모듈로, rhwp 편입 금지
description: HFT RE 결과물은 kdsnr-hwp-toolkit 내부 별도 sub-module 로 만들고, rhwp 에 편입시키지 말 것
type: feedback
originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---
HFT 정공법 RE (raid 1-15+) 의 결과물 (parser, decoder, Rust crate 등) 은 **rhwp 에 편입시키지 않는다**. 대신 `kdsnr-hwp-toolkit/` 내부에 별도 하위 폴더 (예: `kdsnr-hwp-toolkit/work/hft-decoder/` 또는 별도 crate 디렉토리) 로 isolated module 로 구성.

**Why:** rhwp 는 오픈소스 라이브러리 (좋은 일을 해주고 싶지 않은 외부 프로젝트). 우리 RE 노력의 결과를 rhwp 의 PR 로 보내지 말 것. 가치 있는 IP 는 우리 toolkit 안에서만 유지.

**How to apply:**
- HFT parser/decoder/renderer 신규 모듈 생성 시 `kdsnr-hwp-toolkit/` 하위에 디렉토리 만들고 자체적인 Cargo.toml / package.json 으로 독립 crate/module 화
- rhwp 의 `flat-hwp-parser/` 류 안에 직접 코드 두지 말 것 (외부 PR 가능성)
- rhwp 통합이 필요한 경우 toolkit 모듈을 외부 dependency 로 참조하는 방식만 허용
- 이름도 rhwp-aware (예: rhwp-font, rhwp-hft) 가 아니라 toolkit-native (예: ksat-hft, kdsnr-hft) 로
- **단, README/주석/`__init__` 등 모듈 외부에 보이는 곳에는 "rhwp 에 안 들어간다"는 문구를 직접 쓰지 말 것.** 정책은 지키되 광고하지 않음. 자연스럽게 독립 모듈로 보이게.
