//! The document editor domain as a UI module.
//!
//! Owns the editor view states, the open-document pointer the files page reads
//! back, and the two in-flight job maps for diagnostics and definitions. The
//! authoritative document content lives behind the application service, so a
//! view state here is a buffer mirror that is resynced from snapshots, never a
//! second authority.
//!
//! Definition navigation itself stays with the shell: deciding which pane or
//! window a definition lands in crosses into layout, so the module resolves
//! the target and hands it over as a [`ShellEffect`].

use std::collections::BTreeMap;

use lilia_contracts::ProjectId;
use lilia_kernel::{JobEvent, JobId, JobRequest, JobState, Jobs};
use serde::{Deserialize, Serialize};

use crate::application::{
    DesktopApplication, DesktopDocumentDefinitionResult, DesktopDocumentDiagnosticsSnapshot,
    DiagnosticSeverity, DocumentId, DocumentSnapshot, WorkspaceItem, WorkspaceItemResolve,
    WorkspaceItemId,
};
use crate::document_editor::{
    document_editor_cursor_offset, select_document_editor_offsets, DocumentEditorViewState,
};
use crate::runtime_compat::HostedWindowId;
use crate::runtime_shell::{ShellDiagnosticRow, ShellDocumentSnapshot};
use crate::ui_module::{ShellEffect, UiModule, UiModuleContext, UiModuleOutcome};

/// The document domain's own message vocabulary.
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
    /// A diagnostics or definition job reached a terminal state. The shell
    /// forwards kernel job events for the document protocols here; the module's
    /// own job maps decide whether the event is ours.
    Job(JobEvent),
}

/// What a definition job wrote into its completion payload. The project is part
/// of the lookup, so the project travels back with the targets rather than
/// being re-derived on the shell thread.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocumentDefinitionOutcome {
    pub project_id: Option<ProjectId>,
    pub result: DesktopDocumentDefinitionResult,
}

pub struct DocumentsModule {
    editors: BTreeMap<WorkspaceItemId, DocumentEditorViewState>,
    opened_document: Option<DocumentSnapshot>,
    active_diagnostics_jobs: BTreeMap<JobId, DocumentId>,
    active_definition_jobs: BTreeMap<JobId, (WorkspaceItemId, HostedWindowId)>,
}

impl Default for DocumentsModule {
    fn default() -> Self {
        Self {
            editors: BTreeMap::new(),
            opened_document: None,
            active_diagnostics_jobs: BTreeMap::new(),
            active_definition_jobs: BTreeMap::new(),
        }
    }
}

impl DocumentsModule {
    pub fn feature_id() -> lilia_kernel::FeatureId {
        lilia_kernel::FeatureId::new("lilia.documents")
            .expect("the documents feature id is not blank")
    }

    /// The document the files page currently points at, which its selection
    /// highlight and preview read back.
    pub fn opened_document(&self) -> Option<&DocumentSnapshot> {
        self.opened_document.as_ref()
    }

    /// Moves the editor's selection onto a definition target. Returns false
    /// when the offsets no longer match the buffer, which the shell reports as
    /// a stale definition rather than a silent no-op.
    pub fn apply_definition_selection(
        &mut self,
        item_id: &WorkspaceItemId,
        start_offset: usize,
        end_offset: usize,
    ) -> bool {
        let selected = self
            .editors
            .get(item_id)
            .is_some_and(|state| select_document_editor_offsets(state, start_offset, end_offset));
        if !selected {
            if let Some(state) = self.editors.get_mut(item_id) {
                state.definition_error = Some("定义位置已失效，请重新查找。".to_owned());
            }
        }
        selected
    }

    pub fn set_opened_document(&mut self, document: Option<DocumentSnapshot>) {
        self.opened_document = document;
    }

    pub fn has_editor(&self, item_id: &WorkspaceItemId) -> bool {
        self.editors.contains_key(item_id)
    }

