# Agent 入口规范

<!-- CODEGRAPH_START -->
## CodeGraph

仓库根目录存在 `.codegraph/` 时，理解或定位代码应先使用 `codegraph_explore` / `codegraph_node`，或 shell 中的 `codegraph explore` / `codegraph node`，再使用文本搜索。
<!-- CODEGRAPH_END -->

## 项目级 Skills

本仓库通过 `.agents/skills` 提供应用开发、边界、验证、设计、Git 与 Agent Debug 工作流。使用 Skill 前必须完整读取对应 `SKILL.md`；若 Skill 仍描述已删除的旧宿主或工具链，以本文件和当前 Cargo workspace 为准，不恢复旧入口。

## 仓库边界

- LiliaCode 是 Cargo-first Native 仓库；`apps/desktop` 是唯一 NanaUI/WGPU 桌面实现。
- `crates/lilia-desktop-application` 持有宿主无关的桌面应用服务，`crates/lilia-contracts/contracts` 持有 Rust 消费的产品契约。
- `apps/service` 和 `apps/android` 继续保留；Android 平台构建使用 Gradle/JDK，其任务编排仍由 `cargo xtask android ...` 负责。
- `liliacode` 命令行参数由 `apps/desktop` 处理，只用于启动、项目打开、显式导入、任务 handoff 与单实例转发；不建设 TUI、Agent CLI 或第二套 Agent 宿主。
- Markdown 是仓库内文档，不构建或发布文档站。
- 禁止重新引入 Web 桌面宿主、JavaScript 包管理器或编辑器扩展实现。

## 运行入口

从仓库根目录执行：

```bash
cargo run
cargo xtask verify
cargo xtask agent-debug
cargo xtask performance
cargo xtask android doctor|test|build|smoke
cargo xtask release windows --tag vX.Y.Z
cargo xtask installer-smoke --tag vX.Y.Z
```

需要单独检查时可直接使用标准 Cargo 命令；不要增加第二套任务编排脚本。

## 硬约束

- 修复问题先定位根本原因，优先选择更简洁且职责清晰的方案。
- 禁止在 UI 显示技术说明，禁止提供未接入的可见操作。
- 不添加低价值测试或仅硬匹配日志、字符串的测试；测试必须验证功能行为。
- 不覆盖用户或其他 Agent 的已有改动，不创建临时分支或工作树。
- 未经用户明确确认，不提交、不推送、不创建 PR。
- 通用 Native 控件和窗口基础能力属于 NanaUI；LiliaCode 只维护产品业务、应用服务、宿主装配和应用状态。
- 跨边界数据先更新 `crates/lilia-contracts/contracts` 及其 Rust API，再同步消费者。
- 需要长期记录的架构背景与取舍写入 `docs/design/`，代码中不保留复述型注释。

## Native 身份与数据

- 正式产品名为 LiliaCode，二进制/CLI 为 `liliacode`，凭据身份为 `liliacode`。
- 统一使用 `LILIA_HOME`，默认目录为 `~/.lilia`。
- 桌面端只使用共享 Product/Agent 权威数据，不得打开或写入 legacy `db/lilia.db`。
- 旧数据只能通过显式导入进入正式 home；不得自动合并、双写或删除用户数据。

## 验证

- 按风险选择最小有意义验证；文档或注释改动不要求运行桌面系统门禁。
- 默认仓库门禁为 `cargo xtask verify`。
- UI 主路径、Agent runtime、持久化、权限、构建配置、跨端契约或用户关键路径的大型改动，需要运行 `cargo xtask agent-debug`；证据位于 `agent-debug-runs/lilia-*`。
- 性能相关改动运行 `cargo xtask performance`，分别报告绝对阈值与历史基线结果。
- Android 逻辑运行 `cargo xtask android test`；需要真实设备时显式运行 `cargo xtask android smoke`。
- Windows 发布运行 `cargo xtask release windows --tag <tag>`，随后运行 `cargo xtask installer-smoke --tag <tag>`。
- 外部环境阻塞时写清缺失的 NSIS、ADB、Android SDK、GPU/窗口能力或签名配置，以及产物路径与剩余风险。

## Git 提交

- 提交标题使用中文短句概括结果，正文仅列必要改动。
- 提交前检查本次范围的 diff，确保不夹带用户或其他 Agent 的修改。
- 涉及重构或公共模块时，检查并删除重复分支、无效辅助函数和代码复述型注释。
