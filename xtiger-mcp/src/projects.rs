//! Update projects: a durable record of one mod being brought to a newer game version.
//!
//! The record lives outside the window of saved runs, so it survives more than `KEEP_RUNS`
//! checks of any mod, and it is kept by an id rather than by a path, so moving the work to a new
//! folder does not lose what it was compared with. It holds the explicit baseline (a run and the
//! definitions the mod had when the project began), the decisions made about removed content, and
//! the manifest hash that says whether a saved result still belongs to the files on disk.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::journal::{self, Outcome, now_ms};
use crate::ledger::{self, Decision, Snapshot};
use crate::locate::{Locations, game_version};
use crate::mods;
use crate::runs::{self, RunMeta};
use crate::triage::{self, Annotation, Triage};

/// File types whose bytes decide what a mod does or how it is validated.
const MANIFEST_EXTENSIONS: [&str; 10] =
    ["txt", "yml", "gui", "json", "mod", "font", "settings", "asset", "gfx", "shader"];

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The SHA-256 of a file, as lower-case hex.
fn file_hash(path: &Path) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).ok()?);
    Some(hex(&hasher.finalize()))
}

fn files_of(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(folder) = todo.pop() {
        let Ok(entries) = fs::read_dir(&folder) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            // A link is not followed, so it cannot lead the walk out of the mod.
            if kind.is_dir() {
                if entry.file_name() != ".git" {
                    todo.push(path);
                }
            } else if kind.is_file()
                && path.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
                    MANIFEST_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known))
                })
            {
                let rel = path.strip_prefix(dir).unwrap_or(&path);
                found.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    found.sort();
    found
}

/// A file and the hash of its bytes, in path order.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
}

/// The script, localization, descriptor and interface files of a mod with their hashes.
#[must_use]
pub fn manifest(mod_dir: &Path) -> Vec<ManifestEntry> {
    files_of(mod_dir)
        .into_iter()
        .filter_map(|(path, full)| Some(ManifestEntry { path, sha256: file_hash(&full)? }))
        .collect()
}

/// One hash for the whole manifest, or `None` when the mod has no such files. Two folders with
/// the same files give the same hash wherever they are.
#[must_use]
pub fn manifest_hash(mod_dir: &Path) -> Option<String> {
    let entries = manifest(mod_dir);
    if entries.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    for entry in &entries {
        hasher.update(entry.path.to_lowercase().as_bytes());
        hasher.update([0]);
        hasher.update(entry.sha256.as_bytes());
        hasher.update([0]);
    }
    Some(hex(&hasher.finalize()))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// The `.mod` file that is being changed: the working copy when there is one.
    pub mod_file: PathBuf,
    /// The mod as it was when the project began, when work happens on a copy of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_mod_file: Option<PathBuf>,
    pub created_at: u64,
    /// The game version installed when the project began, and the one it is moved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    /// A copy of the game version the mod was written for, when there is one (`base` of
    /// `xtiger_overrides`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_game: Option<PathBuf>,
    /// The saved run chosen as the "before": the mod's state when the project began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_run: Option<String>,
    /// The manifest hash of the mod when the project began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_manifest: Option<String>,
    /// The hash of the validator binary when the project began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validator_sha256: Option<String>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    /// Notes on diagnostics. The diagnostics themselves stay in every run and count.
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

pub fn projects_dir(loc: &Locations) -> PathBuf {
    loc.state_dir.join("projects")
}

fn record_path(loc: &Locations, id: &str) -> PathBuf {
    projects_dir(loc).join(format!("{id}.json"))
}

fn baseline_path(loc: &Locations, id: &str) -> PathBuf {
    projects_dir(loc).join(format!("{id}.baseline.json"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    // Written beside the target and renamed over it, so a crash never leaves half a record.
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, text).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("cannot write {}: {e}", path.display())
    })
}

fn save(loc: &Locations, project: &Project) -> Result<(), String> {
    write_json(&record_path(loc, &project.id), project)
}

/// # Errors
/// If no project has this id.
pub fn load(loc: &Locations, id: &str) -> Result<Project, String> {
    if !valid_id(id) {
        return Err(format!(
            "{id:?} is not a project id. xtiger_project with action list shows them."
        ));
    }
    let text = fs::read_to_string(record_path(loc, id)).map_err(|_| {
        format!("There is no project {id}. xtiger_project with action list shows them.")
    })?;
    serde_json::from_str(&text).map_err(|e| format!("The record of project {id} is damaged: {e}"))
}

/// All projects, newest first.
#[must_use]
pub fn list(loc: &Locations) -> Vec<Project> {
    let mut found: Vec<Project> = fs::read_dir(projects_dir(loc))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension().is_some_and(|ext| ext == "json")
                        && !path.to_string_lossy().ends_with(".baseline.json")
                })
                .filter_map(|path| serde_json::from_str(&fs::read_to_string(path).ok()?).ok())
                .collect()
        })
        .unwrap_or_default();
    found.sort_by(|a: &Project, b| b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)));
    found
}

fn baseline_snapshot(loc: &Locations, id: &str) -> Option<Snapshot> {
    serde_json::from_str(&fs::read_to_string(baseline_path(loc, id)).ok()?).ok()
}

/// Copy a mod folder. Links are not followed and `.git` is left behind.
fn copy_tree(from: &Path, to: &Path) -> Result<usize, String> {
    let mut copied = 0;
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))? {
        let Ok(entry) = entry else { continue };
        let Ok(kind) = entry.file_type() else { continue };
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                copied += copy_tree(&entry.path(), &target)?;
            }
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|e| format!("cannot copy {}: {e}", entry.path().display()))?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Whether `inner` is `outer` or inside it. Whole folder names are compared, so `mod_v2` is not
/// inside `mod`.
fn inside(inner: &Path, outer: &Path) -> bool {
    let inner = runs::mod_key(&mods::clean(inner));
    let outer = runs::mod_key(&mods::clean(outer));
    let outer = outer.trim_end_matches('\\');
    inner == outer || inner.strip_prefix(outer).is_some_and(|rest| rest.starts_with('\\'))
}

