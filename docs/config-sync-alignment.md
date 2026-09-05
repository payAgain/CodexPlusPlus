# 配置覆盖与自动同步

开发分支：`codex/config-sync`。协议更新日期：2026-09-05。

## 使用顺序

1. 源设备登录后选择「推送本地（覆盖服务器）」，确认覆盖。旧版服务器记录不会被当作已对齐。
2. 源设备显示恢复密钥，安全地传给另一台设备。恢复密钥不会上传服务器，与设备登录令牌不同。
3. 另一台设备登录后导入恢复密钥，选择「拉取配置（覆盖本地）」。
4. 两台设备分别显示「已对齐」后，可各自开启自动同步。开启时后端重新校验内容，并向服务器确认版本和摘要。
5. 两边同时修改时暂停自动同步。选择一个覆盖方向重新对齐，再开启开关。

## 数据范围

- `settings.json` 来自 `.codex-session-delete/settings.json`，不是 `.codex/settings.json`。
- 同步设置剔除设备令牌、服务器地址、同步开关、设备游标以及部分机器专属路径。
- `config.toml`、`auth.json` 来自当前 Codex home。
- `models.json` 是协议内的模型目录槽。读取 `model_catalog_json` 指向的 home 内文件；接收端写入 `config-sync-models.json`，同步配置中的指针使用此相对路径。
- 缺失文件作为加密槽同步。强拉时将对应本地文件移入备份目录，不直接删除。
- 不同步会话、数据库、历史和日志。所有文件内容在客户端 AES-GCM 加密。
- 任意外部 catalog 路径不自动读取，需先放入 Codex home。

## 安全与恢复

先解密、校验完整快照，再写入。每次拉取备份放在：
`%USERPROFILE%/.codex-session-delete/config-sync/backups/<唯一操作ID>/`。
备份 manifest 标识原文件存在状态。写回使用原子文件替换，写回失败尝试回滚，不推进对齐版本。
恢复需先关闭自动同步与管理器，再逐个恢复备份中的文件。

设备本地状态与恢复密钥保存在 `config-sync/state.json`，不进入同步文件集。
该文件和备份应按凭据文件保护；当前尚未接入系统凭据保险库。

自动同步在管理器进程存活时运行，2 秒周期内容检测、连续稳定采样去抖，
WebSocket 通知与心跳、断线重连和快照补偿。离线不影响 Codex 本地启动。
它采用保守的文件集冲突暂停策略，不声称已经提供字段级冲突编辑器。

## 回归测试

```powershell
cargo test -p codex-config-sync
cargo test -p codex-plus-core sync --lib
cargo build -p codex-config-sync
$env:CONFIG_SYNC_TEST_SERVER_BIN = (Resolve-Path target/debug/codex-config-sync.exe)
cargo test -p codex-plus-core --test config_sync_e2e -- --ignored
```

端到端测试使用临时数据库、临时设备文件和本机回环端口，不访问生产配置。
