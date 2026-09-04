# Codex++（个人 fork）

<p align="center">
  <img src="docs/images/codex-plus-plus.png" alt="Codex++ 图标" width="160">
</p>

<p align="center">
  中文 | <a href="README_EN.md">English</a>
</p>

<p align="center">
  <img alt="License" src="https://img.shields.io/github/license/payAgain/CodexPlusPlus">
  <img alt="Rust" src="https://img.shields.io/badge/rust-1.85%2B-orange">
  <img alt="Tauri" src="https://img.shields.io/badge/tauri-2.x-24C8DB">
</p>

Codex++ 是面向 OpenAI Codex / ChatGPT 桌面应用的外部启动器与管理工具。它通过 Chromium DevTools Protocol 和本地辅助服务提供供应商切换、协议转换、会话管理与界面增强，不修改官方应用的 `app.asar`，也不向安装目录写入补丁文件。

本仓库是 [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) 的个人 fork，按个人使用习惯精简上游功能：移除赞助商展示、Dream Skin 皮肤管理、Grok 配置、Zed Remote 与 Upstream worktree，保留供应商、模型上下文、会话、增强、脚本等核心能力。本次 fork 改动涉及约 80 个文件、2.9 万行删除。

## 本 fork 改动

| 改动 | 说明 |
| --- | --- |
| 移除赞助商/推荐位 | 删除 `ads` 模块、前端赞助商板块、相关文案与图片资产 |
| 移除 Dream Skin 皮肤管理 | 删除皮肤管理界面、皮肤运行时/社区/市场/安装包模块、内置与第三方皮肤资产，并清空 Tauri asset protocol 的皮肤目录权限 |
| 移除 Grok 配置 | 删除 `grok_config` 模块与前端 Grok 管理界面 |
| 移除 Zed Remote | 删除远程项目识别、打开与配套注入逻辑 |
| 移除 Upstream worktree | 删除 worktree 创建、remote 管理与配套注入逻辑 |
| 同步精简主链路 | 对应精简 `renderer-inject.js`、`styles.css`、Tauri commands/lib、launcher 与测试，其余功能不受影响 |

## 分支与维护

- `main`：只镜像上游 `main`，用于接收 [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) 的同步更新，不合并个人改动。
- `codex/remove-extra-features`：个人维护分支，也是本仓库 GitHub 默认/展示分支，fork 改动集中在这里。

同步上游：

```bash
git remote add upstream https://github.com/BigPizzaV3/CodexPlusPlus.git
git fetch upstream
git checkout main
git merge --ff-only upstream/main
git checkout codex/remove-extra-features
git rebase main
```

## 安装包