#[derive(Debug)]
pub struct Create<'a> {
    pub mod_file: &'a Path,
    pub name: Option<String>,
    /// Work on a copy of the mod in this new folder, so the original is never touched.
    pub copy_to: Option<&'a Path>,
    pub old_game: Option<&'a Path>,
    /// The saved run to compare with. The mod's newest run when left out.
    pub baseline_run: Option<&'a str>,
}

/// Start a project for a mod.
///
/// # Errors
/// If the mod cannot be read, the copy cannot be made, or the record cannot be written.
pub fn create(loc: &Locations, request: &Create) -> Result<Project, String> {
    let info = mods::describe(request.mod_file)
        .ok_or_else(|| format!("Cannot read {}.", request.mod_file.display()))?;
    if info.path_missing {
        return Err(format!("The mod folder {} does not exist.", info.dir.display()));
    }
    if let Some(old) = request.old_game
        && !old.join("game").is_dir()
    {
        return Err(format!("The old game folder {} has no game/ folder.", old.display()));
    }
    let runs_dir = runs::runs_dir(loc);
    let baseline_run = match request.baseline_run {
        Some(id) => {
            runs::run_by_id(&runs_dir, id).ok_or_else(|| format!("There is no saved run {id}."))?;
            Some(id.to_owned())
        }
        None => runs::newest_in(&runs_dir, request.mod_file).map(|(meta, _)| meta.run_id),
    };
    let name = request.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or(info.name.clone());
    let mut id = runs::slug(&name);
    let stem = id.clone();
    let mut n = 2;
    while record_path(loc, &id).exists() {
        id = format!("{stem}-{n}");
        n += 1;
    }
    let baseline = baseline_for(&info.dir);
    let (mod_file, original) = match request.copy_to {
        None => (request.mod_file.to_path_buf(), None),
        Some(to) => {
            if inside(to, &info.dir) || inside(&info.dir, to) {
                return Err("The copy must be in a folder apart from the mod's own.".to_owned());
            }
            if to.exists() && fs::read_dir(to).is_ok_and(|mut e| e.next().is_some()) {
                return Err(format!(
                    "{} already holds files. Pick a new or empty folder.",
                    to.display()
                ));
            }
            copy_tree(&info.dir, to)?;
            let descriptor = to.join("descriptor.mod");
            if !descriptor.is_file() {
                // The original's own .mod file points at its folder; the copy's does not need to.
                let text = fs::read_to_string(request.mod_file).unwrap_or_default();
                let kept: Vec<&str> = text
                    .lines()
                    .filter(|line| !line.trim_start().to_lowercase().starts_with("path"))
                    .collect();
                fs::write(&descriptor, kept.join("\n"))
                    .map_err(|e| format!("cannot write {}: {e}", descriptor.display()))?;
            }
            (mods::clean(&descriptor), Some(request.mod_file.to_path_buf()))
        }
    };
    let project = Project {
        id,
        name,
        mod_file,
        original_mod_file: original,
        created_at: now_ms(),
        game_version: loc.game.as_deref().and_then(game_version),
        old_game: request.old_game.map(Path::to_path_buf),
        baseline_run,
        baseline_manifest: manifest_hash(&info.dir),
        validator_sha256: loc.validator.as_deref().and_then(file_hash),
        decisions: Vec::new(),
        annotations: Vec::new(),
    };
    write_json(&baseline_path(loc, &project.id), &baseline)?;
    save(loc, &project)?;
    Ok(project)
}

fn baseline_for(mod_dir: &Path) -> Snapshot {
    ledger::snapshot(mod_dir)
}

/// How a saved run relates to the files on disk.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RunView {
    pub run_id: String,
    pub finished_at: u64,
    pub reports: usize,
    pub errors: usize,
    /// `current`: made for these exact files. `stale`: the files changed after it (or it is for
    /// other files). `unknown`: the run did not record a hash.
    pub files: &'static str,
}

fn run_view(meta: &RunMeta, reports: &[Value], expected: Option<&str>) -> RunView {
    let errors = reports
        .iter()
        .filter(|r| matches!(r.get("severity").and_then(Value::as_str), Some("error" | "fatal")))
        .count();
    let files = match (meta.snapshot.as_deref(), expected) {
        (Some(have), Some(want)) if have == want => "current",
        (Some(_), _) => "stale",
        (None, _) => "unknown",
    };
    RunView {
        run_id: meta.run_id.clone(),
        finished_at: meta.finished_at,
        reports: reports.len(),
        errors,
        files,
    }
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub project: Project,
    pub game_version_now: Option<String>,
    /// The game was updated after the project began, so older results may not apply.
    pub game_changed: bool,
    pub validator_changed: Option<bool>,
    pub manifest_now: Option<String>,
    /// Edited since the project began.
    pub mod_changed: bool,
    pub latest_run: Option<RunView>,
    pub baseline_run: Option<RunView>,
    pub pinned_runs: Vec<RunView>,
    pub ledger: Option<ledger::Counts>,
    pub review: Option<ledger::Review>,
    /// How the latest run's diagnostics stand against the notes of the project.
    pub triage: Option<Triage>,
    pub update: UpdateStatus,
    pub notes: Vec<&'static str>,
}

/// Where the update stands, stage by stage. A stage that did not run says so: nothing here turns
/// green because it was never looked at.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct UpdateStatus {
    /// `completed` (the validator finished a saved run) or `not_run`.
    pub process: &'static str,
    /// `clean`, `errors`, `stale` (the run is for other files) or `not_run`.
    #[serde(rename = "static")]
    pub static_check: &'static str,
    /// `not_run`, or `ran_after_last_edit` / `ran_before_last_edit` / `failed` from the journal's
    /// last `ck3_run`. It says the game was started, not that what it tested passed.
    pub runtime: &'static str,
    /// `not_run` (no run to review), `pending` or `done`.
    pub review: &'static str,
    /// True only when every stage above is done and nothing blocks.
    pub update_complete: bool,
    pub blocking: Vec<String>,
}

