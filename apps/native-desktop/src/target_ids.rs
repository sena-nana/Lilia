pub const APP_ROOT: &str = "native-preview.app";
pub const CONVERSATION_STATUS_OPEN: &str = "native-preview.conversation-status.open";
pub const CONVERSATION_STATUS_WINDOW: &str = "native-preview.conversation-status.window";
pub const CONVERSATION_STATUS_CLOSE: &str = "native-preview.conversation-status.close";
pub const PROJECTS_LIST: &str = "native-preview.projects";
pub const INBOX: &str = "native-preview.inbox";
pub const PROJECTS_REFRESH: &str = "native-preview.projects.refresh";
pub const PROJECT_CREATE: &str = "native-preview.projects.create";
pub const PROJECT_CLONE_OPEN: &str = "native-preview.projects.clone";
pub const PROJECT_CLONE_BACK: &str = "native-preview.project-clone.back";
pub const PROJECT_CLONE_REPOSITORY: &str = "native-preview.project-clone.repository";
pub const PROJECT_CLONE_PARENT: &str = "native-preview.project-clone.parent";
pub const PROJECT_CLONE_PICK_PARENT: &str = "native-preview.project-clone.pick-parent";
pub const PROJECT_CLONE_START: &str = "native-preview.project-clone.start";
pub const PROJECT_CLONE_CANCEL: &str = "native-preview.project-clone.cancel";
pub const GITHUB_BIND_START: &str = "native-preview.project-clone.github.bind";
pub const GITHUB_BIND_CANCEL: &str = "native-preview.project-clone.github.bind.cancel";
pub const GITHUB_VERIFICATION_OPEN: &str = "native-preview.project-clone.github.verification.open";
pub const GITHUB_USER_CODE_COPY: &str = "native-preview.project-clone.github.user-code.copy";
pub const GITHUB_UNBIND: &str = "native-preview.project-clone.github.unbind";
pub const GITHUB_REPOS_REFRESH: &str = "native-preview.project-clone.github.repos.refresh";
pub const GITHUB_REPOS_LOAD_MORE: &str = "native-preview.project-clone.github.repos.load-more";

