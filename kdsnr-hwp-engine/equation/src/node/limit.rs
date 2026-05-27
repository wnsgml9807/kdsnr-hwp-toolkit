use super::super::metrics::{mul_div, text_box, SCRIPT_SCALE, SOURCE_LIM_MEASURE};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(
    capitalized: bool,
    sub: Option<&EqNode>,
    fs: f64,
    _style: EqStyle,
) -> LayoutBox {
    let text = if capitalized { "Lim" } else { "lim" };
    let base = text_box(text, fs, EqStyle::Roman);
    let sub = sub.map(|node| layout_node(node, fs * SCRIPT_SCALE, EqStyle::MathItalic));
    let width = sub
        .as_ref()
        .map_or(base.width, |sub| base.width.max(sub.width));
    let gap = mul_div(fs, 10);
    let height = sub
        .as_ref()
        .map_or(base.height, |sub| base.height + gap + sub.height);
    let baseline = base.baseline;

    LayoutBox {
        width,
        height,
        baseline,
        kind: LayoutKind::Limit {
            capitalized,
            sub: sub.map(Box::new),
            source: SOURCE_LIM_MEASURE,
        },
    }
}
