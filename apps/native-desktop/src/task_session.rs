use lilia_contracts::{
    PendingProjectionStatus, TimelineProjectionCursor, TimelineProjectionEvent,
    TimelineProjectionPage,
};
use lilia_desktop_application::{
    ChatAttachment, ChatContextUsage, DesktopGoalSnapshot, DesktopTaskSessionSnapshot,
    DesktopTaskTodo, DesktopTaskWorktree,
};
use nana_ui::{MarkdownBlock, NativeMarkdown, VirtualListLayout};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskTimelineItem {
    pub(crate) id: String,
    pub(crate) sequence: u64,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) summary: Option<String>,
    pub(crate) markdown: Option<String>,
    pub(crate) markdown_document: Option<NativeMarkdown>,
    pub(crate) markdown_plain_text: Option<String>,
    pub(crate) markdown_table_count: usize,
    pub(crate) attachments: Vec<ChatAttachment>,
    pub(crate) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskArtifactView {
    pub(crate) id: String,
    pub(crate) summary: String,
    pub(crate) media_type: String,
    pub(crate) status: String,
    pub(crate) size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskSessionView {
    pub(crate) task_title: String,
    pub(crate) goal: Option<DesktopGoalSnapshot>,
    pub(crate) context_usage: Option<ChatContextUsage>,
    pub(crate) timeline: Vec<TaskTimelineItem>,
    pub(crate) timeline_layout: VirtualListLayout,
    pub(crate) timeline_before_cursor: Option<TimelineProjectionCursor>,
    pub(crate) timeline_has_more_before: bool,
    pub(crate) artifact_count: usize,
    pub(crate) todo_count: usize,
    pub(crate) artifacts: Vec<TaskArtifactView>,
    pub(crate) todos: Vec<DesktopTaskTodo>,
    pub(crate) worktree: Option<DesktopTaskWorktree>,
    pub(crate) pending_count: usize,
    pub(crate) open_pending_count: usize,
    pub(crate) pending: Vec<PendingActionView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingActionView {
    pub(crate) request_id: String,
    pub(crate) kind: String,
    pub(crate) prompt: String,
    pub(crate) status: PendingProjectionStatus,
    pub(crate) payload: Value,
}

impl TaskSessionView {
    pub(crate) fn from_snapshot(snapshot: DesktopTaskSessionSnapshot) -> Self {
        let pending = snapshot
            .pending
            .iter()
            .map(|item| PendingActionView {
                request_id: item.request_id.clone(),
                kind: item.kind.clone(),
                prompt: item
                    .prompt
                    .clone()
                    .unwrap_or_else(|| "需要确认后继续".to_owned()),
                status: item.status.clone(),
                payload: item.payload.clone(),
            })
            .collect();
        let timeline = snapshot
            .timeline
            .into_iter()
            .map(task_timeline_item)
            .collect();
        let artifacts = snapshot
            .artifacts
            .iter()
            .map(|artifact| TaskArtifactView {
                id: artifact.artifact_id.clone(),
                summary: artifact.summary.clone(),
                media_type: artifact.media_type.clone(),
                status: artifact.status.clone(),
                size_bytes: artifact.size_bytes,
            })
            .collect::<Vec<_>>();
        let todos = snapshot.task_todos;
        let pending_statuses = snapshot
            .pending
            .iter()
            .map(|pending| pending.status.clone())
            .collect();
        let mut view = Self::from_facts(
            snapshot.task.title,
            timeline,
            artifacts.len(),
            todos.len(),
            pending_statuses,
            pending,
        );
        view.timeline_before_cursor = snapshot.timeline_before_cursor;
        view.timeline_has_more_before = snapshot.timeline_has_more_before;
        view.goal = snapshot.goal;
        view.context_usage = snapshot.context_usage;
        view.artifacts = artifacts;
        view.todos = todos;
        view.worktree = snapshot.worktree;
        view
    }

    pub(crate) fn refresh_preserving_history(
        snapshot: DesktopTaskSessionSnapshot,
        previous: Option<&Self>,
    ) -> Self {
        let mut refreshed = Self::from_snapshot(snapshot);
        let Some(previous) = previous else {
            return refreshed;
        };
        let mut timeline = previous
            .timeline
            .iter()
            .cloned()
            .map(|event| (event.id.clone(), event))
            .collect::<std::collections::BTreeMap<_, _>>();
        timeline.extend(
            refreshed
                .timeline
                .into_iter()
                .map(|event| (event.id.clone(), event)),
        );
        refreshed.timeline = timeline.into_values().collect();
        refreshed
            .timeline
            .sort_by_key(|event| (event.sequence, event.id.clone()));
        refreshed.sync_timeline_layout();
        refreshed.timeline_has_more_before = previous.timeline_has_more_before;
        refreshed.timeline_before_cursor = refreshed
            .timeline_has_more_before
            .then(|| refreshed.timeline.first())
            .flatten()
            .map(timeline_item_cursor);
        refreshed
    }

    pub(crate) fn prepend_timeline_page(&mut self, page: TimelineProjectionPage) -> f32 {
        let previous_first_id = self.timeline.first().map(|event| event.id.clone());
        let mut timeline = self
            .timeline
            .drain(..)
            .map(|event| (event.id.clone(), event))
            .collect::<std::collections::BTreeMap<_, _>>();
        timeline.extend(
            page.events
                .into_iter()
                .map(task_timeline_item)
                .map(|event| (event.id.clone(), event)),
        );
        self.timeline = timeline.into_values().collect();
        self.timeline
            .sort_by_key(|event| (event.sequence, event.id.clone()));
        self.sync_timeline_layout();
        self.timeline_before_cursor = page.before_cursor;
        self.timeline_has_more_before = page.has_more_before;
        previous_first_id
            .and_then(|id| self.timeline.iter().position(|event| event.id == id))
            .map(|index| self.timeline_layout.extent(0..index))
            .unwrap_or_default()
    }

    fn from_facts(
        task_title: String,
        mut timeline: Vec<TaskTimelineItem>,
        artifact_count: usize,
        todo_count: usize,
        pending_statuses: Vec<PendingProjectionStatus>,
        pending: Vec<PendingActionView>,
    ) -> Self {
        timeline.sort_by_key(|event| event.sequence);
        let open_pending_count = pending_statuses
            .iter()
            .filter(|status| **status == PendingProjectionStatus::Open)
            .count();
        let mut view = Self {
            task_title,
            goal: None,
            context_usage: None,
            timeline,
            timeline_layout: VirtualListLayout::default(),
            timeline_before_cursor: None,
            timeline_has_more_before: false,
            artifact_count,
            todo_count,
            artifacts: Vec::new(),
            todos: Vec::new(),
            worktree: None,
            pending_count: pending_statuses.len(),
            open_pending_count,
            pending,
        };
        view.sync_timeline_layout();
        view
    }

    fn sync_timeline_layout(&mut self) {
        self.timeline_layout
            .set_item_extents(self.timeline.iter().map(estimate_timeline_item_extent));
    }
}

fn estimate_timeline_item_extent(event: &TaskTimelineItem) -> f32 {
    const BASE_EXTENT: f32 = 58.0;
    const LINE_EXTENT: f32 = 18.0;
    const ATTACHMENT_EXTENT: f32 = 38.0;
    const ITEM_SPACING: f32 = 6.0;
    const MAX_TEXT_LINES: usize = 24;

    let text = event
        .markdown
        .as_deref()
        .or(event.summary.as_deref())
        .unwrap_or_default();
    let wrapped_lines = text
        .lines()
        .map(|line| line.chars().count().max(1).div_ceil(72))
        .sum::<usize>()
        .min(MAX_TEXT_LINES);
    BASE_EXTENT
        + wrapped_lines as f32 * LINE_EXTENT
        + event.attachments.len() as f32 * ATTACHMENT_EXTENT
        + ITEM_SPACING
}

fn task_timeline_item(event: TimelineProjectionEvent) -> TaskTimelineItem {
    let markdown = timeline_markdown(&event.kind, &event.payload, event.summary.as_deref());
    let markdown_document = markdown.as_deref().map(NativeMarkdown::parse);
    let markdown_plain_text = markdown_document.as_ref().map(NativeMarkdown::plain_text);
    let markdown_table_count = markdown_document
        .as_ref()
        .map(|document| {
            document
                .blocks()
                .iter()
                .filter(|block| matches!(block, MarkdownBlock::Table(_)))
                .count()
        })
        .unwrap_or_default();
    TaskTimelineItem {
        id: event.id.as_str().to_owned(),
        sequence: event.sequence,
        kind: event.kind.clone(),
        title: event.title,
        markdown,
        markdown_document,
        markdown_plain_text,
        markdown_table_count,
        attachments: timeline_attachments(&event.payload),
        summary: event.summary,
        status: event.status,
    }
}

fn timeline_item_cursor(event: &TaskTimelineItem) -> TimelineProjectionCursor {
    TimelineProjectionCursor {
        sequence: event.sequence,
        event_id: event.id.clone(),
    }
}

fn timeline_markdown(kind: &str, payload: &Value, summary: Option<&str>) -> Option<String> {
    if !matches!(kind, "message" | "reasoning" | "plan") {
        return None;
    }
    payload
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| payload.get("markdown").and_then(Value::as_str))
        .or(summary)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
}

fn timeline_attachments(payload: &Value) -> Vec<ChatAttachment> {
    [
        payload.get("attachments"),
        payload.pointer("/context/attachments"),
        payload.pointer("/metadata/attachments"),
    ]
    .into_iter()
    .flatten()
    .find_map(|attachments| serde_json::from_value(attachments.clone()).ok())
    .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InteractionChoice {
    pub(crate) label: String,
    pub(crate) value: Value,
}

pub(crate) fn interaction_choices(pending: &PendingActionView) -> Vec<InteractionChoice> {
    let options = pending.payload.get("options").unwrap_or(&Value::Null);
    let values = options
        .as_array()
        .or_else(|| options.get("choices").and_then(Value::as_array))
        .or_else(|| options.get("options").and_then(Value::as_array));
    values
        .into_iter()
        .flatten()
        .filter_map(|option| {
            if let Some(label) = option.as_str() {
                return Some(InteractionChoice {
                    label: label.to_owned(),
                    value: Value::String(label.to_owned()),
                });
            }
            let object = option.as_object()?;
            let value = object
                .get("value")
                .or_else(|| object.get("id"))
                .cloned()
                .unwrap_or_else(|| option.clone());
            let label = object
                .get("label")
                .or_else(|| object.get("title"))
                .or_else(|| object.get("name"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| value.as_str().map(str::to_owned))?;
            Some(InteractionChoice { label, value })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lilia_contracts::{AgentSessionRef, ProjectionEventId, TaskId, TimelineProjectionEvent};
    use lilia_desktop_application::ChatAttachmentKind;
    use serde_json::json;

    use super::*;

    fn projection_event(sequence: u64) -> TimelineProjectionEvent {
        TimelineProjectionEvent {
            id: ProjectionEventId::from_session_sequence("session-page", sequence),
            task_id: TaskId::new("task-page").unwrap(),
            agent_session: AgentSessionRef::new("session-page").unwrap(),
            sequence,
            turn_id: Some("turn-page".to_owned()),
            kind: "message".to_owned(),
            status: "completed".to_owned(),
            title: format!("Event {sequence}"),
            summary: Some(format!("Message {sequence}")),
            payload: json!({ "content": format!("Message {sequence}") }),
            projected: true,
        }
    }

    #[test]
    fn task_session_state_orders_timeline_and_derives_pending_counts() {
        let state = TaskSessionView::from_facts(
            "实现原生预览".to_owned(),
            vec![
                TaskTimelineItem {
                    id: "event-2".to_owned(),
                    sequence: 2,
                    kind: "message".to_owned(),
                    title: "完成".to_owned(),
                    summary: None,
                    markdown: None,
                    markdown_document: None,
                    markdown_plain_text: None,
                    markdown_table_count: 0,
                    attachments: Vec::new(),
                    status: "completed".to_owned(),
                },
                TaskTimelineItem {
                    id: "event-1".to_owned(),
                    sequence: 1,
                    kind: "command".to_owned(),
                    title: "开始".to_owned(),
                    summary: Some("准备环境".to_owned()),
                    markdown: None,
                    markdown_document: None,
                    markdown_plain_text: None,
                    markdown_table_count: 0,
                    attachments: Vec::new(),
                    status: "running".to_owned(),
                },
            ],
            3,
            4,
            vec![
                PendingProjectionStatus::Resolved,
                PendingProjectionStatus::Open,
                PendingProjectionStatus::Cancelled,
            ],
            vec![PendingActionView {
                request_id: "approval-1".to_owned(),
                kind: "permission_approval".to_owned(),
                prompt: "允许写入文件".to_owned(),
                status: PendingProjectionStatus::Open,
                payload: Value::Null,
            }],
        );

        assert_eq!(
            state
                .timeline
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(state.artifact_count, 3);
        assert_eq!(state.todo_count, 4);
        assert_eq!(state.pending_count, 3);
        assert_eq!(state.open_pending_count, 1);
        assert_eq!(state.pending[0].request_id, "approval-1");
    }

    #[test]
    fn timeline_markdown_reads_semantic_content_only_for_rich_kinds() {
        assert_eq!(
            timeline_markdown(
                "message",
                &json!({ "content": "# Native" }),
                Some("fallback")
            )
            .as_deref(),
            Some("# Native")
        );
        assert_eq!(
            timeline_markdown("command", &json!({ "content": "ignored" }), None),
            None
        );
    }

    #[test]
    fn timeline_markdown_caches_structured_tables_and_complete_copy_text() {
        let mut event = projection_event(1);
        event.payload = json!({
            "content": "| 名称 | 数量 |\n| :--- | ---: |\n| **Alpha** | 42 |"
        });

        let item = task_timeline_item(event);

        assert_eq!(item.markdown_table_count, 1);
        assert_eq!(
            item.markdown_plain_text.as_deref(),
            Some("名称\t数量\nAlpha\t42")
        );
    }

    #[test]
    fn older_page_is_prepended_without_duplicates_and_finishes_pagination() {
        let mut state = TaskSessionView::from_facts(
            "Paged task".to_owned(),
            vec![
                task_timeline_item(projection_event(3)),
                task_timeline_item(projection_event(4)),
            ],
            0,
            0,
            Vec::new(),
            Vec::new(),
        );
        state.timeline_has_more_before = true;
        state.timeline_before_cursor = Some(TimelineProjectionCursor {
            sequence: 3,
            event_id: "session-page:3".to_owned(),
        });

        let anchor_delta = state.prepend_timeline_page(TimelineProjectionPage {
            events: vec![
                projection_event(1),
                projection_event(2),
                projection_event(3),
            ],
            before_cursor: None,
            has_more_before: false,
        });

        assert_eq!(
            state
                .timeline
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert!(!state.timeline_has_more_before);
        assert!(state.timeline_before_cursor.is_none());
        assert_eq!(
            anchor_delta,
            state.timeline_layout.extent(0..2),
            "only newly prepended events contribute to the anchor"
        );
    }

    #[test]
    fn timeline_attachments_accept_projected_and_nested_context_shapes() {
        let projected = timeline_attachments(&json!({
            "attachments": [{
                "id": "att-1",
                "name": "README.md",
                "path": "C:/repo/README.md",
                "kind": "file",
                "size": 4,
                "exists": true,
                "mime": null,
                "directory": null
            }]
        }));
        let nested = timeline_attachments(&json!({
            "context": { "attachments": [{
                "id": "att-2",
                "name": "src",
                "path": "C:/repo/src",
                "kind": "directory",
                "size": null,
                "exists": true,
                "mime": null,
                "directory": null
            }] }
        }));

        assert_eq!(projected[0].id, "att-1");
        assert_eq!(nested[0].kind, ChatAttachmentKind::Directory);
    }

    #[test]
    fn thousand_event_timeline_builds_only_the_viewport_window() {
        let state = TaskSessionView::from_facts(
            "Long task".to_owned(),
            (1..=1_000)
                .map(projection_event)
                .map(task_timeline_item)
                .collect(),
            0,
            0,
            Vec::new(),
            Vec::new(),
        );

        let window =
            state
                .timeline_layout
                .window(state.timeline_layout.total_extent() / 2.0, 720.0, 480.0);

        assert!(window.leading_extent > 0.0);
        assert!(window.trailing_extent > 0.0);
        assert!(window.range.len() < 40, "only visible events are built");
    }
}
