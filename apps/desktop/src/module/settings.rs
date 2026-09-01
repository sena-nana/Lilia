//! Settings-page agent drafts as a UI module.
//!
//! Surface open/close, tab selection and NanaUI appearance stay with the shell.
//! This module owns the Agent 页签's custom-agent editor and toggle copy.

use lilia_kernel::FeatureId;

use crate::application::{DesktopAgentInteractionSettings, DesktopCustomSubagentCatalog};
use crate::runtime_shell::{PrimaryShellSnapshot, ShellActionRow, ShellAgentRow};
use crate::text_editor_state::TextEditorState;
use crate::ui_module::{UiModule, UiModuleContext, UiModuleOutcome};

#[derive(Debug, Clone)]
pub enum SettingsModuleMessage {
    SetCatalog(DesktopCustomSubagentCatalog),
    SetInteraction(DesktopAgentInteractionSettings),
    NameChanged(String),
    DescriptionChanged(String),
    InstructionEdited(String),
    InstructionReplaced(String),
    BeginEdit {
        id: String,
        name: String,
        description: String,
        instruction: String,
    },
    BeginNew,
    CancelEdit,
    SetShortcut(String),
}

pub struct SettingsModule {
    catalog: DesktopCustomSubagentCatalog,
    interaction: DesktopAgentInteractionSettings,
    editor_open: bool,
    editing_id: Option<String>,
    name: String,
    description: String,
    instruction: TextEditorState,
    shortcut: String,
    shortcut_capturing: bool,
    error: Option<String>,
}

impl Default for SettingsModule {
    fn default() -> Self {
        Self {
            catalog: DesktopCustomSubagentCatalog::default(),
            interaction: DesktopAgentInteractionSettings::default(),
            editor_open: false,
            editing_id: None,
            name: String::new(),
            description: String::new(),
            instruction: TextEditorState::new(),
            shortcut: String::new(),
            shortcut_capturing: false,
            error: None,
        }
    }
}

impl SettingsModule {
    pub fn feature_id() -> FeatureId {
        FeatureId::new("lilia.settings").expect("the settings feature id is not blank")
    }

    pub fn editor_open(&self) -> bool {
        self.editor_open
    }

    pub fn editing_id(&self) -> Option<&str> {
        self.editing_id.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn instruction(&self) -> &TextEditorState {
        &self.instruction
    }

    fn clear_editor(&mut self) {
        self.editor_open = false;
        self.editing_id = None;
        self.name.clear();
        self.description.clear();
        self.instruction.clear();
        self.error = None;
    }
}

impl UiModule for SettingsModule {
    type Message = SettingsModuleMessage;

    fn feature(&self) -> FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, _cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            SettingsModuleMessage::SetCatalog(catalog) => {
                self.catalog = catalog;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::SetInteraction(interaction) => {
                self.interaction = interaction;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::NameChanged(value) => {
                self.name = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::DescriptionChanged(value) => {
                self.description = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::InstructionEdited(action) => {
                self.instruction.perform(action);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::InstructionReplaced(value) => {
                self.instruction.set_text(&value);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::BeginEdit {
                id,
                name,
                description,
                instruction,
            } => {
                self.editor_open = true;
                self.editing_id = Some(id);
                self.name = name;
                self.description = description;
                self.instruction.set_text(&instruction);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::BeginNew => {
                self.clear_editor();
                self.editor_open = true;
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::CancelEdit => {
                self.clear_editor();
                UiModuleOutcome::dirty()
            }
            SettingsModuleMessage::SetShortcut(value) => {
                self.shortcut = value;
                self.shortcut_capturing = false;
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
        if !envelope.is::<crate::application::AgentInteractionChanged>() {
            return UiModuleOutcome::clean();
        }
        let Ok(application) = cx.application() else {
            return UiModuleOutcome::clean();
        };
        let (Ok(settings), Ok(catalog)) = (
            application.agent_interaction_settings(),
            application.custom_subagent_catalog(),
        ) else {
            return UiModuleOutcome::clean();
        };
        self.interaction = settings;
        self.catalog = catalog;
        UiModuleOutcome::dirty()
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        if !cx.shows_settings_tab("agent") && !cx.shows_settings_tab("desktop") {
            return;
        }
        if cx.shows_settings_tab("agent") {
            let auto = &self.interaction.auto_turn_decision;
            into.settings.agent_actions = vec![
                ShellActionRow {
                    id: "non_interrupt".into(),
                    label: if self.interaction.non_interrupt_mode {
                        "非中断：开".into()
                    } else {
                        "非中断：关".into()
                    },
                },
                ShellActionRow {
                    id: "debug".into(),
                    label: if self.interaction.debug {
                        "调试：开".into()
                    } else {
                        "调试：关".into()
                    },
                },
                ShellActionRow {
                    id: "subagents".into(),
                    label: if self.interaction.subagent_mode.enabled {
                        "子代理：开".into()
                    } else {
                        "子代理：关".into()
                    },
                },
                ShellActionRow {
                    id: "auto_turn".into(),
                    label: if auto.enabled {
                        "自动回合：开".into()
                    } else {
                        "自动回合：关".into()
                    },
                },
            ];
            into.settings.custom_agents = self
                .catalog
                .agents
                .iter()
                .map(|agent| ShellAgentRow {
                    id: agent.id.clone(),
                    label: agent.name.clone(),
                    enabled: agent.enabled,
                })
                .collect();
            into.settings.custom_agent_editor_open = self.editor_open;
            into.settings.custom_agent_name = self.name.clone();
            into.settings.custom_agent_description = self.description.clone();
            into.settings.custom_agent_instruction = self.instruction.text();
        }
        if cx.shows_settings_tab("desktop") {
            into.settings.shortcut = self.shortcut.clone();
            into.settings.shortcut_capturing = self.shortcut_capturing;
        }
    }
}
