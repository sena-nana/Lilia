use std::fs;
use std::path::{Path, PathBuf};

use lilia_contracts::{
    AgentSessionRef, ProjectId, ProjectionEventId, TaskId, TimelineProjectionCommand,
    TimelineProjectionEvent,
};
use lilia_storage::ProjectionApplyResult;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

const PROJECT_COMMAND_DIR: &str = ".lilia/commands";
const MAX_PROJECT_COMMAND_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSlashCommandSource {
    Native,
    Project,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DesktopSlashCommandAction {
    Execute,
    Review,
    FixSuggestion,
    TaskWorkflow { kind: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSlashCommand {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub source: DesktopSlashCommandSource,
    pub action: DesktopSlashCommandAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSlashCommandSearchResult {
    pub command: DesktopSlashCommand,
    pub matched_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSlashCommandExecution {
    pub command_id: String,
    pub name: String,
    pub source: DesktopSlashCommandSource,
    pub title: String,
    pub result: String,
}

#[derive(Clone, Debug)]
struct ProjectCommandDefinition {
    command: DesktopSlashCommand,
    body: String,
    path: PathBuf,
}

impl DesktopApplication {
    pub fn search_task_slash_commands(
        &self,
        task_id: &TaskId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DesktopSlashCommandSearchResult>, DesktopApplicationError> {
        let task = self.get_task(task_id)?;
        self.search_project_slash_commands(task.project_id.as_ref(), query, limit)
    }

    pub fn search_project_slash_commands(
        &self,
        project_id: Option<&ProjectId>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DesktopSlashCommandSearchResult>, DesktopApplicationError> {
        let workspace = project_id
            .map(|project_id| {
                self.project_context(project_id)
                    .map(|context| context.active_root().to_path_buf())
            })
            .transpose()?;
        Ok(search_commands(workspace.as_deref(), query, limit))
    }

    pub(crate) fn resolve_task_slash_command(
        &self,
        task_id: &TaskId,
        content: &str,
    ) -> Result<Option<DesktopSlashCommandExecution>, DesktopApplicationError> {
        let Some(name) = exact_command_name(content) else {
            return Ok(None);
        };
        let workspace = self.task_workspace_path(task_id)?.map(PathBuf::from);
        let Some(command) = search_commands(workspace.as_deref(), &name, usize::MAX)
            .into_iter()
            .map(|result| result.command)
            .find(|command| {
                command.name == name && command.action == DesktopSlashCommandAction::Execute
            })
        else {
            return Ok(None);
        };
        execute_command(&command, workspace.as_deref()).map(Some)
    }

    pub(crate) fn record_task_slash_command(
        &self,
        task_id: &TaskId,
        composer_revision: u64,
        execution: &DesktopSlashCommandExecution,
    ) -> Result<(), DesktopApplicationError> {
        let session = AgentSessionRef::new(format!("desktop-slash:{}", task_id.as_str()))?;
        let sequence = composer_revision;
        let event = TimelineProjectionEvent {
            id: ProjectionEventId::new(format!(
                "desktop-slash:{}:{composer_revision}",
                task_id.as_str()
            )),
            task_id: task_id.clone(),
            agent_session: session,
            sequence,
            turn_id: None,
            kind: "command".to_owned(),
            status: "success".to_owned(),
            title: format!("/{}", execution.name),
            summary: Some(execution.result.clone()),
            payload: json!({
                "command": format!("/{}", execution.name),
                "source": execution.source,
                "title": execution.title,
                "output": execution.result,
                "exitCode": 0,
                "subkind": "slash_command",
            }),
            projected: true,
        };
        let applied = self
            .authority()
            .apply_projection(TimelineProjectionCommand::UpsertTimelineEvent { event })?;
        if applied != ProjectionApplyResult::DuplicateIgnored {
            self.emit_event(DesktopEventKind::TimelineChanged {
                task_id: task_id.clone(),
                cursor: Some(sequence),
            });
        }
        Ok(())
    }
}

fn native_commands() -> Vec<DesktopSlashCommand> {
    let mut commands = vec![
        DesktopSlashCommand {
            id: "native:help".to_owned(),
            name: "help".to_owned(),
            title: "显示可用斜杠命令".to_owned(),
            description: "列出 Lilia 当前可执行的内置命令和项目命令。".to_owned(),
            source: DesktopSlashCommandSource::Native,
            action: DesktopSlashCommandAction::Execute,
        },
        DesktopSlashCommand {
            id: "native:status".to_owned(),
            name: "status".to_owned(),
            title: "显示当前会话状态".to_owned(),
            description: "写入 Native Agent 和工作目录状态。".to_owned(),
            source: DesktopSlashCommandSource::Native,
            action: DesktopSlashCommandAction::Execute,
        },
        workflow_command(
            "workflow:lilia_review",
            "review",
            "代码审查",
            "对指定代码范围做审查。",
            DesktopSlashCommandAction::Review,
        ),
        workflow_command(
            "workflow:lilia_fix_suggestion",
            "fix",
            "修复建议",
            "生成修复建议。",
            DesktopSlashCommandAction::FixSuggestion,
        ),
    ];
    commands.extend([
        task_workflow_command(
            "task",
            "通用实现任务",
            "按通用实现任务工作流发送。",
            "generalTask",
        ),
        task_workflow_command(
            "debug",
            "问题定位",
            "按问题定位工作流发送。",
            "bugLocalization",
        ),
        task_workflow_command(
            "frontend",
            "前端与交互",
            "按前端与交互工作流发送。",
            "frontend",
        ),
        task_workflow_command(
            "refactor",
            "重构与结构调整",
            "按重构与结构调整工作流发送。",
            "refactor",
        ),
        task_workflow_command(
            "verify",
            "测试与验证",
            "按测试与验证工作流发送。",
            "testAndVerification",
        ),
        task_workflow_command(
            "docs",
            "文档与提示词",
            "按文档与提示词工作流发送。",
            "docsAndPrompt",
        ),
        task_workflow_command(
            "git",
            "Git 与发布",
            "按 Git 与发布工作流发送。",
            "gitAndRelease",
        ),
        task_workflow_command(
            "architecture",
            "架构图与记忆",
            "按架构图与记忆工作流发送。",
            "architectureAndMemory",
        ),
    ]);
    commands
}

fn workflow_command(
    id: &str,
    name: &str,
    title: &str,
    description: &str,
    action: DesktopSlashCommandAction,
) -> DesktopSlashCommand {
    DesktopSlashCommand {
        id: id.to_owned(),
        name: name.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        source: DesktopSlashCommandSource::Native,
        action,
    }
}

fn task_workflow_command(
    name: &str,
    title: &str,
    description: &str,
    kind: &str,
) -> DesktopSlashCommand {
    workflow_command(
        &format!("workflow:lilia_task_workflow:{kind}"),
        name,
        title,
        description,
        DesktopSlashCommandAction::TaskWorkflow {
            kind: kind.to_owned(),
        },
    )
}

fn search_commands(
    workspace: Option<&Path>,
    query: &str,
    limit: usize,
) -> Vec<DesktopSlashCommandSearchResult> {
    let mut results = Vec::new();
    for command in native_commands() {
        if let Some(matched_by) = matches_command(&command, query) {
            results.push(DesktopSlashCommandSearchResult {
                command,
                matched_by: matched_by.to_owned(),
            });
        }
    }
    if let Some(workspace) = workspace {
        for definition in list_project_command_definitions(workspace) {
            if let Some(matched_by) = matches_command(&definition.command, query) {
                results.push(DesktopSlashCommandSearchResult {
                    command: definition.command,
                    matched_by: matched_by.to_owned(),
                });
            }
        }
    }
    results.truncate(limit.clamp(1, 50));
    results
}

fn execute_command(
    command: &DesktopSlashCommand,
    workspace: Option<&Path>,
) -> Result<DesktopSlashCommandExecution, DesktopApplicationError> {
    if command.action != DesktopSlashCommandAction::Execute {
        return Err(DesktopApplicationError::InvalidInput {
            field: "slash_command",
            message: format!(
                "workflow command `/{}` must be submitted as a workflow",
                command.name
            ),
        });
    }
    match command.id.as_str() {
        "native:help" => {
            let project_count = workspace
                .map(list_project_command_definitions)
                .map_or(0, |commands| commands.len());
            let project_path = workspace
                .map(project_command_dir)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "未绑定项目目录".to_owned());
            Ok(DesktopSlashCommandExecution {
                command_id: command.id.clone(),
                name: command.name.clone(),
                source: command.source,
                title: command.title.clone(),
                result: format!(
                    "内置命令：/help、/status\n项目命令：{project_count} 个\n项目命令文件目录：{project_path}"
                ),
            })
        }
        "native:status" => Ok(DesktopSlashCommandExecution {
            command_id: command.id.clone(),
            name: command.name.clone(),
            source: command.source,
            title: command.title.clone(),
            result: format!(
                "当前 Agent：Native AgentKit\n工作目录：{}",
                workspace
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "未绑定".to_owned())
            ),
        }),
        id if id.starts_with("project:") => {
            let workspace = workspace.ok_or_else(|| DesktopApplicationError::InvalidInput {
                field: "slash_command",
                message: "project slash command requires a task workspace".to_owned(),
            })?;
            let definition = list_project_command_definitions(workspace)
                .into_iter()
                .find(|definition| definition.command.id == command.id)
                .ok_or_else(|| DesktopApplicationError::InvalidInput {
                    field: "slash_command",
                    message: format!("project slash command `{}` is unavailable", command.name),
                })?;
            let result = if definition.body.trim().is_empty() {
                definition.command.title.clone()
            } else {
                definition.body.clone()
            };
            Ok(DesktopSlashCommandExecution {
                command_id: definition.command.id,
                name: definition.command.name,
                source: DesktopSlashCommandSource::Project,
                title: definition.command.title,
                result: format!("{}\n\n来源：{}", result, definition.path.display()),
            })
        }
        _ => Err(DesktopApplicationError::InvalidInput {
            field: "slash_command",
            message: format!("unsupported slash command `{}`", command.id),
        }),
    }
}

fn list_project_command_definitions(workspace: &Path) -> Vec<ProjectCommandDefinition> {
    let directory = project_command_dir(workspace);
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = command_name_from_path(&path) else {
            continue;
        };
        if entry
            .metadata()
            .map_or(true, |metadata| metadata.len() > MAX_PROJECT_COMMAND_BYTES)
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let (title, body) = markdown_title_and_body(&text, &name);
        let description = body
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_owned())
            })
            .unwrap_or_else(|| "项目自定义命令".to_owned());
        commands.push(ProjectCommandDefinition {
            command: DesktopSlashCommand {
                id: format!("project:{name}"),
                name,
                title,
                description,
                source: DesktopSlashCommandSource::Project,
                action: DesktopSlashCommandAction::Execute,
            },
            body,
            path,
        });
    }
    commands.sort_by(|left, right| left.command.name.cmp(&right.command.name));
    commands
}

fn exact_command_name(content: &str) -> Option<String> {
    let value = content.trim();
    let name = value.strip_prefix('/')?;
    normalize_command_name(name).filter(|normalized| *normalized == name.to_ascii_lowercase())
}

fn command_name_from_path(path: &Path) -> Option<String> {
    normalize_command_name(path.file_stem()?.to_string_lossy().as_ref())
}

fn normalize_command_name(value: &str) -> Option<String> {
    let name = value.trim().trim_start_matches('/').to_ascii_lowercase();
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return None;
    }
    Some(name)
}

