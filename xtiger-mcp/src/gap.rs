//! "The game said, Tiger did not": the entries of the game's `error.log` that point to a file of
//! the mod, set against the reports of a saved validation. An entry about a file where Tiger has no
//! report at all is a hole in the validator, and the likeliest source of a new rule.
//!
//! The game writes paths relative to the mod (`events/foo.txt line: 84`), so an entry belongs to
//! the mod when such a path exists in the mod's folder. An absolute path inside the mod's folder
//! is made relative first, and a quoted path may hold spaces.
//!
//! Every entry of the log ends in exactly one state, so the counts add up to `log_entries`:
//! - `matched`: Tiger reported at that place, and its message names something the game's entry
//!   names. A tip or an untidy report never matches: it is about layout, not about the error.
//!   The line may differ by up to `NEAR_LINES`, since the game often gives the line of the
//!   enclosing statement; those are counted apart in `matched_near_line`.
//! - `same_location_other_cause`: Tiger reported at that place, but about something else.
//! - `same_file_other_lines`: Tiger reported in that file, not at that line (nor near it, with
//!   the same cause).
//! - `no_tiger_report`: Tiger said nothing about the file.
//! - `unattributed`: the entry names no file of the mod (the reason says what it names).
//! - `unsupported_format`: the entry names a path this module cannot place in the mod.
//!
//! Limits: an entry that names a file but no line is judged against every report in that file.
//! The cause check compares the names between quotes or brackets in the entry (or, without any,
//! its longer words) with the text of the report: it can miss a match that is worded very
//! differently, which then shows as `same_location_other_cause` and is worth a look rather than
//! trust. Unquoted paths with spaces are not recognised. Rows are split by file and by event id,
//! since only quoted names and line numbers are replaced in the message.

use std::collections::{BTreeMap, BTreeSet};
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
    Regex::new(r#"([\w\-]+(?:/[\w\-.]+)+\.\w+)"?(?:\s+(?:near\s+)?line:\s*(\d+))?"#).unwrap()
});
/// A quoted path, which may hold spaces.
static QUOTED_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""((?:[^"/\n]+/)+[^"/\n]+\.\w+)"(?:\s+(?:near\s+)?line:\s*(\d+))?"#).unwrap()
});
/// A path that starts at a drive letter.
static ABSOLUTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b[a-z]:/[^\s"']+"#).unwrap());
/// A quoted name; an apostrophe inside a word (`Can't`) does not open one.
static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^A-Za-z])'[^']*'").unwrap());
static NAMES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:^|[^A-Za-z])'([^']+)'").unwrap());
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]").unwrap());
static LINE_NR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"line:\s*\d+").unwrap());

/// Words too common in the game's messages to say anything about a cause.
const COMMON: [&str; 12] = [
    "error", "script", "system", "location", "should", "defined", "found", "invalid", "value",
    "effect", "trigger", "scope",
];

const STATES: [&str; 6] = [
    "matched",
    "same_location_other_cause",
    "same_file_other_lines",
    "no_tiger_report",
    "unattributed",
    "unsupported_format",
];

/// The most lines of an entry's stack kept with a row.
const STACK_LINES: usize = 8;

/// How many lines apart the game's line and Tiger's line may be for the same cause to count as
/// matched. The game tends to name the line where the enclosing statement or block opens, while
/// Tiger names the offending token; seen live: Tiger line 128, game line 126. Kept small so a
/// second defect of the same kind further down the file is not taken for the one Tiger reported.
const NEAR_LINES: u64 = 3;

/// The most near-line matches listed as examples.
const NEAR_EXAMPLES: usize = 5;

#[derive(Debug)]
pub struct Request<'a> {
    /// The text of the log (`error.log`).
    pub log: &'a str,
    pub mod_dir: &'a Path,
    /// The reports of the validation to compare with.
    pub reports: &'a [Value],
    pub limit: usize,
}

/// One kind of entry the game logged, in one state.
#[derive(Debug, Serialize)]
pub struct Gap {
    /// The game's own source tag, such as `jomini_script_system.cpp:304`.
    pub source: String,
    /// The message with each `'quoted'` name and line number replaced, so one cause is one row.
    pub message: String,
    /// How many log entries it covers.
    pub entries: usize,
    /// What became of these entries, as the module documentation lists.
    pub state: &'static str,
    /// What Tiger did about the file: `nothing` (a hole in the validator), `other lines` (it
    /// reported in the same file, not on this line), `other cause` (it reported there, about
    /// something else) or `n/a`.
    pub tiger: &'static str,
    /// Why an entry is `unattributed` or `unsupported_format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A few places, as `path:line`.
    pub examples: Vec<String>,
    /// The continuation lines of the first entry of the row, as the game wrote them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<String>,
}

