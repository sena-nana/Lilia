# NanaUI Native IDE 架构边界

本文约束 Native Preview 的功能迁移方式，并为后续 IDE 能力保留稳定扩展点。文件树、编辑器、LSP
诊断和 PTY/Terminal 已进入第一阶段纵切；增量语法、完整编辑器能力、远程进程会话真机门禁和调试器仍需独立验收。

## 分层

```text
NanaUI View / Workspace Item
        │ intent + snapshot
        ▼
Native UI model（窗口内选择、焦点、viewport、draft）
        │ typed command / typed event
        ▼
DesktopApplication（产品事实与用例）
        │ ports
        ├── Product / Timeline / Todo / Agent / Automation
        └── DesktopHost（窗口、对话框、剪贴板、Keyring、更新等 OS 能力）
                │
                ▼
        SQLite / Git / AgentKit / Windows
```

- `DesktopApplication` 是 UI 无关的用例边界。项目、任务、会话、审批、自动化运行和持久化
  事实不能由 NanaUI widget 或 Tauri command 私有持有。
- 项目/任务 mutation 通过带 expected revision 和 idempotency key 的 `DesktopProjectPatch` /
  `DesktopTaskPatch` 执行；创建任务时由应用层补齐确定性 conversation，避免未来 Editor、Search
  或 CLI 各自发明一套会话身份与写入顺序。
- 跨项目、Inbox 或父任务的任务移动也是应用用例，而不是 View 拼接写入。应用层预检目标项目、父链、
  目标排序和 expected revision 后只提交一个 Product Core typed aggregate command；repository/SQLite 在
  一个事务内移动根任务及其整个子树、保留子树内部父子关系、同步全部子树绑定 conversation，并写入事件
  与类型化幂等结果。相同 command 重放返回 exact result；任一任务、会话、事件或结果写入失败都必须整体
  回滚，UI 不提供跨实体补偿写路径。
- 任务位置查询不能用 `Option<ProjectId>` 同时表达“全部”和“无项目”。共享查询合同使用互斥的
  `All / Project / Inbox` scope；Workspace 另行持久化 Inbox 选择，`selected_project = None` 不会
  被包装成合成项目。孤立任务仍使用相同 `task:*` Workspace Item，可跨窗口和重启恢复。
- 项目移除属于 Product aggregate mutation，不属于 Workspace 或 View 的级联删除。共享 repository 在一个
  原子提交中归档项目、解除活动任务和活动会话的项目绑定并记录类型化幂等结果；任务 resource identity、
  父子/依赖关系和磁盘工作区保持不变。Workspace 只响应失效事件：项目级 Item 清理，任务 Item 继续存在并
  以 Inbox 选择恢复；任何窗口都不能自行拼接多次写入或删除目录。
- 项目侧栏和任务列表重排使用 NanaUI `ReorderList` 的 moved/before 意图，View 不直接修改本地数组。应用按
  location 与 pinned 状态形成完整排序组，携带逐项 expected revision 调用一个 Product aggregate command；
  repository 在同一原子提交中更新全部 `sort_order`、事件和类型化幂等结果，随后由 Product 事实回读。
  上移/下移与拖拽共用该写路径，搜索结果等不完整集合不能提交排序。
- Workspace 任务摘要保留 Product `parent_id`，但不复制树权威；Native 每次从 snapshot 派生树序，候选
  父项排除自身与后代，最终仍由应用层父链校验。遗留缺失父项或环按根级有界展开，不能让渲染递归失控。
- 跨位置拖放复用 NanaUI `ReorderList` 的 passive destination：source 行可拖动，项目/Inbox 根级与搜索出的
  父任务行不可拖动但可接收释放。NanaUI 只负责阈值、命中和 source/destination 意图；应用层拥有候选过滤、
  业务目标映射和上述 aggregate command 的持久化，不能把 Product 树复制进通用控件。
- NanaUI view 只持有窗口内短生命周期状态，例如当前选中项、焦点、滚动位置、图画布
  viewport 和尚未保存的输入。它通过类型化 command 修改应用事实，并由 snapshot 重绘。
- `DesktopEvent` 是失效通知和实时反馈，不是事实源。订阅队列允许丢弃时，消费者必须用 ID
重新读取 SQLite/应用 snapshot；事件不得成为唯一恢复路径。跨实例/外部 Product 写入通过
`product_events` durable change feed 轮询映射为同一 `DesktopEvent` 合同（源标识
`product-change-feed`），仍不得把 feed 当作事实源。
- `DesktopHost` 只封装 OS 能力。任务、Todo、时间线、Agent 和 Automation 通过专用应用端口
  接入，不能塞入 Host 以绕过领域边界。

## Workspace Item 合同

参考 Zed 将共享状态放入 Entity、将可停靠内容定义为 Workspace Item 的分层思路，NanaUI
不复制 GPUI API，而提供适合自身事件循环的等价合同。每个可停靠业务面板最终应具备：

