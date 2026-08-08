# Global Automations

Lilia automation is a workspace-level capability. A workflow is not owned by a
single project tab; it listens to global Lilia signals and then narrows matches
with project, inbox, task status, backend, and event-kind filters.

## Product Boundary

The first version is a local MVP:

- `/automations` is the global entry point from the sidebar footer.
- The page has a workflow list, Vue Flow node canvas, node/scope inspector, run
  history, and node-state replay.
- Editing updates the draft. Running always binds to an immutable published
  workflow version snapshot.
- A workflow can be saved, published, enabled or disabled, manually run, and
  resumed when a human node is waiting.
- The graph is a DAG and MVP validation requires exactly one trigger node for a
  non-empty workflow.
- Concurrent runs of the same workflow are serialized. New triggers are recorded
  as skipped while an active run exists.

Deferred capabilities: cron, webhook triggers, remote execution, workflow
marketplace, loops, subflows, retries, and concurrent instances.

## Node Model

MVP nodes use JSON envelope data. Inputs contain the trigger, upstream node
outputs, and current node config. Template interpolation is path based, such as
`${trigger.taskId}` or `${nodes.agent_1.output.summary}`.

Supported node groups:

- Trigger: manual, task changes, timeline events, todo changes, and Agent
  interaction requests.
- Agent: dispatches through the Native AgentKit path (Mutsuki Agent Wire),
  composer state, task queue, timeline persistence, and permission flow.
- Logic: condition, switch, and stop.
- Tool: create task, update task status, add agent todo, create a Lilia guide,
  and write a task timeline record with `automationRunId`.
- Human: pauses the run and resumes through an AskUser-style inline panel in the
  automation inspector.

## Signals And Permissions

The signal adapter wraps existing Lilia events into `AutomationSignalEnvelope`.
Task signals keep the trigger kind as `task_changed`, while `eventKind`
distinguishes `task_created`, `task_status_changed`, and `task_updated` for
scope filtering:

- `tasks:changed`
- agent timeline persistence
- todo changes
- Agent interaction requests

Automation-originated changes carry `automationRunId`; signal dispatch ignores
those envelopes by default to prevent self-trigger loops.

Agent nodes do not bypass permissions. They use the existing composer
permission, tool consent, AskUser, pending turn, and native backend interaction
paths. Automation runs are recorded in automation tables; ordinary chat timeline
entries are only written by the existing chat runner path and carry
`automationRunId` when they belong to an automation turn.

## Persistence And Events

SQLite stores workflows, immutable versions, runs, and per-node run state in:

- `automation_workflows`
- `automation_workflow_versions`
- `automation_runs`
- `automation_run_nodes`

Tauri commands cover list/get/save draft/publish/enable/run once/resume/list
runs/get run. UI refreshes through `automation:changed`,
`automation:run-started`, `automation:run-updated`, and
`automation:run-finished`.

## IAB Confirmation Notes

The interaction model still needs live IAB confirmation before treating the
visual design as final. The concrete confirmation points are:

- Sidebar footer entry placement and icon order.
- Three-pane automation page density at desktop and narrow widths.
- Canvas controls and minimap placement.
- Node inspector field grouping for Agent, logic, tool, and human nodes.
- Run history and node replay readability during waiting, failed, and skipped
  runs.

Until that pass happens, the current UI should be treated as implementation
complete for the MVP behavior, but not as final visual sign-off.
