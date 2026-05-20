"""Variable-length signed int decoder used by HFT path opcode streams.

Encoding (per raid 11 reverse-engineering of FUN_10027e90 + verified against
HCHGGGT.HFT entry walks):

    First byte read as i8:
      -123 <= b <= 123       → 1 byte, value = b
      b >= 124               → 2 bytes, value = b * 256 + next - 0x7b84
      -127 <= b <= -124      → 2 bytes, value = b * 256 - next + 0x7b84
      b == -128              → 3 bytes, value = i16 LE of bytes [1..2]
"""
import struct


def read_varint(buf: bytes, pos: int):
    """Read one variable-length signed int. Returns (value, new_pos)."""
    b = struct.unpack_from('<b', buf, pos)[0]
    if -123 <= b <= 123:
        return b, pos + 1
    if b >= 124:
        return b * 256 + buf[pos + 1] - 0x7b84, pos + 2
    if -127 <= b <= -124:
        return b * 256 - buf[pos + 1] + 0x7b84, pos + 2
    # b == -128
    val = struct.unpack_from('<h', buf, pos + 1)[0]
    return val, pos + 3


def encode_varint(value: int) -> bytes:
    """Round-trip helper: encode a signed int to its var-length form."""
    if -123 <= value <= 123:
        return struct.pack('<b', value)
    # 2-byte forms — find which side the value falls on
    if 124 * 256 <= value + 0x7b84 < 128 * 256:
        b = (value + 0x7b84) // 256
        if 124 <= b <= 127:
            r = value + 0x7b84 - b * 256
            if 0 <= r < 256:
                return struct.pack('<bB', b, r)
    if -127 * 256 + 0x7b84 < 0x7b84 - value <= -124 * 256 + 0x7b84 + 255:
        for b in range(-127, -123):
            r = b * 256 - value + 0x7b84
            if 0 <= r < 256:
                return struct.pack('<bB', b, r)
    # Fallback: 3-byte form
    return b'\x80' + struct.pack('<h', value)
