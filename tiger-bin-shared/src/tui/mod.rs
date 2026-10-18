//! The full-screen display of the automatic program: a window with panels instead of lines that
//! scroll away. It finds the folders, lists the mods, shows the check while it runs and then lets
//! the person look through the causes, the files and every report.
//!
//! It is only used when a person is at a terminal. With `--plain`, `XTIGER_ASCII=1`, a redirected
//! input or output, or a terminal that cannot be switched, the line-by-line program runs instead.

mod data;
mod draw;
mod sprite;
mod theme;

use std::cell::Cell;
use std::fmt;
use std::io::{IsTerminal, Stdout, stderr, stdin, stdout};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::text::Line;
use tiger_lib::Severity;

use self::data::{Cause, FileCount, Report, causes, files};
use self::theme::Theme;
use crate::GameConsts;
use crate::auto::{Outcome, Progress, Step, VERSION, check, prepare, relaunch};
use crate::firstrun::{self, Reject};
use crate::pick::{LocalMod, local_mods, mod_from_path};
use crate::progress::{capture_notes, note, release_notes, take_notes};

type Screen = Terminal<CrosstermBackend<Stdout>>;

/// The terminal could not be switched to the full-screen display. Nothing was drawn, so the
/// caller can use the line-by-line program instead.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Unavailable;

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the terminal cannot show the full-screen display")
    }
}

impl std::error::Error for Unavailable {}

/// Whether the full-screen display should be used: a person is at a terminal and has not asked for
/// the plain program.
pub(crate) fn wanted() -> bool {
    std::env::var_os("XTIGER_ASCII").is_none()
        && !std::env::args().any(|a| a == "--plain")
        && stdin().is_terminal()
        && stdout().is_terminal()
        && stderr().is_terminal()
        && std::env::var("TERM").map_or(true, |term| term != "dumb")
}

/// What a panic in the thread that checks the mod said. The screen cannot show the usual message.
static PANIC: Mutex<Option<String>> = Mutex::new(None);

fn enter() -> std::io::Result<Screen> {
    enable_raw_mode()?;
    if let Err(e) = execute!(stdout(), EnterAlternateScreen) {
        _ = disable_raw_mode();
        return Err(e);
    }
    Terminal::new(CrosstermBackend::new(stdout()))
}

fn leave() {
    _ = execute!(stdout(), LeaveAlternateScreen, Show);
    _ = disable_raw_mode();
}

/// Put the terminal back if the program panics, and keep a panic of the checking thread from
/// writing over the screen.
fn guard_panics() {
    let main = thread::current().id();
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if thread::current().id() == main {
            leave();
            previous(info);
        } else if let Ok(mut held) = PANIC.lock() {
            *held = Some(info.to_string());
        }
    }));
}

/// Run the program in the full-screen display.
pub(crate) fn run(consts: &'static GameConsts) -> Result<()> {
    let mut screen = enter().map_err(|_| anyhow::Error::new(Unavailable))?;
    guard_panics();
    capture_notes();
    tiger_lib::set_note_hook(note);

    let mut app = App::new(consts);
    let result = app.event_loop(&mut screen);
    leave();
    release_notes();
    result?;

    // What stays in the window afterwards, for whoever scrolls back.
    if let Page::Result(view) = &app.screen {
        eprintln!("{}", view.summary_line());
        eprintln!("Full report  {}", view.outcome.report.display());
    }
    crate::pause::finished();
    Ok(())
}

/// The folder the person gave, or the one that was found, and how.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) game: Option<(PathBuf, String)>,
    pub(crate) paradox: Option<(PathBuf, String)>,
    pub(crate) workshop: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wanted {
    Game,
    Paradox,
}

/// The screen that asks for a folder that could not be found.
#[derive(Debug)]
pub(crate) struct Ask {
    pub(crate) wanted: Wanted,
    pub(crate) input: String,
    pub(crate) error: Option<String>,
    /// The person is changing a folder that was found: Escape goes back, and the program starts
    /// again once the new one is taken.
    pub(crate) change: bool,
}

