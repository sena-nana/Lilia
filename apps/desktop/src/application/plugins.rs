use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use lilia_storage::{
    AgentkitHooksDocument, AgentkitMcpRegistryEntry, AgentkitPluginPackageRef,
    AgentkitPluginsRegistry, LiliaPluginManifest,
};
use sha2::{Digest, Sha256};

use crate::application::{DesktopApplication, DesktopApplicationError};

const PLUGIN_MANAGER_PROVENANCE: &str = "lilia.desktop.plugin-manager";
const MAX_PLUGIN_FILES: usize = 2_048;
const MAX_PLUGIN_BYTES: u64 = 64 * 1024 * 1024;

pub use lilia_feature_extensions::PluginPackageView as DesktopPluginPackageView;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopPluginInstall {
    pub expected_registry_revision: u64,
    pub source_path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedPluginPackage {
    pub root: PathBuf,
    pub skill_paths: Vec<PathBuf>,
    pub hooks: Vec<(String, PathBuf, AgentkitHooksDocument)>,
    pub mcp_servers: Vec<AgentkitMcpRegistryEntry>,
}

impl DesktopApplication {
    pub fn plugin_packages(
        &self,
    ) -> Result<(u64, String, Vec<DesktopPluginPackageView>), DesktopApplicationError> {
        let paths = self.config().data_paths();
        let registry = lilia_storage::load_plugins_registry(&paths)?.unwrap_or_default();
        let packages = registry
            .packages
            .iter()
            .map(|package| plugin_package_view(&paths, package))
            .collect();
        Ok((
            registry.revision,
            lilia_storage::plugins_registry_path(&paths)
                .to_string_lossy()
                .into_owned(),
            packages,
        ))
    }

    pub fn install_plugin_package(
        &self,
        input: DesktopPluginInstall,
    ) -> Result<DesktopPluginPackageView, DesktopApplicationError> {
        let source = canonical_plugin_source(&input.source_path)?;
        let source_manifest = lilia_storage::load_plugin_manifest(&source)?;
        validate_plugin_contributions(&source, &source_manifest)?;
        let paths = self.config().data_paths();
        let plugins_root = lilia_storage::plugins_root_path(&paths);
        fs::create_dir_all(&plugins_root)
            .map_err(|error| plugin_io_error("create managed Plugins directory", error))?;
        let destination = plugins_root.join(&source_manifest.plugin_id);
        let staging = plugins_root.join(format!(
            ".{}.{}.tmp",
            source_manifest.plugin_id,
            uuid::Uuid::new_v4()
        ));

        let (package, previous) = {
            let _guard = self
                .inner
                .extension_registry
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
            let mut registry = lilia_storage::load_plugins_registry(&paths)?.unwrap_or_default();
            ensure_plugin_revision(registry.revision, input.expected_registry_revision)?;
            let previous = registry.clone();
            if registry
                .packages
                .iter()
                .any(|package| package.plugin_id == source_manifest.plugin_id)
                || destination.exists()
            {
                return Err(plugin_input_error(
                    "plugin_id",
                    format!(
                        "Plugin `{}` is already installed",
                        source_manifest.plugin_id
                    ),
                ));
            }
            copy_plugin_tree(&source, &staging)?;
            let staged_manifest = match lilia_storage::load_plugin_manifest(&staging)
                .map_err(DesktopApplicationError::from)
                .and_then(|manifest| {
                    validate_plugin_contributions(&staging, &manifest)?;
                    Ok(manifest)
                }) {
                Ok(manifest) => manifest,
                Err(error) => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(error);
                }
            };
            if staged_manifest != source_manifest {
                let _ = fs::remove_dir_all(&staging);
                return Err(plugin_input_error(
                    "source_path",
                    "Plugin source changed while it was being installed",
                ));
            }
            let package_sha256 = match plugin_package_digest(&staging) {
                Ok(digest) => digest,
                Err(error) => {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(error);
                }
            };
            fs::rename(&staging, &destination).map_err(|error| {
                let _ = fs::remove_dir_all(&staging);
                plugin_io_error("publish managed Plugin package", error)
            })?;
            let package = AgentkitPluginPackageRef {
                plugin_id: source_manifest.plugin_id,
                name: source_manifest.name,
                plugin_version: source_manifest.plugin_version,
                description: source_manifest.description,
                path: destination.to_string_lossy().into_owned(),
                package_sha256,
                registered_from: PLUGIN_MANAGER_PROVENANCE.to_owned(),
                enabled: false,
            };
            registry.packages.push(package.clone());
            registry
                .packages
                .sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
            bump_plugin_revision(&mut registry)?;
            if let Err(error) = lilia_storage::save_plugins_registry(&paths, &registry) {
                let _ = fs::remove_dir_all(&destination);
                return Err(error.into());
            }
            (package, previous)
        };
        if let Err(error) = self.reload_extension_contributions() {
            let _ = lilia_storage::save_plugins_registry(&paths, &previous);
            let _ = fs::remove_dir_all(&destination);
            let _ = self.reload_extension_contributions();
            return Err(error);
        }
        Ok(plugin_package_view(&paths, &package))
    }

    pub fn set_plugin_package_enabled(
        &self,
        plugin_id: &str,
        enabled: bool,
        expected_registry_revision: u64,
    ) -> Result<DesktopPluginPackageView, DesktopApplicationError> {
        let plugin_id = normalized_plugin_id(plugin_id)?;
        let paths = self.config().data_paths();
        let (package, previous, changed) = {
            let _guard = self
                .inner
                .extension_registry
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
            let mut registry = lilia_storage::load_plugins_registry(&paths)?.unwrap_or_default();
            ensure_plugin_revision(registry.revision, expected_registry_revision)?;
            let previous = registry.clone();
            let package = registry
                .packages
                .iter_mut()
                .find(|package| package.plugin_id == plugin_id)
                .ok_or_else(|| plugin_input_error("plugin_id", "Plugin is not installed"))?;
            if enabled {
                validate_managed_plugin(&paths, package)?;
            }
            if package.enabled != enabled {
                package.enabled = enabled;
                let updated = package.clone();
                bump_plugin_revision(&mut registry)?;
                lilia_storage::save_plugins_registry(&paths, &registry)?;
                (updated, previous, true)
            } else {
                (package.clone(), previous, false)
            }
        };
        if changed {
            if let Err(error) = self.reload_extension_contributions() {
                let _ = lilia_storage::save_plugins_registry(&paths, &previous);
                let _ = self.reload_extension_contributions();
                return Err(error);
            }
        }
        Ok(plugin_package_view(&paths, &package))
    }

    pub fn delete_plugin_package(
        &self,
        plugin_id: &str,
        expected_registry_revision: u64,
    ) -> Result<(), DesktopApplicationError> {
        let plugin_id = normalized_plugin_id(plugin_id)?;
        let paths = self.config().data_paths();
        let (backup, root, previous, mcp_servers) = {
            let _guard = self
                .inner
                .extension_registry
                .lock()
                .map_err(|_| DesktopApplicationError::StateUnavailable("extension registry"))?;
            let mut registry = lilia_storage::load_plugins_registry(&paths)?.unwrap_or_default();
            ensure_plugin_revision(registry.revision, expected_registry_revision)?;
            let previous = registry.clone();
            let index = registry
                .packages
                .iter()
                .position(|package| package.plugin_id == plugin_id)
                .ok_or_else(|| plugin_input_error("plugin_id", "Plugin is not installed"))?;
            let package = registry.packages[index].clone();
            if package.registered_from != PLUGIN_MANAGER_PROVENANCE {
                return Err(plugin_input_error(
                    "plugin_id",
                    "imported Plugin packages are read-only",
                ));
            }
            let loaded = validate_managed_plugin(&paths, &package)?;
            let root = loaded.root;
            registry.packages.remove(index);
            let backup = lilia_storage::plugins_root_path(&paths).join(format!(
                ".{}.{}.delete",
                plugin_id,
                uuid::Uuid::new_v4()
            ));
            if root.exists() {
                fs::rename(&root, &backup)
                    .map_err(|error| plugin_io_error("stage Plugin deletion", error))?;
            }
            bump_plugin_revision(&mut registry)?;
            if let Err(error) = lilia_storage::save_plugins_registry(&paths, &registry) {
                if backup.exists() && !root.exists() {
                    let _ = fs::rename(&backup, &root);
                }
                return Err(error.into());
            }
            (backup, root, previous, loaded.mcp_servers)
        };
        if let Err(error) = self.reload_extension_contributions() {
            let _ = lilia_storage::save_plugins_registry(&paths, &previous);
            if backup.exists() && !root.exists() {
                let _ = fs::rename(&backup, &root);
            }
            let _ = self.reload_extension_contributions();
            return Err(error);
        }
        if let Err(error) = self.delete_mcp_credentials_for_entries(&mcp_servers) {
            let _ = lilia_storage::save_plugins_registry(&paths, &previous);
            if backup.exists() && !root.exists() {
                let _ = fs::rename(&backup, &root);
            }
            let _ = self.reload_extension_contributions();
            return Err(error);
        }
        if backup.exists() {
            fs::remove_dir_all(&backup)
                .map_err(|error| plugin_io_error("remove deleted Plugin package", error))?;
        }
        Ok(())
    }

    pub(crate) fn loaded_plugin_packages(&self) -> Vec<LoadedPluginPackage> {
        let paths = self.config().data_paths();
        lilia_storage::load_plugins_registry(&paths)
            .ok()
            .flatten()
            .into_iter()
            .flat_map(|registry| registry.packages)
            .filter(|package| package.enabled)
            .filter_map(|package| validate_managed_plugin(&paths, &package).ok())
            .collect()
    }
}

