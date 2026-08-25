//! Kernel job lane for the git commands behind a task's worktree.
//!
//! Creating, merging or removing a worktree shells out to git and can take
//! seconds on a large repository, so it ran on its own thread. It is a job now,
//! keyed per task: two tasks own two different directories and must not queue
//! behind each other, while a second operation on one task replaces the first.
//!
//! Completion still travels as a domain event, because archiving a task also
//! moves the surface off it and every open popup has to follow. The job's own
//! terminal state is the backstop for a run that died without reporting.

use std::path::PathBuf;
use std::sync::Arc;

use lilia_kernel::{JobContext, JobProtocol, JobSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OPERATE_PROTOCOL: &str = "lilia.worktree/operate@1";

/// What to do with a task's worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase")]
pub enum WorktreeOperationRequest {
    Create,
    Attach { path: PathBuf },
    Clear,
    CleanupAndArchive,
    MergeAndArchive,
}

/// Payload of [`OPERATE_PROTOCOL`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeRequest {
    pub task_id: String,
    #[serde(flatten)]
    pub operation: WorktreeOperationRequest,
}

/// Runs one worktree operation and announces the outcome as a domain event.
pub trait WorktreePort: Send + Sync + 'static {
    fn operate(&self, request: WorktreeRequest) -> Result<(), String>;
}

/// One lane per task.
pub fn worktree_slot(task_id: &str) -> JobSlot {
    JobSlot::new(format!("lilia.worktree.{task_id}"))
        .expect("the worktree slot name is not blank")
}

pub(crate) fn operate_protocol(port: Arc<dyn WorktreePort>) -> JobProtocol {
    JobProtocol::new(
        OPERATE_PROTOCOL,
        Arc::new(move |payload, _context: &JobContext| {
            run_operate_job(payload, port.as_ref())
        }),
    )
}

fn run_operate_job(payload: Value, port: &dyn WorktreePort) -> Result<Value, String> {
    let request: WorktreeRequest = serde_json::from_value(payload)
        .map_err(|error| format!("invalid worktree request: {error}"))?;
    port.operate(request)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingPort {
        requests: Mutex<Vec<WorktreeRequest>>,
        failure: Option<String>,
    }

    impl WorktreePort for RecordingPort {
        fn operate(&self, request: WorktreeRequest) -> Result<(), String> {
            self.requests.lock().unwrap().push(request);
            match &self.failure {
                Some(message) => Err(message.clone()),
                None => Ok(()),
            }
        }
    }

    fn request(operation: WorktreeOperationRequest) -> WorktreeRequest {
        WorktreeRequest {
            task_id: "task-1".to_owned(),
            operation,
        }
    }

    #[test]
    fn the_job_forwards_the_task_and_its_operation() {
        let port = RecordingPort::default();

        run_operate_job(
            serde_json::to_value(request(WorktreeOperationRequest::MergeAndArchive)).unwrap(),
            &port,
        )
        .unwrap();

        assert_eq!(
            port.requests.lock().unwrap().as_slice(),
            [request(WorktreeOperationRequest::MergeAndArchive)]
        );
    }

    #[test]
    fn every_operation_survives_the_payload_round_trip() {
        for operation in [
            WorktreeOperationRequest::Create,
            WorktreeOperationRequest::Attach {
                path: PathBuf::from("/repos/feature"),
            },
            WorktreeOperationRequest::Clear,
            WorktreeOperationRequest::CleanupAndArchive,
            WorktreeOperationRequest::MergeAndArchive,
        ] {
            let payload = serde_json::to_value(request(operation.clone())).unwrap();
            let restored: WorktreeRequest = serde_json::from_value(payload).unwrap();

            assert_eq!(restored, request(operation));
        }
    }

    #[test]
    fn a_failing_git_command_fails_the_job_with_its_message() {
        let port = RecordingPort {
            failure: Some("工作树目录已被占用".to_owned()),
            ..RecordingPort::default()
        };

        let error = run_operate_job(
            serde_json::to_value(request(WorktreeOperationRequest::Create)).unwrap(),
            &port,
        )
        .expect_err("a failing git command fails the job");

        assert_eq!(error, "工作树目录已被占用");
    }

    #[test]
    fn an_unreadable_payload_fails_the_job_instead_of_panicking() {
        let error = run_operate_job(
            serde_json::json!({ "taskId": "task-1", "operation": "explode" }),
            &RecordingPort::default(),
        )
        .expect_err("an unknown operation cannot run");

        assert!(error.contains("invalid worktree request"), "{error}");
    }

    #[test]
    fn two_tasks_operate_on_their_worktrees_independently() {
        assert_ne!(worktree_slot("task-1"), worktree_slot("task-2"));
    }
}
