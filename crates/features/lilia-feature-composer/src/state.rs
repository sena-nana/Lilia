use lilia_contracts::{
    ChatAttachment, ChatAttachmentKind, ChatConversationReference, ExecutionPermission,
    LiliaAgentWorkflow, TaskId,
};
use serde::{Deserialize, Serialize};

use crate::ComposerError;

/// One editor chip derived from a token still present in `content`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentAtomSpan {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub token: String,
    pub kind: ContentAtomKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentAtomKind {
    File,
    Directory,
    Image,
    Conversation,
}

impl ContentAtomKind {
    fn from_attachment(attachment: &ChatAttachment) -> Self {
        if attachment.is_image() {
            Self::Image
        } else if attachment.kind == ChatAttachmentKind::Directory {
            Self::Directory
        } else {
            Self::File
        }
    }
}

/// Durable draft a task carries between turns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerState {
    pub task_id: TaskId,
    pub revision: u64,
    pub content: String,
    pub attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub inline_attachments: Vec<ChatAttachment>,
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
            inline_attachments: Vec::new(),
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
            ComposerCommand::ApplyPaste {
                expected_revision,
                expected_content,
                content,
                attachments,
            } => {
                ensure_expected_revision(self, expected_revision)?;
                if self.content != expected_content {
                    return Err(ComposerError::ContentConflict);
                }
                self.content = content;
                for attachment in attachments {
                    if let Some(existing) = self
                        .inline_attachments
                        .iter_mut()
                        .find(|current| current.path.eq_ignore_ascii_case(&attachment.path))
                    {
                        *existing = attachment;
                    } else {
                        self.inline_attachments.push(attachment);
                    }
                }
            }
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
                let paths = self
                    .attachments
                    .iter()
                    .chain(&self.inline_attachments)
                    .filter(|attachment| attachment.id == attachment_id)
                    .map(|attachment| attachment.path.clone())
                    .collect::<Vec<_>>();
                let removed = |attachment: &ChatAttachment| {
                    attachment.id == attachment_id
                        || paths
                            .iter()
                            .any(|path| path.eq_ignore_ascii_case(&attachment.path))
                };
                for attachment in self
                    .inline_attachments
                    .iter()
                    .filter(|attachment| removed(attachment))
                {
                    self.content = self.content.replace(&attachment.reference_text(), "");
                }
                self.attachments.retain(|attachment| !removed(attachment));
                self.inline_attachments
                    .retain(|attachment| !removed(attachment));
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

    /// Byte ranges of attachment and conversation reference tokens in `content`.
    /// Each range is one editor atom: caret, delete, and insert skip or replace it whole.
    pub fn content_atom_spans(&self) -> Vec<ContentAtomSpan> {
        let mut needles = Vec::new();
        for attachment in self.attachments.iter().chain(&self.inline_attachments) {
            let needle = attachment.reference_text();
            if needle.is_empty() {
                continue;
            }
            needles.push((
                needle,
                ContentAtomKind::from_attachment(attachment),
                attachment.name.clone(),
                attachment.id.clone(),
            ));
        }
        for reference in &self.conversation_references {
            let needle = reference.reference_text();
            if needle.is_empty() {
                continue;
            }
            needles.push((
                needle,
                ContentAtomKind::Conversation,
                reference.title.clone(),
                reference.task_id.clone(),
            ));
        }
        needles.sort_by(|(left, _, _, _), (right, _, _, _)| right.len().cmp(&left.len()));
        needles.dedup_by(|(left, _, _, _), (right, _, _, _)| left == right);
        let mut spans = Vec::new();
        for (needle, kind, label, token) in needles {
            let mut from = 0;
            while let Some(index) = self.content[from..].find(&needle) {
                let start = from + index;
                let end = start + needle.len();
                if self.content.is_char_boundary(start) && self.content.is_char_boundary(end) {
                    spans.push(ContentAtomSpan {
                        start,
                        end,
                        label: label.clone(),
                        token: token.clone(),
                        kind,
                    });
                }
                from = end;
            }
        }
        spans.sort_by_key(|span| span.start);
        let mut cleaned = Vec::with_capacity(spans.len());
        let mut cursor = 0usize;
        for span in spans {
            if span.start >= cursor {
                cursor = span.end;
                cleaned.push(span);
            }
        }
        cleaned
    }

    pub fn effective_conversation_references(
        &self,
    ) -> impl Iterator<Item = &ChatConversationReference> {
        self.conversation_references
            .iter()
            .filter(|reference| self.content.contains(&reference.reference_text()))
    }

    /// Manual attachments remain active independently of text. Inline sources
    /// are retained for text undo/redo but only active while their reference exists.
    pub fn effective_attachments(&self) -> impl Iterator<Item = &ChatAttachment> {
        self.attachments
            .iter()
            .chain(self.inline_attachments.iter().filter(|attachment| {
                self.content.contains(&attachment.reference_text())
                    && !self
                        .attachments
                        .iter()
                        .any(|manual| manual.path.eq_ignore_ascii_case(&attachment.path))
            }))
    }

    /// Title derived from the draft when a placeholder conversation is
    /// promoted to a real task.
    pub fn submission_title(&self) -> Option<String> {
        let normalized = self
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let source = if normalized.is_empty() {
            self.effective_attachments().next()?.name.trim().to_owned()
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
    ApplyPaste {
        expected_revision: u64,
        expected_content: String,
        content: String,
        attachments: Vec<ChatAttachment>,
    },
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

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::TaskId;

    #[test]
    fn content_atom_spans_cover_each_inline_reference_once() {
        let attachment: ChatAttachment = serde_json::from_value(serde_json::json!({
            "id": "file",
            "name": "a.txt",
            "path": "/tmp/a.txt",
            "kind": "file",
            "exists": true
        }))
        .unwrap();
        let directory: ChatAttachment = serde_json::from_value(serde_json::json!({
            "id": "dir",
            "name": "src",
            "path": "/tmp/src",
            "kind": "directory",
            "exists": true
        }))
        .unwrap();
        let image: ChatAttachment = serde_json::from_value(serde_json::json!({
            "id": "pic",
            "name": "shot.png",
            "path": "/tmp/shot.png",
            "kind": "file",
            "mime": "image/png",
            "exists": true
        }))
        .unwrap();
        let reference = ChatConversationReference {
            task_id: "task-1".into(),
            title: "相关设计".into(),
            route: "/chats/task-1".into(),
            project_id: None,
            project_name: None,
        };
        let mut state = ComposerState::new(TaskId::new("atoms").unwrap());
        state.content = format!(
            "see {} {} {} and {}",
            attachment.reference_text(),
            directory.reference_text(),
            image.reference_text(),
            reference.reference_text()
        );
        state.inline_attachments = vec![attachment.clone(), directory.clone(), image.clone()];
        state.conversation_references = vec![reference.clone()];
        let spans = state.content_atom_spans();
        assert_eq!(spans.len(), 4);
        assert_eq!(spans[0].kind, ContentAtomKind::File);
        assert_eq!(spans[0].label, attachment.name);
        assert_eq!(spans[1].kind, ContentAtomKind::Directory);
        assert_eq!(spans[1].label, directory.name);
        assert_eq!(spans[2].kind, ContentAtomKind::Image);
        assert_eq!(spans[2].label, image.name);
        assert_eq!(spans[3].kind, ContentAtomKind::Conversation);
        assert_eq!(spans[3].label, reference.title);
        assert!(spans
            .iter()
            .all(|span| span.label != state.content[span.start..span.end]));
        state.content = "plain".into();
        assert!(state.content_atom_spans().is_empty());
        assert!(state.effective_conversation_references().next().is_none());
    }
}
