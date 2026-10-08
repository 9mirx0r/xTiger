//! Running ck3-tiger, keeping its results as saved runs, and querying them.
//!
//! The xTiger app saves its checks here too, so the app and the assistants share one history:
//! what is new or fixed is always measured against the last check of the mod, whoever ran it.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::locate::Locations;

/// How many saved runs are kept.
pub const KEEP_RUNS: usize = 20;
/// The longest a validation may take.
pub const TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const SEVERITIES: [&str; 5] = ["tips", "untidy", "warning", "error", "fatal"];
pub const GROUPS: [&str; 5] = ["file", "folder", "key", "message", "severity"];

/// Counts in the order they were made, written as a JSON object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts(pub Vec<(String, usize)>);

impl Serialize for Counts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, count) in &self.0 {
            map.serialize_entry(key, count)?;
        }
        map.end()
    }
}

/// Count the values, most frequent first. Ties keep the order in which values first appeared.
pub fn most_common<I: IntoIterator<Item = String>>(values: I) -> Vec<(String, usize)> {
    let mut order: Vec<(String, usize)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for value in values {
        if let Some(&i) = index.get(&value) {
            order[i].1 += 1;
        } else {
            index.insert(value.clone(), order.len());
            order.push((value, 1));
        }
    }
    order.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    order
}

fn text<'a>(report: &'a Value, field: &str) -> Option<&'a str> {
    report.get(field).and_then(Value::as_str)
}

fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

fn locations(report: &Value) -> Vec<&Value> {
    match report.get("locations").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => list.iter().collect(),
        _ => vec![&Value::Null],
    }
}

/// What identifies a mod's `.mod` file across runs, however its path was written: `/` or `\`,
/// and on Windows any case.
pub fn mod_key(mod_file: &Path) -> String {
    let key = mod_file.to_string_lossy().replace('/', "\\");
    if cfg!(windows) { key.to_lowercase() } else { key }
}

fn same_mod(a: &Path, b: &Path) -> bool {
    a == b || mod_key(a) == mod_key(b)
}

fn norm(path: &str) -> String {
    path.replace('\\', "/")
}

fn loc_path(loc: &Value) -> String {
    norm(text(loc, "path").unwrap_or(""))
}

fn first_path(report: &Value) -> String {
    loc_path(locations(report)[0])
}

fn place(loc: &Value) -> String {
    let line =
        loc.get("linenr").and_then(Value::as_u64).map_or_else(|| "?".to_owned(), |n| n.to_string());
    format!("{}:{line}", loc_path(loc))
}

fn severity(report: &Value) -> String {
    text(report, "severity").unwrap_or("?").to_owned()
}

fn key(report: &Value) -> String {
    text(report, "key").unwrap_or("?").to_owned()
}

fn message_group(report: &Value) -> String {
    first_chars(text(report, "message").unwrap_or(""), 100)
}

fn group_of(group_by: &str, report: &Value) -> String {
    match group_by {
        "message" => message_group(report),
        "key" => key(report),
        "file" => first_path(report),
        "folder" => first_path(report).split('/').take(2).collect::<Vec<_>>().join("/"),
        _ => severity(report),
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Summary {
    pub total: usize,
    pub by_severity: Counts,
    pub top_keys: Vec<(String, usize)>,
    pub top_messages: Vec<(String, usize)>,
}

pub fn summarize(reports: &[Value], top: usize) -> Summary {
    let mut top_keys = most_common(reports.iter().map(key));
    top_keys.truncate(top);
    let mut top_messages = most_common(reports.iter().map(message_group));
    top_messages.truncate(top);
    Summary {
        total: reports.len(),
        by_severity: Counts(most_common(reports.iter().map(severity))),
        top_keys,
        top_messages,
    }
}

/// The filters of [`query`]. Empty strings mean "any".
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub pattern: String,
    pub path: String,
    pub severity: String,
    pub key: String,
}

struct Compiled {
    pattern: Option<Regex>,
    path: String,
    severity: String,
    key: String,
}

impl Filter {
    fn compile(&self) -> Result<Compiled, String> {
        let pattern = if self.pattern.is_empty() {
            None
        } else {
            Some(
                RegexBuilder::new(&self.pattern)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| format!("pattern is not a valid regex: {e}"))?,
            )
        };
        if !self.severity.is_empty() && !SEVERITIES.contains(&self.severity.as_str()) {
            return Err(format!("severity must be one of {}", SEVERITIES.join(", ")));
        }
        Ok(Compiled {
            pattern,
            path: norm(&self.path),
            severity: self.severity.clone(),
            key: self.key.clone(),
        })
    }
}

