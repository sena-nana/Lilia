use lilia_storage::{AgentkitMcpRegistry, AgentkitMcpRegistryEntry};
use mutsuki_agent_contracts::{McpCatalog, McpServerState};

use crate::error::{invalid_input, ExtensionsError};
use crate::types::{
    McpActivationResult, McpCredentialKind, McpPromptArgumentView, McpPromptView,
    McpResourceContentView, McpResourceView, McpServerUpsert, McpToolView, McpTransport,
};

pub fn mcp_tools(catalog: &McpCatalog, server_id: &str) -> Vec<McpToolView> {
    catalog
        .tools
        .iter()
        .filter(|tool| tool.server_id == server_id)
        .map(|tool| McpToolView {
            name: tool.name.clone(),
            namespaced_name: tool.namespaced_name.clone(),
            description: tool.description.clone(),
            read_only: tool.annotations.read_only_hint,
            destructive: tool.annotations.destructive_hint,
            idempotent: tool.annotations.idempotent_hint,
            open_world: tool.annotations.open_world_hint,
        })
        .collect()
}

pub fn mcp_resources(catalog: &McpCatalog, server_id: &str) -> Vec<McpResourceView> {
    catalog
        .resources
        .iter()
        .filter(|resource| resource.server_id == server_id)
        .map(|resource| McpResourceView {
            uri: resource.uri.clone(),
            name: resource.name.clone(),
            description: resource.description.clone(),
            mime_type: resource.mime_type.clone(),
        })
        .collect()
}

pub fn mcp_prompts(catalog: &McpCatalog, server_id: &str) -> Vec<McpPromptView> {
    catalog
        .prompts
        .iter()
        .filter(|prompt| prompt.server_id == server_id)
        .map(|prompt| McpPromptView {
            name: prompt.name.clone(),
            namespaced_name: prompt.namespaced_name.clone(),
            description: prompt.description.clone(),
            arguments: prompt
                .arguments
                .iter()
                .map(|argument| McpPromptArgumentView {
                    name: argument.name.clone(),
                    description: argument.description.clone(),
                    required: argument.required,
                })
                .collect(),
        })
        .collect()
}

pub fn mcp_resource_contents(
    content: &serde_json::Value,
) -> Result<Vec<McpResourceContentView>, ExtensionsError> {
    let contents = content
        .get("contents")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid_input("resource", "MCP resource response has no contents"))?;
    contents
        .iter()
        .map(|content| {
            let uri = content
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid_input("resource", "MCP resource content has no URI"))?;
            Ok(McpResourceContentView {
                uri: uri.to_owned(),
                mime_type: content
                    .get("mimeType")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                text: content
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                encoded_blob_length: content
                    .get("blob")
                    .and_then(serde_json::Value::as_str)
                    .map(str::len),
            })
        })
        .collect()
}

pub fn required_mcp_value<'a>(
    field: &'static str,
    value: &'a str,
) -> Result<&'a str, ExtensionsError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_input(field, format!("MCP {field} is required")));
    }
    Ok(value)
}

pub fn ensure_mcp_credential_is_configured(
    entry: &AgentkitMcpRegistryEntry,
    kind: McpCredentialKind,
    name: &str,
) -> Result<(), ExtensionsError> {
    let configured = match kind {
        McpCredentialKind::Environment => entry
            .env_secret_names
            .iter()
            .any(|configured| configured == name),
        McpCredentialKind::Header => entry
            .header_secret_names
            .iter()
            .any(|configured| configured.eq_ignore_ascii_case(name)),
    };
    if configured {
        Ok(())
    } else {
        Err(invalid_input(
            "credential",
            "MCP credential name is not registered for this server",
        ))
    }
}

pub fn removed_mcp_credentials(
    previous: &AgentkitMcpRegistryEntry,
    next: &AgentkitMcpRegistryEntry,
) -> AgentkitMcpRegistryEntry {
    let mut removed = previous.clone();
    removed.env_secret_names.retain(|name| {
        !next
            .env_secret_names
            .iter()
            .any(|next_name| next_name == name)
    });
    removed.header_secret_names.retain(|name| {
        !next
            .header_secret_names
            .iter()
            .any(|next_name| next_name.eq_ignore_ascii_case(name))
    });
    removed
}

