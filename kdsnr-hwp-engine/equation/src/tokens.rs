#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Word(String),
    Number(String),
    Symbol(String),
    LBrace,
    RBrace,
    Caret,
    Under,
    Amp,
    Newline,
    Space(f64),
    Eof,
}

pub(crate) fn tokenize(script: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let chars: Vec<char> = script.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '{' => {
                toks.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                toks.push(Tok::RBrace);
                i += 1;
            }
            '^' => {
                toks.push(Tok::Caret);
                i += 1;
            }
            '_' => {
                toks.push(Tok::Under);
                i += 1;
            }
            '&' => {
                toks.push(Tok::Amp);
                i += 1;
            }
            '#' | '\n' | '\r' => {
                toks.push(Tok::Newline);
                i += 1;
            }
            '`' => {
                toks.push(Tok::Space(0.22));
                i += 1;
            }
            '~' => {
                toks.push(Tok::Space(0.45));
                i += 1;
            }
            c if c.is_whitespace() => {
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                toks.push(Tok::Number(chars[start..i].iter().collect()));
            }
            c if is_word_char(c) => {
                let start = i;
                i += 1;
                while i < chars.len() && is_word_char(chars[i]) {
                    i += 1;
                }
                // HWP's equation lexer greedily peels known keywords out of an
                // alphabetic run, so `rmbarAB` → `rm` `bar` `AB`, `80timesk` → `times`
                // `k`, `sinx` → `sin` `x`. A run with no keyword stays one identifier.
                for piece in split_keyword_run(&chars[start..i]) {
                    toks.push(Tok::Word(piece));
                }
            }
            '<' | '>' | '!' | '=' => {
                if i + 1 < chars.len() && matches!(chars[i + 1], '=' | '<' | '>') {
                    toks.push(Tok::Symbol(chars[i..=i + 1].iter().collect()));
                    i += 2;
                } else {
                    toks.push(Tok::Symbol(ch.to_string()));
                    i += 1;
                }
            }
            '-' if i + 1 < chars.len() && chars[i + 1] == '>' => {
                toks.push(Tok::Symbol("->".to_string()));
                i += 2;
            }
            _ => {
                toks.push(Tok::Symbol(ch.to_string()));
                i += 1;
            }
        }
    }
    toks.push(Tok::Eof);
    toks
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphabetic() || ch == '\''
}

/// HWP equation keywords (uppercase), matched longest-first when peeling a glued
/// alphabetic run. Structural commands, function names, and symbol words — every
/// word `parser::word_atom` recognizes. A run with none of these stays whole.
const KEYWORDS: &[&str] = &[
    // structural / functions
    "SQRT", "LEFT", "RIGHT", "RM", "IT", "BAR", "VEC", "BOX", "CASES", "EQALIGN", "PILE",
    "LPILE", "RPILE", "PRIME", "LIM", "INT", "SUM", "PROD", "SIN", "COS", "TAN", "LOG", "LN",
    "SEC", "OVER", "ATOP",
    // symbol words (command_symbol)
    "PI", "THETA", "ALPHA", "BETA", "GAMMA", "DELTA", "TIMES", "CDOTS", "CDOT", "CAP",
    "SMALLINTER", "SIM", "LEQ", "LE", "GEQ", "NEQ", "RARROW", "LARROW", "INF", "ANGLE",
];

/// The longest keyword that is a prefix of `upper[pos..]` (char-indexed), or None.
fn longest_keyword_at(upper: &[char], pos: usize) -> Option<usize> {
    KEYWORDS
        .iter()
        .filter_map(|kw| {
            let kc: Vec<char> = kw.chars().collect();
            (pos + kc.len() <= upper.len() && upper[pos..pos + kc.len()] == kc[..])
                .then_some(kc.len())
        })
        .max()
}

/// Split an alphabetic run into keyword tokens and leftover identifiers, greedily
/// (HWP's lexer): peel the longest keyword at each position; characters that begin
/// no keyword accumulate into one identifier token.
fn split_keyword_run(run: &[char]) -> Vec<String> {
    let upper: Vec<char> = run.iter().map(|c| c.to_ascii_uppercase()).collect();
    let mut result = Vec::new();
    let mut pending = String::new();
    let mut i = 0;
    while i < run.len() {
        if let Some(klen) = longest_keyword_at(&upper, i) {
            if !pending.is_empty() {
                result.push(std::mem::take(&mut pending));
            }
            result.push(run[i..i + klen].iter().collect());
            i += klen;
        } else {
            pending.push(run[i]);
            i += 1;
        }
    }
    if !pending.is_empty() {
        result.push(pending);
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}
