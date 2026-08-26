//! Timeline domain feature.
//!
//! Reads the projection the agent runtime maintains: page a task's timeline and
//! recover the payload behind a failed turn. The timeline itself is written by
//! the agent runtime, so this feature owns queries only.

mod retry;

use std::sync::Arc;

use lilia_contracts::{
    ProductError, TaskId, TimelineProjectionCursor, TimelineProjectionEvent, TimelineProjectionPage,
};
use lilia_kernel::{
    Event, Feature, FeatureContext, FeatureId, KernelError, ServiceKey, ServiceRef,
};
use lilia_service::ServiceAuthority;

pub use retry::{timeline_retry_context, TimelineRetryContext};

/// A task's timeline projection advanced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineChanged {
    pub task_id: TaskId,
    pub cursor: Option<u64>,
}

impl Event for TimelineChanged {
    const NAME: &'static str = "lilia.timeline.changed";

    fn subject(&self) -> Option<String> {
        Some(self.task_id.as_str().to_owned())
    }
}

/// Largest page a caller may request in one read.
const MAX_PAGE: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum TimelineError {
    #[error(transparent)]
    Product(#[from] ProductError),
    #[error("invalid desktop input `{field}`: {message}")]
    InvalidInput {
        field: &'static str,
        message: String,
    },
}

/// Read authority over task timelines.
pub struct TimelineService {
    authority: ServiceAuthority,
}

impl TimelineService {
    pub fn new(authority: ServiceAuthority) -> Self {
        Self { authority }
    }

    /// Full timeline of a task, oldest first.
    pub fn events(&self, task_id: &TaskId) -> Vec<TimelineProjectionEvent> {
        self.authority
            .shared_runtime()
            .inner()
            .product_timeline_for_task(task_id)
    }

    /// One page ending at `before`, newest page first when `before` is `None`.
    pub fn page(
        &self,
        task_id: &TaskId,
        before: Option<&TimelineProjectionCursor>,
        limit: usize,
    ) -> Result<TimelineProjectionPage, TimelineError> {
        Ok(self
            .authority
            .shared_runtime()
            .inner()
            .product_timeline_page_before(task_id, before, limit.clamp(1, MAX_PAGE))?)
    }

    /// Retry payload for `event_id`, or an error naming why the event cannot be
    /// retried.
    pub fn retry_context(
        &self,
        task_id: &TaskId,
        event_id: &str,
    ) -> Result<TimelineRetryContext, TimelineError> {
        let events = self.events(task_id);
        let event = events
            .iter()
            .find(|event| event.id.as_str() == event_id)
            .ok_or_else(|| TimelineError::InvalidInput {
                field: "event_id",
                message: "timeline event does not exist for this task".to_owned(),
            })?;
        timeline_retry_context(event, &events).ok_or_else(|| TimelineError::InvalidInput {
            field: "event_id",
            message: "timeline event has no retryable message context".to_owned(),
        })
    }
}

/// Service slot for [`TimelineService`].
pub enum TimelineServiceKey {}

impl ServiceKey for TimelineServiceKey {
    type Value = Arc<TimelineService>;

    const NAME: &'static str = "lilia.timeline";
}

pub struct TimelineFeature {
    authority: ServiceAuthority,
}

impl TimelineFeature {
    pub fn new(authority: ServiceAuthority) -> Self {
        Self { authority }
    }
}

impl Feature for TimelineFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.timeline").expect("the timeline feature id is not blank")
    }

    fn provides(&self) -> Vec<ServiceRef> {
        vec![ServiceRef::of::<TimelineServiceKey>()]
    }

    fn mount(&self, cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        cx.provide::<TimelineServiceKey>(Arc::new(TimelineService::new(self.authority.clone())))
    }
}
