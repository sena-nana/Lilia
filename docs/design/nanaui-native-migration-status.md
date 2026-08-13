# LiliaCode Native Preview 迁移状态

更新时间：2026-08-13

本文是 NanaUI 迁移的交接清单。详细功能判定以
[`nanaui-feature-equivalence.md`](./nanaui-feature-equivalence.md) 为准，宿主实现与验证记录见
[`nanaui-native-preview.md`](./nanaui-native-preview.md)，后续 IDE 边界见
[`nanaui-native-ide-architecture.md`](./nanaui-native-ide-architecture.md)。

## 目标

- Native 与 Tauri 是**独立产品线**：Preview 完成后替代 Tauri，不是长期双端数据共存。
- 迁移期可保留 Tauri 版继续跑，并让其薄调用共享 `DesktopApplication`；但 Native Preview **不**为
  正式 LiliaCode/Tauri 数据目录做兼容、自动迁移或双写。
- 用 UI 无关的 `DesktopApplication` 统一产品、AgentKit、持久化和宿主合同。新合同以共享层为权威；
  Tauri 若适配，应**单写共享层**，不必再双写 legacy store「为了共存」。
- Windows 11 的功能、视觉、性能、Agent 调试和安装门禁全部通过后，再提升 Native 为正式
  `LiliaCode`、安装器和 `liliacode` CLI。
- 当前阶段不增加完整 IDE，但 Workspace Item、Buffer、Document、Language、Git/LSP/MCP 和多 Pane
  边界必须允许后续按 Zed 类架构扩展文件树、编辑器、搜索和终端。

## 当前入口

- 开发启动：在仓库根目录运行 `yarn native:dev`。
- 编译检查：`yarn native:check`。
- 测试：`yarn native:test`。
- Windows Preview 发布构建：`yarn native:release:windows`。
- Native Agent Debug：`yarn verify:native-agent-debug`。
- 程序入口：`apps/native-desktop/src/main.rs`；主要 HostedProgram 在
  `apps/native-desktop/src/preview.rs`。

## 已完成 Todo

- [x] 新增独立 `apps/native-desktop`，使用独立产品名、进程、安装标识、配置目录、SQLite、Keyring
  命名空间、更新通道和 CLI。
- [x] 新增 `crates/lilia-desktop-application`，通过 `DesktopApplication`、`DesktopEvent` 和
  `DesktopHost` 隔离产品业务与窗口、文件对话框、剪贴板、Keyring、托盘、快捷键、单实例和更新器。
- [x] Tauri 的关键项目、任务、Composer、Goal/Todo、Worktree、Memory、Plugin/Hook/MCP、自动化与
  Desktop 事件路径已逐步改为共享应用服务适配，旧 UI 继续可编译运行。
- [x] Native Shell、项目/任务导航、设置、窗口状态、Workspace Item、多 Pane、Tab 重排、拆分、
  跨 Pane/窗口移动及进程重启恢复已落地。
- [x] 项目和任务的创建、编辑、固定、排序、移动、归档/恢复、项目级对话批量归档、项目移除事务、目录选择及
  可取消 Git clone 已接入真实 Product Core；单个任务归档/恢复和项目批量归档均由共享 aggregate 在单个事务中
  同时更新任务与绑定会话。
- [x] GitHub Device OAuth、OS Keyring token、分页仓库目录、private/public 选择、解绑与认证 clone 已
  落地；token 不进入 URL、参数、SQLite、日志或调试观察。
- [x] 任务时间线分页/虚拟化、Composer 草稿与附件、队列/打断、斜杠命令、`@` 文件、`#` 会话、
  权限、提问、审批、Plan、Goal/Todo、Worktree 与 Agent 流事件已接入真实运行时。
- [x] 原生 Markdown、代码块、表格、KaTeX、Mermaid、跨块可选文本、时间线图片与查看器、Automation Canvas、
  配额图、架构图和配对二维码均走 NanaUI/WGPU/Rust 路径，不使用隐藏 WebView。图片支持 data/HTTP(S)/绝对
  file 的常见 raster/SVG，有单图读取、维度与 decode 分配上限；总 decoded/GPU 内存、复杂 SVG、失败重试和多来源
  Windows 回放仍保留为系统门禁。
- [x] Roadmap、Memory、Provider、Agent 设置、Skills、Plugins、Hooks、MCP、Automation、Remote、Quota、
  Architecture 和 Coding Tools 基础能力已形成真实可操作 surface。
