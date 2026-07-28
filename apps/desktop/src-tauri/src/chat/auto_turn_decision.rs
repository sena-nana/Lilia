use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};

use crate::chat::model_selection_contract;
use crate::chat::state::{
    default_model_for_backend, load_persisted_resume_session_id, model_options_for_backend,
    normalize_model_for_backend, normalize_reasoning_effort_for_backend, session_key, ChatStore,
};
use crate::chat::types::{
    ChatAttachment, ChatComposerState, ChatContextUsage, ChatConversationReference,
    ChatRuntimeCommand, ChatWorkflow, ProviderRuntimeOptions, RuntimeSettingsClaude,
    RuntimeSettingsCodex,
};
use crate::chat::workflow::{runtime_command_kind, workflow_kind};
use crate::prompt_contract;
use crate::provider::{
    assistant_ai_secret, codex_account_spark_enabled, load_agent_interaction_settings,
    load_assistant_ai_config, load_model_feature_settings, request_codex_account_spark,
    AssistantAIConfig, AutoTurnDecisionSettings,
};
use crate::store::LiliaStore;
use crate::BACKEND_CODEX;

#[derive(Debug, Clone)]
pub(crate) struct PreparedTurn {
    pub(crate) composer: ChatComposerState,
    pub(crate) runtime_options: Option<ProviderRuntimeOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelTier {
    Light,
    Normal,
    Deep,
}

impl ModelTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Normal => "normal",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAutoTurnDecision {
    tier: Option<String>,
    reasoning_effort: Option<String>,
    plan_mode: Option<bool>,
    goal_mode: Option<bool>,
    session_fork: Option<bool>,
    summary: Option<String>,
    signals: Option<Vec<String>>,
}

pub(crate) fn resolve_resume_session_id<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    backend: &str,
) -> Option<String> {
    let in_memory = app.try_state::<ChatStore>().and_then(|store| {
        store
            .sdk_sessions
            .lock()
            .unwrap()
            .get(&session_key(backend, task_id))
            .cloned()
    });
    in_memory.or_else(|| {
        let store = app.try_state::<LiliaStore>()?;
        let conn = store.conn().ok()?;
        load_persisted_resume_session_id(&conn, task_id, backend)
    })
}

pub(crate) fn prepare_turn_for_start<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    content: &str,
    composer: ChatComposerState,
    project_cwd: &str,
    attachments: &[ChatAttachment],
    conversation_references: &[ChatConversationReference],
    workflow: Option<&ChatWorkflow>,
    runtime_command: Option<&ChatRuntimeCommand>,
    runtime_options: Option<ProviderRuntimeOptions>,
    resume_session_id: Option<&str>,
) -> Result<PreparedTurn, String> {
    let backend = composer.backend.clone();
    if runtime_command_skips_auto_decision(runtime_command, content) {
        return Ok(PreparedTurn {
            composer,
            runtime_options,
        });
    }
    if runtime_options
        .as_ref()
        .and_then(|options| options.common.as_ref())
        .and_then(|common| common.model_selection.as_ref())
        .is_some()
    {
        return Ok(PreparedTurn {
            composer,
            runtime_options,
        });
    }
    if has_explicit_runtime_model_or_effort(&backend, runtime_options.as_ref()) {
        return Ok(apply_runtime_or_manual_selection(
            &backend,
            composer,
            runtime_options,
            "runtimeOptions",
            Vec::new(),
        ));
    }
    if composer.model_selection_mode == "manual" {
        return Ok(apply_runtime_or_manual_selection(
            &backend,
            composer,
            runtime_options,
            "manual",
            vec!["用户手动覆盖".to_string()],
        ));
    }

    let settings = load_agent_interaction_settings(app).auto_turn_decision;
    if !settings.enabled {
        return Ok(PreparedTurn {
            composer,
            runtime_options,
        });
    }

    let raw = request_auto_turn_decision(
        app,
        task_id,
        content,
        project_cwd,
        &composer,
        attachments,
        conversation_references,
        workflow,
        runtime_command,
    )?;
    apply_auto_turn_decision_for_app(
        app,
        &backend,
        composer,
        runtime_options,
        &settings,
        raw,
        resume_session_id,
    )
}

