/// Failure of one OS call.
///
/// `code` is stable and machine-readable; hosts map it to their own error type
/// without re-deriving the cause. `retryable` distinguishes a transient OS
/// condition (clipboard busy, keychain locked) from a rejected request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl PlatformError {
    pub fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub(crate) fn transient(code: &'static str, error: impl std::fmt::Display) -> Self {
        Self::new(code, error.to_string(), true)
    }

    pub(crate) fn rejected(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(code, message, false)
    }
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PlatformError {}

pub type PlatformResult<T> = Result<T, PlatformError>;
