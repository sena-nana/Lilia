# LiliaCode Native Preview 迁移状态

更新时间：2026-08-12

本文是 NanaUI 迁移的交接清单。详细功能判定以
[`nanaui-feature-equivalence.md`](./nanaui-feature-equivalence.md) 为准，宿主实现与验证记录见
[`nanaui-native-preview.md`](./nanaui-native-preview.md)，后续 IDE 边界见
[`nanaui-native-ide-architecture.md`](./nanaui-native-ide-architecture.md)。

## 目标

- 保留现有 Tauri 版，同时交付独立的 `LiliaCode Native Preview`。
- 用 UI 无关的 `DesktopApplication` 统一产品、AgentKit、持久化和宿主合同，使 Tauri 与 NanaUI
  使用同一业务事实，而不是维护两套实现。
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
- [x] 项目和任务的创建、编辑、固定、排序、移动、归档/恢复、项目移除事务、目录选择及可取消 Git
  clone 已接入真实 Product Core。
- [x] GitHub Device OAuth、OS Keyring token、分页仓库目录、private/public 选择、解绑与认证 clone 已
  落地；token 不进入 URL、参数、SQLite、日志或调试观察。
- [x] 任务时间线分页/虚拟化、Composer 草稿与附件、队列/打断、斜杠命令、`@` 文件、`#` 会话、
  权限、提问、审批、Plan、Goal/Todo、Worktree 与 Agent 流事件已接入真实运行时。
- [x] 原生 Markdown、代码块、表格、KaTeX、Mermaid、可复制文本、Automation Canvas、配额图、架构图和
  配对二维码均走 NanaUI/WGPU/Rust 路径，不使用隐藏 WebView。
- [x] Roadmap、Memory、Provider、Agent 设置、Skills、Plugins、Hooks、MCP、Automation、Remote、Quota、
  Architecture 和 Coding Tools 基础能力已形成真实可操作 surface。
- [x] 显式旧数据导入支持预览计划、只复制不删除、逐项报告、失败重试和单独凭据确认；秘密始终通过
  OS Keyring 迁移。
- [x] 托盘、全局快捷键、单实例、CLI task handoff、更新、NSIS 安装/覆盖更新/卸载与独立 PATH 已落地。
- [x] 开发态稳定目标、观察/点击/输入协议、真实 WGPU 截图和 secret canary 扫描已接入
  `verify:native-agent-debug`；Release 会检查调试标记排除。
- [x] NanaUI 固定到完整 revision `3a6d819d3a8d835110a9790231df7172391cc9f3`，Mutsuki 固定到
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
- 共享应用层 260 项、Native 80 项 Rust 测试通过；两 crate 的 `cargo clippy --all-targets -- -D warnings`
  通过；Tauri Rust 宿主 `cargo check -p lilia` 通过。
- 未签名 Preview 安装器完成静默安装、独立 CLI、空格路径、并发单实例转发、覆盖更新、自动重启、
  静默卸载和 PATH 清理烟测。二进制烟测产物保留在本地 `artifacts/native-preview-smoke/`，不进入 Git。

## 尚未完成 Todo

### 正式切换阻断项

- [ ] 清理 Mutsuki Windows 全 workspace 门禁基线：当前未改动的 Rust Analyzer E2E 稳定返回
  `url is not a file`，全 workspace 严格 Clippy 仍被 Bot QQ/Web HTTP 的既有 lint 拦截；本次变更涉及的
  AgentKit package 测试和严格 Clippy 已通过。
- [ ] 完成 Product Core 跨实例/外部写入的 durable change feed 订阅；当前 Native 的进程内
  `DesktopEvent` 已接通，但外部变更仍需手动或页面级刷新。该项已完成设计审计，尚未写入代码。
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

- [ ] 在现有 Workspace Item/Pane 架构上实现项目文件树与 watcher，不把文件系统状态塞回 UI 组件。
- [ ] 将 Buffer/Document/Language 合同接入可编辑文本 surface、增量语法/诊断、保存冲突和恢复模型。
- [ ] 接入 LSP Workspace、全局/符号搜索、Git diff/commit 工作流和可复用命令系统。
- [ ] 新增 PTY/Terminal Item、任务运行器和调试器时保持进程生命周期、权限、输出背压与窗口所有权在宿主
  服务层。
- [ ] 为大型仓库、超长编辑会话、多根工作区和插件扩展建立独立性能及恢复门禁。

## 切换纪律

上述正式切换阻断项未全部关闭前，只发布 `LiliaCode Native Preview`。不得覆盖正式 LiliaCode 数据目录，
不得自动迁移或删除旧数据，不得让 Native 接管正式更新通道和 CLI，也不得以历史截图替代当前 revision 的系统门禁。