fn runtime_command_skips_auto_decision(
    runtime_command: Option<&ChatRuntimeCommand>,
    content: &str,
) -> bool {
    match runtime_command {
        None => false,
        Some(ChatRuntimeCommand::SessionFork { .. }) => content.trim().is_empty(),
        Some(_) => true,
    }
}

fn has_explicit_runtime_model_or_effort(
    backend: &str,
    runtime_options: Option<&ProviderRuntimeOptions>,
) -> bool {
    runtime_options_model_for_backend(backend, runtime_options).is_some()
        || runtime_options_reasoning_effort_for_backend(backend, runtime_options).is_some()
}

fn runtime_options_model_for_backend(
    backend: &str,
    runtime_options: Option<&ProviderRuntimeOptions>,
) -> Option<String> {
    let options = runtime_options?;
    options
        .common
        .as_ref()
        .and_then(|common| non_empty_string(common.model.as_deref()))
        .or_else(|| {
            if backend == BACKEND_CODEX {
                options
                    .provider
                    .as_ref()
                    .and_then(|provider| provider.codex.as_ref())
                    .and_then(|codex| non_empty_string(codex.model.as_deref()))
            } else {
                None
            }
        })
}

fn runtime_options_reasoning_effort_for_backend(
    backend: &str,
    runtime_options: Option<&ProviderRuntimeOptions>,
) -> Option<String> {
    let options = runtime_options?;
    let provider = options.provider.as_ref();
    let provider_effort = if backend == BACKEND_CODEX {
        provider
            .and_then(|p| p.codex.as_ref())
            .and_then(|codex| codex.reasoning_effort.clone())
    } else {
        provider
            .and_then(|p| p.claude.as_ref())
            .and_then(|claude| claude.reasoning_effort.clone())
    };
    provider_effort.or_else(|| {
        options
            .common
            .as_ref()
            .and_then(|common| common.reasoning_effort.clone())
    })
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn apply_runtime_or_manual_selection(
    backend: &str,
    composer: ChatComposerState,
    runtime_options: Option<ProviderRuntimeOptions>,
    source: &str,
    mut signals: Vec<String>,
) -> PreparedTurn {
    let runtime_model = runtime_options_model_for_backend(backend, runtime_options.as_ref());
    let runtime_effort =
        runtime_options_reasoning_effort_for_backend(backend, runtime_options.as_ref());
    if source == "runtimeOptions" {
        signals.push("runtimeOptions 显式覆盖".to_string());
    }
    let selected_model = runtime_model
        .map(|model| normalize_model_for_backend(&model, backend))
        .unwrap_or_else(|| normalize_model_for_backend(&composer.model, backend));
    let selected_effort = normalize_reasoning_effort_for_backend(
        runtime_effort.or_else(|| composer.reasoning_effort.clone()),
        &backend,
    );
    let explanation = json!({
        "mode": if source == "manual" { "manual" } else { "auto" },
        "model": selected_model,
        "reasoningEffort": selected_effort,
        "source": source,
        "signals": signals,
        "summary": format!("{} {}{}",
            if source == "manual" { "手动覆盖" } else { "runtimeOptions 覆盖" },
            selected_model,
            selected_effort.as_deref().map(|effort| format!("，thinking {effort}")).unwrap_or_default()
        ),
    });
    let mut next_composer = composer;
    next_composer.model = selected_model.clone();
    let runtime_options = merge_runtime_selection(
        backend,
        runtime_options,
        &selected_model,
        selected_effort,
        explanation,
    );
    PreparedTurn {
        composer: next_composer,
        runtime_options: Some(runtime_options),
    }
}

fn merge_runtime_selection(
    backend: &str,
    runtime_options: Option<ProviderRuntimeOptions>,
    model: &str,
    effort: Option<String>,
    explanation: JsonValue,
) -> ProviderRuntimeOptions {
    let mut next = runtime_options.unwrap_or_default();
    let mut common = next.common.unwrap_or_default();
    common.model = Some(model.to_string());
    common.reasoning_effort = effort.clone();
    common.model_selection = Some(explanation);
    next.common = Some(common);
    let mut provider = next.provider.unwrap_or_default();
    if backend == BACKEND_CODEX {
        let mut codex: RuntimeSettingsCodex = provider.codex.unwrap_or_default();
        codex.model = Some(model.to_string());
        codex.reasoning_effort = effort.clone();
        provider.codex = Some(codex);
    } else {
        let mut claude: RuntimeSettingsClaude = provider.claude.unwrap_or_default();
        claude.reasoning_effort = effort.clone();
        if effort.is_some() && claude.thinking.is_none() {
            claude.thinking = Some(json!({ "type": "adaptive" }));
        }
        provider.claude = Some(claude);
    }
    next.provider = Some(provider);
    next
}

fn apply_auto_turn_decision_for_app<R: Runtime>(
    app: &AppHandle<R>,
    backend: &str,
    composer: ChatComposerState,
    runtime_options: Option<ProviderRuntimeOptions>,
    settings: &AutoTurnDecisionSettings,
    raw: RawAutoTurnDecision,
    resume_session_id: Option<&str>,
) -> Result<PreparedTurn, String> {
    apply_auto_turn_decision_with_model_features(
        &load_model_feature_settings(app),
        backend,
        composer,
        runtime_options,
        settings,
        raw,
        resume_session_id,
    )
}

#[cfg(test)]
fn apply_auto_turn_decision(
    backend: &str,
    composer: ChatComposerState,
    runtime_options: Option<ProviderRuntimeOptions>,
    settings: &AutoTurnDecisionSettings,
    raw: RawAutoTurnDecision,
    resume_session_id: Option<&str>,
) -> Result<PreparedTurn, String> {
    apply_auto_turn_decision_with_model_features(
        &crate::provider::ModelFeatureSettings::default(),
        backend,
        composer,
        runtime_options,
        settings,
        raw,
        resume_session_id,
    )
}

fn apply_auto_turn_decision_with_model_features(
    feature_settings: &crate::provider::ModelFeatureSettings,
    backend: &str,
    composer: ChatComposerState,
    runtime_options: Option<ProviderRuntimeOptions>,
    settings: &AutoTurnDecisionSettings,
    raw: RawAutoTurnDecision,
    resume_session_id: Option<&str>,
) -> Result<PreparedTurn, String> {
    let mut signals = raw
        .signals
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .take(8)
        .collect::<Vec<_>>();
    signals.insert(0, "辅助模型决策".to_string());

    let selected_tier = if settings.allow_model_tier {
        parse_tier(raw.tier.as_deref())?
    } else {
        signals.push("设置禁止辅助模型操作模型层级".to_string());
        tier_for_model(backend, &composer.model)
    };
    let selected_model = if settings.allow_model_tier {
        model_for_tier_from_features(feature_settings, backend, selected_tier)
    } else {
        normalize_model_for_backend(&composer.model, backend)
    };
    let mut selected_effort = if settings.allow_reasoning_effort {
        parse_reasoning_effort(raw.reasoning_effort.as_deref(), backend)?
    } else {
        signals.push("设置禁止辅助模型操作思考强度".to_string());
        normalize_reasoning_effort_for_backend(composer.reasoning_effort.clone(), backend)
    };
    if selected_effort.is_none() && settings.allow_reasoning_effort {
        selected_effort = Some(default_effort_for_tier(selected_tier));
    }

    let plan_mode = if settings.allow_plan_mode {
        raw.plan_mode.unwrap_or(false)
    } else {
        signals.push("设置禁止辅助模型操作计划模式".to_string());
        composer.plan_mode
    };
    let goal_mode = if settings.allow_goal_mode {
        raw.goal_mode.unwrap_or(false)
    } else {
        signals.push("设置禁止辅助模型操作 Goal 模式".to_string());
        composer.goal_mode
    };
    let session_fork = if settings.allow_session_fork {
        raw.session_fork.unwrap_or(false)
    } else {
        signals.push("设置禁止辅助模型操作会话分叉".to_string());
        false
    };
    if session_fork && resume_session_id.unwrap_or("").trim().is_empty() {
        return Err("辅助模型建议会话分叉，但当前对话没有可分叉的 session".to_string());
    }

    let mut next_composer = composer;
    next_composer.model = selected_model.clone();
    next_composer.reasoning_effort = None;
    next_composer.plan_mode = plan_mode;
    next_composer.goal_mode = goal_mode;

    let summary = raw
        .summary
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            format!(
                "辅助模型选择 {}，thinking {}",
                selected_model,
                selected_effort.as_deref().unwrap_or("default")
            )
        });
    let explanation = json!({
        "mode": "auto",
        "model": selected_model,
        "reasoningEffort": selected_effort,
        "tier": selected_tier.as_str(),
        "planMode": plan_mode,
        "goalMode": goal_mode,
        "sessionFork": session_fork,
        "source": "auto",
        "signals": signals,
        "summary": summary,
    });
    let runtime_options = merge_runtime_selection(
        backend,
        runtime_options,
        &next_composer.model,
        selected_effort,
        explanation,
    );
    Ok(PreparedTurn {
        composer: next_composer,
        runtime_options: Some(runtime_options),
    })
}

