use std::sync::{Arc, Mutex, MutexGuard};

use lilia_storage::Db;

use super::{
    AutomationBeginRunInput, AutomationExecutionRepository, AutomationExecutionTransition,
    AutomationRunDetail, AutomationRunSummary, AutomationSaveDraftInput, AutomationStore,
    AutomationStoreError, AutomationWorkflow, AutomationWorkflowVersion, SqliteAutomationStore,
};
use crate::AutomationEvents;

#[derive(Clone)]
pub struct DesktopAutomationService {
    inner: Arc<DesktopAutomationServiceInner>,
}

impl AutomationExecutionRepository for DesktopAutomationService {
    fn execution_run_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<AutomationRunDetail>, AutomationStoreError> {
        let store = self.inner.store.lock().map_err(|_| {
            AutomationStoreError::storage("lock automation execution store", "state unavailable")
        })?;
        store.run_detail(run_id)
    }

    fn execution_version(
        &self,
        version_id: &str,
    ) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError> {
        let store = self.inner.store.lock().map_err(|_| {
            AutomationStoreError::storage("lock automation execution store", "state unavailable")
        })?;
        store.version(version_id)
    }

    fn apply_execution_transition(
        &self,
        transition: AutomationExecutionTransition,
    ) -> Result<AutomationRunDetail, AutomationStoreError> {
        let detail = {
            let mut store = self.inner.store.lock().map_err(|_| {
                AutomationStoreError::storage(
                    "lock automation execution store",
                    "state unavailable",
                )
            })?;
            store.apply_execution_transition(transition)?
        };
        self.inner.events.run_changed(
            &detail.run.workflow_id,
            &detail.run.id,
            detail.run.status,
        );
        Ok(detail)
    }
}

struct DesktopAutomationServiceInner {
    store: Mutex<Box<dyn AutomationStore>>,
    events: Arc<dyn AutomationEvents>,
}

impl DesktopAutomationService {
    pub fn from_db(
        db: Db,
        events: Arc<dyn AutomationEvents>,
    ) -> Result<Self, DesktopAutomationError> {
        Self::from_store(SqliteAutomationStore::from_db(db)?, events)
    }

    pub fn in_memory(events: Arc<dyn AutomationEvents>) -> Result<Self, DesktopAutomationError> {
        Self::from_store(SqliteAutomationStore::in_memory()?, events)
    }

    pub fn from_store(
        store: impl AutomationStore + 'static,
        events: Arc<dyn AutomationEvents>,
    ) -> Result<Self, DesktopAutomationError> {
        Ok(Self {
            inner: Arc::new(DesktopAutomationServiceInner {
                store: Mutex::new(Box::new(store)),
                events,
            }),
        })
    }

    pub fn list_workflows(&self) -> Result<Vec<AutomationWorkflow>, DesktopAutomationError> {
        Ok(self.store()?.list_workflows()?)
    }

    pub fn workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<AutomationWorkflow>, DesktopAutomationError> {
        Ok(self.store()?.workflow(workflow_id)?)
    }

    pub fn save_draft(
        &self,
        input: AutomationSaveDraftInput,
    ) -> Result<AutomationWorkflow, DesktopAutomationError> {
        let workflow = self.store()?.save_draft(input)?;
        self.workflow_changed(Some(workflow.id.clone()));
        Ok(workflow)
    }

    pub fn publish(
        &self,
        workflow_id: &str,
    ) -> Result<AutomationWorkflowVersion, DesktopAutomationError> {
        let version = self.store()?.publish(workflow_id)?;
        self.workflow_changed(Some(workflow_id.to_owned()));
        Ok(version)
    }

    pub fn set_enabled(
        &self,
        workflow_id: &str,
        enabled: bool,
    ) -> Result<AutomationWorkflow, DesktopAutomationError> {
        let workflow = self.store()?.set_enabled(workflow_id, enabled)?;
        self.workflow_changed(Some(workflow_id.to_owned()));
        Ok(workflow)
    }

    pub fn delete_workflow(&self, workflow_id: &str) -> Result<(), DesktopAutomationError> {
        self.store()?.delete_workflow(workflow_id)?;
        self.workflow_changed(Some(workflow_id.to_owned()));
        Ok(())
    }

