use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use uuid::Uuid;

use super::{
    Milestone, MilestoneDueDateUpdate, MilestoneStatus, MilestoneUpdatePatch, ProjectRoadmap,
    RoadmapStore, RoadmapStoreError, TaskMilestoneLink,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL,
  cwd        TEXT,
  created_at INTEGER NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  pinned     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tasks (
  id           TEXT PRIMARY KEY,
  project_id   TEXT,
  session_id   TEXT NOT NULL,
  title        TEXT NOT NULL,
  title_source TEXT NOT NULL DEFAULT 'auto' CHECK (title_source IN ('auto','manual')),
  status       TEXT NOT NULL DEFAULT 'waiting' CHECK (status IN
                 ('draft','waiting','running','blocked','done','cancelled')),
  created_at   INTEGER NOT NULL,
  parent_id    TEXT,
  archived     INTEGER NOT NULL DEFAULT 0,
  sort_order   INTEGER NOT NULL DEFAULT 0,
  pinned       INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS milestones (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  status      TEXT NOT NULL DEFAULT 'upcoming'
              CHECK (status IN ('upcoming','in-progress','done','abandoned')),
  due_date    INTEGER,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_milestones_project_order
  ON milestones(project_id, sort_order ASC, created_at ASC);

CREATE TABLE IF NOT EXISTS task_milestone_links (
  task_id      TEXT NOT NULL,
  milestone_id TEXT NOT NULL,
  PRIMARY KEY (task_id, milestone_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (milestone_id) REFERENCES milestones(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_task_milestone_links_milestone
  ON task_milestone_links(milestone_id);
"#;

pub struct SqliteRoadmapStore {
    connection: Connection,
}

impl SqliteRoadmapStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RoadmapStoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RoadmapStoreError::storage("create database directory", error))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| RoadmapStoreError::storage("open database", error))?;
        Self::from_connection(connection)
    }

    pub fn in_memory() -> Result<Self, RoadmapStoreError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| RoadmapStoreError::storage("open in-memory database", error))?;
        Self::from_connection(connection)
    }

    pub fn from_connection(connection: Connection) -> Result<Self, RoadmapStoreError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| RoadmapStoreError::storage("configure busy timeout", error))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| RoadmapStoreError::storage("enable foreign keys", error))?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|error| RoadmapStoreError::storage("initialize schema", error))?;
        Ok(Self { connection })
    }

    pub fn into_connection(self) -> Connection {
        self.connection
    }
}

impl RoadmapStore for SqliteRoadmapStore {
    fn list(&self, project_id: &str) -> Result<ProjectRoadmap, RoadmapStoreError> {
        list_roadmap(&self.connection, project_id)
    }

