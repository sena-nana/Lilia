use std::collections::BTreeMap;

use lilia_contracts::{PendingProjectionStatus, TaskId};
use nana_ui::NativeMarkdown;
use serde_json::{json, Value};

use crate::task_session::{PendingActionView, TaskTimelineItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DebugTimelineAction {
    Plan,
    AskUser,
    AskUserMulti,
    AskUserPreview,
    AskUserFlow,
    Permission,
    TodoTool,
    Todo,
    Command,
    FileRead,
    FileChange,
}

impl DebugTimelineAction {
    pub(crate) const ALL: [Self; 11] = [
        Self::Plan,
        Self::AskUser,
        Self::AskUserMulti,
        Self::AskUserPreview,
        Self::AskUserFlow,
        Self::Permission,
        Self::TodoTool,
        Self::Todo,
        Self::Command,
        Self::FileRead,
        Self::FileChange,
    ];

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::AskUser => "ask-user",
            Self::AskUserMulti => "ask-user-multi",
            Self::AskUserPreview => "ask-user-preview",
            Self::AskUserFlow => "ask-user-flow",
            Self::Permission => "permission",
            Self::TodoTool => "todo-tool",
            Self::Todo => "todo",
            Self::Command => "command",
            Self::FileRead => "file-read",
            Self::FileChange => "file-change",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Plan => "计划",
            Self::AskUser => "单选提问",
            Self::AskUserMulti => "多选提问",
            Self::AskUserPreview => "示例提问",
            Self::AskUserFlow => "多题提问",
            Self::Permission => "权限申请",
            Self::TodoTool => "Todo 工具",
            Self::Todo => "待办卡片",
            Self::Command => "命令",
            Self::FileRead => "读文件",
            Self::FileChange => "改文件",
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct NativeDebugTimeline {
    sequence: u64,
    events: BTreeMap<TaskId, Vec<TaskTimelineItem>>,
    pending: BTreeMap<TaskId, Vec<PendingActionView>>,
}

impl NativeDebugTimeline {
    pub(crate) fn inject(&mut self, task_id: TaskId, action: DebugTimelineAction) {
        self.sequence = self.sequence.saturating_add(1);
        let sequence = self.sequence;
        let event_id = format!("native-debug:event:{sequence}");
        let request_id = format!("native-debug:request:{sequence}");
        let (kind, title, summary, markdown, pending) = action_spec(action, &event_id, &request_id);
        let markdown_document = markdown.as_deref().map(NativeMarkdown::parse);
        let markdown_plain_text = markdown_document.as_ref().map(NativeMarkdown::plain_text);
        self.events
            .entry(task_id.clone())
            .or_default()
            .push(TaskTimelineItem {
                id: event_id,
                sequence: u64::MAX.saturating_sub(10_000).saturating_add(sequence),
                kind: kind.to_owned(),
                title: title.to_owned(),
                message_role: None,
                summary: Some(summary.to_owned()),
                markdown,
                markdown_document,
                markdown_plain_text,
                markdown_table_count: 0,
                attachments: Vec::new(),
                status: if pending.is_some() {
                    "requires_action".to_owned()
                } else {
                    "success".to_owned()
                },
                batch_apply: None,
                session_branch_turn_id: None,
                selectable_reply: false,
                can_retry: false,
            });
        if let Some(pending) = pending {
            self.pending.entry(task_id).or_default().push(pending);
        }
    }

    pub(crate) fn event_count(&self, task_id: &TaskId) -> usize {
        self.events.get(task_id).map(Vec::len).unwrap_or_default()
    }

    pub(crate) fn overlay(&self, task_id: &TaskId) -> (&[TaskTimelineItem], &[PendingActionView]) {
        (
            self.events.get(task_id).map(Vec::as_slice).unwrap_or(&[]),
            self.pending.get(task_id).map(Vec::as_slice).unwrap_or(&[]),
        )
    }

    pub(crate) fn resolve(
        &mut self,
        task_id: &TaskId,
        request_id: &str,
        accepted: bool,
        response: &Value,
    ) -> bool {
        let Some(pending) = self.pending.get_mut(task_id) else {
            return false;
        };
        let Some(index) = pending
            .iter()
            .position(|item| item.request_id == request_id)
        else {
            return false;
        };
        let item = pending.remove(index);
        if pending.is_empty() {
            self.pending.remove(task_id);
        }
        let event_id = item
            .payload
            .get("debugEventId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(event) = self
            .events
            .get_mut(task_id)
            .and_then(|events| events.iter_mut().find(|event| event.id == event_id))
        {
            event.status = if accepted { "success" } else { "cancelled" }.to_owned();
            event.summary = Some(debug_resolution_summary(&item.kind, accepted, response));
        }
        true
    }
}

fn action_spec(
    action: DebugTimelineAction,
    event_id: &str,
    request_id: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<String>,
    Option<PendingActionView>,
) {
    let pending = |kind: &str, prompt: &str, payload: Value| PendingActionView {
        request_id: request_id.to_owned(),
        kind: kind.to_owned(),
        prompt: prompt.to_owned(),
        status: PendingProjectionStatus::Open,
        payload: merge_debug_event_id(payload, event_id),
    };
    match action {
        DebugTimelineAction::Plan => (
            "plan",
            "Debug Plan",
            "调试计划等待确认",
            Some("## Debug 计划\n- 梳理当前状态\n- 注入临时事件\n- 验证时间线渲染".to_owned()),
            Some(pending(
                "plan_approval",
                "确认 Debug 计划",
                json!({ "approved": null }),
            )),
        ),
        DebugTimelineAction::AskUser => ask_action(
            "Debug AskUser",
            "要注入哪类临时事件？",
            event_id,
            request_id,
            json!({
                "title": "Debug 提问",
                "source": "Debug",
                "dismissable": true,
                "questions": [{
                    "id": "debug-choice",
                    "header": "Debug",
                    "question": "要注入哪类临时事件？",
                    "mode": "single",
                    "options": [
                        { "id": "plan", "label": "计划" },
                        { "id": "todo", "label": "待办" },
                        { "id": "ask_user", "label": "提问" }
                    ]
                }]
            }),
        ),
        DebugTimelineAction::AskUserMulti => ask_action(
            "Debug 多选提问",
            "这轮调试要覆盖哪些流程？",
            event_id,
            request_id,
            json!({
                "title": "Debug 多选提问",
                "source": "Debug",
                "dismissable": true,
                "questions": [{
                    "id": "debug-multi",
                    "header": "多选",
                    "question": "这轮调试要覆盖哪些流程？",
                    "mode": "multi",
                    "minSelections": 2,
                    "maxSelections": 3,
                    "allowOther": true,
                    "options": [
                        { "id": "plan", "label": "计划确认", "description": "同意、修改要求和取消态。" },
                        { "id": "ask", "label": "提问回答", "description": "单题、多题和选择结果。" },
                        { "id": "permission", "label": "权限申请", "description": "允许、拒绝和入参展开。", "recommended": true },
                        { "id": "cards", "label": "普通卡片", "description": "待办、命令和文件事件。" }
                    ]
                }]
            }),
        ),
        DebugTimelineAction::AskUserPreview => ask_action(
            "Debug 示例提问",
            "选择一种响应模板。",
            event_id,
            request_id,
            json!({
                "title": "Debug 示例提问",
                "source": "Debug",
                "dismissable": true,
                "questions": [{
                    "id": "debug-preview",
                    "header": "示例",
                    "question": "选择一种响应模板。",
                    "mode": "single",
                    "allowOther": true,
                    "options": [
                        { "id": "concise", "label": "精简", "description": "只保留结论和验证结果。", "recommended": true, "preview": "已完成：调试交互可用。" },
                        { "id": "detailed", "label": "详细", "description": "展开关键改动、边界和剩余风险。", "preview": "改动：结构化提问状态与结果。" },
                        { "id": "risk", "label": "风险优先", "description": "先列出行为风险。", "danger": true, "preview": "风险：失效请求不会提交。" }
                    ]
                }]
            }),
        ),
        DebugTimelineAction::AskUserFlow => ask_action(
            "Debug 多题提问",
            "先选择要验证的入口。",
            event_id,
            request_id,
            json!({
                "title": "Debug 多题提问",
                "source": "Debug",
                "dismissable": true,
                "questions": [
                    {
                        "id": "debug-flow-target",
                        "header": "目标",
                        "question": "这次要先验证哪个入口？",
                        "mode": "single",
                        "options": [
                            { "id": "sidebar", "label": "侧栏", "description": "验证动态注册和切换。" },
                            { "id": "timeline", "label": "时间线", "description": "验证卡片和 pending action。" }
                        ]
                    },
                    {
                        "id": "debug-flow-checks",
                        "header": "检查项",
                        "question": "需要一起检查哪些状态？",
                        "mode": "multi",
                        "minSelections": 1,
                        "options": [
                            { "id": "success", "label": "完成态" },
                            { "id": "cancelled", "label": "取消态" },
                            { "id": "expired", "label": "失效态" }
                        ]
                    }
                ]
            }),
        ),
        DebugTimelineAction::Permission => (
            "file_change",
            "Debug Permission",
            "apps/desktop/src/debug-fixture.ts",
            None,
            Some(pending(
                "tool_consent",
                "允许模拟写入调试夹具吗？",
                json!({
                    "toolName": "Write",
                    "title": "写入调试夹具",
                    "displayName": "Write",
                    "description": "Debug 本地权限申请，不会写入文件或通知 runner。",
                    "blockedPath": "apps/desktop/src/debug-fixture.ts",
                    "input": {
                        "file_path": "apps/desktop/src/debug-fixture.ts",
                        "content": "export const debugFixture = true;\n"
                    }
                }),
            )),
        ),
        DebugTimelineAction::TodoTool => (
            "todo_list",
            "Debug TodoWrite",
            "已通过共享 Todo 服务写入调试待办",
            None,
            None,
        ),
        DebugTimelineAction::Todo => (
            "todo_list",
            "Debug TodoWrite",
            "确认面板已出现 · 点击预制事件 · 观察时间线渲染",
            None,
            None,
        ),
        DebugTimelineAction::Command => (
            "command",
            "Debug Bash",
            "cargo test -p lilia-contracts · Contracts verified.",
            None,
            None,
        ),
        DebugTimelineAction::FileRead => (
            "file_read",
            "Debug Read",
            "crates/lilia-contracts/src/lib.rs",
            None,
            None,
        ),
        DebugTimelineAction::FileChange => (
            "file_change",
            "Debug Edit",
            "apps/desktop/src/desktop.rs",
            None,
            None,
        ),
    }
}

fn ask_action(
    title: &'static str,
    prompt: &'static str,
    event_id: &str,
    request_id: &str,
    spec: Value,
) -> (
    &'static str,
    &'static str,
    &'static str,
    Option<String>,
    Option<PendingActionView>,
) {
    (
        "ask_user",
        title,
        prompt,
        None,
        Some(PendingActionView {
            request_id: request_id.to_owned(),
            kind: "ask_user".to_owned(),
            prompt: prompt.to_owned(),
            status: PendingProjectionStatus::Open,
            payload: json!({
                "debugEventId": event_id,
                "spec": spec,
            }),
        }),
    )
}

fn merge_debug_event_id(mut payload: Value, event_id: &str) -> Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "debugEventId".to_owned(),
            Value::String(event_id.to_owned()),
        );
    }
    payload
}

