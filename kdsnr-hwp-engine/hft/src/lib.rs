//! Hancom HFT font decoder — bitmap and vector outlines + per-glyph advance.
//!
//! Clean-room port from HncBaseDraw / HncFontLib (Hancom Office 12.x), verified
//! against Frida-captured (char_code → glyph) pairs. `.HFT` files are loaded
//! from a directory at runtime; no font data is embedded in this crate.

pub mod alias;
pub mod bitmap;
pub mod cache;
pub mod cipher;
pub mod johab;
pub mod ksx1001;
pub mod parser;
pub mod vector;

pub use alias::{category_for_code, AliasMap, FaceCategory, FaceEntry};
pub use cache::{Glyph, GlyphMetrics, HftCache};
pub use parser::{parse, Chunk, Descriptor, HftFile};
pub use vector::{CommandKind, PathCommand};
