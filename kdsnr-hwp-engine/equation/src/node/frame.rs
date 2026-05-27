use super::super::metrics::{
    mul_div, tm_height, BOX_BASELINE_SHIFT_SCALE, DECOR_RULE_GAP_SCALE, SOURCE_BOX_MEASURE,
};
use super::super::native::native_equation_dx;
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let body = layout_node(body, fs, style);
    // FUN_0000e8f4: the E06D rule glyph's extent sizes the frame. width = max(body,
    // rule) + 0.20·rule_width; height = body + 0.20·rule_height; the box axis sits
    // 0.15·rule_width above the body axis (so the body's top gap is 0.15·rule_width
    // and the rest of the 0.20·rule_height pad falls below).
    let rule_width = native_equation_dx('\u{E06D}', fs);
    let rule_height = tm_height(fs);
    let pad_top = mul_div(rule_width, BOX_BASELINE_SHIFT_SCALE);
    LayoutBox {
        width: body.width.max(rule_width) + mul_div(rule_width, DECOR_RULE_GAP_SCALE),
        height: body.height + mul_div(rule_height, DECOR_RULE_GAP_SCALE),
        baseline: body.baseline + pad_top,
        kind: LayoutKind::BoxFrame {
            body: Box::new(body),
            source: SOURCE_BOX_MEASURE,
        },
    }
}
