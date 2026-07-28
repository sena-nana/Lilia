//! Native Credential Broker quota / limits surface (#50).
//!
//! AgentKit currently has no remote Provider quota API. This module exposes
//! honest diagnostics and known Adapter capability limits — never fabricated
//! remaining-quota percentages.

use mutsuki_agent_contracts::{
    CredentialStatus, ANTHROPIC_CREDENTIAL_PROVIDER_ID, OPENAI_CREDENTIAL_PROVIDER_ID,
};
use serde::Serialize;

use crate::anthropic_adapter::AnthropicMessagesAdapter;
use crate::credential::CredentialHealthSnapshot;

/// Whether a remote Provider quota / rate-limit status API is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaApiAvailability {
    /// No AgentKit quota status service for this Provider — do not invent numbers.
    Unavailable,
}

/// A known Adapter / contract limit (not a live remaining quota).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownCapabilityLimit {
    pub kind: &'static str,
    pub label: &'static str,
    pub value: Option<u64>,
    pub note: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeProviderQuotaRow {
    pub provider_id: String,
    pub display_name: String,
    pub adapter_id: Option<&'static str>,
    pub quota_api: QuotaApiAvailability,
    /// Active / expired / none / unsupported — from Credential Broker health only.
    pub credential_health: &'static str,
    pub has_usable_credential: bool,
    pub known_limits: Vec<KnownCapabilityLimit>,
    pub note: &'static str,
}

/// Product settings surface for Credential Broker usage/quota honesty (#50 DoD).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuotaSurface {
    pub source: &'static str,
    /// Local ModelUsage/ModelCost aggregation lives in product quota_usage store.
    pub local_usage_available: bool,
    pub local_usage_note: &'static str,
    pub remote_quota_api: QuotaApiAvailability,
    pub remote_quota_note: &'static str,
    /// ChatGPT / Claude subscription status must not be equated to public API quota.
    pub subscription_not_equated_to_api_quota: bool,
    pub credential: CredentialHealthSnapshot,
    pub providers: Vec<NativeProviderQuotaRow>,
}

impl NativeQuotaSurface {
    pub fn from_credential_health(credential: CredentialHealthSnapshot) -> Self {
        let providers = vec![
            provider_row(
                OPENAI_CREDENTIAL_PROVIDER_ID,
                "OpenAI / OpenAI-compatible",
                Some("openai-compatible"),
                &credential,
                openai_known_limits(),
            ),
            provider_row(
                ANTHROPIC_CREDENTIAL_PROVIDER_ID,
                "Anthropic",
                Some("anthropic-messages"),
                &credential,
                anthropic_known_limits(),
            ),
        ];
        Self {
            source: "credential-broker",
            local_usage_available: true,
            local_usage_note: "本地 Token/成本统计来自产品 usage 聚合（ModelUsage/ModelCost），不是 Provider 远程额度 API。",
            remote_quota_api: QuotaApiAvailability::Unavailable,
            remote_quota_note: "AgentKit 尚未提供 Provider rate-limit/quota status service；远程剩余额度不可用，禁止伪造数字。",
            subscription_not_equated_to_api_quota: true,
            credential,
            providers,
        }
    }
}

fn provider_row(
    provider_id: &str,
    display_name: &str,
    adapter_id: Option<&'static str>,
    health: &CredentialHealthSnapshot,
    known_limits: Vec<KnownCapabilityLimit>,
) -> NativeProviderQuotaRow {
    let matching: Vec<_> = health
        .credentials
        .iter()
        .filter(|c| c.provider_id == provider_id)
        .collect();
    let has_usable = matching.iter().any(|c| {
        c.status == CredentialStatus::Active && c.model_inference
    });
    let credential_health = if matching.is_empty() {
        "none"
    } else if has_usable {
        "active"
    } else if matching
        .iter()
        .any(|c| c.status == CredentialStatus::UnsupportedForCustomRuntime)
    {
        "unsupported_for_custom_runtime"
    } else if matching.iter().any(|c| c.status == CredentialStatus::Expired) {
        "expired"
    } else if matching.iter().any(|c| c.status == CredentialStatus::Revoked) {
        "revoked"
    } else {
        "unavailable"
    };
    NativeProviderQuotaRow {
        provider_id: provider_id.to_string(),
        display_name: display_name.to_string(),
        adapter_id,
        quota_api: QuotaApiAvailability::Unavailable,
        credential_health,
        has_usable_credential: has_usable,
        known_limits,
        note: "远程额度 API 不可用；下列为 Adapter 能力上限，不是剩余额度。",
    }
}

fn openai_known_limits() -> Vec<KnownCapabilityLimit> {
    // Mirrors product openai-compatible Adapter ModelCapability (model_turn).
    vec![
        KnownCapabilityLimit {
            kind: "context_window",
            label: "上下文窗口（tokens）",
            value: Some(128_000),
            note: "来自 openai-compatible Adapter ModelCapability，非远程剩余额度。",
        },
        KnownCapabilityLimit {
            kind: "tools",
            label: "工具调用",
            value: None,
            note: "Adapter 声明支持 tools",
        },
    ]
}

fn anthropic_known_limits() -> Vec<KnownCapabilityLimit> {
    let capability = AnthropicMessagesAdapter::default_descriptor().capability;
    vec![
        KnownCapabilityLimit {
            kind: "context_window",
            label: "上下文窗口（tokens）",
            value: Some(capability.context_window),
            note: "来自 anthropic-messages Adapter ModelCapability，非远程剩余额度。",
        },
        KnownCapabilityLimit {
            kind: "tools",
            label: "工具调用",
            value: None,
            note: if capability.tools {
                "Adapter 声明支持 tools"
            } else {
                "Adapter 未声明 tools"
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{ProductCredentialBridge, ProductCredentialLoginInput};
    use mutsuki_agent_contracts::CredentialKind;

    #[test]
    fn quota_surface_marks_remote_api_unavailable_without_fake_numbers() {
        let bridge = ProductCredentialBridge::new();
        let surface = NativeQuotaSurface::from_credential_health(bridge.health());
        assert_eq!(surface.remote_quota_api, QuotaApiAvailability::Unavailable);
        assert!(surface.subscription_not_equated_to_api_quota);
        assert!(surface.local_usage_available);
        for row in &surface.providers {
            assert_eq!(row.quota_api, QuotaApiAvailability::Unavailable);
            assert_eq!(row.credential_health, "none");
            assert!(!row.has_usable_credential);
            // Known limits may include numbers (context window) — that is capability, not quota remaining.
            assert!(row.known_limits.iter().any(|l| l.kind == "context_window"));
        }
        let encoded = serde_json::to_string(&surface).unwrap();
        assert!(!encoded.contains("remainingPercent"));
        assert!(!encoded.contains("usedPercent"));
    }

    #[test]
    fn usable_credential_updates_health_but_not_remote_quota() {
        let bridge = ProductCredentialBridge::new();
        bridge
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("settings".into()),
            })
            .unwrap();
        let surface = NativeQuotaSurface::from_credential_health(bridge.health());
        let openai = surface
            .providers
            .iter()
            .find(|p| p.provider_id == OPENAI_CREDENTIAL_PROVIDER_ID)
            .unwrap();
        assert!(openai.has_usable_credential);
        assert_eq!(openai.credential_health, "active");
        assert_eq!(openai.quota_api, QuotaApiAvailability::Unavailable);
    }
}
