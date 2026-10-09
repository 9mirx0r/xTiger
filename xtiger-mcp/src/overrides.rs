//! Mods that override the base game's script: which of a mod's definitions replace or shadow a
//! definition of the game, and how they differ from the one the game has installed now.
//!
//! A mod file with the same path as a game file replaces the whole file. A mod definition with the
//! same name as a game definition in another file of the same folder replaces that definition.
//! Either way the mod keeps its copy when the game is patched, so a copy made for an older version
//! silently hides what the new version added. Only the installed game is available, so a
//! difference is either something the mod meant to change or something the game changed since.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::vanilla::depth_change;

/// The folders of a mod that hold script the game merges by name or by path.
const FOLDERS: [&str; 3] = ["common", "events", "history"];
/// Above this many cells the line diff falls back to counting lines.
const MAX_CELLS: usize = 4_000_000;

#[derive(Debug)]
pub struct Request<'a> {
    /// The CK3 install folder (the one that holds `game/`).
    pub install: &'a Path,
    pub mod_dir: &'a Path,
    /// Only mod files whose path, relative to the mod, contains this.
    pub path: &'a str,
    /// The most diff lines shown for one definition.
    pub diff_lines: usize,
    /// The most files, definitions and names listed in each part of the report.
    pub limit: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BlockDiff {
    pub key: String,
    /// How alike the two versions are, 0 to 100. A high value that is not 100 is a near copy.
    pub similarity: usize,
    /// Lines the game has now and the mod lacks.
    pub removed: usize,
    /// Lines the mod has and the game lacks.
    pub added: usize,
    /// `- ` lines are only in the game, `+ ` lines only in the mod.
    pub diff: Vec<String>,
}

/// A mod file with the path of a game file: the mod's version replaces the game's whole file.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FileOverride {
    pub file: String,
    pub identical_blocks: usize,
    pub changed_blocks: usize,
    pub changed: Vec<BlockDiff>,
    /// Definitions of the game's file that the mod's copy lacks. They are gone from the game.
    pub missing_from_mod: Vec<String>,
    pub only_in_mod: Vec<String>,
}

/// A mod definition with the name of a game definition in another file.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct KeyOverride {
    pub file: String,
    pub line: usize,
    pub overrides: String,
    #[serde(flatten)]
    pub diff: BlockDiff,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Report {
    pub mod_files_checked: usize,
    /// Files with the path of a game file, and how many of them are the same as the game's.
    pub same_path_files: usize,
    pub same_path_identical: usize,
    pub same_path: Vec<FileOverride>,
    /// Definitions that replace a game definition from another file, and how many are copies.
    pub same_key_definitions: usize,
    pub same_key_identical: usize,
    pub same_key: Vec<KeyOverride>,
    pub notes: Vec<&'static str>,
}

/// Folders whose same-key definitions are merged with the game's instead of replacing them, so
/// a mod redefining a key there is not an override.
const MERGED_FOLDERS: [&str; 4] =
    ["common/on_action", "common/defines", "history/titles", "history/provinces"];

const NOTES: [&str; 4] = [
    "Only the game installed now is compared: a difference is either the mod's own change or a change the game made after the mod was written. Near copies (high similarity, not 100) are the likeliest to be stale.",
    "In a same-path file the mod's version replaces the whole game file, so every name in missing_from_mod is gone from the game.",
    "common/on_action, common/defines, history/titles and history/provinces merge with the game's files, so only same-path files are compared there.",
    "`- ` diff lines are only in the game, `+ ` lines only in the mod. Comments and spacing are ignored.",
];

/// One top-level definition: `key = { ... }`, with its lines stripped of comments and spacing.
#[derive(Debug, Clone)]
struct Def {
    key: String,
    line: usize,
    body: Vec<String>,
}

