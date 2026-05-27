use super::metrics::{
    mul_div, DECOR_RULE_GAP_SCALE, SCRIPT_SCALE, SOURCE_ATOP_PAINT, SOURCE_BAR_PAINT,
    SOURCE_BOX_PAINT, SOURCE_CHAR_MEASURE, SOURCE_INTEGRAL_PAINT, SOURCE_LIM_PAINT,
    SOURCE_PILE_PAINT, SOURCE_UNDER_OVER_PAINT, SOURCE_VEC_PAINT,
};
use super::native::native_equation_dx;
use super::types::{
    DecorKind, EqStyle, EquationLineRole, EquationPrimitive, LayoutBox, LayoutKind,
};

// HYhwpEQ ink bbox (em, probe_bigop_glyph_bbox). The E06D rule and E06E arrow-head
// both centre their ink ~0.604 em above the glyph baseline; the rule is 0.04 em
// thick. FUN_0000ccc4/FUN_0000b05c draw these glyphs at the bar/arrow baseline.
const E06D_INK_CENTER_EM: f64 = 0.604;
const E06D_INK_THICK_EM: f64 = 0.040;
const E06E_INK_CENTER_EM: f64 = 0.605;

pub(crate) fn lower_box(
    item: &LayoutBox,
    x: f64,
    y: f64,
    fs: f64,
    out: &mut Vec<EquationPrimitive>,
) {
    match &item.kind {
        LayoutKind::Text(text, style) => {
            // Glyphs draw on the GDI text baseline (tmAscent below the cell top),
            // not on the math axis (layout baseline). FUN_0002b930.
            let draw_baseline = y + super::metrics::TEXT_BASELINE_RATIO * item.height;
            lower_native_text_dispatch_primitives(x, draw_baseline, text, fs, *style, out);
        }
        LayoutKind::Row(children) => {
            for child in children {
                lower_box(&child.item, x + child.x, y + child.y, fs, out);
            }
        }
        LayoutKind::Fraction {
            numer,
            denom,
            source,
        } => {
            let _measure_source = *source;
            let line_y = y + numer.height + mul_div(fs, 30) * 0.5;
            let rule_width = item.width - mul_div(fs, 20);
            lower_box(numer, x + (item.width - numer.width) / 2.0, y, fs, out);
            // FUN_0002f9d0 paints the rule glyph E06D (0.04-em-thick bar) stretched
            // to the full width — a single glyph would only span 0.5 em.
            let rule_x = x + mul_div(fs, 10);
            out.push(EquationPrimitive::Line {
                role: EquationLineRole::Rule,
                x1: rule_x,
                y1: line_y,
                x2: rule_x + rule_width,
                y2: line_y,
                stroke_width: mul_div(fs, 4),
            });
            lower_box(
                denom,
                x + (item.width - denom.width) / 2.0,
                y + numer.height + mul_div(fs, 30),
                fs,
                out,
            );
        }
        LayoutKind::Atop {
            upper,
            lower,
            source,
        } => {
            let _measure_source = *source;
            lower_box(upper, x + (item.width - upper.width) / 2.0, y, fs, out);
            lower_box(
                lower,
                x + (item.width - lower.width) / 2.0,
                y + item.baseline - lower.baseline,
                fs,
                out,
            );
            out.push(EquationPrimitive::Guide {
                source: SOURCE_ATOP_PAINT,
            });
        }
        LayoutKind::Sqrt {
            body,
            source,
            index_scale,
            sign_scale,
        } => {
            let _index_scale = *index_scale;
            // E05C ink bbox in em (probe_bigop_glyph_bbox, HYhwpEQ): ink top y_max =
            // 0.8018, advance = 1.0. The vinculum joins the glyph at its real ink top,
            // not a guessed fraction.
            const E05C_INK_TOP_EM: f64 = 0.8018;
            let fc0 = super::metrics::font_ref(fs);
            let top_lift = mul_div(fc0, 5);
            let top_gap = mul_div(fc0, 10);
            // radical_extent = full sign advance (radical_w), back-derived from the
            // measured box: width = radical_extent + body.width + 0.17·fs pad.
            let radical_extent = item.width - body.width - mul_div(fc0, 17);
            // The sign is rendered at sign_fs so its tmHeight = scaled_sign_h, then
            // compressed horizontally (advance_em = 1.0) to radical_extent so width
            // stays fixed while the checkmark stretches in height.
            let sign_fs = fs * *sign_scale;
            // Rule sits top_lift below the box top; the radicand top sits top_gap
            // below the rule (matching the measure's baseline = top_lift+top_gap+body
            // baseline). The √ glyph's ink top aligns to the rule.
            let rule_y = y + top_lift;
            let glyph_baseline = rule_y + E05C_INK_TOP_EM * sign_fs;
            out.push(EquationPrimitive::Text {
                x,
                baseline: glyph_baseline,
                text: "\u{E05C}".to_string(),
                font_size: sign_fs,
                style: EqStyle::MathItalic,
                dx: vec![radical_extent],
                x_scale: radical_extent / sign_fs,
                source: Some(source),
            });
            // Vinculum from the glyph's top-right edge across the radicand, at the
            // glyph's ink-top y so roof and stem meet.
            let rule_x = x + radical_extent;
            out.push(EquationPrimitive::Line {
                role: EquationLineRole::Rule,
                x1: rule_x,
                y1: rule_y,
                x2: rule_x + body.width,
                y2: rule_y,
                stroke_width: mul_div(fc0, 4),
            });
            lower_box(body, rule_x, rule_y + top_gap, fs, out);
        }
        LayoutKind::Sup { base, sup, source } => {
            let _source = *source;
            let base_y = y + (item.baseline - base.baseline);
            lower_box(base, x, base_y, fs, out);
            // The script was laid out at fs·SCRIPT_SCALE; lower it at that same size so
            // its internals (a fraction/sqrt/nested script) get the right gap scale.
            lower_box(sup, x + base.width, y, fs * SCRIPT_SCALE, out);
        }
        LayoutKind::Sub { base, sub, source } => {
            let _source = *source;
            lower_box(base, x, y, fs, out);
            // Sub axis drops max(base_desc − 0.20·fc0, sub_asc) below the baseline
            // (FUN_000320b8), matching the measure so the box and glyph agree.
            let fc0 = super::metrics::font_ref(fs);
            let base_desc = base.height - base.baseline;
            let drop = (base_desc - mul_div(fc0, 20)).max(sub.baseline);
            let sub_axis = y + base.baseline + drop;
            lower_box(sub, x + base.width, sub_axis - sub.baseline, fs * SCRIPT_SCALE, out);
        }
        LayoutKind::SubSup {
            base,
            sub,
            sup,
            source,
        } => {
            let _source = *source;
            let base_y = y + (item.baseline - base.baseline);
            lower_box(base, x, base_y, fs, out);
            let script_x = x + base.width;
            let script_fs = fs * SCRIPT_SCALE;
            lower_box(sup, script_x, y, script_fs, out);
            let fc0 = super::metrics::font_ref(fs);
            let base_desc = base.height - base.baseline;
            let drop = (base_desc - mul_div(fc0, 20)).max(sub.baseline);
            let sub_axis = base_y + base.baseline + drop;
            lower_box(sub, script_x, sub_axis - sub.baseline, script_fs, out);
        }
        LayoutKind::Limit {
            capitalized,
            sub,
            source,
        } => {
            let _measure_source = *source;
            let text = if *capitalized { "Lim" } else { "lim" };
            let base = super::metrics::text_box(text, fs, EqStyle::Roman);
            lower_native_text_dispatch_primitives(
                x + (item.width - base.width) * 0.5,
                y + super::metrics::TEXT_BASELINE_RATIO * base.height,
                text,
                fs,
                EqStyle::Roman,
                out,
            );
            if let Some(sub) = sub {
                lower_box(
                    sub,
                    x + (item.width - sub.width) * 0.5,
                    y + base.height + mul_div(fs, 10),
                    fs * SCRIPT_SCALE,
                    out,
                );
            }
            out.push(EquationPrimitive::Guide {
                source: SOURCE_LIM_PAINT,
            });
        }
        LayoutKind::Integral {
            symbol,
            sub,
            sup,
            source,
        } => {
            let _measure_source = *source;
            // Symbol centred on the axis (item.baseline); sup tucks against the top,
            // sub against the bottom (FUN_0002d95c overlap). symbol.baseline = symH/2.
            let symbol_y = y + item.baseline - symbol.baseline;
            lower_box(symbol, x, symbol_y, symbol.height, out);
            // FUN_0002d95c positions the limits from the symbol's right edge with
            // DIFFERENT x: the upper limit +0.05·fs (just right), the lower limit
            // −0.50·fs (tucked left under the slanted ∫). They are not column-aligned.
            let symbol_right = x + symbol.width;
            let script_fs = fs * SCRIPT_SCALE;
            if let Some(sup) = sup {
                lower_box(sup, symbol_right + mul_div(fs, 5), y, script_fs, out);
            }
            if let Some(sub) = sub {
                lower_box(
                    sub,
                    symbol_right - mul_div(fs, 50),
                    y + item.height - sub.height,
                    script_fs,
                    out,
                );
            }
            out.push(EquationPrimitive::Guide {
                source: SOURCE_INTEGRAL_PAINT,
            });
        }
        LayoutKind::UnderOver {
            symbol,
            sub,
            sup,
            source,
        } => {
            let _measure_source = *source;
            // sup at top, sub at bottom, symbol centred in its 75%-height slot so
            // it overlaps the limits (FUN_00036708 / FUN_000365ac).
            let top = sup.as_ref().map_or(0.0, |s| s.height);
            let sym_eff = mul_div(symbol.height, 75);
            let script_fs = fs * SCRIPT_SCALE;
            if let Some(sup) = sup {
                lower_box(sup, x + (item.width - sup.width) * 0.5, y, script_fs, out);
            }
            lower_box(
                symbol,
                x + (item.width - symbol.width) * 0.5,
                y + top - (symbol.height - sym_eff) * 0.5,
                symbol.height,
                out,
            );
            if let Some(sub) = sub {
                lower_box(
                    sub,
                    x + (item.width - sub.width) * 0.5,
                    y + top + sym_eff,
                    script_fs,
                    out,
                );
            }
            out.push(EquationPrimitive::Guide {
                source: SOURCE_UNDER_OVER_PAINT,
            });
        }
        LayoutKind::Paren {
            left,
            right,
            body,
            left_width,
            right_width,
            source,
        } => {
            let _measure_source = *source;
            // Braces are sized to the body height (FUN_00007fb0), matching the measure.
            let paren_fs = item.height;
            // Draw on the text baseline (tmAscent), like the body glyphs, so the
            // brace vertically encloses the ink rather than the math-axis box.
            let ly = y + super::metrics::TEXT_BASELINE_RATIO * item.height;
            // An extensible delimiter stretches vertically only — its width stays the
            // stem width (the reserved left_width/right_width at the base size), so the
            // tall glyph is compressed horizontally back to that width rather than
            // scaling up and overflowing into the neighbouring atom (FUN_0001ba48).
            push_delim(x, ly, left, paren_fs, *left_width, out);
            lower_box(body, x + *left_width, y, fs, out);
            push_delim(
                x + *left_width + body.width,
                ly,
                right,
                paren_fs,
                *right_width,
                out,
            );
        }
        LayoutKind::Pile { rows, source } => {
            let _measure_source = *source;
            for row in rows {
                lower_box(&row.item, x + row.x, y + row.y, fs, out);
            }
            if !rows.is_empty() {
                out.push(EquationPrimitive::Guide {
                    source: SOURCE_PILE_PAINT,
                });
            }
        }
        LayoutKind::Decoration { kind, body } => {
            // Measure (FUN_0000cea0/FUN_0000b21c): the box reserves 0.20·rule_height
            // above the body box top, so the body sits `top` below the decoration top.
            let rule_height = super::metrics::tm_height(fs);
            let top = mul_div(rule_height, DECOR_RULE_GAP_SCALE);
            let body_y = y + top;
            lower_box(body, x, body_y, fs, out);
            let body_baseline = body_y + super::metrics::TEXT_BASELINE_RATIO * body.height;
            let stroke = E06D_INK_THICK_EM * fs;
            match kind {
                DecorKind::Bar => {
                    // FUN_0000ccc4: the E06D rule is drawn at baseline = body baseline
                    // − 0.30·rule_height (−0.20 box offset, −0.10 from iVar9). Its ink
                    // centres 0.604·em above that baseline.
                    let rule_baseline = body_baseline - mul_div(rule_height, 30);
                    let rule_y = rule_baseline - E06D_INK_CENTER_EM * fs;
                    out.push(EquationPrimitive::Line {
                        role: EquationLineRole::Rule,
                        x1: x,
                        y1: rule_y,
                        x2: x + body.width,
                        y2: rule_y,
                        stroke_width: stroke,
                    });
                    let _ = SOURCE_BAR_PAINT;
                }
                DecorKind::Vec => {
                    // FUN_0000b05c: arrow baseline = body baseline − 0.40·rule_height
                    // (−0.20 box, −0.20 from node 0xa8). The E06D shaft + E06E head are
                    // drawn at that baseline; both inks centre ~0.605·em above it.
                    let arrow_baseline = body_baseline - mul_div(rule_height, 40);
                    let rule_y = arrow_baseline - E06D_INK_CENTER_EM * fs;
                    let arrow_width = native_equation_dx('\u{E06E}', fs);
                    let line_width = (body.width - arrow_width).max(0.0);
                    out.push(EquationPrimitive::Line {
                        role: EquationLineRole::Rule,
                        x1: x,
                        y1: rule_y,
                        x2: x + line_width,
                        y2: rule_y,
                        stroke_width: stroke,
                    });
                    let _ = E06E_INK_CENTER_EM;
                    push_native_text_glyph(
                        x + line_width,
                        arrow_baseline,
                        '\u{E06E}',
                        fs,
                        EqStyle::MathItalic,
                        arrow_width,
                        SOURCE_VEC_PAINT,
                        out,
                    );
                }
            }
        }
        LayoutKind::BoxFrame { body, source } => {
            let _measure_source = *source;
            push_box_frame_rect(x, y, item.width, item.height, mul_div(fs, 4), out);
            // FUN_0000e70c centres the body horizontally; vertically it sits
            // 0.15·rule_width below the box top (matching the frame measure).
            let rule_width = native_equation_dx('\u{E06D}', fs);
            let pad_top = mul_div(rule_width, super::metrics::BOX_BASELINE_SHIFT_SCALE);
            lower_box(body, x + (item.width - body.width) / 2.0, y + pad_top, fs, out);
        }
    }
}

