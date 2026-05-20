"""HFT file parser — chunk + descriptor extraction.

Disk layout (verified raid 15):

    File header: 0x200 bytes (signature + checksum, see hft_header.py — TODO)
    Body @ 0x200: chunk linked list

    Each chunk (14-byte header + descriptors):
      +0x00 u32  total_chunk_size (incl this header)
      +0x04 u16  chunk_code (usually = em size)
      +0x06 i16  discriminator (0 = simple, !=0 = composition)
      +0x08 u16  desc_count
      +0x0a i32  local_e (offset adjust to first descriptor; = 14 typically)

    Each descriptor (variable size = rec_sz):
      +0x00 u32  record_size (incl inner table)
      +0x04 u16  flags (low nibble = type 0-4, bit4 = bitmap mode flag)
      +0x06 u16  range_start
      +0x08 u16  range_end
      +0x0a u16  count (number of glyph entries)
      +0x0c u16  em (size class)
      +0x0e i32  int_at_14 (metric, typically ascent<<16 | 0)
      +0x12 i32  int_at_18 (bitmap dims: (width<<16) | height)
      +0x16      inner table (if type 1/2/4) + glyph data section

    Inner table (type 1/2/4):
      +0x00 u32  header (low16 = inner_size, high16 = upper marker)
      +0x04..inner_size: composition lookup tables

    Glyph data section (size = rec_sz - 22 - inner_size):
      BITMAP (bit4=1): stride = (width+7)/8 * height, total = count * stride
      VECTOR (bit4=0): u32[count] offset table + path opcode data blobs
"""
from __future__ import annotations
import struct
from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class Descriptor:
    """A single descriptor inside a chunk."""
    offset: int                 # absolute file offset of descriptor start
    record_size: int
    type: int                   # 0..4
    is_bitmap: bool             # flags bit4 (=0x10)
    range_start: int
    range_end: int
    count: int
    em: int
    width: int
    height: int
    bytes_per_row: int
    stride: int                 # bytes per glyph (bitmap) or 0 (vector)
    inner_table: bytes          # raw inner table body (excluding 4-byte header)
    inner_header: int           # the u32 header value
    glyph_data: bytes           # raw glyph data section


@dataclass
class Chunk:
    """A chunk in the HFT body."""
    offset: int
    size: int
    chunk_code: int
    desc_count: int
    descriptors: List[Descriptor] = field(default_factory=list)


@dataclass
class HftFile:
    """Parsed HFT file."""
    path: str
    raw: bytes
    chunks: List[Chunk] = field(default_factory=list)

    def find_descriptor(self, em: int, type_filter: Optional[int] = None) -> Optional[Descriptor]:
        """Find the first descriptor matching the given em size (and optional type)."""
        for ch in self.chunks:
            for d in ch.descriptors:
                if d.em == em and (type_filter is None or d.type == type_filter):
                    return d
        return None

    def all_size_tiers(self) -> List[int]:
        return sorted(set(ch.chunk_code for ch in self.chunks if ch.descriptors))


def parse(path: str) -> HftFile:
    """Parse an HFT file into structured chunks + descriptors."""
    with open(path, "rb") as f:
        data = f.read()

    hft = HftFile(path=path, raw=data)

    pos = 0x200
    while pos + 4 <= len(data):
        sz = struct.unpack_from('<I', data, pos)[0]
        if sz == 0 or sz > 0x1000000 or pos + sz > len(data):
            break
        if sz >= 14:
            hft.chunks.append(_parse_chunk(data, pos, sz))
        pos += sz

    return hft


def _parse_chunk(data: bytes, chunk_off: int, chunk_size: int) -> Chunk:
    hdr = data[chunk_off:chunk_off + 14]
    chunk_code = struct.unpack_from('<H', hdr, 4)[0]
    desc_count = struct.unpack_from('<H', hdr, 8)[0]
    local_e = struct.unpack_from('<i', hdr, 10)[0]

    chunk = Chunk(offset=chunk_off, size=chunk_size,
                  chunk_code=chunk_code, desc_count=desc_count)

    cur = chunk_off + local_e
    chunk_end = chunk_off + chunk_size
    for _ in range(desc_count):
        if cur + 22 > min(chunk_end, len(data)):
            break
        chunk.descriptors.append(_parse_descriptor(data, cur))
        cur += chunk.descriptors[-1].record_size

    return chunk


def _parse_descriptor(data: bytes, off: int) -> Descriptor:
    desc = data[off:off + 22]
    rec_sz = struct.unpack_from('<I', desc, 0)[0]
    flags = struct.unpack_from('<H', desc, 4)[0]
    rs = struct.unpack_from('<H', desc, 6)[0]
    re = struct.unpack_from('<H', desc, 8)[0]
    cnt = struct.unpack_from('<H', desc, 10)[0]
    em = struct.unpack_from('<H', desc, 12)[0]
    int_at_18 = struct.unpack_from('<i', desc, 18)[0]
    type_n = flags & 0xf
    is_bitmap = bool(flags & 0x10)
    width = int_at_18 & 0xFFFF
    height = (int_at_18 >> 16) & 0xFFFF

    after_hdr = off + 22
    inner_size = 0
    inner_header = 0
    inner_body = b''
    if type_n in (1, 2, 4) and after_hdr + 4 <= len(data):
        inner_header = struct.unpack_from('<I', data, after_hdr)[0]
        inner_size = inner_header & 0xFFFF
        if inner_size > 4 and after_hdr + inner_size <= len(data):
            inner_body = data[after_hdr + 4:after_hdr + inner_size]

    glyph_section_off = after_hdr + inner_size if type_n in (1, 2, 4) else after_hdr
    glyph_section_size = (off + rec_sz) - glyph_section_off
    glyph_data = data[glyph_section_off:glyph_section_off + glyph_section_size]

    bytes_per_row = (width + 7) // 8 if width > 0 else 0
    stride = bytes_per_row * height if is_bitmap else 0

    return Descriptor(
        offset=off, record_size=rec_sz, type=type_n, is_bitmap=is_bitmap,
        range_start=rs, range_end=re, count=cnt, em=em,
        width=width, height=height, bytes_per_row=bytes_per_row, stride=stride,
        inner_table=inner_body, inner_header=inner_header, glyph_data=glyph_data,
    )


if __name__ == "__main__":
    import sys
    path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/HGMJ.HFT"
    hft = parse(path)
    print(f"Parsed {path}: {len(hft.chunks)} chunks, size_tiers = {hft.all_size_tiers()}")
    for i, ch in enumerate(hft.chunks):
        print(f"\nChunk {i+1} @ 0x{ch.offset:x}, size={ch.size}, code={ch.chunk_code}, descs={ch.desc_count}")
        for j, d in enumerate(ch.descriptors):
            kind = "BITMAP" if d.is_bitmap else "VECTOR"
            print(f"  Desc {j}: type={d.type} {kind}, range=0x{d.range_start:x}..0x{d.range_end:x}, "
                  f"count={d.count}, em={d.em}, {d.width}x{d.height}, "
                  f"stride={d.stride}, inner={len(d.inner_table)}B, glyph={len(d.glyph_data)}B")
