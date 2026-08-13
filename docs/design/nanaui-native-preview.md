# LiliaCode Native Preview

后续 IDE 扩展的所有权、Workspace Item、线程与恢复边界见
[`nanaui-native-ide-architecture.md`](./nanaui-native-ide-architecture.md)。

LiliaCode Native Preview 是与现有 Tauri/Vue 桌面端**独立**的 Rust 原生产品线，完成后替代
Tauri，而不是长期双端数据共存。它以 NanaUI 作为窗口、Workspace、Dock 和基础控件层，通过
共享 `DesktopApplication` 消费产品、存储与 Native AgentKit 合同；Preview 使用独立数据目录，
不为正式 LiliaCode/Tauri 目录做兼容、自动迁移或双写。

## 运行时边界

```text
NanaUI HostedProgram
        │
        ├── lilia-desktop-application ── Lilia service / storage / AgentKit
        │
        └── DesktopHost ── window / dialog / tray / keyring / updater / single-instance
```

`lilia-desktop-application` 不依赖 Tauri、Vue、Iced 或 NanaUI。迁移期 Tauri command
可薄调用该层（单写共享权威），Native Preview 则直接调用类型化服务，并将后台
`DesktopEvent` 转发给 NanaUI 事件循环。两端进程各自持有独立 home；不要求、也不实现
Tauri↔Native 数据双向同步。

Windows 发布入口拆成薄启动器 `lilia-native-preview.exe` 与同目录
`lilia_native_host.dll`。启动器只负责单实例提示判断、立即显示轻量 Win32 启动窗口并加载
Host；现有带随机 token 的文件锁/环回 IPC 仍由 Host 作为单实例事实源。Host 创建第一个 NanaUI/WGPU
加载帧后关闭启动窗口，再在同一事件循环恢复 `DesktopApplication`，不会创建第二套 GPU Device/Queue。
安装、更新、发布排除扫描和体积统计必须同时包含 EXE 与 DLL，缺少任一文件均视为无效产物。

## IAB 确认的交互基线

2026-08-09 在开发态真实 LiliaCode 页面确认以下结构，Native Preview 按语义复现，
不复制 DOM 或 CSS：

- 左侧只保留一层项目/任务导航，设置分类进入设置态后复用同一区域；
- 任务主区以时间线为最高层级，Composer 固定在底部；
- 权限、提问和计划确认以内联待处理面板占据 Composer 区，不另开假对话框；
- IAB、架构图、工作区工具和 Debug 属于可折叠的右侧检查器；
- 所有可见入口必须连接真实状态；未迁移能力不显示占位按钮。

## 身份与数据

预览期使用独立名称、可执行文件、配置目录、SQLite、Keyring 命名空间、单实例标识、
CLI 和更新通道。开发阶段不必做与正式 Tauri 目录的双向兼容工程。旧版数据只能由用户
显式导入：导入先生成计划，再复制到原生目录，旧目录始终只读；凭据复制需要单独确认。
正式切换后原生数据目录成为事实源，旧 Tauri 版可保留一个发布周期作为回滚资产。

当前 Preview 使用 `liliacode.native-preview`、`LiliaCode Native Preview` 和
`%LOCALAPPDATA%/LiliaCodeNativePreview`；也可以用 `LILIA_NATIVE_PREVIEW_HOME`
指定隔离测试目录。开发入口是 `yarn native:dev`，基础门禁是
`yarn verify:native-desktop`。

