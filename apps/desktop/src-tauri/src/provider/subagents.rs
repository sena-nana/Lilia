use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
#[cfg(any(feature = "legacy-runner", test))]
use serde_json::{Map as JsonMap, Value as JsonValue};
use uuid::Uuid;

#[cfg(any(feature = "legacy-runner", test))]
use crate::prompt_contract::codex_subagent_prompts;
use crate::store::resolve_lilia_home;

use super::types::CustomSubagentDefinition;

const SUBAGENTS_CONFIG_FILE: &str = "subagents.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CustomSubagentUpsertInput {
    #[serde(default)]
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
    pub(crate) instruction: String,
    #[serde(default)]
    pub(crate) enabled: Option<bool>,
}

fn subagents_config_path() -> PathBuf {
    resolve_lilia_home()
        .join("config")
        .join(SUBAGENTS_CONFIG_FILE)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_required_text(value: String, label: &str) -> Result<String, String> {
    normalize_optional_text(Some(value)).ok_or_else(|| format!("{label} 不能为空"))
}

fn load_custom_subagents_from_path(path: &Path) -> Result<Vec<CustomSubagentDefinition>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    serde_json::from_str::<Vec<CustomSubagentDefinition>>(&text)
        .map_err(|e| format!("解析 {} 失败：{e}", path.display()))?
        .into_iter()
        .map(normalize_custom_subagent_definition)
        .collect()
}

fn save_custom_subagents_to_path(
    path: &Path,
    subagents: &[CustomSubagentDefinition],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(subagents)
        .map_err(|e| format!("序列化 subagents 失败：{e}"))?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}

fn normalize_custom_subagent_definition(
    definition: CustomSubagentDefinition,
) -> Result<CustomSubagentDefinition, String> {
    let id = normalize_required_text(definition.id, "Agent ID")?;
    let name = normalize_required_text(definition.name, "Agent 名称")?;
    let instruction = normalize_required_text(definition.instruction, "Agent 职责说明")?;
    Ok(CustomSubagentDefinition {
        id,
        name,
        description: normalize_optional_text(Some(definition.description)).unwrap_or_default(),
        instruction,
        enabled: definition.enabled,
    })
}

fn ensure_unique_subagent_names(subagents: &[CustomSubagentDefinition]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for item in subagents {
        if !seen.insert(item.name.as_str()) {
            return Err(format!("已存在同名 Agent：{}", item.name));
        }
    }
    Ok(())
}

#[cfg(any(feature = "legacy-runner", test))]
fn slugify_agent_name(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' || ch.is_whitespace() {
            Some('-')
        } else {
            None
        };
        let Some(next) = next else { continue };
        if next == '-' {
            if last_dash || slug.is_empty() {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        slug.push(next);
    }
    slug.trim_matches('-').to_string()
}

#[cfg(any(feature = "legacy-runner", test))]
fn compose_lilia_subagent_prompt(instruction: &str) -> String {
    let prompts = codex_subagent_prompts();
    format!(
        "{}\n\n{}\n{}",
        prompts.baseline_prompt, prompts.agent_role_label, instruction
    )
}

#[cfg(any(feature = "legacy-runner", test))]
fn compile_codex_subagent_instructions(subagents: &[CustomSubagentDefinition]) -> Option<String> {
    if subagents.is_empty() {
        return None;
    }
    let prompts = codex_subagent_prompts();
    let mut lines = vec![
        prompts.delegation_header.clone(),
        prompts.baseline_intro.clone(),
        prompts.baseline_label.clone(),
        prompts.baseline_prompt.clone(),
        prompts.available_label.clone(),
    ];
    for item in subagents {
        let summary = if item.description.is_empty() {
            item.instruction.as_str()
        } else {
            item.description.as_str()
        };
        lines.push(format!("- {}: {}", item.name, summary));
        lines.push(format!(
            "  {}: {}",
            prompts.agent_specific_instruction_label, item.instruction
        ));
    }
    lines.push(prompts.delegation_rules_label.clone());
    lines.extend(
        prompts
            .delegation_rules
            .iter()
            .enumerate()
            .map(|(index, rule)| format!("{}. {}", index + 1, rule)),
    );
    Some(lines.join("\n"))
}

#[cfg(any(feature = "legacy-runner", test))]
fn compile_claude_managed_subagents(subagents: &[CustomSubagentDefinition]) -> Option<JsonValue> {
    if subagents.is_empty() {
        return None;
    }
    let mut agents = JsonMap::new();
    for (index, item) in subagents.iter().enumerate() {
        let base = slugify_agent_name(&item.name);
        let key = if base.is_empty() {
            format!("lilia-subagent-{}", index + 1)
        } else {
            format!("lilia-{}", base)
        };
        agents.insert(
            key,
            serde_json::json!({
                "description": if item.description.is_empty() {
                    item.name.clone()
                } else {
                    format!("{}: {}", item.name, item.description)
                },
                "prompt": compose_lilia_subagent_prompt(&item.instruction),
            }),
        );
    }
    Some(JsonValue::Object(
        [("agents".to_string(), JsonValue::Object(agents))]
            .into_iter()
            .collect(),
    ))
}

