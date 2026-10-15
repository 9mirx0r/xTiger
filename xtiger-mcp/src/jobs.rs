//! Game runs as background jobs, for runs longer than a client is willing to wait for one call.
//!
//! `ck3_run_start` checks the run, takes the game locks and hands the run to a thread of its own,
//! then answers with a job id at once. The run does not depend on the call that started it: a
//! client that gave up waiting (many do, after five minutes) has not stopped the game, and the job
//! can be read later with `ck3_job`. Each job keeps a record in `jobs/` in the state folder, with
//! the state, the latest progress line, how many commands were typed, and a hash of the mod's
//! files as they were when it started, so a result can be told apart from the mod as it is now.
//! The report goes next to it. A job whose server has gone away is shown as interrupted.
//!
//! A job is also an entry in the activity journal, like a `ck3_run` call, so the app shows it
//! while it runs and the update status of a project sees that the game was run.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::game::{self, Control, RunGame, Started};
use crate::journal::{self, Entry, ModRef, Outcome, now_ms};
use crate::locate::Locations;
use crate::projects;

/// How many job records are kept.
const KEPT: usize = 20;
/// The longest a status call waits for a job to end.
pub const MAX_WAIT: Duration = Duration::from_secs(30);
/// A job record that has not changed for this long belongs to a server that is gone. A running job
/// rewrites it every [`journal::TOUCH`].
const STALE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Running,
    /// The run got as far as a result. It says nothing about whether the mod behaved.
    Finished,
    /// The run tested nothing: it failed, timed out, or a mod was not loaded.
    Failed,
    Cancelled,
    /// The server that ran it stopped before it ended.
    Interrupted,
}

