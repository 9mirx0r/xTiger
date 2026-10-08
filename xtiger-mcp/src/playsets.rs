//! The playsets of the Paradox launcher: which mods the user plays together, in which order. They
//! are read from the launcher's database in the CK3 user folder, which is never written to.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

pub const DB: &str = "launcher-v2.sqlite";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlaysetMod {
    pub name: String,
    pub enabled: bool,
    pub position: i64,
    /// The `.mod` file the game loads, when it is there.
    pub mod_file: Option<PathBuf>,
    pub folder: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Playset {
    pub name: String,
    /// The one the launcher has selected.
    pub active: bool,
    pub enabled: usize,
    pub mods: Vec<PlaysetMod>,
}

fn open(user_dir: &Path) -> Result<Connection, String> {
    let path = user_dir.join(DB);
    if !path.is_file() {
        return Err(format!(
            "No launcher database at {}. Playsets are made in the Paradox launcher.",
            path.display()
        ));
    }
    Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("cannot open {}: {e}", path.display()))
}

/// Every playset with its mods in load order.
pub fn list(user_dir: &Path) -> Result<Vec<Playset>, String> {
    let db = open(user_dir)?;
    let failed = |e: rusqlite::Error| format!("cannot read the launcher's playsets: {e}");
    let mut query = db
        .prepare(
            "SELECT p.id, p.name, p.isActive, pm.enabled, pm.position, \
                    COALESCE(m.displayName, m.name), m.gameRegistryId, m.dirPath \
             FROM playsets p \
             LEFT JOIN playsets_mods pm ON pm.playsetId = p.id \
             LEFT JOIN mods m ON m.id = pm.modId \
             ORDER BY p.name, p.id, pm.position",
        )
        .map_err(failed)?;
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<bool>>(2)?,
                row.get::<_, Option<bool>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(failed)?;
    let mut playsets: Vec<(String, Playset)> = Vec::new();
    for row in rows {
        let (id, name, active, enabled, position, mod_name, registry, dir) = row.map_err(failed)?;
        if playsets.last().is_none_or(|(last, _)| *last != id) {
            let playset = Playset {
                name: name.unwrap_or_else(|| "(no name)".to_owned()),
                active: active.unwrap_or(false),
                enabled: 0,
                mods: Vec::new(),
            };
            playsets.push((id.clone(), playset));
        }
        let Some((_, playset)) = playsets.last_mut() else { continue };
        let Some(position) = position else { continue };
        let mod_file = registry.map(|rel| user_dir.join(rel)).filter(|path| path.is_file());
        let enabled = enabled.unwrap_or(false);
        playset.enabled += usize::from(enabled);
        playset.mods.push(PlaysetMod {
            name: mod_name.unwrap_or_else(|| "(unknown mod)".to_owned()),
            enabled,
            position,
            mod_file,
            folder: dir.map(PathBuf::from),
        });
    }
    Ok(playsets.into_iter().map(|(_, playset)| playset).collect())
}

/// The playset called `name` (ignoring case), or the one that contains it if only one does.
pub fn find(user_dir: &Path, name: &str) -> Result<Playset, String> {
    let playsets = list(user_dir)?;
    let wanted = name.trim().to_lowercase();
    if let Some(exact) = playsets.iter().find(|p| p.name.to_lowercase() == wanted) {
        return Ok(exact.clone());
    }
    let close: Vec<&Playset> =
        playsets.iter().filter(|p| p.name.to_lowercase().contains(&wanted)).collect();
    match close.as_slice() {
        [one] => Ok((*one).clone()),
        [] => {
            let names: Vec<&str> = playsets.iter().map(|p| p.name.as_str()).collect();
            Err(format!("No playset is called {name}. The playsets are: {}.", names.join(", ")))
        }
        many => {
            let names: Vec<&str> = many.iter().map(|p| p.name.as_str()).collect();
            Err(format!("More than one playset matches {name}: {}.", names.join(", ")))
        }
    }
}

/// The `.mod` files of the enabled mods of a playset, in load order, and the names of the enabled
/// mods whose file is missing.
pub fn load_order(playset: &Playset) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut missing = Vec::new();
    for item in playset.mods.iter().filter(|item| item.enabled) {
        match &item.mod_file {
            Some(file) => files.push(file.clone()),
            None => missing.push(item.name.clone()),
        }
    }
    (files, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;

    fn launcher(tmp: &Path) {
        let db = Connection::open(tmp.join(DB)).unwrap();
        db.execute_batch(
            "CREATE TABLE playsets (id TEXT PRIMARY KEY, name TEXT, isActive BOOLEAN);
             CREATE TABLE mods (id TEXT PRIMARY KEY, name TEXT, displayName TEXT, gameRegistryId TEXT, dirPath TEXT);
             CREATE TABLE playsets_mods (playsetId TEXT, modId TEXT, enabled BOOLEAN, position INTEGER);
             INSERT INTO playsets VALUES ('a', 'Big Game', 1), ('b', 'Testing', 0), ('c', 'Empty', 0);
             INSERT INTO mods VALUES ('m1', 'one', 'Silk Road', 'mod/silk.mod', '/m/silk'),
                                     ('m2', 'two', 'Courts', 'mod/courts.mod', '/m/courts'),
                                     ('m3', 'three', NULL, 'mod/gone.mod', NULL);
             INSERT INTO playsets_mods VALUES ('a', 'm2', 1, 1), ('a', 'm1', 1, 0), ('a', 'm3', 1, 2),
                                              ('b', 'm1', 0, 0);",
        )
        .unwrap();
        fs::create_dir_all(tmp.join("mod")).unwrap();
        fs::write(tmp.join("mod/silk.mod"), "").unwrap();
        fs::write(tmp.join("mod/courts.mod"), "").unwrap();
    }

    #[test]
    fn playsets_come_in_load_order() {
        let tmp = TempDir::new();
        assert!(list(&tmp).unwrap_err().contains("No launcher database"));
        launcher(&tmp);
        let playsets = list(&tmp).unwrap();
        let names: Vec<&str> = playsets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Big Game", "Empty", "Testing"]);
        assert!(playsets[0].active);
        assert_eq!(playsets[0].enabled, 3);
        assert_eq!(playsets[1].mods.len(), 0);
        let big = find(&tmp, "big game").unwrap();
        let (files, missing) = load_order(&big);
        assert_eq!(files, [tmp.join("mod/silk.mod"), tmp.join("mod/courts.mod")]);
        assert_eq!(missing, ["three"]);
        assert_eq!(load_order(&find(&tmp, "test").unwrap()).0.len(), 0);
        assert!(find(&tmp, "nope").unwrap_err().contains("Big Game, Empty, Testing"));
        assert!(find(&tmp, "e").unwrap_err().contains("More than one"));
    }
}
