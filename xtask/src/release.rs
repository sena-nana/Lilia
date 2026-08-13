use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

use crate::{command, repo_root, run, signing, Result, XtaskError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdatePlatform {
    pub url: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateManifest {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    pub platforms: BTreeMap<String, UpdatePlatform>,
}

pub fn windows(arguments: &[String]) -> Result {
    if !cfg!(target_os = "windows") {
        return Err(XtaskError::blocker(
            "windows_required",
            "Windows release requires a Windows host with NSIS",
        ));
    }
    let root = repo_root()?;
    let version = parse_tag(required_option(arguments, "--tag")?)?;
    let output = root.join("artifacts/windows").join(version.to_string());
    fs::create_dir_all(&output).map_err(|error| {
        XtaskError::io(
            "release_directory_failed",
            "create release directory",
            error,
        )
    })?;
    run(
        command("cargo").current_dir(&root).args([
            "build",
            "--locked",
            "--release",
            "-p",
            "lilia-desktop",
        ]),
        "build Windows desktop release",
    )?;

    let binary = root.join("target/release/liliacode.exe");
    let host = root.join("target/release/liliacode_host.dll");
    require_file(&binary, "desktop release binary")?;
    require_file(&host, "desktop host library")?;
    ensure_debug_markers_absent(&binary)?;

    let installer = output.join(format!("LiliaCode-{}-setup.exe", version));
    let icon = root.join("apps/desktop/assets/icons/icon.ico");
    let nsis = env::var_os("LILIA_NSIS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("makensis.exe"));
    run(
        command(&nsis).current_dir(&root).args([
            "/INPUTCHARSET",
            "UTF8",
            &format!("/DAPP_VERSION={version}"),
            &format!("/DDESKTOP_BINARY={}", binary.display()),
            &format!("/DHOST_LIBRARY={}", host.display()),
            &format!("/DOUTPUT_FILE={}", installer.display()),
            &format!("/DAPP_ICON={}", icon.display()),
            &root
                .join("apps/desktop/windows/installer.nsi")
                .display()
                .to_string(),
        ]),
        "NSIS compiler",
    )?;
    require_file(&installer, "NSIS installer")?;

    let package = output.join(format!("LiliaCode-{}-windows-x86_64.zip", version));
    create_update_archive(&installer, &package)?;
    let password = env::var("LILIA_SIGNING_PASSWORD").ok();
    let signature = if let Ok(private_key) = env::var("LILIA_SIGNING_PRIVATE_KEY") {
        signing::sign_file_with_private_key(&package, &private_key, password)?
    } else if let Some(key) = env::var_os("LILIA_SIGNING_KEY_PATH").map(PathBuf::from) {
        signing::sign_file(&package, &key, password)?
    } else {
        return Err(XtaskError::blocker(
            "signing_key_missing",
            "LILIA_SIGNING_PRIVATE_KEY or LILIA_SIGNING_KEY_PATH is required",
        ));
    };
    fs::write(package.with_extension("zip.sig"), &signature).map_err(|error| {
        XtaskError::io("signature_write_failed", "write updater signature", error)
    })?;
    let base_url = env::var("LILIA_UPDATER_BASE_URL").map_err(|_| {
        XtaskError::blocker("updater_url_missing", "LILIA_UPDATER_BASE_URL is required")
    })?;
    let manifest = build_manifest(
        &version,
        env::var("LILIA_RELEASE_NOTES").unwrap_or_default(),
        env::var("LILIA_RELEASE_PUB_DATE").unwrap_or_else(|_| {
            time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .expect("UTC release timestamp must format")
        }),
        &base_url,
        package.file_name().unwrap().to_string_lossy().as_ref(),
        signature,
    )?;
    let manifest_path = output.join("latest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .map_err(|error| XtaskError::io("manifest_write_failed", "write latest.json", error))?;
    println!("release windows: ok ({})", output.display());
    Ok(())
}

pub(crate) fn parse_tag(tag: &str) -> Result<Version> {
    if !tag.starts_with('v') {
        return Err(XtaskError::failure(
            "release_tag_invalid",
            "release tag must start with `v`",
        ));
    }
    Version::parse(&tag[1..])
        .map_err(|error| XtaskError::failure("release_tag_invalid", error.to_string()))
}

fn required_option<'a>(arguments: &'a [String], name: &str) -> Result<&'a str> {
    let index = arguments
        .iter()
        .position(|value| value == name)
        .ok_or_else(|| {
            XtaskError::failure("release_option_missing", format!("{name} is required"))
        })?;
    arguments
        .get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| {
            XtaskError::failure("release_option_missing", format!("{name} requires a value"))
        })
}

fn build_manifest(
    version: &Version,
    notes: String,
    pub_date: String,
    base_url: &str,
    package_name: &str,
    signature: String,
) -> Result<UpdateManifest> {
    let base = base_url.trim_end_matches('/');
    if !base.starts_with("https://") {
        return Err(XtaskError::failure(
            "updater_url_invalid",
            "LILIA_UPDATER_BASE_URL must use HTTPS",
        ));
    }
    let platform = UpdatePlatform {
        url: format!("{base}/{package_name}"),
        signature: base64::engine::general_purpose::STANDARD.encode(signature),
    };
    Ok(UpdateManifest {
        version: version.to_string(),
        notes,
        pub_date,
        platforms: BTreeMap::from([("windows-x86_64".to_owned(), platform)]),
    })
}

fn create_update_archive(installer: &Path, output: &Path) -> Result {
    let file = File::create(output).map_err(|error| {
        XtaskError::io(
            "archive_create_failed",
            &output.display().to_string(),
            error,
        )
    })?;
    let mut archive = zip::ZipWriter::new(file);
    let name = installer
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            XtaskError::failure("installer_name_invalid", "installer filename is not UTF-8")
        })?;
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|error| XtaskError::failure("archive_write_failed", error.to_string()))?;
    let mut input = File::open(installer)
        .map_err(|error| XtaskError::io("installer_read_failed", "open installer", error))?;
    std::io::copy(&mut input, &mut archive).map_err(|error| {
        XtaskError::io("archive_write_failed", "write installer archive", error)
    })?;
    archive
        .finish()
        .map_err(|error| XtaskError::failure("archive_write_failed", error.to_string()))?;
    Ok(())
}