/// The newest change time of the mod's files, in milliseconds since the Unix epoch.
fn last_edit(dir: &Path) -> Option<u64> {
    files_of(dir)
        .into_iter()
        .filter_map(|(_, path)| fs::metadata(path).ok()?.modified().ok())
        .filter_map(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .filter_map(|age| u64::try_from(age.as_millis()).ok())
        .max()
}

/// The last `ck3_run` of the project's mod: when it finished and whether it did.
fn last_game_run(loc: &Locations, project: &Project) -> Option<(u64, bool)> {
    let mine = |file: &Path| {
        runs::mod_key(file) == runs::mod_key(&project.mod_file)
            || project
                .original_mod_file
                .as_deref()
                .is_some_and(|o| runs::mod_key(file) == runs::mod_key(o))
    };
    journal::read_entries(&loc.state_dir, 500)
        .into_iter()
        .filter(|entry| entry.tool == "ck3_run")
        .find(|entry| {
            entry.finished_at.is_some() && entry.mod_ref.as_ref().is_some_and(|m| mine(&m.file))
        })
        .and_then(|entry| Some((entry.finished_at?, entry.outcome == Some(Outcome::Ok))))
}

fn update_status(
    latest: Option<&RunView>,
    runtime: &'static str,
    review_done: Option<bool>,
    triage: Option<&Triage>,
    ledger_undecided: usize,
    ledger_restore: usize,
) -> UpdateStatus {
    // A run is saved only when the validator finished and returned reports.
    let process = if latest.is_some() { "completed" } else { "not_run" };
    let static_check = match latest {
        None => "not_run",
        Some(run) if run.files != "current" => "stale",
        Some(run) if run.errors > 0 => "errors",
        Some(_) => "clean",
    };
    let review = match (latest, review_done) {
        (None, _) | (_, None) => "not_run",
        (_, Some(true)) => "done",
        (_, Some(false)) => "pending",
    };
    let mut blocking = Vec::new();
    if process == "not_run" {
        blocking.push("The mod has not been validated.".to_owned());
    }
    match static_check {
        "stale" => blocking.push(
            "The latest run is for other files than the ones on disk: validate again.".to_owned(),
        ),
        "errors" => {
            blocking.push(format!("The latest run has {} errors.", latest.map_or(0, |r| r.errors)));
        }
        _ => {}
    }
    match runtime {
        "not_run" => blocking.push("The game has not been run with this mod (ck3_run).".to_owned()),
        "ran_before_last_edit" => {
            blocking.push("The mod was edited after the last game run: run it again.".to_owned());
        }
        "failed" => blocking.push("The last game run did not finish.".to_owned()),
        _ => {}
    }
    if ledger_undecided > 0 {
        blocking.push(format!(
            "{ledger_undecided} removed or dropped definitions still need a decision."
        ));
    }
    if ledger_restore > 0 {
        blocking.push(format!(
            "{ledger_restore} definitions were decided to be restored and are still missing from the mod."
        ));
    }
    if let Some(open) = triage.map(Triage::open_items).filter(|n| *n > 0) {
        blocking.push(format!(
            "{open} diagnostics are unreviewed, need a new look or are marked as defects."
        ));
    }
    if review == "not_run" {
        blocking
            .push("Nothing has been reviewed: the project has no baseline or no run.".to_owned());
    }
    UpdateStatus {
        process,
        static_check,
        runtime,
        review,
        update_complete: blocking.is_empty(),
        blocking,
    }
}

fn mod_dir_of(mod_file: &Path) -> Result<PathBuf, String> {
    mods::describe(mod_file)
        .map(|info| info.dir)
        .ok_or_else(|| format!("Cannot read {}.", mod_file.display()))
}

/// Where a project stands: what changed, which results still belong to the files, and what
/// still needs a decision.
///
/// # Errors
/// If the project's mod cannot be read.
pub fn status(loc: &Locations, project: &Project) -> Result<Status, String> {
    let dir = mod_dir_of(&project.mod_file)?;
    let now = manifest_hash(&dir);
    let runs_dir = runs::runs_dir(loc);
    let newest = runs::newest_in(&runs_dir, &project.mod_file);
    let latest_run = newest.as_ref().map(|(meta, reports)| run_view(meta, reports, now.as_deref()));
    let triage =
        newest.as_ref().map(|(_, reports)| triage::triage(reports, &project.annotations, &dir));
    let baseline_run = project.baseline_run.as_deref().and_then(|id| {
        runs::run_by_id(&runs_dir, id)
            .map(|(meta, reports)| run_view(&meta, &reports, project.baseline_manifest.as_deref()))
    });
    let pinned_runs = runs::pins(&runs_dir)
        .iter()
        .filter_map(|id| runs::run_by_id(&runs_dir, id))
        .filter(|(meta, _)| {
            runs::mod_key(&meta.mod_file) == runs::mod_key(&project.mod_file)
                || project
                    .original_mod_file
                    .as_deref()
                    .is_some_and(|o| runs::mod_key(&meta.mod_file) == runs::mod_key(o))
        })
        .map(|(meta, reports)| run_view(&meta, &reports, now.as_deref()))
        .collect();
    let (counts, review) = match baseline_snapshot(loc, &project.id) {
        Some(before) => {
            let entries = ledger::diff(&before, &ledger::snapshot(&dir));
            let summary = ledger::summarize(&entries, &project.decisions, 0);
            (Some(summary.counts), Some(summary.review))
        }
        None => (None, None),
    };
    let game_version_now = loc.game.as_deref().and_then(game_version);
    let validator_now = loc.validator.as_deref().and_then(file_hash);
    let runtime = match (last_game_run(loc, project), last_edit(&dir)) {
        (None, _) => "not_run",
        (Some((_, false)), _) => "failed",
        (Some((at, true)), Some(edit)) if at < edit => "ran_before_last_edit",
        (Some(_), _) => "ran_after_last_edit",
    };
    let ledger_undecided = review.as_ref().map_or(0, |r| r.pending.len());
    let ledger_restore = review.as_ref().map_or(0, |r| r.restore.len());
    let review_done = review
        .as_ref()
        .zip(triage.as_ref())
        .map(|(r, t)| r.pending.is_empty() && r.restore.is_empty() && t.open_items() == 0);
    let update = update_status(
        latest_run.as_ref(),
        runtime,
        review_done,
        triage.as_ref(),
        ledger_undecided,
        ledger_restore,
    );
    Ok(Status {
        triage,
        update,
        game_changed: project.game_version.is_some() && project.game_version != game_version_now,
        validator_changed: project
            .validator_sha256
            .as_ref()
            .zip(validator_now)
            .map(|(then, now)| *then != now),
        mod_changed: project.baseline_manifest != now,
        manifest_now: now,
        game_version_now,
        latest_run,
        baseline_run,
        pinned_runs,
        ledger: counts,
        review,
        project: project.clone(),
        notes: vec![
            "A run marked stale was made for other files than the ones on disk now: check again before using its result.",
            "The baseline is the mod as it was when the project began. xtiger_project with action ledger lists what changed since, and what still needs a decision.",
        ],
    })
}

/// The ledger of the working mod against the project's baseline.
///
/// # Errors
/// If the mod cannot be read or the project has no baseline.
pub fn ledger_of(loc: &Locations, project: &Project) -> Result<Vec<ledger::Entry>, String> {
    let before = baseline_snapshot(loc, &project.id)
        .ok_or_else(|| format!("Project {} has no saved baseline.", project.id))?;
    Ok(ledger::diff(&before, &ledger::snapshot(&mod_dir_of(&project.mod_file)?)))
}

/// How many matching definitions an ambiguity error names before it only counts the rest.
const MAX_LISTED: usize = 10;

/// The error for a name that several definitions match. Each name it lists is a `definition` that
/// `decide` takes back as it is.
fn ambiguous(wanted: &str, matches: &[&ledger::Entry]) -> String {
    let mut listed: Vec<String> =
        matches.iter().take(MAX_LISTED).map(|e| format!("`{}`", e.definition)).collect();
    if matches.len() > MAX_LISTED {
        listed.push(format!("… and {} more", matches.len() - MAX_LISTED));
    }
    format!(
        "{wanted} matches more than one definition. Pass one of these exactly as `definition`: {}.",
        listed.join(", ")
    )
}

/// Record a decision about a removed definition or a dropped call.
///
/// # Errors
/// If the definition has no such difference, the reason is empty, or the record cannot be saved.
pub fn decide(
    loc: &Locations,
    id: &str,
    definition: &str,
    decision: &str,
    reason: &str,
    by: &str,
) -> Result<(Project, ledger::Review), String> {
    if !matches!(decision, "intended" | "restore") {
        return Err("decision must be `intended` or `restore`.".to_owned());
    }
    if reason.trim().is_empty() {
        return Err("Say why in `reason`: the decision is kept with the project.".to_owned());
    }
    let mut project = load(loc, id)?;
    let entries = ledger_of(loc, &project)?;
    let wanted = definition.trim();
    let suffix = format!(": {wanted}");
    let candidates: Vec<&ledger::Entry> = entries
        .iter()
        .filter(|e| {
            e.needs_decision() && (e.definition == wanted || e.definition.ends_with(&suffix))
        })
        .collect();
    let found = match candidates.as_slice() {
        [only] => *only,
        [] => {
            return Err(format!(
                "{wanted} has no difference that needs a decision. xtiger_project with action ledger lists them."
            ));
        }
        many => return Err(ambiguous(wanted, many)),
    };
    project.decisions.push(Decision {
        definition: found.definition.clone(),
        hash: found.hash.clone(),
        decision: decision.to_owned(),
        reason: reason.trim().to_owned(),
        by: by.to_owned(),
    });
    save(loc, &project)?;
    let review = ledger::content_review(&entries, &project.decisions);
    Ok((project, review))
}

/// Which diagnostics to write a note on, and the note.
#[derive(Debug, Clone, Copy)]
pub struct Annotate<'a> {
    pub key: &'a str,
    /// A part of the path; empty for any.
    pub path: &'a str,
    /// 0 for any line.
    pub line: usize,
    /// The message of the one diagnostic to note. Then `path` is its whole first location and
    /// `line` its line, and no other diagnostic gets the note, not even one on the same line.
    /// Left out, every diagnostic of the key, path and line gets it.
    pub message: Option<&'a str>,
    pub kind: &'a str,
    pub note: &'a str,
    pub by: &'a str,
}