/// The saved run a report was made against, and whether it still fits the files.
#[derive(Debug, Serialize)]
pub struct RunBinding {
    pub run_id: String,
    /// `current` (made for the files on disk now), `stale` or `unknown` (no hash was recorded).
    pub files: &'static str,
}

#[derive(Debug, Serialize)]
pub struct GapReport {
    pub log_entries: usize,
    /// How every entry ended: the values add up to `log_entries`.
    pub states: BTreeMap<&'static str, usize>,
    /// Entries that point to a file of the mod.
    pub about_the_mod: usize,
    /// Of those, the ones Tiger reported at that place with a message about the same thing.
    pub reported_by_tiger: usize,
    /// Of the `matched` entries, how many matched only through the line tolerance: same file and
    /// cause, Tiger's line within [`NEAR_LINES`] of the game's. The rest matched on the exact line.
    pub matched_near_line: usize,
    /// A few of those, as `path:game line (Tiger: line N)`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub near_line_examples: Vec<String>,
    /// Entries about the mod's files that Tiger did not report (about the mod, not matched).
    pub gaps: usize,
    /// Entries that Tiger said nothing about anywhere in the file.
    pub files_without_any_report: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunBinding>,
    pub rows: Vec<Gap>,
}

/// One entry of a log: the first line and any lines that continue it.
struct Entry {
    source: String,
    /// The first line and the continuation lines joined by ` | `.
    message: String,
    stack: Vec<String>,
}

fn entries(log: &str) -> Vec<Entry> {
    let mut found: Vec<Entry> = Vec::new();
    for line in log.lines() {
        if let Some(caps) = STAMP.captures(line) {
            found.push(Entry {
                source: caps[2].to_owned(),
                message: caps[3].to_owned(),
                stack: Vec::new(),
            });
        } else if let Some(last) = found.last_mut()
            && !line.trim().is_empty()
        {
            last.message.push_str(" | ");
            last.message.push_str(line.trim());
            last.stack.push(line.trim().to_owned());
        }
    }
    found
}

/// What Tiger said at one place.
struct Said {
    /// `None` when the report has no line there.
    line: Option<u64>,
    /// Tips and untidy reports are about layout.
    advisory: bool,
    /// Key, message and info, lower-cased.
    text: String,
}

/// What Tiger reported: file -> what it said there.
fn reported(reports: &[Value]) -> BTreeMap<String, Vec<Said>> {
    let mut map: BTreeMap<String, Vec<Said>> = BTreeMap::new();
    let field = |report: &Value, name: &str| {
        report.get(name).and_then(Value::as_str).unwrap_or("").to_lowercase()
    };
    for report in reports {
        let Some(locations) = report.get("locations").and_then(Value::as_array) else {
            continue;
        };
        let advisory = matches!(field(report, "severity").as_str(), "tips" | "untidy");
        let text = format!(
            "{} {} {}",
            field(report, "key"),
            field(report, "message"),
            field(report, "info")
        );
        for location in locations {
            let Some(path) = location.get("path").and_then(Value::as_str) else {
                continue;
            };
            map.entry(path.replace('\\', "/").to_lowercase()).or_default().push(Said {
                line: location.get("linenr").and_then(Value::as_u64),
                advisory,
                text: text.clone(),
            });
        }
    }
    map
}

fn template(message: &str) -> String {
    let message = QUOTED.replace_all(message, "${1}'…'");
    LINE_NR.replace_all(&message, "line: …").into_owned()
}

/// The names an entry points to: what is between quotes or brackets.
fn names(message: &str) -> BTreeSet<String> {
    let quoted = NAMES.captures_iter(message).map(|c| c[1].to_lowercase());
    let bracketed = BRACKETS.captures_iter(message).flat_map(|c| {
        c[1].split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .filter(|word| word.len() > 3)
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    });
    quoted
        .chain(bracketed)
        .filter(|name| name.len() > 1 && !name.contains('/') && !name.contains('\\'))
        .collect()
}

