use super::super::metrics;
use super::super::types::{EqStyle, LayoutBox};

pub(crate) fn layout(text: &str, fs: f64, style: EqStyle) -> LayoutBox {
    metrics::text_box(text, fs, style)
}
