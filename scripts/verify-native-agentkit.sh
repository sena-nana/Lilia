#!/usr/bin/env bash
# Verify Native AgentKit integration (#44/#50/#46/#47) after Host pin alignment.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"
node scripts/check-mutsuki-pin.mjs
cargo test -p lilia-contracts -p lilia-core -p lilia-client -p lilia-storage -p lilia-agent-integration -p lilia-service -p lilia-service-bin -p lilia-cli --locked
cargo check -p lilia-service-bin -p lilia-cli --locked
# #58 / #60 — Desktop/CLI same LiliaClient + Shared Runtime use case.
cargo test -p lilia-cli --locked desktop_and_cli_clients_share_runtime
# Focused migration / shared-path suites (#47 / #56).
cargo test -p lilia-storage --locked migration::
cargo test -p lilia-storage --locked paths::
cargo test -p lilia-storage --locked artifact_policy::
cargo test -p lilia-service --locked service_and_desktop_share_projection_db_path
# Desktop host status / projection / legacy-compat unit tests (no full app launch).
cargo test -p lilia --lib --locked native_agent::
cargo test -p lilia --lib --locked product_core::
# #47: default package must not declare official Agent Server / Node runner resources.
node scripts/check-default-bundle-no-official-server.mjs
node scripts/mark-legacy-agent-runner.mjs --check
node scripts/check-legacy-runner-reachability.mjs
node scripts/check-legacy-default-unreachable.mjs
# The source-only escape hatch remains supported until 1.0.0; keep it compiling
# without making it part of the default Desktop feature set.
cargo check -p lilia --features legacy-runner --locked
# Default Desktop build (no legacy-runner feature) still typechecks.
cargo check -p lilia --locked
# Migration apply + first Native turn (binding → submit → projection).
cargo test -p lilia-cli --locked migration_apply_then_first_native_turn
cargo test -p lilia-storage --locked migration::compat_apply::
