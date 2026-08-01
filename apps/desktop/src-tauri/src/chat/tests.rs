#[cfg(test)]
mod agent_event_sink_tests {
    use std::collections::BTreeMap;

    use rusqlite::Connection;
    use serde_json::{json, Value as JsonValue};

    use crate::agent_events::{
        runner_interaction_response_control_type, runner_interrupt_turn_control_type,
        runner_settings_update_control_type, AgentRuntimeEvent, AgentTurnContext,
    };
    use crate::agent_interaction_contract;
    use crate::agent_timeline;
    use crate::agent_timeline::AgentTimelineEventInput;
    use crate::chat::commands::{
        agent_interaction_response_payload, attach_stdin_delivery,
        composer_runtime_settings_update_payload, interrupt_turn_control_payload,
        plan_reset_session, ResetSessionPlan,
    };
    use crate::chat::runner::build_runner_stdin_payload;
    use crate::chat::slash_commands::{execute_slash_command, list_slash_commands};
    use crate::chat::state::*;
    use crate::chat::timeline_sink::*;
    use crate::chat::types::*;
    use crate::provider::*;
    use crate::{BACKEND_CLAUDE, BACKEND_CODEX};

    fn test_python_executable() -> String {
        std::env::var("PYTHON").unwrap_or_else(|_| {
            if cfg!(windows) {
                "python".into()
            } else {
                "python3".into()
            }
        })
    }

    fn turn_context() -> AgentTurnContext {
        AgentTurnContext {
            task_id: "task-1".to_string(),
            backend: BACKEND_CLAUDE.to_string(),
            turn_id: "turn-1".to_string(),
            automation_run_id: None,
        }
    }

    #[test]
    fn timeline_runtime_event_maps_to_timeline_input() {
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::Timeline {
                event: json!({
                    "kind": "command",
                    "status": "success",
                    "title": "Run tests",
                    "summary": "17 passed",
                    "payload": { "command": "cargo test" },
                    "sourceId": "cargo-test"
                }),
            },
        )
        .unwrap();

