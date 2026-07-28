//! Minimal CLI host (#58): same `ServiceAuthority` / `LiliaClient` / Shared Runtime
//! / product projection path as Desktop and `apps/service` — not a second execution core.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use lilia_agent_integration::{
    ProductCredentialLoginInput, SharedNativeAgentKitRuntime,
};
use lilia_client::LiliaClient;
use lilia_contracts::{
    BindingId, ProductApprovalDecision, ProjectId, TaskId, TimelineProjectionEvent,
};
use lilia_service::{
    shared_runtime_ptr_eq, shared_timeline_ptr_eq, ServiceAuthority, ServiceAuthorityError,
    ServiceAuthorityStatus,
};
use mutsuki_agent_contracts::{AgentEvent, CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};
use serde::Serialize;

/// Test / smoke API key — never printed by CLI output helpers.
pub const TEST_OPENAI_API_KEY: &str = "sk-test-openai-api-key-0123456789abcdef";

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Authority(#[from] ServiceAuthorityError),
    #[error(transparent)]
    Product(#[from] lilia_contracts::ProductError),
    #[error("{0}")]
    Message(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedCoreProof {
    pub desktop_and_cli_share_runtime: bool,
    pub desktop_and_cli_share_timeline_arc: bool,
    pub shared_runtime_clients: bool,
    pub desktop_exclusive_runtime: bool,
    pub projection_event_count: usize,
    pub credential_bound: bool,
    pub approval_responded: bool,
    pub official_agent_server: bool,
    pub node_runner_default: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SameUseCaseReport {
    pub project_id: String,
    pub task_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub timeline: Vec<TimelineProjectionEvent>,
    pub proof: SharedCoreProof,
    pub status: ServiceAuthorityStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedProjectRow {
    pub id: String,
    pub name: String,
    pub workspace_path: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedTaskRow {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub legacy_source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedBindingRow {
    pub binding_id: String,
    pub task_id: String,
    pub agent_session: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedProductsView {
    pub project_count: usize,
    pub task_count: usize,
    pub binding_count: usize,
    pub provenance_count: usize,
    pub projects: Vec<MigratedProjectRow>,
    pub tasks: Vec<MigratedTaskRow>,
    pub bindings: Vec<MigratedBindingRow>,
}

/// Host-neutral CLI session bound to one `ServiceAuthority` (shared with Desktop clients).
pub struct CliSession {
    authority: ServiceAuthority,
}

impl CliSession {
    pub fn bootstrap_in_memory(storage_key: impl Into<String>) -> Result<Self, CliError> {
        Ok(Self {
            authority: ServiceAuthority::bootstrap_in_memory_named(
                storage_key,
                "lilia-cli",
            )?,
        })
    }

    pub fn bootstrap_with_home(home: impl Into<PathBuf>) -> Result<Self, CliError> {
        Ok(Self {
            authority: ServiceAuthority::bootstrap_with_home(home)?,
        })
    }

    pub fn authority(&self) -> &ServiceAuthority {
        &self.authority
    }

    pub fn client(&self) -> Result<LiliaClient<SharedNativeAgentKitRuntime>, CliError> {
        Ok(self.authority.client()?)
    }

    /// Bind credential through the shared Credential Broker (secret never echoed).
    ///
    /// Same path as Desktop `native_credential_login`: Runtime CredentialBridge,
    /// not a Host-local API on `ServiceAuthority`.
    pub fn login_test_openai_credential(&self) -> Result<(), CliError> {
        let secret = std::env::var("LILIA_CLI_TEST_API_KEY")
            .unwrap_or_else(|_| TEST_OPENAI_API_KEY.to_string());
        let runtime = self.authority.shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: secret,
                account_label: Some("cli-test".into()),
                source: Some("cli_test_credential".into()),
            })
            .map_err(|err| CliError::Message(err.to_string()))?;
        let _ = runtime
            .inner()
            .refresh_product_profile(None)
            .map_err(|err| CliError::Message(err.to_string()))?;
        Ok(())
    }

    /// Read product timeline from the Runtime projection store (ServiceAuthority
    /// surface: `projection_timeline_for_task`). Same facts Desktop/Service observe.
    pub fn product_timeline(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<TimelineProjectionEvent>, CliError> {
        Ok(self.authority.projection_timeline_for_task(task_id))
    }

    /// List durable Project / Task / Binding rows after migration (#47).
    /// Same `product.db` Desktop / Service open via `LiliaDataPaths`.
    pub fn list_migrated_products(&self) -> Result<MigratedProductsView, CliError> {
        let Some(store) = self.authority.product_store() else {
            return Err(CliError::Message(
                "products requires --home / LILIA_HOME durable product.db".into(),
            ));
        };
        let projects = store.list_projects()?;
        let tasks = store.list_tasks()?;
        let bindings = store.list_all_bindings()?;
        let provenance = store.list_legacy_session_provenance()?;
        Ok(MigratedProductsView {
            project_count: projects.len(),
            task_count: tasks.len(),
            binding_count: bindings.len(),
            provenance_count: provenance.len(),
            projects: projects
                .into_iter()
                .map(|p| MigratedProjectRow {
                    id: p.id.as_str().to_string(),
                    name: p.name,
                    workspace_path: p.workspace_path,
                })
                .collect(),
            tasks: tasks
                .into_iter()
                .map(|t| MigratedTaskRow {
                    id: t.id.as_str().to_string(),
                    project_id: t.project_id.map(|id| id.as_str().to_string()),
                    title: t.title,
                    legacy_source: t.legacy_source,
                })
                .collect(),
            bindings: bindings
                .into_iter()
                .map(|b| MigratedBindingRow {
                    binding_id: b.binding_id.as_str().to_string(),
                    task_id: b.task_id.as_str().to_string(),
                    agent_session: b.agent_session.as_str().to_string(),
                })
                .collect(),
        })
    }

    /// Desktop-sim + CLI clients from the same authority complete one use case.
    ///
    /// Proves they are not two execution cores: identical Runtime Arc, shared
    /// projection store, credential → submit (loopback) → timeline → approval.
    /// Product CRUD / approval go through `LiliaClient`; credential + streaming
    /// submit use the shared Runtime (same as Desktop host commands).
    pub fn run_same_use_case_as_desktop(&self) -> Result<SameUseCaseReport, CliError> {
        // Simulate Desktop embedded client and CLI client on one authority.
        let desktop = Arc::new(self.authority.clone());
        let cli = Arc::new(self.authority.clone());
        assert_shared_core(desktop.as_ref(), cli.as_ref())?;

        self.login_test_openai_credential()?;

        let loopback = spawn_tool_call_loopback()?;
        self.authority
            .shared_runtime()
            .inner()
            .set_model_endpoint_override(Some(loopback.endpoint.clone()));

        let client = self.client()?;
        let project = client.create_project(ProjectId::new("cli-p1").unwrap(), "CLI Demo")?;
        let task = client.create_task(
            TaskId::new("cli-t1").unwrap(),
            Some(project.id.clone()),
            "shared use case",
        )?;
        let binding = client.bind_agent_session(
            &task.id,
            None,
            Some("mutsuki.reference.coding-agent"),
            BindingId::new("cli-bind-1").unwrap(),
        )?;
        let session = binding.agent_session.clone();
        let turn_id = "turn-cli-1";

        // Streaming submit on the shared Runtime (same Arc Desktop would hold).
        let page = self
            .authority
            .shared_runtime()
            .inner()
            .submit_turn_streaming(&session, "please fix via shared core", turn_id)
            .map_err(|err| CliError::Message(err.to_string()))?;
        if !page.credential_bound {
            return Err(CliError::Message(
                "expected credential-bound live Adapter turn".into(),
            ));
        }

        let mut approval_responded = false;
        if page.events.iter().any(|envelope| {
            matches!(&envelope.event, AgentEvent::ApprovalRequest { .. })
        }) {
            // Respond through LiliaClient — identical path Desktop product facade uses.
            client.respond_approval(
                &session,
                &ProductApprovalDecision {
                    session_id: session.as_str().to_string(),
                    turn_id: turn_id.into(),
                    action_id: "call-cli-1".into(),
                    version: 1,
                    approved: true,
                },
            )?;
            approval_responded = true;
        }

        let timeline = self.product_timeline(&task.id)?;
        if timeline.is_empty() {
            return Err(CliError::Message(
                "expected product timeline projection rows after turn".into(),
            ));
        }

        // Second client (Desktop-sim) must observe the same projection facts.
        let desktop_client = desktop.client()?;
        let desktop_seen = desktop.projection_timeline_for_task(&task.id);
        if desktop_seen.len() != timeline.len() {
            return Err(CliError::Message(format!(
                "Desktop-sim and CLI timeline length diverge: {} vs {}",
                desktop_seen.len(),
                timeline.len()
            )));
        }
        let _ = desktop_client.list_bindings(&task.id);

        let status = self.authority.status();
        let proof = SharedCoreProof {
            desktop_and_cli_share_runtime: shared_runtime_ptr_eq(desktop.as_ref(), cli.as_ref()),
            desktop_and_cli_share_timeline_arc: shared_timeline_ptr_eq(
                desktop.as_ref(),
                cli.as_ref(),
            ),
            shared_runtime_clients: status.shared_runtime_clients,
            desktop_exclusive_runtime: status.desktop_exclusive_runtime,
            projection_event_count: timeline.len(),
            credential_bound: page.credential_bound,
            approval_responded,
            official_agent_server: status.capabilities.official_agent_server,
            node_runner_default: status.capabilities.node_runner_default,
        };
        if !proof.desktop_and_cli_share_runtime || proof.desktop_exclusive_runtime {
            return Err(CliError::Message(
                "CLI/Desktop clients must share one Runtime Arc".into(),
            ));
        }

        loopback.join();

        Ok(SameUseCaseReport {
            project_id: project.id.as_str().to_string(),
            task_id: task.id.as_str().to_string(),
            session_id: session.as_str().to_string(),
            turn_id: turn_id.to_string(),
            timeline,
            proof,
            status,
        })
    }
}

fn assert_shared_core(left: &ServiceAuthority, right: &ServiceAuthority) -> Result<(), CliError> {
    if !shared_runtime_ptr_eq(left, right) {
        return Err(CliError::Message(
            "ServiceAuthority clones must share Runtime Arc".into(),
        ));
    }
    if !shared_timeline_ptr_eq(left, right) {
        return Err(CliError::Message(
            "ServiceAuthority clones must share timeline Arc".into(),
        ));
    }
    Ok(())
}

struct LoopbackHandle {
    endpoint: String,
    join: Option<thread::JoinHandle<()>>,
}

impl LoopbackHandle {
    fn join(mut self) {
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

/// Fake openai-compatible loopback that emits a tool call → approval path.
fn spawn_tool_call_loopback() -> Result<LoopbackHandle, CliError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| CliError::Message(format!("bind loopback: {err}")))?;
    let address = listener
        .local_addr()
        .map_err(|err| CliError::Message(format!("loopback addr: {err}")))?;
    let join = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut bytes = [0_u8; 16_384];
            let _ = stream.read(&mut bytes);
            let body = serde_json::json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call-cli-1",
                            "type": "function",
                            "function": {
                                "name": "native.coding.fix",
                                "arguments": "{\"prompt\":\"fix\"}"
                            }
                        }]
                    }
                }],
                "usage": {"prompt_tokens": 2, "completion_tokens": 3, "total_tokens": 5}
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Ok(LoopbackHandle {
        endpoint: format!("http://{address}/v1/chat/completions"),
        join: Some(join),
    })
}

/// Parse `LILIA_HOME` / `--home` style bootstrap for the binary.
pub fn resolve_home(args: &[String]) -> Option<PathBuf> {
    args.windows(2)
        .find(|pair| pair[0] == "--home")
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| std::env::var_os("LILIA_HOME").map(PathBuf::from))
}

pub fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|err| CliError::Message(format!("json: {err}")))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_and_cli_clients_share_runtime_and_complete_same_use_case() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let session = CliSession::bootstrap_in_memory(format!("test:cli-shared-{nanos}")).unwrap();
        let report = session.run_same_use_case_as_desktop().unwrap();

        assert!(report.proof.desktop_and_cli_share_runtime);
        assert!(report.proof.desktop_and_cli_share_timeline_arc);
        assert!(report.proof.shared_runtime_clients);
        assert!(!report.proof.desktop_exclusive_runtime);
        assert!(report.proof.credential_bound);
        assert!(report.proof.approval_responded);
        assert!(!report.proof.official_agent_server);
        assert!(!report.proof.node_runner_default);
        assert!(!report.timeline.is_empty());
        assert!(report
            .timeline
            .iter()
            .any(|event| event.projected || event.kind == "tool" || event.kind == "message"));

        // LiliaClient submit_turn / respond_approval surface is wired.
        let client = session.client().unwrap();
        let caps = client.agent_capabilities().unwrap();
        assert_eq!(caps.backend, "native-agentkit");
        assert!(caps.supports_approval);
    }

    #[test]
    fn cli_with_home_uses_shared_lilia_data_paths_layout() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lilia-cli-home-{nanos}"));
        let _ = std::fs::remove_dir_all(&home);
        let session = CliSession::bootstrap_with_home(&home).unwrap();
        let status = session.authority().status();
        assert!(status.shared_projection_db_layout);
        assert!(status
            .projection_db_path
            .as_deref()
            .is_some_and(|p| p.contains("product_projections.db")));
        assert!(home.join("db").join("product.db").is_file());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn client_submit_turn_goes_through_lilia_client_port() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let session =
            CliSession::bootstrap_in_memory(format!("test:cli-submit-{nanos}")).unwrap();
        // Reference path (no credential) still uses shared Runtime via LiliaClient.
        let client = session.client().unwrap();
        let project = client
            .create_project(ProjectId::new("p-sub").unwrap(), "Submit")
            .unwrap();
        let task = client
            .create_task(
                TaskId::new("t-sub").unwrap(),
                Some(project.id),
                "submit via client",
            )
            .unwrap();
        let binding = client
            .bind_agent_session(
                &task.id,
                None,
                Some("mutsuki.reference.coding-agent"),
                BindingId::new("bind-sub").unwrap(),
            )
            .unwrap();
        client
            .submit_turn(&binding.agent_session, "fix the failing test")
            .unwrap();
        let timeline = session.product_timeline(&task.id).unwrap();
        assert!(!timeline.is_empty());
    }

    /// #47 — migrate apply → open bound session → submit_turn → timeline projection.
    #[test]
    fn migration_apply_then_first_native_turn_projects_timeline() {
        use lilia_contracts::AgentSessionRef;
        use lilia_storage::{
            planned_agentkit_session_id, LegacyMigrationTool, LiliaDataPaths, SqliteProductStore,
        };
        use rusqlite::Connection;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("lilia-mig-first-turn-{nanos}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("db")).unwrap();
        std::fs::create_dir_all(home.join("config")).unwrap();

        // Minimal legacy fixture (Claude session only).
        let legacy = home.join("db").join("lilia.db");
        {
            let conn = Connection::open(&legacy).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE projects (
                  id TEXT PRIMARY KEY, name TEXT NOT NULL, cwd TEXT,
                  created_at INTEGER NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0,
                  pinned INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE tasks (
                  id TEXT PRIMARY KEY, project_id TEXT, session_id TEXT NOT NULL,
                  title TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'waiting',
                  created_at INTEGER NOT NULL, parent_id TEXT,
                  archived INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0,
                  pinned INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE task_dependencies (
                  task_id TEXT NOT NULL, depends_on_id TEXT NOT NULL,
                  PRIMARY KEY (task_id, depends_on_id)
                );
                CREATE TABLE task_agent_sessions (
                  task_id TEXT NOT NULL, backend TEXT NOT NULL, session_id TEXT NOT NULL,
                  updated_at INTEGER NOT NULL, PRIMARY KEY (task_id, backend)
                );
                CREATE TABLE agent_timeline_events (
                  id TEXT PRIMARY KEY, task_id TEXT NOT NULL, turn_id TEXT,
                  backend TEXT NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL,
                  title TEXT NOT NULL, summary TEXT, payload TEXT,
                  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                  turn_seq INTEGER NOT NULL, intra_turn_order INTEGER NOT NULL
                );
                INSERT INTO projects VALUES ('proj-1','Demo',NULL,1,0,0);
                INSERT INTO tasks VALUES ('task-claude','proj-1','claude-sess-1','Claude chat','waiting',1,NULL,0,0,0);
                INSERT INTO task_agent_sessions VALUES ('task-claude','claude','claude-sess-1',1);
                INSERT INTO agent_timeline_events VALUES
                  ('ev-1','task-claude',NULL,'claude','message','success','hi','body','{}',1,1,1,0);
                "#,
            )
            .unwrap();
        }

        // Seed Claude MCP for registry apply proof.
        std::fs::write(
            home.join("config").join("claude-mcp-servers.json"),
            r#"{"mcpServers":{"docs":{"command":"npx","args":["-y","docs"],"env":{"TOKEN":"must-not-leak"}}}}"#,
        )
        .unwrap();

        let paths = LiliaDataPaths::from_home(&home);
        let tool = LegacyMigrationTool::from_paths(paths.clone());
        let applied = tool.apply().unwrap();
        assert!(applied.ok);
        assert!(applied
            .objects
            .iter()
            .any(|o| o.id == "registry:mcp" && o.action == "registered"));

        let store = SqliteProductStore::open(paths.product_db()).unwrap();
        let task_id = TaskId::new("task-claude").unwrap();
        let bindings = store.list_bindings_for_task(&task_id).unwrap();
        assert_eq!(bindings.len(), 1);
        let expected_session = planned_agentkit_session_id("claude", "claude-sess-1");
        assert_eq!(bindings[0].agent_session.as_str(), expected_session);

        let mcp = lilia_storage::load_mcp_registry(&paths)
            .unwrap()
            .expect("mcp registry written");
        assert!(mcp.servers.iter().any(|s| s.server_id == "claude-docs"));
        let blob = std::fs::read_to_string(lilia_storage::mcp_registry_path(&paths)).unwrap();
        assert!(!blob.contains("must-not-leak"));

        // Live Runtime first turn on the migrated binding (loopback / reference path).
        let session = CliSession::bootstrap_with_home(&home).unwrap();
        let shared = session.authority().shared_runtime();
        let runtime = shared.inner();
        let bound = runtime
            .open_bound_session(
                &task_id,
                &expected_session,
                Some("mutsuki.reference.coding-agent"),
            )
            .unwrap();
        assert_eq!(bound.as_str(), expected_session);

        let agent_session = AgentSessionRef::new(expected_session.clone()).unwrap();
        runtime
            .submit_turn_streaming(&agent_session, "continue after migration", "turn-mig-1")
            .unwrap();

        let timeline = session.product_timeline(&task_id).unwrap();
        assert!(
            timeline.iter().any(|e| e.projected && e.agent_session.as_str() == expected_session),
            "expected projected events for migrated session; got {timeline:?}"
        );

        let registry_status = runtime
            .shared_agentkit_registry_status(&paths)
            .unwrap();
        assert!(
            registry_status
                .get("mcpServerCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 1
        );
        let servers = registry_status
            .get("mcpServers")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(servers.iter().any(|s| {
            s.get("serverId").and_then(|v| v.as_str()) == Some("claude-docs")
        }));

        let _ = std::fs::remove_dir_all(&home);
    }
}
