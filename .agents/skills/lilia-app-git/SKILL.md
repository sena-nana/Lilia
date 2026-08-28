---
name: lilia-app-git
description: Git workflow for LiliaCode desktop changes. Use when staging, committing, pushing, merging, syncing dependencies, reviewing diffs before committing, or preserving user and other-agent changes.
---

# Lilia App Git

## Before Staging

- Inspect `git status --short`.
- Use `git diff`, `git diff --numstat`, and `git diff --check` when the change is non-trivial, cross-module, generated, or near user edits.
- Stage only files that belong to the current task.
- Do not stage unrelated generated output, caches, build artifacts, local secrets, or user work.
- Never revert or overwrite user or other-agent changes unless the user explicitly requests it.

## Commit Style

- Write the commit title as a short Chinese sentence summarizing the result.
- Add a commit body only when it clarifies concrete changes. Keep it as a short list.
- Before committing logic, refactor, public module, or shared behavior changes, do a quick code self-check for duplicate branches, redundant helpers, dead state, and comments that only restate behavior.

## Push And Merge

- Before pushing, inspect the current branch and remote.
- For sync or merge work, run `git fetch origin` before trusting ahead or behind counts.
- If the repo has uncommitted changes, preserve them unless the user explicitly asks to include them.
- When pulling or merging with local changes, check whether upstream changes touch the same files before proceeding.
- Push only after the intended commit or already-ready branch state is confirmed.

## Dependency Updates

- Keep dependency update commits dependency-only unless the user asked for extra work.
- NanaUI is GIT-pinned in `apps/desktop/Cargo.toml`. Mutsuki pins live in the workspace `Cargo.toml` and must switch together; see `docs/design/mutsuki-dependency-pin.md`.
- After changing pins or `Cargo.lock`, include the validation required by `$lilia-app-validation`.
