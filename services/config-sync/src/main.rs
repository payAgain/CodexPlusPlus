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
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--backup") {
        let destination = args.get(2).ok_or_else(|| anyhow::anyhow!("backup path required"))?;
        db.execute("VACUUM INTO ?", [destination])?;
        return Ok(());
    }
    prepare_database(&db)?;
    let (events, _) = broadcast::channel(256);
    let app = Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/readyz", get(ready))
        .route("/v1/auth/login", post(login))
        .route("/v1/devices", get(list_devices).post(register_device))
        .route("/v1/devices/{id}", delete(revoke_device))
        .route("/v1/sync/state", get(read_snapshot))
        .route("/v1/sync/ack", post(acknowledge))
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
    db.execute_batch("CREATE TABLE IF NOT EXISTS replacements(version INTEGER PRIMARY KEY); CREATE TABLE IF NOT EXISTS acknowledgements(device_id TEXT PRIMARY KEY,version INTEGER NOT NULL);")?;
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
    commit_change(&state, &headers, input, false)
}

async fn force_push(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Change>) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    commit_change(&state, &headers, input, true)
}

fn commit_change(state: &AppState, headers: &HeaderMap, input: Change, replace: bool) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user = authenticate(state, headers, "device")?;
    let id = input.change_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if id.is_empty() || id.len() > 128 || !valid_files(&input.files) { return Err(StatusCode::BAD_REQUEST); }
    let mut db = state.db.lock().unwrap();
    check_device(&db, headers, &input.device_id)?;
    let tx = db.transaction().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(event) = load_event(&tx, &id)? {
        let was_replace = tx.query_row("SELECT EXISTS(SELECT 1 FROM replacements WHERE version=?)", [event.version], |r| r.get::<_, bool>(0)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if event.username != user || event.device_id != input.device_id || event.files != input.files || was_replace != replace { return Err(StatusCode::CONFLICT); }
        return Ok((StatusCode::OK, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)));
    }
    let latest: u64 = tx.query_row("SELECT COALESCE(MAX(version),0) FROM events WHERE username=?", [&user], |r| r.get(0)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !replace && input.base_version != latest { return Ok((StatusCode::CONFLICT, Json(serde_json::json!({"error":"stale_base_version","latestVersion":latest})))); }
    tx.execute("INSERT INTO events(username,change_id,device_id,files) VALUES (?,?,?,?)", params![user, id, input.device_id, input.files.to_string()]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let event = Event { version: tx.last_insert_rowid() as u64, change_id: id, device_id: input.device_id, files: input.files, username: user };
    if replace { tx.execute("INSERT INTO replacements(version) VALUES (?)", [event.version]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?; }
    tx.commit().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = state.events.send(event.clone());
    Ok((StatusCode::CREATED, Json(serde_json::to_value(event).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot { version: u64, files: Vec<serde_json::Value> }

fn snapshot(db: &Connection, user: &str) -> Result<Snapshot, StatusCode> {
    let mut stmt = db.prepare("SELECT e.version,e.files,EXISTS(SELECT 1 FROM replacements r WHERE r.version=e.version) FROM events e WHERE username=? ORDER BY e.version").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([user], |r| Ok((r.get::<_, u64>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut files = std::collections::BTreeMap::new();
    let mut version = 0;
    for row in rows {
        let (v, raw, replace) = row.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        version = v;
        if replace { files.clear(); }
        let items: Vec<serde_json::Value> = serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        for item in items {
            let path = item.get("path").and_then(|p| p.as_str()).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?.to_owned();
            files.insert(path, item);
        }
    }
    Ok(Snapshot { version, files: files.into_values().collect() })
}

async fn read_snapshot(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Snapshot>, StatusCode> {
    let user = authenticate(&state, &headers, "device")?;
    Ok(Json(snapshot(&state.db.lock().unwrap(), &user)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ack { device_id: String, version: u64, hashes: std::collections::BTreeMap<String, Option<String>> }

async fn acknowledge(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<Ack>) -> Result<StatusCode, StatusCode> {
    let user = authenticate(&state, &headers, "device")?;
    let db = state.db.lock().unwrap();
    check_device(&db, &headers, &input.device_id)?;
    let current = snapshot(&db, &user)?;
    let hashes: std::collections::BTreeMap<_, _> = current.files.iter().map(|f| {
        (f["path"].as_str().unwrap_or_default().to_owned(), if f["deleted"] == true { None } else { f["contentHash"].as_str().map(str::to_owned) })
    }).collect();
    if current.version == 0 || current.version != input.version || hashes != input.hashes { return Err(StatusCode::CONFLICT); }
    db.execute("INSERT INTO acknowledgements(device_id,version) VALUES (?,?) ON CONFLICT(device_id) DO UPDATE SET version=excluded.version", params![input.device_id, input.version]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

fn check_device(db: &Connection, headers: &HeaderMap, device: &str) -> Result<(), StatusCode> {
    let token = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?;
    let valid = db.query_row("SELECT EXISTS(SELECT 1 FROM tokens t JOIN devices d ON t.device_id=d.device_id WHERE t.token_hash=? AND t.kind='device' AND d.device_id=? AND d.revoked=0)", params![hash(token), device], |r| r.get::<_, bool>(0)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid { return Err(StatusCode::FORBIDDEN); }
    Ok(())
}
async fn ready(State(state): State<AppState>) -> StatusCode {
    match state.db.lock() {
        Ok(db) if db.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_ok() => StatusCode::NO_CONTENT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
async fn websocket_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>, headers: HeaderMap) -> Result<impl IntoResponse, StatusCode> {
    let user = authenticate(&state, &headers, "device")?;
    Ok(ws.on_upgrade(move |socket| websocket(socket, state, user, headers)))
}
async fn websocket(mut socket: WebSocket, state: AppState, username: String, headers: HeaderMap) {
    let mut events = state.events.subscribe();
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if authenticate(&state, &headers, "device").is_err() { break; }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Ping(bytes))) => { if socket.send(Message::Pong(bytes)).await.is_err() { break; } },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {},
            },
            event = events.recv() => {
                match event {
                    Ok(event) if event.username == username => {
                        if authenticate(&state, &headers, "device").is_err() { break; }
                        if socket.send(Message::Text(serde_json::to_string(&event).unwrap_or_default().into())).await.is_err() { break; }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                    Err(broadcast::error::RecvError::Closed) => break,
                    _ => {},
                }
            },
        }
    }
}
fn load_event(db: &Connection, id: &str) -> Result<Option<Event>, StatusCode> { db.query_row("SELECT version,change_id,device_id,files,username FROM events WHERE change_id=?", [id], |row| Ok(Event { version: row.get(0)?, change_id: row.get(1)?, device_id: row.get(2)?, files: serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(3)?).unwrap_or_default(), username: row.get(4)? })).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR) }
fn authenticate(state: &AppState, headers: &HeaderMap, kind: &str) -> Result<String, StatusCode> { let token = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?; state.db.lock().unwrap().query_row("SELECT username FROM tokens WHERE token_hash=? AND kind=?", params![hash(token), kind], |row| row.get(0)).optional().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED) }
fn valid_files(files: &serde_json::Value) -> bool {
    let Some(files) = files.as_array() else { return false; };
    let mut paths = std::collections::BTreeSet::new();
    !files.is_empty() && files.len() <= 4 && files.iter().all(|file| {
        let path = file.get("path").and_then(serde_json::Value::as_str).unwrap_or_default();
        let content = file.get("encryptedContent").and_then(serde_json::Value::as_str).unwrap_or_default();
        let hash = file.get("contentHash").and_then(serde_json::Value::as_str).unwrap_or_default();
        let deleted = file.get("deleted").and_then(serde_json::Value::as_bool);
        matches!(path, "settings.json" | "config.toml" | "models.json" | "auth.json")
            && paths.insert(path) && content.len() <= 900_000
            && match deleted { Some(true) => content.is_empty() && hash.is_empty(), Some(false) => content.len() >= 40 && (hash.is_empty() || (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))), None => false }
    })
}
fn hash(value: &str) -> String { let mut hasher = Sha256::new(); hasher.update(value.as_bytes()); hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (AppState, HeaderMap) {
        let db = Connection::open_in_memory().unwrap();
        prepare_database(&db).unwrap();
        db.execute("INSERT INTO devices VALUES ('a','user','A',0)", []).unwrap();
        db.execute("INSERT INTO devices VALUES ('b','user','B',0)", []).unwrap();
        db.execute("INSERT INTO tokens VALUES (?,'user','device','a')", [hash("token-a")]).unwrap();
        let (events, _) = broadcast::channel(8);
        let state = AppState { db: Arc::new(Mutex::new(db)), events };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer token-a".parse().unwrap());
        (state, headers)
    }
    fn change(id: &str, base: u64, path: &str) -> Change {
        Change { change_id: Some(id.into()), device_id: "a".into(), base_version: base,
            files: serde_json::json!([{"path":path,"encryptedContent":"x".repeat(40),"contentHash":"a".repeat(64),"deleted":false}]) }
    }
    #[test]
    fn compare_and_swap_retry_and_replace_snapshot() {
        let (state, headers) = fixture();
        assert_eq!(commit_change(&state, &headers, change("first",0,"settings.json"),false).unwrap().0, StatusCode::CREATED);
        assert_eq!(commit_change(&state, &headers, change("first",0,"settings.json"),false).unwrap().0, StatusCode::OK);
        assert_eq!(commit_change(&state, &headers, change("stale",0,"config.toml"),false).unwrap().0, StatusCode::CONFLICT);
        assert_eq!(commit_change(&state, &headers, change("future",99,"config.toml"),false).unwrap().0, StatusCode::CONFLICT);
        assert_eq!(commit_change(&state, &headers, change("replace",0,"config.toml"),true).unwrap().0, StatusCode::CREATED);
        let snap = snapshot(&state.db.lock().unwrap(), "user").unwrap();
        assert_eq!(snap.files.len(),1);
        assert_eq!(snap.files[0]["path"],"config.toml");
    }
    #[test]
    fn device_binding_and_idempotency_payload_are_enforced() {
        let (state, headers) = fixture();
        let mut spoof = change("spoof",0,"auth.json");
        spoof.device_id = "b".into();
        assert_eq!(commit_change(&state,&headers,spoof,true).unwrap_err(),StatusCode::FORBIDDEN);
        let _ = commit_change(&state,&headers,change("first",0,"settings.json"),true).unwrap();
        assert_eq!(commit_change(&state,&headers,change("first",0,"auth.json"),true).unwrap_err(),StatusCode::CONFLICT);
    }
    #[test]
    fn another_users_version_does_not_block_commit() {
        let (state, headers) = fixture();
        state.db.lock().unwrap().execute("INSERT INTO events(username,change_id,device_id,files) VALUES ('other','other-id','other-device','[]')", []).unwrap();
        assert_eq!(commit_change(&state,&headers,change("first",0,"settings.json"),false).unwrap().0,StatusCode::CREATED);
        assert_eq!(snapshot(&state.db.lock().unwrap(),"other").unwrap().version,1);
    }
    #[tokio::test]
    async fn acknowledgement_requires_version_and_content() {
        let (state, headers) = fixture();
        let _ = commit_change(&state,&headers,change("first",0,"settings.json"),true).unwrap();
        let wrong = Ack {device_id:"a".into(),version:1,hashes:Default::default()};
        assert_eq!(acknowledge(State(state.clone()),headers.clone(),Json(wrong)).await.unwrap_err(),StatusCode::CONFLICT);
        let correct = Ack {device_id:"a".into(),version:1,hashes:[("settings.json".into(),Some("a".repeat(64)))].into()};
        assert_eq!(acknowledge(State(state),headers,Json(correct)).await.unwrap(),StatusCode::NO_CONTENT);
    }
    #[test]
    fn rejects_paths_and_duplicates() {
        for path in ["../auth.json","/etc/passwd","C:\\auth.json","config.toml/secret"] {
            assert!(!valid_files(&change("x",0,path).files));
        }
        let file=change("x",0,"auth.json").files[0].clone();
        assert!(!valid_files(&serde_json::json!([file,file])));
    }
}
