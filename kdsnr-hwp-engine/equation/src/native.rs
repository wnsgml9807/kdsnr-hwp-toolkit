use super::metrics::font_width_ref;
use super::types::EqStyle;

pub(crate) fn text_width(text: &str, fs: f64, style: EqStyle) -> f64 {
    remap_text(text, style)
        .chars()
        .map(|ch| native_equation_dx(ch, fs))
        .sum()
}

/// Hancom's per-codepoint advance: `GetTextExtentExPointW` × 9/10
/// (`FUN_0003a934`). The extent is the HYhwpEQ glyph advance, baked from the
/// bundled font into `hyhwpeq_advance` — the faithful source, not a class guess.
/// Space/backtick follow `FUN_0002bb88`'s special branch: width = fc4/4.
pub(crate) fn native_equation_dx(ch: char, fs: f64) -> f64 {
    // The char layout advance is `GetTextExtentExPointW × 9/10` (FUN_0003a934),
    // but GetTextExtent returns ~0.952× the font hmtx advance our table is baked
    // from, so the effective factor on the table value is 9/10 × 0.952 = 6/7.
    // Ground truth: 126 pure-digit corpus equations give stored 472/digit at
    // baseUnit 1100 = 0.5·(6/7)·1100. (The ×9/10 dx passed to ExtTextOut is the
    // wider GDI text cell, not the pen advance.)
    const SIX_SEVENTHS: f64 = 6.0 / 7.0;
    match ch {
        // EqCharNode::measure (FUN_0002bb88): space (0x20) and backtick (0x60) take
        // the fc4/4 branch (`(code|0x40)==0x60`), but tilde (0x7e) takes fc4 — it is
        // a wide, invisible space. fc4 = fc0/2. The engine emits U+2009 for a
        // backtick and U+2002 (en space) for a tilde; both render blank.
        ' ' | '`' | '\u{2009}' | '\u{0009}' => font_width_ref(fs) / 4.0,
        '\u{2002}' => font_width_ref(fs),
        _ => match hyhwpeq_advance_em(ch) {
            Some(em) => em * SIX_SEVENTHS * fs,
            // Not in HYhwpEQ (its own node handles it, or a rare symbol): keep the
            // class-ratio estimate so width never collapses to zero.
            None => char_width_ratio(ch) * fs,
        },
    }
}

/// Symbol-path advance: the GetTextExtentPoint32 result with NO ×9/10. Big
/// operators and the radical sign measure via `FUN_0003ac9c` (a GetTextExtent
/// call); the ×9/10 belongs to the char text path (`FUN_0003a934`) only. But
/// GetTextExtent returns ~20/21 of the font hmtx advance our table is baked from
/// (the same factor the char path carries inside its 6/7), so it applies here
/// too. `ch` is already a HYhwpEQ codepoint.
pub(crate) fn native_symbol_advance(ch: char, fs: f64) -> f64 {
    const TWENTY_TWENTYFIRSTS: f64 = 20.0 / 21.0;
    match hyhwpeq_advance_em(ch) {
        Some(em) => em * TWENTY_TWENTYFIRSTS * fs,
        None => char_width_ratio(ch) * fs,
    }
}

/// HYhwpEQ glyph advance in em units, from the baked `GetTextExtentExPointW`
/// table (None when the font has no glyph for `ch`).
pub(crate) fn hyhwpeq_advance_em(ch: char) -> Option<f64> {
    let cp = ch as u32;
    crate::hyhwpeq_advance::HYHWPEQ_ADVANCE
        .binary_search_by_key(&cp, |e| e.0)
        .ok()
        .map(|i| crate::hyhwpeq_advance::HYHWPEQ_ADVANCE[i].1 as f64 / 10000.0)
}

fn char_width_ratio(ch: char) -> f64 {
    match ch {
        '\u{2009}' => 0.22,
        ' ' => 0.28,
        '0'..='9' => 0.54,
        'a'..='z' => 0.53,
        'A'..='Z' => 0.64,
        '+' | '-' | '=' | '<' | '>' | '≤' | '≥' | '≠' | '∼' | '×' => 0.68,
        '(' | ')' | '[' | ']' | '{' | '}' => 0.34,
        ',' | '.' | '\'' | '′' => 0.24,
        '∫' | '∑' => 0.76,
        '∏' => 0.80,
        'π' | 'θ' | 'α' | 'β' | 'γ' | 'Δ' => 0.58,
        c if c.is_ascii_punctuation() => 0.42,
        c if is_cjk(c) => 0.95,
        _ => 0.60,
    }
}