- [x] 显式旧数据导入支持预览计划、只复制不删除、逐项报告、失败重试和单独凭据确认；秘密始终通过
  OS Keyring 迁移。
- [x] 托盘、全局快捷键、单实例、CLI task handoff、更新、NSIS 安装/覆盖更新/卸载与独立 PATH 已落地。
- [x] 开发态稳定目标、观察/点击/输入协议、真实 WGPU 截图和 secret canary 扫描已接入
  `verify:native-agent-debug`；Release 会检查调试标记排除。
- [x] NanaUI 固定到完整 revision `c0a4404b327bcd27ba7a55657437180459a9b346`，Mutsuki 固定到
  `fea58012240dac944c688f5e164ccf454614db7b`；提交态不使用 sibling path override。

## 已有验证证据

- `native-2026-08-12T01-41-13-419Z`：最终 NanaUI/Mutsuki Git pin 下 68 项完整 Native 回放、
  10 次进程恢复和全部真实 GPU 截图通过；截图错误/跳过均为 0，调试 Keyring 凭据已清理。
- `native-2026-08-11T17-05-51-953Z`：68 项完整 Native 回放通过，17 张 GPU 截图无错误/跳过，
  Provider/MCP/GitHub secret canary 无泄漏，运行实例 Keyring 零残留。
- `native-2026-08-11T17-16-24-611Z`：新增 GitHub 授权取消→重启→绑定→解绑业务路径通过，Keyring
  零残留；该轮仅因 Windows 剪贴板超时和锁屏中性帧未通过视觉硬门禁。
- `equivalence-p0-2026-08-11T16-13-34-587Z`：Tauri/Native 同语料的 schema v8 权威快照与
  `businessEquivalence` 通过。
- `native-performance-2026-08-11T05-18-20-526Z`：固定语料下 7/7 性能门禁通过；后续
  `native-performance-2026-08-11T15-09-35-937Z` 的 Native Composer/resize p95 为
  16.4671/13.7735 ms。
- 当前工作树共享应用层 357 项、Native 119 项 lib 测试通过；两个目标 crate 的严格
  `cargo clippy --lib --no-deps -- -D warnings` 通过；Tauri Rust 宿主 `cargo check -p lilia --locked`
  通过（22 个既有 warning）。
- 未签名 Preview 安装器完成静默安装、独立 CLI、空格路径、并发单实例转发、覆盖更新、自动重启、
  静默卸载和 PATH 清理烟测。二进制烟测产物保留在本地 `artifacts/native-preview-smoke/`，不进入 Git。

## 尚未完成 Todo

### 正式切换阻断项

- [ ] 清理 Mutsuki Windows 全 workspace 门禁基线：当前未改动的 Rust Analyzer E2E 稳定返回
  `url is not a file`，全 workspace 严格 Clippy 仍被 Bot QQ/Web HTTP 的既有 lint 拦截；本次变更涉及的
  AgentKit package 测试和严格 Clippy 已通过。
- [x] 完成 Product Core 跨实例/外部写入的 durable change feed 订阅；`DesktopApplication` 现提供
  `start_product_change_feed` / `poll_product_change_feed`，以 `product_events` 序列为权威游标，将外部
  写入映射为类型化 `DesktopEvent`（源标识 `product-change-feed`）；Native Preview 与 Tauri event bridge
  启动时接入。消费者仍须按 ID 回读 snapshot。
- [ ] 将 GitHub 绑定/仓库目录加入 Tauri/Native 同 corpus，并补多客户端竞态、授权取消边界和真实远端
  clone 的隔离测试。
- [ ] 逐项关闭功能等价矩阵仍标记 `BLOCKED` 的差异；不能用类型、占位 UI 或单端测试替代双端行为证据。
- [ ] 在固定、解锁的 Windows 11 真机完成中文 IME、剪贴板、Explorer 拖放、100/125/150/200% DPI、
  resize、多显示器、Mica/Acrylic 回退、多窗口、休眠唤醒和 WGPU 设备恢复。
- [ ] 在干净 Windows 11 环境重跑同 revision 的 Tauri/Native 冷启动、首任务可用、千条时间线、
  Composer、Panel resize、空闲 CPU/RSS 和两套安装器体积基线。
- [ ] 配置正式 tag、Updater 公钥、签名私钥/密码，生成并验证签名安装器和更新包；当前只有未签名 Preview
  烟测包。
