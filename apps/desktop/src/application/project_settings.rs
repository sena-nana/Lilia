use std::path::PathBuf;

use lilia_storage::SqliteAgentRuntimeStateStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::application::ProjectSettingsChanged;
use crate::application::{DesktopApplication, DesktopApplicationError};

pub const PROJECT_SETTINGS_KEY: &str = "desktop.project.settings.v1";
const PROJECT_SETTINGS_SCHEMA_VERSION: u32 = 1;

const DEFAULT_WORKTREE_AUTO_INSTRUCTIONS: &str = concat!(
    "This task is running inside a dedicated git worktree managed by Lilia.\n",
    "Keep changes scoped to this task and create commits in the worktree before requesting merge/archive."
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopWorktreeSelectionMode {
    #[default]
    Current,
    Create,
    Existing,
}

impl DesktopWorktreeSelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Create => "create",
            Self::Existing => "existing",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "create" => Self::Create,
            "existing" => Self::Existing,
            _ => Self::Current,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorktreeSettings {
    pub default_mode: DesktopWorktreeSelectionMode,
    pub parent_dir: Option<String>,
    pub auto_instructions: String,
    pub cleanup_on_archive: bool,
}

impl Default for DesktopWorktreeSettings {
    fn default() -> Self {
        Self {
            default_mode: DesktopWorktreeSelectionMode::Current,
            parent_dir: None,
            auto_instructions: DEFAULT_WORKTREE_AUTO_INSTRUCTIONS.to_owned(),
            cleanup_on_archive: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectSettings {
    pub clone_parent_dir: Option<String>,
    #[serde(default)]
    pub worktree: DesktopWorktreeSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProjectSettings {
    schema_version: u32,
    settings: DesktopProjectSettings,
}

#[derive(Debug, Error)]
pub enum DesktopProjectSettingsError {
    #[error("project settings persistence failed: {0}")]
    Persistence(String),
    #[error("project settings payload is corrupt: {0}")]
    Corrupt(String),
    #[error("unsupported project settings schema {0}")]
    UnsupportedSchema(u32),
}

impl From<DesktopProjectSettingsError> for DesktopApplicationError {
    fn from(error: DesktopProjectSettingsError) -> Self {
        match error {
            DesktopProjectSettingsError::Persistence(message)
            | DesktopProjectSettingsError::Corrupt(message) => Self::InvalidInput {
                field: "project_settings",
                message,
            },
            DesktopProjectSettingsError::UnsupportedSchema(version) => Self::InvalidInput {
                field: "project_settings",
                message: format!("unsupported schema {version}"),
            },
        }
    }
}

impl DesktopApplication {
    pub fn project_settings(&self) -> Result<DesktopProjectSettings, DesktopApplicationError> {
        Ok(self.load_project_settings()?)
    }

    pub fn save_project_settings(
        &self,
        settings: DesktopProjectSettings,
    ) -> Result<DesktopProjectSettings, DesktopApplicationError> {
        let normalized = normalize_project_settings(settings);
        self.persist_project_settings(&normalized)?;
        self.emit_event(ProjectSettingsChanged);
        Ok(normalized)
    }

    pub fn worktree_parent_directory_preference(
        &self,
    ) -> Result<Option<PathBuf>, DesktopApplicationError> {
        Ok(self
            .project_settings()?
            .worktree
            .parent_dir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from))
    }

    pub fn worktree_auto_instructions_for_task(
        &self,
        task_id: &lilia_contracts::TaskId,
    ) -> Result<Option<String>, DesktopApplicationError> {
        if self.task_worktree(task_id)?.is_none() {
            return Ok(None);
        }
        let text = self.project_settings()?.worktree.auto_instructions;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_owned()))
        }
    }

    fn load_project_settings(&self) -> Result<DesktopProjectSettings, DesktopProjectSettingsError> {
        let value = self
            .project_settings_store()?
            .setting(PROJECT_SETTINGS_KEY)
            .map_err(|error| DesktopProjectSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopProjectSettings::default());
        };
        let stored = serde_json::from_value::<StoredProjectSettings>(value)
            .map_err(|error| DesktopProjectSettingsError::Corrupt(error.to_string()))?;
        if stored.schema_version != PROJECT_SETTINGS_SCHEMA_VERSION {
            return Err(DesktopProjectSettingsError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        Ok(normalize_project_settings(stored.settings))
    }

    fn persist_project_settings(
        &self,
        settings: &DesktopProjectSettings,
    ) -> Result<(), DesktopProjectSettingsError> {
        let stored = StoredProjectSettings {
            schema_version: PROJECT_SETTINGS_SCHEMA_VERSION,
            settings: settings.clone(),
        };
        let value = serde_json::to_value(stored)
            .map_err(|error| DesktopProjectSettingsError::Persistence(error.to_string()))?;
        self.project_settings_store()?
            .put_setting(PROJECT_SETTINGS_KEY, &value)
            .map_err(|error| DesktopProjectSettingsError::Persistence(error.to_string()))
    }

    fn project_settings_store(
        &self,
    ) -> Result<SqliteAgentRuntimeStateStore, DesktopProjectSettingsError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopProjectSettingsError::Persistence(error.to_string()))?;
        SqliteAgentRuntimeStateStore::open(self.config().data_paths().agent_runtime_db())
            .map_err(|error| DesktopProjectSettingsError::Persistence(error.to_string()))
    }
}