- 稳定 `item_id` 和 `kind`，用于布局恢复、Agent Debug 和跨窗口引用；
- 标题、图标、焦点目标、是否可关闭/拆分/跨窗口移动/持久化等能力，而不是按页面类型写分支；
- `snapshot()` 只读呈现状态，`handle(intent)` 只产生类型化意图；
- 可选的 `serialize_state`/`restore_state`，只保存 UI 状态，不复制产品事实；
- 生命周期钩子负责订阅和取消后台任务，不能让窗口关闭终止仍属应用层的任务。

当前基础合同已经落在 `lilia-desktop-application`，而不是 Native widget 内：

- 每个窗口创建独立 `DesktopWorkspaceSession`，共享同一个 `DesktopApplication` 产品权威，
  但各自持有选择、pane tree、焦点 pane、活动标签和单调 revision；默认 session 只用于兼容旧调用。
- `WorkspaceItem` 由稳定 ID、可扩展 kind、标题、图标、语义焦点目标、close/split/persist
  capability 和可选 UI 状态组成。`DesktopCommand` 负责 open/activate/close/focus pane/split/
  move/reorder/state update；跨 pane 移动会保留源 pane 邻项并将目标 pane 设为焦点，空 Pane 只能在
  非最后一个且不含 Item 时折叠，命令路由会真实拒绝违反 capability 的操作。
- session 文件只保存 item ID、kind 和可选 UI 状态；任务标题等产品事实在恢复时从 Product Core
  重新读取。旧版只有 `task:*` 裸 ID 的布局会迁移为类型化 restoration；不存在或已归档的项会从
  pane tree 清理。
- Native 主窗口使用独立 session；schema v3 topology 将主 Workspace 与全部辅助 Workspace Window 的
  window/session、完整 Pane/Item state 和物理 geometry 放入一个权威快照，由单一后台 writer 原子发布。
  窗口描述符不再绑定单个 task/item；schema v2 的旧字段读取后会被移除。旧
  `main-workspace-state.json`/`workspace-windows-state.json` 只作为迁移输入，不再参与运行期提交。writer 先发布
  成员 revision，最后以 topology committed revision 作为 release barrier；Agent Debug 只有整体 revision
  追平后才终止进程。真实回放已证明任务选择、活动标签、item descriptor，以及任务窗口的集合、session、
  Item 所有权、布局与几何可跨进程重启恢复。

项目级非任务 surface 现在也经过同一合同，而不是继续作为 Pane 外的页面枚举：Roadmap、Memory 和
Architecture 分别使用稳定的 `project-roadmap`、`project-memory`、`project-architecture` kind，resource
identity 绑定 Product Project，view identity 独立。恢复器从当前 Product Core 重建标题和能力，只保留
选中记录等可选 UI state；项目归档或删除后会清理失效 Item 和 Pane 引用。激活任一 Item 会先由
`DesktopWorkspaceSession` 同步项目选择，再由 Native 选择对应完整业务 renderer；非焦点 Pane 消费按 Item
缓存并由 `DesktopEvent` 失效刷新的只读应用快照。三种 Item 的辅助窗口编辑状态现按 view identity 保存项目、
surface、任务列表、draft、selection 和图形 viewport；窗口消息同时携带 window/item ID，在对应 Item 状态中
执行后再回写序列化选择，不覆盖主窗口选择。Roadmap、Memory 与 Architecture 因此共享同一完整 renderer，
可在窗口 session 间原子转移并随 Product 事件失效刷新。后续文档、Diff 和 Search Item 应复用同一
kind/resource/restorer/renderer 路由，不得再向
`ProjectSurface` 增加只能在主 Pane 生效的旁路。

应用级非任务 surface 使用相同合同但不伪造项目归属。Automation 现以单例
`automation-workspace` kind 和 `application:automations` resource identity 持久化；激活时在焦点 Pane
呈现完整 Canvas、检查器与运行历史，非焦点 Pane 使用随 Automation 事件刷新的真实工作流只读快照。它可在
主窗口 Pane 间或独立辅助窗口移动；辅助窗口复用完整 Automation renderer，动作经 window ID 包装后仍调用
同一应用服务。退出 Automation 只停用当前标签，不删除 Item；重新激活同一标签会恢复应用路由和已选 workflow。
Settings 同样使用 `settings-workspace` / `application:settings` 单例 Item。`activeTab` 和返回来源 Item ID
保存在 Item UI state 中；其完整 renderer 也可迁入辅助窗口，Provider、配额、扩展、远控、桌面集成和数据迁移
仍由原有应用服务持有。通过按钮或
标签进入 Settings 都会记录当前来源，返回时激活仍存在的来源 Item，否则安全回到项目概览。重复打开同一
kind/resource 的 Item 会保留已有序列化状态，不能用工厂生成的空 descriptor 覆盖用户 UI state。

