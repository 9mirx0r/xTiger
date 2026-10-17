//! What the validator says while it works: a header with the game and the mod, and one line per
//! step with a spinner and the time it took. Everything here goes to stderr and is only meant for
//! a person at a terminal; when stderr is redirected the steps are plain lines.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use console::{StyledObject, Term};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_ASCII: [&str; 4] = ["|", "/", "-", "\\"];

/// `XTIGER_FORCE_TTY=1` gives the terminal experience when the output is captured, which is how
/// the pictures in the documentation are made. Steps are then printed without a spinner.
pub fn forced() -> bool {
    std::env::var_os("XTIGER_FORCE_TTY").is_some_and(|v| v == "1")
}

/// Style a piece of text for stderr, where everything here is written. The default would follow
/// stdout, so `> file` would change the colors of the card.
pub fn style<D>(value: D) -> StyledObject<D> {
    console::style(value).for_stderr()
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

/// Sizes the way people read them: `3.1 MB`, `820 KB`.
pub fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = bytes as f64;
    if value >= 1_000_000.0 {
        format!("{:.1} MB", value / 1_000_000.0)
    } else {
        format!("{:.0} KB", value / 1_000.0)
    }
}

const BAR_WIDTH: usize = 24;

/// The line of a download in progress: spinner, label, a bar, the percentage and the speed.
/// Kept apart from [`Bar`] so that it can be tested and drawn for the documentation.
pub fn bar_line(label: &str, done: u64, total: u64, speed: f64, frame: &str) -> String {
    let fraction = if total == 0 {
        0.0
    } else {
        #[allow(clippy::cast_precision_loss)]
        let f = done as f64 / total as f64;
        f.clamp(0.0, 1.0)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    let filled = (fraction * BAR_WIDTH as f64).round() as usize;
    let (full, empty) = if fancy_glyphs() { ("█", "░") } else { ("#", "-") };
    let bar = format!("{}{}", full.repeat(filled), empty.repeat(BAR_WIDTH - filled));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (fraction * 100.0).round() as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rate = size(speed as u64);
    format!(
        "{} {label}  {} {percent:>3}%  {}",
        style(frame).cyan(),
        style(bar).cyan(),
        style(format!("{} / {}  {rate}/s", size(done), size(total))).dim()
    )
}

/// A download with a progress bar. It is drawn from the thread that does the reading, at most
/// every 60 ms; when stderr is not a terminal only the finished line is printed.
pub struct Bar {
    label: String,
    total: u64,
    started: Instant,
    last_draw: Instant,
    term: Option<Term>,
}

impl Bar {
    pub fn start(label: &str, total: u64) -> Bar {
        let term = Term::stderr();
        let live = term.is_term();
        if !live && !forced() {
            eprintln!("{label}...");
        }
        let now = Instant::now();
        let mut bar = Bar {
            label: label.to_string(),
            total,
            started: now,
            last_draw: now,
            term: live.then_some(term),
        };
        bar.draw(0);
        bar
    }

    fn draw(&mut self, done: u64) {
        let Some(term) = &self.term else { return };
        let frames: &[&str] = if fancy_glyphs() { &SPINNER } else { &SPINNER_ASCII };
        let elapsed = self.started.elapsed();
        #[allow(clippy::cast_possible_truncation, clippy::integer_division)]
        let frame = frames[(elapsed.as_millis() / 90) as usize % frames.len()];
        #[allow(clippy::cast_precision_loss)]
        let speed = done as f64 / elapsed.as_secs_f64().max(0.001);
        _ = term.clear_line();
        _ = term.write_str(&bar_line(&self.label, done, self.total, speed, frame));
        self.last_draw = Instant::now();
    }

    pub fn set(&mut self, done: u64) {
        if self.last_draw.elapsed() >= Duration::from_millis(60) {
            self.draw(done);
        }
    }

    pub fn finish(mut self, detail: &str) {
        let seconds = self.started.elapsed().as_secs_f32();
        if let Some(term) = self.term.take() {
            _ = term.clear_line();
        }
        eprintln!(
            "{} {}  {}{}",
            style(tick()).green().bold(),
            self.label,
            style(format!("{detail}  ")).dim(),
            style(format!("{seconds:.1}s")).dim()
        );
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        if let Some(term) = self.term.take() {
            _ = term.clear_line();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_short() {
        assert_eq!(size(12_929_147), "12.9 MB");
        assert_eq!(size(820_000), "820 KB");
    }

    #[test]
    fn the_bar_fills_with_the_download() {
        let half = bar_line("file.zip", 50, 100, 2_000_000.0, "|");
        assert!(half.contains(" 50%"));
        assert!(half.contains("2.0 MB/s"));
        let done = bar_line("file.zip", 100, 100, 0.0, "|");
        assert!(done.contains("100%"));
        assert!(!bar_line("file.zip", 0, 0, 0.0, "|").contains("NaN"));
    }
}
