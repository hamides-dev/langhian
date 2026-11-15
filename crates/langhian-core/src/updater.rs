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
pub const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/hamides-dev/langhian/releases?per_page=10";

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
    pub commit_sha: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl DownloadProgress {
    pub fn percent(&self) -> Option<u8> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        Some(((self.downloaded.saturating_mul(100)) / total).min(100) as u8)
    }
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
        self.check_for_updates_with_channel(current_version, false)
            .await
    }

    pub async fn check_for_updates_with_channel(
        &self,
        current_version: &str,
        include_prereleases: bool,
    ) -> Result<UpdateCheckResult, UpdateError> {
        let manifest = if include_prereleases {
            self.fetch_github_latest_including_prereleases().await?
        } else {
            match self.fetch_github_latest_release().await {
                Ok(m) => m,
                Err(_) => self.fetch_manifest().await?,
            }
        };
        compare_manifest_with_build(
            current_version,
            option_env!("LANGHIAN_BUILD_SHA"),
            &manifest,
        )
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

        Ok(parse_github_release(&json))
    }

    async fn fetch_github_latest_including_prereleases(
        &self,
    ) -> Result<ReleaseManifest, UpdateError> {
        let resp = self
            .http
            .get(GITHUB_RELEASES_URL)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let releases: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;

        let latest = releases
            .iter()
            .filter(|r| !r["draft"].as_bool().unwrap_or(false))
            .max_by(|a, b| {
                let pa = a["published_at"].as_str().unwrap_or("");
                let pb = b["published_at"].as_str().unwrap_or("");
                pa.cmp(pb)
            })
            .ok_or_else(|| {
                UpdateError::InvalidManifest("No published releases found".to_string())
            })?;

        Ok(parse_github_release(latest))
    }

    pub async fn download_installer(&self, asset: &ReleaseAsset) -> Result<PathBuf, UpdateError> {
        self.download_installer_with_progress(asset, |_| {}).await
    }

    pub async fn download_installer_with_progress<F>(
        &self,
        asset: &ReleaseAsset,
        mut on_progress: F,
    ) -> Result<PathBuf, UpdateError>
    where
        F: FnMut(DownloadProgress),
    {
        let mut resp = self
            .http
            .get(&asset.url)
            .send()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(UpdateError::Network(format!("HTTP {}", resp.status())));
        }

        let total = resp.content_length();
        let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
        on_progress(DownloadProgress {
            downloaded: 0,
            total,
        });

        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| UpdateError::Network(e.to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            on_progress(DownloadProgress {
                downloaded: bytes.len() as u64,
                total,
            });
        }

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

