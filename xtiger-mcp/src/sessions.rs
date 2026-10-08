//! Work sessions: the calls of one job, such as "Update Better Courts to 1.20", and what came of
//! them.
//!
//! A server starts a session with its first call. The next one starts when the assistant names a
//! different job with `xtiger_session`, after it wraps the job up, or after [`GAP_MS`] without
//! calls. Every journal entry carries its session's id, so the app can group the calls and sum
//! them up: how the reports went down from one validation to the next, what was fixed, what is
//! left, and which of the mod's files changed meanwhile.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::journal::{self, Entry, ModRef, Outcome, Tally};
use crate::mods::{self, ModInfo};
use crate::runs::{self, Row};

/// A pause this long ends a session.
pub const GAP_MS: u64 = 30 * 60 * 1000;
/// The tool that names a job and wraps it up.
pub const TOOL: &str = "xtiger_session";
/// The longest title that is kept.
pub const MAX_TITLE: usize = 80;
/// The longest wrap-up that is kept.
pub const MAX_WRAP_UP: usize = 600;
/// How many reports that are left a summary lists, per mod.
const PENDING_SHOWN: usize = 5;
/// How many changed files a summary lists, per mod.
const FILES_SHOWN: usize = 50;
/// A mod with more files than this is not searched any further for changed ones.
const FILES_LOOKED_AT: usize = 20_000;
/// Files saved this soon after the last call still count as part of the session.
const AFTER_LAST_CALL_MS: u64 = 2 * 60 * 1000;

#[derive(Debug, Clone)]
struct Current {
    id: String,
    title: Option<String>,
    last_call: u64,
    wrapped_up: bool,
}

/// The session of one server.
#[derive(Debug, Default)]
pub struct Tracker {
    current: Option<Current>,
}

impl Tracker {
    /// The session a call that starts at `now` belongs to. `title` is the job the call names.
    pub fn join(&mut self, now: u64, title: Option<&str>) -> String {
        let fresh = match &self.current {
            None => true,
            Some(current) => {
                current.wrapped_up
                    || now.saturating_sub(current.last_call) > GAP_MS
                    || matches!((title, &current.title), (Some(new), Some(old)) if new != old)
            }
        };
        let current = match &mut self.current {
            Some(current) if !fresh => current,
            slot => slot.insert(Current {
                id: format!("{}-{now}", std::process::id()),
                title: None,
                last_call: now,
                wrapped_up: false,
            }),
        };
        current.last_call = now;
        if let Some(title) = title {
            current.title = Some(title.to_owned());
        }
        current.id.clone()
    }

    /// A call of the session ended at `now`; a long one keeps the session going.
    pub fn seen(&mut self, now: u64) {
        if let Some(current) = &mut self.current {
            current.last_call = current.last_call.max(now);
        }
    }

    /// The job is done: the next call starts a new session.
    pub fn wrap_up(&mut self) {
        if let Some(current) = &mut self.current {
            current.wrapped_up = true;
        }
    }
}

/// The finished calls of a session, oldest first.
pub fn entries_of(state_dir: &Path, session: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = journal::read_entries(state_dir, usize::MAX)
        .into_iter()
        .filter(|entry| entry.session.as_deref() == Some(session))
        .collect();
    entries.reverse();
    entries
}

/// One validation of a mod in the session.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pass {
    pub at: u64,
    pub run_id: Option<String>,
    pub found: Tally,
}

/// A file of the mod that changed during the session.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Touched {
    /// The path inside the mod, with `/`.
    pub path: String,
    pub full_path: PathBuf,
    pub modified_at: u64,
}

/// What became of one mod in the session.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModProgress {
    #[serde(rename = "mod")]
    pub mod_ref: ModRef,
    pub passes: Vec<Pass>,
    /// Between the first and the last validation of the session, when both runs are still saved.
    pub fixed: Option<usize>,
    pub new: Option<usize>,
    /// The worst reports of the last validation, and how many it had in all.
    pub pending: Vec<Row>,
    pub pending_total: usize,
    /// The run of the last validation, to open its results.
    pub last_run_id: Option<String>,
    /// The mod's files that changed during the session, newest first.
    pub files: Vec<Touched>,
    pub files_total: usize,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub session: String,
    pub title: Option<String>,
    /// What the assistant said when it wrapped the job up.
    pub wrap_up: Option<String>,
    /// Whether the session is over: wrapped up, or quiet for [`GAP_MS`].
    pub ended: bool,
    pub calls: usize,
    pub started_at: u64,
    pub last_at: u64,
    pub mods: Vec<ModProgress>,
}