fn plugin_package_view(
    paths: &lilia_storage::LiliaDataPaths,
    package: &AgentkitPluginPackageRef,
) -> DesktopPluginPackageView {
    match validate_managed_plugin(paths, package) {
        Ok(loaded) => DesktopPluginPackageView {
            plugin_id: package.plugin_id.clone(),
            name: package.name.clone(),
            version: package.plugin_version.clone(),
            description: package.description.clone(),
            path: package.path.clone(),
            enabled: package.enabled,
            editable: package.registered_from == PLUGIN_MANAGER_PROVENANCE,
            runtime_available: package.enabled,
            package_sha256: package.package_sha256.clone(),
            skill_count: loaded.skill_paths.len(),
            hook_count: loaded.hooks.len(),
            mcp_server_count: loaded.mcp_servers.len(),
            warnings: Vec::new(),
        },
        Err(error) => DesktopPluginPackageView {
            plugin_id: package.plugin_id.clone(),
            name: package.name.clone(),
            version: package.plugin_version.clone(),
            description: package.description.clone(),
            path: package.path.clone(),
            enabled: package.enabled,
            editable: package.registered_from == PLUGIN_MANAGER_PROVENANCE,
            runtime_available: false,
            package_sha256: package.package_sha256.clone(),
            skill_count: 0,
            hook_count: 0,
            mcp_server_count: 0,
            warnings: vec![error.to_string()],
        },
    }
}

