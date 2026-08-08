# Legacy Node agent-runner / 官方 Agent Server 退役（#47）

## 现状（诚实状态）

| 面 | 默认值 | 说明 |
| --- | --- | --- |
| Desktop 执行后端 | `native-agentkit` | 新任务默认走 AgentKit Native |
| Node `agent-runner` | **默认不编译** | 需 `--features legacy-runner` **且** `LILIA_AGENT_EXECUTION_BACKEND=node\|legacy\|agent-runner` |
| 兼容截止产品版本 | `1.0.0` | `LEGACY_NODE_RUNNER_COMPAT_UNTIL` |
| 默认安装包 resources | **空** | 不再捆绑 `appServer.mjs` / `accountQuota.mjs` / `codex-account-quota.mjs` / `agent-runner.mjs` |
| 官方 Agent Server | 不作为生产执行依赖 | Native health 不依赖官方客户端是否安装 |
| Codex Spark（Node） | 默认不可达 | 仅 feature + 显式 Node env |
| Legacy 源码树 | `apps/desktop/legacy/` | 已从 Desktop 默认路径移出 |

状态命令：`product_core_status` / `native_agent_host_status` 暴露
`defaultBundleIncludesOfficialAgentServer=false`、
`defaultBundleIncludesNodeAgentRunner=false`、
`legacyRunnerFeatureCompiled`（默认 `false`）、
`nodeRunnerIsDefault=false`、
`nodeRunnerCompatUntil`。

## Legacy 入口（`apps/desktop/legacy/`，安装不打包）

```text
apps/desktop/legacy/
├── LEGACY.md
├── agent-runner.mjs
├── agent-runner/**          # 含 Codex appServer.mjs
├── claude-history.mjs
├── codex-history.mjs
└── codex-account-quota.mjs
```

默认 `cargo check -p lilia` / Desktop 构建 **不**启用 `legacy-runner`，因此：

- `locate_agent_runner` / `run_node_agent_runner` / Codex Spark Node 分支 **不编译**
- 即使设置 `LILIA_AGENT_EXECUTION_BACKEND=node` 也会被忽略并打日志

显式后门（开发/过渡）：

```text
cargo check -p lilia --features legacy-runner
LILIA_AGENT_EXECUTION_BACKEND=node
```

## 迁移策略（产品数据 vs Agent 会话）

两类迁移必须分开（见 Issue #47 正文）：

1. **Product Data** → Lilia Storage / Core（#56/#61）
2. **Agent Session** → 只读 provenance + **新 AgentKit session binding**；不伪造旧 tool 完成态

### 可运行工具（`lilia-storage`）

```text
cargo run -p lilia-storage --bin lilia-migrate -- dry-run [--home DIR] [--legacy PATH] [--product PATH]
cargo run -p lilia-storage --bin lilia-migrate -- apply ...
cargo run -p lilia-storage --bin lilia-migrate -- status ...
cargo run -p lilia-storage --bin lilia-migrate -- report ...
cargo run -p lilia-storage --bin lilia-migrate -- rollback ...
cargo run -p lilia-storage --bin lilia-migrate -- inspect ...
```

库 API：`lilia_storage::LegacyMigrationTool`（inspect / dry-run / apply / status / report / rollback）。

行为摘要：

- 从 Desktop legacy `lilia.db` 读取 Project / Task / dependencies / `task_agent_sessions` / timeline
- 写入共享 `product.db`（`LiliaDataPaths`）与 `product_projections.db`
- Claude / Codex session → `legacy_session_provenance`（`migrated_to_agentkit`，`compat_until=1.0.0`）
  + 确定性新 AgentKit session id：`agentkit-from-legacy:{backend}:{legacySessionId}`
  + `agent_session_bindings` 供后续 Native turns
- Timeline 以 `legacyImport` 只读投影迁入；**pending approval 不跨 Runtime**
- MCP / Skills apply → `$LILIA_HOME/config/agentkit-mcp-registry.json` 与
  `agentkit-skills-registry.json`（**永不写入 secret**；env/token/cookie 不导入）
- apply 前备份；rollback 从备份恢复；apply 幂等
- 首次 Live turn：`open_bound_session(binding)` → `submit_turn` → 产品 timeline 投影

### CLI 查看迁移后产品状态

```text
lilia-cli products --home <DIR>
lilia-cli timeline --task <id> --home <DIR>
```

## 验收门禁

- `node scripts/check-default-bundle-no-official-server.mjs`
- `node scripts/mark-legacy-agent-runner.mjs --check`
- `node scripts/check-legacy-runner-reachability.mjs`
- `node scripts/check-legacy-default-unreachable.mjs`
- `cargo check -p lilia --locked`（默认无 `legacy-runner`）
- `yarn verify:native-agentkit`
- `cargo test -p lilia-storage --locked migration::`
- `cargo test -p lilia-cli --locked migration_apply_then_first_native_turn`

## 非目标 / 剩余诚实说明

- `apps/desktop/legacy/**` 仍保留至截止版本 `1.0.0`，**仅** `--features legacy-runner` 后门可链入
- Jsonl process registry 等通用子进程基建仍在 Desktop 树内（非 Node agent-runner 产品路径）
- Provider/Credential 完整 CredentialRef 导入仍需 Host Native Credential UI 重绑
- Milestone 已进入 migration 切片：`milestones` + `task_milestone_links` → `product_entities` Milestone + 单 `tasks.milestone_id`（M:N 取最低 sort_order）
- Memory / Product Conversation 全量面 / Automation 等继续由 #56/#61 覆盖
- 不把 Legacy continue 做成新任务默认选项或 Native 失败 fallback
- 不逆向官方客户端认证
