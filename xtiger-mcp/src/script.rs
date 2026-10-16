//! Running effect scripts in a CK3 window that is already open: one script, or a list of cases, with
//! what the game logged told apart from the noise it always logs.
//!
//! The game's console `run <file>` command runs plain effects on the player character. It also builds
//! the effects' tooltip, which logs errors of its own, and every run logs again the warnings that
//! earlier runs caused. [`filter`] keeps what a script itself did wrong.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::{Regex, RegexBuilder};

use crate::game::{Shot, WINDOWS_ONLY, send_keys};
use crate::helper::Step;
use crate::locate::Locations;

/// The file the game's console `run` command reads, in the `run` folder of the user folder.
const SCRIPT_FILE: &str = "xtiger_script.txt";
/// The most an effect script may hold: a console line is no place for a whole mod.
const MAX_SCRIPT_BYTES: usize = 64 * 1024;
/// The most cases in one call: each takes a few seconds in the game, and a client may give up waiting.
pub const MAX_CASES: usize = 25;
/// How many entries or lines of one kind a report shows.
const MAX_SHOWN: usize = 40;

/// The text of the file for an effect script: the effects as they were written, then a `debug_log`
/// line with `marker`, which the game writes to debug.log once it has run everything above it.
pub fn script_file_text(effects: &str, marker: &str) -> Result<String, String> {
    let effects = effects.trim();
    if effects.is_empty() {
        return Err("effects is empty: give the effects to run, one per line.".to_owned());
    }
    if effects.len() > MAX_SCRIPT_BYTES {
        return Err(format!("effects is longer than {MAX_SCRIPT_BYTES} bytes."));
    }
    if let Some(line) = effects
        .lines()
        .map(str::trim)
        .find(|line| line.split_whitespace().next().is_some_and(|word| word == "effect"))
    {
        return Err(format!(
            "`{line}` is a console line. The file holds plain effects (add_gold = 1), without the effect prefix."
        ));
    }
    Ok(format!("{effects}\ndebug_log = \"{marker}\"\n"))
}

/// The fixtures a script can ask for, as `kind:name` or `kind:name=value`, with what each makes.
pub const FIXTURE_HELP: &str = "character:NAME[=AGE] (a new adult of the player's culture and faith, age 30), courtier:NAME[=AGE] (the same, at the player's court), child:NAME[=AGE] (the player's child, age 5), self:NAME (the player), capital:NAME (the capital province), county:NAME (the capital county), title:NAME (the primary title), house:NAME, dynasty:NAME, culture:NAME, faith:NAME, value:NAME=NUMBER (a saved value)";

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The effects that make one fixture: a scope saved under `name`, ready for an event the script then
/// fires. `character:friend` makes `scope:friend`.
pub fn fixture_effects(spec: &str) -> Result<String, String> {
    let bad = |why: &str| format!("fixture `{spec}`: {why}. The fixtures are {FIXTURE_HELP}.");
    let (kind, rest) = spec.trim().split_once(':').ok_or_else(|| bad("it is not kind:name"))?;
    let (name, arg) = match rest.split_once('=') {
        Some((name, arg)) => (name.trim(), Some(arg.trim())),
        None => (rest.trim(), None),
    };
    if !is_name(name) {
        return Err(bad("the name is lower-case letters, digits and _"));
    }
    let age = |default: u32| -> Result<u32, String> {
        arg.map_or(Ok(default), |text| {
            text.parse::<u32>()
                .ok()
                .filter(|age| *age <= 100)
                .ok_or_else(|| bad("the age is a whole number up to 100"))
        })
    };
    // A character is placed at a location or at a court, not both.
    let person = |age: u32, extra: &str| {
        format!(
            "create_character = {{ name = \"Xtiger\" age = {age} gender = male culture = root.culture faith = root.faith {extra} save_scope_as = {name} }}"
        )
    };
    let saved = |link: &str| format!("{link} = {{ save_scope_as = {name} }}");
    match (kind, arg) {
        ("character", _) => Ok(person(age(30)?, "location = root.location")),
        ("courtier", _) => Ok(person(age(30)?, "employer = root")),
        ("child", _) => Ok(person(age(5)?, "father = root location = root.location")),
        ("self", None) => Ok(format!("save_scope_as = {name}")),
        ("capital", None) => Ok(saved("capital_province")),
        ("county", None) => Ok(saved("capital_county")),
        ("title", None) => Ok(saved("primary_title")),
        ("house", None) => Ok(saved("house")),
        ("dynasty", None) => Ok(saved("dynasty")),
        ("culture", None) => Ok(saved("culture")),
        ("faith", None) => Ok(saved("faith")),
        ("value", Some(number)) => {
            if number.parse::<f64>().is_err() {
                return Err(bad("the value is a number"));
            }
            Ok(format!("save_scope_value_as = {{ name = {name} value = {number} }}"))
        }
        ("value", None) => Err(bad("value needs a number, as value:name=5")),
        (
            "self" | "capital" | "county" | "title" | "house" | "dynasty" | "culture" | "faith",
            Some(_),
        ) => Err(bad("this fixture takes no =value")),
        _ => Err(bad("unknown kind")),
    }
}

/// The effects of a script with its fixtures in front, and how many lines the fixtures take (the
/// lines the game reports are counted from the top of the file).
pub fn with_fixtures(with: &[String], effects: &str) -> Result<(String, usize), String> {
    let mut lines = Vec::new();
    for spec in with {
        lines.push(fixture_effects(spec)?);
    }
    let count = lines.len();
    lines.push(effects.trim().to_owned());
    Ok((
        lines.join(
            "
",
        ),
        count,
    ))
}