Provider 运行配置遵循同一应用权威边界：默认模型及 OpenAI-compatible/Anthropic endpoint 是可版本化、非 secret
的应用设置，持久化在 Agent runtime 数据库的独立 settings 表；凭据仍只由 Broker 描述符和 OS Keyring 持有。
设置保存以 expected revision 防止多窗口静默覆盖，再原子热应用到共享 `NativeAgentKitRuntime` 并失效缓存 Host；活动
turn 保留已取得的 Host，后续 turn 使用新配置。任务级 model 选择高于应用默认值，且模型切换会同步 Provider capability
map，避免上下文预算继续读取旧模型 key。这为后续 IDE 的 workspace/project/language 分层设置保留了清晰扩展点，UI 不直接
读写环境变量或 provider 私有 payload。

这完成了“合同与恢复”基础。Native 现在已有两类真实 NanaUI 辅助窗口：会话状态窗口读取全部 Product
任务及 Agent phase，可聚焦主窗口任务；任务窗口按 task 创建独立 `DesktopWorkspaceSession`，但从同一
`DesktopApplication` 读取时间线、待处理交互与 Composer，并支持发送/停止、权限、计划、Goal、附件和
审批/提问响应。同一任务重复打开会聚焦既有窗口，不复制产品状态。待处理交互的语义与恢复合同属于应用层：
Agent 投影必须无损保留自定义 interaction payload，`DesktopMcpElicitation` 解析和校验 MCP Form/URL schema，
NanaUI 只拥有控件状态与绘制。这样未来 document/editor 等 Workspace Item 不需要理解 Agent 协议，也不会把
MCP 交互重新耦合到某个窗口；当前仍需真实 MCP server 的 suspended turn E2E 证明响应可以继续原运行。
辅助窗口的 Ready/resize/move/close 生命周期
与主窗口分流，不会覆盖主窗口几何或在关闭时退出应用；任务窗口状态启动时会校验 Product、session、Item、
layout 和 geometry 后再创建 NanaUI 窗口，无效或与主窗口重复拥有的 Item 会被丢弃。主窗口现递归渲染整个 Pane Tree，并按持久化 axis/
ratio 同时展示多个 NanaUI `Tabs` 叶节点；每个叶节点可焦点、水平/垂直拆分、移动 Item 和折叠空 Pane。
活动 Pane 持有完整交互式任务 surface，非活动任务 Pane 持有按 task 缓存的真实 `TaskSessionView` 只读快照，
后台事件到达后重新读取应用事实而不是复制时间线权威。项目概览只取消激活而保留标签，激活任务标签会按
Item 身份切换 Product 项目/任务，关闭活动标签会通过 capability 门禁移除 Item 并选择 pane 邻项。NanaUI
`Dock` 的 center 明确不能进入 dock tabs，它负责资源/检查器等侧边或底部面板，不能被误用为 Editor
Pane Tab。真实 Agent Debug 已覆盖 Pane 拆分、焦点、移动、空 Pane 折叠、同时有内容的双 Pane 场景，以及
NanaUI 原生 `split_pane` 分隔条调整与比例持久化。`DesktopWorkspaceSession::transfer_item_to` 会在同一
`DesktopApplication` 的两个 session 间按稳定锁序原子校验并转移 Item；源移除、目标插入、选择同步和两端
revision 任一步失败都不会提交。Native 的“新窗口”创建不同 view instance 并共享 task resource，“移至窗口/
移回主窗口”则保持同一 Item ID 和序列化视图状态。真实进程重启已恢复后一条路径的窗口 session、原 Item
所有权与物理几何，再成功移回主窗口。磁盘回放还直接断言同一 topology 中主窗口不含该 Item、目标窗口同时
持有 descriptor/布局/geometry，消除了两个状态文件间的撕裂窗口。NanaUI `Tabs::on_reorder` 现以真实
鼠标/触摸阈值、插入指示线和应用持有的 before-item 合同完成同一 Pane 重排；Native 直接复用
`MoveWorkspaceItem`，因此真实 pane item 顺序与 topology revision 同步持久化。多个 Pane 的 Tabs 现加入同一
`TabDragGroup`：NanaUI 用真实绘制矩形解析 source/target/before，Native 仍只解释 Pane ID 并执行同一命令，
完成同一主窗口内跨 Pane 的唯一所有权转移。`TabDragSurface` 又用每个 HostedWindow 的物理 origin 与 DPI scale
统一屏幕坐标，并允许目标窗口 relay move/release。辅助 Workspace Window 现在渲染真实多项 Tabs，并接收
来自主窗口或其它窗口的外部 drop；main→window、window→main 和 window→window 都调用两个 session 的原子
`transfer_item_to`。非空源窗口保留并切换到真实邻项，最后一个 Item 离开时才关闭；schema v3 将完整顺序、
活动项、resource/view identity、revision 与 geometry 一起恢复。活动项的 task 绑定已改为可选，窗口所有权不再
假设 `WorkspaceItemKind == task`。辅助窗口现在也递归渲染完整 Pane Tree：每个叶节点拥有独立 Tabs、焦点、
水平/垂直拆分、跨 Pane/窗口拖放、关闭与空 Pane 折叠；每个 split 使用同一持久化 ratio 和 NanaUI 原生
`split_pane` 控制器。焦点 Pane 提供完整 Composer、附件、审批与提问交互，非焦点任务 Pane 从按 task 缓存并
随 `DesktopEvent` 刷新的真实 `TaskSessionView` 渲染时间线、Markdown 和状态摘要，不再显示假内容或空白占位。
Agent Debug 以结构化 `workspaceWindows` 返回每个窗口的 session、Pane、Split、活动 Pane/Item 和 geometry，
并通过窗口命名空间稳定目标驱动真实拆分、移动、比例调整与重启恢复。

