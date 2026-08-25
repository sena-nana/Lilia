use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use crate::application::{
    DesktopHostContext, DesktopHostError, DesktopHostResult, DesktopUpdateAction,
    DesktopUpdateResult,
};
use minisign_verify::{PublicKey, Signature};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, CONTENT_LENGTH};
use reqwest::Url;
use semver::Version;
use serde::Deserialize;

const STABLE_CHANNEL: &str = "stable";
const DEFAULT_ENDPOINT: &str =
    "https://github.com/sena-nana/LiliaCode/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/sena-nana/LiliaCode/releases";
const USER_AGENT: &str = concat!("LiliaCode/", env!("CARGO_PKG_VERSION"));
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_INSTALLER_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone)]
struct NativeUpdaterConfig {
    endpoint: Url,
    public_key: String,
    current_version: Version,
    targets: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReleaseManifest {
    #[serde(alias = "name")]
    version: String,
    notes: Option<String>,
    platforms: Option<BTreeMap<String, ReleasePlatform>>,
    url: Option<String>,
    signature: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReleasePlatform {
    url: String,
    signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeRelease {
    version: Version,
    notes: Option<String>,
    download_url: Url,
    signature: String,
}

pub fn execute(
    context: &DesktopHostContext,
    action: DesktopUpdateAction,
) -> Result<DesktopHostResult, DesktopHostError> {
    execute_with_progress(context, action, &mut |_| {})
}

pub fn execute_with_progress(
    context: &DesktopHostContext,
    action: DesktopUpdateAction,
    on_download_progress: &mut dyn FnMut(Option<f32>),
) -> Result<DesktopHostResult, DesktopHostError> {
    match action {
        DesktopUpdateAction::Check { channel } => {
            let config = NativeUpdaterConfig::embedded(&channel)?;
            match fetch_release(&config)? {
                Some(release) if release.version > config.current_version => {
                    Ok(DesktopHostResult::Update(DesktopUpdateResult::Available {
                        version: release.version.to_string(),
                        notes: release.notes,
                    }))
                }
                _ => Ok(DesktopHostResult::Update(DesktopUpdateResult::UpToDate)),
            }
        }
        DesktopUpdateAction::Install { version } => {
            let config = NativeUpdaterConfig::embedded(STABLE_CHANNEL)?;
            let expected = parse_version(&version, "native_updater_version_invalid")?;
            if expected <= config.current_version {
                return Err(update_error(
                    "native_updater_version_stale",
                    "所选版本不高于当前版本。",
                    false,
                ));
            }
            let release = fetch_release(&config)?.ok_or_else(|| {
                update_error(
                    "native_updater_release_missing",
                    "更新通道当前没有可安装版本。",
                    true,
                )
            })?;
            if release.version != expected {
                return Err(update_error(
                    "native_updater_version_changed",
                    "可用版本已经变化，请重新检查更新。",
                    true,
                ));
            }
            let package = download_package(&release.download_url, on_download_progress)?;
            verify_package(&package, &release.signature, &config.public_key)?;
            let installer = stage_installer(context, &release.version, &package)?;
            launch_installer(&installer)?;
            Ok(DesktopHostResult::Update(
                DesktopUpdateResult::InstallerLaunched {
                    version: release.version.to_string(),
                },
            ))
        }
    }
}

pub fn is_configured() -> bool {
    cfg!(windows)
        && option_env!("LILIA_UPDATER_PUBKEY").is_some_and(|value| !value.trim().is_empty())
}

impl NativeUpdaterConfig {
    fn embedded(channel: &str) -> Result<Self, DesktopHostError> {
        if channel.trim() != STABLE_CHANNEL {
            return Err(update_error(
                "native_updater_channel_invalid",
                "LiliaCode 仅支持正式更新通道。",
                false,
            ));
        }
        let public_key = option_env!("LILIA_UPDATER_PUBKEY")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if public_key.is_empty() {
            return Err(update_error(
                "native_updater_unconfigured",
                "此构建未配置 LiliaCode 更新签名，自动更新不可用。",
                false,
            ));
        }
        let endpoint = option_env!("LILIA_UPDATER_ENDPOINT")
            .unwrap_or(DEFAULT_ENDPOINT)
            .trim();
        let endpoint = validated_https_url(endpoint, "native_updater_endpoint_invalid")?;
        let current_version = parse_version(
            env!("CARGO_PKG_VERSION"),
            "native_updater_current_version_invalid",
        )?;
        let targets = native_update_targets()?;
        Ok(Self {
            endpoint,
            public_key,
            current_version,
            targets,
        })
    }
}

fn fetch_release(config: &NativeUpdaterConfig) -> Result<Option<NativeRelease>, DesktopHostError> {
    let response = client()?
        .get(config.endpoint.clone())
        .header(ACCEPT, "application/json")
        .send()
        .map_err(|error| {
            update_error(
                "native_updater_check_failed",
                format!("无法检查更新：{error}"),
                true,
            )
        })?;
    if response.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    let response = successful_response(response, "检查更新")?;
    let bytes = read_bounded(response, MAX_MANIFEST_BYTES, "更新清单")?;
    let manifest: ReleaseManifest = serde_json::from_slice(&bytes).map_err(|error| {
        update_error(
            "native_updater_manifest_invalid",
            format!("更新清单格式无效：{error}"),
            false,
        )
    })?;
    select_release(manifest, &config.targets).map(Some)
}

fn select_release(
    manifest: ReleaseManifest,
    targets: &[String],
) -> Result<NativeRelease, DesktopHostError> {
    let version = parse_version(&manifest.version, "native_updater_manifest_version_invalid")?;
    let platform = match manifest.platforms {
        Some(platforms) => targets
            .iter()
            .find_map(|target| platforms.get(target).cloned())
            .ok_or_else(|| {
                update_error(
                    "native_updater_target_missing",
                    "更新清单不包含当前 Windows 架构。",
                    false,
                )
            })?,
        None => ReleasePlatform {
            url: manifest.url.ok_or_else(|| {
                update_error(
                    "native_updater_download_missing",
                    "更新清单缺少下载地址。",
                    false,
                )
            })?,
            signature: manifest.signature.ok_or_else(|| {
                update_error(
                    "native_updater_signature_missing",
                    "更新清单缺少签名。",
                    false,
                )
            })?,
        },
    };
    if platform.signature.trim().is_empty() {
        return Err(update_error(
            "native_updater_signature_missing",
            "更新清单缺少签名。",
            false,
        ));
    }
    Ok(NativeRelease {
        version,
        notes: manifest.notes,
        download_url: validated_https_url(&platform.url, "native_updater_download_url_invalid")?,
        signature: platform.signature,
    })
}

fn download_package(
    url: &Url,
    on_download_progress: &mut dyn FnMut(Option<f32>),
) -> Result<Vec<u8>, DesktopHostError> {
    let response = client()?
        .get(url.clone())
        .header(ACCEPT, "application/octet-stream")
        .send()
        .map_err(|error| {
            update_error(
                "native_updater_download_failed",
                format!("无法下载更新：{error}"),
                true,
            )
        })?;
    let response = successful_response(response, "下载更新")?;
    read_bounded_with_progress(response, MAX_PACKAGE_BYTES, "更新包", on_download_progress)
}

fn client() -> Result<Client, DesktopHostError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() == "https" {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| {
            update_error(
                "native_updater_client_failed",
                format!("无法准备更新连接：{error}"),
                true,
            )
        })
}

fn successful_response(response: Response, operation: &str) -> Result<Response, DesktopHostError> {
    if response.url().scheme() != "https" {
        return Err(update_error(
            "native_updater_insecure_redirect",
            "更新服务返回了不安全的重定向。",
            false,
        ));
    }
    if !response.status().is_success() {
        return Err(update_error(
            "native_updater_http_failed",
            format!("{operation}失败（HTTP {}）。", response.status()),
            response.status().is_server_error(),
        ));
    }
    Ok(response)
}

fn read_bounded(
    response: Response,
    maximum: u64,
    label: &str,
) -> Result<Vec<u8>, DesktopHostError> {
    read_bounded_with_progress(response, maximum, label, &mut |_| {})
}

fn read_bounded_with_progress(
    mut response: Response,
    maximum: u64,
    label: &str,
    on_progress: &mut dyn FnMut(Option<f32>),
) -> Result<Vec<u8>, DesktopHostError> {
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if content_length.is_some_and(|length| length > maximum) {
        return Err(update_error(
            "native_updater_payload_too_large",
            format!("{label}超过允许大小。"),
            false,
        ));
    }
    on_progress(content_length.filter(|length| *length > 0).map(|_| 0.0));
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = response.read(&mut chunk).map_err(|error| {
            update_error(
                "native_updater_payload_read_failed",
                format!("读取{label}失败：{error}"),
                true,
            )
        })?;
        if read == 0 {
            break;
        }
        if bytes.len().saturating_add(read) as u64 > maximum {
            return Err(update_error(
                "native_updater_payload_too_large",
                format!("{label}超过允许大小。"),
                false,
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(length) = content_length.filter(|length| *length > 0) {
            on_progress(Some((bytes.len() as f32 / length as f32).clamp(0.0, 1.0)));
        }
    }
    on_progress(Some(1.0));
    Ok(bytes)
}

fn verify_package(
    package: &[u8],
    encoded_signature: &str,
    encoded_public_key: &str,
) -> Result<(), DesktopHostError> {
    let public_key = decode_base64_text(encoded_public_key, "更新公钥")?;
    let public_key = PublicKey::decode(&public_key).map_err(|error| {
        update_error(
            "native_updater_public_key_invalid",
            format!("更新公钥无效：{error}"),
            false,
        )
    })?;
    let signature = decode_base64_text(encoded_signature, "更新签名")?;
    let signature = Signature::decode(&signature).map_err(|error| {
        update_error(
            "native_updater_signature_invalid",
            format!("更新签名无效：{error}"),
            false,
        )
    })?;
    public_key
        .verify(package, &signature, true)
        .map_err(|error| {
            update_error(
                "native_updater_signature_mismatch",
                format!("更新包签名验证失败：{error}"),
                false,
            )
        })
}

fn decode_base64_text(value: &str, label: &str) -> Result<String, DesktopHostError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| {
            update_error(
                "native_updater_signature_encoding_invalid",
                format!("{label}编码无效：{error}"),
                false,
            )
        })?;
    String::from_utf8(decoded).map_err(|_| {
        update_error(
            "native_updater_signature_text_invalid",
            format!("{label}不是有效文本。"),
            false,
        )
    })
}

fn stage_installer(
    context: &DesktopHostContext,
    version: &Version,
    package: &[u8],
) -> Result<PathBuf, DesktopHostError> {
    let updates_root = context.home.join("updates");
    fs::create_dir_all(&updates_root).map_err(|error| {
        update_error(
            "native_updater_stage_failed",
            format!("无法创建更新目录：{error}"),
            true,
        )
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stage = updates_root.join(format!("{version}-{}-{nonce}", std::process::id()));
    fs::create_dir(&stage).map_err(|error| {
        update_error(
            "native_updater_stage_failed",
            format!("无法准备更新目录：{error}"),
            true,
        )
    })?;
    let result = if package.starts_with(b"PK\x03\x04") {
        extract_nsis_zip(&stage, package)
    } else if package.starts_with(b"MZ") {
        let name = format!("LiliaCode_{version}_x64-setup.exe");
        write_installer(&stage.join(name), package)
    } else {
        Err(update_error(
            "native_updater_package_invalid",
            "更新包不是受支持的 NSIS 安装器。",
            false,
        ))
    };
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

fn extract_nsis_zip(stage: &Path, package: &[u8]) -> Result<PathBuf, DesktopHostError> {
    let cursor = Cursor::new(package);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|error| {
        update_error(
            "native_updater_archive_invalid",
            format!("更新压缩包无效：{error}"),
            false,
        )
    })?;
    let mut selected = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            update_error(
                "native_updater_archive_invalid",
                format!("无法读取更新压缩包：{error}"),
                false,
            )
        })?;
        let Some(path) = entry.enclosed_name() else {
            return Err(update_error(
                "native_updater_archive_path_invalid",
                "更新压缩包包含不安全路径。",
                false,
            ));
        };
        let is_nsis = entry.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
        if is_nsis {
            if selected.replace(index).is_some() {
                return Err(update_error(
                    "native_updater_archive_ambiguous",
                    "更新压缩包包含多个安装器。",
                    false,
                ));
            }
            if entry.size() > MAX_INSTALLER_BYTES {
                return Err(update_error(
                    "native_updater_installer_too_large",
                    "更新安装器超过允许大小。",
                    false,
                ));
            }
        }
    }
    let index = selected.ok_or_else(|| {
        update_error(
            "native_updater_installer_missing",
            "更新压缩包不包含 NSIS 安装器。",
            false,
        )
    })?;
    let mut entry = archive.by_index(index).map_err(|error| {
        update_error(
            "native_updater_archive_invalid",
            format!("无法读取更新安装器：{error}"),
            false,
        )
    })?;
    let name = entry
        .enclosed_name()
        .and_then(|path| path.file_name().map(ToOwned::to_owned))
        .ok_or_else(|| {
            update_error(
                "native_updater_installer_name_invalid",
                "更新安装器文件名无效。",
                false,
            )
        })?;
    let path = stage.join(name);
    let mut magic = [0_u8; 2];
    entry.read_exact(&mut magic).map_err(|_| {
        update_error(
            "native_updater_package_invalid",
            "更新压缩包中的安装器不是有效的 Windows 可执行文件。",
            false,
        )
    })?;
    if magic != *b"MZ" {
        return Err(update_error(
            "native_updater_package_invalid",
            "更新压缩包中的安装器不是有效的 Windows 可执行文件。",
            false,
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            update_error(
                "native_updater_installer_write_failed",
                format!("无法写入更新安装器：{error}"),
                true,
            )
        })?;
    file.write_all(&magic)
        .and_then(|_| std::io::copy(&mut entry, &mut file).map(|_| ()))
        .map_err(|error| {
            update_error(
                "native_updater_installer_write_failed",
                format!("无法解压更新安装器：{error}"),
                true,
            )
        })?;
    file.sync_all().map_err(|error| {
        update_error(
            "native_updater_installer_write_failed",
            format!("无法保存更新安装器：{error}"),
            true,
        )
    })?;
    Ok(path)
}

fn write_installer(path: &Path, package: &[u8]) -> Result<PathBuf, DesktopHostError> {
    if package.len() as u64 > MAX_INSTALLER_BYTES {
        return Err(update_error(
            "native_updater_installer_too_large",
            "更新安装器超过允许大小。",
            false,
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            update_error(
                "native_updater_installer_write_failed",
                format!("无法写入更新安装器：{error}"),
                true,
            )
        })?;
    file.write_all(package).map_err(|error| {
        update_error(
            "native_updater_installer_write_failed",
            format!("无法写入更新安装器：{error}"),
            true,
        )
    })?;
    file.sync_all().map_err(|error| {
        update_error(
            "native_updater_installer_write_failed",
            format!("无法保存更新安装器：{error}"),
            true,
        )
    })?;
    Ok(path.to_path_buf())
}

#[cfg(windows)]
fn launch_installer(path: &Path) -> Result<(), DesktopHostError> {
    let update_pid = format!("/UPDATEPID={}", std::process::id());
    Command::new(path)
        .args(["/passive", "/UPDATE"])
        .arg(update_pid)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            update_error(
                "native_updater_installer_launch_failed",
                format!("无法启动更新安装器：{error}"),
                true,
            )
        })
}

#[cfg(not(windows))]
fn launch_installer(_path: &Path) -> Result<(), DesktopHostError> {
    Err(update_error(
        "native_updater_platform_unsupported",
        "LiliaCode 自动更新当前仅支持 Windows 11。",
        false,
    ))
}

fn native_update_targets() -> Result<Vec<String>, DesktopHostError> {
    if !cfg!(windows) {
        return Err(update_error(
            "native_updater_platform_unsupported",
            "LiliaCode 自动更新当前仅支持 Windows 11。",
            false,
        ));
    }
    let architecture = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "x86" => "i686",
        _ => {
            return Err(update_error(
                "native_updater_arch_unsupported",
                "当前处理器架构不支持自动更新。",
                false,
            ));
        }
    };
    Ok(vec![
        format!("windows-{architecture}-nsis"),
        format!("windows-{architecture}"),
    ])
}