fn request_auto_turn_decision<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    content: &str,
    project_cwd: &str,
    composer: &ChatComposerState,
    attachments: &[ChatAttachment],
    conversation_references: &[ChatConversationReference],
    workflow: Option<&ChatWorkflow>,
    runtime_command: Option<&ChatRuntimeCommand>,
) -> Result<RawAutoTurnDecision, String> {
    let context_usage = current_context_usage(app, task_id, &composer.backend);
    let prompt = build_decision_prompt(
        content,
        project_cwd,
        composer,
        attachments,
        conversation_references,
        workflow,
        runtime_command,
        context_usage.as_ref(),
    );
    let text = if codex_account_spark_enabled(app) {
        request_codex_account_spark(
            app,
            &prompt,
            prompt_contract::auto_turn_decision_system_instruction(),
        )
        .map_err(|err| format!("辅助模型决策失败：{err}"))?
    } else {
        let model =
            assistant_ai_model_request(app, load_model_feature_settings(app).auto_turn_decision)?;
        request_openai_compatible(&model, &prompt)?
    };
    let json_text = extract_json_object(&text)?;
    serde_json::from_str::<RawAutoTurnDecision>(&json_text)
        .map_err(|err| format!("辅助模型决策 JSON 解析失败：{err}"))
}

fn assistant_ai_model_request<R: Runtime>(
    app: &AppHandle<R>,
    override_model: Option<String>,
) -> Result<AssistantAIConfig, String> {
    let mut cfg = load_assistant_ai_config(app);
    cfg.base_url = cfg
        .base_url
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    cfg.model = override_model.or_else(|| {
        cfg.model
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    });
    cfg.api_key = assistant_ai_secret()?;
    if cfg.base_url.is_none() || cfg.model.is_none() || cfg.api_key.is_none() {
        return Err("辅助模型未配置 Base URL、API key 或模型".to_string());
    }
    Ok(cfg)
}