pub fn mcp_credential_key(server_id: &str, kind: McpCredentialKind, name: &str) -> String {
    let name = if kind == McpCredentialKind::Header {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    };
    format!("mcp.server.{server_id}.{}.{}", kind.key_segment(), name)
}

/// Validate a credential value before it reaches an MCP transport.
pub fn validate_mcp_secret(kind: McpCredentialKind, secret: &[u8]) -> Result<(), ExtensionsError> {
    if secret.is_empty() || secret.len() > 65_536 {
        return Err(invalid_input(
            "credential",
            "MCP credential must contain 1-65536 UTF-8 bytes",
        ));
    }
    let value = std::str::from_utf8(secret)
        .map_err(|_| invalid_input("credential", "MCP credential must contain UTF-8 text"))?;
    let unsafe_control = match kind {
        McpCredentialKind::Environment => value.contains('\0'),
        McpCredentialKind::Header => value.chars().any(char::is_control),
    };
    if unsafe_control {
        return Err(invalid_input(
            "credential",
            "MCP credential contains characters unsafe for its transport",
        ));
    }
    Ok(())
}

pub fn mcp_activation_error(server_id: &str, error: impl Into<String>) -> McpActivationResult {
    McpActivationResult {
        server_id: server_id.to_owned(),
        runtime_state: None,
        tool_count: 0,
        resource_count: 0,
        prompt_count: 0,
        error: Some(error.into()),
    }
}

/// A validated MCP server definition ready to become a registry entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedMcpServer {
    pub expected_registry_revision: u64,
    pub server_id: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env_secret_names: Vec<String>,
    pub header_secret_names: Vec<String>,
    pub enabled: bool,
}

