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