    fn create(&mut self, project_id: &str, title: &str) -> Result<Milestone, RoadmapStoreError> {
        let title = normalized_title(title)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RoadmapStoreError::storage("begin milestone create", error))?;
        ensure_project_exists(&transaction, project_id)?;
        let order = transaction
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM milestones WHERE project_id = ?1",
                params![project_id],
                |row| row.get(0),
            )
            .map_err(|error| RoadmapStoreError::storage("allocate milestone order", error))?;
        let milestone = Milestone {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_owned(),
            title,
            description: String::new(),
            status: MilestoneStatus::default(),
            due_date: None,
            order,
            created_at: now_millis(),
        };
        transaction
            .execute(
                r#"INSERT INTO milestones
                   (id, project_id, title, description, status, due_date, sort_order, created_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
                params![
                    milestone.id,
                    milestone.project_id,
                    milestone.title,
                    milestone.description,
                    milestone.status.as_str(),
                    milestone.due_date,
                    milestone.order,
                    milestone.created_at,
                ],
            )
            .map_err(|error| RoadmapStoreError::storage("insert milestone", error))?;
        transaction
            .commit()
            .map_err(|error| RoadmapStoreError::storage("commit milestone create", error))?;
        Ok(milestone)
    }

    fn update(
        &mut self,
        milestone_id: &str,
        patch: MilestoneUpdatePatch,
    ) -> Result<Milestone, RoadmapStoreError> {
        let title = patch.title.as_deref().map(normalized_title).transpose()?;
        let description = patch
            .description
            .as_deref()
            .map(str::trim)
            .map(str::to_owned);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RoadmapStoreError::storage("begin milestone update", error))?;
        let mut milestone = milestone_by_id(&transaction, milestone_id)?.ok_or_else(|| {
            RoadmapStoreError::MilestoneNotFound {
                milestone_id: milestone_id.to_owned(),
            }
        })?;

        if let Some(title) = title {
            milestone.title = title;
        }
        if let Some(description) = description {
            milestone.description = description;
        }
        if let Some(status) = patch.status {
            milestone.status = status;
        }
        match patch.due_date {
            MilestoneDueDateUpdate::Unchanged => {}
            MilestoneDueDateUpdate::Set(due_date) => milestone.due_date = Some(due_date),
            MilestoneDueDateUpdate::Clear => milestone.due_date = None,
        }

        transaction
            .execute(
                r#"UPDATE milestones
                   SET title = ?1, description = ?2, status = ?3, due_date = ?4
                   WHERE id = ?5"#,
                params![
                    milestone.title,
                    milestone.description,
                    milestone.status.as_str(),
                    milestone.due_date,
                    milestone.id,
                ],
            )
            .map_err(|error| RoadmapStoreError::storage("update milestone", error))?;
        transaction
            .commit()
            .map_err(|error| RoadmapStoreError::storage("commit milestone update", error))?;
        Ok(milestone)
    }

    fn delete(&mut self, milestone_id: &str) -> Result<bool, RoadmapStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RoadmapStoreError::storage("begin milestone delete", error))?;
        let deleted = transaction
            .execute(
                "DELETE FROM milestones WHERE id = ?1",
                params![milestone_id],
            )
            .map_err(|error| RoadmapStoreError::storage("delete milestone", error))?;
        transaction
            .commit()
            .map_err(|error| RoadmapStoreError::storage("commit milestone delete", error))?;
        Ok(deleted > 0)
    }

    fn reorder(
        &mut self,
        project_id: &str,
        ordered_ids: Vec<String>,
    ) -> Result<Vec<Milestone>, RoadmapStoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RoadmapStoreError::storage("begin milestone reorder", error))?;
        ensure_project_exists(&transaction, project_id)?;
        validate_reorder(&transaction, project_id, &ordered_ids)?;
        for (order, milestone_id) in ordered_ids.iter().enumerate() {
            transaction
                .execute(
                    "UPDATE milestones SET sort_order = ?1 WHERE id = ?2 AND project_id = ?3",
                    params![order as i64, milestone_id, project_id],
                )
                .map_err(|error| RoadmapStoreError::storage("write milestone order", error))?;
        }
        transaction
            .commit()
            .map_err(|error| RoadmapStoreError::storage("commit milestone reorder", error))?;
        Ok(list_roadmap(&self.connection, project_id)?.milestones)
    }

    fn set_tasks(
        &mut self,
        milestone_id: &str,
        task_ids: Vec<String>,
    ) -> Result<Vec<TaskMilestoneLink>, RoadmapStoreError> {
        let task_ids = deduplicate(task_ids);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| RoadmapStoreError::storage("begin milestone task update", error))?;
        let project_id = transaction
            .query_row(
                "SELECT project_id FROM milestones WHERE id = ?1",
                params![milestone_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| RoadmapStoreError::storage("read milestone project", error))?
            .ok_or_else(|| RoadmapStoreError::MilestoneNotFound {
                milestone_id: milestone_id.to_owned(),
            })?;
        for task_id in &task_ids {
            let eligible = transaction
                .query_row(
                    "SELECT 1 FROM tasks WHERE id = ?1 AND project_id = ?2 AND archived = 0",
                    params![task_id, project_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|error| RoadmapStoreError::storage("validate milestone task", error))?
                .is_some();
            if !eligible {
                return Err(RoadmapStoreError::TaskNotEligible {
                    task_id: task_id.clone(),
                    project_id,
                });
            }
        }

        transaction
            .execute(
                "DELETE FROM task_milestone_links WHERE milestone_id = ?1",
                params![milestone_id],
            )
            .map_err(|error| RoadmapStoreError::storage("clear milestone tasks", error))?;
        for task_id in &task_ids {
            transaction
                .execute(
                    "INSERT INTO task_milestone_links (task_id, milestone_id) VALUES (?1, ?2)",
                    params![task_id, milestone_id],
                )
                .map_err(|error| RoadmapStoreError::storage("link milestone task", error))?;
        }
        transaction
            .commit()
            .map_err(|error| RoadmapStoreError::storage("commit milestone task update", error))?;

        Ok(task_ids
            .into_iter()
            .map(|task_id| TaskMilestoneLink {
                task_id,
                milestone_id: milestone_id.to_owned(),
            })
            .collect())
    }
}

