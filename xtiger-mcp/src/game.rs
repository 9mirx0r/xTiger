//! Driving a real CK3 session through the bundled PowerShell scripts (Windows only), and reading
//! the game's logs.

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};

use crate::locate::Locations;
use crate::runs::{most_common, no_window};

pub const LOGS: [&str; 6] = ["error", "game", "exceptions", "debug", "database_conflicts", "setup"];

const SCRIPTS: [(&str, &str); 3] = [
    ("ck3-input.ps1", include_str!("../scripts/ck3-input.ps1")),
    ("ck3-keys.ps1", include_str!("../scripts/ck3-keys.ps1")),
    ("ck3-run.ps1", include_str!("../scripts/ck3-run.ps1")),
];

/// Write the scripts where PowerShell can run them. They are rewritten every time so they always
/// match this build.
fn scripts_dir(loc: &Locations) -> Result<PathBuf, String> {
    let dir = loc.state_dir.join("scripts");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    for (name, text) in SCRIPTS {
        // Windows PowerShell 5.1 reads files without a byte order mark in the ANSI code page.
        let body = format!("\u{feff}{text}");
        let path = dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(body.as_str()) {
            fs::write(&path, body).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        }
    }
    Ok(dir)
}

/// A fresh folder for one game run's files.
fn work_dir(loc: &Locations, prefix: &str) -> Result<PathBuf, String> {
    let base = loc.state_dir.join("game");
    // Clear out what earlier runs left behind, except the last few.
    if let Ok(entries) = fs::read_dir(&base) {
        let mut old: Vec<PathBuf> =
            entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
        old.sort();
        let excess = old.len().saturating_sub(10);
        for dir in old.into_iter().take(excess) {
            let _ = fs::remove_dir_all(dir);
        }
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = base.join(format!("{nanos}-{prefix}"));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Read a child's output to the end on its own thread, so a full pipe never blocks the child.
fn drain(stream: Option<impl Read + Send + 'static>) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Where `ck3-run.ps1` keeps the user's `dlc_load.json` while a run has changed it: line 1 is the
/// list it wrote for the run, the rest is the original (or `NO_ORIGINAL`).
const DLC_BACKUP: &str = "dlc_load.json.xtiger-backup";
const NO_ORIGINAL: &str = "xtiger:no-original-file";
/// First line of the temporary `.mod` copies the script puts in the user's mod folder.
const TEMP_MOD_MARKER: &str = "# xtiger-temp-mod";

/// Put the user's mod list back after a run that could not do it itself (the script was killed).
/// The file is only touched if it still holds the list the run wrote (or the start of it, when the
/// run was killed while writing it): if the launcher or the user has changed it since, that is
/// theirs. Returns whether the original was restored.
pub fn recover_dlc_load(user_dir: &Path) -> bool {
    let mut restored = false;
    let backup = user_dir.join(DLC_BACKUP);
    if let Ok(raw) = fs::read_to_string(&backup) {
        let target = user_dir.join("dlc_load.json");
        if let Some((written, original)) = raw.split_once('\n') {
            let current = fs::read_to_string(&target).unwrap_or_default();
            if written.trim().starts_with(current.trim()) {
                restored = if original == NO_ORIGINAL {
                    fs::remove_file(&target).is_ok()
                } else {
                    fs::write(&target, original).is_ok()
                };
            }
        }
        let _ = fs::remove_file(&backup);
    }
    // The temporary mod files, recognised by name and marker so nothing of the user's is removed.
    // They are written before the backup, so they are looked for even when there is none.
    if let Ok(entries) = fs::read_dir(user_dir.join("mod")) {
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let ours = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mod"))
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("xtiger-run-"))
                && fs::read_to_string(&path).is_ok_and(|text| {
                    text.trim_start_matches('\u{feff}').starts_with(TEMP_MOD_MARKER)
                });
            if ours {
                let _ = fs::remove_file(path);
            }
        }
    }
    restored
}

/// Whether a `ck3.exe` is running: a game started by hand, or by a run of another xTiger server.
/// Its mod list and temporary mod files are then left alone.
fn ck3_running() -> bool {
    no_window(Command::new("tasklist").args(["/FI", "IMAGENAME eq ck3.exe", "/FO", "CSV", "/NH"]))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| {
            String::from_utf8_lossy(&out.stdout).to_ascii_lowercase().contains("\"ck3.exe\"")
        })
}

