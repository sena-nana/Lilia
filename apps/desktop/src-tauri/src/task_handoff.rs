use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::{
    cli_project::resolve_project_path,
    projects_tasks::{create_handoff_task, display_project_path, ensure_project_row_for_cwd},
    store::LiliaStore,
};

const STORE_READY_TIMEOUT: Duration = Duration::from_secs(5);
const STORE_READY_POLL: Duration = Duration::from_millis(50);
const TASK_HANDOFF_PROTOCOL: &str = "lilia-code-task-handoff";
const TASK_HANDOFF_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskHandoffRepository {
    full_name: String,
    worktree_path: String,
    branch: String,
    remote_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskHandoffSource {
    application: String,
    route: String,
    object_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestHandoffContext {
    number: u64,
    base_branch: String,
    head_branch: String,
    base_sha: Option<String>,
    head_sha: Option<String>,
    review_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowHandoffContext {
    run_id: u64,
    run_url: String,
    workflow_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiliaCodeTaskHandoff {
    protocol: String,
    version: u32,
    id: String,
    created_at: String,
    title: String,
    kind: String,
    repository: TaskHandoffRepository,
    source: TaskHandoffSource,
    problem: String,
    #[serde(default)]
    related_files: Vec<String>,
    log_summary: Option<String>,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    pull_request: Option<PullRequestHandoffContext>,
    workflow: Option<WorkflowHandoffContext>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskHandoffReceipt {
    protocol: &'static str,
    version: u32,
    handoff_id: String,
    status: &'static str,
    task_id: String,
    project_id: String,
    result_route: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportedTaskHandoff {
    task_id: String,
    handoff: LiliaCodeTaskHandoff,
    prompt: String,
}

pub(crate) struct TaskHandoffOpenPayload {
    pub(crate) project_id: String,
    pub(crate) cwd: String,
    pub(crate) task_id: String,
    pub(crate) handoff_id: String,
}

#[tauri::command]
pub(crate) fn task_handoff_get(
    task_id: String,
    store: State<'_, LiliaStore>,
) -> Result<Option<ImportedTaskHandoff>, String> {
    let conn = store.conn()?;
    let payload = conn
        .query_row(
            "SELECT payload_json FROM task_handoffs WHERE task_id = ?1",
            [task_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("task_handoff_get: {error}"))?;
    payload
        .map(|payload| {
            let handoff = parse_task_handoff(&payload)?;
            Ok(ImportedTaskHandoff {
                task_id,
                prompt: task_handoff_prompt(&handoff),
                handoff,
            })
        })
        .transpose()
}

pub(crate) fn resolve_task_handoff<R: Runtime>(
    app: &AppHandle<R>,
    handoff_path: &Path,
    cwd: &Path,
) -> Result<TaskHandoffOpenPayload, String> {
    let handoff_path = if handoff_path.is_absolute() {
        handoff_path.to_path_buf()
    } else {
        cwd.join(handoff_path)
    };
    let payload_json = fs::read_to_string(&handoff_path)
        .map_err(|error| format!("读取任务交接失败：{}：{error}", handoff_path.display()))?;
    let handoff = parse_task_handoff(&payload_json)?;
    let project_path = resolve_project_path(&handoff.repository.worktree_path, cwd)?;
    let project_cwd = display_project_path(&project_path);
    let started = Instant::now();
    loop {
        if let Some(store) = app.try_state::<LiliaStore>() {
            let mut conn = store.conn()?;
            let payload = import_task_handoff(&mut conn, &handoff, &payload_json, &project_cwd)?;
            let result_route =
                format!("/projects/{}/tasks/{}", payload.project_id, payload.task_id);
            write_task_handoff_receipt(
                &handoff_path,
                TaskHandoffReceipt {
                    protocol: TASK_HANDOFF_PROTOCOL,
                    version: TASK_HANDOFF_VERSION,
                    handoff_id: handoff.id.clone(),
                    status: "accepted",
                    task_id: payload.task_id.clone(),
                    project_id: payload.project_id.clone(),
                    result_route,
                    updated_at: now_millis().to_string(),
                },
            )?;
            return Ok(payload);
        }
        if started.elapsed() >= STORE_READY_TIMEOUT {
            return Err("项目存储尚未初始化".to_string());
        }
        thread::sleep(STORE_READY_POLL);
    }
}

fn import_task_handoff(
    conn: &mut Connection,
    handoff: &LiliaCodeTaskHandoff,
    payload_json: &str,
    project_cwd: &str,
) -> Result<TaskHandoffOpenPayload, String> {
    let transaction = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| format!("开始任务交接事务失败：{error}"))?;
    let existing = transaction
        .query_row(
            "SELECT h.task_id, t.project_id, p.cwd FROM task_handoffs h INNER JOIN tasks t ON t.id = h.task_id INNER JOIN projects p ON p.id = t.project_id WHERE h.handoff_id = ?1",
            [handoff.id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| format!("读取已有任务交接失败：{error}"))?;
    if let Some((task_id, project_id, cwd)) = existing {
        transaction
            .commit()
            .map_err(|error| format!("提交已有任务交接事务失败：{error}"))?;
        return Ok(TaskHandoffOpenPayload {
            project_id,
            cwd,
            task_id,
            handoff_id: handoff.id.clone(),
        });
    }

    let project = ensure_project_row_for_cwd(&transaction, project_cwd, "task_handoff")?;
    let task = create_handoff_task(&transaction, &project.id, handoff.title.trim())?;
    transaction
        .execute(
            "INSERT INTO task_handoffs (handoff_id, task_id, payload_json, source_route, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![handoff.id, task.id, payload_json, handoff.source.route, now_millis()],
        )
        .map_err(|error| format!("保存任务交接失败：{error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("提交任务交接事务失败：{error}"))?;
    Ok(TaskHandoffOpenPayload {
        project_id: project.id,
        cwd: project_cwd.to_string(),
        task_id: task.id,
        handoff_id: handoff.id.clone(),
    })
}

fn parse_task_handoff(payload: &str) -> Result<LiliaCodeTaskHandoff, String> {
    let handoff: LiliaCodeTaskHandoff =
        serde_json::from_str(payload).map_err(|error| format!("任务交接格式无效：{error}"))?;
    if handoff.protocol != TASK_HANDOFF_PROTOCOL || handoff.version != TASK_HANDOFF_VERSION {
        return Err(format!(
            "任务交接协议不兼容：{} v{}",
            handoff.protocol, handoff.version
        ));
    }
    if handoff.id.trim().is_empty()
        || handoff.title.trim().is_empty()
        || handoff.problem.trim().is_empty()
        || handoff.repository.worktree_path.trim().is_empty()
        || handoff.repository.full_name.trim().is_empty()
        || handoff.repository.branch.trim().is_empty()
        || handoff.source.application != "LiliaGithub"
        || handoff.source.route.trim().is_empty()
    {
        return Err("任务交接缺少 id、标题、问题、仓库、分支、worktree 或来源".to_string());
    }
    if !matches!(
        handoff.kind.as_str(),
        "issue" | "pullRequestReview" | "workflowFailure" | "syncConflict" | "repository"
    ) {
        return Err(format!("不支持的任务交接类型：{}", handoff.kind));
    }
    if handoff.kind == "pullRequestReview"
        && handoff
            .pull_request
            .as_ref()
            .is_none_or(|pull| pull.review_requirements.is_empty())
    {
        return Err("PR review 任务缺少审查要求".to_string());
    }
    if handoff.kind == "workflowFailure"
        && (handoff.workflow.is_none()
            || handoff
                .log_summary
                .as_deref()
                .is_none_or(|value| value.trim().is_empty()))
    {
        return Err("Workflow 修复任务缺少运行信息或失败日志摘要".to_string());
    }
    Ok(handoff)
}

fn task_handoff_prompt(handoff: &LiliaCodeTaskHandoff) -> String {
    let mut repository_context = format!(
        "仓库：{}\n工作区：{}\n分支：{}",
        handoff.repository.full_name, handoff.repository.worktree_path, handoff.repository.branch,
    );
    if let Some(remote_url) = handoff
        .repository
        .remote_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        repository_context.push_str(&format!("\n远端：{remote_url}"));
    }
    repository_context.push_str(&format!("\n来源：{}", handoff.source.route));
    if let Some(object_url) = handoff
        .source
        .object_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        repository_context.push_str(&format!("\n来源对象：{object_url}"));
    }

    let mut sections = vec![handoff.problem.trim().to_string(), repository_context];
    if let Some(pull) = &handoff.pull_request {
        let mut pull_context = format!(
            "Pull Request #{}：{} -> {}\n审查要求：{}",
            pull.number,
            pull.head_branch,
            pull.base_branch,
            pull.review_requirements.join("；")
        );
        let base_sha = pull
            .base_sha
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let head_sha = pull
            .head_sha
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match (base_sha, head_sha) {
            (Some(base), Some(head)) => {
                pull_context.push_str(&format!("\nDiff 范围：{base}...{head}"));
            }
            (Some(base), None) => pull_context.push_str(&format!("\nBase SHA：{base}")),
            (None, Some(head)) => pull_context.push_str(&format!("\nHead SHA：{head}")),
            (None, None) => {}
        }
        sections.push(pull_context);
    }
    if let Some(workflow) = &handoff.workflow {
        sections.push(format!(
            "Workflow：{}（run {}）\n{}",
            workflow.workflow_name, workflow.run_id, workflow.run_url
        ));
    }
    if !handoff.related_files.is_empty() {
        sections.push(format!(
            "相关文件：\n- {}",
            handoff.related_files.join("\n- ")
        ));
    }
    if let Some(log_summary) = handoff
        .log_summary
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        sections.push(format!("失败日志摘要：\n{}", log_summary.trim()));
    }
    if !handoff.acceptance_criteria.is_empty() {
        sections.push(format!(
            "验收条件：\n- {}",
            handoff.acceptance_criteria.join("\n- ")
        ));
    }
    sections.join("\n\n")
}

#[cfg(feature = "runtime-domain-reference")]
pub(crate) fn runtime_reference_prepare_handoff(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let encoded = serde_json::to_string(payload)
        .map_err(|error| format!("序列化 reference handoff 失败：{error}"))?;
    let handoff = parse_task_handoff(&encoded)?;
    Ok(serde_json::json!({
        "id": handoff.id,
        "kind": handoff.kind,
        "repository": handoff.repository.full_name,
        "worktreePath": handoff.repository.worktree_path,
        "prompt": task_handoff_prompt(&handoff),
    }))
}

fn task_handoff_receipt_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".receipt.json");
    PathBuf::from(value)
}

fn write_task_handoff_receipt(path: &Path, receipt: TaskHandoffReceipt) -> Result<(), String> {
    let value = serde_json::to_vec_pretty(&receipt)
        .map_err(|error| format!("序列化任务交接回执失败：{error}"))?;
    let receipt_path = task_handoff_receipt_path(path);
    let mut pending_value = receipt_path.as_os_str().to_os_string();
    pending_value.push(".pending");
    let pending_path = PathBuf::from(pending_value);
    fs::write(&pending_path, value).map_err(|error| format!("写入任务交接回执失败：{error}"))?;
    if receipt_path.is_file() {
        fs::remove_file(&receipt_path)
            .map_err(|error| format!("替换旧任务交接回执失败：{error}"))?;
    }
    fs::rename(&pending_path, &receipt_path)
        .map_err(|error| format!("发布任务交接回执失败：{error}"))
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_projects_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              cwd TEXT,
              created_at INTEGER NOT NULL,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE tasks (
              id TEXT PRIMARY KEY,
              project_id TEXT,
              session_id TEXT NOT NULL,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              archived INTEGER NOT NULL DEFAULT 0,
              parent_id TEXT,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0,
              title_source TEXT NOT NULL DEFAULT 'auto'
            );
            CREATE TABLE task_dependencies (
              task_id TEXT NOT NULL,
              depends_on_id TEXT NOT NULL,
              PRIMARY KEY (task_id, depends_on_id)
            );
            CREATE TABLE task_handoffs (
              handoff_id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL UNIQUE,
              payload_json TEXT NOT NULL,
              source_route TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            "#,
        )
        .unwrap();
    }

    fn task_handoff_payload(id: &str, source_route: &str) -> String {
        serde_json::json!({
            "protocol": TASK_HANDOFF_PROTOCOL,
            "version": TASK_HANDOFF_VERSION,
            "id": id,
            "createdAt": "2026-07-17T00:00:00Z",
            "title": "修复 workflow",
            "kind": "workflowFailure",
            "repository": {
                "fullName": "acme/widget",
                "worktreePath": "C:\\work\\widget",
                "branch": "fix/ci",
                "remoteUrl": "https://github.com/acme/widget.git"
            },
            "source": {
                "application": "LiliaGithub",
                "route": source_route,
                "objectUrl": "https://github.com/acme/widget/actions/runs/77"
            },
            "problem": "verify workflow failed",
            "relatedFiles": ["src/render.ts"],
            "logSummary": "typecheck failed",
            "acceptanceCriteria": ["typecheck passes"],
            "workflow": {
                "runId": 77,
                "runUrl": "https://github.com/acme/widget/actions/runs/77",
                "workflowName": "verify"
            }
        })
        .to_string()
    }

    #[test]
    fn task_handoff_import_is_atomic_and_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        setup_projects_schema(&conn);
        let payload_json = task_handoff_payload("handoff-atomic", "/discovery?workflow=77");
        let handoff = parse_task_handoff(&payload_json).unwrap();

        let first =
            import_task_handoff(&mut conn, &handoff, &payload_json, "C:\\work\\widget").unwrap();
        let second = import_task_handoff(&mut conn, &handoff, &payload_json, "D:\\other").unwrap();

        assert_eq!(second.task_id, first.task_id);
        assert_eq!(second.project_id, first.project_id);
        assert_eq!(second.cwd, first.cwd);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM task_handoffs", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );

        conn.execute_batch(
            "DELETE FROM task_handoffs; DELETE FROM tasks; DELETE FROM projects; \
             DROP TABLE task_handoffs; \
             CREATE TABLE task_handoffs (handoff_id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE, payload_json TEXT NOT NULL, source_route TEXT NOT NULL CHECK(source_route <> 'reject'), created_at INTEGER NOT NULL);",
        )
        .unwrap();
        let rejected_json = task_handoff_payload("handoff-rejected", "reject");
        let rejected = parse_task_handoff(&rejected_json).unwrap();
        assert!(
            import_task_handoff(&mut conn, &rejected, &rejected_json, "C:\\work\\widget").is_err()
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM projects", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
