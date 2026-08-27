use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use lilia_contracts::TaskId;
use nana_ui::runtime::{
    AboutMetadata, AboutSection, ActionMenu, ActionMenuItem, Activate, AlignSpec, AppContext,
    AppearanceSection, Button, CommandPalette, ConfirmDialog, ConfirmIntent, ConfirmSlots,
    ContextMenu, ContextMenuEvent, ContextMenuItem, DesktopShell, DocumentId, EmptyState, Entity,
    FormField, FrameworkError, GraphCanvas, HighlightRequest, IconButton, IconGlyph, ImageViewer,
    ImageViewerContent, ImageViewerEvent, InteractiveCard, JustifySpec, KeyCaptureLayer,
    LengthSpec, List, ListItem, NativeMarkdown, NodeStyle, OverlayHost, PaneChrome,
    PaneChromeAction, PaneChromeActionKind, PopoverToggled, ScrollAxes, ScrollOffset, ScrollView,
    SemanticColorRole, SettingsBack, SettingsCard, SettingsPage, SettingsRow, SettingsSidebar,
    SettingsTabSelected, SidebarFooter, SidebarFooterButton, SidebarFrame, SidebarRow,
    SidebarRowIcon, SidebarRowState, SidebarSection, SidebarSectionState, StableNodeId, Switch,
    TabOption, Tabs, TabsEvent, Text, TextAlignSpec, TextArea, TextChanged, TimeSeriesChart,
    ToggleChanged, TreeView, TreeViewEvent, View,
};
use nana_ui::{
    AppearanceEvent, AppearanceSettings, ButtonKind, CommandPaletteEvent, CommandPaletteItem,
    ControlSize, Icon, SettingsModel, SettingsState, SettingsTabId, ThemeMode, WindowChrome,
    WindowChromeAction, WindowChromeEvent, WorkspaceModel, UI_METRICS,
};

use crate::runtime_compat::{HostedUiCommand, HostedWindowId};
use crate::runtime_layout::{
    composer_interrupt_button, composer_send_button, flatten_composer_textarea, pill_button,
    reconcile_children, sidebar_icon_button, window_control, HostStack,
};
use crate::target_ids;

const PRIMARY_DOCUMENT: u64 = 1;
const SESSIONS_EMPTY_TEXT: &str = "还没有会话";
const INBOX_EMPTY_TEXT: &str = "没有未绑定的对话";
const PROJECTS_EMPTY_TEXT: &str = "暂无项目";
const PLUS_SLOT_SIZE: f32 = UI_METRICS.icon_button_size;
const COMPOSER_MIN_HEIGHT: f32 = UI_METRICS.control_height;
const COMPOSER_MAX_HEIGHT: f32 = 72.0;
const CHAT_CONTENT_MAX_WIDTH: f32 = 860.0;
const TITLE_BREADCRUMB_WIDTH: f32 = 440.0;
const EMPTY_HEADING_FONT_SIZE: f32 = 24.0;

