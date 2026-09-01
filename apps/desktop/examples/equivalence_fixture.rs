use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lilia_contracts::{
    AgentSessionRef, ProductTaskPriority, ProductTaskStatus, ProjectId, ProjectionEventId, TaskId,
    TimelineProjectionCommand, TimelineProjectionEvent,
};
use lilia_storage::{LiliaDataPaths, LiliaPluginContributions, LiliaPluginManifest};
use liliacode_host::application::{
    DesktopApplication, DesktopApplicationConfig, DesktopHookDocumentUpdate,
    DesktopHookHandlerUpdate, DesktopHookScope, DesktopHost, DesktopHostAction, DesktopHostContext,
    DesktopHostError, DesktopHostResult, DesktopMcpServerUpsert, DesktopMcpTransport,
    DesktopOptionalTextUpdate, DesktopPluginInstall, DesktopProjectCreate, DesktopProjectPatch,
    DesktopSkillCreate, DesktopSkillScope, DesktopTaskCreate, DesktopTaskMove, DesktopTaskPatch,
    DesktopTodoCreate, DesktopTodoPriority,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_version: u32,
    fixture_id: String,
    projects: Vec<ProjectFixture>,
    tasks: Vec<TaskFixture>,
    timeline: Vec<TimelineFixture>,
    #[serde(default)]
    timeline_series: Vec<TimelineSeriesFixture>,
    #[serde(default)]
    goals: Vec<GoalFixture>,
    #[serde(default)]
    todos: Vec<TodoFixture>,
    #[serde(default)]
    skills: Vec<SkillFixture>,
    #[serde(default)]
    plugins: Vec<PluginFixture>,
    #[serde(default)]
    hooks: Vec<HookFixture>,
    #[serde(default)]
    mcp_servers: Vec<McpServerFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFixture {
    id: String,
    name: String,
    pinned: bool,
    sort_order: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskFixture {
    id: String,
    project_id: Option<String>,
    parent_id: Option<String>,
    title: String,
    description: Option<String>,
    status: ProductTaskStatus,
    priority: ProductTaskPriority,
    pinned: bool,
    sort_order: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineFixture {
    id: String,
    task_id: String,
    agent_session: String,
    sequence: u64,
    turn_id: Option<String>,
    kind: String,
    status: String,
    title: String,
    summary: Option<String>,
    payload: JsonValue,
    projected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineSeriesFixture {
    id_prefix: String,
    task_id: String,
    agent_session: String,
    start_sequence: u64,
    count: usize,
    turn_id_prefix: String,
    kind: String,
    status: String,
    title_prefix: String,
    summary_prefix: String,
    payload_content_prefix: String,
    projected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoalFixture {
    task_id: String,
    objective: String,
    token_budget: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TodoFixture {
    task_id: String,
    text: String,
    priority: DesktopTodoPriority,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpServerFixture {
    server_id: String,
    transport: DesktopMcpTransport,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
    #[serde(default)]
    env_secret_names: Vec<String>,
    #[serde(default)]
    header_secret_names: Vec<String>,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillFixture {
    skill_id: String,
    description: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginFixture {
    plugin_id: String,
    name: String,
    version: String,
    description: String,
    enabled: bool,
    skill_id: String,
    skill_description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookFixture {
    enabled: bool,
    handlers: Vec<HookHandlerFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookHandlerFixture {
    id: String,
    event: String,
    matcher: Option<String>,
    command: Option<String>,
    command_windows: Option<String>,
    timeout_seconds: Option<u64>,
    status_message: Option<String>,
}

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse()?;
    let manifest_bytes = fs::read(&arguments.manifest)?;
    let fixture: Fixture = serde_json::from_slice(&manifest_bytes)?;
    if !matches!(fixture.schema_version, 1..=7) {
        return Err(format!(
            "unsupported equivalence fixture schema {}",
            fixture.schema_version
        )
        .into());
    }
    ensure_empty_target(&arguments.home)?;
    fs::create_dir_all(&arguments.home)?;

    let application = DesktopApplication::bootstrap(
        DesktopApplicationConfig::new(&arguments.home, &arguments.identity)?,
        Arc::new(NoopHost),
    )?;
    seed(&application, &fixture)?;
    let snapshot = application.debug_equivalence_snapshot(&fixture.fixture_id)?;
    let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "fixtureId": fixture.fixture_id,
            "manifestSha256": manifest_sha256,
            "home": arguments.home,
            "snapshot": snapshot,
        }))?
    );
    Ok(())
}

fn seed(
    application: &DesktopApplication,
    fixture: &Fixture,
) -> Result<(), Box<dyn std::error::Error>> {
    for project in &fixture.projects {
        let project_id = ProjectId::new(&project.id)?;
        application.create_project(DesktopProjectCreate {
            id: project_id.clone(),
            name: project.name.clone(),
            workspace_path: None,
        })?;
        application.update_project(
            &project_id,
            DesktopProjectPatch {
                pinned: Some(project.pinned),
                sort_order: Some(project.sort_order),
                ..DesktopProjectPatch::default()
            },
        )?;
    }

    for task in &fixture.tasks {
        let task_id = TaskId::new(&task.id)?;
        let project_id = task.project_id.as_deref().map(ProjectId::new).transpose()?;
        application.create_task(DesktopTaskCreate {
            id: task_id.clone(),
            project_id,
            parent_id: None,
            title: task.title.clone(),
        })?;
        application.update_task(
            &task_id,
            DesktopTaskPatch {
                description: task.description.clone().map_or(
                    DesktopOptionalTextUpdate::Clear,
                    DesktopOptionalTextUpdate::Set,
                ),
                status: Some(task.status),
                priority: Some(task.priority),
                pinned: Some(task.pinned),
                sort_order: Some(task.sort_order),
                ..DesktopTaskPatch::default()
            },
        )?;
    }

    for task in fixture.tasks.iter().filter(|task| task.parent_id.is_some()) {
        application.move_task(
            &TaskId::new(&task.id)?,
            DesktopTaskMove {
                target_project_id: task.project_id.as_deref().map(ProjectId::new).transpose()?,
                target_parent_id: task.parent_id.as_deref().map(TaskId::new).transpose()?,
            },
        )?;
    }

    for skill in &fixture.skills {
        let revision = application.extensions_snapshot()?.skills_registry_revision;
        let created = application.create_skill_package(DesktopSkillCreate {
            expected_registry_revision: revision,
            scope: DesktopSkillScope::User,
            project_cwd: None,
            skill_id: skill.skill_id.clone(),
            description: skill.description.clone(),
        })?;
        if !skill.enabled {
            application.set_skill_package_enabled(
                &skill.skill_id,
                false,
                created.skills_registry_revision,
            )?;
        }
    }

    for plugin in &fixture.plugins {
        let source = application
            .config()
            .home()
            .join("equivalence-plugin-sources")
            .join(&plugin.plugin_id);
        let skill_root = source.join("skills").join(&plugin.skill_id);
        fs::create_dir_all(&skill_root)?;
        fs::write(
            skill_root.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: {}\n---\n{}\n",
                plugin.skill_id, plugin.skill_description, plugin.skill_description
            ),
        )?;
        fs::write(
            lilia_storage::plugin_manifest_path(&source),
            serde_json::to_vec_pretty(&LiliaPluginManifest {
                schema_version: 1,
                plugin_id: plugin.plugin_id.clone(),
                name: plugin.name.clone(),
                plugin_version: plugin.version.clone(),
                description: plugin.description.clone(),
                contributions: LiliaPluginContributions {
                    skills: vec![format!("skills/{}", plugin.skill_id)],
                    ..Default::default()
                },
            })?,
        )?;
        let revision = application.extensions_snapshot()?.plugins_registry_revision;
        let installed = application.install_plugin_package(DesktopPluginInstall {
            expected_registry_revision: revision,
            source_path: source.to_string_lossy().into_owned(),
        })?;
        if plugin.enabled {
            let revision = application.extensions_snapshot()?.plugins_registry_revision;
            application.set_plugin_package_enabled(&installed.plugin_id, true, revision)?;
        }
    }

    for hooks in &fixture.hooks {
        let created = application.create_hook_source(DesktopHookScope::User, None)?;
        let updated = application.update_hook_source(
            DesktopHookScope::User,
            None,
            DesktopHookDocumentUpdate {
                expected_revision: created.revision,
                handlers: hooks
                    .handlers
                    .iter()
                    .map(|handler| DesktopHookHandlerUpdate {
                        id: Some(handler.id.clone()),
                        event: handler.event.clone(),
                        matcher: handler.matcher.clone(),
                        handler_type: "command".to_owned(),
                        command: handler.command.clone(),
                        command_windows: handler.command_windows.clone(),
                        timeout_seconds: handler.timeout_seconds,
                        status_message: handler.status_message.clone(),
                    })
                    .collect(),
            },
        )?;
        if hooks.enabled {
            application.set_hook_source_enabled(
                DesktopHookScope::User,
                None,
                updated.source.revision,
                true,
            )?;
        }
    }

    for server in &fixture.mcp_servers {
        let revision = application.extensions_snapshot()?.mcp_registry_revision;
        application.upsert_mcp_server(DesktopMcpServerUpsert {
            expected_registry_revision: revision,
            server_id: server.server_id.clone(),
            transport: server.transport,
            command: server.command.clone(),
            args: server.args.clone(),
            url: server.url.clone(),
            env_secret_names: server.env_secret_names.clone(),
            header_secret_names: server.header_secret_names.clone(),
            enabled: server.enabled,
        })?;
    }

    for event in &fixture.timeline {
        application.authority().apply_projection(
            TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::new(&event.id),
                    task_id: TaskId::new(&event.task_id)?,
                    agent_session: AgentSessionRef::new(&event.agent_session)?,
                    sequence: event.sequence,
                    turn_id: event.turn_id.clone(),
                    kind: event.kind.clone(),
                    status: event.status.clone(),
                    title: event.title.clone(),
                    summary: event.summary.clone(),
                    payload: event.payload.clone(),
                    projected: event.projected,
                },
            },
        )?;
    }
    for series in &fixture.timeline_series {
        if !(1..=10_000).contains(&series.count) {
            return Err(format!(
                "timeline series `{}` count must be between 1 and 10000",
                series.id_prefix
            )
            .into());
        }
        let task_id = TaskId::new(&series.task_id)?;
        let agent_session = AgentSessionRef::new(&series.agent_session)?;
        for index in 0..series.count {
            let ordinal = index + 1;
            let sequence = series
                .start_sequence
                .checked_add(index as u64)
                .ok_or("timeline series sequence overflow")?;
            application.authority().apply_projection(
                TimelineProjectionCommand::UpsertTimelineEvent {
                    event: TimelineProjectionEvent {
                        id: ProjectionEventId::new(format!("{}-{ordinal:04}", series.id_prefix)),
                        task_id: task_id.clone(),
                        agent_session: agent_session.clone(),
                        sequence,
                        turn_id: Some(format!("{}-{ordinal:04}", series.turn_id_prefix)),
                        kind: series.kind.clone(),
                        status: series.status.clone(),
                        title: format!("{} {ordinal}", series.title_prefix),
                        summary: Some(format!("{} {ordinal}", series.summary_prefix)),
                        payload: serde_json::json!({
                            "content": format!("{} {ordinal}", series.payload_content_prefix),
                        }),
                        projected: series.projected,
                    },
                },
            )?;
        }
    }
    for goal in &fixture.goals {
        application.set_task_goal(
            &TaskId::new(&goal.task_id)?,
            goal.objective.clone(),
            goal.token_budget,
        )?;
    }
    for todo in &fixture.todos {
        application.create_task_todo(DesktopTodoCreate {
            task_id: TaskId::new(&todo.task_id)?,
            text: todo.text.clone(),
            priority: todo.priority,
            attachments: Vec::new(),
            conversation_references: Vec::new(),
            workflow: None,
        })?;
    }
    Ok(())
}

fn ensure_empty_target(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let paths = LiliaDataPaths::from_home(home.to_path_buf());
    for path in [
        paths.product_db(),
        paths.agent_runtime_db(),
        paths.legacy_desktop_db(),
    ] {
        if path.exists() {
            return Err(format!(
                "equivalence fixture target already contains `{}`",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

struct Arguments {
    manifest: PathBuf,
    home: PathBuf,
    identity: String,
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut arguments = std::env::args().skip(1);
        let mut manifest = None;
        let mut home = None;
        let mut identity = None;
        while let Some(argument) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for `{argument}`"))?;
            match argument.as_str() {
                "--manifest" => manifest = Some(PathBuf::from(value)),
                "--home" => home = Some(PathBuf::from(value)),
                "--identity" => identity = Some(value),
                _ => return Err(format!("unsupported argument `{argument}`").into()),
            }
        }
        Ok(Self {
            manifest: manifest.ok_or("--manifest is required")?,
            home: home.ok_or("--home is required")?,
            identity: identity.ok_or("--identity is required")?,
        })
    }
}
