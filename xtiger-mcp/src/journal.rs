//! The activity journal: what the assistant did with xTiger, for the app's "AI activity" screen.
//!
//! Each finished tool call adds one JSON line to `activity.jsonl` in the state folder. When that
//! file passes [`MAX_BYTES`] it becomes `activity.1.jsonl` (replacing the one before), so the
//! journal never holds more than about twice that. A call that is still running has its own file
//! in `activity-running/`, rewritten every few seconds and removed when the call ends; a file that
//! stops changing belongs to a server that was killed, and is ignored.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const JOURNAL: &str = "activity.jsonl";
pub const OLD_JOURNAL: &str = "activity.1.jsonl";
pub const RUNNING_DIR: &str = "activity-running";
/// The size at which the journal starts a new file.
pub const MAX_BYTES: u64 = 512 * 1024;
/// How often a running call rewrites its file.
pub const TOUCH: Duration = Duration::from_secs(2);
/// A running file that has not changed for this long belongs to a server that is gone.
pub const STALE: Duration = Duration::from_secs(20);
/// The longest reason that is kept.
const MAX_REASON: usize = 200;
/// The longest text kept for one argument.
const MAX_ARG: usize = 120;
/// How many items of a list argument are kept.
const MAX_ITEMS: usize = 5;

/// Appends from the threads of one server go one at a time.
static WRITING: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Ok,
    Error,
    Cancelled,
}

/// How many reports a validation found, by severity.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Tally {
    pub fatal: usize,
    pub error: usize,
    pub warning: usize,
    pub untidy: usize,
    pub tips: usize,
}

impl Tally {
    /// From (severity, count) pairs; unknown severities are left out.
    pub fn from_pairs<'a, I: IntoIterator<Item = (&'a str, usize)>>(pairs: I) -> Self {
        let mut tally = Self::default();
        for (severity, count) in pairs {
            match severity {
                "fatal" => tally.fatal += count,
                "error" => tally.error += count,
                "warning" => tally.warning += count,
                "untidy" => tally.untidy += count,
                "tips" => tally.tips += count,
                _ => {}
            }
        }
        tally
    }

    pub fn total(&self) -> usize {
        self.fatal + self.error + self.warning + self.untidy + self.tips
    }
}

/// The mod a call worked on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModRef {
    pub name: Option<String>,
    pub file: PathBuf,
}

/// One tool call. While it runs, `outcome`, `finished_at`, `duration_ms` and `summary` are empty.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// Unique among the calls of all servers: the process id and a counter.
    pub id: String,
    pub tool: String,
    /// The assistant, as it named itself when it connected.
    #[serde(default)]
    pub client: Option<String>,
    /// Why the assistant made the call, in its own words.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default, rename = "mod")]
    pub mod_ref: Option<ModRef>,
    /// The arguments, shortened, without the reason.
    #[serde(default)]
    pub args: Map<String, Value>,
    /// Milliseconds since the Unix epoch.
    pub started_at: u64,
    #[serde(default)]
    pub finished_at: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub outcome: Option<Outcome>,
    /// What came out, in a few words, such as "2 warnings · 1 new, 3 fixed".
    #[serde(default)]
    pub summary: Option<String>,
    /// The latest line of progress, while the call runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    /// The saved validation run the call made or read.
    #[serde(default)]
    pub run_id: Option<String>,
    /// What a validation found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<Tally>,
    /// The work session the call belongs to: the calls of one server, from one job to the next.
    #[serde(default)]
    pub session: Option<String>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn first_chars(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// A text as it is kept: one line, trimmed, at most `max` characters, and not empty.
pub fn clean_line(text: Option<&Value>, max: usize) -> Option<String> {
    let text = text?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then(|| first_chars(&text, max))
}

/// The reason as it is kept.
pub fn clean_reason(reason: Option<&Value>) -> Option<String> {
    clean_line(reason, MAX_REASON)
}

/// The arguments as they are kept: no reason, long texts and lists cut short.
pub fn brief_args(args: &Map<String, Value>) -> Map<String, Value> {
    args.iter()
        .filter(|(name, value)| *name != "reason" && !value.is_null())
        .map(|(name, value)| {
            let brief = match value {
                Value::String(text) => Value::String(first_chars(text, MAX_ARG)),
                Value::Array(items) => {
                    let mut kept: Vec<Value> = items
                        .iter()
                        .take(MAX_ITEMS)
                        .map(|item| match item {
                            Value::String(text) => Value::String(first_chars(text, MAX_ARG)),
                            other => other.clone(),
                        })
                        .collect();
                    if items.len() > MAX_ITEMS {
                        kept.push(Value::String(format!("… {} more", items.len() - MAX_ITEMS)));
                    }
                    Value::Array(kept)
                }
                Value::Object(_) => Value::String("{…}".to_owned()),
                other => other.clone(),
            };
            (name.clone(), brief)
        })
        .collect()
}

fn running_dir(state_dir: &Path) -> PathBuf {
    state_dir.join(RUNNING_DIR)
}

fn write_json(path: &Path, entry: &Entry) {
    if let Ok(text) = serde_json::to_string(entry) {
        let _ = fs::write(path, text);
    }
}

fn age(path: &Path) -> Option<Duration> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(SystemTime::now().duration_since(modified).unwrap_or_default())
}

