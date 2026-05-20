"""Vector path opcode parser for type 1 HFT descriptors (HCHGGGT-style fonts).

Per FUN_100ad2c0 case 1 (verified raid 11/17):
  glyph_data section layout =
    +0x00..count*4-1: u32 offset table (per-glyph offset relative to section)
    +count*4..: concatenated path blobs

  Each blob layout depends on flags bit4 (=0x10):
    bit4 == 0:  [metrics: 8 bytes (4× int16)][size: u16][path data: size bytes]
    bit4 == 1:  [size: u16][path data: size bytes]   (metrics come from descriptor)

Path opcodes (from raid 11, verified against HCHGGGT entry decodes):

    0x00       EOF
    0x01 dx    lineto_x (relative)
    0x02 dy    lineto_y
    0x03 dx dy lineto
    0x04       close
    0x05 dx    moveto_x
    0x06 dy    moveto_y
    0x07 dx dy moveto
    0x09 v1..v4  cube4_y (cubic bezier, y-only deltas?)
    0x0A v1..v4  cube4_x
    0x0B v1..v6  cube6 (cubic bezier 3 control points relative)
    0x20 ...   complex contour spec (n records + extra)
    0x21 ...   bytestream list
    0x22 b     marker 0x22
    0x23 b     marker 0x23
    0x40 a b   M0 (metric pair)
    0x41 ...5  M1
    0x42 a b   M2 (advance, sidebearing)
    0x43 ...5  M3
    0x44       toggle
"""
from __future__ import annotations
import struct
from dataclasses import dataclass, field
from typing import List, Optional, Tuple

try:
    from .hft_parser import Descriptor
    from .hft_varint import read_varint
    from .hft_cipher import CipherKey, decrypt
except ImportError:
    from hft_parser import Descriptor
    from hft_varint import read_varint
    from hft_cipher import CipherKey, decrypt


@dataclass
class PathBlob:
    """One glyph's raw path bytes plus metrics."""
    metrics: Tuple[int, int, int, int]   # 4× int16 from blob header
    raw: bytes


def lookup_type1(desc: Descriptor, char_code: int) -> Optional[int]:
    """Binary-search the type 1 inner table for char_code. Returns bitmap idx."""
    if desc.type != 1:
        raise ValueError(f"expected type=1 descriptor, got type={desc.type}")
    inner = desc.inner_table  # u16[count] sorted char codes
    n = desc.count
    if len(inner) < n * 2:
        return None
    lo, hi = 0, n - 1
    target = char_code & 0xFFFF
    while lo <= hi:
        mid = (lo + hi) >> 1
        v = struct.unpack_from('<H', inner, mid * 2)[0]
        if v < target:
            lo = mid + 1
        elif v > target:
            hi = mid - 1
        else:
            return mid
    return None


def extract_blob(desc: Descriptor, idx: int,
                 cipher: Optional["CipherKey"] = None) -> Optional[PathBlob]:
    """Extract the path blob for the given index. Returns None if idx invalid.

    Blob layout depends on flags bit4:
      bit4=0: [metrics: 8B][size: u16][path: size B]
      bit4=1: [size: u16][path: size B]   (metrics come from descriptor)

    If `cipher` is provided, the path bytes are decrypted before returning.
    The dispatcher type determines which cipher to use:
      type 0 → CIPHER_HJSMJ (Hanja/English)
      type 2 → CIPHER_HGMJ (Korean)
      type 1 → FUN_100248e0 stream cipher (state from global DAT_100f30dc)
    """
    if not (0 <= idx < desc.count):
        return None
    data = desc.glyph_data
    if (idx + 1) * 4 > len(data):
        return None
    blob_off = struct.unpack_from('<I', data, idx * 4)[0]
    if blob_off == 0 or blob_off + 2 > len(data):
        return None

    if not desc.is_bitmap:
        # bit4=0: 8 bytes of metrics precede size
        if blob_off + 10 > len(data):
            return None
        metrics = struct.unpack_from('<4h', data, blob_off)
        size = struct.unpack_from('<H', data, blob_off + 8)[0]
        body_off = blob_off + 10
    else:
        # bit4=1: descriptor carries metrics (int@14 / int@18), blob starts with size
        metrics = (desc.width, desc.height, 0, 0)
        size = struct.unpack_from('<H', data, blob_off)[0]
        body_off = blob_off + 2

    if size == 0 or body_off + size > len(data):
        return None
    raw = bytes(data[body_off:body_off + size])
    if cipher is not None:
        raw = decrypt(raw, cipher)
    return PathBlob(metrics=metrics, raw=raw)


