//! Composition root for the micro-kernel inside the desktop process.
//!
//! Assembles the feature list, binds every declared job protocol to the Mutsuki
//! task pool, mounts the features, and forwards kernel job transitions into the
//! shell as ordinary messages. Features never see the shell and the shell never
//! spawns a worker thread.

use std::sync::Arc;

use lilia_agent::LiliaJobRuntime;
use lilia_feature_agent_session::{AgentSessionFeature, TitlePort, TurnPort};
use lilia_feature_architecture::{ArchitectureFeature, DesktopArchitectureService};
use lilia_feature_automation::{AutomationFeature, DesktopAutomationService};
use lilia_feature_coding::{CodeSearchPort, CodingFeature, CodingRefreshPort};
use lilia_feature_composer::{ComposerFeature, PromptOptimizePort};
use lilia_feature_document::{
    DocumentFeature, LanguagePort, SharedDocumentStore, SharedLanguageRegistry,
};
use lilia_feature_extensions::{ExtensionsFeature, ExtensionsPort};
use lilia_feature_github::{GitHubFeature, GitHubPort};
use lilia_feature_hooks::HooksFeature;
use lilia_feature_import::{ImportFeature, ImportPort};
use lilia_feature_memory::{DesktopMemoryService, MemoryFeature};
use lilia_feature_project::{CloneCredentials, ProjectFeature};
use lilia_feature_provider::{AssistantProbePort, CredentialPort, ProviderFeature};
use lilia_feature_remote::{RemoteFeature, RemotePort};
use lilia_feature_roadmap::{DesktopRoadmapService, RoadmapFeature};
use lilia_feature_suggestions::{SuggestionPort, SuggestionsFeature};
use lilia_feature_task::{ProjectTaskEventFanout, ProjectTaskService, TaskFeature};
use lilia_feature_terminal::{DesktopTerminalService, TerminalFeature};
use lilia_feature_timeline::TimelineFeature;
use lilia_feature_update::{UpdateFeature, UpdatePort};
use lilia_feature_usage::{UsageFeature, UsagePort};
use lilia_feature_worktree::{WorktreeFeature, WorktreePort};
use lilia_kernel::{Feature, JobEvent, Journal, Kernel};
use lilia_service::ServiceAuthority;
use lilia_storage::Db;

use crate::shell_service::{WorkspaceSessionFeature, WorkspaceSessions};

/// Authorities the desktop process already owns and hands to the features it
/// mounts. Everything here outlives the kernel.
pub struct KernelServices {
    pub authority: ServiceAuthority,
    pub db: Db,
    pub terminals: Arc<DesktopTerminalService>,
    pub documents: SharedDocumentStore,
    pub languages: SharedLanguageRegistry,
    pub memory: DesktopMemoryService,
    pub roadmap: DesktopRoadmapService,
    pub architecture: DesktopArchitectureService,
    pub automation: DesktopAutomationService,
    pub project_tasks: ProjectTaskService,
    pub project_task_events: Arc<ProjectTaskEventFanout>,
    /// Every window's session. The primary one is built during application
    /// bootstrap because restoring persisted panes has to precede the kernel;
    /// workspace windows add theirs as they open.
    pub workspace_sessions: Arc<WorkspaceSessions>,
    /// The log the shell already writes to, shared so kernel lifecycle, job and
    /// event records interleave with the mutations recorded before boot.
    pub journal: Journal,
    pub clone_credentials: Arc<dyn CloneCredentials>,
    pub update: Arc<dyn UpdatePort>,
    pub prompt_optimize: Arc<dyn PromptOptimizePort>,
    pub code_search: Arc<dyn CodeSearchPort>,
    pub coding_refresh: Arc<dyn CodingRefreshPort>,
    pub extensions: Arc<dyn ExtensionsPort>,
    pub remote: Arc<dyn RemotePort>,
    pub credentials: Arc<dyn CredentialPort>,
    pub usage: Arc<dyn UsagePort>,
    pub github: Arc<dyn GitHubPort>,
    pub language: Arc<dyn LanguagePort>,
    pub suggestions: Arc<dyn SuggestionPort>,
    pub worktrees: Arc<dyn WorktreePort>,
    pub assistant_probes: Arc<dyn AssistantProbePort>,
    pub imports: Arc<dyn ImportPort>,
    pub titles: Arc<dyn TitlePort>,
    pub turns: Arc<dyn TurnPort>,
}