/// The list of mods to pick from.
#[derive(Debug)]
pub(crate) struct Mods {
    pub(crate) all: Vec<LocalMod>,
    /// What was typed: part of a name, or a pasted path.
    pub(crate) typed: String,
    /// The mods that match, as indices into `all`.
    pub(crate) shown: Vec<usize>,
    /// The typed text is a path to a mod.
    pub(crate) pasted: Option<PathBuf>,
    pub(crate) at: usize,
}

impl Mods {
    fn new(paradox: &Path) -> Mods {
        let all = local_mods(paradox);
        let mut mods = Mods { all, typed: String::new(), shown: Vec::new(), pasted: None, at: 0 };
        mods.refilter();
        mods
    }

    fn looks_like_path(&self) -> bool {
        self.typed.contains(['\\', '/', ':'])
    }

    fn refilter(&mut self) {
        self.pasted = if self.looks_like_path() { mod_from_path(&self.typed) } else { None };
        let needle = self.typed.trim().to_lowercase();
        self.shown = if self.looks_like_path() || needle.is_empty() {
            (0..self.all.len()).collect()
        } else {
            (0..self.all.len())
                .filter(|&i| {
                    let m = &self.all[i];
                    m.name.to_lowercase().contains(&needle) || m.id.to_lowercase().contains(&needle)
                })
                .collect()
        };
        if self.looks_like_path() {
            self.shown.clear();
        }
        self.at = self.at.min(self.shown.len().saturating_sub(1));
    }

