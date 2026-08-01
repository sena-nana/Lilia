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
# Rust host contract / service behavior
cargo test -p lilia-editor-compat --locked

# Extension host client 契约测试（不启动 VS Code）
node apps/vscode-extension/tests/hostClient.test.mjs

# Extension 行为（host mock：InlineCompletion + Next Edit command）
node apps/vscode-extension/tests/extensionBehavior.test.mjs

# 一键（要求 Node 26）
yarn verify:vscode-compat
```

Extension 默认启动 PATH 中的 `lilia-editor-compat`。Host 通过环境变量接收
Host-owned Provider 配置；API key 只在 Credential Broker 边界解析，不进入
Completion / Next Edit 请求或日志：

```bash
export LILIA_AGENTKIT_PROVIDER=openai-compatible
export LILIA_AGENTKIT_MODEL=<model>
export LILIA_AGENTKIT_API_KEY=<api-key>
# 可选；OpenAI-compatible 默认 https://api.openai.com/v1
export LILIA_AGENTKIT_ENDPOINT=<https-endpoint>
```

需要使用绝对路径时：

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
