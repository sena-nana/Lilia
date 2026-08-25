use lilia_contracts::{ProductTask, ProductTaskStatus, TaskId};
use uuid::Uuid;

use crate::application::submission::DesktopGuideQueueInput;
use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopEventKind, DesktopSessionBranchAnchor,
    DesktopTaskPatch, DesktopTaskTodo, DesktopTodoCreate, DesktopTodoPriority, DesktopTurnDispatch,
    DesktopTurnRequest,
};

pub(crate) use lilia_feature_composer::ComposerStore as DesktopComposerStore;
pub use lilia_feature_composer::{
    ensure_expected_revision, ComposerCommand as DesktopComposerCommand,
    ComposerError as DesktopComposerError, ComposerState as DesktopComposerState,
};

#[derive(Clone, Debug, PartialEq)]
pub enum DesktopComposerSubmission {
    Turn(DesktopTurnDispatch),
    Command(crate::application::DesktopSlashCommandExecution),
    Guide {
        guide: DesktopTaskTodo,
        turn: Option<DesktopTurnDispatch>,
    },
}

/// Projects a composer draft onto the turn vocabulary owned by the
/// agent-session domain.
pub trait DesktopComposerTurnRequest {
    fn turn_request(&self) -> DesktopTurnRequest;
}

impl DesktopComposerTurnRequest for DesktopComposerState {
    fn turn_request(&self) -> DesktopTurnRequest {
        let mut request = DesktopTurnRequest::new(self.task_id.clone(), self.content.trim())
            .with_attachments(self.attachments.clone())
            .with_conversation_references(self.conversation_references.clone());
        request.model = self.model.clone();
        request.workflow = self.workflow.clone();
        request.reasoning_effort = self.reasoning_effort.clone();
        request.permission = self.permission;
        request.plan_mode = self.plan_mode;
        request.goal_mode = self.goal_mode;
        request.allow_auto_turn_decision = true;
        request
    }
}

impl DesktopApplication {
    /// Materializes a host-owned transient conversation draft as a product
    /// task and durable composer draft. Until this call succeeds, no task is
    /// visible to other hosts.
    pub fn materialize_task_draft(
        &self,
        input: crate::application::DesktopTaskCreate,
        draft: DesktopComposerState,
    ) -> Result<ProductTask, DesktopApplicationError> {
        if input.id != draft.task_id {
            return Err(DesktopApplicationError::InvalidInput {
                field: "draft.task_id",
                message: "must match the task id reserved by the draft".to_owned(),
            });
        }
        self.inner.composers.save(&draft)?;
        let task = match self.create_task(input) {
            Ok(task) => task,
            Err(error) => {
                if self.get_task(&draft.task_id).is_err() {
                    let _ = self.inner.composers.remove(&draft.task_id);
                }
                return Err(error);
            }
        };
        self.emit_event(DesktopEventKind::ComposerChanged {
            task_id: task.id.clone(),
            revision: draft.revision,
        });
        Ok(task)
    }

