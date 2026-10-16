//! Driving a real CK3 session through the bundled PowerShell scripts (Windows only), and reading
//! the game's logs.

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};

use crate::helper::{self, Reply, Step, Trouble};
use crate::locate::Locations;
use crate::runs::{most_common, no_window};

pub const LOGS: [&str; 6] = ["error", "game", "exceptions", "debug", "database_conflicts", "setup"];

const SCRIPTS: [(&str, &str); 4] = [
    ("ck3-input.ps1", include_str!("../scripts/ck3-input.ps1")),
    ("ck3-helper.ps1", include_str!("../scripts/ck3-helper.ps1")),
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

/// What a child printed so far, line by line.
type Lines = Arc<Mutex<Vec<String>>>;

/// Collect a child's output line by line on its own thread, so a full pipe never blocks the child
/// and what it has printed so far can be read while it runs.
fn follow(stream: Option<impl Read + Send + 'static>) -> (Lines, JoinHandle<()>) {
    let lines = Lines::default();
    let sink = Arc::clone(&lines);
    let reader = thread::spawn(move || {
        if let Some(stream) = stream {
            for line in BufReader::new(stream).split(b'\n') {
                let Ok(bytes) = line else { break };
                let text =
                    String::from_utf8_lossy(&bytes).trim_end_matches(['\r', '\n']).to_owned();
                if let Ok(mut lines) = sink.lock() {
                    lines.push(text);
                }
            }
        }
    });
    (lines, reader)
}

/// Hand the lines that came after the first `done` to `progress`.
fn forward(lines: &Lines, done: &mut usize, progress: &mut dyn FnMut(&str)) {
    let new: Vec<String> =
        lines.lock().map(|lines| lines[(*done).min(lines.len())..].to_vec()).unwrap_or_default();
    for line in &new {
        progress(line);
    }
    *done += new.len();
}

fn joined(lines: &Lines) -> String {
    lines.lock().map(|lines| lines.join("\n")).unwrap_or_default()
}

/// How a script is watched while it runs.
pub struct Control<'a> {
    pub cancelled: &'a dyn Fn() -> bool,
    /// Gets each line the script prints, as it prints it.
    pub progress: &'a mut dyn FnMut(&str),
    /// Called a few times a second for as long as the script runs.
    pub tick: &'a mut dyn FnMut(),
}

impl std::fmt::Debug for Control<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control").finish_non_exhaustive()
    }
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
static BUSY: AtomicBool = AtomicBool::new(false);

struct InThisServer;

impl InThisServer {
    fn take() -> Option<Self> {
        BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
            .then_some(Self)
    }
}

impl Drop for InThisServer {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

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
                        "Another xTiger server{who} is running a game right now; wait for it to \
                         finish. If no run is going, delete {}.",
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
/// Exit code 2 means a mod was not mounted; 3 means a requested command was not confirmed.
fn failure_lead(code: Option<i32>, report: bool) -> Option<&'static str> {
    match (code, report) {
        (Some(0), true) => None,
        (None, _) => Some("The run did not finish."),
        (Some(_), false) => Some("The run stopped before it could test anything."),
        (Some(2), true) => Some("A mod was not loaded by the game, so the run did not test it."),
        (Some(3), true) => Some("The game did not confirm every requested console command."),
        (Some(_), true) => Some("The run failed."),
    }
}

pub(crate) const WINDOWS_ONLY: &str =
    "Game control needs Windows (it sends input through the Win32 SendInput API).";

fn powershell(
    loc: &Locations,
    script: &str,
    args: &[String],
    timeout: Duration,
    control: &mut Control,
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
    let (out, reader) = follow(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let mut forwarded = 0;
    let code = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.code();
        }
        forward(&out, &mut forwarded, control.progress);
        (control.tick)();
        let was_cancelled = (control.cancelled)();
        if was_cancelled || started.elapsed() > timeout {
            kill_tree(&mut child);
            let stopped = if was_cancelled {
                "Cancelled.".to_owned()
            } else {
                format!("Stopped after {} seconds.", timeout.as_secs())
            };
            let _ = reader.join();
            forward(&out, &mut forwarded, control.progress);
            return Ok(Outcome {
                code: None,
                stdout: format!("{}\n{stopped}", joined(&out).trim()),
                stderr: String::new(),
            });
        }
        thread::sleep(Duration::from_millis(200));
    };
    let _ = reader.join();
    forward(&out, &mut forwarded, control.progress);
    Ok(Outcome { code, stdout: joined(&out), stderr: err.join().unwrap_or_default() })
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
    /// Run without the user's screen: nothing typed, no screenshot, the game minimized.
    pub background: bool,
}

