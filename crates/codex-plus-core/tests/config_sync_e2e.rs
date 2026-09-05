use codex_plus_core::{
    settings::{BackendSettings, SettingsStore, atomic_write},
    sync::{SyncClient, SyncClientConfig, engine::{Engine, Operation}},
};
use std::{path::PathBuf, process::{Child, Command, Stdio}, time::Duration};

struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

async fn device(root: PathBuf, url: &str, name: &str) -> Engine {
    let login = SyncClient::login(url, "test-user", "test-password").await.unwrap();
    let client = SyncClient::new(SyncClientConfig { server_url: url.into(), access_token: login.access_token, device_id: String::new(), device_token: String::new() }).unwrap();
    let device = client.register_device(name).await.unwrap();
    let engine = Engine { home: root.join("home"), settings_path: root.join("settings.json"), state_dir: root.join("state") };
    std::fs::create_dir_all(&engine.home).unwrap();
    SettingsStore::new(engine.settings_path.clone()).save(&BackendSettings {
        config_sync_server_url: url.into(),
        config_sync_device_id: device.device_id,
        config_sync_device_token: device.device_token,
        config_sync_device_name: name.into(),
        ..Default::default()
    }).unwrap();
    atomic_write(&engine.home.join("config.toml"), format!("model = '{name}'\n").as_bytes()).unwrap();
    engine
}

#[tokio::test]
#[ignore = "requires locally built server; see config-sync deployment guide"]
async fn config_sync_two_devices_overwrite_gate_conflict_and_recovery() {
    let binary = std::env::var_os("CONFIG_SYNC_TEST_SERVER_BIN").expect("Set CONFIG_SYNC_TEST_SERVER_BIN to the locally built sync service binary");
    let dir = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let database = dir.path().join("service.sqlite");
    let spawn = || Service(Command::new(&binary)
        .env("CONFIG_SYNC_BIND",address.to_string())
        .env("CONFIG_SYNC_DB_PATH",&database)
        .env("CONFIG_SYNC_BOOTSTRAP_USER","test-user")
        .env("CONFIG_SYNC_BOOTSTRAP_PASSWORD","test-password")
        .stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let service = spawn();
    let url = format!("http://{address}");
    for _ in 0..100 {
        if reqwest::get(format!("{url}/readyz")).await.is_ok() { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let a = device(dir.path().join("a"),&url,"device-a").await;
    let b = device(dir.path().join("b"),&url,"device-b").await;
    assert!(a.set_enabled(true).await.is_err());
    assert!(a.execute(Operation::Push).await.unwrap().aligned);
    let key = a.recovery_key().unwrap();
    assert!(b.execute(Operation::Pull).await.is_err());
    assert_eq!(std::fs::read_to_string(b.home.join("config.toml")).unwrap(),"model = 'device-b'\n");
    b.import_key(key).unwrap();
    assert!(b.execute(Operation::Pull).await.unwrap().aligned);
    let b_token = SettingsStore::new(b.settings_path.clone()).load().unwrap().config_sync_device_token;
    assert_ne!(b_token,SettingsStore::new(a.settings_path.clone()).load().unwrap().config_sync_device_token);
    a.set_enabled(true).await.unwrap();
    b.set_enabled(true).await.unwrap();
    let background = tokio::spawn(codex_plus_core::sync::engine::run_background(a.clone()));
    atomic_write(&a.home.join("config.toml"),b"model = 'background'\n").unwrap();
    let mut propagated = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if b.status().await.unwrap().server_version > 1 {
            b.execute(Operation::Auto).await.unwrap();
            propagated = std::fs::read_to_string(b.home.join("config.toml")).unwrap() == "model = 'background'\n";
            break;
        }
    }
    background.abort();
    let _ = background.await;
    assert!(propagated, "background file-change detection must upload automatically");
    atomic_write(&a.home.join("config.toml"),b"model = 'updated'\n").unwrap();
    assert!(a.execute(Operation::Auto).await.unwrap().aligned);
    assert!(b.execute(Operation::Auto).await.unwrap().aligned);
    assert_eq!(std::fs::read_to_string(b.home.join("config.toml")).unwrap(),"model = 'updated'\n");
    atomic_write(&a.home.join("config.toml"),b"model = 'conflict-a'\n").unwrap();
    atomic_write(&b.home.join("config.toml"),b"model = 'conflict-b'\n").unwrap();
    a.execute(Operation::Auto).await.unwrap();
    assert!(b.execute(Operation::Auto).await.is_err());
    assert!(!b.status().await.unwrap().enabled);
    assert_eq!(std::fs::read_to_string(b.home.join("config.toml")).unwrap(),"model = 'conflict-b'\n");
    assert!(b.execute(Operation::Push).await.unwrap().aligned);
    assert!(a.execute(Operation::Pull).await.unwrap().aligned);
    assert!(!a.status().await.unwrap().enabled);
    let version = a.status().await.unwrap().server_version;
    drop(service);
    assert!(!a.status().await.unwrap().connected);
    let _restarted = spawn();
    for _ in 0..100 {
        if reqwest::get(format!("{url}/readyz")).await.is_ok() { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(a.status().await.unwrap().server_version,version);
    assert!(a.status().await.unwrap().aligned);
}
