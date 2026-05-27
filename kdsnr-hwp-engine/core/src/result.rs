//! Engine-wide result type.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    pub stage: &'static str,
    pub message: String,
}

impl EngineError {
    pub fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for EngineError {}

pub type EngineResult<T> = Result<T, EngineError>;
