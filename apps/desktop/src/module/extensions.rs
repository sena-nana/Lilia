//! The extensions settings tab as a UI module.
//!
//! Owns the skills / plugins / hooks / MCP drafts the operator is editing.
//! Job submission stays in the shell: credentials must not enter a payload, so
//! the command is parked beside the ticket rather than carried by the module.

use std::collections::BTreeMap;

use lilia_kernel::FeatureId;
use serde_json::Value;

use crate::application::{
    DesktopExtensionsSnapshot, DesktopHookDocumentUpdate, DesktopHookDocumentView,
    DesktopHookHandlerUpdate, DesktopHookScope, DesktopHookSourceView, DesktopHooksOverview,
    DesktopMcpActivationReport, DesktopMcpCredentialKind, DesktopMcpServerUpsert,
    DesktopMcpTransport, DesktopPluginInstall, DesktopSecret, DesktopSkillCreate,
    DesktopSkillScope,
};
use crate::runtime_shell::{PrimaryShellSnapshot, ShellMcpEditor, ShellMcpRow, ShellSkillRow};
use crate::shell::{ExtensionsMessage, HookHandlerDraftField};
use crate::ui_module::{ShellEffect, UiModule, UiModuleContext, UiModuleOutcome};

#[derive(Clone, Debug)]
pub(crate) struct NativeHooksSnapshot {
    pub overview: DesktopHooksOverview,
    pub documents: BTreeMap<String, DesktopHookDocumentView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpContentKind {
    Resource,
    Prompt,
}

#[derive(Debug, Clone)]
pub(crate) struct McpContentPreview {
    pub kind: McpContentKind,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub(crate) struct McpEditorState {
    pub editing_server_id: Option<String>,
    pub server_id: String,
    pub transport: DesktopMcpTransport,
    pub location: String,
    pub args_json: String,
    pub credential_names_json: String,
    pub enabled: bool,
}

impl Default for McpEditorState {
    fn default() -> Self {
        Self {
            editing_server_id: None,
            server_id: String::new(),
            transport: DesktopMcpTransport::Stdio,
            location: String::new(),
            args_json: "[]".to_owned(),
            credential_names_json: "[]".to_owned(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum McpRegistryOperation {
    Upsert(DesktopMcpServerUpsert),
    SetEnabled {
        server_id: String,
        enabled: bool,
        expected_registry_revision: u64,
    },
    Delete {
        server_id: String,
        expected_registry_revision: u64,
    },
    SetCredential {
        server_id: String,
        kind: DesktopMcpCredentialKind,
        name: String,
        secret: DesktopSecret,
    },
    DeleteCredential {
        server_id: String,
        kind: DesktopMcpCredentialKind,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum SkillRegistryOperation {
    Create(DesktopSkillCreate),
    SetEnabled {
        skill_id: String,
        enabled: bool,
        expected_registry_revision: u64,
    },
    Delete {
        skill_id: String,
        expected_registry_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum PluginRegistryOperation {
    Install(DesktopPluginInstall),
    SetEnabled {
        plugin_id: String,
        enabled: bool,
        expected_registry_revision: u64,
    },
    Delete {
        plugin_id: String,
        expected_registry_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum HookSourceOperation {
    Create {
        scope: DesktopHookScope,
        project_cwd: Option<String>,
    },
    Update {
        scope: DesktopHookScope,
        project_cwd: Option<String>,
        input: DesktopHookDocumentUpdate,
    },
    SetEnabled {
        scope: DesktopHookScope,
        project_cwd: Option<String>,
        expected_revision: u64,
        enabled: bool,
    },
    Delete {
        scope: DesktopHookScope,
        project_cwd: Option<String>,
        expected_revision: u64,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum McpContentOperation {
    ReadResource {
        server_id: String,
        uri: String,
    },
    GetPrompt {
        namespaced_name: String,
        arguments: Value,
    },
}

#[derive(Debug)]
pub(crate) enum ExtensionsCommand {
    Skill(SkillRegistryOperation),
    Plugin(PluginRegistryOperation),
    Hook {
        operation: HookSourceOperation,
        overview_project_cwd: Option<String>,
    },
    Refresh {
        project_cwd: Option<String>,
    },
    ActivateMcp,
    McpContent(McpContentOperation),
    McpRegistry(McpRegistryOperation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtensionsLane {
    Skill,
    Plugin,
    Hook,
    Refresh,
    Activation,
    Content,
}

impl ExtensionsLane {
    pub(crate) fn clears_credential_drafts(self) -> bool {
        matches!(self, Self::Refresh | Self::Activation)
    }

    pub(crate) fn releases_pending_activation(self) -> bool {
        matches!(
            self,
            Self::Skill | Self::Refresh | Self::Activation | Self::Content
        )
    }
}

impl ExtensionsCommand {
    pub(crate) fn lane(&self) -> ExtensionsLane {
        match self {
            Self::Skill(_) => ExtensionsLane::Skill,
            Self::Plugin(_) => ExtensionsLane::Plugin,
            Self::Hook { .. } => ExtensionsLane::Hook,
            Self::Refresh { .. } => ExtensionsLane::Refresh,
            Self::ActivateMcp | Self::McpRegistry(_) => ExtensionsLane::Activation,
            Self::McpContent(_) => ExtensionsLane::Content,
        }
    }

    pub(crate) fn operation(&self) -> &'static str {
        match self {
            Self::Skill(SkillRegistryOperation::Create(_)) => "skills.create",
            Self::Skill(SkillRegistryOperation::SetEnabled { .. }) => "skills.set-enabled",
            Self::Skill(SkillRegistryOperation::Delete { .. }) => "skills.delete",
            Self::Plugin(PluginRegistryOperation::Install(_)) => "plugins.install",
            Self::Plugin(PluginRegistryOperation::SetEnabled { .. }) => "plugins.set-enabled",
            Self::Plugin(PluginRegistryOperation::Delete { .. }) => "plugins.delete",
            Self::Hook {
                operation: HookSourceOperation::Create { .. },
                ..
            } => "hooks.create",
            Self::Hook {
                operation: HookSourceOperation::Update { .. },
                ..
            } => "hooks.update",
            Self::Hook {
                operation: HookSourceOperation::SetEnabled { .. },
                ..
            } => "hooks.set-enabled",
            Self::Hook {
                operation: HookSourceOperation::Delete { .. },
                ..
            } => "hooks.delete",
            Self::Refresh { .. } => "extensions.refresh",
            Self::ActivateMcp => "mcp.activate",
            Self::McpContent(McpContentOperation::ReadResource { .. }) => "mcp.read-resource",
            Self::McpContent(McpContentOperation::GetPrompt { .. }) => "mcp.get-prompt",
            Self::McpRegistry(McpRegistryOperation::Upsert(_)) => "mcp.upsert",
            Self::McpRegistry(McpRegistryOperation::SetEnabled { .. }) => "mcp.set-enabled",
            Self::McpRegistry(McpRegistryOperation::Delete { .. }) => "mcp.delete",
            Self::McpRegistry(McpRegistryOperation::SetCredential { .. }) => "mcp.credential.set",
            Self::McpRegistry(McpRegistryOperation::DeleteCredential { .. }) => {
                "mcp.credential.delete"
            }
        }
    }

    pub(crate) fn failure_message(&self) -> &'static str {
        match self {
            Self::Skill(_) => "无法更新 Skills 注册表，请稍后重试。",
            Self::Plugin(_) => "无法更新 Plugins 注册表，请稍后重试。",
            Self::Hook { .. } => "无法更新 Hooks，请稍后重试。",
            Self::Refresh { .. } => "无法读取扩展状态，请稍后重试。",
            Self::ActivateMcp => "无法启动 MCP 连接，请稍后重试。",
            Self::McpContent(_) => "无法读取 MCP 内容，请稍后重试。",
            Self::McpRegistry(_) => "无法更新 MCP 注册表，请稍后重试。",
        }
    }
}

#[derive(Debug)]
pub(crate) enum ExtensionsOutcome {
    Skill(DesktopExtensionsSnapshot),
    Plugin(DesktopExtensionsSnapshot),
    Hook(NativeHooksSnapshot),
    Refresh(DesktopExtensionsSnapshot, NativeHooksSnapshot),
    Activated(DesktopMcpActivationReport),
    Content(McpContentPreview),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ExtensionsJob {
    pub id: lilia_kernel::JobId,
    pub ticket: u64,
    pub lane: ExtensionsLane,
}

pub enum ExtensionsModuleMessage {
    Ui(ExtensionsMessage),
    ApplyOutcome(ExtensionsOutcome),
    SetBusy(bool),
    JobFailed(String),
    PluginDirectoryPicked(String),
}

pub struct ExtensionsModule {
    extensions: Option<DesktopExtensionsSnapshot>,
    hooks: Option<DesktopHooksOverview>,
    hook_documents: BTreeMap<String, DesktopHookDocumentView>,
    hook_drafts: BTreeMap<String, String>,
    hook_delete_confirmation: Option<String>,
    skill_id_input: String,
    skill_description_input: String,
    skill_delete_confirmation: Option<String>,
    plugin_source_input: String,
    plugin_delete_confirmation: Option<String>,
    extensions_activation: Option<DesktopMcpActivationReport>,
    mcp_editor: Option<McpEditorState>,
    mcp_delete_confirmation: Option<String>,
    mcp_credential_drafts: BTreeMap<String, String>,
    mcp_prompt_argument_drafts: BTreeMap<String, String>,
    mcp_content_preview: Option<McpContentPreview>,
    activation_pending: bool,
    error: Option<String>,
    busy: bool,
    pending_submit: Option<ExtensionsCommand>,
}

impl Default for ExtensionsModule {
    fn default() -> Self {
        Self {
            extensions: None,
            hooks: None,
            hook_documents: BTreeMap::new(),
            hook_drafts: BTreeMap::new(),
            hook_delete_confirmation: None,
            skill_id_input: String::new(),
            skill_description_input: String::new(),
            skill_delete_confirmation: None,
            plugin_source_input: String::new(),
            plugin_delete_confirmation: None,
            extensions_activation: None,
            mcp_editor: None,
            mcp_delete_confirmation: None,
            mcp_credential_drafts: BTreeMap::new(),
            mcp_prompt_argument_drafts: BTreeMap::new(),
            mcp_content_preview: None,
            activation_pending: false,
            error: None,
            busy: false,
            pending_submit: None,
        }
    }
}

impl ExtensionsModule {
    pub fn feature_id() -> FeatureId {
        FeatureId::new("lilia.extensions").expect("the extensions feature id is not blank")
    }

    pub fn snapshot(&self) -> Option<&DesktopExtensionsSnapshot> {
        self.extensions.as_ref()
    }

    pub fn skill_id_input(&self) -> &str {
        &self.skill_id_input
    }

    pub fn plugin_source_input(&self) -> &str {
        &self.plugin_source_input
    }

    pub fn skill_delete_confirmation(&self) -> Option<&str> {
        self.skill_delete_confirmation.as_deref()
    }

    pub fn plugin_delete_confirmation(&self) -> Option<&str> {
        self.plugin_delete_confirmation.as_deref()
    }

    pub fn hook_delete_confirmation(&self) -> Option<&str> {
        self.hook_delete_confirmation.as_deref()
    }

    pub fn mcp_delete_confirmation(&self) -> Option<&str> {
        self.mcp_delete_confirmation.as_deref()
    }

    pub fn mcp_editor(&self) -> Option<&McpEditorState> {
        self.mcp_editor.as_ref()
    }

    pub fn mcp_credential_drafts(&self) -> &BTreeMap<String, String> {
        &self.mcp_credential_drafts
    }

    pub fn hook_drafts(&self) -> &BTreeMap<String, String> {
        &self.hook_drafts
    }

    pub fn hooks(&self) -> Option<&DesktopHooksOverview> {
        self.hooks.as_ref()
    }

    pub fn activation(&self) -> Option<&DesktopMcpActivationReport> {
        self.extensions_activation.as_ref()
    }

    pub fn content_preview(&self) -> Option<&McpContentPreview> {
        self.mcp_content_preview.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn take_pending_submit(&mut self) -> Option<ExtensionsCommand> {
        self.pending_submit.take()
    }

    pub fn take_activation_pending(&mut self) -> bool {
        std::mem::take(&mut self.activation_pending)
    }

    pub fn clear_credential_drafts(&mut self) {
        self.mcp_credential_drafts.clear();
    }

    fn queue(&mut self, command: ExtensionsCommand) -> UiModuleOutcome {
        if self.busy {
            if matches!(command, ExtensionsCommand::ActivateMcp) {
                self.activation_pending = true;
            }
            return UiModuleOutcome::clean();
        }
        self.error = None;
        self.pending_submit = Some(command);
        UiModuleOutcome::dirty()
    }

    fn project_cwd(cx: &UiModuleContext<'_>) -> Option<String> {
        let project_id = cx.selected_project()?;
        let session = cx.workspace()?;
        let snapshot = session.snapshot().ok()?;
        snapshot
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| {
                let root = project.workspace_path.as_deref()?.trim();
                (!root.is_empty()).then(|| root.to_owned())
            })
    }

    fn hook_source(&self, source_id: &str) -> Option<DesktopHookSourceView> {
        self.hooks
            .as_ref()?
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .cloned()
    }

    fn apply_hooks_snapshot(&mut self, snapshot: NativeHooksSnapshot) {
        self.hook_drafts = snapshot
            .documents
            .iter()
            .map(|(source_id, document)| (source_id.clone(), hook_document_draft(document)))
            .collect();
        self.hook_documents = snapshot.documents;
        self.hooks = Some(snapshot.overview);
    }

    fn apply_outcome(&mut self, outcome: ExtensionsOutcome) -> UiModuleOutcome {
        self.error = None;
        match outcome {
            ExtensionsOutcome::Skill(snapshot) => {
                self.extensions = Some(snapshot);
                self.skill_id_input.clear();
                self.skill_description_input.clear();
                self.skill_delete_confirmation = None;
            }
            ExtensionsOutcome::Plugin(snapshot) => {
                self.extensions = Some(snapshot);
                self.plugin_source_input.clear();
                self.plugin_delete_confirmation = None;
            }
            ExtensionsOutcome::Hook(snapshot) => {
                self.apply_hooks_snapshot(snapshot);
                self.hook_delete_confirmation = None;
            }
            ExtensionsOutcome::Refresh(snapshot, hooks) => {
                self.extensions = Some(snapshot);
                self.apply_hooks_snapshot(hooks);
            }
            ExtensionsOutcome::Activated(report) => {
                self.extensions = Some(report.snapshot.clone());
                self.extensions_activation = Some(report);
                self.mcp_editor = None;
                self.mcp_delete_confirmation = None;
            }
            ExtensionsOutcome::Content(preview) => {
                self.mcp_content_preview = Some(preview);
            }
        }
        UiModuleOutcome::dirty()
    }

    fn reduce_ui(
        &mut self,
        message: ExtensionsMessage,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        match message {
            ExtensionsMessage::Refresh => self.queue(ExtensionsCommand::Refresh {
                project_cwd: Self::project_cwd(cx),
            }),
            ExtensionsMessage::SkillIdChanged(value) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.skill_id_input = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::SkillDescriptionChanged(value) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.skill_description_input = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::CreateSkill => self.create_skill(),
            ExtensionsMessage::ToggleSkill(skill_id) => self.toggle_skill(&skill_id),
            ExtensionsMessage::RequestDeleteSkill(skill_id) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.skill_delete_confirmation = Some(skill_id);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ConfirmDeleteSkill => self.confirm_delete_skill(),
            ExtensionsMessage::CancelDeleteSkill => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.skill_delete_confirmation = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::PluginSourceChanged(value) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.plugin_source_input = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::PickPluginDirectory => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                UiModuleOutcome::effect(ShellEffect::PickPluginDirectory)
            }
            ExtensionsMessage::InstallPlugin => self.install_plugin(),
            ExtensionsMessage::TogglePlugin(plugin_id) => self.toggle_plugin(&plugin_id),
            ExtensionsMessage::RequestDeletePlugin(plugin_id) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.plugin_delete_confirmation = Some(plugin_id);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ConfirmDeletePlugin => self.confirm_delete_plugin(),
            ExtensionsMessage::CancelDeletePlugin => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.plugin_delete_confirmation = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::HookDraftChanged { source_id, value } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.hook_drafts.insert(source_id, value);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::AddHookHandler(source_id) => self.add_hook_handler_draft(&source_id),
            ExtensionsMessage::HookHandlerDraftChanged {
                source_id,
                index,
                field,
                value,
            } => self.update_hook_handler_draft(&source_id, index, field, value),
            ExtensionsMessage::RemoveHookHandler { source_id, index } => {
                self.remove_hook_handler_draft(&source_id, index)
            }
            ExtensionsMessage::CreateHookSource(source_id) => {
                self.create_hook_source(&source_id, cx)
            }
            ExtensionsMessage::SaveHookSource(source_id) => self.save_hook_source(&source_id, cx),
            ExtensionsMessage::ToggleHookSource(source_id) => {
                self.toggle_hook_source(&source_id, cx)
            }
            ExtensionsMessage::RequestDeleteHookSource(source_id) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.hook_delete_confirmation = Some(source_id);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ConfirmDeleteHookSource => self.confirm_delete_hook_source(cx),
            ExtensionsMessage::CancelDeleteHookSource => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.hook_delete_confirmation = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ActivateRegisteredMcp => {
                self.extensions_activation = None;
                self.mcp_content_preview = None;
                self.queue(ExtensionsCommand::ActivateMcp)
            }
            ExtensionsMessage::NewMcpServer => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_editor = Some(McpEditorState::default());
                self.mcp_delete_confirmation = None;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::EditMcpServer(server_id) => self.begin_edit_mcp_server(&server_id),
            ExtensionsMessage::McpServerIdChanged(value) => {
                if let Some(editor) = &mut self.mcp_editor {
                    if editor.editing_server_id.is_none() && !self.busy {
                        editor.server_id = value;
                        self.error = None;
                        return UiModuleOutcome::dirty();
                    }
                }
                UiModuleOutcome::clean()
            }
            ExtensionsMessage::CycleMcpTransport => {
                if let Some(editor) = &mut self.mcp_editor {
                    if !self.busy {
                        editor.transport = next_mcp_transport(editor.transport);
                        editor.location.clear();
                        editor.args_json = "[]".to_owned();
                        editor.credential_names_json = "[]".to_owned();
                        self.error = None;
                        return UiModuleOutcome::dirty();
                    }
                }
                UiModuleOutcome::clean()
            }
            ExtensionsMessage::McpLocationChanged(value) => self.edit_mcp_field(|editor| {
                editor.location = value;
            }),
            ExtensionsMessage::McpArgsChanged(value) => self.edit_mcp_field(|editor| {
                editor.args_json = value;
            }),
            ExtensionsMessage::McpCredentialNamesChanged(value) => self.edit_mcp_field(|editor| {
                editor.credential_names_json = value;
            }),
            ExtensionsMessage::ToggleMcpEditorEnabled => self.edit_mcp_field(|editor| {
                editor.enabled = !editor.enabled;
            }),
            ExtensionsMessage::SaveMcpServer => self.save_mcp_server(),
            ExtensionsMessage::CancelMcpEditor => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_editor = None;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ToggleMcpServer(server_id) => self.toggle_mcp_server(&server_id),
            ExtensionsMessage::RequestDeleteMcpServer(server_id) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_delete_confirmation = Some(server_id);
                self.mcp_editor = None;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::ConfirmDeleteMcpServer => self.confirm_delete_mcp_server(),
            ExtensionsMessage::CancelDeleteMcpServer => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_delete_confirmation = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::McpCredentialChanged {
                server_id,
                kind,
                name,
                value,
            } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_credential_drafts
                    .insert(mcp_credential_draft_key(&server_id, kind, &name), value);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::SaveMcpCredential {
                server_id,
                kind,
                name,
            } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                let key = mcp_credential_draft_key(&server_id, kind, &name);
                let Some(secret) = self.mcp_credential_drafts.remove(&key) else {
                    return UiModuleOutcome::clean();
                };
                self.extensions_activation = None;
                self.mcp_content_preview = None;
                self.queue(ExtensionsCommand::McpRegistry(
                    McpRegistryOperation::SetCredential {
                        server_id,
                        kind,
                        name,
                        secret: DesktopSecret::new(secret.into_bytes()),
                    },
                ))
            }
            ExtensionsMessage::DeleteMcpCredential {
                server_id,
                kind,
                name,
            } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_credential_drafts
                    .remove(&mcp_credential_draft_key(&server_id, kind, &name));
                self.extensions_activation = None;
                self.mcp_content_preview = None;
                self.queue(ExtensionsCommand::McpRegistry(
                    McpRegistryOperation::DeleteCredential {
                        server_id,
                        kind,
                        name,
                    },
                ))
            }
            ExtensionsMessage::ReadMcpResource { server_id, uri } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_content_preview = None;
                self.queue(ExtensionsCommand::McpContent(
                    McpContentOperation::ReadResource { server_id, uri },
                ))
            }
            ExtensionsMessage::McpPromptArgumentsChanged {
                namespaced_name,
                value,
            } => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.mcp_prompt_argument_drafts
                    .insert(namespaced_name, value);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            ExtensionsMessage::GetMcpPrompt(namespaced_name) => {
                let draft = self
                    .mcp_prompt_argument_drafts
                    .get(&namespaced_name)
                    .map(String::as_str)
                    .unwrap_or("{}");
                match parse_mcp_prompt_arguments(draft) {
                    Ok(arguments) => {
                        self.mcp_content_preview = None;
                        self.queue(ExtensionsCommand::McpContent(
                            McpContentOperation::GetPrompt {
                                namespaced_name,
                                arguments,
                            },
                        ))
                    }
                    Err(error) => {
                        self.error = Some(error);
                        UiModuleOutcome::dirty()
                    }
                }
            }
        }
    }

    fn edit_mcp_field(&mut self, edit: impl FnOnce(&mut McpEditorState)) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(editor) = &mut self.mcp_editor else {
            return UiModuleOutcome::clean();
        };
        edit(editor);
        self.error = None;
        UiModuleOutcome::dirty()
    }

    fn create_skill(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let skill_id = self.skill_id_input.trim();
        if skill_id.is_empty() {
            self.error = Some("请输入 Skill ID。".to_owned());
            return UiModuleOutcome::dirty();
        }
        let revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.skills_registry_revision)
            .unwrap_or_default();
        self.queue(ExtensionsCommand::Skill(SkillRegistryOperation::Create(
            DesktopSkillCreate {
                expected_registry_revision: revision,
                scope: DesktopSkillScope::User,
                project_cwd: None,
                skill_id: self.skill_id_input.clone(),
                description: self.skill_description_input.clone(),
            },
        )))
    }

    fn toggle_skill(&mut self, skill_id: &str) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some((enabled, revision)) = self.extensions.as_ref().and_then(|snapshot| {
            snapshot
                .skills
                .iter()
                .find(|skill| skill.skill_id == skill_id && skill.editable)
                .map(|skill| (!skill.enabled, snapshot.skills_registry_revision))
        }) else {
            return UiModuleOutcome::clean();
        };
        self.queue(ExtensionsCommand::Skill(
            SkillRegistryOperation::SetEnabled {
                skill_id: skill_id.to_owned(),
                enabled,
                expected_registry_revision: revision,
            },
        ))
    }

    fn confirm_delete_skill(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(skill_id) = self.skill_delete_confirmation.clone() else {
            return UiModuleOutcome::clean();
        };
        let revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.skills_registry_revision)
            .unwrap_or_default();
        self.queue(ExtensionsCommand::Skill(SkillRegistryOperation::Delete {
            skill_id,
            expected_registry_revision: revision,
        }))
    }

    fn install_plugin(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let source_path = self.plugin_source_input.trim();
        if source_path.is_empty() {
            self.error = Some("请选择 Plugin 目录。".to_owned());
            return UiModuleOutcome::dirty();
        }
        let revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.plugins_registry_revision)
            .unwrap_or_default();
        self.queue(ExtensionsCommand::Plugin(PluginRegistryOperation::Install(
            DesktopPluginInstall {
                expected_registry_revision: revision,
                source_path: source_path.to_owned(),
            },
        )))
    }

    fn toggle_plugin(&mut self, plugin_id: &str) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some((enabled, revision)) = self.extensions.as_ref().and_then(|snapshot| {
            snapshot
                .plugins
                .iter()
                .find(|plugin| plugin.plugin_id == plugin_id && plugin.editable)
                .map(|plugin| (!plugin.enabled, snapshot.plugins_registry_revision))
        }) else {
            return UiModuleOutcome::clean();
        };
        self.queue(ExtensionsCommand::Plugin(
            PluginRegistryOperation::SetEnabled {
                plugin_id: plugin_id.to_owned(),
                enabled,
                expected_registry_revision: revision,
            },
        ))
    }

    fn confirm_delete_plugin(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(plugin_id) = self.plugin_delete_confirmation.clone() else {
            return UiModuleOutcome::clean();
        };
        let revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.plugins_registry_revision)
            .unwrap_or_default();
        self.queue(ExtensionsCommand::Plugin(PluginRegistryOperation::Delete {
            plugin_id,
            expected_registry_revision: revision,
        }))
    }

    fn create_hook_source(&mut self, source_id: &str, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(source) = self.hook_source(source_id) else {
            return UiModuleOutcome::clean();
        };
        if source.exists {
            return UiModuleOutcome::clean();
        }
        self.queue_hook(
            HookSourceOperation::Create {
                scope: source.scope,
                project_cwd: source.project_cwd,
            },
            cx,
        )
    }

    fn save_hook_source(&mut self, source_id: &str, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(source) = self.hook_source(source_id) else {
            return UiModuleOutcome::clean();
        };
        if !source.exists {
            return UiModuleOutcome::clean();
        }
        let draft = self
            .hook_drafts
            .get(source_id)
            .map(String::as_str)
            .unwrap_or("[]");
        let handlers = match validated_hook_handlers(draft) {
            Ok(handlers) => handlers,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        self.queue_hook(
            HookSourceOperation::Update {
                scope: source.scope,
                project_cwd: source.project_cwd,
                input: DesktopHookDocumentUpdate {
                    expected_revision: source.revision,
                    handlers,
                },
            },
            cx,
        )
    }

    fn toggle_hook_source(&mut self, source_id: &str, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(source) = self.hook_source(source_id) else {
            return UiModuleOutcome::clean();
        };
        if !source.exists {
            return UiModuleOutcome::clean();
        }
        self.queue_hook(
            HookSourceOperation::SetEnabled {
                scope: source.scope,
                project_cwd: source.project_cwd,
                expected_revision: source.revision,
                enabled: !source.enabled,
            },
            cx,
        )
    }

    fn confirm_delete_hook_source(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(source_id) = self.hook_delete_confirmation.clone() else {
            return UiModuleOutcome::clean();
        };
        let Some(source) = self.hook_source(&source_id) else {
            return UiModuleOutcome::clean();
        };
        if !source.exists {
            return UiModuleOutcome::clean();
        }
        self.queue_hook(
            HookSourceOperation::Delete {
                scope: source.scope,
                project_cwd: source.project_cwd,
                expected_revision: source.revision,
            },
            cx,
        )
    }

    fn queue_hook(
        &mut self,
        operation: HookSourceOperation,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        self.queue(ExtensionsCommand::Hook {
            operation,
            overview_project_cwd: Self::project_cwd(cx),
        })
    }

    fn add_hook_handler_draft(&mut self, source_id: &str) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let draft = self
            .hook_drafts
            .get(source_id)
            .cloned()
            .unwrap_or_else(|| hook_handlers_draft(&[]));
        match add_hook_handler_to_draft(&draft) {
            Ok(draft) => {
                self.hook_drafts.insert(source_id.to_owned(), draft);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            Err(error) => {
                self.error = Some(error);
                UiModuleOutcome::dirty()
            }
        }
    }

    fn update_hook_handler_draft(
        &mut self,
        source_id: &str,
        index: usize,
        field: HookHandlerDraftField,
        value: String,
    ) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let draft = self
            .hook_drafts
            .get(source_id)
            .cloned()
            .unwrap_or_else(|| hook_handlers_draft(&[]));
        match edit_hook_handler_draft(&draft, index, field, value) {
            Ok(draft) => {
                self.hook_drafts.insert(source_id.to_owned(), draft);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            Err(error) => {
                self.error = Some(error);
                UiModuleOutcome::dirty()
            }
        }
    }

    fn remove_hook_handler_draft(&mut self, source_id: &str, index: usize) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let draft = self
            .hook_drafts
            .get(source_id)
            .cloned()
            .unwrap_or_else(|| hook_handlers_draft(&[]));
        match remove_hook_handler_from_draft(&draft, index) {
            Ok(draft) => {
                self.hook_drafts.insert(source_id.to_owned(), draft);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            Err(error) => {
                self.error = Some(error);
                UiModuleOutcome::dirty()
            }
        }
    }

    fn begin_edit_mcp_server(&mut self, server_id: &str) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(server) = self.extensions.as_ref().and_then(|snapshot| {
            snapshot
                .mcp_servers
                .iter()
                .find(|server| server.server_id == server_id && server.editable)
        }) else {
            return UiModuleOutcome::clean();
        };
        let transport = match server.transport.as_str() {
            "stdio" => DesktopMcpTransport::Stdio,
            "streamable_http" => DesktopMcpTransport::StreamableHttp,
            "sse" => DesktopMcpTransport::Sse,
            _ => return UiModuleOutcome::clean(),
        };
        self.mcp_editor = Some(McpEditorState {
            editing_server_id: Some(server.server_id.clone()),
            server_id: server.server_id.clone(),
            transport,
            location: server
                .command
                .clone()
                .or_else(|| server.url.clone())
                .unwrap_or_default(),
            args_json: serde_json::to_string(&server.args).unwrap_or_else(|_| "[]".to_owned()),
            credential_names_json: serde_json::to_string(
                &server
                    .credentials
                    .iter()
                    .map(|credential| credential.name.clone())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_owned()),
            enabled: server.enabled,
        });
        self.mcp_delete_confirmation = None;
        self.error = None;
        UiModuleOutcome::dirty()
    }

    fn save_mcp_server(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(editor) = self.mcp_editor.clone() else {
            return UiModuleOutcome::clean();
        };
        let args = match serde_json::from_str::<Vec<String>>(&editor.args_json) {
            Ok(args) => args,
            Err(_) => {
                self.error = Some("参数必须是字符串 JSON 数组。".to_owned());
                return UiModuleOutcome::dirty();
            }
        };
        let credential_names =
            match serde_json::from_str::<Vec<String>>(&editor.credential_names_json) {
                Ok(names) => names,
                Err(_) => {
                    self.error = Some("凭据名称必须是字符串 JSON 数组。".to_owned());
                    return UiModuleOutcome::dirty();
                }
            };
        let expected_registry_revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.mcp_registry_revision)
            .unwrap_or_default();
        let (command, args, url, env_secret_names, header_secret_names) = match editor.transport {
            DesktopMcpTransport::Stdio => (
                Some(editor.location),
                args,
                None,
                credential_names,
                Vec::new(),
            ),
            DesktopMcpTransport::StreamableHttp | DesktopMcpTransport::Sse => (
                None,
                Vec::new(),
                Some(editor.location),
                Vec::new(),
                credential_names,
            ),
        };
        self.extensions_activation = None;
        self.mcp_content_preview = None;
        self.queue(ExtensionsCommand::McpRegistry(
            McpRegistryOperation::Upsert(DesktopMcpServerUpsert {
                expected_registry_revision,
                server_id: editor.server_id,
                transport: editor.transport,
                command,
                args,
                url,
                env_secret_names,
                header_secret_names,
                enabled: editor.enabled,
            }),
        ))
    }

    fn toggle_mcp_server(&mut self, server_id: &str) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some((enabled, revision)) = self.extensions.as_ref().and_then(|snapshot| {
            snapshot
                .mcp_servers
                .iter()
                .find(|server| server.server_id == server_id && server.editable)
                .map(|server| (!server.enabled, snapshot.mcp_registry_revision))
        }) else {
            return UiModuleOutcome::clean();
        };
        self.extensions_activation = None;
        self.mcp_content_preview = None;
        self.queue(ExtensionsCommand::McpRegistry(
            McpRegistryOperation::SetEnabled {
                server_id: server_id.to_owned(),
                enabled,
                expected_registry_revision: revision,
            },
        ))
    }

