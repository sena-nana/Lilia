use lilia_contracts::{ProjectId, SidebarNavigationTarget, TaskId};
use crate::application::DesktopMcpCredentialKind;
use crate::runtime_compat::HostedWindowId;

use crate::desktop::{
    HostedContextMenuEvent, SidebarMenuAction, SidebarMenuTarget, SidebarTreeDropPosition,
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
    SelectRepository { full_name: String },
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
pub enum WorktreeMessage {
    ConfirmAction,
    CancelAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportMessage {
    PickSource,
    Execute,
    Reset,
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
    TitleModelChanged(String),
    SuggestionModelChanged(String),
    PromptRouterModelChanged(String),
    PromptOptimizeModelChanged(String),
    AutoTurnDecisionModelChanged(String),
    FeaturePresetModelChanged {
        preset_id: String,
        value: String,
    },
    CustomPresetDraftChanged(String),
    AddCustomPreset,
    RenameCustomPreset {
        preset_id: String,
        value: String,
    },
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
    ToggleHost,
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
    OpenReleases,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectMessage {
    OpenProjectsOverview,
    ProjectNameChanged(String),
    ProjectWorkspaceChanged(String),
    PickProjectWorkspace,
    ReorderProject {
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    },
    ConfirmProjectRemoval,
    CancelProjectRemoval,
    ConfirmProjectConversationArchive,
    CancelProjectConversationArchive,
    ProjectWorktreeParentChanged(String),
    RestoreProject(ProjectId),
    SelectProject(ProjectId),
    SaveProjectSettings,
    OpenProjectWorkspace,
    OpenNativeProjectTerminal,
    OpenProjectFiles,
    RefreshProjectFiles,
    ToggleProjectFileExpand(String),
    OpenProjectFile(String),
    CloseInspectorDock,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskMessage {
    TaskSearchChanged(String),
    NewTaskTitleChanged(String),
    TaskTitleChanged(String),
    TaskDropSearchChanged(String),
    ReorderTask {
        task_id: TaskId,
        before_task_id: Option<TaskId>,
    },
    DropTask {
        source: TaskDropItem,
        before: Option<TaskDropItem>,
    },
    SelectInbox,
    SelectTask(TaskId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarMessage {
    ToggleSidebarSearch,
    SidebarSearchChanged(String),
    ToggleSidebarProject(ProjectId),
    ToggleAllSidebarProjects,
    ToggleSidebarInbox,
    RevealSidebarInboxTasks,
    RevealSidebarProjectTasks(ProjectId),
    OpenSidebarMenu {
        target: SidebarMenuTarget,
        anchor_y: f32,
    },
    SidebarMenu(HostedContextMenuEvent<SidebarMenuAction>),
    OpenSidebarProjectDraft(ProjectId),
    OpenSidebarInboxDraft,
    OpenSidebarTaskPopup(TaskId),
    SidebarToggleTaskPinned(TaskId),
    SidebarRequestTaskWorktreeMerge(TaskId),
    SidebarArchiveTask(TaskId),
    SidebarStopTask(TaskId),
    SidebarTreeDrop {
        source: SidebarTreeNode,
        target: SidebarTreeNode,
        position: SidebarTreeDropPosition,
    },
    OpenSidebarNavigation(SidebarNavigationTarget),
}
