# 产品定位

LiliaCode 是 Lilia 系列中的代码工程工作台。它不是把外部 Agent 官方 CLI 包进一个聊天窗口，而是在 **Mutsuki 实现的 Lilia 自有协议** 之上，提供项目、任务、会话、权限和过程信息的桌面级组织层。

每条会话都可以被视作可管理的任务。Agent 的执行过程、待处理交互和关键上下文会沉淀为本地状态，并为后续任务树、自动编排和多 Agent 协同提供基础。

## Lilia 系列

Lilia 是面向高 Agent 协同的工具链应用系列。系列目标是把不同执行环境和工程工作流接入同一套可观察、可调度、可恢复的本地工作台。

LiliaCode 聚焦代码工程场景。同系列应用可以继续围绕其他高协同 Agent 工作流扩展，并共享项目状态、任务化会话、插件化能力和人机协作边界等基础理念。

## Agent 核心

| 角色 | 说明 |
| --- | --- |
| Lilia 产品协议 | 用户可见的 `ChatWorkflow`、`ChatRuntimeCommand`、交互与 timeline 契约（`packages/contracts`） |
| LiliaCore / 防腐层 | 任务绑定、profile 装配、Agent Wire 服务、事件投影（`lilia-core` / `lilia-agent-integration`） |
| Mutsuki AgentKit | session / turn / approval / plugin / model gateway 的唯一实现 |
| LLM 协议适配 | OpenAI-compatible 与 Anthropic Messages 等 **模型 API**；不是 Claude Code / Codex 产品 |

详细边界见 [Lilia Agent 三层协议](../design/lilia-agent-interface.md) 与 [Mutsuki 依赖 pin](../design/mutsuki-dependency-pin.md)。

## 核心差异

| 能力 | 说明 |
| --- | --- |
| 任务化会话 | 将对话作为任务管理，而不是只保存聊天记录。 |
| 本地工程状态 | 记录项目、会话、待办、过程和关键交互，便于恢复和继续推进。 |
| 过程可观察 | 用时间线呈现 Agent 的思考、工具调用、命令执行、文件变更和最终回复。 |
| 非打断交互 | 权限请求、计划确认和 Agent 提问可以进入待处理区，减少对输入流的打断。 |
| 面向协同调度 | 为任务树、依赖关系、自动编排和辅助 Agent 留出统一结构。 |

## 存储边界

LiliaCode 维护自己的可恢复任务结构与 **本地 task timeline** 作为主要工作模型。  
AgentKit session / checkpoint 由 Mutsuki 语义拥有；产品 SQLite 不复制官方 CLI 历史格式，也不再提供 Claude / Codex 历史导入工具。
