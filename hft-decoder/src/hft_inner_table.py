"""Inner table parser for type 2/4 Korean composition descriptors.

Memory layout (relative to runtime piVar11 = inner_table_ptr):

    +0x00..+0x07: x_offset_table_ptr (int*), y_offset_table_ptr (int*) — runtime alloc
    +0x08..+0x0b: max_cho_shape, max_jung_shape, max_jong_shape, pad
    +0x0c..+0x4b: u16[32] cho_jamo_base_offset  (per cho_shape)
    +0x4c..+0x8b: u16[32] jung_jamo_base_offset (per jung_shape)
    +0x8c..+0xcb: u16[32] jong_jamo_base_offset (per jong_shape)
    +0xcc..+0xcf: max_cho_count, max_jung_count, max_jong_count, pad
    +0xd0..+0xef: byte[32] cho_shape → flat_cho_idx (compact)
    +0xf0..+0x10f: byte[32] jung_shape → flat_jung_idx
    +0x110..+0x12f: byte[32] jong_shape → flat_jong_idx
    +0x130..: three 3D tables of (max_cho × max_jung × max_jong) bytes each
        - tables[0]: cho-position bitmap_index offset
        - tables[1]: jung-position bitmap_index offset
        - tables[2]: jong-position bitmap_index offset

The on-disk inner_table body (after 4-byte u32 header) corresponds to memory
starting at +0x08 (the +0x00..+0x07 pointers are runtime-only).

★ CRITICAL (verified via Frida raid 16):
The disk u16 arrays store COUNTS, not OFFSETS. The HFT loader transforms them
in-place via cumulative prefix-sum that CARRIES ACROSS all three position arrays
(cho → jung → jong, single accumulator). For each position, only entries
0..max_shape_for_position are transformed (inclusive); the rest are untouched
and the next position picks up where this one left off.

Pseudocode of the loader transform (FUN_100ab9a0):
    acc = 0
    for position in (cho, jung, jong):
        for shape_idx in 0..max_shape_for_position:  # inclusive
            old = arr[shape_idx]
            arr[shape_idx] = acc
            acc += old

After the transform the values are cumulative base offsets used as below.

Glyph index calculation for a Korean syllable with shape classes (cs, js, gs):

    flat_cho  = cho_remap[cs]
    flat_jung = jung_remap[js]
    flat_jong = jong_remap[gs]
    iVar16 = (max_jung * flat_cho + flat_jung) * max_jong + flat_jong
    iVar13 = max_cho * max_jung * max_jong

    cho_bitmap_idx  = cho_jamo_base[cs]  + table_3d[0 * iVar13 + iVar16]
    jung_bitmap_idx = jung_jamo_base[js] + table_3d[1 * iVar13 + iVar16]
    jong_bitmap_idx = jong_jamo_base[gs] + table_3d[2 * iVar13 + iVar16]

Each *bitmap_idx is then a 0-indexed entry in the descriptor's glyph_data
array. Compose the syllable by OR-blitting the 3 bitmaps.
"""
from __future__ import annotations
import struct
from dataclasses import dataclass
from typing import List, Optional


@dataclass
class InnerTable:
    """Parsed type 2/4 inner table."""
    max_cho_shape: int
    max_jung_shape: int
    max_jong_shape: int

    cho_jamo_base: List[int]   # u16[32] per cho_shape
    jung_jamo_base: List[int]  # u16[32] per jung_shape
    jong_jamo_base: List[int]  # u16[32] per jong_shape

    max_cho_count: int
    max_jung_count: int
    max_jong_count: int

    cho_remap: List[int]   # byte[32] cho_shape → flat
    jung_remap: List[int]  # byte[32]
    jong_remap: List[int]  # byte[32]

    table_3d: bytes  # raw bytes for the three 3D tables concatenated

    @property
    def per_position_size(self) -> int:
        return self.max_cho_count * self.max_jung_count * self.max_jong_count

    def combined_index(self, cs: int, js: int, gs: int) -> int:
        """Compute the iVar16 = combined index in the 3D table."""
        flat_cho = self.cho_remap[cs] if 0 <= cs < 32 else 0
        flat_jung = self.jung_remap[js] if 0 <= js < 32 else 0
        flat_jong = self.jong_remap[gs] if 0 <= gs < 32 else 0
        if flat_cho >= self.max_cho_count: return -1
        if flat_jung >= self.max_jung_count: return -1
        if flat_jong >= self.max_jong_count: return -1
        return (self.max_jung_count * flat_cho + flat_jung) * self.max_jong_count + flat_jong

    def bitmap_indices(self, cs: int, js: int, gs: int) -> Optional[tuple]:
        """Return (cho_idx, jung_idx, jong_idx) bitmap indices for this syllable.

        cs/js/gs are shape classes from CHO_SHAPE_TABLE etc. Use -1 for "not present".
        Returns None if the syllable is unsupported.
        """
        idx = -1 if (cs < 0 or js < 0) else self.combined_index(cs, js, gs if gs >= 0 else 0)
        if idx < 0:
            return None
        ps = self.per_position_size
        cho_off  = self.table_3d[0 * ps + idx]
        jung_off = self.table_3d[1 * ps + idx]
        jong_off = self.table_3d[2 * ps + idx]

        cho_idx  = self.cho_jamo_base[cs]  + cho_off  if cs >= 0 else None
        jung_idx = self.jung_jamo_base[js] + jung_off if js >= 0 else None
        jong_idx = self.jong_jamo_base[gs] + jong_off if gs >= 0 else None
        return (cho_idx, jung_idx, jong_idx)


