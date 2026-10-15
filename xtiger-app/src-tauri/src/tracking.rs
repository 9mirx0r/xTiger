//! Update tracking in the window: the project record of a mod, how its update stands, and the
//! notes on its diagnostics. The records are the ones the assistants read and write through
//! `xtiger_project`, so both sides see the same state.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use xtiger_mcp::locate::Locations;
use xtiger_mcp::projects::{self, Annotate, Create, Project, Status};
use xtiger_mcp::runs;

use crate::{AppState, mcp_state_dir};

/// Who a note is credited to.
const BY: &str = "xTiger app";

/// Where the server's records are, with the folders the app has been told about.
fn locations(app: &AppHandle) -> Result<Locations, String> {
    let mut loc = Locations::detect();
    loc.state_dir = mcp_state_dir(app).ok_or("The app has no folder for its records.")?;
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap();
    if let Some(game) = &settings.game_dir {
        loc.game = Some(game.clone());
    }
    if let Some(paradox) = &settings.paradox_dir {
        loc.user_dir = paradox.clone();
    }
    loc.extra_mods = settings.extra_mods.clone();
    drop(settings);
    Ok(loc)
}

/// The project that works on this mod, as the working copy or as the original it began from.
fn project_of(loc: &Locations, mod_file: &Path) -> Option<Project> {
    let key = runs::mod_key(mod_file);
    projects::list(loc).into_iter().find(|project| {
        runs::mod_key(&project.mod_file) == key
            || project
                .original_mod_file
                .as_deref()
                .is_some_and(|original| runs::mod_key(original) == key)
    })
}

/// How the update of a mod stands, or nothing when no project follows it.
#[tauri::command]
pub async fn project_status(app: AppHandle, mod_file: PathBuf) -> Result<Option<Status>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        project_of(&loc, &mod_file).map(|project| projects::status(&loc, &project)).transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start following the update of a mod. The mod's newest saved run becomes the "before".
#[tauri::command]
pub async fn project_track(app: AppHandle, mod_file: PathBuf) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let project = match project_of(&loc, &mod_file) {
            Some(project) => project,
            None => projects::create(
                &loc,
                &Create {
                    mod_file: &mod_file,
                    name: None,
                    copy_to: None,
                    old_game: None,
                    baseline_run: None,
                },
            )?,
        };
        projects::status(&loc, &project)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct Noted {
    pub annotated: usize,
    pub kind: String,
    pub status: Status,
}

/// Write a note on the diagnostics of the newest run that match a key, a place and a line. They
/// stay in the run and in every count. With `message`, the note goes on that one diagnostic only,
/// even when another one shares its line.
#[tauri::command]
pub async fn project_annotate(
    app: AppHandle,
    mod_file: PathBuf,
    key: String,
    path: String,
    line: usize,
    kind: String,
    note: String,
    message: Option<String>,
) -> Result<Noted, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let project = project_of(&loc, &mod_file)
            .ok_or("This mod has no update to follow yet. Start following it first.")?;
        let what = Annotate {
            key: &key,
            path: &path,
            line,
            message: message.as_deref(),
            kind: &kind,
            note: &note,
            by: BY,
        };
        let (project, done) = projects::annotate(&loc, &project.id, &what)?;
        let status = projects::status(&loc, &project)?;
        Ok(Noted { annotated: done.annotated, kind: done.kind, status })
    })
    .await
    .map_err(|e| e.to_string())?
}
