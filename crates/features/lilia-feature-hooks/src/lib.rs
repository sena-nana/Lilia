//! Hooks domain feature.
//!
//! Owns the user, project and plugin hook documents, the revisioned edits the
//! settings surface performs on them, and the once-per-turn execution fence that
//! keeps a recovered turn from replaying a hook's side effects.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError};
use lilia_storage::Db;
use lilia_storage::{AgentkitHookHandler, AgentkitHooksDocument};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Failures the hooks surface raises while reading or editing hook documents.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum HooksError {
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error("Native Agent operation failed: {0}")]
    Agent(String),
    #[error("desktop {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("desktop {0} state revision overflowed")]
    StateRevisionOverflow(&'static str),
}

pub struct HooksFeature;

impl Feature for HooksFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.hooks").expect("the hooks feature id is not blank")
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

pub const USER_SOURCE_ID: &str = "native-agentkit:user";
pub const PROJECT_SOURCE_ID: &str = "native-agentkit:project";
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
pub enum HookEvent {
    UserPromptSubmit,
    Stop,
}

impl HookEvent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::Stop => "Stop",
        }
    }
}

pub struct HookExecutionStore {
    connection: Db,
}

pub enum HookExecutionDecision {
    Execute,
    Completed,
    Failed(String),
    Indeterminate,
    ConfigurationChanged,
}

#[derive(Debug, thiserror::Error)]
pub enum HookError {
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

impl HookExecutionStore {
    pub fn from_shared(
        connection: Db,
    ) -> Result<Self, HookError> {
        connection
            .lock()
            .execute_batch(HOOK_EXECUTION_SCHEMA)
            .map_err(|error| hook_persistence_error("initialize Hook schema", error))?;
        Ok(Self { connection })
    }

    pub fn begin(
        &self,
        turn_id: &str,
        event: HookEvent,
        source_id: &str,
        handler_id: &str,
        fingerprint: &str,
    ) -> Result<HookExecutionDecision, HookError> {
        let connection = self.connection.lock();
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

    pub fn finish(
        &self,
        turn_id: &str,
        event: HookEvent,
        source_id: &str,
        handler_id: &str,
        fingerprint: &str,
        error: Option<&str>,
    ) -> Result<(), HookError> {
        let connection = self.connection.lock();
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
pub enum HookScope {
    User,
    Project,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSourceView {
    pub id: String,
    pub scope: HookScope,
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
pub struct HookHandlerView {
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
pub struct HookDocumentView {
    pub source: HookSourceView,
    pub handlers: Vec<HookHandlerView>,
    pub raw_document: Option<String>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksOverview {
    pub sources: Vec<HookSourceView>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookHandlerUpdate {
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
pub struct HookDocumentUpdate {
    pub expected_revision: u64,
    pub handlers: Vec<HookHandlerUpdate>,
}


pub fn hook_source_view(
    scope: HookScope,
    project_cwd: Option<String>,
    path: PathBuf,
    document: Option<&AgentkitHooksDocument>,
) -> HookSourceView {
    let exists = document.is_some();
    let enabled = document.is_some_and(|document| document.enabled);
    HookSourceView {
        id: match scope {
            HookScope::User => USER_SOURCE_ID.to_owned(),
            HookScope::Project => format!(
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

pub fn hook_handler_view(handler: &AgentkitHookHandler) -> HookHandlerView {
    let executable = platform_command(handler).is_some();
    HookHandlerView {
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

pub fn hook_handler_update(
    input: HookHandlerUpdate,
    index: usize,
) -> Result<AgentkitHookHandler, HooksError> {
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

pub fn ensure_hook_revision(actual: u64, expected: u64) -> Result<(), HooksError> {
    if actual == expected {
        Ok(())
    } else {
        Err(invalid_hook_input(
            "expected_revision",
            format!("stale Hook source revision {expected}; current revision is {actual}"),
        ))
    }
}

pub fn bump_hook_revision(document: &mut AgentkitHooksDocument) -> Result<(), HooksError> {
    document.revision =
        document
            .revision
            .checked_add(1)
            .ok_or(HooksError::StateRevisionOverflow(
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

pub fn hook_matches(matcher: Option<&str>, context: &str) -> bool {
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

pub fn hook_fingerprint(
    source_id: &str,
    revision: u64,
    handler: &AgentkitHookHandler,
) -> Result<String, HookError> {
    let bytes = serde_json::to_vec(&(source_id, revision, handler))
        .map_err(|error| hook_execution_error(source_id, &handler.id, error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn execute_hook_command(
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

pub fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub fn invalid_hook_input(field: &'static str, message: impl Into<String>) -> HooksError {
    HooksError::InvalidInput {
        field,
        message: message.into(),
    }
}

pub fn hook_io_error(action: &str, error: impl std::fmt::Display) -> HooksError {
    HooksError::Agent(format!("{action}: {error}"))
}

pub fn hook_persistence_error(
    operation: &'static str,
    message: impl std::fmt::Display,
) -> HookError {
    HookError::Persistence {
        operation,
        message: message.to_string(),
    }
}

pub fn hook_execution_error(
    source_id: impl Into<String>,
    handler_id: impl Into<String>,
    message: impl Into<String>,
) -> HookError {
    HookError::Execution {
        source_id: source_id.into(),
        handler_id: handler_id.into(),
        message: message.into(),
    }
}

