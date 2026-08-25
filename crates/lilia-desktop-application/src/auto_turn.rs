use lilia_agent::{NativeControlModelRequest, NativeControlModelResult};
use lilia_contracts::{
    auto_context_thresholds_for_scale, auto_model_for_provider_family_tier,
    auto_preset_for_context_scale, auto_preset_for_workflow_type, auto_reasoning_effort_for_preset,
    auto_reasoning_effort_for_tier, auto_turn_decision_request_instruction,
    auto_turn_decision_system_instruction, auto_turn_decision_tier_policy, builtin_preset_label,
    plan_mode_preset, ChatContextUsage,
};
use mutsuki_agent_contracts::{ANTHROPIC_CREDENTIAL_PROVIDER_ID, OPENAI_CREDENTIAL_PROVIDER_ID};
use serde::Deserialize;
use serde_json::json;

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopAutoTurnDecisionSettings,
    DesktopAutomaticTurnSelection, DesktopModelFeatureSettings, DesktopTurnRequest,
};

const NATIVE_BACKEND: &str = "native-agentkit";

#[derive(Debug, thiserror::Error)]
pub enum DesktopAutoTurnDecisionError {
    #[error("automatic turn decision settings were not captured before dispatch")]
    MissingSettings,
    #[error("automatic turn decision model failed: {0}")]
    Model(String),
    #[error("automatic turn decision response is invalid: {0}")]
    InvalidResponse(String),
    #[error("automatic turn decision requested a session fork, but this task has no session")]
    SessionForkUnavailable,
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

impl DesktopApplication {
    pub(crate) fn apply_automatic_turn_selection(
        &self,
        mut request: DesktopTurnRequest,
    ) -> Result<DesktopTurnRequest, DesktopApplicationError> {
        if request.auto_turn_decision_applied || !request.allow_auto_turn_decision {
            return Ok(request);
        }
        normalize_explicit_selection(&mut request)?;
        if request.model.is_some() || request.reasoning_effort.is_some() {
            return Ok(apply_explicit_selection(request));
        }
        let settings = request
            .auto_turn_settings
            .clone()
            .ok_or(DesktopAutoTurnDecisionError::MissingSettings)?;
        let features = self.model_feature_settings()?;
        let context_usage = self.task_context_usage(&request.task_id)?;
        if !settings.enabled {
            return Ok(apply_local_preset_selection(
                request,
                &features,
                context_usage.as_ref(),
            ));
        }

        let has_session = !self
            .authority()
            .list_session_bindings(&request.task_id)?
            .is_empty();
        let prompt = build_decision_prompt(&request, context_usage.as_ref(), has_session);
        let generated = self
            .authority()
            .shared_runtime()
            .inner()
            .generate_control_text(NativeControlModelRequest {
                system_instruction: auto_turn_decision_system_instruction().to_owned(),
                prompt,
                model: features.auto_turn_decision.clone(),
                max_output_tokens: 600,
                reasoning: Some("low".into()),
            })
            .map_err(|error| DesktopAutoTurnDecisionError::Model(error.to_string()))?;
        let raw = parse_decision(&generated.text)?;
        apply_decision(
            &mut request,
            &settings,
            &features,
            raw,
            generated,
            has_session,
        )?;
        request.auto_turn_decision_applied = true;
        Ok(request)
    }
}

pub fn preview_automatic_turn_selection(
    request: &DesktopTurnRequest,
    features: &DesktopModelFeatureSettings,
    context_usage: Option<&ChatContextUsage>,
) -> DesktopAutomaticTurnSelection {
    let mut request = request.clone();
    request.model = None;
    request.reasoning_effort = None;
    apply_local_preset_selection(request, features, context_usage)
        .automatic_selection
        .expect("local preset selection always records its decision")
}

fn normalize_explicit_selection(
    request: &mut DesktopTurnRequest,
) -> Result<(), DesktopAutoTurnDecisionError> {
    request.model = request
        .model
        .take()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    request.reasoning_effort = request
        .reasoning_effort
        .take()
        .map(|value| normalized_reasoning_effort(&value).map(str::to_owned))
        .transpose()?;
    Ok(())
}

fn apply_explicit_selection(mut request: DesktopTurnRequest) -> DesktopTurnRequest {
    let tier = tier_for_existing_model(request.model.as_deref()).to_owned();
    let summary = request.model.as_ref().map(|model| {
        format!(
            "手动覆盖 {model}{}",
            request
                .reasoning_effort
                .as_deref()
                .map(|effort| format!("，thinking {effort}"))
                .unwrap_or_default()
        )
    });
    request.automatic_selection = Some(DesktopAutomaticTurnSelection {
        source: "manual".into(),
        tier,
        model: request.model.clone(),
        reasoning_effort: request.reasoning_effort.clone(),
        plan_mode: request.plan_mode,
        goal_mode: request.goal_mode,
        session_fork: false,
        summary,
        signals: vec!["用户手动覆盖".into()],
        decision_provider_id: "manual".into(),
        decision_model: "composer".into(),
    });
    request.auto_turn_decision_applied = true;
    request
}

fn build_decision_prompt(
    request: &DesktopTurnRequest,
    context_usage: Option<&lilia_contracts::ChatContextUsage>,
    has_session: bool,
) -> String {
    let attachments = request
        .attachments
        .iter()
        .take(8)
        .map(|attachment| {
            json!({
                "kind": attachment.kind.as_str(),
                "name": &attachment.name,
                "path": &attachment.path,
                "size": attachment.size,
            })
        })
        .collect::<Vec<_>>();
    let tier_policy = auto_turn_decision_tier_policy();
    json!({
        "instruction": auto_turn_decision_request_instruction(),
        "backend": NATIVE_BACKEND,
        "projectCwd": request.workspace_path.as_deref(),
        "promptLength": request.content.chars().count(),
        "promptPreview": request.content.chars().take(1600).collect::<String>(),
        "attachmentCount": request.attachments.len(),
        "attachments": attachments,
        "conversationReferenceCount": request.conversation_references.len(),
        "workflowType": request.automation.as_ref().map(|_| "automation"),
        "runtimeCommandType": serde_json::Value::Null,
        "contextUsage": context_usage,
        "hasSession": has_session,
        "current": {
            "model": request.model.as_deref(),
            "reasoningEffort": request.reasoning_effort.as_deref(),
            "planMode": request.plan_mode,
            "goalMode": request.goal_mode,
            "permission": request.permission.as_str(),
        },
        "tierPolicy": {
            "light": &tier_policy.light,
            "normal": &tier_policy.normal,
            "deep": &tier_policy.deep,
        },
    })
    .to_string()
}

fn apply_decision(
    request: &mut DesktopTurnRequest,
    settings: &DesktopAutoTurnDecisionSettings,
    features: &DesktopModelFeatureSettings,
    raw: RawAutoTurnDecision,
    generated: NativeControlModelResult,
    has_session: bool,
) -> Result<(), DesktopAutoTurnDecisionError> {
    let tier = if settings.allow_model_tier {
        normalized_tier(raw.tier.as_deref())?.to_owned()
    } else {
        tier_for_existing_model(request.model.as_deref()).to_owned()
    };
    let family = provider_family(&generated.provider_id, &generated.model);
    let model = if settings.allow_model_tier {
        model_for_tier(features, family, &tier)
    } else {
        request.model.clone()
    };
    let reasoning_effort = if settings.allow_reasoning_effort {
        match raw.reasoning_effort.as_deref() {
            Some(value) => Some(normalized_reasoning_effort(value)?.to_owned()),
            None => effort_for_tier(features, &tier),
        }
    } else {
        request.reasoning_effort.clone()
    };
    let plan_mode = if settings.allow_plan_mode {
        raw.plan_mode.unwrap_or(false)
    } else {
        request.plan_mode
    };
    let goal_mode = if settings.allow_goal_mode {
        raw.goal_mode.unwrap_or(false)
    } else {
        request.goal_mode
    };
    let session_fork = settings.allow_session_fork && raw.session_fork.unwrap_or(false);
    if session_fork && !has_session {
        return Err(DesktopAutoTurnDecisionError::SessionForkUnavailable);
    }
    let summary = raw
        .summary
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let mut signals = raw
        .signals
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .take(8)
        .collect::<Vec<_>>();
    signals.insert(0, "辅助模型决策".into());

    request.model = model.clone();
    request.reasoning_effort = reasoning_effort.clone();
    request.plan_mode = plan_mode;
    request.goal_mode = goal_mode;
    request.session_fork = session_fork;
    request.automatic_selection = Some(DesktopAutomaticTurnSelection {
        source: "auto".into(),
        tier,
        model,
        reasoning_effort,
        plan_mode,
        goal_mode,
        session_fork,
        summary,
        signals,
        decision_provider_id: generated.provider_id,
        decision_model: generated.model,
    });
    Ok(())
}

fn apply_local_preset_selection(
    mut request: DesktopTurnRequest,
    features: &DesktopModelFeatureSettings,
    context_usage: Option<&ChatContextUsage>,
) -> DesktopTurnRequest {
    let mut signals = Vec::new();
    let preset = select_local_preset(&request, context_usage, &mut signals);
    let tier = lilia_contracts::tier_for_preset(preset).unwrap_or("normal");
    let configured_model = preset_model(features, preset, tier);
    let family = configured_model
        .as_deref()
        .map(|model| provider_family("", model))
        .unwrap_or("openai");
    let model = configured_model
        .or_else(|| auto_model_for_provider_family_tier(family, tier).map(str::to_owned));
    let reasoning_effort = preset_effort(features, preset)
        .or_else(|| auto_reasoning_effort_for_preset(preset).map(str::to_owned))
        .or_else(|| auto_reasoning_effort_for_tier(tier).map(str::to_owned));
    let label = builtin_preset_label(preset).unwrap_or(preset);
    let summary = model.as_ref().map(|model| {
        format!(
            "自动选择 [{label}] {model}{}",
            reasoning_effort
                .as_deref()
                .map(|effort| format!("，thinking {effort}"))
                .unwrap_or_default()
        )
    });

    request.model = model.clone();
    request.reasoning_effort = reasoning_effort.clone();
    request.automatic_selection = Some(DesktopAutomaticTurnSelection {
        source: "auto".into(),
        tier: tier.to_owned(),
        model,
        reasoning_effort,
        plan_mode: request.plan_mode,
        goal_mode: request.goal_mode,
        session_fork: false,
        summary,
        signals,
        decision_provider_id: "deterministic".into(),
        decision_model: "preset-router".into(),
    });
    request.auto_turn_decision_applied = true;
    request
}

fn select_local_preset(
    request: &DesktopTurnRequest,
    context_usage: Option<&ChatContextUsage>,
    signals: &mut Vec<String>,
) -> &'static str {
    if request.plan_mode {
        signals.push("计划模式".into());
        return plan_mode_preset();
    }
    if let Some(workflow) = request.workflow.as_ref() {
        if let Some(preset) = auto_preset_for_workflow_type(workflow.kind()) {
            signals.push(if preset == "fast" {
                format!("轻量工作流 {}", workflow.kind())
            } else {
                format!("工作流 {}", workflow.kind())
            });
            return preset;
        }
    }
    let scale = context_scale(request, context_usage);
    signals.push(format!("上下文规模 {scale}"));
    auto_preset_for_context_scale(scale).unwrap_or("default")
}

