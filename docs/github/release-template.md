# LiliaCode v1.0.0-beta.1

## 发布摘要

本版本是 LiliaCode Windows 发布包。发布前请确认本页面仍是 draft，并完成下方安装验证后再正式发布。

## 主要变更

- 内置浏览器：Native Agent 可打开和导航 IAB 窗口，采集页面标题 / URL / 截图元数据，并把结果送回运行中的 turn 或作为消息附件。
- 斜杠命令：输入框支持 `/` 命令面板，可执行内置命令和 `.lilia/commands` 项目命令，并把执行结果回写到任务 timeline。
- 路线图页：项目 Roadmap 页可按当前项目 Task 状态聚合首发 milestone、进度、状态分布、当前重点和最近完成。
- 首发发布链路：`v*` tag 触发 release workflow，先运行 `yarn verify` 和 `yarn release:check --tag <tag>`，再生成 Windows draft Release 安装包。

## 安装包

- Windows x64 安装包：`LiliaCode_<version>_x64-setup.exe`
- Tauri updater 产物：`latest.json`、`*.nsis.zip`、`*.nsis.zip.sig`

## Windows 安装验证

- [ ] 从 draft Release 下载 Windows 安装包。
- [ ] 已确认 `yarn release:smoke:windows --tag <tag>` 或本地 `yarn release:smoke:windows --installer <安装包路径>` 通过。
- [ ] smoke 已覆盖安装包安装、启动 LiliaCode、通过 `liliacode <测试项目路径>` 打开项目。
- [ ] 确认基础窗口操作正常。
- [ ] smoke 已覆盖卸载流程，并确认卸载后新的 PowerShell 或 cmd 中 `liliacode` 不再可用。
- [ ] 完成验证后再将 draft Release 正式发布。

## Windows 安装验证记录

- 验证人：
- 验证日期：
- Windows 环境：
- 安装包文件名：LiliaCode_1.0.0-beta.1_x64-setup.exe
- 安装：
- 启动：
- CLI 入口：
- 卸载：

## 已知限制

- 当前只发布 Windows 安装包。
- 当前安装包使用 `tauri-signing.key` 完成 Tauri 签名。
- 当前不发布 macOS 公证包、macOS 安装包或 Linux 安装包。

## 升级说明

LiliaCode 支持应用内自动更新。启动后会自动检查新版本，用户确认后自动下载、安装并重启；也可以从 GitHub Release 手动下载新版 Windows 安装包并安装。

