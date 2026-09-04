# Codex-X UI 与功能融合方案

> 状态：规划
> 基准分支：`codex/remove-extra-features`
> 集成分支：`codex/ui-feature-integration`
> 更新日期：2026-09-03

## 一、目标

参考 Codex-X 项目的 UI 设计与四块能力，融合进本仓库（CodexPlusPlus fork），同时保留现有全部功能：供应商/Relay Profile、按模型粒度配置上下文窗口、会话管理、MCP&插件、Skills、脚本市场、微信连接、Codex 增强、安装维护、设置、关于。

- UI 重写：参考 Codex-X 视觉体系重写 Manager。
- 指令提示词迁移：在线目录 + 本地模板 + 导入 md + 分类 + append/replace 注入。
- 在线配置管理迁移：针对 `~/.codex/config.toml` 的 live TOML 读写、校验、原子保存、备份与恢复。
- 子代理执行守卫：由 Codex++ 管理 `SubagentStart` / `SubagentStop` hook，降低子代理只确认任务、不调用工具执行的空跑概率。

## 二、总体策略

采用 `model_catalog_json` 机制（与本 fork 现有按模型窗口配置一致），不重写后端供应商行为。UI 一次性整站重写，但按 worktree 拆分为可并发的三条业务线，最后统一合并到集成分支。

Codex-X 为 MIT 协议，可作为设计与部分实现参考；合并项目仍按 AGPL-3.0-only 发布，复用较多时新增第三方声明。

## 三、Worktree 布局

| Worktree | 分支 | 路径 | 职责 |
|---|---|---|---|
| 集成 | `codex/ui-feature-integration` | `E:\Work\CodexPlusPlus-wt-integration` | 合并协调、冲突处理、跨功能回归、最终 PR |
| UI | `codex/x-ui-rewrite` | `E:\Work\CodexPlusPlus-wt-ui` | 设计 token、AppShell、导航、通用组件、页面路由 |
| 提示词 | `codex/prompt-injection` | `E:\Work\CodexPlusPlus-wt-prompts` | 在线目录、本地模板、分类、append/replace 注入、子代理执行守卫 |
| Live TOML | `codex/live-toml-management` | `E:\Work\CodexPlusPlus-wt-live-config` | live config 读取、校验、diff、备份与恢复 |

所有 worktree 从 `codex/remove-extra-features` 切出，不在主工作区直接开发。

## 四、UI 重写

- 拆分 9000+ 行 `App.tsx`：路由壳、页面目录、hooks/context、命令适配层、通用 UI 组件；`App.tsx` 只保留全局状态、路由与 Provider 编排。
- 采用 Codex-X 视觉：256px 侧栏、分组导航、浅灰画布、白色面板、8px 圆角、蓝色主操作、eyebrow + 大标题页头、状态胶囊、顶部 toast、亮/暗主题；保留 Inter/JetBrains Mono 与 lucide-react。
- 保留全部现有 route 与数据加载：overview、relay、sessions、context、skills、weixin、enhance、userScripts、maintenance、about、settings、relayEnvironment。
- 不新增前端依赖。

## 五、指令提示词

- 在线目录默认直连 Codex-X examples（jsDelivr 目录优先，GitHub Contents/raw 回退），支持缓存、刷新、失败时保留可用缓存。
- 本地自定义模板 + 导入 Markdown；持久化在 `~/.codex-session-delete/prompts/`，不改 `settings.json`。
- 启用支持 `append` 与 `replace`：
  - `append`：写带管理标记的 `AGENTS.md` 区块，保留原提示词。
  - `replace`：写受管 md，并设置 `model_instructions_file`；先备份受影响文件。
  - 同一时间只允许一个活动模板。
  - 禁用只移除管理区块/指针，不删除用户外部提示词。
- 在线模板必须显式启用，不自动注入；严格校验 `.md`、文件名、路径穿越、重复 ID、内容大小（上限 256KB）；远程请求 5 秒超时。
- 后端命令：`list_prompt_templates`、`refresh_prompt_templates`、`get_prompt_template`、`save_prompt_template`、`delete_prompt_template`、`import_prompt_markdown`、`enable_prompt_template`、`disable_prompt`、`save_prompt_categories`。

## 六、子代理执行守卫

- 作为 Codex++ 的 opt-in 增强开关提供，默认关闭；用户在 Manager 的“Codex 增强”页面启用或停用，不要求手工编辑 Codex 配置文件或维护外部 PowerShell 脚本。
- Codex++ 提供内置 hook runner，读取 Codex 通过 stdin 传入的 hook JSON，并在 stdout 输出协议兼容的 JSON：
  - `SubagentStart`：注入 `additionalContext`，明确首条含可执行目标的消息是 active task，必须立即检查工作区、调用工具并闭环执行，禁止只确认或复述。
  - `SubagentStop`：识别“已收到规范、请发送具体需求”等空确认回复；命中且缺少文件、命令、测试、提交或阻塞证据时返回 `decision: "block"`，要求当前子代理继续执行。
  - 使用 `stop_hook_active` 等重入标记避免停止 hook 自循环；检测逻辑只做保守匹配，正常分析、提问和真实阻塞不得被误拦截。
