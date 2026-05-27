//! Owned TTF wrapper over `ttf-parser`. `Face` borrows its data, so we keep the
//! bytes and parse a `Face` per query (parsing only slices tables).

use ttf_parser::{Face, OutlineBuilder};

pub struct TtfFont {
    data: Vec<u8>,
    index: u32,
    pub upm: u16,
}

impl TtfFont {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_bytes(data, 0)
    }

    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Self, String> {
        let upm = {
            let f = Face::parse(&data, index).map_err(|e| format!("parse ttf: {e}"))?;
            f.units_per_em()
        };
        Ok(Self { data, index, upm })
    }

    fn face(&self) -> Face<'_> {
        Face::parse(&self.data, self.index).expect("ttf re-parse")
    }

    /// Horizontal advance of `ch` in em units (advance / unitsPerEm), or `None`
    /// if the font has no glyph for it.
    pub fn advance_em(&self, ch: char) -> Option<f64> {
        let f = self.face();
        let gid = f.glyph_index(ch)?;
        let adv = f.glyph_hor_advance(gid)?;
        Some(adv as f64 / self.upm as f64)
    }

    /// Glyph ink bounding box in em units as `(y_min, y_max)` (y-up), or `None`
    /// if the glyph has no outline. `y_max - y_min` is the ink height — what
    /// Hancom's symbol measure (FUN_0003ac9c) reads for big operators.
    pub fn glyph_bbox_em(&self, ch: char) -> Option<(f64, f64)> {
        let f = self.face();
        let gid = f.glyph_index(ch)?;
        let r = f.glyph_bounding_box(gid)?;
        let upm = self.upm as f64;
        Some((r.y_min as f64 / upm, r.y_max as f64 / upm))
    }

    /// Glyph ink x-extent in em units as `(x_min, x_max)`, or `None` if no outline.
    pub fn glyph_xbbox_em(&self, ch: char) -> Option<(f64, f64)> {
        let f = self.face();
        let gid = f.glyph_index(ch)?;
        let r = f.glyph_bounding_box(gid)?;
        let upm = self.upm as f64;
        Some((r.x_min as f64 / upm, r.x_max as f64 / upm))
    }

    /// Glyph outline as an SVG path `d`, scaled to em units (1.0 = one em),
    /// y-up (glyph coordinate space). Empty string for whitespace/no-outline.
    pub fn outline_svg_em(&self, ch: char) -> Option<String> {
        let f = self.face();
        let gid = f.glyph_index(ch)?;
        let mut b = SvgPath {
            out: String::new(),
            s: 1.0 / self.upm as f64,
        };
        f.outline_glyph(gid, &mut b)?;
        Some(b.out)
    }

    /// All family names (id 1/16) lowercased — used to index the font dir.
    pub fn family_names(&self) -> Vec<String> {
        let f = self.face();
        let mut out = Vec::new();
        for name in f.names() {
            if name.name_id == 1 || name.name_id == 16 {
                if let Some(s) = name.to_string() {
                    let s = s.trim().to_lowercase();
                    if !s.is_empty() && !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// Whether this face is the Regular weight/style (prefer it when a family
    /// name maps to multiple files).
    pub fn is_regular(&self) -> bool {
        let f = self.face();
        !f.is_bold() && !f.is_italic()
    }

    pub fn is_bold(&self) -> bool {
        self.face().is_bold()
    }
}

struct SvgPath {
    out: String,
    s: f64,
}

impl OutlineBuilder for SvgPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.out
            .push_str(&format!("M{:.4} {:.4} ", x as f64 * self.s, y as f64 * self.s));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.out
            .push_str(&format!("L{:.4} {:.4} ", x as f64 * self.s, y as f64 * self.s));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.out.push_str(&format!(
            "Q{:.4} {:.4} {:.4} {:.4} ",
            x1 as f64 * self.s,
            y1 as f64 * self.s,
            x as f64 * self.s,
            y as f64 * self.s
        ));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.out.push_str(&format!(
            "C{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} ",
            x1 as f64 * self.s,
            y1 as f64 * self.s,
            x2 as f64 * self.s,
            y2 as f64 * self.s,
            x as f64 * self.s,
            y as f64 * self.s
        ));
    }
    fn close(&mut self) {
        self.out.push_str("Z ");
    }
}