/// Whether what Tiger said is about what the entry says.
fn compatible(message: &str, said: &str) -> bool {
    let names = names(message);
    if !names.is_empty() {
        return names.iter().any(|name| said.contains(name.as_str()));
    }
    let head = message.split(" | ").next().unwrap_or(message).to_lowercase();
    head.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| word.len() >= 5 && !COMMON.contains(word))
        .any(|word| said.contains(word))
}

/// The message with the mod's own folder taken out of any absolute path, and slashes made forward.
fn relative(message: &str, mod_dir: &Path) -> String {
    let text = message.replace('\\', "/");
    let mut root = mod_dir.display().to_string().replace('\\', "/");
    if let Some(stripped) = root.strip_prefix("//?/") {
        root = stripped.to_owned();
    }
    let root = format!("{}/", root.trim_end_matches('/'));
    let (lower, needle) = (text.to_lowercase(), root.to_lowercase());
    // Lower-casing can change a length on unusual letters: then the text is left as it is.
    if lower.len() != text.len() || !lower.contains(&needle) {
        return text;
    }
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(at) = lower[from..].find(&needle) {
        out.push_str(&text[from..from + at]);
        from += at + needle.len();
    }
    out.push_str(&text[from..]);
    out
}

/// A path of the mod and the line the game gave with it.
type Place = (String, Option<u64>);

/// How a place was judged: its state, and for a near-line match the distance to Tiger's line.
struct Verdict {
    state: &'static str,
    /// Set only when `state` is `matched` through [`NEAR_LINES`]: Tiger's line and the game's line.
    near: Option<(u64, u64)>,
}

impl Verdict {
    fn of(state: &'static str) -> Self {
        Verdict { state, near: None }
    }
}

/// The state of one place.
fn judge(place: &Place, message: &str, tiger: &BTreeMap<String, Vec<Said>>) -> Verdict {
    let Some(said) = tiger.get(&place.0.to_lowercase()) else {
        return Verdict::of("no_tiger_report");
    };
    let here: Vec<&Said> = said
        .iter()
        .filter(|s| place.1.is_none() || s.line.is_none() || s.line == place.1)
        .collect();
    if here.iter().any(|s| !s.advisory && compatible(message, &s.text)) {
        return Verdict::of("matched");
    }
    // The game often names the line where the enclosing statement or block opens, a few lines off
    // from the line Tiger blames. The cause must still agree, so this only moves the line.
    if let Some(line) = place.1 {
        let near = said
            .iter()
            .filter(|s| !s.advisory && compatible(message, &s.text))
            .filter_map(|s| s.line)
            .filter(|tiger_line| tiger_line.abs_diff(line) <= NEAR_LINES)
            .min_by_key(|tiger_line| tiger_line.abs_diff(line));
        if let Some(tiger_line) = near {
            return Verdict { state: "matched", near: Some((tiger_line, line)) };
        }
    }
    Verdict::of(if here.is_empty() { "same_file_other_lines" } else { "same_location_other_cause" })
}

/// How far an outcome is from being matched: the worst of an entry's places is the one shown.
fn rank(state: &str) -> usize {
    STATES.iter().position(|s| *s == state).unwrap_or(0)
}

#[derive(Default)]
struct Group {
    entries: usize,
    reason: Option<String>,
    examples: Vec<String>,
    stack: Vec<String>,
}

/// What an entry that names no file of the mod names instead.
fn unplaced(message: &str, mod_dir: &Path) -> (&'static str, String) {
    if let Some(found) = ABSOLUTE.find(message) {
        return (
            "unsupported_format",
            format!("an absolute path outside the mod: {}", found.as_str()),
        );
    }
    let outside: Vec<&str> = PATH
        .captures_iter(message)
        .map(|c| c.get(1).map_or("", |m| m.as_str()))
        .filter(|path| !mod_dir.join(path).is_file())
        .take(3)
        .collect();
    if outside.is_empty() {
        ("unattributed", "it names no file".to_owned())
    } else {
        ("unattributed", format!("it names files that are not in the mod: {}", outside.join(", ")))
    }
}

/// The paths of an entry that exist in the mod, with the line the game gave.
fn places(message: &str, mod_dir: &Path) -> Vec<Place> {
    let mut found: Vec<Place> = Vec::new();
    for caps in PATH.captures_iter(message).chain(QUOTED_PATH.captures_iter(message)) {
        let path = caps[1].to_owned();
        let line = caps.get(2).and_then(|n| n.as_str().parse().ok());
        if mod_dir.join(&path).is_file() && !found.iter().any(|(p, l)| *p == path && *l == line) {
            found.push((path, line));
        }
    }
    found
}

