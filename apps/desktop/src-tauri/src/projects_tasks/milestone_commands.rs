use std::collections::HashSet;

use lilia_desktop_application::{
    DesktopRoadmapService, Milestone, MilestoneDueDateUpdate, MilestoneStatus,
    MilestoneUpdatePatch, ProjectRoadmap, RoadmapStoreError, TaskMilestoneLink,
};
use tauri::State;

use crate::task_contract;

use super::types::{MilestoneRow, ProjectRoadmapRow, TaskMilestoneLinkRow};

fn validate_status(status: &str) -> Result<MilestoneStatus, String> {
    if !task_contract::milestone_statuses()
        .iter()
        .any(|value| value == status)
    {
        return Err(format!("milestone_update: 无效状态：{status}"));
    }
    MilestoneStatus::ALL
        .into_iter()
        .find(|candidate| candidate.as_str() == status)
        .ok_or_else(|| format!("milestone_update: 无效状态：{status}"))
}

fn milestone_row(milestone: Milestone) -> MilestoneRow {
    MilestoneRow {
        id: milestone.id,
        project_id: milestone.project_id,
        title: milestone.title,
        description: milestone.description,
        status: milestone.status.as_str().to_owned(),
        due_date: milestone.due_date,
        order: milestone.order,
        created_at: milestone.created_at,
    }
}

fn link_row(link: TaskMilestoneLink) -> TaskMilestoneLinkRow {
    TaskMilestoneLinkRow {
        task_id: link.task_id,
        milestone_id: link.milestone_id,
    }
}

fn roadmap_row(roadmap: ProjectRoadmap) -> ProjectRoadmapRow {
    ProjectRoadmapRow {
        milestones: roadmap.milestones.into_iter().map(milestone_row).collect(),
        links: roadmap.links.into_iter().map(link_row).collect(),
    }
}

fn command_error(command: &'static str, error: RoadmapStoreError) -> String {
    match (command, &error) {
        ("milestone_create", RoadmapStoreError::InvalidTitle)
        | ("milestone_update", RoadmapStoreError::InvalidTitle) => {
            format!("{command}: 标题不能为空")
        }
        ("milestone_create", RoadmapStoreError::ProjectNotFound { .. }) => {
            "milestone_create: 项目不存在".to_owned()
        }
        ("milestone_update", RoadmapStoreError::MilestoneNotFound { .. }) => {
            "milestone_update: milestone 不存在".to_owned()
        }
        ("milestone_set_tasks", RoadmapStoreError::MilestoneNotFound { .. }) => {
            "milestone_set_tasks: milestone 不存在".to_owned()
        }
        ("milestone_set_tasks", RoadmapStoreError::TaskNotEligible { task_id, .. }) => {
            format!("milestone_set_tasks: 任务不属于当前项目：{task_id}")
        }
        _ => format!("{command}: {error}"),
    }
}

fn milestone_list_core(
    service: &DesktopRoadmapService,
    project_id: &str,
) -> Result<ProjectRoadmapRow, String> {
    service
        .list(project_id)
        .map(roadmap_row)
        .map_err(|error| command_error("milestone_list", error))
}

fn milestone_create_core(
    service: &DesktopRoadmapService,
    project_id: &str,
    title: &str,
) -> Result<MilestoneRow, String> {
    service
        .create(project_id, title)
        .map(milestone_row)
        .map_err(|error| command_error("milestone_create", error))
}

fn milestone_update_core(
    service: &DesktopRoadmapService,
    id: &str,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    due_date: Option<i64>,
    clear_due_date: bool,
) -> Result<(), String> {
    if title.is_none()
        && description.is_none()
        && status.is_none()
        && due_date.is_none()
        && !clear_due_date
    {
        return Ok(());
    }
    let status = status.as_deref().map(validate_status).transpose()?;
    let due_date = if clear_due_date {
        MilestoneDueDateUpdate::Clear
    } else if let Some(due_date) = due_date {
        MilestoneDueDateUpdate::Set(due_date)
    } else {
        MilestoneDueDateUpdate::Unchanged
    };
    service
        .update(
            id,
            MilestoneUpdatePatch {
                title,
                description,
                status,
                due_date,
            },
        )
        .map(|_| ())
        .map_err(|error| command_error("milestone_update", error))
}

