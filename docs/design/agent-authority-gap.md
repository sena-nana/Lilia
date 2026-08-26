# Agent 回合权威对照：LiliaCode 自建 FSM ↔ Mutsuki AgentKit

本文是 Agent 权威下沉 Mutsuki 的前置勘定。目标是把 `crates/lilia-desktop-application/src/agent.rs`（3824 行）里的自建 turn 状态机逐项映射到 Mutsuki AgentKit，明确哪些直接删除、哪些改成 Agent 插件、哪些必须留在 LiliaCode 侧。

对照基准：Mutsuki pin rev `bb728d20`（见 [mutsuki-dependency-pin.md](mutsuki-dependency-pin.md)）。

## 结论

AgentKit 已经拥有**回合执行**的全部权威：session 事务、turn 租约、审批与交互的版本化等待/恢复、会话分叉、工具路由、事件序列与订阅、回合内上下文压缩。这些在 LiliaCode 侧全部删除。

AgentKit **没有**的都不是回合状态机，而是回合之外的产品编排：**多回合排队**、**失败回合重试**、**会话标题生成**、**auto-turn 档位决策**、**automation 关联**，以及三个把产品载荷翻译成 AgentKit 载荷的适配器（`tool_consent`、`mcp_elicitation`、architecture 交互的图应用）。它们留在 LiliaCode，但必须建立在 AgentKit 的 session 之上，不得再自建第二套 turn 状态。

删除的主体是 `agent.rs` 约 292–862 行：`DesktopAgentRuntime` / `AgentRuntimeState` / `TaskRuntimeState` / `ActiveTurn` / `ActiveTurnPhase` / `QueuedTurn` 及其全部相位迁移方法。

## 逐项对照

