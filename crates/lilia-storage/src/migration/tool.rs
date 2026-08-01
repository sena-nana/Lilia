//! Runnable migration tool: inspect / dry-run / apply / status / report / rollback.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lilia_contracts::{
    AgentSessionBinding, AgentSessionRef, BindingId, ConversationId, ProductConversation,
    ProductEntity, ProductError, ProductResult, ProductRevision, ProductTask, ProductTaskStatus,
    Project, ProjectArchiveState, ProjectId, ProjectionEventId, TaskId, TimelineProjectionCommand,
    TimelineProjectionEvent,
};
use lilia_core::ProductRepository;
use rusqlite::{params, Connection};

use crate::migration::compat_apply::apply_compat_assets_to_agentkit_registry;
use crate::migration::compat_preview::preview_compat_assets;
use crate::migration::report::{
    CompatAssetPreview, LegacySessionPlan, MigrationMode, MigrationObjectResult, MigrationReport,
    ObjectKind,
};
use crate::product::SqliteProductStore;
use crate::sqlite::SqliteTimelineProjectionStore;
use crate::timeline::TimelineProjectionRepository;
use crate::LiliaDataPaths;

/// Product version until which Legacy Claude/Codex continue remains available.
pub const LEGACY_SESSION_COMPAT_UNTIL: &str = "1.0.0";
/// Durable marker for the one-time Desktop product-authority cutover.
pub const DESKTOP_PRODUCT_CORE_CUTOVER: &str = "desktop-product-core-cutover-v1";

#[derive(Clone, Debug)]
struct LegacyProjectRow {
    id: String,
    name: String,
    cwd: Option<String>,
    sort_order: i64,
    pinned: bool,
}

#[derive(Clone, Debug)]
struct LegacyTaskRow {
    id: String,
    project_id: Option<String>,
    title: String,
    status: String,
    parent_id: Option<String>,
    sort_order: i64,
    pinned: bool,
    archived: bool,
    created_at: i64,
}

#[derive(Clone, Debug)]
struct LegacySessionRow {
    task_id: String,
    backend: String,
    session_id: String,
}

#[derive(Clone, Debug)]
struct LegacyDependencyRow {
    task_id: String,
    depends_on_id: String,
}

#[derive(Clone, Debug)]
struct LegacyTimelineRow {
    id: String,
    task_id: String,
    turn_id: Option<String>,
    backend: String,
    kind: String,
    status: String,
    title: String,
    summary: Option<String>,
    payload: String,
    turn_seq: i64,
    intra_turn_order: i64,
}

#[derive(Clone, Debug)]
struct LegacySnapshot {
    projects: Vec<LegacyProjectRow>,
    tasks: Vec<LegacyTaskRow>,
    dependencies: Vec<LegacyDependencyRow>,
    sessions: Vec<LegacySessionRow>,
    timeline: Vec<LegacyTimelineRow>,
}

/// Inspect a legacy Desktop `lilia.db` without writing product storage.
fn inspect_legacy_db(path: impl AsRef<Path>) -> ProductResult<LegacySnapshot> {
    let path = path.as_ref();
    if !path.is_file() {
        return Err(ProductError::NotFound {
            entity: "legacy_db".into(),
            id: path.display().to_string(),
        });
    }
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|err| ProductError::Unavailable {
        message: format!("open legacy db: {err}"),
    })?;

    let projects = query_projects(&conn)?;
    let tasks = query_tasks(&conn)?;
    let dependencies = query_dependencies(&conn)?;
    let sessions = query_sessions(&conn)?;
    let timeline = query_timeline(&conn)?;
    Ok(LegacySnapshot {
        projects,
        tasks,
        dependencies,
        sessions,
        timeline,
    })
}

fn query_projects(conn: &Connection) -> ProductResult<Vec<LegacyProjectRow>> {
    if !table_exists(conn, "projects")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT id, name, cwd, sort_order, pinned FROM projects")
        .map_err(map_sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                sort_order: row.get(3)?,
                pinned: row.get::<_, i64>(4).unwrap_or(0) != 0,
            })
        })
        .map_err(map_sql)?;
    collect_rows(rows)
}

fn query_tasks(conn: &Connection) -> ProductResult<Vec<LegacyTaskRow>> {
    if !table_exists(conn, "tasks")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, status, parent_id, sort_order, pinned, archived, created_at FROM tasks",
        )
        .map_err(map_sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyTaskRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                parent_id: row.get(4)?,
                sort_order: row.get(5)?,
                pinned: row.get::<_, i64>(6).unwrap_or(0) != 0,
                archived: row.get::<_, i64>(7).unwrap_or(0) != 0,
                created_at: row.get(8).unwrap_or(0),
            })
        })
        .map_err(map_sql)?;
    collect_rows(rows)
}

fn query_dependencies(conn: &Connection) -> ProductResult<Vec<LegacyDependencyRow>> {
    if !table_exists(conn, "task_dependencies")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT task_id, depends_on_id FROM task_dependencies")
        .map_err(map_sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyDependencyRow {
                task_id: row.get(0)?,
                depends_on_id: row.get(1)?,
            })
        })
        .map_err(map_sql)?;
    collect_rows(rows)
}

fn query_sessions(conn: &Connection) -> ProductResult<Vec<LegacySessionRow>> {
    if !table_exists(conn, "task_agent_sessions")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT task_id, backend, session_id FROM task_agent_sessions")
        .map_err(map_sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LegacySessionRow {
                task_id: row.get(0)?,
                backend: row.get(1)?,
                session_id: row.get(2)?,
            })
        })
        .map_err(map_sql)?;
    collect_rows(rows)
}

