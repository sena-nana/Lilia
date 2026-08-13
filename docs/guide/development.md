# 开发与发布

LiliaCode 是 Cargo-first Native 仓库。桌面产品只有 `apps/desktop` 中的 NanaUI/WGPU 实现；仓库不需要 JavaScript 运行时或包管理器。Android 受平台约束继续使用 Gradle/JDK，但统一由 Rust xtask 编排。

## 项目结构

```text
LiliaCode/
├── apps/
│   ├── desktop/                 # 正式 Native 桌面应用
│   ├── cli/                     # liliacode CLI
│   ├── service/                 # Service 入口
│   └── android/                 # Android remote companion（Gradle/Kotlin）
├── crates/
│   ├── lilia-desktop-application/ # 宿主无关的桌面应用服务
│   ├── lilia-agent-integration/ # Mutsuki AgentKit 防腐层
│   ├── lilia-storage/           # 产品存储与路径
│   └── lilia-contracts/         # Rust API 与 canonical JSON contracts
├── xtask/                       # 开发、验证、发布编排
└── docs/                        # 直接阅读的 Markdown 文档
```

## 本地运行

安装 stable Rust 工具链，并从仓库根目录运行：

```bash
cargo run
cargo check --locked -p lilia-desktop
cargo test --locked -p lilia-desktop-application -p lilia-desktop
```

仓库级验证使用：

```bash
cargo xtask verify
cargo xtask boundary-check
cargo xtask pin-check
```

`verify` 负责 Cargo metadata、依赖边界、immutable revision、定向测试和 workspace 编译检查。无需额外安装前端或文档工具链。

## Agent Debug 与性能

```bash
cargo xtask agent-debug
cargo xtask performance
```

Agent Debug 只连接 Native/WGPU 开发态结构化 TCP 协议，执行 observe/act 并生成真实 GPU 截图、回放、错误和 secret canary 结果。证据写入 `agent-debug-runs/lilia-*`；发布二进制必须排除调试标记。

性能门禁使用固定 Native corpus，分别检查 Composer、resize、千条时间线、冷启动、空闲 CPU 与 RSS 的绝对阈值和历史基线。

## Android

```bash
cargo xtask android doctor
cargo xtask android test
cargo xtask android build
cargo xtask android smoke
```

`doctor` 检查 JDK、Android SDK 与 ADB；`test`、`build` 调用仓库内 Gradle wrapper；`smoke` 需要真实设备并验证远控协议。普通 Rust CI 不安装 Android 工具，Android job 单独准备 JDK/SDK。

## Windows 发布

Windows 发布机需要 Rust 与 NSIS，并配置：

- `LILIA_SIGNING_PRIVATE_KEY`（或 `LILIA_SIGNING_KEY_PATH`，以及私钥需要时的 `LILIA_SIGNING_PASSWORD`）
- `LILIA_UPDATER_PUBKEY`（完整 Minisign 公钥文本的 Base64 编码）与 `LILIA_UPDATER_BASE_URL`

发布与安装验收入口为：

```bash
cargo xtask release windows --tag vX.Y.Z
cargo xtask installer-smoke --tag vX.Y.Z
```

发布任务构建 `liliacode.exe` 与 `liliacode_host.dll`，检查 Release 不含 Agent Debug 标记，调用 NSIS，生成更新归档、签名和 `latest.json`。安装 smoke 使用隔离 home 验证安装、主窗口、`liliacode <project>`、单实例、覆盖升级、卸载和 PATH 清理，且不得删除用户数据。

GitHub Actions 仅接受 `v*` 正式通道。Release workflow 先运行 workspace 与 Agent Debug 门禁，再创建 draft Release 并对其安装包运行 smoke。正式发布前按 [`docs/github/release-template.md`](../github/release-template.md) 补全真实 Windows 验证记录。

## 文档

`docs/` 仅保存 Markdown 事实与设计记录。直接在仓库中阅读和评审，不生成静态站点，也不部署 Pages。

## 数据边界

正式桌面使用 `LILIA_HOME`，默认 `~/.lilia`，凭据身份为 `liliacode`。桌面只消费共享 Product/Agent 权威数据，不打开或写入 legacy `db/lilia.db`。旧数据通过显式导入进入正式 home；导入不会自动合并或删除源数据。
