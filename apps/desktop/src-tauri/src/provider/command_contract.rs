use std::sync::OnceLock;

use serde::Deserialize;

const PROVIDER_COMMANDS_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/provider-commands-contract.json");

static PROVIDER_COMMANDS_CONTRACT: OnceLock<ProviderCommandsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCommandsContract {
    chat_check_env_command: String,
    provider_get_config_command: String,
    provider_set_config_command: String,
    provider_get_active_backend_command: String,
    provider_set_active_backend_command: String,
    router_get_mode_command: String,
    router_set_mode_command: String,
    assistant_ai_get_config_command: String,
    assistant_ai_set_config_command: String,
    assistant_ai_fetch_models_command: String,
    model_feature_list_model_options_command: String,
    model_feature_get_settings_command: String,
    model_feature_set_settings_command: String,
    assistant_ai_test_connection_command: String,
    assistant_ai_optimize_prompt_command: String,
}

fn provider_commands_contract() -> &'static ProviderCommandsContract {
    PROVIDER_COMMANDS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            PROVIDER_COMMANDS_CONTRACT_JSON,
            "provider-commands-contract.json",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::commands::{
        assistant_ai_fetch_models, assistant_ai_get_config, assistant_ai_optimize_prompt,
        assistant_ai_set_config, assistant_ai_test_connection, chat_check_env,
        model_feature_get_settings, model_feature_list_model_options, model_feature_set_settings,
        provider_get_active_backend, provider_get_config, provider_set_active_backend,
        provider_set_config, router_get_mode, router_set_mode,
    };

    #[test]
    fn provider_command_names_load_from_contract_manifest() {
        let contract = provider_commands_contract();
        let _ = chat_check_env;
        let _ = provider_get_config;
        let _ = provider_set_config;
        let _ = provider_get_active_backend;
        let _ = provider_set_active_backend;
        let _ = router_get_mode;
        let _ = router_set_mode;
        let _ = assistant_ai_get_config;
        let _ = assistant_ai_set_config;
        let _ = assistant_ai_fetch_models;
        let _ = model_feature_list_model_options;
        let _ = model_feature_get_settings;
        let _ = model_feature_set_settings;
        let _ = assistant_ai_test_connection;
        let _ = assistant_ai_optimize_prompt;

        assert_eq!(contract.chat_check_env_command, stringify!(chat_check_env));
        assert_eq!(
            contract.provider_get_config_command,
            stringify!(provider_get_config)
        );
        assert_eq!(
            contract.provider_set_config_command,
            stringify!(provider_set_config)
        );
        assert_eq!(
            contract.provider_get_active_backend_command,
            stringify!(provider_get_active_backend)
        );
        assert_eq!(
            contract.provider_set_active_backend_command,
            stringify!(provider_set_active_backend)
        );
        assert_eq!(
            contract.router_get_mode_command,
            stringify!(router_get_mode)
        );
        assert_eq!(
            contract.router_set_mode_command,
            stringify!(router_set_mode)
        );
        assert_eq!(
            contract.assistant_ai_get_config_command,
            stringify!(assistant_ai_get_config)
        );
        assert_eq!(
            contract.assistant_ai_set_config_command,
            stringify!(assistant_ai_set_config)
        );
        assert_eq!(
            contract.assistant_ai_fetch_models_command,
            stringify!(assistant_ai_fetch_models)
        );
        assert_eq!(
            contract.model_feature_list_model_options_command,
            stringify!(model_feature_list_model_options)
        );
        assert_eq!(
            contract.model_feature_get_settings_command,
            stringify!(model_feature_get_settings)
        );
        assert_eq!(
            contract.model_feature_set_settings_command,
            stringify!(model_feature_set_settings)
        );
        assert_eq!(
            contract.assistant_ai_test_connection_command,
            stringify!(assistant_ai_test_connection)
        );
        assert_eq!(
            contract.assistant_ai_optimize_prompt_command,
            stringify!(assistant_ai_optimize_prompt)
        );
    }
}
