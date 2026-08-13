use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::{repo_root, Result, XtaskError};

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitPin {
    source: String,
    revision: String,
    location: String,
}

pub fn check() -> Result {
    let root = repo_root()?;
    let pins = collect_manifest_pins(&root)?;
    validate_pins(&pins)?;
    validate_lockfile(&root.join("Cargo.lock"), &pins)?;
    println!("pin-check: ok ({} immutable git dependencies)", pins.len());
    Ok(())
}

fn collect_manifest_pins(root: &Path) -> Result<Vec<GitPin>> {
    let mut pins = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "target" && entry.file_name() != ".git")
    {
        let entry =
            entry.map_err(|error| XtaskError::failure("pin_walk_failed", error.to_string()))?;
        if entry.file_name() != "Cargo.toml" {
            continue;
        }
        let source = fs::read_to_string(entry.path()).map_err(|error| {
            XtaskError::io("manifest_read_failed", "read Cargo manifest", error)
        })?;
        let document: toml::Value = toml::from_str(&source)
            .map_err(|error| XtaskError::failure("manifest_parse_failed", error.to_string()))?;
        collect_value_pins(&document, entry.path(), &mut pins);
    }
    Ok(pins)
}

fn collect_value_pins(value: &toml::Value, location: &Path, pins: &mut Vec<GitPin>) {
    match value {
        toml::Value::Table(table) => {
            if let Some(source) = table.get("git").and_then(toml::Value::as_str) {
                pins.push(GitPin {
                    source: source.trim_end_matches('/').to_owned(),
                    revision: table
                        .get("rev")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    location: location.display().to_string(),
                });
            }
            for child in table.values() {
                collect_value_pins(child, location, pins);
            }
        }
        toml::Value::Array(values) => {
            for child in values {
                collect_value_pins(child, location, pins);
            }
        }
        _ => {}
    }
}

fn validate_pins(pins: &[GitPin]) -> Result {
    let mut revisions = BTreeMap::<&str, &str>::new();
    for pin in pins {
        if pin.revision.len() != 40 || !pin.revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(XtaskError::failure(
                "git_dependency_not_immutable",
                format!(
                    "{} pins {} without a full 40-character revision",
                    pin.location, pin.source
                ),
            ));
        }
        if let Some(previous) = revisions.insert(&pin.source, &pin.revision) {
            if previous != pin.revision {
                return Err(XtaskError::failure(
                    "git_dependency_revision_mismatch",
                    format!("{} uses both {previous} and {}", pin.source, pin.revision),
                ));
            }
        }
    }
    Ok(())
}

fn validate_lockfile(path: &Path, pins: &[GitPin]) -> Result {
    let lock = fs::read_to_string(path)
        .map_err(|error| XtaskError::io("lockfile_read_failed", "read Cargo.lock", error))?;
    for pin in pins {
        if !lock.contains(&format!("#{}", pin.revision)) {
            return Err(XtaskError::failure(
                "git_pin_missing_from_lockfile",
                format!("{}#{} is absent from Cargo.lock", pin.source, pin.revision),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_floating_and_conflicting_git_dependencies() {
        let floating = GitPin {
            source: "https://example.test/repo".into(),
            revision: "main".into(),
            location: "Cargo.toml".into(),
        };
        assert_eq!(
            validate_pins(&[floating]).unwrap_err().code,
            "git_dependency_not_immutable"
        );
        let one = "a".repeat(40);
        let two = "b".repeat(40);
        let pins = [
            GitPin {
                source: "https://example.test/repo".into(),
                revision: one,
                location: "a".into(),
            },
            GitPin {
                source: "https://example.test/repo".into(),
                revision: two,
                location: "b".into(),
            },
        ];
        assert_eq!(
            validate_pins(&pins).unwrap_err().code,
            "git_dependency_revision_mismatch"
        );
    }
}
