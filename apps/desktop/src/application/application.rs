use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::sync::{Mutex, RwLock};

use lilia_contracts::{
    ArtifactProjection, ChatContextUsage, PendingProjection, ProductError, ProductTask, Project,
    ProjectArchiveState, ProjectId, SidebarNavigationContribution, TaskId,
    TimelineProjectionCursor, TimelineProjectionEvent, TimelineProjectionPage, TodoProjection,
};
use lilia_service::{ServiceAuthority, ServiceAuthorityError};
use lilia_storage::Db;
use serde::{Deserialize, Serialize};

use crate::application::agent::DesktopAgentRuntime;
use crate::application::composer::DesktopComposerStore;
use crate::application::remote::DesktopRemoteControlService;
use crate::application::submission::DesktopSubmissionStore;
use crate::application::todo::DesktopTodoStore;
use crate::application::workspace::DesktopWorkspaceState;
use crate::application::worktree::DesktopWorktreeStore;
use crate::application::{
    DesktopApplicationConfig, DesktopEvent, DesktopEventBus, DesktopEventSubscription, DesktopHost,
    DesktopHostAction, DesktopHostContext, DesktopHostError, DesktopHostResult, ProjectQuery,
    TaskQuery,
};
use crate::application::{
    DesktopArchitectureService, DesktopAutomationService, DesktopMemoryService,
    DesktopRoadmapService, InMemoryMemorySettingsStore, MemorySettingsStore, SqliteMemoryStore,
};
use crate::application::{DocumentStore, LanguageRegistry};
use lilia_feature_agent_session::DesktopTurnQueueStore;

#[derive(Clone)]
pub struct DesktopApplication {
    pub(crate) inner: Arc<DesktopApplicationInner>,
}

pub(crate) struct DesktopApplicationInner {
    config: DesktopApplicationConfig,
    pub(crate) authority: ServiceAuthority,
    pub(crate) host: Arc<dyn DesktopHost>,
    pub(crate) host_context: DesktopHostContext,
    pub(crate) events: DesktopEventBus,
    pub(crate) project_tasks: lilia_feature_task::ProjectTaskService,
    pub(crate) project_task_events: Arc<lilia_feature_task::ProjectTaskEventFanout>,
    pub(crate) journal: lilia_kernel::Journal,
    /// Held only for its `Drop`, which drains and flushes the export writer.
    pub(crate) _journal_export: Option<crate::journal_export::JournalExport>,
    pub(crate) workspace: Arc<Mutex<DesktopWorkspaceState>>,
    pub(crate) timeline: lilia_feature_timeline::TimelineService,
    pub(crate) domain_db: Db,
    pub(crate) composers: DesktopComposerStore,
    pub(crate) submissions: Mutex<DesktopSubmissionStore>,
    pub(crate) terminals: Arc<crate::application::terminal::DesktopTerminalService>,
    pub(crate) pending_turns: Mutex<DesktopTurnQueueStore>,
    pub(crate) turn_submission: Mutex<()>,
    pub(crate) guide_dispatch: Mutex<()>,
    pub(crate) todos: Mutex<DesktopTodoStore>,
    pub(crate) worktrees: Mutex<DesktopWorktreeStore>,
    pub(crate) automation: DesktopAutomationService,
    pub(crate) memory: DesktopMemoryService,
    pub(crate) roadmap: DesktopRoadmapService,
    pub(crate) architecture: DesktopArchitectureService,
    pub(crate) remote: DesktopRemoteControlService,
    pub(crate) update_state: Mutex<crate::application::DesktopUpdateState>,
    pub(crate) update_operation: Mutex<()>,
    pub(crate) provider_revision: AtomicU64,
    pub(crate) provider_settings: Mutex<crate::application::provider::DesktopAgentRuntimeSettingsState>,
    pub(crate) agent_interaction: Mutex<crate::application::agent_interaction::DesktopAgentInteractionState>,
    pub(crate) documents: lilia_feature_document::SharedDocumentStore,
    pub(crate) languages: lilia_feature_document::SharedLanguageRegistry,
    pub(crate) language_services: Mutex<crate::application::language_service::DesktopLanguageServiceState>,
    pub(crate) language_service_operations: Mutex<()>,
    pub(crate) project_files_watchers:
        Mutex<std::collections::BTreeMap<String, crate::application::project_files::ProjectFilesWatcher>>,
    pub(crate) project_files_revisions: Arc<Mutex<std::collections::BTreeMap<String, AtomicU64>>>,
    pub(crate) project_task_runs:
        Mutex<std::collections::BTreeMap<(String, String), crate::application::DesktopTerminalSessionId>>,
    pub(crate) conversation_suggestion_generation: Mutex<()>,
    pub(crate) session_search_cache:
        Mutex<Option<Arc<crate::application::session_search::SessionSearchCorpus>>>,
    pub(crate) product_change_feed: crate::application::change_feed::ProductChangeFeed,
    pub(crate) registry_file_watch: crate::application::registry_watch::RegistryFileWatch,
    pub(crate) title_update: std::sync::Arc<crate::application::title_update::DesktopTitleUpdateCoordinator>,
    pub(crate) title_update_scheduler:
        std::sync::OnceLock<Arc<dyn crate::application::title_update::DesktopTitleUpdateScheduler>>,
    pub(crate) turn_executor: std::sync::OnceLock<Arc<dyn crate::application::agent::DesktopTurnExecutor>>,
    pub(crate) agent: DesktopAgentRuntime,
    pub(crate) cli_requests: Mutex<()>,
    pub(crate) extension_registry: Mutex<()>,
    pub(crate) hook_executions: crate::application::hooks::DesktopHookExecutionStore,
    pub(crate) contribution_host: crate::application::contributions::LiliaContributionHost,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTaskSessionSnapshot {
    pub task: ProductTask,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_block: Option<crate::application::DesktopTaskRunBlock>,
    pub goal: Option<crate::application::DesktopGoalSnapshot>,
    pub context_usage: Option<ChatContextUsage>,
    pub timeline: Vec<TimelineProjectionEvent>,
    pub timeline_before_cursor: Option<TimelineProjectionCursor>,
    pub timeline_has_more_before: bool,
    pub artifacts: Vec<ArtifactProjection>,
    pub todos: Vec<TodoProjection>,
    pub task_todos: Vec<crate::application::DesktopTaskTodo>,
    pub worktree: Option<crate::application::DesktopTaskWorktree>,
    pub pending: Vec<PendingProjection>,
}

impl DesktopApplication {
    pub fn bootstrap(
        config: DesktopApplicationConfig,
        host: Arc<dyn DesktopHost>,
    ) -> Result<Self, DesktopApplicationError> {
        let credentials = crate::application::provider::persistent_credential_bridge(&config, host.clone())?;
        let authority =
            ServiceAuthority::bootstrap_with_home_and_credentials(config.home(), credentials)?;
        Self::from_authority(config, authority, host)
    }