pub fn github_repository(full_name: &str) -> String {
    format!("native-preview.project-clone.github.repo.{full_name}")
}
pub const PROJECT_NAME: &str = "native-preview.project.name";
pub const PROJECT_WORKSPACE: &str = "native-preview.project.workspace";
pub const PROJECT_WORKSPACE_PICK: &str = "native-preview.project.workspace.pick";
pub const PROJECT_WORKSPACE_CLEAR: &str = "native-preview.project.workspace.clear";
pub const PROJECT_SAVE: &str = "native-preview.project.save";
pub const PROJECT_PIN: &str = "native-preview.project.pin";
pub const PROJECT_MOVE_UP: &str = "native-preview.project.move-up";
pub const PROJECT_MOVE_DOWN: &str = "native-preview.project.move-down";
pub const PROJECT_REMOVE: &str = "native-preview.project.remove";
pub const PROJECT_REMOVE_DIALOG: &str = "native-preview.project.remove.dialog";
pub const PROJECT_REMOVE_CONFIRM: &str = "native-preview.project.remove.confirm";
pub const PROJECT_REMOVE_CANCEL: &str = "native-preview.project.remove.cancel";
pub const PROJECT_TASKS: &str = "native-preview.project.tasks";
pub const ROADMAP_OPEN: &str = "native-preview.project.roadmap";
pub const ROADMAP_REFRESH: &str = "native-preview.roadmap.refresh";
pub const ROADMAP_CREATE: &str = "native-preview.roadmap.create";
pub const ROADMAP_TITLE: &str = "native-preview.roadmap.milestone.title";
pub const ROADMAP_DESCRIPTION: &str = "native-preview.roadmap.milestone.description";
pub const ROADMAP_DUE_DATE: &str = "native-preview.roadmap.milestone.due-date";
pub const ROADMAP_SAVE: &str = "native-preview.roadmap.milestone.save";
pub const ROADMAP_STATUS: &str = "native-preview.roadmap.milestone.status";
pub const ROADMAP_MOVE_UP: &str = "native-preview.roadmap.milestone.move-up";
pub const ROADMAP_MOVE_DOWN: &str = "native-preview.roadmap.milestone.move-down";
pub const ROADMAP_DELETE: &str = "native-preview.roadmap.milestone.delete";
pub const MEMORY_OPEN: &str = "native-preview.project.memory";
pub const MEMORY_REFRESH: &str = "native-preview.memory.refresh";
pub const MEMORY_NEW: &str = "native-preview.memory.new";
pub const MEMORY_TITLE: &str = "native-preview.memory.title";
pub const MEMORY_BODY: &str = "native-preview.memory.body";
pub const MEMORY_TAGS: &str = "native-preview.memory.tags";
pub const MEMORY_SCOPE: &str = "native-preview.memory.scope";
pub const MEMORY_SAVE: &str = "native-preview.memory.save";
pub const MEMORY_TOGGLE: &str = "native-preview.memory.toggle";
pub const MEMORY_DELETE: &str = "native-preview.memory.delete";
pub const MEMORY_SETTINGS_GLOBAL: &str = "native-preview.memory.settings.enabled";
pub const MEMORY_SETTINGS_BASELINE: &str = "native-preview.memory.settings.baseline";
pub const MEMORY_SETTINGS_COOLDOWN: &str = "native-preview.memory.settings.cooldown";
pub const MEMORY_SETTINGS_COOLDOWN_INPUT: &str = "native-preview.memory.settings.cooldown.input";
pub const MEMORY_SETTINGS_COOLDOWN_SAVE: &str = "native-preview.memory.settings.cooldown.save";
pub const CODING_TOOLS_OPEN: &str = "native-preview.project.coding-tools";
pub const CODING_TOOLS_REFRESH: &str = "native-preview.coding-tools.refresh";
pub const CODING_TOOLS_QUERY: &str = "native-preview.coding-tools.query";
pub const CODING_TOOLS_SEARCH: &str = "native-preview.coding-tools.search";
pub const CODING_TOOLS_CLOSE: &str = "native-preview.coding-tools.close";
pub const CODING_TOOLS_OPEN_WORKSPACE: &str = "native-preview.coding-tools.open-workspace";
pub const CODING_TOOLS_OPEN_TERMINAL: &str = "native-preview.coding-tools.open-terminal";
pub const CODING_TOOLS_SAVE_MEMORY: &str = "native-preview.coding-tools.save-memory";
pub const ARCHITECTURE_OPEN: &str = "native-preview.project.architecture";
pub const ARCHITECTURE_REFRESH: &str = "native-preview.architecture.refresh";
pub const ARCHITECTURE_ROLLBACK: &str = "native-preview.architecture.rollback";
pub const AUTOMATIONS_OPEN: &str = "native-preview.automations.open";
pub const AUTOMATIONS_BACK: &str = "native-preview.automations.back";
pub const AUTOMATIONS_REFRESH: &str = "native-preview.automations.refresh";
pub const AUTOMATIONS_CREATE: &str = "native-preview.automations.create";
pub const AUTOMATIONS_NAME: &str = "native-preview.automations.name";
pub const AUTOMATIONS_SAVE_DRAFT: &str = "native-preview.automations.save-draft";
pub const AUTOMATIONS_ADD_AGENT: &str = "native-preview.automations.add.agent";
pub const AUTOMATIONS_ADD_TOOL: &str = "native-preview.automations.add.tool";
pub const AUTOMATIONS_ADD_LOGIC: &str = "native-preview.automations.add.logic";
pub const AUTOMATIONS_ADD_HUMAN: &str = "native-preview.automations.add.human";
pub const AUTOMATIONS_DELETE_SELECTION: &str = "native-preview.automations.selection.delete";
pub const AUTOMATIONS_SCOPE_INCLUDE_INBOX: &str = "native-preview.automations.scope.include-inbox";
pub const AUTOMATIONS_NODE_TITLE: &str = "native-preview.automations.node.title";
pub const AUTOMATIONS_NODE_CONFIG: &str = "native-preview.automations.node.config";
pub const AUTOMATIONS_NODE_SAVE: &str = "native-preview.automations.node.save";
pub const AUTOMATIONS_PUBLISH: &str = "native-preview.automations.publish";
pub const AUTOMATIONS_TOGGLE: &str = "native-preview.automations.toggle";
pub const AUTOMATIONS_DELETE: &str = "native-preview.automations.delete";
pub const AUTOMATIONS_RUN: &str = "native-preview.automations.run";
pub const AUTOMATIONS_CANCEL: &str = "native-preview.automations.run.cancel";
pub const AUTOMATIONS_HUMAN_RESPONSE: &str = "native-preview.automations.run.human-response";
pub const AUTOMATIONS_RESUME: &str = "native-preview.automations.run.resume";
pub const TASKS_LIST: &str = "native-preview.tasks";
pub const WORKSPACE_OVERVIEW_TAB: &str = "native-preview.workspace.tab.overview";
pub const TASK_SEARCH: &str = "native-preview.tasks.search";
pub const TASK_CREATE_TITLE: &str = "native-preview.tasks.create.title";
pub const TASK_CREATE: &str = "native-preview.tasks.create";
pub const TASK_SESSION: &str = "native-preview.task-session";
pub const TASK_POPUP_OPEN: &str = "native-preview.task-session.popup.open";
pub const TASK_POPUP_MOVE_SELECTED: &str = "native-preview.task-session.popup.move-selected";
pub const TASK_SESSION_BACK: &str = "native-preview.task-session.back";
pub const TASK_SESSION_SUMMARY: &str = "native-preview.task-session.summary";
pub const TASK_SESSION_TIMELINE: &str = "native-preview.task-session.timeline";
pub const TASK_SESSION_TIMELINE_LOAD_EARLIER: &str =
    "native-preview.task-session.timeline.load-earlier";
