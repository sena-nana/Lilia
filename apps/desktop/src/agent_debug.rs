use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENABLE_ENV: &str = "LILIA_AGENT_DEBUG";
const ADDRESS_ENV: &str = "LILIA_AGENT_DEBUG_ADDR";
const READY_ENV: &str = "LILIA_AGENT_DEBUG_READY";
const DEFAULT_ADDRESS: &str = "127.0.0.1:0";
const UI_RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ERRORS: usize = 80;
const MAX_LOGS: usize = 240;

#[derive(Debug, Clone, PartialEq)]
pub enum DebugCommand {
    Observe,
    EquivalenceSnapshot { fixture_id: String },
    Click { target_id: String },
    Input { target_id: String, text: String },
    InputFrame { target_id: String, text: String },
    ResizePanelFrame { extent: f32 },
    Mark { label: String, data: Option<String> },
    CorruptQueuedTurn { turn_id: String },
    SeedInterruptedTool { task_id: String, turn_id: String },
    HoldDatabaseWriter { duration_ms: u64 },
    RecentErrors,
}

impl DebugCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::EquivalenceSnapshot { .. } => "equivalence-snapshot",
            Self::Click { .. } => "click",
            Self::Input { .. } => "input",
            Self::InputFrame { .. } => "input-frame",
            Self::ResizePanelFrame { .. } => "resize-panel-frame",
            Self::Mark { .. } => "mark",
            Self::CorruptQueuedTurn { .. } => "corrupt-queued-turn",
            Self::SeedInterruptedTool { .. } => "seed-interrupted-tool",
            Self::HoldDatabaseWriter { .. } => "hold-database-writer",
            Self::RecentErrors => "recent-errors",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugErrorEntry {
    pub id: String,
    pub kind: &'static str,
    pub source: String,
    pub message: String,
    pub stack: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLogEntry {
    pub id: String,
    pub action_id: Option<String>,
    pub kind: &'static str,
    pub level: &'static str,
    pub message: String,
    pub data: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Default)]
pub struct DebugState {
    sequence: u64,
    active_errors: BTreeSet<String>,
    errors: VecDeque<DebugErrorEntry>,
    logs: VecDeque<DebugLogEntry>,
}

impl DebugState {
    pub fn mark(&mut self, label: String, data: Option<String>) {
        self.sequence = self.sequence.saturating_add(1);
        self.logs.push_back(DebugLogEntry {
            id: format!("log:{}", self.sequence),
            action_id: None,
            kind: "mark",
            level: "info",
            message: label,
            data,
            created_at: now_millis(),
        });
        trim_front(&mut self.logs, MAX_LOGS);
    }

    pub fn capture_errors(&mut self, entries: impl IntoIterator<Item = (String, String)>) {
        let current = entries
            .into_iter()
            .filter_map(|(source, message)| {
                let source = source.trim();
                let message = message.trim();
                (!source.is_empty() && !message.is_empty())
                    .then(|| (source.to_owned(), message.to_owned()))
            })
            .collect::<Vec<_>>();
        let active = current
            .iter()
            .map(|(source, message)| error_key(source, message))
            .collect::<BTreeSet<_>>();
        for (source, message) in current {
            let key = error_key(&source, &message);
            if self.active_errors.contains(&key) {
                continue;
            }
            self.sequence = self.sequence.saturating_add(1);
            self.errors.push_back(DebugErrorEntry {
                id: format!("error:{}", self.sequence),
                kind: "error",
                source,
                message,
                stack: None,
                created_at: now_millis(),
            });
        }
        self.active_errors = active;
        trim_front(&mut self.errors, MAX_ERRORS);
    }

    pub fn errors(&self) -> Vec<DebugErrorEntry> {
        self.errors.iter().cloned().collect()
    }

