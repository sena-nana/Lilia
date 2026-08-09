# 产品定位

LiliaCode 是 Lilia 系列中的代码工程工作台。它不是把外部 Agent 官方 CLI 包进一个聊天窗口，而是在 **Mutsuki 实现的 Lilia 自有协议** 之上，提供项目、**任务**、权限和过程信息的桌面级组织层。

用户面对的是可管理的 **任务（Task）**，而不是传统聊天 session。Agent 的执行过程、待处理交互和关键上下文沉淀为本地任务状态；底层 AgentKit session 对用户隐形，仅通过产品绑定挂接。这为任务树、自动编排和多 Agent 协同提供基础。

## Lilia 系列

Lilia 是面向高 Agent 协同的工具链应用系列。系列目标是把不同执行环境和工程工作流接入同一套可观察、可调度、可恢复的本地工作台。

LiliaCode 聚焦代码工程场景。同系列应用可以继续围绕其他高协同 Agent 工作流扩展，并共享项目状态、任务主对象、插件化能力和人机协作边界等基础理念。

## Agent 核心（三层）

| 层 | 说明 |
| --- | --- |
| **Lilia Protocol** | 高层对话指令：`ChatWorkflow`、`ChatRuntimeCommand`、交互与 timeline（`packages/contracts`） |
| **Model** | 模型目录、管理器与分流器；按角色预设组 / tier 路由本轮模型 |
| **Provider** | LLM 提供方与连接实现（凭据、端点、协议 Adapter）；不是官方 Agent 产品 |
| LiliaCore / 防腐层 | Task↔Session 绑定、profile 装配、Agent Wire、事件投影 |
| Mutsuki AgentKit | session / turn / approval / plugin / model gateway 的唯一实现 |

详细边界见 [Provider · Model · Lilia Protocol 架构](https://github.com/sena-nana/LiliaCode/blob/main/docs/design/lilia-agent-interface.md) 与 [Mutsuki 依赖 pin](https://github.com/sena-nana/LiliaCode/blob/main/docs/design/mutsuki-dependency-pin.md)。

## 核心差异

| 能力 | 说明 |
| --- | --- |
| 任务为主对象 | 将对话作为任务管理；session 是实现细节，不对用户暴露为工作主模型。 |
| 本地工程状态 | 记录项目、任务、待办、过程和关键交互，便于恢复和继续推进。 |
| 过程可观察 | 用时间线呈现 Agent 的思考、工具调用、命令执行、文件变更和最终回复。 |
| 非打断交互 | 权限请求、计划确认和 Agent 提问可以进入待处理区，减少对输入流的打断。 |
| 面向协同调度 | 为任务树、依赖关系、自动编排和辅助 Agent 留出统一结构。 |

## 存储边界

LiliaCode 维护自己的可恢复 **任务结构** 与 **本地 task timeline** 作为主要工作模型。  
AgentKit session / checkpoint 由 Mutsuki 语义拥有；产品仅保存 `AgentSessionBinding` 等挂钩，不复制官方 CLI 历史格式，也不再提供 Claude / Codex 历史导入工具。