fn parse_version(value: &str, code: &'static str) -> Result<Version, DesktopHostError> {
    Version::parse(value.trim().trim_start_matches('v'))
        .map_err(|error| update_error(code, format!("版本号无效：{error}"), false))
}

fn validated_https_url(value: &str, code: &'static str) -> Result<Url, DesktopHostError> {
    let url = Url::parse(value.trim())
        .map_err(|error| update_error(code, format!("更新地址无效：{error}"), false))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(update_error(
            code,
            "更新地址必须是不含凭据的 HTTPS 地址。",
            false,
        ));
    }
    Ok(url)
}

fn update_error(
    code: &'static str,
    message: impl Into<String>,
    retryable: bool,
) -> DesktopHostError {
    DesktopHostError::new(code, message, retryable)
}

#[cfg(test)]
mod tests {
    use zip::write::SimpleFileOptions;

    use super::*;

    #[test]
    fn static_manifest_prefers_the_nsis_target_and_normalizes_v_prefix() {
        let manifest: ReleaseManifest = serde_json::from_value(serde_json::json!({
            "version": "v0.2.0",
            "notes": "LiliaCode release",
            "platforms": {
                "windows-x86_64": {
                    "url": "https://example.com/generic.zip",
                    "signature": "generic"
                },
                "windows-x86_64-nsis": {
                    "url": "https://example.com/nsis.zip",
                    "signature": "nsis"
                }
            }
        }))
        .unwrap();

        let release = select_release(
            manifest,
            &["windows-x86_64-nsis".into(), "windows-x86_64".into()],
        )
        .unwrap();
        assert_eq!(release.version, Version::new(0, 2, 0));
        assert_eq!(
            release.download_url.as_str(),
            "https://example.com/nsis.zip"
        );
        assert_eq!(release.signature, "nsis");
        assert_eq!(release.notes.as_deref(), Some("LiliaCode release"));
    }