impl State {
    pub fn is_over(self) -> bool {
        self != Self::Running
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Record {
    pub id: String,
    pub state: State,
    /// What the run is doing: starting, loading, commands or finishing.
    pub phase: String,
    /// The process id of the server that runs it.
    pub owner: u32,
    pub started_at: u64,
    pub updated_at: u64,
    pub finished_at: Option<u64>,
    #[serde(rename = "mod")]
    pub mod_ref: Option<ModRef>,
    pub mod_dir: Option<PathBuf>,
    /// A hash of the mod's files when the job started.
    pub snapshot: Option<String>,
    pub background: bool,
    pub play: String,
    pub bookmark: String,
    pub commands_total: usize,
    /// Commands typed so far, and how many of them the game had logged when typed.
    pub commands_sent: usize,
    pub commands_seen: usize,
    /// The most the run is given, in seconds.
    pub budget_secs: u64,
    pub progress: Option<String>,
    pub summary: Option<String>,
    /// Why the run tested nothing, in a line.
    pub error: Option<String>,
    pub screenshot: Option<PathBuf>,
    /// The whole report, as the driver wrote it.
    pub report: PathBuf,
}

/// What the call that starts a job knows about it.
#[derive(Debug, Default)]
pub struct Meta {
    pub mod_ref: Option<ModRef>,
    pub mod_dir: Option<PathBuf>,
    pub reason: Option<String>,
    pub client: Option<String>,
    pub session: String,
    /// The arguments, shortened, for the journal.
    pub args: Map<String, Value>,
}

fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("jobs")
}

/// A job id is a file name: nothing else is accepted from a client.
fn valid(id: &str) -> bool {
    !id.is_empty() && id.len() < 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn record_path(state_dir: &Path, id: &str) -> PathBuf {
    dir(state_dir).join(format!("{id}.json"))
}

fn result_path(state_dir: &Path, id: &str) -> PathBuf {
    dir(state_dir).join(format!("{id}.txt"))
}

fn save(state_dir: &Path, record: &Record) {
    let folder = dir(state_dir);
    let _ = fs::create_dir_all(&folder);
    let Ok(text) = serde_json::to_string_pretty(record) else { return };
    let file = record_path(state_dir, &record.id);
    let tmp = file.with_extension("tmp");
    // Another server may be reading it: it sees the old file or the new one, never half of one.
    if fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &file).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

fn age(path: &Path) -> Duration {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .unwrap_or_default()
}

/// Jobs that run in this server, by id.
/// A job of this server: its id, the flag that cancels it, and its thread.
type Worker = (String, Arc<AtomicBool>, JoinHandle<()>);

static LIVE: Mutex<Vec<Worker>> = Mutex::new(Vec::new());

fn is_live(id: &str) -> bool {
    LIVE.lock().is_ok_and(|live| {
        live.iter().any(|(live_id, _, handle)| live_id == id && !handle.is_finished())
    })
}

/// Stop every job of this server and wait for them, so no game is left behind.
pub fn stop_all() {
    let jobs: Vec<_> = LIVE.lock().map(|mut live| std::mem::take(&mut *live)).unwrap_or_default();
    for (_, cancel, _) in &jobs {
        cancel.store(true, Ordering::SeqCst);
    }
    for (_, _, handle) in jobs {
        let _ = handle.join();
    }
}

/// The record of a job, or `None` if there is none. A job that says it runs but whose server is
/// gone is changed to interrupted.
pub fn load(state_dir: &Path, id: &str) -> Option<Record> {
    if !valid(id) {
        return None;
    }
    let path = record_path(state_dir, id);
    let mut record: Record = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    if record.state == State::Running {
        let gone =
            if record.owner == std::process::id() { !is_live(id) } else { age(&path) > STALE };
        if gone {
            record.state = State::Interrupted;
            "interrupted".clone_into(&mut record.phase);
        }
    }
    Some(record)
}

/// The jobs on record, newest first.
pub fn list(state_dir: &Path) -> Vec<Record> {
    let mut ids: Vec<String> = fs::read_dir(dir(state_dir))
        .map(|files| {
            files
                .filter_map(Result::ok)
                .filter_map(|file| {
                    let name = file.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".json").map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids.reverse();
    ids.iter().filter_map(|id| load(state_dir, id)).collect()
}

fn prune(state_dir: &Path) {
    let mut ids: Vec<String> = fs::read_dir(dir(state_dir))
        .map(|files| {
            files
                .filter_map(Result::ok)
                .filter_map(|file| {
                    let name = file.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".json").map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    let excess = ids.len().saturating_sub(KEPT);
    for id in ids.into_iter().take(excess) {
        // Not one that still runs.
        if load(state_dir, &id).is_some_and(|record| !record.state.is_over()) {
            continue;
        }
        let _ = fs::remove_file(record_path(state_dir, &id));
        let _ = fs::remove_file(result_path(state_dir, &id));
    }
}

/// The report of a job that ended, as it was handed back.
pub fn result_text(state_dir: &Path, id: &str) -> Option<String> {
    valid(id).then(|| fs::read_to_string(result_path(state_dir, id)).ok()).flatten()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

/// What a line of the driver's output says about how far the run is.
fn note_line(record: &mut Record, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if line.starts_with("launched pid") {
        "loading".clone_into(&mut record.phase);
    } else if let Some(rest) = line.strip_prefix("sent:") {
        "commands".clone_into(&mut record.phase);
        record.commands_sent += 1;
        if rest.trim_end().ends_with("(confirmed)") {
            record.commands_seen += 1;
        }
    } else if line.starts_with("report ") || line.starts_with("game closed") {
        "finishing".clone_into(&mut record.phase);
    }
    record.progress = Some(line.chars().take(160).collect());
}

/// The part of a job that changes while it runs, and what keeps it on disk and in the journal.
struct Live {
    record: Record,
    running: journal::Running,
    saved: Instant,
}

impl Live {
    fn write(&mut self, state_dir: &Path, force: bool) {
        if !force && self.saved.elapsed() < journal::TOUCH {
            return;
        }
        self.saved = Instant::now();
        self.record.updated_at = now_ms();
        save(state_dir, &self.record);
        let progress = self.record.progress.clone();
        self.running.touch(progress.as_deref());
    }
}

/// Check a run, take the game locks, and run it in the background. The run is refused here, with
/// the reason, if the game could not start at all.
pub fn start(loc: &Locations, run: &RunGame, meta: Meta) -> Result<Record, String> {
    let started = game::begin(loc, run)?;
    let now = now_ms();
    let id = format!("job-{now:013}-{}", std::process::id());
    let record = Record {
        id: id.clone(),
        state: State::Running,
        phase: "starting".to_owned(),
        owner: std::process::id(),
        started_at: now,
        updated_at: now,
        finished_at: None,
        mod_ref: meta.mod_ref.clone(),
        snapshot: meta.mod_dir.as_deref().and_then(projects::manifest_hash),
        mod_dir: meta.mod_dir,
        background: run.background,
        play: run.play.to_owned(),
        bookmark: run.bookmark.to_owned(),
        commands_total: run.commands.len(),
        commands_sent: 0,
        commands_seen: 0,
        budget_secs: started.budget().as_secs(),
        progress: None,
        summary: None,
        error: None,
        screenshot: None,
        report: started.report().to_path_buf(),
    };
    save(&loc.state_dir, &record);
    prune(&loc.state_dir);
    let entry = Entry {
        id: id.clone(),
        tool: "ck3_run".to_owned(),
        client: meta.client,
        reason: meta.reason,
        mod_ref: meta.mod_ref,
        args: meta.args,
        started_at: now,
        finished_at: None,
        duration_ms: None,
        outcome: None,
        summary: None,
        progress: None,
        run_id: None,
        found: None,
        session: Some(meta.session),
    };
    let running = journal::Running::start(&loc.state_dir, entry);
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = {
        let loc = loc.clone();
        let cancel = Arc::clone(&cancel);
        let live = Live { record: record.clone(), running, saved: Instant::now() };
        thread::spawn(move || work(&loc, started, live, &cancel))
    };
    if let Ok(mut jobs) = LIVE.lock() {
        jobs.retain(|(_, _, handle)| !handle.is_finished());
        jobs.push((id, cancel, handle));
    }
    Ok(record)
}

/// The summary a finished job leaves in the journal.
fn summary_of(record: &Record) -> String {
    if record.background {
        "Played in the background".to_owned()
    } else {
        format!(
            "Played as {}, {} of {} console commands logged by the game",
            record.play, record.commands_seen, record.commands_total
        )
    }
}

fn work(loc: &Locations, started: Started, live: Live, cancel: &AtomicBool) {
    let state = std::cell::RefCell::new(live);
    let cancelled = || cancel.load(Ordering::SeqCst);
    let mut progress = |line: &str| {
        let mut live = state.borrow_mut();
        note_line(&mut live.record, line);
        live.write(&loc.state_dir, false);
    };
    let mut tick = || state.borrow_mut().write(&loc.state_dir, false);
    let mut control = Control { cancelled: &cancelled, progress: &mut progress, tick: &mut tick };
    let result = started.execute(loc, &mut control);
    let mut live = state.into_inner();
    let record = &mut live.record;
    record.finished_at = Some(now_ms());
    record.progress = None;
    let (outcome, text) = match result {
        Ok((text, shot)) => {
            record.state = State::Finished;
            record.screenshot = shot;
            record.summary = Some(summary_of(record));
            (Outcome::Ok, text)
        }
        Err(message) => {
            let outcome = if cancelled() { Outcome::Cancelled } else { Outcome::Error };
            record.state = if cancelled() { State::Cancelled } else { State::Failed };
            record.error = Some(first_line(&message));
            record.summary = record.error.clone();
            (outcome, message)
        }
    };
    "over".clone_into(&mut record.phase);
    let _ = fs::write(result_path(&loc.state_dir, &record.id), &text);
    let summary = record.summary.clone().unwrap_or_default();
    live.write(&loc.state_dir, true);
    live.running.finish(outcome, &summary);
}

/// A job as it is shown: its record, how long it has run, and, when asked, whether the mod's files
/// are still the ones the job was started with.
#[derive(Debug, Serialize)]
pub struct View {
    #[serde(flatten)]
    pub record: Record,
    pub elapsed_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

impl View {
    pub fn new(record: Record, check_files: bool) -> Self {
        let end = record.finished_at.unwrap_or_else(now_ms);
        let files = check_files.then(|| files_since_start(&record));
        let note = match record.state {
            State::Interrupted => Some(
                "The xTiger server that ran this job stopped before it ended. The game may still be \
                 open, and dlc_load.json is put back by the next run.",
            ),
            State::Running => Some(
                "Still running. A client that stopped waiting has not stopped it: ask again with \
                 ck3_job status.",
            ),
            _ => None,
        };
        Self {
            elapsed_secs: end.saturating_sub(record.started_at).div_euclid(1000),
            record,
            files,
            note,
        }
    }
}

fn files_since_start(record: &Record) -> &'static str {
    match (&record.snapshot, record.mod_dir.as_deref().and_then(projects::manifest_hash)) {
        (Some(then), Some(now)) if *then == now => "unchanged_since_start",
        (Some(_), Some(_)) => "changed_since_start",
        _ => "unknown",
    }
}

/// The record of a job, after waiting up to `wait` for it to end.
pub fn status(
    state_dir: &Path,
    id: &str,
    wait: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Record, String> {
    let started = Instant::now();
    loop {
        let record = load(state_dir, id).ok_or_else(|| unknown(id))?;
        if record.state.is_over() || started.elapsed() >= wait.min(MAX_WAIT) || cancelled() {
            return Ok(record);
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn unknown(id: &str) -> String {
    format!("No job {id:?}. ck3_job with action list shows the jobs on record.")
}

/// Stop a job of this server, and wait a moment for it to say so.
pub fn cancel(state_dir: &Path, id: &str) -> Result<Record, String> {
    let record = load(state_dir, id).ok_or_else(|| unknown(id))?;
    if record.state.is_over() {
        return Err(format!("Job {id} is not running ({:?}).", record.state));
    }
    let flag = LIVE.lock().ok().and_then(|live| {
        live.iter()
            .find(|(live_id, _, handle)| live_id == id && !handle.is_finished())
            .map(|(_, cancel, _)| Arc::clone(cancel))
    });
    let Some(flag) = flag else {
        return Err(format!(
            "Job {id} runs in another xTiger server (process {}); cancel it from there.",
            record.owner
        ));
    };
    flag.store(true, Ordering::SeqCst);
    // Killing the game and putting the mod list back takes a few seconds.
    status(state_dir, id, Duration::from_secs(20), &|| false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn record(id: &str, state: State, owner: u32) -> Record {
        Record {
            id: id.to_owned(),
            state,
            phase: "loading".to_owned(),
            owner,
            started_at: 1_000,
            updated_at: 1_000,
            finished_at: None,
            mod_ref: None,
            mod_dir: None,
            snapshot: None,
            background: false,
            play: "k_france".to_owned(),
            bookmark: "bm_867_carolingians".to_owned(),
            commands_total: 2,
            commands_sent: 0,
            commands_seen: 0,
            budget_secs: 900,
            progress: None,
            summary: None,
            error: None,
            screenshot: None,
            report: PathBuf::from("report.txt"),
        }
    }

    #[test]
    fn the_driver_output_says_how_far_the_run_is() {
        let mut job = record("job-1", State::Running, std::process::id());
        for line in [
            "launched pid 4242",
            "sent: event a.0001 (confirmed)",
            "sent: event a.0002 (not logged yet)",
            "",
        ] {
            note_line(&mut job, line);
        }
        assert_eq!((job.commands_sent, job.commands_seen), (2, 1));
        assert_eq!(job.phase, "commands");
        assert_eq!(job.progress.as_deref(), Some("sent: event a.0002 (not logged yet)"));
        note_line(&mut job, "report C:\\x\\report.txt (80 lines)");
        assert_eq!(job.phase, "finishing");
    }

    #[test]
    fn a_job_whose_server_is_gone_is_interrupted_and_ids_are_not_paths() {
        let tmp = TempDir::new();
        // Run by this process, but nothing runs it: the server restarted under the same pid.
        save(&tmp, &record("job-1", State::Running, std::process::id()));
        assert_eq!(load(&tmp, "job-1").unwrap().state, State::Interrupted);
        // Run by another process that wrote to it a moment ago: it is still going.
        save(&tmp, &record("job-2", State::Running, std::process::id() + 1));
        assert_eq!(load(&tmp, "job-2").unwrap().state, State::Running);
        save(&tmp, &record("job-3", State::Finished, std::process::id() + 1));
        assert_eq!(load(&tmp, "job-3").unwrap().state, State::Finished);
        for bad in ["", "..", "../job-1", "job-1.json", "a/b", "a\\b"] {
            assert!(load(&tmp, bad).is_none(), "{bad:?}");
            assert!(result_text(&tmp, bad).is_none());
        }
        let err = status(&tmp, "nope", Duration::ZERO, &|| false).unwrap_err();
        assert!(err.contains("No job"), "{err}");
    }

    #[test]
    fn a_status_call_returns_at_once_for_a_job_that_is_over_and_waits_for_one_that_is_not() {
        let tmp = TempDir::new();
        save(&tmp, &record("job-1", State::Finished, 1));
        let begun = Instant::now();
        let found = status(&tmp, "job-1", Duration::from_secs(20), &|| false).unwrap();
        assert_eq!(found.state, State::Finished);
        assert!(begun.elapsed() < Duration::from_secs(2));
        save(&tmp, &record("job-2", State::Running, std::process::id() + 1));
        let begun = Instant::now();
        let found = status(&tmp, "job-2", Duration::from_millis(600), &|| false).unwrap();
        assert_eq!(found.state, State::Running);
        assert!(begun.elapsed() >= Duration::from_millis(500));
        // A caller that is cancelled is not kept waiting.
        let begun = Instant::now();
        status(&tmp, "job-2", Duration::from_secs(20), &|| true).unwrap();
        assert!(begun.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn only_a_job_of_this_server_can_be_cancelled_and_only_while_it_runs() {
        let tmp = TempDir::new();
        save(&tmp, &record("job-1", State::Finished, 1));
        assert!(cancel(&tmp, "job-1").unwrap_err().contains("not running"));
        save(&tmp, &record("job-2", State::Running, std::process::id() + 1));
        assert!(cancel(&tmp, "job-2").unwrap_err().contains("another xTiger server"));
        assert!(cancel(&tmp, "job-9").unwrap_err().contains("No job"));
    }

    #[test]
    fn old_records_go_but_a_running_one_stays() {
        let tmp = TempDir::new();
        for n in 0..KEPT + 3 {
            let state = if n == 0 { State::Running } else { State::Finished };
            let owner = std::process::id() + 1;
            save(&tmp, &record(&format!("job-{n:03}"), state, owner));
            fs::write(result_path(&tmp, &format!("job-{n:03}")), "report").unwrap();
        }
        prune(&tmp);
        let kept = list(&tmp);
        assert_eq!(kept.len(), KEPT + 1);
        // The oldest one still runs, so it stays; the next two are the ones that went.
        assert!(load(&tmp, "job-000").is_some());
        assert!(load(&tmp, "job-001").is_none());
        assert!(result_text(&tmp, "job-002").is_none());
        assert!(result_text(&tmp, "job-022").is_some());
        // Newest first.
        assert_eq!(kept[0].id, "job-022");
    }

    #[test]
    fn a_result_says_whether_the_mod_is_still_the_one_that_was_run() {
        let tmp = TempDir::new();
        let mod_dir = tmp.join("mod");
        fs::create_dir_all(mod_dir.join("events")).unwrap();
        fs::write(mod_dir.join("events/a.txt"), "namespace = a\n").unwrap();
        let mut job = record("job-1", State::Finished, 1);
        job.finished_at = Some(4_000);
        job.mod_dir = Some(mod_dir.clone());
        assert_eq!(View::new(job.clone(), true).files, Some("unknown"));
        job.snapshot = projects::manifest_hash(&mod_dir);
        assert!(job.snapshot.is_some());
        let view = View::new(job.clone(), true);
        assert_eq!(view.files, Some("unchanged_since_start"));
        assert_eq!(view.elapsed_secs, 3);
        assert_eq!(View::new(job.clone(), false).files, None);
        fs::write(mod_dir.join("events/a.txt"), "namespace = b\n").unwrap();
        assert_eq!(View::new(job, true).files, Some("changed_since_start"));
    }

    #[test]
    fn a_job_that_runs_says_a_client_that_gave_up_has_not_stopped_it() {
        let view = View::new(record("job-1", State::Running, 1), false);
        assert!(view.note.unwrap().contains("has not stopped it"));
        assert!(View::new(record("job-2", State::Interrupted, 1), false).note.is_some());
        assert!(View::new(record("job-3", State::Failed, 1), false).note.is_none());
    }

    #[test]
    fn a_live_job_is_found_by_id_and_stop_all_ends_it() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let handle = thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10));
            }
        });
        LIVE.lock().unwrap().push(("job-live-test".to_owned(), cancel, handle));
        assert!(is_live("job-live-test"));
        assert!(!is_live("job-other"));
        stop_all();
        assert!(!is_live("job-live-test"));
    }

    #[test]
    fn jobs_are_listed_newest_first_with_their_state() {
        let tmp = TempDir::new();
        assert_eq!(list(&tmp).len(), 0);
        save(&tmp, &record("job-1", State::Finished, 1));
        save(&tmp, &record("job-2", State::Failed, 1));
        let ids: Vec<(String, State)> =
            list(&tmp).into_iter().map(|job| (job.id, job.state)).collect();
        assert_eq!(
            ids,
            vec![("job-2".to_owned(), State::Failed), ("job-1".to_owned(), State::Finished)]
        );
    }
}
