//! Client-side primitives for encrypted, versioned configuration synchronization.

use aes_gcm::{AeadCore, Aes256Gcm, KeyInit, aead::{Aead, OsRng}};
use anyhow::{Context, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub mod engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncClientConfig {
    pub server_url: String,
    pub access_token: String,
    pub device_token: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncLoginResponse { pub access_token: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDeviceResponse { pub device_id: String, pub device_token: String }

#[derive(Debug, Clone)]
pub struct SyncClient { http: reqwest::Client, config: SyncClientConfig }

impl SyncClient {
    pub fn new(config: SyncClientConfig) -> anyhow::Result<Self> {
        validate_server_url(&config.server_url)?;
        Ok(Self { http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).user_agent("CodexPlusPlus-ConfigSync/2").build()?, config })
    }
    fn url(&self, path: &str) -> String { format!("{}/{}", self.config.server_url.trim_end_matches('/'), path.trim_start_matches('/')) }
    pub async fn login(server_url: &str, username: &str, password: &str) -> anyhow::Result<SyncLoginResponse> {
        validate_server_url(server_url)?;
        Ok(reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build()?.post(format!("{}/v1/auth/login", server_url.trim_end_matches('/'))).json(&serde_json::json!({"username": username, "password": password})).send().await?.error_for_status()?.json().await?)
    }
    pub async fn register_device(&self, name: &str) -> anyhow::Result<SyncDeviceResponse> {
        Ok(self.http.post(self.url("/v1/devices")).bearer_auth(&self.config.access_token).json(&serde_json::json!({"name": name})).send().await?.error_for_status()?.json().await?)
    }
    pub async fn pull(&self, cursor: u64) -> anyhow::Result<serde_json::Value> {
        Ok(self.http.get(self.url("/v1/sync/changes")).bearer_auth(&self.config.device_token).query(&[("cursor", cursor)]).send().await?.error_for_status()?.json().await?)
    }
    pub async fn push(&self, change: &SyncChange) -> anyhow::Result<serde_json::Value> {
        Ok(self.http.post(self.url("/v1/sync/changes")).bearer_auth(&self.config.device_token).json(change).send().await?.error_for_status()?.json().await?)
    }
    pub async fn force_push(&self, change: &SyncChange) -> anyhow::Result<serde_json::Value> {
        Ok(self.http.post(self.url("/v1/sync/force-push")).bearer_auth(&self.config.device_token).json(change).send().await?.error_for_status()?.json().await?)
    }
    pub async fn snapshot(&self) -> anyhow::Result<SyncSnapshot> {
        Ok(self.http.get(self.url("/v1/sync/state")).bearer_auth(&self.config.device_token).send().await?.error_for_status()?.json().await?)
    }
    pub async fn acknowledge(&self, version: u64, hashes: &BTreeMap<String, Option<String>>) -> anyhow::Result<()> {
        self.http.post(self.url("/v1/sync/ack")).bearer_auth(&self.config.device_token)
            .json(&serde_json::json!({"deviceId":self.config.device_id,"version":version,"hashes":hashes}))
            .send().await?.error_for_status()?;
        Ok(())
    }
    pub fn websocket_url(&self) -> String {
        let base = self.config.server_url.replace("https://", "wss://").replace("http://", "ws://");
        format!("{}/v1/sync/ws", base.trim_end_matches('/'))
    }
}

fn validate_server_url(raw: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(raw)?;
    anyhow::ensure!(url.scheme() == "https" || (url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))), "同步服务器必须使用 HTTPS");
    anyhow::ensure!(url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(), "服务器地址不能包含凭据、查询参数或片段");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSnapshot { pub version: u64, pub files: Vec<SyncFile> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncFile {
    pub path: String,
    pub content_hash: String,
    pub version: u64,
    pub encrypted_content: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncChange {
    pub change_id: String,
    pub device_id: String,
    pub base_version: u64,
    pub files: Vec<SyncFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub path: String,
    pub field: String,
    pub local: Value,
    pub remote: Value,
    pub ancestor: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncManifest {
    pub device_id: String,
    pub cursor: u64,
    pub protocol_version: u32,
}

pub fn new_device_id() -> String { Uuid::new_v4().to_string() }

pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex_encode(&hasher.finalize())
}

pub fn derive_key(secret: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"codex-plus-config-sync-v1:");
    hasher.update(secret.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

pub fn encrypt_content(secret: &str, plaintext: &[u8]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(&derive_key(secret)).context("invalid encryption key")?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, plaintext).map_err(|_| anyhow!("encryption failed"))?;
    let mut payload = nonce.to_vec();
    payload.extend(ciphertext);
    Ok(BASE64.encode(payload))
}

pub fn decrypt_content(secret: &str, encoded: &str) -> anyhow::Result<Vec<u8>> {
    let payload = BASE64.decode(encoded).context("invalid encrypted payload")?;
    if payload.len() < 12 { return Err(anyhow!("encrypted payload is too short")); }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&derive_key(secret)).context("invalid encryption key")?;
    cipher.decrypt(nonce.into(), ciphertext).map_err(|_| anyhow!("decryption failed"))
}

pub fn merge_json_objects(ancestor: &Value, local: &Value, remote: &Value, path: &str) -> (Value, Vec<SyncConflict>) {
    let (Some(a), Some(l), Some(r)) = (ancestor.as_object(), local.as_object(), remote.as_object()) else {
        if local == ancestor { return (remote.clone(), Vec::new()); }
        if remote == ancestor || local == remote { return (local.clone(), Vec::new()); }
        return (local.clone(), vec![SyncConflict { path: path.to_string(), field: path.to_string(), local: local.clone(), remote: remote.clone(), ancestor: ancestor.clone() }]);
    };
    let mut keys = BTreeMap::new();
    for key in a.keys().chain(l.keys()).chain(r.keys()) { keys.insert(key.clone(), ()); }
    let mut merged = serde_json::Map::new();
    let mut conflicts = Vec::new();
    for key in keys.keys() {
        let av = a.get(key).unwrap_or(&Value::Null);
        let lv = l.get(key).unwrap_or(&Value::Null);
        let rv = r.get(key).unwrap_or(&Value::Null);
        let field_path = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
        let (value, mut nested) = merge_json_objects(av, lv, rv, &field_path);
        merged.insert(key.clone(), value);
        conflicts.append(&mut nested);
    }
    (Value::Object(merged), conflicts)
}

pub fn sync_candidate_paths(codex_home: &Path) -> Vec<PathBuf> {
    ["settings.json", "config.toml", "models.json", "auth.json"]
        .into_iter().map(|name| codex_home.join(name)).collect()
}

fn hex_encode(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypt_round_trip_and_hash() {
        let payload = b"secret";
        let encrypted = encrypt_content("pass", payload).unwrap();
        assert_eq!(decrypt_content("pass", &encrypted).unwrap(), payload);
        assert_ne!(encrypted, BASE64.encode(payload));
        assert_eq!(content_hash(payload).len(), 64);
    }
    #[test]
    fn merges_independent_fields_and_reports_same_field_conflict() {
        let ancestor = serde_json::json!({"a": 1, "b": 1});
        let local = serde_json::json!({"a": 2, "b": 1});
        let remote = serde_json::json!({"a": 1, "b": 3});
        let (merged, conflicts) = merge_json_objects(&ancestor, &local, &remote, "");
        assert_eq!(merged, serde_json::json!({"a": 2, "b": 3}));
        assert!(conflicts.is_empty());
        let (_, conflicts) = merge_json_objects(&ancestor, &serde_json::json!({"a": 2, "b": 1}), &serde_json::json!({"a": 3, "b": 1}), "");
        assert_eq!(conflicts.len(), 1);
    }
}