pub const TASK_SESSION_INSPECTOR: &str = "native-preview.task-session.inspector";
pub const TASK_SESSION_INSPECTOR_TOGGLE: &str = "native-preview.task-session.inspector.toggle";
pub const TASK_TITLE: &str = "native-preview.task-session.task.title";
pub const TASK_SAVE: &str = "native-preview.task-session.task.save";
pub const TASK_STATUS: &str = "native-preview.task-session.task.status";
pub const TASK_PRIORITY: &str = "native-preview.task-session.task.priority";
pub const TASK_PIN: &str = "native-preview.task-session.task.pin";
pub const TASK_MOVE_UP: &str = "native-preview.task-session.task.move-up";
pub const TASK_MOVE_DOWN: &str = "native-preview.task-session.task.move-down";
pub const TASK_DROP_SEARCH: &str = "native-preview.task-session.task.drop-search";
pub const TASK_MOVE_PROJECT_TARGET: &str = "native-preview.task-session.task.move-project-target";
pub const TASK_MOVE_PROJECT: &str = "native-preview.task-session.task.move-project";
pub const TASK_PARENT_TARGET: &str = "native-preview.task-session.task.parent-target";
pub const TASK_REPARENT: &str = "native-preview.task-session.task.reparent";
pub const TASK_PARENT_CLEAR: &str = "native-preview.task-session.task.parent-clear";
pub const TASK_ARCHIVE: &str = "native-preview.task-session.task.archive";
pub const COMPOSER_INPUT: &str = "native-preview.task-session.composer.input";
pub const COMPOSER_PASTE_TEXT: &str = "native-preview.task-session.composer.paste-text";
pub const COMPOSER_PASTE_IMAGE: &str = "native-preview.task-session.composer.paste-image";
pub const COMPOSER_ATTACH_FILE: &str = "native-preview.task-session.composer.attach-file";
pub const COMPOSER_ATTACH_DIRECTORY: &str = "native-preview.task-session.composer.attach-directory";
pub const COMPOSER_PLAN_MODE: &str = "native-preview.task-session.composer.plan-mode";
pub const COMPOSER_GOAL_MODE: &str = "native-preview.task-session.composer.goal-mode";
pub const COMPOSER_PERMISSION: &str = "native-preview.task-session.composer.permission";
pub const COMPOSER_SEND: &str = "native-preview.task-session.composer.send";
pub const COMPOSER_INTERRUPT: &str = "native-preview.task-session.composer.interrupt";

