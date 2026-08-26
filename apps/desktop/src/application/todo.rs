use lilia_contracts::{ChatAttachment, TaskId};

use crate::application::composer::DesktopComposerTurnRequest;
use crate::application::{DesktopApplication, DesktopApplicationError, TodosChanged};

pub use lilia_feature_agent_session::{
    guide_message, merge_todos_with_latest_projection, DesktopGuideDispatchResult,
    DesktopGuideDispatchWindow, DesktopTaskTodo, DesktopTodoCreate, DesktopTodoError,
    DesktopTodoGuideStatus, DesktopTodoPriority, DesktopTodoSource, DesktopTodoStore,
    DesktopTodoUpdate,
};

impl DesktopApplication {
    pub fn list_task_todos(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<DesktopTaskTodo>, DesktopApplicationError> {
        self.get_task(task_id)?;
        let stored = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .list(task_id)?;
        let projections = self
            .authority()
            .shared_runtime()
            .inner()
            .product_todos_for_task(task_id);
        Ok(merge_todos_with_latest_projection(stored, &projections))
    }

    pub fn create_task_todo(
        &self,
        input: DesktopTodoCreate,
    ) -> Result<DesktopTaskTodo, DesktopApplicationError> {
        self.get_task(&input.task_id)?;
        let todo = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .create(input)?;
        self.emit_event(TodosChanged {
            task_id: todo.task_id.clone(),
        });
        Ok(todo)
    }

    pub(crate) fn create_task_todo_idempotent(
        &self,
        id: &str,
        input: DesktopTodoCreate,
        source: DesktopTodoSource,
        guide_status: Option<DesktopTodoGuideStatus>,
    ) -> Result<(DesktopTaskTodo, bool), DesktopApplicationError> {
        self.get_task(&input.task_id)?;
        let (todo, inserted) = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .create_idempotent(id, input, source, guide_status)?;
        if inserted {
            self.emit_event(TodosChanged {
                task_id: todo.task_id.clone(),
            });
        }
        Ok((todo, inserted))
    }

    pub fn update_task_todo(
        &self,
        id: &str,
        update: DesktopTodoUpdate,
    ) -> Result<Option<DesktopTaskTodo>, DesktopApplicationError> {
        let todo = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .update(id, update)?;
        if let Some(todo) = &todo {
            self.emit_event(TodosChanged {
                task_id: todo.task_id.clone(),
            });
        }
        Ok(todo)
    }

    pub fn delete_task_todo(&self, id: &str) -> Result<bool, DesktopApplicationError> {
        let task_id = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .delete(id)?;
        if let Some(task_id) = task_id {
            self.emit_event(TodosChanged { task_id });
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn set_task_guide_status(
        &self,
        id: &str,
        status: DesktopTodoGuideStatus,
    ) -> Result<Option<DesktopTaskTodo>, DesktopApplicationError> {
        self.update_task_todo(
            id,
            DesktopTodoUpdate {
                guide_status: Some(status),
                ..DesktopTodoUpdate::default()
            },
        )
    }

    pub fn dispatch_next_task_guide(
        &self,
        task_id: &TaskId,
        window: DesktopGuideDispatchWindow,
    ) -> Result<Option<DesktopGuideDispatchResult>, DesktopApplicationError> {
        let _dispatch = self
            .inner
            .guide_dispatch
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("guide dispatch"))?;
        self.get_task(task_id)?;
        let guide = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .select_pending_guide(task_id, window)?;
        let Some(guide) = guide else {
            return Ok(None);
        };
        self.dispatch_task_guide_value(task_id, guide).map(Some)
    }

    /// Dispatches the exact pending Guide selected by the user.
    pub fn dispatch_task_guide(
        &self,
        task_id: &TaskId,
        guide_id: &str,
    ) -> Result<Option<DesktopGuideDispatchResult>, DesktopApplicationError> {
        let _dispatch = self
            .inner
            .guide_dispatch
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("guide dispatch"))?;
        self.get_task(task_id)?;
        let guide = self
            .inner
            .todos
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("todos"))?
            .select_pending_guide_by_id(task_id, guide_id)?;
        let Some(guide) = guide else {
            return Ok(None);
        };
        self.dispatch_task_guide_value(task_id, guide).map(Some)
    }

    fn dispatch_task_guide_value(
        &self,
        task_id: &TaskId,
        guide: DesktopTaskTodo,
    ) -> Result<DesktopGuideDispatchResult, DesktopApplicationError> {
        let mut request = self.composer_state(task_id)?.turn_request();
        request.content = guide_message(&guide);
        request.attachments = guide
            .attachments
            .iter()
            .cloned()
            .map(|value| {
                serde_json::from_value::<ChatAttachment>(value).map_err(|error| {
                    DesktopTodoError::InvalidAttachment {
                        guide_id: guide.id.clone(),
                        message: error.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        request.conversation_references = guide.conversation_references.clone();
        request.workflow = guide.workflow.clone();
        request.workspace_path = self.task_workspace_path(task_id)?;
        request.guide_id = Some(guide.id.clone());
        let turn = self.start_task_turn(request)?;
        Ok(DesktopGuideDispatchResult { guide, turn })
    }
}
