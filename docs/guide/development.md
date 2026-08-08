# 开发启动

LiliaCode 当前仓库、包名、协议名和本地配置路径仍沿用 `lilia` 命名，以避免破坏既有协议和持久化路径。

## 项目结构

```text
Lilia/
├── apps/
│   ├── desktop/                # 主应用：Vue 3 + Tauri 2
│   │   ├── src/                # UI、services、composables
│   │   └── src-tauri/src/      # Tauri IPC：chat / project / plugins / remote
│   ├── service/                # 共享 LiliaCore + AgentKit 的 Service 入口
│   └── cli/                    # Agent Wire turn 等 CLI
├── crates/
│   ├── lilia-core/             # 产品核心端口与任务绑定
│   ├── lilia-agent-integration/# Mutsuki AgentKit 防腐层（Wire / profile / projection）
│   ├── lilia-storage/          # 产品 SQLite 与路径
│   └── lilia-contracts/        # Rust 侧产品契约
├── packages/
│   └── contracts/              # 跨端 TS 契约（Lilia 协议 + timeline display）
└── docs/
    └── design/
        ├── lilia-agent-interface.md   # 产品协议 ↔ Mutsuki 边界
        └── mutsuki-dependency-pin.md  # mutsuki-* 依赖 pin
```

Agent 执行只走 **Mutsuki Native AgentKit**（`native-agentkit`）。模型侧通过 OpenAI-compatible / Anthropic Messages adapter 配置 API Key 或兼容端点；不要安装或对接 Claude Code / Codex 官方 CLI 作为后端。

## 本地运行

本仓库贡献者工具链统一使用 Node.js 26，并通过显式安装的 Corepack 使用 Yarn 4.17.1。请从仓库根目录通过根 `yarn ...` 脚本运行贡献命令；`npm`、`pnpm`、其他 Yarn 版本和直接进入 workspace 运行脚本都会被检查拦住。仓库提交的 `.env.yarn` 会为重复工具调用启用 Node 可移植模块编译缓存。

```bash
# 1. 安装 Corepack 并启用 Yarn shim
npm install --global corepack@0.35.0
corepack enable yarn

# 2. 首次安装依赖
yarn install

# 3. 仅启动 Vite 前端
yarn dev

# 4. 启动 Tauri 桌面端，需要本地有 Rust 工具链和 WebView2
yarn tauri:dev

# 5. 运行类型检查、单测、Rust 编译检查、契约包检查
yarn verify
```

`yarn tauri:build:no-bundle` 会执行发布级别编译但跳过安装包生成，适合发布前快速验证本机打包链路。

如果启用 Corepack 后 `yarn --version` 不是 `4.17.1`，请显式通过 Corepack 运行命令，例如 `corepack yarn install` 和 `corepack yarn dev`。仓库脚本和 workspace 脚本都会通过同一工具链检查强制使用 Node.js 26 和固定的 Yarn 版本。

`yarn dev` 是普通浏览器预览模式，Vite 会为 `@tauri-apps/api/*` 接入内存态轻量 mock，让页面可以在没有 Tauri shell 的情况下浏览基础项目、对话、设置和插件页面。`yarn tauri:dev`、`yarn build`、`yarn tauri:build`、`yarn tauri:build:no-bundle` 不启用这套 mock，仍然通过现有 Tauri command 与 Rust/SQLite 后端通信。

`yarn tauri:install` 会先注入本机 CPU 优化参数再执行打包，再打开安装程序并尝试安装；该入口面向本机安装验证，不用于通用分发。

可通过 dry-run 校验打包参数：

```bash
TAURI_TEMPLATE_INSTALL_DRY_RUN=1 yarn tauri:install
```

## LiliaUI 本地联调

默认 `package.json` 和提交版 `yarn.lock` 固定使用 GitHub 上同一个 LiliaUI commit 的 `@lilia/build`、`@lilia/config`、`@lilia/tools` 和 `@lilia/ui`。普通 `yarn install` 不依赖本机存在 `C:\Files\workspace\LiliaUI`。

需要同时修改 LiliaUI 时，从 Lilia 仓库根目录运行：

```bash
yarn liliaui:local
```

该命令会通过 `yarn link --relative` 临时维护项目级 `resolutions`，把四个目标 `@lilia/*` 包切到默认的 `../LiliaUI/packages/*` `portal:` 依赖，并刷新 `node_modules`。如果 LiliaUI 不在相邻目录，可用 `LILIA_UI_LOCAL_PATH` 指定路径：

```powershell
$env:LILIA_UI_LOCAL_PATH = "C:\Files\workspace\LiliaUI"
yarn liliaui:local
Remove-Item Env:LILIA_UI_LOCAL_PATH
```

提交 Lilia 依赖或锁文件变更前，先切回固定 GitHub 依赖：

```bash
yarn liliaui:remote
yarn liliaui:status
```

`yarn liliaui:status` 只检查当前四个 LiliaUI 包来自本地 `portal:` 还是固定 GitHub commit。提交策略是：默认远端 manifest 和锁文件可以入库，本地 `resolutions` / `portal:` lockfile 只作为个人联调状态，不随普通业务提交一起提交。

