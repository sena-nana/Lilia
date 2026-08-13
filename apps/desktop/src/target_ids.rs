pub const APP_ROOT: &str = "lilia.app";
pub const COMMAND_PALETTE_OPEN: &str = "lilia.command-palette.open";
pub const COMMAND_PALETTE_INPUT: &str = "lilia.command-palette.input";
const COMMAND_PALETTE_ACTION_PREFIX: &str = "lilia.command-palette.action.";

pub fn command_palette_action(action_id: &str) -> String {
    format!("{COMMAND_PALETTE_ACTION_PREFIX}{action_id}")
}

pub fn parse_command_palette_action(target_id: &str) -> Option<&str> {
    target_id.strip_prefix(COMMAND_PALETTE_ACTION_PREFIX)
}

pub const CONVERSATION_STATUS_OPEN: &str = "lilia.conversation-status.open";
pub const CONVERSATION_STATUS_WINDOW: &str = "lilia.conversation-status.window";
pub const CONVERSATION_STATUS_CLOSE: &str = "lilia.conversation-status.close";
pub const CONVERSATION_STATUS_PIN: &str = "lilia.conversation-status.pin";
pub const CONVERSATION_STATUS_OPACITY: &str = "lilia.conversation-status.opacity";
pub const CONVERSATION_STATUS_NEW_CHAT: &str = "lilia.conversation-status.new-chat";
pub const NEW_CONVERSATION: &str = "lilia.new-conversation";
pub const NEW_CONVERSATION_CLOSE: &str = "lilia.new-conversation.close";
pub const PROJECTS_LIST: &str = "lilia.projects";
pub const INBOX: &str = "lilia.inbox";
pub const PROJECTS_REFRESH: &str = "lilia.projects.refresh";
pub const PROJECT_CREATE: &str = "lilia.projects.create";
pub const PROJECT_CLONE_OPEN: &str = "lilia.projects.clone";
pub const PROJECT_CLONE_BACK: &str = "lilia.project-clone.back";
pub const PROJECT_CLONE_REPOSITORY: &str = "lilia.project-clone.repository";
pub const PROJECT_CLONE_PARENT: &str = "lilia.project-clone.parent";
pub const PROJECT_CLONE_PICK_PARENT: &str = "lilia.project-clone.pick-parent";
pub const PROJECT_CLONE_START: &str = "lilia.project-clone.start";
pub const PROJECT_CLONE_CANCEL: &str = "lilia.project-clone.cancel";
pub const GITHUB_BIND_START: &str = "lilia.project-clone.github.bind";
pub const GITHUB_BIND_CANCEL: &str = "lilia.project-clone.github.bind.cancel";
pub const GITHUB_VERIFICATION_OPEN: &str = "lilia.project-clone.github.verification.open";
pub const GITHUB_USER_CODE_COPY: &str = "lilia.project-clone.github.user-code.copy";
pub const GITHUB_UNBIND: &str = "lilia.project-clone.github.unbind";
pub const GITHUB_REPOS_REFRESH: &str = "lilia.project-clone.github.repos.refresh";
pub const GITHUB_REPOS_LOAD_MORE: &str = "lilia.project-clone.github.repos.load-more";

pub fn github_repository(full_name: &str) -> String {
    format!("lilia.project-clone.github.repo.{full_name}")
}
pub const PROJECT_NAME: &str = "lilia.project.name";
pub const PROJECT_WORKSPACE: &str = "lilia.project.workspace";
pub const PROJECT_WORKSPACE_PICK: &str = "lilia.project.workspace.pick";
pub const PROJECT_WORKSPACE_CLEAR: &str = "lilia.project.workspace.clear";
pub const PROJECT_SAVE: &str = "lilia.project.save";
pub const PROJECT_PIN: &str = "lilia.project.pin";
pub const PROJECT_MOVE_UP: &str = "lilia.project.move-up";
pub const PROJECT_MOVE_DOWN: &str = "lilia.project.move-down";
pub const PROJECT_ARCHIVE_CONVERSATIONS: &str = "lilia.project.archive-conversations";
pub const PROJECT_ARCHIVE_DIALOG: &str = "lilia.project.archive-conversations.dialog";
pub const PROJECT_ARCHIVE_CONFIRM: &str = "lilia.project.archive-conversations.confirm";
pub const PROJECT_ARCHIVE_CANCEL: &str = "lilia.project.archive-conversations.cancel";
pub const PROJECT_REMOVE: &str = "lilia.project.remove";
pub const PROJECT_REMOVE_DIALOG: &str = "lilia.project.remove.dialog";
pub const PROJECT_REMOVE_CONFIRM: &str = "lilia.project.remove.confirm";
pub const PROJECT_REMOVE_CANCEL: &str = "lilia.project.remove.cancel";
pub const PROJECT_TASKS: &str = "lilia.project.tasks";
pub const ROADMAP_OPEN: &str = "lilia.project.roadmap";
pub const ROADMAP_REFRESH: &str = "lilia.roadmap.refresh";
pub const ROADMAP_CREATE: &str = "lilia.roadmap.create";
pub const ROADMAP_TITLE: &str = "lilia.roadmap.milestone.title";
pub const ROADMAP_DESCRIPTION: &str = "lilia.roadmap.milestone.description";
pub const ROADMAP_DUE_DATE: &str = "lilia.roadmap.milestone.due-date";
pub const ROADMAP_SAVE: &str = "lilia.roadmap.milestone.save";
pub const ROADMAP_STATUS: &str = "lilia.roadmap.milestone.status";
pub const ROADMAP_MOVE_UP: &str = "lilia.roadmap.milestone.move-up";
pub const ROADMAP_MOVE_DOWN: &str = "lilia.roadmap.milestone.move-down";
pub const ROADMAP_DELETE: &str = "lilia.roadmap.milestone.delete";
pub const MEMORY_OPEN: &str = "lilia.project.memory";
pub const MEMORY_REFRESH: &str = "lilia.memory.refresh";
pub const MEMORY_NEW: &str = "lilia.memory.new";
pub const MEMORY_TITLE: &str = "lilia.memory.title";
pub const MEMORY_BODY: &str = "lilia.memory.body";
pub const MEMORY_TAGS: &str = "lilia.memory.tags";
pub const MEMORY_SCOPE: &str = "lilia.memory.scope";
pub const MEMORY_SAVE: &str = "lilia.memory.save";
pub const MEMORY_TOGGLE: &str = "lilia.memory.toggle";
pub const MEMORY_DELETE: &str = "lilia.memory.delete";
pub const MEMORY_SETTINGS_GLOBAL: &str = "lilia.memory.settings.enabled";
pub const MEMORY_SETTINGS_BASELINE: &str = "lilia.memory.settings.baseline";
pub const MEMORY_SETTINGS_COOLDOWN: &str = "lilia.memory.settings.cooldown";
pub const MEMORY_SETTINGS_COOLDOWN_INPUT: &str = "lilia.memory.settings.cooldown.input";
pub const MEMORY_SETTINGS_COOLDOWN_SAVE: &str = "lilia.memory.settings.cooldown.save";
pub const CODING_TOOLS_OPEN: &str = "lilia.project.coding-tools";
pub const CODING_TOOLS_REFRESH: &str = "lilia.coding-tools.refresh";
pub const CODING_TOOLS_QUERY: &str = "lilia.coding-tools.query";
pub const CODING_TOOLS_SEARCH_MODE: &str = "lilia.coding-tools.search-mode";
pub const CODING_TOOLS_SEARCH_SCOPE: &str = "lilia.coding-tools.search-scope";
pub const CODING_TOOLS_SEARCH: &str = "lilia.coding-tools.search";
pub const CODING_TOOLS_CLOSE: &str = "lilia.coding-tools.close";
pub const CODING_TOOLS_OPEN_WORKSPACE: &str = "lilia.coding-tools.open-workspace";
pub const CODING_TOOLS_OPEN_CODE_EDITOR: &str = "lilia.coding-tools.open-code-editor";
pub const CODING_TOOLS_OPEN_TERMINAL: &str = "lilia.coding-tools.open-terminal";
pub const CODING_TOOLS_NEW_TERMINAL: &str = "lilia.coding-tools.new-terminal";
pub const CODING_TOOLS_SAVE_MEMORY: &str = "lilia.coding-tools.save-memory";
pub const CODING_TOOLS_DIFF_SCOPE: &str = "lilia.coding-tools.diff-scope";
pub const CODING_TOOLS_SEARCH_HIT_PREFIX: &str = "lilia.coding-tools.search-hit.";
pub const CODING_TOOLS_TASK_PREFIX: &str = "lilia.coding-tools.task.";
pub const IAB_OPEN: &str = "lilia.task-session.iab.open";
pub const IAB_PANEL: &str = "lilia.task-session.iab";
pub const IAB_URL: &str = "lilia.task-session.iab.url";
pub const IAB_NAVIGATE: &str = "lilia.task-session.iab.navigate";
pub const IAB_OPEN_WINDOW: &str = "lilia.task-session.iab.open-window";
pub const IAB_CLOSE: &str = "lilia.task-session.iab.close";

