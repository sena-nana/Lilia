use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{DesktopApplication, DesktopApplicationError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageId(String);

impl LanguageId {
    pub fn new(value: impl Into<String>) -> Result<Self, LanguageRegistryError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(LanguageRegistryError::InvalidLanguageId);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDefinition {
    pub id: LanguageId,
    pub display_name: String,
    pub extensions: Vec<String>,
}

impl LanguageDefinition {
    pub fn new(
        id: LanguageId,
        display_name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, LanguageRegistryError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(LanguageRegistryError::EmptyDisplayName);
        }
        let mut normalized = Vec::new();
        for extension in extensions {
            let extension = normalize_extension(&extension.into())?;
            if !normalized.contains(&extension) {
                normalized.push(extension);
            }
        }
        Ok(Self {
            id,
            display_name,
            extensions: normalized,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct LanguageRegistry {
    languages: BTreeMap<LanguageId, LanguageDefinition>,
    by_extension: BTreeMap<String, LanguageId>,
}

impl LanguageRegistry {
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        let builtins = [
            ("plaintext", "Plain Text", &[] as &[&str]),
            ("markdown", "Markdown", &["md", "markdown", "mdx"]),
            ("rust", "Rust", &["rs"]),
            ("toml", "TOML", &["toml"]),
            ("json", "JSON", &["json", "jsonc"]),
            ("javascript", "JavaScript", &["js", "mjs", "cjs"]),
            ("typescript", "TypeScript", &["ts", "mts", "cts"]),
            ("tsx", "TSX", &["tsx"]),
            ("jsx", "JSX", &["jsx"]),
            ("python", "Python", &["py"]),
            ("css", "CSS", &["css"]),
            ("html", "HTML", &["html", "htm"]),
            ("yaml", "YAML", &["yaml", "yml"]),
            ("shell", "Shell", &["sh", "bash", "zsh"]),
        ];
        for (id, display_name, extensions) in builtins {
            let definition = LanguageDefinition::new(
                LanguageId::new(id).expect("builtin language id is valid"),
                display_name,
                extensions.iter().copied(),
            )
            .expect("builtin language definition is valid");
            registry
                .register(definition)
                .expect("builtin language registration is unique");
        }
        registry
    }

    pub fn register(
        &mut self,
        definition: LanguageDefinition,
    ) -> Result<(), LanguageRegistryError> {
        if self.languages.contains_key(&definition.id) {
            return Err(LanguageRegistryError::DuplicateLanguage(
                definition.id.as_str().to_owned(),
            ));
        }
        for extension in &definition.extensions {
            if let Some(owner) = self.by_extension.get(extension) {
                return Err(LanguageRegistryError::DuplicateExtension {
                    extension: extension.clone(),
                    owner: owner.as_str().to_owned(),
                });
            }
        }
        for extension in &definition.extensions {
            self.by_extension
                .insert(extension.clone(), definition.id.clone());
        }
        self.languages.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub fn get(&self, id: &LanguageId) -> Option<&LanguageDefinition> {
        self.languages.get(id)
    }

    pub fn language_for_path(&self, path: &Path) -> Option<&LanguageDefinition> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        let id = self.by_extension.get(&extension)?;
        self.languages.get(id)
    }
}

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

fn normalize_extension(value: &str) -> Result<String, LanguageRegistryError> {
    let extension = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if extension.is_empty()
        || extension
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err(LanguageRegistryError::InvalidExtension(value.to_owned()));
    }
    Ok(extension)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LanguageRegistryError {
    #[error("language identifier must not be empty or contain control characters")]
    InvalidLanguageId,
    #[error("language display name must not be empty")]
    EmptyDisplayName,
    #[error("invalid language extension `{0}`")]
    InvalidExtension(String),
    #[error("language `{0}` is already registered")]
    DuplicateLanguage(String),
    #[error("extension `{extension}` is already owned by language `{owner}`")]
    DuplicateExtension { extension: String, owner: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_case_insensitive_extensions_without_ambiguous_owners() {
        let mut registry = LanguageRegistry::default();
        registry
            .register(
                LanguageDefinition::new(LanguageId::new("rust").unwrap(), "Rust", ["rs"]).unwrap(),
            )
            .unwrap();
        assert_eq!(
            registry
                .language_for_path(Path::new("src/MAIN.RS"))
                .unwrap()
                .id
                .as_str(),
            "rust"
        );

        let conflict = registry.register(
            LanguageDefinition::new(
                LanguageId::new("other-rust").unwrap(),
                "Other Rust",
                [".RS"],
            )
            .unwrap(),
        );
        assert!(matches!(
            conflict,
            Err(LanguageRegistryError::DuplicateExtension { extension, .. }) if extension == "rs"
        ));
    }
}
