use std::fs;
use std::path::{Path, PathBuf};

use lilia_contracts::TaskId;
use lilia_feature_hooks::{
    bump_hook_revision, ensure_hook_revision, execute_hook_command, hook_execution_error,
    hook_fingerprint, hook_handler_update, hook_handler_view, hook_matches, hook_io_error,
    hook_source_view, invalid_hook_input, HookExecutionDecision,
};
use lilia_storage::AgentkitHooksDocument;
use serde_json::json;

use crate::{DesktopApplication, DesktopApplicationError};

pub(crate) use lilia_feature_hooks::{HookEvent as DesktopHookEvent, HookExecutionStore as DesktopHookExecutionStore};
pub use lilia_feature_hooks::{
    HookDocumentUpdate as DesktopHookDocumentUpdate, HookDocumentView as DesktopHookDocumentView,
    HookError as DesktopHookError, HookHandlerUpdate as DesktopHookHandlerUpdate,
    HookHandlerView as DesktopHookHandlerView, HookScope as DesktopHookScope,
    HookSourceView as DesktopHookSourceView, HooksOverview as DesktopHooksOverview,
};

use lilia_feature_hooks::{PROJECT_SOURCE_ID, USER_SOURCE_ID};

/// The hooks domain raises the same failures the desktop surface already
/// renders, so it keeps the existing error shape instead of nesting one more.
impl From<lilia_feature_hooks::HooksError> for DesktopApplicationError {
    fn from(error: lilia_feature_hooks::HooksError) -> Self {
        use lilia_feature_hooks::HooksError;

        match error {
            HooksError::InvalidInput { field, message } => Self::InvalidInput { field, message },
            HooksError::Agent(message) => Self::Agent(message),
            HooksError::StateUnavailable(state) => Self::StateUnavailable(state),
            HooksError::StateRevisionOverflow(state) => Self::StateRevisionOverflow(state),
        }
    }
}

impl DesktopApplication {
    pub fn hooks_overview(
        &self,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHooksOverview, DesktopApplicationError> {
        let mut sources = vec![self.hook_source(DesktopHookScope::User, None)?];
        if let Some(project_cwd) = project_cwd.filter(|value| !value.trim().is_empty()) {
            sources.push(self.hook_source(DesktopHookScope::Project, Some(project_cwd))?);
        }
        Ok(DesktopHooksOverview {
            sources,
            warnings: Vec::new(),
        })
    }

    pub fn hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?;
        Ok(hook_source_view(
            scope,
            project_cwd,
            path,
            document.as_ref(),
        ))
    }

    pub fn read_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookDocumentView, DesktopApplicationError> {
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?;
        let source = hook_source_view(scope, project_cwd, path, document.as_ref());
        let handlers = document
            .as_ref()
            .map(|document| document.handlers.iter().map(hook_handler_view).collect())
            .unwrap_or_default();
        let raw_document = document
            .as_ref()
            .map(serde_json::to_string_pretty)
            .transpose()
            .map_err(|error| DesktopApplicationError::Agent(error.to_string()))?;
        Ok(DesktopHookDocumentView {
            source: source.clone(),
            handlers,
            raw_document,
            warnings: source.warnings.clone(),
            limitations: source.limitations.clone(),
        })
    }