    pub fn editor(&self, item_id: &WorkspaceItemId) -> Option<&DocumentEditorViewState> {
        self.editors.get(item_id)
    }

    /// The document a workspace item's editor renders, so closing the item can
    /// decide whether its document still has another view left.
    pub fn editor_document_id(&self, item_id: &WorkspaceItemId) -> Option<DocumentId> {
        self.editors.get(item_id).map(|state| state.document_id)
    }

    pub fn ensure_editor(&mut self, item: &WorkspaceItem, snapshot: &DocumentSnapshot) {
        if let Some(existing) = self.editors.get_mut(&item.id) {
            existing.sync_from_snapshot(snapshot);
            return;
        }
        self.editors.insert(
            item.id.clone(),
            DocumentEditorViewState::from_snapshot(snapshot),
        );
    }

    /// Drops the editor states of the closed items. Documents themselves are
    /// released by the shell, which knows what other views remain.
    pub fn remove_editors(&mut self, closing: &[WorkspaceItem]) {
        for item in closing {
            self.editors.remove(&item.id);
        }
    }

    /// The first closed item with unsaved changes, for the close confirmation.
    pub fn first_dirty_title(&self, items: &[WorkspaceItem]) -> Option<String> {
        items.iter().find_map(|item| {
            self.editors
                .get(&item.id)
                .filter(|state| state.dirty)
                .map(|_| item.title.clone())
        })
    }

    /// Rebuilds the editor set from the workspace items that render documents,
    /// keeping existing states (and their buffers) when an item survives.
    pub fn sync_editors_from_items(
        &mut self,
        items: &[WorkspaceItem],
        application: &DesktopApplication,
    ) {
        let mut next = BTreeMap::new();
        for item in items {
            let Some(path) = item.document_path().ok().flatten() else {
                continue;
            };
            let snapshot = match self
                .editors
                .get(&item.id)
                .and_then(|state| application.document_snapshot(state.document_id).ok())
            {
                Some(snapshot) => snapshot,
                None => match application.open_document_at_path(&path) {
                    Ok((snapshot, _)) => snapshot,
                    Err(error) => {
                        eprintln!("failed to sync Native document editor: {error}");
                        continue;
                    }
                },
            };
            let state = if let Some(mut existing) = self.editors.remove(&item.id) {
                existing.sync_from_snapshot(&snapshot);
                existing
            } else {
                DocumentEditorViewState::from_snapshot(&snapshot)
            };
            next.insert(item.id.clone(), state);
        }
        self.editors = next;
    }

    /// Marks every editor of the document as checking, then submits the
    /// diagnostics job on its single-flight lane.
    pub fn start_diagnostics(
        &mut self,
        document_id: DocumentId,
        project_id: Option<ProjectId>,
        jobs: &Jobs,
    ) {
        for state in self
            .editors
            .values_mut()
            .filter(|state| state.document_id == document_id)
        {
            state.mark_diagnostics_checking();
        }
        let request = JobRequest::new(
            lilia_feature_document::DIAGNOSTICS_PROTOCOL,
            serde_json::to_value(lilia_feature_document::DiagnosticsRequest {
                document_id,
                project_id: project_id.map(|project_id| project_id.to_string()),
            })
            .expect("a diagnostics request is representable as JSON"),
        )
        .in_slot(lilia_feature_document::diagnostics_slot(document_id));

        match jobs.submit(request) {
            Ok(handle) => {
                self.active_diagnostics_jobs
                    .insert(handle.id(), document_id);
            }
            Err(error) => {
                eprintln!("failed to submit the LiliaCode document diagnostics job: {error}");
                for state in self
                    .editors
                    .values_mut()
                    .filter(|state| state.document_id == document_id)
                {
                    state.mark_diagnostics_unavailable();
                }
            }
        }
    }

