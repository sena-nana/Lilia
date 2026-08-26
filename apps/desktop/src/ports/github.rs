use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lilia_storage::SqliteAgentRuntimeStateStore;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{ACCEPT, LINK, USER_AGENT};
use serde::{Deserialize, Serialize};

use crate::application::{
    DesktopApplication, DesktopCredentialAction, DesktopHostAction,
    DesktopHostResult, DesktopSecret,
};
use crate::application::{GitHubBindingChanged};

pub use lilia_feature_github::{
    DesktopGitHubBindingMetadata, DesktopGitHubBindingStatus, DesktopGitHubClientIdSource,
    DesktopGitHubDeviceFlowPollResult, DesktopGitHubDeviceFlowStart, DesktopGitHubError,
    DesktopGitHubRepoPage, DesktopGitHubRepoSummary,
};

const GITHUB_CLIENT_ID: &str = "Ov23liJWTEjz4jgqx19u";
const GITHUB_SCOPE: &str = "repo read:user";
const GITHUB_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_USER_AGENT: &str = "LiliaCode/0.1";
const GITHUB_BINDING_SETTINGS_KEY: &str = "desktop.github.binding.v1";
const GITHUB_TOKEN_KEY: &str = "github.oauth.token";
const GITHUB_BINDING_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGitHubBinding {
    schema_version: u32,
    binding: DesktopGitHubBindingMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserResponse {
    login: String,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RepoOwnerResponse {
    login: String,
}

#[derive(Debug, Deserialize)]
struct RepoResponse {
    id: u64,
    name: String,
    full_name: String,
    private: bool,
    description: Option<String>,
    default_branch: Option<String>,
    updated_at: String,
    clone_url: String,
    html_url: String,
    owner: RepoOwnerResponse,
}

#[derive(Clone)]
struct GitHubEndpoints {
    device_code: String,
    access_token: String,
    user: String,
    repositories: String,
}

impl Default for GitHubEndpoints {
    fn default() -> Self {
        Self {
            device_code: "https://github.com/login/device/code".to_owned(),
            access_token: "https://github.com/login/oauth/access_token".to_owned(),
            user: "https://api.github.com/user".to_owned(),
            repositories: "https://api.github.com/user/repos".to_owned(),
        }
    }
}

impl GitHubEndpoints {
    fn current() -> Self {
        #[cfg(debug_assertions)]
        if std::env::var_os("LILIA_AGENT_DEBUG").is_some() {
            if let Ok(base) = std::env::var("LILIA_DESKTOP_GITHUB_FIXTURE_URL") {
                let base = base.trim().trim_end_matches('/');
                if base.starts_with("http://127.0.0.1:") || base.starts_with("http://localhost:") {
                    return Self {
                        device_code: format!("{base}/login/device/code"),
                        access_token: format!("{base}/login/oauth/access_token"),
                        user: format!("{base}/user"),
                        repositories: format!("{base}/user/repos"),
                    };
                }
            }
        }
        Self::default()
    }
}

impl DesktopApplication {
    pub fn github_binding_status(&self) -> Result<DesktopGitHubBindingStatus, DesktopGitHubError> {
        let binding = self.reconcile_github_binding(true)?.0;
        Ok(binding_status(binding))
    }

    pub fn start_github_device_flow(
        &self,
    ) -> Result<DesktopGitHubDeviceFlowStart, DesktopGitHubError> {
        let client_id = github_client_id().ok_or(DesktopGitHubError::ClientIdMissing)?;
        let endpoints = GitHubEndpoints::current();
        let response = github_client()?
            .post(endpoints.device_code)
            .header(USER_AGENT, GITHUB_USER_AGENT)
            .header(ACCEPT, "application/json")
            .form(&[("client_id", client_id), ("scope", GITHUB_SCOPE)])
            .send()
            .map_err(|error| request_error("starting device authorization", error))?;
        if !response.status().is_success() {
            return Err(http_error(
                "starting device authorization",
                response.status(),
            ));
        }
        let body = response
            .json::<DeviceCodeResponse>()
            .map_err(|error| response_error("starting device authorization", error))?;
        if body.device_code.trim().is_empty()
            || body.user_code.trim().is_empty()
            || body.verification_uri.trim().is_empty()
        {
            return Err(DesktopGitHubError::Response {
                operation: "starting device authorization",
                message: "required fields were empty".to_owned(),
            });
        }
        Ok(DesktopGitHubDeviceFlowStart {
            device_code: body.device_code,
            user_code: body.user_code,
            verification_uri: body.verification_uri,
            expires_at: now_millis().saturating_add(body.expires_in.max(1).saturating_mul(1_000)),
            interval_seconds: body.interval.max(1),
        })
    }

    pub fn poll_github_device_flow(
        &self,
        device_code: &str,
        interval_seconds: Option<i64>,
    ) -> Result<DesktopGitHubDeviceFlowPollResult, DesktopGitHubError> {
        let device_code = device_code.trim();
        if device_code.is_empty()
            || device_code.len() > 1_024
            || device_code.chars().any(char::is_control)
        {
            return Err(DesktopGitHubError::InvalidDeviceCode);
        }
        let client_id = github_client_id().ok_or(DesktopGitHubError::ClientIdMissing)?;
        let interval = interval_seconds.unwrap_or(5).max(1);
        let endpoints = GitHubEndpoints::current();
        let client = github_client()?;
        let response = client
            .post(&endpoints.access_token)
            .header(USER_AGENT, GITHUB_USER_AGENT)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|error| request_error("polling device authorization", error))?;
        if !response.status().is_success() {
            return Err(http_error(
                "polling device authorization",
                response.status(),
            ));
        }
        let body = response
            .json::<TokenResponse>()
            .map_err(|error| response_error("polling device authorization", error))?;
        if let Some(token) = body.access_token {
            let user_response = github_request_headers(client.get(&endpoints.user), Some(&token))
                .send()
                .map_err(|error| request_error("reading the authorized account", error))?;
            if !user_response.status().is_success() {
                return Err(http_error(
                    "reading the authorized account",
                    user_response.status(),
                ));
            }
            let user = user_response
                .json::<GitHubUserResponse>()
                .map_err(|error| response_error("reading the authorized account", error))?;
            let binding = DesktopGitHubBindingMetadata {
                login: user.login,
                avatar_url: user.avatar_url,
                bound_at: now_millis(),
                scopes: normalize_scope_list(body.scope.as_deref()),
                client_id_source: DesktopGitHubClientIdSource::Bundled,
            };
            self.store_github_binding(binding.clone(), &token)?;
            let status = binding_status(Some(binding));
            return Ok(DesktopGitHubDeviceFlowPollResult {
                status: "authorized".to_owned(),
                interval_seconds: interval,
                binding_status: Some(status),
                error: None,
            });
        }

        let error = body.error.unwrap_or_else(|| "unknown_error".to_owned());
        match error.as_str() {
            "authorization_pending" => Ok(DesktopGitHubDeviceFlowPollResult {
                status: "pending".to_owned(),
                interval_seconds: interval,
                binding_status: None,
                error: None,
            }),
            "slow_down" => Ok(DesktopGitHubDeviceFlowPollResult {
                status: "pending".to_owned(),
                interval_seconds: interval.saturating_add(5),
                binding_status: None,
                error: None,
            }),
            _ => Ok(DesktopGitHubDeviceFlowPollResult {
                status: "expired".to_owned(),
                interval_seconds: interval,
                binding_status: None,
                error: Some(error),
            }),
        }
    }

    pub fn unbind_github(&self) -> Result<(), DesktopGitHubError> {
        self.delete_github_token()?;
        self.github_store()?
            .delete_setting(GITHUB_BINDING_SETTINGS_KEY)
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
        self.emit_event(GitHubBindingChanged { login: None });
        Ok(())
    }

    pub fn list_github_repositories(
        &self,
        page: Option<u32>,
    ) -> Result<DesktopGitHubRepoPage, DesktopGitHubError> {
        let page = page.unwrap_or(1).clamp(1, 10_000);
        let (binding, token) = self.reconcile_github_binding(true)?;
        let binding = binding.ok_or(DesktopGitHubError::Unbound)?;
        let token = token.ok_or_else(|| DesktopGitHubError::BindingExpired {
            login: binding.login.clone(),
        })?;
        let endpoints = GitHubEndpoints::current();
        let response = github_request_headers(
            github_client()?.get(&endpoints.repositories).query(&[
                ("affiliation", "owner"),
                ("visibility", "all"),
                ("sort", "updated"),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ]),
            Some(&token),
        )
        .send()
        .map_err(|error| request_error("listing repositories", error))?;
        if matches!(response.status().as_u16(), 401 | 403) {
            self.clear_invalid_github_binding();
            return Err(DesktopGitHubError::BindingExpired {
                login: binding.login,
            });
        }
        if !response.status().is_success() {
            return Err(http_error("listing repositories", response.status()));
        }
        let next_page = parse_next_page(
            response
                .headers()
                .get(LINK)
                .and_then(|value| value.to_str().ok()),
        );
        let repositories = response
            .json::<Vec<RepoResponse>>()
            .map_err(|error| response_error("listing repositories", error))?;
        Ok(DesktopGitHubRepoPage {
            items: repositories
                .into_iter()
                .map(|repository| DesktopGitHubRepoSummary {
                    id: repository.id,
                    name: repository.name,
                    full_name: repository.full_name,
                    owner_login: repository.owner.login,
                    private: repository.private,
                    description: repository.description,
                    default_branch: repository.default_branch,
                    updated_at: repository.updated_at,
                    clone_url: repository.clone_url,
                    html_url: repository.html_url,
                })
                .collect(),
            next_page,
        })
    }

    /// Normalizes `repository` to a clone URL for the project clone job.
    pub fn github_clone_repository(&self, repository: &str) -> Result<String, DesktopGitHubError> {
        normalize_github_repository(repository)
    }

    /// Resolves the bound GitHub token used to authenticate a clone. Called on
    /// the job worker thread so the secret never enters a task payload.
    pub fn github_clone_token(
        &self,
        repository: &str,
    ) -> Result<Option<DesktopSecret>, DesktopGitHubError> {
        normalize_github_repository(repository)?;
        Ok(self
            .reconcile_github_binding(false)?
            .1
            .map(|token| DesktopSecret::new(token.into_bytes())))
    }

    fn github_store(&self) -> Result<SqliteAgentRuntimeStateStore, DesktopGitHubError> {
        self.config()
            .data_paths()
            .ensure_layout()
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
        SqliteAgentRuntimeStateStore::open(self.config().data_paths().agent_runtime_db())
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))
    }

    fn load_github_binding(
        &self,
    ) -> Result<Option<DesktopGitHubBindingMetadata>, DesktopGitHubError> {
        let value = self
            .github_store()?
            .setting(GITHUB_BINDING_SETTINGS_KEY)
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
        let Some(value) = value else {
            return Ok(None);
        };
        let stored = serde_json::from_value::<StoredGitHubBinding>(value)
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
        if stored.schema_version != GITHUB_BINDING_SCHEMA_VERSION {
            return Err(DesktopGitHubError::Persistence(format!(
                "unsupported binding schema {}",
                stored.schema_version
            )));
        }
        Ok(Some(stored.binding))
    }

    pub(crate) fn reconcile_github_binding(
        &self,
        token_required: bool,
    ) -> Result<(Option<DesktopGitHubBindingMetadata>, Option<String>), DesktopGitHubError> {
        let Some(binding) = self.load_github_binding()? else {
            return Ok((None, None));
        };
        let token = self.read_github_token()?;
        if token_required && token.is_none() {
            self.github_store()?
                .delete_setting(GITHUB_BINDING_SETTINGS_KEY)
                .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
            self.emit_event(GitHubBindingChanged { login: None });
            return Ok((None, None));
        }
        Ok((Some(binding), token))
    }

    fn store_github_binding(
        &self,
        binding: DesktopGitHubBindingMetadata,
        token: &str,
    ) -> Result<(), DesktopGitHubError> {
        self.write_github_token(token)?;
        let stored = StoredGitHubBinding {
            schema_version: GITHUB_BINDING_SCHEMA_VERSION,
            binding: binding.clone(),
        };
        let value = serde_json::to_value(stored)
            .map_err(|error| DesktopGitHubError::Persistence(error.to_string()))?;
        if let Err(error) = self
            .github_store()?
            .put_setting(GITHUB_BINDING_SETTINGS_KEY, &value)
        {
            let _ = self.delete_github_token();
            return Err(DesktopGitHubError::Persistence(error.to_string()));
        }
        self.emit_event(GitHubBindingChanged {
            login: Some(binding.login),
        });
        Ok(())
    }

    fn read_github_token(&self) -> Result<Option<String>, DesktopGitHubError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Read {
                key: GITHUB_TOKEN_KEY.to_owned(),
            },
        )) {
            Ok(DesktopHostResult::Credential(secret)) => secret
                .map(|secret| {
                    String::from_utf8(secret.into_inner()).map_err(|_| {
                        DesktopGitHubError::Credential(
                            "stored token is not valid UTF-8 text".to_owned(),
                        )
                    })
                })
                .transpose(),
            Ok(_) => Err(DesktopGitHubError::Credential(
                "credential read returned an unexpected host result".to_owned(),
            )),
            Err(error) => Err(DesktopGitHubError::Credential(error.to_string())),
        }
    }

    fn write_github_token(&self, token: &str) -> Result<(), DesktopGitHubError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Write {
                key: GITHUB_TOKEN_KEY.to_owned(),
                secret: DesktopSecret::new(token.as_bytes().to_vec()),
            },
        )) {
            Ok(DesktopHostResult::Completed) => Ok(()),
            Ok(_) => Err(DesktopGitHubError::Credential(
                "credential write returned an unexpected host result".to_owned(),
            )),
            Err(error) => Err(DesktopGitHubError::Credential(error.to_string())),
        }
    }

    fn delete_github_token(&self) -> Result<(), DesktopGitHubError> {
        match self.execute_host(DesktopHostAction::Credential(
            DesktopCredentialAction::Delete {
                key: GITHUB_TOKEN_KEY.to_owned(),
            },
        )) {
            Ok(DesktopHostResult::Completed) => Ok(()),
            Ok(_) => Err(DesktopGitHubError::Credential(
                "credential delete returned an unexpected host result".to_owned(),
            )),
            Err(error) => Err(DesktopGitHubError::Credential(error.to_string())),
        }
    }

    fn clear_invalid_github_binding(&self) {
        let _ = self.delete_github_token();
        if let Ok(store) = self.github_store() {
            let _ = store.delete_setting(GITHUB_BINDING_SETTINGS_KEY);
        }
        self.emit_event(GitHubBindingChanged { login: None });
    }
}

