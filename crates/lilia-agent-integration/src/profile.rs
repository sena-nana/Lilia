//! Product-facing profile builder (#45 / #50).
//!
//! Lilia workflow kinds stay product-only and never enter AgentKit public enums.
//! Provider instances bind CredentialRef only; secrets stay in Credential Broker.

use lilia_contracts::main_agent_system_instruction;
use mutsuki_agent_contracts::{
    AgentProtocolAdapterSelection, AgentProviderInstance, AgentRuntimeMode, AgentRuntimeProfile,
    CredentialRef,
};
use mutsuki_agent_runtime::AgentRuntimeProfileBuilder;

use crate::credential::ProductCredentialBridge;
use crate::NativeRuntimeError;

/// Stable hint used when Desktop/Service assemble a Native Coding profile.
pub const PRODUCT_NATIVE_CODING_PROFILE_HINT: &str = "lilia.product.native-coding";

pub const OPENAI_COMPATIBLE_ADAPTER_ID: &str = "openai-compatible";
pub const OPENAI_COMPATIBLE_PROTOCOL_FAMILY: &str = "openai.chat-completions";
pub const ANTHROPIC_MESSAGES_ADAPTER_ID: &str = "anthropic-messages";
pub const ANTHROPIC_MESSAGES_PROTOCOL_FAMILY: &str = "anthropic.messages";

pub fn build_product_coding_profile_id(_workflow_kind: Option<&str>) -> String {
    PRODUCT_NATIVE_CODING_PROFILE_HINT.to_string()
}

/// Build a product Native Coding profile with Provider/CredentialRef bindings.
///
/// When multiple OpenAI credentials exist they share one protocol Adapter id
/// (`openai-compatible`) — matching LiliaCode#50 DoD for multi-vendor reuse.
pub fn build_product_coding_profile(
    credentials: &ProductCredentialBridge,
    workflow_kind: Option<&str>,
) -> Result<AgentRuntimeProfile, NativeRuntimeError> {
    let profile_id = build_product_coding_profile_id(workflow_kind);
    let openai_bindings = credentials.openai_compatible_bindings();
    let anthropic = credentials.primary_anthropic_credential();

    let mut builder = AgentRuntimeProfileBuilder::new(profile_id)
        .mode(AgentRuntimeMode::Production)
        .system_instruction(
            "Lilia product Native Coding Agent on Mutsuki AgentKit. Use Lilia product protocol and model protocol adapters only; never official Agent Server or brand CLI runners.",
        )
        .system_instruction(main_agent_system_instruction());

    let mut providers: Vec<(String, CredentialRef)> = openai_bindings;
    // Guarantee at least two openai-compatible provider slots when ≥1 credential exists,
    // so the shared-adapter multi-instance contract is exercised in product profiles.
    if providers.len() == 1 {
        let (id, cred) = providers[0].clone();
        providers.push((format!("{id}-alt"), cred));
    }
    let openai_has_real = providers
        .iter()
        .any(|(id, _)| !id.contains("pending") && id.starts_with("openai-compatible"));
    if providers.is_empty() {
        // Placeholder instances without credential: runtime may run reference tools,
        // but model inference remains unavailable until login.
        providers.push((
            "openai-compatible-pending".into(),
            CredentialRef {
                credential_id: "pending".into(),
                revision: 0,
            },
        ));
        providers.push((
            "openai-compatible-pending-alt".into(),
            CredentialRef {
                credential_id: "pending-alt".into(),
                revision: 0,
            },
        ));
    }

    let primary_openai_instance = providers[0].0.clone();
    for (instance_id, credential) in &providers {
        let has_real =
            credential.credential_id != "pending" && credential.credential_id != "pending-alt";
        builder = builder.provider(AgentProviderInstance {
            instance_id: instance_id.clone(),
            adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
            credential_ref: has_real.then(|| credential.clone()),
            capability_tags: vec!["chat".into(), "tools".into()],
            endpoint_profile: Some("default".into()),
            test_only: false,
        });
    }

    let mut anthropic_instance = None;
    if let Some(credential) = anthropic {
        anthropic_instance = Some("anthropic-console".to_string());
        builder = builder.provider(AgentProviderInstance {
            instance_id: "anthropic-console".into(),
            adapter_id: ANTHROPIC_MESSAGES_ADAPTER_ID.into(),
            credential_ref: Some(credential),
            capability_tags: vec!["chat".into(), "tools".into()],
            endpoint_profile: Some("default".into()),
            test_only: false,
        });
    }

    if openai_has_real {
        builder = builder.adapter(AgentProtocolAdapterSelection {
            protocol_family: OPENAI_COMPATIBLE_PROTOCOL_FAMILY.into(),
            adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
            provider_instance_id: primary_openai_instance,
            model: "product-default".into(),
            fallback_provider_instance_ids: providers
                .iter()
                .skip(1)
                .map(|(id, _)| id.clone())
                .collect(),
        });
    } else if let Some(instance_id) = anthropic_instance {
        builder = builder.adapter(AgentProtocolAdapterSelection {
            protocol_family: ANTHROPIC_MESSAGES_PROTOCOL_FAMILY.into(),
            adapter_id: ANTHROPIC_MESSAGES_ADAPTER_ID.into(),
            provider_instance_id: instance_id,
            model: "product-default".into(),
            fallback_provider_instance_ids: Vec::new(),
        });
    } else {
        builder = builder.adapter(AgentProtocolAdapterSelection {
            protocol_family: OPENAI_COMPATIBLE_PROTOCOL_FAMILY.into(),
            adapter_id: OPENAI_COMPATIBLE_ADAPTER_ID.into(),
            provider_instance_id: primary_openai_instance,
            model: "product-default".into(),
            fallback_provider_instance_ids: providers
                .iter()
                .skip(1)
                .map(|(id, _)| id.clone())
                .collect(),
        });
    }

    builder
        .build()
        .map_err(|err| NativeRuntimeError::Agent(err.to_string()))
}

