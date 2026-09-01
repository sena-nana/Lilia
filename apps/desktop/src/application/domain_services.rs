use lilia_contracts::{ProjectId, TaskId};

use crate::application::{
    ArchitectureBackend, ArchitectureChanged, DesktopApplication, DesktopApplicationError,
    DesktopArchitectureService, DesktopMemory, DesktopMemoryService, DesktopRoadmapService,
    MemoryChanged, MemoryInjectionChanged, MemoryInjectionState, MemorySettings,
    MemorySettingsChanged, MemoryUpsertInput, Milestone, MilestoneUpdatePatch,
    ProjectArchitectureApplyInput, ProjectArchitectureApplyResult, ProjectArchitectureChangeEvent,
    ProjectArchitectureChangeRecord, ProjectArchitectureGraph, ProjectArchitectureQuarantineRecord,
    ProjectArchitectureRejectInput, ProjectArchitectureRollbackResult, ProjectRoadmap,
    RoadmapChanged, TaskMilestoneLink,
};

impl DesktopApplication {
    pub fn architecture_service(&self) -> DesktopArchitectureService {
        self.inner.architecture.clone()
    }

    pub fn project_architecture(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectArchitectureGraph, DesktopApplicationError> {
        self.get_project(project_id)?;
        Ok(self.inner.architecture.graph(project_id.as_str())?)
    }

    pub fn project_architecture_changes(
        &self,
        project_id: &ProjectId,
        limit: usize,
    ) -> Result<Vec<ProjectArchitectureChangeRecord>, DesktopApplicationError> {
        self.get_project(project_id)?;
        Ok(self
            .inner
            .architecture
            .list_changes(project_id.as_str(), limit)?)
    }

    pub fn project_architecture_quarantine(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<ProjectArchitectureQuarantineRecord>, DesktopApplicationError> {
        self.get_project(project_id)?;
        Ok(self
            .inner
            .architecture
            .list_quarantine(project_id.as_str())?)
    }

    pub fn apply_project_architecture(
        &self,
        input: ProjectArchitectureApplyInput,
    ) -> Result<ProjectArchitectureApplyResult, DesktopApplicationError> {
        let project_id = ProjectId::new(&input.project_id)?;
        self.validate_architecture_task(&project_id, &input.task_id)?;
        let result = self.inner.architecture.apply(input)?;
        self.emit_event(ArchitectureChanged {
            project_id,
            version: result.graph.version,
        });
        Ok(result)
    }

    pub fn reject_project_architecture(
        &self,
        input: ProjectArchitectureRejectInput,
    ) -> Result<ProjectArchitectureChangeEvent, DesktopApplicationError> {
        let project_id = ProjectId::new(&input.project_id)?;
        self.validate_architecture_task(&project_id, &input.task_id)?;
        Ok(self.inner.architecture.reject(input)?)
    }

    pub fn rollback_project_architecture(
        &self,
        project_id: &ProjectId,
        task_id: &TaskId,
        backend: ArchitectureBackend,
    ) -> Result<ProjectArchitectureRollbackResult, DesktopApplicationError> {
        self.validate_architecture_task(project_id, task_id.as_str())?;
        let result =
            self.inner
                .architecture
                .rollback(project_id.as_str(), task_id.as_str(), backend)?;
        if result.event.is_some() {
            self.emit_event(ArchitectureChanged {
                project_id: project_id.clone(),
                version: result.graph.version,
            });
        }
        Ok(result)
    }

    fn validate_architecture_task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<(), DesktopApplicationError> {
        self.get_project(project_id)?;
        let task_id = TaskId::new(task_id)?;
        let task = self.get_task(&task_id)?;
        if task.project_id.as_ref() != Some(project_id) {
            return Err(DesktopApplicationError::InvalidInput {
                field: "taskId",
                message: "architecture task must belong to the selected project".to_owned(),
            });
        }
        Ok(())
    }

    pub fn memory_service(&self) -> DesktopMemoryService {
        self.inner.memory.clone()
    }

    pub fn list_memories(
        &self,
        project_id: Option<&ProjectId>,
    ) -> Result<Vec<DesktopMemory>, DesktopApplicationError> {
        Ok(self.inner.memory.list(project_id.map(ProjectId::as_str))?)
    }

    pub fn memory(
        &self,
        memory_id: &str,
    ) -> Result<Option<DesktopMemory>, DesktopApplicationError> {
        Ok(self.inner.memory.memory(memory_id)?)
    }

    pub fn save_memory(
        &self,
        input: MemoryUpsertInput,
    ) -> Result<DesktopMemory, DesktopApplicationError> {
        let memory = self.inner.memory.save(input)?;
        self.emit_event(MemoryChanged {
            memory_id: Some(memory.id.clone()),
            project_id: memory
                .project_id
                .as_deref()
                .and_then(|project_id| ProjectId::new(project_id).ok()),
        });
        Ok(memory)
    }

    pub fn set_memory_enabled(
        &self,
        memory_id: &str,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<DesktopMemory, DesktopApplicationError> {
        let memory =
            self.inner
                .memory
                .set_enabled_if_unmodified(memory_id, enabled, expected_updated_at)?;
        self.emit_event(MemoryChanged {
            memory_id: Some(memory.id.clone()),
            project_id: memory
                .project_id
                .as_deref()
                .and_then(|project_id| ProjectId::new(project_id).ok()),
        });
        Ok(memory)
    }

    pub fn delete_memory(
        &self,
        memory_id: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<bool, DesktopApplicationError> {
        let previous = self.inner.memory.memory(memory_id)?;
        let deleted = self
            .inner
            .memory
            .delete_if_unmodified(memory_id, expected_updated_at)?;
        if deleted {
            self.emit_event(MemoryChanged {
                memory_id: Some(memory_id.to_owned()),
                project_id: previous
                    .and_then(|memory| memory.project_id)
                    .and_then(|project_id| ProjectId::new(project_id).ok()),
            });
        }
        Ok(deleted)
    }

    pub fn memory_settings(&self) -> Result<MemorySettings, DesktopApplicationError> {
        Ok(self.inner.memory.settings()?)
    }

    pub fn save_memory_settings(
        &self,
        settings: MemorySettings,
    ) -> Result<MemorySettings, DesktopApplicationError> {
        let settings = self.inner.memory.save_settings(settings)?;
        self.emit_event(MemorySettingsChanged);
        Ok(settings)
    }

    pub fn memory_injection_state(
        &self,
        task_id: &TaskId,
    ) -> Result<MemoryInjectionState, DesktopApplicationError> {
        Ok(self.inner.memory.injection_state(task_id.as_str())?)
    }

    pub fn set_task_memory_enabled(
        &self,
        task_id: &TaskId,
        enabled: bool,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, DesktopApplicationError> {
        let state = self.inner.memory.set_task_enabled_if_unmodified(
            task_id.as_str(),
            enabled,
            expected_updated_at,
        )?;
        self.emit_event(MemoryInjectionChanged {
            task_id: task_id.clone(),
        });
        Ok(state)
    }

    pub fn reset_task_memory_cooldown(
        &self,
        task_id: &TaskId,
        expected_updated_at: Option<i64>,
    ) -> Result<MemoryInjectionState, DesktopApplicationError> {
        let state = self
            .inner
            .memory
            .reset_task_cooldown_if_unmodified(task_id.as_str(), expected_updated_at)?;
        self.emit_event(MemoryInjectionChanged {
            task_id: task_id.clone(),
        });
        Ok(state)
    }

    pub fn roadmap_service(&self) -> DesktopRoadmapService {
        self.inner.roadmap.clone()
    }

    pub fn project_roadmap(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectRoadmap, DesktopApplicationError> {
        Ok(self.inner.roadmap.list(project_id.as_str())?)
    }

    pub fn create_milestone(
        &self,
        project_id: &ProjectId,
        title: &str,
    ) -> Result<Milestone, DesktopApplicationError> {
        let milestone = self.inner.roadmap.create(project_id.as_str(), title)?;
        self.emit_roadmap_changed(project_id, Some(milestone.id.clone()));
        Ok(milestone)
    }

    pub fn update_milestone(
        &self,
        project_id: &ProjectId,
        milestone_id: &str,
        patch: MilestoneUpdatePatch,
    ) -> Result<Milestone, DesktopApplicationError> {
        let roadmap = self.inner.roadmap.list(project_id.as_str())?;
        if !roadmap
            .milestones
            .iter()
            .any(|milestone| milestone.id == milestone_id)
        {
            return Err(crate::application::RoadmapStoreError::MilestoneNotFound {
                milestone_id: milestone_id.to_owned(),
            }
            .into());
        }
        let milestone = self.inner.roadmap.update(milestone_id, patch)?;
        self.emit_roadmap_changed(project_id, Some(milestone.id.clone()));
        Ok(milestone)
    }

    pub fn delete_milestone(
        &self,
        project_id: &ProjectId,
        milestone_id: &str,
    ) -> Result<bool, DesktopApplicationError> {
        let roadmap = self.inner.roadmap.list(project_id.as_str())?;
        if !roadmap
            .milestones
            .iter()
            .any(|milestone| milestone.id == milestone_id)
        {
            return Ok(false);
        }
        let deleted = self.inner.roadmap.delete(milestone_id)?;
        if deleted {
            self.emit_roadmap_changed(project_id, Some(milestone_id.to_owned()));
        }
        Ok(deleted)
    }

    pub fn reorder_milestones(
        &self,
        project_id: &ProjectId,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<Milestone>, DesktopApplicationError> {
        let milestones = self
            .inner
            .roadmap
            .reorder(project_id.as_str(), ordered_ids)?;
        self.emit_roadmap_changed(project_id, None);
        Ok(milestones)
    }

    pub fn set_milestone_tasks(
        &self,
        project_id: &ProjectId,
        milestone_id: &str,
        task_ids: Vec<String>,
    ) -> Result<Vec<TaskMilestoneLink>, DesktopApplicationError> {
        let roadmap = self.inner.roadmap.list(project_id.as_str())?;
        if !roadmap
            .milestones
            .iter()
            .any(|milestone| milestone.id == milestone_id)
        {
            return Err(crate::application::RoadmapStoreError::MilestoneNotFound {
                milestone_id: milestone_id.to_owned(),
            }
            .into());
        }
        let links = self.inner.roadmap.set_tasks(milestone_id, task_ids)?;
        self.emit_roadmap_changed(project_id, Some(milestone_id.to_owned()));
        Ok(links)
    }

    fn emit_roadmap_changed(&self, project_id: &ProjectId, milestone_id: Option<String>) {
        self.emit_event(RoadmapChanged {
            project_id: project_id.clone(),
            milestone_id,
        });
    }
}
