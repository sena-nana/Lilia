//! The composer input surface as a UI module.
//!
//! Owns the per-window draft, the editor, and the slash / mention / reference
//! suggestion lists. Submit, todo, goal and the task session itself stay in the
//! shell. One instance per window: a popup's composer is that window's host,
//! not a field on the popup struct.

use lilia_contracts::{ChatContextSearchResult, ChatConversationReference, ProjectId};
use lilia_kernel::FeatureId;

use crate::application::{
    DesktopComposerCommand, DesktopComposerState, DesktopSlashCommandSearchResult,
};
use crate::runtime_shell::{
    PrimaryShellSnapshot, ShellAttachmentRow, ShellMentionItem, ShellSlashItem,
};
use crate::text_editor_state::TextEditorState;
use crate::ui_module::{UiModule, UiModuleContext, UiModuleOutcome};

const TEXTAREA_MIN_HEIGHT: f32 = 32.0;
const TEXTAREA_LINE_HEIGHT: f32 = 20.0;
const TEXTAREA_MAX_HEIGHT: f32 = 72.0;

/// The composer's own message vocabulary. Window identity comes from the host,
/// so popup and primary share one enum.
#[derive(Debug, Clone)]
pub enum ComposerMessage {
    Refresh,
    LoadTransient {
        composer: DesktopComposerState,
        project_id: Option<ProjectId>,
    },
    Clear,
    SetContent(String),
    Edited(String),
    ApplyCommand(DesktopComposerCommand),
    SelectConversationReference(String),
    SelectContextAttachment(String),
    RefreshSuggestions,
    ClearSuggestions,
}

pub struct ComposerModule {
    composer: Option<DesktopComposerState>,
    composer_editor: TextEditorState,
    slash_commands: Vec<DesktopSlashCommandSearchResult>,
    conversation_reference_results: Vec<ChatConversationReference>,
    context_attachment_results: Vec<ChatContextSearchResult>,
    transient: bool,
    transient_project: Option<ProjectId>,
    error: Option<String>,
}

impl Default for ComposerModule {
    fn default() -> Self {
        Self {
            composer: None,
            composer_editor: TextEditorState::new(),
            slash_commands: Vec::new(),
            conversation_reference_results: Vec::new(),
            context_attachment_results: Vec::new(),
            transient: false,
            transient_project: None,
            error: None,
        }
    }
}

impl ComposerModule {
    pub fn feature_id() -> FeatureId {
        FeatureId::new("lilia.composer").expect("the composer feature id is not blank")
    }

    pub fn composer(&self) -> Option<&DesktopComposerState> {
        self.composer.as_ref()
    }

    pub fn composer_editor(&self) -> &TextEditorState {
        &self.composer_editor
    }

    pub fn slash_commands(&self) -> &[DesktopSlashCommandSearchResult] {
        &self.slash_commands
    }

    pub fn conversation_reference_results(&self) -> &[ChatConversationReference] {
        &self.conversation_reference_results
    }

    pub fn context_attachment_results(&self) -> &[ChatContextSearchResult] {
        &self.context_attachment_results
    }

    pub fn is_transient(&self) -> bool {
        self.transient
    }

    fn apply_state(&mut self, composer: DesktopComposerState) {
        crate::desktop::sync_hosted_textarea(&self.composer_editor, &composer.content);
        self.composer = Some(composer);
    }

    fn clear_state(&mut self) {
        self.composer = None;
        self.composer_editor.clear();
        self.transient = false;
        self.transient_project = None;
        self.error = None;
        self.clear_suggestions();
    }

    fn clear_suggestions(&mut self) {
        self.slash_commands.clear();
        self.conversation_reference_results.clear();
        self.context_attachment_results.clear();
    }

    fn refresh(&mut self, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        let Some(task_id) = cx.selected_task() else {
            self.clear_state();
            return UiModuleOutcome::dirty();
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(error) => {
                self.error = Some(error);
                return UiModuleOutcome::dirty();
            }
        };
        match application.composer_state(&task_id) {
            Ok(composer) => {
                self.transient = false;
                self.transient_project = None;
                self.error = None;
                self.apply_state(composer);
                self.refresh_suggestions(cx);
                UiModuleOutcome::dirty()
            }
            Err(_) => {
                self.clear_state();
                self.error = Some("无法读取输入内容，请重试。".to_owned());
                UiModuleOutcome::dirty()
            }
        }
    }