fn context_scale(
    request: &DesktopTurnRequest,
    context_usage: Option<&ChatContextUsage>,
) -> &'static str {
    if context_exceeds(request, context_usage, "large") {
        "large"
    } else if context_exceeds(request, context_usage, "medium") {
        "medium"
    } else {
        "small"
    }
}

fn context_exceeds(
    request: &DesktopTurnRequest,
    context_usage: Option<&ChatContextUsage>,
    scale: &str,
) -> bool {
    let Some(thresholds) = auto_context_thresholds_for_scale(scale) else {
        return false;
    };
    context_usage
        .and_then(|usage| usage.used_percent)
        .is_some_and(|used| used >= thresholds.context_usage_percent)
        || request.content.trim().chars().count() >= thresholds.prompt_length
        || request.attachments.len() >= thresholds.attachment_count
        || request.conversation_references.len() >= thresholds.conversation_reference_count
        || request.attachments.iter().any(|attachment| {
            attachment.directory.as_ref().is_some_and(|directory| {
                directory.truncated
                    || thresholds
                        .directory_file_count
                        .is_some_and(|count| directory.file_count >= count)
                    || thresholds
                        .directory_total_size
                        .is_some_and(|size| directory.total_size >= size)
            })
        })
}

fn preset_model(
    features: &DesktopModelFeatureSettings,
    preset: &str,
    tier: &str,
) -> Option<String> {
    features
        .presets
        .iter()
        .find(|candidate| candidate.id == preset && candidate.enabled)
        .and_then(|candidate| candidate.model.clone())
        .or_else(|| match tier {
            "light" => features.chat.light.clone(),
            "deep" => features.chat.deep.clone(),
            _ => features.chat.normal.clone(),
        })
}

