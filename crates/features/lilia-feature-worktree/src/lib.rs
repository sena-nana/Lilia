//! Worktree domain feature.
//!
//! Owns the per-task git worktree: where it lives, which branch it tracks and
//! the git commands that create, merge and remove it.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

mod jobs;

use lilia_contracts::{ProjectId, TaskId};
use lilia_storage::Db;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;


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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Merged => "merged",
            Self::Removed => "removed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DesktopWorktreeError> {
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
pub struct GitWorktree {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub prunable: bool,
    pub locked: bool,
}

pub struct DesktopWorktreeStore {
    connection: Db,
}

impl DesktopWorktreeStore {
    pub fn from_db(connection: Db) -> Result<Self, DesktopWorktreeError> {
        connection
            .lock()
            .execute_batch(WORKTREE_SCHEMA)
            .map_err(|error| DesktopWorktreeError::Storage {
                operation: "initialize worktree schema",
                message: error.to_string(),
            })?;
        Ok(Self { connection })
    }

    pub fn active_for_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopTaskWorktree>, DesktopWorktreeError> {
        self.connection
            .lock()
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

    pub fn active_bound_paths(&self) -> Result<BTreeSet<String>, DesktopWorktreeError> {
        let connection = self.connection.lock();
        let mut statement = connection
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

    pub fn save_active(
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
            .lock()
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

    pub fn mark_status(
        &self,
        task_id: &TaskId,
        status: DesktopWorktreeStatus,
    ) -> Result<bool, DesktopWorktreeError> {
        let changed = self.connection
                .lock()
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

    pub fn save_initial_intent(
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
            .lock()
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

    pub fn initial_intent(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopInitialWorktreeSelection>, DesktopWorktreeError> {
        let stored = self.connection
                .lock()
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

    pub fn clear_initial_intent(&self, task_id: &TaskId) -> Result<bool, DesktopWorktreeError> {
        self.connection
            .lock()
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


pub fn parse_worktree_porcelain(input: &str) -> Vec<GitWorktree> {
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

pub fn ensure_git_repo(path: &Path) -> Result<(), DesktopWorktreeError> {
    if !path.is_dir() {
        return Err(DesktopWorktreeError::InvalidPath {
            field: "repository",
            message: format!("directory does not exist: {}", path.display()),
        });
    }
    run_git_text(path, &["rev-parse", "--show-toplevel"]).map(|_| ())
}

pub fn current_branch(path: &Path) -> Result<String, DesktopWorktreeError> {
    let branch = run_git_text(path, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(DesktopWorktreeError::Detached(normalized_path(path)));
    }
    Ok(branch.to_owned())
}

pub fn ensure_clean(path: &Path, label: &'static str) -> Result<(), DesktopWorktreeError> {
    if run_git_text(path, &["status", "--porcelain"])?
        .trim()
        .is_empty()
    {
        Ok(())
    } else {
        Err(DesktopWorktreeError::Dirty { label })
    }
}

pub fn branch_unique_commit_count(
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

pub fn remove_worktree_and_branch(
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

pub fn rollback_created_worktree(base: &Path, worktree: &Path, branch: &str) {
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

pub fn unique_worktree_target(parent: &Path, slug: &str) -> PathBuf {
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

pub fn unique_branch_name(base: &Path, slug: &str) -> Result<String, DesktopWorktreeError> {
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

pub fn task_title_slug(title: &str, task_id: &TaskId) -> String {
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

pub fn canonical_path(path: &Path, field: &'static str) -> Result<PathBuf, DesktopWorktreeError> {
    path.canonicalize()
        .map(platform_compatible_path)
        .map_err(|error| DesktopWorktreeError::InvalidPath {
            field,
            message: format!("{}: {error}", path.display()),
        })
}

pub fn platform_compatible_path(path: PathBuf) -> PathBuf {
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

pub fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn run_git_text(path: &Path, args: &[&str]) -> Result<String, DesktopWorktreeError> {
    run_git(path, &args.iter().map(OsString::from).collect::<Vec<_>>())
}

pub fn run_git(path: &Path, args: &[OsString]) -> Result<String, DesktopWorktreeError> {
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

pub fn git_status_success(path: &Path, args: &[OsString]) -> Result<bool, DesktopWorktreeError> {
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

pub fn git_command(path: &Path) -> Command {
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

pub fn display_command(args: &[OsString]) -> String {
    args.iter()
        .map(|argument| argument.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn row_to_task_worktree(row: &rusqlite::Row<'_>) -> rusqlite::Result<DesktopTaskWorktree> {
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

pub fn invalid_data(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

pub fn now_millis() -> i64 {
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


use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobProtocol, KernelError, ServiceKey, ServiceRef,
};

pub use jobs::{
    worktree_slot, WorktreeOperationRequest, WorktreePort, WorktreeRequest, OPERATE_PROTOCOL,
};

/// Service slot for [`DesktopWorktreeStore`].
pub enum WorktreeStoreKey {}

impl ServiceKey for WorktreeStoreKey {
    type Value = Arc<DesktopWorktreeStore>;

    const NAME: &'static str = "lilia.worktree";
}

pub struct WorktreeFeature {
    db: Db,
    port: Arc<dyn WorktreePort>,
}

impl WorktreeFeature {
    pub fn new(db: Db, port: Arc<dyn WorktreePort>) -> Self {
        Self { db, port }
    }
}

impl Feature for WorktreeFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.worktree").expect("the worktree feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        vec![jobs::operate_protocol(Arc::clone(&self.port))]
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<WorktreeStoreKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        let store =
            DesktopWorktreeStore::from_db(self.db.clone()).map_err(|error| KernelError::Mount {
                feature: self.id(),
                source: Box::new(error),
            })?;
        cx.provide::<WorktreeStoreKey>(Arc::new(store))
    }
}