fn validate_managed_plugin(
    paths: &lilia_storage::LiliaDataPaths,
    package: &AgentkitPluginPackageRef,
) -> Result<LoadedPluginPackage, DesktopApplicationError> {
    let plugins_root = lilia_storage::plugins_root_path(paths)
        .canonicalize()
        .map_err(|error| plugin_io_error("resolve managed Plugins directory", error))?;
    let root = PathBuf::from(&package.path)
        .canonicalize()
        .map_err(|error| plugin_io_error("resolve managed Plugin package", error))?;
    if !root.starts_with(&plugins_root) || root.parent() != Some(plugins_root.as_path()) {
        return Err(plugin_input_error(
            "path",
            "managed Plugin package escaped the Plugins directory",
        ));
    }
    let digest = plugin_package_digest(&root)?;
    if digest != package.package_sha256 {
        return Err(plugin_input_error(
            "package_sha256",
            format!("Plugin `{}` package contents changed", package.plugin_id),
        ));
    }
    let manifest = lilia_storage::load_plugin_manifest(&root)?;
    if manifest.plugin_id != package.plugin_id
        || manifest.name != package.name
        || manifest.plugin_version != package.plugin_version
        || manifest.description != package.description
    {
        return Err(plugin_input_error(
            "manifest",
            format!("Plugin `{}` manifest identity changed", package.plugin_id),
        ));
    }
    let (skill_paths, hooks, mcp_servers) = validate_plugin_contributions(&root, &manifest)?;
    Ok(LoadedPluginPackage {
        root,
        skill_paths,
        hooks,
        mcp_servers,
    })
}

