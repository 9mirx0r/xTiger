//! The load order of a mod in the window: the mods that must load before it when it is checked or
//! play-tested. The lists are the ones the assistants read and write through `xtiger_loadorder`,
//! so both sides see the same order.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use xtiger_mcp::loadorder::{self, Dependency, Entry};
use xtiger_mcp::locate::Locations;
use xtiger_mcp::mods::{self, ModInfo};
use xtiger_mcp::own_playsets;
use xtiger_mcp::playsets;
use xtiger_mcp::setup::{Loaded, Unresolved};

use crate::{AppState, mcp_state_dir};

/// Where the server's records are, with the folders the app has been told about.
pub(crate) fn locations(app: &AppHandle) -> Result<Locations, String> {
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

pub(crate) fn all_mods(loc: &Locations) -> Vec<ModInfo> {
    mods::list(Some(&loc.user_dir), &loc.extra_mods)
}

/// One mod of the list the user keeps.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    /// The mod can still be found; when it cannot, a test goes on without it and says so.
    pub installed: bool,
}

/// The `ck3-tiger.conf` the order can be written to.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfState {
    pub path: PathBuf,
    pub exists: bool,
    /// The file has a block written by the app.
    pub has_block: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    pub name: String,
    /// The list the user keeps, in game order.
    pub saved: Vec<Item>,
    /// What the next check or game test loads before the mod, in game order.
    pub will_load: Vec<Loaded>,
    /// The mods that the descriptors of this mod and of the listed ones name as needed.
    pub dependencies: Vec<Dependency>,
    pub unresolved: Vec<Unresolved>,
    pub warnings: Vec<String>,
    pub conf: ConfState,
}

fn conf_state(mod_file: &Path) -> ConfState {
    let dir = mods::describe(&mods::clean(mod_file)).map_or_else(
        || mod_file.parent().map(Path::to_path_buf).unwrap_or_default(),
        |info| info.dir,
    );
    let path = dir.join("ck3-tiger.conf");
    let text = fs::read_to_string(&path).ok();
    let has_block = text.as_deref().is_some_and(|text| loadorder::strip_conf_block(text) != text);
    ConfState { path, exists: text.is_some(), has_block }
}

fn view(loc: &Locations, all: &[ModInfo], mod_file: &Path) -> Result<View, String> {
    let order = loadorder::resolve(&loc.state_dir, &loc.user_dir, all, mod_file, true)?;
    let (_, missing) = loadorder::saved_files(&loc.state_dir, mod_file, all);
    let missing: HashSet<String> = missing.into_iter().map(|item| item.name).collect();
    let saved = loadorder::load(&loc.state_dir, mod_file)
        .map(|saved| saved.requires)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| Item {
            installed: !missing.contains(&entry.name),
            name: entry.name,
            mod_file: entry.mod_file,
        })
        .collect();
    Ok(View {
        mod_file: order.mod_file,
        name: order.name,
        saved,
        will_load: order.loaded,
        dependencies: order.dependencies,
        unresolved: order.unresolved,
        warnings: order.warnings,
        conf: conf_state(mod_file),
    })
}

/// The saved load order of a mod, and what the next test of it loads.
#[tauri::command]
pub async fn loadorder_get(app: AppHandle, mod_file: PathBuf) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        view(&loc, &all_mods(&loc), &mod_file)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Replace the list of a mod. An empty list forgets it.
#[tauri::command]
pub async fn loadorder_save(
    app: AppHandle,
    mod_file: PathBuf,
    requires: Vec<Entry>,
) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        loadorder::save(&loc.state_dir, &mod_file, requires)?;
        view(&loc, &all_mods(&loc), &mod_file)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A playset to start a list from: one of xTiger's or one of the Paradox launcher's.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaysetBrief {
    pub name: String,
    pub source: &'static str,
    pub active: bool,
    pub enabled: usize,
}

#[tauri::command]
pub async fn loadorder_playsets(app: AppHandle) -> Result<Vec<PlaysetBrief>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let all = all_mods(&loc);
        let mut briefs: Vec<PlaysetBrief> = own_playsets::list(&loc.state_dir)
            .iter()
            .map(|own| own_playsets::to_playset(own, &all))
            .map(|set| PlaysetBrief {
                name: set.name,
                source: set.source,
                active: false,
                enabled: set.enabled,
            })
            .collect();
        // The launcher may not be installed; the playsets of xTiger are still there.
        briefs.extend(playsets::list(&loc.user_dir).unwrap_or_default().into_iter().map(|set| {
            PlaysetBrief {
                name: set.name,
                source: set.source,
                active: set.active,
                enabled: set.enabled,
            }
        }));
        Ok(briefs)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start the list from a launcher playset: the enabled mods that come before this one.
#[tauri::command]
pub async fn loadorder_from_playset(
    app: AppHandle,
    mod_file: PathBuf,
    playset: String,
) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let all = all_mods(&loc);
        let found = own_playsets::find(&loc.state_dir, &loc.user_dir, &all, &playset)?;
        let (files, _) = playsets::check_order(&found);
        let main = mods::clean(&mod_file);
        let before: Vec<PathBuf> = files
            .into_iter()
            .map(|file| mods::clean(&file))
            .take_while(|file| *file != main)
            .collect();
        loadorder::save(&loc.state_dir, &mod_file, loadorder::entries(&before, &all))?;
        view(&loc, &all, &mod_file)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write the order into the mod's `ck3-tiger.conf`, for the validator outside xTiger. Where it
/// was written.
#[tauri::command]
pub async fn loadorder_export_conf(app: AppHandle, mod_file: PathBuf) -> Result<View, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let all = all_mods(&loc);
        loadorder::export_conf(&loc.state_dir, &loc.user_dir, &all, &mod_file)?;
        view(&loc, &all, &mod_file)
    })
    .await
    .map_err(|e| e.to_string())?
}
