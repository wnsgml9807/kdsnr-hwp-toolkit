"""End-to-end syllable renderer using HGMJ-style HFT fonts.

Two rendering paths discovered (per FUN_100ac080 in HncBaseDraw.dll):

  TYPE 3 (precomposed): direct single-bitmap lookup
    Bit re-shuffle: idx = ((c >> 5) & 0x3e0) | ((c & 0x20) << 5) | (c & 0x1f)
    Produces a unique index per Johab syllable code. This is what Hwp uses
    for the bulk of Hangul Syllable rendering — one bitmap per syllable.

  TYPE 2 (composition): cho/jung/jong jamo bitmap OR-blit
    More compact (jamo bitmaps shared across syllables).
    Used as fallback when type 3 doesn't cover a code.

Try type 3 first; fall back to type 2 if char outside type 3's accepted range.
"""
from __future__ import annotations
from typing import List, Optional

try:
    from .hft_johab import shape_classes, unicode_to_johab
    from .hft_parser import HftFile, Descriptor
    from .hft_inner_table import parse_inner_table, InnerTable
    from .hft_bitmap import extract_bitmap, bitmap_to_pixels, or_blit, render_ascii
except ImportError:
    from hft_johab import shape_classes, unicode_to_johab
    from hft_parser import HftFile, Descriptor
    from hft_inner_table import parse_inner_table, InnerTable
    from hft_bitmap import extract_bitmap, bitmap_to_pixels, or_blit, render_ascii


def type3_bitmap_index(char_code: int) -> int:
    """Compute type 3 (precomposed) bitmap index from a 16-bit char code.

    Formula from FUN_100ac080 case 3:
        idx = ((c >> 5) & 0x3e0) | ((c & 0x20) << 5) | (c & 0x1f)

    For Johab Hangul (bit 15 set):
        bit 10 of idx = bit 0 of jung (jung parity)
        bits 5..9 of idx = cho raw value
        bits 0..4 of idx = jong raw value
    """
    c = char_code & 0xFFFF
    return ((c >> 5) & 0x3e0) | ((c & 0x20) << 5) | (c & 0x1f)


def type3_is_eligible(char_code: int) -> bool:
    """Check whether a char_code is eligible for type 3 lookup.

    Per FUN_100ac080 case 3:
        - (c & 0x83c0) == 0x8000 (Hangul Syllables range bit pattern)
        - c < 0xe829 OR c > 0xe83f (avoid certain reserved range)
        - additional byte tests on c's high byte
    """
    c = char_code & 0xFFFF
    if (c & 0x83c0) != 0x8000:
        return False
    if 0xe829 <= c <= 0xe83f:
        return False
    return True


def render_syllable_type3(hft: HftFile, char_code: int, em: int = 17) -> Optional[List[List[int]]]:
    """Render a precomposed syllable bitmap via type 3 lookup."""
    desc = hft.find_descriptor(em=em, type_filter=3)
    if desc is None or not desc.is_bitmap:
        return None
    idx = type3_bitmap_index(char_code)
    if not (0 <= idx < desc.count):
        return None
    b = extract_bitmap(desc, idx)
    return bitmap_to_pixels(b, desc.width, desc.height, desc.bytes_per_row)


def render_syllable_type2(hft: HftFile, char_code: int, em: int = 17) -> Optional[List[List[int]]]:
    """Render a syllable via type 2 jamo composition."""
    cs, js, gs = shape_classes(char_code)

    desc = hft.find_descriptor(em=em, type_filter=2)
    if desc is None:
        return None
    inner = parse_inner_table(desc.inner_table)

    indices = inner.bitmap_indices(cs, js, gs)
    if indices is None:
        return None
    cho_idx, jung_idx, jong_idx = indices

    base = [[0] * desc.width for _ in range(desc.height)]
    if cs != 0 and cho_idx is not None and 0 <= cho_idx < desc.count:
        b = extract_bitmap(desc, cho_idx)
        base = or_blit(base, bitmap_to_pixels(b, desc.width, desc.height, desc.bytes_per_row))
    if js != 0 and jung_idx is not None and 0 <= jung_idx < desc.count:
        b = extract_bitmap(desc, jung_idx)
        base = or_blit(base, bitmap_to_pixels(b, desc.width, desc.height, desc.bytes_per_row))
    if gs != 0 and jong_idx is not None and 0 <= jong_idx < desc.count:
        b = extract_bitmap(desc, jong_idx)
        base = or_blit(base, bitmap_to_pixels(b, desc.width, desc.height, desc.bytes_per_row))
    return base


def render_syllable(hft: HftFile, unicode_cp: int, em: int = 17) -> List[List[int]]:
    """Render a single Korean syllable from a Hangul Syllables codepoint.

    Tries type 3 (precomposed) first per Hwp's iteration order in FUN_100ac080,
    then falls back to type 2 (composition).
    """
    johab = unicode_to_johab(unicode_cp)

    # Type 3 first (precomposed)
    if type3_is_eligible(johab):
        pixels = render_syllable_type3(hft, johab, em)
        if pixels is not None:
            return pixels

    # Fallback: type 2 composition
    pixels = render_syllable_type2(hft, johab, em)
    if pixels is not None:
        return pixels

    raise ValueError(f"unable to render U+{unicode_cp:04X} (johab=0x{johab:04x})")


def render_syllable_with_debug(hft: HftFile, unicode_cp: int, em: int = 17) -> dict:
    """Render + return debug info."""
    johab = unicode_to_johab(unicode_cp)
    cs, js, gs = shape_classes(johab)
    type3_idx = type3_bitmap_index(johab) if type3_is_eligible(johab) else None

    debug = {
        "unicode": unicode_cp, "johab": johab,
        "shape_classes": (cs, js, gs),
        "type3_idx": type3_idx,
        "pixels": render_syllable(hft, unicode_cp, em),
    }
    return debug


if __name__ == "__main__":
    import sys
    sys.path.insert(0, "src" if "/work/hft-decoder" in __file__ else ".")
    from hft_parser import parse

    hft = parse("/tmp/HGMJ.HFT")

    for u in [0xAC00, 0xC7AC, 0xD55C, 0xAC04, 0xB098]:  # 가, 잭, 한, 간, 나
        try:
            ch = chr(u)
            info = render_syllable_with_debug(hft, u)
            print(f"\n{'=' * 30}")
            print(f"U+{u:04X} '{ch}': johab=0x{info['johab']:04x}, shapes={info['shape_classes']}, "
                  f"remap={info['remap']}, combined_idx={info['combined_idx']}, indices={info['indices']}")
            for line in render_ascii(info['pixels']).split("\n"):
                print(f"  |{line}|")
        except Exception as e:
            print(f"U+{u:04X}: error {e}")
