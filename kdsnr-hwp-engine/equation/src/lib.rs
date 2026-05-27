//! HWP equation typesetting: parse a Hancom equation script into a node
//! tree, lay it out with HncEqEdit-measured glyph metrics, and lower it to
//! position-resolved primitives (text runs, fraction/box rules) for a backend
//! to paint. Self-contained — no engine dependencies.
//!
//! Ported module-by-module from the reference engine; the public surface is the
//! primitive fragment and the SVG helper.

mod hyhwpeq_advance;
mod layout;
mod lowering;
mod metrics;
mod native;
mod node;
mod parser;
mod spacing;
mod tokens;
mod types;

pub use types::{EqStyle, EquationLineRole, EquationPrimitive, EquationPrimitiveFragment};

/// Parse, lay out, and lower an equation script to position-resolved primitives
/// at the given base font size (HWPUNIT-agnostic; caller scales). The fragment
/// carries its natural box (width/height/baseline) for placement.
pub fn lower_equation_primitives(script: &str, base_font_size: f64) -> EquationPrimitiveFragment {
    let ast = parser::parse_equation(script);
    let layout = layout::layout_node(&ast, base_font_size, EqStyle::MathItalic);
    let mut primitives = Vec::new();
    lowering::lower_box(&layout, 0.0, 0.0, base_font_size, &mut primitives);
    // The layout baseline is the math axis (box center); glyphs draw on the text
    // baseline (tmAscent), so the equation's line baseline sits below the axis by
    // (TEXT_BASELINE_RATIO·45/44 − 1/2)·fs for a main row at the base font size.
    let axis_to_text = (metrics::TEXT_BASELINE_RATIO * 45.0 / 44.0 - 0.5) * base_font_size;
    EquationPrimitiveFragment {
        primitives,
        natural_width: layout.width,
        natural_height: layout.height,
        natural_baseline: layout.baseline + axis_to_text,
    }
}

#[cfg(test)]
mod tests;
