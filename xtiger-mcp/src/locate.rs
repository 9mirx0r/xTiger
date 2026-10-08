//! Finding the game, the user's CK3 folder, the validator and the xTiger app's settings.
//!
//! The app and the MCP server share this code, so they always agree on what they find. In the
//! server, environment variables win; without them it uses what the xTiger app found, then
//! Steam, then the usual documents folder, so it works on most computers without any setup.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use steamlocate::SteamDir;

/// The xTiger app's identifier, which names the folder with its settings.
pub const APP_ID: &str = "dev.xtiger.desktop";
/// The Steam app id of Crusader Kings III.
pub const STEAM_APP_ID: u32 = 1_158_310;
/// A file that only a CK3 game folder has, relative to that folder.
pub const SIGNATURE_FILE: &str = "game/events/witch_events.txt";

pub const VALIDATOR: &str = if cfg!(windows) { "ck3-tiger.exe" } else { "ck3-tiger" };

#[derive(Debug, Clone, Serialize)]
pub struct GameInfo {
    pub path: PathBuf,
    pub version: Option<String>,
}

pub fn is_game_dir(dir: &Path) -> bool {
    dir.join(SIGNATURE_FILE).is_file()
}

/// Accept the game folder or one of its direct subfolders, such as `game`.
pub fn normalize_game_dir(dir: &Path) -> Option<PathBuf> {
    if is_game_dir(dir) {
        return Some(dir.to_path_buf());
    }
    dir.parent().filter(|parent| is_game_dir(parent)).map(Path::to_path_buf)
}

pub fn game_info(dir: &Path) -> GameInfo {
    GameInfo { path: dir.to_path_buf(), version: game_version(dir) }
}

pub fn game_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("launcher/launcher-settings.json")).ok()?;
    let json: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    json.get("rawVersion")?.as_str().map(str::to_owned)
}

pub fn steam_game_dir() -> Option<PathBuf> {
    let steam = SteamDir::locate().ok()?;
    let (app, library) = steam.find_app(STEAM_APP_ID).ok()??;
    let dir = library.resolve_app_dir(&app);
    is_game_dir(&dir).then_some(dir)
}

