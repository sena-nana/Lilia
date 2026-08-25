//! Repository clone, executed inside one kernel job.
//!
//! The operation owns no thread, no condition variable and no sequence counter:
//! it runs on the task runtime thread, reports percentages through
//! [`JobContext::report`], and stops when [`JobContext::is_cancelled`] flips.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use lilia_kernel::JobContext;
use serde::{Deserialize, Serialize};

const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(15);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneRequest {
    pub repository: String,
    pub parent_directory: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneResult {
    pub workspace_path: PathBuf,
    pub suggested_name: String,
}

/// Payload of [`lilia_kernel::JobState::Running`] while a clone is in flight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneProgress {
    pub percent: u8,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CloneError {
    #[error("repository must not be empty")]
    EmptyRepository,
    #[error("repository URLs must not contain inline credentials")]
    RepositoryCredentialsUnsupported,
    #[error("clone parent `{0}` is not an existing directory")]
    ParentDirectoryUnavailable(PathBuf),
    #[error("unable to reserve a clone target below `{0}`")]
    TargetUnavailable(PathBuf),
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

/// Builds the `git clone` invocation. Injected so tests can substitute a fake
/// Git and so the GitHub path can add an authorization header without putting
/// the token on the command line.
pub type CloneCommandFactory = Box<dyn FnOnce(&str, &Path, &Path) -> Command + Send>;

pub fn clone(request: CloneRequest, context: &JobContext) -> Result<CloneResult, CloneError> {
    clone_with_command(request, context, Box::new(default_clone_command))
}

pub fn clone_with_github_token(
    request: CloneRequest,
    token: Vec<u8>,
    context: &JobContext,
) -> Result<CloneResult, CloneError> {
    clone_with_command(
        request,
        context,
        Box::new(move |repository, target, parent| {
            github_clone_command(repository, target, parent, &token)
        }),
    )
}

pub fn clone_with_command(
    request: CloneRequest,
    context: &JobContext,
    command_factory: CloneCommandFactory,
) -> Result<CloneResult, CloneError> {
    let (repository, parent, suggested_name, mut reservation) = prepare_clone(request)?;
    let outcome = run_clone(
        &repository,
        &parent,
        &suggested_name,
        &reservation.path,
        context,
        command_factory,
    );
    match outcome {
        Ok(result) => {
            reservation.keep();
            Ok(result)
        }
        Err(error) => Err(reservation.cleanup().err().unwrap_or(error)),
    }
}

fn run_clone(
    repository: &str,
    parent: &Path,
    suggested_name: &str,
    target: &Path,
    context: &JobContext,
    command_factory: CloneCommandFactory,
) -> Result<CloneResult, CloneError> {
    if context.is_cancelled() {
        return Err(CloneError::Cancelled);
    }
    report(context, 0, "Preparing clone");

    let mut command = command_factory(repository, target, parent);
    configure_clone_process(&mut command);
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| CloneError::GitUnavailable)?;
    let process_tree = match CloneProcessTree::attach(&child) {
        Ok(process_tree) => process_tree,
        Err(()) => {
            terminate_process_tree(&child, None);
            let _ = child.kill();
            let _ = child.wait();
            return Err(CloneError::GitProcessTreeUnavailable);
        }
    };

    let reported = Arc::new(AtomicU8::new(0));
    let progress_reader = child.stderr.take().map(|stderr| {
        let context = context.clone();
        let reported = Arc::clone(&reported);
        std::thread::Builder::new()
            .name("lilia-clone-progress".to_owned())
            .spawn(move || read_git_progress(stderr, &context, &reported))
    });

    let mut cancelled = false;
    let status = loop {
        if !cancelled && context.is_cancelled() {
            cancelled = true;
            terminate_process_tree(&child, Some(&process_tree));
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(CHILD_POLL_INTERVAL),
            Err(_) => break Err(CloneError::GitWaitFailed),
        }
    };
    if let Some(Ok(reader)) = progress_reader {
        let _ = reader.join();
    }

    if cancelled || context.is_cancelled() {
        return Err(CloneError::Cancelled);
    }
    match status {
        Ok(status) if status.success() => {
            report(context, 100, "Clone completed");
            Ok(CloneResult {
                workspace_path: target.to_owned(),
                suggested_name: suggested_name.to_owned(),
            })
        }
        Ok(status) => Err(CloneError::GitFailed {
            status: status.code(),
        }),
        Err(error) => Err(error),
    }
}

fn default_clone_command(repository: &str, target: &Path, parent: &Path) -> Command {
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
}

fn github_clone_command(repository: &str, target: &Path, parent: &Path, token: &[u8]) -> Command {
    let authorization = STANDARD.encode([b"x-access-token:".as_slice(), token].concat());
    let mut command = default_clone_command(repository, target, parent);
    command
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("AUTHORIZATION: basic {authorization}"),
        );
    command
}

