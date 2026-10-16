//! The mods a mod needs loaded before it: a patch loads after the mods it patches, a mod built
//! on four others loads after all four. Testing a mod without them reports everything they
//! define as missing and runs a game that cannot start the mod's own content.
//!
//! The list is saved per mod, in the state folder the MCP server and the app share, and the
//! validator and the game runs use it by default. [`resolve`] turns it, with the dependencies
//! named in the mod's descriptor, into the mods to load in game order.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::mods::{self, ModInfo};
use crate::setup::{self, Loaded, Unresolved};

/// One mod of a load order. The name is kept so the list can still be read, and the mod found
/// again, when its `.mod` file moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
}

/// What is kept for one mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    /// The `.mod` file of the mod that is tested.
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    pub name: String,
    /// The mods that load before it, in game order.
    pub requires: Vec<Entry>,
    pub updated_ms: u64,
}

/// A mod that a descriptor names as needed, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    pub name: String,
    /// The mod whose descriptor names it.
    pub needed_by: String,
    /// `listed` (it is in the saved list), `automatic` (installed, so a test loads it before
    /// the mod that needs it), `configured` (the mod's own `ck3-tiger.conf` already loads it) or
    /// `missing` (nothing installed has this name, or its folder is gone).
    pub state: &'static str,
    /// The installed mod, when there is one.
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub mod_file: Option<PathBuf>,
}

/// The dependencies the descriptors of `main` and of the mods in `saved` name, and of the mods
/// those pull in, each with where it stands. A mod that is the one tested is not a dependency.
pub fn dependencies(all: &[ModInfo], main: &Path, saved: &[PathBuf]) -> Vec<Dependency> {
    let main = mods::clean(main);
    let saved: Vec<PathBuf> = saved.iter().map(|file| mods::clean(file)).collect();
    let main_name = all
        .iter()
        .find(|info| mods::clean(&info.mod_file) == main)
        .map(|info| info.name.to_lowercase())
        .or_else(|| mods::describe(&main).map(|info| info.name.to_lowercase()))
        .unwrap_or_default();
    let mut queue: Vec<ModInfo> = Vec::new();
    for file in std::iter::once(&main).chain(saved.iter()) {
        if let Some(info) = all
            .iter()
            .find(|info| mods::clean(&info.mod_file) == *file)
            .cloned()
            .or_else(|| mods::describe(file))
        {
            queue.push(info);
        }
    }
    let mut seen_mods: HashSet<PathBuf> =
        queue.iter().map(|info| mods::clean(&info.mod_file)).collect();
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
    let mut found = Vec::new();
    let mut next = 0;
    while next < queue.len() {
        let owner = queue[next].clone();
        next += 1;
        for name in &owner.dependencies {
            let lower = name.to_lowercase();
            if lower == main_name || !seen_pairs.insert((owner.name.to_lowercase(), lower.clone()))
            {
                continue;
            }
            let mut matches: Vec<&ModInfo> = all
                .iter()
                .filter(|info| info.name.to_lowercase() == lower)
                .filter(|info| mods::clean(&info.mod_file) != main)
                .collect();
            matches.sort_by_key(|info| (info.path_missing, setup::source_rank(info.source)));
            let pick = matches.first().filter(|info| !info.path_missing);
            let (state, mod_file) = match pick {
                None => ("missing", None),
                Some(info) => {
                    let file = mods::clean(&info.mod_file);
                    let state = if saved.contains(&file) { "listed" } else { "automatic" };
                    if seen_mods.insert(file.clone()) {
                        queue.push((*info).clone());
                    }
                    (state, Some(file))
                }
            };
            found.push(Dependency {
                name: name.clone(),
                needed_by: owner.name.clone(),
                state,
                mod_file,
            });
        }
    }
    found
}

/// What a test of a mod loads before it.
#[derive(Debug, Default, Serialize)]
pub struct Order {
    /// What the descriptors of the mod and of the saved mods name as needed.
    pub dependencies: Vec<Dependency>,
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    /// The saved list was used.
    pub saved: bool,
    /// The mods to load, in game order. The mod itself loads after all of them.
    pub loaded: Vec<Loaded>,
    pub unresolved: Vec<Unresolved>,
    pub warnings: Vec<String>,
    /// The validator config that loads them, when there are any.
    #[serde(skip)]
    pub conf: Option<String>,
}