pub fn composer_slash_command(command_name: &str) -> String {
    format!("native-preview.task-session.composer.slash.{command_name}")
}
pub fn composer_conversation_reference(task_id: &str) -> String {
    format!("native-preview.task-session.composer.conversation.{task_id}")
}
pub fn composer_context_attachment(relative_path: &str) -> String {
    format!("native-preview.task-session.composer.context.{relative_path}")
}
pub fn composer_conversation_reference_remove(task_id: &str) -> String {
    format!("native-preview.task-session.composer.conversation.{task_id}.remove")
}
pub const TODO_INPUT: &str = "native-preview.task-session.todo.input";
pub const TODO_SAVE: &str = "native-preview.task-session.todo.save";
pub const TODO_CANCEL_EDIT: &str = "native-preview.task-session.todo.cancel-edit";
pub const GOAL_INPUT: &str = "native-preview.task-session.goal.input";
pub const GOAL_SET: &str = "native-preview.task-session.goal.set";
pub const GOAL_REFRESH: &str = "native-preview.task-session.goal.refresh";
pub const GOAL_CLEAR: &str = "native-preview.task-session.goal.clear";
pub const TASK_MEMORY_TOGGLE: &str = "native-preview.task-session.memory.toggle";
pub const TASK_MEMORY_RESET_COOLDOWN: &str = "native-preview.task-session.memory.reset-cooldown";
pub const WORKTREE_CREATE: &str = "native-preview.task-session.worktree.create";
pub const WORKTREE_ATTACH: &str = "native-preview.task-session.worktree.attach";
pub const WORKTREE_OPEN: &str = "native-preview.task-session.worktree.open";
pub const WORKTREE_CLEAR: &str = "native-preview.task-session.worktree.clear";
pub const WORKTREE_REQUEST_CLEANUP: &str = "native-preview.task-session.worktree.request-cleanup";
pub const WORKTREE_REQUEST_MERGE: &str = "native-preview.task-session.worktree.request-merge";
pub const WORKTREE_CONFIRM: &str = "native-preview.task-session.worktree.confirm";
pub const WORKTREE_CANCEL: &str = "native-preview.task-session.worktree.cancel";
pub const SETTINGS_OPEN: &str = "native-preview.settings.open";
pub const SETTINGS_SIDEBAR: &str = "native-preview.settings.sidebar";
pub const SETTINGS_BACK: &str = "native-preview.settings.back";
pub const SETTINGS_APPEARANCE: &str = "native-preview.settings.appearance";
pub const SETTINGS_PROVIDER: &str = "native-preview.settings.provider";
pub const SETTINGS_AGENT: &str = "native-preview.settings.agent";
pub const SETTINGS_QUOTA: &str = "native-preview.settings.quota";
pub const SETTINGS_EXTENSIONS: &str = "native-preview.settings.extensions";
pub const SETTINGS_REMOTE: &str = "native-preview.settings.remote";
pub const SETTINGS_DESKTOP: &str = "native-preview.settings.desktop";
pub const SETTINGS_DATA: &str = "native-preview.settings.data";
pub const THEME_LIGHT: &str = "native-preview.settings.appearance.theme.light";
pub const THEME_DARK: &str = "native-preview.settings.appearance.theme.dark";
pub const PROVIDER_SECRET_INPUT: &str = "native-preview.settings.provider.secret";
pub const PROVIDER_SAVE: &str = "native-preview.settings.provider.save";
pub const PROVIDER_REFRESH: &str = "native-preview.settings.provider.refresh";
pub const PROVIDER_MODEL_INPUT: &str = "native-preview.settings.provider.runtime.model";
pub const PROVIDER_OPENAI_ENDPOINT_INPUT: &str =
    "native-preview.settings.provider.runtime.openai-endpoint";
pub const PROVIDER_ANTHROPIC_ENDPOINT_INPUT: &str =
    "native-preview.settings.provider.runtime.anthropic-endpoint";
pub const PROVIDER_RUNTIME_SAVE: &str = "native-preview.settings.provider.runtime.save";
pub const PROVIDER_RUNTIME_RESET: &str = "native-preview.settings.provider.runtime.reset";
pub const AGENT_SUBAGENT_MODE: &str = "native-preview.settings.agent.subagents";
pub const AGENT_AUTO_TURN: &str = "native-preview.settings.agent.auto-turn";
pub const AGENT_AUTO_MODEL_TIER: &str = "native-preview.settings.agent.auto-turn.model-tier";
pub const AGENT_AUTO_REASONING_EFFORT: &str =
    "native-preview.settings.agent.auto-turn.reasoning-effort";
pub const AGENT_AUTO_PLAN_MODE: &str = "native-preview.settings.agent.auto-turn.plan-mode";
pub const AGENT_AUTO_GOAL_MODE: &str = "native-preview.settings.agent.auto-turn.goal-mode";
pub const AGENT_AUTO_SESSION_FORK: &str = "native-preview.settings.agent.auto-turn.session-fork";
pub const AGENT_NEW: &str = "native-preview.settings.agent.custom.new";
pub const AGENT_NAME_INPUT: &str = "native-preview.settings.agent.custom.name";
pub const AGENT_DESCRIPTION_INPUT: &str = "native-preview.settings.agent.custom.description";
pub const AGENT_INSTRUCTION_INPUT: &str = "native-preview.settings.agent.custom.instruction";
pub const AGENT_SAVE: &str = "native-preview.settings.agent.custom.save";
pub const AGENT_CANCEL_EDIT: &str = "native-preview.settings.agent.custom.cancel";
pub const QUOTA_REFRESH: &str = "native-preview.settings.quota.refresh";
pub const QUOTA_DAYS: &str = "native-preview.settings.quota.days";
pub const QUOTA_BACKEND: &str = "native-preview.settings.quota.backend";
pub const EXTENSIONS_REFRESH: &str = "native-preview.settings.extensions.refresh";
pub const EXTENSIONS_SKILL_ID: &str = "native-preview.settings.extensions.skill.id";
pub const EXTENSIONS_SKILL_DESCRIPTION: &str =
    "native-preview.settings.extensions.skill.description";
