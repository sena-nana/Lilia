//! Project and task domain feature.
//!
//! Provides [`ProjectTaskService`], the authority-backed owner of project and
//! task facts. Consumers resolve it from the kernel's service registry instead
//! of reaching into a shared application object.

mod events;
mod query;
mod service;

use std::sync::Arc;

use lilia_contracts::ProductError;
use lilia_kernel::{
    Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef,
};
use lilia_service::ServiceAuthorityError;

pub use events::{
    KernelProjectTaskEvents, ProjectTaskEventFanout, ProjectTaskEvents, ProjectsChanged,
    SilentProjectTaskEvents, TasksChanged,
};
pub use query::{DesktopTaskScope, ProjectQuery, TaskQuery};
pub use service::{
    create_meta, update_meta, DesktopOptionalTextUpdate, DesktopProjectCreate, DesktopProjectPatch,
    DesktopProjectRemovalPreview, DesktopTaskCreate, DesktopTaskMove, DesktopTaskPatch,
    DesktopTaskRunBlock, ProjectTaskService,
};

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error(transparent)]
    Service(#[from] ServiceAuthorityError),
    #[error(transparent)]
    Product(#[from] ProductError),
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
}

/// Service slot for [`ProjectTaskService`].
pub enum ProjectTaskServiceKey {}

impl ServiceKey for ProjectTaskServiceKey {
    type Value = ProjectTaskService;

    const NAME: &'static str = "lilia.project.tasks";
}

/// Publishes the host-owned [`ProjectTaskService`] into the service registry.
///
/// The host installs [`KernelProjectTaskEvents`] onto the shared bus when it
/// builds the session, so this feature does not add a second sink at mount.
pub struct TaskFeature {
    service: ProjectTaskService,
}

impl TaskFeature {
    pub fn new(service: ProjectTaskService, _events: Arc<ProjectTaskEventFanout>) -> Self {
        Self { service }
    }
}

impl Feature for TaskFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.task").expect("the task feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<ProjectTaskServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<ProjectTaskServiceKey>(self.service.clone())
    }
}