impl Order {
    /// The `.mod` files to load, in game order.
    pub fn files(&self) -> Vec<PathBuf> {
        self.loaded.iter().map(|loaded| loaded.mod_file.clone()).collect()
    }

    /// A line for an answer: what loads and in which order.
    pub fn describe(&self) -> String {
        let mut line = if self.loaded.is_empty() {
            format!("Load order: {} alone.", self.name)
        } else {
            let names: Vec<&str> = self.loaded.iter().map(|l| l.name.as_str()).collect();
            format!("Load order: {}, then {}.", names.join(", "), self.name)
        };
        for missing in &self.unresolved {
            let _ = write!(line, " Not loaded: {} ({}).", missing.name, missing.reason);
        }
        line
    }
}

fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("loadorders")
}

/// The file that holds the order of a mod. It is named after the mod's folder, so a copy of the
/// `.mod` file somewhere else leads to the same list.
pub fn path(state_dir: &Path, mod_file: &Path) -> PathBuf {
    let folder = mods::describe(mod_file).map_or_else(|| mods::clean(mod_file), |info| info.dir);
    let key = mods::clean(&folder).to_string_lossy().replace('\\', "/").to_lowercase();
    let digest = Sha256::digest(key.trim_end_matches('/').as_bytes());
    let id = digest.iter().take(8).fold(String::new(), |mut id, byte| {
        let _ = write!(id, "{byte:02x}");
        id
    });
    dir(state_dir).join(format!("{id}.json"))
}