fn query_timeline(conn: &Connection) -> ProductResult<Vec<LegacyTimelineRow>> {
    if !table_exists(conn, "agent_timeline_events")? {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, task_id, turn_id, backend, kind, status, title, summary, payload, \
             turn_seq, intra_turn_order \
             FROM agent_timeline_events \
             ORDER BY task_id, turn_seq, intra_turn_order",
        )
        .map_err(map_sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LegacyTimelineRow {
                id: row.get(0)?,
                task_id: row.get(1)?,
                turn_id: row.get(2)?,
                backend: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                title: row.get(6)?,
                summary: row.get(7)?,
                payload: row.get(8)?,
                turn_seq: row.get(9)?,
                intra_turn_order: row.get(10)?,
            })
        })
        .map_err(map_sql)?;
    collect_rows(rows)
}

fn table_exists(conn: &Connection, name: &str) -> ProductResult<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |row| row.get(0),
        )
        .map_err(map_sql)?;
    Ok(count > 0)
}

fn collect_rows<T>(
    rows: impl IntoIterator<Item = Result<T, rusqlite::Error>>,
) -> ProductResult<Vec<T>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_sql)?);
    }
    Ok(out)
}

fn map_sql(err: rusqlite::Error) -> ProductError {
    ProductError::Unavailable {
        message: format!("legacy sqlite: {err}"),
    }
}

fn map_status(value: &str, archived: bool) -> ProductTaskStatus {
    if archived {
        return ProductTaskStatus::Cancelled;
    }
    match value {
        "waiting" => ProductTaskStatus::Waiting,
        "running" => ProductTaskStatus::Running,
        "blocked" => ProductTaskStatus::Blocked,
        "done" => ProductTaskStatus::Done,
        "cancelled" => ProductTaskStatus::Cancelled,
        _ => ProductTaskStatus::Draft,
    }
}

/// Deterministic AgentKit session id for a migrated Claude/Codex conversation.
pub fn planned_agentkit_session_id(backend: &str, legacy_session_id: &str) -> String {
    format!("agentkit-from-legacy:{backend}:{legacy_session_id}")
}

fn plan_legacy_session(session: &LegacySessionRow) -> LegacySessionPlan {
    let backend = session.backend.to_ascii_lowercase();
    match backend.as_str() {
        "claude" | "codex" => {
            let new_id = planned_agentkit_session_id(&backend, &session.session_id);
            LegacySessionPlan {
                task_id: session.task_id.clone(),
                legacy_backend: backend.clone(),
                legacy_session_id: session.session_id.clone(),
                // New AgentKit session + readonly provenance; never forge tool completion.
                disposition: "migrated_to_agentkit".into(),
                compat_until: Some(LEGACY_SESSION_COMPAT_UNTIL.into()),
                new_agent_session_id: Some(new_id),
                notes: format!(
                    "Legacy {} session → new AgentKit session binding for subsequent Native turns; \
                     readable history kept as provenance; pending approvals are not migrated; \
                     limited Legacy continue allowed until product {}.",
                    session.backend, LEGACY_SESSION_COMPAT_UNTIL
                ),
            }
        }
        other => LegacySessionPlan {
            task_id: session.task_id.clone(),
            legacy_backend: other.to_string(),
            legacy_session_id: session.session_id.clone(),
            disposition: "skipped".into(),
            compat_until: Some(LEGACY_SESSION_COMPAT_UNTIL.into()),
            new_agent_session_id: None,
            notes: format!(
                "Backend `{other}` is not a Claude/Codex legacy session; skipped for AgentKit migration"
            ),
        },
    }
}

fn timeline_skips_cross_runtime_pending(row: &LegacyTimelineRow) -> bool {
    let kind = row.kind.to_ascii_lowercase();
    let status = row.status.to_ascii_lowercase();
    kind.contains("pending")
        || kind.contains("approval")
        || kind.contains("question")
        || status == "pending"
        || status == "waiting_approval"
}

fn now_stamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    millis.to_string()
}

fn empty_report(mode: MigrationMode, legacy_db: &Path, product_db: &Path) -> MigrationReport {
    MigrationReport {
        mode,
        legacy_db: legacy_db.display().to_string(),
        product_db: product_db.display().to_string(),
        ok: true,
        projects_seen: 0,
        tasks_seen: 0,
        claude_sessions_seen: 0,
        codex_sessions_seen: 0,
        timeline_events_seen: 0,
        agentkit_bindings_planned: 0,
        objects: Vec::new(),
        legacy_sessions: Vec::new(),
        compat_assets: Vec::new(),
        backup_path: None,
        notes: Vec::new(),
        errors: Vec::new(),
    }
}

