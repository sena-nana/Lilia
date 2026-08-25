use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lilia_contracts::{ProjectId, TaskId};
use lilia_kernel::Event;

/// Where the service reports fact changes. The kernel feature publishes typed
/// events; a host that still runs a legacy broadcast supplies its own sink.
pub trait ProjectTaskEvents: Send + Sync + 'static {
    fn projects_changed(&self);
    fn tasks_changed(&self, project_id: Option<ProjectId>, task_id: Option<TaskId>);
}

/// Fans one mutation out to every installed sink.
///
/// The desktop shell still listens on its own broadcast channel while the kernel
/// `EventBus` leg is installed at mount time. Routing both through one fanout is
/// what lets a single [`super::ProjectTaskService`] instance serve both, instead
/// of the host and the kernel each owning a parallel copy over the same `Db`.
#[derive(Default)]
pub struct ProjectTaskEventFanout {
    sinks: Mutex<Vec<Arc<dyn ProjectTaskEvents>>>,
}

impl ProjectTaskEventFanout {
    pub fn install(&self, sink: Arc<dyn ProjectTaskEvents>) {
        self.sinks().push(sink);
    }

    fn sinks(&self) -> MutexGuard<'_, Vec<Arc<dyn ProjectTaskEvents>>> {
        self.sinks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Clones out the sinks so a slow consumer cannot hold the lock while the
    /// next mutation tries to publish.
    fn snapshot(&self) -> Vec<Arc<dyn ProjectTaskEvents>> {
        self.sinks().clone()
    }
}

impl ProjectTaskEvents for ProjectTaskEventFanout {
    fn projects_changed(&self) {
        for sink in self.snapshot() {
            sink.projects_changed();
        }
    }

    fn tasks_changed(&self, project_id: Option<ProjectId>, task_id: Option<TaskId>) {
        for sink in self.snapshot() {
            sink.tasks_changed(project_id.clone(), task_id.clone());
        }
    }
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

    fn subject(&self) -> Option<String> {
        self.task_id
            .as_ref()
            .map(|id| id.as_str().to_owned())
            .or_else(|| {
                self.project_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
            })
    }

    fn detail(&self) -> serde_json::Value {
        serde_json::json!({
            "projectId": self.project_id.as_ref().map(ProjectId::as_str),
            "taskId": self.task_id.as_ref().map(TaskId::as_str),
        })
    }
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