pub fn load(state_dir: &Path, mod_file: &Path) -> Option<Saved> {
    let text = fs::read_to_string(path(state_dir, mod_file)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every saved order, for a screen that lists them.
pub fn list(state_dir: &Path) -> Vec<Saved> {
    let Ok(entries) = fs::read_dir(dir(state_dir)) else { return Vec::new() };
    let mut all: Vec<Saved> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    all.sort_by_key(|saved: &Saved| std::cmp::Reverse(saved.updated_ms));
    all
}

/// Save the order of `mod_file`. The mod itself and repeats are dropped; an empty list removes
/// the record.
pub fn save(state_dir: &Path, mod_file: &Path, requires: Vec<Entry>) -> Result<Saved, String> {
    let main = mods::clean(mod_file);
    let info =
        mods::describe(&main).ok_or_else(|| format!("Cannot read {}", mod_file.display()))?;
    let mut kept: Vec<Entry> = Vec::new();
    for entry in requires {
        let file = mods::clean(&entry.mod_file);
        if file == main || kept.iter().any(|have| mods::clean(&have.mod_file) == file) {
            continue;
        }
        kept.push(Entry { name: entry.name, mod_file: file });
    }
    let saved = Saved {
        mod_file: main.clone(),
        name: info.name,
        requires: kept,
        updated_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
    };
    if saved.requires.is_empty() {
        clear(state_dir, &main)?;
        return Ok(saved);
    }
    let file = path(state_dir, &main);
    fs::create_dir_all(dir(state_dir))
        .map_err(|e| format!("Cannot create {}: {e}", dir(state_dir).display()))?;
    let text = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    let temp = file.with_extension("json.tmp");
    fs::write(&temp, text).map_err(|e| format!("Cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, &file).map_err(|e| format!("Cannot write {}: {e}", file.display()))?;
    Ok(saved)
}

/// Forget the order of a mod. Whether there was one.
pub fn clear(state_dir: &Path, mod_file: &Path) -> Result<bool, String> {
    let file = path(state_dir, mod_file);
    match fs::remove_file(&file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Cannot remove {}: {e}", file.display())),
    }
}

/// Entries for `.mod` files, with the names the mods have now.
pub fn entries(files: &[PathBuf], all: &[ModInfo]) -> Vec<Entry> {
    files
        .iter()
        .map(|file| {
            let file = mods::clean(file);
            let name = all
                .iter()
                .find(|info| mods::clean(&info.mod_file) == file)
                .map(|info| info.name.clone())
                .or_else(|| mods::describe(&file).map(|info| info.name))
                .unwrap_or_else(|| file.display().to_string());
            Entry { name, mod_file: file }
        })
        .collect()
}

/// The `.mod` file of a saved entry: the file itself, or else an installed mod of that name.
pub(crate) fn find(entry: &Entry, all: &[ModInfo]) -> Result<PathBuf, String> {
    let file = mods::clean(&entry.mod_file);
    if mods::describe(&file).is_some_and(|info| !info.path_missing) {
        return Ok(file);
    }
    let lower = entry.name.to_lowercase();
    all.iter()
        .find(|info| !info.path_missing && info.name.to_lowercase() == lower)
        .map(|info| mods::clean(&info.mod_file))
        .ok_or_else(|| {
            format!("saved in the load order, but \"{}\" is no longer installed", entry.name)
        })
}

/// The saved list of `mod_file` as `.mod` files that are installed, in order, and the entries
/// that are not.
pub fn saved_files(
    state_dir: &Path,
    mod_file: &Path,
    all: &[ModInfo],
) -> (Vec<PathBuf>, Vec<Unresolved>) {
    let main = mods::clean(mod_file);
    let mut files: Vec<PathBuf> = Vec::new();
    let mut unresolved = Vec::new();
    for entry in load(state_dir, &main).map(|saved| saved.requires).unwrap_or_default() {
        match find(&entry, all) {
            Ok(file) => {
                if file != main && !files.contains(&file) {
                    files.push(file);
                }
            }
            Err(reason) => unresolved.push(Unresolved { name: entry.name, reason }),
        }
    }
    (files, unresolved)
}

/// What the validator loads for `mod_file`: [`setup::plan`] with the saved list (when
/// `use_saved`) in front of `with`. The saved order is the user's, so the descriptor's
/// dependencies fill in around it. Entries of the list that are not installed are in
/// `unresolved`.
pub fn plan_check(
    state_dir: &Path,
    all: &[ModInfo],
    request: &setup::Request,
    use_saved: bool,
) -> Result<setup::Setup, String> {
    let (saved, missing) = if use_saved {
        saved_files(state_dir, request.mod_file, all)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut with = saved.clone();
    for file in request.with {
        let file = mods::clean(file);
        if !with.contains(&file) {
            with.push(file);
        }
    }
    let mut setup = setup::plan(
        &setup::Request {
            with: &with,
            with_first: request.with_first || !saved.is_empty(),
            ..*request
        },
        all,
    )?;
    for loaded in &mut setup.loaded {
        if saved.contains(&mods::clean(&loaded.mod_file)) && loaded.why.starts_with("asked for") {
            "in the saved load order".clone_into(&mut loaded.why);
        }
    }
    setup.unresolved.extend(missing);
    Ok(setup)
}

/// What a test of `mod_file` loads before it: the saved list in its order (when `use_saved`),
/// the dependencies its descriptor names, and what its `ck3-tiger.conf` already loads. Each mod
/// comes after the mods it needs.
pub fn resolve(
    state_dir: &Path,
    paradox: &Path,
    all: &[ModInfo],
    mod_file: &Path,
    use_saved: bool,
) -> Result<Order, String> {
    let main = mods::clean(mod_file);
    let info =
        mods::describe(&main).ok_or_else(|| format!("Cannot read {}", mod_file.display()))?;
    let mut order = Order { name: info.name, mod_file: main.clone(), ..Order::default() };
    let mut with = Vec::new();
    if use_saved {
        let (files, unresolved) = saved_files(state_dir, &main, all);
        order.saved = !files.is_empty();
        order.unresolved = unresolved;
        with = files;
    }
    let setup = setup::plan(
        &setup::Request {
            mod_file: &main,
            config: None,
            with: &with,
            with_first: true,
            load_dependencies: true,
            paradox,
        },
        all,
    )?;
    for mut loaded in setup.loaded {
        if loaded.why.starts_with("asked for") {
            "in the saved load order".clone_into(&mut loaded.why);
        }
        order.loaded.push(loaded);
    }
    order.dependencies = dependencies(all, &main, &with);
    // A dependency the mod's own config already loads is not loaded again.
    for dep in &mut order.dependencies {
        let by_config = setup.configured.iter().any(|loaded| {
            loaded.name.eq_ignore_ascii_case(&dep.name)
                || dep.mod_file.as_ref().is_some_and(|file| mods::clean(&loaded.mod_file) == *file)
        });
        if by_config {
            dep.state = "configured";
        }
    }
    // What the config loads comes first in the generated config, so it comes first here.
    order.loaded.splice(0..0, setup.configured);
    order.unresolved.extend(setup.unresolved);
    order.warnings = setup.warnings;
    order.conf = setup.conf;
    Ok(order)
}

const BEGIN: &str = "# xTiger load order: begin (written by the xTiger app; delete it to stop)";
const END: &str = "# xTiger load order: end";

/// The `load_mod` blocks of a mod's load order, between markers, for a `ck3-tiger.conf`.
pub fn conf_block(loaded: &[Loaded]) -> String {
    let mut block = format!("{BEGIN}\n");
    let mut labels = std::collections::HashSet::new();
    for item in loaded {
        let file = item.mod_file.to_string_lossy().replace('\\', "/");
        if file.contains('"') {
            continue;
        }
        let label = setup::unique_label(&item.name, &mut labels);
        let _ = writeln!(block, "load_mod = {{ label = \"{label}\" modfile = \"{file}\" }}");
    }
    block.push_str(END);
    block.push('\n');
    block
}

/// `text` without the block [`conf_block`] wrote, if it has one.
pub fn strip_conf_block(text: &str) -> String {
    let (Some(start), Some(end)) = (text.find(BEGIN), text.find(END)) else {
        return text.to_owned();
    };
    if end < start {
        return text.to_owned();
    }
    let after = &text[end + END.len()..];
    let after = after.strip_prefix("\r\n").or_else(|| after.strip_prefix('\n')).unwrap_or(after);
    let before = text[..start].trim_end_matches([' ', '\t']);
    // The blank line that was put in front of the block goes with it.
    let before = before.strip_suffix("\n\n").map_or(before, |kept| &before[..=kept.len()]);
    format!("{before}{after}")
}

/// Write the load order of a mod into its `ck3-tiger.conf`, so the validator finds it without
/// xTiger. What the config already loads by hand stays; an earlier block of ours is replaced.
/// The old file is kept next to it as `ck3-tiger.conf.xtiger-backup`. The file is the mod's own,
/// and the block holds paths of this computer.
pub fn export_conf(
    state_dir: &Path,
    paradox: &Path,
    all: &[ModInfo],
    mod_file: &Path,
) -> Result<PathBuf, String> {
    let info = mods::describe(&mods::clean(mod_file))
        .ok_or_else(|| format!("Cannot read {}", mod_file.display()))?;
    let file = info.dir.join("ck3-tiger.conf");
    let old = match fs::read_to_string(&file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("Cannot read {}: {e}", file.display())),
    };
    let stripped = old.as_deref().map(strip_conf_block).unwrap_or_default();
    if let Some(old) = &old {
        let backup = info.dir.join("ck3-tiger.conf.xtiger-backup");
        if !backup.exists() {
            fs::write(&backup, old)
                .map_err(|e| format!("Cannot write {}: {e}", backup.display()))?;
        }
        // The mods the config loads by hand are not ours to write again, and our own earlier
        // block must not count as one of them.
        fs::write(&file, &stripped).map_err(|e| format!("Cannot write {}: {e}", file.display()))?;
    }
    let result = resolve(state_dir, paradox, all, mod_file, true).and_then(|order| {
        let ours: Vec<Loaded> = order
            .loaded
            .into_iter()
            .filter(|loaded| !loaded.why.starts_with("loaded by the config"))
            .collect();
        let mut text = stripped.clone();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !ours.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&conf_block(&ours));
        }
        fs::write(&file, text).map_err(|e| format!("Cannot write {}: {e}", file.display()))
    });
    if result.is_err()
        && let Some(old) = &old
    {
        let _ = fs::write(&file, old);
    }
    result.map(|()| file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    struct World {
        tmp: TempDir,
    }

    impl World {
        fn new() -> Self {
            let tmp = TempDir::new();
            fs::create_dir_all(tmp.join("mod")).unwrap();
            fs::create_dir_all(tmp.join("state")).unwrap();
            Self { tmp }
        }

        fn state(&self) -> PathBuf {
            self.tmp.join("state")
        }

        fn add(&self, file: &str, name: &str, deps: &[&str]) -> PathBuf {
            fs::create_dir_all(self.tmp.join("mod").join(file)).unwrap();
            let deps = if deps.is_empty() {
                String::new()
            } else {
                let names: Vec<String> = deps.iter().map(|d| format!("\"{d}\"")).collect();
                format!("dependencies = {{ {} }}\n", names.join(" "))
            };
            let path = self.tmp.join("mod").join(format!("{file}.mod"));
            fs::write(&path, format!("name=\"{name}\"\npath=\"mod/{file}\"\n{deps}")).unwrap();
            path
        }

        fn all(&self) -> Vec<ModInfo> {
            mods::list(Some(&self.tmp), &[])
        }

        fn keep(&self, main: &Path, requires: &[&Path]) {
            let files: Vec<PathBuf> = requires.iter().map(|p| p.to_path_buf()).collect();
            save(&self.state(), main, entries(&files, &self.all())).unwrap();
        }

        fn order(&self, main: &Path) -> Order {
            resolve(&self.state(), &self.tmp, &self.all(), main, true).unwrap()
        }
    }

    fn names(order: &Order) -> Vec<&str> {
        order.loaded.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn a_patch_for_two_mods_loads_after_both_in_the_saved_order() {
        // Issue case 1: patch C makes A work with B; the patch declares nothing.
        let world = World::new();
        let a = world.add("a", "Mod A", &[]);
        let b = world.add("b", "Mod B", &[]);
        let c = world.add("c", "Patch C", &[]);
        assert_eq!(world.order(&c).loaded.len(), 0);
        world.keep(&c, &[&b, &a]);
        let order = world.order(&c);
        assert_eq!(names(&order), ["Mod B", "Mod A"]);
        assert!(order.saved);
        assert_eq!(order.loaded[0].why, "in the saved load order");
        let conf = order.conf.unwrap();
        assert!(conf.find("Mod B").unwrap() < conf.find("Mod A").unwrap());
    }

    #[test]
    fn a_patch_for_a_mod_loads_after_it() {
        // Issue case 2: patch B for mod A: A, B.
        let world = World::new();
        let a = world.add("a", "Mod A", &[]);
        let b = world.add("b", "Patch B", &[]);
        world.keep(&b, &[&a]);
        assert_eq!(names(&world.order(&b)), ["Mod A"]);
        assert_eq!(world.order(&b).describe(), "Load order: Mod A, then Patch B.");
    }

    #[test]
    fn a_mod_on_four_mods_loads_them_in_the_descriptors_order_without_a_saved_list() {
        // Issue case 3: E depends on A, B, C, D.
        let world = World::new();
        for (file, name) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D")] {
            world.add(file, name, &[]);
        }
        let e = world.add("e", "E", &["A", "B", "C", "D"]);
        let order = world.order(&e);
        assert_eq!(names(&order), ["A", "B", "C", "D"]);
        assert!(!order.saved);
        assert_eq!(order.loaded[0].why, "needed by \"E\"");
    }

    #[test]
    fn the_saved_order_wins_over_the_order_of_the_descriptor() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let b = world.add("b", "B", &[]);
        let e = world.add("e", "E", &["A", "B"]);
        world.keep(&e, &[&b, &a]);
        assert_eq!(names(&world.order(&e)), ["B", "A"]);
        // Without the saved list the descriptor decides.
        let plain = resolve(&world.state(), &world.tmp, &world.all(), &e, false).unwrap();
        assert_eq!(names(&plain), ["A", "B"]);
    }

    #[test]
    fn what_a_saved_mod_needs_loads_before_it() {
        let world = World::new();
        let base = world.add("base", "Base", &[]);
        let lib = world.add("lib", "Library", &["Base"]);
        let main = world.add("main", "Main", &[]);
        let _ = base;
        world.keep(&main, &[&lib]);
        assert_eq!(names(&world.order(&main)), ["Base", "Library"]);
    }

    #[test]
    fn dependencies_are_reported_as_listed_automatic_or_missing() {
        let world = World::new();
        let base = world.add("base", "Base", &[]);
        let lib = world.add("lib", "Library", &["Base", "Not Installed"]);
        let main = world.add("main", "Main", &["Library", "Main"]);
        world.keep(&main, &[&base]);
        let deps = world.order(&main).dependencies;
        let state = |name: &str| deps.iter().find(|d| d.name == name).map(|d| d.state);
        assert_eq!(state("Library"), Some("automatic"));
        assert_eq!(state("Base"), Some("listed"));
        assert_eq!(state("Not Installed"), Some("missing"));
        // The mod itself is never its own dependency.
        assert_eq!(state("Main"), None);
        let missing = deps.iter().find(|d| d.state == "missing").unwrap();
        assert_eq!(missing.needed_by, "Library");
        assert!(missing.mod_file.is_none());
        assert_eq!(deps.iter().find(|d| d.name == "Library").unwrap().mod_file, Some(lib));
    }

    #[test]
    fn the_list_survives_a_round_trip_and_ignores_the_mod_itself_and_repeats() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let main = world.add("main", "Main", &[]);
        world.keep(&main, &[&a, &main, &a]);
        let saved = load(&world.state(), &main).unwrap();
        assert_eq!(saved.requires.len(), 1);
        assert_eq!(saved.requires[0].name, "A");
        assert_eq!(saved.name, "Main");
        // The file is named after the mod's folder, so it is the same on every call.
        assert_eq!(path(&world.state(), &main), path(&world.state(), &main));
        assert_ne!(path(&world.state(), &main), path(&world.state(), &a));
        assert_eq!(list(&world.state()).len(), 1);
    }

    #[test]
    fn an_empty_list_forgets_the_record() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let main = world.add("main", "Main", &[]);
        world.keep(&main, &[&a]);
        assert!(path(&world.state(), &main).is_file());
        world.keep(&main, &[]);
        assert!(load(&world.state(), &main).is_none());
        assert!(!clear(&world.state(), &main).unwrap());
    }

    #[test]
    fn a_mod_that_moved_is_found_by_its_name_and_one_that_is_gone_is_reported() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let gone = world.add("gone", "Gone", &[]);
        let main = world.add("main", "Main", &[]);
        world.keep(&main, &[&a, &gone]);
        // `A` is installed again under another file; `Gone` is removed.
        fs::remove_file(&a).unwrap();
        world.add("a2", "A", &[]);
        fs::remove_file(&gone).unwrap();
        let order = world.order(&main);
        assert_eq!(names(&order), ["A"]);
        assert!(order.loaded[0].mod_file.ends_with("a2.mod"));
        assert_eq!(order.unresolved.len(), 1);
        assert_eq!(order.unresolved[0].name, "Gone");
        assert!(order.describe().contains("Not loaded: Gone"));
    }

    #[test]
    fn a_missing_dependency_is_reported() {
        let world = World::new();
        let main = world.add("main", "Main", &["Nowhere"]);
        let order = world.order(&main);
        assert_eq!(order.loaded.len(), 0);
        assert_eq!(order.unresolved[0].name, "Nowhere");
    }

    #[test]
    fn the_config_of_the_mod_loads_first() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let b = world.add("b", "B", &[]);
        let main = world.add("main", "Main", &["A"]);
        fs::write(
            world.tmp.join("mod").join("main").join("ck3-tiger.conf"),
            format!(
                "load_mod = {{ label = \"A\" modfile = \"{}\" }}\n",
                a.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
        world.keep(&main, &[&b]);
        let order = world.order(&main);
        assert_eq!(names(&order), ["A", "B"]);
        assert_eq!(order.loaded[0].why, "loaded by the config's load_mod");
        // A dependency the config already loads is said to be loaded by the config.
        assert_eq!(order.dependencies.len(), 1);
        assert_eq!(order.dependencies[0].state, "configured");
    }

    const BASE_CONF: &str = "languages = { check = \"english\" }\n";

    #[test]
    fn the_order_is_written_to_the_config_once_and_replaced_not_repeated() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let b = world.add("b", "B", &[]);
        let main = world.add("main", "Main", &[]);
        let conf = world.tmp.join("mod").join("main").join("ck3-tiger.conf");
        fs::write(&conf, BASE_CONF).unwrap();
        world.keep(&main, &[&b, &a]);
        let written = export_conf(&world.state(), &world.tmp, &world.all(), &main).unwrap();
        assert_eq!(written, conf);
        let text = fs::read_to_string(&conf).unwrap();
        assert!(text.starts_with(BASE_CONF));
        assert_eq!(text.matches("load_mod").count(), 2);
        assert!(text.find("\"B\"").unwrap() < text.find("\"A\"").unwrap());
        let backup = world.tmp.join("mod").join("main").join("ck3-tiger.conf.xtiger-backup");
        assert_eq!(fs::read_to_string(backup).unwrap(), BASE_CONF);
        // Again, in another order: the block is replaced, not added to.
        world.keep(&main, &[&a, &b]);
        export_conf(&world.state(), &world.tmp, &world.all(), &main).unwrap();
        let text = fs::read_to_string(&conf).unwrap();
        assert_eq!(text.matches("load_mod").count(), 2);
        assert!(text.find("\"A\"").unwrap() < text.find("\"B\"").unwrap());
        assert_eq!(text.matches("xTiger load order: begin").count(), 1);
        // With nothing saved the block goes away and the rest stays.
        world.keep(&main, &[]);
        export_conf(&world.state(), &world.tmp, &world.all(), &main).unwrap();
        assert_eq!(fs::read_to_string(&conf).unwrap(), BASE_CONF);
    }

    #[test]
    fn our_own_block_does_not_make_a_dependency_loaded_by_the_config() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let main = world.add("main", "Main", &["A"]);
        let before = world.order(&main);
        assert_eq!(before.dependencies[0].state, "automatic");
        export_conf(&world.state(), &world.tmp, &world.all(), &main).unwrap();
        let conf = world.tmp.join("mod").join("main").join("ck3-tiger.conf");
        assert!(fs::read_to_string(&conf).unwrap().contains("load_mod"));
        // The lines xTiger wrote are not the mod's own config: the view reads the same as before.
        let after = world.order(&main);
        assert_eq!(after.dependencies[0].state, "automatic");
        assert_eq!(names(&after), ["A"]);
        assert_eq!(after.loaded[0].why, before.loaded[0].why);
        assert!(!after.loaded[0].why.starts_with("loaded by the config"));
        let _ = a;
    }

    #[test]
    fn a_hand_written_load_mod_is_left_alone_when_exporting() {
        let world = World::new();
        let a = world.add("a", "A", &[]);
        let b = world.add("b", "B", &[]);
        let main = world.add("main", "Main", &[]);
        let conf = world.tmp.join("mod").join("main").join("ck3-tiger.conf");
        let hand = format!(
            "load_mod = {{ label = \"A\" modfile = \"{}\" }}\n",
            a.display().to_string().replace('\\', "/")
        );
        fs::write(&conf, &hand).unwrap();
        world.keep(&main, &[&b]);
        export_conf(&world.state(), &world.tmp, &world.all(), &main).unwrap();
        let text = fs::read_to_string(&conf).unwrap();
        assert!(text.starts_with(&hand));
        assert_eq!(text.matches("load_mod").count(), 2);
        assert_eq!(strip_conf_block(&text).trim_end(), hand.trim_end());
        assert_eq!(strip_conf_block("no block here\n"), "no block here\n");
    }
}
