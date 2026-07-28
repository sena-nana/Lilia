use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[cfg(windows)]
use crate::process_command::hide_console_window;

use super::codex_probe::{
    build_codex_app_server_probe_status_cached, codex_candidate_filenames, command_output_result,
    managed_codex_home_dir, managed_codex_install_dir, parse_codex_cli_version,
};
use super::types::CodexAppServerStatus;

const CODEX_NPM_LATEST_URL: &str = "https://registry.npmjs.org/@openai%2Fcodex/latest";
const CODEX_GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/openai/codex/releases?per_page=20";
const CODEX_GITHUB_RELEASE_URL_PREFIX: &str =
    "https://api.github.com/repos/openai/codex/releases/tags/rust-v";
const CODEX_PACKAGE_CHECKSUM_ASSET: &str = "codex-package_SHA256SUMS";
const CODEX_METADATA_REQUEST_TIMEOUT_SECS: u64 = 20;
const CODEX_HTTP_CONNECT_TIMEOUT_SECS: u64 = 15;
const CODEX_UPDATE_STATE_IDLE: &str = "idle";
const CODEX_UPDATE_STATE_AVAILABLE: &str = "available";
const CODEX_UPDATE_STATE_DOWNLOADING: &str = "downloading";
const CODEX_UPDATE_STATE_READY: &str = "ready";
const CODEX_UPDATE_STATE_FAILED: &str = "failed";
const CODEX_STAGING_PREFIX: &str = ".staging.";
const CODEX_RELEASE_DIR_NAME: &str = "release";
#[cfg(windows)]
const CODEX_EXE: &str = "codex.exe";
#[cfg(not(windows))]
const CODEX_EXE: &str = "codex";
#[cfg(windows)]
const RG_EXE: &str = "rg.exe";
#[cfg(not(windows))]
const RG_EXE: &str = "rg";
#[cfg(windows)]
const COMMAND_RUNNER_EXE: &str = "codex-command-runner.exe";
#[cfg(not(windows))]
const COMMAND_RUNNER_EXE: &str = "codex-command-runner";
#[cfg(windows)]
const SANDBOX_SETUP_EXE: &str = "codex-windows-sandbox-setup.exe";
#[cfg(not(windows))]
const SANDBOX_SETUP_EXE: &str = "codex-linux-sandbox";