Dock 现已进入同一应用命令边界，而不再是 Native View 私有的显隐布尔值。`PanelLayoutSnapshot` 为 Left、
Right、Bottom 保存多个 `PanelState`；同一 slot 只允许一个可见活动面板，`DesktopCommand` 负责激活、隐藏和
调整 extent，旧快照用幂等 `ensure_panel` 补齐默认面板而不覆盖已有尺寸。任务检查器与 Coding Tools 已共享
Right slot，选择任务激活 Task Inspector，工具入口激活 Coding Tools；NanaUI `WorkspaceController` 只负责
实际 resize/动画，结束时把 extent 通过应用命令写回 topology。Coding Tools 又把 AgentKit Git、Computer Use
和 Code Index 响应归一化为 Lilia 自己的类型化快照，原始 JSON 不进入 View；搜索结果通过
`DesktopMemoryService` 写项目 Memory。只读 Git working-tree/staged diff 也沿同一共享服务取得并可切换；小 patch 直接内联，大 patch
从 AgentKit 的不可变资源引用恢复完整内容，Native 只绘制有界预览。stage/commit 仍须复用 Mutsuki 的审批、
完整 worktree state 与 write context，不由 View 或宿主直接执行 `git`。Mutsuki owner 工作树现有
`GitWorktreeState` 候选实现：canonical worktree 身份、HEAD/index/working files 的重启稳定摘要、同 service
写入串行栅栏，以及外部 HEAD/index/worktree 变化的 typed conflict；macOS 定向测试与严格 Clippy 已通过，
但尚未发布为 Lilia 可固定的完整 revision，Native 因而仍不开放 Git 写入口。
LSP definition 也保持相同分层：Mutsuki 共享 session 返回的三种合法位置形态先在 integration 层归一，
`DesktopApplication` 再以 source Buffer revision、canonical project root 和 DocumentStore 解析目标；View 只负责从
NanaUI retained cursor 发起后台查询并激活返回的 Item/range。单目标直接跳转，多目标保留真实选择列表；目标已在
其它 Workspace Window 时聚焦既有 owner，不复制同一 Item。范围定位使用 O(1) cursor/selection，不再逐动作猜光标。
`DesktopHost::OpenTerminal` 继续表示启动工作区外部终端，与内嵌 Terminal Item 是两个不同动作。内嵌实现由
`DesktopApplication` 持有 `portable-pty` 会话、独立阻塞 reader/waiter、`vt100` 屏幕与有界 scrollback；View
只消费类型化行、样式、光标、尺寸和进程终态。启动目录只能来自活动项目根或任务 worktree，Workspace 恢复只重建
`Restored` 终态标签，绝不自动重放命令。关闭 Item 仅释放视图，显式“终止”才杀进程；仍在运行但已关标签的会话可从
Coding Tools 重新打开。终端可发送 Ctrl+C/Ctrl+D 原始控制字节，并通过宿主剪贴板复制当前类型化屏幕文本；
session 登记栅栏保证极短命令的输出/退出事件不会早于 session 可见性。
Android legacy `process_session` 现在只作为这套 Terminal 服务的远控适配：`spawn` 在 task worktree/项目根内创建
真实 PTY，`write_stdin` 和 `kill` 只操作该 task 当前登记的 session，`tasks.get.runtime.processSessionId` 仅在进程
仍活动时返回。客户端传入的 cwd 必须与服务端解析出的任务工作区相同；若传 permission profile，仅接受旧协议的
`:workspace` 兼容标记。服务端还会按 Product 任务状态和依赖图拒绝不可运行任务。该 PTY 是已配对可信设备触发的
用户 Shell，不是文件系统沙箱，命令仍具有当前用户权限；能力协商在实现落地后返回
`supportsProcessSession=true`。进程不会随远控或 Workspace topology 持久化、重启或自动重放。

