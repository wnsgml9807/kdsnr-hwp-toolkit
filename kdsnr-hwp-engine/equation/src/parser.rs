use super::native::{command_symbol, space_text, symbol_text};
use super::tokens::{tokenize, Tok};
use super::types::{DecorKind, EqNode, EqStyle, PileAlign};

pub(crate) fn parse_equation(script: &str) -> EqNode {
    Parser::new(tokenize(script)).parse()
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Self { toks, pos: 0 }
    }

    fn parse(&mut self) -> EqNode {
        self.parse_row(&[Tok::Eof]).simplify()
    }

    fn parse_row(&mut self, stops: &[Tok]) -> EqNode {
        let mut items = Vec::new();
        while !self.at_any(stops) {
            match self.peek() {
                Tok::RBrace | Tok::Eof => break,
                Tok::Space(em) => {
                    let em = *em;
                    self.bump();
                    items.push(EqNode::Text(space_text(em)));
                }
                Tok::Amp => {
                    // Alignment tab (FUN_0002ab40 type-0x64 glue). Its width is the
                    // column pad (cross-row), unknowable from a single script — so it
                    // lays out as a thin space here and the renderer expands it to the
                    // stored common.width (which Hancom baked the alignment into). The
                    // U+0009 sentinel marks it for the renderer; it is never drawn.
                    self.bump();
                    items.push(EqNode::Text("\u{0009}".to_string()));
                }
                Tok::Newline => {
                    self.bump();
                    items.push(EqNode::Text(" ".to_string()));
                }
                _ => {
                    let next = self.parse_relation();
                    items.push(next);
                }
            }
        }
        EqNode::Row(items).simplify()
    }

    fn parse_relation(&mut self) -> EqNode {
        let mut node = self.parse_postfix();
        loop {
            if self.consume_word("over") {
                let rhs = self.parse_postfix();
                node = EqNode::Fraction(Box::new(node), Box::new(rhs));
            } else if self.consume_word("atop") {
                let rhs = self.parse_postfix();
                node = EqNode::Atop(Box::new(node), Box::new(rhs));
            } else {
                break;
            }
        }
        node
    }

    fn parse_postfix(&mut self) -> EqNode {
        // A leading `^`/`_` with no base is the Hancom strut idiom (`{ ^{` ^{`}}}`):
        // an empty-base script, not a literal caret. Bind it to an empty box so the
        // nested empty scripts collapse to invisible struts instead of `^` glyphs.
        let base = match self.peek() {
            Tok::Caret | Tok::Under => EqNode::Text(String::new()),
            _ => self.parse_atom(),
        };
        let mut sub: Option<EqNode> = None;
        let mut sup: Option<EqNode> = None;
        let mut spaces: Vec<EqNode> = Vec::new();
        loop {
            // A subscript/superscript binds to the base across an intervening
            // syntactic gap, but a backtick/tilde gap is a literal thin space the
            // user typed (`M`` ^2`): Hancom advances the pen for it (no glyph) and
            // bakes it into the box width, so collect the skipped spaces as siblings
            // rather than dropping them. The width is the same wherever they sit.
            let j = self.next_nonspace();
            match self.toks.get(j) {
                Some(Tok::Under) => {
                    self.collect_spaces(self.pos, j, &mut spaces);
                    self.pos = j + 1;
                    sub = Some(self.parse_script_arg());
                }
                Some(Tok::Caret) => {
                    self.collect_spaces(self.pos, j, &mut spaces);
                    self.pos = j + 1;
                    sup = Some(self.parse_script_arg());
                }
                // No script marker: leave the spaces in the stream for the row.
                _ => break,
            }
        }
        let node = match base {
            EqNode::Integral { .. } => EqNode::Integral {
                sub: sub.map(Box::new),
                sup: sup.map(Box::new),
            },
            EqNode::UnderOver { symbol, .. } => EqNode::UnderOver {
                symbol,
                sub: sub.map(Box::new),
                sup: sup.map(Box::new),
            },
            base => match (sub, sup) {
                (Some(a), Some(b)) => EqNode::SubSup(Box::new(base), Box::new(a), Box::new(b)),
                (Some(a), None) => EqNode::Sub(Box::new(base), Box::new(a)),
                (None, Some(b)) => EqNode::Sup(Box::new(base), Box::new(b)),
                (None, None) => base,
            },
        };
        if spaces.is_empty() {
            node
        } else {
            let mut items = vec![node];
            items.append(&mut spaces);
            EqNode::Row(items).simplify()
        }
    }

    /// Push a thin/normal-space node for each `Tok::Space` in `toks[from..to]`,
    /// preserving backtick/tilde advances the script scan would otherwise drop.
    fn collect_spaces(&self, from: usize, to: usize, out: &mut Vec<EqNode>) {
        for tok in &self.toks[from..to] {
            if let Tok::Space(em) = tok {
                out.push(EqNode::Text(space_text(*em)));
            }
        }
    }

    fn parse_script_arg(&mut self) -> EqNode {
        self.skip_script_padding();
        // No trailing skip: spaces after the script argument (e.g. the backtick
        // after `_{6}` in `S _{`6} ``) belong to the enclosing row and must render.
        self.parse_atom()
    }

    fn skip_script_padding(&mut self) {
        while matches!(self.peek(), Tok::Space(_) | Tok::Newline | Tok::Amp) {
            self.bump();
        }
    }

    fn parse_atom(&mut self) -> EqNode {
        // Structural terminators end the enclosing row/pile/brace; an atom expected
        // here (e.g. the empty body of a bare `it`/`rm` before a `#` or `}`) is empty.
        // Do NOT consume them — `IT`/`RM`/`SQRT` reach parse_atom with no brace, and
        // swallowing a `#`/`}`/`&` would merge sibling pile rows or eat a closing brace.
        if matches!(self.peek(), Tok::RBrace | Tok::Eof | Tok::Amp | Tok::Newline) {
            return EqNode::Text(String::new());
        }
        match self.bump() {
            Tok::LBrace => {
                let node = self.parse_row(&[Tok::RBrace]);
                self.consume(&Tok::RBrace);
                node
            }
            Tok::Number(s) => EqNode::Text(s),
            Tok::Symbol(s) => EqNode::Text(symbol_text(&s)),
            Tok::Word(w) => self.word_atom(w),
            Tok::Caret => EqNode::Text("^".to_string()),
            Tok::Under => EqNode::Text("_".to_string()),
            Tok::Space(em) => EqNode::Text(space_text(em)),
            Tok::RBrace | Tok::Eof | Tok::Amp | Tok::Newline => EqNode::Text(String::new()),
        }
    }

    fn word_atom(&mut self, word: String) -> EqNode {
        let upper = word.to_ascii_uppercase();
        match upper.as_str() {
            "SQRT" => EqNode::Sqrt(Box::new(self.parse_script_arg())),
            "LEFT" => {
                let left = self.next_delim();
                let body = self.parse_row(&[Tok::Word("RIGHT".to_string()), Tok::Eof]);
                self.consume_word("RIGHT");
                let right = self.next_delim();
                EqNode::Paren(left, right, Box::new(body))
            }
            "RIGHT" => {
                let _ = self.next_delim();
                EqNode::Text(String::new())
            }
            "RM" => {
                if matches!(self.peek(), Tok::LBrace) {
                    EqNode::Style(EqStyle::Roman, Box::new(self.parse_atom()))
                } else {
                    EqNode::Style(EqStyle::Roman, Box::new(self.parse_postfix()))
                }
            }
            "IT" => EqNode::Style(EqStyle::MathItalic, Box::new(self.parse_postfix())),
            "BAR" => EqNode::Decoration(DecorKind::Bar, Box::new(self.parse_script_arg())),
            "VEC" => EqNode::Decoration(DecorKind::Vec, Box::new(self.parse_script_arg())),
            "BOX" => EqNode::BoxFrame(Box::new(self.parse_script_arg())),
            // `cases` is a pile fenced by a tall left brace (no right delimiter); its
            // rows left-align under the brace.
            "CASES" => EqNode::Paren(
                "{".to_string(),
                String::new(),
                Box::new(self.parse_pile_arg(PileAlign::Left)),
            ),
            // FUN_000255e8: LPILE/EQALIGN left, RPILE right, PILE centred.
            "EQALIGN" | "LPILE" => self.parse_pile_arg(PileAlign::Left),
            "RPILE" => self.parse_pile_arg(PileAlign::Right),
            "PILE" => self.parse_pile_arg(PileAlign::Center),
            // The prime mark renders from HYhwpEQ's apostrophe glyph (U+0027); the
            // typographic ′ (U+2032) has no glyph in the font and drops out.
            "PRIME" => EqNode::Text("'".to_string()),
            "LIM" => self.parse_limit_atom(&word),
            "INT" => EqNode::Integral {
                sub: None,
                sup: None,
            },
            "SUM" | "PROD" => EqNode::UnderOver {
                symbol: command_symbol(&upper).unwrap_or("∑").to_string(),
                sub: None,
                sup: None,
            },
            "SIN" | "COS" | "TAN" | "LOG" | "LN" | "SEC" => EqNode::Style(
                EqStyle::Roman,
                Box::new(EqNode::Text(upper.to_ascii_lowercase())),
            ),
            _ => {
                if let Some(symbol) = command_symbol(&upper) {
                    EqNode::Text(symbol.to_string())
                } else if upper.starts_with("RM") && word.len() > 2 {
                    EqNode::Style(
                        EqStyle::Roman,
                        Box::new(EqNode::Text(word[2..].to_string())),
                    )
                } else if upper.starts_with("IT") && word.len() > 2 {
                    EqNode::Style(
                        EqStyle::MathItalic,
                        Box::new(EqNode::Text(word[2..].to_string())),
                    )
                } else {
                    EqNode::Text(word)
                }
            }
        }
    }

    fn next_delim(&mut self) -> String {
        let delim = match self.bump() {
            Tok::LBrace => "{".to_string(),
            Tok::RBrace => "}".to_string(),
            Tok::Symbol(s) | Tok::Word(s) | Tok::Number(s) => symbol_text(&s),
            _ => String::new(),
        };
        // `LEFT .` / `RIGHT .` is the null (invisible) delimiter.
        if delim == "." {
            String::new()
        } else {
            delim
        }
    }

    fn parse_pile_arg(&mut self, align: PileAlign) -> EqNode {
        if !matches!(self.peek(), Tok::LBrace) {
            return self.parse_script_arg();
        }
        self.bump();
        let mut rows = Vec::new();
        while !matches!(self.peek(), Tok::RBrace | Tok::Eof) {
            // Each row is one or more `&`-separated columns; `#`/RBrace end the row.
            let mut cols = Vec::new();
            loop {
                cols.push(self.parse_row(&[Tok::Newline, Tok::Amp, Tok::RBrace]));
                if matches!(self.peek(), Tok::Amp) {
                    self.bump();
                } else {
                    break;
                }
            }
            rows.push(cols);
            while matches!(self.peek(), Tok::Newline) {
                self.bump();
            }
        }
        self.consume(&Tok::RBrace);
        EqNode::Pile(rows, align).simplify()
    }

    fn parse_limit_atom(&mut self, word: &str) -> EqNode {
        let sub = if matches!(self.peek(), Tok::Under) {
            self.bump();
            Some(Box::new(self.parse_script_arg()))
        } else {
            None
        };
        EqNode::Limit {
            capitalized: word != "lim",
            sub,
        }
    }

    fn peek(&self) -> &Tok {
        self.toks.get(self.pos).unwrap_or(&Tok::Eof)
    }

    fn bump(&mut self) -> Tok {
        let tok = self.peek().clone();
        self.pos += 1;
        tok
    }

    fn consume(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.bump();
            true
        } else {
            false
        }
    }

    fn consume_word(&mut self, word: &str) -> bool {
        match self.peek() {
            Tok::Word(w) if w.eq_ignore_ascii_case(word) => {
                self.bump();
                true
            }
            _ => false,
        }
    }

    /// Index of the next token from `pos` that is not a thin/normal space.
    fn next_nonspace(&self) -> usize {
        let mut i = self.pos;
        while matches!(self.toks.get(i), Some(Tok::Space(_))) {
            i += 1;
        }
        i
    }

    fn at_any(&self, stops: &[Tok]) -> bool {
        stops.iter().any(|stop| match (stop, self.peek()) {
            (Tok::Word(a), Tok::Word(b)) => a.eq_ignore_ascii_case(b),
            _ => stop == self.peek(),
        })
    }
}

impl EqNode {
    pub(crate) fn simplify(self) -> Self {
        match self {
            EqNode::Row(items) => {
                let mut flat = Vec::new();
                for item in items {
                    match item.simplify() {
                        EqNode::Row(children) => flat.extend(children),
                        EqNode::Text(s) if s.is_empty() => {}
                        other => flat.push(other),
                    }
                }
                if flat.len() == 1 {
                    flat.remove(0)
                } else {
                    EqNode::Row(flat)
                }
            }
            EqNode::Pile(rows, align) => EqNode::Pile(
                rows.into_iter()
                    .map(|cols| cols.into_iter().map(EqNode::simplify).collect())
                    .collect(),
                align,
            ),
            EqNode::Integral { sub, sup } => EqNode::Integral {
                sub: sub.map(|node| Box::new(node.simplify())),
                sup: sup.map(|node| Box::new(node.simplify())),
            },
            EqNode::UnderOver { symbol, sub, sup } => EqNode::UnderOver {
                symbol,
                sub: sub.map(|node| Box::new(node.simplify())),
                sup: sup.map(|node| Box::new(node.simplify())),
            },
            other => other,
        }
    }
}