# Path opcode walker — produces a marker/coord stream identical to what painter
# (FUN_10029c50, HncBaseDraw.dll @ +0x29c50) emits.
#
# Verified against:
#   1. path_painter.txt (Ghidra decompile of FUN_10029c50, raid 18)
#   2. /tmp/painter_dump2.log (Frida live capture, raid 19)
#   3. libhsp.dylib FUN_00091c30 — macOS native CGContext consumer (raid 20)
#
# Painter opcode → marker emit:
#   0x01/0x02/0x03 (ON-curve, "Move")  → marker = 1    (CGContextMoveToPoint)
#   0x05/0x06/0x07 ("Line")            → marker = 2    (CGContextAddLineToPoint)
#   0x09/0x0A/0x0B (cubic, 3-points)   → marker = 3 ×3 (CGContextAddCurveToPoint
#                                                      with coords[i], [i+1], [i+2])
#   0x04 / 0x00 (close)                → marker = 4    (CGContextClosePath)
#                                                      conditional: emitted only if
#                                                      the previous emit was Line/Cubic
#
# Also: when a Move (0x01-0x03) follows a Line/Cubic *without* an explicit close,
# the painter emits an implicit marker=4 first, then the marker=1.
#
# Low 2 bits of opcode: bit0=dx present, bit1=dy present (signed varint each).
# Cubic (9/A/B) reads 4 or 6 varints depending on bits — see _read_cubic_deltas.
#
# Marker semantics (from libhsp.dylib FUN_00091c30):
#   1 = MoveTo coords[i]                            advance 1
#   2 = LineTo coords[i]                            advance 1
#   3 = CurveTo(coords[i]=c1, [i+1]=c2, [i+2]=end)  advance 3
#   4 = ClosePath                                   advance 1
# No off-curve quadratic markers exist.

POINT = 'P'      # ON-curve point (markers 1=Move, 2=Line)
CLOSE = 'Z'      # close subpath (marker 4)
CUBIC = 'C'      # cubic Bezier — 3 ON-curve points (3× marker 3)


@dataclass
class PathCommand:
    op: str               # 'P', 'Z', 'C'
    marker: int = 0       # for op='P': 1=Move, 2=Line
    points: Tuple = ()    # (x, y) for P; () for Z; (cx1, cy1, cx2, cy2, ex, ey) for C


def walk_path(raw: bytes) -> List[PathCommand]:
    """Walk a raw path opcode stream and emit a command list matching the
    painter's marker/coord stream.

    Verified against macOS native painter FUN_000296ac in libHncBaseDraw.dylib
    (raid 22). Key invariant from macOS painter line 53-54:

        do {
            iVar8  = (int)param_2;     // = initial_x  (reset every iteration)
            iVar23 = (int)uVar25;      // = initial_y  (reset every iteration)
            ...
        } while (...)

    All conditional varint reads (in cases 1/2/3, 5/6/7, 9/A/B) default to the
    initial_x/y (typically 0) when the opcode bit is not set — NOT to the
    previous op's value. So we initialize dx=dy=0 at the start of *every*
    opcode iteration (no cross-op state).

    Cubic case 9/A/B varint count (from macos_painter.txt line 250-441):
        0x09: dx0 + local_24 + local_28 + local_18         (4 varints)
        0x0A: dy0 + local_24 + local_28 + local_1c         (4 varints)
        0x0B: dx0 + dy0 + local_24 + local_28 + local_1c + local_18 (6 varints)

    Cubic coordinates (chained deltas):
        c1  = (current_x + dx0,         current_y + dy0)
        c2  = (c1.x + local_24,         c1.y + local_28)
        end = (c2.x + local_1c,         c2.y + local_18)

    All conditional defaults are 0 — same as initial_x/y.
    """
    pos = 0
    x, y = 0, 0
    cmds: List[PathCommand] = []
    n = len(raw)
    # bVar3 in painter source: True when previous emit was Line(2)/Cubic(3),
    # False when previous emit was Move(1)/Close(4) or initial state.
    needs_close = False
    try:
      while pos < n:
        op = raw[pos]; pos += 1
        if op == 0x00 or op == 0x04:
            # Close (only if previous was Line/Cubic — matches painter case 0/4)
            if needs_close:
                cmds.append(PathCommand(CLOSE))
                needs_close = False
            if op == 0x00:
                break
        elif op in (0x01, 0x02, 0x03):
            # Move — emit implicit close first if previous was Line/Cubic
            if needs_close:
                cmds.append(PathCommand(CLOSE))
                needs_close = False
            dx = dy = 0
            if op & 1:
                dx, pos = read_varint(raw, pos)
            if op & 2:
                dy, pos = read_varint(raw, pos)
            x += dx
            y += dy
            cmds.append(PathCommand(POINT, marker=1, points=(x, y)))
        elif op in (0x05, 0x06, 0x07):
            # Line
            dx = dy = 0
            if op & 1:
                dx, pos = read_varint(raw, pos)
            if op & 2:
                dy, pos = read_varint(raw, pos)
            x += dx
            y += dy
            cmds.append(PathCommand(POINT, marker=2, points=(x, y)))
            needs_close = True
        elif op in (0x09, 0x0A, 0x0B):
            # Cubic. Chained delta semantics from macos_painter.txt line 397-432.
            dx0 = dy0 = 0
            if op & 1:
                dx0, pos = read_varint(raw, pos)
            if op & 2:
                dy0, pos = read_varint(raw, pos)
            d24, pos = read_varint(raw, pos)  # local_24 (always read)
            d28, pos = read_varint(raw, pos)  # local_28 (always read)
            d1c = d18 = 0                      # conditional defaults to 0
            if op & 2:
                d1c, pos = read_varint(raw, pos)
            if op & 1:
                d18, pos = read_varint(raw, pos)
            cx1 = x + dx0
            cy1 = y + dy0
            cx2 = cx1 + d24
            cy2 = cy1 + d28
            ex  = cx2 + d1c
            ey  = cy2 + d18
            cmds.append(PathCommand(CUBIC, points=(cx1, cy1, cx2, cy2, ex, ey)))
            x, y = ex, ey
            needs_close = True
        elif op in (0x40, 0x42):
            _, pos = read_varint(raw, pos)
            _, pos = read_varint(raw, pos)
        elif op in (0x41, 0x43):
            for _ in range(5):
                _, pos = read_varint(raw, pos)
        elif op == 0x44:
            pass
        elif op in (0x22, 0x23):
            pos += 1
        elif op == 0x20:
            if pos >= n: break
            nrec = raw[pos]; pos += 1 + 2 * nrec
            if pos + 4 > n: break
            extra = raw[pos + 3]
            pos += 4 + 2 * extra
        elif op == 0x21:
            if pos >= n: break
            nrec = raw[pos]; pos += 1
            for _ in range(nrec):
                if pos >= n: break
                ln = raw[pos]; pos += 1 + ln
        else:
            break
    except (IndexError, Exception):
        # raw was truncated mid-opcode; return partial command list
        pass
    # final implicit close if path ended with Line/Cubic
    if needs_close:
        cmds.append(PathCommand(CLOSE))
    return cmds


