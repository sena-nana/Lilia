//! Coding domain feature.
//!
//! Owns the read-only projections the workspace surface renders — git status,
//! git diff, directory listings and code search — plus the search job protocol.
//!
//! Searching a code index walks the filesystem across every eligible project,
//! so it runs as a kernel job on one single-flight lane. That lane replaces the
//! shell's `coding_operation_sequence` / `active_coding_operation` pair.

mod types;

use std::sync::Arc;

use lilia_kernel::{
    Feature, FeatureContext, FeatureId, JobContext, JobProtocol, JobSlot, KernelError,
};
use serde_json::Value;

pub use types::{
    CodeSearchHit, CodeSearchMode, CodeSearchResult, CodeSearchScope, GitChange, GitDiff,
    GitDiffScope, GitFileStatus, GitStatus, WorkspaceCodeSearchFailure, WorkspaceCodeSearchHit,
    WorkspaceCodeSearchResult, WorkspaceEntry, WorkspaceListing,
};

pub const SEARCH_PROTOCOL: &str = "lilia.code/search@1";

/// Payload of [`SEARCH_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub scope: CodeSearchScope,
    pub query: String,
    pub mode: CodeSearchMode,
}

/// Runs a code index search. Implemented by the host, which owns the agent
/// runtime's coding services.
pub trait CodeSearchPort: Send + Sync + 'static {
    fn search(&self, request: SearchRequest) -> Result<WorkspaceCodeSearchResult, String>;
}

/// Single-flight lane for the workspace surface: a new search supersedes the
/// one still running, because only the newest result is ever rendered.
pub fn search_slot() -> JobSlot {
    JobSlot::new("lilia.code.search").expect("the code search slot name is not blank")
}

pub const REFRESH_PROTOCOL: &str = "lilia.code/refresh@1";

/// Payload of [`REFRESH_PROTOCOL`]. The refresh gathers several views of the
/// selected workspace at once, and their shapes belong to the host, so the
/// request names only the ticket the host parked them under.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshRequest {
    pub ticket: u64,
}

/// Re-reads the coding services snapshot together with the git, workspace and
/// task views the surface renders beside it.
pub trait CodingRefreshPort: Send + Sync + 'static {
    fn refresh(&self, ticket: u64) -> Result<(), String>;
}

/// Single-flight lane for the workspace refresh, separate from search so a
/// refresh never cancels a search the operator is waiting on.
pub fn refresh_slot() -> JobSlot {
    JobSlot::new("lilia.code.refresh").expect("the code refresh slot name is not blank")
}

pub struct CodingFeature {
    search: Arc<dyn CodeSearchPort>,
    refresh: Arc<dyn CodingRefreshPort>,
}

impl CodingFeature {
    pub fn new(search: Arc<dyn CodeSearchPort>, refresh: Arc<dyn CodingRefreshPort>) -> Self {
        Self { search, refresh }
    }
}

impl Feature for CodingFeature {
    fn id(&self) -> FeatureId {
        FeatureId::new("lilia.feature.coding").expect("the coding feature id is not blank")
    }

    fn protocols(&self) -> Vec<JobProtocol> {
        let search = Arc::clone(&self.search);
        let refresh = Arc::clone(&self.refresh);
        vec![
            JobProtocol::new(
                SEARCH_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_search_job(payload, search.as_ref())
                }),
            ),
            JobProtocol::new(
                REFRESH_PROTOCOL,
                Arc::new(move |payload, _context: &JobContext| {
                    run_refresh_job(payload, refresh.as_ref())
                }),
            ),
        ]
    }

    fn mount(&self, _cx: &mut FeatureContext<'_>) -> Result<(), KernelError> {
        Ok(())
    }
}

fn run_search_job(payload: Value, port: &dyn CodeSearchPort) -> Result<Value, String> {
    let request: SearchRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid code search request: {error}"))?;
    let result = port.search(request)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn run_refresh_job(payload: Value, port: &dyn CodingRefreshPort) -> Result<Value, String> {
    let request: RefreshRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid code refresh request: {error}"))?;
    port.refresh(request.ticket)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingPort;

    impl CodeSearchPort for FailingPort {
        fn search(&self, _request: SearchRequest) -> Result<WorkspaceCodeSearchResult, String> {
            Err("no project has an index".to_owned())
        }
    }

    struct FailingRefresh;

    impl CodingRefreshPort for FailingRefresh {
        fn refresh(&self, _ticket: u64) -> Result<(), String> {
            Err("the workspace is gone".to_owned())
        }
    }

    #[test]
    fn the_feature_declares_both_protocols_before_any_mount() {
        let protocols =
            CodingFeature::new(Arc::new(FailingPort), Arc::new(FailingRefresh)).protocols();

        assert_eq!(
            protocols
                .iter()
                .map(|protocol| protocol.id.as_str())
                .collect::<Vec<_>>(),
            vec![SEARCH_PROTOCOL, REFRESH_PROTOCOL]
        );
    }

    #[test]
    fn a_search_and_a_refresh_run_in_separate_lanes() {
        assert_ne!(search_slot().as_str(), refresh_slot().as_str());
    }

    #[test]
    fn a_failing_refresh_fails_the_job_with_the_hosts_message() {
        let payload = serde_json::to_value(RefreshRequest { ticket: 4 }).unwrap();

        let error = run_refresh_job(payload, &FailingRefresh)
            .expect_err("a vanished workspace fails the refresh");

        assert_eq!(error, "the workspace is gone");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_search_job(serde_json::json!({ "query": 7 }), &FailingPort)
            .expect_err("a malformed request cannot be searched");

        assert!(error.contains("invalid code search request"), "{error}");
    }

    #[test]
    fn a_failing_port_fails_the_job_with_the_hosts_message() {
        let payload = serde_json::to_value(SearchRequest {
            scope: CodeSearchScope::AllProjects,
            query: "fn main".to_owned(),
            mode: CodeSearchMode::Text,
        })
        .unwrap();

        let error =
            run_search_job(payload, &FailingPort).expect_err("a failing index fails the job");

        assert_eq!(error, "no project has an index");
    }
}