/// One game run at a time in this server: a second one would take the first one's mod list for
/// the user's own, and could not start the game anyway.
static RUN_LOCK: Mutex<()> = Mutex::new(());

/// The same rule across servers (Claude Code and the xTiger app each start their own): a file in
/// the game's user folder, with the process id of its owner. It goes when the run ends.
const LOCK_FILE: &str = "xtiger-run.lock";
/// A lock older than this is stale whatever its owner, in case the id was reused by another
/// process. The longest run is far shorter.
const LOCK_MAX_AGE: Duration = Duration::from_secs(3 * 60 * 60);
/// A lock with no readable id yet was only just created by another server.
const LOCK_GRACE: Duration = Duration::from_secs(10);

struct RunLock(PathBuf);

impl Drop for RunLock {
    fn drop(&mut self) {
        // Only while it is still ours: a server that took it over as stale owns it now.
        let ours = fs::read_to_string(&self.0)
            .is_ok_and(|text| text.trim() == std::process::id().to_string());
        if ours {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn pid_alive(pid: u32) -> bool {
    no_window(Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
}

/// Take the run lock, or say who holds it. A lock whose owner is gone is taken over.
fn take_run_lock(user_dir: &Path, alive: &dyn Fn(u32) -> bool) -> Result<RunLock, String> {
    let path = user_dir.join(LOCK_FILE);
    for _ in 0..3 {
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write as _;
                let _ = write!(file, "{}", std::process::id());
                return Ok(RunLock(path));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let age = fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.elapsed().ok())
                    .unwrap_or_default();
                let owner =
                    fs::read_to_string(&path).ok().and_then(|text| text.trim().parse().ok());
                let busy = match owner {
                    Some(pid) => age < LOCK_MAX_AGE && alive(pid),
                    None => age < LOCK_GRACE,
                };
                if busy {
                    let who = owner.map_or_else(String::new, |pid| format!(" (process {pid})"));
                    return Err(format!(
                        "Another xTiger server{who} is running a game right now; wait for it to                          finish. If no run is going, delete {}.",
                        path.display()
                    ));
                }
                let _ = fs::remove_file(&path);
            }
            Err(e) => return Err(format!("cannot create {}: {e}", path.display())),
        }
    }
    Err(format!("cannot take the run lock {}", path.display()))
}

/// Stop a script and everything it started (the game too). Killing only the script would leave
/// `ck3.exe` running and `dlc_load.json` changed.
fn kill_tree(child: &mut Child) {
    let pid = child.id().to_string();
    let _ = no_window(Command::new("taskkill").args(["/PID", &pid, "/T", "/F"]))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

/// Why a game run counts as failed rather than as a result, if it does. `code` is the script's
/// exit code (`None`: it was stopped), `report` whether it got as far as writing its report.
/// Commands the game never logged do not fail a run; they are listed in the report.
/// Exit code 2 of `ck3-run.ps1` means a mod was not mounted by the game.
fn failure_lead(code: Option<i32>, report: bool) -> Option<&'static str> {
    match (code, report) {
        (Some(0), _) => None,
        (None, _) => Some("The run did not finish."),
        (Some(_), false) => Some("The run stopped before it could test anything."),
        (Some(2), true) => Some("A mod was not loaded by the game, so the run did not test it."),
        (Some(_), true) => Some("The run failed."),
    }
}

const WINDOWS_ONLY: &str =
    "Game control needs Windows (it sends input through the Win32 SendInput API).";

fn powershell(
    loc: &Locations,
    script: &str,
    args: &[String],
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    if !cfg!(windows) {
        return Err(WINDOWS_ONLY.to_owned());
    }
    let script = scripts_dir(loc)?.join(script);
    let mut child = no_window(
        Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .args(args),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("cannot start PowerShell: {e}"))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let code = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.code();
        }
        if cancelled() || started.elapsed() > timeout {
            kill_tree(&mut child);
            let stopped = if cancelled() {
                "Cancelled.".to_owned()
            } else {
                format!("Stopped after {} seconds.", timeout.as_secs())
            };
            let stdout = out.join().unwrap_or_default();
            return Ok(Outcome {
                code: None,
                stdout: format!("{}\n{stopped}", stdout.trim()),
                stderr: String::new(),
            });
        }
        thread::sleep(Duration::from_millis(200));
    };
    Ok(Outcome {
        code,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

fn status(outcome: &Outcome) -> String {
    let mut text = outcome.stdout.trim().to_owned();
    if outcome.code != Some(0) {
        match outcome.code {
            Some(code) => {
                let _ = write!(text, "\nFAILED (exit {code})");
            }
            None => text.push_str("\nFAILED"),
        }
        if !outcome.stderr.trim().is_empty() {
            text.push('\n');
            text.push_str(outcome.stderr.trim());
        }
    }
    text
}

#[derive(Debug)]
pub struct RunGame<'a> {
    pub commands: &'a [String],
    pub screenshot: bool,
    pub bookmark: &'a str,
    pub play: &'a str,
    pub keep_open: bool,
    pub mods: &'a [PathBuf],
    pub load_timeout: u64,
    pub typing: &'a str,
}

/// Start CK3, load a game, type the commands, and report. Returns the report and the
/// screenshot, if one was taken.
pub fn run_game(
    loc: &Locations,
    run: &RunGame,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    let game = loc.require_game()?;
    if !["unicode", "scancode"].contains(&run.typing) {
        return Err("typing must be unicode or scancode".to_owned());
    }
    let work = work_dir(loc, "run")?;
    let shot = run.screenshot.then(|| work.join("screen.png"));
    let report = work.join("report.txt");
    let mut args: Vec<String> = [
        "-GameDir",
        &game.display().to_string(),
        "-UserDir",
        &loc.user_dir.display().to_string(),
        "-Bookmark",
        run.bookmark,
        "-Play",
        run.play,
        "-LoadTimeout",
        &run.load_timeout.to_string(),
        "-Report",
        &report.display().to_string(),
        "-Typing",
        run.typing,
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect();
    if !run.commands.is_empty() {
        let file = work.join("commands.txt");
        fs::write(&file, run.commands.join("\n")).map_err(|e| e.to_string())?;
        args.extend(["-CmdFile".to_owned(), file.display().to_string()]);
    }
    if !run.mods.is_empty() {
        let mods: Vec<String> = run.mods.iter().map(|path| path.display().to_string()).collect();
        args.extend(["-Mod".to_owned(), mods.join(";")]);
    }
    if let Some(shot) = &shot {
        args.extend(["-Shot".to_owned(), shot.display().to_string()]);
    }
    if run.keep_open {
        args.push("-Keep".to_owned());
    }
    let commands = u64::try_from(run.commands.len()).unwrap_or(u64::MAX);
    let timeout = Duration::from_secs(run.load_timeout + 60 + 30 * commands);
    // Before the locks and the mod list recovery, which use Windows tools.
    if !cfg!(windows) {
        return Err(WINDOWS_ONLY.to_owned());
    }
    let _one_at_a_time = match RUN_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err("Another ck3_run is still going; wait for it to finish.".to_owned());
        }
    };
    let _across_servers = take_run_lock(&loc.user_dir, &pid_alive)?;
    // A run that was killed earlier may have left the mod list changed; the script would take
    // that list for the user's own. Not while a game is running: the list is its own then, and the
    // script refuses to start anyway.
    if !ck3_running() {
        recover_dlc_load(&loc.user_dir);
    }
    let mut outcome = powershell(loc, "ck3-run.ps1", &args, timeout, cancelled)?;
    if outcome.code.is_none() {
        // The script was stopped together with the game, so it could not put the list back itself.
        outcome.stdout.push_str("\nThe script and the game it started were stopped.");
        if recover_dlc_load(&loc.user_dir) {
            outcome.stdout.push_str(" Your mod list (dlc_load.json) was put back.");
        }
    }
    let mut text = status(&outcome);
    let report_bytes = fs::read(&report).ok();
    if let Some(bytes) = &report_bytes {
        text.push_str("\n\n");
        text.push_str(String::from_utf8_lossy(bytes).trim_start_matches('\u{feff}'));
    }
    let shot = shot.filter(|path| path.is_file());
    // Nothing was tested when the script failed, was stopped, or a mod was not loaded: that is an
    // error for the caller, not a result, but what the run did find stays in the message.
    if let Some(lead) = failure_lead(outcome.code, report_bytes.is_some()) {
        if let Some(shot) = &shot {
            let _ = write!(text, "\nScreenshot: {}", shot.display());
        }
        // The lead is a line of its own: the journal and the app show the first line.
        return Err(format!("{lead}\n{text}"));
    }
    Ok((text, shot))
}

