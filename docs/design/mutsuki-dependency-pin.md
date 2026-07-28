# MutsukiCore 依赖：PATH / GIT 切换

LiliaCode 通过根 `Cargo.toml` 的 `[workspace.dependencies]` 统一 pin Host + AgentKit。
所有 `mutsuki-*` 条目必须使用同一模式与同一 revision，禁止混用 path / git。

## PATH 模式（本地 sibling，开发默认）

前提：仓库布局为

```text
Documents/workspace/
  LiliaCode/
  MutsukiCore/
```

根 `Cargo.toml` 当前为 PATH 模式，例如：

```toml
mutsuki-agent-adapter-anthropic = { path = "../MutsukiCore/kits/agent/crates/mutsuki-agent-adapter-anthropic" }
mutsuki-agent-bundle = { path = "../MutsukiCore/kits/agent/crates/mutsuki-agent-bundle" }
# …其余 mutsuki-* 同目录 sibling path…
```

验证：

```bash
cargo check -p lilia-agent-integration --locked
cargo test -p lilia-agent-integration --locked
cargo check -p lilia --locked
```

用途：在 MutsukiCore 尚未 push 的 crate（如 `mutsuki-agent-adapter-anthropic`）上联调产品侧。

## GIT 模式（可提交 / CI / Release）

当目标 crate 已进入远端 revision 后：

1. 记录 MutsukiCore commit SHA（含 anthropic adapter + workspace 注册）。
2. 将根 `Cargo.toml` 中全部 `mutsuki-*` 从 `path = "../MutsukiCore/..."` 改为同 `git` + `rev`。
3. 更新 `Cargo.lock`（根 workspace）。
4. 再跑上节验证命令；Desktop CI 使用同一 lock。

示例（仅示意，`rev` 必须替换为真实 SHA）：

```toml
mutsuki-agent-adapter-anthropic = {
  git = "https://github.com/sena-nana/MutsukiCore.git",
  rev = "REPLACE_WITH_MUTSUKI_REV",
  package = "mutsuki-agent-adapter-anthropic",
}
```

## 切换检查清单

- [ ] 全部 `mutsuki-*` 同一模式（全 path 或全同一 git rev）
- [ ] anthropic adapter 已在 MutsukiCore `workspace.dependencies` 与 `kits/agent/crates/*` 成员中
- [ ] `cargo metadata --locked` / 相关 `cargo test` 通过
- [ ] 未把本地 secret、临时 endpoint 写进提交

## 当前阻塞切回 GIT 的原因

`mutsuki-agent-adapter-anthropic` 已在本地 MutsukiCore `8a02d74` 提交，但尚未 push；远端历史 pin `75dcfc74` 仍无该 package。PATH 模式可先完成产品联调与测试，adapter push 后再切 GIT。
