# VS Code Extension — AgentKit Compat (#40)

兼容客户端，不是 Native IDE。最小目标：在不依赖 LiliaCodeCore / Node
`agent-runner` / 官方 Agent Server 的前提下，调用 AgentKit **Completion** 与
**Next Edit**。

```text
AgentKit CodeCompletionService / SharedNextEditService
        ↑
crates/lilia-editor-compat   ← Rust host（可测事实源）
        ↑
apps/vscode-extension        ← InlineCompletion + Next Edit command
```

## 运行

```bash
# Rust host（真实 AgentKit 服务，deterministic adapter）
cargo test -p lilia-editor-compat --locked

# Extension host client 契约测试（不启动 VS Code）
node apps/vscode-extension/tests/hostClient.test.mjs

# Extension 行为（host mock：InlineCompletion + Next Edit command）
node apps/vscode-extension/tests/extensionBehavior.test.mjs

# 一键（要求 Node 26；本机若为 Node 25，请直接跑上面三条底层命令）
yarn verify:vscode-compat
```

**Toolchain 限制：** 根 `engines.node` / `check-toolchain` 要求 Node `>=26 <27`。
Node 25 下请用底层命令验证，不要宣称 `yarn verify:vscode-compat` 已绿。

Extension 默认 `lilia.agentkit.hostMode=deterministic`。接真机二进制时：

```json
{
  "lilia.agentkit.hostMode": "process",
  "lilia.agentkit.hostBinary": "/path/to/lilia-editor-compat"
}
```

## 非目标

- 不实现完整 Chat / Product sync / Remote SSH
- 不 import Desktop Vue/Tauri
- 不把 VS Code API 提升为 AgentKit 公共模型
