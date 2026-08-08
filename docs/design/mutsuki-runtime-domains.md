# LiliaCode RuntimeDomain 参考装配

LiliaCode 为 MutsukiCore Issue #43 提供一个可运行、无 Tauri/Vue 依赖的三运行域参考装配：

- `lilia-product-domain`：产品命令和 projection，保留独立交互线程；
- `lilia-agent-domain`：Agent event 和脚本执行；
- `lilia-workspace-domain`：文件、Git、扫描和索引工作。

参考装配位于 `apps/desktop/src-tauri/src/runtime_domains.rs`。它使用
`RuntimeGroupHost`、typed cross-domain request、共享 Host services 和独立 domain
abort，不共享 Core 内部 TaskPool、lease、ResourceManager 或 StateStore。

## 性能场景

运行：

```powershell
cargo run --release --locked --features runtime-domain-reference `
  --manifest-path apps/desktop/src-tauri/Cargo.toml `
  --bin lilia-runtime-domain-bench -- `
  --samples 100 `
  --min-background-ms 20 `
  --output artifacts/perf/issue43-liliacode-runtime-domains.json
```

单域和三域使用相同协议、Runner、payload、实际业务函数和总计三个 worker：

- 单域：三个 worker 共享一个 RuntimeDomain；
- 三域：product、agent、workspace 各一个 worker；
- 压力：两个真实 Agent stdin payload 构建与一次真实
  `git worktree list --porcelain` 检查同时运行；基准先自动校准迭代次数，使两类后台工作
  各自至少持续 20 ms；
- 测量：生产 handoff 合约解析和 prompt 构建从 submit 到 terminal outcome 的延迟；
- 方法：每种拓扑复用一个预热后的长生命周期 Runtime，单双域按样本交替先后顺序，
  使用至少 100 个样本和 nearest-rank 计算 p99，避免把小样本最大值误称为 p99；
- 门槛：三域 p99 至少降低 50%。

## 与生产迁移的边界

该模块是可执行 reference profile 和性能门禁，不会把空 RuntimeDomain 注入桌面进程，
也不宣称桌面产品数据库或全部 workspace command 已迁入多 RuntimeDomain。生产
Embedded/Service 共用 bootstrap、LiliaCore、Mutsuki AgentKit 和 workspace authority。
迁移时复用这里验证过的 domain ID、路由语义和性能场景，不建立第二套产品或 Agent 事实源。

## 生产切片状态（诚实 partial）

| 面 | 状态 |
| --- | --- |
| `lilia-workspace-domain` | **已**在 Desktop 生产路径挂载：`worktree_list` 经 `production_workspace_domain` 提交真实 `git worktree list` |
| AgentKit turns | 仍为单 `HostRuntime`（`native_agent` / AgentKitHost），**未**迁入 agent domain |
| Product SQLite / LiliaCore | 仍为 Embedded/Service bootstrap，**未**迁入 product domain |
| 空域注入 | **禁止**：不在 setup 里插入无 runner 的 product/agent domain |

生产协议 ID：`lilia.workspace.worktree.list.v1`（与 reference 的 `lilia.reference.workspace.*` 区分）。
