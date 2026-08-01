use serde::{Deserialize, Serialize};

use crate::{ExpectedRevision, ProductError};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, ProductError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ProductError::InvalidInput {
                field: "idempotency_key".into(),
                message: "idempotency_key must not be empty".into(),
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCommandMeta {
    pub command_id: String,
    pub idempotency_key: IdempotencyKey,
    pub expected_revision: Option<ExpectedRevision>,
}

impl ProductCommandMeta {
    pub fn create(
        command_id: impl Into<String>,
        idempotency_key: IdempotencyKey,
    ) -> Result<Self, ProductError> {
        let command_id = command_id.into();
        if command_id.trim().is_empty() {
            return Err(ProductError::InvalidInput {
                field: "command_id".into(),
                message: "command_id must not be empty".into(),
            });
        }
        Ok(Self {
            command_id,
            idempotency_key,
            expected_revision: None,
        })
    }

    pub fn update(
        command_id: impl Into<String>,
        idempotency_key: IdempotencyKey,
        expected_revision: ExpectedRevision,
    ) -> Result<Self, ProductError> {
        let mut meta = Self::create(command_id, idempotency_key)?;
        meta.expected_revision = Some(expected_revision);
        Ok(meta)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProductEventSequence(u64);

impl ProductEventSequence {
    pub const ORIGIN: Self = Self(0);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCommandResult<T> {
    pub command_id: String,
    pub event_sequence: ProductEventSequence,
    pub value: T,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductEvent {
    pub sequence: ProductEventSequence,
    pub command_id: String,
    pub entity: String,
    pub entity_id: String,
    pub action: String,
    pub revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub after: Option<ProductEventSequence>,
    pub limit: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            after: None,
            limit: 100,
        }
    }
}

impl PageRequest {
    pub fn normalized_limit(&self) -> usize {
        self.limit.clamp(1, 500) as usize
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<ProductEventSequence>,
}