- Codex++ 负责安装、升级和移除自己的 hook 注册：
  - 对 `~/.codex/hooks.json` 做结构化 JSON 解析和增量合并，不覆盖整个文件。
  - 使用稳定的 Codex++ 管理标识识别自有条目，重复启用保持幂等，升级时原位更新。
  - 完整保留 Nebula、用户脚本和其他第三方 hook 的事件、matcher、顺序及未知字段。
  - 停用或卸载时只移除 Codex++ 自有的 `SubagentStart` / `SubagentStop` 条目；事件下仍有其他 hook 时保留事件节点。
  - 写入前创建备份，采用临时文件 + 原子替换；JSON 无法解析、Codex 版本不支持对应事件或目标可执行文件不可用时拒绝修改并返回可操作错误。
- hook 命令指向 Codex++ 随应用交付的稳定可执行入口，路径参数按 Windows/macOS/Linux 正确转义；hook runner 不依赖 Manager 正在运行，不访问网络，不读取或输出 `auth.json`、`config.toml` 凭据。
- 状态查询返回 `supported`、`enabled`、`installedVersion`、`healthy`、`configPath`、冲突/错误摘要；UI 展示实际安装状态，并提供“修复注册”操作处理应用路径变化或条目被外部修改的情况。
- 后端能力建议：`get_subagent_guard_status`、`set_subagent_guard_enabled`、`repair_subagent_guard`；设置字段建议为 `codexSubagentGuardEnabled`，保存设置时调用专用同步函数，不把 hook 合并逻辑塞进 Relay 或 Live TOML 供应商流程。
- 调度侧仍需使用自包含、祈使式的子代理任务消息，并优先传递足够历史；hook 是兜底增强，不承诺修复缺失目标、错误 worktree 或父代理主动中断等问题。

## 七、Live TOML 管理

- `read_live_config`：路径、是否存在、文本、内容 SHA-256、解析状态、摘要。
- `preview_live_config`：解析候选 TOML，返回错误位置/消息、受影响 root key/table 摘要；不写文件。
- `save_live_config`：读取时 SHA-256 并发检查，外部修改则拒绝；先备份，原子写入，失败回滚；默认只编辑 `config.toml`，不显示/修改 `auth.json` 密钥。
- `list_live_backups`：列出 `~/.codex/backups/codex-plus-live-*` 的时间、路径、包含文件与大小。
- `restore_live_backup`：恢复前再备份当前文件；必要时处理备份内 `auth.json`，但不把凭据写入 API 响应或日志。
- 与供应商切换共存：复用/接入现有 `relay_switch_mutex` 与写入锁；不重写 `relay_config.rs` 供应商行为，不绕过 model catalog、desktop personalization、marketplace 保留逻辑。

## 八、并发与合并顺序

第一波并行（三条独立后端/前端基线）：
1. UI Shell 重写。
2. Prompt Injection 与子代理执行守卫后端能力。
3. Live TOML 后端能力。

第二波（依附于 UI Shell 合入后）：
1. `PromptsPage` 接入提示词后端，并在“Codex 增强”页面接入子代理执行守卫开关与健康状态。
2. `LiveTomlPage` 接入 live TOML 后端。
3. 继续迁移供应商、会话、MCP&插件、Skills、微信、增强、脚本、维护、设置、关于页面。

合并顺序：UI → Live TOML → Prompt → 前端页面。命令注册点各自独立，仅保留很小的注册行冲突面。

## 九、边界与安全

- `App.tsx` 破坏性拆分只允许 UI worktree 修改；后端 worktree 不碰它。
- Prompt 与 Live TOML 先交付稳定 command 契约，前端页面等 UI Shell 合并后接入。
- `relay_config.rs` 供应商行为默认不重写。
- 新增持久化目录 `~/.codex-session-delete/prompts/`，不改 `settings.json` 结构。
- 子代理执行守卫只能结构化合并 `hooks.json`，禁止覆盖用户完整配置；备份不得包含额外凭据文件，日志不得记录完整 hook stdin、用户 prompt 或会话 transcript。
- 不修改 `Cargo.toml` / `package.json`，优先复用现有 `reqwest`、`sha2`、`serde`、`serde_json`、`toml_edit`、`tempfile`。

## 十、验证

- UI：`pnpm check`、`pnpm test`、`pnpm vite:build`。
- Prompt / Live：`cargo test -p codex-plus-core`（涉及 manager 时 `cargo test -p codex-plus-manager`）。
- 子代理执行守卫：覆盖空配置、保留第三方 hook、重复启用幂等、升级替换、停用精准移除、损坏 JSON 拒绝写入、路径转义、`SubagentStart` 上下文注入、`SubagentStop` 空确认拦截、正常完成放行和防重入测试。
- 集成：`cargo test`、`pnpm check`、`pnpm test`、`pnpm vite:build`，并运行 Tauri dev 检查亮/暗主题、路由、提示词启停、子代理执行守卫启停/修复、TOML 保存/恢复、供应商切换。

## 十一、已确认决策

- 在线配置管理按 live TOML 管理实现。
- 提示词默认数据源直连 Codex-X examples，模板内容需用户显式启用。
- 子代理执行守卫由 Codex++ 内置和管理，不要求用户直接修改 Codex 配置；启停必须保留现有第三方 hook，hook 只作为调度 prompt 与 `followup_task` 恢复机制的兜底。
- 不迁移 Codex-X 的会话管理与供应商模型作为替代品；当前 CodexPlusPlus 供应商与上下文窗口体系是唯一事实来源。
