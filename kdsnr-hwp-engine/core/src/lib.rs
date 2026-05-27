//! Shared native HWP engine types.

mod diagnostic;
mod geom;
mod ids;
mod result;
mod unit;

pub use diagnostic::{Diagnostic, DiagnosticLevel};
pub use geom::{Insets, Point, Rect};
pub use ids::{ControlId, PageId, ParagraphId, SectionId, SourceRef, StyleId};
pub use result::{EngineError, EngineResult};
pub use unit::EngineUnit;