/// One entry of error.log: a header line and the indented lines under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The game's source file and line that logged it, such as `jomini_script_system.cpp:304`.
    pub source: String,
    pub message: String,
    /// The indented lines under the header, trimmed.
    pub body: Vec<String>,
}

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[\d{2}:\d{2}:\d{2}\]\[[A-Z]\]\[([^\]]+)\]: ?(.*)$").expect("a valid regex")
});

/// The entries in the text of an error.log: a header, then its indented lines up to a blank line.
pub fn parse_entries(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        if let Some(caps) = HEADER.captures(line) {
            entries.push(Entry {
                source: caps[1].to_owned(),
                message: caps[2].trim().to_owned(),
                body: Vec::new(),
            });
            open = true;
        } else if line.trim().is_empty() {
            open = false;
        } else if open && line.starts_with(char::is_whitespace) {
            if let Some(last) = entries.last_mut() {
                last.body.push(line.trim().to_owned());
            }
        } else {
            open = false;
        }
    }
    entries
}

impl Entry {
    /// What makes two entries the same problem: where it was logged and what it said, without the time.
    pub fn signature(&self) -> String {
        format!("{}|{}|{}", self.source, self.message, self.body.join("|"))
    }

    /// Logged while the game builds the tooltip of the effects, which `run` does too: the same error
    /// shows up again, without the label, when the effects run, and an effect that is fine to run can
    /// still fail to describe itself.
    pub fn is_tooltip_pass(&self) -> bool {
        let said = |text: &str| text.to_ascii_lowercase().contains("while building");
        said(&self.message) || self.body.iter().any(|line| said(line))
    }

    /// A warning from the check of all event targets, which every script the game compiles logs again:
    /// news only the first time.
    pub fn is_repeating_warning(&self) -> bool {
        self.message.contains("is used but is never set")
            || self.message.contains("is set but is never used")
    }

    /// The entry on one line for a report.
    pub fn describe(&self) -> String {
        let mut text = if self.body.is_empty() || self.message != "Script system error!" {
            format!("{}: {}", self.source, self.message)
        } else {
            self.source.clone()
        };
        for line in &self.body {
            let _ = write!(text, "{}{line}", if text.ends_with(':') { " " } else { " | " });
        }
        match text.char_indices().nth(400) {
            Some((at, _)) => format!("{}...", &text[..at]),
            None => text,
        }
    }
}

/// The signatures of the repeating warnings in the old part of an error.log: what it already said.
pub fn known_warnings(old: &str) -> HashSet<String> {
    parse_entries(old)
        .iter()
        .filter(|entry| entry.is_repeating_warning())
        .map(Entry::signature)
        .collect()
}

/// What a script did wrong, once the noise is out.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Filtered {
    /// Each different entry once, with how many times it was logged.
    pub kept: Vec<(Entry, usize)>,
    /// Entries left out because the game was only describing the effects.
    pub tooltip: usize,
    /// Warnings left out because the log already said them before the script ran.
    pub repeats: usize,
}

/// Keep the entries a script caused: not the tooltip pass, not warnings the log already held in
/// `known`, and each different entry once.
pub fn filter<S: std::hash::BuildHasher>(
    entries: Vec<Entry>,
    known: &HashSet<String, S>,
) -> Filtered {
    let mut out = Filtered::default();
    let mut at: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        if entry.is_tooltip_pass() {
            out.tooltip += 1;
            continue;
        }
        let signature = entry.signature();
        if entry.is_repeating_warning() && known.contains(&signature) {
            out.repeats += 1;
            continue;
        }
        if let Some(&index) = at.get(&signature) {
            out.kept[index].1 += 1;
        } else {
            at.insert(signature, out.kept.len());
            out.kept.push((entry, 1));
        }
    }
    out
}

impl Filtered {
    /// The kept entries as report lines, then a note on what was left out.
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .kept
            .iter()
            .take(MAX_SHOWN)
            .map(|(entry, count)| {
                let times = if *count > 1 { format!(" (x{count})") } else { String::new() };
                format!("{}{times}", entry.describe())
            })
            .collect();
        if self.kept.len() > MAX_SHOWN {
            lines.push(format!("... and {} more", self.kept.len() - MAX_SHOWN));
        }
        lines
    }

    /// What was left out, for the end of a report; empty when nothing was. The warnings the log already
    /// held are not worth a line: every run has them.
    pub fn left_out(&self) -> String {
        if self.tooltip == 0 {
            String::new()
        } else {
            format!("(left out: {} from the game describing the effects)", self.tooltip)
        }
    }
}

/// The complete lines of a log written after byte `from` that match `pattern` (a regex, case-insensitive).
/// A log that is missing, or shorter than `from`, gives nothing.
pub fn lines_since(path: &Path, from: u64, pattern: &str) -> Vec<String> {
    let Ok(regex) = RegexBuilder::new(pattern).case_insensitive(true).build() else {
        return Vec::new();
    };
    fresh_text(path, from).lines().filter(|line| regex.is_match(line)).map(str::to_owned).collect()
}

/// The complete lines of a log written after byte `from`.
fn fresh_text(path: &Path, from: u64) -> String {
    let Ok(bytes) = fs::read(path) else { return String::new() };
    let Some(fresh) = usize::try_from(from).ok().and_then(|at| bytes.get(at..)) else {
        return String::new();
    };
    let whole = fresh.iter().rposition(|b| *b == b'\n').map_or(0, |at| at + 1);
    String::from_utf8_lossy(&fresh[..whole]).into_owned()
}

