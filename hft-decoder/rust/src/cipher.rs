//! Stream ciphers used by HFT dispatchers.
//!
//! Both known variants share the same linear-congruential generator
//! (multiplier `0xC73E`) keyed by a 16-bit running state. The 8-bit XOR key
//! for each byte is the high byte of the current state. After each byte the
//! state advances as `state = (byte + state) * 0xC73E + constant (mod 2^16)`.
//!
//! Verified via Ghidra decompile + Frida raid 18:
//!
//! - `FUN_100ad9c0` HGMJ (Korean, type 2): state `0xa729`, const `0xe696`
//! - `FUN_10026b70` HJSMJ (Hanja, type 0): state `0xe696`, const `0xc863`
//! - `FUN_100248e0` type 1: different algorithm (global state)

const MULT: u32 = 0xC73E;

#[derive(Debug, Clone, Copy)]
pub struct CipherKey {
    pub state: u16,
    pub constant: u16,
}

/// Korean Gungmyeongjo cipher (HGMJ.HFT, type 2 dispatcher).
pub const HGMJ: CipherKey = CipherKey { state: 0xA729, constant: 0xE696 };

/// Hanja & English Shinmyeongjo cipher (HJSMJ.HFT, ENSMJ.HFT, type 0).
pub const HJSMJ: CipherKey = CipherKey { state: 0xE696, constant: 0xC863 };

/// Return the standard cipher for a dispatcher type, if any. Type 1 uses a
/// different stream cipher whose state is global; HCHGGGT samples observed
/// to date ship raw / unencrypted data.
pub fn for_type(type_id: u8) -> Option<CipherKey> {
    match type_id {
        0 => Some(HJSMJ),
        2 => Some(HGMJ),
        _ => None,
    }
}

/// In-place decrypt of a buffer. The cipher is self-inverse on the per-byte
/// XOR; calling this twice with the same key restores the original.
pub fn decrypt(data: &[u8], key: CipherKey) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut s: u32 = key.state as u32;
    let c: u32 = key.constant as u32;
    for &b in data {
        out.push(((s >> 8) as u8) ^ b);
        s = ((b as u32 + s).wrapping_mul(MULT).wrapping_add(c)) & 0xFFFF;
    }
    out
}

/// Alias for `decrypt` (the cipher is symmetric on byte data).
pub fn encrypt(data: &[u8], key: CipherKey) -> Vec<u8> {
    decrypt(data, key)
}
