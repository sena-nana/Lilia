use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::{repo_root, Result, XtaskError};

const FORBIDDEN_DEPENDENCIES: &[&str] =
    &["tauri", "vue", "vite", "node", "napi", "wasm-bindgen-vue"];
const FORBIDDEN_EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs", "ts", "tsx", "vue"];
const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".codegraph",
    "target",
    "node_modules",
    "build",
    "dist",
    "agent-debug-runs",
];

pub fn check() -> Result {
    let root = repo_root()?;
    let violations = collect_violations(&root)?;
    if violations.is_empty() {
        println!("boundary-check: ok");
        Ok(())
    } else {
        Err(XtaskError::failure(
            "boundary_violation",
            violations.into_iter().collect::<Vec<_>>().join("\n"),
        ))
    }
}

fn collect_violations(root: &Path) -> Result<BTreeSet<String>> {
    let mut violations = BTreeSet::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        !entry.file_type().is_dir()
            || !IGNORED_DIRECTORIES
                .iter()
                .any(|name| entry.file_name() == *name)
    }) {
        let entry = entry
            .map_err(|error| XtaskError::failure("boundary_walk_failed", error.to_string()))?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(path);
        if entry.file_type().is_dir() {
            let normalized = relative.to_string_lossy().replace('\\', "/");
            if normalized == "apps/vscode-extension" || normalized.ends_with("/src-tauri") {
                violations.insert(format!("forbidden directory: {normalized}"));
            }
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| FORBIDDEN_EXTENSIONS.contains(&extension))
        {
            violations.insert(format!(
                "forbidden Node/frontend source: {}",
                relative.display()
            ));
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| {
                matches!(
                    name,
                    "package.json"
                        | "package-lock.json"
                        | "yarn.lock"
                        | ".yarnrc.yml"
                        | "pnpm-lock.yaml"
                )
            })
        {
            violations.insert(format!(
                "forbidden package-manager file: {}",
                relative.display()
            ));
        }
        if path.file_name().and_then(|value| value.to_str()) == Some("Cargo.toml") {
            inspect_manifest(path, relative, &mut violations)?;
        }
    }
    Ok(violations)
}

fn inspect_manifest(path: &Path, relative: &Path, violations: &mut BTreeSet<String>) -> Result {
    let source = fs::read_to_string(path).map_err(|error| {
        XtaskError::io(
            "manifest_read_failed",
            &relative.display().to_string(),
            error,
        )
    })?;
    let manifest: toml::Value = toml::from_str(&source).map_err(|error| {
        XtaskError::failure(
            "manifest_parse_failed",
            format!("{}: {error}", relative.display()),
        )
    })?;
    inspect_manifest_value(&manifest, relative, violations);
    Ok(())
}

fn inspect_manifest_value(value: &toml::Value, relative: &Path, violations: &mut BTreeSet<String>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (name, child) in table {
        if matches!(
            name.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            if let Some(dependencies) = child.as_table() {
                for dependency in dependencies.keys() {
                    if FORBIDDEN_DEPENDENCIES.iter().any(|forbidden| {
                        dependency == forbidden || dependency.starts_with(&format!("{forbidden}-"))
                    }) {
                        violations.insert(format!(
                            "forbidden dependency `{dependency}` in {}",
                            relative.display()
                        ));
                    }
                }
            }
        } else {
            inspect_manifest_value(child, relative, violations);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_frontend_sources_and_tauri_dependencies() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("package.json"), "{}").unwrap();
        fs::write(
            directory.path().join("Cargo.toml"),
            "[dependencies]\ntauri = \"2\"\n",
        )
        .unwrap();
        let violations = collect_violations(directory.path()).unwrap();
        assert!(violations
            .iter()
            .any(|value| value.contains("package-manager")));
        assert!(violations
            .iter()
            .any(|value| value.contains("forbidden dependency `tauri`")));
    }
}