pub(crate) fn remap_text(text: &str, style: EqStyle) -> String {
    text.chars()
        .map(|ch| {
            let mapped = match ch {
                'π' => '\u{E0AC}',
                'θ' => '\u{E0B0}',
                'α' => '\u{E09D}',
                'β' => '\u{E09E}',
                'γ' => '\u{E09F}',
                'Δ' => '\u{E088}',
                // Big operators live in HYhwpEQ's PUA, not at their Unicode points
                // (which the font lacks → invisible). Codepoints from the HYhwpEQ
                // glyph grid: ∫ E05B, ∑ E067, ∏ E068.
                '∫' => '\u{E05B}',
                '∑' => '\u{E067}',
                '∏' => '\u{E068}',
                _ => ch,
            };
            if let Some(native) = native_hyhwpeq_code(mapped) {
                native
            } else if style == EqStyle::MathItalic && mapped.is_ascii_lowercase() {
                char::from_u32(0xE0E5 + (mapped as u32 - 'a' as u32)).unwrap_or(mapped)
            } else {
                mapped
            }
        })
        .collect()
}

fn native_hyhwpeq_code(ch: char) -> Option<char> {
    // Windows HncBaseDraw trace `equation_surface_trace_m10.jsonl` shows
    // HWP equations lowering common ASCII tokens into HYhwpEQ private-use
    // codepoints before FUN_1005d710 dispatches one UTF-16 unit to ExtTextOutW.
    // Capitals are the exception: the COM-PDF export draws `A`..`Z` from HYhwpEQ's
    // own regular-ASCII glyphs (U+0041…), whose advance differs from the wider PUA
    // E000 block — so they keep their ASCII codepoint and use that table entry.
    let code = match ch {
        '1'..='9' => 0xE034 + (ch as u32 - '1' as u32),
        '0' => 0xE03D,
        '(' => 0xE044,
        ')' => 0xE045,
        '-' => 0xE046,
        '=' => 0xE047,
        '+' => 0xE048,
        '{' => 0xE04B,
        '}' => 0xE04C,
        ',' => 0xE052,
        '.' => 0xE053,
        '<' => 0xE055,
        '>' => 0xE056,
        _ => return None,
    };
    char::from_u32(code)
}

pub(crate) fn symbol_text(symbol: &str) -> String {
    match symbol {
        "!=" => "≠".to_string(),
        "<=" => "≤".to_string(),
        ">=" => "≥".to_string(),
        "->" => "→".to_string(),
        _ => symbol.to_string(),
    }
}

pub(crate) fn command_symbol(command: &str) -> Option<&'static str> {
    match command {
        "PI" => Some("π"),
        "THETA" => Some("θ"),
        "ALPHA" => Some("α"),
        "BETA" => Some("β"),
        "GAMMA" => Some("γ"),
        "DELTA" => Some("Δ"),
        "TIMES" => Some("×"),
        "CDOT" => Some("·"),
        "CDOTS" => Some("⋯"),
        "CAP" => Some("∩"),
        "SMALLINTER" => Some("∩"),
        "SIM" => Some("∼"),
        "LE" => Some("≤"),
        "LEQ" => Some("≤"),
        "GEQ" => Some("≥"),
        "NEQ" => Some("≠"),
        "RARROW" => Some("→"),
        "LARROW" => Some("←"),
        "INF" => Some("∞"),
        "ANGLE" => Some("∠"),
        "INT" => Some("∫"),
        "SUM" => Some("∑"),
        "PROD" => Some("∏"),
        _ => None,
    }
}

pub(crate) fn normalize_delim(delim: &str) -> String {
    match delim {
        "{" => "{".to_string(),
        "}" => "}".to_string(),
        "" => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn space_text(em: f64) -> String {
    // Backtick/thin gap → U+2009 (fc4/4); tilde → U+2002 en space (fc4). Both are
    // whitespace, so the renderer draws no glyph; only the advance differs.
    if em < 0.3 {
        "\u{2009}".to_string()
    } else {
        "\u{2002}".to_string()
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{f900}'..='\u{faff}'
    )
}