/// Seconds a background run waits after the game state is ready, for the mod's `on_action`s to run.
const BACKGROUND_SETTLE_SECS: u64 = 20;

const MAX_CONSOLE_UNITS: usize = 255;

fn validate_commands(commands: &[String]) -> Result<(), String> {
    for (index, command) in commands.iter().enumerate() {
        let number = index + 1;
        if command.trim().is_empty() || command.chars().any(char::is_control) {
            return Err(format!(
                "Console command {number} must be one non-empty line without control characters."
            ));
        }
        let length = command.encode_utf16().count();
        if length > MAX_CONSOLE_UNITS {
            return Err(format!(
                "Console command {number} has {length} UTF-16 code units; CK3 accepts at most {MAX_CONSOLE_UNITS}. Split it into shorter commands."
            ));
        }
    }
    Ok(())
}

/// The title to play is handed to PowerShell and to the game's command line as it is, so it must be
/// a plain key: a value such as `-Shot:C:\x` or `k_france -other` would set other parameters.
fn validate_play(play: &str) -> Result<(), String> {
    if !play.is_empty() && play.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(format!("play must be a title key such as k_france or c_jaffa, not {play:?}."))
    }
}

fn validate_bookmark(game: &Path, mods: &[PathBuf], bookmark: &str) -> Result<(), String> {
    let relative = "common/bookmarks/bookmarks";
    let exists = crate::overrides::has_definition(&game.join("game").join(relative), bookmark)
        || mods
            .iter()
            .filter_map(|file| crate::mods::describe(file))
            .any(|info| crate::overrides::has_definition(&info.dir.join(relative), bookmark));
    if exists {
        Ok(())
    } else {
        Err(format!(
            "Bookmark {bookmark:?} was not found in CK3 or the requested mods. Use a bookmark key such as bm_867_carolingians, rather than a date."
        ))
    }
}

/// The two locks that keep game runs apart: one for this server, one for all of them.
struct Locks {
    _in_this_server: InThisServer,
    _across_servers: RunLock,
}

/// A game run that has its locks and its work folder, ready to execute: on this thread, or on one
/// of its own (see `jobs`).
pub struct Started {
    args: Vec<String>,
    report: PathBuf,
    shot: Option<PathBuf>,
    timeout: Duration,
    /// How many console commands the run was asked to send, and whether it types nothing.
    commands: usize,
    background: bool,
    _locks: Locks,
}

impl std::fmt::Debug for Started {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Started").field("report", &self.report).finish_non_exhaustive()
    }
}

/// Check a run and take the locks for it. Everything that can be refused is refused here, before
/// any game starts.
pub fn begin(loc: &Locations, run: &RunGame) -> Result<Started, String> {
    let game = loc.require_game()?;
    validate_commands(run.commands)?;
    if run.background && !run.commands.is_empty() {
        return Err("A background run types nothing, so it cannot send console commands. \
                    Let the mod trigger its events from an on_action. To type commands, the user has to \
                    switch Game tests to \"Take over the screen\" in the xTiger app's settings."
            .to_owned());
    }
    validate_play(run.play)?;
    validate_bookmark(game, run.mods, run.bookmark)?;
    if !["unicode", "scancode"].contains(&run.typing) {
        return Err("typing must be unicode or scancode".to_owned());
    }
    // Before the locks and the mod list recovery, which use Windows tools.
    if !cfg!(windows) {
        return Err(WINDOWS_ONLY.to_owned());
    }
    let in_this_server = InThisServer::take().ok_or(
        "Another game run is still going in this server (a ck3_run, or a job: ck3_job lists them); \
         wait for it to finish or cancel it.",
    )?;
    let across_servers = take_run_lock(&loc.user_dir, &pid_alive)?;
    let work = work_dir(loc, "run")?;
    let shot = (run.screenshot && !run.background).then(|| work.join("screen.png"));
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
    let mut settle = 0;
    if run.background {
        settle = BACKGROUND_SETTLE_SECS;
        args.extend(["-Background".to_owned(), "-Settle".to_owned(), settle.to_string()]);
    }
    let commands = u64::try_from(run.commands.len()).unwrap_or(u64::MAX);
    let timeout = Duration::from_secs(run.load_timeout + 60 + settle + 30 * commands);
    Ok(Started {
        args,
        report,
        shot,
        timeout,
        commands: run.commands.len(),
        background: run.background,
        _locks: Locks { _in_this_server: in_this_server, _across_servers: across_servers },
    })
}