pub fn iab_window(window_id: u64) -> String {
    format!("lilia.iab-window.{window_id}")
}

pub fn iab_window_url(window_id: u64) -> String {
    format!("{}.url", iab_window(window_id))
}

pub fn iab_window_navigate(window_id: u64) -> String {
    format!("{}.navigate", iab_window(window_id))
}

pub fn iab_window_note(window_id: u64) -> String {
    format!("{}.note", iab_window(window_id))
}

pub fn iab_window_submit(window_id: u64) -> String {
    format!("{}.submit", iab_window(window_id))
}

pub fn iab_window_close(window_id: u64) -> String {
    format!("{}.close", iab_window(window_id))
}

pub fn coding_project_task(task_id: &str) -> String {
    let encoded_id = task_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{CODING_TOOLS_TASK_PREFIX}{encoded_id}")
}

pub fn coding_search_hit(path: &str, line: Option<u32>, character: Option<u32>) -> String {
    let encoded_path = path
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!(
        "{CODING_TOOLS_SEARCH_HIT_PREFIX}{encoded_path}.{}.{}",
        line.map_or_else(|| "none".to_owned(), |value| value.to_string()),
        character.map_or_else(|| "none".to_owned(), |value| value.to_string())
    )
}

pub fn coding_project_search_hit(
    project_id: &str,
    path: &str,
    line: Option<u32>,
    character: Option<u32>,
) -> String {
    coding_search_hit(&format!("{project_id}/{path}"), line, character)
}

pub fn document_editor_input(item_id: &str) -> String {
    let encoded_item = item_id
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("lilia.document-editor.{encoded_item}")
}

pub fn document_editor_definition(item_id: &str) -> String {
    format!("{}.definition", document_editor_input(item_id))
}

pub fn document_editor_definition_target(item_id: &str, index: usize) -> String {
    format!("{}.definition.{index}", document_editor_input(item_id))
}

pub fn terminal_input(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.input")
}

pub fn terminal_submit(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.submit")
}

pub fn terminal_interrupt(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.interrupt")
}

pub fn terminal_eof(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.eof")
}

pub fn terminal_copy(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.copy")
}

pub fn terminal_resize(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.resize")
}

pub fn terminal_terminate(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.terminate")
}

pub fn terminal_new(session_id: &str) -> String {
    format!("lilia.terminal.{session_id}.new")
}