    fn confirm_delete_mcp_server(&mut self) -> UiModuleOutcome {
        if self.busy {
            return UiModuleOutcome::clean();
        }
        let Some(server_id) = self.mcp_delete_confirmation.clone() else {
            return UiModuleOutcome::clean();
        };
        let revision = self
            .extensions
            .as_ref()
            .map(|snapshot| snapshot.mcp_registry_revision)
            .unwrap_or_default();
        self.extensions_activation = None;
        self.mcp_content_preview = None;
        self.queue(ExtensionsCommand::McpRegistry(
            McpRegistryOperation::Delete {
                server_id,
                expected_registry_revision: revision,
            },
        ))
    }
}

impl UiModule for ExtensionsModule {
    type Message = ExtensionsModuleMessage;

    fn feature(&self) -> FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            ExtensionsModuleMessage::Ui(message) => self.reduce_ui(message, cx),
            ExtensionsModuleMessage::ApplyOutcome(outcome) => self.apply_outcome(outcome),
            ExtensionsModuleMessage::SetBusy(busy) => {
                self.busy = busy;
                UiModuleOutcome::dirty()
            }
            ExtensionsModuleMessage::JobFailed(error) => {
                self.busy = false;
                self.error = Some(error);
                UiModuleOutcome::dirty()
            }
            ExtensionsModuleMessage::PluginDirectoryPicked(path) => {
                if self.busy {
                    return UiModuleOutcome::clean();
                }
                self.plugin_source_input = path;
                self.error = None;
                UiModuleOutcome::dirty()
            }
        }
    }

