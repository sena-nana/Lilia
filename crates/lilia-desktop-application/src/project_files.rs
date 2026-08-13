use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lilia_contracts::{ProjectArchiveState, ProjectId};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::{
    DesktopApplication, DesktopApplicationError, DesktopEventKind, DocumentSnapshot,
    ProjectContext, ProjectContextError,
};

const MAX_DIRECTORY_ENTRIES: usize = 2_000;
const MAX_OPEN_FILE_BYTES: u64 = 2 * 1024 * 1024;
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFileKind {
    File,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: ProjectFileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ProjectFileEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilesViewState {
    #[serde(default)]
    pub expanded_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilesSnapshot {
    pub project_id: ProjectId,
    pub root_name: String,
    pub workspace_root: PathBuf,
    pub revision: u64,
    pub entries: Vec<ProjectFileEntry>,
    pub view: ProjectFilesViewState,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProjectFilesError {
    #[error("path must stay relative to the active project root: `{0}`")]
    PathEscapesWorkspace(String),
    #[error("path `{0}` is not a directory")]
    NotADirectory(String),
    #[error("path `{0}` is not a file")]
    NotAFile(String),
    #[error("file `{0}` exceeds the open size limit")]
    FileTooLarge(String),
    #[error("file `{0}` contains binary content")]
    BinaryFile(String),
    #[error("failed to read `{path}`: {message}")]
    Io { path: String, message: String },
}

impl From<ProjectContextError> for ProjectFilesError {
    fn from(error: ProjectContextError) -> Self {
        match error {
            ProjectContextError::InvalidRelativePath(path) => {
                Self::PathEscapesWorkspace(path.display().to_string())
            }
            other => Self::Io {
                path: String::new(),
                message: other.to_string(),
            },
        }
    }
}

impl DesktopApplication {
    pub fn list_project_directory(
        &self,
        project_id: &ProjectId,
        relative_dir: &str,
    ) -> Result<Vec<ProjectFileEntry>, DesktopApplicationError> {
        let context = self.project_context(project_id)?;
        Ok(list_directory(&context, relative_dir)?)
    }

    pub fn project_files_snapshot(
        &self,
        project_id: &ProjectId,
        view: ProjectFilesViewState,
    ) -> Result<ProjectFilesSnapshot, DesktopApplicationError> {
        let project = self.get_project(project_id)?;
        if project.archive == ProjectArchiveState::Archived {
            return Err(DesktopApplicationError::InvalidInput {
                field: "projectId",
                message: format!("project `{}` is archived", project_id.as_str()),
            });
        }
        let context = ProjectContext::from_project(&project)?;
        let view = sanitize_view_state(&context, view)?;
        let revision = self.project_files_revision(project_id);
        let entries = build_tree(&context, &view.expanded_paths)?;
        Ok(ProjectFilesSnapshot {
            project_id: project_id.clone(),
            root_name: project.name,
            workspace_root: context.active_root().to_path_buf(),
            revision,
            entries,
            view,
        })
    }

    pub fn open_project_file(
        &self,
        project_id: &ProjectId,
        relative_path: &str,
    ) -> Result<DocumentSnapshot, DesktopApplicationError> {
        let context = self.project_context(project_id)?;
        let relative = normalize_relative_path(relative_path)?;
        let absolute = context.resolve_relative(&relative)?;
        let metadata = fs::metadata(&absolute).map_err(|error| ProjectFilesError::Io {
            path: relative_path.to_owned(),
            message: error.to_string(),
        })?;
        if !metadata.is_file() {
            return Err(ProjectFilesError::NotAFile(relative_path.to_owned()).into());
        }
        if metadata.len() > MAX_OPEN_FILE_BYTES {
            return Err(ProjectFilesError::FileTooLarge(relative_path.to_owned()).into());
        }
        let bytes = fs::read(&absolute).map_err(|error| ProjectFilesError::Io {
            path: relative_path.to_owned(),
            message: error.to_string(),
        })?;
        if bytes.contains(&0) {
            return Err(ProjectFilesError::BinaryFile(relative_path.to_owned()).into());
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| ProjectFilesError::BinaryFile(relative_path.to_owned()))?;
        let absolute = fs::canonicalize(&absolute).map_err(|error| ProjectFilesError::Io {
            path: relative_path.to_owned(),
            message: error.to_string(),
        })?;
        let (snapshot, _) = self.open_document(absolute, text, None, false)?;
        self.ensure_project_files_watcher(project_id)?;
        Ok(snapshot)
    }

    pub fn ensure_project_files_watcher(
        &self,
        project_id: &ProjectId,
    ) -> Result<(), DesktopApplicationError> {
        let context = self.project_context(project_id)?;
        let root = context.active_root().to_path_buf();
        if !root.is_dir() {
            return Err(ProjectFilesError::NotADirectory(root.display().to_string()).into());
        }
        let mut watchers = self
            .inner
            .project_files_watchers
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("project files watchers"))?;
        if let Some(existing) = watchers.get(project_id.as_str()) {
            if existing.root == root && !existing.stop_flag.load(Ordering::SeqCst) {
                return Ok(());
            }
            existing.stop();
        }
        let watcher = ProjectFilesWatcher::start(
            self.clone(),
            project_id.clone(),
            root.clone(),
            self.inner.project_files_revisions.clone(),
        )?;
        watchers.insert(project_id.as_str().to_owned(), watcher);
        Ok(())
    }

    pub fn stop_project_files_watcher(&self, project_id: &ProjectId) {
        if let Ok(mut watchers) = self.inner.project_files_watchers.lock() {
            if let Some(watcher) = watchers.remove(project_id.as_str()) {
                watcher.stop();
            }
        }
    }

    pub fn project_files_view_state_from_value(
        value: Option<&serde_json::Value>,
    ) -> ProjectFilesViewState {
        value
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    pub fn project_files_view_state_value(state: &ProjectFilesViewState) -> serde_json::Value {
        serde_json::to_value(state).unwrap_or_else(|_| serde_json::json!({}))
    }

    fn project_files_revision(&self, project_id: &ProjectId) -> u64 {
        self.inner
            .project_files_revisions
            .lock()
            .ok()
            .and_then(|revisions| {
                revisions
                    .get(project_id.as_str())
                    .map(|revision| revision.load(Ordering::SeqCst))
            })
            .unwrap_or(0)
    }
}

pub(crate) struct ProjectFilesWatcher {
    root: PathBuf,
    stop_flag: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl ProjectFilesWatcher {
    fn start(
        application: DesktopApplication,
        project_id: ProjectId,
        root: PathBuf,
        revisions: Arc<Mutex<BTreeMap<String, AtomicU64>>>,
    ) -> Result<Self, DesktopApplicationError> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = tx.send(result);
        })
        .map_err(|error| ProjectFilesError::Io {
            path: root.display().to_string(),
            message: error.to_string(),
        })?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| ProjectFilesError::Io {
                path: root.display().to_string(),
                message: error.to_string(),
            })?;

        let stop_flag_thread = Arc::clone(&stop_flag);
        let root_thread = root.clone();
        let join = thread::Builder::new()
            .name(format!("lilia-project-files-{}", project_id.as_str()))
            .spawn(move || {
                let mut pending = false;
                let mut last_fire = Instant::now()
                    .checked_sub(WATCHER_DEBOUNCE)
                    .unwrap_or_else(Instant::now);
                while !stop_flag_thread.load(Ordering::SeqCst) {
                    match rx.recv_timeout(WATCHER_DEBOUNCE) {
                        Ok(Ok(event)) => {
                            if event_is_relevant(&event.kind) {
                                pending = true;
                            }
                        }
                        Ok(Err(_)) => pending = true,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if pending && last_fire.elapsed() >= WATCHER_DEBOUNCE {
                        pending = false;
                        last_fire = Instant::now();
                        bump_revision(&revisions, project_id.as_str());
                        application.emit_event(DesktopEventKind::ProjectFilesChanged {
                            project_id: project_id.clone(),
                        });
                    }
                }
                let _ = root_thread;
            })
            .map_err(|error| ProjectFilesError::Io {
                path: root.display().to_string(),
                message: error.to_string(),
            })?;

        Ok(Self {
            root,
            stop_flag,
            _watcher: watcher,
            join: Mutex::new(Some(join)),
        })
    }

    fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Ok(mut join) = self.join.lock() {
            if let Some(handle) = join.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for ProjectFilesWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn bump_revision(revisions: &Arc<Mutex<BTreeMap<String, AtomicU64>>>, project_id: &str) {
    if let Ok(mut guard) = revisions.lock() {
        guard
            .entry(project_id.to_owned())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
    }
}

fn event_is_relevant(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn sanitize_view_state(
    context: &ProjectContext,
    mut view: ProjectFilesViewState,
) -> Result<ProjectFilesViewState, ProjectFilesError> {
    let mut expanded = BTreeSet::new();
    for path in view.expanded_paths.drain(..) {
        let normalized = match normalize_relative_path(&path) {
            Ok(path) if path.as_os_str().is_empty() => continue,
            Ok(path) => path,
            Err(_) => continue,
        };
        if context.resolve_relative(&normalized).is_ok() {
            expanded.insert(relative_path_text(&normalized));
        }
    }
    view.expanded_paths = expanded.into_iter().collect();
    if let Some(selected) = view.selected_path.take() {
        if let Ok(normalized) = normalize_relative_path(&selected) {
            if !normalized.as_os_str().is_empty() && context.resolve_relative(&normalized).is_ok() {
                view.selected_path = Some(relative_path_text(&normalized));
            }
        }
    }
    Ok(view)
}

fn build_tree(
    context: &ProjectContext,
    expanded_paths: &[String],
) -> Result<Vec<ProjectFileEntry>, ProjectFilesError> {
    let expanded = expanded_paths.iter().cloned().collect::<BTreeSet<_>>();
    list_directory_recursive(context, "", &expanded)
}

fn list_directory_recursive(
    context: &ProjectContext,
    relative_dir: &str,
    expanded: &BTreeSet<String>,
) -> Result<Vec<ProjectFileEntry>, ProjectFilesError> {
    let mut entries = list_directory(context, relative_dir)?;
    for entry in &mut entries {
        if entry.kind != ProjectFileKind::Directory {
            continue;
        }
        if expanded.contains(&entry.relative_path) {
            entry.children = Some(list_directory_recursive(
                context,
                &entry.relative_path,
                expanded,
            )?);
        }
    }
    Ok(entries)
}

fn list_directory(
    context: &ProjectContext,
    relative_dir: &str,
) -> Result<Vec<ProjectFileEntry>, ProjectFilesError> {
    let relative = normalize_relative_path(relative_dir)?;
    let directory = if relative.as_os_str().is_empty() {
        context.active_root().to_path_buf()
    } else {
        context.resolve_relative(&relative)?
    };
    if !directory.is_dir() {
        return Err(ProjectFilesError::NotADirectory(relative_path_text(
            &relative,
        )));
    }
    let mut children = fs::read_dir(&directory)
        .map_err(|error| ProjectFilesError::Io {
            path: relative_path_text(&relative),
            message: error.to_string(),
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| !should_skip_project_files_path(path))
        .collect::<Vec<_>>();
    children.sort_by(|left, right| {
        let left_dir = left.is_dir();
        let right_dir = right.is_dir();
        right_dir.cmp(&left_dir).then_with(|| {
            left.file_name()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .cmp(&right.file_name().unwrap_or_default().to_ascii_lowercase())
        })
    });
    let mut entries = Vec::new();
    for path in children.into_iter().take(MAX_DIRECTORY_ENTRIES) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        if name.is_empty() {
            continue;
        }
        let child_relative = if relative.as_os_str().is_empty() {
            PathBuf::from(&name)
        } else {
            relative.join(&name)
        };
        let kind = if path.is_dir() {
            ProjectFileKind::Directory
        } else {
            ProjectFileKind::File
        };
        entries.push(ProjectFileEntry {
            name,
            relative_path: relative_path_text(&child_relative),
            kind,
            children: None,
        });
    }
    Ok(entries)
}

fn should_skip_project_files_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if name.starts_with('.') {
        return true;
    }
    if matches!(
        name.as_str(),
        "node_modules" | "dist" | "target" | "build" | "cache"
    ) {
        return true;
    }
    name == "cache"
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|parent| parent.to_str())
            .is_some_and(|parent| parent.eq_ignore_ascii_case(".yarn"))
}

fn normalize_relative_path(value: &str) -> Result<PathBuf, ProjectFilesError> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() || normalized == "." {
        return Ok(PathBuf::new());
    }
    let mut path = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(ProjectFilesError::PathEscapesWorkspace(value.to_owned()));
            }
        }
    }
    Ok(path)
}

