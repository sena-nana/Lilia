# Development and release

LiliaCode is a Cargo-first Native repository. The only desktop product is the NanaUI/WGPU application in `apps/desktop`; no JavaScript runtime or package manager is required. Android retains Gradle and the JDK as platform requirements, orchestrated through the Rust xtask.

## Repository layout

```text
LiliaCode/
├── apps/
│   ├── desktop/                 # Production Native desktop app
│   ├── service/                 # Service entry
│   └── android/                 # Android companion (Gradle/Kotlin)
├── crates/
│   ├── lilia-agent/ # Mutsuki AgentKit anticorruption layer
│   ├── lilia-storage/           # Product storage and paths
│   └── lilia-contracts/         # Rust API and canonical JSON contracts
├── xtask/                       # Development, verification, and release tasks
└── docs/                        # Markdown documentation
```

The `liliacode` command-line arguments belong to `apps/desktop` and only launch the application, open projects, perform explicit imports or task handoffs, and forward requests to the active instance. The repository does not provide a TUI, an Agent CLI, or a second command-line Agent host.

## Local development

Install the stable Rust toolchain and run from the repository root:

The Linux desktop also requires the GTK 3 and AppIndicator development libraries. On Debian or Ubuntu, install `libgtk-3-dev libayatana-appindicator3-dev`. Global shortcuts use the X11 backend on Linux.

```bash
cargo run
cargo check --locked -p lilia-desktop
cargo test --locked -p lilia-desktop
cargo xtask verify
```

Focused repository checks are available through `cargo xtask boundary-check` and `cargo xtask pin-check`.

## Agent Debug and performance

```bash
cargo xtask agent-debug
cargo xtask performance
```

Agent Debug connects only to the Native/WGPU structured development protocol and records observe/act replay, real GPU screenshots, errors, and secret-canary results under `agent-debug-runs/lilia-*`. Release binaries must not contain the debug instrumentation.

The performance task uses the fixed Native corpus for Composer, resize, a thousand-entry timeline, cold start, idle CPU, and RSS gates.

## Android

```bash
cargo xtask android doctor
cargo xtask android test
cargo xtask android build
cargo xtask android smoke
```

The first three commands check or invoke the repository Gradle wrapper. `smoke` requires ADB and a real device. CI prepares the JDK and Android SDK only for the Android job.

## Windows release

Windows builders require Rust, NSIS, `LILIA_SIGNING_PRIVATE_KEY` (or `LILIA_SIGNING_KEY_PATH`), `LILIA_SIGNING_PASSWORD` when required, `LILIA_UPDATER_PUBKEY` (the complete Minisign public-key text encoded as Base64), and `LILIA_UPDATER_BASE_URL`:

```bash
cargo xtask release windows --tag vX.Y.Z
cargo xtask installer-smoke --tag vX.Y.Z
```

The release task builds `liliacode.exe` and `liliacode_host.dll`, excludes Agent Debug markers, creates the NSIS installer, and emits the signed updater archive and `latest.json`. Installer smoke verifies installation, launch, `liliacode <project>`, single-instance behavior, upgrade, uninstall, and PATH cleanup without deleting user data.

GitHub Actions publishes only the `v*` channel. It creates a draft release after workspace and Agent Debug gates, then smokes the draft installer. Complete the real Windows record from [`docs/github/release-template.md`](../../github/release-template.md) before publishing.

## Documentation and data

`docs/` is reviewed directly as Markdown; there is no static documentation build or Pages deployment.

The production desktop uses `LILIA_HOME` (default `~/.lilia`) and the `liliacode` credential identity. It consumes shared Product/Agent authority and never opens or writes legacy `db/lilia.db`. Legacy data enters the production home only through explicit import, without automatic merging or source deletion.
