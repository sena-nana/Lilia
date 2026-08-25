use std::fmt;

/// Secret material in transit between a credential store and the component that
/// needs it. Its `Debug` never reveals the bytes, so a secret cannot reach a log
/// line, a journal record or an error message by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self(secret.into())
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DesktopSecret([REDACTED])")
    }
}
