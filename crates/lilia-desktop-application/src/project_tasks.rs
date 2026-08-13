use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use lilia_contracts::{ProjectArchiveState, ProjectId};
use serde::{Deserialize, Serialize};

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopTerminalCommand, DesktopTerminalError,
    DesktopTerminalLaunch, DesktopTerminalScope, DesktopTerminalSessionId, DesktopTerminalSnapshot,
    ProjectContext,
};

const TASKS_RELATIVE_PATH: &str = ".lilia/tasks.json";
const TASKS_SCHEMA_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: u64 = 256 * 1024;
const MAX_TASKS: usize = 64;
const MAX_TASK_ID_CHARS: usize = 64;
const MAX_LABEL_CHARS: usize = 120;
const MAX_PROGRAM_CHARS: usize = 1_024;
const MAX_ARGUMENTS: usize = 128;
const MAX_ARGUMENT_CHARS: usize = 8_192;
const MAX_ENVIRONMENT_ENTRIES: usize = 128;
const MAX_ENVIRONMENT_VALUE_CHARS: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectTaskCatalog {
    pub project_id: ProjectId,
    pub source_path: PathBuf,
    pub tasks: Vec<DesktopProjectTaskView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectTaskView {
    pub id: String,
    pub label: String,
    pub allow_concurrent_runs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_session_id: Option<DesktopTerminalSessionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectTaskLaunch {
    pub project_id: ProjectId,
    pub task_id: String,
    pub terminal: DesktopTerminalSnapshot,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTaskFile {
    version: u32,
    tasks: Vec<ProjectTaskDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTaskDefinition {
    id: String,
    label: String,
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    allow_concurrent_runs: bool,
}

impl DesktopApplication {
    pub fn project_task_catalog(
        &self,
        project_id: &ProjectId,
    ) -> Result<DesktopProjectTaskCatalog, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(DesktopProjectTaskError::ArchivedProject(project_id.clone()).into());
        }
        let context = ProjectContext::from_project(&project)?;
        let source_path = context.active_root().join(TASKS_RELATIVE_PATH);
        let definitions = read_project_tasks(context.active_root(), &source_path)?;
        let active_runs = self
            .inner
            .project_task_runs
            .lock()
            .map_err(|_| DesktopProjectTaskError::StateUnavailable)?
            .clone();
        let tasks = definitions
            .into_iter()
            .map(|definition| {
                let key = (project_id.as_str().to_owned(), definition.id.clone());
                let running_session_id = active_runs.get(&key).and_then(|session_id| {
                    self.terminal_snapshot(session_id, 0)
                        .ok()
                        .filter(|snapshot| snapshot.process.is_running())
                        .map(|snapshot| snapshot.id)
                });
                DesktopProjectTaskView {
                    id: definition.id,
                    label: definition.label,
                    allow_concurrent_runs: definition.allow_concurrent_runs,
                    running_session_id,
                }
            })
            .collect();
        Ok(DesktopProjectTaskCatalog {
            project_id: project_id.clone(),
            source_path,
            tasks,
        })
    }

    pub fn launch_project_task(
        &self,
        project_id: &ProjectId,
        task_id: &str,
    ) -> Result<DesktopProjectTaskLaunch, DesktopApplicationError> {
        let task_id = validate_requested_task_id(task_id)?;
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(DesktopProjectTaskError::ArchivedProject(project_id.clone()).into());
        }
        let context = ProjectContext::from_project(&project)?;
        let source_path = context.active_root().join(TASKS_RELATIVE_PATH);
        let definition = read_project_tasks(context.active_root(), &source_path)?
            .into_iter()
            .find(|definition| definition.id == task_id)
            .ok_or_else(|| DesktopProjectTaskError::TaskNotFound {
                project_id: project_id.clone(),
                task_id: task_id.to_owned(),
            })?;

        let key = (project_id.as_str().to_owned(), task_id.to_owned());
        let mut active_runs = self
            .inner
            .project_task_runs
            .lock()
            .map_err(|_| DesktopProjectTaskError::StateUnavailable)?;
        if !definition.allow_concurrent_runs {
            if let Some(session_id) = active_runs.get(&key) {
                match self.terminal_snapshot(session_id, 0) {
                    Ok(snapshot) if snapshot.process.is_running() => {
                        return Err(DesktopProjectTaskError::AlreadyRunning {
                            project_id: project_id.clone(),
                            task_id: task_id.to_owned(),
                            session_id: session_id.clone(),
                        }
                        .into());
                    }
                    Ok(_)
                    | Err(DesktopApplicationError::Terminal(
                        DesktopTerminalError::SessionNotFound(_),
                    )) => {
                        active_runs.remove(&key);
                    }
                    Err(error) => return Err(error),
                }
            }
        }

        let terminal = self.launch_terminal(DesktopTerminalLaunch {
            scope: DesktopTerminalScope::Project(project_id.clone()),
            command: Some(DesktopTerminalCommand {
                program: definition.program,
                arguments: definition.args,
                environment: definition.env,
                label: Some(definition.label),
            }),
            rows: 24,
            columns: 80,
        })?;
        active_runs.insert(key, terminal.id.clone());
        Ok(DesktopProjectTaskLaunch {
            project_id: project_id.clone(),
            task_id: task_id.to_owned(),
            terminal,
        })
    }
}

fn read_project_tasks(
    workspace_root: &Path,
    source_path: &Path,
) -> Result<Vec<ProjectTaskDefinition>, DesktopProjectTaskError> {
    let canonical_root = std::fs::canonicalize(workspace_root)
        .map_err(|error| io_error("resolve workspace", workspace_root, error))?;
    let canonical_source = match std::fs::canonicalize(source_path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error("resolve", source_path, error)),
    };
    if !canonical_source.starts_with(&canonical_root) {
        return Err(DesktopProjectTaskError::ConfigEscapesWorkspace(
            canonical_source,
        ));
    }
    let mut file = File::open(&canonical_source)
        .map_err(|error| io_error("open", &canonical_source, error))?;
    let declared_size = file
        .metadata()
        .map_err(|error| io_error("inspect", source_path, error))?
        .len();
    if declared_size > MAX_CONFIG_BYTES {
        return Err(DesktopProjectTaskError::ConfigTooLarge(declared_size));
    }
    let mut bytes = Vec::with_capacity(declared_size as usize);
    file.by_ref()
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read", source_path, error))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(DesktopProjectTaskError::ConfigTooLarge(bytes.len() as u64));
    }
    let file: ProjectTaskFile =
        serde_json::from_slice(&bytes).map_err(|error| DesktopProjectTaskError::InvalidConfig {
            path: source_path.to_path_buf(),
            message: error.to_string(),
        })?;
    validate_project_task_file(file)
}