    /// The mod under the cursor, or the pasted one.
    pub(crate) fn selected(&self) -> Option<(PathBuf, String)> {
        if let Some(path) = &self.pasted {
            let name = path
                .parent()
                .and_then(Path::file_name)
                .map_or_else(|| "that mod".to_string(), |n| n.to_string_lossy().to_string());
            return Some((path.clone(), name));
        }
        let m = &self.all[*self.shown.get(self.at)?];
        Some((m.path.clone(), m.name.clone()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepState {
    Waiting,
    Running(Instant),
    Done(Duration),
}

/// What the thread that checks the mod sends to the screen.
enum Msg {
    Rows(Vec<(&'static str, String)>),
    Step(Progress),
    Done(Result<Finished, String>),
}

struct Finished {
    outcome: Outcome,
    reports: Vec<Report>,
}

/// The screen while the mod is being checked.
#[derive(Debug)]
pub(crate) struct Running {
    rx: Receiver<Msg>,
    pub(crate) name: String,
    pub(crate) rows: Vec<(&'static str, String)>,
    pub(crate) steps: [StepState; 3],
    pub(crate) notes: Vec<String>,
    pub(crate) started: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Causes,
    Files,
    Reports,
}

impl Tab {
    pub(crate) const ALL: [Tab; 3] = [Tab::Causes, Tab::Files, Tab::Reports];

    fn next(self) -> Tab {
        match self {
            Tab::Causes => Tab::Files,
            Tab::Files => Tab::Reports,
            Tab::Reports => Tab::Causes,
        }
    }

    fn previous(self) -> Tab {
        match self {
            Tab::Causes => Tab::Reports,
            Tab::Files => Tab::Causes,
            Tab::Reports => Tab::Files,
        }
    }
}

/// Which reports the list of reports shows, by how bad they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Floor {
    All,
    Warnings,
    Errors,
}

impl Floor {
    fn least(self) -> Severity {
        match self {
            Floor::All => Severity::Tips,
            Floor::Warnings => Severity::Warning,
            Floor::Errors => Severity::Error,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Floor::All => "all of them",
            Floor::Warnings => "warnings and worse",
            Floor::Errors => "errors only",
        }
    }
}

/// A place in a list, and how far the list is scrolled.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Cursor {
    pub(crate) at: usize,
}

impl Cursor {
    /// Move the cursor in a list of `len` rows. Returns whether the key was one for moving.
    fn go(&mut self, key: KeyCode, len: usize) -> bool {
        let last = len.saturating_sub(1);
        self.at = match key {
            KeyCode::Up | KeyCode::Char('k') => self.at.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (self.at + 1).min(last),
            KeyCode::PageUp => self.at.saturating_sub(10),
            KeyCode::PageDown => (self.at + 10).min(last),
            KeyCode::Home => 0,
            KeyCode::End => last,
            _ => return false,
        };
        true
    }
}

/// The screen with the result of the check.
#[derive(Debug)]
pub(crate) struct Results {
    pub(crate) outcome: Outcome,
    pub(crate) reports: Vec<Report>,
    pub(crate) causes: Vec<Cause>,
    pub(crate) files: Vec<FileCount>,
    pub(crate) tab: Tab,
    pub(crate) cause: Cursor,
    pub(crate) file: Cursor,
    pub(crate) report: Cursor,
    pub(crate) only_key: Option<String>,
    pub(crate) only_file: Option<String>,
    pub(crate) floor: Floor,
    /// The reports the list of reports shows, as indices into `reports`.
    pub(crate) shown: Vec<usize>,
    /// How far the text on the right is scrolled.
    pub(crate) scroll: u16,
    /// How far the text can be scrolled, as the last drawing found out.
    pub(crate) max_scroll: Cell<u16>,
}

impl Results {
    fn new(done: Finished) -> Results {
        let Finished { outcome, reports } = done;
        let mut view = Results {
            causes: causes(&reports),
            files: files(&reports),
            outcome,
            reports,
            tab: Tab::Causes,
            cause: Cursor::default(),
            file: Cursor::default(),
            report: Cursor::default(),
            only_key: None,
            only_file: None,
            floor: Floor::All,
            shown: Vec::new(),
            scroll: 0,
            max_scroll: Cell::new(0),
        };
        view.refilter();
        view
    }

    fn refilter(&mut self) {
        let least = self.floor.least();
        self.shown = self
            .reports
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.severity >= least
                    && self.only_key.as_ref().is_none_or(|k| &r.key == k)
                    && self.only_file.as_ref().is_none_or(|f| r.file() == Some(f.as_str()))
            })
            .map(|(i, _)| i)
            .collect();
        self.report = Cursor::default();
        self.scroll = 0;
    }

    pub(crate) fn current_report(&self) -> Option<&Report> {
        self.reports.get(*self.shown.get(self.report.at)?)
    }

    fn summary_line(&self) -> String {
        let stats = &self.outcome.stats;
        let mut parts = Vec::new();
        for (severity, one, many) in [
            (Severity::Fatal, "fatal", "fatal"),
            (Severity::Error, "error", "errors"),
            (Severity::Warning, "warning", "warnings"),
            (Severity::Untidy, "untidy", "untidy"),
            (Severity::Tips, "tip", "tips"),
        ] {
            let n = stats.count(severity);
            if n > 0 {
                parts.push(format!("{n} {}", if n == 1 { one } else { many }));
            }
        }
        let name = self.outcome.rows.get(1).map_or("", |(_, name)| name.as_str());
        if parts.is_empty() {
            format!("{name}: no problems found.")
        } else {
            format!("{name}: {}.", parts.join(", "))
        }
    }
}

/// The screen on show.
#[derive(Debug)]
pub(crate) enum Page {
    Ask(Ask),
    Mods(Mods),
    Running(Running),
    Result(Box<Results>),
}

/// What has to wait until the terminal is the person's again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Update,
    Relaunch,
}

/// What a key asked for, once the screen has looked at it.
enum Do {
    Nothing,
    Quit,
    Check(PathBuf, String),
    Submit(String),
    Cancel,
    Folder,
    Update,
    Another,
    Open(PathBuf),
    Help,
}

pub(crate) struct App {
    pub(crate) consts: &'static GameConsts,
    pub(crate) theme: Theme,
    pub(crate) sprite: Vec<Line<'static>>,
    pub(crate) screen: Page,
    pub(crate) found: Found,
    pub(crate) started: Instant,
    /// A line of news for the footer, and when it was said.
    pub(crate) status: Option<(String, Instant)>,
    /// Something went wrong; it is shown on top of the next screen until a key is pressed.
    pub(crate) problem: Option<String>,
    pub(crate) help: bool,
    quit: bool,
    pending: Option<Pending>,
}

impl App {
    fn new(consts: &'static GameConsts) -> App {
        let found = Found {
            game: firstrun::find_game(consts).map(|(p, how)| (p, how.to_string())),
            paradox: firstrun::find_paradox(consts).map(|(p, how)| (p, how.to_string())),
            workshop: None,
        };
        let mut app = App {
            consts,
            theme: Theme::detect(),
            sprite: sprite::qubis(),
            screen: Page::Mods(Mods::new(Path::new(""))),
            found,
            started: Instant::now(),
            status: None,
            problem: None,
            help: false,
            quit: false,
            pending: None,
        };
        app.screen = app.next_screen();
        app
    }

    /// What comes once the folders are known: the mods, or the question about what is missing.
    fn next_screen(&mut self) -> Page {
        if self.found.game.is_none() {
            return Page::Ask(Ask::new(Wanted::Game, false));
        }
        let Some((paradox, _)) = &self.found.paradox else {
            return Page::Ask(Ask::new(Wanted::Paradox, false));
        };
        if let Some((game, _)) = &self.found.game {
            self.found.workshop = firstrun::find_workshop(self.consts, game);
        }
        Page::Mods(Mods::new(paradox))
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some((text.into(), Instant::now()));
    }

    fn event_loop(&mut self, screen: &mut Screen) -> Result<()> {
        loop {
            self.pump();
            screen.draw(|frame| draw::frame(frame, self))?;
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind != KeyEventKind::Release
            {
                self.key(key);
            }
            if self.quit {
                return Ok(());
            }
            match self.pending.take() {
                Some(Pending::Update) => {
                    leave();
                    release_notes();
                    update_in_plain_mode();
                    capture_notes();
                    *screen = enter()?;
                }
                Some(Pending::Relaunch) => {
                    leave();
                    release_notes();
                    // Only comes back when the new program could not be started.
                    relaunch();
                    capture_notes();
                    *screen = enter()?;
                    self.problem = Some("The program could not be started again.".to_string());
                }
                None => {}
            }
        }
    }

    /// Take in what the checking thread has sent.
    fn pump(&mut self) {
        let Page::Running(run) = &mut self.screen else {
            return;
        };
        run.notes.extend(take_notes());
        let mut finished = None;
        loop {
            match run.rx.try_recv() {
                Ok(Msg::Rows(rows)) => run.rows = rows,
                Ok(Msg::Step(Progress::Started(step))) => {
                    run.steps[step_index(step)] = StepState::Running(Instant::now());
                }
                Ok(Msg::Step(Progress::Finished(step))) => {
                    let slot = &mut run.steps[step_index(step)];
                    if let StepState::Running(since) = *slot {
                        *slot = StepState::Done(since.elapsed());
                    }
                }
                Ok(Msg::Done(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let panic = PANIC.lock().ok().and_then(|mut held| held.take());
                    finished = Some(Err(panic.map_or_else(
                        || "The check stopped unexpectedly.".to_string(),
                        |p| format!("The check stopped unexpectedly: {p}"),
                    )));
                    break;
                }
            }
        }
        run.notes.extend(take_notes());
        match finished {
            Some(Ok(done)) => self.screen = Page::Result(Box::new(Results::new(done))),
            Some(Err(message)) => {
                self.problem = Some(message);
                self.screen = self.next_screen();
            }
            None => {}
        }
    }

    fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        self.problem = None;
        let action = match &mut self.screen {
            Page::Ask(ask) => key_ask(ask, key),
            Page::Mods(mods) => key_mods(mods, key),
            Page::Running(_) => Do::Nothing,
            Page::Result(view) => key_results(view, key),
        };
        self.act(action);
    }

    fn act(&mut self, action: Do) {
        match action {
            Do::Nothing => {}
            Do::Quit => self.quit = true,
            Do::Help => self.help = true,
            Do::Check(path, name) => self.begin(path, name),
            Do::Submit(text) => self.submit(&text),
            Do::Cancel => self.screen = self.next_screen(),
            Do::Folder => self.screen = Page::Ask(Ask::new(Wanted::Game, true)),
            Do::Update => self.pending = Some(Pending::Update),
            Do::Another => self.pending = Some(Pending::Relaunch),
            Do::Open(path) => {
                if path.is_file() && open_quietly(&path) {
                    self.say("Opened it.");
                } else {
                    self.say("Could not open it. Open the file by hand.");
                }
            }
        }
    }

    /// The person typed a folder.
    fn submit(&mut self, text: &str) {
        let Page::Ask(ask) = &mut self.screen else {
            return;
        };
        let (wanted, change) = (ask.wanted, ask.change);
        let result = match wanted {
            Wanted::Game => firstrun::accept_game(self.consts, text),
            Wanted::Paradox => firstrun::accept_paradox(self.consts, text),
        };
        match result {
            Ok(path) => {
                let entry = Some((path, "the folder you gave".to_string()));
                match wanted {
                    Wanted::Game => self.found.game = entry,
                    Wanted::Paradox => self.found.paradox = entry,
                }
                if change {
                    self.pending = Some(Pending::Relaunch);
                } else {
                    self.screen = self.next_screen();
                }
            }
            Err(Reject::Empty) => ask.error = Some("Paste a folder first.".to_string()),
            Err(reject) => ask.error = Some(reject.describe()),
        }
    }

    /// Start checking a mod in a thread of its own, so the screen keeps moving.
    fn begin(&mut self, target: PathBuf, name: String) {
        let (Some((game, _)), Some((paradox, _))) = (&self.found.game, &self.found.paradox) else {
            return;
        };
        let (game, paradox, workshop) =
            (game.clone(), paradox.clone(), self.found.workshop.clone());
        let consts = self.consts;
        let (tx, rx) = channel();
        let work = move || {
            let result = (|| -> Result<Finished> {
                let prepared = prepare(consts, &target, &paradox.join("logs"))?;
                _ = tx.send(Msg::Rows(prepared.rows.clone()));
                let outcome =
                    check(&game, workshop.as_deref(), Some(&paradox), prepared, &mut |progress| {
                        _ = tx.send(Msg::Step(progress));
                    })?;
                Ok(Finished { outcome, reports: data::collect() })
            })();
            _ = tx.send(Msg::Done(result.map_err(|e| format!("{e:#}"))));
        };
        // The validator recurses deeply in places; give it the room the main thread would have.
        let spawned = thread::Builder::new()
            .name("check".to_string())
            .stack_size(256 * 1024 * 1024)
            .spawn(work);
        if let Err(e) = spawned {
            self.problem = Some(format!("Could not start the check: {e}"));
            return;
        }
        self.screen = Page::Running(Running {
            rx,
            name,
            rows: Vec::new(),
            steps: [StepState::Waiting; 3],
            notes: Vec::new(),
            started: Instant::now(),
        });
    }
}

impl Ask {
    fn new(wanted: Wanted, change: bool) -> Ask {
        Ask { wanted, input: String::new(), error: None, change }
    }
}

fn step_index(step: Step) -> usize {
    Step::ALL.iter().position(|&s| s == step).unwrap_or(0)
}

/// The keys that type text: no control or alt, which are for shortcuts.
fn typed(key: &KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char(c)
            if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(c)
        }
        _ => None,
    }
}

