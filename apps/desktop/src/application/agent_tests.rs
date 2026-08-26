use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::*;
use crate::application::{
    DesktopApplicationConfig, DesktopGoalSnapshot, DesktopGoalStatus, DesktopHost,
    DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult,
};
use lilia_contracts::{
    ChatAttachment, ChatAttachmentKind, ChatConversationReference, LiliaAgentWorkflow,
    ProductEntity,
};
use lilia_service::ServiceAuthority;

static NEXT_AGENT_APPLICATION_ID: AtomicU64 = AtomicU64::new(1);

struct NoopHost;

impl DesktopHost for NoopHost {
    fn execute(
        &self,
        _context: &DesktopHostContext,
        _action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopHostError> {
        Ok(DesktopHostResult::Completed)
    }
}

fn mcp_pending() -> PendingProjection {
    PendingProjection {
        id: "pending-mcp".to_owned(),
        task_id: TaskId::new("task-mcp").unwrap(),
        agent_session: AgentSessionRef::new("session-mcp").unwrap(),
        sequence: 1,
        turn_id: Some("turn-mcp".to_owned()),
        request_id: "request-mcp".to_owned(),
        kind: "mcp_elicitation".to_owned(),
        status: PendingProjectionStatus::Open,
        prompt: Some("选择项目".to_owned()),
        action_revision: Some(1),
        payload: json!({
            "threadId": "thread-mcp",
            "turnId": "turn-mcp",
            "serverName": "linear",
            "mode": "form",
            "message": "选择项目",
            "requestedSchema": {
                "type": "object",
                "required": ["project"],
                "properties": {
                    "project": {"type": "string", "enum": ["A", "B"]}
                }
            }
        }),
    }
}

#[test]
fn session_fork_replaces_the_task_binding_without_leaving_the_parent_preferred() {
    let id = NEXT_AGENT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
    let authority = ServiceAuthority::bootstrap_in_memory_named(
        format!("test:desktop-agent-session-fork:{id}"),
        format!("desktop-agent-session-fork:{id}"),
    )
    .unwrap();
    let task_id = TaskId::new(format!("session-fork-task-{id}")).unwrap();
    authority
        .client()
        .unwrap()
        .products()
        .create_entity(ProductEntity::Task(
            ProductTask::new(task_id.clone(), None, "Session fork").unwrap(),
        ))
        .unwrap();
    let application = DesktopApplication::from_authority(
        DesktopApplicationConfig::new(
            "C:/lilia/native-session-fork-test",
            format!("liliacode.native-session-fork-test.{id}"),
        )
        .unwrap(),
        authority,
        Arc::new(NoopHost),
    )
    .unwrap();

    application
        .persist_session_binding(&task_id, "parent-session", "profile")
        .unwrap();
    application
        .replace_session_binding(&task_id, "forked-session", "profile")
        .unwrap();

    let bindings = application
        .authority()
        .list_session_bindings(&task_id)
        .unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].agent_session.as_str(), "forked-session");
}

#[test]
fn typed_workflow_can_start_without_message_content_and_reaches_turn_context() {
    let id = NEXT_AGENT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
    let authority = ServiceAuthority::bootstrap_in_memory_named(
        format!("test:desktop-agent-workflow:{id}"),
        format!("desktop-agent-workflow:{id}"),
    )
    .unwrap();
    let task_id = TaskId::new(format!("workflow-task-{id}")).unwrap();
    authority
        .client()
        .unwrap()
        .products()
        .create_entity(ProductEntity::Task(
            ProductTask::new(task_id.clone(), None, "Workflow turn").unwrap(),
        ))
        .unwrap();
    let application = DesktopApplication::from_authority(
        DesktopApplicationConfig::new(
            "C:/lilia/native-workflow-test",
            format!("liliacode.native-workflow-test.{id}"),
        )
        .unwrap(),
        authority,
        Arc::new(NoopHost),
    )
    .unwrap();
    let mut request = DesktopTurnRequest::new(task_id.clone(), "");
    request.workflow = Some(LiliaAgentWorkflow::LiliaCompact);

    let prepared = application.prepare_task_turn_request(request).unwrap();
    let context = turn_context(&task_id, "turn-workflow", &prepared, None, None, None, None);

    assert_eq!(context["workflow"]["type"], "lilia_compact");
}

#[test]
fn mcp_interaction_response_preserves_actions_and_validates_form_content() {
    assert!(supported_pending_interaction_kind("mcp_elicitation"));
    let pending = mcp_pending();
    let accepted = normalized_pending_interaction_response(
        &pending,
        true,
        json!({"action": "accept", "content": {"project": "B"}}),
    )
    .unwrap();
    assert!(accepted.0);
    assert_eq!(accepted.1["content"]["project"], "B");
    assert!(normalized_pending_interaction_response(
        &pending,
        true,
        json!({"action": "accept", "content": {}}),
    )
    .is_err());
    assert_eq!(
        normalized_pending_interaction_response(&pending, false, json!({"action": "decline"}),)
            .unwrap(),
        (false, json!({"action": "decline"}))
    );
    assert!(
        normalized_pending_interaction_response(&pending, true, json!({"action": "cancel"}),)
            .is_err()
    );
}