#[derive(Debug, Deserialize)]
struct NpmLatestPackage {
    version: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    body: Option<String>,
    tag_name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedCodexUpdate {
    version: String,
    install_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexInstallLayout {
    Package,
    LegacyPlatformNpm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodexTarget {
    package_target: &'static str,
    npm_tag: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexReleaseAssetSelection {
    layout: CodexInstallLayout,
    package: GithubReleaseAsset,
    checksum: Option<GithubReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CodexUpdateManagerState {
    Idle,
    Downloading {
        version: String,
        progress_percent: Option<u8>,
    },
    Ready(String),
    Failed {
        version: String,
        error: String,
        prepared: bool,
    },
}

fn codex_update_manager() -> &'static Mutex<CodexUpdateManagerState> {
    static MANAGER: OnceLock<Mutex<CodexUpdateManagerState>> = OnceLock::new();
    MANAGER.get_or_init(|| Mutex::new(CodexUpdateManagerState::Idle))
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("Lilia Codex app-server updater")
        .connect_timeout(Duration::from_secs(CODEX_HTTP_CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|err| format!("创建 HTTP 客户端失败：{err}"))
}

fn github_latest_version(releases: &[GithubRelease]) -> Result<String, String> {
    for release in releases {
        if release.draft || release.prerelease {
            continue;
        }
        let Some(tag) = release.tag_name.as_deref() else {
            continue;
        };
        let Some(version) = tag.strip_prefix("rust-v") else {
            continue;
        };
        if parse_codex_cli_version(version).is_some() {
            return Ok(version.to_string());
        }
    }
    Err("GitHub releases 未找到稳定 rust-vX.Y.Z 版本。".to_string())
}

fn fetch_npm_latest_version_with(client: &reqwest::blocking::Client) -> Result<String, String> {
    let pkg = client
        .get(CODEX_NPM_LATEST_URL)
        .timeout(Duration::from_secs(CODEX_METADATA_REQUEST_TIMEOUT_SECS))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("npm registry 请求失败：{err}"))?
        .json::<NpmLatestPackage>()
        .map_err(|err| format!("npm registry 响应解析失败：{err}"))?;
    let version = pkg.version.trim();
    if version.is_empty() {
        Err("npm registry 最新版本为空。".to_string())
    } else {
        Ok(version.to_string())
    }
}

fn fetch_github_latest_version_with(client: &reqwest::blocking::Client) -> Result<String, String> {
    let releases = client
        .get(CODEX_GITHUB_RELEASES_URL)
        .timeout(Duration::from_secs(CODEX_METADATA_REQUEST_TIMEOUT_SECS))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("GitHub releases 请求失败：{err}"))?
        .json::<Vec<GithubRelease>>()
        .map_err(|err| format!("GitHub releases 响应解析失败：{err}"))?;
    github_latest_version(&releases)
}

fn fetch_latest_version_with(client: &reqwest::blocking::Client) -> Result<String, String> {
    match fetch_npm_latest_version_with(client) {
        Ok(version) => Ok(version),
        Err(npm_err) => fetch_github_latest_version_with(client).map_err(|github_err| {
            format!(
                "查询 Codex 最新版本失败：npm registry：{npm_err}；GitHub releases：{github_err}"
            )
        }),
    }
}

fn fetch_release_notes_with(
    client: &reqwest::blocking::Client,
    version: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{CODEX_GITHUB_RELEASE_URL_PREFIX}{version}");
    let release = client
        .get(url)
        .timeout(Duration::from_secs(CODEX_METADATA_REQUEST_TIMEOUT_SECS))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("查询 Codex 更新内容失败：{err}"))?
        .json::<GithubRelease>()
        .map_err(|err| format!("解析 Codex 更新内容失败：{err}"))?;
    Ok(summarize_release_notes(
        release.body.as_deref().unwrap_or(""),
    ))
}

pub(crate) fn summarize_release_notes(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("- ").map(str::trim))
        .filter(|line| !line.is_empty())
        .take(5)
        .map(ToString::to_string)
        .collect()
}

fn parsed_version(value: Option<&str>) -> Option<(u32, u32, u32)> {
    value.and_then(parse_codex_cli_version)
}

pub(crate) fn enrich_codex_status_with_update<F, G>(
    mut status: CodexAppServerStatus,
    mut fetch_latest_version: F,
    mut fetch_release_notes: G,
) -> CodexAppServerStatus
where
    F: FnMut() -> Result<String, String>,
    G: FnMut(&str) -> Result<Vec<String>, String>,
{
    match fetch_latest_version() {
        Ok(latest) => {
            let current = parsed_version(status.version.as_deref());
            let latest_parsed = parse_codex_cli_version(&latest);
            status.latest_version = Some(latest.clone());
            status.update_available = latest_parsed.is_some()
                && (!status.managed
                    || current
                        .map(|version| version < latest_parsed.unwrap())
                        .unwrap_or(true));
            status.release_notes = fetch_release_notes(&latest).unwrap_or_default();
            status.update_error = None;
        }
        Err(err) => {
            status.latest_version = None;
            status.update_available = false;
            status.release_notes = Vec::new();
            status.update_error = Some(err);
        }
    }
    status
}

pub(crate) fn check_codex_app_server_update_status() -> CodexAppServerStatus {
    let status = build_codex_app_server_probe_status_cached(false).public;
    check_codex_app_server_update_status_for(status)
}

fn sanitized_version_path(version: &str) -> String {
    version
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn prepared_release_root(root: &Path) -> PathBuf {
    root.join(CODEX_RELEASE_DIR_NAME)
}

fn install_dir_for_layout(release_root: &Path, layout: CodexInstallLayout) -> PathBuf {
    match layout {
        CodexInstallLayout::Package => release_root.join("bin"),
        CodexInstallLayout::LegacyPlatformNpm => release_root.to_path_buf(),
    }
}

fn prepared_install_dir(root: &Path) -> PathBuf {
    let release_root = prepared_release_root(root);
    if release_root.join("bin").exists() || release_root.join("codex-package.json").exists() {
        install_dir_for_layout(&release_root, CodexInstallLayout::Package)
    } else {
        install_dir_for_layout(&release_root, CodexInstallLayout::LegacyPlatformNpm)
    }
}

fn prepared_codex_root(version: &str) -> PathBuf {
    managed_codex_home_dir()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("prepared")
        .join(sanitized_version_path(version))
}

fn prepared_codex_staging_root(version: &str) -> PathBuf {
    let pid = std::process::id();
    let staging_name = format!(
        "{CODEX_STAGING_PREFIX}{}.{}",
        sanitized_version_path(version),
        pid
    );
    prepared_codex_root(version)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(staging_name)
}

fn prepared_codex_update(version: &str) -> PreparedCodexUpdate {
    let root = prepared_codex_root(version);
    PreparedCodexUpdate {
        version: version.to_string(),
        install_dir: prepared_install_dir(&root),
    }
}

fn apply_update_state(mut status: CodexAppServerStatus) -> CodexAppServerStatus {
    status.update_state = if status.update_available {
        CODEX_UPDATE_STATE_AVAILABLE.to_string()
    } else {
        CODEX_UPDATE_STATE_IDLE.to_string()
    };
    status.prepared_version = None;
    status.update_progress_percent = None;

    let latest = status.latest_version.clone();
    let state = codex_update_manager()
        .lock()
        .map(|state| state.clone())
        .unwrap_or(CodexUpdateManagerState::Idle);
    match (latest.as_deref(), state) {
        (
            Some(latest),
            CodexUpdateManagerState::Downloading {
                version,
                progress_percent,
            },
        ) if version == latest => {
            status.update_state = CODEX_UPDATE_STATE_DOWNLOADING.to_string();
            status.prepared_version = Some(version);
            status.update_progress_percent = progress_percent;
        }
        (Some(latest), CodexUpdateManagerState::Ready(version)) if version == latest => {
            status.update_state = CODEX_UPDATE_STATE_READY.to_string();
            status.prepared_version = Some(version);
        }
        (
            Some(latest),
            CodexUpdateManagerState::Failed {
                version,
                error,
                prepared,
            },
        ) if version == latest => {
            status.update_state = CODEX_UPDATE_STATE_FAILED.to_string();
            if prepared {
                status.prepared_version = Some(version);
            }
            status.update_error = Some(error);
        }
        _ => {}
    }
    status
}

fn should_start_download_for(version: &str, state: &CodexUpdateManagerState) -> bool {
    match state {
        CodexUpdateManagerState::Downloading {
            version: active, ..
        }
        | CodexUpdateManagerState::Ready(active) => active != version,
        CodexUpdateManagerState::Failed {
            version: active,
            prepared,
            ..
        } => active != version || !prepared,
        CodexUpdateManagerState::Idle => true,
    }
}

fn prepared_update_available_with<F>(version: &str, mut verify: F) -> Result<bool, String>
where
    F: FnMut(&PreparedCodexUpdate) -> Result<(), String>,
{
    let prepared = prepared_codex_update(version);
    if !prepared.install_dir.exists() {
        return Ok(false);
    }
    verify(&prepared)?;
    Ok(true)
}

fn restore_ready_update_from_disk_with<F>(version: &str, verify: F)
where
    F: FnMut(&PreparedCodexUpdate) -> Result<(), String>,
{
    let should_probe_disk = codex_update_manager()
        .lock()
        .map(|state| matches!(*state, CodexUpdateManagerState::Idle))
        .unwrap_or(false);
    if !should_probe_disk {
        return;
    }
    if !matches!(prepared_update_available_with(version, verify), Ok(true)) {
        return;
    }
    if let Ok(mut state) = codex_update_manager().lock() {
        if matches!(*state, CodexUpdateManagerState::Idle) {
            *state = CodexUpdateManagerState::Ready(version.to_string());
        }
    }
}

fn restore_ready_update_from_disk(version: &str) {
    restore_ready_update_from_disk_with(version, verify_prepared_codex_update);
}

fn maybe_start_codex_update_download(status: &CodexAppServerStatus) {
    if !status.update_available {
        return;
    }
    let Some(version) = status.latest_version.clone() else {
        return;
    };

    {
        let Ok(mut state) = codex_update_manager().lock() else {
            return;
        };
        if !should_start_download_for(&version, &state) {
            return;
        }
        *state = CodexUpdateManagerState::Downloading {
            version: version.clone(),
            progress_percent: None,
        };
    }

    thread::spawn(move || {
        let result = prepare_codex_update(&version);
        if let Ok(mut state) = codex_update_manager().lock() {
            *state = match result {
                Ok(()) => CodexUpdateManagerState::Ready(version),
                Err(error) => CodexUpdateManagerState::Failed {
                    version,
                    error,
                    prepared: false,
                },
            };
        }
    });
}

fn set_codex_update_download_progress(version: &str, progress_percent: Option<u8>) {
    if let Ok(mut state) = codex_update_manager().lock() {
        if let CodexUpdateManagerState::Downloading {
            version: active,
            progress_percent: active_progress,
        } = &mut *state
        {
            if active == version {
                *active_progress = progress_percent;
            }
        }
    }
}

fn powershell_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn current_codex_target() -> Result<CodexTarget, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok(CodexTarget {
            package_target: "x86_64-pc-windows-msvc",
            npm_tag: "win32-x64",
        }),
        ("windows", "aarch64") => Ok(CodexTarget {
            package_target: "aarch64-pc-windows-msvc",
            npm_tag: "win32-arm64",
        }),
        ("macos", "x86_64") => Ok(CodexTarget {
            package_target: "x86_64-apple-darwin",
            npm_tag: "darwin-x64",
        }),
        ("macos", "aarch64") => Ok(CodexTarget {
            package_target: "aarch64-apple-darwin",
            npm_tag: "darwin-arm64",
        }),
        ("linux", "x86_64") => Ok(CodexTarget {
            package_target: "x86_64-unknown-linux-musl",
            npm_tag: "linux-x64",
        }),
        ("linux", "aarch64") => Ok(CodexTarget {
            package_target: "aarch64-unknown-linux-musl",
            npm_tag: "linux-arm64",
        }),
        (os, arch) => Err(format!(
            "当前平台不支持 Lilia 内置 Codex 更新：{}-{}",
            os, arch
        )),
    }
}