Composer 与可执行 turn 队列现在也遵循同一权威边界：按 task 的 revisioned 草稿与完整
`DesktopTurnRequest` 分别持久化在 Preview 独立 SQLite 中，UI 只缓存当前快照。活动 turn 期间的新发送先分配
跨进程稳定 UUID，再入持久 FIFO；启动必须先从 Product 投影恢复审批/提问/计划活动态，再恢复其后的队列，不能让
排队项越过暂停 turn。显式取消会删除该 task 的可执行队列并发布终态，不触发下一项。Tauri 的 Todo Guide
`pending → queued → sent` 与 high/normal/low 调度窗口现已迁入应用层：tool 窗口只取 high，user 窗口只取
normal，idle 窗口按 high→normal→low 选择，并复用发送时的 typed attachments 与 Composer 设置。三类仓储现共享
一条受控 SQLite 连接：空闲发送以单事务 clear+enqueue，运行中发送以单事务 create Guide+clear，等待交互的
即时 Guide 还会在同一事务完成 FIFO 选择、queued 状态与 enqueue。队列以每 task 唯一 `queued/claimed` 行、
随机 token 和进程 epoch 约束 worker 所有权；terminal ack 与 next claim 在同一事务提交，stale claim 重启时
保留稳定 turn ID/full request 并换发所有权。下一阶段仍须证明 Provider/工具外部副作用幂等，隔离单个损坏队列行，
并覆盖 DB busy/corrupt/磁盘满；不能把应用队列不丢失表述成全局恰好一次。

斜杠命令同样归应用层所有：服务从内置定义及项目 `.lilia/commands/*.md` 生成有界、排序、类型化搜索结果，Native
只负责展示候选与将选择写回 revisioned Composer。只有无附件和对话引用的单一精确 token 会执行；命令以稳定事件 ID 写入
Product 时间线后再清空草稿，因此跨 Product/legacy SQLite 边界失败时可用同一修订幂等重试，而不会伪造 Agent turn。

对话与项目上下文也遵循这个边界。`DesktopApplication` 跨 Product 项目搜索可引用 task，并从任务所属项目的
`ProjectContext` 根执行有界文件/目录搜索；路径浏览只返回直接子项，普通搜索遵守 `.gitignore`、隐藏目录和固定
排除目录，任何 `..` 逃逸都会在服务层拒绝。Native 与 Tauri 只渲染同一类型化结果；选择通过 expected revision
同时更新正文与引用集合，`ChatConversationReference`/`ChatAttachment` 随 Composer、Todo Guide 和 durable
`DesktopTurnRequest` 持久化，并以结构化 metadata 交给 Agent。字符串引用只是当前模型输入的兼容投影，不是产品权威。

上下文用量也由应用层从绑定 AgentKit session 的持久化事件恢复，并随 `DesktopTaskSessionSnapshot` 提供给两套
宿主。`ContextUsageUpdated` 能提供真实 input、reserved 与 limit 时才计算百分比；当前 AgentKit 只产生普通
`Usage` 时，Native 仅显示实际 total tokens，明确不从模型名称或静态能力表推测上下文上限。Mutsuki 已有内部
`TranscriptContextWindow`，普通回合会在不修改完整 durable transcript 的前提下生成确定性模型输入窗口。用户主动
“压缩上下文”则是另一条应用层控制事务：共享 `DesktopApplication` 先从当前绑定 session 获取有界转录，调用真实
control model 生成持久摘要，再创建含摘要与完成事件的新 AgentKit session；只有新 session 持久化和投影均成功后才
原子替换 Product binding，失败或取消继续绑定旧 session。Tauri、Native 主窗口与辅助任务窗口均调用同一事务，workflow
不再通过 profile 后缀伪装成普通空消息 turn。Mutsuki owner 工作树已让高层
`ContextCompactionCoordinator` 通过 profile policy 和正常 ModelGateway 进入实时运行链路，并为失败 fallback、
usage/cost、相同 snapshot 缓存及 resume 时机建立合同；但它尚未发布并固定到 Lilia，产品 profile 也还没有消费
该 policy，所以不能用当前手动产品动作或 owner 本地候选冒充应用侧已完成。

`WorkspaceItemId` 现在只标识可重复的 view instance，`WorkspaceResourceId` 独立标识 task、document 或
buffer 等稳定资源；恢复数据同时保存两者，旧版只含 Item ID 的 schema v1 状态会安全补齐资源身份并回写。
当前 task 的默认视图仍使用同值的 instance/resource ID，但应用层合同与恢复测试已经允许多个 view instance
共享同一 task resource，并在不同 Pane 中独立保存布局。后续 Editor 以 canonical document/buffer ID 作为
resource，多个 view instance 共享同一 revisioned buffer，各自保存 selection、scroll、fold 和焦点状态；
文档生命周期和脏状态继续由共享 `DocumentStore` 拥有。

