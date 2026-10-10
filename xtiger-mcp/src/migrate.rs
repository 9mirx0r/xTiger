//! A dry run of the mechanical part of moving a mod to a newer game version: renames that are
//! the same everywhere, found line by line. Nothing is written here (`migrate_apply` writes them);
//! each proposed edit has the file, the line and the line as it would read. A line
//! that matches several rules is one edit with all of them applied; comments and quoted text are
//! never changed.
//!
//! A rule belongs here only when the replacement does not depend on the surrounding script. Renames
//! that do (a removed trait, a datafunction that changed its arguments) are only explained by the
//! validator's hints.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

/// The folders of a mod that hold script.
const FOLDERS: [&str; 3] = ["common", "events", "history"];

struct Rule {
    name: &'static str,
    pattern: &'static str,
    replacement: &'static str,
    note: &'static str,
}

/// Renames from before 1.20, each checked against the game's own files.
const RULES: [Rule; 7] = [
    Rule {
        name: "every_character",
        // Only as a key (`every_character = {`): a saved scope name such as
        // `save_scope_as = every_character` or `scope:every_character` is the mod's own word.
        pattern: r"(^|[^\w:.$@])(any|every|random|ordered)_character(\s*=)",
        replacement: "${1}${2}_living_character${3}",
        note: "the global character iterators are the *_living_character ones in 1.20",
    },
    Rule {
        name: "is_created",
        pattern: r"(^|[^\w:.$@])is_created(\s*=)",
        replacement: "${1}is_title_created${2}",
        note: "the trigger is is_title_created in 1.20",
    },
    Rule {
        name: "create_holy_order_effect",
        pattern: r"\bcreate_holy_order_effect\b",
        replacement: "create_holy_order_accompanying_effect",
        note: "renamed in 1.20; check its arguments against the game's scripted effect",
    },
    Rule {
        name: "has_doctrine = tenet_",
        pattern: r"\bhas_doctrine(\s*=\s*)(tenet_\w+)",
        replacement: "has_tenet${1}${2}",
        note: "tenets are tested with has_tenet in 1.20",
    },
    Rule {
        name: "doctrine:tenet_",
        pattern: r"\bdoctrine:(tenet_\w+)",
        replacement: "tenet:${1}",
        note: "tenets are their own scope type, `tenet:tenet_x`, in 1.20",
    },
    Rule {
        name: "guardian_or_court_tutor_trigger_event",
        pattern: r"\bguardian_or_court_tutor_trigger_event\b",
        replacement: "guardian_or_court_tutor_trigger_event_effect",
        note: "the scripted effect has the _effect suffix in 1.20",
    },
    Rule {
        name: "guardian_or_court_tutor_trait",
        pattern: r"\bguardian_or_court_tutor_trait\b",
        replacement: "guardian_or_court_tutor_trait_trigger",
        note: "the scripted trigger has the _trigger suffix in 1.20",
    },
];

static COMPILED: LazyLock<Vec<Regex>> =
    LazyLock::new(|| RULES.iter().map(|rule| Regex::new(rule.pattern).unwrap()).collect());

#[derive(Debug)]
pub struct Request<'a> {
    pub mod_dir: &'a Path,
    /// Only mod files whose path, relative to the mod, contains this.
    pub path: &'a str,
    /// The most edits listed.
    pub limit: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Edit {
    pub file: String,
    pub line: usize,
    pub rules: Vec<&'static str>,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RuleCount {
    pub rule: &'static str,
    pub note: &'static str,
    /// The lines this rule changes.
    pub edits: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Report {
    pub files_scanned: usize,
    pub total_edits: usize,
    pub rules: Vec<RuleCount>,
    pub edits: Vec<Edit>,
}

/// Where the comment of a script line starts, outside quotes.
fn comment_start(line: &str) -> usize {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return i,
            _ => {}
        }
    }
    line.len()
}

/// The code with everything inside quotes blanked out (same byte length), so that a rule cannot
/// match text that is only a string.
fn without_quoted(code: &str) -> String {
    let mut quoted = false;
    let mut out = String::with_capacity(code.len());
    for c in code.chars() {
        if c == '"' {
            quoted = !quoted;
            out.push(c);
        } else if quoted {
            out.extend(std::iter::repeat_n('_', c.len_utf8()));
        } else {
            out.push(c);
        }
    }
    out
}

fn files(dir: &Path, base: &Path, found: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        // A symlink is not followed, so a link cannot lead the scan out of the mod.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            files(&path, base, found);
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            found.push((rel.to_string_lossy().replace('\\', "/"), path));
        }
    }
}

/// The script files of a mod, as (path relative to the mod, full path), sorted, keeping those whose
/// relative path contains `filter` (all of them when it is empty).
pub(crate) fn script_files(mod_dir: &Path, filter: &str) -> Vec<(String, std::path::PathBuf)> {
    let filter = filter.trim().replace('\\', "/");
    let mut found = Vec::new();
    for folder in FOLDERS {
        files(&mod_dir.join(folder), mod_dir, &mut found);
    }
    found.retain(|(rel, _)| filter.is_empty() || rel.contains(&filter));
    found.sort();
    found
}

