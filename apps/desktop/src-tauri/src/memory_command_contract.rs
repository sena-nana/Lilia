use std::sync::OnceLock;

use serde::Deserialize;

const MEMORY_COMMAND_CONTRACT_JSON: &str =
    include_str!("../../../../packages/contracts/src/memory-command-contract.json");

static MEMORY_COMMAND_CONTRACT: OnceLock<MemoryCommandContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct MemoryCommandContract {
    commands: MemoryCommandsContract,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryCommandsContract {
    list: String,
    upsert: String,
    set_enabled: String,
    delete: String,
    get_settings: String,
    set_settings: String,
    get_injection_state: String,
    set_task_enabled: String,
    reset_task_cooldown: String,
}

fn memory_command_contract() -> &'static MemoryCommandContract {
    MEMORY_COMMAND_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            MEMORY_COMMAND_CONTRACT_JSON,
            "memory-command-contract.json",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{
        memory_delete, memory_get_injection_state, memory_get_settings, memory_list,
        memory_reset_task_cooldown, memory_set_enabled, memory_set_settings,
        memory_set_task_enabled, memory_upsert,
    };

    #[test]
    fn memory_command_names_load_from_contract_manifest() {
        let commands = &memory_command_contract().commands;
        let _ = memory_list;
        let _ = memory_upsert;
        let _ = memory_set_enabled;
        let _ = memory_delete;
        let _ = memory_get_settings;
        let _ = memory_set_settings;
        let _ = memory_get_injection_state;
        let _ = memory_set_task_enabled;
        let _ = memory_reset_task_cooldown;

        assert_eq!(commands.list, stringify!(memory_list));
        assert_eq!(commands.upsert, stringify!(memory_upsert));
        assert_eq!(commands.set_enabled, stringify!(memory_set_enabled));
        assert_eq!(commands.delete, stringify!(memory_delete));
        assert_eq!(commands.get_settings, stringify!(memory_get_settings));
        assert_eq!(commands.set_settings, stringify!(memory_set_settings));
        assert_eq!(
            commands.get_injection_state,
            stringify!(memory_get_injection_state)
        );
        assert_eq!(
            commands.set_task_enabled,
            stringify!(memory_set_task_enabled)
        );
        assert_eq!(
            commands.reset_task_cooldown,
            stringify!(memory_reset_task_cooldown)
        );
    }
}
