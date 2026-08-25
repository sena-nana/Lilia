use super::{DesktopMemory, MemoryInjectionState, MemorySettings, MemoryUpsertInput};

pub trait MemoryStore: Send {
    fn list(&self, project_id: Option<&str>) -> Result<Vec<DesktopMemory>, MemoryStoreError>;

    fn memory(&self, memory_id: &str) -> Result<Option<DesktopMemory>, MemoryStoreError>;

    fn save(&mut self, input: MemoryUpsertInput) -> Result<DesktopMemory, MemoryStoreError>;

    fn set_enabled(
        &mut self,
        memory_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<DesktopMemory, MemoryStoreError>;

    fn delete(
        &mut self,
        memory_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<bool, MemoryStoreError>;

    fn injection_state(&self, task_id: &str) -> Result<MemoryInjectionState, MemoryStoreError>;

    fn set_task_enabled(
        &mut self,
        task_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, MemoryStoreError>;

    fn reset_task_cooldown(
        &mut self,
        task_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, MemoryStoreError>;
}

pub trait MemorySettingsStore: Send {
    fn load(&self) -> Result<Option<MemorySettings>, MemoryStoreError>;

    fn save(&mut self, settings: &MemorySettings) -> Result<(), MemoryStoreError>;
}

#[derive(Default)]
pub struct InMemoryMemorySettingsStore {
    settings: Option<MemorySettings>,
}

impl MemorySettingsStore for InMemoryMemorySettingsStore {
    fn load(&self) -> Result<Option<MemorySettings>, MemoryStoreError> {
        Ok(self.settings.clone())
    }

    fn save(&mut self, settings: &MemorySettings) -> Result<(), MemoryStoreError> {
        self.settings = Some(settings.clone());
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryStoreError {
    #[error("memory title must not be empty")]
    EmptyTitle,
    #[error("memory body must not be empty")]
    EmptyBody,
    #[error("project memory must reference a project")]
    ProjectIdRequired,
    #[error("memory project does not exist: {project_id}")]
    ProjectNotFound { project_id: String },
    #[error("memory does not exist: {memory_id}")]
    MemoryNotFound { memory_id: String },
    #[error("expected_updated_at requires an existing memory id")]
    ExpectedUpdateRequiresId,
    #[error(
        "memory {memory_id} changed since it was loaded: expected updated_at {expected_updated_at}, actual {actual_updated_at}"
    )]
    Conflict {
        memory_id: String,
        expected_updated_at: i64,
        actual_updated_at: i64,
    },
    #[error("stored memory {memory_id} has invalid scope {scope}")]
    InvalidStoredScope { memory_id: String, scope: String },
    #[error("stored memory {memory_id} violates its {scope} scope/project invariant")]
    InvalidStoredProjectScope { memory_id: String, scope: String },
    #[error("stored memory {memory_id} has invalid enabled value {value}")]
    InvalidStoredEnabled { memory_id: String, value: i64 },
    #[error("stored memory {memory_id} has invalid JSON in {field}: {message}")]
    CorruptJson {
        memory_id: String,
        field: &'static str,
        message: String,
    },
    #[error("memory serialization failed for {field}: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("memory storage requires the projects authority table")]
    ProjectsSchemaRequired,
    #[error("memory injection storage requires the tasks authority table")]
    TasksSchemaRequired,
    #[error("memory task does not exist: {task_id}")]
    TaskNotFound { task_id: String },
    #[error(
        "memory injection state for task {task_id} changed since it was loaded: expected updated_at {expected_updated_at}, actual {actual_updated_at}"
    )]
    InjectionStateConflict {
        task_id: String,
        expected_updated_at: i64,
        actual_updated_at: i64,
    },
    #[error("stored memory injection state for task {task_id} has invalid enabled value {value}")]
    InvalidStoredInjectionEnabled { task_id: String, value: i64 },
    #[error("stored memory settings are invalid: {message}")]
    CorruptSettings { message: String },
    #[error("memory settings storage operation {operation} failed: {message}")]
    SettingsStorage {
        operation: &'static str,
        message: String,
    },
    #[error("memory storage operation {operation} failed: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

impl MemoryStoreError {
    pub(crate) fn storage(operation: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Storage {
            operation,
            message: error.to_string(),
        }
    }
}