## Extension Package 合同

为后续 IDE 语言、调试器和工具扩展预留的是可版本化的声明式 package 边界，不是 UI 私有插件列表或任意
进程内动态库。当前 `lilia-plugin.json` 只声明 Skill、Hook 和 MCP contribution；安装器负责复制到受管目录、
限制文件数量/体积、拒绝路径逃逸与符号链接、计算整包 SHA-256，并以 revision-safe 注册表控制启停。UI 只发送
install/toggle/delete 意图，运行时 catalog、Hook source 和 MCP namespace 均由 `DesktopApplication` 重建。

- Skill、Hook、MCP 保持各自应用层所有权，不因打包在一起而获得跨边界直接写入能力；
- MCP executable 必须位于 package 内，HTTP 地址不得携带 inline credentials，secret 值只绑定到 OS Keyring；
- package 默认停用，启用前复核 manifest 与整包 digest；删除会先撤销运行时贡献，再事务清理 package MCP
  的 Keyring namespace；
- 当前 package 不允许加载 Rust/C/C++ DLL。未来需要原生语言服务器、调试适配器或用户可执行扩展时，沿用
  manifest + process deployment + capability grant + generation drain-and-swap 边界，不能把不受信代码注入
  NanaUI/WGPU 主进程；
- Editor、Diff、Terminal、Search 等未来 Workspace Item 只消费扩展服务产生的类型化结果，不能依赖某个插件
  自行操作 Pane Tree、SQLite 或 Host 凭据。

任务时间线不再要求窗口加载完整历史。`TimelineProjectionRepository` 以
`(sequence, eventId)` 作为稳定 keyset cursor，SQLite 在仓储边界倒序限量读取后恢复为时间顺序；
`DesktopApplication` 返回类型化页面和读取错误。各窗口只持有自己的已展开页，并在 `DesktopEvent`
刷新时按事件 ID 合并最新页，因此不会复制权威事实，也不会因后台增量事件折叠用户已加载的历史。
列表虚拟化、滚动锚点和千条时间线 p95 仍属于渲染层后续门禁。

双宿主等价验证也只读取这一应用权威。`tests/equivalence/p0-v1.json` 是版本化、无凭据且不访问网络的最小
corpus，`equivalence_fixture` 示例只能通过 `DesktopApplication` 用例写入两个空 home；开发态 Tauri command
和 Native JSONL 协议调用同一个归一化快照实现。schema v8 快照按 fixture 前缀过滤并稳定排序，排除随机
Todo/Milestone/Memory/Automation/node ID、节点位置、当前时间和绝对路径；Composer 与 Memory 正文只输出字节数
或 SHA-256，Goal 目标、Todo 正文和 Automation config 只输出 SHA-256。Goal/Todo 比较任务归属、规范化状态、
优先级、顺序和哈希事实；Roadmap 比较项目、顺序、状态、截止日和已排序任务关联，Memory 设置比较归一化值，
Automation 比较名称、scope、语义节点/边、发布与启用状态；Plugin 比较包 ID、版本、启用/运行时状态、
整包 SHA-256 与 contribution 计数，不记录绝对路径或凭据。等价 fixture 会话让 Tauri 临时
选择 Product domain 并使用隔离的内存 Memory 设置；普通开发与正式构建各自使用本进程数据目录，
不要求与另一宿主目录双向兼容。
`yarn verify:ui-equivalence:p0` 先比较业务快照，再独立执行 GPU 截图门禁；锁屏、黑屏或不含 Native 中性色
surface 时必须保持 blocked，不能用 UI 文案匹配或历史截图代替。

Automation Canvas 是第一个按应用级合同实现的复杂 Item。未来 Editor、Diff、Terminal 和 Search 等 Item
可以复用 Pane、焦点、命令路由和持久化，不需要改写 Shell。

## Automation 作为首个纵切

- workflow draft、发布版本、run 和 node state 由 `DesktopAutomationService`/`AutomationStore`
  持有；Tauri 与 Native 调用同一服务语义。
- `try_begin_run` 必须在一个 SQLite 事务中创建 run 和全部 node state，并原子拒绝同 workflow
  的第二个 active run。
- 删除 workflow 必须是单事务；存在 active run 时明确拒绝，不能留下半删除关系。
- Tool/Agent 副作用通过带 `run_id + node_id` 幂等键的端口执行；Canvas 不直接写任务、Todo、
  时间线或 ChatStore。
- Canvas model 只包含节点/边、选择与 viewport；执行状态来自 run snapshot，不能把 UI 动画状态
  反写为领域状态。