## Rust 构建缓存

根脚本会在 Rust 编译入口自动复用本机已有的 `sccache`：`yarn verify:tauri`、`yarn tauri:build`、`yarn tauri:build:no-bundle` 和 `yarn verify:agent-debug` 都会先检查 `RUSTC_WRAPPER`，如果用户已经设置则保持原值；如果未设置且当前 `PATH` 中能执行 `sccache --version`，脚本会只给本次子进程注入 `RUSTC_WRAPPER=sccache`。

仓库不会自动安装 `sccache`，没有安装时这些命令仍按普通 Rust / Tauri 流程执行。需要显式关闭本仓库的自动接入时，可在命令前设置 `LILIA_RUST_CACHE=0`。`verify:agent-debug` 本身不构建 debug 桌面二进制，但它启动的 Rust 子流程会继承同一份缓存环境；如果二进制缺失，仍需先运行 `yarn verify:tauri`、`yarn tauri:build:no-bundle` 或手动构建 debug binary。

## 文档站

```bash
# 启动 VitePress 文档站
yarn docs:dev

# 构建 GitHub Pages 静态产物
yarn docs:build

# 本地预览构建产物
yarn docs:preview
```

文档命令也需要从仓库根目录通过根 `yarn ...` 脚本运行。

GitHub Pages 部署由仓库中的 Actions workflow 自动完成。推送到 `main` 后，站点会构建并发布到 `https://sena-nana.github.io/LiliaCode/`。

## CI/CD

GitHub Actions 会在 pull request 到 `main`、推送到 `main` 或手动触发时运行 CI。CI 会执行 `yarn verify`，并单独构建文档站，确保桌面测试、前端构建、Tauri Rust 检查、contracts 类型检查和文档构建都通过。

推送到 `main` 后，文档站会继续由 Pages workflow 自动发布。发布 Windows 桌面安装包前，先同步并检查四处版本号：根 `package.json`、`apps/desktop/package.json`、`apps/desktop/src-tauri/Cargo.toml` 和 `apps/desktop/src-tauri/tauri.conf.json`。版本号必须与发布 tag 去掉 `v` 后一致。

```bash
yarn release:check --tag v1.0.0-beta.1
```

检查通过后推送 `v*` tag：

```bash
git tag v1.0.0-beta.1 && git push origin v1.0.0-beta.1
```

Release workflow 会先运行 `yarn verify` 和 `yarn release:check --tag <tag>`，再构建 Windows Tauri NSIS 安装包和 updater 产物，并上传到 draft GitHub Release。安装包命名按 `LiliaCode_<version>_x64-setup.*` 检查；updater 产物必须包含 `latest.json`、`*.nsis.zip` 和 `*.nsis.zip.sig`。release draft 会附带发布检查清单和自动生成的变更记录。

`release:check` 会自动核对版本号、Tauri Windows 打包配置、updater endpoint、`bundle.createUpdaterArtifacts`、GitHub 仓库变量 `TAURI_UPDATER_PUBKEY`、NSIS CLI 安装 hook、release notes 已知限制、安装包命名预期、Windows 安装验证记录入口，以及 release workflow 是否接入安装包 smoke 和 updater 产物上传。检查通过只代表发布元数据、记录入口和 workflow gate 已准备好。

`release:smoke:windows` 可重复执行 Windows 安装包 smoke：从本地安装包或 draft Release 安装包出发，覆盖安装、启动、`liliacode <测试项目路径>` 和卸载后的 CLI 清理。CI 会在 draft Release 生成后自动运行：

```bash
yarn release:smoke:windows --tag v1.0.0-beta.1
```

本地复核已有安装包时传入安装包路径：

```bash
yarn release:smoke:windows --installer path/to/LiliaCode_1.0.0-beta.1_x64-setup.exe
```

正式发布前，仍需要在 Release 正文的 Windows 安装验证记录中写入验证人、验证日期、Windows 环境、安装包文件名和安装 / 启动 / CLI / 卸载结果。

当前发布包使用 `tauri-signing.key` 完成签名，私钥来自 `TAURI_SIGNING_PRIVATE_KEY` secret，Tauri updater 公钥来自 GitHub 仓库变量 `TAURI_UPDATER_PUBKEY`。Windows SmartScreen 或安全软件警告风险已由签名策略降低。当前不包含 macOS 公证或 Linux/macOS 安装包。Windows 桌面应用启动后会自动检查更新，用户确认后自动下载、安装并重启；用户也可以手动下载并安装新版 Windows 安装包。

GitHub Release 正文可从 `docs/github/release-template.md` 复制后补全。

## 图标

Tauri 图标的设计稿是 `apps/desktop/src-tauri/icons/icon.png`。要用 Tauri CLI 重新生成桌面 PNG 或 ICO 时运行：

```bash
yarn icons:generate
```

`icons:tauri` 保留为同一套生成入口：

```bash
yarn icons:tauri
```