/// The text of a log before byte `to`.
fn old_text(path: &Path, to: u64) -> String {
    let Ok(bytes) = fs::read(path) else { return String::new() };
    let end = usize::try_from(to).map_or(bytes.len(), |at| at.min(bytes.len()));
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Wait for a log that is still being written to stop growing, up to `limit`.
fn wait_until_still(path: &Path, limit: Duration) {
    let size = || fs::metadata(path).map_or(0, |m| m.len());
    let start = Instant::now();
    let (mut last, mut since) = (size(), Instant::now());
    while start.elapsed() < limit && since.elapsed() < Duration::from_millis(500) {
        thread::sleep(Duration::from_millis(100));
        let now = size();
        if now != last {
            (last, since) = (now, Instant::now());
        }
    }
}

/// What came of running one script.
#[derive(Debug, Default)]
pub struct Outcome {
    pub marker: String,
    /// Why the script did not run to its end; none when it did.
    pub failure: Option<String>,
    /// What `debug_log` effects printed while the script ran (the script's own and those of the events it
    /// fired), and the game's console failures.
    pub printed: Vec<String>,
    /// What the game logged in error.log because of the script.
    pub errors: Filtered,
    /// What the key presses reported, when they went through.
    pub sent: String,
    pub shot: Option<PathBuf>,
    /// How many lines at the top of the script are fixtures, which shifts the lines the game reports.
    pub setup_lines: usize,
}

/// The line the game logs for a `debug_log` effect, without its time and source.
fn printed_line(line: &str) -> String {
    static PREFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\[[^\]]*\]\[[A-Z]\]\[[^\]]*\]: ?").expect("a valid regex"));
    PREFIX.replace(line, "").into_owned()
}

/// Run a script of effects on the player in the open game: write it to the `run` folder, type `run`
/// for it in the console, and wait for the marker the script ends with. A failure to run it is in
/// the outcome; only a script that cannot be sent at all is an error.
pub fn execute(
    loc: &Locations,
    effects: &str,
    with: &[String],
    wait_ms: u64,
    close_console: bool,
    screenshot: Shot,
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    if !cfg!(windows) {
        return Err(WINDOWS_ONLY.to_owned());
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let marker = format!("XTIGER_DONE_{nanos:x}");
    let (effects, setup_lines) = with_fixtures(with, effects)?;
    let text = script_file_text(&effects, &marker)?;
    let folder = loc.user_dir.join("run");
    fs::create_dir_all(&folder).map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    let file = folder.join(SCRIPT_FILE);
    // With the byte order mark: without it the game logs a warning about the file's encoding every run.
    fs::write(&file, format!("\u{feff}{text}"))
        .map_err(|e| format!("cannot write {}: {e}", file.display()))?;

    let logs = loc.user_dir.join("logs");
    let (debug_log, error_log) = (logs.join("debug.log"), logs.join("error.log"));
    let size = |path: &Path| fs::metadata(path).map_or(0, |m| m.len());
    let (debug_from, error_from) = (size(&debug_log), size(&error_log));
    let known = known_warnings(&old_text(&error_log, error_from));
    let step = |scancodes: &str, wait_ms: u64| Step {
        scancodes: scancodes.into(),
        wait_ms,
        ..Step::default()
    };
    let asked = |what: &str| -> Result<bool, String> {
        send_keys(loc, &[step("?console", 0)], Shot::OFF, cancelled)
            .map(|(said, _)| said.contains("console open"))
            .map_err(|why| format!("{what}: {why}"))
    };
    // The console key is a toggle: pressed with the console open it closes it, and the text typed next
    // would go to the game as hotkeys. So the picture of the window says whether it is open first, and
    // the key is only pressed when it is not. Nothing is typed unless the console is seen open.
    let open_console = || -> Result<(), String> {
        if !asked("Cannot tell whether the console is open")? {
            // The key that opens the console can leave its own `|` in the line, so the line is wiped of it.
            send_keys(loc, &[step("29", 600), step("0E", 100)], Shot::OFF, cancelled)?;
            if !asked("Cannot tell whether the console opened")? {
                return Err("The console did not open after its key; nothing was typed.".to_owned());
            }
        }
        Ok(())
    };
    let mut steps = vec![
        Step { text: format!("run {SCRIPT_FILE}"), ..Step::default() },
        Step {
            scancodes: "1C".into(),
            wait_ms,
            log: "debug".into(),
            wait_for: marker.clone(),
            ..Step::default()
        },
    ];
    if close_console {
        steps.push(step("29", 300));
    }
    // Never sent twice: the game writes debug.log late, so "the command is not in the log yet" does not
    // prove it did not run, and a script with real effects must not run twice.
    let sent = open_console().and_then(|()| send_keys(loc, &steps, screenshot, cancelled));
    // error.log is written late too.
    wait_until_still(&error_log, Duration::from_millis(2500));
    let errors = filter(parse_entries(&fresh_text(&error_log, error_from)), &known);
    let mut outcome = Outcome { marker: marker.clone(), errors, setup_lines, ..Outcome::default() };
    let pattern = match &sent {
        Ok(_) => r"jomini_effect_impl\.cpp:450\]|console_failure".to_owned(),
        // The marker itself too: a script that ends up inside an unclosed block logs it in some other
        // form, and that says where the game stopped reading.
        Err(_) => format!(r"jomini_effect_impl\.cpp:450\]|console_failure|{marker}"),
    };
    outcome.printed = lines_since(&debug_log, debug_from, &pattern)
        .iter()
        .filter(|line| sent.is_err() || !line.contains(&marker))
        .map(|line| printed_line(line))
        .collect();
    match sent {
        Ok((said, shot)) => {
            outcome.sent = said;
            outcome.shot = shot.filter(|path| path.is_file());
        }
        Err(why) => outcome.failure = Some(why),
    }
    Ok(outcome)
}

/// The report of one script: what it printed and what the game said was wrong with it.
fn report(what: &str, outcome: &Outcome) -> String {
    let mut text = what.to_owned();
    if !outcome.printed.is_empty() {
        let _ = write!(
            text,
            "\nGame log:\n{}",
            outcome.printed[..outcome.printed.len().min(MAX_SHOWN)].join("\n")
        );
    }
    let errors = outcome.errors.lines();
    if !errors.is_empty() {
        let _ = write!(text, "\nerror.log:\n{}", errors.join("\n"));
    }
    let left_out = outcome.errors.left_out();
    if !left_out.is_empty() {
        let _ = write!(text, "\n{left_out}");
    }
    if outcome.setup_lines > 0 && (!outcome.printed.is_empty() || !outcome.errors.kept.is_empty()) {
        let _ = write!(
            text,
            "\n(the first {} lines of the file are the fixtures)",
            outcome.setup_lines
        );
    }
    text
}

/// Run one script for `ck3_script`: says what the script's own `debug_log` lines printed, or on
/// failure what the game said was wrong with it.
pub fn run_script(
    loc: &Locations,
    effects: &str,
    with: &[String],
    wait_ms: u64,
    close_console: bool,
    screenshot: Shot,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    let outcome = execute(loc, effects, with, wait_ms, close_console, screenshot, cancelled)?;
    match &outcome.failure {
        None => {
            let what = format!("Script ran to its end ({}).", outcome.marker);
            Ok((format!("{}\n{}", report(&what, &outcome), outcome.sent), outcome.shot.clone()))
        }
        Some(why) => Err(report(
            &format!(
                "The script did not finish: {why}\nThe game may still have run it (debug.log is written late), so check the game before sending it again."
            ),
            &outcome,
        )),
    }
}

/// One script of a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub effects: String,
    /// The fixtures set up before the effects.
    pub with: Vec<String>,
}

