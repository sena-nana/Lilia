use std::path::PathBuf;
use std::sync::Arc;

use lilia_contracts::ProjectArchiveState;

use crate::application::{DesktopApplication, DesktopApplicationError};

pub(crate) use lilia_feature_terminal::DesktopTerminalService;
pub use lilia_feature_terminal::{
    DesktopTerminalColor, DesktopTerminalCommand, DesktopTerminalError, DesktopTerminalLaunch,
    DesktopTerminalProcessState, DesktopTerminalRestoration, DesktopTerminalRow,
    DesktopTerminalScope, DesktopTerminalSessionId, DesktopTerminalSnapshot, DesktopTerminalStyle,
    DesktopTerminalStyleSpan,
};

impl DesktopApplication {
    pub fn launch_terminal(
        &self,
        launch: DesktopTerminalLaunch,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        let cwd = self.terminal_workspace_root(&launch.scope)?;
        let events = Arc::new(lilia_feature_terminal::KernelTerminalEvents::new(
            self.inner.events.bus().clone(),
        ));
        Ok(self.inner.terminals.launch(launch, cwd, events)?)
    }

    pub fn terminal_snapshot(
        &self,
        session_id: &DesktopTerminalSessionId,
        scrollback_position: usize,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        Ok(self
            .inner
            .terminals
            .snapshot(session_id, scrollback_position)?)
    }

    pub fn list_terminal_sessions(
        &self,
    ) -> Result<Vec<DesktopTerminalSnapshot>, DesktopApplicationError> {
        Ok(self.inner.terminals.list()?)
    }