    pub fn logs(&self) -> Vec<DebugLogEntry> {
        self.logs.iter().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct DebugRequest {
    pub command: DebugCommand,
    pub reply: mpsc::Sender<String>,
}

pub struct DebugObservation {
    pub page: String,
    pub workspace_session_id: String,
    pub workspace_revision: u64,
    pub workspace_persisted_revision: u64,
    pub workspace_windows_revision: u64,
    pub workspace_windows_persisted_revision: u64,
    pub workspace_items: Vec<DebugWorkspaceItem>,
    pub workspace_item_ids: Vec<String>,
    pub active_workspace_item_ids: Vec<String>,
    pub workspace_panes: Vec<DebugWorkspacePane>,
    pub workspace_splits: Vec<DebugWorkspaceSplit>,
    pub active_workspace_pane_id: String,
    pub conversation_status_window_open: bool,
    pub conversation_status_window_ready: bool,
    pub conversation_status_task_count: usize,
    pub conversation_status_active_count: usize,
    pub task_popup_window_count: usize,
    pub task_popup_ready_count: usize,
    pub task_popup_session_ids: Vec<String>,
    pub task_popup_task_ids: Vec<String>,
    pub task_popup_workspace_item_ids: Vec<String>,
    pub task_popup_workspace_resource_ids: Vec<String>,
    pub workspace_window_item_ids: Vec<Vec<String>>,
    pub workspace_window_active_item_ids: Vec<String>,
    pub workspace_windows: Vec<DebugWorkspaceWindow>,
    pub task_popup_workspace_revisions: Vec<u64>,
    pub task_popup_geometries: Vec<Option<DebugWindowGeometry>>,
    pub task_popup_composer_lengths: Vec<usize>,
    pub task_popup_timeline_counts: Vec<usize>,
    pub task_popup_timeline_has_more: Vec<bool>,
    pub timeline_event_count: usize,
    pub timeline_has_more_before: bool,
    pub markdown_table_count: usize,
    pub markdown_copy_target_count: usize,
    pub last_copied_markdown_event_id: Option<String>,
    pub last_copied_markdown_bytes: Option<usize>,
    pub selected_project: Option<String>,
    pub pending_project_removal: Option<String>,
    pub pending_project_archive: Option<String>,
    pub project_order: Vec<String>,
    pub inbox_selected: bool,
    pub selected_task: Option<String>,
    pub selected_task_parent: Option<String>,
    pub task_order: Vec<String>,
    pub project_count: usize,
    pub archived_project_count: usize,
    pub task_count: usize,
    pub visible_task_count: usize,
    pub archived_task_count: usize,
    pub selected_project_name: Option<String>,
    pub selected_project_workspace: Option<String>,
    pub selected_project_pinned: Option<bool>,
    pub project_clone_busy: bool,
    pub project_clone_outcome: &'static str,
    pub project_clone_phase: Option<String>,
    pub project_clone_percent: Option<u8>,
    pub project_clone_target: Option<String>,
    pub github_binding_state: String,
    pub github_binding_login: Option<String>,
    pub github_binding_busy: bool,
    pub github_device_flow_active: bool,
    pub github_repository_busy: bool,
    pub github_repository_count: usize,
    pub github_repository_names: Vec<String>,
    pub selected_github_repository: Option<String>,
    pub github_error: Option<String>,
    pub selected_task_title: Option<String>,
    pub selected_task_status: Option<&'static str>,
    pub selected_task_priority: Option<&'static str>,
    pub selected_task_pinned: Option<bool>,
    pub selected_automation: Option<String>,
    pub automation_count: usize,
    pub automation_published: bool,
    pub automation_enabled: bool,
    pub automation_node_count: usize,
    pub automation_edge_count: usize,
    pub automation_run_count: usize,
    pub selected_automation_run: Option<String>,
    pub automation_run_status: Option<&'static str>,
    pub automation_waiting_human_node: Option<String>,
    pub automation_waiting_agent_node: Option<String>,
    pub automation_selected_node_title: Option<String>,
    pub automation_selected_node_kind: Option<String>,
    pub automation_scope: Option<serde_json::Value>,
    pub automation_selected_node_config: Option<serde_json::Value>,
    pub automation_selected_node_config_draft: Option<serde_json::Value>,
    pub selected_milestone: Option<String>,
    pub selected_milestone_title: Option<String>,
    pub selected_milestone_description: Option<String>,
    pub selected_milestone_due_date: Option<String>,
    pub selected_milestone_status: Option<&'static str>,
    pub selected_milestone_task_count: usize,
    pub milestone_count: usize,
    pub roadmap_error: Option<String>,
    pub selected_memory: Option<String>,
    pub selected_memory_title: Option<String>,
    pub selected_memory_body_line_count: Option<usize>,
    pub memory_draft_body_line_count: usize,
    pub memory_count: usize,
    pub memory_enabled: Option<bool>,
    pub memory_scope: Option<&'static str>,
    pub memory_global_enabled: bool,
    pub memory_baseline_enabled: bool,
    pub memory_cooldown_turns: u64,
    pub task_memory_enabled: Option<bool>,
    pub sidebar_region_extent: Option<f32>,
    pub sidebar_region_collapsed: bool,
    pub inspector_region_extent: Option<f32>,
    pub coding_tools_dock_open: bool,
    pub coding_tools_panel_extent: Option<f32>,
    pub iab_dock_open: bool,
    pub iab_browser_attached: bool,
    pub iab_browser_ready: bool,
    pub iab_url: String,
    pub iab_error: Option<String>,
    pub iab_window_count: usize,
    pub iab_window_ready_count: usize,
    pub iab_window_task_ids: Vec<String>,
    pub iab_window_urls: Vec<String>,
    pub iab_window_capture_pending_count: usize,
    pub iab_window_notice: Option<String>,
    pub iab_window_error: Option<String>,
    pub coding_tools_busy: bool,
    pub coding_tools_shared_identity: bool,
    pub coding_tools_mcp_servers: usize,
    pub coding_tools_lsp_workspaces: usize,
    pub coding_tools_has_git: bool,
    pub coding_tools_has_workspace: bool,
    pub coding_tools_has_search: bool,
    pub architecture_version: i64,
    pub architecture_node_count: usize,
    pub architecture_edge_count: usize,
    pub architecture_history_count: usize,
    pub architecture_quarantine_count: usize,
    pub architecture_selected_node: Option<String>,
    pub visible_target_ids: Vec<String>,
    pub errors: Vec<DebugErrorEntry>,
    pub logs: Vec<DebugLogEntry>,
    pub error: Option<String>,
    pub turn_state: Option<String>,
    pub active_turn_id: Option<String>,
    pub active_turn_durable_state: Option<String>,
    pub active_turn_claim_attempts: Option<u64>,
    pub active_turn_owned_by_current_epoch: Option<bool>,
    pub durable_turn_count: usize,
    pub quarantined_turn_count: usize,
    pub quarantined_turn_ids: Vec<String>,
    pub quarantine_reason_codes: Vec<String>,
    pub queue_depth: usize,
    pub queued_turn_ids: Vec<String>,
    pub pending_interaction_ids: Vec<String>,
    pub pending_interaction_kinds: Vec<String>,
    pub task_action_error: Option<String>,
    pub theme: &'static str,
    pub sidebar_display_mode: &'static str,
    pub settings_tab: String,
    pub provider_id: Option<String>,
    pub provider_ids: Vec<String>,
    pub provider_credential_count: usize,
    pub provider_active_credential_count: usize,
    pub provider_profile_has_credential_refs: bool,
    pub provider_live_model_adapter: bool,
    pub provider_runtime_revision: u64,
    pub provider_runtime_model: Option<String>,
    pub provider_openai_endpoint: Option<String>,
    pub provider_anthropic_endpoint: Option<String>,
    pub provider_runtime_dirty: bool,
    pub provider_busy: bool,
    pub provider_error: Option<String>,
    pub agent_interaction_revision: u64,
    pub agent_non_interrupt_mode: bool,
    pub agent_debug_enabled: bool,
    pub agent_subagents_enabled: bool,
    pub agent_auto_turn_enabled: bool,
    pub agent_auto_model_tier: bool,
    pub agent_auto_reasoning_effort: bool,
    pub agent_auto_plan_mode: bool,
    pub agent_auto_goal_mode: bool,
    pub agent_auto_session_fork: bool,
    pub custom_agent_count: usize,
    pub custom_agent_enabled_count: usize,
    pub custom_agent_editor_open: bool,
    pub editing_custom_agent_id: Option<String>,
    pub custom_agent_name_draft: String,
    pub custom_agent_description_draft: String,
    pub custom_agent_instruction_length: usize,
    pub agent_interaction_error: Option<String>,
    pub quota_days: i64,
    pub quota_backend: String,
    pub quota_total_tokens: i64,
    pub quota_record_count: i64,
    pub quota_known_cost: bool,
    pub quota_busy: bool,
    pub quota_error: Option<String>,
    pub extensions_busy: bool,
    pub extensions_shared_identity: bool,
    pub extensions_runtime_service_count: usize,
    pub extensions_skill_count: usize,
    pub extensions_skills_registry_revision: u64,
    pub extensions_editable_skill_count: usize,
    pub extensions_enabled_skill_count: usize,
    pub extensions_runtime_skill_count: usize,
    pub extensions_skill_delete_confirmation: Option<String>,
    pub extensions_plugin_count: usize,
    pub extensions_plugins_registry_revision: u64,
    pub extensions_enabled_plugin_count: usize,
    pub extensions_runtime_plugin_count: usize,
    pub extensions_plugin_source_input: String,
    pub extensions_plugin_delete_confirmation: Option<String>,
    pub extensions_hook_source_count: usize,
    pub extensions_existing_hook_source_count: usize,
    pub extensions_enabled_hook_source_count: usize,
    pub extensions_hook_handler_count: usize,
    pub extensions_hook_revisions: BTreeMap<String, u64>,
    pub extensions_hook_delete_confirmation: Option<String>,
    pub extensions_mcp_count: usize,
    pub extensions_mcp_registry_revision: u64,
    pub extensions_editable_mcp_count: usize,
    pub extensions_enabled_mcp_count: usize,
    pub extensions_active_mcp_count: usize,
    pub extensions_mcp_tool_count: usize,
    pub extensions_mcp_resource_count: usize,
    pub extensions_mcp_prompt_count: usize,
    pub extensions_mcp_content_kind: Option<String>,
    pub extensions_mcp_content_title: Option<String>,
    pub extensions_mcp_content_text: Option<String>,
    pub extensions_mcp_credential_count: usize,
    pub extensions_mcp_configured_credential_count: usize,
    pub extensions_activation_error_count: usize,
    pub extensions_mcp_editor_open: bool,
    pub extensions_editing_mcp_id: Option<String>,
    pub extensions_mcp_editor_transport: Option<String>,
    pub extensions_mcp_delete_confirmation: Option<String>,
    pub extensions_error: Option<String>,
    pub remote_busy: bool,
    pub remote_host_enabled: bool,
    pub remote_pc_name: Option<String>,
    pub remote_state: Option<String>,
    pub remote_pairing_active: bool,
    pub remote_trusted_device_count: usize,
    pub remote_keep_awake_enabled: bool,
    pub remote_error: Option<String>,
    pub tray_active: bool,
    pub shell_shortcut: Option<String>,
    pub shell_shortcut_active: bool,
    pub shell_shortcut_capturing: bool,
    pub shell_error: Option<String>,
    pub update_configured: bool,
    pub update_state: &'static str,
    pub update_busy: bool,
    pub update_error: Option<String>,
    pub data_import_busy: bool,
    pub data_import_has_source: bool,
    pub data_import_plan_status: Option<&'static str>,
    pub data_import_report_status: Option<&'static str>,
    pub data_import_credentials_confirmed: bool,
    pub data_import_restart_required: bool,
    pub data_import_error: Option<String>,
    pub composer_revision: u64,
    pub composer_length: usize,
    pub composer_attachment_count: usize,
    pub composer_conversation_reference_count: usize,
    pub context_usage_used_tokens: Option<u64>,
    pub context_usage_limit_tokens: Option<u64>,
    pub context_usage_used_percent: Option<f64>,
    pub composer_plan_mode: bool,
    pub composer_goal_mode: bool,
    pub composer_permission: &'static str,
    pub goal_objective: Option<String>,
    pub goal_status: Option<&'static str>,
    pub worktree_path: Option<String>,
    pub worktree_branch: Option<String>,
    pub worktree_busy: bool,
    pub worktree_confirmation: Option<&'static str>,
    pub todo_count: usize,
    pub editable_todo_count: usize,
    pub completed_todo_count: usize,
    pub pending_guide_count: usize,
    pub queued_guide_count: usize,
    pub sent_guide_count: usize,
    pub todo_titles: Vec<String>,
    pub todo_priorities: Vec<String>,
    pub todo_editing: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugWorkspaceItem {
    pub id: String,
    pub resource_id: String,
    pub kind: String,
    pub title: String,
    pub focus_target: String,
    pub closable: bool,
    pub splittable: bool,
    pub movable_across_windows: bool,
    pub persistent: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugWorkspacePane {
    pub id: String,
    pub item_ids: Vec<String>,
    pub active_item_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugWorkspaceSplit {
    pub first_pane_id: String,
    pub second_pane_id: String,
    pub axis: String,
    pub ratio: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugWorkspaceWindow {
    pub window_id: u64,
    pub session_id: String,
    pub revision: u64,
    pub item_ids: Vec<String>,
    pub active_pane_id: String,
    pub active_item_id: Option<String>,
    pub panes: Vec<DebugWorkspacePane>,
    pub splits: Vec<DebugWorkspaceSplit>,
    pub geometry: Option<DebugWindowGeometry>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugWindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl DebugObservation {
    fn to_json(&self) -> String {
        let mut observation = serde_json::json!({
            "page": &self.page,
            "selectedProject": self.selected_project.as_deref(),
            "pendingProjectRemoval": self.pending_project_removal.as_deref(),
            "inboxSelected": self.inbox_selected,
            "selectedTask": self.selected_task.as_deref(),
            "selectedTaskParent": self.selected_task_parent.as_deref(),
            "projectCount": self.project_count,
            "archivedProjectCount": self.archived_project_count,
            "taskCount": self.task_count,
            "visibleTaskCount": self.visible_task_count,
            "archivedTaskCount": self.archived_task_count,
            "selectedProjectName": self.selected_project_name.as_deref(),
            "selectedProjectWorkspace": self.selected_project_workspace.as_deref(),
            "selectedProjectPinned": self.selected_project_pinned,
            "selectedTaskTitle": self.selected_task_title.as_deref(),
            "selectedTaskStatus": self.selected_task_status,
            "selectedTaskPriority": self.selected_task_priority,
            "selectedTaskPinned": self.selected_task_pinned,
            "selectedAutomation": self.selected_automation.as_deref(),
            "automationCount": self.automation_count,
            "automationPublished": self.automation_published,
            "automationEnabled": self.automation_enabled,
            "automationNodeCount": self.automation_node_count,
            "automationEdgeCount": self.automation_edge_count,
            "automationRunCount": self.automation_run_count,
            "selectedAutomationRun": self.selected_automation_run.as_deref(),
            "automationRunStatus": self.automation_run_status,
            "automationWaitingHumanNode": self.automation_waiting_human_node.as_deref(),
            "automationWaitingAgentNode": self.automation_waiting_agent_node.as_deref(),
            "automationSelectedNodeTitle": self.automation_selected_node_title.as_deref(),
            "selectedMilestone": self.selected_milestone.as_deref(),
            "selectedMilestoneTitle": self.selected_milestone_title.as_deref(),
            "milestoneCount": self.milestone_count,
            "selectedMemory": self.selected_memory.as_deref(),
            "selectedMemoryTitle": self.selected_memory_title.as_deref(),
            "memoryCount": self.memory_count,
            "memoryEnabled": self.memory_enabled,
            "memoryScope": self.memory_scope,
            "memoryGlobalEnabled": self.memory_global_enabled,
            "memoryBaselineEnabled": self.memory_baseline_enabled,
            "memoryCooldownTurns": self.memory_cooldown_turns,
            "taskMemoryEnabled": self.task_memory_enabled,
            "codingToolsBusy": self.coding_tools_busy,
            "codingToolsSharedIdentity": self.coding_tools_shared_identity,
            "codingToolsMcpServers": self.coding_tools_mcp_servers,
            "codingToolsLspWorkspaces": self.coding_tools_lsp_workspaces,
            "codingToolsHasGit": self.coding_tools_has_git,
            "codingToolsHasWorkspace": self.coding_tools_has_workspace,
            "codingToolsHasSearch": self.coding_tools_has_search,
            "architectureVersion": self.architecture_version,
            "architectureNodeCount": self.architecture_node_count,
            "architectureEdgeCount": self.architecture_edge_count,
            "architectureHistoryCount": self.architecture_history_count,
            "architectureSelectedNode": self.architecture_selected_node.as_deref(),
            "visibleTargetIds": &self.visible_target_ids,
            "errors": &self.errors,
            "logs": &self.logs,
            "error": self.error.as_deref(),
            "turnState": self.turn_state.as_deref(),
            "taskActionError": self.task_action_error.as_deref(),
            "theme": self.theme,
            "settingsTab": &self.settings_tab,
            "providerId": self.provider_id.as_deref(),
            "providerCredentialCount": self.provider_credential_count,
            "providerActiveCredentialCount": self.provider_active_credential_count,
            "providerProfileHasCredentialRefs": self.provider_profile_has_credential_refs,
            "providerLiveModelAdapter": self.provider_live_model_adapter,
            "providerBusy": self.provider_busy,
            "providerError": self.provider_error.as_deref(),
            "composerPlanMode": self.composer_plan_mode,
            "composerGoalMode": self.composer_goal_mode,
            "composerPermission": self.composer_permission,
            "goalObjective": self.goal_objective.as_deref(),
            "goalStatus": self.goal_status,
            "worktreePath": self.worktree_path.as_deref(),
            "worktreeBranch": self.worktree_branch.as_deref(),
            "worktreeBusy": self.worktree_busy,
            "worktreeConfirmation": self.worktree_confirmation,
            "todoCount": self.todo_count,
            "editableTodoCount": self.editable_todo_count,
            "completedTodoCount": self.completed_todo_count,
            "todoTitles": &self.todo_titles,
            "todoPriorities": &self.todo_priorities,
            "todoEditing": self.todo_editing,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert(
                "pendingProjectArchive".to_owned(),
                serde_json::json!(self.pending_project_archive.as_deref()),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert(
                "sidebarDisplayMode".to_owned(),
                serde_json::json!(self.sidebar_display_mode),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert(
                "providerIds".to_owned(),
                serde_json::json!(&self.provider_ids),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert(
                "architectureQuarantineCount".to_owned(),
                serde_json::json!(self.architecture_quarantine_count),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                serde_json::json!({
                    "selectedMemoryBodyLineCount": self.selected_memory_body_line_count,
                    "memoryDraftBodyLineCount": self.memory_draft_body_line_count,
                })
                .as_object()
                .expect("Memory body observation is an object")
                .clone(),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert(
                "projectOrder".to_owned(),
                serde_json::json!(&self.project_order),
            );
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .insert("taskOrder".to_owned(), serde_json::json!(&self.task_order));
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                serde_json::json!({
                    "pendingInteractionIds": &self.pending_interaction_ids,
                    "pendingInteractionKinds": &self.pending_interaction_kinds,
                })
                .as_object()
                .expect("pending interaction observation is an object")
                .clone(),
            );
        let provider_runtime = serde_json::json!({
            "providerRuntimeRevision": self.provider_runtime_revision,
            "providerRuntimeModel": self.provider_runtime_model.as_deref(),
            "providerOpenAiEndpoint": self.provider_openai_endpoint.as_deref(),
            "providerAnthropicEndpoint": self.provider_anthropic_endpoint.as_deref(),
            "providerRuntimeDirty": self.provider_runtime_dirty,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                provider_runtime
                    .as_object()
                    .expect("provider runtime observation is an object")
                    .clone(),
            );
        let agent_interaction = serde_json::json!({
            "agentInteractionRevision": self.agent_interaction_revision,
            "agentNonInterruptMode": self.agent_non_interrupt_mode,
            "agentDebugEnabled": self.agent_debug_enabled,
            "agentSubagentsEnabled": self.agent_subagents_enabled,
            "agentAutoTurnEnabled": self.agent_auto_turn_enabled,
            "agentAutoModelTier": self.agent_auto_model_tier,
            "agentAutoReasoningEffort": self.agent_auto_reasoning_effort,
            "agentAutoPlanMode": self.agent_auto_plan_mode,
            "agentAutoGoalMode": self.agent_auto_goal_mode,
            "agentAutoSessionFork": self.agent_auto_session_fork,
            "customAgentCount": self.custom_agent_count,
            "customAgentEnabledCount": self.custom_agent_enabled_count,
            "customAgentEditorOpen": self.custom_agent_editor_open,
            "editingCustomAgentId": self.editing_custom_agent_id.as_deref(),
            "customAgentNameDraft": &self.custom_agent_name_draft,
            "customAgentDescriptionDraft": &self.custom_agent_description_draft,
            "customAgentInstructionLength": self.custom_agent_instruction_length,
            "agentInteractionError": self.agent_interaction_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                agent_interaction
                    .as_object()
                    .expect("Agent interaction observation is an object")
                    .clone(),
            );
        let coding_dock = serde_json::json!({
            "sidebarRegionExtent": self.sidebar_region_extent,
            "sidebarRegionCollapsed": self.sidebar_region_collapsed,
            "inspectorRegionExtent": self.inspector_region_extent,
            "codingToolsDockOpen": self.coding_tools_dock_open,
            "codingToolsPanelExtent": self.coding_tools_panel_extent,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                coding_dock
                    .as_object()
                    .expect("coding Dock observation is an object")
                    .clone(),
            );
        let iab = serde_json::json!({
            "iabDockOpen": self.iab_dock_open,
            "iabBrowserAttached": self.iab_browser_attached,
            "iabBrowserReady": self.iab_browser_ready,
            "iabUrl": &self.iab_url,
            "iabError": self.iab_error.as_deref(),
            "iabWindowCount": self.iab_window_count,
            "iabWindowReadyCount": self.iab_window_ready_count,
            "iabWindowTaskIds": &self.iab_window_task_ids,
            "iabWindowUrls": &self.iab_window_urls,
            "iabWindowCapturePendingCount": self.iab_window_capture_pending_count,
            "iabWindowNotice": self.iab_window_notice.as_deref(),
            "iabWindowError": self.iab_window_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                iab.as_object()
                    .expect("IAB observation is an object")
                    .clone(),
            );
        let markdown = serde_json::json!({
            "markdownTableCount": self.markdown_table_count,
            "markdownCopyTargetCount": self.markdown_copy_target_count,
            "lastCopiedMarkdownEventId": self.last_copied_markdown_event_id.as_deref(),
            "lastCopiedMarkdownBytes": self.last_copied_markdown_bytes,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                markdown
                    .as_object()
                    .expect("markdown observation is an object")
                    .clone(),
            );
        let automation_inspector = serde_json::json!({
            "automationSelectedNodeKind": self.automation_selected_node_kind.as_deref(),
            "automationScope": self.automation_scope.as_ref(),
            "automationSelectedNodeConfig": self.automation_selected_node_config.as_ref(),
            "automationSelectedNodeConfigDraft": self.automation_selected_node_config_draft.as_ref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                automation_inspector
                    .as_object()
                    .expect("automation inspector observation is an object")
                    .clone(),
            );
        let project_clone = serde_json::json!({
            "projectCloneBusy": self.project_clone_busy,
            "projectCloneOutcome": self.project_clone_outcome,
            "projectClonePhase": self.project_clone_phase.as_deref(),
            "projectClonePercent": self.project_clone_percent,
            "projectCloneTarget": self.project_clone_target.as_deref(),
            "githubBindingState": &self.github_binding_state,
            "githubBindingLogin": self.github_binding_login.as_deref(),
            "githubBindingBusy": self.github_binding_busy,
            "githubDeviceFlowActive": self.github_device_flow_active,
            "githubRepositoryBusy": self.github_repository_busy,
            "githubRepositoryCount": self.github_repository_count,
            "githubRepositoryNames": &self.github_repository_names,
            "selectedGitHubRepository": self.selected_github_repository.as_deref(),
            "githubError": self.github_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                project_clone
                    .as_object()
                    .expect("project clone observation is an object")
                    .clone(),
            );
        let roadmap = serde_json::json!({
            "selectedMilestoneDescription": self.selected_milestone_description.as_deref(),
            "selectedMilestoneDueDate": self.selected_milestone_due_date.as_deref(),
            "selectedMilestoneStatus": self.selected_milestone_status,
            "selectedMilestoneTaskCount": self.selected_milestone_task_count,
            "roadmapError": self.roadmap_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                roadmap
                    .as_object()
                    .expect("roadmap observation is an object")
                    .clone(),
            );
        let todo_guides = serde_json::json!({
            "pendingGuideCount": self.pending_guide_count,
            "queuedGuideCount": self.queued_guide_count,
            "sentGuideCount": self.sent_guide_count,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                todo_guides
                    .as_object()
                    .expect("todo Guide observation is an object")
                    .clone(),
            );
        let composer_runtime = serde_json::json!({
            "activeTurnId": self.active_turn_id.as_deref(),
            "activeTurnDurableState": self.active_turn_durable_state.as_deref(),
            "activeTurnClaimAttempts": self.active_turn_claim_attempts,
            "activeTurnOwnedByCurrentEpoch": self.active_turn_owned_by_current_epoch,
            "durableTurnCount": self.durable_turn_count,
            "quarantinedTurnCount": self.quarantined_turn_count,
            "quarantinedTurnIds": &self.quarantined_turn_ids,
            "quarantineReasonCodes": &self.quarantine_reason_codes,
            "queueDepth": self.queue_depth,
            "queuedTurnIds": &self.queued_turn_ids,
            "composerRevision": self.composer_revision,
            "composerLength": self.composer_length,
            "composerAttachmentCount": self.composer_attachment_count,
            "composerConversationReferenceCount": self.composer_conversation_reference_count,
            "contextUsageUsedTokens": self.context_usage_used_tokens,
            "contextUsageLimitTokens": self.context_usage_limit_tokens,
            "contextUsageUsedPercent": self.context_usage_used_percent,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                composer_runtime
                    .as_object()
                    .expect("composer runtime observation is an object")
                    .clone(),
            );
        let workspace = serde_json::json!({
            "workspaceSessionId": &self.workspace_session_id,
            "workspaceRevision": self.workspace_revision,
            "workspacePersistedRevision": self.workspace_persisted_revision,
            "workspaceTopologyRevision": self.workspace_windows_revision,
            "workspaceTopologyPersistedRevision": self.workspace_windows_persisted_revision,
            "workspaceWindowsRevision": self.workspace_windows_revision,
            "workspaceWindowsPersistedRevision": self.workspace_windows_persisted_revision,
            "workspaceItems": &self.workspace_items,
            "workspaceItemIds": &self.workspace_item_ids,
            "activeWorkspaceItemIds": &self.active_workspace_item_ids,
            "workspacePanes": &self.workspace_panes,
            "workspaceSplits": &self.workspace_splits,
            "activeWorkspacePaneId": &self.active_workspace_pane_id,
            "workspacePaneCount": self.workspace_panes.len(),
            "conversationStatusWindowOpen": self.conversation_status_window_open,
            "conversationStatusWindowReady": self.conversation_status_window_ready,
            "conversationStatusTaskCount": self.conversation_status_task_count,
            "conversationStatusActiveCount": self.conversation_status_active_count,
            "taskPopupWindowCount": self.task_popup_window_count,
            "taskPopupReadyCount": self.task_popup_ready_count,
            "taskPopupSessionIds": &self.task_popup_session_ids,
            "taskPopupTaskIds": &self.task_popup_task_ids,
            "taskPopupWorkspaceItemIds": &self.task_popup_workspace_item_ids,
            "taskPopupWorkspaceResourceIds": &self.task_popup_workspace_resource_ids,
            "workspaceWindowItemIds": &self.workspace_window_item_ids,
            "workspaceWindowActiveItemIds": &self.workspace_window_active_item_ids,
            "workspaceWindows": &self.workspace_windows,
            "taskPopupWorkspaceRevisions": &self.task_popup_workspace_revisions,
            "taskPopupGeometries": &self.task_popup_geometries,
            "taskPopupComposerLengths": &self.task_popup_composer_lengths,
            "taskPopupTimelineCounts": &self.task_popup_timeline_counts,
            "taskPopupTimelineHasMore": &self.task_popup_timeline_has_more,
            "timelineEventCount": self.timeline_event_count,
            "timelineHasMoreBefore": self.timeline_has_more_before,
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                workspace
                    .as_object()
                    .expect("workspace observation is an object")
                    .clone(),
            );
        let quota = serde_json::json!({
            "quotaDays": self.quota_days,
            "quotaBackend": &self.quota_backend,
            "quotaTotalTokens": self.quota_total_tokens,
            "quotaRecordCount": self.quota_record_count,
            "quotaKnownCost": self.quota_known_cost,
            "quotaBusy": self.quota_busy,
            "quotaError": self.quota_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                quota
                    .as_object()
                    .expect("quota observation is an object")
                    .clone(),
            );
        let extensions = serde_json::json!({
            "extensionsBusy": self.extensions_busy,
            "extensionsSharedIdentity": self.extensions_shared_identity,
            "extensionsRuntimeServiceCount": self.extensions_runtime_service_count,
            "extensionsSkillCount": self.extensions_skill_count,
            "extensionsSkillsRegistryRevision": self.extensions_skills_registry_revision,
            "extensionsEditableSkillCount": self.extensions_editable_skill_count,
            "extensionsEnabledSkillCount": self.extensions_enabled_skill_count,
            "extensionsRuntimeSkillCount": self.extensions_runtime_skill_count,
            "extensionsSkillDeleteConfirmation": self.extensions_skill_delete_confirmation.as_deref(),
            "extensionsPluginCount": self.extensions_plugin_count,
            "extensionsPluginsRegistryRevision": self.extensions_plugins_registry_revision,
            "extensionsEnabledPluginCount": self.extensions_enabled_plugin_count,
            "extensionsRuntimePluginCount": self.extensions_runtime_plugin_count,
            "extensionsPluginSourceInput": &self.extensions_plugin_source_input,
            "extensionsPluginDeleteConfirmation": self.extensions_plugin_delete_confirmation.as_deref(),
            "extensionsHookSourceCount": self.extensions_hook_source_count,
            "extensionsExistingHookSourceCount": self.extensions_existing_hook_source_count,
            "extensionsEnabledHookSourceCount": self.extensions_enabled_hook_source_count,
            "extensionsHookHandlerCount": self.extensions_hook_handler_count,
            "extensionsHookRevisions": &self.extensions_hook_revisions,
            "extensionsHookDeleteConfirmation": self.extensions_hook_delete_confirmation.as_deref(),
            "extensionsMcpCount": self.extensions_mcp_count,
            "extensionsMcpRegistryRevision": self.extensions_mcp_registry_revision,
            "extensionsEditableMcpCount": self.extensions_editable_mcp_count,
            "extensionsEnabledMcpCount": self.extensions_enabled_mcp_count,
            "extensionsActiveMcpCount": self.extensions_active_mcp_count,
            "extensionsMcpToolCount": self.extensions_mcp_tool_count,
            "extensionsMcpResourceCount": self.extensions_mcp_resource_count,
            "extensionsMcpPromptCount": self.extensions_mcp_prompt_count,
            "extensionsMcpContentKind": self.extensions_mcp_content_kind.as_deref(),
            "extensionsMcpContentTitle": self.extensions_mcp_content_title.as_deref(),
            "extensionsMcpContentText": self.extensions_mcp_content_text.as_deref(),
            "extensionsMcpCredentialCount": self.extensions_mcp_credential_count,
            "extensionsMcpConfiguredCredentialCount": self.extensions_mcp_configured_credential_count,
            "extensionsActivationErrorCount": self.extensions_activation_error_count,
            "extensionsMcpEditorOpen": self.extensions_mcp_editor_open,
            "extensionsEditingMcpId": self.extensions_editing_mcp_id.as_deref(),
            "extensionsMcpEditorTransport": self.extensions_mcp_editor_transport.as_deref(),
            "extensionsMcpDeleteConfirmation": self.extensions_mcp_delete_confirmation.as_deref(),
            "extensionsError": self.extensions_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                extensions
                    .as_object()
                    .expect("extensions observation is an object")
                    .clone(),
            );
        let remote = serde_json::json!({
            "remoteBusy": self.remote_busy,
            "remoteHostEnabled": self.remote_host_enabled,
            "remotePcName": self.remote_pc_name.as_deref(),
            "remoteState": self.remote_state.as_deref(),
            "remotePairingActive": self.remote_pairing_active,
            "remoteTrustedDeviceCount": self.remote_trusted_device_count,
            "remoteKeepAwakeEnabled": self.remote_keep_awake_enabled,
            "remoteError": self.remote_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                remote
                    .as_object()
                    .expect("remote observation is an object")
                    .clone(),
            );
        let shell = serde_json::json!({
            "trayActive": self.tray_active,
            "shellShortcut": self.shell_shortcut.as_deref(),
            "shellShortcutActive": self.shell_shortcut_active,
            "shellShortcutCapturing": self.shell_shortcut_capturing,
            "shellError": self.shell_error.as_deref(),
            "updateConfigured": self.update_configured,
            "updateState": self.update_state,
            "updateBusy": self.update_busy,
            "updateError": self.update_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                shell
                    .as_object()
                    .expect("shell observation is an object")
                    .clone(),
            );
        let data_import = serde_json::json!({
            "dataImportBusy": self.data_import_busy,
            "dataImportHasSource": self.data_import_has_source,
            "dataImportPlanStatus": self.data_import_plan_status,
            "dataImportReportStatus": self.data_import_report_status,
            "dataImportCredentialsConfirmed": self.data_import_credentials_confirmed,
            "dataImportRestartRequired": self.data_import_restart_required,
            "dataImportError": self.data_import_error.as_deref(),
        });
        observation
            .as_object_mut()
            .expect("debug observation is an object")
            .extend(
                data_import
                    .as_object()
                    .expect("data import observation is an object")
                    .clone(),
            );
        observation.to_string()
    }
}

pub fn success_response(command: &str, observation: &DebugObservation) -> String {
    format!(
        "{{\"ok\":true,\"command\":{},\"observation\":{}}}",
        json_string(command),
        observation.to_json(),
    )
}

pub fn snapshot_response<T: serde::Serialize>(command: &str, snapshot: &T) -> String {
    serde_json::json!({
        "ok": true,
        "command": command,
        "snapshot": snapshot,
    })
    .to_string()
}

pub fn frame_response(command: &str, observation: &DebugObservation, duration: Duration) -> String {
    serde_json::json!({
        "ok": true,
        "command": command,
        "durationMs": duration.as_secs_f64() * 1_000.0,
        "observation": serde_json::from_str::<serde_json::Value>(&observation.to_json())
            .expect("debug observation JSON is valid"),
    })
    .to_string()
}

pub fn failure_response(
    command: &str,
    code: &str,
    message: &str,
    observation: Option<&DebugObservation>,
) -> String {
    let observation = observation.map_or_else(|| "null".to_owned(), DebugObservation::to_json);
    format!(
        "{{\"ok\":false,\"command\":{},\"error\":{{\"code\":{},\"message\":{}}},\"observation\":{}}}",
        json_string(command),
        json_string(code),
        json_string(message),
        observation,
    )
}

pub fn install(
    dispatch: impl Fn(DebugRequest) -> Result<(), String> + Send + Sync + 'static,
) -> Result<(), String> {
    if !debug_enabled(std::env::var(ENABLE_ENV).ok().as_deref()) {
        return Ok(());
    }

    let address = std::env::var(ADDRESS_ENV).unwrap_or_else(|_| DEFAULT_ADDRESS.to_owned());
    let address: SocketAddr = address
        .parse()
        .map_err(|error| format!("invalid {ADDRESS_ENV}: {error}"))?;
    if !address.ip().is_loopback() {
        return Err(format!("{ADDRESS_ENV} must use a loopback address"));
    }
    let ready_path = std::env::var_os(READY_ENV)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{READY_ENV} must be set when Agent debug is enabled"))?;
    let listener = TcpListener::bind(address)
        .map_err(|error| format!("failed to bind Native Agent debug service: {error}"))?;
    let actual_address = listener
        .local_addr()
        .map_err(|error| format!("failed to read Native Agent debug address: {error}"))?;
    write_ready_atomically(Path::new(&ready_path), &actual_address.to_string())?;

    let dispatch = Arc::new(dispatch);
    std::thread::Builder::new()
        .name("lilia-native-agent-debug".to_owned())
        .spawn(move || serve(listener, dispatch))
        .map_err(|error| format!("failed to start Native Agent debug service: {error}"))?;
    Ok(())
}

fn serve(
    listener: TcpListener,
    dispatch: Arc<dyn Fn(DebugRequest) -> Result<(), String> + Send + Sync>,
) {
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if let Err(error) = serve_connection(stream, &dispatch) {
                    eprintln!("Native Agent debug connection failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("Native Agent debug accept failed: {error}");
                break;
            }
        }
    }
}

fn serve_connection(
    stream: TcpStream,
    dispatch: &Arc<dyn Fn(DebugRequest) -> Result<(), String> + Send + Sync>,
) -> Result<(), String> {
    stream
        .set_nodelay(true)
        .map_err(|error| format!("failed to configure debug socket: {error}"))?;
    let reader_stream = stream
        .try_clone()
        .map_err(|error| format!("failed to clone debug socket: {error}"))?;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read debug request: {error}"))?;
        if read == 0 {
            return Ok(());
        }
        let response = match parse_request(line.trim_end_matches(['\r', '\n'])) {
            Ok(command) => {
                let command_name = command.name();
                let (reply, response) = mpsc::channel();
                let request = DebugRequest { command, reply };
                if let Err(error) = dispatch(request) {
                    failure_response(command_name, "dispatch_failed", &error, None)
                } else {
                    response
                        .recv_timeout(UI_RESPONSE_TIMEOUT)
                        .unwrap_or_else(|_| {
                            failure_response(
                                command_name,
                                "ui_timeout",
                                "the UI did not answer the debug command in time",
                                None,
                            )
                        })
                }
            }
            Err(error) => failure_response("invalid", "invalid_request", &error, None),
        };
        writer
            .write_all(response.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|error| format!("failed to write debug response: {error}"))?;
    }
}

fn parse_request(line: &str) -> Result<DebugCommand, String> {
    let fields = JsonCursor::new(line).parse_object()?;
    let command = fields
        .get("command")
        .ok_or_else(|| "command is required".to_owned())?;
    match command.as_str() {
        "observe" => Ok(DebugCommand::Observe),
        "equivalence-snapshot" => Ok(DebugCommand::EquivalenceSnapshot {
            fixture_id: required_field(&fields, "fixtureId")?.to_owned(),
        }),
        "click" => Ok(DebugCommand::Click {
            target_id: required_field(&fields, "targetId")?.to_owned(),
        }),
        "input" => Ok(DebugCommand::Input {
            target_id: required_field(&fields, "targetId")?.to_owned(),
            text: required_field(&fields, "text")?.to_owned(),
        }),
        "input-frame" => Ok(DebugCommand::InputFrame {
            target_id: required_field(&fields, "targetId")?.to_owned(),
            text: required_field(&fields, "text")?.to_owned(),
        }),
        "resize-panel-frame" => Ok(DebugCommand::ResizePanelFrame {
            extent: required_field(&fields, "extent")?
                .parse::<f32>()
                .map_err(|_| "extent must be a finite number".to_owned())
                .and_then(|extent| {
                    extent
                        .is_finite()
                        .then_some(extent)
                        .ok_or_else(|| "extent must be a finite number".to_owned())
                })?,
        }),
        "mark" => Ok(DebugCommand::Mark {
            label: required_field(&fields, "label")?.to_owned(),
            data: fields.get("data").cloned(),
        }),
        "corrupt-queued-turn" => Ok(DebugCommand::CorruptQueuedTurn {
            turn_id: required_field(&fields, "turnId")?.to_owned(),
        }),
        "seed-interrupted-tool" => Ok(DebugCommand::SeedInterruptedTool {
            task_id: required_field(&fields, "taskId")?.to_owned(),
            turn_id: required_field(&fields, "turnId")?.to_owned(),
        }),
        "hold-database-writer" => Ok(DebugCommand::HoldDatabaseWriter {
            duration_ms: required_field(&fields, "durationMs")?
                .parse::<u64>()
                .map_err(|_| "durationMs must be an unsigned integer".to_owned())?,
        }),
        "recent-errors" => Ok(DebugCommand::RecentErrors),
        other => Err(format!("unsupported command `{other}`")),
    }
}

fn required_field<'a>(fields: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    fields
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} is required"))
}

fn debug_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

fn error_key(source: &str, message: &str) -> String {
    format!("{source}\0{message}")
}

fn trim_front<T>(values: &mut VecDeque<T>, limit: usize) {
    while values.len() > limit {
        values.pop_front();
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn write_ready_atomically(path: &Path, address: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create Agent debug artifact directory: {error}"))?;
    }
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary)
            .map_err(|error| format!("failed to remove stale Agent debug ready file: {error}"))?;
    }
    fs::write(&temporary, address)
        .map_err(|error| format!("failed to write Agent debug ready file: {error}"))?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to replace Agent debug ready file: {error}"))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("failed to publish Agent debug ready file: {error}"))
}

fn json_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\u{08}' => encoded.push_str("\\b"),
            '\u{0c}' => encoded.push_str("\\f"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            value if value.is_control() => {
                encoded.push_str(&format!("\\u{:04x}", u32::from(value)));
            }
            value => encoded.push(value),
        }
    }
    encoded.push('"');
    encoded
}

struct JsonCursor<'a> {
    characters: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> JsonCursor<'a> {
    fn new(value: &'a str) -> Self {
        Self {
            characters: value.chars().peekable(),
        }
    }

    fn parse_object(mut self) -> Result<BTreeMap<String, String>, String> {
        self.whitespace();
        self.expect('{')?;
        let mut fields = BTreeMap::new();
        loop {
            self.whitespace();
            if self.consume('}') {
                break;
            }
            let key = self.string()?;
            self.whitespace();
            self.expect(':')?;
            self.whitespace();
            let value = self.string()?;
            if fields.insert(key.clone(), value).is_some() {
                return Err(format!("duplicate field `{key}`"));
            }
            self.whitespace();
            if self.consume('}') {
                break;
            }
            self.expect(',')?;
        }
        self.whitespace();
        if self.characters.next().is_some() {
            return Err("unexpected content after JSON object".to_owned());
        }
        Ok(fields)
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut value = String::new();
        loop {
            let character = self
                .characters
                .next()
                .ok_or_else(|| "unterminated JSON string".to_owned())?;
            match character {
                '"' => return Ok(value),
                '\\' => {
                    let escaped = self
                        .characters
                        .next()
                        .ok_or_else(|| "unterminated JSON escape".to_owned())?;
                    match escaped {
                        '"' | '\\' | '/' => value.push(escaped),
                        'b' => value.push('\u{08}'),
                        'f' => value.push('\u{0c}'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        'u' => value.push(self.unicode_escape()?),
                        other => return Err(format!("invalid JSON escape `\\{other}`")),
                    }
                }
                value_character if value_character.is_control() => {
                    return Err("JSON strings cannot contain control characters".to_owned());
                }
                value_character => value.push(value_character),
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, String> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let digit = self
                .characters
                .next()
                .and_then(|character| character.to_digit(16))
                .ok_or_else(|| "invalid JSON unicode escape".to_owned())?;
            value = value * 16 + digit;
        }
        char::from_u32(value).ok_or_else(|| "invalid JSON unicode scalar".to_owned())
    }

    fn whitespace(&mut self) {
        while self
            .characters
            .next_if(|character| character.is_whitespace())
            .is_some()
        {}
    }

    fn consume(&mut self, expected: char) -> bool {
        self.characters.next_if_eq(&expected).is_some()
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!("expected `{expected}`"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_jsonl_commands_and_escapes() {
        assert_eq!(
            parse_request(r#"{"command":"observe"}"#).unwrap(),
            DebugCommand::Observe
        );
        assert_eq!(
            parse_request(r#"{"command":"equivalence-snapshot","fixtureId":"equivalence-p0-v1"}"#)
                .unwrap(),
            DebugCommand::EquivalenceSnapshot {
                fixture_id: "equivalence-p0-v1".to_owned()
            }
        );
        assert_eq!(
            parse_request(r#"{"command":"click","targetId":"lilia.settings.open"}"#).unwrap(),
            DebugCommand::Click {
                target_id: "lilia.settings.open".to_owned()
            }
        );
        assert_eq!(
            parse_request(r#"{"command":"input","targetId":"field","text":"中文\ninput"}"#)
                .unwrap(),
            DebugCommand::Input {
                target_id: "field".to_owned(),
                text: "中文\ninput".to_owned()
            }
        );
        assert_eq!(
            parse_request(
                r#"{"command":"input-frame","targetId":"lilia.task-session.composer.input","text":"frame"}"#
            )
            .unwrap(),
            DebugCommand::InputFrame {
                target_id: "lilia.task-session.composer.input".to_owned(),
                text: "frame".to_owned(),
            }
        );
        assert_eq!(
            parse_request(r#"{"command":"resize-panel-frame","extent":"376"}"#).unwrap(),
            DebugCommand::ResizePanelFrame { extent: 376.0 }
        );
        assert_eq!(
            parse_request(r#"{"command":"mark","label":"scenario:start","data":"{\"id\":1}"}"#)
                .unwrap(),
            DebugCommand::Mark {
                label: "scenario:start".to_owned(),
                data: Some(r#"{"id":1}"#.to_owned()),
            }
        );
        assert_eq!(
            parse_request(r#"{"command":"corrupt-queued-turn","turnId":"turn-debug-corrupt"}"#)
                .unwrap(),
            DebugCommand::CorruptQueuedTurn {
                turn_id: "turn-debug-corrupt".to_owned(),
            }
        );
        assert_eq!(
            parse_request(
                r#"{"command":"seed-interrupted-tool","taskId":"task-debug","turnId":"turn-debug-tool"}"#
            )
            .unwrap(),
            DebugCommand::SeedInterruptedTool {
                task_id: "task-debug".to_owned(),
                turn_id: "turn-debug-tool".to_owned(),
            }
        );
        assert_eq!(
            parse_request(r#"{"command":"hold-database-writer","durationMs":"6500"}"#).unwrap(),
            DebugCommand::HoldDatabaseWriter { duration_ms: 6_500 }
        );
        assert_eq!(
            parse_request(r#"{"command":"recent-errors"}"#).unwrap(),
            DebugCommand::RecentErrors
        );
    }

    #[test]
    fn rejects_non_loopback_activation_and_malformed_requests_at_the_boundary() {
        assert!(!debug_enabled(None));
        assert!(!debug_enabled(Some("true")));
        assert!(debug_enabled(Some("1")));
        assert!(parse_request(r#"{"command":"click"}"#).is_err());
        assert!(parse_request(r#"{"command":"resize-panel-frame","extent":"NaN"}"#).is_err());
        assert!(parse_request(r#"{"command":"unknown"}"#).is_err());
        assert!(parse_request(r#"{"command":"observe","command":"click"}"#).is_err());
    }

    #[test]
    fn responses_are_valid_jsonl_without_raw_control_characters() {
        let observation = DebugObservation {
            inbox_selected: false,
            page: "projects".to_owned(),
            workspace_session_id: "lilia.primary".to_owned(),
            workspace_revision: 3,
            workspace_persisted_revision: 3,
            workspace_windows_revision: 2,
            workspace_windows_persisted_revision: 2,
            workspace_items: vec![DebugWorkspaceItem {
                id: "task:one".to_owned(),
                resource_id: "task:one".to_owned(),
                kind: "task".to_owned(),
                title: "One".to_owned(),
                focus_target: "composer".to_owned(),
                closable: true,
                splittable: true,
                movable_across_windows: true,
                persistent: true,
            }],
            workspace_item_ids: vec!["task:one".to_owned()],
            active_workspace_item_ids: vec!["task:one".to_owned()],
            workspace_panes: vec![DebugWorkspacePane {
                id: "primary".to_owned(),
                item_ids: vec!["task:one".to_owned()],
                active_item_id: Some("task:one".to_owned()),
            }],
            workspace_splits: Vec::new(),
            active_workspace_pane_id: "primary".to_owned(),
            conversation_status_window_open: true,
            conversation_status_window_ready: true,
            conversation_status_task_count: 2,
            conversation_status_active_count: 1,
            task_popup_window_count: 1,
            task_popup_ready_count: 1,
            task_popup_session_ids: vec!["lilia.popup.task.one.100".to_owned()],
            task_popup_task_ids: vec!["one".to_owned()],
            task_popup_workspace_item_ids: vec!["task-popup-view:100:one".to_owned()],
            task_popup_workspace_resource_ids: vec!["task:one".to_owned()],
            workspace_window_item_ids: vec![vec!["task-popup-view:100:one".to_owned()]],
            workspace_window_active_item_ids: vec!["task-popup-view:100:one".to_owned()],
            workspace_windows: vec![DebugWorkspaceWindow {
                window_id: 100,
                session_id: "lilia.popup.task.one.100".to_owned(),
                revision: 2,
                item_ids: vec!["task-popup-view:100:one".to_owned()],
                active_pane_id: "primary".to_owned(),
                active_item_id: Some("task-popup-view:100:one".to_owned()),
                panes: vec![DebugWorkspacePane {
                    id: "primary".to_owned(),
                    item_ids: vec!["task-popup-view:100:one".to_owned()],
                    active_item_id: Some("task-popup-view:100:one".to_owned()),
                }],
                splits: Vec::new(),
                geometry: Some(DebugWindowGeometry {
                    x: 24,
                    y: 32,
                    width: 430,
                    height: 760,
                    maximized: false,
                }),
            }],
            task_popup_workspace_revisions: vec![2],
            task_popup_geometries: vec![Some(DebugWindowGeometry {
                x: 24,
                y: 32,
                width: 430,
                height: 760,
                maximized: false,
            })],
            task_popup_composer_lengths: vec![4],
            task_popup_timeline_counts: vec![100],
            task_popup_timeline_has_more: vec![true],
            timeline_event_count: 100,
            timeline_has_more_before: true,
            markdown_table_count: 1,
            markdown_copy_target_count: 2,
            last_copied_markdown_event_id: Some("event-1".to_owned()),
            last_copied_markdown_bytes: Some(42),
            selected_project: Some("project\n1".to_owned()),
            pending_project_removal: Some("project-remove".to_owned()),
            pending_project_archive: None,
            project_order: vec!["project\n1".to_owned()],
            selected_task: None,
            selected_task_parent: None,
            task_order: vec!["task\n1".to_owned()],
            project_count: 1,
            archived_project_count: 0,
            task_count: 2,
            visible_task_count: 1,
            archived_task_count: 1,
            selected_project_name: Some("Native".to_owned()),
            selected_project_workspace: Some("C:\\Native".to_owned()),
            selected_project_pinned: Some(true),
            project_clone_busy: true,
            project_clone_outcome: "running",
            project_clone_phase: Some("cloning".to_owned()),
            project_clone_percent: Some(42),
            project_clone_target: Some("C:\\Native\\clone".to_owned()),
            github_binding_state: "bound".to_owned(),
            github_binding_login: Some("native-debug".to_owned()),
            github_binding_busy: false,
            github_device_flow_active: false,
            github_repository_busy: false,
            github_repository_count: 1,
            github_repository_names: vec!["sena-nana/Lilia".to_owned()],
            selected_github_repository: Some("sena-nana/Lilia".to_owned()),
            github_error: None,
            selected_task_title: None,
            selected_task_status: None,
            selected_task_priority: None,
            selected_task_pinned: None,
            selected_automation: Some("workflow-1".to_owned()),
            automation_count: 1,
            automation_published: true,
            automation_enabled: false,
            automation_node_count: 2,
            automation_edge_count: 1,
            automation_run_count: 3,
            selected_automation_run: Some("run-1".to_owned()),
            automation_run_status: Some("waiting_user"),
            automation_waiting_human_node: Some("human-1".to_owned()),
            automation_waiting_agent_node: None,
            automation_selected_node_title: Some("Trigger".to_owned()),
            automation_selected_node_kind: Some("trigger".to_owned()),
            automation_scope: Some(serde_json::json!({"includeInbox": true})),
            automation_selected_node_config: Some(serde_json::json!({"triggerKind": "manual"})),
            automation_selected_node_config_draft: Some(
                serde_json::json!({"triggerKind": "task_changed"}),
            ),
            selected_milestone: Some("milestone-1".to_owned()),
            selected_milestone_title: Some("M1".to_owned()),
            selected_milestone_description: Some("Native milestone".to_owned()),
            selected_milestone_due_date: Some("2028-02-29".to_owned()),
            selected_milestone_status: Some("in-progress"),
            selected_milestone_task_count: 2,
            milestone_count: 1,
            roadmap_error: Some("invalid date".to_owned()),
            selected_memory: Some("memory-1".to_owned()),
            selected_memory_title: Some("Remember".to_owned()),
            selected_memory_body_line_count: Some(3),
            memory_draft_body_line_count: 3,
            memory_count: 1,
            memory_enabled: Some(true),
            memory_scope: Some("project"),
            memory_global_enabled: true,
            memory_baseline_enabled: true,
            memory_cooldown_turns: 5,
            task_memory_enabled: Some(true),
            sidebar_region_extent: Some(220.0),
            sidebar_region_collapsed: false,
            inspector_region_extent: Some(352.0),
            coding_tools_dock_open: true,
            coding_tools_panel_extent: Some(360.0),
            iab_dock_open: false,
            iab_browser_attached: false,
            iab_browser_ready: false,
            iab_url: "about:blank".to_owned(),
            iab_error: None,
            iab_window_count: 0,
            iab_window_ready_count: 0,
            iab_window_task_ids: Vec::new(),
            iab_window_urls: Vec::new(),
            iab_window_capture_pending_count: 0,
            iab_window_notice: None,
            iab_window_error: None,
            coding_tools_busy: false,
            coding_tools_shared_identity: true,
            coding_tools_mcp_servers: 0,
            coding_tools_lsp_workspaces: 0,
            coding_tools_has_git: true,
            coding_tools_has_workspace: true,
            coding_tools_has_search: false,
            architecture_version: 2,
            architecture_node_count: 3,
            architecture_edge_count: 2,
            architecture_history_count: 2,
            architecture_quarantine_count: 1,
            architecture_selected_node: Some("ui".to_owned()),
            visible_target_ids: vec!["lilia.projects".to_owned()],
            errors: vec![DebugErrorEntry {
                id: "error:1".to_owned(),
                kind: "error",
                source: "provider".to_owned(),
                message: "provider failed".to_owned(),
                stack: None,
                created_at: 1,
            }],
            logs: vec![DebugLogEntry {
                id: "log:2".to_owned(),
                action_id: None,
                kind: "mark",
                level: "info",
                message: "scenario:start".to_owned(),
                data: None,
                created_at: 2,
            }],
            error: None,
            turn_state: None,
            active_turn_id: None,
            active_turn_durable_state: None,
            active_turn_claim_attempts: None,
            active_turn_owned_by_current_epoch: None,
            durable_turn_count: 0,
            quarantined_turn_count: 0,
            quarantined_turn_ids: Vec::new(),
            quarantine_reason_codes: Vec::new(),
            queue_depth: 0,
            queued_turn_ids: Vec::new(),
            pending_interaction_ids: vec!["recovery-1".to_owned()],
            pending_interaction_kinds: vec!["agent_interaction".to_owned()],
            task_action_error: None,
            theme: "dark",
            sidebar_display_mode: "grouped",
            settings_tab: "provider".to_owned(),
            provider_id: Some("mutsuki.credential.openai".to_owned()),
            provider_ids: vec!["mutsuki.credential.openai".to_owned()],
            provider_credential_count: 1,
            provider_active_credential_count: 1,
            provider_profile_has_credential_refs: true,
            provider_live_model_adapter: true,
            provider_runtime_revision: 2,
            provider_runtime_model: Some("gpt-4.1".to_owned()),
            provider_openai_endpoint: Some("https://models.example.test/v1".to_owned()),
            provider_anthropic_endpoint: None,
            provider_runtime_dirty: false,
            provider_busy: false,
            provider_error: None,
            agent_interaction_revision: 3,
            agent_non_interrupt_mode: false,
            agent_debug_enabled: false,
            agent_subagents_enabled: true,
            agent_auto_turn_enabled: true,
            agent_auto_model_tier: true,
            agent_auto_reasoning_effort: true,
            agent_auto_plan_mode: true,
            agent_auto_goal_mode: true,
            agent_auto_session_fork: true,
            custom_agent_count: 1,
            custom_agent_enabled_count: 1,
            custom_agent_editor_open: false,
            editing_custom_agent_id: None,
            custom_agent_name_draft: String::new(),
            custom_agent_description_draft: String::new(),
            custom_agent_instruction_length: 0,
            agent_interaction_error: None,
            quota_days: 30,
            quota_backend: "all".to_owned(),
            quota_total_tokens: 42,
            quota_record_count: 1,
            quota_known_cost: false,
            quota_busy: false,
            quota_error: None,
            extensions_busy: false,
            extensions_shared_identity: true,
            extensions_runtime_service_count: 6,
            extensions_skill_count: 0,
            extensions_skills_registry_revision: 0,
            extensions_editable_skill_count: 0,
            extensions_enabled_skill_count: 0,
            extensions_runtime_skill_count: 0,
            extensions_skill_delete_confirmation: None,
            extensions_plugin_count: 0,
            extensions_plugins_registry_revision: 0,
            extensions_enabled_plugin_count: 0,
            extensions_runtime_plugin_count: 0,
            extensions_plugin_source_input: String::new(),
            extensions_plugin_delete_confirmation: None,
            extensions_hook_source_count: 0,
            extensions_existing_hook_source_count: 0,
            extensions_enabled_hook_source_count: 0,
            extensions_hook_handler_count: 0,
            extensions_hook_revisions: BTreeMap::new(),
            extensions_hook_delete_confirmation: None,
            extensions_mcp_count: 0,
            extensions_mcp_registry_revision: 0,
            extensions_editable_mcp_count: 0,
            extensions_enabled_mcp_count: 0,
            extensions_active_mcp_count: 0,
            extensions_mcp_tool_count: 0,
            extensions_mcp_resource_count: 0,
            extensions_mcp_prompt_count: 0,
            extensions_mcp_content_kind: None,
            extensions_mcp_content_title: None,
            extensions_mcp_content_text: None,
            extensions_mcp_credential_count: 0,
            extensions_mcp_configured_credential_count: 0,
            extensions_activation_error_count: 0,
            extensions_mcp_editor_open: false,
            extensions_editing_mcp_id: None,
            extensions_mcp_editor_transport: None,
            extensions_mcp_delete_confirmation: None,
            extensions_error: None,
            remote_busy: false,
            remote_host_enabled: true,
            remote_pc_name: Some("Native Agent Debug PC".to_owned()),
            remote_state: Some("pairing".to_owned()),
            remote_pairing_active: true,
            remote_trusted_device_count: 1,
            remote_keep_awake_enabled: true,
            remote_error: None,
            tray_active: true,
            shell_shortcut: Some("Ctrl+Shift+L".to_owned()),
            shell_shortcut_active: true,
            shell_shortcut_capturing: false,
            shell_error: None,
            update_configured: true,
            update_state: "up_to_date",
            update_busy: false,
            update_error: None,
            data_import_busy: false,
            data_import_has_source: true,
            data_import_plan_status: Some("ready"),
            data_import_report_status: None,
            data_import_credentials_confirmed: false,
            data_import_restart_required: false,
            data_import_error: None,
            composer_revision: 3,
            composer_length: 12,
            composer_attachment_count: 2,
            composer_conversation_reference_count: 1,
            context_usage_used_tokens: Some(4_096),
            context_usage_limit_tokens: Some(8_192),
            context_usage_used_percent: Some(50.0),
            composer_plan_mode: true,
            composer_goal_mode: false,
            composer_permission: "ask",
            goal_objective: Some("ship Native".to_owned()),
            goal_status: Some("active"),
            worktree_path: Some("C:/repo-native".to_owned()),
            worktree_branch: Some("lilia/native".to_owned()),
            worktree_busy: false,
            worktree_confirmation: None,
            todo_count: 2,
            editable_todo_count: 1,
            completed_todo_count: 1,
            pending_guide_count: 1,
            queued_guide_count: 0,
            sent_guide_count: 0,
            todo_titles: vec!["todo one".to_owned(), "todo two".to_owned()],
            todo_priorities: vec!["high".to_owned(), "normal".to_owned()],
            todo_editing: false,
        };
        let response = success_response("observe", &observation);
        assert!(!response.contains("project\n1"));
        assert!(response.contains(r#"project\n1"#));
        assert!(!response.contains('\r'));
        assert!(!response.contains('\n'));
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            response
                .pointer("/observation/workspacePaneCount")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            response
                .pointer("/observation/activeWorkspacePaneId")
                .and_then(serde_json::Value::as_str),
            Some("primary")
        );
        assert_eq!(
            response
                .pointer("/observation/projectOrder/0")
                .and_then(serde_json::Value::as_str),
            Some("project\n1")
        );
        assert_eq!(
            response
                .pointer("/observation/taskOrder/0")
                .and_then(serde_json::Value::as_str),
            Some("task\n1")
        );
        assert_eq!(
            response
                .pointer("/observation/workspaceWindows/0/windowId")
                .and_then(serde_json::Value::as_u64),
            Some(100)
        );
        assert_eq!(
            response
                .pointer("/observation/workspaceWindows/0/panes/0/activeItemId")
                .and_then(serde_json::Value::as_str),
            Some("task-popup-view:100:one")
        );
        assert_eq!(
            response
                .pointer("/observation/selectedMilestoneDueDate")
                .and_then(serde_json::Value::as_str),
            Some("2028-02-29")
        );
        assert_eq!(
            response
                .pointer("/observation/selectedMilestoneTaskCount")
                .and_then(serde_json::Value::as_u64),
            Some(2)
        );
        assert_eq!(
            response
                .pointer("/observation/roadmapError")
                .and_then(serde_json::Value::as_str),
            Some("invalid date")
        );
    }

    #[test]
    fn debug_state_bounds_marks_and_records_error_reappearance_only() {
        let mut state = DebugState::default();
        for index in 0..(MAX_LOGS + 2) {
            state.mark(format!("mark-{index}"), None);
        }
        assert_eq!(state.logs().len(), MAX_LOGS);
        assert_eq!(state.logs()[0].message, "mark-2");

        state.capture_errors([("provider".to_owned(), "failed".to_owned())]);
        state.capture_errors([("provider".to_owned(), "failed".to_owned())]);
        assert_eq!(state.errors().len(), 1);
        state.capture_errors([]);
        state.capture_errors([("provider".to_owned(), "failed".to_owned())]);
        assert_eq!(state.errors().len(), 2);
    }
}
