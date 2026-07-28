use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ProductResult<T> = Result<T, ProductError>;

#[derive(Clone, Debug, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProductError {
    #[error("invalid input on `{field}`: {message}")]
    InvalidInput { field: String, message: String },

    #[error("not found: {entity} `{id}`")]
    NotFound { entity: String, id: String },

    #[error("conflict ({conflict:?}): {message}")]
    Conflict {
        conflict: ConflictKind,
        message: String,
    },

    #[error("invalid state: {message}")]
    InvalidState { message: String },

    #[error("unavailable: {message}")]
    Unavailable { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    StaleRevision,
    DuplicateIdempotency,
    DependencyCycle,
    DuplicateBinding,
}
