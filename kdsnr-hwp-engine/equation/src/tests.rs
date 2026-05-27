//! Engine tests against the primitive API (parse → layout → lower). The SVG
//! backend lives in the render crate, so these assert structure, not glyphs.

use super::{lower_equation_primitives, EquationPrimitive, EquationPrimitiveFragment};

fn lower(script: &str) -> EquationPrimitiveFragment {
    lower_equation_primitives(script, 1000.0)
}

fn text_count(f: &EquationPrimitiveFragment) -> usize {
    f.primitives.iter().filter(|p| matches!(p, EquationPrimitive::Text { .. })).count()
}

#[test]
fn plain_text_lowers_to_text_primitive() {
    let f = lower("x");
    assert!(text_count(&f) >= 1, "{:?}", f.primitives);
    assert!(f.natural_width > 0.0 && f.natural_height > 0.0);
}

#[test]
fn fraction_emits_numer_bar_denom() {
    // `a over b`: numerator + denominator as text glyphs, plus the rule as a
    // stretched Line (the E06D rule glyph is 0.5 em wide, so a single glyph would
    // not span the fraction — it is drawn as a width-spanning line instead).
    let f = lower("a over b");
    assert!(text_count(&f) >= 2, "fraction missing numer/denom: {:?}", f.primitives);
    let rules = f
        .primitives
        .iter()
        .filter(|p| matches!(p, EquationPrimitive::Line { role: crate::EquationLineRole::Rule, .. }))
        .count();
    assert_eq!(rules, 1, "fraction missing rule line: {:?}", f.primitives);
    assert!(f.natural_height > 0.0);
}

#[test]
fn sqrt_lowers_without_panic() {
    let f = lower("sqrt {2}");
    assert!(!f.primitives.is_empty());
    assert!(f.natural_height > 0.0);
}

#[test]
fn box_frame_emits_rectangle() {
    let f = lower("BOX{a}");
    let has_box = f.primitives.iter().any(|p| matches!(p, EquationPrimitive::Rectangle { .. }));
    assert!(has_box, "BOX did not emit a rectangle: {:?}", f.primitives);
}

#[test]
fn complex_script_lowers() {
    // The first sample equation: nested sup/sqrt/delimiters.
    let f = lower("LEFT ( 2 ^{`2- sqrt {2}}` RIGHT ) ^{`2+ sqrt {2}} `");
    assert!(!f.primitives.is_empty());
    assert!(f.natural_width > 0.0);
}

#[test]
#[ignore]
fn probe_natural_box_vs_stored() {
    // (script, baseUnit, stored_w, stored_h) from math_input_sample.hwpx.
    let cases = [
        ("int _{ 0} ^{1} {} LEFT ( 8x ^{`3} +1 RIGHT )`dx`", 1100.0, 6819, 2800),
        ("int _{ 0} ^{k} {}  f(x)`dx`", 1100.0, 4947, 2800),
        ("y ^{`2} =8x", 1100.0, 3217, 1313),
        ("{100} over {pi } & TIMES  int _{0} ^{{pi } over {4}}  f  ( x  ) `sec ^{`2}  x`dx`", 1100.0, 11942, 3287),
        ("f(x)= {2} over {LEFT ( x ^{`2} +1 RIGHT ) ^{`2}} &+ int _{  0} ^{2}  tf(t)`dt`", 1100.0, 14274, 2956),
    ];
    for (s, base, sw, sh) in cases {
        let f = lower_equation_primitives(s, base);
        let nw = f.natural_width;
        let nh = f.natural_height;
        // If we scale uniformly to height, the rendered width becomes nw*(sh/nh).
        let rw = nw * (sh as f64 / nh);
        eprintln!(
            "stored {:>6}x{:<5} natural {:>7.0}x{:<7.0} bl={:>6.0} | height-scaled width={:>7.0} (stored_w {}) ratio_w={:.3} ratio_h(nat/stored)={:.3}",
            sw, sh, nw, nh, f.natural_baseline, rw, sw, rw / sw as f64, nh / sh as f64
        );
    }
}

