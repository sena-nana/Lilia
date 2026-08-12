use std::sync::OnceLock;

use serde::Deserialize;

use super::MemorySettings;

const TASK_STATUS_MANIFEST_JSON: &str =
    include_str!("../../../../packages/contracts/src/task-statuses.json");

static DEFAULT_MEMORY_SETTINGS: OnceLock<MemorySettings> = OnceLock::new();

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskStatusManifest {
    default_memory_settings: MemorySettingsDefaults,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemorySettingsDefaults {
    enabled: bool,
    baseline_injection_enabled: bool,
    cooldown_turns: u64,
}

pub(super) fn default_memory_settings() -> MemorySettings {
    DEFAULT_MEMORY_SETTINGS
        .get_or_init(|| {
            let manifest = serde_json::from_str::<TaskStatusManifest>(TASK_STATUS_MANIFEST_JSON)
                .expect("task-statuses.json must contain valid memory defaults");
            MemorySettings {
                enabled: manifest.default_memory_settings.enabled,
                baseline_injection_enabled: manifest
                    .default_memory_settings
                    .baseline_injection_enabled,
                cooldown_turns: manifest.default_memory_settings.cooldown_turns,
            }
        })
        .clone()
}
