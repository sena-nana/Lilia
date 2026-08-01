use serde::{Deserialize, Serialize};

use crate::{BindingId, ConversationId, ProductRevision, ProjectId, TaskId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductConversationStatus {
    Active,
    Waiting,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductConversation {
    pub id: ConversationId,
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
    pub title: String,
    pub status: ProductConversationStatus,
    pub archived: bool,
    pub labels: Vec<String>,
    pub binding_ids: Vec<BindingId>,
    pub forked_from: Option<ConversationId>,
    pub migrated_from: Option<ConversationId>,
    pub legacy_source: Option<String>,
    pub timeline_cursor: u64,
    /// Unix epoch milliseconds. Hosts set this through their clock port.
    #[serde(default)]
    pub created_at: i64,
    /// Unix epoch milliseconds. Updated by the application command boundary.
    #[serde(default)]
    pub updated_at: i64,
    pub revision: ProductRevision,
}

impl ProductConversation {
    pub fn new(
        id: ConversationId,
        project_id: Option<ProjectId>,
        task_id: Option<TaskId>,
        title: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(crate::ProductError::InvalidInput {
                field: "title".into(),
                message: "conversation title must not be empty".into(),
            });
        }
        Ok(Self {
            id,
            project_id,
            task_id,
            title,
            status: ProductConversationStatus::Active,
            archived: false,
            labels: Vec::new(),
            binding_ids: Vec::new(),
            forked_from: None,
            migrated_from: None,
            legacy_source: None,
            timeline_cursor: 0,
            created_at: 0,
            updated_at: 0,
            revision: ProductRevision::INITIAL,
        })
    }

    pub fn fork(
        id: ConversationId,
        source: &ProductConversation,
        title: impl Into<String>,
    ) -> Result<Self, crate::ProductError> {
        let mut conversation =
            Self::new(id, source.project_id.clone(), source.task_id.clone(), title)?;
        conversation.forked_from = Some(source.id.clone());
        Ok(conversation)
    }

    pub fn bind_session(&mut self, binding_id: BindingId) -> bool {
        if self.binding_ids.contains(&binding_id) {
            return false;
        }
        self.binding_ids.push(binding_id);
        self.revision = self.revision.next();
        true
    }

    pub fn unbind_session(&mut self, binding_id: &BindingId) -> bool {
        let previous_len = self.binding_ids.len();
        self.binding_ids.retain(|candidate| candidate != binding_id);
        if self.binding_ids.len() == previous_len {
            return false;
        }
        self.revision = self.revision.next();
        true
    }

    pub fn advance_timeline_cursor(&mut self, cursor: u64) -> Result<bool, crate::ProductError> {
        if cursor < self.timeline_cursor {
            return Err(crate::ProductError::InvalidState {
                message: "conversation timeline cursor must be monotonic".into(),
            });
        }
        if cursor == self.timeline_cursor {
            return Ok(false);
        }
        self.timeline_cursor = cursor;
        self.revision = self.revision.next();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_can_bind_multiple_sessions_and_fork_without_copying_bindings() {
        let mut source = ProductConversation::new(
            ConversationId::new("conversation-1").unwrap(),
            Some(ProjectId::new("project-1").unwrap()),
            Some(TaskId::new("task-1").unwrap()),
            "Source",
        )
        .unwrap();
        assert!(source.bind_session(BindingId::new("binding-1").unwrap()));
        assert!(source.bind_session(BindingId::new("binding-2").unwrap()));
        assert!(!source.bind_session(BindingId::new("binding-2").unwrap()));

        let fork = ProductConversation::fork(
            ConversationId::new("conversation-2").unwrap(),
            &source,
            "Fork",
        )
        .unwrap();
        assert_eq!(fork.forked_from, Some(source.id));
        assert!(fork.binding_ids.is_empty());
        assert_eq!(source.binding_ids.len(), 2);
    }

    #[test]
    fn timeline_cursor_cannot_move_backwards() {
        let mut conversation = ProductConversation::new(
            ConversationId::new("conversation-1").unwrap(),
            None,
            None,
            "Conversation",
        )
        .unwrap();
        assert!(conversation.advance_timeline_cursor(4).unwrap());
        assert!(!conversation.advance_timeline_cursor(4).unwrap());
        assert!(matches!(
            conversation.advance_timeline_cursor(3),
            Err(crate::ProductError::InvalidState { .. })
        ));
    }
}
