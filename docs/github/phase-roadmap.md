# LiliaCode v1.0-v3.0 GitHub Planning Source

This document is the source for the GitHub milestones and phase issues. It mirrors the public roadmap while keeping issue text short enough to use as development todos.

## Milestones

### v1.0 稳定化收尾

目标：让当前首发 beta 版本可信可用，不扩展新的大功能面。重点是核心对话链路、当前任务/侧栏/roadmap/memory 基线、插件与工具管理面、IAB/斜杠命令、发布链路，以及 Android 远控 beta 的实验性边界与验证。

### v2.0 会话工作台与团队模式

目标：把 LiliaCode 变成日常会话级工作台。重点是更大操作区、完整侧边栏、产物/Memory/文件树/Git 入口、任务创建/依赖/分配、多 Agent 写作、团队模式、自动编排主链路和对话级功能补全。

### v3.0 项目级知识资产

目标：把项目层级信息从辅助页面升级为长期可维护资产。重点是项目高级度量、架构图、美术/设计原则、项目 Memory 高级检索、Roadmap/Milestone 高级解释、自动化高级能力和远控/多设备协作。

## Issues

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] 发布准入与回归清单
labels: release,area: docs
-->
### [v1.0] 发布准入与回归清单

#### 要做什么

完成 Windows 首发发布前的准入清单，让 release workflow、release notes、安装验证和已知限制保持一致。

#### 任务清单

- [ ] 核对版本号、tag、release workflow 和 release notes 模板。
- [ ] 保留签名链路、无自动更新、仅 Windows 安装包等已知限制。
- [ ] 完成安装、启动、命令行入口和卸载验证记录。

#### 完成条件

- [ ] 发布说明可直接用于 draft Release。
- [ ] 关键发布风险都有明确处理结论。

#### 当前准入落点

- `yarn release:check --tag <tag>` 自动核对版本号、Tauri Windows 打包配置、NSIS CLI 安装 hook、release notes 已知限制、安装包命名预期和 Windows 安装验证记录入口。
- `yarn release:smoke:windows --tag <tag>` 可对 Windows draft Release 安装包重复执行安装、启动、`liliacode <测试项目路径>` 和卸载后的 CLI 清理 smoke，结果写入 Release 正文的 Windows 安装验证记录。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] 核心对话链路稳定化
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v1.0] 核心对话链路稳定化

#### 要做什么

稳定发送、继续、恢复、时间线展示、权限交互、计划确认和文件上下文等首发关键路径。

#### 任务清单

- [ ] 覆盖 Native AgentKit（Mutsuki）的普通发送、继续历史会话和任务恢复路径。
- [ ] 核对 timeline、pending action、permission approval 和 plan confirmation 展示。
- [ ] 确认文件上下文、图片附件和失败提示不会破坏主链路。

#### 完成条件

- [ ] 首发核心对话路径有最小必要回归验证。
- [ ] 已知失败模式有用户可理解的错误或诊断落点。

#### 当前回归落点

- Agent Debug v1.0 核心对话回归覆盖普通发送、继续历史会话、任务恢复、plan pending action 和 permission pending action；运行 `yarn verify:agent-debug` 后查看 `agent-debug-runs/<timestamp>/scenario-results.json` 与对应截图。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] 当前侧边栏与任务列表稳定化
labels: enhancement,area: desktop,area: tauri
-->
### [v1.0] 当前侧边栏与任务列表稳定化

#### 要做什么

稳定当前任务列表、置顶、归档、父子任务、依赖展示、阻塞提示和工作树相关基础操作。

#### 任务清单

- [ ] 核对 grouped / unified 任务列表展示和切换。
- [ ] 核对置顶、归档、弹出窗口继续、子对话入口和工作树合并删除入口。
- [ ] 确认父子任务、依赖和阻塞状态不会造成列表错乱。

#### 完成条件

- [ ] 当前侧边栏可以作为 v1.0 的稳定任务入口。
- [ ] 不把 v2.0 完整侧边栏能力提前塞进 v1.0。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] Roadmap/Milestone 当前链路稳定化
labels: enhancement,area: desktop,area: tauri,area: docs
-->
### [v1.0] Roadmap/Milestone 当前链路稳定化

#### 要做什么

确保项目路线图、里程碑、任务关联和首发进度说明可以作为当前项目管理入口使用。

#### 任务清单

- [ ] 核对 milestone 创建、状态、描述、日期、排序和删除。
- [ ] 核对 milestone 与任务关联和项目任务加载。
- [ ] 核对文档中的阶段目标与 GitHub milestone 描述一致。

#### 完成条件

