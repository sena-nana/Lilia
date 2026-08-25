use std::path::{Path, PathBuf};

use lilia_contracts::ProjectArchiveState;

use crate::application::{
    DesktopApplication, DesktopApplicationError, DesktopCliRequest, DesktopCliResult,
    DesktopEventKind, DesktopNavigationTarget, DesktopProjectCreate, DesktopProjectPatch,
    ProjectQuery,
};

impl DesktopApplication {
    pub fn handle_cli_request(
        &self,
        request: DesktopCliRequest,
    ) -> Result<DesktopCliResult, DesktopApplicationError> {
        if request.request_id.trim().is_empty() {
            return Ok(cli_rejected("CLI request id must not be empty"));
        }
        let arguments = normalized_arguments(request.arguments);
        if arguments.is_empty() {
            return Ok(cli_accepted("LiliaCode is already running"));
        }
        if let [flag, handoff_path] = arguments.as_slice() {
            if flag == "--task-handoff" {
                let _guard = self.cli_request_lock()?;
                return Ok(
                    match self.accept_task_handoff_file(
                        Path::new(handoff_path),
                        request.working_directory.as_deref(),
                    ) {
                        Ok(opened) => cli_accepted(format!(
                            "accepted task handoff {} as task {}",
                            opened.handoff_id,
                            opened.task_id.as_str()
                        )),
                        Err(error) => cli_rejected(error.to_string()),
                    },
                );
            }
        }
        if arguments.len() != 1 {
            return Ok(cli_rejected(
                "usage: liliacode [project-directory] | --task-handoff <handoff.json>",
            ));
        }
        let path = match resolve_project_path(&arguments[0], request.working_directory.as_deref()) {
            Ok(path) => path,
            Err(message) => return Ok(cli_rejected(message)),
        };
        let display_path = display_path(&path);
        let _guard = self.cli_request_lock()?;
        let project = self
            .query_projects(ProjectQuery {
                include_archived: true,
            })?
            .into_iter()
            .find(|project| {
                project
                    .workspace_path
                    .as_deref()
                    .is_some_and(|candidate| paths_equal(candidate, &display_path))
            });
        let project = match project {
            Some(project) if project.archive == ProjectArchiveState::Archived => self
                .update_project(
                    &project.id,
                    DesktopProjectPatch {
                        archived: Some(false),
                        ..DesktopProjectPatch::default()
                    },
                )?,
            Some(project) => project,
            None => {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Project")
                    .to_owned();
                let mut input = DesktopProjectCreate::new(name);
                input.workspace_path = Some(display_path.clone());
                self.create_project(input)?
            }
        };
        self.emit_event(DesktopEventKind::NavigationRequested {
            target: DesktopNavigationTarget::Project(project.id.clone()),
        });
        Ok(cli_accepted(format!(
            "opened project {}",
            project.id.as_str()
        )))
    }

    fn cli_request_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, DesktopApplicationError> {
        self.inner
            .cli_requests
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("CLI request"))
    }
}

fn normalized_arguments(arguments: Vec<String>) -> Vec<String> {
    let mut arguments = arguments
        .into_iter()
        .filter_map(|argument| {
            let argument = argument.trim();
            let argument = trim_surrounding_quotes(argument).trim();
            (!argument.is_empty()).then(|| argument.to_owned())
        })
        .collect::<Vec<_>>();
    if arguments.len() > 1 && looks_like_executable(&arguments[0]) {
        arguments.remove(0);
    }
    arguments
}

fn looks_like_executable(value: &str) -> bool {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "liliacode" || name == "liliacode.exe"
        })
}

