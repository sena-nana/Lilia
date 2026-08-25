use lilia_contracts::{
    ChatAttachment, ChatConversationReference, ExecutionPermission, LiliaAgentWorkflow, TaskId,
};
use serde::{Deserialize, Serialize};

use crate::ComposerError;

/// Durable draft a task carries between turns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerState {
    pub task_id: TaskId,
    pub revision: u64,
    pub content: String,
    pub attachments: Vec<ChatAttachment>,
    pub conversation_references: Vec<ChatConversationReference>,
    pub workflow: Option<LiliaAgentWorkflow>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: ExecutionPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
}

impl ComposerState {
    pub(crate) fn new(task_id: TaskId) -> Self {
        Self {
            task_id,
            revision: 0,
            content: String::new(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            model: None,
            reasoning_effort: None,
            permission: ExecutionPermission::Ask,
            plan_mode: false,
            goal_mode: false,
        }
    }

    /// Creates an in-memory composer for a conversation that has not been
    /// materialized as a product task yet.
    pub fn transient(task_id: TaskId) -> Self {
        Self::new(task_id)
    }

    /// Applies the canonical composer reducer without writing a draft row.
    ///
    /// Native hosts use this while a new-conversation window is still
    /// transient. Once the user sends, the resulting state can be materialized
    /// together with the product task.
    pub fn apply_transient_command(
        &mut self,
        command: ComposerCommand,
    ) -> Result<bool, ComposerError> {
        let before = self.clone();
        match command {
            ComposerCommand::SetContent(content) => self.content = content,
            ComposerCommand::ApplyPromptOptimization {
                expected_revision,
                content,
            } => {
                ensure_expected_revision(self, expected_revision)?;
                self.content = content;
                self.workflow = None;
            }
            ComposerCommand::ApplySlashWorkflow {
                expected_revision,
                workflow,
            } => {
                ensure_expected_revision(self, expected_revision)?;
                self.content.clear();
                self.workflow = Some(workflow);
            }
            ComposerCommand::ReplaceAttachments(attachments) => self.attachments = attachments,
            ComposerCommand::RemoveAttachment(attachment_id) => {
                self.attachments
                    .retain(|attachment| attachment.id != attachment_id);
            }
            ComposerCommand::ApplyContextAttachment {
                expected_revision,
                content,
                attachment,
            } => {
                ensure_expected_revision(self, expected_revision)?;
                self.content = content;
                if !self
                    .attachments
                    .iter()
                    .any(|candidate| candidate.path.eq_ignore_ascii_case(&attachment.path))
                {
                    self.attachments.push(attachment);
                }
            }
            ComposerCommand::ApplyConversationReference {
                expected_revision,
                content,
                reference,
            } => {
                ensure_expected_revision(self, expected_revision)?;
                self.content = content;
                if !self
                    .conversation_references
                    .iter()
                    .any(|candidate| candidate.task_id == reference.task_id)
                {
                    self.conversation_references.push(reference);
                }
            }
            ComposerCommand::RemoveConversationReference(task_id) => {
                self.conversation_references
                    .retain(|reference| reference.task_id != task_id);
            }
            ComposerCommand::SetWorkflow(workflow) => self.workflow = workflow,
            ComposerCommand::SetModelSelection {
                model,
                reasoning_effort,
            } => {
                self.model = normalized_option(model);
                self.reasoning_effort = normalized_option(reasoning_effort);
            }
            ComposerCommand::SetModel(model) => self.model = normalized_option(model),
            ComposerCommand::SetReasoningEffort(effort) => {
                self.reasoning_effort = normalized_option(effort)
            }
            ComposerCommand::SetPermission(permission) => self.permission = permission,
            ComposerCommand::SetPlanMode(enabled) => self.plan_mode = enabled,
            ComposerCommand::SetGoalMode(enabled) => self.goal_mode = enabled,
        }
        let changed = self != &before;
        if changed {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(ComposerError::RevisionOverflow)?;
        }
        Ok(changed)
    }

    /// Title derived from the draft when a placeholder conversation is
    /// promoted to a real task.
    pub fn submission_title(&self) -> Option<String> {
        let normalized = self.content.split_whitespace().collect::<Vec<_>>().join(" ");
        let source = if normalized.is_empty() {
            self.attachments.first()?.name.trim().to_owned()
        } else {
            normalized
        };
        if source.is_empty() {
            return None;
        }
        let mut characters = source.chars();
        let mut title = characters.by_ref().take(30).collect::<String>();
        if characters.next().is_some() {
            title.push('…');
        }
        Some(title)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerCommand {
    SetContent(String),
    ApplyPromptOptimization {
        expected_revision: u64,
        content: String,
    },
    ApplySlashWorkflow {
        expected_revision: u64,
        workflow: LiliaAgentWorkflow,
    },
    ReplaceAttachments(Vec<ChatAttachment>),
    RemoveAttachment(String),
    ApplyContextAttachment {
        expected_revision: u64,
        content: String,
        attachment: ChatAttachment,
    },
    ApplyConversationReference {
        expected_revision: u64,
        content: String,
        reference: ChatConversationReference,
    },
    RemoveConversationReference(String),
    SetWorkflow(Option<LiliaAgentWorkflow>),
    SetModelSelection {
        model: Option<String>,
        reasoning_effort: Option<String>,
    },
    SetModel(Option<String>),
    SetReasoningEffort(Option<String>),
    SetPermission(ExecutionPermission),
    SetPlanMode(bool),
    SetGoalMode(bool),
}

pub fn ensure_expected_revision(
    state: &ComposerState,
    expected_revision: u64,
) -> Result<(), ComposerError> {
    if state.revision == expected_revision {
        Ok(())
    } else {
        Err(ComposerError::RevisionConflict {
            expected: expected_revision,
            actual: state.revision,
        })
    }
}

fn normalized_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}