impl Compiled {
    fn matches(&self, report: &Value) -> bool {
        if let Some(pattern) = &self.pattern
            && !pattern.is_match(text(report, "message").unwrap_or(""))
        {
            return false;
        }
        // A report can point at several places (for example a use and its definition); any of
        // them may match.
        if !self.path.is_empty()
            && !locations(report).iter().any(|loc| loc_path(loc).contains(&self.path))
        {
            return false;
        }
        if !self.severity.is_empty() && text(report, "severity") != Some(self.severity.as_str()) {
            return false;
        }
        self.key.is_empty() || text(report, "key") == Some(self.key.as_str())
    }
}

/// One report as the tools show it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Row {
    pub severity: Option<String>,
    pub key: Option<String>,
    pub message: Option<String>,
    pub info: Option<String>,
    /// Relative path and line of the first location.
    #[serde(rename = "where")]
    pub place: String,
    /// The full path of that file, to open or edit it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The line of code, trimmed.
    pub line: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub also: Option<Vec<String>>,
}

pub fn row(report: &Value) -> Row {
    let locs = locations(report);
    let owned = |field: &str| text(report, field).map(str::to_owned);
    Row {
        severity: owned("severity"),
        key: owned("key"),
        message: owned("message"),
        info: owned("info"),
        place: place(locs[0]),
        file: text(locs[0], "fullpath").map(str::to_owned),
        line: first_chars(text(locs[0], "line").unwrap_or("").trim(), 200),
        also: (locs.len() > 1).then(|| locs[1..].iter().map(|loc| place(loc)).collect()),
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum QueryResult {
    Reports {
        matched: usize,
        offset: usize,
        reports: Vec<Row>,
    },
    /// `total_groups` counts every group, so a caller can tell that `groups` was cut by
    /// `limit` and `offset`.
    Groups {
        matched: usize,
        total_groups: usize,
        groups: Vec<(String, usize)>,
    },
}

pub fn query(
    reports: &[Value],
    filter: &Filter,
    group_by: &str,
    limit: usize,
    offset: usize,
) -> Result<QueryResult, String> {
    if !group_by.is_empty() && !GROUPS.contains(&group_by) {
        return Err(format!("group_by must be one of {}", GROUPS.join(", ")));
    }
    let compiled = filter.compile()?;
    let hits: Vec<&Value> = reports.iter().filter(|report| compiled.matches(report)).collect();
    if !group_by.is_empty() {
        let groups = most_common(hits.iter().map(|report| group_of(group_by, report)));
        let total_groups = groups.len();
        let groups = groups.into_iter().skip(offset).take(limit).collect();
        return Ok(QueryResult::Groups { matched: hits.len(), total_groups, groups });
    }
    let rows = hits.iter().skip(offset).take(limit).map(|report| row(report)).collect();
    Ok(QueryResult::Reports { matched: hits.len(), offset, reports: rows })
}

/// What identifies a report between runs. Line numbers are left out because they move whenever
/// the lines above change. The app uses the same rule for its NEW tags.
pub fn fingerprint(report: &Value) -> String {
    format!(
        "{}|{}|{}",
        text(report, "key").unwrap_or(""),
        text(report, "message").unwrap_or(""),
        text(locations(report)[0], "path").unwrap_or("")
    )
}

/// The reports of `current` that `previous` did not have, and how many of `previous` are gone.
pub fn compare<'a>(previous: &[Value], current: &'a [Value]) -> (Vec<&'a Value>, usize) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for report in previous {
        *seen.entry(fingerprint(report)).or_default() += 1;
    }
    let mut new = Vec::new();
    for report in current {
        match seen.get_mut(&fingerprint(report)) {
            Some(count) if *count > 0 => *count -= 1,
            _ => new.push(report),
        }
    }
    let fixed = seen.values().sum();
    (new, fixed)
}

/// The reports of `current` that `previous` did not have, and the reports of `previous` that are
/// gone from `current`.
pub fn diff<'a>(previous: &'a [Value], current: &'a [Value]) -> (Vec<&'a Value>, Vec<&'a Value>) {
    (compare(previous, current).0, compare(current, previous).0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMeta {
    pub run_id: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    #[serde(default)]
    pub mod_name: Option<String>,
    /// Who ran it: the assistant's name, or `xTiger app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub tiger: PathBuf,
    pub tiger_source: String,
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub seconds: f64,
    /// When the run ended, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub finished_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedRun {
    meta: RunMeta,
    reports: Vec<Value>,
}

pub fn runs_dir(loc: &Locations) -> PathBuf {
    loc.state_dir.join("runs")
}

/// The saved run files, newest first. Run ids start with the time, so names sort by age.
fn run_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect()
        })
        .unwrap_or_default();
    // By name without `.json`, so that a second run in the same second (`-2`) comes after the
    // first one.
    files.sort_by(|a, b| b.file_stem().cmp(&a.file_stem()));
    files
}

