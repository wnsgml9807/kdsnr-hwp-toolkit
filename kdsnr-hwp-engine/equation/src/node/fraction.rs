use super::super::metrics::{font_ref, font_width_ref, mul_div, SOURCE_OVER_MEASURE};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(numer: &EqNode, denom: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let numer = layout_node(numer, fs, style);
    let denom = layout_node(denom, fs, style);

    // FUN_0002fd28: gap = MulDiv(fc0,0x1e,100)=0.30·fc0 (stored at +0xb0),
    // side pad = MulDiv(fc0,0x32,100)=0.50·fc0, inner width = max(fc4,numer,denom)
    // with fc4 = fc0/2. The axis (+0x58 = -(denom.h + gap/2)) is the rule line.
    let fc0 = font_ref(fs);
    let gap = mul_div(fc0, 30);
    let side_pad_total = mul_div(fc0, 50);
    let fc4 = font_width_ref(fs);
    let width = numer.width.max(denom.width).max(fc4) + side_pad_total;
    let height = numer.height + denom.height + gap;
    let baseline = numer.height + gap * 0.5;

    LayoutBox {
        width,
        height,
        baseline,
        kind: LayoutKind::Fraction {
            numer: Box::new(numer),
            denom: Box::new(denom),
            source: SOURCE_OVER_MEASURE,
        },
    }
}
