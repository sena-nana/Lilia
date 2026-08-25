use std::sync::{Arc, Mutex, MutexGuard};

use lilia_storage::Db;

use super::{
    DesktopMemory, InMemoryMemorySettingsStore, MemoryInjectionState, MemorySettings,
    MemorySettingsStore, MemoryStore, MemoryStoreError, MemoryUpsertInput, SqliteMemoryStore,
};

#[derive(Clone)]
pub struct DesktopMemoryService {
    state: Arc<Mutex<DesktopMemoryServiceState>>,
}

struct DesktopMemoryServiceState {
    records: Box<dyn MemoryStore>,
    settings: Box<dyn MemorySettingsStore>,
}

impl DesktopMemoryService {
    pub fn from_db_with_settings(
        db: Db,
        settings: impl MemorySettingsStore + 'static,
    ) -> Result<Self, DesktopMemoryError> {
        Ok(Self::from_stores(SqliteMemoryStore::from_db(db)?, settings))
    }

    pub fn in_memory() -> Result<Self, DesktopMemoryError> {
        Ok(Self::from_store(SqliteMemoryStore::in_memory()?))
    }

    pub fn from_store(store: impl MemoryStore + 'static) -> Self {
        Self::from_stores(store, InMemoryMemorySettingsStore::default())
    }