Windows Preview 发布使用 `yarn native:release:windows -Tag native-preview-v<version>`。
发布构建必须提供独立的 `LILIA_NATIVE_UPDATER_PUBKEY`、
`TAURI_SIGNING_PRIVATE_KEY`（或 `TAURI_SIGNING_PRIVATE_KEY_PATH`）和
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`；GitHub workflow 使用对应的 Native Preview
变量/Secret，先创建草稿版本并运行安装冒烟，只有显式 `promote` 才发布版本并更新浮动通道。

旧数据导入不会在首启时静默执行。用户可以在 Native 设置的“数据迁移”页选择旧版
home，先查看逐项计划，再单独决定是否复制凭据并执行；CLI 也保留相同的两阶段合同：

```powershell
cargo run --locked -p lilia-native-preview -- import plan --source <旧版 home> --output <plan.json>
cargo run --locked -p lilia-native-preview -- import execute --plan <plan.json> --credentials deny
```

计划只接受产品、投影、Agent runtime、旧版 SQLite 与凭据五类白名单项。执行前要求
旧版持有并释放真实 `db/writer.lock`，源目录以只读方式打开；数据库通过 SQLite
online backup 复制到内部 `create_new` staging，检查 schema 上限和
`integrity_check` 后才原子发布。拒绝凭据时不会调用 Host；确认凭据由 Windows Keyring Host 在源
registry 生成并复核的显式 key manifest 内逐项复制，密钥材料始终只在
Credential Manager 中流转。目标已有相同 secret 时幂等跳过，不同 secret 或源 secret 缺失会报告
失败且绝不覆盖目标；测试使用唯一临时命名空间并在结束后清理。

现有 Tauri 宿主启动时也持有同一 writer lock，因此运行中的旧版会阻止导入，且
导入失败不会修改旧库。目标数据库、WAL/SHM 或 staging 冲突均会保留可重试报告，
清理失败会列出残留路径。

设置页不会热替换运行中的 SQLite。数据库先写入 Preview home 下的隔离暂存区；只有
报告完整成功时才生成 pending manifest。用户点击重启时，独立辅助进程等待当前
Native writer lock 释放；用户直接关闭后再次启动时，主进程也会在取得单实例所有权、
bootstrap SQLite 之前处理同一 pending。两条路径都会再次确认目标库没有用户数据，复核
每个暂存库的长度、SHA-256 和 `integrity_check`。激活前的空库及 sidecar 会移动到
`import-backups/<plan-id>/db/`，激活失败则回滚；已有 Native 用户数据时整个激活被拒绝。

## 当前实现范围

- NanaUI 原生窗口已连接真实项目、任务与权威 AgentKit SQLite 投影；任务区显示有序
  时间线、artifact/todo/pending 事实，并由 `DesktopEvent` 提示后重新读取快照。时间线首次只从
  Product projection SQLite 读取最新 100 条，主窗口与独立任务窗口都可使用稳定
  `(sequence, eventId)` 排他游标加载更早记录；实时刷新会按事件 ID 合并并保留已展开历史，读取失败
  保持现有页面并给出可重试错误。项目和任务的
  新建、编辑、固定、状态/优先级、搜索、归档与恢复通过 UI 无关的 revision/idempotency 用例写入
  Product Core；任务创建会在同一应用边界幂等补齐 conversation，Native view 不直接写数据库。
  项目工作区可手工编辑或通过类型化 `DesktopHost::FileDialog` 选择目录；取消不改值，清除与失败状态
  留在项目编辑器内，不污染全局错误。项目排序通过完整 pinned 分组和逐项 expected revision 的 Product
  aggregate command 持久化；SQLite 在一个事务内更新全部 `sortOrder`、事件和类型化幂等结果，中途失败
  会整体回滚，相同顺序不会增加 revision；
  Git clone 也已抽成 Tauri/Native 共用服务；operation handle 提供有序进度快照、取消和等待，Windows
  通过 Job Object 与进程树终止收口 Git 及其后代。服务只预留本次新建的目标目录，失败或取消只清理该
  reservation，不会触碰既有同级目录；Native 在后台操作成功后才创建 Product Core 项目。内联 HTTP
  凭据和原始 Git 输出不会进入进程日志。任务排序使用相同的完整 pinned 分组聚合事务，Native 未搜索列表
  通过 NanaUI moved/before 拖拽与键盘上移/下移共用该写路径。跨位置移动已收口为 Product Core typed
  aggregate command：应用层预检目标项目、父链和 revision 后只提交一次；repository/SQLite 在一个事务内
  移动根任务及整个任务子树、保持子树内部父子关系、同步全部子树绑定 conversation，并记录事件与类型化
  幂等结果。相同命令重放返回 exact result，任一任务、会话、事件或结果写入失败都会整体回滚。
  NanaUI `ReorderList` 的 passive destination 可让不可拖动的项目、Inbox 根级和搜索父任务行接收释放；Native
  只将 source/destination 意图交给应用层，候选过滤（排除自身、后代和当前父项）、目标映射和持久化仍由
  应用层持有。
  `TaskQuery` 以 All/Project/Inbox 互斥 scope 消除 `None` 歧义；Workspace 显式保存 Inbox 选择，无项目
  任务可创建、搜索、排序、归档、恢复和打开持久化 Tab。Native 目标位置操作会保持原任务 Workspace
  Tab 聚焦，不复制产品事实。任务列表直接消费 Product `parent_id` 并按安全树序呈现；父任务候选排除
  自身和后代，支持重挂载与恢复根级，遗留缺失父项或环不会阻塞渲染。Tauri 的任务排序与移动 command
  也已改为同一 `DesktopApplication` 用例的薄适配器，前端刷新共享 Product 权威结果，不再维护第二套写路径。
- Native 主窗口递归渲染持久化 Pane Tree：水平/垂直节点按已保存比例布局，每个叶 Pane 有独立 Tabs、焦点、
  拆分、跨 Pane 移动和空 Pane 关闭操作。NanaUI 原生 `split_pane` 分隔条会更新应用层 ratio 并随窗口 session
  原子持久化。活动 Pane 提供完整任务交互；非活动任务 Pane 从按 task 缓存的 `TaskSessionView` 显示真实
  artifact/Todo/pending 与最近时间线摘要，后台事件刷新时仍回读应用事实。Workspace Item 已拆分可重复的
  view instance ID 与稳定 resource ID，旧恢复状态会迁移补齐。共享应用层可在两个窗口 session 间原子转移
  同一 Item；Native 的“新窗口”创建独立 view instance，“移至窗口/移回主窗口”保留原 Item ID、序列化状态、
  resource、选择和 revision。schema v3 topology 现以单一原子快照持久化主 Workspace 与辅助 Workspace
  Window 集合的 window/session、完整 Pane/Item 布局和物理 geometry；窗口描述符不再绑定单个 task/item，
  schema v2 读取后自动移除旧字段，旧两文件仅作迁移输入。启动时校验后恢复真实 NanaUI 窗口，
  强制结束并重启已证明 Item 所有权不会回流或重复。同 Pane 已消费 NanaUI
  `Tabs::on_reorder` 的真实鼠标/触摸拖拽合同，并通过同一 `MoveWorkspaceItem` 命令持久化真实 Item 顺序；
  同一窗口的 Pane Tabs 还共享 `TabDragGroup`，以真实绘制矩形解析跨 strip 释放并转移唯一 Item 所有权。
  `TabDragSurface` 进一步使用主/辅助窗口的物理 origin 与 DPI scale 统一坐标；Workspace Window 渲染真实
  多项 Tabs，并接收主窗口或其它窗口的外部 drop。main→window、window→main、window→window 共用原子
  session transfer，非空源保留，最后一个 Item 离开时才关闭；活动项 task 绑定可选，为 document/editor 等
  `WorkspaceItemKind` 保留窗口所有权。`document-editor` 与 Host-owned `terminal` Item 现均可在这些 Pane/窗口
  中呈现真实内容；Terminal 支持行输入、Ctrl+C/Ctrl+D、输出复制、ANSI 屏幕、scrollback、resize、显式终止和从 Coding Tools 重新打开，
  恢复 topology 时只显示上次会话已结束，不会重放命令。Android legacy `process_session` 复用同一 task-scoped
  PTY，服务端固定 task cwd、校验任务依赖并只公布活动 session ID；它是已配对设备触发的用户 Shell，不宣称
  文件系统沙箱。真实 Android/LAN 和 Windows ConPTY 仍待系统门禁。
  辅助 Workspace Window 也已递归渲染完整 Pane Tree，每个叶 Pane 有独立
  Tabs、焦点、水平/垂直拆分、跨 Pane/窗口拖放、关闭与空 Pane 折叠；焦点 Pane 提供完整任务交互，非焦点任务
  Pane 从按 task 缓存并随事件刷新的真实 `TaskSessionView` 渲染时间线、Markdown 与状态摘要。当前剩余的是
  更多 IDE Item、完整编辑器/终端交互和系统门禁，而不是窗口布局合同。
- 多行 Composer 已通过 `DesktopApplication` 调用 Service 持有的 Agent Wire，支持
  真实发送、停止、session binding 复用、流刷新、权限允许/拒绝、最多 8 MiB 的系统剪贴板文字粘贴，以及
  经过维度/RGBA 上限校验并编码到独立 Preview 缓存的剪贴板图片；Explorer/Finder 复制的文件列表由
  `DesktopHost` 类型化读取，应用层统一去空、去重并描述为附件，主窗口、草稿和任务窗口都有真实粘贴入口；
  主窗口与任务窗口共享同一 task-scoped revisioned Composer。正文、附件、对话引用、模型、reasoning、权限与
  Plan/Goal 模式保存到 Preview 独立 SQLite，未发送草稿可跨进程恢复；该能力强于当前 Tauri 普通草稿。
  pending 决策会校验当前 task/session/turn/revision，失败时保持 Open 以便重试。MCP `mcp_elicitation`
  不再降级为普通提问：应用层保留 interaction/schema/meta，统一解析 Form/URL、字段默认值和必填/类型约束，
  并归一化 accept/decline/cancel 响应。主窗口和任务窗口使用 NanaUI 原生表单、枚举/多选/布尔控件、自由数组
  或系统 URL 打开动作；不支持或无效 schema 会禁用接受但继续提供真实拒绝/取消，不使用隐藏 WebView。
- NanaUI Hosted runtime 会把 Explorer 文件 hover/drop/cancel 作为带窗口身份、路径和逻辑光标位置的类型化事件交给应用；
  主窗口与任务窗口使用同一附件合并/去重路径，并在可接收区域显示真实释放提示。Composer 和时间线中的 raster/SVG
  图片通过 Iced/WGPU 原生预览，可显式交给系统打开，不启动隐藏 WebView。当前仍需在解锁 Windows 桌面完成真实
  多文件拖放、文件/图片剪贴板、中文/无权限路径、缩略图视觉和缓存回收门禁；Native 的 Ctrl/Cmd+V 自动文件探测
  还需等 NanaUI 按键事件 revision 正式发布并更新 immutable pin，当前不伪造为已接入。
- 活动 turn 期间 Composer 继续可编辑：非空发送将完整请求和 UUID turn ID 持久化为 FIFO，空输入才显示停止。
  启动先恢复权限/计划/提问等待态，再按持久顺序挂回队列；正常完成逐项推进，显式停止清空队列且不推进。
  Composer 首个无空格 `/` token 会通过应用层搜索内置 `/help`、`/status` 与项目 `.lilia/commands/*.md`；
  主窗口和任务窗口均使用稳定目标显示真实候选。选择后写回 revisioned 草稿，只有无附件和对话引用的单一精确命令会走命令
  执行并投影 Product 时间线，其余文本继续作为普通 Agent 请求。
  Composer 的 `#` 会跨项目搜索可引用任务，`@` 会在当前项目根内搜索文件与目录；共享应用层统一限制结果数量、
  遵守 `.gitignore`/隐藏目录并拒绝父路径逃逸，现有 Tauri command 也只适配该服务。两种选择均以 expected revision
  原子应用并移除触发词；类型化引用保存在 Composer、Todo Guide、durable turn 与 Agent metadata 中，显示标记只追加一次。
  运行中发送会创建持久 Todo Guide，并由真实 Agent tool/user/idle 事件按 high/normal/low 窗口调度；状态按
  `pending → queued → sent` 推进，显式停止会把未执行 Guide 恢复为 `pending`。Composer、Guide 与 turn queue
  共享同一 SQLite 连接：空闲发送以单事务 clear+enqueue，运行中发送以单事务 create Guide+clear，等待交互时
  还在同一事务按 FIFO 选择 Guide、标记 queued 并 enqueue。队列以每 task 唯一 `queued/claimed` 状态、随机
  claim token 和进程 epoch 持有所有权，terminal ack 与 next claim 同事务；恢复会保留完整 request 和稳定 ID。
  该状态机不代表 Provider/工具外部副作用已经具备全局 exactly-once 证明。
- 开发态 `LILIA_NATIVE_AGENT_DEBUG=1` 提供环回 TCP 的稳定目标 observe/click/input
  协议，命令经 NanaUI `EventLoopProxy` 进入真实消息处理；生产构建不编译该入口。
- 多行 Composer、AskUser/计划修改和原生 Markdown/KaTeX/Mermaid 已接入真实时间线；
  Automation 已具备共享持久化服务与原生 WGPU Canvas，可真实创建、发布、启停、删除
  workflow，并编辑节点位置、端口连线和节点/边；Scope 覆盖 Inbox、项目、任务状态、Agent 后端与事件 kind，
  Trigger/Agent/Logic/Tool/Human 按各自合同显示类型化检查器，同时保留明确标识的高级 JSON 入口。各节点使用 UI 无关
  执行器，Native 可手动运行、查看 run/node 状态并提交人工响应继续。Agent correlation 使用
  两阶段启动并在 terminal callback 后自动推进图。Native 可原子取消活动 run/node；等待中的 Agent
  通过 run/node/turn correlation 精确取消，不复用会清空任务 FIFO 的用户中断语义。旧状态约束会在
  启动时事务迁移且保留既有运行历史。Agent turn 的持久 claim/ack 已能跨进程重绑并继续，
  但自动化端到端副作用幂等、进程终止点故障注入与恢复报告仍是切换阻断项。
- Roadmap 已接入 Milestone CRUD、状态、重排和任务关联；Memory 已接入用户/项目条目的保存、
  启停、删除、设置与任务注入状态。Native 新目录中的二者默认扩展 Product Core 的
  `product.db`，以同一 `projects/tasks` 权威关系约束外键；Tauri 显式保留旧库兼容模式。
  Coding Tools 直接复用同一 Native AgentKit Runtime，显示
  MCP/LSP/registry 状态，并提供真实 Git status、working-tree/staged diff、Code Index search 与 Computer Use
  工作区列表。大 patch 通过共享不可变资源存储恢复，界面只展示标注范围的预览；尚未提供 stage/commit。
- Quota 已通过共享服务读取 Product Core 本地用量并绘制原生 WGPU 趋势图；Extensions 展示同一
  AgentKit Runtime 的 service/Skill/MCP 状态。MCP 注册表管理已下沉到共享应用服务，以 revision
  冲突检测和 staging/backup/rollback 原子替换支持 Stdio、Streamable HTTP、SSE 的新增、编辑、启停与删除；
  Native 参数表单保存类型化 JSON 数组且不经 shell 解析，HTTP URL 拒绝内联凭据。注册表只保存 Stdio 环境变量名
  或 HTTP/SSE 请求头名；值进入独立 Preview Windows Keyring，快照只公开配置状态，运行时在内存解析并注入同一
  AgentKit transport。引用或 server 删除同步清理 Keyring，注册表写入失败会恢复已删除值。MCP catalog 可用时，
  Extensions 直接展示同一运行时的工具、资源和提示词详情。持久化后按 server ID 真实断开/重连并逐项报告失败。
  Skill 与 Hooks 都已下沉到共享应用服务：用户 Skill 通过 revision-safe 注册表和原子目录发布管理；用户/项目
  Hook 通过原子 JSON 文档创建、编辑、启停与删除，并在 Agent turn 的 `UserPromptSubmit` 和 `Stop` 边界执行。
  Hook 子进程只接收受控 JSON stdin、最小环境和任务工作目录，输出受限且不写入产品时间线；SQLite execution fence
  防止完成项或崩溃后状态不明的外部命令被静默重放。Remote Control 已迁入共享
  应用服务，Native 可启停宿主、管理受信设备和系统保活，并用 NanaUI/WGPU 原生二维码完成配对，
  不启动隐藏 WebView。
- Architecture 已从 Tauri 私有 SQL 迁到共享服务，保留既有 JSON 合同并增加 request ID 幂等与
  expected version 冲突；Native 以 WGPU GraphCanvas 展示节点/关系、版本历史和一次性回滚。
- Native 薄启动器在加载 Host DLL 期间显示不占任务栏的真实 Win32 启动窗口；Host 的第一个 NanaUI/WGPU
  加载帧 present 后才关闭该窗口并恢复应用状态，因此可见冷启动不等待数据库与 AgentKit 初始化，且不引入第二套
  WGPU 上下文。Native Host 已接入主窗口状态、独立托盘、全局快捷键和带 token 的单实例环回 IPC；会话状态与任务
  会话均可打开为真实 NanaUI 辅助窗口。会话状态窗保留 geometry、置顶与透明度偏好，显示 Provider 真实状态，
  并可打开未落库的新对话草稿。主侧栏、全局快捷键、子对话和时间线选中文本提问也复用该 transient draft；主/辅窗口
  共享同一草稿 renderer 和 Composer reducer，关闭草稿或切换项目/任务不创建任务，首次发送才通过共享应用层 materialize
  并在原 surface 晋升。会话建议只出现在空的项目草稿，不再污染
  既有会话。每个正式任务窗口持有独立 Workspace session、共享应用层时间线与
  Composer，支持附件、发送/停止、权限/计划/Goal 切换以及审批/提问；重复打开同一任务只聚焦原窗口，
  辅助窗口关闭不退出主进程或覆盖主窗口几何。任务窗口集合、session、Item 布局/所有权和物理 geometry
  已通过真实进程重启恢复。共享 CLI
  可按 cwd 打开项目并接收版本化 task handoff。独立 `liliacode-native.cmd` 已通过 NSIS 安装后的
  两个空格路径、运行中转发、task handoff accepted 回执/Product Core 持久化和卸载 PATH 清理
  实测；10 个并发独立 CLI 也已全部进入同一 Product Core。Explorer 启动、跨显示器窗口恢复、任意窗口类型
  topology 和 CLI 接收端崩溃恢复仍待系统门禁。
- Native 在创建 NanaUI 窗口前设置独立进程 AppUserModelID
  `sena-nana.LiliaCode.NativePreview`，避免与正式 Tauri 版共享任务栏身份；快捷方式属性与通知身份仍需
  在干净 Windows 11 安装环境实检。
- Preview 更新器使用独立 HTTPS 通道，限制下载大小，在 staging 内验证 Minisign、拒绝不安全
  重定向、路径穿越、多安装器和伪 PE，并只启动唯一 NSIS 安装器；设置页展示真实检查、可用版本、下载/安装/重启
  与失败状态。发布 workflow 会构建签名包、草稿 Release、运行安装冒烟并显式提升浮动通道；
  `agent-debug-runs/native-launcher-installer-smoke-2026-08-11.exe`
  是一次较早源码构建的 22180364-byte 本机未签名烟测包；它已证明 Host DLL 随安装器落盘、静默安装、独立 PATH/CLI、两个空格路径、
  10 路并发单实例转发、task handoff、等待旧进程 PID、覆盖安装/自动重启、卸载和 PATH 清理。该包不可发布；
  生产密钥下联网签名升级、失败回滚和
  干净 Windows 11 VM 仍待端到端验证。
- Native 设置已提供显式数据迁移页，展示真实源目录、计划状态、逐类文件/凭据状态和
  执行报告；凭据必须单独选择。数据库成功复制后由重启辅助进程完成空库备份与激活，
  旧目录始终只读。CLI 仍可用于无人值守的计划文件流程。
- Tauri 启动也已收敛为单一 `DesktopApplication`/`ServiceAuthority`，不再分别打开 Product Core、
  Runtime 和领域服务；旧 command 合同通过 Tauri Host/Event bridge 保持兼容。
- NanaUI 依赖当前固定到已推送的完整 revision
  `c0a4404b327bcd27ba7a55657437180459a9b346`；该 release set 包含 HostedBrowser、窗口置顶、KeyCapture、
  ConfirmDialog busy 与跨显示器/DPI 窗口恢复合同，已通过 workspace 全特性 check/test、严格
  Clippy、Windows `hosted,browser` 交叉检查和 UI 快照生成。本仓库 `lilia-native-preview` 仍需独立通过自己的
  业务与发布门禁。

原生调试入口为 `yarn verify:native-agent-debug`。脚本使用隔离 home、真实临时 Git
仓库、按 home 哈希隔离的 Windows Keyring 命名空间和环回模型 fixture 启动 Debug
binary，以稳定 target ID 回放 Provider 保存/刷新/撤销、设置、主题、数据迁移计划/执行/报告/重置、Worktree、
项目/任务 CRUD、搜索与归档恢复、Goal/Todo、Composer、斜杠命令、`#` 对话引用、`@` 项目上下文、附件、计划/提问/权限、草稿与 FIFO 重启恢复和最终时间线。操作必须进入真实
`DesktopApplication`/Broker/SQLite/Git 路径；脚本会等待凭据清理数量下降，并扫描
summary、协议和日志，确保 secret canary 不进入产物。不可输入目标返回结构化不可用，
同一 Win32/WGPU 窗口会产出任务页、Provider、Automation、Roadmap、Memory、Coding Tools、Architecture、
Quota、Extensions 和 Remote Control
页面 PNG。截图除像素变化外
还必须包含足量中性 UI 像素，防止把锁屏壁纸误判为应用。产物写入
`agent-debug-runs/native-*/`。首个严格通过这一组基础门禁的产物是
`native-2026-08-10T01-42-35-289Z`，共记录 34 项功能检查和 10 张有效 Win32/WGPU PNG；已通过
项目观察、设置/主题、真实 updater 状态、托盘、全局快捷键注册/撤销、系统剪贴板、数据迁移、
Provider 和凭据清理。验证器先在真实权限卡出现后终止 Native
进程，用同一 home 恢复并拒绝原 turn；随后在真实计划卡等待时再次终止进程，新进程从 Product Core
绑定、AgentKit 持久会话和开放投影恢复同一计划 turn，批准后继续模型请求直至完成。另一独立任务还通过
`cancel-turn` 稳定目标直接中断计划，不提交伪造的 decline 响应。交互工具现在始终注册，只有工作区工具
依赖 workspace，因此未绑定目录的任务也能提问和确认计划。该运行还要求窗口内 Workspace revision 先与
磁盘 committed revision 对齐再强杀，并验证任务 Item 的稳定 kind、Product Core 标题、composer 焦点、
close/split/persist capability 和活动标签均可恢复。该流程还打开真实 NanaUI 会话状态工具窗口，等待
Ready、从窗口内任务入口导航并聚焦主窗口，再向辅助 Win32 窗口发送真实 `WM_CLOSE`；主进程继续运行，且前后
`main-window-state.json` 完全一致。该流程还从主任务页打开真实任务窗口，等待 Ready，确认其 Workspace
session 与主窗口不同；从弹窗读取真实 Windows 剪贴板、输入共享 Composer 草稿、再次打开时复用同一
session，再以真实 `WM_CLOSE` 关闭，并再次核对主进程与主窗口几何未变。运行继续完成 Automation、
Roadmap、Memory、Coding Tools、Architecture、Quota、Extensions 和 Remote Control 的真实业务操作及
GPU 截图；Quota 从 Product 投影聚合到 23 tokens，远控二维码由原生 WGPU 路径绘制。`native-2026-08-10T10-02-55-092Z`
在截图前完成 30 项功能协议与 6 次重启，覆盖 Guide 状态推进、持久 FIFO、claim token/epoch 重绑、ack 原子提升
和显式取消清 durable rows；最终仍因锁屏/非应用画面和剪贴板 PowerShell 超时被硬门禁拒绝；最近完整有效
GPU 产物仍是 `native-2026-08-10T01-42-35-289Z`。该自动化通过不替代
Windows 11 的 DPI、IME、拖放、休眠/唤醒、设备恢复和人工扫码门禁。
Native 调试协议还会用限长状态保存 `mark` 与真实运行错误；启动会在首帧后自动恢复已启用 MCP，因此回放先断言
错误历史只包含无效 MCP 夹具按 server 归因的 `recent-errors`，不允许混入其它启动错误。`agent_debug` 模块只在 `debug_assertions` 下编译；当前源码的
release 严格 Clippy 与 release build 均通过。发布脚本会在生成 NSIS 前调用
`yarn verify:native-agent-debug:release-exclusion` 的同一二进制扫描器，以 UTF-8/UTF-16LE 检查最终
`lilia-native-preview.exe`，调试环境变量、监听错误文案和 `recent-errors` 任一存在都会阻断发布。

`native-2026-08-10T10-54-16-381Z` 在首张截图前通过 32 项功能协议与 6 次真实进程重启。新增场景从主
Composer 输入 `#计划` 与 `@README`，通过稳定目标选择真实 Product 任务和工作区文件，随后发送普通 Agent turn；
模型 fixture 确认同一最后用户消息同时包含类型化对话引用与文件引用，发送完成后 Composer 的正文、附件和对话引用
全部由 revisioned clear 清空。功能路径通过后，真实 GPU 截图仍因锁屏环境只得到 1 个中性色样本而被硬门禁拒绝，
并记录剪贴板 PowerShell 超时；最近完整 GPU 产物不变。该 revision 的 release EXE 为 73,667,584 bytes，6 个
调试标记的 UTF-8/UTF-16LE 排除扫描通过。

