use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{ChatConversationReference, LiliaAgentWorkflow, TaskId, TodoProjection};
use lilia_storage::Db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::DesktopTurnDispatch;

const LILIA_GUIDE_PREFIX: &str = "[Lilia 引导]";

const TODO_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS task_todos (
  id               TEXT PRIMARY KEY,
  task_id          TEXT NOT NULL,
  text             TEXT NOT NULL,
  done             INTEGER NOT NULL DEFAULT 0,
  "order"          INTEGER NOT NULL,
  source           TEXT NOT NULL CHECK (source IN ('lilia','agent')),
  priority         TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('high','normal','low')),
  guide_status     TEXT CHECK (guide_status IS NULL OR guide_status IN ('pending','queued','sent')),
  attachments_json TEXT NOT NULL DEFAULT '[]',
  conversation_references_json TEXT NOT NULL DEFAULT '[]',
  workflow_json    TEXT,
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_task_todos_task_id_order
  ON task_todos(task_id, "order");
"#;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTodoPriority {
    High,
    #[default]
    Normal,
    Low,
}

impl DesktopTodoPriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "high" => Self::High,
            "low" => Self::Low,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTodoSource {
    Lilia,
    Agent,
}

impl DesktopTodoSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lilia => "lilia",
            Self::Agent => "agent",
        }
    }

    fn parse(value: &str) -> Result<Self, DesktopTodoError> {
        match value {
            "lilia" => Ok(Self::Lilia),
            "agent" => Ok(Self::Agent),
            other => Err(DesktopTodoError::InvalidStoredValue {
                field: "source",
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTodoGuideStatus {
    Pending,
    Queued,
    Sent,
}

impl DesktopTodoGuideStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Sent => "sent",
        }
    }

    fn parse(value: &str) -> Result<Self, DesktopTodoError> {
        match value {
            "pending" => Ok(Self::Pending),
            "queued" => Ok(Self::Queued),
            "sent" => Ok(Self::Sent),
            other => Err(DesktopTodoError::InvalidStoredValue {
                field: "guide_status",
                value: other.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopGuideDispatchWindow {
    Tool,
    User,
    Idle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopGuideDispatchResult {
    pub guide: DesktopTaskTodo,
    pub turn: DesktopTurnDispatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTaskTodo {
    pub id: String,
    pub task_id: TaskId,
    pub text: String,
    pub done: bool,
    pub order: i64,
    pub source: DesktopTodoSource,
    pub priority: DesktopTodoPriority,
    pub guide_status: Option<DesktopTodoGuideStatus>,
    pub attachments: Vec<Value>,
    pub conversation_references: Vec<ChatConversationReference>,
    pub workflow: Option<LiliaAgentWorkflow>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTodoCreate {
    pub task_id: TaskId,
    pub text: String,
    #[serde(default)]
    pub priority: DesktopTodoPriority,
    #[serde(default)]
    pub attachments: Vec<Value>,
    #[serde(default)]
    pub conversation_references: Vec<ChatConversationReference>,
    #[serde(default)]
    pub workflow: Option<LiliaAgentWorkflow>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTodoUpdate {
    pub text: Option<String>,
    pub done: Option<bool>,
    pub order: Option<i64>,
    pub priority: Option<DesktopTodoPriority>,
    pub guide_status: Option<DesktopTodoGuideStatus>,
}

pub struct DesktopTodoStore {
    connection: Db,
}

impl DesktopTodoStore {
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, DesktopTodoError> {
        let connection = Db::in_memory().map_err(|error| DesktopTodoError::Storage {
            operation: "open in-memory database",
            message: error.to_string(),
        })?;
        Self::from_shared(connection)
    }

    pub fn from_shared(connection: Db) -> Result<Self, DesktopTodoError> {
        let locked = connection.lock();
        locked
            .execute_batch(TODO_SCHEMA)
            .map_err(|error| DesktopTodoError::Storage {
                operation: "initialize todo schema",
                message: error.to_string(),
            })?;
        ensure_column(
            &locked,
            "task_todos",
            "conversation_references_json",
            "ALTER TABLE task_todos ADD COLUMN conversation_references_json TEXT NOT NULL DEFAULT '[]'",
        )?;
        ensure_column(
            &locked,
            "task_todos",
            "workflow_json",
            "ALTER TABLE task_todos ADD COLUMN workflow_json TEXT",
        )?;
        drop(locked);
        Ok(Self { connection })
    }

    pub fn list(&self, task_id: &TaskId) -> Result<Vec<DesktopTaskTodo>, DesktopTodoError> {
        let connection = self.connection();
        Self::list_from(&connection, task_id)
    }

    pub fn list_from(
        connection: &Connection,
        task_id: &TaskId,
    ) -> Result<Vec<DesktopTaskTodo>, DesktopTodoError> {
        let mut statement = connection
            .prepare(
                r#"SELECT id, task_id, text, done, "order", source, priority,
                          guide_status, attachments_json, created_at, updated_at,
                          conversation_references_json, workflow_json
                   FROM task_todos WHERE task_id = ?1
                   ORDER BY "order" ASC, created_at ASC, id ASC"#,
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "prepare todo list",
                message: error.to_string(),
            })?;
        let rows = statement
            .query_map(params![task_id.as_str()], row_to_todo)
            .map_err(|error| DesktopTodoError::Storage {
                operation: "query todo list",
                message: error.to_string(),
            })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "decode todo list",
                message: error.to_string(),
            })
    }

    pub fn create(&self, input: DesktopTodoCreate) -> Result<DesktopTaskTodo, DesktopTodoError> {
        self.create_idempotent(
            &Uuid::new_v4().to_string(),
            input,
            DesktopTodoSource::Lilia,
            Some(DesktopTodoGuideStatus::Pending),
        )
        .map(|(todo, _)| todo)
    }

    pub fn create_idempotent(
        &self,
        id: &str,
        input: DesktopTodoCreate,
        source: DesktopTodoSource,
        guide_status: Option<DesktopTodoGuideStatus>,
    ) -> Result<(DesktopTaskTodo, bool), DesktopTodoError> {
        let connection = self.connection();
        let transaction =
            connection
                .unchecked_transaction()
                .map_err(|error| DesktopTodoError::Storage {
                    operation: "begin idempotent todo creation",
                    message: error.to_string(),
                })?;
        let result = Self::create_idempotent_in(&transaction, id, input, source, guide_status)?;
        transaction
            .commit()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "commit idempotent todo creation",
                message: error.to_string(),
            })?;
        Ok(result)
    }

    pub fn create_idempotent_in(
        connection: &Connection,
        id: &str,
        input: DesktopTodoCreate,
        source: DesktopTodoSource,
        guide_status: Option<DesktopTodoGuideStatus>,
    ) -> Result<(DesktopTaskTodo, bool), DesktopTodoError> {
        let text = normalized_text(&input.text)?;
        if let Some(existing) = Self::get_from(connection, id)? {
            if existing.task_id == input.task_id
                && existing.text == text
                && existing.source == source
                && existing.priority == input.priority
                && existing.guide_status == guide_status
                && existing.attachments == input.attachments
                && existing.conversation_references == input.conversation_references
                && existing.workflow == input.workflow
            {
                return Ok((existing, false));
            }
            return Err(DesktopTodoError::IdempotencyConflict { id: id.to_owned() });
        }
        let order = connection
            .query_row(
                r#"SELECT COALESCE(MAX("order"), -1) + 1 FROM task_todos WHERE task_id = ?1"#,
                params![input.task_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "read next todo order",
                message: error.to_string(),
            })?;
        let now = now_millis();
        let todo = DesktopTaskTodo {
            id: id.to_owned(),
            task_id: input.task_id,
            text,
            done: false,
            order,
            source,
            priority: input.priority,
            guide_status,
            attachments: input.attachments,
            conversation_references: input.conversation_references,
            workflow: input.workflow,
            created_at: now,
            updated_at: now,
        };
        let attachments = serde_json::to_string(&todo.attachments).map_err(|error| {
            DesktopTodoError::Storage {
                operation: "encode todo attachments",
                message: error.to_string(),
            }
        })?;
        let conversation_references = serde_json::to_string(&todo.conversation_references)
            .map_err(|error| DesktopTodoError::Storage {
                operation: "encode todo conversation references",
                message: error.to_string(),
            })?;
        let workflow = todo
            .workflow
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "encode todo workflow",
                message: error.to_string(),
            })?;
        connection
            .execute(
                r#"INSERT INTO task_todos
                   (id, task_id, text, done, "order", source, priority, guide_status,
                     attachments_json, created_at, updated_at, conversation_references_json,
                     workflow_json)
                    VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
                params![
                    todo.id,
                    todo.task_id.as_str(),
                    todo.text,
                    todo.order,
                    todo.source.as_str(),
                    todo.priority.as_str(),
                    todo.guide_status.map(DesktopTodoGuideStatus::as_str),
                    attachments,
                    todo.created_at,
                    todo.updated_at,
                    conversation_references,
                    workflow,
                ],
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "insert todo",
                message: error.to_string(),
            })?;
        Ok((todo, true))
    }

    pub fn update(
        &self,
        id: &str,
        update: DesktopTodoUpdate,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        let current = self.get_editable(id)?;
        let Some(mut todo) = current else {
            return Ok(None);
        };
        if let Some(text) = update.text {
            todo.text = normalized_text(&text)?;
        }
        if let Some(done) = update.done {
            todo.done = done;
        }
        if let Some(order) = update.order {
            todo.order = order;
        }
        if let Some(priority) = update.priority {
            todo.priority = priority;
        }
        if let Some(guide_status) = update.guide_status {
            todo.guide_status = Some(guide_status);
        }
        todo.updated_at = now_millis();
        self.connection()
            .execute(
                r#"UPDATE task_todos SET text = ?1, done = ?2, "order" = ?3,
                          priority = ?4, guide_status = ?5, updated_at = ?6
                   WHERE id = ?7 AND source = 'lilia'"#,
                params![
                    todo.text,
                    i64::from(todo.done),
                    todo.order,
                    todo.priority.as_str(),
                    todo.guide_status.map(DesktopTodoGuideStatus::as_str),
                    todo.updated_at,
                    todo.id,
                ],
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "update todo",
                message: error.to_string(),
            })?;
        Ok(Some(todo))
    }

    pub fn delete(&self, id: &str) -> Result<Option<TaskId>, DesktopTodoError> {
        let task_id = self
            .connection()
            .query_row(
                "SELECT task_id FROM task_todos WHERE id = ?1 AND source = 'lilia'",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "find todo before delete",
                message: error.to_string(),
            })?
            .map(TaskId::new)
            .transpose()?;
        if task_id.is_none() {
            return Ok(None);
        }
        self.connection()
            .execute(
                "DELETE FROM task_todos WHERE id = ?1 AND source = 'lilia'",
                params![id],
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "delete todo",
                message: error.to_string(),
            })?;
        Ok(task_id)
    }

    fn get_editable(&self, id: &str) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        self.connection()
            .query_row(
                r#"SELECT id, task_id, text, done, "order", source, priority,
                          guide_status, attachments_json, created_at, updated_at,
                          conversation_references_json, workflow_json
                   FROM task_todos WHERE id = ?1 AND source = 'lilia'"#,
                params![id],
                row_to_todo,
            )
            .optional()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "read todo",
                message: error.to_string(),
            })
    }

    pub fn get_from(
        connection: &Connection,
        id: &str,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        connection
            .query_row(
                r#"SELECT id, task_id, text, done, "order", source, priority,
                          guide_status, attachments_json, created_at, updated_at,
                          conversation_references_json, workflow_json
                   FROM task_todos WHERE id = ?1"#,
                params![id],
                row_to_todo,
            )
            .optional()
            .map_err(|error| DesktopTodoError::Storage {
                operation: "read todo by id",
                message: error.to_string(),
            })
    }

    pub fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.connection.lock()
    }

    pub fn select_pending_guide(
        &self,
        task_id: &TaskId,
        window: DesktopGuideDispatchWindow,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        let connection = self.connection();
        Self::select_pending_guide_from(&connection, task_id, window)
    }

    pub fn select_pending_guide_by_id(
        &self,
        task_id: &TaskId,
        guide_id: &str,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        Ok(self.get_editable(guide_id)?.filter(|todo| {
            &todo.task_id == task_id
                && !todo.done
                && todo.guide_status == Some(DesktopTodoGuideStatus::Pending)
        }))
    }

    pub fn select_pending_guide_from(
        connection: &Connection,
        task_id: &TaskId,
        window: DesktopGuideDispatchWindow,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        Ok(Self::list_from(connection, task_id)?
            .into_iter()
            .filter(|todo| {
                todo.source == DesktopTodoSource::Lilia
                    && !todo.done
                    && todo.guide_status == Some(DesktopTodoGuideStatus::Pending)
                    && guide_priority_allowed(window, todo.priority)
            })
            .min_by_key(|todo| {
                (
                    guide_priority_rank(todo.priority),
                    todo.order,
                    todo.created_at,
                    todo.id.clone(),
                )
            }))
    }

    pub fn set_guide_status_in(
        connection: &Connection,
        id: &str,
        status: DesktopTodoGuideStatus,
    ) -> Result<Option<DesktopTaskTodo>, DesktopTodoError> {
        let changed = connection
            .execute(
                "UPDATE task_todos SET guide_status = ?1, updated_at = ?2 WHERE id = ?3 AND source = 'lilia'",
                params![status.as_str(), now_millis(), id],
            )
            .map_err(|error| DesktopTodoError::Storage {
                operation: "set Guide status",
                message: error.to_string(),
            })?;
        if changed == 0 {
            return Ok(None);
        }
        Self::get_from(connection, id)
    }
}

