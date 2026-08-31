# NanaUI API 缺口提案

对照：LiliaCode 桌面（pin `nana-ui` `d3cdc1c638730583a8b2c0fb04907c490c4144de`）、聊天桌面（Claude / ChatGPT）与 IDE 工作台（VS Code / Cursor）。权威文档：该 pin 的 `docs/rust-layout.md`、`docs/components.md`、`docs/workspace.md`。

不修改 cargo git checkout。下列「建议 API」是给 NanaUI 仓库的提案草稿。

## 分类

- **A 已有、应用未用或用了平行实现**
- **B 有控件但能力不完整**
- **C 正常 GUI 有、NanaUI 无（真缺口）**

## A. 已有能力

| 能力 | NanaUI API | Lilia 现状 | 建议 |
| --- | --- | --- | --- |
| Flex 预设 | `Stack::{row,fill_row,column,fill_column,bar}` + `outline` | 已全部迁移 `Stack` 预设（`runtime_layout.rs`） | 已完成；`reconcile_children` 继续负责子节点对账。 |
| 描边卡片 | `Card` + `CardKind::Outlined`，或 `Stack.surface.outline.radius` | `composer_card()` 已是 `Stack` 三件套 | 已完成；重复卡片再升 `Card`。 |
| 桌面壳分区 | `DesktopShell` navigation / primary / inspector / bottom | 已用 | 继续。不要再手写四栏。 |
| 工作区区域 | `Workspace` / `WorkspaceModel` / 可折叠 inspector | `DesktopShell.from_model` 已接 model；内容仍是自拼 column | 项目页批次再绑 Region 内容。 |
| Dock 拆出 | `Dock` / `DockWorkspace` / `DockController` | 应用层有 `DockSlot` 与分栏状态，**不用** Runtime `Dock` 控件 | 完整 IDE 停靠列入后续；先 `PaneChrome`。 |
| 分栏控件 | `SplitPane` | 应用用 `SplitPaneController` 算尺寸，壳层按钮触发 | 工作区批次把 sash 交给 `SplitPane` 控件。 |
| 浮层 | `Dialog` `ConfirmDialog` `Drawer` `Popover` `ActionMenu` `ContextMenu` `CommandPalette` | 确认框、命令面板、ActionMenu、ContextMenu 已用。**未用 Drawer / 通用 Dialog** | 窄宽度检查器可用 `Drawer`。AskUser 保持对话内卡片，不改成模态。 |
| 锚定菜单 | `AnchoredActionMenu` | 标题栏/侧栏用 `ContextMenu::new(x, y)`，更多菜单曾落到 `(420, 48)` | 后续改锚定触发槽，禁止写死坐标。 |
| 提示 | `Tooltip` / `IconButton::with_tooltip` | 图标按钮已接默认 tooltip | 保持。 |
| 状态徽标 | `StatusBadge` | 待处理/运行态靠文字或侧栏图标 | 任务行与待处理卡可用徽标，避免自绘 ✓/○。 |
| 空状态 | `EmptyState` | 自动化已用；对话空状态是大号 `Text` | 对话空状态可改 `EmptyState`。 |
| Toast / 进度 | `Toast` `Progress` `Spinner` `Skeleton` | 未用 | 长操作（clone、导入、更新）用框架反馈，不自画「处理中」按钮文案。 |
| 下拉 | `Dropdown` `SearchDropdown` `Select` | 模型选择是 pill/按钮循环 | 模型/权限改下拉，避免 Cycle 按钮。 |
| 虚拟化 | `AppContext::materialize_virtual_*` | 主窗口时间线已走 `materialize_virtual_list`；弹窗仍挂最近 24 条 | 弹窗可后做同一模式。 |
| 设置页 | `SettingsSidebar` `SettingsPage` `SettingsRow` | 已用 | 保持。 |
| 底栏区域 | `DesktopShell::bottom` | 仅诊断列表；产品 `DockSlot::Bottom` 未绑 NanaUI region | 终端/问题面板走 bottom region 或 Dock，不当成状态栏。 |

## B. 不完整

