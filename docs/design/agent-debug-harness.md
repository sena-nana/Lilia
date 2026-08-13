# Native Agent Debug Harness

## 目标

Agent Debug 为正式 NanaUI/WGPU 桌面提供开发态结构化观察与操作入口。它验证真实窗口、GPU 渲染和应用行为，不是生产功能，也不依赖浏览器自动化。

## 协议边界

- Desktop Debug 二进制在 `LILIA_AGENT_DEBUG=1` 时开启本地 TCP 调试协议。
- `observe` 返回窗口、稳定目标、可见状态、运行时摘要与最近错误。
- `act` 只接受协议定义的稳定目标和类型化动作，不按坐标猜测控件。
- 截图来自真实 WGPU surface；截图与业务快照分别判断，避免图像存在却未证明状态正确。
- secret canary 确认凭据、环境变量和敏感正文不会进入日志、回放或截图元数据。
- Release 构建不包含调试监听、固定调试标记或测试 fixture。

## 运行

从仓库根目录执行：

```bash
cargo xtask agent-debug
```

xtask 构建并启动正式 Native Debug target，等待协议 ready，按固定 corpus 执行 observe/act，最终关闭进程并生成证据。外部窗口或 GPU 能力不可用时必须返回结构化 blocker，不能把跳过当作通过。

## 产物

每次运行写入 `agent-debug-runs/lilia-<timestamp>/`，至少包含：

- `summary.json`：环境、版本、场景与总结果；
- `observations.json`：结构化窗口和产品状态快照；
- `replay.json`：可重放动作及动作结果；
- `errors.json`：协议、运行时与窗口错误；
- `screenshots/`：关键阶段的真实 GPU 截图；
- `secret-canary.json`：敏感数据泄漏检查结果。

产物不得记录凭据原文、用户 home 绝对路径或未脱敏的环境变量。

## 场景与通过标准

固定场景覆盖项目/任务打开、Composer 输入与发送、待审批交互、设置、窗口操作和单实例。每个场景必须同时满足：

1. 稳定目标可观察且动作命中唯一目标；
2. 动作后的产品状态符合合同；
3. 截图非空且属于预期 Native 窗口；
4. 没有未归类错误；
5. secret canary 未泄漏。

真实 provider 回复、远端凭据或设备能力不属于无凭据默认 corpus；需要时作为独立系统验收报告。

## 使用要求

涉及 UI 主路径、Agent runtime、持久化、权限、构建配置、跨端契约或用户关键路径的大型改动，最终确认必须包含 `cargo xtask agent-debug` 的结果，或具体 blocker、产物路径和剩余风险。普通 Markdown、注释或无运行时影响的整理不运行该门禁。
