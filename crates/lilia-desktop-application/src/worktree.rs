use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ProjectId, TaskId};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

const WORKTREE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS task_worktrees (
  task_id        TEXT PRIMARY KEY,
  project_id     TEXT,
  base_repo_path TEXT NOT NULL,
  worktree_path  TEXT NOT NULL UNIQUE,
  branch_name    TEXT NOT NULL,
  base_branch    TEXT NOT NULL,
  status         TEXT NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active','merged','removed')),
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_worktrees_project_status
  ON task_worktrees(project_id, status, updated_at DESC);
CREATE TABLE IF NOT EXISTS initial_worktree_intents (
  task_id       TEXT PRIMARY KEY,
  mode          TEXT NOT NULL CHECK (mode IN ('create','existing')),
  worktree_path TEXT,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  CHECK (
    (mode = 'create' AND worktree_path IS NULL) OR
    (mode = 'existing' AND worktree_path IS NOT NULL)
  )
);
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopWorktreeStatus {
    Active,
    Merged,
    Removed,
}

impl DesktopWorktreeStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Merged => "merged",
            Self::Removed => "removed",
        }
    }

    fn parse(value: &str) -> Result<Self, DesktopWorktreeError> {
        match value {
            "active" => Ok(Self::Active),
            "merged" => Ok(Self::Merged),
            "removed" => Ok(Self::Removed),
            other => Err(DesktopWorktreeError::InvalidStoredStatus(other.to_owned())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTaskWorktree {
    pub task_id: TaskId,
    pub project_id: Option<ProjectId>,
    pub base_repo_path: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub base_branch: String,
    pub status: DesktopWorktreeStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorktreeListItem {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub prunable: bool,
    pub locked: bool,
    pub is_main: bool,
    pub is_task_bound: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorktreeMergeResult {
    pub merged: bool,
    pub removed: bool,
    pub archived: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "worktree_path")]
pub enum DesktopInitialWorktreeSelection {
    Create,
    Existing(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitWorktree {
    path: String,
    head: Option<String>,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    prunable: bool,
    locked: bool,
}

pub(crate) struct DesktopWorktreeStore {
    connection: Connection,
}

impl DesktopWorktreeStore {
    pub(crate) fn open(path: &Path) -> Result<Self, DesktopWorktreeError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| DesktopWorktreeError::Storage {
                operation: "create worktree database directory",
                message: error.to_string(),
            })?;
        }
        let connection = Connection::open(path).map_err(|error| DesktopWorktreeError::Storage {
            operation: "open worktree database",
            message: error.to_string(),
        })?;
        Self::from_connection(connection)
    }

    pub(crate) fn in_memory() -> Result<Self, DesktopWorktreeError> {
        let connection =
            Connection::open_in_memory().map_err(|error| DesktopWorktreeError::Storage {
                operation: "open in-memory worktree database",
                message: error.to_string(),
            })?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, DesktopWorktreeError> {
        connection.execute_batch(WORKTREE_SCHEMA).map_err(|error| {
            DesktopWorktreeError::Storage {
                operation: "initialize worktree schema",
                message: error.to_string(),
            }
        })?;
        Ok(Self { connection })
    }

    fn active_for_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskWorktree>, DesktopWorktreeError> {
        self.connection
            .query_row(
                r#"SELECT task_id, project_id, base_repo_path, worktree_path, branch_name,
                          base_branch, status, created_at, updated_at
                   FROM task_worktrees
                   WHERE task_id = ?1 AND status = 'active'"#,
                params![task_id.as_str()],
                row_to_task_worktree,
            )
            .optional()
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "read task worktree",
                message: error.to_string(),
            })
    }

    fn active_bound_paths(&self) -> Result<BTreeSet<String>, DesktopWorktreeError> {
        let mut statement = self
            .connection
            .prepare("SELECT worktree_path FROM task_worktrees WHERE status = 'active'")
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "prepare active worktree paths",
                message: error.to_string(),
            })?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "query active worktree paths",
                message: error.to_string(),
            })?;
        rows.collect::<Result<BTreeSet<_>, _>>()
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "decode active worktree paths",
                message: error.to_string(),
            })
    }

    fn save_active(
        &self,
        task_id: &TaskId,
        project_id: Option<&ProjectId>,
        base_repo_path: &str,
        worktree_path: &str,
        branch_name: &str,
        base_branch: &str,
    ) -> Result<DesktopTaskWorktree, DesktopWorktreeError> {
        let now = now_millis();
        self.connection
            .execute(
                r#"INSERT INTO task_worktrees
                   (task_id, project_id, base_repo_path, worktree_path, branch_name,
                    base_branch, status, created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?7)
                   ON CONFLICT(task_id) DO UPDATE SET
                     project_id = excluded.project_id,
                     base_repo_path = excluded.base_repo_path,
                     worktree_path = excluded.worktree_path,
                     branch_name = excluded.branch_name,
                     base_branch = excluded.base_branch,
                     status = 'active',
                     updated_at = excluded.updated_at"#,
                params![
                    task_id.as_str(),
                    project_id.map(ProjectId::as_str),
                    base_repo_path,
                    worktree_path,
                    branch_name,
                    base_branch,
                    now,
                ],
            )
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "save task worktree",
                message: error.to_string(),
            })?;
        Ok(DesktopTaskWorktree {
            task_id: task_id.clone(),
            project_id: project_id.cloned(),
            base_repo_path: base_repo_path.to_owned(),
            worktree_path: worktree_path.to_owned(),
            branch_name: branch_name.to_owned(),
            base_branch: base_branch.to_owned(),
            status: DesktopWorktreeStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }

    fn mark_status(
        &self,
        task_id: &TaskId,
        status: DesktopWorktreeStatus,
    ) -> Result<bool, DesktopWorktreeError> {
        let changed = self
            .connection
            .execute(
                "UPDATE task_worktrees SET status = ?1, updated_at = ?2 WHERE task_id = ?3 AND status = 'active'",
                params![status.as_str(), now_millis(), task_id.as_str()],
            )
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "update task worktree status",
                message: error.to_string(),
            })?;
        Ok(changed > 0)
    }

    fn save_initial_intent(
        &self,
        task_id: &TaskId,
        selection: &DesktopInitialWorktreeSelection,
    ) -> Result<(), DesktopWorktreeError> {
        let (mode, worktree_path) = match selection {
            DesktopInitialWorktreeSelection::Create => ("create", None),
            DesktopInitialWorktreeSelection::Existing(path) => {
                ("existing", Some(normalized_path(path)))
            }
        };
        let now = now_millis();
        self.connection
            .execute(
                r#"INSERT INTO initial_worktree_intents
                   (task_id, mode, worktree_path, created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?4)
                   ON CONFLICT(task_id) DO UPDATE SET
                     mode = excluded.mode,
                     worktree_path = excluded.worktree_path,
                     updated_at = excluded.updated_at"#,
                params![task_id.as_str(), mode, worktree_path, now],
            )
            .map(|_| ())
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "save initial worktree intent",
                message: error.to_string(),
            })
    }

    fn initial_intent(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopInitialWorktreeSelection>, DesktopWorktreeError> {
        let stored = self
            .connection
            .query_row(
                "SELECT mode, worktree_path FROM initial_worktree_intents WHERE task_id = ?1",
                params![task_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "read initial worktree intent",
                message: error.to_string(),
            })?;
        stored
            .map(|(mode, path)| match (mode.as_str(), path) {
                ("create", None) => Ok(DesktopInitialWorktreeSelection::Create),
                ("existing", Some(path)) => Ok(DesktopInitialWorktreeSelection::Existing(
                    PathBuf::from(path),
                )),
                _ => Err(DesktopWorktreeError::InvalidStoredInitialIntent(mode)),
            })
            .transpose()
    }

    fn clear_initial_intent(&self, task_id: &TaskId) -> Result<bool, DesktopWorktreeError> {
        self.connection
            .execute(
                "DELETE FROM initial_worktree_intents WHERE task_id = ?1",
                params![task_id.as_str()],
            )
            .map(|changed| changed > 0)
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "clear initial worktree intent",
                message: error.to_string(),
            })
    }
}

