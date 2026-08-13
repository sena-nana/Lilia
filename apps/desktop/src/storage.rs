use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lilia_desktop_application::{
    DesktopWorkspaceSessionState, MemorySettings, MemorySettingsStore, MemoryStoreError,
};
use nana_ui::{AppearanceSettings, HostedWindowPlacement, ThemeMode};
use serde::{Deserialize, Serialize};

const HOME_ENV: &str = "LILIA_HOME";
const THEME_FILE: &str = "appearance.theme";
const APPEARANCE_FILE: &str = "appearance.settings.json";
const SIDEBAR_DISPLAY_MODE_FILE: &str = "sidebar-display-mode.json";
const SIDEBAR_TREE_STATE_FILE: &str = "sidebar-tree-state.json";
const MEMORY_SETTINGS_FILE: &str = "memory.settings.json";
const WINDOW_STATE_FILE: &str = "main-window-state.json";
const CONVERSATION_STATUS_STATE_FILE: &str = "conversation-status-window.json";
const WORKSPACE_STATE_FILE: &str = "main-workspace-state.json";
const WORKSPACE_TOPOLOGY_STATE_FILE: &str = "workspace-topology-state.json";
const LEGACY_WORKSPACE_WINDOWS_STATE_FILE: &str = "workspace-windows-state.json";
pub const LILIA_INSTANCE_IDENTITY: &str = "liliacode";
const MIN_WINDOW_WIDTH: u32 = 780;
const MIN_WINDOW_HEIGHT: u32 = 560;
const MIN_AUXILIARY_WINDOW_WIDTH: u32 = 320;
const MIN_AUXILIARY_WINDOW_HEIGHT: u32 = 420;
const MIN_CONVERSATION_STATUS_WINDOW_WIDTH: u32 = 260;
const MIN_CONVERSATION_STATUS_WINDOW_HEIGHT: u32 = 320;
const WINDOW_STATE_DEBOUNCE: Duration = Duration::from_millis(180);