#[test]
#[ignore]
fn probe_primitive_positions() {
    let script = std::env::var("EQ").unwrap_or_else(|_| "int _{ 0} ^{1} {} LEFT ( 8x ^{`3} +1 RIGHT )`dx`".to_string());
    let f = lower_equation_primitives(&script, 1100.0);
    eprintln!("natural {}x{} bl={}", f.natural_width, f.natural_height, f.natural_baseline);
    for p in &f.primitives {
        match p {
            EquationPrimitive::Text { x, baseline, text, font_size, dx, .. } =>
                eprintln!("  TEXT x={:.0} bl={:.0} fs={:.0} dx={:?} {:?}", x, baseline, font_size, dx, text),
            EquationPrimitive::Line { x1,y1,x2,y2,.. } =>
                eprintln!("  LINE ({:.0},{:.0})-({:.0},{:.0})", x1,y1,x2,y2),
            EquationPrimitive::Rectangle { x,y,width,height,.. } =>
                eprintln!("  RECT x={:.0} y={:.0} {:.0}x{:.0}", x,y,width,height),
            EquationPrimitive::Guide { .. } => {}
        }
    }
}

#[test]
#[ignore]
fn probe_subexpr_heights() {
    // Localize where height is lost in (2^{2-√2})^{2+√2}: stored box h/fs=1.568.
    for s in [
        "2",
        "2- sqrt {2}",
        "2 ^{2}",
        "2 ^{2- sqrt {2}}",
        "LEFT ( 2 ^{2- sqrt {2}} RIGHT )",
        "LEFT ( 2 ^{`2- sqrt {2}}` RIGHT ) ^{`2+ sqrt {2}} `",
        "sqrt {2}",
        "x ^{2}",
        "x _{2}",
    ] {
        let f = lower(s);
        eprintln!("h/fs={:.3} w/fs={:.3}  {:?}", f.natural_height/1000.0, f.natural_width/1000.0, s);
    }
}

#[test]
#[ignore = "measurement probe for Q12 (가)(나)(다) box height convergence"]
fn probe_q12_box_height() {
    let scripts = [
        ("(가)box", "h` LEFT ( x RIGHT ) & =x ^{`4} -2x ^{`2} + {BOX{~(가)~ _{{}_{}}^{{}^{{}^{}}}}}"),
        ("(나)box", "f` prime  LEFT ( x RIGHT ) & = {BOX{~(나)~ _{{}_{}}^{{}^{{}^{}}}}} & TIMES  LEFT ( x+1 RIGHT )"),
        ("(다)box", "g` LEFT ( 2 RIGHT ) & -g` LEFT ( -2 RIGHT ) & =` {BOX{~(다)~ _{{}_{}}^{{}^{{}^{}}}}}"),
        ("box-only", "{BOX{~(가)~ _{{}_{}}^{{}^{{}^{}}}}}"),
        ("base-only", "{BOX{~(가)~}}"),
        ("sub-only",  "{BOX{~(가)~ _{{}_{}}}}"),
        ("sup-only",  "{BOX{~(가)~ ^{{}^{{}^{}}}}}"),
    ];
    for (name, s) in scripts {
        let f = super::lower_equation_primitives(s, 1100.0);
        println!("{:10} W={:8.1} H={:8.1} (H/1100={:.4}em)  GT_H=1701", name, f.natural_width, f.natural_height, f.natural_height/1100.0);
    }
}

#[test]
#[ignore = "per-level nested strut height dump"]
fn probe_strut_levels() {
    // Each piece at fs=1000. RE: empty leaf = h 1000, baseline 500, desc 500.
    // char base ) leaf = treated ±fc0/2 in script measure.
    for s in [
        ")",              // base char
        "{}",             // empty leaf
        "{}^{}",          // S2 = Sup(empty,empty)
        "{}^{{}^{}}",     // S1 = Sup(empty, S2)
        "{}_{}",          // sub leaf
        ")^{{}^{{}^{}}}", // ) with the 3-level sup only
        ")_{{}_{}}",      // ) with 2-level sub only
        ")^{{}^{{}^{}}}_{{}_{}}", // ) full subsup (no tildes/box)
    ] {
        let f = super::lower_equation_primitives(s, 1000.0);
        println!("h={:7.1} bl={:7.1} w={:7.1}  {:?}", f.natural_height, f.natural_baseline, f.natural_width, s);
    }
}