pub fn compare(request: &Request) -> GapReport {
    let all = entries(request.log);
    let tiger = reported(request.reports);
    let mut states: BTreeMap<&'static str, usize> = STATES.iter().map(|s| (*s, 0)).collect();
    let mut groups: BTreeMap<(String, String, &'static str), Group> = BTreeMap::new();
    let mut about_the_mod = 0;
    let mut near_lines = 0;
    let mut near_examples: Vec<String> = Vec::new();
    for entry in &all {
        let message = relative(&entry.message, request.mod_dir);
        let mine = places(&message, request.mod_dir);
        let (state, shown, reason) = if mine.is_empty() {
            let (state, reason) = unplaced(&message, request.mod_dir);
            (state, None, Some(reason))
        } else {
            about_the_mod += 1;
            // Matched when any of its places is; otherwise the worst place is the one shown.
            let judged: Vec<(&Place, Verdict)> =
                mine.iter().map(|place| (place, judge(place, &message, &tiger))).collect();
            // An exact match is preferred over a near-line one when the entry has both.
            let best = judged
                .iter()
                .filter(|(_, v)| v.state == "matched")
                .min_by_key(|(_, v)| v.near.is_some());
            if let Some((place, verdict)) = best {
                if let Some((tiger_line, game_line)) = verdict.near {
                    near_lines += 1;
                    if near_examples.len() < NEAR_EXAMPLES {
                        near_examples
                            .push(format!("{}:{game_line} (Tiger: line {tiger_line})", place.0));
                    }
                }
                ("matched", None, None)
            } else {
                let state = judged
                    .iter()
                    .map(|(_, v)| v.state)
                    .max_by_key(|s| rank(s))
                    .unwrap_or("matched");
                let place =
                    judged.iter().find(|(_, v)| v.state == state).map(|(p, _)| (*p).clone());
                (state, place, None)
            }
        };
        *states.entry(state).or_default() += 1;
        if state == "matched" {
            continue;
        }
        let group = groups.entry((entry.source.clone(), template(&message), state)).or_default();
        group.entries += 1;
        group.reason = group.reason.take().or(reason);
        if group.stack.is_empty() {
            group.stack = entry.stack.iter().take(STACK_LINES).cloned().collect();
        }
        if group.examples.len() < 3 {
            let example = match shown {
                Some((path, Some(line))) => format!("{path}:{line}"),
                Some((path, None)) => path,
                None => message.chars().take(120).collect(),
            };
            if !group.examples.contains(&example) {
                group.examples.push(example);
            }
        }
    }
    let mut rows: Vec<Gap> = groups
        .into_iter()
        .map(|((source, message, state), group)| Gap {
            source,
            message,
            entries: group.entries,
            state,
            tiger: match state {
                "no_tiger_report" => "nothing",
                "same_file_other_lines" => "other lines",
                "same_location_other_cause" => "other cause",
                _ => "n/a",
            },
            reason: group.reason,
            examples: group.examples,
            stack: group.stack,
        })
        .collect();
    // The mod's own gaps first, the hole in the validator before the rest; what names no file of
    // the mod last.
    let order = |state: &str| match state {
        "no_tiger_report" => 0,
        "same_location_other_cause" => 1,
        "same_file_other_lines" => 2,
        "unsupported_format" => 3,
        _ => 4,
    };
    rows.sort_by(|a, b| {
        order(a.state)
            .cmp(&order(b.state))
            .then(b.entries.cmp(&a.entries))
            .then_with(|| a.message.cmp(&b.message))
    });
    let matched = states["matched"];
    rows.truncate(request.limit);
    GapReport {
        log_entries: all.len(),
        about_the_mod,
        reported_by_tiger: matched,
        matched_near_line: near_lines,
        near_line_examples: near_examples,
        gaps: about_the_mod - matched,
        files_without_any_report: states["no_tiger_report"],
        states,
        run: None,
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

    fn said(severity: &str, path: &str, line: u64, message: &str) -> Value {
        json!({"severity": severity, "key": "k", "message": message,
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

    fn total(result: &GapReport) -> usize {
        result.states.values().sum()
    }

    #[test]
    fn splits_entries_and_keeps_continuation_lines() {
        let all = entries(LOG);
        assert_eq!(all.len(), 5);
        assert_eq!(all[1].source, "jomini_script_system.cpp:304");
        assert!(all[1].message.contains("Script location: file: events/a.txt line: 84"));
        assert_eq!(all[1].stack.len(), 2);
    }

    #[test]
    fn every_entry_ends_in_one_state_and_only_the_same_cause_is_matched() {
        let dir = mod_with(&[
            "localization/english/a_l_english.yml",
            "events/a.txt",
            "gfx/portraits/m.txt",
        ]);
        // Tiger said the same thing at the event's line and another thing in the portrait file.
        let reports = vec![
            said("error", "events/a.txt", 84, "Unknown opinion type"),
            said("warning", "gfx/portraits/m.txt", 99, "Something else"),
        ];
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &reports, limit: 10 });
        assert_eq!(result.log_entries, 5);
        assert_eq!(total(&result), 5, "the states add up to the log");
        assert_eq!(result.about_the_mod, 3);
        assert_eq!(result.reported_by_tiger, 1);
        assert_eq!(result.gaps, 2);
        assert_eq!(result.states["matched"], 1);
        assert_eq!(result.states["no_tiger_report"], 1);
        assert_eq!(result.states["same_file_other_lines"], 1);
        assert_eq!(result.states["unattributed"], 2);
        assert_eq!(result.files_without_any_report, 1);
        // The hole (nothing in the file) comes first.
        assert_eq!(result.rows[0].tiger, "nothing");
        assert!(result.rows[0].message.contains("Duplicate localization key. Key '…'"));
        assert_eq!(result.rows[1].tiger, "other lines");
        assert_eq!(result.rows[1].examples, vec!["gfx/portraits/m.txt:24"]);
        let vanilla = result.rows.iter().find(|r| r.state == "unattributed").unwrap();
        assert!(vanilla.reason.is_some());
    }

    #[test]
    fn a_formatting_report_at_the_place_is_not_the_same_cause() {
        let dir = mod_with(&["events/a.txt"]);
        let log = "[04:04:21][E][jomini_script_system.cpp:304]: Script system error!\n  \
                   Error: add_opinion effect [ 'opinion' is not defined ]\n  \
                   Script location: file: events/a.txt line: 84 (a.1:option)\n";
        let run = |reports: Vec<Value>| {
            compare(&Request { log, mod_dir: &dir, reports: &reports, limit: 10 }).states
        };
        // A tip that happens to mention the word is still about layout.
        let tip = run(vec![said("tips", "events/a.txt", 84, "the opinion line could be tidier")]);
        assert_eq!((tip["matched"], tip["same_location_other_cause"]), (0, 1));
        let other = run(vec![said("error", "events/a.txt", 84, "Missing trait")]);
        assert_eq!((other["matched"], other["same_location_other_cause"]), (0, 1));
        let same = run(vec![said("error", "events/a.txt", 84, "Unknown opinion")]);
        assert_eq!((same["matched"], same["same_location_other_cause"]), (1, 0));
    }

    #[test]
    fn a_report_a_few_lines_off_with_the_same_cause_is_matched_and_flagged() {
        let dir = mod_with(&["events/a.txt"]);
        let log = "[04:04:21][E][jomini_script_system.cpp:304]: Script system error!\n  \
                   Error: add_opinion effect [ 'opinion' is not defined ]\n  \
                   Script location: file: events/a.txt line: 126 (a.1:option)\n";
        let run = |reports: Vec<Value>| {
            compare(&Request { log, mod_dir: &dir, reports: &reports, limit: 10 })
        };
        // Tiger blames line 128, two lines below the game's: the same defect.
        let near = run(vec![said("error", "events/a.txt", 128, "Unknown opinion")]);
        assert_eq!((near.states["matched"], near.gaps, near.matched_near_line), (1, 0, 1));
        assert_eq!(near.near_line_examples, vec!["events/a.txt:126 (Tiger: line 128)"]);
        // The edge of the tolerance still matches, one more line does not.
        let edge = run(vec![said("error", "events/a.txt", 126 + NEAR_LINES, "Unknown opinion")]);
        assert_eq!(edge.matched_near_line, 1);
        let far = run(vec![said("error", "events/a.txt", 127 + NEAR_LINES, "Unknown opinion")]);
        assert_eq!((far.states["matched"], far.states["same_file_other_lines"]), (0, 1));
        assert_eq!(far.matched_near_line, 0);
        // A near line about something else is not the same defect: the cause is not loosened.
        let other = run(vec![said("error", "events/a.txt", 127, "Missing trait")]);
        assert_eq!((other.states["matched"], other.states["same_file_other_lines"]), (0, 1));
        // A tip next to the line is about layout and never matches.
        let tip = run(vec![said("tips", "events/a.txt", 127, "the opinion line could be tidier")]);
        assert_eq!(tip.states["matched"], 0);
        // When the exact line matches too, it is an exact match and not a near one.
        let both = run(vec![
            said("error", "events/a.txt", 127, "Unknown opinion"),
            said("error", "events/a.txt", 126, "Unknown opinion"),
        ]);
        assert_eq!((both.states["matched"], both.matched_near_line), (1, 0));
        // An exact line about another cause with a near line of the right cause is matched near.
        let mixed = run(vec![
            said("error", "events/a.txt", 126, "Missing trait"),
            said("error", "events/a.txt", 128, "Unknown opinion"),
        ]);
        assert_eq!((mixed.states["matched"], mixed.matched_near_line), (1, 1));
    }

    #[test]
    fn a_file_report_without_a_line_in_the_log_is_judged_by_its_cause() {
        let dir = mod_with(&["localization/english/a_l_english.yml"]);
        let reports =
            vec![said("error", "localization/english/a_l_english.yml", 5, "Duplicate key A")];
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &reports, limit: 10 });
        assert_eq!(result.about_the_mod, 1);
        assert_eq!(result.gaps, 0);
        let unrelated =
            vec![said("error", "localization/english/a_l_english.yml", 5, "Missing icon")];
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &unrelated, limit: 10 });
        assert_eq!(result.gaps, 1);
        assert_eq!(result.rows[0].state, "same_location_other_cause");
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
        let other = vec![said("error", "common/men_at_arms_types/x.txt", 3, "unexpected token")];
        let result = compare(&Request { log, mod_dir: &dir, reports: &other, limit: 10 });
        assert_eq!(result.gaps, 1);
        assert_eq!(result.rows[0].tiger, "other lines");
        assert_eq!(result.rows[0].examples, vec!["common/men_at_arms_types/x.txt:17"]);
        // Tiger reported exactly that line, and said the same thing.
        let same = vec![said("error", "common/men_at_arms_types/x.txt", 17, "Unexpected token")];
        let result = compare(&Request { log, mod_dir: &dir, reports: &same, limit: 10 });
        assert_eq!(result.gaps, 0);
        assert_eq!(result.reported_by_tiger, 1);
    }

    #[test]
    fn a_path_with_non_ascii_letters_is_recognised() {
        let dir = mod_with(&["events/événement.txt"]);
        let log = "[04:00:00][E][jomini_script_system.cpp:304]: Script system error!\n  \
                   Script location: file: events/événement.txt line: 9 (a.1:option)\n";
        let result = compare(&Request { log, mod_dir: &dir, reports: &[], limit: 10 });
        assert_eq!(result.about_the_mod, 1);
        assert_eq!(result.rows[0].examples, vec!["events/événement.txt:9"]);
    }

    #[test]
    fn absolute_and_quoted_paths_are_placed_or_reported_unsupported() {
        let dir = mod_with(&["events/a.txt", "common/my folder/b.txt"]);
        let root = dir.display().to_string();
        let absolute = format!(
            "[04:00:00][E][x.cpp:1]: Bad thing 'q' in {root}\\events\\a.txt line: 4\n\
             [04:00:01][E][x.cpp:2]: Bad thing 'q' in file: \"common/my folder/b.txt\" near line: 7\n\
             [04:00:02][E][x.cpp:3]: Bad thing 'q' in C:\\Other\\place\\c.txt line: 2\n"
        );
        let result = compare(&Request { log: &absolute, mod_dir: &dir, reports: &[], limit: 10 });
        assert_eq!(result.about_the_mod, 2, "{result:?}");
        assert_eq!(result.states["no_tiger_report"], 2);
        assert_eq!(result.states["unsupported_format"], 1);
        assert_eq!(total(&result), 3);
        let examples: Vec<&str> =
            result.rows.iter().flat_map(|r| r.examples.iter().map(String::as_str)).collect();
        assert!(examples.contains(&"events/a.txt:4"));
        assert!(examples.contains(&"common/my folder/b.txt:7"));
    }

    #[test]
    fn limit_cuts_the_rows_not_the_counts() {
        let dir = mod_with(&["localization/english/a_l_english.yml", "events/a.txt"]);
        let result = compare(&Request { log: LOG, mod_dir: &dir, reports: &[], limit: 1 });
        assert_eq!(result.gaps, 2);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(total(&result), 5);
    }
}