- [ ] Roadmap 页能表达 v1.0 当前状态和后续阶段。
- [ ] GitHub milestone 与仓库文档没有明显冲突。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] Memory Layer 1 基线稳定化
labels: enhancement,area: desktop,area: tauri,area: docs
-->
### [v1.0] Memory Layer 1 基线稳定化

#### 要做什么

稳定手动保存、用户级/项目级记忆展示、会话启动基线注入和文档边界。

#### 任务清单

- [ ] 核对用户级和项目级 Memory 的创建、查看和删除。
- [ ] 核对会话启动时 Layer 1 基线注入行为。
- [ ] 在文档中明确外置模型检索和机会窗口引导不属于 v1.0。

#### 完成条件

- [ ] Memory 当前能力不会被误读为完整检索系统。
- [ ] v2.0 / v3.0 的 Memory 后续目标已明确分层。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] 插件/Hook/MCP 管理面稳定化
labels: enhancement,area: desktop,area: tauri
-->
### [v1.0] 插件/Hook/MCP 管理面稳定化

#### 要做什么

收口 MCP、Skill、Plugin、Hook 当前管理面和诊断展示，不扩展完整治理策略。

#### 任务清单

- [ ] 核对 AgentKit 原生 MCP / Skill / Hook 来源展示。
- [ ] 核对只读 transport、警告、诊断和配置文件入口。
- [ ] 明确 AgentKit 扩展完整管理与 Hook 治理归入 v2.0。

#### 完成条件

- [ ] 当前插件与工具链页面可作为 v1.0 管理入口。
- [ ] 未完成能力在文档和 issue 中有清晰阶段归属。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] IAB 与斜杠命令回归
labels: enhancement,area: desktop,area: contracts
-->
### [v1.0] IAB 与斜杠命令回归

#### 要做什么

稳定 Windows 首发路径下的内置浏览器交互和 Lilia 斜杠命令，不承诺完整后端原生命令。

#### 任务清单

- [ ] 核对 IAB 打开、导航、标题 / URL / 截图元数据采集。
- [ ] 核对 `/` 命令面板、内置命令和 `.lilia/commands` 项目命令。
- [ ] 核对命令执行结果回写 task timeline。

#### 完成条件

- [ ] IAB 和斜杠命令的 v1.0 能力边界在功能状态中标清。
- [ ] 关键路径有最小必要回归验证。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] Android 远控 beta 边界与验证
labels: enhancement,area: android,area: contracts,area: docs
-->
### [v1.0] Android 远控 beta 边界与验证

#### 要做什么

把当前 Android remote 纳入 `v1.0-beta` 的实验性能力面，同时明确它不是稳定完整远控产品承诺。

#### 任务清单

- [ ] 同步 Android 设计文档、README、roadmap 和 release 口径。
- [ ] 确认 `packages/contracts` 的 typed remote-control 类型和 IPC command manifest 是当前权威契约。
- [ ] 发布 Android companion beta 前运行 `yarn android:verify`。
- [ ] 在发布说明中标明 Android remote 仍为 experimental，不承诺离线队列、PC-PC、多设备协作、Android 本地 runner 或完整设置面。

#### 完成条件

- [ ] beta 发布不会同时出现“空壳冻结”和“完整远控已稳定”的冲突描述。
- [ ] Android remote 如进入发布资产，必须带 experimental 标识和 `yarn android:verify` 验证记录。
- [ ] v2.0 / v3.0 的远控稳定化、多设备和 PC-PC 演进边界清楚。
<!-- /issue -->

<!-- issue
milestone: v1.0 稳定化收尾
title: [v1.0] 文档与功能状态同步
labels: documentation,area: docs
-->
### [v1.0] 文档与功能状态同步

#### 要做什么

让路线图、功能状态、GitHub milestone 和 issue 清单都表达同一套 v1.0-v3.0 阶段目标。

#### 任务清单

- [ ] 同步中英文 roadmap。
- [ ] 同步中英文 feature status。
- [ ] 回查 GitHub milestone 和 issue 挂载关系。

#### 完成条件

- [ ] 文档站构建通过。
- [ ] GitHub 远端对象与 `docs/github/phase-roadmap.md` 一致。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 大操作区聊天布局
labels: enhancement,area: desktop
-->
### [v2.0] 大操作区聊天布局

#### 要做什么

重构聊天主界面，让对话、时间线、输入区和待处理交互拥有更大的稳定操作区。

#### 任务清单

- [ ] 重新划分主内容、状态、过程信息和辅助操作层级。
- [ ] 优化 timeline、composer 和 pending action 的空间分配。
- [ ] 保持 Lilia 克制、清晰、可扫描的工程工具气质。

