//! KDSNR HWP/HWPX parser.
//!
//! This crate owns the HWP binary parser, HWPX parser, shared document model,
//! and HWPX serializer used by `kdsnr-hwp-toolkit`'s conversion API. Rendering
//! remains in the legacy `rhwp` path until the renderer is split separately.

pub mod error;
pub mod model;
pub mod parser;
pub mod preservation;
pub mod serializer;
pub mod split;

pub use error::HwpError;
pub use parser::{parse_document, DocumentParser};
pub use preservation::{apply_hwpx_preservation_contract, PreservationStats};
pub use serializer::{serialize_hwpx, SerializeError};
pub use split::{
    detect_subject, detect_units, split_document_contract, split_document_units, DetectedUnit,
    QuestionDocument, SplitError, Subject, UnitContract,
};
