use super::*;
use crate::settings::{SettingsStore, atomic_write};
use anyhow::{bail, ensure};
use futures_util::{SinkExt, StreamExt};
use std::{fs, io::ErrorKind, time::Duration};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

pub static OPERATION_LOCK: Mutex<()> = Mutex::const_new(());
const NAMES: [&str; 4] = ["settings.json", "config.toml", "models.json", "auth.json"];
type Contents = BTreeMap<String, Option<Vec<u8>>>;
type Hashes = BTreeMap<String, Option<String>>;

#[derive(Clone)]
pub struct Engine {
    pub home: PathBuf,
    pub settings_path: PathBuf,
    pub state_dir: PathBuf,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            home: crate::codex_home::default_codex_home_dir(),
            settings_path: crate::paths::default_settings_path(),
            state_dir: crate::paths::default_app_state_dir().join("config-sync"),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct LocalState {
    server: String,
    device: String,
    version: u64,
    hashes: Hashes,
    enabled: bool,
    secret: String,
    last_sync: Option<u64>,
    error: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub connected: bool,
    pub enabled: bool,
    pub aligned: bool,
    pub local_version: u64,
    pub server_version: u64,
    pub last_sync: Option<u64>,
    pub error: String,
    pub files: usize,
}

#[derive(Clone, Copy)]
pub enum Operation { Push, Pull, Auto }

impl Engine {
    fn state(&self) -> anyhow::Result<LocalState> {
        let state = match fs::read(self.state_dir.join("state.json")) {
            Ok(raw) => serde_json::from_slice(&raw)?,
            Err(e) if e.kind() == ErrorKind::NotFound => LocalState::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(state)
    }

    fn save(&self, state: &LocalState) -> anyhow::Result<()> {
        atomic_write(&self.state_dir.join("state.json"), &serde_json::to_vec_pretty(state)?)
    }

    fn session(&self) -> anyhow::Result<(SyncClient, LocalState)> {
        let settings = SettingsStore::new(self.settings_path.clone()).load()?;
        let shared_token = if !settings.config_sync_token.is_empty() { settings.config_sync_token.clone() } else { settings.config_sync_device_token.clone() };
        ensure!(!shared_token.is_empty(), "请先输入同步令牌");
        let mut state = self.state()?;
        let server = settings.config_sync_server_url.trim_end_matches('/').to_string();
        if state.server != server || state.device != settings.config_sync_device_id || state.secret != shared_token {
            state = LocalState { server: server.clone(), device: settings.config_sync_device_id.clone(), secret: shared_token.clone(), ..Default::default() };
        }
        let client = SyncClient::new(SyncClientConfig { server_url: server, shared_token, device_id: settings.config_sync_device_id })?;
        Ok((client, state))
    }

    fn path(&self, name: &str) -> anyhow::Result<PathBuf> {
        ensure!(NAMES.contains(&name), "非法同步文件路径");
        Ok(match name {
            "settings.json" => self.settings_path.clone(),
            "models.json" => self.home.join("config-sync-models.json"),
            _ => self.home.join(name),
        })
    }

    fn collect(&self) -> anyhow::Result<Contents> {
        let mut contents = Contents::new();
        for name in NAMES {
            let mut path = self.path(name)?;
            if name == "models.json" {
                if let Ok(config) = fs::read_to_string(self.home.join("config.toml")) {
                    let config: toml::Value = toml::from_str(&config)?;
                    if let Some(reference) = config.get("model_catalog_json").and_then(|v| v.as_str()) {
                        path = self.home.join(reference);
                        ensure!(path.canonicalize()?.starts_with(self.home.canonicalize()?), "model_catalog_json 必须位于 Codex 配置目录内");
                    }
                }
            }
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(!meta.file_type().is_symlink() && meta.is_file(), "同步路径必须是普通文件：{name}");
                ensure!(meta.len() <= 600_000, "配置文件过大：{name}");
            }
            let content = match fs::read(&path) {
                Ok(raw) => Some(match name {
                    "settings.json" => project_settings(&raw)?,
                    "config.toml" => project_config(&raw)?,
                    _ => raw,
                }),
                Err(e) if e.kind() == ErrorKind::NotFound => None,
                Err(e) => return Err(e).with_context(|| format!("读取配置失败：{name}")),
            };
            contents.insert(name.to_owned(), content);
        }
        Ok(contents)
    }