/// The most lines of the report that are handed back.
const MAX_REPORT_LINES: usize = 1500;

/// The report as it is handed back: cut at [`MAX_REPORT_LINES`], saying so and where the whole file is.
fn cap_report(text: &str, file: &Path) -> String {
    let total = text.lines().count();
    if total <= MAX_REPORT_LINES {
        return text.to_owned();
    }
    let mut kept: Vec<&str> = text.lines().take(MAX_REPORT_LINES).collect();
    let note = format!(
        "... report cut: {MAX_REPORT_LINES} of {total} lines shown. The whole report is in {}",
        file.display()
    );
    kept.push(&note);
    kept.join("\n")
}

/// What the run itself checked, apart from what its commands did. A command the game logged only
/// shows that the game accepted it; whether the mod behaved is for the logs to say.
fn confirmation(report: &str, commands: usize, background: bool) -> String {
    let find = |prefix: &str| {
        report.lines().map(str::trim).find(|line| line.starts_with(prefix)).map(str::to_owned)
    };
    let mut out = String::from("=== what this run confirms ===");
    for (label, prefix) in [("boot", "loaded after"), ("mods", "mods loaded:")] {
        if let Some(line) = find(prefix) {
            let _ = write!(out, "\n{label}: {line}");
        }
    }
    if commands > 0
        && let Some(line) = find("commands sent:")
    {
        let _ = write!(out, "\ncommands: {line}");
    }
    out.push_str(if background {
        "\nnot confirmed: whether the mod behaved as intended. Nothing was typed; what the mod \
         tests has to come from its own on_actions and be read in the logs below or with ck3_logs."
    } else {
        "\nnot confirmed: whether the mod behaved as intended. A command the game ran was only \
         accepted; its effect shows in the logs below or with ck3_logs."
    });
    out
}

impl Started {
    /// Where the full report will be written.
    pub fn report(&self) -> &Path {
        &self.report
    }

    /// The most the run is given before it is stopped.
    pub fn budget(&self) -> Duration {
        self.timeout
    }

    /// Start CK3, load a game, type the commands, and report. Returns the report and the
    /// screenshot, if one was taken.
    pub fn execute(
        self,
        loc: &Locations,
        control: &mut Control,
    ) -> Result<(String, Option<PathBuf>), String> {
        // A run that was killed earlier may have left the mod list changed; the script would take
        // that list for the user's own. Not while a game is running: the list is its own then, and
        // the script refuses to start anyway.
        if !ck3_running() {
            recover_dlc_load(&loc.user_dir);
        }
        let mut outcome = powershell(loc, "ck3-run.ps1", &self.args, self.timeout, control)?;
        if outcome.code.is_none() {
            // The script was stopped together with the game, so it could not put the list back itself.
            outcome.stdout.push_str("\nThe script and the game it started were stopped.");
            if recover_dlc_load(&loc.user_dir) {
                outcome.stdout.push_str(" Your mod list (dlc_load.json) was put back.");
            }
        }
        let mut text = status(&outcome);
        let report_bytes = fs::read(&self.report).ok();
        if let Some(bytes) = &report_bytes {
            let report = String::from_utf8_lossy(bytes);
            let report = report.trim_start_matches('\u{feff}');
            let sections = [
                confirmation(report, self.commands, self.background),
                cap_report(report.trim(), &self.report),
            ];
            for section in sections {
                text.push_str("\n\n");
                text.push_str(&section);
            }
        }
        let shot = self.shot.filter(|path| path.is_file());
        // Nothing was tested when the script failed, was stopped, or a mod was not loaded: that is
        // an error for the caller, not a result, but what the run did find stays in the message.
        if let Some(lead) = failure_lead(outcome.code, report_bytes.is_some()) {
            if let Some(shot) = &shot {
                let _ = write!(text, "\nScreenshot: {}", shot.display());
            }
            // The lead is a line of its own: the journal and the app show the first line.
            return Err(format!("{lead}\n{text}"));
        }
        Ok((text, shot))
    }
}

/// Start CK3, load a game, type the commands, and report, all before returning.
pub fn run_game(
    loc: &Locations,
    run: &RunGame,
    control: &mut Control,
) -> Result<(String, Option<PathBuf>), String> {
    begin(loc, run)?.execute(loc, control)
}

/// Whether to take a screenshot of the game window after an action, and how much of it to keep. A
/// smaller picture costs the reader fewer tokens: `region` keeps part of the window (as fractions of its
/// width and height) and `scale` shrinks what is kept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub take: bool,
    /// `[left, top, right, bottom]`, each from 0 to 1.
    pub region: Option<[f64; 4]>,
    /// From 0.1 to 1; 1 keeps the pixels as they are.
    pub scale: f64,
}