`native-2026-08-10T11-41-17-462Z` 将 GPU/剪贴板系统门禁改为延迟收口：首个真实截图失败会被记录，后续
页面继续执行功能协议但不再重复无意义采集，最终仍统一返回非零。该运行因此完成 49 项功能检查和 6 次真实进程
重启；Roadmap 真实回读持久描述、有效闰日、无效日期拒绝且不落库、日期清空/恢复、状态与任务关联，随后继续通过
Memory、Worktree、Todo/Goal、Composer、Quota、Extensions、Remote Control 和最终双 Pane 状态。运行最后同时报告
PowerShell 剪贴板超时、原生剪贴板占用，以及 Provider 页面只有 5 个中性色样本的锁屏截图，9 个后续 surface 被
明确标记为跳过；这些系统门禁没有降级为成功，最近完整有效 GPU 产物仍是
`native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T12-00-46-613Z` 在同一完整回放中新增 Automation 取消门禁：等待 Human 节点的 run
通过稳定目标真实转为 `cancelled`，取消目标随即消失；同 workflow 立即建立第二条 run 并成功恢复为
`succeeded`，最终观测保留两条运行历史。49 项功能协议与 6 次进程重启全部完成。最终仍因 PowerShell
剪贴板超时、原生剪贴板被占用及 Provider 截图只有 4 个中性色样本而非零退出，9 个后续 surface 被明确
跳过；最近完整有效 GPU 产物保持不变。该 revision 的 release EXE 为 73,708,032 bytes，6 个调试标记的
UTF-8/UTF-16LE 排除扫描通过。