fn key_ask(ask: &mut Ask, key: KeyEvent) -> Do {
    if let Some(c) = typed(&key) {
        ask.input.push(c);
        ask.error = None;
        return Do::Nothing;
    }
    match key.code {
        KeyCode::Backspace => {
            ask.input.pop();
            ask.error = None;
            Do::Nothing
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ask.input.clear();
            Do::Nothing
        }
        KeyCode::Tab | KeyCode::BackTab if ask.change => {
            ask.wanted = match ask.wanted {
                Wanted::Game => Wanted::Paradox,
                Wanted::Paradox => Wanted::Game,
            };
            ask.input.clear();
            ask.error = None;
            Do::Nothing
        }
        KeyCode::Enter => Do::Submit(ask.input.clone()),
        KeyCode::Esc if ask.change => Do::Cancel,
        KeyCode::Esc => Do::Quit,
        _ => Do::Nothing,
    }
}

fn key_mods(mods: &mut Mods, key: KeyEvent) -> Do {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('g') => Do::Folder,
            KeyCode::Char('u') => Do::Update,
            _ => Do::Nothing,
        };
    }
    if let Some(c) = typed(&key) {
        mods.typed.push(c);
        mods.refilter();
        return Do::Nothing;
    }
    let len = mods.shown.len();
    let mut cursor = Cursor { at: mods.at };
    let moved = match key.code {
        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
            cursor.go(key.code, len)
        }
        KeyCode::Home | KeyCode::End if mods.typed.is_empty() => cursor.go(key.code, len),
        _ => false,
    };
    if moved {
        mods.at = cursor.at;
        return Do::Nothing;
    }
    match key.code {
        KeyCode::Backspace => {
            mods.typed.pop();
            mods.refilter();
            Do::Nothing
        }
        KeyCode::Enter => mods.selected().map_or(Do::Nothing, |(path, name)| Do::Check(path, name)),
        KeyCode::Esc if mods.typed.is_empty() => Do::Quit,
        KeyCode::Esc => {
            mods.typed.clear();
            mods.refilter();
            Do::Nothing
        }
        KeyCode::F(1) => Do::Help,
        _ => Do::Nothing,
    }
}