pub fn guide_message(todo: &DesktopTaskTodo) -> String {
    format!(
        "{LILIA_GUIDE_PREFIX}\n优先级：{}\n\n{}",
        guide_priority_label(todo.priority),
        todo.text
    )
}

fn guide_priority_allowed(
    window: DesktopGuideDispatchWindow,
    priority: DesktopTodoPriority,
) -> bool {
    match window {
        DesktopGuideDispatchWindow::Tool => priority == DesktopTodoPriority::High,
        DesktopGuideDispatchWindow::User => priority == DesktopTodoPriority::Normal,
        DesktopGuideDispatchWindow::Idle => true,
    }
}

fn guide_priority_rank(priority: DesktopTodoPriority) -> u8 {
    match priority {
        DesktopTodoPriority::High => 0,
        DesktopTodoPriority::Normal => 1,
        DesktopTodoPriority::Low => 2,
    }
}

fn guide_priority_label(priority: DesktopTodoPriority) -> &'static str {
    match priority {
        DesktopTodoPriority::High => "高",
        DesktopTodoPriority::Normal => "普通",
        DesktopTodoPriority::Low => "低",
    }
}

pub fn merge_todos_with_latest_projection(
    stored: Vec<DesktopTaskTodo>,
    projections: &[TodoProjection],
) -> Vec<DesktopTaskTodo> {
    let Some(latest) = projections
        .iter()
        .max_by_key(|projection| projection.sequence)
    else {
        return stored;
    };
    let mut todos = stored
        .into_iter()
        .filter(|todo| todo.source == DesktopTodoSource::Lilia)
        .collect::<Vec<_>>();
    let next_order = todos.iter().map(|todo| todo.order).max().unwrap_or(-1) + 1;
    let timestamp = i64::try_from(latest.sequence).unwrap_or(i64::MAX);
    for (index, value) in latest.items.as_array().into_iter().flatten().enumerate() {
        let Some((text, done, priority, item_id)) = projected_todo_fields(value) else {
            continue;
        };
        todos.push(DesktopTaskTodo {
            id: format!(
                "agent:{}:{}",
                latest.todo_id,
                item_id.unwrap_or_else(|| index.to_string())
            ),
            task_id: latest.task_id.clone(),
            text,
            done,
            order: next_order.saturating_add(i64::try_from(index).unwrap_or(i64::MAX)),
            source: DesktopTodoSource::Agent,
            priority,
            guide_status: None,
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            created_at: timestamp,
            updated_at: timestamp,
        });
    }
    todos
}