fn preset_effort(features: &DesktopModelFeatureSettings, preset: &str) -> Option<String> {
    features
        .presets
        .iter()
        .find(|candidate| candidate.id == preset && candidate.enabled)
        .and_then(|candidate| candidate.reasoning_effort.clone())
}

fn model_for_tier(
    features: &DesktopModelFeatureSettings,
    family: &str,
    tier: &str,
) -> Option<String> {
    let preset = match tier {
        "light" => "fast",
        "deep" => "plan",
        _ => "default",
    };
    preset_model(features, preset, tier)
        .or_else(|| auto_model_for_provider_family_tier(family, tier).map(str::to_owned))
}

fn effort_for_tier(features: &DesktopModelFeatureSettings, tier: &str) -> Option<String> {
    let preset = match tier {
        "light" => "fast",
        "deep" => "plan",
        _ => "default",
    };
    preset_effort(features, preset)
        .or_else(|| auto_reasoning_effort_for_preset(preset).map(str::to_owned))
        .or_else(|| auto_reasoning_effort_for_tier(tier).map(str::to_owned))
}

fn parse_decision(text: &str) -> Result<RawAutoTurnDecision, DesktopAutoTurnDecisionError> {
    let trimmed = text.trim();
    let start = trimmed.find('{').ok_or_else(|| {
        DesktopAutoTurnDecisionError::InvalidResponse("missing JSON object".into())
    })?;
    let end = trimmed.rfind('}').ok_or_else(|| {
        DesktopAutoTurnDecisionError::InvalidResponse("incomplete JSON object".into())
    })?;
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|error| DesktopAutoTurnDecisionError::InvalidResponse(error.to_string()))
}