- Native Shell 不再用 Pane 外的 `automations_open` 页面绕过 Workspace。应用层恢复器校验
  `automation-workspace` kind 与 `application:automations` resource，Native 由活动 Item 推导路由；完整 renderer
  始终位于 Pane Tree 内，因此 Automation 与任务、Roadmap、Memory、Architecture 可以真实共存并通过标签切换。
- 当前纵切已让 Tauri command 与 Native Preview 共用持久化服务；Native Canvas 的创建、发布、
  启停、删除、节点添加/拖动、端口连线和节点/边删除都回写 workflow draft。UI 无关执行器已
  覆盖 Trigger、Logic、Tool、Human 和 Agent；Agent 启动采用 prepare → 持久化 waiting state →
  activate 两阶段协议，避免快速完成先于 correlation 落库。Native 已接入手动运行、历史、节点
  状态和人工恢复；Tauri 的手动/信号触发与人工恢复也调用同一执行器，旧 SQL 图执行只保留为
  已存在运行的启动恢复兜底。

## 单一服务权威

- 每个桌面进程只能 bootstrap 一个 `ServiceAuthority`。Tauri 的 `EmbeddedProductCore`、
  Native AgentKit、Automation、Memory 和 Roadmap 都由同一个 `DesktopApplication` 派生，不能
  各自打开第二个 writer 或第二套 Runtime。
- Native 新数据目录中，带项目/任务关系的 Memory 与 Roadmap 表扩展同一个 `product.db`，外键直接
  指向 Product Core 权威表；不得在新的兼容库中复制项目/任务行。现有 Tauri 仅为读取旧数据而显式
  选择 legacy domain database，正式迁移由只读导入计划处理。
- Tauri command 是兼容现有前端合同的薄适配器；`DesktopEvent` 由宿主桥接为旧事件名，事件到达
  后仍重新读取共享 snapshot。Tauri `DesktopHost` 已真实适配窗口、文件对话框、文本剪贴板、
  Keyring、路径和 HTTP(S) 外链，其余宿主能力在接入前必须返回明确不可用。
- Architecture 的 graph/history/rollback 也由同一应用边界持有：request ID 重放必须返回原结果，
  expected version 冲突必须在事务写入前失败；GraphCanvas 的选择、viewport 和临时节点位置仍是 UI
  状态，不反写架构产品事实。Tauri 与 Native 都只能通过该服务读写版本历史。
- 启动恢复先读取共享 run/detail 并用共享状态机终止已丢失的 Agent worker；只对无法由新合同
  识别的历史行使用旧 SQL 恢复逻辑。

## 线程与恢复

- NanaUI/Winit 事件循环只处理 UI 更新。后台执行通过 `EventLoopProxy` 发送轻量消息，消息处理
  后重新读取应用 snapshot；不得从后台线程直接操作 widget。
- SQLite、Git、Keyring、网络和 AgentKit 不在绘制回调中运行。每个长操作必须有 operation ID，
  旧完成消息不能结束较新的操作。
- Git clone 已按此边界落地：URL/目标命名和 Git 子进程属于共享应用服务，Tauri command 只是薄适配器；
  可克隆 operation handle 暴露 `snapshot/cancel/wait`，快照以 sequence 发布单调进度和终态。Native 监控线程
  只把新 sequence 与 operation ID 通过 `EventLoopProxy` 发回 view，只有成功工作树才进入 Product Core。HTTP
  内联凭据被拒绝，原始子进程输出不进入日志；Windows Job Object 与进程树终止负责真实取消，失败/取消只
  清理服务为本次操作新建的 reservation。view 不持有 child process。未来索引、搜索、下载等 IDE 长操作复用
  同一后台 operation 合同，而不是各自在 widget 中建立线程与取消状态。
- 进程恢复以持久化状态为准：运行关联、等待用户/Agent 的 correlation 和外部副作用幂等键必须
  可重建。仅存在内存 callback 的能力不算 Native 对齐。

## 富文本与未来文档面

- NanaUI `NativeMarkdown` 是只读富文本的公共解析/布局边界；Lilia 时间线只缓存原始 Markdown、解析文档、
  完整复制文本和结构统计，不持有 HTML、DOM 或 provider 私有 payload。
- GFM 表格必须保留列、对齐与单元格 span，使用 Iced/WGPU 原生网格呈现；超宽表格在块内滚动，不能改变
  Workspace Pane 的所有权或尺寸合同。
- 块内选择由 NanaUI `SelectableRichText` 管理，完整文档复制由 `NativeMarkdown::plain_text` 交给
  `DesktopHost::WriteClipboardText`。因此选择绘制、应用消息和 OS 权限仍是三个独立边界。
- Markdown 图片继续遵循 owner/app 分层：NanaUI 只提供 `view_with_images` 的结构化注入和 ImageViewer 呈现；
  Lilia 拥有 URL/file/data 获取策略、有界解码、缓存、失败状态和点击预览。网络、文件权限或 Product 时间线事实
  不进入 NanaUI。总 decoded/GPU 内存与复杂 SVG 仍需独立系统门禁。
