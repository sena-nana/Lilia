use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "legacy-runner")]
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSkill {
    pub backend: String,
    pub scope: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackage {
    pub backend: String,
    pub scope: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMcpServer {
    pub backend: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    pub env_keys: Vec<String>,
    pub enabled: bool,
    pub editable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PluginMcpServerInput {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub remove_env_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginsOverview {
    pub skills: Vec<PluginSkill>,
    pub packages: Vec<PluginPackage>,
    pub mcp_servers: Vec<PluginMcpServer>,
    pub config_paths: BTreeMap<String, Option<String>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookTrustState {
    Unknown,
    Required,
    Managed,
    NA,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSourceSummary {
    pub id: String,
    pub backend: String,
    pub scope: String,
    pub format: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub editable: bool,
    pub managed: bool,
    pub enabled: bool,
    pub handler_count: usize,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
    pub trust_state: HookTrustState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookHandlerView {
    pub id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_windows: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub supported: bool,
    pub executable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_advanced_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced_json: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDocumentView {
    pub source: HookSourceSummary,
    pub handlers: Vec<HookHandlerView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_document: Option<String>,
    pub raw_format: String,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksOverview {
    pub sources: Vec<HookSourceSummary>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HookHandlerUpdateInput {
    pub id: Option<String>,
    pub event: String,
    #[serde(default)]
    pub matcher: Option<String>,
    #[serde(rename = "type")]
    pub handler_type: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub command_windows: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub group_advanced_json: Option<String>,
    #[serde(default)]
    pub advanced_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HookDocumentUpdateInput {
    #[serde(default)]
    pub handlers: Vec<HookHandlerUpdateInput>,
}

#[cfg(feature = "legacy-runner")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRuntimePlugin {
    pub r#type: String,
    pub path: String,
}

#[cfg(feature = "legacy-runner")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRuntimeExtensions {
    pub skills: Vec<String>,
    pub plugins: Vec<ClaudeRuntimePlugin>,
    pub mcp_servers: BTreeMap<String, ClaudeRuntimeMcpServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<JsonValue>,
    pub warnings: Vec<String>,
}

#[cfg(any(feature = "legacy-runner", test))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeRuntimeMcpServer {
    pub r#type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[cfg(feature = "legacy-runner")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRuntimeExtensions {
    pub mcp_servers: Vec<PluginMcpServer>,
    pub config_path: Option<String>,
    pub warnings: Vec<String>,
}

#[cfg(feature = "legacy-runner")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeExtensions {
    pub claude: ClaudeRuntimeExtensions,
    pub codex: CodexRuntimeExtensions,
}
