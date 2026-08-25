use std::time::Duration;

use lilia_contracts::{
    prompt_optimize_request_instruction, prompt_optimize_requirements,
    prompt_optimize_system_instruction, prompt_router_request_instruction,
    prompt_router_requirements, prompt_router_scenarios, prompt_router_system_instruction,
    LiliaAgentWorkflow,
};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::application::auxiliary_model::{request_auxiliary_model_text, DesktopAuxiliaryModelRequest};
use crate::application::{DesktopApplication, DesktopApplicationError, ASSISTANT_AI_CREDENTIAL_KEY};

const PROMPT_OPTIMIZE_TIMEOUT: Duration = Duration::from_secs(12);
const PROMPT_ROUTE_CONFIDENCE_THRESHOLD: f64 = 0.6;
const GENERAL_TASK_SCENARIO: &str = "general_task_optimize";

pub use lilia_feature_composer::{
    PromptOptimizeInput as DesktopPromptOptimizeInput,
    PromptOptimizeResult as DesktopPromptOptimizeResult, PromptRoute as DesktopPromptRoute,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPromptRoute {
    scenario: Option<String>,
    workflow: Option<JsonValue>,
    confidence: Option<f64>,
    reason: Option<String>,
    signals: Option<Vec<String>>,
}

impl DesktopApplication {
    pub fn optimize_prompt(
        &self,
        input: DesktopPromptOptimizeInput,
    ) -> Result<DesktopPromptOptimizeResult, DesktopApplicationError> {
        let prompt = input.prompt.trim();
        if prompt.is_empty() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "prompt",
                message: "prompt must not be empty".into(),
            });
        }
        let features = self.model_feature_settings()?;
        let route_model = self.resolve_auxiliary_model(features.prompt_router)?;
        let route_text = request_auxiliary_model_text(
            &route_model,
            prompt_router_system_instruction(),
            &build_prompt_route_request(prompt, &input),
            700,
            PROMPT_OPTIMIZE_TIMEOUT,
            "prompt router",
        )
        .map_err(DesktopApplicationError::Agent)?;
        let route = normalize_prompt_route(&route_text).map_err(DesktopApplicationError::Agent)?;

        let optimize_model = self.resolve_auxiliary_model(features.prompt_optimize)?;
        let optimized_text = request_auxiliary_model_text(
            &optimize_model,
            prompt_optimize_system_instruction(),
            &build_prompt_optimize_request(prompt, &input, &route),
            900,
            PROMPT_OPTIMIZE_TIMEOUT,
            "prompt optimize",
        )
        .map_err(DesktopApplicationError::Agent)?;
        Ok(DesktopPromptOptimizeResult {
            optimized_prompt: normalize_optimized_prompt(&optimized_text)
                .map_err(DesktopApplicationError::Agent)?,
            route,
        })
    }

    fn resolve_auxiliary_model(
        &self,
        override_model: Option<String>,
    ) -> Result<DesktopAuxiliaryModelRequest, DesktopApplicationError> {
        let assistant = self.assistant_ai_settings()?;
        let base_url = assistant
            .base_url
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty());
        let model = override_model
            .or(assistant.model)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let api_key = self
            .read_host_credential_text_result(ASSISTANT_AI_CREDENTIAL_KEY)?
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        match (base_url, model, api_key) {
            (Some(base_url), Some(model), Some(api_key)) => Ok(DesktopAuxiliaryModelRequest {
                base_url,
                model,
                api_key,
            }),
            _ => Err(DesktopApplicationError::InvalidInput {
                field: "assistant_ai",
                message: "assistant AI base URL, API key, and model are required".into(),
            }),
        }
    }
}

fn prompt_context_json(input: &DesktopPromptOptimizeInput) -> (Vec<JsonValue>, Vec<JsonValue>) {
    let attachments = input
        .attachments
        .iter()
        .take(8)
        .map(|item| {
            json!({
                "name": item.name,
                "path": item.path,
                "kind": item.kind,
                "size": item.size,
            })
        })
        .collect();
    let references = input
        .conversation_references
        .iter()
        .take(8)
        .map(|item| {
            json!({
                "taskId": item.task_id,
                "title": item.title,
                "projectName": item.project_name,
            })
        })
        .collect();
    (attachments, references)
}

fn build_prompt_route_request(prompt: &str, input: &DesktopPromptOptimizeInput) -> String {
    let (attachments, conversation_references) = prompt_context_json(input);
    json!({
        "instruction": prompt_router_request_instruction(),
        "originalPrompt": prompt,
        "attachments": attachments,
        "conversationReferences": conversation_references,
        "projectCwd": input.project_cwd,
        "taskId": input.task_id,
        "scenarios": prompt_router_scenarios(),
        "requirements": prompt_router_requirements(),
    })
    .to_string()
}

fn build_prompt_optimize_request(
    prompt: &str,
    input: &DesktopPromptOptimizeInput,
    route: &DesktopPromptRoute,
) -> String {
    let (attachments, conversation_references) = prompt_context_json(input);
    json!({
        "instruction": prompt_optimize_request_instruction(),
        "originalPrompt": prompt,
        "route": route,
        "attachments": attachments,
        "conversationReferences": conversation_references,
        "requirements": prompt_optimize_requirements(),
    })
    .to_string()
}

