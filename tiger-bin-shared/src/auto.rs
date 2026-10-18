use std::fs::{File, create_dir_all};
use std::io::{IsTerminal, stdin};
use std::mem::forget;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};
use console::{Term, measure_text_width};
use tiger_lib::ModFile;
use tiger_lib::{
    EmitOptions, Everything, OutputFormat, ReportStats, Severity, config_reports_pending,
    emit_reports, emit_reports_with, enable_ansi_colors,
};

use crate::GameConsts;
use crate::brand;
use crate::explain::explain_for;
use crate::firstrun::{self, Folders};
use crate::guide::{self, Guide, causes};
use crate::menu::{select, wait_for_enter};
use crate::pause;
use crate::pick::{choose, local_mods, mod_from_path};
use crate::progress::{Stage, header, note, style};

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A window at least this tall has room for Qubis on the first screen.
const TALL_FIRST: u16 = 30;
/// The result screen is longer, so it needs a taller window for the same.
const TALL_RESULT: u16 = 46;
/// The screens that are full of text never have room for Qubis.
const NEVER: u16 = u16::MAX;

/// Run the automatic version of the tiger application.
///
/// It looks for the game and the Paradox folder, asks for them when it cannot find them, lists the
/// mods in the mod folder for the user to pick, and writes the report next to the game's logs. Then
/// it offers a menu, so the window stays until the user chooses to finish. It has **no** command
/// line arguments and hence less customizable compared to the main application.
pub fn run(game_consts: &'static GameConsts) -> Result<()> {
    if crate::tui::wanted() {
        match crate::tui::run(game_consts) {
            Err(e) if e.is::<crate::tui::Unavailable>() => {}
            done => return done,
        }
    }
    let &GameConsts { name, version, .. } = game_consts;

    // Colors are off by default, but enable ANSI support in case the config file turns colors on again.
    #[cfg(windows)]
    let _ = ansiterm::enable_ansi_support().map_err(|_| {
        eprintln!("Failed to enable ANSI support for Windows10 users. Continuing anyway.");
    });
    tiger_lib::set_note_hook(note);

    // Colors are off by default but ANSI is on, so the banner is the one place that may use them.
    brand::screen(VERSION, TALL_FIRST);

    eprintln!("This validator was made for {name} version {version}.");
    eprintln!("If you are using a newer version of {name}, it may be inaccurate.");

    let Folders { game, workshop, paradox } = firstrun::welcome(game_consts)?;
    let pdxlogs = paradox.join("logs");

    eprintln!();
    let mods = local_mods(&paradox);
    let modfile = if mods.is_empty() { ask_mod(&paradox)? } else { choose(&mods)? };
    let outcome =
        validate_mod(game_consts, &game, workshop.as_deref(), Some(&paradox), &modfile, &pdxlogs)?;

    if !stdin().is_terminal() {
        return Ok(());
    }
    menu(game_consts, &outcome)
}

/// There are no mods in the mod folder: let the user point at one.
fn ask_mod(paradox: &Path) -> Result<PathBuf> {
    eprintln!("There are no mods in {}", paradox.join("mod").display());
    eprintln!();
    eprintln!("Paste the folder of the mod you want to check (the one with descriptor.mod");
    eprintln!("inside), or the path of its .mod file.");
    if !stdin().is_terminal() {
        bail!("Did not find any mods to validate.");
    }
    loop {
        eprint!("\n{} ", style("Mod (Enter alone to quit):").bold());
        let mut line = String::new();
        if stdin().read_line(&mut line).unwrap_or(0) == 0 {
            bail!("No mod was given.");
        }
        if line.trim().is_empty() {
            bail!("No mod was given.");
        }
        if let Some(path) = mod_from_path(&line) {
            return Ok(path);
        }
        eprintln!(
            "  {} I cannot find a mod there. Paste the folder or the .mod file again.",
            style("!").red().bold()
        );
    }
}

/// What a finished check leaves behind for the menu.
#[derive(Debug)]
pub(crate) struct Outcome {
    pub(crate) rows: Vec<(&'static str, String)>,
    pub(crate) stats: ReportStats,
    pub(crate) report: PathBuf,
}

/// The three steps of a check, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Reading,
    Understanding,
    Checking,
}

impl Step {
    pub(crate) const ALL: [Step; 3] = [Step::Reading, Step::Understanding, Step::Checking];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Step::Reading => "Reading the game files and your mod",
            Step::Understanding => "Understanding your scripts",
            Step::Checking => "Checking everything",
        }
    }
}

/// What a check tells whoever is watching it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Progress {
    Started(Step),
    Finished(Step),
}

