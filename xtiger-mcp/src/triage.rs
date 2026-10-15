//! Review notes on diagnostics that do not hide them.
//!
//! A note says what a person or assistant decided about one diagnostic: it is an intentional
//! override, a test assertion, a defect still to fix, or only formatting. The diagnostic stays in
//! the run and in every count. A note belongs to the diagnostic's identity (its key, message,
//! file and the definition it is in) and to the hash of that definition's text, so when the
//! definition changes the note stops applying and the diagnostic needs a new look.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::overrides::definitions;

pub const KINDS: [&str; 4] =
    ["intentional_override", "test_assertion", "unresolved_defect", "advisory_formatting"];

/// The most examples listed for a group.
const EXAMPLES: usize = 5;
/// The most diagnostics one `annotate` call may cover.
pub const MAX_BULK: usize = 25;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Annotation {
    /// Identifies the diagnostic: a hash of key, message, file and definition.
    pub id: String,
    pub kind: String,
    pub note: String,
    pub by: String,
    pub key: String,
    pub message: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// The hash of the definition's text when the note was made. Without one (the file or the
    /// definition could not be read) the note cannot tell when the text changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<String>,
    pub at: u64,
}

/// What identifies one diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub id: String,
    pub key: String,
    pub message: String,
    pub path: String,
    pub definition: Option<String>,
    pub block_hash: Option<String>,
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}

fn hash(parts: &[&str]) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize().iter().take(8).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn block_hash(body: &[String]) -> String {
    hash(&[&body.join("\n")])
}

/// The first location of a diagnostic, as its path (forward slashes) and line.
fn place(report: &Value) -> (String, usize) {
    let first = report.get("locations").and_then(Value::as_array).and_then(|l| l.first());
    let path = first.map_or("", |loc| text(loc, "path")).replace('\\', "/");
    let line = first
        .and_then(|loc| loc.get("linenr"))
        .and_then(Value::as_u64)
        .map_or(0, |n| usize::try_from(n).unwrap_or(0));
    (path, line)
}

/// A message as an identity holds it: its words, separated by one space.
fn message_of(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The identity of a diagnostic of the mod in `mod_dir`.
#[must_use]
pub fn identity(report: &Value, mod_dir: &Path) -> Identity {
    let key = text(report, "key").to_owned();
    let message = message_of(text(report, "message"));
    let (path, line) = place(report);
    let found = (line > 0 && !path.is_empty())
        .then(|| fs::read(mod_dir.join(&path)).ok())
        .flatten()
        .and_then(|bytes| {
            let defs = definitions(&String::from_utf8_lossy(&bytes));
            defs.into_iter().rfind(|def| def.line <= line)
        });
    let definition = found.as_ref().map(|def| def.key.clone());
    let block_hash = found.as_ref().map(|def| block_hash(&def.body));
    let id = hash(&[&key, &message, &path, definition.as_deref().unwrap_or("")]);
    Identity { id, key, message, path, definition, block_hash }
}

/// The diagnostics of `reports` that a selector picks: by key, a part of the path and a line
/// (0 for any).
#[must_use]
pub fn select<'a>(reports: &'a [Value], key: &str, path: &str, line: usize) -> Vec<&'a Value> {
    reports
        .iter()
        .filter(|report| text(report, "key") == key)
        .filter(|report| {
            let locations = report.get("locations").and_then(Value::as_array);
            locations.is_some_and(|all| {
                all.iter().any(|loc| {
                    text(loc, "path").replace('\\', "/").contains(path)
                        && (line == 0
                            || loc.get("linenr").and_then(Value::as_u64)
                                == u64::try_from(line).ok())
                })
            })
        })
        .collect()
}

/// The diagnostics of `reports` that one note is for: the same key, the same message (its words
/// compared, not its spaces) and the same first place, path and line. Unlike `select`, a diagnostic
/// that only shares that line is not picked.
#[must_use]
pub fn select_spot<'a>(
    reports: &'a [Value],
    key: &str,
    path: &str,
    line: usize,
    message: &str,
) -> Vec<&'a Value> {
    let key = key.trim();
    let path = path.trim().replace('\\', "/");
    let message = message_of(message);
    reports
        .iter()
        .filter(|report| text(report, "key") == key)
        .filter(|report| message_of(text(report, "message")) == message)
        .filter(|report| {
            let (at, at_line) = place(report);
            at == path && at_line == line
        })
        .collect()
}