pub const EXTENSIONS_SKILL_CREATE: &str = "native-preview.settings.extensions.skill.create";
pub const EXTENSIONS_PLUGIN_SOURCE: &str = "native-preview.settings.extensions.plugin.source";
pub const EXTENSIONS_PLUGIN_PICK: &str = "native-preview.settings.extensions.plugin.pick";
pub const EXTENSIONS_PLUGIN_INSTALL: &str = "native-preview.settings.extensions.plugin.install";
pub const EXTENSIONS_ACTIVATE_MCP: &str = "native-preview.settings.extensions.activate-mcp";
pub const EXTENSIONS_MCP_ADD: &str = "native-preview.settings.extensions.mcp.add";
pub const EXTENSIONS_MCP_ID: &str = "native-preview.settings.extensions.mcp.editor.id";
pub const EXTENSIONS_MCP_TRANSPORT: &str =
    "native-preview.settings.extensions.mcp.editor.transport";
pub const EXTENSIONS_MCP_LOCATION: &str = "native-preview.settings.extensions.mcp.editor.location";
pub const EXTENSIONS_MCP_ARGS: &str = "native-preview.settings.extensions.mcp.editor.args";
pub const EXTENSIONS_MCP_CREDENTIAL_NAMES: &str =
    "native-preview.settings.extensions.mcp.editor.credential-names";
pub const EXTENSIONS_MCP_ENABLED: &str = "native-preview.settings.extensions.mcp.editor.enabled";
pub const EXTENSIONS_MCP_SAVE: &str = "native-preview.settings.extensions.mcp.editor.save";
pub const EXTENSIONS_MCP_CANCEL: &str = "native-preview.settings.extensions.mcp.editor.cancel";
pub const REMOTE_REFRESH: &str = "native-preview.settings.remote.refresh";
pub const REMOTE_HOST_TOGGLE: &str = "native-preview.settings.remote.host-toggle";
pub const REMOTE_PC_NAME: &str = "native-preview.settings.remote.pc-name";
pub const REMOTE_PC_NAME_SAVE: &str = "native-preview.settings.remote.pc-name-save";
pub const REMOTE_KEEP_AWAKE: &str = "native-preview.settings.remote.keep-awake";
pub const REMOTE_START_PAIRING: &str = "native-preview.settings.remote.start-pairing";
pub const REMOTE_CANCEL_PAIRING: &str = "native-preview.settings.remote.cancel-pairing";
pub const REMOTE_COPY_PAIRING: &str = "native-preview.settings.remote.copy-pairing";
pub const DESKTOP_SHORTCUT: &str = "native-preview.settings.desktop.shortcut";
pub const DESKTOP_SHORTCUT_SAVE: &str = "native-preview.settings.desktop.shortcut.save";
pub const DESKTOP_SHORTCUT_CLEAR: &str = "native-preview.settings.desktop.shortcut.clear";
pub const DESKTOP_UPDATE_CHECK: &str = "native-preview.settings.desktop.update.check";
pub const DESKTOP_UPDATE_INSTALL: &str = "native-preview.settings.desktop.update.install";
pub const DESKTOP_UPDATE_RELEASES: &str = "native-preview.settings.desktop.update.releases";
pub const DATA_IMPORT_PICK_SOURCE: &str = "native-preview.settings.data.pick-source";
pub const DATA_IMPORT_CREDENTIALS: &str = "native-preview.settings.data.credentials";
pub const DATA_IMPORT_EXECUTE: &str = "native-preview.settings.data.execute";
pub const DATA_IMPORT_RESET: &str = "native-preview.settings.data.reset";
pub const DATA_IMPORT_RESTART: &str = "native-preview.settings.data.restart";

pub fn project(project_id: &str) -> String {
    format!("native-preview.project.{project_id}")
}

pub fn project_reorder_before(project_id: &str, before_project_id: Option<&str>) -> String {
    format!(
        "native-preview.project-reorder.{project_id}.before.{}",
        before_project_id.unwrap_or("end")
    )
}

pub fn archived_project(project_id: &str) -> String {
    format!("native-preview.project.{project_id}.restore")
}

pub fn automation(workflow_id: &str) -> String {
    format!("native-preview.automation.{workflow_id}")
}

pub fn automation_run(run_id: &str) -> String {
    format!("native-preview.automations.run.{run_id}")
}

pub fn automation_scope(field: &str, value: &str) -> String {
    format!("native-preview.automations.scope.{field}.{value}")
}

pub fn automation_node_config(field: &str) -> String {
    format!("native-preview.automations.node.config.{field}")
}

pub fn milestone(milestone_id: &str) -> String {
    format!("native-preview.roadmap.milestone.{milestone_id}")
}

