//! Watch AgentKit registry documents for external edits.
//!
//! Local Skills/Hooks/MCP/Plugin mutations already emit DesktopEvents from the
//! application services. External editors rewriting JSON on disk must still
//! invalidate Extensions surfaces without inventing a second registry authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecursiveMode, Watcher};

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopEvent, DesktopEventKind, ProjectQuery,
};

pub const REGISTRY_WATCH_SOURCE: &str = "registry-file-watch";
const DEBOUNCE: Duration = Duration::from_millis(150);
const WATCHER_RETRY_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RegistryWatchKind {
    Hooks,
    Skills,
    Mcp,
    Plugins,
}

pub(crate) struct RegistryFileWatch {
    stop: AtomicBool,
    running: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct RegistryWatchRunGuard(DesktopApplication);

impl Drop for RegistryWatchRunGuard {
    fn drop(&mut self) {
        self.0
            .inner
            .registry_file_watch
            .running
            .store(false, Ordering::SeqCst);
    }
}

impl Default for RegistryFileWatch {
    fn default() -> Self {
        Self {
            stop: AtomicBool::new(false),
            running: AtomicBool::new(false),
            thread: Mutex::new(None),
        }
    }
}

impl DesktopApplication {
    pub fn start_registry_file_watch(&self) -> Result<(), DesktopApplicationError> {
        if self.inner.authority.data_paths().is_none() {
            return Ok(());
        }
        let watch = &self.inner.registry_file_watch;
        if watch
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        watch.stop.store(false, Ordering::SeqCst);
        let application = self.clone();
        let handle = match thread::Builder::new()
            .name("lilia-registry-file-watch".to_owned())
            .spawn(move || registry_watch_loop(application))
        {
            Ok(handle) => handle,
            Err(error) => {
                watch.running.store(false, Ordering::SeqCst);
                return Err(DesktopApplicationError::InvalidInput {
                    field: "registry_file_watch",
                    message: format!("failed to start registry file watch: {error}"),
                });
            }
        };
        *watch
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
        Ok(())
    }

    pub fn stop_registry_file_watch(&self) {
        let watch = &self.inner.registry_file_watch;
        watch.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = watch
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = handle.join();
        }
        watch.running.store(false, Ordering::SeqCst);
    }

