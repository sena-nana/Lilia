use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime};

use super::codex_spark::{codex_account_spark_enabled, request_codex_account_spark};
use super::config::{assistant_ai_secret, load_assistant_ai_config, load_model_feature_settings};
use super::credentials::normalize_secret;
use super::types::{
    AssistantAIConfig, AssistantAIModelPoolItem, AssistantAIModelsResult, AssistantAITestResult,
};
use crate::chat::types::{ChatAttachment, ChatConversationReference, ChatWorkflow};
use crate::prompt_contract;
use crate::BACKEND_CODEX;

const PROMPT_OPTIMIZE_TIMEOUT: Duration = Duration::from_secs(12);
const PROMPT_ROUTE_CONFIDENCE_THRESHOLD: f64 = 0.6;
const GENERAL_TASK_SCENARIO: &str = "general_task_optimize";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptOptimizeInput {
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub(crate) conversation_references: Vec<ChatConversationReference>,
    #[serde(default)]
    pub(crate) project_cwd: Option<String>,
    #[serde(default)]
    pub(crate) task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptOptimizeResult {
    pub(crate) optimized_prompt: String,
    pub(crate) route: PromptRoute,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptRoute {
    pub(crate) scenario: String,
    pub(crate) workflow: Option<ChatWorkflow>,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
    pub(crate) signals: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPromptRoute {
    scenario: Option<String>,
    workflow: Option<JsonValue>,
    confidence: Option<f64>,
    reason: Option<String>,
    signals: Option<Vec<String>>,
}

pub(crate) fn test_connection(mut config: AssistantAIConfig) -> AssistantAITestResult {
    if config
        .api_key
        .as_deref()
        .and_then(normalize_secret)
        .is_none()
    {
        config.api_key = assistant_ai_secret().ok().flatten();
    }
    let base = config
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');
    let key = config.api_key.as_deref().unwrap_or("").trim();
    let model = config.model.as_deref().unwrap_or("").trim();
    if base.is_empty() || key.is_empty() || model.is_empty() {
        return AssistantAITestResult {
            ok: false,
            error: Some("baseUrl / apiKey / model 必须全部填写".into()),
            models: None,
            model_matched: None,
        };
    }
    let url = format!("{base}/models");
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return AssistantAITestResult {
                ok: false,
                error: Some(format!("HTTP 客户端构造失败：{e}")),
                models: None,
                model_matched: None,
            }
        }
    };
    match client.get(&url).bearer_auth(key).send() {
        Ok(resp) if resp.status().is_success() => {
            let parsed: Option<Vec<String>> = resp
                .json::<JsonValue>()
                .ok()
                .and_then(|v| v.get("data").cloned())
                .and_then(|d| d.as_array().cloned())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                        .collect()
                });
            let matched = parsed.as_ref().map(|list| list.iter().any(|m| m == model));
            AssistantAITestResult {
                ok: true,
                error: None,
                models: parsed,
                model_matched: matched,
            }
        }
        Ok(resp) => AssistantAITestResult {
            ok: false,
            error: Some(format!("HTTP {} from {url}", resp.status())),
            models: None,
            model_matched: None,
        },
        Err(e) => AssistantAITestResult {
            ok: false,
            error: Some(format!("请求失败：{e}")),
            models: None,
            model_matched: None,
        },
    }
}

