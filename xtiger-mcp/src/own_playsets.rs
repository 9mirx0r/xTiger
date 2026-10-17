//! Playsets that xTiger keeps itself: a named list of mods in load order, to test a mod together
//! with them. They live in the state folder the app and the MCP server share, so the user and the
//! assistant edit the same ones. The Paradox launcher's own playsets are never written to; a
//! playset of ours can start as a copy of one of them.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::loadorder::{self, Entry};
use crate::mods::{self, ModInfo};
use crate::playsets::{self, Playset, PlaysetMod};

const MAX_NAME: usize = 80;

const fn yes() -> bool {
    true
}

/// One mod of a playset. The name is kept so the mod can be found again when its `.mod` file moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnMod {
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    /// A mod that is switched off stays in the list but is not loaded.
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Own {
    pub name: String,
    /// What the playset is for.
    #[serde(default)]
    pub notes: String,
    /// In load order: the first one loads first.
    pub mods: Vec<OwnMod>,
    #[serde(default)]
    pub updated_ms: u64,
}

fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("playsets")
}

/// The name as it is kept: trimmed, in one line.
pub fn clean_name(name: &str) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("A playset needs a name.".to_owned());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("A playset name is at most {MAX_NAME} characters."));
    }
    if name.chars().any(char::is_control) {
        return Err("A playset name cannot hold control characters.".to_owned());
    }
    Ok(name)
}