/// What one run printed and which errors it caused, as indented report lines.
fn outcome_lines(outcome: &Outcome) -> String {
    let mut body = String::new();
    if let Some(why) = &outcome.failure {
        let _ = writeln!(body, "  {why}");
    }
    for line in &outcome.printed {
        let _ = writeln!(body, "  log: {line}");
    }
    for line in outcome.errors.lines() {
        let _ = writeln!(body, "  error.log: {line}");
    }
    let left_out = outcome.errors.left_out();
    if !left_out.is_empty() {
        let _ = writeln!(body, "  {left_out}");
    }
    if outcome.setup_lines > 0 && (!outcome.printed.is_empty() || !outcome.errors.kept.is_empty()) {
        let _ = writeln!(
            body,
            "  (the first {} lines of the file are the fixtures)",
            outcome.setup_lines
        );
    }
    body
}

/// Run the cases one after the other in the open game and say, for each, what it printed and which
/// errors it caused that the log did not hold before. Stops at the first case that does not finish:
/// the game may be busy or gone, and what ran last is uncertain.
pub fn run_cases(
    loc: &Locations,
    cases: &[Case],
    wait_ms: u64,
    close_console: bool,
    screenshot: Shot,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    if cases.is_empty() {
        return Err("cases is empty: give at least one case.".to_owned());
    }
    if cases.len() > MAX_CASES {
        return Err(format!("At most {MAX_CASES} cases in one call."));
    }
    // Everything is checked before the first one is sent.
    for case in cases {
        with_fixtures(&case.with, &case.effects)
            .and_then(|(effects, _)| script_file_text(&effects, "M"))
            .map_err(|why| format!("case {}: {why}", case.name))?;
    }
    let (mut clean, mut broken, mut unfinished) = (0, 0, 0);
    let mut body = String::new();
    let mut shot = None;
    for (index, case) in cases.iter().enumerate() {
        let last = index + 1 == cases.len();
        let outcome = execute(
            loc,
            &case.effects,
            &case.with,
            wait_ms,
            close_console && last,
            screenshot.when(last),
            cancelled,
        )
        .map_err(|why| format!("case {}: {why}", case.name))?;
        let state = if outcome.failure.is_some() {
            unfinished += 1;
            "NOT FINISHED"
        } else if outcome.errors.kept.is_empty() {
            clean += 1;
            "ok"
        } else {
            broken += 1;
            "ERRORS"
        };
        let _ = writeln!(body, "[{state}] {}", case.name);
        body.push_str(&outcome_lines(&outcome));
        shot = outcome.shot.clone().or(shot);
        if outcome.failure.is_some() {
            let skipped = cases.len() - index - 1;
            if skipped > 0 {
                let _ = writeln!(body, "{skipped} more cases were not run.");
            }
            break;
        }
        if cancelled() {
            let _ = writeln!(body, "Stopped: the call was cancelled.");
            break;
        }
    }
    let summary = format!(
        "{} of {} cases run: {clean} ok, {broken} with errors, {unfinished} not finished.",
        clean + broken + unfinished,
        cases.len()
    );
    let text = format!("{summary}\n{}", body.trim_end());
    if unfinished > 0 { Err(text) } else { Ok((text, shot)) }
}