pub(crate) fn fetch_models(mut config: AssistantAIConfig) -> AssistantAIModelsResult {
    if config
        .api_key
        .as_deref()
        .and_then(normalize_secret)
        .is_none()
    {
        config.api_key = assistant_ai_secret().ok().flatten();
    }
    let base = config
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_end_matches('/');
    let key = config.api_key.as_deref().unwrap_or("").trim();
    if base.is_empty() || key.is_empty() {
        return AssistantAIModelsResult {
            ok: false,
            error: Some("baseUrl / apiKey 必须填写".into()),
            models: Vec::new(),
        };
    }
    let url = format!("{base}/models");
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return AssistantAIModelsResult {
                ok: false,
                error: Some(format!("HTTP 客户端构造失败：{e}")),
                models: Vec::new(),
            }
        }
    };
    match client.get(&url).bearer_auth(key).send() {
        Ok(resp) if resp.status().is_success() => {
            let models = resp
                .json::<JsonValue>()
                .ok()
                .and_then(|v| v.get("data").cloned())
                .and_then(|d| d.as_array().cloned())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|i| i.as_str()))
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(|id| AssistantAIModelPoolItem {
                            id: id.to_string(),
                            label: id.to_string(),
                            source: "remote".to_string(),
                            backend: BACKEND_CODEX.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            AssistantAIModelsResult {
                ok: true,
                error: None,
                models,
            }
        }
        Ok(resp) => AssistantAIModelsResult {
            ok: false,
            error: Some(format!("HTTP {} from {url}", resp.status())),
            models: Vec::new(),
        },
        Err(e) => AssistantAIModelsResult {
            ok: false,
            error: Some(format!("请求失败：{e}")),
            models: Vec::new(),
        },
    }
}

pub(crate) fn optimize_prompt<R: Runtime>(
    app: AppHandle<R>,
    input: PromptOptimizeInput,
) -> Result<PromptOptimizeResult, String> {
    let prompt = input.prompt.trim();
    if prompt.is_empty() {
        return Err("提示词为空".to_string());
    }
    let model_features = load_model_feature_settings(&app);
    let route_request = build_prompt_route_request(prompt, &input);
    let route_text = request_assistant_text(
        &app,
        &route_request,
        prompt_contract::prompt_router_system_instruction(),
        700,
        "Prompt Router",
        model_features.prompt_router.clone(),
    )?;
    let route = normalize_prompt_route(&route_text)?;

    let optimize_request = build_prompt_optimize_request(prompt, &input, &route);
    let optimized_text = request_assistant_text(
        &app,
        &optimize_request,
        prompt_contract::prompt_optimize_system_instruction(),
        900,
        "提示词优化",
        model_features.prompt_optimize,
    )?;
    Ok(PromptOptimizeResult {
        optimized_prompt: normalize_optimized_prompt(&optimized_text)?,
        route,
    })
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

fn request_assistant_text<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    system_instruction: &str,
    max_tokens: u32,
    label: &str,
    override_model: Option<String>,
) -> Result<String, String> {
    if codex_account_spark_enabled(app) {
        return request_codex_account_spark(app, prompt, system_instruction)
            .map_err(|err| format!("{label}失败：{err}"));
    }
    let model = assistant_ai_model_request(app, override_model)?;
    request_openai_compatible(&model, prompt, system_instruction, max_tokens)
}

