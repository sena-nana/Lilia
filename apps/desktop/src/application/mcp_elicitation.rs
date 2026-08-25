use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Number, Value};

pub const MCP_ELICITATION_INTERACTION_KIND: &str = "mcp_elicitation";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMcpElicitationMode {
    Form,
    Url,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopMcpElicitationAction {
    Accept,
    Decline,
    Cancel,
}

impl DesktopMcpElicitationAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }

    pub const fn accepted(self) -> bool {
        matches!(self, Self::Accept)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopMcpFormFieldKind {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopMcpFormOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopMcpFormField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub kind: DesktopMcpFormFieldKind,
    pub required: bool,
    pub options: Vec<DesktopMcpFormOption>,
    pub default_value: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopMcpElicitation {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub server_name: String,
    pub mode: DesktopMcpElicitationMode,
    pub message: String,
    pub requested_schema: Value,
    pub url: Option<String>,
    pub elicitation_id: Option<String>,
    pub metadata: Option<Value>,
}

impl DesktopMcpElicitation {
    pub fn from_payload(payload: &Value) -> Result<Self, DesktopMcpElicitationError> {
        let object = payload
            .as_object()
            .ok_or(DesktopMcpElicitationError::PayloadMustBeObject)?;
        let thread_id = required_string(object, "threadId")?;
        let server_name = required_string(object, "serverName")?;
        let mode = match required_string(object, "mode")?.as_str() {
            "form" => DesktopMcpElicitationMode::Form,
            "url" => DesktopMcpElicitationMode::Url,
            value => {
                return Err(DesktopMcpElicitationError::UnsupportedMode(
                    value.to_owned(),
                ))
            }
        };
        let url = optional_string(object, "url");
        if mode == DesktopMcpElicitationMode::Url && url.is_none() {
            return Err(DesktopMcpElicitationError::MissingField("url"));
        }
        Ok(Self {
            thread_id,
            turn_id: optional_string(object, "turnId"),
            server_name,
            mode,
            message: optional_string(object, "message").unwrap_or_default(),
            requested_schema: object
                .get("requestedSchema")
                .cloned()
                .unwrap_or(Value::Null),
            url,
            elicitation_id: optional_string(object, "elicitationId"),
            metadata: object.get("_meta").cloned(),
        })
    }

    pub fn fields(&self) -> Vec<DesktopMcpFormField> {
        if self.mode != DesktopMcpElicitationMode::Form {
            return Vec::new();
        }
        let Some(schema) = self.requested_schema.as_object() else {
            return Vec::new();
        };
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        properties
            .iter()
            .map(|(key, value)| {
                let field = value.as_object();
                let type_name = field
                    .and_then(|field| field.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("string");
                DesktopMcpFormField {
                    key: key.clone(),
                    label: field
                        .and_then(|field| field.get("title"))
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or(key)
                        .to_owned(),
                    description: field
                        .and_then(|field| field.get("description"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    kind: match type_name {
                        "string" => DesktopMcpFormFieldKind::String,
                        "number" => DesktopMcpFormFieldKind::Number,
                        "integer" => DesktopMcpFormFieldKind::Integer,
                        "boolean" => DesktopMcpFormFieldKind::Boolean,
                        "array" => DesktopMcpFormFieldKind::Array,
                        _ => DesktopMcpFormFieldKind::Other,
                    },
                    required: required.contains(key.as_str()),
                    options: field.map(form_options).unwrap_or_default(),
                    default_value: field.and_then(|field| field.get("default")).cloned(),
                }
            })
            .collect()
    }

    pub fn initial_content(&self) -> Map<String, Value> {
        self.fields()
            .into_iter()
            .map(|field| {
                let value = field.default_value.unwrap_or_else(|| match field.kind {
                    DesktopMcpFormFieldKind::Boolean => Value::Bool(false),
                    DesktopMcpFormFieldKind::Array => Value::Array(Vec::new()),
                    _ => Value::String(String::new()),
                });
                (field.key, value)
            })
            .collect()
    }

    pub fn response(
        &self,
        action: DesktopMcpElicitationAction,
        content: Option<&Map<String, Value>>,
    ) -> Result<(bool, Value), DesktopMcpElicitationError> {
        if action != DesktopMcpElicitationAction::Accept {
            return Ok((false, json!({ "action": action.as_str() })));
        }
        if self.mode == DesktopMcpElicitationMode::Url {
            return Ok((true, json!({ "action": action.as_str() })));
        }
        let content = content.cloned().unwrap_or_default();
        let normalized = self.normalize_content(&content)?;
        Ok((
            true,
            json!({
                "action": action.as_str(),
                "content": normalized,
            }),
        ))
    }

    fn normalize_content(
        &self,
        content: &Map<String, Value>,
    ) -> Result<Map<String, Value>, DesktopMcpElicitationError> {
        let fields = self.fields();
        if fields.is_empty() {
            return Ok(content.clone());
        }
        let mut normalized = Map::new();
        for field in fields {
            let raw = content.get(&field.key);
            let value = normalize_field_value(&field, raw)?;
            if let Some(value) = value {
                normalized.insert(field.key, value);
            } else if field.required {
                return Err(DesktopMcpElicitationError::RequiredField(field.key));
            }
        }
        Ok(normalized)
    }
}

fn required_string(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<String, DesktopMcpElicitationError> {
    optional_string(object, field).ok_or(DesktopMcpElicitationError::MissingField(field))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Option<String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn form_options(field: &Map<String, Value>) -> Vec<DesktopMcpFormOption> {
    if let Some(values) = field.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .map(|value| DesktopMcpFormOption {
                value: value.to_owned(),
                label: value.to_owned(),
            })
            .collect();
    }
    let variants = field.get("oneOf").and_then(Value::as_array).or_else(|| {
        field
            .get("items")
            .and_then(Value::as_object)
            .and_then(|items| items.get("anyOf"))
            .and_then(Value::as_array)
    });
    if let Some(variants) = variants {
        return variants
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|variant| {
                let value = variant.get("const")?.as_str()?;
                Some(DesktopMcpFormOption {
                    value: value.to_owned(),
                    label: variant
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or(value)
                        .to_owned(),
                })
            })
            .collect();
    }
    field
        .get("items")
        .and_then(Value::as_object)
        .and_then(|items| items.get("enum"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| DesktopMcpFormOption {
            value: value.to_owned(),
            label: value.to_owned(),
        })
        .collect()
}

fn normalize_field_value(
    field: &DesktopMcpFormField,
    raw: Option<&Value>,
) -> Result<Option<Value>, DesktopMcpElicitationError> {
    match field.kind {
        DesktopMcpFormFieldKind::String => {
            let text = raw.and_then(Value::as_str).unwrap_or_default();
            if text.trim().is_empty() {
                return if field.required {
                    Err(DesktopMcpElicitationError::RequiredField(field.key.clone()))
                } else {
                    Ok(None)
                };
            }
            validate_option(field, text)?;
            Ok(Some(Value::String(text.to_owned())))
        }
        DesktopMcpFormFieldKind::Number | DesktopMcpFormFieldKind::Integer => {
            let number = match raw {
                Some(Value::Number(value)) => value.clone(),
                Some(Value::String(value)) if !value.trim().is_empty() => {
                    if field.kind == DesktopMcpFormFieldKind::Integer {
                        Number::from(value.parse::<i64>().map_err(|_| {
                            DesktopMcpElicitationError::InvalidField {
                                field: field.key.clone(),
                                message: "must be an integer".to_owned(),
                            }
                        })?)
                    } else {
                        let parsed = value.parse::<f64>().map_err(|_| {
                            DesktopMcpElicitationError::InvalidField {
                                field: field.key.clone(),
                                message: "must be a number".to_owned(),
                            }
                        })?;
                        Number::from_f64(parsed).ok_or_else(|| {
                            DesktopMcpElicitationError::InvalidField {
                                field: field.key.clone(),
                                message: "must be a finite number".to_owned(),
                            }
                        })?
                    }
                }
                None | Some(Value::String(_)) if !field.required => return Ok(None),
                _ => {
                    return Err(DesktopMcpElicitationError::InvalidField {
                        field: field.key.clone(),
                        message: "must be a number".to_owned(),
                    })
                }
            };
            if field.kind == DesktopMcpFormFieldKind::Integer
                && !number.is_i64()
                && !number.is_u64()
            {
                return Err(DesktopMcpElicitationError::InvalidField {
                    field: field.key.clone(),
                    message: "must be an integer".to_owned(),
                });
            }
            Ok(Some(Value::Number(number)))
        }
        DesktopMcpFormFieldKind::Boolean => raw
            .and_then(Value::as_bool)
            .map(Value::Bool)
            .or_else(|| (!field.required).then_some(Value::Bool(false)))
            .ok_or_else(|| DesktopMcpElicitationError::InvalidField {
                field: field.key.clone(),
                message: "must be true or false".to_owned(),
            })
            .map(Some),
        DesktopMcpFormFieldKind::Array => {
            let values = raw.and_then(Value::as_array).cloned().unwrap_or_default();
            if values.is_empty() {
                return if field.required {
                    Err(DesktopMcpElicitationError::RequiredField(field.key.clone()))
                } else {
                    Ok(None)
                };
            }
            for value in &values {
                let Some(value) = value.as_str() else {
                    return Err(DesktopMcpElicitationError::InvalidField {
                        field: field.key.clone(),
                        message: "must contain only string values".to_owned(),
                    });
                };
                validate_option(field, value)?;
            }
            Ok(Some(Value::Array(values)))
        }
        DesktopMcpFormFieldKind::Other => match raw {
            Some(value) if !value.is_null() => Ok(Some(value.clone())),
            _ if field.required => {
                Err(DesktopMcpElicitationError::RequiredField(field.key.clone()))
            }
            _ => Ok(None),
        },
    }
}

fn validate_option(
    field: &DesktopMcpFormField,
    value: &str,
) -> Result<(), DesktopMcpElicitationError> {
    if field.options.is_empty() || field.options.iter().any(|option| option.value == value) {
        return Ok(());
    }
    Err(DesktopMcpElicitationError::InvalidField {
        field: field.key.clone(),
        message: format!("unsupported option `{value}`"),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopMcpElicitationError {
    #[error("MCP elicitation payload must be an object")]
    PayloadMustBeObject,
    #[error("MCP elicitation is missing required field `{0}`")]
    MissingField(&'static str),
    #[error("unsupported MCP elicitation mode `{0}`")]
    UnsupportedMode(String),
    #[error("MCP elicitation field `{0}` is required")]
    RequiredField(String),
    #[error("MCP elicitation field `{field}` {message}")]
    InvalidField { field: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> DesktopMcpElicitation {
        DesktopMcpElicitation::from_payload(&json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "serverName": "linear",
            "mode": "form",
            "message": "选择项目并补充说明",
            "requestedSchema": {
                "type": "object",
                "required": ["project", "labels", "count"],
                "properties": {
                    "project": {"type": "string", "enum": ["A", "B"]},
                    "labels": {
                        "type": "array",
                        "items": {"enum": ["bug", "feature"]}
                    },
                    "note": {"type": "string"},
                    "count": {"type": "integer", "default": 1},
                    "notify": {"type": "boolean", "default": true}
                }
            },
            "elicitationId": "elicitation-1"
        }))
        .unwrap()
    }

    #[test]
    fn parses_form_fields_and_defaults_without_ui_owned_schema_logic() {
        let elicitation = form();
        let fields = elicitation.fields();
        assert_eq!(fields.len(), 5);
        let labels = fields.iter().find(|field| field.key == "labels").unwrap();
        assert_eq!(labels.kind, DesktopMcpFormFieldKind::Array);
        assert_eq!(labels.options.len(), 2);
        assert!(labels.required);
        let initial = elicitation.initial_content();
        assert_eq!(initial["count"], 1);
        assert_eq!(initial["notify"], true);
        assert_eq!(initial["labels"], json!([]));
    }

    #[test]
    fn accept_normalizes_typed_content_and_rejects_missing_or_invalid_fields() {
        let elicitation = form();
        let mut content = elicitation.initial_content();
        content.insert("project".to_owned(), json!("B"));
        content.insert("labels".to_owned(), json!(["bug"]));
        content.insert("count".to_owned(), json!("3"));
        content.insert("note".to_owned(), json!("ship it"));
        let (accepted, response) = elicitation
            .response(DesktopMcpElicitationAction::Accept, Some(&content))
            .unwrap();
        assert!(accepted);
        assert_eq!(response["action"], "accept");
        assert_eq!(response["content"]["project"], "B");
        assert_eq!(response["content"]["count"], 3.0);
        assert_eq!(response["content"]["notify"], true);

        content.insert("project".to_owned(), json!("unknown"));
        assert!(matches!(
            elicitation.response(DesktopMcpElicitationAction::Accept, Some(&content)),
            Err(DesktopMcpElicitationError::InvalidField { .. })
        ));
        content.insert("project".to_owned(), json!("A"));
        content.insert("labels".to_owned(), json!([]));
        assert!(matches!(
            elicitation.response(DesktopMcpElicitationAction::Accept, Some(&content)),
            Err(DesktopMcpElicitationError::RequiredField(field)) if field == "labels"
        ));
    }

    #[test]
    fn decline_cancel_and_url_actions_do_not_invent_form_content() {
        let elicitation = form();
        assert_eq!(
            elicitation
                .response(DesktopMcpElicitationAction::Decline, None)
                .unwrap(),
            (false, json!({"action": "decline"}))
        );
        assert_eq!(
            elicitation
                .response(DesktopMcpElicitationAction::Cancel, None)
                .unwrap(),
            (false, json!({"action": "cancel"}))
        );

        let url = DesktopMcpElicitation::from_payload(&json!({
            "threadId": "thread-1",
            "serverName": "auth",
            "mode": "url",
            "message": "Open authorization page",
            "url": "https://example.test/authorize"
        }))
        .unwrap();
        assert_eq!(
            url.response(DesktopMcpElicitationAction::Accept, None)
                .unwrap(),
            (true, json!({"action": "accept"}))
        );
    }
}