def _prefix_sum_3way(cho_counts, jung_counts, jong_counts,
                     max_cs, max_js, max_gs):
    """Apply the in-place cumulative prefix-sum transform that the runtime
    loader (FUN_100ab9a0) performs. The accumulator carries across all three
    position arrays in sequence. Each array's loop iterates 0..max_shape
    (inclusive); entries beyond max_shape remain unchanged.

    Returns three new lists in transformed (offset) form.
    """
    cho = list(cho_counts)
    jung = list(jung_counts)
    jong = list(jong_counts)
    acc = 0
    for arr, max_s in ((cho, max_cs), (jung, max_js), (jong, max_gs)):
        for i in range(max_s + 1):
            old = arr[i] & 0xFFFF
            # Read as signed 16-bit to match `short` semantics in C
            if old & 0x8000:
                old -= 0x10000
            arr[i] = acc & 0xFFFF
            acc = (acc + old) & 0xFFFF
    return cho, jung, jong


def parse_inner_table(body: bytes) -> InnerTable:
    """Parse the inner table body (after the 4-byte u32 header).

    Performs the same prefix-sum transform that the runtime loader applies,
    so cho_jamo_base / jung_jamo_base / jong_jamo_base are returned as
    *runtime offsets* ready for the bitmap_idx formula.
    """
    if len(body) < 0x130 - 8:
        raise ValueError(f"inner table too short: {len(body)} bytes")

    max_cs = body[0]
    max_js = body[1]
    max_gs = body[2]
    # body[3] reserved

    cho_counts = list(struct.unpack_from('<32H', body, 0x04))
    jung_counts = list(struct.unpack_from('<32H', body, 0x44))
    jong_counts = list(struct.unpack_from('<32H', body, 0x84))

    cho_jamo_base, jung_jamo_base, jong_jamo_base = _prefix_sum_3way(
        cho_counts, jung_counts, jong_counts, max_cs, max_js, max_gs)

    max_cc = body[0xc4]
    max_jc = body[0xc5]
    max_gc = body[0xc6]

    cho_remap = list(body[0xc8:0xc8 + 32])
    jung_remap = list(body[0xe8:0xe8 + 32])
    jong_remap = list(body[0x108:0x108 + 32])

    table_3d = bytes(body[0x128:])

    return InnerTable(
        max_cho_shape=max_cs, max_jung_shape=max_js, max_jong_shape=max_gs,
        cho_jamo_base=cho_jamo_base,
        jung_jamo_base=jung_jamo_base,
        jong_jamo_base=jong_jamo_base,
        max_cho_count=max_cc, max_jung_count=max_jc, max_jong_count=max_gc,
        cho_remap=cho_remap, jung_remap=jung_remap, jong_remap=jong_remap,
        table_3d=table_3d,
    )


if __name__ == "__main__":
    import sys
    sys.path.insert(0, "src" if "/work/hft-decoder" in __file__ else ".")
    from hft_parser import parse

    hft = parse("/tmp/HGMJ.HFT")
    d = hft.find_descriptor(em=17, type_filter=2)
    inner = parse_inner_table(d.inner_table)
    print(f"Inner table for em={d.em} (size={len(d.inner_table)} bytes):")
    print(f"  max shapes: cho={inner.max_cho_shape}, jung={inner.max_jung_shape}, jong={inner.max_jong_shape}")
    print(f"  max counts: cho={inner.max_cho_count}, jung={inner.max_jung_count}, jong={inner.max_jong_count}")
    print(f"  per_position_size = {inner.per_position_size}")
    print(f"  table_3d size = {len(inner.table_3d)} (expect {3 * inner.per_position_size})")
    print(f"  cho_jamo_base[:5] = {inner.cho_jamo_base[:5]}")
    print(f"  jung_jamo_base[:5] = {inner.jung_jamo_base[:5]}")
    print(f"  cho_remap[:10] = {inner.cho_remap[:10]}")