/// The places the `Paradox Interactive/Crusader Kings III` folder can be, most likely first.
/// Documents can be moved, for example into a cloud drive, so Windows is asked where it is.
pub fn user_dir_candidates() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = dirs::document_dir().into_iter().collect();
    if let Some(home) = dirs::home_dir() {
        bases.push(home.join("Documents"));
        bases.push(home.join(".local/share"));
        bases.push(home.join("Library/Application Support"));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for base in bases {
        let dir: PathBuf =
            base.join("Paradox Interactive").join("Crusader Kings III").components().collect();
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// The `Paradox Interactive/Crusader Kings III` folder, if there is one.
pub fn paradox_dir() -> Option<PathBuf> {
    user_dir_candidates().into_iter().find(|dir| dir.is_dir())
}

/// The server's own folder inside the app's data folder: saved runs and the activity journal.
pub const STATE_FOLDER: &str = "mcp";

/// Where the xTiger app keeps its settings: a `data` folder next to a portable copy, else the
/// user's app data folder.
pub fn app_data_dir(exe_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = exe_dir
        && dir.join("portable.txt").is_file()
    {
        return Some(dir.join("data"));
    }
    dirs::data_dir().map(|dir| dir.join(APP_ID))
}

/// The part of the xTiger app's settings that the server reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub game_dir: Option<PathBuf>,
    pub paradox_dir: Option<PathBuf>,
    pub extra_mods: Vec<PathBuf>,
}

impl AppSettings {
    pub fn load(data_dir: &Path) -> Self {
        fs::read_to_string(data_dir.join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

/// Finds a program on the PATH.
pub type FindOnPath = dyn Fn(&str) -> Option<PathBuf>;

/// Everything the search looks at. [`Sources::system`] reads the real computer; tests build
/// their own.
pub struct Sources {
    /// The environment variables that matter, read once.
    pub env: HashMap<String, String>,
    /// The folder of the running program.
    pub exe_dir: Option<PathBuf>,
    /// The xTiger app's data folder.
    pub app_data: Option<PathBuf>,
    pub app_settings: AppSettings,
    /// Where the setup installs xTiger.
    pub installed_app: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub user_dir_candidates: Vec<PathBuf>,
    pub steam: Box<dyn Fn() -> Option<PathBuf>>,
    pub on_path: Box<FindOnPath>,
}

const VARIABLES: [&str; 4] = ["XTIGER_BIN", "CK3_GAME_DIR", "CK3_USER_DIR", "XTIGER_STATE_DIR"];

impl Sources {
    pub fn system() -> Self {
        let exe_dir = env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        let app_data = app_data_dir(exe_dir.as_deref());
        let app_settings = app_data.as_deref().map(AppSettings::load).unwrap_or_default();
        Self {
            env: VARIABLES
                .iter()
                .filter_map(|name| env::var(name).ok().map(|value| ((*name).to_owned(), value)))
                .collect(),
            exe_dir,
            app_data,
            app_settings,
            installed_app: dirs::data_local_dir().map(|dir| dir.join("Programs").join("xTiger")),
            home: dirs::home_dir(),
            user_dir_candidates: user_dir_candidates(),
            steam: Box::new(steam_game_dir),
            on_path: Box::new(find_on_path),
        }
    }

    fn var(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(String::as_str).filter(|value| !value.trim().is_empty())
    }
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources")
            .field("env", &self.env)
            .field("exe_dir", &self.exe_dir)
            .finish_non_exhaustive()
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).map(|dir| dir.join(name)).find(|candidate| candidate.is_file())
}

/// What the search found, and where each answer came from.
#[derive(Debug, Clone)]
pub struct Locations {
    pub validator: Option<PathBuf>,
    pub validator_from: &'static str,
    pub game: Option<PathBuf>,
    pub game_from: &'static str,
    /// The CK3 user folder, with `mod/` and `logs/`.
    pub user_dir: PathBuf,
    pub user_from: &'static str,
    /// Where the server keeps its saved runs.
    pub state_dir: PathBuf,
    pub state_from: &'static str,
    /// Mod folders added by hand in the xTiger app.
    pub extra_mods: Vec<PathBuf>,
}

impl Locations {
    pub fn detect() -> Self {
        Self::from_sources(&Sources::system())
    }

    pub fn from_sources(sources: &Sources) -> Self {
        let (validator, validator_from) = find_validator(sources);
        let (game, game_from) = find_game(sources);
        let (user_dir, user_from) = find_user_dir(sources);
        let (state_dir, state_from) = if let Some(dir) = sources.var("XTIGER_STATE_DIR") {
            (PathBuf::from(dir), "XTIGER_STATE_DIR")
        } else if let Some(dir) = &sources.app_data {
            (dir.join(STATE_FOLDER), "xTiger app data")
        } else {
            (sources.home.clone().unwrap_or_default().join(".xtiger"), "home folder")
        };
        Self {
            validator,
            validator_from,
            game,
            game_from,
            user_dir,
            user_from,
            state_dir,
            state_from,
            extra_mods: sources.app_settings.extra_mods.clone(),
        }
    }

    pub fn game_ok(&self) -> bool {
        self.game.as_deref().is_some_and(is_game_dir)
    }

    pub fn require_game(&self) -> Result<&Path, String> {
        match &self.game {
            None => Err("CK3 not found. Pick the game folder once in the xTiger app, or set CK3_GAME_DIR to \
                         the CK3 install folder (the one that contains game/ and binaries/)."
                .to_owned()),
            Some(dir) if !is_game_dir(dir) => Err(format!(
                "CK3_GAME_DIR points to {}, which is not a CK3 install (no game/ folder). Fix the variable \
                 or remove it to search automatically.",
                dir.display()
            )),
            Some(dir) => Ok(dir),
        }
    }

    pub fn require_validator(&self) -> Result<&Path, String> {
        match &self.validator {
            None => Err("ck3-tiger not found. Install the xTiger app, build it with `cargo build --release -p \
                         ck3-tiger`, or set XTIGER_BIN."
                .to_owned()),
            Some(path) if !path.is_file() => {
                Err(format!("ck3-tiger not found at {} (from {}).", path.display(), self.validator_from))
            }
            Some(path) => Ok(path),
        }
    }

    /// What was found and where it came from, for the `xtiger_status` tool.
    pub fn describe(&self) -> Value {
        json!({
            "validator": {
                "path": self.validator,
                "from": self.validator_from,
                "ok": self.validator.as_deref().is_some_and(Path::is_file),
            },
            "game": {
                "path": self.game,
                "from": self.game_from,
                "ok": self.game_ok(),
                "version": self.game.as_deref().filter(|dir| is_game_dir(dir)).and_then(game_version),
            },
            "user_dir": {
                "path": self.user_dir,
                "from": self.user_from,
                "ok": self.user_dir.is_dir(),
            },
            "state_dir": self.state_dir,
        })
    }
}

fn find_validator(sources: &Sources) -> (Option<PathBuf>, &'static str) {
    if let Some(explicit) = sources.var("XTIGER_BIN") {
        // A wrong variable is reported as such instead of being silently replaced.
        return (Some(PathBuf::from(explicit)), "XTIGER_BIN");
    }
    if let Some(dir) = &sources.exe_dir {
        // The installed app, a portable copy, a release archive and a checkout's target folder all
        // keep the validator next to this program.
        let beside = dir.join(VALIDATOR);
        if beside.is_file() {
            return (Some(beside), "next to xtiger-mcp");
        }
        // A debug build of the server: prefer the much faster release build of the validator.
        if dir.file_name().is_some_and(|name| name == "debug")
            && let Some(release) = dir.parent().map(|target| target.join("release").join(VALIDATOR))
            && release.is_file()
        {
            return (Some(release), "release build");
        }
    }
    if let Some(installed) = sources.installed_app.as_ref().map(|dir| dir.join(VALIDATOR))
        && installed.is_file()
    {
        return (Some(installed), "xTiger app");
    }
    // Last, because a ck3-tiger on the PATH is often upstream Tiger, which does not know CK3 1.20.
    if let Some(found) = (sources.on_path)(VALIDATOR) {
        return (Some(found), "PATH");
    }
    (None, "not found")
}

fn find_game(sources: &Sources) -> (Option<PathBuf>, &'static str) {
    if let Some(explicit) = sources.var("CK3_GAME_DIR") {
        let explicit = PathBuf::from(explicit);
        return match normalize_game_dir(&explicit) {
            Some(found) => (Some(found), "CK3_GAME_DIR"),
            None => (Some(explicit), "CK3_GAME_DIR (not a CK3 install)"),
        };
    }
    if let Some(found) = sources.app_settings.game_dir.as_deref().and_then(normalize_game_dir) {
        return (Some(found), "xTiger app");
    }
    if let Some(found) = (sources.steam)() {
        return (Some(found), "Steam");
    }
    (None, "not found")
}

fn find_user_dir(sources: &Sources) -> (PathBuf, &'static str) {
    if let Some(explicit) = sources.var("CK3_USER_DIR") {
        return (PathBuf::from(explicit), "CK3_USER_DIR");
    }
    if let Some(saved) = sources.app_settings.paradox_dir.as_ref().filter(|dir| dir.is_dir()) {
        return (saved.clone(), "xTiger app");
    }
    if let Some(found) = sources.user_dir_candidates.iter().find(|dir| dir.is_dir()) {
        return (found.clone(), "documents");
    }
    (sources.user_dir_candidates.first().cloned().unwrap_or_default(), "default")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn sources(root: &Path) -> Sources {
        Sources {
            env: HashMap::new(),
            exe_dir: Some(root.join("bin")),
            app_data: Some(root.join("app-data")),
            app_settings: AppSettings::default(),
            installed_app: Some(root.join("installed")),
            home: Some(root.join("home")),
            user_dir_candidates: vec![
                root.join("home/Documents/Paradox Interactive/Crusader Kings III"),
            ],
            steam: Box::new(|| None),
            on_path: Box::new(|_| None),
        }
    }

    fn touch(path: &Path) -> PathBuf {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "never executed").unwrap();
        path.to_path_buf()
    }

    fn make_game(dir: &Path) -> PathBuf {
        touch(&dir.join(SIGNATURE_FILE));
        dir.to_path_buf()
    }

    #[test]
    fn explicit_validator_wins_and_is_not_replaced_when_missing() {
        let tmp = TempDir::new();
        touch(&tmp.join("bin").join(VALIDATOR));
        let mut s = sources(&tmp);
        s.env.insert("XTIGER_BIN".into(), tmp.join("missing").display().to_string());
        let found = Locations::from_sources(&s);
        assert_eq!(found.validator_from, "XTIGER_BIN");
        assert!(found.require_validator().unwrap_err().contains("from XTIGER_BIN"));
    }

    #[test]
    fn validator_next_to_the_server_wins_over_installed_and_path() {
        let tmp = TempDir::new();
        let beside = touch(&tmp.join("bin").join(VALIDATOR));
        touch(&tmp.join("installed").join(VALIDATOR));
        let mut s = sources(&tmp);
        s.on_path = Box::new(|_| Some(PathBuf::from("from-path")));
        let found = Locations::from_sources(&s);
        assert_eq!(
            (found.validator.unwrap(), found.validator_from),
            (beside, "next to xtiger-mcp")
        );
    }

    #[test]
    fn debug_server_uses_the_release_validator() {
        let tmp = TempDir::new();
        let release = touch(&tmp.join("target/release").join(VALIDATOR));
        let mut s = sources(&tmp);
        s.exe_dir = Some(tmp.join("target/debug"));
        let found = Locations::from_sources(&s);
        assert_eq!((found.validator.unwrap(), found.validator_from), (release, "release build"));
    }

    #[test]
    fn installed_app_wins_over_path_and_path_is_last() {
        let tmp = TempDir::new();
        let installed = touch(&tmp.join("installed").join(VALIDATOR));
        let mut s = sources(&tmp);
        s.on_path = Box::new(|_| Some(PathBuf::from("from-path")));
        let found = Locations::from_sources(&s);
        assert_eq!(
            (found.validator.clone().unwrap(), found.validator_from),
            (installed.clone(), "xTiger app")
        );
        fs::remove_file(installed).unwrap();
        let found = Locations::from_sources(&s);
        assert_eq!(
            (found.validator.unwrap(), found.validator_from),
            (PathBuf::from("from-path"), "PATH")
        );
    }

    #[test]
    fn nothing_found() {
        let tmp = TempDir::new();
        let found = Locations::from_sources(&sources(&tmp));
        assert_eq!((found.validator.as_ref(), found.validator_from), (None, "not found"));
        assert!(found.require_validator().unwrap_err().contains("ck3-tiger not found"));
        assert_eq!(found.game_from, "not found");
        assert!(found.require_game().unwrap_err().contains("CK3 not found"));
        assert_eq!(found.user_from, "default");
        assert_eq!(
            (found.state_dir, found.state_from),
            (tmp.join("app-data/mcp"), "xTiger app data")
        );
    }

    #[test]
    fn environment_paths_win() {
        let tmp = TempDir::new();
        let game = make_game(&tmp.join("game-data"));
        let mut s = sources(&tmp);
        s.app_settings.game_dir = Some(make_game(&tmp.join("other-game")));
        s.env.insert("CK3_GAME_DIR".into(), game.join("game").display().to_string());
        s.env.insert("CK3_USER_DIR".into(), tmp.join("user").display().to_string());
        s.env.insert("XTIGER_STATE_DIR".into(), tmp.join("state").display().to_string());
        let found = Locations::from_sources(&s);
        assert_eq!(found.require_game().unwrap(), game);
        assert_eq!((found.user_dir, found.user_from), (tmp.join("user"), "CK3_USER_DIR"));
        assert_eq!((found.state_dir, found.state_from), (tmp.join("state"), "XTIGER_STATE_DIR"));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let tmp = TempDir::new();
        let mut s = sources(&tmp);
        s.env.insert("CK3_GAME_DIR".into(), "  ".into());
        s.env.insert("XTIGER_BIN".into(), String::new());
        let found = Locations::from_sources(&s);
        assert_eq!(found.game_from, "not found");
        assert_eq!(found.validator_from, "not found");
    }

    #[test]
    fn wrong_game_variable_is_reported_not_replaced() {
        let tmp = TempDir::new();
        let steam = make_game(&tmp.join("steam/Crusader Kings III"));
        let mut s = sources(&tmp);
        s.steam = Box::new(move || Some(steam.clone()));
        s.env.insert("CK3_GAME_DIR".into(), tmp.join("deleted").display().to_string());
        let found = Locations::from_sources(&s);
        assert_eq!(found.game_from, "CK3_GAME_DIR (not a CK3 install)");
        assert!(found.require_game().unwrap_err().contains("not a CK3 install"));
    }

    #[test]
    fn app_settings_then_steam() {
        let tmp = TempDir::new();
        let steam = make_game(&tmp.join("steam/Crusader Kings III"));
        let app = make_game(&tmp.join("anywhere/ck3"));
        let mut s = sources(&tmp);
        let steam_copy = steam.clone();
        s.steam = Box::new(move || Some(steam_copy.clone()));
        s.app_settings.game_dir = Some(app.join("game"));
        let found = Locations::from_sources(&s);
        assert_eq!((found.game.clone().unwrap(), found.game_from), (app, "xTiger app"));
        s.app_settings.game_dir = Some(tmp.join("gone"));
        let found = Locations::from_sources(&s);
        assert_eq!((found.game.unwrap(), found.game_from), (steam, "Steam"));
    }

    #[test]
    fn user_dir_from_app_then_documents() {
        let tmp = TempDir::new();
        let documents = tmp.join("home/Documents/Paradox Interactive/Crusader Kings III");
        fs::create_dir_all(&documents).unwrap();
        let picked = tmp.join("picked");
        fs::create_dir_all(&picked).unwrap();
        let mut s = sources(&tmp);
        let found = Locations::from_sources(&s);
        assert_eq!((found.user_dir, found.user_from), (documents, "documents"));
        s.app_settings.paradox_dir = Some(picked.clone());
        let found = Locations::from_sources(&s);
        assert_eq!((found.user_dir, found.user_from), (picked, "xTiger app"));
    }

    #[test]
    fn portable_app_keeps_data_next_to_it() {
        let tmp = TempDir::new();
        touch(&tmp.join("portable.txt"));
        assert_eq!(app_data_dir(Some(&tmp)), Some(tmp.join("data")));
    }

    #[test]
    fn app_settings_are_read_leniently() {
        let tmp = TempDir::new();
        fs::write(
            tmp.join("settings.json"),
            r#"{"game_dir": "C:/Games/CK3", "theme": "dark", "extra_mods": ["a"]}"#,
        )
        .unwrap();
        let settings = AppSettings::load(&tmp);
        assert_eq!(settings.game_dir, Some(PathBuf::from("C:/Games/CK3")));
        assert_eq!(settings.extra_mods, vec![PathBuf::from("a")]);
        fs::write(tmp.join("settings.json"), "not json").unwrap();
        assert!(AppSettings::load(&tmp).game_dir.is_none());
    }
}