`native-2026-08-10T12-25-06-731Z` 进一步通过真实 Scope 控件写入 Inbox、任务状态、Agent 后端和事件 kind，
再通过 Human 节点类型化 Prompt 输入保存配置；刷新自动化后，调试观察从 SQLite workflow 快照回读相同 Scope 与
节点配置。随后发布、启用、取消首条等待运行、立即建立第二条运行并人工恢复至 `succeeded`，49 项功能协议与
6 次进程重启全部完成。最终仍因 PowerShell 剪贴板调用超时、原生剪贴板被占用及 Provider 截图只有 3 个中性
样本而非零退出，9 个后续 surface 明确跳过；最近完整有效 GPU 产物仍为
`native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T02-05-40-603Z` 在完整回放中新增第三次真实强杀：普通
`ask_user_question` 从 Product 投影恢复为同一 `WaitingInteraction`，稳定 option target 的选择进入
AgentKit transcript，fixture 确认收到精确答案后同一 turn 完成。该次运行随后在 Provider GPU 截图处因
Windows 会话已进入锁屏而失败；截图硬门禁正确拒绝了锁屏壁纸，因此最近一次**完整通过**仍是上面的
`native-2026-08-10T01-42-35-289Z`，需在桌面解锁后重跑全套。

时间线现使用 NanaUI 的可复用可变高度虚拟布局：主任务面、任务窗口和窗口次级 Pane 仅构建
viewport 与 overscan 内的事件；加载更早页后，Hosted runtime 在新 UI tree 构建完成时对稳定
scrollable ID 执行 retained `ScrollBy`，从而保持 prepend 前的可见事件锚点。千条事件模型测试
证明单帧窗口少于 40 项，但真实 WGPU p95 仍必须在解锁的固定 Windows 11 环境测量。

上下文压缩落在 Mutsuki Agent Kit 的真实 Context Runner/AgentLoop 边界，而不在 UI 截断历史。
`AgentRunBudget.max_context_tokens` 与整次运行 token 预算分离；Lilia 从当前模型 capability 的
`context_window` 分配 80% 输入窗口。旧 turn 被压成带 provenance metadata 的确定性摘要，最新
user/tool 因果链与 turn-scoped system 上下文保留；模型只接收压缩窗口，Session Runner 继续持久
完整 transcript。公开 HostRuntime 到 Anthropic HTTP Adapter 的回归测试已同时验证两条路径。

用户主动压缩与上述自动输入窗口不同。Native 主窗口、任务辅助窗口和 Tauri 现在都提交共享
`lilia_compact` workflow：应用层读取当前绑定的完整 session，以确定性窗口限制 control-model 输入，
生成包含目标、约束、已完成工作、未决风险与下一步的持久摘要，并把摘要和可见完成回复写入新的
AgentKit session。Product binding 只在新 session 和时间线投影成功后替换；模型失败、空结果或提交前取消
保持旧 binding，旧 session 不修改。Mutsuki `ContextCompactionCoordinator` 的自动高层 summarizer 仍是
独立 owner 缺口。

Native Coding Tools 已消费共享 Code Index 的工作区文本搜索，结果行不再只是只读摘要：选择结果会沿
项目相对路径门禁打开 DocumentStore 文档，并在当前 Workspace 中激活真实 `document-editor` Item；同一
路径的多个结果按 path/range 暴露不同 Agent-debug target。Code Index byte range 会转换为编辑器字符位置并
用 NanaUI O(1) cursor/selection 选中真实文本；Text/Symbol mode 直接调用共享索引，不在 View 中伪造筛选。
光标移动、选择和滚动只更新 View 状态，不再推进 Buffer revision。导航完成后 `HostedUiCommand::Focus` 将键盘焦点
交给实际拥有该 Item 的主窗口或辅助窗口。
共享 LSP 问题列表也消费 DiagnosticStore 的 byte offsets；选择问题行会选中对应文本，同样不制造文档编辑。
共享 definition 用例现从同一 LSP session 归一 `Location`/`LocationLink`，以 source buffer revision 拒绝陈旧查询，
并把目标限制在 canonical active project root 内再打开 DocumentStore；UTF-16 range 转为真实 UTF-8 buffer offsets。
Native 的 F12 与可见按钮从 retained cursor 发起后台查询；单目标直接跳转，多目标显示真实选择列表，已有跨窗口
Item 会聚焦 owner 而非复制。未找到、陈旧 revision 与 LSP 不可用均显示真实状态。
Coding Tools 同时通过共享 Git 服务读取 working-tree/staged diff 并可切换，
包括资源化的大 patch，并明确只展示前 24 行；Native 没有绕过审批/HEAD fence 暴露写操作。
Document Item 已按 LanguageRegistry token 与当前深浅主题消费 NanaUI 增量 syntax highlighter；高亮复用同一
hosted editor 状态，不替换 text、cursor、selection、IME 或 undo/redo。编辑器内诊断 decoration 尚未实现。
Mutsuki owner 工作树现已把仅覆盖 HEAD 的候选栅栏扩展为 canonical worktree 的完整状态令牌，并对同 service
写请求串行执行状态复核；在 owner 发布不可变 revision、Lilia 更新 pin 并完成 Windows 回放前，stage/commit
仍保持不可见。NanaUI owner 的 HostedBrowser 与 HostedWindow PNG capture 已发布；Native 右侧 IAB Dock 可把当前
页面转入按任务复用的独立浏览器窗口，Windows 截图成功时附加 PNG，其它平台生成诚实的 metadata-only 结果；共享
应用层把 URL、标题、备注和截图作为同任务的持久 Agent turn 提交，运行中进入 durable FIFO，不保留旧 Codex runner
stdin 特例。当前代码与交叉编译证据不能替代 Windows 11 上的真实 WebView、像素与 Agent Debug 回放。

`native-2026-08-10T02-27-48-957Z` 进一步在首张 GPU 截图前通过了 Workspace Pane Tab
回放：两个持久化任务 Item 均暴露稳定标签目标，激活旧标签会切换到其真实 Product 项目/任务，关闭活动
标签会从 Pane Tree 注销 Item 并自动选择相邻任务。该次运行随后仍在 Provider 截图处捕获到 Windows
锁屏画面并被中性像素硬门禁拒绝，因此这只是功能协议通过证据，最近完整通过产物不变。

`native-2026-08-10T02-44-00-569Z` 在上述持久化 Workspace 回放之外，还以前置场景验证了项目编辑器
通过 `DesktopHost` 选择隔离真实 Git 目录、清除路径并再次选择；稳定目标随真实可用状态出现和消失。该次
运行同样在 Provider 首张 GPU 截图处捕获锁屏并被硬门禁拒绝，因此新增的是 Host/功能协议证据，最近完整
通过产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T03-04-02-594Z` 又在首张截图前从隔离真实 Git 仓库执行后台 clone，确认新的 `.git`
工作树落盘后才创建 Product Core 项目，并通过稳定上移/下移目标把同一 pinned 分组顺序写回 `sortOrder`。
Host/业务协议均通过；Provider 截图仍是 Windows 锁屏并被硬门禁拒绝，最近完整通过产物保持不变。

`native-2026-08-10T03-20-33-771Z` 进一步在截图前把种子任务下移再恢复，并将同一任务移动到真实 clone
项目后移回原项目；两次移动都保持任务 Tab 与选择，绑定 conversation 的项目归属由共享应用用例同步。
完整功能协议通过，首张 Provider GPU 截图仍因 Windows 锁屏被硬门禁拒绝。

`native-2026-08-10T03-38-47-328Z` 又把该任务移动到显式 Inbox，打开收集箱列表、重新打开孤立任务并移回
原项目；过程不创建合成 Project，Workspace 与 conversation 归属保持一致。功能协议通过，GPU 截图仍因
锁屏被硬门禁拒绝。

`native-2026-08-10T03-49-19-729Z` 在同一前置协议中把种子任务挂到另一个 Product 任务下，再通过专用
稳定目标恢复根级；`selectedTaskParent` 观测来自真实 Workspace 快照。其余协议通过，截图仍被锁屏门禁拒绝。

`native-2026-08-10T04-19-59-493Z` 又在真实主窗口中递归拆分 Pane Tree，将选中任务 Item 移到新 Pane、
切换焦点到已清空的旧 Pane，再关闭空 Pane 并确认树折叠和任务选择恢复；最终还建立两个同时有内容的 Pane
作为 GPU 截图场景。所有功能协议通过，首张 Provider 截图因只获得 6 个中性样本被硬门禁拒绝，说明当前
Windows 会话仍无法提供可信应用画面；最近完整通过产物保持为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T04-45-06-232Z` 进一步通过稳定目标调整真实 NanaUI 分隔条，将 ratio 从 0.5 持久化为
0.6，并验证 Workspace Item 的 `resourceId` 在普通打开及 Plan 强杀恢复后保持一致。全部功能协议通过；首张
Provider 截图仍因锁屏会话只产生 1 个中性色样本被硬门禁拒绝，因此最近完整 GPU 产物仍保持不变。