    #[test]
    fn update_urls_reject_credentials_and_insecure_transport() {
        assert_eq!(
            validated_https_url("http://example.com/update", "invalid")
                .unwrap_err()
                .code,
            "invalid"
        );
        assert_eq!(
            validated_https_url("https://token@example.com/update", "invalid")
                .unwrap_err()
                .code,
            "invalid"
        );
    }

    #[test]
    fn signed_package_is_accepted_and_tampering_is_rejected() {
        let public_key = base64::engine::general_purpose::STANDARD.encode(
            "untrusted comment: minisign public key E7620F1842B4E81F\n\
             RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3",
        );
        let signature = base64::engine::general_purpose::STANDARD.encode(
            "untrusted comment: signature from minisign secret key\n\
             RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\n\
             trusted comment: timestamp:1555779966\tfile:test\n\
             QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==",
        );

        verify_package(b"test", &signature, &public_key).unwrap();
        assert_eq!(
            verify_package(b"Test", &signature, &public_key)
                .unwrap_err()
                .code,
            "native_updater_signature_mismatch"
        );
    }

    #[test]
    fn direct_nsis_package_is_staged_below_the_lilia_home() {
        let home = std::env::temp_dir().join(format!(
            "lilia-native-updater-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let context = DesktopHostContext {
            home: home.clone(),
            instance_identity: "liliacode.test".into(),
        };

        let installer = stage_installer(&context, &Version::new(0, 2, 0), b"MZfixture").unwrap();
        assert!(installer.starts_with(home.join("updates")));
        assert_eq!(fs::read(&installer).unwrap(), b"MZfixture");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn zipped_nsis_package_extracts_only_the_pe_installer() {
        let context = updater_context("valid-zip");
        let package = zip_package(&[
            ("notes.txt", b"release notes"),
            ("nested/setup.exe", b"MZinstaller"),
        ]);

        let installer = stage_installer(&context, &Version::new(0, 2, 0), &package).unwrap();
        assert!(installer.starts_with(context.home.join("updates")));
        assert_eq!(installer.file_name().unwrap(), "setup.exe");
        assert_eq!(fs::read(&installer).unwrap(), b"MZinstaller");
        fs::remove_dir_all(context.home).unwrap();
    }

    #[test]
    fn zip_package_rejects_unsafe_or_ambiguous_installers_without_staging_residue() {
        for (label, package, expected_code) in [
            (
                "unsafe",
                zip_package(&[("../escape.exe", b"MZescape")]),
                "native_updater_archive_path_invalid",
            ),
            (
                "ambiguous",
                zip_package(&[("one.exe", b"MZone"), ("two.exe", b"MZtwo")]),
                "native_updater_archive_ambiguous",
            ),
            (
                "not-pe",
                zip_package(&[("setup.exe", b"not a PE executable")]),
                "native_updater_package_invalid",
            ),
        ] {
            let context = updater_context(label);
            assert_eq!(
                stage_installer(&context, &Version::new(0, 2, 0), &package)
                    .unwrap_err()
                    .code,
                expected_code
            );
            let updates = context.home.join("updates");
            assert_eq!(fs::read_dir(&updates).unwrap().count(), 0);
            fs::remove_dir_all(context.home).unwrap();
        }
    }

    fn updater_context(label: &str) -> DesktopHostContext {
        DesktopHostContext {
            home: std::env::temp_dir().join(format!(
                "lilia-native-updater-{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )),
            instance_identity: format!("liliacode.{label}"),
        }
    }

    fn zip_package(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            archive
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(content).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }
}