type ValidatedContributions = (
    Vec<PathBuf>,
    Vec<(String, PathBuf, AgentkitHooksDocument)>,
    Vec<AgentkitMcpRegistryEntry>,
);

fn validate_plugin_contributions(
    root: &Path,
    manifest: &LiliaPluginManifest,
) -> Result<ValidatedContributions, DesktopApplicationError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| plugin_io_error("resolve Plugin package", error))?;
    let mut skill_paths = Vec::new();
    for relative in &manifest.contributions.skills {
        let path = contribution_path(&canonical_root, relative, true, "Skill")?;
        if !path.join("SKILL.md").is_file() {
            return Err(plugin_input_error(
                "skills",
                format!("Plugin Skill `{relative}` has no SKILL.md"),
            ));
        }
        skill_paths.push(path);
    }

    let mut hooks = Vec::new();
    for (index, relative) in manifest.contributions.hooks.iter().enumerate() {
        let path = contribution_path(&canonical_root, relative, false, "Hook")?;
        let document = lilia_storage::load_hooks_document(&path)?
            .ok_or_else(|| plugin_input_error("hooks", "Plugin Hook document is missing"))?;
        hooks.push((
            format!("native-agentkit:plugin:{}:{}", manifest.plugin_id, index),
            path,
            document,
        ));
    }

    let mut mcp_servers = Vec::new();
    let mut server_ids = BTreeSet::new();
    for relative in &manifest.contributions.mcp {
        let path = contribution_path(&canonical_root, relative, false, "MCP")?;
        let registry = lilia_storage::load_mcp_registry_file(&path)?;
        for mut server in registry.servers {
            if !server.env_allowlist.is_empty() {
                return Err(plugin_input_error(
                    "mcp",
                    "Plugin MCP contributions cannot embed environment values",
                ));
            }
            let local_id = normalized_plugin_id(&server.server_id)?;
            let server_id = format!("plugin.{}.{}", manifest.plugin_id, local_id);
            if !server_ids.insert(server_id.clone()) {
                return Err(plugin_input_error(
                    "mcp",
                    format!("Plugin declares duplicate MCP server `{local_id}`"),
                ));
            }
            match server.transport.as_str() {
                "stdio" => {
                    let command = server.command.as_deref().ok_or_else(|| {
                        plugin_input_error("mcp", "Plugin stdio MCP server requires a command")
                    })?;
                    let command =
                        contribution_path(&canonical_root, command, false, "MCP command")?;
                    server.command = Some(command.to_string_lossy().into_owned());
                    server.url = None;
                }
                "streamable_http" | "sse" => {
                    let url = server.url.as_deref().ok_or_else(|| {
                        plugin_input_error("mcp", "Plugin HTTP MCP server requires a URL")
                    })?;
                    // Same https-default + private/metadata SSRF policy as MCP upsert (#85 / M3).
                    lilia_feature_extensions::validate_mcp_url(url).map_err(|error| match error {
                        lilia_feature_extensions::ExtensionsError::InvalidInput {
                            message, ..
                        } => plugin_input_error("mcp", message),
                        other => plugin_input_error("mcp", other.to_string()),
                    })?;
                    server.command = None;
                }
                _ => {
                    return Err(plugin_input_error(
                        "mcp",
                        format!("Plugin MCP transport `{}` is unsupported", server.transport),
                    ));
                }
            }
            server.server_id = server_id;
            server.source = format!("plugin:{}", manifest.plugin_id);
            server.registered_from = format!("lilia.plugin:{}", manifest.plugin_id);
            mcp_servers.push(server);
        }
    }
    Ok((skill_paths, hooks, mcp_servers))
}

