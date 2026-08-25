use serde::{Deserialize, Serialize};

pub const MEMORY_SETTINGS_KEY: &str = "memory.settings";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryScope {
    User,
    Project,
}

impl MemoryScope {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMemory {
    pub id: String,
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub source_task_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUpsertInput {
    #[serde(default)]
    pub id: Option<String>,
    pub scope: MemoryScope,
    #[serde(default)]
    pub project_id: Option<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source_task_id: Option<String>,
    #[serde(default)]
    pub expected_updated_at: Option<i64>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,
    #[serde(default = "default_baseline_injection_enabled")]
    pub baseline_injection_enabled: bool,
    #[serde(default = "default_cooldown_turns")]
    pub cooldown_turns: u64,
}

impl MemorySettings {
    pub fn normalized(mut self) -> Self {
        if self.cooldown_turns == 0 {
            self.cooldown_turns = default_cooldown_turns();
        }
        self
    }
}

impl Default for MemorySettings {
    fn default() -> Self {
        super::contract::default_memory_settings()
    }
}

fn default_memory_enabled() -> bool {
    super::contract::default_memory_settings().enabled
}

fn default_baseline_injection_enabled() -> bool {
    super::contract::default_memory_settings().baseline_injection_enabled
}

fn default_cooldown_turns() -> u64 {
    super::contract::default_memory_settings().cooldown_turns
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInjectionState {
    pub task_id: String,
    pub enabled: bool,
    pub last_injected_turn_seq: Option<i64>,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_and_partial_payload_follow_shared_contract() {
        let defaults = MemorySettings::default();
        assert_eq!(
            defaults,
            MemorySettings {
                enabled: true,
                baseline_injection_enabled: true,
                cooldown_turns: 5,
            }
        );
        let partial = serde_json::from_value::<MemorySettings>(serde_json::json!({
            "enabled": false
        }))
        .unwrap();
        assert_eq!(
            partial,
            MemorySettings {
                enabled: false,
                ..defaults.clone()
            }
        );
        assert_eq!(
            MemorySettings {
                cooldown_turns: 0,
                ..defaults.clone()
            }
            .normalized(),
            defaults
        );
        assert_eq!(MEMORY_SETTINGS_KEY, "memory.settings");
    }
}
