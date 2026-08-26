//! The Memory page as a UI module.
//!
//! Owns the memory list, the editor fields, the injection settings and the
//! selected task's injection state. One instance per window: the editor is
//! window state, and the settings are read back from the service on refresh so
//! two windows cannot disagree about what is stored.

use lilia_kernel::FeatureId;

use crate::application::{
    DesktopMemory, MemoryInjectionState, MemoryScope, MemorySettings, MemoryUpsertInput,
    ProjectWorkspaceSurface,
};
use crate::runtime_shell::{PrimaryShellSnapshot, ShellProjectPage};
use crate::text_editor_state::TextEditorState;
use crate::ui_module::{ShellEffect, UiModule, UiModuleContext, UiModuleOutcome};

/// The memory domain's own message vocabulary.
#[derive(Debug, Clone)]
pub enum MemoryMessage {
    Open,
    Refresh,
    Select(String),
    New,
    TitleChanged(String),
    BodyReplaced(String),
    TagsChanged(String),
    ToggleScope,
    Save,
    ToggleEnabled,
    Delete,
    ToggleGlobal,
    ToggleBaseline,
    CycleCooldown,
    CooldownChanged(String),
    SaveCooldown,
    ToggleTaskInjection,
    ResetTaskCooldown,
}

pub struct MemoryModule {
    memories: Vec<DesktopMemory>,
    selected: Option<String>,
    title: String,
    body: TextEditorState,
    tags: String,
    scope: MemoryScope,
    updated_at: Option<i64>,
    error: Option<String>,
    settings: MemorySettings,
    cooldown_input: String,
    injection: Option<MemoryInjectionState>,
}

impl Default for MemoryModule {
    fn default() -> Self {
        let settings = MemorySettings::default();
        Self {
            memories: Vec::new(),
            selected: None,
            title: String::new(),
            body: TextEditorState::new(),
            tags: String::new(),
            scope: MemoryScope::Project,
            updated_at: None,
            error: None,
            cooldown_input: settings.cooldown_turns.to_string(),
            settings,
            injection: None,
        }
    }
}

impl MemoryModule {
    pub fn feature_id() -> FeatureId {
        FeatureId::new("lilia.memory").expect("the memory feature id is not blank")
    }