pub fn milestone_task(milestone_id: &str, task_id: &str) -> String {
    format!("native-preview.roadmap.milestone.{milestone_id}.task.{task_id}")
}

pub fn memory(memory_id: &str) -> String {
    format!("native-preview.memory.{memory_id}")
}

pub fn provider(provider_id: &str) -> String {
    format!("native-preview.settings.provider.{provider_id}")
}

pub fn provider_revoke(credential_id: &str) -> String {
    format!("native-preview.settings.provider.credential.{credential_id}.revoke")
}

pub fn custom_agent_edit(agent_id: &str) -> String {
    format!("native-preview.settings.agent.custom.{agent_id}.edit")
}

pub fn custom_agent_toggle(agent_id: &str) -> String {
    format!("native-preview.settings.agent.custom.{agent_id}.toggle")
}

pub fn custom_agent_delete(agent_id: &str) -> String {
    format!("native-preview.settings.agent.custom.{agent_id}.delete")
}

pub fn remote_revoke(device_id: &str) -> String {
    format!("native-preview.settings.remote.device.{device_id}.revoke")
}

pub fn composer_remove_attachment(attachment_id: &str) -> String {
    format!("native-preview.task-session.composer.attachment.{attachment_id}.remove")
}

pub fn todo_edit(todo_id: &str) -> String {
    format!("native-preview.task-session.todo.{todo_id}.edit")
}

pub fn todo_toggle(todo_id: &str) -> String {
    format!("native-preview.task-session.todo.{todo_id}.toggle")
}

pub fn todo_priority(todo_id: &str) -> String {
    format!("native-preview.task-session.todo.{todo_id}.priority")
}

pub fn todo_delete(todo_id: &str) -> String {
    format!("native-preview.task-session.todo.{todo_id}.delete")
}

pub fn task(task_id: &str) -> String {
    format!("native-preview.task.{task_id}")
}

pub fn extensions_mcp_edit(server_id: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.edit")
}

pub fn extensions_skill_toggle(skill_id: &str) -> String {
    format!("native-preview.settings.extensions.skill.{skill_id}.toggle")
}

pub fn extensions_skill_delete(skill_id: &str) -> String {
    format!("native-preview.settings.extensions.skill.{skill_id}.delete")
}

pub fn extensions_skill_delete_confirm(skill_id: &str) -> String {
    format!("native-preview.settings.extensions.skill.{skill_id}.delete.confirm")
}

pub fn extensions_skill_delete_cancel(skill_id: &str) -> String {
    format!("native-preview.settings.extensions.skill.{skill_id}.delete.cancel")
}

pub fn extensions_plugin_toggle(plugin_id: &str) -> String {
    format!("native-preview.settings.extensions.plugin.{plugin_id}.toggle")
}

pub fn extensions_plugin_delete(plugin_id: &str) -> String {
    format!("native-preview.settings.extensions.plugin.{plugin_id}.delete")
}

pub fn extensions_plugin_delete_confirm(plugin_id: &str) -> String {
    format!("native-preview.settings.extensions.plugin.{plugin_id}.delete.confirm")
}

pub fn extensions_plugin_delete_cancel(plugin_id: &str) -> String {
    format!("native-preview.settings.extensions.plugin.{plugin_id}.delete.cancel")
}

pub fn extensions_hook_draft(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.draft")
}

pub fn extensions_hook_create(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.create")
}

pub fn extensions_hook_save(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.save")
}

pub fn extensions_hook_toggle(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.toggle")
}

pub fn extensions_hook_delete(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.delete")
}

pub fn extensions_hook_delete_confirm(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.delete.confirm")
}

pub fn extensions_hook_delete_cancel(source_id: &str) -> String {
    format!("native-preview.settings.extensions.hook.{source_id}.delete.cancel")
}

pub fn extensions_mcp_toggle(server_id: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.toggle")
}

pub fn extensions_mcp_delete(server_id: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.delete")
}

pub fn extensions_mcp_delete_confirm(server_id: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.delete-confirm")
}

pub fn extensions_mcp_delete_cancel(server_id: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.delete-cancel")
}

pub fn extensions_mcp_credential(server_id: &str, kind: &str, name: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.credential.{kind}.{name}")
}

pub fn extensions_mcp_credential_save(server_id: &str, kind: &str, name: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.credential.{kind}.{name}.save")
}

pub fn extensions_mcp_credential_delete(server_id: &str, kind: &str, name: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.credential.{kind}.{name}.delete")
}

pub fn extensions_mcp_resource_read(server_id: &str, uri: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.resource.{uri}.read")
}

pub fn extensions_mcp_prompt_arguments(server_id: &str, prompt_name: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.prompt.{prompt_name}.arguments")
}

