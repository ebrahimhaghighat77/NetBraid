use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REPO: &str = "ebrahimhaghighat77/NetBraid";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTALLER_SUFFIX: &str = "-setup.exe";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    #[default]
    Idle,
    Checking,
    Current,
    Available,
    Downloading,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    url: String,
    file_name: String,
    size: u64,
    sha256: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateState {
    pub stage: Stage,
    pub release: Option<Release>,
    pub progress: Option<u8>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(&format!("NetBraid/{VERSION}"))
        .build()
}

/// The newest published release, if it has an installer.
pub fn latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(err) => return Err(format!("Couldn't reach GitHub: {err}")),
    };
    let release: ApiRelease = serde_json::from_reader(response.into_reader())
        .map_err(|err| format!("GitHub sent an unexpected answer: {err}"))?;
    let Some(asset) = release.assets.into_iter().find(|a| a.name.ends_with(INSTALLER_SUFFIX)) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version: release.tag_name.trim_start_matches('v').to_owned(),
        notes: release.body.unwrap_or_default(),
        url: asset.browser_download_url,
        file_name: asset.name,
        size: asset.size,
        sha256: asset
            .digest
            .and_then(|d| d.strip_prefix("sha256:").map(str::to_ascii_lowercase)),
    }))
}

fn parts(version: &str) -> Vec<u64> {
    version
        .trim_start_matches('v')
        .split(['.', '-'])
        .map_while(|p| p.parse().ok())
        .collect()
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parts(candidate) > parts(current)
}

/// Downloads the installer into the temp folder, checking its SHA-256
/// against the one GitHub publishes for it.
pub fn download(release: &Release, progress: impl Fn(u8)) -> Result<PathBuf, String> {
    let response = agent()
        .get(&release.url)
        .call()
        .map_err(|err| format!("The download failed: {err}"))?;
    let dir = std::env::temp_dir().join("NetBraid-update");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join(&release.file_name);
    let mut file = File::create(&path).map_err(|err| err.to_string())?;
    let mut reader = response.into_reader();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|err| format!("The download stopped: {err}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|err| err.to_string())?;
        done += n as u64;
        if let Some(percent) = (done * 100).checked_div(release.size) {
            progress(percent.min(100) as u8);
        }
    }
    drop(file);
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if let Some(expected) = &release.sha256 {
        if *expected != actual {
            let _ = std::fs::remove_file(&path);
            return Err("The download didn't match the published checksum, so it was deleted.".into());
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn the_app_and_its_bundles_share_one_version() {
        let tauri: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let npm: serde_json::Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
        assert_eq!(tauri["version"], VERSION);
        assert_eq!(npm["version"], VERSION);
    }
}