fn report(context: &JobContext, percent: u8, detail: impl Into<String>) {
    let progress = CloneProgress {
        percent,
        detail: detail.into(),
    };
    if let Ok(value) = serde_json::to_value(progress) {
        context.report(value);
    }
}

struct ReservedCloneTarget {
    path: PathBuf,
    cleanup_required: bool,
}

impl ReservedCloneTarget {
    fn cleanup(&mut self) -> Result<(), CloneError> {
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
            Err(error) => Err(CloneError::CleanupFailed {
                path: self.path.clone(),
                message: error.to_string(),
            }),
        }
    }

    fn keep(&mut self) {
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

fn prepare_clone(
    request: CloneRequest,
) -> Result<(String, PathBuf, String, ReservedCloneTarget), CloneError> {
    let repository = request.repository.trim().to_owned();
    if repository.is_empty() {
        return Err(CloneError::EmptyRepository);
    }
    if repository_contains_http_credentials(&repository) {
        return Err(CloneError::RepositoryCredentialsUnsupported);
    }
    let parent = if request.parent_directory.is_absolute() {
        request.parent_directory.clone()
    } else {
        std::env::current_dir()
            .map(|current| current.join(&request.parent_directory))
            .map_err(|_| {
                CloneError::ParentDirectoryUnavailable(request.parent_directory.clone())
            })?
    };
    if !parent.is_dir() {
        return Err(CloneError::ParentDirectoryUnavailable(
            request.parent_directory,
        ));
    }

    let suggested_name = derive_repository_name(&repository);
    let reservation = reserve_unique_target_path(&parent, &suggested_name)?;
    Ok((repository, parent, suggested_name, reservation))
}

fn read_git_progress(stderr: impl std::io::Read, context: &JobContext, reported: &AtomicU8) {
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
            let Some(progress) = parse_git_progress(line) else {
                continue;
            };
            // Git interleaves stages, so only ever move the bar forward.
            if reported.fetch_max(progress.percent, Ordering::Relaxed) > progress.percent {
                continue;
            }
            report(context, progress.percent, progress.detail);
        }
    }
}

fn parse_git_progress(line: &str) -> Option<CloneProgress> {
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
    Some(CloneProgress {
        percent,
        detail: format!("{label}: {stage_percent}%"),
    })
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
struct CloneProcessTree {
    #[cfg(unix)]
    process_group_id: i32,
}

#[cfg(not(windows))]
impl CloneProcessTree {
    fn attach(child: &Child) -> Result<Self, ()> {
        let _ = child;
        Ok(Self {
            #[cfg(unix)]
            process_group_id: child.id().try_into().map_err(|_| ())?,
        })
    }
}

#[cfg(windows)]
fn terminate_process_tree(child: &Child, process_tree: Option<&CloneProcessTree>) {
    let mut command = Command::new("taskkill");
    configure_clone_process(&mut command);
    let _ = command
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if let Some(process_tree) = process_tree {
        process_tree.terminate();
    }
}

#[cfg(unix)]
fn terminate_process_tree(_child: &Child, process_tree: Option<&CloneProcessTree>) {
    if let Some(process_tree) = process_tree {
        unsafe {
            libc::kill(-process_tree.process_group_id, libc::SIGKILL);
        }
    }
}

#[cfg(not(any(windows, unix)))]
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
) -> Result<ReservedCloneTarget, CloneError> {
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
            Err(_) => return Err(CloneError::TargetUnavailable(parent.to_owned())),
        }
    }
    Err(CloneError::TargetUnavailable(parent.to_owned()))
}

