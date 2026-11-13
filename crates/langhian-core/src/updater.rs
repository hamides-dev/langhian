use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

pub const VERSION_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/hamides-dev/langhian/main/version.json";
pub const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/hamides-dev/langhian/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub url: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub version: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub release_url: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub assets: BTreeMap<String, ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub notes: String,
    pub installer_asset: Option<ReleaseAsset>,
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid version or manifest: {0}")]
    InvalidManifest(String),
    #[error("Checksum mismatch for downloaded installer")]
    ChecksumMismatch,
    #[error("No installer asset available for this platform")]
    NoPlatformAsset,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct UpdaterClient {
    http: Client,
}

impl Default for UpdaterClient {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdaterClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("Langhian/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { http }
    }

    pub async fn check_for_updates(
        &self,
        current_version: &str,
    ) -> Result<UpdateCheckResult, UpdateError> {
        let manifest = match self.fetch_manifest().await {
            Ok(m) => m,
            Err(_) => self.fetch_github_latest_release().await?,
        };
        compare_manifest(current_version, &manifest)
    }

    async fn fetch_manifest(&self) -> Result<ReleaseManifest, UpdateError> {
        let resp = self
            .http
            .get(VERSION_MANIFEST_URL)
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        resp.json::<ReleaseManifest>()
            .await
            .map_err(|e| UpdateError::InvalidManifest(e.to_string()))
    }

    async fn fetch_github_latest_release(&self) -> Result<ReleaseManifest, UpdateError> {
        let resp = self
            .http
            .get(GITHUB_LATEST_RELEASE_URL)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;

        let tag = json["tag_name"].as_str().unwrap_or("v0.1.0").to_string();
        let version = tag.trim_start_matches('v').to_string();
        let release_url = json["html_url"]
            .as_str()
            .unwrap_or("https://github.com/hamides-dev/langhian/releases/latest")
            .to_string();
        let notes = json["body"].as_str().unwrap_or_default().to_string();

        let mut assets = BTreeMap::new();
        if let Some(arr) = json["assets"].as_array() {
            for a in arr {
                if let (Some(name), Some(url)) =
                    (a["name"].as_str(), a["browser_download_url"].as_str())
                {
                    let key = if name.contains("setup-windows") && name.ends_with(".exe") {
                        Some("windows_x86_64_installer")
                    } else if name.contains("windows") && name.ends_with(".zip") {
                        Some("windows_x86_64_portable")
                    } else if name.contains("macos-arm64") && name.ends_with(".dmg") {
                        Some("macos_arm64_dmg")
                    } else if name.contains("macos-x86_64") && name.ends_with(".dmg") {
                        Some("macos_x86_64_dmg")
                    } else if name.ends_with(".deb") {
                        Some("linux_x86_64_deb")
                    } else if name.contains("linux") && name.ends_with(".tar.gz") {
                        Some("linux_x86_64_tar")
                    } else {
                        None
                    };
                    if let Some(k) = key {
                        assets.insert(
                            k.to_string(),
                            ReleaseAsset {
                                url: url.to_string(),
                                sha256: String::new(),
                            },
                        );
                    }
                }
            }
        }

        Ok(ReleaseManifest {
            version,
            tag,
            published_at: json["published_at"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            release_url,
            notes,
            assets,
        })
    }

    pub async fn download_installer(&self, asset: &ReleaseAsset) -> Result<PathBuf, UpdateError> {
        let resp = self
            .http
            .get(&asset.url)
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !asset.sha256.trim().is_empty() {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hex::encode(hasher.finalize());
            if !digest.eq_ignore_ascii_case(asset.sha256.trim()) {
                return Err(UpdateError::ChecksumMismatch);
            }
        }

        let filename = asset
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("langhian-update");
        let dest = std::env::temp_dir().join(filename);
        std::fs::write(&dest, &bytes)?;
        Ok(dest)
    }
}

pub fn platform_asset_key() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows_x86_64_installer"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos_arm64_dmg"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos_x86_64_dmg"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux_x86_64_deb"
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    {
        "portable"
    }
}

pub fn compare_manifest(
    current_version: &str,
    manifest: &ReleaseManifest,
) -> Result<UpdateCheckResult, UpdateError> {
    let cur = Version::parse(current_version.trim().trim_start_matches('v'))
        .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;
    let latest = Version::parse(manifest.version.trim().trim_start_matches('v'))
        .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;

    let key = platform_asset_key();
    let installer_asset = manifest
        .assets
        .get(key)
        .cloned()
        .or_else(|| manifest.assets.values().next().cloned());

    Ok(UpdateCheckResult {
        current_version: cur.to_string(),
        latest_version: latest.to_string(),
        has_update: latest > cur,
        release_url: if manifest.release_url.is_empty() {
            "https://github.com/hamides-dev/langhian/releases/latest".to_string()
        } else {
            manifest.release_url.clone()
        },
        notes: manifest.notes.clone(),
        installer_asset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions_correctly() {
        let manifest = ReleaseManifest {
            version: "0.2.0".to_string(),
            tag: "v0.2.0".to_string(),
            published_at: "2026-11-20".to_string(),
            release_url: "https://github.com/hamides-dev/langhian/releases/tag/v0.2.0".to_string(),
            notes: "Bug fixes".to_string(),
            assets: BTreeMap::new(),
        };
        let res = compare_manifest("0.1.0", &manifest).expect("valid semver");
        assert!(res.has_update);

        let same = compare_manifest("0.2.0", &manifest).expect("valid semver");
        assert!(!same.has_update);
    }
}