fn contribution_path(
    root: &Path,
    relative: &str,
    directory: bool,
    label: &str,
) -> Result<PathBuf, DesktopApplicationError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(plugin_input_error(
            "manifest",
            format!("Plugin {label} contribution escaped its package"),
        ));
    }
    let path = root
        .join(relative_path)
        .canonicalize()
        .map_err(|error| plugin_io_error("resolve Plugin contribution", error))?;
    if !path.starts_with(root) || (directory && !path.is_dir()) || (!directory && !path.is_file()) {
        return Err(plugin_input_error(
            "manifest",
            format!("Plugin {label} contribution has the wrong file type"),
        ));
    }
    Ok(path)
}

fn canonical_plugin_source(value: &str) -> Result<PathBuf, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(plugin_input_error(
            "source_path",
            "Plugin source is required",
        ));
    }
    let path = Path::new(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(plugin_input_error(
            "source_path",
            "Plugin source must be an existing absolute directory",
        ));
    }
    path.canonicalize()
        .map_err(|error| plugin_io_error("resolve Plugin source", error))
}

fn normalized_plugin_id(value: &str) -> Result<String, DesktopApplicationError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(plugin_input_error("plugin_id", "Plugin id is invalid"));
    }
    Ok(value.to_owned())
}

fn copy_plugin_tree(source: &Path, destination: &Path) -> Result<(), DesktopApplicationError> {
    if destination.exists() {
        return Err(plugin_input_error(
            "destination",
            "Plugin staging directory already exists",
        ));
    }
    fs::create_dir(destination)
        .map_err(|error| plugin_io_error("create Plugin staging directory", error))?;
    let mut file_count = 0;
    let mut total_bytes = 0;
    let result = copy_plugin_directory(source, destination, &mut file_count, &mut total_bytes);
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn copy_plugin_directory(
    source: &Path,
    destination: &Path,
    file_count: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), DesktopApplicationError> {
    let mut entries = fs::read_dir(source)
        .map_err(|error| plugin_io_error("read Plugin package directory", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| plugin_io_error("read Plugin package entry", error))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| plugin_io_error("inspect Plugin package entry", error))?;
        if metadata.file_type().is_symlink() {
            return Err(plugin_input_error(
                "source_path",
                "Plugin packages cannot contain symbolic links or junctions",
            ));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| plugin_io_error("create Plugin package directory", error))?;
            copy_plugin_directory(&entry.path(), &target, file_count, total_bytes)?;
        } else if metadata.is_file() {
            *file_count += 1;
            *total_bytes = total_bytes.saturating_add(metadata.len());
            if *file_count > MAX_PLUGIN_FILES || *total_bytes > MAX_PLUGIN_BYTES {
                return Err(plugin_input_error(
                    "source_path",
                    "Plugin package exceeds the file or byte limit",
                ));
            }
            fs::copy(entry.path(), &target)
                .map_err(|error| plugin_io_error("copy Plugin package file", error))?;
            OpenOptions::new()
                .write(true)
                .open(&target)
                .and_then(|file| file.sync_all())
                .map_err(|error| plugin_io_error("sync Plugin package file", error))?;
        } else {
            return Err(plugin_input_error(
                "source_path",
                "Plugin package contains an unsupported filesystem entry",
            ));
        }
    }
    Ok(())
}

