use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopGitHubClientIdSource {
    None,
    Bundled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubBindingMetadata {
    pub login: String,
    pub avatar_url: Option<String>,
    pub bound_at: i64,
    pub scopes: Vec<String>,
    pub client_id_source: DesktopGitHubClientIdSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubBindingStatus {
    pub state: String,
    pub client_id_configured: bool,
    pub client_id_source: DesktopGitHubClientIdSource,
    pub binding: Option<DesktopGitHubBindingMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubDeviceFlowStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: i64,
    pub interval_seconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubDeviceFlowPollResult {
    pub status: String,
    pub interval_seconds: i64,
    pub binding_status: Option<DesktopGitHubBindingStatus>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubRepoSummary {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner_login: String,
    pub private: bool,
    pub description: Option<String>,
    pub default_branch: Option<String>,
    pub updated_at: String,
    pub clone_url: String,
    pub html_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGitHubRepoPage {
    pub items: Vec<DesktopGitHubRepoSummary>,
    pub next_page: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopGitHubError {
    #[error("GitHub Client ID is not configured")]
    ClientIdMissing,
    #[error("GitHub request failed while {operation}: {message}")]
    Request {
        operation: &'static str,
        message: String,
    },
    #[error("GitHub returned HTTP {status} while {operation}")]
    Http {
        operation: &'static str,
        status: u16,
    },
    #[error("GitHub response was invalid while {operation}: {message}")]
    Response {
        operation: &'static str,
        message: String,
    },
    #[error("GitHub binding is required")]
    Unbound,
    #[error("GitHub binding for `{login}` is no longer authorized")]
    BindingExpired { login: String },
    #[error("GitHub device code is invalid")]
    InvalidDeviceCode,
    #[error("GitHub repository must be owner/repo or a github.com repository URL")]
    InvalidRepository,
    #[error("GitHub binding persistence failed: {0}")]
    Persistence(String),
    #[error("GitHub credential storage failed: {0}")]
    Credential(String),
}
