use lilia_contracts::{ProjectId, SidebarNavigationTarget, TaskId};
use crate::application::{DesktopMcpCredentialKind, WorkspaceItemId};
use crate::runtime_compat::HostedWindowId;

use crate::desktop::{
    HostedContextMenuEvent, Point, SidebarMenuAction, SidebarMenuTarget, SidebarTreeDropPosition,
    SidebarTreeNode, TaskDropItem,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookHandlerDraftField {
    Event,
    Matcher,
    Type,
    TimeoutSeconds,
    Command,
    CommandWindows,
    StatusMessage,
}

impl HookHandlerDraftField {
    pub(crate) const ALL: [Self; 7] = [
        Self::Event,
        Self::Matcher,
        Self::Type,
        Self::TimeoutSeconds,
        Self::Command,
        Self::CommandWindows,
        Self::StatusMessage,
    ];

    pub(crate) const fn target_key(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Matcher => "matcher",
            Self::Type => "type",
            Self::TimeoutSeconds => "timeout",
            Self::Command => "command",
            Self::CommandWindows => "command-windows",
            Self::StatusMessage => "status-message",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectCloneMessage {
    Close,
    RepositoryChanged(String),
    ParentChanged(String),
    PickParent,
    Start,
    Cancel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GitHubMessage {
    StartBinding,
    CancelBinding,
    OpenVerification,
    CopyUserCode,
    Unbind,
    RefreshRepositories,
    LoadMoreRepositories,
    SelectRepository { full_name: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DocumentMessage {
    EditorEdited {
        item_id: WorkspaceItemId,
        action: String,
    },
    GoToDefinition {
        item_id: WorkspaceItemId,
        window_id: HostedWindowId,
    },
    OpenDefinitionTarget {
        item_id: WorkspaceItemId,
        window_id: HostedWindowId,
        index: usize,
    },
    SaveEditor(WorkspaceItemId),
    DiscardEditor(WorkspaceItemId),
    SelectDiagnostic {
        item_id: WorkspaceItemId,
        index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SuggestionsMessage {
    Refresh {
        window_id: HostedWindowId,
        force: bool,
    },
    Apply {
        window_id: HostedWindowId,
        prompt: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PromptOptimizeMessage {
    Optimize(HostedWindowId),
    ApplyRoute(HostedWindowId),
    DismissRoute(HostedWindowId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorktreeMessage {
    Create,
    Pick,
    Open,
    Clear,
    RequestCleanup,
    RequestMerge,
    ConfirmAction,
    CancelAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportMessage {
    PickSource,
    ToggleCredentials,
    Execute,
    Reset,
    RestartAfter,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProviderMessage {
    Select(String),
    SecretChanged(String),
    SaveCredential,
    RevokeCredential {
        credential_id: String,
        revision: u64,
    },
    Refresh,
    ModelChanged(String),
    OpenAiEndpointChanged(String),
    AnthropicEndpointChanged(String),
    SaveRuntimeSettings,
    ResetRuntimeSettings,
    AssistantBaseUrlChanged(String),
    AssistantModelChanged(String),
    AssistantSecretChanged(String),
    AssistantNewModelIdChanged(String),
    AssistantNewModelLabelChanged(String),
    AddAssistantModel,
    RenameAssistantModel {
        model_id: String,
        value: String,
    },
    FetchAssistantModels,
    TestAssistantConnection,
    SaveAssistantConfiguration,
    ClearAssistantSecret,
    TitleModelChanged(String),
    SuggestionModelChanged(String),
    PromptRouterModelChanged(String),
    PromptOptimizeModelChanged(String),
    AutoTurnDecisionModelChanged(String),
    FeaturePresetModelChanged {
        preset_id: String,
        value: String,
    },
    CycleFeaturePresetEffort(String),
    CustomPresetDraftChanged(String),
    AddCustomPreset,
    RenameCustomPreset {
        preset_id: String,
        value: String,
    },
    RemoveCustomPreset(String),
    SaveModelFeatureSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuotaMessage {
    Refresh,
    CycleDays,
    CycleBackend,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionsMessage {
    Refresh,
    SkillIdChanged(String),
    SkillDescriptionChanged(String),
    CreateSkill,
    ToggleSkill(String),
    RequestDeleteSkill(String),
    ConfirmDeleteSkill,
    CancelDeleteSkill,
    PluginSourceChanged(String),
    PickPluginDirectory,
    InstallPlugin,
    TogglePlugin(String),
    RequestDeletePlugin(String),
    ConfirmDeletePlugin,
    CancelDeletePlugin,
    HookDraftChanged {
        source_id: String,
        value: String,
    },
    HookHandlerDraftChanged {
        source_id: String,
        index: usize,
        field: HookHandlerDraftField,
        value: String,
    },
    AddHookHandler(String),
    RemoveHookHandler {
        source_id: String,
        index: usize,
    },
    CreateHookSource(String),
    SaveHookSource(String),
    ToggleHookSource(String),
    RequestDeleteHookSource(String),
    ConfirmDeleteHookSource,
    CancelDeleteHookSource,
    ActivateRegisteredMcp,
    NewMcpServer,
    EditMcpServer(String),
    McpServerIdChanged(String),
    CycleMcpTransport,
    McpLocationChanged(String),
    McpArgsChanged(String),
    McpCredentialNamesChanged(String),
    ToggleMcpEditorEnabled,
    SaveMcpServer,
    CancelMcpEditor,
    ToggleMcpServer(String),
    RequestDeleteMcpServer(String),
    ConfirmDeleteMcpServer,
    CancelDeleteMcpServer,
    McpCredentialChanged {
        server_id: String,
        kind: DesktopMcpCredentialKind,
        name: String,
        value: String,
    },
    SaveMcpCredential {
        server_id: String,
        kind: DesktopMcpCredentialKind,
        name: String,
    },
    DeleteMcpCredential {
        server_id: String,
        kind: DesktopMcpCredentialKind,
        name: String,
    },
    ReadMcpResource {
        server_id: String,
        uri: String,
    },
    McpPromptArgumentsChanged {
        namespaced_name: String,
        value: String,
    },
    GetMcpPrompt(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RemoteMessage {
    Refresh,
    ToggleHost,
    PcNameChanged(String),
    SavePcName,
    ToggleKeepAwake,
    StartPairing,
    CancelPairing,
    CopyPairingUri,
    RevokeDevice(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateMessage {
    Check,
    Install,
    DismissPrompt,
    PromptDialogInteraction,
    OpenReleases,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectMessage {
    RefreshProjects,
    OpenProjectsOverview,
    RefreshProjectsOverview,
    CreateProject,
    ProjectNameChanged(String),
    ProjectWorkspaceChanged(String),
    PickProjectWorkspace,
    ClearProjectWorkspace,
    SaveProject,
    ToggleProjectPinned,
    MoveProjectUp,
    MoveProjectDown,
    ReorderProject {
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    },
    RequestProjectRemoval,
    ConfirmProjectRemoval,
    CancelProjectRemoval,
    ProjectRemovalDialogInteraction,
    RequestProjectConversationArchive,
    ConfirmProjectConversationArchive,
    CancelProjectConversationArchive,
    ProjectConversationArchiveDialogInteraction,
    RestoreProject(ProjectId),
    SelectProject(ProjectId),
    CycleProjectWorktreeMode,
    ProjectWorktreeParentChanged(String),
    PickProjectWorktreeParent,
    ProjectWorktreeInstructionsEdited(String),
    ToggleProjectWorktreeCleanup,
    SaveProjectSettings,
    OpenProjectSettings,
    OpenProjectWorkspace,
    OpenProjectCodeEditor,
    OpenProjectTerminal,
    OpenNativeProjectTerminal,
    OpenProjectFiles,
    RefreshProjectFiles,
    ToggleProjectFileExpand(String),
    OpenProjectFile(String),
    OpenProjectTasks,
    RunProjectTask(String),
    CloseInspectorDock,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskMessage {
    TaskSearchChanged(String),
    NewTaskTitleChanged(String),
    CreateTask,
    CloseMainConversationDraft,
    TaskTitleChanged(String),
    SaveTask,
    CycleTaskDependency,
    ToggleTaskDependency,
    CycleTaskStatus,
    CycleTaskPriority,
    ToggleTaskPinned,
    MoveTaskUp,
    MoveTaskDown,
    ReorderTask {
        task_id: TaskId,
        before_task_id: Option<TaskId>,
    },
    DropTask {
        source: TaskDropItem,
        before: Option<TaskDropItem>,
    },
    TaskDropInteraction,
    TaskDropSearchChanged(String),
    CycleTaskMoveTarget,
    MoveTaskToProject,
    CycleTaskParentTarget,
    ReparentTask,
    ClearTaskParent,
    ArchiveTask,
    RestoreTask(TaskId),
    SelectInbox,
    SelectTask(TaskId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarMessage {
    ToggleSidebarSearch,
    SidebarSearchChanged(String),
    SidebarSearchSelectionChanged(usize),
    ToggleSidebarProject(ProjectId),
    ToggleAllSidebarProjects,
    ToggleSidebarInbox,
    RevealSidebarInboxTasks,
    RevealSidebarProjectTasks(ProjectId),
    OpenSidebarMenuAt {
        target: SidebarMenuTarget,
        anchor: Point,
    },
    OpenSidebarMenu {
        target: SidebarMenuTarget,
        anchor_y: f32,
    },
    SidebarMenu(HostedContextMenuEvent<SidebarMenuAction>),
    OpenSidebarProjectPopup(ProjectId),
    OpenSidebarProjectDraft(ProjectId),
    OpenSidebarInboxDraft,
    OpenSidebarTaskPopup(TaskId),
    SidebarToggleTaskPinned(TaskId),
    SidebarRequestTaskWorktreeMerge(TaskId),
    SidebarArchiveTask(TaskId),
    CancelSidebarTaskArchive(TaskId),
    SidebarStopTask(TaskId),
    SidebarTreeDrop {
        source: SidebarTreeNode,
        target: SidebarTreeNode,
        position: SidebarTreeDropPosition,
    },
    SidebarTreeInteraction,
    DismissSidebarError,
    OpenSidebarNavigation(SidebarNavigationTarget),
}