fn milestone_delete_core(service: &DesktopRoadmapService, id: &str) -> Result<bool, String> {
    service
        .delete(id)
        .map_err(|error| command_error("milestone_delete", error))
}

fn milestone_reorder_core(
    service: &DesktopRoadmapService,
    project_id: &str,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let roadmap = service
        .list(project_id)
        .map_err(|error| command_error("milestone_reorder", error))?;
    let project_ids = roadmap
        .milestones
        .iter()
        .map(|milestone| milestone.id.clone())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut normalized = ordered_ids
        .into_iter()
        .filter(|id| project_ids.contains(id) && seen.insert(id.clone()))
        .collect::<Vec<_>>();
    normalized.extend(
        roadmap
            .milestones
            .into_iter()
            .map(|milestone| milestone.id)
            .filter(|id| seen.insert(id.clone())),
    );
    match service.reorder(project_id, normalized) {
        Ok(_) | Err(RoadmapStoreError::ProjectNotFound { .. }) => Ok(()),
        Err(error) => Err(command_error("milestone_reorder", error)),
    }
}

fn milestone_set_tasks_core(
    service: &DesktopRoadmapService,
    milestone_id: &str,
    task_ids: Vec<String>,
) -> Result<Vec<TaskMilestoneLinkRow>, String> {
    service
        .set_tasks(milestone_id, task_ids)
        .map(|links| links.into_iter().map(link_row).collect())
        .map_err(|error| command_error("milestone_set_tasks", error))
}

#[tauri::command]
pub fn milestone_list(
    project_id: String,
    service: State<'_, DesktopRoadmapService>,
) -> Result<ProjectRoadmapRow, String> {
    milestone_list_core(&service, &project_id)
}

#[tauri::command]
pub fn milestone_create(
    project_id: String,
    title: String,
    service: State<'_, DesktopRoadmapService>,
) -> Result<MilestoneRow, String> {
    milestone_create_core(&service, &project_id, &title)
}

#[tauri::command]
pub fn milestone_update(
    id: String,
    title: Option<String>,
    description: Option<String>,
    status: Option<String>,
    due_date: Option<i64>,
    clear_due_date: Option<bool>,
    service: State<'_, DesktopRoadmapService>,
) -> Result<(), String> {
    milestone_update_core(
        &service,
        &id,
        title,
        description,
        status,
        due_date,
        clear_due_date.unwrap_or(false),
    )
}

#[tauri::command]
pub fn milestone_delete(
    id: String,
    service: State<'_, DesktopRoadmapService>,
) -> Result<bool, String> {
    milestone_delete_core(&service, &id)
}

#[tauri::command]
pub fn milestone_reorder(
    project_id: String,
    ordered_ids: Vec<String>,
    service: State<'_, DesktopRoadmapService>,
) -> Result<(), String> {
    milestone_reorder_core(&service, &project_id, ordered_ids)
}

