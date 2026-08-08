# Mutsuki 依赖：PATH / GIT 切换

LiliaCode 通过根 `Cargo.toml` 的 `[workspace.dependencies]` 统一 pin Host + AgentKit。
所有 `mutsuki-*` 条目必须使用同一模式与同一 revision，禁止混用 path / git。

远端仓库：`https://github.com/sena-nana/Mutsuki.git`（本地 checkout 目录名可能仍为 `MutsukiCore/`）。

## 当前默认：GIT 模式

根 `Cargo.toml` 当前为 **GIT pin**：

- `git = "https://github.com/sena-nana/Mutsuki.git"`
- `rev = "873af51f5391f7bd5f1d3216c2bcfe8680c839f8"`（短写 `873af51`）

```toml
mutsuki-agent-adapter-anthropic = {
  git = "https://github.com/sena-nana/Mutsuki.git",
  rev = "873af51f5391f7bd5f1d3216c2bcfe8680c839f8",
  package = "mutsuki-agent-adapter-anthropic",
}
# …其余 mutsuki-* 同一 git + rev…
```

验证：

```bash
cargo check -p lilia-agent-integration --locked
cargo test -p lilia-agent-integration --locked
cargo check -p lilia --locked
bash scripts/check-rust-boundaries.sh
```

用途：可提交 / CI / Release；与远端 Mutsuki `main` 对齐。

## PATH 模式（本地 sibling，联调临时）

前提：仓库布局为

```text
Documents/workspace/
  LiliaCode/
  MutsukiCore/
```

需要联调尚未 push 的 Mutsuki 改动时：

1. 将根 `Cargo.toml` 中全部 `mutsuki-*` 从 `git` + `rev` 改为 `path = "../MutsukiCore/..."`。
2. 更新 `Cargo.lock`。
3. 再跑上节验证命令。

示例：

```toml
mutsuki-agent-adapter-anthropic = { path = "../MutsukiCore/kits/agent/crates/mutsuki-agent-adapter-anthropic" }
mutsuki-agent-bundle = { path = "../MutsukiCore/kits/agent/crates/mutsuki-agent-bundle" }
# …其余 mutsuki-* 同目录 sibling path…
```

联调结束后务必切回 GIT pin，勿将 PATH 模式提交为默认。

## 切回 / 更新 GIT pin 检查清单

- [ ] 全部 `mutsuki-*` 同一模式（全 path 或全同一 git rev）
- [ ] 目标 crate 已在远端 `Mutsuki` revision 的 workspace 成员中
- [ ] `rev` 与文档、`Cargo.toml` 注释一致
- [ ] `cargo metadata --locked` / 相关 `cargo test` 通过
- [ ] 未把本地 secret、临时 endpoint 写进提交