fn read_run(path: &Path) -> Result<SavedRun, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} is damaged: {e}", path.display()))
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RunListing {
    pub run_id: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    pub mod_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    pub total: usize,
    pub tiger: PathBuf,
    pub exit_code: Option<i32>,
    pub seconds: f64,
}

pub fn list_runs(loc: &Locations) -> Vec<RunListing> {
    run_files(&runs_dir(loc))
        .iter()
        .filter_map(|path| read_run(path).ok())
        .map(|run| RunListing {
            total: run.reports.len(),
            run_id: run.meta.run_id,
            mod_file: run.meta.mod_file,
            mod_name: run.meta.mod_name,
            by: run.meta.by,
            tiger: run.meta.tiger,
            exit_code: run.meta.exit_code,
            seconds: run.meta.seconds,
        })
        .collect()
}

/// A saved run by id, or the newest one.
pub fn load_run(loc: &Locations, run_id: &str) -> Result<(RunMeta, Vec<Value>), String> {
    let files = run_files(&runs_dir(loc));
    let file = if run_id.is_empty() {
        files.first().ok_or("No results yet. Run xtiger_validate first.")?
    } else {
        files
            .iter()
            .find(|path| path.file_stem().is_some_and(|stem| stem == run_id))
            .ok_or_else(|| format!("No saved run {run_id}. Use xtiger_runs to list them."))?
    };
    let run = read_run(file)?;
    Ok((run.meta, run.reports))
}

/// The newest saved run of a mod.
pub fn newest_run_of(loc: &Locations, mod_file: &Path) -> Result<(RunMeta, Vec<Value>), String> {
    run_files(&runs_dir(loc))
        .iter()
        .filter_map(|path| read_run(path).ok())
        .find(|run| same_mod(&run.meta.mod_file, mod_file))
        .map(|run| (run.meta, run.reports))
        .ok_or_else(|| {
            format!("No saved run of {}. Run xtiger_validate first.", mod_file.display())
        })
}

/// The saved run of the same mod just before `meta`.
pub fn run_before(loc: &Locations, meta: &RunMeta) -> Result<(RunMeta, Vec<Value>), String> {
    let files = run_files(&runs_dir(loc));
    let at = files
        .iter()
        .position(|path| path.file_stem().is_some_and(|stem| *stem == *meta.run_id))
        .unwrap_or(files.len());
    files
        .iter()
        .skip(at + 1)
        .filter_map(|path| read_run(path).ok())
        .find(|run| same_mod(&run.meta.mod_file, &meta.mod_file))
        .map(|run| (run.meta, run.reports))
        .ok_or_else(|| {
            format!(
                "Run {} is the oldest saved run of its mod: there is nothing to compare it with.",
                meta.run_id
            )
        })
}

/// A saved run opened for reading in the app.
#[derive(Debug)]
pub struct OpenedRun {
    pub meta: RunMeta,
    /// The reports, each with an added `isNew`: whether the run of the same mod before it lacked it.
    pub reports: Vec<Value>,
    /// The number of reports in that earlier run, if there was one.
    pub previous_total: Option<usize>,
    pub new: usize,
}

/// The run `run_id` among the saved runs in `dir`.
pub fn open_run(dir: &Path, run_id: &str) -> Result<OpenedRun, String> {
    let files = run_files(dir);
    let at = files
        .iter()
        .position(|path| path.file_stem().is_some_and(|stem| stem == run_id))
        .ok_or("That run is no longer saved: only the newest 20 are kept.")?;
    let SavedRun { meta, mut reports } = read_run(&files[at])?;
    let previous = files[at + 1..]
        .iter()
        .filter_map(|path| read_run(path).ok())
        .find(|run| same_mod(&run.meta.mod_file, &meta.mod_file));
    let prints: Option<Vec<String>> =
        previous.as_ref().map(|run| run.reports.iter().map(fingerprint).collect());
    let new = mark_new(prints.as_deref(), &mut reports);
    Ok(OpenedRun { meta, reports, previous_total: previous.map(|run| run.reports.len()), new })
}