| # | 能力 | LiliaCode 现状 | Mutsuki AgentKit | 处置 |
|---|------|----------------|------------------|------|
| 1 | 回合状态机 | `ActiveTurnPhase`（`Starting`/`Running`/`WaitingApproval`/`ResolvingApproval`/`WaitingInteraction`/`ResolvingInteraction`/`Finishing`），`agent.rs:329` | `AgentRunStatus`（`Completed`/`WaitingApproval`/`WaitingInteraction`/`BudgetExceeded`/`Cancelled`/`Failed`）由 `mutsuki-plugin-agent-loop` 驱动 | **删除**。`DesktopTurnState` 改为 `AgentEvent::TurnState` 的投影 |
| 2 | 单回合互斥 | `TaskRuntimeState.active` + `claim_token` + `worker_started` | `AgentLoop::acquire_turn` 返回 `AgentTurnLease`，重复进入报 `agent.turn.already_active` | **删除**，依赖 AgentKit 租约 |
| 3 | 回合排队 | 双层：SQLite `desktop_pending_turns` FIFO + 内存 `VecDeque`，`turn_queue.rs` | **不提供**。一个 session 同时只有一个非终态 turn | **保留**，但降级为纯队列：只负责"下一个提交什么"，不再持有 turn 生命周期 |
| 4 | 幂等提交 | `enqueue_idempotent` 的 `ON CONFLICT(turn_id) DO NOTHING`；automation 用 `automation-turn:{key}` | wire `SubmitTurn { idempotency_key }`，同键同载荷重放既有结果，异载荷报 `agent.approval.idempotency_conflict` 类错误 | **改接**：队列只做本地去重，权威幂等交给 wire |
| 5 | 显式取消 | `interrupt_task_turn` + `TurnCancellationMode::User` + 队列清空，`agent.rs:1885` | `CancelTurn { session_id, turn_id, expected_version }`；`local_runtime` 转 `runtime.cancel_task` | **删除**取消状态机，保留"取消同时清空本地队列"的产品语义 |
| 6 | 中断（回合中新输入） | 默认排队不抢占；`non_interrupt_mode` 只在 UI 层生效 | 明确拒绝：waiting turn 追加 user message 报 `agent.run.resume_messages_not_allowed` | **对齐**。LiliaCode 的"排队不抢占"与 AgentKit 语义一致，实现改为队列驱动 |
| 7 | 失败重试 | 无自动重试。`timeline_retry.rs` 用 `retryContext` 重建请求并以**新 turn id** 重投 | **不提供** `RetryTurn`。只有 wire 幂等重放与回合内 `RetryableFailure` 步骤重试 | **保留** LiliaCode 的"重建请求 + 新 turn"，它本来就不是状态机 |
| 8 | 审批 | `WaitingApproval` 相位 + `pending_projections` 表 + `respond_task_approval`，`agent.rs:1921` | `PermissionRequest`/`PermissionDecision`（带 `version`），wire `ApproveAction`/`RejectAction`，空 `messages` + `permission_decisions` 恢复同一 turn | **删除**自建相位与 worker，直接透传带版本的 decision |
| 9 | 交互 | `WaitingInteraction` 相位 + 五种 kind 的手工归一化，`agent.rs:895` | `InteractionRequest`/`InteractionResolution`（带 `version`），陈旧解析报 `agent.interaction.stale` | **删除**相位。`architecture_change` 这类产品交互改成 `AgentToolExecution::Interaction` 工具 |
| 10 | 上下文压缩（回合内） | `LiliaCompact` workflow turn 走自建相位（`run_context_compaction_turn`） | profile 的 `context.compaction_service` 在 `AgentContextBuildProtocol` 内联触发，durable transcript 原文不被摘要覆盖 | **删除**自建压缩相位，改为配置 AgentKit 压缩策略 |
| 10b | 上下文压缩（显式换会话） | `context_compaction.rs`：控制模型生成摘要 → `create_compacted_product_session` → `replace_session_binding` | **不提供**。AgentKit 的压缩是回合内的，不换 session | **保留**。这是操作者显式触发的产品工作流，与 10 不是同一件事 |
| 11 | 自动续回合 | `finish_turn` → `finish_and_activate_next`（纯队列出队），`agent.rs:3112`；`auto_turn.rs` 只做模型/档位选择 | 回合内多步自动推进（`max_steps`，默认 8）；跨回合只有 `ProactiveScheduleService::due()` | **保留**队列出队与模型选择，二者都在 turn 之外 |
| 12 | 标题更新 | `title_update.rs`：2 线程池 + generation 去重 + 人工复核 | **不提供**任何标题生成。`AgentSession.title` 只在 create/fork 时可写 | **保留**，改为 Kernel Job（协议 `lilia.agent/title@1`），删除私有线程池 |
| 13 | 事件投影 | `submit_agent_task_turn_observed` 回调 → 粗粒度 `TimelineChanged { cursor }` 整片 refresh | `AgentEventEnvelope { session_id, sequence, meta, event }`，`subscribe_events(session_id, after_sequence)` 增量推送 | **改接**订阅式增量投影，删除整片 refresh |
| 14 | 工具注册 | bespoke wire dispatch | `AgentPluginRegistrar::new(plugin_id, generation).tool(AgentToolDescriptor)`，`Routed` 走 `target_protocol_id`，`Interaction` 交回 loop | **改造**：worktree / architecture / memory / todo / automation 全部注册为工具描述符 |
| 15 | 持久化 | `desktop_pending_turns`、`desktop_quarantined_turns`、`pending_projections`、`agent_session_bindings` | `SessionPersistence`（transcript）、`AgentSessionStore`（checkpoint + 事件流） | **收敛**：审批/交互不再另存 `pending_projections`，改读 AgentKit checkpoint；只保留队列表 |
| 16 | 线程 | 每回合一条 `lilia-native-turn-*`，另有 approval / interaction / title 线程 | 回合本身就是一个 `Task`，由 `TaskPool` 调度 | **删除**全部裸线程，改 Kernel `Jobs` → `HostRuntime::submit_task` |
| 17 | 会话分叉 | `DesktopSessionBranchAnchor`（`Continue`/`Fork` + `source_turn_id`）与 `session_fork` 标志，在 `run_turn_worker` 里编排（`agent.rs:2497`） | `AgentSessionForkRequest.through_turn_id` 按回合边界复制 messages/events，协议 `mutsuki.agent.session/fork@1` | **删除**编排，直接提交 fork 请求；保留 task↔session 绑定替换与 auto-turn 的 `session_fork` 判定 |

## 留在 LiliaCode 的产品语义

以下与 turn 状态机无关，迁移后仍属产品 Feature：

