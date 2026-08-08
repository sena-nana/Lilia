# 功能状态

以下按当前可交付能力列出。已勾选项目表示可作为用户功能使用；未勾选项目表示目标能力尚有关键闭环待补齐。

## 共通 Agent 能力（Mutsuki / Lilia 协议）

- [x] Native AgentKit 执行：对话 turn 经 LiliaCore → Mutsuki Agent Wire / AgentKit，不再直连 Claude Code / Codex 官方产品。
- [x] 权限模式：按执行风险选择完全访问、询问、只读等执行范围（对齐 Mutsuki `permission_mode`）。
- [x] Todo 展示：展示 Agent 当前任务清单和执行进度。
- [x] 过程时间线：区分并展示 Agent 的思考、命令、工具调用、文件变更和回复（AgentKit 事件投影）。
- [x] 关键节点跳转：在滚动条中高亮关键节点，并支持快速跳转。
- [x] 非打断交互切换：将权限请求、Agent 提问和计划确认收进待处理区，不抢占输入框。
- [x] 引导功能：提供优先级操作队列，让用户消息和插件行为进入统一引导队列。
- [x] MCP / 共享服务接入：经 AgentKit plugin 与 shared services 发现 MCP、Git、代码索引、LSP、Memory 等能力。
- [x] 统一交互协议：计划确认、工具确认和 Agent 提问使用 Lilia 中性 interaction 契约。
- [x] 统一 Lilia 协议：界面层只暴露 Lilia workflow / runtime command；实现层由 Mutsuki 兑现。
- [x] 内置工作流类型：将通用任务、前端、重构、测试验证、文档提示词、Git 发布、架构记忆等作为可持久化 `ChatWorkflow`。
- [x] 智能模型选择：根据工作流、计划模式、上下文规模自动选择模型级别与思考强度，发送前可手动覆盖。
- [x] 文件上下文：支持通过 `@` 提及文件、目录和图片等上下文。
- [x] 斜杠命令：支持 `/` 命令面板、内置命令和 `.lilia/commands` 项目命令；结果回写任务 timeline。
- [x] Native 凭据：OpenAI-compatible / Anthropic Messages 等 LLM API Key 登录、导入与诊断。
- [x] 模型协议适配：Mutsuki `openai-compatible` 与 `anthropic-messages` adapter（**不是**官方 Agent CLI）。

## LiliaCode 特色功能

- [x] 项目级管理：管理本地项目和 GitHub clone 项目；项目总览可查看任务状态分布、最近活跃、进行中 / 阻塞数量、会话 / 任务统计和已知用量成本。
- [x] 会话任务化：会话以 Task 持久化，支持草稿提升、项目 / 孤儿会话、归档、置顶和排序。
- [x] 任务树：支持父子关系、依赖维护、树形拖拽和阻塞状态提示；自动驱动与失败重排闭环归入 `v2.0`。
- [x] 内置 Lilia 工作流类型：通过 `lilia_task_workflow.kind` 路由，不作为外部 Skill 管理。
- [ ] 自动编排：已具备 automation 执行框架；多 Agent 调度与策略闭环目标阶段为 `v2.0`。
- [ ] 插件系统（部分）：官方 Claude/Codex 扩展管理已移除；AgentKit 原生 MCP / Skill / Hook 管理面仍在迭代。
- [x] Memory：支持手动保存用户级和项目级记忆，并在会话启动时按 Layer 1 基线注入。
- [x] Roadmap / Milestone：项目路线图、里程碑与任务里程碑关联已落地。
- [ ] 辅助 Agent：会话内低成本辅助 Agent 目标阶段为 `v2.0`。
- [x] 内置 Lilia 协议：运行时只保留 `native-agentkit` 单一产品后端路径。

## Android Remote Beta

- [x] 实验性 companion：PC HTTP bridge、配对、任务收件箱、timeline、composer 与关键交互。
- [ ] 稳定远控与完整发布回归：不在当前 `v1.0` 承诺范围内。