/// The wrap-up of a session, if the assistant gave one.
fn wrap_up_of(entries: &[Entry]) -> Option<String> {
    entries
        .iter()
        .rev()
        .find(|entry| entry.tool == TOOL && entry.args.contains_key("wrap_up"))
        .and_then(|entry| entry.summary.clone())
}

fn title_of(entries: &[Entry]) -> Option<String> {
    entries.iter().rev().find_map(|entry| {
        (entry.tool == TOOL).then(|| journal::clean_line(entry.args.get("title"), MAX_TITLE))?
    })
}

fn severity_rank(row: &Row) -> usize {
    match row.severity.as_deref() {
        Some("fatal") => 0,
        Some("error") => 1,
        Some("warning") => 2,
        Some("untidy") => 3,
        _ => 4,
    }
}

/// The folder that holds a mod's files.
fn mod_dir(mod_file: &Path, mods: &[ModInfo]) -> Option<PathBuf> {
    if let Some(info) = mods.iter().find(|info| info.mod_file == mod_file) {
        return Some(info.dir.clone());
    }
    let dir = mod_file.parent()?;
    (mod_file.file_name()? == "descriptor.mod")
        .then(|| mods::read_mod_folder(dir))?
        .map(|info| info.dir)
}

fn millis(time: std::time::SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The files under `dir` changed between `from` and `until`, newest first.
fn touched(dir: &Path, from: u64, until: u64) -> Vec<Touched> {
    let mut found = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    let mut looked_at = 0;
    while let Some(folder) = folders.pop() {
        let Ok(items) = fs::read_dir(&folder) else { continue };
        for item in items.filter_map(Result::ok) {
            looked_at += 1;
            if looked_at > FILES_LOOKED_AT {
                break;
            }
            // Hidden folders such as .git change on their own.
            if item.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(meta) = item.metadata() else { continue };
            let path = item.path();
            if meta.is_dir() {
                folders.push(path);
                continue;
            }
            let modified = meta.modified().map_or(0, millis);
            if modified >= from && modified <= until {
                let inside = path.strip_prefix(dir).unwrap_or(&path);
                let parts: Vec<String> = inside
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                found.push(Touched {
                    path: parts.join("/"),
                    full_path: path,
                    modified_at: modified,
                });
            }
        }
    }
    found.sort_by(|a, b| b.modified_at.cmp(&a.modified_at).then_with(|| a.path.cmp(&b.path)));
    found
}

fn progress_of(
    mod_ref: &ModRef,
    entries: &[Entry],
    runs_dir: &Path,
    mods: &[ModInfo],
    (from, until): (u64, u64),
) -> ModProgress {
    let passes: Vec<Pass> = entries
        .iter()
        .filter(|entry| {
            entry.tool == "xtiger_validate"
                && entry.outcome == Some(Outcome::Ok)
                && entry.mod_ref.as_ref().is_some_and(|other| other.file == mod_ref.file)
        })
        .filter_map(|entry| {
            Some(Pass {
                at: entry.finished_at.unwrap_or(entry.started_at),
                run_id: entry.run_id.clone(),
                found: entry.found?,
            })
        })
        .collect();
    let load = |pass: Option<&Pass>| runs::saved_run(runs_dir, pass?.run_id.as_deref()?);
    let last = load(passes.last());
    let (fixed, new) = match (passes.len() > 1).then(|| load(passes.first())).flatten() {
        Some(first) if last.is_some() => {
            let last = last.as_ref().map_or(&[][..], Vec::as_slice);
            let (new, fixed) = runs::compare(&first, last);
            (Some(fixed), Some(new.len()))
        }
        _ => (None, None),
    };
    let mut pending: Vec<Row> = last.iter().flatten().map(runs::row).collect();
    let pending_total =
        last.as_ref().map_or_else(|| passes.last().map_or(0, |pass| pass.found.total()), Vec::len);
    pending.sort_by_key(severity_rank);
    pending.truncate(PENDING_SHOWN);
    let mut files =
        mod_dir(&mod_ref.file, mods).map(|dir| touched(&dir, from, until)).unwrap_or_default();
    let files_total = files.len();
    files.truncate(FILES_SHOWN);
    ModProgress {
        mod_ref: mod_ref.clone(),
        last_run_id: passes.last().and_then(|pass| pass.run_id.clone()),
        passes,
        fixed,
        new,
        pending,
        pending_total,
        files,
        files_total,
    }
}

/// Sum up a session from its calls (`entries`, oldest first). `state_dir` holds the saved runs and
/// `mods` tells where each mod's files are.
pub fn summarize(
    state_dir: &Path,
    session: &str,
    entries: &[Entry],
    mods: &[ModInfo],
    now: u64,
) -> Summary {
    let started_at = entries.first().map_or(now, |entry| entry.started_at);
    let last_at = entries
        .iter()
        .map(|entry| entry.finished_at.unwrap_or(entry.started_at))
        .max()
        .unwrap_or(started_at);
    let wrap_up = wrap_up_of(entries);
    let ended = wrap_up.is_some() || now.saturating_sub(last_at) > GAP_MS;
    let until = if ended { last_at + AFTER_LAST_CALL_MS } else { now };

    // The mods in the order the session first touched them.
    let mut seen: HashSet<&Path> = HashSet::new();
    let worked_on: Vec<&ModRef> = entries
        .iter()
        .filter_map(|entry| entry.mod_ref.as_ref())
        .filter(|mod_ref| seen.insert(&mod_ref.file))
        .collect();
    let runs_dir = state_dir.join("runs");
    let mods = worked_on
        .into_iter()
        .map(|mod_ref| progress_of(mod_ref, entries, &runs_dir, mods, (started_at, until)))
        .collect();
    Summary {
        session: session.to_owned(),
        title: title_of(entries),
        wrap_up,
        ended,
        calls: entries.iter().filter(|entry| entry.tool != TOOL).count(),
        started_at,
        last_at,
        mods,
    }
}

/// The reports of a saved run, used by tests to build runs without the validator.
#[cfg(test)]
pub(crate) fn report(severity: &str, key: &str, path: &str) -> serde_json::Value {
    serde_json::json!({
        "severity": severity,
        "key": key,
        "message": format!("{key} in {path}"),
        "locations": [{"path": path, "fullpath": path, "linenr": 1, "column": 1, "line": "x = y"}],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::{Map, Value};

    fn call(tool: &str, session: &str, at: u64) -> Entry {
        Entry {
            id: format!("1-{at}"),
            tool: tool.to_owned(),
            client: None,
            reason: None,
            mod_ref: None,
            args: Map::new(),
            started_at: at,
            finished_at: Some(at + 10),
            duration_ms: Some(10),
            outcome: Some(Outcome::Ok),
            summary: Some("done".to_owned()),
            progress: None,
            run_id: None,
            found: None,
            session: Some(session.to_owned()),
        }
    }

    #[test]
    fn sessions_start_and_end_with_the_job() {
        let mut tracker = Tracker::default();
        let first = tracker.join(1000, None);
        assert_eq!(tracker.join(2000, None), first);
        // Naming the job keeps the session the calls so far belong to.
        assert_eq!(tracker.join(3000, Some("Fix the events")), first);
        assert_eq!(tracker.join(4000, Some("Fix the events")), first);
        let second = tracker.join(5000, Some("Update to 1.20"));
        assert_ne!(second, first);
        tracker.wrap_up();
        let third = tracker.join(6000, None);
        assert_ne!(third, second);
        // A long call keeps the session going; a long pause ends it.
        tracker.seen(6000 + GAP_MS);
        assert_eq!(tracker.join(7000 + GAP_MS, None), third);
        assert_ne!(tracker.join(8000 + 3 * GAP_MS, None), third);
    }

    #[test]
    fn a_summary_shows_the_way_down() {
        let tmp = TempDir::new();
        let state = tmp.join("state");
        let runs_dir = state.join("runs");
        let mod_dir = tmp.join("my_mod");
        fs::create_dir_all(mod_dir.join("events")).unwrap();
        fs::create_dir_all(mod_dir.join(".git")).unwrap();
        let mod_file = mod_dir.join("descriptor.mod");
        fs::write(&mod_file, "name=\"My Mod\"\n").unwrap();
        let set_time = |path: &Path, at: u64| {
            let file = fs::File::options().write(true).open(path).unwrap();
            file.set_modified(UNIX_EPOCH + std::time::Duration::from_millis(at)).unwrap();
        };
        set_time(&mod_file, 1_000_000);

        let a = report("error", "missing-item", "events/a.txt");
        let b = report("warning", "unused-field", "events/b.txt");
        let c = report("tips", "tip", "events/c.txt");
        let d = report("fatal", "parse", "events/d.txt");
        runs::save_test_run(
            &runs_dir,
            "20260101-000001-my_mod",
            &mod_file,
            &[a.clone(), b.clone(), c.clone()],
        );
        runs::save_test_run(
            &runs_dir,
            "20260101-000002-my_mod",
            &mod_file,
            &[c.clone(), d.clone()],
        );

        let now = journal::now_ms();
        fs::write(mod_dir.join("events").join("a.txt"), "fixed").unwrap();
        fs::write(mod_dir.join(".git").join("index"), "noise").unwrap();

        let mod_ref = ModRef { name: Some("My Mod".to_owned()), file: mod_file.clone() };
        let mut start = call(TOOL, "s1", now - 5000);
        start.args.insert("title".to_owned(), Value::from("Fix  the\nevents"));
        let mut first = call("xtiger_validate", "s1", now - 4000);
        first.mod_ref = Some(mod_ref.clone());
        first.run_id = Some("20260101-000001-my_mod".to_owned());
        first.found = Some(Tally { error: 1, warning: 1, tips: 1, ..Tally::default() });
        let mut second = first.clone();
        second.started_at = now - 1000;
        second.finished_at = Some(now - 900);
        second.run_id = Some("20260101-000002-my_mod".to_owned());
        second.found = Some(Tally { fatal: 1, tips: 1, ..Tally::default() });
        let entries = vec![start, first, second];

        // A moment later: the file was saved after `now` was read.
        let summary = summarize(&state, "s1", &entries, &[], now + 1000);
        assert_eq!(summary.title.as_deref(), Some("Fix the events"));
        assert_eq!(summary.calls, 2);
        assert!(!summary.ended);
        assert_eq!(summary.wrap_up, None);
        let progress = &summary.mods[0];
        assert_eq!(progress.passes.len(), 2);
        assert_eq!(progress.passes[0].found.total(), 3);
        assert_eq!((progress.fixed, progress.new), (Some(2), Some(1)));
        assert_eq!(progress.pending_total, 2);
        assert_eq!(progress.pending[0].severity.as_deref(), Some("fatal"));
        assert_eq!(progress.last_run_id.as_deref(), Some("20260101-000002-my_mod"));
        let paths: Vec<&str> = progress.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["events/a.txt"]);

        // Wrapped up, the session is over, and files saved long after it no longer count.
        let mut end = call(TOOL, "s1", now - 500);
        end.args.insert("wrap_up".to_owned(), Value::from("Fixed the trait."));
        end.summary = Some("Fixed the trait.".to_owned());
        let mut entries = entries;
        entries.push(end);
        let summary = summarize(&state, "s1", &entries, &[], now + GAP_MS);
        assert!(summary.ended);
        assert_eq!(summary.wrap_up.as_deref(), Some("Fixed the trait."));

        let late = mod_dir.join("events").join("late.txt");
        fs::write(&late, "later").unwrap();
        set_time(&late, now + 10 * 60 * 1000);
        let summary = summarize(&state, "s1", &entries, &[], now + 3 * GAP_MS);
        assert_eq!(summary.mods[0].files.len(), 1);
    }

    #[test]
    fn a_session_without_saved_runs_still_sums_up() {
        let tmp = TempDir::new();
        let mod_ref = ModRef { name: None, file: tmp.join("gone.mod") };
        let mut pass = call("xtiger_validate", "s2", 1000);
        pass.mod_ref = Some(mod_ref);
        pass.run_id = Some("20200101-000000-gone".to_owned());
        pass.found = Some(Tally { error: 4, ..Tally::default() });
        let summary = summarize(&tmp.join("state"), "s2", &[pass], &[], 2000);
        let progress = &summary.mods[0];
        assert_eq!((progress.fixed, progress.new), (None, None));
        assert_eq!(progress.pending_total, 4);
        assert_eq!(progress.pending.len(), 0);
        assert_eq!(progress.files.len(), 0);
    }
}