/// Give each report an `isNew`: whether the earlier run, known by the fingerprints of its
/// reports, lacked it. Without an earlier run nothing is new. Returns how many are new.
pub fn mark_new(previous: Option<&[String]>, reports: &mut [Value]) -> usize {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for print in previous.unwrap_or_default() {
        *seen.entry(print).or_default() += 1;
    }
    let mut new = 0;
    for report in reports {
        let is_new = match seen.get_mut(fingerprint(report).as_str()) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => previous.is_some(),
        };
        new += usize::from(is_new);
        if let Value::Object(fields) = report {
            fields.insert("isNew".to_owned(), Value::Bool(is_new));
        }
    }
    new
}

/// The newest saved run of a mod in `dir`, whoever ran it.
pub fn newest_in(dir: &Path, mod_file: &Path) -> Option<(RunMeta, Vec<Value>)> {
    run_files(dir)
        .iter()
        .filter_map(|path| read_run(path).ok())
        .find(|run| same_mod(&run.meta.mod_file, mod_file))
        .map(|run| (run.meta, run.reports))
}

/// For each mod with a saved run: how many reports its newest run had, and when that run ended
/// in milliseconds since the Unix epoch. Keyed by [`mod_key`].
pub fn latest_of_each(dir: &Path) -> HashMap<String, (usize, u64)> {
    let mut latest = HashMap::new();
    for run in run_files(dir).iter().filter_map(|path| read_run(path).ok()) {
        latest
            .entry(mod_key(&run.meta.mod_file))
            .or_insert((run.reports.len(), run.meta.finished_at));
    }
    latest
}

/// A run to save. Its id is made from the time it is saved and the mod's name.
#[derive(Debug)]
pub struct NewRun {
    pub mod_file: PathBuf,
    pub mod_name: Option<String>,
    pub by: Option<String>,
    pub tiger: PathBuf,
    pub tiger_source: String,
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub seconds: f64,
}

#[derive(Serialize)]
struct SavedRunRef<'a> {
    meta: &'a RunMeta,
    reports: &'a [Value],
}

/// Save a run in `dir`, keeping only the newest `KEEP_RUNS`.
pub fn save_run(dir: &Path, run: NewRun, reports: &[Value]) -> Result<RunMeta, String> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let finished = SystemTime::now();
    let stem = format!(
        "{}-{}",
        timestamp(finished),
        slug(run.mod_name.as_deref().unwrap_or_else(|| {
            run.mod_file.file_stem().and_then(|stem| stem.to_str()).unwrap_or("mod")
        }))
    );
    let mut run_id = stem.clone();
    let mut n = 2;
    while dir.join(format!("{run_id}.json")).exists() {
        run_id = format!("{stem}-{n}");
        n += 1;
    }
    let meta = RunMeta {
        run_id,
        mod_file: run.mod_file,
        mod_name: run.mod_name,
        by: run.by,
        tiger: run.tiger,
        tiger_source: run.tiger_source,
        command: run.command,
        exit_code: run.exit_code,
        seconds: run.seconds,
        finished_at: finished
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
    };
    let file = dir.join(format!("{}.json", meta.run_id));
    let json =
        serde_json::to_string(&SavedRunRef { meta: &meta, reports }).map_err(|e| e.to_string())?;
    fs::write(&file, json).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    for old in run_files(dir).into_iter().skip(KEEP_RUNS) {
        let _ = fs::remove_file(old);
    }
    Ok(meta)
}

/// The reports of the run `run_id` among the saved runs in `dir`, if it is still saved.
pub fn saved_run(dir: &Path, run_id: &str) -> Option<Vec<Value>> {
    let path = dir.join(format!("{run_id}.json"));
    let valid =
        !run_id.is_empty() && run_id.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c));
    valid.then(|| read_run(&path).ok()).flatten().map(|run| run.reports)
}

/// Save a run as the validator would, for tests.
#[cfg(test)]
pub(crate) fn save_test_run(dir: &Path, run_id: &str, mod_file: &Path, reports: &[Value]) {
    let meta = RunMeta {
        run_id: run_id.to_owned(),
        mod_file: mod_file.to_path_buf(),
        mod_name: None,
        by: None,
        tiger: PathBuf::from("ck3-tiger"),
        tiger_source: "test".to_owned(),
        command: vec![],
        exit_code: Some(0),
        seconds: 1.0,
        finished_at: 0,
    };
    let run = SavedRun { meta, reports: reports.to_vec() };
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("{run_id}.json")), serde_json::to_string(&run).unwrap()).unwrap();
}

/// `YYYYMMDD-HHMMSS` in UTC.
#[allow(clippy::integer_division)]
pub fn timestamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rest = secs % 86_400;
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = first_chars(out.trim_matches('_'), 40);
    if out.is_empty() { "mod".to_owned() } else { out }
}