- **worktree 闸门**：`ensure_initial_worktree_ready` 阻止未就绪任务发起回合（`worktree.rs:378`）。
- **worktree 上下文注入**：`worktree_auto_instructions_for_task` 作为 `additionalContext`（`agent.rs:3265`）。
- **hooks**：提交与停止两个时机执行用户/项目/插件 hook（`hooks.rs:430`）。
- **Guide/todo 派发**：`dispatch_next_task_guide` 在 Tool/User/Idle 窗口自动发起 Guide 回合（`todo.rs:674`）。
- **automation 关联**：`DesktopAutomationTurnCorrelation` 在回合终态完成 automation 节点（`agent.rs:2995`）。
- **auto-turn 档位决策**：`apply_automatic_turn_selection` 基于上下文占用选模型（`auto_turn.rs:45`）。
- **任务运行闸门**：`ensure_task_runnable` 的 run block（`product_management.rs:171`）。
- **slash command 本地执行**：不进 Agent 的本地路径（`composer.rs:584`）。
- **载荷适配器**：`tool_consent.rs` 归一化 allow/deny 与可编辑命令，`mcp_elicitation.rs` 解析并校验表单/URL。AgentKit 只认 `PermissionDecision` 与 `InteractionResolution.response`，这两层翻译删不掉。
- **architecture 交互的图应用**：`respond_task_architecture_interaction` 必须先改产品图再恢复回合；AgentKit 不知道这张图。
- **事件投影**：`projection.rs` 把 `AgentEventEnvelope` 翻成产品 timeline/pending 命令，本身不写库。这是有意的适配边界，保留。

## 缺口与补法

| 缺口 | 补法 | 归属 |
|------|------|------|
| AgentKit 无多回合队列 | `feature-agent-session` 保留 `desktop_pending_turns`，只在上一回合终态事件到达后提交下一回合 | LiliaCode |
| AgentKit 无失败重试 API | 沿用"读取 `retryContext` 重建请求 + 新 turn id"，不引入状态机 | LiliaCode |
| AgentKit 无标题生成 | Kernel Job 协议 `lilia.agent/title@1`，结果写回 `AgentSession.title` | LiliaCode |
| AgentKit 无 automation 节点关联 | `DesktopAutomationTurnCorrelation` 留在请求与队列行上，终态时完成 automation 节点 | LiliaCode |
| `AgentSessionCoordinator` 未接入 loop 插件 | 采用 loop 插件路径，不引用 coordinator，避免第二套状态机 | 记为 Mutsuki 待办 |
| 审批 `version` 绑定的是 transcript 长度而非 session 版本 | 恢复决策时原样回传 `PermissionRequest.version`，不自行推导 | 记为 Mutsuki 待办 |

## 拆除顺序上的一个正确性约束

队列 ack 用 `claim_token` 保证所有权：重启后 `prepare_recovery` 会换新 token，陈旧 worker 不能确认已被新进程重投的回合。AgentKit `SessionVersion` 在 claim 之后写入 `claim_epoch = sv:{n}`，ack 可带 `expected_session_version` 做前置校验。不得同时删掉 token 与版本绑定。

## 硬约束

不得保留第二套 turn 状态机。任何"AgentKit 缺这个能力"的结论，只能落在**队列、重试触发、标题、auto-turn 决策、automation 关联、载荷适配**六处；其余一律改为 Agent 插件或直接删除。

## 壳层已接线

`LiliaShell` 按标题调度器同一模式接入回合 job，application 不持有 kernel。

1. **安装回合执行器**  
   `install_title_update_scheduler` 之后、`restore_persisted_turn_queue` 之前安装 `QueuedTurnExecutor`。它只发顶层 `Message::RequestTurnJob` / `RequestApprovalJob` / `RequestInteractionJob`；壳层再 `jobs().submit` 到 `lilia.agent/turn@1` / `approval@1` / `interaction@1`，槽位 `lilia.agent.turn.{task_id}`。`DesktopTurnPort` 在 job 线程回调 `execute_*_job`。未安装执行器时 application 仍会自建私有 `LiliaJobRuntime`，仅测试路径使用。

2. **快照相位不再出现 resolving / finishing**  
   `task_runtime_snapshot().phase` 只投影 `idle` / `starting` / `running` / `waiting_approval` / `waiting_interaction`。`restored_turn_state` 已去掉 `resolving_*` 分支。`DesktopTurnState::Resolving*` 事件仍会发出，UI 继续用事件而不是 snapshot 相位。

3. **时间线改为增量 cursor**  
   回合主路径不再在 `handle_turn_page` 里发 `TimelineChanged { cursor: None }`。观察者按 `AgentEventEnvelope.sequence` 推增量。整片 refresh 只留在取消、隔离恢复等没有增量流的路径。

4. **壳层不再 spawn 回合 / 审批 / 交互线程**  
   `lilia-native-turn-*` / `lilia-native-approval-*` / `lilia-native-interaction-*` 已从 `agent.rs` 删除。生产路径只走内核 job。
