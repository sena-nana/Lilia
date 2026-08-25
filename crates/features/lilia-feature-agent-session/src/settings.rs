use serde::{Deserialize, Serialize};

/// Which parts of a turn the automatic tier decision may override.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAutoTurnDecisionSettings {
    pub enabled: bool,
    pub allow_model_tier: bool,
    pub allow_reasoning_effort: bool,
    pub allow_plan_mode: bool,
    pub allow_goal_mode: bool,
    pub allow_session_fork: bool,
}

impl Default for DesktopAutoTurnDecisionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_model_tier: true,
            allow_reasoning_effort: true,
            allow_plan_mode: true,
            allow_goal_mode: true,
            allow_session_fork: true,
        }
    }
}