    fn invalidate(
        &mut self,
        envelope: &lilia_kernel::EventEnvelope,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        if envelope.is::<crate::application::HooksRegistryChanged>()
            || envelope.is::<crate::application::SkillsRegistryChanged>()
            || envelope.is::<crate::application::McpRegistryChanged>()
            || envelope.is::<crate::application::PluginsRegistryChanged>()
        {
            return self.queue(ExtensionsCommand::Refresh {
                project_cwd: Self::project_cwd(cx),
            });
        }
        UiModuleOutcome::clean()
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        if !cx.shows_settings_tab("extensions") {
            return;
        }
        into.settings.extensions_status = self.error.clone().unwrap_or_else(|| {
            self.extensions
                .as_ref()
                .map(|snapshot| {
                    format!(
                        "技能 {} · 插件 {} · MCP {}",
                        snapshot.skills.len(),
                        snapshot.plugins.len(),
                        snapshot.mcp_servers.len()
                    )
                })
                .unwrap_or_else(|| "尚未读取扩展。".to_owned())
        });
        into.settings.skills = self
            .extensions
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .skills
                    .iter()
                    .map(|skill| ShellSkillRow {
                        id: skill.skill_id.clone(),
                        label: if skill.description.trim().is_empty() {
                            skill.skill_id.clone()
                        } else {
                            skill.description.clone()
                        },
                        enabled: skill.enabled,
                    })
                    .collect()
            })
            .unwrap_or_default();
        into.settings.skill_id = self.skill_id_input.clone();
        into.settings.skill_description = self.skill_description_input.clone();
        into.settings.can_create_skill = !self.busy && !self.skill_id_input.trim().is_empty();
        into.settings.mcp_servers = self
            .extensions
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .mcp_servers
                    .iter()
                    .map(|server| ShellMcpRow {
                        id: server.server_id.clone(),
                        label: server.server_id.clone(),
                        enabled: server.enabled,
                    })
                    .collect()
            })
            .unwrap_or_default();
        into.settings.mcp_editor = self.mcp_editor.as_ref().map(|editor| ShellMcpEditor {
            server_id: editor.server_id.clone(),
            transport: editor.transport.as_registry().to_owned(),
            location: editor.location.clone(),
            args: editor.args_json.clone(),
            enabled: editor.enabled,
        });
    }
}

