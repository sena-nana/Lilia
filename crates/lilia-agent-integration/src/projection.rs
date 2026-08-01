//! AgentKit event → product timeline / todo / artifact / pending projection (#45 / #46).
//!
//! The projector itself performs **no** database writes. Storage apply is owned by
//! `lilia-storage` (product fact surface). Desktop may mirror timeline rows into
//! SQLite as a rebuildable UI cache only — never as the execution / recovery fact source.

use lilia_contracts::{
    AgentSessionRef, ArtifactProjection, PendingProjection, PendingProjectionStatus,
    ProjectionEventId, TaskId, TimelineProjectionCommand, TimelineProjectionEvent, TodoProjection,
    PRODUCT_TIMELINE_STORE_ID,
};
use mutsuki_agent_contracts::{AgentEvent, AgentEventEnvelope, InteractionKind};
use serde_json::{json, Value as JsonValue};

/// Convert one AgentKit envelope into product projection command(s).
pub fn project_agent_event(
    task_id: &TaskId,
    envelope: &AgentEventEnvelope,
) -> Vec<TimelineProjectionCommand> {
    let session = match AgentSessionRef::new(envelope.session_id.clone()) {
        Ok(session) => session,
        Err(_) => {
            return vec![TimelineProjectionCommand::SkipUnknown {
                session_id: envelope.session_id.clone(),
                sequence: envelope.sequence,
                reason: "invalid session id".into(),
            }];
        }
    };

    let mut commands = Vec::new();

    let mapped = match &envelope.event {
        AgentEvent::ModelDelta { text, turn_id, .. } => Some((
            "message",
            "streaming",
            "Native 流式输出",
            Some(text.clone()),
            json!({
                "role": "assistant",
                "content": text,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::ReasoningDelta { text, turn_id, .. } => Some((
            "reasoning",
            "streaming",
            "Native 推理",
            Some(text.clone()),
            json!({
                "content": text,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::ToolCallStarted {
            name,
            call_id,
            turn_id,
            ..
        } => Some((
            "tool",
            "running",
            "Native 工具调用",
            Some(name.clone()),
            json!({
                "tool": name,
                "callId": call_id,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::ToolCallCompleted {
            call_id,
            summary,
            turn_id,
            details,
            ..
        } => Some((
            "tool",
            "success",
            "Native 工具完成",
            Some(summary.clone()),
            json!({
                "callId": call_id,
                "summary": summary,
                "detailsRef": details,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::CommandStarted { command, turn_id } => Some((
            "command",
            "running",
            "Native 命令",
            Some(command.command.clone()),
            json!({
                "commandId": command.command_id,
                "command": command.command,
                "args": command.args,
                "cwd": command.cwd,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::CommandOutput {
            command_id,
            stream,
            chunk,
            turn_id,
            details,
        } => Some((
            "command",
            "running",
            "Native 命令输出",
            Some(chunk.clone()),
            json!({
                "commandId": command_id,
                "stream": stream,
                "chunk": chunk,
                "detailsRef": details,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::CommandExited {
            command_id,
            exit_code,
            summary,
            turn_id,
        } => Some((
            "command",
            if *exit_code == 0 { "success" } else { "error" },
            "Native 命令结束",
            Some(summary.clone()),
            json!({
                "commandId": command_id,
                "exitCode": exit_code,
                "summary": summary,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::PlanUpdated { plan, turn_id } => Some((
            "plan",
            "in_progress",
            "Native Plan",
            Some(plan.plan_id.clone()),
            json!({
                "plan": plan,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::TodoUpdated { todo, turn_id } => {
            commands.push(TimelineProjectionCommand::UpsertTodo {
                todo: TodoProjection {
                    id: format!("{}:{}", session.as_str(), todo.todo_id),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence: envelope.sequence,
                    turn_id: Some(turn_id.clone()),
                    todo_id: todo.todo_id.clone(),
                    revision: todo.revision,
                    items: serde_json::to_value(&todo.items).unwrap_or(JsonValue::Null),
                },
            });
            Some((
                "todo_list",
                "in_progress",
                "Native Todo",
                Some(todo.todo_id.clone()),
                json!({
                    "todo": todo,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                    "notProductTask": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::FileChangeProposed { change, turn_id }
        | AgentEvent::FileChangeApplied { change, turn_id }
        | AgentEvent::FileChangeRejected { change, turn_id } => {
            let status = match &envelope.event {
                AgentEvent::FileChangeProposed { .. } => "pending",
                AgentEvent::FileChangeApplied { .. } => "success",
                AgentEvent::FileChangeRejected { .. } => "error",
                _ => "info",
            };
            Some((
                "file_change",
                status,
                "Native 文件变更",
                Some(change.summary.clone()),
                json!({
                    "change": {
                        "changeId": change.change_id,
                        "summary": change.summary,
                        "status": change.status,
                        "detailsRef": change.details,
                        "rejectionReason": change.rejection_reason,
                    },
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::WorkspaceEditProposed { proposal, turn_id } => Some((
            "file_change",
            "pending",
            "Native Workspace Edit",
            Some(proposal.summary.clone()),
            json!({
                "proposal": {
                    "proposalId": proposal.proposal_id,
                    "summary": proposal.summary,
                    "changeCount": proposal.changes.len(),
                    "detailsRef": proposal.details,
                },
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::ArtifactProduced { artifact, turn_id } => {
            let content_ref = artifact
                .content_ref
                .as_ref()
                .and_then(|r| serde_json::to_value(r).ok());
            commands.push(TimelineProjectionCommand::UpsertArtifact {
                artifact: ArtifactProjection {
                    id: format!("{}:{}", session.as_str(), artifact.artifact_id),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence: envelope.sequence,
                    turn_id: Some(turn_id.clone()),
                    artifact_id: artifact.artifact_id.clone(),
                    media_type: artifact.media_type.clone(),
                    summary: artifact.summary.clone(),
                    kind: artifact.kind.clone(),
                    size_bytes: artifact.size_bytes,
                    content_hash: artifact.content_hash.clone(),
                    content_ref,
                    provenance: artifact.provenance.clone(),
                    status: "available".into(),
                },
            });
            Some((
                "artifact",
                "success",
                "Native Artifact",
                Some(artifact.summary.clone()),
                json!({
                    "artifactId": artifact.artifact_id,
                    "mediaType": artifact.media_type,
                    "kind": artifact.kind,
                    "sizeBytes": artifact.size_bytes,
                    "contentHash": artifact.content_hash,
                    "contentRef": artifact.content_ref,
                    "provenance": artifact.provenance,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::FinalResponse {
            summary, turn_id, ..
        } => Some((
            "message",
            "success",
            "Native 最终回复",
            Some(summary.clone()),
            json!({
                "role": "assistant",
                "content": summary,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
        AgentEvent::TurnState { status, turn_id } => {
            let (kind, mapped_status, title) = match status.as_str() {
                "waiting_approval" => ("plan", "requires_action", "Native 等待审批"),
                "approval_granted" => ("plan", "success", "Native 审批通过"),
                "approval_denied" => ("plan", "error", "Native 审批拒绝"),
                "completed" => ("diagnostic", "success", "Native Turn"),
                _ => ("diagnostic", "info", "Native Turn"),
            };
            Some((
                kind,
                mapped_status,
                title,
                Some(status.clone()),
                json!({
                    "turnStatus": status,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                    "productProjectionStore": PRODUCT_TIMELINE_STORE_ID,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::ApprovalRequest { request } => {
            commands.push(TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("{}:{}", session.as_str(), request.action_id),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence: envelope.sequence,
                    turn_id: Some(request.turn_id.clone()),
                    request_id: request.action_id.clone(),
                    kind: "permission_approval".into(),
                    status: PendingProjectionStatus::Open,
                    prompt: Some(request.summary.clone()),
                    action_revision: Some(request.version),
                    payload: json!({
                        "tool": request.tool,
                        "sideEffect": request.side_effect,
                        "providerContext": {
                            "native": {
                                "sessionId": request.session_id,
                                "turnId": request.turn_id,
                                "actionId": request.action_id,
                                "version": request.version,
                                "tool": request.tool,
                            }
                        }
                    }),
                },
            });
            Some((
                "plan",
                "requires_action",
                "Native 审批",
                Some(request.summary.clone()),
                json!({
                    "interaction": "permission_approval",
                    "requestId": request.action_id,
                    "reason": request.summary,
                    "requestedAccess": {
                        "tool": request.tool,
                        "sideEffect": request.side_effect,
                    },
                    "scopeSuggestion": "turn",
                    "providerContext": {
                        "native": {
                            "sessionId": request.session_id,
                            "turnId": request.turn_id,
                            "actionId": request.action_id,
                            "version": request.version,
                            "tool": request.tool,
                        }
                    },
                    "tool": request.tool,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                    "productProjectionStore": PRODUCT_TIMELINE_STORE_ID,
                }),
                Some(request.turn_id.clone()),
            ))
        }
        AgentEvent::InteractionRequested {
            interaction,
            turn_id,
        } => {
            commands.push(TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("{}:{}", session.as_str(), interaction.interaction_id),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence: envelope.sequence,
                    turn_id: Some(turn_id.clone()),
                    request_id: interaction.interaction_id.clone(),
                    kind: match interaction.kind {
                        InteractionKind::Approval => "approval".into(),
                        InteractionKind::Clarification => "clarification".into(),
                        InteractionKind::PlanConfirm => "plan_confirm".into(),
                        InteractionKind::Custom => "custom".into(),
                    },
                    status: PendingProjectionStatus::Open,
                    prompt: Some(interaction.prompt.clone()),
                    action_revision: None,
                    payload: json!({
                        "options": interaction.options,
                        "detailsRef": interaction.details,
                    }),
                },
            });
            Some((
                "plan",
                "requires_action",
                "Native 交互请求",
                Some(interaction.prompt.clone()),
                json!({
                    "interaction": interaction.kind,
                    "requestId": interaction.interaction_id,
                    "prompt": interaction.prompt,
                    "options": interaction.options,
                    "detailsRef": interaction.details,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::InteractionResolved {
            resolution,
            turn_id,
        } => {
            commands.push(TimelineProjectionCommand::ResolvePending {
                session_id: session.as_str().into(),
                request_id: resolution.interaction_id.clone(),
                status: if resolution.accepted {
                    PendingProjectionStatus::Resolved
                } else {
                    PendingProjectionStatus::Cancelled
                },
                sequence: envelope.sequence,
                response: json!({
                    "accepted": resolution.accepted,
                    "response": resolution.response,
                }),
            });
            Some((
                "plan",
                if resolution.accepted {
                    "success"
                } else {
                    "cancelled"
                },
                "Native 交互已决",
                Some(resolution.interaction_id.clone()),
                json!({
                    "requestId": resolution.interaction_id,
                    "accepted": resolution.accepted,
                    "response": resolution.response,
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        _ => None,
    };

    match mapped {
        Some((kind, status, title, summary, mut payload, turn_id)) => {
            if let Some(obj) = payload.as_object_mut() {
                obj.entry("productProjectionStore")
                    .or_insert_with(|| json!(PRODUCT_TIMELINE_STORE_ID));
                obj.entry("projected").or_insert_with(|| json!(true));
                obj.entry("notExecutionFactSource")
                    .or_insert_with(|| json!(true));
            }
            commands.push(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::from_session_sequence(
                        session.as_str(),
                        envelope.sequence,
                    ),
                    task_id: task_id.clone(),
                    agent_session: session,
                    sequence: envelope.sequence,
                    turn_id,
                    kind: kind.into(),
                    status: status.into(),
                    title: title.into(),
                    summary,
                    payload,
                    projected: true,
                },
            });
            commands
        }
        None => {
            if commands.is_empty() {
                vec![TimelineProjectionCommand::SkipUnknown {
                    session_id: envelope.session_id.clone(),
                    sequence: envelope.sequence,
                    reason: "optional/unknown agent event".into(),
                }]
            } else {
                commands
            }
        }
    }
}

pub fn project_agent_events(
    task_id: &TaskId,
    events: &[AgentEventEnvelope],
) -> Vec<TimelineProjectionCommand> {
    events
        .iter()
        .flat_map(|envelope| project_agent_event(task_id, envelope))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mutsuki_agent_contracts::{
        AgentEventMeta, ArtifactRef, InteractionKind, InteractionRequest, InteractionResolution,
        TodoItem, TodoItemStatus, TodoState,
    };

    #[test]
    fn projects_tool_and_final_events_idempotently_by_sequence() {
        let task = TaskId::new("task-proj").unwrap();
        let envelope = AgentEventEnvelope {
            session_id: "sess-1".into(),
            sequence: 7,
            meta: AgentEventMeta::new("evt-1", "tool"),
            event: AgentEvent::ToolCallStarted {
                turn_id: "t1".into(),
                call_id: "c1".into(),
                name: "native.coding.fix".into(),
                input: json!({}),
            },
        };
        let a = project_agent_event(&task, &envelope);
        let b = project_agent_event(&task, &envelope);
        assert_eq!(a, b);
        match &a[..] {
            [TimelineProjectionCommand::UpsertTimelineEvent { event }] => {
                assert!(event.projected);
                assert_eq!(event.sequence, 7);
                assert_eq!(event.kind, "tool");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn projects_todo_artifact_and_pending_surfaces() {
        let task = TaskId::new("task-side").unwrap();
        let todo_env = AgentEventEnvelope {
            session_id: "sess-side".into(),
            sequence: 1,
            meta: AgentEventMeta::new("evt-todo", "todo"),
            event: AgentEvent::TodoUpdated {
                turn_id: "t1".into(),
                todo: TodoState {
                    todo_id: "todo-1".into(),
                    revision: 2,
                    items: vec![TodoItem {
                        item_id: "i1".into(),
                        title: "ship".into(),
                        status: TodoItemStatus::Pending,
                        priority: 1,
                        relation: None,
                    }],
                },
            },
        };
        let artifact_env = AgentEventEnvelope {
            session_id: "sess-side".into(),
            sequence: 2,
            meta: AgentEventMeta::new("evt-art", "artifact"),
            event: AgentEvent::ArtifactProduced {
                turn_id: "t1".into(),
                artifact: ArtifactRef {
                    artifact_id: "a1".into(),
                    media_type: "text/plain".into(),
                    summary: "out".into(),
                    content_ref: None,
                    kind: Some("file".into()),
                    size_bytes: Some(4),
                    content_hash: Some("h".into()),
                    provenance: Some("test".into()),
                    open_hint: None,
                    action_hint: None,
                },
            },
        };
        let pending_env = AgentEventEnvelope {
            session_id: "sess-side".into(),
            sequence: 3,
            meta: AgentEventMeta::new("evt-int", "interaction"),
            event: AgentEvent::InteractionRequested {
                turn_id: "t1".into(),
                interaction: InteractionRequest {
                    interaction_id: "int-1".into(),
                    kind: InteractionKind::Clarification,
                    prompt: "which?".into(),
                    options: json!(["a", "b"]),
                    details: None,
                },
            },
        };
        let resolved_env = AgentEventEnvelope {
            session_id: "sess-side".into(),
            sequence: 4,
            meta: AgentEventMeta::new("evt-res", "interaction"),
            event: AgentEvent::InteractionResolved {
                turn_id: "t1".into(),
                resolution: InteractionResolution {
                    interaction_id: "int-1".into(),
                    accepted: true,
                    response: json!({ "choice": "a" }),
                },
            },
        };

        let todo_cmds = project_agent_event(&task, &todo_env);
        assert!(todo_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::UpsertTodo { .. })));
        assert!(todo_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::UpsertTimelineEvent { event } if event.kind == "todo_list")));

        let art_cmds = project_agent_event(&task, &artifact_env);
        assert!(art_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::UpsertArtifact { .. })));

        let pending_cmds = project_agent_event(&task, &pending_env);
        assert!(pending_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::UpsertPending { .. })));

        let resolved_cmds = project_agent_event(&task, &resolved_env);
        assert!(resolved_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::ResolvePending { .. })));
    }
}
