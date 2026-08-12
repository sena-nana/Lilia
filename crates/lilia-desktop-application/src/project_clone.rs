use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::{DesktopApplication, DesktopSecret};

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopProjectCloneRequest {
    pub repository: String,
    pub parent_directory: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopProjectCloneResult {
    pub workspace_path: PathBuf,
    pub suggested_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopProjectClonePhase {
    Preparing,
    Cloning,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopProjectCloneSnapshot {
    pub sequence: u64,
    pub phase: DesktopProjectClonePhase,
    pub workspace_path: PathBuf,
    pub suggested_name: String,
    pub percent: Option<u8>,
    pub detail: Option<String>,
}

#[derive(Clone)]
pub struct DesktopProjectCloneOperation {
    inner: Arc<CloneOperationInner>,
}

impl DesktopProjectCloneOperation {
    pub fn snapshot(&self) -> DesktopProjectCloneSnapshot {
        let state = lock_operation(&self.inner);
        state.snapshot()
    }

    /// Requests cancellation and terminates the active Git child process.
    ///
    /// Returns `true` only for the first cancellation request accepted before
    /// the operation reached a terminal state. Call [`Self::wait`] to observe
    /// cleanup completion.
    pub fn cancel(&self) -> bool {
        let mut state = lock_operation(&self.inner);
        if state.outcome.is_some() || state.cancel_requested {
            return false;
        }

        state.cancel_requested = true;
        state.phase = DesktopProjectClonePhase::Cancelling;
        state.detail = Some("Cancelling Git clone".to_owned());
        state.sequence = state.sequence.saturating_add(1);
        if let Some(child) = state.child.as_ref() {
            terminate_process_tree(child, state.process_tree.as_ref());
        }
        if let Some(child) = state.child.as_mut() {
            let _ = child.kill();
        }
        self.inner.changed.notify_all();
        true
    }

    pub fn wait(&self) -> Result<DesktopProjectCloneResult, DesktopProjectCloneError> {
        let mut state = lock_operation(&self.inner);
        loop {
            if let Some(outcome) = &state.outcome {
                return outcome.clone();
            }
            state = self
                .inner
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopProjectCloneError {
    #[error("repository must not be empty")]
    EmptyRepository,
    #[error("repository URLs must not contain inline credentials")]
    RepositoryCredentialsUnsupported,
    #[error("clone parent `{0}` is not an existing directory")]
    ParentDirectoryUnavailable(PathBuf),
    #[error("unable to reserve a clone target below `{0}`")]
    TargetUnavailable(PathBuf),
    #[error("unable to start the clone worker")]
    WorkerUnavailable,
    #[error("unable to start Git")]
    GitUnavailable,
    #[error("unable to contain the Git process tree")]
    GitProcessTreeUnavailable,
    #[error("unable to wait for Git")]
    GitWaitFailed,
    #[error("Git clone failed with exit code {status:?}")]
    GitFailed { status: Option<i32> },
    #[error("Git clone was cancelled")]
    Cancelled,
    #[error("unable to clean clone target `{path}`: {message}")]
    CleanupFailed { path: PathBuf, message: String },
}

struct CloneOperationInner {
    state: Mutex<CloneOperationState>,
    changed: Condvar,
}

struct CloneOperationState {
    sequence: u64,
    phase: DesktopProjectClonePhase,
    workspace_path: PathBuf,
    suggested_name: String,
    percent: Option<u8>,
    detail: Option<String>,
    cancel_requested: bool,
    child: Option<Child>,
    process_tree: Option<CloneProcessTree>,
    outcome: Option<Result<DesktopProjectCloneResult, DesktopProjectCloneError>>,
}

impl CloneOperationState {
    fn snapshot(&self) -> DesktopProjectCloneSnapshot {
        DesktopProjectCloneSnapshot {
            sequence: self.sequence,
            phase: self.phase,
            workspace_path: self.workspace_path.clone(),
            suggested_name: self.suggested_name.clone(),
            percent: self.percent,
            detail: self.detail.clone(),
        }
    }
}

struct ReservedCloneTarget {
    path: PathBuf,
    cleanup_required: bool,
}

impl ReservedCloneTarget {
    fn cleanup(&mut self) -> Result<(), DesktopProjectCloneError> {
        if !self.cleanup_required {
            return Ok(());
        }
        match fs::remove_dir_all(&self.path) {
            Ok(()) => {
                self.cleanup_required = false;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.cleanup_required = false;
                Ok(())
            }
            Err(error) => Err(DesktopProjectCloneError::CleanupFailed {
                path: self.path.clone(),
                message: error.to_string(),
            }),
        }
    }

    fn keep(mut self) {
        self.cleanup_required = false;
    }
}

impl Drop for ReservedCloneTarget {
    fn drop(&mut self) {
        if self.cleanup_required {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

impl DesktopApplication {
    pub fn start_project_repository_clone(
        &self,
        request: DesktopProjectCloneRequest,
    ) -> Result<DesktopProjectCloneOperation, DesktopProjectCloneError> {
        self.start_project_repository_clone_with_command(request, |repository, target, parent| {
            let mut command = Command::new("git");
            command
                .arg("clone")
                .arg("--progress")
                .arg("--")
                .arg(repository)
                .arg(target)
                .current_dir(parent)
                .env("GIT_TERMINAL_PROMPT", "0");
            command
        })
    }

    pub fn clone_project_repository(
        &self,
        request: DesktopProjectCloneRequest,
    ) -> Result<DesktopProjectCloneResult, DesktopProjectCloneError> {
        self.start_project_repository_clone(request)?.wait()
    }

    pub(crate) fn start_project_repository_clone_with_github_token(
        &self,
        request: DesktopProjectCloneRequest,
        token: DesktopSecret,
    ) -> Result<DesktopProjectCloneOperation, DesktopProjectCloneError> {
        self.start_project_repository_clone_with_command(
            request,
            move |repository, target, parent| {
                github_clone_command(repository, target, parent, &token)
            },
        )
    }

    fn start_project_repository_clone_with_command<F>(
        &self,
        request: DesktopProjectCloneRequest,
        command_factory: F,
    ) -> Result<DesktopProjectCloneOperation, DesktopProjectCloneError>
    where
        F: FnOnce(&str, &Path, &Path) -> Command + Send + 'static,
    {
        let (repository, parent, suggested_name, reservation) = prepare_clone(request)?;
        let workspace_path = reservation.path.clone();
        let inner = Arc::new(CloneOperationInner {
            state: Mutex::new(CloneOperationState {
                sequence: 0,
                phase: DesktopProjectClonePhase::Preparing,
                workspace_path,
                suggested_name,
                percent: None,
                detail: None,
                cancel_requested: false,
                child: None,
                process_tree: None,
                outcome: None,
            }),
            changed: Condvar::new(),
        });
        let operation = DesktopProjectCloneOperation {
            inner: Arc::clone(&inner),
        };
        std::thread::Builder::new()
            .name("lilia-project-clone".to_owned())
            .spawn(move || {
                run_clone_operation(inner, reservation, repository, parent, command_factory);
            })
            .map_err(|_| DesktopProjectCloneError::WorkerUnavailable)?;
        Ok(operation)
    }
}

fn github_clone_command(
    repository: &str,
    target: &Path,
    parent: &Path,
    token: &DesktopSecret,
) -> Command {
    let authorization = STANDARD.encode([b"x-access-token:".as_slice(), token.expose()].concat());
    let mut command = Command::new("git");
    command
        .arg("clone")
        .arg("--progress")
        .arg("--")
        .arg(repository)
        .arg(target)
        .current_dir(parent)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("AUTHORIZATION: basic {authorization}"),
        );
    command
}

fn prepare_clone(
    request: DesktopProjectCloneRequest,
) -> Result<(String, PathBuf, String, ReservedCloneTarget), DesktopProjectCloneError> {
    let repository = request.repository.trim().to_owned();
    if repository.is_empty() {
        return Err(DesktopProjectCloneError::EmptyRepository);
    }
    if repository_contains_http_credentials(&repository) {
        return Err(DesktopProjectCloneError::RepositoryCredentialsUnsupported);
    }
    let parent = if request.parent_directory.is_absolute() {
        request.parent_directory.clone()
    } else {
        std::env::current_dir()
            .map(|current| current.join(&request.parent_directory))
            .map_err(|_| {
                DesktopProjectCloneError::ParentDirectoryUnavailable(
                    request.parent_directory.clone(),
                )
            })?
    };
    if !parent.is_dir() {
        return Err(DesktopProjectCloneError::ParentDirectoryUnavailable(
            request.parent_directory,
        ));
    }

    let suggested_name = derive_repository_name(&repository);
    let reservation = reserve_unique_target_path(&parent, &suggested_name)?;
    Ok((repository, parent, suggested_name, reservation))
}

fn run_clone_operation<F>(
    inner: Arc<CloneOperationInner>,
    mut reservation: ReservedCloneTarget,
    repository: String,
    parent: PathBuf,
    command_factory: F,
) where
    F: FnOnce(&str, &Path, &Path) -> Command,
{
    if lock_operation(&inner).cancel_requested {
        finish_failed_operation(
            &inner,
            &mut reservation,
            DesktopProjectCloneError::Cancelled,
        );
        return;
    }

    let mut command = command_factory(&repository, &reservation.path, &parent);
    hide_console_window(&mut command);
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            finish_failed_operation(
                &inner,
                &mut reservation,
                DesktopProjectCloneError::GitUnavailable,
            );
            return;
        }
    };
    let process_tree = match CloneProcessTree::attach(&child) {
        Ok(process_tree) => process_tree,
        Err(()) => {
            terminate_process_tree(&child, None);
            let _ = child.kill();
            let _ = child.wait();
            finish_failed_operation(
                &inner,
                &mut reservation,
                DesktopProjectCloneError::GitProcessTreeUnavailable,
            );
            return;
        }
    };
    let progress_reader = child.stderr.take().map(|stderr| {
        let progress_inner = Arc::clone(&inner);
        std::thread::spawn(move || read_git_progress(stderr, &progress_inner))
    });

    {
        let mut state = lock_operation(&inner);
        state.child = Some(child);
        state.process_tree = Some(process_tree);
        if state.cancel_requested {
            state.phase = DesktopProjectClonePhase::Cancelling;
            if let Some(child) = state.child.as_ref() {
                terminate_process_tree(child, state.process_tree.as_ref());
            }
            if let Some(child) = state.child.as_mut() {
                let _ = child.kill();
            }
        } else {
            state.phase = DesktopProjectClonePhase::Cloning;
        }
        state.sequence = state.sequence.saturating_add(1);
        inner.changed.notify_all();
    }

    let status = loop {
        let poll = {
            let mut state = lock_operation(&inner);
            if state.cancel_requested {
                if let Some(child) = state.child.as_ref() {
                    terminate_process_tree(child, state.process_tree.as_ref());
                }
                if let Some(child) = state.child.as_mut() {
                    let _ = child.kill();
                }
            }
            state
                .child
                .as_mut()
                .expect("clone child is installed before polling")
                .try_wait()
        };
        match poll {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => break Err(DesktopProjectCloneError::GitWaitFailed),
        }
    };
    {
        let mut state = lock_operation(&inner);
        state.child.take();
    }
    if let Some(reader) = progress_reader {
        let _ = reader.join();
    }
    lock_operation(&inner).process_tree.take();

    let cancelled = lock_operation(&inner).cancel_requested;
    if cancelled {
        finish_failed_operation(
            &inner,
            &mut reservation,
            DesktopProjectCloneError::Cancelled,
        );
        return;
    }

    match status {
        Ok(status) if status.success() => {
            let result = {
                let state = lock_operation(&inner);
                DesktopProjectCloneResult {
                    workspace_path: state.workspace_path.clone(),
                    suggested_name: state.suggested_name.clone(),
                }
            };
            reservation.keep();
            finish_operation(
                &inner,
                DesktopProjectClonePhase::Completed,
                Some(100),
                Some("Clone completed".to_owned()),
                Ok(result),
            );
        }
        Ok(status) => finish_failed_operation(
            &inner,
            &mut reservation,
            DesktopProjectCloneError::GitFailed {
                status: status.code(),
            },
        ),
        Err(error) => finish_failed_operation(&inner, &mut reservation, error),
    }
}

fn finish_failed_operation(
    inner: &CloneOperationInner,
    reservation: &mut ReservedCloneTarget,
    error: DesktopProjectCloneError,
) {
    let error = match reservation.cleanup() {
        Ok(()) => error,
        Err(cleanup_error) => cleanup_error,
    };
    let phase = if matches!(error, DesktopProjectCloneError::Cancelled) {
        DesktopProjectClonePhase::Cancelled
    } else {
        DesktopProjectClonePhase::Failed
    };
    finish_operation(inner, phase, None, None, Err(error));
}

fn finish_operation(
    inner: &CloneOperationInner,
    phase: DesktopProjectClonePhase,
    percent: Option<u8>,
    detail: Option<String>,
    outcome: Result<DesktopProjectCloneResult, DesktopProjectCloneError>,
) {
    let mut state = lock_operation(inner);
    state.phase = phase;
    state.percent = percent.or(state.percent);
    state.detail = detail.or(state.detail.take());
    state.outcome = Some(outcome);
    state.sequence = state.sequence.saturating_add(1);
    inner.changed.notify_all();
}

fn read_git_progress(stderr: impl std::io::Read, inner: &CloneOperationInner) {
    let mut reader = BufReader::new(stderr);
    let mut fragment = Vec::new();
    loop {
        fragment.clear();
        let bytes = match reader.read_until(b'\r', &mut fragment) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };
        if bytes == 0 {
            return;
        }
        let text = String::from_utf8_lossy(&fragment);
        for line in text.split(['\r', '\n']) {
            if let Some(progress) = parse_git_progress(line) {
                publish_git_progress(inner, progress);
            }
        }
    }
}

struct GitProgress {
    percent: u8,
    detail: String,
}

fn parse_git_progress(line: &str) -> Option<GitProgress> {
    let (label, base, span) = if line.contains("Enumerating objects") {
        ("Enumerating objects", 0_u16, 5_u16)
    } else if line.contains("Counting objects") {
        ("Counting objects", 5, 5)
    } else if line.contains("Compressing objects") {
        ("Compressing objects", 10, 15)
    } else if line.contains("Receiving objects") {
        ("Receiving objects", 25, 65)
    } else if line.contains("Resolving deltas") {
        ("Resolving deltas", 90, 9)
    } else {
        return None;
    };
    let marker = line.find('%')?;
    let digits = line[..marker]
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let stage_percent = digits.parse::<u16>().ok()?.min(100);
    let percent = (base + (stage_percent * span / 100)).min(99) as u8;
    Some(GitProgress {
        percent,
        detail: format!("{label}: {stage_percent}%"),
    })
}

fn publish_git_progress(inner: &CloneOperationInner, progress: GitProgress) {
    let mut state = lock_operation(inner);
    if state.outcome.is_some() || state.phase == DesktopProjectClonePhase::Cancelling {
        return;
    }
    if state
        .percent
        .is_some_and(|current| current > progress.percent)
    {
        return;
    }
    let percent = state
        .percent
        .map_or(progress.percent, |current| current.max(progress.percent));
    if state.percent == Some(percent) && state.detail.as_deref() == Some(&progress.detail) {
        return;
    }
    state.percent = Some(percent);
    state.detail = Some(progress.detail);
    state.sequence = state.sequence.saturating_add(1);
    inner.changed.notify_all();
}

fn lock_operation(inner: &CloneOperationInner) -> MutexGuard<'_, CloneOperationState> {
    inner
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
struct CloneProcessTree {
    handle: usize,
}

#[cfg(windows)]
impl CloneProcessTree {
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
impl Drop for CloneProcessTree {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};

        let _ = unsafe { CloseHandle(HANDLE(self.handle as *mut _)) };
    }
}

#[cfg(not(windows))]
struct CloneProcessTree;

#[cfg(not(windows))]
impl CloneProcessTree {
    fn attach(_child: &Child) -> Result<Self, ()> {
        Ok(Self)
    }
}

#[cfg(windows)]
fn terminate_process_tree(child: &Child, process_tree: Option<&CloneProcessTree>) {
    let mut command = Command::new("taskkill");
    hide_console_window(&mut command);
    let _ = command
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Some(process_tree) = process_tree {
        process_tree.terminate();
    }
}

#[cfg(not(windows))]
fn terminate_process_tree(_child: &Child, _process_tree: Option<&CloneProcessTree>) {}

fn derive_repository_name(repository: &str) -> String {
    let trimmed = repository.trim().trim_end_matches('/');
    let without_suffix = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let candidate = without_suffix
        .rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or_default()
        .trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." {
        "repository".to_owned()
    } else {
        candidate.to_owned()
    }
}

fn repository_contains_http_credentials(repository: &str) -> bool {
    let lower = repository.to_ascii_lowercase();
    let authority = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .and_then(|value| value.split('/').next());
    authority.is_some_and(|authority| authority.contains('@'))
}

fn reserve_unique_target_path(
    parent: &Path,
    suggested_name: &str,
) -> Result<ReservedCloneTarget, DesktopProjectCloneError> {
    for suffix in 1..=10_000 {
        let candidate = if suffix == 1 {
            parent.join(suggested_name)
        } else {
            parent.join(format!("{suggested_name}-{suffix}"))
        };
        match fs::create_dir(&candidate) {
            Ok(()) => {
                return Ok(ReservedCloneTarget {
                    path: candidate,
                    cleanup_required: true,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => {
                return Err(DesktopProjectCloneError::TargetUnavailable(
                    parent.to_owned(),
                ));
            }
        }
    }
    Err(DesktopProjectCloneError::TargetUnavailable(
        parent.to_owned(),
    ))
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    const FAKE_GIT_MODE: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_MODE";
    const FAKE_GIT_TARGET: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_TARGET";
    const FAKE_GIT_MARKER: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_MARKER";

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

    fn application() -> DesktopApplication {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:project-clone:{id}"),
            format!("project-clone-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/project-clone",
                format!("liliacode.project-clone-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    fn run_git(arguments: &[&str]) {
        let status = Command::new("git").args(arguments).status().unwrap();
        assert!(status.success(), "git command failed: {arguments:?}");
    }

    fn create_source_repository(temp: &Path) -> PathBuf {
        let source = temp.join("source.git");
        fs::create_dir_all(&source).unwrap();
        run_git(&["init", source.to_str().unwrap()]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.email",
            "native-test@example.invalid",
        ]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.name",
            "Native Test",
        ]);
        fs::write(source.join("README.md"), "native clone\n").unwrap();
        run_git(&["-C", source.to_str().unwrap(), "add", "README.md"]);
        run_git(&["-C", source.to_str().unwrap(), "commit", "-m", "fixture"]);
        source
    }

    fn fake_git_command(mode: &'static str, target: &Path, marker: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("project_clone::tests::fake_git_child")
            .arg("--nocapture")
            .env(FAKE_GIT_MODE, mode)
            .env(FAKE_GIT_TARGET, target)
            .env(FAKE_GIT_MARKER, marker);
        command
    }

    fn wait_for_path(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(path.exists(), "timed out waiting for {}", path.display());
    }

    fn wait_for_clone_snapshot(
        operation: &DesktopProjectCloneOperation,
        predicate: impl Fn(&DesktopProjectCloneSnapshot) -> bool,
    ) -> DesktopProjectCloneSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = operation.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for clone state; last snapshot: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn fake_git_child() {
        let Ok(mode) = std::env::var(FAKE_GIT_MODE) else {
            return;
        };
        if mode == "descendant" {
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        let target = PathBuf::from(std::env::var_os(FAKE_GIT_TARGET).unwrap());
        let marker = PathBuf::from(std::env::var_os(FAKE_GIT_MARKER).unwrap());
        fs::write(target.join("partial.pack"), "partial clone").unwrap();
        fs::write(&marker, "started").unwrap();
        eprint!("Counting objects: 100% (1/1)\rReceiving objects: 40% (2/5)\r");
        std::io::stderr().flush().unwrap();
        if mode == "fail" {
            panic!("intentional fake Git failure");
        }
        let mut descendant = Command::new(std::env::current_exe().unwrap());
        descendant
            .arg("--exact")
            .arg("project_clone::tests::fake_git_child")
            .arg("--nocapture")
            .env(FAKE_GIT_MODE, "descendant")
            .env(FAKE_GIT_TARGET, &target)
            .env(FAKE_GIT_MARKER, &marker);
        let mut descendant = descendant.spawn().unwrap();
        std::thread::spawn(move || {
            let _ = descendant.wait();
        });
        std::thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn clone_uses_a_unique_target_and_does_not_create_product_state() {
        let temp = tempfile::tempdir().unwrap();
        let source = create_source_repository(temp.path());
        let clones = temp.path().join("clones");
        fs::create_dir_all(&clones).unwrap();
        fs::create_dir_all(clones.join("source")).unwrap();

        let app = application();
        let result = app
            .clone_project_repository(DesktopProjectCloneRequest {
                repository: source.display().to_string(),
                parent_directory: clones,
            })
            .unwrap();
        assert_eq!(result.suggested_name, "source");
        assert_eq!(
            result.workspace_path.file_name().unwrap().to_string_lossy(),
            "source-2"
        );
        assert!(result.workspace_path.join(".git").is_dir());
        assert!(app
            .query_projects(crate::ProjectQuery::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cancellation_kills_git_cleans_only_the_reserved_target_and_allows_retry() {
        let temp = tempfile::tempdir().unwrap();
        let source = create_source_repository(temp.path());
        let clones = temp.path().join("clones");
        let existing = clones.join("source");
        let marker = temp.path().join("slow-git-started");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("keep.txt"), "keep").unwrap();

        let app = application();
        let request = DesktopProjectCloneRequest {
            repository: source.display().to_string(),
            parent_directory: clones.clone(),
        };
        let marker_for_command = marker.clone();
        let operation = app
            .start_project_repository_clone_with_command(request.clone(), move |_, target, _| {
                fake_git_command("slow", target, &marker_for_command)
            })
            .unwrap();
        let initial = operation.snapshot();
        wait_for_path(&marker);
        let running = wait_for_clone_snapshot(&operation, |snapshot| {
            snapshot.phase == DesktopProjectClonePhase::Cloning && snapshot.percent.is_some()
        });
        assert!(running.sequence > initial.sequence);
        assert_eq!(running.phase, DesktopProjectClonePhase::Cloning);
        assert!(running.percent.is_some());
        assert!(operation.cancel());
        let (outcome_tx, outcome_rx) = std::sync::mpsc::channel();
        let wait_operation = operation.clone();
        std::thread::spawn(move || {
            let _ = outcome_tx.send(wait_operation.wait());
        });
        let outcome = outcome_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("cancelled clone must stop its process tree and finish cleanup");
        assert!(matches!(outcome, Err(DesktopProjectCloneError::Cancelled)));
        assert_eq!(
            fs::read_to_string(existing.join("keep.txt")).unwrap(),
            "keep"
        );
        assert!(!clones.join("source-2").exists());
        assert!(app
            .query_projects(crate::ProjectQuery::default())
            .unwrap()
            .is_empty());

        let retry = app.clone_project_repository(request).unwrap();
        assert_eq!(retry.workspace_path, clones.join("source-2"));
        assert!(retry.workspace_path.join(".git").is_dir());
    }

    #[test]
    fn failed_clone_cleans_the_reserved_target_and_allows_immediate_retry() {
        let temp = tempfile::tempdir().unwrap();
        let source = create_source_repository(temp.path());
        let clones = temp.path().join("clones");
        let marker = temp.path().join("failed-git-started");
        fs::create_dir_all(&clones).unwrap();

        let app = application();
        let request = DesktopProjectCloneRequest {
            repository: source.display().to_string(),
            parent_directory: clones.clone(),
        };
        let marker_for_command = marker.clone();
        let operation = app
            .start_project_repository_clone_with_command(request.clone(), move |_, target, _| {
                fake_git_command("fail", target, &marker_for_command)
            })
            .unwrap();
        wait_for_path(&marker);
        assert!(matches!(
            operation.wait(),
            Err(DesktopProjectCloneError::GitFailed { .. })
        ));
        assert!(!clones.join("source").exists());

        let retry = app.clone_project_repository(request).unwrap();
        assert_eq!(retry.workspace_path, clones.join("source"));
        assert!(retry.workspace_path.join(".git").is_dir());
    }

    #[test]
    fn clone_rejects_empty_repositories_and_missing_parents_before_git() {
        let app = application();
        let empty = app
            .clone_project_repository(DesktopProjectCloneRequest {
                repository: "  ".to_owned(),
                parent_directory: PathBuf::from("missing"),
            })
            .unwrap_err();
        assert!(matches!(empty, DesktopProjectCloneError::EmptyRepository));

        let credential = app
            .clone_project_repository(DesktopProjectCloneRequest {
                repository: "https://token@example.invalid/repository.git".to_owned(),
                parent_directory: PathBuf::from("missing"),
            })
            .unwrap_err();
        assert!(matches!(
            credential,
            DesktopProjectCloneError::RepositoryCredentialsUnsupported
        ));

        let missing = app
            .clone_project_repository(DesktopProjectCloneRequest {
                repository: "https://example.invalid/repository.git".to_owned(),
                parent_directory: PathBuf::from("missing"),
            })
            .unwrap_err();
        assert!(matches!(
            missing,
            DesktopProjectCloneError::ParentDirectoryUnavailable(_)
        ));
    }

    #[test]
    fn github_clone_keeps_the_token_out_of_arguments_and_the_repository_url() {
        let token = "github-clone-token-canary";
        let parent = Path::new("C:/native-clone-parent");
        let target = parent.join("repository");
        let command = github_clone_command(
            "https://github.com/native-debug/private-repo.git",
            &target,
            parent,
            &DesktopSecret::new(token.as_bytes().to_vec()),
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments.iter().all(|argument| !argument.contains(token)));
        assert!(arguments
            .iter()
            .any(|argument| argument == "https://github.com/native-debug/private-repo.git"));

        let environment = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            environment.get("GIT_CONFIG_KEY_0"),
            Some(&Some("http.https://github.com/.extraheader".to_owned()))
        );
        assert_eq!(
            environment.get("GIT_CONFIG_VALUE_0"),
            Some(&Some(format!(
                "AUTHORIZATION: basic {}",
                STANDARD.encode(format!("x-access-token:{token}"))
            )))
        );
        assert!(environment
            .values()
            .all(|value| value.as_deref().is_none_or(|value| !value.contains(token))));
    }

    #[test]
    fn parses_git_progress_into_monotonic_overall_percentages() {
        let counting = parse_git_progress("remote: Counting objects: 80% (8/10)").unwrap();
        let receiving = parse_git_progress("Receiving objects: 40% (4/10)").unwrap();
        let resolving = parse_git_progress("Resolving deltas: 50% (2/4)").unwrap();
        assert!(counting.percent < receiving.percent);
        assert!(receiving.percent < resolving.percent);
        assert!(parse_git_progress("Cloning into 'target'...").is_none());
    }
}
