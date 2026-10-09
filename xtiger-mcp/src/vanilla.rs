//! Searching the base game's own files: how the game defines a trigger, an event, a decision or a
//! localization key, so a mod can be brought in line with it.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
use serde::Serialize;

/// The folders searched unless `path` names another one. `gfx/` is mostly portraits and meshes.
const FOLDERS: [&str; 6] = ["common", "events", "gui", "history", "localization", "notifications"];
/// Only English localization, unless `path` asks for another language.
const LANGUAGE: &str = "english";
const EXTENSIONS: [&str; 3] = ["txt", "gui", "yml"];
/// The most lines shown of one definition.
const MAX_BLOCK: usize = 60;
/// The longest line shown for a pattern hit.
const MAX_LINE: usize = 240;

#[derive(Debug)]
pub struct Search<'a> {
    /// The name of a definition, such as `has_trait_rank`, `my_events.0001` or a localization key.
    pub name: &'a str,
    /// A case-insensitive regex on lines.
    pub pattern: &'a str,
    /// Only files whose path, relative to `game/`, contains this.
    pub path: &'a str,
    pub limit: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Hit {
    /// The path relative to the game's `game/` folder, with forward slashes.
    pub file: String,
    pub full_path: PathBuf,
    pub line: usize,
    /// The whole definition, or the matching line.
    pub text: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Found {
    pub matched: usize,
    pub hits: Vec<Hit>,
    pub files_searched: usize,
}

fn norm(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Whether to look inside a folder, given its path relative to `game/`. With a filter, every
/// folder is looked in, and the filter picks the files.
fn wanted_dir(rel: &str, filter: &str) -> bool {
    let mut parts = rel.split('/');
    let top = parts.next().unwrap_or("");
    if filter.is_empty() && !FOLDERS.contains(&top) {
        return false;
    }
    if top == "localization" && !filter.contains("localization/") {
        return parts.next().is_none_or(|lang| lang == LANGUAGE);
    }
    true
}

fn files(game: &Path, filter: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![game.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let rel = norm(path.strip_prefix(game).unwrap_or(&path));
            if path.is_dir() {
                if wanted_dir(&rel, filter) {
                    todo.push(path);
                }
            } else if path.extension().is_some_and(|ext| EXTENSIONS.iter().any(|e| ext == *e))
                && rel.contains('/')
                && (filter.is_empty() || rel.contains(filter))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// How far a line moves into or out of `{ }`, outside quotes and comments.
pub(crate) fn depth_change(line: &str) -> i32 {
    let mut change = 0;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => break,
            '{' if !quoted => change += 1,
            '}' if !quoted => change -= 1,
            _ => {}
        }
    }
    change
}

/// Whether `line` defines `name`: `name = ...` at the top level of a script file, `name:0 "..."` in
/// localization, or `type name =` / `template name` in a GUI file.
fn defines(line: &str, name: &str, kind: &str, depth: i32) -> bool {
    let trimmed = line.trim_start();
    match kind {
        "yml" => {
            trimmed.strip_prefix(name).and_then(|rest| rest.strip_prefix(':')).is_some_and(|rest| {
                rest.starts_with(|c: char| c.is_ascii_digit() || c == ' ' || c == '"')
            })
        }
        "gui" => {
            let mut words = trimmed.split(|c: char| c.is_whitespace() || c == '=' || c == '{');
            matches!(words.next(), Some("type" | "template"))
                && words.find(|w| !w.is_empty()) == Some(name)
        }
        _ => {
            depth == 0
                && trimmed.strip_prefix(name).is_some_and(|rest| rest.trim_start().starts_with('='))
        }
    }
}

/// The definition that starts at `lines[start]`, up to its closing brace.
fn block(lines: &[&str], start: usize) -> String {
    let mut depth = 0;
    let mut end = start;
    for (i, line) in lines.iter().enumerate().skip(start) {
        depth += depth_change(line);
        end = i;
        if depth <= 0 && (i > start || !line.contains('{')) {
            break;
        }
    }
    let shown = (end + 1 - start).min(MAX_BLOCK);
    let mut text = lines[start..start + shown].join("\n");
    if end + 1 - start > shown {
        let _ = write!(text, "\n… ({} more lines)", end + 1 - start - shown);
    }
    text
}

fn compile(pattern: &str) -> Result<Option<Regex>, String> {
    if pattern.trim().is_empty() {
        return Ok(None);
    }
    RegexBuilder::new(pattern.trim())
        .case_insensitive(true)
        .build()
        .map(Some)
        .map_err(|e| format!("pattern is not a valid regex: {e}"))
}

/// Search the `game/` folder of the CK3 install `install`.
pub fn search(install: &Path, search: &Search) -> Result<Found, String> {
    let name = search.name.trim();
    let pattern = compile(search.pattern)?;
    if name.is_empty() && pattern.is_none() {
        return Err("Give a name, a pattern, or both.".to_owned());
    }
    let game = install.join("game");
    let filter = search.path.trim().replace('\\', "/").trim_start_matches("game/").to_owned();
    let files = files(&game, &filter);
    let mut hits = Vec::new();
    let mut matched = 0;
    for path in &files {
        let Ok(bytes) = fs::read(path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        if !name.is_empty() && !text.contains(name) {
            continue;
        }
        if name.is_empty() && pattern.as_ref().is_some_and(|re| !re.is_match(&text)) {
            continue;
        }
        let kind =
            path.extension().map(|ext| ext.to_string_lossy().into_owned()).unwrap_or_default();
        let file = norm(path.strip_prefix(&game).unwrap_or(path));
        let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
        let mut depth = 0;
        for (i, line) in lines.iter().enumerate() {
            let found = if name.is_empty() {
                pattern.as_ref().is_some_and(|re| re.is_match(line)).then(|| {
                    let line = line.trim();
                    line.chars().take(MAX_LINE).collect::<String>()
                })
            } else if defines(line, name, &kind, depth) {
                let text = if kind == "yml" { line.trim().to_owned() } else { block(&lines, i) };
                pattern.as_ref().is_none_or(|re| re.is_match(&text)).then_some(text)
            } else {
                None
            };
            depth = (depth + depth_change(line)).max(0);
            if let Some(text) = found {
                matched += 1;
                if hits.len() < search.limit {
                    hits.push(Hit {
                        file: file.clone(),
                        full_path: path.clone(),
                        line: i + 1,
                        text,
                    });
                }
            }
        }
    }
    Ok(Found { matched, hits, files_searched: files.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn game() -> TempDir {
        let tmp = TempDir::new();
        let put = |rel: &str, text: &str| {
            let path = tmp.join("game").join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        put(
            "common/scripted_triggers/00_t.txt",
            "\u{feff}# A comment with { a brace\nis_ruler_trigger = {\n\tis_ruler = yes # }\n\thas_trait = \"x{\"\n}\nother = {\n\tis_ruler_trigger = yes\n}\n",
        );
        put("events/my_events.txt", "namespace = my\nmy.0001 = {\n\ttype = character_event\n}\n");
        put(
            "localization/english/my_l_english.yml",
            "l_english:\n is_ruler_trigger:0 \"Is a ruler\"\n",
        );
        put("localization/french/my_l_french.yml", "l_french:\n is_ruler_trigger:0 \"Est\"\n");
        put("gfx/portraits/x.txt", "is_ruler_trigger = { }\n");
        put("gui/window.gui", "types T {\n\ttype my_button = button {\n\t}\n}\n");
        tmp
    }

    fn find(tmp: &Path, name: &str, pattern: &str, path: &str) -> Found {
        search(tmp, &Search { name, pattern, path, limit: 10 }).unwrap()
    }

    #[test]
    fn definitions_are_found_with_their_block() {
        let tmp = game();
        let found = find(&tmp, "is_ruler_trigger", "", "");
        // The definition and the English localization; not the use inside `other`, not French or gfx.
        assert_eq!(found.matched, 2);
        assert_eq!(found.hits[0].file, "common/scripted_triggers/00_t.txt");
        assert_eq!(found.hits[0].line, 2);
        assert!(found.hits[0].text.ends_with("has_trait = \"x{\"\n}"));
        assert_eq!(found.hits[1].text, "is_ruler_trigger:0 \"Is a ruler\"");
        assert_eq!(find(&tmp, "my.0001", "", "events/").hits[0].line, 2);
        assert_eq!(find(&tmp, "my_button", "", "").hits[0].file, "gui/window.gui");
        assert_eq!(find(&tmp, "is_ruler_trigger", "", "localization/french").matched, 1);
        assert_eq!(find(&tmp, "is_ruler_trigger", "", "gfx/").matched, 1);
    }

    #[test]
    fn patterns_find_lines() {
        let tmp = game();
        let found = find(&tmp, "", "IS_RULER = yes", "common");
        assert_eq!(found.matched, 1);
        assert_eq!(found.hits[0].text, "is_ruler = yes # }");
        assert!(search(&tmp, &Search { name: "", pattern: "", path: "", limit: 1 }).is_err());
        assert!(search(&tmp, &Search { name: "", pattern: "(", path: "", limit: 1 }).is_err());
    }
}