    pub fn bootstrap_with_memory_settings(
        config: DesktopApplicationConfig,
        host: Arc<dyn DesktopHost>,
        memory_settings: impl MemorySettingsStore + 'static,
    ) -> Result<Self, DesktopApplicationError> {
        let credentials = crate::application::provider::persistent_credential_bridge(&config, host.clone())?;
        let authority =
            ServiceAuthority::bootstrap_with_home_and_credentials(config.home(), credentials)?;
        Self::from_authority_with_memory_settings(config, authority, host, memory_settings)
    }

    pub fn from_authority(
        config: DesktopApplicationConfig,
        authority: ServiceAuthority,
        host: Arc<dyn DesktopHost>,
    ) -> Result<Self, DesktopApplicationError> {
        Self::from_authority_with_memory_settings(
            config,
            authority,
            host,
            InMemoryMemorySettingsStore::default(),
        )
    }

    pub fn from_authority_with_memory_settings(
        config: DesktopApplicationConfig,
        authority: ServiceAuthority,
        host: Arc<dyn DesktopHost>,
        memory_settings: impl MemorySettingsStore + 'static,
    ) -> Result<Self, DesktopApplicationError> {
        if let Some(paths) = authority.data_paths() {
            if paths.home() != config.home() {
                return Err(DesktopApplicationError::AuthorityHomeMismatch {
                    configured: config.home().to_path_buf(),
                    authority: paths.home().to_path_buf(),
                });
            }
        }
        let host_context = DesktopHostContext::from(&config);
        // Desktop domain tables live in the same `product.db` the authority owns,
        // so they join the authority's handle instead of opening a second writer.
        let domain_connection = match authority.product_store() {
            Some(store) => store.db(),
            None => Db::in_memory()?,
        };
        let composers = DesktopComposerStore::new(domain_connection.clone())?;
        let todos = DesktopTodoStore::from_shared(domain_connection.clone())?;
        let pending_turns = DesktopTurnQueueStore::from_shared(domain_connection.clone())?;
        let hook_executions =
            crate::application::hooks::DesktopHookExecutionStore::from_shared(domain_connection.clone())?;
        let submissions = DesktopSubmissionStore::new(domain_connection.clone());
        let worktrees = DesktopWorktreeStore::from_db(domain_connection.clone())?;
        let journal = lilia_kernel::Journal::new();
        let events = DesktopEventBus::from_bus(lilia_kernel::EventBus::with_journal(journal.clone()));
        let automation = DesktopAutomationService::from_db(
            domain_connection.clone(),
            Arc::new(lilia_feature_automation::KernelAutomationEvents::new(
                events.bus().clone(),
            )),
        )?;
        // Product `projects` / `tasks` live in the authority store, which owns a
        // separate database when the authority is in-memory. Memory and roadmap
        // depend on those tables, so they only join the shared handle when the
        // authority is durable.
        let memory = if authority.data_paths().is_some() {
            DesktopMemoryService::from_db_with_settings(domain_connection.clone(), memory_settings)?
        } else {
            DesktopMemoryService::from_stores(
                SqliteMemoryStore::in_memory().map_err(crate::application::DesktopMemoryError::from)?,
                memory_settings,
            )
        };
        let roadmap = if authority.data_paths().is_some() {
            DesktopRoadmapService::from_db(domain_connection.clone())?
        } else {
            DesktopRoadmapService::in_memory()?
        };
        let architecture = DesktopArchitectureService::from_db(domain_connection.clone())?;
        let remote = DesktopRemoteControlService::from_db(
            domain_connection.clone(),
            Arc::new(crate::application::remote::DesktopRemoteWakeHost::from_host(
                host.clone(),
                host_context.clone(),
            )),
        )?;
        let provider_settings_store = if authority.data_paths().is_some() {
            lilia_storage::SqliteAgentRuntimeStateStore::open(
                config.data_paths().agent_runtime_db(),
            )
        } else {
            lilia_storage::SqliteAgentRuntimeStateStore::open_in_memory()
        }
        .map_err(|error| crate::application::DesktopProviderError::Persistence(error.to_string()))?;
        let provider_settings =
            crate::application::provider::DesktopAgentRuntimeSettingsState::open(provider_settings_store)?;
        authority
            .shared_runtime()
            .inner()
            .configure_model_runtime(crate::application::provider::runtime_configuration(
                &provider_settings.current(),
            ))
            .map_err(|error| crate::application::DesktopProviderError::Runtime(error.to_string()))?;
        let agent_interaction_store = if authority.data_paths().is_some() {
            lilia_storage::SqliteAgentRuntimeStateStore::open(
                config.data_paths().agent_runtime_db(),
            )
        } else {
            lilia_storage::SqliteAgentRuntimeStateStore::open_in_memory()
        }
        .map_err(|error| crate::application::DesktopAgentInteractionError::Persistence(error.to_string()))?;
        let agent_interaction =
            crate::application::agent_interaction::DesktopAgentInteractionState::open(agent_interaction_store)?;
        authority
            .shared_runtime()
            .inner()
            .configure_subagents(
                crate::application::agent_interaction::DesktopAgentInteractionState::runtime_definitions(
                    &agent_interaction.settings(),
                    &agent_interaction.agents,
                ),
            )
            .map_err(|error| crate::application::DesktopAgentInteractionError::RuntimeApply {
                message: error.to_string(),
                rollback_failed: None,
            })?;
        let project_task_events = Arc::new(lilia_feature_task::ProjectTaskEventFanout::default());
        project_task_events.install(Arc::new(
            lilia_feature_task::KernelProjectTaskEvents::new(events.bus().clone()),
        ));
        // The journal is built here rather than by the kernel because services
        // bootstrapped alongside storage already write facts worth recording; the
        // shell hands this instance to `Kernel::with_events` so one ordered log
        // and one bus cover both halves.
        let journal_export = crate::journal_export::install_from_env(&journal);
        let project_tasks = lilia_feature_task::ProjectTaskService::new(
            authority.clone(),
            project_task_events.clone(),
        )
        .with_journal(journal.clone());
        let contribution_host = crate::application::contributions::LiliaContributionHost::bootstrap()
            .map_err(|error| DesktopApplicationError::Contribution(error.to_string()))?;
        let timeline = lilia_feature_timeline::TimelineService::new(authority.clone());
        Ok(Self {
            inner: Arc::new(DesktopApplicationInner {
                config,
                authority,
                host,
                host_context,
                events,
                project_tasks,
                project_task_events,
                journal,
                _journal_export: journal_export,
                workspace: Arc::new(Mutex::new(DesktopWorkspaceState::default())),
                timeline,
                domain_db: domain_connection,
                composers,
                submissions: Mutex::new(submissions),
                terminals: Arc::new(crate::application::terminal::DesktopTerminalService::default()),
                pending_turns: Mutex::new(pending_turns),
                turn_submission: Mutex::new(()),
                guide_dispatch: Mutex::new(()),
                todos: Mutex::new(todos),
                worktrees: Mutex::new(worktrees),
                automation,
                memory,
                roadmap,
                architecture,
                remote,
                update_state: Mutex::new(crate::application::DesktopUpdateState::Idle),
                update_operation: Mutex::new(()),
                provider_revision: AtomicU64::new(1),
                provider_settings: Mutex::new(provider_settings),
                agent_interaction: Mutex::new(agent_interaction),
                documents: Arc::new(Mutex::new(DocumentStore::default())),
                languages: Arc::new(RwLock::new(LanguageRegistry::with_builtins())),
                language_services: Mutex::new(Default::default()),
                language_service_operations: Mutex::new(()),
                project_files_watchers: Mutex::new(std::collections::BTreeMap::new()),
                project_files_revisions: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
                project_task_runs: Mutex::new(std::collections::BTreeMap::new()),
                conversation_suggestion_generation: Mutex::new(()),
                session_search_cache: Mutex::new(None),
                product_change_feed: crate::application::change_feed::ProductChangeFeed::default(),
                registry_file_watch: crate::application::registry_watch::RegistryFileWatch::default(),
                title_update: std::sync::Arc::new(
                    crate::application::title_update::DesktopTitleUpdateCoordinator::default(),
                ),
                title_update_scheduler: std::sync::OnceLock::new(),
                turn_executor: std::sync::OnceLock::new(),
                agent: DesktopAgentRuntime::default(),
                cli_requests: Mutex::new(()),
                extension_registry: Mutex::new(()),
                hook_executions,
                contribution_host,
            }),
        })
    }