pub const NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NativeSidebarDisplayMode {
    #[default]
    Grouped,
    Unified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeSidebarTreeState {
    #[serde(default)]
    pub expanded_project_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub inbox_expanded: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
    #[serde(default)]
    pub sidebar_collapsed: bool,
}

impl Default for NativeSidebarTreeState {
    fn default() -> Self {
        Self {
            expanded_project_ids: Vec::new(),
            inbox_expanded: true,
            sidebar_width: default_sidebar_width(),
            sidebar_collapsed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeWindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_window_scale_factor")]
    pub scale_factor: f32,
    pub maximized: bool,
}

impl NativeWindowState {
    pub fn hosted_placement(self) -> HostedWindowPlacement {
        HostedWindowPlacement::new(
            self.x,
            self.y,
            self.width,
            self.height,
            self.scale_factor,
            self.maximized,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeWindowSnapshot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeConversationStatusWindowState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<NativeWindowState>,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "default_conversation_status_opacity")]
    pub opacity: f32,
}

impl Default for NativeConversationStatusWindowState {
    fn default() -> Self {
        Self {
            geometry: None,
            always_on_top: false,
            opacity: default_conversation_status_opacity(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeWorkspaceWindowState {
    pub window_id: u64,
    pub session_id: String,
    pub workspace: DesktopWorkspaceSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<NativeWindowState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeWorkspaceTopologyState {
    pub schema_version: u32,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_workspace: Option<DesktopWorkspaceSessionState>,
    pub windows: Vec<NativeWorkspaceWindowState>,
}

impl Default for NativeWorkspaceTopologyState {
    fn default() -> Self {
        Self {
            schema_version: NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION,
            revision: 0,
            primary_workspace: None,
            windows: Vec::new(),
        }
    }
}

impl From<NativeWindowSnapshot> for NativeWindowState {
    fn from(snapshot: NativeWindowSnapshot) -> Self {
        Self {
            x: snapshot.x,
            y: snapshot.y,
            width: snapshot.width,
            height: snapshot.height,
            scale_factor: snapshot.scale_factor,
            maximized: snapshot.maximized,
        }
    }
}

enum WindowStateMessage {
    Persist(NativeWindowState),
    Shutdown,
}

pub struct NativeWindowStateWriter {
    current: Option<NativeWindowState>,
    sender: Sender<WindowStateMessage>,
    worker: Option<JoinHandle<()>>,
}

enum ConversationStatusStateMessage {
    Persist(NativeConversationStatusWindowState),
    Shutdown,
}

pub struct NativeConversationStatusStateWriter {
    sender: Sender<ConversationStatusStateMessage>,
    worker: Option<JoinHandle<()>>,
}

enum WorkspaceTopologyStateMessage {
    Persist(Box<NativeWorkspaceTopologyState>),
    Shutdown,
}

pub struct NativeWorkspaceTopologyStateWriter {
    sender: Sender<WorkspaceTopologyStateMessage>,
    worker: Option<JoinHandle<()>>,
    #[cfg(any(debug_assertions, test))]
    committed_revision: Arc<AtomicU64>,
    #[cfg(any(debug_assertions, test))]
    committed_primary_revision: Arc<AtomicU64>,
}

pub fn lilia_home() -> Result<PathBuf, String> {
    let paths = lilia_storage::LiliaDataPaths::resolve();
    paths
        .ensure_layout()
        .map_err(|error| format!("failed to prepare {HOME_ENV}: {error}"))?;
    Ok(paths.home().to_path_buf())
}

pub fn load_theme(home: &Path) -> ThemeMode {
    match fs::read_to_string(home.join(THEME_FILE)) {
        Ok(value) if value.trim().eq_ignore_ascii_case("light") => ThemeMode::Light,
        _ => ThemeMode::Dark,
    }
}

pub fn save_theme(home: &Path, theme: ThemeMode) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| format!("failed to create Lilia home: {error}"))?;
    let value = match theme {
        ThemeMode::Dark => "dark",
        ThemeMode::Light => "light",
    };
    fs::write(home.join(THEME_FILE), value)
        .map_err(|error| format!("failed to persist desktop theme: {error}"))
}

pub fn load_appearance(home: &Path) -> AppearanceSettings {
    let mut appearance = AppearanceSettings::default();
    if let Ok(value) = fs::read_to_string(home.join(APPEARANCE_FILE)) {
        let _ = appearance.restore_json(&value);
    }
    appearance
}

pub fn save_appearance(home: &Path, appearance: &AppearanceSettings) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| format!("failed to create Lilia home: {error}"))?;
    let value = appearance
        .to_json()
        .map_err(|error| format!("failed to serialize desktop appearance: {error}"))?;
    fs::write(home.join(APPEARANCE_FILE), value)
        .map_err(|error| format!("failed to persist desktop appearance: {error}"))
}

pub fn load_sidebar_display_mode(home: &Path) -> NativeSidebarDisplayMode {
    fs::read_to_string(home.join(SIDEBAR_DISPLAY_MODE_FILE))
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub fn save_sidebar_display_mode(
    home: &Path,
    mode: NativeSidebarDisplayMode,
) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| format!("failed to create Lilia home: {error}"))?;
    let value = serde_json::to_vec(&mode)
        .map_err(|error| format!("failed to serialize Native sidebar display mode: {error}"))?;
    fs::write(home.join(SIDEBAR_DISPLAY_MODE_FILE), value)
        .map_err(|error| format!("failed to persist Native sidebar display mode: {error}"))
}

pub fn load_sidebar_tree_state(home: &Path) -> NativeSidebarTreeState {
    fs::read_to_string(home.join(SIDEBAR_TREE_STATE_FILE))
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

pub fn save_sidebar_tree_state(home: &Path, state: &NativeSidebarTreeState) -> Result<(), String> {
    fs::create_dir_all(home).map_err(|error| format!("failed to create Lilia home: {error}"))?;
    let value = serde_json::to_vec(state)
        .map_err(|error| format!("failed to serialize Native sidebar tree state: {error}"))?;
    fs::write(home.join(SIDEBAR_TREE_STATE_FILE), value)
        .map_err(|error| format!("failed to persist Native sidebar tree state: {error}"))
}

const fn default_true() -> bool {
    true
}

const fn default_sidebar_width() -> u16 {
    220
}

pub fn is_restorable_window_state(state: &NativeWindowState) -> bool {
    state.width >= MIN_WINDOW_WIDTH && state.height >= MIN_WINDOW_HEIGHT
}

pub fn is_restorable_auxiliary_window_state(state: &NativeWindowState) -> bool {
    state.width >= MIN_AUXILIARY_WINDOW_WIDTH && state.height >= MIN_AUXILIARY_WINDOW_HEIGHT
}

pub fn is_restorable_conversation_status_window_state(state: &NativeWindowState) -> bool {
    state.width >= MIN_CONVERSATION_STATUS_WINDOW_WIDTH
        && state.height >= MIN_CONVERSATION_STATUS_WINDOW_HEIGHT
}

pub fn merge_window_state(
    previous: Option<NativeWindowState>,
    snapshot: NativeWindowSnapshot,
) -> NativeWindowState {
    merge_window_state_with(previous, snapshot, is_restorable_window_state)
}

pub fn merge_auxiliary_window_state(
    previous: Option<NativeWindowState>,
    snapshot: NativeWindowSnapshot,
) -> NativeWindowState {
    merge_window_state_with(previous, snapshot, is_restorable_auxiliary_window_state)
}

pub fn merge_conversation_status_window_state(
    previous: Option<NativeWindowState>,
    snapshot: NativeWindowSnapshot,
) -> NativeWindowState {
    merge_window_state_with(
        previous,
        snapshot,
        is_restorable_conversation_status_window_state,
    )
}

fn merge_window_state_with(
    previous: Option<NativeWindowState>,
    snapshot: NativeWindowSnapshot,
    is_restorable: fn(&NativeWindowState) -> bool,
) -> NativeWindowState {
    if snapshot.maximized {
        if let Some(previous) = previous.filter(is_restorable) {
            return NativeWindowState {
                maximized: true,
                ..previous
            };
        }
    }
    snapshot.into()
}

pub fn load_window_state(home: &Path) -> Option<NativeWindowState> {
    [
        home.join(WINDOW_STATE_FILE),
        home.join(format!("{WINDOW_STATE_FILE}.bak")),
    ]
    .into_iter()
    .find_map(|path| {
        fs::read(&path)
            .ok()
            .and_then(|content| serde_json::from_slice::<NativeWindowState>(&content).ok())
            .filter(is_restorable_window_state)
    })
}

pub fn load_conversation_status_state(home: &Path) -> NativeConversationStatusWindowState {
    [
        home.join(CONVERSATION_STATUS_STATE_FILE),
        home.join(format!("{CONVERSATION_STATUS_STATE_FILE}.bak")),
    ]
    .into_iter()
    .find_map(|path| {
        fs::read(&path)
            .ok()
            .and_then(|content| serde_json::from_slice(&content).ok())
    })
    .map(normalize_conversation_status_state)
    .unwrap_or_default()
}

fn normalize_conversation_status_state(
    mut state: NativeConversationStatusWindowState,
) -> NativeConversationStatusWindowState {
    state.geometry = state
        .geometry
        .filter(is_restorable_conversation_status_window_state);
    state.opacity = normalize_conversation_status_opacity(state.opacity);
    state
}

fn default_window_scale_factor() -> f32 {
    1.0
}

fn default_conversation_status_opacity() -> f32 {
    0.96
}

pub fn normalize_conversation_status_opacity(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.4, 1.0)
    } else {
        default_conversation_status_opacity()
    }
}

impl NativeWindowStateWriter {
    pub fn start(
        home: impl Into<PathBuf>,
        initial: Option<NativeWindowState>,
    ) -> Result<Self, String> {
        let home = home.into();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("lilia-native-window-state".to_owned())
            .spawn(move || run_window_state_writer(home, receiver))
            .map_err(|error| format!("failed to start Native window-state writer: {error}"))?;
        Ok(Self {
            current: initial,
            sender,
            worker: Some(worker),
        })
    }

    pub fn record(&mut self, snapshot: NativeWindowSnapshot) -> Result<(), String> {
        let state = merge_window_state(self.current, snapshot);
        self.current = Some(state);
        self.sender
            .send(WindowStateMessage::Persist(state))
            .map_err(|_| "Native window-state writer is unavailable".to_owned())
    }
}

impl Drop for NativeWindowStateWriter {
    fn drop(&mut self) {
        let _ = self.sender.send(WindowStateMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl NativeConversationStatusStateWriter {
    pub fn start(home: impl Into<PathBuf>) -> Result<Self, String> {
        let home = home.into();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("lilia-native-conversation-status-state".to_owned())
            .spawn(move || run_conversation_status_state_writer(home, receiver))
            .map_err(|error| {
                format!("failed to start Native conversation-status state writer: {error}")
            })?;
        Ok(Self {
            sender,
            worker: Some(worker),
        })
    }

    pub fn record(&self, state: NativeConversationStatusWindowState) -> Result<(), String> {
        self.sender
            .send(ConversationStatusStateMessage::Persist(state))
            .map_err(|_| "Native conversation-status state writer is unavailable".to_owned())
    }
}

impl Drop for NativeConversationStatusStateWriter {
    fn drop(&mut self) {
        let _ = self.sender.send(ConversationStatusStateMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_conversation_status_state_writer(
    home: PathBuf,
    receiver: Receiver<ConversationStatusStateMessage>,
) {
    while let Ok(message) = receiver.recv() {
        let ConversationStatusStateMessage::Persist(mut pending) = message else {
            break;
        };
        let mut shutdown = false;
        loop {
            match receiver.recv_timeout(WINDOW_STATE_DEBOUNCE) {
                Ok(ConversationStatusStateMessage::Persist(state)) => pending = state,
                Ok(ConversationStatusStateMessage::Shutdown) => {
                    shutdown = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
        }
        if let Err(error) = persist_conversation_status_state(&home, pending) {
            eprintln!("[native-conversation-status-state] {error}");
        }
        if shutdown {
            break;
        }
    }
}

fn persist_conversation_status_state(
    home: &Path,
    state: NativeConversationStatusWindowState,
) -> Result<(), String> {
    persist_json_with_backup(home, CONVERSATION_STATUS_STATE_FILE, &state)
}

fn persist_json_with_backup(
    home: &Path,
    file_name: &str,
    value: &impl Serialize,
) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|error| format!("failed to create LiliaCode home: {error}"))?;
    let path = home.join(file_name);
    let staging = home.join(format!("{file_name}.tmp"));
    let backup = home.join(format!("{file_name}.bak"));
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize Native state: {error}"))?;
    fs::write(&staging, content)
        .map_err(|error| format!("failed to stage Native state: {error}"))?;
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("failed to remove stale Native state backup: {error}"))?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("failed to back up Native state: {error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!("failed to publish Native state: {error}"));
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("failed to remove Native state backup: {error}"))?;
    }
    Ok(())
}

fn run_window_state_writer(home: PathBuf, receiver: Receiver<WindowStateMessage>) {
    while let Ok(message) = receiver.recv() {
        let WindowStateMessage::Persist(mut pending) = message else {
            break;
        };
        let mut shutdown = false;
        loop {
            match receiver.recv_timeout(WINDOW_STATE_DEBOUNCE) {
                Ok(WindowStateMessage::Persist(state)) => pending = state,
                Ok(WindowStateMessage::Shutdown) => {
                    shutdown = true;
                    break;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
        }
        if let Err(error) = persist_window_state(&home, pending) {
            eprintln!("[native-window-state] {error}");
        }
        if shutdown {
            break;
        }
    }
}

fn persist_window_state(home: &Path, state: NativeWindowState) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|error| format!("failed to create LiliaCode home: {error}"))?;
    let path = home.join(WINDOW_STATE_FILE);
    let staging = home.join(format!("{WINDOW_STATE_FILE}.tmp"));
    let backup = home.join(format!("{WINDOW_STATE_FILE}.bak"));
    let content = serde_json::to_vec_pretty(&state)
        .map_err(|error| format!("failed to serialize Native window state: {error}"))?;
    fs::write(&staging, content)
        .map_err(|error| format!("failed to stage Native window state: {error}"))?;

    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| {
            format!("failed to remove stale Native window-state backup: {error}")
        })?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("failed to back up Native window state: {error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!("failed to publish Native window state: {error}"));
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("failed to remove Native window-state backup: {error}"))?;
    }
    Ok(())
}

pub fn load_workspace_state(home: &Path) -> Option<DesktopWorkspaceSessionState> {
    [
        home.join(WORKSPACE_STATE_FILE),
        home.join(format!("{WORKSPACE_STATE_FILE}.bak")),
    ]
    .into_iter()
    .find_map(|path| {
        fs::read(&path)
            .ok()
            .and_then(|content| serde_json::from_slice(&content).ok())
    })
}

pub fn load_workspace_topology_state(home: &Path) -> Option<NativeWorkspaceTopologyState> {
    [
        home.join(WORKSPACE_TOPOLOGY_STATE_FILE),
        home.join(format!("{WORKSPACE_TOPOLOGY_STATE_FILE}.bak")),
    ]
    .into_iter()
    .find_map(|path| {
        fs::read(&path)
            .ok()
            .and_then(|content| serde_json::from_slice(&content).ok())
            .filter(|state: &NativeWorkspaceTopologyState| {
                matches!(
                    state.schema_version,
                    2 | NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION
                )
            })
            .map(normalize_workspace_topology_state)
    })
    .or_else(|| load_legacy_workspace_topology_state(home))
}

fn load_legacy_workspace_topology_state(home: &Path) -> Option<NativeWorkspaceTopologyState> {
    [
        home.join(LEGACY_WORKSPACE_WINDOWS_STATE_FILE),
        home.join(format!("{LEGACY_WORKSPACE_WINDOWS_STATE_FILE}.bak")),
    ]
    .into_iter()
    .find_map(|path| {
        fs::read(&path)
            .ok()
            .and_then(|content| serde_json::from_slice(&content).ok())
            .filter(|state: &NativeWorkspaceTopologyState| state.schema_version == 1)
            .map(|mut state| {
                state.primary_workspace = load_workspace_state(home);
                normalize_workspace_topology_state(state)
            })
    })
}

fn normalize_workspace_topology_state(
    mut state: NativeWorkspaceTopologyState,
) -> NativeWorkspaceTopologyState {
    state.schema_version = NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION;
    for window in &mut state.windows {
        window.geometry = window.geometry.filter(is_restorable_auxiliary_window_state);
    }
    state
}

impl NativeWorkspaceTopologyStateWriter {
    pub fn start(
        home: impl Into<PathBuf>,
        committed_revision: u64,
        committed_primary_revision: u64,
    ) -> Result<Self, String> {
        let home = home.into();
        let (sender, receiver) = mpsc::channel();
        let committed_revision = Arc::new(AtomicU64::new(committed_revision));
        let worker_revision = Arc::clone(&committed_revision);
        let committed_primary_revision = Arc::new(AtomicU64::new(committed_primary_revision));
        let worker_primary_revision = Arc::clone(&committed_primary_revision);
        let worker = thread::Builder::new()
            .name("lilia-native-workspace-topology-state".to_owned())
            .spawn(move || {
                run_workspace_topology_state_writer(
                    home,
                    receiver,
                    worker_revision,
                    worker_primary_revision,
                )
            })
            .map_err(|error| {
                format!("failed to start Native workspace topology writer: {error}")
            })?;
        Ok(Self {
            sender,
            worker: Some(worker),
            #[cfg(any(debug_assertions, test))]
            committed_revision,
            #[cfg(any(debug_assertions, test))]
            committed_primary_revision,
        })
    }

    pub fn record(&self, state: NativeWorkspaceTopologyState) -> Result<(), String> {
        self.sender
            .send(WorkspaceTopologyStateMessage::Persist(Box::new(state)))
            .map_err(|_| "Native workspace topology writer is unavailable".to_owned())
    }

    #[cfg(any(debug_assertions, test))]
    pub fn committed_revision(&self) -> u64 {
        self.committed_revision.load(Ordering::Acquire)
    }

    #[cfg(any(debug_assertions, test))]
    pub fn committed_primary_revision(&self) -> u64 {
        self.committed_primary_revision.load(Ordering::Acquire)
    }
}

impl Drop for NativeWorkspaceTopologyStateWriter {
    fn drop(&mut self) {
        let _ = self.sender.send(WorkspaceTopologyStateMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
fn persist_workspace_state(
    home: &Path,
    state: &DesktopWorkspaceSessionState,
) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|error| format!("failed to create LiliaCode home: {error}"))?;
    let path = home.join(WORKSPACE_STATE_FILE);
    let staging = home.join(format!("{WORKSPACE_STATE_FILE}.tmp"));
    let backup = home.join(format!("{WORKSPACE_STATE_FILE}.bak"));
    let content = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("failed to serialize Native workspace state: {error}"))?;
    fs::write(&staging, content)
        .map_err(|error| format!("failed to stage Native workspace state: {error}"))?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| {
            format!("failed to remove stale Native workspace-state backup: {error}")
        })?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("failed to back up Native workspace state: {error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!("failed to publish Native workspace state: {error}"));
    }
    if backup.exists() {
        fs::remove_file(&backup)
            .map_err(|error| format!("failed to remove Native workspace-state backup: {error}"))?;
    }
    Ok(())
}

fn run_workspace_topology_state_writer(
    home: PathBuf,
    receiver: Receiver<WorkspaceTopologyStateMessage>,
    committed_revision: Arc<AtomicU64>,
    committed_primary_revision: Arc<AtomicU64>,
) {
    while let Ok(message) = receiver.recv() {
        let WorkspaceTopologyStateMessage::Persist(mut pending) = message else {
            break;
        };
        let mut shutdown = false;
        loop {
            match receiver.try_recv() {
                Ok(WorkspaceTopologyStateMessage::Persist(state)) => pending = state,
                Ok(WorkspaceTopologyStateMessage::Shutdown) => {
                    shutdown = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
        }
        match persist_workspace_topology_state(&home, &pending) {
            Ok(()) => {
                committed_primary_revision.store(
                    pending
                        .primary_workspace
                        .as_ref()
                        .map(|workspace| workspace.revision)
                        .unwrap_or_default(),
                    Ordering::Release,
                );
                committed_revision.store(pending.revision, Ordering::Release);
            }
            Err(error) => eprintln!("[native-workspace-topology-state] {error}"),
        }
        if shutdown {
            break;
        }
    }
}

fn persist_workspace_topology_state(
    home: &Path,
    state: &NativeWorkspaceTopologyState,
) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|error| format!("failed to create LiliaCode home: {error}"))?;
    let path = home.join(WORKSPACE_TOPOLOGY_STATE_FILE);
    let staging = home.join(format!("{WORKSPACE_TOPOLOGY_STATE_FILE}.tmp"));
    let backup = home.join(format!("{WORKSPACE_TOPOLOGY_STATE_FILE}.bak"));
    let content = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("failed to serialize Native workspace topology: {error}"))?;
    fs::write(&staging, content)
        .map_err(|error| format!("failed to stage Native workspace topology: {error}"))?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| {
            format!("failed to remove stale Native workspace topology backup: {error}")
        })?;
    }
    if path.exists() {
        fs::rename(&path, &backup)
            .map_err(|error| format!("failed to back up Native workspace topology: {error}"))?;
    }
    if let Err(error) = fs::rename(&staging, &path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, &path);
        }
        return Err(format!(
            "failed to publish Native workspace topology: {error}"
        ));
    }
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| {
            format!("failed to remove Native workspace topology backup: {error}")
        })?;
    }
    Ok(())
}

