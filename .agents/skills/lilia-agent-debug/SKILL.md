---
name: lilia-agent-debug
description: Agent debugging workflow for the LiliaCode native desktop. Use when adding, changing, validating, or reviewing stable target IDs, the LILIA_AGENT_DEBUG TCP protocol, debug-only instrumentation, or cargo xtask agent-debug harness behavior.
---

# Lilia Agent Debug

Treat Agent debugging as a developer interface, not as visible product UI. Users see normal application state; Agents get stable hidden targets and a development-only protocol.

Harness behavior, corpus, and evidence layout are defined in `docs/design/agent-debug-harness.md`. Run `cargo xtask agent-debug` from the repository root.

## Ownership

- Protocol, debug commands, and release exclusion live in `apps/desktop/src/agent_debug.rs`.
- Product target IDs live in `apps/desktop/src/target_ids.rs`.
- xtask starts the debug desktop, drives observe/act, and writes `agent-debug-runs/lilia-*`.
- Generic control instrumentation belongs with the control owner; see `$lilia-app-boundary`.

## Implementation Pattern

1. Gate the TCP listener with `LILIA_AGENT_DEBUG`. Release builds must not contain the listener, fixed debug markers, or test fixtures.
2. Expose stable target IDs for primary controls, important rows, retry/recover actions, filters, tabs, dialogs, and destructive confirmations.
3. Name targets by functional path, not translated text, layout position, or pixel coordinates: `lilia.settings.open`, `lilia.task-session.composer.input`.
4. Keep target IDs invisible and non-semantic to users. Do not add public technical instructions, automation labels, or debug-only copy.
5. `act` only accepts protocol-defined targets and typed actions. Do not drive the UI by screenshot matching or guessed coordinates.
6. Screenshots come from the real WGPU surface. A screenshot without a matching product-state observation does not pass.

## Validation

- For target ID, protocol, or main-path UI changes, run `cargo xtask agent-debug`.
- Evidence must include observations, replay, errors, screenshots, and secret-canary results under `agent-debug-runs/lilia-*`.
- If the harness cannot run, report the missing window/GPU capability, command, artifact path if any, and remaining risk. Do not treat a skip as a pass.
