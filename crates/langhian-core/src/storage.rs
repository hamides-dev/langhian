use crate::history::HistoryEntry;
use crate::models::AppConfig;
use crate::vocab::VocabEntry;
use directories::ProjectDirs;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const KEYRING_SERVICE: &str = "langhian-translation";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct AppStorage {
    data_dir: PathBuf,
    locales_dir: PathBuf,
}

impl AppStorage {
    pub fn open_default() -> Self {
        let dir = ProjectDirs::from("io", "hamides-dev", "Langhian")
            .map(|p| p.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".langhian"));
        Self::open_in(dir)
    }

    pub fn open_in(data_dir: PathBuf) -> Self {
        let locales_dir = data_dir.join("locales");
        let _ = fs::create_dir_all(&data_dir);
        let _ = fs::create_dir_all(&locales_dir);
        Self {
            data_dir,
            locales_dir,
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn locales_dir(&self) -> &Path {
        &self.locales_dir
    }

    pub fn load_config(&self) -> AppConfig {
        let path = self.data_dir.join("config.json");
        let mut cfg = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<AppConfig>(&raw).ok())
            .unwrap_or_default();
        cfg.ensure_valid();
        cfg
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), StorageError> {
        let path = self.data_dir.join("config.json");
        let json = serde_json::to_string_pretty(config)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_vocab(&self) -> Vec<VocabEntry> {
        let path = self.data_dir.join("vocab.json");
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<VocabEntry>>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_vocab(&self, vocab: &[VocabEntry]) -> Result<(), StorageError> {
        let path = self.data_dir.join("vocab.json");
        let json = serde_json::to_string_pretty(vocab)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_history(&self) -> Vec<HistoryEntry> {
        let path = self.data_dir.join("history.json");
        fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<HistoryEntry>>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_history(&self, history: &[HistoryEntry]) -> Result<(), StorageError> {
        let path = self.data_dir.join("history.json");
        let json = serde_json::to_string_pretty(history)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    fallback_file: PathBuf,
}

impl SecretStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            fallback_file: data_dir.join("secrets.local.json"),
        }
    }

    pub fn get_api_key(&self, profile_id: &str) -> Option<String> {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, profile_id)
            && let Ok(secret) = entry.get_password()
            && !secret.trim().is_empty()
        {
            return Some(secret);
        }

        // Fallback for headless Linux environments lacking Secret Service daemon
        self.read_fallback_map()
            .get(profile_id)
            .cloned()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn set_api_key(&self, profile_id: &str, api_key: &str) -> Result<(), StorageError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            self.delete_api_key(profile_id);
            return Ok(());
        }

        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, profile_id)
            && entry.set_password(trimmed).is_ok()
        {
            // Remove stale fallback entry if keyring succeeded
            let mut map = self.read_fallback_map();
            if map.remove(profile_id).is_some() {
                let _ = self.write_fallback_map(&map);
            }
            return Ok(());
        }

        let mut map = self.read_fallback_map();
        map.insert(profile_id.to_string(), trimmed.to_string());
        self.write_fallback_map(&map)
    }

    pub fn delete_api_key(&self, profile_id: &str) {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, profile_id) {
            let _ = entry.delete_credential();
        }
        let mut map = self.read_fallback_map();
        if map.remove(profile_id).is_some() {
            let _ = self.write_fallback_map(&map);
        }
    }

    fn read_fallback_map(&self) -> BTreeMap<String, String> {
        fs::read_to_string(&self.fallback_file)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_fallback_map(&self, map: &BTreeMap<String, String>) -> Result<(), StorageError> {
        if map.is_empty() {
            let _ = fs::remove_file(&self.fallback_file);
            return Ok(());
        }
        let json = serde_json::to_string_pretty(map)?;
        fs::write(&self.fallback_file, json)?;
        Ok(())
    }
}