/// The ids of the events defined in the text of an events file: the `namespace.number = {` blocks at
/// the top level, not what is inside them.
pub fn event_ids(text: &str) -> Vec<String> {
    static START: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*\.[A-Za-z0-9_]+)\s*=\s*\{").expect("a valid regex")
    });
    let mut ids = Vec::new();
    let mut depth = 0_i32;
    for line in text.lines() {
        if depth == 0
            && let Some(caps) = START.captures(line)
        {
            ids.push(caps[1].to_owned());
        }
        let mut quoted = false;
        for c in line.chars() {
            match c {
                '"' => quoted = !quoted,
                '#' if !quoted => break,
                '{' if !quoted => depth += 1,
                '}' if !quoted => depth = (depth - 1).max(0),
                _ => {}
            }
        }
    }
    ids
}

/// The events a mod defines, as `(id, file)` with the file relative to the mod folder, in file order.
pub fn mod_events(dir: &Path) -> Vec<(String, String)> {
    fn collect(folder: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(folder) else { return };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    collect(&dir.join("events"), &mut files);
    files.sort();
    let mut events = Vec::new();
    for file in files {
        let Ok(bytes) = fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let relative = file.strip_prefix(dir).map_or_else(
            |_| file.display().to_string(),
            |path| path.to_string_lossy().replace('\\', "/"),
        );
        events.extend(event_ids(&text).into_iter().map(|id| (id, relative.clone())));
    }
    events
}

/// The names of scopes the game said were undefined in the new errors: `Undefined event target 'friend'`,
/// or, in an event's text, `Failed to find type 'friend' in 'friend.GetName'`.
pub fn missing_scopes(errors: &Filtered) -> Vec<String> {
    static UNDEFINED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"Undefined event target '([A-Za-z0-9_]+)'|Failed to find type '([a-z][A-Za-z0-9_]*)' in '[A-Za-z0-9_]+\.",
        )
        .expect("a valid regex")
    });
    const BUILT_IN: [&str; 6] = ["root", "prev", "this", "from", "actor", "recipient"];
    let mut names: Vec<String> = Vec::new();
    for (entry, _) in &errors.kept {
        let lines = std::iter::once(&entry.message).chain(&entry.body);
        for caps in lines.filter_map(|line| UNDEFINED.captures(line)) {
            let name = caps.get(1).or_else(|| caps.get(2)).map_or("", |m| m.as_str());
            if !name.is_empty()
                && !BUILT_IN.contains(&name)
                && !names.iter().any(|known| known == name)
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// The fixture that most likely makes `scope:name`, judging by what it is called.
pub fn guess_fixture(name: &str) -> String {
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    let kind = if has(&["child", "kid", "son", "daughter", "heir", "baby", "infant"]) {
        "child"
    } else if has(&["courtier", "councillor", "guest", "knight"]) {
        "courtier"
    } else if has(&["title", "barony", "duchy", "kingdom", "empire"]) {
        "title"
    } else if has(&["county"]) {
        "county"
    } else if has(&["province", "capital", "location"]) {
        "capital"
    } else if has(&["faith", "religion"]) {
        "faith"
    } else if has(&["culture"]) {
        "culture"
    } else if has(&["dynasty"]) {
        "dynasty"
    } else if has(&["house"]) {
        "house"
    } else if has(&[
        "value", "level", "amount", "count", "number", "days", "years", "chance", "stage",
    ]) {
        return format!("value:{name}=1");
    } else if has(&["root", "actor", "player", "ruler", "self"]) {
        "self"
    } else {
        "character"
    };
    format!("{kind}:{name}")
}

/// The name a fixture spec saves its scope under.
fn fixture_name(spec: &str) -> &str {
    let rest = spec.split_once(':').map_or(spec, |(_, rest)| rest);
    rest.split_once('=').map_or(rest, |(name, _)| name).trim()
}

/// What an event id looks like: `namespace.number`.
static EVENT_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z0-9_]+$").expect("a valid regex")
});

/// The most times an event is run again with the scopes its errors asked for.
const MAX_RETRIES: usize = 3;