`native-2026-08-10T05-00-41-962Z` 验证任务新窗口使用不同 view instance 共享同一 task resource，并将主窗口
原 Item 携带序列化视图身份、resource、选择和 revision 原子移入真实 NanaUI 任务窗口再移回；主窗口持久化
revision 随后追平。全部功能协议继续通过，首张 Provider 截图因锁屏会话只获得 6 个中性色样本被拒绝，最近
完整 GPU 产物仍保持不变。

`native-2026-08-10T05-14-01-767Z` 在移动原 Item 后等待主 Workspace 与任务窗口集合的 committed revision
分别追平，随后终止并重启 Native 进程；同一任务窗口 session、Item/view/resource、主窗口无重复所有权及
物理 geometry 均恢复，再从真实 NanaUI 窗口原子移回主窗口。该轮 4 次进程重启和全部后续功能协议通过；
最终首张 Provider 截图因锁屏会话得到 0 个中性色样本被硬门禁拒绝，最近完整 GPU 产物仍保持不变。

`native-2026-08-10T05-25-17-175Z` 将上述恢复改为 schema v2 单一 topology 提交。回放在终止进程前直接读取
`workspace-topology-state.json`，断言 schema/revision、主 Workspace 无该 Item、任务窗口同时持有同一
descriptor/布局/geometry；重启后仍保持窗口 session 与精确所有权，再完成其余三次 Agent 交互重启。全部
功能协议通过，最终截图仍因锁屏得到 0 个中性色样本被拒绝，最近完整 GPU 产物保持不变。

`native-2026-08-10T05-41-18-716Z` 又通过稳定目标驱动同一 Pane 的 NanaUI Tabs 重排，直接断言真实 pane
`itemIds` 顺序、topology 持久化 revision、激活与关闭邻项，并继续完成 workspace-window、permission、plan、
question 四次强杀恢复和全部后续功能协议。最终截图只得到 5 个中性色样本，被锁屏硬门禁拒绝；最近完整 GPU
产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T06-00-40-974Z` 使用独立 `drag-to-pane` 稳定目标回放跨 Pane Tabs 释放，断言目标 Pane 获得
原 Item、源 Pane 不再拥有、Workspace 与 topology revision 均落盘，并继续通过上述四次强杀恢复及全部后续
功能协议。最终截图只有 7 个中性色样本，仍被锁屏硬门禁拒绝；最近完整 GPU 产物保持不变。

`native-2026-08-10T06-20-03-920Z` 在真实任务窗口强杀恢复后改用跨 surface Tab target，而非“移回主窗口”
按钮，将同一 Item 原子转回主 Pane并关闭空窗口；物理 geometry、session、resource、所有权和双 revision
断言及其余三次重启全部通过。最终截图只有 8 个中性色样本，仍被锁屏硬门禁拒绝；最近完整 GPU 产物不变。

`native-2026-08-10T06-45-53-511Z` 将 topology 升级为 schema v3。回放把两个真实任务 Item 依次从主 Workspace
移入同一辅助窗口，直接断言新文件不含单 task/item 描述符、主窗口无重复所有权，并在强杀后恢复同一 session、
两项顺序、活动 Tab 和 geometry；随后逐项跨 surface 移回，第一项离开后窗口仍存活，最后一项离开才关闭。
workspace-window、permission、plan、question 四次重启及全部后续功能协议通过。最终截图只有 1 个中性色样本，
被锁屏硬门禁拒绝；最近完整 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T07-12-42-627Z` 在 schema v3 多 Item 窗口中继续通过真实稳定目标拆分辅助窗口 Pane、把第二项
拖入新 Pane、将分隔比例调整为 0.6，并直接断言结构化 `workspaceWindows` 观察结果与磁盘 Pane Tree 一致；
强杀后恢复了同一 window/session、两个 Pane 的唯一 Item 所有权、活动 Pane/Tab、ratio 和 geometry。回放随后
先聚焦仍有内容的 Pane，逐项跨 surface 移回，验证空 Pane 不导致非空窗口提前关闭。workspace-window、permission、
plan、question 四次重启及全部后续功能协议均通过。辅助窗口的非焦点任务 Pane 已读取真实缓存
`TaskSessionView`，不再显示占位 surface。最终截图只有 2 个中性色样本，被严格锁屏门禁拒绝；最近完整 GPU
产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T07-46-49-415Z` 使用真实环回挂起 Git HTTP 请求验证了 clone operation 生命周期：Native
显示有序进度和真实取消入口，取消会终止请求/进程树、删除仅属于本次操作的预留目录且不创建 Product 项目；
同一页面随后立即以隔离本地 Git 仓库重试成功，并确认 `.git` 工作树和项目入库。workspace-window、permission、
plan、question 四次重启及全部后续功能协议继续通过。最终截图只有 2 个中性色样本，被锁屏硬门禁拒绝；最近
完整 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T11-16-26-456Z` 在 33 项功能协议和 6 次真实进程重启中继续覆盖 revisioned Composer、
`#` 对话引用、`@` 项目上下文和 Agent 请求；新增检查从任务绑定的持久化 AgentKit `Usage` 事件恢复出
`4 tokens`，主窗口与任务窗口消费同一 `DesktopTaskSessionSnapshot`，调试观察同时证明 limit 与 percent
保持为空，没有根据模型能力伪造上限。功能协议全部通过；最终 GPU 截图只获得 3 个中性样本，仍被锁屏硬门禁
拒绝，因此最近完整 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T14-08-40-774Z` 将 Coding Tools 从主页面迁入持久化 Right Dock。共享 Git、目录和
Code Index 响应先归一化为 Lilia 类型化快照；回放验证 Dock 打开/关闭/重开、360px extent、真实搜索、项目
Memory 写入及经 Memory 页面删除的完整路径。全部 49 项业务检查与 6 次真实进程重启通过。该运行仍因当前
Windows 会话剪贴板被占用、Provider 截图只得到中性画面而被整体硬门禁拒绝，后续 9 个截图 surface 跳过；
最近完整有效 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T14-30-25-606Z` 将模型 fixture 的真实 Markdown 扩为含中英文、左/中/右列对齐、
行内公式和 Mermaid 的 corpus。Native 时间线观测到 6 个结构化表格与 20 个完整复制目标；新增检查确认
NanaUI 原生表格和 Host-backed 文档复制入口已进入真实业务路径，其余 49 项业务检查与 6 次进程重启继续
通过。系统剪贴板仍被其他进程占用，复制错误被如实保留；Provider 截图仍是中性锁屏画面并跳过后续 9 个
surface，因此最近完整有效 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。NanaUI Gallery 另行生成并
人工检查了 dark/light 富文本离屏 WGPU 快照，表格网格与列对齐在两种主题下均可读。

`native-2026-08-10T14-57-23-509Z` 将 Roadmap、Memory 与 Architecture 打开路径迁入真实 Workspace Item。
回放逐项断言稳定 kind/resource identity、close/split/persist capability、禁止跨窗口的诚实门禁，并在保留
三个 Item 的同时通过标签目标重新激活 Architecture 与 Memory 完整业务 surface；新增检查及其余业务路径、
6 次真实进程重启全部通过。整体门禁仍只因 PowerShell/原生剪贴板被其他进程占用，以及锁屏会话下 Provider
截图只得到 3 个中性样本而失败；最近完整有效 GPU 产物仍为 `native-2026-08-10T01-42-35-289Z`。

`native-2026-08-10T15-10-49-832Z` 进一步将 Automation 迁入应用级 Workspace Item。回放断言
`automation-workspace` kind、`application:automations` resource identity、主窗口 Pane 能力和跨窗口禁用门禁，
完成完整 Canvas/发布/运行/取消/重试/人工恢复后退出，再由稳定标签目标重新激活同一 workflow 路由。新增检查、
全部既有业务路径与 6 次真实进程重启通过；最终 topology 同时保留 Automation、三种项目 Item 和任务 Item。
整体非零仍只来自 PowerShell 剪贴板超时、原生剪贴板被占用及锁屏下 Provider 截图的 3 个中性样本，后续 9 个
surface 按硬门禁跳过；最近完整有效 GPU 产物不变。

`native-2026-08-10T15-29-32-638Z` 将 Settings 也迁入 `settings-workspace` /
`application:settings` 应用级 Item，并修正重复打开保留 UI state、标签激活刷新返回来源、返回任务/概览的
真实导航语义。回放同时验证 Settings 与 Automation Item、三种项目 Item、Pane Tabs、54 项业务检查和 6 次
真实进程重启；文本/Markdown/远控剪贴板门禁全部通过，主窗口、Provider、Automation、Roadmap、Memory、
Coding Tools、Architecture、Quota、Extensions、Remote 与最终 Workspace 的 Win32/WGPU 截图全部有效，脚本
以退出码 0 完成。

`native-2026-08-10T16-31-12-841Z` 增加 MCP Elicitation 原生交互回放。脚本先证明必填字段为空时
`mcp-accept` 不可观察，再通过稳定目标填写布尔、枚举、两个多选值、必填文本和自由数组，最后确认接受动作只在
应用层 schema 校验通过后出现，并生成真实 `mcp-elicitation.png`。该次共记录 55 项功能检查、6 次真实进程
重启和 11 张有效 Win32/WGPU PNG，截图门禁错误为 0，退出码为 0，是当前最新的完整 Native Agent Debug
证据。夹具只注入真实持久化的开放交互投影，不伪造 suspended Agent continuation；真实 MCP server 发起、响应后
继续同一 turn 的系统 E2E 仍是切换门禁。

