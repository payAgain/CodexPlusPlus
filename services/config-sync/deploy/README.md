# CodexPlusPlus Config Sync deployment

目标服务器：`root@43.133.2.168`。私钥仅在部署主机使用：`C:\Users\dym\.ssh\searxng_tunnel_ed25519`。

```powershell
ssh -i "C:\Users\dym\.ssh\searxng_tunnel_ed25519" root@43.133.2.168
mkdir -p /opt/codex-config-sync
```

在服务器上传仓库后执行（服务已部署到 `https://codexpp.mdyself.com`）：

```bash
cd /opt/codex-config-sync
docker compose -f services/config-sync/deploy/docker-compose.yml up -d --build
curl -fsS http://127.0.0.1:8080/healthz
```

生产环境应在前置 Caddy/Nginx 配置 HTTPS，并转发 `/v1/sync/ws` 的 WebSocket Upgrade。真实令牌、域名和密钥通过服务器环境变量或 Docker secrets 注入，不提交仓库。升级使用 `docker compose pull && docker compose up -d`; 回滚使用上一版本镜像重新启动。数据库/事件存储接入前需先配置每日卷备份。
