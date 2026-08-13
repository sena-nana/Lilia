use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lilia_contracts::TaskId;
use lilia_storage::{AgentkitHookHandler, AgentkitHooksDocument};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::legacy_database::SharedLegacyConnection;
use crate::{DesktopApplication, DesktopApplicationError};

const USER_SOURCE_ID: &str = "native-agentkit:user";
const PROJECT_SOURCE_ID: &str = "native-agentkit:project";
const DEFAULT_HOOK_TIMEOUT_SECONDS: u64 = 30;
const MAX_CAPTURED_HOOK_OUTPUT: usize = 64 * 1024;

const HOOK_EXECUTION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS desktop_hook_executions (
  turn_id TEXT NOT NULL,
  event TEXT NOT NULL,
  source_id TEXT NOT NULL,
  handler_id TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  state TEXT NOT NULL,
  error_message TEXT,
  started_at INTEGER NOT NULL,
  completed_at INTEGER,
  PRIMARY KEY (turn_id, event, source_id, handler_id)
);
CREATE INDEX IF NOT EXISTS idx_desktop_hook_executions_turn
  ON desktop_hook_executions(turn_id, event);
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesktopHookEvent {
    UserPromptSubmit,
    Stop,
}

impl DesktopHookEvent {
    const fn as_str(self) -> &'static str {
        match self {
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::Stop => "Stop",
        }
    }
}

pub(crate) struct DesktopHookExecutionStore {
    connection: SharedLegacyConnection,
}

enum HookExecutionDecision {
    Execute,
    Completed,
    Failed(String),
    Indeterminate,
    ConfigurationChanged,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopHookError {
    #[error("Hook persistence failed during {operation}: {message}")]
    Persistence {
        operation: &'static str,
        message: String,
    },
    #[error("Hook `{source_id}/{handler_id}` failed: {message}")]
    Execution {
        source_id: String,
        handler_id: String,
        message: String,
    },
}

impl DesktopHookExecutionStore {
    pub(crate) fn from_shared(
        connection: SharedLegacyConnection,
    ) -> Result<Self, DesktopHookError> {
        connection
            .lock()
            .map_err(|_| hook_persistence_error("lock Hook database", "connection lock poisoned"))?
            .execute_batch(HOOK_EXECUTION_SCHEMA)
            .map_err(|error| hook_persistence_error("initialize Hook schema", error))?;
        Ok(Self { connection })
    }