fn key_results(view: &mut Results, key: KeyEvent) -> Do {
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Up => {
                view.scroll = view.scroll.saturating_sub(2).min(view.max_scroll.get());
                return Do::Nothing;
            }
            KeyCode::Down => {
                view.scroll = view.scroll.saturating_add(2).min(view.max_scroll.get());
                return Do::Nothing;
            }
            _ => {}
        }
    }
    let before = (view.cause.at, view.file.at, view.report.at);
    let moved = match view.tab {
        Tab::Causes => view.cause.go(key.code, view.causes.len()),
        Tab::Files => view.file.go(key.code, view.files.len()),
        Tab::Reports => view.report.go(key.code, view.shown.len()),
    };
    if moved {
        if before != (view.cause.at, view.file.at, view.report.at) {
            view.scroll = 0;
        }
        return Do::Nothing;
    }
    match key.code {
        KeyCode::Tab | KeyCode::Right => switch(view, view.tab.next()),
        KeyCode::BackTab | KeyCode::Left => switch(view, view.tab.previous()),
        KeyCode::Char('1') => switch(view, Tab::Causes),
        KeyCode::Char('2') => switch(view, Tab::Files),
        KeyCode::Char('3') => switch(view, Tab::Reports),
        KeyCode::Enter => drill(view),
        KeyCode::Esc | KeyCode::Backspace => {
            if view.only_key.is_some() || view.only_file.is_some() {
                view.only_key = None;
                view.only_file = None;
                view.refilter();
            }
        }
        KeyCode::Char('s') => {
            view.floor = match view.floor {
                Floor::All if view.outcome.stats.at_least(Severity::Warning) > 0 => Floor::Warnings,
                Floor::All | Floor::Warnings
                    if view.outcome.stats.at_least(Severity::Error) > 0 =>
                {
                    Floor::Errors
                }
                _ => Floor::All,
            };
            view.refilter();
            view.tab = Tab::Reports;
        }
        KeyCode::Char('o') => return Do::Open(view.outcome.report.clone()),
        KeyCode::Char('e') => {
            let path = view
                .current_report()
                .and_then(|r| r.locations.first())
                .and_then(|l| l.fullpath.as_deref().map(PathBuf::from).filter(|p| p.is_file()));
            if let Some(path) = path.filter(|_| view.tab == Tab::Reports) {
                return Do::Open(path);
            }
        }
        KeyCode::Char('m') => return Do::Another,
        KeyCode::Char('g') => return Do::Folder,
        KeyCode::Char('u') => return Do::Update,
        KeyCode::Char('q') => return Do::Quit,
        KeyCode::Char('?') | KeyCode::F(1) => return Do::Help,
        _ => {}
    }
    Do::Nothing
}