fn push_box_frame_rect(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    stroke_width: f64,
    out: &mut Vec<EquationPrimitive>,
) {
    out.push(EquationPrimitive::Rectangle {
        role: EquationLineRole::BoxFrameEdge,
        x,
        y,
        width,
        height,
        stroke_width,
        source: SOURCE_BOX_PAINT,
    });
}

/// Draw a delimiter string at `font_size` (its body-scaled height) but compressed
/// horizontally so its advance equals `target_width` (the reserved stem width): the
/// glyph stretches in height only. Empty (null `LEFT .`) draws nothing.
fn push_delim(
    x: f64,
    baseline: f64,
    delim: &str,
    font_size: f64,
    target_width: f64,
    out: &mut Vec<EquationPrimitive>,
) {
    if delim.is_empty() {
        return;
    }
    let natural: f64 = delim.chars().map(|c| native_equation_dx(c, font_size)).sum();
    let x_scale = if natural > 0.0 { target_width / natural } else { 1.0 };
    let mut cursor = x;
    for ch in delim.chars() {
        let dx = native_equation_dx(ch, font_size) * x_scale;
        out.push(EquationPrimitive::Text {
            x: cursor,
            baseline,
            text: ch.to_string(),
            font_size,
            style: EqStyle::MathItalic,
            dx: vec![dx],
            x_scale,
            source: Some(SOURCE_CHAR_MEASURE),
        });
        cursor += dx;
    }
}

fn lower_native_text_dispatch_primitives(
    x: f64,
    baseline: f64,
    native_text: &str,
    font_size: f64,
    style: EqStyle,
    out: &mut Vec<EquationPrimitive>,
) {
    let mut cursor = x;
    for ch in native_text.chars() {
        let dx = native_equation_dx(ch, font_size);
        out.push(EquationPrimitive::Text {
            x: cursor,
            baseline,
            text: ch.to_string(),
            font_size,
            style,
            dx: vec![dx],
            x_scale: 1.0,
            source: Some(SOURCE_CHAR_MEASURE),
        });
        cursor += dx;
    }
}

fn push_native_text_glyph(
    x: f64,
    baseline: f64,
    ch: char,
    font_size: f64,
    style: EqStyle,
    dx: f64,
    source: &'static str,
    out: &mut Vec<EquationPrimitive>,
) {
    out.push(EquationPrimitive::Text {
        x,
        baseline,
        text: ch.to_string(),
        font_size,
        style,
        dx: vec![dx],
        x_scale: 1.0,
        source: Some(source),
    });
}