/// What an `annotate` call did.
#[derive(Debug, Serialize)]
pub struct Annotated {
    pub kind: String,
    pub annotated: usize,
    /// Diagnostics in the latest run that this selector picked: every one stays in the run.
    pub examples: Vec<String>,
}

/// Write a note on the diagnostics of the latest run that match a key, a part of the path and a
/// line, or with a `message`, the one diagnostic with that message at that path and line. The
/// diagnostics are not removed from the run or from any count.
///
/// # Errors
/// If the kind is unknown, the note is empty, nothing matches, too many match, or the record
/// cannot be saved.
pub fn annotate(
    loc: &Locations,
    id: &str,
    what: &Annotate,
) -> Result<(Project, Annotated), String> {
    let Annotate { key, path, line, message, kind, note, by } = *what;
    if !triage::KINDS.contains(&kind) {
        return Err(format!("kind must be one of {}.", triage::KINDS.join(", ")));
    }
    if note.trim().is_empty() {
        return Err("Say why in `note`: it is kept with the project.".to_owned());
    }
    if key.trim().is_empty() {
        return Err(
            "annotate needs `key`: the diagnostic's key, as xtiger_reports shows it.".to_owned()
        );
    }
    let mut project = load(loc, id)?;
    let dir = mod_dir_of(&project.mod_file)?;
    let (_, reports) = runs::newest_in(&runs::runs_dir(loc), &project.mod_file)
        .ok_or("The project's mod has no saved run: validate it first.")?;
    let picked = match message {
        Some(message) => triage::select_spot(&reports, key.trim(), path.trim(), line, message),
        None => triage::select(&reports, key.trim(), path.trim(), line),
    };
    if picked.is_empty() {
        return Err(match message {
            Some(_) => "No diagnostic with that message is at that place. Validate the mod again.",
            None => "No diagnostic of the latest run matches that key, path and line.",
        }
        .to_owned());
    }
    if picked.len() > triage::MAX_BULK {
        return Err(format!(
            "{} diagnostics match: narrow it with `path` or `line` (at most {} at a time).",
            picked.len(),
            triage::MAX_BULK
        ));
    }
    let at = now_ms();
    let mut examples = Vec::new();
    for report in &picked {
        let who = triage::identity(report, &dir);
        project.annotations.retain(|n| n.id != who.id);
        project.annotations.push(triage::annotation(&who, kind, note, by, at));
        if examples.len() < 5 {
            examples.push(format!("{} {}", who.path, who.key));
        }
    }
    save(loc, &project)?;
    let done = Annotated { kind: kind.to_owned(), annotated: picked.len(), examples };
    Ok((project, done))
}

