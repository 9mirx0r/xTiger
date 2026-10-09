//! "The game said, Tiger did not": the entries of the game's `error.log` that point to a file of
//! the mod, set against the reports of a saved validation. An entry about a file where Tiger has no
//! report at all is a hole in the validator, and the likeliest source of a new rule.
//!
//! The game writes paths relative to the mod (`events/foo.txt line: 84`), so an entry belongs to
//! the mod when such a path exists in the mod's folder.
//!
//! Limits: an entry that names a file but no line counts as covered by any Tiger report in that
//! file, which makes the covered count optimistic. Paths with spaces, non-ASCII characters or an
//! absolute path are not recognised, so such entries are left out. Rows are split by file and by
//! event id, since only quoted names and line numbers are replaced in the message.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

/// `[04:03:59][E][dnamodifier.cpp:482]: text`
static STAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\d{2}:\d{2}:\d{2}\]\[(\w)\]\[([^\]]+)\]: ?(.*)$").unwrap());
/// A path with at least one folder, and the line the game gives after it, if any. The game writes
/// `events/a.txt line: 84` for script errors and `file: "common/x.txt" near line: 17` for parse
/// errors, so a closing quote and `near` may sit between the path and the line.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"([A-Za-z0-9_\-]+(?:/[A-Za-z0-9_\-.]+)+\.[A-Za-z0-9]+)"?(?:\s+(?:near\s+)?line:\s*(\d+))?"#,
    )
    .unwrap()
});
/// A quoted name; an apostrophe inside a word (`Can't`) does not open one.
static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^A-Za-z])'[^']*'").unwrap());
static LINE_NR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"line:\s*\d+").unwrap());

#[derive(Debug)]
pub struct Request<'a> {
    /// The text of the log (`error.log`).
    pub log: &'a str,
    pub mod_dir: &'a Path,
    /// The reports of the validation to compare with.
    pub reports: &'a [Value],
    pub limit: usize,
}