pub fn coding_terminal_session(session_id: &str) -> String {
    format!("lilia.coding-tools.terminal.{session_id}")
}
pub const ARCHITECTURE_OPEN: &str = "lilia.project.architecture";
pub const ARCHITECTURE_REFRESH: &str = "lilia.architecture.refresh";
pub const ARCHITECTURE_ROLLBACK: &str = "lilia.architecture.rollback";
pub const PROJECT_FILES_OPEN: &str = "lilia.project.files";
pub const PROJECT_FILES_REFRESH: &str = "lilia.project-files.refresh";
pub const AUTOMATIONS_OPEN: &str = "lilia.automations.open";
pub const AUTOMATIONS_BACK: &str = "lilia.automations.back";
pub const AUTOMATIONS_REFRESH: &str = "lilia.automations.refresh";
pub const AUTOMATIONS_CREATE: &str = "lilia.automations.create";
pub const AUTOMATIONS_NAME: &str = "lilia.automations.name";
pub const AUTOMATIONS_SAVE_DRAFT: &str = "lilia.automations.save-draft";
pub const AUTOMATIONS_ADD_AGENT: &str = "lilia.automations.add.agent";
pub const AUTOMATIONS_ADD_TOOL: &str = "lilia.automations.add.tool";
pub const AUTOMATIONS_ADD_LOGIC: &str = "lilia.automations.add.logic";
pub const AUTOMATIONS_ADD_HUMAN: &str = "lilia.automations.add.human";
pub const AUTOMATIONS_DELETE_SELECTION: &str = "lilia.automations.selection.delete";
pub const AUTOMATIONS_SCOPE_INCLUDE_INBOX: &str = "lilia.automations.scope.include-inbox";
pub const AUTOMATIONS_NODE_TITLE: &str = "lilia.automations.node.title";
pub const AUTOMATIONS_NODE_CONFIG: &str = "lilia.automations.node.config";
pub const AUTOMATIONS_NODE_SAVE: &str = "lilia.automations.node.save";
pub const AUTOMATIONS_PUBLISH: &str = "lilia.automations.publish";
pub const AUTOMATIONS_TOGGLE: &str = "lilia.automations.toggle";
pub const AUTOMATIONS_DELETE: &str = "lilia.automations.delete";
pub const AUTOMATIONS_RUN: &str = "lilia.automations.run";
pub const AUTOMATIONS_CANCEL: &str = "lilia.automations.run.cancel";
pub const AUTOMATIONS_HUMAN_RESPONSE: &str = "lilia.automations.run.human-response";
pub const AUTOMATIONS_RESUME: &str = "lilia.automations.run.resume";
pub const TASKS_LIST: &str = "lilia.tasks";
pub const WORKSPACE_OVERVIEW_TAB: &str = "lilia.workspace.tab.overview";
pub const TASK_SEARCH: &str = "lilia.tasks.search";
pub const TASK_CREATE_TITLE: &str = "lilia.tasks.create.title";
pub const TASK_CREATE: &str = "lilia.tasks.create";
pub const TASK_SESSION: &str = "lilia.task-session";
pub const TASK_POPUP_OPEN: &str = "lilia.task-session.popup.open";
pub const TASK_POPUP_ASK_CHILD: &str = "lilia.task-session.popup.ask-child";
pub const TASK_POPUP_MOVE_SELECTED: &str = "lilia.task-session.popup.move-selected";
pub const TASK_SESSION_BACK: &str = "lilia.task-session.back";
pub const TASK_SESSION_SUMMARY: &str = "lilia.task-session.summary";
pub const TASK_SESSION_TIMELINE: &str = "lilia.task-session.timeline";
pub const TASK_SESSION_TIMELINE_LOAD_EARLIER: &str = "lilia.task-session.timeline.load-earlier";
pub const TASK_SESSION_INSPECTOR: &str = "lilia.task-session.inspector";
pub const TASK_SESSION_INSPECTOR_TOGGLE: &str = "lilia.task-session.inspector.toggle";
pub const TASK_TITLE: &str = "lilia.task-session.task.title";
pub const TASK_SAVE: &str = "lilia.task-session.task.save";
pub const TASK_DEPENDENCY_TARGET: &str = "lilia.task-session.task.dependency-target";
pub const TASK_DEPENDENCY_TOGGLE: &str = "lilia.task-session.task.dependency-toggle";
pub const TASK_STATUS: &str = "lilia.task-session.task.status";
pub const TASK_PRIORITY: &str = "lilia.task-session.task.priority";
pub const TASK_PIN: &str = "lilia.task-session.task.pin";
pub const TASK_MOVE_UP: &str = "lilia.task-session.task.move-up";
pub const TASK_MOVE_DOWN: &str = "lilia.task-session.task.move-down";
pub const TASK_DROP_SEARCH: &str = "lilia.task-session.task.drop-search";
pub const TASK_MOVE_PROJECT_TARGET: &str = "lilia.task-session.task.move-project-target";
pub const TASK_MOVE_PROJECT: &str = "lilia.task-session.task.move-project";
pub const TASK_PARENT_TARGET: &str = "lilia.task-session.task.parent-target";
pub const TASK_REPARENT: &str = "lilia.task-session.task.reparent";
pub const TASK_PARENT_CLEAR: &str = "lilia.task-session.task.parent-clear";
pub const TASK_ARCHIVE: &str = "lilia.task-session.task.archive";
pub const COMPOSER_INPUT: &str = "lilia.task-session.composer.input";
pub const COMPOSER_PASTE_TEXT: &str = "lilia.task-session.composer.paste-text";
pub const COMPOSER_PASTE_IMAGE: &str = "lilia.task-session.composer.paste-image";
pub const COMPOSER_PASTE_FILES: &str = "lilia.task-session.composer.paste-files";
pub const COMPOSER_ATTACH_FILE: &str = "lilia.task-session.composer.attach-file";
pub const COMPOSER_ATTACH_DIRECTORY: &str = "lilia.task-session.composer.attach-directory";
pub const COMPOSER_PLAN_MODE: &str = "lilia.task-session.composer.plan-mode";
pub const COMPOSER_GOAL_MODE: &str = "lilia.task-session.composer.goal-mode";
pub const COMPOSER_PERMISSION: &str = "lilia.task-session.composer.permission";
pub const COMPOSER_MODEL: &str = "lilia.task-session.composer.model";
pub const COMPOSER_REASONING: &str = "lilia.task-session.composer.reasoning";
pub const COMPOSER_OPTIMIZE_PROMPT: &str = "lilia.task-session.composer.optimize-prompt";
pub const COMPOSER_WORKTREE: &str = "lilia.task-session.composer.worktree";
pub const COMPOSER_WORKTREE_PICK: &str = "lilia.task-session.composer.worktree.pick";
pub const COMPOSER_WORKTREE_RETRY: &str = "lilia.task-session.composer.worktree.retry";
pub const COMPOSER_ROUTE_APPLY: &str = "lilia.task-session.composer.route.apply";
pub const COMPOSER_ROUTE_DISMISS: &str = "lilia.task-session.composer.route.dismiss";
pub const COMPOSER_SEND: &str = "lilia.task-session.composer.send";
pub const COMPOSER_COMPACT_CONTEXT: &str = "lilia.task-session.composer.compact-context";
pub const COMPOSER_INTERRUPT: &str = "lilia.task-session.composer.interrupt";
pub const COMPOSER_SUGGESTIONS_REFRESH: &str = "lilia.task-session.composer.suggestions.refresh";

pub fn composer_suggestion(item_id: &str) -> String {
    format!("lilia.task-session.composer.suggestion.{item_id}")
}

pub fn composer_slash_command(command_name: &str) -> String {
    format!("lilia.task-session.composer.slash.{command_name}")
}
pub fn composer_conversation_reference(task_id: &str) -> String {
    format!("lilia.task-session.composer.conversation.{task_id}")
}
pub fn composer_context_attachment(relative_path: &str) -> String {
    format!("lilia.task-session.composer.context.{relative_path}")
}

pub fn task_popup_suggestions_refresh(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.suggestions.refresh")
}