fn build_report(
    mode: MigrationMode,
    legacy_db: &Path,
    product_db: &Path,
    snapshot: &LegacySnapshot,
    compat_assets: Vec<CompatAssetPreview>,
) -> MigrationReport {
    let mut objects = Vec::new();
    for project in &snapshot.projects {
        objects.push(MigrationObjectResult {
            kind: ObjectKind::Project,
            id: project.id.clone(),
            action: if mode == MigrationMode::DryRun || mode == MigrationMode::Report {
                "would_upsert".into()
            } else if mode == MigrationMode::Inspect {
                "seen".into()
            } else {
                "upsert".into()
            },
            detail: Some(project.name.clone()),
        });
    }
    for task in &snapshot.tasks {
        objects.push(MigrationObjectResult {
            kind: ObjectKind::Task,
            id: task.id.clone(),
            action: if mode == MigrationMode::DryRun || mode == MigrationMode::Report {
                "would_upsert".into()
            } else if mode == MigrationMode::Inspect {
                "seen".into()
            } else {
                "upsert".into()
            },
            detail: Some(task.title.clone()),
        });
    }
    for dep in &snapshot.dependencies {
        objects.push(MigrationObjectResult {
            kind: ObjectKind::TaskDependency,
            id: format!("{}->{}", dep.task_id, dep.depends_on_id),
            action: if mode == MigrationMode::DryRun || mode == MigrationMode::Report {
                "would_link".into()
            } else if mode == MigrationMode::Inspect {
                "seen".into()
            } else {
                "link".into()
            },
            detail: None,
        });
    }

    let legacy_sessions: Vec<_> = snapshot.sessions.iter().map(plan_legacy_session).collect();
    let mut agentkit_bindings_planned = 0usize;
    for session in &legacy_sessions {
        objects.push(MigrationObjectResult {
            kind: ObjectKind::LegacySession,
            id: format!("{}:{}", session.task_id, session.legacy_backend),
            action: session.disposition.clone(),
            detail: Some(session.legacy_session_id.clone()),
        });
        if let Some(new_id) = &session.new_agent_session_id {
            agentkit_bindings_planned += 1;
            objects.push(MigrationObjectResult {
                kind: ObjectKind::AgentKitBinding,
                id: format!("bind:{}:{}", session.task_id, session.legacy_backend),
                action: if mode == MigrationMode::Apply {
                    "upsert_binding".into()
                } else {
                    "would_upsert_binding".into()
                },
                detail: Some(new_id.clone()),
            });
        }
    }

    let mut timeline_migratable = 0usize;
    for row in &snapshot.timeline {
        if timeline_skips_cross_runtime_pending(row) {
            objects.push(MigrationObjectResult {
                kind: ObjectKind::TimelineEvent,
                id: row.id.clone(),
                action: "skipped_pending".into(),
                detail: Some("pending approvals do not cross Runtime".into()),
            });
            continue;
        }
        timeline_migratable += 1;
        objects.push(MigrationObjectResult {
            kind: ObjectKind::TimelineEvent,
            id: row.id.clone(),
            action: if mode == MigrationMode::Apply {
                "project".into()
            } else {
                "would_project".into()
            },
            detail: Some(row.kind.clone()),
        });
    }

    for asset in &compat_assets {
        objects.push(MigrationObjectResult {
            kind: ObjectKind::CompatAsset,
            id: format!("{}:{}", asset.kind, asset.id),
            action: asset.disposition.clone(),
            detail: Some(asset.reason.clone()),
        });
    }

    let claude = legacy_sessions
        .iter()
        .filter(|s| s.legacy_backend == "claude")
        .count();
    let codex = legacy_sessions
        .iter()
        .filter(|s| s.legacy_backend == "codex")
        .count();

    MigrationReport {
        mode,
        legacy_db: legacy_db.display().to_string(),
        product_db: product_db.display().to_string(),
        ok: true,
        projects_seen: snapshot.projects.len(),
        tasks_seen: snapshot.tasks.len(),
        claude_sessions_seen: claude,
        codex_sessions_seen: codex,
        timeline_events_seen: snapshot.timeline.len(),
        agentkit_bindings_planned,
        objects,
        legacy_sessions,
        compat_assets,
        backup_path: None,
        notes: vec![
            "Product objects migrate into lilia-storage product.db".into(),
            "Claude/Codex sessions become AgentKit bindings + legacy provenance; tool attempts are not forged as completed".into(),
            format!("Legacy continue cutoff product version: {LEGACY_SESSION_COMPAT_UNTIL}"),
            format!(
                "Timeline rows projected as readonly legacy imports (pending skipped); migratable≈{timeline_migratable}"
            ),
            "MCP/Skills/Provider/Credential preview never embeds secret values".into(),
        ],
        errors: Vec::new(),
    }
}