/// A note for a diagnostic.
#[must_use]
pub fn annotation(identity: &Identity, kind: &str, note: &str, by: &str, at: u64) -> Annotation {
    Annotation {
        id: identity.id.clone(),
        kind: kind.to_owned(),
        note: note.trim().to_owned(),
        by: by.to_owned(),
        key: identity.key.clone(),
        message: identity.message.clone(),
        path: identity.path.clone(),
        definition: identity.definition.clone(),
        block_hash: identity.block_hash.clone(),
        at,
    }
}

/// Where one diagnostic is listed: its key and its first location (path with forward slashes).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Spot {
    pub key: String,
    pub path: String,
    pub line: usize,
}

/// A diagnostic that has a note: where its first location is, its normalised message, and the
/// identity of its note. `applies` says whether that note still applies.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NotedSpot {
    pub key: String,
    pub path: String,
    pub line: usize,
    pub message: String,
    pub id: String,
    pub applies: bool,
}

/// How a run's diagnostics stand against the notes. Every diagnostic is still in the run: this
/// only says which have been looked at.
#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Triage {
    pub diagnostics: usize,
    /// With a note that still applies, by kind.
    pub reviewed: BTreeMap<String, usize>,
    /// Their definition changed after the note was made.
    pub needs_review: usize,
    /// Which diagnostics those are, by the key, file and line a window lists them under.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_look: Vec<Spot>,
    /// Every diagnostic that has a note, with the identity its note holds. Two diagnostics can share
    /// a key, a message and a file (in different definitions), so a window matches each report to
    /// its own note through this list rather than by key and message alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub noted: Vec<NotedSpot>,
    /// No note.
    pub unreviewed: usize,
    /// Notes whose diagnostic is no longer in the run.
    pub notes_without_diagnostic: usize,
    /// The first diagnostics that need a look, as `file:line key`.
    pub examples: Vec<String>,
}

impl Triage {
    /// A diagnostic that is a defect, or that nobody has looked at, keeps the update open.
    #[must_use]
    pub fn open_items(&self) -> usize {
        self.needs_review
            + self.unreviewed
            + self.reviewed.get("unresolved_defect").copied().unwrap_or(0)
    }
}

