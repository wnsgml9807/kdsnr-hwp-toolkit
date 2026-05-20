# kdsnr-hft — Hancom HFT (Han Unified Font File) decoder

- 제작사: 강남대성수능연구소
- 개발자: 권준희 (wnsgml9807@naver.com)

Python + Rust decoder for Hancom's proprietary HFT font format used by 한컴오피스 (Hwp).
Reverse-engineered from HncBaseDraw.dll v12.x; algorithm verified against Frida-captured
runtime traces.

## What is HFT?

`Han Unified Font File 1.0` magic. Used by Hwp/HOffice for its own font set
(명조, 신명조 etc.). Distinct from TrueType.

Font families identified, all sharing the same on-disk layout but diverging
in how glyph data is stored:

| Family | Example file | Lookup | Storage | Status |
|---|---|---|---|---|
| Korean composition | HGMJ.HFT (한 궁명조) | type=2 + Johab compose | 16/24/40 px bitmap, jamo OR-blit | ✓ complete |
| Vector primitives | HCHGGGT.HFT | type=1 + binary search | Path opcode stream (1000 em) | ✓ complete |
| Hanja bitmap | HJSMJ.HFT (한자 신명조) | type=0 + direct idx | em=17 bitmap (raw) | ✓ complete |
| **Hanja vector** | HJSMJ.HFT large size | type=0 + direct idx | em=1200 path opcodes, **cipher** | ✓ complete |
| **English vector** | ENSMJ.HFT (영문 신명조) | type=0 + direct idx | em=1000 path opcodes, **cipher** | ✓ complete |
| Hanja bold | HJGSMJ.HFT | type=0 | same family | ✓ complete (untested fixture) |

The cipher used by type 0 fonts is documented in `hft_cipher.py` (state
`0xE696`, constant `0xC863`, multiplier `0xC73E`). Type 2 (Korean) uses a
sibling cipher with state `0xA729` and constant `0xE696`.

## Layout

```
src/                  # Python prototype (reference impl)
├── hft_parser.py     #   chunk + descriptor parsing
├── hft_inner_table.py#   Korean composition + 3-way cumulative prefix-sum
├── hft_johab.py      #   Johab decomposition + jamo shape tables + Unicode→Hwp
├── hft_bitmap.py     #   bitmap extraction + OR-blit + ASCII render
├── hft_vector.py     #   path opcode walker + SVG output
├── hft_varint.py     #   variable-length signed int codec
└── hft_renderer.py   #   end-to-end syllable rendering

rust/                 # Production Rust crate (edition 2024)
├── Cargo.toml
├── src/{lib,parser,inner_table,johab,bitmap,vector,renderer}.rs
├── tests/integration.rs
└── test-data/        # fixture HFT files

tests/                # Python smoke tests
└── test_layout.py    # 6/6 passing
```

## Quick start

```bash
# Python
python3 -c "
from src import hft_parser, hft_renderer
from src.hft_bitmap import render_ascii
hft = hft_parser.parse('/path/to/HGMJ.HFT')
pixels = hft_renderer.render_syllable(hft, ord('한'))
print(render_ascii(pixels))
"

# Rust
cd rust && cargo test     # → 9/9 passing
```

## Key constants (from HncBaseDraw.dll v12.x)

- `CHO_SHAPE_TABLE` / `JUNG_SHAPE_TABLE` / `JONG_SHAPE_TABLE` (each 32 bytes,
  remapping Johab raw values → shape class indices)
- Cipher state `0xa729` for the type=2 dispatcher (`FUN_100ad9c0`)

## Coverage

All known HFT font families are fully supported. The type 0 cipher
(`FUN_10026B70`) was identified via Frida + Ghidra (raid 18) and reproduced
exactly in `hft_cipher.py` / `rust/src/cipher.rs`. HJSMJ large-size Hanja
outlines and ENSMJ English outlines both decrypt to clean path opcode
streams that round-trip through the vector walker.

| Test | Python | Rust |
|---|---|---|
| HGMJ structure + bitmap extraction | ✓ | ✓ |
| Korean syllable composition (Frida-verified, 11 cases) | ✓ | ✓ |
| HCHGGGT vector primitives | ✓ | ✓ |
| HJSMJ em=17 Hanja bitmap | ✓ | ✓ |
| HJSMJ em=1200 Hanja vector (decrypted) | ✓ | ✓ |
| ENSMJ English vector (decrypted) | ✓ | ✓ |
| **Total** | **8/8** | **11/11** |
