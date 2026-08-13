# LiliaCode Windows Release

## 发布摘要

本版本是 LiliaCode Native Windows 发布包。完成以下真实安装验证前保持 draft。

## 构建与产物

- [ ] `cargo xtask verify` 通过。
- [ ] `cargo xtask agent-debug` 通过，证据位于 `agent-debug-runs/lilia-*`。
- [ ] `cargo xtask release windows --tag <tag>` 通过。
- [ ] 安装包名称符合 `LiliaCode-<version>-setup.exe`。
- [ ] 更新资产包含 `latest.json`、更新归档及其 `.sig`。
- [ ] 更新签名通过运行时公钥往返验证。
- [ ] Release 二进制不含 Agent Debug 标记。

## Windows 安装验证

- [ ] `cargo xtask installer-smoke --tag <tag>` 通过。
- [ ] 已覆盖安装、启动主窗口和 `liliacode <测试项目路径>`。
- [ ] 已覆盖单实例和从上一正式版本覆盖升级。
- [ ] 已覆盖应用内更新等待、确认、安装与重启。
- [ ] 已覆盖卸载与新 shell 的 PATH 清理。
- [ ] 已确认升级和卸载不会删除 `LILIA_HOME` 用户数据。

## 验证记录

- 验证人：
- 验证日期：
- Windows 版本：
- revision/tag：
- 安装包文件名：
- 安装与启动：
- CLI 与单实例：
- 覆盖升级与应用内更新：
- 卸载与 PATH：
- 用户数据保留：
- Agent Debug 产物：

## 已知限制

- 当前只发布 Windows 安装包。
- 当前不发布 macOS 公证包或 Linux/macOS 安装包。
- Android companion 如作为实验性资产附加，需单独记录 `cargo xtask android test|build|smoke` 结果。

## 升级说明

LiliaCode 启动后检查 `latest.json`。用户确认后下载、验证签名、安装并重启；也可以从 GitHub Release 手动安装新版。