/// Send keys and text to the open CK3 window.
pub fn send_keys(
    loc: &Locations,
    scancodes: &str,
    text: &str,
    screenshot: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    let shot = if screenshot { Some(work_dir(loc, "keys")?.join("screen.png")) } else { None };
    let mut args = Vec::new();
    if !scancodes.is_empty() {
        args.extend(["-Sc".to_owned(), scancodes.to_owned()]);
    }
    if !text.is_empty() {
        args.extend(["-Text".to_owned(), text.to_owned()]);
    }
    if let Some(shot) = &shot {
        args.extend(["-Shot".to_owned(), shot.display().to_string()]);
    }
    let outcome = powershell(loc, "ck3-keys.ps1", &args, Duration::from_secs(120), cancelled)?;
    Ok((status(&outcome), shot.filter(|path| path.is_file())))
}

#[derive(Debug)]
pub struct ReadLog<'a> {
    pub name: &'a str,
    pub max_lines: usize,
    pub dedupe: bool,
    pub tail: bool,
    pub pattern: &'a str,
}

/// Read a log from the CK3 user folder.
pub fn read_log(user_dir: &Path, read: &ReadLog) -> Result<String, String> {
    if !LOGS.contains(&read.name) {
        return Err(format!("name must be one of {}", LOGS.join(", ")));
    }
    let pattern = if read.pattern.is_empty() {
        None
    } else {
        Some(
            RegexBuilder::new(read.pattern)
                .case_insensitive(true)
                .build()
                .map_err(|e| format!("pattern is not a valid regex: {e}"))?,
        )
    };
    let path = user_dir.join("logs").join(format!("{}.log", read.name));
    let file_name = format!("{}.log", read.name);
    let Ok(bytes) = fs::read(&path) else {
        return Ok(format!("{file_name} does not exist yet."));
    };
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    let total = all.len();
    let mut lines: Vec<String> = match &pattern {
        Some(regex) => {
            all.iter().filter(|line| regex.is_match(line)).map(|line| (*line).to_owned()).collect()
        }
        None => all.iter().map(|line| (*line).to_owned()).collect(),
    };
    let mut head = format!("{file_name}: {total} lines");
    if pattern.is_some() {
        let _ = write!(head, ", {} matching '{}'", lines.len(), read.pattern);
    }
    if read.dedupe {
        let stamp = Regex::new(r"^\[[0-9:.]+\]").map_err(|e| e.to_string())?;
        let stripped = lines.iter().map(|line| stamp.replace(line, "").into_owned());
        lines = most_common(stripped).into_iter().map(|(line, n)| format!("{n}x {line}")).collect();
    }
    if lines.len() > read.max_lines {
        let cut = lines.len() - read.max_lines;
        if read.tail {
            let mut kept = vec![format!("... {cut} earlier lines")];
            kept.extend(lines.split_off(cut));
            lines = kept;
        } else {
            lines.truncate(read.max_lines);
            lines.push(format!("... {cut} more lines"));
        }
    }
    Ok(std::iter::once(head).chain(lines).collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn setup() -> TempDir {
        let tmp = TempDir::new();
        fs::create_dir_all(tmp.join("logs")).unwrap();
        fs::write(
            tmp.join("logs/error.log"),
            "[12:00:00.001] Missing faith\n[12:00:01.002] Missing faith\n[12:00:02.003] Invalid value\n\
             [12:00:03.004] Another warning\n",
        )
        .unwrap();
        tmp
    }

    fn log(dir: &Path, max_lines: usize, dedupe: bool, tail: bool, pattern: &str) -> String {
        read_log(dir, &ReadLog { name: "error", max_lines, dedupe, tail, pattern }).unwrap()
    }

    #[test]
    fn head_tail_and_truncation() {
        let tmp = setup();
        assert_eq!(
            log(&tmp, 2, false, false, ""),
            "error.log: 4 lines\n[12:00:00.001] Missing faith\n[12:00:01.002] Missing faith\n... 2 more lines"
        );
        assert_eq!(
            log(&tmp, 2, false, true, ""),
            "error.log: 4 lines\n... 2 earlier lines\n[12:00:02.003] Invalid value\n[12:00:03.004] Another warning"
        );
        let all = log(&tmp, 4, false, true, "");
        assert!(!all.contains("..."));
        assert_eq!(log(&tmp, 0, false, false, ""), "error.log: 4 lines\n... 4 more lines");
        assert_eq!(log(&tmp, 0, false, true, ""), "error.log: 4 lines\n... 4 earlier lines");
    }

    #[test]
    fn patterns_and_dedupe() {
        let tmp = setup();
        assert_eq!(
            log(&tmp, 1, false, true, "MISSING.*faith"),
            "error.log: 4 lines, 2 matching 'MISSING.*faith'\n... 1 earlier lines\n[12:00:01.002] Missing faith"
        );
        assert_eq!(
            log(&tmp, 10, true, false, ""),
            "error.log: 4 lines\n2x  Missing faith\n1x  Invalid value\n1x  Another warning"
        );
        assert_eq!(
            log(&tmp, 1, true, false, ""),
            "error.log: 4 lines\n2x  Missing faith\n... 2 more lines"
        );
        assert_eq!(
            log(&tmp, 1, true, true, ""),
            "error.log: 4 lines\n... 2 earlier lines\n1x  Another warning"
        );
        assert_eq!(
            log(&tmp, 5, true, false, "missing"),
            "error.log: 4 lines, 2 matching 'missing'\n2x  Missing faith"
        );
        assert_eq!(
            log(&tmp, 10, false, false, "not present"),
            "error.log: 4 lines, 0 matching 'not present'"
        );
    }

    #[test]
    fn missing_empty_invalid() {
        let tmp = TempDir::new();
        assert_eq!(log(&tmp, 10, false, false, ""), "error.log does not exist yet.");
        fs::create_dir_all(tmp.join("logs")).unwrap();
        fs::write(tmp.join("logs/error.log"), "").unwrap();
        assert_eq!(log(&tmp, 10, false, false, ""), "error.log: 0 lines");
        fs::write(tmp.join("logs/error.log"), b"Invalid byte: \xff\n").unwrap();
        assert_eq!(log(&tmp, 10, false, false, ""), "error.log: 1 lines\nInvalid byte: \u{fffd}");
        let bad = read_log(
            &tmp,
            &ReadLog { name: "invalid", max_lines: 1, dedupe: false, tail: false, pattern: "" },
        );
        assert!(bad.unwrap_err().contains("name must be one of"));
        let bad = read_log(
            &tmp,
            &ReadLog { name: "error", max_lines: 1, dedupe: false, tail: false, pattern: "(" },
        );
        assert!(bad.unwrap_err().contains("not a valid regex"));
    }

    const WRITTEN: &str = r#"{"enabled_mods":["mod/xtiger-run-1-0.mod"],"disabled_dlcs":[]}"#;

    fn killed_run(original: &str) -> TempDir {
        let tmp = TempDir::new();
        fs::create_dir_all(tmp.join("mod")).unwrap();
        fs::write(tmp.join("dlc_load.json"), WRITTEN).unwrap();
        fs::write(tmp.join(DLC_BACKUP), format!("{WRITTEN}\n{original}")).unwrap();
        fs::write(tmp.join("mod/xtiger-run-1-0.mod"), "# xtiger-temp-mod\nname=\"x\"\n").unwrap();
        // A file of the user's with a similar name, and one that is ours by name but not by content.
        fs::write(tmp.join("mod/xtiger-run-mine.mod"), "name=\"mine\"\n").unwrap();
        fs::write(tmp.join("mod/other.mod"), "# xtiger-temp-mod\n").unwrap();
        tmp
    }

    #[test]
    fn a_killed_run_gets_the_mod_list_back() {
        let original = "{\"enabled_mods\":[\"mod/a.mod\"],\r\n\"disabled_dlcs\":[]}\r\n";
        let tmp = killed_run(original);
        assert!(recover_dlc_load(&tmp));
        assert_eq!(fs::read_to_string(tmp.join("dlc_load.json")).unwrap(), original);
        assert!(!tmp.join(DLC_BACKUP).exists());
        assert!(!tmp.join("mod/xtiger-run-1-0.mod").exists());
        assert!(tmp.join("mod/xtiger-run-mine.mod").exists());
        assert!(tmp.join("mod/other.mod").exists());
        // Nothing left to do the second time.
        assert!(!recover_dlc_load(&tmp));
    }

    #[test]
    fn without_an_original_the_file_is_removed() {
        let tmp = killed_run(NO_ORIGINAL);
        assert!(recover_dlc_load(&tmp));
        assert!(!tmp.join("dlc_load.json").exists());
    }

    #[test]
    fn a_list_changed_since_the_run_is_left_alone() {
        let tmp = killed_run("{}");
        fs::write(tmp.join("dlc_load.json"), "{\"enabled_mods\":[\"mod/theirs.mod\"]}").unwrap();
        assert!(!recover_dlc_load(&tmp));
        assert!(fs::read_to_string(tmp.join("dlc_load.json")).unwrap().contains("theirs"));
        assert!(!tmp.join(DLC_BACKUP).exists());
    }

    #[test]
    fn only_a_clean_exit_is_a_result() {
        assert_eq!(failure_lead(Some(0), true), None);
        assert_eq!(failure_lead(Some(0), false), None);
        assert!(failure_lead(None, false).unwrap().contains("did not finish"));
        assert!(failure_lead(Some(1), false).unwrap().contains("before it could test"));
        assert!(failure_lead(Some(1), true).unwrap().contains("failed"));
        // A mod the game did not mount (exit 2) is a failed run too.
        assert!(failure_lead(Some(2), true).unwrap().contains("not loaded"));
    }

    #[test]
    fn a_list_cut_short_by_the_kill_is_still_ours() {
        let original = "{\"enabled_mods\":[],\"disabled_dlcs\":[]}";
        let tmp = killed_run(original);
        fs::write(tmp.join("dlc_load.json"), &WRITTEN[..20]).unwrap();
        assert!(recover_dlc_load(&tmp));
        assert_eq!(fs::read_to_string(tmp.join("dlc_load.json")).unwrap(), original);
    }

    #[test]
    fn leftover_temp_mods_go_even_without_a_backup() {
        let tmp = killed_run("{}");
        fs::remove_file(tmp.join(DLC_BACKUP)).unwrap();
        assert!(!recover_dlc_load(&tmp));
        assert_eq!(fs::read_to_string(tmp.join("dlc_load.json")).unwrap(), WRITTEN);
        assert!(!tmp.join("mod/xtiger-run-1-0.mod").exists());
        assert!(tmp.join("mod/xtiger-run-mine.mod").exists());
    }

    #[test]
    fn the_run_lock_is_shared_and_stale_ones_are_taken_over() {
        let tmp = TempDir::new();
        let first = take_run_lock(&tmp, &|_| true).unwrap();
        // The owner is alive: a second server has to wait.
        let busy = take_run_lock(&tmp, &|_| true).err().unwrap();
        assert!(busy.contains("Another xTiger server"), "{busy}");
        assert!(busy.contains(&std::process::id().to_string()));
        // The owner is gone (it was killed): the lock is taken over.
        let second = take_run_lock(&tmp, &|_| false).unwrap();
        drop(first);
        drop(second);
        // Dropping the lock frees it.
        assert!(!tmp.join(LOCK_FILE).exists());
        assert!(take_run_lock(&tmp, &|_| true).is_ok());
        // A lock with no readable owner was only just created by someone else.
        fs::write(tmp.join(LOCK_FILE), "").unwrap();
        assert!(take_run_lock(&tmp, &|_| false).is_err());
    }
}