/// A call that is running. Writing the journal never fails a call: problems are ignored.
#[derive(Debug)]
pub struct Running {
    state_dir: PathBuf,
    file: PathBuf,
    pub entry: Entry,
}

impl Running {
    /// Show the call as running. Running files that were left behind by killed servers are
    /// cleaned up on the way.
    pub fn start(state_dir: &Path, entry: Entry) -> Self {
        let dir = running_dir(state_dir);
        let _ = fs::create_dir_all(&dir);
        if let Ok(files) = fs::read_dir(&dir) {
            for file in files.filter_map(Result::ok).map(|file| file.path()) {
                if age(&file).is_some_and(|age| age > STALE * 15) {
                    let _ = fs::remove_file(file);
                }
            }
        }
        let file = dir.join(format!("{}.json", entry.id));
        write_json(&file, &entry);
        Self { state_dir: state_dir.to_path_buf(), file, entry }
    }

    /// Rewrite the running file, with the latest progress if there is some. This also tells the
    /// app that the server is still alive.
    pub fn touch(&mut self, progress: Option<&str>) {
        if let Some(line) = progress.map(str::trim).filter(|line| !line.is_empty()) {
            self.entry.progress = Some(first_chars(line, MAX_ARG));
        }
        write_json(&self.file, &self.entry);
    }

    /// Add the finished call to the journal and stop showing it as running.
    pub fn finish(mut self, outcome: Outcome, summary: &str) {
        let finished = now_ms();
        self.entry.finished_at = Some(finished);
        self.entry.duration_ms = Some(finished.saturating_sub(self.entry.started_at));
        self.entry.outcome = Some(outcome);
        self.entry.summary = Some(first_chars(summary.lines().next().unwrap_or("").trim(), 160));
        self.entry.progress = None;
        append(&self.state_dir, &self.entry);
        let _ = fs::remove_file(&self.file);
    }
}

/// Add one line to the journal, starting a new file when it is full.
pub fn append(state_dir: &Path, entry: &Entry) {
    let Ok(mut line) = serde_json::to_string(entry) else { return };
    line.push('\n');
    let _guard = WRITING.lock();
    let _ = fs::create_dir_all(state_dir);
    let journal = state_dir.join(JOURNAL);
    if fs::metadata(&journal).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = fs::rename(&journal, state_dir.join(OLD_JOURNAL));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&journal) {
        // One write per line, so lines from other servers do not get mixed in.
        let _ = file.write_all(line.as_bytes());
    }
}

/// The finished calls in the journal, newest first, at most `limit`.
pub fn read_entries(state_dir: &Path, limit: usize) -> Vec<Entry> {
    let mut entries: Vec<Entry> = [OLD_JOURNAL, JOURNAL]
        .iter()
        .filter_map(|name| fs::read_to_string(state_dir.join(name)).ok())
        .flat_map(|text| {
            text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect::<Vec<_>>()
        })
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

/// The calls running now, oldest first.
pub fn read_running(state_dir: &Path) -> Vec<Entry> {
    let mut running: Vec<Entry> = fs::read_dir(running_dir(state_dir))
        .map(|files| {
            files
                .filter_map(Result::ok)
                .map(|file| file.path())
                .filter(|path| age(path).is_some_and(|age| age < STALE))
                .filter_map(|path| fs::read_to_string(path).ok())
                .filter_map(|text| serde_json::from_str(&text).ok())
                .collect()
        })
        .unwrap_or_default();
    running.sort_by(|a: &Entry, b: &Entry| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)));
    running
}

