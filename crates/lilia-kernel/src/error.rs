use crate::{FeatureId, ServiceRef};

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("feature id must not be blank: {0:?}")]
    InvalidFeatureId(String),

    #[error("job slot must not be blank: {0:?}")]
    InvalidJobSlot(String),

    #[error("feature {0} is already mounted")]
    DuplicateFeature(FeatureId),

    #[error("service {service} is already provided by feature {provider}")]
    DuplicateService {
        service: &'static str,
        provider: FeatureId,
    },

    #[error("service {0} is not provided by any mounted feature")]
    MissingService(&'static str),

    #[error("feature {feature} requires service {service}, which no mounted feature provides")]
    UnsatisfiedRequirement {
        feature: FeatureId,
        service: &'static str,
    },

    #[error("feature dependency cycle across: {0}")]
    DependencyCycle(String),

    #[error("service {service} is registered with a mismatched value type")]
    ServiceTypeMismatch { service: &'static str },

    #[error("feature {feature} is not mounted")]
    UnknownFeature { feature: FeatureId },

    #[error("feature {feature} failed to mount: {source}")]
    Mount {
        feature: FeatureId,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error(transparent)]
    Job(#[from] crate::JobError),

    #[error("{0}")]
    Feature(String),
}

impl KernelError {
    pub fn feature(message: impl Into<String>) -> Self {
        Self::Feature(message.into())
    }

    pub(crate) fn duplicate_service(service: ServiceRef, provider: FeatureId) -> Self {
        Self::DuplicateService {
            service: service.name(),
            provider,
        }
    }
}