fn list_roadmap(
    connection: &Connection,
    project_id: &str,
) -> Result<ProjectRoadmap, RoadmapStoreError> {
    let mut milestone_statement = connection
        .prepare(
            r#"SELECT id, project_id, title, description, status, due_date, sort_order, created_at
               FROM milestones
               WHERE project_id = ?1
               ORDER BY sort_order ASC, created_at ASC"#,
        )
        .map_err(|error| RoadmapStoreError::storage("prepare milestone list", error))?;
    let raw_milestones = milestone_statement
        .query_map(params![project_id], raw_milestone)
        .map_err(|error| RoadmapStoreError::storage("query milestone list", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RoadmapStoreError::storage("read milestone list", error))?;
    let milestones = raw_milestones
        .into_iter()
        .map(decode_milestone)
        .collect::<Result<Vec<_>, _>>()?;

    let mut link_statement = connection
        .prepare(
            r#"SELECT l.task_id, l.milestone_id
               FROM task_milestone_links l
               INNER JOIN milestones m ON m.id = l.milestone_id
               INNER JOIN tasks t ON t.id = l.task_id
               WHERE m.project_id = ?1 AND t.project_id = ?1 AND t.archived = 0
               ORDER BY m.sort_order ASC, t.sort_order ASC, t.created_at ASC"#,
        )
        .map_err(|error| RoadmapStoreError::storage("prepare milestone links", error))?;
    let links = link_statement
        .query_map(params![project_id], |row| {
            Ok(TaskMilestoneLink {
                task_id: row.get(0)?,
                milestone_id: row.get(1)?,
            })
        })
        .map_err(|error| RoadmapStoreError::storage("query milestone links", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RoadmapStoreError::storage("read milestone links", error))?;

    Ok(ProjectRoadmap { milestones, links })
}

fn milestone_by_id(
    connection: &Connection,
    milestone_id: &str,
) -> Result<Option<Milestone>, RoadmapStoreError> {
    let raw = connection
        .query_row(
            r#"SELECT id, project_id, title, description, status, due_date, sort_order, created_at
               FROM milestones WHERE id = ?1"#,
            params![milestone_id],
            raw_milestone,
        )
        .optional()
        .map_err(|error| RoadmapStoreError::storage("read milestone", error))?;
    raw.map(decode_milestone).transpose()
}

fn ensure_project_exists(
    transaction: &Transaction<'_>,
    project_id: &str,
) -> Result<(), RoadmapStoreError> {
    let exists = transaction
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| RoadmapStoreError::storage("read project", error))?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(RoadmapStoreError::ProjectNotFound {
            project_id: project_id.to_owned(),
        })
    }
}

fn validate_reorder(
    transaction: &Transaction<'_>,
    project_id: &str,
    ordered_ids: &[String],
) -> Result<(), RoadmapStoreError> {
    let mut provided = BTreeSet::new();
    for milestone_id in ordered_ids {
        if !provided.insert(milestone_id.clone()) {
            return Err(RoadmapStoreError::DuplicateReorderId {
                milestone_id: milestone_id.clone(),
            });
        }
    }
    let mut statement = transaction
        .prepare("SELECT id FROM milestones WHERE project_id = ?1")
        .map_err(|error| RoadmapStoreError::storage("prepare reorder validation", error))?;
    let expected = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(|error| RoadmapStoreError::storage("query reorder validation", error))?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| RoadmapStoreError::storage("read reorder validation", error))?;
    if expected == provided {
        return Ok(());
    }
    Err(RoadmapStoreError::IncompleteReorder {
        project_id: project_id.to_owned(),
        missing: expected.difference(&provided).cloned().collect(),
        unexpected: provided.difference(&expected).cloned().collect(),
    })
}

fn normalized_title(title: &str) -> Result<String, RoadmapStoreError> {
    let title = title.trim();
    if title.is_empty() {
        Err(RoadmapStoreError::InvalidTitle)
    } else {
        Ok(title.to_owned())
    }
}