    pub fn config(&self) -> &DesktopApplicationConfig {
        &self.inner.config
    }

    #[cfg(debug_assertions)]
    pub fn hold_domain_database_writer_for_debug(
        &self,
        duration_ms: u64,
    ) -> Result<(), DesktopApplicationError> {
        if !(100..=10_000).contains(&duration_ms) {
            return Err(DesktopApplicationError::InvalidInput {
                field: "duration_ms",
                message: "debug database writer duration must be between 100 and 10000".to_owned(),
            });
        }
        let path = self.config().domain_database_path();
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("lilia-debug-database-writer".to_owned())
            .spawn(move || {
                let result = rusqlite::Connection::open(path)
                    .map_err(|error| error.to_string())
                    .and_then(|connection| {
                        connection
                            .busy_timeout(std::time::Duration::from_secs(1))
                            .map_err(|error| error.to_string())?;
                        connection
                            .execute_batch("BEGIN IMMEDIATE")
                            .map_err(|error| error.to_string())?;
                        let _ = ready_sender.send(Ok(()));
                        std::thread::sleep(std::time::Duration::from_millis(duration_ms));
                        connection
                            .execute_batch("ROLLBACK")
                            .map_err(|error| error.to_string())
                    });
                if let Err(error) = result {
                    let _ = ready_sender.send(Err(error));
                }
            })
            .map_err(|error| {
                DesktopApplicationError::Agent(format!("start debug database writer: {error}"))
            })?;
        ready_receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|error| {
                DesktopApplicationError::Agent(format!("wait for debug database writer: {error}"))
            })?
            .map_err(|error| {
                DesktopApplicationError::Agent(format!("acquire debug database writer: {error}"))
            })
    }

    pub fn authority(&self) -> &ServiceAuthority {
        &self.inner.authority
    }

    /// Handle to the database desktop domain tables share with the authority.
    pub fn domain_db(&self) -> &Db {
        &self.inner.domain_db
    }

    /// Terminal sessions this process owns.
    pub fn terminal_service(&self) -> &Arc<crate::application::terminal::DesktopTerminalService> {
        &self.inner.terminals
    }

    /// Open documents and the buffers behind them.
    pub fn document_store(&self) -> &lilia_feature_document::SharedDocumentStore {
        &self.inner.documents
    }

    /// Language definitions the editor resolves paths against.
    pub fn language_registry(&self) -> &lilia_feature_document::SharedLanguageRegistry {
        &self.inner.languages
    }

    pub fn query_projects(
        &self,
        query: ProjectQuery,
    ) -> Result<Vec<Project>, DesktopApplicationError> {
        let mut projects = self.inner.authority.client()?.products().list_projects()?;
        if !query.include_archived {
            projects.retain(|project| project.archive == ProjectArchiveState::Active);
        }
        Ok(projects)
    }

    pub fn get_project(&self, project_id: &ProjectId) -> Result<Project, DesktopApplicationError> {
        Ok(self
            .inner
            .authority
            .client()?
            .products()
            .get_project(project_id)?)
    }