        assert_eq!(input.id, Some("task-1:turn-1:cargo-test".to_string()));
        assert_eq!(input.task_id, "task-1");
        assert_eq!(input.turn_id, Some("turn-1".to_string()));
        assert_eq!(input.backend, BACKEND_CLAUDE);
        assert_eq!(input.kind, "command");
        assert_eq!(input.status, "success");
        assert_eq!(input.title, "Run tests");
        assert_eq!(input.summary, Some("17 passed".to_string()));
        assert_eq!(input.payload, json!({ "command": "cargo test" }));
        assert_eq!(input.created_at, None);
        assert_eq!(input.updated_at, None);
    }

    #[test]
    fn timeline_runtime_event_without_payload_still_maps() {
        // runner 不再产出 display；事件缺 payload 时也要兜底为 null 而非被丢弃。
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::Timeline {
                event: json!({
                    "kind": "reasoning",
                    "status": "running",
                    "title": "思考中"
                }),
            },
        )
        .unwrap();

        assert_eq!(input.kind, "reasoning");
        assert_eq!(input.status, "running");
        assert_eq!(input.title, "思考中");
        assert_eq!(input.payload, JsonValue::Null);
    }

    #[test]
    fn timeline_runtime_event_can_override_turn_and_timestamps_for_history() {
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::Timeline {
                event: json!({
                    "kind": "message",
                    "status": "success",
                    "title": "Assistant",
                    "summary": "历史回复",
                    "payload": {
                        "role": "assistant",
                        "content": "历史回复",
                        "history": true
                    },
                    "sourceId": "codex-history:thread-1:codex-turn-1:item-1",
                    "turnIdOverride": "codex-turn-1",
                    "createdAt": 10_000,
                    "updatedAt": 12_000
                }),
            },
        )
        .unwrap();

        assert_eq!(
            input.id,
            Some("task-1:codex-turn-1:codex-history:thread-1:codex-turn-1:item-1".to_string())
        );
        assert_eq!(input.turn_id, Some("codex-turn-1".to_string()));
        assert_eq!(input.created_at, Some(10_000));
        assert_eq!(input.updated_at, Some(12_000));
    }

    #[test]
    fn runner_error_duplicate_detection_uses_assistant_error_message() {
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::Timeline {
                event: json!({
                    "kind": "message",
                    "status": "error",
                    "title": "Assistant",
                    "payload": {
                        "role": "assistant",
                        "content": "API Error: 503 所有供应商已熔断，无可用渠道."
                    }
                }),
            },
        )
        .unwrap();
        let assistant_text = assistant_error_text(&input);

        assert_eq!(
            assistant_text.as_deref(),
            Some("API Error: 503 所有供应商已熔断，无可用渠道.")
        );
        let assistant_text = assistant_text.unwrap();
        assert!(normalize_timeline_text(
            "Claude Code returned an error result: API Error: 503 所有供应商已熔断，无可用渠道.",
        )
        .contains(&assistant_text));
        assert!(!normalize_timeline_text("无法启动 node 子进程").contains(&assistant_text));
    }

    #[test]
    fn non_object_timeline_event_is_ignored() {
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::Timeline {
                event: json!("not an object"),
            },
        );

        assert!(input.is_none());
    }

    #[test]
    fn non_timeline_runtime_event_is_not_a_timeline_input() {
        let input = timeline_input_from_runtime_event(
            &turn_context(),
            &AgentRuntimeEvent::ToolUse {
                name: "Read".to_string(),
                input: json!({ "file": "README.md" }),
            },
        );

        assert!(input.is_none());
    }

    #[test]
    fn agent_interaction_response_payload_uses_runner_id_field() {
        assert_eq!(
            agent_interaction_response_payload(
                "ask-1".to_string(),
                agent_interaction_contract::plan_approval_interaction_kind().to_string(),
                json!({
                    "cancelled": false,
                    "answers": {
                        "approve-plan": {
                            "questionId": "approve-plan",
                            "value": "yes"
                        }
                    }
                }),
            ),
            json!({
                "type": runner_interaction_response_control_type(),
                "id": "ask-1",
                "kind": agent_interaction_contract::plan_approval_interaction_kind(),
                "result": {
                    "cancelled": false,
                    "answers": {
                        "approve-plan": {
                            "questionId": "approve-plan",
                            "value": "yes"
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn interrupt_turn_control_payload_uses_runner_control_message() {
        assert_eq!(
            interrupt_turn_control_payload(),
            json!({ "type": runner_interrupt_turn_control_type() })
        );
    }

    #[test]
    fn composer_runtime_settings_update_payload_requires_existing_changed_settings() {
        let mut previous = default_composer("task-1");
        previous.permission = "ask".to_string();
        previous.model = "gpt-5".to_string();
        let mut next = previous.clone();

        assert_eq!(composer_runtime_settings_update_payload(None, &next), None);
        assert_eq!(
            composer_runtime_settings_update_payload(Some(&previous), &next),
            None
        );

        next.permission = "readonly".to_string();
        assert_eq!(
            composer_runtime_settings_update_payload(Some(&previous), &next),
            Some(json!({
                "type": runner_settings_update_control_type(),
                "permission": "readonly",
            }))
        );

        next.permission = "invalid".to_string();
        assert_eq!(
            composer_runtime_settings_update_payload(Some(&previous), &next),
            None
        );

        next.permission = previous.permission.clone();
        next.model = "gpt-5.1".to_string();
        assert_eq!(
            composer_runtime_settings_update_payload(Some(&previous), &next),
            Some(json!({
                "type": runner_settings_update_control_type(),
                "model": "gpt-5.1",
            }))
        );

        next.permission = "full".to_string();
        next.model = "  gpt-5.2  ".to_string();
        assert_eq!(
            composer_runtime_settings_update_payload(Some(&previous), &next),
            Some(json!({
                "type": runner_settings_update_control_type(),
                "permission": "full",
                "model": "gpt-5.2",
            }))
        );
    }

    #[test]
    fn clearing_runtime_state_removes_persisted_runtime_state() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        let running_turn = RunningTurn {
            turn_id: "turn-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            native_approval_pause: None,
        };
        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &running_turn,
            "running",
            None,
            None,
        )
        .unwrap();
        clear_runtime_state(&conn, "task-1").unwrap();

        assert!(load_any_runtime_state(&conn, "task-1").unwrap().is_none());
    }

    #[test]
    fn pending_turns_roundtrip_through_persistent_queue() {
        let conn = resume_conn_with_task();
        let turn = PendingChatTurn {
            workflow: Some(ChatWorkflow::Automation {
                automation_run_id: "run-1".to_string(),
            }),
            guide_id: Some("guide-1".to_string()),
            ..pending_turn("queued")
        };

        persist_pending_turn(&conn, "task-1", &turn).unwrap();

        assert_eq!(count_pending_turns(&conn, "task-1").unwrap(), 1);
        let restored = take_next_persisted_pending_turn(&conn, "task-1")
            .unwrap()
            .expect("persisted pending turn");
        assert_eq!(restored.content, "content queued");
        assert_eq!(restored.turn_id, turn.turn_id);
        assert_eq!(restored.guide_id.as_deref(), Some("guide-1"));
        let Some(ChatWorkflow::Automation { automation_run_id }) = restored.workflow else {
            panic!("unexpected workflow");
        };
        assert_eq!(automation_run_id, "run-1");
        assert_eq!(count_pending_turns(&conn, "task-1").unwrap(), 0);
    }

    #[test]
    fn clear_persisted_pending_turns_returns_guide_ids() {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        conn.execute(
            r#"INSERT INTO tasks (id, session_id) VALUES ('task-1', 'session-1')"#,
            [],
        )
        .unwrap();
        persist_pending_turn(
            &conn,
            "task-1",
            &PendingChatTurn {
                guide_id: Some("guide-1".to_string()),
                ..pending_turn("queued-1")
            },
        )
        .unwrap();
        persist_pending_turn(
            &conn,
            "task-1",
            &PendingChatTurn {
                guide_id: None,
                ..pending_turn("queued-2")
            },
        )
        .unwrap();

        let guide_ids = clear_persisted_pending_turns(&conn, "task-1").unwrap();

        assert_eq!(guide_ids, vec!["guide-1".to_string()]);
        assert_eq!(count_pending_turns(&conn, "task-1").unwrap(), 0);
    }

    #[test]
    fn stdin_delivery_attributes_capture_forwarded_and_error_states() {
        let mut forwarded = BTreeMap::new();
        attach_stdin_delivery(&mut forwarded, &Ok(true));
        assert_eq!(
            forwarded.get("stdinForwarded").map(String::as_str),
            Some("true")
        );
        assert!(forwarded.get("stdinError").is_none());

        let mut missing = BTreeMap::new();
        attach_stdin_delivery(&mut missing, &Ok(false));
        assert_eq!(
            missing.get("stdinForwarded").map(String::as_str),
            Some("false")
        );
        assert!(missing.get("stdinError").is_none());

        let mut failed = BTreeMap::new();
        attach_stdin_delivery(&mut failed, &Err("broken pipe".to_string()));
        assert_eq!(
            failed.get("stdinForwarded").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            failed.get("stdinError").map(String::as_str),
            Some("broken pipe")
        );
    }

    #[test]
    fn empty_lilia_workflows_do_not_persist_user_message() {
        let workflows = vec![
            ChatWorkflow::LiliaReview {
                target: LiliaReviewTarget::UncommittedChanges,
                instructions: None,
                delivery: Some("inline".to_string()),
            },
            ChatWorkflow::LiliaFixSuggestion {
                target: LiliaReviewTarget::UncommittedChanges,
                instructions: None,
                mode: Some("suggest".to_string()),
            },
            ChatWorkflow::LiliaBatchApply {
                source_turn_id: "turn-source".to_string(),
                source_kind: "fix_suggestion".to_string(),
                source_summary: "建议修复权限边界".to_string(),
                instructions: None,
            },
            ChatWorkflow::LiliaTaskWorkflow {
                kind: "frontend".to_string(),
                instructions: None,
            },
            ChatWorkflow::LiliaGoal {
                action: "set".to_string(),
                objective: Some("完成 Thread Goal 接入".to_string()),
                status: Some("active".to_string()),
                token_budget: None,
            },
            ChatWorkflow::LiliaCompact,
            ChatWorkflow::LiliaBackgroundTerminalsClean,
            ChatWorkflow::LiliaMemoryMode {
                mode: "enabled".to_string(),
            },
            ChatWorkflow::LiliaMemoryReset,
            ChatWorkflow::LiliaConfigDiagnostics {
                include_layers: Some(true),
            },
            ChatWorkflow::SlashCommand {
                command_id: "native:help".to_string(),
                source: "native".to_string(),
                arguments: BTreeMap::new(),
            },
        ];

        for workflow in workflows {
            let workflow = Some(workflow);
            assert!(!should_persist_user_message("", &workflow, &None));
            assert!(!should_persist_user_message("  ", &workflow, &None));
            assert!(should_persist_user_message("补充说明", &workflow, &None));
        }
        assert!(should_persist_user_message("", &None, &None));
    }

    #[test]
    fn chat_workflow_serializes_struct_variant_fields_as_camel_case() {
        let goal = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "lilia_goal",
            "action": "set",
            "objective": "完成接口接入",
            "status": "active",
            "tokenBudget": 100,
        }))
        .unwrap();
        let ChatWorkflow::LiliaGoal { token_budget, .. } = &goal else {
            panic!("unexpected workflow: {goal:?}");
        };
        assert_eq!(*token_budget, Some(100));
        let goal_json = serde_json::to_value(&goal).unwrap();
        assert_eq!(goal_json["tokenBudget"], json!(100));
        assert!(goal_json.get("token_budget").is_none());

        let fix = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "lilia_fix_suggestion",
            "target": { "type": "baseBranch", "branch": "main" },
            "instructions": "只给建议",
            "mode": "suggest",
        }))
        .unwrap();
        let ChatWorkflow::LiliaFixSuggestion { mode, .. } = &fix else {
            panic!("unexpected workflow: {fix:?}");
        };
        assert_eq!(mode.as_deref(), Some("suggest"));
        let fix_json = serde_json::to_value(&fix).unwrap();
        assert_eq!(fix_json["type"], json!("lilia_fix_suggestion"));
        assert_eq!(fix_json["target"]["branch"], json!("main"));

        let batch_apply = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "lilia_batch_apply",
            "sourceTurnId": "turn-source",
            "sourceKind": "fix_suggestion",
            "sourceSummary": "建议修复权限边界",
            "instructions": "应用最小改动",
        }))
        .unwrap();
        let ChatWorkflow::LiliaBatchApply {
            source_turn_id,
            source_kind,
            source_summary,
            instructions,
        } = &batch_apply
        else {
            panic!("unexpected workflow: {batch_apply:?}");
        };
        assert_eq!(source_turn_id, "turn-source");
        assert_eq!(source_kind, "fix_suggestion");
        assert_eq!(source_summary, "建议修复权限边界");
        assert_eq!(instructions.as_deref(), Some("应用最小改动"));
        let batch_apply_json = serde_json::to_value(&batch_apply).unwrap();
        assert_eq!(batch_apply_json["sourceTurnId"], json!("turn-source"));
        assert_eq!(batch_apply_json["sourceKind"], json!("fix_suggestion"));
        assert_eq!(batch_apply_json["sourceSummary"], json!("建议修复权限边界"));
        assert!(batch_apply_json.get("source_turn_id").is_none());

        let task_workflow = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "lilia_task_workflow",
            "kind": "frontend",
            "instructions": "保持现有交互",
        }))
        .unwrap();
        let ChatWorkflow::LiliaTaskWorkflow { kind, instructions } = &task_workflow else {
            panic!("unexpected workflow: {task_workflow:?}");
        };
        assert_eq!(kind, "frontend");
        assert_eq!(instructions.as_deref(), Some("保持现有交互"));
        let task_workflow_json = serde_json::to_value(&task_workflow).unwrap();
        assert_eq!(task_workflow_json["type"], json!("lilia_task_workflow"));
        assert_eq!(task_workflow_json["kind"], json!("frontend"));
        assert_eq!(task_workflow_json["instructions"], json!("保持现有交互"));

        assert!(serde_json::from_value::<ChatWorkflow>(json!({
            "type": "session_fork",
            "excludeTurns": false,
        }))
        .is_err());
        assert!(serde_json::from_value::<ChatWorkflow>(json!({
            "type": "session_management",
            "action": "list",
        }))
        .is_err());
        assert!(serde_json::from_value::<ChatWorkflow>(json!({
            "type": "runtime_settings",
            "action": "diagnose",
        }))
        .is_err());
        assert!(serde_json::from_value::<ChatWorkflow>(json!({
            "type": "sandbox_diagnostics",
            "includeDetails": true,
        }))
        .is_err());

        let fork = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "session_fork",
            "excludeTurns": false,
            "sourceTurnId": "turn-source",
            "mode": "fork",
        }))
        .unwrap();
        let ChatRuntimeCommand::SessionFork {
            exclude_turns,
            source_turn_id,
            mode,
        } = &fork
        else {
            panic!("unexpected runtime command: {fork:?}");
        };
        assert_eq!(*exclude_turns, Some(false));
        assert_eq!(source_turn_id.as_deref(), Some("turn-source"));
        assert_eq!(mode.as_deref(), Some("fork"));
        let fork_json = serde_json::to_value(&fork).unwrap();
        assert_eq!(fork_json["excludeTurns"], json!(false));
        assert_eq!(fork_json["sourceTurnId"], json!("turn-source"));
        assert_eq!(fork_json["mode"], json!("fork"));
        assert!(fork_json.get("exclude_turns").is_none());
        assert!(fork_json.get("source_turn_id").is_none());

        let session_management = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "session_management",
            "action": "tag",
            "sessionId": "thread-1",
            "title": "???",
            "tag": "release",
            "archived": true,
            "limit": 20,
            "cursor": "cursor-1",
            "searchTerm": "bug",
            "includeSystemMessages": true,
        }))
        .unwrap();
        let ChatRuntimeCommand::SessionManagement {
            action,
            session_id,
            title,
            tag,
            archived,
            limit,
            cursor,
            search_term,
            include_system_messages,
        } = &session_management
        else {
            panic!("unexpected runtime command: {session_management:?}");
        };
        assert_eq!(action, "tag");
        assert_eq!(session_id.as_deref(), Some("thread-1"));
        assert_eq!(title.as_deref(), Some("???"));
        assert_eq!(tag.as_deref(), Some("release"));
        assert_eq!(*archived, Some(true));
        assert_eq!(*limit, Some(20));
        assert_eq!(cursor.as_deref(), Some("cursor-1"));
        assert_eq!(search_term.as_deref(), Some("bug"));
        assert_eq!(*include_system_messages, Some(true));
        let session_management_json = serde_json::to_value(&session_management).unwrap();
        assert_eq!(session_management_json["type"], json!("session_management"));
        assert_eq!(session_management_json["sessionId"], json!("thread-1"));
        assert_eq!(session_management_json["tag"], json!("release"));
        assert_eq!(session_management_json["archived"], json!(true));
        assert_eq!(session_management_json["searchTerm"], json!("bug"));
        assert_eq!(
            session_management_json["includeSystemMessages"],
            json!(true)
        );
        assert!(session_management_json.get("session_id").is_none());
        assert!(session_management_json.get("search_term").is_none());

        let diagnostics = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "lilia_config_diagnostics",
            "includeLayers": false,
        }))
        .unwrap();
        let ChatWorkflow::LiliaConfigDiagnostics { include_layers } = &diagnostics else {
            panic!("unexpected workflow: {diagnostics:?}");
        };
        assert_eq!(*include_layers, Some(false));
        let diagnostics_json = serde_json::to_value(&diagnostics).unwrap();
        assert_eq!(diagnostics_json["includeLayers"], json!(false));
        assert!(diagnostics_json.get("include_layers").is_none());

        let provider_settings = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "runtime_settings",
            "action": "update"
        }))
        .unwrap();
        let ChatRuntimeCommand::RuntimeSettings { action } = &provider_settings else {
            panic!("unexpected runtime command: {provider_settings:?}");
        };
        assert_eq!(action, "update");
        let sandbox_diagnostics = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "sandbox_diagnostics",
            "includeDetails": true
        }))
        .unwrap();
        let ChatRuntimeCommand::SandboxDiagnostics { include_details } = &sandbox_diagnostics
        else {
            panic!("unexpected runtime command: {sandbox_diagnostics:?}");
        };
        assert_eq!(*include_details, Some(true));
        let sandbox_diagnostics_json = serde_json::to_value(&sandbox_diagnostics).unwrap();
        assert_eq!(
            sandbox_diagnostics_json["type"],
            json!("sandbox_diagnostics")
        );
        assert_eq!(sandbox_diagnostics_json["includeDetails"], json!(true));
        assert!(sandbox_diagnostics_json.get("include_details").is_none());
        let runtime_options = serde_json::from_value::<ProviderRuntimeOptions>(json!({
            "common": { "model": "gpt-5.5", "permission": "ask" },
            "provider": {
                "codex": {
                    "profile": "deep",
                    "reasoningEffort": "high",
                    "runtimeWorkspaceRoots": ["C:/repo"],
                    "persistExtendedHistory": true,
                    "environments": [{ "id": "env-1" }],
                    "experimentalRawEvents": true,
                    "responsesApiClientMetadata": { "surface": "lilia" }
                },
                "claude": {
                    "allowedTools": ["Read"],
                    "disallowedTools": ["Bash"],
                    "additionalDirectories": ["D:/shared"],
                    "maxTurns": 4,
                    "maxBudgetUsd": 1.5,
                    "tools": { "type": "preset", "preset": "claude_code" },
                    "permissionPromptToolName": "mcp__lilia__permission_prompt",
                    "settings": { "model": "claude-opus-4-5" },
                    "managedSettings": { "sandbox": { "enabled": true } },
                    "settingSources": ["user", "project"],
                    "sandbox": { "enabled": true },
                    "outputFormat": { "type": "json" },
                    "includeHookEvents": true,
                    "forwardSubagentText": true,
                    "agentProgressSummaries": true,
                    "continue": true,
                    "resumeSessionAt": "message-uuid",
                    "sessionId": "00000000-0000-4000-8000-000000000001",
                    "abortAfterMs": 3000,
                    "sessionStore": { "explicit": true }
                }
            }
        }))
        .unwrap();
        let provider = runtime_options.provider.as_ref().expect("provider options");
        assert_eq!(
            provider
                .codex
                .as_ref()
                .and_then(|value| value.reasoning_effort.as_deref()),
            Some("high")
        );
        assert_eq!(
            provider
                .claude
                .as_ref()
                .and_then(|value| value.additional_directories.as_ref())
                .unwrap(),
            &vec!["D:/shared".to_string()]
        );
        assert_eq!(
            provider
                .claude
                .as_ref()
                .and_then(|value| value.continue_session),
            Some(true)
        );
        let provider_settings_json = serde_json::to_value(&provider_settings).unwrap();
        assert_eq!(provider_settings_json["type"], json!("runtime_settings"));
        assert!(provider_settings_json.get("common").is_none());
        assert!(provider_settings_json.get("runtimeOptions").is_none());
        let provider_settings_json = serde_json::to_value(&runtime_options).unwrap();
        assert_eq!(
            provider_settings_json["provider"]["codex"]["reasoningEffort"],
            json!("high")
        );
        assert_eq!(
            provider_settings_json["provider"]["codex"]["runtimeWorkspaceRoots"],
            json!(["C:/repo"])
        );
        assert_eq!(
            provider_settings_json["provider"]["codex"]["experimentalRawEvents"],
            json!(true)
        );
        assert_eq!(
            provider_settings_json["provider"]["codex"]["responsesApiClientMetadata"],
            json!({ "surface": "lilia" })
        );
        assert_eq!(
            provider_settings_json["provider"]["claude"]["maxBudgetUsd"],
            json!(1.5)
        );
        assert_eq!(
            provider_settings_json["provider"]["claude"]["continue"],
            json!(true)
        );
        assert_eq!(
            provider_settings_json["provider"]["claude"]["abortAfterMs"],
            json!(3000)
        );
        assert!(provider_settings_json["provider"]["claude"]
            .get("continue_session")
            .is_none());
        assert!(provider_settings_json["provider"]["codex"]
            .get("reasoning_effort")
            .is_none());

        let slash = serde_json::from_value::<ChatWorkflow>(json!({
            "type": "slash_command",
            "commandId": "native:help",
            "source": "native",
            "arguments": {},
        }))
        .unwrap();
        let ChatWorkflow::SlashCommand {
            command_id,
            source,
            arguments,
        } = &slash
        else {
            panic!("unexpected workflow: {slash:?}");
        };
        assert_eq!(command_id, "native:help");
        assert_eq!(source, "native");
        assert!(arguments.is_empty());
        let slash_json = serde_json::to_value(&slash).unwrap();
        assert_eq!(slash_json["commandId"], json!("native:help"));
        assert!(slash_json.get("command_id").is_none());
    }

    #[test]
    fn slash_commands_discover_native_and_project_commands() {
        let temp = std::env::temp_dir().join(format!(
            "lilia-slash-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let command_dir = temp.join(".lilia").join("commands");
        std::fs::create_dir_all(&command_dir).unwrap();
        std::fs::write(
            command_dir.join("release.md"),
            "# 生成发布检查\n\n请完成发布前检查并整理风险项。",
        )
        .unwrap();
        let cwd = temp.to_string_lossy().to_string();

        let help = list_slash_commands(&cwd, "help", 12);
        assert_eq!(help[0].command.id, "native:help");
        assert_eq!(help[0].matched_by, "name");

        let project = list_slash_commands(&cwd, "release", 12);
        assert_eq!(project[0].command.id, "project:release");
        assert_eq!(project[0].command.title, "生成发布检查");
        assert_eq!(project[0].command.source, ChatSlashCommandSource::Project);
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn slash_command_execution_returns_timeline_ready_output() {
        let temp = std::env::temp_dir().join(format!(
            "lilia-slash-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let command_dir = temp.join(".lilia").join("commands");
        std::fs::create_dir_all(&command_dir).unwrap();
        std::fs::write(
            command_dir.join("release.md"),
            "# 生成发布检查\n\n请完成发布前检查并整理风险项。",
        )
        .unwrap();
        let cwd = temp.to_string_lossy().to_string();

        let native = execute_slash_command("native:status", &cwd, BACKEND_CODEX).unwrap();
        assert_eq!(native.name, "status");
        assert!(native.result.contains("当前后端：codex"));

        let project = execute_slash_command("project:release", &cwd, BACKEND_CODEX).unwrap();
        assert_eq!(project.name, "release");
        assert!(project.result.contains("请完成发布前检查"));
        assert!(project.result.contains("release.md"));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn runner_stdin_payload_keeps_ui_to_runner_contract() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: true,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let workflow = ChatWorkflow::LiliaFixSuggestion {
            target: LiliaReviewTarget::UncommittedChanges,
            instructions: Some("重点看全链路".to_string()),
            mode: Some("suggest".to_string()),
        };

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "重点看全链路",
            &[],
            &[],
            Some(&workflow),
            None,
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["cwd"], json!("C:\\Files\\workspace\\Lilia"));
        assert_eq!(payload["turn"]["prompt"], json!("重点看全链路"));
        assert_eq!(payload["turn"]["resumeSessionId"], json!("thread-1"));
        assert_eq!(payload["turn"]["planMode"], json!(true));
        assert_eq!(payload["turn"]["permission"], json!("ask"));
        assert_eq!(payload["workflow"]["type"], json!("lilia_fix_suggestion"));
        assert_eq!(payload["workflow"]["mode"], json!("suggest"));
        assert_eq!(payload["workflow"]["instructions"], json!("重点看全链路"));
        assert_eq!(payload["extensions"]["mcpServers"], json!([]));
        assert_eq!(payload["turn"]["model"], json!("gpt-5.5"));
        assert!(payload.get("cwd").is_none());
        assert!(payload.get("prompt").is_none());
        assert!(payload.get("model").is_none());
        assert!(payload.get("resumeSessionId").is_none());
    }

    #[test]
    fn runner_stdin_payload_keeps_lilia_batch_apply_workflow() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let workflow = ChatWorkflow::LiliaBatchApply {
            source_turn_id: "turn-source".to_string(),
            source_kind: "fix_suggestion".to_string(),
            source_summary: "建议修复权限边界".to_string(),
            instructions: None,
        };

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "",
            &[],
            &[],
            Some(&workflow),
            None,
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!(""));
        assert_eq!(payload["workflow"]["type"], json!("lilia_batch_apply"));
        assert_eq!(payload["workflow"]["sourceTurnId"], json!("turn-source"));
        assert_eq!(payload["workflow"]["sourceKind"], json!("fix_suggestion"));
        assert_eq!(
            payload["workflow"]["sourceSummary"],
            json!("建议修复权限边界")
        );
    }

    #[test]
    fn runner_stdin_payload_keeps_lilia_task_workflow() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let workflow = ChatWorkflow::LiliaTaskWorkflow {
            kind: "frontend".to_string(),
            instructions: Some("调整按钮布局".to_string()),
        };

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\\\Files\\\\workspace\\\\Lilia",
            "调整按钮布局",
            &[],
            &[],
            Some(&workflow),
            None,
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!("调整按钮布局"));
        assert_eq!(payload["workflow"]["type"], json!("lilia_task_workflow"));
        assert_eq!(payload["workflow"]["kind"], json!("frontend"));
        assert_eq!(payload["workflow"]["instructions"], json!("调整按钮布局"));
    }

    #[test]
    fn runner_stdin_payload_keeps_runtime_settings_runtime_command() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let runtime_command = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "runtime_settings",
            "action": "update"
        }))
        .unwrap();
        let runtime_options = json!({
            "common": { "model": "gpt-5.6", "permission": "readonly" },
            "provider": {
                "codex": {
                    "reasoningEffort": "high",
                    "runtimeWorkspaceRoots": ["C:/repo"],
                    "persistExtendedHistory": true
                }
            }
        });

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "",
            &[],
            &[],
            None,
            Some(&runtime_command),
            Some(&runtime_options),
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!(""));
        assert!(payload["workflow"].is_null());
        assert!(payload.get("protocol").is_none());
        assert_eq!(payload["runtimeCommand"]["type"], json!("runtime_settings"));
        assert_eq!(payload["runtimeCommand"]["action"], json!("update"));
        assert!(payload["runtimeCommand"].get("common").is_none());
        assert!(payload["runtimeCommand"].get("runtimeOptions").is_none());
        assert_eq!(
            payload["runtimeOptions"]["common"]["permission"],
            json!("readonly")
        );
        assert_eq!(
            payload["runtimeOptions"]["provider"]["codex"]["reasoningEffort"],
            json!("high")
        );
        assert_eq!(
            payload["runtimeOptions"]["provider"]["codex"]["runtimeWorkspaceRoots"],
            json!(["C:/repo"])
        );
        assert_eq!(
            payload["runtimeOptions"]["provider"]["codex"]["persistExtendedHistory"],
            json!(true)
        );
    }

    #[test]
    fn runner_stdin_payload_keeps_session_management_runtime_command() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let runtime_command = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "session_management",
            "action": "rename",
            "sessionId": "thread-1",
            "title": "新标题",
            "limit": 20,
            "cursor": "cursor-1",
            "searchTerm": "bug",
            "includeSystemMessages": true,
        }))
        .unwrap();

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "",
            &[],
            &[],
            None,
            Some(&runtime_command),
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!(""));
        assert!(payload["workflow"].is_null());
        assert!(payload.get("protocol").is_none());
        assert_eq!(
            payload["runtimeCommand"]["type"],
            json!("session_management")
        );
        assert_eq!(payload["runtimeCommand"]["action"], json!("rename"));
        assert_eq!(payload["runtimeCommand"]["sessionId"], json!("thread-1"));
        assert_eq!(payload["runtimeCommand"]["title"], json!("新标题"));
        assert_eq!(payload["runtimeCommand"]["searchTerm"], json!("bug"));
        assert_eq!(
            payload["runtimeCommand"]["includeSystemMessages"],
            json!(true)
        );
        assert!(payload["runtimeCommand"].get("session_id").is_none());
    }

    #[test]
    fn runner_stdin_payload_keeps_sandbox_diagnostics_runtime_command() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let runtime_command = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "sandbox_diagnostics",
            "includeDetails": true,
        }))
        .unwrap();

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "",
            &[],
            &[],
            None,
            Some(&runtime_command),
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!(""));
        assert!(payload["workflow"].is_null());
        assert_eq!(
            payload["runtimeCommand"]["type"],
            json!("sandbox_diagnostics")
        );
        assert_eq!(payload["runtimeCommand"]["includeDetails"], json!(true));
        assert!(payload["runtimeCommand"].get("include_details").is_none());
    }

    #[test]
    fn runner_stdin_payload_keeps_process_session_runtime_command() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CODEX.to_string(),
            model: "gpt-5.5".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: CodexComposerSettings::default(),
        };
        let runtime_command = serde_json::from_value::<ChatRuntimeCommand>(json!({
            "type": "process_session",
            "action": "spawn",
            "command": "npm test -- --watch=false",
            "cwd": "C:/repo",
            "env": { "CI": "1" },
            "tty": true,
            "rows": 24,
            "cols": 100,
            "permissionProfile": ":workspace"
        }))
        .unwrap();

        let payload = build_runner_stdin_payload(
            BACKEND_CODEX,
            "C:\\Files\\workspace\\Lilia",
            "",
            &[],
            &[],
            None,
            Some(&runtime_command),
            None,
            &composer,
            Some("thread-1"),
            &json!({ "mcpServers": [], "warnings": [] }),
        );

        assert_eq!(payload["backend"], json!("codex"));
        assert_eq!(payload["turn"]["prompt"], json!(""));
        assert!(payload["workflow"].is_null());
        assert_eq!(payload["runtimeCommand"]["type"], json!("process_session"));
        assert_eq!(payload["runtimeCommand"]["action"], json!("spawn"));
        assert_eq!(
            payload["runtimeCommand"]["command"],
            json!("npm test -- --watch=false")
        );
        assert_eq!(payload["runtimeCommand"]["env"]["CI"], json!("1"));
        assert_eq!(
            payload["runtimeCommand"]["permissionProfile"],
            json!(":workspace")
        );
        assert!(payload["runtimeCommand"].get("process_id").is_none());
    }

    #[test]
    fn chat_message_ids_do_not_reset_to_counter_values() {
        let first = new_chat_message_id();
        let second = new_chat_message_id();

        assert!(first.starts_with("u-"));
        assert!(second.starts_with("u-"));
        assert_ne!(first, second);
        assert_ne!(first, "u-0");
    }

    #[test]
    fn active_backend_normalizes_unknown_values_to_claude() {
        assert_eq!(normalize_backend(BACKEND_CLAUDE), BACKEND_CLAUDE);
        assert_eq!(normalize_backend(BACKEND_CODEX), BACKEND_CODEX);
        assert_eq!(normalize_backend(""), BACKEND_CLAUDE);
        assert_eq!(normalize_backend("unknown"), BACKEND_CLAUDE);
    }

    #[test]
    fn codex_cli_version_parser_reads_codex_cli_output() {
        assert_eq!(
            parse_codex_cli_version("codex-cli 0.128.0"),
            Some((0, 128, 0))
        );
        assert_eq!(
            parse_codex_cli_version("codex-cli 0.130.0-alpha.5"),
            Some((0, 130, 0))
        );
        assert_eq!(
            parse_codex_cli_version("codex-cli 0.136.0-alpha.2"),
            Some((0, 136, 0))
        );
    }

    #[test]
    fn codex_cli_probe_skips_failed_managed_candidate() {
        let candidates = vec![
            "C:\\Users\\me\\.lilia\\runtime\\codex\\bin\\codex.cmd".to_string(),
            "C:\\Users\\me\\.lilia\\runtime\\codex\\bin\\codex.exe".to_string(),
        ];
        let status = build_codex_app_server_probe_status_with(&candidates, |program, args| {
            if program.ends_with("codex.cmd") {
                return Err("command not found".to_string());
            }
            match args {
                ["--version"] => Ok("codex-cli 0.136.0".to_string()),
                ["app-server", "--help"] => {
                    Ok("Usage: codex app-server [OPTIONS] [COMMAND]".to_string())
                }
                _ => Err("unexpected args".to_string()),
            }
        });

        assert_eq!(
            status.path.as_deref(),
            Some("C:\\Users\\me\\.lilia\\runtime\\codex\\bin\\codex.exe")
        );
        assert!(status.public.supports_required_protocol);
        assert_eq!(status.public.version.as_deref(), Some("codex-cli 0.136.0"));
    }

    #[test]
    fn codex_send_block_reason_mentions_cli_and_responses_support() {
        let reason = codex_send_block_reason(&CodexAppServerStatus {
            version: Some("codex-cli 0.125.0".to_string()),
            install_path: None,
            managed: false,
            available: true,
            supports_required_protocol: false,
            failure_kind: Some("experimentalApiUnsupported".to_string()),
            issues: vec!["当前 codex CLI 版本过低，需要 0.136.0 或更新版本。".to_string()],
            latest_version: None,
            update_available: false,
            release_notes: Vec::new(),
            update_error: None,
            update_state: "idle".to_string(),
            prepared_version: None,
            update_progress_percent: None,
        })
        .unwrap();

        assert!(reason.contains("当前 codex CLI 版本过低"));
        assert!(reason.contains("0.136.0"));
        assert!(!reason.contains("OpenAI Responses API"));

        let reason = codex_send_block_reason(&CodexAppServerStatus {
            version: Some("codex-cli 0.136.0".to_string()),
            install_path: None,
            managed: false,
            available: true,
            supports_required_protocol: false,
            failure_kind: Some("providerIncompatible".to_string()),
            issues: vec!["当前上游 provider 不兼容 Codex。".to_string()],
            latest_version: None,
            update_available: false,
            release_notes: Vec::new(),
            update_error: None,
            update_state: "idle".to_string(),
            prepared_version: None,
            update_progress_percent: None,
        })
        .unwrap();
        assert!(reason.contains("OpenAI Responses API"));

        assert!(codex_send_block_reason(&CodexAppServerStatus {
            version: Some("codex-cli 0.136.0".to_string()),
            install_path: None,
            managed: false,
            available: true,
            supports_required_protocol: true,
            failure_kind: None,
            issues: Vec::new(),
            latest_version: None,
            update_available: false,
            release_notes: Vec::new(),
            update_error: None,
            update_state: "idle".to_string(),
            prepared_version: None,
            update_progress_percent: None,
        })
        .is_none());
    }

    #[test]
    fn composer_uses_active_backend_and_matching_default_model() {
        let composer = ChatComposerState {
            task_id: "stale-task".to_string(),
            backend: BACKEND_CLAUDE.to_string(),
            model: "claude-sonnet-4-6".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: true,
            goal_mode: false,
            permission: "readonly".to_string(),
            codex_settings: Default::default(),
        };

        let normalized = normalize_composer_for_backend(composer, "task-1", BACKEND_CODEX);

        assert_eq!(normalized.task_id, "task-1");
        assert_eq!(normalized.backend, BACKEND_CODEX);
        assert_eq!(normalized.model, "gpt-5.5");
        assert!(normalized.plan_mode);
        assert_eq!(normalized.permission, "readonly");
    }

    #[test]
    fn composer_keeps_model_when_it_belongs_to_active_backend() {
        let composer = ChatComposerState {
            task_id: "task-1".to_string(),
            backend: BACKEND_CLAUDE.to_string(),
            model: "gpt-5.4-mini".to_string(),
            model_selection_mode: "auto".to_string(),
            reasoning_effort: None,
            plan_mode: false,
            goal_mode: false,
            permission: "ask".to_string(),
            codex_settings: Default::default(),
        };

        let normalized = normalize_composer_for_backend(composer, "task-1", BACKEND_CODEX);

        assert_eq!(normalized.backend, BACKEND_CODEX);
        assert_eq!(normalized.model, "gpt-5.4-mini");
    }

    #[test]
    fn composer_normalizes_model_selection_and_reasoning_effort_for_backend() {
        let codex = normalize_composer_for_backend(
            ChatComposerState {
                task_id: "task-1".to_string(),
                backend: BACKEND_CODEX.to_string(),
                model: "gpt-5.4-mini".to_string(),
                model_selection_mode: "manual".to_string(),
                reasoning_effort: Some("max".to_string()),
                plan_mode: false,
                goal_mode: false,
                permission: "ask".to_string(),
                codex_settings: Default::default(),
            },
            "task-1",
            BACKEND_CODEX,
        );
        assert_eq!(codex.model_selection_mode, "manual");
        assert_eq!(codex.reasoning_effort.as_deref(), Some("xhigh"));

        let claude = normalize_composer_for_backend(
            ChatComposerState {
                task_id: "task-1".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                model: "claude-opus-4-7".to_string(),
                model_selection_mode: "unexpected".to_string(),
                reasoning_effort: Some("max".to_string()),
                plan_mode: false,
                goal_mode: false,
                permission: "ask".to_string(),
                codex_settings: Default::default(),
            },
            "task-1",
            BACKEND_CLAUDE,
        );
        assert_eq!(claude.model_selection_mode, "auto");
        assert_eq!(claude.reasoning_effort.as_deref(), Some("max"));
    }

    fn pending_turn(id: &str) -> PendingChatTurn {
        PendingChatTurn {
            content: format!("content {id}"),
            composer: default_composer("task-1"),
            project_cwd: "D:\\PROJECT\\workspace\\Lilia".to_string(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            runtime_command: None,
            runtime_options: None,
            message: ChatMessage {
                id: format!("u-{id}"),
                task_id: "task-1".to_string(),
                role: "user".to_string(),
                content: format!("content {id}"),
                attachments: Vec::new(),
                conversation_references: Vec::new(),
                created_at: 100,
            },
            turn_id: format!("turn-{id}"),
            guide_id: None,
        }
    }

    #[test]
    fn clearing_pending_turns_removes_executable_queue_and_returns_guide_ids() {
        let store = ChatStore::default();
        {
            let mut pending = store.pending_turns.lock().unwrap();
            pending
                .entry("task-1".to_string())
                .or_default()
                .push_back(PendingChatTurn {
                    guide_id: Some("guide-1".to_string()),
                    ..pending_turn("queued")
                });
        }

        assert_eq!(
            clear_pending_turns(&store, "task-1"),
            vec!["guide-1".to_string()]
        );
        assert!(store.pending_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn prepare_running_turn_stop_for_interrupt_marks_interrupted_and_clears_queue() {
        let store = ChatStore::default();
        store.running_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-running".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(PendingChatTurn {
                guide_id: Some("guide-queued".to_string()),
                ..pending_turn("queued")
            });

        let prepared = prepare_running_turn_stop(&store, "task-1", true, false).unwrap();

        assert_eq!(prepared.guide_ids, vec!["guide-queued".to_string()]);
        assert!(store.pending_turns.lock().unwrap().get("task-1").is_none());
        assert_eq!(
            store
                .interrupted_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|turn| turn.turn_id.as_str()),
            Some("turn-running")
        );
        assert!(store.reset_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn prepare_running_turn_stop_for_reset_marks_reset_without_interrupted_event() {
        let store = ChatStore::default();
        store.running_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-reset".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );

        let prepared = prepare_running_turn_stop(&store, "task-1", false, true).unwrap();

        assert!(prepared.guide_ids.is_empty());
        assert!(store
            .interrupted_turns
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
        assert_eq!(
            store
                .reset_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|turn| turn.turn_id.as_str()),
            Some("turn-reset")
        );
    }

    #[test]
    fn reset_marker_is_read_without_consuming_until_turn_finish() {
        let store = ChatStore::default();
        store.reset_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-reset".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );

        assert!(is_turn_marked_reset(
            &store,
            "task-1",
            "turn-reset",
            BACKEND_CLAUDE
        ));
        assert!(store.reset_turns.lock().unwrap().get("task-1").is_some());

        let (interrupted, reset) =
            take_turn_stop_marks(&store, "task-1", "turn-reset", BACKEND_CLAUDE);
        assert!(!interrupted);
        assert!(reset);
        assert!(store.reset_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn finish_running_turn_handles_clears_handles_and_consumes_matching_marks() {
        let store = ChatStore::default();
        store
            .running_process_sessions
            .lock()
            .unwrap()
            .insert("task-1".to_string(), "proc-1".to_string());
        store.running_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-stop".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );
        store.interrupted_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-stop".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );

        let finished = finish_running_turn_handles(&store, "task-1", "turn-stop", BACKEND_CLAUDE);

        assert!(finished.interrupted);
        assert!(!finished.reset);
        assert!(store
            .running_process_sessions
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
        assert!(store.running_turns.lock().unwrap().get("task-1").is_none());
        assert!(store
            .interrupted_turns
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
    }

    #[test]
    fn native_turn_is_interruptible_without_creating_process_runtime_state() {
        let store = ChatStore::default();
        register_running_turn(
            &store,
            "task-native",
            "turn-native",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
        );

        let running = store
            .running_turns
            .lock()
            .unwrap()
            .get("task-native")
            .cloned()
            .expect("Native turn must be visible to chat_interrupt_turn");
        assert!(!turn_uses_legacy_process_runtime(&running));
        assert!(store
            .running_process_sessions
            .lock()
            .unwrap()
            .get("task-native")
            .is_none());

        prepare_running_turn_stop(&store, "task-native", true, false)
            .expect("Native turn can be interrupted through the shared product entrypoint");
        clear_running_handles(&store, "task-native");
        let finished = finish_running_turn_handles(
            &store,
            "task-native",
            "turn-native",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
        );

        assert!(finished.interrupted);
        assert!(!finished.reset);
    }

    #[test]
    fn native_approval_pause_and_interrupt_have_one_lifecycle_owner() {
        let store = ChatStore::default();
        register_running_turn(
            &store,
            "task-native",
            "turn-native",
            crate::native_agent::BACKEND_NATIVE_AGENTKIT,
        );
        assert!(pause_native_running_turn(
            &store,
            "task-native",
            "turn-native",
            Some("automation-1".into()),
            Some("session-native".into()),
        ));

        let prepared = prepare_running_turn_stop(&store, "task-native", true, false).unwrap();
        let pause = prepared.running_turn.native_approval_pause.unwrap();
        assert_eq!(pause.automation_run_id.as_deref(), Some("automation-1"));
        assert_eq!(pause.last_session_id.as_deref(), Some("session-native"));
        assert!(!pause_native_running_turn(
            &store,
            "task-native",
            "turn-native",
            None,
            None,
        ));
    }

    #[test]
    fn reset_finish_does_not_advance_pending_queue() {
        let store = ChatStore::default();
        store
            .running_tasks
            .lock()
            .unwrap()
            .insert("task-1".to_string(), true);
        store.reset_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-reset".to_string(),
                backend: BACKEND_CLAUDE.to_string(),
                native_approval_pause: None,
            },
        );
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(pending_turn("queued"));

        let finished = finish_running_turn_handles(&store, "task-1", "turn-reset", BACKEND_CLAUDE);
        assert!(finished.reset);
        assert!(take_next_pending_turn(&store, "task-1", !finished.reset).is_none());

        assert_eq!(
            store
                .pending_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|queue| queue.len()),
            Some(1)
        );
        assert!(store.running_tasks.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn interrupted_finish_does_not_advance_pending_queue() {
        let store = ChatStore::default();
        store
            .running_tasks
            .lock()
            .unwrap()
            .insert("task-1".to_string(), true);
        store.interrupted_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-interrupt".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
        );
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(pending_turn("queued"));

        let finished =
            finish_running_turn_handles(&store, "task-1", "turn-interrupt", BACKEND_CODEX);
        assert!(finished.interrupted);
        assert!(!finished.reset);
        assert!(
            take_next_pending_turn(&store, "task-1", !finished.interrupted && !finished.reset)
                .is_none()
        );

        assert_eq!(
            store
                .pending_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|queue| queue.len()),
            Some(1)
        );
        assert!(store.running_tasks.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn pending_reset_cleanup_is_consumed_once() {
        let store = ChatStore::default();

        mark_pending_reset_cleanup(&store, "task-1");

        assert!(take_pending_reset_cleanup(&store, "task-1"));
        assert!(!take_pending_reset_cleanup(&store, "task-1"));
    }

    #[test]
    fn runtime_finalization_persists_and_consumes_rollback_once() {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        insert_resume_task(&conn);
        let rollback = ChatRollbackResult {
            rolled_back: true,
            restored_content: "restore".to_string(),
            restored_attachments: vec![ChatAttachment {
                id: "att-1".to_string(),
                kind: "file".to_string(),
                path: "C:\\Files\\workspace\\Lilia\\README.md".to_string(),
                name: "README.md".to_string(),
                size: Some(12),
                exists: true,
                mime: Some("text/markdown".to_string()),
                directory: None,
            }],
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["evt-1".to_string()],
        };

        persist_pending_rollback(&conn, "task-1", &rollback).unwrap();
        let stored = take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .expect("pending rollback");

        assert_eq!(stored.restored_content, rollback.restored_content);
        assert_eq!(stored.restored_attachments.len(), 1);
        assert_eq!(stored.restored_attachments[0].id, "att-1");
        assert_eq!(
            stored.restored_attachments[0].path,
            rollback.restored_attachments[0].path
        );
        assert_eq!(
            stored.restored_attachments[0].mime.as_deref(),
            Some("text/markdown")
        );
        assert_eq!(stored.removed_event_ids, rollback.removed_event_ids);
        assert!(take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn runtime_finalization_persists_and_consumes_reset_cleanup_once() {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        insert_resume_task(&conn);

        persist_pending_reset_cleanup(&conn, "task-1").unwrap();

        assert!(take_persisted_pending_reset_cleanup(&conn, "task-1").unwrap());
        assert!(!take_persisted_pending_reset_cleanup(&conn, "task-1").unwrap());
    }

    #[test]
    fn runtime_state_clear_keeps_pending_finalization_until_explicit_cleanup() {
        let store = ChatStore::default();
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        insert_resume_task(&conn);
        let running_turn = RunningTurn {
            turn_id: "turn-reset".to_string(),
            backend: BACKEND_CODEX.to_string(),
            native_approval_pause: None,
        };
        let rollback = ChatRollbackResult {
            rolled_back: true,
            restored_content: "restore after restart".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["evt-1".to_string()],
        };
        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &running_turn,
            "reset_pending_finish",
            Some("process-1"),
            None,
        )
        .unwrap();
        persist_pending_rollback(&conn, "task-1", &rollback).unwrap();
        persist_pending_reset_cleanup(&conn, "task-1").unwrap();

        clear_runtime_state(&conn, "task-1").unwrap();

        assert!(load_runtime_state(&conn, &store, "task-1")
            .unwrap()
            .is_none());
        let stored = take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .expect("pending rollback");
        assert_eq!(stored.restored_content, rollback.restored_content);
        assert!(take_persisted_pending_reset_cleanup(&conn, "task-1").unwrap());

        persist_pending_rollback(&conn, "task-1", &rollback).unwrap();
        persist_pending_reset_cleanup(&conn, "task-1").unwrap();
        clear_runtime_finalization(&conn, "task-1").unwrap();

        assert!(take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .is_none());
        assert!(!take_persisted_pending_reset_cleanup(&conn, "task-1").unwrap());
    }

    #[test]
    fn peek_persisted_rollback_does_not_consume() {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        insert_resume_task(&conn);
        let rollback = ChatRollbackResult {
            rolled_back: true,
            restored_content: "peek content".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["e-1".to_string()],
        };

        persist_pending_rollback(&conn, "task-1", &rollback).unwrap();

        let first = peek_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .expect("first peek");
        assert_eq!(first.restored_content, "peek content");

        let second = peek_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .expect("second peek still returns content");
        assert_eq!(second.restored_content, "peek content");

        let take = take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .expect("take after peek");
        assert_eq!(take.restored_content, "peek content");

        assert!(take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn clear_persisted_rollback_removes_before_consume() {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        insert_resume_task(&conn);
        let rollback = ChatRollbackResult {
            rolled_back: true,
            restored_content: "clear test".to_string(),
            restored_attachments: Vec::new(),
            restored_conversation_references: Vec::new(),
            removed_event_ids: vec!["e-2".to_string()],
        };

        persist_pending_rollback(&conn, "task-1", &rollback).unwrap();
        clear_persisted_pending_rollback(&conn, "task-1").unwrap();

        assert!(peek_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .is_none());
        assert!(take_persisted_pending_rollback(&conn, "task-1")
            .unwrap()
            .is_none());
    }

    #[test]
    fn reset_session_plan_without_running_turn_clears_queue_and_requests_immediate_cleanup() {
        let store = ChatStore::default();
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(PendingChatTurn {
                guide_id: Some("guide-free".to_string()),
                ..pending_turn("queued")
            });

        let plan = plan_reset_session(&store, "task-1", None);

        assert_eq!(
            plan,
            ResetSessionPlan {
                cleared_guide_ids: vec!["guide-free".to_string()],
                stopped_running: false,
                immediate_cleanup: true,
            }
        );
        assert!(store.pending_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn reset_session_plan_for_builtin_running_turn_defers_to_stop_running_turn() {
        let store = ChatStore::default();
        let running_turn = RunningTurn {
            turn_id: "turn-builtin".to_string(),
            backend: BACKEND_CLAUDE.to_string(),
            native_approval_pause: None,
        };
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(PendingChatTurn {
                guide_id: Some("guide-builtin".to_string()),
                ..pending_turn("queued")
            });

        let plan = plan_reset_session(&store, "task-1", Some(&running_turn));

        assert_eq!(plan, ResetSessionPlan::default());
        assert_eq!(
            store
                .pending_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|queue| queue.len()),
            Some(1)
        );
        assert!(store.reset_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn runtime_snapshot_reports_idle_state() {
        let store = ChatStore::default();

        let snapshot = chat_runtime_snapshot(&store, "task-1");

        assert_eq!(snapshot.task_id, "task-1");
        assert_eq!(snapshot.phase, "idle");
        assert!(snapshot.backend.is_none());
        assert!(snapshot.turn_id.is_none());
        assert_eq!(snapshot.queued_count, 0);
        assert!(!snapshot.pending_rollback);
        assert!(!snapshot.pending_reset_cleanup);
    }

    #[test]
    fn runtime_snapshot_reports_running_and_queued_state() {
        let store = ChatStore::default();
        store.running_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-1".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
        );
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(PendingChatTurn {
                guide_id: Some("guide-1".to_string()),
                ..pending_turn("queued")
            });
        let snapshot = chat_runtime_snapshot(&store, "task-1");

        assert_eq!(snapshot.phase, "running_and_queued");
        assert_eq!(snapshot.backend.as_deref(), Some(BACKEND_CODEX));
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(snapshot.queued_count, 1);
    }

    #[test]
    fn runtime_snapshot_prefers_reset_pending_finish_phase() {
        let store = ChatStore::default();
        store.reset_turns.lock().unwrap().insert(
            "task-1".to_string(),
            RunningTurn {
                turn_id: "turn-reset".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
        );
        mark_pending_reset_cleanup(&store, "task-1");
        set_pending_rollback(
            &store,
            "task-1",
            ChatRollbackResult {
                rolled_back: true,
                restored_content: "restore".to_string(),
                restored_attachments: Vec::new(),
                restored_conversation_references: Vec::new(),
                removed_event_ids: vec!["evt-1".to_string()],
            },
        );

        let snapshot = chat_runtime_snapshot(&store, "task-1");

        assert_eq!(snapshot.phase, "reset_pending_finish");
        assert!(snapshot.pending_rollback);
        assert!(snapshot.pending_reset_cleanup);
    }

    #[test]
    fn runtime_snapshot_falls_back_to_current_epoch_persisted_state() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        let running_turn = RunningTurn {
            turn_id: "turn-persisted".to_string(),
            backend: BACKEND_CODEX.to_string(),
            native_approval_pause: None,
        };

        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &running_turn,
            "running",
            None,
            None,
        )
        .unwrap();
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(pending_turn("queued"));
        let snapshot = chat_runtime_snapshot_with_persisted(Some(&conn), &store, "task-1");

        assert_eq!(snapshot.phase, "running_and_queued");
        assert_eq!(snapshot.backend.as_deref(), Some(BACKEND_CODEX));
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-persisted"));
    }

    #[test]
    fn persisted_runtime_state_reports_stale_epoch_as_abandoned() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, runtime_epoch, updated_at)
               VALUES ('task-1', 'turn-old', 'codex', 'running', 'stale-epoch', 1)"#,
            [],
        )
        .unwrap();

        let snapshot = chat_runtime_snapshot_with_persisted(Some(&conn), &store, "task-1");

        assert_eq!(snapshot.phase, "abandoned");
        assert_eq!(snapshot.backend.as_deref(), Some(BACKEND_CODEX));
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-old"));
    }

    #[test]
    fn pending_turn_recovery_clears_abandoned_runtime_state() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, runtime_epoch, updated_at)
               VALUES ('task-1', 'turn-old', 'codex', 'running', 'stale-epoch', 1)"#,
            [],
        )
        .unwrap();

        assert!(prepare_pending_turn_recovery(&conn, &store, "task-1").unwrap());
        assert!(load_any_runtime_state(&conn, "task-1").unwrap().is_none());
    }

    #[test]
    fn recoverable_pending_turn_drains_queue_after_clearing_abandoned_runtime_state() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, runtime_epoch, updated_at)
               VALUES ('task-1', 'turn-old', 'codex', 'running', 'stale-epoch', 1)"#,
            [],
        )
        .unwrap();
        persist_pending_turn(
            &conn,
            "task-1",
            &PendingChatTurn {
                ..pending_turn("recover")
            },
        )
        .unwrap();

        let turn = take_next_recoverable_pending_turn(&conn, &store, "task-1")
            .unwrap()
            .expect("recoverable turn");

        assert_eq!(turn.turn_id, "turn-recover");
        assert!(load_any_runtime_state(&conn, "task-1").unwrap().is_none());
        assert_eq!(count_pending_turns(&conn, "task-1").unwrap(), 0);
    }

    #[test]
    fn pending_turn_recovery_keeps_live_runtime_state_blocking() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &RunningTurn {
                turn_id: "turn-live".to_string(),
                backend: BACKEND_CODEX.to_string(),
                native_approval_pause: None,
            },
            "running",
            None,
            None,
        )
        .unwrap();

        assert!(!prepare_pending_turn_recovery(&conn, &store, "task-1").unwrap());
        assert!(load_any_runtime_state(&conn, "task-1").unwrap().is_some());
    }

    #[test]
    fn runtime_state_phase_update_preserves_live_process_session() {
        let store = ChatStore::default();
        let conn = resume_conn();
        insert_resume_task(&conn);
        let running_turn = RunningTurn {
            turn_id: "turn-live".to_string(),
            backend: BACKEND_CODEX.to_string(),
            native_approval_pause: None,
        };

        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &running_turn,
            "running",
            Some("process-live"),
            None,
        )
        .unwrap();
        persist_runtime_state(
            &conn,
            &store,
            "task-1",
            &running_turn,
            "reset_pending_finish",
            None,
            None,
        )
        .unwrap();

        let persisted = load_runtime_state(&conn, &store, "task-1")
            .unwrap()
            .expect("runtime state");
        assert_eq!(persisted.phase, "reset_pending_finish");
        assert_eq!(
            persisted.process_session_id.as_deref(),
            Some("process-live")
        );
    }

    #[test]
    fn queued_user_message_recovery_updates_existing_timeline_row() {
        let conn = resume_conn_with_task();
        let message = ChatMessage {
            id: "msg-recover".to_string(),
            task_id: "task-1".to_string(),
            role: "user".to_string(),
            content: "resume queued".to_string(),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            created_at: 100,
        };
        let queued_input = AgentTimelineEventInput {
            id: Some(message.id.clone()),
            task_id: message.task_id.clone(),
            turn_id: Some("turn-recover".to_string()),
            backend: BACKEND_CODEX.to_string(),
            kind: "message".to_string(),
            status: "pending".to_string(),
            title: "用户输入".to_string(),
            summary: Some(message.content.clone()),
            payload: json!({
                "role": message.role,
                "content": message.content,
                "attachments": message.attachments,
                "queued": true,
            }),
            created_at: Some(message.created_at as i64),
            updated_at: Some(101),
        };
        agent_timeline::insert(&conn, queued_input).unwrap();
        let recovered_input = AgentTimelineEventInput {
            id: Some(message.id.clone()),
            task_id: message.task_id.clone(),
            turn_id: Some("turn-recover".to_string()),
            backend: BACKEND_CODEX.to_string(),
            kind: "message".to_string(),
            status: "success".to_string(),
            title: "用户输入".to_string(),
            summary: Some(message.content.clone()),
            payload: json!({
                "role": message.role,
                "content": message.content,
                "attachments": message.attachments,
                "queued": false,
            }),
            created_at: Some(message.created_at as i64),
            updated_at: Some(102),
        };

        let recovered = agent_timeline::insert(&conn, recovered_input).unwrap();

        assert_eq!(recovered.id, "msg-recover");
        assert_eq!(recovered.status, "success");
        assert_eq!(recovered.payload["queued"], json!(false));
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_timeline_events WHERE task_id = 'task-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn persisted_runtime_state_with_live_process_session_survives_epoch_change() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();

        let python = test_python_executable();
        let child = match std::process::Command::new(python)
            .args([
                "-c",
                "import sys,time; print('{\"kind\":\"timeline\"}'); sys.stdout.flush(); time.sleep(0.2)",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!("skip live process session snapshot test: {err}");
                return;
            }
        };
        let process_session_id =
            crate::chat::runner::start_test_process_session(child, &json!({"boot": true})).unwrap();

        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            rusqlite::params![
                "task-1",
                "turn-live",
                "codex",
                "running",
                process_session_id.clone(),
                "stale-epoch",
                1_i64
            ],
        )
        .unwrap();

        let snapshot = chat_runtime_snapshot_with_persisted(Some(&conn), &store, "task-1");

        assert_eq!(snapshot.phase, "running");
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-live"));
        assert_eq!(
            snapshot.process_session_id.as_deref(),
            Some(process_session_id.as_str())
        );
        crate::chat::runner::remove_test_process_session(&process_session_id);
    }

    #[test]
    fn restore_active_runtime_sessions_rehydrates_running_turns_from_live_process_session() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();

        let python = test_python_executable();
        let child = match std::process::Command::new(python)
            .args(["-c", "import time; time.sleep(0.2)"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!("skip runtime restore test: {err}");
                return;
            }
        };
        let process_session_id =
            crate::chat::runner::start_test_process_session(child, &json!({"boot": true})).unwrap();

        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            rusqlite::params![
                "task-1",
                "turn-restore",
                "codex",
                "running",
                process_session_id.clone(),
                "stale-epoch",
                1_i64
            ],
        )
        .unwrap();

        restore_active_runtime_sessions(&conn, &store);

        assert!(store
            .running_tasks
            .lock()
            .unwrap()
            .get("task-1")
            .copied()
            .unwrap_or(false));
        assert_eq!(
            store
                .running_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|turn| turn.turn_id.as_str()),
            Some("turn-restore")
        );
        assert_eq!(
            store
                .running_process_sessions
                .lock()
                .unwrap()
                .get("task-1")
                .map(String::as_str),
            Some(process_session_id.as_str())
        );
        crate::chat::runner::remove_test_process_session(&process_session_id);
    }

    #[test]
    fn restore_active_runtime_sessions_skips_dead_process_session() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();

        let python = test_python_executable();
        let child = match std::process::Command::new(python)
            .args(["-c", "pass"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                eprintln!("skip dead runtime restore test: {err}");
                return;
            }
        };
        let process_session_id =
            crate::chat::runner::start_test_process_session(child, &json!({"boot": true})).unwrap();
        crate::chat::runner::remove_test_process_session(&process_session_id);

        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
            rusqlite::params![
                "task-1",
                "turn-dead",
                "codex",
                "running",
                process_session_id.clone(),
                "stale-epoch",
                1_i64
            ],
        )
        .unwrap();

        restore_active_runtime_sessions(&conn, &store);

        assert!(!store
            .running_tasks
            .lock()
            .unwrap()
            .get("task-1")
            .copied()
            .unwrap_or(false));
        assert!(store.running_turns.lock().unwrap().get("task-1").is_none());
        assert!(store
            .running_process_sessions
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
    }

    #[test]
    fn restore_active_runtime_sessions_preserves_backend_for_resume_dispatch() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();
        insert_task_with_id(&conn, "task-2", "session-2");

        let python = test_python_executable();
        let child_1 = std::process::Command::new(&python)
            .args(["-c", "import time; time.sleep(0.2)"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let child_2 = std::process::Command::new(&python)
            .args(["-c", "import time; time.sleep(0.2)"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let proc_builtin =
            crate::chat::runner::start_test_process_session(child_1, &json!({"boot": true}))
                .unwrap();
        let proc_second =
            crate::chat::runner::start_test_process_session(child_2, &json!({"boot": true}))
                .unwrap();

        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES ('task-1', 'turn-claude', 'claude', 'running', ?1, 'stale-1', 1)"#,
            rusqlite::params![proc_builtin.clone()],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES ('task-2', 'turn-codex', 'codex', 'running', ?1, 'stale-2', 2)"#,
            rusqlite::params![proc_second.clone()],
        )
        .unwrap();

        let restored = restore_active_runtime_sessions(&conn, &store);

        assert_eq!(restored.len(), 2);
        assert!(restored
            .iter()
            .any(|state| state.task_id == "task-1" && state.turn.backend == BACKEND_CLAUDE));
        assert!(restored
            .iter()
            .any(|state| state.task_id == "task-2" && state.turn.backend == BACKEND_CODEX));

        crate::chat::runner::remove_test_process_session(&proc_builtin);
        crate::chat::runner::remove_test_process_session(&proc_second);
    }

    #[test]
    fn restore_active_runtime_sessions_only_returns_still_reattachable_sessions() {
        let store = ChatStore::default();
        let conn = resume_conn_with_task();

        let python = test_python_executable();
        let child = std::process::Command::new(python)
            .args(["-c", "pass"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let process_session_id =
            crate::chat::runner::start_test_process_session(child, &json!({"boot": true})).unwrap();
        crate::chat::runner::remove_test_process_session(&process_session_id);

        conn.execute(
            r#"INSERT INTO task_runtime_states
               (task_id, turn_id, backend, phase, process_session_id, runtime_epoch, updated_at)
               VALUES ('task-1', 'turn-finished', 'claude', 'running', ?1, 'stale-x', 1)"#,
            rusqlite::params![process_session_id.clone()],
        )
        .unwrap();

        let restored = restore_active_runtime_sessions(&conn, &store);

        assert!(restored.is_empty());
        assert!(store.running_turns.lock().unwrap().get("task-1").is_none());
        assert!(store
            .running_process_sessions
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
    }

    #[test]
    fn prepare_running_turn_stop_without_running_turn_leaves_queue_intact() {
        let store = ChatStore::default();
        store
            .pending_turns
            .lock()
            .unwrap()
            .entry("task-1".to_string())
            .or_default()
            .push_back(PendingChatTurn {
                guide_id: Some("guide-still-queued".to_string()),
                ..pending_turn("queued")
            });

        assert!(prepare_running_turn_stop(&store, "task-1", true, true).is_none());
        assert_eq!(
            store
                .pending_turns
                .lock()
                .unwrap()
                .get("task-1")
                .map(|queue| queue.len()),
            Some(1)
        );
        assert!(store
            .interrupted_turns
            .lock()
            .unwrap()
            .get("task-1")
            .is_none());
        assert!(store.reset_turns.lock().unwrap().get("task-1").is_none());
    }

    #[test]
    fn interrupted_exit_does_not_emit_runner_error() {
        assert!(!should_emit_runner_exit_error(
            true,
            true,
            "agent 进程被终止",
        ));
        assert!(should_emit_runner_exit_error(
            false,
            true,
            "agent 进程异常退出",
        ));
        assert!(!should_emit_runner_exit_error(false, true, "   "));
    }

    fn create_resume_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL
            );
            CREATE TABLE task_agent_sessions (
              task_id         TEXT NOT NULL,
              backend         TEXT NOT NULL CHECK (backend IN ('claude','codex')),
              session_id      TEXT NOT NULL,
              updated_at      INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );
            CREATE TABLE task_runtime_states (
              task_id         TEXT PRIMARY KEY,
              turn_id         TEXT NOT NULL,
              backend         TEXT NOT NULL CHECK (backend IN ('claude','codex')),
              phase           TEXT NOT NULL CHECK (phase IN
                                ('running','interrupted_pending_finish','reset_pending_finish')),
              process_session_id TEXT,
              runtime_epoch   TEXT NOT NULL,
              context_json    TEXT,
              updated_at      INTEGER NOT NULL
            );
            CREATE TABLE task_runtime_finalizations (
              task_id               TEXT PRIMARY KEY,
              pending_reset_cleanup INTEGER NOT NULL DEFAULT 0
                                    CHECK (pending_reset_cleanup IN (0, 1)),
              rollback_json         TEXT,
              updated_at            INTEGER NOT NULL
            );
            CREATE TABLE task_pending_turns (
              id              INTEGER PRIMARY KEY AUTOINCREMENT,
              task_id         TEXT NOT NULL,
              content         TEXT NOT NULL,
              composer_json   TEXT NOT NULL,
              project_cwd     TEXT NOT NULL,
              attachments_json TEXT NOT NULL DEFAULT '[]',
              workflow_json   TEXT,
              runtime_command_json TEXT,
              runtime_options_json TEXT,
              message_json    TEXT NOT NULL,
              turn_id         TEXT NOT NULL,
              guide_id        TEXT,
              created_at      INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
        agent_timeline::create_timeline_schema(conn).unwrap();
    }

    fn resume_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_resume_schema(&conn);
        conn
    }

    fn resume_conn_with_task() -> Connection {
        let conn = resume_conn();
        insert_task(&conn, "session-1");
        conn
    }

    fn resume_conn_with_legacy_task() -> Connection {
        let conn = resume_conn();
        insert_resume_task(&conn);
        conn
    }

    fn insert_resume_task(conn: &Connection) {
        insert_task(conn, "legacy-session");
    }

    fn insert_task(conn: &Connection, session_id: &str) {
        insert_task_with_id(conn, "task-1", session_id);
    }

    fn insert_task_with_id(conn: &Connection, task_id: &str, session_id: &str) {
        conn.execute(
            "INSERT INTO tasks (id, session_id) VALUES (?1, ?2)",
            [task_id, session_id],
        )
        .unwrap();
    }

    fn insert_codex_timeline_session(conn: &Connection, event_id: &str, session_id: &str, at: i64) {
        agent_timeline::insert(
            conn,
            AgentTimelineEventInput {
                id: Some(event_id.to_string()),
                task_id: "task-1".to_string(),
                turn_id: Some("turn-1".to_string()),
                backend: BACKEND_CODEX.to_string(),
                kind: "turn".to_string(),
                status: "success".to_string(),
                title: "Codex done".to_string(),
                summary: None,
                payload: json!({ "sessionId": session_id }),
                created_at: Some(at),
                updated_at: Some(at),
            },
        )
        .unwrap();
    }

    fn assert_resume_session(conn: &Connection, backend: &str, expected: Option<&str>) {
        assert_eq!(
            load_persisted_resume_session_id(conn, "task-1", backend),
            expected.map(|sid| sid.to_string())
        );
    }

    #[test]
    fn persisted_resume_session_id_ignores_unscoped_task_session_id() {
        let conn = resume_conn();
        insert_task(&conn, "claude-session");
        insert_codex_timeline_session(&conn, "codex-turn", "codex-thread", 200);

        assert_eq!(
            load_persisted_resume_session_id(&conn, "task-1", BACKEND_CODEX),
            Some("codex-thread".to_string())
        );
        assert_eq!(
            load_persisted_resume_session_id(&conn, "task-1", BACKEND_CLAUDE),
            None
        );
    }

    #[test]
    fn persisted_resume_session_id_reads_backend_scoped_checkpoint() {
        let conn = resume_conn_with_legacy_task();

        persist_agent_session_id(&conn, "task-1", BACKEND_CLAUDE, "claude-session").unwrap();
        persist_agent_session_id(&conn, "task-1", BACKEND_CODEX, "codex-thread").unwrap();

        assert_resume_session(&conn, BACKEND_CLAUDE, Some("claude-session"));
        assert_resume_session(&conn, BACKEND_CODEX, Some("codex-thread"));
    }

    #[test]
    fn persisted_resume_session_id_uses_latest_backend_checkpoint() {
        let conn = resume_conn_with_legacy_task();
        insert_codex_timeline_session(&conn, "codex-turn", "timeline-thread", 200);

        persist_agent_session_id(&conn, "task-1", BACKEND_CODEX, "builtin-thread").unwrap();
        persist_agent_session_id(&conn, "task-1", BACKEND_CODEX, "updated-thread").unwrap();

        assert_eq!(
            load_persisted_resume_session_id(&conn, "task-1", BACKEND_CODEX),
            Some("updated-thread".to_string())
        );
        assert_eq!(
            load_persisted_resume_session_id(&conn, "task-1", BACKEND_CLAUDE),
            None
        );
    }

    #[test]
    fn persisted_resume_session_id_prefers_checkpoint_over_timeline() {
        let conn = resume_conn_with_legacy_task();
        insert_codex_timeline_session(&conn, "codex-turn-old", "timeline-thread", 100);
        persist_agent_session_id(&conn, "task-1", BACKEND_CODEX, "checkpoint-thread").unwrap();

        assert_resume_session(&conn, BACKEND_CODEX, Some("checkpoint-thread"));
    }

    #[test]
    fn persisted_resume_session_id_falls_back_to_timeline_without_checkpoint() {
        let conn = resume_conn_with_legacy_task();
        insert_codex_timeline_session(&conn, "codex-turn", "timeline-thread", 200);

        assert_resume_session(&conn, BACKEND_CODEX, Some("timeline-thread"));
    }

    #[test]
    fn clear_agent_sessions_for_task_removes_backend_checkpoints() {
        let conn = resume_conn_with_legacy_task();
        persist_agent_session_id(&conn, "task-1", BACKEND_CLAUDE, "claude-session").unwrap();
        persist_agent_session_id(&conn, "task-1", BACKEND_CODEX, "codex-thread").unwrap();

        clear_agent_sessions_for_task(&conn, "task-1").unwrap();

        assert_resume_session(&conn, BACKEND_CLAUDE, None);
        assert_resume_session(&conn, BACKEND_CODEX, None);
    }
}
