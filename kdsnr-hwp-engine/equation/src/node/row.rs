use super::super::metrics;
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind, Positioned};
use super::layout as layout_node;

pub(crate) fn layout(items: &[EqNode], fs: f64, style: EqStyle) -> LayoutBox {
    if items.is_empty() {
        return metrics::empty_axis_box(fs);
    }

    let mut children = Vec::new();
    let mut x = 0.0;
    let mut ascent = 0.0_f64;
    let mut descent = 0.0_f64;
    let mut prev_right: Option<usize> = None;
    for item in items {
        // The alignment tab (`&`, U+0009) is an alignment marker, not content: it
        // contributes no width and is transparent to inter-atom spacing (the atom
        // after it sees the real atom before it). Hancom left-aligns the equation at
        // its natural width inside the stored box; the tab does not expand to fill it.
        let is_tab = matches!(item, EqNode::Text(s) if s == "\u{0009}");
        let (left_class, right_class) = super::super::spacing::node_classes(item);
        // Inter-atom space before this atom, indexed by the previous atom's right
        // class then this atom's left class (FUN_0002bb88/FUN_000390ec:
        // width += SPACING[left][this]·7·fc0/100).
        if !is_tab {
            if let Some(prev) = prev_right {
                x += super::super::spacing::inter_atom_space(prev, left_class, fs);
            }
        }
        let child = layout_node(item, fs, style);
        ascent = ascent.max(child.baseline);
        descent = descent.max(child.height - child.baseline);
        let width = if is_tab { 0.0 } else { child.width };
        children.push(Positioned {
            x,
            y: 0.0,
            item: child,
        });
        x += width;
        if !is_tab {
            prev_right = Some(right_class);
        }
    }

    for child in &mut children {
        child.y = ascent - child.item.baseline;
    }

    LayoutBox {
        width: x,
        height: ascent + descent,
        baseline: ascent,
        kind: LayoutKind::Row(children),
    }
}