    /// Looks up the cursor offset and submits the definition job, unless one
    /// for this editor is already in flight.
    pub fn start_definition(
        &mut self,
        item_id: WorkspaceItemId,
        window_id: HostedWindowId,
        jobs: &Jobs,
    ) {
        let Some(state) = self.editors.get(&item_id) else {
            return;
        };
        if state.definition_job.is_some() {
            return;
        }
        let document_id = state.document_id;
        let revision = state.revision;
        let Some(source_offset) = document_editor_cursor_offset(state) else {
            if let Some(state) = self.editors.get_mut(&item_id) {
                state.definition_error = Some("当前光标位置不可用。".to_owned());
            }
            return;
        };
        if let Some(state) = self.editors.get_mut(&item_id) {
            state.definition_project_id = None;
            state.definition_targets.clear();
            state.definition_message = Some("正在查找定义…".to_owned());
            state.definition_error = None;
        }
        let request = JobRequest::new(
            lilia_feature_document::DEFINITION_PROTOCOL,
            serde_json::to_value(lilia_feature_document::DefinitionRequest {
                document_id,
                revision,
                source_offset,
            })
            .expect("a definition request is representable as JSON"),
        )
        .in_slot(lilia_feature_document::definition_slot(item_id.as_str()));

        match jobs.submit(request) {
            Ok(handle) => {
                self.active_definition_jobs
                    .insert(handle.id(), (item_id.clone(), window_id));
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.definition_job = Some(handle.id());
                }
            }
            Err(error) => {
                eprintln!("failed to submit the LiliaCode document definition job: {error}");
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.definition_job = None;
                    state.definition_message = None;
                    state.definition_error = Some("无法启动定义查找，请重试。".to_owned());
                }
            }
        }
    }

    /// Applies the editor action the control emitted, writing the buffer back
    /// through the application under an optimistic revision check.
    fn handle_editor_action(
        &mut self,
        item_id: WorkspaceItemId,
        action: String,
        application: &DesktopApplication,
    ) {
        let Some(state) = self.editors.get_mut(&item_id) else {
            return;
        };
        if state.read_only {
            return;
        }
        let is_edit = !action.is_empty();
        let document_id = state.document_id;
        let expected = state.revision;
        state.editor.perform(action);
        if !is_edit {
            return;
        }
        let text = state.editor.text();
        match application.replace_document_text(document_id, expected, text) {
            Ok(revision) => {
                if revision == expected {
                    return;
                }
                self.sync_views(document_id, application);
                for state in self
                    .editors
                    .values_mut()
                    .filter(|state| state.document_id == document_id)
                {
                    state.note_text_changed();
                }
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.revision = revision;
                    state.conflict_message = None;
                    state.status_message = None;
                }
            }
            Err(error) => {
                if let Ok(snapshot) = application.document_snapshot(document_id) {
                    if let Some(state) = self.editors.get_mut(&item_id) {
                        state.sync_from_snapshot(&snapshot);
                        state.conflict_message =
                            Some(format!("编辑冲突，已恢复当前缓冲区：{error}"));
                    }
                } else if let Some(state) = self.editors.get_mut(&item_id) {
                    state.conflict_message = Some(format!("无法写入文档：{error}"));
                }
            }
        }
    }

    fn save_editor(
        &mut self,
        item_id: WorkspaceItemId,
        application: &DesktopApplication,
        jobs: &Jobs,
    ) {
        let Some(state) = self.editors.get(&item_id) else {
            return;
        };
        let document_id = state.document_id;
        let expected = state.revision;
        match application.save_document(document_id, expected) {
            Ok(snapshot) => {
                self.sync_views(document_id, application);
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.conflict_message = None;
                    state.status_message = Some("已保存".to_owned());
                }
                self.opened_document = Some(snapshot);
                self.start_diagnostics(document_id, None, jobs);
            }
            Err(error) => {
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.conflict_message = Some(format!("保存失败：{error}"));
                    state.status_message = None;
                }
            }
        }
    }

    fn discard_editor(
        &mut self,
        item_id: WorkspaceItemId,
        application: &DesktopApplication,
        jobs: &Jobs,
    ) {
        let Some(state) = self.editors.get(&item_id) else {
            return;
        };
        let document_id = state.document_id;
        match application.discard_document_changes(document_id) {
            Ok(snapshot) => {
                self.sync_views(document_id, application);
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.conflict_message = None;
                    state.status_message = Some("已丢弃未保存更改".to_owned());
                }
                self.opened_document = Some(snapshot);
                self.start_diagnostics(document_id, None, jobs);
            }
            Err(error) => {
                if let Some(state) = self.editors.get_mut(&item_id) {
                    state.conflict_message = Some(format!("无法丢弃更改：{error}"));
                }
            }
        }
    }

    /// Hands the picked definition target to the shell. Which pane or window it
    /// opens in is layout, not domain.
    fn open_definition_target(
        &mut self,
        item_id: WorkspaceItemId,
        window_id: HostedWindowId,
        index: usize,
    ) -> UiModuleOutcome {
        let Some((project_id, target)) = self.editors.get(&item_id).and_then(|state| {
            state
                .definition_project_id
                .clone()
                .zip(state.definition_targets.get(index).cloned())
        }) else {
            return UiModuleOutcome::clean();
        };
        UiModuleOutcome::effect(ShellEffect::OpenDocumentDefinition {
            source_window: window_id,
            project_id,
            target,
        })
    }

    fn apply_diagnostics_job(
        &mut self,
        job_id: JobId,
        state: JobState,
        application: &DesktopApplication,
    ) {
        let result = match state {
            JobState::Pending | JobState::Running { .. } => return,
            JobState::Completed { output } => {
                serde_json::from_value::<DesktopDocumentDiagnosticsSnapshot>(output)
                    .map_err(|error| error.to_string())
            }
            JobState::Failed { message } => Err(message),
            // Superseded by a newer check of the same document, which will
            // deliver the answer this one would have.
            _ => {
                self.active_diagnostics_jobs.remove(&job_id);
                return;
            }
        };
        let Some(document_id) = self.active_diagnostics_jobs.remove(&job_id) else {
            return;
        };
        self.finish_diagnostics(document_id, result, application);
    }

    fn finish_diagnostics(
        &mut self,
        document_id: DocumentId,
        result: Result<DesktopDocumentDiagnosticsSnapshot, String>,
        application: &DesktopApplication,
    ) {
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("failed to refresh Native document diagnostics: {error}");
                match application.document_diagnostics(document_id) {
                    Ok(snapshot) => snapshot,
                    Err(snapshot_error) => {
                        eprintln!(
                            "failed to read Native document diagnostics state: {snapshot_error}"
                        );
                        for state in self
                            .editors
                            .values_mut()
                            .filter(|state| state.document_id == document_id)
                        {
                            state.mark_diagnostics_unavailable();
                        }
                        return;
                    }
                }
            }
        };
        for state in self
            .editors
            .values_mut()
            .filter(|state| state.document_id == document_id)
        {
            state.sync_diagnostics(&snapshot);
        }
    }

    fn apply_definition_job(&mut self, event: &JobEvent) -> UiModuleOutcome {
        let job_id = event.job_id;
        let result = match &event.state {
            JobState::Pending | JobState::Running { .. } => return UiModuleOutcome::clean(),
            JobState::Completed { output } => {
                serde_json::from_value::<DocumentDefinitionOutcome>(output.clone())
                    .map_err(|error| error.to_string())
            }
            JobState::Failed { message } => Err(message.clone()),
            _ => {
                self.clear_definition_job(job_id);
                return UiModuleOutcome::clean();
            }
        };
        let Some((item_id, window_id)) = self.active_definition_jobs.remove(&job_id) else {
            return UiModuleOutcome::clean();
        };
        let (project_id, result) = match result {
            Ok(outcome) => (outcome.project_id, Ok(outcome.result)),
            Err(error) => (None, Err(error)),
        };
        self.finish_definitions(item_id, window_id, job_id, project_id, result)
    }

    fn finish_definitions(
        &mut self,
        item_id: WorkspaceItemId,
        window_id: HostedWindowId,
        job_id: JobId,
        project_id: Option<ProjectId>,
        result: Result<DesktopDocumentDefinitionResult, String>,
    ) -> UiModuleOutcome {
        let mut immediate_target = None;
        let Some(state) = self.editors.get_mut(&item_id) else {
            return UiModuleOutcome::clean();
        };
        if state.definition_job != Some(job_id) {
            return UiModuleOutcome::clean();
        }
        state.definition_job = None;
        state.definition_message = None;
        match result {
            Ok(result)
                if result.source_document_id == state.document_id
                    && result.source_revision == state.revision =>
            {
                state.definition_project_id = project_id.clone();
                match result.targets.as_slice() {
                    [] => {
                        state.definition_targets.clear();
                        state.definition_message = Some("未找到定义。".to_owned());
                    }
                    [target] => {
                        state.definition_targets.clear();
                        immediate_target =
                            project_id.map(|project_id| (project_id, target.clone()));
                    }
                    targets => {
                        state.definition_targets = targets.to_vec();
                        state.definition_message =
                            Some(format!("找到 {} 个定义，请选择。", targets.len()));
                    }
                }
                state.definition_error = None;
            }
            Ok(_) => {
                state.definition_targets.clear();
                state.definition_error = Some("文档已变化，请重新查找定义。".to_owned());
            }
            Err(error) => {
                eprintln!("failed to resolve Native document definition: {error}");
                state.definition_targets.clear();
                state.definition_error = Some("暂时无法查找定义，请稍后重试。".to_owned());
            }
        }
        match immediate_target {
            Some((project_id, target)) => {
                UiModuleOutcome::effect(ShellEffect::OpenDocumentDefinition {
                    source_window: window_id,
                    project_id,
                    target,
                })
            }
            None => UiModuleOutcome::dirty(),
        }
    }

    /// Drops a definition job the surface will never hear about again.
    fn clear_definition_job(&mut self, job_id: JobId) {
        let Some((item_id, _)) = self.active_definition_jobs.remove(&job_id) else {
            return;
        };
        if let Some(state) = self.editors.get_mut(&item_id) {
            if state.definition_job == Some(job_id) {
                state.definition_job = None;
                state.definition_message = None;
            }
        }
    }

    fn sync_views(&mut self, document_id: DocumentId, application: &DesktopApplication) {
        let Ok(snapshot) = application.document_snapshot(document_id) else {
            return;
        };
        for state in self
            .editors
            .values_mut()
            .filter(|state| state.document_id == document_id)
        {
            state.sync_from_snapshot(&snapshot);
        }
    }
}