pub fn task_popup_suggestion(window_id: u64, item_id: &str) -> String {
    format!("lilia.task-popup.{window_id}.composer.suggestion.{item_id}")
}
pub fn composer_conversation_reference_remove(task_id: &str) -> String {
    format!("lilia.task-session.composer.conversation.{task_id}.remove")
}
pub const TODO_INPUT: &str = "lilia.task-session.todo.input";
pub const TODO_SAVE: &str = "lilia.task-session.todo.save";
pub const TODO_CANCEL_EDIT: &str = "lilia.task-session.todo.cancel-edit";
pub const GOAL_INPUT: &str = "lilia.task-session.goal.input";
pub const GOAL_SET: &str = "lilia.task-session.goal.set";
pub const GOAL_REFRESH: &str = "lilia.task-session.goal.refresh";
pub const GOAL_CLEAR: &str = "lilia.task-session.goal.clear";
pub const TASK_MEMORY_TOGGLE: &str = "lilia.task-session.memory.toggle";
pub const TASK_MEMORY_RESET_COOLDOWN: &str = "lilia.task-session.memory.reset-cooldown";
pub const WORKTREE_CREATE: &str = "lilia.task-session.worktree.create";
pub const WORKTREE_ATTACH: &str = "lilia.task-session.worktree.attach";
pub const WORKTREE_OPEN: &str = "lilia.task-session.worktree.open";
pub const WORKTREE_CLEAR: &str = "lilia.task-session.worktree.clear";
pub const WORKTREE_REQUEST_CLEANUP: &str = "lilia.task-session.worktree.request-cleanup";
pub const WORKTREE_REQUEST_MERGE: &str = "lilia.task-session.worktree.request-merge";
pub const WORKTREE_CONFIRM: &str = "lilia.task-session.worktree.confirm";
pub const WORKTREE_CANCEL: &str = "lilia.task-session.worktree.cancel";
pub const SETTINGS_OPEN: &str = "lilia.settings.open";
pub const SETTINGS_SIDEBAR: &str = "lilia.settings.sidebar";
pub const SETTINGS_BACK: &str = "lilia.settings.back";
pub const SETTINGS_APPEARANCE: &str = "lilia.settings.appearance";
pub const SETTINGS_PROJECT: &str = "lilia.settings.project";
pub const SETTINGS_PROVIDER: &str = "lilia.settings.provider";
pub const SETTINGS_AGENT: &str = "lilia.settings.agent";
pub const SETTINGS_QUOTA: &str = "lilia.settings.quota";
pub const SETTINGS_EXTENSIONS: &str = "lilia.settings.extensions";
pub const SETTINGS_REMOTE: &str = "lilia.settings.remote";
pub const SETTINGS_DESKTOP: &str = "lilia.settings.desktop";
pub const SETTINGS_DATA: &str = "lilia.settings.data";
pub const SETTINGS_ABOUT: &str = "lilia.settings.about";
pub const THEME_LIGHT: &str = "lilia.settings.appearance.theme.light";
pub const THEME_DARK: &str = "lilia.settings.appearance.theme.dark";
pub const SIDEBAR_MODE_GROUPED: &str = "lilia.settings.appearance.sidebar.grouped";
pub const SIDEBAR_MODE_UNIFIED: &str = "lilia.settings.appearance.sidebar.unified";
pub const UNIFIED_SIDEBAR_TASK_PREFIX: &str = "lilia.sidebar.conversation.";

pub fn unified_sidebar_task(task_id: &str) -> String {
    format!("{UNIFIED_SIDEBAR_TASK_PREFIX}{task_id}")
}
pub const PROJECT_SETTINGS_CLONE_PARENT: &str = "lilia.settings.project.clone-parent";
pub const PROJECT_SETTINGS_PICK_CLONE_PARENT: &str = "lilia.settings.project.clone-parent.pick";
pub const PROJECT_WORKTREE_MODE: &str = "lilia.settings.project.worktree.mode";
pub const PROJECT_WORKTREE_PARENT: &str = "lilia.settings.project.worktree.parent";
pub const PROJECT_WORKTREE_PICK_PARENT: &str = "lilia.settings.project.worktree.parent.pick";
pub const PROJECT_WORKTREE_INSTRUCTIONS: &str = "lilia.settings.project.worktree.instructions";
pub const PROJECT_WORKTREE_CLEANUP: &str = "lilia.settings.project.worktree.cleanup";
pub const PROJECT_SETTINGS_SAVE: &str = "lilia.settings.project.save";
pub const PROVIDER_SECRET_INPUT: &str = "lilia.settings.provider.secret";
pub const PROVIDER_SAVE: &str = "lilia.settings.provider.save";
pub const PROVIDER_REFRESH: &str = "lilia.settings.provider.refresh";
pub const PROVIDER_MODEL_INPUT: &str = "lilia.settings.provider.runtime.model";
pub const PROVIDER_OPENAI_ENDPOINT_INPUT: &str = "lilia.settings.provider.runtime.openai-endpoint";
pub const PROVIDER_ANTHROPIC_ENDPOINT_INPUT: &str =
    "lilia.settings.provider.runtime.anthropic-endpoint";
pub const PROVIDER_RUNTIME_SAVE: &str = "lilia.settings.provider.runtime.save";
pub const PROVIDER_RUNTIME_RESET: &str = "lilia.settings.provider.runtime.reset";
pub const ASSISTANT_AI_BASE_URL_INPUT: &str = "lilia.settings.provider.assistant-ai.base-url";
pub const ASSISTANT_AI_MODEL_INPUT: &str = "lilia.settings.provider.assistant-ai.model";
pub const ASSISTANT_AI_SECRET_INPUT: &str = "lilia.settings.provider.assistant-ai.secret";
pub const ASSISTANT_AI_NEW_MODEL_ID: &str =
    "lilia.settings.provider.assistant-ai.model-pool.new-id";
pub const ASSISTANT_AI_NEW_MODEL_LABEL: &str =
    "lilia.settings.provider.assistant-ai.model-pool.new-label";
pub const ASSISTANT_AI_ADD_MODEL: &str = "lilia.settings.provider.assistant-ai.model-pool.add";
pub const ASSISTANT_AI_FETCH_MODELS: &str = "lilia.settings.provider.assistant-ai.model-pool.fetch";
pub const ASSISTANT_AI_TEST_CONNECTION: &str = "lilia.settings.provider.assistant-ai.test";
pub const ASSISTANT_AI_SAVE: &str = "lilia.settings.provider.assistant-ai.save";
pub const ASSISTANT_AI_CLEAR_SECRET: &str = "lilia.settings.provider.assistant-ai.clear-secret";
const ASSISTANT_AI_MODEL_POOL_PREFIX: &str = "lilia.settings.provider.assistant-ai.model-pool.";

pub fn assistant_ai_model_label(model_id: &str) -> String {
    format!("{ASSISTANT_AI_MODEL_POOL_PREFIX}{model_id}.label")
}

pub fn parse_assistant_ai_model_label(target_id: &str) -> Option<&str> {
    target_id
        .strip_prefix(ASSISTANT_AI_MODEL_POOL_PREFIX)?
        .strip_suffix(".label")
        .filter(|model_id| !model_id.is_empty())
}
pub const FEATURE_MODEL_TITLE_INPUT: &str = "lilia.settings.provider.feature-model.title";
pub const FEATURE_MODEL_SUGGESTION_INPUT: &str = "lilia.settings.provider.feature-model.suggestion";
pub const FEATURE_MODEL_PROMPT_ROUTER_INPUT: &str =
    "lilia.settings.provider.feature-model.prompt-router";