#[tauri::command]
pub fn milestone_set_tasks(
    milestone_id: String,
    task_ids: Vec<String>,
    service: State<'_, DesktopRoadmapService>,
) -> Result<Vec<TaskMilestoneLinkRow>, String> {
    milestone_set_tasks_core(&service, &milestone_id, task_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lilia_desktop_application::SqliteRoadmapStore;
    use rusqlite::Connection;

    fn service() -> DesktopRoadmapService {
        let connection = Connection::open_in_memory().unwrap();
        let store = SqliteRoadmapStore::from_connection(connection).unwrap();
        let connection = store.into_connection();
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, name, created_at) VALUES ('p1', 'P1', 1), ('p2', 'P2', 1);
                INSERT INTO tasks (id, project_id, session_id, title, status, created_at, sort_order)
                  VALUES
                    ('t1', 'p1', 't1', 'T1', 'waiting', 1, 0),
                    ('t2', 'p1', 't2', 'T2', 'done', 2, 1),
                    ('t3', 'p2', 't3', 'T3', 'waiting', 3, 0),
                    ('t4', 'p1', 't4', 'T4', 'waiting', 4, 2);
                UPDATE tasks SET archived = 1 WHERE id = 't4';
                INSERT INTO milestones (id, project_id, title, status, sort_order, created_at)
                  VALUES
                    ('m1', 'p1', 'M1', 'upcoming', 0, 1),
                    ('m2', 'p1', 'M2', 'upcoming', 1, 2),
                    ('m3', 'p2', 'M3', 'upcoming', 0, 1);
                "#,
            )
            .unwrap();
        DesktopRoadmapService::from_store(SqliteRoadmapStore::from_connection(connection).unwrap())
            .unwrap()
    }

    #[test]
    fn validate_status_accepts_contract_values() {
        let statuses: Vec<&str> = task_contract::milestone_statuses()
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(
            statuses,
            vec!["upcoming", "in-progress", "done", "abandoned"]
        );
        assert_eq!(task_contract::default_milestone_status(), "upcoming");
        for status in task_contract::milestone_statuses() {
            assert_eq!(validate_status(status).unwrap().as_str(), status);
        }
        assert!(validate_status("running").is_err());
    }

    #[test]
    fn create_uses_the_shared_service_and_preserves_the_frontend_row() {
        let service = service();
        let milestone = milestone_create_core(&service, "p1", "  Delivery  ").unwrap();
        assert_eq!(milestone.project_id, "p1");
        assert_eq!(milestone.title, "Delivery");
        assert_eq!(milestone.status, "upcoming");
        assert_eq!(milestone.order, 2);
    }

    #[test]
    fn set_tasks_replaces_links_for_same_project_unarchived_tasks() {
        let service = service();
        milestone_set_tasks_core(&service, "m1", vec!["t1".into()]).unwrap();
        let links = milestone_set_tasks_core(&service, "m1", vec!["t2".into()]).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].task_id, "t2");
        let stored = milestone_list_core(&service, "p1").unwrap().links;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].task_id, links[0].task_id);
        assert_eq!(stored[0].milestone_id, links[0].milestone_id);
    }

    #[test]
    fn task_validation_rejects_cross_project_and_archived_tasks() {
        let service = service();
        for task_id in ["t3", "t4"] {
            assert!(milestone_set_tasks_core(&service, "m1", vec![task_id.into()]).is_err());
        }
    }

    #[test]
    fn update_milestone_core_updates_description_and_due_date() {
        let service = service();
        milestone_update_core(
            &service,
            "m1",
            None,
            Some("  可验证的交付边界  ".into()),
            None,
            Some(1_781_596_800_000),
            false,
        )
        .unwrap();
        let milestone = &milestone_list_core(&service, "p1").unwrap().milestones[0];
        assert_eq!(milestone.description, "可验证的交付边界");
        assert_eq!(milestone.due_date, Some(1_781_596_800_000));

        milestone_update_core(&service, "m1", None, None, None, None, true).unwrap();
        assert_eq!(
            milestone_list_core(&service, "p1").unwrap().milestones[0].due_date,
            None
        );
    }

    #[test]
    fn empty_update_remains_a_noop_for_legacy_callers() {
        let service = service();
        milestone_update_core(&service, "missing", None, None, None, None, false).unwrap();
    }

    #[test]
    fn delete_milestone_removes_links_without_touching_tasks() {
        let service = service();
        milestone_set_tasks_core(&service, "m1", vec!["t1".into()]).unwrap();
        assert!(milestone_delete_core(&service, "m1").unwrap());
        assert!(!milestone_delete_core(&service, "missing").unwrap());
        assert!(milestone_list_core(&service, "p1")
            .unwrap()
            .links
            .is_empty());

        let replacement = milestone_create_core(&service, "p1", "Replacement").unwrap();
        assert_eq!(
            milestone_set_tasks_core(&service, &replacement.id, vec!["t1".into()]).unwrap()[0]
                .task_id,
            "t1"
        );
    }

    #[test]
    fn reorder_milestones_updates_only_current_project_rows() {
        let service = service();
        milestone_reorder_core(&service, "p1", vec!["m2".into(), "m1".into(), "m3".into()])
            .unwrap();
        let rows = milestone_list_core(&service, "p1").unwrap().milestones;
        assert_eq!(
            rows.iter()
                .map(|milestone| milestone.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m2", "m1"]
        );
        assert_eq!(
            milestone_list_core(&service, "p2").unwrap().milestones[0].order,
            0
        );
    }
}
