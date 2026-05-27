use super::super::metrics::{
    font_ref, mul_div, symbol_box, tm_height, ROOT_INDEX_SCALE, SOURCE_ROOT_MEASURE,
};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

pub(crate) fn layout(body: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let body = layout_node(body, fs, style);

    // EqRootNode measure (FUN_00034324):
    //  • radicand x-offset = full sign advance radical_w (no index; 0xac).
    //  • right pad 0.17·fs (0x11).
    //  • The sign is scaled in HEIGHT to cover the radicand: scale% = max(100,
    //    (0.10·fs + radicand_h)·100 / glyph_h), where glyph_h = GetTextExtent(E05C)
    //    cy = tmHeight (the high word of FUN_0003ac9c, NOT fc0). So scaled_sign_h =
    //    max(glyph_h, 0.10·fs + radicand_h), measured against the GLYPH'S own extent.
    //  • top_lift (no index) = 0.05·fs (0x05). Box height = scaled_sign_h + top_lift;
    //    box baseline = radicand top (top_lift + 0.10·fs below the box top) + radicand
    //    baseline, so the radical's baseline is its radicand's baseline.
    let fc0 = font_ref(fs);
    let radical_extent = symbol_box("\u{E05C}", fs).width;
    let right_pad = mul_div(fc0, 17);
    let top_gap = mul_div(fc0, 10);
    let top_lift = mul_div(fc0, 5);
    let glyph_h = tm_height(fs);
    let scaled_sign_h = glyph_h.max(top_gap + body.height);
    let sign_scale = scaled_sign_h / glyph_h;

    let width = radical_extent + body.width + right_pad;
    let height = scaled_sign_h + top_lift;

    LayoutBox {
        width,
        height,
        baseline: top_lift + top_gap + body.baseline,
        kind: LayoutKind::Sqrt {
            body: Box::new(body),
            source: SOURCE_ROOT_MEASURE,
            index_scale: ROOT_INDEX_SCALE,
            sign_scale,
        },
    }
}