#### 完成条件

- [ ] 日常对话操作区明显更稳定。
- [ ] 不牺牲关键状态和交互可见性。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 完整侧边栏框架
labels: enhancement,area: desktop
-->
### [v2.0] 完整侧边栏框架

#### 要做什么

补齐侧边栏的信息架构，让任务、产物、Memory、文件树、Git 和项目上下文都有稳定入口。

#### 任务清单

- [ ] 定义侧边栏一级入口和切换模型。
- [ ] 支持任务列表与上下文面板之间的稳定切换。
- [ ] 保证窄宽度和长列表场景可用。

#### 完成条件

- [ ] 侧边栏不再只是任务列表。
- [ ] 新入口不干扰核心聊天操作区。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 侧边栏产物面板
labels: enhancement,area: desktop,area: tauri
-->
### [v2.0] 侧边栏产物面板

#### 要做什么

在侧边栏展示 Agent 产出的文件、报告、截图、构建结果、链接和后续动作。

#### 任务清单

- [ ] 定义产物来源和基础数据模型。
- [ ] 展示最近产物、关联任务和打开动作。
- [ ] 支持失败或不可打开产物的清晰提示。

#### 完成条件

- [ ] 用户能从任务上下文快速找到 Agent 输出。
- [ ] 产物面板不依赖手动翻 timeline。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] Memory 浏览与 Layer 2 机会窗口
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] Memory 浏览与 Layer 2 机会窗口

#### 要做什么

提供更完整的 Memory 浏览、过滤和会话机会窗口引导。

#### 任务清单

- [ ] 支持按用户级、项目级、来源和时间过滤 Memory。
- [ ] 在合适时机提示 Agent 有相关记忆可查。
- [ ] 保留 Agent 自决使用记忆的边界。

#### 完成条件

- [ ] Memory 不再只是保存和启动注入。
- [ ] 机会窗口引导可被用户和 Agent 明确识别。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 文件树浏览与上下文发送
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 文件树浏览与上下文发送

#### 要做什么

支持浏览项目文件树，并把文件、目录、图片等上下文稳定发送给 Agent。

#### 任务清单

- [ ] 展示项目文件树和基础文件元信息。
- [ ] 支持选择文件、目录和图片作为上下文。
- [ ] 与现有 `@` 文件上下文能力保持一致。

#### 完成条件

- [ ] 用户可从侧边栏完成常见上下文选择。
- [ ] 上下文发送结果可在 composer 或 timeline 中追踪。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 简单 Git 操作
labels: enhancement,area: desktop,area: tauri
-->
### [v2.0] 简单 Git 操作

#### 要做什么

提供状态、diff、stage/unstage、commit、分支和工作树基础操作；push、merge、rebase 等高风险操作继续显式确认。

#### 任务清单

- [ ] 展示当前分支、文件状态和基础 diff。
- [ ] 支持 stage / unstage 和 commit 基础流程。
- [ ] 支持分支和工作树基础查看操作。

#### 完成条件

- [ ] 常见本地 Git 操作不需要离开 Lilia。
- [ ] 高风险远端或历史重写操作不会被静默执行。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 任务创建与生命周期
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 任务创建与生命周期

#### 要做什么

让用户可以创建任务，并设置状态、优先级、归属和完成条件。

#### 任务清单

- [ ] 支持从项目、侧边栏或对话入口创建任务。
- [ ] 支持状态、优先级、归属和完成条件编辑。
- [ ] 与现有 task timeline 和会话任务化模型对齐。

#### 完成条件

- [ ] 任务不再只能从会话自然产生。
- [ ] 用户能明确管理一个任务从创建到完成的生命周期。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 任务依赖/阻塞闭环
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 任务依赖/阻塞闭环

#### 要做什么

补齐任务依赖、阻塞状态、自动驱动和失败重排的可操作闭环。

#### 任务清单

- [ ] 支持清晰维护 depends-on 和 blocked 状态。
- [ ] 在会话主链路中使用依赖和阻塞信息。
- [ ] 定义失败、重试和重排的用户可见策略。

#### 完成条件

- [ ] 依赖关系不只是展示数据。
- [ ] 阻塞和失败能影响后续任务调度。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 任务分配与团队模式
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 任务分配与团队模式

#### 要做什么

支持任务分配和团队模式，让多个 Agent 或执行角色可以围绕同一目标协作。

#### 任务清单

- [ ] 定义任务分配对象、角色和状态。
- [ ] 支持在 UI 中查看和调整分配。
- [ ] 与多 Agent 写作和自动编排保持同一任务模型。