pub const FEATURE_MODEL_PROMPT_OPTIMIZE_INPUT: &str =
    "lilia.settings.provider.feature-model.prompt-optimize";
pub const FEATURE_MODEL_AUTO_TURN_INPUT: &str =
    "lilia.settings.provider.feature-model.auto-turn-decision";
pub const FEATURE_MODEL_SAVE: &str = "lilia.settings.provider.feature-model.save";
pub const FEATURE_CUSTOM_PRESET_NAME: &str = "lilia.settings.provider.feature-preset.custom.name";
pub const FEATURE_CUSTOM_PRESET_ADD: &str = "lilia.settings.provider.feature-preset.custom.add";
const FEATURE_PRESET_PREFIX: &str = "lilia.settings.provider.feature-preset.";

pub fn feature_preset_model(preset_id: &str) -> String {
    format!("{FEATURE_PRESET_PREFIX}{preset_id}.model")
}

pub fn feature_preset_effort(preset_id: &str) -> String {
    format!("{FEATURE_PRESET_PREFIX}{preset_id}.effort")
}

pub fn feature_preset_label(preset_id: &str) -> String {
    format!("{FEATURE_PRESET_PREFIX}{preset_id}.label")
}

pub fn feature_preset_remove(preset_id: &str) -> String {
    format!("{FEATURE_PRESET_PREFIX}{preset_id}.remove")
}

pub fn parse_feature_preset_model(target_id: &str) -> Option<&str> {
    target_id
        .strip_prefix(FEATURE_PRESET_PREFIX)?
        .strip_suffix(".model")
        .filter(|preset_id| !preset_id.is_empty())
}

pub fn parse_feature_preset_effort(target_id: &str) -> Option<&str> {
    target_id
        .strip_prefix(FEATURE_PRESET_PREFIX)?
        .strip_suffix(".effort")
        .filter(|preset_id| !preset_id.is_empty())
}

pub fn parse_feature_preset_label(target_id: &str) -> Option<&str> {
    target_id
        .strip_prefix(FEATURE_PRESET_PREFIX)?
        .strip_suffix(".label")
        .filter(|preset_id| !preset_id.is_empty())
}

pub fn parse_feature_preset_remove(target_id: &str) -> Option<&str> {
    target_id
        .strip_prefix(FEATURE_PRESET_PREFIX)?
        .strip_suffix(".remove")
        .filter(|preset_id| !preset_id.is_empty())
}
pub const CONVERSATION_SUGGESTIONS_ENABLE: &str =
    "lilia.settings.provider.conversation-suggestions.enable";
pub const CONVERSATION_SUGGESTIONS_DISABLE: &str =
    "lilia.settings.provider.conversation-suggestions.disable";
pub const AGENT_SUBAGENT_MODE: &str = "lilia.settings.agent.subagents";
pub const AGENT_NON_INTERRUPT_MODE: &str = "lilia.settings.agent.non-interrupt";
pub const AGENT_DEBUG_MODE: &str = "lilia.settings.agent.debug";
pub const DEBUG_TIMELINE_PREFIX: &str = "lilia.debug.timeline.";

pub fn debug_timeline_action(action_id: &str) -> String {
    format!("{DEBUG_TIMELINE_PREFIX}{action_id}")
}
pub const AGENT_AUTO_TURN: &str = "lilia.settings.agent.auto-turn";
pub const AGENT_AUTO_MODEL_TIER: &str = "lilia.settings.agent.auto-turn.model-tier";
pub const AGENT_AUTO_REASONING_EFFORT: &str = "lilia.settings.agent.auto-turn.reasoning-effort";
pub const AGENT_AUTO_PLAN_MODE: &str = "lilia.settings.agent.auto-turn.plan-mode";
pub const AGENT_AUTO_GOAL_MODE: &str = "lilia.settings.agent.auto-turn.goal-mode";
pub const AGENT_AUTO_SESSION_FORK: &str = "lilia.settings.agent.auto-turn.session-fork";
pub const AGENT_NEW: &str = "lilia.settings.agent.custom.new";
pub const AGENT_NAME_INPUT: &str = "lilia.settings.agent.custom.name";
pub const AGENT_DESCRIPTION_INPUT: &str = "lilia.settings.agent.custom.description";
pub const AGENT_INSTRUCTION_INPUT: &str = "lilia.settings.agent.custom.instruction";
pub const AGENT_SAVE: &str = "lilia.settings.agent.custom.save";
pub const AGENT_CANCEL_EDIT: &str = "lilia.settings.agent.custom.cancel";
pub const QUOTA_REFRESH: &str = "lilia.settings.quota.refresh";
pub const QUOTA_DAYS: &str = "lilia.settings.quota.days";
pub const QUOTA_BACKEND: &str = "lilia.settings.quota.backend";
pub const EXTENSIONS_REFRESH: &str = "lilia.settings.extensions.refresh";
pub const EXTENSIONS_SKILL_ID: &str = "lilia.settings.extensions.skill.id";
pub const EXTENSIONS_SKILL_DESCRIPTION: &str = "lilia.settings.extensions.skill.description";
pub const EXTENSIONS_SKILL_CREATE: &str = "lilia.settings.extensions.skill.create";
pub const EXTENSIONS_PLUGIN_SOURCE: &str = "lilia.settings.extensions.plugin.source";
pub const EXTENSIONS_PLUGIN_PICK: &str = "lilia.settings.extensions.plugin.pick";
pub const EXTENSIONS_PLUGIN_INSTALL: &str = "lilia.settings.extensions.plugin.install";
pub const EXTENSIONS_ACTIVATE_MCP: &str = "lilia.settings.extensions.activate-mcp";
pub const EXTENSIONS_MCP_ADD: &str = "lilia.settings.extensions.mcp.add";
pub const EXTENSIONS_MCP_ID: &str = "lilia.settings.extensions.mcp.editor.id";
pub const EXTENSIONS_MCP_TRANSPORT: &str = "lilia.settings.extensions.mcp.editor.transport";
pub const EXTENSIONS_MCP_LOCATION: &str = "lilia.settings.extensions.mcp.editor.location";
pub const EXTENSIONS_MCP_ARGS: &str = "lilia.settings.extensions.mcp.editor.args";
pub const EXTENSIONS_MCP_CREDENTIAL_NAMES: &str =
    "lilia.settings.extensions.mcp.editor.credential-names";
