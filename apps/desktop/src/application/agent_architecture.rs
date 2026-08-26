//! Host-side architecture-change interaction for an active Agent turn.

use serde::{Deserialize, Serialize};
use serde_json::json;

use lilia_contracts::TaskId;

use lilia_contracts::ExecutionPermission as DesktopExecutionPermission;
use lilia_feature_agent_session::DesktopInteractionResponse;

use crate::application::architecture::{
    ArchitectureBackend, ArchitecturePermission, ProjectArchitectureApplyInput,
    ProjectArchitectureChange, ProjectArchitectureChangeEvent, ProjectArchitectureGraph,
    ProjectArchitectureRejectInput,
};
use crate::application::{DesktopApplication, DesktopApplicationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopArchitectureInteractionDecision {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopArchitectureInteractionResponse {
    pub decision: DesktopArchitectureInteractionDecision,
    pub graph: Option<ProjectArchitectureGraph>,
    pub event: ProjectArchitectureChangeEvent,
    pub message: String,
    pub interaction: DesktopInteractionResponse,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopArchitectureInteractionPayload {
    pub project_id: String,
    pub task_id: String,
    pub turn_id: Option<String>,
    pub backend: ArchitectureBackend,
    pub permission: ArchitecturePermission,
    pub reason: String,
    pub changes: Vec<ProjectArchitectureChange>,
    pub expected_version: Option<i64>,
}

fn architecture_permission(permission: DesktopExecutionPermission) -> ArchitecturePermission {
    match permission {
        DesktopExecutionPermission::Full => ArchitecturePermission::Full,
        DesktopExecutionPermission::Ask => ArchitecturePermission::Ask,
        DesktopExecutionPermission::Readonly => ArchitecturePermission::Readonly,
    }
}

impl DesktopApplication {
    pub fn respond_task_architecture_interaction(
        &self,
        task_id: &TaskId,
        request_id: &str,
        decision: DesktopArchitectureInteractionDecision,
    ) -> Result<DesktopArchitectureInteractionResponse, DesktopApplicationError> {
        let pending = self
            .task_session_snapshot(task_id)?
            .pending
            .into_iter()
            .find(|pending| {
                pending.request_id == request_id
                    && pending.kind == "architecture_change"
                    && pending.status == lilia_contracts::PendingProjectionStatus::Open
            })
            .ok_or_else(|| DesktopApplicationError::PendingInteractionNotFound {
                task_id: task_id.clone(),
                request_id: request_id.to_owned(),
            })?;
        let turn_id = pending.turn_id.clone().ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction is missing its turn id".to_owned(),
            }
        })?;
        let waiting = self
            .inner
            .agent
            .waiting_interaction(task_id)
            .ok_or_else(|| DesktopApplicationError::TurnNotWaitingInteraction {
                task_id: task_id.clone(),
                turn_id: turn_id.clone(),
            })?;
        if waiting.turn_id != turn_id
            || waiting.session_id.as_deref() != Some(pending.agent_session.as_str())
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction does not belong to the active turn".to_owned(),
            });
        }
        let request = self
            .inner
            .agent
            .request(task_id, &turn_id)
            .ok_or_else(|| DesktopApplicationError::NoActiveTurn(task_id.clone()))?;
        let payload: DesktopArchitectureInteractionPayload =
            serde_json::from_value(pending.payload.clone()).map_err(|error| {
                DesktopApplicationError::InvalidPendingInteraction {
                    request_id: request_id.to_owned(),
                    message: format!("invalid architecture payload: {error}"),
                }
            })?;
        let task = self.get_task(task_id)?;
        let project_id =
            task.project_id
                .ok_or_else(|| DesktopApplicationError::InvalidPendingInteraction {
                    request_id: request_id.to_owned(),
                    message: "architecture changes require a project task".to_owned(),
                })?;
        let permission = architecture_permission(request.permission);
        if payload.project_id != project_id.as_str()
            || payload.task_id != task_id.as_str()
            || payload.turn_id.as_deref() != Some(turn_id.as_str())
            || payload.backend != ArchitectureBackend::NativeAgentkit
            || payload.permission != permission
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture payload does not match the authoritative task, turn, backend or permission"
                    .to_owned(),
            });
        }
        let expected_version = payload.expected_version.ok_or_else(|| {
            DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "architecture interaction is missing its expected graph version"
                    .to_owned(),
            }
        })?;
        if decision == DesktopArchitectureInteractionDecision::Allow
            && permission == ArchitecturePermission::Readonly
        {
            return Err(DesktopApplicationError::InvalidPendingInteraction {
                request_id: request_id.to_owned(),
                message: "readonly turns cannot apply architecture changes".to_owned(),
            });
        }

        let (graph, event, message) = match decision {
            DesktopArchitectureInteractionDecision::Allow => {
                let result = self.apply_project_architecture(ProjectArchitectureApplyInput {
                    project_id: project_id.as_str().to_owned(),
                    task_id: task_id.as_str().to_owned(),
                    turn_id: Some(turn_id.clone()),
                    backend: ArchitectureBackend::NativeAgentkit,
                    permission,
                    reason: payload.reason,
                    changes: payload.changes,
                    request_id: Some(request_id.to_owned()),
                    expected_version: Some(expected_version),
                })?;
                (Some(result.graph), result.event, "架构图已更新".to_owned())
            }
            DesktopArchitectureInteractionDecision::Deny => {
                let event = self.reject_project_architecture(ProjectArchitectureRejectInput {
                    project_id: project_id.as_str().to_owned(),
                    task_id: task_id.as_str().to_owned(),
                    turn_id: Some(turn_id.clone()),
                    backend: ArchitectureBackend::NativeAgentkit,
                    permission,
                    reason: payload.reason,
                    changes: payload.changes,
                    request_id: Some(request_id.to_owned()),
                    expected_version: Some(expected_version),
                })?;
                (None, event, "架构图变更已拒绝".to_owned())
            }
        };
        let response = json!({
            "interaction": "architecture_change",
            "decision": decision,
            "graph": graph,
            "event": event,
            "message": message,
        });
        let interaction = self.respond_task_interaction(task_id, request_id, true, response)?;
        Ok(DesktopArchitectureInteractionResponse {
            decision,
            graph,
            event,
            message,
            interaction,
        })
    }
}