/// One kind of entry the game logged about the mod.
#[derive(Debug, Serialize)]
pub struct Gap {
    /// The game's own source tag, such as `jomini_script_system.cpp:304`.
    pub source: String,
    /// The message with each `'quoted'` name and line number replaced, so one cause is one row.
    pub message: String,
    /// How many log entries it covers.
    pub entries: usize,
    /// Whether Tiger said anything about the file: `nothing` (a hole in the validator) or
    /// `other lines` (it reported in the same file, not on this line).
    pub tiger: &'static str,
    /// A few places, as `path:line`.
    pub examples: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GapReport {
    pub log_entries: usize,
    /// Entries that point to a file of the mod.
    pub about_the_mod: usize,
    /// Of those, the ones Tiger reported on the same file and line (or on the file, when the
    /// game gives no line).
    pub reported_by_tiger: usize,
    /// Entries about the mod's files that Tiger did not report.
    pub gaps: usize,
    /// Entries that Tiger said nothing about anywhere in the file.
    pub files_without_any_report: usize,
    pub rows: Vec<Gap>,
}

/// The entries of a log: the first line and any lines that continue it.
fn entries(log: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in log.lines() {
        if let Some(caps) = STAMP.captures(line) {
            found.push((caps[2].to_owned(), caps[3].to_owned()));
        } else if let Some(last) = found.last_mut()
            && !line.trim().is_empty()
        {
            last.1.push_str(" | ");
            last.1.push_str(line.trim());
        }
    }
    found
}

/// What Tiger reported: file -> the lines it reported on.
fn reported(reports: &[Value]) -> BTreeMap<String, Vec<u64>> {
    let mut map: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for report in reports {
        let Some(locations) = report.get("locations").and_then(Value::as_array) else {
            continue;
        };
        for location in locations {
            let Some(path) = location.get("path").and_then(Value::as_str) else {
                continue;
            };
            let lines = map.entry(path.replace('\\', "/").to_lowercase()).or_default();
            if let Some(line) = location.get("linenr").and_then(Value::as_u64) {
                lines.push(line);
            }
        }
    }
    map
}

fn template(message: &str) -> String {
    let message = QUOTED.replace_all(message, "${1}'…'");
    LINE_NR.replace_all(&message, "line: …").into_owned()
}

/// A path of the mod and the line the game gave with it.
type Place = (String, Option<u64>);

#[derive(Default)]
struct Group {
    entries: usize,
    nothing: bool,
    examples: Vec<String>,
}

pub fn compare(request: &Request) -> GapReport {
    let all = entries(request.log);
    let tiger = reported(request.reports);
    let mut about_the_mod = 0;
    let mut reported_by_tiger = 0;
    let mut files_without_any_report = 0;
    let mut groups: BTreeMap<(String, String, bool), Group> = BTreeMap::new();
    for (source, message) in &all {
        // The paths of the entry that exist in the mod; the first decides the verdict, the rest
        // only matter when the first one is covered.
        let mine: Vec<Place> = PATH
            .captures_iter(message)
            .filter(|caps| request.mod_dir.join(&caps[1]).is_file())
            .map(|caps| (caps[1].to_owned(), caps.get(2).and_then(|n| n.as_str().parse().ok())))
            .collect();
        if mine.is_empty() {
            continue;
        }
        about_the_mod += 1;
        let verdicts: Vec<(&Place, Option<bool>)> = mine
            .iter()
            .map(|place| {
                let lines = tiger.get(&place.0.to_lowercase());
                let verdict = match (lines, place.1) {
                    (None, _) => Some(true),
                    (Some(_), None) => None,
                    (Some(lines), Some(line)) if lines.contains(&line) => None,
                    (Some(_), Some(_)) => Some(false),
                };
                (place, verdict)
            })
            .collect();
        // Covered when any of its paths is reported by Tiger.
        if verdicts.iter().any(|(_, verdict)| verdict.is_none()) {
            reported_by_tiger += 1;
            continue;
        }
        let (place, nothing) = verdicts
            .iter()
            .find_map(|(place, verdict)| verdict.filter(|nothing| *nothing).map(|n| (*place, n)))
            .unwrap_or_else(|| (verdicts[0].0, false));
        if nothing {
            files_without_any_report += 1;
        }
        let group = groups.entry((source.clone(), template(message), nothing)).or_default();
        group.entries += 1;
        group.nothing = nothing;
        if group.examples.len() < 3 {
            let example = match place.1 {
                Some(line) => format!("{}:{line}", place.0),
                None => place.0.clone(),
            };
            if !group.examples.contains(&example) {
                group.examples.push(example);
            }
        }
    }
    let mut rows: Vec<Gap> = groups
        .into_iter()
        .map(|((source, message, _), group)| Gap {
            source,
            message,
            entries: group.entries,
            tiger: if group.nothing { "nothing" } else { "other lines" },
            examples: group.examples,
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.tiger != "nothing")
            .cmp(&(b.tiger != "nothing"))
            .then(b.entries.cmp(&a.entries))
            .then_with(|| a.message.cmp(&b.message))
    });
    let gaps = rows.iter().map(|row| row.entries).sum();
    rows.truncate(request.limit);
    GapReport {
        log_entries: all.len(),
        about_the_mod,
        reported_by_tiger,
        gaps,
        files_without_any_report,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::json;
    use std::fs;

    fn mod_with(files: &[&str]) -> TempDir {
        let dir = TempDir::new();
        for file in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x = yes\n").unwrap();
        }
        dir
    }

    fn report(path: &str, line: u64) -> Value {
        json!({"severity": "error", "key": "k", "message": "m",
               "locations": [{"path": path, "linenr": line}]})
    }