/// What `xtiger_validate` returns.
#[derive(Debug, Serialize)]
pub struct Validation {
    pub run_id: String,
    #[serde(rename = "mod")]
    pub mod_file: PathBuf,
    pub mod_name: Option<String>,
    #[serde(flatten)]
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_last_run: Option<Comparison>,
    pub seconds: f64,
    pub exit_code: Option<i32>,
    pub tiger: PathBuf,
    pub tiger_source: String,
    pub results_file: PathBuf,
    /// The validator's own messages: the game version check and any problems loading the mod.
    pub stderr: String,
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub previous_run_id: String,
    pub previous_total: usize,
    pub new: usize,
    pub fixed: usize,
    /// The first new reports.
    pub new_reports: Vec<Row>,
}

#[derive(Debug)]
pub struct Options<'a> {
    pub show_vanilla: bool,
    pub config: Option<&'a Path>,
    pub timeout: Duration,
    /// Who runs it, kept with the run.
    pub by: Option<&'a str>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self { show_vanilla: false, config: None, timeout: TIMEOUT, by: None }
    }
}

pub fn no_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn last_chars(text: &str, n: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(n)).collect()
}

/// Run the validator on `mod_file` and save the run. `progress` gets each line the validator
/// prints while it works; when `cancelled` turns true the validator is stopped.
pub fn validate(
    loc: &Locations,
    mod_file: &Path,
    mod_name: Option<String>,
    options: &Options,
    progress: &mut dyn FnMut(&str),
    cancelled: &dyn Fn() -> bool,
) -> Result<Validation, String> {
    let tiger = loc.require_validator()?.to_path_buf();
    if !mod_file.is_file() {
        return Err(format!("No .mod file at {}", mod_file.display()));
    }
    let mut args: Vec<String> =
        vec!["--json".into(), "--paradox".into(), loc.user_dir.display().to_string()];
    if loc.game.is_some() {
        args.push("--game".into());
        args.push(loc.require_game()?.display().to_string());
    }
    if options.show_vanilla {
        args.push("--show-vanilla".into());
    }
    if let Some(config) = options.config {
        if !config.is_file() {
            return Err(format!("No config file at {}", config.display()));
        }
        args.push("--config".into());
        args.push(config.display().to_string());
    }
    args.push(mod_file.display().to_string());

    let started = Instant::now();
    let mut child = no_window(Command::new(&tiger).args(&args))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", tiger.display()))?;
    let mut stdout = child.stdout.take().ok_or("no output from the validator")?;
    let stderr = child.stderr.take().ok_or("no output from the validator")?;
    let out_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let (lines_tx, lines_rx) = mpsc::channel::<String>();
    let err_thread = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut log: Vec<String> = Vec::new();
    let status = loop {
        match lines_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if !line.trim().is_empty() {
                    progress(line.trim_end());
                    log.push(line);
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
        }
        if cancelled() || started.elapsed() > options.timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = err_thread.join();
            let _ = out_thread.join();
            return Err(if cancelled() {
                "Cancelled.".to_owned()
            } else {
                format!(
                    "The validator took longer than {} minutes and was stopped.",
                    options.timeout.as_secs().div_euclid(60)
                )
            });
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
    };
    let _ = err_thread.join();
    log.extend(lines_rx.try_iter().filter(|line| !line.trim().is_empty()));
    let output = out_thread.join().unwrap_or_default();
    let seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    let stderr = last_chars(log.join("\n").trim(), 2000);

    let output = String::from_utf8_lossy(&output);
    let reports: Vec<Value> = if output.trim().is_empty() && status.success() {
        Vec::new()
    } else {
        serde_json::from_str(&output).map_err(|_| {
            format!(
                "ck3-tiger did not return reports (exit {}): {stderr}",
                status.code().unwrap_or(-1)
            )
        })?
    };

    let dir = runs_dir(loc);
    // The last check of the same mod, by the app or an assistant, to say what changed.
    let since_last_run = newest_in(&dir, mod_file).map(|(meta, previous)| {
        let (new, fixed) = compare(&previous, &reports);
        Comparison {
            previous_run_id: meta.run_id,
            previous_total: previous.len(),
            new: new.len(),
            fixed,
            new_reports: new.iter().take(10).map(|report| row(report)).collect(),
        }
    });
    let summary = summarize(&reports, 15);
    let meta = save_run(
        &dir,
        NewRun {
            mod_file: mod_file.to_path_buf(),
            mod_name: mod_name.clone(),
            by: options.by.map(str::to_owned),
            tiger: tiger.clone(),
            tiger_source: loc.validator_from.to_owned(),
            command: std::iter::once(tiger.display().to_string()).chain(args).collect(),
            exit_code: status.code(),
            seconds,
        },
        &reports,
    )?;

    Ok(Validation {
        results_file: dir.join(format!("{}.json", meta.run_id)),
        run_id: meta.run_id,
        mod_file: mod_file.to_path_buf(),
        mod_name,
        summary,
        since_last_run,
        seconds,
        exit_code: status.code(),
        tiger,
        tiger_source: loc.validator_from.to_owned(),
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn fake_reports() -> Vec<Value> {
        vec![
            json!({
                "severity": "warning", "key": "missing-item", "message": "Missing faith definition",
                "info": "Define the referenced faith",
                "locations": [
                    {"path": "events/story/intro.txt", "fullpath": "C:/mod/events/story/intro.txt", "linenr": 10,
                     "line": format!("  {}  ", "x".repeat(210))},
                    {"path": "common\\religion\\faiths.txt", "linenr": 20},
                ],
            }),
            json!({
                "severity": "error", "key": "invalid-value", "message": "Invalid faith value",
                "locations": [{"path": "common/religion/faiths.txt", "linenr": 30, "line": "  faith = invalid  "}],
            }),
            json!({
                "severity": "warning", "key": "missing-item", "message": "Missing faith definition",
                "locations": [{"path": "events/story/other.txt", "linenr": 8}],
            }),
            json!({
                "severity": "tips", "key": "style", "message": "Prefer short descriptions",
                "locations": [{"path": "localization/english/story.yml", "linenr": 5}],
            }),
            json!({}),
        ]
    }

    fn pairs(list: &[(&str, usize)]) -> Vec<(String, usize)> {
        list.iter().map(|(k, n)| ((*k).to_owned(), *n)).collect()
    }

    #[test]
    fn summarize_counts_and_top_limit() {
        let summary = summarize(&fake_reports(), 2);
        assert_eq!(summary.total, 5);
        assert_eq!(
            summary.by_severity.0,
            pairs(&[("warning", 2), ("error", 1), ("tips", 1), ("?", 1)])
        );
        assert_eq!(summary.top_keys, pairs(&[("missing-item", 2), ("invalid-value", 1)]));
        assert_eq!(
            summary.top_messages,
            pairs(&[("Missing faith definition", 2), ("Invalid faith value", 1)])
        );
        let json = serde_json::to_string(&summary.by_severity).unwrap();
        assert_eq!(json, r#"{"warning":2,"error":1,"tips":1,"?":1}"#);
    }

    #[test]
    fn summarize_empty_and_long_messages() {
        assert_eq!(summarize(&[], 15).total, 0);
        let prefix = "x".repeat(100);
        let reports = [
            json!({"message": format!("{prefix}first")}),
            json!({"message": format!("{prefix}second")}),
        ];
        assert_eq!(summarize(&reports, 15).top_messages, vec![(prefix, 2)]);
    }

    fn messages(result: &QueryResult) -> Vec<Option<String>> {
        match result {
            QueryResult::Reports { reports, .. } => {
                reports.iter().map(|row| row.message.clone()).collect()
            }
            QueryResult::Groups { .. } => panic!("expected reports"),
        }
    }

    #[test]
    fn query_filters() {
        let reports = fake_reports();
        let message = |i: usize| text(&reports[i], "message").map(str::to_owned);
        let cases: Vec<(Filter, Vec<usize>)> = vec![
            (Filter { pattern: "MISSING.*faith".into(), ..Filter::default() }, vec![0, 2]),
            (Filter { path: r"common\religion".into(), ..Filter::default() }, vec![0, 1]),
            (Filter { severity: "warning".into(), ..Filter::default() }, vec![0, 2]),
            (Filter { key: "invalid-value".into(), ..Filter::default() }, vec![1]),
            (
                Filter {
                    pattern: "faith".into(),
                    path: "religion".into(),
                    severity: "warning".into(),
                    key: "missing-item".into(),
                },
                vec![0],
            ),
            (Filter { pattern: "unmatched".into(), ..Filter::default() }, vec![]),
        ];
        for (filter, indices) in cases {
            let result = query(&reports, &filter, "", 50, 0).unwrap();
            assert_eq!(
                messages(&result),
                indices.iter().map(|&i| message(i)).collect::<Vec<_>>(),
                "{filter:?}"
            );
        }
    }

    #[test]
    fn query_rejects_bad_input() {
        let reports = fake_reports();
        let bad_severity = Filter { severity: "Warning".into(), ..Filter::default() };
        assert!(
            query(&reports, &bad_severity, "", 50, 0)
                .unwrap_err()
                .contains("severity must be one of")
        );
        let bad_regex = Filter { pattern: "(".into(), ..Filter::default() };
        assert!(query(&reports, &bad_regex, "", 50, 0).unwrap_err().contains("not a valid regex"));
        assert!(
            query(&reports, &Filter::default(), "unknown", 50, 0)
                .unwrap_err()
                .contains("group_by must be one of")
        );
    }

    #[test]
    fn query_rows() {
        let QueryResult::Reports { reports: rows, .. } =
            query(&fake_reports(), &Filter::default(), "", 50, 0).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            rows[0],
            Row {
                severity: Some("warning".into()),
                key: Some("missing-item".into()),
                message: Some("Missing faith definition".into()),
                info: Some("Define the referenced faith".into()),
                place: "events/story/intro.txt:10".into(),
                file: Some("C:/mod/events/story/intro.txt".into()),
                line: "x".repeat(200),
                also: Some(vec!["common/religion/faiths.txt:20".into()]),
            }
        );
        assert_eq!(rows[1].line, "faith = invalid");
        assert!(rows[1].also.is_none());
        assert_eq!(rows[4].place, ":?");
        assert_eq!(rows[4].line, "");
        let json = serde_json::to_string(&rows[1]).unwrap();
        assert!(json.starts_with(r#"{"severity":"error","key":"invalid-value""#), "{json}");
        assert!(json.contains(r#""info":null"#) && !json.contains("also"), "{json}");
    }

    #[test]
    fn query_paging() {
        let reports = fake_reports();
        for (offset, limit, expected) in [(1, 2, vec![1, 2]), (5, 2, vec![]), (0, 0, vec![])] {
            let result = query(&reports, &Filter::default(), "", limit, offset).unwrap();
            let QueryResult::Reports { matched, offset: got, .. } = &result else { panic!() };
            assert_eq!((*matched, *got), (5, offset));
            let want: Vec<_> =
                expected.iter().map(|&i| text(&reports[i], "message").map(str::to_owned)).collect();
            assert_eq!(messages(&result), want);
        }
    }

    #[test]
    fn query_groups() {
        let reports = fake_reports();
        let cases = [
            ("key", vec![("missing-item", 2), ("invalid-value", 1), ("style", 1), ("?", 1)]),
            ("severity", vec![("warning", 2), ("error", 1), ("tips", 1), ("?", 1)]),
            (
                "message",
                vec![
                    ("Missing faith definition", 2),
                    ("Invalid faith value", 1),
                    ("Prefer short descriptions", 1),
                    ("", 1),
                ],
            ),
            (
                "file",
                vec![
                    ("events/story/intro.txt", 1),
                    ("common/religion/faiths.txt", 1),
                    ("events/story/other.txt", 1),
                    ("localization/english/story.yml", 1),
                    ("", 1),
                ],
            ),
            (
                "folder",
                vec![
                    ("events/story", 2),
                    ("common/religion", 1),
                    ("localization/english", 1),
                    ("", 1),
                ],
            ),
        ];
        for (group_by, expected) in cases {
            assert_eq!(
                query(&reports, &Filter::default(), group_by, 50, 0).unwrap(),
                QueryResult::Groups {
                    matched: 5,
                    total_groups: expected.len(),
                    groups: pairs(&expected)
                },
                "{group_by}"
            );
        }
        let religion = Filter { path: "religion".into(), ..Filter::default() };
        assert_eq!(
            query(&reports, &religion, "key", 1, 0).unwrap(),
            QueryResult::Groups {
                matched: 2,
                total_groups: 2,
                groups: pairs(&[("missing-item", 1)])
            }
        );
        assert_eq!(
            query(&reports, &Filter::default(), "key", 2, 1).unwrap(),
            QueryResult::Groups {
                matched: 5,
                total_groups: 4,
                groups: pairs(&[("invalid-value", 1), ("style", 1)])
            }
        );
    }

    #[test]
    fn any_location_matches_and_missing_locations_are_handled() {
        let reports = fake_reports();
        let filter =
            |path: &str| Filter { path: path.into(), ..Filter::default() }.compile().unwrap();
        assert!(filter("events/story").matches(&reports[0]));
        assert!(filter("faiths").matches(&reports[0]));
        assert!(!filter("missing-folder").matches(&reports[0]));
        for report in [json!({}), json!({"locations": []})] {
            assert!(filter("").matches(&report));
            assert!(!filter("events").matches(&report));
        }
    }

    #[test]
    fn compare_counts_new_and_fixed() {
        let old = fake_reports();
        let mut current = old[1..].to_vec();
        current
            .push(json!({"key": "brand-new", "message": "new", "locations": [{"path": "a.txt"}]}));
        current.push(old[2].clone());
        let (new, fixed) = compare(&old, &current);
        // The brand-new report and the second copy of old[2] are new; old[0] is gone.
        assert_eq!(new.len(), 2);
        assert_eq!(text(new[0], "key"), Some("brand-new"));
        assert_eq!(fixed, 1);
        let (new, fixed) = compare(&old, &old[2..]);
        assert_eq!((new.len(), fixed), (0, 2));
    }

    #[test]
    fn app_and_assistant_share_one_history() {
        let tmp = crate::testing::TempDir::new();
        let new_run = |by: &str| NewRun {
            mod_file: PathBuf::from("silk.mod"),
            mod_name: Some("Silk Road".to_owned()),
            by: Some(by.to_owned()),
            tiger: PathBuf::from("ck3-tiger"),
            tiger_source: "test".to_owned(),
            command: vec![],
            exit_code: Some(0),
            seconds: 1.0,
        };
        let old = fake_reports();
        let first = save_run(&tmp, new_run("xTiger app"), &old).unwrap();
        assert!(first.run_id.ends_with("silk_road"));
        let (meta, reports) = newest_in(&tmp, Path::new("silk.mod")).unwrap();
        assert_eq!((meta.by.as_deref(), reports.len()), (Some("xTiger app"), 5));
        assert!(newest_in(&tmp, Path::new("other.mod")).is_none());

        let mut current = old[1..].to_vec();
        current.push(json!({"key": "new-one", "message": "x", "locations": [{"path": "a.txt"}]}));
        let prints: Vec<String> = reports.iter().map(fingerprint).collect();
        assert_eq!(mark_new(Some(&prints), &mut current), 1);
        assert_eq!(current.last().unwrap()["isNew"], json!(true));
        assert_eq!(current[0]["isNew"], json!(false));
        assert_eq!(mark_new(None, &mut current), 0);

        let second = save_run(&tmp, new_run("Claude Code"), &current).unwrap();
        assert_ne!(first.run_id, second.run_id);
        let latest = latest_of_each(&tmp);
        assert_eq!(latest[&mod_key(Path::new("silk.mod"))].0, 5);
        assert!(same_mod(Path::new("D:/mods/silk.mod"), Path::new(r"D:\mods\silk.mod")));
        let newest = read_run(&run_files(&tmp)[0]).unwrap();
        assert_eq!(newest.meta.by.as_deref(), Some("Claude Code"));
    }

    #[test]
    fn opened_runs_mark_what_is_new() {
        let tmp = crate::testing::TempDir::new();
        let save = |run_id: &str, mod_file: &str, reports: &[Value]| {
            let meta = RunMeta {
                run_id: run_id.to_owned(),
                mod_file: PathBuf::from(mod_file),
                mod_name: Some("Silk Road".to_owned()),
                by: None,
                tiger: PathBuf::from("ck3-tiger"),
                tiger_source: "test".to_owned(),
                command: vec![],
                exit_code: Some(0),
                seconds: 1.0,
                finished_at: 0,
            };
            let run = SavedRun { meta, reports: reports.to_vec() };
            fs::write(tmp.join(format!("{run_id}.json")), serde_json::to_string(&run).unwrap())
                .unwrap();
        };
        let old = fake_reports();
        save("20260101-000000-silk", "silk.mod", &old);
        save("20260102-000000-other", "other.mod", &[]);
        save("20260103-000000-silk", "silk.mod", &old[1..]);

        let first = open_run(&tmp, "20260101-000000-silk").unwrap();
        assert_eq!((first.previous_total, first.new), (None, 0));
        assert!(first.reports.iter().all(|report| report["isNew"] == false));
        let later = open_run(&tmp, "20260103-000000-silk").unwrap();
        assert_eq!((later.previous_total, later.new), (Some(old.len()), 0));
        assert_eq!(later.meta.mod_name.as_deref(), Some("Silk Road"));
        save("20260104-000000-silk", "silk.mod", &old);
        assert_eq!(open_run(&tmp, "20260104-000000-silk").unwrap().new, 1);
        assert!(open_run(&tmp, "gone").unwrap_err().contains("no longer saved"));
    }

    #[test]
    fn timestamps_are_utc_dates() {
        assert_eq!(timestamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_767_225_600 + 3_723)),
            "20260101-010203"
        );
        assert_eq!(timestamp(UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229-000000");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Silk Road: Events!"), "silk_road_events");
        assert_eq!(slug("???"), "mod");
    }
}