fn switch(view: &mut Results, tab: Tab) {
    view.tab = tab;
    view.scroll = 0;
}

/// Enter: look at the reports of the cause or the file under the cursor.
fn drill(view: &mut Results) {
    match view.tab {
        Tab::Causes => {
            if let Some(cause) = view.causes.get(view.cause.at) {
                view.only_key = Some(cause.key.clone());
                view.only_file = None;
            }
        }
        Tab::Files => {
            if let Some(file) = view.files.get(view.file.at) {
                view.only_file = Some(file.path.clone());
                view.only_key = None;
            }
        }
        Tab::Reports => return,
    }
    view.refilter();
    view.tab = Tab::Reports;
}

/// Leave the full-screen display for the update, which prints lines and a progress bar.
fn update_in_plain_mode() {
    eprintln!();
    if let Err(e) = crate::update::update(VERSION, None, false) {
        eprintln!("{e}");
    }
    crate::menu::wait_for_enter("Press Enter to go back.");
}

/// Open a file with whatever the system uses for it, without touching this window.
fn open_quietly(path: &Path) -> bool {
    #[cfg(windows)]
    let opened = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    #[cfg(not(windows))]
    let opened = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    opened.is_ok()
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use std::path::PathBuf;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::{
        App, Ask, Cursor, Do, Finished, Floor, Found, Mods, Page, Results, Running, StepState, Tab,
        Wanted, data, draw, key_ask, sprite, theme::Theme,
    };
    use crate::GameConsts;
    use crate::auto::Outcome;
    use crate::pick::LocalMod;

    static CONSTS: GameConsts = GameConsts {
        name: "Test Game",
        name_short: "test",
        version: "1.0.0",
        app_id: 1,
        signature_file: "x",
        paradox_dir: "x",
    };

    fn app(screen: Page) -> App {
        App {
            consts: &CONSTS,
            theme: Theme::detect(),
            sprite: sprite::qubis(),
            screen,
            found: Found {
                game: Some((PathBuf::from("C:/Games/Test"), "found it".to_string())),
                paradox: Some((PathBuf::from("C:/Users/x/Documents/Test"), "found it".to_string())),
                workshop: None,
            },
            started: Instant::now(),
            status: None,
            problem: None,
            help: false,
            quit: false,
            pending: None,
        }
    }

    fn results(json: &str) -> Page {
        let outcome = Outcome {
            rows: vec![("Game", "Test".to_string()), ("Mod", "A Mod  (123)".to_string())],
            stats: tiger_lib::ReportStats::default(),
            report: PathBuf::from("C:/Users/x/logs/report.log"),
        };
        let reports = data::parse(json.as_bytes());
        Page::Result(Box::new(Results::new(Finished { outcome, reports })))
    }

    const REPORTS: &str = r#"[
        {"severity":"error","key":"missing-item","message":"Gene template not found",
         "locations":[{"path":"common/a.txt","linenr":10,"column":25,"length":4,"line":"  gene = { x }"}]},
        {"severity":"warning","key":"logic","message":"a very long message that has to wrap around \
the narrow detail pane without ever overflowing it or panicking at any size",
         "info":"some more information","locations":[]}
    ]"#;

    fn pages() -> Vec<Page> {
        let mut with_mods = Mods::new(std::path::Path::new(""));
        with_mods.all = vec![
            LocalMod {
                path: PathBuf::from("a.mod"),
                name: "Short".to_string(),
                id: "1".to_string(),
            },
            LocalMod {
                path: PathBuf::from("b.mod"),
                name: "A very long name that cannot possibly fit in a narrow column".to_string(),
                id: "a_long_identifier_for_a_local_mod".to_string(),
            },
        ];
        with_mods.refilter();
        let (_tx, rx) = channel();
        let running = Running {
            rx,
            name: "A mod".to_string(),
            rows: vec![("Game", "Test".to_string()), ("Mod", "A mod".to_string())],
            steps: [
                StepState::Done(std::time::Duration::from_secs(1)),
                StepState::Waiting,
                StepState::Waiting,
            ],
            notes: vec!["a note".to_string(); 9],
            started: Instant::now(),
        };
        let mut pages = vec![
            Page::Ask(Ask::new(Wanted::Game, false)),
            Page::Ask(Ask::new(Wanted::Game, true)),
            Page::Ask(Ask::new(Wanted::Paradox, true)),
            Page::Mods(with_mods),
            Page::Running(running),
            results("[]"),
        ];
        for tab in Tab::ALL {
            let Page::Result(mut view) = results(REPORTS) else { unreachable!() };
            view.tab = tab;
            pages.push(Page::Result(view));
        }
        pages
    }

    /// Every screen has to draw at any size, the tiny ones included, without panicking.
    #[test]
    fn every_screen_draws_at_every_size() {
        let sizes = [
            (1, 1),
            (10, 3),
            (40, 10),
            (63, 17),
            (64, 18),
            (80, 24),
            (99, 30),
            (100, 30),
            (240, 70),
        ];
        for (width, height) in sizes {
            for index in 0..pages().len() {
                for help in [false, true] {
                    let mut app = app(pages().remove(index));
                    app.help = help;
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal.draw(|frame| draw::frame(frame, &app)).unwrap();
                }
            }
        }
    }

    #[test]
    fn the_cursor_stays_in_the_list() {
        let mut c = Cursor::default();
        assert!(c.go(KeyCode::Up, 5));
        assert_eq!(c.at, 0);
        assert!(c.go(KeyCode::End, 5));
        assert_eq!(c.at, 4);
        assert!(c.go(KeyCode::Down, 5));
        assert_eq!(c.at, 4);
        assert!(c.go(KeyCode::PageUp, 5));
        assert_eq!(c.at, 0);
        assert!(!c.go(KeyCode::Enter, 5));
        // An empty list has nowhere to go.
        let mut empty = Cursor::default();
        assert!(empty.go(KeyCode::Down, 0));
        assert_eq!(empty.at, 0);
    }

    #[test]
    fn tab_swaps_the_folder_only_when_changing_one() {
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        let mut change = Ask::new(Wanted::Game, true);
        change.input = "typed".to_string();
        assert!(matches!(key_ask(&mut change, tab), Do::Nothing));
        assert_eq!(change.wanted, Wanted::Paradox);
        assert!(change.input.is_empty());
        let mut first_run = Ask::new(Wanted::Game, false);
        key_ask(&mut first_run, tab);
        assert_eq!(first_run.wanted, Wanted::Game);
    }

    #[test]
    fn tabs_go_round() {
        assert_eq!(Tab::Reports.next(), Tab::Causes);
        assert_eq!(Tab::Causes.previous(), Tab::Reports);
        assert_eq!(Floor::Errors.label(), "errors only");
    }
}
