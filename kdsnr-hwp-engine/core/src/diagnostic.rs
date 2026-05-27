//! Structured diagnostics emitted by engine stages.

use crate::SourceRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Trace,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub source: Option<SourceRef>,
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            source: None,
            code,
            message: message.into(),
        }
    }
}
