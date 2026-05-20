---
name: hft-decoder 완성 (Rust + Python)
description: kdsnr-hwp-toolkit/hft-decoder 가 이미 6 HFT family (Korean 명조/신명조, Hanja bitmap/vector, English vector, Hanja bold) 모두 완성. cargo test 5/5 pass. 더 이상 byte-eq port 진행률에 포함시키지 말 것.
metadata:
  type: reference
  originSessionId: b2b2688b-7d2d-4212-9fa9-ee199c9adb0d
---

# hft-decoder 완성 상태 (2026-05-17 확인)

**위치**: `/Users/wnsgml/Desktop/새 폴더/Project/KSAT Agent/Agent_Streamlit/kdsnr-hwp-toolkit/hft-decoder/`

## 완성 family (한컴 HFT 6종 모두)

| Family | Example | Lookup | Storage | 상태 |
|---|---|---|---|---|
| Korean composition | HGMJ.HFT (한 궁명조) | type=2 + Johab compose | 16/24/40 px bitmap, jamo OR-blit | ✅ |
| Vector primitives | HCHGGGT.HFT | type=1 + binary search | Path opcode stream (1000 em) | ✅ |
| Hanja bitmap | HJSMJ.HFT (한자 신명조) | type=0 + direct idx | em=17 bitmap (raw) | ✅ |
| **Hanja vector** | HJSMJ.HFT large | type=0 + direct idx | em=1200 path opcodes, **cipher** | ✅ |
| **English vector** | ENSMJ.HFT (영문 신명조) | type=0 + direct idx | em=1000 path opcodes, **cipher** | ✅ |
| Hanja bold | HJGSMJ.HFT | type=0 | same family | ✅ |

## 두 cipher 정확 reproduce

- **type 0** (`FUN_10026B70`): state `0xE696`, const `0xC863`, mult `0xC73E` — Frida + Ghidra (raid 18) 로 RE
- **type 2** (`FUN_100ad9c0`): state `0xA729`, const `0xE696`

## 코드 구조

- **Python reference**: `src/hft_{parser,inner_table,johab,bitmap,vector,renderer,cipher}.py`
- **Rust production**: `rust/src/{lib,parser,inner_table,johab,bitmap,vector,renderer,cipher,cache,alias,ksx1001}.rs`
- **Tests**: `cargo test --lib` = **5/5 pass** (Python smoke `tests/test_layout.py` 6/6 pass)
- **Fixtures**: `rust/test-data/`

## API

```rust
use kdsnr_hft::{parse, HftCache, Glyph, render_syllable, category_for_code};

// 직접 사용
let hft = parse("HGMJ.HFT")?;
let pixels = render_syllable(&hft, '한' as u32)?;

// Cache 사용
let cache = HftCache::new();
let glyph: Glyph = cache.get_glyph("HGMJ", '한' as u32)?;
```

## byte-eq pipeline 진행률 영향

❌ **HFT decoder 는 byte-eq port 진행률 산정에서 제외** — 이미 100% 완성.
이전 산정에서 "Stage 3 paginator/HFT decoder ~5%" 라고 했던 것은 잘못. **Stage 3 = 100%**.

## HFT vs TTF dispatch (2026-05-17 검증)

한컴 macOS 의 텍스트 그리기는 **font type 별 dispatch**:

| Font type | dylib 위치 | 처리 |
|---|---|---|
| **HFT** (`#태고딕`, `신명 디나루` 등 한컴 자체 폰트) | `libHncFontLib.arm64.dylib` (1.4MB, 별도 dylib) | ✅ **kdsnr-hft 가 1:1 port 완료** |
| **TTF** (`궁서체`, `한컴바탕`, `함초롬바탕`, `Pretendard` 등 시스템 폰트) | `libHncDrawingEngine` 의 `FUN_0x07ae80` (~800B) | ⏸️ macOS CoreText API (`_CTFontCreateWithName` × 5 fallback + `_CTFontCreatePathForGlyph`) wrapper. 별도 port 필요 |

**input audit (toolkit 전체 hwpx 의 `<hh:font type=...>` 분포)**:
- TTF: 8610건 (51%)
- HFT: 8157건 (49%)

→ 둘 다 critical. HFT 는 ✅ 완료, TTF 는 ⏸️ port 대기 (L-5c-RE-5b2-ttf).

## How to apply

- HFT 폰트 처리가 필요할 때 본 crate (`kdsnr-hft`) 를 dependency 로 link
- 새로 byte-eq port 진행률 산정 시 HFT 영역은 포함하지 않음
- Render-engine 의 text 그리기 chain (CharItemView::Draw 의 GetCachedRenderPath
  callback 의 build_glyph_path) 에서 font type 으로 dispatch:
  - HFT → `kdsnr-hft::HftCache::get_glyph`
  - TTF → CoreText wrapper (별도 port, macOS) 또는 ab_glyph/rusttype (cross-platform)