#### 完成条件

- [ ] 用户能把任务明确分配给不同 Agent 或角色。
- [ ] 团队模式不只是并行启动多个会话。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 多 Agent 写作与调度
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 多 Agent 写作与调度

#### 要做什么

支持多个 Agent 参与同一目标，提供写作、审查、验证和整合流程。

#### 任务清单

- [ ] 定义多 Agent 工作流的输入、输出和整合方式。
- [ ] 支持主 Agent 与辅助 Agent 的角色边界。
- [ ] 展示各 Agent 的进度、产物和待处理交互。

#### 完成条件

- [ ] 多 Agent 协作结果可追踪、可恢复、可整合。
- [ ] 主 Agent 仍负责最终判断和收口。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 自动编排进入会话主链路
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 自动编排进入会话主链路

#### 要做什么

把自动化框架接入会话主路径，形成调度、失败处理、恢复和降级策略。

#### 任务清单

- [ ] 将自动化触发与 task / timeline / todo / interaction 信号对齐。
- [ ] 让 Agent 节点走现有 composer、permission 和 timeline 路径。
- [ ] 明确失败、跳过、等待人工和恢复策略。

#### 完成条件

- [ ] 自动编排可以服务真实会话任务。
- [ ] 自动化不会绕过既有权限和人机交互边界。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] AgentKit 原生插件完整管理
labels: enhancement,area: desktop,area: tauri
-->
### [v2.0] AgentKit 原生插件完整管理

#### 要做什么

完整管理 Mutsuki AgentKit 侧 MCP / Skill / Hook / 插件包的安装、启停、更新和作用域。

#### 任务清单

- [ ] 展示已安装 AgentKit 扩展与状态。
- [ ] 支持安装、启停、更新和作用域控制。
- [ ] 与共享服务（MCP、Memory、Git、LSP）入口保持一致。

#### 完成条件

- [ ] 插件管理不再是空状态占位。
- [ ] 用户能理解扩展启用后会影响哪些项目或会话。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] Subagents / 辅助 Agent 接入
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] Subagents / 辅助 Agent 接入

#### 要做什么

补齐 Mutsuki / Lilia 协议下的 Subagents 展示调度和低成本辅助 Agent 能力。

#### 任务清单

- [ ] 展示可用 Subagents 或自定义 Agent。
- [ ] 支持从任务或会话中调度辅助 Agent。
- [ ] 将辅助 Agent 输出纳入 timeline、产物和主 Agent 整合路径。

#### 完成条件

- [ ] 辅助 Agent 能监督或补充主 Agent，而不是替代主 Agent。
- [ ] 调度结果可见、可恢复、可审查。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] 对话级搜索/分叉/继续/重生成补全
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v2.0] 对话级搜索/分叉/继续/重生成补全

#### 要做什么

补齐消息正文搜索、会话分叉、从指定 turn 继续、重生成和更多会话管理操作。

#### 任务清单

- [ ] 支持消息正文搜索和定位。
- [ ] 支持从指定 turn 分叉或继续。
- [ ] 支持重生成、命名、标签、归档等会话级操作。

#### 完成条件

- [ ] 对话不只是按任务列表打开。
- [ ] 用户能对长期会话进行可靠维护。
<!-- /issue -->

<!-- issue
milestone: v2.0 会话工作台与团队模式
title: [v2.0] Android 远控稳定化与能力补全
labels: enhancement,area: contracts,area: docs
-->
### [v2.0] Android 远控稳定化与能力补全

#### 要做什么

把 `v1.0-beta` 的 Android remote 实验链路收敛为可长期维护的稳定能力，而不是把已存在能力重新当作未来 MVP。

#### 任务清单

- [ ] 补齐 remote-control event stream、能力协商和错误语义的跨端回归。
- [ ] 收敛 HTTP bridge 与后续 iroh / shared remote crate 的取舍，避免重复连接状态机。
- [ ] 扩展 Android smoke，覆盖配对、active PC、任务继续、pending interaction 和撤销设备。
- [ ] 补齐通知、前后台恢复、长 timeline 性能和异常恢复。

#### 完成条件

- [ ] Android remote 从 experimental beta 升级为稳定远控入口。
- [ ] Android 不绕过 PC 端既有权限和 Agent 执行边界。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 项目级总览与高级度量
labels: enhancement,area: desktop,area: tauri
-->
### [v3.0] 项目级总览与高级度量

#### 要做什么

解释任务状态、活跃度、成本、风险和项目进展，而不是只展示聚合数字。

#### 任务清单

