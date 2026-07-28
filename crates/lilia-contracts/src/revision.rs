use serde::{Deserialize, Serialize};

/// Monotonic product revision used with `expected_revision` optimistic concurrency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProductRevision(u64);

impl ProductRevision {
    pub const INITIAL: Self = Self(1);

    pub fn new(value: u64) -> Result<Self, crate::ProductError> {
        if value == 0 {
            return Err(crate::ProductError::InvalidInput {
                field: "revision".into(),
                message: "revision must be non-zero".into(),
            });
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1).max(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExpectedRevision(u64);

impl ExpectedRevision {
    pub fn new(value: u64) -> Result<Self, crate::ProductError> {
        if value == 0 {
            return Err(crate::ProductError::InvalidInput {
                field: "expected_revision".into(),
                message: "expected_revision must be non-zero".into(),
            });
        }
        Ok(Self(value))
    }

    pub fn matches(self, actual: ProductRevision) -> bool {
        self.0 == actual.get()
    }

    pub fn get(self) -> u64 {
        self.0
    }
}
