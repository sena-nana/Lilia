use std::fs;
use std::path::{Path, PathBuf};

use lilia_contracts::{
    GitWorkspaceRef, LiliaCodeTaskHandoff, LiliaCodeTaskHandoffKind, ProductTask,
    ProductTaskHandoffImport, ProductTaskHandoffRecord, Project, ProjectArchiveState, ProjectId,
    TaskId, LILIA_CODE_TASK_HANDOFF_PROTOCOL, LILIA_CODE_TASK_HANDOFF_VERSION,
};
use serde::Serialize;
use uuid::Uuid;

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopNavigationTarget,
    DesktopProjectPatch, ProjectQuery, TaskQuery,
};
use crate::application::{ProjectsChanged, TasksChanged, NavigationRequested};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImportedTaskHandoff {
    pub task_id: String,
    pub handoff: LiliaCodeTaskHandoff,
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopTaskHandoffOpen {
    pub project_id: ProjectId,
    pub cwd: String,
    pub task_id: TaskId,
    pub handoff_id: String,
    pub prompt: String,
    pub duplicate: bool,
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

impl DesktopApplication {
    pub fn accept_task_handoff_file(
        &self,
        handoff_path: &Path,
        working_directory: Option<&Path>,
    ) -> Result<DesktopTaskHandoffOpen, DesktopApplicationError> {
        let handoff_path = resolve_file(handoff_path, working_directory)?;
        let payload_json = fs::read_to_string(&handoff_path).map_err(|error| {
            DesktopApplicationError::TaskHandoff(format!(
                "failed to read task handoff {}: {error}",
                handoff_path.display()
            ))
        })?;
        let opened = self.accept_task_handoff_payload(
            &payload_json,
            working_directory.unwrap_or_else(|| Path::new(".")),
        )?;
        write_task_handoff_receipt(
            &handoff_path,
            TaskHandoffReceipt {
                protocol: LILIA_CODE_TASK_HANDOFF_PROTOCOL,
                version: LILIA_CODE_TASK_HANDOFF_VERSION,
                handoff_id: opened.handoff_id.clone(),
                status: "accepted",
                task_id: opened.task_id.as_str().to_owned(),
                project_id: opened.project_id.as_str().to_owned(),
                result_route: format!(
                    "/projects/{}/tasks/{}",
                    opened.project_id.as_str(),
                    opened.task_id.as_str()
                ),
                updated_at: now_millis().to_string(),
            },
        )?;
        Ok(opened)
    }

    pub fn accept_task_handoff_payload(
        &self,
        payload_json: &str,
        working_directory: &Path,
    ) -> Result<DesktopTaskHandoffOpen, DesktopApplicationError> {
        let handoff = parse_task_handoff(payload_json)?;
        let workspace = resolve_directory(&handoff.repository.worktree_path, working_directory)?;
        let workspace = display_path(&workspace);
        let existing_project = self
            .query_projects(ProjectQuery {
                include_archived: true,
            })?
            .into_iter()
            .find(|project| {
                project
                    .workspace_path
                    .as_deref()
                    .is_some_and(|candidate| paths_equal(candidate, &workspace))
            });
        let existing_project = match existing_project {
            Some(project) if project.archive == ProjectArchiveState::Archived => {
                Some(self.update_project(
                    &project.id,
                    DesktopProjectPatch {
                        archived: Some(false),
                        ..DesktopProjectPatch::default()
                    },
                )?)
            }
            project => project,
        };
        let project_was_new = existing_project.is_none();
        let project = match existing_project {
            Some(project) => project,
            None => build_handoff_project(self, &handoff, &workspace)?,
        };
        let task = build_handoff_task(self, &handoff, &project)?;
        let record = self.authority().client()?.products().accept_task_handoff(
            ProductTaskHandoffImport {
                handoff: handoff.clone(),
                payload_json: payload_json.to_owned(),
                project: project.clone(),
                task,
                accepted_at: now_millis(),
            },
        )?;
        self.ensure_task_conversation(&record.task, &record.task.title)?;
        if !record.duplicate {
            if project_was_new {
                self.emit_event(ProjectsChanged);
            }
            self.emit_event(TasksChanged {
                project_id: Some(record.project.id.clone()),
                task_id: Some(record.task.id.clone()),
            });
        }
        self.emit_event(NavigationRequested {
            target: DesktopNavigationTarget::Task(record.task.id.clone()),
        });
        Ok(open_record(record))
    }

    pub fn imported_task_handoff(
        &self,
        task_id: &TaskId,
    ) -> Result<Option<DesktopImportedTaskHandoff>, DesktopApplicationError> {
        self.authority()
            .client()?
            .products()
            .task_handoff_for_task(task_id)
            .map(|record| {
                record.map(|record| DesktopImportedTaskHandoff {
                    task_id: task_id.as_str().to_owned(),
                    prompt: task_handoff_prompt(&record.handoff),
                    handoff: record.handoff,
                })
            })
            .map_err(DesktopApplicationError::from)
    }
}

pub fn prepare_task_handoff_reference(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, DesktopApplicationError> {
    let payload_json = serde_json::to_string(payload).map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!("failed to encode task handoff: {error}"))
    })?;
    let handoff = parse_task_handoff(&payload_json)?;
    Ok(serde_json::json!({
        "id": handoff.id,
        "kind": handoff.kind,
        "repository": handoff.repository.full_name,
        "worktreePath": handoff.repository.worktree_path,
        "prompt": task_handoff_prompt(&handoff),
    }))
}