fn plugin_package_digest(root: &Path) -> Result<String, DesktopApplicationError> {
    let mut files = Vec::new();
    collect_plugin_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut digest = Sha256::new();
    let mut total_bytes = 0_u64;
    for (relative, path) in files {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| plugin_io_error("inspect Plugin package file", error))?;
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > MAX_PLUGIN_BYTES {
            return Err(plugin_input_error(
                "source_path",
                "Plugin package exceeds the byte limit",
            ));
        }
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(metadata.len().to_le_bytes());
        let mut file = File::open(&path)
            .map_err(|error| plugin_io_error("open Plugin package file", error))?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| plugin_io_error("read Plugin package file", error))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_plugin_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), DesktopApplicationError> {
    let entries = fs::read_dir(directory)
        .map_err(|error| plugin_io_error("read Plugin package for hashing", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| plugin_io_error("read Plugin package entry", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| plugin_io_error("inspect Plugin package entry", error))?;
        if metadata.file_type().is_symlink() {
            return Err(plugin_input_error(
                "source_path",
                "Plugin packages cannot contain symbolic links or junctions",
            ));
        }
        if metadata.is_dir() {
            collect_plugin_files(root, &entry.path(), files)?;
        } else if metadata.is_file() {
            if files.len() >= MAX_PLUGIN_FILES {
                return Err(plugin_input_error(
                    "source_path",
                    "Plugin package exceeds the file limit",
                ));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| plugin_input_error("source_path", "Plugin file escaped package"))?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, entry.path()));
        } else {
            return Err(plugin_input_error(
                "source_path",
                "Plugin package contains an unsupported filesystem entry",
            ));
        }
    }
    Ok(())
}

fn ensure_plugin_revision(actual: u64, expected: u64) -> Result<(), DesktopApplicationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(plugin_input_error(
            "expected_registry_revision",
            format!("stale Plugins registry revision {expected}; current revision is {actual}"),
        ))
    }
}

fn bump_plugin_revision(
    registry: &mut AgentkitPluginsRegistry,
) -> Result<(), DesktopApplicationError> {
    registry.revision =
        registry
            .revision
            .checked_add(1)
            .ok_or(DesktopApplicationError::StateRevisionOverflow(
                "Plugins registry",
            ))?;
    Ok(())
}

fn plugin_input_error(field: &'static str, message: impl Into<String>) -> DesktopApplicationError {
    DesktopApplicationError::InvalidInput {
        field,
        message: message.into(),
    }
}