pub fn extensions_mcp_prompt_get(server_id: &str, prompt_name: &str) -> String {
    format!("native-preview.settings.extensions.mcp.{server_id}.prompt.{prompt_name}.get")
}

pub fn task_reorder_before(task_id: &str, before_task_id: Option<&str>) -> String {
    format!(
        "native-preview.task-reorder.{task_id}.before.{}",
        before_task_id.unwrap_or("end")
    )
}

pub fn task_drop_target(
    task_id: &str,
    project_id: Option<&str>,
    parent_id: Option<&str>,
) -> String {
    format!(
        "native-preview.task-drop.{task_id}.project.{}.parent.{}",
        project_id.unwrap_or("inbox"),
        parent_id.unwrap_or("root")
    )
}

pub fn workspace_tab(item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}")
}

pub fn workspace_tab_close(item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.close")
}

pub fn workspace_tab_move_to_new_window(item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.move-to-new-window")
}

pub fn workspace_window_project_action(window_id: u64, target_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.project-action.{target_id}")
}

pub fn workspace_tab_drag_left(item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.drag-left")
}

pub fn workspace_tab_drag_right(item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.drag-right")
}

pub fn workspace_tab_drag_to_pane(item_id: &str, pane_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.drag-to-pane.{pane_id}")
}

pub fn workspace_pane(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}")
}

pub fn workspace_pane_overview(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.overview")
}

pub fn workspace_pane_focus(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.focus")
}

pub fn workspace_pane_split_horizontal(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.split-horizontal")
}

pub fn workspace_pane_split_vertical(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.split-vertical")
}

pub fn workspace_pane_move_next(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.move-next")
}

pub fn workspace_pane_close(pane_id: &str) -> String {
    format!("native-preview.workspace.pane.{pane_id}.close")
}

pub fn workspace_split_grow(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("native-preview.workspace.split.{first_pane_id}.{second_pane_id}.grow")
}

pub fn workspace_split_shrink(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("native-preview.workspace.split.{first_pane_id}.{second_pane_id}.shrink")
}

pub fn workspace_split_reset(first_pane_id: &str, second_pane_id: &str) -> String {
    format!("native-preview.workspace.split.{first_pane_id}.{second_pane_id}.reset")
}

pub fn conversation_status_task(task_id: &str) -> String {
    format!("native-preview.conversation-status.task.{task_id}")
}

pub fn task_popup(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}")
}

pub fn task_popup_close(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.close")
}

pub fn task_popup_move_to_main(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.move-to-main")
}

pub fn task_popup_tab_drag_to_main(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("native-preview.task-popup.{window_id}.tab.{item_id}.drag-to-main-pane.{pane_id}")
}

pub fn workspace_window_tab(window_id: u64, item_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.tab.{item_id}")
}

pub fn workspace_window_tab_close(window_id: u64, item_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.tab.{item_id}.close")
}

pub fn workspace_window_tab_drag_to_pane(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.tab.{item_id}.drag-to-pane.{pane_id}")
}

pub fn workspace_window_pane(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}")
}

pub fn workspace_window_pane_focus(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}.focus")
}

pub fn workspace_window_pane_split_horizontal(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}.split-horizontal")
}

pub fn workspace_window_pane_split_vertical(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}.split-vertical")
}

pub fn workspace_window_pane_move_next(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}.move-next")
}

pub fn workspace_window_pane_close(window_id: u64, pane_id: &str) -> String {
    format!("native-preview.workspace-window.{window_id}.pane.{pane_id}.close")
}

pub fn workspace_window_split_grow(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!(
        "native-preview.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.grow"
    )
}

pub fn workspace_window_split_shrink(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!(
        "native-preview.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.shrink"
    )
}

pub fn workspace_window_split_reset(
    window_id: u64,
    first_pane_id: &str,
    second_pane_id: &str,
) -> String {
    format!(
        "native-preview.workspace-window.{window_id}.split.{first_pane_id}.{second_pane_id}.reset"
    )
}

pub fn workspace_tab_drag_to_window(window_id: u64, item_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.drag-to-window.{window_id}")
}

pub fn workspace_tab_drag_to_window_pane(window_id: u64, item_id: &str, pane_id: &str) -> String {
    format!("native-preview.workspace.tab.{item_id}.drag-to-window.{window_id}.pane.{pane_id}")
}

pub fn task_popup_load_earlier(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.timeline.load-earlier")
}

pub fn task_popup_composer(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.input")
}

pub fn task_popup_attach_file(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.attach-file")
}

pub fn task_popup_attach_directory(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.attach-directory")
}

pub fn task_popup_paste_text(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.paste-text")
}

pub fn task_popup_paste_image(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.paste-image")
}

