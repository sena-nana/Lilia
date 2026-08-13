use rusqlite::{Transaction, TransactionBehavior};

use crate::agent::turn_content_with_references;
use crate::composer::DesktopComposerStore;
use crate::legacy_database::SharedLegacyConnection;
use crate::todo::{guide_message, DesktopTodoStore};
use crate::turn_queue::DesktopTurnQueueStore;
use crate::{
    ChatAttachment, DesktopComposerState, DesktopGuideDispatchWindow, DesktopTaskTodo,
    DesktopTodoCreate, DesktopTodoError, DesktopTodoGuideStatus, DesktopTodoSource,
    DesktopTurnRequest,
};

pub(crate) struct DesktopGuideQueueInput {
    pub turn_id: String,
    pub request: DesktopTurnRequest,
}

pub(crate) struct DesktopQueuedGuide {
    pub guide: DesktopTaskTodo,
    pub turn_id: String,
    pub request: DesktopTurnRequest,
}

pub(crate) struct DesktopGuideSubmissionCommit {
    pub guide: DesktopTaskTodo,
    pub cleared: Option<DesktopComposerState>,
    pub queued: Option<DesktopQueuedGuide>,
}

pub(crate) struct DesktopSubmissionStore {
    connection: SharedLegacyConnection,
}

impl DesktopSubmissionStore {
    pub(crate) fn new(connection: SharedLegacyConnection) -> Self {
        Self { connection }
    }