- [ ] 完成正式安装/更新/卸载门禁后再切换产品名、安装标识、更新通道和 `liliacode` CLI；Tauri 版保留
  一个发布周期作为回滚资产。

### 后续 IDE 能力

- [x] 在现有 Workspace Item/Pane 架构上实现项目文件树与 watcher，不把文件系统状态塞回 UI 组件。已落地
  `project_files` 服务、`project-files` Item、FS watcher、`ProjectFilesChanged`，以及 Native Left Dock /
  `project_files_panel`；文件系统、展开和选择状态继续由 `DesktopApplication` 拥有，Native 绘制与交互已消费
  NanaUI `TreeView`，目录与文件事件分别回到真实展开和 Document Item 打开路径。
- [x] 将 Buffer/Document/Language 合同接入可编辑文本 surface、保存冲突和恢复模型。已落地
  `document-editor` Item、DocumentStore open/edit/save/冲突/discard、多 view 共享 resource，以及 Native
  `HostedTextarea`。保存会在同一个 DocumentStore 临界区内校验 buffer revision 和磁盘指纹、同目录写入并
  `fsync` 隐藏 staging 文件、再次复核磁盘后原子替换，再提交 `mark_saved`；原文件权限会继承到新文件。
  这消除了同一应用多 view 并发编辑造成的“旧内容已写盘但新 revision 被标为已保存”窗口，也避免目标文件半写。
  文档 Item 可在主/辅助窗口完整渲染和编辑；dirty view 拒绝关闭，最后一个 view 关闭时才释放 DocumentStore。
  外部编辑器不共享应用锁，最终复核与替换之间仍不是跨进程强 CAS，后续系统门禁必须继续覆盖高频外部写入。
- [x] 接入真实 LSP 文档诊断第一纵切。`DesktopLanguageServiceState` 负责 Document↔LSP 绑定、buffer
  revision/version fence、UTF-16 range 映射和 DiagnosticStore；Native 打开、编辑、保存、放弃时调用同一
  `NativeAgentKitRuntime` 的共享 LSP，并只呈现当前 revision 的真实结果；问题列表项可选择实际诊断范围且
  不修改 Buffer revision。固定 Mutsuki 依赖已通过需
  `rust-analyzer` 的显式 smoke test；当前问题列表是真实诊断，但仍不代表编辑器内诊断装饰完成。
- [x] 共享 Code Index 工作区文本搜索已在 Native Coding Tools 中接通，搜索结果可直接打开受项目根约束的
  DocumentStore 文档和 `document-editor` Item；当前会转换 byte range、选中真实文本并为同文件多结果生成
  唯一目标。Native 也能在同一索引上切换 Text/Symbol mode，符号结果复用相同导航；作用域可在当前项目与
  全部活动项目间切换。跨项目路径逐项来自 Product Core 的项目工作区根，复用同一个 Mutsuki Code Index，
  结果携带项目身份与 index revision，点击时先切换权威项目再走相同 DocumentStore；最多搜索 32 个项目并
  展示 128 项命中，部分项目失败或截断会明确提示。文档导航已消费 NanaUI `HostedUiCommand::Focus`，打开结果后
  把键盘焦点移交给实际拥有该 Item 的主窗口或辅助窗口。
- [x] 共享 Git working-tree/staged diff 已在 Native Coding Tools 中接通并可真实切换。Lilia 只消费 Mutsuki 的
  `SharedGitService`；超过内联阈值的 patch 从同一不可变资源存储恢复，View 只显示明确标注的前 24 行。
