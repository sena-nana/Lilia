use lilia_contracts::{
    PendingProjectionStatus, TimelineProjectionCursor, TimelineProjectionEvent,
    TimelineProjectionPage,
};
use lilia_desktop_application::{
    timeline_retry_context, ChatAttachment, ChatContextUsage, DesktopGoalSnapshot,
    DesktopTaskRunBlock, DesktopTaskSessionSnapshot, DesktopTaskTodo, DesktopTaskWorktree,
    TITLE_UPDATE_ACTION_KIND,
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
    pub(crate) batch_apply: Option<TaskTimelineBatchApply>,
    pub(crate) session_branch_turn_id: Option<String>,
    pub(crate) selectable_reply: bool,
    pub(crate) can_retry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskTimelineBatchApply {
    pub(crate) source_turn_id: String,
    pub(crate) source_kind: String,
    pub(crate) source_summary: String,
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
    pub(crate) run_block: Option<DesktopTaskRunBlock>,
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
    pub(crate) blocking_pending_count: usize,
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
    pub(crate) fn with_ephemeral_debug_overlay(
        &self,
        events: &[TaskTimelineItem],
        pending: &[PendingActionView],
    ) -> Self {
        let mut view = self.clone();
        view.timeline.extend_from_slice(events);
        view.timeline
            .sort_by_key(|event| (event.sequence, event.id.clone()));
        view.pending.extend_from_slice(pending);
        view.pending_count = view.pending.len();
        view.open_pending_count = view
            .pending
            .iter()
            .filter(|item| item.status == PendingProjectionStatus::Open)
            .count();
        view.blocking_pending_count = view
            .pending
            .iter()
            .filter(|item| {
                item.status == PendingProjectionStatus::Open
                    && item.kind != TITLE_UPDATE_ACTION_KIND
            })
            .count();
        view.sync_timeline_layout();
        view
    }

    pub(crate) fn has_open_composer_interaction(&self) -> bool {
        self.pending.iter().any(|pending| {
            pending.status == PendingProjectionStatus::Open
                && matches!(
                    pending.kind.as_str(),
                    "ask_user" | "plan_approval" | "tool_consent"
                )
        })
    }

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
        let timeline = task_timeline_items(snapshot.timeline);
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
        view.run_block = snapshot.run_block;
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
            task_timeline_items(page.events)
                .into_iter()
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
        let blocking_pending_count = pending
            .iter()
            .filter(|pending| {
                pending.status == PendingProjectionStatus::Open
                    && pending.kind != TITLE_UPDATE_ACTION_KIND
            })
            .count();
        let mut view = Self {
            task_title,
            run_block: None,
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
            blocking_pending_count,
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

#[cfg(test)]
fn task_timeline_item(event: TimelineProjectionEvent) -> TaskTimelineItem {
    let can_retry = timeline_retry_context(&event, std::slice::from_ref(&event)).is_some();
    task_timeline_item_with_retry(event, can_retry)
}

fn task_timeline_items(events: Vec<TimelineProjectionEvent>) -> Vec<TaskTimelineItem> {
    let retryable = events
        .iter()
        .map(|event| timeline_retry_context(event, &events).is_some())
        .collect::<Vec<_>>();
    events
        .into_iter()
        .zip(retryable)
        .map(|(event, can_retry)| task_timeline_item_with_retry(event, can_retry))
        .collect()
}

fn task_timeline_item_with_retry(
    event: TimelineProjectionEvent,
    can_retry: bool,
) -> TaskTimelineItem {
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
    let batch_apply = timeline_batch_apply(&event);
    let session_branch_turn_id = timeline_session_branch_turn_id(&event);
    let selectable_reply = timeline_selectable_reply(&event);
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
        batch_apply,
        session_branch_turn_id,
        selectable_reply,
        can_retry,
    }
}

fn timeline_selectable_reply(event: &TimelineProjectionEvent) -> bool {
    event.kind == "message"
        && event.payload.get("role").and_then(Value::as_str) == Some("assistant")
        && event
            .payload
            .get("content")
            .and_then(Value::as_str)
            .is_some_and(|content| !content.trim().is_empty())
}

fn timeline_session_branch_turn_id(event: &TimelineProjectionEvent) -> Option<String> {
    if event.kind != "message"
        || !matches!(event.status.as_str(), "success" | "completed" | "done")
        || event.payload.get("role")?.as_str()? != "assistant"
    {
        return None;
    }
    event
        .turn_id
        .as_deref()
        .map(str::trim)
        .filter(|turn_id| !turn_id.is_empty())
        .map(str::to_owned)
}

fn timeline_batch_apply(event: &TimelineProjectionEvent) -> Option<TaskTimelineBatchApply> {
    if event.kind != "message" || event.status != "success" {
        return None;
    }
    let source_turn_id = event.turn_id.as_deref()?.trim();
    if event.payload.get("role")?.as_str()? != "assistant" {
        return None;
    }
    let source_summary = event.payload.get("content")?.as_str()?.trim();
    if source_turn_id.is_empty() || source_summary.is_empty() {
        return None;
    }
    let source_kind = event
        .payload
        .pointer("/workflowSource/sourceKind")?
        .as_str()?;
    if !matches!(source_kind, "review" | "fix_suggestion") {
        return None;
    }
    Some(TaskTimelineBatchApply {
        source_turn_id: source_turn_id.to_owned(),
        source_kind: source_kind.to_owned(),
        source_summary: source_summary.to_owned(),
    })
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
                    batch_apply: None,
                    session_branch_turn_id: None,
                    selectable_reply: false,
                    can_retry: false,
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
                    batch_apply: None,
                    session_branch_turn_id: None,
                    selectable_reply: false,
                    can_retry: false,
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
        assert_eq!(state.blocking_pending_count, 1);
        assert_eq!(state.pending[0].request_id, "approval-1");
    }

    #[test]
    fn title_review_is_visible_without_blocking_the_composer() {
        let state = TaskSessionView::from_facts(
            "Manual title".to_owned(),
            Vec::new(),
            0,
            0,
            vec![PendingProjectionStatus::Open],
            vec![PendingActionView {
                request_id: "title-review-1".to_owned(),
                kind: TITLE_UPDATE_ACTION_KIND.to_owned(),
                prompt: "建议更新标题".to_owned(),
                status: PendingProjectionStatus::Open,
                payload: json!({ "proposedTitle": "新的建议标题" }),
            }],
        );

        assert_eq!(state.open_pending_count, 1);
        assert_eq!(state.blocking_pending_count, 0);
    }

    #[test]
    fn composer_interaction_tracks_only_open_composer_bound_requests() {
        let mut state = TaskSessionView::from_facts(
            "Task".to_owned(),
            Vec::new(),
            0,
            0,
            vec![PendingProjectionStatus::Open],
            vec![PendingActionView {
                request_id: "ask-open".to_owned(),
                kind: "ask_user".to_owned(),
                prompt: "Choose".to_owned(),
                status: PendingProjectionStatus::Open,
                payload: Value::Null,
            }],
        );
        assert!(state.has_open_composer_interaction());

        state.pending[0].status = PendingProjectionStatus::Resolved;
        assert!(!state.has_open_composer_interaction());

        state.pending[0].status = PendingProjectionStatus::Open;
        state.pending[0].kind = "permission_approval".to_owned();
        assert!(!state.has_open_composer_interaction());
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
    fn successful_review_reply_exposes_a_typed_batch_apply_source() {
        let mut event = projection_event(1);
        event.status = "success".to_owned();
        event.turn_id = Some("turn-review".to_owned());
        event.payload = json!({
            "role": "assistant",
            "content": "## 建议\n\n修复权限边界",
            "workflowSource": { "sourceKind": "fix_suggestion" }
        });

        let item = task_timeline_item(event.clone());
        assert_eq!(
            item.batch_apply,
            Some(TaskTimelineBatchApply {
                source_turn_id: "turn-review".to_owned(),
                source_kind: "fix_suggestion".to_owned(),
                source_summary: "## 建议\n\n修复权限边界".to_owned(),
            })
        );

        event.status = "completed".to_owned();
        assert!(task_timeline_item(event).batch_apply.is_none());
    }

    #[test]
    fn completed_assistant_reply_exposes_a_session_branch_anchor() {
        let mut event = projection_event(1);
        event.status = "done".to_owned();
        event.turn_id = Some("turn-anchor".to_owned());
        event.payload = json!({ "role": "assistant", "content": "完成" });

        let reply = task_timeline_item(event.clone());
        assert_eq!(reply.session_branch_turn_id.as_deref(), Some("turn-anchor"));
        assert!(reply.selectable_reply);

        event.payload["role"] = json!("user");
        let user = task_timeline_item(event);
        assert!(user.session_branch_turn_id.is_none());
        assert!(!user.selectable_reply);
    }

    #[test]
    fn failed_event_recovers_retry_context_from_its_turn_user_message() {
        let mut source = projection_event(1);
        source.turn_id = Some("turn-retry".to_owned());
        source.payload = json!({ "role": "user", "content": "retry this request" });
        let mut error = projection_event(2);
        error.kind = "error".to_owned();
        error.status = "failed".to_owned();
        error.turn_id = Some("turn-retry".to_owned());
        error.payload = json!({});

        let timeline = task_timeline_items(vec![source, error]);

        assert!(!timeline[0].can_retry);
        assert!(timeline[1].can_retry);
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
