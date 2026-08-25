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
use mutsuki_agent_contracts::{
    AgentEvent, AgentEventEnvelope, AgentPermissionMode, InteractionKind, InteractionRequest,
};
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
        AgentEvent::UserMessage {
            turn_id,
            content,
            metadata,
        } => Some((
            "message",
            "success",
            "用户输入",
            Some(content.clone()),
            json!({
                "role": "user",
                "content": content,
                "attachments": metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("attachments"))
                    .cloned()
                    .unwrap_or_else(|| json!([])),
                "context": metadata,
                "source": "native-agentkit",
                "sequence": envelope.sequence,
                "projected": true,
            }),
            Some(turn_id.clone()),
        )),
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
        AgentEvent::Usage { turn_id, usage } => Some((
            "usage",
            "success",
            "Native 用量",
            Some(format!("{} tokens", usage.total_tokens)),
            json!({
                "inputTokens": usage.input_tokens,
                "outputTokens": usage.output_tokens,
                "totalTokens": usage.total_tokens,
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
        } => {
            let subagent = name == "delegate_agent";
            Some((
                if subagent { "subagent" } else { "tool" },
                "running",
                if subagent {
                    "Native 子 Agent"
                } else {
                    "Native 工具调用"
                },
                Some(name.clone()),
                json!({
                    "tool": name,
                    "callId": call_id,
                    "agentType": subagent.then_some("custom"),
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
        AgentEvent::ToolCallCompleted {
            call_id,
            summary,
            turn_id,
            details,
            ..
        } => {
            let subagent = summary == "delegate_agent";
            Some((
                if subagent { "subagent" } else { "tool" },
                "success",
                if subagent {
                    "Native 子 Agent 完成"
                } else {
                    "Native 工具完成"
                },
                Some(summary.clone()),
                json!({
                    "callId": call_id,
                    "summary": summary,
                    "detailsRef": details,
                    "agentType": subagent.then_some("custom"),
                    "source": "native-agentkit",
                    "sequence": envelope.sequence,
                    "projected": true,
                }),
                Some(turn_id.clone()),
            ))
        }
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
            let product_kind = product_interaction_kind(interaction);
            let (timeline_kind, timeline_title) = match product_kind {
                "plan_approval" => ("plan", "Native 计划确认"),
                "architecture_change" => ("architecture_change", "Native 架构变更"),
                _ => ("ask_user", "Native 交互请求"),
            };
            let spec = interaction_ask_user_spec(interaction);
            let pending_payload = match product_kind {
                "mcp_elicitation" => mcp_elicitation_payload(interaction),
                "architecture_change" => architecture_interaction_payload(task_id, interaction),
                _ => json!({
                    "interaction": product_kind,
                    "sessionId": interaction.session_id,
                    "turnId": interaction.turn_id,
                    "version": interaction.version,
                    "options": interaction.options,
                    "spec": spec,
                    "detailsRef": interaction.details,
                }),
            };
            commands.push(TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("{}:{}", session.as_str(), interaction.interaction_id),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence: envelope.sequence,
                    turn_id: Some(turn_id.clone()),
                    request_id: interaction.interaction_id.clone(),
                    kind: product_kind.into(),
                    status: PendingProjectionStatus::Open,
                    prompt: Some(interaction.prompt.clone()),
                    action_revision: Some(interaction.version),
                    payload: pending_payload.clone(),
                },
            });
            let mut timeline_payload = match product_kind {
                "mcp_elicitation" | "architecture_change" => pending_payload,
                _ => json!({
                    "interaction": product_kind,
                    "requestId": interaction.interaction_id,
                    "sessionId": interaction.session_id,
                    "turnId": interaction.turn_id,
                    "version": interaction.version,
                    "prompt": interaction.prompt,
                    "options": interaction.options,
                    "questions": spec.get("questions").cloned().unwrap_or_else(|| json!([])),
                    "spec": spec,
                    "plan": interaction.options.get("plan"),
                    "detailsRef": interaction.details,
                }),
            };
            if let Some(payload) = timeline_payload.as_object_mut() {
                payload.insert("requestId".to_owned(), json!(interaction.interaction_id));
                payload.insert("prompt".to_owned(), json!(interaction.prompt));
            }
            Some((
                timeline_kind,
                "requires_action",
                timeline_title,
                Some(interaction.prompt.clone()),
                timeline_payload,
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
            let architecture_change = resolution
                .response
                .get("interaction")
                .and_then(JsonValue::as_str)
                == Some("architecture_change");
            Some((
                if architecture_change {
                    "architecture_change"
                } else {
                    "plan"
                },
                if resolution.accepted {
                    "success"
                } else {
                    "cancelled"
                },
                if architecture_change {
                    "Native 架构变更已决"
                } else {
                    "Native 交互已决"
                },
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
                obj.entry("createdAt")
                    .or_insert_with(|| json!(envelope.meta.timestamp_unix_ms));
                obj.entry("backend")
                    .or_insert_with(|| json!("native-agentkit"));
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

fn product_interaction_kind(interaction: &InteractionRequest) -> &'static str {
    match interaction.kind {
        InteractionKind::PlanConfirm => "plan_approval",
        InteractionKind::Custom if is_architecture_change(interaction) => "architecture_change",
        InteractionKind::Custom if is_mcp_elicitation(interaction) => "mcp_elicitation",
        InteractionKind::Approval | InteractionKind::Clarification | InteractionKind::Custom => {
            "ask_user"
        }
    }
}

fn is_architecture_change(interaction: &InteractionRequest) -> bool {
    interaction.source_tool.as_deref() == Some("update_project_architecture")
        || ["interaction", "interactionKind", "kind", "type"]
            .into_iter()
            .any(|field| {
                interaction.options.get(field).and_then(JsonValue::as_str)
                    == Some("architecture_change")
            })
}

fn architecture_interaction_payload(
    task_id: &TaskId,
    interaction: &InteractionRequest,
) -> JsonValue {
    let context = interaction.context.as_ref();
    let project_id = context
        .and_then(|context| context.get("productProjectId"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let product_task_id = context
        .and_then(|context| context.get("productTaskId"))
        .and_then(JsonValue::as_str)
        .unwrap_or_else(|| task_id.as_str());
    let expected_version = context
        .and_then(|context| context.get("projectArchitectureVersion"))
        .and_then(JsonValue::as_i64)
        .or_else(|| {
            interaction
                .options
                .get("expectedVersion")
                .and_then(JsonValue::as_i64)
        });
    let permission = match interaction.permission_mode {
        AgentPermissionMode::Full => "full",
        AgentPermissionMode::Ask => "ask",
        AgentPermissionMode::ReadOnly => "readonly",
    };
    json!({
        "interaction": "architecture_change",
        "projectId": project_id,
        "taskId": product_task_id,
        "turnId": interaction.turn_id,
        "backend": "native-agentkit",
        "permission": permission,
        "reason": interaction.options.get("reason").and_then(JsonValue::as_str).unwrap_or(&interaction.prompt),
        "changes": interaction.options.get("changes").cloned().unwrap_or_else(|| json!([])),
        "requestId": interaction.interaction_id,
        "expectedVersion": expected_version,
        "status": if interaction.permission_mode == AgentPermissionMode::ReadOnly { "proposed" } else { "pending" },
        "requiresConfirmation": interaction.permission_mode != AgentPermissionMode::Full,
        "sourceTool": interaction.source_tool,
        "sessionId": interaction.session_id,
        "version": interaction.version,
        "detailsRef": interaction.details,
    })
}

fn is_mcp_elicitation(interaction: &InteractionRequest) -> bool {
    if interaction.kind != InteractionKind::Custom {
        return false;
    }
    let options = &interaction.options;
    ["interaction", "interactionKind", "kind", "type"]
        .into_iter()
        .any(|field| options.get(field).and_then(JsonValue::as_str) == Some("mcp_elicitation"))
        || (options
            .get("serverName")
            .and_then(JsonValue::as_str)
            .is_some()
            && matches!(
                options.get("mode").and_then(JsonValue::as_str),
                Some("form" | "url")
            ))
}

fn mcp_elicitation_payload(interaction: &InteractionRequest) -> JsonValue {
    let options = interaction.options.as_object();
    let string = |camel: &str, snake: &str| {
        options
            .and_then(|options| options.get(camel).or_else(|| options.get(snake)))
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
    };
    let requested_schema = options
        .and_then(|options| {
            options
                .get("requestedSchema")
                .or_else(|| options.get("requested_schema"))
        })
        .cloned();
    let url = string("url", "url");
    let mode = string("mode", "mode").unwrap_or_else(|| {
        if url.is_some() && requested_schema.is_none() {
            "url".to_owned()
        } else {
            "form".to_owned()
        }
    });
    let mut payload = json!({
        "interaction": "mcp_elicitation",
        "sessionId": interaction.session_id,
        "threadId": string("threadId", "thread_id").unwrap_or_else(|| interaction.session_id.clone()),
        "turnId": string("turnId", "turn_id").unwrap_or_else(|| interaction.turn_id.clone()),
        "version": interaction.version,
        "serverName": string("serverName", "server_name").unwrap_or_else(|| "MCP".to_owned()),
        "mode": mode,
        "message": string("message", "message").unwrap_or_else(|| interaction.prompt.clone()),
        "requestedSchema": requested_schema,
        "url": url,
        "elicitationId": string("elicitationId", "elicitation_id"),
        "_meta": options.and_then(|options| options.get("_meta")).cloned(),
        "detailsRef": interaction.details,
    });
    if let Some(payload) = payload.as_object_mut() {
        payload.retain(|_, value| !value.is_null());
    }
    payload
}

fn interaction_ask_user_spec(interaction: &InteractionRequest) -> JsonValue {
    if interaction.kind == InteractionKind::PlanConfirm {
        return json!({
            "title": interaction.options.get("title").and_then(JsonValue::as_str).unwrap_or("确认 Native Agent 计划"),
            "source": "Native AgentKit",
            "intent": "plan_approval",
            "dismissable": true,
            "questions": [{
                "id": "approve-plan",
                "header": "计划确认",
                "question": interaction.prompt,
                "mode": "confirm",
                "confirmLabel": "按计划执行",
                "cancelLabel": "先不执行",
            }],
        });
    }
    if let Some(questions) = interaction
        .options
        .get("questions")
        .and_then(JsonValue::as_array)
    {
        return json!({
            "title": interaction.options.get("title").and_then(JsonValue::as_str).unwrap_or("需要你的回答"),
            "source": "Native AgentKit",
            "dismissable": true,
            "questions": questions,
        });
    }

    let options = interaction
        .options
        .get("options")
        .or_else(|| interaction.options.as_array().map(|_| &interaction.options))
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, option)| {
            if let Some(label) = option.as_str() {
                return Some(json!({ "id": label, "label": label }));
            }
            let object = option.as_object()?;
            let label = object
                .get("label")
                .or_else(|| object.get("title"))
                .or_else(|| object.get("name"))
                .and_then(JsonValue::as_str)?;
            Some(json!({
                "id": object
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("option-{}", index + 1)),
                "label": label,
                "description": object.get("description"),
                "preview": object.get("preview"),
                "recommended": object.get("recommended"),
            }))
        })
        .collect::<Vec<_>>();
    json!({
        "title": interaction.options.get("title").and_then(JsonValue::as_str).unwrap_or("需要你的回答"),
        "source": "Native AgentKit",
        "dismissable": true,
        "questions": [{
            "id": interaction.interaction_id,
            "header": interaction.options.get("header").and_then(JsonValue::as_str).unwrap_or("确认"),
            "question": interaction.prompt,
            "mode": "single",
            "options": options,
            "allowOther": true,
        }],
    })
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
        AgentEventMeta, AgentUsage, ArtifactRef, InteractionKind, InteractionRequest,
        InteractionResolution, TodoItem, TodoItemStatus, TodoState,
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
    fn projects_custom_agent_completion_as_subagent_timeline_kind() {
        let task = TaskId::new("task-subagent").unwrap();
        let commands = project_agent_event(
            &task,
            &AgentEventEnvelope {
                session_id: "session-subagent".into(),
                sequence: 9,
                meta: AgentEventMeta::new("evt-subagent", "subagent"),
                event: AgentEvent::ToolCallCompleted {
                    turn_id: "turn-subagent".into(),
                    call_id: "delegate-1".into(),
                    summary: "delegate_agent".into(),
                    details: None,
                },
            },
        );
        match &commands[..] {
            [TimelineProjectionCommand::UpsertTimelineEvent { event }] => {
                assert_eq!(event.kind, "subagent");
                assert_eq!(event.payload["agentType"], "custom");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn projects_usage_with_authoritative_timestamp_and_backend() {
        let task = TaskId::new("task-usage").unwrap();
        let mut meta = AgentEventMeta::new("evt-usage", "usage");
        meta.timestamp_unix_ms = 1_725_000_000_000;
        let commands = project_agent_event(
            &task,
            &AgentEventEnvelope {
                session_id: "session-usage".into(),
                sequence: 8,
                meta,
                event: AgentEvent::Usage {
                    turn_id: "turn-usage".into(),
                    usage: AgentUsage {
                        input_tokens: 21,
                        output_tokens: 13,
                        total_tokens: 34,
                    },
                },
            },
        );
        match &commands[..] {
            [TimelineProjectionCommand::UpsertTimelineEvent { event }] => {
                assert_eq!(event.kind, "usage");
                assert_eq!(event.payload["inputTokens"], 21);
                assert_eq!(event.payload["outputTokens"], 13);
                assert_eq!(event.payload["totalTokens"], 34);
                assert_eq!(event.payload["createdAt"], 1_725_000_000_000_u64);
                assert_eq!(event.payload["backend"], "native-agentkit");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn projects_user_message_with_structured_attachments() {
        let task = TaskId::new("task-user-message").unwrap();
        let envelope = AgentEventEnvelope {
            session_id: "sess-user-message".into(),
            sequence: 1,
            meta: AgentEventMeta::new("evt-user", "user message"),
            event: AgentEvent::UserMessage {
                turn_id: "turn-1".into(),
                content: "Inspect this".into(),
                metadata: Some(json!({
                    "attachments": [{
                        "id": "att-1",
                        "name": "README.md",
                        "path": "C:/repo/README.md",
                        "kind": "file",
                        "size": 42,
                        "exists": true,
                        "mime": null,
                        "directory": null
                    }]
                })),
            },
        };

        let commands = project_agent_event(&task, &envelope);

        match &commands[..] {
            [TimelineProjectionCommand::UpsertTimelineEvent { event }] => {
                assert_eq!(event.kind, "message");
                assert_eq!(event.title, "用户输入");
                assert_eq!(event.payload["role"], "user");
                assert_eq!(event.payload["attachments"][0]["id"], "att-1");
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
                    session_id: "sess-side".into(),
                    turn_id: "t1".into(),
                    version: 1,
                    interaction_id: "int-1".into(),
                    kind: InteractionKind::Clarification,
                    source_tool: Some("ask_user_question".into()),
                    permission_mode: AgentPermissionMode::Ask,
                    prompt: "which?".into(),
                    options: json!(["a", "b"]),
                    context: None,
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
                    session_id: "sess-side".into(),
                    turn_id: "t1".into(),
                    version: 1,
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
        let pending = pending_cmds
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertPending { pending } => Some(pending),
                _ => None,
            })
            .expect("pending projection");
        assert_eq!(pending.kind, "ask_user");
        assert_eq!(pending.payload["interaction"], "ask_user");
        assert_eq!(pending.payload["spec"]["questions"][0]["id"], "int-1");
        assert_eq!(pending.payload["spec"]["questions"][0]["allowOther"], true);
        let interaction_event = pending_cmds
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertTimelineEvent { event } => Some(event),
                _ => None,
            })
            .expect("interaction timeline event");
        assert_eq!(interaction_event.kind, "ask_user");
        assert_eq!(interaction_event.payload["interaction"], "ask_user");

        let resolved_cmds = project_agent_event(&task, &resolved_env);
        assert!(resolved_cmds
            .iter()
            .any(|c| matches!(c, TimelineProjectionCommand::ResolvePending { .. })));
    }

    #[test]
    fn projects_plan_confirmation_as_product_plan_approval() {
        let task = TaskId::new("task-plan").unwrap();
        let envelope = AgentEventEnvelope {
            session_id: "sess-plan".into(),
            sequence: 9,
            meta: AgentEventMeta::new("evt-plan", "plan"),
            event: AgentEvent::InteractionRequested {
                turn_id: "turn-plan".into(),
                interaction: InteractionRequest {
                    session_id: "sess-plan".into(),
                    turn_id: "turn-plan".into(),
                    version: 3,
                    interaction_id: "plan-1".into(),
                    kind: InteractionKind::PlanConfirm,
                    source_tool: Some("confirm_plan".into()),
                    permission_mode: AgentPermissionMode::Ask,
                    prompt: "Execute this plan?".into(),
                    options: json!({ "plan": "1. inspect\n2. change" }),
                    context: None,
                    details: None,
                },
            },
        };

        let commands = project_agent_event(&task, &envelope);
        let pending = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertPending { pending } => Some(pending),
                _ => None,
            })
            .expect("pending projection");
        assert_eq!(pending.kind, "plan_approval");
        assert_eq!(pending.payload["spec"]["intent"], "plan_approval");
        assert_eq!(
            pending.payload["spec"]["questions"][0]["id"],
            "approve-plan"
        );
        let timeline = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertTimelineEvent { event } => Some(event),
                _ => None,
            })
            .expect("timeline projection");
        assert_eq!(timeline.kind, "plan");
        assert_eq!(timeline.payload["plan"], "1. inspect\n2. change");
    }

    #[test]
    fn projects_architecture_tool_with_authoritative_scope_and_permission() {
        let task = TaskId::new("task-architecture").unwrap();
        let envelope = AgentEventEnvelope {
            session_id: "sess-architecture".into(),
            sequence: 10,
            meta: AgentEventMeta::new("evt-architecture", "interaction"),
            event: AgentEvent::InteractionRequested {
                turn_id: "turn-architecture".into(),
                interaction: InteractionRequest {
                    session_id: "sess-architecture".into(),
                    turn_id: "turn-architecture".into(),
                    version: 4,
                    interaction_id: "architecture-1".into(),
                    kind: InteractionKind::Custom,
                    source_tool: Some("update_project_architecture".into()),
                    permission_mode: AgentPermissionMode::Ask,
                    prompt: "Add the native application boundary".into(),
                    options: json!({
                        "reason": "Represent the UI-to-application dependency",
                        "changes": [{
                            "type": "set_summary",
                            "summary": "Native UI depends on typed application services."
                        }]
                    }),
                    context: Some(json!({
                        "productTaskId": "task-architecture",
                        "productProjectId": "project-architecture",
                        "projectArchitectureVersion": 7
                    })),
                    details: None,
                },
            },
        };

        let commands = project_agent_event(&task, &envelope);
        let pending = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertPending { pending } => Some(pending),
                _ => None,
            })
            .expect("architecture pending projection");
        assert_eq!(pending.kind, "architecture_change");
        assert_eq!(pending.payload["projectId"], "project-architecture");
        assert_eq!(pending.payload["taskId"], "task-architecture");
        assert_eq!(pending.payload["permission"], "ask");
        assert_eq!(pending.payload["expectedVersion"], 7);
        assert_eq!(pending.payload["requiresConfirmation"], true);
        let timeline = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertTimelineEvent { event } => Some(event),
                _ => None,
            })
            .expect("architecture timeline projection");
        assert_eq!(timeline.kind, "architecture_change");
        assert_eq!(timeline.title, "Native 架构变更");
    }

    #[test]
    fn projects_custom_mcp_elicitation_without_losing_its_form_contract() {
        let task = TaskId::new("task-mcp").unwrap();
        let envelope = AgentEventEnvelope {
            session_id: "sess-mcp".into(),
            sequence: 10,
            meta: AgentEventMeta::new("evt-mcp", "interaction"),
            event: AgentEvent::InteractionRequested {
                turn_id: "turn-mcp".into(),
                interaction: InteractionRequest {
                    session_id: "sess-mcp".into(),
                    turn_id: "turn-mcp".into(),
                    version: 4,
                    interaction_id: "mcp-1".into(),
                    kind: InteractionKind::Custom,
                    source_tool: None,
                    permission_mode: AgentPermissionMode::Ask,
                    prompt: "选择 Linear 项目".into(),
                    options: json!({
                        "interaction": "mcp_elicitation",
                        "threadId": "thread-1",
                        "serverName": "linear",
                        "mode": "form",
                        "message": "选择 Linear 项目",
                        "requestedSchema": {
                            "type": "object",
                            "required": ["project"],
                            "properties": {
                                "project": {"type": "string", "enum": ["A", "B"]}
                            }
                        },
                        "elicitationId": "elicitation-1",
                        "_meta": {"trace": "trace-1"}
                    }),
                    context: None,
                    details: None,
                },
            },
        };

        let commands = project_agent_event(&task, &envelope);
        let pending = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertPending { pending } => Some(pending),
                _ => None,
            })
            .expect("MCP pending projection");
        assert_eq!(pending.kind, "mcp_elicitation");
        assert_eq!(pending.payload["threadId"], "thread-1");
        assert_eq!(pending.payload["serverName"], "linear");
        assert_eq!(pending.payload["mode"], "form");
        assert_eq!(
            pending.payload["requestedSchema"]["properties"]["project"]["enum"],
            json!(["A", "B"])
        );
        assert_eq!(pending.payload["_meta"]["trace"], "trace-1");
        assert!(pending.payload.get("spec").is_none());

        let timeline = commands
            .iter()
            .find_map(|command| match command {
                TimelineProjectionCommand::UpsertTimelineEvent { event } => Some(event),
                _ => None,
            })
            .expect("MCP timeline projection");
        assert_eq!(timeline.payload["interaction"], "mcp_elicitation");
        assert_eq!(timeline.payload["requestId"], "mcp-1");
        assert_eq!(timeline.payload["serverName"], "linear");
    }
}
