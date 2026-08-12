use std::collections::BTreeMap;

use lilia_contracts::{ChatConversationReference, TaskId};

use crate::{DesktopApplication, DesktopApplicationError, ProjectQuery, TaskQuery};

const MAX_CONVERSATION_REFERENCE_RESULTS: usize = 50;

impl DesktopApplication {
    pub fn search_conversation_references(
        &self,
        current_task_id: &TaskId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ChatConversationReference>, DesktopApplicationError> {
        self.get_task(current_task_id)?;
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let project_names = self
            .query_projects(ProjectQuery::default())?
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect::<BTreeMap<_, _>>();
        let mut references = self
            .query_tasks(TaskQuery::default())?
            .into_iter()
            .filter(|task| &task.id != current_task_id)
            .filter_map(|task| {
                let project_name = task
                    .project_id
                    .as_ref()
                    .and_then(|project_id| project_names.get(project_id));
                let title = task.title.to_lowercase();
                let project = project_name.map(|name| name.to_lowercase());
                let title_match = title.find(&query);
                let project_match = project.as_deref().and_then(|name| name.find(&query));
                if title_match.is_none() && project_match.is_none() {
                    return None;
                }
                let project_id = task.project_id.as_ref().map(|id| id.as_str().to_owned());
                let route = match &project_id {
                    Some(project_id) => {
                        format!("/projects/{project_id}/tasks/{}", task.id.as_str())
                    }
                    None => format!("/chats/{}", task.id.as_str()),
                };
                Some((
                    title_match.unwrap_or(usize::MAX),
                    project_match.unwrap_or(usize::MAX),
                    ChatConversationReference {
                        task_id: task.id.as_str().to_owned(),
                        title: task.title,
                        route,
                        project_id,
                        project_name: project_name.cloned(),
                    },
                ))
            })
            .collect::<Vec<_>>();
        references.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.title.cmp(&right.2.title))
                .then_with(|| left.2.task_id.cmp(&right.2.task_id))
        });
        Ok(references
            .into_iter()
            .take(limit.clamp(1, MAX_CONVERSATION_REFERENCE_RESULTS))
            .map(|(_, _, reference)| reference)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_contracts::{ProductEntity, ProductTask, Project, ProjectId};
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

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

    #[test]
    fn search_is_cross_project_deterministic_and_excludes_the_current_task() {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            "test:conversation-reference",
            "conversation-reference-test",
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/conversation-reference",
                "liliacode.conversation-reference-test",
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let project_id = ProjectId::new("project-1").unwrap();
        let current_task_id = TaskId::new("task-current").unwrap();
        let referenced_task_id = TaskId::new("task-reference").unwrap();
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(project_id.clone(), "Native 搜索").unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(
                    current_task_id.clone(),
                    Some(project_id.clone()),
                    "当前任务",
                )
                .unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(referenced_task_id.clone(), Some(project_id), "上下文设计")
                    .unwrap(),
            ))
            .unwrap();

        let result = app
            .search_conversation_references(&current_task_id, "设计", 12)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].task_id, referenced_task_id.as_str());
        assert_eq!(result[0].route, "/projects/project-1/tasks/task-reference");
        assert_eq!(result[0].project_name.as_deref(), Some("Native 搜索"));
    }
}