fn extract_json_object(text: &str) -> Result<&str, String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed);
    }
    let start = trimmed
        .find('{')
        .ok_or_else(|| "prompt router did not return a JSON object".to_owned())?;
    let end = trimmed
        .rfind('}')
        .ok_or_else(|| "prompt router returned an incomplete JSON object".to_owned())?;
    Ok(&trimmed[start..=end])
}

fn normalize_prompt_route(text: &str) -> Result<DesktopPromptRoute, String> {
    let raw = serde_json::from_str::<RawPromptRoute>(extract_json_object(text)?)
        .map_err(|error| format!("prompt router JSON is invalid: {error}"))?;
    Ok(normalize_raw_prompt_route(raw))
}

fn normalize_raw_prompt_route(raw: RawPromptRoute) -> DesktopPromptRoute {
    let mut scenario = raw
        .scenario
        .as_deref()
        .map(str::trim)
        .filter(|value| prompt_router_scenarios().iter().any(|item| item == value))
        .unwrap_or(GENERAL_TASK_SCENARIO)
        .to_owned();
    let mut confidence = raw.confidence.unwrap_or(0.0).clamp(0.0, 1.0);
    if confidence < PROMPT_ROUTE_CONFIDENCE_THRESHOLD {
        scenario = GENERAL_TASK_SCENARIO.to_owned();
        confidence = confidence.min(0.5);
    }
    let workflow = raw
        .workflow
        .and_then(|value| serde_json::from_value::<LiliaAgentWorkflow>(value).ok())
        .filter(|workflow| prompt_route_workflow_matches(&scenario, workflow));
    DesktopPromptRoute {
        scenario,
        workflow,
        confidence,
        reason: raw.reason.unwrap_or_default().trim().to_owned(),
        signals: raw
            .signals
            .unwrap_or_default()
            .into_iter()
            .map(|signal| signal.trim().to_owned())
            .filter(|signal| !signal.is_empty())
            .take(8)
            .collect(),
    }
}

fn prompt_route_workflow_matches(scenario: &str, workflow: &LiliaAgentWorkflow) -> bool {
    fn task_workflow_matches(scenario: &str, kind: &str) -> bool {
        matches!(
            (scenario, kind),
            ("bug_localization", "bugLocalization")
                | ("frontend", "frontend")
                | ("refactor", "refactor")
                | ("test_verification", "testAndVerification")
                | ("docs_prompt", "docsAndPrompt")
                | ("git_release", "gitAndRelease")
                | ("architecture_memory", "architectureAndMemory")
                | ("general_task_optimize", "generalTask")
        )
    }

    matches!(
        (scenario, workflow),
        ("review", LiliaAgentWorkflow::LiliaReview { .. })
            | (
                "fix_suggestion",
                LiliaAgentWorkflow::LiliaFixSuggestion { .. }
            )
            | ("batch_apply", LiliaAgentWorkflow::LiliaBatchApply { .. })
            | ("context_compact", LiliaAgentWorkflow::LiliaCompact)
            | (
                "config_diagnostics",
                LiliaAgentWorkflow::LiliaConfigDiagnostics { .. }
            )
            | ("goal_update", LiliaAgentWorkflow::LiliaGoal { .. })
    ) || matches!(
        workflow,
        LiliaAgentWorkflow::LiliaTaskWorkflow { kind, .. }
            if task_workflow_matches(scenario, kind)
    )
}

fn normalize_optimized_prompt(text: &str) -> Result<String, String> {
    let trimmed = text.trim().trim_matches('`').trim();
    if trimmed.is_empty() {
        return Err("assistant AI returned an empty prompt".into());
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Err("assistant AI returned a non-text prompt".into());
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_confidence_route_falls_back_without_workflow() {
        let route = normalize_raw_prompt_route(RawPromptRoute {
            scenario: Some("review".into()),
            workflow: Some(json!({
                "type": "lilia_review",
                "target": { "type": "uncommittedChanges" },
            })),
            confidence: Some(0.3),
            reason: Some("unclear".into()),
            signals: Some(vec![" weak ".into(), "".into()]),
        });

        assert_eq!(route.scenario, GENERAL_TASK_SCENARIO);
        assert!(route.workflow.is_none());
        assert_eq!(route.signals, ["weak"]);
    }

    #[test]
    fn matching_route_keeps_typed_workflow() {
        let route = normalize_prompt_route(
            "```json\n{\"scenario\":\"frontend\",\"workflow\":{\"type\":\"lilia_task_workflow\",\"kind\":\"frontend\"},\"confidence\":0.9}\n```",
        )
        .unwrap();

        assert!(matches!(
            route.workflow,
            Some(LiliaAgentWorkflow::LiliaTaskWorkflow { ref kind, .. }) if kind == "frontend"
        ));
    }

    #[test]
    fn optimize_prompt_rejects_non_text_output() {
        assert!(normalize_optimized_prompt("{\"prompt\":\"ok\"}").is_err());
        assert!(normalize_optimized_prompt("  ").is_err());
    }
}