fn github_client_id() -> Option<&'static str> {
    (!GITHUB_CLIENT_ID.trim().is_empty()).then_some(GITHUB_CLIENT_ID.trim())
}

pub(crate) fn github_client() -> Result<Client, DesktopGitHubError> {
    Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| request_error("building the HTTP client", error))
}

pub(crate) fn github_request_headers(
    builder: RequestBuilder,
    token: Option<&str>,
) -> RequestBuilder {
    let builder = builder
        .header(USER_AGENT, GITHUB_USER_AGENT)
        .header(ACCEPT, GITHUB_ACCEPT)
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token {
        builder.bearer_auth(token)
    } else {
        builder
    }
}

fn binding_status(binding: Option<DesktopGitHubBindingMetadata>) -> DesktopGitHubBindingStatus {
    DesktopGitHubBindingStatus {
        state: if binding.is_some() {
            "bound"
        } else {
            "unbound"
        }
        .to_owned(),
        client_id_configured: github_client_id().is_some(),
        client_id_source: if github_client_id().is_some() {
            DesktopGitHubClientIdSource::Bundled
        } else {
            DesktopGitHubClientIdSource::None
        },
        binding,
    }
}

fn normalize_scope_list(scope: Option<&str>) -> Vec<String> {
    scope
        .unwrap_or_default()
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

fn normalize_github_repository(input: &str) -> Result<String, DesktopGitHubError> {
    let input = input.trim().trim_end_matches('/');
    if input.is_empty() || input.chars().any(char::is_control) {
        return Err(DesktopGitHubError::InvalidRepository);
    }
    let path = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .unwrap_or(input)
        .trim_end_matches(".git");
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || !parts.iter().all(|part| valid_github_name(part)) {
        return Err(DesktopGitHubError::InvalidRepository);
    }
    Ok(format!("https://github.com/{}/{}.git", parts[0], parts[1]))
}

fn valid_github_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn parse_next_page(link: Option<&str>) -> Option<u32> {
    for part in link?.split(',') {
        if !part.contains("rel=\"next\"") {
            continue;
        }
        let query = part.split_once('?')?.1.split_once('>')?.0;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=')?;
            if key == "page" {
                return value.parse().ok();
            }
        }
    }
    None
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn request_error(operation: &'static str, error: reqwest::Error) -> DesktopGitHubError {
    DesktopGitHubError::Request {
        operation,
        message: error.to_string(),
    }
}

fn response_error(operation: &'static str, error: reqwest::Error) -> DesktopGitHubError {
    DesktopGitHubError::Response {
        operation,
        message: error.to_string(),
    }
}

fn http_error(operation: &'static str, status: reqwest::StatusCode) -> DesktopGitHubError {
    DesktopGitHubError::Http {
        operation,
        status: status.as_u16(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{DesktopApplicationConfig, DesktopHost, DesktopHostContext, DesktopHostError, GitHubBindingChanged};

    #[derive(Default)]
    struct MemoryCredentialHost {
        secrets: Mutex<BTreeMap<(String, String), Vec<u8>>>,
    }

    impl DesktopHost for MemoryCredentialHost {
        fn execute(
            &self,
            context: &DesktopHostContext,
            action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            match action {
                DesktopHostAction::Credential(DesktopCredentialAction::Write { key, secret }) => {
                    self.secrets.lock().unwrap().insert(
                        (context.instance_identity.clone(), key),
                        secret.into_inner(),
                    );
                    Ok(DesktopHostResult::Completed)
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Read { key }) => {
                    Ok(DesktopHostResult::Credential(
                        self.secrets
                            .lock()
                            .unwrap()
                            .get(&(context.instance_identity.clone(), key))
                            .cloned()
                            .map(DesktopSecret::new),
                    ))
                }
                DesktopHostAction::Credential(DesktopCredentialAction::Delete { key }) => {
                    self.secrets
                        .lock()
                        .unwrap()
                        .remove(&(context.instance_identity.clone(), key));
                    Ok(DesktopHostResult::Completed)
                }
                _ => Err(DesktopHostError::new(
                    "unexpected_test_host_action",
                    "test host only supports credential actions",
                    false,
                )),
            }
        }
    }

    fn application(
        home: &std::path::Path,
        identity: &str,
        host: Arc<MemoryCredentialHost>,
    ) -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("github-test:{}", uuid::Uuid::new_v4()),
            "github-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(home, identity).unwrap(),
            authority,
            host,
        )
        .unwrap()
    }

    fn assert_tree_excludes(root: &std::path::Path, canary: &[u8]) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                assert_tree_excludes(&path, canary);
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes.windows(canary.len()).any(|window| window == canary),
                "secret canary leaked to {}",
                path.display()
            );
        }
    }

    #[test]
    fn repository_normalization_accepts_only_github_owner_and_repo() {
        assert_eq!(
            normalize_github_repository("sena-nana/Lilia").unwrap(),
            "https://github.com/sena-nana/Lilia.git"
        );
        assert_eq!(
            normalize_github_repository("https://github.com/sena-nana/Lilia.git").unwrap(),
            "https://github.com/sena-nana/Lilia.git"
        );
        assert!(normalize_github_repository("https://example.com/acme/repo").is_err());
        assert!(normalize_github_repository("acme").is_err());
        assert!(normalize_github_repository("acme/repo?token=secret").is_err());
    }

    #[test]
    fn scope_and_pagination_parsers_preserve_the_contract() {
        assert_eq!(
            normalize_scope_list(Some("repo, read:user workflow")),
            vec!["repo", "read:user", "workflow"]
        );
        let link = r#"<https://api.github.com/user/repos?page=2>; rel="next", <https://api.github.com/user/repos?page=4>; rel="last""#;
        assert_eq!(parse_next_page(Some(link)), Some(2));
        assert_eq!(parse_next_page(None), None);
    }

    #[test]
    fn binding_metadata_persists_but_token_stays_in_the_instance_keyring() {
        let root = tempfile::tempdir().unwrap();
        let host = Arc::new(MemoryCredentialHost::default());
        let app = application(root.path(), "liliacode", host.clone());
        let binding = DesktopGitHubBindingMetadata {
            login: "native-user".to_owned(),
            avatar_url: Some("https://avatars.example/native-user".to_owned()),
            bound_at: 42,
            scopes: vec!["repo".to_owned(), "read:user".to_owned()],
            client_id_source: DesktopGitHubClientIdSource::Bundled,
        };

        app.store_github_binding(binding.clone(), "github-token-canary")
            .unwrap();
        assert_eq!(
            app.github_binding_status().unwrap().binding,
            Some(binding.clone())
        );
        assert_eq!(
            host.secrets
                .lock()
                .unwrap()
                .get(&("liliacode".to_owned(), GITHUB_TOKEN_KEY.to_owned()))
                .cloned(),
            Some(b"github-token-canary".to_vec())
        );
        assert_tree_excludes(root.path(), b"github-token-canary");

        let restored = application(root.path(), "liliacode", host.clone());
        assert_eq!(
            restored.github_binding_status().unwrap().binding,
            Some(binding)
        );
        restored.unbind_github().unwrap();
        assert_eq!(restored.github_binding_status().unwrap().state, "unbound");
        assert!(host.secrets.lock().unwrap().is_empty());
    }
}
