//! Shared Lilia data-path assembly for Desktop / Service / CLI (#56 / #47).
//!
//! All hosts that open the product projection DB must resolve through this
//! module so Embedded Desktop and Service authority share one file layout.

use std::env;
use std::path::{Path, PathBuf};

/// Environment override for the product data home (same as Desktop `LILIA_HOME`).
pub const LILIA_HOME_ENV: &str = "LILIA_HOME";

pub const PRODUCT_PROJECTIONS_DB_FILE: &str = "product_projections.db";
pub const PRODUCT_DB_FILE: &str = "product.db";
pub const AGENT_RUNTIME_DB_FILE: &str = "agent_runtime.db";
pub const LEGACY_DESKTOP_DB_FILE: &str = "lilia.db";

/// Canonical on-disk layout under a Lilia home directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiliaDataPaths {
    home: PathBuf,
}

impl LiliaDataPaths {
    pub fn from_home(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// Resolve home from `LILIA_HOME`, else `~/.lilia`, else `./.lilia`.
    pub fn resolve() -> Self {
        if let Ok(env_val) = env::var(LILIA_HOME_ENV) {
            let trimmed = env_val.trim();
            if !trimmed.is_empty() {
                return Self::from_home(PathBuf::from(trimmed));
            }
        }
        let home = dirs_next_home()
            .map(|d| d.join(".lilia"))
            .unwrap_or_else(|| PathBuf::from(".lilia"));
        Self::from_home(home)
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn db_dir(&self) -> PathBuf {
        self.home.join("db")
    }

    /// Product Agent-event projection store (timeline / todo / artifact / pending).
    pub fn product_projections_db(&self) -> PathBuf {
        self.db_dir().join(PRODUCT_PROJECTIONS_DB_FILE)
    }

    /// Product domain store (Project / Task / Binding / migration ledger).
    pub fn product_db(&self) -> PathBuf {
        self.db_dir().join(PRODUCT_DB_FILE)
    }

    /// Native AgentKit execution facts (session/Wire/recovery state).
    pub fn agent_runtime_db(&self) -> PathBuf {
        self.db_dir().join(AGENT_RUNTIME_DB_FILE)
    }

    /// Legacy Desktop SQLite (`apps/desktop` historical fact source).
    pub fn legacy_desktop_db(&self) -> PathBuf {
        self.db_dir().join(LEGACY_DESKTOP_DB_FILE)
    }

    pub fn migration_backup_dir(&self) -> PathBuf {
        self.db_dir().join("migration_backups")
    }

    pub fn ensure_layout(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.home())?;
        for sub in ["config", "db", "cache"] {
            std::fs::create_dir_all(self.home.join(sub))?;
        }
        std::fs::create_dir_all(self.migration_backup_dir())?;
        Ok(())
    }
}

fn dirs_next_home() -> Option<PathBuf> {
    // Avoid a new workspace dep: mirror Desktop's `dirs::home_dir` via env only.
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_and_service_resolve_same_projection_path_under_home() {
        let home = std::env::temp_dir().join("lilia-paths-shared-home");
        let desktop = LiliaDataPaths::from_home(&home);
        let service = LiliaDataPaths::from_home(&home);
        assert_eq!(
            desktop.product_projections_db(),
            service.product_projections_db()
        );
        assert_eq!(
            desktop.product_projections_db(),
            home.join("db").join(PRODUCT_PROJECTIONS_DB_FILE)
        );
        assert_eq!(desktop.product_db(), home.join("db").join(PRODUCT_DB_FILE));
        assert_eq!(
            desktop.agent_runtime_db(),
            home.join("db").join(AGENT_RUNTIME_DB_FILE)
        );
        assert_eq!(
            desktop.legacy_desktop_db(),
            home.join("db").join(LEGACY_DESKTOP_DB_FILE)
        );
    }

    #[test]
    fn env_override_wins() {
        let previous = env::var_os(LILIA_HOME_ENV);
        let custom = std::env::temp_dir().join("lilia-paths-env-home");
        env::set_var(LILIA_HOME_ENV, &custom);
        let paths = LiliaDataPaths::resolve();
        assert_eq!(paths.home(), custom.as_path());
        match previous {
            Some(value) => env::set_var(LILIA_HOME_ENV, value),
            None => env::remove_var(LILIA_HOME_ENV),
        }
    }

    /// #46 / #56 — legacy `lilia.db` is migration/cache only; product facts live elsewhere.
    #[test]
    fn legacy_desktop_db_is_not_product_fact_store() {
        let home = std::env::temp_dir().join("lilia-paths-legacy-isolation");
        let paths = LiliaDataPaths::from_home(&home);
        let legacy = paths.legacy_desktop_db();
        let projections = paths.product_projections_db();
        let product = paths.product_db();
        assert_ne!(legacy, projections);
        assert_ne!(legacy, product);
        assert_eq!(
            legacy.file_name().and_then(|n| n.to_str()),
            Some(LEGACY_DESKTOP_DB_FILE)
        );
        assert_eq!(
            projections.file_name().and_then(|n| n.to_str()),
            Some(PRODUCT_PROJECTIONS_DB_FILE)
        );
        assert_eq!(
            product.file_name().and_then(|n| n.to_str()),
            Some(PRODUCT_DB_FILE)
        );
    }
}