fn validate_project_task_file(
    file: ProjectTaskFile,
) -> Result<Vec<ProjectTaskDefinition>, DesktopProjectTaskError> {
    if file.version != TASKS_SCHEMA_VERSION {
        return Err(DesktopProjectTaskError::UnsupportedVersion(file.version));
    }
    if file.tasks.len() > MAX_TASKS {
        return Err(DesktopProjectTaskError::TooManyTasks(file.tasks.len()));
    }
    let mut ids = BTreeSet::new();
    for task in &file.tasks {
        validate_task_id(&task.id)?;
        if !ids.insert(task.id.clone()) {
            return Err(DesktopProjectTaskError::DuplicateTaskId(task.id.clone()));
        }
        validate_text("label", &task.label, MAX_LABEL_CHARS, true)?;
        validate_text("program", &task.program, MAX_PROGRAM_CHARS, true)?;
        if task.label.chars().any(char::is_control) {
            return Err(DesktopProjectTaskError::InvalidField {
                field: "label",
                max_chars: MAX_LABEL_CHARS,
            });
        }
        if task.program.chars().any(char::is_control) {
            return Err(DesktopProjectTaskError::InvalidField {
                field: "program",
                max_chars: MAX_PROGRAM_CHARS,
            });
        }
        if task.args.len() > MAX_ARGUMENTS {
            return Err(DesktopProjectTaskError::TooManyArguments {
                task_id: task.id.clone(),
                count: task.args.len(),
            });
        }
        for argument in &task.args {
            validate_text("argument", argument, MAX_ARGUMENT_CHARS, false)?;
        }
        if task.env.len() > MAX_ENVIRONMENT_ENTRIES {
            return Err(DesktopProjectTaskError::TooManyEnvironmentEntries {
                task_id: task.id.clone(),
                count: task.env.len(),
            });
        }
        for (key, value) in &task.env {
            if key.is_empty() || key.contains('=') || key.chars().any(char::is_control) {
                return Err(DesktopProjectTaskError::InvalidEnvironmentKey(key.clone()));
            }
            validate_text(
                "environment value",
                value,
                MAX_ENVIRONMENT_VALUE_CHARS,
                false,
            )?;
        }
    }
    Ok(file.tasks)
}

fn validate_requested_task_id(task_id: &str) -> Result<&str, DesktopProjectTaskError> {
    validate_task_id(task_id)?;
    Ok(task_id)
}

fn validate_task_id(task_id: &str) -> Result<(), DesktopProjectTaskError> {
    if task_id.is_empty()
        || task_id.chars().count() > MAX_TASK_ID_CHARS
        || !task_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(DesktopProjectTaskError::InvalidTaskId(task_id.to_owned()));
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
    require_non_empty: bool,
) -> Result<(), DesktopProjectTaskError> {
    if (require_non_empty && value.trim().is_empty())
        || value.chars().count() > max_chars
        || value.contains('\0')
    {
        return Err(DesktopProjectTaskError::InvalidField { field, max_chars });
    }
    Ok(())
}