impl DesktopApplication {
    pub fn set_initial_worktree_intent(
        &self,
        task_id: &TaskId,
        selection: Option<&DesktopInitialWorktreeSelection>,
    ) -> Result<(), DesktopApplicationError> {
        let worktrees = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?;
        if let Some(selection) = selection {
            worktrees.save_initial_intent(task_id, selection)?;
        } else {
            worktrees.clear_initial_intent(task_id)?;
        }
        Ok(())
    }

    pub fn initial_worktree_intent(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopInitialWorktreeSelection>, DesktopApplicationError> {
        Ok(self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .initial_intent(task_id)?)
    }

    pub fn retry_initial_worktree(
        &self,
        task_id: &TaskId,
    ) -> Result<bool, DesktopApplicationError> {
        let Some(selection) = self.initial_worktree_intent(task_id)? else {
            return Ok(false);
        };
        if self.task_worktree(task_id)?.is_none() {
            match selection {
                DesktopInitialWorktreeSelection::Create => {
                    self.create_task_worktree(task_id, None)?;
                }
                DesktopInitialWorktreeSelection::Existing(path) => {
                    self.attach_task_worktree(task_id, &path)?;
                }
            }
        }
        self.set_initial_worktree_intent(task_id, None)?;
        Ok(true)
    }

    pub(crate) fn ensure_initial_worktree_ready(
        &self,
        task_id: &TaskId,
    ) -> Result<(), DesktopApplicationError> {
        if self.initial_worktree_intent(task_id)?.is_some() {
            return Err(DesktopWorktreeError::InitialPreparationPending(task_id.clone()).into());
        }
        Ok(())
    }

    pub fn task_workspace_path(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<String>, DesktopApplicationError> {
        if let Some(worktree) = self.task_worktree(task_id)? {
            return Ok(Some(worktree.worktree_path));
        }
        let task = self.get_task(task_id)?;
        let Some(project_id) = task.project_id else {
            return Ok(None);
        };
        let project = self.get_project(&project_id)?;
        Ok(project.workspace_path.or_else(|| {
            project
                .git_workspace
                .and_then(|workspace| workspace.worktree_path)
        }))
    }

    pub fn task_worktree(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskWorktree>, DesktopApplicationError> {
        self.get_task(task_id)?;
        Ok(self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .active_for_task(task_id)?)
    }

    pub fn list_task_repository_worktrees(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<DesktopWorktreeListItem>, DesktopApplicationError> {
        let (_, _, base) = self.task_repository(task_id)?;
        let base = canonical_path(&base, "base repository")?;
        let base_text = normalized_path(&base);
        let bound_paths = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .active_bound_paths()?;
        Ok(list_git_worktrees(&base)?
            .into_iter()
            .map(|item| {
                let item_path = canonical_path(Path::new(&item.path), "worktree")
                    .map(|path| normalized_path(&path))
                    .unwrap_or(item.path);
                DesktopWorktreeListItem {
                    is_main: item_path == base_text,
                    is_task_bound: bound_paths.contains(&item_path),
                    path: item_path,
                    head: item.head,
                    branch: item.branch,
                    bare: item.bare,
                    detached: item.detached,
                    prunable: item.prunable,
                    locked: item.locked,
                }
            })
            .collect())
    }

    pub fn create_task_worktree(
        &self,
        task_id: &TaskId,
        parent_directory: Option<&Path>,
    ) -> Result<DesktopTaskWorktree, DesktopApplicationError> {
        let (task, project_id, base) = self.task_repository(task_id)?;
        if self.task_worktree(task_id)?.is_some() {
            return Err(DesktopWorktreeError::AlreadyBound(task_id.clone()).into());
        }
        ensure_git_repo(&base)?;
        let base = canonical_path(&base, "base repository")?;
        let base_branch = current_branch(&base)?;
        let preferred_parent = self.worktree_parent_directory_preference()?;
        let parent = parent_directory
            .map(Path::to_path_buf)
            .or(preferred_parent)
            .unwrap_or_else(|| {
                base.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| base.clone())
            });
        let parent = canonical_path(&parent, "worktree parent directory")?;
        let slug = task_title_slug(&task.title, task_id);
        let target = unique_worktree_target(&parent, &slug);
        let branch = unique_branch_name(&base, &slug)?;
        run_git(
            &base,
            &[
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("-b"),
                OsString::from(&branch),
                target.as_os_str().to_owned(),
                OsString::from(&base_branch),
            ],
        )?;
        let worktree = match canonical_path(&target, "created worktree") {
            Ok(path) => path,
            Err(error) => {
                rollback_created_worktree(&base, &target, &branch);
                return Err(error.into());
            }
        };
        let base_text = normalized_path(&base);
        let worktree_text = normalized_path(&worktree);
        let saved = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .save_active(
                task_id,
                project_id.as_ref(),
                &base_text,
                &worktree_text,
                &branch,
                &base_branch,
            );
        let saved = match saved {
            Ok(saved) => saved,
            Err(error) => {
                rollback_created_worktree(&base, &worktree, &branch);
                return Err(error.into());
            }
        };
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(saved)
    }

    pub fn attach_task_worktree(
        &self,
        task_id: &TaskId,
        worktree_path: &Path,
    ) -> Result<DesktopTaskWorktree, DesktopApplicationError> {
        let (_, project_id, base) = self.task_repository(task_id)?;
        if self.task_worktree(task_id)?.is_some() {
            return Err(DesktopWorktreeError::AlreadyBound(task_id.clone()).into());
        }
        ensure_git_repo(&base)?;
        ensure_git_repo(worktree_path)?;
        let base = canonical_path(&base, "base repository")?;
        let worktree = canonical_path(worktree_path, "worktree")?;
        if base == worktree {
            return Err(DesktopWorktreeError::MainRepositoryCannotBeAttached.into());
        }
        let worktree_text = normalized_path(&worktree);
        let registered = list_git_worktrees(&base)?
            .into_iter()
            .find(|item| {
                canonical_path(Path::new(&item.path), "worktree").is_ok_and(|path| path == worktree)
            })
            .ok_or_else(|| DesktopWorktreeError::NotRegistered(worktree_text.clone()))?;
        let branch = registered
            .branch
            .filter(|branch| !branch.trim().is_empty())
            .ok_or_else(|| DesktopWorktreeError::Detached(worktree_text.clone()))?;
        let base_branch = current_branch(&base)?;
        let saved = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .save_active(
                task_id,
                project_id.as_ref(),
                &normalized_path(&base),
                &worktree_text,
                &branch,
                &base_branch,
            )?;
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(saved)
    }

    pub fn clear_task_worktree(&self, task_id: &TaskId) -> Result<bool, DesktopApplicationError> {
        self.get_task(task_id)?;
        let changed = self
            .inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .mark_status(task_id, DesktopWorktreeStatus::Removed)?;
        if changed {
            self.emit_event(DesktopEventKind::WorktreeChanged {
                task_id: task_id.clone(),
            });
        }
        Ok(changed)
    }

    pub fn cleanup_task_worktree_and_archive(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        let worktree = self
            .task_worktree(task_id)?
            .ok_or_else(|| DesktopWorktreeError::NotBound(task_id.clone()))?;
        let base = PathBuf::from(&worktree.base_repo_path);
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        ensure_git_repo(&base)?;
        ensure_git_repo(&worktree_path)?;
        ensure_clean(&base, "base repository")?;
        ensure_clean(&worktree_path, "worktree")?;
        if branch_unique_commit_count(&worktree_path, &worktree.base_branch)? > 0 {
            return Err(DesktopWorktreeError::UnmergedCommits.into());
        }
        remove_worktree_and_branch(&base, &worktree_path, &worktree.branch_name)?;
        self.finish_worktree_archive(
            task_id,
            DesktopWorktreeStatus::Removed,
            false,
            "Removed the worktree without unique commits and archived the task",
        )
    }

    pub fn merge_task_worktree_and_archive(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        let worktree = self
            .task_worktree(task_id)?
            .ok_or_else(|| DesktopWorktreeError::NotBound(task_id.clone()))?;
        let base = PathBuf::from(&worktree.base_repo_path);
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        ensure_git_repo(&base)?;
        ensure_git_repo(&worktree_path)?;
        ensure_clean(&base, "base repository")?;
        ensure_clean(&worktree_path, "worktree")?;
        if branch_unique_commit_count(&worktree_path, &worktree.base_branch)? == 0 {
            return Err(DesktopWorktreeError::NoUniqueCommits.into());
        }
        if current_branch(&base)? != worktree.base_branch {
            run_git_text(&base, &["checkout", &worktree.base_branch])?;
        }
        run_git_text(&base, &["merge", "--no-ff", &worktree.branch_name])?;
        remove_worktree_and_branch(&base, &worktree_path, &worktree.branch_name)?;
        self.finish_worktree_archive(
            task_id,
            DesktopWorktreeStatus::Merged,
            true,
            "Merged the worktree branch, removed the worktree, and archived the task",
        )
    }

    fn task_repository(
        &self,
        task_id: &TaskId,
    ) -> Result<(lilia_contracts::ProductTask, Option<ProjectId>, PathBuf), DesktopApplicationError>
    {
        let task = self.get_task(task_id)?;
        let project_id = task
            .project_id
            .clone()
            .ok_or_else(|| DesktopWorktreeError::TaskHasNoProject(task_id.clone()))?;
        let project = self.get_project(&project_id)?;
        let workspace = project
            .workspace_path
            .as_deref()
            .or_else(|| {
                project
                    .git_workspace
                    .as_ref()
                    .and_then(|workspace| workspace.worktree_path.as_deref())
            })
            .filter(|path| !path.trim().is_empty())
            .ok_or_else(|| DesktopWorktreeError::ProjectHasNoWorkspace(project_id.clone()))?;
        Ok((task, Some(project_id), PathBuf::from(workspace)))
    }

    fn finish_worktree_archive(
        &self,
        task_id: &TaskId,
        status: DesktopWorktreeStatus,
        merged: bool,
        message: &str,
    ) -> Result<DesktopWorktreeMergeResult, DesktopApplicationError> {
        self.inner
            .worktrees
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("worktrees"))?
            .mark_status(task_id, status)?;
        let task = self.get_task(task_id)?;
        let archived = !task.archived;
        if archived {
            self.set_task_archived(task_id, true)?;
        }
        self.emit_event(DesktopEventKind::WorktreeChanged {
            task_id: task_id.clone(),
        });
        Ok(DesktopWorktreeMergeResult {
            merged,
            removed: true,
            archived,
            message: message.to_owned(),
        })
    }
}

fn list_git_worktrees(base_repo_path: &Path) -> Result<Vec<GitWorktree>, DesktopWorktreeError> {
    ensure_git_repo(base_repo_path)?;
    let output = run_git_text(base_repo_path, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_porcelain(&output))
}

fn parse_worktree_porcelain(input: &str) -> Vec<GitWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<GitWorktree> = None;
    for line in input.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(item) = current.take() {
                worktrees.push(item);
            }
            current = Some(GitWorktree {
                path: path.to_owned(),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                prunable: false,
                locked: false,
            });
            continue;
        }
        let Some(item) = current.as_mut() else {
            continue;
        };
        if let Some(head) = line.strip_prefix("HEAD ") {
            item.head = Some(head.to_owned());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            item.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_owned(),
            );
        } else if line == "bare" {
            item.bare = true;
        } else if line == "detached" {
            item.detached = true;
        } else if line.starts_with("prunable") {
            item.prunable = true;
        } else if line.starts_with("locked") {
            item.locked = true;
        }
    }
    if let Some(item) = current {
        worktrees.push(item);
    }
    worktrees
}