impl Shot {
    pub const OFF: Self = Self { take: false, region: None, scale: 1.0 };

    pub const fn full(take: bool) -> Self {
        Self { take, region: None, scale: 1.0 }
    }

    /// The same view, but only taken when `last` (the screenshot goes after the last of several runs).
    #[must_use]
    pub const fn when(self, last: bool) -> Self {
        Self { take: self.take && last, ..self }
    }

    /// What the input helper is told: `x0,y0,x1,y1,scale`, or nothing for the whole window.
    pub fn view(&self) -> String {
        if self.region.is_none() && (self.scale - 1.0).abs() < f64::EPSILON {
            return String::new();
        }
        let [left, top, right, bottom] = self.region.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        format!("{left:.4},{top:.4},{right:.4},{bottom:.4},{:.3}", self.scale)
    }

    /// The `shot_region` and `shot_scale` arguments of a tool call; `take` is `screenshot`.
    pub fn from_arguments(
        take: bool,
        region: Option<&[f64]>,
        scale: Option<f64>,
    ) -> Result<Self, String> {
        let region = match region {
            None => None,
            Some([left, top, right, bottom]) => {
                let all = [*left, *top, *right, *bottom];
                if all.iter().any(|n| !(0.0..=1.0).contains(n)) || left >= right || top >= bottom {
                    return Err("shot_region is [left, top, right, bottom] as fractions of the window, from 0 to 1, with left < right and top < bottom.".to_owned());
                }
                Some(all)
            }
            Some(_) => {
                return Err(
                    "shot_region needs four numbers: [left, top, right, bottom].".to_owned()
                );
            }
        };
        let scale = scale.unwrap_or(1.0);
        if !(0.1..=1.0).contains(&scale) {
            return Err("shot_scale must be between 0.1 and 1.".to_owned());
        }
        Ok(Self { take, region, scale })
    }
}

/// Send steps of keys and text to the open CK3 window, in order. The resident input helper does it
/// in a fraction of a second per step; if it cannot start, each step goes through `ck3-keys.ps1`
/// instead, which takes seconds. A screenshot is taken after the last step.
pub fn send_keys(
    loc: &Locations,
    steps: &[Step],
    screenshot: Shot,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    if !cfg!(windows) {
        return Err(WINDOWS_ONLY.to_owned());
    }
    let scripts = scripts_dir(loc)?;
    let shot = if screenshot.take { Some(work_dir(loc, "keys")?.join("screen.png")) } else { None };
    let view = screenshot.view();
    let mut lines: Vec<String> = Vec::new();
    let mut resident = true;
    for (index, step) in steps.iter().enumerate() {
        let want = if index + 1 == steps.len() { shot.as_deref() } else { None };
        let label = if steps.len() > 1 { format!("step {}: ", index + 1) } else { String::new() };
        let failed = |lines: &[String], message: &str| {
            let mut text = if steps.len() > 1 {
                format!("Step {} of {} failed: {message}", index + 1, steps.len())
            } else {
                message.to_owned()
            };
            if !lines.is_empty() {
                let _ = write!(text, "\nAlready sent:\n{}", lines.join("\n"));
            }
            // A step that waited for a log line still took its screenshot, which shows what happened.
            if let Some(path) = want.filter(|path| path.is_file()) {
                let _ = write!(text, "\nScreenshot: {}", path.display());
            }
            text
        };
        let reply = if resident {
            match helper::request(
                &scripts.join("ck3-helper.ps1"),
                &loc.user_dir.join("logs"),
                step,
                want,
                &view,
                cancelled,
            ) {
                Ok(reply) => Some(reply),
                Err(Trouble::Lost(why)) => return Err(failed(&lines, &why)),
                Err(Trouble::Unavailable(why)) => {
                    lines.push(format!("(input helper unavailable: {why}; using the slow script)"));
                    resident = false;
                    None
                }
            }
        } else {
            None
        };
        let message = match reply {
            Some(Reply::Done(message)) => message,
            Some(Reply::Failed(message)) => return Err(failed(&lines, &message)),
            None => send_with_script(loc, step, want, &view, cancelled)
                .map_err(|message| failed(&lines, &message))?,
        };
        lines.push(format!("{label}{message}"));
    }
    Ok((lines.join("\n"), shot.filter(|path| path.is_file())))
}