/// Kernel plus its mounted features, owned by the shell for the process
/// lifetime. Dropping it shuts the job facade down.
pub struct KernelHost {
    kernel: Kernel,
}

impl KernelHost {
    /// Builds the kernel, installs the task runtime and mounts every feature.
    ///
    /// `on_job` runs on the job worker thread, so it must only hand the event
    /// to the shell's message queue.
    pub fn start<F>(services: KernelServices, on_job: F) -> Result<Self, String>
    where
        F: Fn(JobEvent) + Send + Sync + 'static,
    {
        let journal = services.journal.clone();
        let features = features(services);
        let runtime = LiliaJobRuntime::builder()
            .protocols(features.iter().flat_map(|feature| feature.protocols()))
            .map_err(|error| format!("failed to bind job protocols: {error}"))?
            .build()
            .map_err(|error| format!("failed to start the job runtime: {error}"))?;

        let kernel = Kernel::with_journal(journal);
        kernel.jobs().install_runtime(Arc::new(runtime));
        kernel
            .events()
            .on::<JobEvent, _>(None, move |event| on_job(event.clone()));
        kernel
            .mount_all(features)
            .map_err(|error| format!("failed to mount features: {error}"))?;
        Ok(Self { kernel })
    }

    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }
}

impl Drop for KernelHost {
    fn drop(&mut self) {
        self.kernel.shutdown();
    }
}

