use std::fs;
use std::path::{Component, Path, PathBuf};

use lilia_contracts::{Project, ProjectId};
use serde::{Deserialize, Serialize};

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

    /// Lexically reject `..` / absolute inputs, then canonicalize (or deepest
    /// existing ancestor + suffix) and require the result under the canonical
    /// active project root. Escaping symlinks are refused before any read/list.
    ///
    /// Residual risk: TOCTOU between this fence and later IO if a path is
    /// replaced with an escaping symlink mid-call; consider `O_NOFOLLOW` later.
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
        resolve_under_root(self.active_root(), relative)
    }

    /// Canonical active root used as the jail fence for list/open helpers.
    pub fn canonical_active_root(&self) -> Result<PathBuf, ProjectContextError> {
        fs::canonicalize(self.active_root()).map_err(|error| ProjectContextError::Io {
            path: self.active_root().to_path_buf(),
            message: error.to_string(),
        })
    }
}

fn is_path_within_root(path: &Path, canonical_root: &Path) -> bool {
    path == canonical_root || path.starts_with(canonical_root)
}

fn resolve_under_root(root: &Path, relative: &Path) -> Result<PathBuf, ProjectContextError> {
    let canonical_root = fs::canonicalize(root).map_err(|error| ProjectContextError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })?;
    let joined = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };

    let mut probe = joined;
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    loop {
        match fs::canonicalize(&probe) {
            Ok(canon) => {
                let mut resolved = canon;
                for part in missing.iter().rev() {
                    resolved.push(part);
                }
                if !is_path_within_root(&resolved, &canonical_root) {
                    return Err(ProjectContextError::PathEscapesWorkspace(
                        relative.to_path_buf(),
                    ));
                }
                return Ok(resolved);
            }
            Err(_) => {
                let Some(name) = probe.file_name().map(std::ffi::OsStr::to_os_string) else {
                    return Err(ProjectContextError::PathEscapesWorkspace(
                        relative.to_path_buf(),
                    ));
                };
                missing.push(name);
                if !probe.pop() {
                    return Err(ProjectContextError::PathEscapesWorkspace(
                        relative.to_path_buf(),
                    ));
                }
            }
        }
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
    #[error("path escapes the active project root (symlink or resolution): `{0:?}`")]
    PathEscapesWorkspace(PathBuf),
    #[error("failed to resolve `{path:?}`: {message}")]
    Io { path: PathBuf, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_at(root: PathBuf) -> ProjectContext {
        let project = Project::new(ProjectId::new("project").unwrap(), "Project").unwrap();
        ProjectContext {
            project_id: project.id,
            workspace_root: root,
            worktree_root: None,
        }
    }

    #[test]
    fn project_context_rejects_paths_that_escape_the_active_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}").unwrap();
        let context = context_at(root.path().to_path_buf());

        assert!(matches!(
            context.resolve_relative(Path::new("../secret.txt")),
            Err(ProjectContextError::InvalidRelativePath(_))
        ));
        assert_eq!(
            context.resolve_relative(Path::new("src/main.rs")).unwrap(),
            fs::canonicalize(root.path().join("src/main.rs")).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_relative_denies_symlink_file_pointing_outside() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "top-secret").unwrap();
        let link = root.path().join("leak.txt");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let context = context_at(root.path().to_path_buf());

        assert!(matches!(
            context.resolve_relative(Path::new("leak.txt")),
            Err(ProjectContextError::PathEscapesWorkspace(_))
        ));
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "top-secret");
    }

    #[cfg(unix)]
    #[test]
    fn resolve_relative_denies_symlink_dir_pointing_outside() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "nope").unwrap();
        let link = root.path().join("escape_dir");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let context = context_at(root.path().to_path_buf());

        assert!(matches!(
            context.resolve_relative(Path::new("escape_dir")),
            Err(ProjectContextError::PathEscapesWorkspace(_))
        ));
        assert!(matches!(
            context.resolve_relative(Path::new("escape_dir/secret.txt")),
            Err(ProjectContextError::PathEscapesWorkspace(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_relative_allows_in_tree_symlink_and_real_paths() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        let target = root.path().join("src/lib.rs");
        std::fs::write(&target, "pub fn ok() {}").unwrap();
        let link = root.path().join("alias.rs");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let context = context_at(root.path().to_path_buf());

        assert_eq!(
            context.resolve_relative(Path::new("alias.rs")).unwrap(),
            fs::canonicalize(&target).unwrap()
        );
        assert_eq!(
            context.resolve_relative(Path::new("src/lib.rs")).unwrap(),
            fs::canonicalize(&target).unwrap()
        );
    }
}