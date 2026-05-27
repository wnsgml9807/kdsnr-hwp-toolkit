use super::super::metrics::{symbol_box, SOURCE_PAIR_BRACE_MEASURE};
use super::super::native::normalize_delim;
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(left: &str, right: &str, body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let body = layout_node(body, fs, style);
    let left = normalize_delim(left);
    let right = normalize_delim(right);
    // FUN_00007fb0 → FUN_0001ba48: the brace is realized at the body height, but an
    // extensible delimiter assembles a top/middle/bottom stack whose WIDTH is the
    // piece stem width — it stretches vertically only. So width = glyph advance at
    // the base size, not body-height-scaled (linear width∝height over-counts tall
    // delimiters around integrals/fractions badly).
    let left_width = symbol_box(&left, fs).width;
    let right_width = symbol_box(&right, fs).width;

    LayoutBox {
        width: body.width + left_width + right_width,
        height: body.height,
        baseline: body.baseline,
        kind: LayoutKind::Paren {
            left,
            right,
            body: Box::new(body),
            left_width,
            right_width,
            source: SOURCE_PAIR_BRACE_MEASURE,
        },
    }
}
