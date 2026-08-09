use std::env;

use tauri::{AppHandle, Runtime};

use crate::chat::state::normalize_backend;

use super::config::{
    backend_api_key_env, backend_direct_url, connection_mode_uses_api_key,
    connection_mode_uses_codex_account, connection_mode_uses_custom_url,
    connection_mode_uses_default_api, load_legacy_cc_switch_base_url, load_provider_config,
    load_router_mode, provider_api_key, provider_has_api_key, provider_key_for_backend,
    uses_legacy_cc_switch_mode, ROUTER_API, ROUTER_CODEX_ACCOUNT,
};
use super::types::{BackendConnectionPlan, BackendEnvStatus, ConnectionMode};

pub(crate) fn try_api_for_backend<R: Runtime>(
    app: &AppHandle<R>,
    backend: &'static str,
) -> BackendConnectionPlan {
    let cfg = load_provider_config(app, provider_key_for_backend(backend)).unwrap_or_default();
    let base_url = cfg.base_url.filter(|s| !s.trim().is_empty()).or_else(|| {
        if uses_legacy_cc_switch_mode(app, backend) {
            load_legacy_cc_switch_base_url(app)
        } else {
            None
        }
    });
    let api_key = provider_api_key(backend).ok().flatten();
    let env_api_key = env::var(backend_api_key_env(backend)).ok();
    let has_key = api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
    let has_env_key = env_api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
    let has_url = base_url.as_ref().map(|u| !u.is_empty()).unwrap_or(false);
    if !has_key && !has_env_key && !has_url {
        return BackendConnectionPlan {
            mode: ConnectionMode::Unconfigured,
            base_url: None,
            api_key: None,
        };
    }
    let mode = if has_url {
        ConnectionMode::CustomBaseUrl
    } else {
        ConnectionMode::Api
    };
    BackendConnectionPlan {
        mode,
        base_url,
        api_key: api_key.filter(|s| !s.is_empty()),
    }
}

pub(crate) fn resolve_connection_for<R: Runtime>(
    app: &AppHandle<R>,
    backend_str: &str,
) -> BackendConnectionPlan {
    let backend = normalize_backend(backend_str);
    let mode = load_router_mode(app, backend);
    match mode.as_str() {
        // Legacy router value: treat as API rather than official Codex account.
        ROUTER_API | ROUTER_CODEX_ACCOUNT => try_api_for_backend(app, backend),
        _ => BackendConnectionPlan {
            mode: ConnectionMode::Unconfigured,
            base_url: None,
            api_key: None,
        },
    }
}

pub(crate) fn build_backend_env_status<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
) -> BackendEnvStatus {
    let backend = normalize_backend(backend);
    let plan = resolve_connection_for(app, backend);
    let key_env = backend_api_key_env(backend);
    let configured_api_key = plan
        .api_key
        .as_ref()
        .map(|k| !k.is_empty())
        .unwrap_or(false)
        || env::var(key_env).map(|v| !v.is_empty()).unwrap_or(false)
        || provider_has_api_key(backend).unwrap_or(false);
    let has_api_key = connection_mode_uses_api_key(plan.mode)
        && !connection_mode_uses_codex_account(plan.mode)
        && configured_api_key;

    let effective_url = if connection_mode_uses_custom_url(plan.mode) {
        plan.base_url.clone()
    } else if connection_mode_uses_default_api(plan.mode) {
        Some(backend_direct_url(backend).to_string())
    } else {
        None
    };

    BackendEnvStatus {
        backend: backend.to_string(),
        has_api_key,
        connection_mode: plan.mode,
        effective_url,
    }
}
