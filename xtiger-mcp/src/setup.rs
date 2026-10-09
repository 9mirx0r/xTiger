//! What the validator loads besides the mod it checks: the mods it depends on, and any mods the
//! caller asks for. Without them everything those mods define is reported as missing.
//!
//! The validator takes extra mods from `load_mod` blocks in a `ck3-tiger.conf`. [`plan`] works out
//! which mods to load and writes that config as text; it touches no files, so it can be tested
//! without running the validator.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::mods::{self, ModInfo};

/// The validator numbers the mods it loads in a `u8`; this keeps well under that, and under what
/// a check can load in reasonable time.
const MAX_LOADED: usize = 32;

/// A mod the validator loads besides the one it checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loaded {
    pub name: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    /// Why it is loaded: what needs it, or that it was asked for.
    pub why: String,
}

/// A dependency that could not be loaded.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unresolved {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Setup {
    /// The mods to load, dependencies before the mods that need them.
    pub loaded: Vec<Loaded>,
    pub unresolved: Vec<Unresolved>,
    pub warnings: Vec<String>,
    /// The config to give the validator, when mods are added to it. Without it the validator
    /// uses the config it finds by itself.
    #[serde(skip)]
    pub conf: Option<String>,
}

#[derive(Debug)]
pub struct Request<'a> {
    /// The `.mod` file that is checked.
    pub mod_file: &'a Path,
    /// A config given by the caller. It replaces the mod's own `ck3-tiger.conf`.
    pub config: Option<&'a Path>,
    /// Mods to load besides its dependencies.
    pub with: &'a [PathBuf],
    pub load_dependencies: bool,
    /// The Paradox folder, where Workshop mods have their `.mod` files.
    pub paradox: &'a Path,
}

/// Work out what to load. `all` is every mod that was found.
pub fn plan(request: &Request, all: &[ModInfo]) -> Result<Setup, String> {
    let main_file = mods::clean(request.mod_file);
    let main = mods::describe(&main_file)
        .ok_or_else(|| format!("Cannot read {}", request.mod_file.display()))?;

    // The config the validator would use, with its relative paths made absolute: it is
    // written to another folder, where they would no longer lead anywhere.
    let base = match request.config {
        Some(config) => Some(config.to_path_buf()),
        None => Some(main.dir.join("ck3-tiger.conf")).filter(|path| path.is_file()),
    };
    let (base_text, existing) = match base {
        Some(path) => {
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("Cannot read the config {}: {e}", path.display()))?;
            rewrite_conf(&text, path.parent().unwrap_or(Path::new(".")), request.paradox)
        }
        None => (String::new(), Vec::new()),
    };
    // What the config already loads is not loaded again, by file or by name.
    let existing_names: HashSet<String> = existing
        .iter()
        .filter_map(|file| mods::describe(file))
        .map(|info| info.name.to_lowercase())
        .collect();

    let mut walk = Walk {
        all,
        main_file: main_file.clone(),
        main_name: main.name.to_lowercase(),
        provided: Vec::new(),
        visited: HashSet::from([main_file]),
        existing: existing.into_iter().collect(),
        existing_names,
        load_dependencies: request.load_dependencies,
        setup: Setup::default(),
    };

    for file in request.with {
        let file = mods::clean(file);
        match mods::describe(&file) {
            Some(info) => walk.provided.push(info),
            None => walk
                .setup
                .warnings
                .push(format!("Cannot read {}, so it is not loaded.", file.display())),
        }
    }
    if request.load_dependencies {
        for name in &main.dependencies {
            walk.dependency(name, &main.name);
        }
    }
    for info in walk.provided.clone() {
        walk.add(info, "asked for with `with`".to_owned());
    }

    let mut setup = walk.setup;
    if !setup.loaded.is_empty() {
        let mut conf = base_text;
        if !conf.is_empty() && !conf.ends_with('\n') {
            conf.push('\n');
        }
        conf.push_str("\n# Added by xTiger: the mods this check loads.\n");
        let mut labels = HashSet::new();
        let mut skipped = Vec::new();
        for (n, loaded) in setup.loaded.iter().enumerate() {
            let file = loaded.mod_file.to_string_lossy().replace('\\', "/");
            if file.contains('"') {
                skipped.push(n);
                continue;
            }
            let label = unique_label(&loaded.name, &mut labels);
            let _ = writeln!(conf, "load_mod = {{ label = \"{label}\" modfile = \"{file}\" }}");
        }
        for &n in skipped.iter().rev() {
            let loaded = setup.loaded.remove(n);
            setup.warnings.push(format!(
                "{} has a quote in its path, which the validator's config cannot hold, so it is not loaded.",
                loaded.mod_file.display()
            ));
        }
        if !setup.loaded.is_empty() {
            setup.conf = Some(conf);
        }
    }
    Ok(setup)
}