fn ensure_git_repo(path: &Path) -> Result<(), DesktopWorktreeError> {
    if !path.is_dir() {
        return Err(DesktopWorktreeError::InvalidPath {
            field: "repository",
            message: format!("directory does not exist: {}", path.display()),
        });
    }
    run_git_text(path, &["rev-parse", "--show-toplevel"]).map(|_| ())
}

fn current_branch(path: &Path) -> Result<String, DesktopWorktreeError> {
    let branch = run_git_text(path, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(DesktopWorktreeError::Detached(normalized_path(path)));
    }
    Ok(branch.to_owned())
}

fn ensure_clean(path: &Path, label: &'static str) -> Result<(), DesktopWorktreeError> {
    if run_git_text(path, &["status", "--porcelain"])?
        .trim()
        .is_empty()
    {
        Ok(())
    } else {
        Err(DesktopWorktreeError::Dirty { label })
    }
}

fn branch_unique_commit_count(
    worktree_path: &Path,
    base_branch: &str,
) -> Result<u64, DesktopWorktreeError> {
    let output = run_git_text(
        worktree_path,
        &["rev-list", "--count", &format!("{base_branch}..HEAD")],
    )?;
    output
        .trim()
        .parse::<u64>()
        .map_err(|error| DesktopWorktreeError::Git {
            command: "rev-list --count".to_owned(),
            message: format!("invalid commit count `{}`: {error}", output.trim()),
        })
}