- [ ] 补齐编辑器内诊断装饰、多根工作区和 Git stage/unstage/commit 工作流。NanaUI 命令系统、TreeView、增量语法
  高亮和 definition 导航已完成：Lilia 注册文件、Coding Tools、设置、侧栏、文档保存和主题业务动作，NanaUI
  `ActionRegistry`/`KeyContext`/`Keymap`/`KeymapLayer`/`CommandPalette` 负责上下文可用性、窗口级快捷键、搜索和键盘选择；
  主/辅助 Workspace Window 根层只捕获真实匹配的命令，未绑定按键继续交给文本输入与 IME；Picker 关闭后
  恢复来源 Document Item 焦点。Document Item 按 LanguageRegistry token 与深浅主题启用
  NanaUI retained highlighter，不替换 text/cursor/IME/undo 状态；Code Index/诊断范围也已改用 O(1) cursor selection。
  F12 与可见按钮从真实 cursor byte offset 发起后台 LSP 查询，以 Buffer revision 拒绝陈旧结果；`Location` /
  `LocationLink` 只允许 canonical active project root 内的文件。单目标直接在来源窗口打开/聚焦，多目标显示真实
  选择列表，目标 Item 已在其它窗口时聚焦既有 owner，不复制资源所有权。编辑器内诊断装饰仍未实现。
  Git 写工作流所需的 Mutsuki owner 候选现已用 `GitWorktreeState` 覆盖 canonical worktree 的 HEAD、index 与
  working files，并以同 worktree service-local 栅栏串行“重读→校验→写入”；外部修改与 service 重启已有功能测试。
  该 owner 改动尚未形成可固定 revision，Windows 交叉检查又受本机缺少 Windows SDK C headers 阻断，因此不能
  把候选代码计为 Native 已消费或系统门禁通过。
  当前项目/全部项目 Code Index 的真实双根测试、共享应用层 339 项、Native 102 项及两个目标 crate 严格
  Clippy 已通过；`agent-debug-runs/native-2026-08-12T14-44-44-315Z` 已构建新 Debug binary，但本机 macOS 在
  ready 前被 Windows 11 平台门禁拒绝，因此新增的高亮、focus 与 definition 控件仍无 Windows 可见回放或截图。
- [x] PTY/Terminal 第一纵切已保持进程生命周期、输出持续 drain、ANSI 屏幕、行输入、Ctrl+C/Ctrl+D、输出复制、resize、scrollback、显式终止与
  窗口所有权在应用宿主服务层；`terminal` Item 可分栏/跨窗口移动，关闭标签不终止进程，重启只恢复诚实终态且
  不重放命令。Android legacy `process_session` 已复用 task-scoped PTY；调试器、字符级键盘/鼠标协议、真实
  Android/LAN 与 Windows ConPTY 门禁仍 open。
- [x] 项目任务运行器第一纵切已接通应用层 `.lilia/tasks.json` 与 Host-owned Terminal Item。任务使用类型化
  `program` / `args` / `env`，不把整条命令隐式交给 shell；需要管道或复合语句时由项目显式配置 shell 程序。
  配置读取限制在 canonical active project root 内，限制为 256 KiB / 64 项，公开 catalog 只包含 ID、显示名、
  并发策略与活动 terminal ID，不暴露参数或环境值。启动时重新读取配置，默认拒绝同一项目任务并发运行；Native
  Coding Tools 可运行任务或打开已在运行的真实终端。共享应用层功能测试覆盖实际 PTY cwd/启动/重复门禁与
  symlink 越界，严格 Clippy 和 Tauri 兼容检查通过；可见 Agent Debug 仍受上述 Edge preflight 阻断。Debugger
  不在本纵切中，不能把“任务可运行”解释为调试能力完成。
- [ ] 为大型仓库、超长编辑会话、多根工作区和插件扩展建立独立性能及恢复门禁。

## 审计新发现 / 已关闭（原 TODO 未提及）

以下缺口不在上方「尚未完成 Todo」明文列表中，由 Tauri 命令面 vs `DesktopApplication`/Native 对齐审计发现并部分关闭：

当前验收快照（2026-08-13）：Lilia Native 已固定 NanaUI
`c0a4404b327bcd27ba7a55657437180459a9b346`。该 revision 在既有 Markdown 图片、窗口置顶、KeyCapture、
确认框 busy/自定义动作标签合同上增加了跨显示器、DPI 与 Windows 工作区感知的窗口恢复；共享应用层 357 项、Native 119 项 lib 测试、两个目标 crate
严格 Clippy、Tauri `cargo check --locked -p lilia` 和 Agent Debug 脚本语法验证通过。
`agent-debug-runs/native-2026-08-12T19-45-32-834Z` 用当前工作树重新构建 Debug binary（7.08 秒）；
macOS 宿主随后在 ready 前被 Windows 11 门禁拒绝，因此本快照只关闭代码与确定性测试差异，不替代 Windows 可见回放。
下文更早日期的测试数均是历史切片证据，不代表当前工作树。