    pub fn composer_state(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopComposerState, DesktopApplicationError> {
        self.get_task(task_id)?;
        Ok(self.inner.composers.snapshot(task_id)?)
    }

    pub fn execute_composer_command(
        &self,
        task_id: &TaskId,
        command: DesktopComposerCommand,
    ) -> Result<DesktopComposerState, DesktopApplicationError> {
        self.get_task(task_id)?;
        let (state, changed) = self.inner.composers.execute(task_id, command)?;
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
        self.start_composer_turn_with_session_branch(task_id, None)
    }

    fn start_composer_turn_with_session_branch(
        &self,
        task_id: &TaskId,
        session_branch: Option<DesktopSessionBranchAnchor>,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        self.ensure_initial_worktree_ready(task_id)?;
        let composer = self.composer_state(task_id)?;
        let mut request = composer.turn_request();
        request.session_branch = session_branch;
        request.workspace_path = self.task_workspace_path(task_id)?;
        let request = self.prepare_task_turn_request(request)?;
        self.promote_composer_task_if_draft(&composer)?;
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
        self.submit_composer_with_optional_session_branch(task_id, None)
    }

    pub fn submit_composer_with_session_branch(
        &self,
        task_id: &TaskId,
        session_branch: DesktopSessionBranchAnchor,
    ) -> Result<DesktopComposerSubmission, DesktopApplicationError> {
        self.submit_composer_with_optional_session_branch(task_id, Some(session_branch))
    }

    fn submit_composer_with_optional_session_branch(
        &self,
        task_id: &TaskId,
        session_branch: Option<DesktopSessionBranchAnchor>,
    ) -> Result<DesktopComposerSubmission, DesktopApplicationError> {
        self.ensure_initial_worktree_ready(task_id)?;
        let submission = self
            .inner
            .turn_submission
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("turn submission"))?;
        let composer = self.composer_state(task_id)?;
        if session_branch.is_none()
            && composer.attachments.is_empty()
            && composer.conversation_references.is_empty()
        {
            if let Some(execution) = self.resolve_task_slash_command(task_id, &composer.content)? {
                self.promote_composer_task_if_draft(&composer)?;
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
                .start_composer_turn_with_session_branch(task_id, session_branch)
                .map(DesktopComposerSubmission::Turn);
        }
        if session_branch.is_some() {
            return Err(DesktopApplicationError::InvalidInput {
                field: "session_branch",
                message: "task must be idle before continuing from an earlier turn".to_owned(),
            });
        }

        let guide_text = crate::application::agent::turn_content_with_references(&composer.turn_request());
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
            "waiting_approval" | "waiting_interaction"
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
                    workflow: composer.workflow.clone(),
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

    fn promote_composer_task_if_draft(
        &self,
        composer: &DesktopComposerState,
    ) -> Result<(), DesktopApplicationError> {
        let task = self.get_task(&composer.task_id)?;
        if task.status != ProductTaskStatus::Draft {
            return Ok(());
        }
        let title = (task.title.trim() == "新对话")
            .then(|| composer.submission_title())
            .flatten();
        self.update_task(
            &composer.task_id,
            DesktopTaskPatch {
                title,
                status: Some(ProductTaskStatus::Waiting),
                ..DesktopTaskPatch::default()
            },
        )?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_contracts::{
        ChatConversationReference, LiliaAgentWorkflow, ProductEntity, ProductTask,
    };
    use lilia_service::ServiceAuthority;

    use lilia_storage::Db;

    use crate::application::DesktopExecutionPermission;

    use super::*;
    use crate::application::{
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
    fn transient_composer_materializes_only_when_the_host_promotes_it() {
        let (application, _, parent_id) = application();
        let input = crate::application::DesktopTaskCreate::new(None, "新对话").with_parent(parent_id.clone());
        let task_id = input.id.clone();
        let mut draft = DesktopComposerState::transient(task_id.clone());
        assert!(draft
            .apply_transient_command(DesktopComposerCommand::SetContent(
                "first message".to_owned(),
            ))
            .unwrap());

        assert!(application.get_task(&task_id).is_err());
        let task = application
            .materialize_task_draft(input, draft.clone())
            .unwrap();

        assert_eq!(task.id, task_id);
        assert_eq!(task.parent_id, Some(parent_id));
        assert_eq!(task.status, ProductTaskStatus::Draft);
        assert_eq!(application.composer_state(&task_id).unwrap(), draft);
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
            workflow: Some(LiliaAgentWorkflow::LiliaCompact),
            model: Some(" gpt-native ".to_owned()),
            reasoning_effort: Some(" high ".to_owned()),
            permission: DesktopExecutionPermission::Full,
            plan_mode: true,
            goal_mode: true,
        };

        let request = state.turn_request();

        assert_eq!(request.task_id, task_id);
        assert_eq!(request.content, "implement native");
        assert_eq!(request.workflow, Some(LiliaAgentWorkflow::LiliaCompact));
        assert_eq!(request.model.as_deref(), Some(" gpt-native "));
        assert_eq!(request.reasoning_effort.as_deref(), Some(" high "));
        assert_eq!(request.permission, DesktopExecutionPermission::Full);
        assert!(request.plan_mode);
        assert!(request.goal_mode);
        assert_eq!(state.content, "  implement native  ");
    }

    #[test]
    fn anchored_session_branch_is_validated_and_kept_in_the_durable_turn_request() {
        let (application, task_id, _) = application();
        let mut request = DesktopTurnRequest::new(task_id, "继续处理");
        request.session_branch = Some(DesktopSessionBranchAnchor {
            source_turn_id: "  turn-source  ".to_owned(),
            mode: crate::application::DesktopSessionBranchMode::Continue,
        });

        let prepared = application.prepare_task_turn_request(request).unwrap();
        assert_eq!(
            prepared.session_branch,
            Some(DesktopSessionBranchAnchor {
                source_turn_id: "turn-source".to_owned(),
                mode: crate::application::DesktopSessionBranchMode::Continue,
            })
        );
        let restored: DesktopTurnRequest =
            serde_json::from_value(serde_json::to_value(&prepared).unwrap()).unwrap();
        assert_eq!(restored.session_branch, prepared.session_branch);

        let mut invalid = DesktopTurnRequest::new(prepared.task_id, "继续处理");
        invalid.session_branch = Some(DesktopSessionBranchAnchor {
            source_turn_id: "   ".to_owned(),
            mode: crate::application::DesktopSessionBranchMode::Fork,
        });
        assert!(matches!(
            application.prepare_task_turn_request(invalid),
            Err(DesktopApplicationError::InvalidInput {
                field: "session_branch.source_turn_id",
                ..
            })
        ));
    }

    #[test]
    fn model_selection_updates_model_and_effort_in_one_persisted_revision() {
        let task_id = TaskId::new("composer-model-selection").unwrap();
        let store = DesktopComposerStore::in_memory().unwrap();

        let manual = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetModelSelection {
                    model: Some("  gpt-manual  ".to_owned()),
                    reasoning_effort: Some(" high ".to_owned()),
                },
            )
            .unwrap()
            .0;

        assert_eq!(manual.revision, 1);
        assert_eq!(manual.model.as_deref(), Some("gpt-manual"));
        assert_eq!(manual.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(store.snapshot(&task_id).unwrap(), manual);

        let automatic = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetModelSelection {
                    model: None,
                    reasoning_effort: None,
                },
            )
            .unwrap()
            .0;
        assert_eq!(automatic.revision, 2);
        assert!(automatic.model.is_none());
        assert!(automatic.reasoning_effort.is_none());
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
    fn asynchronous_content_replacement_does_not_overwrite_a_newer_draft() {
        let task_id = TaskId::new("composer-replace-content").unwrap();
        let store = DesktopComposerStore::in_memory().unwrap();
        let original = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetContent("原始提示".to_owned()),
            )
            .unwrap()
            .0;
        let routed = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetWorkflow(Some(LiliaAgentWorkflow::LiliaCompact)),
            )
            .unwrap()
            .0;
        let optimized = store
            .execute(
                &task_id,
                DesktopComposerCommand::ApplyPromptOptimization {
                    expected_revision: routed.revision,
                    content: "优化后的提示".to_owned(),
                },
            )
            .unwrap()
            .0;
        let newer = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetContent("用户继续输入".to_owned()),
            )
            .unwrap()
            .0;

        assert_eq!(original.revision + 1, routed.revision);
        assert_eq!(optimized.content, "优化后的提示");
        assert!(optimized.workflow.is_none());
        assert!(matches!(
            store.execute(
                &task_id,
                DesktopComposerCommand::ApplyPromptOptimization {
                    expected_revision: optimized.revision,
                    content: "过期优化结果".to_owned(),
                },
            ),
            Err(DesktopComposerError::RevisionConflict { expected, actual })
                if expected == optimized.revision && actual == newer.revision
        ));
        assert_eq!(store.snapshot(&task_id).unwrap().content, "用户继续输入");
    }