    pub fn memories(&self) -> &[DesktopMemory] {
        &self.memories
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body(&self) -> &TextEditorState {
        &self.body
    }

    pub fn scope(&self) -> MemoryScope {
        self.scope
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn settings(&self) -> &MemorySettings {
        &self.settings
    }

    pub fn cooldown_input(&self) -> &str {
        &self.cooldown_input
    }

    pub fn injection(&self) -> Option<&MemoryInjectionState> {
        self.injection.as_ref()
    }

    /// Restores the memory a window remembered, falling back to the first one
    /// when the saved id no longer exists.
    pub fn restore_selection(&mut self, memory_id: Option<String>) {
        self.selected = memory_id
            .filter(|selected| self.has(selected))
            .or_else(|| self.memories.first().map(|memory| memory.id.clone()));
        self.load_selected();
    }

    fn has(&self, memory_id: &str) -> bool {
        self.memories.iter().any(|memory| memory.id == memory_id)
    }

    fn memory(&self) -> Option<&DesktopMemory> {
        let selected = self.selected.as_deref()?;
        self.memories.iter().find(|memory| memory.id == selected)
    }

    fn refresh(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.memory_settings() {
            Ok(settings) => {
                if settings.cooldown_turns != self.settings.cooldown_turns {
                    self.cooldown_input = settings.cooldown_turns.to_string();
                }
                self.settings = settings;
            }
            Err(error) => self.error = Some(format!("无法读取 Memory 设置：{error}")),
        }
        self.refresh_injection(cx);
        let Some(project_id) = cx.selected_project() else {
            self.memories.clear();
            self.selected = None;
            self.clear_editor();
            return UiModuleOutcome::dirty();
        };
        match application.list_memories(Some(&project_id)) {
            Ok(memories) => {
                self.memories = memories;
                if !self
                    .selected
                    .as_deref()
                    .is_some_and(|selected| self.has(selected))
                {
                    self.selected = self.memories.first().map(|memory| memory.id.clone());
                }
                self.load_selected();
                self.error = None;
            }
            Err(error) => self.error = Some(format!("无法读取 Memory：{error}")),
        }
        UiModuleOutcome::dirty()
    }

    fn select(&mut self, memory_id: String) -> UiModuleOutcome {
        if !self.has(&memory_id) {
            return UiModuleOutcome::clean();
        }
        self.selected = Some(memory_id);
        self.load_selected();
        UiModuleOutcome::dirty()
    }

    /// Empties the editor, which is also how a new memory is started: an unsaved
    /// draft is the absence of a selection.
    fn clear_editor(&mut self) {
        self.selected = None;
        self.title.clear();
        self.body.clear();
        self.tags.clear();
        self.scope = MemoryScope::Project;
        self.updated_at = None;
        self.error = None;
    }

    fn load_selected(&mut self) {
        let loaded = self.memory().map(|memory| {
            (
                memory.title.clone(),
                memory.body.clone(),
                memory.tags.join(", "),
                memory.scope,
                memory.updated_at,
            )
        });
        match loaded {
            Some((title, body, tags, scope, updated_at)) => {
                self.title = title;
                self.body.set_text(&body);
                self.tags = tags;
                self.scope = scope;
                self.updated_at = Some(updated_at);
            }
            None => self.clear_editor(),
        }
    }

    fn save(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        let project_id = match self.scope {
            MemoryScope::User => None,
            MemoryScope::Project => cx
                .selected_project()
                .map(|project_id| project_id.as_str().to_owned()),
        };
        let input = MemoryUpsertInput {
            id: self.selected.clone(),
            scope: self.scope,
            project_id,
            title: self.title.clone(),
            body: self.body.text(),
            tags: crate::desktop::parse_memory_tags(&self.tags),
            enabled: self.memory().is_none_or(|memory| memory.enabled),
            source_task_id: None,
            expected_updated_at: self.updated_at,
        };
        match application.save_memory(input) {
            Ok(memory) => {
                self.selected = Some(memory.id);
                self.refresh(cx)
            }
            Err(error) => {
                self.error = Some(format!("无法保存 Memory：{error}"));
                UiModuleOutcome::dirty()
            }
        }
    }

    fn toggle_enabled(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some((memory_id, enabled, expected_updated_at)) = self
            .memory()
            .map(|memory| (memory.id.clone(), !memory.enabled, Some(memory.updated_at)))
        else {
            return UiModuleOutcome::clean();
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.set_memory_enabled(&memory_id, enabled, expected_updated_at) {
            Ok(_) => self.refresh(cx),
            Err(error) => {
                self.error = Some(format!("无法更新 Memory 状态：{error}"));
                UiModuleOutcome::dirty()
            }
        }
    }

    fn delete(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(memory_id) = self.selected.clone() else {
            return UiModuleOutcome::clean();
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.delete_memory(&memory_id, self.updated_at) {
            Ok(_) => {
                self.selected = None;
                self.refresh(cx)
            }
            Err(error) => {
                self.error = Some(format!("无法删除 Memory：{error}"));
                UiModuleOutcome::dirty()
            }
        }
    }

    fn save_settings(
        &mut self,
        settings: MemorySettings,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        let cooldown_changed = settings.cooldown_turns != self.settings.cooldown_turns;
        match application.save_memory_settings(settings) {
            Ok(settings) => {
                if cooldown_changed {
                    self.cooldown_input = settings.cooldown_turns.to_string();
                }
                self.settings = settings;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("无法保存 Memory 设置：{error}")),
        }
        UiModuleOutcome::dirty()
    }

    fn save_cooldown(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match crate::desktop::parse_memory_cooldown(&self.cooldown_input) {
            Ok(cooldown_turns) => {
                let mut settings = self.settings.clone();
                settings.cooldown_turns = cooldown_turns;
                self.save_settings(settings, cx)
            }
            Err(error) => {
                self.error = Some(error.to_owned());
                UiModuleOutcome::dirty()
            }
        }
    }

    /// Reloads the injection state of the task this window has open. Task-scoped
    /// rather than project-scoped, so a window with no task has none.
    fn refresh_injection(&mut self, cx: &UiModuleContext<'_>) {
        let Some(task_id) = cx.selected_task() else {
            self.injection = None;
            return;
        };
        let Ok(application) = cx.application() else {
            return;
        };
        match application.memory_injection_state(&task_id) {
            Ok(state) => self.injection = Some(state),
            Err(error) => self.error = Some(format!("无法读取 Memory 注入状态：{error}")),
        }
    }

    fn toggle_task_injection(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let (Some(task_id), Some(state)) = (cx.selected_task(), self.injection.clone()) else {
            return UiModuleOutcome::clean();
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.set_task_memory_enabled(&task_id, !state.enabled, Some(state.updated_at)) {
            Ok(state) => {
                self.injection = Some(state);
                self.error = None;
            }
            Err(error) => self.error = Some(format!("无法更新 Memory 注入状态：{error}")),
        }
        self.refresh_injection(cx);
        UiModuleOutcome::dirty()
    }

    fn reset_task_cooldown(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let (Some(task_id), Some(state)) = (cx.selected_task(), self.injection.clone()) else {
            return UiModuleOutcome::clean();
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.reset_task_memory_cooldown(&task_id, Some(state.updated_at)) {
            Ok(state) => {
                self.injection = Some(state);
                self.error = None;
            }
            Err(error) => self.error = Some(format!("无法重置 Memory 冷却：{error}")),
        }
        UiModuleOutcome::dirty()
    }
}

impl UiModule for MemoryModule {
    type Message = MemoryMessage;

    fn feature(&self) -> FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            MemoryMessage::Open => UiModuleOutcome::effect(ShellEffect::RevealProjectSurface(
                ProjectWorkspaceSurface::Memory,
            )),
            MemoryMessage::Refresh => self.refresh(cx),
            MemoryMessage::Select(memory_id) => self.select(memory_id),
            MemoryMessage::New => {
                self.clear_editor();
                UiModuleOutcome::dirty()
            }
            MemoryMessage::TitleChanged(value) => {
                self.title = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            MemoryMessage::BodyReplaced(value) => {
                self.body.set_text(&value);
                self.error = None;
                UiModuleOutcome::dirty()
            }
            MemoryMessage::TagsChanged(value) => {
                self.tags = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            MemoryMessage::ToggleScope => {
                self.scope = match self.scope {
                    MemoryScope::User => MemoryScope::Project,
                    MemoryScope::Project => MemoryScope::User,
                };
                UiModuleOutcome::dirty()
            }
            MemoryMessage::Save => self.save(cx),
            MemoryMessage::ToggleEnabled => self.toggle_enabled(cx),
            MemoryMessage::Delete => self.delete(cx),
            MemoryMessage::ToggleGlobal => {
                let mut settings = self.settings.clone();
                settings.enabled = !settings.enabled;
                self.save_settings(settings, cx)
            }
            MemoryMessage::ToggleBaseline => {
                let mut settings = self.settings.clone();
                settings.baseline_injection_enabled = !settings.baseline_injection_enabled;
                self.save_settings(settings, cx)
            }
            MemoryMessage::CycleCooldown => {
                let mut settings = self.settings.clone();
                settings.cooldown_turns =
                    crate::desktop::next_memory_cooldown(settings.cooldown_turns);
                self.save_settings(settings, cx)
            }
            MemoryMessage::CooldownChanged(value) => {
                self.cooldown_input = value;
                self.error = None;
                UiModuleOutcome::dirty()
            }
            MemoryMessage::SaveCooldown => self.save_cooldown(cx),
            MemoryMessage::ToggleTaskInjection => self.toggle_task_injection(cx),
            MemoryMessage::ResetTaskCooldown => self.reset_task_cooldown(cx),
        }
    }

    fn invalidate(
        &mut self,
        envelope: &lilia_kernel::EventEnvelope,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        if let Some(event) = envelope.downcast::<crate::application::MemoryChanged>() {
            if event
                .project_id
                .as_ref()
                .is_none_or(|project_id| cx.selected_project().as_ref() == Some(project_id))
            {
                return self.refresh(cx);
            }
        }
        if envelope.is::<crate::application::MemoryInjectionChanged>() {
            self.refresh_injection(cx);
            return UiModuleOutcome::dirty();
        }
        if envelope.is::<crate::application::MemorySettingsChanged>() {
            return self.refresh(cx);
        }
        UiModuleOutcome::clean()
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        if !cx.shows(ShellProjectPage::Memory) {
            return;
        }
        into.project_page_body = self.error.clone().unwrap_or_else(|| {
            self.memories
                .iter()
                .map(|memory| memory.title.clone())
                .collect::<Vec<_>>()
                .join("\n")
        });
    }
}