/// The file of a playset: readable, and different for names that differ only in punctuation.
fn file(state_dir: &Path, name: &str) -> PathBuf {
    let lower = name.to_lowercase();
    let slug: String = lower
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(40)
        .collect();
    let digest = Sha256::digest(lower.as_bytes());
    let id = digest.iter().take(4).fold(String::new(), |mut id, byte| {
        let _ = write!(id, "{byte:02x}");
        id
    });
    dir(state_dir).join(format!("{slug}-{id}.json"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

pub fn load(state_dir: &Path, name: &str) -> Option<Own> {
    let name = clean_name(name).ok()?;
    let text = fs::read_to_string(file(state_dir, &name)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every playset, by name.
pub fn list(state_dir: &Path) -> Vec<Own> {
    let Ok(entries) = fs::read_dir(dir(state_dir)) else { return Vec::new() };
    let mut all: Vec<Own> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    all.sort_by_key(|own: &Own| own.name.to_lowercase());
    all
}

/// Keep `own` under its name, replacing the one that has it. Repeats are dropped.
pub fn save(state_dir: &Path, mut own: Own) -> Result<Own, String> {
    own.name = clean_name(&own.name)?;
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut kept = Vec::new();
    for item in own.mods {
        let path = mods::clean(&item.mod_file);
        if seen.contains(&path) {
            continue;
        }
        seen.push(path.clone());
        kept.push(OwnMod { mod_file: path, ..item });
    }
    own.mods = kept;
    let notes = own.notes.trim().to_owned();
    own.notes = notes;
    own.updated_ms = now_ms();
    let path = file(state_dir, &own.name);
    fs::create_dir_all(dir(state_dir))
        .map_err(|e| format!("Cannot create {}: {e}", dir(state_dir).display()))?;
    let text = serde_json::to_string_pretty(&own).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| format!("Cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, &path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(own)
}

/// Forget a playset. Whether there was one.
pub fn delete(state_dir: &Path, name: &str) -> Result<bool, String> {
    let path = file(state_dir, &clean_name(name)?);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Cannot remove {}: {e}", path.display())),
    }
}

/// Give a playset another name.
pub fn rename(state_dir: &Path, old: &str, new: &str) -> Result<Own, String> {
    let new = clean_name(new)?;
    let mut own = load(state_dir, old).ok_or_else(|| format!("No playset is called {old}."))?;
    if own.name.to_lowercase() != new.to_lowercase() && load(state_dir, &new).is_some() {
        return Err(format!("A playset called {new} already exists."));
    }
    let old_name = own.name.clone();
    own.name = new;
    let saved = save(state_dir, own)?;
    if file(state_dir, &old_name) != file(state_dir, &saved.name) {
        delete(state_dir, &old_name)?;
    }
    Ok(saved)
}

/// A playset that is a copy of `source`: its mods, switched on or off as there.
pub fn from_playset(name: &str, source: &Playset, all: &[ModInfo]) -> Result<Own, String> {
    let mods = source
        .mods
        .iter()
        .filter_map(|item| {
            let file = item.mod_file.clone().or_else(|| {
                item.folder.as_ref().map(|dir| dir.join("descriptor.mod")).filter(|p| p.is_file())
            })?;
            let entry = loadorder::entries(&[file], all).pop()?;
            Some(OwnMod { name: entry.name, mod_file: entry.mod_file, enabled: item.enabled })
        })
        .collect();
    Ok(Own { name: clean_name(name)?, notes: String::new(), mods, updated_ms: 0 })
}

/// The mods of a playset as the rest of the server reads them: the ones that are installed have
/// their `.mod` file; the others are listed without one.
pub fn to_playset(own: &Own, all: &[ModInfo]) -> Playset {
    let mods: Vec<PlaysetMod> = own
        .mods
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let entry = Entry { name: item.name.clone(), mod_file: item.mod_file.clone() };
            let mod_file = loadorder::find(&entry, all).ok();
            let folder = mod_file.as_ref().and_then(|file| mods::describe(file)).map(|i| i.dir);
            PlaysetMod {
                name: item.name.clone(),
                enabled: item.enabled,
                position: i64::try_from(position).unwrap_or(i64::MAX),
                mod_file,
                folder,
            }
        })
        .collect();
    Playset {
        name: own.name.clone(),
        active: false,
        enabled: mods.iter().filter(|item| item.enabled).count(),
        source: "xtiger",
        mods,
    }
}

/// The playset called `name`: one of ours (ignoring case), or else one of the launcher's.
pub fn find(
    state_dir: &Path,
    user_dir: &Path,
    all: &[ModInfo],
    name: &str,
) -> Result<Playset, String> {
    let wanted = name.trim().to_lowercase();
    let ours = list(state_dir);
    if let Some(own) = ours.iter().find(|own| own.name.to_lowercase() == wanted) {
        return Ok(to_playset(own, all));
    }
    match playsets::find(user_dir, name) {
        Ok(found) => Ok(found),
        Err(error) => {
            let close: Vec<&Own> =
                ours.iter().filter(|own| own.name.to_lowercase().contains(&wanted)).collect();
            if let [one] = close.as_slice() {
                return Ok(to_playset(one, all));
            }
            if ours.is_empty() {
                return Err(error);
            }
            let names: Vec<&str> = ours.iter().map(|own| own.name.as_str()).collect();
            Err(format!("{error} Playsets kept by xTiger: {}.", names.join(", ")))
        }
    }
}

/// A mod that the playset needs and that is not right yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Need {
    /// The name the descriptors give it.
    pub name: String,
    /// The mods of the playset (or pulled in by it) whose descriptors name it.
    pub needed_by: Vec<String>,
    /// `add` (installed, but not in the playset), `late` (in the playset, but after a mod that
    /// needs it) or `missing` (nothing installed has this name).
    pub state: &'static str,
    /// The installed mod, for `add` and `late`.
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub mod_file: Option<PathBuf>,
}

/// What the descriptors of the loaded mods of a playset ask for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Analysis {
    /// Only what is wrong; a dependency that is loaded in time is not listed.
    pub needs: Vec<Need>,
    /// The order of the loaded mods with every mod after the mods it needs, changing as little as
    /// possible. `None` when the order is fine already.
    pub suggested: Option<Vec<PathBuf>>,
}

/// Check the mods of `own` that are switched on against what their descriptors need.
pub fn analyze(own: &Own, all: &[ModInfo]) -> Analysis {
    // The loaded mods, in order, as installed mods; one that cannot be found has no descriptor to read.
    let loaded: Vec<ModInfo> = own
        .mods
        .iter()
        .filter(|item| item.enabled)
        .filter_map(|item| {
            let entry = Entry { name: item.name.clone(), mod_file: item.mod_file.clone() };
            let file = mods::clean(&loadorder::find(&entry, all).ok()?);
            all.iter().find(|info| mods::clean(&info.mod_file) == file).cloned()
        })
        .collect();
    let position = |file: &Path| {
        let file = mods::clean(file);
        loaded.iter().position(|info| mods::clean(&info.mod_file) == file)
    };
    let mut needs: Vec<Need> = Vec::new();
    let mut note = |name: &str, owner: &str, state: &'static str, mod_file: Option<PathBuf>| {
        let lower = name.to_lowercase();
        if let Some(found) = needs.iter_mut().find(|need| need.name.to_lowercase() == lower) {
            if !found.needed_by.iter().any(|by| by == owner) {
                found.needed_by.push(owner.to_owned());
            }
        } else {
            needs.push(Need {
                name: name.to_owned(),
                needed_by: vec![owner.to_owned()],
                state,
                mod_file,
            });
        }
    };
    // The dependencies of the dependencies that have to be added are looked at too.
    let mut queue: Vec<ModInfo> = loaded.clone();
    let mut queued: Vec<PathBuf> = loaded.iter().map(|info| mods::clean(&info.mod_file)).collect();
    let mut next = 0;
    while next < queue.len() {
        let owner = queue[next].clone();
        next += 1;
        let owner_position = position(&owner.mod_file);
        for name in &owner.dependencies {
            let lower = name.to_lowercase();
            if lower == owner.name.to_lowercase() {
                continue;
            }
            let mut matches: Vec<&ModInfo> = all
                .iter()
                .filter(|info| info.name.to_lowercase() == lower && !info.path_missing)
                .collect();
            // The copy that is in the playset wins, then the usual order of sources.
            matches.sort_by_key(|info| {
                (position(&info.mod_file).is_none(), crate::setup::source_rank(info.source))
            });
            let Some(pick) = matches.first() else {
                note(name, &owner.name, "missing", None);
                continue;
            };
            let file = mods::clean(&pick.mod_file);
            match (position(&file), owner_position) {
                (Some(dep), Some(own_at)) if dep > own_at => {
                    note(name, &owner.name, "late", Some(file));
                }
                (Some(_), _) => {}
                (None, _) => {
                    note(name, &owner.name, "add", Some(file.clone()));
                    if !queued.contains(&file) {
                        queued.push(file);
                        queue.push((*pick).clone());
                    }
                }
            }
        }
    }
    Analysis { needs, suggested: suggested_order(&loaded) }
}

