use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

const CHAT_BACKENDS_JSON: &str =
    include_str!("../../../../packages/contracts/src/chat-backends.json");

static CHAT_BACKENDS_CONTRACT: OnceLock<ChatBackendsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatBackendsContract {
    pub(crate) chat_backends: Vec<String>,
    pub(crate) default_backend: String,
    pub(crate) default_models: BTreeMap<String, String>,
    pub(crate) backend_models: BTreeMap<String, Vec<ChatModelOptionContract>>,
    pub(crate) allowed_model_prefixes: BTreeMap<String, Vec<String>>,
    pub(crate) reasoning_efforts: Vec<String>,
    pub(crate) backend_reasoning_efforts: BTreeMap<String, Vec<String>>,
    pub(crate) direct_urls: BTreeMap<String, String>,
    pub(crate) api_key_env: BTreeMap<String, String>,
    pub(crate) provider_store_keys: BTreeMap<String, String>,
    pub(crate) router_store_keys: BTreeMap<String, String>,
    pub(crate) backend_router_modes: BTreeMap<String, Vec<String>>,
    pub(crate) default_router_modes: BTreeMap<String, String>,
    pub(crate) connection_modes_using_api_key: Vec<String>,
    pub(crate) connection_modes_using_default_api: Vec<String>,
    pub(crate) connection_modes_using_custom_url: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatModelOptionContract {
    pub(crate) id: String,
    pub(crate) label: String,
}

pub(crate) fn chat_backends_contract() -> &'static ChatBackendsContract {
    CHAT_BACKENDS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(CHAT_BACKENDS_JSON, "chat-backends.json")
    })
}
