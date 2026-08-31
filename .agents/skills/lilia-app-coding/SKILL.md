---
name: lilia-app-coding
description: Coding workflow for the LiliaCode native desktop app. Use when implementing features, fixing bugs, refactoring, adding UI surfaces or runtime commands, changing cross-end data contracts, touching apps/desktop Rust code, NanaUI consumers, or crates/lilia-contracts.
---

# Lilia App Coding

## Start With Context

- Read the relevant module, data contract, and tests before editing. `apps/desktop` is the only desktop implementation: Rust native NanaUI/WGPU, no web host, no Tauri, no JavaScript toolchain.
- Use CodeGraph first only when the index reflects the current native workspace; the index can lag behind architecture migrations, and stale hits from removed code are worse than a text search. Verify the symbol exists before building on it.
- Run `cargo xtask verify` as the default gate; see `$lilia-app-validation` for choosing checks by risk.
- For complex tasks, split work into clear sub-tasks and use subagents only where the boundary is clean enough for independent investigation or validation.

## Ownership

- `apps/desktop` owns the launcher, `LiliaShell`, feature composition, application services, platform implementations, and app state (`src/application/`, `src/runtime_shell.rs`, `src/ui_module.rs`, `src/module/`).
- `crates/lilia-contracts/contracts` is authoritative for cross-boundary data. Update the contract and its Rust API first, then sync consumers.
- Generic native controls, window primitives, layout primitives, and theming belong in NanaUI (sibling checkout; GIT-pinned in `apps/desktop/Cargo.toml`). Propose reusable layout/visual primitives there instead of re-creating them app-side.
- Do not copy shell behavior, titlebar, sidebar, settings, menus, theme, global styling, or window-state code between app and framework; see `$lilia-app-boundary` when ownership is unclear.

## Implementation Rules

- Fix root causes at the correct boundary. Do not patch symptoms with local workarounds.
- Preserve existing structures, names, and visible behavior unless the task requires changing them.
- Keep changes scoped to the requested feature or bug.
- Before changing a cross-boundary contract, define the boundary first, then update contracts, Rust APIs, consumers, and tests together.
- Do not display technical explanations in the UI.
- Do not add controls, sidebar entries, commands, or disabled placeholders that are not connected to real behavior.
- UI composition follows `$lilia-app-design`: `Stack` presets for layout, `outline(role, width)` for borders, `SemanticColorRole` for color.
- Use `$lilia-agent-debug` when adding or changing stable target IDs, the debug protocol, or desktop replay support.
- When adding Agent, automation, timeline, permission, or approval behavior, define the user-visible workflow, runtime command, event shape, persistence, and fallback before wiring UI.
- Keep provider-specific or experimental payloads behind adapter/runtime boundaries.
- Prefer simple data flow over new abstractions. Add an abstraction only when it removes real duplication or matches an existing local pattern.
- Avoid comments that restate code. Long-lived context and tradeoffs go in `docs/design/`.
- Never overwrite user or other-agent changes. If nearby files are dirty, inspect and work with those changes.

## Native UI Pattern

- Shell and view assembly live in `apps/desktop/src/runtime_shell.rs` with per-domain UiModules under `src/ui_module.rs` + `src/module/`.
- Layout composition uses `apps/desktop/src/runtime_layout.rs` (`Stack` presets, `reconcile_children`, shared control builders such as `composer_send_button`, `pill_button`).
- Style controls through `NodeStyle` builders (`surface`/`outline`/`radius`) or component builders; never write raw `LayoutStyle` fields when a preset exists.
- New messages flow through the `DesktopProgram` message enum in `src/desktop.rs`; keep snapshot assembly and mutations inside the retained-tree API, no second render path.

## Before Finishing

- Remove duplicate branches, dead state, unused helper functions, and comments that only narrate the code.
- Confirm no fake UI or unconnected action was introduced.
- Confirm `$lilia-agent-debug` requirements are met when the changed flow needs Agent/debug validation.
- Run the smallest meaningful validation for the changed behavior (`$lilia-app-validation`), or explain why validation was not run.