fn request_openai_compatible(model: &AssistantAIConfig, prompt: &str) -> Result<String, String> {
    let base_url = model
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");
    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|err| format!("辅助模型 HTTP 客户端构造失败：{err}"))?;
    let resp = client
        .post(url)
        .bearer_auth(model.api_key.as_deref().unwrap_or(""))
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": prompt_contract::auto_turn_decision_system_instruction() },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.1,
            "max_tokens": 600
        }))
        .send()
        .map_err(|err| format!("辅助模型请求失败：{err}"))?;
    if !resp.status().is_success() {
        return Err(format!("辅助模型 HTTP {}", resp.status()));
    }
    let value = resp
        .json::<JsonValue>()
        .map_err(|err| format!("辅助模型响应解析失败：{err}"))?;
    value
        .get("choices")
        .and_then(JsonValue::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| "辅助模型响应缺少 message.content".to_string())
}

fn build_decision_prompt(
    content: &str,
    project_cwd: &str,
    composer: &ChatComposerState,
    attachments: &[ChatAttachment],
    conversation_references: &[ChatConversationReference],
    workflow: Option<&ChatWorkflow>,
    runtime_command: Option<&ChatRuntimeCommand>,
    context_usage: Option<&ChatContextUsage>,
) -> String {
    let attachment_summary = attachments
        .iter()
        .take(8)
        .map(|item| {
            json!({
                "kind": item.kind,
                "name": item.name,
                "path": item.path,
                "size": item.size,
            })
        })
        .collect::<Vec<_>>();
    let tier_policy = prompt_contract::auto_turn_decision_tier_policy();
    json!({
        "instruction": prompt_contract::auto_turn_decision_request_instruction(),
        "backend": composer.backend,
        "projectCwd": project_cwd,
        "promptLength": content.chars().count(),
        "promptPreview": truncate_chars(content, 1600),
        "attachmentCount": attachments.len(),
        "attachments": attachment_summary,
        "conversationReferenceCount": conversation_references.len(),
        "workflowType": workflow_kind(workflow),
        "runtimeCommandType": runtime_command_kind(runtime_command),
        "contextUsage": context_usage,
        "current": {
            "model": composer.model,
            "planMode": composer.plan_mode,
            "goalMode": composer.goal_mode,
            "permission": composer.permission,
        },
        "tierPolicy": {
            "light": &tier_policy.light,
            "normal": &tier_policy.normal,
            "deep": &tier_policy.deep
        }
    })
    .to_string()
}

