//! Inter-atom spacing (HncEqEdit FUN_0002bb88 / FUN_000390ec / FUN_0003903c).
//! Each char adds, before itself, `SPACING[class(self)][class(left)] × 7 × fc0/100`
//! where the math class comes from a jump table (ASCII) or a symbol table.

use super::types::EqNode;

/// Math classes (FUN_0003903c): 1 Ord, 2 Op, 3 Bin, 4 Rel, 5 Open, 6 Close,
/// 7 Punct, 8 Inner. Non-char nodes default to 1 (Ord), as the engine does.
pub(crate) fn math_class(ch: char) -> usize {
    // ASCII 0x28..=0x7e jump table (decoded from the binary at 0x44960):
    match ch {
        ':' | '<' | '=' | '>' => 4,             // Rel
        '+' | '-' => 3,                          // Bin
        '(' | '[' | '{' => 5,                    // Open
        ')' | ']' | '}' => 6,                    // Close
        '*' | ',' => 7,                          // Punct
        // Common non-ASCII operators (symbol-table path FUN_0003903c@0x39040):
        '×' | '÷' | '±' | '∓' | '·' | '∗' | '∩' | '∪' | '∧' | '∨' | '⊕' | '⊗' | '∘' => 3, // Bin
        '≠' | '≤' | '≥' | '≈' | '≅' | '≡' | '∼' | '∝' | '→' | '←' | '↔' | '⇒' | '⇔'
        | '∈' | '∉' | '⊂' | '⊃' | '⊆' | '⊇' | '⊥' | '∥' | '≪' | '≫' | '↦' | '≒' => 4, // Rel
        '∫' | '∑' | '∏' => 2,                    // Op
        _ => 1,                                  // Ord (letters, digits, ., /, |, Greek, …)
    }
}

/// SPACING[left_class][this_class] (binary @0x449b8). Value × 7 × fc0/100 is the
/// space placed before an atom, indexed by the LEFT neighbor's class (row) then
/// this atom's class (col). Confirmed by Frida pen-position measurement: in
/// "f(x)" Hancom inserts the gap before "(" (SPACING[Ord][Open]=1), not before
/// "x" — the table is indexed [left][this], not [this][left].
const SPACING: [[i32; 10]; 9] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 2, 3, 1, 1, 0, 1, 0],
    [0, 1, 3, 2, 3, 0, 0, 0, 1, 0],
    [0, 2, 0, 0, 0, 2, 0, 0, 2, 0],
    [0, 3, 3, 0, 0, 3, 0, 0, 3, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 2, 3, 0, 0, 0, 1, 0],
    [0, 1, 1, 0, 1, 1, 1, 1, 1, 0],
    [0, 1, 1, 2, 3, 1, 0, 1, 1, 0],
];

/// Space (HWPUNIT) before an atom of class `this` whose left neighbor is class
/// `left`: `SPACING[left][this] × 7 × fc0/100`, with fc0 = fs.
pub(crate) fn inter_atom_space(left: usize, this: usize, fs: f64) -> f64 {
    let v = SPACING.get(left).and_then(|r| r.get(this)).copied().unwrap_or(0);
    fs * (v * 7) as f64 / 100.0
}

/// The (left-edge, right-edge) math class of a row atom. Char-bearing atoms use
/// their boundary chars; every other node is Ord (1), matching the engine's
/// non-char default.
pub(crate) fn node_classes(node: &EqNode) -> (usize, usize) {
    match node {
        EqNode::Text(s) => {
            let mut chars = s.chars().filter(|c| !c.is_whitespace());
            match chars.next() {
                Some(first) => {
                    let last = s.chars().rev().find(|c| !c.is_whitespace()).unwrap_or(first);
                    (math_class(first), math_class(last))
                }
                None => (1, 1),
            }
        }
        EqNode::Style(_, body) => node_classes(body),
        EqNode::Integral { .. } | EqNode::UnderOver { .. } => (2, 2),
        _ => (1, 1),
    }
}
