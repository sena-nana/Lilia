use std::path::{Component, Path, PathBuf};

use lilia_contracts::{Project, ProjectId};
use serde::{Deserialize, Serialize};

use crate::{DesktopApplication, DesktopApplicationError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContext {
    pub project_id: ProjectId,
    pub workspace_root: PathBuf,
    pub worktree_root: Option<PathBuf>,
}

impl ProjectContext {
    pub fn from_project(project: &Project) -> Result<Self, ProjectContextError> {
        let workspace_root = project
            .workspace_path
            .as_ref()
            .ok_or_else(|| ProjectContextError::MissingWorkspace(project.id.clone()))
            .map(PathBuf::from)?;
        validate_absolute_root(&workspace_root)?;
        let worktree_root = project
            .git_workspace
            .as_ref()
            .and_then(|git| git.worktree_path.as_ref())
            .map(PathBuf::from)
            .map(|path| {
                validate_absolute_root(&path)?;
                Ok(path)
            })
            .transpose()?;
        Ok(Self {
            project_id: project.id.clone(),
            workspace_root,
            worktree_root,
        })
    }

    pub fn active_root(&self) -> &Path {
        self.worktree_root
            .as_deref()
            .unwrap_or(&self.workspace_root)
    }

    pub fn resolve_relative(&self, relative: &Path) -> Result<PathBuf, ProjectContextError> {
        if relative.as_os_str().is_empty() || relative.is_absolute() {
            return Err(ProjectContextError::InvalidRelativePath(
                relative.to_path_buf(),
            ));
        }
        for component in relative.components() {
            if !matches!(component, Component::Normal(_) | Component::CurDir) {
                return Err(ProjectContextError::InvalidRelativePath(
                    relative.to_path_buf(),
                ));
            }
        }
        Ok(self.active_root().join(relative))
    }
}

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

fn validate_absolute_root(path: &Path) -> Result<(), ProjectContextError> {
    if !path.is_absolute() {
        return Err(ProjectContextError::WorkspaceRootMustBeAbsolute(
            path.to_path_buf(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProjectContextError {
    #[error("project `{0}` has no workspace root")]
    MissingWorkspace(ProjectId),
    #[error("workspace root must be absolute: `{0:?}`")]
    WorkspaceRootMustBeAbsolute(PathBuf),
    #[error("path must stay relative to the active project root: `{0:?}`")]
    InvalidRelativePath(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_context_rejects_paths_that_escape_the_active_root() {
        let project = Project::new(ProjectId::new("project").unwrap(), "Project").unwrap();
        let context = ProjectContext {
            project_id: project.id,
            workspace_root: std::env::current_dir().unwrap(),
            worktree_root: None,
        };

        assert!(matches!(
            context.resolve_relative(Path::new("../secret.txt")),
            Err(ProjectContextError::InvalidRelativePath(_))
        ));
        assert_eq!(
            context.resolve_relative(Path::new("src/main.rs")).unwrap(),
            context.workspace_root.join("src/main.rs")
        );
    }
}