    fn begin(
        &self,
        turn_id: &str,
        event: DesktopHookEvent,
        source_id: &str,
        handler_id: &str,
        fingerprint: &str,
    ) -> Result<HookExecutionDecision, DesktopHookError> {
        let connection = self.connection.lock().map_err(|_| {
            hook_persistence_error("lock Hook execution", "connection lock poisoned")
        })?;
        let inserted = connection
            .execute(
                "INSERT OR IGNORE INTO desktop_hook_executions
                 (turn_id, event, source_id, handler_id, fingerprint, state, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'started', ?6)",
                params![
                    turn_id,
                    event.as_str(),
                    source_id,
                    handler_id,
                    fingerprint,
                    unix_millis(),
                ],
            )
            .map_err(|error| hook_persistence_error("start Hook execution", error))?;
        if inserted == 1 {
            return Ok(HookExecutionDecision::Execute);
        }
        let stored = connection
            .query_row(
                "SELECT fingerprint, state, error_message
                 FROM desktop_hook_executions
                 WHERE turn_id = ?1 AND event = ?2 AND source_id = ?3 AND handler_id = ?4",
                params![turn_id, event.as_str(), source_id, handler_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| hook_persistence_error("read Hook execution", error))?
            .ok_or_else(|| {
                hook_persistence_error("read Hook execution", "execution row disappeared")
            })?;
        if stored.0 != fingerprint {
            return Ok(HookExecutionDecision::ConfigurationChanged);
        }
        Ok(match stored.1.as_str() {
            "completed" => HookExecutionDecision::Completed,
            "failed" => HookExecutionDecision::Failed(
                stored
                    .2
                    .unwrap_or_else(|| "previous execution failed".to_owned()),
            ),
            _ => HookExecutionDecision::Indeterminate,
        })
    }

    fn finish(
        &self,
        turn_id: &str,
        event: DesktopHookEvent,
        source_id: &str,
        handler_id: &str,
        fingerprint: &str,
        error: Option<&str>,
    ) -> Result<(), DesktopHookError> {
        let connection = self.connection.lock().map_err(|_| {
            hook_persistence_error("lock Hook completion", "connection lock poisoned")
        })?;
        let updated = connection
            .execute(
                "UPDATE desktop_hook_executions
                 SET state = ?1, error_message = ?2, completed_at = ?3
                 WHERE turn_id = ?4 AND event = ?5 AND source_id = ?6 AND handler_id = ?7
                   AND fingerprint = ?8 AND state = 'started'",
                params![
                    if error.is_some() {
                        "failed"
                    } else {
                        "completed"
                    },
                    error,
                    unix_millis(),
                    turn_id,
                    event.as_str(),
                    source_id,
                    handler_id,
                    fingerprint,
                ],
            )
            .map_err(|error| hook_persistence_error("finish Hook execution", error))?;
        if updated != 1 {
            return Err(hook_persistence_error(
                "finish Hook execution",
                "execution fence was no longer active",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopHookScope {
    User,
    Project,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookSourceView {
    pub id: String,
    pub scope: DesktopHookScope,
    pub project_cwd: Option<String>,
    pub path: String,
    pub exists: bool,
    pub editable: bool,
    pub enabled: bool,
    pub revision: u64,
    pub handler_count: usize,
    pub trust_state: String,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookHandlerView {
    pub id: String,
    pub event: String,
    pub matcher: Option<String>,
    #[serde(rename = "type")]
    pub handler_type: String,
    pub command: Option<String>,
    pub command_windows: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub status_message: Option<String>,
    pub supported: bool,
    pub executable: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookDocumentView {
    pub source: DesktopHookSourceView,
    pub handlers: Vec<DesktopHookHandlerView>,
    pub raw_document: Option<String>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHooksOverview {
    pub sources: Vec<DesktopHookSourceView>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookHandlerUpdate {
    pub id: Option<String>,
    pub event: String,
    pub matcher: Option<String>,
    #[serde(rename = "type")]
    pub handler_type: String,
    pub command: Option<String>,
    pub command_windows: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub status_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookDocumentUpdate {
    pub expected_revision: u64,
    pub handlers: Vec<DesktopHookHandlerUpdate>,
}

impl DesktopApplication {
    pub fn hooks_overview(
        &self,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHooksOverview, DesktopApplicationError> {
        let mut sources = vec![self.hook_source(DesktopHookScope::User, None)?];
        if let Some(project_cwd) = project_cwd.filter(|value| !value.trim().is_empty()) {
            sources.push(self.hook_source(DesktopHookScope::Project, Some(project_cwd))?);
        }
        Ok(DesktopHooksOverview {
            sources,
            warnings: Vec::new(),
        })
    }

    pub fn hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?;
        Ok(hook_source_view(
            scope,
            project_cwd,
            path,
            document.as_ref(),
        ))
    }

    pub fn read_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookDocumentView, DesktopApplicationError> {
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?;
        let source = hook_source_view(scope, project_cwd, path, document.as_ref());
        let handlers = document
            .as_ref()
            .map(|document| document.handlers.iter().map(hook_handler_view).collect())
            .unwrap_or_default();
        let raw_document = document
            .as_ref()
            .map(serde_json::to_string_pretty)
            .transpose()
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        Ok(DesktopHookDocumentView {
            source: source.clone(),
            handlers,
            raw_document,
            warnings: source.warnings.clone(),
            limitations: source.limitations.clone(),
        })
    }

    pub fn create_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        if path.exists() {
            return Err(invalid_hook_input("source", "Hook source already exists"));
        }
        let document = AgentkitHooksDocument {
            revision: 1,
            ..AgentkitHooksDocument::default()
        };
        lilia_storage::save_hooks_document(&path, &document)?;
        Ok(hook_source_view(scope, project_cwd, path, Some(&document)))
    }

    pub fn update_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        input: DesktopHookDocumentUpdate,
    ) -> Result<DesktopHookDocumentView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let mut document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, input.expected_revision)?;
        document.handlers = input
            .handlers
            .into_iter()
            .enumerate()
            .map(|(index, handler)| hook_handler_update(handler, index))
            .collect::<Result<Vec<_>, _>>()?;
        bump_hook_revision(&mut document)?;
        lilia_storage::save_hooks_document(&path, &document)?;
        drop(_guard);
        self.read_hook_source(scope, project_cwd.as_deref())
    }

    pub fn set_hook_source_enabled(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        expected_revision: u64,
        enabled: bool,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let mut document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, expected_revision)?;
        if document.enabled != enabled {
            document.enabled = enabled;
            bump_hook_revision(&mut document)?;
            lilia_storage::save_hooks_document(&path, &document)?;
        }
        Ok(hook_source_view(scope, project_cwd, path, Some(&document)))
    }

    pub fn delete_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        expected_revision: u64,
    ) -> Result<(), DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, _) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, expected_revision)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid_hook_input("source", "Hook source has an invalid path"))?;
        let staged = path.with_file_name(format!(".{file_name}.deleting-{}", uuid::Uuid::new_v4()));
        fs::rename(&path, &staged).map_err(|error| hook_io_error("stage Hook deletion", error))?;
        if let Err(error) = fs::remove_file(&staged) {
            let _ = fs::rename(&staged, &path);
            return Err(hook_io_error("delete Hook source", error));
        }
        Ok(())
    }

    pub(crate) fn execute_turn_hooks(
        &self,
        event: DesktopHookEvent,
        task_id: &TaskId,
        turn_id: &str,
        workspace_path: Option<&str>,
        context: &str,
    ) -> Result<(), DesktopHookError> {
        let mut sources = vec![(
            USER_SOURCE_ID.to_owned(),
            lilia_storage::user_hooks_document_path(&self.config().data_paths()),
            None,
            None,
        )];
        if let Some(workspace_path) = workspace_path.filter(|value| !value.trim().is_empty()) {
            let workspace = Path::new(workspace_path);
            if workspace.is_absolute() && workspace.is_dir() {
                let workspace = workspace.canonicalize().map_err(|error| {
                    hook_execution_error(
                        PROJECT_SOURCE_ID,
                        "source",
                        format!("resolve project workspace: {error}"),
                    )
                })?;
                sources.push((
                    PROJECT_SOURCE_ID.to_owned(),
                    lilia_storage::project_hooks_document_path(&workspace),
                    None,
                    None,
                ));
            }
        }
        for package in self.loaded_plugin_packages() {
            for (source_id, path, document) in package.hooks {
                sources.push((source_id, path, Some(package.root.clone()), Some(document)));
            }
        }

        for (source_id, path, plugin_root, loaded_document) in sources {
            let document = match loaded_document {
                Some(document) => document,
                None => {
                    let Some(document) =
                        lilia_storage::load_hooks_document(&path).map_err(|error| {
                            hook_execution_error(&source_id, "source", error.to_string())
                        })?
                    else {
                        continue;
                    };
                    document
                }
            };
            if !document.enabled {
                continue;
            }
            for handler in document
                .handlers
                .iter()
                .filter(|handler| handler.event == event.as_str())
                .filter(|handler| hook_matches(handler.matcher.as_deref(), context))
            {
                let fingerprint = hook_fingerprint(&source_id, document.revision, handler)?;
                match self.inner.hook_executions.begin(
                    turn_id,
                    event,
                    &source_id,
                    &handler.id,
                    &fingerprint,
                )? {
                    HookExecutionDecision::Completed => continue,
                    HookExecutionDecision::Failed(message) => {
                        return Err(hook_execution_error(&source_id, &handler.id, message));
                    }
                    HookExecutionDecision::Indeterminate => {
                        return Err(hook_execution_error(
                            &source_id,
                            &handler.id,
                            "previous execution outcome is unknown; refusing to replay side effects",
                        ));
                    }
                    HookExecutionDecision::ConfigurationChanged => {
                        return Err(hook_execution_error(
                            &source_id,
                            &handler.id,
                            "Hook configuration changed after this turn began; refusing recovery replay",
                        ));
                    }
                    HookExecutionDecision::Execute => {}
                }
                let payload = serde_json::to_vec(&json!({
                    "event": event.as_str(),
                    "taskId": task_id.as_str(),
                    "turnId": turn_id,
                    "projectDir": workspace_path,
                    "context": context,
                }))
                .map_err(|error| {
                    hook_execution_error(&source_id, &handler.id, error.to_string())
                })?;
                let result =
                    execute_hook_command(handler, workspace_path, plugin_root.as_deref(), &payload);
                let failure = result.as_ref().err().map(String::as_str);
                self.inner.hook_executions.finish(
                    turn_id,
                    event,
                    &source_id,
                    &handler.id,
                    &fingerprint,
                    failure,
                )?;
                if let Err(message) = result {
                    return Err(hook_execution_error(&source_id, &handler.id, message));
                }
            }
        }
        Ok(())
    }

    fn resolve_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<(PathBuf, Option<String>), DesktopApplicationError> {
        match scope {
            DesktopHookScope::User => Ok((
                lilia_storage::user_hooks_document_path(&self.config().data_paths()),
                None,
            )),
            DesktopHookScope::Project => {
                let raw = project_cwd
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        invalid_hook_input("project_cwd", "project Hook source requires a project")
                    })?;
                let project = Path::new(raw);
                if !project.is_absolute() || !project.is_dir() {
                    return Err(invalid_hook_input(
                        "project_cwd",
                        "project Hook source requires an existing absolute directory",
                    ));
                }
                let project = project
                    .canonicalize()
                    .map_err(|error| hook_io_error("resolve project Hook workspace", error))?;
                Ok((
                    lilia_storage::project_hooks_document_path(&project),
                    Some(project.to_string_lossy().into_owned()),
                ))
            }
        }
    }
}

fn hook_source_view(
    scope: DesktopHookScope,
    project_cwd: Option<String>,
    path: PathBuf,
    document: Option<&AgentkitHooksDocument>,
) -> DesktopHookSourceView {
    let exists = document.is_some();
    let enabled = document.is_some_and(|document| document.enabled);
    DesktopHookSourceView {
        id: match scope {
            DesktopHookScope::User => USER_SOURCE_ID.to_owned(),
            DesktopHookScope::Project => format!(
                "{PROJECT_SOURCE_ID}:{}",
                project_cwd.as_deref().map_or_else(
                    || "missing".to_owned(),
                    |cwd| format!("{:x}", Sha256::digest(cwd.as_bytes()))
                )
            ),
        },
        scope,
        project_cwd,
        path: path.to_string_lossy().into_owned(),
        exists,
        editable: true,
        enabled,
        revision: document.map_or(0, |document| document.revision),
        handler_count: document.map_or(0, |document| document.handlers.len()),
        trust_state: if !exists {
            "n_a"
        } else if enabled {
            "managed"
        } else {
            "required"
        }
        .to_owned(),
        warnings: Vec::new(),
        limitations: vec![
            "支持 UserPromptSubmit 与 Stop command Handler；项目来源只对对应任务工作区生效"
                .to_owned(),
        ],
    }
}

fn hook_handler_view(handler: &AgentkitHookHandler) -> DesktopHookHandlerView {
    let executable = platform_command(handler).is_some();
    DesktopHookHandlerView {
        id: handler.id.clone(),
        event: handler.event.clone(),
        matcher: handler.matcher.clone(),
        handler_type: handler.handler_type.clone(),
        command: handler.command.clone(),
        command_windows: handler.command_windows.clone(),
        timeout_seconds: handler.timeout_seconds,
        status_message: handler.status_message.clone(),
        supported: matches!(handler.event.as_str(), "UserPromptSubmit" | "Stop")
            && handler.handler_type == "command",
        executable,
        warnings: if executable {
            Vec::new()
        } else {
            vec!["当前平台没有可执行命令".to_owned()]
        },
    }
}

fn hook_handler_update(
    input: DesktopHookHandlerUpdate,
    index: usize,
) -> Result<AgentkitHookHandler, DesktopApplicationError> {
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("handler-{}", index + 1));
    let event = input.event.trim().to_owned();
    let handler_type = input.handler_type.trim().to_owned();
    Ok(AgentkitHookHandler {
        id,
        event,
        matcher: normalized_optional(input.matcher),
        handler_type,
        command: normalized_optional(input.command),
        command_windows: normalized_optional(input.command_windows),
        timeout_seconds: input.timeout_seconds,
        status_message: normalized_optional(input.status_message),
    })
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn ensure_hook_revision(actual: u64, expected: u64) -> Result<(), DesktopApplicationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid_hook_input(
            "expected_revision",
            format!("stale Hook source revision {expected}; current revision is {actual}"),
        ))
    }
}