    pub fn query_tasks(
        &self,
        query: TaskQuery,
    ) -> Result<Vec<ProductTask>, DesktopApplicationError> {
        let mut tasks = self.inner.authority.client()?.products().list_tasks()?;
        match query.scope {
            crate::application::DesktopTaskScope::All => {}
            crate::application::DesktopTaskScope::Project(project_id) => {
                tasks.retain(|task| task.project_id.as_ref() == Some(&project_id));
            }
            crate::application::DesktopTaskScope::Inbox => {
                tasks.retain(|task| task.project_id.is_none());
            }
        }
        if !query.include_archived {
            tasks.retain(|task| !task.archived);
        }
        Ok(tasks)
    }

    pub fn get_task(&self, task_id: &TaskId) -> Result<ProductTask, DesktopApplicationError> {
        Ok(self
            .inner
            .authority
            .client()?
            .products()
            .get_task(task_id)?)
    }

    pub fn task_session_snapshot(
        &self,
        task_id: &TaskId,
    ) -> Result<DesktopTaskSessionSnapshot, DesktopApplicationError> {
        let client = self.inner.authority.client()?;
        let task = client.products().get_task(task_id)?;
        let run_block = self.task_run_block(task_id)?;
        let runtime = self.inner.authority.shared_runtime();
        Ok(DesktopTaskSessionSnapshot {
            task,
            run_block,
            goal: self.task_goal(task_id)?,
            context_usage: self.task_context_usage(task_id)?,
            timeline: runtime.inner().product_timeline_for_task(task_id),
            timeline_before_cursor: None,
            timeline_has_more_before: false,
            artifacts: runtime.inner().product_artifacts_for_task(task_id),
            todos: runtime.inner().product_todos_for_task(task_id),
            task_todos: self.list_task_todos(task_id)?,
            worktree: self.task_worktree(task_id)?,
            pending: self.merge_task_pending(
                task_id,
                runtime.inner().product_pending_for_task(task_id),
            ),
        })
    }

    pub fn task_session_snapshot_page(
        &self,
        task_id: &TaskId,
        limit: usize,
    ) -> Result<DesktopTaskSessionSnapshot, DesktopApplicationError> {
        let mut snapshot = self.task_session_snapshot(task_id)?;
        let page = self.task_timeline_page(task_id, None, limit)?;
        snapshot.timeline = page.events;
        snapshot.timeline_before_cursor = page.before_cursor;
        snapshot.timeline_has_more_before = page.has_more_before;
        Ok(snapshot)
    }

    pub fn task_timeline_page(
        &self,
        task_id: &TaskId,
        before: Option<&TimelineProjectionCursor>,
        limit: usize,
    ) -> Result<TimelineProjectionPage, DesktopApplicationError> {
        Ok(self.inner.timeline.page(task_id, before, limit)?)
    }

    pub fn execute_host(
        &self,
        action: DesktopHostAction,
    ) -> Result<DesktopHostResult, DesktopApplicationError> {
        Ok(self.inner.host.execute(&self.inner.host_context, action)?)
    }

    pub fn subscribe_events(&self) -> DesktopEventSubscription {
        self.inner.events.subscribe()
    }

    pub fn event_bus(&self) -> lilia_kernel::EventBus {
        self.inner.events.bus().clone()
    }

    pub fn sidebar_navigation_contributions(&self) -> Vec<SidebarNavigationContribution> {
        self.inner.contribution_host.sidebar_navigation()
    }

    pub fn emit_event<E: lilia_kernel::Event>(&self, event: E) -> DesktopEvent {
        self.inner.events.publish(event)
    }
}

/// Keeps the feature's error surface identical to the one callers already
/// match on, so moving the domain out did not reshape any error path.
impl From<lilia_feature_timeline::TimelineError> for DesktopApplicationError {
    fn from(error: lilia_feature_timeline::TimelineError) -> Self {
        match error {
            lilia_feature_timeline::TimelineError::Product(error) => Self::Product(error),
            lilia_feature_timeline::TimelineError::InvalidInput { field, message } => {
                Self::InvalidInput { field, message }
            }
        }
    }
}

