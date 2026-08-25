use std::collections::{BTreeMap, BTreeSet};

use lilia_agent::NativeSubagentDefinition;
use lilia_storage::SqliteAgentRuntimeStateStore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::{DesktopApplication, DesktopApplicationError, DesktopEventKind};

const AGENT_INTERACTION_SETTINGS_KEY: &str = "agent.interaction.v1";
const AGENT_INTERACTION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSubagentModeSettings {
    pub enabled: bool,
    pub forward_subagent_text: bool,
    pub agent_progress_summaries: bool,
}

impl Default for DesktopSubagentModeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            forward_subagent_text: true,
            agent_progress_summaries: true,
        }
    }
}

pub use lilia_feature_agent_session::DesktopAutoTurnDecisionSettings;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAgentInteractionSettings {
    pub revision: u64,
    pub non_interrupt_mode: bool,
    pub debug: bool,
    pub permission_mode: String,
    pub permission_mode_availability: BTreeMap<String, bool>,
    pub main_agent_prompt_mode: String,
    pub main_agent_custom_prompt: String,
    pub subagent_mode: DesktopSubagentModeSettings,
    pub auto_turn_decision: DesktopAutoTurnDecisionSettings,
}

impl Default for DesktopAgentInteractionSettings {
    fn default() -> Self {
        Self {
            revision: 1,
            non_interrupt_mode: false,
            debug: false,
            permission_mode: "ask".into(),
            permission_mode_availability: default_permission_mode_availability(),
            main_agent_prompt_mode: "conservative".into(),
            main_agent_custom_prompt: String::new(),
            subagent_mode: DesktopSubagentModeSettings::default(),
            auto_turn_decision: DesktopAutoTurnDecisionSettings::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAgentInteractionSettingsUpdate {
    pub expected_revision: u64,
    pub non_interrupt_mode: bool,
    pub debug: bool,
    pub permission_mode: String,
    pub permission_mode_availability: BTreeMap<String, bool>,
    pub main_agent_prompt_mode: String,
    pub main_agent_custom_prompt: String,
    pub subagent_mode: DesktopSubagentModeSettings,
    pub auto_turn_decision: DesktopAutoTurnDecisionSettings,
}

impl DesktopAgentInteractionSettingsUpdate {
    pub fn from_settings(settings: &DesktopAgentInteractionSettings) -> Self {
        Self {
            expected_revision: settings.revision,
            non_interrupt_mode: settings.non_interrupt_mode,
            debug: settings.debug,
            permission_mode: settings.permission_mode.clone(),
            permission_mode_availability: settings.permission_mode_availability.clone(),
            main_agent_prompt_mode: settings.main_agent_prompt_mode.clone(),
            main_agent_custom_prompt: settings.main_agent_custom_prompt.clone(),
            subagent_mode: settings.subagent_mode.clone(),
            auto_turn_decision: settings.auto_turn_decision.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCustomSubagentDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub enabled: bool,
}

impl From<DesktopCustomSubagentDefinition> for NativeSubagentDefinition {
    fn from(value: DesktopCustomSubagentDefinition) -> Self {
        Self {
            id: value.id,
            name: value.name,
            description: value.description,
            instruction: value.instruction,
            enabled: value.enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCustomSubagentCatalog {
    pub revision: u64,
    pub agents: Vec<DesktopCustomSubagentDefinition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCustomSubagentUpsert {
    pub expected_revision: u64,
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAgentInteractionState {
    schema_version: u32,
    settings: DesktopAgentInteractionSettings,
    agents: Vec<DesktopCustomSubagentDefinition>,
}

pub(crate) struct DesktopAgentInteractionState {
    store: SqliteAgentRuntimeStateStore,
    settings: DesktopAgentInteractionSettings,
    pub(crate) agents: Vec<DesktopCustomSubagentDefinition>,
}

impl DesktopAgentInteractionState {
    pub(crate) fn open(
        store: SqliteAgentRuntimeStateStore,
    ) -> Result<Self, DesktopAgentInteractionError> {
        let (settings, agents) = match store
            .setting(AGENT_INTERACTION_SETTINGS_KEY)
            .map_err(persistence_error)?
        {
            Some(value) => {
                let stored: StoredAgentInteractionState = serde_json::from_value(value)
                    .map_err(|error| DesktopAgentInteractionError::Corrupt(error.to_string()))?;
                if stored.schema_version != AGENT_INTERACTION_SCHEMA_VERSION {
                    return Err(DesktopAgentInteractionError::UnsupportedSchema(
                        stored.schema_version,
                    ));
                }
                (
                    validate_settings(stored.settings)?,
                    validate_catalog(stored.agents)?,
                )
            }
            None => (DesktopAgentInteractionSettings::default(), Vec::new()),
        };
        Ok(Self {
            store,
            settings,
            agents,
        })
    }

    pub(crate) fn settings(&self) -> DesktopAgentInteractionSettings {
        self.settings.clone()
    }

    pub(crate) fn catalog(&self) -> DesktopCustomSubagentCatalog {
        DesktopCustomSubagentCatalog {
            revision: self.settings.revision,
            agents: self.agents.clone(),
        }
    }

    fn prepare_settings(
        &self,
        update: DesktopAgentInteractionSettingsUpdate,
    ) -> Result<DesktopAgentInteractionSettings, DesktopAgentInteractionError> {
        self.ensure_revision(update.expected_revision)?;
        validate_settings(DesktopAgentInteractionSettings {
            revision: next_revision(self.settings.revision)?,
            non_interrupt_mode: update.non_interrupt_mode,
            debug: update.debug,
            permission_mode: update.permission_mode,
            permission_mode_availability: update.permission_mode_availability,
            main_agent_prompt_mode: update.main_agent_prompt_mode,
            main_agent_custom_prompt: update.main_agent_custom_prompt,
            subagent_mode: update.subagent_mode,
            auto_turn_decision: update.auto_turn_decision,
        })
    }

    fn prepare_upsert(
        &self,
        input: DesktopCustomSubagentUpsert,
    ) -> Result<
        (
            DesktopAgentInteractionSettings,
            Vec<DesktopCustomSubagentDefinition>,
            DesktopCustomSubagentDefinition,
        ),
        DesktopAgentInteractionError,
    > {
        self.ensure_revision(input.expected_revision)?;
        let id = normalize_optional(input.id).unwrap_or_else(|| Uuid::new_v4().to_string());
        let next_agent = validate_agent(DesktopCustomSubagentDefinition {
            id: id.clone(),
            name: input.name,
            description: input.description,
            instruction: input.instruction,
            enabled: input.enabled,
        })?;
        let mut agents = self.agents.clone();
        if let Some(index) = agents.iter().position(|agent| agent.id == id) {
            agents[index] = next_agent.clone();
        } else {
            agents.push(next_agent.clone());
        }
        let agents = validate_catalog(agents)?;
        let mut settings = self.settings.clone();
        settings.revision = next_revision(settings.revision)?;
        Ok((settings, agents, next_agent))
    }

    fn prepare_delete(
        &self,
        expected_revision: u64,
        id: &str,
    ) -> Result<
        (
            DesktopAgentInteractionSettings,
            Vec<DesktopCustomSubagentDefinition>,
        ),
        DesktopAgentInteractionError,
    > {
        self.ensure_revision(expected_revision)?;
        let id = id.trim();
        if id.is_empty() {
            return Err(DesktopAgentInteractionError::InvalidAgentId);
        }
        let mut agents = self.agents.clone();
        let before = agents.len();
        agents.retain(|agent| agent.id != id);
        if agents.len() == before {
            return Err(DesktopAgentInteractionError::AgentNotFound(id.to_owned()));
        }
        let mut settings = self.settings.clone();
        settings.revision = next_revision(settings.revision)?;
        Ok((settings, agents))
    }

    fn ensure_revision(&self, expected: u64) -> Result<(), DesktopAgentInteractionError> {
        if expected != self.settings.revision {
            return Err(DesktopAgentInteractionError::RevisionConflict {
                expected,
                actual: self.settings.revision,
            });
        }
        Ok(())
    }

    fn persist(
        &self,
        settings: &DesktopAgentInteractionSettings,
        agents: &[DesktopCustomSubagentDefinition],
    ) -> Result<(), DesktopAgentInteractionError> {
        let value = serde_json::to_value(StoredAgentInteractionState {
            schema_version: AGENT_INTERACTION_SCHEMA_VERSION,
            settings: settings.clone(),
            agents: agents.to_vec(),
        })
        .map_err(|error| DesktopAgentInteractionError::Persistence(error.to_string()))?;
        self.store
            .put_setting(AGENT_INTERACTION_SETTINGS_KEY, &value)
            .map_err(persistence_error)
    }

    pub(crate) fn runtime_definitions(
        settings: &DesktopAgentInteractionSettings,
        agents: &[DesktopCustomSubagentDefinition],
    ) -> Vec<NativeSubagentDefinition> {
        agents
            .iter()
            .cloned()
            .map(|mut agent| {
                agent.enabled &= settings.subagent_mode.enabled;
                agent.into()
            })
            .collect()
    }
}

impl DesktopApplication {
    pub fn agent_interaction_settings(
        &self,
    ) -> Result<DesktopAgentInteractionSettings, DesktopApplicationError> {
        self.inner
            .agent_interaction
            .lock()
            .map(|state| state.settings())
            .map_err(|_| DesktopAgentInteractionError::StateUnavailable.into())
    }

    pub fn save_agent_interaction_settings(
        &self,
        update: DesktopAgentInteractionSettingsUpdate,
    ) -> Result<DesktopAgentInteractionSettings, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut state = self
            .inner
            .agent_interaction
            .lock()
            .map_err(|_| DesktopAgentInteractionError::StateUnavailable)?;
        let previous_settings = state.settings();
        let previous_agents = state.agents.clone();
        let next_settings = state.prepare_settings(update)?;
        state.persist(&next_settings, &state.agents)?;
        if let Err(error) =
            runtime
                .inner()
                .configure_subagents(DesktopAgentInteractionState::runtime_definitions(
                    &next_settings,
                    &state.agents,
                ))
        {
            let rollback = state.persist(&previous_settings, &previous_agents);
            return Err(DesktopAgentInteractionError::RuntimeApply {
                message: error.to_string(),
                rollback_failed: rollback.err().map(|error| error.to_string()),
            }
            .into());
        }
        state.settings = next_settings.clone();
        drop(state);
        self.emit_event(DesktopEventKind::AgentInteractionChanged {
            revision: next_settings.revision,
        });
        Ok(next_settings)
    }

    pub fn custom_subagent_catalog(
        &self,
    ) -> Result<DesktopCustomSubagentCatalog, DesktopApplicationError> {
        self.inner
            .agent_interaction
            .lock()
            .map(|state| state.catalog())
            .map_err(|_| DesktopAgentInteractionError::StateUnavailable.into())
    }

    pub fn upsert_custom_subagent(
        &self,
        input: DesktopCustomSubagentUpsert,
    ) -> Result<DesktopCustomSubagentDefinition, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut state = self
            .inner
            .agent_interaction
            .lock()
            .map_err(|_| DesktopAgentInteractionError::StateUnavailable)?;
        let previous_settings = state.settings();
        let previous_agents = state.agents.clone();
        let (next_settings, next_agents, saved) = state.prepare_upsert(input)?;
        state.persist(&next_settings, &next_agents)?;
        if let Err(error) =
            runtime
                .inner()
                .configure_subagents(DesktopAgentInteractionState::runtime_definitions(
                    &next_settings,
                    &next_agents,
                ))
        {
            let rollback = state.persist(&previous_settings, &previous_agents);
            return Err(DesktopAgentInteractionError::RuntimeApply {
                message: error.to_string(),
                rollback_failed: rollback.err().map(|error| error.to_string()),
            }
            .into());
        }
        state.settings = next_settings.clone();
        state.agents = next_agents;
        drop(state);
        self.emit_event(DesktopEventKind::AgentInteractionChanged {
            revision: next_settings.revision,
        });
        Ok(saved)
    }

    pub fn delete_custom_subagent(
        &self,
        expected_revision: u64,
        id: &str,
    ) -> Result<DesktopCustomSubagentCatalog, DesktopApplicationError> {
        let runtime = self.authority().shared_runtime();
        let mut state = self
            .inner
            .agent_interaction
            .lock()
            .map_err(|_| DesktopAgentInteractionError::StateUnavailable)?;
        let previous_settings = state.settings();
        let previous_agents = state.agents.clone();
        let (next_settings, next_agents) = state.prepare_delete(expected_revision, id)?;
        state.persist(&next_settings, &next_agents)?;
        if let Err(error) =
            runtime
                .inner()
                .configure_subagents(DesktopAgentInteractionState::runtime_definitions(
                    &next_settings,
                    &next_agents,
                ))
        {
            let rollback = state.persist(&previous_settings, &previous_agents);
            return Err(DesktopAgentInteractionError::RuntimeApply {
                message: error.to_string(),
                rollback_failed: rollback.err().map(|error| error.to_string()),
            }
            .into());
        }
        state.settings = next_settings.clone();
        state.agents = next_agents;
        let catalog = state.catalog();
        drop(state);
        self.emit_event(DesktopEventKind::AgentInteractionChanged {
            revision: next_settings.revision,
        });
        Ok(catalog)
    }
}

fn validate_settings(
    mut settings: DesktopAgentInteractionSettings,
) -> Result<DesktopAgentInteractionSettings, DesktopAgentInteractionError> {
    if settings.revision == 0 {
        return Err(DesktopAgentInteractionError::Corrupt(
            "revision must be positive".into(),
        ));
    }
    settings.permission_mode_availability =
        normalize_permission_availability(settings.permission_mode_availability);
    settings.permission_mode = settings.permission_mode.trim().to_lowercase();
    if !matches!(
        settings.permission_mode.as_str(),
        "full" | "ask" | "readonly" | "free"
    ) || !settings
        .permission_mode_availability
        .get(&settings.permission_mode)
        .copied()
        .unwrap_or(false)
    {
        settings.permission_mode = "ask".into();
    }
    settings.main_agent_prompt_mode = settings.main_agent_prompt_mode.trim().to_lowercase();
    if !matches!(
        settings.main_agent_prompt_mode.as_str(),
        "conservative" | "aggressive" | "custom"
    ) {
        return Err(DesktopAgentInteractionError::InvalidPromptMode);
    }
    settings.main_agent_custom_prompt = settings.main_agent_custom_prompt.trim().to_owned();
    if settings.main_agent_custom_prompt.chars().count() > 16_000
        || has_forbidden_multiline_control(&settings.main_agent_custom_prompt)
    {
        return Err(DesktopAgentInteractionError::InvalidCustomPrompt);
    }
    Ok(settings)
}

fn validate_catalog(
    agents: Vec<DesktopCustomSubagentDefinition>,
) -> Result<Vec<DesktopCustomSubagentDefinition>, DesktopAgentInteractionError> {
    if agents.len() > 64 {
        return Err(DesktopAgentInteractionError::CatalogLimit);
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut normalized = Vec::with_capacity(agents.len());
    for agent in agents {
        let agent = validate_agent(agent)?;
        if !ids.insert(agent.id.clone()) {
            return Err(DesktopAgentInteractionError::DuplicateAgentId(agent.id));
        }
        if !names.insert(agent.name.to_lowercase()) {
            return Err(DesktopAgentInteractionError::DuplicateAgentName(agent.name));
        }
        normalized.push(agent);
    }
    normalized.sort_by_key(|agent| agent.name.to_lowercase());
    Ok(normalized)
}

fn validate_agent(
    mut agent: DesktopCustomSubagentDefinition,
) -> Result<DesktopCustomSubagentDefinition, DesktopAgentInteractionError> {
    agent.id = agent.id.trim().to_owned();
    agent.name = agent.name.trim().to_owned();
    agent.description = agent.description.trim().to_owned();
    agent.instruction = agent.instruction.trim().to_owned();
    NativeSubagentDefinition::from(agent.clone())
        .validate()
        .map_err(|message| DesktopAgentInteractionError::InvalidAgent(message.into()))?;
    Ok(agent)
}

fn default_permission_mode_availability() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("full".into(), true),
        ("ask".into(), true),
        ("readonly".into(), true),
        ("free".into(), true),
    ])
}

fn normalize_permission_availability(
    mut availability: BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    availability.retain(|key, _| matches!(key.as_str(), "full" | "ask" | "readonly" | "free"));
    for mode in ["full", "ask", "readonly", "free"] {
        availability.entry(mode.into()).or_insert(true);
    }
    availability.insert("ask".into(), true);
    availability.insert("readonly".into(), true);
    availability
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn has_forbidden_multiline_control(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn next_revision(current: u64) -> Result<u64, DesktopAgentInteractionError> {
    current
        .checked_add(1)
        .ok_or(DesktopAgentInteractionError::RevisionOverflow)
}

fn persistence_error(error: lilia_contracts::ProductError) -> DesktopAgentInteractionError {
    DesktopAgentInteractionError::Persistence(error.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesktopAgentInteractionError {
    #[error("agent interaction state is unavailable")]
    StateUnavailable,
    #[error("agent interaction settings revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("agent interaction settings revision overflowed")]
    RevisionOverflow,
    #[error("agent interaction settings use unsupported schema version {0}")]
    UnsupportedSchema(u32),
    #[error("agent interaction settings are corrupt: {0}")]
    Corrupt(String),
    #[error("main Agent prompt mode is invalid")]
    InvalidPromptMode,
    #[error("main Agent custom prompt is invalid")]
    InvalidCustomPrompt,
    #[error("custom Agent is invalid: {0}")]
    InvalidAgent(String),
    #[error("custom Agent id must not be empty")]
    InvalidAgentId,
    #[error("custom Agent `{0}` was not found")]
    AgentNotFound(String),
    #[error("custom Agent id `{0}` is duplicated")]
    DuplicateAgentId(String),
    #[error("custom Agent name `{0}` is duplicated")]
    DuplicateAgentName(String),
    #[error("custom Agent catalog exceeds 64 entries")]
    CatalogLimit,
    #[error("agent interaction persistence failed: {0}")]
    Persistence(String),
    #[error(
        "agent interaction settings could not be applied: {message}{rollback}",
        rollback = rollback_failed
            .as_ref()
            .map(|value| format!("; rollback failed: {value}"))
            .unwrap_or_default()
    )]
    RuntimeApply {
        message: String,
        rollback_failed: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lilia_service::ServiceAuthority;

    use super::*;
    use crate::application::{
        DesktopApplicationConfig, DesktopHost, DesktopHostAction, DesktopHostContext,
        DesktopHostError, DesktopHostResult,
    };

    struct TestHost;

    impl DesktopHost for TestHost {
        fn execute(
            &self,
            _context: &DesktopHostContext,
            _action: DesktopHostAction,
        ) -> Result<DesktopHostResult, DesktopHostError> {
            Ok(DesktopHostResult::Completed)
        }
    }

    fn application() -> DesktopApplication {
        let authority = ServiceAuthority::bootstrap_in_memory_named(
            format!("agent-interaction-test:{}", Uuid::new_v4()),
            "agent-interaction-test",
        )
        .unwrap();
        DesktopApplication::from_authority(
            DesktopApplicationConfig::new(
                "C:/lilia/agent-interaction-test",
                "agent-interaction-test",
            )
            .unwrap(),
            authority,
            Arc::new(TestHost),
        )
        .unwrap()
    }

    #[test]
    fn custom_agent_catalog_is_revisioned_and_hot_applied_only_when_enabled() {
        let application = application();
        let settings = application.agent_interaction_settings().unwrap();
        assert_eq!(settings.revision, 1);
        let saved = application
            .upsert_custom_subagent(DesktopCustomSubagentUpsert {
                expected_revision: 1,
                id: Some("reviewer".into()),
                name: " Reviewer ".into(),
                description: " architecture ".into(),
                instruction: "Review ownership.\nReturn evidence.".into(),
                enabled: true,
            })
            .unwrap();
        assert_eq!(saved.name, "Reviewer");
        assert!(
            !application
                .authority()
                .shared_runtime()
                .inner()
                .subagent_configuration()
                .unwrap()[0]
                .enabled
        );

        let settings = application.agent_interaction_settings().unwrap();
        let mut update = DesktopAgentInteractionSettingsUpdate::from_settings(&settings);
        update.subagent_mode.enabled = true;
        let settings = application.save_agent_interaction_settings(update).unwrap();
        assert_eq!(settings.revision, 3);
        assert!(
            application
                .authority()
                .shared_runtime()
                .inner()
                .subagent_configuration()
                .unwrap()[0]
                .enabled
        );

        let stale = application.upsert_custom_subagent(DesktopCustomSubagentUpsert {
            expected_revision: 2,
            id: Some("reviewer".into()),
            name: "Reviewer".into(),
            description: String::new(),
            instruction: "stale".into(),
            enabled: true,
        });
        assert!(matches!(
            stale,
            Err(DesktopApplicationError::AgentInteraction(
                DesktopAgentInteractionError::RevisionConflict { .. }
            ))
        ));
    }

    #[test]
    fn duplicate_names_and_invalid_multiline_controls_do_not_mutate_catalog() {
        let application = application();
        application
            .upsert_custom_subagent(DesktopCustomSubagentUpsert {
                expected_revision: 1,
                id: Some("one".into()),
                name: "Reviewer".into(),
                description: String::new(),
                instruction: "Review\ncarefully".into(),
                enabled: true,
            })
            .unwrap();
        let duplicate = application.upsert_custom_subagent(DesktopCustomSubagentUpsert {
            expected_revision: 2,
            id: Some("two".into()),
            name: " reviewer ".into(),
            description: String::new(),
            instruction: "Other".into(),
            enabled: true,
        });
        assert!(matches!(
            duplicate,
            Err(DesktopApplicationError::AgentInteraction(
                DesktopAgentInteractionError::DuplicateAgentName(_)
            ))
        ));
        let catalog = application.custom_subagent_catalog().unwrap();
        assert_eq!(catalog.revision, 2);
        assert_eq!(catalog.agents.len(), 1);
    }
}
