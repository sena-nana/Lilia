use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    AutomationBeginRunInput, AutomationExecutionRepository, AutomationExecutionTransition,
    AutomationRunDetail, AutomationRunSummary, AutomationSaveDraftInput, AutomationStore,
    AutomationStoreError, AutomationWorkflow, AutomationWorkflowVersion, SqliteAutomationStore,
};
use crate::{DesktopApplication, DesktopEventBus, DesktopEventKind};

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
        self.inner.events.publish(
            self.inner.source_instance.clone(),
            DesktopEventKind::AutomationRunChanged {
                automation_id: detail.run.workflow_id.clone(),
                run_id: detail.run.id.clone(),
                status: detail.run.status,
            },
        );
        Ok(detail)
    }
}

struct DesktopAutomationServiceInner {
    store: Mutex<Box<dyn AutomationStore>>,
    events: DesktopEventBus,
    source_instance: String,
}

impl DesktopAutomationService {
    pub fn open(
        path: impl AsRef<Path>,
        events: DesktopEventBus,
        source_instance: impl Into<String>,
    ) -> Result<Self, DesktopAutomationError> {
        Self::from_store(SqliteAutomationStore::open(path)?, events, source_instance)
    }

    pub fn in_memory(
        events: DesktopEventBus,
        source_instance: impl Into<String>,
    ) -> Result<Self, DesktopAutomationError> {
        Self::from_store(SqliteAutomationStore::in_memory()?, events, source_instance)
    }

    pub fn from_store(
        store: impl AutomationStore + 'static,
        events: DesktopEventBus,
        source_instance: impl Into<String>,
    ) -> Result<Self, DesktopAutomationError> {
        let source_instance = source_instance.into();
        if source_instance.trim().is_empty() {
            return Err(DesktopAutomationError::EmptySourceInstance);
        }
        Ok(Self {
            inner: Arc::new(DesktopAutomationServiceInner {
                store: Mutex::new(Box::new(store)),
                events,
                source_instance,
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
        self.inner.events.publish(
            self.inner.source_instance.clone(),
            DesktopEventKind::AutomationRunChanged {
                automation_id: detail.run.workflow_id.clone(),
                run_id: detail.run.id.clone(),
                status: detail.run.status,
            },
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
        self.inner.events.publish(
            self.inner.source_instance.clone(),
            DesktopEventKind::AutomationChanged { automation_id },
        );
    }

    fn store(&self) -> Result<MutexGuard<'_, Box<dyn AutomationStore>>, DesktopAutomationError> {
        self.inner
            .store
            .lock()
            .map_err(|_| DesktopAutomationError::StateUnavailable)
    }
}

impl DesktopApplication {
    pub fn automation_service(&self) -> DesktopAutomationService {
        self.inner.automation.clone()
    }

    pub fn list_automation_workflows(
        &self,
    ) -> Result<Vec<AutomationWorkflow>, DesktopAutomationError> {
        self.inner.automation.list_workflows()
    }

    pub fn automation_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Option<AutomationWorkflow>, DesktopAutomationError> {
        self.inner.automation.workflow(workflow_id)
    }

    pub fn save_automation_draft(
        &self,
        input: AutomationSaveDraftInput,
    ) -> Result<AutomationWorkflow, DesktopAutomationError> {
        self.inner.automation.save_draft(input)
    }

    pub fn publish_automation(
        &self,
        workflow_id: &str,
    ) -> Result<AutomationWorkflowVersion, DesktopAutomationError> {
        self.inner.automation.publish(workflow_id)
    }

    pub fn set_automation_enabled(
        &self,
        workflow_id: &str,
        enabled: bool,
    ) -> Result<AutomationWorkflow, DesktopAutomationError> {
        self.inner.automation.set_enabled(workflow_id, enabled)
    }

    pub fn delete_automation(&self, workflow_id: &str) -> Result<(), DesktopAutomationError> {
        self.inner.automation.delete_workflow(workflow_id)
    }

    pub fn begin_automation_run(
        &self,
        input: AutomationBeginRunInput,
    ) -> Result<AutomationRunDetail, DesktopAutomationError> {
        self.inner.automation.try_begin_run(input)
    }

    pub fn list_automation_runs(
        &self,
        workflow_id: Option<&str>,
    ) -> Result<Vec<AutomationRunSummary>, DesktopAutomationError> {
        self.inner.automation.list_runs(workflow_id)
    }

    pub fn automation_run_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<AutomationRunDetail>, DesktopAutomationError> {
        self.inner.automation.run_detail(run_id)
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
        AutomationNode, AutomationNodePosition, AutomationScopeFilter, AutomationSignalEnvelope,
        AutomationStoreError,
    };

    fn service() -> (DesktopAutomationService, crate::DesktopEventSubscription) {
        let events = DesktopEventBus::new();
        let subscription = events.subscribe();
        (
            DesktopAutomationService::in_memory(events, "native-test").unwrap(),
            subscription,
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
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::AutomationChanged { automation_id: Some(ref id) }
                if id == &workflow.id
        ));

        service.publish(&workflow.id).unwrap();
        events.recv().unwrap();
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
        assert!(matches!(
            events.recv().unwrap().kind,
            DesktopEventKind::AutomationRunChanged {
                automation_id,
                run_id,
                status: super::super::AutomationRunStatus::Running,
            } if automation_id == workflow.id && run_id == detail.run.id
        ));
    }

    #[test]
    fn failed_mutation_does_not_publish_a_success_event() {
        let (service, events) = service();
        let error = service.publish("missing").unwrap_err();
        assert!(matches!(
            error,
            DesktopAutomationError::Store(AutomationStoreError::WorkflowNotFound { .. })
        ));
        assert!(events.try_recv().is_err());
    }
}
