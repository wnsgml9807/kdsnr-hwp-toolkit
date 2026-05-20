//! Hancom HFT decoder — bitmap and vector font support.
//!
//! Reverse-engineered from HncBaseDraw.dll (Hancom Office 12.x). Verified
//! against Frida-captured (char_code → bitmap_index) pairs at runtime.

pub mod alias;
pub mod johab;
pub mod parser;
pub mod inner_table;
pub mod bitmap;
pub mod vector;
pub mod renderer;
pub mod cipher;
pub mod cache;
pub mod ksx1001;

#[cfg(feature = "embedded")]
pub mod embedded;

pub use alias::{AliasMap, FaceCategory, FaceEntry, category_for_code};
pub use parser::{parse, HftFile, Chunk, Descriptor};
pub use renderer::{render_syllable, RenderError};
pub use cache::{HftCache, Glyph};
