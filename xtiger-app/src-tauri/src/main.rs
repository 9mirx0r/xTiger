//! The xTiger desktop app: a window around the `ck3-tiger` validator.

// Do not open a console window next to the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clients;
mod editor;
mod mods;
mod settings;
mod update;
mod validate;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::mods::ModInfo;
use crate::settings::Settings;
use crate::validate::{RunArgs, RunResult, Runner};
use xtiger_mcp::journal::{self, Entry};
use xtiger_mcp::locate::{self as detect, GameInfo};
use xtiger_mcp::requests::{self, BriefMod, NewRequest, Request};
use xtiger_mcp::runs;
use xtiger_mcp::sessions;

struct AppState {
    settings: Mutex<Settings>,
    runner: Runner,
    updates: update::Checker,
}

impl AppState {
    fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut settings = self.settings.lock().unwrap();
        change(&mut settings);
        settings::save(app, &settings)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Setup {
    game: Option<GameInfo>,
    paradox: Option<PathBuf>,
    has_vscode: bool,
    open_in_editor: bool,
    theme: String,
    last_mod: Option<PathBuf>,
    check_updates: bool,
}

/// What the app knows about the game and the user's folders. Missing folders are searched for,
/// and what is found is remembered.
#[tauri::command]
fn get_setup(app: AppHandle, state: State<AppState>) -> Setup {
    let mut settings = state.settings.lock().unwrap();
    let mut changed = false;
    if settings.game_dir.as_deref().is_none_or(|dir| !detect::is_game_dir(dir)) {
        settings.game_dir = detect::steam_game_dir();
        changed = true;
    }
    if settings.paradox_dir.as_deref().is_none_or(|dir| !dir.is_dir()) {
        settings.paradox_dir = detect::paradox_dir();
        changed = true;
    }
    if changed {
        let _ = settings::save(&app, &settings);
    }
    Setup {
        game: settings.game_dir.as_deref().map(detect::game_info),
        paradox: settings.paradox_dir.clone(),
        has_vscode: editor::has_vscode(),
        open_in_editor: settings.open_in_editor,
        theme: settings.theme.clone(),
        last_mod: settings.last_mod.clone(),
        check_updates: settings.check_updates,
    }
}

#[tauri::command]
fn set_game_dir(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<GameInfo, String> {
    let dir = detect::normalize_game_dir(&path)
        .ok_or("That folder has no game\\events inside. Pick the main game folder.")?;
    state.update(&app, |settings| settings.game_dir = Some(dir.clone()))?;
    Ok(detect::game_info(&dir))
}

#[tauri::command]
fn set_paradox_dir(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<(), String> {
    if !path.is_dir() {
        return Err("That folder does not exist.".to_owned());
    }
    state.update(&app, |settings| settings.paradox_dir = Some(path))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModEntry {
    #[serde(flatten)]
    info: ModInfo,
    /// The number of reports in the last run, if the mod was validated before.
    last_count: Option<usize>,
    /// When the last run ended, in milliseconds since the Unix epoch.
    last_run: Option<u64>,
}

/// The size and time of the newest saved run of each mod, by the app or an assistant.
fn latest_runs(app: &AppHandle) -> HashMap<String, (usize, u64)> {
    mcp_state_dir(app).map(|dir| runs::latest_of_each(&dir.join("runs"))).unwrap_or_default()
}

/// `latest` comes from `latest_runs`.
fn entry(app: &AppHandle, info: ModInfo, latest: &HashMap<String, (usize, u64)>) -> ModEntry {
    let last = latest
        .get(&runs::mod_key(&info.mod_file))
        .copied()
        .or_else(|| validate::legacy_previous_run(app, &info.mod_file));
    ModEntry { info, last_count: last.map(|(count, _)| count), last_run: last.map(|(_, at)| at) }
}

#[tauri::command]
fn list_mods(app: AppHandle, state: State<AppState>) -> Vec<ModEntry> {
    let mods = {
        let settings = state.settings.lock().unwrap();
        mods::list(settings.paradox_dir.as_deref(), &settings.extra_mods)
    };
    let latest = latest_runs(&app);
    mods.into_iter().map(|info| entry(&app, info, &latest)).collect()
}

#[tauri::command]
fn mod_picture(path: PathBuf) -> Option<String> {
    mods::picture_data_url(&path)
}

#[tauri::command]
fn add_mod_folder(
    app: AppHandle,
    state: State<AppState>,
    path: PathBuf,
) -> Result<ModEntry, String> {
    let info = mods::read_mod_folder(&path)
        .ok_or("That folder has no descriptor.mod inside. Pick the mod's main folder.")?;
    state.update(&app, |settings| {
        if !settings.extra_mods.contains(&path) {
            settings.extra_mods.push(path);
        }
    })?;
    Ok(entry(&app, info, &latest_runs(&app)))
}

#[tauri::command]
fn remove_mod_folder(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<(), String> {
    state.update(&app, |settings| settings.extra_mods.retain(|dir| dir != &path))
}

#[tauri::command]
fn set_preferences(
    app: AppHandle,
    state: State<AppState>,
    open_in_editor: Option<bool>,
    theme: Option<String>,
    check_updates: Option<bool>,
) -> Result<(), String> {
    state.update(&app, |settings| {
        if let Some(check_updates) = check_updates {
            settings.check_updates = check_updates;
        }
        if let Some(open_in_editor) = open_in_editor {
            settings.open_in_editor = open_in_editor;
        }
        if let Some(theme) = theme {
            settings.theme = theme;
        }
    })
}

/// How long the last run of this mod took, to estimate the next one.
#[tauri::command]
fn last_duration(state: State<AppState>, mod_file: PathBuf) -> Option<u64> {
    let settings = state.settings.lock().unwrap();
    settings.durations.get(&*mod_file.to_string_lossy()).copied()
}

#[tauri::command]
async fn validate(app: AppHandle, mod_file: PathBuf) -> Result<RunResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (game, paradox, extra) = {
            let settings = state.settings.lock().unwrap();
            (settings.game_dir.clone(), settings.paradox_dir.clone(), settings.extra_mods.clone())
        };
        let game = game.ok_or("The game folder is not set.")?;
        let mod_name = mods::list(paradox.as_deref(), &extra)
            .into_iter()
            .find(|info| info.mod_file == mod_file)
            .map(|info| info.name);
        let runs_dir = mcp_state_dir(&app).map(|dir| dir.join("runs"));
        let args = RunArgs {
            mod_file: &mod_file,
            mod_name,
            game: &game,
            paradox: paradox.as_deref(),
            runs_dir: runs_dir.as_deref(),
        };
        let result = state.runner.run(&app, &args)?;
        state.update(&app, |settings| {
            settings.last_mod = Some(mod_file.clone());
            settings.durations.insert(mod_file.to_string_lossy().into_owned(), result.duration_ms);
        })?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_validation(state: State<AppState>) {
    state.runner.cancel();
}

#[tauri::command]
fn open_location(
    state: State<AppState>,
    path: PathBuf,
    line: Option<u32>,
    column: Option<u32>,
) -> Result<(), String> {
    let prefer_vscode = state.settings.lock().unwrap().open_in_editor;
    editor::open(&path, line, column, prefer_vscode)
}

#[tauri::command]
fn reveal(path: PathBuf) -> Result<(), String> {
    editor::reveal(Path::new(&path))
}

/// Write an export of the reports to the file the user picked.
#[tauri::command]
fn write_text(path: PathBuf, contents: String) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

/// How often the app asks GitHub for a new release while it is open.
const UPDATE_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatus {
    /// A newer release the user has not skipped. Missing when xTiger is up to date.
    update: Option<update::Update>,
    /// When GitHub last answered, in milliseconds since the Unix epoch.
    checked_at: Option<u64>,
}

fn update_status(
    app: &AppHandle,
    found: Option<update::Update>,
    checked_at: Option<u64>,
) -> UpdateStatus {
    let state = app.state::<AppState>();
    let skipped = state.settings.lock().unwrap().skipped_version.clone();
    let update = found.filter(|update| skipped.as_deref() != Some(update.version()));
    UpdateStatus { update, checked_at }
}

/// What the background check found so far.
#[tauri::command]
fn get_update(app: AppHandle, state: State<AppState>) -> UpdateStatus {
    let (found, checked_at) = state.updates.known();
    update_status(&app, found, checked_at)
}

/// Ask GitHub again now, for example after the user turned update checks back on.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<UpdateStatus, String> {
    let handle = app.clone();
    let found =
        tauri::async_runtime::spawn_blocking(move || handle.state::<AppState>().updates.check())
            .await
            .map_err(|e| e.to_string())??;
    let (_, checked_at) = app.state::<AppState>().updates.known();
    Ok(update_status(&app, found, checked_at))
}

/// Look for a new release soon after startup and then every hour, and tell the window when one
/// appears. Nothing is shown while xTiger is up to date.
fn watch_for_updates(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        let mut announced: Option<String> = None;
        loop {
            let state = app.state::<AppState>();
            let enabled = state.settings.lock().unwrap().check_updates;
            if enabled && let Ok(found) = state.updates.check() {
                let (_, checked_at) = state.updates.known();
                let status = update_status(&app, found, checked_at);
                let version = status.update.as_ref().map(|u| u.version().to_owned());
                if version.is_some() && version != announced {
                    let _ = app.emit("update-available", &status);
                    announced = version;
                }
            }
            std::thread::sleep(UPDATE_INTERVAL);
        }
    });
}

#[tauri::command]
fn skip_update(app: AppHandle, state: State<AppState>, version: String) -> Result<(), String> {
    state.update(&app, |settings| settings.skipped_version = Some(version))
}

/// Download the new setup, start it, and close the app so that it can be replaced. The notes
/// of the new releases are kept, to show them once the new version opens.
#[tauri::command]
async fn install_update(
    app: AppHandle,
    setup: update::SetupAsset,
    on_progress: tauri::ipc::Channel<update::Progress>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || update::download_and_start(&setup, &on_progress))
        .await
        .map_err(|e| e.to_string())??;
    let state = app.state::<AppState>();
    let notes = state.updates.known().0.map(|found| found.releases().to_vec()).unwrap_or_default();
    let _ = state.update(&app, |settings| settings.whats_new = notes);
    state.runner.cancel();
    app.exit(0);
    Ok(())
}

/// What is new in this version, once, the first time it runs after an update. Empty on a first
/// install and on every later start.
#[tauri::command]
async fn whats_new(app: AppHandle) -> Vec<update::ReleaseNotes> {
    let current = update::current_version();
    let (last, saved) = {
        let state = app.state::<AppState>();
        let settings = state.settings.lock().unwrap();
        (settings.last_version.clone(), settings.whats_new.clone())
    };
    let state = app.state::<AppState>();
    let _ = state.update(&app, |settings| {
        settings.last_version = Some(current.to_string());
        settings.whats_new.clear();
    });
    let updated = last
        .as_deref()
        .and_then(|last| semver::Version::parse(last).ok())
        .is_some_and(|last| last < current);
    if !updated {
        return Vec::new();
    }
    update::remove_download();
    if saved.first().is_some_and(|notes| notes.version() == current.to_string()) {
        return saved;
    }
    // Updated with a setup run by hand: ask GitHub for this version's notes.
    let version = current.to_string();
    tauri::async_runtime::spawn_blocking(move || update::notes_for(&version))
        .await
        .ok()
        .and_then(Result::ok)
        .into_iter()
        .collect()
}

/// Which AI assistants are on this computer and whether xTiger is connected to each.
#[tauri::command]
fn ai_clients() -> clients::Overview {
    clients::overview()
}

#[tauri::command]
fn connect_ai(id: String) -> Result<clients::ClientStatus, String> {
    clients::connect(&id)
}

#[tauri::command]
fn disconnect_ai(id: String) -> Result<clients::ClientStatus, String> {
    clients::disconnect(&id)
}

/// Where the MCP server keeps its saved runs and its activity journal.
fn mcp_state_dir(app: &AppHandle) -> Option<PathBuf> {
    match std::env::var_os("XTIGER_STATE_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => settings::data_dir(app).map(|dir| dir.join(detect::STATE_FOLDER)),
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
    /// Changes when a call finishes; pass it back to skip reading the journal when nothing did.
    stamp: String,
    running: Vec<Entry>,
    /// The finished calls, newest first, or nothing when `stamp` did not change.
    entries: Option<Vec<Entry>>,
    /// Changes when a request is made, picked up or closed.
    requests_stamp: String,
    /// What the user asked the assistants for, newest first, or nothing when `requests_stamp` did
    /// not change.
    requests: Option<Vec<Request>>,
}

/// How many finished calls the activity screen shows.
const ACTIVITY_SHOWN: usize = 200;

/// What the AI assistants did with xTiger.
#[tauri::command]
fn ai_activity(app: AppHandle, stamp: Option<String>, requests_stamp: Option<String>) -> Activity {
    let Some(dir) = mcp_state_dir(&app) else { return Activity::default() };
    let now = journal::stamp(&dir);
    let entries = (stamp.as_deref() != Some(now.as_str()))
        .then(|| journal::read_entries(&dir, ACTIVITY_SHOWN));
    let asked = requests::stamp(&dir);
    let requests =
        (requests_stamp.as_deref() != Some(asked.as_str())).then(|| requests::list(&dir));
    Activity {
        running: journal::read_running(&dir),
        entries,
        stamp: now,
        requests_stamp: asked,
        requests,
    }
}

/// Leave a request for the AI assistants: fix these reports, or bring a mod up to date.
#[tauri::command]
fn ask_ai(app: AppHandle, request: NewRequest) -> Result<Request, String> {
    let dir = mcp_state_dir(&app).ok_or("cannot find the app's data folder")?;
    requests::add(&dir, request, journal::now_ms())
}

/// Take back a request, or clear a finished one.
#[tauri::command]
fn remove_request(app: AppHandle, id: String) -> Result<(), String> {
    let dir = mcp_state_dir(&app).ok_or("cannot find the app's data folder")?;
    requests::remove(&dir, &id)
}

/// A report on what it takes to bring a mod up to the current game version, to hand to an AI.
#[tauri::command]
fn update_brief(
    info: BriefMod,
    game_version: Option<String>,
    reports: Vec<serde_json::Value>,
) -> String {
    requests::update_brief(&info, game_version.as_deref(), &reports)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiRun {
    #[serde(flatten)]
    result: RunResult,
    mod_file: PathBuf,
    mod_name: Option<String>,
    /// Who ran it, when the run says.
    by: Option<String>,
    finished_at: u64,
}

/// A validation an AI assistant ran, to show on the results screen.
#[tauri::command]
fn ai_run(app: AppHandle, run_id: String) -> Result<AiRun, String> {
    let dir = mcp_state_dir(&app).ok_or("cannot find the app's data folder")?;
    let run = runs::open_run(&dir.join("runs"), &run_id)?;
    Ok(AiRun {
        result: RunResult {
            reports: run.reports,
            duration_ms: (run.meta.seconds * 1000.0).round() as u64,
            previous_count: run.previous_total,
            new_count: run.new,
        },
        mod_file: run.meta.mod_file,
        mod_name: run.meta.mod_name,
        by: run.meta.by,
        finished_at: run.meta.finished_at,
    })
}

/// What came of one work session of an assistant: the passes, what was fixed, what is left and
/// which files changed.
#[tauri::command]
fn ai_session(
    app: AppHandle,
    state: State<AppState>,
    session: String,
) -> Result<sessions::Summary, String> {
    let dir = mcp_state_dir(&app).ok_or("cannot find the app's data folder")?;
    let mods = {
        let settings = state.settings.lock().unwrap();
        mods::list(settings.paradox_dir.as_deref(), &settings.extra_mods)
    };
    let entries = sessions::entries_of(&dir, &session);
    if entries.is_empty() {
        return Err("That session is no longer in the journal.".to_owned());
    }
    Ok(sessions::summarize(&dir, &session, &entries, &mods, journal::now_ms()))
}

#[tauri::command]
fn open_release_page(url: String) -> Result<(), String> {
    update::open_page(&url)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let settings = settings::load(app.handle());
            app.manage(AppState {
                settings: Mutex::new(settings),
                runner: Runner::default(),
                updates: update::Checker::default(),
            });
            watch_for_updates(app.handle().clone());
            std::thread::spawn(clients::remove_old_copies);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_setup,
            set_game_dir,
            set_paradox_dir,
            list_mods,
            mod_picture,
            add_mod_folder,
            remove_mod_folder,
            set_preferences,
            last_duration,
            validate,
            cancel_validation,
            open_location,
            reveal,
            write_text,
            get_update,
            check_update,
            skip_update,
            install_update,
            whats_new,
            open_release_page,
            ai_clients,
            connect_ai,
            disconnect_ai,
            ai_activity,
            ai_run,
            ai_session,
            ask_ai,
            remove_request,
            update_brief,
        ])
        .on_window_event(|window, event| {
            // Do not leave a validation running after the window is gone.
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<AppState>().runner.cancel();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running the xTiger app");
}