    #[test]
    fn slash_workflow_replaces_only_the_trigger_text_at_the_expected_revision() {
        let task_id = TaskId::new("composer-slash-workflow").unwrap();
        let store = DesktopComposerStore::in_memory().unwrap();
        let draft = store
            .execute(
                &task_id,
                DesktopComposerCommand::SetContent("/frontend".to_owned()),
            )
            .unwrap()
            .0;

        let selected = store
            .execute(
                &task_id,
                DesktopComposerCommand::ApplySlashWorkflow {
                    expected_revision: draft.revision,
                    workflow: LiliaAgentWorkflow::LiliaTaskWorkflow {
                        kind: "frontend".to_owned(),
                        instructions: None,
                    },
                },
            )
            .unwrap()
            .0;

        assert!(selected.content.is_empty());
        assert_eq!(
            selected.workflow,
            Some(LiliaAgentWorkflow::LiliaTaskWorkflow {
                kind: "frontend".to_owned(),
                instructions: None,
            })
        );
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
                    workflow: None,
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
                    workflow: None,
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
    fn legacy_composer_schema_migrates_optional_context_without_losing_drafts() {
        let connection = Db::in_memory().unwrap();
        connection
            .lock()
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
        let store = DesktopComposerStore::new(connection).unwrap();
        let task_id = TaskId::new("legacy-task").unwrap();

        let state = store.snapshot(&task_id).unwrap();

        assert_eq!(state.revision, 4);
        assert_eq!(state.content, "legacy draft");
        assert!(state.conversation_references.is_empty());
        assert_eq!(state.workflow, None);
    }

    #[test]
    fn bare_slash_command_records_real_timeline_and_clears_the_draft() {
        let (application, task_id, _) = application();
        application
            .update_task(
                &task_id,
                DesktopTaskPatch {
                    title: Some("新对话".to_owned()),
                    ..DesktopTaskPatch::default()
                },
            )
            .unwrap();
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
        let task = application.get_task(&task_id).unwrap();
        assert_eq!(task.status, ProductTaskStatus::Waiting);
        assert_eq!(task.title, "/status");
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
