//! Conversation suggestions domain feature.
//!
//! Owns the suggestion vocabulary, the prompt and response handling that turns a
//! scope into suggestion items, the local git signal collector, the cache key
//! that decides when a suggestion set is still fresh, and the persisted
//! settings. Reading the scope and calling a model stays with the host, which
//! owns the projects, the GitHub binding and the auxiliary model.

pub mod cache;
pub mod generation;
mod jobs;
pub mod local_git;
pub mod settings;
pub mod types;

use std::sync::Arc;

use lilia_kernel::{Feature, FeatureContext, FeatureId, JobProtocol, KernelError};

pub use jobs::{generate_slot, GenerateRequest, SuggestionPort, GENERATE_PROTOCOL};

pub struct SuggestionsFeature {
    port: Arc<dyn SuggestionPort>,
}

impl SuggestionsFeature {
    pub fn new(port: Arc<dyn SuggestionPort>) -> Self {
        Self { port }
    }
}

impl Feature for SuggestionsFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.suggestions")
            .expect("the suggestions feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        vec![jobs::generate_protocol(Arc::clone(&self.port))]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}
