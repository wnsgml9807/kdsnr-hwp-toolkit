use super::super::metrics::{font_ref, mul_div, SCRIPT_SCALE, SOURCE_SUB_MEASURE};
use super::super::types::{EqNode, EqStyle, LayoutBox, LayoutKind};
use super::layout as layout_node;

// EqSubNode::measure (FUN_000320b8) places the scripts relative to the base
// extent, not by a flat axis offset: the sup rides 0.25·fc0 (0x19) below the base
// top and grows the box above it; the sub drops max(base_desc − 0.20·fc0 (0x14),
// sub_asc) below the baseline. See layout_sup / layout_sub.

/// FUN_000320b8 width term `iVar6` (block A): when a superscript is present and
/// the base is a bare uppercase char node, Hancom adds `MulDiv(fc0,0xf,100)` =
/// 0.15·fc0 of italic correction before the script. The decompile gates this on
/// the previous sibling being type 0x6f (a char node) whose first code unit is in
/// [0x41,0x5b) — i.e. a bare `A`..`Z`. A styled/grouped base (RM/IT, a row) is a
/// different node type and gets no correction.
const SUP_UPPERCASE_KERN_PCT: i32 = 15;

/// The base text if `base` reduces to a single bare char node — a plain `Text`,
/// or a `Text` wrapped in a style (RM/IT set a flag on the char node, keeping it
/// type 0x6f; they do not make it a composite). A row or any structural node is a
/// different type and yields `None`.
fn bare_char_base(base: &EqNode) -> Option<char> {
    match base {
        EqNode::Text(s) => {
            let mut chars = s.chars().filter(|c| !c.is_whitespace());
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        }
        EqNode::Style(_, body) => bare_char_base(body),
        _ => None,
    }
}

fn sup_uppercase_kern(base: &EqNode, fs: f64) -> f64 {
    // The decompile gates on a bare char node (type 0x6f). RM/IT wrap stays
    // excluded pending data on whether Hancom keeps them type 0x6f.
    match base {
        EqNode::Text(_) => match bare_char_base(base) {
            Some(c) if c.is_ascii_uppercase() => mul_div(font_ref(fs), SUP_UPPERCASE_KERN_PCT),
            _ => 0.0,
        },
        _ => 0.0,
    }
}

fn descent(b: &LayoutBox) -> f64 {
    b.height - b.baseline
}

/// An empty group `{}` (parsed to an empty `Row`/`Text`). When such a group is
/// the BASE of a script node, HncEqEdit's empty-list measure (FUN_0002ae84)
/// returns ZERO extent, not the fc0×fc4 axis box: the empty-list branch reads the
/// list's owner node at +0xc0 and, when that owner is an EqSubNode (type 0x77,
/// which falls in the function's "return 0" jump-table set), yields {0,0}. A bare
/// `{}` elsewhere (owner not a script) keeps the fc0 box. So a script base of `{}`
/// contributes nothing, while a script ARGUMENT of `{}` still measures fc0 tall.
fn is_empty_group(node: &EqNode) -> bool {
    match node {
        EqNode::Row(items) => items.is_empty(),
        EqNode::Text(s) => s.is_empty(),
        _ => false,
    }
}

fn script_base_box(base: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    if is_empty_group(base) {
        LayoutBox { width: 0.0, height: 0.0, baseline: 0.0, kind: LayoutKind::Text(String::new(), EqStyle::MathItalic) }
    } else {
        layout_node(base, fs, style)
    }
}

pub(crate) fn layout_sup(base: &EqNode, sup: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let kern = sup_uppercase_kern(base, fs);
    let base = script_base_box(base, fs, style);
    let sup = layout_node(sup, fs * SCRIPT_SCALE, style);
    let fc0 = font_ref(fs);
    // EqSubNode::measure (FUN_000320b8), sup-only branch. The script box ascent is
    // the sup's own height (short base — the 0xac clamp drops the offset to
    // −sup_descent so the sup sits from the baseline up) OR `base_asc + sup_asc −
    // 0.25·fc0` (tall base — the sup rides 0.25·fc0 below the base top and the box
    // grows above it). The two branches join continuously at base_asc = 0.25·fc0 +
    // sup_descent, so the box ascent is their max. descent stays the base descent;
    // a superscript never drops below the baseline. 0x19 = 25.
    let ascent = sup.height.max(base.baseline + sup.baseline - mul_div(fc0, 25));
    let height = ascent + descent(&base);
    LayoutBox {
        width: base.width + sup.width + kern,
        height,
        baseline: ascent,
        kind: LayoutKind::Sup {
            base: Box::new(base),
            sup: Box::new(sup),
            source: SOURCE_SUB_MEASURE,
        },
    }
}

pub(crate) fn layout_sub(base: &EqNode, sub: &EqNode, fs: f64, style: EqStyle) -> LayoutBox {
    let base = script_base_box(base, fs, style);
    let sub = layout_node(sub, fs * SCRIPT_SCALE, style);
    let fc0 = font_ref(fs);
    // FUN_000320b8 sub-only: the sub axis drops max(base_desc − 0.20·fc0, sub_asc)
    // below the baseline (the 0xa8 clamp lifts a deep-descending base's sub to the
    // sub's own ascent). Ascent stays the base ascent; a subscript never rises. The
    // box grows only where the dropped sub overruns the base. 0x14 = 20.
    let drop = (descent(&base) - mul_div(fc0, 20)).max(sub.baseline);
    let sub_axis = base.baseline + drop;
    let height = base.height.max(sub_axis + descent(&sub));
    LayoutBox {
        width: base.width + sub.width,
        height,
        baseline: base.baseline,
        kind: LayoutKind::Sub {
            base: Box::new(base),
            sub: Box::new(sub),
            source: SOURCE_SUB_MEASURE,
        },
    }
}

pub(crate) fn layout_subsup(
    base: &EqNode,
    sub: &EqNode,
    sup: &EqNode,
    fs: f64,
    style: EqStyle,
) -> LayoutBox {
    let kern = sup_uppercase_kern(base, fs);
    let base = script_base_box(base, fs, style);
    let sub = layout_node(sub, fs * SCRIPT_SCALE, style);
    let sup = layout_node(sup, fs * SCRIPT_SCALE, style);
    let fc0 = font_ref(fs);
    // FUN_000320b8 with both scripts: the ascent follows the sup branch (rides the
    // base top, grows above), the descent follows the sub branch (the sub drops
    // max(base_desc − 0.20·fc0, sub_asc) below the baseline). 0x19=25, 0x14=20.
    let ascent = sup.height.max(base.baseline + sup.baseline - mul_div(fc0, 25));
    let drop = (descent(&base) - mul_div(fc0, 20)).max(sub.baseline);
    let bottom = ascent + descent(&base).max(drop + descent(&sub));
    LayoutBox {
        width: base.width + sub.width.max(sup.width) + kern,
        height: bottom,
        baseline: ascent,
        kind: LayoutKind::SubSup {
            base: Box::new(base),
            sub: Box::new(sub),
            sup: Box::new(sup),
            source: SOURCE_SUB_MEASURE,
        },
    }
}
