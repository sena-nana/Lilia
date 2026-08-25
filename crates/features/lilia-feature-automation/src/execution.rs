use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use serde_json::Value as JsonValue;

use super::contract::{
    default_agent_prompt, default_human_prompt, default_logic_kind, default_logic_path,
    default_tool_action, default_tool_priority,
};
use super::template::{
    automation_json_path, automation_json_value_is_truthy, automation_json_value_to_port,
    automation_json_value_to_string,
};
use super::{
    automation_active_outgoing_edges, automation_initial_active_nodes,
    automation_topological_order, render_automation_template, validate_automation_graph,
    AutomationDraft, AutomationExecutionTransition, AutomationNode, AutomationNodeStateUpdate,
    AutomationResumeRunInput, AutomationRun, AutomationRunDetail, AutomationRunStateUpdate,
    AutomationRunStatus, AutomationSignalEnvelope, AutomationStoreError, AutomationWorkflowVersion,
    GraphExecution,
};

const NATIVE_AGENT_BACKEND: &str = "native-agentkit";

/// Short-lived persistence boundary used by the state machine. Implementations
/// must release their database transaction and lock before returning.
pub trait AutomationExecutionRepository: Send + Sync {
    fn execution_run_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<AutomationRunDetail>, AutomationStoreError>;

    fn execution_version(
        &self,
        version_id: &str,
    ) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError>;

