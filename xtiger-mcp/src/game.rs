//! Driving a real CK3 session through the bundled PowerShell scripts (Windows only), and reading
//! the game's logs.

use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

fn powershell(
    loc: &Locations,
    script: &str,
    args: &[String],
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    if !cfg!(windows) {
        return Err("Game control needs Windows (it sends input through the Win32 SendInput API)."
            .to_owned());
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
            let _ = child.kill();
            let _ = child.wait();
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
    let timeout = Duration::from_secs(run.load_timeout + 60 + 10 * commands);
    let outcome = powershell(loc, "ck3-run.ps1", &args, timeout, cancelled)?;
    let mut text = status(&outcome);
    if let Ok(bytes) = fs::read(&report) {
        text.push_str("\n\n");
        text.push_str(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}'));
    }
    Ok((text, shot.filter(|path| path.is_file())))
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
}
