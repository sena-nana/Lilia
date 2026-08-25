//! LiliaCode micro-kernel.
//!
//! The kernel owns five mechanisms and no product knowledge:
//!
//! - [`ServiceRegistry`] — typed slots a feature provides and consumers resolve.
//! - [`ContributionRegistry`] — ordered collections features append to, keeping
//!   host vocabulary (UI, agent tools, migrations) out of the kernel.
//! - [`EventBus`] — typed topics with one shared monotonic sequence.
//! - [`Journal`] — append-only record of mutations, jobs and lifecycle, so a
//!   reader can reconstruct causality without querying each feature.
//! - [`Jobs`] — the single entry point for long work, backed by a
//!   [`TaskRuntime`] port.
//!
//! Product capability enters through [`Feature`], whose registrations the kernel
//! records so [`Kernel::unmount`] can reverse them.

mod contribution;
mod error;
mod event;
mod feature;
mod id;
mod job;
mod journal;
mod kernel;
mod service;

pub use contribution::{Contribution, ContributionRegistry};
pub use error::KernelError;
pub use event::{Event, EventBus, EventEnvelope, SubscriptionId};
pub use feature::{Feature, FeatureContext};
pub use id::{FeatureId, JobId, JobSlot, ServiceRef};
pub use job::{
    JobContext, JobError, JobEvent, JobHandle, JobHandler, JobProtocol, JobRequest, JobState, Jobs,
    TaskProgress, TaskRuntime, TaskSpec, TaskTicket,
};
pub use journal::{Journal, JournalRecord, JournalSink, RecordKind};
pub use kernel::Kernel;
pub use service::{ServiceKey, ServiceRegistry};

#[cfg(test)]
mod tests;
