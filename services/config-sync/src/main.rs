use axum::{extract::{DefaultBodyLimit, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}}, http::{HeaderMap, StatusCode}, response::IntoResponse, routing::{delete, get, post}, Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, path::PathBuf, sync::{Arc, Mutex}};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)] struct AppState { db: Arc<Mutex<Connection>>, events: broadcast::Sender<Event> }
#[derive(Clone, Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
struct Event { version: u64, change_id: String, device_id: String, files: serde_json::Value, #[serde(skip)] username: String }
#[derive(Deserialize)] struct Login { username: String, password: String }
#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct AccessToken { access_token: String }
#[derive(Deserialize)] struct NewDevice { name: String }
#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct DeviceToken { device_id: String, device_token: String }
#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct Device { device_id: String, name: String, revoked: bool }
#[derive(Deserialize)] struct Cursor { cursor: Option<u64> }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")] struct Change { change_id: Option<String>, device_id: String, #[serde(default)] base_version: u64, files: serde_json::Value }
#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct ChangeList { cursor: u64, latest_version: u64, events: Vec<Event> }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database = std::env::var("CONFIG_SYNC_DB_PATH").unwrap_or_else(|_| "/var/lib/codex-config-sync/sync.sqlite".to_string());
    if let Some(parent) = PathBuf::from(&database).parent() { std::fs::create_dir_all(parent)?; }
    let db = Connection::open(database)?;
    prepare_database(&db)?;
    let (events, _) = broadcast::channel(256);
    let app = Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/readyz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/v1/auth/login", post(login))
        .route("/v1/devices", get(list_devices).post(register_device))
        .route("/v1/devices/{id}", delete(revoke_device))
        .route("/v1/sync/state", get(read_changes))
        .route("/v1/sync/changes", get(read_changes).post(write_change))
        .route("/v1/sync/force-push", post(force_push))
        .route("/v1/sync/ws", get(websocket_upgrade))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(AppState { db: Arc::new(Mutex::new(db)), events });
    let bind: SocketAddr = std::env::var("CONFIG_SYNC_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string()).parse()?;
    axum::serve(tokio::net::TcpListener::bind(bind).await?, app).await?;
    Ok(())
}

fn prepare_database(db: &Connection) -> anyhow::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS users(username TEXT PRIMARY KEY,password_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS devices(device_id TEXT PRIMARY KEY,username TEXT NOT NULL,name TEXT NOT NULL,revoked INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS tokens(token_hash TEXT PRIMARY KEY,username TEXT NOT NULL,kind TEXT NOT NULL,device_id TEXT); CREATE TABLE IF NOT EXISTS events(version INTEGER PRIMARY KEY AUTOINCREMENT,username TEXT NOT NULL,change_id TEXT NOT NULL UNIQUE,device_id TEXT NOT NULL,files TEXT NOT NULL);")?;
    let user = std::env::var("CONFIG_SYNC_BOOTSTRAP_USER").unwrap_or_default();
    let password = std::env::var("CONFIG_SYNC_BOOTSTRAP_PASSWORD").unwrap_or_default();
    if !user.is_empty() && !password.is_empty() { db.execute("INSERT OR IGNORE INTO users(username,password_hash) VALUES (?,?)", params![user, hash(&password)])?; }
    Ok(())
}

async fn login(State(state): State<AppState>, Json(input): Json<Login>) -> Result<Json<AccessToken>, StatusCode> {
    let db = state.db.lock().unwrap();
    let saved: Option<String> = db.query_row("SELECT password_hash FROM users WHERE username=?", [&input.username], |row| row.get(0)).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if saved.as_deref() != Some(&hash(&input.password)) { return Err(StatusCode::UNAUTHORIZED); }
    let token = Uuid::new_v4().to_string();
    db.execute("INSERT INTO tokens(token_hash,username,kind) VALUES (?,?,'access')", params![hash(&token), input.username]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(AccessToken { access_token: token }))
}

async fn register_device(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<NewDevice>) -> Result<Json<DeviceToken>, StatusCode> {
    let user = authenticate(&state, &headers, "access")?;
    if input.name.trim().is_empty() || input.name.len() > 96 { return Err(StatusCode::BAD_REQUEST); }
    let id = Uuid::new_v4().to_string(); let token = Uuid::new_v4().to_string(); let db = state.db.lock().unwrap();
    db.execute("INSERT INTO devices(device_id,username,name) VALUES (?,?,?)", params![id, user, input.name]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.execute("INSERT INTO tokens(token_hash,username,kind,device_id) VALUES (?,?,'device',?)", params![hash(&token), user, id]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(DeviceToken { device_id: id, device_token: token }))
}

async fn list_devices(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<Device>>, StatusCode> {
    let user = authenticate(&state, &headers, "access")?;
    let db = state.db.lock().unwrap();
    let mut statement = db.prepare("SELECT device_id,name,revoked FROM devices WHERE username=? ORDER BY rowid DESC").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let devices = statement.query_map([user], |row| Ok(Device { device_id: row.get(0)?, name: row.get(1)?, revoked: row.get::<_, i64>(2)? != 0 })).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.collect::<Result<Vec<_>, _>>().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(devices))
}

async fn revoke_device(State(state): State<AppState>, headers: HeaderMap, axum::extract::Path(id): axum::extract::Path<String>) -> Result<StatusCode, StatusCode> {
    let user = authenticate(&state, &headers, "access")?;
    let db = state.db.lock().unwrap();
    let updated = db.execute("UPDATE devices SET revoked=1 WHERE device_id=? AND username=?", params![id, user]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if updated == 0 { return Err(StatusCode::NOT_FOUND); }
    db.execute("DELETE FROM tokens WHERE device_id=? AND kind='device'", [id]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn read_changes(State(state): State<AppState>, headers: HeaderMap, Query(cursor): Query<Cursor>) -> Result<Json<ChangeList>, StatusCode> {
    let user = authenticate(&state, &headers, "device")?; let from = cursor.cursor.unwrap_or(0); let db = state.db.lock().unwrap();
    let mut statement = db.prepare("SELECT version,change_id,device_id,files,username FROM events WHERE username=? AND version>? ORDER BY version").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let events = statement.query_map(params![user, from], |row| Ok(Event { version: row.get(0)?, change_id: row.get(1)?, device_id: row.get(2)?, files: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(3)?).unwrap_or_default(), username: row.get(4).unwrap_or_default() })).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.collect::<Result<Vec<_>, _>>().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let latest_version: u64 = db.query_row("SELECT COALESCE(MAX(version),0) FROM events WHERE username=?", [&user], |row| row.get(0)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let cursor = events.last().map(|event| event.version).unwrap_or(from); Ok(Json(ChangeList { cursor, latest_version, events }))
}

async fn write_change(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Change>) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user = authenticate(&state, &headers, "device")?; let id = input.change_id.unwrap_or_else(|| Uuid::new_v4().to_string()); let db = state.db.lock().unwrap();
    if id.len() > 128 || !valid_files(&input.files) { return Err(StatusCode::BAD_REQUEST); }
    let owner: Option<String> = db.query_row("SELECT username FROM devices WHERE device_id=? AND revoked=0", [&input.device_id], |row| row.get(0)).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if owner.as_deref() != Some(&user) { return Err(StatusCode::FORBIDDEN); }
    let latest: u64 = db.query_row("SELECT COALESCE(MAX(version),0) FROM events WHERE username=?", [&user], |row| row.get(0)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if input.base_version < latest { return Ok((StatusCode::CONFLICT, Json(serde_json::json!({"error":"stale_base_version","latestVersion":latest})))); }
    if let Some(event) = load_event(&db, &id)? { return Ok((StatusCode::OK, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))); }
    db.execute("INSERT INTO events(username,change_id,device_id,files) VALUES (?,?,?,?)", params![user, id, input.device_id, input.files.to_string()]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let event = Event { version: db.last_insert_rowid() as u64, change_id: id, device_id: input.device_id, files: input.files, username: user }; let _ = state.events.send(event.clone()); Ok((StatusCode::CREATED, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)))
}

async fn force_push(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Change>) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user = authenticate(&state, &headers, "device")?;
    let id = input.change_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if id.len() > 128 || !valid_files(&input.files) { return Err(StatusCode::BAD_REQUEST); }
    let db = state.db.lock().unwrap();
    let owner: Option<String> = db.query_row("SELECT username FROM devices WHERE device_id=? AND revoked=0", [&input.device_id], |row| row.get(0)).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if owner.as_deref() != Some(&user) { return Err(StatusCode::FORBIDDEN); }
    if let Some(event) = load_event(&db, &id)? { return Ok((StatusCode::OK, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?))); }
    db.execute("INSERT INTO events(username,change_id,device_id,files) VALUES (?,?,?,?)", params![user, id, input.device_id, input.files.to_string()]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let event = Event { version: db.last_insert_rowid() as u64, change_id: id, device_id: input.device_id, files: input.files, username: user };
    let _ = state.events.send(event.clone());
    Ok((StatusCode::CREATED, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)))
}

async fn websocket_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse, StatusCode> { let user = authenticate(&state, &headers, "device")?; Ok(ws.on_upgrade(move |socket| websocket(socket, state, user))) }
async fn websocket(mut socket: WebSocket, state: AppState, username: String) { let mut events = state.events.subscribe(); while let Ok(event) = events.recv().await { if event.username != username { continue; } if socket.send(Message::Text(serde_json::to_string(&event).unwrap_or_default().into())).await.is_err() { break; } } }
fn load_event(db: &Connection, id: &str) -> Result<Option<Event>, StatusCode> { db.query_row("SELECT version,change_id,device_id,files,username FROM events WHERE change_id=?", [id], |row| Ok(Event { version: row.get(0)?, change_id: row.get(1)?, device_id: row.get(2)?, files: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(3)?).unwrap_or_default(), username: row.get(4)? })).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR) }
fn authenticate(state: &AppState, headers: &HeaderMap, kind: &str) -> Result<String, StatusCode> { let token = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?; state.db.lock().unwrap().query_row("SELECT username FROM tokens WHERE token_hash=? AND kind=?", params![hash(token), kind], |row| row.get(0)).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED) }
fn valid_files(files: &serde_json::Value) -> bool {
    let Some(files) = files.as_array() else { return false; };
    files.len() <= 32 && files.iter().all(|file| {
        let path = file.get("path").and_then(serde_json::Value::as_str).unwrap_or_default();
        let content = file.get("encryptedContent").and_then(serde_json::Value::as_str).unwrap_or_default();
        !path.is_empty() && path.len() <= 256 && !path.contains("..") && !path.contains('\\') && content.len() <= 900_000
    })
}
fn hash(value: &str) -> String { let mut hasher = Sha256::new(); hasher.update(value.as_bytes()); hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect() }
