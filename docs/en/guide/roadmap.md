# Roadmap

LiliaCode's roadmap is organized around one goal: turning agent sessions in software engineering into an observable, recoverable, and schedulable local task system. The project is now in the `v1.0` closing phase, so the roadmap focus has shifted from expanding new surfaces to stabilization and phased completion.

## Delivered Capabilities

- Project management: local projects and GitHub-cloned projects can show task state, activity, progress, session / task statistics, and support archiving / ordering.
- Task-based conversations: conversations are persisted as Tasks, with draft promotion, project assignment, pinning, priority, and lifecycle management.
- Task tree basics: parent-child relationships, dependencies, tree drag-and-drop, and blocker hints are available.
- Milestones: milestone persistence and task-milestone links are connected to the project roadmap page.
- Plugin and toolchain integration: MCP, Skill, Plugin, and Hook management plus runtime injection are available.
- Memory baseline: user-level and project-level memories can be saved manually and injected at session startup as the Layer 1 baseline.
- Android remote beta: the current experimental path includes the PC HTTP bridge, pairing QR code, active PC, task inbox, task detail, timeline, composer, and key remote actions.

## v1.0 Stabilization Closeout

`v1.0` is about making the first beta release trustworthy. It does not add large new surfaces; it closes the gap between existing entry points, protocol boundaries, docs, and release readiness.

- Core conversation path: stabilize sending, continuing, restoring, timeline display, permission interactions, plan confirmation, file context, slash commands, and basic IAB paths.
- Task and sidebar baseline: stabilize the current task list, pinning, archiving, parent-child tasks, dependency display, blocker hints, and basic worktree operations.
- Roadmap / Milestone baseline: make the project roadmap, milestones, task links, and first-release progress notes usable as the current project-management entry.
- Memory Layer 1: stabilize manual save, user/project memory display, session-start baseline injection, and docs boundaries.
- Plugins and tools: close the current MCP, Skill, Plugin, and Hook management surfaces plus diagnostics without expanding to full governance.
- Release workflow: complete the Windows installer, release workflow, release notes, installation checks, and known-limitations documentation.
- Android remote control: document and verify it as an experimental `v1.0-beta` companion capability. Before shipping an Android asset, run `cargo xtask android test` and `cargo xtask android build`, record real-device smoke separately, and avoid promises for a stable full remote-control product.

## v2.0 Conversation Workspace And Team Mode

`v2.0` turns LiliaCode into a daily conversation workspace. The focus expands from continuing one Agent session to organizing multiple sessions, multiple Agents, and task dependencies.

- Larger operation area: reshape the chat UI so the conversation, timeline, composer, and pending interactions have a larger stable work surface.
- Complete sidebar: finish task, artifact, Memory, file tree, simple Git operation, and project-context entries.
- Sidebar artifacts: show Agent-produced files, reports, screenshots, build results, links, and follow-up actions.
- Memory browsing and Layer 2: provide better Memory browsing, filtering, and session opportunity-window guidance.
- File tree and context: browse project files and send files, directories, images, and other context reliably to Agents.
- Simple Git operations: start with status, diff, stage/unstage, commit, branch, and worktree basics; push, merge, rebase, and other high-risk actions continue to require explicit confirmation.
- Task creation and lifecycle: let users create tasks and set status, priority, ownership, and completion criteria.
- Task dependencies and assignment: complete the dependency, blocker, assignment, and team-mode operation loop.
- Multi-Agent writing and scheduling: support multiple Agents contributing to one goal through writing, review, verification, and integration flows.
- Automation in the main conversation path: connect the automation framework to scheduling, failure handling, recovery, and degradation strategy.
- AgentKit extensions and helper agents: complete Mutsuki-native MCP / Skill / Hook management, Subagents display and scheduling, and low-cost helper Agents.
- Conversation-level completion: add message-body search, session fork, continue from a specific turn, regeneration, and more session-management actions.
- Android remote-control hardening: turn the `v1.0-beta` experimental path into a stable remote-control entry point, including event stream coverage, capability negotiation, foreground/background recovery, error recovery, and end-to-end regression coverage.

## v3.0 Project-Level Knowledge Assets

`v3.0` upgrades project-level information from support pages into long-lived maintainable assets. Architecture, principles, roadmap, and memory should persist and influence Agent work.

- Advanced project overview metrics: explain task state, activity, cost, risk, and progress instead of only showing aggregates.
- Architecture graph as a first-class asset: support generation, editing, versioning, change history, rollback, and task references.
- Art / design principles as first-class assets: make design principles, visual standards, product constraints, and project style viewable, referenceable, and evolvable.
- Advanced project Memory retrieval: add external-model retrieval, quality evaluation, recall-gap checks, and adoption metrics.
- Advanced Roadmap / Milestone interpretation: explain progress, risk, and next actions across weeks, versions, and tasks.
- Advanced automation: add cron, webhooks, retries, concurrent instances, subflows, remote execution, and workflow reuse.
- Plugin / workflow distribution: move reusable plugins, skills, and workflows from local management into distributable assets.
- Remote control and multi-device collaboration: evaluate multi-device, PC-PC, and more complex remote collaboration after Android remote-control hardening.

## Development Status

LiliaCode is still changing quickly. The local database schema may be migrated or cleared as features evolve, so do not use it as the only place where important data is stored.