    fn decrypt(&self, snapshot: &SyncSnapshot, secret: &str) -> anyhow::Result<Contents> {
        ensure!(snapshot.version > 0, "服务器暂无配置，请先推送本地");
        ensure!(snapshot.files.len() == NAMES.len(), "服务器是旧版或不完整快照，请在源设备推送本地以初始化");
        let mut contents = Contents::new();
        for file in &snapshot.files {
            self.path(&file.path)?;
            ensure!(!contents.contains_key(&file.path), "服务器快照存在重复文件");
            let bytes = decrypt_content(secret, &file.encrypted_content)
                .with_context(|| format!("无法解密 {}，请导入源设备恢复密钥", file.path))?;
            // 路径和删除标记位于认证密文中，防止服务端调换文件或伪造删除。
            let envelope: Envelope = serde_json::from_slice(&bytes)?;
            ensure!(envelope.path == file.path, "密文文件路径不匹配");
            let content = envelope.content.map(|s| BASE64.decode(s)).transpose()?;
            ensure!(content.as_ref().map(|c| content_hash(c)).unwrap_or_default() == file.content_hash, "配置内容校验失败");
            ensure!(!file.deleted, "不支持未认证删除标记");
            if let Some(raw) = &content { validate_content(&file.path, raw)?; }
            contents.insert(file.path.clone(), content);
        }
        ensure!(contents["settings.json"].is_some(), "快照缺少管理器设置，拒绝覆盖设备身份");
        Ok(contents)
    }

    fn encrypt(&self, contents: &Contents, state: &LocalState) -> anyhow::Result<Vec<SyncFile>> {
        contents.iter().map(|(path, content)| {
            let envelope = Envelope { path: path.clone(), content: content.as_ref().map(|c| BASE64.encode(c)) };
            Ok(SyncFile {
                path: path.clone(), version: state.version, deleted: false,
                content_hash: content.as_ref().map(|c| content_hash(c)).unwrap_or_default(),
                encrypted_content: encrypt_content(&state.secret, &serde_json::to_vec(&envelope)?)?,
            })
        }).collect()
    }