impl From<lilia_feature_task::TaskError> for DesktopApplicationError {
    fn from(error: lilia_feature_task::TaskError) -> Self {
        match error {
            lilia_feature_task::TaskError::Service(error) => Self::Service(error),
            lilia_feature_task::TaskError::Product(error) => Self::Product(error),
            lilia_feature_task::TaskError::InvalidInput { field, message } => {
                Self::InvalidInput { field, message }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopApplicationError {
    #[error(
        "desktop application home `{configured}` does not match authority home `{authority}`",
        configured = configured.display(),
        authority = authority.display()
    )]
    AuthorityHomeMismatch {
        configured: PathBuf,
        authority: PathBuf,
    },
    #[error(transparent)]
    Service(#[from] ServiceAuthorityError),
    #[error(transparent)]
    Product(#[from] ProductError),
    #[error(transparent)]
    Host(#[from] DesktopHostError),
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
    #[error("Native Agent operation failed: {0}")]
    Agent(String),
    #[error("Lilia contribution composition failed: {0}")]
    Contribution(String),
    #[error("task handoff failed: {0}")]
    TaskHandoff(String),
    #[error("desktop update operation is already running")]
    UpdateBusy,
    #[error("desktop update host returned an unexpected result for {0}")]
    UnexpectedUpdateHostResult(&'static str),
    #[error("desktop {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("desktop {0} state revision overflowed")]
    StateRevisionOverflow(&'static str),
    #[error(transparent)]
    PanelLayout(#[from] crate::application::PanelLayoutError),
    #[error(transparent)]
    WorkspaceSession(#[from] crate::application::DesktopWorkspaceSessionIdError),
    #[error(transparent)]
    WorkspaceSessionState(#[from] crate::application::DesktopWorkspaceSessionStateError),
    #[error(transparent)]
    WorkspaceItem(#[from] crate::application::WorkspaceItemError),
    #[error(transparent)]
    Document(#[from] crate::application::DocumentError),
    #[error(transparent)]
    Language(#[from] crate::application::LanguageRegistryError),
    #[error(transparent)]
    ProjectContext(#[from] crate::application::ProjectContextError),
    #[error(transparent)]
    ProjectFiles(#[from] crate::application::ProjectFilesError),
    #[error(transparent)]
    ProjectTask(#[from] crate::application::DesktopProjectTaskError),
    #[error(transparent)]
    Todo(#[from] crate::application::DesktopTodoError),
    #[error(transparent)]
    Composer(#[from] crate::application::DesktopComposerError),
    #[error(transparent)]
    AutoTurnDecision(#[from] crate::application::DesktopAutoTurnDecisionError),
    #[error(transparent)]
    TurnQueue(#[from] crate::application::DesktopTurnQueueError),
    #[error(transparent)]
    Database(#[from] lilia_storage::DbError),
    #[error(transparent)]
    Submission(#[from] crate::application::DesktopSubmissionError),
    #[error(transparent)]
    Terminal(#[from] crate::application::DesktopTerminalError),
    #[error(transparent)]
    Worktree(#[from] crate::application::DesktopWorktreeError),
    #[error(transparent)]
    Automation(#[from] crate::application::DesktopAutomationError),
    #[error(transparent)]
    Memory(#[from] crate::application::DesktopMemoryError),
    #[error(transparent)]
    Roadmap(#[from] crate::application::RoadmapStoreError),
    #[error(transparent)]
    Architecture(#[from] crate::application::DesktopArchitectureError),
    #[error(transparent)]
    RemoteControl(#[from] crate::application::DesktopRemoteControlError),
    #[error(transparent)]
    Provider(#[from] crate::application::DesktopProviderError),
    #[error(transparent)]
    Workspace(#[from] crate::application::WorkspaceSessionError),
    #[error(transparent)]
    AgentInteraction(#[from] crate::application::DesktopAgentInteractionError),
    #[error(transparent)]
    Hook(#[from] crate::application::DesktopHookError),
    #[error("task `{0}` has no active Goal")]
    GoalNotFound(TaskId),
    #[error("task `{0}` has no active Native Agent turn")]
    NoActiveTurn(TaskId),
    #[error("task `{task_id}` has no open interaction `{request_id}`")]
    PendingInteractionNotFound { task_id: TaskId, request_id: String },
    #[error("pending interaction `{request_id}` has unsupported kind `{kind}`")]
    UnsupportedPendingInteraction { request_id: String, kind: String },
    #[error("pending interaction `{request_id}` is invalid: {message}")]
    InvalidPendingInteraction { request_id: String, message: String },
    #[error("task `{task_id}` turn `{turn_id}` is not waiting for approval")]
    TurnNotWaitingApproval { task_id: TaskId, turn_id: String },
    #[error("task `{task_id}` turn `{turn_id}` is not waiting for interaction input")]
    TurnNotWaitingInteraction { task_id: TaskId, turn_id: String },
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use lilia_agent::ProductCredentialLoginInput;
    use lilia_contracts::{
        AgentSessionRef, ArtifactProjection, PendingProjection, PendingProjectionStatus,
        ProductEntity, ProductTask, Project, ProjectId, ProjectionEventId, TaskId,
        TimelineProjectionCommand, TimelineProjectionEvent, TodoProjection,
    };
    use mutsuki_agent_contracts::{CredentialKind, OPENAI_CREDENTIAL_PROVIDER_ID};
    use serde_json::json;

    use super::*;
    use crate::application::{DesktopCredentialAction, DesktopHostResult, DesktopSecret, DesktopWindowAction, ProjectsChanged, TurnStateChanged};

    static NEXT_APPLICATION_ID: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct RecordingHost {
        calls: Mutex<Vec<(DesktopHostContext, DesktopHostAction)>>,
        secrets: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    }

    impl DesktopHost for RecordingHost {
        fn execute(
            &self,
            context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            self.calls
                .lock()
                .unwrap()
                .push((context.clone(), action.clone()));
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Write { key, secret }) => {
                    self.secrets.lock().unwrap().insert(
                        (context.instance_identity.clone(), key),
                        secret.into_inner(),
                    );
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key }) => {
                    Ok(DesktopHostResult::Credential(
                        self.secrets
                            .lock()
                            .unwrap()
                            .get(&(context.instance_identity.clone(), key))
                            .cloned()
                            .map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key }) => {
                    self.secrets
                        .lock()
                        .unwrap()
                        .remove(&(context.instance_identity.clone(), key));
                    Ok(DesktopHostResult::Completed)
                }
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    fn application(host: Arc<dyn DesktopHost>) -> DesktopApplication {
        let application_id = NEXT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-application:{application_id}"),
            format!("desktop-application-test:{application_id}"),
        )
        .unwrap();
        let config = DesktopApplicationConfig::new("C:/lilia/lilia", "liliacode").unwrap();
        DesktopApplication::from_authority(config, authority, host).unwrap()
    }

    #[test]
    fn fresh_persistent_application_uses_product_authority_for_relational_domain_extensions() {
        let application_id = NEXT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let home = std::env::temp_dir().join(format!(
            "lilia-native-domain-bootstrap-{}-{application_id}",
            std::process::id()
        ));
        let config = DesktopApplicationConfig::new(
            &home,
            format!("liliacode.native-bootstrap-test.{application_id}"),
        )
        .unwrap();
        let legacy_sentinel = b"legacy-database-must-remain-untouched";
        config.data_paths().ensure_layout().unwrap();
        std::fs::write(config.data_paths().legacy_desktop_db(), legacy_sentinel).unwrap();

        {
            let app =
                DesktopApplication::bootstrap(config.clone(), Arc::new(RecordingHost::default()))
                    .unwrap();
            let project = app
                .create_project(crate::application::DesktopProjectCreate::new("Native project"))
                .unwrap();
            let task = app
                .create_task(crate::application::DesktopTaskCreate::new(
                    Some(project.id.clone()),
                    "Native task",
                ))
                .unwrap();

            let memory = app
                .save_memory(crate::application::MemoryUpsertInput {
                    id: None,
                    scope: crate::application::MemoryScope::Project,
                    project_id: Some(project.id.as_str().to_owned()),
                    title: "Native memory".to_owned(),
                    body: "Stored beside Product Core authority".to_owned(),
                    tags: vec!["native".to_owned()],
                    enabled: true,
                    source_task_id: Some(task.id.as_str().to_owned()),
                    expected_updated_at: None,
                })
                .unwrap();
            let milestone = app
                .create_milestone(&project.id, "Native milestone")
                .unwrap();
            let links = app
                .set_milestone_tasks(
                    &project.id,
                    &milestone.id,
                    vec![task.id.as_str().to_owned()],
                )
                .unwrap();

            assert_eq!(memory.project_id.as_deref(), Some(project.id.as_str()));
            assert_eq!(links.len(), 1);
            assert_eq!(
                config.domain_database_path(),
                config.data_paths().product_db()
            );
            assert!(config.data_paths().product_db().exists());
        }

        assert_eq!(
            std::fs::read(config.data_paths().legacy_desktop_db()).unwrap(),
            legacy_sentinel
        );

        if home.exists() {
            std::fs::remove_dir_all(&home).unwrap();
        }
    }

    fn apply_session_projection_rows(
        app: &DesktopApplication,
        task_id: &TaskId,
        session: &AgentSessionRef,
        sequence: u64,
    ) {
        let commands = [
            TimelineProjectionCommand::UpsertTimelineEvent {
                event: TimelineProjectionEvent {
                    id: ProjectionEventId::from_session_sequence(session.as_str(), sequence),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence,
                    turn_id: Some(format!("turn-{sequence}")),
                    kind: "message".into(),
                    status: "success".into(),
                    title: format!("message-{sequence}"),
                    summary: None,
                    payload: json!({ "opaqueTimeline": sequence }),
                    projected: true,
                },
            },
            TimelineProjectionCommand::UpsertArtifact {
                artifact: ArtifactProjection {
                    id: format!("artifact-row-{sequence}"),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence,
                    turn_id: Some(format!("turn-{sequence}")),
                    artifact_id: format!("artifact-{sequence}"),
                    media_type: "application/json".into(),
                    summary: format!("artifact {sequence}"),
                    kind: Some("result".into()),
                    size_bytes: Some(sequence),
                    content_hash: None,
                    content_ref: Some(json!({ "opaqueArtifact": sequence })),
                    provenance: Some("test".into()),
                    status: "available".into(),
                },
            },
            TimelineProjectionCommand::UpsertTodo {
                todo: TodoProjection {
                    id: format!("todo-row-{sequence}"),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence,
                    turn_id: Some(format!("turn-{sequence}")),
                    todo_id: format!("todo-{sequence}"),
                    revision: sequence,
                    items: json!([{ "opaqueTodo": sequence }]),
                },
            },
            TimelineProjectionCommand::UpsertPending {
                pending: PendingProjection {
                    id: format!("pending-row-{sequence}"),
                    task_id: task_id.clone(),
                    agent_session: session.clone(),
                    sequence,
                    turn_id: Some(format!("turn-{sequence}")),
                    request_id: format!("request-{sequence}"),
                    kind: "approval".into(),
                    status: PendingProjectionStatus::Open,
                    prompt: Some(format!("approve {sequence}")),
                    action_revision: Some(sequence),
                    payload: json!({ "opaquePending": sequence }),
                },
            },
        ];
        for command in commands {
            app.authority().apply_projection(command).unwrap();
        }
    }

    #[test]
    fn project_and_task_queries_use_the_shared_authority() {
        let app = application(Arc::new(RecordingHost::default()));
        let client = app.authority().client().unwrap();
        let active_id = ProjectId::new("project-active").unwrap();
        let archived_id = ProjectId::new("project-archived").unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(
                Project::new(active_id.clone(), "Active").unwrap(),
            ))
            .unwrap();
        let mut archived = Project::new(archived_id.clone(), "Archived").unwrap();
        archived.archive = ProjectArchiveState::Archived;
        client
            .products()
            .create_entity(ProductEntity::Project(archived))
            .unwrap();

        let active_task_id = TaskId::new("task-active").unwrap();
        let archived_task_id = TaskId::new("task-archived").unwrap();
        let inbox_task_id = TaskId::new("task-inbox").unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(
                    active_task_id.clone(),
                    Some(active_id.clone()),
                    "Active task",
                )
                .unwrap(),
            ))
            .unwrap();
        client
            .products()
            .create_entity(ProductEntity::Task(
                ProductTask::new(inbox_task_id.clone(), None, "Inbox task").unwrap(),
            ))
            .unwrap();
        let mut archived_task =
            ProductTask::new(archived_task_id, Some(active_id.clone()), "Archived task").unwrap();
        archived_task.archived = true;
        client
            .products()
            .create_entity(ProductEntity::Task(archived_task))
            .unwrap();

        let projects = app.query_projects(ProjectQuery::default()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, active_id);
        assert_eq!(app.get_project(&archived_id).unwrap().name, "Archived");

        let tasks = app
            .query_tasks(TaskQuery::for_project(projects[0].id.clone()))
            .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, active_task_id);
        assert_eq!(app.get_task(&active_task_id).unwrap(), tasks[0]);
        assert_eq!(app.query_tasks(TaskQuery::default()).unwrap().len(), 2);
        let inbox = app.query_tasks(TaskQuery::for_inbox()).unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, inbox_task_id);
    }

    #[test]
    fn host_calls_receive_the_preview_identity_and_home() {
        let host = Arc::new(RecordingHost::default());
        let app = application(host.clone());
        let action = DesktopHostAction::Window(DesktopWindowAction::Focus {
            window_id: "main".into(),
        });

        assert_eq!(
            app.execute_host(action.clone()).unwrap(),
            DesktopHostResult::Completed
        );
        let calls = host.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0.instance_identity, "liliacode");
        assert_eq!(calls[0].0.home, app.config().home());
        assert_eq!(calls[0].1, action);
    }

    #[test]
    fn application_events_are_scoped_to_the_configured_instance() {
        let app = application(Arc::new(RecordingHost::default()));
        let events = app.subscribe_events();

        let published = app.emit_event(ProjectsChanged);
        assert!(published.is::<ProjectsChanged>());
        let received = events.recv().unwrap();
        assert!(received.is::<ProjectsChanged>());
        assert_eq!(received.sequence(), published.sequence());
    }

    #[test]
    fn application_owns_one_document_and_language_model_across_host_clones() {
        let app = application(Arc::new(RecordingHost::default()));
        let path = std::env::current_dir().unwrap().join("src/main.rs");
        let (document, created) = app
            .open_document(&path, "fn main() {}", None, false)
            .unwrap();

        assert!(created);
        assert_eq!(document.language.unwrap().as_str(), "rust");
        let clone = app.clone();
        let revision = clone
            .edit_document(
                document.id,
                document.buffer.revision,
                vec![crate::application::TextEdit::new(3..7, "entry")],
            )
            .unwrap();
        assert_eq!(
            app.document_snapshot(document.id).unwrap().buffer.text,
            "fn entry() {}"
        );
        app.mark_document_saved(document.id, revision).unwrap();
        assert!(!clone
            .document_snapshot(document.id)
            .unwrap()
            .buffer
            .is_dirty());
    }

    #[test]
    fn task_session_snapshot_reads_projection_facts_in_storage_order() {
        let app = application(Arc::new(RecordingHost::default()));
        let task_id = TaskId::new("task-session-snapshot").unwrap();
        let project_id = ProjectId::new("project-session-snapshot").unwrap();
        app.authority()
            .client()
            .unwrap()
            .create_task(task_id.clone(), Some(project_id), "Session snapshot")
            .unwrap();
        let session = AgentSessionRef::new("session-snapshot").unwrap();
        apply_session_projection_rows(&app, &task_id, &session, 2);
        apply_session_projection_rows(&app, &task_id, &session, 1);

        let snapshot = app.task_session_snapshot(&task_id).unwrap();
        assert_eq!(snapshot.task.id, task_id);
        assert_eq!(
            snapshot
                .timeline
                .iter()
                .map(|row| row.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            snapshot
                .artifacts
                .iter()
                .map(|row| row.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            snapshot
                .todos
                .iter()
                .map(|row| row.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            snapshot
                .pending
                .iter()
                .map(|row| row.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(snapshot.timeline[0].payload, json!({ "opaqueTimeline": 1 }));
        assert_eq!(
            snapshot.artifacts[0].content_ref,
            Some(json!({ "opaqueArtifact": 1 }))
        );
        assert_eq!(snapshot.todos[0].items, json!([{ "opaqueTodo": 1 }]));
        assert_eq!(snapshot.pending[0].payload, json!({ "opaquePending": 1 }));
    }

    #[test]
    fn task_session_page_keeps_side_facts_and_exposes_older_cursor() {
        let app = application(Arc::new(RecordingHost::default()));
        let task_id = TaskId::new("task-session-page").unwrap();
        let project_id = ProjectId::new("project-session-page").unwrap();
        app.authority()
            .client()
            .unwrap()
            .create_task(task_id.clone(), Some(project_id), "Session page")
            .unwrap();
        let session = AgentSessionRef::new("session-page").unwrap();
        apply_session_projection_rows(&app, &task_id, &session, 1);
        apply_session_projection_rows(&app, &task_id, &session, 2);

        let snapshot = app.task_session_snapshot_page(&task_id, 1).unwrap();
        assert_eq!(snapshot.timeline.len(), 1);
        assert_eq!(snapshot.timeline[0].sequence, 2);
        assert!(snapshot.timeline_has_more_before);
        assert_eq!(snapshot.artifacts.len(), 2);
        assert_eq!(snapshot.todos.len(), 2);
        assert_eq!(snapshot.pending.len(), 2);

        let older = app
            .task_timeline_page(&task_id, snapshot.timeline_before_cursor.as_ref(), 1)
            .unwrap();
        assert_eq!(older.events[0].sequence, 1);
        assert!(!older.has_more_before);
    }

    #[test]
    fn task_session_snapshot_preserves_product_not_found() {
        let app = application(Arc::new(RecordingHost::default()));
        let task_id = TaskId::new("missing-session-task").unwrap();

        let error = app.task_session_snapshot(&task_id).unwrap_err();
        assert!(matches!(
            error,
            DesktopApplicationError::Product(ProductError::NotFound {
                entity,
                id,
            }) if entity == "task" && id == task_id.as_str()
        ));
    }

    #[test]
    fn task_turn_runs_through_service_wire_and_refreshes_authoritative_projection() {
        let app = application(Arc::new(RecordingHost::default()));
        let task_id = TaskId::new("task-native-turn").unwrap();
        app.authority()
            .client()
            .unwrap()
            .create_task(task_id.clone(), None, "Native turn")
            .unwrap();
        let runtime = app.authority().shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        runtime.inner().refresh_product_profile(None).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 16_384];
            let _ = stream.read(&mut request).unwrap();
            let body = json!({
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "native complete"}
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        runtime
            .inner()
            .set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        let events = app.subscribe_events();

        let dispatch = app
            .start_task_turn(crate::application::DesktopTurnRequest::new(
                task_id.clone(),
                "complete this turn",
            ))
            .unwrap();
        assert_eq!(dispatch.kind, crate::application::DesktopTurnDispatchKind::Started);

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut completed = false;
        while Instant::now() < deadline {
            let Ok(event) = events.recv_timeout(Duration::from_millis(250)) else {
                continue;
            };
            if matches!(
                event.downcast::<TurnStateChanged>(),
                Some(TurnStateChanged {
                    task_id,
                    state: crate::application::DesktopTurnState::Completed,
                    ..
                }) if task_id.as_str() == "task-native-turn"
            ) {
                completed = true;
                break;
            }
        }
        server.join().unwrap();
        assert!(completed, "Native Agent turn did not reach completion");
        let snapshot = app.task_session_snapshot(&task_id).unwrap();
        assert!(snapshot.timeline.iter().any(|event| {
            event.kind == "message"
                && event.status == "success"
                && event.summary.as_deref() == Some("native complete")
        }));
        assert_eq!(
            app.authority()
                .list_session_bindings(&task_id)
                .unwrap()
                .len(),
            1
        );
        assert!(app
            .inner
            .pending_turns
            .lock()
            .unwrap()
            .list(&task_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn restarted_application_restores_and_rejects_a_native_permission() {
        let application_id = NEXT_APPLICATION_ID.fetch_add(1, Ordering::Relaxed);
        let home = tempfile::tempdir().unwrap();
        let config = DesktopApplicationConfig::new(
            home.path(),
            format!("liliacode.native-restart-test.{application_id}"),
        )
        .unwrap();
        let host = Arc::new(RecordingHost::default());
        let app = DesktopApplication::bootstrap(config.clone(), host.clone()).unwrap();
        let project_id = ProjectId::new("project-native-rejection").unwrap();
        let task_id = TaskId::new("task-native-rejection").unwrap();
        let workspace = app.config().home().join("approval-workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let mut project = Project::new(project_id.clone(), "Native rejection").unwrap();
        project.workspace_path = Some(workspace.to_string_lossy().into_owned());
        let client = app.authority().client().unwrap();
        client
            .products()
            .create_entity(ProductEntity::Project(project))
            .unwrap();
        client
            .products()
            .create_task(
                task_id.clone(),
                Some(project_id),
                "Reject Native permission",
            )
            .unwrap();
        let runtime = app.authority().shared_runtime();
        runtime
            .inner()
            .credentials()
            .login(ProductCredentialLoginInput {
                provider_id: OPENAI_CREDENTIAL_PROVIDER_ID.into(),
                kind: CredentialKind::ApiKey,
                secret_material: "sk-test-openai-api-key-0123456789abcdef".into(),
                account_label: None,
                source: Some("user_api_key".into()),
            })
            .unwrap();
        runtime.inner().refresh_product_profile(None).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return false;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("permission fixture accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = [0_u8; 16_384];
            let _ = stream.read(&mut request).unwrap();
            let body = json!({
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "reject-write",
                            "type": "function",
                            "function": {
                                "name": "computer.fs.write",
                                "arguments": "{\"path\":\"must-not-exist.txt\",\"content\":\"blocked\",\"create\":true}"
                            }
                        }]
                    }
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            true
        });
        runtime
            .inner()
            .set_model_endpoint_override(Some(format!("http://{address}/v1/chat/completions")));
        let events = app.subscribe_events();

        app.start_task_turn(crate::application::DesktopTurnRequest::new(
            task_id.clone(),
            "ask before writing",
        ))
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut waiting = false;
        let mut failure = None;
        while Instant::now() < deadline {
            let Ok(event) = events.recv_timeout(Duration::from_millis(250)) else {
                continue;
            };
            if matches!(
                event.downcast::<TurnStateChanged>(),
                Some(TurnStateChanged {
                    task_id,
                    state: crate::application::DesktopTurnState::WaitingApproval { .. },
                    ..
                }) if task_id.as_str() == "task-native-rejection"
            ) {
                waiting = true;
                break;
            }
            if let Some(TurnStateChanged {
                task_id: event_task_id,
                state: crate::application::DesktopTurnState::Failed { message },
                ..
            }) = event.downcast()
            {
                if event_task_id == &task_id {
                    failure = Some(message.clone());
                    break;
                }
            }
        }
        let fixture_served = server.join().unwrap();
        assert!(
            waiting,
            "Native Agent turn did not request permission; fixture_served={fixture_served}; failure={failure:?}"
        );
        let original_claim = {
            let rows = app
                .inner
                .pending_turns
                .lock()
                .unwrap()
                .list(&task_id)
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].state,
                lilia_feature_agent_session::PersistedDesktopTurnState::Claimed
            );
            assert_eq!(rows[0].request.content, "ask before writing");
            rows[0].claim_token.clone().expect("original claim token")
        };
        let worker_release_deadline = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&app.inner) > 1 && Instant::now() < worker_release_deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            Arc::strong_count(&app.inner),
            1,
            "Native permission worker retained the application after entering a durable wait"
        );
        drop(events);
        drop(runtime);
        drop(client);
        drop(app);
        let restarted = DesktopApplication::bootstrap(config, host).unwrap();
        let restored = restarted
            .restore_task_runtime_from_projection(&task_id)
            .unwrap();
        assert_eq!(restored.phase, "waiting_approval");
        assert!(restored.session_id.is_some());
        restarted.restore_persisted_turn_queue().unwrap();
        let (recovered_claim, recovered_turn_id) = {
            let rows = restarted
                .inner
                .pending_turns
                .lock()
                .unwrap()
                .list(&task_id)
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].state,
                lilia_feature_agent_session::PersistedDesktopTurnState::Claimed
            );
            assert_eq!(rows[0].request.content, "ask before writing");
            (
                rows[0].claim_token.clone().expect("recovered claim token"),
                rows[0].turn_id.clone(),
            )
        };
        assert_ne!(recovered_claim, original_claim);
        assert!(matches!(
            restarted
                .inner
                .pending_turns
                .lock()
                .unwrap()
                .ack_and_claim_next(
                    &task_id,
                    recovered_turn_id.as_str(),
                    original_claim.as_str(),
                    None,
                ),
            Err(lilia_feature_agent_session::DesktopTurnQueueError::ClaimOwnership { .. })
        ));
        let events = restarted.subscribe_events();
        let request_id = restarted
            .task_session_snapshot(&task_id)
            .unwrap()
            .pending
            .into_iter()
            .find(|pending| {
                pending.status == PendingProjectionStatus::Open
                    && pending.kind == "permission_approval"
            })
            .unwrap()
            .request_id;
        restarted
            .respond_task_approval(&task_id, &request_id, false)
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut cancelled = false;
        let mut observed_states = Vec::new();
        while Instant::now() < deadline {
            let Ok(event) = events.recv_timeout(Duration::from_millis(250)) else {
                continue;
            };
            if let Some(changed) = event.downcast::<TurnStateChanged>() {
                if changed.task_id == task_id {
                    observed_states.push(format!("{:?}", changed.state));
                }
                match &changed.state {
                    crate::application::DesktopTurnState::Cancelled
                        if changed.task_id.as_str() == "task-native-rejection" =>
                    {
                        cancelled = true;
                        break;
                    }
                    crate::application::DesktopTurnState::Failed { message }
                        if changed.task_id.as_str() == "task-native-rejection" =>
                    {
                        panic!(
                            "permission rejection surfaced as an application failure: {message}"
                        )
                    }
                    _ => {}
                }
            }
        }
        assert!(
            cancelled,
            "permission rejection did not cancel the Native turn; states={observed_states:?}; runtime={:?}",
            restarted.task_runtime_snapshot(&task_id)
        );
        assert!(!workspace.join("must-not-exist.txt").exists());
        assert!(restarted
            .task_session_snapshot(&task_id)
            .unwrap()
            .pending
            .iter()
            .any(|pending| {
                pending.request_id == request_id
                    && pending.status == PendingProjectionStatus::Cancelled
            }));
        assert!(restarted
            .inner
            .pending_turns
            .lock()
            .unwrap()
            .list(&task_id)
            .unwrap()
            .is_empty());
    }
}