    /// Recomputes watched paths and publishes coalesced invalidation events for
    /// the supplied filesystem paths. Used by tests and the background watcher.
    pub fn publish_registry_path_changes(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<DesktopEvent>, DesktopApplicationError> {
        let watched = self.registry_watch_targets()?;
        let mut kinds = BTreeSet::new();
        for path in paths {
            for target in &watched {
                if paths_refer_to_same_file(path, &target.path) {
                    kinds.insert(target.kind);
                }
            }
        }
        let mut published = Vec::new();
        for kind in kinds {
            published.push(self.inner.events.publish(
                REGISTRY_WATCH_SOURCE,
                match kind {
                    RegistryWatchKind::Hooks => DesktopEventKind::HooksRegistryChanged,
                    RegistryWatchKind::Skills => DesktopEventKind::SkillsRegistryChanged,
                    RegistryWatchKind::Mcp => DesktopEventKind::McpRegistryChanged,
                    RegistryWatchKind::Plugins => DesktopEventKind::PluginsRegistryChanged,
                },
            ));
        }
        Ok(published)
    }

    fn registry_watch_targets(&self) -> Result<Vec<RegistryWatchTarget>, DesktopApplicationError> {
        let paths = self.config().data_paths();
        let mut targets = vec![
            RegistryWatchTarget {
                path: lilia_storage::user_hooks_document_path(&paths),
                kind: RegistryWatchKind::Hooks,
            },
            RegistryWatchTarget {
                path: lilia_storage::skills_registry_path(&paths),
                kind: RegistryWatchKind::Skills,
            },
            RegistryWatchTarget {
                path: lilia_storage::mcp_registry_path(&paths),
                kind: RegistryWatchKind::Mcp,
            },
            RegistryWatchTarget {
                path: lilia_storage::plugins_registry_path(&paths),
                kind: RegistryWatchKind::Plugins,
            },
        ];
        for project in self.query_projects(ProjectQuery::default())? {
            let Some(workspace) = project
                .workspace_path
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            targets.push(RegistryWatchTarget {
                path: PathBuf::from(workspace).join(".lilia").join("hooks.json"),
                kind: RegistryWatchKind::Hooks,
            });
        }
        Ok(targets)
    }
}

#[derive(Clone, Debug)]
struct RegistryWatchTarget {
    path: PathBuf,
    kind: RegistryWatchKind,
}

fn registry_watch_loop(application: DesktopApplication) {
    let _running = RegistryWatchRunGuard(application.clone());
    let (sender, receiver) = std::sync::mpsc::channel();
    let callback_sender = sender.clone();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = callback_sender.send(result);
    })
    .map_err(|error| eprintln!("[registry-file-watch] failed to create watcher: {error}"))
    .ok();

    let mut watched_dirs = BTreeSet::new();
    let mut failed_dirs = BTreeSet::new();
    let mut target_stamps = BTreeMap::new();
    let mut pending = BTreeSet::new();
    let mut last_change = Instant::now();
    let mut last_watcher_attempt = Instant::now();

    loop {
        if application
            .inner
            .registry_file_watch
            .stop
            .load(Ordering::SeqCst)
        {
            break;
        }

        if watcher.is_none() && last_watcher_attempt.elapsed() >= WATCHER_RETRY_INTERVAL {
            last_watcher_attempt = Instant::now();
            let callback_sender = sender.clone();
            watcher = notify::recommended_watcher(move |result| {
                let _ = callback_sender.send(result);
            })
            .map_err(|error| eprintln!("[registry-file-watch] retry failed: {error}"))
            .ok();
        }

        if let Ok(targets) = application.registry_watch_targets() {
            for path in scan_registry_target_changes(&targets, &mut target_stamps) {
                pending.insert(path);
                last_change = Instant::now();
            }
            if let Some(watcher) = watcher.as_mut() {
                for target in targets {
                    let Some(parent) = target.path.parent() else {
                        continue;
                    };
                    if !watched_dirs.contains(parent) {
                        if let Err(error) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                            if failed_dirs.insert(parent.to_path_buf()) {
                                eprintln!(
                                    "[registry-file-watch] failed to watch {}: {error}",
                                    parent.display()
                                );
                            }
                        } else {
                            failed_dirs.remove(parent);
                            watched_dirs.insert(parent.to_path_buf());
                        }
                    }
                }
            }
        }

        match receiver.recv_timeout(DEBOUNCE) {
            Ok(Ok(event)) => {
                if !matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    continue;
                }
                for path in event.paths {
                    pending.insert(path);
                }
                last_change = Instant::now();
            }
            Ok(Err(error)) => {
                eprintln!("[registry-file-watch] {error}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() && last_change.elapsed() >= DEBOUNCE {
                    let paths = pending.iter().cloned().collect::<Vec<_>>();
                    pending.clear();
                    if let Err(error) = application.publish_registry_path_changes(&paths) {
                        eprintln!("[registry-file-watch] publish failed: {error}");
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegistryFileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn registry_file_stamp(path: &Path) -> Option<RegistryFileStamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(RegistryFileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn scan_registry_target_changes(
    targets: &[RegistryWatchTarget],
    stamps: &mut BTreeMap<PathBuf, Option<RegistryFileStamp>>,
) -> Vec<PathBuf> {
    let live_paths = targets
        .iter()
        .map(|target| target.path.clone())
        .collect::<BTreeSet<_>>();
    stamps.retain(|path, _| live_paths.contains(path));
    let mut changed = Vec::new();
    for target in targets {
        let current = registry_file_stamp(&target.path);
        if stamps
            .insert(target.path.clone(), current)
            .is_some_and(|previous| previous != current)
        {
            changed.push(target.path.clone());
        }
    }
    changed
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => {
            let left = left.to_string_lossy().replace('\\', "/");
            let right = right.to_string_lossy().replace('\\', "/");
            left.eq_ignore_ascii_case(&right)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    #[derive(Default)]
    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    #[test]
    fn external_hooks_document_edit_publishes_hooks_registry_changed() {
        let dir = TempDir::new().unwrap();
        let config =
            DesktopApplicationConfig::new(dir.path(), format!("registry-{}", Uuid::new_v4()))
                .unwrap();
        let app = DesktopApplication::bootstrap(config, Arc::new(TestHost)).unwrap();
        let hooks_path = lilia_storage::user_hooks_document_path(&app.config().data_paths());
        if let Some(parent) = hooks_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&hooks_path, r#"{"revision":1,"handlers":[]}"#).unwrap();

        let published = app.publish_registry_path_changes(&[hooks_path]).unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].source_instance, REGISTRY_WATCH_SOURCE);
        assert!(matches!(
            published[0].kind,
            DesktopEventKind::HooksRegistryChanged
        ));
    }

    #[test]
    fn polling_fallback_detects_a_registry_created_after_its_parent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("late").join("hooks.json");
        let targets = vec![RegistryWatchTarget {
            path: path.clone(),
            kind: RegistryWatchKind::Hooks,
        }];
        let mut stamps = BTreeMap::new();

        assert!(scan_registry_target_changes(&targets, &mut stamps).is_empty());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"revision":1,"handlers":[]}"#).unwrap();

        assert_eq!(
            scan_registry_target_changes(&targets, &mut stamps),
            vec![path]
        );
        assert!(scan_registry_target_changes(&targets, &mut stamps).is_empty());
    }

    #[test]
    fn registry_watch_can_stop_and_restart() {
        let dir = TempDir::new().unwrap();
        let config =
            DesktopApplicationConfig::new(dir.path(), format!("registry-{}", Uuid::new_v4()))
                .unwrap();
        let app = DesktopApplication::bootstrap(config, Arc::new(TestHost)).unwrap();

        app.start_registry_file_watch().unwrap();
        assert!(app.inner.registry_file_watch.running.load(Ordering::SeqCst));
        app.stop_registry_file_watch();
        assert!(!app.inner.registry_file_watch.running.load(Ordering::SeqCst));

        app.start_registry_file_watch().unwrap();
        app.stop_registry_file_watch();
    }
}
