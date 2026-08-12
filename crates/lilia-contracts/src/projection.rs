//! Product projection contracts for AgentKit events (#46 / #56).
//!
//! Timeline / Todo / Artifact / Pending rows here are product projections, not
//! Agent Runtime facts. Desktop SQLite may mirror timeline rows as a UI cache
//! only — never as the execution / recovery fact source.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{AgentSessionRef, TaskId};

/// Product timeline projection store id (payload / diagnostics).
pub const PRODUCT_TIMELINE_STORE_ID: &str = "lilia-storage";

/// Marker value: Desktop SQLite row is a rebuildable UI cache of product projection.
pub const TIMELINE_UI_CACHE_KIND: &str = "desktop-sqlite-ui-cache";

/// Product-facing approval decision (Core/Client never import AgentKit wire DTOs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductApprovalDecision {
    pub session_id: String,
    pub turn_id: String,
    pub action_id: String,
    pub version: u64,
    /// `true` = approve and execute tool; `false` = deny and stop tool.
    pub approved: bool,
}

/// Stable product projection identity derived from AgentKit session + sequence.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionEventId(String);

impl ProjectionEventId {
    pub fn from_session_sequence(session_id: &str, sequence: u64) -> Self {
        Self(format!("{session_id}:{sequence}"))
    }

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Product timeline projection row. Does not embed Agent Runtime private state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionEvent {
    pub id: ProjectionEventId,
    pub task_id: TaskId,
    pub agent_session: AgentSessionRef,
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub summary: Option<String>,
    pub payload: JsonValue,
    /// True when this row was produced by replaying AgentKit events.
    pub projected: bool,
}

/// Stable keyset cursor for task timeline pagination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionCursor {
    pub sequence: u64,
    pub event_id: String,
}

/// One chronological task timeline page read from the product projection store.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionPage {
    pub events: Vec<TimelineProjectionEvent>,
    pub before_cursor: Option<TimelineProjectionCursor>,
    pub has_more_before: bool,
}

/// Artifact product projection (refs + metadata only; no unbounded blobs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProjection {
    pub id: String,
    pub task_id: TaskId,
    pub agent_session: AgentSessionRef,
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub artifact_id: String,
    pub media_type: String,
    pub summary: String,
    pub kind: Option<String>,
    pub size_bytes: Option<u64>,
    pub content_hash: Option<String>,
    /// ResourceRef / materialized path JSON — never large content bodies.
    pub content_ref: Option<JsonValue>,
    pub provenance: Option<String>,
    pub status: String,
}

/// Agent Todo list projection (run checklist; not Product Task).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoProjection {
    pub id: String,
    pub task_id: TaskId,
    pub agent_session: AgentSessionRef,
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub todo_id: String,
    pub revision: u64,
    pub items: JsonValue,
}

/// Pending interaction / approval product projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingProjectionStatus {
    Open,
    Resolved,
    Expired,
    Cancelled,
    Stale,
}

/// Pending approval/question/plan-decision projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingProjection {
    pub id: String,
    pub task_id: TaskId,
    pub agent_session: AgentSessionRef,
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub request_id: String,
    pub kind: String,
    pub status: PendingProjectionStatus,
    pub prompt: Option<String>,
    pub action_revision: Option<u64>,
    pub payload: JsonValue,
}

/// Application command produced by the integration projector (no DB writes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineProjectionCommand {
    UpsertTimelineEvent {
        event: TimelineProjectionEvent,
    },
    UpsertArtifact {
        artifact: ArtifactProjection,
    },
    UpsertTodo {
        todo: TodoProjection,
    },
    UpsertPending {
        pending: PendingProjection,
    },
    ResolvePending {
        session_id: String,
        request_id: String,
        status: PendingProjectionStatus,
        sequence: u64,
        response: JsonValue,
    },
    SkipUnknown {
        session_id: String,
        sequence: u64,
        reason: String,
    },
}
