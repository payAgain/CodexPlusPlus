use codex_plus_core::sync::{
    new_device_id,
    engine::{Engine, OPERATION_LOCK, Operation, Status},
};
use codex_plus_core::settings::SettingsStore;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ResultPayload<T: Serialize> {
    status: &'static str,
    message: String,
    data: Option<T>,
}

fn result<T: Serialize>(outcome: anyhow::Result<T>) -> ResultPayload<T> {
    match outcome {
        Ok(data) => ResultPayload { status: "ok", message: "操作完成".into(), data: Some(data) },
        Err(error) => ResultPayload { status: "failed", message: error.to_string(), data: None },
    }
}

#[tauri::command]
pub async fn config_sync_status() -> ResultPayload<Status> {
    let _guard = OPERATION_LOCK.lock().await;
    result(Engine::default().status().await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    server_url: String,
    token: String,
    device_name: String,
}

#[tauri::command]
pub async fn config_sync_connect(request: ConnectRequest) -> ResultPayload<()> {
    let _guard = OPERATION_LOCK.lock().await;
    result(async {
        anyhow::ensure!(!request.token.trim().is_empty(), "请输入同步令牌");
        let store = SettingsStore::default();
        let mut settings = store.load()?;
        settings.config_sync_enabled = false;
        settings.config_sync_server_url = request.server_url;
        settings.config_sync_device_name = request.device_name;
        if settings.config_sync_device_id.is_empty() { settings.config_sync_device_id = new_device_id(); }
        settings.config_sync_token = request.token.trim().to_owned();
        settings.config_sync_device_token.clear();
        settings.config_sync_cursor = 0;
        store.save(&settings)?;
        Ok(())
    }.await)
}

#[tauri::command]
pub async fn config_sync_push_local(confirmed: bool) -> ResultPayload<Status> {
    let _guard = OPERATION_LOCK.lock().await;
    if !confirmed { return result(Err(anyhow::anyhow!("请确认覆盖服务器"))); }
    result(Engine::default().execute(Operation::Push).await)
}

#[tauri::command]
pub async fn config_sync_pull_remote(confirmed: bool) -> ResultPayload<Status> {
    let _guard = OPERATION_LOCK.lock().await;
    if !confirmed { return result(Err(anyhow::anyhow!("请确认备份并覆盖本地"))); }
    result(Engine::default().execute(Operation::Pull).await)
}

#[tauri::command]
pub async fn config_sync_now() -> ResultPayload<Status> {
    let _guard = OPERATION_LOCK.lock().await;
    result(Engine::default().execute(Operation::Auto).await)
}

#[tauri::command]
pub async fn config_sync_set_enabled(enabled: bool) -> ResultPayload<Status> {
    let _guard = OPERATION_LOCK.lock().await;
    result(Engine::default().set_enabled(enabled).await)
}

pub fn start_background() {
    tauri::async_runtime::spawn(codex_plus_core::sync::engine::run_background(Engine::default()));
}
