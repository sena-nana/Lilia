use serde_json::{json, Map, Value};

pub const TOOL_CONSENT_INTERACTION_KIND: &str = "tool_consent";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopToolConsentDecision {
    Allow,
    Deny,
}

impl DesktopToolConsentDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub const fn accepted(self) -> bool {
        matches!(self, Self::Allow)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopToolConsent {
    pub tool_name: String,
    pub input: Map<String, Value>,
    pub title: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub blocked_path: Option<String>,
    pub decision_reason: Option<String>,
    pub cwd: Option<String>,
    pub reason: Option<String>,
}

impl DesktopToolConsent {
    pub fn from_payload(payload: &Value) -> Result<Self, DesktopToolConsentError> {
        let object = payload
            .as_object()
            .ok_or(DesktopToolConsentError::PayloadMustBeObject)?;
        let input = object
            .get("input")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            tool_name: optional_string(object, "toolName").unwrap_or_else(|| "tool".to_owned()),
            input,
            title: optional_string(object, "title"),
            display_name: optional_string(object, "displayName"),
            description: optional_string(object, "description"),
            blocked_path: optional_string(object, "blockedPath"),
            decision_reason: optional_string(object, "decisionReason"),
            cwd: optional_string(object, "cwd"),
            reason: optional_string(object, "reason"),
        })
    }

    pub fn editable_command(&self) -> Option<String> {
        self.input
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                ["parsedCmd", "command", "cmd", "commandActions"]
                    .into_iter()
                    .find_map(|field| self.input.get(field).and_then(stringify_command))
            })
            .filter(|command| !command.trim().is_empty())
    }

    pub fn response(
        &self,
        task_id: &str,
        request_id: &str,
        decision: DesktopToolConsentDecision,
        message: Option<&str>,
        command: Option<&str>,
    ) -> Result<Value, DesktopToolConsentError> {
        let mut response = json!({
            "taskId": task_id,
            "requestId": request_id,
            "decision": decision.as_str(),
            "message": message.map(str::trim).filter(|value| !value.is_empty()),
        });
        if decision == DesktopToolConsentDecision::Allow {
            if let Some(original) = self.editable_command() {
                let command = command.unwrap_or(&original).trim();
                if command.is_empty() {
                    return Err(DesktopToolConsentError::CommandMustNotBeEmpty);
                }
                if command != original {
                    let mut updated_input = self.input.clone();
                    updated_input.insert("command".to_owned(), Value::String(command.to_owned()));
                    response["updatedInput"] = Value::Object(updated_input);
                }
            }
        }
        Ok(response)
    }
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Option<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn stringify_command(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Array(values) => {
            let command = values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.clone()),
                    Value::Object(object) => ["text", "value", "arg", "command"]
                        .into_iter()
                        .find_map(|field| optional_string(object, field)),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            (!command.is_empty()).then_some(command)
        }
        Value::Object(object) => ["parsedCmd", "command", "cmd", "args"]
            .into_iter()
            .find_map(|field| object.get(field).and_then(stringify_command)),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopToolConsentError {
    #[error("tool consent payload must be an object")]
    PayloadMustBeObject,
    #[error("tool consent command must not be empty")]
    CommandMustNotBeEmpty,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_editable_command_from_provider_command_parts() {
        let consent = DesktopToolConsent::from_payload(&json!({
            "toolName": "shell",
            "input": {
                "parsedCmd": [
                    {"text": "cargo"},
                    {"arg": "test"},
                    "--locked"
                ]
            }
        }))
        .unwrap();

        assert_eq!(
            consent.editable_command().as_deref(),
            Some("cargo test --locked")
        );
    }

    #[test]
    fn allow_response_only_replaces_an_edited_command() {
        let consent = DesktopToolConsent::from_payload(&json!({
            "toolName": "shell",
            "input": {"command": "cargo check", "cwd": "/workspace"}
        }))
        .unwrap();

        let unchanged = consent
            .response(
                "task-1",
                "request-1",
                DesktopToolConsentDecision::Allow,
                None,
                Some("cargo check"),
            )
            .unwrap();
        assert!(unchanged.get("updatedInput").is_none());

        let edited = consent
            .response(
                "task-1",
                "request-1",
                DesktopToolConsentDecision::Allow,
                None,
                Some("cargo check --locked"),
            )
            .unwrap();
        assert_eq!(
            edited["updatedInput"],
            json!({"command": "cargo check --locked", "cwd": "/workspace"})
        );
    }

    #[test]
    fn deny_response_preserves_the_user_reason() {
        let consent = DesktopToolConsent::from_payload(&json!({
            "toolName": "write_file",
            "input": {"path": "/tmp/file"}
        }))
        .unwrap();
        let response = consent
            .response(
                "task-1",
                "request-1",
                DesktopToolConsentDecision::Deny,
                Some("不要修改这个文件"),
                None,
            )
            .unwrap();

        assert_eq!(response["decision"], "deny");
        assert_eq!(response["message"], "不要修改这个文件");
        assert_eq!(response["taskId"], "task-1");
    }
}