pub(crate) fn load_custom_subagents() -> Result<Vec<CustomSubagentDefinition>, String> {
    let subagents = load_custom_subagents_from_path(&subagents_config_path())?;
    ensure_unique_subagent_names(&subagents)?;
    Ok(subagents)
}

#[cfg(feature = "legacy-runner")]
pub(crate) fn enabled_custom_subagents() -> Result<Vec<CustomSubagentDefinition>, String> {
    Ok(load_custom_subagents()?
        .into_iter()
        .filter(|item| item.enabled)
        .collect())
}

pub(crate) fn upsert_custom_subagent(
    input: CustomSubagentUpsertInput,
) -> Result<CustomSubagentDefinition, String> {
    let path = subagents_config_path();
    let mut subagents = load_custom_subagents_from_path(&path)?;
    let id = normalize_optional_text(input.id).unwrap_or_else(|| Uuid::new_v4().to_string());
    let next = CustomSubagentDefinition {
        id: id.clone(),
        name: normalize_required_text(input.name, "Agent 名称")?,
        description: normalize_optional_text(input.description).unwrap_or_default(),
        instruction: normalize_required_text(input.instruction, "Agent 职责说明")?,
        enabled: input.enabled.unwrap_or(true),
    };
    if let Some(index) = subagents.iter().position(|item| item.id == id) {
        subagents[index] = next.clone();
    } else {
        subagents.push(next.clone());
    }
    ensure_unique_subagent_names(&subagents)?;
    save_custom_subagents_to_path(&path, &subagents)?;
    Ok(next)
}

pub(crate) fn delete_custom_subagent(id: &str) -> Result<(), String> {
    let id = normalize_required_text(id.to_string(), "Agent ID")?;
    let path = subagents_config_path();
    let mut subagents = load_custom_subagents_from_path(&path)?;
    let original_len = subagents.len();
    subagents.retain(|item| item.id != id);
    if subagents.len() == original_len {
        return Err("未找到要删除的 Agent".to_string());
    }
    save_custom_subagents_to_path(&path, &subagents)
}

#[cfg(feature = "legacy-runner")]
pub(crate) fn codex_subagent_instructions() -> Result<Option<String>, String> {
    Ok(compile_codex_subagent_instructions(
        &enabled_custom_subagents()?,
    ))
}

#[cfg(feature = "legacy-runner")]
pub(crate) fn claude_managed_subagents() -> Result<Option<JsonValue>, String> {
    Ok(compile_claude_managed_subagents(
        &enabled_custom_subagents()?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("lilia-subagents-{name}-{}.json", Uuid::new_v4()))
    }

    #[test]
    fn load_save_and_delete_subagents_roundtrip() {
        let path = temp_path("roundtrip");
        let item = CustomSubagentDefinition {
            id: "agent-1".to_string(),
            name: "Reviewer".to_string(),
            description: "Check diffs".to_string(),
            instruction: "Review code changes and summarize risk.".to_string(),
            enabled: true,
        };
        save_custom_subagents_to_path(&path, std::slice::from_ref(&item)).unwrap();
        let loaded = load_custom_subagents_from_path(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Reviewer");
        fs::remove_file(path).ok();
    }

    #[test]
    fn duplicate_trimmed_names_are_rejected() {
        let result = ensure_unique_subagent_names(&[
            CustomSubagentDefinition {
                id: "1".to_string(),
                name: "Reviewer".to_string(),
                description: String::new(),
                instruction: "Check diffs".to_string(),
                enabled: true,
            },
            CustomSubagentDefinition {
                id: "2".to_string(),
                name: "Reviewer".to_string(),
                description: String::new(),
                instruction: "Check tests".to_string(),
                enabled: true,
            },
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn codex_and_claude_compilers_compile_enabled_agents() {
        let prompts = codex_subagent_prompts();
        let compiled_codex = compile_codex_subagent_instructions(&[CustomSubagentDefinition {
            id: "agent-1".to_string(),
            name: "Reviewer".to_string(),
            description: "Check diffs".to_string(),
            instruction: "Review code changes and summarize risk.".to_string(),
            enabled: true,
        }])
        .unwrap();
        assert!(compiled_codex.contains("Reviewer"));
        assert!(compiled_codex.contains(prompts.baseline_prompt.as_str()));
        assert!(compiled_codex.contains(prompts.agent_specific_instruction_label.as_str()));

        let compiled_claude = compile_claude_managed_subagents(&[CustomSubagentDefinition {
            id: "agent-1".to_string(),
            name: "Reviewer".to_string(),
            description: "Check diffs".to_string(),
            instruction: "Review code changes and summarize risk.".to_string(),
            enabled: true,
        }])
        .unwrap();
        let compiled_claude_prompt = compiled_claude["agents"]["lilia-reviewer"]["prompt"]
            .as_str()
            .unwrap();
        assert!(compiled_claude_prompt.contains(prompts.baseline_prompt.as_str()));
        assert!(compiled_claude_prompt.contains("Review code changes and summarize risk."));
    }
}