fn ensure_debug_markers_absent(path: &Path) -> Result {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| {
            XtaskError::io(
                "release_binary_read_failed",
                "inspect release binary",
                error,
            )
        })?;
    for marker in [b"LILIA_AGENT_DEBUG".as_slice()] {
        if bytes.windows(marker.len()).any(|window| window == marker) {
            return Err(XtaskError::failure(
                "release_contains_debug_marker",
                format!(
                    "{} contains {}",
                    path.display(),
                    String::from_utf8_lossy(marker)
                ),
            ));
        }
    }
    let digest = Sha256::digest(&bytes);
    println!("release binary sha256: {}", hex::encode(digest));
    Ok(())
}

fn require_file(path: &Path, label: &str) -> Result {
    path.is_file().then_some(()).ok_or_else(|| {
        XtaskError::failure(
            "release_artifact_missing",
            format!("{label} is missing: {}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_matches_native_updater_platform_shape() {
        let manifest = build_manifest(
            &Version::new(1, 2, 3),
            "notes".into(),
            "2026-08-13T00:00:00Z".into(),
            "https://downloads.example/lilia/",
            "update.zip",
            "signature".into(),
        )
        .unwrap();
        let value = serde_json::to_value(manifest).unwrap();
        assert_eq!(value["version"], "1.2.3");
        assert_eq!(
            value["platforms"]["windows-x86_64"]["url"],
            "https://downloads.example/lilia/update.zip"
        );
        assert_eq!(
            value["platforms"]["windows-x86_64"]["signature"],
            "c2lnbmF0dXJl"
        );
    }

    #[test]
    fn tag_requires_v_prefixed_semver() {
        assert_eq!(parse_tag("v1.2.3").unwrap(), Version::new(1, 2, 3));
        assert_eq!(parse_tag("1.2.3").unwrap_err().code, "release_tag_invalid");
    }
}