/// The line without its comment, with the spacing between words made single.
fn code_part(line: &str) -> String {
    let mut quoted = false;
    let mut end = line.len();
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    line[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The top-level definitions of a script file. Values without a block (`namespace = x`,
/// `@variable = 1`) are not definitions.
fn definitions(text: &str) -> Vec<Def> {
    let mut defs = Vec::new();
    let mut current: Option<(Def, i32, bool)> = None;
    for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = code_part(raw);
        if line.is_empty() {
            continue;
        }
        let change = depth_change(&line);
        if let Some((def, depth, opened)) = current.as_mut() {
            def.body.push(line.clone());
            *depth += change;
            *opened |= line.contains('{');
            if *opened && *depth <= 0 {
                defs.extend(current.take().map(|(def, ..)| def));
            }
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else { continue };
        let (key, rest) = (key.trim(), rest.trim());
        if key.is_empty() || !(rest.is_empty() || rest.contains('{')) {
            continue;
        }
        let def = Def { key: key.to_owned(), line: i + 1, body: vec![line.clone()] };
        let opened = line.contains('{');
        if opened && change <= 0 {
            defs.push(def);
        } else {
            current = Some((def, change, opened));
        }
    }
    defs.extend(current.map(|(def, ..)| def));
    defs
}

fn read_defs(path: &Path) -> Vec<Def> {
    fs::read(path).map(|bytes| definitions(&String::from_utf8_lossy(&bytes))).unwrap_or_default()
}

/// The `.txt` files under `dir`, as `(path relative to base with forward slashes, full path)`.
fn walk(dir: &Path, base: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            // A symlink is not followed, so a link cannot lead the scan out of the folder.
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                todo.push(path);
            } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                found.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    found.sort();
    found
}

/// The lines in the longest common run of `old` and `new`, and the whole diff in order.
fn line_diff(old: &[String], new: &[String]) -> (usize, Vec<(char, String)>) {
    if old.len().saturating_mul(new.len()) > MAX_CELLS {
        let mut counts: HashMap<&str, i64> = HashMap::new();
        for line in old {
            *counts.entry(line).or_default() += 1;
        }
        let mut diff = Vec::new();
        let mut common = 0;
        for line in new {
            match counts.get_mut(line.as_str()) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    common += 1;
                }
                _ => diff.push(('+', line.clone())),
            }
        }
        for line in old {
            if let Some(n) = counts.get_mut(line.as_str())
                && *n > 0
            {
                *n -= 1;
                diff.push(('-', line.clone()));
            }
        }
        return (common, diff);
    }
    let (n, m) = (old.len(), new.len());
    let mut table = vec![0_u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if old[i] == new[j] {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j, mut common) = (0, 0, 0);
    let mut diff = Vec::new();
    while i < n && j < m {
        if old[i] == new[j] {
            common += 1;
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            diff.push(('-', old[i].clone()));
            i += 1;
        } else {
            diff.push(('+', new[j].clone()));
            j += 1;
        }
    }
    diff.extend(old[i..].iter().map(|line| ('-', line.clone())));
    diff.extend(new[j..].iter().map(|line| ('+', line.clone())));
    (common, diff)
}

/// How the mod's definition `new` differs from the game's definition `old`.
fn compare(key: &str, old: &Def, new: &Def, shown: usize) -> BlockDiff {
    let (common, diff) = line_diff(&old.body, &new.body);
    let total = old.body.len() + new.body.len();
    let similarity = (200 * common).checked_div(total).unwrap_or(100);
    let removed = diff.iter().filter(|(sign, _)| *sign == '-').count();
    let added = diff.len() - removed;
    let mut lines: Vec<String> =
        diff.iter().take(shown).map(|(sign, line)| format!("{sign} {line}")).collect();
    if diff.len() > shown {
        lines.push(format!("… ({} more changed lines)", diff.len() - shown));
    }
    BlockDiff { key: key.to_owned(), similarity, removed, added, diff: lines }
}

/// The last definition of each name, in the order the names first appear.
fn last_of_each(defs: &[Def]) -> Vec<&Def> {
    let mut last: HashMap<&str, usize> = HashMap::new();
    for (i, def) in defs.iter().enumerate() {
        last.insert(&def.key, i);
    }
    let mut seen = HashSet::new();
    defs.iter()
        .filter(|def| seen.insert(def.key.as_str()))
        .map(|def| &defs[last[def.key.as_str()]])
        .collect()
}

/// The folder whose definitions share one namespace: `common/decisions`, `events`.
fn category(rel: &str) -> String {
    let dir = rel.rsplit_once('/').map_or("", |(dir, _)| dir);
    dir.split('/').take(2).collect::<Vec<_>>().join("/")
}

/// Compare a mod's script with the game's.
///
/// # Errors
/// If the game or the mod folder is missing.
pub fn compare_with_game(req: &Request) -> Result<Report, String> {
    let game = req.install.join("game");
    if !game.is_dir() {
        return Err(format!("{} has no game/ folder.", req.install.display()));
    }
    if !req.mod_dir.is_dir() {
        return Err(format!("The mod folder {} does not exist.", req.mod_dir.display()));
    }
    let filter = req.path.trim().replace('\\', "/");
    let mod_files: Vec<(String, PathBuf)> = FOLDERS
        .iter()
        .flat_map(|folder| walk(&req.mod_dir.join(folder), req.mod_dir))
        .filter(|(rel, _)| filter.is_empty() || rel.contains(&filter))
        .collect();

    let mut same_path = Vec::new();
    let mut same_path_files = 0;
    let mut same_path_identical = 0;
    let mut same_key = Vec::new();
    let mut same_key_definitions = 0;
    let mut same_key_identical = 0;
    // The game's definitions by folder, read when the first mod file of that folder needs them.
    let mut index: HashMap<String, HashMap<String, (String, Def)>> = HashMap::new();

    for (rel, path) in &mod_files {
        let mod_defs = read_defs(path);
        let game_file = game.join(rel);
        if game_file.is_file() {
            same_path_files += 1;
            let game_defs = read_defs(&game_file);
            let game_last = last_of_each(&game_defs);
            let game_by_key: HashMap<&str, &Def> =
                game_last.iter().map(|def| (def.key.as_str(), *def)).collect();
            let mod_last = last_of_each(&mod_defs);
            let mod_keys: HashSet<&str> = mod_last.iter().map(|def| def.key.as_str()).collect();
            let mut file = FileOverride {
                file: rel.clone(),
                identical_blocks: 0,
                changed_blocks: 0,
                changed: Vec::new(),
                missing_from_mod: Vec::new(),
                only_in_mod: Vec::new(),
            };
            for def in &mod_last {
                match game_by_key.get(def.key.as_str()) {
                    None => file.only_in_mod.push(def.key.clone()),
                    Some(old) if old.body == def.body => file.identical_blocks += 1,
                    Some(old) => {
                        file.changed_blocks += 1;
                        file.changed.push(compare(&def.key, old, def, req.diff_lines));
                    }
                }
            }
            file.missing_from_mod = game_last
                .iter()
                .filter(|def| !mod_keys.contains(def.key.as_str()))
                .map(|def| def.key.clone())
                .collect();
            if file.changed_blocks == 0
                && file.missing_from_mod.is_empty()
                && file.only_in_mod.is_empty()
            {
                same_path_identical += 1;
            } else {
                same_path.push(file);
            }
            continue;
        }
        let folder = category(rel);
        if MERGED_FOLDERS.contains(&folder.as_str()) {
            continue;
        }
        let by_key = index.entry(folder.clone()).or_insert_with(|| {
            let mut by_key = HashMap::new();
            for (game_rel, game_path) in walk(&game.join(&folder), &game) {
                for def in read_defs(&game_path) {
                    by_key.insert(def.key.clone(), (game_rel.clone(), def));
                }
            }
            by_key
        });
        for def in last_of_each(&mod_defs) {
            let Some((game_rel, old)) = by_key.get(&def.key) else { continue };
            same_key_definitions += 1;
            if old.body == def.body {
                same_key_identical += 1;
                continue;
            }
            same_key.push(KeyOverride {
                file: rel.clone(),
                line: def.line,
                overrides: game_rel.clone(),
                diff: compare(&def.key, old, def, req.diff_lines),
            });
        }
    }

    // The files that lose the most from the game, and the near copies, first.
    same_path.sort_by(|a, b| {
        (b.missing_from_mod.len() + b.changed_blocks, &a.file)
            .cmp(&(a.missing_from_mod.len() + a.changed_blocks, &b.file))
    });
    same_key.sort_by(|a, b| b.diff.similarity.cmp(&a.diff.similarity).then(a.file.cmp(&b.file)));
    same_path.truncate(req.limit);
    for file in &mut same_path {
        file.changed.sort_by_key(|block| std::cmp::Reverse(block.similarity));
        file.changed.truncate(req.limit);
        file.missing_from_mod.truncate(req.limit.saturating_mul(5));
        file.only_in_mod.truncate(req.limit.saturating_mul(5));
    }
    same_key.truncate(req.limit);

    Ok(Report {
        mod_files_checked: mod_files.len(),
        same_path_files,
        same_path_identical,
        same_path,
        same_key_definitions,
        same_key_identical,
        same_key,
        notes: NOTES.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_top_level_blocks_only() {
        let defs = definitions(
            "\u{feff}namespace = x\n@var = 1\n# comment\na = {\n\tb = { c = 1 } # tail\n}\nd = { }\ne =\n{\n\tf = 1\n}\n",
        );
        let keys: Vec<&str> = defs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, ["a", "d", "e"]);
        assert_eq!(defs[0].body, ["a = {", "b = { c = 1 }", "}"]);
        assert_eq!(defs[0].line, 4);
        assert_eq!(defs[2].body.len(), 4);
    }

    #[test]
    fn diff_counts_lines_on_each_side() {
        let a: Vec<String> = ["x = {", "a = 1", "b = 2", "}"].map(String::from).to_vec();
        let b: Vec<String> = ["x = {", "a = 1", "c = 3", "d = 4", "}"].map(String::from).to_vec();
        let (common, diff) = line_diff(&a, &b);
        assert_eq!(common, 3);
        assert_eq!(
            diff,
            [('-', "b = 2".to_owned()), ('+', "c = 3".to_owned()), ('+', "d = 4".to_owned())]
        );
    }

    #[test]
    fn compares_a_mod_with_the_game() {
        let tmp = TempDir::new();
        let game = tmp.join("game");
        let modded = tmp.join("mod");
        write(
            &game,
            "game/common/decisions/00_a.txt",
            "keep = { a = 1 }\nlost = { b = 1 }\nchanged = {\n\tone = 1\n\ttwo = 2\n}\n",
        );
        write(
            &game,
            "game/common/decisions/00_b.txt",
            "shadowed = {\n\tx = 1\n\ty = 2\n}\ncopy = { z = 1 }\n",
        );
        write(
            &modded,
            "common/decisions/00_a.txt",
            "keep = { a = 1 }\nchanged = {\n\tone = 1\n\tthree = 3\n}\nnew_one = { q = 1 }\n",
        );
        write(
            &modded,
            "common/decisions/mine.txt",
            "shadowed = {\n\tx = 1\n\ty = 3\n}\ncopy = { z = 1 }\nbrand_new = { w = 1 }\n",
        );
        write(&modded, "common/decisions/same.txt", "# nothing\n");
        let report = compare_with_game(&Request {
            install: &game,
            mod_dir: &modded,
            path: "",
            diff_lines: 10,
            limit: 10,
        })
        .unwrap();
        assert_eq!(report.mod_files_checked, 3);
        assert_eq!(report.same_path_files, 1);
        assert_eq!(report.same_path.len(), 1);
        let file = &report.same_path[0];
        assert_eq!(file.file, "common/decisions/00_a.txt");
        assert_eq!(file.identical_blocks, 1);
        assert_eq!(file.missing_from_mod, ["lost"]);
        assert_eq!(file.only_in_mod, ["new_one"]);
        assert_eq!(file.changed[0].key, "changed");
        assert_eq!((file.changed[0].removed, file.changed[0].added), (1, 1));
        assert_eq!(file.changed[0].diff, ["- two = 2", "+ three = 3"]);
        assert_eq!(report.same_key_definitions, 2);
        assert_eq!(report.same_key_identical, 1);
        assert_eq!(report.same_key.len(), 1);
        assert_eq!(report.same_key[0].diff.key, "shadowed");
        assert_eq!(report.same_key[0].overrides, "common/decisions/00_b.txt");
        assert_eq!(report.same_key[0].line, 1);
        assert_eq!(report.same_key[0].diff.diff, ["- y = 2", "+ y = 3"]);

        let only = compare_with_game(&Request {
            install: &game,
            mod_dir: &modded,
            path: "mine",
            diff_lines: 10,
            limit: 10,
        })
        .unwrap();
        assert_eq!(only.mod_files_checked, 1);
        assert_eq!(only.same_path_files, 0);
    }

    #[test]
    fn missing_folders_are_errors() {
        let tmp = TempDir::new();
        let request = |install: &Path, mod_dir: &Path| {
            compare_with_game(&Request { install, mod_dir, path: "", diff_lines: 5, limit: 5 })
        };
        assert!(request(&tmp, &tmp).unwrap_err().contains("no game/ folder"));
        fs::create_dir_all(tmp.join("game")).unwrap();
        assert!(request(&tmp, &tmp.join("nope")).unwrap_err().contains("does not exist"));
    }
}