fn current_context_usage<R: Runtime>(
    app: &AppHandle<R>,
    task_id: &str,
    backend: &str,
) -> Option<ChatContextUsage> {
    let store = app.try_state::<ChatStore>()?;
    let usage = store
        .context_usage
        .lock()
        .unwrap()
        .get(&session_key(backend, task_id))
        .cloned();
    usage
}

fn extract_json_object(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }
    let Some(start) = trimmed.find('{') else {
        return Err("辅助模型决策未返回 JSON 对象".to_string());
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err("辅助模型决策未返回完整 JSON 对象".to_string());
    };
    Ok(trimmed[start..=end].to_string())
}

fn parse_tier(value: Option<&str>) -> Result<ModelTier, String> {
    match value.map(str::trim) {
        Some("light") => Ok(ModelTier::Light),
        Some("normal") => Ok(ModelTier::Normal),
        Some("deep") => Ok(ModelTier::Deep),
        _ => Err("辅助模型决策缺少有效 tier".to_string()),
    }
}

fn parse_reasoning_effort(value: Option<&str>, backend: &str) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("辅助模型决策缺少有效 reasoningEffort".to_string());
    }
    normalize_reasoning_effort_for_backend(Some(trimmed.to_string()), backend)
        .ok_or_else(|| "辅助模型决策包含无效 reasoningEffort".to_string())
        .map(Some)
}

fn model_for_tier_from_features(
    feature_settings: &crate::provider::ModelFeatureSettings,
    backend: &str,
    tier: ModelTier,
) -> String {
    let configured = match tier {
        ModelTier::Light => feature_settings.chat.light.as_deref(),
        ModelTier::Normal => feature_settings.chat.normal.as_deref(),
        ModelTier::Deep => feature_settings.chat.deep.as_deref(),
    };
    model_for_tier_with_override(backend, tier, configured)
}

#[cfg(test)]
fn model_for_tier(backend: &str, tier: ModelTier) -> String {
    model_for_tier_from_features(
        &crate::provider::ModelFeatureSettings::default(),
        backend,
        tier,
    )
}

fn model_for_tier_with_override(
    backend: &str,
    tier: ModelTier,
    configured: Option<&str>,
) -> String {
    let desired = configured.unwrap_or_else(|| {
        model_selection_contract::auto_model_for_tier(backend, tier.as_str())
            .unwrap_or_else(|| default_model_for_backend(backend))
    });
    if model_options_for_backend(backend)
        .iter()
        .any(|option| option.id == desired)
    {
        desired.to_string()
    } else {
        default_model_for_backend(backend).to_string()
    }
}

fn tier_for_model(backend: &str, model: &str) -> ModelTier {
    if let Some(tier_name) = model_selection_contract::tier_for_model(backend, model) {
        return parse_tier(Some(tier_name)).unwrap_or(ModelTier::Normal);
    }
    ModelTier::Normal
}

