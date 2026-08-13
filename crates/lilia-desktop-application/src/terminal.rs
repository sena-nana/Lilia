use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use lilia_contracts::{ProjectArchiveState, ProjectId, TaskId};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DesktopApplication, DesktopApplicationError};

const TERMINAL_SCROLLBACK_ROWS: usize = 10_000;
const MIN_TERMINAL_ROWS: u16 = 2;
const MAX_TERMINAL_ROWS: u16 = 500;
const MIN_TERMINAL_COLUMNS: u16 = 8;
const MAX_TERMINAL_COLUMNS: u16 = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DesktopTerminalSessionId(String);

impl DesktopTerminalSessionId {
    fn new() -> Self {
        Self(format!("terminal-{}", Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DesktopTerminalSessionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum DesktopTerminalScope {
    Project(ProjectId),
    Task(TaskId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalCommand {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub label: Option<String>,
}

impl DesktopTerminalCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalLaunch {
    pub scope: DesktopTerminalScope,
    pub command: Option<DesktopTerminalCommand>,
    pub rows: u16,
    pub columns: u16,
}

impl DesktopTerminalLaunch {
    pub fn shell(scope: DesktopTerminalScope) -> Self {
        Self {
            scope,
            command: None,
            rows: 24,
            columns: 80,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DesktopTerminalProcessState {
    Running,
    Terminating,
    Exited {
        success: bool,
        exit_code: u32,
        signal: Option<String>,
    },
    Failed {
        message: String,
    },
    Restored,
}

impl DesktopTerminalProcessState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running | Self::Terminating)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DesktopTerminalColor {
    Default,
    Indexed(u8),
    Rgb([u8; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalStyle {
    pub foreground: DesktopTerminalColor,
    pub background: DesktopTerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalRow {
    pub text: String,
    pub styles: Vec<DesktopTerminalStyleSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalStyleSpan {
    pub start: usize,
    pub end: usize,
    pub style: DesktopTerminalStyle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalSnapshot {
    pub id: DesktopTerminalSessionId,
    pub scope: DesktopTerminalScope,
    pub cwd: PathBuf,
    pub command_label: String,
    pub process_id: Option<u32>,
    pub rows: u16,
    pub columns: u16,
    pub cursor_row: u16,
    pub cursor_column: u16,
    pub cursor_visible: bool,
    pub scrollback_position: usize,
    pub maximum_scrollback_position: usize,
    pub screen: Vec<DesktopTerminalRow>,
    pub process: DesktopTerminalProcessState,
    pub revision: u64,
    pub output_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTerminalRestoration {
    pub id: DesktopTerminalSessionId,
    pub scope: DesktopTerminalScope,
    pub cwd: PathBuf,
    pub command_label: String,
    pub rows: u16,
    pub columns: u16,
}

impl DesktopTerminalRestoration {
    pub fn from_snapshot(snapshot: &DesktopTerminalSnapshot) -> Self {
        Self {
            id: snapshot.id.clone(),
            scope: snapshot.scope.clone(),
            cwd: snapshot.cwd.clone(),
            command_label: snapshot.command_label.clone(),
            rows: snapshot.rows,
            columns: snapshot.columns,
        }
    }

    pub fn snapshot(&self) -> DesktopTerminalSnapshot {
        DesktopTerminalSnapshot {
            id: self.id.clone(),
            scope: self.scope.clone(),
            cwd: self.cwd.clone(),
            command_label: self.command_label.clone(),
            process_id: None,
            rows: self.rows,
            columns: self.columns,
            cursor_row: 0,
            cursor_column: 0,
            cursor_visible: false,
            scrollback_position: 0,
            maximum_scrollback_position: 0,
            screen: vec![DesktopTerminalRow {
                text: "该终端会话已随上次应用退出而结束。".to_owned(),
                styles: Vec::new(),
            }],
            process: DesktopTerminalProcessState::Restored,
            revision: 0,
            output_error: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopTerminalError {
    #[error("terminal rows must be between {MIN_TERMINAL_ROWS} and {MAX_TERMINAL_ROWS}")]
    InvalidRows,
    #[error("terminal columns must be between {MIN_TERMINAL_COLUMNS} and {MAX_TERMINAL_COLUMNS}")]
    InvalidColumns,
    #[error("terminal command program must not be empty or contain control characters")]
    InvalidProgram,
    #[error("terminal command label must not be empty or contain control characters")]
    InvalidCommandLabel,
    #[error("terminal environment key `{0}` is invalid")]
    InvalidEnvironmentKey(String),
    #[error("terminal scope has no workspace directory")]
    MissingWorkspace,
    #[error("terminal workspace `{0}` is not a directory")]
    WorkspaceNotDirectory(PathBuf),
    #[error("terminal session `{0}` does not exist")]
    SessionNotFound(DesktopTerminalSessionId),
    #[error("terminal session `{0}` is no longer running")]
    SessionNotRunning(DesktopTerminalSessionId),
    #[error("terminal session `{0}` is still running")]
    SessionStillRunning(DesktopTerminalSessionId),
    #[error("terminal {operation} failed: {message}")]
    Operation {
        operation: &'static str,
        message: String,
    },
    #[error("terminal state is unavailable")]
    StateUnavailable,
}

pub(crate) struct DesktopTerminalService {
    sessions: Mutex<HashMap<DesktopTerminalSessionId, DesktopTerminalSession>>,
}

impl Default for DesktopTerminalService {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

struct DesktopTerminalSession {
    id: DesktopTerminalSessionId,
    scope: DesktopTerminalScope,
    cwd: PathBuf,
    command_label: String,
    process_id: Option<u32>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    state: Arc<Mutex<DesktopTerminalRuntimeState>>,
    registration: TerminalRegistration,
}

type TerminalRegistration = Arc<(Mutex<bool>, Condvar)>;

struct DesktopTerminalRuntimeState {
    parser: vt100::Parser,
    process: DesktopTerminalProcessState,
    revision: u64,
    output_error: Option<String>,
}

impl DesktopApplication {
    pub fn launch_terminal(
        &self,
        launch: DesktopTerminalLaunch,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        validate_size(launch.rows, launch.columns)?;
        validate_command(launch.command.as_ref())?;
        let cwd = self.terminal_workspace_root(&launch.scope)?;
        let session = DesktopTerminalSession::spawn(
            launch,
            cwd,
            self.inner.events.clone(),
            self.config().instance_identity().to_owned(),
        )?;
        let session_id = session.id.clone();
        let registration = session.registration.clone();
        self.inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?
            .insert(session.id.clone(), session);
        mark_terminal_registered(&registration);
        self.terminal_snapshot(&session_id, 0)
    }

    pub fn terminal_snapshot(
        &self,
        session_id: &DesktopTerminalSessionId,
        scrollback_position: usize,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        let sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| DesktopTerminalError::SessionNotFound(session_id.clone()))?;
        Ok(session.snapshot(scrollback_position)?)
    }

    pub fn list_terminal_sessions(
        &self,
    ) -> Result<Vec<DesktopTerminalSnapshot>, DesktopApplicationError> {
        let sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let mut snapshots = sessions
            .values()
            .map(|session| session.snapshot(0))
            .collect::<Result<Vec<_>, _>>()?;
        snapshots.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(snapshots)
    }

    pub fn write_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
        input: &[u8],
    ) -> Result<(), DesktopApplicationError> {
        if input.is_empty() {
            return Ok(());
        }
        let mut sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopTerminalError::SessionNotFound(session_id.clone()))?;
        session.ensure_running()?;
        session
            .writer
            .write_all(input)
            .and_then(|()| session.writer.flush())
            .map_err(|error| operation_error("write input", error))?;
        Ok(())
    }

    pub fn resize_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
        rows: u16,
        columns: u16,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        validate_size(rows, columns)?;
        let sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| DesktopTerminalError::SessionNotFound(session_id.clone()))?;
        session
            .master
            .resize(pty_size(rows, columns))
            .map_err(|error| DesktopTerminalError::Operation {
                operation: "resize",
                message: error.to_string(),
            })?;
        let mut state = session
            .state
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        state.parser.screen_mut().set_size(rows, columns);
        state.revision = state.revision.saturating_add(1);
        drop(state);
        Ok(session.snapshot(0)?)
    }

    pub fn terminate_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
    ) -> Result<(), DesktopApplicationError> {
        let mut sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| DesktopTerminalError::SessionNotFound(session_id.clone()))?;
        session.ensure_running()?;
        session
            .killer
            .kill()
            .map_err(|error| operation_error("terminate", error))?;
        let mut state = session
            .state
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        state.process = DesktopTerminalProcessState::Terminating;
        state.revision = state.revision.saturating_add(1);
        Ok(())
    }

    pub fn forget_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
    ) -> Result<(), DesktopApplicationError> {
        let mut sessions = self
            .inner
            .terminals
            .sessions
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| DesktopTerminalError::SessionNotFound(session_id.clone()))?;
        if session.is_running()? {
            return Err(DesktopTerminalError::SessionStillRunning(session_id.clone()).into());
        }
        sessions.remove(session_id);
        Ok(())
    }

    pub(crate) fn terminal_workspace_root(
        &self,
        scope: &DesktopTerminalScope,
    ) -> Result<PathBuf, DesktopApplicationError> {
        let root = match scope {
            DesktopTerminalScope::Project(project_id) => {
                let project = self.get_project(project_id)?;
                if project.archive == ProjectArchiveState::Archived {
                    return Err(DesktopApplicationError::InvalidInput {
                        field: "terminalScope",
                        message: format!("project `{}` is archived", project_id.as_str()),
                    });
                }
                crate::ProjectContext::from_project(&project)?
                    .active_root()
                    .to_path_buf()
            }
            DesktopTerminalScope::Task(task_id) => {
                let task = self.get_task(task_id)?;
                if task.archived {
                    return Err(DesktopApplicationError::InvalidInput {
                        field: "terminalScope",
                        message: format!("task `{}` is archived", task_id.as_str()),
                    });
                }
                if let Some(worktree) = self.task_worktree(task_id)? {
                    PathBuf::from(worktree.worktree_path)
                } else {
                    let project_id = task
                        .project_id
                        .ok_or(DesktopTerminalError::MissingWorkspace)?;
                    crate::ProjectContext::from_project(&self.get_project(&project_id)?)?
                        .active_root()
                        .to_path_buf()
                }
            }
        };
        canonical_directory(&root).map_err(DesktopApplicationError::from)
    }
}

impl DesktopTerminalSession {
    fn spawn(
        launch: DesktopTerminalLaunch,
        cwd: PathBuf,
        events: crate::DesktopEventBus,
        source_instance: String,
    ) -> Result<Self, DesktopTerminalError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(pty_size(launch.rows, launch.columns))
            .map_err(|error| DesktopTerminalError::Operation {
                operation: "open PTY",
                message: error.to_string(),
            })?;
        let mut reader =
            pair.master
                .try_clone_reader()
                .map_err(|error| DesktopTerminalError::Operation {
                    operation: "open output reader",
                    message: error.to_string(),
                })?;
        let writer =
            pair.master
                .take_writer()
                .map_err(|error| DesktopTerminalError::Operation {
                    operation: "open input writer",
                    message: error.to_string(),
                })?;
        let (command, command_label) = command_builder(&launch, &cwd);
        let mut child =
            pair.slave
                .spawn_command(command)
                .map_err(|error| DesktopTerminalError::Operation {
                    operation: "spawn process",
                    message: error.to_string(),
                })?;
        drop(pair.slave);

        let process_id = child.process_id();
        let mut killer = child.clone_killer();
        let state = Arc::new(Mutex::new(DesktopTerminalRuntimeState {
            parser: vt100::Parser::new(launch.rows, launch.columns, TERMINAL_SCROLLBACK_ROWS),
            process: DesktopTerminalProcessState::Running,
            revision: 1,
            output_error: None,
        }));
        let registration = Arc::new((Mutex::new(false), Condvar::new()));

        let session_id = DesktopTerminalSessionId::new();
        let wait_state = state.clone();
        let wait_events = events.clone();
        let wait_source_instance = source_instance.clone();
        let wait_session_id = session_id.clone();
        let wait_registration = registration.clone();
        let wait_spawn = std::thread::Builder::new()
            .name(format!(
                "lilia-terminal-wait-{}",
                process_id.unwrap_or_default()
            ))
            .spawn(move || {
                let process = match child.wait() {
                    Ok(status) => DesktopTerminalProcessState::Exited {
                        success: status.success(),
                        exit_code: status.exit_code(),
                        signal: status.signal().map(str::to_owned),
                    },
                    Err(error) => DesktopTerminalProcessState::Failed {
                        message: error.to_string(),
                    },
                };
                if let Ok(mut state) = wait_state.lock() {
                    state.process = process;
                    state.revision = state.revision.saturating_add(1);
                    wait_until_terminal_registered(&wait_registration);
                    wait_events.publish(
                        wait_source_instance,
                        crate::DesktopEventKind::TerminalChanged {
                            session_id: wait_session_id,
                            revision: state.revision,
                        },
                    );
                }
            });
        if let Err(error) = wait_spawn {
            mark_terminal_registered(&registration);
            let _ = killer.kill();
            return Err(DesktopTerminalError::Operation {
                operation: "start process waiter",
                message: error.to_string(),
            });
        }

        let reader_state = state.clone();
        let reader_events = events;
        let reader_session_id = session_id.clone();
        let reader_registration = registration.clone();
        let reader_spawn = std::thread::Builder::new()
            .name(format!(
                "lilia-terminal-read-{}",
                process_id.unwrap_or_default()
            ))
            .spawn(move || {
                let mut buffer = [0_u8; 16 * 1024];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(read) => {
                            let Ok(mut state) = reader_state.lock() else {
                                break;
                            };
                            state.parser.process(&buffer[..read]);
                            state.revision = state.revision.saturating_add(1);
                            wait_until_terminal_registered(&reader_registration);
                            reader_events.publish(
                                source_instance.clone(),
                                crate::DesktopEventKind::TerminalChanged {
                                    session_id: reader_session_id.clone(),
                                    revision: state.revision,
                                },
                            );
                        }
                        Err(error) => {
                            if let Ok(mut state) = reader_state.lock() {
                                if state.process.is_running() {
                                    state.output_error = Some(error.to_string());
                                    state.revision = state.revision.saturating_add(1);
                                    wait_until_terminal_registered(&reader_registration);
                                    reader_events.publish(
                                        source_instance.clone(),
                                        crate::DesktopEventKind::TerminalChanged {
                                            session_id: reader_session_id.clone(),
                                            revision: state.revision,
                                        },
                                    );
                                }
                            }
                            break;
                        }
                    }
                }
            });
        if let Err(error) = reader_spawn {
            mark_terminal_registered(&registration);
            let _ = killer.kill();
            return Err(DesktopTerminalError::Operation {
                operation: "start output reader",
                message: error.to_string(),
            });
        }

        Ok(Self {
            id: session_id,
            scope: launch.scope,
            cwd,
            command_label,
            process_id,
            master: pair.master,
            writer,
            killer,
            state,
            registration,
        })
    }

    fn snapshot(
        &self,
        scrollback_position: usize,
    ) -> Result<DesktopTerminalSnapshot, DesktopTerminalError> {
        let state = self
            .state
            .lock()
            .map_err(|_| DesktopTerminalError::StateUnavailable)?;
        let mut screen = state.parser.screen().clone();
        let mut maximum_screen = screen.clone();
        maximum_screen.set_scrollback(usize::MAX);
        let maximum_scrollback_position = maximum_screen.scrollback();
        screen.set_scrollback(scrollback_position);
        let (rows, columns) = screen.size();
        let (cursor_row, cursor_column) = screen.cursor_position();
        Ok(DesktopTerminalSnapshot {
            id: self.id.clone(),
            scope: self.scope.clone(),
            cwd: self.cwd.clone(),
            command_label: self.command_label.clone(),
            process_id: self.process_id,
            rows,
            columns,
            cursor_row,
            cursor_column,
            cursor_visible: !screen.hide_cursor(),
            scrollback_position: screen.scrollback(),
            maximum_scrollback_position,
            screen: snapshot_rows(&screen),
            process: state.process.clone(),
            revision: state.revision,
            output_error: state.output_error.clone(),
        })
    }

    fn is_running(&self) -> Result<bool, DesktopTerminalError> {
        self.state
            .lock()
            .map(|state| state.process.is_running())
            .map_err(|_| DesktopTerminalError::StateUnavailable)
    }

    fn ensure_running(&self) -> Result<(), DesktopTerminalError> {
        self.is_running()?
            .then_some(())
            .ok_or_else(|| DesktopTerminalError::SessionNotRunning(self.id.clone()))
    }
}

fn mark_terminal_registered(registration: &TerminalRegistration) {
    let (registered, signal) = &**registration;
    if let Ok(mut registered) = registered.lock() {
        *registered = true;
        signal.notify_all();
    }
}

fn wait_until_terminal_registered(registration: &TerminalRegistration) {
    let (registered, signal) = &**registration;
    let Ok(mut registered) = registered.lock() else {
        return;
    };
    while !*registered {
        let Ok(next) = signal.wait(registered) else {
            return;
        };
        registered = next;
    }
}

impl Drop for DesktopTerminalService {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.get_mut() {
            for session in sessions.values_mut() {
                if session.is_running().unwrap_or(false) {
                    let _ = session.killer.kill();
                }
            }
        }
    }
}

fn validate_size(rows: u16, columns: u16) -> Result<(), DesktopTerminalError> {
    if !(MIN_TERMINAL_ROWS..=MAX_TERMINAL_ROWS).contains(&rows) {
        return Err(DesktopTerminalError::InvalidRows);
    }
    if !(MIN_TERMINAL_COLUMNS..=MAX_TERMINAL_COLUMNS).contains(&columns) {
        return Err(DesktopTerminalError::InvalidColumns);
    }
    Ok(())
}

fn validate_command(command: Option<&DesktopTerminalCommand>) -> Result<(), DesktopTerminalError> {
    let Some(command) = command else {
        return Ok(());
    };
    if command.program.trim().is_empty() || command.program.chars().any(char::is_control) {
        return Err(DesktopTerminalError::InvalidProgram);
    }
    if command
        .label
        .as_deref()
        .is_some_and(|label| label.trim().is_empty() || label.chars().any(char::is_control))
    {
        return Err(DesktopTerminalError::InvalidCommandLabel);
    }
    if let Some(key) = command
        .environment
        .keys()
        .find(|key| key.is_empty() || key.contains('=') || key.chars().any(char::is_control))
    {
        return Err(DesktopTerminalError::InvalidEnvironmentKey(key.clone()));
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, DesktopTerminalError> {
    let canonical =
        std::fs::canonicalize(path).map_err(|error| DesktopTerminalError::Operation {
            operation: "resolve workspace",
            message: error.to_string(),
        })?;
    if !canonical.is_dir() {
        return Err(DesktopTerminalError::WorkspaceNotDirectory(canonical));
    }
    Ok(canonical)
}

fn command_builder(launch: &DesktopTerminalLaunch, cwd: &Path) -> (CommandBuilder, String) {
    let (mut command, label) = if let Some(specification) = &launch.command {
        let mut command = CommandBuilder::new(&specification.program);
        command.args(&specification.arguments);
        for (key, value) in &specification.environment {
            command.env(key, value);
        }
        (
            command,
            specification
                .label
                .clone()
                .unwrap_or_else(|| specification.program.clone()),
        )
    } else {
        (CommandBuilder::new_default_prog(), "Shell".to_owned())
    };
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    (command, label)
}

fn pty_size(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn snapshot_rows(screen: &vt100::Screen) -> Vec<DesktopTerminalRow> {
    let (rows, columns) = screen.size();
    (0..rows)
        .map(|row| {
            let mut text = String::new();
            let mut styles = Vec::new();
            let mut current_style = None;
            let mut current_start = 0;
            for column in 0..columns {
                let Some(cell) = screen.cell(row, column) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let style = terminal_style(cell);
                let start = text.len();
                if cell.has_contents() {
                    text.push_str(cell.contents());
                } else {
                    text.push(' ');
                }
                if current_style != Some(style) {
                    if let Some(previous) = current_style {
                        styles.push(DesktopTerminalStyleSpan {
                            start: current_start,
                            end: start,
                            style: previous,
                        });
                    }
                    current_style = Some(style);
                    current_start = start;
                }
            }
            if let Some(style) = current_style {
                styles.push(DesktopTerminalStyleSpan {
                    start: current_start,
                    end: text.len(),
                    style,
                });
            }
            DesktopTerminalRow { text, styles }
        })
        .collect()
}

fn terminal_style(cell: &vt100::Cell) -> DesktopTerminalStyle {
    DesktopTerminalStyle {
        foreground: terminal_color(cell.fgcolor()),
        background: terminal_color(cell.bgcolor()),
        bold: cell.bold(),
        dim: cell.dim(),
        italic: cell.italic(),
        underline: cell.underline(),
        inverse: cell.inverse(),
    }
}

fn terminal_color(color: vt100::Color) -> DesktopTerminalColor {
    match color {
        vt100::Color::Default => DesktopTerminalColor::Default,
        vt100::Color::Idx(index) => DesktopTerminalColor::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => DesktopTerminalColor::Rgb([red, green, blue]),
    }
}

fn operation_error(operation: &'static str, error: std::io::Error) -> DesktopTerminalError {
    DesktopTerminalError::Operation {
        operation,
        message: error.to_string(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

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
            Err(DesktopHostError::new(
                "unsupported",
                "terminal test host",
                false,
            ))
        }
    }

    fn app_with_project(root: &Path) -> (DesktopApplication, ProjectId) {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:terminal:{id}"),
            format!("terminal-test:{id}"),
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                std::env::temp_dir().join(format!("lilia-terminal-test-{id}")),
                format!("liliacode.terminal-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let project = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(root.display().to_string()),
                ..DesktopProjectCreate::new("Terminal")
            })
            .unwrap();
        (app, project.id)
    }

    fn wait_for_snapshot(
        app: &DesktopApplication,
        id: &DesktopTerminalSessionId,
        predicate: impl Fn(&DesktopTerminalSnapshot) -> bool,
    ) -> DesktopTerminalSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = app.terminal_snapshot(id, 0).unwrap();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "terminal state did not converge");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn screen_text(snapshot: &DesktopTerminalSnapshot) -> String {
        snapshot
            .screen
            .iter()
            .map(|row| row.text.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn terminal_runs_in_project_and_round_trips_input_through_a_real_pty() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec![
            "-lc".to_owned(),
            "printf '\\033[31mready\\033[0m:%s\\n' \"$PWD\"; read line; printf 'input:%s\\n' \"$line\""
                .to_owned(),
        ];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 80,
            })
            .unwrap();

        let ready = wait_for_snapshot(&app, &launched.id, |snapshot| {
            screen_text(snapshot).contains("ready:")
        });
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        assert!(screen_text(&ready).contains(&format!("ready:{}", canonical_root.display())));
        assert!(ready
            .screen
            .iter()
            .flat_map(|row| &row.styles)
            .any(|span| { span.style.foreground == DesktopTerminalColor::Indexed(1) }));

        app.write_terminal(&launched.id, b"hello\r").unwrap();
        let completed = wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running() && screen_text(snapshot).contains("input:hello")
        });
        assert!(matches!(
            completed.process,
            DesktopTerminalProcessState::Exited { success: true, .. }
        ));
        app.forget_terminal(&launched.id).unwrap();
        assert!(app.list_terminal_sessions().unwrap().is_empty());
    }

    #[test]
    fn terminal_resize_and_terminate_are_explicit_lifecycle_operations() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec!["-lc".to_owned(), "sleep 30".to_owned()];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 40,
            })
            .unwrap();
        let resized = app.resize_terminal(&launched.id, 12, 60).unwrap();
        assert_eq!((resized.rows, resized.columns), (12, 60));

        app.terminate_terminal(&launched.id).unwrap();
        let completed = wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running()
        });
        assert!(matches!(
            completed.process,
            DesktopTerminalProcessState::Exited { success: false, .. }
                | DesktopTerminalProcessState::Failed { .. }
        ));
    }

    #[test]
    fn terminal_workspace_restoration_never_restarts_the_process() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec!["-lc".to_owned(), "exit 0".to_owned()];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 40,
            })
            .unwrap();
        wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running()
        });
        let item = app.terminal_workspace_item(&launched).unwrap();
        let restoration = item.restoration().unwrap();
        app.forget_terminal(&launched.id).unwrap();

        let restored = app.restore_workspace_item(&restoration).unwrap().unwrap();
        let restored_state: DesktopTerminalRestoration =
            serde_json::from_value(restored.serialized_state.unwrap()).unwrap();
        assert_eq!(restored_state.id, launched.id);
        assert_eq!(
            restored_state.snapshot().process,
            DesktopTerminalProcessState::Restored
        );
        assert!(matches!(
            app.terminal_snapshot(&launched.id, 0),
            Err(DesktopApplicationError::Terminal(
                DesktopTerminalError::SessionNotFound(_)
            ))
        ));
    }

    #[test]
    fn terminal_rejects_scopes_without_a_workspace() {
        let root = tempfile::tempdir().unwrap();
        let (app, _) = app_with_project(root.path());
        let project = app
            .create_project(DesktopProjectCreate::new("No workspace"))
            .unwrap();
        assert!(matches!(
            app.launch_terminal(DesktopTerminalLaunch::shell(DesktopTerminalScope::Project(
                project.id
            ))),
            Err(DesktopApplicationError::Terminal(
                DesktopTerminalError::MissingWorkspace
            )) | Err(DesktopApplicationError::ProjectContext(_))
        ));
    }
}
