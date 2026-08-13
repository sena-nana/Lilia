# Product Positioning

LiliaCode is the software engineering workbench in the Lilia family. It does not wrap official agent CLIs into a chat window. Instead, it organizes projects, **tasks**, permissions, and process state on top of the **Lilia product protocol implemented by Mutsuki AgentKit**.

Users work with manageable **tasks**, not traditional chat sessions. Agent execution details, pending interactions, and key context are saved as local task state; AgentKit sessions stay invisible and attach only through product bindings. This underpins task trees, automatic orchestration, and multi-agent collaboration.

## The Lilia Family

Lilia is a family of toolchain applications for high-collaboration agent workflows. Its goal is to connect execution environments and engineering workflows into one observable, schedulable, and recoverable local workbench.

LiliaCode focuses on software engineering. Other applications in the same family may expand into additional collaboration workflows while sharing project state, task-first models, plugins, and human-agent collaboration boundaries.

## Agent Core (three layers)

| Layer | Description |
| --- | --- |
| **Lilia Protocol** | High-level instructions: `ChatWorkflow`, `ChatRuntimeCommand`, interactions, timeline (`crates/lilia-contracts`) |
| **Model** | Catalog, manager, and router; role preset groups / tiers pick the turn model |
| **Provider** | LLM vendor connection (credentials, endpoints, protocol adapters) — not official agent products |
| LiliaCore / anticorruption | Task↔session binding, profile assembly, Agent Wire, event projection |
| Mutsuki AgentKit | Sole implementation of session / turn / approval / plugins / model gateway |

See [Provider · Model · Lilia Protocol](../../design/lilia-agent-interface.md) and [Mutsuki dependency pin](../../design/mutsuki-dependency-pin.md).

## What Makes It Different

| Capability | Description |
| --- | --- |
| Task as primary object | Manage conversations as tasks; sessions are an implementation detail, not the user work model. |
| Local engineering state | Record projects, tasks, todos, process details, and key interactions for recovery. |
| Observable process | Show reasoning, tool calls, commands, file changes, and replies in a timeline. |
| Non-interruptive interaction | Move permission requests, plan confirmations, and agent questions into a pending area. |
| Collaboration-ready structure | Shared shape for task trees, dependencies, orchestration, and helper agents. |

## Storage Boundary

LiliaCode owns its recoverable **task structure** and local task timeline as the primary working model. AgentKit sessions/checkpoints follow Mutsuki semantics; the product only stores bindings such as `AgentSessionBinding`. Product SQLite does not copy official CLI history formats and no longer ships Claude/Codex history importers.