- [x] **项目偏好持久化**（`project_get/set_settings` 的 clone 父目录与 worktree 默认项）：原先只落在 Tauri `settings_store`，Native `project_clone_parent` 为进程内临时状态。现已收口到 `DesktopApplication::project_settings` / `save_project_settings`（`desktop.project.settings.v1`）；Tauri 薄适配单写共享层（不做旧键自动迁移）；host store 仅暂存尚未迁完的 GitHub/Codex 绑定字段；Native 启动回读/编辑持久化 clone 父目录；`create_task_worktree(None)` 会消费 `worktree.parentDir`。
- [x] **会话建议设置**（`conversation_suggestions_get/set_settings`）：原先仅 Tauri store。现已提供宿主中立 `conversation_suggestion_settings` / `save_conversation_suggestion_settings`；Tauri 薄适配单写共享层（不做 legacy 双写/旧键共存迁移）；Native「模型服务」设置提供真实开关，保存后主窗口与任务弹窗会立即按同一权威重载建议。
- [x] **Popup 窗口快捷键/最近项目设置**（`popup_get/set_window_settings`、`popup_remember_last_project`）：原先仅 Tauri store。现已提供 `popup_window_settings` / `remember_popup_last_project`；Tauri 薄适配单写共享层（不做 legacy 双写/旧键共存迁移）。
- [x] **Worktree `autoInstructions` 注入**：Tauri 前端在 Composer 路径合并 `additionalContext`，Native Agent turn 原先缺失。现当任务绑定 worktree 时，共享 `turn_context` 写入 `additionalContext`（来自项目偏好）。
- [x] **跨会话标题搜索应用层合同**（原仅埋在 EQ-003「跨会话全文/向量搜索」；migration TODO 未点名）：共享 `DesktopApplication::search_sessions` 以 Product Core 任务标题为语料，复用子串命中 + 字符 bigram TF-IDF/余弦；Native 任务搜索框接入。消息体全文仍待持久化后再扩。
- [x] **跨会话标题搜索前端薄适配**：`sessionSearch.ts` 改为调用共享 `search_sessions`（去重搜索逻辑，非双端数据一致）；Native 任务列表搜索改为一次 `search_sessions` 缓存命中集，避免每行查询。
- [x] **Hooks/Skills/MCP/Plugin 注册表外部文件变更订阅**（原埋在 EQ-015/016「外部文件变更」；migration TODO 未点名）：`start_registry_file_watch` 监视用户 hooks/skills/mcp/plugins 文档及项目 `.lilia/hooks.json`，发出 `HooksRegistryChanged` / `SkillsRegistryChanged` / `McpRegistryChanged` / `PluginsRegistryChanged`；Native Extensions 收到后真实刷新。
- [x] **Hooks 逐 Handler 结构化编辑**（原仅 Tauri Hooks 页具备）：Native Extensions 现按 Handler 提供 event、matcher、type、timeout、command、Windows command 与 status message 的新增、编辑和删除，并通过共享 `DesktopHookHandlerUpdate` 校验/持久化；既有高级 JSON 入口继续保留。功能测试覆盖新增、类型化编辑、非法 timeout 与删除，最新 Native 102 项、共享应用层 339 项及双 crate 严格 Clippy 已通过；`native-2026-08-12T14-44-44-315Z` 的 Debug binary 在 macOS ready 前被 Windows 11 门禁拒绝，因此仍缺 Windows 可见表单回放。
- [x] **Model feature / Assistant AI / Router mode 设置权威**：原先仅 Tauri `provider` store。现已收口到 `DesktopApplication`（`desktop.model-feature.settings.v1` / `desktop.assistant-ai.settings.v1` / `desktop.router-mode.settings.v1`，带 revision）；Tauri 薄适配**单写共享层**（不做 legacy store 双写/迁移；Native 与 Tauri 数据目录独立、不考虑共存）；凭据仍走 Keyring，不进设置正文。Assistant AI 的 `/models` 拉取、Base URL/凭据校验、响应大小限制与默认模型匹配也已下沉到共享应用层，Tauri 同名命令只做 DTO 映射；Native 可手动增改模型池显示名、异步获取模型并按 ID 合并且保留本地显示名，也可测试连接，网络结果仅更新草稿，仍需用户保存才持久化。Native「模型服务」还可编辑自动标题、对话建议、Prompt Router、Prompt Optimize、自动回合决策模型及 Fast/Default/Plan/Review 的模型和思考强度，并支持自定义预设的创建、改名、模型/强度编辑与删除。主窗口和辅助任务窗口 Composer 均可从运行时默认、启用预设、聊天层级和模型池形成的同一候选集手动选择模型与思考强度；手动选择以一个 revision-safe 命令同时持久化两项，`auto` 会同时清除，提交时共享应用层优先产出 `manual` 决策，不再被自动分流覆盖。关闭 LLM 自动决策时共享应用层仍按同一 contracts manifest 做确定性预设分流，开启时控制模型 override 与最终预设均进入真实 Agent request。Tauri 与 Native 主/辅任务面现调用同一两阶段提示分类/优化服务；Native 以 Composer revision CAS 应用异步结果，不覆盖优化期间的新输入。分类返回 typed workflow 时，Native 与旧 Tauri 一样先显示「应用 / 忽略」建议，只有用户显式应用才进入发送；已应用 workflow 随 Composer 与 Guide 持久化、进入 direct turn 和 FIFO Guide request，重启后继续显示真实发送状态并可取消。设置权威、探测/模型池、手动选择与自定义预设的代码路径已接通；`2026-08-12T11-47-02-722Z` Agent Debug 在启动应用前因无法探测 Microsoft Edge 版本而 `blocked`，没有生成可见 UI 回放或截图，因此 Windows 双宿主可见回放仍未完成，不能据此宣称完整 UI 等价。
- [x] **会话建议生成/源探测全量迁入**（`conversation_suggestions_get` / `get_sources`）：GitHub / local-git / scope / cache / generation 收口到 `DesktopApplication`（Product Core 任务+todos+timeline + 共享 GitHub 绑定）；Tauri 仅薄适配与 ModelPort；Native 只在未落库、空内容且绑定项目的新对话草稿中消费建议，选择后写入同一个 transient Composer。既有会话清空输入不会显示“新对话建议”。
- [x] **未发送新对话草稿生命周期**：Native 主侧栏、全局快捷键、会话状态窗、子对话和时间线选中文本提问统一使用 host-owned transient draft；主窗口与辅助窗口复用同一草稿 renderer 和共享 Composer reducer。关闭草稿或切换项目/任务不会创建 Product task 或 conversation，首次发送才调用 `materialize_task_draft`，保留项目、父任务、引文内容、附件和 Composer 模型/权限/Plan/Goal 状态，并在原 surface 晋升为正式任务。功能测试证明 materialize 前任务不存在、晋升后只出现一个 Draft task 且 Composer 状态完整；Native Debug 已加入主窗口输入后关闭零 task、辅助窗口关闭零 task和首次发送只增一个 task，P0 双宿主脚本也会比较未发送草稿前后的 Product task/conversation 快照。
- [x] **会话状态悬浮窗控制**：Native 已补齐 Tauri 的新对话、模型服务状态、运行时置顶、透明度及窗口 geometry 持久化；新对话复用上述 transient draft，不再因打开或关闭窗口留下空任务。置顶能力来自 NanaUI owner 的类型化 `HostedWindowCommand::SetAlwaysOnTop`，业务偏好仍由 Lilia 持有。
- [x] **跨显示器/DPI 窗口恢复**：NanaUI `HostedWindowPlacement` 在主窗口和所有辅助窗口创建前统一按当前显示区域归一化 restore bounds；保留有效负坐标副屏，断开屏幕时回主屏居中，按持久化 source scale 适配 DPI，并在 Windows 使用 `rcWork` 排除任务栏。Lilia 只保存 geometry、scale、maximized 与 Workspace topology；旧状态缺少 scale 时按 1.0 兼容。NanaUI 177 项全 feature 测试、严格 Clippy 与 Windows MSVC 目标交叉检查，以及 Lilia Native 119 项测试均通过；真实多屏/DPI 视觉回放继续保留为 EQ-024 系统门禁。
- [x] **Title update coordinator**：`DesktopTitleUpdateCoordinator` 与 `spawn_title_update_after_turn` 在应用层；turn Completed 后自动调度；成功结果以稳定事件 ID 写入 Product 时间线，手动标题建议继续使用 durable Pending projection。Tauri `spawn`/`respond` 只调用共享权威，不再保留独立模型请求、并发队列、legacy SQLite 回退或时间线写入；标题 source 存 agent-runtime，不双写 Tauri legacy tasks 表。
- [x] **AskUser 完整交互合同**：Native 不再把 `ask_user` 压成单文本框或点击即提交的单值；主窗口和任务弹窗
  共享结构化状态机，按 `questions` 支持 confirm/single/multi、min/max selection、Other 自定义回答、多题前进/
  返回、跳过、dismissable 约束和选项说明/预览，最终提交与 Tauri 相同的 `answers` / `cancelled` 结果。功能测试
  覆盖多题、多选上限、Other、返回恢复与不可关闭；Agent Debug 脚本已加入真实两题流程，但本机仍在 Windows ready
  前停止，尚无可见回放。