    fn apply(&self, contents: &Contents, expected: &Contents) -> anyhow::Result<()> {
        ensure!(&self.collect()? == expected, "操作期间本地配置发生变化，请重试");
        let backup = self.state_dir.join("backups").join(new_device_id());
        fs::create_dir_all(&backup)?;
        let mut originals = Contents::new();
        for (name, content) in contents {
            let path = self.path(name)?;
            let original = match fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if let Some(raw) = &original { atomic_write(&backup.join(name), raw)?; }
            originals.insert(name.clone(), original);
            if let Some(raw) = content { validate_content(name, raw)?; }
        }
        atomic_write(&backup.join("manifest.json"), &serde_json::to_vec(&hashes(&originals))?)?;
        let mut written: Vec<String> = Vec::new();
        for (name, content) in contents {
            let path = self.path(name)?;
            let outcome = (|| -> anyhow::Result<()> {
                // 每个文件写前再次检查，避免网络请求期间的本地修改被静默丢弃。
                let now = match fs::read(&path) {
                    Ok(v) => Some(v), Err(e) if e.kind() == ErrorKind::NotFound => None, Err(e) => return Err(e.into()),
                };
                ensure!(now == originals[name], "配置已被其他程序修改：{name}");
                match content {
                    Some(raw) => {
                        let bytes = if name == "settings.json" { restore_settings(raw, originals[name].as_deref())? } else { raw.clone() };
                        atomic_write(&path, &bytes)?;
                    },
                    None if path.exists() => { fs::rename(&path, backup.join(format!("{name}.removed")))?; },
                    None => {},
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                let mut rollback_errors = Vec::new();
                for prior in written.iter().rev() {
                    let path = self.path(prior)?;
                    let result = match &originals[prior] {
                        Some(raw) => atomic_write(&path, raw),
                        None => fs::rename(&path, backup.join(format!("{prior}.failed"))).map_err(Into::into),
                    };
                    if result.is_err() { rollback_errors.push(prior.clone()); }
                }
                bail!("写回失败：{error}；备份：{}；回滚失败文件：{rollback_errors:?}", backup.display());
            }
            written.push(name.clone());
        }
        Ok(())
    }

    pub async fn status(&self) -> anyhow::Result<Status> {
        let (client, mut state) = self.session()?;
        let mut result = Status { enabled: state.enabled, local_version: state.version, last_sync: state.last_sync, error: state.error.clone(), ..Default::default() };
        match client.snapshot().await {
            Ok(remote) => {
                if !state.error.is_empty() {
                    state.error.clear();
                    self.save(&state)?;
                }
                result.error.clear();
                result.connected = true;
                result.server_version = remote.version;
                result.files = remote.files.len();
                if !state.secret.is_empty() {
                    let local = self.collect()?;
                    if let Ok(contents) = self.decrypt(&remote, &state.secret) {
                        result.aligned = remote.version == state.version && local == contents && hashes(&local) == state.hashes;
                    }
                }
            }
            Err(e) => result.error = format!("无法读取服务器状态：{e}"),
        }
        Ok(result)
    }

    pub async fn set_enabled(&self, enabled: bool) -> anyhow::Result<Status> {
        let (client, mut state) = self.session()?;
        if enabled {
            ensure!(self.status().await?.aligned, "版本或内容尚未对齐，请先推送本地或拉取配置");
            client.acknowledge(state.version, &wire_hashes(&self.collect()?)).await?;
        }
        state.enabled = enabled;
        state.error.clear();
        self.save(&state)?;
        self.status().await
    }

    pub async fn execute(&self, mode: Operation) -> anyhow::Result<Status> {
        let (client, mut state) = self.session()?;
        let result = self.execute_inner(&client, &mut state, mode).await;
        if let Err(e) = result {
            // 网络故障不使本地配置不可用；冲突只暂停当前设备。
            state.error = e.to_string();
            self.save(&state)?;
            return Err(e);
        }
        self.status().await
    }

    async fn execute_inner(&self, client: &SyncClient, state: &mut LocalState, mode: Operation) -> anyhow::Result<()> {
        let local = self.collect()?;
        let local_hashes = hashes(&local);
        let remote = client.snapshot().await?;
        let push = match mode {
            Operation::Push => true,
            Operation::Pull => false,
            Operation::Auto => {
                if !state.enabled { return Ok(()); }
                ensure!(!state.hashes.is_empty(), "请先对齐配置");
                let local_changed = local_hashes != state.hashes;
                let remote_changed = remote.version != state.version;
                if remote_changed && local_changed {
                    let decoded = self.decrypt(&remote, &state.secret)?;
                    if decoded != local {
                        state.enabled = false;
                        bail!("两端配置均已修改，自动同步已暂停；请选择推送本地或拉取配置");
                    }
                }
                if !local_changed && !remote_changed { return Ok(()); }
                local_changed && !remote_changed
            },
        };
        // 显式覆盖后也不自动打开开关，必须再次经过对齐确认。
        if !matches!(mode, Operation::Auto) { state.enabled = false; }
        if push {
            for (name, raw) in &local { if let Some(raw) = raw { validate_content(name, raw)?; } }
            let change = SyncChange { change_id: new_device_id(), device_id: state.device.clone(), base_version: state.version, files: self.encrypt(&local, state)? };
            let result = if matches!(mode, Operation::Push) { client.force_push(&change).await? } else { client.push(&change).await? };
            let version = result.get("version").and_then(Value::as_u64).context("服务器未返回提交版本")?;
            ensure!(self.collect()? == local, "上传期间本地配置已变化，请再次同步");
            let check = client.snapshot().await?;
            ensure!(check.version == version && self.decrypt(&check, &state.secret)? == local, "上传后服务器已有新版本，请再次对齐");
            client.acknowledge(version, &wire_hashes(&local)).await?;
            state.version = version;
            state.hashes = local_hashes;
        } else {
            let decoded = self.decrypt(&remote, &state.secret)?;
            self.apply(&decoded, &local)?;
            ensure!(self.collect()? == decoded, "写回后配置校验失败");
            client.acknowledge(remote.version, &wire_hashes(&decoded)).await?;
            state.version = remote.version;
            state.hashes = hashes(&decoded);
        }
        state.error.clear();
        state.last_sync = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs());
        self.save(state)
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope { path: String, content: Option<String> }

fn hashes(contents: &Contents) -> Hashes {
    contents.iter().map(|(p, c)| (p.clone(), c.as_ref().map(|b| content_hash(b)))).collect()
}

// 空文件槽也以认证密文上传，服务端只比较客户端声明的摘要。
fn wire_hashes(contents: &Contents) -> Hashes {
    contents.iter().map(|(p, c)| (p.clone(), Some(c.as_ref().map(|b| content_hash(b)).unwrap_or_default()))).collect()
}

fn local_setting(key: &str) -> bool {
    key.starts_with("configSync") || matches!(key, "codexAppPath" | "codexAppUserDataDir" | "ccsDbPath" | "codexAppImageOverlayPath" | "weixinConnectWorkDir" | "weixinConnectCodexPath" | "weixinConnectToken")
}

fn project_settings(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(raw).context("settings.json 格式无效")?;
    let object = value.as_object_mut().context("settings.json 必须为对象")?;
    object.retain(|key, _| !local_setting(key));
    Ok(serde_json::to_vec(&value)?)
}

fn project_config(raw: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut doc: toml_edit::DocumentMut = std::str::from_utf8(raw)?.parse()?;
    if doc.get("model_catalog_json").is_some() {
        doc["model_catalog_json"] = toml_edit::value("config-sync-models.json");
    }
    Ok(doc.to_string().into_bytes())
}

fn restore_settings(raw: &[u8], original: Option<&[u8]>) -> anyhow::Result<Vec<u8>> {
    let mut value: Value = serde_json::from_slice(&project_settings(raw)?)?;
    if let Some(original) = original {
        let old: Value = serde_json::from_slice(original)?;
        for (key, val) in old.as_object().context("本地设置无效")? {
            if local_setting(key) { value[key] = val.clone(); }
        }
    }
    Ok(serde_json::to_vec_pretty(&value)?)
}

fn validate_content(name: &str, raw: &[u8]) -> anyhow::Result<()> {
    if name == "config.toml" { let _: toml::Value = toml::from_str(std::str::from_utf8(raw)?)?; }
    else { let _: Value = serde_json::from_slice(raw).with_context(|| format!("{name} 格式无效"))?; }
    Ok(())
}

pub async fn run_background(engine: Engine) {
    let mut previous = None;
    loop {
        let connection = {
            let _guard = OPERATION_LOCK.lock().await;
            engine.session().ok().filter(|(_, state)| state.enabled)
        };
        let Some((client, _)) = connection else { tokio::time::sleep(Duration::from_secs(2)).await; continue; };
        let mut request = match client.websocket_url().into_client_request() { Ok(v) => v, Err(_) => { tokio::time::sleep(Duration::from_secs(5)).await; continue; } };
        if let Ok(value) = format!("Bearer {}", client.config.shared_token).parse() { request.headers_mut().insert("Authorization", value); }
        let socket = tokio::time::timeout(Duration::from_secs(15), tokio_tungstenite::connect_async(request)).await;
        let mut socket = match socket { Ok(Ok((socket, _))) => Some(socket), _ => None };
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
        loop {
            tokio::select! {
                _ = tick.tick() => {},
                _ = heartbeat.tick() => {
                    if let Some(ws) = &mut socket {
                        if ws.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                    }
                    continue;
                },
                message = async { match &mut socket { Some(ws) => ws.next().await, None => std::future::pending().await } } => {
                    match message {
                        Some(Ok(Message::Ping(payload))) => { if let Some(ws) = &mut socket { let _ = ws.send(Message::Pong(payload)).await; } continue; },
                        Some(Ok(Message::Text(_))) => {},
                        Some(Ok(_)) => continue,
                        _ => break,
                    }
                }
            }
            {
                let _guard = OPERATION_LOCK.lock().await;
                let Ok((_, current)) = engine.session() else { break; };
                if !current.enabled { break; }
                let observed = engine.collect().ok().map(|v| hashes(&v));
                // 连续两次内容一致才提交，去抖并抑制自身写回。
                if observed.is_some() && observed == previous { let _ = engine.execute(Operation::Auto).await; }
                previous = observed;
            }
            if socket.is_none() { break; }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Engine) {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine { home: dir.path().join("home"), settings_path: dir.path().join("settings.json"), state_dir: dir.path().join("state") };
        fs::create_dir_all(&engine.home).unwrap();
        atomic_write(&engine.settings_path, br#"{"configSyncDeviceToken":"local-only","theme":"dark"}"#).unwrap();
        (dir, engine)
    }

    #[test]
    fn settings_projection_preserves_device_credentials() {
        let (_dir, engine) = fixture();
        let local = engine.collect().unwrap();
        let projected = String::from_utf8(local["settings.json"].clone().unwrap()).unwrap();
        assert!(!projected.contains("local-only"));
        let restored = restore_settings(br#"{"theme":"light","configSyncDeviceToken":"foreign"}"#, Some(&fs::read(engine.settings_path).unwrap())).unwrap();
        let value: Value = serde_json::from_slice(&restored).unwrap();
        assert_eq!(value["configSyncDeviceToken"], "local-only");
        assert_eq!(value["theme"], "light");
    }

    #[test]
    fn snapshot_authenticates_paths_missing_slots_and_key() {
        let (_dir, engine) = fixture();
        let content = engine.collect().unwrap();
        let state = LocalState { secret: "test-key".into(), ..Default::default() };
        let mut snapshot = SyncSnapshot { version: 1, files: engine.encrypt(&content, &state).unwrap() };
        assert_eq!(engine.decrypt(&snapshot, "test-key").unwrap(), content);
        assert!(engine.decrypt(&snapshot, "wrong-key").is_err());
        snapshot.files[0].path = "../auth.json".into();
        assert!(engine.decrypt(&snapshot, "test-key").is_err());
    }

    #[test]
    fn pull_backs_up_and_rejects_concurrent_edits() {
        let (_dir, engine) = fixture();
        atomic_write(&engine.home.join("config.toml"), b"model = 'old'\n").unwrap();
        let before = engine.collect().unwrap();
        let mut after = before.clone();
        after.insert("config.toml".into(), Some(b"model = 'new'\n".to_vec()));
        engine.apply(&after, &before).unwrap();
        assert_eq!(fs::read_to_string(engine.home.join("config.toml")).unwrap(), "model = 'new'\n");
        assert_eq!(fs::read_dir(engine.state_dir.join("backups")).unwrap().count(), 1);
        assert!(engine.apply(&before, &before).is_err());
    }
}
