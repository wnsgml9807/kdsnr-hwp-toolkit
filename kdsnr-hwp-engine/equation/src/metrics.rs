use super::native::{native_symbol_advance, remap_text, text_width};
use super::types::{EqStyle, LayoutBox, LayoutKind};

pub(crate) const SOURCE_CHAR_MEASURE: &str = "HncEqEdit FUN_0002bb88";
pub(crate) const SOURCE_OVER_MEASURE: &str = "HncEqEdit FUN_0002fd28";
pub(crate) const SOURCE_ATOP_MEASURE: &str = "HncEqEdit FUN_00030aac";
pub(crate) const SOURCE_ATOP_PAINT: &str = "HncEqEdit FUN_00030880";
pub(crate) const SOURCE_SUB_MEASURE: &str = "HncEqEdit FUN_000320b8";
pub(crate) const SOURCE_LIM_MEASURE: &str = "HncEqEdit FUN_000333d8/FUN_000334dc";
pub(crate) const SOURCE_LIM_PAINT: &str = "HncEqEdit FUN_000330f8";
pub(crate) const SOURCE_INTEGRAL_MEASURE: &str = "HncEqEdit FUN_0002dba8";
pub(crate) const SOURCE_INTEGRAL_PAINT: &str = "HncEqEdit FUN_0002d95c";
pub(crate) const SOURCE_UNDER_OVER_MEASURE: &str = "HncEqEdit FUN_00036708";
pub(crate) const SOURCE_UNDER_OVER_PAINT: &str = "HncEqEdit FUN_000365ac";
pub(crate) const SOURCE_ROOT_MEASURE: &str = "HncEqEdit FUN_00034324";
pub(crate) const SOURCE_BAR_PAINT: &str = "HncEqEdit FUN_0000cbc0/FUN_0000ccc4";
pub(crate) const SOURCE_VEC_PAINT: &str = "HncEqEdit FUN_0000af58/FUN_0000b05c";
pub(crate) const SOURCE_BOX_MEASURE: &str = "HncEqEdit FUN_0000e8f4";
pub(crate) const SOURCE_BOX_PAINT: &str = "HncEqEdit FUN_0000e70c/FUN_0003a76c";
pub(crate) const SOURCE_PAIR_BRACE_MEASURE: &str = "HncEqEdit FUN_00007fb0/FUN_0001bbd0";
pub(crate) const SOURCE_PILE_MEASURE: &str = "HncEqEdit FUN_00025804";
pub(crate) const SOURCE_PILE_PAINT: &str = "HncEqEdit FUN_000255e8";

pub(crate) const SCRIPT_SCALE: f64 = 0.68;
pub(crate) const ROOT_INDEX_SCALE: f64 = 0.50;
/// Glyph DRAW baseline as a fraction of the box height (tmAscent / tmHeight). The
/// layout baseline (`LayoutBox::baseline`) is the math axis (box center, fc0/2 —
/// FUN_0002bb88 sets `+0x58 = -(height/2)`), used for sub/sup/fraction centering.
/// But GDI draws each glyph's baseline at tmAscent below the cell top
/// (FUN_0002b930: cell top = axis − height/2; glyph baseline = top + tmAscent), so
/// the ink sits in the lower part of the box, not on the axis. Ground truth:
/// single-char corpus equations store `baseLine=85` = tmAscent/tmHeight.
pub(crate) const TEXT_BASELINE_RATIO: f64 = 0.85;
pub(crate) const DECOR_RULE_GAP_SCALE: i32 = 20;
pub(crate) const BOX_BASELINE_SHIFT_SCALE: i32 = 15;
pub(crate) const BIG_OP_SCRIPT_GAP_SCALE: i32 = 8;
/// Big-operator symbol font size = fs × (node+0xcc)/100. The integral-node
/// constructor sets 0xcc = 200 (`mov w22,#0xc8` at 0x2c990), glyph E05B at +0xc8.
pub(crate) const INTEGRAL_SYMBOL_SCALE: f64 = 2.00;
/// Under/over big operators (∑ ∏) size at MulDiv(fs,0xb4,100)=1.80fs and overlap
/// their limits by a 0x4b=75% symbol-height factor (FUN_00036708).
pub(crate) const UNDER_OVER_SYMBOL_SCALE: f64 = 1.80;
pub(crate) const UNDER_OVER_SYMBOL_OVERLAP: i32 = 75;

