use std::collections::BTreeSet;
use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::application::{
    DesktopApplication, DesktopAssistantAiModelPoolItem, DesktopSecret, ASSISTANT_AI_CREDENTIAL_KEY,
};

const ASSISTANT_AI_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MODELS_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopAssistantAiProbeInput {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<DesktopSecret>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAssistantAiModelsResult {
    pub ok: bool,
    pub error: Option<String>,
    pub models: Vec<DesktopAssistantAiModelPoolItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAssistantAiTestResult {
    pub ok: bool,
    pub error: Option<String>,
    pub models: Option<Vec<String>>,
    pub model_matched: Option<bool>,
}

impl DesktopApplication {
    pub fn fetch_assistant_ai_models(
        &self,
        input: DesktopAssistantAiProbeInput,
    ) -> DesktopAssistantAiModelsResult {
        let request = match self.resolve_assistant_ai_probe(input, false) {
            Ok(request) => request,
            Err(error) => {
                return DesktopAssistantAiModelsResult {
                    ok: false,
                    error: Some(error),
                    models: Vec::new(),
                };
            }
        };
        match request_model_ids(&request.base_url, &request.api_key) {
            Ok(models) => DesktopAssistantAiModelsResult {
                ok: true,
                error: None,
                models: models
                    .unwrap_or_default()
                    .into_iter()
                    .map(|id| DesktopAssistantAiModelPoolItem {
                        label: id.clone(),
                        id,
                        source: "remote".to_owned(),
                        backend: "native-agentkit".to_owned(),
                    })
                    .collect(),
            },
            Err(error) => DesktopAssistantAiModelsResult {
                ok: false,
                error: Some(error),
                models: Vec::new(),
            },
        }
    }

    pub fn test_assistant_ai_connection(
        &self,
        input: DesktopAssistantAiProbeInput,
    ) -> DesktopAssistantAiTestResult {
        let request = match self.resolve_assistant_ai_probe(input, true) {
            Ok(request) => request,
            Err(error) => {
                return DesktopAssistantAiTestResult {
                    ok: false,
                    error: Some(error),
                    models: None,
                    model_matched: None,
                };
            }
        };
        match request_model_ids(&request.base_url, &request.api_key) {
            Ok(models) => {
                let model_matched = models.as_ref().map(|models| {
                    models
                        .iter()
                        .any(|candidate| Some(candidate.as_str()) == request.model.as_deref())
                });
                DesktopAssistantAiTestResult {
                    ok: true,
                    error: None,
                    models,
                    model_matched,
                }
            }
            Err(error) => DesktopAssistantAiTestResult {
                ok: false,
                error: Some(error),
                models: None,
                model_matched: None,
            },
        }
    }

    fn resolve_assistant_ai_probe(
        &self,
        input: DesktopAssistantAiProbeInput,
        require_model: bool,
    ) -> Result<AssistantAiProbeRequest, String> {
        let settings = self
            .assistant_ai_settings()
            .map_err(|error| error.to_string())?;
        let base_url = normalize(input.base_url.or(settings.base_url))
            .ok_or_else(|| "Base URL 不能为空。".to_owned())?;
        let model = normalize(input.model.or(settings.model));
        if require_model && model.is_none() {
            return Err("模型不能为空。".to_owned());
        }
        let api_key = match input.api_key {
            Some(secret) => String::from_utf8(secret.into_inner())
                .map_err(|_| "API 密钥不是有效文本。".to_owned())?,
            None => self
                .read_host_credential_text_result(ASSISTANT_AI_CREDENTIAL_KEY)
                .map_err(|error| error.to_string())?
                .unwrap_or_default(),
        };
        let api_key = api_key.trim().to_owned();
        if api_key.is_empty() {
            return Err("API 密钥不能为空。".to_owned());
        }
        let mut url =
            reqwest::Url::parse(&base_url).map_err(|_| "Base URL 格式无效。".to_owned())?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Base URL 必须是无内联凭据的 HTTP(S) 地址。".to_owned());
        }
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "Base URL 无法追加模型路径。".to_owned())?;
            segments.pop_if_empty();
            segments.push("models");
        }
        Ok(AssistantAiProbeRequest {
            base_url: url,
            model,
            api_key,
        })
    }
}

struct AssistantAiProbeRequest {
    base_url: reqwest::Url,
    model: Option<String>,
    api_key: String,
}

