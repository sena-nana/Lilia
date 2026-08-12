use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchitectureBackend {
    NativeAgentkit,
}

impl ArchitectureBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeAgentkit => "native-agentkit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitecturePermission {
    Ask,
    Full,
    Readonly,
}

impl ArchitecturePermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Full => "full",
            Self::Readonly => "readonly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureChangeStatus {
    Proposed,
    Pending,
    Applied,
    Rejected,
    RolledBack,
}

impl ArchitectureChangeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::RolledBack => "rolled_back",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Option<Self> {
        match value {
            "proposed" => Some(Self::Proposed),
            "pending" => Some(Self::Pending),
            "applied" => Some(Self::Applied),
            "rejected" => Some(Self::Rejected),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub summary: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureGraph {
    pub project_id: String,
    pub version: i64,
    pub summary: String,
    pub nodes: Vec<ProjectArchitectureNode>,
    pub edges: Vec<ProjectArchitectureEdge>,
    pub updated_at: i64,
}

impl ProjectArchitectureGraph {
    pub fn empty(project_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            version: 0,
            summary: String::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectArchitectureChange {
    UpsertNode { node: ProjectArchitectureNode },
    RemoveNode { node_id: String },
    UpsertEdge { edge: ProjectArchitectureEdge },
    RemoveEdge { edge_id: String },
    SetSummary { summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureChangeEvent {
    pub id: Option<String>,
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: ArchitectureBackend,
    pub permission: ArchitecturePermission,
    pub status: ArchitectureChangeStatus,
    pub reason: String,
    pub changes: Vec<ProjectArchitectureChange>,
    pub before_version: i64,
    pub after_version: Option<i64>,
    pub created_at: Option<i64>,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureApplyInput {
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: ArchitectureBackend,
    pub permission: ArchitecturePermission,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<ProjectArchitectureChange>,
    pub request_id: Option<String>,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureRejectInput {
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: ArchitectureBackend,
    pub permission: ArchitecturePermission,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<ProjectArchitectureChange>,
    pub request_id: Option<String>,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureApplyResult {
    pub graph: ProjectArchitectureGraph,
    pub event: ProjectArchitectureChangeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureRollbackResult {
    pub graph: ProjectArchitectureGraph,
    pub event: Option<ProjectArchitectureChangeEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureChangeRecord {
    #[serde(flatten)]
    pub event: ProjectArchitectureChangeEvent,
    pub before_graph: Option<ProjectArchitectureGraph>,
    pub after_graph: Option<ProjectArchitectureGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchitectureQuarantineRecord {
    pub id: String,
    pub project_id: String,
    pub record_kind: String,
    pub record_id: String,
    pub reason_code: String,
    pub quarantined_at: i64,
}