pub fn describe_task_handoff(
    task_id: &str,
    payload_json: &str,
) -> Result<DesktopImportedTaskHandoff, DesktopApplicationError> {
    let handoff = parse_task_handoff(payload_json)?;
    Ok(DesktopImportedTaskHandoff {
        task_id: task_id.to_owned(),
        prompt: task_handoff_prompt(&handoff),
        handoff,
    })
}

fn build_handoff_project(
    application: &DesktopApplication,
    handoff: &LiliaCodeTaskHandoff,
    workspace: &str,
) -> Result<Project, DesktopApplicationError> {
    let name = handoff
        .repository
        .full_name
        .rsplit('/')
        .find(|value| !value.trim().is_empty())
        .unwrap_or("Project");
    let mut project = Project::new(ProjectId::new(format!("project-{}", Uuid::new_v4()))?, name)?;
    project.workspace_path = Some(workspace.to_owned());
    project.git_workspace = Some(GitWorkspaceRef {
        repository: Some(handoff.repository.full_name.clone()),
        branch: Some(handoff.repository.branch.clone()),
        worktree_path: Some(workspace.to_owned()),
    });
    project.sort_order = application
        .query_projects(ProjectQuery {
            include_archived: true,
        })?
        .into_iter()
        .map(|project| project.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1);
    Ok(project)
}

fn build_handoff_task(
    application: &DesktopApplication,
    handoff: &LiliaCodeTaskHandoff,
    project: &Project,
) -> Result<ProductTask, DesktopApplicationError> {
    let mut task = ProductTask::new(
        TaskId::new(format!("task-{}", Uuid::new_v4()))?,
        Some(project.id.clone()),
        handoff.title.trim(),
    )?;
    task.description = Some(handoff.problem.trim().to_owned());
    task.completion_criteria = handoff.acceptance_criteria.clone();
    task.tags = vec![
        "task-handoff".to_owned(),
        handoff_kind_key(handoff.kind).to_owned(),
    ];
    task.legacy_source = Some("lilia-code-task-handoff".to_owned());
    task.sort_order = application
        .query_tasks(TaskQuery::for_project(project.id.clone()).including_archived())?
        .into_iter()
        .map(|task| task.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1);
    task.created_at = now_millis();
    task.updated_at = task.created_at;
    Ok(task)
}

fn open_record(record: ProductTaskHandoffRecord) -> DesktopTaskHandoffOpen {
    DesktopTaskHandoffOpen {
        project_id: record.project.id,
        cwd: record.project.workspace_path.unwrap_or_default(),
        task_id: record.task.id,
        handoff_id: record.handoff.id.clone(),
        prompt: task_handoff_prompt(&record.handoff),
        duplicate: record.duplicate,
    }
}