fn plugin_io_error(
    operation: &'static str,
    error: impl std::fmt::Display,
) -> DesktopApplicationError {
    DesktopApplicationError::Agent(format!("{operation}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use crate::application::{
        DesktopApplicationConfig, DesktopCredentialAction, DesktopHost, DesktopHostAction,
        DesktopHostError, DesktopHostResult, DesktopMcpCredentialKind, DesktopSecret,
    };

    use super::*;

    #[derive(Default)]
    struct TestHost {
        secrets: Mutex<BTreeMap<String, Vec<u8>>>,
    }

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &crate::application::DesktopHostContext,
            action: crate::application::DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key }) => {
                    Ok(DesktopHostResult::Credential(
                        self.secrets
                            .lock()
                            .unwrap()
                            .get(&key)
                            .cloned()
                            .map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Write { key, secret }) => {
                    self.secrets
                        .lock()
                        .unwrap()
                        .insert(key, secret.into_inner());
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key }) => {
                    self.secrets.lock().unwrap().remove(&key);
                    Ok(DesktopHostResult::Completed)
                }
                _ => Ok(DesktopHostResult::Completed),
            }
        }
    }

    fn plugin_fixture(root: &Path) {
        fs::create_dir_all(root.join("skills/review")).unwrap();
        fs::write(
            root.join("skills/review/SKILL.md"),
            "---\nname: review\ndescription: Review changes\n---\nReview carefully.\n",
        )
        .unwrap();
        fs::write(
            root.join("mcp.json"),
            serde_json::to_vec_pretty(&lilia_storage::AgentkitMcpRegistry {
                version: 1,
                revision: 1,
                secret_free: true,
                servers: vec![AgentkitMcpRegistryEntry {
                    server_id: "remote".to_owned(),
                    source: "plugin".to_owned(),
                    transport: "streamable_http".to_owned(),
                    command: None,
                    args: Vec::new(),
                    env_allowlist: Vec::new(),
                    env_secret_names: Vec::new(),
                    url: Some("https://example.com/mcp".to_owned()),
                    header_secret_names: vec!["Authorization".to_owned()],
                    registered_from: "plugin-fixture".to_owned(),
                    enabled: true,
                }],
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(
            lilia_storage::plugin_manifest_path(root),
            serde_json::to_vec_pretty(&LiliaPluginManifest {
                schema_version: 1,
                plugin_id: "review-tools".to_owned(),
                name: "Review Tools".to_owned(),
                plugin_version: "1.0.0".to_owned(),
                description: "Review extension".to_owned(),
                contributions: lilia_storage::LiliaPluginContributions {
                    skills: vec!["skills/review".to_owned()],
                    hooks: Vec::new(),
                    mcp: vec!["mcp.json".to_owned()],
                },
            })
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn plugin_install_toggle_and_delete_are_revisioned_and_runtime_backed() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        plugin_fixture(&source);
        let home = temp.path().join("home");
        let host = Arc::new(TestHost::default());
        let application = DesktopApplication::bootstrap(
            DesktopApplicationConfig::new(home.clone(), "plugin-test").unwrap(),
            host.clone(),
        )
        .unwrap();

        let installed = application
            .install_plugin_package(DesktopPluginInstall {
                expected_registry_revision: 0,
                source_path: source.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert!(!installed.enabled);
        assert_eq!(installed.skill_count, 1);
        assert_eq!(installed.mcp_server_count, 1);
        assert_eq!(application.plugin_packages().unwrap().0, 1);

        let enabled = application
            .set_plugin_package_enabled("review-tools", true, 1)
            .unwrap();
        assert!(enabled.enabled);
        assert!(enabled.runtime_available);
        let skills = application.extensions_snapshot().unwrap().skills;
        assert!(skills
            .iter()
            .any(|skill| { skill.skill_id == "review" && skill.runtime_available }));
        let server_id = "plugin.review-tools.remote";
        application
            .set_mcp_server_credential(
                server_id,
                DesktopMcpCredentialKind::Header,
                "Authorization",
                DesktopSecret::new(b"Bearer plugin-secret".to_vec()),
            )
            .unwrap();
        assert!(
            application
                .extensions_snapshot()
                .unwrap()
                .mcp_servers
                .iter()
                .find(|server| server.server_id == server_id)
                .unwrap()
                .credentials[0]
                .present
        );

        assert!(application
            .set_plugin_package_enabled("review-tools", false, 1)
            .is_err());
        application
            .set_plugin_package_enabled("review-tools", false, 2)
            .unwrap();
        application
            .delete_plugin_package("review-tools", 3)
            .unwrap();
        assert_eq!(application.plugin_packages().unwrap().0, 4);
        assert!(
            !lilia_storage::plugins_root_path(&application.config().data_paths())
                .join("review-tools")
                .exists()
        );
        assert!(host.secrets.lock().unwrap().is_empty());
    }

    #[test]
    fn plugin_package_hash_rejects_external_mutation_before_enable() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        plugin_fixture(&source);
        let application = DesktopApplication::bootstrap(
            DesktopApplicationConfig::new(temp.path().join("home"), "plugin-test").unwrap(),
            Arc::new(TestHost::default()),
        )
        .unwrap();
        let installed = application
            .install_plugin_package(DesktopPluginInstall {
                expected_registry_revision: 0,
                source_path: source.to_string_lossy().into_owned(),
            })
            .unwrap();
        fs::write(
            Path::new(&installed.path).join("skills/review/SKILL.md"),
            "changed",
        )
        .unwrap();
        assert!(application
            .set_plugin_package_enabled("review-tools", true, 1)
            .is_err());
        let view = application.plugin_packages().unwrap().2.remove(0);
        assert!(!view.runtime_available);
        assert!(!view.warnings.is_empty());
    }
}