fn relative_path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate, ProjectWorkspaceSurface,
        WorkspaceItemRestoration,
    };
    use lilia_service::ServiceAuthority;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::Arc;

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

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn app_with_project(root: &Path) -> (DesktopApplication, ProjectId) {
        let id = NEXT_ID.fetch_add(1, AtomicOrdering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:project-files:{id}"),
            format!("project-files-test:{id}"),
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                format!("/tmp/lilia-project-files-{id}"),
                format!("liliacode.project-files-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let project = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(root.display().to_string()),
                ..DesktopProjectCreate::new("Files")
            })
            .unwrap();
        (app, project.id)
    }

    #[test]
    fn listing_rejects_parent_escape_and_skips_hidden_or_excluded_dirs() {
        let root = tempfile::tempdir().unwrap();
        write_file(&root.path().join("src/main.rs"), "fn main() {}");
        write_file(&root.path().join(".env"), "secret");
        write_file(&root.path().join("node_modules/pkg/index.js"), "ignore");
        write_file(&root.path().join("target/debug/app"), "ignore");
        let (app, project_id) = app_with_project(root.path());

        assert!(matches!(
            app.list_project_directory(&project_id, "../secret"),
            Err(DesktopApplicationError::ProjectFiles(
                ProjectFilesError::PathEscapesWorkspace(_)
            ))
        ));
        let entries = app.list_project_directory(&project_id, "").unwrap();
        let names = entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["src"]);
        let nested = app.list_project_directory(&project_id, "src").unwrap();
        assert_eq!(
            nested
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.rs"]
        );
    }

    #[test]
    fn snapshot_expands_selected_directories_and_opens_documents() {
        let root = tempfile::tempdir().unwrap();
        write_file(&root.path().join("src/lib.rs"), "pub fn ok() {}");
        write_file(&root.path().join("README.md"), "# hi");
        let (app, project_id) = app_with_project(root.path());

        let snapshot = app
            .project_files_snapshot(
                &project_id,
                ProjectFilesViewState {
                    expanded_paths: vec!["src".to_owned()],
                    selected_path: Some("src/lib.rs".to_owned()),
                },
            )
            .unwrap();
        assert_eq!(snapshot.view.selected_path.as_deref(), Some("src/lib.rs"));
        let src = snapshot
            .entries
            .iter()
            .find(|entry| entry.relative_path == "src")
            .unwrap();
        assert!(src.children.as_ref().unwrap().iter().any(|entry| {
            entry.relative_path == "src/lib.rs" && entry.kind == ProjectFileKind::File
        }));

        let document = app.open_project_file(&project_id, "src/lib.rs").unwrap();
        assert!(document.canonical_path.ends_with("src/lib.rs"));
        assert_eq!(document.buffer.text, "pub fn ok() {}");
        assert!(matches!(
            app.open_project_file(&project_id, "../outside.rs"),
            Err(DesktopApplicationError::ProjectFiles(
                ProjectFilesError::PathEscapesWorkspace(_)
            ))
        ));
    }

    #[test]
    fn watcher_invalidates_revision_and_item_restores_without_product_facts() {
        let root = tempfile::tempdir().unwrap();
        write_file(&root.path().join("a.txt"), "one");
        let (app, project_id) = app_with_project(root.path());
        let before = app
            .project_files_snapshot(&project_id, ProjectFilesViewState::default())
            .unwrap()
            .revision;
        let events = app.subscribe_events();
        app.ensure_project_files_watcher(&project_id).unwrap();
        write_file(&root.path().join("b.txt"), "two");

        let mut observed = false;
        for _ in 0..50 {
            if let Ok(event) = events.recv_timeout(Duration::from_millis(100)) {
                if matches!(
                    event.kind,
                    DesktopEventKind::ProjectFilesChanged {
                        project_id: ref changed
                    } if changed == &project_id
                ) {
                    observed = true;
                    break;
                }
            }
        }
        assert!(observed, "watcher must publish ProjectFilesChanged");
        let after = app
            .project_files_snapshot(&project_id, ProjectFilesViewState::default())
            .unwrap();
        assert!(after.revision > before);
        assert!(after
            .entries
            .iter()
            .any(|entry| entry.relative_path == "b.txt"));

        let item = app
            .project_workspace_item(&project_id, ProjectWorkspaceSurface::Files)
            .unwrap()
            .with_serialized_state(Some(DesktopApplication::project_files_view_state_value(
                &ProjectFilesViewState {
                    expanded_paths: vec![],
                    selected_path: Some("b.txt".to_owned()),
                },
            )));
        assert_eq!(item.kind.as_str(), crate::PROJECT_FILES_WORKSPACE_ITEM_KIND);
        let restored = app
            .restore_workspace_item(&WorkspaceItemRestoration {
                id: item.id.clone(),
                resource_id: Some(item.resource_id.clone()),
                kind: item.kind.clone(),
                serialized_state: item.serialized_state.clone(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(restored.title, "Files · 文件");
        assert_eq!(
            restored.serialized_state, item.serialized_state,
            "UI state must round-trip; product title comes from Product Core"
        );
    }
}
