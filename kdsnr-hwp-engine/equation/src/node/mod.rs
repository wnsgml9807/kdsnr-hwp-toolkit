mod atop;
mod decoration;
mod delimiter;
mod fraction;
mod frame;
mod limit;
mod operator;
mod pile;
mod radical;
mod row;
mod script;
mod text;

use super::types::{DecorKind, EqNode, EqStyle, LayoutBox};

pub(crate) fn layout(node: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    match node {
        EqNode::Text(value) => text::layout(value, fs, style),
        EqNode::Style(next, body) => layout(body, fs, *next),
        EqNode::Row(items) => row::layout(items, fs, style),
        EqNode::Fraction(numer, denom) => fraction::layout(numer, denom, fs, style),
        EqNode::Atop(upper, lower) => atop::layout(upper, lower, fs, style),
        EqNode::Sqrt(body) => radical::layout(body, fs, style),
        EqNode::Sup(base, sup) => script::layout_sup(base, sup, fs, style),
        EqNode::Sub(base, sub) => script::layout_sub(base, sub, fs, style),
        EqNode::SubSup(base, sub, sup) => script::layout_subsup(base, sub, sup, fs, style),
        EqNode::Limit { capitalized, sub } => {
            limit::layout(*capitalized, sub.as_deref(), fs, style)
        }
        EqNode::Integral { sub, sup } => {
            operator::layout_integral(sub.as_deref(), sup.as_deref(), fs, style)
        }
        EqNode::UnderOver { symbol, sub, sup } => {
            operator::layout_under_over(symbol, sub.as_deref(), sup.as_deref(), fs, style)
        }
        EqNode::Paren(left, right, body) => delimiter::layout(left, right, body, fs, style),
        EqNode::Pile(grid, align) => pile::layout(grid, *align, fs, style),
        EqNode::Decoration(DecorKind::Bar, body) => decoration::layout_bar(body, fs, style),
        EqNode::Decoration(DecorKind::Vec, body) => decoration::layout_vec(body, fs, style),
        EqNode::BoxFrame(body) => frame::layout(body, fs, style),
    }
}
