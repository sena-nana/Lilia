//! Turn request vocabulary shared by the composer, the queue and the agent
//! runtime. The turn's *state* belongs to the agent runtime; these types only
//! describe what a caller asked for.

use lilia_contracts::{
    ChatAttachment, ChatConversationReference, ExecutionPermission as DesktopExecutionPermission,
    LiliaAgentWorkflow, TaskId,
};
use serde::{Deserialize, Serialize};

use crate::DesktopAutoTurnDecisionSettings;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAutomationTurnCorrelation {
    pub run_id: String,
    pub node_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSessionBranchMode {
    Continue,
    Fork,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSessionBranchAnchor {
    pub source_turn_id: String,
    pub mode: DesktopSessionBranchMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurnRequest {
    pub task_id: TaskId,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub conversation_references: Vec<ChatConversationReference>,
    pub workspace_path: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: DesktopExecutionPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
    #[serde(default)]
    pub allow_auto_turn_decision: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_turn_settings: Option<DesktopAutoTurnDecisionSettings>,
    #[serde(default)]
    pub auto_turn_decision_applied: bool,
    #[serde(default)]
    pub session_fork: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_branch: Option<DesktopSessionBranchAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_selection: Option<DesktopAutomaticTurnSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<DesktopAutomationTurnCorrelation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<LiliaAgentWorkflow>,
}

impl DesktopTurnRequest {
    pub fn new(task_id: TaskId, content: impl Into<String>) -> Self {
        Self {
            task_id,
            content: content.into(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workspace_path: None,
            model: None,
            reasoning_effort: None,
            permission: DesktopExecutionPermission::Ask,
            plan_mode: false,
            goal_mode: false,
            allow_auto_turn_decision: false,
            auto_turn_settings: None,
            auto_turn_decision_applied: false,
            session_fork: false,
            session_branch: None,
            automatic_selection: None,
            automation: None,
            guide_id: None,
            workflow: None,
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<ChatAttachment>) -> Self {
        self.attachments = attachments;
        self
    }

    pub fn with_conversation_references(
        mut self,
        references: Vec<ChatConversationReference>,
    ) -> Self {
        self.conversation_references = references;
        self
    }

    pub fn content_with_references(&self) -> String {
        let mut content = self.content.trim().to_owned();
        for attachment in &self.attachments {
            let reference = attachment.reference_text();
            if content.contains(&reference) {
                continue;
            }
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&reference);
        }
        for conversation in &self.conversation_references {
            let reference = conversation.reference_text();
            if content.contains(&reference) {
                continue;
            }
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&reference);
        }
        content
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAutomaticTurnSelection {
    pub source: String,
    pub tier: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub plan_mode: bool,
    pub goal_mode: bool,
    pub session_fork: bool,
    pub summary: Option<String>,
    pub signals: Vec<String>,
    pub decision_provider_id: String,
    pub decision_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTurnDispatchKind {
    Started,
    Queued { position: usize },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurnDispatch {
    pub turn_id: String,
    pub kind: DesktopTurnDispatchKind,
}