/// Fire each event in the open game, one after the other, with `with` set up first. When the game says
/// an event target is undefined, the fixture that most likely makes it is added and the event is
/// fired again, so an event that needs `scope:friend` gets one without being told. Says for each
/// event what it printed and which new errors it caused, and which fixtures it ended up with.
pub fn run_events(
    loc: &Locations,
    events: &[String],
    with: &[String],
    wait_ms: u64,
    close_console: bool,
    screenshot: Shot,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, Option<PathBuf>), String> {
    if events.is_empty() {
        return Err("events is empty: give at least one event id.".to_owned());
    }
    if events.len() > MAX_CASES {
        return Err(format!("At most {MAX_CASES} events in one call."));
    }
    for id in events {
        if !EVENT_ID.is_match(id) {
            return Err(format!(
                "event {id}: an event id is `namespace.number`, such as my_mod.0001."
            ));
        }
        with_fixtures(with, &format!("trigger_event = {id}"))
            .map_err(|why| format!("event {id}: {why}"))?;
    }
    let (mut clean, mut silent, mut broken, mut unfinished) = (0, 0, 0, 0);
    let mut body = String::new();
    let mut shot = None;
    for (index, id) in events.iter().enumerate() {
        let last = index + 1 == events.len();
        let mut fixtures = with.to_vec();
        let mut learned: Vec<String> = Vec::new();
        let mut round = 0;
        let (outcome, unresolved) = loop {
            let outcome = execute(
                loc,
                &format!("trigger_event = {id}"),
                &fixtures,
                wait_ms,
                close_console && last,
                screenshot.when(last),
                cancelled,
            )
            .map_err(|why| format!("event {id}: {why}"))?;
            let have: Vec<&str> = fixtures.iter().map(|spec| fixture_name(spec)).collect();
            let wanted: Vec<String> = missing_scopes(&outcome.errors)
                .into_iter()
                .filter(|name| !have.contains(&name.as_str()))
                .collect();
            let given_up = outcome.failure.is_some() || wanted.is_empty() || round == MAX_RETRIES;
            if given_up {
                let unresolved = missing_scopes(&outcome.errors);
                break (outcome, unresolved);
            }
            for name in wanted {
                let spec = guess_fixture(&name);
                learned.push(spec.clone());
                fixtures.push(spec);
            }
            round += 1;
        };
        let state = if outcome.failure.is_some() {
            unfinished += 1;
            "NOT FINISHED"
        } else if !unresolved.is_empty() {
            broken += 1;
            "NEEDS SCOPES"
        } else if !outcome.errors.kept.is_empty() {
            broken += 1;
            "ERRORS"
        } else if outcome.printed.is_empty() {
            silent += 1;
            "silent"
        } else {
            clean += 1;
            "ok"
        };
        let added = if learned.is_empty() {
            String::new()
        } else {
            format!(" (added {})", learned.join(", "))
        };
        let _ = writeln!(body, "[{state}] {id}{added}");
        body.push_str(&outcome_lines(&outcome));
        shot = outcome.shot.clone().or(shot);
        if outcome.failure.is_some() {
            let skipped = events.len() - index - 1;
            if skipped > 0 {
                let _ = writeln!(body, "{skipped} more events were not fired.");
            }
            let _ = writeln!(
                body,
                "(An event window left open can stall the next run: look with ck3_keys, close it with ck3_click, and continue with skip.)"
            );
            break;
        }
        if cancelled() {
            let _ = writeln!(body, "Stopped: the call was cancelled.");
            break;
        }
    }
    let summary = format!(
        "{} of {} events fired: {clean} ok, {silent} silent, {broken} with errors, {unfinished} not finished.",
        clean + silent + broken + unfinished,
        events.len()
    );
    let note = if silent > 0 {
        "\n(silent: no error and nothing logged. The event may print nothing, or its trigger was false and it did not run. A visible event opens its window: ck3_keys with no keys shows it, ck3_click picks an option.)"
    } else {
        ""
    };
    let text = format!("{summary}\n{}{note}", body.trim_end());
    if unfinished > 0 { Err(text) } else { Ok((text, shot)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn a_script_file_ends_with_the_marker_and_trims_the_effects() {
        let text = script_file_text("  add_gold = 5\nadd_piety = 2  ", "XTIGER_DONE_1").unwrap();
        assert_eq!(text, "add_gold = 5\nadd_piety = 2\ndebug_log = \"XTIGER_DONE_1\"\n");
    }

    #[test]
    fn a_script_must_not_be_empty_or_hold_console_lines() {
        assert!(script_file_text(" \n ", "M").unwrap_err().contains("empty"));
        let err = script_file_text("add_gold = 1\n  effect add_gold = 1", "M").unwrap_err();
        assert!(err.contains("`effect add_gold = 1`") && err.contains("console line"), "{err}");
        // A word that only starts with "effect" is not the console prefix.
        assert!(script_file_text("effect_not_a_prefix = 1", "M").is_ok());
        let long = "a".repeat(MAX_SCRIPT_BYTES + 1);
        assert!(script_file_text(&long, "M").unwrap_err().contains("longer"));
    }

    #[test]
    fn new_log_lines_are_the_complete_matching_ones_after_an_offset() {
        let tmp = TempDir::new();
        let log = tmp.join("debug.log");
        let old = "old file: run/xtiger_script.txt line: 0: old\n";
        let text = format!(
            "{old}file: run/xtiger_script.txt line: 1: one\nother\nfile: run/xtiger_script.txt line: 2: tw"
        );
        fs::write(&log, text).unwrap();
        assert_eq!(
            lines_since(&log, old.len() as u64, "file: run/xtiger_script"),
            vec!["file: run/xtiger_script.txt line: 1: one".to_owned()]
        );
        // A log that restarted is shorter than the offset: nothing, not a panic.
        assert!(lines_since(&log, 10_000, "").is_empty());
        assert!(lines_since(&tmp.join("missing.log"), 0, "").is_empty());
    }

    const SAMPLE: &str = "\
[18:20:56][E][pdx_persistent_reader.cpp:216]: Error: \"Unknown effect: bogus, near line: 0\" in file: \"run/xtiger_script.txt\" near line: 3
[18:20:57][E][jomini_effect.cpp:1162]: Event target 'nobody' is used but is never set. Setting it in an unused scripted trigger or effect does not count
[18:20:57][E][jomini_script_system.cpp:304]: Script system error! (while building tooltip/description)
  Error: Undefined event target 'nobody'
  Script location: file: run/xtiger_script.txt line: 1

[18:20:57][E][jomini_script_system.cpp:304]: Script system error!
  Error: Undefined event target 'nobody'
  Script location: file: run/xtiger_script.txt line: 1

[18:20:57][E][jomini_script_system.cpp:304]: Script system error!
  Error: Undefined event target 'nobody'
  Script location: file: run/xtiger_script.txt line: 1

";

    #[test]
    fn entries_have_a_header_and_the_indented_lines_under_it() {
        let entries = parse_entries(SAMPLE);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].source, "pdx_persistent_reader.cpp:216");
        assert!(entries[0].body.is_empty());
        assert_eq!(entries[2].body.len(), 2);
        assert_eq!(entries[2].body[0], "Error: Undefined event target 'nobody'");
        // An indented line after a blank one belongs to nothing.
        assert_eq!(parse_entries("\n  stray\n").len(), 0);
        assert_eq!(parse_entries("[10:00:00][E][a.cpp:1]: one\n\n  stray\n")[0].body.len(), 0);
    }

    #[test]
    fn the_filter_drops_the_tooltip_pass_and_known_warnings_and_counts_repeats() {
        let entries = parse_entries(SAMPLE);
        let none = HashSet::new();
        let kept = filter(entries.clone(), &none);
        assert_eq!(kept.tooltip, 1);
        assert_eq!(kept.repeats, 0);
        // The warning is new, the unknown effect, and the runtime error twice.
        assert_eq!(kept.kept.len(), 3);
        assert_eq!(kept.kept[2].1, 2);
        // A warning the log held before is not news.
        let known = known_warnings(
            "[10:00:00][E][jomini_effect.cpp:1162]: Event target 'nobody' is used but is never set. Setting it in an unused scripted trigger or effect does not count\n",
        );
        let again = filter(entries, &known);
        assert_eq!(again.repeats, 1);
        assert_eq!(again.kept.len(), 2);
        assert!(again.left_out().contains("1 from the game describing"));
        assert_eq!(filter(Vec::new(), &none).left_out(), "");
    }

    #[test]
    fn an_error_is_never_dropped_for_having_been_logged_before() {
        // Only the repeating warnings are taken as known; a runtime error of an earlier case still counts.
        let known = known_warnings(SAMPLE);
        assert_eq!(known.len(), 1);
        let kept = filter(parse_entries(SAMPLE), &known);
        assert_eq!(kept.kept.len(), 2);
    }

    #[test]
    fn entries_read_as_one_line() {
        let entries = parse_entries(SAMPLE);
        assert_eq!(
            entries[3].describe(),
            "jomini_script_system.cpp:304 | Error: Undefined event target 'nobody' | Script location: file: run/xtiger_script.txt line: 1"
        );
        assert!(
            entries[0]
                .describe()
                .starts_with("pdx_persistent_reader.cpp:216: Error: \"Unknown effect")
        );
        let long = Entry { source: "a.cpp:1".into(), message: "x".repeat(900), body: vec![] };
        assert!(long.describe().len() < 420);
    }

    #[test]
    fn the_game_log_prefix_is_dropped_from_printed_lines() {
        assert_eq!(
            printed_line(
                "[18:18:32][D][jomini_effect_impl.cpp:450]: file: run/xtiger_script.txt line: 1: hi"
            ),
            "file: run/xtiger_script.txt line: 1: hi"
        );
        assert_eq!(printed_line("no prefix"), "no prefix");
    }

    #[test]
    fn fixtures_save_scopes_under_the_name_asked_for() {
        let friend = fixture_effects("character:friend").unwrap();
        assert!(
            friend.contains("age = 30") && friend.ends_with("save_scope_as = friend }"),
            "{friend}"
        );
        assert!(fixture_effects("child:kid=7").unwrap().contains("age = 7 gender = male"));
        assert!(fixture_effects("courtier:c").unwrap().contains("employer = root"));
        assert!(fixture_effects("child:kid").unwrap().contains("father = root"));
        assert_eq!(fixture_effects("self:me").unwrap(), "save_scope_as = me");
        assert_eq!(
            fixture_effects("capital:p").unwrap(),
            "capital_province = { save_scope_as = p }"
        );
        assert_eq!(
            fixture_effects(" value:level=2.5 ").unwrap(),
            "save_scope_value_as = { name = level value = 2.5 }"
        );
    }

    #[test]
    fn a_bad_fixture_says_what_is_wrong_and_lists_the_good_ones() {
        for (spec, why) in [
            ("friend", "kind:name"),
            ("character:Friend", "lower-case"),
            ("character:a b", "lower-case"),
            ("child:k=old", "whole number"),
            ("child:k=101", "whole number"),
            ("value:x", "needs a number"),
            ("value:x=abc", "is a number"),
            ("capital:p=1", "no =value"),
            ("wizard:w", "unknown kind"),
        ] {
            let err = fixture_effects(spec).unwrap_err();
            assert!(err.contains(why) && err.contains("courtier:NAME"), "{spec}: {err}");
        }
    }

    #[test]
    fn fixtures_go_in_front_of_the_effects_and_are_counted() {
        let with = vec!["character:friend".to_owned(), "value:n=3".to_owned()];
        let (text, lines) = with_fixtures(&with, "  scope:friend = { add_gold = 1 }\n").unwrap();
        assert_eq!(lines, 2);
        assert_eq!(text.lines().count(), 3);
        assert!(text.ends_with("scope:friend = { add_gold = 1 }"));
        assert_eq!(with_fixtures(&[], "a = 1").unwrap(), ("a = 1".to_owned(), 0));
        assert!(with_fixtures(&["nope".to_owned()], "a = 1").is_err());
    }

    #[test]
    fn cases_are_checked_before_any_is_sent() {
        let loc = Locations::detect();
        let case = |name: &str, effects: &str| Case {
            name: name.into(),
            effects: effects.into(),
            with: Vec::new(),
        };
        let err = run_cases(&loc, &[], 1000, false, Shot::OFF, &|| false).unwrap_err();
        assert!(err.contains("empty"), "{err}");
        let many: Vec<Case> =
            (0..=MAX_CASES).map(|n| case(&format!("c{n}"), "add_gold = 1")).collect();
        assert!(
            run_cases(&loc, &many, 1000, false, Shot::OFF, &|| false)
                .unwrap_err()
                .contains("At most")
        );
        let err = run_cases(
            &loc,
            &[case("ok", "add_gold = 1"), case("bad", "effect add_gold = 1")],
            1000,
            false,
            Shot::OFF,
            &|| false,
        )
        .unwrap_err();
        assert!(err.contains("case bad") && err.contains("console line"), "{err}");
    }

    #[test]
    fn the_events_of_a_file_are_the_blocks_at_the_top_level() {
        let text = "\
namespace = my_mod

# my_mod.0009 = { commented out }
my_mod.0001 = {
	type = character_event
	option = {
		name = \"my_mod.0001.a\" # }
		trigger_event = my_mod.0002
	}
}

my_mod.0002 = { hidden = yes }
  other_ns.12 = {
	immediate = { x = { y = 1 } }
}
";
        assert_eq!(event_ids(text), vec!["my_mod.0001", "my_mod.0002", "other_ns.12"]);
        assert!(event_ids("a = { b.c = { } }").is_empty());
    }

    #[test]
    fn the_events_of_a_mod_come_with_their_files() {
        let tmp = TempDir::new();
        let dir = tmp.join("mod");
        fs::create_dir_all(dir.join("events/sub")).unwrap();
        fs::write(dir.join("events/a.txt"), "ns.1 = { }\nns.2 = { }\n").unwrap();
        fs::write(dir.join("events/sub/b.txt"), "ns2.1 = { }\n").unwrap();
        fs::write(dir.join("events/readme.md"), "ns.99 = { }\n").unwrap();
        assert_eq!(
            mod_events(&dir),
            vec![
                ("ns.1".to_owned(), "events/a.txt".to_owned()),
                ("ns.2".to_owned(), "events/a.txt".to_owned()),
                ("ns2.1".to_owned(), "events/sub/b.txt".to_owned()),
            ]
        );
        assert!(mod_events(&tmp.join("nothing")).is_empty());
    }

    #[test]
    fn undefined_scopes_are_read_from_the_new_errors_once_each() {
        let log = "\
[10:00:00][E][jomini_script_system.cpp:304]: Script system error!
  Error: Undefined event target 'friend'
  Script location: file: events/e.txt line: 18 (ns.0002:immediate)

[10:00:00][E][jomini_script_system.cpp:304]: Script system error!
  Error: Undefined event target 'friend'
  Script location: file: events/e.txt line: 19 (ns.0002:immediate)

[10:00:00][E][jomini_script_system.cpp:304]: Script system error!
  Error: Undefined event target 'child_one'
  Script location: file: events/e.txt line: 20 (ns.0002:immediate)

[10:00:00][E][jomini_script_system.cpp:304]: Script system error!
  Error: Event target link 'scope' returned an unset scope

[10:00:00][E][pdx_data_factory.cpp:1364]: Failed to find type 'kid' in 'kid.GetFirstName'.
[10:00:00][E][pdx_data_factory.cpp:1364]: Failed to find type 'kid' in 'kid.GetHerHis'.
[10:00:00][E][pdx_data_factory.cpp:1364]: Failed to find type 'Character' in 'Character.GetName'.
[10:00:00][E][pdx_data_factory.cpp:1364]: Failed to find type 'root' in 'root.GetName'.
";
        let errors = filter(parse_entries(log), &HashSet::new());
        assert_eq!(missing_scopes(&errors), vec!["friend", "child_one", "kid"]);
        assert!(missing_scopes(&Filtered::default()).is_empty());
    }

    #[test]
    fn a_missing_scope_is_guessed_from_its_name() {
        for (name, spec) in [
            ("friend", "character:friend"),
            ("target", "character:target"),
            ("child", "child:child"),
            ("new_son", "child:new_son"),
            ("guest_of_honour", "courtier:guest_of_honour"),
            ("my_title", "title:my_title"),
            ("home_county", "county:home_county"),
            ("province", "capital:province"),
            ("new_faith", "faith:new_faith"),
            ("old_culture", "culture:old_culture"),
            ("level", "value:level=1"),
            ("actor", "self:actor"),
        ] {
            assert_eq!(guess_fixture(name), spec);
            // Whatever is guessed is a fixture that exists.
            assert!(fixture_effects(&guess_fixture(name)).is_ok(), "{name}");
        }
        assert_eq!(fixture_name("character:friend"), "friend");
        assert_eq!(fixture_name("value:n=3"), "n");
    }

    #[test]
    fn events_are_checked_before_any_is_fired() {
        let loc = Locations::detect();
        assert!(
            run_events(&loc, &[], &[], 1000, false, Shot::OFF, &|| false)
                .unwrap_err()
                .contains("empty")
        );
        let err = run_events(&loc, &["a.1} x".to_owned()], &[], 1000, false, Shot::OFF, &|| false)
            .unwrap_err();
        assert!(err.contains("namespace.number"), "{err}");
        let err = run_events(
            &loc,
            &["a.1".to_owned()],
            &["wizard:w".to_owned()],
            1000,
            false,
            Shot::OFF,
            &|| false,
        )
        .unwrap_err();
        assert!(err.contains("event a.1") && err.contains("unknown kind"), "{err}");
    }
}