`native-2026-08-10T17-00-44-796Z` 将项目移除接入共享 Product aggregate command 和真实 NanaUI 确认框。
首次打开确认框只生成包含活动任务、活动会话和工作区路径的预览，取消后 Product Core 零变化；再次确认后，
应用在一个 SQLite 事务内将 1 个活动任务和 1 个活动会话移入 Inbox、归档项目并发布失效事件，磁盘工作区继续
存在。回放随后终止并重启真实进程，确认项目仍归档、任务仍可从 Inbox 打开且没有遗留确认状态；恢复项目也不会
把任务静默重绑。该次共记录 55 项功能检查、7 次真实进程重启和 12 张有效 Win32/WGPU PNG，截图门禁错误为 0，
退出码为 0，是当前最新的完整 Native Agent Debug 证据；`project-removal-confirmation.png` 已人工确认文案、数量、
危险操作层级和未删除磁盘目录说明完整可见。该证据不替代 Tauri/Native 同 corpus、并发竞态和 Windows 11 人工门禁。

`native-2026-08-10T17-24-49-172Z` 又将项目侧栏接入 NanaUI 公共 `ReorderList`。该控件以 4px 鼠标/触摸阈值、
Grab/Grabbing 指针、横向插入线和 moved/before 值发出纵向重排意图，不持有 Product 项目或持久化；Native 将
pinned/unpinned 映射为两个不可串组的列表，并把完整组顺序交给共享 `reorder_projects`。回放通过开发态稳定
before-value 目标把真实 clone 项目移到种子项目之前，从 `projectOrder` 回读持久化顺序，再拖到组尾恢复原序，
并生成 `project-ordering.png`。该次共记录 55 项功能检查、7 次真实进程重启和 13 张有效 Win32/WGPU PNG，
截图门禁错误为 0，退出码为 0，是当前最新的完整 Native Agent Debug 证据；侧栏顺序、深色主题和布局已人工检查。

项目排序聚合事务落地后的 `native-2026-08-10T17-35-20-840Z` 与
`native-2026-08-10T17-37-16-205Z` 均完成 53 项业务检查和 7 次真实进程重启，但 Windows 剪贴板调用超时，
且锁屏/中性画面没有可识别的 Native GPU surface，后续 12 个 surface 按硬门禁跳过并以非零退出。两次失败
没有业务排序断言错误，也不能替代 `native-2026-08-10T17-24-49-172Z` 的有效 GPU 证据。

`native-2026-08-10T17-52-09-520Z` 进一步回放任务 moved/before 拖拽：稳定目标把种子任务移到相对位置，
从 `taskOrder` 回读 Product 持久化结果，再按原 before 目标恢复完全相同的顺序；后续按钮排序、父级和跨位置
移动仍通过同一套业务路径。该次完成 54 项业务检查和 7 次真实进程重启，但 Windows 剪贴板超时，首个
MCP 截图只有 8 个中性色样本，任务排序等后续 13 个 surface 按硬门禁跳过并以非零退出；因此它证明业务
回放，不构成新的视觉证据，最新完整 GPU 证据仍是 `native-2026-08-10T17-24-49-172Z`。

任务跨位置路径随后改为上述 Product Core 原子聚合，并接入 NanaUI passive destination：根级目标覆盖项目与
Inbox，输入搜索后才展开可用父任务，以避免默认构建无界目标列表。`native-2026-08-10T18-17-05-175Z`
已用稳定目标依次完成跨项目根级拖放、按 task ID 搜索父任务并嵌套拖放、恢复根级、拖入 Inbox 和拖回原项目，
权威观察中的项目、任务与父级身份均逐步回读正确；该次同时完成全部既有业务路径和 7 次真实进程重启。退出码
仍由系统门禁拦截：Windows 剪贴板进程超时，首张 MCP 截图只有 2 个中性色样本，后续包含任务跨位置界面的
14 个 surface 按硬门禁跳过。因此它是新的功能回放证据，不是可信 GPU 证据，也不能据此声明 Tauri/Native
同 corpus 等价；最新完整 GPU 证据仍保持为 `native-2026-08-10T17-24-49-172Z`。

P0 同语料回放由 `yarn verify:ui-equivalence:p0` 单独负责。`tests/equivalence/p0-v1.json` 通过共享应用层
类型化 API 分别写入两个空 home；Tauri/WebDriver 与 Native/WGPU 顺序打开相同项目、根任务及固定时间线，
输入相同 Composer 草稿，清除固定 Goal、删除固定 Todo，并分别从真实 UI 创建 Roadmap、Memory、Automation、
Skill、Plugin、Hook 与 MCP 事实。Tauri 仅在 debug 且显式设置 equivalence fixture 时选择 Product domain 与
隔离 Memory 设置；正式构建和普通开发各自使用本进程数据目录，不要求跨宿主双向兼容。

`native-2026-08-11T16-03-18-261Z` 已从真实 Native UI 把三行正文写入 NanaUI `HostedTextarea`，将条目切到
用户作用域、保存精确 37 turn 冷却、停用条目，并在 Workspace Item 重激活后回读相同正文行数。整轮完成
67 项检查、10 次真实进程重启、36 次模型请求与 17 张 GPU 截图，截图错误和跳过均为 0；`memory.png`
人工检查可见三行正文、用户作用域、停用状态和冷却值 37。

`equivalence-p0-2026-08-11T16-13-34-587Z` 随后让两套真实 UI 使用同一 schema v6 manifest/schema v8
authority snapshot 创建三行用户级 Memory，依次停用、启用、删除并重建，同时保存相同的 Memory 全局开关、
基线开关和 37 turn 冷却值。两端最终正文 SHA-256、scope、空 projectId、排序标签、启用状态与设置完全一致；
完整初始/最终快照相等，`businessEquivalence=passed`，Tauri 与 Native 截图均通过有效 surface 门禁。
正文、Goal/Todo 和 Automation config 不进入产物，只保留长度或 SHA-256；随机实体 ID 与节点位置也被排除。

`native-2026-08-11T17-05-51-953Z` 将 GitHub Device OAuth 与仓库目录纳入完整 Native Agent Debug。共享
应用层通过 loopback fixture 完成 device code、pending、authorized、用户信息和两页仓库请求；Native 真实打开
验证链接、复制用户代码、自动轮询，加载 3 个仓库并选择其中的 private 仓库，再执行解绑。绑定元数据只写入
Preview SQLite，OAuth token 只写入独立 OS Keyring；认证 clone 通过 Git 子进程临时 extraheader 复用既有可取消
clone operation，不把 token 放入 URL、参数或输出。整轮通过 68 项检查、10 次进程启动和 36 次模型请求，
`github-repositories.png` 显示已绑定账号、private 仓库选择和可执行 clone 表单；17 张 GPU 截图均无错误/跳过，
Provider、MCP、GitHub 三类 secret canary 对协议、stdout、stderr 和 summary 的扫描为空。清理阶段现遍历所有
Provider 而非只看当前选择项；按该轮 instance identity 查询 Windows Credential Manager，确认没有残留目标。

`2026-08-10T19-13-49-665Z` 的 Tauri Agent Debug 从新建会话完成真实任务/会话提升，随后成功持久化 Composer
草稿与状态并调用 `chat_send_message`。Agent runtime 最终因本机未配置模型 Provider 凭据返回可诊断错误，脚本
将本轮记为 `blocked`，而不是把环境缺失误报成产品回归。这份证据只确认首次发送进入共享后端，Provider 就绪后的
流式响应、工具和审批场景仍需重跑。

`native-2026-08-10T21-14-20-248Z` 将应用级和项目级 Workspace Item 的跨窗口编辑链路纳入完整回放。
Settings 与 Automation 原子迁入辅助窗口后继续操作 Provider/刷新并无重复所有权地返回；Roadmap 则以
window/item 作用域稳定目标在辅助窗口修改并保存里程碑描述，强杀重启后恢复相同 window/item 与操作目标，
移回主窗口后从共享 SQLite 回读相同内容。
共享应用层 223 项与 Native 75 项测试通过，回放的全部业务检查和后续场景继续完成。该轮总体仍按硬门禁失败：
Windows 剪贴板 PowerShell 调用超时，当前锁屏会话的首张 MCP Win32/WGPU 捕获不能形成可信 Native surface，
后续 14 张截图被主动跳过；因此它是跨窗口功能协议证据，不替代最近一次有效 GPU 视觉证据。

`native-2026-08-11T05-19-49-097Z` 以薄启动器加载 Debug Host，完成 63 项业务检查和 9 次真实进程重启，
包括单实例/CLI、Workspace、Composer/Agent、Automation、架构审批、远控和数据恢复；业务链未发现启动器拆分回归。
本轮总体仍按硬门禁失败：PowerShell 剪贴板调用超时，MCP elicitation 的 Win32 捕获只有 1 个中性色样本，
后续 16 个 surface 被主动跳过。因此它只构成功能协议证据，不能替代解锁桌面上的 GPU 视觉门禁。

`native-2026-08-11T05-49-45-615Z` 在相同完整回放中加入 MCP 注册表管理：可见稳定目标创建一个停用的
Stdio server，按字符串 JSON 数组保存参数，编辑命令后启用并收到真实 `program not found`，再停用、请求删除、
确认删除；脚本直接核对 `secretFree`、条目内容和 revision 1→6，删除后确认磁盘条目消失，并继续验证原夹具的
不支持 transport 激活失败。全部业务场景完成；总体仍仅因 PowerShell 剪贴板超时、锁屏 MCP elicitation
截图只有 1 个中性色样本及随后 16 个 surface 跳过而非零，因此构成功能/持久化证据，不构成新的 GPU 视觉通过。

