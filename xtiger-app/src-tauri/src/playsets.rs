//! The playsets xTiger keeps, in the window. They are the ones the assistants read and write
//! through `xtiger_playsets`, so both sides see the same lists. The launcher's playsets are only
//! read, to start one of ours from.

use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use xtiger_mcp::own_playsets::{self, Own};
use xtiger_mcp::playsets;

use crate::loadorder::{all_mods, locations};

/// One mod of a playset as the window shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditMod {
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    pub enabled: bool,
    /// The mod can still be found; when it cannot, a test goes on without it and says so.
    pub installed: bool,
}

/// A playset of ours with all its mods, in load order.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub name: String,
    pub notes: String,
    pub mods: Vec<EditMod>,
    /// What the descriptors of the loaded mods still need: only what is wrong.
    pub needs: Vec<own_playsets::Need>,
    /// The loaded mods with each after the mods it needs, when the order is not that already.
    pub suggested: Option<Vec<PathBuf>>,
}

/// A playset of the Paradox launcher, to copy from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherBrief {
    pub name: String,
    pub active: bool,
    pub mods: usize,
    pub enabled: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub ours: Vec<Edit>,
    pub launcher: Vec<LauncherBrief>,
}

fn edit(own: &Own, all: &[xtiger_mcp::mods::ModInfo]) -> Edit {
    let shown = own_playsets::to_playset(own, all);
    let checked = own_playsets::analyze(own, all);
    Edit {
        name: own.name.clone(),
        notes: own.notes.clone(),
        mods: own
            .mods
            .iter()
            .zip(&shown.mods)
            .map(|(item, found)| EditMod {
                name: item.name.clone(),
                mod_file: item.mod_file.clone(),
                enabled: item.enabled,
                installed: found.mod_file.is_some(),
            })
            .collect(),
        needs: checked.needs,
        suggested: checked.suggested,
    }
}

/// Every playset of ours, and the launcher's to copy from.
#[tauri::command]
pub async fn playsets_overview(app: AppHandle) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let all = all_mods(&loc);
        let ours = own_playsets::list(&loc.state_dir).iter().map(|own| edit(own, &all)).collect();
        // The launcher may not be installed; the playsets of xTiger are still there.
        let launcher = playsets::list(&loc.user_dir)
            .unwrap_or_default()
            .into_iter()
            .map(|set| LauncherBrief {
                name: set.name,
                active: set.active,
                mods: set.mods.len(),
                enabled: set.enabled,
            })
            .collect();
        Ok(Overview { ours, launcher })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Keep `playset` as it is, replacing the one with its name.
#[tauri::command]
pub async fn playsets_save(app: AppHandle, playset: Own) -> Result<Edit, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let saved = own_playsets::save(&loc.state_dir, playset)?;
        Ok(edit(&saved, &all_mods(&loc)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A new playset: empty, or a copy of one of ours or of the launcher's.
#[tauri::command]
pub async fn playsets_create(
    app: AppHandle,
    name: String,
    from: Option<String>,
) -> Result<Edit, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let all = all_mods(&loc);
        let name = own_playsets::clean_name(&name)?;
        if own_playsets::load(&loc.state_dir, &name).is_some() {
            return Err(format!("A playset called {name} already exists."));
        }
        let own = match from.as_deref().filter(|from| !from.trim().is_empty()) {
            Some(from) => {
                let source = own_playsets::find(&loc.state_dir, &loc.user_dir, &all, from)?;
                own_playsets::from_playset(&name, &source, &all)?
            }
            None => Own { name, notes: String::new(), mods: Vec::new(), updated_ms: 0 },
        };
        let saved = own_playsets::save(&loc.state_dir, own)?;
        Ok(edit(&saved, &all))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn playsets_rename(
    app: AppHandle,
    name: String,
    new_name: String,
) -> Result<Edit, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        let saved = own_playsets::rename(&loc.state_dir, &name, &new_name)?;
        Ok(edit(&saved, &all_mods(&loc)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn playsets_delete(app: AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let loc = locations(&app)?;
        own_playsets::delete(&loc.state_dir, &name).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}