fn remove_worktree_and_branch(
    base: &Path,
    worktree: &Path,
    branch: &str,
) -> Result<(), DesktopWorktreeError> {
    run_git(
        base,
        &[
            OsString::from("worktree"),
            OsString::from("remove"),
            worktree.as_os_str().to_owned(),
        ],
    )?;
    run_git_text(base, &["branch", "-d", branch])?;
    Ok(())
}

fn rollback_created_worktree(base: &Path, worktree: &Path, branch: &str) {
    let _ = run_git(
        base,
        &[
            OsString::from("worktree"),
            OsString::from("remove"),
            OsString::from("--force"),
            worktree.as_os_str().to_owned(),
        ],
    );
    let _ = run_git_text(base, &["branch", "-D", branch]);
}

fn unique_worktree_target(parent: &Path, slug: &str) -> PathBuf {
    let base = format!("lilia-{slug}");
    let candidate = parent.join(&base);
    if !candidate.exists() {
        return candidate;
    }
    for index in 2..1024 {
        let candidate = parent.join(format!("{base}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{base}-{}", Uuid::new_v4()))
}

fn unique_branch_name(base: &Path, slug: &str) -> Result<String, DesktopWorktreeError> {
    for _ in 0..8 {
        let id = Uuid::new_v4().to_string();
        let branch = format!("lilia/{slug}-{}", &id[..8]);
        if !git_status_success(
            base,
            &[
                OsString::from("show-ref"),
                OsString::from("--verify"),
                OsString::from("--quiet"),
                OsString::from(format!("refs/heads/{branch}")),
            ],
        )? {
            return Ok(branch);
        }
    }
    Ok(format!("lilia/{slug}-{}", Uuid::new_v4()))
}

fn task_title_slug(title: &str, task_id: &TaskId) -> String {
    let slug = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        task_id.as_str().chars().take(8).collect()
    } else {
        slug
    }
}

