use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use lilia_contracts::{
    AgentSessionRef, ArtifactProjection, PendingProjection, ProductError, ProductResult, TaskId,
    TimelineProjectionCommand, TimelineProjectionEvent, TodoProjection,
};

/// Port for product timeline / artifact / todo / pending projection persistence.
///
/// This is the product fact surface. Host SQLite mirrors (if any) are UI caches
/// and must be rebuildable from this store.
pub trait TimelineProjectionRepository: Send + Sync {
    fn apply(&self, command: TimelineProjectionCommand) -> ProductResult<ProjectionApplyResult>;
    fn list_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent>;
    fn list_for_session(&self, session: &AgentSessionRef) -> Vec<TimelineProjectionEvent>;
    fn list_artifacts_for_task(&self, task_id: &TaskId) -> Vec<ArtifactProjection>;
    fn list_todos_for_task(&self, task_id: &TaskId) -> Vec<TodoProjection>;
    fn list_pending_for_task(&self, task_id: &TaskId) -> Vec<PendingProjection>;
    fn clear_session(&self, session: &AgentSessionRef) -> ProductResult<()>;
    /// Apply commands without clearing. Duplicate ids are ignored.
    fn rebuild_from(&self, commands: Vec<TimelineProjectionCommand>) -> ProductResult<usize>;
    /// Clear one session projection then replay commands (idempotent rebuild path).
    fn rebuild_session(
        &self,
        session: &AgentSessionRef,
        commands: Vec<TimelineProjectionCommand>,
    ) -> ProductResult<usize>;
    fn cursor_for_session(&self, session: &AgentSessionRef) -> Option<u64>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionApplyResult {
    Inserted,
    DuplicateIgnored,
    Updated,
    SkippedUnknown,
}

#[derive(Default)]
struct TimelineStoreInner {
    /// Keyed by projection event id (`session:sequence`).
    events: BTreeMap<String, TimelineProjectionEvent>,
    /// Keyed by artifact projection id.
    artifacts: BTreeMap<String, ArtifactProjection>,
    /// Keyed by todo projection id (`session:todo_id`).
    todos: BTreeMap<String, TodoProjection>,
    /// Keyed by pending id (`session:request_id`).
    pendings: BTreeMap<String, PendingProjection>,
    /// Highest applied AgentKit sequence per session.
    cursors: BTreeMap<String, u64>,
}

/// In-memory projection store. Safe for tests and lightweight hosts.
#[derive(Clone, Default)]
pub struct InMemoryTimelineProjectionStore {
    inner: Arc<Mutex<TimelineStoreInner>>,
}

impl InMemoryTimelineProjectionStore {
    pub fn new() -> Self {
        Self::default()
    }
}

fn bump_cursor(cursors: &mut BTreeMap<String, u64>, session_id: &str, sequence: u64) {
    let cursor = cursors.entry(session_id.to_string()).or_insert(0);
    *cursor = (*cursor).max(sequence);
}

pub(crate) fn apply_command_to_maps(
    events: &mut BTreeMap<String, TimelineProjectionEvent>,
    artifacts: &mut BTreeMap<String, ArtifactProjection>,
    todos: &mut BTreeMap<String, TodoProjection>,
    pendings: &mut BTreeMap<String, PendingProjection>,
    cursors: &mut BTreeMap<String, u64>,
    command: TimelineProjectionCommand,
) -> ProjectionApplyResult {
    match command {
        TimelineProjectionCommand::UpsertTimelineEvent { event } => {
            let key = event.id.as_str().to_string();
            if events.contains_key(&key) {
                return ProjectionApplyResult::DuplicateIgnored;
            }
            let session_id = event.agent_session.as_str().to_string();
            let sequence = event.sequence;
            events.insert(key, event);
            bump_cursor(cursors, &session_id, sequence);
            ProjectionApplyResult::Inserted
        }
        TimelineProjectionCommand::UpsertArtifact { artifact } => {
            let key = artifact.id.clone();
            let session_id = artifact.agent_session.as_str().to_string();
            let sequence = artifact.sequence;
            let result = if artifacts.contains_key(&key) {
                ProjectionApplyResult::Updated
            } else {
                ProjectionApplyResult::Inserted
            };
            artifacts.insert(key, artifact);
            bump_cursor(cursors, &session_id, sequence);
            result
        }
        TimelineProjectionCommand::UpsertTodo { todo } => {
            let key = todo.id.clone();
            let session_id = todo.agent_session.as_str().to_string();
            let sequence = todo.sequence;
            let result = if todos.contains_key(&key) {
                ProjectionApplyResult::Updated
            } else {
                ProjectionApplyResult::Inserted
            };
            todos.insert(key, todo);
            bump_cursor(cursors, &session_id, sequence);
            result
        }
        TimelineProjectionCommand::UpsertPending { pending } => {
            let key = pending.id.clone();
            let session_id = pending.agent_session.as_str().to_string();
            let sequence = pending.sequence;
            let result = if pendings.contains_key(&key) {
                ProjectionApplyResult::Updated
            } else {
                ProjectionApplyResult::Inserted
            };
            pendings.insert(key, pending);
            bump_cursor(cursors, &session_id, sequence);
            result
        }
        TimelineProjectionCommand::ResolvePending {
            session_id,
            request_id,
            status,
            sequence,
            response,
        } => {
            let key = format!("{session_id}:{request_id}");
            bump_cursor(cursors, &session_id, sequence);
            if let Some(pending) = pendings.get_mut(&key) {
                pending.status = status;
                if let Some(obj) = pending.payload.as_object_mut() {
                    obj.insert("resolution".into(), response);
                } else {
                    pending.payload = serde_json::json!({ "resolution": response });
                }
                ProjectionApplyResult::Updated
            } else {
                ProjectionApplyResult::SkippedUnknown
            }
        }
        TimelineProjectionCommand::SkipUnknown {
            session_id,
            sequence,
            ..
        } => {
            bump_cursor(cursors, &session_id, sequence);
            ProjectionApplyResult::SkippedUnknown
        }
    }
}

impl TimelineProjectionRepository for InMemoryTimelineProjectionStore {
    fn apply(&self, command: TimelineProjectionCommand) -> ProductResult<ProjectionApplyResult> {
        let mut store = self.inner.lock().map_err(|_| ProductError::Unavailable {
            message: "timeline projection store lock poisoned".into(),
        })?;
        let store = &mut *store;
        Ok(apply_command_to_maps(
            &mut store.events,
            &mut store.artifacts,
            &mut store.todos,
            &mut store.pendings,
            &mut store.cursors,
            command,
        ))
    }

