#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EqNode {
    Row(Vec<EqNode>),
    Text(String),
    Fraction(Box<EqNode>, Box<EqNode>),
    Atop(Box<EqNode>, Box<EqNode>),
    Sqrt(Box<EqNode>),
    Sup(Box<EqNode>, Box<EqNode>),
    Sub(Box<EqNode>, Box<EqNode>),
    SubSup(Box<EqNode>, Box<EqNode>, Box<EqNode>),
    Limit {
        capitalized: bool,
        sub: Option<Box<EqNode>>,
    },
    Integral {
        sub: Option<Box<EqNode>>,
        sup: Option<Box<EqNode>>,
    },
    UnderOver {
        symbol: String,
        sub: Option<Box<EqNode>>,
        sup: Option<Box<EqNode>>,
    },
    Paren(String, String, Box<EqNode>),
    /// A pile/matrix grid: outer Vec = rows (split by `#`), inner Vec = columns
    /// (split by `&`). A plain PILE has one column per row.
    Pile(Vec<Vec<EqNode>>, PileAlign),
    Style(EqStyle, Box<EqNode>),
    Decoration(DecorKind, Box<EqNode>),
    BoxFrame(Box<EqNode>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqStyle {
    MathItalic,
    Roman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecorKind {
    Bar,
    Vec,
}

/// Per-column cell alignment within a pile (FUN_000255e8 node+0x8): LPILE/EQALIGN
/// left (0x11), RPILE right (0x12), PILE centred (default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PileAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub(crate) struct LayoutBox {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) baseline: f64,
    pub(crate) kind: LayoutKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquationPrimitiveFragment {
    pub primitives: Vec<EquationPrimitive>,
    pub natural_width: f64,
    pub natural_height: f64,
    pub natural_baseline: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EquationPrimitive {
    Text {
        x: f64,
        baseline: f64,
        text: String,
        font_size: f64,
        style: EqStyle,
        dx: Vec<f64>,
        /// Horizontal glyph scale (장평): 1.0 normally. A vertically-stretched sign
        /// (radical) is drawn at a larger `font_size` with `x_scale = 1/scale` so it
        /// grows in height only.
        x_scale: f64,
        source: Option<&'static str>,
    },
    Line {
        role: EquationLineRole,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke_width: f64,
    },
    Rectangle {
        role: EquationLineRole,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        stroke_width: f64,
        source: &'static str,
    },
    Guide {
        source: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquationLineRole {
    BoxFrameEdge,
    /// Fraction/radical horizontal rule (HYhwpEQ glyph E06D is a 0.04-em bar);
    /// emitted as a stretched line so it spans the full rule width.
    Rule,
}

#[derive(Debug, Clone)]
pub(crate) enum LayoutKind {
    Row(Vec<Positioned>),
    Text(String, EqStyle),
    Fraction {
        numer: Box<LayoutBox>,
        denom: Box<LayoutBox>,
        source: &'static str,
    },
    Atop {
        upper: Box<LayoutBox>,
        lower: Box<LayoutBox>,
        source: &'static str,
    },
    Sqrt {
        body: Box<LayoutBox>,
        source: &'static str,
        index_scale: f64,
        /// Vertical scale applied to the E05C sign so it covers the radicand.
        sign_scale: f64,
    },
    Sup {
        base: Box<LayoutBox>,
        sup: Box<LayoutBox>,
        source: &'static str,
    },
    Sub {
        base: Box<LayoutBox>,
        sub: Box<LayoutBox>,
        source: &'static str,
    },
    SubSup {
        base: Box<LayoutBox>,
        sub: Box<LayoutBox>,
        sup: Box<LayoutBox>,
        source: &'static str,
    },
    Limit {
        capitalized: bool,
        sub: Option<Box<LayoutBox>>,
        source: &'static str,
    },
    Integral {
        symbol: Box<LayoutBox>,
        sub: Option<Box<LayoutBox>>,
        sup: Option<Box<LayoutBox>>,
        source: &'static str,
    },
    UnderOver {
        symbol: Box<LayoutBox>,
        sub: Option<Box<LayoutBox>>,
        sup: Option<Box<LayoutBox>>,
        source: &'static str,
    },
    Paren {
        left: String,
        right: String,
        body: Box<LayoutBox>,
        left_width: f64,
        right_width: f64,
        source: &'static str,
    },
    Pile {
        rows: Vec<Positioned>,
        source: &'static str,
    },
    Decoration {
        kind: DecorKind,
        body: Box<LayoutBox>,
    },
    BoxFrame {
        body: Box<LayoutBox>,
        source: &'static str,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Positioned {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) item: LayoutBox,
}

impl EquationLineRole {
    pub fn as_str(self) -> &'static str {
        match self {
            EquationLineRole::BoxFrameEdge => "box-frame-edge",
            EquationLineRole::Rule => "rule",
        }
    }
}
