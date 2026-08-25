use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackageView {
    pub skill_id: String,
    pub path: String,
    pub registered_from: String,
    pub scope: String,
    pub description: String,
    pub enabled: bool,
    pub editable: bool,
    pub runtime_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    User,
    Project,
}

impl SkillScope {
    pub const fn as_registry(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCreate {
    pub expected_registry_revision: u64,
    pub scope: SkillScope,
    pub project_cwd: Option<String>,
    pub skill_id: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageView {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: String,
    pub enabled: bool,
    pub editable: bool,
    pub runtime_available: bool,
    pub package_sha256: String,
    pub skill_count: usize,
    pub hook_count: usize,
    pub mcp_server_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerView {
    pub server_id: String,
    pub source: String,
    pub transport: String,
    pub location: Option<String>,
    pub registered: bool,
    pub editable: bool,
    pub enabled: bool,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub registered_from: Option<String>,
    pub runtime_state: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub restart_count: u64,
    pub last_error: Option<String>,
    pub tools: Vec<McpToolView>,
    pub resources: Vec<McpResourceView>,
    pub prompts: Vec<McpPromptView>,
    pub credentials: Vec<McpCredentialView>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpCredentialKind {
    Environment,
    Header,
}

impl McpCredentialKind {
    pub const fn key_segment(self) -> &'static str {
        match self {
            Self::Environment => "env",
            Self::Header => "header",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCredentialView {
    pub kind: McpCredentialKind,
    pub name: String,
    pub present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolView {
    pub name: String,
    pub namespaced_name: String,
    pub description: String,
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceView {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptArgumentView {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptView {
    pub name: String,
    pub namespaced_name: String,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgumentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceContentView {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub encoded_blob_length: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceReadView {
    pub server_id: String,
    pub uri: String,
    pub summary: String,
    pub contents: Vec<McpResourceContentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptFragmentView {
    pub fragment_id: String,
    pub content: String,
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptGetView {
    pub namespaced_name: String,
    pub description: Option<String>,
    pub fragments: Vec<McpPromptFragmentView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeServiceView {
    pub service_id: String,
    pub label: String,
    pub shared_with_agent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionsSnapshot {
    pub data_source: String,
    pub shared_identity_ok: bool,
    pub skills_registry_path: String,
    pub skills_registry_revision: u64,
    pub mcp_registry_path: String,
    pub mcp_registry_revision: u64,
    pub plugins_registry_path: String,
    pub plugins_registry_revision: u64,
    pub skill_roots: Vec<String>,
    pub skills: Vec<SkillPackageView>,
    pub plugins: Vec<PluginPackageView>,
    pub mcp_servers: Vec<McpServerView>,
    pub runtime_services: Vec<RuntimeServiceView>,
    pub legacy_plugin_manager_available: bool,
    pub legacy_hooks_manager_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    Stdio,
    StreamableHttp,
    Sse,
}

impl McpTransport {
    pub const fn as_registry(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::StreamableHttp => "streamable_http",
            Self::Sse => "sse",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerUpsert {
    pub expected_registry_revision: u64,
    pub server_id: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env_secret_names: Vec<String>,
    #[serde(default)]
    pub header_secret_names: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpActivationResult {
    pub server_id: String,
    pub runtime_state: Option<String>,
    pub tool_count: usize,
    pub resource_count: usize,
    pub prompt_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpActivationReport {
    pub results: Vec<McpActivationResult>,
    pub snapshot: ExtensionsSnapshot,
}
