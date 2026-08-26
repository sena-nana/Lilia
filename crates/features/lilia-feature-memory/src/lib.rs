//! Memory domain feature.
//!
//! Owns durable memories, their scopes and the injection settings that decide
//! which memories reach an agent turn.

mod contract;
mod service;
mod sqlite;
mod store;
mod types;

pub use service::{DesktopMemoryError, DesktopMemoryService};
pub use sqlite::SqliteMemoryStore;
pub use store::{InMemoryMemorySettingsStore, MemorySettingsStore, MemoryStore, MemoryStoreError};
pub use types::{
    DesktopMemory, MemoryInjectionState, MemoryScope, MemorySettings, MemoryUpsertInput,
    MEMORY_SETTINGS_KEY,
};

use lilia_contracts::{ProjectId, TaskId};
use lilia_kernel::{
    Event, Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef,
};

/// A memory was created, updated or removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryChanged {
    pub memory_id: Option<String>,
    pub project_id: Option<ProjectId>,
}

impl Event for MemoryChanged {
    const NAME: &'static str = "lilia.memory.changed";

    fn subject(&self) -> Option<String> {
        self.project_id
            .as_ref()
            .map(|id| id.as_str().to_owned())
            .or_else(|| self.memory_id.clone())
    }
}

/// Injection or persistence settings for memories changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySettingsChanged;

impl Event for MemorySettingsChanged {
    const NAME: &'static str = "lilia.memory.settings_changed";
}

/// The memories injected into a task turn changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryInjectionChanged {
    pub task_id: TaskId,
}

impl Event for MemoryInjectionChanged {
    const NAME: &'static str = "lilia.memory.injection_changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

/// Service slot for [`DesktopMemoryService`].
pub enum MemoryServiceKey {}

impl ServiceKey for MemoryServiceKey {
    type Value = DesktopMemoryService;

    const NAME: &'static str = "lilia.memory";
}

pub struct MemoryFeature {
    service: DesktopMemoryService,
}

impl MemoryFeature {
    pub fn new(service: DesktopMemoryService) -> Self {
        Self { service }
    }
}

impl Feature for MemoryFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.memory").expect("the memory feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<MemoryServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<MemoryServiceKey>(self.service.clone())
    }
}