#[must_use]
pub fn triage(reports: &[Value], notes: &[Annotation], mod_dir: &Path) -> Triage {
    let mut out = Triage { diagnostics: reports.len(), ..Triage::default() };
    let mut seen = Vec::new();
    for report in reports {
        let who = identity(report, mod_dir);
        let note = notes.iter().rev().find(|n| n.id == who.id);
        let (path, line) = place(report);
        let applies = note.is_some_and(|n| n.block_hash == who.block_hash);
        match note {
            Some(n) if applies => *out.reviewed.entry(n.kind.clone()).or_default() += 1,
            Some(_) => {
                out.needs_review += 1;
                out.needs_look.push(Spot { key: who.key.clone(), path: path.clone(), line });
            }
            None => out.unreviewed += 1,
        }
        if let Some(n) = note {
            out.noted.push(NotedSpot {
                key: who.key.clone(),
                path: path.clone(),
                line,
                message: who.message.clone(),
                id: n.id.clone(),
                applies,
            });
        }
        if (note.is_none() || !applies) && out.examples.len() < EXAMPLES {
            out.examples.push(format!("{path}:{line} {}", who.key));
        }
        seen.push(who.id);
    }
    out.notes_without_diagnostic = notes.iter().filter(|n| !seen.contains(&n.id)).count();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::json;

    fn report(line: u64) -> Value {
        json!({"severity": "warning", "key": "duplicate-item", "message": "ev.1 is redefined",
               "locations": [{"path": "events/x.txt", "linenr": line}]})
    }

    fn write(dir: &Path, text: &str) {
        fs::create_dir_all(dir.join("events")).unwrap();
        fs::write(dir.join("events/x.txt"), text).unwrap();
    }

    #[test]
    fn a_note_applies_until_its_definition_changes_and_never_hides_the_diagnostic() {
        let tmp = TempDir::new();
        write(&tmp, "ev.1 = {\n\ttype = character_event\n}\n");
        let reports = vec![report(2)];
        let who = identity(&reports[0], &tmp);
        assert_eq!(who.definition.as_deref(), Some("ev.1"));
        let notes = vec![annotation(&who, "intentional_override", "replaces vanilla", "t", 1)];
        let now = triage(&reports, &notes, &tmp);
        assert_eq!((now.diagnostics, now.unreviewed, now.needs_review), (1, 0, 0));
        assert_eq!(now.reviewed.get("intentional_override"), Some(&1));
        assert_eq!(now.open_items(), 0);

        write(&tmp, "ev.1 = {\n\ttype = character_event\n\tgold = 5\n}\n");
        let later = triage(&reports, &notes, &tmp);
        assert_eq!((later.diagnostics, later.needs_review), (1, 1));
        assert_eq!(
            later.needs_look,
            vec![Spot { key: "duplicate-item".into(), path: "events/x.txt".into(), line: 2 }]
        );
        assert!(later.reviewed.is_empty());
        assert_eq!(later.open_items(), 1);
        assert_eq!(later.noted.iter().map(|n| n.applies).collect::<Vec<_>>(), vec![false]);
    }

    #[test]
    fn a_defect_note_keeps_the_update_open_and_a_new_diagnostic_is_unreviewed() {
        let tmp = TempDir::new();
        write(&tmp, "ev.1 = {\n\ttype = character_event\n}\nev.2 = {\n}\n");
        let first = report(2);
        let who = identity(&first, &tmp);
        let notes = vec![annotation(&who, "unresolved_defect", "fix later", "t", 1)];
        let reports = vec![
            first,
            json!({"key": "other", "message": "m", "locations": [{"path": "events/x.txt", "linenr": 5}]}),
        ];
        let now = triage(&reports, &notes, &tmp);
        assert_eq!((now.unreviewed, now.open_items()), (1, 2));
    }

    #[test]
    fn a_selector_matches_by_key_path_and_line() {
        let reports = vec![report(2), report(9)];
        assert_eq!(select(&reports, "duplicate-item", "events", 0).len(), 2);
        assert_eq!(select(&reports, "duplicate-item", "events", 9).len(), 1);
        assert!(select(&reports, "other", "events", 0).is_empty());
    }

    #[test]
    fn a_note_is_for_one_report_when_another_has_the_same_key_and_message() {
        let tmp = TempDir::new();
        write(
            &tmp,
            "ev.1 = {\n\ttype = character_event\n}\nev.2 = {\n\ttype = character_event\n}\n",
        );
        let reports = vec![report(2), report(5)];
        let who = identity(&reports[0], &tmp);
        assert_ne!(who.id, identity(&reports[1], &tmp).id);
        let notes = vec![annotation(&who, "intentional_override", "replaces vanilla", "t", 1)];
        let now = triage(&reports, &notes, &tmp);
        assert_eq!((now.reviewed.get("intentional_override"), now.unreviewed), (Some(&1), 1));
        assert_eq!(
            now.noted,
            vec![NotedSpot {
                key: "duplicate-item".into(),
                path: "events/x.txt".into(),
                line: 2,
                message: "ev.1 is redefined".into(),
                id: who.id.clone(),
                applies: true,
            }]
        );
    }

    #[test]
    fn a_spot_is_one_report_even_when_another_shares_its_line() {
        let other = json!({"key": "duplicate-item", "message": "ev.2 is redefined",
                           "locations": [{"path": "events/x.txt", "linenr": 2}]});
        let reports = vec![report(2), other];
        let picked =
            select_spot(&reports, "duplicate-item", "events/x.txt", 2, " ev.1  is redefined ");
        assert_eq!(picked.len(), 1);
        assert_eq!(text(picked[0], "message"), "ev.1 is redefined");
        assert_eq!(select(&reports, "duplicate-item", "events/x.txt", 2).len(), 2);
    }

    #[test]
    fn a_spot_needs_its_message_its_line_and_its_whole_path() {
        let reports = vec![report(9)];
        let spot = |path: &str, line: usize, message: &str| {
            select_spot(&reports, "duplicate-item", path, line, message).len()
        };
        assert_eq!(spot("events\\x.txt", 9, "ev.1 is redefined"), 1);
        assert_eq!(spot("events/x.txt", 9, "ev.2 is redefined"), 0);
        assert_eq!(spot("events/x.txt", 4, "ev.1 is redefined"), 0);
        assert_eq!(spot("events", 9, "ev.1 is redefined"), 0);
    }
}
