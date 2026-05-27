//! Unicode codepoint → HWP per-script slot (index into `CharShape` 7-arrays).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Hangul = 0,
    Latin = 1,
    Hanja = 2,
    Japanese = 3,
    Other = 4,
    Symbol = 5,
    User = 6,
}

impl Script {
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn hftinfo_category(self) -> &'static str {
        match self {
            Script::Hangul => "Hangul",
            Script::Latin => "Latin",
            Script::Hanja => "Hanja",
            Script::Japanese => "Japanese",
            Script::Symbol => "Symbol",
            Script::User => "User",
            Script::Other => "Other",
        }
    }
}

/// ASCII (0x20–0x24F, incl. space/punct) → Latin; enclosed/other marks → Symbol.
pub fn script_of(ch: char) -> Script {
    let c = ch as u32;
    match c {
        0x0020..=0x024F => Script::Latin,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF => Script::Hanja,
        0x3040..=0x30FF | 0x31F0..=0x31FF => Script::Japanese,
        _ => Script::Symbol,
    }
}