fn default_effort_for_tier(tier: ModelTier) -> String {
    model_selection_contract::auto_reasoning_effort_for_tier(tier.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match tier {
            ModelTier::Light => "low".to_string(),
            ModelTier::Normal => "medium".to_string(),
            ModelTier::Deep => "high".to_string(),
        })
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::types::{
        CodexComposerSettings, ProviderRuntimeOptionsProvider, RuntimeSettingsCommon,
    };
    use crate::BACKEND_CLAUDE;

    fn composer(backend: &str) -> ChatComposerState {
        ChatComposerState {
            task_id: "task-1".to_string(),
            backend: backend.to_string(),
            model: if backend == BACKEND_CODEX {
                "gpt-5.4".to_string()
            } else {
                "claude-sonnet-4-6".to_string()
            },
            model_selection_mode: "auto".to_string(),
            reasoning_effort: Some("medium".to_string()),
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        }
    }

    fn raw_decision() -> RawAutoTurnDecision {
        RawAutoTurnDecision {
            tier: Some("deep".to_string()),
            reasoning_effort: Some("max".to_string()),
            plan_mode: Some(true),
            goal_mode: Some(true),
            session_fork: Some(true),
            summary: Some("需要深度处理".to_string()),
            signals: Some(vec!["复杂实现".to_string()]),
        }
    }

    #[test]
    fn applies_auto_decision_fields() {
        let prepared = apply_auto_turn_decision(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            None,
            &AutoTurnDecisionSettings::default(),
            raw_decision(),
            Some("thread-1"),
        )
        .expect("auto decision should apply");
        let runtime_options = prepared.runtime_options.expect("runtime options");
        let common = runtime_options.common.expect("common settings");
        let explanation = common.model_selection.expect("model selection");

        assert_eq!(prepared.composer.model, "gpt-5.5");
        assert_eq!(prepared.composer.plan_mode, true);
        assert_eq!(prepared.composer.goal_mode, true);
        assert_eq!(common.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(common.reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(explanation["tier"], "deep");
        assert_eq!(explanation["reasoningEffort"], "xhigh");
        assert_eq!(explanation["planMode"], true);
        assert_eq!(explanation["goalMode"], true);
        assert_eq!(explanation["sessionFork"], true);
        assert_eq!(explanation["source"], "auto");
    }

    #[test]
    fn permission_switches_ignore_disallowed_fields() {
        let mut settings = AutoTurnDecisionSettings::default();
        settings.allow_model_tier = false;
        settings.allow_reasoning_effort = false;
        settings.allow_plan_mode = false;
        settings.allow_goal_mode = false;
        settings.allow_session_fork = false;

        let prepared = apply_auto_turn_decision(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            None,
            &settings,
            raw_decision(),
            None,
        )
        .expect("disallowed session fork should not require a source session");
        let common = prepared
            .runtime_options
            .expect("runtime options")
            .common
            .unwrap();
        let explanation = common.model_selection.expect("model selection");

        assert_eq!(prepared.composer.model, "gpt-5.4");
        assert_eq!(prepared.composer.plan_mode, false);
        assert_eq!(prepared.composer.goal_mode, false);
        assert_eq!(common.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(common.reasoning_effort.as_deref(), Some("medium"));
        assert_eq!(explanation["tier"], "normal");
        assert_eq!(explanation["planMode"], false);
        assert_eq!(explanation["goalMode"], false);
        assert_eq!(explanation["sessionFork"], false);
        assert!(explanation["signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "设置禁止辅助模型操作会话分叉"));
    }

    #[test]
    fn disallowed_model_tier_ignores_invalid_tier() {
        let mut settings = AutoTurnDecisionSettings::default();
        settings.allow_model_tier = false;
        let mut raw = raw_decision();
        raw.tier = Some("huge".to_string());
        raw.session_fork = Some(false);

        let prepared = apply_auto_turn_decision(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            None,
            &settings,
            raw,
            None,
        )
        .expect("disabled tier permission should ignore invalid tier");
        let common = prepared
            .runtime_options
            .expect("runtime options")
            .common
            .unwrap();
        let explanation = common.model_selection.expect("model selection");

        assert_eq!(prepared.composer.model, "gpt-5.4");
        assert_eq!(explanation["tier"], "normal");
    }

    #[test]
    fn runtime_options_helpers_match_contracts_precedence() {
        let options = ProviderRuntimeOptions {
            common: Some(RuntimeSettingsCommon {
                model: Some(" gpt-5.4-mini ".to_string()),
                reasoning_effort: Some("medium".to_string()),
                ..RuntimeSettingsCommon::default()
            }),
            provider: Some(ProviderRuntimeOptionsProvider {
                codex: Some(RuntimeSettingsCodex {
                    model: Some("gpt-5.5".to_string()),
                    reasoning_effort: Some("high".to_string()),
                    ..RuntimeSettingsCodex::default()
                }),
                claude: Some(RuntimeSettingsClaude {
                    reasoning_effort: Some("xhigh".to_string()),
                    ..RuntimeSettingsClaude::default()
                }),
            }),
            ..ProviderRuntimeOptions::default()
        };

        assert_eq!(
            runtime_options_model_for_backend(BACKEND_CODEX, Some(&options)).as_deref(),
            Some("gpt-5.4-mini")
        );
        assert_eq!(
            runtime_options_reasoning_effort_for_backend(BACKEND_CODEX, Some(&options)).as_deref(),
            Some("high")
        );
        assert_eq!(
            runtime_options_reasoning_effort_for_backend(BACKEND_CLAUDE, Some(&options)).as_deref(),
            Some("xhigh")
        );
        assert!(has_explicit_runtime_model_or_effort(
            BACKEND_CODEX,
            Some(&options)
        ));
    }

    #[test]
    fn runtime_options_selection_uses_provider_effort_before_common_effort() {
        let options = ProviderRuntimeOptions {
            common: Some(RuntimeSettingsCommon {
                model: Some("gpt-5.4-mini".to_string()),
                reasoning_effort: Some("medium".to_string()),
                ..RuntimeSettingsCommon::default()
            }),
            provider: Some(ProviderRuntimeOptionsProvider {
                codex: Some(RuntimeSettingsCodex {
                    reasoning_effort: Some("high".to_string()),
                    ..RuntimeSettingsCodex::default()
                }),
                ..ProviderRuntimeOptionsProvider::default()
            }),
            ..ProviderRuntimeOptions::default()
        };
        let prepared = apply_runtime_or_manual_selection(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            Some(options),
            "runtimeOptions",
            Vec::new(),
        );
        let common = prepared
            .runtime_options
            .expect("runtime options")
            .common
            .expect("common settings");

        assert_eq!(prepared.composer.model, "gpt-5.4-mini");
        assert_eq!(common.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(common.model_selection.unwrap()["source"], "runtimeOptions");
    }

    #[test]
    fn invalid_decision_enums_block_turn() {
        let mut invalid_tier = raw_decision();
        invalid_tier.tier = Some("huge".to_string());
        assert!(apply_auto_turn_decision(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            None,
            &AutoTurnDecisionSettings::default(),
            invalid_tier,
            Some("thread-1"),
        )
        .unwrap_err()
        .contains("tier"));

        let mut invalid_effort = raw_decision();
        invalid_effort.reasoning_effort = Some("extreme".to_string());
        assert!(apply_auto_turn_decision(
            BACKEND_CODEX,
            composer(BACKEND_CODEX),
            None,
            &AutoTurnDecisionSettings::default(),
            invalid_effort,
            Some("thread-1"),
        )
        .unwrap_err()
        .contains("reasoningEffort"));
    }

    #[test]
    fn session_fork_requires_resume_session() {
        let err = apply_auto_turn_decision(
            BACKEND_CLAUDE,
            composer(BACKEND_CLAUDE),
            None,
            &AutoTurnDecisionSettings::default(),
            raw_decision(),
            None,
        )
        .unwrap_err();

        assert!(err.contains("没有可分叉的 session"));
    }

    #[test]
    fn model_selection_defaults_are_loaded_from_contracts_manifest() {
        assert_eq!(
            model_for_tier(BACKEND_CODEX, ModelTier::Light),
            "gpt-5.4-mini"
        );
        assert_eq!(model_for_tier(BACKEND_CODEX, ModelTier::Deep), "gpt-5.5");
        assert_eq!(
            model_for_tier(BACKEND_CLAUDE, ModelTier::Normal),
            "claude-sonnet-4-6",
        );
        assert_eq!(
            tier_for_model(BACKEND_CLAUDE, "claude-opus-4-7"),
            ModelTier::Deep
        );
        assert_eq!(default_effort_for_tier(ModelTier::Normal), "medium");
    }

    #[test]
    fn extracts_json_object_from_model_text() {
        assert_eq!(
            extract_json_object("```json\n{\"tier\":\"normal\"}\n```").unwrap(),
            "{\"tier\":\"normal\"}",
        );
        assert!(extract_json_object("没有 JSON").is_err());
    }
}
