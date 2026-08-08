# Development

The repository, package names, protocol names, and local configuration paths still use the `lilia` name to avoid breaking existing protocols and persistence paths.

## Project Structure

```text
Lilia/
├── apps/
│   ├── desktop/                # Main app: Vue 3 + Tauri 2
│   │   ├── src/                # UI, services, composables
│   │   └── src-tauri/src/      # Tauri IPC: chat / project / plugins / remote
│   ├── service/                # Shared LiliaCore + AgentKit service entry
│   └── cli/                    # Agent Wire turn CLI
├── crates/
│   ├── lilia-core/             # Product ports and task binding
│   ├── lilia-agent-integration/# Mutsuki AgentKit anticorruption layer
│   ├── lilia-storage/          # Product SQLite and paths
│   └── lilia-contracts/        # Rust product contracts
├── packages/
│   └── contracts/              # Shared TS contracts (Lilia protocol + timeline)
└── docs/
    └── design/
        ├── lilia-agent-interface.md
        └── mutsuki-dependency-pin.md
```

Agent execution uses **Mutsuki Native AgentKit** only (`native-agentkit`). Configure OpenAI-compatible or Anthropic Messages credentials for model APIs; do not install Claude Code or Codex CLIs as product backends.

## Local Development

This repository uses Node.js 26 and Yarn 4.17.1 through an explicitly installed Corepack. Run contributor commands from the repository root through the root `yarn ...` scripts. `npm`, `pnpm`, other Yarn releases, and direct workspace script entrypoints are guarded and not supported as the contributor path. The committed `.env.yarn` enables Node's portable module compile cache for repeated tooling runs.

```bash
# 1. Install Corepack and enable its Yarn shim
npm install --global corepack@0.35.0
corepack enable yarn

# 2. Install dependencies
yarn install

# 3. Start only the Vite frontend
yarn dev

# 4. Start the Tauri desktop app, which requires a local Rust toolchain and WebView2
yarn tauri:dev

# 5. Run type checks, unit tests, Rust check, and contracts check
yarn verify
```

If `yarn --version` does not report `4.17.1` after enabling Corepack, run commands through Corepack explicitly, for example `corepack yarn install` and `corepack yarn dev`. Repository scripts and workspace scripts enforce Node.js 26 and the pinned Yarn release through the same toolchain check.

## Local LiliaUI Development

The committed `package.json` files and default `yarn.lock` pin `@lilia/build`, `@lilia/config`, `@lilia/tools`, and `@lilia/ui` to the same GitHub LiliaUI commit. A normal `yarn install` does not require a local `C:\Files\workspace\LiliaUI` checkout.

When changing LiliaUI and Lilia together, run this from the Lilia repository root:

```bash
yarn liliaui:local
```

The command uses `yarn link --relative` to temporarily maintain project-level `resolutions` so the four target `@lilia/*` packages resolve to the default `../LiliaUI/packages/*` `portal:` dependencies, then refreshes `node_modules`. If the LiliaUI checkout is elsewhere, pass it through `LILIA_UI_LOCAL_PATH`:

```powershell
$env:LILIA_UI_LOCAL_PATH = "C:\Files\workspace\LiliaUI"
yarn liliaui:local
Remove-Item Env:LILIA_UI_LOCAL_PATH
```

Before committing Lilia dependency or lockfile changes, switch back to the pinned GitHub dependencies:

```bash
yarn liliaui:remote
yarn liliaui:status
```

`yarn liliaui:status` only reports whether the four LiliaUI packages currently resolve from local `portal:` paths or from the pinned GitHub commit. The lockfile policy is: the default remote manifest and lockfile are commit-ready, while local `resolutions` / `portal:` lockfile state is for personal cross-repository development and should not be bundled with ordinary app changes.

## Documentation Site

```bash
# Start the VitePress documentation site
yarn docs:dev

# Build static output for GitHub Pages
yarn docs:build

# Preview the built output locally
yarn docs:preview
```

Run documentation commands from the repository root through the root `yarn ...` scripts.

GitHub Pages deployment is handled by the repository Actions workflow. After pushing to `main`, the site is built and published to `https://sena-nana.github.io/LiliaCode/`.

## CI/CD

GitHub Actions runs CI for pull requests to `main`, pushes to `main`, and manual workflow dispatches. CI runs `yarn verify` and builds the documentation site separately, covering desktop tests, frontend build, Tauri Rust check, contracts type check, and docs build.

After pushing to `main`, the Pages workflow continues to publish the documentation site automatically. Before publishing a Windows desktop installer, sync and check the four version sources: root `package.json`, `apps/desktop/package.json`, `apps/desktop/src-tauri/Cargo.toml`, and `apps/desktop/src-tauri/tauri.conf.json`. They must match the release tag without the leading `v`.

```bash
yarn release:check --tag v1.0.0-beta.1
```

After the check passes, push a `v*` tag:

```bash
git tag v1.0.0-beta.1 && git push origin v1.0.0-beta.1
```

The release workflow runs `yarn verify` and `yarn release:check --tag <tag>` first, then builds the Windows Tauri NSIS bundle and updater artifacts and uploads them to a draft GitHub Release. Installer asset names are checked against `LiliaCode_<version>_x64-setup.*`; updater assets must include `latest.json`, `*.nsis.zip`, and `*.nsis.zip.sig`. The draft release includes a release checklist and generated release notes.

After the draft Release is created, CI runs the repeatable Windows installer smoke with `yarn release:smoke:windows --tag <tag>`. The same script can be run locally with `yarn release:smoke:windows --installer path/to/LiliaCode_<version>_x64-setup.exe`. It verifies install, main-window launch, opening a project from a fresh PowerShell or cmd with `liliacode <test-project-path>`, and removal of `liliacode` from fresh PATH after uninstall. Before publishing the release, record the installer smoke result in the Windows verification section of the Release body. Release artifacts are signed with `tauri-signing.key` for Tauri signing; the private key comes from the `TAURI_SIGNING_PRIVATE_KEY` secret and the updater public key comes from the `TAURI_UPDATER_PUBKEY` repository variable. They do not include macOS notarization or Linux/macOS installers. The Windows desktop app checks for updates on startup, then downloads, installs, and restarts after user confirmation; users can also upgrade manually by downloading and installing the newer Windows installer.

Use `docs/github/release-template.md` as the source template when preparing the GitHub Release body.

## Icons

The Tauri icon source is `apps/desktop/src-tauri/icons/icon.png`. To regenerate the desktop PNG or ICO set with the Tauri CLI, run:

```bash
yarn icons:generate
```

`icons:tauri` is kept as the same generation entrypoint:

```bash
yarn icons:tauri
```
