use super::super::metrics::{font_ref, mul_div, symbol_box, SOURCE_ATOP_MEASURE};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(upper: &EqNode, lower: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let upper = layout_node(upper, fs, style);
    let lower = layout_node(lower, fs, style);

    // FUN_00030aac: 0xb0 = E05B glyph height/10 = 0.10·fs (the row gap); side pad =
    // 5×0xb0 = 0.50·fs; minimum inner width = the E05B reference advance. The axis
    // sits gap/2 below the upper row (+0x58 = -(lower.h + gap/2)).
    let gap = mul_div(font_ref(fs), 10);
    let side_pad = gap * 5.0;
    let min_w = symbol_box("∫", fs).width;
    let width = upper.width.max(lower.width).max(min_w) + side_pad;
    let height = upper.height + lower.height + gap;
    let baseline = upper.height + gap * 0.5;

    LayoutBox {
        width,
        height,
        baseline,
        kind: LayoutKind::Atop {
            upper: Box::new(upper),
            lower: Box::new(lower),
            source: SOURCE_ATOP_MEASURE,
        },
    }
}
