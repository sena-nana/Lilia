use std::any::TypeId;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Identity of a mounted [`crate::Feature`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FeatureId(String);

impl FeatureId {
    pub fn new(value: impl Into<String>) -> Result<Self, crate::KernelError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(crate::KernelError::InvalidFeatureId(value));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FeatureId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Erased reference to a service slot, used for dependency declaration and
/// diagnostics. Construct with [`ServiceRef::of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServiceRef {
    type_id: TypeId,
    name: &'static str,
}

impl ServiceRef {
    pub fn of<K: crate::ServiceKey + ?Sized>() -> Self {
        Self {
            type_id: TypeId::of::<K>(),
            name: K::NAME,
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl fmt::Display for ServiceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name)
    }
}

/// Identity of a job submitted through [`crate::Jobs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobId(u64);

impl JobId {
    /// Only [`crate::Jobs`] mints ids for real jobs. The constructor is public
    /// so a surface that keys its own state on a job id can test that state
    /// machine without standing up a kernel; a forged id matches nothing the
    /// kernel tracks.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "job:{}", self.0)
    }
}

/// Named single-flight lane. Submitting into an occupied slot cancels the
/// previous job, so stale completions never reach consumers and features do not
/// hand-roll sequence comparisons.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct JobSlot(String);

impl JobSlot {
    pub fn new(value: impl Into<String>) -> Result<Self, crate::KernelError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(crate::KernelError::InvalidJobSlot(value));
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JobSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
