//! `Hnc::Shape::Color::SchemeStyle` — u32 enum, 12 valid variants (0..11).
//!
//! raw 출처: `ColorScheme::ColorScheme()` (`0x14fd1c`) 가 12 SetAt 호출 시
//! key `0x0..0xb` (= 0..11) 을 사용 — `mov w1, #0x0`, `mov w1, #0x1`, ...
//! `mov w1, #0xb` 까지.
//!
//! libc++ `std::map<SchemeStyle, Color>` 의 key 타입. `__tree::__find_equal` 의
//! `cmp w8, w9` 비교가 `int` 비교이므로 raw 와 byte-equivalent.
//!
//! # variant 의미 (Office Open XML / Hancom theme schema 와 동등)
//!
//! 0..11 의 의미는 ColorScheme.SetAt 의 12 호출 순서에서 도출.
//! 본 매핑은 raw 의 hardcoded 색상 + ColorScheme.SetAt(0=SystemStyle Window,
//! 1=SystemStyle WindowText, 2..11=Rgb hardcoded) 에서 가장 자연스러운 의미:
//!
//! - 0: Background1 (default = Window system color)
//! - 1: Text1 (default = WindowText system color)
//! - 2: Background2 (default = #843c3a hardcoded)
//! - 3: Text2 (default = #dbf3fa)
//! - 4: Accent1 (default = #d68261)
//! - 5: Accent2 (default = #3a84ff)
//! - 6: Accent3 (default = #b2b2b2)
//! - 7: Accent4 (default = #00d7ff)
//! - 8: Accent5 (default = #6e9b28)
//! - 9: Accent6 (default = #bb5c9d)
//! - 10: Hyperlink (default = #ff0080)
//! - 11: FollowedHyperlink (default = #808080)
//!
//! 의미는 본 단계의 byte-equivalence 에 영향 없음 (key 가 u32 0..11 이기만 하면
//! 동등).

/// `Hnc::Shape::Color::SchemeStyle` — u32 enum, 12 valid variants.
///
/// `#[repr(u32)]` 으로 raw 의 `int` 와 byte-equivalent.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SchemeStyle {
    Background1 = 0,
    Text1 = 1,
    Background2 = 2,
    Text2 = 3,
    Accent1 = 4,
    Accent2 = 5,
    Accent3 = 6,
    Accent4 = 7,
    Accent5 = 8,
    Accent6 = 9,
    Hyperlink = 10,
    FollowedHyperlink = 11,
}

impl SchemeStyle {
    /// raw u32 값 (4B, byte-equivalent).
    #[inline]
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// u32 → SchemeStyle (0..11 만 valid). Invalid 값은 None.
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(SchemeStyle::Background1),
            1 => Some(SchemeStyle::Text1),
            2 => Some(SchemeStyle::Background2),
            3 => Some(SchemeStyle::Text2),
            4 => Some(SchemeStyle::Accent1),
            5 => Some(SchemeStyle::Accent2),
            6 => Some(SchemeStyle::Accent3),
            7 => Some(SchemeStyle::Accent4),
            8 => Some(SchemeStyle::Accent5),
            9 => Some(SchemeStyle::Accent6),
            10 => Some(SchemeStyle::Hyperlink),
            11 => Some(SchemeStyle::FollowedHyperlink),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_size_align() {
        assert_eq!(std::mem::size_of::<SchemeStyle>(), 4);
        assert_eq!(std::mem::align_of::<SchemeStyle>(), 4);
    }

    #[test]
    fn discriminants_match_raw_set_at_keys() {
        // raw ColorScheme::ColorScheme() 의 SetAt 호출 순서대로
        assert_eq!(SchemeStyle::Background1 as u32, 0);
        assert_eq!(SchemeStyle::Text1 as u32, 1);
        assert_eq!(SchemeStyle::Background2 as u32, 2);
        assert_eq!(SchemeStyle::Text2 as u32, 3);
        assert_eq!(SchemeStyle::Accent1 as u32, 4);
        assert_eq!(SchemeStyle::Accent2 as u32, 5);
        assert_eq!(SchemeStyle::Accent3 as u32, 6);
        assert_eq!(SchemeStyle::Accent4 as u32, 7);
        assert_eq!(SchemeStyle::Accent5 as u32, 8);
        assert_eq!(SchemeStyle::Accent6 as u32, 9);
        assert_eq!(SchemeStyle::Hyperlink as u32, 10);
        assert_eq!(SchemeStyle::FollowedHyperlink as u32, 11);
    }

    #[test]
    fn from_u32_round_trip() {
        for i in 0u32..12 {
            let s = SchemeStyle::from_u32(i).unwrap();
            assert_eq!(s.as_u32(), i);
        }
    }

    #[test]
    fn from_u32_out_of_range_is_none() {
        assert!(SchemeStyle::from_u32(12).is_none());
        assert!(SchemeStyle::from_u32(13).is_none());
        assert!(SchemeStyle::from_u32(0xFFFF_FFFF).is_none());
    }

    #[test]
    fn ord_matches_raw_int_compare() {
        // raw 의 `cmp w8, w9` (signed compare) 와 일치
        assert!(SchemeStyle::Background1 < SchemeStyle::Text1);
        assert!(SchemeStyle::Text1 < SchemeStyle::Background2);
        assert!(SchemeStyle::Hyperlink < SchemeStyle::FollowedHyperlink);
    }
}