    fn load_transient(
        &mut self,
        composer: DesktopComposerState,
        project_id: Option<ProjectId>,
    ) -> UiModuleOutcome {
        self.transient = true;
        self.transient_project = project_id;
        self.apply_state(composer);
        self.clear_suggestions();
        UiModuleOutcome::dirty()
    }

    fn attachments_locked(&self, cx: &UiModuleContext<'_>) -> bool {
        if self.transient {
            return false;
        }
        cx.task_session()
            .is_some_and(|session| session.has_open_composer_interaction())
    }

    fn apply_command(
        &mut self,
        command: DesktopComposerCommand,
        cx: &UiModuleContext<'_>,
        refresh_suggestions: bool,
    ) -> UiModuleOutcome {
        if self.attachments_locked(cx) && attachment_command(&command) {
            return UiModuleOutcome::clean();
        }
        if self.transient {
            let Some(composer) = self.composer.as_mut() else {
                return UiModuleOutcome::clean();
            };
            return match composer.apply_transient_command(command) {
                Ok(_) => {
                    self.error = None;
                    let composer = composer.clone();
                    self.apply_state(composer);
                    if refresh_suggestions {
                        self.refresh_suggestions(cx);
                    }
                    UiModuleOutcome::dirty()
                }
                Err(error) => {
                    eprintln!("failed to update Native transient composer: {error}");
                    self.error = Some("无法更新输入内容，请重试。".to_owned());
                    UiModuleOutcome::dirty()
                }
            };
        }
        let Some(task_id) = self
            .composer
            .as_ref()
            .map(|composer| composer.task_id.clone())
            .or_else(|| cx.selected_task())
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
        match application.execute_composer_command(&task_id, command) {
            Ok(composer) => {
                self.error = None;
                self.apply_state(composer);
                if refresh_suggestions {
                    self.refresh_suggestions(cx);
                }
                UiModuleOutcome::dirty()
            }
            Err(_) => {
                self.error = Some("无法更新输入内容，请重试。".to_owned());
                UiModuleOutcome::dirty()
            }
        }
    }

    fn set_content(&mut self, value: String, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        self.apply_command(DesktopComposerCommand::SetContent(value), cx, true)
    }

    fn select_conversation_reference(
        &mut self,
        referenced_task_id: &str,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        let Some(reference) = self
            .conversation_reference_results
            .iter()
            .find(|reference| reference.task_id == referenced_task_id)
            .cloned()
        else {
            return UiModuleOutcome::clean();
        };
        let Some(composer) = self.composer.clone() else {
            return UiModuleOutcome::clean();
        };
        let Some(content) = crate::desktop::composer_content_without_trigger(&composer.content, '#')
        else {
            return UiModuleOutcome::clean();
        };
        let outcome = self.apply_command(
            DesktopComposerCommand::ApplyConversationReference {
                expected_revision: composer.revision,
                content,
                reference,
            },
            cx,
            false,
        );
        if self.error.is_none() && outcome.dirty {
            self.clear_suggestions();
        }
        outcome
    }

    fn select_context_attachment(
        &mut self,
        relative_path: &str,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        let Some(attachment) = self
            .context_attachment_results
            .iter()
            .find(|result| result.relative_path == relative_path)
            .map(|result| result.attachment.clone())
        else {
            return UiModuleOutcome::clean();
        };
        let Some(composer) = self.composer.clone() else {
            return UiModuleOutcome::clean();
        };
        let Some(content) = crate::desktop::composer_content_without_trigger(&composer.content, '@')
        else {
            return UiModuleOutcome::clean();
        };
        let outcome = self.apply_command(
            DesktopComposerCommand::ApplyContextAttachment {
                expected_revision: composer.revision,
                content,
                attachment,
            },
            cx,
            false,
        );
        if self.error.is_none() && outcome.dirty {
            self.clear_suggestions();
        }
        outcome
    }

    fn refresh_suggestions(&mut self, cx: &UiModuleContext<'_>) {
        self.refresh_slash_commands(cx);
        self.refresh_conversation_references(cx);
        self.refresh_context_attachments(cx);
    }

    fn refresh_slash_commands(&mut self, cx: &UiModuleContext<'_>) {
        let Some(composer) = self.composer.as_ref() else {
            self.slash_commands.clear();
            return;
        };
        let Some(query) = crate::desktop::composer_slash_query(&composer.content) else {
            self.slash_commands.clear();
            return;
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(_) => {
                self.slash_commands.clear();
                return;
            }
        };
        let commands = if self.transient {
            application.search_project_slash_commands(self.transient_project.as_ref(), &query, 8)
        } else {
            application.search_task_slash_commands(&composer.task_id, &query, 8)
        };
        match commands {
            Ok(commands) => self.slash_commands = commands,
            Err(error) => {
                eprintln!("failed to search Native slash commands: {error}");
                self.slash_commands.clear();
            }
        }
    }

