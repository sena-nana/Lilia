# Feature Status

Checked items are currently usable as user-facing features. Unchecked items still need critical product loops.

## Shared Agent Capabilities (Mutsuki / Lilia protocol)

- [x] Native AgentKit execution: conversation turns go LiliaCore → Mutsuki Agent Wire / AgentKit. Claude Code / Codex product CLIs are not required.
- [x] Permission modes: full access, ask-first, read-only, and free modes mapped to Mutsuki `permission_mode`.
- [x] Todo display: show the agent task list and progress.
- [x] Process timeline: reasoning, commands, tool calls, file changes, and replies projected from AgentKit events.
- [x] Key node navigation: highlight important timeline nodes and jump quickly.
- [x] Non-interruptive interaction: permission requests, agent questions, and plan confirmations move into a pending area.
- [x] Guidance queue: priority actions for user messages and plugin behavior.
- [x] MCP / shared services: MCP, Git, code index, LSP, and Memory via AgentKit plugins and shared services.
- [x] Unified interaction protocol: plan confirmation, tool consent, and agent questions use Lilia-neutral interactions.
- [x] Unified Lilia protocol: UI exposes Lilia workflows and runtime commands only; Mutsuki implements them.
- [x] Built-in workflow types: general task, frontend, refactor, test/verification, docs/prompt, Git/release, architecture/memory as persistent `ChatWorkflow` payloads.
- [x] Intelligent model selection: automatic model tier and reasoning effort with send-time overrides.
- [x] File context: mention files, directories, and images with `@`.
- [x] Slash commands: `/` palette, built-ins, and `.lilia/commands` project commands.
- [x] Native credentials: OpenAI-compatible / Anthropic Messages API keys, import, and diagnostics.
- [x] Model protocol adapters: Mutsuki `openai-compatible` and `anthropic-messages` (LLM APIs, not official agent products).

## LiliaCode-Specific Features

- [x] Project management: local and GitHub-cloned projects with overview metrics.
- [x] Task-based conversations: tasks with draft promotion, project/orphan sessions, archive, pin, and order.
- [x] Task tree: parent-child, dependencies, drag-and-drop, blockers. Auto-driving belongs to `v2.0`.
- [x] Built-in Lilia workflows: routed by `lilia_task_workflow.kind`, not external Skills.
- [ ] Automatic orchestration: framework exists; multi-agent scheduling targets `v2.0`.
- [ ] Plugin system (partial): official Claude/Codex extension management removed; AgentKit-native MCP/Skill/Hook surface is still iterating.
- [x] Memory: manual user/project memories with Layer 1 injection at session start.
- [x] Roadmap and milestones: project roadmap and task-milestone links.
- [ ] Helper agents: lower-cost assistants target `v2.0`.
- [x] Built-in Lilia protocol: single product backend path `native-agentkit`.

## Android Remote Beta

- [x] Experimental companion: PC HTTP bridge, pairing, task inbox, timeline, composer, and key interactions.
- [ ] Stable remote control and full release regression: outside current `v1.0` commitments.