fn release_asset_sha256(asset: &GithubReleaseAsset) -> Result<String, String> {
    let digest = asset
        .digest
        .as_deref()
        .ok_or_else(|| format!("Codex release asset {} 缺少 SHA-256 digest。", asset.name))?
        .trim();
    digest
        .strip_prefix("sha256:")
        .filter(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| {
            format!(
                "Codex release asset {} 的 SHA-256 digest 无效。",
                asset.name
            )
        })
}

fn find_release_asset(
    release: &GithubRelease,
    name: &str,
) -> Result<Option<GithubReleaseAsset>, String> {
    let Some(asset) = release.assets.iter().find(|asset| asset.name == name) else {
        return Ok(None);
    };
    release_asset_sha256(asset)?;
    Ok(Some(asset.clone()))
}

fn select_codex_release_assets(
    release: &GithubRelease,
    version: &str,
    target: CodexTarget,
) -> Result<CodexReleaseAssetSelection, String> {
    let package_name = format!("codex-package-{}.tar.gz", target.package_target);
    let package = find_release_asset(release, &package_name)?;
    let checksum = find_release_asset(release, CODEX_PACKAGE_CHECKSUM_ASSET)?;
    if let (Some(package), Some(checksum)) = (package, checksum) {
        return Ok(CodexReleaseAssetSelection {
            layout: CodexInstallLayout::Package,
            package,
            checksum: Some(checksum),
        });
    }

    let legacy_name = format!("codex-npm-{}-{version}.tgz", target.npm_tag);
    if let Some(package) = find_release_asset(release, &legacy_name)? {
        return Ok(CodexReleaseAssetSelection {
            layout: CodexInstallLayout::LegacyPlatformNpm,
            package,
            checksum: None,
        });
    }

    Err(format!(
        "Codex {version} 缺少当前平台所需的 release asset：{package_name} 或 {legacy_name}。"
    ))
}

fn fetch_github_release_with(
    client: &reqwest::blocking::Client,
    version: &str,
) -> Result<GithubRelease, String> {
    let url = format!("{CODEX_GITHUB_RELEASE_URL_PREFIX}{version}");
    client
        .get(url)
        .timeout(Duration::from_secs(CODEX_METADATA_REQUEST_TIMEOUT_SECS))
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("查询 Codex release 失败：{err}"))?
        .json::<GithubRelease>()
        .map_err(|err| format!("解析 Codex release 失败：{err}"))
}

fn fetch_asset_bytes(
    client: &reqwest::blocking::Client,
    asset: &GithubReleaseAsset,
    progress_version: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|err| format!("下载 Codex release asset {} 失败：{err}", asset.name))?;
    let total = response.content_length();
    let progress_target = progress_version.zip(total.filter(|value| *value > 0));
    if let Some((version, _)) = progress_target {
        set_codex_update_download_progress(version, Some(0));
    }

    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(64 * 1024 * 1024) as usize);
    let mut buffer = [0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    let mut last_progress = 0_u8;
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|err| format!("读取 Codex release asset {} 失败：{err}", asset.name))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        downloaded += read as u64;
        if let Some((version, total)) = progress_target {
            let progress = ((downloaded.saturating_mul(100)) / total).min(100) as u8;
            if progress != last_progress {
                set_codex_update_download_progress(version, Some(progress));
                last_progress = progress;
            }
        }
    }
    if let Some((version, _)) = progress_target {
        set_codex_update_download_progress(version, Some(100));
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn test_archive_digest(
    bytes: &[u8],
    expected_digest: &str,
    asset_name: &str,
) -> Result<(), String> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected_digest) {
        Ok(())
    } else {
        Err(format!(
            "Codex release asset {asset_name} 校验失败：期望 {expected_digest}，实际 {actual}。"
        ))
    }
}