fn normalized_tier(value: Option<&str>) -> Result<&str, DesktopAutoTurnDecisionError> {
    match value.map(str::trim) {
        Some(value @ ("light" | "normal" | "deep")) => Ok(value),
        _ => Err(DesktopAutoTurnDecisionError::InvalidResponse(
            "tier must be light, normal, or deep".into(),
        )),
    }
}

fn normalized_reasoning_effort(value: &str) -> Result<&str, DesktopAutoTurnDecisionError> {
    match value.trim() {
        value @ ("low" | "medium" | "high" | "xhigh" | "max") => Ok(value),
        _ => Err(DesktopAutoTurnDecisionError::InvalidResponse(
            "reasoningEffort must be low, medium, high, xhigh, or max".into(),
        )),
    }
}

fn provider_family(provider_id: &str, model: &str) -> &'static str {
    if provider_id == ANTHROPIC_CREDENTIAL_PROVIDER_ID
        || provider_id.to_ascii_lowercase().contains("anthropic")
        || model.to_ascii_lowercase().contains("claude")
    {
        "anthropic"
    } else {
        debug_assert!(
            provider_id == OPENAI_CREDENTIAL_PROVIDER_ID
                || !provider_id.to_ascii_lowercase().contains("anthropic")
        );
        "openai"
    }
}