`native-2026-08-11T06-18-40-322Z` 将 MCP 环境变量凭据引用与 Windows Keyring 纳入同一回放：创建停用
Stdio server 时登记 `NATIVE_DEBUG_TOKEN`，registry 只保存名称；稳定安全输入把 canary 写入 Keyring，registry
revision 不因秘密写入而变化。后续编辑、启用失败、停用和确认删除仍推进 revision 1→6，删除 server 同步清除
引用的 Keyring 条目。脚本扫描 registry、Agent Debug 协议、stdout、stderr 和 summary，均未发现 Provider 或
MCP secret canary；应用层测试另覆盖缺失凭据的逐 server 错误、显式清除和写入失败回滚。本轮 63 项业务检查
全部完成，总体仍只被 PowerShell 剪贴板超时、锁屏 MCP elicitation 的 2 个中性色样本及随后 16 个 surface
跳过阻断，因此是 Keyring/功能/持久化证据，不替代解锁桌面上的 GPU 视觉门禁。

`native-2026-08-11T06-37-24-887Z` 将有效 MCP transport 纳入完整回放。调试脚本创建真实 Node Stdio
server，并通过 Keyring 注入 `NATIVE_DEBUG_TOKEN`；Extensions 从同一 AgentKit session 回读 1 个工具、1 个资源、
1 个提示词及其说明/安全分类/必填参数。环回模型的正常 Agent turn 收到 namespaced 工具描述符，Native 在 Ask
权限下显示真实允许/拒绝卡；允许后 fixture 执行 `credential_probe`，无秘密 marker 证明凭据存在，第二次模型请求
收到同一工具结果并完成 turn。随后 server 被停用、确认删除，Keyring 引用清理，原无效 transport 仍按 server
归因错误，registry revision 精确从 1 前进到 6。该场景同时暴露并修复 Mutsuki Windows Stdio 隔离环境缺少
`SystemRoot`、导致 Node CSPRNG 启动失败的根因；transport 只补回该非秘密系统启动变量，仍不继承 `PATH`，新增
单测与 crate Clippy 均通过。Lilia 250 项应用层、81 项 Native、35 项 storage 测试也通过。完整回放完成 63 项
业务检查且 secret 扫描为空，整体仅因 PowerShell 剪贴板超时和锁屏 MCP 截图只有 7 个中性色样本、随后 16 个
surface 跳过而非零。Mutsuki 修复仍须形成可固定的完整上游 revision；真实 HTTP/SSE 鉴权、资源/提示词读取和
传输崩溃恢复仍是 EQ-017 剩余门禁。

`native-2026-08-11T06-50-18-905Z` 进一步覆盖 MCP 启动恢复。脚本在真实 Node Stdio server 已启用且
`NATIVE_DEBUG_TOKEN` 已写入独立 Windows Keyring 后强杀 Native 进程，先将活动 Workspace 切回任务，确保重启
过程中没有进入 Extensions；首帧后的后台初始化自动恢复 1 个活动连接及 1/1/1 工具、资源、提示词 catalog，
并继续完成 Ask 审批、`credential_probe` 执行和同 turn 工具结果回传。实现同时修复 Settings/Extensions 快照刷新
先占用扩展 busy 状态时启动激活被丢弃的竞态：激活请求现在排队并在刷新结束后继续。最终 marker 为
`called=true`、`credentialPresent=true`，模型 fixture 同时记录工具描述符与工具结果，registry revision 为 6、
`secretFree=true` 且 secret canary 扫描为空。完整回放的 63 项业务检查全部完成；总体仍因 PowerShell 剪贴板
超时、锁屏 MCP elicitation 截图只有 2 个非中性色样本及随后 16 个 surface 跳过而非零，因此这是启动/Keyring/
功能/持久化证据，不构成新的 GPU 视觉通过。

`native-2026-08-11T10-30-22-536Z` 在相同重启恢复链路上补齐 catalog 内容消费。Extensions 的资源操作
通过同一 live `SharedMcpService` 发出 `resources/read`，再从共享 `AgentResourceStore` 解析不可变内容，调试观察
精确得到 `{"credentialPresent":true}`；提示词操作把稳定输入中的 `{"scope":"restart"}` 作为 JSON 对象传给
`prompts/get`，返回 `Native credential scope: restart`。这两条路径没有复制 MCP session，也没有以 catalog 元数据
或本地模板伪造内容，随后同一连接仍完成 Ask 审批和真实工具调用。集成层 54 项、应用层 250 项和 Native 81 项
测试通过；集成 crate 的 lib Clippy 与应用/Native 全 targets Clippy 通过。完整 `lilia-agent-integration --all-targets`
仍被当前工作区另一处 `native_runtime.rs:3062` 的 Rust 1.97 `while_let_on_iterator` lint 阻断，本轮没有覆盖该既有改动。
完整回放仍完成 63 项业务检查且 secret 扫描为空；总体非零仅来自 PowerShell 剪贴板超时、锁屏 MCP 截图
没有可信非中性色样本及随后 16 个 surface 跳过，因此不构成新的 GPU 视觉通过。

`native-2026-08-11T10-51-13-284Z` 继续验证远端 MCP 传输与请求头凭据。Native 稳定目标分别创建
Streamable HTTP 和 SSE 配置，将 `Authorization` 的值只写入独立 Windows Keyring；真实 loopback 服务收到
initialize/initialized、tools/resources/prompts catalog 与 `resources/read` 共 14 条请求，全部鉴权成功，且
`Accept` 精确为 `application/json, text/event-stream`。Streamable HTTP 的有 ID 响应使用 JSON，SSE 配置的
有 ID 响应全部使用 `text/event-stream` 帧，两条资源读取分别返回脱敏的 authorized/transport 状态。首次回放
由此暴露 Native Host 仍有一份旧 HTTP 客户端：它会把合法 `202` 空通知响应强制按 JSON 解码，也不能解析 SSE；
实现已删除该重复层并统一使用 Mutsuki 的原生 transport client。两种配置随后都完成停用与确认删除，注册表未出现
secret canary。Agent Integration 54 项测试通过，完整回放完成 63 项业务检查；总体仍仅因 PowerShell 剪贴板
超时与锁屏 MCP 截图硬门禁非零，因此本轮是功能/鉴权/协议证据，不是新的视觉通过。当前 `Sse` 合同仍是
SSE-framed POST compatibility，legacy SSE 的 GET stream/endpoint 协商及 transport 崩溃恢复尚未完成。

`native-2026-08-11T11-01-06-830Z` 在同一真实服务上加入 transport 故障注入。首次鉴权 catalog 与资源读取
完成后，fixture 对下一次 `resources/read` 返回 503；Mutsuki 现在会在 request send/receive 失败时把 session
原子标记为 Failed、保留 `last_error` 并清理 pending，Extensions 刷新后 `Ready` 计数随之下降。服务恢复后，
“激活已注册 MCP”不再只返回既有 Failed 状态，而是先断开旧 session，再从当前 secret-free 注册表与 Windows
Keyring 重新解析 manifest/请求头，完成第二次 initialize 和三类 catalog。随后 SSE-framed 路径继续成功，两个
server 均停用、确认删除并清理 Keyring 引用。结构化产物记录 21 条请求，包含唯一 503、两次 Streamable HTTP
initialize，所有请求的鉴权均为 true，未记录请求头值。Mutsuki 新增 send failure→Failed→reload Ready 单测；
完整回放仍完成 63 项业务检查，仅被既有剪贴板和锁屏 GPU 系统门禁阻断。transport 中断/显式恢复不再是
EQ-017 的功能缺口；legacy SSE GET stream/endpoint 协商仍需单独实现。

`native-2026-08-11T11-25-06-655Z` 补齐真实 legacy SSE 网络链路。Mutsuki 的 `Sse` transport 现在以独立
可终止 reader 打开鉴权 `GET /sse`，从增量 event stream 解析 `event: endpoint`，仅接受同 scheme/host/port
且不含 URL credentials 的 message endpoint；initialize、initialized、三类 catalog、`resources/read` 和 cancel
随后通过鉴权 `POST /sse/messages` 发出，所有 JSON-RPC 响应均从原 GET stream 回流。配置的 request timeout
同时约束 Streamable HTTP 与 legacy SSE POST，混合 CRLF/LF frame 按最早边界消费，16 MiB 缓冲上限和 shutdown
channel 防止无界内存或遗留 reader。结构化产物记录 22 条真实请求，其中 legacy SSE 为 1 条 GET 和 7 条 POST，
全部鉴权成功；63 项业务检查完成。总体仍仅被既有 PowerShell 剪贴板超时、锁屏 MCP 截图无可信非中性色样本及
随后 16 个 surface 跳过阻断，因此这是协议/功能证据，不是 Windows GPU 视觉通过。EQ-017 的剩余项收窄为
Tauri/Native 同 corpus 与 Windows 真机系统门禁。

`equivalence-p0-2026-08-11T11-51-21-963Z` 把 MCP 纳入既有双宿主同语料门禁。schema v3 manifest
为两个隔离 home 通过 `DesktopApplication` 类型化 API 写入同一 disabled Stdio server；schema v5 authority
snapshot 只记录 server ID、transport、状态、credential 引用和固定顺序 tuple 的配置 SHA-256，不暴露 command、
args、URL 或 secret。Tauri 的设置页已不再调用固定失败的旧 MCP 命令，而是以薄适配器读取同一 Extensions
snapshot，并把 Stdio 新增、编辑、启停、删除、打开注册表与 Env Keyring 生命周期交给应用层；编辑态 server ID
不可变，HTTP/SSE 条目只读。回放中 Tauri 与 Native 都通过真实可见 UI 编辑同一 server，最终 registry revision
均为 2，配置指纹均为 `2e54c9fc963f7a2fcef0f5963f413e0694d2cc63be28a03f0ec85a4b1c5e1338`，完整初始与
最终 snapshot 相等，`businessEquivalence=passed`。首次比较还捕获并修复了受 `serde_json preserve_order`
feature 影响的 object-key hash 非确定性，现改用固定顺序 tuple 编码。总体仍只因锁屏 Native 截图仅取得 5 个
可信中性色样本而标记 blocked；MCP 的 Tauri/Native 同 corpus 不再是功能缺口。