fn features(services: KernelServices) -> Vec<Arc<dyn Feature>> {
    let KernelServices {
        authority,
        db,
        terminals,
        documents,
        languages,
        memory,
        roadmap,
        architecture,
        automation,
        project_tasks,
        project_task_events,
        workspace_sessions,
        journal: _,
        clone_credentials,
        update,
        prompt_optimize,
        code_search,
        coding_refresh,
        extensions,
        remote,
        credentials,
        usage,
        github,
        language,
        suggestions,
        worktrees,
        assistant_probes,
        imports,
        titles,
        turns,
    } = services;
    vec![
        Arc::new(ProjectFeature::new(clone_credentials)),
        Arc::new(UpdateFeature::new(update)),
        Arc::new(CodingFeature::new(code_search, coding_refresh)),
        Arc::new(TaskFeature::new(project_tasks, project_task_events)),
        Arc::new(ComposerFeature::new(db.clone(), prompt_optimize)),
        Arc::new(AgentSessionFeature::new(db.clone(), titles, turns)),
        Arc::new(WorktreeFeature::new(db, worktrees)),
        Arc::new(TimelineFeature::new(authority)),
        Arc::new(TerminalFeature::new(terminals)),
        Arc::new(DocumentFeature::new(documents, languages, language)),
        Arc::new(MemoryFeature::new(memory)),
        Arc::new(RoadmapFeature::new(roadmap)),
        Arc::new(ArchitectureFeature::new(architecture)),
        Arc::new(AutomationFeature::new(automation)),
        Arc::new(ExtensionsFeature::new(extensions)),
        Arc::new(RemoteFeature::new(remote)),
        Arc::new(HooksFeature),
        Arc::new(ProviderFeature::new(credentials, assistant_probes)),
        Arc::new(UsageFeature::new(usage)),
        Arc::new(GitHubFeature::new(github)),
        Arc::new(ImportFeature::new(imports)),
        Arc::new(SuggestionsFeature::new(suggestions)),
        Arc::new(WorkspaceSessionFeature::new(workspace_sessions)),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lilia_feature_automation::SilentAutomationEvents;
    use lilia_feature_coding::{SearchRequest, WorkspaceCodeSearchResult};
    use lilia_feature_composer::{PromptOptimizeInput, PromptOptimizeResult};
    use lilia_feature_project::NoCloneCredentials;
    use lilia_feature_remote::RemoteRequest;
    use lilia_feature_usage::{QuotaUsageStats, QuotaUsageStatsInput};
    use serde_json::Value;

    use super::*;
    use crate::application::{
        DesktopApplication, DesktopApplicationConfig, DesktopHost, DesktopHostAction,
        DesktopHostContext, DesktopHostError, DesktopHostResult,
    };
    use crate::shell_service::WorkspaceSessionsKey;

    /// Stands in for the shell's own broadcast sink so a test can tell whether
    /// the shell leg still observes mutations written through the kernel.
    #[derive(Default)]
    struct CountingProjectTaskEvents {
        projects_changed: AtomicUsize,
    }

    impl lilia_feature_task::ProjectTaskEvents for CountingProjectTaskEvents {
        fn projects_changed(&self) {
            self.projects_changed.fetch_add(1, Ordering::Relaxed);
        }

        fn tasks_changed(
            &self,
            _project_id: Option<lilia_contracts::ProjectId>,
            _task_id: Option<lilia_contracts::TaskId>,
        ) {
        }
    }

    /// Answers every port the composition root needs. The boot path never calls
    /// them, so refusing is enough to prove the wiring holds without a real
    /// desktop application behind it.
    struct IdlePort;

    impl DesktopHost for IdlePort {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    impl UpdatePort for IdlePort {
        fn check(&self, _channel: &str) -> Result<(), String> {
            Err("idle".to_owned())
        }
        fn install(&self, _version: &str) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl PromptOptimizePort for IdlePort {
        fn optimize(&self, _input: PromptOptimizeInput) -> Result<PromptOptimizeResult, String> {
            Err("idle".to_owned())
        }
    }

    impl CodeSearchPort for IdlePort {
        fn search(&self, _request: SearchRequest) -> Result<WorkspaceCodeSearchResult, String> {
            Err("idle".to_owned())
        }
    }

    impl CodingRefreshPort for IdlePort {
        fn refresh(&self, _ticket: u64) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl ExtensionsPort for IdlePort {
        fn run(&self, _ticket: u64) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl RemotePort for IdlePort {
        fn operate(&self, _request: RemoteRequest) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    impl CredentialPort for IdlePort {
        fn save_api_key(&self, _provider_id: &str) -> Result<(), String> {
            Err("idle".to_owned())
        }
        fn revoke(&self, _credential_id: &str, _revision: u64) -> Result<(), String> {
            Err("idle".to_owned())
        }
        fn refresh(&self, _provider_id: Option<&str>) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl UsagePort for IdlePort {
        fn quota(&self, _input: QuotaUsageStatsInput) -> Result<QuotaUsageStats, String> {
            Err("idle".to_owned())
        }
    }

    impl ImportPort for IdlePort {
        fn plan(&self, _ticket: u64) -> Result<Value, String> {
            Err("idle".to_owned())
        }
        fn execute(&self, _ticket: u64) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    impl AssistantProbePort for IdlePort {
        fn probe(
            &self,
            _ticket: u64,
            _kind: lilia_feature_provider::AssistantProbeKind,
        ) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    impl WorktreePort for IdlePort {
        fn operate(&self, _request: lilia_feature_worktree::WorktreeRequest) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl TitlePort for IdlePort {
        fn title(&self, _request: lilia_feature_agent_session::TitleRequest) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl TurnPort for IdlePort {
        fn run_turn(
            &self,
            _request: lilia_feature_agent_session::TurnJobRequest,
        ) -> Result<(), String> {
            Err("idle".to_owned())
        }
        fn run_approval(
            &self,
            _request: lilia_feature_agent_session::ApprovalJobRequest,
        ) -> Result<(), String> {
            Err("idle".to_owned())
        }
        fn run_interaction(
            &self,
            _request: lilia_feature_agent_session::InteractionJobRequest,
        ) -> Result<(), String> {
            Err("idle".to_owned())
        }
    }

    impl SuggestionPort for IdlePort {
        fn generate(
            &self,
            _request: lilia_feature_suggestions::GenerateRequest,
        ) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    impl LanguagePort for IdlePort {
        fn diagnostics(
            &self,
            _request: lilia_feature_document::DiagnosticsRequest,
        ) -> Result<Value, String> {
            Err("idle".to_owned())
        }
        fn definitions(
            &self,
            _request: lilia_feature_document::DefinitionRequest,
        ) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    impl GitHubPort for IdlePort {
        fn bind(&self, _context: &lilia_kernel::JobContext) -> Result<Value, String> {
            Err("idle".to_owned())
        }
        fn repositories(&self, _page: u32) -> Result<Value, String> {
            Err("idle".to_owned())
        }
    }

    /// The authority takes an exclusive writer lease on its storage key, so
    /// every set of services needs its own key to stay independent of the other
    /// tests running beside it.
    fn test_services(storage_key: &str) -> KernelServices {
        let db = Db::in_memory().expect("an in-memory database opens");
        let port = Arc::new(IdlePort);
        let authority = ServiceAuthority::bootstrap_in_memory_named(storage_key, "lilia-service")
            .expect("the authority bootstraps");
        let project_task_events = Arc::new(ProjectTaskEventFanout::default());
        let journal = Journal::new();
        let application = DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/workspace", "liliacode.test")
                .expect("the test config is valid"),
            authority.clone(),
            Arc::new(IdlePort),
        )
        .expect("the desktop application opens against the in-memory authority");
        let workspace_sessions = Arc::new(WorkspaceSessions::new());
        workspace_sessions.install(
            nana_ui_platform::WindowId::PRIMARY,
            application.default_workspace_session(),
        );
        KernelServices {
            workspace_sessions,
            project_tasks: ProjectTaskService::new(authority.clone(), project_task_events.clone())
                .with_journal(journal.clone()),
            project_task_events,
            journal,
            authority,
            db: db.clone(),
            terminals: Arc::new(DesktopTerminalService::default()),
            documents: SharedDocumentStore::default(),
            languages: SharedLanguageRegistry::default(),
            memory: DesktopMemoryService::in_memory().expect("the memory service opens"),
            roadmap: DesktopRoadmapService::from_db(db.clone()).expect("the roadmap opens"),
            architecture: DesktopArchitectureService::from_db(db.clone())
                .expect("the architecture opens"),
            automation: DesktopAutomationService::from_db(db, Arc::new(SilentAutomationEvents))
                .expect("the automation service opens"),
            clone_credentials: Arc::new(NoCloneCredentials),
            update: port.clone(),
            prompt_optimize: port.clone(),
            code_search: port.clone(),
            coding_refresh: port.clone(),
            extensions: port.clone(),
            remote: port.clone(),
            credentials: port.clone(),
            usage: port.clone(),
            github: port.clone(),
            language: port.clone(),
            suggestions: port.clone(),
            worktrees: port.clone(),
            assistant_probes: port.clone(),
            imports: port.clone(),
            titles: port.clone(),
            turns: port,
        }
    }

    #[test]
    fn the_composition_root_boots_with_every_declared_feature_mounted() {
        let declared: BTreeSet<_> = features(test_services("in-memory:boot-declared"))
            .iter()
            .map(|feature| feature.id())
            .collect();

        let host = KernelHost::start(test_services("in-memory:boot-mounted"), |_| {})
            .expect("the desktop composition root boots");

        let mounted: BTreeSet<_> = host.kernel().mounted_features().into_iter().collect();
        assert_eq!(mounted, declared);
    }

    /// A module is built from `&Kernel` and its window alone, so the session it
    /// renders has to be reachable through the registry and has to be the
    /// shell's own instance rather than a second session over the same rows.
    #[test]
    fn a_windows_workspace_session_is_resolvable_from_the_registry() {
        let services = test_services("in-memory:workspace-slot");
        let expected = services
            .workspace_sessions
            .get(nana_ui_platform::WindowId::PRIMARY)
            .expect("the primary session is installed before boot")
            .id()
            .clone();

        let host = KernelHost::start(services, |_| {}).expect("the composition root boots");
        let sessions = host
            .kernel()
            .service::<WorkspaceSessionsKey>()
            .expect("the workspace sessions slot is filled");

        let resolved = sessions
            .get(nana_ui_platform::WindowId::PRIMARY)
            .expect("the primary window has a session");
        assert_eq!(resolved.id(), &expected);
    }

    #[test]
    fn no_two_features_claim_the_same_id() {
        let features = features(test_services("in-memory:unique-ids"));

        let ids: BTreeSet<_> = features.iter().map(|feature| feature.id()).collect();

        assert_eq!(
            ids.len(),
            features.len(),
            "two features share an id, so mounting would reject one of them"
        );
    }

    /// The shell bootstraps its persistence before the kernel starts, so the
    /// task feature must publish the instance the shell already writes through.
    /// A second instance over the same rows would leave each half blind to the
    /// other's mutations.
    #[test]
    fn the_task_feature_publishes_the_host_service_instead_of_a_second_instance() {
        let services = test_services("in-memory:single-task-service");
        let shell_service = services.project_tasks.clone();
        let shell_sink = Arc::new(CountingProjectTaskEvents::default());
        services.project_task_events.install(shell_sink.clone());

        let host = KernelHost::start(services, |_| {}).expect("the composition root boots");
        let bus_notifications = Arc::new(AtomicUsize::new(0));
        {
            let observed = Arc::clone(&bus_notifications);
            host.kernel()
                .events()
                .on::<lilia_feature_task::ProjectsChanged, _>(None, move |_| {
                    observed.fetch_add(1, Ordering::Relaxed);
                });
        }

        let resolved = host
            .kernel()
            .service::<lilia_feature_task::ProjectTaskServiceKey>()
            .expect("the task feature provides its service");
        resolved
            .create_project(lilia_feature_task::DesktopProjectCreate::new(
                "Kernel resolved",
            ))
            .expect("the resolved service writes a project");

        let seen_by_shell = shell_service
            .query_projects(lilia_feature_task::ProjectQuery::default())
            .expect("the shell handle reads projects");
        assert_eq!(
            seen_by_shell.len(),
            1,
            "the shell handle cannot see the row the kernel-resolved handle wrote, \
             so they are separate instances"
        );
        assert_eq!(
            shell_sink.projects_changed.load(Ordering::Relaxed),
            1,
            "the shell broadcast leg missed the mutation"
        );
        assert_eq!(
            bus_notifications.load(Ordering::Relaxed),
            1,
            "the kernel event leg missed the mutation"
        );
    }

    /// A protocol bound twice makes the runtime refuse to build, and a protocol
    /// the shell submits but no feature declares fails only once a user
    /// triggers it. Both are boot-time facts, so they are asserted here.
    #[test]
    fn every_declared_job_protocol_is_unique_and_reaches_the_runtime() {
        let declared: Vec<_> = features(test_services("in-memory:protocols"))
            .iter()
            .flat_map(|feature| feature.protocols())
            .map(|protocol| protocol.id)
            .collect();

        let unique: BTreeSet<_> = declared.iter().cloned().collect();
        assert_eq!(
            unique.len(),
            declared.len(),
            "a job protocol is declared twice"
        );

        for expected in [
            lilia_feature_project::CLONE_PROTOCOL,
            lilia_feature_update::CHECK_PROTOCOL,
            lilia_feature_update::INSTALL_PROTOCOL,
            lilia_feature_coding::SEARCH_PROTOCOL,
            lilia_feature_coding::REFRESH_PROTOCOL,
            lilia_feature_composer::OPTIMIZE_PROMPT_PROTOCOL,
            lilia_feature_extensions::MUTATE_PROTOCOL,
            lilia_feature_remote::OPERATE_PROTOCOL,
            lilia_feature_provider::CREDENTIAL_PROTOCOL,
            lilia_feature_usage::QUOTA_PROTOCOL,
            lilia_feature_github::BIND_PROTOCOL,
            lilia_feature_github::REPOSITORIES_PROTOCOL,
            lilia_feature_document::DIAGNOSTICS_PROTOCOL,
            lilia_feature_document::DEFINITION_PROTOCOL,
            lilia_feature_suggestions::GENERATE_PROTOCOL,
            lilia_feature_worktree::OPERATE_PROTOCOL,
            lilia_feature_provider::ASSISTANT_PROBE_PROTOCOL,
            lilia_feature_import::PLAN_PROTOCOL,
            lilia_feature_import::EXECUTE_PROTOCOL,
            lilia_feature_agent_session::TITLE_PROTOCOL,
            lilia_feature_agent_session::TURN_PROTOCOL,
            lilia_feature_agent_session::APPROVAL_PROTOCOL,
            lilia_feature_agent_session::INTERACTION_PROTOCOL,
        ] {
            assert!(
                unique.contains(expected),
                "the shell submits {expected}, but no mounted feature declares it"
            );
        }
    }
}