struct Walk<'a> {
    all: &'a [ModInfo],
    main_file: PathBuf,
    main_name: String,
    /// The mods asked for by the caller, which also satisfy a dependency of the same name.
    provided: Vec<ModInfo>,
    visited: HashSet<PathBuf>,
    /// The `.mod` files the config already loads.
    existing: HashSet<PathBuf>,
    existing_names: HashSet<String>,
    load_dependencies: bool,
    setup: Setup,
}

/// Local copies come before added folders, which come before Workshop copies.
fn source_rank(source: &str) -> u8 {
    match source {
        "local" => 0,
        "added" => 1,
        _ => 2,
    }
}

impl Walk<'_> {
    /// The mod to load for a dependency called `name`.
    fn pick(&mut self, name: &str) -> Option<ModInfo> {
        let lower = name.to_lowercase();
        let mut seen = HashSet::new();
        let mut found: Vec<ModInfo> = self
            .provided
            .iter()
            .chain(self.all.iter())
            .filter(|info| info.name.to_lowercase() == lower)
            .map(|info| ModInfo { mod_file: mods::clean(&info.mod_file), ..info.clone() })
            .filter(|info| info.mod_file != self.main_file && seen.insert(info.mod_file.clone()))
            .collect();
        // A copy whose folder exists, then the provided one, then local before Workshop.
        let provided: HashSet<PathBuf> =
            self.provided.iter().map(|info| mods::clean(&info.mod_file)).collect();
        found.sort_by_key(|info| {
            (
                info.path_missing,
                !provided.contains(&info.mod_file),
                source_rank(info.source),
                info.mod_file.clone(),
            )
        });
        let mut found = found.into_iter();
        let chosen = found.next()?;
        let others: Vec<String> = found.map(|info| info.mod_file.display().to_string()).collect();
        if !others.is_empty() {
            self.setup.warnings.push(format!(
                "\"{name}\" is installed more than once. Loading {}; the others are {}. To choose another copy, pass it in `with`.",
                chosen.mod_file.display(),
                others.join(", ")
            ));
        }
        Some(chosen)
    }

    fn unresolved(&mut self, name: &str, reason: String) {
        if !self.setup.unresolved.iter().any(|u| u.name.eq_ignore_ascii_case(name)) {
            self.setup.unresolved.push(Unresolved { name: name.to_owned(), reason });
        }
    }

    fn dependency(&mut self, name: &str, wanted_by: &str) {
        let lower = name.to_lowercase();
        if lower == self.main_name || self.existing_names.contains(&lower) {
            return;
        }
        match self.pick(name) {
            None => self.unresolved(
                name,
                format!("no installed mod has this name (needed by \"{wanted_by}\")"),
            ),
            Some(info) => {
                let why = format!("needed by \"{wanted_by}\"");
                self.add(info, why);
            }
        }
    }

    /// Load `info`, after the mods it needs.
    fn add(&mut self, info: ModInfo, why: String) {
        let file = mods::clean(&info.mod_file);
        if self.existing.contains(&file) || !self.visited.insert(file.clone()) {
            return;
        }
        if info.path_missing {
            self.unresolved(
                &info.name,
                format!("installed, but its folder {} does not exist", info.dir.display()),
            );
            return;
        }
        if self.load_dependencies {
            for name in &info.dependencies {
                self.dependency(name, &info.name);
            }
        }
        if self.setup.loaded.len() >= MAX_LOADED {
            let warning = format!(
                "More than {MAX_LOADED} mods would be loaded, so \"{}\" and anything after it is left out.",
                info.name
            );
            if !self.setup.warnings.contains(&warning) {
                self.setup.warnings.push(warning);
            }
            return;
        }
        self.setup.loaded.push(Loaded { name: info.name, mod_file: file, why });
    }
}