pub fn normalize_project_settings(settings: DesktopProjectSettings) -> DesktopProjectSettings {
    DesktopProjectSettings {
        clone_parent_dir: normalize_optional_path(settings.clone_parent_dir),
        worktree: DesktopWorktreeSettings {
            default_mode: settings.worktree.default_mode,
            parent_dir: normalize_optional_path(settings.worktree.parent_dir),
            auto_instructions: {
                let trimmed = settings.worktree.auto_instructions.trim();
                if trimmed.is_empty() {
                    DEFAULT_WORKTREE_AUTO_INSTRUCTIONS.to_owned()
                } else {
                    trimmed.to_owned()
                }
            },
            cleanup_on_archive: settings.worktree.cleanup_on_archive,
        },
    }
}

pub fn default_worktree_auto_instructions() -> &'static str {
    DEFAULT_WORKTREE_AUTO_INSTRUCTIONS
}

fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };
    use lilia_service::ServiceAuthority;

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

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

    fn application() -> DesktopApplication {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lilia-project-settings-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config =
            DesktopApplicationConfig::new(&root, format!("project-settings-{id}")).unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:project-settings:{id}"),
            format!("project-settings-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(config, authority, Arc::new(NoopHost)).unwrap()
    }

    #[test]
    fn project_settings_default_round_trip_and_normalize() {
        let app = application();
        let defaults = app.project_settings().unwrap();
        assert_eq!(defaults, DesktopProjectSettings::default());
        assert!(defaults.worktree.cleanup_on_archive);
        assert_eq!(
            defaults.worktree.auto_instructions,
            DEFAULT_WORKTREE_AUTO_INSTRUCTIONS
        );

        let saved = app
            .save_project_settings(DesktopProjectSettings {
                clone_parent_dir: Some("  /tmp/clones  ".into()),
                worktree: DesktopWorktreeSettings {
                    default_mode: DesktopWorktreeSelectionMode::Create,
                    parent_dir: Some("  /tmp/worktrees  ".into()),
                    auto_instructions: "  stay scoped  ".into(),
                    cleanup_on_archive: false,
                },
            })
            .unwrap();
        assert_eq!(saved.clone_parent_dir.as_deref(), Some("/tmp/clones"));
        assert_eq!(saved.worktree.parent_dir.as_deref(), Some("/tmp/worktrees"));
        assert_eq!(saved.worktree.auto_instructions, "stay scoped");
        assert!(!saved.worktree.cleanup_on_archive);
        assert_eq!(
            saved.worktree.default_mode,
            DesktopWorktreeSelectionMode::Create
        );
        assert_eq!(app.project_settings().unwrap(), saved);
        assert_eq!(
            app.worktree_parent_directory_preference().unwrap(),
            Some(PathBuf::from("/tmp/worktrees"))
        );
    }

    #[test]
    fn empty_auto_instructions_restore_default_text() {
        let app = application();
        let saved = app
            .save_project_settings(DesktopProjectSettings {
                clone_parent_dir: None,
                worktree: DesktopWorktreeSettings {
                    auto_instructions: "   ".into(),
                    ..DesktopWorktreeSettings::default()
                },
            })
            .unwrap();
        assert_eq!(
            saved.worktree.auto_instructions,
            DEFAULT_WORKTREE_AUTO_INSTRUCTIONS
        );
    }
}