| 正常 GUI | 当前 API | 缺口 | 应用能否绕过 | 建议 API |
| --- | --- | --- | --- | --- |
| `Stack::row` 默认起点对齐 | `Stack` 预设统一默认起点对齐 | 历史平行实现与框架预设不一致，工具条容易排反 | 平行实现已删除，`Stack::row` 是唯一水平预设 | 无需新 API；已完成 |
| 侧栏任务行：拖拽排序 + 行内停止/菜单 | `ReorderItem::tools` + live 行子节点 | 已接到 Lilia 侧栏；跨列表拖到收集箱仍不能一次完成 | 任务/运行中会话可拖；项目是 drop target | 无需新 API |
| 补全列表贴着输入框 | `ActionMenu` / `Popover` 可锚定；应用把补全做成 Composer 内 `column` | 键盘上下 + 锚定宽度跟输入框走不完整 | 继续槽位列表 | `Popover`/`SearchDropdown` 支持 textarea 插车锚点与 Arrow 导航 |
| 代码编辑器 | `TextArea` + 可选 `syntax-highlighting` presenter | 无行号、诊断沟、 minimap、多光标 | 继续 `TextArea`，诊断放 bottom | `TextEditor`：gutter、diagnostics、revision 仍由应用拥有 |
| 终端 | 无；应用用只读 `TextArea` + 底栏输入 | 无 PTY 单元格、选区、resize 事件模型 | 继续应用终端快照 | `TerminalView`：rows/styles、resize(cols,rows)、submit/interrupt |
| Markdown 图与公式 | `NativeMarkdown` 给出 mermaid/math 槽，**不渲染** | 槽要宿主自己画 | 继续纯文本/代码围栏 | 官方 presenter 或明确「宿主 GPU 槽」示例 |
| 架构/自动化图 | `GraphCanvas` 画网格、框、边 | 节点内部 UI 要应用塞子节点 | 继续 | 保持；文档写清节点内容合同即可 |

## C. 真缺口（相对正常 GUI）

| 正常 GUI | 建议 API | 为何是框架而不是 Lilia | 应用暂绕过 |
| --- | --- | --- | --- |
| VS Code 状态栏：左 Git/诊断，右缩进/编码 | `StatusBar` 槽（leading/trailing），高度进 `UI_METRICS` | 多应用重复的壳层 chrome | `DesktopShell.bottom` 只放诊断列表 |
| 聊天输入的附件拖放目标 | `DropTarget` 事件：文件 URI 列表 | 拖放命中与系统剪贴板同属输入适配器 | 只用「添加文件」按钮 |
| 多条待处理队列（badge + 列表） | 不必新控件；缺的是 `Tabs`/`List` 上的 overflow badge 模式 | 若变成通用「inbox strip」再升框架 | 一次只投影当前 pending |
| 检查器工具窗标签（Problems / Todo / Browser） | Dock pane 标题条 + tab；部分已在 `DockPanel` | 应用不该自绘第二套 tool window chrome | 第一批：单表面 + 关闭。后续用 Dock |
| 可聚焦分区环（F6 在侧栏/编辑器/终端间跳） | `Workspace` 焦点环 API | 分区焦点属于壳 | 继续单焦点控件 |
| 面包屑 | 标题栏槽位 | 应用手写 `Text` 面包屑 | 继续手写短面包屑 | 若多产品需要，再升 `Breadcrumb` |
| Agent 补丁 diff / merge | 无 | Cursor 式行内 diff 属于编辑器平台 | 时间线用 markdown 摘要 | `DiffView`：hunk、接受/拒绝，buffer 仍由应用拥有 |

## 第一批对 Lilia 的约束

1. 新容器用 `Stack` 预设，不手写 `LayoutStyle` 布局字段。
2. 边框必须 `outline(role, width)`。
3. 浮层只用已有 overlay 控件，不绝对定位。
4. 不在 app 里实现 Dock 算法、终端仿真或代码编辑器。
5. 真缺口只记录在本文件，不在 UI 上放「即将推出」。

## 建议提交给 NanaUI 的优先序

1. **TextEditor gutter/diagnostics** — 挡住文件页。
2. **TerminalView** — 挡住终端页。
3. **DropTarget** — 挡住 Composer 拖入文件。
4. **DiffView** — 挡住 Agent 补丁审阅。
5. **StatusBar 槽** — 体验，可后做。

应用侧先修、不必等 NanaUI 的：`Stack` 替换新容器（pending/检查器顶栏已做）、菜单锚定槽而不是 `(420, 48)`、对话与 IAB 检查器 `EmptyState`（已做）、图标 `IconButton::with_tooltip`、时间线 `materialize_virtual_list`（已做）。

Lilia 已钉 NanaUI `d3cdc1c63`：`IconButton::with_tooltip`、侧栏 `ReorderList` + `ReorderItem::tools`。

每条提案应对 NanaUI：现有类型、缺的方法/事件、应用侧绕过、以及一个最小 example。