/// A label for a `load_mod` block, which the validator shows with the reports of that mod.
fn unique_label(name: &str, used: &mut HashSet<String>) -> String {
    let mut label: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .take(40)
        .collect::<String>()
        .trim()
        .to_owned();
    if label.is_empty() {
        "mod".clone_into(&mut label);
    }
    let mut candidate = label.clone();
    let mut n = 2;
    while !used.insert(candidate.to_lowercase()) {
        candidate = format!("{label} {n}");
        n += 1;
    }
    candidate
}

/// Is `key` at `at` a whole word?
fn is_word_at(text: &str, at: usize, key: &str) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at + key.len()..].chars().next();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    !before.is_some_and(word) && !after.is_some_and(word)
}

/// Make the `modfile` paths of a config absolute, as `conf_dir` is where it was read, and list
/// the `.mod` files it loads.
fn rewrite_conf(text: &str, conf_dir: &Path, paradox: &Path) -> (String, Vec<PathBuf>) {
    let mut out = String::with_capacity(text.len());
    let mut existing = Vec::new();
    for line in text.split_inclusive('\n') {
        // Keep a comment as it is; a `#` inside quotes is not one.
        let mut in_quote = false;
        let cut = line
            .char_indices()
            .find(|&(_, c)| {
                if c == '"' {
                    in_quote = !in_quote;
                }
                c == '#' && !in_quote
            })
            .map_or(line.len(), |(at, _)| at);
        let (code, comment) = line.split_at(cut);
        out.push_str(&rewrite_line(code, conf_dir, paradox, &mut existing));
        out.push_str(comment);
    }
    (out, existing)
}

fn rewrite_line(
    code: &str,
    conf_dir: &Path,
    paradox: &Path,
    existing: &mut Vec<PathBuf>,
) -> String {
    let mut out = String::new();
    let mut rest = code;
    let key = "modfile";
    while let Some(at) =
        rest.match_indices(key).map(|(at, _)| at).find(|&at| is_word_at(rest, at, key))
    {
        let after = &rest[at + key.len()..];
        let value = after
            .trim_start()
            .strip_prefix('=')
            .map(str::trim_start)
            .and_then(|v| v.strip_prefix('"'))
            .and_then(|v| v.find('"').map(|end| (&v[..end], &v[end + 1..])));
        let Some((value, tail)) = value else {
            out.push_str(&rest[..at + key.len()]);
            rest = after;
            continue;
        };
        let path = PathBuf::from(value);
        if path.is_absolute() {
            existing.push(mods::clean(&path));
            out.push_str(&rest[..rest.len() - tail.len()]);
        } else {
            let absolute = mods::clean(&conf_dir.join(&path));
            existing.push(absolute.clone());
            out.push_str(&rest[..at]);
            let _ = write!(out, "{key} = \"{}\"", absolute.to_string_lossy().replace('\\', "/"));
        }
        rest = tail;
    }
    out.push_str(rest);
    // A Workshop id stands for the `.mod` file the launcher keeps for that mod.
    let id_key = "workshop_id";
    if let Some(at) =
        code.match_indices(id_key).map(|(at, _)| at).find(|&at| is_word_at(code, at, id_key))
        && let Some(value) = code[at + id_key.len()..].trim_start().strip_prefix('=')
    {
        let id: String = value
            .trim_start()
            .trim_start_matches('"')
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !id.is_empty() {
            existing.push(mods::clean(&paradox.join("mod").join(format!("ugc_{id}.mod"))));
        }
    }
    out
}

#[cfg(test)]
#[allow(unknown_lints, clippy::assert_is_empty)]
mod tests {
    use super::*;
    use crate::mods::{mod_dir, parse_dependencies};
    use crate::testing::TempDir;

    struct World {
        tmp: TempDir,
    }

    impl World {
        fn new() -> Self {
            let tmp = TempDir::new();
            fs::create_dir_all(tmp.join("mod")).unwrap();
            Self { tmp }
        }

        /// A mod called `name` in the Paradox `mod` folder, with its folder.
        fn add(&self, file: &str, name: &str, deps: &[&str]) -> PathBuf {
            let dir = self.tmp.join("mod").join(file);
            fs::create_dir_all(&dir).unwrap();
            self.add_without_folder(file, name, deps)
        }