pub const EXTENSIONS_MCP_ENABLED: &str = "lilia.settings.extensions.mcp.editor.enabled";
pub const EXTENSIONS_MCP_SAVE: &str = "lilia.settings.extensions.mcp.editor.save";
pub const EXTENSIONS_MCP_CANCEL: &str = "lilia.settings.extensions.mcp.editor.cancel";
pub const REMOTE_REFRESH: &str = "lilia.settings.remote.refresh";
pub const REMOTE_HOST_TOGGLE: &str = "lilia.settings.remote.host-toggle";
pub const REMOTE_PC_NAME: &str = "lilia.settings.remote.pc-name";
pub const REMOTE_PC_NAME_SAVE: &str = "lilia.settings.remote.pc-name-save";
pub const REMOTE_KEEP_AWAKE: &str = "lilia.settings.remote.keep-awake";
pub const REMOTE_START_PAIRING: &str = "lilia.settings.remote.start-pairing";
pub const REMOTE_CANCEL_PAIRING: &str = "lilia.settings.remote.cancel-pairing";
pub const REMOTE_COPY_PAIRING: &str = "lilia.settings.remote.copy-pairing";
pub const DESKTOP_SHORTCUT: &str = "lilia.settings.desktop.shortcut";
pub const DESKTOP_SHORTCUT_SAVE: &str = "lilia.settings.desktop.shortcut.save";
pub const DESKTOP_SHORTCUT_CLEAR: &str = "lilia.settings.desktop.shortcut.clear";
pub const DESKTOP_UPDATE_CHECK: &str = "lilia.settings.desktop.update.check";
pub const DESKTOP_UPDATE_INSTALL: &str = "lilia.settings.desktop.update.install";
pub const DESKTOP_UPDATE_RELEASES: &str = "lilia.settings.desktop.update.releases";
pub const DESKTOP_UPDATE_PROMPT_CONFIRM: &str = "lilia.update.prompt.confirm";
pub const DESKTOP_UPDATE_PROMPT_DISMISS: &str = "lilia.update.prompt.dismiss";
pub const DATA_IMPORT_PICK_SOURCE: &str = "lilia.settings.data.pick-source";
pub const DATA_IMPORT_CREDENTIALS: &str = "lilia.settings.data.credentials";
pub const DATA_IMPORT_EXECUTE: &str = "lilia.settings.data.execute";
pub const DATA_IMPORT_RESET: &str = "lilia.settings.data.reset";
pub const DATA_IMPORT_RESTART: &str = "lilia.settings.data.restart";

pub fn project(project_id: &str) -> String {
    format!("lilia.project.{project_id}")
}

pub fn project_reorder_before(project_id: &str, before_project_id: Option<&str>) -> String {
    format!(
        "lilia.project-reorder.{project_id}.before.{}",
        before_project_id.unwrap_or("end")
    )
}

pub fn archived_project(project_id: &str) -> String {
    format!("lilia.project.{project_id}.restore")
}

pub fn automation(workflow_id: &str) -> String {
    format!("lilia.automation.{workflow_id}")
}

pub fn automation_run(run_id: &str) -> String {
    format!("lilia.automations.run.{run_id}")
}

pub fn automation_scope(field: &str, value: &str) -> String {
    format!("lilia.automations.scope.{field}.{value}")
}

pub fn automation_node_config(field: &str) -> String {
    format!("lilia.automations.node.config.{field}")
}

pub fn milestone(milestone_id: &str) -> String {
    format!("lilia.roadmap.milestone.{milestone_id}")
}

pub fn milestone_task(milestone_id: &str, task_id: &str) -> String {
    format!("lilia.roadmap.milestone.{milestone_id}.task.{task_id}")
}

pub fn memory(memory_id: &str) -> String {
    format!("lilia.memory.{memory_id}")
}

pub fn provider(provider_id: &str) -> String {
    format!("lilia.settings.provider.{provider_id}")
}

pub fn provider_revoke(credential_id: &str) -> String {
    format!("lilia.settings.provider.credential.{credential_id}.revoke")
}

pub fn custom_agent_edit(agent_id: &str) -> String {
    format!("lilia.settings.agent.custom.{agent_id}.edit")
}

pub fn custom_agent_toggle(agent_id: &str) -> String {
    format!("lilia.settings.agent.custom.{agent_id}.toggle")
}

pub fn custom_agent_delete(agent_id: &str) -> String {
    format!("lilia.settings.agent.custom.{agent_id}.delete")
}

pub fn remote_revoke(device_id: &str) -> String {
    format!("lilia.settings.remote.device.{device_id}.revoke")
}

pub fn composer_remove_attachment(attachment_id: &str) -> String {
    format!("lilia.task-session.composer.attachment.{attachment_id}.remove")
}

pub fn todo_edit(todo_id: &str) -> String {
    format!("lilia.task-session.todo.{todo_id}.edit")
}

pub fn todo_toggle(todo_id: &str) -> String {
    format!("lilia.task-session.todo.{todo_id}.toggle")
}

pub fn todo_priority(todo_id: &str) -> String {
    format!("lilia.task-session.todo.{todo_id}.priority")
}

pub fn todo_delete(todo_id: &str) -> String {
    format!("lilia.task-session.todo.{todo_id}.delete")
}

pub fn task(task_id: &str) -> String {
    format!("lilia.task.{task_id}")
}

pub fn extensions_mcp_edit(server_id: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.edit")
}

pub fn extensions_skill_toggle(skill_id: &str) -> String {
    format!("lilia.settings.extensions.skill.{skill_id}.toggle")
}

pub fn extensions_skill_delete(skill_id: &str) -> String {
    format!("lilia.settings.extensions.skill.{skill_id}.delete")
}

pub fn extensions_skill_delete_confirm(skill_id: &str) -> String {
    format!("lilia.settings.extensions.skill.{skill_id}.delete.confirm")
}

pub fn extensions_skill_delete_cancel(skill_id: &str) -> String {
    format!("lilia.settings.extensions.skill.{skill_id}.delete.cancel")
}

pub fn extensions_plugin_toggle(plugin_id: &str) -> String {
    format!("lilia.settings.extensions.plugin.{plugin_id}.toggle")
}

pub fn extensions_plugin_delete(plugin_id: &str) -> String {
    format!("lilia.settings.extensions.plugin.{plugin_id}.delete")
}

pub fn extensions_plugin_delete_confirm(plugin_id: &str) -> String {
    format!("lilia.settings.extensions.plugin.{plugin_id}.delete.confirm")
}

pub fn extensions_plugin_delete_cancel(plugin_id: &str) -> String {
    format!("lilia.settings.extensions.plugin.{plugin_id}.delete.cancel")
}

pub fn extensions_hook_draft(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.draft")
}

pub fn extensions_hook_add_handler(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.handler.add")
}

pub fn extensions_hook_handler_field(source_id: &str, index: usize, field: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.handler.{index}.{field}")
}

pub fn extensions_hook_remove_handler(source_id: &str, index: usize) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.handler.{index}.remove")
}