pub(crate) fn content_hash(value: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellTaskRow {
    pub id: TaskId,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellSidebarKind {
    Header,
    DropHint,
    Running,
    Project,
    Task,
    Inbox,
    Reveal,
    Empty,
    Archived,
    SearchProject,
    SearchTask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellSidebarRow {
    pub id: String,
    pub label: String,
    pub kind: ShellSidebarKind,
    pub selected: bool,
    pub ancestor: bool,
    pub depth: u16,
    pub expanded: Option<bool>,
    pub icon: Icon,
    pub can_stop: bool,
    pub can_menu: bool,
    pub can_draft: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellConfirmKind {
    ArchiveConversations,
    RemoveProject,
    Update,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellConfirm {
    pub kind: ShellConfirmKind,
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub danger: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellTodoRow {
    pub id: String,
    pub label: String,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellPendingKind {
    PermissionApproval,
    PlanApproval,
    AskUser,
    ToolConsent,
    McpElicitation,
    ArchitectureChange,
    TitleUpdate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPendingOption {
    pub id: String,
    pub label: String,
    pub selected: bool,
    pub danger: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellToolConsentPending {
    pub command: String,
    pub message: String,
    pub command_editable: bool,
    pub can_allow: bool,
    pub can_deny: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellAskUserPending {
    pub show_other: bool,
    pub other_selected: bool,
    pub freeform: String,
    pub show_freeform: bool,
    pub show_skip: bool,
    pub show_back: bool,
    pub show_cancel: bool,
    pub show_reject: bool,
    pub can_submit: bool,
    pub submit_label: String,
    pub reject_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMcpFieldOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
    pub multi: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMcpField {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub value: String,
    pub enabled: bool,
    pub options: Vec<ShellMcpFieldOption>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMcpPending {
    pub url: Option<String>,
    pub raw_json: Option<String>,
    pub fields: Vec<ShellMcpField>,
    pub can_accept: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPending {
    pub request_id: String,
    pub kind: ShellPendingKind,
    pub title: String,
    pub prompt: String,
    pub draft: String,
    pub options: Vec<ShellPendingOption>,
    pub tool: Option<ShellToolConsentPending>,
    pub ask: Option<ShellAskUserPending>,
    pub mcp: Option<ShellMcpPending>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellSlashItem {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellProjectPage {
    Overview,
    Clone,
    Roadmap,
    Memory,
    Architecture,
    Settings,
    Files,
}

impl From<crate::application::ProjectWorkspaceSurface> for ShellProjectPage {
    fn from(surface: crate::application::ProjectWorkspaceSurface) -> Self {
        use crate::application::ProjectWorkspaceSurface;
        match surface {
            ProjectWorkspaceSurface::Roadmap => Self::Roadmap,
            ProjectWorkspaceSurface::Memory => Self::Memory,
            ProjectWorkspaceSurface::Architecture => Self::Architecture,
            ProjectWorkspaceSurface::Files => Self::Files,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellNavItem {
    pub id: String,
    pub label: String,
    pub settings: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMenuItem {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellTimelineRow {
    pub id: String,
    pub markdown: String,
    pub expanded: bool,
    pub can_expand: bool,
    pub can_retry: bool,
    pub can_copy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMentionItem {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellProjectCard {
    pub id: String,
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellRoadmapCard {
    pub id: String,
    pub title: String,
    pub status: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellArchitectureRecord {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellCodingFile {
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellCodingHit {
    pub id: String,
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellCodingSnapshot {
    pub query: String,
    pub mode_label: String,
    pub scope_label: String,
    pub busy: bool,
    pub git: String,
    pub files: Vec<ShellCodingFile>,
    pub hits: Vec<ShellCodingHit>,
    pub terminals: Vec<ShellActionRow>,
    pub tasks: Vec<ShellActionRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellAttachmentRow {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellSuggestionRow {
    pub id: String,
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellActionRow {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellProviderRow {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellCredentialRow {
    pub id: String,
    pub revision: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellAgentRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellSkillRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMcpRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMcpEditor {
    pub server_id: String,
    pub transport: String,
    pub location: String,
    pub args: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPaneItem {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub selected: bool,
    pub closable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellPaneRow {
    pub id: String,
    pub active: bool,
    pub items: Vec<ShellPaneItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellAutomationRow {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellDocumentSnapshot {
    pub item_id: String,
    pub title: String,
    pub text: String,
    pub language: String,
    pub status: String,
    pub read_only: bool,
    pub dirty: bool,
    pub diagnostics: Vec<ShellDiagnosticRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellDiagnosticRow {
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellMarkdownPreview {
    pub title: String,
    pub metadata: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellTerminalSnapshot {
    pub output: String,
    pub input: String,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellFilesSnapshot {
    pub tree: TreeView,
    pub preview: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettingsSnapshot {
    pub model: SettingsModel,
    pub state: SettingsState,
    pub appearance: AppearanceSettings,
    pub material_status: String,
    pub project_name: String,
    pub project_workspace: String,
    pub project_error: Option<String>,
    pub providers: Vec<ShellProviderRow>,
    pub provider_status: String,
    pub agent_actions: Vec<ShellActionRow>,
    pub quota_status: String,
    pub extensions_status: String,
    pub remote_status: String,
    pub remote_host_enabled: bool,
    pub remote_keep_awake: bool,
    pub desktop_status: String,
    pub data_status: String,
    pub data_can_import: bool,
    pub provider_secret: String,
    pub provider_model: String,
    pub provider_openai_endpoint: String,
    pub provider_anthropic_endpoint: String,
    pub can_save_credential: bool,
    pub credentials: Vec<ShellCredentialRow>,
    pub custom_agents: Vec<ShellAgentRow>,
    pub custom_agent_editor_open: bool,
    pub custom_agent_name: String,
    pub custom_agent_description: String,
    pub custom_agent_instruction: String,
    pub quota_days_label: String,
    pub quota_backend_label: String,
    pub quota_values: Vec<f64>,
    pub skills: Vec<ShellSkillRow>,
    pub skill_id: String,
    pub skill_description: String,
    pub can_create_skill: bool,
    pub mcp_servers: Vec<ShellMcpRow>,
    pub mcp_editor: Option<ShellMcpEditor>,
    pub github_state: String,
    pub github_login: String,
    pub github_busy: bool,
    pub github_can_bind: bool,
    pub shortcut: String,
    pub shortcut_capturing: bool,
    pub shortcut_registered: bool,
}

#[derive(Debug, Clone)]
pub struct PrimaryShellSnapshot {
    pub theme: ThemeMode,
    pub title: String,
    pub title_parent: String,
    pub title_context: String,
    pub heading: String,
    pub error: Option<String>,
    pub settings_open: bool,
    pub sidebar_collapsed: bool,
    pub sidebar_search_open: bool,
    pub sidebar_search_query: String,
    pub provider_badge: String,
    pub nav_items: Vec<ShellNavItem>,
    pub sidebar_rows: Vec<ShellSidebarRow>,
    pub sidebar_menu: Vec<ShellMenuItem>,
    pub sidebar_menu_anchor: Option<(f32, f32)>,
    pub add_project_menu_open: bool,
    pub workspace: WorkspaceModel,
    pub tasks: Vec<ShellTaskRow>,
    pub timeline: Vec<ShellTimelineRow>,
    pub composer: String,
    pub composer_task_id: Option<String>,
    pub composer_revision: u64,
    pub composer_height: f32,
    pub composer_placeholder: String,
    pub composer_disabled: bool,
    pub can_send: bool,
    pub can_interrupt: bool,
    pub pending_blocks_send: bool,
    pub clone_repository: String,
    pub clone_parent: String,
    pub milestone_title: String,
    pub attachments: Vec<ShellAttachmentRow>,
    pub plan_mode: bool,
    pub goal_mode: bool,
    pub permission_label: String,
    pub worktree_label: Option<String>,
    pub worktree_can_pick: bool,
    pub suggestions: Vec<ShellSuggestionRow>,
    pub suggestions_can_refresh: bool,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_selected: usize,
    pub command_palette_items: Vec<CommandPaletteItem>,
    pub settings: SettingsSnapshot,
    pub document: Option<ShellDocumentSnapshot>,
    pub files: Option<ShellFilesSnapshot>,
    pub terminal: Option<ShellTerminalSnapshot>,
    pub markdown_preview: Option<ShellMarkdownPreview>,
    pub inspector_title: String,
    pub inspector_body: String,
    pub inspector_todos: Vec<ShellTodoRow>,
    pub iab_url: String,
    pub confirm: Option<ShellConfirm>,
    pub pending: Option<ShellPending>,
    pub slash_items: Vec<ShellSlashItem>,
    pub mention_items: Vec<ShellMentionItem>,
    pub timeline_can_load_earlier: bool,
    pub composer_plus_open: bool,
    pub project_page: Option<ShellProjectPage>,
    pub project_page_title: String,
    pub project_page_body: String,
    pub project_cards: Vec<ShellProjectCard>,
    pub roadmap_cards: Vec<ShellRoadmapCard>,
    pub architecture_records: Vec<ShellArchitectureRecord>,
    pub architecture_graph: nana_ui::GraphModel,
    pub architecture_viewport: nana_ui::GraphViewport,
    pub architecture_selection: Option<nana_ui::GraphSelection>,
    pub inspector_kind: String,
    pub coding: Option<ShellCodingSnapshot>,
    pub pane_can_move_window: bool,
    pub pane_can_move_next: bool,
    pub titlebar_menu_open: bool,
    pub titlebar_has_task: bool,
    pub titlebar_can_split: bool,
    pub titlebar_can_close: bool,
    pub automations_open: bool,
    pub automations: Vec<ShellAutomationRow>,
    pub automation_graph: nana_ui::GraphModel,
    pub automation_viewport: nana_ui::GraphViewport,
    pub automation_selection: Option<nana_ui::GraphSelection>,
    pub panes: Vec<ShellPaneRow>,
}

#[derive(Debug, Clone)]
pub enum ShellIntent {
    ToggleSidebar,
    NewConversation,
    SelectTask(TaskId),
    ToggleSidebarSearch,
    SidebarSearchChanged(String),
    ToggleSidebarProject(String),
    ToggleSidebarInbox,
    RevealSidebarProject(String),
    RevealSidebarInbox,
    OpenProjectsOverview,
    OpenAddProjectMenu,
    OpenProjectMenu(String),
    OpenTaskMenu(String),
    OpenProjectDraft(String),
    RestoreProject(String),
    SelectProject(String),
    StopSidebarTask(TaskId),
    SidebarMenuAction(String),
    OpenAutomations,
    CloseAutomations,
    ConfirmDestructive,
    CancelDestructive,
    SelectPaneTab {
        pane_id: String,
        item_id: Option<String>,
    },
    ReorderPaneTab {
        pane_id: String,
        item_id: String,
        before: Option<String>,
    },
    ClosePaneTab {
        pane_id: String,
        item_id: String,
    },
    TransferPaneTab {
        source_strip: String,
        target_strip: String,
        item_id: String,
        before: Option<String>,
    },
    CloseMarkdownPreview,
    MarkdownImageViewerInteraction,
    IabUrlChanged(String),
    IabNavigate,
    IabOpenWindow,
    OpenTaskBrowser,
    ToggleComposerPlus,
    ComposerPlus(String),
    CyclePermission,
    CycleWorktree,
    PickWorktree,
    ApplySlash(String),
    SelectMention(String),
    ToggleTimelineExpand(String),
    StartGitHubBinding,
    CancelGitHubBinding,
    BeginShortcutCapture,
    SaveShortcut,
    ClearShortcut,
    CodingQueryChanged(String),
    SearchCoding,
    RefreshCoding,
    CycleCodingMode,
    ToggleCodingScope,
    OpenCodingHit(String),
    OpenCodingWorkspace,
    OpenCodingTerminal,
    SelectRoadmapMilestone(String),
    RefreshArchitecture,
    RollbackArchitecture,
    ArchitectureGraph(nana_ui::GraphCanvasEvent),
    RespondApproval {
        request_id: String,
        approved: bool,
    },
    RespondTitle {
        request_id: String,
        accepted: bool,
    },
    RespondArchitecture {
        request_id: String,
        approved: bool,
    },
    RespondPlan {
        request_id: String,
        action: String,
    },
    RespondToolConsent {
        request_id: String,
        approved: bool,
    },
    ToolConsentDraftChanged {
        request_id: String,
        command: String,
        message: String,
    },
    AskUserPending {
        request_id: String,
        action: String,
        value: String,
    },
    PendingDraftChanged {
        request_id: String,
        value: String,
    },
    SelectPendingOption {
        request_id: String,
        option_id: String,
    },
    RespondMcp {
        request_id: String,
        action: String,
    },
    McpFieldChanged {
        request_id: String,
        field_key: String,
        value: String,
    },
    McpRawJsonChanged {
        request_id: String,
        value: String,
    },
    McpToggleOption {
        request_id: String,
        field_key: String,
        value: String,
        multi: bool,
    },
    McpToggleBoolean {
        request_id: String,
        field_key: String,
    },
    OpenMarkdownLink(String),
    CloneRepositoryChanged(String),
    PickCloneParent,
    StartClone,
    CancelClone,
    MilestoneTitleChanged(String),
    CreateMilestone,
    SaveMilestone,
    LoadEarlierTimeline,
    CopyTimeline(String),
    RetryTimeline(String),
    MovePaneToWindow,
    MovePaneToNext,
    OpenSettings,
    CloseSettings,
    ComposerChanged(String),
    SubmitTurn,
    InterruptTurn,
    WindowChrome(WindowChromeEvent),
    ToggleCommandPalette,
    CommandPalette(CommandPaletteEvent),
    SelectSettingsTab(SettingsTabId),
    Appearance(AppearanceEvent),
    ProjectNameChanged(String),
    SaveProjectSettings,
    PickProjectWorkspace,
    SelectProvider(String),
    RefreshProvider,
    ToggleAgent(String),
    RefreshQuota,
    RefreshExtensions,
    ToggleRemoteHost,
    ToggleRemoteKeepAwake,
    CheckForUpdate,
    PickDataImportSource,
    ExecuteDataImport,
    ResetDataImport,
    RemoveAttachment(String),
    ApplySuggestion(String),
    RefreshSuggestions,
    DocumentChanged(String),
    SaveDocument,
    DiscardDocument,
    TerminalInput(String),
    TerminalSubmit,
    TerminalInterrupt,
    ToggleProjectFile(String),
    OpenProjectFile(String),
    RefreshProjectFiles,
    ProviderSecretChanged(String),
    SaveProviderCredential,
    RevokeProviderCredential {
        credential_id: String,
        revision: u64,
    },
    ProviderModelChanged(String),
    ProviderOpenAiEndpointChanged(String),
    ProviderAnthropicEndpointChanged(String),
    SaveProviderRuntimeSettings,
    ResetProviderRuntimeSettings,
    AgentNameChanged(String),
    AgentDescriptionChanged(String),
    AgentInstructionChanged(String),
    NewCustomAgent,
    EditCustomAgent(String),
    SaveCustomAgent,
    CancelCustomAgentEdit,
    ToggleCustomAgent(String),
    DeleteCustomAgent(String),
    CycleQuotaDays,
    CycleQuotaBackend,
    SkillIdChanged(String),
    SkillDescriptionChanged(String),
    CreateSkill,
    ToggleSkill(String),
    NewMcpServer,
    EditMcpServer(String),
    McpServerIdChanged(String),
    CycleMcpTransport,
    McpLocationChanged(String),
    McpArgsChanged(String),
    ToggleMcpEditorEnabled,
    SaveMcpServer,
    CancelMcpEditor,
    ToggleMcpServer(String),
    ToggleTitlebarMenu,
    BackToTaskList,
    OpenTaskPopup,
    AskTaskPopup,
    ToggleTaskInspector,
    SplitWorkspaceHorizontal,
    SplitWorkspaceVertical,
    CloseCurrentWorkspaceItem,
    OpenConversationStatus,
    CloseConversationStatus,
    ToggleConversationStatusPin,
    OpenConversationStatusNewChat,
    OpenStatusTask(TaskId),
    StopStatusTask(TaskId),
    ActivateWorkspaceItem(String),
    FocusWorkspacePane(String),
    SelectAutomation(String),
    CreateAutomation,
    SaveAutomationDraft,
    RunAutomation,
    RefreshAutomations,
    AutomationGraph(nana_ui::GraphCanvasEvent),
    CloseTaskPopup(nana_ui_platform::WindowId),
    TaskPopupComposerChanged {
        window_id: nana_ui_platform::WindowId,
        value: String,
    },
    TaskPopupSubmit(nana_ui_platform::WindowId),
    TaskPopupInterrupt(nana_ui_platform::WindowId),
}

type IntentSink = Arc<dyn Fn(ShellIntent) + Send + Sync>;

/// 上一次真正同步进 `RuntimeDocument` 的输入。`sync` 是快照的纯函数，
/// 所以输入不变时重跑 reconcile 只会产生同一棵树，可以整段跳过。
#[derive(Default)]
struct SyncedInputs {
    sidebar_rows: Vec<ShellSidebarRow>,
    sidebar_tasks: Vec<ShellTaskRow>,
    sidebar_search_open: bool,
    timeline: Vec<ShellTimelineRow>,
    workspace: Option<WorkspaceInputs>,
    inspector: Option<InspectorInputs>,
}

/// 工作区页、面板标签条与诊断面板共同的输入。
#[derive(PartialEq)]
struct WorkspaceInputs {
    panes: Vec<ShellPaneRow>,
    document: Option<ShellDocumentSnapshot>,
    terminal: Option<ShellTerminalSnapshot>,
    files: Option<ShellFilesSnapshot>,
}

#[derive(PartialEq)]
struct InspectorInputs {
    kind: String,
    todos: Vec<ShellTodoRow>,
    iab_url: String,
    coding: Option<ShellCodingSnapshot>,
}

pub struct ShellHandles {
    sink: IntentSink,
    shell: Entity<DesktopShell>,
    overlay_host: Option<Entity<OverlayHost>>,
    palette: Option<Entity<CommandPalette>>,
    more_menu: Option<Entity<ContextMenu>>,
    titlebar_menu: Option<Entity<ContextMenu>>,
    sidebar_toggle: Entity<IconButton>,
    footer_more: Entity<SidebarFooterButton>,
    form_fields: HashMap<String, Entity<TextArea>>,
    form_wrappers: HashMap<String, Entity<FormField>>,
    form_switches: HashMap<String, Entity<Switch>>,
    settings_card: Entity<SettingsCard>,
    quota_chart: Option<Entity<TimeSeriesChart>>,
    pane_bar: Entity<HostStack>,
    pane_buttons: HashMap<String, Entity<Button>>,
    automations_page: Entity<HostStack>,
    automation_list: Entity<HostStack>,
    automation_actions: Entity<HostStack>,
    automation_canvas: Entity<GraphCanvas>,
    title_center: Entity<HostStack>,
    title_parent: Entity<Text>,
    title_separator: Entity<Text>,
    title_context: Entity<Text>,
    title_leading: Entity<HostStack>,
    title_trailing: Entity<HostStack>,
    conversation_sidebar: Entity<SidebarFrame>,
    automations_sidebar: Entity<SidebarFrame>,
    automations_back: Entity<SidebarRow>,
    automations_section: Entity<SidebarSection>,
    automations_body: Entity<HostStack>,
    automations_footer: Entity<SidebarFooter>,
    sidebar_top: Entity<HostStack>,
    new_conversation: Entity<SidebarRow>,
    search_toggle: Entity<IconButton>,
    search_input: Entity<TextArea>,
    search_close: Entity<IconButton>,
    sidebar_scroll: Entity<ScrollView>,
    conversation_section: Entity<SidebarSection>,
    task_body: Entity<List>,
    project_section: Entity<SidebarSection>,
    project_header: Entity<ListItem>,
    project_body: Entity<List>,
    add_project_menu: Entity<ActionMenu>,
    inbox_section: Entity<SidebarSection>,
    inbox_body: Entity<List>,
    task_rows: HashMap<String, Entity<SidebarRow>>,
    row_kinds: HashMap<String, ShellSidebarKind>,
    row_tools: HashMap<String, Entity<HostStack>>,
    row_tool_buttons: HashMap<String, Entity<Button>>,
    footer_nav: HashMap<String, Entity<SidebarFooterButton>>,
    provider_badge: Entity<SidebarFooterButton>,
    conversation: Entity<HostStack>,
    conversation_column: Entity<HostStack>,
    conversation_body: Entity<HostStack>,
    settings_sidebar: Entity<SettingsSidebar>,
    settings_page: Entity<SettingsPage>,
    appearance: Entity<AppearanceSection>,
    about: Entity<AboutSection>,
    product_settings: Entity<HostStack>,
    product_heading: Entity<Text>,
    product_body: Entity<Text>,
    product_error: Entity<Text>,
    project_name: Entity<TextArea>,
    project_name_field: Entity<FormField>,
    project_workspace: Entity<Text>,
    project_workspace_row: Entity<SettingsRow>,
    product_actions: HashMap<String, Entity<Button>>,
    provider_rows: HashMap<String, Entity<Button>>,
    heading_slot: Entity<HostStack>,
    heading: Entity<Text>,
    error: Entity<Text>,
    timeline_scroll: Entity<ScrollView>,
    timeline_items: HashMap<String, Entity<HostStack>>,
    timeline_markdown: HashMap<String, Entity<NativeMarkdown>>,
    timeline_markdown_source: HashMap<String, u64>,
    timeline_actions: HashMap<String, Entity<Button>>,
    synced: SyncedInputs,
    composer_generation: ComposerGeneration,
    shell_assembled: bool,
    load_earlier: Option<Entity<Button>>,
    composer: Entity<TextArea>,
    composer_toolbar: Entity<HostStack>,
    extras: Entity<HostStack>,
    extra_buttons: HashMap<String, Entity<Button>>,
    plus_items: HashMap<String, Entity<ActionMenuItem>>,
    completion_slot: Entity<HostStack>,
    completion_items: HashMap<String, Entity<ActionMenuItem>>,
    plus_slot: Entity<HostStack>,
    plus_menu: Entity<ActionMenu>,
    attach: Entity<IconButton>,
    permission_slot: Entity<HostStack>,
    permission_icon: Entity<IconGlyph>,
    permission: Entity<Button>,
    worktree_slot: Entity<HostStack>,
    worktree_icon: Entity<IconGlyph>,
    worktree: Entity<Button>,
    worktree_pick: Entity<IconButton>,
    pending_panel: Entity<HostStack>,
    pending_title: Entity<Text>,
    pending_prompt: Entity<Text>,
    pending_draft: Entity<TextArea>,
    pending_tool_command: Entity<TextArea>,
    pending_tool_message: Entity<TextArea>,
    pending_request: Arc<Mutex<String>>,
    pending_tool_command_value: Arc<Mutex<String>>,
    pending_tool_message_value: Arc<Mutex<String>>,
    composer_actions: Entity<HostStack>,
    send: Entity<IconButton>,
    interrupt: Option<Entity<IconButton>>,
    project_page: Entity<HostStack>,
    project_page_title: Entity<Text>,
    project_page_body: Entity<Text>,
    project_cards: HashMap<String, Entity<InteractiveCard>>,
    architecture_canvas: Entity<GraphCanvas>,
    automations_empty: Entity<EmptyState>,
    workspace_page: Entity<HostStack>,
    pane_chrome: Entity<PaneChrome>,
    pane_header: Entity<HostStack>,
    pane_tabs: Entity<Tabs>,
    pane_body: Entity<HostStack>,
    workspace_content: Entity<HostStack>,
    workspace_heading: Entity<Text>,
    workspace_status: Entity<Text>,
    workspace_editor: Entity<TextArea>,
    workspace_log: Entity<TextArea>,
    workspace_input: Entity<TextArea>,
    diagnostics_panel: Entity<HostStack>,
    diagnostic_rows: HashMap<String, Entity<Text>>,
    image_viewer: Option<Entity<ImageViewer>>,
    workspace_actions: Entity<HostStack>,
    workspace_buttons: HashMap<String, Entity<Button>>,
    workspace_tree: Entity<TreeView>,
    inspector: Entity<HostStack>,
    inspector_heading: Entity<Text>,
    inspector_body: Entity<Text>,
    inspector_todos: Entity<HostStack>,
    inspector_todo_rows: HashMap<String, Entity<Text>>,
    coding_panel: Entity<HostStack>,
    coding_query: Entity<TextArea>,
    coding_rows: HashMap<String, Entity<Button>>,
    shortcut_capture: Entity<KeyCaptureLayer>,
    pane_move_window: Entity<Button>,
    pane_move_next: Entity<Button>,
    iab_url: Entity<TextArea>,
    iab_navigate: Entity<Button>,
    iab_open: Entity<Button>,
    confirm: Option<Entity<ConfirmDialog>>,
    confirm_cancel: Option<Entity<Button>>,
    confirm_commit: Option<Entity<Button>>,
    focus_targets: HashMap<String, StableNodeId>,
}

pub(crate) fn emit(sink: &IntentSink, intent: ShellIntent) {
    sink(intent);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposerGeneration {
    task_id: Option<String>,
    revision: u64,
}

impl ComposerGeneration {
    pub fn new(task_id: Option<String>, revision: u64) -> Self {
        Self { task_id, revision }
    }
}

pub(crate) fn composer_is_focused(context: &AppContext, composer: Entity<TextArea>) -> bool {
    context
        .world()
        .node(composer.stable_id())
        .is_some_and(|node| context.world().focused(node.document) == Some(composer.stable_id()))
}

pub(crate) fn bind_activate<V: View>(
    context: &mut AppContext,
    entity: Entity<V>,
    sink: IntentSink,
    intent: ShellIntent,
) -> Result<(), FrameworkError> {
    context.on(entity, move |_, _event: &Activate, _| {
        emit(&sink, intent.clone());
    })
}

fn sidebar_toggle_button(collapsed: bool) -> IconButton {
    IconButton::new(
        Icon::Sidebar,
        if collapsed {
            "显示会话栏"
        } else {
            "隐藏会话栏"
        },
    )
}

fn new_conversation_row(leading: StableNodeId) -> SidebarRow {
    let mut row = SidebarRow::new("新对话").size(ControlSize::Medium).slots(
        nana_ui::runtime::ListItemSlots {
            leading: Some(leading),
            content: None,
            trailing: None,
        },
    );
    row.style = sidebar_row_style();
    row
}

fn sidebar_row_style() -> NodeStyle {
    let mut style = NodeStyle::default();
    let layout = Arc::make_mut(&mut style.layout);
    // Rows are fixed height; a wrapping title would spill over the row tools.
    layout.white_space_nowrap = true;
    layout.text_overflow_ellipsis = true;
    style
}

fn sidebar_search_toggle() -> IconButton {
    sidebar_icon_button(Icon::Search, "搜索")
}

fn sidebar_search_close() -> IconButton {
    sidebar_icon_button(Icon::Close, "关闭搜索")
}

fn empty_heading(value: String) -> Text {
    let mut text = Text::new(value);
    let mut style = text.style.clone();
    style.foreground = Some(SemanticColorRole::Text);
    let layout = Arc::make_mut(&mut style.layout);
    layout.font_size = Some(EMPTY_HEADING_FONT_SIZE);
    layout.font_weight = Some(500);
    layout.text_align = TextAlignSpec::Center;
    text.style = style;
    text
}

fn breadcrumb_parent(text: &str) -> Text {
    breadcrumb_text(text, SemanticColorRole::Muted, None)
}

fn breadcrumb_separator() -> Text {
    breadcrumb_text("›", SemanticColorRole::Faint, None)
}

fn breadcrumb_context(text: &str) -> Text {
    breadcrumb_text(text, SemanticColorRole::Text, Some(600))
}

fn breadcrumb_text(value: &str, color: SemanticColorRole, weight: Option<u16>) -> Text {
    let mut text = Text::new(value.to_owned());
    let mut style = text.style.clone();
    style.foreground = Some(color);
    let layout = Arc::make_mut(&mut style.layout);
    layout.white_space_nowrap = true;
    layout.text_overflow_ellipsis = true;
    layout.font_weight = weight;
    text.style = style;
    text
}

fn extra_button(label: &str, kind: ButtonKind) -> Button {
    pill_button(label, kind)
}

fn markdown_image_viewer(preview: &ShellMarkdownPreview) -> ImageViewer {
    ImageViewer::new(ImageViewerContent::None)
        .name(preview.title.clone())
        .metadata(preview.metadata.clone())
}

fn fill_workspace_editor(value: impl Into<String>, language: Option<&str>) -> TextArea {
    let mut editor = TextArea::new(value.into());
    apply_workspace_editor_chrome(&mut editor, language);
    editor
}

fn apply_workspace_editor_chrome(editor: &mut TextArea, language: Option<&str>) {
    editor.highlight = language
        .filter(|language| !language.is_empty())
        .map(|language| HighlightRequest::highlight(language.to_owned()));
    fill_workspace_surface(editor);
}

fn fill_workspace_log(value: impl Into<String>) -> TextArea {
    let mut log = fill_workspace_editor(value, None);
    log.disabled = true;
    log
}

fn fill_workspace_surface(area: &mut TextArea) {
    let layout = Arc::make_mut(&mut area.style.layout);
    layout.height = Some(LengthSpec::Fill);
    layout.flex_grow = Some(1.0);
    layout.min_height = Some(LengthSpec::Px(0.0));
}

fn row_menu_button() -> Button {
    Button::new("•••")
        .kind(ButtonKind::Text)
        .size(ControlSize::Small)
}

fn row_stop_button() -> Button {
    Button::new("停止")
        .kind(ButtonKind::Danger)
        .size(ControlSize::Small)
}

fn row_draft_button() -> Button {
    Button::new("＋")
        .kind(ButtonKind::Text)
        .size(ControlSize::Small)
}

fn composer_view(snapshot: &PrimaryShellSnapshot) -> TextArea {
    flatten_composer_textarea(
        TextArea::new(snapshot.composer.clone())
            .placeholder(snapshot.composer_placeholder.clone())
            .disabled(snapshot.composer_disabled)
            .height(
                snapshot
                    .composer_height
                    .clamp(COMPOSER_MIN_HEIGHT, COMPOSER_MAX_HEIGHT),
            ),
    )
}

fn composer_plus_menu(open: bool) -> ActionMenu {
    ActionMenu::new().trigger("+").open(open)
}

fn composer_attach_button() -> IconButton {
    IconButton::new(Icon::Paperclip, "添加文件")
        .kind(ButtonKind::Text)
        .size(ControlSize::Small)
}

fn worktree_pick_button() -> IconButton {
    IconButton::new(Icon::Folder, "选择工作树")
        .kind(ButtonKind::Text)
        .size(ControlSize::Small)
}

fn plus_menu_items(snapshot: &PrimaryShellSnapshot) -> Vec<(String, String)> {
    vec![
        ("add-file".into(), "添加文件".into()),
        ("add-directory".into(), "添加目录".into()),
        ("reference".into(), "引用其他对话".into()),
        ("paste-text".into(), "粘贴文字".into()),
        ("paste-image".into(), "粘贴图片".into()),
        ("paste-files".into(), "粘贴文件".into()),
        (
            "plan".into(),
            if snapshot.plan_mode {
                "关闭计划模式".into()
            } else {
                "开启计划模式".into()
            },
        ),
        (
            "goal".into(),
            if snapshot.goal_mode {
                "关闭目标模式".into()
            } else {
                "开启目标模式".into()
            },
        ),
    ]
}

fn pane_tab_options(snapshot: &PrimaryShellSnapshot) -> (String, Vec<TabOption>) {
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.active)
        .or_else(|| snapshot.panes.first());
    let selected = pane
        .and_then(|pane| {
            pane.items
                .iter()
                .find(|item| item.selected)
                .map(|item| item.id.clone())
        })
        .unwrap_or_default();
    let options = pane
        .map(|pane| {
            pane.items
                .iter()
                .map(|item| {
                    TabOption::new(item.id.clone(), item.title.clone()).closable(item.closable)
                })
                .collect()
        })
        .unwrap_or_default();
    (selected, options)
}

fn active_pane_strip_id(snapshot: &PrimaryShellSnapshot) -> String {
    snapshot
        .panes
        .iter()
        .find(|pane| pane.active)
        .or_else(|| snapshot.panes.first())
        .map(|pane| format!("workspace/main/pane/{}", pane.id))
        .unwrap_or_else(|| "workspace/main/pane/active".to_owned())
}

fn workspace_tabs_intent(event: &TabsEvent) -> ShellIntent {
    match event {
        TabsEvent::Select(value) => {
            let item_id = if value.as_ref() == "conversation" {
                None
            } else {
                Some(value.to_string())
            };
            ShellIntent::SelectPaneTab {
                pane_id: "active".into(),
                item_id,
            }
        }
        TabsEvent::Reorder { value, before } => ShellIntent::ReorderPaneTab {
            pane_id: "active".into(),
            item_id: value.to_string(),
            before: before.as_ref().map(|value| value.to_string()),
        },
        TabsEvent::Close(value) => ShellIntent::ClosePaneTab {
            pane_id: "active".into(),
            item_id: value.to_string(),
        },
        TabsEvent::Transfer {
            source_strip,
            value,
            target_strip,
            before,
        } => ShellIntent::TransferPaneTab {
            source_strip: source_strip.to_string(),
            target_strip: target_strip.to_string(),
            item_id: value.to_string(),
            before: before.as_ref().map(|value| value.to_string()),
        },
    }
}

fn settings_field_label(id: &str) -> &'static str {
    match id {
        "provider_secret" => "访问密钥",
        "provider_model" => "模型",
        "provider_openai" => "OpenAI 端点",
        "provider_anthropic" => "Anthropic 端点",
        "agent_name" => "名称",
        "agent_description" => "说明",
        "agent_instruction" => "指令",
        "skill_id" => "技能标识",
        "skill_description" => "技能说明",
        "mcp_server_id" => "MCP 标识",
        "mcp_location" => "位置",
        "mcp_args" => "参数",
        "project-clone-repository" => "仓库",
        "project-milestone-title" => "里程碑",
        _ if id.starts_with("pending-") => "内容",
        _ => "值",
    }
}

fn product_action_button(label: &str, primary: bool) -> Button {
    Button::new(label)
        .kind(if primary {
            ButtonKind::Primary
        } else {
            ButtonKind::Subtle
        })
        .size(ControlSize::Medium)
}

fn command_palette_view(snapshot: &PrimaryShellSnapshot) -> CommandPalette {
    CommandPalette::new("命令", snapshot.command_palette_items.clone())
        .placeholder("搜索命令")
        .query(snapshot.command_palette_query.clone())
}

pub fn mount_primary_shell(
    snapshot: &PrimaryShellSnapshot,
    sink: IntentSink,
) -> Result<(nana_ui::runtime::RuntimeDocument, ShellHandles), FrameworkError> {
    let document_id = DocumentId::new(PRIMARY_DOCUMENT).expect("primary document id");
    let mut document = nana_ui::runtime::RuntimeDocument::new(document_id);
    let context = document.context_mut();
    let _ = context.set_theme(snapshot.theme);

    let title_leading =
        context.create_detached_component(document_id, HostStack::leading_row(0.0))?;
    let sidebar_toggle = context.create_detached_component(
        document_id,
        sidebar_toggle_button(snapshot.sidebar_collapsed),
    )?;
    context.append_child(title_leading, sidebar_toggle)?;
    bind_activate(
        context,
        sidebar_toggle,
        Arc::clone(&sink),
        ShellIntent::ToggleSidebar,
    )?;

    let title_center = context.create_detached_component(document_id, HostStack::bar(6.0))?;
    let title_parent = context
        .create_detached_component(document_id, breadcrumb_parent(&snapshot.title_parent))?;
    let title_separator = context.create_detached_component(document_id, breadcrumb_separator())?;
    let title_context = context
        .create_detached_component(document_id, breadcrumb_context(&snapshot.title_context))?;
    context.append_child(title_center, title_parent)?;
    context.append_child(title_center, title_separator)?;
    context.append_child(title_center, title_context)?;
    // The reference chrome keeps window controls alone on the trailing edge; the
    // command palette and inspector already live in the titlebar more menu, which
    // now hangs off the sidebar footer.
    let title_trailing = context.create_detached_component(document_id, HostStack::row(6.0))?;
    if WindowChrome::platform_default().uses_custom_controls() {
        let minimize = context.create_detached_component(
            document_id,
            window_control(Icon::Minimize, "最小化", ButtonKind::Text),
        )?;
        let maximize = context.create_detached_component(
            document_id,
            window_control(Icon::Maximize, "最大化", ButtonKind::Text),
        )?;
        let close = context.create_detached_component(
            document_id,
            window_control(Icon::Close, "关闭", ButtonKind::Text),
        )?;
        context.append_child(title_trailing, minimize)?;
        context.append_child(title_trailing, maximize)?;
        context.append_child(title_trailing, close)?;
        bind_activate(
            context,
            minimize,
            Arc::clone(&sink),
            ShellIntent::WindowChrome(WindowChromeEvent::Action(WindowChromeAction::Minimize)),
        )?;
        bind_activate(
            context,
            maximize,
            Arc::clone(&sink),
            ShellIntent::WindowChrome(WindowChromeEvent::Action(
                WindowChromeAction::ToggleMaximize,
            )),
        )?;
        bind_activate(
            context,
            close,
            Arc::clone(&sink),
            ShellIntent::WindowChrome(WindowChromeEvent::Action(WindowChromeAction::Close)),
        )?;
    }

    let sidebar_top = context.create_detached_component(document_id, HostStack::bar(6.0))?;
    let new_conversation_icon = context
        .create_detached_component(document_id, SidebarRowIcon::new(Icon::MessageSquarePlus))?;
    let new_conversation = context.create_detached_component(
        document_id,
        new_conversation_row(new_conversation_icon.stable_id()),
    )?;
    context.append_child(new_conversation, new_conversation_icon)?;
    let search_toggle = context.create_detached_component(document_id, sidebar_search_toggle())?;
    let search_close = context.create_detached_component(document_id, sidebar_search_close())?;
    let search_input = context.create_detached_component(
        document_id,
        TextArea::new(snapshot.sidebar_search_query.clone())
            .placeholder("搜索项目和会话")
            .height(32.0),
    )?;
    let search_sink = Arc::clone(&sink);
    context.on(search_input, move |_, event: &TextChanged, _| {
        emit(
            &search_sink,
            ShellIntent::SidebarSearchChanged(event.value.clone()),
        );
    })?;
    bind_activate(
        context,
        new_conversation,
        Arc::clone(&sink),
        ShellIntent::NewConversation,
    )?;
    bind_activate(
        context,
        search_toggle,
        Arc::clone(&sink),
        ShellIntent::ToggleSidebarSearch,
    )?;
    bind_activate(
        context,
        search_close,
        Arc::clone(&sink),
        ShellIntent::ToggleSidebarSearch,
    )?;
    if snapshot.sidebar_search_open {
        context.append_child(sidebar_top, search_input)?;
        context.append_child(sidebar_top, search_close)?;
    } else {
        context.append_child(sidebar_top, new_conversation)?;
        context.append_child(sidebar_top, search_toggle)?;
    }

    let add_project_menu =
        context.create_detached_component(document_id, composer_plus_menu(false))?;
    context.on(add_project_menu, {
        let sink = Arc::clone(&sink);
        move |_, event: &PopoverToggled, _| {
            if event.open {
                emit(&sink, ShellIntent::OpenAddProjectMenu);
            } else {
                emit(&sink, ShellIntent::SidebarMenuAction(String::new()));
            }
        }
    })?;
    let (section, _session_header, task_body) = mount_sidebar_section(
        context,
        document_id,
        "会话",
        Some(SESSIONS_EMPTY_TEXT),
        None,
    )?;
    let (project_section, project_header, project_body) = mount_sidebar_section(
        context,
        document_id,
        "项目",
        Some(PROJECTS_EMPTY_TEXT),
        Some(add_project_menu),
    )?;
    bind_activate(
        context,
        project_header,
        Arc::clone(&sink),
        ShellIntent::OpenProjectsOverview,
    )?;
    let (inbox_section, inbox_header, inbox_body) =
        mount_sidebar_section(context, document_id, "收集箱", Some(INBOX_EMPTY_TEXT), None)?;
    bind_activate(
        context,
        inbox_header,
        Arc::clone(&sink),
        ShellIntent::ToggleSidebarInbox,
    )?;
    let scroll =
        context.create_detached_component(document_id, SidebarFrame::vertical_body_scroll())?;
    context.append_child(scroll, section)?;
    let footer = context.create_detached_component(document_id, SidebarFooter::new())?;
    let mut footer_nav = HashMap::new();
    for item in &snapshot.nav_items {
        let button = context.create_detached_component(
            document_id,
            SidebarFooterButton::new(item.label.clone(), nav_icon(item.settings))
                .selected(item.selected),
        )?;
        context.append_child(footer, button)?;
        bind_activate(
            context,
            button,
            Arc::clone(&sink),
            if item.settings {
                ShellIntent::OpenSettings
            } else {
                ShellIntent::OpenAutomations
            },
        )?;
        footer_nav.insert(item.id.clone(), button);
    }
    let more = context
        .create_detached_component(document_id, SidebarFooterButton::new("更多", Icon::Nodes))?;
    context.append_child(footer, more)?;
    bind_activate(
        context,
        more,
        Arc::clone(&sink),
        ShellIntent::ToggleTitlebarMenu,
    )?;
    let provider_badge = context.create_detached_component(
        document_id,
        SidebarFooterButton::new(snapshot.provider_badge.clone(), Icon::Appearance),
    )?;
    context.append_child(footer, provider_badge)?;
    bind_activate(
        context,
        provider_badge,
        Arc::clone(&sink),
        ShellIntent::OpenSettings,
    )?;
    let conversation_sidebar = context.create_detached_component(
        document_id,
        SidebarFrame::new()
            .top(sidebar_top.stable_id())
            .body(scroll.stable_id())
            .footer(footer.stable_id()),
    )?;
    context.append_child(conversation_sidebar, sidebar_top)?;
    context.append_child(conversation_sidebar, scroll)?;
    context.append_child(conversation_sidebar, footer)?;

    let conversation = context.create_detached_component(
        document_id,
        HostStack::fill_column(0.0)
            .padding_xy(24.0, 20.0)
            .radius(UI_METRICS.radius_lg)
            .align(AlignSpec::Center),
    )?;
    let conversation_column = context.create_detached_component(
        document_id,
        HostStack::fill_column(12.0).max_width(CHAT_CONTENT_MAX_WIDTH),
    )?;
    let conversation_body =
        context.create_detached_component(document_id, HostStack::fill_column(12.0))?;
    let heading_slot = context.create_detached_component(
        document_id,
        HostStack::headline_slot(!snapshot.heading.trim().is_empty()),
    )?;
    let heading =
        context.create_detached_component(document_id, empty_heading(snapshot.heading.clone()))?;
    context.append_child(heading_slot, heading)?;
    let error = context.create_detached_component(
        document_id,
        Text::new(snapshot.error.clone().unwrap_or_default()),
    )?;
    let timeline_scroll =
        context.create_detached_component(document_id, ScrollView::new(ScrollAxes::Vertical))?;
    context.append_child(conversation_body, heading_slot)?;
    context.append_child(conversation_body, error)?;
    context.append_child(conversation_body, timeline_scroll)?;
    let composer_dock =
        context.create_detached_component(document_id, HostStack::composer_card())?;
    let composer = context.create_detached_component(document_id, composer_view(snapshot))?;
    let composer_sink = Arc::clone(&sink);
    context.on(composer, move |_, event: &TextChanged, _| {
        emit(
            &composer_sink,
            ShellIntent::ComposerChanged(event.value.clone()),
        );
    })?;
    let extras = context.create_detached_component(document_id, HostStack::fill_row(6.0))?;
    let plus_slot = context.create_detached_component(
        document_id,
        HostStack::trigger_slot(PLUS_SLOT_SIZE, PLUS_SLOT_SIZE),
    )?;
    let plus_menu = context
        .create_detached_component(document_id, composer_plus_menu(snapshot.composer_plus_open))?;
    context.on(plus_menu, {
        let sink = Arc::clone(&sink);
        move |_, _: &PopoverToggled, _| emit(&sink, ShellIntent::ToggleComposerPlus)
    })?;
    let attach = context.create_detached_component(document_id, composer_attach_button())?;
    bind_activate(
        context,
        attach,
        Arc::clone(&sink),
        ShellIntent::ComposerPlus("add-file".to_owned()),
    )?;
    let permission_slot =
        context.create_detached_component(document_id, HostStack::leading_row(4.0))?;
    let permission_icon =
        context.create_detached_component(document_id, IconGlyph::new(Icon::ShieldCheck))?;
    let permission = context.create_detached_component(
        document_id,
        pill_button(&snapshot.permission_label, ButtonKind::Text),
    )?;
    bind_activate(
        context,
        permission,
        Arc::clone(&sink),
        ShellIntent::CyclePermission,
    )?;
    let worktree_slot =
        context.create_detached_component(document_id, HostStack::leading_row(4.0))?;
    let worktree_icon =
        context.create_detached_component(document_id, IconGlyph::new(Icon::GitBranch))?;
    let worktree = context.create_detached_component(
        document_id,
        pill_button(
            snapshot.worktree_label.as_deref().unwrap_or_default(),
            ButtonKind::Text,
        ),
    )?;
    bind_activate(
        context,
        worktree,
        Arc::clone(&sink),
        ShellIntent::CycleWorktree,
    )?;
    let worktree_pick = context.create_detached_component(document_id, worktree_pick_button())?;
    bind_activate(
        context,
        worktree_pick,
        Arc::clone(&sink),
        ShellIntent::PickWorktree,
    )?;
    context.append_child(worktree_slot, worktree_icon)?;
    context.append_child(worktree_slot, worktree)?;
    context.append_child(plus_slot, plus_menu)?;
    context.append_child(permission_slot, permission_icon)?;
    context.append_child(permission_slot, permission)?;
    context.append_child(extras, plus_slot)?;
    context.append_child(extras, attach)?;
    context.append_child(extras, permission_slot)?;
    let pending_panel = context.create_detached_component(document_id, HostStack::column(8.0))?;
    let pending_title = context.create_detached_component(document_id, Text::new(String::new()))?;
    let pending_prompt =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let pending_draft = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    let pending_request = Arc::new(Mutex::new(String::new()));
    let pending_tool_command_value = Arc::new(Mutex::new(String::new()));
    let pending_tool_message_value = Arc::new(Mutex::new(String::new()));
    context.on(pending_draft, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                ShellIntent::PendingDraftChanged {
                    request_id,
                    value: event.value.clone(),
                },
            );
        }
    })?;
    let pending_tool_command = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    context.on(pending_tool_command, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        let pending_tool_command_value = Arc::clone(&pending_tool_command_value);
        let pending_tool_message_value = Arc::clone(&pending_tool_message_value);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if let Ok(mut guard) = pending_tool_command_value.lock() {
                *guard = event.value.clone();
            }
            let message = pending_tool_message_value
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                ShellIntent::ToolConsentDraftChanged {
                    request_id,
                    command: event.value.clone(),
                    message,
                },
            );
        }
    })?;
    let pending_tool_message = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(48.0))?;
    context.on(pending_tool_message, {
        let sink = Arc::clone(&sink);
        let pending_request = Arc::clone(&pending_request);
        let pending_tool_command_value = Arc::clone(&pending_tool_command_value);
        let pending_tool_message_value = Arc::clone(&pending_tool_message_value);
        move |_, event: &TextChanged, _| {
            let request_id = pending_request
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            if let Ok(mut guard) = pending_tool_message_value.lock() {
                *guard = event.value.clone();
            }
            let command = pending_tool_command_value
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            emit(
                &sink,
                ShellIntent::ToolConsentDraftChanged {
                    request_id,
                    command,
                    message: event.value.clone(),
                },
            );
        }
    })?;
    context.append_child(pending_panel, pending_title)?;
    context.append_child(pending_panel, pending_prompt)?;
    let actions = context.create_detached_component(document_id, HostStack::row(6.0))?;
    let send =
        context.create_detached_component(document_id, composer_send_button(snapshot.can_send))?;
    bind_activate(context, send, Arc::clone(&sink), ShellIntent::SubmitTurn)?;
    context.append_child(actions, send)?;
    let composer_toolbar = context.create_detached_component(
        document_id,
        HostStack::bar(8.0).justify(JustifySpec::SpaceBetween),
    )?;
    context.append_child(composer_toolbar, extras)?;
    context.append_child(composer_toolbar, actions)?;
    let completion_slot = context.create_detached_component(document_id, HostStack::column(1.0))?;
    context.append_child(composer_dock, pending_panel)?;
    context.append_child(composer_dock, composer)?;
    context.append_child(composer_dock, composer_toolbar)?;
    context.append_child(conversation_column, conversation_body)?;
    context.append_child(conversation_column, composer_dock)?;
    context.append_child(conversation, conversation_column)?;

    let settings_sidebar = context.create_detached_component(
        document_id,
        SettingsSidebar::new(
            snapshot.settings.model.clone(),
            snapshot.settings.state.clone(),
        ),
    )?;
    context.on(settings_sidebar, {
        let sink = Arc::clone(&sink);
        move |_, _event: &SettingsBack, _| emit(&sink, ShellIntent::CloseSettings)
    })?;
    context.on(settings_sidebar, {
        let sink = Arc::clone(&sink);
        move |_, event: &SettingsTabSelected, _| {
            emit(&sink, ShellIntent::SelectSettingsTab(event.tab.clone()))
        }
    })?;
    let appearance = context.create_detached_component(
        document_id,
        AppearanceSection::new(snapshot.theme, snapshot.settings.appearance.clone())
            .material_status(snapshot.settings.material_status.clone()),
    )?;
    context.on(appearance, {
        let sink = Arc::clone(&sink);
        move |_, event: &AppearanceEvent, _| emit(&sink, ShellIntent::Appearance(*event))
    })?;
    let about = context.create_detached_component(
        document_id,
        AboutSection::new(
            AboutMetadata::new("LiliaCode", env!("CARGO_PKG_VERSION")).description("本机工作区"),
        ),
    )?;
    let product_settings = context
        .create_detached_component(document_id, HostStack::fill_column(10.0).padding(4.0))?;
    let settings_card =
        context.create_detached_component(document_id, SettingsCard::new(String::new()))?;
    let product_heading =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let product_body = context.create_detached_component(document_id, Text::new(String::new()))?;
    let product_error = context.create_detached_component(document_id, Text::new(String::new()))?;
    let shortcut_capture =
        context.create_detached_component(document_id, KeyCaptureLayer::new())?;
    let project_name = context.create_detached_component(
        document_id,
        TextArea::new(snapshot.settings.project_name.clone()).height(40.0),
    )?;
    let project_name_sink = Arc::clone(&sink);
    context.on(project_name, move |_, event: &TextChanged, _| {
        emit(
            &project_name_sink,
            ShellIntent::ProjectNameChanged(event.value.clone()),
        );
    })?;
    let project_name_field = context.create_detached_component(
        document_id,
        FormField::new("项目名称").control_child(project_name.stable_id()),
    )?;
    context.append_child(project_name_field, project_name)?;
    let project_workspace = context.create_detached_component(
        document_id,
        Text::new(snapshot.settings.project_workspace.clone()),
    )?;
    let project_workspace_row = context.create_detached_component(
        document_id,
        SettingsRow::new("工作区")
            .stacked(true)
            .control_child(project_workspace.stable_id()),
    )?;
    context.append_child(project_workspace_row, project_workspace)?;
    context.append_child(product_settings, product_heading)?;
    context.append_child(product_settings, product_body)?;
    context.append_child(product_settings, product_error)?;
    context.append_child(product_settings, settings_card)?;
    let settings_page = context.create_detached_component(
        document_id,
        SettingsPage::new(
            snapshot.settings.model.clone(),
            snapshot.settings.state.clone(),
        )
        .content(appearance.stable_id()),
    )?;

    let workspace_page =
        context.create_detached_component(document_id, HostStack::fill_column(0.0))?;
    let pane_header = context.create_detached_component(document_id, HostStack::bar(6.0))?;
    let (pane_selected, pane_options) = pane_tab_options(snapshot);
    let pane_tabs = context.create_detached_component(
        document_id,
        Tabs::new(pane_selected)
            .options(pane_options)
            .strip_id(active_pane_strip_id(snapshot))
            .fill(true),
    )?;
    let pane_sink = Arc::clone(&sink);
    context.on(pane_tabs, move |_, event: &TabsEvent, _| {
        emit(&pane_sink, workspace_tabs_intent(event));
    })?;
    let pane_split_h = context
        .create_detached_component(document_id, extra_button("左右分栏", ButtonKind::Text))?;
    let pane_split_v = context
        .create_detached_component(document_id, extra_button("上下分栏", ButtonKind::Text))?;
    bind_activate(
        context,
        pane_split_h,
        Arc::clone(&sink),
        ShellIntent::SplitWorkspaceHorizontal,
    )?;
    bind_activate(
        context,
        pane_split_v,
        Arc::clone(&sink),
        ShellIntent::SplitWorkspaceVertical,
    )?;
    let pane_move_window = context
        .create_detached_component(document_id, extra_button("移至新窗口", ButtonKind::Text))?;
    let pane_move_next = context
        .create_detached_component(document_id, extra_button("移至下一窗格", ButtonKind::Text))?;
    bind_activate(
        context,
        pane_move_window,
        Arc::clone(&sink),
        ShellIntent::MovePaneToWindow,
    )?;
    bind_activate(
        context,
        pane_move_next,
        Arc::clone(&sink),
        ShellIntent::MovePaneToNext,
    )?;
    let pane_body = context.create_detached_component(document_id, HostStack::fill_column(0.0))?;
    let pane_chrome = context.create_detached_component(
        document_id,
        PaneChrome::new()
            .header(pane_header.stable_id())
            .tabs(pane_tabs.stable_id())
            .body(pane_body.stable_id())
            .actions([
                PaneChromeAction::new(PaneChromeActionKind::SplitHorizontal, "左右分栏")
                    .target(pane_split_h.stable_id()),
                PaneChromeAction::new(PaneChromeActionKind::SplitVertical, "上下分栏")
                    .target(pane_split_v.stable_id()),
                PaneChromeAction::new(PaneChromeActionKind::MoveToWindow, "移至新窗口")
                    .target(pane_move_window.stable_id()),
                PaneChromeAction::new(PaneChromeActionKind::MoveToNextPane, "移至下一窗格")
                    .target(pane_move_next.stable_id()),
            ]),
    )?;
    context.append_child(pane_chrome, pane_tabs)?;
    context.append_child(pane_chrome, pane_header)?;
    context.append_child(pane_chrome, pane_body)?;
    context.append_child(workspace_page, pane_chrome)?;
    let pane_bar = context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    context.append_child(workspace_page, pane_bar)?;
    let workspace_content = context
        .create_detached_component(document_id, HostStack::fill_column(12.0).padding(16.0))?;
    let workspace_heading =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let workspace_status =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let workspace_editor = context
        .create_detached_component(document_id, fill_workspace_editor(String::new(), None))?;
    let workspace_editor_sink = Arc::clone(&sink);
    context.on(workspace_editor, move |_, event: &TextChanged, _| {
        emit(
            &workspace_editor_sink,
            ShellIntent::DocumentChanged(event.value.clone()),
        );
    })?;
    let workspace_log =
        context.create_detached_component(document_id, fill_workspace_log(String::new()))?;
    let workspace_tree =
        context.create_detached_component(document_id, TreeView::new(Vec::new()))?;
    let tree_sink = Arc::clone(&sink);
    context.on(
        workspace_tree,
        move |_, event: &TreeViewEvent<Arc<str>>, _| match event {
            TreeViewEvent::Toggle(path) => {
                emit(&tree_sink, ShellIntent::ToggleProjectFile(path.to_string()));
            }
            TreeViewEvent::Select(path) => {
                emit(&tree_sink, ShellIntent::OpenProjectFile(path.to_string()));
            }
        },
    )?;
    let workspace_input = context
        .create_detached_component(document_id, TextArea::new(String::new()).height(72.0))?;
    let workspace_input_sink = Arc::clone(&sink);
    context.on(workspace_input, move |_, event: &TextChanged, _| {
        emit(
            &workspace_input_sink,
            ShellIntent::TerminalInput(event.value.clone()),
        );
    })?;
    let workspace_actions = context.create_detached_component(document_id, HostStack::row(8.0))?;
    context.append_child(workspace_content, workspace_heading)?;
    context.append_child(workspace_content, workspace_status)?;
    context.append_child(workspace_content, workspace_tree)?;
    context.append_child(workspace_content, workspace_editor)?;
    context.append_child(workspace_content, workspace_log)?;
    context.append_child(workspace_content, workspace_input)?;
    context.append_child(workspace_content, workspace_actions)?;
    context.append_child(pane_body, workspace_content)?;

    let inspector = context
        .create_detached_component(document_id, HostStack::fill_column(8.0).padding(12.0))?;
    let inspector_heading = context
        .create_detached_component(document_id, Text::new(snapshot.inspector_title.clone()))?;
    let inspector_body = context
        .create_detached_component(document_id, Text::new(snapshot.inspector_body.clone()))?;
    context.append_child(inspector, inspector_heading)?;
    context.append_child(inspector, inspector_body)?;
    let inspector_todos = context.create_detached_component(document_id, HostStack::column(4.0))?;
    context.append_child(inspector, inspector_todos)?;
    let iab_url = context.create_detached_component(
        document_id,
        TextArea::new(snapshot.iab_url.clone())
            .placeholder("地址")
            .height(36.0),
    )?;
    context.on(iab_url, {
        let sink = Arc::clone(&sink);
        move |_, event: &TextChanged, _| {
            emit(&sink, ShellIntent::IabUrlChanged(event.value.clone()))
        }
    })?;
    let iab_navigate = context
        .create_detached_component(document_id, extra_button("打开", ButtonKind::Primary))?;
    let iab_open = context
        .create_detached_component(document_id, extra_button("新窗口", ButtonKind::Subtle))?;
    bind_activate(
        context,
        iab_navigate,
        Arc::clone(&sink),
        ShellIntent::IabNavigate,
    )?;
    bind_activate(
        context,
        iab_open,
        Arc::clone(&sink),
        ShellIntent::IabOpenWindow,
    )?;
    context.append_child(inspector, iab_url)?;
    context.append_child(inspector, iab_navigate)?;
    context.append_child(inspector, iab_open)?;
    let diagnostics_panel =
        context.create_detached_component(document_id, HostStack::column(4.0).padding(8.0))?;
    let coding_panel =
        context.create_detached_component(document_id, HostStack::fill_column(8.0))?;
    let coding_query = context.create_detached_component(
        document_id,
        TextArea::new(String::new())
            .placeholder("搜索工作区")
            .height(36.0),
    )?;
    context.on(coding_query, {
        let sink = Arc::clone(&sink);
        move |_, event: &TextChanged, _| {
            emit(&sink, ShellIntent::CodingQueryChanged(event.value.clone()))
        }
    })?;
    context.append_child(coding_panel, coding_query)?;
    context.append_child(inspector, coding_panel)?;

    let automations_page = context
        .create_detached_component(document_id, HostStack::fill_column(10.0).padding(16.0))?;
    let automation_list =
        context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    let automation_actions =
        context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    let automation_canvas = context.create_detached_component(
        document_id,
        GraphCanvas::new("automations", snapshot.automation_graph.clone())
            .viewport(snapshot.automation_viewport.clone())
            .selection(snapshot.automation_selection.clone()),
    )?;
    let graph_sink = Arc::clone(&sink);
    context.on(
        automation_canvas,
        move |_, event: &nana_ui::GraphCanvasEvent, _| {
            emit(&graph_sink, ShellIntent::AutomationGraph(event.clone()));
        },
    )?;
    context.append_child(automations_page, automation_list)?;
    context.append_child(automations_page, automation_actions)?;
    context.append_child(automations_page, automation_canvas)?;
    let automations_empty = context.create_detached_component(
        document_id,
        EmptyState::new("还没有自动化").message("新建一个工作流后，可在节点图中检查并发布。"),
    )?;
    let automations_back = context.create_detached_component(
        document_id,
        SidebarRow::new("返回项目").state(SidebarRowState::Idle),
    )?;
    bind_activate(
        context,
        automations_back,
        Arc::clone(&sink),
        ShellIntent::CloseAutomations,
    )?;
    let automations_body =
        context.create_detached_component(document_id, HostStack::fill_column(4.0))?;
    let automations_section = context.create_detached_component(
        document_id,
        SidebarSection::new("自动化").count(snapshot.automations.len()),
    )?;
    let automations_footer =
        context.create_detached_component(document_id, SidebarFooter::new())?;
    let automations_refresh = context.create_detached_component(
        document_id,
        SidebarFooterButton::new("刷新", Icon::Workspace),
    )?;
    let automations_new = context
        .create_detached_component(document_id, SidebarFooterButton::new("新建", Icon::Add))?;
    bind_activate(
        context,
        automations_refresh,
        Arc::clone(&sink),
        ShellIntent::RefreshAutomations,
    )?;
    bind_activate(
        context,
        automations_new,
        Arc::clone(&sink),
        ShellIntent::CreateAutomation,
    )?;
    context.append_child(automations_footer, automations_refresh)?;
    context.append_child(automations_footer, automations_new)?;
    context.append_child(automations_section, automations_body)?;
    let automations_sidebar = context.create_detached_component(
        document_id,
        SidebarFrame::new()
            .top(automations_back.stable_id())
            .body(automations_section.stable_id())
            .footer(automations_footer.stable_id()),
    )?;
    context.append_child(automations_sidebar, automations_back)?;
    context.append_child(automations_sidebar, automations_section)?;
    context.append_child(automations_sidebar, automations_footer)?;

    let project_page = context
        .create_detached_component(document_id, HostStack::fill_column(12.0).padding(16.0))?;
    let project_page_title =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let project_page_body =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let architecture_canvas = context.create_detached_component(
        document_id,
        GraphCanvas::new("architecture", snapshot.architecture_graph.clone())
            .viewport(snapshot.architecture_viewport.clone())
            .selection(snapshot.architecture_selection.clone()),
    )?;
    let architecture_sink = Arc::clone(&sink);
    context.on(
        architecture_canvas,
        move |_, event: &nana_ui::GraphCanvasEvent, _| {
            emit(
                &architecture_sink,
                ShellIntent::ArchitectureGraph(event.clone()),
            );
        },
    )?;
    context.append_child(project_page, project_page_title)?;
    context.append_child(project_page, project_page_body)?;
    context.append_child(project_page, architecture_canvas)?;

    let navigation = if snapshot.settings_open {
        settings_sidebar.stable_id()
    } else if snapshot.automations_open {
        automations_sidebar.stable_id()
    } else {
        conversation_sidebar.stable_id()
    };
    let primary = primary_content_id(
        snapshot,
        conversation,
        settings_page,
        workspace_page,
        automations_page,
        project_page,
    );
    let mut shell_builder = DesktopShell::from_model(snapshot.workspace.clone())
        .title(snapshot.title_context.clone())
        .title_leading(title_leading.stable_id())
        .title_center(title_center.stable_id())
        .title_center_width(TITLE_BREADCRUMB_WIDTH)
        // The trailing slot already owns bound window controls; the shell's own
        // strip would only add a second, inert set.
        .title_window_controls(false)
        .title_trailing(title_trailing.stable_id())
        .navigation(navigation)
        .primary(primary);
    if !snapshot.inspector_title.is_empty() {
        shell_builder = shell_builder.inspector(inspector.stable_id());
    }
    let shell = context.create_component(document_id, shell_builder)?;
    context.assemble_settings_sidebar(settings_sidebar)?;
    context.assemble_appearance_section(appearance)?;
    context.assemble_about_section(about)?;
    context.assemble_settings_page(settings_page)?;
    context.assemble_desktop_shell(shell)?;

    let overlay_host = context
        .read(shell, |shell| {
            shell.overlay.map(Entity::<OverlayHost>::from_stable_id)
        })
        .ok()
        .flatten();

    let mut handles = ShellHandles {
        sink,
        shell,
        overlay_host,
        palette: None,
        more_menu: None,
        titlebar_menu: None,
        sidebar_toggle,
        footer_more: more,
        form_fields: HashMap::new(),
        form_wrappers: HashMap::new(),
        form_switches: HashMap::new(),
        settings_card,
        quota_chart: None,
        pane_bar,
        pane_buttons: HashMap::new(),
        automations_page,
        automation_list,
        automation_actions,
        automation_canvas,
        title_center,
        title_parent,
        title_separator,
        title_context,
        title_leading,
        title_trailing,
        conversation_sidebar,
        automations_sidebar,
        automations_back,
        automations_section,
        automations_body,
        automations_footer,
        sidebar_top,
        new_conversation,
        search_toggle,
        search_input,
        search_close,
        sidebar_scroll: scroll,
        conversation_section: section,
        task_body,
        project_section,
        project_header,
        project_body,
        add_project_menu,
        inbox_section,
        inbox_body,
        task_rows: HashMap::new(),
        row_kinds: HashMap::new(),
        row_tools: HashMap::new(),
        row_tool_buttons: HashMap::new(),
        footer_nav,
        provider_badge,
        conversation,
        conversation_column,
        conversation_body,
        settings_sidebar,
        settings_page,
        appearance,
        about,
        product_settings,
        product_heading,
        product_body,
        product_error,
        project_name,
        project_name_field,
        project_workspace,
        project_workspace_row,
        product_actions: HashMap::new(),
        provider_rows: HashMap::new(),
        heading_slot,
        heading,
        error,
        timeline_scroll,
        timeline_items: HashMap::new(),
        timeline_markdown: HashMap::new(),
        timeline_markdown_source: HashMap::new(),
        timeline_actions: HashMap::new(),
        synced: SyncedInputs::default(),
        composer_generation: ComposerGeneration::default(),
        shell_assembled: false,
        load_earlier: None,
        composer,
        composer_toolbar,
        extras,
        extra_buttons: HashMap::new(),
        plus_items: HashMap::new(),
        completion_slot,
        completion_items: HashMap::new(),
        plus_slot,
        plus_menu,
        attach,
        permission_slot,
        permission_icon,
        permission,
        worktree_slot,
        worktree_icon,
        worktree,
        worktree_pick,
        pending_panel,
        pending_title,
        pending_prompt,
        pending_draft,
        pending_tool_command,
        pending_tool_message,
        pending_request,
        pending_tool_command_value,
        pending_tool_message_value,
        composer_actions: actions,
        send,
        interrupt: None,
        project_page,
        project_page_title,
        project_page_body,
        project_cards: HashMap::new(),
        architecture_canvas,
        automations_empty,
        workspace_page,
        pane_chrome,
        pane_header,
        pane_tabs,
        pane_body,
        workspace_content,
        workspace_heading,
        workspace_status,
        workspace_editor,
        workspace_log,
        workspace_input,
        diagnostics_panel,
        diagnostic_rows: HashMap::new(),
        image_viewer: None,
        workspace_actions,
        workspace_buttons: HashMap::new(),
        workspace_tree,
        inspector,
        inspector_heading,
        inspector_body,
        inspector_todos,
        inspector_todo_rows: HashMap::new(),
        coding_panel,
        coding_query,
        coding_rows: HashMap::new(),
        shortcut_capture,
        pane_move_window,
        pane_move_next,
        iab_url,
        iab_navigate,
        iab_open,
        confirm: None,
        confirm_cancel: None,
        confirm_commit: None,
        focus_targets: HashMap::new(),
    };
    handles.focus_targets.insert(
        target_ids::COMMAND_PALETTE_OPEN.to_owned(),
        more.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::SIDEBAR_NEW_CONVERSATION.to_owned(),
        new_conversation.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::SIDEBAR_SEARCH_TOGGLE.to_owned(),
        search_toggle.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::SIDEBAR_SEARCH_INPUT.to_owned(),
        search_input.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::SIDEBAR_PROJECTS_OVERVIEW.to_owned(),
        project_header.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::SIDEBAR_PROJECTS_ADD.to_owned(),
        add_project_menu.stable_id(),
    );
    handles.focus_targets.insert(
        target_ids::COMPOSER_INPUT.to_owned(),
        composer.stable_id(),
    );
    handles.sync_lists(context, snapshot)?;
    handles.sync_settings_content(context, document_id, snapshot)?;
    handles.sync_workspace_page(context, document_id, snapshot)?;
    handles.sync_overlay(context, document_id, snapshot)?;
    Ok((document, handles))
}

fn active_pane_item_kind(snapshot: &PrimaryShellSnapshot) -> Option<&str> {
    snapshot
        .panes
        .iter()
        .find(|pane| pane.active)
        .or_else(|| snapshot.panes.first())
        .and_then(|pane| pane.items.iter().find(|item| item.selected))
        .map(|item| item.kind.as_str())
}

fn has_workspace_primary_content(snapshot: &PrimaryShellSnapshot) -> bool {
    matches!(
        active_pane_item_kind(snapshot),
        Some("document-editor" | "terminal" | "project-files")
    ) && (snapshot.document.is_some() || snapshot.terminal.is_some() || snapshot.files.is_some())
}

fn primary_content_id(
    snapshot: &PrimaryShellSnapshot,
    conversation: Entity<HostStack>,
    settings_page: Entity<SettingsPage>,
    workspace_page: Entity<HostStack>,
    automations_page: Entity<HostStack>,
    project_page: Entity<HostStack>,
) -> StableNodeId {
    if snapshot.settings_open {
        settings_page.stable_id()
    } else if snapshot.automations_open {
        automations_page.stable_id()
    } else if snapshot.project_page.is_some() {
        project_page.stable_id()
    } else if has_workspace_primary_content(snapshot) {
        workspace_page.stable_id()
    } else {
        conversation.stable_id()
    }
}

impl ShellHandles {
    pub fn sync(
        &mut self,
        document: &mut nana_ui::runtime::RuntimeDocument,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let context = document.context_mut();
        let _ = context.set_theme(snapshot.theme);
        context.update_component(self.sidebar_toggle, |button, _| {
            *button = sidebar_toggle_button(snapshot.sidebar_collapsed);
        })?;
        context.update_component(self.footer_more, |button, _| {
            *button =
                SidebarFooterButton::new("更多", Icon::Nodes).selected(snapshot.titlebar_menu_open);
        })?;
        context.update_component(self.title_parent, |title, _| {
            *title = breadcrumb_parent(&snapshot.title_parent);
        })?;
        context.update_component(self.title_separator, |title, _| {
            *title = breadcrumb_separator();
        })?;
        context.update_component(self.title_context, |title, _| {
            *title = breadcrumb_context(&snapshot.title_context);
        })?;
        context.update_component(self.search_toggle, |button, _| {
            *button = sidebar_search_toggle();
        })?;
        context.update_component(self.search_close, |button, _| {
            *button = sidebar_search_close();
        })?;
        context.update_component(self.search_input, |editor, _| {
            if editor.state.value != snapshot.sidebar_search_query {
                editor
                    .state
                    .replace_value(snapshot.sidebar_search_query.clone());
            }
        })?;
        context.update_component(self.provider_badge, |button, _| {
            *button = SidebarFooterButton::new(snapshot.provider_badge.clone(), Icon::Appearance);
        })?;
        context.update_component(self.heading, |heading, _| {
            *heading = empty_heading(snapshot.heading.clone());
        })?;
        let headline_active = !snapshot.heading.trim().is_empty();
        context.update_component(self.heading_slot, |slot, _| {
            *slot = HostStack::headline_slot(headline_active);
        })?;
        context.update_component(self.error, |error, _| {
            *error = Text::new(snapshot.error.clone().unwrap_or_default());
        })?;
        let composer_generation = ComposerGeneration::new(
            snapshot.composer_task_id.clone(),
            snapshot.composer_revision,
        );
        let write_composer = !composer_is_focused(context, self.composer)
            || self.composer_generation != composer_generation;
        context.update_component(self.composer, |composer, _| {
            if write_composer && composer.state.value != snapshot.composer {
                composer.state.replace_value(snapshot.composer.clone());
            }
            composer.placeholder = Arc::from(snapshot.composer_placeholder.as_str());
            composer.disabled = snapshot.composer_disabled;
            Arc::make_mut(&mut composer.style.layout).height = Some(LengthSpec::Px(
                snapshot
                    .composer_height
                    .clamp(COMPOSER_MIN_HEIGHT, COMPOSER_MAX_HEIGHT),
            ));
        })?;
        self.composer_generation = composer_generation;
        context.update_component(self.inspector_heading, |text, _| {
            *text = Text::new(snapshot.inspector_title.clone());
        })?;
        context.update_component(self.inspector_body, |text, _| {
            *text = Text::new(snapshot.inspector_body.clone());
        })?;
        let document_id = context
            .world()
            .node(self.task_body.stable_id())
            .map(|node| node.document)
            .ok_or(FrameworkError::MissingView(self.task_body.stable_id()))?;
        self.sync_lists(context, snapshot)?;
        self.sync_settings_content(context, document_id, snapshot)?;
        let document_id = context
            .world()
            .node(self.workspace_page.stable_id())
            .map(|node| node.document)
            .ok_or(FrameworkError::MissingView(self.workspace_page.stable_id()))?;
        let workspace_inputs = WorkspaceInputs {
            panes: snapshot.panes.clone(),
            document: snapshot.document.clone(),
            terminal: snapshot.terminal.clone(),
            files: snapshot.files.clone(),
        };
        if self.synced.workspace.as_ref() != Some(&workspace_inputs) {
            self.sync_workspace_page(context, document_id, snapshot)?;
            self.sync_panes(context, document_id, snapshot)?;
            self.sync_diagnostics(context, document_id, snapshot)?;
            self.synced.workspace = Some(workspace_inputs);
        }
        self.sync_automations(context, document_id, snapshot)?;
        self.sync_composer_stage(context, document_id, snapshot)?;
        self.sync_project_page(context, document_id, snapshot)?;
        let inspector_inputs = InspectorInputs {
            kind: snapshot.inspector_kind.clone(),
            todos: snapshot.inspector_todos.clone(),
            iab_url: snapshot.iab_url.clone(),
            coding: snapshot.coding.clone(),
        };
        if self.synced.inspector.as_ref() != Some(&inspector_inputs) {
            self.sync_inspector_details(context, document_id, snapshot)?;
            self.synced.inspector = Some(inspector_inputs);
        }
        self.sync_overlay(context, document_id, snapshot)?;
        let navigation = if snapshot.settings_open {
            self.settings_sidebar.stable_id()
        } else if snapshot.automations_open {
            self.automations_sidebar.stable_id()
        } else {
            self.conversation_sidebar.stable_id()
        };
        let primary = primary_content_id(
            snapshot,
            self.conversation,
            self.settings_page,
            self.workspace_page,
            self.automations_page,
            self.project_page,
        );
        let inspector = (!snapshot.inspector_title.is_empty()).then(|| self.inspector.stable_id());
        let bottom = snapshot
            .document
            .as_ref()
            .is_some_and(|document| !document.diagnostics.is_empty())
            .then(|| self.diagnostics_panel.stable_id());
        let mut shell_changed = !self.shell_assembled;
        context.update_component(self.shell, |shell, _| {
            shell_changed = shell_changed
                || shell.model != snapshot.workspace
                || shell.title.as_deref() != Some(snapshot.title_context.as_str())
                || shell.navigation != Some(navigation)
                || shell.primary != Some(primary)
                || shell.inspector != inspector
                || shell.bottom != bottom;
            shell.model = snapshot.workspace.clone();
            shell.title = Some(Arc::from(snapshot.title_context.as_str()));
            shell.title_leading = Some(self.title_leading.stable_id());
            shell.title_center = Some(self.title_center.stable_id());
            shell.title_trailing = Some(self.title_trailing.stable_id());
            shell.navigation = Some(navigation);
            shell.primary = Some(primary);
            shell.inspector = inspector;
            shell.bottom = bottom;
        })?;
        if shell_changed {
            context.assemble_desktop_shell(self.shell)?;
            self.shell_assembled = true;
        }
        self.overlay_host = context
            .read(self.shell, |shell| {
                shell.overlay.map(Entity::<OverlayHost>::from_stable_id)
            })
            .ok()
            .flatten();
        Ok(())
    }

    pub fn apply_ui_commands(
        &mut self,
        document: &mut nana_ui::runtime::RuntimeDocument,
        window_id: HostedWindowId,
        commands: impl IntoIterator<Item = HostedUiCommand>,
    ) -> Result<(), FrameworkError> {
        let document_id = document.document();
        let context = document.context_mut();
        for command in commands {
            match command {
                HostedUiCommand::Focus {
                    window_id: target_window,
                    target,
                } if target_window == window_id => {
                    if let Some(node) = self.focus_targets.get(&target).copied() {
                        let _ = context.focus_node(document_id, node)?;
                    } else if target == target_ids::COMMAND_PALETTE_INPUT {
                        if let Some(palette) = self.palette {
                            let _ = context.focus_node(document_id, palette.stable_id())?;
                        }
                    }
                }
                HostedUiCommand::ScrollBy {
                    window_id: target_window,
                    target,
                    x,
                    y,
                } if target_window == window_id => {
                    if target.contains("timeline") {
                        let _ = context.scroll_by(self.timeline_scroll, ScrollOffset { x, y })?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn sync_lists(
        &mut self,
        context: &mut AppContext,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let document_id = context
            .world()
            .node(self.task_body.stable_id())
            .map(|node| node.document)
            .ok_or(FrameworkError::MissingView(self.task_body.stable_id()))?;
        if self.synced.sidebar_rows != snapshot.sidebar_rows
            || self.synced.sidebar_tasks != snapshot.tasks
            || self.synced.sidebar_search_open != snapshot.sidebar_search_open
        {
            let groups = partition_sidebar_rows(snapshot);
            self.reconcile_task_rows(context, document_id, &groups)?;
            self.sync_sidebar_sections(context, snapshot, &groups)?;
            self.synced.sidebar_rows = snapshot.sidebar_rows.clone();
            self.synced.sidebar_tasks = snapshot.tasks.clone();
            self.synced.sidebar_search_open = snapshot.sidebar_search_open;
        }
        self.sync_sidebar_chrome(context, snapshot)?;
        if self.synced.timeline != snapshot.timeline {
            self.reconcile_timeline(context, document_id, snapshot)?;
            self.synced.timeline = snapshot.timeline.clone();
        }
        Ok(())
    }

    fn sync_sidebar_sections(
        &mut self,
        context: &mut AppContext,
        snapshot: &PrimaryShellSnapshot,
        groups: &SidebarRowGroups,
    ) -> Result<(), FrameworkError> {
        context.update_component(self.conversation_section, |section, _| {
            section.title = Arc::from(if snapshot.sidebar_search_open {
                "搜索"
            } else {
                "会话"
            });
            section.count = Some(groups.sessions.len());
            section.empty_text = (!snapshot.sidebar_search_open && groups.sessions.is_empty())
                .then(|| Arc::from(SESSIONS_EMPTY_TEXT));
        })?;
        context.update_component(self.project_section, |section, _| {
            section.title = Arc::from("项目");
            section.count = Some(sidebar_project_entry_count(&groups.projects));
            section.empty_text = groups
                .projects
                .is_empty()
                .then(|| Arc::from(PROJECTS_EMPTY_TEXT));
        })?;
        context.update_component(self.inbox_section, |section, _| {
            section.title = Arc::from("收集箱");
            section.count = Some(groups.inbox.len());
            section.empty_text = groups.inbox.is_empty().then(|| Arc::from(INBOX_EMPTY_TEXT));
            section.collapsible = true;
            section.state = SidebarSectionState::new(groups.inbox_expanded);
        })?;
        let sections = if snapshot.sidebar_search_open {
            vec![self.conversation_section.stable_id()]
        } else if groups.grouped {
            vec![
                self.project_section.stable_id(),
                self.inbox_section.stable_id(),
            ]
        } else {
            vec![self.conversation_section.stable_id()]
        };
        reconcile_children(context, self.sidebar_scroll.stable_id(), &sections)
    }

    fn sync_sidebar_chrome(
        &mut self,
        context: &mut AppContext,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let document_id = context
            .world()
            .node(self.sidebar_top.stable_id())
            .map(|node| node.document)
            .ok_or(FrameworkError::MissingView(self.sidebar_top.stable_id()))?;
        let top = if snapshot.sidebar_search_open {
            vec![self.search_input.stable_id(), self.search_close.stable_id()]
        } else {
            vec![
                self.new_conversation.stable_id(),
                self.search_toggle.stable_id(),
            ]
        };
        reconcile_children(context, self.sidebar_top.stable_id(), &top)?;
        let mut footer = Vec::new();
        let mut keep = HashSet::new();
        for item in &snapshot.nav_items {
            keep.insert(item.id.clone());
            let button = if let Some(button) = self.footer_nav.get(&item.id).copied() {
                context.update_component(button, |button, _| {
                    *button = SidebarFooterButton::new(item.label.clone(), nav_icon(item.settings))
                        .selected(item.selected);
                })?;
                button
            } else {
                let button = context.create_detached_component(
                    document_id,
                    SidebarFooterButton::new(item.label.clone(), nav_icon(item.settings))
                        .selected(item.selected),
                )?;
                bind_activate(
                    context,
                    button,
                    Arc::clone(&self.sink),
                    if item.settings {
                        ShellIntent::OpenSettings
                    } else {
                        ShellIntent::OpenAutomations
                    },
                )?;
                self.footer_nav.insert(item.id.clone(), button);
                button
            };
            footer.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .footer_nav
            .keys()
            .filter(|id| !keep.contains(*id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(button) = self.footer_nav.remove(&id) {
                let _ = context.remove_view(button);
            }
        }
        footer.push(self.provider_badge.stable_id());
        let footer_id = context
            .world()
            .node(self.provider_badge.stable_id())
            .and_then(|node| node.parent)
            .ok_or(FrameworkError::MissingView(self.provider_badge.stable_id()))?;
        reconcile_children(context, footer_id, &footer)
    }

    fn sync_row_tools(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        item: &ShellSidebarRow,
        row: Entity<SidebarRow>,
    ) -> Result<(), FrameworkError> {
        let mut tools = Vec::new();
        if item.can_stop {
            if let Some(task_id) = TaskId::new(&item.id).ok() {
                let id = format!("{}-stop", item.id);
                let button = if let Some(button) = self.row_tool_buttons.get(&id).copied() {
                    button
                } else {
                    let button =
                        context.create_detached_component(document_id, row_stop_button())?;
                    bind_activate(
                        context,
                        button,
                        Arc::clone(&self.sink),
                        ShellIntent::StopSidebarTask(task_id),
                    )?;
                    self.row_tool_buttons.insert(id, button);
                    button
                };
                tools.push(button.stable_id());
            }
        }
        if item.can_draft {
            let id = format!("{}-draft", item.id);
            let button = if let Some(button) = self.row_tool_buttons.get(&id).copied() {
                button
            } else {
                let button = context.create_detached_component(document_id, row_draft_button())?;
                bind_activate(
                    context,
                    button,
                    Arc::clone(&self.sink),
                    ShellIntent::OpenProjectDraft(item.id.clone()),
                )?;
                self.row_tool_buttons.insert(id, button);
                button
            };
            tools.push(button.stable_id());
        }
        if item.can_menu {
            let id = format!("{}-menu", item.id);
            let button = if let Some(button) = self.row_tool_buttons.get(&id).copied() {
                button
            } else {
                let button = context.create_detached_component(document_id, row_menu_button())?;
                let intent = match item.kind {
                    ShellSidebarKind::Task
                    | ShellSidebarKind::SearchTask
                    | ShellSidebarKind::Running => ShellIntent::OpenTaskMenu(item.id.clone()),
                    _ => ShellIntent::OpenProjectMenu(item.id.clone()),
                };
                bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                self.row_tool_buttons.insert(id, button);
                button
            };
            tools.push(button.stable_id());
        }
        if tools.is_empty() {
            return self.clear_row_tools(context, &item.id, Some(row));
        }
        let host = if let Some(host) = self.row_tools.get(&item.id).copied() {
            host
        } else {
            let host = context.create_detached_component(document_id, HostStack::row(2.0))?;
            context.update_component(row, |row, _| {
                row.tools = Some(host.stable_id());
            })?;
            context.append_child(row, host)?;
            self.row_tools.insert(item.id.clone(), host);
            host
        };
        for tool in &tools {
            if context.world().node(*tool).and_then(|node| node.parent) != Some(host.stable_id()) {
                context.append_child(host, Entity::<Button>::from_stable_id(*tool))?;
            }
        }
        reconcile_children(context, host.stable_id(), &tools)
    }

    fn sync_sidebar_row_group(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        items: &[ShellSidebarRow],
        keep: &mut HashSet<String>,
    ) -> Result<Vec<StableNodeId>, FrameworkError> {
        let mut order = Vec::new();
        for item in items {
            keep.insert(item.id.clone());
            if self
                .row_kinds
                .get(&item.id)
                .is_some_and(|kind| *kind != item.kind)
            {
                self.remove_sidebar_row(context, &item.id);
            }
            let state = if item.selected {
                SidebarRowState::Active
            } else if item.ancestor {
                SidebarRowState::AncestorActive
            } else {
                SidebarRowState::Idle
            };
            let row = if let Some(row) = self.task_rows.get(&item.id).copied() {
                context.update_component(row, |row, _| {
                    row.label = Arc::from(item.label.as_str());
                    row.state = state;
                    row.depth = item.depth;
                    row.disclosure = item.expanded;
                })?;
                row
            } else {
                // Nested session rows read as children of their project through
                // indentation alone; a glyph there only competes with the label.
                let leading =
                    if item.depth == 0 {
                        Some(context.create_detached_component(
                            document_id,
                            SidebarRowIcon::new(item.icon),
                        )?)
                    } else {
                        None
                    };
                let mut row_view = SidebarRow::new(item.label.clone())
                    .state(state)
                    .depth(item.depth)
                    .slots(nana_ui::runtime::ListItemSlots {
                        leading: leading.map(|leading| leading.stable_id()),
                        content: None,
                        trailing: None,
                    });
                row_view.style = sidebar_row_style();
                if let Some(expanded) = item.expanded {
                    row_view = row_view.disclosure(expanded);
                }
                let row = context.create_detached_component(document_id, row_view)?;
                if let Some(leading) = leading {
                    context.append_child(row, leading)?;
                }
                if let Some(intent) = sidebar_row_intent(item) {
                    bind_activate(context, row, Arc::clone(&self.sink), intent)?;
                }
                self.task_rows.insert(item.id.clone(), row);
                self.row_kinds.insert(item.id.clone(), item.kind);
                row
            };
            self.sync_row_tools(context, document_id, item, row)?;
            order.push(row.stable_id());
        }
        Ok(order)
    }

    fn clear_row_tools(
        &mut self,
        context: &mut AppContext,
        id: &str,
        row: Option<Entity<SidebarRow>>,
    ) -> Result<(), FrameworkError> {
        if let Some(row) = row {
            context.update_component(row, |row, _| {
                row.tools = None;
            })?;
        }
        if let Some(host) = self.row_tools.remove(id) {
            let _ = context.remove_view(host);
        }
        for suffix in ["stop", "draft", "menu"] {
            if let Some(button) = self.row_tool_buttons.remove(&format!("{id}-{suffix}")) {
                let _ = context.remove_view(button);
            }
        }
        Ok(())
    }

    fn remove_sidebar_row(&mut self, context: &mut AppContext, id: &str) {
        self.row_kinds.remove(id);
        let row = self.task_rows.remove(id);
        let _ = self.clear_row_tools(context, id, row);
        if let Some(row) = row {
            let _ = context.remove_view(row);
        }
    }

    fn reconcile_task_rows(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        groups: &SidebarRowGroups,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let session_order =
            self.sync_sidebar_row_group(context, document_id, &groups.sessions, &mut keep)?;
        let project_order =
            self.sync_sidebar_row_group(context, document_id, &groups.projects, &mut keep)?;
        let inbox_order =
            self.sync_sidebar_row_group(context, document_id, &groups.inbox, &mut keep)?;
        let stale: Vec<_> = self
            .task_rows
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            self.remove_sidebar_row(context, &key);
        }
        reconcile_children(context, self.task_body.stable_id(), &session_order)?;
        reconcile_children(context, self.project_body.stable_id(), &project_order)?;
        reconcile_children(context, self.inbox_body.stable_id(), &inbox_order)
    }

    fn sync_composer_actions(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut order = Vec::new();
        if snapshot.can_interrupt && !snapshot.can_send {
            let interrupt = if let Some(interrupt) = self.interrupt {
                context.update_component(interrupt, |button, _| {
                    *button = composer_interrupt_button(true);
                })?;
                interrupt
            } else {
                let interrupt = context
                    .create_detached_component(document_id, composer_interrupt_button(true))?;
                bind_activate(
                    context,
                    interrupt,
                    Arc::clone(&self.sink),
                    ShellIntent::InterruptTurn,
                )?;
                self.interrupt = Some(interrupt);
                interrupt
            };
            order.push(interrupt.stable_id());
        } else if let Some(interrupt) = self.interrupt.take() {
            let _ = context.remove_view(interrupt);
            order.push(self.send.stable_id());
        } else {
            order.push(self.send.stable_id());
        }
        context.update_component(self.send, |button, _| {
            *button = composer_send_button(snapshot.can_send && !snapshot.pending_blocks_send);
        })?;
        reconcile_children(context, self.composer_actions.stable_id(), &order)
    }

    fn sync_composer_stage(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        context.update_component(self.plus_menu, |menu, _| {
            *menu = composer_plus_menu(snapshot.composer_plus_open);
        })?;
        context.update_component(self.permission, |chip, _| {
            *chip = pill_button(&snapshot.permission_label, ButtonKind::Text);
        })?;
        let plus_order = if snapshot.composer_plus_open {
            let mut plus_order = Vec::new();
            let mut plus_keep = HashSet::new();
            for (id, label) in plus_menu_items(snapshot) {
                plus_keep.insert(id.clone());
                let item = if let Some(item) = self.plus_items.get(&id).copied() {
                    context.update_component(item, |item, _| {
                        *item = ActionMenuItem::new(label);
                    })?;
                    item
                } else {
                    let item = context
                        .create_detached_component(document_id, ActionMenuItem::new(label))?;
                    bind_activate(
                        context,
                        item,
                        Arc::clone(&self.sink),
                        ShellIntent::ComposerPlus(id.clone()),
                    )?;
                    self.plus_items.insert(id.clone(), item);
                    item
                };
                plus_order.push(item.stable_id());
            }
            self.plus_items.retain(|key, item| {
                if plus_keep.contains(key) {
                    true
                } else {
                    let _ = context.remove_view(*item);
                    false
                }
            });
            plus_order
        } else {
            for item in self.plus_items.drain() {
                let _ = context.remove_view(item.1);
            }
            Vec::new()
        };
        reconcile_children(context, self.plus_menu.stable_id(), &plus_order)?;
        self.reconcile_composer_extras(context, document_id, snapshot)?;
        self.reconcile_composer_completion(context, document_id, snapshot)?;
        self.sync_pending_panel(context, document_id, snapshot)?;
        self.sync_composer_actions(context, document_id, snapshot)?;

        let dock = context
            .world()
            .node(self.composer.stable_id())
            .and_then(|node| node.parent)
            .unwrap_or(self.composer.stable_id());
        let mut stage = Vec::new();
        if snapshot.pending.is_some() {
            stage.push(self.pending_panel.stable_id());
        }
        if !self.completion_items.is_empty() {
            stage.push(self.completion_slot.stable_id());
        }
        stage.push(self.composer.stable_id());
        stage.push(self.composer_toolbar.stable_id());
        reconcile_children(context, dock, &stage)?;
        reconcile_children(
            context,
            self.composer_toolbar.stable_id(),
            &[self.extras.stable_id(), self.composer_actions.stable_id()],
        )
    }

    fn sync_pending_panel(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let Some(pending) = &snapshot.pending else {
            if let Ok(mut guard) = self.pending_request.lock() {
                guard.clear();
            }
            reconcile_children(context, self.pending_panel.stable_id(), &[])?;
            return Ok(());
        };
        if let Ok(mut guard) = self.pending_request.lock() {
            *guard = pending.request_id.clone();
        }
        context.update_component(self.pending_title, |text, _| {
            *text = Text::new(pending.title.clone());
        })?;
        context.update_component(self.pending_prompt, |text, _| {
            *text = Text::new(pending.prompt.clone());
        })?;
        context.update_component(self.pending_draft, |editor, _| {
            editor.placeholder = Arc::from("补充说明");
            let value = pending
                .ask
                .as_ref()
                .map(|ask| ask.freeform.clone())
                .unwrap_or_else(|| pending.draft.clone());
            if editor.state.value != value {
                editor.state.replace_value(value);
            }
        })?;
        let mut keep = HashSet::new();
        let mut field_keep = HashSet::new();
        let mut order = vec![
            self.pending_title.stable_id(),
            self.pending_prompt.stable_id(),
        ];
        let request_id = pending.request_id.clone();
        match pending.kind {
            ShellPendingKind::PlanApproval => {
                order.push(self.pending_draft.stable_id());
            }
            ShellPendingKind::ToolConsent => {
                if let Some(tool) = &pending.tool {
                    if let Ok(mut guard) = self.pending_tool_command_value.lock() {
                        *guard = tool.command.clone();
                    }
                    if let Ok(mut guard) = self.pending_tool_message_value.lock() {
                        *guard = tool.message.clone();
                    }
                    if tool.command_editable {
                        context.update_component(self.pending_tool_command, |editor, _| {
                            editor.placeholder = Arc::from("确认执行的命令");
                            if editor.state.value != tool.command {
                                editor.state.replace_value(tool.command.clone());
                            }
                        })?;
                        order.push(self.pending_tool_command.stable_id());
                    }
                    context.update_component(self.pending_tool_message, |editor, _| {
                        editor.placeholder = Arc::from("拒绝理由");
                        if editor.state.value != tool.message {
                            editor.state.replace_value(tool.message.clone());
                        }
                    })?;
                    order.push(self.pending_tool_message.stable_id());
                }
            }
            ShellPendingKind::AskUser => {
                if pending.ask.as_ref().is_some_and(|ask| ask.show_freeform) {
                    order.push(self.pending_draft.stable_id());
                }
            }
            ShellPendingKind::McpElicitation => {
                if let Some(mcp) = &pending.mcp {
                    if let Some(url) = &mcp.url {
                        let open = self.upsert_tagged_button(
                            context,
                            document_id,
                            &format!("pending-mcp-url-{request_id}"),
                            "打开链接",
                            ButtonKind::Subtle,
                            ShellIntent::OpenMarkdownLink(url.clone()),
                            false,
                        )?;
                        keep.insert(format!("pending-mcp-url-{request_id}"));
                        order.push(open.stable_id());
                    }
                    if let Some(raw) = &mcp.raw_json {
                        self.upsert_field(
                            context,
                            document_id,
                            &mut field_keep,
                            &mut order,
                            &format!("pending-mcp-raw-{request_id}"),
                            raw,
                            {
                                let request_id = request_id.clone();
                                move |value| ShellIntent::McpRawJsonChanged {
                                    request_id: request_id.clone(),
                                    value,
                                }
                            },
                        )?;
                    }
                    for field in &mcp.fields {
                        if field.options.is_empty() && field.kind != "boolean" {
                            self.upsert_field(
                                context,
                                document_id,
                                &mut field_keep,
                                &mut order,
                                &format!("pending-mcp-field-{request_id}-{}", field.key),
                                &field.value,
                                {
                                    let request_id = request_id.clone();
                                    let field_key = field.key.clone();
                                    move |value| ShellIntent::McpFieldChanged {
                                        request_id: request_id.clone(),
                                        field_key: field_key.clone(),
                                        value,
                                    }
                                },
                            )?;
                        } else if field.kind == "boolean" {
                            let id = format!("pending-mcp-bool-{request_id}-{}", field.key);
                            let button = self.upsert_tagged_button(
                                context,
                                document_id,
                                &id,
                                if field.enabled {
                                    "已开启"
                                } else {
                                    "已关闭"
                                },
                                if field.enabled {
                                    ButtonKind::Primary
                                } else {
                                    ButtonKind::Subtle
                                },
                                ShellIntent::McpToggleBoolean {
                                    request_id: request_id.clone(),
                                    field_key: field.key.clone(),
                                },
                                false,
                            )?;
                            keep.insert(id);
                            order.push(button.stable_id());
                        } else {
                            for option in &field.options {
                                let id = format!(
                                    "pending-mcp-opt-{request_id}-{}-{}",
                                    field.key, option.value
                                );
                                let button = self.upsert_tagged_button(
                                    context,
                                    document_id,
                                    &id,
                                    &option.label,
                                    if option.selected {
                                        ButtonKind::Primary
                                    } else {
                                        ButtonKind::Subtle
                                    },
                                    ShellIntent::McpToggleOption {
                                        request_id: request_id.clone(),
                                        field_key: field.key.clone(),
                                        value: option.value.clone(),
                                        multi: option.multi,
                                    },
                                    false,
                                )?;
                                keep.insert(id);
                                order.push(button.stable_id());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        for option in &pending.options {
            let id = format!("pending-opt-{}-{}", request_id, option.id);
            let button = self.upsert_tagged_button(
                context,
                document_id,
                &id,
                &option.label,
                if option.selected {
                    ButtonKind::Primary
                } else if option.danger {
                    ButtonKind::Danger
                } else {
                    ButtonKind::Subtle
                },
                ShellIntent::SelectPendingOption {
                    request_id: request_id.clone(),
                    option_id: option.id.clone(),
                },
                false,
            )?;
            keep.insert(id);
            order.push(button.stable_id());
        }
        if let Some(ask) = &pending.ask {
            if ask.show_other {
                let id = format!("pending-ask-other-{request_id}");
                let button = self.upsert_tagged_button(
                    context,
                    document_id,
                    &id,
                    if ask.other_selected {
                        "✓ 其他"
                    } else {
                        "其他"
                    },
                    if ask.other_selected {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Subtle
                    },
                    ShellIntent::AskUserPending {
                        request_id: request_id.clone(),
                        action: "select".to_owned(),
                        value: "other".to_owned(),
                    },
                    false,
                )?;
                keep.insert(id);
                order.push(button.stable_id());
            }
        }
        let actions: Vec<(String, String, ButtonKind, ShellIntent, bool)> = match pending.kind {
            ShellPendingKind::PermissionApproval => vec![
                (
                    format!("pending-approve-{request_id}"),
                    "允许".to_owned(),
                    ButtonKind::Primary,
                    ShellIntent::RespondApproval {
                        request_id: request_id.clone(),
                        approved: true,
                    },
                    false,
                ),
                (
                    format!("pending-reject-{request_id}"),
                    "拒绝".to_owned(),
                    ButtonKind::Danger,
                    ShellIntent::RespondApproval {
                        request_id: request_id.clone(),
                        approved: false,
                    },
                    false,
                ),
            ],
            ShellPendingKind::PlanApproval => vec![
                (
                    format!("pending-plan-approve-{request_id}"),
                    "执行计划".to_owned(),
                    ButtonKind::Primary,
                    ShellIntent::RespondPlan {
                        request_id: request_id.clone(),
                        action: "approve".to_owned(),
                    },
                    false,
                ),
                (
                    format!("pending-plan-revise-{request_id}"),
                    "要求修改".to_owned(),
                    ButtonKind::Subtle,
                    ShellIntent::RespondPlan {
                        request_id: request_id.clone(),
                        action: "revise".to_owned(),
                    },
                    pending.draft.trim().is_empty(),
                ),
                (
                    format!("pending-plan-decline-{request_id}"),
                    "拒绝".to_owned(),
                    ButtonKind::Danger,
                    ShellIntent::RespondPlan {
                        request_id: request_id.clone(),
                        action: "decline".to_owned(),
                    },
                    false,
                ),
                (
                    format!("pending-plan-interrupt-{request_id}"),
                    "取消任务".to_owned(),
                    ButtonKind::Subtle,
                    ShellIntent::InterruptTurn,
                    false,
                ),
            ],
            ShellPendingKind::ToolConsent => {
                let tool = pending.tool.as_ref();
                vec![
                    (
                        format!("pending-consent-allow-{request_id}"),
                        "允许".to_owned(),
                        ButtonKind::Primary,
                        ShellIntent::RespondToolConsent {
                            request_id: request_id.clone(),
                            approved: true,
                        },
                        tool.is_some_and(|tool| !tool.can_allow),
                    ),
                    (
                        format!("pending-consent-deny-{request_id}"),
                        "拒绝".to_owned(),
                        ButtonKind::Danger,
                        ShellIntent::RespondToolConsent {
                            request_id: request_id.clone(),
                            approved: false,
                        },
                        tool.is_some_and(|tool| !tool.can_deny),
                    ),
                ]
            }
            ShellPendingKind::AskUser => {
                let ask = pending.ask.as_ref();
                let mut actions = Vec::new();
                if ask.is_some_and(|ask| ask.show_skip) {
                    actions.push((
                        format!("pending-ask-skip-{request_id}"),
                        "跳过".to_owned(),
                        ButtonKind::Subtle,
                        ShellIntent::AskUserPending {
                            request_id: request_id.clone(),
                            action: "skip".to_owned(),
                            value: String::new(),
                        },
                        false,
                    ));
                }
                if ask.is_some_and(|ask| ask.show_back) {
                    actions.push((
                        format!("pending-ask-back-{request_id}"),
                        "上一题".to_owned(),
                        ButtonKind::Subtle,
                        ShellIntent::AskUserPending {
                            request_id: request_id.clone(),
                            action: "back".to_owned(),
                            value: String::new(),
                        },
                        false,
                    ));
                }
                if ask.is_some_and(|ask| ask.show_cancel) {
                    actions.push((
                        format!("pending-ask-cancel-{request_id}"),
                        "关闭".to_owned(),
                        ButtonKind::Subtle,
                        ShellIntent::AskUserPending {
                            request_id: request_id.clone(),
                            action: "cancel".to_owned(),
                            value: String::new(),
                        },
                        false,
                    ));
                }
                if ask.is_some_and(|ask| ask.show_reject) {
                    actions.push((
                        format!("pending-ask-reject-{request_id}"),
                        ask.map(|ask| ask.reject_label.clone())
                            .unwrap_or_else(|| "不要".to_owned()),
                        ButtonKind::Subtle,
                        ShellIntent::AskUserPending {
                            request_id: request_id.clone(),
                            action: "reject".to_owned(),
                            value: String::new(),
                        },
                        false,
                    ));
                }
                actions.push((
                    format!("pending-ask-submit-{request_id}"),
                    ask.map(|ask| ask.submit_label.clone())
                        .unwrap_or_else(|| "提交".to_owned()),
                    ButtonKind::Primary,
                    ShellIntent::AskUserPending {
                        request_id: request_id.clone(),
                        action: "submit".to_owned(),
                        value: String::new(),
                    },
                    ask.is_some_and(|ask| !ask.can_submit),
                ));
                actions
            }
            ShellPendingKind::ArchitectureChange => vec![
                (
                    format!("pending-arch-allow-{request_id}"),
                    "允许".to_owned(),
                    ButtonKind::Primary,
                    ShellIntent::RespondArchitecture {
                        request_id: request_id.clone(),
                        approved: true,
                    },
                    false,
                ),
                (
                    format!("pending-arch-deny-{request_id}"),
                    "拒绝".to_owned(),
                    ButtonKind::Danger,
                    ShellIntent::RespondArchitecture {
                        request_id: request_id.clone(),
                        approved: false,
                    },
                    false,
                ),
            ],
            ShellPendingKind::TitleUpdate => vec![
                (
                    format!("pending-title-accept-{request_id}"),
                    "采用".to_owned(),
                    ButtonKind::Primary,
                    ShellIntent::RespondTitle {
                        request_id: request_id.clone(),
                        accepted: true,
                    },
                    false,
                ),
                (
                    format!("pending-title-reject-{request_id}"),
                    "拒绝".to_owned(),
                    ButtonKind::Danger,
                    ShellIntent::RespondTitle {
                        request_id: request_id.clone(),
                        accepted: false,
                    },
                    false,
                ),
            ],
            ShellPendingKind::McpElicitation => vec![
                (
                    format!("pending-mcp-accept-{request_id}"),
                    "接受".to_owned(),
                    ButtonKind::Primary,
                    ShellIntent::RespondMcp {
                        request_id: request_id.clone(),
                        action: "accept".to_owned(),
                    },
                    pending.mcp.as_ref().is_some_and(|mcp| !mcp.can_accept),
                ),
                (
                    format!("pending-mcp-decline-{request_id}"),
                    "拒绝".to_owned(),
                    ButtonKind::Subtle,
                    ShellIntent::RespondMcp {
                        request_id: request_id.clone(),
                        action: "decline".to_owned(),
                    },
                    false,
                ),
                (
                    format!("pending-mcp-cancel-{request_id}"),
                    "取消".to_owned(),
                    ButtonKind::Subtle,
                    ShellIntent::RespondMcp {
                        request_id: request_id.clone(),
                        action: "cancel".to_owned(),
                    },
                    false,
                ),
            ],
        };
        for (id, label, kind, intent, disabled) in actions {
            keep.insert(id.clone());
            let button = self.upsert_tagged_button(
                context,
                document_id,
                &id,
                &label,
                kind,
                intent,
                disabled,
            )?;
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .extra_buttons
            .keys()
            .filter(|key| key.starts_with("pending-") && !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.extra_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        let stale_fields: Vec<_> = self
            .form_fields
            .keys()
            .filter(|key| key.starts_with("pending-") && !field_keep.contains(*key))
            .cloned()
            .collect();
        for key in stale_fields {
            if let Some(field) = self.form_fields.remove(&key) {
                let _ = context.remove_view(field);
            }
            if let Some(wrapper) = self.form_wrappers.remove(&key) {
                let _ = context.remove_view(wrapper);
            }
        }
        reconcile_children(context, self.pending_panel.stable_id(), &order)
    }

    fn upsert_tagged_button(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        id: &str,
        label: &str,
        kind: ButtonKind,
        intent: ShellIntent,
        disabled: bool,
    ) -> Result<Entity<Button>, FrameworkError> {
        if let Some(button) = self.extra_buttons.get(id).copied() {
            context.update_component(button, |button, _| {
                *button = extra_button(label, kind);
                button.disabled = disabled;
            })?;
            Ok(button)
        } else {
            let mut view = extra_button(label, kind);
            view.disabled = disabled;
            let button = context.create_detached_component(document_id, view)?;
            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
            self.extra_buttons.insert(id.to_owned(), button);
            Ok(button)
        }
    }

    fn sync_inspector_details(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        context.update_component(self.iab_url, |editor, _| {
            if editor.state.value != snapshot.iab_url {
                editor.state.replace_value(snapshot.iab_url.clone());
            }
        })?;
        let mut order = Vec::new();
        for todo in &snapshot.inspector_todos {
            let row = if let Some(row) = self.inspector_todo_rows.get(&todo.id).copied() {
                context.update_component(row, |text, _| {
                    *text = Text::new(format!(
                        "{} {}",
                        if todo.done { "✓" } else { "○" },
                        todo.label
                    ));
                })?;
                row
            } else {
                let row = context.create_detached_component(
                    document_id,
                    Text::new(format!(
                        "{} {}",
                        if todo.done { "✓" } else { "○" },
                        todo.label
                    )),
                )?;
                self.inspector_todo_rows.insert(todo.id.clone(), row);
                row
            };
            order.push(row.stable_id());
        }
        let stale: Vec<_> = self
            .inspector_todo_rows
            .keys()
            .filter(|id| snapshot.inspector_todos.iter().all(|todo| todo.id != **id))
            .cloned()
            .collect();
        for id in stale {
            if let Some(row) = self.inspector_todo_rows.remove(&id) {
                let _ = context.remove_view(row);
            }
        }
        reconcile_children(context, self.inspector_todos.stable_id(), &order)?;
        self.sync_coding_tools(context, document_id, snapshot)?;
        let mut inspector_order = vec![self.inspector_heading.stable_id()];
        match snapshot.inspector_kind.as_str() {
            "coding" => inspector_order.push(self.coding_panel.stable_id()),
            "iab" => {
                inspector_order.push(self.iab_url.stable_id());
                inspector_order.push(self.iab_navigate.stable_id());
                inspector_order.push(self.iab_open.stable_id());
            }
            _ => {
                inspector_order.push(self.inspector_body.stable_id());
                inspector_order.push(self.inspector_todos.stable_id());
            }
        }
        reconcile_children(context, self.inspector.stable_id(), &inspector_order)
    }

    fn sync_coding_tools(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let Some(coding) = &snapshot.coding else {
            reconcile_children(context, self.coding_panel.stable_id(), &[])?;
            return Ok(());
        };
        context.update_component(self.coding_query, |editor, _| {
            if editor.state.value != coding.query {
                editor.state.replace_value(coding.query.clone());
            }
        })?;
        let mut keep = HashSet::new();
        let mut order = vec![self.coding_query.stable_id()];
        for (id, label, intent) in [
            ("coding-search", "搜索", ShellIntent::SearchCoding),
            (
                "coding-refresh",
                if coding.busy { "处理中" } else { "刷新" },
                ShellIntent::RefreshCoding,
            ),
            (
                "coding-mode",
                coding.mode_label.as_str(),
                ShellIntent::CycleCodingMode,
            ),
            (
                "coding-scope",
                coding.scope_label.as_str(),
                ShellIntent::ToggleCodingScope,
            ),
            (
                "coding-files",
                "文件管理器",
                ShellIntent::OpenCodingWorkspace,
            ),
            (
                "coding-terminal",
                "工作区终端",
                ShellIntent::OpenCodingTerminal,
            ),
        ] {
            keep.insert(id.to_owned());
            let button = self.upsert_coding_button(context, document_id, id, label, intent)?;
            order.push(button.stable_id());
        }
        if !coding.git.is_empty() {
            keep.insert("coding-git".into());
            let button = self.upsert_coding_button(
                context,
                document_id,
                "coding-git",
                &coding.git,
                ShellIntent::RefreshCoding,
            )?;
            order.push(button.stable_id());
        }
        for file in &coding.files {
            let id = format!("coding-file-{}", file.path);
            keep.insert(id.clone());
            let button = self.upsert_coding_button(
                context,
                document_id,
                &id,
                &format!("{} · {}", file.path, file.kind),
                ShellIntent::OpenProjectFile(file.path.clone()),
            )?;
            order.push(button.stable_id());
        }
        for hit in &coding.hits {
            let id = format!("coding-hit-{}", hit.id);
            keep.insert(id.clone());
            let label = if hit.summary.is_empty() {
                hit.label.clone()
            } else {
                format!("{}\n{}", hit.label, hit.summary)
            };
            let button = self.upsert_coding_button(
                context,
                document_id,
                &id,
                &label,
                ShellIntent::OpenCodingHit(hit.id.clone()),
            )?;
            order.push(button.stable_id());
        }
        for row in coding.terminals.iter().chain(coding.tasks.iter()) {
            let id = format!("coding-row-{}", row.id);
            keep.insert(id.clone());
            let button = self.upsert_coding_button(
                context,
                document_id,
                &id,
                &row.label,
                ShellIntent::OpenCodingTerminal,
            )?;
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .coding_rows
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.coding_rows.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.coding_panel.stable_id(), &order)
    }

    fn upsert_coding_button(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        id: &str,
        label: &str,
        intent: ShellIntent,
    ) -> Result<Entity<Button>, FrameworkError> {
        if let Some(button) = self.coding_rows.get(id).copied() {
            context.update_component(button, |button, _| {
                *button = extra_button(label, ButtonKind::Subtle);
            })?;
            Ok(button)
        } else {
            let button = context
                .create_detached_component(document_id, extra_button(label, ButtonKind::Subtle))?;
            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
            self.coding_rows.insert(id.to_owned(), button);
            Ok(button)
        }
    }

    fn sync_project_page(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        if snapshot.project_page.is_none() {
            return Ok(());
        }
        context.update_component(self.project_page_title, |text, _| {
            *text = Text::new(snapshot.project_page_title.clone());
        })?;
        context.update_component(self.project_page_body, |text, _| {
            *text = Text::new(snapshot.project_page_body.clone());
        })?;
        context.update_component(self.architecture_canvas, |canvas, _| {
            *canvas = GraphCanvas::new("architecture", snapshot.architecture_graph.clone())
                .viewport(snapshot.architecture_viewport.clone())
                .selection(snapshot.architecture_selection.clone());
        })?;
        let mut keep = HashSet::new();
        let mut field_keep = HashSet::new();
        let mut order = vec![self.project_page_title.stable_id()];
        match snapshot.project_page {
            Some(ShellProjectPage::Clone) => {
                order.push(self.project_page_body.stable_id());
                self.upsert_field(
                    context,
                    document_id,
                    &mut field_keep,
                    &mut order,
                    "project-clone-repository",
                    &snapshot.clone_repository,
                    ShellIntent::CloneRepositoryChanged,
                )?;
                let parent = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-clone-parent",
                    if snapshot.clone_parent.is_empty() {
                        "选择父目录"
                    } else {
                        snapshot.clone_parent.as_str()
                    },
                    ButtonKind::Subtle,
                    ShellIntent::PickCloneParent,
                    false,
                )?;
                order.push(parent.stable_id());
                let start = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-clone-start",
                    "开始克隆",
                    ButtonKind::Primary,
                    ShellIntent::StartClone,
                    false,
                )?;
                order.push(start.stable_id());
                let cancel = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-clone-cancel",
                    "取消",
                    ButtonKind::Subtle,
                    ShellIntent::CancelClone,
                    false,
                )?;
                order.push(cancel.stable_id());
            }
            Some(ShellProjectPage::Settings) => {
                order.push(self.project_page_body.stable_id());
                self.upsert_field(
                    context,
                    document_id,
                    &mut field_keep,
                    &mut order,
                    "project-settings-name",
                    &snapshot.settings.project_name,
                    ShellIntent::ProjectNameChanged,
                )?;
                let workspace = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-settings-workspace",
                    if snapshot.settings.project_workspace.is_empty() {
                        "选择工作区"
                    } else {
                        snapshot.settings.project_workspace.as_str()
                    },
                    ButtonKind::Subtle,
                    ShellIntent::PickProjectWorkspace,
                    false,
                )?;
                order.push(workspace.stable_id());
                let save = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-settings-save",
                    "保存项目",
                    ButtonKind::Primary,
                    ShellIntent::SaveProjectSettings,
                    false,
                )?;
                order.push(save.stable_id());
            }
            Some(ShellProjectPage::Roadmap) => {
                self.upsert_field(
                    context,
                    document_id,
                    &mut field_keep,
                    &mut order,
                    "project-milestone-title",
                    &snapshot.milestone_title,
                    ShellIntent::MilestoneTitleChanged,
                )?;
                let create = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-milestone-create",
                    "新建里程碑",
                    ButtonKind::Primary,
                    ShellIntent::CreateMilestone,
                    false,
                )?;
                order.push(create.stable_id());
                let save = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-milestone-save",
                    "保存里程碑",
                    ButtonKind::Subtle,
                    ShellIntent::SaveMilestone,
                    false,
                )?;
                order.push(save.stable_id());
            }
            Some(ShellProjectPage::Architecture) => {
                let refresh = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-architecture-refresh",
                    "刷新",
                    ButtonKind::Subtle,
                    ShellIntent::RefreshArchitecture,
                    false,
                )?;
                order.push(refresh.stable_id());
                let rollback = self.upsert_tagged_button(
                    context,
                    document_id,
                    "project-architecture-rollback",
                    "回滚",
                    ButtonKind::Subtle,
                    ShellIntent::RollbackArchitecture,
                    false,
                )?;
                order.push(rollback.stable_id());
                order.push(self.architecture_canvas.stable_id());
            }
            _ => {
                if !snapshot.project_page_body.is_empty() {
                    order.push(self.project_page_body.stable_id());
                }
            }
        }
        let cards: Vec<(String, String, String, Option<ShellIntent>)> = match snapshot.project_page
        {
            Some(ShellProjectPage::Overview) => snapshot
                .project_cards
                .iter()
                .map(|card| {
                    (
                        format!("overview-{}", card.id),
                        card.title.clone(),
                        card.subtitle.clone(),
                        Some(ShellIntent::SelectProject(card.id.clone())),
                    )
                })
                .collect(),
            Some(ShellProjectPage::Roadmap) => snapshot
                .roadmap_cards
                .iter()
                .map(|card| {
                    (
                        format!("roadmap-{}", card.id),
                        card.title.clone(),
                        format!("{} · {}", card.status, card.date),
                        Some(ShellIntent::SelectRoadmapMilestone(card.id.clone())),
                    )
                })
                .collect(),
            Some(ShellProjectPage::Architecture) => snapshot
                .architecture_records
                .iter()
                .map(|record| {
                    (
                        format!("arch-{}", record.id),
                        record.title.clone(),
                        record.status.clone(),
                        None,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        for (id, title, subtitle, intent) in cards {
            keep.insert(id.clone());
            let card = if let Some(card) = self.project_cards.get(&id).copied() {
                card
            } else {
                let card =
                    context.create_detached_component(document_id, InteractiveCard::new())?;
                let heading =
                    context.create_detached_component(document_id, Text::new(title.clone()))?;
                let body =
                    context.create_detached_component(document_id, Text::new(subtitle.clone()))?;
                context.append_child(card, heading)?;
                context.append_child(card, body)?;
                if let Some(intent) = intent {
                    bind_activate(context, card, Arc::clone(&self.sink), intent)?;
                }
                self.project_cards.insert(id.clone(), card);
                card
            };
            order.push(card.stable_id());
        }
        let stale: Vec<_> = self
            .project_cards
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(card) = self.project_cards.remove(&key) {
                let _ = context.remove_view(card);
            }
        }
        reconcile_children(context, self.project_page.stable_id(), &order)
    }

    fn reconcile_timeline(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut action_keep = HashSet::new();
        let mut order = Vec::new();
        for item in &snapshot.timeline {
            keep.insert(item.id.clone());
            let source = content_hash(&item.markdown);
            let markdown = if let Some(entity) = self.timeline_markdown.get(&item.id).copied() {
                if self.timeline_markdown_source.get(&item.id) != Some(&source) {
                    context.update_component(entity, |markdown, _| {
                        *markdown = NativeMarkdown::parse(&item.markdown);
                    })?;
                    context.assemble_markdown(entity)?;
                    self.timeline_markdown_source.insert(item.id.clone(), source);
                }
                entity
            } else {
                let entity = context.create_detached_component(
                    document_id,
                    NativeMarkdown::parse(&item.markdown),
                )?;
                context.assemble_markdown(entity)?;
                self.timeline_markdown.insert(item.id.clone(), entity);
                self.timeline_markdown_source.insert(item.id.clone(), source);
                entity
            };
            let root = if let Some(entity) = self.timeline_items.get(&item.id).copied() {
                entity
            } else {
                let entity =
                    context.create_detached_component(document_id, HostStack::fill_column(6.0))?;
                self.timeline_items.insert(item.id.clone(), entity);
                entity
            };
            let mut children = vec![markdown.stable_id()];
            if item.can_expand {
                let id = format!("expand-{}", item.id);
                action_keep.insert(id.clone());
                let button = self.upsert_timeline_action(
                    context,
                    document_id,
                    &id,
                    if item.expanded { "收起" } else { "展开" },
                    ShellIntent::ToggleTimelineExpand(item.id.clone()),
                )?;
                children.push(button.stable_id());
            }
            if item.can_copy {
                let id = format!("copy-{}", item.id);
                action_keep.insert(id.clone());
                let button = self.upsert_timeline_action(
                    context,
                    document_id,
                    &id,
                    "复制",
                    ShellIntent::CopyTimeline(item.id.clone()),
                )?;
                children.push(button.stable_id());
            }
            if item.can_retry {
                let id = format!("retry-{}", item.id);
                action_keep.insert(id.clone());
                let button = self.upsert_timeline_action(
                    context,
                    document_id,
                    &id,
                    "重试",
                    ShellIntent::RetryTimeline(item.id.clone()),
                )?;
                children.push(button.stable_id());
            }
            reconcile_children(context, root.stable_id(), &children)?;
            order.push(root.stable_id());
        }
        if snapshot.timeline_can_load_earlier {
            let button = if let Some(button) = self.load_earlier {
                context.update_component(button, |button, _| {
                    *button = extra_button("加载更早", ButtonKind::Subtle);
                    button.disabled = false;
                })?;
                button
            } else {
                let button = context.create_detached_component(
                    document_id,
                    extra_button("加载更早", ButtonKind::Subtle),
                )?;
                bind_activate(
                    context,
                    button,
                    Arc::clone(&self.sink),
                    ShellIntent::LoadEarlierTimeline,
                )?;
                self.load_earlier = Some(button);
                button
            };
            reconcile_children(
                context,
                self.conversation_body.stable_id(),
                &[
                    self.heading_slot.stable_id(),
                    self.error.stable_id(),
                    self.timeline_scroll.stable_id(),
                    button.stable_id(),
                ],
            )?;
        } else {
            if let Some(button) = self.load_earlier.take() {
                let _ = context.remove_view(button);
            }
            reconcile_children(
                context,
                self.conversation_body.stable_id(),
                &[
                    self.heading_slot.stable_id(),
                    self.error.stable_id(),
                    self.timeline_scroll.stable_id(),
                ],
            )?;
        }
        let stale: Vec<_> = self
            .timeline_items
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(entity) = self.timeline_items.remove(&key) {
                let _ = context.remove_view(entity);
            }
            if let Some(entity) = self.timeline_markdown.remove(&key) {
                let _ = context.remove_view(entity);
            }
            self.timeline_markdown_source.remove(&key);
        }
        let stale_actions: Vec<_> = self
            .timeline_actions
            .keys()
            .filter(|key| !action_keep.contains(*key))
            .cloned()
            .collect();
        for key in stale_actions {
            if let Some(entity) = self.timeline_actions.remove(&key) {
                let _ = context.remove_view(entity);
            }
        }
        reconcile_children(context, self.timeline_scroll.stable_id(), &order)
    }

    fn upsert_timeline_action(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        id: &str,
        label: &str,
        intent: ShellIntent,
    ) -> Result<Entity<Button>, FrameworkError> {
        if let Some(button) = self.timeline_actions.get(id).copied() {
            context.update_component(button, |button, _| {
                *button = extra_button(label, ButtonKind::Subtle);
            })?;
            Ok(button)
        } else {
            let button = context
                .create_detached_component(document_id, extra_button(label, ButtonKind::Subtle))?;
            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
            self.timeline_actions.insert(id.to_owned(), button);
            Ok(button)
        }
    }

    fn reconcile_composer_extras(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut desired = Vec::new();
        if snapshot.suggestions_can_refresh {
            desired.push((
                "refresh-suggestions".to_owned(),
                "刷新建议".to_owned(),
                ButtonKind::Subtle,
                ShellIntent::RefreshSuggestions,
            ));
        }
        for suggestion in &snapshot.suggestions {
            desired.push((
                suggestion.id.clone(),
                suggestion.label.clone(),
                ButtonKind::Subtle,
                ShellIntent::ApplySuggestion(suggestion.prompt.clone()),
            ));
        }
        for attachment in &snapshot.attachments {
            desired.push((
                attachment.id.clone(),
                attachment.label.clone(),
                ButtonKind::Ghost,
                ShellIntent::RemoveAttachment(attachment.id.clone()),
            ));
        }
        let mut order = vec![
            self.plus_slot.stable_id(),
            self.attach.stable_id(),
            self.permission_slot.stable_id(),
        ];
        if let Some(label) = snapshot.worktree_label.as_deref() {
            context.update_component(self.worktree, |button, _| {
                *button = pill_button(label, ButtonKind::Text);
            })?;
            let worktree_children = if snapshot.worktree_can_pick {
                vec![
                    self.worktree_icon.stable_id(),
                    self.worktree.stable_id(),
                    self.worktree_pick.stable_id(),
                ]
            } else {
                vec![self.worktree_icon.stable_id(), self.worktree.stable_id()]
            };
            reconcile_children(context, self.worktree_slot.stable_id(), &worktree_children)?;
            order.push(self.worktree_slot.stable_id());
        }
        for (id, label, kind, intent) in desired {
            keep.insert(id.clone());
            let button = if let Some(button) = self.extra_buttons.get(&id).copied() {
                context.update_component(button, |button, _| {
                    *button = extra_button(&label, kind);
                })?;
                button
            } else {
                let button =
                    context.create_detached_component(document_id, extra_button(&label, kind))?;
                bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                self.extra_buttons.insert(id, button);
                button
            };
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .extra_buttons
            .keys()
            .filter(|key| {
                !keep.contains(*key) && !key.starts_with("pending-") && !key.starts_with("project-")
            })
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.extra_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.extras.stable_id(), &order)
    }

    fn reconcile_composer_completion(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        let desired = snapshot
            .slash_items
            .iter()
            .map(|item| {
                (
                    format!("slash-{}", item.name),
                    item.label.clone(),
                    ShellIntent::ApplySlash(item.name.clone()),
                )
            })
            .chain(snapshot.mention_items.iter().map(|item| {
                (
                    format!("mention-{}", item.id),
                    item.label.clone(),
                    ShellIntent::SelectMention(item.id.clone()),
                )
            }));
        for (id, label, intent) in desired {
            keep.insert(id.clone());
            let item = if let Some(item) = self.completion_items.get(&id).copied() {
                context.update_component(item, |item, _| {
                    *item = ActionMenuItem::new(label);
                })?;
                item
            } else {
                let item =
                    context.create_detached_component(document_id, ActionMenuItem::new(label))?;
                bind_activate(context, item, Arc::clone(&self.sink), intent)?;
                self.completion_items.insert(id, item);
                item
            };
            order.push(item.stable_id());
        }
        self.completion_items.retain(|key, item| {
            if keep.contains(key) {
                true
            } else {
                let _ = context.remove_view(*item);
                false
            }
        });
        reconcile_children(context, self.completion_slot.stable_id(), &order)
    }

    fn sync_settings_content(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        if !snapshot.settings_open {
            return Ok(());
        }
        context.update_component(self.settings_sidebar, |sidebar, _| {
            sidebar.model = snapshot.settings.model.clone();
            sidebar.state = snapshot.settings.state.clone();
        })?;
        context.update_component(self.appearance, |section, _| {
            section.theme = snapshot.theme;
            section.appearance = snapshot.settings.appearance.clone();
            section.platform_hint = None;
            section.material_status = Some(Arc::from(snapshot.settings.material_status.as_str()));
        })?;
        let tab = snapshot.settings.state.active_tab().as_str();
        let (heading, body, error, show_project, actions) = settings_tab_copy(&snapshot.settings);
        context.update_component(self.product_heading, |text, _| {
            *text = Text::new(heading.clone());
        })?;
        context.update_component(self.product_body, |text, _| {
            *text = Text::new(body);
        })?;
        context.update_component(self.product_error, |text, _| {
            *text = Text::new(error.unwrap_or_default());
        })?;
        context.update_component(self.project_name, |editor, _| {
            if editor.state.value != snapshot.settings.project_name {
                editor
                    .state
                    .replace_value(snapshot.settings.project_name.clone());
            }
            editor.disabled = !show_project;
        })?;
        context.update_component(self.project_workspace, |text, _| {
            *text = Text::new(if show_project {
                snapshot.settings.project_workspace.clone()
            } else {
                String::new()
            });
        })?;
        let content = match tab {
            "appearance" => self.appearance.stable_id(),
            "about" => self.about.stable_id(),
            _ => self.product_settings.stable_id(),
        };
        context.update_component(self.settings_page, |page, _| {
            page.model = snapshot.settings.model.clone();
            page.state = snapshot.settings.state.clone();
            page.content = Some(content);
        })?;
        context.assemble_settings_sidebar(self.settings_sidebar)?;
        context.assemble_appearance_section(self.appearance)?;
        context.assemble_about_section(self.about)?;
        context.assemble_settings_page(self.settings_page)?;

        let mut keep = HashSet::new();
        let mut card_order = Vec::new();
        if show_project {
            card_order.push(self.project_name_field.stable_id());
            card_order.push(self.project_workspace_row.stable_id());
        }
        if tab == "desktop" {
            context.update_component(self.shortcut_capture, |layer, _| {
                layer.set_recording(snapshot.settings.shortcut_capturing);
            })?;
            card_order.push(self.shortcut_capture.stable_id());
        }
        for action in actions {
            keep.insert(action.id.clone());
            let button = if let Some(button) = self.product_actions.get(&action.id).copied() {
                context.update_component(button, |button, _| {
                    *button = product_action_button(&action.label, action.primary);
                })?;
                button
            } else {
                let button = context.create_detached_component(
                    document_id,
                    product_action_button(&action.label, action.primary),
                )?;
                bind_activate(context, button, Arc::clone(&self.sink), action.intent)?;
                self.product_actions.insert(action.id.clone(), button);
                button
            };
            card_order.push(button.stable_id());
        }
        if tab == "provider" {
            for provider in &snapshot.settings.providers {
                keep.insert(provider.id.clone());
                let label = if provider.selected {
                    format!("当前：{}", provider.label)
                } else {
                    provider.label.clone()
                };
                let button = if let Some(button) = self.provider_rows.get(&provider.id).copied() {
                    context.update_component(button, |button, _| {
                        *button = extra_button(
                            &label,
                            if provider.selected {
                                ButtonKind::Primary
                            } else {
                                ButtonKind::Subtle
                            },
                        );
                    })?;
                    button
                } else {
                    let button = context.create_detached_component(
                        document_id,
                        extra_button(&label, ButtonKind::Subtle),
                    )?;
                    bind_activate(
                        context,
                        button,
                        Arc::clone(&self.sink),
                        ShellIntent::SelectProvider(provider.id.clone()),
                    )?;
                    self.provider_rows.insert(provider.id.clone(), button);
                    button
                };
                card_order.push(button.stable_id());
            }
        }
        self.append_settings_forms(context, document_id, snapshot, &mut keep, &mut card_order)?;
        let stale: Vec<_> = self
            .product_actions
            .keys()
            .chain(self.provider_rows.keys())
            .chain(self.form_fields.keys())
            .chain(self.form_switches.keys())
            .filter(|key| {
                !keep.contains(*key) && !key.starts_with("project-") && !key.starts_with("pending-")
            })
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.product_actions.remove(&key) {
                let _ = context.remove_view(button);
            }
            if let Some(button) = self.provider_rows.remove(&key) {
                let _ = context.remove_view(button);
            }
            if let Some(field) = self.form_fields.remove(&key) {
                let _ = context.remove_view(field);
            }
            if let Some(wrapper) = self.form_wrappers.remove(&key) {
                let _ = context.remove_view(wrapper);
            }
            if let Some(toggle) = self.form_switches.remove(&key) {
                let _ = context.remove_view(toggle);
            }
        }
        context.update_component(self.settings_card, |card, _| {
            *card = SettingsCard::new(heading);
        })?;
        reconcile_children(context, self.settings_card.stable_id(), &card_order)?;
        reconcile_children(
            context,
            self.product_settings.stable_id(),
            &[
                self.product_heading.stable_id(),
                self.product_body.stable_id(),
                self.product_error.stable_id(),
                self.settings_card.stable_id(),
            ],
        )
    }

    fn append_settings_forms(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
        keep: &mut HashSet<String>,
        order: &mut Vec<StableNodeId>,
    ) -> Result<(), FrameworkError> {
        let settings = &snapshot.settings;
        match settings.state.active_tab().as_str() {
            "provider" => {
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "provider_secret",
                    &settings.provider_secret,
                    |value| ShellIntent::ProviderSecretChanged(value),
                )?;
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "provider_model",
                    &settings.provider_model,
                    |value| ShellIntent::ProviderModelChanged(value),
                )?;
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "provider_openai",
                    &settings.provider_openai_endpoint,
                    |value| ShellIntent::ProviderOpenAiEndpointChanged(value),
                )?;
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "provider_anthropic",
                    &settings.provider_anthropic_endpoint,
                    |value| ShellIntent::ProviderAnthropicEndpointChanged(value),
                )?;
                for credential in &settings.credentials {
                    let id = format!("revoke-{}", credential.id);
                    keep.insert(id.clone());
                    let label = format!("撤销 {}", credential.label);
                    let button = if let Some(button) = self.product_actions.get(&id).copied() {
                        context.update_component(button, |button, _| {
                            *button = extra_button(&label, ButtonKind::Danger);
                        })?;
                        button
                    } else {
                        let button = context.create_detached_component(
                            document_id,
                            extra_button(&label, ButtonKind::Danger),
                        )?;
                        bind_activate(
                            context,
                            button,
                            Arc::clone(&self.sink),
                            ShellIntent::RevokeProviderCredential {
                                credential_id: credential.id.clone(),
                                revision: credential.revision,
                            },
                        )?;
                        self.product_actions.insert(id, button);
                        button
                    };
                    order.push(button.stable_id());
                }
            }
            "agent" => {
                if settings.custom_agent_editor_open {
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "agent_name",
                        &settings.custom_agent_name,
                        |value| ShellIntent::AgentNameChanged(value),
                    )?;
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "agent_description",
                        &settings.custom_agent_description,
                        |value| ShellIntent::AgentDescriptionChanged(value),
                    )?;
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "agent_instruction",
                        &settings.custom_agent_instruction,
                        |value| ShellIntent::AgentInstructionChanged(value),
                    )?;
                }
                for agent in &settings.custom_agents {
                    for (suffix, label, intent) in [
                        (
                            "edit",
                            format!("编辑 {}", agent.label),
                            ShellIntent::EditCustomAgent(agent.id.clone()),
                        ),
                        (
                            "toggle",
                            if agent.enabled {
                                format!("关闭 {}", agent.label)
                            } else {
                                format!("开启 {}", agent.label)
                            },
                            ShellIntent::ToggleCustomAgent(agent.id.clone()),
                        ),
                        (
                            "delete",
                            format!("删除 {}", agent.label),
                            ShellIntent::DeleteCustomAgent(agent.id.clone()),
                        ),
                    ] {
                        let id = format!("agent-{}-{}", agent.id, suffix);
                        keep.insert(id.clone());
                        let kind = if suffix == "delete" {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Subtle
                        };
                        let button = if let Some(button) = self.product_actions.get(&id).copied() {
                            context.update_component(button, |button, _| {
                                *button = extra_button(&label, kind);
                            })?;
                            button
                        } else {
                            let button = context.create_detached_component(
                                document_id,
                                extra_button(&label, kind),
                            )?;
                            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                            self.product_actions.insert(id, button);
                            button
                        };
                        order.push(button.stable_id());
                    }
                }
            }
            "quota" => {
                let chart = if let Some(chart) = self.quota_chart {
                    context.update_component(chart, |view, _| {
                        *view = TimeSeriesChart::new(settings.quota_values.iter().copied())
                            .label("用量");
                    })?;
                    chart
                } else {
                    let chart = context.create_detached_component(
                        document_id,
                        TimeSeriesChart::new(settings.quota_values.iter().copied()).label("用量"),
                    )?;
                    self.quota_chart = Some(chart);
                    chart
                };
                order.push(chart.stable_id());
            }
            "extensions" => {
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "skill_id",
                    &settings.skill_id,
                    |value| ShellIntent::SkillIdChanged(value),
                )?;
                self.upsert_field(
                    context,
                    document_id,
                    keep,
                    order,
                    "skill_description",
                    &settings.skill_description,
                    |value| ShellIntent::SkillDescriptionChanged(value),
                )?;
                for skill in &settings.skills {
                    let id = format!("skill-{}", skill.id);
                    keep.insert(id.clone());
                    let label = if skill.enabled {
                        format!("关闭 {}", skill.label)
                    } else {
                        format!("开启 {}", skill.label)
                    };
                    let button = if let Some(button) = self.product_actions.get(&id).copied() {
                        context.update_component(button, |button, _| {
                            *button = extra_button(&label, ButtonKind::Subtle);
                        })?;
                        button
                    } else {
                        let button = context.create_detached_component(
                            document_id,
                            extra_button(&label, ButtonKind::Subtle),
                        )?;
                        bind_activate(
                            context,
                            button,
                            Arc::clone(&self.sink),
                            ShellIntent::ToggleSkill(skill.id.clone()),
                        )?;
                        self.product_actions.insert(id, button);
                        button
                    };
                    order.push(button.stable_id());
                }
                for server in &settings.mcp_servers {
                    for (suffix, label, intent) in [
                        (
                            "edit",
                            format!("编辑 {}", server.label),
                            ShellIntent::EditMcpServer(server.id.clone()),
                        ),
                        (
                            "toggle",
                            if server.enabled {
                                format!("关闭 {}", server.label)
                            } else {
                                format!("开启 {}", server.label)
                            },
                            ShellIntent::ToggleMcpServer(server.id.clone()),
                        ),
                    ] {
                        let id = format!("mcp-{}-{}", server.id, suffix);
                        keep.insert(id.clone());
                        let button = if let Some(button) = self.product_actions.get(&id).copied() {
                            context.update_component(button, |button, _| {
                                *button = extra_button(&label, ButtonKind::Subtle);
                            })?;
                            button
                        } else {
                            let button = context.create_detached_component(
                                document_id,
                                extra_button(&label, ButtonKind::Subtle),
                            )?;
                            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                            self.product_actions.insert(id, button);
                            button
                        };
                        order.push(button.stable_id());
                    }
                }
                if let Some(editor) = &settings.mcp_editor {
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "mcp_server_id",
                        &editor.server_id,
                        |value| ShellIntent::McpServerIdChanged(value),
                    )?;
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "mcp_location",
                        &editor.location,
                        |value| ShellIntent::McpLocationChanged(value),
                    )?;
                    self.upsert_field(
                        context,
                        document_id,
                        keep,
                        order,
                        "mcp_args",
                        &editor.args,
                        |value| ShellIntent::McpArgsChanged(value),
                    )?;
                }
            }
            "remote" => {
                self.upsert_switch(
                    context,
                    document_id,
                    keep,
                    order,
                    "remote_host",
                    "远程主机",
                    settings.remote_host_enabled,
                    ShellIntent::ToggleRemoteHost,
                )?;
                self.upsert_switch(
                    context,
                    document_id,
                    keep,
                    order,
                    "remote_keep_awake",
                    "保持唤醒",
                    settings.remote_keep_awake,
                    ShellIntent::ToggleRemoteKeepAwake,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn upsert_field(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        keep: &mut HashSet<String>,
        order: &mut Vec<StableNodeId>,
        id: &str,
        value: &str,
        intent: impl Fn(String) -> ShellIntent + Send + Sync + 'static,
    ) -> Result<(), FrameworkError> {
        keep.insert(id.to_owned());
        let label = settings_field_label(id);
        let editor = if let Some(field) = self.form_fields.get(id).copied() {
            context.update_component(field, |editor, _| {
                if editor.state.value != value {
                    editor.state.replace_value(value.to_owned());
                }
            })?;
            field
        } else {
            let field = context.create_detached_component(
                document_id,
                TextArea::new(value.to_owned()).height(40.0),
            )?;
            let sink = Arc::clone(&self.sink);
            context.on(field, move |_, event: &TextChanged, _| {
                emit(&sink, intent(event.value.clone()));
            })?;
            self.form_fields.insert(id.to_owned(), field);
            field
        };
        let wrapper = if let Some(wrapper) = self.form_wrappers.get(id).copied() {
            context.update_component(wrapper, |field, _| {
                *field = FormField::new(label).control_child(editor.stable_id());
            })?;
            wrapper
        } else {
            let wrapper = context.create_detached_component(
                document_id,
                FormField::new(label).control_child(editor.stable_id()),
            )?;
            context.append_child(wrapper, editor)?;
            self.form_wrappers.insert(id.to_owned(), wrapper);
            wrapper
        };
        order.push(wrapper.stable_id());
        Ok(())
    }

    fn upsert_switch(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        keep: &mut HashSet<String>,
        order: &mut Vec<StableNodeId>,
        id: &str,
        label: &str,
        checked: bool,
        intent: ShellIntent,
    ) -> Result<(), FrameworkError> {
        keep.insert(id.to_owned());
        let toggle = if let Some(toggle) = self.form_switches.get(id).copied() {
            context.update_component(toggle, |view, _| {
                *view = Switch::new(label, checked);
            })?;
            toggle
        } else {
            let toggle =
                context.create_detached_component(document_id, Switch::new(label, checked))?;
            let sink = Arc::clone(&self.sink);
            context.on(toggle, move |_, _event: &ToggleChanged, _| {
                emit(&sink, intent.clone());
            })?;
            self.form_switches.insert(id.to_owned(), toggle);
            toggle
        };
        order.push(toggle.stable_id());
        Ok(())
    }

    fn sync_workspace_page(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let pane_kind = active_pane_item_kind(snapshot);
        let (title, status) = match pane_kind {
            Some("document-editor") => snapshot
                .document
                .as_ref()
                .map(|document| (document.title.clone(), document.status.clone()))
                .unwrap_or_default(),
            Some("terminal") => (
                "终端".to_owned(),
                snapshot
                    .terminal
                    .as_ref()
                    .and_then(|terminal| terminal.notice.clone())
                    .unwrap_or_default(),
            ),
            Some("project-files") => (
                "项目文件".to_owned(),
                snapshot
                    .files
                    .as_ref()
                    .and_then(|files| files.preview.clone())
                    .unwrap_or_default(),
            ),
            _ => Default::default(),
        };
        context.update_component(self.workspace_heading, |text, _| {
            *text = Text::new(title);
        })?;
        context.update_component(self.workspace_status, |text, _| {
            *text = Text::new(status);
        })?;
        if let Some(document) = &snapshot.document {
            context.update_component(self.workspace_editor, |editor_view, _| {
                if editor_view.state.value != document.text {
                    editor_view.state.replace_value(document.text.clone());
                }
                editor_view.disabled = document.read_only;
                apply_workspace_editor_chrome(editor_view, Some(document.language.as_str()));
            })?;
        }
        if let Some(terminal) = &snapshot.terminal {
            context.update_component(self.workspace_log, |log, _| {
                if log.state.value != terminal.output {
                    log.state.replace_value(terminal.output.clone());
                }
                apply_workspace_editor_chrome(log, None);
                log.disabled = true;
            })?;
        }
        let terminal_input = snapshot
            .terminal
            .as_ref()
            .map(|terminal| terminal.input.clone())
            .unwrap_or_default();
        context.update_component(self.workspace_input, |input, _| {
            if input.state.value != terminal_input {
                input.state.replace_value(terminal_input);
            }
            input.disabled = snapshot.terminal.is_none();
        })?;
        if let Some(files) = &snapshot.files {
            context.update_component(self.workspace_tree, |tree, _| {
                *tree = files.tree.clone();
            })?;
        } else {
            context.update_component(self.workspace_tree, |tree, _| {
                *tree = TreeView::new(Vec::new());
            })?;
        }
        self.reconcile_workspace_actions(context, document_id, snapshot)?;
        let mut order = vec![
            self.workspace_heading.stable_id(),
            self.workspace_status.stable_id(),
        ];
        match pane_kind {
            Some("document-editor") => {
                order.push(self.workspace_editor.stable_id());
                order.push(self.workspace_actions.stable_id());
            }
            Some("project-files") => {
                order.push(self.workspace_tree.stable_id());
                order.push(self.workspace_actions.stable_id());
            }
            Some("terminal") => {
                order.push(self.workspace_log.stable_id());
                order.push(self.workspace_input.stable_id());
                order.push(self.workspace_actions.stable_id());
            }
            _ => {}
        }
        reconcile_children(context, self.workspace_content.stable_id(), &order)
    }

    fn reconcile_workspace_actions(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut desired = Vec::new();
        if let Some(document) = &snapshot.document {
            if document.dirty && !document.read_only {
                desired.push((
                    "save",
                    "保存",
                    ButtonKind::Primary,
                    ShellIntent::SaveDocument,
                ));
                desired.push((
                    "discard",
                    "放弃",
                    ButtonKind::Subtle,
                    ShellIntent::DiscardDocument,
                ));
            }
        }
        if snapshot.files.is_some() {
            desired.push((
                "refresh_files",
                "刷新",
                ButtonKind::Subtle,
                ShellIntent::RefreshProjectFiles,
            ));
        }
        if snapshot.terminal.is_some() {
            desired.push((
                "terminal_submit",
                "运行",
                ButtonKind::Primary,
                ShellIntent::TerminalSubmit,
            ));
            desired.push((
                "terminal_interrupt",
                "停止",
                ButtonKind::Danger,
                ShellIntent::TerminalInterrupt,
            ));
        }
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for (id, label, kind, intent) in desired {
            keep.insert(id.to_owned());
            let button = if let Some(button) = self.workspace_buttons.get(id).copied() {
                context.update_component(button, |button, _| {
                    *button = extra_button(label, kind);
                })?;
                button
            } else {
                let button =
                    context.create_detached_component(document_id, extra_button(label, kind))?;
                bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                self.workspace_buttons.insert(id.to_owned(), button);
                button
            };
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .workspace_buttons
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.workspace_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.workspace_actions.stable_id(), &order)
    }

    fn sync_panes(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let (pane_selected, pane_options) = pane_tab_options(snapshot);
        context.update_component(self.pane_tabs, |tabs, _| {
            *tabs = Tabs::new(pane_selected)
                .options(pane_options)
                .strip_id(active_pane_strip_id(snapshot))
                .fill(true);
        })?;
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        if snapshot.panes.len() > 1 {
            for pane in &snapshot.panes {
                let id = format!("pane-{}", pane.id);
                keep.insert(id.clone());
                let label = if pane.active {
                    format!("窗格 {}", pane.id)
                } else {
                    format!("切换 {}", pane.id)
                };
                let button = self.upsert_chrome_button(
                    context,
                    document_id,
                    &id,
                    &label,
                    if pane.active {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Subtle
                    },
                    ShellIntent::FocusWorkspacePane(pane.id.clone()),
                )?;
                order.push(button.stable_id());
            }
        }
        let stale: Vec<_> = self
            .pane_buttons
            .keys()
            .filter(|key| !key.starts_with("auto-") && !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.pane_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.pane_bar.stable_id(), &order)?;
        context.update_component(self.pane_chrome, |chrome, _| {
            let mut actions: Vec<PaneChromeAction> = chrome
                .actions
                .iter()
                .filter(|action| {
                    matches!(
                        action.kind,
                        PaneChromeActionKind::SplitHorizontal | PaneChromeActionKind::SplitVertical
                    )
                })
                .cloned()
                .collect();
            if snapshot.pane_can_move_window {
                actions.push(
                    PaneChromeAction::new(PaneChromeActionKind::MoveToWindow, "移至新窗口")
                        .target(self.pane_move_window.stable_id()),
                );
            }
            if snapshot.pane_can_move_next {
                actions.push(
                    PaneChromeAction::new(PaneChromeActionKind::MoveToNextPane, "移至下一窗格")
                        .target(self.pane_move_next.stable_id()),
                );
            }
            chrome.actions = actions;
        })
    }

    fn upsert_chrome_button(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        id: &str,
        label: &str,
        kind: ButtonKind,
        intent: ShellIntent,
    ) -> Result<Entity<Button>, FrameworkError> {
        if let Some(button) = self.pane_buttons.get(id).copied() {
            context.update_component(button, |button, _| {
                *button = extra_button(label, kind);
            })?;
            Ok(button)
        } else {
            let button =
                context.create_detached_component(document_id, extra_button(label, kind))?;
            bind_activate(context, button, Arc::clone(&self.sink), intent)?;
            self.pane_buttons.insert(id.to_owned(), button);
            Ok(button)
        }
    }

    fn sync_automations(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        if !snapshot.automations_open {
            return Ok(());
        }
        context.update_component(self.automation_canvas, |canvas, _| {
            canvas.model = snapshot.automation_graph.clone();
            canvas.viewport = snapshot.automation_viewport.clone();
            canvas.selection = snapshot.automation_selection.clone();
        })?;
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for automation in &snapshot.automations {
            let id = format!("auto-{}", automation.id);
            keep.insert(id.clone());
            let button = if let Some(button) = self.pane_buttons.get(&id).copied() {
                context.update_component(button, |button, _| {
                    *button = extra_button(
                        &automation.label,
                        if automation.selected {
                            ButtonKind::Primary
                        } else {
                            ButtonKind::Subtle
                        },
                    );
                })?;
                button
            } else {
                let button = context.create_detached_component(
                    document_id,
                    extra_button(&automation.label, ButtonKind::Subtle),
                )?;
                bind_activate(
                    context,
                    button,
                    Arc::clone(&self.sink),
                    ShellIntent::SelectAutomation(automation.id.clone()),
                )?;
                self.pane_buttons.insert(id, button);
                button
            };
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .pane_buttons
            .keys()
            .filter(|key| key.starts_with("auto-") && !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.pane_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.automations_body.stable_id(), &order)?;
        let page = if snapshot.automations.is_empty() {
            vec![self.automations_empty.stable_id()]
        } else {
            vec![
                self.automation_actions.stable_id(),
                self.automation_canvas.stable_id(),
            ]
        };
        reconcile_children(context, self.automations_page.stable_id(), &page)?;
        let mut actions = Vec::new();
        for (id, label, kind, intent) in [
            (
                "auto-refresh",
                "刷新",
                ButtonKind::Subtle,
                ShellIntent::RefreshAutomations,
            ),
            (
                "auto-create",
                "新建",
                ButtonKind::Primary,
                ShellIntent::CreateAutomation,
            ),
            (
                "auto-save",
                "保存草稿",
                ButtonKind::Subtle,
                ShellIntent::SaveAutomationDraft,
            ),
            (
                "auto-run",
                "运行",
                ButtonKind::Primary,
                ShellIntent::RunAutomation,
            ),
        ] {
            keep.insert(id.to_owned());
            let button =
                self.upsert_chrome_button(context, document_id, id, label, kind, intent)?;
            actions.push(button.stable_id());
        }
        reconcile_children(context, self.automation_actions.stable_id(), &actions)
    }

    fn titlebar_more_items(
        snapshot: &PrimaryShellSnapshot,
    ) -> Vec<(&'static str, String, ShellIntent)> {
        let mut items = vec![
            (
                "more-palette",
                "命令面板".to_owned(),
                ShellIntent::ToggleCommandPalette,
            ),
            (
                "more-status",
                "会话状态".to_owned(),
                ShellIntent::OpenConversationStatus,
            ),
        ];
        if snapshot.titlebar_has_task {
            items.extend([
                (
                    "more-back",
                    "返回任务列表".to_owned(),
                    ShellIntent::BackToTaskList,
                ),
                (
                    "more-popup",
                    "在弹出窗口继续".to_owned(),
                    ShellIntent::OpenTaskPopup,
                ),
                (
                    "more-ask",
                    "在弹出窗口询问".to_owned(),
                    ShellIntent::AskTaskPopup,
                ),
                (
                    "more-inspector",
                    "会话详情".to_owned(),
                    ShellIntent::ToggleTaskInspector,
                ),
                (
                    "more-browser",
                    "浏览器".to_owned(),
                    ShellIntent::OpenTaskBrowser,
                ),
            ]);
        }
        if snapshot.titlebar_can_split {
            items.extend([
                (
                    "more-split-h",
                    "横向拆分".to_owned(),
                    ShellIntent::SplitWorkspaceHorizontal,
                ),
                (
                    "more-split-v",
                    "纵向拆分".to_owned(),
                    ShellIntent::SplitWorkspaceVertical,
                ),
            ]);
        }
        if snapshot.titlebar_can_close {
            items.push((
                "more-close",
                "关闭当前".to_owned(),
                ShellIntent::CloseCurrentWorkspaceItem,
            ));
        }
        items
    }

    fn sync_overlay(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let Some(host) = self.overlay_host else {
            return Ok(());
        };
        if let Some(confirm) = &snapshot.confirm {
            let dialog = if let Some(dialog) = self.confirm {
                context.update_component(dialog, |view, _| {
                    *view = ConfirmDialog::new(confirm.title.clone(), confirm.message.clone());
                    view.danger = confirm.danger;
                    view.busy = confirm.busy;
                })?;
                dialog
            } else {
                let dialog = context.create_detached_component(
                    document_id,
                    ConfirmDialog::new(confirm.title.clone(), confirm.message.clone()),
                )?;
                let cancel = context.create_detached_component(
                    document_id,
                    extra_button(&confirm.cancel_label, ButtonKind::Subtle),
                )?;
                let commit = context.create_detached_component(
                    document_id,
                    extra_button(
                        &confirm.confirm_label,
                        if confirm.danger {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Primary
                        },
                    ),
                )?;
                context.set_confirm_slots(
                    dialog,
                    ConfirmSlots {
                        body: None,
                        close_action: None,
                        cancel: cancel.stable_id(),
                        secondary: None,
                        confirm: commit.stable_id(),
                    },
                )?;
                let sink = Arc::clone(&self.sink);
                context.on(dialog, move |_, intent: &ConfirmIntent, _| {
                    emit(
                        &sink,
                        match intent {
                            ConfirmIntent::Confirm { .. } => ShellIntent::ConfirmDestructive,
                            ConfirmIntent::Cancel | ConfirmIntent::Secondary => {
                                ShellIntent::CancelDestructive
                            }
                        },
                    );
                })?;
                context.append_child(host, dialog)?;
                self.confirm = Some(dialog);
                self.confirm_cancel = Some(cancel);
                self.confirm_commit = Some(commit);
                dialog
            };
            if let (Some(cancel), Some(commit)) = (self.confirm_cancel, self.confirm_commit) {
                context.update_component(cancel, |button, _| {
                    *button = extra_button(&confirm.cancel_label, ButtonKind::Subtle);
                    button.disabled = confirm.busy;
                })?;
                context.update_component(commit, |button, _| {
                    *button = extra_button(
                        &confirm.confirm_label,
                        if confirm.danger {
                            ButtonKind::Danger
                        } else {
                            ButtonKind::Primary
                        },
                    );
                    button.disabled = confirm.busy;
                })?;
            }
            let _ = context.set_confirm_state(dialog, confirm.busy, confirm.danger);
            context.update_component(self.shell, |shell, _| {
                shell.overlays = vec![dialog.stable_id()];
            })?;
            context.activate_overlay(host, dialog)?;
            return Ok(());
        } else if let Some(dialog) = self.confirm.take() {
            let _ = context.remove_view(dialog);
            if let Some(cancel) = self.confirm_cancel.take() {
                let _ = context.remove_view(cancel);
            }
            if let Some(commit) = self.confirm_commit.take() {
                let _ = context.remove_view(commit);
            }
        }

        if snapshot.command_palette_open {
            let palette = if let Some(palette) = self.palette {
                context.update_component(palette, |view, _| {
                    *view = command_palette_view(snapshot);
                    view.selected = snapshot.command_palette_selected;
                })?;
                palette
            } else {
                let palette = context
                    .create_detached_component(document_id, command_palette_view(snapshot))?;
                let sink = Arc::clone(&self.sink);
                context.on(palette, move |_, event: &CommandPaletteEvent, _| {
                    emit(&sink, ShellIntent::CommandPalette(event.clone()));
                })?;
                context.append_child(host, palette)?;
                self.palette = Some(palette);
                palette
            };
            context.update_component(self.shell, |shell, _| {
                shell.overlays = vec![palette.stable_id()];
            })?;
            context.activate_overlay(host, palette)?;
            self.focus_targets.insert(
                target_ids::COMMAND_PALETTE_INPUT.to_owned(),
                palette.stable_id(),
            );
            return Ok(());
        } else if let Some(palette) = self.palette.take() {
            let _ = context.remove_view(palette);
            self.focus_targets.remove(target_ids::COMMAND_PALETTE_INPUT);
        }

        let mut overlays = Vec::new();
        if !snapshot.sidebar_menu.is_empty() {
            let items: Vec<ContextMenuItem> = snapshot
                .sidebar_menu
                .iter()
                .map(|item| ContextMenuItem::new(item.id.clone(), item.label.clone()))
                .collect();
            let (anchor_x, anchor_y) = snapshot
                .add_project_menu_open
                .then(|| context.world().layout_box(self.add_project_menu.stable_id()))
                .flatten()
                .map(|bounds| (bounds.x, bounds.y + bounds.height))
                .or(snapshot.sidebar_menu_anchor)
                .unwrap_or((420.0, 48.0));
            let menu = if let Some(menu) = self.more_menu {
                context.update_component(menu, |view, _| {
                    *view = ContextMenu::new(anchor_x, anchor_y).items(items).open(true);
                })?;
                menu
            } else {
                let menu = context.create_detached_component(
                    document_id,
                    ContextMenu::new(anchor_x, anchor_y).items(items).open(true),
                )?;
                let sink = Arc::clone(&self.sink);
                context.on(menu, move |_, event: &ContextMenuEvent, _| match event {
                    ContextMenuEvent::Select(value) => {
                        emit(&sink, ShellIntent::SidebarMenuAction(value.to_string()));
                    }
                    ContextMenuEvent::Dismiss => {
                        emit(&sink, ShellIntent::SidebarMenuAction(String::new()));
                    }
                    ContextMenuEvent::Search(_) => {}
                })?;
                context.append_child(host, menu)?;
                self.more_menu = Some(menu);
                menu
            };
            overlays.push(menu.stable_id());
            context.activate_overlay(host, menu)?;
        } else if let Some(menu) = self.more_menu.take() {
            let _ = context.remove_view(menu);
        }

        if snapshot.titlebar_menu_open {
            let items: Vec<ContextMenuItem> = Self::titlebar_more_items(snapshot)
                .into_iter()
                .map(|(id, label, _)| ContextMenuItem::new(id, label))
                .collect();
            let menu = if let Some(menu) = self.titlebar_menu {
                context.update_component(menu, |view, _| {
                    *view = ContextMenu::new(420.0, 48.0).items(items).open(true);
                })?;
                menu
            } else {
                let menu = context.create_detached_component(
                    document_id,
                    ContextMenu::new(420.0, 48.0).items(items).open(true),
                )?;
                let sink = Arc::clone(&self.sink);
                context.on(menu, move |_, event: &ContextMenuEvent, _| match event {
                    ContextMenuEvent::Select(value) => {
                        emit(&sink, titlebar_menu_intent(value.as_ref()));
                    }
                    ContextMenuEvent::Dismiss => {
                        emit(&sink, ShellIntent::ToggleTitlebarMenu);
                    }
                    ContextMenuEvent::Search(_) => {}
                })?;
                context.append_child(host, menu)?;
                self.titlebar_menu = Some(menu);
                menu
            };
            overlays.push(menu.stable_id());
            context.activate_overlay(host, menu)?;
        } else if let Some(menu) = self.titlebar_menu.take() {
            let _ = context.remove_view(menu);
        }

        if let Some(preview) = &snapshot.markdown_preview {
            let viewer = if let Some(viewer) = self.image_viewer {
                context.update_component(viewer, |view, _| {
                    *view = markdown_image_viewer(preview);
                })?;
                viewer
            } else {
                let viewer = context
                    .create_detached_component(document_id, markdown_image_viewer(preview))?;
                let sink = Arc::clone(&self.sink);
                context.on(viewer, move |_, event: &ImageViewerEvent, _| match event {
                    ImageViewerEvent::Close | ImageViewerEvent::Outside => {
                        emit(&sink, ShellIntent::CloseMarkdownPreview);
                    }
                    ImageViewerEvent::Interaction => {
                        emit(&sink, ShellIntent::MarkdownImageViewerInteraction);
                    }
                })?;
                context.append_child(host, viewer)?;
                self.image_viewer = Some(viewer);
                viewer
            };
            overlays.push(viewer.stable_id());
            context.activate_overlay(host, viewer)?;
        } else if let Some(viewer) = self.image_viewer.take() {
            let _ = context.remove_view(viewer);
        }

        context.update_component(self.shell, |shell, _| {
            shell.overlays = overlays;
        })?;
        Ok(())
    }

    fn sync_diagnostics(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let diagnostics = snapshot
            .document
            .as_ref()
            .map(|document| document.diagnostics.as_slice())
            .unwrap_or(&[]);
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            let id = format!("{index}");
            keep.insert(id.clone());
            let label = format!("{}  {}", diagnostic.severity, diagnostic.message);
            let row = if let Some(row) = self.diagnostic_rows.get(&id).copied() {
                context.update_component(row, |text, _| {
                    *text = Text::new(label);
                })?;
                row
            } else {
                let row = context.create_detached_component(document_id, Text::new(label))?;
                self.diagnostic_rows.insert(id, row);
                row
            };
            order.push(row.stable_id());
        }
        let stale: Vec<_> = self
            .diagnostic_rows
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(row) = self.diagnostic_rows.remove(&key) {
                let _ = context.remove_view(row);
            }
        }
        reconcile_children(context, self.diagnostics_panel.stable_id(), &order)
    }
}

fn titlebar_menu_intent(id: &str) -> ShellIntent {
    match id {
        "more-palette" => ShellIntent::ToggleCommandPalette,
        "more-status" => ShellIntent::OpenConversationStatus,
        "more-back" => ShellIntent::BackToTaskList,
        "more-popup" => ShellIntent::OpenTaskPopup,
        "more-ask" => ShellIntent::AskTaskPopup,
        "more-inspector" => ShellIntent::ToggleTaskInspector,
        "more-browser" => ShellIntent::OpenTaskBrowser,
        "more-split-h" => ShellIntent::SplitWorkspaceHorizontal,
        "more-split-v" => ShellIntent::SplitWorkspaceVertical,
        "more-close" => ShellIntent::CloseCurrentWorkspaceItem,
        _ => ShellIntent::ToggleTitlebarMenu,
    }
}

fn nav_icon(settings: bool) -> Icon {
    if settings {
        Icon::Settings
    } else {
        Icon::Nodes
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarBucket {
    Session,
    Project,
    Inbox,
}

struct SidebarRowGroups {
    sessions: Vec<ShellSidebarRow>,
    projects: Vec<ShellSidebarRow>,
    inbox: Vec<ShellSidebarRow>,
    grouped: bool,
    inbox_expanded: bool,
}

fn sidebar_project_entry_count(rows: &[ShellSidebarRow]) -> usize {
    rows.iter()
        .filter(|row| {
            matches!(
                row.kind,
                ShellSidebarKind::Project | ShellSidebarKind::Archived
            )
        })
        .count()
}

fn sidebar_row_is_section_chrome(row: &ShellSidebarRow) -> bool {
    matches!(row.kind, ShellSidebarKind::Header | ShellSidebarKind::Inbox)
        || matches!(
            row.id.as_str(),
            "sessions-empty" | "projects-empty" | "inbox-empty"
        )
}

fn partition_sidebar_rows(snapshot: &PrimaryShellSnapshot) -> SidebarRowGroups {
    let grouped = !snapshot.sidebar_search_open
        && snapshot
            .sidebar_rows
            .iter()
            .any(|row| row.kind == ShellSidebarKind::Inbox || row.id == "projects-header");
    let inbox_expanded = snapshot
        .sidebar_rows
        .iter()
        .find(|row| row.kind == ShellSidebarKind::Inbox)
        .and_then(|row| row.expanded)
        .unwrap_or(true);
    if snapshot.sidebar_rows.is_empty() {
        return SidebarRowGroups {
            sessions: snapshot
                .tasks
                .iter()
                .map(|task| ShellSidebarRow {
                    id: task.id.as_str().to_owned(),
                    label: task.title.clone(),
                    kind: ShellSidebarKind::Task,
                    selected: task.selected,
                    ancestor: false,
                    depth: 0,
                    expanded: None,
                    icon: Icon::Workspace,
                    can_stop: false,
                    can_menu: true,
                    can_draft: false,
                })
                .collect(),
            projects: Vec::new(),
            inbox: Vec::new(),
            grouped: false,
            inbox_expanded,
        };
    }
    if snapshot.sidebar_search_open {
        return SidebarRowGroups {
            sessions: snapshot
                .sidebar_rows
                .iter()
                .filter(|row| !sidebar_row_is_section_chrome(row))
                .cloned()
                .collect(),
            projects: Vec::new(),
            inbox: Vec::new(),
            grouped: false,
            inbox_expanded,
        };
    }
    let mut sessions = Vec::new();
    let mut projects = Vec::new();
    let mut inbox = Vec::new();
    let mut bucket = SidebarBucket::Session;
    for row in &snapshot.sidebar_rows {
        if row.kind == ShellSidebarKind::Header && row.id == "projects-header" {
            bucket = SidebarBucket::Project;
            continue;
        }
        if row.kind == ShellSidebarKind::Inbox {
            bucket = SidebarBucket::Inbox;
            continue;
        }
        if sidebar_row_is_section_chrome(row) {
            continue;
        }
        let target = match row.kind {
            ShellSidebarKind::DropHint | ShellSidebarKind::Archived if grouped => {
                SidebarBucket::Project
            }
            _ => bucket,
        };
        match target {
            SidebarBucket::Session => sessions.push(row.clone()),
            SidebarBucket::Project => projects.push(row.clone()),
            SidebarBucket::Inbox => inbox.push(row.clone()),
        }
    }
    SidebarRowGroups {
        sessions,
        projects,
        inbox,
        grouped,
        inbox_expanded,
    }
}

fn mount_sidebar_section(
    context: &mut AppContext,
    document_id: DocumentId,
    title: &str,
    empty: Option<&str>,
    tool: Option<Entity<ActionMenu>>,
) -> Result<(Entity<SidebarSection>, Entity<ListItem>, Entity<List>), FrameworkError> {
    let mut spec = SidebarSection::new(title);
    if let Some(empty) = empty {
        spec = spec.empty_text(empty);
    }
    let title_label = context.create_detached_component(document_id, spec.title_label())?;
    spec = spec.title_slot(title_label.stable_id());
    let header = context.create_detached_component(document_id, spec.header_item())?;
    context.append_child(header, title_label)?;
    if let Some(tool) = tool {
        context.append_child(header, tool)?;
    }
    let body = context.create_detached_component(document_id, SidebarSection::body_port())?;
    let section = context.create_detached_component(
        document_id,
        spec.header(header.stable_id()).body(body.stable_id()),
    )?;
    context.append_child(section, header)?;
    context.append_child(section, body)?;
    Ok((section, header, body))
}

fn sidebar_row_intent(row: &ShellSidebarRow) -> Option<ShellIntent> {
    match row.kind {
        ShellSidebarKind::Header if row.id == "projects-header" => {
            Some(ShellIntent::OpenProjectsOverview)
        }
        ShellSidebarKind::Header => None,
        ShellSidebarKind::DropHint => None,
        ShellSidebarKind::Empty => None,
        ShellSidebarKind::Project => Some(ShellIntent::ToggleSidebarProject(row.id.clone())),
        ShellSidebarKind::SearchProject => Some(ShellIntent::SelectProject(row.id.clone())),
        ShellSidebarKind::Task | ShellSidebarKind::SearchTask | ShellSidebarKind::Running => {
            TaskId::new(&row.id).ok().map(ShellIntent::SelectTask)
        }
        ShellSidebarKind::Inbox => Some(ShellIntent::ToggleSidebarInbox),
        ShellSidebarKind::Reveal if row.id == "inbox-reveal" => {
            Some(ShellIntent::RevealSidebarInbox)
        }
        ShellSidebarKind::Reveal => Some(ShellIntent::RevealSidebarProject(
            row.id.strip_prefix("reveal-").unwrap_or(&row.id).to_owned(),
        )),
        ShellSidebarKind::Archived => Some(ShellIntent::RestoreProject(row.id.clone())),
    }
}

struct SettingsAction {
    id: String,
    label: String,
    primary: bool,
    intent: ShellIntent,
}

fn settings_tab_copy(
    settings: &SettingsSnapshot,
) -> (String, String, Option<String>, bool, Vec<SettingsAction>) {
    match settings.state.active_tab().as_str() {
        "project" => (
            "项目".to_owned(),
            "保存当前项目名称和工作区路径。".to_owned(),
            settings.project_error.clone(),
            true,
            vec![
                SettingsAction {
                    id: "save-project".into(),
                    label: "保存项目".into(),
                    primary: true,
                    intent: ShellIntent::SaveProjectSettings,
                },
                SettingsAction {
                    id: "pick-workspace".into(),
                    label: "选择工作区".into(),
                    primary: false,
                    intent: ShellIntent::PickProjectWorkspace,
                },
            ],
        ),
        "provider" => (
            "模型服务".to_owned(),
            settings.provider_status.clone(),
            None,
            false,
            {
                let mut actions = vec![SettingsAction {
                    id: "refresh-provider".into(),
                    label: "刷新服务".into(),
                    primary: false,
                    intent: ShellIntent::RefreshProvider,
                }];
                if settings.can_save_credential {
                    actions.push(SettingsAction {
                        id: "save-credential".into(),
                        label: "保存凭据".into(),
                        primary: true,
                        intent: ShellIntent::SaveProviderCredential,
                    });
                }
                actions.push(SettingsAction {
                    id: "save-runtime".into(),
                    label: "保存运行配置".into(),
                    primary: false,
                    intent: ShellIntent::SaveProviderRuntimeSettings,
                });
                actions.push(SettingsAction {
                    id: "reset-runtime".into(),
                    label: "恢复默认配置".into(),
                    primary: false,
                    intent: ShellIntent::ResetProviderRuntimeSettings,
                });
                actions
            },
        ),
        "agent" => ("Agent".to_owned(), String::new(), None, false, {
            let mut actions: Vec<_> = settings
                .agent_actions
                .iter()
                .map(|action| SettingsAction {
                    id: action.id.clone(),
                    label: action.label.clone(),
                    primary: false,
                    intent: ShellIntent::ToggleAgent(action.id.clone()),
                })
                .collect();
            actions.push(SettingsAction {
                id: "new-agent".into(),
                label: "新建 Agent".into(),
                primary: true,
                intent: ShellIntent::NewCustomAgent,
            });
            if settings.custom_agent_editor_open {
                actions.push(SettingsAction {
                    id: "save-agent".into(),
                    label: "保存 Agent".into(),
                    primary: true,
                    intent: ShellIntent::SaveCustomAgent,
                });
                actions.push(SettingsAction {
                    id: "cancel-agent".into(),
                    label: "取消编辑".into(),
                    primary: false,
                    intent: ShellIntent::CancelCustomAgentEdit,
                });
            }
            actions
        }),
        "quota" => (
            "用量与额度".to_owned(),
            settings.quota_status.clone(),
            None,
            false,
            vec![
                SettingsAction {
                    id: "refresh-quota".into(),
                    label: "刷新用量".into(),
                    primary: false,
                    intent: ShellIntent::RefreshQuota,
                },
                SettingsAction {
                    id: "cycle-quota-days".into(),
                    label: settings.quota_days_label.clone(),
                    primary: false,
                    intent: ShellIntent::CycleQuotaDays,
                },
                SettingsAction {
                    id: "cycle-quota-backend".into(),
                    label: settings.quota_backend_label.clone(),
                    primary: false,
                    intent: ShellIntent::CycleQuotaBackend,
                },
            ],
        ),
        "extensions" => (
            "扩展".to_owned(),
            settings.extensions_status.clone(),
            None,
            false,
            {
                let mut actions = vec![SettingsAction {
                    id: "refresh-extensions".into(),
                    label: "刷新扩展".into(),
                    primary: false,
                    intent: ShellIntent::RefreshExtensions,
                }];
                if settings.can_create_skill {
                    actions.push(SettingsAction {
                        id: "create-skill".into(),
                        label: "创建技能".into(),
                        primary: true,
                        intent: ShellIntent::CreateSkill,
                    });
                }
                actions.push(SettingsAction {
                    id: "new-mcp".into(),
                    label: "新建 MCP".into(),
                    primary: false,
                    intent: ShellIntent::NewMcpServer,
                });
                if let Some(editor) = &settings.mcp_editor {
                    actions.push(SettingsAction {
                        id: "cycle-mcp-transport".into(),
                        label: format!("传输：{}", editor.transport),
                        primary: false,
                        intent: ShellIntent::CycleMcpTransport,
                    });
                    actions.push(SettingsAction {
                        id: "toggle-mcp-enabled".into(),
                        label: if editor.enabled {
                            "MCP：开".into()
                        } else {
                            "MCP：关".into()
                        },
                        primary: false,
                        intent: ShellIntent::ToggleMcpEditorEnabled,
                    });
                    actions.push(SettingsAction {
                        id: "save-mcp".into(),
                        label: "保存 MCP".into(),
                        primary: true,
                        intent: ShellIntent::SaveMcpServer,
                    });
                    actions.push(SettingsAction {
                        id: "cancel-mcp".into(),
                        label: "取消编辑".into(),
                        primary: false,
                        intent: ShellIntent::CancelMcpEditor,
                    });
                }
                actions
            },
        ),
        "remote" => (
            "远程控制".to_owned(),
            settings.remote_status.clone(),
            None,
            false,
            Vec::new(),
        ),
        "desktop" => (
            "桌面".to_owned(),
            {
                let github = if settings.github_login.is_empty() {
                    format!("GitHub：{}", settings.github_state)
                } else {
                    format!(
                        "GitHub：{} · {}",
                        settings.github_state, settings.github_login
                    )
                };
                let shortcut = if settings.shortcut_capturing {
                    "快捷键：正在录制，按下组合键".to_owned()
                } else if settings.shortcut.is_empty() {
                    "快捷键：未设置".to_owned()
                } else {
                    format!(
                        "快捷键：{}{}",
                        settings.shortcut,
                        if settings.shortcut_registered {
                            "（已注册）"
                        } else {
                            ""
                        }
                    )
                };
                format!("{}\n{}\n{}", settings.desktop_status, github, shortcut)
            },
            None,
            false,
            {
                let mut actions = vec![SettingsAction {
                    id: "check-update".into(),
                    label: "检查更新".into(),
                    primary: false,
                    intent: ShellIntent::CheckForUpdate,
                }];
                if settings.github_busy {
                    actions.push(SettingsAction {
                        id: "cancel-github".into(),
                        label: "取消绑定".into(),
                        primary: false,
                        intent: ShellIntent::CancelGitHubBinding,
                    });
                } else if settings.github_can_bind {
                    actions.push(SettingsAction {
                        id: "bind-github".into(),
                        label: "绑定 GitHub".into(),
                        primary: true,
                        intent: ShellIntent::StartGitHubBinding,
                    });
                }
                actions.push(SettingsAction {
                    id: "record-shortcut".into(),
                    label: if settings.shortcut_capturing {
                        "录制中".into()
                    } else {
                        "录制快捷键".into()
                    },
                    primary: false,
                    intent: ShellIntent::BeginShortcutCapture,
                });
                actions.push(SettingsAction {
                    id: "save-shortcut".into(),
                    label: "保存快捷键".into(),
                    primary: false,
                    intent: ShellIntent::SaveShortcut,
                });
                actions.push(SettingsAction {
                    id: "clear-shortcut".into(),
                    label: "清空快捷键".into(),
                    primary: false,
                    intent: ShellIntent::ClearShortcut,
                });
                actions
            },
        ),
        "data" => (
            "数据迁移".to_owned(),
            settings.data_status.clone(),
            None,
            false,
            vec![
                SettingsAction {
                    id: "pick-import".into(),
                    label: "选择导入目录".into(),
                    primary: false,
                    intent: ShellIntent::PickDataImportSource,
                },
                SettingsAction {
                    id: "execute-import".into(),
                    label: "开始导入".into(),
                    primary: true,
                    intent: ShellIntent::ExecuteDataImport,
                },
                SettingsAction {
                    id: "reset-import".into(),
                    label: "重置".into(),
                    primary: false,
                    intent: ShellIntent::ResetDataImport,
                },
            ]
            .into_iter()
            .filter(|action| action.id != "execute-import" || settings.data_can_import)
            .collect(),
        ),
        _ => (String::new(), String::new(), None, false, Vec::new()),
    }
}

/// A projection with every field at rest.
///
/// Lives outside the test module because the UI module tests assert on which
/// fields a module writes, and that only means something against a baseline
/// where nothing is set. Not a `Default` impl: `SettingsModel` is a NanaUI type
/// and giving it a default is not this crate's call.
#[cfg(test)]
pub(crate) fn empty_snapshot() -> PrimaryShellSnapshot {
    PrimaryShellSnapshot {
            theme: ThemeMode::Light,
            title: "LiliaCode".to_owned(),
            title_parent: "LiliaCode".to_owned(),
            title_context: "今天想做什么？".to_owned(),
            heading: "今天想做什么？".to_owned(),
            error: None,
            settings_open: false,
            sidebar_collapsed: false,
            sidebar_search_open: false,
            sidebar_search_query: String::new(),
            provider_badge: "未连接".to_owned(),
            nav_items: Vec::new(),
            sidebar_rows: Vec::new(),
            sidebar_menu: Vec::new(),
            sidebar_menu_anchor: None,
            add_project_menu_open: false,
            workspace: WorkspaceModel::new(),
            tasks: Vec::new(),
            timeline: Vec::new(),
            composer: String::new(),
            composer_task_id: None,
            composer_revision: 0,
            composer_height: COMPOSER_MIN_HEIGHT,
            composer_placeholder: "输入消息".to_owned(),
            composer_disabled: true,
            can_send: false,
            can_interrupt: false,
            pending_blocks_send: false,
            clone_repository: String::new(),
            clone_parent: String::new(),
            milestone_title: String::new(),
            attachments: Vec::new(),
            plan_mode: false,
            goal_mode: false,
            permission_label: "询问".to_owned(),
            worktree_label: None,
            worktree_can_pick: false,
            suggestions: Vec::new(),
            suggestions_can_refresh: false,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected: 0,
            command_palette_items: Vec::new(),
            settings: {
                let model = SettingsModel::new(
                    "appearance",
                    [nana_ui::SettingsTab::new("appearance", "外观")],
                )
                .expect("settings model");
                let state = SettingsState::new(&model);
                SettingsSnapshot {
                    model,
                    state,
                    appearance: AppearanceSettings::default(),
                    material_status: String::new(),
                    project_name: String::new(),
                    project_workspace: String::new(),
                    project_error: None,
                    providers: Vec::new(),
                    provider_status: String::new(),
                    agent_actions: Vec::new(),
                    quota_status: String::new(),
                    extensions_status: String::new(),
                    remote_status: String::new(),
                    remote_host_enabled: false,
                    remote_keep_awake: false,
                    desktop_status: String::new(),
                    data_status: String::new(),
                    data_can_import: false,
                    provider_secret: String::new(),
                    provider_model: String::new(),
                    provider_openai_endpoint: String::new(),
                    provider_anthropic_endpoint: String::new(),
                    can_save_credential: false,
                    credentials: Vec::new(),
                    custom_agents: Vec::new(),
                    custom_agent_editor_open: false,
                    custom_agent_name: String::new(),
                    custom_agent_description: String::new(),
                    custom_agent_instruction: String::new(),
                    quota_days_label: String::new(),
                    quota_backend_label: String::new(),
                    quota_values: Vec::new(),
                    skills: Vec::new(),
                    skill_id: String::new(),
                    skill_description: String::new(),
                    can_create_skill: false,
                    mcp_servers: Vec::new(),
                    mcp_editor: None,
                    github_state: String::new(),
                    github_login: String::new(),
                    github_busy: false,
                    github_can_bind: false,
                    shortcut: String::new(),
                    shortcut_capturing: false,
                    shortcut_registered: false,
                }
            },
            document: None,
            files: None,
            terminal: None,
            markdown_preview: None,
            inspector_title: String::new(),
            inspector_body: String::new(),
            inspector_todos: Vec::new(),
            iab_url: String::new(),
            confirm: None,
            pending: None,
            slash_items: Vec::new(),
            mention_items: Vec::new(),
            timeline_can_load_earlier: false,
            composer_plus_open: false,
            project_page: None,
            project_page_title: String::new(),
            project_page_body: String::new(),
            project_cards: Vec::new(),
            roadmap_cards: Vec::new(),
            architecture_records: Vec::new(),
            architecture_graph: nana_ui::GraphModel::empty(),
            architecture_viewport: nana_ui::GraphViewport::default(),
            architecture_selection: None,
            inspector_kind: String::new(),
            coding: None,
            pane_can_move_window: false,
            pane_can_move_next: false,
            titlebar_menu_open: false,
            titlebar_has_task: false,
            titlebar_can_split: false,
            titlebar_can_close: false,
            automations_open: false,
            automations: Vec::new(),
            automation_graph: nana_ui::GraphModel::empty(),
            automation_viewport: nana_ui::GraphViewport::default(),
            automation_selection: None,
        panes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_layout::COMPOSER_CARD_RADIUS;

    fn snapshot_with_empty_primary_pane() -> PrimaryShellSnapshot {
        let mut snapshot = empty_snapshot();
        snapshot.panes = vec![ShellPaneRow {
            id: "primary".to_owned(),
            active: true,
            items: Vec::new(),
        }];
        snapshot
    }

    fn test_sidebar_row(id: &str, label: &str, kind: ShellSidebarKind) -> ShellSidebarRow {
        ShellSidebarRow {
            id: id.to_owned(),
            label: label.to_owned(),
            kind,
            selected: false,
            ancestor: false,
            depth: 0,
            expanded: None,
            icon: Icon::Folder,
            can_stop: false,
            can_menu: false,
            can_draft: false,
        }
    }

    fn mounted_primary(
        snapshot: &PrimaryShellSnapshot,
    ) -> (
        nana_ui::runtime::RuntimeDocument,
        ShellHandles,
        Option<StableNodeId>,
    ) {
        let (mut document, mut handles) =
            mount_primary_shell(snapshot, Arc::new(|_| {})).expect("mount shell");
        handles.sync(&mut document, snapshot).expect("sync shell");
        let primary = document
            .context_mut()
            .read(handles.shell, |shell| shell.primary)
            .expect("read shell primary");
        (document, handles, primary)
    }

    #[test]
    fn mounts_a_primary_shell_document() {
        let (document, _handles) =
            mount_primary_shell(&empty_snapshot(), Arc::new(|_| {})).expect("mount shell");
        assert_eq!(document.document(), DocumentId::new(1).unwrap());
    }

    #[test]
    fn default_empty_layout_selects_conversation_primary() {
        let (document, handles, primary) = mounted_primary(&snapshot_with_empty_primary_pane());
        assert_eq!(primary, Some(handles.conversation.stable_id()));
        assert_ne!(primary, Some(handles.workspace_page.stable_id()));

        let timeline = handles.timeline_scroll.stable_id();
        let heading = handles.heading_slot.stable_id();
        let body = document
            .context()
            .world()
            .node(timeline)
            .and_then(|node| node.parent)
            .expect("conversation body");
        assert_eq!(
            document
                .context()
                .world()
                .node(heading)
                .and_then(|node| node.parent),
            Some(body)
        );
        let body_layout = &document
            .context()
            .world()
            .node_style(body)
            .expect("conversation body style")
            .layout;
        assert_eq!(body_layout.flex_grow, Some(1.0));
        assert_eq!(
            body_layout.min_height,
            Some(nana_ui::runtime::LengthSpec::Px(0.0))
        );

        let extras = handles.extras.stable_id();
        let send = handles.send.stable_id();
        let dock = document
            .context()
            .world()
            .node(handles.composer.stable_id())
            .and_then(|node| node.parent)
            .expect("composer dock");
        assert_eq!(
            document
                .context()
                .world()
                .node(extras)
                .and_then(|node| node.parent),
            Some(handles.composer_toolbar.stable_id())
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(send)
                .and_then(|node| node.parent),
            Some(handles.composer_actions.stable_id())
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.composer_toolbar.stable_id())
                .and_then(|node| node.parent),
            Some(dock)
        );
        let dock_layout = &document
            .context()
            .world()
            .node_style(dock)
            .expect("composer dock style")
            .layout;
        assert_eq!(dock_layout.flex_grow, Some(0.0));
        assert_eq!(
            dock_layout.height,
            Some(nana_ui::runtime::LengthSpec::Shrink)
        );
        assert_eq!(dock_layout.border_radius, Some(COMPOSER_CARD_RADIUS));
        assert_eq!(
            document
                .context()
                .world()
                .node_style(dock)
                .expect("composer dock style")
                .background,
            Some(nana_ui::runtime::SemanticColorRole::Surface)
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.conversation.stable_id())
                .map(|node| node.children),
            Some(vec![handles.conversation_column.stable_id()])
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.conversation_column.stable_id())
                .map(|node| node.children),
            Some(vec![body, dock])
        );
    }

    #[test]
    fn open_document_selects_workspace_primary() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.document = Some(ShellDocumentSnapshot {
            item_id: "doc-1".to_owned(),
            title: "notes.md".to_owned(),
            text: String::new(),
            language: "markdown".to_owned(),
            status: String::new(),
            read_only: false,
            dirty: false,
            diagnostics: Vec::new(),
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "doc-1".to_owned(),
            title: "notes.md".to_owned(),
            kind: "document-editor".to_owned(),
            selected: true,
            closable: true,
        });
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.workspace_page.stable_id()));
    }

    #[test]
    fn open_files_selects_workspace_primary() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.files = Some(ShellFilesSnapshot {
            tree: TreeView::new(Vec::new()),
            preview: None,
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "files".to_owned(),
            title: "文件".to_owned(),
            kind: "project-files".to_owned(),
            selected: true,
            closable: true,
        });
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.workspace_page.stable_id()));
    }

    #[test]
    fn open_terminal_selects_workspace_primary() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.terminal = Some(ShellTerminalSnapshot {
            output: String::new(),
            input: String::new(),
            notice: None,
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "term".to_owned(),
            title: "终端".to_owned(),
            kind: "terminal".to_owned(),
            selected: true,
            closable: true,
        });
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.workspace_page.stable_id()));
    }

    #[test]
    fn pane_with_workspace_item_selects_workspace_primary() {
        let mut snapshot = empty_snapshot();
        snapshot.panes = vec![ShellPaneRow {
            id: "primary".to_owned(),
            active: true,
            items: vec![ShellPaneItem {
                id: "item-1".to_owned(),
                title: "会话".to_owned(),
                kind: "task".to_owned(),
                selected: true,
                closable: true,
            }],
        }];
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.conversation.stable_id()));
    }

    #[test]
    fn document_pane_item_selects_workspace_primary() {
        let mut snapshot = empty_snapshot();
        snapshot.document = Some(ShellDocumentSnapshot {
            item_id: "item-1".to_owned(),
            title: "main.rs".to_owned(),
            text: String::new(),
            language: "rust".to_owned(),
            status: String::new(),
            read_only: false,
            dirty: false,
            diagnostics: Vec::new(),
        });
        snapshot.panes = vec![ShellPaneRow {
            id: "primary".to_owned(),
            active: true,
            items: vec![ShellPaneItem {
                id: "item-1".to_owned(),
                title: "main.rs".to_owned(),
                kind: "document-editor".to_owned(),
                selected: true,
                closable: true,
            }],
        }];
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.workspace_page.stable_id()));
    }

    #[test]
    fn settings_open_selects_settings_primary() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.settings_open = true;
        let (_document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.settings_page.stable_id()));
    }

    #[test]
    fn automations_open_selects_automations_primary() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.automations_open = true;
        let (document, handles, primary) = mounted_primary(&snapshot);
        assert_eq!(primary, Some(handles.automations_page.stable_id()));
        let navigation = document
            .context()
            .read(handles.shell, |shell| shell.navigation)
            .expect("read navigation");
        assert_eq!(navigation, Some(handles.automations_sidebar.stable_id()));
    }

    #[test]
    fn conversation_column_keeps_chat_max_width() {
        let (document, handles, _primary) = mounted_primary(&snapshot_with_empty_primary_pane());
        let width = document
            .context()
            .world()
            .node_style(handles.conversation_column.stable_id())
            .expect("conversation column style")
            .layout
            .max_width;
        assert_eq!(
            width,
            Some(nana_ui::runtime::LengthSpec::Px(CHAT_CONTENT_MAX_WIDTH))
        );
    }

    #[test]
    fn composer_plus_menu_stays_in_the_toolbar() {
        let (document, handles, _primary) = mounted_primary(&snapshot_with_empty_primary_pane());
        let extras = document
            .context()
            .world()
            .node(handles.extras.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            extras,
            vec![
                handles.plus_slot.stable_id(),
                handles.attach.stable_id(),
                handles.permission_slot.stable_id()
            ]
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.plus_slot.stable_id())
                .map(|node| node.children.clone())
                .unwrap_or_default(),
            vec![handles.plus_menu.stable_id()]
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.permission_slot.stable_id())
                .map(|node| node.children.clone())
                .unwrap_or_default(),
            vec![
                handles.permission_icon.stable_id(),
                handles.permission.stable_id()
            ]
        );
        assert!(handles.plus_items.is_empty());
        let plus_children = document
            .context()
            .world()
            .node(handles.plus_menu.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert!(plus_children.is_empty());
        let actions = document
            .context()
            .world()
            .node(handles.composer_actions.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(actions, vec![handles.send.stable_id()]);
        assert!(handles.load_earlier.is_none());
        let body = document
            .context()
            .world()
            .node(handles.conversation_body.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            body,
            vec![
                handles.heading_slot.stable_id(),
                handles.error.stable_id(),
                handles.timeline_scroll.stable_id(),
            ]
        );
    }

    #[test]
    fn slash_items_mount_above_the_composer() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.slash_items = vec![ShellSlashItem {
            name: "status".to_owned(),
            label: "查看状态".to_owned(),
        }];
        let (document, handles, _primary) = mounted_primary(&snapshot);
        let extras = document
            .context()
            .world()
            .node(handles.extras.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            extras,
            vec![
                handles.plus_slot.stable_id(),
                handles.attach.stable_id(),
                handles.permission_slot.stable_id()
            ]
        );
        let dock = document
            .context()
            .world()
            .node(handles.composer.stable_id())
            .and_then(|node| node.parent)
            .expect("composer dock");
        assert_eq!(
            document
                .context()
                .world()
                .node(dock)
                .map(|node| node.children.clone())
                .unwrap_or_default(),
            vec![
                handles.completion_slot.stable_id(),
                handles.composer.stable_id(),
                handles.composer_toolbar.stable_id(),
            ]
        );
        assert_eq!(
            document
                .context()
                .world()
                .node(handles.completion_slot.stable_id())
                .map(|node| node.children.clone())
                .unwrap_or_default()
                .first()
                .copied(),
            handles
                .completion_items
                .get("slash-status")
                .map(|item| item.stable_id())
        );
    }

    fn composer_plus_items_mount_inside_the_open_menu() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.composer_plus_open = true;
        let (document, handles, _primary) = mounted_primary(&snapshot);
        let extras = document
            .context()
            .world()
            .node(handles.extras.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            extras,
            vec![
                handles.plus_slot.stable_id(),
                handles.attach.stable_id(),
                handles.permission_slot.stable_id()
            ]
        );
        let plus_children = document
            .context()
            .world()
            .node(handles.plus_menu.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(plus_children.len(), plus_menu_items(&snapshot).len());
        assert_eq!(
            plus_children.first().copied(),
            handles
                .plus_items
                .get("add-file")
                .map(|item| item.stable_id())
        );
    }

    #[test]
    fn add_project_menu_stays_on_the_section_header() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.add_project_menu_open = true;
        snapshot.sidebar_menu_anchor = Some((24.0, 96.0));
        snapshot.sidebar_menu = vec![ShellMenuItem {
            id: "add-local-folder".to_owned(),
            label: "使用本地文件夹".to_owned(),
        }];
        let (mut document, handles, _primary) = mounted_primary(&snapshot);
        let header_children = document
            .context()
            .world()
            .node(handles.project_header.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert!(header_children.contains(&handles.add_project_menu.stable_id()));
        assert!(document
            .context()
            .world()
            .node(handles.add_project_menu.stable_id())
            .is_some_and(|node| node.children.is_empty()));
        let more_menu = handles.more_menu.expect("add-project items use the overlay");
        assert_eq!(
            document
                .context_mut()
                .read(more_menu, |menu| menu.items[0].value.to_string())
                .expect("read overlay menu"),
            "add-local-folder"
        );
    }

    #[test]
    fn sidebar_new_conversation_lives_in_the_top_slot() {
        let (document, handles, _primary) = mounted_primary(&snapshot_with_empty_primary_pane());
        let children = document
            .context()
            .world()
            .node(handles.sidebar_top.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            children.first().copied(),
            Some(handles.new_conversation.stable_id())
        );
    }

    #[test]
    fn an_enabled_composer_accepts_pointer_and_focus() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.composer_disabled = false;
        let (mut document, handles, _primary) = mounted_primary(&snapshot);
        let disabled = document
            .context_mut()
            .read(handles.composer, |composer| composer.disabled)
            .expect("read composer");
        assert!(!disabled);
        assert_eq!(
            handles
                .focus_targets
                .get(target_ids::COMPOSER_INPUT)
                .copied(),
            Some(handles.composer.stable_id())
        );
    }

    #[test]
    fn focused_composer_keeps_cleared_text_until_revision_changes() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.composer = "a".to_owned();
        snapshot.composer_revision = 1;
        snapshot.composer_disabled = false;
        let (mut document, mut handles, _primary) = mounted_primary(&snapshot);

        let composer_id = handles.composer.stable_id();
        let document_id = document.document();
        document
            .context_mut()
            .focus_node(document_id, composer_id)
            .expect("focus composer");
        document
            .context_mut()
            .update_component(handles.composer, |composer, _| {
                composer.state.replace_value(String::new());
            })
            .expect("clear composer");

        handles
            .sync(&mut document, &snapshot)
            .expect("sync stale snapshot");
        let after_stale = document
            .context()
            .read(handles.composer, |composer| composer.state.value.clone())
            .expect("read composer");
        assert_eq!(after_stale, "");

        snapshot.composer = "@file".to_owned();
        snapshot.composer_revision = 2;
        handles
            .sync(&mut document, &snapshot)
            .expect("sync revision bump");
        let after_revision = document
            .context()
            .read(handles.composer, |composer| composer.state.value.clone())
            .expect("read composer");
        assert_eq!(after_revision, "@file");
    }

    #[test]
    fn empty_session_sidebar_uses_section_empty_state() {
        let (document, handles, _primary) = mounted_primary(&snapshot_with_empty_primary_pane());
        let empty_text = document
            .context()
            .read(handles.conversation_section, |section| {
                section.empty_text.clone()
            })
            .expect("read session section");
        assert_eq!(empty_text.as_deref(), Some(SESSIONS_EMPTY_TEXT));
        let children = document
            .context()
            .world()
            .node(handles.task_body.stable_id())
            .map(|node| node.children.len())
            .unwrap_or(usize::MAX);
        assert_eq!(children, 0);
    }

    #[test]
    fn session_rows_replace_sidebar_empty_state() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.tasks = vec![ShellTaskRow {
            id: TaskId::new("task-1").expect("task id"),
            title: "设计稿".to_owned(),
            selected: true,
        }];
        let (document, handles, _primary) = mounted_primary(&snapshot);
        let children = document
            .context()
            .world()
            .node(handles.task_body.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(children.len(), 1);
        assert_eq!(
            children[0],
            handles
                .task_rows
                .get("task-1")
                .expect("task row")
                .stable_id()
        );
    }

    #[test]
    fn grouped_sidebar_mounts_projects_and_inbox_only() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.sidebar_rows = vec![
            test_sidebar_row("projects-header", "项目", ShellSidebarKind::Header),
            test_sidebar_row("inbox", "收集箱", ShellSidebarKind::Inbox),
        ];
        let (document, handles, _primary) = mounted_primary(&snapshot);
        let project_empty = document
            .context()
            .read(handles.project_section, |section| {
                section.empty_text.clone()
            })
            .expect("read project section");
        assert_eq!(project_empty.as_deref(), Some(PROJECTS_EMPTY_TEXT));
        let inbox_empty = document
            .context()
            .read(handles.inbox_section, |section| section.empty_text.clone())
            .expect("read inbox section");
        assert_eq!(inbox_empty.as_deref(), Some(INBOX_EMPTY_TEXT));
        let scroll = document
            .context()
            .world()
            .node(handles.sidebar_scroll.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(
            scroll,
            vec![
                handles.project_section.stable_id(),
                handles.inbox_section.stable_id(),
            ]
        );
        assert_eq!(
            handles
                .focus_targets
                .get(target_ids::SIDEBAR_PROJECTS_OVERVIEW)
                .copied(),
            Some(handles.project_header.stable_id())
        );
    }

    #[test]
    fn partition_sidebar_keeps_drop_hint_and_archived_with_projects() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.sidebar_rows = vec![
            test_sidebar_row("drop-hint", "松开以添加项目", ShellSidebarKind::DropHint),
            test_sidebar_row("projects-header", "项目", ShellSidebarKind::Header),
            test_sidebar_row("proj-1", "Demo", ShellSidebarKind::Project),
            test_sidebar_row("inbox", "收集箱", ShellSidebarKind::Inbox),
            test_sidebar_row("inbox-task", "未绑定", ShellSidebarKind::Task),
            test_sidebar_row("archived-1", "恢复 · 旧项目", ShellSidebarKind::Archived),
        ];
        let groups = partition_sidebar_rows(&snapshot);
        assert!(groups.grouped);
        assert_eq!(
            groups
                .projects
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["drop-hint", "proj-1", "archived-1"]
        );
        assert_eq!(
            groups
                .inbox
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["inbox-task"]
        );
        assert_eq!(sidebar_project_entry_count(&groups.projects), 2);
    }

    #[test]
    fn grouped_sidebar_project_count_excludes_nested_sessions() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        let mut project = test_sidebar_row("proj-1", "Demo", ShellSidebarKind::Project);
        project.expanded = Some(true);
        let mut nested = test_sidebar_row("task-1", "会话", ShellSidebarKind::Task);
        nested.depth = 1;
        snapshot.sidebar_rows = vec![
            test_sidebar_row("projects-header", "项目", ShellSidebarKind::Header),
            project,
            nested,
            test_sidebar_row("reveal-proj-1", "…", ShellSidebarKind::Reveal),
            test_sidebar_row("inbox", "收集箱", ShellSidebarKind::Inbox),
        ];
        let (document, handles, _primary) = mounted_primary(&snapshot);
        let count = document
            .context()
            .read(handles.project_section, |section| section.count)
            .expect("read project count");
        assert_eq!(count, Some(1));
        let body = document
            .context()
            .world()
            .node(handles.project_body.stable_id())
            .map(|node| node.children.len())
            .unwrap_or(0);
        assert_eq!(body, 3);
    }

    #[test]
    fn grouped_sidebar_rebuilds_row_when_kind_changes() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        let mut project = test_sidebar_row("proj-1", "Demo", ShellSidebarKind::Project);
        project.expanded = Some(true);
        project.can_menu = true;
        project.can_draft = true;
        snapshot.sidebar_rows = vec![
            test_sidebar_row("projects-header", "项目", ShellSidebarKind::Header),
            project,
            test_sidebar_row("inbox", "收集箱", ShellSidebarKind::Inbox),
        ];
        let (mut document, mut handles, _primary) = mounted_primary(&snapshot);
        let original = handles
            .task_rows
            .get("proj-1")
            .map(|row| row.stable_id())
            .expect("project row");
        assert!(handles.row_tools.contains_key("proj-1"));

        snapshot.sidebar_rows = vec![
            test_sidebar_row("projects-header", "项目", ShellSidebarKind::Header),
            test_sidebar_row("inbox", "收集箱", ShellSidebarKind::Inbox),
            test_sidebar_row("proj-1", "恢复 · Demo", ShellSidebarKind::Archived),
        ];
        handles.sync(&mut document, &snapshot).expect("resync");
        let rebuilt = handles
            .task_rows
            .get("proj-1")
            .map(|row| row.stable_id())
            .expect("archived row");
        assert_ne!(original, rebuilt);
        assert!(!handles.row_tools.contains_key("proj-1"));
        let tools = document
            .context()
            .read(
                *handles
                    .task_rows
                    .get("proj-1")
                    .expect("archived row entity"),
                |row| row.tools,
            )
            .expect("read tools");
        assert_eq!(tools, None);
        let project_children = document
            .context()
            .world()
            .node(handles.project_body.stable_id())
            .map(|node| node.children.clone())
            .unwrap_or_default();
        assert_eq!(project_children, vec![rebuilt]);
    }

    #[test]
    fn workspace_tabs_close_and_transfer_emit_intents() {
        assert!(matches!(
            workspace_tabs_intent(&TabsEvent::Close(Arc::from("doc-1"))),
            ShellIntent::ClosePaneTab { item_id, .. } if item_id == "doc-1"
        ));
        assert!(matches!(
            workspace_tabs_intent(&TabsEvent::Transfer {
                source_strip: Arc::from("workspace/main/pane/a"),
                value: Arc::from("doc-1"),
                target_strip: Arc::from("workspace/main/pane/b"),
                before: None,
            }),
            ShellIntent::TransferPaneTab { item_id, .. } if item_id == "doc-1"
        ));
    }

    #[test]
    fn document_editor_requests_syntax_highlight() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.document = Some(ShellDocumentSnapshot {
            item_id: "doc-1".to_owned(),
            title: "main.rs".to_owned(),
            text: "fn main() {}".to_owned(),
            language: "rust".to_owned(),
            status: String::new(),
            read_only: false,
            dirty: false,
            diagnostics: Vec::new(),
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "doc-1".to_owned(),
            title: "main.rs".to_owned(),
            kind: "document-editor".to_owned(),
            selected: true,
            closable: true,
        });
        let (mut document, handles, _) = mounted_primary(&snapshot);
        let language = document
            .context_mut()
            .read(handles.workspace_editor, |editor| {
                editor
                    .highlight
                    .as_ref()
                    .map(|request| request.language.to_string())
            })
            .expect("read editor");
        assert_eq!(language.as_deref(), Some("rust"));
    }

    #[test]
    fn terminal_pane_uses_plain_log_not_document_editor() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.terminal = Some(ShellTerminalSnapshot {
            output: "$ ls".to_owned(),
            input: String::new(),
            notice: None,
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "term".to_owned(),
            title: "终端".to_owned(),
            kind: "terminal".to_owned(),
            selected: true,
            closable: true,
        });
        let (mut document, handles, _) = mounted_primary(&snapshot);
        let children = document
            .context()
            .world()
            .node(handles.workspace_content.stable_id())
            .map(|node| node.children.clone())
            .expect("workspace children");
        assert!(children.contains(&handles.workspace_log.stable_id()));
        assert!(!children.contains(&handles.workspace_editor.stable_id()));
        let (log_value, log_highlight, log_disabled) = document
            .context_mut()
            .read(handles.workspace_log, |log| {
                (
                    log.state.value.clone(),
                    log.highlight
                        .as_ref()
                        .map(|request| request.language.to_string()),
                    log.disabled,
                )
            })
            .expect("read terminal log");
        assert_eq!(log_value, "$ ls");
        assert_eq!(log_highlight, None);
        assert!(log_disabled);
    }

    #[test]
    fn diagnostics_attach_bottom_slot() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.document = Some(ShellDocumentSnapshot {
            item_id: "doc-1".to_owned(),
            title: "main.rs".to_owned(),
            text: String::new(),
            language: "rust".to_owned(),
            status: String::new(),
            read_only: false,
            dirty: false,
            diagnostics: vec![ShellDiagnosticRow {
                severity: "错误".to_owned(),
                message: "unused".to_owned(),
            }],
        });
        snapshot.panes[0].items.push(ShellPaneItem {
            id: "doc-1".to_owned(),
            title: "main.rs".to_owned(),
            kind: "document-editor".to_owned(),
            selected: true,
            closable: true,
        });
        let (mut document, handles, _) = mounted_primary(&snapshot);
        let bottom = document
            .context_mut()
            .read(handles.shell, |shell| shell.bottom)
            .expect("read bottom");
        assert_eq!(bottom, Some(handles.diagnostics_panel.stable_id()));
    }

    #[test]
    fn provider_settings_use_form_fields() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.settings_open = true;
        let model = SettingsModel::new(
            "provider",
            [nana_ui::SettingsTab::new("provider", "模型服务")],
        )
        .expect("settings model");
        snapshot.settings.state = SettingsState::new(&model);
        snapshot.settings.model = model;
        snapshot.settings.provider_secret = "secret".to_owned();
        let (_document, handles, _) = mounted_primary(&snapshot);
        assert!(handles.form_wrappers.contains_key("provider_secret"));
        assert!(handles.form_switches.is_empty());
    }

    #[test]
    fn remote_settings_use_switches() {
        let mut snapshot = snapshot_with_empty_primary_pane();
        snapshot.settings_open = true;
        let model = SettingsModel::new("remote", [nana_ui::SettingsTab::new("remote", "远程控制")])
            .expect("settings model");
        snapshot.settings.state = SettingsState::new(&model);
        snapshot.settings.model = model;
        snapshot.settings.remote_host_enabled = true;
        let (_document, handles, _) = mounted_primary(&snapshot);
        assert!(handles.form_switches.contains_key("remote_host"));
        assert!(handles.form_switches.contains_key("remote_keep_awake"));
    }
}