fn package_archive_digest(checksum_manifest: &str, asset_name: &str) -> Result<String, String> {
    for line in checksum_manifest.lines() {
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name == asset_name
            && digest.len() == 64
            && digest.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Ok(digest.to_ascii_lowercase());
        }
    }
    Err(format!(
        "Codex checksum manifest 缺少 release asset {asset_name} 的 SHA-256。"
    ))
}

fn unpack_tgz(bytes: &[u8], destination: &Path, asset_name: &str) -> Result<(), String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|err| format!("解压 Codex release asset {asset_name} 失败：{err}"))
}

fn path_has_file(root: &Path, relative: impl AsRef<Path>) -> bool {
    root.join(relative.as_ref()).is_file()
}

fn package_contents_are_complete(release_root: &Path) -> bool {
    path_has_file(release_root, "codex-package.json")
        && path_has_file(release_root, Path::new("bin").join(CODEX_EXE))
        && path_has_file(release_root, Path::new("codex-path").join(RG_EXE))
        && path_has_file(
            release_root,
            Path::new("codex-resources").join(COMMAND_RUNNER_EXE),
        )
        && (!cfg!(windows)
            || path_has_file(
                release_root,
                Path::new("codex-resources").join(SANDBOX_SETUP_EXE),
            ))
}

