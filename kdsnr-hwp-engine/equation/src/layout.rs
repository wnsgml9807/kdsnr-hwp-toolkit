use super::node;
use super::types::{EqNode, EqStyle, LayoutBox};

pub(crate) fn layout_node(node: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    node::layout(node, fs, style)
}