    fn list_for_task(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        let store = self.inner.lock().expect("timeline projection store lock");
        let mut events: Vec<_> = store
            .events
            .values()
            .filter(|event| &event.task_id == task_id)
            .cloned()
            .collect();
        events.sort_by(|a, b| {
            a.sequence
                .cmp(&b.sequence)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        events
    }

    fn list_for_session(&self, session: &AgentSessionRef) -> Vec<TimelineProjectionEvent> {
        let store = self.inner.lock().expect("timeline projection store lock");
        let mut events: Vec<_> = store
            .events
            .values()
            .filter(|event| &event.agent_session == session)
            .cloned()
            .collect();
        events.sort_by(|a, b| a.sequence.cmp(&b.sequence));
        events
    }

    fn list_artifacts_for_task(&self, task_id: &TaskId) -> Vec<ArtifactProjection> {
        let store = self.inner.lock().expect("timeline projection store lock");
        let mut rows: Vec<_> = store
            .artifacts
            .values()
            .filter(|row| &row.task_id == task_id)
            .cloned()
            .collect();
        rows.sort_by(|a, b| a.sequence.cmp(&b.sequence).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    fn list_todos_for_task(&self, task_id: &TaskId) -> Vec<TodoProjection> {
        let store = self.inner.lock().expect("timeline projection store lock");
        let mut rows: Vec<_> = store
            .todos
            .values()
            .filter(|row| &row.task_id == task_id)
            .cloned()
            .collect();
        rows.sort_by(|a, b| a.sequence.cmp(&b.sequence).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    fn list_pending_for_task(&self, task_id: &TaskId) -> Vec<PendingProjection> {
        let store = self.inner.lock().expect("timeline projection store lock");
        let mut rows: Vec<_> = store
            .pendings
            .values()
            .filter(|row| &row.task_id == task_id)
            .cloned()
            .collect();
        rows.sort_by(|a, b| a.sequence.cmp(&b.sequence).then_with(|| a.id.cmp(&b.id)));
        rows
    }

    fn clear_session(&self, session: &AgentSessionRef) -> ProductResult<()> {
        let mut store = self.inner.lock().map_err(|_| ProductError::Unavailable {
            message: "timeline projection store lock poisoned".into(),
        })?;
        let session_id = session.as_str();
        store
            .events
            .retain(|_, event| event.agent_session.as_str() != session_id);
        store
            .artifacts
            .retain(|_, row| row.agent_session.as_str() != session_id);
        store
            .todos
            .retain(|_, row| row.agent_session.as_str() != session_id);
        store
            .pendings
            .retain(|_, row| row.agent_session.as_str() != session_id);
        store.cursors.remove(session_id);
        Ok(())
    }

    fn rebuild_from(&self, commands: Vec<TimelineProjectionCommand>) -> ProductResult<usize> {
        let mut inserted = 0;
        for command in commands {
            match self.apply(command)? {
                ProjectionApplyResult::Inserted => inserted += 1,
                ProjectionApplyResult::DuplicateIgnored
                | ProjectionApplyResult::Updated
                | ProjectionApplyResult::SkippedUnknown => {}
            }
        }
        Ok(inserted)
    }

    fn rebuild_session(
        &self,
        session: &AgentSessionRef,
        commands: Vec<TimelineProjectionCommand>,
    ) -> ProductResult<usize> {
        self.clear_session(session)?;
        self.rebuild_from(commands)
    }

    fn cursor_for_session(&self, session: &AgentSessionRef) -> Option<u64> {
        let store = self.inner.lock().expect("timeline projection store lock");
        store.cursors.get(session.as_str()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_contracts::{PendingProjectionStatus, ProjectionEventId, TaskId};
    use serde_json::json;

    fn sample_event(session: &str, sequence: u64) -> TimelineProjectionEvent {
        TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence(session, sequence),
            task_id: TaskId::new("task-1").unwrap(),
            agent_session: AgentSessionRef::new(session).unwrap(),
            sequence,
            turn_id: Some("turn-1".into()),
            kind: "message".into(),
            status: "success".into(),
            title: "ok".into(),
            summary: Some("hi".into()),
            payload: json!({ "role": "assistant", "projected": true }),
            projected: true,
        }
    }

    #[test]
    fn apply_is_idempotent_by_session_sequence() {
        let store = InMemoryTimelineProjectionStore::new();
        let event = sample_event("sess-1", 3);
        let cmd = TimelineProjectionCommand::UpsertTimelineEvent {
            event: event.clone(),
        };
        assert_eq!(
            store.apply(cmd.clone()).unwrap(),
            ProjectionApplyResult::Inserted
        );
        assert_eq!(
            store.apply(cmd).unwrap(),
            ProjectionApplyResult::DuplicateIgnored
        );
        assert_eq!(store.list_for_task(&event.task_id).len(), 1);
        assert_eq!(store.cursor_for_session(&event.agent_session), Some(3));
    }

    #[test]
    fn rebuild_clears_and_replays_without_duplicates() {
        let store = InMemoryTimelineProjectionStore::new();
        let session = AgentSessionRef::new("sess-rebuild").unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(session.as_str(), 1),
            })
            .unwrap();
        store.clear_session(&session).unwrap();
        assert!(store.list_for_session(&session).is_empty());

        let inserted = store
            .rebuild_from(vec![
                TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(session.as_str(), 1),
                },
                TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(session.as_str(), 2),
                },
                TimelineProjectionCommand::UpsertTimelineEvent {
                    event: sample_event(session.as_str(), 1),
                },
            ])
            .unwrap();
        assert_eq!(inserted, 2);
        assert_eq!(store.list_for_session(&session).len(), 2);
    }

    #[test]
    fn rebuild_session_is_deterministic_and_idempotent() {
        let store = InMemoryTimelineProjectionStore::new();
        let session = AgentSessionRef::new("sess-det").unwrap();
        let commands = vec![
            TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(session.as_str(), 1),
            },
            TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(session.as_str(), 2),
            },
            TimelineProjectionCommand::SkipUnknown {
                session_id: session.as_str().into(),
                sequence: 3,
                reason: "optional".into(),
            },
            TimelineProjectionCommand::UpsertTimelineEvent {
                event: sample_event(session.as_str(), 1),
            },
        ];
        let first = store.rebuild_session(&session, commands.clone()).unwrap();
        let second = store.rebuild_session(&session, commands).unwrap();
        assert_eq!(first, 2);
        assert_eq!(second, 2);
        let listed = store.list_for_session(&session);
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|event| event.projected));
        assert_eq!(store.cursor_for_session(&session), Some(3));
    }