pub fn extensions_hook_create(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.create")
}

pub fn extensions_hook_save(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.save")
}

pub fn extensions_hook_toggle(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.toggle")
}

pub fn extensions_hook_delete(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.delete")
}

pub fn extensions_hook_delete_confirm(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.delete.confirm")
}

pub fn extensions_hook_delete_cancel(source_id: &str) -> String {
    format!("lilia.settings.extensions.hook.{source_id}.delete.cancel")
}

pub fn extensions_mcp_toggle(server_id: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.toggle")
}

pub fn extensions_mcp_delete(server_id: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.delete")
}

pub fn extensions_mcp_delete_confirm(server_id: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.delete-confirm")
}

pub fn extensions_mcp_delete_cancel(server_id: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.delete-cancel")
}

pub fn extensions_mcp_credential(server_id: &str, kind: &str, name: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.credential.{kind}.{name}")
}

pub fn extensions_mcp_credential_save(server_id: &str, kind: &str, name: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.credential.{kind}.{name}.save")
}

pub fn extensions_mcp_credential_delete(server_id: &str, kind: &str, name: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.credential.{kind}.{name}.delete")
}

pub fn extensions_mcp_resource_read(server_id: &str, uri: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.resource.{uri}.read")
}

pub fn extensions_mcp_prompt_arguments(server_id: &str, prompt_name: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.prompt.{prompt_name}.arguments")
}

pub fn extensions_mcp_prompt_get(server_id: &str, prompt_name: &str) -> String {
    format!("lilia.settings.extensions.mcp.{server_id}.prompt.{prompt_name}.get")
}

pub fn task_reorder_before(task_id: &str, before_task_id: Option<&str>) -> String {
    format!(
        "lilia.task-reorder.{task_id}.before.{}",
        before_task_id.unwrap_or("end")
    )
}

pub fn task_drop_target(
    task_id: &str,
    project_id: Option<&str>,
    parent_id: Option<&str>,
) -> String {
    format!(
        "lilia.task-drop.{task_id}.project.{}.parent.{}",
        project_id.unwrap_or("inbox"),
        parent_id.unwrap_or("root")
    )
}

pub fn workspace_tab(item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}")
}

pub fn workspace_tab_close(item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.close")
}

pub fn workspace_tab_move_to_new_window(item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.move-to-new-window")
}

pub fn workspace_window_project_action(window_id: u64, target_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.project-action.{target_id}")
}

pub fn workspace_tab_drag_left(item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.drag-left")
}

pub fn workspace_tab_drag_right(item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.drag-right")
}

pub fn workspace_tab_drag_to_pane(item_id: &str, pane_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.drag-to-pane.{pane_id}")
}

pub fn workspace_pane(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}")
}

pub fn workspace_pane_overview(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.overview")
}

pub fn workspace_pane_focus(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.focus")
}

pub fn workspace_pane_split_horizontal(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.split-horizontal")
}

pub fn workspace_pane_split_vertical(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.split-vertical")
}

pub fn workspace_pane_move_next(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.move-next")
}

pub fn workspace_pane_close(pane_id: &str) -> String {
    format!("lilia.workspace.pane.{pane_id}.close")
}

pub fn workspace_split_grow(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("lilia.workspace.split.{first_pane_id}.{second_pane_id}.grow")
}

pub fn workspace_split_shrink(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("lilia.workspace.split.{first_pane_id}.{second_pane_id}.shrink")
}

pub fn workspace_split_reset(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("lilia.workspace.split.{first_pane_id}.{second_pane_id}.reset")
}

pub fn conversation_status_task(task_id: &str) -> String {
    format!("lilia.conversation-status.task.{task_id}")
}

pub fn conversation_status_stop(task_id: &str) -> String {
    format!("lilia.conversation-status.task.{task_id}.stop")
}

pub fn task_popup(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}")
}

pub fn task_popup_close(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.close")
}

pub fn task_popup_focus_main(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.focus-main")
}

pub fn task_popup_new_chat(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.new-chat")
}

pub fn task_popup_move_to_main(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.move-to-main")
}

pub fn task_popup_tab_drag_to_main(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("lilia.task-popup.{window_id}.tab.{item_id}.drag-to-main-pane.{pane_id}")
}

pub fn workspace_window_tab(window_id: u64, item_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.tab.{item_id}")
}

pub fn workspace_window_tab_close(window_id: u64, item_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.tab.{item_id}.close")
}

pub fn workspace_window_tab_drag_to_pane(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.tab.{item_id}.drag-to-pane.{pane_id}")
}

pub fn workspace_window_pane(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}")
}

pub fn workspace_window_pane_focus(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}.focus")
}

pub fn workspace_window_pane_split_horizontal(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}.split-horizontal")
}

pub fn workspace_window_pane_split_vertical(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}.split-vertical")
}

pub fn workspace_window_pane_move_next(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}.move-next")
}

pub fn workspace_window_pane_close(window_id: u64, pane_id: &str) -> String {
    format!("lilia.workspace-window.{window_id}.pane.{pane_id}.close")
}

pub fn workspace_window_split_grow(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!("lilia.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.grow")
}

pub fn workspace_window_split_shrink(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!("lilia.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.shrink")
}

pub fn workspace_window_split_reset(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!("lilia.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.reset")
}

pub fn workspace_tab_drag_to_window(window_id: u64, item_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.drag-to-window.{window_id}")
}

pub fn workspace_tab_drag_to_window_pane(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("lilia.workspace.tab.{item_id}.drag-to-window.{window_id}.pane.{pane_id}")
}

pub fn task_popup_load_earlier(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.timeline.load-earlier")
}

pub fn task_popup_composer(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.input")
}

pub fn task_popup_attach_file(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.attach-file")
}

pub fn task_popup_attach_directory(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.attach-directory")
}

pub fn task_popup_paste_text(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.paste-text")
}

pub fn task_popup_paste_image(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.paste-image")
}

pub fn task_popup_paste_files(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.paste-files")
}

pub fn attachment_preview(window_id: u64, attachment_id: &str) -> String {
    format!("lilia.window.{window_id}.attachment.{attachment_id}.preview")
}

pub fn attachment_preview_close(window_id: u64) -> String {
    format!("lilia.window.{window_id}.attachment-preview.close")
}

pub fn attachment_preview_open_path(window_id: u64) -> String {
    format!("lilia.window.{window_id}.attachment-preview.open-path")
}

pub fn task_popup_remove_attachment(window_id: u64, attachment_id: &str) -> String {
    format!("lilia.task-popup.{window_id}.composer.attachment.{attachment_id}.remove")
}

pub fn task_popup_send(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.send")
}

pub fn task_popup_ask_child(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.ask-child")
}

pub fn task_popup_optimize_prompt(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.optimize-prompt")
}

