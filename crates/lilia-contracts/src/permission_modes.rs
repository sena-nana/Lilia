//! Permission mode contract (`permission-modes.json`) consumers.
//!
//! The JSON is the product source of truth for mode labels and
//! `toolApprovalOverrides`. Runtime code must not leave those overrides as
//! documentation-only: classify high-risk tools here and consult the helpers
//! from AgentKit / remote boundaries.

use serde::Deserialize;
use serde_json::Value;

/// Embedded permission-modes contract.
pub const PERMISSION_MODES_JSON: &str =
    include_str!("../contracts/permission-modes.json");

const HIGH_RISK_TOOL_CLASSES: &[&str] = &["process", "network", "http", "browser", "shell"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionModesContract {
    #[serde(default)]
    high_risk_tool_classes: Vec<String>,
    #[serde(default)]
    runtime_mappings: RuntimeMappings,
    #[serde(default)]
    remote_policy: RemotePolicy,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeMappings {
    #[serde(rename = "native-agentkit", default)]
    native_agentkit: NativeAgentkitMappings,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct NativeAgentkitMappings {
    #[serde(default)]
    full: ModeMapping,
    #[serde(default)]
    free: ModeMapping,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModeMapping {
    #[serde(default)]
    tool_approval_overrides: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemotePolicy {
    #[serde(default)]
    allow_full: bool,
    #[serde(default = "default_ask")]
    full_maps_to: String,
    #[serde(default = "default_ask")]
    free_maps_to: String,
}

fn default_ask() -> String {
    "ask".into()
}

fn contract() -> PermissionModesContract {
    serde_json::from_str(PERMISSION_MODES_JSON)
        .expect("permission-modes.json must be valid")
}

/// High-risk tool class names from the contract (`process`, `shell`, …).
pub fn high_risk_tool_classes() -> Vec<&'static str> {
    // Static slice keeps call sites allocation-free; tests assert parity with JSON.
    let _ = &contract().high_risk_tool_classes;
    HIGH_RISK_TOOL_CLASSES.to_vec()
}

/// Whether `tool_class` requires an explicit approval override under `mode`
/// (`full` / `free`) per `toolApprovalOverrides`.
pub fn tool_approval_override_requires_approval(mode: &str, tool_class: &str) -> bool {
    let c = contract();
    let mapping = match mode {
        "full" => &c.runtime_mappings.native_agentkit.full,
        "free" => &c.runtime_mappings.native_agentkit.free,
        _ => return mode == "ask" || mode == "readonly",
    };
    mapping
        .tool_approval_overrides
        .get(tool_class)
        .copied()
        .unwrap_or(false)
}

/// Classify a tool name into a high-risk class when one of the contract
/// classes appears as a path segment (`computer.shell.exec` → `shell`).
pub fn classify_high_risk_tool(tool: &str) -> Option<&'static str> {
    let normalized = tool.to_ascii_lowercase();
    if normalized == "computer.shell.exec" {
        return Some("shell");
    }
    if normalized == "computer.browser.snapshot" {
        return Some("browser");
    }
    for class in HIGH_RISK_TOOL_CLASSES {
        let hit = normalized
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|part| part == *class);
        if hit {
            return Some(*class);
        }
    }
    None
}

/// True when Full-mode auto-approval must not apply to this tool name.
pub fn tool_requires_explicit_approval_under_full(tool: &str) -> bool {
    classify_high_risk_tool(tool).is_some_and(|class| {
        tool_approval_override_requires_approval("full", class)
    })
}

/// Remote composers cannot elevate to Full; map `full`/`free` → Ask.
pub fn remote_permission_mode(requested: &str) -> &'static str {
    let policy = contract().remote_policy;
    let map_to_ask = |target: &str| -> &'static str {
        if target == "readonly" {
            "readonly"
        } else if target == "full" && policy.allow_full {
            "full"
        } else {
            "ask"
        }
    };
    match requested {
        "full" if policy.allow_full => "full",
        "full" => map_to_ask(&policy.full_maps_to),
        "free" => map_to_ask(&policy.free_maps_to),
        "readonly" => "readonly",
        "ask" => "ask",
        _ => "ask",
    }
}

/// Raw contract JSON (for UI / diagnostics).
pub fn permission_modes_contract_value() -> Value {
    serde_json::from_str(PERMISSION_MODES_JSON).expect("permission-modes.json must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_json_lists_high_risk_classes_and_full_overrides() {
        let value = permission_modes_contract_value();
        let classes = value["highRiskToolClasses"]
            .as_array()
            .expect("highRiskToolClasses");
        let parsed = contract();
        assert_eq!(
            parsed.high_risk_tool_classes.len(),
            HIGH_RISK_TOOL_CLASSES.len(),
            "JSON highRiskToolClasses must stay in sync with HIGH_RISK_TOOL_CLASSES"
        );
        for class in HIGH_RISK_TOOL_CLASSES {
            assert!(
                classes.iter().any(|v| v.as_str() == Some(*class)),
                "missing class {class}"
            );
            assert!(
                parsed
                    .high_risk_tool_classes
                    .iter()
                    .any(|c| c == class),
                "parsed contract missing {class}"
            );
            assert!(tool_approval_override_requires_approval("full", class));
            assert!(tool_approval_override_requires_approval("free", class));
        }
        assert_eq!(value["remotePolicy"]["allowFull"], false);
        assert_eq!(value["remotePolicy"]["fullMapsTo"], "ask");
        assert_eq!(high_risk_tool_classes(), HIGH_RISK_TOOL_CLASSES);
    }

    #[test]
    fn full_mode_marks_shell_and_browser_as_explicit_approval() {
        assert!(tool_requires_explicit_approval_under_full("computer.shell.exec"));
        assert!(tool_requires_explicit_approval_under_full("computer.browser.snapshot"));
        assert!(tool_requires_explicit_approval_under_full("host.process.run"));
        assert!(tool_requires_explicit_approval_under_full("tool.network.fetch"));
        assert!(!tool_requires_explicit_approval_under_full("computer.fs.write"));
        assert!(!tool_requires_explicit_approval_under_full("computer.fs.read"));
        assert_eq!(classify_high_risk_tool("computer.shell.exec"), Some("shell"));
    }

    #[test]
    fn remote_full_and_free_force_downgrade_to_ask() {
        assert_eq!(remote_permission_mode("full"), "ask");
        assert_eq!(remote_permission_mode("free"), "ask");
        assert_eq!(remote_permission_mode("ask"), "ask");
        assert_eq!(remote_permission_mode("readonly"), "readonly");
        assert_eq!(remote_permission_mode("unknown"), "ask");
    }
}