- [ ] 定义项目级度量和解释口径。
- [ ] 展示风险、瓶颈、成本和近期变化。
- [ ] 将度量与 roadmap、milestone 和任务状态关联。

#### 完成条件

- [ ] 项目总览能回答当前项目为什么处于这个状态。
- [ ] 用户能从总览进入下一步处理对象。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 架构图一等资产
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v3.0] 架构图一等资产

#### 要做什么

支持项目架构图生成、编辑、版本化、变更记录、回滚和任务引用。

#### 任务清单

- [ ] 定义架构图节点、边、摘要和变更记录的长期模型。
- [ ] 支持生成、编辑、版本化和回滚。
- [ ] 支持从任务、timeline 和项目页引用架构变化。

#### 完成条件

- [ ] 架构图成为项目资产，而不是聊天附属展示。
- [ ] 架构变化可追踪、可解释、可回滚。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 美术/设计原则一等资产
labels: enhancement,area: desktop,area: docs
-->
### [v3.0] 美术/设计原则一等资产

#### 要做什么

把设计原则、视觉规范、产品约束和项目风格落到可查看、可引用、可演进的项目资产。

#### 任务清单

- [ ] 定义项目设计原则和风格资产的存储与展示方式。
- [ ] 支持 Agent 在前端、UI 和文档任务中引用这些原则。
- [ ] 记录原则变更和适用范围。

#### 完成条件

- [ ] 美术和设计原则不再只存在于零散文档。
- [ ] Agent 工作能稳定看到项目风格约束。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 项目 Memory 高级检索与质量评估
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v3.0] 项目 Memory 高级检索与质量评估

#### 要做什么

引入外置模型检索、质量评估、召回盲区检查和采纳率指标。

#### 任务清单

- [ ] 在 grep 之外加入查询扩展或外置模型检索。
- [ ] 记录记忆采纳率、召回盲区和误触发情况。
- [ ] 支持项目级 Memory 的质量维护。

#### 完成条件

- [ ] Memory 能被评估，而不是只被保存。
- [ ] 检索策略能根据指标持续调整。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] Roadmap/Milestone 高级解释视图
labels: enhancement,area: desktop,area: tauri
-->
### [v3.0] Roadmap/Milestone 高级解释视图

#### 要做什么

支持跨周、跨版本、跨任务的进展解释、风险汇总和后续建议。

#### 任务清单

- [ ] 汇总 milestone 进度、任务状态和近期活动。
- [ ] 解释风险、阻塞、延期和完成质量。
- [ ] 给出下一步建议和可进入的任务。

#### 完成条件

- [ ] Roadmap 页能解释工程进展，而不是只列任务。
- [ ] milestone 状态对长期项目管理有实际指导价值。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 自动化高级能力
labels: enhancement,area: desktop,area: tauri,area: contracts
-->
### [v3.0] 自动化高级能力

#### 要做什么

补齐 cron、webhook、重试、并发实例、子流程、远程执行和工作流复用。

#### 任务清单

- [ ] 支持 cron 和 webhook 触发。
- [ ] 支持重试、并发实例、子流程和失败恢复策略。
- [ ] 支持远程执行和工作流复用。

#### 完成条件

- [ ] 自动化不再只是本地 MVP DAG。
- [ ] 高级能力仍保留权限和人机确认边界。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 插件/工作流市场化能力
labels: enhancement,area: desktop,area: tauri,area: docs
-->
### [v3.0] 插件/工作流市场化能力

#### 要做什么

让可复用插件、技能和工作流从本地管理面升级为可分发资产。

#### 任务清单

- [ ] 定义可分发插件、技能和工作流的元数据。
- [ ] 支持安装、更新、禁用和作用域控制。
- [ ] 展示来源、风险、权限和兼容性。

#### 完成条件

- [ ] 用户可以复用和分享工作流能力。
- [ ] 可分发能力不会绕过本地权限治理。
<!-- /issue -->

<!-- issue
milestone: v3.0 项目级知识资产
title: [v3.0] 远控多设备/PC-PC 长期协作
labels: enhancement,area: contracts,area: docs
-->
### [v3.0] 远控多设备/PC-PC 长期协作

#### 要做什么

在 Android remote 稳定化之后评估多设备、PC-PC 和更复杂远程协作。

#### 任务清单

- [ ] 评估共享 remote crate、连接池和多节点路由。
- [ ] 设计多设备状态同步和权限边界。
- [ ] 明确 PC-PC 与 Android 远控之间的协议复用关系。

#### 完成条件

- [ ] 远控长期架构有明确演进路径。
- [ ] 多设备协作不破坏 PC 端权威源边界。
<!-- /issue -->
