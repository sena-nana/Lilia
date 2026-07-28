# LEGACY — Node agent-runner / 官方 Server 源码树（#47）

**状态**：限期兼容，**默认 workspace / Desktop 构建不打包、不链接**。  
**源码位置**：本目录（`apps/desktop/legacy/`），已从 Desktop 默认路径移出。  
**启用 Rust 后门**：`--features legacy-runner` **且**  
`LILIA_AGENT_EXECUTION_BACKEND=node|legacy|agent-runner`。  
**截止产品版本**：`1.0.0`（`LEGACY_NODE_RUNNER_COMPAT_UNTIL`）。

| 路径 | 角色 |
| --- | --- |
| `agent-runner.mjs` | Node runner CLI 入口 |
| `agent-runner/**` | Claude/Codex Node runtime（含 `codex/appServer.mjs`） |
| `claude-history.mjs` / `codex-history.mjs` | 历史浏览 utility |
| `codex-account-quota.mjs` | 账号配额（经官方 app-server） |

新任务必须走 Native AgentKit。详见  
[`docs/design/legacy-agent-runner-retirement.md`](../../../docs/design/legacy-agent-runner-retirement.md)。