fn parse_task_handoff(payload: &str) -> Result<LiliaCodeTaskHandoff, DesktopApplicationError> {
    let handoff: LiliaCodeTaskHandoff = serde_json::from_str(payload).map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!("invalid task handoff: {error}"))
    })?;
    if handoff.protocol != LILIA_CODE_TASK_HANDOFF_PROTOCOL
        || handoff.version != LILIA_CODE_TASK_HANDOFF_VERSION
    {
        return Err(DesktopApplicationError::TaskHandoff(format!(
            "incompatible task handoff protocol: {} v{}",
            handoff.protocol, handoff.version
        )));
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
        return Err(DesktopApplicationError::TaskHandoff(
            "task handoff is missing identity, title, problem, repository, branch, worktree, or source"
                .to_owned(),
        ));
    }
    if handoff.kind == LiliaCodeTaskHandoffKind::PullRequestReview
        && handoff
            .pull_request
            .as_ref()
            .is_none_or(|pull| pull.review_requirements.is_empty())
    {
        return Err(DesktopApplicationError::TaskHandoff(
            "pull request review handoff has no review requirements".to_owned(),
        ));
    }
    if handoff.kind == LiliaCodeTaskHandoffKind::WorkflowFailure
        && (handoff.workflow.is_none()
            || handoff
                .log_summary
                .as_deref()
                .is_none_or(|value| value.trim().is_empty()))
    {
        return Err(DesktopApplicationError::TaskHandoff(
            "workflow handoff has no run context or log summary".to_owned(),
        ));
    }
    Ok(handoff)
}

fn task_handoff_prompt(handoff: &LiliaCodeTaskHandoff) -> String {
    let mut repository_context = format!(
        "仓库：{}\n工作区：{}\n分支：{}",
        handoff.repository.full_name, handoff.repository.worktree_path, handoff.repository.branch,
    );
    if let Some(remote_url) = optional_text(handoff.repository.remote_url.as_deref()) {
        repository_context.push_str(&format!("\n远端：{remote_url}"));
    }
    repository_context.push_str(&format!("\n来源：{}", handoff.source.route));
    if let Some(object_url) = optional_text(handoff.source.object_url.as_deref()) {
        repository_context.push_str(&format!("\n来源对象：{object_url}"));
    }

    let mut sections = vec![handoff.problem.trim().to_owned(), repository_context];
    if let Some(pull) = &handoff.pull_request {
        let mut pull_context = format!(
            "Pull Request #{}：{} -> {}\n审查要求：{}",
            pull.number,
            pull.head_branch,
            pull.base_branch,
            pull.review_requirements.join("；")
        );
        match (
            optional_text(pull.base_sha.as_deref()),
            optional_text(pull.head_sha.as_deref()),
        ) {
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
    if let Some(log_summary) = optional_text(handoff.log_summary.as_deref()) {
        sections.push(format!("失败日志摘要：\n{log_summary}"));
    }
    if !handoff.acceptance_criteria.is_empty() {
        sections.push(format!(
            "验收条件：\n- {}",
            handoff.acceptance_criteria.join("\n- ")
        ));
    }
    sections.join("\n\n")
}

fn handoff_kind_key(kind: LiliaCodeTaskHandoffKind) -> &'static str {
    match kind {
        LiliaCodeTaskHandoffKind::Issue => "issue",
        LiliaCodeTaskHandoffKind::PullRequestReview => "pull-request-review",
        LiliaCodeTaskHandoffKind::WorkflowFailure => "workflow-failure",
        LiliaCodeTaskHandoffKind::SyncConflict => "sync-conflict",
        LiliaCodeTaskHandoffKind::Repository => "repository",
    }
}

fn optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn resolve_file(
    path: &Path,
    working_directory: Option<&Path>,
) -> Result<PathBuf, DesktopApplicationError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    };
    if !path.is_file() {
        return Err(DesktopApplicationError::TaskHandoff(format!(
            "task handoff file does not exist: {}",
            path.display()
        )));
    }
    path.canonicalize().map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!(
            "failed to resolve task handoff {}: {error}",
            path.display()
        ))
    })
}