/// # Errors
/// If the project or the run cannot be found, or the pin list cannot be written.
pub fn pin(loc: &Locations, id: &str, run_id: &str, pinned: bool) -> Result<(), String> {
    load(loc, id)?;
    runs::set_pin(&runs::runs_dir(loc), run_id, pinned)
}

/// Make a saved run the project's baseline.
///
/// # Errors
/// If the run is not saved or the record cannot be written.
pub fn set_baseline(loc: &Locations, id: &str, run_id: &str) -> Result<Project, String> {
    let mut project = load(loc, id)?;
    runs::run_by_id(&runs::runs_dir(loc), run_id)
        .ok_or_else(|| format!("There is no saved run {run_id}."))?;
    project.baseline_run = Some(run_id.to_owned());
    save(loc, &project)?;
    Ok(project)
}

/// What an export wrote.
#[derive(Debug, Serialize)]
pub struct Exported {
    pub folder: PathBuf,
    pub files: Vec<String>,
    pub unresolved: Vec<String>,
}

/// Write the evidence of a project into a new folder: the record, the manifest, the ledger, and
/// a copy of each run that matters. The original mod is not part of it and is never touched.
///
/// # Errors
/// If the folder already holds files or cannot be written.
pub fn export(loc: &Locations, project: &Project, to: &Path) -> Result<Exported, String> {
    if to.exists() && fs::read_dir(to).is_ok_and(|mut e| e.next().is_some()) {
        return Err(format!("{} already holds files. Pick a new or empty folder.", to.display()));
    }
    let status = status(loc, project)?;
    let entries = ledger_of(loc, project)?;
    let summary = ledger::summarize(&entries, &project.decisions, usize::MAX);
    let dir = mod_dir_of(&project.mod_file)?;
    let mut written = Vec::new();
    put(to, "project.json", &status, &mut written)?;
    put(to, "manifest.json", &manifest(&dir), &mut written)?;
    put(to, "ledger.json", &summary, &mut written)?;
    let runs_dir = runs::runs_dir(loc);
    // The pins of this project's mod only: pins are kept for every mod in one list.
    let mut keep: Vec<String> = status.pinned_runs.iter().map(|run| run.run_id.clone()).collect();
    keep.extend(project.baseline_run.clone());
    keep.extend(runs::newest_in(&runs_dir, &project.mod_file).map(|(meta, _)| meta.run_id));
    keep.sort();
    keep.dedup();
    for run_id in keep {
        if let Some(from) = Some(runs_dir.join(format!("{run_id}.json"))).filter(|p| p.is_file()) {
            let name = format!("runs/{run_id}.json");
            fs::create_dir_all(to.join("runs")).map_err(|e| e.to_string())?;
            fs::copy(&from, to.join(&name)).map_err(|e| format!("cannot copy {run_id}: {e}"))?;
            written.push(name);
        }
    }
    let mut unresolved = summary.review.pending.clone();
    unresolved.extend(summary.review.restore.iter().cloned());
    Ok(Exported { folder: to.to_path_buf(), files: written, unresolved })
}

