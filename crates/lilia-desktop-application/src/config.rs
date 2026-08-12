use std::path::{Path, PathBuf};

use lilia_storage::LiliaDataPaths;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesktopDomainDatabase {
    #[default]
    Product,
    LegacyDesktop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopApplicationConfig {
    home: PathBuf,
    instance_identity: String,
    domain_database: DesktopDomainDatabase,
}

impl DesktopApplicationConfig {
    pub fn new(
        home: impl Into<PathBuf>,
        instance_identity: impl Into<String>,
    ) -> Result<Self, DesktopApplicationConfigError> {
        let home = home.into();
        if home.as_os_str().is_empty() {
            return Err(DesktopApplicationConfigError::EmptyHome);
        }

        let instance_identity = instance_identity.into();
        let instance_identity = instance_identity.trim();
        if instance_identity.is_empty() {
            return Err(DesktopApplicationConfigError::EmptyInstanceIdentity);
        }
        if instance_identity.chars().any(char::is_control) {
            return Err(DesktopApplicationConfigError::InvalidInstanceIdentity);
        }

        Ok(Self {
            home,
            instance_identity: instance_identity.to_owned(),
            domain_database: DesktopDomainDatabase::Product,
        })
    }

    pub fn with_legacy_desktop_domain_database(mut self) -> Self {
        self.domain_database = DesktopDomainDatabase::LegacyDesktop;
        self
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn instance_identity(&self) -> &str {
        &self.instance_identity
    }

    pub fn data_paths(&self) -> LiliaDataPaths {
        LiliaDataPaths::from_home(self.home.clone())
    }

    pub fn domain_database(&self) -> DesktopDomainDatabase {
        self.domain_database
    }

    pub fn domain_database_path(&self) -> PathBuf {
        let paths = self.data_paths();
        match self.domain_database {
            DesktopDomainDatabase::Product => paths.product_db(),
            DesktopDomainDatabase::LegacyDesktop => paths.legacy_desktop_db(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopApplicationConfigError {
    #[error("desktop application home must not be empty")]
    EmptyHome,
    #[error("desktop application instance identity must not be empty")]
    EmptyInstanceIdentity,
    #[error("desktop application instance identity must not contain control characters")]
    InvalidInstanceIdentity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_keeps_preview_identity_and_storage_layout_independent() {
        let stable = DesktopApplicationConfig::new("C:/lilia/stable", "liliacode").unwrap();
        let preview =
            DesktopApplicationConfig::new("C:/lilia/native-preview", "liliacode.native-preview")
                .unwrap();

        assert_ne!(stable.home(), preview.home());
        assert_ne!(stable.instance_identity(), preview.instance_identity());
        assert_eq!(preview.data_paths().home(), preview.home());
        assert_eq!(preview.domain_database(), DesktopDomainDatabase::Product);
        assert_eq!(
            preview.domain_database_path(),
            preview.data_paths().product_db()
        );
        assert_eq!(
            stable
                .clone()
                .with_legacy_desktop_domain_database()
                .domain_database_path(),
            stable.data_paths().legacy_desktop_db()
        );
    }

    #[test]
    fn config_rejects_ambiguous_identity_or_home() {
        assert_eq!(
            DesktopApplicationConfig::new("", "liliacode").unwrap_err(),
            DesktopApplicationConfigError::EmptyHome
        );
        assert_eq!(
            DesktopApplicationConfig::new("C:/lilia", "  ").unwrap_err(),
            DesktopApplicationConfigError::EmptyInstanceIdentity
        );
    }
}