impl UiModule for DocumentsModule {
    type Message = DocumentMessage;

    fn feature(&self) -> lilia_kernel::FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            DocumentMessage::EditorEdited { item_id, action } => {
                let application = match cx.application() {
                    Ok(application) => application,
                    Err(error) => return UiModuleOutcome::failed(error),
                };
                self.handle_editor_action(item_id, action, &application);
                UiModuleOutcome::dirty()
            }
            DocumentMessage::GoToDefinition { item_id, window_id } => {
                self.start_definition(item_id, window_id, cx.kernel().jobs());
                UiModuleOutcome::dirty()
            }
            DocumentMessage::OpenDefinitionTarget {
                item_id,
                window_id,
                index,
            } => self.open_definition_target(item_id, window_id, index),
            DocumentMessage::SaveEditor(item_id) => {
                let application = match cx.application() {
                    Ok(application) => application,
                    Err(error) => return UiModuleOutcome::failed(error),
                };
                self.save_editor(item_id, &application, cx.kernel().jobs());
                UiModuleOutcome::dirty()
            }
            DocumentMessage::DiscardEditor(item_id) => {
                let application = match cx.application() {
                    Ok(application) => application,
                    Err(error) => return UiModuleOutcome::failed(error),
                };
                self.discard_editor(item_id, &application, cx.kernel().jobs());
                UiModuleOutcome::dirty()
            }
            DocumentMessage::Job(event) => {
                let application = match cx.application() {
                    Ok(application) => application,
                    Err(error) => return UiModuleOutcome::failed(error),
                };
                match event.protocol.as_str() {
                    lilia_feature_document::DIAGNOSTICS_PROTOCOL => {
                        self.apply_diagnostics_job(event.job_id, event.state, &application);
                        UiModuleOutcome::dirty()
                    }
                    lilia_feature_document::DEFINITION_PROTOCOL => self.apply_definition_job(&event),
                    _ => UiModuleOutcome::clean(),
                }
            }
        }
    }

    fn project(
        &self,
        cx: &UiModuleContext<'_>,
        into: &mut crate::runtime_shell::PrimaryShellSnapshot,
    ) {
        // The document pane is where the active workspace item renders an
        // editor; anything else leaves the field empty for the shell.
        let Some(active_item) = cx
            .workspace()
            .and_then(|session| session.snapshot().ok())
            .and_then(|snapshot| {
                snapshot
                    .panel_layout
                    .active_workspace_item()
                    .ok()
                    .flatten()
                    .cloned()
            })
        else {
            return;
        };
        let Some(state) = self.editors.get(&active_item) else {
            return;
        };
        let item_id = &active_item;
        into.document = Some(ShellDocumentSnapshot {
            item_id: item_id.as_str().to_owned(),
            title: state.path_label.clone(),
            text: state.editor.text(),
            language: state.language_label.clone(),
            status: state
                .conflict_message
                .clone()
                .or_else(|| state.status_message.clone())
                .unwrap_or_else(|| {
                    if state.dirty {
                        "未保存".to_owned()
                    } else {
                        state.language_label.clone()
                    }
                }),
            read_only: state.read_only,
            dirty: state.dirty,
            diagnostics: state
                .diagnostics
                .iter()
                .map(|diagnostic| ShellDiagnosticRow {
                    severity: match diagnostic.severity {
                        DiagnosticSeverity::Error => "错误".to_owned(),
                        DiagnosticSeverity::Warning => "警告".to_owned(),
                        DiagnosticSeverity::Information => "信息".to_owned(),
                        DiagnosticSeverity::Hint => "提示".to_owned(),
                    },
                    message: diagnostic.message.clone(),
                })
                .collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::application::{
        BufferId, BufferRevision, BufferSnapshot, WorkspaceFocusTarget, WorkspaceItemCapabilities,
        WorkspaceItemKind, WorkspaceResourceId,
    };

    fn document_snapshot(text: &str) -> DocumentSnapshot {
        DocumentSnapshot {
            id: DocumentId::new(1),
            canonical_path: PathBuf::from("/tmp/notes.md"),
            language: None,
            read_only: false,
            buffer: BufferSnapshot {
                id: BufferId::new(1),
                text: text.to_owned(),
                revision: BufferRevision::INITIAL,
                saved_revision: BufferRevision::INITIAL,
            },
            disk_fingerprint: 0,
        }
    }

    fn item_id(value: &str) -> WorkspaceItemId {
        WorkspaceItemId::new(value).expect("the test item id is valid")
    }

    /// One editor whose state comes from a real snapshot, keyed by `value`.
    fn editor_with(value: &str, text: &str) -> (WorkspaceItemId, DocumentEditorViewState) {
        (
            item_id(value),
            DocumentEditorViewState::from_snapshot(&document_snapshot(text)),
        )
    }

    /// Fills the workspace-sessions slot, which the projection resolves.
    struct SessionsFeature;

    impl lilia_kernel::Feature for SessionsFeature {
        fn id(&self) -> lilia_kernel::FeatureId {
            lilia_kernel::FeatureId::new("test.sessions").expect("the test feature id is valid")
        }

        fn mount(
            &self,
            cx: &mut lilia_kernel::FeatureContext<'_>,
        ) -> Result<(), lilia_kernel::KernelError> {
            cx.provide::<crate::shell_service::WorkspaceSessionsKey>(std::sync::Arc::new(
                crate::shell_service::WorkspaceSessions::new(),
            ))?;
            Ok(())
        }
    }

    #[test]
    fn without_a_window_session_the_projection_stays_empty() {
        let kernel = lilia_kernel::Kernel::new();
        kernel
            .mount_all(vec![std::sync::Arc::new(SessionsFeature)
                as std::sync::Arc<dyn lilia_kernel::Feature>])
            .expect("the sessions feature mounts");
        let mut module = DocumentsModule::default();
        let (id, state) = editor_with("item-1", "hello");
        module.editors.insert(id, state);

        let cx = UiModuleContext::new(&kernel, nana_ui_platform::WindowId::PRIMARY);
        let mut snapshot = crate::runtime_shell::empty_snapshot();
        module.project(&cx, &mut snapshot);
        assert!(
            snapshot.document.is_none(),
            "no session means no active pane, so nothing to project"
        );
    }

    #[test]
    fn stale_definition_offsets_report_a_stale_target() {
        let mut module = DocumentsModule::default();
        let (id, state) = editor_with("item-1", "hello world");
        module.editors.insert(id.clone(), state);

        assert!(module.apply_definition_selection(&id, 0, 5));
        assert!(module.editors[&id].definition_error.is_none());

        assert!(!module.apply_definition_selection(&id, 100, 110));
        assert_eq!(
            module.editors[&id].definition_error.as_deref(),
            Some("定义位置已失效，请重新查找。")
        );
    }

    #[test]
    fn closing_items_drops_their_editors_and_dirty_titles() {
        let mut module = DocumentsModule::default();
        let (dirty, mut state) = editor_with("item-1", "a");
        state.dirty = true;
        let (clean, clean_state) = editor_with("item-2", "b");
        module.editors.insert(dirty.clone(), state);
        module.editors.insert(clean, clean_state);

        assert_eq!(
            module.first_dirty_title(&[]),
            None,
            "no closed item means nothing to confirm"
        );
        let items = vec![workspace_item("item-1", "a.md")];
        assert_eq!(module.first_dirty_title(&items), Some("a.md".to_owned()));

        module.remove_editors(&items);
        assert!(!module.has_editor(&dirty));
    }

    fn workspace_item(value: &str, title: &str) -> WorkspaceItem {
        WorkspaceItem::new(
            item_id(value),
            WorkspaceResourceId::new(format!("document:{value}"))
                .expect("the resource id is valid"),
            WorkspaceItemKind::new(crate::application::DOCUMENT_WORKSPACE_ITEM_KIND)
                .expect("the kind is valid"),
            title,
            WorkspaceFocusTarget::new("editor").expect("the focus target is valid"),
            WorkspaceItemCapabilities::dockable(),
        )
        .expect("the test item is valid")
    }

    #[test]
    fn a_job_event_without_the_application_reports_a_failure() {
        let kernel = lilia_kernel::Kernel::new();
        let mut module = DocumentsModule::default();
        let (id, state) = editor_with("item-1", "text");
        module.editors.insert(id, state);

        let cx = UiModuleContext::new(&kernel, nana_ui_platform::WindowId::PRIMARY);
        let outcome = module.reduce(
            DocumentMessage::Job(JobEvent {
                job_id: JobId::new(7),
                protocol: lilia_feature_document::DIAGNOSTICS_PROTOCOL.to_owned(),
                slot: None,
                state: JobState::Completed {
                    output: serde_json::json!({}),
                },
            }),
            &cx,
        );
        assert!(
            outcome.error.is_some(),
            "the shell owes the module an application"
        );
    }
}