    pub fn create_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        if path.exists() {
            return Err(invalid_hook_input("source", "Hook source already exists").into());
        }
        let document = AgentkitHooksDocument {
            revision: 1,
            ..AgentkitHooksDocument::default()
        };
        lilia_storage::save_hooks_document(&path, &document)?;
        Ok(hook_source_view(scope, project_cwd, path, Some(&document)))
    }

    pub fn update_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        input: DesktopHookDocumentUpdate,
    ) -> Result<DesktopHookDocumentView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let mut document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, input.expected_revision)?;
        document.handlers = input
            .handlers
            .into_iter()
            .enumerate()
            .map(|(index, handler)| hook_handler_update(handler, index))
            .collect::<Result<Vec<_>, _>>()?;
        bump_hook_revision(&mut document)?;
        lilia_storage::save_hooks_document(&path, &document)?;
        drop(_guard);
        self.read_hook_source(scope, project_cwd.as_deref())
    }

    pub fn set_hook_source_enabled(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        expected_revision: u64,
        enabled: bool,
    ) -> Result<DesktopHookSourceView, DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, project_cwd) = self.resolve_hook_source(scope, project_cwd)?;
        let mut document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, expected_revision)?;
        if document.enabled != enabled {
            document.enabled = enabled;
            bump_hook_revision(&mut document)?;
            lilia_storage::save_hooks_document(&path, &document)?;
        }
        Ok(hook_source_view(scope, project_cwd, path, Some(&document)))
    }

    pub fn delete_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
        expected_revision: u64,
    ) -> Result<(), DesktopApplicationError> {
        let _guard = self
            .inner
            .extension_registry
            .lock()
            .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
        let (path, _) = self.resolve_hook_source(scope, project_cwd)?;
        let document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| invalid_hook_input("source", "Hook source does not exist"))?;
        ensure_hook_revision(document.revision, expected_revision)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid_hook_input("source", "Hook source has an invalid path"))?;
        let staged = path.with_file_name(format!(".{file_name}.deleting-{}", uuid::Uuid::new_v4()));
        fs::rename(&path, &staged).map_err(|error| hook_io_error("stage Hook deletion", error))?;
        if let Err(error) = fs::remove_file(&staged) {
            let _ = fs::rename(&staged, &path);
            return Err(hook_io_error("delete Hook source", error).into());
        }
        Ok(())
    }

    pub(crate) fn execute_turn_hooks(
        &self,
        event: DesktopHookEvent,
        task_id: &TaskId,
        turn_id: &str,
        workspace_path: Option<&str>,
        context: &str,
    ) -> Result<(), DesktopHookError> {
        let mut sources = vec![(
            USER_SOURCE_ID.to_owned(),
            lilia_storage::user_hooks_document_path(&self.config().data_paths()),
            None,
            None,
        )];
        if let Some(workspace_path) = workspace_path.filter(|value| !value.trim().is_empty()) {
            let workspace = Path::new(workspace_path);
            if workspace.is_absolute() && workspace.is_dir() {
                let workspace = workspace.canonicalize().map_err(|error| {
                    hook_execution_error(
                        PROJECT_SOURCE_ID,
                        "source",
                        format!("resolve project workspace: {error}"),
                    )
                })?;
                sources.push((
                    PROJECT_SOURCE_ID.to_owned(),
                    lilia_storage::project_hooks_document_path(&workspace),
                    None,
                    None,
                ));
            }
        }
        for package in self.loaded_plugin_packages() {
            for (source_id, path, document) in package.hooks {
                sources.push((source_id, path, Some(package.root.clone()), Some(document)));
            }
        }

        for (source_id, path, plugin_root, loaded_document) in sources {
            let document = match loaded_document {
                Some(document) => document,
                None => {
                    let Some(document) =
                        lilia_storage::load_hooks_document(&path).map_err(|error| {
                            hook_execution_error(&source_id, "source", error.to_string())
                        })?
                    else {
                        continue;
                    };
                    document
                }
            };
            if !document.enabled {
                continue;
            }
            for handler in document
                .handlers
                .iter()
                .filter(|handler| handler.event == event.as_str())
                .filter(|handler| hook_matches(handler.matcher.as_deref(), context))
            {
                let fingerprint = hook_fingerprint(&source_id, document.revision, handler)?;
                match self.inner.hook_executions.begin(
                    turn_id,
                    event,
                    &source_id,
                    &handler.id,
                    &fingerprint,
                )? {
                    HookExecutionDecision::Completed => continue,
                    HookExecutionDecision::Failed(message) => {
                        return Err(hook_execution_error(&source_id, &handler.id, message));
                    }
                    HookExecutionDecision::Indeterminate => {
                        return Err(hook_execution_error(
                            &source_id,
                            &handler.id,
                            "previous execution outcome is unknown; refusing to replay side effects",
                        ));
                    }
                    HookExecutionDecision::ConfigurationChanged => {
                        return Err(hook_execution_error(
                            &source_id,
                            &handler.id,
                            "Hook configuration changed after this turn began; refusing recovery replay",
                        ));
                    }
                    HookExecutionDecision::Execute => {}
                }
                let payload = serde_json::to_vec(&json!({
                    "event": event.as_str(),
                    "taskId": task_id.as_str(),
                    "turnId": turn_id,
                    "projectDir": workspace_path,
                    "context": context,
                }))
                .map_err(|error| {
                    hook_execution_error(&source_id, &handler.id, error.to_string())
                })?;
                let result =
                    execute_hook_command(handler, workspace_path, plugin_root.as_deref(), &payload);
                let failure = result.as_ref().err().map(String::as_str);
                self.inner.hook_executions.finish(
                    turn_id,
                    event,
                    &source_id,
                    &handler.id,
                    &fingerprint,
                    failure,
                )?;
                if let Err(message) = result {
                    return Err(hook_execution_error(&source_id, &handler.id, message));
                }
            }
        }
        Ok(())
    }

    fn resolve_hook_source(
        &self,
        scope: DesktopHookScope,
        project_cwd: Option<&str>,
    ) -> Result<(PathBuf, Option<String>), DesktopApplicationError> {
        match scope {
            DesktopHookScope::User => Ok((
                lilia_storage::user_hooks_document_path(&self.config().data_paths()),
                None,
            )),
            DesktopHookScope::Project => {
                let raw = project_cwd
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        invalid_hook_input("project_cwd", "project Hook source requires a project")
                    })?;
                let project = Path::new(raw);
                if !project.is_absolute() || !project.is_dir() {
                    return Err(invalid_hook_input(
                        "project_cwd",
                        "project Hook source requires an existing absolute directory",
                    )
                    .into());
                }
                let project = project
                    .canonicalize()
                    .map_err(|error| hook_io_error("resolve project Hook workspace", error))?;
                Ok((
                    lilia_storage::project_hooks_document_path(&project),
                    Some(project.to_string_lossy().into_owned()),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::{DesktopApplicationConfig, DesktopHost};

    #[derive(Default)]
    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &crate::DesktopHostContext,
            _action: crate::DesktopHostAction,
        ) -> Result<crate::DesktopHostResult, crate::DesktopHostError> {
            Ok(crate::DesktopHostResult::Completed)
        }
    }

    fn application(home: &Path) -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:hooks:{}", uuid::Uuid::new_v4()),
            "hooks-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(home, "hooks-test").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    #[test]
    fn hook_source_lifecycle_is_revisioned_and_project_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let application = application(&home);

        let created = application
            .create_hook_source(DesktopHookScope::Project, project.to_str())
            .unwrap();
        assert_eq!(created.revision, 1);
        assert!(!created.enabled);
        assert!(
            created.path.ends_with(".lilia\\hooks.json")
                || created.path.ends_with(".lilia/hooks.json")
        );

        let updated = application
            .update_hook_source(
                DesktopHookScope::Project,
                project.to_str(),
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: vec![DesktopHookHandlerUpdate {
                        id: Some("prompt-check".to_owned()),
                        event: "UserPromptSubmit".to_owned(),
                        matcher: None,
                        handler_type: "command".to_owned(),
                        command: Some("check".to_owned()),
                        command_windows: None,
                        timeout_seconds: Some(5),
                        status_message: None,
                    }],
                },
            )
            .unwrap();
        assert_eq!(updated.source.revision, 2);
        assert_eq!(updated.handlers.len(), 1);
        assert!(application
            .update_hook_source(
                DesktopHookScope::Project,
                project.to_str(),
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: Vec::new(),
                },
            )
            .is_err());

        let enabled = application
            .set_hook_source_enabled(DesktopHookScope::Project, project.to_str(), 2, true)
            .unwrap();
        assert_eq!(enabled.revision, 3);
        assert!(enabled.enabled);
        application
            .delete_hook_source(DesktopHookScope::Project, project.to_str(), 3)
            .unwrap();
        assert!(!Path::new(&enabled.path).exists());
    }

    #[test]
    fn enabled_hook_executes_once_per_turn_and_completed_fence_skips_replay() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let project = directory.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let marker = project.join("hook-ran.txt");
        let application = application(&home);
        application
            .create_hook_source(DesktopHookScope::User, None)
            .unwrap();
        #[cfg(windows)]
        let command = "echo ran>>hook-ran.txt".to_owned();
        #[cfg(not(windows))]
        let command = format!("printf 'ran\\n' >> '{}'", marker.display());
        application
            .update_hook_source(
                DesktopHookScope::User,
                None,
                DesktopHookDocumentUpdate {
                    expected_revision: 1,
                    handlers: vec![DesktopHookHandlerUpdate {
                        id: Some("once".to_owned()),
                        event: "UserPromptSubmit".to_owned(),
                        matcher: Some("*ship*".to_owned()),
                        handler_type: "command".to_owned(),
                        command: Some(command.clone()),
                        command_windows: Some(command),
                        timeout_seconds: Some(5),
                        status_message: None,
                    }],
                },
            )
            .unwrap();
        application
            .set_hook_source_enabled(DesktopHookScope::User, None, 2, true)
            .unwrap();
        let task_id = TaskId::new("hook-task").unwrap();
        application
            .execute_turn_hooks(
                DesktopHookEvent::UserPromptSubmit,
                &task_id,
                "hook-turn",
                project.to_str(),
                "please ship this",
            )
            .unwrap();
        application
            .execute_turn_hooks(
                DesktopHookEvent::UserPromptSubmit,
                &task_id,
                "hook-turn",
                project.to_str(),
                "please ship this",
            )
            .unwrap();
        assert_eq!(fs::read_to_string(marker).unwrap().lines().count(), 1);
    }

    #[test]
    fn started_execution_fence_refuses_uncertain_replay() {
        let directory = tempfile::tempdir().unwrap();
        let application = application(directory.path());
        let store = &application.inner.hook_executions;
        assert!(matches!(
            store
                .begin(
                    "uncertain-turn",
                    DesktopHookEvent::UserPromptSubmit,
                    USER_SOURCE_ID,
                    "side-effect",
                    "fingerprint",
                )
                .unwrap(),
            HookExecutionDecision::Execute
        ));
        assert!(matches!(
            store
                .begin(
                    "uncertain-turn",
                    DesktopHookEvent::UserPromptSubmit,
                    USER_SOURCE_ID,
                    "side-effect",
                    "fingerprint",
                )
                .unwrap(),
            HookExecutionDecision::Indeterminate
        ));
    }
}
