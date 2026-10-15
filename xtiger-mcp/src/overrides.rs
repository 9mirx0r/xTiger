//! Mods that override the base game's script: which of a mod's definitions replace or shadow a
//! definition of the game, and how they differ from the one the game has installed now.
//!
//! A mod file with the same path as a game file replaces the whole file. A mod definition with the
//! same name as a game definition in another file of the same folder replaces that definition.
//! Either way the mod keeps its copy when the game is patched, so a copy made for an older version
//! silently hides what the new version added. Only the installed game is available, so a
//! difference is either something the mod meant to change or something the game changed since.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::vanilla::depth_change;

/// The folders of a mod that hold script the game merges by name or by path.
pub(crate) const FOLDERS: [&str; 3] = ["common", "events", "history"];
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
    /// The game version the mod was written for, as a CK3 install folder (the one that holds
    /// `game/`). With it, each difference is told apart as the mod's change or the game's.
    pub base: Option<&'a Path>,
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
    /// Who changed what since the old game version. Only with a `base`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebase: Option<Rebase>,
}

/// How a mod's copy of a definition relates to the old game version it was made from (`base`)
/// and to the game now.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Rebase {
    /// `keep_mod`: only the mod changed it. `take_game`: only the game changed it, so the mod
    /// holds an old copy. `combine`: both changed it in different places. `review`: both changed
    /// the same place. `no_old_definition`: the old game version did not have it.
    /// Never applied by xTiger: each one is a suggestion for the person or assistant to check.
    pub verdict: &'static str,
    /// The changes the mod made to the old game's definition, as `line N: - old / + new`.
    pub mod_changes: Vec<String>,
    /// The changes the game made since, in the same form.
    pub game_changes: Vec<String>,
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
    /// For events: which definition the game uses, by `id_override_priority`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
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
    /// The old game folder the differences were measured against, or `unavailable`.
    pub baseline: String,
    pub notes: Vec<&'static str>,
}

/// Folders whose same-key definitions are merged with the game's instead of replacing them, so
/// a mod redefining a key there is not an override.
const MERGED_FOLDERS: [&str; 4] =
    ["common/on_action", "common/defines", "history/titles", "history/provinces"];

const NOTES: [&str; 5] = [
    "Only the game installed now is compared: a difference is either the mod's own change or a change the game made after the mod was written. Near copies (high similarity, not 100) are the likeliest to be stale.",
    "In a same-path file the mod's version replaces the whole game file, so every name in missing_from_mod is gone from the game.",
    "common/on_action, common/defines, history/titles and history/provinces merge with the game's files, so only same-path files are compared there.",
    "`- ` diff lines are only in the game, `+ ` lines only in the mod. Comments and spacing are ignored.",
    "With `base` (the game version the mod was written for) each difference has a `rebase` verdict: keep_mod, take_game, combine or review. They are suggestions; nothing is applied, and a definition the old game did not have is no_old_definition. Without `base` nothing says which side made a difference.",
];