- [x] **剪贴板附件权威与显式 Native 粘贴**：文件路径描述、原始编码图片和长文本缓存已收口到共享
  `DesktopApplication`；Tauri 命令只解码 DTO 后薄调用共享权威。Native 主窗口与任务弹窗均可显式粘贴文件、图片和
  文本；短文本进入 Composer，达到旧版 2000 个 UTF-16 单元阈值的文本写入有界 `.txt` 附件。共享功能测试覆盖格式、
  字节内容、阈值与空载荷；Native 119 项、严格 Clippy 和 Tauri locked check 通过。
- [ ] **焦点感知 Ctrl/Cmd+V**：NanaUI owner `c789695` 已让稳定 ID 的焦点控件随 `HostedWindowEvent::KeyPressed`
  投影，`1b8087d` 接通 Iced 系统剪贴板请求，`50c25b7` 又允许应用只为明确焦点的富输入动作停止当前按键传播；
  三者共同避免普通输入框失去标准粘贴，或任务页其它输入框获得焦点时误写 Composer。该 owner series 尚未发布，
  Lilia 继续固定已发布的 `c0a4404b327bcd27ba7a55657437180459a9b346`。发布后只需在三个 Composer Input 复用
  同一稳定 ID 和剪贴板分流，显式粘贴入口在此期间保持可用。
