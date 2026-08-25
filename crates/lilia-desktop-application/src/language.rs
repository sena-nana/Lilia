use std::path::Path;

use crate::{DesktopApplication, DesktopApplicationError};

pub use lilia_feature_document::{
    LanguageDefinition, LanguageId, LanguageRegistry, LanguageRegistryError,
};

impl DesktopApplication {
    pub fn register_language(
        &self,
        definition: LanguageDefinition,
    ) -> Result<(), DesktopApplicationError> {
        self.inner
            .languages
            .write()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language registry"))?
            .register(definition)?;
        Ok(())
    }

    pub fn language_for_path(
        &self,
        path: &Path,
    ) -> Result<Option<LanguageDefinition>, DesktopApplicationError> {
        Ok(self
            .inner
            .languages
            .read()
            .map_err(|_| DesktopApplicationError::StateUnavailable("language registry"))?
            .language_for_path(path)
            .cloned())
    }
}