fn debug_resolution_summary(kind: &str, accepted: bool, response: &Value) -> String {
    if !accepted {
        return "调试交互已取消".to_owned();
    }
    if kind == "plan_approval" {
        return match response.get("action").and_then(Value::as_str) {
            Some("revise") => "Debug 计划已要求修改",
            Some("decline") => "Debug 计划已拒绝",
            _ => "Debug 计划已同意",
        }
        .to_owned();
    }
    response
        .get("answer")
        .and_then(Value::as_str)
        .filter(|answer| !answer.trim().is_empty())
        .map(|answer| format!("调试回答：{answer}"))
        .unwrap_or_else(|| "调试交互已完成".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_debug_event_resolves_in_the_ephemeral_overlay() {
        let task_id = TaskId::new("task-one").unwrap();
        let mut timeline = NativeDebugTimeline::default();
        timeline.inject(task_id.clone(), DebugTimelineAction::Plan);
        let (events, pending) = timeline.overlay(&task_id);
        assert_eq!(events.len(), 1);
        assert_eq!(pending.len(), 1);
        let request_id = pending[0].request_id.clone();

        assert!(timeline.resolve(&task_id, &request_id, true, &json!({ "action": "approve" }),));
        let (events, pending) = timeline.overlay(&task_id);
        assert!(pending.is_empty());
        assert_eq!(events[0].status, "success");
        assert_eq!(events[0].summary.as_deref(), Some("Debug 计划已同意"));
    }
}
