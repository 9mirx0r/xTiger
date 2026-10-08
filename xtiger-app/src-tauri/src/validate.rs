//! Running the validator. It runs as its own process (`ck3-tiger --json`) so that a run can be
//! cancelled by ending that process, and so that each run starts from a clean state.
//!
//! Each check is saved with the MCP server's runs, so the app and the assistants share one
//! history: what is new is measured against the last check of the mod, whoever ran it.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use xtiger_mcp::runs::{self, NewRun};

/// The name the app's own checks are saved under.
pub const BY: &str = "xTiger app";

#[derive(Debug, Default)]
pub struct Runner {
    child: Mutex<Option<Child>>,
    cancelled: AtomicBool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    /// The reports as the validator wrote them, each with an added `isNew` field.
    pub reports: Vec<Value>,
    pub duration_ms: u64,
    /// The number of reports in the previous run of this mod, if there was one.
    pub previous_count: Option<usize>,
    pub new_count: usize,
}

/// The validator sits next to the app's own executable, both in development and when installed.
fn validator_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "ck3-tiger.exe" } else { "ck3-tiger" };
    let path = exe.with_file_name(name);
    if path.is_file() { Ok(path) } else { Err(format!("cannot find {name} next to the app")) }
}

pub struct RunArgs<'a> {
    pub mod_file: &'a Path,
    pub mod_name: Option<String>,
    pub game: &'a Path,
    pub paradox: Option<&'a Path>,
    /// The shared runs folder, if the app's data folder is known.
    pub runs_dir: Option<&'a Path>,
}

impl Runner {
    pub fn run(&self, app: &AppHandle, args: &RunArgs) -> Result<RunResult, String> {
        let validator = validator_path()?;
        let mut command = Command::new(&validator);
        command.arg("--json").arg("--game").arg(args.game);
        if let Some(paradox) = args.paradox {
            command.arg("--paradox").arg(paradox);
        }
        command.arg(args.mod_file).stdout(Stdio::piped()).stderr(Stdio::piped());
        let command_line: Vec<String> = std::iter::once(validator.display().to_string())
            .chain(command.get_args().map(|arg| arg.to_string_lossy().into_owned()))
            .collect();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let started = Instant::now();
        self.cancelled.store(false, Ordering::SeqCst);
        let mut child = command.spawn().map_err(|e| format!("cannot start the validator: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("no output from the validator")?;
        let stderr = child.stderr.take().ok_or("no output from the validator")?;
        *self.child.lock().unwrap() = Some(child);

        let out_thread = thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        });
        let log_app = app.clone();
        let err_thread = thread::spawn(move || {
            let mut lines = Vec::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    let _ = log_app.emit("validate-log", &line);
                    lines.push(line);
                }
            }
            lines
        });

        let status = loop {
            let mut guard = self.child.lock().unwrap();
            let Some(child) = guard.as_mut() else { return Err("cancelled".to_owned()) };
            match child.try_wait() {
                Ok(Some(status)) => {
                    *guard = None;
                    break status;
                }
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
            drop(guard);
            thread::sleep(Duration::from_millis(100));
        };
        let output = out_thread.join().unwrap_or_default();
        let log = err_thread.join().unwrap_or_default();

        if self.cancelled.load(Ordering::SeqCst) {
            return Err("cancelled".to_owned());
        }
        if !status.success() {
            let tail = log.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join("\n");
            return Err(if tail.is_empty() {
                format!("the validator stopped ({status})")
            } else {
                tail
            });
        }

        let mut reports: Vec<Value> = if output.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&output).map_err(|e| format!("cannot read the reports: {e}"))?
        };
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let run = NewRun {
            mod_file: args.mod_file.to_path_buf(),
            mod_name: args.mod_name.clone(),
            by: Some(BY.to_owned()),
            tiger: validator,
            tiger_source: "next to the app".to_owned(),
            command: command_line,
            exit_code: status.code(),
            seconds: (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        };
        let (previous_count, new_count) = record(app, args, run, &mut reports);
        Ok(RunResult { reports, duration_ms, previous_count, new_count })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Save this check with the shared runs and mark each report that the last check of the mod
/// lacked. Returns the number of reports of that last check, and how many are new.
fn record(
    app: &AppHandle,
    args: &RunArgs,
    run: NewRun,
    reports: &mut [Value],
) -> (Option<usize>, usize) {
    let previous: Option<Vec<String>> = args
        .runs_dir
        .and_then(|dir| runs::newest_in(dir, args.mod_file))
        .map(|(_, reports)| reports.iter().map(runs::fingerprint).collect())
        .or_else(|| legacy_prints(app, args.mod_file));
    if let Some(Err(e)) = args.runs_dir.map(|dir| runs::save_run(dir, run, reports)) {
        let _ = app.emit("validate-log", format!("Could not save this check: {e}"));
    }
    let new_count = runs::mark_new(previous.as_deref(), reports);
    (previous.map(|previous| previous.len()), new_count)
}

/// Before the app shared its runs, it kept the fingerprints of each mod's last run in its own
/// file. They are still read for a mod with no shared run yet, so its first check after the
/// update still shows what is new.
fn run_file(app: &AppHandle, mod_file: &Path) -> Option<PathBuf> {
    // FNV-1a, so the name does not change between builds.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in mod_file.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let dir = crate::settings::data_dir(app)?.join("runs");
    Some(dir.join(format!("{hash:016x}.json")))
}

/// The number of reports in the last run of this mod, from the app's own old file, and when that
/// run ended in milliseconds since the Unix epoch.
pub fn legacy_previous_run(app: &AppHandle, mod_file: &Path) -> Option<(usize, u64)> {
    let file = run_file(app, mod_file)?;
    let modified = fs::metadata(&file).ok()?.modified().ok()?;
    let at = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    let prints: Vec<String> = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    Some((prints.len(), u64::try_from(at).unwrap_or(u64::MAX)))
}

/// The fingerprints of the last run of this mod in the app's own old file.
fn legacy_prints(app: &AppHandle, mod_file: &Path) -> Option<Vec<String>> {
    let text = fs::read_to_string(run_file(app, mod_file)?).ok()?;
    serde_json::from_str(&text).ok()
}