pub(crate) fn load_native_hooks(
    application: &crate::application::DesktopApplication,
    project_cwd: Option<&str>,
) -> Result<NativeHooksSnapshot, String> {
    let overview = application
        .hooks_overview(project_cwd)
        .map_err(|error| error.to_string())?;
    let documents = overview
        .sources
        .iter()
        .filter(|source| source.exists)
        .map(|source| {
            application
                .read_hook_source(source.scope, source.project_cwd.as_deref())
                .map(|document| (source.id.clone(), document))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(NativeHooksSnapshot {
        overview,
        documents,
    })
}

fn next_mcp_transport(transport: DesktopMcpTransport) -> DesktopMcpTransport {
    match transport {
        DesktopMcpTransport::Stdio => DesktopMcpTransport::StreamableHttp,
        DesktopMcpTransport::StreamableHttp => DesktopMcpTransport::Sse,
        DesktopMcpTransport::Sse => DesktopMcpTransport::Stdio,
    }
}

fn hook_document_draft(document: &DesktopHookDocumentView) -> String {
    let handlers = document
        .handlers
        .iter()
        .map(|handler| DesktopHookHandlerUpdate {
            id: Some(handler.id.clone()),
            event: handler.event.clone(),
            matcher: handler.matcher.clone(),
            handler_type: handler.handler_type.clone(),
            command: handler.command.clone(),
            command_windows: handler.command_windows.clone(),
            timeout_seconds: handler.timeout_seconds,
            status_message: handler.status_message.clone(),
        })
        .collect::<Vec<_>>();
    hook_handlers_draft(&handlers)
}

fn empty_hook_handler_draft() -> DesktopHookHandlerUpdate {
    DesktopHookHandlerUpdate {
        id: None,
        event: String::new(),
        matcher: None,
        handler_type: "command".to_owned(),
        command: None,
        command_windows: None,
        timeout_seconds: None,
        status_message: None,
    }
}

pub(crate) fn hook_handlers_draft(handlers: &[DesktopHookHandlerUpdate]) -> String {
    let handlers = if handlers.is_empty() {
        vec![empty_hook_handler_draft()]
    } else {
        handlers.to_vec()
    };
    serde_json::to_string(&handlers).expect("Hook handler drafts are serializable")
}

pub(crate) fn parse_hook_handlers_draft(
    draft: &str,
) -> Result<Vec<DesktopHookHandlerUpdate>, String> {
    serde_json::from_str(draft).map_err(|error| format!("Handlers 格式无效：{error}"))
}

pub(crate) fn edit_hook_handler_draft(
    draft: &str,
    index: usize,
    field: HookHandlerDraftField,
    value: String,
) -> Result<String, String> {
    let mut handlers = parse_hook_handlers_draft(draft)?;
    let handler = handlers
        .get_mut(index)
        .ok_or_else(|| "要编辑的 Handler 已不存在，请刷新后重试。".to_owned())?;
    match field {
        HookHandlerDraftField::Event => handler.event = value,
        HookHandlerDraftField::Matcher => handler.matcher = optional_hook_text(value),
        HookHandlerDraftField::Type => handler.handler_type = value,
        HookHandlerDraftField::TimeoutSeconds => {
            let value = value.trim();
            handler.timeout_seconds = if value.is_empty() {
                None
            } else {
                let timeout = value
                    .parse::<u64>()
                    .map_err(|_| "Timeout 必须是 1 到 300 秒之间的整数。".to_owned())?;
                if !(1..=300).contains(&timeout) {
                    return Err("Timeout 必须是 1 到 300 秒之间的整数。".to_owned());
                }
                Some(timeout)
            };
        }
        HookHandlerDraftField::Command => handler.command = optional_hook_text(value),
        HookHandlerDraftField::CommandWindows => {
            handler.command_windows = optional_hook_text(value);
        }
        HookHandlerDraftField::StatusMessage => {
            handler.status_message = optional_hook_text(value);
        }
    }
    Ok(hook_handlers_draft(&handlers))
}

pub(crate) fn add_hook_handler_to_draft(draft: &str) -> Result<String, String> {
    let mut handlers = parse_hook_handlers_draft(draft)?;
    handlers.push(empty_hook_handler_draft());
    Ok(hook_handlers_draft(&handlers))
}

pub(crate) fn remove_hook_handler_from_draft(draft: &str, index: usize) -> Result<String, String> {
    let mut handlers = parse_hook_handlers_draft(draft)?;
    if index >= handlers.len() {
        return Err("要删除的 Handler 已不存在，请刷新后重试。".to_owned());
    }
    handlers.remove(index);
    Ok(hook_handlers_draft(&handlers))
}

pub(crate) fn validated_hook_handlers(
    draft: &str,
) -> Result<Vec<DesktopHookHandlerUpdate>, String> {
    parse_hook_handlers_draft(draft)?
        .into_iter()
        .map(|mut handler| {
            handler.event = handler.event.trim().to_owned();
            if handler.event.is_empty() {
                return Err("每条 Hook 都需要事件。".to_owned());
            }
            handler.handler_type = handler.handler_type.trim().to_owned();
            if handler.handler_type.is_empty() {
                return Err("每条 Hook 都需要类型。".to_owned());
            }
            if handler
                .timeout_seconds
                .is_some_and(|timeout| !(1..=300).contains(&timeout))
            {
                return Err("Timeout 必须是 1 到 300 秒之间的整数。".to_owned());
            }
            handler.matcher = handler.matcher.and_then(optional_hook_text);
            handler.command = handler.command.and_then(optional_hook_text);
            handler.command_windows = handler.command_windows.and_then(optional_hook_text);
            handler.status_message = handler.status_message.and_then(optional_hook_text);
            Ok(handler)
        })
        .collect()
}

fn optional_hook_text(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn mcp_credential_kind_key(kind: DesktopMcpCredentialKind) -> &'static str {
    match kind {
        DesktopMcpCredentialKind::Environment => "env",
        DesktopMcpCredentialKind::Header => "header",
    }
}

pub(crate) fn mcp_credential_draft_key(
    server_id: &str,
    kind: DesktopMcpCredentialKind,
    name: &str,
) -> String {
    format!("{server_id}\0{}\0{name}", mcp_credential_kind_key(kind))
}

pub(crate) fn parse_mcp_prompt_arguments(value: &str) -> Result<Value, String> {
    let value = value.trim();
    let arguments = serde_json::from_str::<Value>(if value.is_empty() { "{}" } else { value })
        .map_err(|_| "提示词参数必须是有效的 JSON 对象。".to_owned())?;
    if !arguments.is_object() {
        return Err("提示词参数必须是 JSON 对象。".to_owned());
    }
    Ok(arguments)
}

pub(crate) fn mcp_resource_preview(
    resource: crate::application::DesktopMcpResourceReadView,
) -> McpContentPreview {
    let text = resource
        .contents
        .into_iter()
        .map(|content| {
            content.text.unwrap_or_else(|| {
                content.encoded_blob_length.map_or_else(
                    || format!("{} 没有可显示的内容", content.uri),
                    |length| format!("{} · 二进制内容（编码长度 {length}）", content.uri),
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    McpContentPreview {
        kind: McpContentKind::Resource,
        title: resource.uri,
        text: if text.is_empty() {
            "资源未返回内容。".to_owned()
        } else {
            text
        },
    }
}

pub(crate) fn mcp_prompt_preview(
    prompt: crate::application::DesktopMcpPromptGetView,
) -> McpContentPreview {
    let text = prompt
        .fragments
        .into_iter()
        .map(|fragment| fragment.content)
        .collect::<Vec<_>>()
        .join("\n\n");
    McpContentPreview {
        kind: McpContentKind::Prompt,
        title: prompt.namespaced_name,
        text: if text.is_empty() {
            "提示词未返回内容。".to_owned()
        } else {
            text
        },
    }
}