fn deduplicate(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

struct RawMilestone {
    id: String,
    project_id: String,
    title: String,
    description: String,
    status: String,
    due_date: Option<i64>,
    order: i64,
    created_at: i64,
}

fn raw_milestone(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMilestone> {
    Ok(RawMilestone {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        due_date: row.get(5)?,
        order: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn decode_milestone(raw: RawMilestone) -> Result<Milestone, RoadmapStoreError> {
    let status = MilestoneStatus::from_storage(&raw.status).ok_or_else(|| {
        RoadmapStoreError::InvalidStoredStatus {
            milestone_id: raw.id.clone(),
            status: raw.status,
        }
    })?;
    Ok(Milestone {
        id: raw.id,
        project_id: raw.project_id,
        title: raw.title,
        description: raw.description,
        status,
        due_date: raw.due_date,
        order: raw.order,
        created_at: raw.created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteRoadmapStore {
        let store = SqliteRoadmapStore::in_memory().unwrap();
        store
            .connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, name, created_at)
                  VALUES ('p1', 'P1', 1), ('p2', 'P2', 1);
                INSERT INTO tasks
                  (id, project_id, session_id, title, status, created_at, sort_order)
                  VALUES
                    ('t1', 'p1', 't1', 'T1', 'waiting', 1, 0),
                    ('t2', 'p1', 't2', 'T2', 'done', 2, 1),
                    ('t3', 'p2', 't3', 'T3', 'waiting', 3, 0),
                    ('t4', 'p1', 't4', 'T4', 'waiting', 4, 2);
                UPDATE tasks SET archived = 1 WHERE id = 't4';
                INSERT INTO milestones
                  (id, project_id, title, description, status, sort_order, created_at)
                  VALUES
                    ('m1', 'p1', 'M1', '', 'upcoming', 0, 1),
                    ('m2', 'p1', 'M2', '', 'in-progress', 1, 2),
                    ('m3', 'p2', 'M3', '', 'done', 0, 1);
                "#,
            )
            .unwrap();
        store
    }

    #[test]
    fn types_match_frontend_contract_json() {
        let milestone = Milestone {
            id: "m1".into(),
            project_id: "p1".into(),
            title: "M1".into(),
            description: String::new(),
            status: MilestoneStatus::InProgress,
            due_date: Some(10),
            order: 0,
            created_at: 1,
        };
        let json = serde_json::to_value(milestone).unwrap();
        assert_eq!(json["projectId"], "p1");
        assert_eq!(json["status"], "in-progress");
        assert_eq!(json["dueDate"], 10);
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../packages/contracts/src/task-statuses.json"
        ))
        .unwrap();
        let contract_statuses = manifest["milestoneStatuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            MilestoneStatus::ALL
                .iter()
                .map(|status| status.as_str())
                .collect::<Vec<_>>(),
            contract_statuses
        );
        assert_eq!(
            MilestoneStatus::default().as_str(),
            manifest["defaultMilestoneStatus"].as_str().unwrap()
        );
    }

    #[test]
    fn create_and_update_trim_fields_and_round_trip_due_date() {
        let mut store = store();
        let created = store.create("p1", "  Delivery  ").unwrap();
        assert_eq!(created.title, "Delivery");
        assert_eq!(created.order, 2);
        assert_eq!(created.status, MilestoneStatus::Upcoming);

        let updated = store
            .update(
                &created.id,
                MilestoneUpdatePatch {
                    description: Some("  acceptance  ".into()),
                    status: Some(MilestoneStatus::Done),
                    due_date: MilestoneDueDateUpdate::Set(42),
                    ..MilestoneUpdatePatch::default()
                },
            )
            .unwrap();
        assert_eq!(updated.description, "acceptance");
        assert_eq!(updated.status, MilestoneStatus::Done);
        assert_eq!(updated.due_date, Some(42));

        let cleared = store
            .update(
                &created.id,
                MilestoneUpdatePatch {
                    due_date: MilestoneDueDateUpdate::Clear,
                    ..MilestoneUpdatePatch::default()
                },
            )
            .unwrap();
        assert_eq!(cleared.due_date, None);
    }

    #[test]
    fn invalid_update_is_atomic() {
        let mut store = store();
        let error = store
            .update(
                "m1",
                MilestoneUpdatePatch {
                    title: Some("   ".into()),
                    description: Some("must not persist".into()),
                    status: Some(MilestoneStatus::Done),
                    ..MilestoneUpdatePatch::default()
                },
            )
            .unwrap_err();
        assert!(matches!(error, RoadmapStoreError::InvalidTitle));
        let milestone = store.list("p1").unwrap().milestones.remove(0);
        assert_eq!(milestone.description, "");
        assert_eq!(milestone.status, MilestoneStatus::Upcoming);
    }

    #[test]
    fn reorder_requires_the_complete_project_set_and_commits_atomically() {
        let mut store = store();
        let errors = [
            store.reorder("p1", vec!["m1".into()]).unwrap_err(),
            store
                .reorder("p1", vec!["m1".into(), "m1".into()])
                .unwrap_err(),
            store
                .reorder("p1", vec!["m1".into(), "m3".into()])
                .unwrap_err(),
        ];
        assert!(matches!(
            errors[0],
            RoadmapStoreError::IncompleteReorder { .. }
        ));
        assert!(matches!(
            errors[1],
            RoadmapStoreError::DuplicateReorderId { .. }
        ));
        assert!(matches!(
            errors[2],
            RoadmapStoreError::IncompleteReorder { .. }
        ));
        assert_eq!(
            store
                .list("p1")
                .unwrap()
                .milestones
                .iter()
                .map(|milestone| milestone.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m1", "m2"]
        );

        let reordered = store.reorder("p1", vec!["m2".into(), "m1".into()]).unwrap();
        assert_eq!(
            reordered
                .iter()
                .map(|milestone| (milestone.id.as_str(), milestone.order))
                .collect::<Vec<_>>(),
            vec![("m2", 0), ("m1", 1)]
        );
        assert_eq!(store.list("p2").unwrap().milestones[0].order, 0);
    }

    #[test]
    fn task_links_are_deduplicated_and_invalid_replace_rolls_back() {
        let mut store = store();
        let links = store
            .set_tasks("m1", vec!["t2".into(), "t1".into(), "t2".into()])
            .unwrap();
        assert_eq!(
            links
                .iter()
                .map(|link| link.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["t2", "t1"]
        );

        for invalid_task in ["t3", "t4", "missing"] {
            let error = store
                .set_tasks("m1", vec![invalid_task.to_owned()])
                .unwrap_err();
            assert!(matches!(error, RoadmapStoreError::TaskNotEligible { .. }));
            assert_eq!(store.list("p1").unwrap().links.len(), 2);
        }
    }

    #[test]
    fn list_filters_legacy_ineligible_links_and_delete_cascades_only_links() {
        let mut store = store();
        store
            .connection
            .execute_batch(
                r#"
                INSERT INTO task_milestone_links (task_id, milestone_id)
                  VALUES ('t1', 'm1'), ('t4', 'm1');
                "#,
            )
            .unwrap();
        let roadmap = store.list("p1").unwrap();
        assert_eq!(roadmap.links.len(), 1);
        assert_eq!(roadmap.links[0].task_id, "t1");

        assert!(store.delete("m1").unwrap());
        assert!(!store.delete("m1").unwrap());
        let task_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        let link_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM task_milestone_links", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(task_count, 4);
        assert_eq!(link_count, 0);
    }

    #[test]
    fn missing_records_return_typed_errors() {
        let mut store = store();
        assert!(matches!(
            store.create("missing", "M"),
            Err(RoadmapStoreError::ProjectNotFound { .. })
        ));
        assert!(matches!(
            store.update("missing", MilestoneUpdatePatch::default()),
            Err(RoadmapStoreError::MilestoneNotFound { .. })
        ));
        assert!(matches!(
            store.set_tasks("missing", vec![]),
            Err(RoadmapStoreError::MilestoneNotFound { .. })
        ));
    }

    #[test]
    fn corrupt_legacy_status_is_reported_instead_of_normalized() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE milestones (
                  id TEXT PRIMARY KEY,
                  project_id TEXT NOT NULL,
                  title TEXT NOT NULL,
                  description TEXT NOT NULL DEFAULT '',
                  status TEXT NOT NULL,
                  due_date INTEGER,
                  sort_order INTEGER NOT NULL DEFAULT 0,
                  created_at INTEGER NOT NULL
                );
                "#,
            )
            .unwrap();
        let store = SqliteRoadmapStore::from_connection(connection).unwrap();
        store
            .connection
            .execute(
                r#"INSERT INTO milestones
                   (id, project_id, title, status, sort_order, created_at)
                   VALUES ('broken', 'p1', 'Broken', 'running', 0, 1)"#,
                [],
            )
            .unwrap();

        let error = store.list("p1").unwrap_err();
        assert!(matches!(
            error,
            RoadmapStoreError::InvalidStoredStatus {
                milestone_id,
                status
            } if milestone_id == "broken" && status == "running"
        ));
    }

    #[test]
    fn file_store_reopens_the_existing_roadmap_schema_and_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lilia.db");
        let connection = SqliteRoadmapStore::open(&path).unwrap().into_connection();
        connection
            .execute(
                "INSERT INTO projects (id, name, created_at) VALUES ('p1', 'P1', 1)",
                [],
            )
            .unwrap();
        drop(connection);

        let mut store = SqliteRoadmapStore::open(&path).unwrap();
        let created = store.create("p1", "Persistent").unwrap();
        drop(store);

        let roadmap = SqliteRoadmapStore::open(&path).unwrap().list("p1").unwrap();
        assert_eq!(roadmap.milestones.len(), 1);
        assert_eq!(roadmap.milestones[0].id, created.id);
        assert_eq!(roadmap.milestones[0].title, "Persistent");
    }
}