/// The slow way: a new PowerShell for one step. Its waits are fixed, so `wait_ms` does not apply.
fn send_with_script(
    loc: &Locations,
    step: &Step,
    shot: Option<&Path>,
    view: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, String> {
    if step.scancodes.trim().starts_with("?click") {
        return Err("A click needs the resident input helper, which could not start.".to_owned());
    }
    let mut args = Vec::new();
    if !step.scancodes.trim().is_empty() {
        args.extend(["-Sc".to_owned(), step.scancodes.trim().to_owned()]);
    }
    if !step.text.is_empty() {
        args.extend(["-Text".to_owned(), step.text.clone()]);
    }
    if let Some(shot) = shot {
        args.extend([
            "-Shot".to_owned(),
            shot.display().to_string(),
            "-View".to_owned(),
            view.to_owned(),
        ]);
    }
    let mut control = Control { cancelled, progress: &mut |_| {}, tick: &mut || {} };
    let outcome = powershell(loc, "ck3-keys.ps1", &args, Duration::from_secs(120), &mut control)?;
    let mut text = status(&outcome);
    if outcome.code != Some(0) {
        return Err(text);
    }
    if !step.wait_for.is_empty() {
        text.push_str("\n(wait_for was not checked: the slow script cannot watch a log)");
    }
    Ok(text)
}

/// Where to start reading a log, so a second read returns only what the game wrote since the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Since {
    /// Read nothing, only report the offset to resume from: a marker to set before an action.
    End,
    /// Read from this byte offset, as an earlier read reported it.
    Offset(u64),
}

