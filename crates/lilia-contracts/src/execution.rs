use serde::{Deserialize, Serialize};

/// Permission a user grants to a turn before the agent may act on the
/// workspace. Shared by the composer draft, the turn request and every
/// persisted turn row, so it lives in the contract vocabulary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPermission {
    Full,
    #[default]
    Ask,
    Readonly,
}

impl ExecutionPermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Ask => "ask",
            Self::Readonly => "readonly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "ask" => Some(Self::Ask),
            "readonly" => Some(Self::Readonly),
            _ => None,
        }
    }
}
