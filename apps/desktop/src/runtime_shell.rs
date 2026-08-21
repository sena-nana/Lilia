use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lilia_contracts::TaskId;
use nana_ui::runtime::{
    AboutMetadata, AboutSection, Activate, AppearanceSection, AppContext, Button, CommandPalette,
    DesktopShell, DocumentId, Entity, FrameworkError, GraphCanvas, IconButton, List, NativeMarkdown,
    OverlayHost, TimeSeriesChart,
    ScrollAxes, ScrollOffset, ScrollView, SettingsBack, SettingsPage, SettingsSidebar,
    SettingsTabSelected, SidebarFooter, SidebarFooterButton, SidebarFrame, SidebarRow,
    SidebarRowIcon, SidebarRowState, SidebarSection, StableNodeId, Text, TextArea, TextChanged,
    TreeView, TreeViewEvent, View,
};
use nana_ui::{
    platform_material_support, AppearanceEvent, AppearanceSettings, ButtonKind,
    CommandPaletteEvent, CommandPaletteItem, ControlSize, Icon, SettingsModel, SettingsState,
    SettingsTabId, ThemeMode, WindowChrome, WindowChromeAction, WindowChromeEvent, WorkspaceModel,
};

use crate::runtime_compat::{HostedUiCommand, HostedWindowId};
use crate::runtime_layout::{reconcile_children, HostStack};
use crate::target_ids;

const PRIMARY_DOCUMENT: u64 = 1;