fn apply_snapshot(
    store: &SqliteProductStore,
    timeline_store: Option<&SqliteTimelineProjectionStore>,
    snapshot: &LegacySnapshot,
) -> ProductResult<()> {
    for project in &snapshot.projects {
        let mut mapped = Project::new(ProjectId::new(&project.id)?, project.name.clone())?;
        mapped.workspace_path = project.cwd.clone();
        mapped.pinned = project.pinned;
        mapped.sort_order = project.sort_order;
        mapped.archive = ProjectArchiveState::Active;
        mapped.revision = ProductRevision::INITIAL;
        store.upsert_project(&mapped)?;
    }

    for task in &snapshot.tasks {
        let project_id = task
            .project_id
            .as_ref()
            .map(|id| ProjectId::new(id))
            .transpose()?;
        let mut mapped = ProductTask::new(TaskId::new(&task.id)?, project_id, task.title.clone())?;
        mapped.status = map_status(&task.status, task.archived);
        mapped.parent_id = task
            .parent_id
            .as_ref()
            .map(|id| TaskId::new(id))
            .transpose()?;
        mapped.pinned = task.pinned;
        mapped.sort_order = task.sort_order;
        mapped.created_at = task.created_at;
        mapped.updated_at = task.created_at;
        mapped.legacy_source = snapshot
            .sessions
            .iter()
            .find(|s| s.task_id == task.id && matches!(s.backend.as_str(), "claude" | "codex"))
            .map(|s| s.backend.to_ascii_lowercase());
        store.upsert_task(&mapped)?;

        let mut conversation = ProductConversation::new(
            ConversationId::new(&task.id)?,
            mapped.project_id.clone(),
            Some(mapped.id.clone()),
            mapped.title.clone(),
        )?;
        conversation.archived = mapped.archived;
        conversation.legacy_source = mapped.legacy_source.clone();
        conversation.created_at = mapped.created_at;
        conversation.updated_at = mapped.updated_at;
        match store.create_entity(ProductEntity::Conversation(conversation.clone())) {
            Ok(_) => {}
            Err(ProductError::Conflict { .. }) => {
                let current = store.get_entity(
                    lilia_contracts::ProductEntityKind::Conversation,
                    conversation.id.as_str(),
                )?;
                if let ProductEntity::Conversation(current_conversation) = current {
                    conversation.revision = current_conversation.revision;
                    if current_conversation != conversation {
                        store.update_entity(
                            ProductEntity::Conversation(conversation),
                            lilia_contracts::ExpectedRevision::new(
                                current_conversation.revision.get(),
                            )?,
                        )?;
                    }
                }
            }
            Err(err) => return Err(err),
        }
    }

    for task in &snapshot.tasks {
        let deps: Vec<TaskId> = snapshot
            .dependencies
            .iter()
            .filter(|d| d.task_id == task.id)
            .filter_map(|d| TaskId::new(&d.depends_on_id).ok())
            .collect();
        if deps.is_empty() {
            continue;
        }
        let mut mapped = store.get_task(&TaskId::new(&task.id)?)?;
        mapped.depends_on = deps;
        mapped.revision = mapped.revision.next();
        store.upsert_task(&mapped)?;
    }

    let session_plans: Vec<_> = snapshot.sessions.iter().map(plan_legacy_session).collect();
    for plan in &session_plans {
        let task_id = TaskId::new(&plan.task_id)?;
        store.record_legacy_session_provenance(
            &format!("{}:{}", plan.task_id, plan.legacy_backend),
            &task_id,
            &plan.legacy_backend,
            &plan.legacy_session_id,
            &plan.disposition,
            plan.compat_until.as_deref(),
            Some(&plan.notes),
        )?;
        if let Some(new_session) = &plan.new_agent_session_id {
            let binding = AgentSessionBinding {
                binding_id: BindingId::new(format!(
                    "mig-{}-{}",
                    plan.task_id, plan.legacy_backend
                ))?,
                task_id: task_id.clone(),
                conversation_id: Some(ConversationId::new(&plan.task_id)?),
                agent_session: AgentSessionRef::new(new_session.clone())?,
                profile_id: Some("native-coding".into()),
                revision: ProductRevision::INITIAL,
            };
            store.upsert_binding(&binding)?;
        }
    }

    if let Some(timeline_store) = timeline_store {
        let mut commands = Vec::new();
        for row in &snapshot.timeline {
            if timeline_skips_cross_runtime_pending(row) {
                continue;
            }
            let plan = session_plans.iter().find(|p| p.task_id == row.task_id);
            let session_id = plan
                .and_then(|p| p.new_agent_session_id.clone())
                .unwrap_or_else(|| {
                    format!(
                        "agentkit-from-legacy:{}:orphan-{}",
                        row.backend.to_ascii_lowercase(),
                        row.task_id
                    )
                });
            let Ok(task_id) = TaskId::new(&row.task_id) else {
                continue;
            };
            let Ok(agent_session) = AgentSessionRef::new(session_id.clone()) else {
                continue;
            };
            let sequence = ((row.turn_seq.max(0) as u64) << 16)
                | (row.intra_turn_order.max(0) as u64 & 0xffff);
            let payload: serde_json::Value = serde_json::from_str(&row.payload)
                .unwrap_or_else(|_| serde_json::json!({ "raw": row.payload }));
            let mut payload_obj = match payload {
                serde_json::Value::Object(map) => map,
                other => {
                    let mut map = serde_json::Map::new();
                    map.insert("legacyPayload".into(), other);
                    map
                }
            };
            payload_obj.insert("legacyImport".into(), serde_json::json!(true));
            payload_obj.insert(
                "legacyBackend".into(),
                serde_json::Value::String(row.backend.clone()),
            );
            payload_obj.insert(
                "legacyEventId".into(),
                serde_json::Value::String(row.id.clone()),
            );
            // Do not re-execute tools — mark readonly import.
            payload_obj.insert("readonly".into(), serde_json::json!(true));

            commands.push(TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::from_session_sequence(&session_id, sequence),
                    task_id,
                    agent_session,
                    sequence,
                    turn_id: row.turn_id.clone(),
                    kind: row.kind.clone(),
                    status: row.status.clone(),
                    title: row.title.clone(),
                    summary: row.summary.clone(),
                    payload: serde_json::Value::Object(payload_obj),
                    projected: false,
                },
            });
        }
        let _ = timeline_store.rebuild_from(commands)?;
    }

    Ok(())
}

fn backup_file(src: &Path, backup_dir: &Path) -> ProductResult<PathBuf> {
    fs::create_dir_all(backup_dir).map_err(|err| ProductError::Unavailable {
        message: format!("create backup dir: {err}"),
    })?;
    let stamp = now_stamp();
    let dest = backup_dir.join(format!("product-{stamp}.db"));
    if src.is_file() {
        fs::copy(src, &dest).map_err(|err| ProductError::Unavailable {
            message: format!("backup product db: {err}"),
        })?;
    } else {
        fs::write(&dest, b"").map_err(|err| ProductError::Unavailable {
            message: format!("write empty backup marker: {err}"),
        })?;
    }
    Ok(dest)
}

