//! Font resolution + per-char advance (자간/장평) for the render path.
//! See `docs/FONT_MODEL.md`.

pub mod advance;
pub mod fontcheck;
pub mod fontmap;
pub mod hftinfo;
pub mod resolver;
pub mod script;
pub mod ttf;

pub use advance::{advance_hwpunit, advance_of, glyph_em, CharMetrics, SPACE_EM};
pub use fontcheck::{format_missing_table, FontManifest, MissingFont};
pub use resolver::FontResolver;
pub use script::{script_of, Script};
pub use ttf::TtfFont;