fn io_error(
    operation: &'static str,
    path: &Path,
    error: std::io::Error,
) -> DesktopProjectTaskError {
    DesktopProjectTaskError::Io {
        operation,
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopProjectTaskError {
    #[error("project `{0}` is archived")]
    ArchivedProject(ProjectId),
    #[error("project task id `{0}` is invalid")]
    InvalidTaskId(String),
    #[error("project task config contains duplicate id `{0}`")]
    DuplicateTaskId(String),
    #[error("project task config field `{field}` is invalid or exceeds {max_chars} characters")]
    InvalidField {
        field: &'static str,
        max_chars: usize,
    },
    #[error("project task environment key `{0}` is invalid")]
    InvalidEnvironmentKey(String),
    #[error("project task config version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("project task config contains {0} tasks; at most {MAX_TASKS} are allowed")]
    TooManyTasks(usize),
    #[error(
        "project task `{task_id}` contains {count} arguments; at most {MAX_ARGUMENTS} are allowed"
    )]
    TooManyArguments { task_id: String, count: usize },
    #[error(
        "project task `{task_id}` contains {count} environment entries; at most {MAX_ENVIRONMENT_ENTRIES} are allowed"
    )]
    TooManyEnvironmentEntries { task_id: String, count: usize },
    #[error("project task config is too large: {0} bytes")]
    ConfigTooLarge(u64),
    #[error("project task config resolves outside the active workspace: `{0}`")]
    ConfigEscapesWorkspace(PathBuf),
    #[error("project task config `{path}` is invalid: {message}")]
    InvalidConfig { path: PathBuf, message: String },
    #[error("project task `{task_id}` does not exist in project `{project_id}`")]
    TaskNotFound {
        project_id: ProjectId,
        task_id: String,
    },
    #[error("project task `{task_id}` is already running in terminal `{session_id}`")]
    AlreadyRunning {
        project_id: ProjectId,
        task_id: String,
        session_id: DesktopTerminalSessionId,
    },
    #[error("project task {operation} failed for `{path}`: {message}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        message: String,
    },
    #[error("project task state is unavailable")]
    StateUnavailable,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate,
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

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

    fn app_with_project(root: &Path) -> (DesktopApplication, ProjectId) {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:project-tasks:{id}"),
            format!("project-tasks-test:{id}"),
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                format!("/tmp/lilia-project-tasks-{id}"),
                format!("liliacode.project-tasks-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let project = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(root.display().to_string()),
                ..DesktopProjectCreate::new("Tasks")
            })
            .unwrap();
        (app, project.id)
    }

    fn write_tasks(root: &Path, tasks: serde_json::Value) {
        let directory = root.join(".lilia");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("tasks.json"),
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "tasks": tasks,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn catalog_exposes_safe_task_metadata_without_command_or_environment() {
        let root = tempfile::tempdir().unwrap();
        write_tasks(
            root.path(),
            serde_json::json!([{
                "id": "check",
                "label": "Cargo check",
                "program": "cargo",
                "args": ["check"],
                "env": { "ACCESS_TOKEN": "secret" }
            }]),
        );
        let (app, project_id) = app_with_project(root.path());

        let catalog = app.project_task_catalog(&project_id).unwrap();

        assert_eq!(catalog.tasks.len(), 1);
        assert_eq!(catalog.tasks[0].id, "check");
        assert_eq!(catalog.tasks[0].label, "Cargo check");
        let serialized = serde_json::to_string(&catalog).unwrap();
        assert!(!serialized.contains("cargo"));
        assert!(!serialized.contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn task_launch_uses_project_terminal_and_blocks_duplicate_running_task() {
        let root = tempfile::tempdir().unwrap();
        write_tasks(
            root.path(),
            serde_json::json!([{
                "id": "serve",
                "label": "Serve",
                "program": "/bin/sh",
                "args": ["-lc", "sleep 5"]
            }]),
        );
        let (app, project_id) = app_with_project(root.path());

        let launched = app.launch_project_task(&project_id, "serve").unwrap();
        assert_eq!(
            launched.terminal.scope,
            DesktopTerminalScope::Project(project_id.clone())
        );
        assert_eq!(launched.terminal.cwd, root.path().canonicalize().unwrap());
        assert!(matches!(
            app.launch_project_task(&project_id, "serve"),
            Err(DesktopApplicationError::ProjectTask(
                DesktopProjectTaskError::AlreadyRunning { .. }
            ))
        ));
        app.terminate_terminal(&launched.terminal.id).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn config_symlink_cannot_escape_workspace() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), br#"{"version":1,"tasks":[]}"#).unwrap();
        fs::create_dir_all(root.path().join(".lilia")).unwrap();
        symlink(outside.path(), root.path().join(TASKS_RELATIVE_PATH)).unwrap();
        let (app, project_id) = app_with_project(root.path());

        assert!(matches!(
            app.project_task_catalog(&project_id),
            Err(DesktopApplicationError::ProjectTask(
                DesktopProjectTaskError::ConfigEscapesWorkspace(_)
            ))
        ));
    }
}