def to_svg_path(commands: List[PathCommand]) -> str:
    """Convert command list to SVG 'd' attribute string.

    This mirrors libhsp.dylib FUN_00091c30 (raid 20). Marker semantics:
        marker=1 → M (CGContextMoveToPoint)
        marker=2 → L (CGContextAddLineToPoint)
        cubic    → C (CGContextAddCurveToPoint with the 3 ON-curve points)
        close    → Z (CGContextClosePath)

    All outlines are Move + Line + Cubic + Close only. No off-curve quadratics.
    """
    parts: List[str] = []
    for c in commands:
        if c.op == CLOSE:
            parts.append("Z")
        elif c.op == CUBIC:
            cx1, cy1, cx2, cy2, ex, ey = c.points
            parts.append(f"C{cx1},{cy1} {cx2},{cy2} {ex},{ey}")
        else:  # POINT
            x, y = c.points
            if c.marker == 1:
                parts.append(f"M{x},{y}")
            elif c.marker == 2:
                parts.append(f"L{x},{y}")
    return "".join(parts)


def bbox(commands: List[PathCommand]) -> Tuple[int, int, int, int]:
    """Return (xmin, ymin, xmax, ymax) over all points in the command list."""
    xs, ys = [], []
    for c in commands:
        if c.op == POINT:
            xs.append(c.points[0]); ys.append(c.points[1])
        elif c.op == CUBIC:
            xs.extend([c.points[0], c.points[2], c.points[4]])
            ys.extend([c.points[1], c.points[3], c.points[5]])
    if not xs:
        return (0, 0, 0, 0)
    return (min(xs), min(ys), max(xs), max(ys))


def render_glyph_to_svg(desc: Descriptor, char_code: int,
                        em_box: int = 1000,
                        cipher: Optional["CipherKey"] = None) -> Optional[str]:
    """Full pipeline: char_code → SVG <path d="..."> string."""
    idx = lookup_type1(desc, char_code)
    if idx is None:
        return None
    blob = extract_blob(desc, idx, cipher=cipher)
    if blob is None:
        return None
    cmds = walk_path(blob.raw)
    d = to_svg_path(cmds)
    return d


def cipher_for_type(type_id: int) -> Optional["CipherKey"]:
    """Return the standard cipher key used by each dispatcher type.

    Returns None for type 1 (uses a different stream cipher whose state comes
    from a global, not statically reconstructable). HCHGGGT samples observed
    so far ship raw / unencrypted data so leaving cipher=None for type 1 is
    correct for those.
    """
    try:
        from .hft_cipher import CIPHER_HGMJ, CIPHER_HJSMJ
    except ImportError:
        from hft_cipher import CIPHER_HGMJ, CIPHER_HJSMJ
    if type_id == 0:
        return CIPHER_HJSMJ
    if type_id == 2:
        return CIPHER_HGMJ
    return None