- [x] **更新下载进度与不可中断状态**：共享 `DesktopHost::execute_update` 以可选进度回调保留旧 Host 默认实现，
  Native Host 对更新包做有界流式读取并发布单调进度；设置页和全局更新框持续显示下载/安装/重启状态。NanaUI
  `ConfirmDialog` busy 合同会禁用确认、取消、关闭与 outside dismiss，失败后恢复真实重试。共享应用层测试覆盖乱序/
  重复进度过滤；正式签名更新、下载中断和安装回滚仍属于 Windows 发布门禁。
- [x] **最终回复“应用建议”**：Native 主任务面与辅助任务窗口现在只为成功的 assistant 回复、稳定来源 Turn 及 `review` / `fix_suggestion` 来源显示真实动作；点击后提交带完整 `sourceTurnId` / `sourceKind` / `sourceSummary` 的 `lilia_batch_apply`，运行中、存在阻塞交互或 task run block 时不显示。现有复制与回复呈现保持不变。
- [x] **手动上下文压缩**：`lilia_compact` 已成为共享类型化 workflow；Tauri 与 Native 主/辅任务面调用同一 `DesktopApplication` 事务，以 Mutsuki `TranscriptContextWindow` 生成有界摘要输入，真实 control model 产出持久摘要，新 AgentKit session、完成事件与 Product binding 在成功路径收口。模型空结果/失败与取消不会替换旧 binding，旧 durable session 保留为不可变来源。
- [ ] **ContextCompactionCoordinator 自动高层 summarizer**：Mutsuki owner 工作树已把 profile policy → AgentLoop →
  Context runner → ModelGateway → coordinator 两阶段请求接入真实 turn；语义摘要使用不可变 transcript
  `ResourceRef`，失败回退确定性摘要，usage/cost 进入 run budget，相同 source hash 有界复用，resume 只在真正
  进入下一次主模型前压缩，durable transcript 不被替换。该 owner 变更尚未发布为 Lilia 可固定 revision，
  当前 Lilia pin 也尚未注册产品 profile 的 context policy，因此本项继续保持 open；手动产品动作不能代替它。
- [ ] **MCP elicitation 真机 E2E**（EQ-008）：应用层 Form/URL 合同已在，剩余真实 MCP server 发起与 Windows 门禁。
- [x] **IAB 侧栏**：NanaUI owner 已发布 `HostedBrowser`、typed load/title/failure 事件和 `LayoutProbe`，Lilia Native
  当前固定 `c0a4404b327bcd27ba7a55657437180459a9b346`（包含原 HostedBrowser release）并以右侧持久 Dock 接入。URL 草稿与当前 URL、标题、加载/
  错误状态由应用拥有；真实 Iced bounds 驱动 child WebView attach/resize，Dock 切换驱动 visible，Enter 与按钮均执行
  真实 navigate。Native 102 项、共享应用层 339 项及双 crate 严格 Clippy 已通过；Windows Agent Debug 已增加本地
  fixture 导航、WebView ready 与截图回放，仍待 Windows 环境执行。
