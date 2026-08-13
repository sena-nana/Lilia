> [English](README.md) | 简体中文

> **开发状态声明：** LiliaCode 仍在快速演进。本地结构与显式迁移流程可能调整，请为重要工作保留独立副本。

<p align="center"><img src="./apps/desktop/assets/icons/icon.png" width="128" alt="LiliaCode logo" /></p>

<h1 align="center">LiliaCode</h1>

<p align="center"><strong>面向代码工程的 Native Agent 协同桌面客户端。</strong></p>

LiliaCode 以 Lilia 产品协议与 Mutsuki AgentKit 为 Agent 核心。唯一桌面实现基于 NanaUI/WGPU，负责组织项目、任务化会话、权限、待处理交互和可恢复的执行时间线，不依赖 Web 桌面运行时。

## 仓库结构

- `apps/desktop`：唯一正式桌面应用，使用 Rust、NanaUI 与 WGPU
- `apps/cli`：`liliacode` 命令行入口
- `apps/service`：共享 Service 入口
- `apps/android`：实验性 Android 远控 companion；平台构建继续使用 Gradle
- `crates/lilia-desktop-application`：与宿主无关的桌面应用服务
- `crates/lilia-contracts/contracts`：Rust 消费的产品契约事实源
- `xtask`：仓库验证、调试、性能、Android 与发布编排
- `docs`：Markdown 设计与产品文档，不构建或发布文档站

## 本地开发

安装稳定版 Rust 工具链后，从仓库根目录运行：

```bash
cargo run
cargo xtask verify
cargo xtask agent-debug
cargo xtask performance
```

Agent Debug 证据写入 `agent-debug-runs/lilia-*`。Android 使用 `cargo xtask android doctor|test|build|smoke`，其中 `smoke` 需要可用的 ADB 与真实设备。

正式桌面统一使用 `LILIA_HOME`，默认目录为 `~/.lilia`。旧版数据不会自动合并或删除，只能通过显式导入进入正式 home。

## Windows 发布

Windows 发布使用 NSIS 安装器和 Minisign 兼容更新签名：

```bash
cargo xtask release windows --tag vX.Y.Z
cargo xtask installer-smoke --tag vX.Y.Z
```

推送 `v*` tag 后，GitHub workflow 创建 draft Release，发布 `latest.json` 与带签名的更新归档，并执行安装、启动、CLI、单实例、升级和卸载 smoke。签名配置使用 `LILIA_SIGNING_*`，运行时更新验证使用 `LILIA_UPDATER_*`。

Android companion 发布前至少运行 `cargo xtask android test` 与 `cargo xtask android build`；真实设备能力通过 `cargo xtask android smoke` 单独确认。

## 感谢

- Codex 为界面设计和交互整理提供了重要参考；LiliaCode 的用户交互在这些思考基础上继续迭代。