/// Shared migration entry used by CLI tests and hosts.
pub struct LegacyMigrationTool {
    pub paths: LiliaDataPaths,
    pub legacy_db: PathBuf,
    pub product_db: PathBuf,
}

impl LegacyMigrationTool {
    pub fn from_paths(paths: LiliaDataPaths) -> Self {
        let legacy_db = paths.legacy_desktop_db();
        let product_db = paths.product_db();
        Self {
            paths,
            legacy_db,
            product_db,
        }
    }

    pub fn with_explicit(legacy_db: PathBuf, product_db: PathBuf, home: PathBuf) -> Self {
        Self {
            paths: LiliaDataPaths::from_home(home),
            legacy_db,
            product_db,
        }
    }

    fn assets(&self) -> Vec<CompatAssetPreview> {
        preview_compat_assets(&self.paths)
    }

    pub fn inspect(&self) -> ProductResult<MigrationReport> {
        let snapshot = inspect_legacy_db(&self.legacy_db)?;
        Ok(build_report(
            MigrationMode::Inspect,
            &self.legacy_db,
            &self.product_db,
            &snapshot,
            self.assets(),
        ))
    }

    pub fn dry_run(&self) -> ProductResult<MigrationReport> {
        let snapshot = inspect_legacy_db(&self.legacy_db)?;
        Ok(build_report(
            MigrationMode::DryRun,
            &self.legacy_db,
            &self.product_db,
            &snapshot,
            self.assets(),
        ))
    }

    /// Combined inspect of legacy + durable status + compat preview (no writes).
    pub fn report(&self) -> ProductResult<MigrationReport> {
        let mut combined = if self.legacy_db.is_file() {
            let snapshot = inspect_legacy_db(&self.legacy_db)?;
            build_report(
                MigrationMode::Report,
                &self.legacy_db,
                &self.product_db,
                &snapshot,
                self.assets(),
            )
        } else {
            let mut empty = empty_report(MigrationMode::Report, &self.legacy_db, &self.product_db);
            empty.compat_assets = self.assets();
            empty
                .notes
                .push("legacy db missing; reporting compat assets only".into());
            empty
        };
        if let Ok(status) = self.status() {
            combined.notes.push(format!(
                "durable status: projects={} tasks={} claude={} codex={}",
                status.projects_seen,
                status.tasks_seen,
                status.claude_sessions_seen,
                status.codex_sessions_seen
            ));
        }
        Ok(combined)
    }

    pub fn apply(&self) -> ProductResult<MigrationReport> {
        let _ = self.paths.ensure_layout();
        let snapshot = inspect_legacy_db(&self.legacy_db)?;
        let mut report = build_report(
            MigrationMode::Apply,
            &self.legacy_db,
            &self.product_db,
            &snapshot,
            self.assets(),
        );
        report
            .notes
            .push(format!("cutover marker: {DESKTOP_PRODUCT_CORE_CUTOVER}"));
        let backup = backup_file(&self.product_db, &self.paths.migration_backup_dir())?;
        report.backup_path = Some(backup.display().to_string());

        let store = SqliteProductStore::open(&self.product_db)?;
        let timeline_path = self.paths.product_projections_db();
        let timeline_store = SqliteTimelineProjectionStore::open(&timeline_path)?;
        match apply_snapshot(&store, Some(&timeline_store), &snapshot) {
            Ok(()) => {
                match apply_compat_assets_to_agentkit_registry(&self.paths) {
                    Ok(applied) => {
                        report.objects.extend(applied.objects);
                        report.notes.push(format!(
                            "AgentKit registry: mcp={} skills={} (secret-free)",
                            applied.mcp_count, applied.skill_count
                        ));
                        // Refresh compat preview after durable registry write.
                        report.compat_assets = self.assets();
                        for asset in &mut report.compat_assets {
                            if asset.disposition == "map_to_agentkit"
                                && (asset.kind == "mcp" || asset.kind == "skill")
                            {
                                asset.disposition = "registered".into();
                                asset.reason = format!(
                                    "applied into {} / {}",
                                    crate::migration::AGENTKIT_MCP_REGISTRY_FILE,
                                    crate::migration::AGENTKIT_SKILLS_REGISTRY_FILE
                                );
                            }
                        }
                    }
                    Err(err) => {
                        report.notes.push(format!(
                            "compat registry apply warning: {err}; product/session migration kept"
                        ));
                    }
                }
                let started = now_stamp();
                let json = serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
                store.record_migration_run(
                    &format!("apply-{started}"),
                    "apply",
                    &report.legacy_db,
                    &report.product_db,
                    "completed",
                    &started,
                    Some(&started),
                    report.backup_path.as_deref(),
                    &json,
                )?;
                Ok(report)
            }
            Err(err) => {
                report.ok = false;
                report.errors.push(err.to_string());
                Err(err)
            }
        }
    }

    /// Apply the Desktop product-authority cutover exactly once.
    ///
    /// The legacy database remains in use for runtime compatibility caches, so
    /// filesystem timestamps cannot be used as a migration signal: doing so
    /// would replay stale Product rows over newer Product Core revisions.
    pub fn apply_if_needed(&self) -> ProductResult<Option<MigrationReport>> {
        if !self.legacy_db.is_file() {
            return Ok(None);
        }
        if self.product_db.is_file() {
            let store = SqliteProductStore::open(&self.product_db)?;
            if let Some(run) = store.latest_migration_run()? {
                if run.status == "completed" {
                    if let Ok(report) = serde_json::from_str::<MigrationReport>(&run.report_json) {
                        if report
                            .notes
                            .iter()
                            .any(|note| note.contains(DESKTOP_PRODUCT_CORE_CUTOVER))
                        {
                            return Ok(None);
                        }
                    }
                }
            }
        }
        self.apply().map(Some)
    }

