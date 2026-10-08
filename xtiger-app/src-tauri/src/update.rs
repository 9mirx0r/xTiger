//! Finding newer releases of xTiger on GitHub, and installing them.
//!
//! The installed app downloads the new setup and starts it with `--update`; the setup waits for
//! the app to close, installs over it and opens it again. A portable copy only shows what is new
//! and links to the release page, because it was never installed.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;

const RELEASES_URL: &str = "https://api.github.com/repos/9mirx0r/xTiger/releases?per_page=30";
const RELEASE_PAGE_PREFIX: &str = "https://github.com/9mirx0r/xTiger/releases";
const USER_AGENT: &str = concat!("xTiger/", env!("CARGO_PKG_VERSION"));
/// A setup bigger than this is not ours.
const MAX_SETUP_SIZE: u64 = 200 * 1024 * 1024;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    /// `sha256:<hex>`, which GitHub computes for every uploaded asset.
    digest: Option<String>,
}

/// One release newer than this copy, with its notes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotes {
    version: String,
    name: String,
    notes: String,
    date: Option<String>,
    url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// The newest version.
    version: String,
    /// The release page of the newest version.
    url: String,
    /// Every release between this copy and the newest one, newest first.
    releases: Vec<ReleaseNotes>,
    /// The setup to download, missing when the release has none or this copy is portable.
    setup: Option<SetupAsset>,
    portable: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupAsset {
    url: String,
    size: u64,
    sha256: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    done: u64,
    total: u64,
}

impl Update {
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn releases(&self) -> &[ReleaseNotes] {
        &self.releases
    }
}

impl ReleaseNotes {
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// The notes of one release, for when xTiger was updated by running a setup by hand.
pub fn notes_for(version: &str) -> Result<ReleaseNotes, String> {
    let url = format!("https://api.github.com/repos/9mirx0r/xTiger/releases/tags/v{version}");
    let release: GhRelease = agent()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut response| response.body_mut().read_json())
        .map_err(|e| format!("Cannot reach GitHub: {e}"))?;
    Ok(ReleaseNotes {
        version: version.to_owned(),
        name: release
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("xTiger {version}")),
        notes: release.body.unwrap_or_default(),
        date: release.published_at,
        url: release.html_url,
    })
}

/// The downloaded setup is not needed once the new version runs.
pub fn remove_download() {
    let _ = fs::remove_dir_all(std::env::temp_dir().join("xtiger-update"));
}

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is valid semver")
}

fn tag_version(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

pub fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("portable.txt").is_file()))
        .unwrap_or(false)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

/// Asks GitHub for new releases. It remembers the answer's ETag, so that asking again when
/// nothing changed gets a small `304 Not Modified` instead of the whole list.
#[derive(Default)]
pub struct Checker {
    inner: Mutex<CheckerState>,
}

#[derive(Default)]
struct CheckerState {
    etag: Option<String>,
    found: Option<Update>,
    /// When GitHub last answered, in milliseconds since the Unix epoch.
    checked_at: Option<u64>,
}

impl Checker {
    /// The newest release that is newer than this copy, or `None` when this copy is up to date.
    /// Pre-releases count, because xTiger is in alpha.
    pub fn check(&self) -> Result<Option<Update>, String> {
        let etag = self.inner.lock().unwrap().etag.clone();
        let mut request = agent().get(RELEASES_URL).header("Accept", "application/vnd.github+json");
        if let Some(etag) = &etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request.call().map_err(|e| format!("Cannot reach GitHub: {e}"))?;
        let mut inner = self.inner.lock().unwrap();
        inner.checked_at = Some(now_ms());
        if response.status() == 304 {
            return Ok(inner.found.clone());
        }
        let new_etag =
            response.headers().get("ETag").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let releases: Vec<GhRelease> = response
            .body_mut()
            .read_json()
            .map_err(|e| format!("GitHub sent an unexpected answer: {e}"))?;
        inner.etag = new_etag;
        inner.found = newer_than(releases, &current_version(), is_portable());
        Ok(inner.found.clone())
    }

    /// What the last check found, without asking again.
    pub fn known(&self) -> (Option<Update>, Option<u64>) {
        let inner = self.inner.lock().unwrap();
        (inner.found.clone(), inner.checked_at)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn newer_than(releases: Vec<GhRelease>, current: &Version, portable: bool) -> Option<Update> {
    let mut newer: Vec<(Version, GhRelease)> = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| tag_version(&release.tag_name).map(|version| (version, release)))
        .filter(|(version, _)| version > current)
        .collect();
    newer.sort_by(|a, b| b.0.cmp(&a.0));
    let (newest, latest) = newer.first()?;

    let setup = latest
        .assets
        .iter()
        .find(|asset| asset.name.starts_with("xTiger_") && asset.name.ends_with("-setup.exe"))
        .filter(|asset| asset.size <= MAX_SETUP_SIZE)
        .map(|asset| SetupAsset {
            url: asset.browser_download_url.clone(),
            size: asset.size,
            sha256: asset
                .digest
                .as_deref()
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(str::to_lowercase),
        });

    Some(Update {
        version: newest.to_string(),
        url: latest.html_url.clone(),
        setup: if portable { None } else { setup },
        portable,
        releases: newer
            .iter()
            .map(|(version, release)| ReleaseNotes {
                version: version.to_string(),
                name: release
                    .name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| format!("xTiger {version}")),
                notes: release.body.clone().unwrap_or_default(),
                date: release.published_at.clone(),
                url: release.html_url.clone(),
            })
            .collect(),
    })
}

