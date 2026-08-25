use lilia_contracts::{ProjectId, TaskId};
use lilia_kernel::Event;

/// Where the service reports fact changes. The kernel feature publishes typed
/// events; a host that still runs a legacy broadcast supplies its own sink.
pub trait ProjectTaskEvents: Send + Sync + 'static {
    fn projects_changed(&self);
    fn tasks_changed(&self, project_id: Option<ProjectId>, task_id: Option<TaskId>);
}

/// Discards every notification. Used by tests that assert on stored facts.
pub struct SilentProjectTaskEvents;

impl ProjectTaskEvents for SilentProjectTaskEvents {
    fn projects_changed(&self) {}
    fn tasks_changed(&self, _project_id: Option<ProjectId>, _task_id: Option<TaskId>) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectsChanged;

impl Event for ProjectsChanged {
    const NAME: &'static str = "lilia.project.projects_changed";
}

/// Names the changed rows so a consumer re-reads only what moved instead of
/// refreshing every project surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TasksChanged {
    pub project_id: Option<ProjectId>,
    pub task_id: Option<TaskId>,
}

impl Event for TasksChanged {
    const NAME: &'static str = "lilia.project.tasks_changed";
}

/// Publishes typed events onto the kernel bus.
pub struct KernelProjectTaskEvents {
    events: lilia_kernel::EventBus,
}

impl KernelProjectTaskEvents {
    pub fn new(events: lilia_kernel::EventBus) -> Self {
        Self { events }
    }
}

impl ProjectTaskEvents for KernelProjectTaskEvents {
    fn projects_changed(&self) {
        self.events.publish(ProjectsChanged);
    }

    fn tasks_changed(&self, project_id: Option<ProjectId>, task_id: Option<TaskId>) {
        self.events.publish(TasksChanged {
            project_id,
            task_id,
        });
    }
}
