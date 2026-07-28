# LEGACY — Node agent-runner (#47)

**状态**：限期兼容，非默认执行路径。  
**位置**：`apps/desktop/legacy/agent-runner/`（已移出 Desktop 默认路径）。  
**启用**：`--features legacy-runner` **且** `LILIA_AGENT_EXECUTION_BACKEND=node|legacy|agent-runner`。  
**截止产品版本**：`1.0.0`（`LEGACY_NODE_RUNNER_COMPAT_UNTIL`）。  
**安装包**：默认 NSIS **不** 打包本目录或 `appServer.mjs`。

新任务必须走 Native AgentKit。详见
[`docs/design/legacy-agent-runner-retirement.md`](../../../../docs/design/legacy-agent-runner-retirement.md)。
