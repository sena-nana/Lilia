use tauri::{utils::config::Color, Manager, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

mod agent_debug;
pub mod agent_events;
mod agent_interaction_contract;
pub mod agent_timeline;
mod agent_timeline_contract;
mod app_commands;
mod app_delivery;
mod app_events_contract;
mod automation;
mod chat;
mod chat_backends_contract;
mod cli_project;
mod contract_manifest;
mod conversation_suggestions;
mod github;
#[cfg(test)]
mod github_command_contract;
mod lilia_iab;
mod memory;
#[cfg(test)]
mod memory_command_contract;
#[cfg(test)]
mod milestone_command_contract;
mod native_agent;
mod native_agent_contract;
mod native_projection_hooks;
mod native_shared_services;
mod plugins;
#[cfg(test)]
mod plugins_command_contract;
mod popup_windows;
mod process_command;
mod product_core;
mod project_architecture_contract;
#[cfg(test)]
mod project_contract;
mod project_shell;
mod projects_tasks;
mod prompt_contract;
mod provider;
mod quota_usage;
mod quota_usage_contract;
mod remote_control;
mod runner_protocol_contract;
mod production_workspace_domain;
#[cfg(feature = "runtime-domain-reference")]
pub mod runtime_domains;
mod settings_store;
mod store;
#[cfg(test)]
mod system_popup_command_contract;
mod system_wake;
#[cfg(test)]
mod task_command_contract;
mod task_contract;
mod task_handoff;
#[cfg(test)]
mod todo_command_contract;
mod todos;
mod tray;
mod util;
mod window_state;
mod worktrees;

pub(crate) const MAIN_WINDOW_LABEL: &str = "main";

// 始终使用暗色：与前端 CSS 变量 --bg = #181818 保持一致，避免 Windows 拉伸/还原时
// 露出 WebView 之外的默认白底。
pub(crate) const BG: Color = Color(0x18, 0x18, 0x18, 0xFF);

/// Legacy brand labels retained only for reading old local rows / fixtures.
pub(crate) const BACKEND_CLAUDE: &str = "claude";
pub(crate) const BACKEND_CODEX: &str = "codex";

#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

fn handle_runtime_restore_failure<R: Runtime>(
    app: &tauri::AppHandle<R>,
    persisted: &chat::state::PersistedRuntimeState,
    runtime_name: &str,
    err: String,
) {
    let store = app.state::<chat::state::ChatStore>();
    store
        .running_tasks
        .lock()
        .unwrap()
        .remove(&persisted.task_id);
    chat::state::clear_running_handles(&store, &persisted.task_id);
    chat::state::clear_runtime_state_for_app(app, &persisted.task_id);
    chat::timeline_sink::persist_and_emit_error_timeline_event(
        app,
        &persisted.task_id,
        &persisted.turn.backend,
        Some(&persisted.turn.turn_id),
        format!("恢复 {runtime_name} runtime 失败：{err}"),
    );
}

fn restore_runtime_sessions_on_startup<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(chat_store) = app.try_state::<chat::state::ChatStore>() {
        if let Some(lilia_store) = app.try_state::<store::LiliaStore>() {
            if let Ok(conn) = lilia_store.conn() {
                // Legacy Node agent-runner sessions cannot be resumed; Native AgentKit only.
                let restored = chat::state::restore_active_runtime_sessions(&conn, &chat_store);
                for persisted in restored {
                    handle_runtime_restore_failure(
                        app,
                        &persisted,
                        "legacy-node",
                        "Node agent-runner 已移除；仅支持 Native AgentKit".to_string(),
                    );
                }
                if let Err(err) = automation::recover_abandoned_agent_runs(app, &chat_store) {
                    eprintln!("[automation] recover abandoned agent runs failed: {err}");
                }
                if let Ok(task_ids) = chat::state::list_pending_turn_task_ids(&conn) {
                    for task_id in task_ids {
                        let app_handle = app.clone();
                        std::thread::spawn(move || {
                            if let Err(err) =
                                chat::runner::resume_or_dispatch_persisted_pending_turn(
                                    app_handle.clone(),
                                    task_id.clone(),
                                )
                            {
                                eprintln!(
                                    "[chat-runtime] restore persisted queued turn failed for {}: {}",
                                    task_id, err
                                );
                            }
                        });
                    }
                }
            }
        }
    }
}

