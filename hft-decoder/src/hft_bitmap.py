"""Bitmap glyph extraction + rendering for HGMJ-style HFT fonts.

Per raid 15: HGMJ.HFT stores fixed-stride bitmap glyphs.
Each glyph = (width+7)/8 * height bytes, MSB-first bit order, row by row.

For Korean composition (type=2, bit4=1), the glyph index for a syllable is
computed from cho/jung/jong shape classes via the inner table's 3D lookup.

For pre-composed Hangul (type=3, bit4=1), index = bit-manipulation of char code.
"""
from typing import List
try:
    from .hft_parser import Descriptor
except ImportError:
    from hft_parser import Descriptor


def extract_bitmap(desc: Descriptor, idx: int) -> bytes:
    """Extract the raw bitmap bytes for a glyph at the given index."""
    if not desc.is_bitmap:
        raise ValueError("descriptor is not a bitmap descriptor")
    if not (0 <= idx < desc.count):
        raise IndexError(f"glyph index {idx} out of range (0..{desc.count - 1})")
    return desc.glyph_data[idx * desc.stride : (idx + 1) * desc.stride]


def bitmap_to_pixels(data: bytes, width: int, height: int, bytes_per_row: int) -> List[List[int]]:
    """Convert raw bitmap bytes to a 2D list of 0/1 pixels."""
    rows = []
    for r in range(height):
        row = []
        for c in range(width):
            byte_idx = r * bytes_per_row + c // 8
            bit_pos = 7 - (c % 8)
            row.append(1 if (byte_idx < len(data) and data[byte_idx] & (1 << bit_pos)) else 0)
        rows.append(row)
    return rows


def or_blit(base: List[List[int]], overlay: List[List[int]]) -> List[List[int]]:
    """OR-blit overlay onto base (used for Korean jamo composition)."""
    h = max(len(base), len(overlay))
    w = max((max((len(r) for r in base), default=0)),
            (max((len(r) for r in overlay), default=0)))
    out = [[0] * w for _ in range(h)]
    for r in range(h):
        for c in range(w):
            v = 0
            if r < len(base) and c < len(base[r]):
                v |= base[r][c]
            if r < len(overlay) and c < len(overlay[r]):
                v |= overlay[r][c]
            out[r][c] = v
    return out


def render_ascii(pixels: List[List[int]], on: str = "█", off: str = " ") -> str:
    """Render a 2D pixel grid as a multi-line string."""
    return "\n".join("".join(on if v else off for v in row) for row in pixels)


def render_pgm(pixels: List[List[int]]) -> bytes:
    """Render pixels as 8-bit PGM (256 grayscale, 1-bit only — 0 or 255)."""
    h = len(pixels)
    w = len(pixels[0]) if h > 0 else 0
    header = f"P5\n{w} {h}\n255\n".encode("ascii")
    data = bytearray()
    for row in pixels:
        for v in row:
            data.append(0 if v else 255)  # white background
    return header + bytes(data)