/// A mod that was read and is ready to be checked.
#[derive(Debug)]
pub(crate) struct Prepared {
    pub(crate) rows: Vec<(&'static str, String)>,
    modfile: ModFile,
    modpath: PathBuf,
    report: PathBuf,
}

/// Read the `.mod` file and work out where the mod is and where the report goes.
pub(crate) fn prepare(consts: &GameConsts, modpath: &Path, logdir: &Path) -> Result<Prepared> {
    let modfile = ModFile::read(modpath)?;
    let folder = modfile.modpath();
    if !folder.is_dir() {
        bail!(
            "Cannot find the mod directory {}. Please make sure the .mod file is correct.",
            folder.display()
        );
    }
    let id = folder.file_name().map_or_else(String::new, |n| n.to_string_lossy().to_string());
    let shown = match modfile.display_name() {
        Some(n) if n != id => format!("{n}  ({id})"),
        Some(n) => n,
        None => id.clone(),
    };
    create_dir_all(logdir)?;
    let report = logdir.join(format!("{}-tiger-{id}.log", consts.name_short));
    let rows = vec![
        ("Game", format!("{} {}", consts.name, consts.version)),
        ("Mod", shown),
        ("Folder", folder.display().to_string()),
    ];
    Ok(Prepared { rows, modfile, modpath: folder, report })
}

/// Run the check and write the report file. `on` hears about each step as it starts and ends.
/// The reports stay in memory, so they can be read again.
pub(crate) fn check(
    game: &Path,
    workshop: Option<&Path>,
    paradox: Option<&Path>,
    prepared: Prepared,
    on: &mut dyn FnMut(Progress),
) -> Result<Outcome> {
    let Prepared { rows, modfile, modpath, report } = prepared;
    let mut output = File::create(&report)?;

    on(Progress::Started(Step::Reading));
    let mut everything =
        Everything::new(None, Some(game), workshop, paradox, &modpath, modfile.replace_paths())?;
    everything.check_dependencies(&modfile);

    // Unfortunately have to disable the colors by default because
    // on Windows there's no easy way to view a file that contains those escape sequences.
    // There are workarounds but those defeat the purpose of -auto.
    // The colors can be enabled again in the config file.
    everything.load_output_settings(false);
    everything.load_config_filtering_rules();
    if config_reports_pending() {
        emit_reports(&mut output, false, false, false);
        bail!("Invalid config");
    }
    on(Progress::Finished(Step::Reading));

    on(Progress::Started(Step::Understanding));
    everything.load_all();
    on(Progress::Finished(Step::Understanding));

    on(Progress::Started(Step::Checking));
    everything.validate_all();
    everything.check_rivers();
    on(Progress::Finished(Step::Checking));

    let stats = emit_reports_with(
        &mut output,
        EmitOptions { summary_last: true, keep: true, ..EmitOptions::default() },
    );
    drop(output);

    // Properly dropping `everything` takes a noticeable amount of time, and we're exiting anyway.
    forget(everything);
    Ok(Outcome { rows, stats, report })
}

/// The check with the progress lines of the plain program.
fn validate_mod(
    consts: &GameConsts,
    game: &Path,
    workshop: Option<&Path>,
    paradox: Option<&Path>,
    modpath: &Path,
    logdir: &Path,
) -> Result<Outcome> {
    let prepared = prepare(consts, modpath, logdir)?;
    brand::screen(VERSION, TALL_RESULT);
    header(&prepared.rows);

    let mut stage = None;
    let outcome = check(game, workshop, paradox, prepared, &mut |progress| match progress {
        Progress::Started(step) => stage = Some(Stage::start(step.label())),
        Progress::Finished(_) => {
            if let Some(stage) = stage.take() {
                stage.finish("");
            }
        }
    })?;
    card(&outcome);
    Ok(outcome)
}

/// The result of the check, as the card the command line program prints too, but without the
/// commands to type: the menu below it is how this program is used.
fn card(o: &Outcome) {
    guide::print(&Guide {
        stats: &o.stats,
        command: "",
        modarg: "",
        hidden_below: None,
        filtered: false,
        has_fail_on: true,
        only_keys: &[],
        show_next: false,
    });
    eprintln!("{}  {}", style("Full report").dim(), o.report.display());
}

/// Show the first screen of the result again, after another screen has replaced it.
fn redraw(o: &Outcome) {
    brand::screen(VERSION, TALL_RESULT);
    header(&o.rows);
    card(o);
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Causes,
    Errors,
    Open,
    Another,
    Folder,
    Update,
    Quit,
}

/// What the user can do once the result is on the screen. The window stays until they quit.
fn menu(consts: &GameConsts, o: &Outcome) -> Result<()> {
    loop {
        let mut items: Vec<(Action, (String, String))> = Vec::new();
        let mut add = |action, label: &str, note: &str| {
            items.push((action, (label.to_string(), note.to_string())));
        };
        if o.stats.total() > 0 {
            add(Action::Causes, "Look at the causes", "what each one means and how to fix it");
        }
        if o.stats.at_least(Severity::Error) > 0 {
            add(Action::Errors, "Read the errors here", "page by page, worst first");
        }
        add(Action::Open, "Open the full report", "in your text editor");
        add(Action::Another, "Check another mod", "back to the list");
        add(Action::Folder, "Change the game folder", "if you moved or reinstalled it");
        add(Action::Update, "Look for a newer xTiger", "and install it");
        add(Action::Quit, "Quit", "");

        eprintln!();
        eprintln!("{}", style("What do you want to do now?").bold());
        eprintln!("{}", style("Up and down to move, Enter to choose, or press its number.").dim());
        eprintln!();
        let labels: Vec<(String, String)> = items.iter().map(|(_, item)| item.clone()).collect();
        let chosen = select(&labels)?.map_or(Action::Quit, |i| items[i].0);

        match chosen {
            Action::Quit => {
                pause::finished();
                return Ok(());
            }
            Action::Open => open(&o.report),
            Action::Causes => {
                causes_screen(o)?;
                redraw(o);
            }
            Action::Errors => {
                errors_screen();
                redraw(o);
            }
            Action::Another => {
                relaunch();
                return Ok(());
            }
            Action::Folder => {
                brand::screen(VERSION, TALL_FIRST);
                match firstrun::ask_game(consts) {
                    Ok(_) => {
                        relaunch();
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("\n{e}");
                        wait_for_enter("Press Enter to go back.");
                        redraw(o);
                    }
                }
            }
            Action::Update => {
                brand::screen(VERSION, TALL_FIRST);
                if let Err(e) = crate::update::update(VERSION, None, false) {
                    eprintln!("{e}");
                }
                wait_for_enter("Press Enter to go back.");
                redraw(o);
            }
        }
    }
}

/// Start the program again in this window, so it is as if it had just been opened. When that
/// one ends because the user quit, so does this one.
pub(crate) fn relaunch() {
    let ended = std::env::current_exe()
        .and_then(|exe| Command::new(exe).status())
        .is_ok_and(|status| status.success());
    if ended {
        pause::finished();
        std::process::exit(0);
    }
    eprintln!("The check did not finish.");
}

fn open(report: &Path) {
    #[cfg(windows)]
    let opened = Command::new("cmd").args(["/C", "start", ""]).arg(report).status();
    #[cfg(not(windows))]
    let opened = Command::new("xdg-open").arg(report).status();
    if opened.is_err() {
        eprintln!("Could not open it. Open the file by hand.");
    }
}

/// The causes, worst first. Choosing one explains it.
fn causes_screen(o: &Outcome) -> Result<()> {
    let keys = causes(&o.stats);
    loop {
        brand::screen(VERSION, NEVER);
        eprintln!("{}", style("The causes, worst first").bold());
        eprintln!("{}", style("Enter shows what one means. q goes back.").dim());
        eprintln!();
        let items: Vec<(String, String)> = keys
            .iter()
            .map(|k| {
                (
                    format!("{:>5}  {}", k.count, k.key),
                    tiger_lib::key_info(k.key).summary.to_string(),
                )
            })
            .collect();
        let Some(i) = select(&items)? else {
            return Ok(());
        };
        brand::screen(VERSION, NEVER);
        if let Err(e) = explain_for(Some(&keys[i].key.to_string()), false) {
            eprintln!("{e}");
        }
        wait_for_enter("Press Enter to go back to the list.");
    }
}

/// The errors, a page at a time. The reports are in memory still, so this asks for them again in
/// the short format.
fn errors_screen() {
    if console::colors_enabled_stderr() {
        enable_ansi_colors();
    }
    let mut buffer: Vec<u8> = Vec::new();
    emit_reports_with(
        &mut buffer,
        EmitOptions {
            format: OutputFormat::Short,
            pretty: true,
            hide_below: Some(Severity::Error),
            keep: true,
            ..EmitOptions::default()
        },
    );
    let text = String::from_utf8_lossy(&buffer).to_string();
    let lines: Vec<&str> = text.lines().collect();
    let (rows, columns) =
        Term::stderr().size_checked().map_or((30, 100), |(r, c)| (usize::from(r), usize::from(c)));
    let budget = rows.saturating_sub(6).max(8);

    let mut shown = 0;
    while shown < lines.len() {
        brand::screen(VERSION, NEVER);
        // A long line wraps in the terminal and takes more than one row.
        let mut to = shown;
        let mut used = 0;
        while to < lines.len() {
            let height = measure_text_width(lines[to]).div_ceil(columns.max(1)).max(1);
            if used + height > budget && to > shown {
                break;
            }
            used += height;
            to += 1;
        }
        eprintln!("{}", style(format!("Errors {}-{to} of {}", shown + 1, lines.len())).bold());
        eprintln!();
        for line in &lines[shown..to] {
            eprintln!("{line}");
        }
        shown = to;
        if shown >= lines.len() {
            break;
        }
        eprint!("\n{} ", style("Enter for more, q and Enter to go back:").dim());
        let mut answer = String::new();
        if stdin().read_line(&mut answer).unwrap_or(0) == 0
            || answer.trim().eq_ignore_ascii_case("q")
        {
            return;
        }
    }
    wait_for_enter("That was all of them. Press Enter to go back.");
}