fn request_openai_compatible(
    model: &AssistantAIConfig,
    prompt: &str,
    system_instruction: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let base_url = model
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let url = format!("{base_url}/chat/completions");
    let client = Client::builder()
        .timeout(PROMPT_OPTIMIZE_TIMEOUT)
        .build()
        .map_err(|err| format!("辅助模型 HTTP 客户端构造失败：{err}"))?;
    let resp = client
        .post(url)
        .bearer_auth(model.api_key.as_deref().unwrap_or(""))
        .json(&json!({
            "model": model.model,
            "messages": [
                { "role": "system", "content": system_instruction },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.2,
            "max_tokens": max_tokens
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

fn prompt_context_json(input: &PromptOptimizeInput) -> (Vec<JsonValue>, Vec<JsonValue>) {
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
        .collect::<Vec<_>>();
    let conversation_references = input
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
        .collect::<Vec<_>>();
    (attachments, conversation_references)
}

fn build_prompt_route_request(prompt: &str, input: &PromptOptimizeInput) -> String {
    let (attachments, conversation_references) = prompt_context_json(input);
    json!({
        "instruction": prompt_contract::prompt_router_request_instruction(),
        "originalPrompt": prompt,
        "attachments": attachments,
        "conversationReferences": conversation_references,
        "projectCwd": input.project_cwd,
        "taskId": input.task_id,
        "scenarios": prompt_contract::prompt_router_scenarios(),
        "requirements": prompt_contract::prompt_router_requirements()
    })
    .to_string()
}

fn build_prompt_optimize_request(
    prompt: &str,
    input: &PromptOptimizeInput,
    route: &PromptRoute,
) -> String {
    let (attachments, conversation_references) = prompt_context_json(input);
    json!({
        "instruction": prompt_contract::prompt_optimize_request_instruction(),
        "originalPrompt": prompt,
        "route": route,
        "attachments": attachments,
        "conversationReferences": conversation_references,
        "requirements": prompt_contract::prompt_optimize_requirements()
    })
    .to_string()
}

fn extract_json_object(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Ok(trimmed.to_string());
    }
    let Some(start) = trimmed.find('{') else {
        return Err("Prompt Router 未返回 JSON 对象".to_string());
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err("Prompt Router 未返回完整 JSON 对象".to_string());
    };
    Ok(trimmed[start..=end].to_string())
}

fn normalize_prompt_route(text: &str) -> Result<PromptRoute, String> {
    Ok(normalize_raw_prompt_route(
        serde_json::from_str::<RawPromptRoute>(&extract_json_object(text)?)
            .map_err(|err| format!("Prompt Router JSON 解析失败：{err}"))?,
    ))
}

fn normalize_raw_prompt_route(raw: RawPromptRoute) -> PromptRoute {
    let mut scenario = raw
        .scenario
        .as_deref()
        .map(str::trim)
        .filter(|value| {
            prompt_contract::prompt_router_scenarios()
                .iter()
                .any(|item| item == value)
        })
        .unwrap_or(GENERAL_TASK_SCENARIO)
        .to_string();
    let mut confidence = raw.confidence.unwrap_or(0.0).clamp(0.0, 1.0);
    if confidence < PROMPT_ROUTE_CONFIDENCE_THRESHOLD {
        scenario = GENERAL_TASK_SCENARIO.to_string();
        confidence = confidence.min(0.5);
    }
    let workflow = raw
        .workflow
        .and_then(|value| serde_json::from_value::<ChatWorkflow>(value).ok())
        .filter(|workflow| prompt_route_workflow_matches(&scenario, workflow));
    PromptRoute {
        scenario,
        workflow,
        confidence,
        reason: raw.reason.unwrap_or_default().trim().to_string(),
        signals: raw.signals.unwrap_or_default(),
    }
}

fn prompt_route_workflow_matches(scenario: &str, workflow: &ChatWorkflow) -> bool {
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
        ("review", ChatWorkflow::LiliaReview { .. })
            | ("fix_suggestion", ChatWorkflow::LiliaFixSuggestion { .. })
            | ("batch_apply", ChatWorkflow::LiliaBatchApply { .. })
            | ("context_compact", ChatWorkflow::LiliaCompact)
            | (
                "config_diagnostics",
                ChatWorkflow::LiliaConfigDiagnostics { .. }
            )
            | ("goal_update", ChatWorkflow::LiliaGoal { .. })
    ) || matches!(
        workflow,
        ChatWorkflow::LiliaTaskWorkflow { kind, .. } if task_workflow_matches(scenario, kind)
    )
}

fn normalize_optimized_prompt(text: &str) -> Result<String, String> {
    let trimmed = text.trim().trim_matches('`').trim();
    if trimmed.is_empty() {
        return Err("辅助模型返回空提示词".to_string());
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Err("辅助模型返回了非文本结果".to_string());
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_ai_requires_base_url_key_and_model() {
        let result = test_connection(AssistantAIConfig {
            base_url: Some("https://example.com/v1".to_string()),
            api_key: None,
            model: Some("model".to_string()),
            model_pool: Vec::new(),
            codex_account_spark_enabled: false,
            has_api_key: false,
            clear_api_key: false,
        });

        assert!(!result.ok);
        assert_eq!(result.models, None);
        assert_eq!(result.model_matched, None);
        assert!(result.error.unwrap().contains("必须全部填写"));
    }

    #[test]
    fn normalize_optimized_prompt_rejects_empty_text() {
        let err = normalize_optimized_prompt("  \n ").expect_err("empty output should fail");
        assert!(err.contains("空提示词"));
    }

    #[test]
    fn normalize_optimized_prompt_rejects_json_result() {
        let err = normalize_optimized_prompt("{\"prompt\":\"ok\"}").expect_err("json should fail");
        assert!(err.contains("非文本结果"));
    }

    #[test]
    fn build_prompt_optimize_request_keeps_scope_context() {
        let input = PromptOptimizeInput {
            prompt: "修一下输入框按钮".to_string(),
            attachments: vec![ChatAttachment {
                id: "att-1".to_string(),
                name: "ChatComposer.vue".to_string(),
                path: "apps/desktop/src/components/chat/ChatComposer.vue".to_string(),
                kind: "file".to_string(),
                size: Some(42),
                exists: true,
                mime: None,
                directory: None,
            }],
            conversation_references: vec![ChatConversationReference {
                task_id: "task-1".to_string(),
                title: "旧对话".to_string(),
                route: "/projects/lilia/tasks/task-1".to_string(),
                project_id: Some("lilia".to_string()),
                project_name: Some("Lilia".to_string()),
            }],
            project_cwd: Some("C:\\Files\\workspace\\Lilia".to_string()),
            task_id: Some("task-current".to_string()),
        };
        let route = PromptRoute {
            scenario: GENERAL_TASK_SCENARIO.to_string(),
            workflow: None,
            confidence: 0.4,
            reason: "fallback".to_string(),
            signals: vec!["unclear".to_string()],
        };
        let route_prompt = build_prompt_route_request(&input.prompt, &input);
        let prompt = build_prompt_optimize_request(&input.prompt, &input, &route);

        assert!(route_prompt.contains("task-current"));
        assert!(route_prompt.contains("C:\\\\Files\\\\workspace\\\\Lilia"));
        assert!(prompt.contains("ChatComposer.vue"));
        assert!(prompt.contains("旧对话"));
        assert!(prompt.contains(GENERAL_TASK_SCENARIO));
    }

    #[test]
    fn prompt_router_low_confidence_falls_back_without_workflow() {
        let route = normalize_raw_prompt_route(RawPromptRoute {
            scenario: Some("review".to_string()),
            workflow: Some(json!({
                "type": "lilia_review",
                "target": { "type": "uncommittedChanges" },
            })),
            confidence: Some(0.3),
            reason: Some("unclear".to_string()),
            signals: Some(vec!["weak".to_string()]),
        });

        assert_eq!(route.scenario, GENERAL_TASK_SCENARIO);
        assert!(route.workflow.is_none());
        assert!(route.confidence <= 0.5);
    }

    #[test]
    fn prompt_router_keeps_matching_workflow() {
        let route = normalize_raw_prompt_route(RawPromptRoute {
            scenario: Some("review".to_string()),
            workflow: Some(json!({
                "type": "lilia_review",
                "target": { "type": "uncommittedChanges" },
                "delivery": "inline",
            })),
            confidence: Some(0.9),
            reason: Some("review requested".to_string()),
            signals: Some(vec!["review".to_string()]),
        });

        assert_eq!(route.scenario, "review");
        assert!(matches!(
            route.workflow,
            Some(ChatWorkflow::LiliaReview { .. })
        ));

        let route = normalize_raw_prompt_route(RawPromptRoute {
            scenario: Some("frontend".to_string()),
            workflow: Some(json!({
                "type": "lilia_task_workflow",
                "kind": "frontend",
            })),
            confidence: Some(0.9),
            reason: Some("frontend requested".to_string()),
            signals: Some(vec!["ui".to_string()]),
        });

        assert_eq!(route.scenario, "frontend");
        assert!(matches!(
            route.workflow,
            Some(ChatWorkflow::LiliaTaskWorkflow { .. })
        ));
    }
}
