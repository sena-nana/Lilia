---
name: lilia-app-design
description: Design and interaction standards for the LiliaCode native desktop app. Use when designing, implementing, reviewing, or fixing pages, panels, cards, empty/loading/error states, dialogs, menus, layout composition, borders, visual hierarchy, or any UI behavior in apps/desktop rendered with NanaUI (Rust retained tree, WGPU).
---

# Lilia App Design

## Core Direction

Design the app as a restrained engineering tool. Prioritize clear position, current state, available actions, and decisions that need user attention.

Every main view must answer:

- Where am I?
- What is the current state?
- Does the user need to act?

Never show technical implementation notes, roadmap placeholders, or UI that looks functional but is not wired to real behavior. Sidebar items, buttons, menu entries, disabled controls, and status surfaces must represent actual reachable state or real unavailable state.

## Layout: use Stack presets, never hand-written layout fields

The desktop is a NanaUI retained tree (`apps/desktop/src/desktop.rs` + `runtime_shell.rs`). There is no CSS. Vertical stacking is the layout engine default; a horizontal row only exists if you ask for it.

- Compose rows with the `Stack` presets from `nana-ui::runtime::Stack`: `row` (toolbar, shrink to content), `fill_row` (span leftover width), `bar` (full-width, no grow: top/bottom bars), `column` (vertical, height from content), `fill_column` (main content area: fill leftover height, grow 1, shrink 1).
- The most common composition bug: using `column` where `fill_column` is needed — the main pane stops stretching and bottom composers stop hugging the bottom. Main content areas and composer-pane bodies use `fill_column`.
- `apps/desktop/src/runtime_layout.rs` (`HostStack`) is the app-shell composition idiom and mirrors these presets; its `reconcile_children` pattern is how ordered children get mounted. Keep using it for existing shell surfaces; prefer `Stack` for new standalone containers.
- Authoritative framework references: NanaUI `docs/rust-layout.md` (layout + border recipes), `docs/components.md`, `docs/look.md` — found in the NanaUI checkout (`~/.cargo/git/checkouts/nanaui-*/<rev>/docs/`) or the sibling workspace checkout.
- Shrink is opt-in: unwritten `flex_shrink` behaves as 0 (not CSS's 1) and `align_items` defaults to `Start`. The presets already encode this; do not "fix" a preset by hand-editing raw layout fields.

## Cards And Borders

Border color and border width live in different fields; if either is missing the border silently does not draw. Always write them as one unit:

- Container cards: `Stack::column(gap).padding(x).surface(SemanticColorRole::Surface).outline(SemanticColorRole::Border, 1.0).radius(r)` — see `composer_card()` in `apps/desktop/src/runtime_layout.rs:61` for the canonical card.
- `Card` control: pick `kind(CardKind::Outlined)` for the 1px outline instead of hand-assembling borders. An explicit `.style(...)` border/background/radius now wins over the kind default.
- Never set `border` without `border_width`, or the reverse. Colors always come from `SemanticColorRole` tokens, never raw `[f32;4]` values.

## Visual Language

- Keep the interface quiet, dense enough for repeated work, and easy to scan.
- Use this hierarchy: main content > current state > process information and secondary actions.
- Use short, direct, actionable copy. Buttons name actions, status text states facts, hints explain impact.
- Prefer `IconButton` for familiar tools such as collapse, search, settings, expand, and window controls; use text buttons when the action needs explicit wording.
- Cards are for independent information groups, repeated items, dialogs, and actual tool containers — not marketing streams or nested panels. Avoid hero blocks and oversized headings.
- Rows and controls must have clear hover, active, muted, disabled, loading, empty, and error states without layout shift.

## Tokens And Themes

- Use `SemanticColorRole` roles (`Surface`, `Border`, `BorderSoft`, `Text`, `TextMuted`, `Accent`, `Selected`, ...) resolved by the NanaUI theme. Never hardcode RGBA in business modules.
- Soft state roles are for state backgrounds and selection only, not large page backgrounds or decorative blocks.
- Shared tokens, metrics, and component states belong in NanaUI first (`UI_METRICS`, theme); `apps/desktop` keeps only product-specific composition. See `docs/design/style-standard.md`.
- Light and dark themes both derive from the shared palette — verify both before finishing a visual change.

## Menus, Dialogs, Overlays

- Use NanaUI overlay controls: `Dialog`, `ActionMenu`/`AnchoredActionMenu`, `Popover`, `Drawer`. Anchor them to the trigger control's slot; do not position overlays with absolute coordinates.
- Menus and confirm dialogs receive real items and handlers; dangerous actions use error color only for dangerous hover, pending, or confirmation state.

## Agent-Friendly UI

Use `$lilia-agent-debug` for detailed Agent debug implementation and validation rules. Keep the user-facing UI normal while exposing stable hidden structure.

- Keep `data-agent-id` invisible and non-semantic to users. Do not add public technical instructions, automation labels, or debug-only copy to the UI.
- Important state must be visible as product state: pending approval, blocked work, failed action, empty result, loading, unavailable provider, and recoverable error all need clear user-facing states and real actions where applicable.
- If a visible action cannot be executed, show a truthful unavailable state or remove the action. No placeholder buttons, fake menus, fake sidebar items, or unconnected Agent affordances.

## Review Checklist

- The UI still feels like a restrained engineering tool.
- Rows/panels use `Stack` presets (or `HostStack` helpers) with correct fill vs. shrink semantics; nothing stacks vertically by accident.
- Every border pairs color + width through `outline(...)` or `CardKind`; no silent zero-width or colorless borders.
- Colors come from `SemanticColorRole`; no new private color system.
- Navigation and actions are real, reachable, and wired.
- `$lilia-agent-debug` is followed for Agent-operated flows.
- Light and dark themes remain readable; hover, active, loading, empty, and disabled states keep stable geometry.