- 可编辑 IDE 文档继续使用 revisioned `DocumentStore`/buffer，不复用 Markdown 只读状态；open/edit/save/
  冲突与 `document-editor` surface 已接入。真实 LSP 诊断通过应用层 Document↔LSP 绑定、version fence 和
  DiagnosticStore 进入 Native 问题列表，不由 view 持有语言服务器；问题行会把应用层 byte offsets 转为当前
  编辑器范围并选中真实文本，选择本身不推进 Buffer revision。预览、Diff、Hover 和编辑器内诊断装饰
  可继续消费相同富文本原语；增量语法和完整撤销仍待补齐。

## 后续 IDE 扩展顺序

1. ~~复用已拆分的 resource/view identity、`DocumentStore`、revisioned buffer 和 `LanguageRegistry`，接入可编辑文本 surface，并由 `WorkspaceItemKind` 选择内容 renderer。~~ 已落地 `document-editor` / DocumentStore open-edit-save-冲突 与 Native `HostedTextarea`；项目文件树 + watcher（`project_files` / Left Dock）亦已落地。
2. ~~将真实 LSP 文档诊断作为应用服务接入，不让 Editor view 直接拥有进程或数据库。~~ 已落地共享
   AgentKit LSP、revision/version fence、UTF-16 range 映射与 Native 问题列表；增量语法高亮和 definition
   导航也已消费 NanaUI retained editor API，仍需补齐编辑器内诊断装饰。共享 Code Index 的工作区文本搜索已经接入 Native Coding
   Tools，结果现在可直接打开受项目根约束的真实 `document-editor` Item，并按 Code Index byte range 转换后
   选中实际文本；同文件多结果使用 path/range 唯一目标，并通过 `HostedUiCommand::Focus` 把焦点移交到真实窗口。
   同一索引的 Text/Symbol mode 已由 Native 明确切换；更大规模的多根工作区仍需继续补齐性能与恢复模型。
   共享 Git 的 working-tree/staged diff 已进入 Coding Tools，只展示真实有界预览；
   stage/unstage/commit 与冲突工作流仍待带审批和 HEAD 栅栏的应用命令承接。
3. ~~PTY/Terminal 作为独立 Host 服务和 Workspace Item；关闭面板与终止进程必须是两个动作。~~ 已落地
   Host-owned 跨平台 PTY、ANSI 屏幕、行输入、Ctrl+C/Ctrl+D、输出复制、resize/scrollback/terminate、可移动
   `terminal` Item 和安全终态恢复；Android legacy `process_session` 已映射到 task-scoped PTY，并带服务端
   cwd、活动 session 和任务依赖门禁；后续补齐完整字符级键盘协议、鼠标/链接/选择、任务运行器、真实 Android/LAN 回放与
   Windows ConPTY 门禁。
4. 在现有任意 Workspace Window topology 上增加协作和远程工作区；继续用单一 topology writer 承载
   所有权转换，不能重新拆成按窗口独立提交的状态文件。

NanaUI 已发布 `ActionRegistry`/`Keymap`/`CommandPalette`、`TreeView`、capability-driven `PaneTree`/`PaneChrome`、
`HostedTextareaState` O(1) cursor/selection、增量 syntax highlighter、HostedBrowser 和 HostedWindow PNG capture，并通过
all-features、no-default-features、Gallery 功能测试、dark/light 离屏 WGPU 快照及相关 Windows 交叉检查。Lilia 固定
到完整 revision `1965ce627a5245ea05e5383f288abca8a0c150e3`；HostedBrowser/capture 与 retained editor 能力已消费，
ActionRegistry/KeyContext/Keymap/CommandPalette、TreeView 与 PaneTree 也已在产品路径接入；主/辅助 Workspace 的
递归 topology traversal 与比例 fallback 统一由 PaneTree 承担，Lilia 继续拥有 Pane 内容、可调整 split controller 和消息。
主窗口与辅助窗口的活动/非活动 Pane 已统一消费 PaneChrome；辅助窗口的项目身份、移回主窗与关闭窗口动作独立位于 PaneTree 之外，
文档、终端、应用页、项目页与任务页共享同一套标签、分栏、移动和关闭生命周期。Native 业务动作继续由 Lilia 注册和执行，NanaUI
只拥有上下文筛选、快捷键解析、搜索与键盘 Picker；`KeymapLayer` 在主/辅助 Workspace Window 根层只捕获当前上下文
真实匹配的命令，未绑定按键继续交给文本输入与 IME。关闭 Picker 会把焦点还给来源 Document Item。

Zed 参考：

- GPUI ownership/view/element 分层：<https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md>
- Workspace `Item`/`SerializableItem` 合同：
  <https://github.com/zed-industries/zed/blob/main/crates/workspace/src/item.rs>