本 fork 不发布独立安装包。需要现成安装包时，请使用[上游 Releases](https://github.com/BigPizzaV3/CodexPlusPlus/releases)，或按下方“开发”一节自行构建。

## 当前功能

| 模块 | 功能 |
| --- | --- |
| 供应商配置 | 官方登录、官方登录混入 API、纯 API、聚合供应商；Responses / Chat Completions；模型测试、模型列表、Provider Doctor、cc-switch 与链接导入 |
| 模型与上下文 | 每模型上下文窗口、自动压缩阈值、`model_catalog_json`、通用配置，以及按供应商选择 MCP、Skill 和 Plugin |
| 会话管理 | 扫描本地会话、批量删除、Markdown 导出、Token 用量历史、Provider metadata 同步与备份、会话导入与分享链接 |
| Skills 与脚本 | Skills 技能管理、脚本市场安装/启停 |
| Codex 增强 | 插件市场与模型白名单处理、粘贴修复、强制中文、快速启动、原生菜单汉化、会话宽度/滚动恢复/线程 ID、服务层级控制、Goals、图片覆盖层 |
| 微信连接 | 通过个人微信连接本机 Codex 会话 |
| 安装维护 | 应用检测、快捷方式、Watcher、环境冲突、日志诊断、健康检查和 Release 更新提示 |

所有界面增强都可以单独关闭。关闭“Codex 增强”总开关后，Codex++ 仍可作为供应商和启动管理工具使用。

## 供应商模式

Codex++ 将官方登录、混入 API 和纯 API 分开保存和切换：

| 模式 | 用途 | 认证边界 |
| --- | --- | --- |
| 官方登录 | 只使用 ChatGPT / Codex 官方账号 | 清理自定义 provider 和 API Key，保留官方登录状态 |
| 官方登录 + API | 保留官方账号与插件入口，模型请求走兼容 API | API Key 写入 provider bearer token，不写入纯 API 的 `auth.json` |
| 纯 API | 不依赖官方账号，完全使用自定义 Base URL / Key | 独立保存 `config.toml` 与 API Key，不混入官方认证 |
| 聚合供应商 | 在多个普通 API 供应商之间路由 | 支持故障转移、按会话轮转、按请求轮转和权重轮转 |

每个供应商可配置 Responses 或 Chat Completions 协议、模型列表、测试模型、User-Agent、上下文窗口、自动压缩阈值，以及该供应商启用的 MCP Server、Skill 和 Plugin。Chat Completions 可通过本地代理转换为 Codex 使用的 Responses 协议。

每模型窗口支持 `1M`、`200K` 或纯数字。Codex++ 会生成独立 `model_catalog_json`，让 Codex 按当前模型使用对应窗口。

切换供应商时会先保存当前配置，再写入目标配置。真实 API Key 只保存在本机，请勿放入日志、截图或 issue。

## Codex 界面增强

- 插件市场解锁与模型白名单处理。
- 会话删除、批量删除和 Markdown 导出。
- 富文本粘贴转纯文本、强制中文、快速启动和原生菜单汉化。
- 会话宽度、滚动位置恢复、线程 ID 和服务层级切换。
- Goals 按供应商开关，可写回 config.toml。
- 图片覆盖层。

依赖注入脚本的设置通常需要保存后重新启动 Codex++ 才会生效。

## 数据位置

- Codex 配置：`~/.codex/config.toml`
- Codex 登录状态：`~/.codex/auth.json`
- Codex 本地数据库：优先读取 `~/.codex/sqlite/*.db`，旧版回退到 `~/.codex/state_5.sqlite`
- Codex++ 状态与日志：`~/.codex-session-delete/`
- Provider 同步备份：`~/.codex/backups_state/provider-sync`

## 开发

```bash
# 前端检查
cd apps/codex-plus-manager
npm ci
npm run check
npm run vite:build

# Rust 检查
cd ../..
cargo fmt --all -- --check
cargo test
cargo build --release
```

主要结构：

```text
apps/
  codex-plus-launcher/          静默启动入口
  codex-plus-manager/           Tauri 管理工具
assets/inject/
  renderer-inject.js            注入到 Codex 渲染端的增强脚本
crates/
  codex-plus-core/              启动、注入、配置、更新、安装、桥接等核心逻辑
  codex-plus-data/              会话数据、导出、Provider 同步
scripts/installer/
  windows/CodexPlusPlus.nsi     Windows NSIS 安装包
  macos/package-dmg.sh          macOS DMG 打包
```

## 开源协议

上游项目 Copyright (C) 2026 BigPizzaV3。本 fork 继续沿用 [GNU Affero General Public License v3.0](LICENSE)，SPDX 标识为 `AGPL-3.0-only`。修改并分发本项目，或通过网络提供修改后的版本时，需要按 AGPLv3 提供对应源代码。

许可证只覆盖 CodexPlusPlus 自身代码，不授予 OpenAI、ChatGPT、Codex 的商标、应用资源或其他第三方内容的权利。

## 兼容性说明

Codex++ 依赖官方桌面应用的页面结构、CDP 和本地数据格式。官方应用更新后，部分注入功能可能需要跟随适配；修改供应商配置或本地会话数据前应保留备份。
