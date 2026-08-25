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

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef};

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