#[cfg(windows)]
fn configure_clone_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn configure_clone_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(any(windows, unix)))]
fn configure_clone_process(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::Instant;

    use super::*;

    const FAKE_GIT_MODE: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_MODE";
    const FAKE_GIT_TARGET: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_TARGET";
    const FAKE_GIT_MARKER: &str = "LILIA_PROJECT_CLONE_FAKE_GIT_MARKER";

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
            .arg("clone::tests::fake_git_child")
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
            .arg("clone::tests::fake_git_child")
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
    fn clone_reserves_a_unique_target_beside_the_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = create_source_repository(temp.path());
        let clones = temp.path().join("clones");
        fs::create_dir_all(clones.join("source")).unwrap();

        let result = clone(
            CloneRequest {
                repository: source.display().to_string(),
                parent_directory: clones,
            },
            &JobContext::new(),
        )
        .unwrap();

        assert_eq!(result.suggested_name, "source");
        assert_eq!(
            result.workspace_path.file_name().unwrap().to_string_lossy(),
            "source-2"
        );
        assert!(result.workspace_path.join(".git").is_dir());
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

        let request = CloneRequest {
            repository: source.display().to_string(),
            parent_directory: clones.clone(),
        };
        let context = JobContext::new();
        let (outcome_sender, outcome_receiver) = std::sync::mpsc::channel();
        let clone_context = context.clone();
        let clone_request = request.clone();
        let marker_for_command = marker.clone();
        std::thread::spawn(move || {
            let outcome = clone_with_command(
                clone_request,
                &clone_context,
                Box::new(move |_, target, _| fake_git_command("slow", target, &marker_for_command)),
            );
            let _ = outcome_sender.send(outcome);
        });

        wait_for_path(&marker);
        context.request_cancel();
        let outcome = outcome_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("a cancelled clone stops its process tree and finishes cleanup");
        assert!(matches!(outcome, Err(CloneError::Cancelled)));
        assert_eq!(
            fs::read_to_string(existing.join("keep.txt")).unwrap(),
            "keep"
        );
        assert!(!clones.join("source-2").exists());

        let retry = clone(request, &JobContext::new()).unwrap();
        assert_eq!(retry.workspace_path, clones.join("source-2"));
        assert!(retry.workspace_path.join(".git").is_dir());
    }

    #[test]
    fn a_failed_clone_cleans_the_reserved_target_and_allows_immediate_retry() {
        let temp = tempfile::tempdir().unwrap();
        let source = create_source_repository(temp.path());
        let clones = temp.path().join("clones");
        let marker = temp.path().join("failed-git-started");
        fs::create_dir_all(&clones).unwrap();

        let request = CloneRequest {
            repository: source.display().to_string(),
            parent_directory: clones.clone(),
        };
        let marker_for_command = marker.clone();
        let outcome = clone_with_command(
            request.clone(),
            &JobContext::new(),
            Box::new(move |_, target, _| fake_git_command("fail", target, &marker_for_command)),
        );

        assert!(matches!(outcome, Err(CloneError::GitFailed { .. })));
        assert!(!clones.join("source").exists());

        let retry = clone(request, &JobContext::new()).unwrap();
        assert_eq!(retry.workspace_path, clones.join("source"));
        assert!(retry.workspace_path.join(".git").is_dir());
    }

    #[test]
    fn clone_rejects_empty_repositories_credentials_and_missing_parents_before_git() {
        let context = JobContext::new();
        assert!(matches!(
            clone(
                CloneRequest {
                    repository: "  ".to_owned(),
                    parent_directory: PathBuf::from("missing"),
                },
                &context,
            ),
            Err(CloneError::EmptyRepository)
        ));
        assert!(matches!(
            clone(
                CloneRequest {
                    repository: "https://token@example.invalid/repository.git".to_owned(),
                    parent_directory: PathBuf::from("missing"),
                },
                &context,
            ),
            Err(CloneError::RepositoryCredentialsUnsupported)
        ));
        assert!(matches!(
            clone(
                CloneRequest {
                    repository: "https://example.invalid/repository.git".to_owned(),
                    parent_directory: PathBuf::from("missing"),
                },
                &context,
            ),
            Err(CloneError::ParentDirectoryUnavailable(_))
        ));
    }

    #[test]
    fn github_clone_keeps_the_token_out_of_arguments_and_the_repository_url() {
        let token = "github-clone-token-canary";
        let parent = Path::new("/native-clone-parent");
        let target = parent.join("repository");
        let command = github_clone_command(
            "https://github.com/native-debug/private-repo.git",
            &target,
            parent,
            token.as_bytes(),
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
    fn git_progress_parses_into_monotonic_overall_percentages() {
        let counting = parse_git_progress("remote: Counting objects: 80% (8/10)").unwrap();
        let receiving = parse_git_progress("Receiving objects: 40% (4/10)").unwrap();
        let resolving = parse_git_progress("Resolving deltas: 50% (2/4)").unwrap();
        assert!(counting.percent < receiving.percent);
        assert!(receiving.percent < resolving.percent);
        assert!(parse_git_progress("Cloning into 'target'...").is_none());
    }
}