/// One script line (without its line ending) with every rule applied, and the indexes in `RULES`
/// of the rules that changed it; `None` when no rule matches.
pub(crate) fn rewrite(line: &str) -> Option<(String, Vec<usize>)> {
    let (code, comment) = line.split_at(comment_start(line));
    let masked = without_quoted(code);
    // (start, end, replacement) of every match of every rule, outside quotes.
    let mut changes: Vec<(usize, usize, String)> = Vec::new();
    let mut hit = Vec::new();
    for (n, (rule, regex)) in RULES.iter().zip(COMPILED.iter()).enumerate() {
        let before = changes.len();
        for caps in regex.captures_iter(&masked) {
            let Some(whole) = caps.get(0) else { continue };
            let mut replacement = String::new();
            caps.expand(rule.replacement, &mut replacement);
            changes.push((whole.start(), whole.end(), replacement));
        }
        if changes.len() > before {
            hit.push(n);
        }
    }
    if changes.is_empty() {
        return None;
    }
    changes.sort_by_key(|change| change.0);
    let mut after = String::with_capacity(line.len());
    let mut at = 0;
    for (start, end, replacement) in &changes {
        if *start < at {
            continue;
        }
        after.push_str(&code[at..*start]);
        after.push_str(replacement);
        at = *end;
    }
    after.push_str(&code[at..]);
    after.push_str(comment);
    Some((after, hit))
}

/// The names in `RULES` of the rules at these indexes.
pub(crate) fn rule_names(indexes: &[usize]) -> Vec<&'static str> {
    indexes.iter().map(|&n| RULES[n].name).collect()
}

/// The edits a mod needs to follow the renames in `RULES`.
///
/// # Errors
/// If the mod folder is missing.
pub fn plan(req: &Request) -> Result<Report, String> {
    if !req.mod_dir.is_dir() {
        return Err(format!("The mod folder {} does not exist.", req.mod_dir.display()));
    }
    let found = script_files(req.mod_dir, req.path);

    let mut counts = vec![0_usize; RULES.len()];
    let mut edits = Vec::new();
    let mut total = 0;
    for (rel, path) in &found {
        let Ok(bytes) = fs::read(path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let Some((after, hit)) = rewrite(line) else { continue };
            for &n in &hit {
                counts[n] += 1;
            }
            total += 1;
            if edits.len() < req.limit {
                edits.push(Edit {
                    file: rel.clone(),
                    line: i + 1,
                    rules: rule_names(&hit),
                    before: line.trim().to_owned(),
                    after: after.trim().to_owned(),
                });
            }
        }
    }
    let rules = RULES
        .iter()
        .zip(&counts)
        .map(|(rule, edits)| RuleCount { rule: rule.name, note: rule.note, edits: *edits })
        .collect();
    Ok(Report { files_scanned: found.len(), total_edits: total, rules, edits })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn proposes_the_renames_and_keeps_comments() {
        let tmp = TempDir::new();
        let dir = tmp.join("common/scripted_effects");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("a.txt"),
            "x = {\n\tevery_character = { limit = { is_created = yes } } # every_character\n\
             \thas_doctrine = tenet_pacifism\n\thas_doctrine = doctrine_monogamy\n\
             \tdesc = \"is_created = no # not a comment\"\n\
             \tguardian_or_court_tutor_trait = { TRAIT = craven }\n\
             \tNOT = { doctrine:tenet_pacifism = { is_in_list = x } }\n\
             \tsave_scope_as = every_character\n\
             \tscope:every_character = { add_gold = 1 }\n}\n",
        )
        .unwrap();
        fs::write(tmp.join("readme.txt"), "every_character = yes\n").unwrap();
        let report = plan(&Request { mod_dir: &tmp, path: "", limit: 10 }).unwrap();
        assert_eq!(report.files_scanned, 1);
        assert_eq!(report.total_edits, 4);
        let afters: Vec<&str> = report.edits.iter().map(|e| e.after.as_str()).collect();
        assert_eq!(
            afters,
            [
                "every_living_character = { limit = { is_title_created = yes } } # every_character",
                "has_tenet = tenet_pacifism",
                "guardian_or_court_tutor_trait_trigger = { TRAIT = craven }",
                "NOT = { tenet:tenet_pacifism = { is_in_list = x } }",
            ]
        );
        assert_eq!(report.edits[0].rules.len(), 2);
        assert_eq!(report.edits[1].line, 3);
        assert_eq!(report.rules.iter().map(|r| r.edits).collect::<Vec<_>>(), [1, 1, 0, 1, 1, 0, 1]);
        let limited = plan(&Request { mod_dir: &tmp, path: "nothing", limit: 10 }).unwrap();
        assert_eq!(limited.total_edits, 0);
        assert!(plan(&Request { mod_dir: &tmp.join("nope"), path: "", limit: 1 }).is_err());
    }
}