fn canonical_path(path: &Path, field: &'static str) -> Result<PathBuf, DesktopWorktreeError> {
    path.canonicalize()
        .map(platform_compatible_path)
        .map_err(|error| DesktopWorktreeError::InvalidPath {
            field,
            message: format!("{}: {error}", path.display()),
        })
}

fn platform_compatible_path(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn run_git_text(path: &Path, args: &[&str]) -> Result<String, DesktopWorktreeError> {
    run_git(path, &args.iter().map(OsString::from).collect::<Vec<_>>())
}

fn run_git(path: &Path, args: &[OsString]) -> Result<String, DesktopWorktreeError> {
    let mut command = git_command(path);
    let output = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| DesktopWorktreeError::Git {
            command: display_command(args),
            message: format!("failed to start git: {error}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(DesktopWorktreeError::Git {
            command: display_command(args),
            message: if detail.is_empty() {
                format!("exit {}", output.status.code().unwrap_or(-1))
            } else {
                detail.to_owned()
            },
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_status_success(path: &Path, args: &[OsString]) -> Result<bool, DesktopWorktreeError> {
    git_command(path)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| DesktopWorktreeError::Git {
            command: display_command(args),
            message: format!("failed to start git: {error}"),
        })
}

fn git_command(path: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(path).env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn display_command(args: &[OsString]) -> String {
    args.iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

fn row_to_task_worktree(row: &rusqlite::Row<'_>) -> rusqlite::Result<DesktopTaskWorktree> {
    let task_id =
        TaskId::new(row.get::<_, String>(0)?).map_err(|error| invalid_data(error.to_string()))?;
    let project_id = row
        .get::<_, Option<String>>(1)?
        .map(ProjectId::new)
        .transpose()
        .map_err(|error| invalid_data(error.to_string()))?;
    let status = DesktopWorktreeStatus::parse(&row.get::<_, String>(6)?)
        .map_err(|error| invalid_data(error.to_string()))?;
    Ok(DesktopTaskWorktree {
        task_id,
        project_id,
        base_repo_path: row.get(2)?,
        worktree_path: row.get(3)?,
        branch_name: row.get(4)?,
        base_branch: row.get(5)?,
        status,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn invalid_data(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopWorktreeError {
    #[error("task `{0}` is already bound to an active worktree")]
    AlreadyBound(TaskId),
    #[error("task `{0}` has no active worktree")]
    NotBound(TaskId),
    #[error("task `{0}` does not belong to a project")]
    TaskHasNoProject(TaskId),
    #[error("project `{0}` has no workspace path")]
    ProjectHasNoWorkspace(ProjectId),
    #[error("the main repository cannot be attached as a task worktree")]
    MainRepositoryCannotBeAttached,
    #[error("worktree `{0}` is not registered with the task repository")]
    NotRegistered(String),
    #[error("worktree `{0}` is detached and cannot be bound")]
    Detached(String),
    #[error("{label} contains uncommitted changes")]
    Dirty { label: &'static str },
    #[error("the worktree branch has unmerged commits")]
    UnmergedCommits,
    #[error("the worktree branch has no unique commits to merge")]
    NoUniqueCommits,
    #[error("invalid worktree {field}: {message}")]
    InvalidPath {
        field: &'static str,
        message: String,
    },
    #[error("invalid stored worktree status `{0}`")]
    InvalidStoredStatus(String),
    #[error("invalid stored initial worktree intent `{0}`")]
    InvalidStoredInitialIntent(String),
    #[error("task `{0}` is waiting for its initial worktree to be prepared")]
    InitialPreparationPending(TaskId),
    #[error("git {command} failed: {message}")]
    Git { command: String, message: String },
    #[error("worktree storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask, Project};
    use lilia_service::ServiceAuthority;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn initialize_repository(root: &Path) {
        fs::create_dir_all(root).unwrap();
        run_git_text(root, &["init", "-b", "main"]).unwrap();
        run_git_text(root, &["config", "user.email", "native@example.invalid"]).unwrap();
        run_git_text(root, &["config", "user.name", "Native Test"]).unwrap();
        run_git_text(root, &["config", "core.autocrlf", "false"]).unwrap();
        fs::write(root.join("README.md"), "native\n").unwrap();
        run_git_text(root, &["add", "README.md"]).unwrap();
        run_git_text(root, &["commit", "-m", "initial"]).unwrap();
    }

    fn application(repo: &Path) -> (DesktopApplication, TaskId) {
        let instance_id = Uuid::new_v4();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-worktree:{instance_id}"),
            format!("desktop-worktree-test:{instance_id}"),
        )
        .unwrap();
        let project_id = ProjectId::new("worktree-project").unwrap();
        let mut project = Project::new(project_id.clone(), "Worktree project").unwrap();
        project.workspace_path = Some(normalized_path(repo));
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Project(project))
            .unwrap();
        let task_id = TaskId::new("worktree-task").unwrap();
        authority
            .client()
            .unwrap()
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(task_id.clone(), Some(project_id), "Native worktree").unwrap(),
            ))
            .unwrap();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/worktree-test", "liliacode.test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        (application, task_id)
    }

    #[test]
    fn parses_porcelain_worktree_state() {
        let worktrees = parse_worktree_porcelain(
            "worktree D:/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree D:/repo-wt\nHEAD def\nbranch refs/heads/lilia/task\nlocked reason\n",
        );

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].path, "D:/repo-wt");
        assert_eq!(worktrees[1].branch.as_deref(), Some("lilia/task"));
        assert!(worktrees[1].locked);
    }

    #[test]
    fn create_merge_remove_and_archive_uses_real_git_and_product_state() {
        let root = TempDir::new().unwrap();
        let repo = root.path().join("repo");
        initialize_repository(&repo);
        let (application, task_id) = application(&repo);

        let binding = application
            .create_task_worktree(&task_id, Some(root.path()))
            .unwrap();
        assert_eq!(
            application
                .task_workspace_path(&task_id)
                .unwrap()
                .as_deref(),
            Some(binding.worktree_path.as_str())
        );
        assert!(Path::new(&binding.worktree_path).is_dir());
        assert!(binding.branch_name.starts_with("lilia/native-worktree-"));
        let listed = application
            .list_task_repository_worktrees(&task_id)
            .unwrap();
        assert_eq!(listed.iter().filter(|item| item.is_main).count(), 1);
        assert_eq!(listed.iter().filter(|item| item.is_task_bound).count(), 1);

        let worktree_path = PathBuf::from(&binding.worktree_path);
        fs::write(worktree_path.join("native.txt"), "complete\n").unwrap();
        run_git_text(&worktree_path, &["add", "native.txt"]).unwrap();
        run_git_text(&worktree_path, &["commit", "-m", "native worktree"]).unwrap();

        let result = application
            .merge_task_worktree_and_archive(&task_id)
            .unwrap();

        assert!(result.merged);
        assert!(result.removed);
        assert!(result.archived);
        assert!(!worktree_path.exists());
        assert!(repo.join("native.txt").is_file());
        assert!(application.get_task(&task_id).unwrap().archived);
        assert_eq!(application.task_worktree(&task_id).unwrap(), None);
    }

    #[test]
    fn initial_worktree_intent_survives_reopen_until_explicitly_cleared() {
        let root = TempDir::new().unwrap();
        let database = root.path().join("worktrees.db");
        let task_id = TaskId::new("pending-worktree").unwrap();
        let selection = DesktopInitialWorktreeSelection::Existing(root.path().join("existing"));
        {
            let store = DesktopWorktreeStore::open(&database).unwrap();
            store.save_initial_intent(&task_id, &selection).unwrap();
        }

        let store = DesktopWorktreeStore::open(&database).unwrap();
        assert_eq!(store.initial_intent(&task_id).unwrap(), Some(selection));
        assert!(store.clear_initial_intent(&task_id).unwrap());
        assert_eq!(store.initial_intent(&task_id).unwrap(), None);
    }

    #[test]
    fn pending_initial_worktree_blocks_turns_and_retry_clears_the_gate() {
        let root = TempDir::new().unwrap();
        let repo = root.path().join("repo");
        initialize_repository(&repo);
        let (application, task_id) = application(&repo);
        application
            .set_initial_worktree_intent(&task_id, Some(&DesktopInitialWorktreeSelection::Create))
            .unwrap();

        assert!(matches!(
            application.start_composer_turn(&task_id),
            Err(DesktopApplicationError::Worktree(
                DesktopWorktreeError::InitialPreparationPending(ref pending)
            )) if pending == &task_id
        ));
        assert!(application.retry_initial_worktree(&task_id).unwrap());
        assert_eq!(application.initial_worktree_intent(&task_id).unwrap(), None);
        assert!(application.task_worktree(&task_id).unwrap().is_some());
    }
}