    pub fn write_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
        input: &[u8],
    ) -> Result<(), DesktopApplicationError> {
        Ok(self.inner.terminals.write(session_id, input)?)
    }

    pub fn resize_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
        rows: u16,
        columns: u16,
    ) -> Result<DesktopTerminalSnapshot, DesktopApplicationError> {
        Ok(self.inner.terminals.resize(session_id, rows, columns)?)
    }

    pub fn terminate_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
    ) -> Result<(), DesktopApplicationError> {
        Ok(self.inner.terminals.terminate(session_id)?)
    }

    pub fn forget_terminal(
        &self,
        session_id: &DesktopTerminalSessionId,
    ) -> Result<(), DesktopApplicationError> {
        Ok(self.inner.terminals.forget(session_id)?)
    }

    pub(crate) fn terminal_workspace_root(
        &self,
        scope: &DesktopTerminalScope,
    ) -> Result<PathBuf, DesktopApplicationError> {
        let root = match scope {
            DesktopTerminalScope::Project(project_id) => {
                let project = self.get_project(project_id)?;
                if project.archive == ProjectArchiveState::Archived {
                    return Err(DesktopApplicationError::InvalidInput {
                        field: "terminalScope",
                        message: format!("project `{}` is archived", project_id.as_str()),
                    });
                }
                crate::application::ProjectContext::from_project(&project)?
                    .active_root()
                    .to_path_buf()
            }
            DesktopTerminalScope::Task(task_id) => {
                let task = self.get_task(task_id)?;
                if task.archived {
                    return Err(DesktopApplicationError::InvalidInput {
                        field: "terminalScope",
                        message: format!("task `{}` is archived", task_id.as_str()),
                    });
                }
                if let Some(worktree) = self.task_worktree(task_id)? {
                    PathBuf::from(worktree.worktree_path)
                } else {
                    let project_id = task
                        .project_id
                        .ok_or(DesktopTerminalError::MissingWorkspace)?;
                    crate::application::ProjectContext::from_project(
                        &self.get_project(&project_id)?,
                    )?
                    .active_root()
                    .to_path_buf()
                }
            }
        };
        Ok(lilia_feature_terminal::canonical_directory(&root)?)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    use std::path::Path;

    use lilia_contracts::ProjectId;
    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate,
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Err(DesktopHostError::new(
                "unsupported",
                "terminal test host",
                false,
            ))
        }
    }

    fn app_with_project(root: &Path) -> (DesktopApplication, ProjectId) {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:terminal:{id}"),
            format!("terminal-test:{id}"),
        )
        .unwrap();
        let app = DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                std::env::temp_dir().join(format!("lilia-terminal-test-{id}")),
                format!("liliacode.terminal-test.{id}"),
            )
            .unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap();
        let project = app
            .create_project(DesktopProjectCreate {
                workspace_path: Some(root.display().to_string()),
                ..DesktopProjectCreate::new("Terminal")
            })
            .unwrap();
        (app, project.id)
    }

    fn wait_for_snapshot(
        app: &DesktopApplication,
        id: &DesktopTerminalSessionId,
        predicate: impl Fn(&DesktopTerminalSnapshot) -> bool,
    ) -> DesktopTerminalSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = app.terminal_snapshot(id, 0).unwrap();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "terminal state did not converge");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn screen_text(snapshot: &DesktopTerminalSnapshot) -> String {
        snapshot
            .screen
            .iter()
            .map(|row| row.text.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn terminal_runs_in_project_and_round_trips_input_through_a_real_pty() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec![
            "-lc".to_owned(),
            "printf '\\033[31mready\\033[0m:%s\\n' \"$PWD\"; read line; printf 'input:%s\\n' \"$line\""
                .to_owned(),
        ];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 80,
            })
            .unwrap();

        let ready = wait_for_snapshot(&app, &launched.id, |snapshot| {
            screen_text(snapshot).contains("ready:")
        });
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        assert!(screen_text(&ready).contains(&format!("ready:{}", canonical_root.display())));
        assert!(ready
            .screen
            .iter()
            .flat_map(|row| &row.styles)
            .any(|span| { span.style.foreground == DesktopTerminalColor::Indexed(1) }));

        app.write_terminal(&launched.id, b"hello\r").unwrap();
        let completed = wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running() && screen_text(snapshot).contains("input:hello")
        });
        assert!(matches!(
            completed.process,
            DesktopTerminalProcessState::Exited { success: true, .. }
        ));
        app.forget_terminal(&launched.id).unwrap();
        assert!(app.list_terminal_sessions().unwrap().is_empty());
    }

    #[test]
    fn terminal_resize_and_terminate_are_explicit_lifecycle_operations() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec!["-lc".to_owned(), "sleep 30".to_owned()];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 40,
            })
            .unwrap();
        let resized = app.resize_terminal(&launched.id, 12, 60).unwrap();
        assert_eq!((resized.rows, resized.columns), (12, 60));

        app.terminate_terminal(&launched.id).unwrap();
        let completed = wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running()
        });
        assert!(matches!(
            completed.process,
            DesktopTerminalProcessState::Exited { success: false, .. }
                | DesktopTerminalProcessState::Failed { .. }
        ));
    }

    #[test]
    fn terminal_workspace_restoration_never_restarts_the_process() {
        let root = tempfile::tempdir().unwrap();
        let (app, project_id) = app_with_project(root.path());
        let mut command = DesktopTerminalCommand::new("/bin/sh");
        command.arguments = vec!["-lc".to_owned(), "exit 0".to_owned()];
        let launched = app
            .launch_terminal(DesktopTerminalLaunch {
                scope: DesktopTerminalScope::Project(project_id),
                command: Some(command),
                rows: 8,
                columns: 40,
            })
            .unwrap();
        wait_for_snapshot(&app, &launched.id, |snapshot| {
            !snapshot.process.is_running()
        });
        let item = app.terminal_workspace_item(&launched).unwrap();
        let restoration = item.restoration().unwrap();
        app.forget_terminal(&launched.id).unwrap();

        let restored = app.restore_workspace_item(&restoration).unwrap().unwrap();
        let restored_state: DesktopTerminalRestoration =
            serde_json::from_value(restored.serialized_state.unwrap()).unwrap();
        assert_eq!(restored_state.id, launched.id);
        assert_eq!(
            restored_state.snapshot().process,
            DesktopTerminalProcessState::Restored
        );
        assert!(matches!(
            app.terminal_snapshot(&launched.id, 0),
            Err(DesktopApplicationError::Terminal(
                DesktopTerminalError::SessionNotFound(_)
            ))
        ));
    }

    #[test]
    fn terminal_rejects_scopes_without_a_workspace() {
        let root = tempfile::tempdir().unwrap();
        let (app, _) = app_with_project(root.path());
        let project = app
            .create_project(DesktopProjectCreate::new("No workspace"))
            .unwrap();
        assert!(matches!(
            app.launch_terminal(DesktopTerminalLaunch::shell(DesktopTerminalScope::Project(
                project.id
            ))),
            Err(DesktopApplicationError::Terminal(
                DesktopTerminalError::MissingWorkspace
            )) | Err(DesktopApplicationError::ProjectContext(_))
        ));
    }
}