/// Download the setup, check it, and start it in update mode. The caller then closes the app.
pub fn download_and_start(
    setup: &SetupAsset,
    on_progress: &Channel<Progress>,
) -> Result<(), String> {
    if !setup.url.starts_with("https://github.com/9mirx0r/xTiger/releases/download/") {
        return Err("That download is not an xTiger release.".to_owned());
    }
    let dir = std::env::temp_dir().join("xtiger-update");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("xTiger-setup.exe");

    let mut response = agent()
        .get(&setup.url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .call()
        .map_err(|e| format!("Cannot download the update: {e}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(&path).map_err(|e| format!("Cannot save the update: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut done = 0u64;
    let mut last_percent = u64::MAX;
    loop {
        let read = reader.read(&mut buffer).map_err(|e| format!("The download stopped: {e}"))?;
        if read == 0 {
            break;
        }
        done += read as u64;
        if done > MAX_SETUP_SIZE {
            return Err("The download is much bigger than expected.".to_owned());
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|e| format!("Cannot save the update: {e}"))?;
        let percent = done * 100 / setup.size.max(1);
        if percent != last_percent {
            last_percent = percent;
            let _ = on_progress.send(Progress { done, total: setup.size });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if done != setup.size {
        return Err(format!(
            "The download is incomplete ({done} of {} bytes). Try again.",
            setup.size
        ));
    }
    if let Some(expected) = &setup.sha256 {
        let actual: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
        if &actual != expected {
            let _ = fs::remove_file(&path);
            return Err(
                "The download is damaged (its checksum does not match). Try again.".to_owned()
            );
        }
    }

    start_setup(path)
}

fn start_setup(path: PathBuf) -> Result<(), String> {
    Command::new(&path)
        .arg("--update")
        .current_dir(std::env::temp_dir())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Cannot start the update: {e}"))
}

/// Open a release page in the browser. Only xTiger's own pages are opened.
pub fn open_page(url: &str) -> Result<(), String> {
    if !url.starts_with(RELEASE_PAGE_PREFIX) {
        return Err("That is not an xTiger release page.".to_owned());
    }
    #[cfg(windows)]
    let result = Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();
    result.map(drop).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool) -> GhRelease {
        GhRelease {
            tag_name: tag.to_owned(),
            name: Some(format!("Release {tag}")),
            body: Some(format!("Notes for {tag}")),
            published_at: None,
            html_url: format!("{RELEASE_PAGE_PREFIX}/tag/{tag}"),
            draft,
            assets: vec![GhAsset {
                name: format!("xTiger_{}_x64-setup.exe", tag.trim_start_matches('v')),
                size: 10,
                browser_download_url: format!("{RELEASE_PAGE_PREFIX}/download/{tag}/setup.exe"),
                digest: Some("sha256:ABCD".to_owned()),
            }],
        }
    }

    #[test]
    fn picks_the_newest_and_lists_everything_since() {
        let current = Version::parse("1.0.0-alpha").unwrap();
        let releases = vec![
            release("v0.9.0", false),
            release("v1.0.0-alpha", false),
            release("v1.0.0-alpha.2", false),
            release("v1.0.0-beta", true),
            release("v1.0.0", false),
            release("not-a-version", false),
        ];
        let update = newer_than(releases, &current, false).unwrap();
        assert_eq!(update.version, "1.0.0");
        let versions: Vec<_> = update.releases.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["1.0.0", "1.0.0-alpha.2"]);
        let setup = update.setup.unwrap();
        assert_eq!(setup.sha256.as_deref(), Some("abcd"));
    }

    /// Asks the real GitHub twice; the second answer is a 304. Run with `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn live_check() {
        let checker = Checker::default();
        let first = checker.check().unwrap().map(|u| u.version);
        let second = checker.check().unwrap().map(|u| u.version);
        assert_eq!(first, second);
        assert!(checker.inner.lock().unwrap().etag.is_some());
    }

    #[test]
    fn up_to_date() {
        let current = Version::parse("1.0.0").unwrap();
        assert!(
            newer_than(
                vec![release("v1.0.0", false), release("v1.0.0-alpha", false)],
                &current,
                false
            )
            .is_none()
        );
    }

    #[test]
    fn portable_gets_no_setup() {
        let current = Version::parse("1.0.0-alpha").unwrap();
        let update = newer_than(vec![release("v1.0.0", false)], &current, true).unwrap();
        assert!(update.portable && update.setup.is_none());
    }
}
