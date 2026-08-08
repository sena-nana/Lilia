<!-- Replace the main screenshot while keeping .github/assets/main-window.png to avoid README churn -->

> English | [简体中文](README.zh-CN.md) | [Web docs](https://sena-nana.github.io/LiliaCode/)

> **Development status**
>
> LiliaCode is still changing quickly; core features are incomplete; local database schemas may change and data may be cleared or migrated. Do not rely on it as the only copy of important work.

<p align="center">
  <img src="./apps/desktop/src-tauri/icons/icon.png" width="128" alt="LiliaCode logo" />
</p>

<h1 align="center">LiliaCode</h1>

<p align="center">
  <a href="https://qm.qq.com/q/WViyGEq8oA">
    <img alt="LiliaCode QQ group" src="https://img.shields.io/badge/LiliaCode-289582454-blue">
  </a>
</p>

<p align="center"><strong>An Agent collaboration desktop client for software engineering.</strong></p>

<p align="center">LiliaCode uses the Lilia product protocol implemented by Mutsuki AgentKit as the Agent core and persists recoverable local task state, helping developers manage project sessions, context, todos, and execution history.</p>

<p align="center">
  <img src="./.github/assets/main-window.png" alt="LiliaCode main window" />
</p>

---

## Product positioning

LiliaCode is the software engineering workbench in the Lilia family. Agent execution is driven by the **Lilia product protocol (Mutsuki implementation)**; the desktop layer organizes projects, tasks, sessions, permissions, and process state.

It targets developers who need long-running engineering work: each conversation can be managed as a task, agent execution and pending interactions become local state, and that state supports task trees, automation, and multi-agent collaboration.

## Lilia family

Lilia is a toolchain family for high Agent collaboration. The goal is one observable, schedulable, recoverable local workbench for different agents, runtimes, and engineering workflows.

## Core differences

- Task-shaped sessions instead of chat-only history
- Local engineering state for projects, sessions, todos, process, and key interactions
- Observable timelines for thinking, tools, commands, file changes, and final replies
- Non-interrupt interactions: permissions, plan approval, and agent questions can wait without hijacking the composer
- Structure for task trees, dependencies, automation, and helper agents

LiliaCode owns its recoverable task model and timeline. It does **not** use Claude Code or Codex official CLI / SDK / app-server as the execution path. Models can still be called through OpenAI-compatible or Anthropic Messages **LLM APIs**.

## Getting started

- Configure **Native credentials** (OpenAI or Anthropic API key, or a compatible endpoint) in Settings.
- Agent turns run on **Native AgentKit** (`native-agentkit`); Claude Code / Codex CLI are not required.
- Compatible proxies: set Base URL on the credential or model settings.
- After credentials are ready, open a conversation and send the first message.

## Feature status

Checked against the real product surface. Updated 2026-08-08.

### Shared Agent capabilities

- [x] Native AgentKit execution via LiliaCore
- [x] Permission modes: full access, ask-first, read-only
- [x] Todo display for the agent task list
- [x] Process timeline for thinking, tools, commands, plans, and replies
- [x] Key-node jump on the scrollbar
- [x] Non-interrupt interactions for permissions, questions, and plan approval
- [x] Guide queue for user todos
- [x] Unified interaction protocol for plan / tool / ask-user flows
- [x] Unified Lilia workflows (review, fix suggestion, batch apply, and built-in task kinds)
- [x] File context via `@` mentions and paste/drag attachments
- [x] Model selection with optional manual override
- [x] Slash commands for built-in and `.lilia/commands` project commands
- [x] Native credentials for OpenAI / Anthropic API keys

### LiliaCode product features

- [x] Project management, GitHub clone, overview stats, and known usage cost
- [x] Session-as-task persistence: promote drafts, archive, pin, reorder
- [x] Task tree with parent/child and dependency hints
- [x] Built-in Lilia workflow kinds (general, frontend, refactor, test, docs, git release, architecture memory)
- [ ] Plugin system (partial): official Claude/Codex extension management removed; AgentKit-native extension governance is still evolving
- [x] Memory: user/project memory with Layer 1 injection at session start
- [x] Roadmap / Milestone data path
- [ ] Automation orchestration (`v2.0` target)
- [ ] Helper agents (`v2.0` target)
- [x] Single built-in Lilia protocol path

### Android Remote Beta

See [docs/design/android-remote-control.md](docs/design/android-remote-control.md).

## Development

Common monorepo commands from the repository root:

```bash
yarn tauri:dev
yarn dev
yarn verify:desktop:test
yarn verify:contracts
yarn verify:tauri
```

Desktop app lives in `apps/desktop`. Shared contracts live in `packages/contracts`. Agent integration and storage live under `crates/`.

## License

See repository license files.