pub fn attachment_preview(window_id: u64, attachment_id: &str) -> String {
    format!("native-preview.window.{window_id}.attachment.{attachment_id}.preview")
}

pub fn attachment_preview_close(window_id: u64) -> String {
    format!("native-preview.window.{window_id}.attachment-preview.close")
}

pub fn attachment_preview_open_path(window_id: u64) -> String {
    format!("native-preview.window.{window_id}.attachment-preview.open-path")
}

pub fn task_popup_remove_attachment(window_id: u64, attachment_id: &str) -> String {
    format!("native-preview.task-popup.{window_id}.composer.attachment.{attachment_id}.remove")
}

pub fn task_popup_send(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.send")
}

pub fn task_popup_interrupt(window_id: u64) -> String {
    format!("native-preview.task-popup.{window_id}.composer.interrupt")
}

pub fn task_popup_slash_command(window_id: u64, command_name: &str) -> String {
    format!("native-preview.task-popup.{window_id}.composer.slash.{command_name}")
}
pub fn task_popup_conversation_reference(window_id: u64, task_id: &str) -> String {
    format!("native-preview.task-window.{window_id}.composer.conversation.{task_id}")
}
pub fn task_popup_context_attachment(window_id: u64, relative_path: &str) -> String {
    format!("native-preview.task-window.{window_id}.composer.context.{relative_path}")
}
pub fn task_popup_conversation_reference_remove(window_id: u64, task_id: &str) -> String {
    format!("native-preview.task-window.{window_id}.composer.conversation.{task_id}.remove")
}

pub fn archived_task(task_id: &str) -> String {
    format!("native-preview.task.{task_id}.restore")
}

pub fn timeline_event(event_id: &str) -> String {
    format!("native-preview.task-session.timeline.{event_id}")
}

pub fn timeline_copy(event_id: &str) -> String {
    format!("native-preview.task-session.timeline.{event_id}.copy")
}

pub fn approval_approve(request_id: &str) -> String {
    format!("native-preview.task-session.approval.{request_id}.approve")
}

pub fn approval_deny(request_id: &str) -> String {
    format!("native-preview.task-session.approval.{request_id}.deny")
}

pub fn architecture_allow(request_id: &str) -> String {
    format!("native-preview.task-session.architecture.{request_id}.allow")
}

pub fn architecture_deny(request_id: &str) -> String {
    format!("native-preview.task-session.architecture.{request_id}.deny")
}

pub fn interaction_input(request_id: &str) -> String {
    format!("native-preview.task-session.interaction.{request_id}.input")
}

pub fn interaction_submit(request_id: &str) -> String {
    format!("native-preview.task-session.interaction.{request_id}.submit")
}

pub fn interaction_cancel(request_id: &str) -> String {
    format!("native-preview.task-session.interaction.{request_id}.cancel")
}

pub fn interaction_option(request_id: &str, option_index: usize) -> String {
    format!("native-preview.task-session.interaction.{request_id}.option.{option_index}")
}

pub fn mcp_field(request_id: &str, field_index: usize) -> String {
    format!("native-preview.task-session.mcp.{request_id}.field.{field_index}")
}

pub fn mcp_option(request_id: &str, field_index: usize, option_index: usize) -> String {
    format!(
        "native-preview.task-session.mcp.{request_id}.field.{field_index}.option.{option_index}"
    )
}

pub fn mcp_boolean(request_id: &str, field_index: usize) -> String {
    format!("native-preview.task-session.mcp.{request_id}.field.{field_index}.toggle")
}

pub fn mcp_raw_json(request_id: &str) -> String {
    format!("native-preview.task-session.mcp.{request_id}.json")
}

pub fn mcp_open_url(request_id: &str) -> String {
    format!("native-preview.task-session.mcp.{request_id}.open-url")
}

pub fn mcp_accept(request_id: &str) -> String {
    format!("native-preview.task-session.mcp.{request_id}.accept")
}

pub fn mcp_decline(request_id: &str) -> String {
    format!("native-preview.task-session.mcp.{request_id}.decline")
}

pub fn mcp_cancel(request_id: &str) -> String {
    format!("native-preview.task-session.mcp.{request_id}.cancel")
}

pub fn plan_approve(request_id: &str) -> String {
    format!("native-preview.task-session.plan.{request_id}.approve")
}

pub fn plan_revise(request_id: &str) -> String {
    format!("native-preview.task-session.plan.{request_id}.revise")
}

pub fn plan_decline(request_id: &str) -> String {
    format!("native-preview.task-session.plan.{request_id}.decline")
}

pub fn plan_cancel(request_id: &str) -> String {
    format!("native-preview.task-session.plan.{request_id}.cancel-turn")
}
