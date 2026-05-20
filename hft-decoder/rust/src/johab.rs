//! Johab decomposition + jamo shape tables.

/// Returns true if a shape class value indicates "invalid" (table entry 255).
pub fn shape_class_invalid(s: u8) -> bool {
    s == 255
}

/// 32-byte table mapping raw cho (0..31) → shape class. From DAT_100c6e20.
pub const CHO_SHAPE_TABLE: [u8; 32] = [
    20, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];

/// 32-byte table mapping raw jung (0..31) → shape class (255 = invalid).
/// From DAT_100c6e40.
pub const JUNG_SHAPE_TABLE: [u8; 32] = [
    255, 255, 0, 1, 2, 3, 4, 5, 255, 255, 6, 7, 8, 9, 10, 11,
    255, 22, 12, 13, 14, 15, 16, 17, 23, 24, 18, 19, 20, 21, 25, 26,
];

/// 32-byte table mapping raw jong (0..31) → shape class. From DAT_100c6e60.
pub const JONG_SHAPE_TABLE: [u8; 32] = [
    28, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    15, 16, 29, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 30, 31,
];

/// Decomposed Johab syllable: 5-bit cho / jung / jong values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Johab {
    pub cho: u8,
    pub jung: u8,
    pub jong: u8,
}

impl Johab {
    /// Decompose a 16-bit syllable code (bit 15 must be set).
    pub fn decompose(code: u16) -> Option<Self> {
        if code & 0x8000 == 0 {
            return None;
        }
        Some(Johab {
            cho: ((code >> 10) & 0x1f) as u8,
            jung: ((code >> 5) & 0x1f) as u8,
            jong: (code & 0x1f) as u8,
        })
    }

    /// Compose back to a 16-bit code.
    pub fn compose(&self) -> u16 {
        0x8000 | ((self.cho as u16) << 10) | ((self.jung as u16) << 5) | (self.jong as u16)
    }

    /// Shape classes after remap. Returns 255 for invalid positions.
    pub fn shape_classes(&self) -> (u8, u8, u8) {
        (
            CHO_SHAPE_TABLE[self.cho as usize],
            JUNG_SHAPE_TABLE[self.jung as usize],
            JONG_SHAPE_TABLE[self.jong as usize],
        )
    }
}

const JUNG_UNI_TO_HWP: [u8; 21] = [
    3, 4, 5, 6, 7,            // ㅏㅐㅑㅒㅓ
    10, 11, 12, 13, 14, 15,    // ㅔㅕㅖㅗㅘㅙ
    18, 19, 20, 21, 22, 23,    // ㅚㅛㅜㅝㅞㅟ
    26, 27, 28, 29,            // ㅠㅡㅢㅣ
];

const JONG_UNI_TO_HWP: [u8; 28] = [
    1,                          // 0: filler (no jong)
    2, 3, 4, 5, 6, 7, 8,         // 1..7: ㄱㄲㄳㄴㄵㄶㄷ
    9, 10, 11, 12, 13, 14, 15, 16, // 8..15: ㄹㄺㄻㄼㄽㄾㄿㅀ
    17,                         // 16: ㅁ
    19, 20, 21, 22, 23,         // 17..21: ㅂㅄㅅㅆㅇ (skip Hwp 18 filler)
    24, 25, 26, 27,             // 22..25: ㅈㅊㅋㅌ
    29, 30,                     // 26..27: ㅍㅎ (skip Hwp 28 filler)
];

/// Convert a Hangul Syllables codepoint (U+AC00..U+D7A3) to Hwp's Johab-like
/// 16-bit char code.
///
/// Verified via Frida raid 16: Hwp's cho field = (Unicode 0-indexed cho) + 2.
pub fn unicode_to_johab(codepoint: u32) -> Option<u16> {
    if !(0xAC00..=0xD7A3).contains(&codepoint) {
        return None;
    }
    let rel = codepoint - 0xAC00;
    let cho_uni = (rel / (21 * 28)) as u8;
    let rest = rel % (21 * 28);
    let jung_uni = (rest / 28) as usize;
    let jong_uni = (rest % 28) as usize;

    let cho_hwp = cho_uni + 2;
    let jung_hwp = JUNG_UNI_TO_HWP[jung_uni];
    let jong_hwp = JONG_UNI_TO_HWP[jong_uni];

    Some(0x8000
        | ((cho_hwp as u16) << 10)
        | ((jung_hwp as u16) << 5)
        | jong_hwp as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_to_johab_known() {
        // Verified Frida raid 16 captures
        assert_eq!(unicode_to_johab(0xAC00), Some(0x8861)); // 가
        assert_eq!(unicode_to_johab(0xB098), Some(0x9061)); // 나
        assert_eq!(unicode_to_johab(0xB2E4), Some(0x9461)); // 다
        assert_eq!(unicode_to_johab(0xC7AD), Some(0xb882)); // 잭
        assert_eq!(unicode_to_johab(0xD55C), Some(0xd065)); // 한
        assert_eq!(unicode_to_johab(0xAC15), Some(0x8877)); // 강
    }

    #[test]
    fn johab_decompose_compose() {
        let j = Johab::decompose(0xb882).unwrap();
        assert_eq!(j.cho, 14);
        assert_eq!(j.jung, 4);
        assert_eq!(j.jong, 2);
        assert_eq!(j.compose(), 0xb882);
    }

    #[test]
    fn shape_classes_for_known() {
        // 잭: cho=14 → CHO_SHAPE[14]=13; jung=4 → JUNG_SHAPE[4]=2; jong=2 → JONG_SHAPE[2]=1
        let j = Johab::decompose(0xb882).unwrap();
        assert_eq!(j.shape_classes(), (13, 2, 1));
    }
}
