//! Requests the user leaves for the assistant in the xTiger app, such as "fix this report" or
//! "bring this mod up to the current game version". The assistant picks them up with
//! `xtiger_pending_requests` and closes them with `xtiger_finish_request`.
//!
//! Each request is one JSON file in `requests/` in the state folder, written whole and then moved
//! into place, so a reader never sees half of one. The app writes and removes them; the server
//! only changes their status.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::journal::{ModRef, Tally};
use crate::runs;

pub const DIR: &str = "requests";
/// How many finished requests are kept; the oldest go first.
pub const KEEP_FINISHED: usize = 40;
/// The most reports one request carries.
pub const MAX_REPORTS: usize = 200;
/// The longest note the user can add.
pub const MAX_NOTE: usize = 1000;
/// The longest outcome the assistant can leave.
pub const MAX_OUTCOME: usize = 600;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Fix the reports that come with the request.
    Fix,
    /// Bring the mod up to the game version it is checked against.
    Update,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// No assistant has seen it yet.
    Waiting,
    /// An assistant has it.
    Taken,
    Done,
    /// The assistant could not or would not do it, and said why.
    Skipped,
}

impl Status {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Waiting | Self::Taken)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub id: String,
    pub kind: Kind,
    /// Milliseconds since the Unix epoch.
    pub created_at: u64,
    #[serde(rename = "mod")]
    pub mod_ref: ModRef,
    /// The game version the mod was checked against.
    #[serde(default)]
    pub game_version: Option<String>,
    /// What the user added in their own words.
    #[serde(default)]
    pub note: Option<String>,
    /// The reports to fix, as the validator wrote them.
    #[serde(default)]
    pub reports: Vec<Value>,
    /// How many reports were picked, when that is more than the request carries.
    #[serde(default)]
    pub reports_total: usize,
    /// For an update: the brief the app prepared.
    #[serde(default)]
    pub brief: Option<String>,
    pub status: Status,
    #[serde(default)]
    pub taken_at: Option<u64>,
    /// The assistant that took it, as it named itself.
    #[serde(default)]
    pub taken_by: Option<String>,
    /// The work session of that assistant.
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub finished_at: Option<u64>,
    /// What the assistant did, or why it skipped the request.
    #[serde(default)]
    pub outcome: Option<String>,
}

/// What the app hands over to make a request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRequest {
    pub kind: Kind,
    pub mod_file: PathBuf,
    #[serde(default)]
    pub mod_name: Option<String>,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub reports: Vec<Value>,
    #[serde(default)]
    pub brief: Option<String>,
}

pub fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join(DIR)
}

fn path_of(state_dir: &Path, id: &str) -> PathBuf {
    dir(state_dir).join(format!("{id}.json"))
}

/// Ids are made of digits, letters and dashes, so one can never point outside the folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn write(state_dir: &Path, request: &Request) -> Result<(), String> {
    let folder = dir(state_dir);
    fs::create_dir_all(&folder).map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    let text = serde_json::to_string_pretty(request).map_err(|e| e.to_string())?;
    let target = path_of(state_dir, &request.id);
    let temp = folder.join(format!(".{}.{}.tmp", request.id, std::process::id()));
    fs::write(&temp, text).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, &target).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot write {}: {e}", target.display())
    })
}

fn read(path: &Path) -> Option<Request> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Every request, newest first. Damaged files are left out.
pub fn list(state_dir: &Path) -> Vec<Request> {
    let mut requests: Vec<Request> = fs::read_dir(dir(state_dir))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .filter_map(|path| read(&path))
                .collect()
        })
        .unwrap_or_default();
    requests.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    requests
}

/// Changes when a request is added, changed or removed.
pub fn stamp(state_dir: &Path) -> String {
    let mut parts: Vec<String> = fs::read_dir(dir(state_dir))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .map(|entry| {
                    let modified = entry
                        .metadata()
                        .and_then(|meta| meta.modified())
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_millis());
                    format!("{}:{modified}", entry.file_name().to_string_lossy())
                })
                .collect()
        })
        .unwrap_or_default();
    parts.sort();
    parts.join("|")
}

fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

fn clean_text(text: Option<&str>, max: usize) -> Option<String> {
    let text = text?.trim();
    (!text.is_empty()).then(|| first_chars(text, max))
}

/// Save a new request from the app.
pub fn add(state_dir: &Path, new: NewRequest, now: u64) -> Result<Request, String> {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    if new.kind == Kind::Fix && new.reports.is_empty() {
        return Err("Pick at least one report to fix.".to_owned());
    }
    let reports_total = new.reports.len();
    let mut reports = new.reports;
    reports.truncate(MAX_REPORTS);
    let request = Request {
        id: format!("{now}-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst)),
        kind: new.kind,
        created_at: now,
        mod_ref: ModRef { name: new.mod_name, file: new.mod_file },
        game_version: new.game_version,
        note: clean_text(new.note.as_deref(), MAX_NOTE),
        reports,
        reports_total,
        brief: new.brief,
        status: Status::Waiting,
        taken_at: None,
        taken_by: None,
        session: None,
        finished_at: None,
        outcome: None,
    };
    write(state_dir, &request)?;
    prune(state_dir);
    Ok(request)
}

/// Keep only the newest finished requests.
fn prune(state_dir: &Path) {
    for old in list(state_dir).into_iter().filter(|r| !r.status.is_open()).skip(KEEP_FINISHED) {
        let _ = fs::remove_file(path_of(state_dir, &old.id));
    }
}

/// Remove a request, as the user asks in the app.
pub fn remove(state_dir: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("No request {id}."));
    }
    match fs::remove_file(path_of(state_dir, id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove the request: {e}")),
    }
}

/// The open requests, oldest first. The waiting ones are marked as taken by `client`.
pub fn take(state_dir: &Path, client: Option<&str>, session: &str, now: u64) -> Vec<Request> {
    let mut open: Vec<Request> =
        list(state_dir).into_iter().filter(|r| r.status.is_open()).collect();
    open.reverse();
    for request in &mut open {
        if request.status == Status::Waiting {
            request.status = Status::Taken;
            request.taken_at = Some(now);
            request.taken_by = client.map(str::to_owned);
            request.session = Some(session.to_owned());
            // Another reader may have it already; the next call tries again.
            let _ = write(state_dir, request);
        }
    }
    open
}

/// Close a request: done, or skipped with the reason.
pub fn finish(
    state_dir: &Path,
    id: &str,
    status: Status,
    outcome: Option<&str>,
    now: u64,
) -> Result<Request, String> {
    let missing = || {
        format!(
            "No request {id}. xtiger_pending_requests lists the open ones; the user may have removed it."
        )
    };
    if !valid_id(id) {
        return Err(missing());
    }
    let mut request = read(&path_of(state_dir, id)).ok_or_else(missing)?;
    if !request.status.is_open() {
        return Err(format!("Request {id} is already closed."));
    }
    request.status = status;
    request.finished_at = Some(now);
    request.outcome = clean_text(outcome, MAX_OUTCOME);
    write(state_dir, &request)?;
    Ok(request)
}

/// How many requests no assistant has seen yet, and how many are open in all.
pub fn counts(state_dir: &Path) -> (usize, usize) {
    let requests = list(state_dir);
    let waiting = requests.iter().filter(|r| r.status == Status::Waiting).count();
    let open = requests.iter().filter(|r| r.status.is_open()).count();
    (waiting, open)
}

/// A request as the assistant gets it: the reports as `xtiger_reports` shows them.
#[derive(Debug, Serialize)]
pub struct Pending {
    pub id: String,
    pub kind: Kind,
    pub asked: String,
    #[serde(rename = "mod")]
    pub mod_ref: ModRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<runs::Row>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more_reports: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    pub what_to_do: &'static str,
}

