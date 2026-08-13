use lilia_contracts::{ChatAttachment, ChatConversationReference, TaskId, TimelineProjectionEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DesktopApplication, DesktopApplicationError, DesktopTurnDispatch};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTimelineRetryContext {
    pub content: String,
    pub attachments: Vec<ChatAttachment>,
    pub conversation_references: Vec<ChatConversationReference>,
}

pub fn timeline_retry_context(
    error: &TimelineProjectionEvent,
    events: &[TimelineProjectionEvent],
) -> Option<DesktopTimelineRetryContext> {
    if error.kind != "error" {
        return None;
    }
    if let Some(context) = retry_context_value(error.payload.get("retryContext")) {
        return Some(context);
    }
    let turn_id = error.turn_id.as_deref()?;
    let source = events.iter().find(|event| {
        event.kind == "message"
            && event.turn_id.as_deref() == Some(turn_id)
            && event.payload.get("role").and_then(Value::as_str) == Some("user")
    })?;
    retry_context_value(Some(&source.payload))
}

impl DesktopApplication {
    pub fn retry_task_timeline_event(
        &self,
        task_id: &TaskId,
        event_id: &str,
    ) -> Result<DesktopTurnDispatch, DesktopApplicationError> {
        let events = self
            .authority()
            .shared_runtime()
            .inner()
            .product_timeline_for_task(task_id);
        let event = events
            .iter()
            .find(|event| event.id.as_str() == event_id)
            .ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "event_id",
                message: "timeline event does not exist for this task".to_owned(),
            })?;
        let context = timeline_retry_context(event, &events).ok_or_else(|| {
            DesktopApplicationError::InvalidInput {
                field: "event_id",
                message: "timeline event has no retryable message context".to_owned(),
            }
        })?;

        let composer = self.composer_state(task_id)?;
        let mut request = composer.turn_request();
        request.content = context.content;
        request.attachments = context.attachments;
        request.conversation_references = context.conversation_references;
        request.workflow = None;
        self.start_task_turn(request)
    }
}

fn retry_context_value(value: Option<&Value>) -> Option<DesktopTimelineRetryContext> {
    let value = value?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let attachments = decode_array(value.get("attachments"));
    let conversation_references = decode_array(value.get("conversationReferences"));
    (!content.trim().is_empty() || !attachments.is_empty() || !conversation_references.is_empty())
        .then_some(DesktopTimelineRetryContext {
            content,
            attachments,
            conversation_references,
        })
}

fn decode_array<T>(value: Option<&Value>) -> Vec<T>
where
    T: for<'de> Deserialize<'de>,
{
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use lilia_contracts::{AgentSessionRef, ChatAttachmentKind, ProjectionEventId, TaskId};
    use serde_json::json;

    use super::*;

    fn event(
        id: &str,
        turn_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) -> TimelineProjectionEvent {
        TimelineProjectionEvent {
            id: ProjectionEventId::new(id),
            task_id: TaskId::new("retry-task").unwrap(),
            agent_session: AgentSessionRef::new("retry-session").unwrap(),
            sequence: 1,
            turn_id: turn_id.map(str::to_owned),
            kind: kind.to_owned(),
            status: "failed".to_owned(),
            title: "Retry".to_owned(),
            summary: None,
            payload,
            projected: true,
        }
    }

    #[test]
    fn embedded_retry_context_preserves_message_attachments_and_conversation_references() {
        let error = event(
            "error",
            Some("turn-1"),
            "error",
            json!({
                "retryContext": {
                    "content": "retry this",
                    "attachments": [{
                        "id": "attachment-1",
                        "name": "README.md",
                        "path": "C:/workspace/README.md",
                        "kind": "file",
                        "size": 12,
                        "exists": true
                    }],
                    "conversationReferences": [{
                        "taskId": "source-task",
                        "title": "Source task",
                        "route": "/tasks/source-task"
                    }]
                }
            }),
        );

        let context = timeline_retry_context(&error, std::slice::from_ref(&error)).unwrap();

        assert_eq!(context.content, "retry this");
        assert_eq!(context.attachments[0].kind, ChatAttachmentKind::File);
        assert_eq!(context.conversation_references[0].task_id, "source-task");
    }

    #[test]
    fn projected_error_recovers_the_user_message_from_the_same_turn() {
        let source = event(
            "message",
            Some("turn-2"),
            "message",
            json!({"role": "user", "content": "try again"}),
        );
        let error = event("error", Some("turn-2"), "error", json!({}));

        let events = [source, error];
        let context = timeline_retry_context(&events[1], &events).unwrap();

        assert_eq!(context.content, "try again");
    }
}
