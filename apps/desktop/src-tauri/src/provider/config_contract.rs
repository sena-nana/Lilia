use std::sync::OnceLock;

use serde::Deserialize;

const PERMISSION_MODES_JSON: &str =
    include_str!("../../../../../packages/contracts/src/permission-modes.json");

static PERMISSION_MODES: OnceLock<PermissionModesManifest> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionModesManifest {
    permission_modes: Vec<String>,
    default_permission_mode: String,
}

fn permission_modes_manifest() -> &'static PermissionModesManifest {
    PERMISSION_MODES.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            PERMISSION_MODES_JSON,
            "permission-modes.json",
        )
    })
}

/// Official Codex settings profiles removed; keep empty list for callers.
pub(super) fn codex_settings_profiles() -> &'static [String] {
    &[]
}

pub(super) fn default_codex_settings_profile() -> &'static str {
    "default"
}

pub(super) fn permission_modes() -> &'static [String] {
    &permission_modes_manifest().permission_modes
}

pub(super) fn default_permission_mode() -> &'static str {
    &permission_modes_manifest().default_permission_mode
}