    fn refresh_conversation_references(&mut self, cx: &UiModuleContext<'_>) {
        let Some(composer) = self.composer.as_ref() else {
            self.conversation_reference_results.clear();
            return;
        };
        let Some(query) = crate::desktop::composer_conversation_query(&composer.content) else {
            self.conversation_reference_results.clear();
            return;
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(_) => {
                self.conversation_reference_results.clear();
                return;
            }
        };
        let references = if self.transient {
            application.search_conversation_references_from(&composer.task_id, &query, 8)
        } else {
            application.search_conversation_references(&composer.task_id, &query, 8)
        };
        match references {
            Ok(references) => self.conversation_reference_results = references,
            Err(error) => {
                eprintln!("failed to search Native conversation references: {error}");
                self.conversation_reference_results.clear();
            }
        }
    }

    fn refresh_context_attachments(&mut self, cx: &UiModuleContext<'_>) {
        let Some(composer) = self.composer.as_ref() else {
            self.context_attachment_results.clear();
            return;
        };
        let Some(query) = crate::desktop::composer_context_query(&composer.content) else {
            self.context_attachment_results.clear();
            return;
        };
        let application = match cx.application() {
            Ok(application) => application,
            Err(_) => {
                self.context_attachment_results.clear();
                return;
            }
        };
        let attachments = if self.transient {
            let Some(project_id) = self.transient_project.as_ref() else {
                self.context_attachment_results.clear();
                return;
            };
            application.search_project_context_attachments(project_id, &query, 8)
        } else {
            match application.get_task(&composer.task_id).ok().and_then(|task| task.project_id) {
                Some(_) => application.search_task_context_attachments(&composer.task_id, &query, 8),
                None => {
                    self.context_attachment_results.clear();
                    return;
                }
            }
        };
        match attachments {
            Ok(attachments) => self.context_attachment_results = attachments,
            Err(error) => {
                eprintln!("failed to search Native project context: {error}");
                self.context_attachment_results.clear();
            }
        }
    }
}

fn attachment_command(command: &DesktopComposerCommand) -> bool {
    matches!(
        command,
        DesktopComposerCommand::ReplaceAttachments(_)
            | DesktopComposerCommand::RemoveAttachment(_)
            | DesktopComposerCommand::ApplyContextAttachment { .. }
            | DesktopComposerCommand::ApplyConversationReference { .. }
            | DesktopComposerCommand::RemoveConversationReference(_)
    )
}

fn permission_label(permission: crate::application::DesktopExecutionPermission) -> &'static str {
    use crate::application::DesktopExecutionPermission;
    match permission {
        DesktopExecutionPermission::Ask => "询问",
        DesktopExecutionPermission::Readonly => "只读",
        DesktopExecutionPermission::Full => "完全",
    }
}

pub(crate) fn textarea_height(state: &TextEditorState) -> f32 {
    let additional_lines = state.line_count().saturating_sub(1) as f32;
    (TEXTAREA_MIN_HEIGHT + additional_lines * TEXTAREA_LINE_HEIGHT).min(TEXTAREA_MAX_HEIGHT)
}

impl UiModule for ComposerModule {
    type Message = ComposerMessage;