pub struct NativeMemorySettingsStore {
    home: PathBuf,
}

impl NativeMemorySettingsStore {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    fn settings_path(&self) -> PathBuf {
        self.home.join(MEMORY_SETTINGS_FILE)
    }

    fn backup_path(&self) -> PathBuf {
        self.home.join(format!("{MEMORY_SETTINGS_FILE}.bak"))
    }

    fn staging_path(&self) -> PathBuf {
        self.home.join(format!("{MEMORY_SETTINGS_FILE}.tmp"))
    }
}

impl MemorySettingsStore for NativeMemorySettingsStore {
    fn load(&self) -> Result<Option<MemorySettings>, MemoryStoreError> {
        let path = self.settings_path();
        let fallback = self.backup_path();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::read_to_string(&fallback) {
                    Ok(content) => content,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(error) => {
                        return Err(MemoryStoreError::SettingsStorage {
                            operation: "read Native Memory settings backup",
                            message: error.to_string(),
                        });
                    }
                }
            }
            Err(error) => {
                return Err(MemoryStoreError::SettingsStorage {
                    operation: "read Native Memory settings",
                    message: error.to_string(),
                });
            }
        };
        serde_json::from_str(&content).map(Some).map_err(|error| {
            MemoryStoreError::CorruptSettings {
                message: error.to_string(),
            }
        })
    }

    fn save(&mut self, settings: &MemorySettings) -> Result<(), MemoryStoreError> {
        fs::create_dir_all(&self.home).map_err(|error| MemoryStoreError::SettingsStorage {
            operation: "create Native Memory settings directory",
            message: error.to_string(),
        })?;
        let content = serde_json::to_vec_pretty(settings).map_err(|error| {
            MemoryStoreError::SettingsStorage {
                operation: "serialize Native Memory settings",
                message: error.to_string(),
            }
        })?;
        let path = self.settings_path();
        let backup = self.backup_path();
        let staging = self.staging_path();
        fs::write(&staging, content).map_err(|error| MemoryStoreError::SettingsStorage {
            operation: "stage Native Memory settings",
            message: error.to_string(),
        })?;

        if path.exists() {
            if backup.exists() {
                fs::remove_file(&backup).map_err(|error| MemoryStoreError::SettingsStorage {
                    operation: "remove stale Native Memory settings backup",
                    message: error.to_string(),
                })?;
            }
            fs::rename(&path, &backup).map_err(|error| MemoryStoreError::SettingsStorage {
                operation: "backup Native Memory settings",
                message: error.to_string(),
            })?;
        }
        if let Err(error) = fs::rename(&staging, &path) {
            if backup.exists() && !path.exists() {
                let _ = fs::rename(&backup, &path);
            }
            return Err(MemoryStoreError::SettingsStorage {
                operation: "publish Native Memory settings",
                message: error.to_string(),
            });
        }
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| MemoryStoreError::SettingsStorage {
                operation: "remove Native Memory settings backup",
                message: error.to_string(),
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_display_mode_round_trips_and_invalid_values_fall_back_to_grouped() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-sidebar-mode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        assert_eq!(
            load_sidebar_display_mode(&directory),
            NativeSidebarDisplayMode::Grouped
        );
        save_sidebar_display_mode(&directory, NativeSidebarDisplayMode::Unified).unwrap();
        assert_eq!(
            load_sidebar_display_mode(&directory),
            NativeSidebarDisplayMode::Unified
        );
        fs::write(directory.join(SIDEBAR_DISPLAY_MODE_FILE), b"\"unknown\"").unwrap();
        assert_eq!(
            load_sidebar_display_mode(&directory),
            NativeSidebarDisplayMode::Grouped
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn sidebar_tree_expansion_round_trips_and_legacy_state_keeps_inbox_visible() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-sidebar-tree-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        assert_eq!(
            load_sidebar_tree_state(&directory),
            NativeSidebarTreeState::default()
        );
        let state = NativeSidebarTreeState {
            expanded_project_ids: vec!["project-a".to_owned(), "project-b".to_owned()],
            inbox_expanded: false,
            sidebar_width: 312,
            sidebar_collapsed: true,
        };
        save_sidebar_tree_state(&directory, &state).unwrap();
        assert_eq!(load_sidebar_tree_state(&directory), state);

        fs::write(
            directory.join(SIDEBAR_TREE_STATE_FILE),
            br#"{"expandedProjectIds":["legacy-project"]}"#,
        )
        .unwrap();
        assert_eq!(
            load_sidebar_tree_state(&directory),
            NativeSidebarTreeState {
                expanded_project_ids: vec!["legacy-project".to_owned()],
                inbox_expanded: true,
                sidebar_width: 220,
                sidebar_collapsed: false,
            }
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn conversation_status_preferences_and_geometry_round_trip() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-conversation-status-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = NativeConversationStatusWindowState {
            geometry: Some(NativeWindowState {
                x: 120,
                y: 80,
                width: 360,
                height: 520,
                scale_factor: 1.25,
                maximized: false,
            }),
            always_on_top: true,
            opacity: 0.72,
        };
        {
            let writer = NativeConversationStatusStateWriter::start(&directory).unwrap();
            writer.record(state).unwrap();
        }

        assert_eq!(load_conversation_status_state(&directory), state);
        assert_eq!(normalize_conversation_status_opacity(0.1), 0.4);
        assert_eq!(normalize_conversation_status_opacity(2.0), 1.0);

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn native_memory_settings_round_trip_and_recover_from_backup() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-memory-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut store = NativeMemorySettingsStore::new(&directory);
        let settings = MemorySettings {
            enabled: false,
            baseline_injection_enabled: true,
            cooldown_turns: 8,
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap(), Some(settings.clone()));

        fs::rename(store.settings_path(), store.backup_path()).unwrap();
        assert_eq!(store.load().unwrap(), Some(settings));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn maximized_window_snapshot_preserves_last_normal_geometry() {
        let previous = NativeWindowState {
            x: 120,
            y: 80,
            width: 1180,
            height: 760,
            scale_factor: 1.5,
            maximized: false,
        };
        let merged = merge_window_state(
            Some(previous),
            NativeWindowSnapshot {
                x: -8,
                y: -8,
                width: 1936,
                height: 1056,
                scale_factor: 2.0,
                maximized: true,
            },
        );

        assert_eq!(
            merged,
            NativeWindowState {
                maximized: true,
                ..previous
            }
        );
    }

    #[test]
    fn legacy_window_state_restores_with_a_safe_scale_factor() {
        let state: NativeWindowState = serde_json::from_str(
            r#"{"x":-1200,"y":40,"width":1180,"height":760,"maximized":false}"#,
        )
        .unwrap();

        assert_eq!(state.scale_factor, 1.0);
        assert_eq!(state.hosted_placement().scale_factor, 1.0);
    }

    #[test]
    fn window_state_writer_coalesces_and_flushes_on_drop() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-window-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let final_state = NativeWindowState {
            x: 64,
            y: 48,
            width: 1360,
            height: 820,
            scale_factor: 1.5,
            maximized: false,
        };
        {
            let mut writer = NativeWindowStateWriter::start(&directory, None).unwrap();
            writer
                .record(NativeWindowSnapshot {
                    x: 10,
                    y: 10,
                    width: 1180,
                    height: 760,
                    scale_factor: 1.0,
                    maximized: false,
                })
                .unwrap();
            writer
                .record(NativeWindowSnapshot {
                    x: final_state.x,
                    y: final_state.y,
                    width: final_state.width,
                    height: final_state.height,
                    scale_factor: final_state.scale_factor,
                    maximized: final_state.maximized,
                })
                .unwrap();
        }

        assert_eq!(load_window_state(&directory), Some(final_state));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_workspace_state_loads_and_recovers_from_backup() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-workspace-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = DesktopWorkspaceSessionState {
            revision: 4,
            ..DesktopWorkspaceSessionState::default()
        };
        persist_workspace_state(&directory, &state).unwrap();
        assert_eq!(load_workspace_state(&directory), Some(state.clone()));

        let path = directory.join(WORKSPACE_STATE_FILE);
        let backup = directory.join(format!("{WORKSPACE_STATE_FILE}.bak"));
        fs::rename(&path, &backup).unwrap();
        assert_eq!(load_workspace_state(&directory), Some(state));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn workspace_topology_round_trips_primary_and_windows_and_recovers_from_backup() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-workspace-topology-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let state = NativeWorkspaceTopologyState {
            schema_version: NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION,
            revision: 3,
            primary_workspace: Some(DesktopWorkspaceSessionState {
                revision: 7,
                ..DesktopWorkspaceSessionState::default()
            }),
            windows: vec![NativeWorkspaceWindowState {
                window_id: 100,
                session_id: "lilia.popup.task.one.100".to_owned(),
                workspace: DesktopWorkspaceSessionState {
                    revision: 2,
                    ..DesktopWorkspaceSessionState::default()
                },
                geometry: Some(NativeWindowState {
                    x: 48,
                    y: 64,
                    width: 430,
                    height: 760,
                    scale_factor: 1.25,
                    maximized: false,
                }),
            }],
        };
        {
            let writer = NativeWorkspaceTopologyStateWriter::start(&directory, 0, 0).unwrap();
            writer.record(state.clone()).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while writer.committed_revision() != state.revision
                && std::time::Instant::now() < deadline
            {
                thread::yield_now();
            }
            assert_eq!(writer.committed_revision(), state.revision);
            assert_eq!(writer.committed_primary_revision(), 7);
            assert_eq!(
                load_workspace_topology_state(&directory),
                Some(state.clone())
            );
        }

        let path = directory.join(WORKSPACE_TOPOLOGY_STATE_FILE);
        let backup = directory.join(format!("{WORKSPACE_TOPOLOGY_STATE_FILE}.bak"));
        fs::rename(&path, &backup).unwrap();
        assert_eq!(load_workspace_topology_state(&directory), Some(state));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_workspace_files_migrate_into_one_topology_snapshot() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-workspace-topology-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let primary_workspace = DesktopWorkspaceSessionState {
            revision: 9,
            ..DesktopWorkspaceSessionState::default()
        };
        persist_workspace_state(&directory, &primary_workspace).unwrap();
        let legacy = NativeWorkspaceTopologyState {
            schema_version: 1,
            revision: 4,
            primary_workspace: None,
            windows: Vec::new(),
        };
        fs::write(
            directory.join(LEGACY_WORKSPACE_WINDOWS_STATE_FILE),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let migrated = load_workspace_topology_state(&directory).unwrap();
        assert_eq!(
            migrated.schema_version,
            NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION
        );
        assert_eq!(migrated.revision, 4);
        assert_eq!(migrated.primary_workspace, Some(primary_workspace));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn schema_two_task_window_descriptor_migrates_to_generic_workspace_window() {
        let directory = std::env::temp_dir().join(format!(
            "lilia-native-workspace-topology-v2-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut legacy = serde_json::to_value(NativeWorkspaceTopologyState {
            schema_version: 2,
            revision: 8,
            primary_workspace: None,
            windows: vec![NativeWorkspaceWindowState {
                window_id: 100,
                session_id: "lilia.popup.task.one.100".to_owned(),
                workspace: DesktopWorkspaceSessionState {
                    revision: 2,
                    ..DesktopWorkspaceSessionState::default()
                },
                geometry: None,
            }],
        })
        .unwrap();
        let window = legacy["windows"][0].as_object_mut().unwrap();
        window.insert("taskId".to_owned(), serde_json::json!("one"));
        window.insert(
            "workspaceItemId".to_owned(),
            serde_json::json!("task-popup-view:100:one"),
        );
        fs::write(
            directory.join(WORKSPACE_TOPOLOGY_STATE_FILE),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let migrated = load_workspace_topology_state(&directory).unwrap();
        assert_eq!(
            migrated.schema_version,
            NATIVE_WORKSPACE_TOPOLOGY_SCHEMA_VERSION
        );
        assert_eq!(migrated.revision, 8);
        assert_eq!(migrated.windows.len(), 1);
        let encoded = serde_json::to_value(migrated).unwrap();
        assert!(encoded["windows"][0].get("taskId").is_none());
        assert!(encoded["windows"][0].get("workspaceItemId").is_none());
        fs::remove_dir_all(directory).unwrap();
    }
}