    fn apply_execution_transition(
        &self,
        transition: AutomationExecutionTransition,
    ) -> Result<AutomationRunDetail, AutomationStoreError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// Stable key passed to every external side effect. Port implementations must
/// return the original result when the same key is retried after a crash.
pub struct AutomationIdempotencyKey {
    pub run_id: String,
    pub node_id: String,
}

impl fmt::Display for AutomationIdempotencyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.run_id, self.node_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationPortContext {
    pub idempotency_key: AutomationIdempotencyKey,
    pub workflow_id: String,
    pub workflow_version_id: String,
    pub trigger: AutomationSignalEnvelope,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationCreateTaskRequest {
    pub context: AutomationPortContext,
    pub project_id: Option<String>,
    pub title: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationUpdateTaskStatusRequest {
    pub context: AutomationPortContext,
    pub task_id: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationAddTodoRequest {
    pub context: AutomationPortContext,
    pub task_id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationSendGuideRequest {
    pub context: AutomationPortContext,
    pub task_id: String,
    pub text: String,
    pub priority: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationRecordTimelineRequest {
    pub context: AutomationPortContext,
    pub task_id: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub backend: Option<String>,
    pub payload: JsonValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutomationAgentTarget {
    ExistingTask {
        task_id: String,
    },
    CreateTask {
        project_id: Option<String>,
        title: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationStartAgentRequest {
    pub context: AutomationPortContext,
    pub target: AutomationAgentTarget,
    pub prompt: String,
    pub backend: String,
    pub model: String,
    pub permission: String,
    pub project_cwd: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutomationAgentDispatch {
    Waiting {
        task_id: String,
        turn_id: String,
        dispatch: String,
        queued_count: u64,
        worker_start_required: bool,
        turn_inserted: bool,
        message_id: Option<String>,
        metadata: JsonValue,
    },
    Completed {
        task_id: String,
        turn_id: String,
        metadata: JsonValue,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationAgentActivation {
    pub run_id: String,
    pub node_id: String,
    pub task_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct AutomationPortError {
    pub message: String,
}

impl AutomationPortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait AutomationTaskPort: Send + Sync {
    fn create_task(
        &self,
        request: AutomationCreateTaskRequest,
    ) -> Result<JsonValue, AutomationPortError>;

    fn update_task_status(
        &self,
        request: AutomationUpdateTaskStatusRequest,
    ) -> Result<JsonValue, AutomationPortError>;
}

pub trait AutomationTodoPort: Send + Sync {
    fn add_todo(&self, request: AutomationAddTodoRequest)
        -> Result<JsonValue, AutomationPortError>;
}

pub trait AutomationGuidePort: Send + Sync {
    fn send_guide(
        &self,
        request: AutomationSendGuideRequest,
    ) -> Result<JsonValue, AutomationPortError>;
}

pub trait AutomationTimelinePort: Send + Sync {
    fn record_timeline(
        &self,
        request: AutomationRecordTimelineRequest,
    ) -> Result<JsonValue, AutomationPortError>;
}

pub trait AutomationAgentPort: Send + Sync {
    fn start_agent(
        &self,
        request: AutomationStartAgentRequest,
    ) -> Result<AutomationAgentDispatch, AutomationPortError>;

    fn activate_agent(
        &self,
        _activation: AutomationAgentActivation,
    ) -> Result<(), AutomationPortError> {
        Ok(())
    }

    fn abort_agent(
        &self,
        _activation: AutomationAgentActivation,
    ) -> Result<(), AutomationPortError> {
        Ok(())
    }
}

/// Host-owned side effects. Every method must deduplicate requests by the
/// idempotency key carried in its request context.
pub trait AutomationExecutionPorts:
    AutomationTaskPort
    + AutomationTodoPort
    + AutomationGuidePort
    + AutomationTimelinePort
    + AutomationAgentPort
{
}

impl<T> AutomationExecutionPorts for T where
    T: AutomationTaskPort
        + AutomationTodoPort
        + AutomationGuidePort
        + AutomationTimelinePort
        + AutomationAgentPort
{
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationCompleteAgentInput {
    pub run_id: String,
    pub node_id: Option<String>,
    pub turn_id: String,
    pub success: bool,
    pub payload: Option<JsonValue>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationExecutionResult {
    pub detail: AutomationRunDetail,
    pub execution: GraphExecution,
}

#[derive(Clone)]
pub struct AutomationExecutionEngine {
    ports: Arc<dyn AutomationExecutionPorts>,
}

impl AutomationExecutionEngine {
    pub fn new<P>(ports: Arc<P>) -> Self
    where
        P: AutomationExecutionPorts + 'static,
    {
        Self { ports }
    }

    pub fn from_ports(ports: Arc<dyn AutomationExecutionPorts>) -> Self {
        Self { ports }
    }

    pub fn execute_run(
        &self,
        repository: &dyn AutomationExecutionRepository,
        run_id: &str,
    ) -> Result<AutomationExecutionResult, AutomationExecutionError> {
        let detail = required_run(repository, run_id)?;
        require_run_status(&detail.run, &[AutomationRunStatus::Running])?;
        let version = required_version(repository, &detail.run)?;
        self.advance(repository, detail, version.snapshot, None)
    }

    pub fn resume_human(
        &self,
        repository: &dyn AutomationExecutionRepository,
        run_id: &str,
        input: AutomationResumeRunInput,
    ) -> Result<AutomationExecutionResult, AutomationExecutionError> {
        let detail = required_run(repository, run_id)?;
        require_run_status(&detail.run, &[AutomationRunStatus::WaitingUser])?;
        let version = required_version(repository, &detail.run)?;
        let waiting = select_waiting_node(
            &detail,
            input.node_id.as_deref(),
            AutomationRunStatus::WaitingUser,
        )?;
        require_snapshot_node_kind(&version.snapshot, &waiting.node_id, "human")?;
        let user = input
            .payload
            .unwrap_or_else(|| serde_json::json!({ "confirmed": true }));
        let output = merge_json_objects(
            waiting.output.clone().unwrap_or_else(empty_object),
            serde_json::json!({
                "waitingUser": false,
                "confirmed": true,
                "user": user,
                "selectedHandle": "success",
            }),
        );
        let detail = repository.apply_execution_transition(AutomationExecutionTransition {
            run_id: detail.run.id.clone(),
            run: active_run_update(
                vec![AutomationRunStatus::WaitingUser],
                AutomationRunStatus::Running,
            ),
            nodes: vec![completed_node_update(
                &waiting.node_id,
                vec![AutomationRunStatus::WaitingUser],
                AutomationRunStatus::Succeeded,
                waiting.input.clone(),
                Some(output),
                None,
            )],
        })?;
        self.advance(
            repository,
            detail,
            version.snapshot,
            Some(waiting.node_id.clone()),
        )
    }

    pub fn complete_agent(
        &self,
        repository: &dyn AutomationExecutionRepository,
        input: AutomationCompleteAgentInput,
    ) -> Result<AutomationExecutionResult, AutomationExecutionError> {
        let detail = required_run(repository, &input.run_id)?;
        require_run_status(&detail.run, &[AutomationRunStatus::Running])?;
        let version = required_version(repository, &detail.run)?;
        let waiting = select_waiting_agent_node(&detail, input.node_id.as_deref(), &input.turn_id)?;
        require_snapshot_node_kind(&version.snapshot, &waiting.node_id, "agent")?;
        let output = merge_json_objects(
            waiting.output.clone().unwrap_or_else(empty_object),
            serde_json::json!({
                "waitingAgent": false,
                "completed": input.success,
                "selectedHandle": if input.success { "success" } else { "error" },
                "result": input.payload,
            }),
        );
        let node_status = if input.success {
            AutomationRunStatus::Succeeded
        } else {
            AutomationRunStatus::Failed
        };
        let run_status = if input.success {
            AutomationRunStatus::Running
        } else {
            AutomationRunStatus::Failed
        };
        let run_error = (!input.success).then(|| {
            input
                .error
                .clone()
                .unwrap_or_else(|| "Agent node failed or was interrupted".to_owned())
        });
        let detail = repository.apply_execution_transition(AutomationExecutionTransition {
            run_id: detail.run.id.clone(),
            run: AutomationRunStateUpdate {
                expected_statuses: vec![AutomationRunStatus::Running],
                status: run_status,
                error: run_error.clone(),
                finished: !input.success,
            },
            nodes: vec![completed_node_update(
                &waiting.node_id,
                vec![AutomationRunStatus::Running],
                node_status,
                waiting.input.clone(),
                Some(output),
                run_error,
            )],
        })?;
        if !input.success {
            return Ok(AutomationExecutionResult {
                detail,
                execution: GraphExecution::Failed,
            });
        }
        self.advance(
            repository,
            detail,
            version.snapshot,
            Some(waiting.node_id.clone()),
        )
    }

    pub fn cancel_run(
        &self,
        repository: &dyn AutomationExecutionRepository,
        run_id: &str,
    ) -> Result<AutomationRunDetail, AutomationExecutionError> {
        let detail = required_run(repository, run_id)?;
        require_run_status(
            &detail.run,
            &[
                AutomationRunStatus::Running,
                AutomationRunStatus::WaitingUser,
            ],
        )?;
        let active_statuses = vec![
            AutomationRunStatus::Pending,
            AutomationRunStatus::Running,
            AutomationRunStatus::WaitingUser,
        ];
        let nodes = detail
            .nodes
            .iter()
            .filter(|node| node.status.is_active())
            .map(|node| AutomationNodeStateUpdate {
                node_id: node.node_id.clone(),
                expected_statuses: active_statuses.clone(),
                status: AutomationRunStatus::Cancelled,
                input: node.input.clone(),
                output: node.output.clone(),
                error: None,
                mark_started: false,
                finished: true,
            })
            .collect();
        repository
            .apply_execution_transition(AutomationExecutionTransition {
                run_id: detail.run.id,
                run: AutomationRunStateUpdate {
                    expected_statuses: vec![
                        AutomationRunStatus::Running,
                        AutomationRunStatus::WaitingUser,
                    ],
                    status: AutomationRunStatus::Cancelled,
                    error: None,
                    finished: true,
                },
                nodes,
            })
            .map_err(Into::into)
    }

    fn advance(
        &self,
        repository: &dyn AutomationExecutionRepository,
        mut detail: AutomationRunDetail,
        draft: AutomationDraft,
        resume_from_node: Option<String>,
    ) -> Result<AutomationExecutionResult, AutomationExecutionError> {
        validate_automation_graph(&draft.nodes, &draft.edges)?;
        let ordered = automation_topological_order(&draft.nodes, &draft.edges)?;
        let node_map = draft
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut outputs = outputs_from_detail(&detail);
        let mut active_nodes = resume_from_node
            .as_deref()
            .and_then(|node_id| node_map.get(node_id))
            .map(|node| {
                let output = outputs
                    .get(&node.id)
                    .and_then(|value| value.get("output"))
                    .unwrap_or(&JsonValue::Null);
                automation_active_outgoing_edges(&draft.edges, &node.id, output)
                    .into_iter()
                    .map(|edge| edge.target.clone())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_else(|| automation_initial_active_nodes(&draft.nodes, &draft.edges));

        for node_id in ordered {
            let Some(node) = node_map.get(&node_id) else {
                continue;
            };
            if !active_nodes.contains(&node.id) {
                continue;
            }
            if let Some(existing) = outputs.get(&node.id) {
                let output = existing.get("output").unwrap_or(&JsonValue::Null);
                for edge in automation_active_outgoing_edges(&draft.edges, &node.id, output) {
                    active_nodes.insert(edge.target.clone());
                }
                continue;
            }
            let state = run_node(&detail, &node.id)?;
            if state.status == AutomationRunStatus::WaitingUser {
                return Ok(AutomationExecutionResult {
                    detail,
                    execution: GraphExecution::WaitingUser,
                });
            }
            if state.status == AutomationRunStatus::Running
                && state
                    .output
                    .as_ref()
                    .and_then(|output| output.get("waitingAgent"))
                    .and_then(JsonValue::as_bool)
                    == Some(true)
            {
                return Ok(AutomationExecutionResult {
                    detail,
                    execution: GraphExecution::WaitingAgent,
                });
            }
            if !matches!(
                state.status,
                AutomationRunStatus::Pending | AutomationRunStatus::Running
            ) {
                return Err(AutomationExecutionError::InvalidNodeState {
                    run_id: detail.run.id.clone(),
                    node_id: node.id.clone(),
                    status: state.status,
                });
            }
            let input = serde_json::json!({
                "trigger": detail.run.trigger,
                "nodes": outputs,
                "config": node.config,
            });
            detail = repository.apply_execution_transition(AutomationExecutionTransition {
                run_id: detail.run.id.clone(),
                run: active_run_update(
                    vec![AutomationRunStatus::Running],
                    AutomationRunStatus::Running,
                ),
                nodes: vec![AutomationNodeStateUpdate {
                    node_id: node.id.clone(),
                    expected_statuses: vec![
                        AutomationRunStatus::Pending,
                        AutomationRunStatus::Running,
                    ],
                    status: AutomationRunStatus::Running,
                    input: input.clone(),
                    output: None,
                    error: None,
                    mark_started: true,
                    finished: false,
                }],
            })?;

            let output = match self.execute_node(&detail.run, node, &input) {
                Ok(output) => output,
                Err(message) => {
                    let detail =
                        repository.apply_execution_transition(AutomationExecutionTransition {
                            run_id: detail.run.id.clone(),
                            run: AutomationRunStateUpdate {
                                expected_statuses: vec![AutomationRunStatus::Running],
                                status: AutomationRunStatus::Failed,
                                error: Some(message.clone()),
                                finished: true,
                            },
                            nodes: vec![completed_node_update(
                                &node.id,
                                vec![AutomationRunStatus::Running],
                                AutomationRunStatus::Failed,
                                input,
                                None,
                                Some(message),
                            )],
                        })?;
                    return Ok(AutomationExecutionResult {
                        detail,
                        execution: GraphExecution::Failed,
                    });
                }
            };
            if node.kind == "human"
                && output.get("waitingUser").and_then(JsonValue::as_bool) == Some(true)
            {
                let detail =
                    repository.apply_execution_transition(AutomationExecutionTransition {
                        run_id: detail.run.id.clone(),
                        run: active_run_update(
                            vec![AutomationRunStatus::Running],
                            AutomationRunStatus::WaitingUser,
                        ),
                        nodes: vec![AutomationNodeStateUpdate {
                            node_id: node.id.clone(),
                            expected_statuses: vec![AutomationRunStatus::Running],
                            status: AutomationRunStatus::WaitingUser,
                            input,
                            output: Some(output),
                            error: None,
                            mark_started: false,
                            finished: false,
                        }],
                    })?;
                return Ok(AutomationExecutionResult {
                    detail,
                    execution: GraphExecution::WaitingUser,
                });
            }
            if node.kind == "agent"
                && output.get("waitingAgent").and_then(JsonValue::as_bool) == Some(true)
            {
                let activation = AutomationAgentActivation {
                    run_id: detail.run.id.clone(),
                    node_id: node.id.clone(),
                    task_id: output
                        .get("taskId")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    turn_id: output
                        .get("turnId")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                };
                let worker_start_required = output
                    .get("workerStartRequired")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false);
                let turn_inserted = output
                    .get("turnInserted")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false);
                let transition =
                    repository.apply_execution_transition(AutomationExecutionTransition {
                        run_id: detail.run.id.clone(),
                        run: active_run_update(
                            vec![AutomationRunStatus::Running],
                            AutomationRunStatus::Running,
                        ),
                        nodes: vec![AutomationNodeStateUpdate {
                            node_id: node.id.clone(),
                            expected_statuses: vec![AutomationRunStatus::Running],
                            status: AutomationRunStatus::Running,
                            input,
                            output: Some(output),
                            error: None,
                            mark_started: false,
                            finished: false,
                        }],
                    });
                let detail = match transition {
                    Ok(detail) => detail,
                    Err(error) => {
                        if turn_inserted {
                            let _ = self.ports.abort_agent(activation);
                        }
                        return Err(error.into());
                    }
                };
                if worker_start_required {
                    if let Err(error) = self.ports.activate_agent(activation) {
                        let message = error.to_string();
                        let failed =
                            repository.apply_execution_transition(AutomationExecutionTransition {
                                run_id: detail.run.id.clone(),
                                run: AutomationRunStateUpdate {
                                    expected_statuses: vec![AutomationRunStatus::Running],
                                    status: AutomationRunStatus::Failed,
                                    error: Some(message.clone()),
                                    finished: true,
                                },
                                nodes: vec![completed_node_update(
                                    &node.id,
                                    vec![AutomationRunStatus::Running],
                                    AutomationRunStatus::Failed,
                                    run_node(&detail, &node.id)?.input.clone(),
                                    run_node(&detail, &node.id)?.output.clone(),
                                    Some(message),
                                )],
                            });
                        let detail = match failed {
                            Ok(detail) => detail,
                            Err(_) => required_run(repository, &detail.run.id)?,
                        };
                        return Ok(AutomationExecutionResult {
                            detail,
                            execution: GraphExecution::Failed,
                        });
                    }
                }
                return Ok(AutomationExecutionResult {
                    detail,
                    execution: GraphExecution::WaitingAgent,
                });
            }
            let node_status = if output.get("skipped").and_then(JsonValue::as_bool) == Some(true) {
                AutomationRunStatus::Skipped
            } else {
                AutomationRunStatus::Succeeded
            };
            detail = repository.apply_execution_transition(AutomationExecutionTransition {
                run_id: detail.run.id.clone(),
                run: active_run_update(
                    vec![AutomationRunStatus::Running],
                    AutomationRunStatus::Running,
                ),
                nodes: vec![completed_node_update(
                    &node.id,
                    vec![AutomationRunStatus::Running],
                    node_status,
                    input,
                    Some(output.clone()),
                    None,
                )],
            })?;
            outputs.insert(
                node.id.clone(),
                serde_json::json!({
                    "status": node_status.as_str(),
                    "output": output,
                }),
            );
            if outputs
                .get(&node.id)
                .and_then(|value| value.get("output"))
                .and_then(|value| value.get("stopped"))
                .and_then(JsonValue::as_bool)
                == Some(true)
            {
                return finish_success(repository, detail, &draft, &outputs);
            }
            let output = outputs
                .get(&node.id)
                .and_then(|value| value.get("output"))
                .unwrap_or(&JsonValue::Null);
            for edge in automation_active_outgoing_edges(&draft.edges, &node.id, output) {
                active_nodes.insert(edge.target.clone());
            }
        }
        finish_success(repository, detail, &draft, &outputs)
    }

    fn execute_node(
        &self,
        run: &AutomationRun,
        node: &AutomationNode,
        input: &JsonValue,
    ) -> Result<JsonValue, String> {
        match node.kind.as_str() {
            "trigger" => Ok(serde_json::json!({ "triggered": true })),
            "logic" => execute_logic_node(node, input),
            "human" => Ok(serde_json::json!({
                "waitingUser": true,
                "prompt": render_automation_template(
                    node.config
                        .get("prompt")
                        .and_then(JsonValue::as_str)
                        .unwrap_or(default_human_prompt()),
                    input,
                ),
            })),
            "tool" => self.execute_tool_node(run, node, input),
            "agent" => self.execute_agent_node(run, node, input),
            other => Err(format!("unknown automation node kind: {other}")),
        }
    }

    fn execute_tool_node(
        &self,
        run: &AutomationRun,
        node: &AutomationNode,
        input: &JsonValue,
    ) -> Result<JsonValue, String> {
        let context = port_context(run, node);
        let action = node
            .config
            .get("action")
            .and_then(JsonValue::as_str)
            .unwrap_or(default_tool_action());
        match action {
            "create_task" => self
                .ports
                .create_task(AutomationCreateTaskRequest {
                    context,
                    project_id: rendered(node.config.get("projectId"), input)
                        .or_else(|| run.trigger.project_id.clone()),
                    title: render_config(node, "title", "自动化任务", input),
                    status: render_config(node, "status", "waiting", input),
                })
                .map_err(|error| error.to_string()),
            "update_task_status" => self
                .ports
                .update_task_status(AutomationUpdateTaskStatusRequest {
                    context,
                    task_id: rendered(node.config.get("taskId"), input)
                        .or_else(|| run.trigger.task_id.clone())
                        .ok_or_else(|| "update_task_status requires taskId".to_owned())?,
                    status: render_config(node, "status", "waiting", input),
                })
                .map_err(|error| error.to_string()),
            "add_todo" => self
                .ports
                .add_todo(AutomationAddTodoRequest {
                    context,
                    task_id: rendered(node.config.get("taskId"), input)
                        .or_else(|| run.trigger.task_id.clone())
                        .ok_or_else(|| "add_todo requires taskId".to_owned())?,
                    text: render_config(node, "text", "自动化 Todo", input),
                })
                .map_err(|error| error.to_string()),
            "send_guide" => self
                .ports
                .send_guide(AutomationSendGuideRequest {
                    context,
                    task_id: rendered(node.config.get("taskId"), input)
                        .or_else(|| run.trigger.task_id.clone())
                        .ok_or_else(|| "send_guide requires taskId".to_owned())?,
                    text: node
                        .config
                        .get("text")
                        .or_else(|| node.config.get("title"))
                        .and_then(JsonValue::as_str)
                        .map(|value| render_automation_template(value, input))
                        .unwrap_or_else(|| "自动化引导".to_owned()),
                    priority: normalized_priority(
                        node.config
                            .get("priority")
                            .and_then(JsonValue::as_str)
                            .unwrap_or(default_tool_priority()),
                    ),
                })
                .map_err(|error| error.to_string()),
            "record_timeline" => {
                let task_id = rendered(node.config.get("taskId"), input)
                    .or_else(|| run.trigger.task_id.clone())
                    .ok_or_else(|| "record_timeline requires taskId".to_owned())?;
                let title = render_config(node, "title", "自动化记录", input);
                self.ports
                    .record_timeline(AutomationRecordTimelineRequest {
                        context,
                        task_id,
                        summary: rendered(node.config.get("summary"), input)
                            .unwrap_or_else(|| title.clone()),
                        title,
                        status: render_config(node, "status", "info", input),
                        backend: rendered(node.config.get("backend"), input)
                            .or_else(|| run.trigger.backend.clone()),
                        payload: serde_json::json!({
                            "automationRunId": run.id,
                            "workflowId": run.workflow_id,
                            "workflowVersionId": run.workflow_version_id,
                            "nodeId": node.id,
                        }),
                    })
                    .map_err(|error| error.to_string())
            }
            other => Err(format!("unknown automation tool action: {other}")),
        }
    }

    fn execute_agent_node(
        &self,
        run: &AutomationRun,
        node: &AutomationNode,
        input: &JsonValue,
    ) -> Result<JsonValue, String> {
        let configured_backend = node
            .config
            .get("backend")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if configured_backend.is_some_and(|backend| {
            matches!(
                backend,
                "claude" | "codex" | "node" | "node-agent-runner" | "agent-runner"
            )
        }) {
            return Err(format!(
                "automation agent backend must be {NATIVE_AGENT_BACKEND}"
            ));
        }
        let target = if node
            .config
            .get("createTask")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            AutomationAgentTarget::CreateTask {
                project_id: rendered(node.config.get("projectId"), input)
                    .or_else(|| run.trigger.project_id.clone()),
                title: render_config(node, "title", "自动化 Agent 任务", input),
            }
        } else {
            AutomationAgentTarget::ExistingTask {
                task_id: rendered(node.config.get("taskId"), input)
                    .or_else(|| run.trigger.task_id.clone())
                    .ok_or_else(|| "agent node requires taskId".to_owned())?,
            }
        };
        let dispatch = self
            .ports
            .start_agent(AutomationStartAgentRequest {
                context: port_context(run, node),
                target,
                prompt: render_config(node, "prompt", default_agent_prompt(), input),
                backend: NATIVE_AGENT_BACKEND.to_owned(),
                model: node
                    .config
                    .get("model")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("native-default")
                    .to_owned(),
                permission: node
                    .config
                    .get("permission")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("")
                    .to_owned(),
                project_cwd: node
                    .config
                    .get("projectCwd")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
            .map_err(|error| error.to_string())?;
        match dispatch {
            AutomationAgentDispatch::Waiting {
                task_id,
                turn_id,
                dispatch,
                queued_count,
                worker_start_required,
                turn_inserted,
                message_id,
                metadata,
            } => {
                require_agent_dispatch_ids(&task_id, &turn_id)?;
                Ok(serde_json::json!({
                    "waitingAgent": true,
                    "taskId": task_id,
                    "turnId": turn_id,
                    "dispatch": dispatch,
                    "queuedCount": queued_count,
                    "workerStartRequired": worker_start_required,
                    "turnInserted": turn_inserted,
                    "messageId": message_id,
                    "executionBackend": NATIVE_AGENT_BACKEND,
                    "metadata": metadata,
                }))
            }
            AutomationAgentDispatch::Completed {
                task_id,
                turn_id,
                metadata,
            } => {
                require_agent_dispatch_ids(&task_id, &turn_id)?;
                Ok(serde_json::json!({
                    "waitingAgent": false,
                    "completed": true,
                    "selectedHandle": "success",
                    "taskId": task_id,
                    "turnId": turn_id,
                    "executionBackend": NATIVE_AGENT_BACKEND,
                    "terminalReplay": true,
                    "metadata": metadata,
                }))
            }
        }
    }
}

fn require_agent_dispatch_ids(task_id: &str, turn_id: &str) -> Result<(), String> {
    if task_id.trim().is_empty() || turn_id.trim().is_empty() {
        Err("automation agent port returned an empty task or turn id".to_owned())
    } else {
        Ok(())
    }
}

fn execute_logic_node(node: &AutomationNode, input: &JsonValue) -> Result<JsonValue, String> {
    let logic = node
        .config
        .get("logic")
        .and_then(JsonValue::as_str)
        .unwrap_or(default_logic_kind());
    match logic {
        "stop" => Ok(serde_json::json!({ "stopped": true })),
        "condition" => {
            let path = node
                .config
                .get("path")
                .and_then(JsonValue::as_str)
                .unwrap_or(default_logic_path());
            let actual = automation_json_path(input, path);
            let passed = match node.config.get("equals").and_then(JsonValue::as_str) {
                Some(expected) => {
                    actual.is_some_and(|value| automation_json_value_to_string(value) == expected)
                }
                None => actual.is_some_and(automation_json_value_is_truthy),
            };
            Ok(serde_json::json!({
                "passed": passed,
                "selectedHandle": if passed { "true" } else { "false" },
            }))
        }
        "switch" => {
            let path = node
                .config
                .get("path")
                .and_then(JsonValue::as_str)
                .unwrap_or(default_logic_path());
            let value = automation_json_path(input, path)
                .cloned()
                .unwrap_or(JsonValue::Null);
            Ok(serde_json::json!({
                "value": value,
                "selectedHandle": automation_json_value_to_port(&value),
                "routeKind": "switch",
            }))
        }
        other => Err(format!("unknown automation logic kind: {other}")),
    }
}

fn required_run(
    repository: &dyn AutomationExecutionRepository,
    run_id: &str,
) -> Result<AutomationRunDetail, AutomationExecutionError> {
    repository
        .execution_run_detail(run_id)?
        .ok_or_else(|| AutomationExecutionError::RunNotFound {
            run_id: run_id.to_owned(),
        })
}

fn required_version(
    repository: &dyn AutomationExecutionRepository,
    run: &AutomationRun,
) -> Result<AutomationWorkflowVersion, AutomationExecutionError> {
    let version = repository
        .execution_version(&run.workflow_version_id)?
        .ok_or_else(|| AutomationExecutionError::VersionNotFound {
            version_id: run.workflow_version_id.clone(),
        })?;
    if version.workflow_id != run.workflow_id {
        return Err(AutomationExecutionError::VersionWorkflowMismatch {
            version_id: version.id,
            expected_workflow_id: run.workflow_id.clone(),
            actual_workflow_id: version.workflow_id,
        });
    }
    validate_automation_graph(&version.snapshot.nodes, &version.snapshot.edges)?;
    Ok(version)
}

fn require_run_status(
    run: &AutomationRun,
    expected: &[AutomationRunStatus],
) -> Result<(), AutomationExecutionError> {
    if expected.contains(&run.status) {
        Ok(())
    } else {
        Err(AutomationExecutionError::InvalidRunState {
            run_id: run.id.clone(),
            expected: expected.to_vec(),
            actual: run.status,
        })
    }
}

fn require_snapshot_node_kind<'a>(
    draft: &'a AutomationDraft,
    node_id: &str,
    expected_kind: &'static str,
) -> Result<&'a AutomationNode, AutomationExecutionError> {
    let node = draft
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| AutomationExecutionError::SnapshotNodeNotFound {
            node_id: node_id.to_owned(),
        })?;
    if node.kind != expected_kind {
        return Err(AutomationExecutionError::SnapshotNodeKindMismatch {
            node_id: node_id.to_owned(),
            expected: expected_kind,
            actual: node.kind.clone(),
        });
    }
    Ok(node)
}

fn select_waiting_node<'a>(
    detail: &'a AutomationRunDetail,
    node_id: Option<&str>,
    status: AutomationRunStatus,
) -> Result<&'a super::AutomationRunNodeState, AutomationExecutionError> {
    let matches = detail
        .nodes
        .iter()
        .filter(|state| state.status == status && node_id.is_none_or(|id| state.node_id == id))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [state] => Ok(*state),
        [] => Err(AutomationExecutionError::WaitingNodeNotFound {
            run_id: detail.run.id.clone(),
            node_id: node_id.map(str::to_owned),
        }),
        _ => Err(AutomationExecutionError::AmbiguousWaitingNode {
            run_id: detail.run.id.clone(),
        }),
    }
}

fn select_waiting_agent_node<'a>(
    detail: &'a AutomationRunDetail,
    node_id: Option<&str>,
    turn_id: &str,
) -> Result<&'a super::AutomationRunNodeState, AutomationExecutionError> {
    let matches = detail
        .nodes
        .iter()
        .filter(|state| {
            state.status == AutomationRunStatus::Running
                && node_id.is_none_or(|id| state.node_id == id)
                && state
                    .output
                    .as_ref()
                    .and_then(|output| output.get("waitingAgent"))
                    .and_then(JsonValue::as_bool)
                    == Some(true)
                && state
                    .output
                    .as_ref()
                    .and_then(|output| output.get("turnId"))
                    .and_then(JsonValue::as_str)
                    == Some(turn_id)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [state] => Ok(*state),
        [] => Err(AutomationExecutionError::WaitingAgentNodeNotFound {
            run_id: detail.run.id.clone(),
            node_id: node_id.map(str::to_owned),
            turn_id: turn_id.to_owned(),
        }),
        _ => Err(AutomationExecutionError::AmbiguousWaitingNode {
            run_id: detail.run.id.clone(),
        }),
    }
}

fn run_node<'a>(
    detail: &'a AutomationRunDetail,
    node_id: &str,
) -> Result<&'a super::AutomationRunNodeState, AutomationExecutionError> {
    detail
        .nodes
        .iter()
        .find(|state| state.node_id == node_id)
        .ok_or_else(|| AutomationExecutionError::RunNodeNotFound {
            run_id: detail.run.id.clone(),
            node_id: node_id.to_owned(),
        })
}

fn outputs_from_detail(detail: &AutomationRunDetail) -> BTreeMap<String, JsonValue> {
    detail
        .nodes
        .iter()
        .filter_map(|state| {
            matches!(
                state.status,
                AutomationRunStatus::Succeeded | AutomationRunStatus::Skipped
            )
            .then(|| {
                state.output.clone().map(|output| {
                    (
                        state.node_id.clone(),
                        serde_json::json!({
                            "status": state.status.as_str(),
                            "output": output,
                        }),
                    )
                })
            })
            .flatten()
        })
        .collect()
}

fn finish_success(
    repository: &dyn AutomationExecutionRepository,
    detail: AutomationRunDetail,
    draft: &AutomationDraft,
    outputs: &BTreeMap<String, JsonValue>,
) -> Result<AutomationExecutionResult, AutomationExecutionError> {
    let skipped_input = serde_json::json!({
        "trigger": detail.run.trigger,
        "nodes": outputs,
    });
    let mut updates = Vec::new();
    for node in &draft.nodes {
        let state = run_node(&detail, &node.id)?;
        match state.status {
            AutomationRunStatus::Pending => {
                updates.push(completed_node_update(
                    &node.id,
                    vec![AutomationRunStatus::Pending],
                    AutomationRunStatus::Skipped,
                    skipped_input.clone(),
                    Some(serde_json::json!({
                        "skipped": true,
                        "reason": "branch_not_selected",
                    })),
                    None,
                ));
            }
            AutomationRunStatus::Succeeded | AutomationRunStatus::Skipped => {}
            status => {
                return Err(AutomationExecutionError::InvalidNodeState {
                    run_id: detail.run.id.clone(),
                    node_id: node.id.clone(),
                    status,
                });
            }
        }
    }
    let detail = repository.apply_execution_transition(AutomationExecutionTransition {
        run_id: detail.run.id.clone(),
        run: AutomationRunStateUpdate {
            expected_statuses: vec![AutomationRunStatus::Running],
            status: AutomationRunStatus::Succeeded,
            error: None,
            finished: true,
        },
        nodes: updates,
    })?;
    Ok(AutomationExecutionResult {
        detail,
        execution: GraphExecution::Finished,
    })
}

fn port_context(run: &AutomationRun, node: &AutomationNode) -> AutomationPortContext {
    AutomationPortContext {
        idempotency_key: AutomationIdempotencyKey {
            run_id: run.id.clone(),
            node_id: node.id.clone(),
        },
        workflow_id: run.workflow_id.clone(),
        workflow_version_id: run.workflow_version_id.clone(),
        trigger: run.trigger.clone(),
    }
}

fn render_config(node: &AutomationNode, field: &str, fallback: &str, input: &JsonValue) -> String {
    render_automation_template(
        node.config
            .get(field)
            .and_then(JsonValue::as_str)
            .unwrap_or(fallback),
        input,
    )
}

fn rendered(value: Option<&JsonValue>, input: &JsonValue) -> Option<String> {
    value
        .and_then(JsonValue::as_str)
        .map(|value| render_automation_template(value, input))
        .filter(|value| !value.trim().is_empty())
}

fn normalized_priority(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "low" | "normal" | "high") {
        value
    } else {
        default_tool_priority().to_owned()
    }
}

fn active_run_update(
    expected_statuses: Vec<AutomationRunStatus>,
    status: AutomationRunStatus,
) -> AutomationRunStateUpdate {
    AutomationRunStateUpdate {
        expected_statuses,
        status,
        error: None,
        finished: false,
    }
}

fn completed_node_update(
    node_id: &str,
    expected_statuses: Vec<AutomationRunStatus>,
    status: AutomationRunStatus,
    input: JsonValue,
    output: Option<JsonValue>,
    error: Option<String>,
) -> AutomationNodeStateUpdate {
    AutomationNodeStateUpdate {
        node_id: node_id.to_owned(),
        expected_statuses,
        status,
        input,
        output,
        error,
        mark_started: false,
        finished: true,
    }
}

fn merge_json_objects(base: JsonValue, patch: JsonValue) -> JsonValue {
    let mut output = base.as_object().cloned().unwrap_or_default();
    if let Some(patch) = patch.as_object() {
        output.extend(patch.clone());
    }
    JsonValue::Object(output)
}

fn empty_object() -> JsonValue {
    serde_json::json!({})
}

#[derive(Debug, thiserror::Error)]
pub enum AutomationExecutionError {
    #[error(transparent)]
    Store(#[from] AutomationStoreError),
    #[error(transparent)]
    Graph(#[from] super::AutomationGraphError),
    #[error("automation run does not exist: {run_id}")]
    RunNotFound { run_id: String },
    #[error("automation workflow version does not exist: {version_id}")]
    VersionNotFound { version_id: String },
    #[error(
        "automation version {version_id} belongs to {actual_workflow_id}, not {expected_workflow_id}"
    )]
    VersionWorkflowMismatch {
        version_id: String,
        expected_workflow_id: String,
        actual_workflow_id: String,
    },
    #[error("automation run {run_id} is {actual:?}; expected one of {expected:?}")]
    InvalidRunState {
        run_id: String,
        expected: Vec<AutomationRunStatus>,
        actual: AutomationRunStatus,
    },
    #[error("automation run node does not exist: {run_id}/{node_id}")]
    RunNodeNotFound { run_id: String, node_id: String },
    #[error("automation node {node_id} is missing from the published snapshot")]
    SnapshotNodeNotFound { node_id: String },
    #[error("automation node {node_id} must be {expected}, found {actual}")]
    SnapshotNodeKindMismatch {
        node_id: String,
        expected: &'static str,
        actual: String,
    },
    #[error("automation run {run_id} has no matching waiting node {node_id:?}")]
    WaitingNodeNotFound {
        run_id: String,
        node_id: Option<String>,
    },
    #[error(
        "automation run {run_id} has no matching waiting agent node {node_id:?} for turn {turn_id}"
    )]
    WaitingAgentNodeNotFound {
        run_id: String,
        node_id: Option<String>,
        turn_id: String,
    },
    #[error("automation run {run_id} has multiple matching waiting nodes")]
    AmbiguousWaitingNode { run_id: String },
    #[error("automation run node {run_id}/{node_id} has invalid execution status {status:?}")]
    InvalidNodeState {
        run_id: String,
        node_id: String,
        status: AutomationRunStatus,
    },
    #[error("could not cancel Automation Agent turn for {run_id}/{node_id}: {message}")]
    AgentCancellation {
        run_id: String,
        node_id: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        AutomationBeginRunInput, AutomationEdge, AutomationNodePosition, AutomationSaveDraftInput,
        AutomationScopeFilter, DesktopAutomationService, SilentAutomationEvents,
    };

    #[derive(Default)]
    struct TestPorts {
        repository: Mutex<Option<DesktopAutomationService>>,
        tool_keys: Mutex<Vec<AutomationIdempotencyKey>>,
        agent_keys: Mutex<Vec<AutomationIdempotencyKey>>,
        agent_activations: Mutex<Vec<AutomationAgentActivation>>,
        tool_output: Mutex<Option<JsonValue>>,
        fail_tool: Mutex<bool>,
    }

    struct StaticRepository {
        detail: AutomationRunDetail,
        version: AutomationWorkflowVersion,
    }

    impl AutomationExecutionRepository for StaticRepository {
        fn execution_run_detail(
            &self,
            run_id: &str,
        ) -> Result<Option<AutomationRunDetail>, AutomationStoreError> {
            Ok((self.detail.run.id == run_id).then(|| self.detail.clone()))
        }

        fn execution_version(
            &self,
            version_id: &str,
        ) -> Result<Option<AutomationWorkflowVersion>, AutomationStoreError> {
            Ok((self.version.id == version_id).then(|| self.version.clone()))
        }

        fn apply_execution_transition(
            &self,
            _transition: AutomationExecutionTransition,
        ) -> Result<AutomationRunDetail, AutomationStoreError> {
            Err(AutomationStoreError::SchemaInvariant {
                message: "read-only test repository must not be mutated".to_owned(),
            })
        }
    }

    impl TestPorts {
        fn observe_repository_without_execution_lock(&self) {
            if let Some(repository) = self.repository.lock().unwrap().as_ref() {
                repository.list_runs(None).unwrap();
            }
        }

        fn execute_test_tool(
            &self,
            context: AutomationPortContext,
        ) -> Result<JsonValue, AutomationPortError> {
            self.observe_repository_without_execution_lock();
            self.tool_keys
                .lock()
                .unwrap()
                .push(context.idempotency_key.clone());
            if *self.fail_tool.lock().unwrap() {
                return Err(AutomationPortError::new("test tool failed"));
            }
            Ok(self.tool_output.lock().unwrap().clone().unwrap_or_else(|| {
                serde_json::json!({
                    "taskId": format!("task:{}", context.idempotency_key),
                })
            }))
        }
    }

    impl AutomationTaskPort for TestPorts {
        fn create_task(
            &self,
            request: AutomationCreateTaskRequest,
        ) -> Result<JsonValue, AutomationPortError> {
            self.execute_test_tool(request.context)
        }

        fn update_task_status(
            &self,
            request: AutomationUpdateTaskStatusRequest,
        ) -> Result<JsonValue, AutomationPortError> {
            self.execute_test_tool(request.context)
        }
    }

    impl AutomationTodoPort for TestPorts {
        fn add_todo(
            &self,
            request: AutomationAddTodoRequest,
        ) -> Result<JsonValue, AutomationPortError> {
            self.execute_test_tool(request.context)
        }
    }

    impl AutomationGuidePort for TestPorts {
        fn send_guide(
            &self,
            request: AutomationSendGuideRequest,
        ) -> Result<JsonValue, AutomationPortError> {
            self.execute_test_tool(request.context)
        }
    }

    impl AutomationTimelinePort for TestPorts {
        fn record_timeline(
            &self,
            request: AutomationRecordTimelineRequest,
        ) -> Result<JsonValue, AutomationPortError> {
            self.execute_test_tool(request.context)
        }
    }

    impl AutomationAgentPort for TestPorts {
        fn start_agent(
            &self,
            request: AutomationStartAgentRequest,
        ) -> Result<AutomationAgentDispatch, AutomationPortError> {
            self.observe_repository_without_execution_lock();
            let key = request.context.idempotency_key;
            self.agent_keys.lock().unwrap().push(key.clone());
            Ok(AutomationAgentDispatch::Waiting {
                task_id: format!("task:{key}"),
                turn_id: format!("turn:{key}"),
                dispatch: "started".to_owned(),
                queued_count: 0,
                worker_start_required: true,
                turn_inserted: false,
                message_id: Some(format!("message:{key}")),
                metadata: serde_json::json!({ "key": key.to_string() }),
            })
        }

        fn activate_agent(
            &self,
            activation: AutomationAgentActivation,
        ) -> Result<(), AutomationPortError> {
            let repository = self.repository.lock().unwrap().clone().unwrap();
            let detail = repository.run_detail(&activation.run_id).unwrap().unwrap();
            let node = state(&detail, &activation.node_id);
            assert_eq!(node.status, AutomationRunStatus::Running);
            assert_eq!(
                node.output
                    .as_ref()
                    .and_then(|output| output.get("waitingAgent"))
                    .and_then(JsonValue::as_bool),
                Some(true)
            );
            self.agent_activations.lock().unwrap().push(activation);
            Ok(())
        }
    }

    fn node(id: &str, kind: &str, config: JsonValue) -> AutomationNode {
        AutomationNode {
            id: id.to_owned(),
            kind: kind.to_owned(),
            title: id.to_owned(),
            position: AutomationNodePosition { x: 0.0, y: 0.0 },
            config,
        }
    }

    fn edge(source: &str, target: &str, source_handle: Option<&str>) -> AutomationEdge {
        AutomationEdge {
            id: format!("{source}:{target}:{}", source_handle.unwrap_or("")),
            source: source.to_owned(),
            target: target.to_owned(),
            source_handle: source_handle.map(str::to_owned),
            target_handle: None,
        }
    }

    fn begin_run(
        nodes: Vec<AutomationNode>,
        edges: Vec<AutomationEdge>,
        payload: JsonValue,
    ) -> (DesktopAutomationService, AutomationRunDetail) {
        let service =
            DesktopAutomationService::in_memory(Arc::new(SilentAutomationEvents)).unwrap();
        let workflow = service
            .save_draft(AutomationSaveDraftInput {
                id: Some("workflow".to_owned()),
                name: "Execution".to_owned(),
                scope: AutomationScopeFilter::default(),
                nodes,
                edges,
            })
            .unwrap();
        service.publish(&workflow.id).unwrap();
        let detail = service
            .try_begin_run(AutomationBeginRunInput {
                workflow_id: workflow.id,
                trigger: AutomationSignalEnvelope {
                    id: "signal".to_owned(),
                    kind: "manual".to_owned(),
                    project_id: Some("project".to_owned()),
                    task_id: Some("task".to_owned()),
                    backend: None,
                    event_kind: None,
                    automation_run_id: None,
                    payload,
                    created_at: 1,
                },
            })
            .unwrap();
        (service, detail)
    }

    fn engine(service: &DesktopAutomationService) -> (Arc<TestPorts>, AutomationExecutionEngine) {
        let ports = Arc::new(TestPorts::default());
        *ports.repository.lock().unwrap() = Some(service.clone());
        let engine = AutomationExecutionEngine::new(ports.clone());
        (ports, engine)
    }

    fn state<'a>(
        detail: &'a AutomationRunDetail,
        node_id: &str,
    ) -> &'a super::super::AutomationRunNodeState {
        detail
            .nodes
            .iter()
            .find(|state| state.node_id == node_id)
            .unwrap()
    }

    #[test]
    fn tool_execution_releases_repository_lock_and_uses_stable_key() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "tool",
                    "tool",
                    serde_json::json!({
                        "action": "create_task",
                        "title": "${trigger.payload.title}",
                    }),
                ),
            ],
            vec![edge("trigger", "tool", None)],
            serde_json::json!({ "title": "Rendered" }),
        );
        let (ports, engine) = engine(&service);
        assert!(state(&begun, "tool").started_at.is_none());
        let result = engine.execute_run(&service, &begun.run.id).unwrap();

        assert_eq!(result.execution, GraphExecution::Finished);
        assert_eq!(result.detail.run.status, AutomationRunStatus::Succeeded);
        assert_eq!(
            state(&result.detail, "tool").status,
            AutomationRunStatus::Succeeded
        );
        assert!(state(&result.detail, "tool").started_at.is_some());
        assert_eq!(
            ports.tool_keys.lock().unwrap().as_slice(),
            &[AutomationIdempotencyKey {
                run_id: begun.run.id,
                node_id: "tool".to_owned(),
            }]
        );
    }

    #[test]
    fn human_resume_is_atomic_and_continues_from_published_human_node() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "human",
                    "human",
                    serde_json::json!({ "prompt": "Confirm ${trigger.taskId}" }),
                ),
            ],
            vec![edge("trigger", "human", None)],
            serde_json::json!({}),
        );
        let (_, engine) = engine(&service);
        let waiting = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(waiting.execution, GraphExecution::WaitingUser);
        assert_eq!(waiting.detail.run.status, AutomationRunStatus::WaitingUser);
        assert_eq!(
            state(&waiting.detail, "human").status,
            AutomationRunStatus::WaitingUser
        );

        let wrong_node = engine
            .resume_human(
                &service,
                &begun.run.id,
                AutomationResumeRunInput {
                    node_id: Some("trigger".to_owned()),
                    payload: None,
                },
            )
            .unwrap_err();
        assert!(matches!(
            wrong_node,
            AutomationExecutionError::WaitingNodeNotFound { .. }
        ));

        let resumed = engine
            .resume_human(
                &service,
                &begun.run.id,
                AutomationResumeRunInput {
                    node_id: Some("human".to_owned()),
                    payload: Some(serde_json::json!({ "approved": true })),
                },
            )
            .unwrap();
        assert_eq!(resumed.execution, GraphExecution::Finished);
        assert_eq!(resumed.detail.run.status, AutomationRunStatus::Succeeded);
        assert_eq!(
            state(&resumed.detail, "human").status,
            AutomationRunStatus::Succeeded
        );
    }

    #[test]
    fn cancel_waiting_run_is_atomic_and_preserves_completed_nodes() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "human",
                    "human",
                    serde_json::json!({ "prompt": "Confirm cancellation" }),
                ),
            ],
            vec![edge("trigger", "human", None)],
            serde_json::json!({}),
        );
        let (_, engine) = engine(&service);
        let waiting = engine.execute_run(&service, &begun.run.id).unwrap();
        let cancelled = engine.cancel_run(&service, &begun.run.id).unwrap();

        assert_eq!(cancelled.run.status, AutomationRunStatus::Cancelled);
        assert!(cancelled.run.finished_at.is_some());
        assert_eq!(
            state(&cancelled, "trigger").status,
            AutomationRunStatus::Succeeded
        );
        assert_eq!(
            state(&cancelled, "human").status,
            AutomationRunStatus::Cancelled
        );
        assert_eq!(
            state(&cancelled, "human").output,
            state(&waiting.detail, "human").output
        );
        assert!(state(&cancelled, "human").finished_at.is_some());

        let replay = engine.cancel_run(&service, &begun.run.id).unwrap_err();
        assert!(matches!(
            replay,
            AutomationExecutionError::InvalidRunState {
                actual: AutomationRunStatus::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn resume_rejects_waiting_output_from_non_human_snapshot_node() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "tool",
                    "tool",
                    serde_json::json!({ "action": "create_task" }),
                ),
            ],
            vec![edge("trigger", "tool", None)],
            serde_json::json!({}),
        );
        let version = service
            .version(&begun.run.workflow_version_id)
            .unwrap()
            .unwrap();
        let mut detail = begun;
        detail.run.status = AutomationRunStatus::WaitingUser;
        let tool = detail
            .nodes
            .iter_mut()
            .find(|state| state.node_id == "tool")
            .unwrap();
        tool.status = AutomationRunStatus::WaitingUser;
        tool.output = Some(serde_json::json!({ "waitingUser": true }));
        let run_id = detail.run.id.clone();
        let repository = StaticRepository { detail, version };
        let engine = AutomationExecutionEngine::new(Arc::new(TestPorts::default()));
        let error = engine
            .resume_human(
                &repository,
                &run_id,
                AutomationResumeRunInput {
                    node_id: Some("tool".to_owned()),
                    payload: None,
                },
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AutomationExecutionError::SnapshotNodeKindMismatch {
                expected: "human",
                ..
            }
        ));
    }

    #[test]
    fn agent_dispatch_is_persisted_and_completion_resumes_without_redispatch() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "agent",
                    "agent",
                    serde_json::json!({ "taskId": "${trigger.taskId}" }),
                ),
            ],
            vec![edge("trigger", "agent", None)],
            serde_json::json!({}),
        );
        let (ports, engine) = engine(&service);
        let waiting = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(waiting.execution, GraphExecution::WaitingAgent);
        let agent = state(&waiting.detail, "agent");
        let activations = ports.agent_activations.lock().unwrap();
        assert_eq!(activations.len(), 1);
        assert_eq!(activations[0].run_id.as_str(), begun.run.id.as_str());
        assert_eq!(activations[0].node_id.as_str(), "agent");
        drop(activations);
        let turn_id = agent
            .output
            .as_ref()
            .and_then(|output| output.get("turnId"))
            .and_then(JsonValue::as_str)
            .unwrap()
            .to_owned();

        let retried = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(retried.execution, GraphExecution::WaitingAgent);
        assert_eq!(ports.agent_keys.lock().unwrap().len(), 1);

        let completed = engine
            .complete_agent(
                &service,
                AutomationCompleteAgentInput {
                    run_id: begun.run.id,
                    node_id: Some("agent".to_owned()),
                    turn_id,
                    success: true,
                    payload: Some(serde_json::json!({ "answer": 42 })),
                    error: None,
                },
            )
            .unwrap();
        assert_eq!(completed.execution, GraphExecution::Finished);
        assert_eq!(completed.detail.run.status, AutomationRunStatus::Succeeded);
    }

    #[test]
    fn agent_crash_retry_reuses_the_same_idempotency_key() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "agent",
                    "agent",
                    serde_json::json!({ "taskId": "${trigger.taskId}" }),
                ),
            ],
            vec![edge("trigger", "agent", None)],
            serde_json::json!({}),
        );
        let (ports, engine) = engine(&service);
        let waiting = engine.execute_run(&service, &begun.run.id).unwrap();
        let agent = state(&waiting.detail, "agent");
        service
            .apply_execution_transition(AutomationExecutionTransition {
                run_id: begun.run.id.clone(),
                run: active_run_update(
                    vec![AutomationRunStatus::Running],
                    AutomationRunStatus::Running,
                ),
                nodes: vec![AutomationNodeStateUpdate {
                    node_id: "agent".to_owned(),
                    expected_statuses: vec![AutomationRunStatus::Running],
                    status: AutomationRunStatus::Running,
                    input: agent.input.clone(),
                    output: None,
                    error: None,
                    mark_started: false,
                    finished: false,
                }],
            })
            .unwrap();

        let retried = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(retried.execution, GraphExecution::WaitingAgent);
        let keys = ports.agent_keys.lock().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], keys[1]);
    }

    #[test]
    fn agent_completion_rejects_waiting_output_from_non_agent_snapshot_node() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "tool",
                    "tool",
                    serde_json::json!({ "action": "create_task" }),
                ),
            ],
            vec![edge("trigger", "tool", None)],
            serde_json::json!({}),
        );
        let version = service
            .version(&begun.run.workflow_version_id)
            .unwrap()
            .unwrap();
        let mut detail = begun;
        let tool = detail
            .nodes
            .iter_mut()
            .find(|state| state.node_id == "tool")
            .unwrap();
        tool.status = AutomationRunStatus::Running;
        tool.output = Some(serde_json::json!({
            "waitingAgent": true,
            "turnId": "turn-tool",
        }));
        let run_id = detail.run.id.clone();
        let repository = StaticRepository { detail, version };
        let engine = AutomationExecutionEngine::new(Arc::new(TestPorts::default()));
        let error = engine
            .complete_agent(
                &repository,
                AutomationCompleteAgentInput {
                    run_id,
                    node_id: Some("tool".to_owned()),
                    turn_id: "turn-tool".to_owned(),
                    success: true,
                    payload: None,
                    error: None,
                },
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AutomationExecutionError::SnapshotNodeKindMismatch {
                expected: "agent",
                ..
            }
        ));
    }

    #[test]
    fn switch_and_stop_skip_unselected_branch_without_calling_ports() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "switch",
                    "logic",
                    serde_json::json!({ "logic": "switch", "path": "trigger.payload.route" }),
                ),
                node("stop", "logic", serde_json::json!({ "logic": "stop" })),
                node(
                    "tool",
                    "tool",
                    serde_json::json!({ "action": "create_task" }),
                ),
            ],
            vec![
                edge("trigger", "switch", None),
                edge("switch", "stop", Some("halt")),
                edge("switch", "tool", Some("default")),
            ],
            serde_json::json!({ "route": "halt" }),
        );
        let (ports, engine) = engine(&service);
        let result = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(result.detail.run.status, AutomationRunStatus::Succeeded);
        assert_eq!(
            state(&result.detail, "stop").status,
            AutomationRunStatus::Succeeded
        );
        assert_eq!(
            state(&result.detail, "tool").status,
            AutomationRunStatus::Skipped
        );
        assert!(ports.tool_keys.lock().unwrap().is_empty());
    }

    #[test]
    fn port_failure_updates_run_and_node_in_one_transition() {
        let (service, begun) = begin_run(
            vec![
                node("trigger", "trigger", serde_json::json!({})),
                node(
                    "tool",
                    "tool",
                    serde_json::json!({ "action": "create_task" }),
                ),
            ],
            vec![edge("trigger", "tool", None)],
            serde_json::json!({}),
        );
        let (ports, engine) = engine(&service);
        *ports.fail_tool.lock().unwrap() = true;
        let result = engine.execute_run(&service, &begun.run.id).unwrap();
        assert_eq!(result.execution, GraphExecution::Failed);
        assert_eq!(result.detail.run.status, AutomationRunStatus::Failed);
        assert_eq!(
            state(&result.detail, "tool").status,
            AutomationRunStatus::Failed
        );
        assert_eq!(result.detail.run.error.as_deref(), Some("test tool failed"));
    }

    #[test]
    fn invalid_multi_node_transition_rolls_back_every_change() {
        let (service, begun) = begin_run(
            vec![node("trigger", "trigger", serde_json::json!({}))],
            vec![],
            serde_json::json!({}),
        );
        let error = service
            .apply_execution_transition(AutomationExecutionTransition {
                run_id: begun.run.id.clone(),
                run: active_run_update(
                    vec![AutomationRunStatus::Running],
                    AutomationRunStatus::Running,
                ),
                nodes: vec![
                    AutomationNodeStateUpdate {
                        node_id: "trigger".to_owned(),
                        expected_statuses: vec![AutomationRunStatus::Pending],
                        status: AutomationRunStatus::Running,
                        input: serde_json::json!({}),
                        output: None,
                        error: None,
                        mark_started: true,
                        finished: false,
                    },
                    AutomationNodeStateUpdate {
                        node_id: "missing".to_owned(),
                        expected_statuses: vec![AutomationRunStatus::Pending],
                        status: AutomationRunStatus::Running,
                        input: serde_json::json!({}),
                        output: None,
                        error: None,
                        mark_started: true,
                        finished: false,
                    },
                ],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            AutomationStoreError::RunNodeNotFound { .. }
        ));
        let unchanged = service.run_detail(&begun.run.id).unwrap().unwrap();
        assert_eq!(unchanged.run.status, AutomationRunStatus::Running);
        assert_eq!(
            state(&unchanged, "trigger").status,
            AutomationRunStatus::Pending
        );
    }

    #[test]
    fn condition_logic_uses_template_input_truth_and_exact_equals() {
        let input = serde_json::json!({ "trigger": { "payload": { "count": 3 } } });
        let condition = node(
            "condition",
            "logic",
            serde_json::json!({
                "logic": "condition",
                "path": "trigger.payload.count",
                "equals": "3",
            }),
        );
        assert_eq!(
            execute_logic_node(&condition, &input).unwrap(),
            serde_json::json!({ "passed": true, "selectedHandle": "true" })
        );
    }
}
