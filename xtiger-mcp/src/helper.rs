//! The resident input helper behind `ck3_keys`: one PowerShell kept alive between calls, so a key
//! press no longer pays for starting PowerShell and compiling the input code each time.
//!
//! The protocol is one line per request on its stdin and one line per reply on its stdout (see
//! `scripts/ck3-helper.ps1`). Everything is ASCII; text and messages travel as base64 UTF-8 so the
//! console code page never touches them.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::runs::no_window;

/// How long the helper may take to start: PowerShell plus compiling the input code.
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// How long one request may take. Typing is 30 ms a character, so this is generous.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// One thing to send to the game: scancodes first, then text, then a wait for the game to react.
/// With `wait_for`, the wait ends as soon as a line of the `log` written after the keys matches that
/// regular expression, and `wait_ms` is only the longest it may take.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    pub scancodes: String,
    pub text: String,
    pub wait_ms: u64,
    pub log: String,
    pub wait_for: String,
}

/// What the helper said about one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Done(String),
    Failed(String),
}

/// Why a request got no reply from the helper.
#[derive(Debug, PartialEq, Eq)]
pub enum Trouble {
    /// The helper could not be started, so the caller can fall back to the one-shot script.
    Unavailable(String),
    /// It started, but this request was cancelled, timed out or the helper died.
    Lost(String),
}

struct Helper {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
}

impl Helper {
    fn start(script: &Path, logs: &Path) -> Result<Self, String> {
        let mut child = no_window(
            Command::new("powershell")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(script)
                .arg("-Logs")
                .arg(logs),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start PowerShell: {e}"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("no pipes to the input helper".to_owned());
        };
        let (sender, replies) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let helper = Self { child, stdin, replies };
        let said = helper.replies.recv_timeout(START_TIMEOUT);
        if let Ok(line) = &said
            && decode_reply(line) == Some(Reply::Done("ready".to_owned()))
        {
            return Ok(helper);
        }
        helper.stop();
        match said {
            Ok(line) => Err(format!("the input helper said {line:?} instead of ready")),
            Err(_) => Err("the input helper did not start".to_owned()),
        }
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The helper of this server. Held for a whole request, so key presses never interleave.
static HELPER: Mutex<Option<Helper>> = Mutex::new(None);

/// The request line for one step. `shot` is where to save a screenshot afterwards, if anywhere, and
/// `view` which part of it to keep and how much to shrink it (`Shot::view`).
pub fn encode_request(step: &Step, shot: Option<&Path>, view: &str) -> String {
    let shot = shot.map_or_else(String::new, |path| STANDARD.encode(path.display().to_string()));
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        step.scancodes.trim(),
        STANDARD.encode(&step.text),
        shot,
        step.wait_ms,
        step.log,
        STANDARD.encode(&step.wait_for),
        view,
    )
}

/// `None` when the line is not a reply at all.
pub fn decode_reply(line: &str) -> Option<Reply> {
    let (kind, message) = line.trim().split_once('|')?;
    let message = String::from_utf8(STANDARD.decode(message).ok()?).ok()?;
    match kind {
        "OK" => Some(Reply::Done(message)),
        "ERR" => Some(Reply::Failed(message)),
        _ => None,
    }
}

/// Send one step to the helper, starting it if needed.
pub fn request(
    script: &Path,
    logs: &Path,
    step: &Step,
    shot: Option<&Path>,
    view: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<Reply, Trouble> {
    let mut slot = HELPER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.as_mut().is_some_and(|helper| !helper.alive()) {
        *slot = None;
    }
    if slot.is_none() {
        *slot = Some(Helper::start(script, logs).map_err(Trouble::Unavailable)?);
    }
    let Some(helper) = slot.as_mut() else {
        return Err(Trouble::Unavailable("the input helper is gone".to_owned()));
    };
    let line = encode_request(step, shot, view);
    // A broken pipe means the helper died after the check above.
    let sent = writeln!(helper.stdin, "{line}").and_then(|()| helper.stdin.flush());
    let lost = |slot: &mut Option<Helper>, why: &str| {
        if let Some(helper) = slot.take() {
            helper.stop();
        }
        Err(Trouble::Lost(why.to_owned()))
    };
    if let Err(e) = sent {
        return lost(&mut slot, &format!("the input helper stopped ({e})"));
    }
    let started = Instant::now();
    loop {
        match helper.replies.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => match decode_reply(&line) {
                Some(reply) => return Ok(reply),
                // Something stray on stdout; the reply is still to come.
                None => continue,
            },
            Err(RecvTimeoutError::Disconnected) => {
                return lost(&mut slot, "the input helper stopped");
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if cancelled() {
            return lost(&mut slot, "Cancelled.");
        }
        if started.elapsed() > REQUEST_TIMEOUT {
            return lost(
                &mut slot,
                &format!("Stopped after {} seconds.", REQUEST_TIMEOUT.as_secs()),
            );
        }
    }
}

/// The process id of the running helper, for tests that check it is reused.
#[cfg(test)]
pub fn running_pid() -> Option<u32> {
    HELPER.lock().ok()?.as_ref().map(|helper| helper.child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(scancodes: &str, text: &str, wait_ms: u64) -> Step {
        Step {
            scancodes: scancodes.to_owned(),
            text: text.to_owned(),
            wait_ms,
            log: "debug".to_owned(),
            wait_for: String::new(),
        }
    }

    #[test]
    fn request_lines_are_ascii_with_seven_fields() {
        let mut one = step(" 29, 1C ", "evento ñ 日本", 300);
        one.wait_for = "Running console command: (ñ|x)".to_owned();
        let line = encode_request(&one, None, "0,0,1,1,0.5");
        assert!(line.is_ascii());
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(fields.len(), 7, "base64 never contains the separator: {line}");
        assert_eq!(fields[4], "debug");
        assert_eq!(
            STANDARD.decode(fields[5]).unwrap(),
            "Running console command: (ñ|x)".as_bytes()
        );
        assert_eq!(fields[0], "29, 1C");
        assert_eq!(STANDARD.decode(fields[1]).unwrap(), "evento ñ 日本".as_bytes());
        assert_eq!(fields[2], "");
        assert_eq!(fields[3], "300");
        assert_eq!(fields[6], "0,0,1,1,0.5");
    }

    #[test]
    fn the_screenshot_path_travels_as_text() {
        let line = encode_request(&step("", "", 0), Some(Path::new(r"C:\x y\screen.png")), "");
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(STANDARD.decode(fields[2]).unwrap(), br"C:\x y\screen.png");
    }

    #[test]
    fn replies_decode_both_kinds_and_reject_garbage() {
        let ok = format!("OK|{}", STANDARD.encode("sent 1 keys"));
        let err = format!("ERR|{}", STANDARD.encode("no CK3 window ñ"));
        assert_eq!(decode_reply(&ok), Some(Reply::Done("sent 1 keys".to_owned())));
        assert_eq!(
            decode_reply(&format!("{err}\r\n")),
            Some(Reply::Failed("no CK3 window ñ".to_owned()))
        );
        assert_eq!(decode_reply("WARNING: something"), None);
        assert_eq!(decode_reply("OK|!!!not base64"), None);
        assert_eq!(decode_reply("MAYBE|aGk="), None);
    }
}