pub fn profile_has_credential_refs(profile: &AgentRuntimeProfile) -> bool {
    profile
        .providers
        .iter()
        .any(|provider| provider.credential_ref.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{ProductCredentialBridge, ProductCredentialLoginInput};
    use mutsuki_agent_contracts::{CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};

    #[test]
    fn profile_id_stays_stable_across_turn_workflows() {
        assert_eq!(
            build_product_coding_profile_id(None),
            PRODUCT_NATIVE_CODING_PROFILE_HINT,
        );
        assert_eq!(
            build_product_coding_profile_id(Some("fix")),
            PRODUCT_NATIVE_CODING_PROFILE_HINT,
        );
    }

    #[test]
    fn product_profile_carries_the_shared_main_agent_instruction() {
        let profile = build_product_coding_profile(&ProductCredentialBridge::new(), None).unwrap();
        assert!(profile
            .system_instructions
            .iter()
            .any(|instruction| instruction == &main_agent_system_instruction()));
    }

    #[test]
    fn two_openai_compatible_providers_share_one_adapter() {
        let bridge = ProductCredentialBridge::new();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: Some("a".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-second-vendor-abcdef".into(),
                account_label: Some("b".into()),
                source: Some("user_api_key".into()),
            })
            .unwrap();
        let profile = build_product_coding_profile(&bridge, None).unwrap();
        let openai_providers: Vec<_> = profile
            .providers
            .iter()
            .filter(|p| p.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID)
            .collect();
        assert!(openai_providers.len() >= 2);
        assert!(openai_providers.iter().all(|p| p.credential_ref.is_some()));
        assert!(profile
            .adapters
            .iter()
            .all(|a| a.adapter_id == OPENAI_COMPATIBLE_ADAPTER_ID));
        let encoded = serde_json::to_string(&profile).unwrap();
        assert!(!encoded.contains("sk-test"));
        assert!(profile_has_credential_refs(&profile));
    }

    #[test]
    fn anthropic_only_profile_selects_messages_adapter() {
        use mutsuki_agent_contracts::ANTHROPIC_CREDENTIAL_PROVIDER_ID;
        let bridge = ProductCredentialBridge::new();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: ANTHROPIC_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-ant-api03-console-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("anthropic_console".into()),
            })
            .unwrap();
        let profile = build_product_coding_profile(&bridge, None).unwrap();
        assert!(profile
            .providers
            .iter()
            .any(|p| p.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID && p.credential_ref.is_some()));
        assert!(profile
            .adapters
            .iter()
            .any(|a| a.adapter_id == ANTHROPIC_MESSAGES_ADAPTER_ID));
    }
}