/// Changes whenever a call finishes, so a reader knows when to read the journal again.
pub fn stamp(state_dir: &Path) -> String {
    match fs::metadata(state_dir.join(JOURNAL)) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{}-{modified}", meta.len())
        }
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::json;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.to_owned(),
            tool: "xtiger_validate".to_owned(),
            client: Some("Test client".to_owned()),
            reason: Some("Checking the fix".to_owned()),
            mod_ref: None,
            args: Map::new(),
            started_at: now_ms(),
            finished_at: None,
            duration_ms: None,
            outcome: None,
            summary: None,
            progress: None,
            run_id: None,
            found: None,
            session: None,
        }
    }

    #[test]
    fn reasons_and_arguments_are_kept_short() {
        assert_eq!(clean_reason(Some(&json!("  Fix\n the  event "))), Some("Fix the event".into()));
        assert_eq!(clean_reason(Some(&json!("   "))), None);
        assert_eq!(clean_reason(Some(&json!(3))), None);
        assert_eq!(
            clean_reason(Some(&json!("x".repeat(500)))).unwrap().chars().count(),
            MAX_REASON
        );
        let args = json!({
            "reason": "why",
            "mod_path": "a".repeat(300),
            "commands": ["1", "2", "3", "4", "5", "6", "7"],
            "limit": 5,
            "nothing": null,
        });
        let brief = brief_args(args.as_object().unwrap());
        assert!(!brief.contains_key("reason") && !brief.contains_key("nothing"));
        assert_eq!(brief["mod_path"].as_str().unwrap().chars().count(), MAX_ARG);
        assert_eq!(brief["commands"].as_array().unwrap().len(), MAX_ITEMS + 1);
        assert_eq!(brief["commands"][MAX_ITEMS], "… 2 more");
        assert_eq!(brief["limit"], 5);
    }

    #[test]
    fn a_call_runs_then_lands_in_the_journal() {
        let tmp = TempDir::new();
        assert_eq!(stamp(&tmp), "");
        let mut call = Running::start(&tmp, entry("1-1"));
        call.touch(Some("Using mod directory"));
        let running = read_running(&tmp);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].progress.as_deref(), Some("Using mod directory"));
        assert_eq!(read_entries(&tmp, 10).len(), 0);

        call.finish(Outcome::Ok, "2 warnings\nsecond line");
        assert_eq!(read_running(&tmp).len(), 0);
        let entries = read_entries(&tmp, 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].outcome, Some(Outcome::Ok));
        assert_eq!(entries[0].summary.as_deref(), Some("2 warnings"));
        assert_eq!(entries[0].progress, None);
        assert!(entries[0].duration_ms.is_some());
        assert_ne!(stamp(&tmp), "");
    }

    #[test]
    fn newest_first_and_bad_lines_are_skipped() {
        let tmp = TempDir::new();
        for id in ["a", "b", "c"] {
            Running::start(&tmp, entry(id)).finish(Outcome::Error, "no");
        }
        let mut file = OpenOptions::new().append(true).open(tmp.join(JOURNAL)).unwrap();
        file.write_all(b"{not json\n").unwrap();
        let ids: Vec<String> = read_entries(&tmp, 2).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["c", "b"]);
    }

    #[test]
    fn a_full_journal_starts_a_new_file() {
        let tmp = TempDir::new();
        let filler = "x".repeat(usize::try_from(MAX_BYTES).unwrap() + 1);
        fs::write(tmp.join(JOURNAL), format!("{filler}\n")).unwrap();
        fs::write(tmp.join(OLD_JOURNAL), "older\n").unwrap();
        append(&tmp, &entry("new"));
        assert!(fs::read_to_string(tmp.join(OLD_JOURNAL)).unwrap().starts_with("xxx"));
        let ids: Vec<String> = read_entries(&tmp, 10).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["new"]);
    }

    #[test]
    fn stale_running_files_are_ignored() {
        let tmp = TempDir::new();
        let call = Running::start(&tmp, entry("old"));
        let past = SystemTime::now() - STALE * 2;
        fs::File::options().write(true).open(&call.file).unwrap().set_modified(past).unwrap();
        assert_eq!(read_running(&tmp).len(), 0);
    }
}
