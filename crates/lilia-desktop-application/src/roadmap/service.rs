use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    Milestone, MilestoneUpdatePatch, ProjectRoadmap, RoadmapStore, RoadmapStoreError,
    SqliteRoadmapStore, TaskMilestoneLink,
};

#[derive(Clone)]
pub struct DesktopRoadmapService {
    inner: Arc<DesktopRoadmapServiceInner>,
}

struct DesktopRoadmapServiceInner {
    store: Mutex<Box<dyn RoadmapStore>>,
}

impl DesktopRoadmapService {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RoadmapStoreError> {
        Self::from_store(SqliteRoadmapStore::open(path)?)
    }

    pub fn in_memory() -> Result<Self, RoadmapStoreError> {
        Self::from_store(SqliteRoadmapStore::in_memory()?)
    }

    pub fn from_store(store: impl RoadmapStore + 'static) -> Result<Self, RoadmapStoreError> {
        Ok(Self {
            inner: Arc::new(DesktopRoadmapServiceInner {
                store: Mutex::new(Box::new(store)),
            }),
        })
    }

    pub fn list(&self, project_id: &str) -> Result<ProjectRoadmap, RoadmapStoreError> {
        self.store()?.list(project_id)
    }

    pub fn create(&self, project_id: &str, title: &str) -> Result<Milestone, RoadmapStoreError> {
        self.store()?.create(project_id, title)
    }

    pub fn update(
        &self,
        milestone_id: &str,
        patch: MilestoneUpdatePatch,
    ) -> Result<Milestone, RoadmapStoreError> {
        self.store()?.update(milestone_id, patch)
    }

    pub fn delete(&self, milestone_id: &str) -> Result<bool, RoadmapStoreError> {
        self.store()?.delete(milestone_id)
    }

    pub fn reorder(
        &self,
        project_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<Milestone>, RoadmapStoreError> {
        self.store()?.reorder(project_id, ordered_ids)
    }

    pub fn set_tasks(
        &self,
        milestone_id: &str,
        task_ids: Vec<String>,
    ) -> Result<Vec<TaskMilestoneLink>, RoadmapStoreError> {
        self.store()?.set_tasks(milestone_id, task_ids)
    }

    fn store(&self) -> Result<MutexGuard<'_, Box<dyn RoadmapStore>>, RoadmapStoreError> {
        self.inner
            .store
            .lock()
            .map_err(|_| RoadmapStoreError::StateUnavailable)
    }
}
