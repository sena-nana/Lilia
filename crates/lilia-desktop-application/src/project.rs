use lilia_contracts::ProjectId;

use crate::{DesktopApplication, DesktopApplicationError};

pub use lilia_feature_document::{ProjectContext, ProjectContextError};

impl DesktopApplication {
    pub fn project_context(
        &self,
        project_id: &ProjectId,
    ) -> Result<ProjectContext, DesktopApplicationError> {
        Ok(ProjectContext::from_project(
            &self.get_project(project_id)?,
        )?)
    }
}
