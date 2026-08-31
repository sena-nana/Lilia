//! Timeline view state as a UI module.
//!
//! The events themselves live on the task session the shell publishes. This
//! module owns expansion state for its window.

use std::collections::BTreeSet;

use lilia_kernel::FeatureId;

use crate::runtime_shell::{PrimaryShellSnapshot, ShellTimelineRow};
use crate::task_session::TaskTimelineItem;
use crate::ui_module::{UiModule, UiModuleContext, UiModuleOutcome};

#[derive(Debug, Clone)]
pub struct TimelineTextSelection {
    pub event_id: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum TimelineModuleMessage {
    Toggle(String),
    ClearTextSelection,
}

pub struct TimelineModule {
    toggled_events: BTreeSet<String>,
    text_selection: Option<TimelineTextSelection>,
}

impl Default for TimelineModule {
    fn default() -> Self {
        Self {
            toggled_events: BTreeSet::new(),
            text_selection: None,
        }
    }
}

impl TimelineModule {
    pub fn feature_id() -> FeatureId {
        FeatureId::new("lilia.timeline").expect("the timeline feature id is not blank")
    }

    pub fn text_selection(&self) -> Option<&TimelineTextSelection> {
        self.text_selection.as_ref()
    }

    pub fn row(item: &TaskTimelineItem, expanded: bool, can_retry: bool) -> ShellTimelineRow {
        let full = item
            .markdown
            .clone()
            .or_else(|| item.summary.clone())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| item.title.clone());
        let preview = item
            .summary
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| item.title.clone());
        let can_expand = item.markdown.as_ref().is_some_and(|markdown| {
            item.summary
                .as_ref()
                .is_some_and(|summary| summary != markdown)
                || item.title != *markdown
        });
        ShellTimelineRow {
            id: item.id.clone(),
            markdown: if expanded || !can_expand {
                full
            } else {
                preview
            },
            expanded,
            can_expand,
            can_retry,
            can_copy: item
                .markdown_plain_text
                .as_ref()
                .or(item.markdown.as_ref())
                .is_some_and(|text| !text.trim().is_empty()),
        }
    }
}

impl UiModule for TimelineModule {
    type Message = TimelineModuleMessage;

    fn feature(&self) -> FeatureId {
        Self::feature_id()
    }

    fn reduce(&mut self, message: Self::Message, _cx: &UiModuleContext<'_>) -> UiModuleOutcome {
        match message {
            TimelineModuleMessage::Toggle(event_id) => {
                if !self.toggled_events.remove(&event_id) {
                    self.toggled_events.insert(event_id);
                }
                UiModuleOutcome::dirty()
            }
            TimelineModuleMessage::ClearTextSelection => {
                self.text_selection = None;
                UiModuleOutcome::dirty()
            }
        }
    }

    fn project(&self, cx: &UiModuleContext<'_>, into: &mut PrimaryShellSnapshot) {
        if !crate::module::conversation_is_visible(cx) {
            return;
        }
        let Some(session) = cx.task_session() else {
            into.timeline.clear();
            into.timeline_layout = nana_ui::VirtualListLayout::default();
            into.timeline_can_load_earlier = false;
            return;
        };
        into.timeline_can_load_earlier = session.timeline_has_more_before;
        into.timeline_layout = session.timeline_layout.clone();
        into.timeline = session
            .timeline
            .iter()
            .map(|item| {
                Self::row(
                    item,
                    self.toggled_events.contains(&item.id),
                    item.can_retry && session.run_block.is_none(),
                )
            })
            .collect();
    }

    fn invalidate(
        &mut self,
        envelope: &lilia_kernel::EventEnvelope,
        cx: &UiModuleContext<'_>,
    ) -> UiModuleOutcome {
        let selected = cx.selected_task();
        let for_selected = |task_id: &lilia_contracts::TaskId| selected.as_ref() == Some(task_id);
        if envelope
            .downcast::<crate::application::TimelineChanged>()
            .is_some_and(|event| for_selected(&event.task_id))
            || envelope
                .downcast::<crate::application::ApprovalChanged>()
                .is_some_and(|event| for_selected(&event.task_id))
            || envelope
                .downcast::<crate::application::InteractionChanged>()
                .is_some_and(|event| for_selected(&event.task_id))
        {
            return UiModuleOutcome::dirty();
        }
        UiModuleOutcome::clean()
    }
}
