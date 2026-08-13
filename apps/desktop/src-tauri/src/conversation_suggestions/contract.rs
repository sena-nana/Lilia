use super::types::SuggestionSource;

use std::sync::OnceLock;

use serde::Deserialize;

const SUGGESTIONS_CONTRACT_JSON: &str =
    include_str!("../../../../../packages/contracts/src/suggestions-contract.json");

static SUGGESTIONS_CONTRACT: OnceLock<SuggestionsContract> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionsContract {
    #[cfg(test)]
    commands: SuggestionCommandsContract,
    suggestion_sources: Vec<SuggestionSource>,
    default_suggestion_source: SuggestionSource,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::conversation_suggestions) struct SuggestionCommandsContract {
    pub(in crate::conversation_suggestions) get: String,
    pub(in crate::conversation_suggestions) get_sources: String,
    pub(in crate::conversation_suggestions) get_settings: String,
    pub(in crate::conversation_suggestions) set_settings: String,
}

fn suggestions_contract() -> &'static SuggestionsContract {
    SUGGESTIONS_CONTRACT.get_or_init(|| {
        crate::contract_manifest::parse_contract_json(
            SUGGESTIONS_CONTRACT_JSON,
            "suggestions-contract.json",
        )
    })
}

#[cfg(test)]
pub(in crate::conversation_suggestions) fn commands() -> &'static SuggestionCommandsContract {
    &suggestions_contract().commands
}

pub(super) fn default_suggestion_source() -> SuggestionSource {
    let contract = suggestions_contract();
    assert!(
        contract
            .suggestion_sources
            .contains(&contract.default_suggestion_source),
        "suggestions-contract.json defaultSuggestionSource must be listed in suggestionSources"
    );
    contract.default_suggestion_source
}