#[test]
fn tool_consent_response_is_supported_and_decision_fenced() {
    assert!(supported_pending_interaction_kind("tool_consent"));
    assert!(!supported_pending_interaction_kind("agent_interaction"));
    let mut pending = mcp_pending();
    pending.kind = "tool_consent".to_owned();
    pending.payload = json!({
        "toolName": "shell",
        "input": {"command": "cargo test"}
    });

    let response = json!({
        "taskId": "task-1",
        "requestId": pending.request_id.clone(),
        "decision": "allow",
        "message": null,
        "updatedInput": {"command": "cargo test --locked"}
    });
    assert_eq!(
        normalized_pending_interaction_response(&pending, true, response.clone()).unwrap(),
        (true, response)
    );
    assert!(
        normalized_pending_interaction_response(&pending, false, json!({"decision": "allow"}),)
            .is_err()
    );
    assert!(normalized_pending_interaction_response(
        &pending,
        false,
        json!({"decision": "deny", "updatedInput": "invalid"}),
    )
    .is_err());
}

#[test]
fn native_turn_context_preserves_structured_attachments() {
    let task_id = TaskId::new("task-attachment").unwrap();
    let request = DesktopTurnRequest::new(task_id.clone(), "inspect").with_attachments(vec![
        ChatAttachment {
            id: "att-1".to_owned(),
            name: "README.md".to_owned(),
            path: "C:/repo/README.md".to_owned(),
            kind: ChatAttachmentKind::File,
            size: Some(42),
            exists: true,
            mime: None,
            directory: None,
        },
    ]);

    let context = turn_context(&task_id, "turn-1", &request, None, None, None, None);

    assert_eq!(context["attachments"][0]["id"], "att-1");
    assert_eq!(context["attachments"][0]["kind"], "file");
    assert_eq!(context["attachments"][0]["path"], "C:/repo/README.md");
}

#[test]
fn conversation_references_are_structured_and_serialized_once() {
    let task_id = TaskId::new("task-reference").unwrap();
    let reference = ChatConversationReference {
        task_id: "related-task".to_owned(),
        title: "相关设计".to_owned(),
        route: "/chats/related-task".to_owned(),
        project_id: None,
        project_name: None,
    };
    let request = DesktopTurnRequest::new(task_id.clone(), "inspect")
        .with_conversation_references(vec![reference.clone()]);

    assert_eq!(
        turn_content_with_references(&request),
        "inspect\n[对话引用: 相关设计 | related-task]"
    );
    let already_referenced = DesktopTurnRequest::new(
        task_id.clone(),
        "inspect\n[对话引用: 相关设计 | related-task]",
    )
    .with_conversation_references(vec![reference]);
    assert_eq!(
        turn_content_with_references(&already_referenced),
        "inspect\n[对话引用: 相关设计 | related-task]"
    );
    let context = turn_context(&task_id, "turn-reference", &request, None, None, None, None);
    assert_eq!(
        context["conversationReferences"][0]["taskId"],
        "related-task"
    );
}

#[test]
fn native_turn_context_includes_the_task_goal_snapshot() {
    let task_id = TaskId::new("task-goal").unwrap();
    let request = DesktopTurnRequest::new(task_id.clone(), "continue");
    let goal = DesktopGoalSnapshot {
        thread_id: task_id.as_str().to_owned(),
        objective: "finish Native parity".to_owned(),
        status: DesktopGoalStatus::Active,
        token_budget: Some(4_096),
        tokens_used: 512,
        time_used_seconds: 30,
        created_at: 100,
        updated_at: 200,
    };

    let context = turn_context(
        &task_id,
        "turn-goal",
        &request,
        Some(&goal),
        None,
        None,
        None,
    );

    assert_eq!(context["goal"]["objective"], "finish Native parity");
    assert_eq!(context["goal"]["status"], "active");
    assert_eq!(context["goal"]["tokenBudget"], 4_096);
}

#[test]
fn attachment_references_are_appended_once_and_support_attachment_only_turns() {
    let attachment = ChatAttachment {
        id: "att-1".to_owned(),
        name: "src".to_owned(),
        path: "C:/repo/src".to_owned(),
        kind: ChatAttachmentKind::Directory,
        size: None,
        exists: true,
        mime: None,
        directory: None,
    };
    let task_id = TaskId::new("task-attachment").unwrap();
    let only_attachment =
        DesktopTurnRequest::new(task_id.clone(), "").with_attachments(vec![attachment.clone()]);
    assert_eq!(
        turn_content_with_references(&only_attachment),
        "[目录引用: src | C:/repo/src]"
    );

    let referenced = DesktopTurnRequest::new(task_id, "Inspect\n[目录引用: src | C:/repo/src]")
        .with_attachments(vec![attachment]);
    assert_eq!(
        turn_content_with_references(&referenced),
        "Inspect\n[目录引用: src | C:/repo/src]"
    );
}