    pub fn version(
        &self,
        version_id: &str,
    ) -> Result<Option<AutomationWorkflowVersion>, DesktopAutomationError> {
        Ok(self.store()?.version(version_id)?)
    }

    pub fn try_begin_run(
        &self,
        input: AutomationBeginRunInput,
    ) -> Result<AutomationRunDetail, DesktopAutomationError> {
        let detail = self.store()?.try_begin_run(input)?;
        self.inner.events.run_changed(
            &detail.run.workflow_id,
            &detail.run.id,
            detail.run.status,
        );
        Ok(detail)
    }

    pub fn list_runs(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<AutomationRunSummary>, DesktopAutomationError> {
        Ok(self.store()?.list_runs(workflow_id)?)
    }

    pub fn run_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<AutomationRunDetail>, DesktopAutomationError> {
        Ok(self.store()?.run_detail(run_id)?)
    }

    fn workflow_changed(&self, automation_id: Option<String>) {
        self.inner.events.workflow_changed(automation_id.as_deref());
    }

    fn store(&self) -> Result<MutexGuard<'_, Box<dyn AutomationStore>>, DesktopAutomationError> {
        self.inner
            .store
            .lock()
            .map_err(|_| DesktopAutomationError::StateUnavailable)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopAutomationError {
    #[error("automation service source instance must not be empty")]
    EmptySourceInstance,
    #[error("desktop automation state is unavailable")]
    StateUnavailable,
    #[error(transparent)]
    Store(#[from] AutomationStoreError),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        AutomationNode, AutomationNodePosition, AutomationNotification, AutomationRunStatus,
        AutomationScopeFilter, AutomationSignalEnvelope, AutomationStoreError,
        RecordingAutomationEvents,
    };

    fn service() -> (DesktopAutomationService, Arc<RecordingAutomationEvents>) {
        let events = Arc::new(RecordingAutomationEvents::default());
        (
            DesktopAutomationService::in_memory(events.clone()).unwrap(),
            events,
        )
    }

    fn draft(id: &str) -> AutomationSaveDraftInput {
        AutomationSaveDraftInput {
            id: Some(id.to_owned()),
            name: "Native automation".to_owned(),
            scope: AutomationScopeFilter::default(),
            nodes: vec![AutomationNode {
                id: "trigger".to_owned(),
                kind: "trigger".to_owned(),
                title: "Trigger".to_owned(),
                position: AutomationNodePosition { x: 0.0, y: 0.0 },
                config: json!({}),
            }],
            edges: Vec::new(),
        }
    }

    #[test]
    fn service_mutations_publish_typed_invalidation_events() {
        let (service, events) = service();
        let workflow = service.save_draft(draft("workflow-1")).unwrap();
        assert_eq!(
            events.take(),
            Some(AutomationNotification::WorkflowChanged {
                automation_id: Some(workflow.id.clone()),
            })
        );

        service.publish(&workflow.id).unwrap();
        events.take().unwrap();
        let detail = service
            .try_begin_run(AutomationBeginRunInput {
                workflow_id: workflow.id.clone(),
                trigger: AutomationSignalEnvelope {
                    id: "signal-1".to_owned(),
                    kind: "manual".to_owned(),
                    project_id: None,
                    task_id: None,
                    backend: None,
                    event_kind: None,
                    automation_run_id: None,
                    payload: json!({}),
                    created_at: 1,
                },
            })
            .unwrap();
        assert_eq!(
            events.take(),
            Some(AutomationNotification::RunChanged {
                automation_id: workflow.id.clone(),
                run_id: detail.run.id.clone(),
                status: AutomationRunStatus::Running,
            })
        );
    }

    #[test]
    fn failed_mutation_does_not_publish_a_success_event() {
        let (service, events) = service();
        let error = service.publish("missing").unwrap_err();
        assert!(matches!(
            error,
            DesktopAutomationError::Store(AutomationStoreError::WorkflowNotFound { .. })
        ));
        assert!(events.is_empty());
    }
}