fn legacy_platform_contents_are_complete(release_root: &Path) -> bool {
    path_has_file(release_root, CODEX_EXE)
        && path_has_file(
            release_root,
            Path::new("codex-resources").join(COMMAND_RUNNER_EXE),
        )
        && path_has_file(release_root, Path::new("codex-resources").join(RG_EXE))
        && (!cfg!(windows)
            || path_has_file(
                release_root,
                Path::new("codex-resources").join(SANDBOX_SETUP_EXE),
            ))
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建 Codex release 目录 {} 失败：{err}", parent.display()))?;
    }
    fs::copy(source, destination).map_err(|err| {
        format!(
            "复制 Codex release 文件 {} -> {} 失败：{err}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn copy_legacy_platform_package(
    extract_root: &Path,
    release_root: &Path,
    target: CodexTarget,
) -> Result<(), String> {
    let vendor_root = extract_root
        .join("package")
        .join("vendor")
        .join(target.package_target);
    copy_file(
        &vendor_root.join("codex").join(CODEX_EXE),
        &release_root.join(CODEX_EXE),
    )?;
    copy_file(
        &vendor_root.join("codex").join(COMMAND_RUNNER_EXE),
        &release_root
            .join("codex-resources")
            .join(COMMAND_RUNNER_EXE),
    )?;
    copy_file(
        &vendor_root.join("path").join(RG_EXE),
        &release_root.join("codex-resources").join(RG_EXE),
    )?;
    if cfg!(windows) {
        copy_file(
            &vendor_root.join("codex").join(SANDBOX_SETUP_EXE),
            &release_root.join("codex-resources").join(SANDBOX_SETUP_EXE),
        )?;
    }
    if legacy_platform_contents_are_complete(release_root) {
        Ok(())
    } else {
        Err("Downloaded Codex npm archive did not contain the expected legacy platform package layout.".to_string())
    }
}

fn retry_fs_operation(
    ignore_not_found: bool,
    mut operation: impl FnMut() -> Result<(), std::io::Error>,
) -> Result<(), std::io::Error> {
    let mut last_err = None;
    for attempt in 0..8 {
        match operation() {
            Ok(()) => return Ok(()),
            Err(err) if ignore_not_found && err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(err) => {
                last_err = Some(err);
                if attempt < 7 {
                    thread::sleep(Duration::from_millis(150));
                }
            }
        }
    }
    Err(last_err.expect("retry_fs_operation must record an error"))
}

fn remove_dir_all_with_retry(path: &Path) -> Result<(), std::io::Error> {
    retry_fs_operation(true, || fs::remove_dir_all(path))
}

fn rename_dir_with_retry(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    retry_fs_operation(false, || fs::rename(source, destination))
}

fn clean_stale_prepared_staging(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(CODEX_STAGING_PREFIX) {
            let _ = remove_dir_all_with_retry(&entry.path());
        }
    }
}

fn publish_prepared_root(staging_root: &Path, final_root: &Path) -> Result<(), String> {
    if final_root.exists() {
        remove_dir_all_with_retry(final_root)
            .map_err(|err| format!("清理旧 Codex 更新缓存 {} 失败：{err}", final_root.display()))?;
    }
    rename_dir_with_retry(staging_root, final_root).map_err(|err| {
        format!(
            "发布 Codex 更新缓存 {} -> {} 失败：{err}",
            staging_root.display(),
            final_root.display()
        )
    })
}

fn download_release_to_staging(
    client: &reqwest::blocking::Client,
    selection: &CodexReleaseAssetSelection,
    target: CodexTarget,
    version: &str,
    release_root: &Path,
) -> Result<CodexInstallLayout, String> {
    fs::create_dir_all(release_root).map_err(|err| {
        format!(
            "创建 Codex release 解压目录 {} 失败：{err}",
            release_root.display()
        )
    })?;

    let package_bytes = fetch_asset_bytes(client, &selection.package, Some(version))?;
    match selection.layout {
        CodexInstallLayout::Package => {
            let checksum = selection
                .checksum
                .as_ref()
                .ok_or_else(|| "Codex package release 缺少 checksum asset。".to_string())?;
            let checksum_bytes = fetch_asset_bytes(client, checksum, None)?;
            test_archive_digest(
                &checksum_bytes,
                &release_asset_sha256(checksum)?,
                &checksum.name,
            )?;
            let checksum_manifest = String::from_utf8(checksum_bytes).map_err(|err| {
                format!("解析 Codex checksum manifest {} 失败：{err}", checksum.name)
            })?;
            let expected_package_digest =
                package_archive_digest(&checksum_manifest, &selection.package.name)?;
            test_archive_digest(
                &package_bytes,
                &expected_package_digest,
                &selection.package.name,
            )?;
            unpack_tgz(&package_bytes, release_root, &selection.package.name)?;
            if package_contents_are_complete(release_root) {
                Ok(CodexInstallLayout::Package)
            } else {
                Err(
                    "Downloaded Codex package archive did not contain the expected package layout."
                        .to_string(),
                )
            }
        }
        CodexInstallLayout::LegacyPlatformNpm => {
            test_archive_digest(
                &package_bytes,
                &release_asset_sha256(&selection.package)?,
                &selection.package.name,
            )?;
            let extract_root = release_root
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("extract");
            fs::create_dir_all(&extract_root).map_err(|err| {
                format!(
                    "创建 Codex legacy 解压目录 {} 失败：{err}",
                    extract_root.display()
                )
            })?;
            unpack_tgz(&package_bytes, &extract_root, &selection.package.name)?;
            copy_legacy_platform_package(&extract_root, release_root, target)?;
            remove_dir_all_with_retry(&extract_root).map_err(|err| {
                format!(
                    "清理 Codex legacy 解压目录 {} 失败：{err}",
                    extract_root.display()
                )
            })?;
            Ok(CodexInstallLayout::LegacyPlatformNpm)
        }
    }
}

fn codex_help_supports_app_server(help: &str) -> bool {
    help.contains("codex app-server") || help.contains("Usage:")
}

fn verify_prepared_codex_update(prepared: &PreparedCodexUpdate) -> Result<(), String> {
    for filename in codex_candidate_filenames() {
        let candidate = prepared.install_dir.join(filename);
        if !candidate.exists() {
            continue;
        }
        let candidate = candidate.to_string_lossy().to_string();
        let version_output = command_output_result(&candidate, &["--version"])
            .map_err(|err| format!("验证已下载 Codex {} 版本失败：{err}", prepared.version))?;
        let parsed = parse_codex_cli_version(&version_output)
            .ok_or_else(|| format!("无法解析已下载 Codex 版本：{version_output}"))?;
        let expected = parse_codex_cli_version(&prepared.version)
            .ok_or_else(|| format!("无法解析 Codex 目标版本：{}", prepared.version))?;
        if parsed != expected {
            return Err(format!(
                "已下载 Codex 版本不匹配：期望 {}，实际 {}",
                prepared.version, version_output
            ));
        }
        let help = command_output_result(&candidate, &["app-server", "--help"])
            .map_err(|err| format!("验证已下载 Codex app-server 失败：{err}"))?;
        if !codex_help_supports_app_server(&help) {
            return Err("已下载 Codex 不支持 app-server。".to_string());
        }
        return Ok(());
    }
    Err(format!(
        "已下载 Codex {} 缺少可执行文件。",
        prepared.version
    ))
}

fn prepare_codex_update(version: &str) -> Result<(), String> {
    let client = http_client()?;
    let target = current_codex_target()?;
    let release = fetch_github_release_with(&client, version)?;
    let selection = select_codex_release_assets(&release, version, target)?;
    let final_root = prepared_codex_root(version);
    let parent = final_root
        .parent()
        .ok_or_else(|| format!("无法解析 Codex 更新缓存父目录：{}", final_root.display()))?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("创建 Codex 更新缓存目录 {} 失败：{err}", parent.display()))?;
    clean_stale_prepared_staging(parent);

    let staging_root = prepared_codex_staging_root(version);
    if staging_root.exists() {
        remove_dir_all_with_retry(&staging_root).map_err(|err| {
            format!(
                "清理旧 Codex staging 更新缓存 {} 失败：{err}",
                staging_root.display()
            )
        })?;
    }

    let result = (|| {
        let release_root = prepared_release_root(&staging_root);
        let layout =
            download_release_to_staging(&client, &selection, target, version, &release_root)?;
        let staged = PreparedCodexUpdate {
            version: version.to_string(),
            install_dir: install_dir_for_layout(&release_root, layout),
        };
        verify_prepared_codex_update(&staged)?;
        publish_prepared_root(&staging_root, &final_root)?;
        verify_prepared_codex_update(&prepared_codex_update(version))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = remove_dir_all_with_retry(&staging_root);
    }
    result
}

fn active_bin_backup_path(active_bin: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    active_bin.with_file_name(format!(
        "{}.backup.{}",
        active_bin
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bin"),
        stamp
    ))
}

#[cfg(windows)]
fn create_directory_link(link_path: &Path, target_path: &Path) -> Result<(), String> {
    let script = format!(
        "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
        powershell_single_quoted(&link_path.to_string_lossy()),
        powershell_single_quoted(&target_path.to_string_lossy()),
    );
    let mut command = Command::new("powershell.exe");
    hide_console_window(&mut command);
    let output = command
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("创建 Codex 切换链接失败：{err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        Err(if detail.is_empty() {
            format!("创建 Codex 切换链接失败：{}", output.status)
        } else {
            format!("创建 Codex 切换链接失败：{detail}")
        })
    }
}

#[cfg(not(windows))]
fn create_directory_link(link_path: &Path, target_path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target_path, link_path).map_err(|err| {
        format!(
            "创建 Codex 切换链接 {} -> {} 失败：{err}",
            link_path.display(),
            target_path.display()
        )
    })
}