`native-2026-08-11T12-22-48-270Z` 将 Native AgentKit Skills 从只读目录提升为真实管理能力。共享应用层以
revision-safe、原子 JSON 注册表创建用户 Skill 包，`SKILL.md` 先在同目录 staging 中写入并同步后再原子发布；
停用不移动或改写用户目录，而是让 Mutsuki SkillRegistry 只加载已启用的精确包目录，避免同 root 的停用兄弟包被
误发现。回放通过稳定目标创建、停用、再启用、取消删除和确认删除，注册表 revision 从 0 增至 4，运行时可用计数
随每次状态同步，最终目录确实删除。63 项以上完整业务路径均完成；总体仍只因 PowerShell 剪贴板超时及锁屏下
MCP elicitation 截图只有 5 个中性色样本而 blocked。

`equivalence-p0-2026-08-11T12-29-27-649Z` 进一步将 Skill 加入 schema v4 manifest/schema v6 权威快照。
Tauri Skills 页与 Native Extensions 均通过真实 UI 停用 `equivalence-p0-v1-skill-primary`；两端最终
`skillsRegistryRevision=2`、`enabled=false`、`runtimeAvailable=false`，说明文字只以 SHA-256 进入证据，完整初始和
最终快照一致，`businessEquivalence=passed`。MCP 最终 revision 仍为 2，固定 tuple 配置指纹仍为
`2e54c9fc963f7a2fcef0f5963f413e0694d2cc63be28a03f0ec85a4b1c5e1338`。该轮视觉仅因锁屏 Native
截图只有 4 个中性色样本而 blocked，不能作为 GPU 视觉通过。

`native-2026-08-11T13-18-52-834Z` 将 Hook 管理和执行纳入完整 Native Agent Debug。回放通过稳定目标创建
默认停用的用户 Hook、保存一个 `UserPromptSubmit` command handler，并使 revision 依次推进 1→2→3；随后关闭
设置，从真实 Composer 发起 Agent turn，Hook 在该任务已绑定的 worktree 内写入唯一 `hook-ran` 标记，turn 正常
完成。回放再重新打开 Extensions，按 revision 3→4 停用 Hook，覆盖取消删除与确认删除，最终配置文件和所有 Hook
观察状态均清空。本轮 66 项功能检查、10 次真实进程重启和全部既有业务路径通过，`screenshotGateErrors` 与
`screenshotSkippedSurfaces` 均为空，退出码为 0。

`equivalence-p0-2026-08-11T13-13-12-441Z` 又把 Hook 加入 schema v5 manifest/schema v7 权威快照。
Tauri Hooks 页与 Native Extensions 均通过真实 UI 停用同一用户 Hook；两端最终 revision 均为 4、`enabled=false`、
handler 数量与 ID/event/matcher 完全一致，命令配置只以 SHA-256 进入证据，初始和最终 authority snapshot 均相等，
`businessEquivalence=passed`。该轮 Native capture 虽有 576 个非黑样本，但 575 个为中性色；因此它只证明业务
等价和截图采集完成，不单独作为有意义的双宿主视觉等价结论。

Plugin 不再沿用旧 Claude/Codex 目录开关，也不加载进程内 DLL。共享应用层定义 `lilia-plugin.json` 的
声明式包合同：安装时把来源复制到独立受管目录，校验相对路径、文件/总大小、manifest schema、Skill、Hook、
MCP transport 和整包 SHA-256；注册表 mutation 使用 expected revision，默认停用，启用前重新核验包哈希。
启用后 Skill 以 Plugin provenance 注入共享 SkillRegistry，Hook 以 namespaced source 注入当前 turn，MCP server
以 `plugin.<plugin-id>.<server-id>` 注入同一共享 runtime。MCP manifest 只能声明 Keyring 名称，不能携带环境值或
URL credentials；Native 可为只读 Plugin MCP 单独绑定/清除 OS Keyring 值。停用会移除运行时贡献并断开对应
MCP session，删除采用目录暂存、注册表回滚和 Keyring 清理事务，不修改安装来源。Tauri 与 Native 均调用同一
应用服务；Tauri 通过宿主目录对话框安装，Native 同时提供目录输入和对话框，两端都使用稳定 Plugin ID 操作。

`native-2026-08-11T14-46-11-326Z` 已把该合同纳入完整 Native Agent Debug：真实 UI 从目录安装默认停用的
Plugin，注册表 revision 0→1；启用后 revision 2，Plugin Skill 进入运行时、Plugin Hook 在任务 worktree 写入
唯一标记，Plugin MCP 从 Preview Keyring 解析环境凭据并完成 initialize/catalog 与 `resources/read`，返回
`{"credentialPresent":true}`。回放随后停用并覆盖取消/确认删除，revision 最终为 4，受管目录和 Keyring
凭据均删除。整轮通过 67 项业务检查、10 次真实进程重启、36 次模型请求和 17 张截图，截图错误/跳过均为 0。

`equivalence-p0-2026-08-11T14-48-08-783Z` 又用 schema v6 manifest/schema v8 authority snapshot 在两个
隔离 home 安装并启用同一个 Plugin，再由 Tauri packages UI 与 Native Extensions UI 分别停用。两端初始
revision 均为 2、最终均为 3；Plugin ID、版本、整包 SHA-256、Skill/Hook/MCP 计数、enabled 与
runtimeAvailable 完全一致，初始/最终完整快照相等，`businessEquivalence=passed`。两张 GPU 截图均已人工检查
为可见设置界面，但它们处于不同设置 surface，不能据此声称像素或视觉设计等价。

性能门禁由 `yarn verify:native-performance` 执行。它使用 `tests/equivalence/performance-v1.json` 为两个隔离
home 写入完全相同的 1000 条时间线语料，记录 Lilia/NanaUI revision、dirty-content SHA-256、Windows build、
CPU、GPU/驱动和电源计划；交互样本等待 Tauri 下一次 paint 或 Native 下一次真实 WGPU present。schema v2
在启动目标进程前预启动一个持久 Rust 探针，再异步等待可见窗口和采样完整 Win32 进程树，避免同步启动探针污染冷启动，
也不以单一父进程 working set 代替整棵树。Native 体积始终累加薄启动器与 Host DLL。

`native-performance-2026-08-11T05-18-20-526Z` 记录 Lilia/NanaUI dirty-content 指纹和 manifest SHA-256，
同轮自动门禁 7/7 通过。Native 的 30 次 Composer/resize p95 为 16.4944/14.5016 ms，首任务可用和千条时间线
为 1962.39/357.16 ms，发布态冷启动 p50 为 45.2909 ms、空闲 CPU/RSS p95 为 0%/206139392 bytes；
对应 Tauri 为 17.80/32.00 ms、3962.15/1079.22 ms、59.8779 ms、0%/466395136 bytes。Native 的
`lilia-native-preview.exe` 与 `lilia_native_host.dll` 合计 76860416 bytes，Tauri EXE 为 44503552 bytes；
精确当前 Native 未签名烟测安装器为 22180364 bytes。交互、冷启动与空闲资源门禁已通过，但尚无同轮 Tauri
安装器、正式签名分发包、干净 Windows 11 重复基线和 DPI/GPU 系统证据，不能据此解除正式切换门禁。

`native-performance-2026-08-11T15-09-35-937Z` 以当前 Plugin/Extensions 源码重新跑了 30 次交互样本：Native
Composer/resize p95 为 16.4671/13.7735 ms，首任务可用和千条时间线为 1918.88/238.22 ms；同轮 Tauri 为
20.20/49.90 ms 与 4394.49/1154.42 ms。该轮是 `interaction-only`，只证明核心交互继续低于 16.67 ms，官方
gate 因用户工作树现有 `useTaskComposerController.ts:627` 类型错误无法重建 Tauri Release，未评估冷启动、
空闲 CPU/RSS 或安装包体积，不能用早前二进制补齐。

最终源码又生成 `LiliaCodeNativePreview_0.1.0_plugin-current-2_x64-setup.exe`，大小 22489133 bytes；对应
Release 启动器 223744 bytes、Host DLL 77936128 bytes，UTF-8/UTF-16LE 扫描的 6 类 Agent Debug 标记均不存在。
Windows 烟测完成静默安装、独立 PATH/CLI、空格路径、运行中第二实例、10 路并发 CLI、版本化 task handoff、
覆盖更新等待旧 PID/自动重启、静默卸载和 PATH/CLI 清理。NSIS 卸载器会复制自身后异步完成，烟测现给真实
清理 90 秒窗口，不再把原卸载进程先退出误判为 CLI 遗留。正式 release 脚本仍要求匹配 tag、Updater 公钥、
签名私钥和密码；当前环境未配置这些真实凭据，因此未生成可发布签名包。

## 切换门禁

- NanaUI 固定到通过格式、测试、全特性检查、Clippy 和 Windows 真机验证的完整 revision；
- 项目、任务、对话、审批、设置、插件、自动化和异常恢复完成行为等价；
- 原生 Agent 调试通过稳定 target ID 操作真实 UI，并产出截图与回放记录；
- Windows 11 的 IME、DPI、拖放、resize、窗口材质、休眠恢复、单实例、安装和更新通过；
- 固定环境下核心交互 p95 低于 16.67 ms，冷启动与空闲资源不劣于 Tauri 基线。

任何门禁未完成时都只交付 Preview，不替换正式 `LiliaCode` 或 `liliacode` CLI。
