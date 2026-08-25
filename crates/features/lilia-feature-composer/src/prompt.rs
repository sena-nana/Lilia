//! Prompt optimization protocol.
//!
//! Rewriting a draft calls an auxiliary model over the network, so it runs as a
//! kernel job rather than on the UI thread. Every editing surface (main window
//! and each task popup) gets its own single-flight lane, which is what the
//! shell used to emulate with `prompt_optimization_operation_sequence` plus a
//! per-window `PromptOptimizationState`.

use lilia_contracts::{ChatAttachment, ChatConversationReference, LiliaAgentWorkflow};
use lilia_kernel::JobSlot;
use serde::{Deserialize, Serialize};

pub const OPTIMIZE_PROMPT_PROTOCOL: &str = "lilia.composer/optimize-prompt@1";

/// Payload of [`OPTIMIZE_PROMPT_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOptimizeInput {
    pub prompt: String,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub conversation_references: Vec<ChatConversationReference>,
    #[serde(default)]
    pub project_cwd: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOptimizeResult {
    pub optimized_prompt: String,
    pub route: PromptRoute,
}

/// Scenario the router picked for a prompt, with the evidence behind it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRoute {
    pub scenario: String,
    pub workflow: Option<LiliaAgentWorkflow>,
    pub confidence: f64,
    pub reason: String,
    pub signals: Vec<String>,
}

/// Rewrites a draft through the auxiliary model. Implemented by the host, which
/// owns the model settings and the network client.
pub trait PromptOptimizePort: Send + Sync + 'static {
    fn optimize(&self, input: PromptOptimizeInput) -> Result<PromptOptimizeResult, String>;
}

/// Single-flight lane for one editing surface. Optimizing in the main window
/// must not cancel an optimization running in a task popup, so the surface
/// discriminator is part of the slot name.
pub fn optimize_prompt_slot(surface: u64) -> JobSlot {
    JobSlot::new(format!("lilia.composer.optimize-prompt.{surface}"))
        .expect("the prompt optimization slot name is not blank")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_editing_surface_gets_its_own_single_flight_lane() {
        assert_ne!(optimize_prompt_slot(0), optimize_prompt_slot(1));
        assert_eq!(
            optimize_prompt_slot(3).as_str(),
            "lilia.composer.optimize-prompt.3"
        );
    }
}