fn trim_surrounding_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn resolve_project_path(
    argument: &str,
    working_directory: Option<&Path>,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(argument);
    let path = if path.is_absolute() {
        path
    } else {
        working_directory
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    };
    if !path.exists() {
        return Err(format!("project path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!(
            "project path is not a directory: {}",
            path.display()
        ));
    }
    std::fs::canonicalize(&path)
        .map_err(|error| format!("failed to resolve project path {}: {error}", path.display()))
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

fn cli_accepted(message: impl Into<String>) -> DesktopCliResult {
    DesktopCliResult {
        accepted: true,
        exit_code: Some(0),
        message: Some(message.into()),
    }
}

fn cli_rejected(message: impl Into<String>) -> DesktopCliResult {
    DesktopCliResult {
        accepted: false,
        exit_code: Some(2),
        message: Some(message.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

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
        let id = uuid::Uuid::new_v4();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:desktop-cli:{id}"),
            format!("desktop-cli-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new("C:/lilia/native-cli", "liliacode.native-cli").unwrap(),
            authority,
            Arc::new(TestHost),
        )
        .unwrap()
    }

    #[test]
    fn cli_project_open_is_idempotent_and_emits_navigation() {
        let app = application();
        let events = app.subscribe_events();
        let root = std::env::current_dir().unwrap();
        let request = |id: &str| DesktopCliRequest {
            request_id: id.to_owned(),
            arguments: vec![root.display().to_string()],
            working_directory: None,
        };

        assert!(app.handle_cli_request(request("first")).unwrap().accepted);
        assert!(app.handle_cli_request(request("second")).unwrap().accepted);
        assert_eq!(
            app.query_projects(ProjectQuery::default()).unwrap().len(),
            1
        );
        let navigation_count = std::iter::from_fn(|| events.try_recv().ok())
            .filter(|event| matches!(event.kind, DesktopEventKind::NavigationRequested { .. }))
            .count();
        assert_eq!(navigation_count, 2);
    }

    #[test]
    fn cli_rejects_missing_or_non_directory_paths_without_creating_projects() {
        let app = application();
        let missing = std::env::temp_dir().join("lilia-native-cli-missing-path");
        let result = app
            .handle_cli_request(DesktopCliRequest {
                request_id: "missing".to_owned(),
                arguments: vec![missing.display().to_string()],
                working_directory: None,
            })
            .unwrap();

        assert!(!result.accepted);
        assert_eq!(result.exit_code, Some(2));
        assert!(app
            .query_projects(ProjectQuery::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cli_accepts_versioned_task_handoff_and_navigates_to_task() {
        let app = application();
        let events = app.subscribe_events();
        let directory = tempfile::tempdir().unwrap();
        let workspace = std::env::current_dir().unwrap();
        let handoff_path = directory.path().join("handoff.json");
        std::fs::write(
            &handoff_path,
            serde_json::json!({
                "protocol": "lilia-code-task-handoff",
                "version": 1,
                "id": "cli-handoff",
                "createdAt": "2026-08-10T00:00:00Z",
                "title": "检查仓库",
                "kind": "repository",
                "repository": {
                    "fullName": "acme/widget",
                    "worktreePath": workspace,
                    "branch": "main"
                },
                "source": {
                    "application": "LiliaGithub",
                    "route": "/repositories/acme/widget"
                },
                "problem": "检查仓库状态",
                "relatedFiles": [],
                "acceptanceCriteria": ["状态明确"]
            })
            .to_string(),
        )
        .unwrap();

        let result = app
            .handle_cli_request(DesktopCliRequest {
                request_id: "handoff".to_owned(),
                arguments: vec![
                    "--task-handoff".to_owned(),
                    handoff_path.display().to_string(),
                ],
                working_directory: Some(directory.path().to_path_buf()),
            })
            .unwrap();

        assert!(result.accepted, "{result:?}");
        assert_eq!(
            app.query_tasks(crate::application::TaskQuery::default()).unwrap().len(),
            1
        );
        assert!(
            std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(
                event.kind,
                DesktopEventKind::NavigationRequested {
                    target: DesktopNavigationTarget::Task(_)
                }
            ))
        );
    }
}
