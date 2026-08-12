use lilia_agent_integration::{NativeControlModelRequest, NativeControlModelResult};
use lilia_contracts::{
    auto_model_for_provider_family_tier, auto_reasoning_effort_for_tier,
    auto_turn_decision_request_instruction, auto_turn_decision_system_instruction,
    auto_turn_decision_tier_policy,
};
use mutsuki_agent_contracts::{ANTHROPIC_CREDENTIAL_PROVIDER_ID, OPENAI_CREDENTIAL_PROVIDER_ID};
use serde::Deserialize;
use serde_json::json;

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopAutoTurnDecisionSettings,
    DesktopAutomaticTurnSelection, DesktopTurnRequest,
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
        let settings = request
            .auto_turn_settings
            .clone()
            .ok_or(DesktopAutoTurnDecisionError::MissingSettings)?;
        if !settings.enabled {
            request.auto_turn_decision_applied = true;
            return Ok(request);
        }

        let context_usage = self.task_context_usage(&request.task_id)?;
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
                max_output_tokens: 600,
                reasoning: Some("low".into()),
            })
            .map_err(|error| DesktopAutoTurnDecisionError::Model(error.to_string()))?;
        let raw = parse_decision(&generated.text)?;
        apply_decision(&mut request, &settings, raw, generated, has_session)?;
        request.auto_turn_decision_applied = true;
        Ok(request)
    }
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
        auto_model_for_provider_family_tier(family, &tier).map(str::to_owned)
    } else {
        request.model.clone()
    };
    let reasoning_effort = if settings.allow_reasoning_effort {
        match raw.reasoning_effort.as_deref() {
            Some(value) => Some(normalized_reasoning_effort(value)?.to_owned()),
            None => auto_reasoning_effort_for_tier(&tier).map(str::to_owned),
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
    use super::*;

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
}
