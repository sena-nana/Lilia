---
name: lilia-app-validation
description: Validation strategy for LiliaCode native desktop changes. Use when choosing or reporting checks after app code, contracts, UI, platform, build config, documentation, or dependency pins change.
---

# Lilia App Validation

Run the smallest check that proves the changed behavior. Prefer targeted functional checks over broad or brittle assertions. Repository command authority is `Agents.md` and `docs/guide/development.md`.

- Docs, comments, or ignore-rule cleanup: no desktop system gate. If `.gitignore` or boundary rules changed, run `cargo xtask boundary-check`.
- Default repo gate: `cargo xtask verify` (boundary, pin, workspace test, workspace check).
- Desktop-only Rust: `cargo check --locked -p lilia-desktop` and `cargo test --locked -p lilia-desktop`.
- UI main paths, Agent runtime, persistence, permission, contracts, or user-critical flows: `cargo xtask agent-debug`. See `$lilia-agent-debug`.
- Performance: `cargo xtask performance`.
- Android logic: `cargo xtask android test`. Real device: `cargo xtask android smoke`.
- Windows release packaging: `cargo xtask release windows --tag <tag>` then `cargo xtask installer-smoke --tag <tag>`.

## Test Quality

- Add tests only for behavior changes or meaningful regression risk.
- Do not add tests for documentation-only, comment-only, or formatting-only changes.
- Do not write tests that only hard-match log text, incidental strings, or snapshot-like markup.
- Assert user-visible behavior, command results, persisted records, permission outcomes, or data-contract handling.

## Reporting

- State exactly which checks ran and whether they passed.
- If a check was skipped, explain why it was not necessary.
- If a check cannot run, include the blocking error, any artifact path, and remaining risk.
- Do not treat unrelated full-suite failures as blockers without confirming they touch the edited surface.