fn extract_nightly_commit_sha(notes: &str) -> Option<String> {
    const MARKER: &str = "Automated nightly build for commit ";
    let idx = notes.find(MARKER)?;
    let tail = &notes[idx + MARKER.len()..];
    let token = tail
        .split_whitespace()
        .next()?
        .trim_matches(|c: char| !c.is_ascii_hexdigit());
    if token.len() == 40 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

fn parse_github_release(json: &serde_json::Value) -> ReleaseManifest {
    let tag = json["tag_name"].as_str().unwrap_or("v0.1.0").to_string();
    let version = tag.trim_start_matches('v').to_string();
    let release_url = json["html_url"]
        .as_str()
        .unwrap_or("https://github.com/hamides-dev/langhian/releases/latest")
        .to_string();
    let notes = json["body"].as_str().unwrap_or_default().to_string();
    let commit_sha = extract_nightly_commit_sha(&notes);

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

    ReleaseManifest {
        version,
        tag,
        published_at: json["published_at"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        release_url,
        notes,
        commit_sha,
        assets,
    }
}

pub fn compare_manifest(
    current_version: &str,
    manifest: &ReleaseManifest,
) -> Result<UpdateCheckResult, UpdateError> {
    compare_manifest_with_build(current_version, None, manifest)
}

pub fn compare_manifest_with_build(
    current_version: &str,
    current_commit: Option<&str>,
    manifest: &ReleaseManifest,
) -> Result<UpdateCheckResult, UpdateError> {
    let cur = Version::parse(current_version.trim().trim_start_matches('v'))
        .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;

    let is_nightly = manifest.tag.eq_ignore_ascii_case("nightly")
        || manifest.version.eq_ignore_ascii_case("nightly");

    let (latest_version, has_update) = if is_nightly {
        let label = match manifest.published_at.get(..10) {
            Some(date) if !date.is_empty() => format!("nightly ({date})"),
            _ => "nightly".to_string(),
        };
        let same_commit = match (
            current_commit.map(str::trim).filter(|s| !s.is_empty()),
            manifest
                .commit_sha
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        ) {
            (Some(cur_sha), Some(rel_sha)) => cur_sha.eq_ignore_ascii_case(rel_sha),
            _ => false,
        };
        (label, !same_commit)
    } else {
        let latest = Version::parse(manifest.version.trim().trim_start_matches('v'))
            .map_err(|e| UpdateError::InvalidManifest(e.to_string()))?;
        let newer = latest > cur;
        (latest.to_string(), newer)
    };

    let key = platform_asset_key();
    let installer_asset = manifest
        .assets
        .get(key)
        .cloned()
        .or_else(|| manifest.assets.values().next().cloned());

    Ok(UpdateCheckResult {
        current_version: cur.to_string(),
        latest_version,
        has_update,
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
            commit_sha: None,
            assets: BTreeMap::new(),
        };
        let res = compare_manifest("0.1.0", &manifest).expect("valid semver");
        assert!(res.has_update);

        let same = compare_manifest("0.2.0", &manifest).expect("valid semver");
        assert!(!same.has_update);
    }

    #[test]
    fn compares_nightly_and_prerelease_versions() {
        let nightly_sha = "0123456789abcdef0123456789abcdef01234567";
        let nightly = ReleaseManifest {
            version: "nightly".to_string(),
            tag: "nightly".to_string(),
            published_at: "2026-11-14T20:00:00Z".to_string(),
            release_url: "https://github.com/hamides-dev/langhian/releases/tag/nightly".to_string(),
            notes: format!("Automated nightly build for commit {nightly_sha}.\n\nChangelog"),
            commit_sha: Some(nightly_sha.to_string()),
            assets: BTreeMap::new(),
        };
        let res = compare_manifest("0.2.0", &nightly).expect("nightly manifest");
        assert!(res.has_update);
        assert_eq!(res.latest_version, "nightly (2026-11-14)");

        let same_nightly =
            compare_manifest_with_build("0.3.0", Some(nightly_sha), &nightly).expect("same commit");
        assert!(!same_nightly.has_update);

        let older_nightly = compare_manifest_with_build(
            "0.3.0",
            Some("fedcba9876543210fedcba9876543210fedcba98"),
            &nightly,
        )
        .expect("different commit");
        assert!(older_nightly.has_update);

        let parsed = parse_github_release(&serde_json::json!({
            "tag_name": "nightly",
            "published_at": "2026-11-15T08:00:00Z",
            "html_url": "https://github.com/hamides-dev/langhian/releases/tag/nightly",
            "body": format!("Automated nightly build for commit {nightly_sha}."),
            "assets": []
        }));
        assert_eq!(parsed.commit_sha.as_deref(), Some(nightly_sha));

        let rc = ReleaseManifest {
            version: "0.2.1-rc.1".to_string(),
            tag: "v0.2.1-rc.1".to_string(),
            published_at: "2026-11-14T20:00:00Z".to_string(),
            release_url: "https://github.com/hamides-dev/langhian/releases/tag/v0.2.1-rc.1"
                .to_string(),
            notes: "RC".to_string(),
            commit_sha: None,
            assets: BTreeMap::new(),
        };
        let rc_res = compare_manifest("0.2.0", &rc).expect("prerelease semver");
        assert!(rc_res.has_update);
        assert_eq!(rc_res.latest_version, "0.2.1-rc.1");
    }

    #[test]
    fn download_progress_computes_percentage() {
        assert_eq!(
            DownloadProgress {
                downloaded: 0,
                total: Some(200),
            }
            .percent(),
            Some(0)
        );
        assert_eq!(
            DownloadProgress {
                downloaded: 84,
                total: Some(200),
            }
            .percent(),
            Some(42)
        );
        assert_eq!(
            DownloadProgress {
                downloaded: 250,
                total: Some(200),
            }
            .percent(),
            Some(100)
        );
        assert_eq!(
            DownloadProgress {
                downloaded: 50,
                total: None,
            }
            .percent(),
            None
        );
    }
}