        fn add_without_folder(&self, file: &str, name: &str, deps: &[&str]) -> PathBuf {
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

        fn plan(&self, main: &Path, with: &[PathBuf]) -> Setup {
            let all = mods::list(Some(&self.tmp), &[]);
            let request = Request {
                mod_file: main,
                config: None,
                with,
                load_dependencies: true,
                paradox: &self.tmp,
            };
            plan(&request, &all).unwrap()
        }
    }

    fn names(setup: &Setup) -> Vec<&str> {
        setup.loaded.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn reads_the_dependencies_of_a_mod_file() {
        let text = "\u{feff}name=\"X\"\ndependencies = {\n\t\"Community Flavor Pack\" # base\n\t\"Mod # one\"\n\t\"A=B\"\n}\ntags={ \"Events\" }\n";
        assert_eq!(
            parse_dependencies(text.trim_start_matches('\u{feff}')),
            ["Community Flavor Pack", "Mod # one", "A=B"]
        );
        assert_eq!(parse_dependencies("dependencies={\"A\" \"B\"}"), ["A", "B"]);
        assert!(parse_dependencies("name=\"dependencies = { \\\"A\\\" }\"").is_empty());
        assert!(parse_dependencies("# dependencies = { \"A\" }\nname=\"x\"").is_empty());
        assert!(parse_dependencies("my_dependencies = { \"A\" }").is_empty());
        assert!(parse_dependencies("name=\"x\"").is_empty());
    }

    #[test]
    fn finds_the_folder_the_validator_checks() {
        let dir = |file: &str, path: Option<&str>| mod_dir(Path::new(file), path);
        assert_eq!(dir("/p/mod/x.mod", Some("mod/x")), Path::new("/p").join("mod/x"));
        assert_eq!(dir("/elsewhere/x.mod", Some("x")), Path::new("/elsewhere").join("x"));
        assert_eq!(dir("/m/x/descriptor.mod", Some("ignored")), Path::new("/m/x"));
        assert_eq!(dir("/elsewhere/x.mod", None), Path::new("/elsewhere"));
    }

    #[test]
    fn dependencies_load_before_the_mods_that_need_them() {
        let world = World::new();
        world.add("d", "D", &[]);
        world.add("b", "B", &["D"]);
        world.add("c", "C", &["D"]);
        let main = world.add("a", "A", &["B", "C"]);
        let setup = world.plan(&main, &[]);
        assert_eq!(names(&setup), ["D", "B", "C"]);
        assert!(setup.unresolved.is_empty());
        let conf = setup.conf.unwrap();
        assert_eq!(conf.matches("load_mod").count(), 3);
        assert!(conf.find("\"D\"").unwrap() < conf.find("\"B\"").unwrap());
    }

    #[test]
    fn a_cycle_or_a_dependency_on_itself_is_harmless() {
        let world = World::new();
        world.add("b", "B", &["A"]);
        let main = world.add("a", "A", &["B", "a"]);
        let setup = world.plan(&main, &[]);
        assert_eq!(names(&setup), ["B"]);
    }

    #[test]
    fn a_missing_dependency_is_reported_not_ignored() {
        let world = World::new();
        let main = world.add("a", "A", &["Nowhere"]);
        let setup = world.plan(&main, &[]);
        assert!(setup.loaded.is_empty());
        assert!(setup.conf.is_none());
        assert_eq!(setup.unresolved[0].name, "Nowhere");
        assert!(setup.unresolved[0].reason.contains("no installed mod"));
    }

    #[test]
    fn a_dependency_whose_folder_is_missing_is_skipped() {
        let world = World::new();
        world.add_without_folder("b", "B", &[]);
        let main = world.add("a", "A", &["B"]);
        let setup = world.plan(&main, &[]);
        assert!(setup.conf.is_none());
        assert!(setup.unresolved[0].reason.contains("does not exist"));
    }

    #[test]
    fn two_copies_of_a_dependency_pick_one_and_say_so() {
        let world = World::new();
        world.add_without_folder("gone", "B", &[]);
        let workshop = world.tmp.join("mod/ugc_5.mod");
        fs::create_dir_all(world.tmp.join("mod/five")).unwrap();
        fs::write(&workshop, "name=\"B\"\npath=\"mod/five\"\n").unwrap();
        let main = world.add("a", "A", &["B"]);
        let setup = world.plan(&main, &[]);
        assert_eq!(setup.loaded[0].mod_file, workshop);
        assert!(setup.warnings[0].contains("installed more than once"));
    }

    #[test]
    fn a_mod_asked_for_satisfies_a_dependency_and_loads_once() {
        let world = World::new();
        world.add("b", "B", &[]);
        let other = world.add("b2", "B", &[]);
        let extra = world.add("x", "X", &[]);
        let main = world.add("a", "A", &["B"]);
        let setup = world.plan(&main, &[other.clone(), extra]);
        assert_eq!(names(&setup), ["B", "X"]);
        assert_eq!(setup.loaded[0].mod_file, other);
    }

    #[test]
    fn what_the_config_already_loads_is_not_loaded_again() {
        let world = World::new();
        let dep = world.add("b", "B", &[]);
        world.add("c", "C", &[]);
        let main = world.add("a", "A", &["B", "C"]);
        fs::write(
            world.tmp.join("mod/a/ck3-tiger.conf"),
            format!(
                "load_mod = {{ modfile = \"{}\" }} # B\n",
                dep.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
        let setup = world.plan(&main, &[]);
        assert_eq!(names(&setup), ["C"]);
        let conf = setup.conf.unwrap();
        assert_eq!(conf.matches("load_mod").count(), 2, "{conf}");
        assert!(conf.contains("# B"));
    }

    #[test]
    fn relative_paths_in_the_config_are_made_absolute() {
        let world = World::new();
        let dep = world.add("b", "B", &[]);
        world.add("c", "C", &[]);
        let main = world.add("a", "A", &["C"]);
        fs::write(
            world.tmp.join("mod/a/ck3-tiger.conf"),
            "languages = { check = \"english\" }\nload_mod = { label = \"B\" modfile = \"../b.mod\" }\n",
        )
        .unwrap();
        let setup = world.plan(&main, &[]);
        let conf = setup.conf.clone().unwrap();
        let expected = dep.display().to_string().replace('\\', "/");
        assert!(conf.contains(&format!("modfile = \"{expected}\"")), "{conf}");
        assert!(conf.contains("languages = { check = \"english\" }"));
        assert_eq!(names(&setup), ["C"]);
    }

    #[test]
    fn a_workshop_id_in_the_config_counts_as_loaded() {
        let world = World::new();
        let dep = world.tmp.join("mod/ugc_7.mod");
        fs::create_dir_all(world.tmp.join("mod/seven")).unwrap();
        fs::write(&dep, "name=\"B\"\npath=\"mod/seven\"\n").unwrap();
        let main = world.add("a", "A", &["B"]);
        fs::write(world.tmp.join("mod/a/ck3-tiger.conf"), "load_mod = { workshop_id = 7 }\n")
            .unwrap();
        assert!(world.plan(&main, &[]).conf.is_none());
    }

    #[test]
    fn labels_are_safe_and_unique() {
        let mut used = HashSet::new();
        assert_eq!(unique_label("Bad \"name\" \\ here", &mut used), "Bad name  here");
        assert_eq!(unique_label("Bad \"name\" \\ here", &mut used), "Bad name  here 2");
        assert_eq!(unique_label("\"\"", &mut used), "mod");
    }

    #[test]
    fn paths_with_spaces_and_accents_are_written_as_they_are() {
        let world = World::new();
        let dir = world.tmp.join("mod/Ménorá v2");
        fs::create_dir_all(&dir).unwrap();
        fs::write(world.tmp.join("mod/menora.mod"), "name=\"Menorá\"\npath=\"mod/Ménorá v2\"\n")
            .unwrap();
        let main = world.add("a", "A", &["Menorá"]);
        let conf = world.plan(&main, &[]).conf.unwrap();
        assert!(conf.contains("label = \"Menorá\""), "{conf}");
        assert!(!conf.contains('\\'));
    }

    #[test]
    fn no_dependencies_means_no_generated_config() {
        let world = World::new();
        world.add("b", "B", &[]);
        let main = world.add("a", "A", &["B"]);
        let all = mods::list(Some(&world.tmp), &[]);
        let request = Request {
            mod_file: &main,
            config: None,
            with: &[],
            load_dependencies: false,
            paradox: &world.tmp,
        };
        let setup = plan(&request, &all).unwrap();
        assert!(setup.conf.is_none() && setup.loaded.is_empty());
    }
}