- [x] **IAB 独立会话窗口与结果提交**：同一任务复用独立 NanaUI HostedWindow，窗口内 child WebView 的 URL、标题、
  备注和加载状态均为应用状态。NanaUI `HostedWindowCommand::CapturePng` 在 Windows 以 GDI 捕获完整窗口并返回类型化
  成功/失败事件；其它平台沿同一失败事件生成诚实的 `metadata_only` 结果。共享 `DesktopApplication::submit_iab_snapshot`
  将页面信息和可用截图附件提交为同任务的持久 Agent turn；空闲时立即启动，运行中按既有 durable FIFO 排队。这取代
  旧 Codex 子进程 stdin 特例，并修复旧 Tauri 非 runner 分支只返回 `delivery=message`、实际未写入对话的问题。NanaUI
  owner 的 macOS 全特性/严格 Clippy 与 Windows `hosted,browser` 交叉检查通过，Lilia 双 crate 严格 Clippy、339/102 项
  测试通过；完整 Lilia Windows 交叉检查在本机因缺少 MSVC/Windows SDK C headers 阻断，真实截图与提交回放仍须在
  Windows 11 Agent Debug 门禁执行，不能据此关闭系统等价项。
- [x] **IAB 截图附件生命周期**：Native 新捕获文件写入明确的 durable attachment 目录；关窗、窗口打开失败、
  owner 已消失或提交失败时会删除从未进入消息的截图，捕获成功事件与关闭命令竞态也按 capture id 回收。成功提交后
  被持久 turn 直接引用的附件不会被缓存清理误删；长期消息附件保留策略应由统一 attachment owner 负责。
- [x] **Legacy process session 的 Native/Android 适配**：Host-owned PTY/Terminal Item 已承接本地终端；Android
  `chat.send.runtimeCommand` 的 `spawn` / `write_stdin` / `kill` 现映射到 task-scoped 真实 PTY，并通过
  `tasks.get.runtime.processSessionId` 回读活动会话。服务端不信任客户端 cwd，固定 task worktree/项目根，校验
  Product 任务状态/依赖图，并只接受旧协议的 `:workspace` 标记；PTY 仍是已配对设备触发的当前用户 Shell，
  不宣称文件系统沙箱。能力协商已返回 `supportsProcessSession=true`。旧 Tauri Node runner JSONL 路径保留给旧宿主，
  不迁入共享业务层。真实 Android/LAN 与 Windows ConPTY 系统回放仍是 EQ-019/EQ-022 的验收阻断项。

IDE 纵切进度（与上方「后续 IDE 能力」同步）：项目文件树 + watcher、可编辑文档面
（open/edit/save/冲突）、共享 LSP 真实诊断列表、Code Index 文本搜索→文档 Item，以及共享 Git 只读 diff
已完成；Code Index range 已用 O(1) selection 选中真实文本并把焦点移交到实际窗口，增量语法高亮和 LSP definition
可见导航也已接通。编辑器内诊断装饰、多根工作区、Git 写工作流和调试器仍 open；
本地 Host-owned PTY/Terminal 与应用层项目任务运行器第一纵切已完成。NanaUI 的命令系统、TreeView、PaneTree、
HostedBrowser、HostedWindow PNG capture、增量 syntax highlighter 及 O(1) cursor/selection 恢复已发布并由 Lilia 固定到同一 revision；当前
HostedBrowser、窗口捕获、命令系统、TreeView、PaneTree、syntax highlighter 与 cursor/selection 已在产品路径实际接入；
PaneChrome 已承接主窗口与辅助窗口的活动/非活动 Pane；辅助窗口的项目身份、移回主窗与关闭窗口动作已独立到 PaneTree 之外，
Document/Terminal/Project/Application/Task Item 共享同一套标签与 capability-driven Pane 动作。

## 切换纪律

产品定位是**独立产品线 → 完成后替代**，不是长期 Tauri↔Native 数据双向兼容工程。

正式切换阻断项未全部关闭前，只发布 `LiliaCode Native Preview`：

- 不得覆盖正式 LiliaCode 数据目录；不得静默自动迁移或删除旧数据。
- 开发 / Preview 阶段不必做 Tauri↔Native 数据目录双向兼容、旧键迁移或为共存而双写。
- 旧数据仅由用户显式导入进入独立 Preview 目录；正式切换后原生目录成为事实源。
- 不得让 Native 接管正式更新通道和 CLI；也不得以历史截图替代当前 revision 的系统门禁。
- 迁移期 Tauri 可继续薄调用 `DesktopApplication`，但新偏好/合同以共享层单写为准。