fn project_command_dir(workspace: &Path) -> PathBuf {
    workspace.join(PROJECT_COMMAND_DIR)
}

fn markdown_title_and_body(text: &str, fallback_title: &str) -> (String, String) {
    let mut title = fallback_title.to_owned();
    let mut body = Vec::new();
    let mut title_seen = false;
    for line in text.lines() {
        if !title_seen {
            if let Some(heading) = line.trim().strip_prefix("# ") {
                let heading = heading.trim();
                if !heading.is_empty() {
                    title = heading.to_owned();
                    title_seen = true;
                    continue;
                }
            }
        }
        body.push(line);
    }
    (title, body.join("\n").trim().to_owned())
}

fn matches_command(command: &DesktopSlashCommand, query: &str) -> Option<&'static str> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() || command.name.contains(&query) {
        Some("name")
    } else if command.title.to_ascii_lowercase().contains(&query) {
        Some("title")
    } else if command.description.to_ascii_lowercase().contains(&query) {
        Some("description")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_command_requires_one_bare_slash_token() {
        assert_eq!(exact_command_name(" /HELP ").as_deref(), Some("help"));
        assert_eq!(exact_command_name("/status now"), None);
        assert_eq!(exact_command_name("message /status"), None);
        assert_eq!(exact_command_name("/../status"), None);
    }

    #[test]
    fn project_commands_are_bounded_sorted_and_keep_markdown_body() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().join(PROJECT_COMMAND_DIR);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("verify.md"),
            "# 验证项目\n\n运行完整验证并报告失败。",
        )
        .unwrap();
        fs::write(directory.join("bad name.md"), "ignored").unwrap();
        fs::write(
            directory.join("oversized.md"),
            vec![b'x'; (MAX_PROJECT_COMMAND_BYTES + 1) as usize],
        )
        .unwrap();

        let commands = list_project_command_definitions(workspace.path());

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command.name, "verify");
        assert_eq!(commands[0].command.title, "验证项目");
        assert_eq!(commands[0].body, "运行完整验证并报告失败。");
    }

    #[test]
    fn search_matches_name_title_and_description_with_a_hard_limit() {
        let results = search_commands(None, "状态", 50);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command.id, "native:status");
        assert_eq!(results[0].matched_by, "title");
        assert_eq!(search_commands(None, "", 1).len(), 1);
    }

    #[test]
    fn workflow_commands_expose_typed_review_and_task_actions() {
        let review = search_commands(None, "review", 12).remove(0).command;
        assert_eq!(review.action, DesktopSlashCommandAction::Review);

        let task = search_commands(None, "frontend", 12).remove(0).command;
        assert_eq!(
            task.action,
            DesktopSlashCommandAction::TaskWorkflow {
                kind: "frontend".to_owned(),
            }
        );
    }
}