/// Every mod after the mods it needs, keeping the given order wherever it already holds.
fn suggested_order(loaded: &[ModInfo]) -> Option<Vec<PathBuf>> {
    fn visit(at: usize, loaded: &[ModInfo], placed: &mut Vec<usize>, visiting: &mut Vec<usize>) {
        if placed.contains(&at) || visiting.contains(&at) {
            return;
        }
        visiting.push(at);
        for name in &loaded[at].dependencies {
            let lower = name.to_lowercase();
            if let Some(dep) = loaded.iter().position(|info| info.name.to_lowercase() == lower) {
                visit(dep, loaded, placed, visiting);
            }
        }
        visiting.pop();
        placed.push(at);
    }
    let mut placed: Vec<usize> = Vec::new();
    for at in 0..loaded.len() {
        visit(at, loaded, &mut placed, &mut Vec::new());
    }
    let same = placed.iter().enumerate().all(|(i, at)| i == *at);
    (!same).then(|| placed.into_iter().map(|at| mods::clean(&loaded[at].mod_file)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn own(name: &str, mods: &[(&str, bool)]) -> Own {
        Own {
            name: name.to_owned(),
            notes: String::new(),
            mods: mods
                .iter()
                .map(|(mod_name, enabled)| OwnMod {
                    name: (*mod_name).to_owned(),
                    mod_file: PathBuf::from(format!("/m/{mod_name}.mod")),
                    enabled: *enabled,
                })
                .collect(),
            updated_ms: 0,
        }
    }

    #[test]
    fn a_playset_is_kept_listed_renamed_and_deleted() {
        let tmp = TempDir::new();
        assert_eq!(list(&tmp).len(), 0);
        save(&tmp, own("  Patch  test ", &[("a", true), ("b", false), ("a", true)])).unwrap();
        save(&tmp, own("Big", &[])).unwrap();
        let names: Vec<String> = list(&tmp).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Big", "Patch test"]);
        let kept = load(&tmp, "patch TEST").unwrap();
        assert_eq!(kept.mods.len(), 2);
        assert!(!kept.mods[1].enabled);
        assert!(save(&tmp, own("  ", &[])).unwrap_err().contains("needs a name"));
        assert!(rename(&tmp, "Big", "patch test").unwrap_err().contains("already exists"));
        rename(&tmp, "Big", "Huge").unwrap();
        assert!(load(&tmp, "Big").is_none());
        assert_eq!(load(&tmp, "huge").unwrap().name, "Huge");
        // A change of case only keeps the same record.
        rename(&tmp, "Huge", "HUGE").unwrap();
        assert_eq!(list(&tmp).len(), 2);
        assert!(delete(&tmp, "huge").unwrap());
        assert!(!delete(&tmp, "huge").unwrap());
    }

    #[test]
    fn names_that_differ_only_in_punctuation_do_not_share_a_file() {
        let tmp = TempDir::new();
        assert_ne!(file(&tmp, "a b"), file(&tmp, "a-b"));
        assert_eq!(file(&tmp, "A B"), file(&tmp, "a b"));
    }

    #[test]
    fn ours_come_before_the_launchers_and_a_missing_name_lists_ours() {
        let tmp = TempDir::new();
        let state = tmp.join("state");
        let user = tmp.join("user");
        save(&state, own("Patch test", &[("a", true)])).unwrap();
        let found = find(&state, &user, &[], "patch TEST").unwrap();
        assert_eq!(found.source, "xtiger");
        assert_eq!(found.enabled, 1);
        // The mod is not installed, so it has no .mod file.
        assert!(found.mods[0].mod_file.is_none());
        let found = find(&state, &user, &[], "patch").unwrap();
        assert_eq!(found.name, "Patch test");
        let error = find(&state, &user, &[], "nope").unwrap_err();
        assert!(error.contains("Playsets kept by xTiger: Patch test"), "{error}");
    }

    fn info(name: &str, deps: &[&str]) -> ModInfo {
        ModInfo {
            mod_file: PathBuf::from(format!("/m/{name}.mod")),
            dir: PathBuf::from(format!("/m/{name}")),
            name: name.to_owned(),
            version: None,
            supported_version: None,
            source: "local",
            picture: None,
            dependencies: deps.iter().map(|d| (*d).to_owned()).collect(),
            path_missing: false,
        }
    }

    #[test]
    fn what_a_playset_still_needs_is_listed() {
        let all =
            [info("Base", &[]), info("Extra", &[]), info("Patch", &["Base", "Extra", "Gone"])];
        // Patch is first: Base is not in the playset, Extra loads after it, Gone is not installed.
        let set = own("Mix", &[("Patch", true), ("Extra", true)]);
        let found = analyze(&set, &all);
        let state = |name: &str| found.needs.iter().find(|n| n.name == name).map(|n| n.state);
        assert_eq!(state("Base"), Some("add"));
        assert_eq!(state("Extra"), Some("late"));
        assert_eq!(state("Gone"), Some("missing"));
        assert_eq!(found.needs.len(), 3);
        assert_eq!(
            found.suggested,
            Some(vec![PathBuf::from("/m/Extra.mod"), PathBuf::from("/m/Patch.mod")])
        );
    }

    #[test]
    fn a_playset_in_order_has_nothing_to_report() {
        let all = [info("Base", &[]), info("Patch", &["base"])];
        let found = analyze(&own("Ok", &[("Base", true), ("Patch", true)]), &all);
        assert_eq!(found, Analysis::default());
        // A mod that is switched off does not count as loaded, so it is still to be added.
        let found = analyze(&own("Off", &[("Base", false), ("Patch", true)]), &all);
        assert_eq!(found.needs.len(), 1);
        assert_eq!(found.needs[0].state, "add");
    }

    #[test]
    fn dependencies_of_a_mod_to_add_are_listed_too_and_cycles_end() {
        let all = [info("A", &["B"]), info("B", &["C"]), info("C", &["B"])];
        let found = analyze(&own("Chain", &[("A", true)]), &all);
        let names: Vec<&str> = found.needs.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["B", "C"]);
        assert_eq!(found.needs[1].needed_by, ["B"]);
    }
}
