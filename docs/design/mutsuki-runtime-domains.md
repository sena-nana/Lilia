# LiliaCode RuntimeDomain 边界

LiliaCode 使用 MutsukiCore RuntimeDomain 隔离三类工作：

- `lilia-product-domain`：产品命令和 projection；
- `lilia-agent-domain`：Agent event 与执行；
- `lilia-workspace-domain`：文件、Git、扫描和索引。

运行域装配属于 Rust 应用服务和宿主装配，不得由 UI 控件持有事实，也不得建立第二套产品、Agent 或 workspace 权威。跨域请求必须类型化，取消与 terminal outcome 必须可观察。

## 性能门禁

运行：

```bash
cargo xtask performance
```

固定 corpus 在相同协议、Runner、payload、业务函数和 worker 数量下比较当前实现与历史基线。样本应交错执行并报告 p50/p95/p99、CPU、RSS 与冷启动；绝对门禁和相对回归分别判断。孤立路径改善只证明受控并发下的隔离效果，不等同于整机端到端收益。

## 生产约束

- 每个桌面进程只 bootstrap 一个 `ServiceAuthority`。
- Product SQLite、AgentKit 与 workspace authority 仍各自只有一个事实源。
- 不在进程中挂载没有真实 runner 的空 RuntimeDomain。
- 引入或调整运行域前，必须用相同真实工作流证明调度收益，并检查取消、失败与进程退出行为。
