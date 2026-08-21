use lilia_contracts::TaskId;
use lilia_desktop_application::DesktopSuggestionItem;

#[derive(Clone, Debug, Default)]
pub(crate) struct ConversationSuggestionState {
    task_id: Option<TaskId>,
    project_id: Option<String>,
    items: Vec<DesktopSuggestionItem>,
    loading: bool,
    enabled: bool,
    initialized: bool,
    active_operation: Option<u64>,
    error: bool,
}

impl ConversationSuggestionState {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn disable(&mut self, task_id: Option<TaskId>) {
        self.clear();
        self.task_id = task_id;
        self.initialized = true;
    }

    pub(crate) fn begin(&mut self, task_id: TaskId, project_id: String, operation_id: u64) {
        if self.task_id.as_ref() != Some(&task_id)
            || self.project_id.as_deref() != Some(&project_id)
        {
            self.items.clear();
        }
        self.task_id = Some(task_id);
        self.project_id = Some(project_id);
        self.loading = true;
        self.enabled = true;
        self.initialized = true;
        self.active_operation = Some(operation_id);
        self.error = false;
    }

    pub(crate) fn finish(
        &mut self,
        task_id: &TaskId,
        operation_id: u64,
        result: Result<Vec<DesktopSuggestionItem>, String>,
    ) -> bool {
        if self.task_id.as_ref() != Some(task_id) || self.active_operation != Some(operation_id) {
            return false;
        }
        self.loading = false;
        self.active_operation = None;
        match result {
            Ok(items) => {
                self.items = items;
                self.error = false;
            }
            Err(_) => {
                self.items.clear();
                self.error = true;
            }
        }
        true
    }

    pub(crate) fn is_current_for(&self, task_id: &TaskId) -> bool {
        self.task_id.as_ref() == Some(task_id) && self.initialized
    }

    pub(crate) fn should_show(&self) -> bool {
        self.enabled && (self.loading || self.error || !self.items.is_empty())
    }

    pub(crate) fn can_refresh(&self) -> bool {
        self.should_show() && !self.loading
    }

    pub(crate) fn visible_item_ids(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .filter(|_| !self.loading && !self.error)
            .map(|item| item.id.as_str())
    }

    pub(crate) fn prompt_for(&self, item_id: &str) -> Option<String> {
        self.items
            .iter()
            .find(|item| item.id == item_id)
            .map(|item| item.prompt.clone())
    }
}

#[cfg(test)]
mod tests {
    use lilia_desktop_application::DesktopSuggestionItemSource;

    use super::*;

    fn suggestion(id: &str, prompt: &str) -> DesktopSuggestionItem {
        DesktopSuggestionItem {
            id: id.to_owned(),
            project_id: Some("project".to_owned()),
            task_ids: Vec::new(),
            source: DesktopSuggestionItemSource::Task,
            github_activities: Vec::new(),
            local_git_contexts: Vec::new(),
            codex_threads: Vec::new(),
            summary: "继续任务".to_owned(),
            reason: "仍有未完成事项".to_owned(),
            prompt: prompt.to_owned(),
            generated_at: 1,
        }
    }

    #[test]
    fn stale_completion_cannot_replace_the_active_task_suggestions() {
        let first_task = TaskId::new("first").unwrap();
        let active_task = TaskId::new("active").unwrap();
        let mut state = ConversationSuggestionState::default();
        state.begin(first_task.clone(), "project".to_owned(), 1);
        state.begin(active_task.clone(), "project".to_owned(), 2);

        assert!(!state.finish(&first_task, 1, Ok(vec![suggestion("stale", "不要写入")])));
        assert!(state.finish(
            &active_task,
            2,
            Ok(vec![suggestion("current", "继续当前任务")])
        ));

        assert_eq!(state.visible_item_ids().collect::<Vec<_>>(), ["current"]);
        assert_eq!(state.prompt_for("current").as_deref(), Some("继续当前任务"));
    }
}
