//! A dry run of the mechanical part of moving a mod to a newer game version: renames that are
//! the same everywhere, found line by line. Nothing is written; each proposed edit has the file,
//! the line and the line as it would read, so the assistant (or the author) can apply it. A line
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
        pattern: r"\b(any|every|random|ordered)_character\b",
        replacement: "${1}_living_character",
        note: "the global character iterators are the *_living_character ones in 1.20",
    },
    Rule {
        name: "is_created",
        pattern: r"\bis_created\b(\s*=)",
        replacement: "is_title_created${1}",
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

/// The edits a mod needs to follow the renames in `RULES`.
///
/// # Errors
/// If the mod folder is missing.
pub fn plan(req: &Request) -> Result<Report, String> {
    if !req.mod_dir.is_dir() {
        return Err(format!("The mod folder {} does not exist.", req.mod_dir.display()));
    }
    let filter = req.path.trim().replace('\\', "/");
    let mut found = Vec::new();
    for folder in FOLDERS {
        files(&req.mod_dir.join(folder), req.mod_dir, &mut found);
    }
    found.retain(|(rel, _)| filter.is_empty() || rel.contains(&filter));
    found.sort();

    let mut counts = vec![0_usize; RULES.len()];
    let mut edits = Vec::new();
    let mut total = 0;
    for (rel, path) in &found {
        let Ok(bytes) = fs::read(path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
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
                    counts[n] += 1;
                    hit.push(rule.name);
                }
            }
            if changes.is_empty() {
                continue;
            }
            total += 1;
            if edits.len() < req.limit {
                changes.sort_by_key(|change| change.0);
                let mut after = String::with_capacity(code.len());
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
                edits.push(Edit {
                    file: rel.clone(),
                    line: i + 1,
                    rules: hit,
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
             \tNOT = { doctrine:tenet_pacifism = { is_in_list = x } }\n}\n",
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