    fn feature(&self) -> FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            ComposerMessage::Refresh => self.refresh(cx),
            ComposerMessage::LoadTransient {
                composer,
                project_id,
            } => self.load_transient(composer, project_id),
            ComposerMessage::Clear => {
                self.clear_state();
                UiModuleOutcome::dirty()
            }
            ComposerMessage::SetContent(value) => self.set_content(value, cx),
            ComposerMessage::Edited(action) => {
                self.composer_editor.perform(action);
                self.set_content(self.composer_editor.text(), cx)
            }
            ComposerMessage::ApplyCommand(command) => self.apply_command(command, cx, false),
            ComposerMessage::SelectConversationReference(task_id) => {
                self.select_conversation_reference(&task_id, cx)
            }
            ComposerMessage::SelectContextAttachment(relative_path) => {
                self.select_context_attachment(&relative_path, cx)
            }
            ComposerMessage::RefreshSuggestions => {
                self.refresh_suggestions(cx);
                UiModuleOutcome::dirty()
            }
            ComposerMessage::ClearSuggestions => {
                self.clear_suggestions();
                UiModuleOutcome::dirty()
            }
        }
    }

    fn invalidate(
        &mut self,
        envelope: &lilia_kernel::EventEnvelope,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        let Some(event) = envelope.downcast::<crate::application::ComposerChanged>() else {
            return UiModuleOutcome::clean();
        };
        if cx.selected_task().as_ref() != Some(&event.task_id) {
            return UiModuleOutcome::clean();
        }
        self.refresh(cx)
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        if !crate::module::conversation_is_visible(cx) {
            return;
        }
        into.pending_blocks_send = cx
            .task_session()
            .is_some_and(|session| session.blocking_pending_count > 0);
        if let Some(error) = &self.error {
            into.error = Some(error.clone());
        }
        let Some(composer) = self.composer.as_ref() else {
            into.composer = self.composer_editor.text();
            into.composer_height = textarea_height(&self.composer_editor);
            into.attachments.clear();
            into.plan_mode = false;
            into.goal_mode = false;
            into.slash_items.clear();
            into.mention_items.clear();
            return;
        };
        into.composer = composer.content.clone();
        into.composer_height = textarea_height(&self.composer_editor);
        into.attachments = composer
            .attachments
            .iter()
            .map(|attachment| ShellAttachmentRow {
                id: attachment.id.clone(),
                label: attachment.name.clone(),
            })
            .collect();
        into.plan_mode = composer.plan_mode;
        into.goal_mode = composer.goal_mode;
        into.permission_label = permission_label(composer.permission).to_owned();
        into.slash_items = self
            .slash_commands
            .iter()
            .map(|item| ShellSlashItem {
                name: item.command.name.clone(),
                label: if item.command.description.trim().is_empty() {
                    item.command.title.clone()
                } else {
                    item.command.description.clone()
                },
            })
            .collect();
        into.mention_items = self
            .context_attachment_results
            .iter()
            .map(|result| ShellMentionItem {
                id: result.relative_path.clone(),
                label: result.attachment.name.clone(),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use lilia_contracts::TaskId;
    use lilia_kernel::Kernel;
    use nana_ui_platform::WindowId;

    use super::*;
    use crate::application::ApplicationWorkspaceSurface;
    use crate::runtime_shell::{empty_snapshot, ShellProjectPage};

    fn loaded_draft() -> ComposerModule {
        let mut module = ComposerModule::default();
        let mut composer =
            DesktopComposerState::transient(TaskId::new("draft-task").expect("the id is not blank"));
        composer.content = "hello".to_owned();
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel, WindowId::PRIMARY);
        module.reduce(
            ComposerMessage::LoadTransient {
                composer,
                project_id: None,
            },
            &cx,
        );
        module
    }

    #[test]
    fn a_transient_draft_projects_its_own_content() {
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel, WindowId::PRIMARY);
        let module = loaded_draft();

        let mut snapshot = empty_snapshot();
        module.project(&cx, &mut snapshot);
        assert_eq!(snapshot.composer, "hello");
        assert!(module.is_transient());
    }

    #[test]
    fn clearing_the_input_surface_drops_the_projected_draft() {
        let kernel = Kernel::new();
        let cx = UiModuleContext::new(&kernel, WindowId::PRIMARY);
        let mut module = loaded_draft();
        module.reduce(ComposerMessage::Clear, &cx);

        let mut snapshot = empty_snapshot();
        module.project(&cx, &mut snapshot);
        assert_eq!(snapshot.composer, "");
        assert!(module.composer().is_none());
    }

    #[test]
    fn hidden_surfaces_and_pages_do_not_project_the_draft() {
        let kernel = Kernel::new();
        let module = loaded_draft();
        let mut snapshot = empty_snapshot();
        snapshot.composer = "stale".to_owned();

        module.project(
            &UiModuleContext::new(&kernel, WindowId::PRIMARY)
                .showing_surface(Some(ApplicationWorkspaceSurface::Settings)),
            &mut snapshot,
        );
        assert_eq!(snapshot.composer, "stale");

        snapshot.composer = "stale".to_owned();
        module.project(
            &UiModuleContext::new(&kernel, WindowId::PRIMARY)
                .showing(Some(ShellProjectPage::Architecture)),
            &mut snapshot,
        );
        assert_eq!(snapshot.composer, "stale");
    }
}