fn bump_hook_revision(document: &mut AgentkitHooksDocument) -> Result<(), DesktopApplicationError> {
    document.revision =
        document
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "Hook source",
            ))?;
    Ok(())
}

fn platform_command(handler: &AgentkitHookHandler) -> Option<&str> {
    #[cfg(windows)]
    {
        handler
            .command_windows
            .as_deref()
            .or(handler.command.as_deref())
    }
    #[cfg(not(windows))]
    {
        handler.command.as_deref()
    }
}

fn hook_matches(matcher: Option<&str>, context: &str) -> bool {
    let Some(matcher) = matcher.filter(|matcher| !matcher.is_empty()) else {
        return true;
    };
    if matcher == "*" {
        return true;
    }
    wildcard_match(matcher.as_bytes(), context.as_bytes())
}

fn wildcard_match(pattern: &[u8], value: &[u8]) -> bool {
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star_index, mut star_value_index) = (None, 0);
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn hook_fingerprint(
    source_id: &str,
    revision: u64,
    handler: &AgentkitHookHandler,
) -> Result<String, DesktopHookError> {
    let bytes = serde_json::to_vec(&(source_id, revision, handler))
        .map_err(|error| hook_execution_error(source_id, &handler.id, error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn execute_hook_command(
    handler: &AgentkitHookHandler,
    workspace_path: Option<&str>,
    plugin_root: Option<&Path>,
    payload: &[u8],
) -> Result<(), String> {
    let command_text = platform_command(handler)
        .ok_or_else(|| "current platform has no configured command".to_owned())?;
    let mut command = shell_command(command_text);
    command
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    copy_hook_environment(&mut command);
    if let Some(plugin_root) = plugin_root {
        command.env("LILIA_PLUGIN_ROOT", plugin_root);
    }
    if let Some(workspace_path) = workspace_path {
        command.current_dir(workspace_path);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("start command: {error}"))?;
    let process_tree = HookProcessTree::attach(&child).ok();
    let stdout = child.stdout.take().map(drain_hook_output);
    let stderr = child.stderr.take().map(drain_hook_output);
    let mut stdin = child.stdin.take();
    let payload = payload.to_vec();
    let stdin_writer = std::thread::spawn(move || {
        if let Some(stdin) = stdin.as_mut() {
            stdin.write_all(&payload)?;
        }
        Ok::<(), std::io::Error>(())
    });
    let timeout = Duration::from_secs(
        handler
            .timeout_seconds
            .unwrap_or(DEFAULT_HOOK_TIMEOUT_SECONDS),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait for command: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            terminate_hook_process_tree(&child, process_tree.as_ref());
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdin_writer.join();
            if let Some(stdout) = stdout {
                let _ = stdout.join();
            }
            if let Some(stderr) = stderr {
                let _ = stderr.join();
            }
            return Err(format!(
                "command timed out after {} seconds",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let input_result = stdin_writer
        .join()
        .map_err(|_| "command input writer panicked".to_owned())?;
    if let Err(error) = input_result {
        if error.kind() != std::io::ErrorKind::BrokenPipe {
            return Err(format!("write command input: {error}"));
        }
    }
    if let Some(stdout) = stdout {
        stdout
            .join()
            .map_err(|_| "command output reader panicked".to_owned())?
            .map_err(|error| format!("read command output: {error}"))?;
    }
    if let Some(stderr) = stderr {
        stderr
            .join()
            .map_err(|_| "command error reader panicked".to_owned())?
            .map_err(|error| format!("read command error output: {error}"))?;
    }
    if status.success() {
        Ok(())
    } else {
        Err(status.code().map_or_else(
            || "command terminated without an exit code".to_owned(),
            |code| format!("command exited with code {code}"),
        ))
    }
}

fn drain_hook_output(
    mut stream: impl Read + Send + 'static,
) -> std::thread::JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let mut captured = Vec::with_capacity(MAX_CAPTURED_HOOK_OUTPUT);
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            let read = stream.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let remaining = MAX_CAPTURED_HOOK_OUTPUT.saturating_sub(captured.len());
            captured.extend_from_slice(&buffer[..read.min(remaining)]);
        }
        Ok(())
    })
}

fn copy_hook_environment(command: &mut Command) {
    const KEYS: &[&str] = &[
        "PATH",
        "SystemRoot",
        "ComSpec",
        "PATHEXT",
        "TEMP",
        "TMP",
        "HOME",
        "TMPDIR",
    ];
    for key in KEYS {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

#[cfg(windows)]
fn shell_command(command_text: &str) -> Command {
    use std::os::windows::process::CommandExt;

    let shell = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut command = Command::new(shell);
    command.args(["/D", "/S", "/C", command_text]);
    command.creation_flags(0x0800_0000);
    command
}

#[cfg(not(windows))]
fn shell_command(command_text: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", command_text]);
    command
}

#[cfg(windows)]
struct HookProcessTree {
    handle: usize,
}

#[cfg(windows)]
impl HookProcessTree {
    fn attach(child: &Child) -> Result<Self, ()> {
        use std::ffi::c_void;
        use std::os::windows::io::AsRawHandle;

        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|_| ())?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        let process = HANDLE(child.as_raw_handle());
        if configured.is_err() || unsafe { AssignProcessToJobObject(handle, process) }.is_err() {
            let _ = unsafe { CloseHandle(handle) };
            return Err(());
        }
        Ok(Self {
            handle: handle.0 as usize,
        })
    }

    fn terminate(&self) {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::TerminateJobObject;

        let _ = unsafe { TerminateJobObject(HANDLE(self.handle as *mut _), 1) };
    }
}

#[cfg(windows)]
impl Drop for HookProcessTree {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};

        let _ = unsafe { CloseHandle(HANDLE(self.handle as *mut _)) };
    }
}

#[cfg(not(windows))]
struct HookProcessTree;

#[cfg(not(windows))]
impl HookProcessTree {
    fn attach(_child: &Child) -> Result<Self, ()> {
        Ok(Self)
    }
}

#[cfg(windows)]
fn terminate_hook_process_tree(_child: &Child, process_tree: Option<&HookProcessTree>) {
    if let Some(process_tree) = process_tree {
        process_tree.terminate();
    }
}

#[cfg(not(windows))]
fn terminate_hook_process_tree(_child: &Child, _process_tree: Option<&HookProcessTree>) {}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn invalid_hook_input(field: &'static str, message: impl Into<String>) -> DesktopApplicationError {
    DesktopApplicationError::InvalidInput {
        field,
        message: message.into(),
    }
}

fn hook_io_error(action: &str, error: impl std::fmt::Display) -> DesktopApplicationError {
    DesktopApplicationError::Agent(format!("{action}: {error}"))
}

fn hook_persistence_error(
    operation: &'static str,
    message: impl std::fmt::Display,
) -> DesktopHookError {
    DesktopHookError::Persistence {
        operation,
        message: message.to_string(),
    }
}

fn hook_execution_error(
    source_id: impl Into<String>,
    handler_id: impl Into<String>,
    message: impl Into<String>,
) -> DesktopHookError {
    DesktopHookError::Execution {
        source_id: source_id.into(),
        handler_id: handler_id.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{DesktopApplicationConfig, DesktopHost};

    #[derive(Default)]
    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &crate::DesktopHostContext,
            _action: crate::DesktopHostAction,
        ) -> Result<crate::DesktopHostResult, crate::DesktopHostError> {
            Ok(crate::DesktopHostResult::Completed)
        }
    }

    fn application(home: &Path) -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:hooks:{}", uuid::Uuid::new_v4()),
            "hooks-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(home, "hooks-test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn hook_source_lifecycle_is_revisioned_and_project_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let application = application(&home);

        let created = application
            .create_hook_source(DesktopHookScope::Project, project.to_str())
            .unwrap();
        assert_eq!(created.revision, 1);
        assert!(!created.enabled);
        assert!(
            created.path.ends_with(".lilia\\hooks.json")
                || created.path.ends_with(".lilia/hooks.json")
        );

        let updated = application
            .update_hook_source(
                DesktopHookScope::Project,
                project.to_str(),
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: vec![DesktopHookHandlerUpdate {
                        id: Some("prompt-check".to_owned()),
                        event: "UserPromptSubmit".to_owned(),
                        matcher: None,
                        handler_type: "command".to_owned(),
                        command: Some("check".to_owned()),
                        command_windows: None,
                        timeout_seconds: Some(5),
                        status_message: None,
                    }],
                },
            )
            .unwrap();
        assert_eq!(updated.source.revision, 2);
        assert_eq!(updated.handlers.len(), 1);
        assert!(application
            .update_hook_source(
                DesktopHookScope::Project,
                project.to_str(),
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: Vec::new(),
                },
            )
            .is_err());

        let enabled = application
            .set_hook_source_enabled(DesktopHookScope::Project, project.to_str(), 2, true)
            .unwrap();
        assert_eq!(enabled.revision, 3);
        assert!(enabled.enabled);
        application
            .delete_hook_source(DesktopHookScope::Project, project.to_str(), 3)
            .unwrap();
        assert!(!Path::new(&enabled.path).exists());
    }

    #[test]
    fn enabled_hook_executes_once_per_turn_and_completed_fence_skips_replay() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let marker = project.join("hook-ran.txt");
        let application = application(&home);
        application
            .create_hook_source(DesktopHookScope::User, None)
            .unwrap();
        #[cfg(windows)]
        let command = "echo ran>>hook-ran.txt".to_owned();
        #[cfg(not(windows))]
        let command = format!("printf 'ran\\n' >> '{}'", marker.display());
        application
            .update_hook_source(
                DesktopHookScope::User,
                None,
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: vec![DesktopHookHandlerUpdate {
                        id: Some("once".to_owned()),
                        event: "UserPromptSubmit".to_owned(),
                        matcher: Some("*ship*".to_owned()),
                        handler_type: "command".to_owned(),
                        command: Some(command.clone()),
                        command_windows: Some(command),
                        timeout_seconds: Some(5),
                        status_message: None,
                    }],
                },
            )
            .unwrap();
        application
            .set_hook_source_enabled(DesktopHookScope::User, None, 2, true)
            .unwrap();
        let task_id = TaskId::new("hook-task").unwrap();
        application
            .execute_turn_hooks(
                DesktopHookEvent::UserPromptSubmit,
                &task_id,
                "hook-turn",
                project.to_str(),
                "please ship this",
            )
            .unwrap();
        application
            .execute_turn_hooks(
                DesktopHookEvent::UserPromptSubmit,
                &task_id,
                "hook-turn",
                project.to_str(),
                "please ship this",
            )
            .unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }

    #[test]
    fn started_execution_fence_refuses_uncertain_replay() {
        let directory = tempfile::tempdir().unwrap();
        let application = application(directory.path());
        let store = &application.inner.hook_executions;
        assert!(matches!(
            store
                .begin(
                    "uncertain-turn",
                    DesktopHookEvent::UserPromptSubmit,
                    USER_SOURCE_ID,
                    "side-effect",
                    "fingerprint",
                )
                .unwrap(),
            HookExecutionDecision::Execute
        ));
        assert!(matches!(
            store
                .begin(
                    "uncertain-turn",
                    DesktopHookEvent::UserPromptSubmit,
                    USER_SOURCE_ID,
                    "side-effect",
                    "fingerprint",
                )
                .unwrap(),
            HookExecutionDecision::Indeterminate
        ));
    }
}
