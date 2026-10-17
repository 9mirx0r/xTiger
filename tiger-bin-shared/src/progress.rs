//! What the validator says while it works: a header with the game and the mod, and one line per
//! step with a spinner and the time it took. Everything here goes to stderr and is only meant for
//! a person at a terminal; when stderr is redirected the steps are plain lines.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use console::{Term, style};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_ASCII: [&str; 4] = ["|", "/", "-", "\\"];

/// `XTIGER_FORCE_TTY=1` gives the terminal experience when the output is captured, which is how
/// the pictures in the documentation are made. Steps are then printed without a spinner.
pub fn forced() -> bool {
    std::env::var_os("XTIGER_FORCE_TTY").is_some_and(|v| v == "1")
}

/// Whether the terminal is known to draw characters beyond ASCII properly.
pub fn fancy_glyphs() -> bool {
    forced() || Term::stderr().features().wants_emoji()
}

/// The glyph in front of a finished step.
pub fn tick() -> &'static str {
    if fancy_glyphs() { "✓" } else { "ok" }
}

/// Print the facts about this run in a short aligned block.
pub fn header(rows: &[(&str, String)]) {
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    for (label, value) in rows {
        eprintln!("  {}  {value}", style(format!("{label:<width$}")).dim());
    }
    eprintln!();
}

/// One step of the run, shown with a spinner until [`Stage::finish`].
pub struct Stage {
    label: String,
    started: Instant,
    spinner: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

impl Stage {
    pub fn start(label: &str) -> Stage {
        let started = Instant::now();
        let term = Term::stderr();
        let spinner = if term.is_term() {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let text = label.to_string();
            let frames: &[&str] = if fancy_glyphs() { &SPINNER } else { &SPINNER_ASCII };
            let handle = thread::spawn(move || {
                let mut i = 0;
                while !flag.load(Ordering::Relaxed) {
                    let line = format!(
                        "{} {text}  {}",
                        style(frames[i % frames.len()]).cyan(),
                        style(format!("{}s", started.elapsed().as_secs())).dim()
                    );
                    _ = term.clear_line();
                    _ = term.write_str(&line);
                    i += 1;
                    thread::sleep(Duration::from_millis(90));
                }
            });
            Some((stop, handle))
        } else {
            if !forced() {
                eprintln!("{label}...");
            }
            None
        };
        Stage { label: label.to_string(), started, spinner }
    }

    fn stop_spinner(&mut self) {
        if let Some((stop, handle)) = self.spinner.take() {
            stop.store(true, Ordering::Relaxed);
            _ = handle.join();
            _ = Term::stderr().clear_line();
        }
    }

    /// Replace the spinner with a finished line. `detail` is shown in front of the time.
    pub fn finish(mut self, detail: &str) {
        let seconds = self.started.elapsed().as_secs_f32();
        self.stop_spinner();
        let detail = if detail.is_empty() { String::new() } else { format!("{detail}  ") };
        eprintln!(
            "{} {}  {}{}",
            style(tick()).green().bold(),
            self.label,
            style(detail).dim(),
            style(format!("{seconds:.1}s")).dim()
        );
    }
}

/// A step that ends in an error never reaches `finish`; stop its spinner so the message that
/// follows starts on a clean line.
impl Drop for Stage {
    fn drop(&mut self) {
        self.stop_spinner();
    }
}
