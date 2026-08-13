# 时间线显示派生

时间线只持久化事实字段：`kind`、`status`、`title`、`summary`、`payload`、`sourceId`。Native UI 通过 `lilia-contracts` 的白名单规则派生图标、标题、预览、详情与相邻分组；工具或扩展不能注入任意渲染组件。

新增事件时先更新 `crates/lilia-contracts/contracts/timeline-contract.json` 和 Rust 派生 API，再由 NanaUI 组件消费同一类型化结果。历史事件不存 display 快照，因此展示规则可升级而不改写事实记录。

## 最终回复折叠

同一 turn 的过程事件只有在收到 terminal turn event 后才折叠到最终 assistant reply。流式阶段按 `(turnSeq, intraTurnOrder)` 保持顺序展示，避免最终回复锚点变化造成抖动。

用户消息与最终回复保留在外；两者之间的工具、计划和中间内容进入 process group。reasoning 与 terminal turn 可为恢复和诊断持久化，但默认不作为独立可见节点。

## 计划与权限

计划正文只出现在 `kind: plan` 的时间线事件。待确认时展开，处理后可折叠；交互区只显示真实可执行的确认、修改或拒绝动作，不复制计划正文。

计划确认与执行权限正交。确认后恢复该 turn 选择的权限；修改要求进入当前 pending interaction，不创建无关的新任务。不同 provider 的计划能力由 Agent integration 适配为统一事实事件，Native UI 不读取 provider 私有 payload。
