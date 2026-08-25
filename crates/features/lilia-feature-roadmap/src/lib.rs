//! Roadmap domain feature.
//!
//! Owns milestones and the links that place tasks on them.

mod service;
mod sqlite;
mod store;
mod types;

pub use service::DesktopRoadmapService;
pub use sqlite::SqliteRoadmapStore;
pub use store::{RoadmapStore, RoadmapStoreError};
pub use types::{
    Milestone, MilestoneDueDateUpdate, MilestoneStatus, MilestoneUpdatePatch, ProjectRoadmap,
    TaskMilestoneLink,
};

use lilia_kernel::{Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef};

/// Service slot for [`DesktopRoadmapService`].
pub enum RoadmapServiceKey {}

impl ServiceKey for RoadmapServiceKey {
    type Value = DesktopRoadmapService;

    const NAME: &'static str = "lilia.roadmap";
}

pub struct RoadmapFeature {
    service: DesktopRoadmapService,
}

impl RoadmapFeature {
    pub fn new(service: DesktopRoadmapService) -> Self {
        Self { service }
    }
}

impl Feature for RoadmapFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.roadmap").expect("the roadmap feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<RoadmapServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<RoadmapServiceKey>(self.service.clone())
    }
}