    pub fn status(&self) -> ProductResult<MigrationReport> {
        if !self.product_db.is_file() {
            let mut report = empty_report(MigrationMode::Status, &self.legacy_db, &self.product_db);
            report.compat_assets = self.assets();
            report.notes.push("product db not created yet".into());
            return Ok(report);
        }
        let store = SqliteProductStore::open(&self.product_db)?;
        let run = store.latest_migration_run()?;
        let provenance = store.list_legacy_session_provenance()?;
        let bindings = store.list_all_bindings()?;
        let projects = store.list_projects()?.len();
        let tasks = store.list_tasks()?.len();
        let claude = provenance
            .iter()
            .filter(|p| p.legacy_backend == "claude")
            .count();
        let codex = provenance
            .iter()
            .filter(|p| p.legacy_backend == "codex")
            .count();
        Ok(MigrationReport {
            mode: MigrationMode::Status,
            legacy_db: self.legacy_db.display().to_string(),
            product_db: self.product_db.display().to_string(),
            ok: true,
            projects_seen: projects,
            tasks_seen: tasks,
            claude_sessions_seen: claude,
            codex_sessions_seen: codex,
            timeline_events_seen: 0,
            agentkit_bindings_planned: bindings.len(),
            objects: bindings
                .iter()
                .map(|b| MigrationObjectResult {
                    kind: ObjectKind::AgentKitBinding,
                    id: b.binding_id.as_str().to_string(),
                    action: "present".into(),
                    detail: Some(b.agent_session.as_str().to_string()),
                })
                .collect(),
            legacy_sessions: provenance
                .into_iter()
                .map(|p| {
                    let new_id = if p.disposition == "migrated_to_agentkit" {
                        Some(planned_agentkit_session_id(
                            &p.legacy_backend,
                            &p.legacy_session_id,
                        ))
                    } else {
                        None
                    };
                    LegacySessionPlan {
                        task_id: p.task_id,
                        legacy_backend: p.legacy_backend,
                        legacy_session_id: p.legacy_session_id,
                        disposition: p.disposition,
                        compat_until: p.compat_until,
                        new_agent_session_id: new_id,
                        notes: p.notes.unwrap_or_default(),
                    }
                })
                .collect(),
            compat_assets: self.assets(),
            backup_path: run.and_then(|r| r.backup_path),
            notes: vec![
                "status from product.db migration_runs + legacy_session_provenance + bindings"
                    .into(),
            ],
            errors: Vec::new(),
        })
    }

