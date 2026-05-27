use super::super::metrics::{
    font_ref, mul_div, symbol_box, BIG_OP_SCRIPT_GAP_SCALE, INTEGRAL_SYMBOL_SCALE, SCRIPT_SCALE,
    SOURCE_INTEGRAL_MEASURE, SOURCE_UNDER_OVER_MEASURE, UNDER_OVER_SYMBOL_OVERLAP,
    UNDER_OVER_SYMBOL_SCALE,
};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout_integral(
    sub: Option<&EqNode>,
    sup: Option<&EqNode>,
    fs: f64,
    style: EqStyle,
) -> LayoutBox {
    let symbol = symbol_box("∫", fs * INTEGRAL_SYMBOL_SCALE);
    let sub = sub.map(|node| layout_node(node, fs * SCRIPT_SCALE, style));
    let sup = sup.map(|node| layout_node(node, fs * SCRIPT_SCALE, style));
    let script_width = sub
        .as_ref()
        .map_or(0.0_f64, |sub| sub.width)
        .max(sup.as_ref().map_or(0.0_f64, |sup| sup.width));
    let script_gap = mul_div(fs, BIG_OP_SCRIPT_GAP_SCALE);

    // FUN_0002dba8: the limits overlap the tall symbol about its centre (the math
    // axis). sup sits at the top with offset e8 = 0.68·fc0/6 − symH/2 (it pulls
    // up into the symbol); sub at the bottom with e4 = symH/2. So measured from
    // the centre: ascent = symH/2 + sup.h/2 − 0.68·fc0/6, descent = symH/2 + sub.h/2.
    let fc0 = font_ref(fs);
    let half_sym = symbol.height * 0.5;
    let limit_lift = mul_div(fc0, 68) / 6.0;
    let ascent = half_sym + sup.as_ref().map_or(0.0_f64, |s| s.height * 0.5 - limit_lift);
    let descent = half_sym + sub.as_ref().map_or(0.0_f64, |s| s.height * 0.5);

    let width = symbol.width + script_gap + script_width;
    LayoutBox {
        width,
        height: ascent + descent,
        baseline: ascent,
        kind: LayoutKind::Integral {
            symbol: Box::new(symbol),
            sub: sub.map(Box::new),
            sup: sup.map(Box::new),
            source: SOURCE_INTEGRAL_MEASURE,
        },
    }
}

pub(crate) fn layout_under_over(
    symbol: &str,
    sub: Option<&EqNode>,
    sup: Option<&EqNode>,
    fs: f64,
    style: EqStyle,
) -> LayoutBox {
    let symbol = symbol_box(symbol, fs * UNDER_OVER_SYMBOL_SCALE);
    let sub = sub.map(|node| layout_node(node, fs * SCRIPT_SCALE, style));
    let sup = sup.map(|node| layout_node(node, fs * SCRIPT_SCALE, style));
    let width = symbol
        .width
        .max(sub.as_ref().map_or(0.0_f64, |sub| sub.width))
        .max(sup.as_ref().map_or(0.0_f64, |sup| sup.width));
    // FUN_00036708: the symbol contributes only 0x4b=75% of its height; the limits
    // overlap it (no gap). height = sup.h + 0.75·symH + sub.h.
    let sym_eff = mul_div(symbol.height, UNDER_OVER_SYMBOL_OVERLAP);
    let top = sup.as_ref().map_or(0.0_f64, |sup| sup.height);
    let bottom = sub.as_ref().map_or(0.0_f64, |sub| sub.height);

    LayoutBox {
        width,
        height: top + sym_eff + bottom,
        baseline: top + sym_eff * 0.5,
        kind: LayoutKind::UnderOver {
            symbol: Box::new(symbol),
            sub: sub.map(Box::new),
            sup: sup.map(Box::new),
            source: SOURCE_UNDER_OVER_MEASURE,
        },
    }
}