fn projected_todo_fields(
    value: &Value,
) -> Option<(String, bool, DesktopTodoPriority, Option<String>)> {
    if let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some((text.to_owned(), false, DesktopTodoPriority::Normal, None));
    }
    let object = value.as_object()?;
    let text = ["content", "text", "title", "description"]
        .iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))?
        .trim();
    if text.is_empty() {
        return None;
    }
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let done = object
        .get("completed")
        .and_then(Value::as_bool)
        .or_else(|| object.get("done").and_then(Value::as_bool))
        .unwrap_or_else(|| status.eq_ignore_ascii_case("completed"));
    let priority = object
        .get("priority")
        .and_then(Value::as_str)
        .map(DesktopTodoPriority::parse)
        .unwrap_or_default();
    let item_id = object
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Some((text.to_owned(), done, priority, item_id))
}

fn row_to_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<DesktopTaskTodo> {
    let task_id =
        TaskId::new(row.get::<_, String>(1)?).map_err(|error| invalid_data(error.to_string()))?;
    let source = DesktopTodoSource::parse(&row.get::<_, String>(5)?)
        .map_err(|error| invalid_data(error.to_string()))?;
    let priority = DesktopTodoPriority::parse(&row.get::<_, String>(6)?);
    let guide_status = row
        .get::<_, Option<String>>(7)?
        .map(|value| DesktopTodoGuideStatus::parse(&value))
        .transpose()
        .map_err(|error| invalid_data(error.to_string()))?;
    let attachments = serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default();
    let conversation_references = serde_json::from_str(&row.get::<_, String>(11)?)
        .map_err(|error| invalid_data(error.to_string()))?;
    let workflow = row
        .get::<_, Option<String>>(12)?
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| invalid_data(error.to_string()))?;
    Ok(DesktopTaskTodo {
        id: row.get(0)?,
        task_id,
        text: row.get(2)?,
        done: row.get::<_, i64>(3)? != 0,
        order: row.get(4)?,
        source,
        priority,
        guide_status,
        attachments,
        conversation_references,
        workflow,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn normalized_text(value: &str) -> Result<String, DesktopTodoError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DesktopTodoError::EmptyText);
    }
    Ok(value.to_owned())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    migration: &str,
) -> Result<(), DesktopTodoError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| DesktopTodoError::Storage {
            operation: "inspect todo schema",
            message: error.to_string(),
        })?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| DesktopTodoError::Storage {
            operation: "inspect todo schema",
            message: error.to_string(),
        })?;
    for candidate in columns {
        if candidate.map_err(|error| DesktopTodoError::Storage {
            operation: "inspect todo schema",
            message: error.to_string(),
        })? == column
        {
            return Ok(());
        }
    }
    connection
        .execute_batch(migration)
        .map_err(|error| DesktopTodoError::Storage {
            operation: "migrate todo schema",
            message: error.to_string(),
        })
}