/// The `id_override_priority` of an event definition, 0 when it has none.
fn event_priority(def: &Def) -> i64 {
    def.body
        .iter()
        .find_map(|line| line.split_once("id_override_priority = ").map(|(_, rest)| rest))
        .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// Which of two definitions of an event the game uses.
fn priority_outcome(modded: &Def, game: &Def) -> String {
    let (m, g) = (event_priority(modded), event_priority(game));
    match m.cmp(&g) {
        std::cmp::Ordering::Greater => format!("mod {m} beats game {g}: the mod's event is used"),
        std::cmp::Ordering::Less => {
            format!("game {g} beats mod {m}: the game's event is used and the mod's is ignored")
        }
        std::cmp::Ordering::Equal => format!(
            "both {m}: the game logs an error and uses the mod's event; give the mod's a higher id_override_priority"
        ),
    }
}

/// One top-level definition: `key = { ... }`, with its lines stripped of comments and spacing.
#[derive(Debug, Clone)]
pub(crate) struct Def {
    pub(crate) key: String,
    pub(crate) line: usize,
    pub(crate) body: Vec<String>,
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
pub(crate) fn definitions(text: &str) -> Vec<Def> {
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

/// Exact top-level block keys, independent of line layout. The diff parser above intentionally
/// works with whole lines, so it cannot be used to reject a bookmark before launching the game.
fn contains_definition(text: &str, key: &str) -> bool {
    static TOKENS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r##"#[^\r\n]*|"(?:\\.|[^"\\])*"|[{}=]|[^\s{}="#]+"##).unwrap());
    let mut depth = 0usize;
    let mut previous = ["", ""];
    for token in TOKENS.find_iter(text.trim_start_matches('\u{feff}')).map(|m| m.as_str()) {
        if token.starts_with('#') {
            continue;
        }
        match token {
            "{" => {
                if depth == 0 && previous[1] == "=" && previous[0].trim_matches('"') == key {
                    return true;
                }
                depth += 1;
            }
            "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        previous = [previous[1], token];
    }
    false
}

/// Whether a script folder contains a top-level definition with this exact key.
pub(crate) fn has_definition(dir: &Path, key: &str) -> bool {
    walk(dir, dir).into_iter().any(|(_, path)| {
        fs::read(path).is_ok_and(|bytes| contains_definition(&String::from_utf8_lossy(&bytes), key))
    })
}

/// The `.txt` files under `dir`, as `(path relative to base with forward slashes, full path)`.
pub(crate) fn walk(dir: &Path, base: &Path) -> Vec<(String, PathBuf)> {
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

/// A changed stretch of the old lines: `old[start..end]` became `new`.
#[derive(Debug, PartialEq, Eq)]
struct Hunk {
    start: usize,
    end: usize,
    new: Vec<String>,
}

impl Hunk {
    fn touches(&self, other: &Hunk) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    fn describe(&self, old: &[String]) -> String {
        let mut text = format!("line {}:", self.start + 1);
        for line in &old[self.start..self.end] {
            let _ = write!(text, " - {line}");
        }
        for line in &self.new {
            let _ = write!(text, " + {line}");
        }
        text
    }
}

/// Where `new` differs from `old`, as stretches of `old`. Too large to compare line by line:
/// the whole of it.
fn hunks(old: &[String], new: &[String]) -> Vec<Hunk> {
    let (n, m) = (old.len(), new.len());
    if n.saturating_mul(m) > MAX_CELLS {
        return if old == new {
            Vec::new()
        } else {
            vec![Hunk { start: 0, end: n, new: new.to_vec() }]
        };
    }
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
    let (mut i, mut j) = (0, 0);
    let mut found = Vec::new();
    let mut open: Option<Hunk> = None;
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            found.extend(open.take());
            i += 1;
            j += 1;
            continue;
        }
        let hunk = open.get_or_insert(Hunk { start: i, end: i, new: Vec::new() });
        if j >= m || (i < n && table[at(i + 1, j)] >= table[at(i, j + 1)]) {
            i += 1;
            hunk.end = i;
        } else {
            hunk.new.push(new[j].clone());
            j += 1;
        }
    }
    found.extend(open);
    found
}

/// Who changed a definition since the old game version `base`: the mod (`new`), the game now
/// (`old`), or both.
fn rebase(base: Option<&Def>, old: &Def, new: &Def, shown: usize) -> Rebase {
    let Some(base) = base else {
        return Rebase {
            verdict: "no_old_definition",
            mod_changes: Vec::new(),
            game_changes: Vec::new(),
        };
    };
    let (mine, theirs) = (hunks(&base.body, &new.body), hunks(&base.body, &old.body));
    let list = |found: &[Hunk]| {
        let mut lines: Vec<String> =
            found.iter().take(shown).map(|h| h.describe(&base.body)).collect();
        if found.len() > shown {
            lines.push(format!("… ({} more changes)", found.len() - shown));
        }
        lines
    };
    let verdict = if mine.is_empty() {
        "take_game"
    } else if theirs.is_empty() {
        "keep_mod"
    } else if mine.iter().any(|a| theirs.iter().any(|b| a.touches(b))) {
        "review"
    } else {
        "combine"
    };
    Rebase { verdict, mod_changes: list(&mine), game_changes: list(&theirs) }
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
    BlockDiff { key: key.to_owned(), similarity, removed, added, diff: lines, rebase: None }
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
pub(crate) fn category(rel: &str) -> String {
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
    let base_game = match req.base {
        Some(base) if base.join("game").is_dir() => Some(base.join("game")),
        Some(base) => {
            return Err(format!(
                "The old game folder {} has no game/ folder. Give the folder that holds game/.",
                base.display()
            ));
        }
        None => None,
    };
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
    let mut base_index: HashMap<String, HashMap<String, Def>> = HashMap::new();

    for (rel, path) in &mod_files {
        let mod_defs = read_defs(path);
        let game_file = game.join(rel);
        if game_file.is_file() {
            same_path_files += 1;
            let game_defs = read_defs(&game_file);
            let game_last = last_of_each(&game_defs);
            let game_by_key: HashMap<&str, &Def> =
                game_last.iter().map(|def| (def.key.as_str(), *def)).collect();
            let base_defs = base_game.as_ref().map(|base| read_defs(&base.join(rel)));
            let base_by_key: HashMap<&str, &Def> = base_defs
                .iter()
                .flat_map(|defs| last_of_each(defs))
                .map(|def| (def.key.as_str(), def))
                .collect();
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
                        let mut diff = compare(&def.key, old, def, req.diff_lines);
                        if base_game.is_some() {
                            let old_def = base_by_key.get(def.key.as_str()).copied();
                            diff.rebase = Some(rebase(old_def, old, def, req.diff_lines));
                        }
                        file.changed.push(diff);
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
        let base_by_key = base_game.as_ref().map(|base| {
            base_index.entry(folder.clone()).or_insert_with(|| {
                let mut by_key = HashMap::new();
                for (_, path) in walk(&base.join(&folder), base) {
                    for def in read_defs(&path) {
                        by_key.insert(def.key.clone(), def);
                    }
                }
                by_key
            })
        });
        for def in last_of_each(&mod_defs) {
            let Some((game_rel, old)) = by_key.get(&def.key) else { continue };
            same_key_definitions += 1;
            if old.body == def.body {
                same_key_identical += 1;
                continue;
            }
            let mut diff = compare(&def.key, old, def, req.diff_lines);
            if let Some(base_by_key) = &base_by_key {
                diff.rebase = Some(rebase(base_by_key.get(&def.key), old, def, req.diff_lines));
            }
            same_key.push(KeyOverride {
                file: rel.clone(),
                line: def.line,
                overrides: game_rel.clone(),
                priority: (folder == "events").then(|| priority_outcome(def, old)),
                diff,
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
        baseline: req.base.map_or_else(|| "unavailable".to_owned(), |b| b.display().to_string()),
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
            base: None,
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
            base: None,
        })
        .unwrap();
        assert_eq!(only.mod_files_checked, 1);
        assert_eq!(only.same_path_files, 0);
    }

    #[test]
    fn event_overrides_say_which_priority_wins() {
        let tmp = TempDir::new();
        let game = tmp.join("game");
        let modded = tmp.join("mod");
        write(
            &game,
            "game/events/00_a.txt",
            "namespace = t
t.1 = { hidden = yes }
t.2 = { hidden = yes }
t.3 = {
	id_override_priority = 5
}
",
        );
        write(
            &modded,
            "events/mine.txt",
            "namespace = t
t.1 = { id_override_priority = 2 }
t.2 = { hidden = no }
t.3 = {
	id_override_priority = 1
}
",
        );
        let report = compare_with_game(&Request {
            install: &game,
            mod_dir: &modded,
            path: "",
            diff_lines: 5,
            limit: 10,
            base: None,
        })
        .unwrap();
        let outcome = |key: &str| {
            report.same_key.iter().find(|o| o.diff.key == key).and_then(|o| o.priority.clone())
        };
        assert_eq!(outcome("t.1").unwrap(), "mod 2 beats game 0: the mod's event is used");
        assert!(outcome("t.2").unwrap().starts_with("both 0: the game logs an error"));
        assert!(outcome("t.3").unwrap().starts_with("game 5 beats mod 1"));
    }

    #[test]
    fn three_way_tells_the_mods_change_from_the_games() {
        let tmp = TempDir::new();
        let old_game = tmp.join("old");
        let game = tmp.join("game");
        let modded = tmp.join("mod");
        let event = |lines: &[&str]| {
            format!(
                "t.1 = {{
{}
}}
",
                lines.join(
                    "
"
                )
            )
        };
        let put = |root: &Path, name: &str, lines: &[&str]| {
            write(root, &format!("game/common/decisions/{name}.txt"), &event(lines));
        };
        // The mod added a reward; the game then added a guard far from it: combine.
        put(&old_game, "a", &["trigger = { a = 1 }", "x = 1", "x = 2", "x = 3", "reward = 1"]);
        put(&game, "a", &["trigger = { a = 1 dlc = 1 }", "x = 1", "x = 2", "x = 3", "reward = 1"]);
        write(
            &modded,
            "common/decisions/a.txt",
            &event(&["trigger = { a = 1 }", "x = 1", "x = 2", "x = 3", "reward = 1", "extra = 1"]),
        );
        // Both changed the same condition: review.
        put(&old_game, "b", &["limit = { a = 1 }"]);
        put(&game, "b", &["limit = { a = 2 }"]);
        write(&modded, "common/decisions/b.txt", &event(&["limit = { a = 3 }"]));
        // Only the game changed it: the mod holds an old copy.
        put(&old_game, "c", &["v = 1"]);
        put(&game, "c", &["v = 2"]);
        write(&modded, "common/decisions/c.txt", &event(&["v = 1"]));
        // Only the mod changed it.
        put(&old_game, "d", &["v = 1"]);
        put(&game, "d", &["v = 1"]);
        write(&modded, "common/decisions/d.txt", &event(&["v = 9"]));
        let install = tmp.join("cur");
        fs::create_dir_all(&install).unwrap();
        for name in ["a", "b", "c", "d"] {
            let text =
                fs::read_to_string(game.join(format!("game/common/decisions/{name}.txt"))).unwrap();
            write(&install, &format!("game/common/decisions/{name}.txt"), &text);
        }
        let go = |base: Option<&Path>| {
            compare_with_game(&Request {
                install: &install,
                mod_dir: &modded,
                path: "",
                diff_lines: 10,
                limit: 10,
                base,
            })
            .unwrap()
        };
        let with = go(Some(&old_game));
        let verdict = |name: &str| {
            let file = with.same_path.iter().find(|f| f.file.ends_with(&format!("/{name}.txt")));
            file.unwrap().changed[0].rebase.as_ref().unwrap().verdict
        };
        assert_eq!(verdict("a"), "combine");
        assert_eq!(verdict("b"), "review");
        assert_eq!(verdict("c"), "take_game");
        assert_eq!(verdict("d"), "keep_mod");
        let combined = with.same_path.iter().find(|f| f.file.ends_with("/a.txt")).unwrap();
        let rebase = combined.changed[0].rebase.as_ref().unwrap();
        assert_eq!(rebase.mod_changes.len(), 1);
        assert!(rebase.mod_changes[0].contains("+ extra = 1"), "{rebase:?}");
        assert!(rebase.game_changes[0].contains("dlc = 1"), "{rebase:?}");
        assert_eq!(with.baseline, old_game.display().to_string());
        // Without the old game nothing says who changed what.
        let without = go(None);
        assert_eq!(without.baseline, "unavailable");
        assert!(without.same_path.iter().all(|f| f.changed.iter().all(|c| c.rebase.is_none())));
        // An old game folder without game/ is an error, not a silent skip.
        assert!(
            compare_with_game(&Request {
                install: &install,
                mod_dir: &modded,
                path: "",
                diff_lines: 5,
                limit: 5,
                base: Some(&tmp.join("nope")),
            })
            .unwrap_err()
            .contains("no game/ folder")
        );
    }

    #[test]
    fn missing_folders_are_errors() {
        let tmp = TempDir::new();
        let request = |install: &Path, mod_dir: &Path| {
            compare_with_game(&Request {
                install,
                mod_dir,
                path: "",
                diff_lines: 5,
                limit: 5,
                base: None,
            })
        };
        assert!(request(&tmp, &tmp).unwrap_err().contains("no game/ folder"));
        fs::create_dir_all(tmp.join("game")).unwrap();
        assert!(request(&tmp, &tmp.join("nope")).unwrap_err().contains("does not exist"));
    }
}
