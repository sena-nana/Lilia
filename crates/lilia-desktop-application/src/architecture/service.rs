use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    ArchitectureStore, DesktopArchitectureError, ProjectArchitectureApplyInput,
    ProjectArchitectureApplyResult, ProjectArchitectureChangeEvent,
    ProjectArchitectureChangeRecord, ProjectArchitectureGraph, ProjectArchitectureQuarantineRecord,
    ProjectArchitectureRejectInput, ProjectArchitectureRollbackResult, SqliteArchitectureStore,
};

#[derive(Clone)]
pub struct DesktopArchitectureService {
    inner: Arc<DesktopArchitectureServiceInner>,
}

struct DesktopArchitectureServiceInner {
    store: Mutex<Box<dyn ArchitectureStore>>,
}

impl DesktopArchitectureService {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DesktopArchitectureError> {
        Self::from_store(SqliteArchitectureStore::open(path)?)
    }

    pub fn in_memory() -> Result<Self, DesktopArchitectureError> {
        Self::from_store(SqliteArchitectureStore::in_memory()?)
    }

    pub fn from_store(
        store: impl ArchitectureStore + 'static,
    ) -> Result<Self, DesktopArchitectureError> {
        Ok(Self {
            inner: Arc::new(DesktopArchitectureServiceInner {
                store: Mutex::new(Box::new(store)),
            }),
        })
    }

    pub fn graph(
        &self,
        project_id: &str,
    ) -> Result<ProjectArchitectureGraph, DesktopArchitectureError> {
        self.store()?.graph(project_id)
    }

    pub fn list_changes(
        &self,
        project_id: &str,
        limit: usize,
    ) -> Result<Vec<ProjectArchitectureChangeRecord>, DesktopArchitectureError> {
        self.store()?.list_changes(project_id, limit.clamp(1, 200))
    }

    pub fn list_quarantine(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectArchitectureQuarantineRecord>, DesktopArchitectureError> {
        self.store()?.list_quarantine(project_id)
    }

    pub fn apply(
        &self,
        input: ProjectArchitectureApplyInput,
    ) -> Result<ProjectArchitectureApplyResult, DesktopArchitectureError> {
        self.store()?.apply(input)
    }

    pub fn reject(
        &self,
        input: ProjectArchitectureRejectInput,
    ) -> Result<ProjectArchitectureChangeEvent, DesktopArchitectureError> {
        self.store()?.reject(input)
    }

    pub fn rollback(
        &self,
        project_id: &str,
        task_id: &str,
        backend: super::ArchitectureBackend,
    ) -> Result<ProjectArchitectureRollbackResult, DesktopArchitectureError> {
        self.store()?.rollback(project_id, task_id, backend)
    }

    fn store(
        &self,
    ) -> Result<MutexGuard<'_, Box<dyn ArchitectureStore>>, DesktopArchitectureError> {
        self.inner
            .store
            .lock()
            .map_err(|_| DesktopArchitectureError::StateUnavailable)
    }
}