pub fn task_popup_worktree(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.worktree")
}

pub fn task_popup_worktree_pick(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.worktree.pick")
}

pub fn task_popup_worktree_retry(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.worktree.retry")
}

pub fn task_popup_model(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.model")
}

pub fn task_popup_reasoning(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.reasoning")
}

pub fn task_popup_route_apply(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.route.apply")
}

pub fn task_popup_route_dismiss(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.route.dismiss")
}

pub fn task_popup_compact_context(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.compact-context")
}

pub fn task_popup_interrupt(window_id: u64) -> String {
    format!("lilia.task-popup.{window_id}.composer.interrupt")
}

pub fn task_popup_slash_command(window_id: u64, command_name: &str) -> String {
    format!("lilia.task-popup.{window_id}.composer.slash.{command_name}")
}
pub fn review_workflow_target(window_id: u64, target: &str) -> String {
    format!("lilia.task-window.{window_id}.review-workflow.target.{target}")
}
pub fn review_workflow_target_input(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.review-workflow.target-input")
}
pub fn review_workflow_submit(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.review-workflow.submit")
}
pub fn review_workflow_cancel(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.review-workflow.cancel")
}
pub fn task_popup_conversation_reference(window_id: u64, task_id: &str) -> String {
    format!("lilia.task-window.{window_id}.composer.conversation.{task_id}")
}
pub fn task_popup_context_attachment(window_id: u64, relative_path: &str) -> String {
    format!("lilia.task-window.{window_id}.composer.context.{relative_path}")
}
pub fn task_popup_conversation_reference_remove(window_id: u64, task_id: &str) -> String {
    format!("lilia.task-window.{window_id}.composer.conversation.{task_id}.remove")
}

pub fn archived_task(task_id: &str) -> String {
    format!("lilia.task.{task_id}.restore")
}

pub fn timeline_event(event_id: &str) -> String {
    format!("lilia.task-session.timeline.{event_id}")
}

pub fn timeline_copy(event_id: &str) -> String {
    format!("lilia.task-session.timeline.{event_id}.copy")
}

pub fn timeline_selection_copy(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.selection.copy")
}

pub fn timeline_selection_quote(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.selection.quote")
}

pub fn timeline_selection_ask(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.selection.ask")
}

pub fn markdown_image(window_id: u64, source: &str) -> String {
    format!(
        "lilia.task-window.{window_id}.markdown-image.{:016x}",
        stable_target_hash(source)
    )
}

pub fn markdown_image_retry(window_id: u64, source: &str) -> String {
    format!(
        "lilia.task-window.{window_id}.markdown-image.{:016x}.retry",
        stable_target_hash(source)
    )
}

pub fn markdown_image_close(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.markdown-image.close")
}

pub fn timeline_retry(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.retry")
}

pub fn timeline_apply_suggestion(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.apply-suggestion")
}

pub fn timeline_continue(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.continue")
}

pub fn timeline_fork(window_id: u64, event_id: &str) -> String {
    format!("lilia.task-window.{window_id}.timeline.{event_id}.fork")
}

pub fn session_branch_clear(window_id: u64) -> String {
    format!("lilia.task-window.{window_id}.session-branch.clear")
}

pub fn approval_approve(request_id: &str) -> String {
    format!("lilia.task-session.approval.{request_id}.approve")
}

pub fn approval_deny(request_id: &str) -> String {
    format!("lilia.task-session.approval.{request_id}.deny")
}

pub fn title_update_accept(request_id: &str) -> String {
    format!("lilia.task-session.title-update.{request_id}.accept")
}

pub fn title_update_decline(request_id: &str) -> String {
    format!("lilia.task-session.title-update.{request_id}.decline")
}

pub fn architecture_allow(request_id: &str) -> String {
    format!("lilia.task-session.architecture.{request_id}.allow")
}

pub fn architecture_deny(request_id: &str) -> String {
    format!("lilia.task-session.architecture.{request_id}.deny")
}

pub fn interaction_input(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.input")
}

pub fn interaction_submit(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.submit")
}

pub fn interaction_cancel(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.cancel")
}

pub fn interaction_reject(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.reject")
}

pub fn interaction_back(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.back")
}

pub fn interaction_skip(request_id: &str) -> String {
    format!("lilia.task-session.interaction.{request_id}.skip")
}

pub fn interaction_option(request_id: &str, option_index: usize) -> String {
    format!("lilia.task-session.interaction.{request_id}.option.{option_index}")
}

pub fn tool_consent_command(window_id: u64, request_id: &str) -> String {
    format!("lilia.task-window.{window_id}.tool-consent.{request_id}.command")
}

pub fn tool_consent_message(window_id: u64, request_id: &str) -> String {
    format!("lilia.task-window.{window_id}.tool-consent.{request_id}.message")
}

pub fn tool_consent_allow(window_id: u64, request_id: &str) -> String {
    format!("lilia.task-window.{window_id}.tool-consent.{request_id}.allow")
}

pub fn tool_consent_deny(window_id: u64, request_id: &str) -> String {
    format!("lilia.task-window.{window_id}.tool-consent.{request_id}.deny")
}

pub fn mcp_field(request_id: &str, field_index: usize) -> String {
    format!("lilia.task-session.mcp.{request_id}.field.{field_index}")
}

pub fn mcp_option(request_id: &str, field_index: usize, option_index: usize) -> String {
    format!("lilia.task-session.mcp.{request_id}.field.{field_index}.option.{option_index}")
}

pub fn mcp_boolean(request_id: &str, field_index: usize) -> String {
    format!("lilia.task-session.mcp.{request_id}.field.{field_index}.toggle")
}

pub fn mcp_raw_json(request_id: &str) -> String {
    format!("lilia.task-session.mcp.{request_id}.json")
}

pub fn mcp_open_url(request_id: &str) -> String {
    format!("lilia.task-session.mcp.{request_id}.open-url")
}

pub fn mcp_accept(request_id: &str) -> String {
    format!("lilia.task-session.mcp.{request_id}.accept")
}

pub fn mcp_decline(request_id: &str) -> String {
    format!("lilia.task-session.mcp.{request_id}.decline")
}

pub fn mcp_cancel(request_id: &str) -> String {
    format!("lilia.task-session.mcp.{request_id}.cancel")
}

pub fn plan_approve(request_id: &str) -> String {
    format!("lilia.task-session.plan.{request_id}.approve")
}

pub fn plan_revise(request_id: &str) -> String {
    format!("lilia.task-session.plan.{request_id}.revise")
}

pub fn plan_decline(request_id: &str) -> String {
    format!("lilia.task-session.plan.{request_id}.decline")
}

pub fn plan_cancel(request_id: &str) -> String {
    format!("lilia.task-session.plan.{request_id}.cancel-turn")
}

fn stable_target_hash(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}