    /// Restore product.db from the backup taken by the latest apply.
    pub fn rollback(&self) -> ProductResult<MigrationReport> {
        let store = if self.product_db.is_file() {
            Some(SqliteProductStore::open(&self.product_db)?)
        } else {
            None
        };
        let run = store
            .as_ref()
            .map(|s| s.latest_migration_run())
            .transpose()?
            .flatten();
        let Some(run) = run else {
            return Err(ProductError::InvalidState {
                message: "no migration run to roll back".into(),
            });
        };
        let Some(backup) = run.backup_path.as_ref() else {
            return Err(ProductError::InvalidState {
                message: "latest migration run has no backup_path".into(),
            });
        };
        let backup_path = PathBuf::from(backup);
        if !backup_path.is_file() {
            return Err(ProductError::NotFound {
                entity: "migration_backup".into(),
                id: backup.clone(),
            });
        }

        let meta = fs::metadata(&backup_path).map_err(|err| ProductError::Unavailable {
            message: format!("read backup: {err}"),
        })?;
        if meta.len() == 0 {
            let _ = fs::remove_file(&self.product_db);
        } else {
            if let Some(parent) = self.product_db.parent() {
                fs::create_dir_all(parent).map_err(|err| ProductError::Unavailable {
                    message: format!("create product dir: {err}"),
                })?;
            }
            fs::copy(&backup_path, &self.product_db).map_err(|err| ProductError::Unavailable {
                message: format!("restore product db: {err}"),
            })?;
        }

        if self.product_db.is_file() {
            let store = SqliteProductStore::open(&self.product_db)?;
            let stamp = now_stamp();
            let report = MigrationReport {
                mode: MigrationMode::Rollback,
                legacy_db: self.legacy_db.display().to_string(),
                product_db: self.product_db.display().to_string(),
                ok: true,
                projects_seen: store.list_projects().map(|v| v.len()).unwrap_or(0),
                tasks_seen: store.list_tasks().map(|v| v.len()).unwrap_or(0),
                claude_sessions_seen: 0,
                codex_sessions_seen: 0,
                timeline_events_seen: 0,
                agentkit_bindings_planned: 0,
                objects: Vec::new(),
                legacy_sessions: Vec::new(),
                compat_assets: Vec::new(),
                backup_path: Some(backup.clone()),
                notes: vec!["rolled back product.db from migration backup".into()],
                errors: Vec::new(),
            };
            let json = serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
            store.record_migration_run(
                &format!("rollback-{stamp}"),
                "rollback",
                &report.legacy_db,
                &report.product_db,
                "rolled_back",
                &stamp,
                Some(&stamp),
                Some(backup),
                &json,
            )?;
            return Ok(report);
        }

        Ok(MigrationReport {
            mode: MigrationMode::Rollback,
            legacy_db: self.legacy_db.display().to_string(),
            product_db: self.product_db.display().to_string(),
            ok: true,
            projects_seen: 0,
            tasks_seen: 0,
            claude_sessions_seen: 0,
            codex_sessions_seen: 0,
            timeline_events_seen: 0,
            agentkit_bindings_planned: 0,
            objects: Vec::new(),
            legacy_sessions: Vec::new(),
            compat_assets: Vec::new(),
            backup_path: Some(backup.clone()),
            notes: vec!["rolled back to empty product db (removed file)".into()],
            errors: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_legacy_fixture(path: &Path) {
        let conn = Connection::open(path).unwrap();
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
              status TEXT NOT NULL DEFAULT 'waiting',
              created_at INTEGER NOT NULL,
              parent_id TEXT,
              archived INTEGER NOT NULL DEFAULT 0,
              sort_order INTEGER NOT NULL DEFAULT 0,
              pinned INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE task_dependencies (
              task_id TEXT NOT NULL,
              depends_on_id TEXT NOT NULL,
              PRIMARY KEY (task_id, depends_on_id)
            );
            CREATE TABLE task_agent_sessions (
              task_id TEXT NOT NULL,
              backend TEXT NOT NULL,
              session_id TEXT NOT NULL,
              updated_at INTEGER NOT NULL,
              PRIMARY KEY (task_id, backend)
            );
            CREATE TABLE agent_timeline_events (
              id TEXT PRIMARY KEY,
              task_id TEXT NOT NULL,
              turn_id TEXT,
              backend TEXT NOT NULL,
              kind TEXT NOT NULL,
              status TEXT NOT NULL,
              title TEXT NOT NULL,
              summary TEXT,
              payload TEXT NOT NULL,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              turn_seq INTEGER NOT NULL,
              intra_turn_order INTEGER NOT NULL
            );
            INSERT INTO projects(id, name, cwd, created_at, sort_order, pinned)
              VALUES ('proj-1', 'Demo', '/tmp/demo', 1, 0, 1);
            INSERT INTO tasks(id, project_id, session_id, title, status, created_at, parent_id, archived, sort_order, pinned)
              VALUES
                ('task-claude', 'proj-1', 'sess', 'Claude task', 'done', 1, NULL, 0, 0, 0),
                ('task-codex', 'proj-1', 'sess', 'Codex task', 'waiting', 1, NULL, 0, 1, 0);
            INSERT INTO task_dependencies(task_id, depends_on_id)
              VALUES ('task-codex', 'task-claude');
            INSERT INTO task_agent_sessions(task_id, backend, session_id, updated_at)
              VALUES
                ('task-claude', 'claude', 'claude-sess-1', 1),
                ('task-codex', 'codex', 'codex-thread-9', 1);
            INSERT INTO agent_timeline_events(
              id, task_id, turn_id, backend, kind, status, title, summary, payload,
              created_at, updated_at, turn_seq, intra_turn_order
            ) VALUES
              ('ev-1', 'task-claude', 't1', 'claude', 'message', 'done', 'hi', NULL, '{}', 1, 1, 1, 0),
              ('ev-pending', 'task-claude', 't1', 'claude', 'approval', 'pending', 'allow?', NULL, '{}', 1, 1, 1, 1),
              ('ev-2', 'task-codex', 't2', 'codex', 'message', 'done', 'yo', NULL, '{}', 1, 1, 1, 0);
            "#,
        )
        .unwrap();
    }

    #[test]
    fn dry_run_reports_claude_and_codex_without_writing() {
        let root = std::env::temp_dir().join(format!("lilia-mig-dry-{}", now_stamp()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("db")).unwrap();
        let legacy = root.join("db").join("lilia.db");
        let product = root.join("db").join("product.db");
        write_legacy_fixture(&legacy);

        let tool = LegacyMigrationTool::with_explicit(legacy, product.clone(), root);
        let report = tool.dry_run().unwrap();
        assert!(report.ok);
        assert_eq!(report.mode, MigrationMode::DryRun);
        assert_eq!(report.projects_seen, 1);
        assert_eq!(report.tasks_seen, 2);
        assert_eq!(report.claude_sessions_seen, 1);
        assert_eq!(report.codex_sessions_seen, 1);
        assert_eq!(report.agentkit_bindings_planned, 2);
        assert!(report.timeline_events_seen >= 2);
        assert!(report
            .legacy_sessions
            .iter()
            .all(|s| s.disposition == "migrated_to_agentkit"));
        assert!(report
            .legacy_sessions
            .iter()
            .all(|s| s.compat_until.as_deref() == Some(LEGACY_SESSION_COMPAT_UNTIL)));
        assert!(report
            .legacy_sessions
            .iter()
            .all(|s| s.new_agent_session_id.is_some()));
        assert!(!product.exists());
    }

    #[test]
    fn inspect_and_status_cover_migration_acceptance_surface() {
        let root = std::env::temp_dir().join(format!("lilia-mig-status-{}", now_stamp()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("db")).unwrap();
        let legacy = root.join("db").join("lilia.db");
        let product = root.join("db").join("product.db");
        write_legacy_fixture(&legacy);

        let tool = LegacyMigrationTool::with_explicit(legacy, product.clone(), root.clone());
        let inspected = tool.inspect().unwrap();
        assert!(inspected.ok);
        assert_eq!(inspected.mode, MigrationMode::Inspect);
        assert_eq!(inspected.projects_seen, 1);
        assert_eq!(inspected.claude_sessions_seen, 1);
        assert_eq!(inspected.codex_sessions_seen, 1);
        assert!(!inspected.compat_assets.is_empty());
        assert!(!product.exists());

        let reported = tool.report().unwrap();
        assert!(reported.ok);
        assert_eq!(reported.mode, MigrationMode::Report);

        let before = tool.status().unwrap();
        assert!(before.ok);
        assert_eq!(before.mode, MigrationMode::Status);
        assert_eq!(before.projects_seen, 0);

        let applied = tool.apply().unwrap();
        assert!(applied.ok);
        let after = tool.status().unwrap();
        assert!(after.ok);
        assert_eq!(after.mode, MigrationMode::Status);
        assert_eq!(after.projects_seen, 1);
        assert_eq!(after.tasks_seen, 2);
        assert_eq!(after.claude_sessions_seen, 1);
        assert_eq!(after.codex_sessions_seen, 1);
        assert_eq!(after.agentkit_bindings_planned, 2);
        assert!(after
            .legacy_sessions
            .iter()
            .all(|s| s.disposition == "migrated_to_agentkit"
                && s.compat_until.as_deref() == Some(LEGACY_SESSION_COMPAT_UNTIL)
                && s.new_agent_session_id.is_some()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_creates_agentkit_bindings_and_timeline_without_pending() {
        let root = std::env::temp_dir().join(format!("lilia-mig-bindings-{}", now_stamp()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("db")).unwrap();
        let legacy = root.join("db").join("lilia.db");
        let product = root.join("db").join("product.db");
        write_legacy_fixture(&legacy);

        let tool = LegacyMigrationTool::with_explicit(legacy, product.clone(), root.clone());
        let applied = tool.apply().unwrap();
        assert!(applied.ok);

        let store = SqliteProductStore::open(&product).unwrap();
        let claude_bindings = store
            .list_bindings_for_task(&TaskId::new("task-claude").unwrap())
            .unwrap();
        let codex_bindings = store
            .list_bindings_for_task(&TaskId::new("task-codex").unwrap())
            .unwrap();
        assert_eq!(claude_bindings.len(), 1);
        assert_eq!(codex_bindings.len(), 1);
        assert_eq!(
            claude_bindings[0].agent_session.as_str(),
            "agentkit-from-legacy:claude:claude-sess-1"
        );
        assert_eq!(
            claude_bindings[0]
                .conversation_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("task-claude")
        );
        assert_eq!(
            codex_bindings[0].agent_session.as_str(),
            "agentkit-from-legacy:codex:codex-thread-9"
        );
        let conversations = store
            .list_entities(lilia_contracts::ProductEntityKind::Conversation)
            .unwrap();
        assert_eq!(conversations.len(), 2);
        assert!(conversations.iter().all(|entity| matches!(
            entity,
            ProductEntity::Conversation(conversation)
                if conversation.task_id.as_ref().map(|id| id.as_str())
                    == Some(conversation.id.as_str())
                    && conversation.created_at == 1
        )));

        let timeline =
            SqliteTimelineProjectionStore::open(tool.paths.product_projections_db()).unwrap();
        let claude_events = timeline.list_for_task(&TaskId::new("task-claude").unwrap());
        let codex_events = timeline.list_for_task(&TaskId::new("task-codex").unwrap());
        assert_eq!(claude_events.len(), 1, "pending approval must not migrate");
        assert_eq!(codex_events.len(), 1);
        assert!(!claude_events[0].projected);
        assert_eq!(
            claude_events[0].payload.get("legacyImport"),
            Some(&serde_json::json!(true))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_is_idempotent_and_rollback_restores_backup() {
        let root = std::env::temp_dir().join(format!("lilia-mig-apply-{}", now_stamp()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("db")).unwrap();
        let legacy = root.join("db").join("lilia.db");
        let product = root.join("db").join("product.db");
        write_legacy_fixture(&legacy);

        let tool = LegacyMigrationTool::with_explicit(legacy, product.clone(), root.clone());
        let applied = tool.apply().unwrap();
        assert!(applied.ok);
        assert!(product.is_file());

        let store = SqliteProductStore::open(&product).unwrap();
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert_eq!(store.list_tasks().unwrap().len(), 2);
        let codex = store.get_task(&TaskId::new("task-codex").unwrap()).unwrap();
        assert_eq!(codex.legacy_source.as_deref(), Some("codex"));
        assert_eq!(codex.depends_on.len(), 1);
        let provenance = store.list_legacy_session_provenance().unwrap();
        assert_eq!(provenance.len(), 2);
        assert!(provenance
            .iter()
            .all(|p| p.compat_until.as_deref() == Some(LEGACY_SESSION_COMPAT_UNTIL)));
        assert!(provenance
            .iter()
            .all(|p| p.disposition == "migrated_to_agentkit"));

        let again = tool.apply().unwrap();
        assert!(again.ok);
        assert_eq!(
            SqliteProductStore::open(&product)
                .unwrap()
                .list_tasks()
                .unwrap()
                .len(),
            2
        );
        let before_cutover_skip = SqliteProductStore::open(&product)
            .unwrap()
            .get_task(&TaskId::new("task-codex").unwrap())
            .unwrap();
        assert!(tool.apply_if_needed().unwrap().is_none());
        let after_cutover_skip = SqliteProductStore::open(&product)
            .unwrap()
            .get_task(&TaskId::new("task-codex").unwrap())
            .unwrap();
        assert_eq!(after_cutover_skip, before_cutover_skip);
        assert_eq!(
            SqliteProductStore::open(&product)
                .unwrap()
                .list_all_bindings()
                .unwrap()
                .len(),
            2
        );

        let rolled = tool.rollback().unwrap();
        assert!(rolled.ok);
        assert_eq!(rolled.mode, MigrationMode::Rollback);

        let _ = fs::remove_dir_all(&root);
    }
}
