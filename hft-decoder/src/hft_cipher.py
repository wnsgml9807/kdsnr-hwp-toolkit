"""Stream ciphers used by HFT type-0/2 dispatchers.

All variants implement the same linear-congruential generator (multiplier
0xC73E) keyed by a 16-bit running state. The 8-bit XOR key for each byte is
the high byte of the current state. After processing each byte the state
advances as `state = (byte + state) * 0xC73E + constant (mod 0x10000)`.

The two variants differ only in (initial_state, constant):

    FUN_100ad9c0  HGMJ  (Korean, type=2)  state=0xa729, const=0xe696
    FUN_10026b70  HJSMJ (Hanja,  type=0)  state=0xe696, const=0xc863

Both are linear ciphers in GF(2^16); the cipher is its own inverse on the
state side and a simple XOR on each byte, so encrypt and decrypt are the
same operation.
"""
from __future__ import annotations
from dataclasses import dataclass


_MULT = 0xC73E


@dataclass(frozen=True)
class CipherKey:
    state: int
    constant: int


# Verified via Ghidra decompile + Frida raid 18.
CIPHER_HGMJ = CipherKey(state=0xA729, constant=0xE696)
CIPHER_HJSMJ = CipherKey(state=0xE696, constant=0xC863)


def decrypt(data: bytes, key: CipherKey) -> bytes:
    """Decrypt a buffer in place. Encrypt is identical (cipher is self-inverse)."""
    out = bytearray(len(data))
    s = key.state & 0xFFFF
    c = key.constant & 0xFFFF
    for i, b in enumerate(data):
        out[i] = ((s >> 8) & 0xFF) ^ b
        s = ((b + s) * _MULT + c) & 0xFFFF
    return bytes(out)


def encrypt(data: bytes, key: CipherKey) -> bytes:
    """Alias for `decrypt` — same operation."""
    return decrypt(data, key)
