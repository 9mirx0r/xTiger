//! `ck3-tiger update`: find the newest xTiger release on GitHub, download the validator for this
//! system, check it and put it in place of the running program, showing each step as it goes.
//!
//! GitHub's "latest release" address answers 404 while every release is a pre-release, which is
//! what xTiger's are, so the list of releases is read and the newest version is chosen here.
//! The same module keeps the optional "a new version is out" line at the end of a validation.

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub use imp::*;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub use unsupported::*;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod unsupported {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum UpdateError {
        #[error("Not supported on the platform: {0}")]
        NotSupported(&'static str),
    }

    pub fn update(
        _current_version: &str,
        _target_version: Option<&str>,
        _check_only: bool,
    ) -> Result<(), UpdateError> {
        Err(UpdateError::NotSupported(std::env::consts::OS))
    }

    pub fn set_notices(_on: bool) -> Result<(), UpdateError> {
        Err(UpdateError::NotSupported(std::env::consts::OS))
    }

    pub fn notice(_current_version: &str, _command: &str) {}
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod imp {
    use std::fmt::Write as _;
    use std::fs::{self, File};
    use std::io::{IsTerminal, Read, Write, stderr, stdin};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use semver::Version;
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use thiserror::Error;

    use crate::progress::{Bar, Stage, forced, header, size, style, tick};

    const BIN_NAME: &str = "ck3-tiger";
    const AUTO_NAME: &str = "ck3-tiger-auto";
    const RELEASES_URL: &str = "https://api.github.com/repos/9mirx0r/xTiger/releases?per_page=30";
    const DOWNLOAD_PREFIX: &str = "https://github.com/9mirx0r/xTiger/releases/download/";
    const USER_AGENT: &str = concat!("xTiger-CK3/", env!("CARGO_PKG_VERSION"));
    /// A validator archive bigger than this is not ours.
    const MAX_ARCHIVE_SIZE: u64 = 200 * 1024 * 1024;
    /// How many lines of a release's notes are shown after an update.
    const NOTES_LINES: usize = 8;
    /// The notice looks for news at most this often.
    const CHECK_EVERY_SECONDS: u64 = 24 * 60 * 60;

    #[cfg(target_os = "windows")]
    const PLATFORM: &str = "windows";
    #[cfg(target_os = "windows")]
    const ARCHIVE_EXTENSION: &str = "zip";
    #[cfg(target_os = "windows")]
    const EXE_SUFFIX: &str = ".exe";
    #[cfg(target_os = "linux")]
    const PLATFORM: &str = "linux";
    #[cfg(target_os = "linux")]
    const ARCHIVE_EXTENSION: &str = "tar.gz";
    #[cfg(target_os = "linux")]
    const EXE_SUFFIX: &str = "";

    #[derive(Debug, Error)]
    pub enum UpdateError {
        #[error("'{0}' is not a version like 1.6.5-alpha")]
        VersionTag(String),
        #[error(
            "There is no release {0}. Releases are listed at https://github.com/9mirx0r/xTiger/releases"
        )]
        NoSuchRelease(Version),
        #[error("Cannot reach GitHub: {0}")]
        Network(String),
        #[error("Release {0} has no validator for this system ({1}).")]
        NoAsset(Version, String),
        #[error("{0}")]
        Download(String),
        #[error("{0}")]
        Install(String),
    }

    #[derive(Deserialize)]
    struct GhRelease {
        tag_name: String,
        body: Option<String>,
        html_url: String,
        #[serde(default)]
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

    fn tag_version(tag: &str) -> Option<Version> {
        Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
    }

    /// `1.6.5-alpha` or `v1.6.5-alpha`.
    fn parse_version(text: &str) -> Result<Version, UpdateError> {
        tag_version(text.trim()).ok_or_else(|| UpdateError::VersionTag(text.to_owned()))
    }

    fn archive_name(version: &Version) -> String {
        format!("{BIN_NAME}-{PLATFORM}-v{version}.{ARCHIVE_EXTENSION}")
    }

    /// Where a file of the validator is inside its archive.
    fn path_in_archive(version: &Version, file: &str) -> String {
        format!("{BIN_NAME}-{PLATFORM}-v{version}/{file}{EXE_SUFFIX}")
    }

    /// What to do about the releases GitHub listed.
    #[derive(Debug, PartialEq)]
    enum Choice {
        /// Install this release (index into the list).
        Install(usize, Version),
        /// This copy is already the wanted version, or newer than any release.
        Current(Version),
    }

    /// The release to install. With no `wanted` version it is the newest one, pre-releases
    /// included, and only if it is newer than `current`; with one, exactly that release, which
    /// may also be an older one.
    fn choose(
        releases: &[GhRelease],
        current: &Version,
        wanted: Option<&Version>,
    ) -> Result<Choice, UpdateError> {
        let mut listed: Vec<(usize, Version)> = releases
            .iter()
            .enumerate()
            .filter(|(_, release)| !release.draft)
            .filter_map(|(i, release)| tag_version(&release.tag_name).map(|v| (i, v)))
            .collect();
        listed.sort_by(|a, b| b.1.cmp(&a.1));
        if let Some(wanted) = wanted {
            if wanted == current {
                return Ok(Choice::Current(current.clone()));
            }
            return listed
                .into_iter()
                .find(|(_, version)| version == wanted)
                .map(|(i, version)| Choice::Install(i, version))
                .ok_or_else(|| UpdateError::NoSuchRelease(wanted.clone()));
        }
        match listed.into_iter().next() {
            Some((i, version)) if &version > current => Ok(Choice::Install(i, version)),
            Some((_, version)) => Ok(Choice::Current(version.max(current.clone()))),
            None => Ok(Choice::Current(current.clone())),
        }
    }

    fn agent(seconds: u64) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(seconds)))
            .user_agent(USER_AGENT)
            .build()
            .into()
    }

    fn fetch_releases(seconds: u64) -> Result<Vec<GhRelease>, UpdateError> {
        agent(seconds)
            .get(RELEASES_URL)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| UpdateError::Network(e.to_string()))?
            .body_mut()
            .read_json()
            .map_err(|e| UpdateError::Network(format!("unexpected answer: {e}")))
    }

    /// The part of a release's notes worth showing in a terminal: the introduction before the
    /// first heading, or when there is none the first lines. Markdown marks are left out.
    fn whats_new(body: &str) -> Vec<String> {
        let mut intro: Vec<String> = Vec::new();
        // The release page starts with a paragraph about which file to download, which says
        // nothing to someone who is updating.
        let mut skipping = false;
        for line in body.lines().map(str::trim_end) {
            if line.is_empty() {
                skipping = false;
            } else if line.contains("Which file?") {
                skipping = true;
            }
            if skipping {
                continue;
            }
            if line.starts_with('#') {
                if intro.iter().any(|l| !l.is_empty()) {
                    break;
                }
                continue;
            }
            if line.is_empty() && intro.last().is_none_or(String::is_empty) {
                continue;
            }
            intro.push(line.replace("**", "").replace('`', ""));
        }
        while intro.last().is_some_and(String::is_empty) {
            intro.pop();
        }
        if intro.len() > NOTES_LINES {
            intro.truncate(NOTES_LINES);
            intro.push("...".to_owned());
        }
        intro
    }

    /// Install `target_version`, or the newest release when there is none. With `check_only`
    /// only say whether there is something newer.
    pub fn update(
        current_version: &str,
        target_version: Option<&str>,
        check_only: bool,
    ) -> Result<(), UpdateError> {
        let current = parse_version(current_version)?;
        let wanted = target_version.map(parse_version).transpose()?;

        header(&[("Installed", current.to_string())]);
        let stage = Stage::start("Looking for the newest version");
        let releases = fetch_releases(20)?;
        let choice = choose(&releases, &current, wanted.as_ref())?;
        match &choice {
            Choice::Install(_, version) if wanted.is_some() => {
                stage.finish(&format!("found {version}"));
            }
            Choice::Install(_, version) => stage.finish(&format!("{version} is out")),
            Choice::Current(_) => stage.finish("nothing newer"),
        }

        let (index, version) = match choice {
            Choice::Current(version) => {
                eprintln!();
                if wanted.is_some() {
                    eprintln!("Version {version} is already the one installed.");
                } else {
                    eprintln!(
                        "{} xTiger-CK3 {current} is up to date.",
                        style(tick()).green().bold()
                    );
                }
                return Ok(());
            }
            Choice::Install(index, version) => (index, version),
        };
        let release = &releases[index];

        if check_only {
            eprintln!();
            eprintln!(
                "A new version is available: {} (you have {current}).",
                style(&version).bold()
            );
            eprintln!("Run `{BIN_NAME} update` to install it. Notes: {}", release.html_url);
            return Ok(());
        }

        let name = archive_name(&version);
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| UpdateError::NoAsset(version.clone(), name.clone()))?;
        if !asset.browser_download_url.starts_with(DOWNLOAD_PREFIX) {
            return Err(UpdateError::Download(
                "That download is not an xTiger release.".to_owned(),
            ));
        }
        if asset.size > MAX_ARCHIVE_SIZE {
            return Err(UpdateError::Download("The download is bigger than expected.".to_owned()));
        }

        let work = WorkDir::new()?;
        let archive = work.path.join(&name);
        download(asset, &archive)?;
        verify(asset, &archive)?;
        let unpacked = unpack(&version, &archive, &work.path)?;
        replace(&unpacked)?;

        eprintln!();
        let word = if version < current { "Installed" } else { "Updated to" };
        eprintln!(
            "{} {word} xTiger-CK3 {}   {}",
            style(tick()).green().bold(),
            style(&version).bold(),
            style(format!("(was {current})")).dim()
        );
        let notes = whats_new(release.body.as_deref().unwrap_or_default());
        if !notes.is_empty() {
            eprintln!();
            eprintln!("{}", style("What's new").bold());
            for line in notes {
                eprintln!("  {}", style(line).dim());
            }
        }
        eprintln!();
        eprintln!("  {}  {}", style("Full notes").dim(), release.html_url);
        Ok(())
    }

    /// A temporary folder for the download that goes away with the update.
    struct WorkDir {
        path: PathBuf,
    }

    impl WorkDir {
        fn new() -> Result<WorkDir, UpdateError> {
            let path = std::env::temp_dir().join(format!("xtiger-update-{}", std::process::id()));
            _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).map_err(|e| {
                UpdateError::Install(format!("Cannot make a temporary folder: {e}"))
            })?;
            Ok(WorkDir { path })
        }
    }

    impl Drop for WorkDir {
        fn drop(&mut self) {
            _ = fs::remove_dir_all(&self.path);
        }
    }

    fn download(asset: &GhAsset, to: &Path) -> Result<(), UpdateError> {
        let label = format!("Downloading {}", asset.name);
        let mut bar = Bar::start(&label, asset.size);
        let fail = UpdateError::Download;

        let mut response = agent(600)
            .get(&asset.browser_download_url)
            .call()
            .map_err(|e| fail(format!("Cannot download the update: {e}")))?;
        let mut reader = response.body_mut().as_reader();
        let mut file =
            File::create(to).map_err(|e| fail(format!("Cannot save the update: {e}")))?;
        let mut buffer = vec![0; 64 * 1024];
        let mut done = 0u64;
        loop {
            let read =
                reader.read(&mut buffer).map_err(|e| fail(format!("The download stopped: {e}")))?;
            if read == 0 {
                break;
            }
            done += read as u64;
            if done > MAX_ARCHIVE_SIZE {
                return Err(fail("The download is much bigger than expected.".to_owned()));
            }
            file.write_all(&buffer[..read])
                .map_err(|e| fail(format!("Cannot save the update: {e}")))?;
            bar.set(done);
        }
        file.flush().map_err(|e| fail(e.to_string()))?;
        if done != asset.size {
            return Err(fail(format!(
                "The download is incomplete ({done} of {} bytes). Try again.",
                asset.size
            )));
        }
        bar.finish(&size(done));
        Ok(())
    }

    fn verify(asset: &GhAsset, file: &Path) -> Result<(), UpdateError> {
        let stage = Stage::start("Checking the download");
        let Some(expected) = asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            return Err(UpdateError::Download(
                "This release has no checksum to check the download against, so it was not \
                 installed. Download it by hand from the release page."
                    .to_owned(),
            ));
        };
        let read_error =
            |e: std::io::Error| UpdateError::Download(format!("Cannot read the download: {e}"));
        let mut reader = File::open(file).map_err(read_error)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer).map_err(read_error)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let actual = hasher.finalize().iter().fold(String::new(), |mut hex, byte| {
            _ = write!(hex, "{byte:02x}");
            hex
        });
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(UpdateError::Download(
                "The download is damaged (its checksum does not match). Nothing was changed. Try again."
                    .to_owned(),
            ));
        }
        stage.finish("sha256 matches");
        Ok(())
    }

    /// The new programs, taken out of the archive into the work folder.
    struct Unpacked {
        validator: PathBuf,
        /// The double-click launcher, when the archive has one.
        auto: Option<PathBuf>,
    }

    fn unpack(version: &Version, archive: &Path, work: &Path) -> Result<Unpacked, UpdateError> {
        let stage = Stage::start("Unpacking");
        let out = work.join("new");
        fs::create_dir_all(&out)
            .map_err(|e| UpdateError::Install(format!("Cannot make a temporary folder: {e}")))?;
        // The file lands in `out` under the path it had in the archive.
        let extract = |file: &str| {
            let inside = path_in_archive(version, file);
            self_update::Extract::from_source(archive)
                .extract_file(&out, &inside)
                .map(|()| out.join(inside))
        };
        let validator = extract(BIN_NAME).map_err(|e| {
            UpdateError::Install(format!("The archive does not hold the validator: {e}"))
        })?;
        let auto = extract(AUTO_NAME).ok().filter(|path| path.is_file());
        stage.finish("");
        Ok(Unpacked { validator, auto })
    }

    fn replace(new: &Unpacked) -> Result<(), UpdateError> {
        let stage = Stage::start("Replacing the program");
        let exe = std::env::current_exe().map_err(|e| {
            UpdateError::Install(format!("Cannot tell where xTiger runs from: {e}"))
        })?;
        self_replace::self_replace(&new.validator).map_err(|e| {
            UpdateError::Install(format!(
                "Cannot replace {}: {e}\nIf it is in a protected folder, run the update from a terminal with the rights to write there.",
                exe.display()
            ))
        })?;
        // The launcher beside it is not running, so it can simply be overwritten.
        if let (Some(auto), Some(dir)) = (&new.auto, exe.parent()) {
            let target = dir.join(format!("{AUTO_NAME}{EXE_SUFFIX}"));
            if target.is_file() {
                _ = fs::copy(auto, target);
            }
        }
        stage.finish("");
        Ok(())
    }

    // ---- The notice at the end of a validation -------------------------------------------

    /// What the validator remembers between runs about new versions.
    #[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
    struct Notice {
        /// `None` until the user has been asked.
        enabled: Option<bool>,
        /// When GitHub was last asked, in seconds since the Unix epoch.
        checked_at: Option<u64>,
        /// The newest version GitHub listed then.
        latest: Option<String>,
    }

    fn now() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    fn due(checked_at: Option<u64>, now: u64) -> bool {
        checked_at.is_none_or(|at| now.saturating_sub(at) >= CHECK_EVERY_SECONDS || at > now)
    }

    /// `XTIGER_CONFIG_DIR` first (tests and demos), then the user's config folder.
    fn state_file() -> Option<PathBuf> {
        let dir = std::env::var_os("XTIGER_CONFIG_DIR").map(PathBuf::from).or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("xTiger"))
            }
            #[cfg(target_os = "linux")]
            {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(|dir| PathBuf::from(dir).join("xtiger"))
                    .or_else(|| home::home_dir().map(|home| home.join(".config").join("xtiger")))
            }
        })?;
        Some(dir.join("cli.json"))
    }

    fn load_notice() -> Notice {
        state_file()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_notice(notice: &Notice) -> Result<(), UpdateError> {
        let path = state_file().ok_or_else(|| {
            UpdateError::Install("Cannot tell where to keep the setting.".to_owned())
        })?;
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&path, serde_json::to_string_pretty(notice)?)
        };
        write().map_err(|e| UpdateError::Install(format!("Cannot save {}: {e}", path.display())))
    }

    /// `ck3-tiger update --notices on|off`.
    pub fn set_notices(on: bool) -> Result<(), UpdateError> {
        let mut notice = load_notice();
        notice.enabled = Some(on);
        save_notice(&notice)?;
        if on {
            eprintln!(
                "Update notices are on: after a check at a terminal, xTiger-CK3 asks GitHub once a day if there is a new version."
            );
        } else {
            eprintln!(
                "Update notices are off: xTiger-CK3 only goes online when you run `{BIN_NAME} update`."
            );
        }
        Ok(())
    }

    /// The line "a new version is available" at the end of a validation at a terminal. The first
    /// time it asks whether to do this at all; nothing goes online before the answer is yes.
    /// It never runs in CI, when `XTIGER_NO_UPDATE_CHECK` is set, or when it cannot ask.
    pub fn notice(current_version: &str, command: &str) {
        if std::env::var_os("XTIGER_NO_UPDATE_CHECK").is_some()
            || std::env::var_os("CI").is_some()
            || (!stderr().is_terminal() && !forced())
        {
            return;
        }
        let Ok(current) = Version::parse(current_version) else { return };
        let mut state = load_notice();
        match state.enabled {
            Some(false) => return,
            Some(true) => {}
            None => {
                if !stdin().is_terminal() {
                    return;
                }
                state.enabled = Some(ask());
                _ = save_notice(&state);
                if state.enabled != Some(true) {
                    return;
                }
            }
        }
        let now = now();
        if due(state.checked_at, now) {
            // A short wait: a slow network must not hold up the end of a validation.
            if let Ok(releases) = fetch_releases(3) {
                state.latest = releases
                    .iter()
                    .filter(|release| !release.draft)
                    .filter_map(|release| tag_version(&release.tag_name))
                    .max()
                    .map(|version| version.to_string());
            }
            state.checked_at = Some(now);
            _ = save_notice(&state);
        }
        if let Some(latest) = state.latest.as_deref().and_then(|v| Version::parse(v).ok())
            && latest > current
        {
            eprintln!(
                "  {} {} (you have {current}). Run {} to get it.",
                style("A new xTiger version is available:").yellow(),
                style(&latest).bold(),
                style(format!("{command} update")).bold()
            );
        }
    }

    fn ask() -> bool {
        eprintln!();
        eprintln!("  Tell you here when a new xTiger version is out?");
        eprintln!(
            "  {}",
            style("It would ask GitHub once a day and send nothing else. Say no and the validator")
                .dim()
        );
        eprintln!(
            "  {}",
            style(format!(
                "only goes online when you run `{BIN_NAME} update`. Change it with `{BIN_NAME} update --notices on|off`."
            ))
            .dim()
        );
        eprint!("  [y/N] ");
        _ = stderr().flush();
        let mut answer = String::new();
        if stdin().read_line(&mut answer).is_err() {
            return false;
        }
        matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn release(tag: &str, draft: bool) -> GhRelease {
            GhRelease {
                tag_name: tag.to_owned(),
                body: None,
                html_url: format!("https://github.com/9mirx0r/xTiger/releases/tag/{tag}"),
                draft,
                assets: Vec::new(),
            }
        }

        fn v(text: &str) -> Version {
            Version::parse(text).unwrap()
        }

        #[test]
        fn pre_releases_are_found_and_the_newest_wins() {
            let releases = vec![release("v1.6.4-alpha", false), release("v1.6.5-alpha", false)];
            let choice = choose(&releases, &v("1.6.3-alpha"), None).unwrap();
            assert_eq!(choice, Choice::Install(1, v("1.6.5-alpha")));
        }

        #[test]
        fn nothing_is_installed_when_up_to_date() {
            let releases = vec![release("v1.6.5-alpha", false)];
            let choice = choose(&releases, &v("1.6.5-alpha"), None).unwrap();
            assert_eq!(choice, Choice::Current(v("1.6.5-alpha")));
            // A build newer than every release is left alone too.
            let choice = choose(&releases, &v("1.7.0-alpha"), None).unwrap();
            assert_eq!(choice, Choice::Current(v("1.7.0-alpha")));
        }

        #[test]
        fn drafts_are_ignored() {
            let releases = vec![release("v1.6.9-alpha", true), release("v1.6.5-alpha", false)];
            let choice = choose(&releases, &v("1.6.4-alpha"), None).unwrap();
            assert_eq!(choice, Choice::Install(1, v("1.6.5-alpha")));
        }

        #[test]
        fn a_named_version_may_be_older() {
            let releases = vec![release("v1.6.4-alpha", false), release("v1.6.5-alpha", false)];
            let wanted = v("1.6.4-alpha");
            let choice = choose(&releases, &v("1.6.5-alpha"), Some(&wanted)).unwrap();
            assert_eq!(choice, Choice::Install(0, wanted.clone()));
            assert_eq!(choose(&releases, &wanted, Some(&wanted)).unwrap(), Choice::Current(wanted));
            assert!(matches!(
                choose(&releases, &v("1.6.5-alpha"), Some(&v("9.9.9"))),
                Err(UpdateError::NoSuchRelease(_))
            ));
        }

        #[test]
        fn versions_are_read_with_or_without_the_v() {
            assert_eq!(parse_version("v1.6.5-alpha").unwrap(), v("1.6.5-alpha"));
            assert_eq!(parse_version("1.6.5-alpha").unwrap(), v("1.6.5-alpha"));
            assert!(parse_version("latest").is_err());
        }

        #[test]
        fn a_download_is_installed_only_when_its_checksum_is_published_and_matches() {
            let dir = std::env::temp_dir().join(format!("xtiger-verify-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let file = dir.join("asset.zip");
            fs::write(&file, b"abc").unwrap();
            // The SHA-256 of "abc".
            let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
            let asset = |digest: Option<String>| GhAsset {
                name: "asset.zip".to_owned(),
                size: 3,
                browser_download_url: String::new(),
                digest,
            };
            let digest = |hex: &str| Some(format!("sha256:{hex}"));
            assert!(verify(&asset(digest(good)), &file).is_ok());
            assert!(verify(&asset(digest(&good.to_uppercase())), &file).is_ok());
            let wrong = "0".repeat(64);
            for bad in
                [digest(&wrong), digest(&good[..40]), digest("zz"), Some("md5:abc".into()), None]
            {
                let result = verify(&asset(bad.clone()), &file);
                assert!(result.is_err(), "{bad:?} must not pass");
            }
            let message = verify(&asset(None), &file).unwrap_err().to_string();
            assert!(message.contains("no checksum"), "{message}");
            let message = verify(&asset(digest(&wrong)), &file).unwrap_err().to_string();
            assert!(message.contains("does not match"), "{message}");
            fs::remove_dir_all(&dir).unwrap();
        }

        /// The archive reader is a dependency that has changed hands before: this checks it still
        /// takes the validator out of a release archive, and only the files asked for.
        #[cfg(target_os = "windows")]
        #[test]
        fn the_validator_comes_out_of_a_release_archive() {
            use zip::write::SimpleFileOptions;
            let dir = std::env::temp_dir().join(format!("xtiger-unpack-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            let version = v("1.2.3-alpha");
            let make = |name: &str, files: &[(&str, &[u8])]| {
                let path = dir.join(name);
                let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
                for (file, bytes) in files {
                    writer.start_file(*file, SimpleFileOptions::default()).unwrap();
                    writer.write_all(bytes).unwrap();
                }
                writer.finish().unwrap();
                path
            };
            let inside = |file: &str| path_in_archive(&version, file);
            let both = make(
                "both.zip",
                &[
                    (&inside(BIN_NAME), b"validator"),
                    (&inside(AUTO_NAME), b"launcher"),
                    ("ck3-tiger.conf", b"not wanted"),
                ],
            );
            let unpacked = unpack(&version, &both, &dir.join("work-both")).unwrap();
            assert_eq!(fs::read(&unpacked.validator).unwrap(), b"validator");
            assert_eq!(fs::read(unpacked.auto.unwrap()).unwrap(), b"launcher");
            assert!(!dir.join("work-both").join("new").join("ck3-tiger.conf").exists());

            let only = make("only.zip", &[(&inside(BIN_NAME), b"validator")]);
            assert!(unpack(&version, &only, &dir.join("work-only")).unwrap().auto.is_none());

            let neither = make("neither.zip", &[("readme.txt", b"hi")]);
            assert!(unpack(&version, &neither, &dir.join("work-neither")).is_err());
            fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn archive_names_follow_the_release_files() {
            let version = v("1.6.5-alpha");
            let name = archive_name(&version);
            assert!(name.starts_with("ck3-tiger-") && name.contains("-v1.6.5-alpha."));
            let inside = path_in_archive(&version, BIN_NAME);
            assert!(inside.starts_with(&format!("ck3-tiger-{PLATFORM}-v1.6.5-alpha/ck3-tiger")));
        }

        #[test]
        fn the_notes_show_the_introduction() {
            let body = "First **line** with `code`.\nSecond line.\n\n### Details\n\n- a\n- b\n";
            assert_eq!(whats_new(body), ["First line with code.", "Second line."]);
        }

        #[test]
        fn the_which_file_paragraph_is_left_out() {
            let body = "**Which file?** Pick one of the first two.
- **Desktop app:** `x`.
- **Command line:** `y`.

The real news.

## Details
";
            assert_eq!(whats_new(body), ["The real news."]);
        }

        #[test]
        fn notes_without_an_introduction_start_at_the_first_heading() {
            let body = "## Fixes\n- one\n- two\n";
            assert_eq!(whats_new(body), ["- one", "- two"]);
        }

        #[test]
        fn long_notes_are_cut() {
            let body = (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
            let notes = whats_new(&body);
            assert_eq!(notes.len(), NOTES_LINES + 1);
            assert_eq!(notes.last().unwrap(), "...");
        }

        #[test]
        fn the_notice_looks_once_a_day() {
            assert!(due(None, 1_000));
            assert!(!due(Some(1_000), 1_000 + 60));
            assert!(due(Some(1_000), 1_000 + CHECK_EVERY_SECONDS));
            // A clock that went back must not stop the check for ever.
            assert!(due(Some(5_000), 1_000));
        }

        #[test]
        fn the_setting_survives_a_round_trip() {
            let notice = Notice {
                enabled: Some(true),
                checked_at: Some(7),
                latest: Some("1.6.6-alpha".to_owned()),
            };
            let text = serde_json::to_string(&notice).unwrap();
            assert_eq!(serde_json::from_str::<Notice>(&text).unwrap(), notice);
            assert_eq!(serde_json::from_str::<Notice>("{}").unwrap(), Notice::default());
        }
    }
}
