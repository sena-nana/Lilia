# Product Positioning

LiliaCode is the software engineering workbench in the Lilia family. It does not wrap official agent CLIs into a chat window. Instead, it organizes projects, tasks, sessions, permissions, and process state on top of the **Lilia product protocol implemented by Mutsuki AgentKit**.

Each conversation can be treated as a manageable task. Agent execution details, pending interactions, and key context are saved as local state, providing the foundation for future task trees, automatic orchestration, and multi-agent collaboration.

## The Lilia Family

Lilia is a family of toolchain applications for high-collaboration agent workflows. Its goal is to connect execution environments and engineering workflows into one observable, schedulable, and recoverable local workbench.

LiliaCode focuses on software engineering. Other applications in the same family may expand into additional collaboration workflows while sharing project state, task-based sessions, plugins, and human-agent collaboration boundaries.

## Agent Core

| Role | Description |
| --- | --- |
| Lilia product protocol | User-visible `ChatWorkflow`, `ChatRuntimeCommand`, interactions, and timeline contracts (`packages/contracts`) |
| LiliaCore / anticorruption | Task binding, profile assembly, Agent Wire service, event projection |
| Mutsuki AgentKit | Sole implementation of session / turn / approval / plugins / model gateway |
| LLM protocol adapters | OpenAI-compatible and Anthropic Messages APIs — not Claude Code / Codex products |

See [Lilia Agent protocol](../../design/lilia-agent-interface.md) and [Mutsuki dependency pin](../../design/mutsuki-dependency-pin.md).

## What Makes It Different

| Capability | Description |
| --- | --- |
| Task-based sessions | Manage conversations as tasks instead of only saving chat history. |
| Local engineering state | Record projects, sessions, todos, process details, and key interactions for recovery. |
| Observable process | Show reasoning, tool calls, commands, file changes, and replies in a timeline. |
| Non-interruptive interaction | Move permission requests, plan confirmations, and agent questions into a pending area. |
| Collaboration-ready structure | Shared shape for task trees, dependencies, orchestration, and helper agents. |

## Storage Boundary

LiliaCode owns its recoverable task structure and local task timeline as the primary working model. AgentKit sessions/checkpoints follow Mutsuki semantics. Product SQLite does not copy official CLI history formats and no longer ships Claude/Codex history importers.