#[derive(Debug, Clone)]
pub struct ShellTaskRow {
    pub id: TaskId,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ShellTimelineRow {
    pub id: String,
    pub markdown: String,
}

#[derive(Debug, Clone)]
pub struct ShellAttachmentRow {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct ShellSuggestionRow {
    pub id: String,
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct ShellActionRow {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct ShellProviderRow {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ShellCredentialRow {
    pub id: String,
    pub revision: u64,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct ShellAgentRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ShellSkillRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ShellMcpRow {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ShellMcpEditor {
    pub server_id: String,
    pub transport: String,
    pub location: String,
    pub args: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ShellPaneItem {
    pub id: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ShellPaneRow {
    pub id: String,
    pub active: bool,
    pub items: Vec<ShellPaneItem>,
}

#[derive(Debug, Clone)]
pub struct ShellAutomationRow {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct ShellDocumentSnapshot {
    pub item_id: String,
    pub title: String,
    pub text: String,
    pub status: String,
    pub read_only: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct ShellTerminalSnapshot {
    pub output: String,
    pub input: String,
    pub notice: Option<String>,
}

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone)]
pub struct PrimaryShellSnapshot {
    pub theme: ThemeMode,
    pub title: String,
    pub heading: String,
    pub empty_hint: String,
    pub error: Option<String>,
    pub settings_open: bool,
    pub sidebar_collapsed: bool,
    pub workspace: WorkspaceModel,
    pub tasks: Vec<ShellTaskRow>,
    pub timeline: Vec<ShellTimelineRow>,
    pub composer: String,
    pub composer_placeholder: String,
    pub composer_disabled: bool,
    pub can_send: bool,
    pub can_interrupt: bool,
    pub attachments: Vec<ShellAttachmentRow>,
    pub plan_mode: bool,
    pub goal_mode: bool,
    pub permission_label: String,
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
    pub inspector_title: String,
    pub inspector_body: String,
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
    PickAttachmentFiles,
    PickAttachmentDirectories,
    RemoveAttachment(String),
    TogglePlanMode,
    ToggleGoalMode,
    CyclePermission,
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
    RevokeProviderCredential { credential_id: String, revision: u64 },
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

pub struct ShellHandles {
    sink: IntentSink,
    shell: Entity<DesktopShell>,
    overlay_host: Option<Entity<OverlayHost>>,
    palette: Option<Entity<CommandPalette>>,
    more_menu: Option<Entity<HostStack>>,
    sidebar_toggle: Entity<IconButton>,
    search_button: Entity<IconButton>,
    more_button: Entity<IconButton>,
    form_fields: HashMap<String, Entity<TextArea>>,
    quota_chart: Option<Entity<TimeSeriesChart>>,
    pane_bar: Entity<HostStack>,
    pane_buttons: HashMap<String, Entity<Button>>,
    automations_page: Entity<HostStack>,
    automation_list: Entity<HostStack>,
    automation_actions: Entity<HostStack>,
    automation_canvas: Entity<GraphCanvas>,
    title_center: Entity<Text>,
    title_leading: Entity<HostStack>,
    title_trailing: Entity<HostStack>,
    conversation_sidebar: Entity<SidebarFrame>,
    task_body: Entity<List>,
    task_rows: HashMap<String, Entity<SidebarRow>>,
    conversation: Entity<HostStack>,
    settings_sidebar: Entity<SettingsSidebar>,
    settings_page: Entity<SettingsPage>,
    appearance: Entity<AppearanceSection>,
    about: Entity<AboutSection>,
    product_settings: Entity<HostStack>,
    product_heading: Entity<Text>,
    product_body: Entity<Text>,
    product_error: Entity<Text>,
    project_name: Entity<TextArea>,
    project_workspace: Entity<Text>,
    product_actions: HashMap<String, Entity<Button>>,
    provider_rows: HashMap<String, Entity<Button>>,
    heading: Entity<Text>,
    empty_hint: Entity<Text>,
    error: Entity<Text>,
    timeline_scroll: Entity<ScrollView>,
    timeline_items: HashMap<String, Entity<NativeMarkdown>>,
    composer: Entity<TextArea>,
    extras: Entity<HostStack>,
    extra_buttons: HashMap<String, Entity<Button>>,
    send: Entity<Button>,
    interrupt: Entity<Button>,
    workspace_page: Entity<HostStack>,
    workspace_heading: Entity<Text>,
    workspace_status: Entity<Text>,
    workspace_editor: Entity<TextArea>,
    workspace_input: Entity<TextArea>,
    workspace_actions: Entity<HostStack>,
    workspace_buttons: HashMap<String, Entity<Button>>,
    workspace_tree: Entity<TreeView>,
    inspector: Entity<HostStack>,
    inspector_heading: Entity<Text>,
    inspector_body: Entity<Text>,
    focus_targets: HashMap<String, StableNodeId>,
}

pub(crate) fn emit(sink: &IntentSink, intent: ShellIntent) {
    sink(intent);
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

fn send_button(enabled: bool) -> Button {
    Button::new("发送")
        .kind(ButtonKind::Primary)
        .size(ControlSize::Medium)
        .disabled(!enabled)
}

fn interrupt_button(enabled: bool) -> Button {
    Button::new("停止")
        .kind(ButtonKind::Danger)
        .size(ControlSize::Medium)
        .disabled(!enabled)
}

fn window_control(icon: Icon, label: &'static str) -> IconButton {
    IconButton::new(icon, label)
}

fn search_button() -> IconButton {
    IconButton::new(Icon::Search, "搜索命令")
        .size(ControlSize::Small)
        .kind(ButtonKind::Text)
}

fn more_button() -> IconButton {
    IconButton::new(Icon::ChevronDown, "更多")
        .size(ControlSize::Small)
        .kind(ButtonKind::Text)
}

fn extra_button(label: &str, kind: ButtonKind) -> Button {
    Button::new(label).kind(kind).size(ControlSize::Small)
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

    let title_center =
        context.create_detached_component(document_id, Text::new(snapshot.title.clone()))?;
    let title_trailing = context.create_detached_component(document_id, HostStack::row(6.0))?;
    let search = context.create_detached_component(document_id, search_button())?;
    context.append_child(title_trailing, search)?;
    bind_activate(
        context,
        search,
        Arc::clone(&sink),
        ShellIntent::ToggleCommandPalette,
    )?;
    let more = context.create_detached_component(document_id, more_button())?;
    context.append_child(title_trailing, more)?;
    bind_activate(
        context,
        more,
        Arc::clone(&sink),
        ShellIntent::ToggleTitlebarMenu,
    )?;
    if WindowChrome::platform_default().uses_custom_controls() {
        let minimize = context
            .create_detached_component(document_id, window_control(Icon::Minimize, "最小化"))?;
        let maximize = context
            .create_detached_component(document_id, window_control(Icon::Maximize, "最大化"))?;
        let close =
            context.create_detached_component(document_id, window_control(Icon::Close, "关闭"))?;
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

    let mut spec = nana_ui::runtime::SidebarSection::new("会话").count(snapshot.tasks.len());
    let section_title = context.create_detached_component(document_id, spec.title_label())?;
    spec = spec.title_slot(section_title.stable_id());
    let header = context.create_detached_component(document_id, spec.header_item())?;
    context.append_child(header, section_title)?;
    let task_body = context.create_detached_component(document_id, SidebarSection::body_port())?;
    let section = context.create_detached_component(
        document_id,
        spec.header(header.stable_id()).body(task_body.stable_id()),
    )?;
    context.append_child(section, header)?;
    context.append_child(section, task_body)?;
    let scroll =
        context.create_detached_component(document_id, SidebarFrame::vertical_body_scroll())?;
    context.append_child(scroll, section)?;
    let footer = context.create_detached_component(document_id, SidebarFooter::new())?;
    let new_conversation = context
        .create_detached_component(document_id, SidebarFooterButton::new("新会话", Icon::Add))?;
    let settings = context.create_detached_component(
        document_id,
        SidebarFooterButton::new("设置", Icon::Settings),
    )?;
    context.append_child(footer, new_conversation)?;
    context.append_child(footer, settings)?;
    bind_activate(
        context,
        new_conversation,
        Arc::clone(&sink),
        ShellIntent::NewConversation,
    )?;
    bind_activate(
        context,
        settings,
        Arc::clone(&sink),
        ShellIntent::OpenSettings,
    )?;
    let conversation_sidebar = context.create_detached_component(
        document_id,
        SidebarFrame::new()
            .body(scroll.stable_id())
            .footer(footer.stable_id()),
    )?;
    context.append_child(conversation_sidebar, scroll)?;
    context.append_child(conversation_sidebar, footer)?;

    let conversation = context
        .create_detached_component(document_id, HostStack::fill_column(12.0).padding(16.0))?;
    let heading =
        context.create_detached_component(document_id, Text::new(snapshot.heading.clone()))?;
    let empty_hint =
        context.create_detached_component(document_id, Text::new(snapshot.empty_hint.clone()))?;
    let error = context.create_detached_component(
        document_id,
        Text::new(snapshot.error.clone().unwrap_or_default()),
    )?;
    let timeline_scroll =
        context.create_detached_component(document_id, ScrollView::new(ScrollAxes::Vertical))?;
    let composer = context.create_detached_component(
        document_id,
        TextArea::new(snapshot.composer.clone())
            .placeholder(snapshot.composer_placeholder.clone())
            .disabled(snapshot.composer_disabled)
            .height(96.0),
    )?;
    let composer_sink = Arc::clone(&sink);
    context.on(composer, move |_, event: &TextChanged, _| {
        emit(
            &composer_sink,
            ShellIntent::ComposerChanged(event.value.clone()),
        );
    })?;
    let extras = context.create_detached_component(document_id, HostStack::fill_row(8.0))?;
    let actions = context.create_detached_component(document_id, HostStack::fill_row(8.0))?;
    let send = context.create_detached_component(document_id, send_button(snapshot.can_send))?;
    let interrupt =
        context.create_detached_component(document_id, interrupt_button(snapshot.can_interrupt))?;
    bind_activate(context, send, Arc::clone(&sink), ShellIntent::SubmitTurn)?;
    bind_activate(
        context,
        interrupt,
        Arc::clone(&sink),
        ShellIntent::InterruptTurn,
    )?;
    context.append_child(actions, send)?;
    context.append_child(actions, interrupt)?;
    context.append_child(conversation, heading)?;
    context.append_child(conversation, empty_hint)?;
    context.append_child(conversation, error)?;
    context.append_child(conversation, timeline_scroll)?;
    context.append_child(conversation, composer)?;
    context.append_child(conversation, extras)?;
    context.append_child(conversation, actions)?;

    let settings_sidebar = context.create_detached_component(
        document_id,
        SettingsSidebar::new(snapshot.settings.model.clone(), snapshot.settings.state.clone()),
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
            .platform_hint(platform_material_support().hint())
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
    let product_heading = context.create_detached_component(document_id, Text::new(String::new()))?;
    let product_body = context.create_detached_component(document_id, Text::new(String::new()))?;
    let product_error = context.create_detached_component(document_id, Text::new(String::new()))?;
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
    let project_workspace = context.create_detached_component(
        document_id,
        Text::new(snapshot.settings.project_workspace.clone()),
    )?;
    context.append_child(product_settings, product_heading)?;
    context.append_child(product_settings, product_body)?;
    context.append_child(product_settings, product_error)?;
    context.append_child(product_settings, project_name)?;
    context.append_child(product_settings, project_workspace)?;
    let settings_page = context.create_detached_component(
        document_id,
        SettingsPage::new(snapshot.settings.model.clone(), snapshot.settings.state.clone())
            .content(appearance.stable_id()),
    )?;

    let workspace_page = context
        .create_detached_component(document_id, HostStack::fill_column(12.0).padding(16.0))?;
    let pane_bar = context.create_detached_component(document_id, HostStack::leading_row(8.0))?;
    context.append_child(workspace_page, pane_bar)?;
    let workspace_heading =
        context.create_detached_component(document_id, Text::new(String::new()))?;
    let workspace_status = context.create_detached_component(document_id, Text::new(String::new()))?;
    let workspace_editor = context.create_detached_component(
        document_id,
        TextArea::new(String::new()).height(420.0),
    )?;
    let workspace_editor_sink = Arc::clone(&sink);
    context.on(workspace_editor, move |_, event: &TextChanged, _| {
        emit(
            &workspace_editor_sink,
            ShellIntent::DocumentChanged(event.value.clone()),
        );
    })?;
    let workspace_tree =
        context.create_detached_component(document_id, TreeView::new(Vec::new()))?;
    let tree_sink = Arc::clone(&sink);
    context.on(workspace_tree, move |_, event: &TreeViewEvent<Arc<str>>, _| {
        match event {
            TreeViewEvent::Toggle(path) => {
                emit(&tree_sink, ShellIntent::ToggleProjectFile(path.to_string()));
            }
            TreeViewEvent::Select(path) => {
                emit(&tree_sink, ShellIntent::OpenProjectFile(path.to_string()));
            }
        }
    })?;
    let workspace_input = context.create_detached_component(
        document_id,
        TextArea::new(String::new()).height(72.0),
    )?;
    let workspace_input_sink = Arc::clone(&sink);
    context.on(workspace_input, move |_, event: &TextChanged, _| {
        emit(
            &workspace_input_sink,
            ShellIntent::TerminalInput(event.value.clone()),
        );
    })?;
    let workspace_actions =
        context.create_detached_component(document_id, HostStack::row(8.0))?;
    context.append_child(workspace_page, workspace_heading)?;
    context.append_child(workspace_page, workspace_status)?;
    context.append_child(workspace_page, workspace_tree)?;
    context.append_child(workspace_page, workspace_editor)?;
    context.append_child(workspace_page, workspace_input)?;
    context.append_child(workspace_page, workspace_actions)?;

    let inspector = context
        .create_detached_component(document_id, HostStack::fill_column(8.0).padding(12.0))?;
    let inspector_heading =
        context.create_detached_component(document_id, Text::new(snapshot.inspector_title.clone()))?;
    let inspector_body =
        context.create_detached_component(document_id, Text::new(snapshot.inspector_body.clone()))?;
    context.append_child(inspector, inspector_heading)?;
    context.append_child(inspector, inspector_body)?;

    let automations_page =
        context.create_detached_component(document_id, HostStack::fill_column(10.0).padding(16.0))?;
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

    let navigation = if snapshot.settings_open {
        settings_sidebar.stable_id()
    } else {
        conversation_sidebar.stable_id()
    };
    let primary = primary_content_id(
        snapshot,
        conversation,
        settings_page,
        workspace_page,
        automations_page,
    );
    let shell = context.create_component(
        document_id,
        DesktopShell::from_model(snapshot.workspace.clone())
            .title(snapshot.title.clone())
            .title_leading(title_leading.stable_id())
            .title_center(title_center.stable_id())
            .title_trailing(title_trailing.stable_id())
            .navigation(navigation)
            .primary(primary)
            .inspector(inspector.stable_id()),
    )?;
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
        sidebar_toggle,
        search_button: search,
        title_center,
        title_leading,
        title_trailing,
        conversation_sidebar,
        task_body,
        task_rows: HashMap::new(),
        conversation,
        settings_sidebar,
        settings_page,
        appearance,
        about,
        product_settings,
        product_heading,
        product_body,
        product_error,
        project_name,
        project_workspace,
        product_actions: HashMap::new(),
        provider_rows: HashMap::new(),
        heading,
        empty_hint,
        error,
        timeline_scroll,
        timeline_items: HashMap::new(),
        composer,
        extras,
        extra_buttons: HashMap::new(),
        more_button: more,
        form_fields: HashMap::new(),
        quota_chart: None,
        pane_bar,
        pane_buttons: HashMap::new(),
        automations_page,
        automation_list,
        automation_actions,
        automation_canvas,
        send,
        interrupt,
        workspace_page,
        workspace_heading,
        workspace_status,
        workspace_editor,
        workspace_input,
        workspace_actions,
        workspace_buttons: HashMap::new(),
        workspace_tree,
        inspector,
        inspector_heading,
        inspector_body,
        focus_targets: HashMap::new(),
    };
    handles.focus_targets.insert(
        target_ids::COMMAND_PALETTE_OPEN.to_owned(),
        search.stable_id(),
    );
    handles
        .focus_targets
        .insert("lilia.composer.input".to_owned(), composer.stable_id());
    handles.sync_lists(context, snapshot)?;
    handles.sync_settings_content(context, document_id, snapshot)?;
    handles.sync_workspace_page(context, document_id, snapshot)?;
    handles.sync_overlay(context, document_id, snapshot)?;
    Ok((document, handles))
}

fn primary_content_id(
    snapshot: &PrimaryShellSnapshot,
    conversation: Entity<HostStack>,
    settings_page: Entity<SettingsPage>,
    workspace_page: Entity<HostStack>,
    automations_page: Entity<HostStack>,
) -> StableNodeId {
    if snapshot.settings_open {
        settings_page.stable_id()
    } else if snapshot.automations_open {
        automations_page.stable_id()
    } else if snapshot.document.is_some()
        || snapshot.files.is_some()
        || snapshot.terminal.is_some()
        || !snapshot.panes.is_empty()
    {
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
        context.update_component(self.search_button, |button, _| {
            *button = search_button();
        })?;
        context.update_component(self.more_button, |button, _| {
            *button = more_button();
        })?;
        context.update_component(self.title_center, |title, _| {
            *title = Text::new(snapshot.title.clone());
        })?;
        context.update_component(self.heading, |heading, _| {
            *heading = Text::new(snapshot.heading.clone());
        })?;
        context.update_component(self.empty_hint, |hint, _| {
            *hint = Text::new(snapshot.empty_hint.clone());
        })?;
        context.update_component(self.error, |error, _| {
            *error = Text::new(snapshot.error.clone().unwrap_or_default());
        })?;
        context.update_component(self.composer, |composer, _| {
            if composer.state.value != snapshot.composer {
                composer.state.replace_value(snapshot.composer.clone());
            }
            composer.placeholder = Arc::from(snapshot.composer_placeholder.as_str());
            composer.disabled = snapshot.composer_disabled;
        })?;
        context.update_component(self.send, |button, _| {
            *button = send_button(snapshot.can_send);
        })?;
        context.update_component(self.interrupt, |button, _| {
            *button = interrupt_button(snapshot.can_interrupt);
        })?;
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
        self.sync_workspace_page(context, document_id, snapshot)?;
        self.sync_panes(context, document_id, snapshot)?;
        self.sync_automations(context, document_id, snapshot)?;
        self.sync_overlay(context, document_id, snapshot)?;
        let navigation = if snapshot.settings_open {
            self.settings_sidebar.stable_id()
        } else {
            self.conversation_sidebar.stable_id()
        };
        let primary = primary_content_id(
            snapshot,
            self.conversation,
            self.settings_page,
            self.workspace_page,
            self.automations_page,
        );
        context.update_component(self.shell, |shell, _| {
            shell.model = snapshot.workspace.clone();
            shell.title = Some(Arc::from(snapshot.title.as_str()));
            shell.title_leading = Some(self.title_leading.stable_id());
            shell.title_center = Some(self.title_center.stable_id());
            shell.title_trailing = Some(self.title_trailing.stable_id());
            shell.navigation = Some(navigation);
            shell.primary = Some(primary);
            shell.inspector = Some(self.inspector.stable_id());
        })?;
        context.assemble_desktop_shell(self.shell)?;
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
                        let _ = context.scroll_by(
                            self.timeline_scroll,
                            ScrollOffset { x, y },
                        )?;
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
        self.reconcile_task_rows(context, document_id, snapshot)?;
        self.reconcile_timeline(context, document_id, snapshot)?;
        self.reconcile_composer_extras(context, document_id, snapshot)
    }

    fn reconcile_task_rows(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for task in &snapshot.tasks {
            let key = task.id.as_str().to_owned();
            keep.insert(key.clone());
            let row = if let Some(row) = self.task_rows.get(&key).copied() {
                context.update_component(row, |row, _| {
                    row.label = Arc::from(task.title.as_str());
                    row.state = if task.selected {
                        SidebarRowState::Active
                    } else {
                        SidebarRowState::Idle
                    };
                })?;
                row
            } else {
                let leading = context
                    .create_detached_component(document_id, SidebarRowIcon::new(Icon::Workspace))?;
                let row = context.create_detached_component(
                    document_id,
                    SidebarRow::new(task.title.clone())
                        .state(if task.selected {
                            SidebarRowState::Active
                        } else {
                            SidebarRowState::Idle
                        })
                        .slots(nana_ui::runtime::ListItemSlots {
                            leading: Some(leading.stable_id()),
                            content: None,
                            trailing: None,
                        }),
                )?;
                context.append_child(row, leading)?;
                let sink = Arc::clone(&self.sink);
                let task_id = task.id.clone();
                context.on(row, move |_, _event: &Activate, _| {
                    emit(&sink, ShellIntent::SelectTask(task_id.clone()));
                })?;
                self.task_rows.insert(key, row);
                row
            };
            order.push(row.stable_id());
        }
        let stale: Vec<_> = self
            .task_rows
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(row) = self.task_rows.remove(&key) {
                let _ = context.remove_view(row);
            }
        }
        reconcile_children(context, self.task_body.stable_id(), &order)
    }

    fn reconcile_timeline(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for item in &snapshot.timeline {
            keep.insert(item.id.clone());
            let entity = if let Some(entity) = self.timeline_items.get(&item.id).copied() {
                context.update_component(entity, |markdown, _| {
                    *markdown = NativeMarkdown::parse(&item.markdown);
                })?;
                context.assemble_markdown(entity)?;
                entity
            } else {
                let entity = context.create_detached_component(
                    document_id,
                    NativeMarkdown::parse(&item.markdown),
                )?;
                context.assemble_markdown(entity)?;
                self.timeline_items.insert(item.id.clone(), entity);
                entity
            };
            order.push(entity.stable_id());
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
        }
        reconcile_children(context, self.timeline_scroll.stable_id(), &order)
    }

    fn reconcile_composer_extras(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let mut desired = Vec::new();
        desired.push((
            "attach-file",
            "添加文件",
            ButtonKind::Subtle,
            ShellIntent::PickAttachmentFiles,
        ));
        desired.push((
            "attach-dir",
            "添加目录",
            ButtonKind::Subtle,
            ShellIntent::PickAttachmentDirectories,
        ));
        desired.push((
            "plan",
            if snapshot.plan_mode {
                "计划：开"
            } else {
                "计划"
            },
            if snapshot.plan_mode {
                ButtonKind::Primary
            } else {
                ButtonKind::Subtle
            },
            ShellIntent::TogglePlanMode,
        ));
        desired.push((
            "goal",
            if snapshot.goal_mode {
                "目标：开"
            } else {
                "目标"
            },
            if snapshot.goal_mode {
                ButtonKind::Primary
            } else {
                ButtonKind::Subtle
            },
            ShellIntent::ToggleGoalMode,
        ));
        desired.push((
            "permission",
            snapshot.permission_label.as_str(),
            ButtonKind::Subtle,
            ShellIntent::CyclePermission,
        ));
        if snapshot.suggestions_can_refresh {
            desired.push((
                "refresh-suggestions",
                "刷新建议",
                ButtonKind::Subtle,
                ShellIntent::RefreshSuggestions,
            ));
        }
        for suggestion in &snapshot.suggestions {
            desired.push((
                suggestion.id.as_str(),
                suggestion.label.as_str(),
                ButtonKind::Subtle,
                ShellIntent::ApplySuggestion(suggestion.prompt.clone()),
            ));
        }
        for attachment in &snapshot.attachments {
            desired.push((
                attachment.id.as_str(),
                attachment.label.as_str(),
                ButtonKind::Ghost,
                ShellIntent::RemoveAttachment(attachment.id.clone()),
            ));
        }
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for (id, label, kind, intent) in desired {
            keep.insert(id.to_owned());
            let button = if let Some(button) = self.extra_buttons.get(id).copied() {
                context.update_component(button, |button, _| {
                    *button = extra_button(label, kind);
                })?;
                button
            } else {
                let button =
                    context.create_detached_component(document_id, extra_button(label, kind))?;
                bind_activate(context, button, Arc::clone(&self.sink), intent)?;
                self.extra_buttons.insert(id.to_owned(), button);
                button
            };
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .extra_buttons
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.extra_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.extras.stable_id(), &order)
    }

    fn sync_settings_content(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        context.update_component(self.settings_sidebar, |sidebar, _| {
            sidebar.model = snapshot.settings.model.clone();
            sidebar.state = snapshot.settings.state.clone();
        })?;
        context.update_component(self.appearance, |section, _| {
            section.theme = snapshot.theme;
            section.appearance = snapshot.settings.appearance.clone();
            section.platform_hint = Some(Arc::from(platform_material_support().hint()));
            section.material_status = Some(Arc::from(snapshot.settings.material_status.as_str()));
        })?;
        let tab = snapshot.settings.state.active_tab().as_str();
        let (heading, body, error, show_project, actions) = settings_tab_copy(&snapshot.settings);
        context.update_component(self.product_heading, |text, _| {
            *text = Text::new(heading);
        })?;
        context.update_component(self.product_body, |text, _| {
            *text = Text::new(body);
        })?;
        context.update_component(self.product_error, |text, _| {
            *text = Text::new(error.unwrap_or_default());
        })?;
        context.update_component(self.project_name, |editor, _| {
            if editor.state.value != snapshot.settings.project_name {
                editor.state.replace_value(snapshot.settings.project_name.clone());
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
        let mut order = vec![
            self.product_heading.stable_id(),
            self.product_body.stable_id(),
            self.product_error.stable_id(),
        ];
        if show_project {
            order.push(self.project_name.stable_id());
            order.push(self.project_workspace.stable_id());
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
            order.push(button.stable_id());
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
                order.push(button.stable_id());
            }
        }
        self.append_settings_forms(context, document_id, snapshot, &mut keep, &mut order)?;
        let stale: Vec<_> = self
            .product_actions
            .keys()
            .chain(self.provider_rows.keys())
            .chain(self.form_fields.keys())
            .filter(|key| !keep.contains(*key))
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
        }
        reconcile_children(context, self.product_settings.stable_id(), &order)
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
        let field = if let Some(field) = self.form_fields.get(id).copied() {
            context.update_component(field, |editor, _| {
                if editor.state.value != value {
                    editor.state.replace_value(value.to_owned());
                }
            })?;
            field
        } else {
            let field = context
                .create_detached_component(document_id, TextArea::new(value.to_owned()).height(40.0))?;
            let sink = Arc::clone(&self.sink);
            context.on(field, move |_, event: &TextChanged, _| {
                emit(&sink, intent(event.value.clone()));
            })?;
            self.form_fields.insert(id.to_owned(), field);
            field
        };
        order.push(field.stable_id());
        Ok(())
    }

    fn sync_workspace_page(
        &mut self,
        context: &mut AppContext,
        document_id: DocumentId,
        snapshot: &PrimaryShellSnapshot,
    ) -> Result<(), FrameworkError> {
        let (title, status, editor, disabled) = if let Some(document) = &snapshot.document {
            (
                document.title.clone(),
                document.status.clone(),
                document.text.clone(),
                document.read_only,
            )
        } else if let Some(terminal) = &snapshot.terminal {
            (
                "终端".to_owned(),
                terminal.notice.clone().unwrap_or_default(),
                terminal.output.clone(),
                true,
            )
        } else if let Some(files) = &snapshot.files {
            (
                "项目文件".to_owned(),
                files.preview.clone().unwrap_or_default(),
                String::new(),
                true,
            )
        } else {
            (String::new(), String::new(), String::new(), true)
        };
        context.update_component(self.workspace_heading, |text, _| {
            *text = Text::new(title);
        })?;
        context.update_component(self.workspace_status, |text, _| {
            *text = Text::new(status);
        })?;
        context.update_component(self.workspace_editor, |editor_view, _| {
            if editor_view.state.value != editor {
                editor_view.state.replace_value(editor);
            }
            editor_view.disabled = disabled;
        })?;
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
        self.reconcile_workspace_actions(context, document_id, snapshot)
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
        let mut keep = HashSet::new();
        let mut order = Vec::new();
        for pane in &snapshot.panes {
            if snapshot.panes.len() > 1 {
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
            for item in &pane.items {
                let id = format!("item-{}", item.id);
                keep.insert(id.clone());
                let button = self.upsert_chrome_button(
                    context,
                    document_id,
                    &id,
                    &item.title,
                    if item.selected {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Subtle
                    },
                    ShellIntent::ActivateWorkspaceItem(item.id.clone()),
                )?;
                order.push(button.stable_id());
            }
        }
        if snapshot.titlebar_can_split {
            for (id, label, intent) in [
                (
                    "split-h",
                    "横向拆分",
                    ShellIntent::SplitWorkspaceHorizontal,
                ),
                (
                    "split-v",
                    "纵向拆分",
                    ShellIntent::SplitWorkspaceVertical,
                ),
            ] {
                keep.insert(id.to_owned());
                let button = self.upsert_chrome_button(
                    context,
                    document_id,
                    id,
                    label,
                    ButtonKind::Subtle,
                    intent,
                )?;
                order.push(button.stable_id());
            }
        }
        if snapshot.titlebar_can_close {
            keep.insert("close-item".into());
            let button = self.upsert_chrome_button(
                context,
                document_id,
                "close-item",
                "关闭当前",
                ButtonKind::Danger,
                ShellIntent::CloseCurrentWorkspaceItem,
            )?;
            order.push(button.stable_id());
        }
        let stale: Vec<_> = self
            .pane_buttons
            .keys()
            .filter(|key| !keep.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(button) = self.pane_buttons.remove(&key) {
                let _ = context.remove_view(button);
            }
        }
        reconcile_children(context, self.pane_bar.stable_id(), &order)
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
        reconcile_children(context, self.automation_list.stable_id(), &order)?;
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
            let button = self.upsert_chrome_button(
                context,
                document_id,
                id,
                label,
                kind,
                intent,
            )?;
            actions.push(button.stable_id());
        }
        reconcile_children(context, self.automation_actions.stable_id(), &actions)
    }

    fn titlebar_more_items(snapshot: &PrimaryShellSnapshot) -> Vec<(&'static str, String, ShellIntent)> {
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
                    "返回会话列表".to_owned(),
                    ShellIntent::BackToTaskList,
                ),
                (
                    "more-popup",
                    "在新窗口打开".to_owned(),
                    ShellIntent::OpenTaskPopup,
                ),
                (
                    "more-ask",
                    "追问子对话".to_owned(),
                    ShellIntent::AskTaskPopup,
                ),
                (
                    "more-inspector",
                    "会话详情".to_owned(),
                    ShellIntent::ToggleTaskInspector,
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
        } else if let Some(palette) = self.palette.take() {
            let _ = context.remove_view(palette);
            context.update_component(self.shell, |shell, _| {
                shell.overlays.clear();
            })?;
            self.focus_targets
                .remove(target_ids::COMMAND_PALETTE_INPUT);
        } else if snapshot.titlebar_menu_open {
            let items = Self::titlebar_more_items(snapshot);
            let menu = if let Some(menu) = self.more_menu {
                menu
            } else {
                let menu = context
                    .create_detached_component(document_id, HostStack::fill_column(6.0).padding(12.0))?;
                context.append_child(host, menu)?;
                self.more_menu = Some(menu);
                menu
            };
            let mut order = Vec::new();
            for (id, label, intent) in items {
                let button = self.upsert_chrome_button(
                    context,
                    document_id,
                    id,
                    &label,
                    ButtonKind::Subtle,
                    intent,
                )?;
                order.push(button.stable_id());
            }
            reconcile_children(context, menu.stable_id(), &order)?;
            context.update_component(self.shell, |shell, _| {
                shell.overlays = vec![menu.stable_id()];
            })?;
            context.activate_overlay(host, menu)?;
        } else if let Some(menu) = self.more_menu.take() {
            let _ = context.remove_view(menu);
            context.update_component(self.shell, |shell, _| {
                shell.overlays.clear();
            })?;
        }
        Ok(())
    }
}

struct SettingsAction {
    id: String,
    label: String,
    primary: bool,
    intent: ShellIntent,
}

fn settings_tab_copy(settings: &SettingsSnapshot) -> (String, String, Option<String>, bool, Vec<SettingsAction>) {
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
        "agent" => (
            "Agent".to_owned(),
            "切换已接入的 Agent 行为。".to_owned(),
            None,
            false,
            {
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
            },
        ),
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
            vec![
                SettingsAction {
                    id: "toggle-remote-host".into(),
                    label: if settings.remote_host_enabled {
                        "关闭远程主机"
                    } else {
                        "开启远程主机"
                    }
                    .into(),
                    primary: true,
                    intent: ShellIntent::ToggleRemoteHost,
                },
                SettingsAction {
                    id: "toggle-keep-awake".into(),
                    label: if settings.remote_keep_awake {
                        "关闭保持唤醒"
                    } else {
                        "保持唤醒"
                    }
                    .into(),
                    primary: false,
                    intent: ShellIntent::ToggleRemoteKeepAwake,
                },
            ],
        ),
        "desktop" => (
            "桌面".to_owned(),
            settings.desktop_status.clone(),
            None,
            false,
            vec![SettingsAction {
                id: "check-update".into(),
                label: "检查更新".into(),
                primary: false,
                intent: ShellIntent::CheckForUpdate,
            }],
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
        _ => (
            String::new(),
            String::new(),
            None,
            false,
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_snapshot() -> PrimaryShellSnapshot {
        PrimaryShellSnapshot {
            theme: ThemeMode::Light,
            title: "LiliaCode".to_owned(),
            heading: "选择一个会话".to_owned(),
            empty_hint: "从左侧打开会话，或新建会话。".to_owned(),
            error: None,
            settings_open: false,
            sidebar_collapsed: false,
            workspace: WorkspaceModel::new(),
            tasks: Vec::new(),
            timeline: Vec::new(),
            composer: String::new(),
            composer_placeholder: "输入消息".to_owned(),
            composer_disabled: true,
            can_send: false,
            can_interrupt: false,
            attachments: Vec::new(),
            plan_mode: false,
            goal_mode: false,
            permission_label: "权限：询问".to_owned(),
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
                }
            },
            document: None,
            files: None,
            terminal: None,
            inspector_title: String::new(),
            inspector_body: String::new(),
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

    #[test]
    fn mounts_a_primary_shell_document() {
        let (document, _handles) =
            mount_primary_shell(&empty_snapshot(), Arc::new(|_| {})).expect("mount shell");
        assert_eq!(document.document(), DocumentId::new(1).unwrap());
    }
}