fn resolve_directory(
    value: &str,
    working_directory: &Path,
) -> Result<PathBuf, DesktopApplicationError> {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        working_directory.join(path)
    };
    if !path.is_dir() {
        return Err(DesktopApplicationError::TaskHandoff(format!(
            "task handoff worktree is not a directory: {}",
            path.display()
        )));
    }
    path.canonicalize().map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!(
            "failed to resolve task handoff worktree {}: {error}",
            path.display()
        ))
    })
}

fn task_handoff_receipt_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".receipt.json");
    PathBuf::from(value)
}

fn write_task_handoff_receipt(
    path: &Path,
    receipt: TaskHandoffReceipt,
) -> Result<(), DesktopApplicationError> {
    let content = serde_json::to_vec_pretty(&receipt).map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!(
            "failed to serialize handoff receipt: {error}"
        ))
    })?;
    let path = task_handoff_receipt_path(path);
    let mut staging = path.as_os_str().to_os_string();
    staging.push(".pending");
    let staging = PathBuf::from(staging);
    fs::write(&staging, content).map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!("failed to stage handoff receipt: {error}"))
    })?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| {
            DesktopApplicationError::TaskHandoff(format!(
                "failed to replace previous handoff receipt: {error}"
            ))
        })?;
    }
    fs::rename(&staging, &path).map_err(|error| {
        DesktopApplicationError::TaskHandoff(format!("failed to publish handoff receipt: {error}"))
    })
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return rest.to_owned();
        }
    }
    value.into_owned()
}

fn paths_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;
    use tempfile::tempdir;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, ProjectsChanged, TasksChanged, NavigationRequested};

    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let id = Uuid::new_v4();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-handoff:{id}"),
            format!("desktop-handoff-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/handoff", "liliacode.handoff").unwrap(),
            authority,
            Arc::new(TestHost),
        )
        .unwrap()
    }

    fn payload(id: &str, workspace: &Path) -> String {
        serde_json::json!({
            "protocol": LILIA_CODE_TASK_HANDOFF_PROTOCOL,
            "version": LILIA_CODE_TASK_HANDOFF_VERSION,
            "id": id,
            "createdAt": "2026-08-10T00:00:00Z",
            "title": "修复 workflow",
            "kind": "workflowFailure",
            "repository": {
                "fullName": "acme/widget",
                "worktreePath": workspace,
                "branch": "fix/ci",
                "remoteUrl": "https://github.com/acme/widget.git"
            },
            "source": {
                "application": "LiliaGithub",
                "route": "/discovery?workflow=77",
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
    fn task_handoff_is_idempotent_and_available_to_both_hosts() {
        let app = application();
        let workspace = tempdir().unwrap();
        let payload = payload("handoff-shared", workspace.path());

        let first = app
            .accept_task_handoff_payload(&payload, workspace.path())
            .unwrap();
        let second = app
            .accept_task_handoff_payload(&payload, workspace.path())
            .unwrap();

        assert_eq!(second.task_id, first.task_id);
        assert_eq!(second.project_id, first.project_id);
        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(
            app.query_projects(ProjectQuery::default()).unwrap().len(),
            1
        );
        assert_eq!(app.query_tasks(TaskQuery::default()).unwrap().len(), 1);
        let imported = app.imported_task_handoff(&first.task_id).unwrap().unwrap();
        assert_eq!(imported.handoff.id, "handoff-shared");
        assert!(imported.prompt.contains("typecheck failed"));
    }

    #[test]
    fn task_handoff_file_writes_versioned_receipt() {
        let app = application();
        let workspace = tempdir().unwrap();
        let handoff_path = workspace.path().join("handoff.json");
        fs::write(&handoff_path, payload("handoff-receipt", workspace.path())).unwrap();

        let opened = app
            .accept_task_handoff_file(&handoff_path, Some(workspace.path()))
            .unwrap();
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(task_handoff_receipt_path(&handoff_path)).unwrap())
                .unwrap();
        assert_eq!(receipt["status"], "accepted");
        assert_eq!(receipt["taskId"], opened.task_id.as_str());
    }
}