    pub(crate) fn commit_turn(
        &self,
        composer: &DesktopComposerState,
        turn_id: &str,
        request: &DesktopTurnRequest,
    ) -> Result<Option<DesktopComposerState>, DesktopSubmissionError> {
        let mut connection =
            self.connection
                .lock()
                .map_err(|_| DesktopSubmissionError::Storage {
                    operation: "lock direct Composer submission",
                    message: "connection lock poisoned".to_owned(),
                })?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopSubmissionError::Storage {
                operation: "begin direct Composer submission",
                message: error.to_string(),
            })?;
        let cleared = DesktopComposerStore::clear_dispatched_payload_in(
            &transaction,
            &composer.task_id,
            composer.revision,
        )?;
        DesktopTurnQueueStore::enqueue_in(&transaction, turn_id, request)?;
        commit(transaction, "commit direct Composer submission")?;
        Ok(cleared)
    }

    pub(crate) fn commit_composer_clear(
        &self,
        composer: &DesktopComposerState,
    ) -> Result<Option<DesktopComposerState>, DesktopSubmissionError> {
        let mut connection =
            self.connection
                .lock()
                .map_err(|_| DesktopSubmissionError::Storage {
                    operation: "lock local Composer submission",
                    message: "connection lock poisoned".to_owned(),
                })?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopSubmissionError::Storage {
                operation: "begin local Composer submission",
                message: error.to_string(),
            })?;
        let cleared = DesktopComposerStore::clear_dispatched_payload_in(
            &transaction,
            &composer.task_id,
            composer.revision,
        )?;
        commit(transaction, "commit local Composer submission")?;
        Ok(cleared)
    }

    pub(crate) fn commit_guide(
        &self,
        composer: &DesktopComposerState,
        guide_id: &str,
        input: DesktopTodoCreate,
        queue: Option<DesktopGuideQueueInput>,
    ) -> Result<DesktopGuideSubmissionCommit, DesktopSubmissionError> {
        let mut connection =
            self.connection
                .lock()
                .map_err(|_| DesktopSubmissionError::Storage {
                    operation: "lock Composer Guide submission",
                    message: "connection lock poisoned".to_owned(),
                })?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| DesktopSubmissionError::Storage {
                operation: "begin Composer Guide submission",
                message: error.to_string(),
            })?;
        let (guide, _) = DesktopTodoStore::create_idempotent_in(
            &transaction,
            guide_id,
            input,
            DesktopTodoSource::Lilia,
            Some(DesktopTodoGuideStatus::Pending),
        )?;
        let cleared = DesktopComposerStore::clear_dispatched_payload_in(
            &transaction,
            &composer.task_id,
            composer.revision,
        )?;
        let queued = queue
            .map(|mut queue| {
                let selected = DesktopTodoStore::select_pending_guide_from(
                    &transaction,
                    &composer.task_id,
                    DesktopGuideDispatchWindow::User,
                )?
                .ok_or_else(|| DesktopSubmissionError::Storage {
                    operation: "select immediate Composer Guide",
                    message: "no pending Guide was available after insertion".to_owned(),
                })?;
                queue.request.content = guide_message(&selected);
                queue.request.attachments = selected
                    .attachments
                    .iter()
                    .cloned()
                    .map(|value| {
                        serde_json::from_value::<ChatAttachment>(value).map_err(|error| {
                            DesktopTodoError::InvalidAttachment {
                                guide_id: selected.id.clone(),
                                message: error.to_string(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                queue.request.conversation_references = selected.conversation_references.clone();
                queue.request.workflow = selected.workflow.clone();
                queue.request.guide_id = Some(selected.id.clone());
                queue.request.content = turn_content_with_references(&queue.request);
                DesktopTurnQueueStore::enqueue_in(&transaction, &queue.turn_id, &queue.request)?;
                let selected = DesktopTodoStore::set_guide_status_in(
                    &transaction,
                    &selected.id,
                    DesktopTodoGuideStatus::Queued,
                )?
                .ok_or_else(|| DesktopSubmissionError::Storage {
                    operation: "queue immediate Composer Guide",
                    message: "selected Guide disappeared during submission".to_owned(),
                })?;
                Ok::<_, DesktopSubmissionError>(DesktopQueuedGuide {
                    guide: selected,
                    turn_id: queue.turn_id,
                    request: queue.request,
                })
            })
            .transpose()?;
        commit(transaction, "commit Composer Guide submission")?;
        Ok(DesktopGuideSubmissionCommit {
            guide,
            cleared,
            queued,
        })
    }
}

fn commit(
    transaction: Transaction<'_>,
    operation: &'static str,
) -> Result<(), DesktopSubmissionError> {
    transaction
        .commit()
        .map_err(|error| DesktopSubmissionError::Storage {
            operation,
            message: error.to_string(),
        })
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopSubmissionError {
    #[error(transparent)]
    Composer(#[from] crate::DesktopComposerError),
    #[error(transparent)]
    Todo(#[from] crate::DesktopTodoError),
    #[error(transparent)]
    TurnQueue(#[from] crate::DesktopTurnQueueError),
    #[error("desktop submission storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lilia_contracts::TaskId;
    use rusqlite::Connection;

    use super::*;
    use crate::legacy_database::in_memory_shared_legacy_connection;
    use crate::{DesktopExecutionPermission, DesktopTodoPriority};

    fn stores() -> (
        SharedLegacyConnection,
        DesktopSubmissionStore,
        DesktopTurnQueueStore,
    ) {
        let connection = in_memory_shared_legacy_connection().unwrap();
        DesktopComposerStore::from_shared(connection.clone()).unwrap();
        DesktopTodoStore::from_shared(connection.clone()).unwrap();
        let queue = DesktopTurnQueueStore::from_shared(connection.clone()).unwrap();
        let submissions = DesktopSubmissionStore::new(connection.clone());
        (connection, submissions, queue)
    }

    fn file_stores(
        path: &std::path::Path,
    ) -> (
        SharedLegacyConnection,
        DesktopSubmissionStore,
        DesktopTurnQueueStore,
    ) {
        let connection = crate::legacy_database::open_shared_legacy_connection(path).unwrap();
        connection
            .lock()
            .unwrap()
            .busy_timeout(Duration::from_millis(25))
            .unwrap();
        DesktopComposerStore::from_shared(connection.clone()).unwrap();
        DesktopTodoStore::from_shared(connection.clone()).unwrap();
        let queue = DesktopTurnQueueStore::from_shared(connection.clone()).unwrap();
        let submissions = DesktopSubmissionStore::new(connection.clone());
        (connection, submissions, queue)
    }

    fn composer(task_id: &TaskId, content: &str, revision: u64) -> DesktopComposerState {
        DesktopComposerState {
            task_id: task_id.clone(),
            revision,
            content: content.to_owned(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            model: Some("native-model".to_owned()),
            reasoning_effort: Some("high".to_owned()),
            permission: DesktopExecutionPermission::Ask,
            plan_mode: false,
            goal_mode: false,
        }
    }

    fn save(connection: &SharedLegacyConnection, state: &DesktopComposerState) {
        let connection = connection.lock().unwrap();
        DesktopComposerStore::save_to(&connection, state).unwrap();
    }

    #[test]
    fn direct_submission_clears_composer_and_enqueues_turn_in_one_commit() {
        let (connection, submissions, queue) = stores();
        let task_id = TaskId::new("atomic-direct").unwrap();
        let state = composer(&task_id, "ship the native path", 4);
        save(&connection, &state);
        let request = state.turn_request();

        let cleared = submissions
            .commit_turn(&state, "turn-atomic", &request)
            .unwrap()
            .unwrap();

        assert_eq!(cleared.revision, 5);
        assert!(cleared.content.is_empty());
        let queued = queue.list(&task_id).unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].turn_id, "turn-atomic");
        assert_eq!(queued[0].request, request);
    }

    #[test]
    fn turn_enqueue_conflict_rolls_back_composer_clear() {
        let (connection, submissions, queue) = stores();
        let task_id = TaskId::new("atomic-conflict").unwrap();
        let state = composer(&task_id, "must remain", 7);
        save(&connection, &state);
        queue
            .enqueue(
                "turn-conflict",
                &DesktopTurnRequest::new(task_id.clone(), "existing"),
            )
            .unwrap();

        let error = submissions
            .commit_turn(
                &state,
                "turn-conflict",
                &DesktopTurnRequest::new(task_id.clone(), "replacement"),
            )
            .unwrap_err();

        assert!(matches!(error, DesktopSubmissionError::TurnQueue(_)));
        let connection = connection.lock().unwrap();
        let restored = DesktopComposerStore::snapshot_from(&connection, &task_id).unwrap();
        assert_eq!(restored, state);
    }

    #[test]
    fn external_database_writer_keeps_submission_atomic_and_retryable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("busy.db");
        let (connection, submissions, queue) = file_stores(&path);
        let task_id = TaskId::new("atomic-busy").unwrap();
        let state = composer(&task_id, "retry after writer lock", 11);
        save(&connection, &state);
        let request = state.turn_request();
        let external = Connection::open(&path).unwrap();
        external.execute_batch("BEGIN IMMEDIATE").unwrap();

        let error = submissions
            .commit_turn(&state, "turn-busy", &request)
            .unwrap_err();
        assert!(error.to_string().contains("locked") || error.to_string().contains("busy"));
        assert_eq!(
            DesktopComposerStore::snapshot_from(&connection.lock().unwrap(), &task_id).unwrap(),
            state
        );
        assert!(queue.list(&task_id).unwrap().is_empty());

        external.execute_batch("ROLLBACK").unwrap();
        let cleared = submissions
            .commit_turn(&state, "turn-busy", &request)
            .unwrap()
            .unwrap();
        assert!(cleared.content.is_empty());
        assert_eq!(queue.list(&task_id).unwrap().len(), 1);
    }

    #[test]
    fn database_full_rolls_back_composer_and_allows_retry_after_capacity_returns() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("full.db");
        let (connection, submissions, queue) = file_stores(&path);
        let task_id = TaskId::new("atomic-full").unwrap();
        let state = composer(&task_id, "retain after full database", 13);
        save(&connection, &state);
        {
            let connection = connection.lock().unwrap();
            connection.execute_batch("VACUUM").unwrap();
            let page_count = connection
                .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
                .unwrap();
            connection
                .query_row(
                    &format!("PRAGMA max_page_count = {page_count}"),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap();
        }
        let mut request = state.turn_request();
        request.content = "x".repeat(2 * 1024 * 1024);

        let error = submissions
            .commit_turn(&state, "turn-full", &request)
            .unwrap_err();
        assert!(error.to_string().contains("full"));
        assert_eq!(
            DesktopComposerStore::snapshot_from(&connection.lock().unwrap(), &task_id).unwrap(),
            state
        );
        assert!(queue.list(&task_id).unwrap().is_empty());

        connection
            .lock()
            .unwrap()
            .query_row("PRAGMA max_page_count = 1073741823", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        let cleared = submissions
            .commit_turn(&state, "turn-full", &request)
            .unwrap()
            .unwrap();
        assert!(cleared.content.is_empty());
        assert_eq!(queue.list(&task_id).unwrap().len(), 1);
    }

    #[test]
    fn guide_submission_creates_pending_guide_and_clears_composer_atomically() {
        let (connection, submissions, queue) = stores();
        let task_id = TaskId::new("atomic-guide").unwrap();
        let state = composer(&task_id, "guide the active turn", 2);
        save(&connection, &state);

        let committed = submissions
            .commit_guide(
                &state,
                "guide-atomic",
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: state.content.clone(),
                    priority: DesktopTodoPriority::Normal,
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    workflow: None,
                },
                None,
            )
            .unwrap();

        assert_eq!(committed.guide.id, "guide-atomic");
        assert_eq!(
            committed.guide.guide_status,
            Some(DesktopTodoGuideStatus::Pending)
        );
        assert!(committed.cleared.unwrap().content.is_empty());
        assert!(committed.queued.is_none());
        assert!(queue.list(&task_id).unwrap().is_empty());
        let connection = connection.lock().unwrap();
        assert_eq!(
            DesktopTodoStore::get_from(&connection, "guide-atomic")
                .unwrap()
                .unwrap(),
            committed.guide
        );
    }

    #[test]
    fn immediate_guide_submission_queues_the_fifo_guide_in_the_same_commit() {
        let (connection, submissions, queue) = stores();
        let task_id = TaskId::new("atomic-immediate-guide").unwrap();
        let state = composer(&task_id, "new Guide", 3);
        save(&connection, &state);
        {
            let connection = connection.lock().unwrap();
            DesktopTodoStore::create_idempotent_in(
                &connection,
                "guide-existing",
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: "existing Guide".to_owned(),
                    priority: DesktopTodoPriority::Normal,
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    workflow: Some(lilia_contracts::LiliaAgentWorkflow::LiliaCompact),
                },
                DesktopTodoSource::Lilia,
                Some(DesktopTodoGuideStatus::Pending),
            )
            .unwrap();
        }
        let mut request = state.turn_request();
        request.workspace_path = Some("C:/native-workspace".to_owned());

        let committed = submissions
            .commit_guide(
                &state,
                "guide-new",
                DesktopTodoCreate {
                    task_id: task_id.clone(),
                    text: state.content.clone(),
                    priority: DesktopTodoPriority::Normal,
                    attachments: Vec::new(),
                    conversation_references: Vec::new(),
                    workflow: None,
                },
                Some(DesktopGuideQueueInput {
                    turn_id: "turn-guide".to_owned(),
                    request,
                }),
            )
            .unwrap();

        assert_eq!(committed.guide.id, "guide-new");
        assert_eq!(
            committed.guide.guide_status,
            Some(DesktopTodoGuideStatus::Pending)
        );
        let queued = committed.queued.unwrap();
        assert_eq!(queued.guide.id, "guide-existing");
        assert_eq!(
            queued.guide.guide_status,
            Some(DesktopTodoGuideStatus::Queued)
        );
        assert_eq!(queued.request.guide_id.as_deref(), Some("guide-existing"));
        assert_eq!(
            queued.request.workflow,
            Some(lilia_contracts::LiliaAgentWorkflow::LiliaCompact)
        );
        assert_eq!(queue.list(&task_id).unwrap()[0].request, queued.request);
        assert!(committed.cleared.unwrap().content.is_empty());
    }
}