/// "5 minutes ago", for the assistant.
pub fn ago(then: u64, now: u64) -> String {
    let minutes = now.saturating_sub(then).div_euclid(60_000);
    match minutes {
        0 => "just now".to_owned(),
        1 => "1 minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        60..120 => "1 hour ago".to_owned(),
        120..2880 => format!("{} hours ago", minutes.div_euclid(60)),
        _ => format!("{} days ago", minutes.div_euclid(1440)),
    }
}

pub fn pending(request: Request, now: u64) -> Pending {
    let what_to_do = match request.kind {
        Kind::Fix => {
            "Fix these reports in the mod's files, validate the mod again to check, then call \
             xtiger_finish_request."
        }
        Kind::Update => {
            "Bring the mod up to the game version: validate it, fix the errors first, then the warnings, \
             and validate again after each round. Read the brief first. Call xtiger_finish_request at \
             the end, with what is left."
        }
    };
    let more = request.reports_total.saturating_sub(request.reports.len());
    Pending {
        asked: ago(request.created_at, now),
        reports: request.reports.iter().map(runs::row).collect(),
        more_reports: (more > 0).then_some(more),
        id: request.id,
        kind: request.kind,
        mod_ref: request.mod_ref,
        game_version: request.game_version,
        note: request.note,
        brief: request.brief,
        what_to_do,
    }
}

/// What the app knows about the mod, for the update brief.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefMod {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub supported_version: Option<String>,
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

fn severity(report: &Value) -> &str {
    report.get("severity").and_then(Value::as_str).unwrap_or("tips")
}

fn first_place(report: &Value) -> Option<(String, Option<u64>)> {
    let loc = report.get("locations")?.as_array()?.first()?;
    let path = loc.get("path")?.as_str()?.replace('\\', "/");
    Some((path, loc.get("linenr").and_then(Value::as_u64)))
}

/// One report as a Markdown list item: its place, then its message.
fn brief_line(report: &Value) -> String {
    let message = report.get("message").and_then(Value::as_str).unwrap_or("");
    let place = first_place(report)
        .map(|(path, line)| match line {
            Some(line) => format!(" `{path}:{line}`"),
            None => format!(" `{path}`"),
        })
        .unwrap_or_default();
    format!("-{place} {message}")
}