pub(crate) fn mul_div(value: f64, numerator: i32) -> f64 {
    value * numerator as f64 / 100.0
}

/// Font reference `fc0 = *(font+0xfc0)`, set by the font-setup routine at 0x398b4
/// to the point size: the em height (1.0 × size for HYhwpEQ, winAscent+winDescent
/// = unitsPerEm). Vertical metrics MulDiv it — script shifts, fraction/big-op gaps
/// (FUN_000320b8, FUN_0002fd28, FUN_0002dba8 …). Char baseline = fc0/2.
pub(crate) fn font_ref(fs: f64) -> f64 {
    fs
}

/// Width reference `fc4 = *(font+0xfc4)`. The setup computes `fc4 = fc0 >> 1`
/// (`asr w8,w1,#1` at 0x39910) — half the em. Horizontal defaults read it: space
/// and backtick advance = fc4/4, tilde = fc4 (FUN_0002bb88), fraction minimum
/// inner width = fc4 (FUN_0002fd28).
pub(crate) fn font_width_ref(fs: f64) -> f64 {
    font_ref(fs) * 0.5
}

/// Char/symbol box height = the GetTextExtentPoint32 `cy` (tmHeight) Hancom reads
/// for the box (FUN_0002bb88/FUN_0003ac9c second word). GDI returns tmHeight =
/// 45/44 × em for HYhwpEQ (em = winAscent+winDescent = unitsPerEm), not the bare
/// em. Ground truth: 998 corpus equations at baseUnit 1100 store height 1125 =
/// 1100·45/44 exactly. The baseline stays fc0/2 (the math axis, RE'd separately).
pub(crate) fn tm_height(fs: f64) -> f64 {
    fs * 45.0 / 44.0
}

pub(crate) fn text_box(text: &str, fs: f64, style: EqStyle) -> LayoutBox {
    LayoutBox {
        width: text_width(text, fs, style),
        height: tm_height(fs),
        baseline: fs * 0.5,
        kind: LayoutKind::Text(remap_text(text, style), style),
    }
}

/// A single big-operator / radical glyph measured the symbol way: raw advance
/// (no ×9/10) for width, tmHeight (= fs) for height (FUN_0003ac9c).
pub(crate) fn symbol_box(symbol: &str, fs: f64) -> LayoutBox {
    let remapped = remap_text(symbol, EqStyle::MathItalic);
    let width = remapped.chars().map(|c| native_symbol_advance(c, fs)).sum();
    LayoutBox {
        width,
        height: tm_height(fs),
        baseline: fs * 0.5,
        kind: LayoutKind::Text(remapped, EqStyle::MathItalic),
    }
}

pub(crate) fn empty_axis_box(fs: f64) -> LayoutBox {
    // Empty list/group `{}` (FUN_0002ab40 → FUN_0002ae84): extent = CONCAT(fc0, fc4)
    // → width = fc4 (fc0/2), height = fc0, baseline = fc0/2 (axis-centred). NOTE: the
    // RE width fc4 is kept as 0 here on purpose — feeding it through the not-yet-byte-eq
    // sup/sub *width* model (which sums script width onto the base) over-extends struts
    // like `^{{}^{{}^{}}}` (box (가) → 3.5em vs GT 2.88em). Restore fc4 once the
    // FUN_000320b8 script-width term is ported. Height is fc0 (font_ref), NOT tm_height:
    // FUN_0002ae84 returns CONCAT(fc0, fc4) literally, and FUN_000320b8 reads the empty
    // base bottom as fc0>>1 exactly. tm_height (45/44·fc0) is 0.0227·fc0 too tall per
    // nesting level and compounds through deep struts like `^{{}^{{}^{}}}`.
    LayoutBox {
        width: 0.0,
        height: font_ref(fs),
        baseline: fs * 0.5,
        kind: LayoutKind::Text(String::new(), EqStyle::MathItalic),
    }
}
