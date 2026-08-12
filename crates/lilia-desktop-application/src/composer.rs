#[cfg(test)]
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ChatAttachment, ChatConversationReference, TaskId};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::legacy_database::SharedLegacyConnection;
use crate::submission::DesktopGuideQueueInput;
use crate::{
    DesktopApplication, DesktopApplicationError, DesktopEventKind, DesktopExecutionPermission,
    DesktopTaskTodo, DesktopTodoCreate, DesktopTodoPriority, DesktopTurnDispatch,
    DesktopTurnRequest,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopComposerState {
    pub task_id: TaskId,
    pub revision: u64,
    pub content: String,
    pub attachments: Vec<ChatAttachment>,
    pub conversation_references: Vec<ChatConversationReference>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission: DesktopExecutionPermission,
    pub plan_mode: bool,
    pub goal_mode: bool,
}

impl DesktopComposerState {
    fn new(task_id: TaskId) -> Self {
        Self {
            task_id,
            revision: 0,
            content: String::new(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            model: None,
            reasoning_effort: None,
            permission: DesktopExecutionPermission::Ask,
            plan_mode: false,
            goal_mode: false,
        }
    }

    pub(crate) fn turn_request(&self) -> DesktopTurnRequest {
        let mut request = DesktopTurnRequest::new(self.task_id.clone(), self.content.trim())
            .with_attachments(self.attachments.clone())
            .with_conversation_references(self.conversation_references.clone());
        request.model = self.model.clone();
        request.reasoning_effort = self.reasoning_effort.clone();
        request.permission = self.permission;
        request.plan_mode = self.plan_mode;
        request.goal_mode = self.goal_mode;
        request.allow_auto_turn_decision = true;
        request
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DesktopComposerSubmission {
    Turn(DesktopTurnDispatch),
    Command(crate::DesktopSlashCommandExecution),
    Guide {
        guide: DesktopTaskTodo,
        turn: Option<DesktopTurnDispatch>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopComposerCommand {
    SetContent(String),
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
    SetModel(Option<String>),
    SetReasoningEffort(Option<String>),
    SetPermission(DesktopExecutionPermission),
    SetPlanMode(bool),
    SetGoalMode(bool),
}

pub(crate) struct DesktopComposerStore {
    connection: SharedLegacyConnection,
}

impl DesktopComposerStore {
    #[cfg(test)]
    pub(crate) fn in_memory() -> Result<Self, DesktopComposerError> {
        let connection =
            Connection::open_in_memory().map_err(|error| DesktopComposerError::Storage {
                operation: "open in-memory composer database",
                message: error.to_string(),
            })?;
        Self::from_connection(connection)
    }

    #[cfg(test)]
    fn from_connection(connection: Connection) -> Result<Self, DesktopComposerError> {
        Self::from_shared(Arc::new(Mutex::new(connection)))
    }

    pub(crate) fn from_shared(
        connection: SharedLegacyConnection,
    ) -> Result<Self, DesktopComposerError> {
        let locked = connection
            .lock()
            .map_err(|_| DesktopComposerError::Storage {
                operation: "lock composer database for initialization",
                message: "connection lock poisoned".to_owned(),
            })?;
        locked
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS desktop_composer_drafts (
                  task_id          TEXT PRIMARY KEY,
                  revision         INTEGER NOT NULL,
                  content          TEXT NOT NULL,
                  attachments_json TEXT NOT NULL,
                  conversation_references_json TEXT NOT NULL DEFAULT '[]',
                  model            TEXT,
                  reasoning_effort TEXT,
                  permission       TEXT NOT NULL CHECK (permission IN ('full','ask','readonly')),
                  plan_mode        INTEGER NOT NULL CHECK (plan_mode IN (0, 1)),
                  goal_mode        INTEGER NOT NULL CHECK (goal_mode IN (0, 1)),
                  updated_at       INTEGER NOT NULL
                );
                "#,
            )
            .map_err(|error| DesktopComposerError::Storage {
                operation: "initialize composer schema",
                message: error.to_string(),
            })?;
        ensure_column(
            &locked,
            "desktop_composer_drafts",
            "conversation_references_json",
            "ALTER TABLE desktop_composer_drafts ADD COLUMN conversation_references_json TEXT NOT NULL DEFAULT '[]'",
        )?;
        drop(locked);
        Ok(Self { connection })
    }

    fn snapshot(&self, task_id: &TaskId) -> Result<DesktopComposerState, DesktopComposerError> {
        let connection = self.connection("read composer draft")?;
        Self::snapshot_from(&connection, task_id)
    }

    pub(crate) fn snapshot_from(
        connection: &Connection,
        task_id: &TaskId,
    ) -> Result<DesktopComposerState, DesktopComposerError> {
        connection
            .query_row(
                r#"SELECT task_id, revision, content, attachments_json, model,
                          reasoning_effort, permission, plan_mode, goal_mode,
                          conversation_references_json
                   FROM desktop_composer_drafts WHERE task_id = ?1"#,
                params![task_id.as_str()],
                row_to_composer,
            )
            .optional()
            .map_err(|error| DesktopComposerError::Storage {
                operation: "read composer draft",
                message: error.to_string(),
            })
            .map(|state| state.unwrap_or_else(|| DesktopComposerState::new(task_id.clone())))
    }

    fn execute(
        &self,
        task_id: &TaskId,
        command: DesktopComposerCommand,
    ) -> Result<(DesktopComposerState, bool), DesktopComposerError> {
        let before = self.snapshot(task_id)?;
        let mut state = before.clone();
        match command {
            DesktopComposerCommand::SetContent(content) => state.content = content,
            DesktopComposerCommand::ReplaceAttachments(attachments) => {
                state.attachments = attachments
            }
            DesktopComposerCommand::RemoveAttachment(attachment_id) => {
                state
                    .attachments
                    .retain(|attachment| attachment.id != attachment_id);
            }
            DesktopComposerCommand::ApplyContextAttachment {
                expected_revision,
                content,
                attachment,
            } => {
                ensure_expected_revision(&state, expected_revision)?;
                state.content = content;
                if !state
                    .attachments
                    .iter()
                    .any(|candidate| candidate.path.eq_ignore_ascii_case(&attachment.path))
                {
                    state.attachments.push(attachment);
                }
            }
            DesktopComposerCommand::ApplyConversationReference {
                expected_revision,
                content,
                reference,
            } => {
                ensure_expected_revision(&state, expected_revision)?;
                state.content = content;
                if !state
                    .conversation_references
                    .iter()
                    .any(|candidate| candidate.task_id == reference.task_id)
                {
                    state.conversation_references.push(reference);
                }
            }
            DesktopComposerCommand::RemoveConversationReference(task_id) => {
                state
                    .conversation_references
                    .retain(|reference| reference.task_id != task_id);
            }
            DesktopComposerCommand::SetModel(model) => state.model = normalized_option(model),
            DesktopComposerCommand::SetReasoningEffort(effort) => {
                state.reasoning_effort = normalized_option(effort)
            }
            DesktopComposerCommand::SetPermission(permission) => state.permission = permission,
            DesktopComposerCommand::SetPlanMode(enabled) => state.plan_mode = enabled,
            DesktopComposerCommand::SetGoalMode(enabled) => state.goal_mode = enabled,
        }
        let changed = state != before;
        if changed {
            state.revision = state
                .revision
                .checked_add(1)
                .ok_or(DesktopComposerError::RevisionOverflow)?;
            self.save(&state)?;
        }
        Ok((state, changed))
    }

    #[cfg(test)]
    fn clear_dispatched_payload(
        &self,
        task_id: &TaskId,
        dispatched_revision: u64,
    ) -> Result<Option<DesktopComposerState>, DesktopComposerError> {
        let connection = self.connection("clear dispatched composer payload")?;
        Self::clear_dispatched_payload_in(&connection, task_id, dispatched_revision)
    }

    pub(crate) fn clear_dispatched_payload_in(
        connection: &Connection,
        task_id: &TaskId,
        dispatched_revision: u64,
    ) -> Result<Option<DesktopComposerState>, DesktopComposerError> {
        let mut state = Self::snapshot_from(connection, task_id)?;
        if state.revision != dispatched_revision
            || (state.content.is_empty()
                && state.attachments.is_empty()
                && state.conversation_references.is_empty())
        {
            return Ok(None);
        }
        state.content.clear();
        state.attachments.clear();
        state.conversation_references.clear();
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or(DesktopComposerError::RevisionOverflow)?;
        Self::save_to(connection, &state)?;
        Ok(Some(state))
    }

    fn save(&self, state: &DesktopComposerState) -> Result<(), DesktopComposerError> {
        let connection = self.connection("save composer draft")?;
        Self::save_to(&connection, state)
    }

    pub(crate) fn save_to(
        connection: &Connection,
        state: &DesktopComposerState,
    ) -> Result<(), DesktopComposerError> {
        let attachments = serde_json::to_string(&state.attachments).map_err(|error| {
            DesktopComposerError::Serialization {
                field: "attachments",
                message: error.to_string(),
            }
        })?;
        let conversation_references = serde_json::to_string(&state.conversation_references)
            .map_err(|error| DesktopComposerError::Serialization {
                field: "conversationReferences",
                message: error.to_string(),
            })?;
        connection
            .execute(
                r#"INSERT INTO desktop_composer_drafts
                   (task_id, revision, content, attachments_json, model, reasoning_effort,
                    permission, plan_mode, goal_mode, updated_at, conversation_references_json)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                   ON CONFLICT(task_id) DO UPDATE SET
                     revision = excluded.revision,
                     content = excluded.content,
                     attachments_json = excluded.attachments_json,
                     model = excluded.model,
                     reasoning_effort = excluded.reasoning_effort,
                     permission = excluded.permission,
                     plan_mode = excluded.plan_mode,
                     goal_mode = excluded.goal_mode,
                     updated_at = excluded.updated_at,
                     conversation_references_json = excluded.conversation_references_json"#,
                params![
                    state.task_id.as_str(),
                    i64::try_from(state.revision)
                        .map_err(|_| DesktopComposerError::RevisionOverflow)?,
                    state.content,
                    attachments,
                    state.model,
                    state.reasoning_effort,
                    state.permission.as_str(),
                    i64::from(state.plan_mode),
                    i64::from(state.goal_mode),
                    now_millis(),
                    conversation_references,
                ],
            )
            .map(|_| ())
            .map_err(|error| DesktopComposerError::Storage {
                operation: "save composer draft",
                message: error.to_string(),
            })
    }

    fn connection(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, Connection>, DesktopComposerError> {
        self.connection
            .lock()
            .map_err(|_| DesktopComposerError::Storage {
                operation,
                message: "connection lock poisoned".to_owned(),
            })
    }
}

impl DesktopApplication {
    pub fn composer_state(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopComposerState, DesktopApplicationError> {
        self.get_task(task_id)?;
        Ok(self
            .inner
            .composers
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("composer"))?
            .snapshot(task_id)?)
    }

    pub fn execute_composer_command(
        &self,
        task_id: &TaskId,
        command: DesktopComposerCommand,
    ) -> Result<DesktopComposerState, DesktopApplicationError> {
        self.get_task(task_id)?;
        let (state, changed) = self
            .inner
            .composers
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("composer"))?
            .execute(task_id, command)?;
        if changed {
            self.emit_event(DesktopEventKind::ComposerChanged {
                task_id: task_id.clone(),
                revision: state.revision,
            });
        }
        Ok(state)
    }

    pub fn start_composer_turn(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        let composer = self.composer_state(task_id)?;
        let mut request = composer.turn_request();
        request.workspace_path = self.task_workspace_path(task_id)?;
        let request = self.prepare_task_turn_request(request)?;
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let turn_id = format!("native-turn-{}", Uuid::new_v4());
        let cleared = self
            .inner
            .submissions
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("submission"))?
            .commit_turn(&composer, &turn_id, &request)?;
        let (dispatch, should_start) = self.accept_persisted_task_turn(request, turn_id, false)?;
        drop(submission);
        if let Some(state) = cleared {
            self.emit_event(DesktopEventKind::ComposerChanged {
                task_id: task_id.clone(),
                revision: state.revision,
            });
        }
        if should_start {
            self.activate_turn_worker(task_id.clone(), dispatch.turn_id.clone())?;
        }
        Ok(dispatch)
    }

    pub fn submit_composer(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopComposerSubmission, DesktopApplicationError> {
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let composer = self.composer_state(task_id)?;
        if composer.attachments.is_empty() && composer.conversation_references.is_empty() {
            if let Some(execution) = self.resolve_task_slash_command(task_id, &composer.content)? {
                self.record_task_slash_command(task_id, composer.revision, &execution)?;
                let cleared = self
                    .inner
                    .submissions
                    .lock()
                    .map_err(|_| DesktopApplicationError::StateUnavailable("submission"))?
                    .commit_composer_clear(&composer)?;
                drop(submission);
                if let Some(state) = cleared {
                    self.emit_event(DesktopEventKind::ComposerChanged {
                        task_id: task_id.clone(),
                        revision: state.revision,
                    });
                }
                return Ok(DesktopComposerSubmission::Command(execution));
            }
        }
        let runtime = self.task_runtime_snapshot(task_id);
        if runtime.turn_id.is_none() && runtime.queued_turns == 0 {
            drop(submission);
            return self
                .start_composer_turn(task_id)
                .map(DesktopComposerSubmission::Turn);
        }

        let guide_text = crate::agent::turn_content_with_references(&composer.turn_request());
        let attachments = composer
            .attachments
            .iter()
            .map(|attachment| {
                serde_json::to_value(attachment).map_err(|error| {
                    DesktopComposerError::Serialization {
                        field: "attachments",
                        message: error.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let guide_id = Uuid::new_v4().to_string();
        let queue = matches!(
            runtime.phase.as_str(),
            "waiting_approval"
                | "resolving_approval"
                | "waiting_interaction"
                | "resolving_interaction"
        )
        .then(|| {
            let mut request = composer.turn_request();
            request.workspace_path = self.task_workspace_path(task_id)?;
            let request = self.prepare_task_turn_request(request)?;
            Ok::<_, DesktopApplicationError>(DesktopGuideQueueInput {
                turn_id: format!("native-turn-{}", Uuid::new_v4()),
                request,
            })
        })
        .transpose()?;
        let committed = self
            .inner
            .submissions
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("submission"))?
            .commit_guide(
                &composer,
                &guide_id,
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: guide_text,
                    priority: DesktopTodoPriority::Normal,
                    attachments,
                    conversation_references: composer.conversation_references.clone(),
                },
                queue,
            )?;
        let queued_turn = committed
            .queued
            .map(|queued| {
                debug_assert_eq!(
                    queued.request.guide_id.as_deref(),
                    Some(queued.guide.id.as_str())
                );
                self.accept_persisted_task_turn(queued.request, queued.turn_id, true)
            })
            .transpose()?;
        drop(submission);
        self.emit_event(DesktopEventKind::TodosChanged {
            task_id: task_id.clone(),
        });
        if let Some(state) = committed.cleared {
            self.emit_event(DesktopEventKind::ComposerChanged {
                task_id: task_id.clone(),
                revision: state.revision,
            });
        }
        let turn = if let Some((dispatch, should_start)) = queued_turn {
            if should_start {
                self.activate_turn_worker(task_id.clone(), dispatch.turn_id.clone())?;
            }
            Some(dispatch)
        } else {
            None
        };
        Ok(DesktopComposerSubmission::Guide {
            guide: committed.guide,
            turn,
        })
    }

    pub fn submit_composer_guide(
        &self,
        expected_revision: u64,
        input: DesktopTodoCreate,
    ) -> Result<DesktopTaskTodo, DesktopApplicationError> {
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let composer = self.composer_state(&input.task_id)?;
        ensure_expected_revision(&composer, expected_revision)?;
        let guide_id = Uuid::new_v4().to_string();
        let committed = self
            .inner
            .submissions
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("submission"))?
            .commit_guide(&composer, &guide_id, input, None)?;
        drop(submission);
        self.emit_event(DesktopEventKind::TodosChanged {
            task_id: composer.task_id.clone(),
        });
        if let Some(state) = committed.cleared {
            self.emit_event(DesktopEventKind::ComposerChanged {
                task_id: composer.task_id,
                revision: state.revision,
            });
        }
        Ok(committed.guide)
    }
}

fn row_to_composer(row: &rusqlite::Row<'_>) -> rusqlite::Result<DesktopComposerState> {
    let task_id =
        TaskId::new(row.get::<_, String>(0)?).map_err(|error| invalid_data(error.to_string()))?;
    let revision =
        u64::try_from(row.get::<_, i64>(1)?).map_err(|error| invalid_data(error.to_string()))?;
    let attachments_json = row.get::<_, String>(3)?;
    let attachments =
        serde_json::from_str(&attachments_json).map_err(|error| invalid_data(error.to_string()))?;
    let conversation_references_json = row.get::<_, String>(9)?;
    let conversation_references = serde_json::from_str(&conversation_references_json)
        .map_err(|error| invalid_data(error.to_string()))?;
    let permission = DesktopExecutionPermission::parse(&row.get::<_, String>(6)?)
        .ok_or_else(|| invalid_data("invalid composer permission".to_owned()))?;
    Ok(DesktopComposerState {
        task_id,
        revision,
        content: row.get(2)?,
        attachments,
        conversation_references,
        model: row.get(4)?,
        reasoning_effort: row.get(5)?,
        permission,
        plan_mode: row.get::<_, i64>(7)? != 0,
        goal_mode: row.get::<_, i64>(8)? != 0,
    })
}

fn ensure_expected_revision(
    state: &DesktopComposerState,
    expected_revision: u64,
) -> Result<(), DesktopComposerError> {
    if state.revision == expected_revision {
        Ok(())
    } else {
        Err(DesktopComposerError::RevisionConflict {
            expected: expected_revision,
            actual: state.revision,
        })
    }
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    migration: &str,
) -> Result<(), DesktopComposerError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| DesktopComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| DesktopComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })?;
    for candidate in columns {
        if candidate.map_err(|error| DesktopComposerError::Storage {
            operation: "inspect composer schema",
            message: error.to_string(),
        })? == column
        {
            return Ok(());
        }
    }
    connection
        .execute_batch(migration)
        .map_err(|error| DesktopComposerError::Storage {
            operation: "migrate composer schema",
            message: error.to_string(),
        })
}

fn invalid_data(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopComposerError {
    #[error("composer revision overflowed")]
    RevisionOverflow,
    #[error("composer revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("composer serialization failed for {field}: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("composer storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

fn normalized_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask};
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    static NEXT_COMPOSER_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> (DesktopApplication, TaskId, TaskId) {
        let id = NEXT_COMPOSER_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-composer:{id}"),
            format!("desktop-composer-test:{id}"),
        )
        .unwrap();
        let first = TaskId::new("composer-first").unwrap();
        let second = TaskId::new("composer-second").unwrap();
        let client = authority.client().unwrap();
        for task_id in [&first, &second] {
            client
                .products()
                .create_entity(ProductEntity::Task(
                    ProductTask::new(task_id.clone(), None, task_id.as_str()).unwrap(),
                ))
                .unwrap();
        }
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/composer", "liliacode.test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        (application, first, second)
    }

    #[test]
    fn composer_commands_are_task_scoped_revisioned_and_evented() {
        let (application, first, second) = application();
        let events = application.subscribe_events();

        let updated = application
            .execute_composer_command(
                &first,
                DesktopComposerCommand::SetContent("  ship native  ".to_owned()),
            )
            .unwrap();
        let configured = application
            .execute_composer_command(
                &first,
                DesktopComposerCommand::SetPermission(DesktopExecutionPermission::Readonly),
            )
            .unwrap();

        assert_eq!(updated.revision, 1);
        assert_eq!(configured.revision, 2);
        assert_eq!(configured.content, "  ship native  ");
        assert_eq!(configured.permission, DesktopExecutionPermission::Readonly);
        assert_eq!(application.composer_state(&second).unwrap().revision, 0);
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::ComposerChanged { ref task_id, revision: 1 } if task_id == &first
        ));
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::ComposerChanged { ref task_id, revision: 2 } if task_id == &first
        ));
    }

    #[test]
    fn turn_request_uses_composer_settings_and_trims_only_the_dispatched_content() {
        let task_id = TaskId::new("composer-request").unwrap();
        let state = DesktopComposerState {
            task_id: task_id.clone(),
            revision: 8,
            content: "  implement native  ".to_owned(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            model: Some(" gpt-native ".to_owned()),
            reasoning_effort: Some(" high ".to_owned()),
            permission: DesktopExecutionPermission::Full,
            plan_mode: true,
            goal_mode: true,
        };

        let request = state.turn_request();

        assert_eq!(request.task_id, task_id);
        assert_eq!(request.content, "implement native");
        assert_eq!(request.model.as_deref(), Some(" gpt-native "));
        assert_eq!(request.reasoning_effort.as_deref(), Some(" high "));
        assert_eq!(request.permission, DesktopExecutionPermission::Full);
        assert!(request.plan_mode);
        assert!(request.goal_mode);
        assert_eq!(state.content, "  implement native  ");
    }

    #[test]
    fn dispatched_payload_clear_does_not_erase_a_concurrent_draft() {
        let task_id = TaskId::new("composer-concurrent").unwrap();
        let store = DesktopComposerStore::in_memory().unwrap();
        let (sent, _) = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetContent("first".to_owned()),
            )
            .unwrap();
        store
            .execute(
                &task_id,
                DesktopComposerCommand::SetContent("next".to_owned()),
            )
            .unwrap();

        assert_eq!(
            store
                .clear_dispatched_payload(&task_id, sent.revision)
                .unwrap(),
            None
        );
        assert_eq!(store.snapshot(&task_id).unwrap().content, "next");
    }

    #[test]
    fn conversation_reference_selection_is_revision_safe_and_clears_with_the_turn() {
        let task_id = TaskId::new("composer-reference").unwrap();
        let store = DesktopComposerStore::in_memory().unwrap();
        let reference = ChatConversationReference {
            task_id: "related-task".to_owned(),
            title: "相关任务".to_owned(),
            route: "/chats/related-task".to_owned(),
            project_id: None,
            project_name: None,
        };
        let selected = store
            .execute(
                &task_id,
                DesktopComposerCommand::ApplyConversationReference {
                    expected_revision: 0,
                    content: "继续实现 ".to_owned(),
                    reference: reference.clone(),
                },
            )
            .unwrap()
            .0;

        assert_eq!(selected.revision, 1);
        assert_eq!(selected.conversation_references, vec![reference.clone()]);
        assert!(matches!(
            store.execute(
                &task_id,
                DesktopComposerCommand::ApplyConversationReference {
                    expected_revision: 0,
                    content: String::new(),
                    reference,
                },
            ),
            Err(DesktopComposerError::RevisionConflict {
                expected: 0,
                actual: 1
            })
        ));
        let cleared = store
            .clear_dispatched_payload(&task_id, selected.revision)
            .unwrap()
            .unwrap();
        assert!(cleared.conversation_references.is_empty());
        assert!(cleared.content.is_empty());
    }

    #[test]
    fn external_guide_submission_uses_the_staged_composer_revision_atomically() {
        let (application, task_id, _) = application();
        let staged = application
            .execute_composer_command(
                &task_id,
                DesktopComposerCommand::SetContent("补充原生恢复测试".to_owned()),
            )
            .unwrap();

        let guide = application
            .submit_composer_guide(
                staged.revision,
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: "补充原生恢复测试".to_owned(),
                    priority: DesktopTodoPriority::Normal,
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                },
            )
            .unwrap();

        assert_eq!(guide.text, "补充原生恢复测试");
        assert_eq!(application.list_task_todos(&task_id).unwrap(), vec![guide]);
        let cleared = application.composer_state(&task_id).unwrap();
        assert!(cleared.content.is_empty());
        assert_eq!(cleared.revision, staged.revision + 1);

        let newer = application
            .execute_composer_command(
                &task_id,
                DesktopComposerCommand::SetContent("不要清除的新草稿".to_owned()),
            )
            .unwrap();
        let error = application
            .submit_composer_guide(
                staged.revision,
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: "过期引导".to_owned(),
                    priority: DesktopTodoPriority::Normal,
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                },
            )
            .unwrap_err();

        assert!(matches!(
            error,
            DesktopApplicationError::Composer(DesktopComposerError::RevisionConflict {
                expected,
                actual,
            }) if expected == staged.revision && actual == newer.revision
        ));
        assert_eq!(
            application.composer_state(&task_id).unwrap().content,
            "不要清除的新草稿"
        );
        assert_eq!(application.list_task_todos(&task_id).unwrap().len(), 1);
    }

    #[test]
    fn legacy_composer_schema_migrates_conversation_references_without_losing_drafts() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE desktop_composer_drafts (
                  task_id TEXT PRIMARY KEY,
                  revision INTEGER NOT NULL,
                  content TEXT NOT NULL,
                  attachments_json TEXT NOT NULL,
                  model TEXT,
                  reasoning_effort TEXT,
                  permission TEXT NOT NULL,
                  plan_mode INTEGER NOT NULL,
                  goal_mode INTEGER NOT NULL,
                  updated_at INTEGER NOT NULL
                );
                INSERT INTO desktop_composer_drafts VALUES
                  ('legacy-task', 4, 'legacy draft', '[]', NULL, NULL, 'ask', 0, 0, 1);
                "#,
            )
            .unwrap();
        let store = DesktopComposerStore::from_connection(connection).unwrap();
        let task_id = TaskId::new("legacy-task").unwrap();

        let state = store.snapshot(&task_id).unwrap();

        assert_eq!(state.revision, 4);
        assert_eq!(state.content, "legacy draft");
        assert!(state.conversation_references.is_empty());
    }

    #[test]
    fn bare_slash_command_records_real_timeline_and_clears_the_draft() {
        let (application, task_id, _) = application();
        application
            .execute_composer_command(
                &task_id,
                DesktopComposerCommand::SetContent("/status".to_owned()),
            )
            .unwrap();

        let submitted = application.submit_composer(&task_id).unwrap();

        let DesktopComposerSubmission::Command(execution) = submitted else {
            panic!("bare slash command must execute locally");
        };
        assert_eq!(execution.command_id, "native:status");
        let composer = application.composer_state(&task_id).unwrap();
        assert!(composer.content.is_empty());
        assert_eq!(composer.revision, 2);
        let timeline = application
            .authority()
            .shared_runtime()
            .inner()
            .product_timeline_for_task(&task_id);
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].kind, "command");
        assert_eq!(timeline[0].title, "/status");
        assert_eq!(
            timeline[0]
                .payload
                .get("subkind")
                .and_then(serde_json::Value::as_str),
            Some("slash_command")
        );
    }

    #[test]
    fn composer_draft_and_modes_survive_application_restart() {
        let home = tempfile::tempdir().unwrap();
        let config =
            DesktopApplicationConfig::new(home.path(), "desktop-composer-persistence").unwrap();
        let task_id = TaskId::new("persistent-composer-task").unwrap();
        {
            let authority = ServiceAuthority::bootstrap_with_home(home.path()).unwrap();
            authority
                .client()
                .unwrap()
                .products()
                .create_entity(ProductEntity::Task(
                    ProductTask::new(task_id.clone(), None, "Persistent composer").unwrap(),
                ))
                .unwrap();
            let application =
                DesktopApplication::from_authority(config.clone(), authority, Arc::new(NoopHost))
                    .unwrap();
            application
                .execute_composer_command(
                    &task_id,
                    DesktopComposerCommand::SetContent("unfinished native draft".to_owned()),
                )
                .unwrap();
            application
                .execute_composer_command(
                    &task_id,
                    DesktopComposerCommand::SetPermission(DesktopExecutionPermission::Readonly),
                )
                .unwrap();
            application
                .execute_composer_command(&task_id, DesktopComposerCommand::SetPlanMode(true))
                .unwrap();
        }

        let authority = ServiceAuthority::bootstrap_with_home(home.path()).unwrap();
        let restarted =
            DesktopApplication::from_authority(config, authority, Arc::new(NoopHost)).unwrap();
        let restored = restarted.composer_state(&task_id).unwrap();

        assert_eq!(restored.content, "unfinished native draft");
        assert_eq!(restored.permission, DesktopExecutionPermission::Readonly);
        assert!(restored.plan_mode);
        assert_eq!(restored.revision, 3);
    }
}