#[derive(Debug)]
pub struct ReadLog<'a> {
    pub name: &'a str,
    pub max_lines: usize,
    pub dedupe: bool,
    pub tail: bool,
    pub pattern: &'a str,
    pub since: Option<Since>,
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
        let offset = if read.since.is_some() { "; offset 0" } else { "" };
        return Ok(format!("{file_name} does not exist yet{offset}."));
    };
    // With `since`, only the part written after that offset is read. A line the game has not finished
    // writing is left for the next read, so the offset reported always sits on a line start.
    let mut resume = None;
    let mut part: &[u8] = &bytes;
    let mut restarted = false;
    if let Some(since) = read.since {
        let line_end =
            |bytes: &[u8]| bytes.iter().rposition(|b| *b == b'\n').map_or(0, |at| at + 1);
        let from = match since {
            // The start of the unfinished last line, so that line is read whole next time.
            Since::End => line_end(&bytes),
            Since::Offset(at) => {
                usize::try_from(at).ok().filter(|at| *at <= bytes.len()).unwrap_or_else(|| {
                    restarted = true;
                    0
                })
            }
        };
        let fresh = &bytes[from..];
        let whole = line_end(fresh);
        part = if since == Since::End { &[] } else { &fresh[..whole] };
        resume = Some(from + whole);
    }
    let text = String::from_utf8_lossy(part);
    let all: Vec<&str> = text.lines().collect();
    let total = all.len();
    let mut lines: Vec<String> = match &pattern {
        Some(regex) => {
            all.iter().filter(|line| regex.is_match(line)).map(|line| (*line).to_owned()).collect()
        }
        None => all.iter().map(|line| (*line).to_owned()).collect(),
    };
    let mut head = format!("{file_name}: {total} lines");
    if read.since.is_some() {
        head = format!("{file_name}: {total} new lines");
    }
    if pattern.is_some() {
        let _ = write!(head, ", {} matching '{}'", lines.len(), read.pattern);
    }
    if let Some(offset) = resume {
        let _ = write!(head, "; offset {offset}");
        if restarted {
            head.push_str(
                " (the log is shorter than the offset given, so it was read from the start)",
            );
        }
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
        read_log(dir, &ReadLog { name: "error", max_lines, dedupe, tail, pattern, since: None })
            .unwrap()
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

    fn since(dir: &Path, since: Since, pattern: &str) -> String {
        let read = ReadLog {
            name: "error",
            max_lines: 50,
            dedupe: false,
            tail: false,
            pattern,
            since: Some(since),
        };
        read_log(dir, &read).unwrap()
    }

    #[test]
    fn since_returns_only_what_the_game_wrote_after() {
        use std::io::Write as _;
        let tmp = setup();
        let path = tmp.join("logs/error.log");
        let old = fs::metadata(&path).unwrap().len();
        // The marker reads nothing and says where the log ends.
        assert_eq!(since(&tmp, Since::End, ""), format!("error.log: 0 new lines; offset {old}"));
        let whole = "[12:00:04.005] New one\n";
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(format!("{whole}[12:00:05.006] Half").as_bytes()).unwrap();
        // A line still being written waits for the next read, so no fragment comes back.
        let next = old + whole.len() as u64;
        assert_eq!(
            since(&tmp, Since::Offset(old), ""),
            format!("error.log: 1 new lines; offset {next}\n[12:00:04.005] New one")
        );
        assert_eq!(
            since(&tmp, Since::Offset(old), "nothing like this"),
            format!("error.log: 1 new lines, 0 matching 'nothing like this'; offset {next}")
        );
        file.write_all(b" done\n[12:00:06.007] Last\n").unwrap();
        let end = fs::metadata(&path).unwrap().len();
        assert_eq!(
            since(&tmp, Since::Offset(next), ""),
            format!(
                "error.log: 2 new lines; offset {end}\n[12:00:05.006] Half done\n[12:00:06.007] Last"
            )
        );
        assert_eq!(
            since(&tmp, Since::Offset(end), ""),
            format!("error.log: 0 new lines; offset {end}")
        );
        // A log the game started again is shorter than the offset: read it from the start.
        let again = since(&tmp, Since::Offset(end + 1000), "");
        assert!(
            again.contains("read from the start") && again.contains("Missing faith"),
            "{again}"
        );
        // An unfinished line at the marker is read whole once it is finished.
        file.write_all(b"[12:00:07.008] Part").unwrap();
        let marker = since(&tmp, Since::End, "");
        assert_eq!(marker, format!("error.log: 0 new lines; offset {end}"));
    }

    #[test]
    fn since_on_a_missing_log_starts_at_zero() {
        let tmp = TempDir::new();
        assert_eq!(since(&tmp, Since::End, ""), "error.log does not exist yet; offset 0.");
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
            &ReadLog {
                name: "invalid",
                max_lines: 1,
                dedupe: false,
                tail: false,
                pattern: "",
                since: None,
            },
        );
        assert!(bad.unwrap_err().contains("name must be one of"));
        let bad = read_log(
            &tmp,
            &ReadLog {
                name: "error",
                max_lines: 1,
                dedupe: false,
                tail: false,
                pattern: "(",
                since: None,
            },
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
        assert!(failure_lead(Some(0), false).unwrap().contains("before it could test"));
        assert!(failure_lead(None, false).unwrap().contains("did not finish"));
        assert!(failure_lead(Some(1), false).unwrap().contains("before it could test"));
        assert!(failure_lead(Some(1), true).unwrap().contains("failed"));
        // A mod the game did not mount (exit 2) is a failed run too.
        assert!(failure_lead(Some(2), true).unwrap().contains("not loaded"));
        assert!(failure_lead(Some(3), true).unwrap().contains("every requested"));
    }

    #[test]
    fn console_preflight_prevents_truncation_and_command_splitting() {
        assert!(validate_commands(&[]).is_ok());
        assert!(validate_commands(&["a".repeat(255)]).is_ok());
        assert!(validate_commands(&["a".repeat(256)]).unwrap_err().contains("256"));
        assert!(validate_commands(&["\u{1f600}".repeat(128)]).unwrap_err().contains("256"));
        for command in ["", "  ", "gold 1\ngold 2", "gold\r1", "gold\t1", "gold\0"] {
            assert!(validate_commands(&[command.to_owned()]).is_err(), "{command:?}");
        }
        assert!(
            validate_commands(&["gold 1".to_owned(), "a".repeat(256)])
                .unwrap_err()
                .contains("command 2")
        );
    }

    #[test]
    fn play_preflight_accepts_only_a_plain_title_key() {
        for play in ["k_france", "c_jaffa", "e_hre", "d_A1"] {
            assert!(validate_play(play).is_ok(), "{play:?}");
        }
        for play in ["", "-Keep", "-Shot:C:\\x.png", "k_france -mod=x", "k_france\"", "k;france"] {
            assert!(validate_play(play).is_err(), "{play:?}");
        }
    }

    #[test]
    fn bookmark_preflight_checks_exact_keys_in_game_and_mods() {
        let tmp = TempDir::new();
        let game = tmp.join("install");
        let bookmarks = game.join("game/common/bookmarks/bookmarks");
        fs::create_dir_all(&bookmarks).unwrap();
        fs::write(
            bookmarks.join("base.txt"),
            concat!(
                "\u{feff}# bm_comment = {}\n",
                "bm_real = { nested = { } text = \"bm_string = { } # \\\"\" } bm_compact = {}\n",
                "\"bm_quoted\" = {}\n",
                "bm_multiline\n = # bm_inline_comment = {}\n { }\n",
            ),
        )
        .unwrap();
        for bookmark in ["bm_real", "bm_compact", "bm_quoted", "bm_multiline"] {
            assert!(validate_bookmark(&game, &[], bookmark).is_ok(), "{bookmark}");
        }
        for missing in
            ["867.1.1", "bm_comment", "bm_string", "bm_inline_comment", "nested", "bm_re"]
        {
            assert!(validate_bookmark(&game, &[], missing).is_err());
        }
        let mod_dir = tmp.join("modded");
        fs::create_dir_all(mod_dir.join("common/bookmarks/bookmarks")).unwrap();
        fs::write(mod_dir.join("descriptor.mod"), "name=\"Bookmark test\"\n").unwrap();
        fs::write(mod_dir.join("common/bookmarks/bookmarks/mod.txt"), "bm_mod = {}\n").unwrap();
        assert!(validate_bookmark(&game, &[mod_dir.join("descriptor.mod")], "bm_mod").is_ok());
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

    #[test]
    fn a_long_report_is_cut_and_says_where_the_whole_one_is() {
        let file = Path::new("C:/state/report.txt");
        let short = "one\ntwo";
        assert_eq!(cap_report(short, file), short);
        let long = (0..MAX_REPORT_LINES + 40).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "line {n}");
            text
        });
        let cut = cap_report(&long, file);
        assert_eq!(cut.lines().count(), MAX_REPORT_LINES + 1);
        assert!(cut.contains(&format!("line {}", MAX_REPORT_LINES - 1)));
        assert!(!cut.contains(&format!("line {MAX_REPORT_LINES}\n")));
        let note = cut.lines().last().unwrap();
        assert!(
            note.contains(&format!("{MAX_REPORT_LINES} of {}", MAX_REPORT_LINES + 40)),
            "{note}"
        );
        assert!(note.contains("report.txt"), "{note}");
    }

    #[test]
    fn the_confirmation_keeps_what_was_checked_apart_from_how_the_mod_behaved() {
        let report =
            "loaded after 41 s\n  mods loaded: my_mod, other\ncommands sent: 2 of 2\nnoise\n";
        let typed = confirmation(report, 2, false);
        assert!(typed.starts_with("=== what this run confirms ==="), "{typed}");
        for line in [
            "boot: loaded after 41 s",
            "mods: mods loaded: my_mod, other",
            "commands: commands sent: 2 of 2",
        ] {
            assert!(typed.contains(line), "{typed}");
        }
        assert!(typed.contains("not confirmed: whether the mod behaved"), "{typed}");
        assert!(!typed.contains("noise"));
        // A background run types nothing, so it has no commands line to show.
        let quiet = confirmation(report, 0, true);
        assert!(!quiet.contains("commands:"), "{quiet}");
        assert!(quiet.contains("Nothing was typed"), "{quiet}");
    }

    #[test]
    fn output_is_followed_line_by_line_and_forwarded_once() {
        let data: &[u8] = b"first\r\nsecond\nlast without a newline";
        let (lines, reader) = follow(Some(data));
        reader.join().unwrap();
        assert_eq!(joined(&lines), "first\nsecond\nlast without a newline");
        let mut seen = Vec::new();
        let mut done = 0;
        forward(&lines, &mut done, &mut |line| seen.push(line.to_owned()));
        forward(&lines, &mut done, &mut |line| seen.push(line.to_owned()));
        assert_eq!(seen, ["first", "second", "last without a newline"]);
        assert_eq!(done, 3);
        let (nothing, reader) = follow(None::<&[u8]>);
        reader.join().unwrap();
        assert_eq!(joined(&nothing), "");
    }

    #[test]
    fn only_one_run_at_a_time_in_this_server_and_the_lock_moves_between_threads() {
        let first = InThisServer::take().expect("free");
        assert!(InThisServer::take().is_none());
        // A job holds its lock on a thread of its own.
        let handle = thread::spawn(move || drop(first));
        handle.join().unwrap();
        let again = InThisServer::take();
        assert!(again.is_some());
        drop(again);
    }

    #[cfg(windows)]
    fn locations_in(tmp: &TempDir) -> Locations {
        use crate::locate::{AppSettings, Sources};
        let mut env = std::collections::HashMap::new();
        env.insert("CK3_USER_DIR".to_owned(), tmp.join("user").display().to_string());
        env.insert("XTIGER_STATE_DIR".to_owned(), tmp.join("state").display().to_string());
        Locations::from_sources(&Sources {
            env,
            exe_dir: None,
            app_data: None,
            app_settings: AppSettings::default(),
            installed_app: None,
            home: None,
            user_dir_candidates: vec![],
            steam: Box::new(|| None),
            on_path: Box::new(|_| None),
        })
    }

    #[cfg(windows)]
    #[test]
    fn a_script_is_followed_while_it_runs_and_can_be_cancelled() {
        let tmp = TempDir::new();
        let loc = locations_in(&tmp);
        let dir = scripts_dir(&loc).unwrap();
        fs::write(
            dir.join("test-lines.ps1"),
            "Write-Output 'one'; Start-Sleep -Milliseconds 400; Write-Output 'two'; Start-Sleep -Milliseconds 400; Write-Output 'three'",
        )
        .unwrap();
        fs::write(dir.join("test-wait.ps1"), "Write-Output 'waiting'; Start-Sleep -Seconds 60")
            .unwrap();
        let mut seen = Vec::new();
        let mut ticks = 0;
        let outcome = {
            let mut progress = |line: &str| seen.push(line.to_owned());
            let mut tick = || ticks += 1;
            let mut control =
                Control { cancelled: &|| false, progress: &mut progress, tick: &mut tick };
            powershell(&loc, "test-lines.ps1", &[], Duration::from_secs(60), &mut control).unwrap()
        };
        assert_eq!(outcome.code, Some(0));
        assert_eq!(seen, ["one", "two", "three"]);
        assert!(ticks >= 1);
        // Cancelled a moment after the script began: it is stopped, and what it said stays.
        let begun = Instant::now();
        let flag = || begun.elapsed() > Duration::from_secs(2);
        let mut control = Control { cancelled: &flag, progress: &mut |_| {}, tick: &mut || {} };
        let outcome =
            powershell(&loc, "test-wait.ps1", &[], Duration::from_secs(60), &mut control).unwrap();
        assert_eq!(outcome.code, None);
        assert!(outcome.stdout.contains("waiting") && outcome.stdout.contains("Cancelled."));
        assert!(begun.elapsed() < Duration::from_secs(15));
    }

    #[cfg(windows)]
    #[test]
    fn keys_without_a_game_window_fail_fast_and_reuse_the_helper() {
        if ck3_running() {
            return;
        }
        let tmp = TempDir::new();
        let loc = locations_in(&tmp);
        let steps = [
            Step { scancodes: "29".to_owned(), ..Step::default() },
            Step { text: "hello".to_owned(), ..Step::default() },
        ];
        // The first call pays for starting the helper; the answer is its own message.
        let err = send_keys(&loc, &steps, Shot::OFF, &|| false).unwrap_err();
        assert!(err.contains("no CK3 window"), "{err}");
        assert!(err.contains("Step 1 of 2"), "{err}");
        let pid = helper::running_pid().expect("the helper stays up after an error reply");
        // The second one goes to the same process and is quick.
        let again = Instant::now();
        let err = send_keys(&loc, &steps[..1], Shot::OFF, &|| false).unwrap_err();
        assert!(err.contains("no CK3 window") && !err.contains("Step 1"), "{err}");
        assert_eq!(helper::running_pid(), Some(pid));
        assert!(again.elapsed() < Duration::from_secs(5), "{:?}", again.elapsed());
    }

    #[test]
    fn a_screenshot_view_is_a_region_and_a_scale() {
        assert_eq!(Shot::full(true).view(), "");
        assert!(!Shot::OFF.when(true).take);
        assert!(Shot::full(true).when(true).take && !Shot::full(true).when(false).take);
        let shot = Shot::from_arguments(true, Some(&[0.2, 0.25, 0.8, 0.75]), Some(0.5)).unwrap();
        assert_eq!(shot.view(), "0.2000,0.2500,0.8000,0.7500,0.500");
        let only_scale = Shot::from_arguments(true, None, Some(0.4)).unwrap();
        assert_eq!(only_scale.view(), "0.0000,0.0000,1.0000,1.0000,0.400");
        assert_eq!(only_scale.when(false).view(), only_scale.view());
    }

    #[test]
    fn a_bad_screenshot_view_is_refused() {
        for region in [
            vec![0.5, 0.0, 0.2, 1.0],
            vec![0.0, 0.6, 1.0, 0.6],
            vec![0.0, 0.0, 1.5, 1.0],
            vec![-0.1, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0],
        ] {
            assert!(Shot::from_arguments(true, Some(&region), None).is_err(), "{region:?}");
        }
        for scale in [0.0, 0.05, 1.01, f64::NAN] {
            assert!(Shot::from_arguments(true, None, Some(scale)).is_err(), "{scale}");
        }
    }
}
