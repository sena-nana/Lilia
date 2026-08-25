use crate::{PlatformError, PlatformResult};

/// One OS keychain entry, addressed by `(service, key)`.
///
/// The caller owns the naming policy: this type neither invents service names
/// nor decides which keys may be copied between services.
pub struct CredentialEntry {
    inner: keyring::Entry,
}

impl CredentialEntry {
    pub fn open(service: &str, key: &str) -> PlatformResult<Self> {
        if key.trim().is_empty() {
            return Err(PlatformError::rejected(
                "credential_key_invalid",
                "credential key must not be empty",
            ));
        }
        keyring::Entry::new(service, key)
            .map(|inner| Self { inner })
            .map_err(|error| PlatformError::new("credential_entry_failed", error.to_string(), false))
    }

    /// `Ok(None)` means the entry does not exist.
    pub fn read(&self) -> PlatformResult<Option<Vec<u8>>> {
        match self.inner.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(PlatformError::transient("credential_read_failed", error)),
        }
    }

    pub fn write(&self, secret: &[u8]) -> PlatformResult<()> {
        self.inner
            .set_secret(secret)
            .map_err(|error| PlatformError::transient("credential_write_failed", error))
    }

    /// Deleting an absent entry succeeds, so callers can converge on "gone".
    pub fn delete(&self) -> PlatformResult<()> {
        match self.inner.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(PlatformError::transient("credential_delete_failed", error)),
        }
    }
}