fn tier_for_existing_model(model: Option<&str>) -> &'static str {
    let model = model.unwrap_or_default().to_ascii_lowercase();
    if model.contains("mini") || model.contains("haiku") {
        "light"
    } else if model.contains("5.5") || model.contains("opus") {
        "deep"
    } else {
        "normal"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopModelFeatureChatSettings,
        DesktopModelPresetGroup,
    };
    use lilia_contracts::{LiliaAgentWorkflow, TaskId};

    static NEXT_AUTO_TURN_APPLICATION_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let id = NEXT_AUTO_TURN_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-auto-turn:{id}"),
            format!("desktop-auto-turn-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/auto-turn",
                format!("liliacode.auto-turn-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn parses_json_wrapped_by_model_text() {
        let parsed = parse_decision(
            "result:\n```json\n{\"tier\":\"deep\",\"reasoningEffort\":\"high\"}\n```",
        )
        .unwrap();
        assert_eq!(parsed.tier.as_deref(), Some("deep"));
        assert_eq!(parsed.reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn provider_family_uses_provider_and_model_identity() {
        assert_eq!(
            provider_family(ANTHROPIC_CREDENTIAL_PROVIDER_ID, "model"),
            "anthropic"
        );
        assert_eq!(provider_family("custom", "claude-sonnet-4-6"), "anthropic");
        assert_eq!(
            provider_family(OPENAI_CREDENTIAL_PROVIDER_ID, "gpt-5.4"),
            "openai"
        );
    }

    #[test]
    fn deterministic_router_applies_configured_role_presets() {
        let features = DesktopModelFeatureSettings {
            chat: DesktopModelFeatureChatSettings {
                light: Some("chat-light".into()),
                normal: Some("chat-normal".into()),
                deep: Some("chat-deep".into()),
            },
            presets: vec![DesktopModelPresetGroup {
                id: "review".into(),
                label: "Review".into(),
                kind: "builtin".into(),
                model: Some("review-model".into()),
                reasoning_effort: Some("xhigh".into()),
                enabled: true,
            }],
            ..DesktopModelFeatureSettings::default()
        };
        let mut request = DesktopTurnRequest::new(TaskId::new("review-task").unwrap(), "review");
        request.workflow = Some(LiliaAgentWorkflow::LiliaReview {
            target: lilia_contracts::LiliaReviewTarget::UncommittedChanges,
            instructions: None,
            delivery: None,
        });

        let selected = apply_local_preset_selection(request, &features, None);

        assert_eq!(selected.model.as_deref(), Some("review-model"));
        assert_eq!(selected.reasoning_effort.as_deref(), Some("xhigh"));
        assert!(selected.auto_turn_decision_applied);
        let selection = selected.automatic_selection.unwrap();
        assert_eq!(selection.tier, "deep");
        assert_eq!(selection.decision_provider_id, "deterministic");
        assert!(selection
            .signals
            .iter()
            .any(|signal| signal == "工作流 lilia_review"));
    }

    #[test]
    fn model_decision_uses_feature_override_and_preset_output() {
        let features = DesktopModelFeatureSettings {
            auto_turn_decision: Some("decision-model".into()),
            presets: vec![DesktopModelPresetGroup {
                id: "plan".into(),
                label: "Plan".into(),
                kind: "builtin".into(),
                model: Some("configured-deep".into()),
                reasoning_effort: Some("max".into()),
                enabled: true,
            }],
            ..DesktopModelFeatureSettings::default()
        };

        assert_eq!(
            features.auto_turn_decision.as_deref(),
            Some("decision-model")
        );
        assert_eq!(
            model_for_tier(&features, "openai", "deep").as_deref(),
            Some("configured-deep")
        );
        assert_eq!(effort_for_tier(&features, "deep").as_deref(), Some("max"));
    }

    #[test]
    fn explicit_composer_selection_bypasses_automatic_routing() {
        let mut request = DesktopTurnRequest::new(TaskId::new("manual-model-task").unwrap(), "fix");
        request.model = Some("  gpt-manual  ".into());
        request.reasoning_effort = Some(" high ".into());
        request.plan_mode = true;
        normalize_explicit_selection(&mut request).unwrap();

        let selected = apply_explicit_selection(request);

        assert_eq!(selected.model.as_deref(), Some("gpt-manual"));
        assert_eq!(selected.reasoning_effort.as_deref(), Some("high"));
        assert!(selected.auto_turn_decision_applied);
        let decision = selected.automatic_selection.unwrap();
        assert_eq!(decision.source, "manual");
        assert_eq!(decision.decision_provider_id, "manual");
        assert!(decision.plan_mode);
    }

    #[test]
    fn application_accepts_manual_selection_without_auto_settings_or_model_call() {
        let application = application();
        let mut request =
            DesktopTurnRequest::new(TaskId::new("manual-application-task").unwrap(), "fix");
        request.model = Some("gpt-manual".into());
        request.reasoning_effort = Some("high".into());
        request.allow_auto_turn_decision = true;

        let selected = application.apply_automatic_turn_selection(request).unwrap();

        assert_eq!(selected.model.as_deref(), Some("gpt-manual"));
        assert_eq!(selected.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(
            selected
                .automatic_selection
                .as_ref()
                .map(|value| value.source.as_str()),
            Some("manual")
        );
    }
}
