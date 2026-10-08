//! Settings that survive between runs, kept as JSON in the app's config folder.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The game folder, when the user picked it or it was found once.
    pub game_dir: Option<PathBuf>,
    /// The `Paradox Interactive/Crusader Kings III` folder in the user's documents.
    pub paradox_dir: Option<PathBuf>,
    /// Mod folders the user added by hand, outside the Paradox `mod` folder.
    pub extra_mods: Vec<PathBuf>,
    /// Open reports in VS Code instead of the default program for the file.
    pub open_in_editor: bool,
    /// "system", "dark" or "light".
    pub theme: String,
    /// The `.mod` file of the mod chosen last.
    pub last_mod: Option<PathBuf>,
    /// How long the last run of each mod took, in milliseconds, keyed by `.mod` file.
    pub durations: HashMap<String, u64>,
    /// Look for a newer xTiger when the app starts.
    pub check_updates: bool,
    /// A version the user chose not to be told about again.
    pub skipped_version: Option<String>,
    /// The version that ran last, to tell when xTiger was updated.
    pub last_version: Option<String>,
    /// The notes of the releases an update installed, shown once when the new version opens.
    pub whats_new: Vec<crate::update::ReleaseNotes>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_dir: None,
            paradox_dir: None,
            extra_mods: Vec::new(),
            open_in_editor: true,
            theme: "system".to_owned(),
            last_mod: None,
            durations: HashMap::new(),
            check_updates: true,
            skipped_version: None,
            last_version: None,
            whats_new: Vec::new(),
        }
    }
}

/// Where the app keeps its settings and past runs. A portable copy has a `portable.txt` next to
/// the exe and keeps everything in a `data` folder beside it.
pub fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if exe_dir.join("portable.txt").is_file() {
        return Some(exe_dir.join("data"));
    }
    app.path().app_data_dir().ok()
}

fn settings_file(app: &AppHandle) -> Option<PathBuf> {
    data_dir(app).map(|dir| dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    settings_file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_file(app).ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}