    pub fn from_stores(
        store: impl MemoryStore + 'static,
        settings: impl MemorySettingsStore + 'static,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(DesktopMemoryServiceState {
                records: Box::new(store),
                settings: Box::new(settings),
            })),
        }
    }

    pub fn list(&self, project_id: Option<&str>) -> Result<Vec<DesktopMemory>, DesktopMemoryError> {
        Ok(self.state()?.records.list(project_id)?)
    }

    pub fn memory(&self, memory_id: &str) -> Result<Option<DesktopMemory>, DesktopMemoryError> {
        Ok(self.state()?.records.memory(memory_id)?)
    }

    pub fn save(&self, input: MemoryUpsertInput) -> Result<DesktopMemory, DesktopMemoryError> {
        Ok(self.state()?.records.save(input)?)
    }

    pub fn set_enabled(
        &self,
        memory_id: &str,
        enabled: bool,
    ) -> Result<DesktopMemory, DesktopMemoryError> {
        self.set_enabled_if_unmodified(memory_id, enabled, None)
    }

    pub fn set_enabled_if_unmodified(
        &self,
        memory_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<DesktopMemory, DesktopMemoryError> {
        Ok(self
            .state()?
            .records
            .set_enabled(memory_id, enabled, expected_updated_at)?)
    }

    pub fn delete(&self, memory_id: &str) -> Result<bool, DesktopMemoryError> {
        self.delete_if_unmodified(memory_id, None)
    }

    pub fn delete_if_unmodified(
        &self,
        memory_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<bool, DesktopMemoryError> {
        Ok(self
            .state()?
            .records
            .delete(memory_id, expected_updated_at)?)
    }

    pub fn settings(&self) -> Result<MemorySettings, DesktopMemoryError> {
        Ok(self
            .state()?
            .settings
            .load()?
            .unwrap_or_default()
            .normalized())
    }

    pub fn save_settings(
        &self,
        settings: MemorySettings,
    ) -> Result<MemorySettings, DesktopMemoryError> {
        let settings = settings.normalized();
        self.state()?.settings.save(&settings)?;
        Ok(settings)
    }

    pub fn injection_state(
        &self,
        task_id: &str,
    ) -> Result<MemoryInjectionState, DesktopMemoryError> {
        Ok(self.state()?.records.injection_state(task_id)?)
    }

    pub fn set_task_enabled(
        &self,
        task_id: &str,
        enabled: bool,
    ) -> Result<MemoryInjectionState, DesktopMemoryError> {
        self.set_task_enabled_if_unmodified(task_id, enabled, None)
    }

    pub fn set_task_enabled_if_unmodified(
        &self,
        task_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, DesktopMemoryError> {
        Ok(self
            .state()?
            .records
            .set_task_enabled(task_id, enabled, expected_updated_at)?)
    }

    pub fn reset_task_cooldown(
        &self,
        task_id: &str,
    ) -> Result<MemoryInjectionState, DesktopMemoryError> {
        self.reset_task_cooldown_if_unmodified(task_id, None)
    }

    pub fn reset_task_cooldown_if_unmodified(
        &self,
        task_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, DesktopMemoryError> {
        Ok(self
            .state()?
            .records
            .reset_task_cooldown(task_id, expected_updated_at)?)
    }

    fn state(&self) -> Result<MutexGuard<'_, DesktopMemoryServiceState>, DesktopMemoryError> {
        self.state
            .lock()
            .map_err(|_| DesktopMemoryError::StateUnavailable)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopMemoryError {
    #[error("desktop memory state is unavailable")]
    StateUnavailable,
    #[error(transparent)]
    Store(#[from] MemoryStoreError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryScope;

    fn input() -> MemoryUpsertInput {
        MemoryUpsertInput {
            id: Some("memory-1".to_owned()),
            scope: MemoryScope::User,
            project_id: None,
            title: "Review".to_owned(),
            body: "Run the focused tests".to_owned(),
            tags: vec!["workflow".to_owned()],
            enabled: true,
            source_task_id: None,
            expected_updated_at: None,
        }
    }

    #[test]
    fn service_exposes_complete_record_lifecycle() {
        let service = DesktopMemoryService::in_memory().unwrap();
        let saved = service.save(input()).unwrap();
        assert_eq!(service.list(None).unwrap(), vec![saved.clone()]);
        assert_eq!(service.memory(&saved.id).unwrap(), Some(saved.clone()));

        let disabled = service.set_enabled(&saved.id, false).unwrap();
        assert!(!disabled.enabled);
        assert!(service.delete(&saved.id).unwrap());
        assert_eq!(service.memory(&saved.id).unwrap(), None);
    }

    #[test]
    fn service_preserves_typed_store_errors() {
        let service = DesktopMemoryService::in_memory().unwrap();
        assert!(matches!(
            service.set_enabled("missing", false),
            Err(DesktopMemoryError::Store(MemoryStoreError::MemoryNotFound {
                memory_id
            })) if memory_id == "missing"
        ));
    }

    #[test]
    fn service_normalizes_and_persists_host_owned_settings() {
        let service = DesktopMemoryService::in_memory().unwrap();
        assert_eq!(service.settings().unwrap(), MemorySettings::default());
        let saved = service
            .save_settings(MemorySettings {
                enabled: false,
                baseline_injection_enabled: false,
                cooldown_turns: 0,
            })
            .unwrap();
        assert_eq!(
            saved.cooldown_turns,
            MemorySettings::default().cooldown_turns
        );
        assert_eq!(service.settings().unwrap(), saved);
    }

    struct CorruptSettingsStore;

    impl MemorySettingsStore for CorruptSettingsStore {
        fn load(&self) -> Result<Option<MemorySettings>, MemoryStoreError> {
            Err(MemoryStoreError::CorruptSettings {
                message: "injected invalid JSON".to_owned(),
            })
        }

        fn save(&mut self, _settings: &MemorySettings) -> Result<(), MemoryStoreError> {
            Err(MemoryStoreError::SettingsStorage {
                operation: "save test settings",
                message: "injected write failure".to_owned(),
            })
        }
    }

    #[test]
    fn service_preserves_typed_host_settings_errors() {
        let service = DesktopMemoryService::from_stores(
            SqliteMemoryStore::in_memory().unwrap(),
            CorruptSettingsStore,
        );
        assert!(matches!(
            service.settings(),
            Err(DesktopMemoryError::Store(
                MemoryStoreError::CorruptSettings { .. }
            ))
        ));
        assert!(matches!(
            service.save_settings(MemorySettings::default()),
            Err(DesktopMemoryError::Store(
                MemoryStoreError::SettingsStorage { .. }
            ))
        ));
    }
}
