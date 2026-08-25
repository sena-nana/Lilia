use lilia_storage::SqliteAgentRuntimeStateStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::application::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

pub const POPUP_WINDOW_SETTINGS_KEY: &str = "desktop.popup-window.settings.v1";
pub const POPUP_LAST_PROJECT_KEY: &str = "desktop.popup-window.last-project.v1";
const POPUP_WINDOW_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPopupWindowSettings {
    pub shortcut: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPopupWindowSettings {
    schema_version: u32,
    settings: DesktopPopupWindowSettings,
}

#[derive(Debug, Error)]
pub enum DesktopPopupSettingsError {
    #[error("popup settings persistence failed: {0}")]
    Persistence(String),
    #[error("popup settings payload is corrupt: {0}")]
    Corrupt(String),
    #[error("unsupported popup settings schema {0}")]
    UnsupportedSchema(u32),
}

impl From<DesktopPopupSettingsError> for DesktopApplicationError {
    fn from(error: DesktopPopupSettingsError) -> Self {
        match error {
            DesktopPopupSettingsError::Persistence(message)
            | DesktopPopupSettingsError::Corrupt(message) => Self::InvalidInput {
                field: "popup_settings",
                message,
            },
            DesktopPopupSettingsError::UnsupportedSchema(version) => Self::InvalidInput {
                field: "popup_settings",
                message: format!("unsupported schema {version}"),
            },
        }
    }
}

impl DesktopApplication {
    pub fn popup_window_settings(
        &self,
    ) -> Result<DesktopPopupWindowSettings, DesktopApplicationError> {
        Ok(self.load_popup_window_settings()?)
    }

    pub fn save_popup_window_settings(
        &self,
        settings: DesktopPopupWindowSettings,
    ) -> Result<DesktopPopupWindowSettings, DesktopApplicationError> {
        let normalized = normalize_popup_window_settings(settings);
        self.persist_popup_window_settings(&normalized)?;
        self.emit_event(DesktopEventKind::PopupWindowSettingsChanged);
        Ok(normalized)
    }

    pub fn popup_last_project_id(&self) -> Result<Option<String>, DesktopApplicationError> {
        Ok(self.load_popup_last_project_id()?)
    }

    pub fn remember_popup_last_project(
        &self,
        project_id: impl Into<String>,
    ) -> Result<(), DesktopApplicationError> {
        let project_id = project_id.into();
        let trimmed = project_id.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let Ok(project_id) = lilia_contracts::ProjectId::new(trimmed) else {
            return Ok(());
        };
        if self.get_project(&project_id).is_err() {
            return Ok(());
        }
        self.persist_popup_last_project_id(trimmed)?;
        Ok(())
    }

    fn load_popup_window_settings(
        &self,
    ) -> Result<DesktopPopupWindowSettings, DesktopPopupSettingsError> {
        let value = self
            .popup_settings_store()?
            .setting(POPUP_WINDOW_SETTINGS_KEY)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(DesktopPopupWindowSettings::default());
        };
        let stored = serde_json::from_value::<StoredPopupWindowSettings>(value)
            .map_err(|error| DesktopPopupSettingsError::Corrupt(error.to_string()))?;
        if stored.schema_version != POPUP_WINDOW_SETTINGS_SCHEMA_VERSION {
            return Err(DesktopPopupSettingsError::UnsupportedSchema(
                stored.schema_version,
            ));
        }
        Ok(normalize_popup_window_settings(stored.settings))
    }

    fn persist_popup_window_settings(
        &self,
        settings: &DesktopPopupWindowSettings,
    ) -> Result<(), DesktopPopupSettingsError> {
        let stored = StoredPopupWindowSettings {
            schema_version: POPUP_WINDOW_SETTINGS_SCHEMA_VERSION,
            settings: settings.clone(),
        };
        let value = serde_json::to_value(stored)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))?;
        self.popup_settings_store()?
            .put_setting(POPUP_WINDOW_SETTINGS_KEY, &value)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))
    }

    fn load_popup_last_project_id(&self) -> Result<Option<String>, DesktopPopupSettingsError> {
        let value = self
            .popup_settings_store()?
            .setting(POPUP_LAST_PROJECT_KEY)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(None);
        };
        let project_id = serde_json::from_value::<String>(value)
            .map_err(|error| DesktopPopupSettingsError::Corrupt(error.to_string()))?;
        let trimmed = project_id.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_owned()))
        }
    }

    fn persist_popup_last_project_id(
        &self,
        project_id: &str,
    ) -> Result<(), DesktopPopupSettingsError> {
        let value = serde_json::to_value(project_id)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))?;
        self.popup_settings_store()?
            .put_setting(POPUP_LAST_PROJECT_KEY, &value)
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))
    }

    fn popup_settings_store(
        &self,
    ) -> Result<SqliteAgentRuntimeStateStore, DesktopPopupSettingsError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))?;
        SqliteAgentRuntimeStateStore::open(self.config().data_paths().agent_runtime_db())
            .map_err(|error| DesktopPopupSettingsError::Persistence(error.to_string()))
    }
}

pub fn normalize_popup_window_settings(
    settings: DesktopPopupWindowSettings,
) -> DesktopPopupWindowSettings {
    DesktopPopupWindowSettings {
        shortcut: settings
            .shortcut
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult, DesktopProjectCreate,
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
        let root = std::env::temp_dir().join(format!("lilia-popup-settings-{id}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let config = DesktopApplicationConfig::new(&root, format!("popup-settings-{id}")).unwrap();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:popup-settings:{id}"),
            format!("popup-settings-test:{id}"),
        )
        .unwrap();
        DesktopApplication::from_authority(config, authority, Arc::new(NoopHost)).unwrap()
    }

    #[test]
    fn popup_window_settings_and_last_project_persist() {
        let app = application();
        assert_eq!(
            app.popup_window_settings().unwrap(),
            DesktopPopupWindowSettings::default()
        );
        let saved = app
            .save_popup_window_settings(DesktopPopupWindowSettings {
                shortcut: Some("  Ctrl+Shift+L  ".into()),
            })
            .unwrap();
        assert_eq!(saved.shortcut.as_deref(), Some("Ctrl+Shift+L"));
        assert_eq!(app.popup_window_settings().unwrap(), saved);

        let project = app
            .create_project(DesktopProjectCreate::new("Popup"))
            .unwrap();
        app.remember_popup_last_project(project.id.as_str())
            .unwrap();
        assert_eq!(
            app.popup_last_project_id().unwrap().as_deref(),
            Some(project.id.as_str())
        );
        app.remember_popup_last_project("missing-project").unwrap();
        assert_eq!(
            app.popup_last_project_id().unwrap().as_deref(),
            Some(project.id.as_str())
        );
    }
}
