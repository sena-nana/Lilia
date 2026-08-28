---
name: lilia-app-boundary
description: Ownership rules for deciding whether a LiliaCode desktop change belongs in apps/desktop, NanaUI, or lilia-contracts. Use when a change touches shell, titlebar, sidebar, settings, menus, theme, window state, layout primitives, platform APIs, or cross-end data.
---

# Lilia App Boundary

Put behavior in `apps/desktop` when it is application-specific: product workflows, launcher and CLI forwarding, `LiliaShell` composition, feature wiring, application services, platform implementations that the product owns, and app state.

Put behavior in NanaUI when it is a reusable native control, window primitive, layout primitive, or theme capability. NanaUI is GIT-pinned from `apps/desktop/Cargo.toml`; do not copy framework code into the app, and do not edit cargo git checkouts in place as a product fix.

Put cross-boundary data in `crates/lilia-contracts/contracts` first, then the crate's Rust API, then consumers. Do not let UI modules invent a second payload shape.

## Common Decisions

- New product page, panel, or workflow: implement in `apps/desktop` (`src/application/`, `src/module/`, `src/runtime_shell.rs`) and route messages through `DesktopProgram`.
- New reusable row, dialog, menu, card, or layout preset: implement in NanaUI, then consume it from the app.
- Titlebar, sidebar, settings, and menus as LiliaCode product chrome: keep in `apps/desktop`. Generic window chrome, control behavior, and tokens: NanaUI.
- One-off business visualization: keep scoped in the app. A pattern that repeats across surfaces: lift to NanaUI.

## Agent-Friendly Ownership

Use `$lilia-agent-debug` for debug protocol and target-ID rules. Use this section only to decide who owns the control.

- Generic shell controls that NanaUI owns also own their stable target IDs.
- Product controls, rows, records, and actions in `apps/desktop` own `target_ids` in `apps/desktop/src/target_ids.rs`.
- App-specific Agent timeline, approval, automation, and persistence stay in the app and `lilia-contracts`.

If both sides change, define the NanaUI or contract interface first, then wire the app through that interface.

## Guardrails

- Do not copy NanaUI shell or style code into the app to make a quick fix.
- Do not make the app depend on private NanaUI internals or undocumented node structure.
- When unsure, inspect the current app module and the NanaUI public surface before choosing a boundary.
