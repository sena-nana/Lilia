<!-- 若要更换主界面截图，保持文件名 .github/assets/main-window.png 以避免改动 README -->

> [English](README.md) | 简体中文 | [网页版文档](https://sena-nana.github.io/LiliaCode/)

> **开发状态声明**
>
> LiliaCode 仍处于快速变更阶段；基本功能尚未完整补完；本地数据库结构可能随新功能调整，数据可能随时被清空或迁移。不建议在重度生产场景中依赖它保存唯一副本。

<p align="center">
  <img src="./apps/desktop/src-tauri/icons/icon.png" width="128" alt="LiliaCode logo" />
</p>

<h1 align="center">LiliaCode</h1>

<p align="center">
  <a href="https://qm.qq.com/q/WViyGEq8oA">
    <img alt="LiliaCode QQ 群" src="https://img.shields.io/badge/LiliaCode-289582454-blue">
  </a>
</p>

<p align="center"><strong>面向代码工程的 Agent 协同桌面客户端。</strong></p>

<p align="center">LiliaCode 以 Lilia 自有协议 + Mutsuki AgentKit 为 Agent 核心，将执行过程沉淀为可恢复、可追踪、可调度的本地任务状态，帮助开发者管理项目里的会话、上下文、待办和执行过程。</p>

<p align="center">
  <img src="./.github/assets/main-window.png" alt="LiliaCode 主界面" />
</p>

---

## 产品定位

LiliaCode 是 Lilia 系列中的代码工程工作台。Agent 执行由 **Lilia 产品协议（Mutsuki 实现）** 驱动，桌面层提供项目、任务、会话、权限和过程信息的组织能力。

它面向需要长期推进工程项目的开发者：每条会话都可以被视作可管理的任务，Agent 的执行过程和待处理交互会沉淀为本地状态，并为后续任务树、自动编排和多 Agent 协同提供基础。

## Lilia 系列

Lilia 是面向高 Agent 协同的工具链应用系列。系列目标是把不同 Agent、执行环境和工程工作流接入同一套可观察、可调度、可恢复的本地工作台。

LiliaCode 聚焦代码工程场景；同系列应用可以继续围绕其他高协同 Agent 工作流扩展，并共享项目状态、任务化会话、插件化能力和人机协作边界等基础理念。

## 核心差异

- 任务化会话：将对话作为任务管理，而不是只保存聊天记录。
- 本地工程状态：记录项目、会话、待办、过程和关键交互，便于恢复和继续推进。
- 过程可观察：用时间线呈现 Agent 的思考、工具调用、命令执行、文件变更和最终回复。
- 非打断交互：权限请求、计划确认和 Agent 提问可以进入待处理区，减少对输入流的打断。
- 面向协同调度：为任务树、依赖关系、自动编排和辅助 Agent 留出统一结构。

LiliaCode 维护自己的可恢复任务结构与 timeline；不再依赖 Claude Code / Codex 官方 CLI、SDK 或 app-server 作为执行路径。模型侧仍可通过 OpenAI-compatible / Anthropic Messages 等 **LLM API** 接入。

## 安装后如何跑起来

- 在设置中配置 **Native 凭据**（OpenAI 或 Anthropic API Key，或兼容端点）。
- Agent 执行默认走 **Native AgentKit**（`native-agentkit`），无需安装 Claude Code 或 Codex CLI。
- 兼容 API / 本地代理：在凭据或模型配置中填写 Base URL，例如 `http://127.0.0.1:15721`。
- 配置完成后回到对话页发送第一条消息；连接与凭据状态可在设置页刷新检查。

## 功能状态

以下按当前真实接入面记录。只有已经能作为用户功能使用的项目标记为完成。最近核对时间：2026-08-08。

### 共通 Agent 能力

- [x] Native AgentKit 执行：对话 turn 走 LiliaCore，不再直连 Claude Code / Codex 官方产品。
- [x] 权限模式：按执行风险选择完全访问、询问、只读等执行范围。
- [x] Todo 展示：展示 Agent 当前任务清单和执行进度。
- [x] 过程时间线：区分并展示 Agent 的思考、命令、工具调用、文件变更、计划和最终回复。
- [x] 关键节点跳转：在滚动条中高亮关键节点，并支持快速跳转。
- [x] 非打断交互切换：权限请求、Agent 提问和计划确认可以进入待处理区，不抢占输入框。
- [x] 引导队列：用户引导 Todo 可创建、排队、串行发送，并在运行中恢复队列状态。
- [x] 统一交互协议：统一计划确认、工具确认和 Agent 提问。
- [x] 统一 Lilia 工作流：内置任务工作流、审查、修复建议、批量应用等在界面层使用 Lilia 协议名。
- [x] 文件上下文：支持通过 `@` 提及文件、目录和图片等上下文，也支持粘贴/拖入附件。
- [x] 智能模型选择：按任务上下文自动选择模型级别与思考强度，发送前仍可手动覆盖。
- [x] 斜杠命令：支持在输入框通过 `/` 打开命令面板，执行内置命令和 `.lilia/commands` 项目命令。
- [x] Native 凭据：OpenAI / Anthropic API Key 登录、导入与诊断。

### LiliaCode 特色功能

- [x] 项目级管理：支持本地项目、GitHub clone 项目、项目总览、任务状态分布、最近活跃、会话 / 任务统计和已知用量成本。
- [x] 会话任务化：会话以 Task 持久化，支持草稿提升、项目内会话、孤儿会话、归档、置顶和排序。
- [x] 任务树：支持父子关系、依赖维护、树形拖拽和阻塞状态提示；自动驱动、阻塞调度和失败重排闭环尚未完整打通。
- [x] 内置 Lilia 工作流类型：通用任务、前端、重构、测试验证、文档提示词、Git 发布和架构记忆等内置目录通过 `lilia_task_workflow.kind` 路由。
- [ ] 插件系统（部分接入）：官方 Claude/Codex 扩展管理已移除；AgentKit 原生扩展治理仍在迭代。
- [x] Memory：支持手动保存用户级和项目级记忆，并在会话启动时按 Layer 1 基线注入；外置模型检索与机会窗口引导尚未实现。
- [x] Roadmap / Milestone：项目路线图、里程碑与任务里程碑关联的数据链路已落地；当前主要待补齐的是度量解释性和高级汇总视图体验。
- [ ] 自动编排（目标阶段：`v2.0`）：还没有根据任务状态、依赖关系和用户策略调度多个 Agent。
- [ ] 辅助 Agent（目标阶段：`v2.0`）：还没有在会话中运行低成本 Agent 来监督和辅助主 Agent。
- [x] 内置 Lilia 协议：运行时只保留单一内置协议路径。

### Android Remote Beta

- [x] 实验性 Android companion：PC HTTP bridge、二维码配对、trusted device、active PC、任务收件箱、任务详情、timeline 轮询、composer、中断 / 重试、process 命令、session fork 和 pending interaction 响应已通过 PC runner 与 task timeline 接入。
- [x] 远控契约基线：`packages/contracts` 维护 typed remote-control request / response / event 形状；`remote-control-command-contract.json` 只保留 Tauri IPC 命令名。
- [ ] v1 beta 限制：离线队列、PC-PC 路由、多设备协作、Android 本地 agent、完整 Android 设置面、push-style event stream 和完整发布回归不属于当前承诺。
- [ ] Android 发布准入：只有在当前构建通过 `yarn android:verify` 后，才把 companion APK 作为 experimental beta 资产发布。

## 项目结构

> 当前仓库、包名、协议名和本地配置路径仍沿用 `lilia` 命名，以避免破坏既有协议和持久化路径。

```text
Lilia/
├── apps/
│   ├── android/                # 实验性 Android remote companion
│   └── desktop/                # 主应用：Vue 3 + Tauri 2
│       ├── src/
│       │   ├── layouts/        # AppShell / SecondaryPanel / TitleBar
│       │   ├── components/     # ViewTabs / TodoFloat / ChatComposer 等
│       │   ├── pages/          # project/ProjectShell / TaskDetail / Settings
│       │   ├── services/       # projectsStore / tasksStore / todos / chat
│       │   ├── styles/         # 主题令牌、标准组件样式、壳层样式和按需页面样式
│       │   ├── router.ts
│       │   └── mainBootstrap.ts
│       └── src-tauri/          # Tauri 2 Rust 端
│           └── src/
│               ├── store.rs    # lilia-store：SQLite + r2d2 + 迁移
│               ├── todos.rs    # TodoWrite / todo_list 事件拦截 → TaskTodo upsert
│               ├── plugins.rs  # Claude skills / plugins / MCP 与 Codex MCP 管理
│               └── lib.rs      # chat / settings / project / plugin IPC
└── packages/
    └── contracts/              # 跨端共享 TS 类型与 timeline display 规则
```

## 早期开发

LiliaCode 贡献者工具链统一使用 Node.js 26，并通过显式安装的 Corepack 使用 Yarn 4.17.1。请从仓库根目录通过根 `yarn ...` 脚本运行贡献命令；`npm`、`pnpm`、其他 Yarn 版本和直接进入 workspace 运行脚本都会被检查拦住。仓库提交的 `.env.yarn` 会为重复工具调用启用 Node 可移植模块编译缓存。

```bash
# 1) 安装 Corepack 并启用 Yarn shim
npm install --global corepack@0.35.0
corepack enable yarn

# 2) 安装依赖（首次）
yarn install

# 3) 仅启动 Vite 前端
yarn dev

# 4) 启动 Tauri 桌面端（需要本地有 Rust 工具链 + WebView2）
yarn tauri:dev

# 5) 运行类型检查 / 单测 / Rust 编译检查 / 契约包检查
yarn verify

# 6) 启动、构建或预览文档站
yarn docs:dev
yarn docs:build
yarn docs:preview
```

如果启用 Corepack 后 `yarn --version` 不是 `4.17.1`，请显式通过 Corepack 运行命令，例如 `corepack yarn install` 和 `corepack yarn dev`。仓库脚本和 workspace 脚本都会通过同一工具链检查强制使用 Node.js 26 和固定的 Yarn 版本。

## 首发发布打包

Windows 首发安装包由 release workflow 生成。发布前先同步根 `package.json`、`apps/desktop/package.json`、`apps/desktop/src-tauri/Cargo.toml` 和 `apps/desktop/src-tauri/tauri.conf.json` 四处版本号，再运行：

```bash
yarn release:check --tag vX.Y.Z
```

推送 `v*` tag 后，workflow 会先运行 `yarn verify` 和 `yarn release:check --tag <tag>`，再构建 Windows Tauri 安装包并上传到 draft GitHub Release，随后对 draft 安装包运行 `yarn release:smoke:windows --tag <tag>`。正式发布前保持 draft 状态，确认 Windows 安装包 smoke 已覆盖安装、启动主窗口、`liliacode <测试项目路径>` 和卸载后的 CLI 清理，并补全 Release 验证记录。当前稳定发布包仅面向 Windows，没有代码签名，不包含 Tauri updater，升级方式是手动下载并安装新版安装包。

Android companion APK 属于实验性 beta 资产。只有在当前构建通过 `yarn android:verify` 后才附加到 release，并在文件名或发布说明中标明 Android remote beta，不能描述为稳定完整远控产品。

Tauri 图标的设计稿是 [apps/desktop/src-tauri/icons/icon.png](apps/desktop/src-tauri/icons/icon.png)。要通过 Tauri CLI 重新生成桌面 PNG / ICO 时跑 `yarn icons:generate`。`yarn icons:tauri` 保留为同一套生成入口。

## 感谢

- Codex 为界面设计和交互整理提供了重要参考；LiliaCode 的用户交互在这些思考基础上继续迭代。
