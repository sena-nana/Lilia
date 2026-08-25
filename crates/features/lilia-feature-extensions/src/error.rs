use thiserror::Error;

/// Failures raised while validating or mutating extension registries.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ExtensionsError {
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error("Native Agent operation failed: {0}")]
    Agent(String),
    #[error("desktop {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("desktop {0} state revision overflowed")]
    StateRevisionOverflow(&'static str),
}

pub fn invalid_input(field: &'static str, message: impl Into<String>) -> ExtensionsError {
    ExtensionsError::InvalidInput {
        field,
        message: message.into(),
    }
}
