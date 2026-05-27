use super::super::metrics::{mul_div, tm_height, DECOR_RULE_GAP_SCALE};
use super::super::native::native_equation_dx;
use super::super::types::{DecorKind, EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout_bar(body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    layout_deco(DecorKind::Bar, body, fs, style)
}

pub(crate) fn layout_vec(body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    layout_deco(DecorKind::Vec, body, fs, style)
}

fn layout_deco(kind: DecorKind, body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let body = layout_node(body, fs, style);
    // FUN_0000cea0 (bar) / FUN_0000b21c (vec): the E06D rule glyph's extent
    // (FUN_0003ac9c = GetTextExtentPoint) sizes the clearance. The vertical gap above
    // the body is 0.20·rule_height (the extent's cy = tmHeight), NOT 0.20·fs; the
    // horizontal pad is 0.20·rule_width (the extent's cx = advance).
    let rule_width = native_equation_dx('\u{E06D}', fs);
    let rule_height = tm_height(fs);
    let rule_pad = mul_div(rule_width, DECOR_RULE_GAP_SCALE);
    let top = mul_div(rule_height, DECOR_RULE_GAP_SCALE);
    let width = match kind {
        DecorKind::Bar => body.width.max(rule_width) + rule_pad,
        DecorKind::Vec => (body.width + rule_pad).max(rule_width),
    };
    LayoutBox {
        width,
        height: body.height + top,
        baseline: body.baseline + top,
        kind: LayoutKind::Decoration {
            kind,
            body: Box::new(body),
        },
    }
}
