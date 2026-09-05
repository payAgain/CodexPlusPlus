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

本仓库是 [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) 的个人 fork：按个人使用习惯精简上游功能（移除赞助商展示、Dream Skin 皮肤管理、Grok 配置、Zed Remote、Upstream worktree 与 stepwise 悬浮面板），补齐 Codex-X 界面与纯 API 增强，并按需摘取上游有价值的修复与新特性（见下方「上游同步记录」）。截至 2026-09-04，fork 相对分叉点净删约 4 万行。

## 本 fork 改动

| 改动 | 说明 |
| --- | --- |
| 移除赞助商/推荐位 | 删除 `ads` 模块、前端赞助商板块、相关文案与图片资产 |
| 移除 Dream Skin 皮肤管理 | 删除皮肤管理界面、皮肤运行时/社区/市场/安装包模块、内置与第三方皮肤资产，并清空 Tauri asset protocol 的皮肤目录权限 |
| 移除 Grok 配置 | 删除 `grok_config` 模块与前端 Grok 管理界面 |
| 移除 Zed Remote | 删除远程项目识别、打开与配套注入逻辑 |
| 移除 Upstream worktree | 删除 worktree 创建、remote 管理与配套注入逻辑 |
| 同步精简主链路 | 对应精简 `renderer-inject.js`、`styles.css`、Tauri commands/lib、launcher 与测试，其余功能不受影响 |
| 移除 stepwise 悬浮面板 | 删除 stepwise 模块、悬浮面板注入资源与对应测试 |
| Codex-X 界面 | 新增指令提示词页、实时 TOML 页与工作区状态栏，统一 Codex++ 命名 |
| 纯 API 增强 | 默认 yolo 模式、剥离 ChatGPT 登录残留、通用配置启用 multi-agent v2 |
| Skills 主目录 | 以 `~/.agents/skills` 为技能管理主根 |
| 打包脚本 | 新增 `scripts/package-windows.ps1`，本地即可打包 Windows 安装器 |

## 分支与维护

- `main`：只镜像上游 `main`，用于接收 [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) 的同步更新，不合并个人改动。
- `Codex/main`：个人维护主线，也是本仓库 GitHub 默认/展示分支。fork 全部改动集中在这里，采用「整合基线 + 逐个摘取上游提交」的方式跟踪上游，避免整支合并带回已裁剪的功能。

同步上游：

```bash
git remote add upstream https://github.com/BigPizzaV3/CodexPlusPlus.git
git fetch upstream
git checkout main
git merge --ff-only upstream/main
git checkout Codex/main
# 对照「上游同步记录」继续摘取需要的提交
git cherry-pick -x <upstream-commit>
```

## 上游同步记录

每次整理完上游提交，在本节追加一条记录，写明摘取范围、跳过内容和下次继续的起点，避免重复筛选。

### 2026-09-04（首次整理）

- 分叉基线：`d6b85e36`。此提交及之前的上游代码已全部包含在 `Codex/main` 中。
- 本次摘取（保留 `Codex/main` 上的 `-x` 引用，可直接 `git log` 回溯）：
  - `28ff56b` 切换供应商时保留 Hook 状态
  - `cd1eba2` TOML 语义合并替代行级去重，修复 invalid transport
  - `2950462` launcher 协议代理启动不变量
  - `93686cd` 绝对上下文窗口同步进 catalog
  - `5c521de` 无鉴权 profile 凭据保持为空
  - `9580982` 无鉴权代理启动不变量
  - `33cd20a` provider 同步生命周期守卫
  - `f70ec61` PR #1944 整体重落地：按模型元数据导入、catalog 回滚、按模型自动压缩（已含 `99e9eed`、`7271d1e`）
  - `77fc5de` 修复 Codex++ 页面下切换会话
- 明确跳过（后续默认继续跳过，除非另有决定）：
  - stepwise 悬浮面板系列（PR #2006 / #2008 / #2009 / #2018 / #2086），fork 已移除该功能
  - VLM 测试入口系列（`534014f` 起）
  - 皮肤、赞助相关改动
- 下次继续：

  ```bash
  git fetch upstream
  git log --oneline f70ec61..upstream/main
  ```

## 安装包

Windows 安装包必须使用仓库根目录的 `scripts/package-windows.ps1` 生成，不要直接运行 Tauri 默认的 `tauri build`。默认入口已固定为：

```powershell
cd apps/codex-plus-manager
npm run build
```

该命令会调用 `scripts/package-windows.ps1`，只生成 NSIS `setup.exe`：

```text
dist/windows/CodexPlusPlus-<version>-windows-x64-setup.exe
```

打包规范：

- 使用完整 Rust workspace release 构建，不能只构建 manager。
- 安装器必须同时包含 `codex-plus-plus.exe`（静默启动入口）和 `codex-plus-plus-manager.exe`（管理器）。管理器的“重启 Codex++”会从安装目录调用前者，缺少它会触发 Windows `os error 2`。
- `scripts/package-windows.ps1` 会在打包前校验两个 exe，在 NSIS 完成后校验 setup 文件存在且大小合理；任一检查失败都必须视为构建失败。
- `apps/codex-plus-manager/src-tauri/tauri.conf.json` 保持 `bundle.active = false`。Tauri 默认 bundle 可能只带 manager，不能作为本项目的发布安装包流程。
- `-SkipBuild` 只适用于已确认 `target/release` 中两个 exe 均来自同一次完整构建的场景。

如果需要修改版本号，先同步 `apps/codex-plus-manager/src-tauri/tauri.conf.json` 中的 `version`，再执行上述命令。安装前请退出正在运行的 Codex++，避免文件被占用。

## 当前功能

| 模块 | 功能 |
| --- | --- |
| 供应商配置 | 官方登录、官方登录混入 API、纯 API、聚合供应商；Responses / Chat Completions；模型测试、模型列表、Provider Doctor、cc-switch 与链接导入 |
| 模型与上下文 | 每模型上下文窗口、每模型自动压缩阈值（默认 90%）、每模型元数据导入与 catalog 回滚、`model_catalog_json`、通用配置，以及按供应商选择 MCP、Skill 和 Plugin |
| 会话管理 | 扫描本地会话、批量删除、Markdown 导出、Token 用量历史、Provider metadata 同步与备份、会话导入与分享链接 |
| Skills 与脚本 | Skills 技能管理（主根 `~/.agents/skills`）、脚本市场安装/启停 |
| 提示词与 TOML | 指令提示词页、实时 TOML 预览与编辑 |
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

每模型窗口支持 `1M`、`200K` 或纯数字；每模型自动压缩阈值接受 `90`、`84.5%` 这类写法，留空时按 Codex++ 默认 90% 落盘。Codex++ 会生成独立 `model_catalog_json`（支持元数据导入与回滚），让 Codex 按当前模型使用对应窗口与压缩行为。

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
scripts/package-windows.ps1     Windows setup 打包脚本
```

## 开源协议

上游项目 Copyright (C) 2026 BigPizzaV3。本 fork 继续沿用 [GNU Affero General Public License v3.0](LICENSE)，SPDX 标识为 `AGPL-3.0-only`。修改并分发本项目，或通过网络提供修改后的版本时，需要按 AGPLv3 提供对应源代码。

许可证只覆盖 CodexPlusPlus 自身代码，不授予 OpenAI、ChatGPT、Codex 的商标、应用资源或其他第三方内容的权利。

## 兼容性说明

Codex++ 依赖官方桌面应用的页面结构、CDP 和本地数据格式。官方应用更新后，部分注入功能可能需要跟随适配；修改供应商配置或本地会话数据前应保留备份。