fn put(
    to: &Path,
    name: &str,
    value: &impl Serialize,
    written: &mut Vec<String>,
) -> Result<(), String> {
    write_json(&to.join(name), value)?;
    written.push(name.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use serde_json::json;

    fn locations(state: &Path) -> Locations {
        Locations {
            validator: None,
            validator_from: "test",
            game: None,
            game_from: "test",
            user_dir: state.join("user"),
            user_from: "test",
            state_dir: state.join("state"),
            state_from: "test",
            extra_mods: Vec::new(),
            game_tests_background: true,
        }
    }

    fn make_mod(dir: &Path, event: &str) -> PathBuf {
        fs::create_dir_all(dir.join("events")).unwrap();
        fs::write(dir.join("descriptor.mod"), "name=\"Demo\"\nversion=\"1\"\n").unwrap();
        fs::write(dir.join("events/x.txt"), event).unwrap();
        dir.join("descriptor.mod")
    }

    const EVENT: &str = "ev.1 = {\n\ttype = character_event\n\toption = { name = a }\n}\nev.2 = {\n\ttype = character_event\n}\n";

    #[test]
    fn the_manifest_follows_the_bytes_not_the_place() {
        let tmp = TempDir::new();
        make_mod(&tmp.join("a"), EVENT);
        make_mod(&tmp.join("b"), EVENT);
        assert_eq!(manifest_hash(&tmp.join("a")), manifest_hash(&tmp.join("b")));
        fs::write(tmp.join("b/events/x.txt"), "ev.1 = { }\n").unwrap();
        assert_ne!(manifest_hash(&tmp.join("a")), manifest_hash(&tmp.join("b")));
        assert_eq!(manifest_hash(&tmp.join("missing")), None);
    }

    #[test]
    fn a_project_keeps_its_baseline_when_the_work_moves_to_a_copy() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let original = make_mod(&tmp.join("orig"), EVENT);
        let project = create(
            &loc,
            &Create {
                mod_file: &original,
                name: None,
                copy_to: Some(&tmp.join("work")),
                old_game: None,
                baseline_run: None,
            },
        )
        .unwrap();
        assert_eq!(project.original_mod_file.as_deref(), Some(original.as_path()));
        assert!(project.mod_file.starts_with(tmp.join("work")));
        // The original is never touched; the copy is edited.
        fs::write(tmp.join("work/events/x.txt"), "ev.1 = {\n\ttype = character_event\n}\n")
            .unwrap();
        assert!(fs::read_to_string(tmp.join("orig/events/x.txt")).unwrap().contains("ev.2"));

        let loaded = load(&loc, &project.id).unwrap();
        let status = status(&loc, &loaded).unwrap();
        assert!(status.mod_changed);
        let counts = status.ledger.unwrap();
        assert_eq!((counts.removed, counts.changed), (1, 1));
        assert_eq!(status.review.unwrap().state, "needs_decision");
    }

    #[test]
    fn a_copy_may_sit_beside_the_mod_under_a_longer_name_but_not_inside_it() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let original = make_mod(&tmp.join("demo"), EVENT);
        let copy = |to: &Path| {
            create(
                &loc,
                &Create {
                    mod_file: &original,
                    name: None,
                    copy_to: Some(to),
                    old_game: None,
                    baseline_run: None,
                },
            )
        };
        assert!(copy(&tmp.join("demo").join("work")).unwrap_err().contains("apart from"));
        assert!(copy(&tmp.join("demo")).is_err());
        let beside = tmp.join("demo_120");
        assert!(copy(&beside).is_ok());
        assert!(beside.join("events/x.txt").is_file());
    }

    #[test]
    fn removing_content_needs_a_decision_bound_to_that_removal() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = create(
            &loc,
            &Create {
                mod_file: &mod_file,
                name: None,
                copy_to: None,
                old_game: None,
                baseline_run: None,
            },
        )
        .unwrap();
        fs::write(
            tmp.join("m/events/x.txt"),
            "ev.1 = {\n\ttype = character_event\n\toption = { name = a }\n}\n",
        )
        .unwrap();
        assert!(decide(&loc, &project.id, "ev.2", "intended", "  ", "t").is_err());
        let (saved, review) =
            decide(&loc, &project.id, "ev.2", "intended", "dropped on purpose", "t").unwrap();
        assert_eq!((saved.decisions.len(), review.state), (1, "decided"));
        // A different removal after the decision is not covered by it.
        fs::write(tmp.join("m/events/x.txt"), "ev.9 = { }\n").unwrap();
        let again = status(&loc, &load(&loc, &project.id).unwrap()).unwrap();
        assert_eq!(again.review.unwrap().state, "needs_decision");
    }

    /// A mod whose event `ev.1` names the key `ev.1.t`, with that key's text in each language.
    fn titled_mod(dir: &Path, languages: &[&str]) -> PathBuf {
        let mod_file = make_mod(dir, "ev.1 = {\n\ttype = character_event\n\ttitle = ev.1.t\n}\n");
        for lang in languages {
            let folder = dir.join("localization").join(lang);
            fs::create_dir_all(&folder).unwrap();
            fs::write(
                folder.join(format!("x_l_{lang}.yml")),
                format!("\u{feff}l_{lang}:\n ev.1.t:0 \"Title\"\n"),
            )
            .unwrap();
        }
        mod_file
    }

    #[test]
    fn an_ambiguous_name_lists_each_definition_as_decide_takes_it() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = titled_mod(&tmp.join("m"), &["english", "french"]);
        let project = started(&loc, &mod_file);
        fs::remove_dir_all(tmp.join("m/localization")).unwrap();
        let Err(message) =
            decide(&loc, &project.id, "ev.1.t", "intended", "dropped on purpose", "t")
        else {
            panic!("both languages name the key, so the name matches two definitions");
        };
        assert_eq!(
            message,
            "ev.1.t matches more than one definition. Pass one of these exactly as `definition`: `localization/english: ev.1.t`, `localization/french: ev.1.t`."
        );
        // Each listed form is taken back as it is, and then names exactly one definition.
        let (saved, _) = decide(
            &loc,
            &project.id,
            "localization/french: ev.1.t",
            "intended",
            "dropped on purpose",
            "t",
        )
        .unwrap();
        assert_eq!(saved.decisions[0].definition, "localization/french: ev.1.t");
    }

    #[test]
    fn a_long_list_of_matches_names_ten_and_counts_the_rest() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let languages = [
            "english",
            "french",
            "german",
            "spanish",
            "russian",
            "korean",
            "simp_chinese",
            "japanese",
            "polish",
            "turkish",
            "braz_por",
            "italian",
        ];
        let mod_file = titled_mod(&tmp.join("m"), &languages);
        let project = started(&loc, &mod_file);
        fs::remove_dir_all(tmp.join("m/localization")).unwrap();
        let Err(message) =
            decide(&loc, &project.id, "ev.1.t", "intended", "dropped on purpose", "t")
        else {
            panic!("all twelve languages name the key");
        };
        assert_eq!(message.matches("`localization/").count(), MAX_LISTED);
        assert!(
            message.ends_with("`localization/simp_chinese: ev.1.t`, … and 2 more."),
            "{message}"
        );
    }

    #[test]
    fn pinned_runs_survive_the_window_and_edited_files_make_a_run_stale() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = create(
            &loc,
            &Create {
                mod_file: &mod_file,
                name: None,
                copy_to: None,
                old_game: None,
                baseline_run: None,
            },
        )
        .unwrap();
        let dir = runs::runs_dir(&loc);
        let run = |snapshot: Option<String>| runs::NewRun {
            mod_file: mod_file.clone(),
            mod_name: Some("Demo".to_owned()),
            by: None,
            tiger: PathBuf::from("ck3-tiger"),
            tiger_source: "test".to_owned(),
            command: vec![],
            exit_code: Some(0),
            seconds: 1.0,
            loaded_mods: vec![],
            snapshot,
        };
        let first = runs::save_run(&dir, run(manifest_hash(&tmp.join("m"))), &[]).unwrap();
        pin(&loc, &project.id, &first.run_id, true).unwrap();
        for _ in 0..runs::KEEP_RUNS + 3 {
            runs::save_run(&dir, run(None), &[]).unwrap();
        }
        assert!(runs::saved_run(&dir, &first.run_id).is_some(), "the pinned run is kept");
        assert!(pin(&loc, &project.id, "nope", true).is_err());

        let before = status(&loc, &project).unwrap();
        assert_eq!(before.pinned_runs[0].files, "current");
        fs::write(tmp.join("m/events/x.txt"), "ev.1 = { }\n").unwrap();
        let after = status(&loc, &project).unwrap();
        assert_eq!(after.pinned_runs[0].files, "stale");
        pin(&loc, &project.id, &first.run_id, false).unwrap();
        runs::save_run(&dir, run(None), &[]).unwrap();
        assert!(runs::saved_run(&dir, &first.run_id).is_none(), "an unpinned old run is pruned");
    }

    fn started(loc: &Locations, mod_file: &Path) -> Project {
        create(
            loc,
            &Create { mod_file, name: None, copy_to: None, old_game: None, baseline_run: None },
        )
        .unwrap()
    }

    fn save_clean_run(loc: &Locations, mod_file: &Path, reports: &[Value]) {
        let dir = mod_file.parent().unwrap();
        runs::save_run(
            &runs::runs_dir(loc),
            runs::NewRun {
                mod_file: mod_file.to_path_buf(),
                mod_name: Some("Demo".to_owned()),
                by: None,
                tiger: PathBuf::from("ck3-tiger"),
                tiger_source: "test".to_owned(),
                command: vec![],
                exit_code: Some(0),
                seconds: 1.0,
                loaded_mods: vec![],
                snapshot: manifest_hash(dir),
            },
            reports,
        )
        .unwrap();
    }

    #[test]
    fn a_clean_static_run_is_not_a_complete_update_without_a_game_run() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = started(&loc, &mod_file);
        let none = status(&loc, &project).unwrap().update;
        assert_eq!(
            (none.process, none.static_check, none.runtime),
            ("not_run", "not_run", "not_run")
        );
        assert!(!none.update_complete);

        save_clean_run(&loc, &mod_file, &[]);
        let clean = status(&loc, &project).unwrap().update;
        assert_eq!((clean.process, clean.static_check), ("completed", "clean"));
        assert_eq!(clean.runtime, "not_run");
        assert!(!clean.update_complete);
        assert!(clean.blocking.iter().any(|b| b.contains("ck3_run")));

        // With every stage done and nothing open, nothing blocks.
        let run = RunView {
            run_id: "r".to_owned(),
            finished_at: 1,
            reports: 0,
            errors: 0,
            files: "current",
        };
        let all = update_status(
            Some(&run),
            "ran_after_last_edit",
            Some(true),
            Some(&Triage::default()),
            0,
            0,
        );
        assert!(all.update_complete && all.blocking.is_empty());
        // A definition decided to be restored is decided, but the mod still lacks it.
        let restoring = update_status(
            Some(&run),
            "ran_after_last_edit",
            Some(true),
            Some(&Triage::default()),
            1,
            2,
        );
        assert!(!restoring.update_complete);
        assert!(restoring.blocking.iter().any(|b| b.starts_with("1 removed or dropped")));
        assert!(
            restoring
                .blocking
                .iter()
                .any(|b| b.starts_with("2 definitions were decided to be restored"))
        );
        let only_restore = update_status(
            Some(&run),
            "ran_after_last_edit",
            Some(true),
            Some(&Triage::default()),
            0,
            1,
        );
        assert!(only_restore.blocking.iter().all(|b| !b.contains("need a decision")));
        assert_eq!(only_restore.blocking.len(), 1);
        let edited = update_status(
            Some(&run),
            "ran_before_last_edit",
            Some(true),
            Some(&Triage::default()),
            0,
            0,
        );
        assert!(!edited.update_complete);
        let errors = RunView { errors: 2, ..run };
        let failing = update_status(
            Some(&errors),
            "ran_after_last_edit",
            Some(true),
            Some(&Triage::default()),
            0,
            0,
        );
        assert_eq!(failing.static_check, "errors");
        assert!(!failing.update_complete);
    }

    #[test]
    fn a_reviewed_diagnostic_stays_counted_and_needs_review_again_when_its_block_changes() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = started(&loc, &mod_file);
        let report = json!({"severity": "warning", "key": "duplicate-item",
            "message": "ev.1 is redefined", "locations": [{"path": "events/x.txt", "linenr": 2}]});
        save_clean_run(&loc, &mod_file, &[report]);

        assert!(
            annotate(
                &loc,
                &project.id,
                &Annotate {
                    key: "duplicate-item",
                    path: "events",
                    line: 0,
                    message: None,
                    kind: "intentional_override",
                    note: " ",
                    by: "t",
                },
            )
            .is_err()
        );
        let what = Annotate {
            key: "duplicate-item",
            path: "events",
            line: 0,
            message: None,
            kind: "intentional_override",
            note: "replaces the vanilla event on purpose",
            by: "t",
        };
        let (_, done) = annotate(&loc, &project.id, &what).unwrap();
        assert_eq!(done.annotated, 1);
        let reviewed = status(&loc, &load(&loc, &project.id).unwrap()).unwrap();
        assert_eq!(reviewed.latest_run.as_ref().map(|r| r.reports), Some(1));
        let triage = reviewed.triage.unwrap();
        assert_eq!(triage.reviewed.get("intentional_override"), Some(&1));
        assert_eq!(triage.open_items(), 0);

        fs::write(
            tmp.join("m/events/x.txt"),
            "ev.1 = {\n\ttype = character_event\n\toption = { name = b }\n}\n",
        )
        .unwrap();
        let changed = status(&loc, &load(&loc, &project.id).unwrap()).unwrap().triage.unwrap();
        assert_eq!((changed.diagnostics, changed.needs_review), (1, 1));
        assert!(changed.reviewed.is_empty());
        assert!(annotate(&loc, &project.id, &Annotate { key: "nope", ..what }).is_err());
    }

    #[test]
    fn a_note_with_a_message_is_for_its_report_alone_even_on_a_shared_line() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = started(&loc, &mod_file);
        let report = |message: &str| {
            json!({"severity": "warning", "key": "duplicate-item", "message": message,
                "locations": [{"path": "events/x.txt", "linenr": 2}]})
        };
        save_clean_run(
            &loc,
            &mod_file,
            &[report("ev.1 is redefined"), report("ev.2 is redefined")],
        );

        let note = |message| Annotate {
            key: "duplicate-item",
            path: "events/x.txt",
            line: 2,
            message,
            kind: "intentional_override",
            note: "replaces the vanilla event on purpose",
            by: "t",
        };
        assert!(annotate(&loc, &project.id, &note(Some("ev.3 is redefined"))).is_err());
        let (_, one) = annotate(&loc, &project.id, &note(Some(" ev.1  is redefined "))).unwrap();
        assert_eq!(one.annotated, 1);
        let saved = load(&loc, &project.id).unwrap();
        assert_eq!(saved.annotations.len(), 1);
        assert_eq!(saved.annotations[0].message, "ev.1 is redefined");
        let triage = status(&loc, &saved).unwrap().triage.unwrap();
        assert_eq!((triage.reviewed.get("intentional_override"), triage.unreviewed), (Some(&1), 1));

        let (_, both) = annotate(&loc, &project.id, &note(None)).unwrap();
        assert_eq!(both.annotated, 2);
        assert_eq!(load(&loc, &project.id).unwrap().annotations.len(), 2);
    }

    #[test]
    fn many_diagnostics_on_one_line_need_a_message_to_be_told_apart() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = started(&loc, &mod_file);
        let reports: Vec<Value> = (0..=triage::MAX_BULK)
            .map(|n| {
                json!({"severity": "warning", "key": "duplicate-item",
                    "message": format!("ev.{n} is redefined"),
                    "locations": [{"path": "events/x.txt", "linenr": 2}]})
            })
            .collect();
        save_clean_run(&loc, &mod_file, &reports);

        let last = format!("ev.{} is redefined", triage::MAX_BULK);
        let note = |message| Annotate {
            key: "duplicate-item",
            path: "events/x.txt",
            line: 2,
            message,
            kind: "intentional_override",
            note: "replaces the vanilla event on purpose",
            by: "t",
        };
        let refused = annotate(&loc, &project.id, &note(None)).err().unwrap_or_default();
        assert!(refused.contains("narrow it"), "{refused}");
        let (_, done) = annotate(&loc, &project.id, &note(Some(last.as_str()))).unwrap();
        assert_eq!(done.annotated, 1);
        assert_eq!(load(&loc, &project.id).unwrap().annotations.len(), 1);
    }

    #[test]
    fn an_export_holds_the_evidence_and_refuses_a_used_folder() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let project = create(
            &loc,
            &Create {
                mod_file: &mod_file,
                name: None,
                copy_to: None,
                old_game: None,
                baseline_run: None,
            },
        )
        .unwrap();
        let out = tmp.join("out");
        let exported = export(&loc, &project, &out).unwrap();
        assert!(exported.files.contains(&"ledger.json".to_owned()));
        assert!(out.join("manifest.json").is_file());
        assert!(export(&loc, &project, &out).is_err());
    }

    #[test]
    fn an_export_leaves_out_the_pinned_runs_of_other_mods() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let mod_file = make_mod(&tmp.join("m"), EVENT);
        let other = make_mod(&tmp.join("other"), EVENT);
        let project = started(&loc, &mod_file);
        save_clean_run(&loc, &mod_file, &[]);
        save_clean_run(&loc, &other, &[]);
        let runs_dir = runs::runs_dir(&loc);
        let mine = runs::newest_in(&runs_dir, &mod_file).unwrap().0.run_id;
        let theirs = runs::newest_in(&runs_dir, &other).unwrap().0.run_id;
        runs::set_pin(&runs_dir, &mine, true).unwrap();
        runs::set_pin(&runs_dir, &theirs, true).unwrap();
        let exported = export(&loc, &project, &tmp.join("out")).unwrap();
        assert!(exported.files.contains(&format!("runs/{mine}.json")), "{:?}", exported.files);
        assert!(!exported.files.iter().any(|f| f.contains(&theirs)), "{:?}", exported.files);
    }
}
