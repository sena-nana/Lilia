> English | [简体中文](README.zh-CN.md)

> **Development status:** LiliaCode is evolving quickly. Keep independent copies of important work because local schemas and migration paths may change.

<p align="center"><img src="./apps/desktop/assets/icons/icon.png" width="128" alt="LiliaCode logo" /></p>

<h1 align="center">LiliaCode</h1>

<p align="center"><strong>A native Agent collaboration desktop client for software engineering.</strong></p>

LiliaCode uses the Lilia product protocol implemented by Mutsuki AgentKit. Its NanaUI/WGPU desktop organizes projects, task-shaped sessions, permissions, pending interactions, and recoverable execution timelines without a web desktop runtime.

## Repository

- `apps/desktop`: the only desktop application, built with Rust, NanaUI, and WGPU
- `apps/cli`: the `liliacode` command-line entry
- `apps/service`: the shared service entry
- `apps/android`: the experimental Android remote companion; Gradle remains the platform build system
- `crates/lilia-desktop-application`: host-independent desktop application services
- `crates/lilia-contracts/contracts`: canonical product contracts consumed by Rust
- `xtask`: repository verification, debug, performance, Android, and release orchestration
- `docs`: Markdown design and product documentation; no documentation site is built or published

## Development

Install the stable Rust toolchain, then run commands from the repository root:

```bash
cargo run
cargo xtask verify
cargo xtask agent-debug
cargo xtask performance
```

Agent Debug evidence is written under `agent-debug-runs/lilia-*`. Android commands are `cargo xtask android doctor|test|build|smoke`.

## Windows release

Windows releases use NSIS and Minisign-compatible updater signatures:

```bash
cargo xtask release windows --tag vX.Y.Z
cargo xtask installer-smoke --tag vX.Y.Z
```

The `v*` GitHub workflow builds a draft release, publishes `latest.json` with the signed updater archive, and runs installer smoke. Signing uses `LILIA_SIGNING_*`; runtime updater verification uses `LILIA_UPDATER_*`.

## License

See the repository license files.
