"""Johab decomposition + jamo shape lookup tables.

Tables extracted from HncBaseDraw.dll (Hancom Office 12.x, raid 15 dump):
- DAT_100c6e20: cho (initial consonant) shape class remap
- DAT_100c6e40: jung (medial vowel) shape class remap
- DAT_100c6e60: jong (final consonant) shape class remap

These remap a raw 5-bit Johab value (0..31) to a shape class index.
255 (0xff) means "no shape" — used as sentinel for invalid Johab positions.
"""
from typing import Tuple


CHO_SHAPE_TABLE = (
    20, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
)

JUNG_SHAPE_TABLE = (
    255, 255, 0, 1, 2, 3, 4, 5, 255, 255, 6, 7, 8, 9, 10, 11,
    255, 22, 12, 13, 14, 15, 16, 17, 23, 24, 18, 19, 20, 21, 25, 26,
)

JONG_SHAPE_TABLE = (
    28, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    15, 16, 29, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 30, 31,
)


def decompose_johab(code: int) -> Tuple[int, int, int]:
    """Decompose a Johab Hangul code (bit 15 set) into 5-bit (cho, jung, jong).

    bit layout (16 bits, MSB to LSB):
      15: 1 (syllable marker)
      14..10: cho (initial consonant) 0..31
      9..5: jung (medial vowel) 0..31
      4..0: jong (final consonant) 0..31
    """
    if not (code & 0x8000):
        raise ValueError(f"not a Johab syllable code: 0x{code:04x}")
    cho = (code >> 10) & 0x1f
    jung = (code >> 5) & 0x1f
    jong = code & 0x1f
    return cho, jung, jong


def shape_classes(code: int) -> Tuple[int, int, int]:
    """Decompose + remap a Johab code into (cho_shape, jung_shape, jong_shape).

    Returns -1 for any component whose shape table entry is 255 (invalid).
    """
    cho, jung, jong = decompose_johab(code)
    cs = CHO_SHAPE_TABLE[cho]
    js = JUNG_SHAPE_TABLE[jung]
    gs = JONG_SHAPE_TABLE[jong]
    return (cs if cs != 255 else -1,
            js if js != 255 else -1,
            gs if gs != 255 else -1)


def unicode_to_johab(u: int) -> int:
    """Convert a Unicode Hangul Syllable codepoint (AC00..D7A3) to Hwp's
    internal Johab-like encoding.

    Verified via Frida raid 16 capture against Hwp.exe (HncBaseDraw.dll v12.x):
    Hwp's cho encoding is `cho_uni + 2` (i.e. one MORE than standard Johab's
    1-indexed scheme). Examples from Frida:
        가 (cho_uni=0) → Hwp cho 2
        잭 (cho_uni=12) → Hwp cho 14
        한 (cho_uni=18) → Hwp cho 20
    """
    if not (0xAC00 <= u <= 0xD7A3):
        raise ValueError(f"not a Hangul Syllable: U+{u:04X}")
    rel = u - 0xAC00
    cho_uni = rel // (21 * 28)
    rest = rel % (21 * 28)
    jung_uni = rest // 28
    jong_uni = rest % 28

    # Hwp cho = Unicode 0-indexed cho + 2 (verified Frida raid 16)
    cho_hwp = cho_uni + 2

    # Hwp jung mapping (verified Frida raid 16):
    #   잭 (jung_uni=1, ㅐ) → Hwp jung 4
    #   한 (jung_uni=0, ㅏ) → Hwp jung 3
    JUNG_UNI_TO_HWP = [
        3, 4, 5, 6, 7,    # ㅏㅐㅑㅒㅓ
        10, 11, 12, 13, 14, 15,   # ㅔㅕㅖㅗㅘㅙ
        18, 19, 20, 21, 22, 23,   # ㅚㅛㅜㅝㅞㅟ
        26, 27, 28, 29,           # ㅠㅡㅢㅣ
    ]
    jung_hwp = JUNG_UNI_TO_HWP[jung_uni]

    # Hwp jong mapping (verified Frida raid 16). Hwp/Johab jong values
    # 0,1 = filler; 18 = filler; 28 = filler. The Unicode jong table is
    # contiguous (0..27) so we use a lookup that skips Hwp's filler slots.
    #   잭 (jong_uni=1, ㄱ)  → Hwp jong 2  (+1)
    #   한 (jong_uni=4, ㄴ)  → Hwp jong 5  (+1)
    #   강 (jong_uni=21, ㅇ) → Hwp jong 23 (+2; skip Hwp filler at 18)
    # When jong is absent (jong_uni=0), Hwp uses filler value 1.
    JONG_UNI_TO_HWP = [
        1,                          # 0: filler (no jong)
        2, 3, 4, 5, 6, 7, 8,         # 1..7: ㄱㄲㄳㄴㄵㄶㄷ
        9, 10, 11, 12, 13, 14, 15, 16,  # 8..15: ㄹㄺㄻㄼㄽㄾㄿㅀ
        17,                          # 16: ㅁ
        19, 20, 21, 22, 23,          # 17..21: ㅂㅄㅅㅆㅇ (skip Hwp 18 filler)
        24, 25, 26, 27,              # 22..25: ㅈㅊㅋㅌ
        29, 30,                      # 26..27: ㅍㅎ (skip Hwp 28 filler)
    ]
    jong_hwp = JONG_UNI_TO_HWP[jong_uni]

    return 0x8000 | (cho_hwp << 10) | (jung_hwp << 5) | jong_hwp


if __name__ == "__main__":
    # Self-test: 잭 (U+C7AC)
    for u in [0xAC00, 0xAC01, 0xAC02, 0xC7AC, 0xD55C]:
        try:
            j = unicode_to_johab(u)
            cho, jung, jong = decompose_johab(j)
            cs, js, gs = shape_classes(j)
            print(f"U+{u:04X} → Johab 0x{j:04X} = cho:{cho} jung:{jung} jong:{jong} → shapes: ({cs}, {js}, {gs})")
        except Exception as e:
            print(f"U+{u:04X}: error {e}")
