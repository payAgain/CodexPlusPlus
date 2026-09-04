use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveConfigSnapshot {
    pub path: String,
    pub exists: bool,
    pub text: String,
    pub sha256: String,
    pub parse_status: String,
    pub summary: LiveConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LiveConfigSummary {
    pub root_keys: Vec<String>,
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveConfigPreview {
    pub parse_status: String,
    pub error: Option<String>,
    pub summary: LiveConfigSummary,
}

pub fn read_live_config(home: &Path) -> anyhow::Result<LiveConfigSnapshot> {
    let path = home.join("config.toml");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let sha256 = sha256_hex(&bytes);
            let (parse_status, summary) = match text.parse::<DocumentMut>() {
                Ok(document) => ("valid".to_string(), summarize(&document)),
                Err(_) => ("invalid".to_string(), LiveConfigSummary::default()),
            };
            Ok(LiveConfigSnapshot {
                path: path.to_string_lossy().into_owned(),
                exists: true,
                text,
                sha256,
                parse_status,
                summary,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LiveConfigSnapshot {
            path: path.to_string_lossy().into_owned(),
            exists: false,
            text: String::new(),
            sha256: sha256_hex(&[]),
            parse_status: "missing".to_string(),
            summary: LiveConfigSummary::default(),
        }),
        Err(error) => Err(error.into()),
    }
}

pub fn preview_live_config(text: &str) -> LiveConfigPreview {
    match text.parse::<DocumentMut>() {
        Ok(document) => LiveConfigPreview {
            parse_status: "valid".to_string(),
            error: None,
            summary: summarize(&document),
        },
        Err(error) => LiveConfigPreview {
            parse_status: "invalid".to_string(),
            error: Some(error.to_string()),
            summary: LiveConfigSummary::default(),
        },
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn summarize(document: &DocumentMut) -> LiveConfigSummary {
    let mut root_keys = document
        .iter()
        .filter_map(|(key, item)| (!item.is_table()).then(|| key.to_string()))
        .collect::<Vec<_>>();
    let mut tables = document
        .iter()
        .filter_map(|(key, item)| item.is_table().then(|| key.to_string()))
        .collect::<Vec<_>>();
    root_keys.sort();
    tables.sort();
    LiveConfigSummary { root_keys, tables }
}

pub fn default_live_config_home() -> PathBuf {
    crate::relay_config::default_codex_home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reads_valid_config_with_hash_and_summary() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), "model = 'gpt'\n\n[profiles.dev]\nmodel='x'\n").unwrap();
        let snapshot = read_live_config(dir.path()).unwrap();
        assert!(snapshot.exists);
        assert_eq!(snapshot.parse_status, "valid");
        assert_eq!(snapshot.summary.root_keys, vec!["model"]);
        assert_eq!(snapshot.summary.tables, vec!["profiles"]);
        assert_eq!(snapshot.sha256.len(), 64);
    }

    #[test]
    fn previews_invalid_config_without_writing() {
        let preview = preview_live_config("model = [");
        assert_eq!(preview.parse_status, "invalid");
        assert!(preview.error.is_some());
    }
}