impl NormalizedMcpServer {
    pub fn new(input: McpServerUpsert) -> Result<Self, ExtensionsError> {
        let server_id = normalized_server_id(&input.server_id)?;
        if input.args.len() > 64 {
            return Err(invalid_input(
                "args",
                "MCP args may contain at most 64 items",
            ));
        }
        let args = input
            .args
            .into_iter()
            .map(|argument| {
                normalized_text("args", argument, 4096)?
                    .ok_or_else(|| invalid_input("args", "MCP args must not be empty"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let command = normalized_text("command", input.command.unwrap_or_default(), 4096)?;
        let url = normalized_text("url", input.url.unwrap_or_default(), 8192)?;
        let env_secret_names = normalized_mcp_credential_names(
            McpCredentialKind::Environment,
            input.env_secret_names,
        )?;
        let header_secret_names =
            normalized_mcp_credential_names(McpCredentialKind::Header, input.header_secret_names)?;
        match input.transport {
            McpTransport::Stdio => {
                if command.is_none() {
                    return Err(invalid_input("command", "stdio MCP requires a command"));
                }
                if url.is_some() {
                    return Err(invalid_input("url", "stdio MCP must not define a URL"));
                }
                if !header_secret_names.is_empty() {
                    return Err(invalid_input(
                        "header_secret_names",
                        "stdio MCP must not define HTTP header credentials",
                    ));
                }
            }
            McpTransport::StreamableHttp | McpTransport::Sse => {
                if command.is_some() || !args.is_empty() {
                    return Err(invalid_input(
                        "command",
                        "HTTP MCP must not define a command or args",
                    ));
                }
                let value = url
                    .as_deref()
                    .ok_or_else(|| invalid_input("url", "HTTP MCP requires a URL"))?;
                validate_mcp_url(value)?;
                if !env_secret_names.is_empty() {
                    return Err(invalid_input(
                        "env_secret_names",
                        "HTTP MCP must not define environment credentials",
                    ));
                }
            }
        }
        Ok(Self {
            expected_registry_revision: input.expected_registry_revision,
            server_id,
            transport: input.transport,
            command,
            args,
            url,
            env_secret_names,
            header_secret_names,
            enabled: input.enabled,
        })
    }

    pub fn registry_entry(&self) -> AgentkitMcpRegistryEntry {
        AgentkitMcpRegistryEntry {
            server_id: self.server_id.clone(),
            source: "lilia.desktop".to_owned(),
            transport: self.transport.as_registry().to_owned(),
            command: self.command.clone(),
            args: self.args.clone(),
            env_allowlist: Vec::new(),
            env_secret_names: self.env_secret_names.clone(),
            url: self.url.clone(),
            header_secret_names: self.header_secret_names.clone(),
            registered_from: "lilia-native-settings".to_owned(),
            enabled: self.enabled,
        }
    }
}

fn normalized_mcp_credential_names(
    kind: McpCredentialKind,
    names: Vec<String>,
) -> Result<Vec<String>, ExtensionsError> {
    if names.len() > 32 {
        return Err(invalid_input(
            "credential_names",
            "MCP credentials may contain at most 32 names",
        ));
    }
    let mut normalized = names
        .into_iter()
        .map(|name| normalized_mcp_credential_name(kind, &name))
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort_by(|left, right| {
        if kind == McpCredentialKind::Header {
            left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
        } else {
            left.cmp(right)
        }
    });
    let duplicate = normalized.windows(2).any(|pair| {
        if kind == McpCredentialKind::Header {
            pair[0].eq_ignore_ascii_case(&pair[1])
        } else {
            pair[0] == pair[1]
        }
    });
    if duplicate {
        return Err(invalid_input(
            "credential_names",
            "MCP credential names must be unique",
        ));
    }
    Ok(normalized)
}

pub fn normalized_mcp_credential_name(
    kind: McpCredentialKind,
    name: &str,
) -> Result<String, ExtensionsError> {
    let name = name.trim();
    let valid = match kind {
        McpCredentialKind::Environment => {
            let mut bytes = name.bytes();
            bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }
        McpCredentialKind::Header => {
            !name.is_empty()
                && name.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(
                            byte,
                            b'!' | b'#'
                                | b'$'
                                | b'%'
                                | b'&'
                                | b'\''
                                | b'*'
                                | b'+'
                                | b'-'
                                | b'.'
                                | b'^'
                                | b'_'
                                | b'`'
                                | b'|'
                                | b'~'
                        )
                })
                && !matches!(
                    name.to_ascii_lowercase().as_str(),
                    "connection" | "content-length" | "content-type" | "host" | "transfer-encoding"
                )
        }
    };
    if name.len() > 128 || !valid {
        return Err(invalid_input(
            "credential_names",
            match kind {
                McpCredentialKind::Environment => {
                    "environment credential names must be valid ASCII environment variables"
                }
                McpCredentialKind::Header => {
                    "header credential names must be safe HTTP field names"
                }
            },
        ));
    }
    Ok(name.to_owned())
}

pub fn ensure_registry_revision(actual: u64, expected: u64) -> Result<(), ExtensionsError> {
    if actual == expected {
        return Ok(());
    }
    Err(invalid_input(
        "expected_registry_revision",
        format!("MCP registry changed: expected revision {expected}, actual {actual}"),
    ))
}

pub fn bump_registry_revision(registry: &mut AgentkitMcpRegistry) -> Result<(), ExtensionsError> {
    registry.revision = registry
        .revision
        .checked_add(1)
        .ok_or(ExtensionsError::StateRevisionOverflow("extension registry"))?;
    Ok(())
}

pub fn normalized_server_id(value: &str) -> Result<String, ExtensionsError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.starts_with("plugin.")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(invalid_input(
            "server_id",
            "MCP server id must use 1-128 ASCII letters, digits, dot, dash, or underscore and cannot use the reserved plugin prefix",
        ));
    }
    Ok(value.to_owned())
}

fn normalized_text(
    field: &'static str,
    value: String,
    max_bytes: usize,
) -> Result<Option<String>, ExtensionsError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(invalid_input(
            field,
            format!("value must not contain control characters or exceed {max_bytes} bytes"),
        ));
    }
    Ok(Some(value.to_owned()))
}

fn validate_mcp_url(value: &str) -> Result<(), ExtensionsError> {
    let url = url::Url::parse(value).map_err(|_| invalid_input("url", "MCP URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(invalid_input(
            "url",
            "MCP URL must use http or https and include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid_input(
            "url",
            "MCP URL must not contain inline credentials",
        ));
    }
    if url.fragment().is_some() {
        return Err(invalid_input("url", "MCP URL must not contain a fragment"));
    }
    Ok(())
}

pub fn mcp_state_key(state: &McpServerState) -> &'static str {
    match state {
        McpServerState::Connecting => "connecting",
        McpServerState::Ready => "ready",
        McpServerState::Failed => "failed",
        McpServerState::Draining => "draining",
    }
}