fn now_millis() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn invalid_data(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopTodoError {
    #[error("todo text must not be empty")]
    EmptyText,
    #[error("todo idempotency key conflicts with existing todo `{id}`")]
    IdempotencyConflict { id: String },
    #[error("invalid stored todo {field} value `{value}`")]
    InvalidStoredValue { field: &'static str, value: String },
    #[error("guide `{guide_id}` contains an invalid attachment: {message}")]
    InvalidAttachment { guide_id: String, message: String },
    #[error("todo storage failed during {operation}: {message}")]
    Storage {
        operation: &'static str,
        message: String,
    },
    #[error(transparent)]
    Product(#[from] lilia_contracts::ProductError),
}

#[cfg(test)]
mod tests {
    use lilia_contracts::AgentSessionRef;
    use serde_json::json;

    use super::*;

    #[test]
    fn manual_todo_crud_preserves_old_desktop_schema_semantics() {
        let store = DesktopTodoStore::in_memory().unwrap();
        let task_id = TaskId::new("todo-task").unwrap();
        let created = store
            .create(DesktopTodoCreate {
                task_id: task_id.clone(),
                text: "  implement Native  ".to_owned(),
                priority: DesktopTodoPriority::High,
                attachments: vec![json!({"path": "src/main.rs"})],
                conversation_references: Vec::new(),
                workflow: Some(LiliaAgentWorkflow::LiliaCompact),
            })
            .unwrap();

        assert_eq!(created.text, "implement Native");
        assert_eq!(created.guide_status, Some(DesktopTodoGuideStatus::Pending));
        assert_eq!(created.workflow, Some(LiliaAgentWorkflow::LiliaCompact));
        let updated = store
            .update(
                &created.id,
                DesktopTodoUpdate {
                    done: Some(true),
                    priority: Some(DesktopTodoPriority::Low),
                    ..DesktopTodoUpdate::default()
                },
            )
            .unwrap()
            .unwrap();
        assert!(updated.done);
        assert_eq!(updated.priority, DesktopTodoPriority::Low);
        assert_eq!(store.list(&task_id).unwrap(), vec![updated.clone()]);
        assert_eq!(store.delete(&created.id).unwrap(), Some(task_id.clone()));
        assert!(store.list(&task_id).unwrap().is_empty());
    }

    #[test]
    fn legacy_todo_schema_migrates_optional_context_without_losing_guides() {
        let connection = Db::in_memory().unwrap();
        connection
            .lock()
            .execute_batch(
                r#"
                CREATE TABLE task_todos (
                  id TEXT PRIMARY KEY,
                  task_id TEXT NOT NULL,
                  text TEXT NOT NULL,
                  done INTEGER NOT NULL DEFAULT 0,
                  "order" INTEGER NOT NULL,
                  source TEXT NOT NULL CHECK (source IN ('lilia','agent')),
                  priority TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('high','normal','low')),
                  guide_status TEXT CHECK (guide_status IS NULL OR guide_status IN ('pending','queued','sent')),
                  attachments_json TEXT NOT NULL DEFAULT '[]',
                  created_at INTEGER NOT NULL,
                  updated_at INTEGER NOT NULL
                );
                INSERT INTO task_todos VALUES
                  ('legacy-guide', 'legacy-task', 'legacy guide', 0, 0, 'lilia', 'high', 'pending', '[]', 1, 1);
                "#,
            )
            .unwrap();
        let store = DesktopTodoStore::from_shared(connection).unwrap();
        let task_id = TaskId::new("legacy-task").unwrap();

        let guides = store.list(&task_id).unwrap();

        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].text, "legacy guide");
        assert!(guides[0].conversation_references.is_empty());
        assert_eq!(guides[0].workflow, None);
    }

    #[test]
    fn guide_windows_filter_priorities_and_idle_prefers_high_then_normal_then_low() {
        let store = DesktopTodoStore::in_memory().unwrap();
        let task_id = TaskId::new("guide-priority-task").unwrap();
        let create = |id: &str, text: &str, priority: DesktopTodoPriority| {
            store
                .create_idempotent(
                    id,
                    DesktopTodoCreate {
                        task_id: task_id.clone(),
                        text: text.to_owned(),
                        priority,
                        attachments: Vec::new(),
                        conversation_references: Vec::new(),
                        workflow: None,
                    },
                    DesktopTodoSource::Lilia,
                    Some(DesktopTodoGuideStatus::Pending),
                )
                .unwrap()
                .0
        };
        let low = create("guide-low", "low", DesktopTodoPriority::Low);
        let normal = create("guide-normal", "normal", DesktopTodoPriority::Normal);
        let high = create("guide-high", "high", DesktopTodoPriority::High);

        assert_eq!(
            store
                .select_pending_guide(&task_id, DesktopGuideDispatchWindow::Tool)
                .unwrap(),
            Some(high.clone())
        );
        assert_eq!(
            store
                .select_pending_guide(&task_id, DesktopGuideDispatchWindow::User)
                .unwrap(),
            Some(normal.clone())
        );
        assert_eq!(
            store
                .select_pending_guide(&task_id, DesktopGuideDispatchWindow::Idle)
                .unwrap(),
            Some(high.clone())
        );

        store
            .update(
                &high.id,
                DesktopTodoUpdate {
                    done: Some(true),
                    ..DesktopTodoUpdate::default()
                },
            )
            .unwrap();
        assert_eq!(
            store
                .select_pending_guide(&task_id, DesktopGuideDispatchWindow::Idle)
                .unwrap(),
            Some(normal)
        );
        store
            .update(
                &low.id,
                DesktopTodoUpdate {
                    guide_status: Some(DesktopTodoGuideStatus::Queued),
                    ..DesktopTodoUpdate::default()
                },
            )
            .unwrap();
        assert_eq!(
            store
                .select_pending_guide(&task_id, DesktopGuideDispatchWindow::Tool)
                .unwrap(),
            None
        );
    }

    #[test]
    fn explicit_guide_selection_dispatches_only_the_requested_pending_task_guide() {
        let store = DesktopTodoStore::in_memory().unwrap();
        let task_id = TaskId::new("explicit-guide-task").unwrap();
        let other_task_id = TaskId::new("other-guide-task").unwrap();
        let create = |id: &str, task_id: TaskId| {
            store
                .create_idempotent(
                    id,
                    DesktopTodoCreate {
                        task_id,
                        text: id.to_owned(),
                        priority: DesktopTodoPriority::Normal,
                        attachments: Vec::new(),
                        conversation_references: Vec::new(),
                        workflow: None,
                    },
                    DesktopTodoSource::Lilia,
                    Some(DesktopTodoGuideStatus::Pending),
                )
                .unwrap()
                .0
        };
        let requested = create("guide-requested", task_id.clone());
        let other_task = create("guide-other-task", other_task_id);
        let queued = create("guide-queued", task_id.clone());
        store
            .update(
                &queued.id,
                DesktopTodoUpdate {
                    guide_status: Some(DesktopTodoGuideStatus::Queued),
                    ..DesktopTodoUpdate::default()
                },
            )
            .unwrap();

        assert_eq!(
            store
                .select_pending_guide_by_id(&task_id, &requested.id)
                .unwrap(),
            Some(requested)
        );
        assert_eq!(
            store
                .select_pending_guide_by_id(&task_id, &other_task.id)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .select_pending_guide_by_id(&task_id, &queued.id)
                .unwrap(),
            None
        );
    }

    #[test]
    fn guide_message_preserves_the_product_prefix_and_priority_label() {
        let task_id = TaskId::new("guide-message-task").unwrap();
        let guide = DesktopTaskTodo {
            id: "guide-message".to_owned(),
            task_id,
            text: "inspect the failure".to_owned(),
            done: false,
            order: 0,
            source: DesktopTodoSource::Lilia,
            priority: DesktopTodoPriority::High,
            guide_status: Some(DesktopTodoGuideStatus::Pending),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: Some(LiliaAgentWorkflow::LiliaCompact),
            created_at: 1,
            updated_at: 1,
        };

        assert_eq!(
            guide_message(&guide),
            "[Lilia 引导]\n优先级：高\n\ninspect the failure"
        );
    }

    #[test]
    fn latest_agent_projection_replaces_only_agent_rows() {
        let task_id = TaskId::new("projection-task").unwrap();
        let manual = DesktopTaskTodo {
            id: "manual".to_owned(),
            task_id: task_id.clone(),
            text: "manual".to_owned(),
            done: false,
            order: 0,
            source: DesktopTodoSource::Lilia,
            priority: DesktopTodoPriority::Normal,
            guide_status: Some(DesktopTodoGuideStatus::Pending),
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            created_at: 1,
            updated_at: 1,
        };
        let stale_agent = DesktopTaskTodo {
            id: "stale".to_owned(),
            task_id: task_id.clone(),
            text: "stale".to_owned(),
            done: false,
            order: 1,
            source: DesktopTodoSource::Agent,
            priority: DesktopTodoPriority::Normal,
            guide_status: None,
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
            created_at: 1,
            updated_at: 1,
        };
        let projection = TodoProjection {
            id: "projection".to_owned(),
            task_id: task_id.clone(),
            agent_session: AgentSessionRef::new("session").unwrap(),
            sequence: 7,
            turn_id: Some("turn".to_owned()),
            todo_id: "checklist".to_owned(),
            revision: 1,
            items: json!([
                {"id": "one", "content": "native", "status": "completed", "priority": "high"},
                "verify"
            ]),
        };

        let merged = merge_todos_with_latest_projection(vec![manual, stale_agent], &[projection]);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].source, DesktopTodoSource::Lilia);
        assert_eq!(merged[1].id, "agent:checklist:one");
        assert!(merged[1].done);
        assert_eq!(merged[1].priority, DesktopTodoPriority::High);
        assert_eq!(merged[2].text, "verify");
    }
}
