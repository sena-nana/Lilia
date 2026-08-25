use std::fs;
use std::path::{Path, PathBuf};

use lilia_storage::{AgentkitSkillPackageRef, AgentkitSkillsRegistry};

use crate::error::{invalid_input, ExtensionsError};
use crate::types::SkillScope;

pub fn ensure_skills_registry_revision(actual: u64, expected: u64) -> Result<(), ExtensionsError> {
    if actual == expected {
        return Ok(());
    }
    Err(invalid_input(
        "expected_registry_revision",
        format!("Skills registry changed: expected revision {expected}, actual {actual}"),
    ))
}

pub fn bump_skills_registry_revision(
    registry: &mut AgentkitSkillsRegistry,
) -> Result<(), ExtensionsError> {
    registry.revision = registry
        .revision
        .checked_add(1)
        .ok_or(ExtensionsError::StateRevisionOverflow("Skills registry"))?;
    Ok(())
}

pub fn normalized_skill_id(value: &str) -> Result<String, ExtensionsError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != "..";
    if !valid {
        return Err(invalid_input(
            "skill_id",
            "Skill ID must use 1-64 ASCII letters, digits, dots, dashes, or underscores",
        ));
    }
    Ok(value.to_owned())
}

pub fn normalized_skill_description(value: &str) -> Result<String, ExtensionsError> {
    let value = value.trim();
    if value.len() > 2_048 || value.chars().any(|character| character == '\0') {
        return Err(invalid_input(
            "description",
            "Skill description must be at most 2048 characters and contain no NUL",
        ));
    }
    Ok(value.to_owned())
}

pub fn managed_skill_root(
    home: &Path,
    scope: SkillScope,
    project_cwd: Option<&str>,
) -> Result<PathBuf, ExtensionsError> {
    match scope {
        SkillScope::User => Ok(home.join("skills")),
        SkillScope::Project => {
            let project_cwd = project_cwd
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    invalid_input("project_cwd", "project Skills require a workspace")
                })?;
            let path = PathBuf::from(project_cwd);
            if !path.is_absolute() || !path.is_dir() {
                return Err(invalid_input(
                    "project_cwd",
                    "project Skill workspace must be an existing absolute directory",
                ));
            }
            Ok(path.join(".lilia").join("skills"))
        }
    }
}

pub fn write_skill_document(
    package_path: &Path,
    skill_id: &str,
    description: &str,
) -> Result<(), ExtensionsError> {
    use std::io::Write;

    let quoted_id =
        serde_json::to_string(skill_id).map_err(|error| ExtensionsError::Agent(error.to_string()))?;
    let quoted_description = serde_json::to_string(description)
        .map_err(|error| ExtensionsError::Agent(error.to_string()))?;
    let instructions = if description.is_empty() {
        format!("Apply the `{skill_id}` skill to the requested task.")
    } else {
        description.to_owned()
    };
    let document = format!(
        "---\nid: {quoted_id}\nversion: \"0.1.0\"\ntitle: {quoted_id}\nsummary: {quoted_description}\n---\n\n{instructions}\n"
    );
    let path = package_path.join("SKILL.md");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| skill_io_error("create SKILL.md", error))?;
    file.write_all(document.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| skill_io_error("write SKILL.md", error))
}

pub fn is_managed_skill(package: &AgentkitSkillPackageRef) -> bool {
    package.registered_from == "lilia.desktop.skill-manager" && package.scope == "user"
}

pub fn verified_managed_skill_path(
    root: &Path,
    package_path: &str,
    skill_id: &str,
) -> Result<PathBuf, ExtensionsError> {
    let expected = root.join(skill_id);
    let expected = expected
        .canonicalize()
        .map_err(|error| skill_io_error("resolve managed Skill directory", error))?;
    let registered = PathBuf::from(package_path)
        .canonicalize()
        .map_err(|error| skill_io_error("resolve registered Skill directory", error))?;
    let root = root
        .canonicalize()
        .map_err(|error| skill_io_error("resolve managed Skill root", error))?;
    if registered != expected || registered.parent() != Some(root.as_path()) {
        return Err(invalid_input(
            "skill_id",
            "registered Skill path is outside the managed Skill root",
        ));
    }
    Ok(registered)
}

pub fn skill_io_error(action: &str, error: impl std::fmt::Display) -> ExtensionsError {
    ExtensionsError::Agent(format!("{action}: {error}"))
}