/// A Markdown brief on what it takes to bring a mod up to `game_version`: what the validator
/// found, grouped by key, worst first, with examples and the files that need the most work.
pub fn update_brief(info: &BriefMod, game_version: Option<&str>, reports: &[Value]) -> String {
    const KEYS_SHOWN: usize = 15;
    const EXAMPLES: usize = 3;
    const FILES_SHOWN: usize = 12;
    let rank = |severity: &str| runs::SEVERITIES.iter().position(|s| *s == severity).unwrap_or(0);

    let mut out = format!("# Update brief: {}\n\n", info.name);
    let game = game_version.unwrap_or("the current version");
    let _ = writeln!(out, "- Checked against: CK3 {game}");
    let _ = writeln!(
        out,
        "- The mod says it supports: {}",
        info.supported_version.as_deref().unwrap_or("(not set)")
    );
    if let Some(version) = &info.version {
        let _ = writeln!(out, "- Mod version: {version}");
    }
    if let Some(dir) = &info.dir {
        let _ = writeln!(out, "- Folder: {}", dir.display());
    }

    let tally = Tally::from_pairs(reports.iter().map(|report| (severity(report), 1)));
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let _ = write!(
        out,
        "\n## What the validator found\n\n{}: {} fatal, {}, {}, {} untidy, {}.\n",
        count(tally.total(), "report", "reports"),
        tally.fatal,
        count(tally.error, "error", "errors"),
        count(tally.warning, "warning", "warnings"),
        tally.untidy,
        count(tally.tips, "tip", "tips"),
    );
    if reports.is_empty() {
        out.push_str("\nNothing to fix: bump `supported_version` in the descriptor and test it in the game.\n");
        return out;
    }

    // Files in a folder the game no longer reads: until they move, all they define is reported
    // missing elsewhere, so these come before everything else.
    let moved: Vec<&Value> = reports
        .iter()
        .filter(|report| report.get("key").and_then(Value::as_str) == Some("filename"))
        .collect();
    if !moved.is_empty() {
        out.push_str(
            "\n## Start here: files the game does not read\n\nThe game does not load these files where \
             they are, so whatever they define shows up as missing in other reports. Move or rename \
             them first, then validate again: many of the reports below may go with them.\n\n",
        );
        for report in moved.iter().take(KEYS_SHOWN) {
            let _ = writeln!(out, "{}", brief_line(report));
        }
        if moved.len() > KEYS_SHOWN {
            let _ = writeln!(out, "- and {} more", moved.len() - KEYS_SHOWN);
        }
    }

    // Keys, worst severity first, then the most reports.
    let mut keys: Vec<(String, Vec<&Value>)> = Vec::new();
    for report in reports {
        let key = report.get("key").and_then(Value::as_str).unwrap_or("(no key)").to_owned();
        match keys.iter_mut().find(|(k, _)| *k == key) {
            Some((_, items)) => items.push(report),
            None => keys.push((key, vec![report])),
        }
    }
    let worst = |items: &[&Value]| items.iter().map(|r| rank(severity(r))).max().unwrap_or(0);
    keys.sort_by(|a, b| worst(&b.1).cmp(&worst(&a.1)).then(b.1.len().cmp(&a.1.len())));
    out.push_str("\n## By key, worst first\n");
    for (key, items) in keys.iter().take(KEYS_SHOWN) {
        let top = items.iter().max_by_key(|r| rank(severity(r))).map_or("tips", |r| severity(r));
        let _ = write!(out, "\n### {key} ({top}, {})\n\n", items.len());
        for report in items.iter().take(EXAMPLES) {
            let _ = writeln!(out, "{}", brief_line(report));
        }
        if items.len() > EXAMPLES {
            let _ = writeln!(out, "- and {} more", items.len() - EXAMPLES);
        }
    }
    if keys.len() > KEYS_SHOWN {
        let _ = writeln!(out, "\nAnd {} more keys.", keys.len() - KEYS_SHOWN);
    }

    let files =
        runs::most_common(reports.iter().filter_map(|r| first_place(r).map(|(path, _)| path)));
    out.push_str("\n## Files with the most reports\n\n");
    for (file, n) in files.iter().take(FILES_SHOWN) {
        let _ = writeln!(out, "- `{file}`: {n}");
    }
    out.push_str(
        "\n## How to go about it\n\n1. Fix the fatal reports and errors first: they break loading or whole \
         features.\n2. Then the warnings, key by key.\n3. Validate again after each round and compare with \
         the run before.\n4. Set `supported_version` in the descriptor when it loads cleanly, and test it \
         in the game.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::json;

    fn report(severity: &str, key: &str, path: &str) -> Value {
        json!({"severity": severity, "key": key, "message": format!("{key} here"),
               "locations": [{"path": path, "linenr": 3, "fullpath": format!("/m/{path}")}]})
    }

    fn fix(reports: Vec<Value>) -> NewRequest {
        NewRequest {
            kind: Kind::Fix,
            mod_file: PathBuf::from("/m/silk.mod"),
            mod_name: Some("Silk Road".to_owned()),
            game_version: Some("1.20".to_owned()),
            note: Some("  Keep the old names.  ".to_owned()),
            reports,
            brief: None,
        }
    }

    #[test]
    fn a_request_goes_from_waiting_to_done() {
        let tmp = TempDir::new();
        let added =
            add(&tmp, fix(vec![report("error", "missing-item", "events/a.txt")]), 1000).unwrap();
        assert_eq!(added.note.as_deref(), Some("Keep the old names."));
        assert_eq!(counts(&tmp), (1, 1));

        let taken = take(&tmp, Some("Claude Code"), "s1", 2000);
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].status, Status::Taken);
        assert_eq!(list(&tmp)[0].taken_by.as_deref(), Some("Claude Code"));
        assert_eq!(counts(&tmp), (0, 1));
        // Still open: a later call sees it again, without taking it twice.
        let again = take(&tmp, Some("Other"), "s2", 3000);
        assert_eq!(again[0].taken_by.as_deref(), Some("Claude Code"));

        let done =
            finish(&tmp, &added.id, Status::Done, Some("Fixed the item name."), 4000).unwrap();
        assert_eq!(done.outcome.as_deref(), Some("Fixed the item name."));
        assert_eq!(counts(&tmp), (0, 0));
        assert_eq!(take(&tmp, None, "s3", 5000).len(), 0);
        assert!(finish(&tmp, &added.id, Status::Done, None, 6000).unwrap_err().contains("already"));
        assert!(finish(&tmp, "../x", Status::Done, None, 6000).unwrap_err().contains("No request"));
    }

    #[test]
    fn requests_can_be_removed_and_old_ones_go() {
        let tmp = TempDir::new();
        assert!(add(&tmp, fix(vec![]), 1).unwrap_err().contains("at least one"));
        for i in 0..KEEP_FINISHED + 3 {
            let r = add(&tmp, fix(vec![report("error", "k", "a.txt")]), 10 + i as u64).unwrap();
            finish(&tmp, &r.id, Status::Skipped, Some("No"), 100).unwrap();
        }
        let open = add(&tmp, fix(vec![report("error", "k", "a.txt")]), 1000).unwrap();
        let all = list(&tmp);
        assert_eq!(all.len(), KEEP_FINISHED + 1);
        assert_eq!(all[0].id, open.id);
        let before = stamp(&tmp);
        remove(&tmp, &open.id).unwrap();
        assert_ne!(stamp(&tmp), before);
        assert_eq!(counts(&tmp), (0, 0));
        remove(&tmp, &open.id).unwrap();
    }

    #[test]
    fn the_assistant_gets_rows() {
        let many: Vec<Value> =
            (0..MAX_REPORTS + 5).map(|_| report("warning", "k", "a.txt")).collect();
        let request = add(&TempDir::new(), fix(many), 0).unwrap();
        let pending = pending(request, 3 * 60_000);
        assert_eq!(pending.asked, "3 minutes ago");
        assert_eq!(pending.reports.len(), MAX_REPORTS);
        assert_eq!(pending.more_reports, Some(5));
        assert_eq!(pending.reports[0].place, "a.txt:3");
        assert_eq!(ago(0, 3 * 86_400_000), "3 days ago");
    }

    #[test]
    fn the_brief_puts_the_worst_first() {
        let info = BriefMod {
            name: "Better Courts".to_owned(),
            supported_version: Some("1.12.*".to_owned()),
            ..BriefMod::default()
        };
        let reports = vec![
            report("warning", "unused", "common/a.txt"),
            report("warning", "unused", "common/a.txt"),
            report("error", "missing-item", "events/b.txt"),
        ];
        let brief = update_brief(&info, Some("1.20"), &reports);
        assert!(brief.starts_with("# Update brief: Better Courts"));
        assert!(brief.contains("- The mod says it supports: 1.12.*"));
        assert!(brief.contains("3 reports: 0 fatal, 1 error, 2 warnings"));
        assert!(brief.find("### missing-item").unwrap() < brief.find("### unused").unwrap());
        assert!(brief.contains("- `common/a.txt`: 2"));
        assert!(!brief.contains("Start here"));
        assert!(update_brief(&info, None, &[]).contains("Nothing to fix"));

        // A folder the game no longer reads comes first, however few reports it has.
        let mut outdated = reports.clone();
        outdated.extend((0..5).map(|_| report("error", "missing-item", "events/c.txt")));
        outdated.push(report("error", "filename", "common/religion/religions/a.txt"));
        let brief = update_brief(&info, Some("1.20"), &outdated);
        let start = brief.find("## Start here").unwrap();
        assert!(start < brief.find("## By key").unwrap());
        assert!(brief[start..].contains("- `common/religion/religions/a.txt:3` filename here"));
    }
}