#[cfg(debug_assertions)]
fn apply_agent_debug_dev_url_override<R: Runtime>(context: &mut tauri::Context<R>) {
    if !agent_debug::agent_debug_enabled() {
        return;
    }
    let Ok(dev_url) = std::env::var("LILIA_AGENT_DEBUG_DEV_URL") else {
        return;
    };
    let dev_url = dev_url.trim();
    if dev_url.is_empty() {
        return;
    }
    match tauri::Url::parse(dev_url) {
        Ok(url) => {
            context.config_mut().build.dev_url = Some(url);
        }
        Err(err) => {
            eprintln!("[agent-debug] ignored invalid LILIA_AGENT_DEBUG_DEV_URL={dev_url:?}: {err}");
        }
    }
}

#[cfg(not(debug_assertions))]
fn apply_agent_debug_dev_url_override<R: Runtime>(_context: &mut tauri::Context<R>) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    apply_agent_debug_dev_url_override(&mut context);

    let builder = tauri::Builder::default();
    let updater_builder = tauri_plugin_updater::Builder::new();
    let updater_builder = if let Some(pubkey) = option_env!("TAURI_UPDATER_PUBKEY")
        .map(str::trim)
        .filter(|pubkey| !pubkey.is_empty())
    {
        updater_builder.pubkey(pubkey)
    } else {
        updater_builder
    };
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
        cli_project::handle_second_instance(app, argv, cwd);
    }));
    #[cfg(feature = "agent-debug-webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(updater_builder.build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Err(err) = popup_windows::open_popup_for_shortcut(app) {
                            eprintln!("[popup-window] shortcut failed: {err}");
                        }
                    }
                })
                .build(),
        )
        .manage(chat::state::ChatStore::default())
        .manage(cli_project::CliProjectOpenState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                let _ = window.set_background_color(Some(BG));
                if let Some(state) = window_state::load_main_window_state(app.handle()) {
                    window_state::restore_main_window_state(&window, state);
                }
                let _ = window.show();
            }
            let home = store::resolve_lilia_home();
            let legacy_store = store::LiliaStore::new(&home)
                .map_err(|err| std::io::Error::other(format!("legacy store init failed: {err}")))?;
            let product_core = product_core::EmbeddedProductCore::open(&home)
                .map_err(|err| std::io::Error::other(format!("product core init failed: {err}")))?;
            app.manage(product_core);
            app.manage(legacy_store);
            remote_control::restore_http_bridge_if_enabled(app.handle());
            restore_runtime_sessions_on_startup(app.handle());
            cli_project::handle_initial_args(app.handle());
            {
                if let Err(err) = tauri::async_runtime::block_on(async {
                    app_delivery::start_app_delivery_endpoint(app.handle())
                }) {
                    eprintln!("[app-delivery] endpoint start failed: {err}");
                }
            }
            popup_windows::register_initial_popup_shortcut(app.handle());
            if let Err(err) = tray::setup_tray(app.handle()) {
                eprintln!("[tray] setup failed: {err}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW_LABEL {
                return;
            }
            if matches!(
                event,
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
            ) {
                if let Some(webview_window) = window.get_webview_window(MAIN_WINDOW_LABEL) {
                    window_state::persist_main_window_state(&window.app_handle(), &webview_window);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            app_commands::app_restart,
            chat::attachments::chat_describe_attachments,
            chat::attachments::chat_read_clipboard_file_paths,
            chat::attachments::chat_save_clipboard_image,
            chat::attachments::chat_save_clipboard_text,
            chat::attachments::chat_search_context_attachments,
            chat::slash_commands::chat_search_slash_commands,
            cli_project::cli_project_open_consume_pending,
            task_handoff::task_handoff_get,
            chat::commands::chat_send_message,
            chat::commands::chat_interrupt_turn,
            chat::commands::chat_respond_agent_interaction,
            chat::commands::chat_send_process_session_command,
            chat::title_update::chat_respond_title_update,
            chat::commands::chat_list_models,
            chat::commands::chat_get_composer_state,
            chat::commands::chat_get_runtime_snapshot,
            chat::commands::chat_ack_restored_rollback,
            chat::commands::chat_set_composer_state,
            lilia_iab::lilia_iab_open,
            lilia_iab::lilia_iab_submit,
            memory::memory_list,
            memory::memory_upsert,
            memory::memory_set_enabled,
            memory::memory_delete,
            memory::memory_get_settings,
            memory::memory_set_settings,
            memory::memory_get_injection_state,
            memory::memory_set_task_enabled,
            memory::memory_reset_task_cooldown,
            provider::chat_check_env,
            provider::provider_get_config,
            provider::provider_set_config,
            provider::provider_get_active_backend,
            provider::provider_set_active_backend,
            provider::assistant_ai_get_config,
            provider::assistant_ai_set_config,
            provider::assistant_ai_fetch_models,
            provider::model_feature_list_model_options,
            provider::model_feature_get_settings,
            provider::model_feature_set_settings,
            provider::assistant_ai_test_connection,
            provider::assistant_ai_optimize_prompt,
            provider::agent_interaction_get_settings,
            provider::agent_interaction_set_settings,
            provider::agent_interaction_list_subagents,
            provider::agent_interaction_upsert_subagent,
            provider::agent_interaction_delete_subagent,
            popup_windows::popup_get_window_settings,
            popup_windows::popup_set_window_settings,
            popup_windows::popup_remember_last_project,
            popup_windows::popup_open_new_chat,
            popup_windows::popup_open_task,
            popup_windows::popup_open_child_question,
            popup_windows::popup_open_conversation_status,
            popup_windows::popup_toggle_conversation_status,
            popup_windows::popup_focus_main,
            provider::router_get_mode,
            provider::router_set_mode,
            conversation_suggestions::conversation_suggestions_get,
            conversation_suggestions::conversation_suggestions_get_sources,
            conversation_suggestions::conversation_suggestions_get_settings,
            conversation_suggestions::conversation_suggestions_set_settings,
            project_shell::project_get_settings,
            project_shell::project_set_settings,
            project_shell::git_clone_repo,
            github::github_get_binding_status,
            github::github_start_device_flow,
            github::github_poll_device_flow,
            github::github_unbind,
            github::github_list_repos,
            github::github_clone_repo,
            project_shell::system_open_path,
            project_shell::system_open_url,
            project_shell::system_open_in_vscode,
            worktrees::worktree_list,
            worktrees::worktree_create_for_task,
            worktrees::worktree_attach_task,
            worktrees::worktree_get_for_task,
            worktrees::worktree_clear_task,
            worktrees::worktree_cleanup_archive,
            worktrees::worktree_merge_delete_archive,
            plugins::plugins_overview,
            plugins::plugins_hooks_overview,
            plugins::plugins_create_skill,
            plugins::plugins_delete_skill,
            plugins::plugins_set_skill_enabled,
            plugins::plugins_set_package_enabled,
            plugins::plugins_create_mcp_server,
            plugins::plugins_update_mcp_server,
            plugins::plugins_delete_mcp_server,
            plugins::plugins_set_mcp_server_enabled,
            plugins::plugins_open_mcp_config,
            plugins::plugins_read_hook_source,
            plugins::plugins_update_hook_source,
            plugins::plugins_create_hook_source,
            plugins::plugins_delete_hook_source,
            plugins::plugins_set_hook_source_enabled,
            plugins::plugins_open_hook_config,
            todos::todo_list,
            todos::todo_create,
            todos::todo_update,
            todos::todo_delete,
            todos::todo_apply_agent_event,
            projects_tasks::project_list,
            projects_tasks::project_dashboard_list,
            projects_tasks::project_get,
            projects_tasks::project_create,
            projects_tasks::project_ensure_folders,
            projects_tasks::project_rename,
            projects_tasks::project_remove,
            projects_tasks::project_toggle_pin,
            projects_tasks::milestone_list,
            projects_tasks::milestone_create,
            projects_tasks::milestone_update,
            projects_tasks::milestone_delete,
            projects_tasks::milestone_reorder,
            projects_tasks::milestone_set_tasks,
            projects_tasks::project_architecture_get,
            projects_tasks::project_architecture_list_changes,
            projects_tasks::project_architecture_apply,
            projects_tasks::project_architecture_reject,
            projects_tasks::project_architecture_rollback,
            projects_tasks::task_list,
            projects_tasks::task_list_sidebar_conversations,
            projects_tasks::task_get,
            projects_tasks::task_create,
            projects_tasks::task_update,
            projects_tasks::task_update_dependencies,
            projects_tasks::task_delete,
            projects_tasks::task_promote,
            projects_tasks::task_archive_project,
            projects_tasks::task_archive,
            projects_tasks::task_toggle_pin,
            projects_tasks::project_reorder,
            projects_tasks::task_reorder,
            projects_tasks::task_reparent,
            agent_timeline::agent_timeline_list,
            agent_timeline::agent_timeline_clear_task,
            quota_usage::quota_usage_get_stats,
            remote_control::remote_control_status,
            remote_control::remote_control_set_host_enabled,
            remote_control::remote_control_set_pc_name,
            remote_control::remote_control_set_keep_awake_enabled,
            remote_control::remote_control_start_pairing,
            remote_control::remote_control_cancel_pairing,
            remote_control::remote_control_pair_device,
            remote_control::remote_control_revoke_device,
            remote_control::remote_control_dispatch_request,
            automation::commands::automation_list_workflows,
            automation::commands::automation_save_draft,
            automation::commands::automation_publish,
            automation::commands::automation_delete_workflow,
            automation::commands::automation_set_enabled,
            automation::commands::automation_run_once,
            automation::commands::automation_resume_run,
            automation::commands::automation_list_runs,
            automation::commands::automation_get_run,
            agent_debug::agent_debug_status,
            agent_debug::agent_debug_logs,
            agent_debug::agent_debug_runtime_snapshot,
            agent_debug::agent_debug_record_action,
            agent_debug::agent_debug_reset_state,
            product_core::product_core_status,
            product_core::product_create_entity,
            product_core::product_update_entity,
            product_core::product_get_entity,
            product_core::product_list_entities,
            product_core::product_list_events,
            native_agent::native_agent_host_status,
            native_agent::native_credential_providers,
            native_agent::native_credential_login,
            native_agent::native_credential_import,
            native_agent::native_credential_revoke,
            native_agent::native_credential_diagnostics,
            native_agent::native_quota_surface,
            native_agent::native_respond_approval,
            native_agent::native_product_timeline,
            native_agent::native_product_artifacts,
            native_agent::native_product_todos,
            native_agent::native_product_pending,
            native_agent::native_rebuild_product_timeline,
            native_agent::native_rebuild_ui_timeline_cache,
            native_shared_services::native_shared_coding_services_status,
            native_shared_services::native_shared_git_status,
            native_shared_services::native_shared_code_index_search,
            native_shared_services::native_shared_workspace_list,
            native_shared_services::native_shared_mcp_list_servers,
            native_shared_services::native_shared_lsp_status,
            native_shared_services::native_shared_lsp_open_workspace,
            native_shared_services::native_shared_memory_query,
            native_shared_services::native_shared_memory_write,
        ])
        .run(context)
        .expect("error while running tauri application");
}