fn replace_managed_bin_with_prepared(prepared: &PreparedCodexUpdate) -> Result<(), String> {
    let active_bin = managed_codex_install_dir();
    if let Some(parent) = active_bin.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建 Codex runtime 目录 {} 失败：{err}", parent.display()))?;
    }
    let backup = if active_bin.exists() {
        let metadata = fs::symlink_metadata(&active_bin).map_err(|err| {
            format!(
                "检查当前 Codex 安装入口 {} 失败：{err}",
                active_bin.display()
            )
        })?;
        if is_reparse_point(&metadata) {
            fs::remove_dir(&active_bin).map_err(|err| {
                format!(
                    "移除当前 Codex 安装链接 {} 失败：{err}",
                    active_bin.display()
                )
            })?;
            None
        } else {
            let backup = active_bin_backup_path(&active_bin);
            fs::rename(&active_bin, &backup).map_err(|err| {
                format!(
                    "备份当前 Codex 安装入口 {} 失败：{err}",
                    active_bin.display()
                )
            })?;
            Some(backup)
        }
    } else {
        None
    };

    match create_directory_link(&active_bin, &prepared.install_dir).and_then(|_| {
        verify_prepared_codex_update(&PreparedCodexUpdate {
            version: prepared.version.clone(),
            install_dir: active_bin.clone(),
        })
    }) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_dir(&active_bin);
            if let Some(backup) = backup {
                let _ = fs::rename(&backup, &active_bin);
            }
            Err(err)
        }
    }
}

pub(crate) fn install_or_update_codex_app_server() -> Result<CodexAppServerStatus, String> {
    let version = {
        let mut state = codex_update_manager()
            .lock()
            .map_err(|_| "Codex 更新状态不可用。".to_string())?;
        match state.clone() {
            CodexUpdateManagerState::Ready(version) => {
                *state = CodexUpdateManagerState::Idle;
                version
            }
            CodexUpdateManagerState::Failed {
                version,
                prepared: true,
                ..
            } => {
                *state = CodexUpdateManagerState::Idle;
                version
            }
            CodexUpdateManagerState::Downloading { .. } => {
                return Err("Codex app-server 更新仍在后台下载。".to_string());
            }
            CodexUpdateManagerState::Failed { error, .. } => return Err(error),
            _ => return Err("Codex app-server 更新尚未准备好。".to_string()),
        }
    };
    let prepared = prepared_codex_update(&version);

    if let Err(err) = replace_managed_bin_with_prepared(&prepared) {
        if let Ok(mut state) = codex_update_manager().lock() {
            *state = CodexUpdateManagerState::Failed {
                version,
                error: err.clone(),
                prepared: true,
            };
        }
        return Err(err);
    }
    if let Ok(mut state) = codex_update_manager().lock() {
        *state = CodexUpdateManagerState::Idle;
    }
    let status = build_codex_app_server_probe_status_cached(true).public;
    Ok(check_codex_app_server_update_status_for(status))
}

