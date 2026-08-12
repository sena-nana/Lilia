use lilia_desktop_application::{
    DesktopApplication, DesktopHookDocumentView, DesktopHookHandlerView, DesktopHookScope,
    DesktopHookSourceView,
};

use super::runtime::NATIVE_AGENTKIT_BACKEND;
use super::types::{
    HookDocumentView, HookHandlerView, HookSourceSummary, HookTrustState, HooksOverview,
};

pub fn hooks_overview(
    application: &DesktopApplication,
    project_cwd: Option<&str>,
) -> Result<HooksOverview, String> {
    application
        .hooks_overview(project_cwd)
        .map(|overview| HooksOverview {
            sources: overview.sources.into_iter().map(hook_source).collect(),
            warnings: overview.warnings,
        })
        .map_err(|error| error.to_string())
}

pub fn hook_source(source: DesktopHookSourceView) -> HookSourceSummary {
    HookSourceSummary {
        id: source.id,
        backend: NATIVE_AGENTKIT_BACKEND.to_owned(),
        scope: hook_scope_key(source.scope).to_owned(),
        format: "hooks_json".to_owned(),
        name: match source.scope {
            DesktopHookScope::User => "Lilia 全局 Hooks",
            DesktopHookScope::Project => "Lilia 项目 Hooks",
        }
        .to_owned(),
        path: source.path,
        project_cwd: source.project_cwd,
        exists: source.exists,
        editable: source.editable,
        managed: true,
        enabled: source.enabled,
        revision: source.revision,
        handler_count: source.handler_count,
        warnings: source.warnings,
        limitations: source.limitations,
        trust_state: match source.trust_state.as_str() {
            "required" => HookTrustState::Required,
            "managed" => HookTrustState::Managed,
            "n_a" => HookTrustState::NA,
            _ => HookTrustState::Unknown,
        },
        description: Some("任务运行前后执行的 Lilia AgentKit 命令".to_owned()),
    }
}

pub fn hook_document(document: DesktopHookDocumentView) -> HookDocumentView {
    HookDocumentView {
        source: hook_source(document.source),
        handlers: document.handlers.into_iter().map(hook_handler).collect(),
        raw_document: document.raw_document,
        raw_format: "json".to_owned(),
        warnings: document.warnings,
        limitations: document.limitations,
    }
}

pub fn hook_handler(handler: DesktopHookHandlerView) -> HookHandlerView {
    HookHandlerView {
        id: handler.id,
        event: handler.event,
        matcher: handler.matcher,
        r#type: handler.handler_type,
        command: handler.command,
        command_windows: handler.command_windows,
        timeout_seconds: handler.timeout_seconds,
        status_message: handler.status_message,
        supported: handler.supported,
        executable: handler.executable,
        group_advanced_json: None,
        advanced_json: None,
        warnings: handler.warnings,
    }
}

pub fn desktop_hook_scope(scope: &str) -> Result<DesktopHookScope, String> {
    match scope {
        "user" => Ok(DesktopHookScope::User),
        "project" => Ok(DesktopHookScope::Project),
        value => Err(format!("unsupported Hook scope `{value}`")),
    }
}

const fn hook_scope_key(scope: DesktopHookScope) -> &'static str {
    match scope {
        DesktopHookScope::User => "user",
        DesktopHookScope::Project => "project",
    }
}