    const LOG: &str = "\
[04:03:14][E][pdx_localize.cpp:279]: Duplicate localization key. Key 'A' is defined in both 'localization/english/a_l_english.yml' and 'localization/english/b_l_english.yml'.
[04:04:21][E][jomini_script_system.cpp:304]: Script system error!
  Error: add_opinion effect [ 'opinion' is not defined ]
  Script location: file: events/a.txt line: 84 (a.0300:option)

[04:04:22][E][jomini_effect.cpp:1146]: Flag 'judaism' is set but is never used.
[04:04:23][E][dnamodifier.cpp:482]: Can't find accessory 'x', near file: gfx/portraits/m.txt line: 24 (m)
[04:04:24][E][lexer.cpp:306]: File 'common/vanilla_only.txt' should be in utf8-bom encoding
";

    #[test]
    fn splits_entries_and_keeps_continuation_lines() {
        let all = entries(LOG);
        assert_eq!(all.len(), 5);
        assert_eq!(all[1].0, "jomini_script_system.cpp:304");
        assert!(all[1].1.contains("Script location: file: events/a.txt line: 84"));
    }

    #[test]
    fn only_the_mods_own_files_count_and_tiger_reports_are_matched() {
        let dir = mod_with(&[
            "localization/english/a_l_english.yml",
            "events/a.txt",
            "gfx/portraits/m.txt",
        ]);
        // Tiger reported the exact line of the event and another line of the portrait file.
        let reports = vec![report("events/a.txt", 84), report("gfx/portraits/m.txt", 99)];
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &reports, limit: 10 });
        assert_eq!(result.log_entries, 5);
        assert_eq!(result.about_the_mod, 3);
        assert_eq!(result.reported_by_tiger, 1);
        assert_eq!(result.gaps, 2);
        assert_eq!(result.files_without_any_report, 1);
        // The hole (nothing in the file) comes first.
        assert_eq!(result.rows[0].tiger, "nothing");
        assert!(result.rows[0].message.contains("Duplicate localization key. Key '…'"));
        assert_eq!(result.rows[1].tiger, "other lines");
        assert_eq!(result.rows[1].examples, vec!["gfx/portraits/m.txt:24"]);
    }

    #[test]
    fn a_file_report_without_a_line_in_the_log_counts_as_covered() {
        let dir = mod_with(&["localization/english/a_l_english.yml"]);
        let reports = vec![report("localization/english/a_l_english.yml", 5)];
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &reports, limit: 10 });
        assert_eq!(result.about_the_mod, 1);
        assert_eq!(result.gaps, 0);
    }

    #[test]
    fn templates_replace_names_and_lines_but_not_apostrophes() {
        assert_eq!(
            template("Can't find accessory 'x', near file: a/b.txt line: 24 (m)"),
            "Can't find accessory '…', near file: a/b.txt line: … (m)"
        );
    }

    #[test]
    fn a_parse_error_line_is_read_after_the_quoted_path() {
        let dir = mod_with(&["common/men_at_arms_types/x.txt"]);
        let log = concat!(
            "[04:00:00][E][pdx_persistent_reader.cpp:62]: Error: \"unexpected token\" ",
            "in file: \"common/men_at_arms_types/x.txt\" near line: 17\n"
        );
        // Tiger reported another line of that file, so the entry is a gap and not covered.
        let other = vec![report("common/men_at_arms_types/x.txt", 3)];
        let result = compare(&Request { log, mod_dir: &dir, reports: &other, limit: 10 });
        assert_eq!(result.gaps, 1);
        assert_eq!(result.rows[0].tiger, "other lines");
        assert_eq!(result.rows[0].examples, vec!["common/men_at_arms_types/x.txt:17"]);
        // Tiger reported exactly that line.
        let same = vec![report("common/men_at_arms_types/x.txt", 17)];
        let result = compare(&Request { log, mod_dir: &dir, reports: &same, limit: 10 });
        assert_eq!(result.gaps, 0);
        assert_eq!(result.reported_by_tiger, 1);
    }

    #[test]
    fn limit_cuts_the_rows_not_the_counts() {
        let dir = mod_with(&["localization/english/a_l_english.yml", "events/a.txt"]);
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &[], limit: 1 });
        assert_eq!(result.gaps, 2);
        assert_eq!(result.rows.len(), 1);
    }
}