fn request_model_ids(url: &reqwest::Url, api_key: &str) -> Result<Option<Vec<String>>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(ASSISTANT_AI_PROBE_TIMEOUT)
        .build()
        .map_err(|error| format!("无法创建模型服务连接：{error}"))?;
    let response = client
        .get(url.clone())
        .bearer_auth(api_key)
        .send()
        .map_err(|error| format!("模型服务请求失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("模型服务返回 HTTP {}。", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODELS_RESPONSE_BYTES)
    {
        return Err("模型列表响应过大。".to_owned());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_MODELS_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取模型列表：{error}"))?;
    if bytes.len() as u64 > MAX_MODELS_RESPONSE_BYTES {
        return Err("模型列表响应过大。".to_owned());
    }
    let value = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("模型列表响应格式无效：{error}"))?;
    let Some(data) = value.get("data") else {
        return Ok(None);
    };
    let Some(items) = data.as_array() else {
        return Err("模型列表响应中的 data 不是数组。".to_owned());
    };
    let mut seen = BTreeSet::new();
    let mut models = Vec::new();
    for id in items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if seen.insert(id.to_owned()) {
            models.push(id.to_owned());
        }
    }
    Ok(Some(models))
}

fn normalize(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };
    use lilia_service::ServiceAuthority;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    struct NoopHost;

    impl DesktopHost for NoopHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let data = tempfile::tempdir().unwrap().keep();
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("test:assistant-ai-probe:{}", uuid::Uuid::new_v4()),
            format!("assistant-ai-probe:{}", uuid::Uuid::new_v4()),
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(&data, "assistant-ai-probe").unwrap(),
            authority,
            Arc::new(NoopHost),
        )
        .unwrap()
    }

    fn models_server() -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let body =
                br#"{"data":[{"id":"model-alpha"},{"id":"model-beta"},{"id":"model-alpha"}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (format!("http://{address}/v1"), handle)
    }

    fn oversized_models_server() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            let body = vec![b'x'; MAX_MODELS_RESPONSE_BYTES as usize + 1];
            let _ = stream.write_all(&body);
        });
        (format!("http://{address}/v1"), handle)
    }

    #[test]
    fn fetch_and_test_models_share_bounded_authenticated_probe() {
        let app = application();
        let (base_url, fetch_request) = models_server();
        let fetched = app.fetch_assistant_ai_models(DesktopAssistantAiProbeInput {
            base_url: Some(base_url),
            model: None,
            api_key: Some(DesktopSecret::new(b"secret-token".to_vec())),
        });
        assert!(fetched.ok, "{:?}", fetched.error);
        assert_eq!(
            fetched
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["model-alpha", "model-beta"]
        );
        let request = fetch_request.join().unwrap();
        assert!(request.starts_with("GET /v1/models HTTP/1.1"));
        assert!(request.contains("authorization: Bearer secret-token"));

        let (base_url, test_request) = models_server();
        let tested = app.test_assistant_ai_connection(DesktopAssistantAiProbeInput {
            base_url: Some(base_url),
            model: Some("model-beta".to_owned()),
            api_key: Some(DesktopSecret::new(b"secret-token".to_vec())),
        });
        assert!(tested.ok, "{:?}", tested.error);
        assert_eq!(tested.model_matched, Some(true));
        assert_eq!(tested.models.as_ref().map(Vec::len), Some(2));
        let request = test_request.join().unwrap();
        assert!(request.starts_with("GET /v1/models HTTP/1.1"));
    }

    #[test]
    fn probe_rejects_embedded_credentials_before_network_access() {
        let result = application().fetch_assistant_ai_models(DesktopAssistantAiProbeInput {
            base_url: Some("https://user:password@example.com/v1".to_owned()),
            model: None,
            api_key: Some(DesktopSecret::new(b"secret-token".to_vec())),
        });

        assert!(!result.ok);
        assert!(result.models.is_empty());
    }

    #[test]
    fn probe_stops_reading_unbounded_responses_at_the_configured_limit() {
        let (base_url, server) = oversized_models_server();
        let result = application().fetch_assistant_ai_models(DesktopAssistantAiProbeInput {
            base_url: Some(base_url),
            model: None,
            api_key: Some(DesktopSecret::new(b"secret-token".to_vec())),
        });
        server.join().unwrap();

        assert!(!result.ok);
        assert_eq!(result.error.as_deref(), Some("模型列表响应过大。"));
    }
}