fn check_codex_app_server_update_status_for(status: CodexAppServerStatus) -> CodexAppServerStatus {
    let Ok(client) = http_client() else {
        return apply_update_state(enrich_codex_status_with_update(
            status,
            || Err("创建 HTTP 客户端失败。".to_string()),
            |_| Ok(Vec::new()),
        ));
    };
    let status = enrich_codex_status_with_update(
        status,
        || fetch_latest_version_with(&client),
        |version| fetch_release_notes_with(&client, version),
    );
    if status.update_available {
        if let Some(version) = status.latest_version.as_deref() {
            restore_ready_update_from_disk(version);
        }
    }
    maybe_start_codex_update_download(&status);
    apply_update_state(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(version: Option<&str>, managed: bool) -> CodexAppServerStatus {
        CodexAppServerStatus {
            version: version.map(ToString::to_string),
            install_path: None,
            managed,
            available: version.is_some(),
            supports_required_protocol: version.is_some(),
            failure_kind: None,
            issues: Vec::new(),
            latest_version: None,
            update_available: false,
            release_notes: Vec::new(),
            update_error: None,
            update_state: CODEX_UPDATE_STATE_IDLE.to_string(),
            prepared_version: None,
            update_progress_percent: None,
        }
    }

    fn update_manager_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_update_manager_state<T>(state: CodexUpdateManagerState, run: impl FnOnce() -> T) -> T {
        let _guard = update_manager_test_lock().lock().unwrap();
        *codex_update_manager().lock().unwrap() = state;
        let result = run();
        *codex_update_manager().lock().unwrap() = CodexUpdateManagerState::Idle;
        result
    }

    fn release_asset(name: &str, digest: &str) -> GithubReleaseAsset {
        GithubReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/{name}"),
            digest: Some(format!("sha256:{digest}")),
        }
    }

    fn release_with_assets(assets: Vec<GithubReleaseAsset>) -> GithubRelease {
        GithubRelease {
            body: None,
            tag_name: None,
            draft: false,
            prerelease: false,
            assets,
        }
    }

    fn release_tag(tag_name: &str, draft: bool, prerelease: bool) -> GithubRelease {
        GithubRelease {
            body: None,
            tag_name: Some(tag_name.to_string()),
            draft,
            prerelease,
            assets: Vec::new(),
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lilia-codex-update-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_lilia_home<T>(home: &Path, run: impl FnOnce() -> T) -> T {
        let previous = std::env::var_os("LILIA_HOME");
        std::env::set_var("LILIA_HOME", home);
        let result = run();
        if let Some(previous) = previous {
            std::env::set_var("LILIA_HOME", previous);
        } else {
            std::env::remove_var("LILIA_HOME");
        }
        result
    }

    #[test]
    fn update_status_marks_missing_managed_copy_as_update_available() {
        let updated = enrich_codex_status_with_update(
            status(Some("codex 0.141.0"), false),
            || Ok("0.141.0".to_string()),
            |_| Ok(vec!["notes".to_string()]),
        );

        assert!(updated.update_available);
        assert_eq!(updated.latest_version.as_deref(), Some("0.141.0"));
        assert_eq!(updated.release_notes, vec!["notes"]);
    }

    #[test]
    fn update_status_does_not_mark_current_managed_copy() {
        let updated = enrich_codex_status_with_update(
            status(Some("codex 0.141.0"), true),
            || Ok("0.141.0".to_string()),
            |_| Ok(Vec::new()),
        );

        assert!(!updated.update_available);
    }

    #[test]
    fn update_status_handles_release_note_failure_as_empty_notes() {
        let updated = enrich_codex_status_with_update(
            status(Some("codex 0.140.0"), true),
            || Ok("0.141.0".to_string()),
            |_| Err("release missing".to_string()),
        );

        assert!(updated.update_available);
        assert!(updated.release_notes.is_empty());
        assert!(updated.update_error.is_none());
    }

    #[test]
    fn update_status_records_latest_version_failure() {
        let updated = enrich_codex_status_with_update(
            status(Some("codex 0.140.0"), true),
            || Err("offline".to_string()),
            |_| Ok(Vec::new()),
        );

        assert!(!updated.update_available);
        assert_eq!(updated.update_error.as_deref(), Some("offline"));
    }

    #[test]
    fn github_latest_version_uses_first_stable_rust_tag() {
        let releases = vec![
            release_tag("rust-v0.144.0", true, false),
            release_tag("rust-v0.143.0", false, true),
            release_tag("codex-v0.142.0", false, false),
            release_tag("rust-vnot-a-version", false, false),
            release_tag("rust-v0.141.0", false, false),
        ];

        assert_eq!(github_latest_version(&releases).unwrap(), "0.141.0");
    }

    #[test]
    fn github_latest_version_rejects_missing_stable_rust_tag() {
        let releases = vec![
            release_tag("rust-v0.143.0", false, true),
            release_tag("codex-v0.142.0", false, false),
            release_tag("rust-vnot-a-version", false, false),
        ];

        assert!(github_latest_version(&releases)
            .unwrap_err()
            .contains("未找到"));
    }

    #[test]
    fn release_note_summary_uses_first_bullets() {
        let notes = summarize_release_notes(
            "## New Features\n\n- First item\n- Second item\n\n## Chores\n- Third item",
        );

        assert_eq!(notes, vec!["First item", "Second item", "Third item"]);
    }

    #[test]
    fn update_state_reports_ready_prepared_version() {
        let mut base = status(Some("codex 0.140.0"), true);
        base.latest_version = Some("0.141.0".to_string());
        base.update_available = true;

        let updated = with_update_manager_state(
            CodexUpdateManagerState::Ready("0.141.0".to_string()),
            || apply_update_state(base),
        );

        assert_eq!(updated.update_state, CODEX_UPDATE_STATE_READY);
        assert_eq!(updated.prepared_version.as_deref(), Some("0.141.0"));
        assert_eq!(updated.update_progress_percent, None);
    }

    #[test]
    fn update_state_reports_download_progress() {
        let mut base = status(Some("codex 0.140.0"), true);
        base.latest_version = Some("0.141.0".to_string());
        base.update_available = true;

        let updated = with_update_manager_state(
            CodexUpdateManagerState::Downloading {
                version: "0.141.0".to_string(),
                progress_percent: Some(42),
            },
            || apply_update_state(base),
        );

        assert_eq!(updated.update_state, CODEX_UPDATE_STATE_DOWNLOADING);
        assert_eq!(updated.prepared_version.as_deref(), Some("0.141.0"));
        assert_eq!(updated.update_progress_percent, Some(42));
    }

    #[test]
    fn update_state_keeps_unknown_download_progress_empty() {
        let mut base = status(Some("codex 0.140.0"), true);
        base.latest_version = Some("0.141.0".to_string());
        base.update_available = true;

        let updated = with_update_manager_state(
            CodexUpdateManagerState::Downloading {
                version: "0.141.0".to_string(),
                progress_percent: None,
            },
            || apply_update_state(base),
        );

        assert_eq!(updated.update_state, CODEX_UPDATE_STATE_DOWNLOADING);
        assert_eq!(updated.update_progress_percent, None);
    }

    #[test]
    fn update_state_reports_failed_prepared_version_only_when_switch_can_retry() {
        for (prepared, error, expected_prepared) in [
            (false, "download failed", None),
            (true, "switch failed", Some("0.141.0")),
        ] {
            let mut base = status(Some("codex 0.140.0"), true);
            base.latest_version = Some("0.141.0".to_string());
            base.update_available = true;

            let updated = with_update_manager_state(
                CodexUpdateManagerState::Failed {
                    version: "0.141.0".to_string(),
                    error: error.to_string(),
                    prepared,
                },
                || apply_update_state(base),
            );

            assert_eq!(updated.update_state, CODEX_UPDATE_STATE_FAILED);
            assert_eq!(updated.prepared_version.as_deref(), expected_prepared);
            assert_eq!(updated.update_error.as_deref(), Some(error));
        }
    }

    #[test]
    fn same_version_download_is_not_started_twice() {
        assert!(!should_start_download_for(
            "0.141.0",
            &CodexUpdateManagerState::Downloading {
                version: "0.141.0".to_string(),
                progress_percent: Some(10),
            }
        ));
        assert!(should_start_download_for(
            "0.142.0",
            &CodexUpdateManagerState::Downloading {
                version: "0.141.0".to_string(),
                progress_percent: Some(10),
            }
        ));
    }

    #[test]
    fn failed_download_for_same_version_can_start_again() {
        assert!(should_start_download_for(
            "0.141.0",
            &CodexUpdateManagerState::Failed {
                version: "0.141.0".to_string(),
                error: "download failed".to_string(),
                prepared: false,
            }
        ));
        assert!(!should_start_download_for(
            "0.141.0",
            &CodexUpdateManagerState::Failed {
                version: "0.141.0".to_string(),
                error: "switch failed".to_string(),
                prepared: true,
            }
        ));
    }

    #[test]
    fn disk_prepared_update_restores_ready_state() {
        let root = temp_test_dir("restore-ready");
        let updated = with_update_manager_state(CodexUpdateManagerState::Idle, || {
            with_lilia_home(&root, || {
                let prepared = prepared_codex_update("0.141.0");
                fs::create_dir_all(&prepared.install_dir).unwrap();

                restore_ready_update_from_disk_with("0.141.0", |_| Ok(()));

                let mut base = status(Some("codex 0.140.0"), true);
                base.latest_version = Some("0.141.0".to_string());
                base.update_available = true;
                apply_update_state(base)
            })
        });

        assert_eq!(updated.update_state, CODEX_UPDATE_STATE_READY);
        assert_eq!(updated.prepared_version.as_deref(), Some("0.141.0"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_update_without_ready_state_does_not_download() {
        let error = with_update_manager_state(CodexUpdateManagerState::Idle, || {
            install_or_update_codex_app_server().unwrap_err()
        });

        assert!(error.contains("尚未准备好"));
    }

    #[test]
    fn release_asset_selection_prefers_package_layout() {
        let digest = "a".repeat(64);
        let release = release_with_assets(vec![
            release_asset("codex-package-x86_64-pc-windows-msvc.tar.gz", &digest),
            release_asset(CODEX_PACKAGE_CHECKSUM_ASSET, &digest),
            release_asset("codex-npm-win32-x64-0.142.2.tgz", &digest),
        ]);

        let selected = select_codex_release_assets(
            &release,
            "0.142.2",
            CodexTarget {
                package_target: "x86_64-pc-windows-msvc",
                npm_tag: "win32-x64",
            },
        )
        .unwrap();

        assert_eq!(selected.layout, CodexInstallLayout::Package);
        assert_eq!(
            selected.package.name,
            "codex-package-x86_64-pc-windows-msvc.tar.gz"
        );
        assert_eq!(
            selected.checksum.as_ref().map(|asset| asset.name.as_str()),
            Some(CODEX_PACKAGE_CHECKSUM_ASSET)
        );
    }

    #[test]
    fn release_asset_selection_falls_back_to_legacy_npm_layout() {
        let digest = "b".repeat(64);
        let release = release_with_assets(vec![release_asset(
            "codex-npm-win32-x64-0.142.2.tgz",
            &digest,
        )]);

        let selected = select_codex_release_assets(
            &release,
            "0.142.2",
            CodexTarget {
                package_target: "x86_64-pc-windows-msvc",
                npm_tag: "win32-x64",
            },
        )
        .unwrap();

        assert_eq!(selected.layout, CodexInstallLayout::LegacyPlatformNpm);
        assert_eq!(selected.package.name, "codex-npm-win32-x64-0.142.2.tgz");
        assert!(selected.checksum.is_none());
    }

    #[test]
    fn release_asset_selection_reports_missing_platform_assets() {
        let release = release_with_assets(Vec::new());

        let error = select_codex_release_assets(
            &release,
            "0.142.2",
            CodexTarget {
                package_target: "x86_64-pc-windows-msvc",
                npm_tag: "win32-x64",
            },
        )
        .unwrap_err();

        assert!(error.contains("缺少当前平台所需"));
    }

    #[test]
    fn checksum_manifest_returns_digest_for_asset() {
        let digest = "c".repeat(64);
        let manifest = format!("ignored\n{digest}  codex-package-x86_64-pc-windows-msvc.tar.gz\n");

        assert_eq!(
            package_archive_digest(&manifest, "codex-package-x86_64-pc-windows-msvc.tar.gz")
                .unwrap(),
            digest
        );
        assert!(package_archive_digest(&manifest, "missing.tar.gz").is_err());
    }

    #[test]
    fn archive_digest_validation_compares_sha256() {
        let bytes = b"codex-test-archive";
        let digest = sha256_hex(bytes);

        assert!(test_archive_digest(bytes, &digest, "codex.tgz").is_ok());
        assert!(test_archive_digest(bytes, &"0".repeat(64), "codex.tgz").is_err());
    }

    #[test]
    fn prepared_install_dir_detects_package_and_legacy_layouts() {
        let root = temp_test_dir("layout");
        let package_root = root.join("package");
        fs::create_dir_all(package_root.join("release/bin")).unwrap();
        fs::write(package_root.join("release/codex-package.json"), "{}").unwrap();
        assert_eq!(
            prepared_install_dir(&package_root),
            package_root.join("release/bin")
        );

        let legacy_root = root.join("legacy");
        fs::create_dir_all(legacy_root.join("release")).unwrap();
        assert_eq!(
            prepared_install_dir(&legacy_root),
            legacy_root.join("release")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staging_publish_removes_old_final_without_reusing_stale_staging() {
        let root = temp_test_dir("publish");
        let parent = root.join("prepared");
        let final_root = parent.join("0.142.2");
        let stale_staging = parent.join(".staging.old");
        let staging_root = parent.join(".staging.0.142.2.test");
        fs::create_dir_all(final_root.join("release")).unwrap();
        fs::write(final_root.join("release/old.txt"), "old").unwrap();
        fs::create_dir_all(stale_staging.join("release")).unwrap();
        fs::write(stale_staging.join("release/stale.txt"), "stale").unwrap();

        clean_stale_prepared_staging(&parent);
        assert!(!stale_staging.exists());

        fs::create_dir_all(staging_root.join("release")).unwrap();
        fs::write(staging_root.join("release/new.txt"), "new").unwrap();
        assert!(staging_root.exists());

        publish_prepared_root(&staging_root, &final_root).unwrap();

        assert!(final_root.join("release/new.txt").exists());
        assert!(!final_root.join("release/old.txt").exists());
        assert!(!staging_root.exists());

        let _ = fs::remove_dir_all(root);
    }
}
