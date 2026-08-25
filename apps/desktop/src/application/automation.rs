//! Desktop delegation for the automation domain.

use lilia_feature_automation::{
    AutomationBeginRunInput, AutomationRunDetail, AutomationRunStatus, AutomationRunSummary,
    AutomationSaveDraftInput, AutomationWorkflow, AutomationWorkflowVersion,
    DesktopAutomationError, DesktopAutomationService,
};

use crate::application::{DesktopApplication, DesktopEventBus, DesktopEventKind};

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

/// Relays automation changes onto the desktop event bus.
pub(crate) struct BroadcastAutomationEvents {
    events: DesktopEventBus,
    source_instance: String,
}

impl BroadcastAutomationEvents {
    pub(crate) fn new(events: DesktopEventBus, source_instance: impl Into<String>) -> Self {
        Self {
            events,
            source_instance: source_instance.into(),
        }
    }
}

impl lilia_feature_automation::AutomationEvents for BroadcastAutomationEvents {
    fn workflow_changed(&self, automation_id: Option<&str>) {
        self.events.publish(
            self.source_instance.clone(),
            DesktopEventKind::AutomationChanged {
                automation_id: automation_id.map(str::to_owned),
            },
        );
    }

    fn run_changed(&self, automation_id: &str, run_id: &str, status: AutomationRunStatus) {
        self.events.publish(
            self.source_instance.clone(),
            DesktopEventKind::AutomationRunChanged {
                automation_id: automation_id.to_owned(),
                run_id: run_id.to_owned(),
                status,
            },
        );
    }
}