    #[test]
    fn artifact_todo_pending_projection_roundtrip() {
        let store = InMemoryTimelineProjectionStore::new();
        let task = TaskId::new("task-side").unwrap();
        let session = AgentSessionRef::new("sess-side").unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertArtifact {
                artifact: ArtifactProjection {
                    id: format!("{}:art-1", session.as_str()),
                    task_id: task.clone(),
                    agent_session: session.clone(),
                    sequence: 1,
                    turn_id: Some("t1".into()),
                    artifact_id: "art-1".into(),
                    media_type: "text/plain".into(),
                    summary: "note".into(),
                    kind: Some("file".into()),
                    size_bytes: Some(12),
                    content_hash: Some("abc".into()),
                    content_ref: Some(json!({ "kind": "resource", "id": "r1" })),
                    provenance: Some("native".into()),
                    status: "available".into(),
                },
            })
            .unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertTodo {
                todo: TodoProjection {
                    id: format!("{}:todo-1", session.as_str()),
                    task_id: task.clone(),
                    agent_session: session.clone(),
                    sequence: 2,
                    turn_id: Some("t1".into()),
                    todo_id: "todo-1".into(),
                    revision: 1,
                    items: json!([{ "itemId": "i1", "title": "do", "status": "pending" }]),
                },
            })
            .unwrap();
        store
            .apply(TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("{}:req-1", session.as_str()),
                    task_id: task.clone(),
                    agent_session: session.clone(),
                    sequence: 3,
                    turn_id: Some("t1".into()),
                    request_id: "req-1".into(),
                    kind: "approval".into(),
                    status: PendingProjectionStatus::Open,
                    prompt: Some("allow?".into()),
                    action_revision: Some(1),
                    payload: json!({ "tool": "write" }),
                },
            })
            .unwrap();
        store
            .apply(TimelineProjectionCommand::ResolvePending {
                session_id: session.as_str().into(),
                request_id: "req-1".into(),
                status: PendingProjectionStatus::Resolved,
                sequence: 4,
                response: json!({ "accepted": true }),
            })
            .unwrap();

        assert_eq!(store.list_artifacts_for_task(&task).len(), 1);
        assert_eq!(store.list_todos_for_task(&task).len(), 1);
        let pending = store.list_pending_for_task(&task);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, PendingProjectionStatus::Resolved);
        assert_eq!(store.cursor_for_session(&session), Some(4));
    }
}
