use std::io::{Write, stdout};
use std::{mem::forget, path::PathBuf};

use std::str::FromStr;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum, error::ErrorKind};
use tiger_lib::ModFile;
use tiger_lib::{
    CliFilter, EmitOptions, ErrorKey, Everything, OutputFormat, Severity, config_reports_pending,
    disable_ansi_colors, emit_reports, emit_reports_with, get_version_from_launcher,
    restrict_reports, set_show_loaded_mods, set_show_vanilla, suppress_from_json,
    validate_config_file,
};

use crate::GameConsts;
use crate::explain::explain;
use crate::gamedir::{
    find_game_directory_steam, find_paradox_directory, find_workshop_directory_steam,
};
use crate::update::update;
use crate::version::warn_versions;

#[derive(Parser)]
#[clap(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(flatten)]
    validate_args: Option<ValidateArgs>,
}

#[derive(Subcommand)]
enum Commands {
    /// Update the binary. If no version is specified, the latest release is pulled from GitHub and
    /// installed over the current binary.
    Update {
        /// release version (e.g. 0.9.3)
        version: Option<String>,
    },
    /// Explain a report key: what it means, how to fix it and how to silence it.
    /// Without a key, list all of them.
    Explain {
        /// the key as it appears in the output, e.g. missing-item
        key: Option<String>,
    },
}

/// How the reports are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Each report with its source lines (the default)
    Full,
    /// One line per report: `file:line: severity key: message`
    Short,
    /// JSON array, same as --json
    Json,
    /// Only the counts by severity, key and file
    Summary,
}

/// The worst severity that makes the exit code non-zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum FailOn {
    /// Always exit with 0 (the default)
    Never,
    /// Fail on fatal reports only
    Fatal,
    /// Fail on errors and fatal reports
    Error,
    /// Fail on warnings and anything worse
    Warning,
    /// Fail on untidy reports and anything worse
    Untidy,
    /// Fail on any report
    Tips,
}

impl FailOn {
    fn severity(self) -> Option<Severity> {
        match self {
            FailOn::Never => None,
            FailOn::Fatal => Some(Severity::Fatal),
            FailOn::Error => Some(Severity::Error),
            FailOn::Warning => Some(Severity::Warning),
            FailOn::Untidy => Some(Severity::Untidy),
            FailOn::Tips => Some(Severity::Tips),
        }
    }
}

fn parse_severity(s: &str) -> Result<Severity, String> {
    Severity::from_str(&s.to_ascii_lowercase())
        .map_err(|_| format!("`{s}` is not a severity; use fatal, error, warning, untidy or tips"))
}

fn parse_key(s: &str) -> Result<ErrorKey, String> {
    ErrorKey::from_str(&s.to_ascii_lowercase())
        .map_err(|_| format!("`{s}` is not a report key; run `explain` to list them"))
}

/// Exit status when reports at or above `--fail-on` were printed.
/// Status 2 is for runs that could not be done at all (bad arguments, no game, invalid config).
const EXIT_PROBLEMS: i32 = 1;

#[derive(Args)]
struct ValidateArgs {
    /// Path to .mod file of mod to check.
    modpath: PathBuf,
    #[clap(visible_alias = "ck3")]
    #[clap(long)]
    /// Path to game main directory.
    game: Option<PathBuf>,
    #[clap(skip)]
    /// Path to game workshop directory.
    workshop: Option<PathBuf>,
    #[clap(long)]
    /// Path to paradox directory
    paradox: Option<PathBuf>,
    /// Path to custom .conf file.
    #[clap(long)]
    config: Option<PathBuf>,
    /// Show errors in the base game script code as well
    #[clap(long)]
    show_vanilla: bool,
    /// Show errors in other loaded mods as well
    #[clap(long)]
    show_mods: bool,
    /// Output the reports in JSON format (same as --format json)
    #[clap(long)]
    json: bool,
    /// How to write the reports
    #[clap(long, value_enum, default_value = "full")]
    format: Format,
    /// Print counts by severity, key and file before the reports.
    /// The first thing to read in a mod with thousands of reports.
    #[clap(long)]
    summary: bool,
    /// Exit with status 1 when a report at this severity or worse is printed. Filters apply first.
    /// Without this the exit status is 0 even when problems are found.
    #[clap(long, value_enum, default_value = "never")]
    fail_on: FailOn,
    /// Only show reports at this severity or worse (fatal, error, warning, untidy, tips)
    #[clap(long, value_name = "LEVEL", value_parser = parse_severity)]
    severity: Option<Severity>,
    /// Only show reports with this key. Repeat or separate with commas. See `explain`.
    #[clap(long, value_name = "KEY", value_delimiter = ',', value_parser = parse_key)]
    only_key: Vec<ErrorKey>,
    /// Hide reports with this key. Repeat or separate with commas.
    #[clap(long, value_name = "KEY", value_delimiter = ',', value_parser = parse_key)]
    skip_key: Vec<ErrorKey>,
    /// Only show reports in this file, folder or glob, relative to the mod folder
    /// (e.g. events, common/traits/*.txt). Repeat for several.
    #[clap(long, value_name = "PATH")]
    path: Vec<String>,
    /// Consolidate certain repeated errors
    #[clap(long, short)]
    consolidate: bool,
    /// Warn about items that are defined but unused
    #[clap(long)]
    unused: bool,
    /// Do checks specific to the Princes of Darkness mod
    #[clap(long)]
    pod: bool,
    /// Omit color from the output. False by default.
    /// Can also be configured in the config file.
    #[clap(long)]
    no_color: bool,
    /// Load a JSON file of reports to remove from the output.
    #[clap(long)]
    suppress: Option<PathBuf>,
}

/// Run the main tiger application.
///
/// It provides a number of command line arguments, as well as self-updating capability with the `update` subcommand.
#[allow(clippy::missing_panics_doc)] // it thinks we can panic on cli.validate_args.unwrap()
pub fn run(
    game_consts: &GameConsts,
    current_version: &'static str,
    bin_name: &'static str,
) -> Result<()> {
    use clap::{CommandFactory, FromArgMatches};

    let &GameConsts { name, name_short, version, app_id, signature_file, paradox_dir } =
        game_consts;

    let matches = Cli::command()
        .version(current_version)
        .name(bin_name)
        .display_name("xTiger")
        .about("xTiger: Tiger, the Crusader Kings III mod validator, updated for CK3 1.20")
        .after_help(
            "Based on Tiger by amtep. Source and releases: https://github.com/9mirx0r/xTiger",
        )
        .get_matches();
    let cli = Cli::from_arg_matches(&matches).map_err(|err| err.exit()).unwrap();

    #[allow(clippy::single_match_else)]
    match cli.command {
        Some(Commands::Update { version: target_version }) => {
            update(current_version, target_version.as_deref())?;
            Ok(())
        }
        Some(Commands::Explain { key }) => explain(key.as_deref()),
        None => {
            let mut args = cli.validate_args.unwrap();

            if args.json {
                if args.format != Format::Full && args.format != Format::Json {
                    Cli::command()
                        .error(ErrorKind::ArgumentConflict, "--json conflicts with --format.")
                        .exit();
                }
                args.format = Format::Json;
            }
            if args.format == Format::Json && (args.consolidate || args.summary) {
                Cli::command()
                    .error(
                        ErrorKind::ArgumentConflict,
                        "Can't use report consolidation or --summary with JSON output.",
                    )
                    .exit();
            }

            #[cfg(windows)]
            if !args.no_color {
                let _ = ansiterm::enable_ansi_support()
                    .map_err(|_| eprintln!("Failed to enable ANSI support for Windows10 users. Continuing probably without colored output."));
            }

            crate::brand::banner(bin_name, current_version, !args.no_color);

            if args.game.is_none() {
                args.game = find_game_directory_steam(app_id).ok();
            }
            if args.workshop.is_none() {
                args.workshop = find_workshop_directory_steam(app_id).ok();
            }
            if args.paradox.is_none() {
                args.paradox = find_paradox_directory(&PathBuf::from(paradox_dir));
            }
            if let Some(ref mut game) = args.game {
                eprintln!("Using {name_short} directory: {}", game.display());
                let mut sig = game.clone();
                sig.push(signature_file);
                if !sig.is_file() {
                    eprintln!("That does not look like a {name_short} directory.");
                    game.push("..");
                    eprintln!("Trying: {}", game.display());
                    sig.clone_from(game);
                    sig.push(signature_file);
                    if sig.is_file() {
                        eprintln!("Ok.");
                    } else {
                        bail!(
                            "Cannot find {name_short} directory. Please supply it as the --game option."
                        );
                    }
                }
            } else {
                bail!("Cannot find {name_short} directory. Please supply it as the --game option.");
            }

            if let Some(ref game_dir) = args.game {
                if let Ok(launcher_game_version) = get_version_from_launcher(game_dir) {
                    if warn_versions(name, version, &launcher_game_version).is_err() {
                        eprintln!("Tiger was made for {name} version {version}.");
                        eprintln!(
                            "Comparing this to the game's version {launcher_game_version} failed."
                        );
                        eprintln!(
                            "If you are using a newer version of {name}, it may be inaccurate."
                        );
                    }
                } else {
                    eprintln!("Tiger was made for {name} version {version}.");
                    eprintln!("If you are using a newer version of {name}, it may be inaccurate.");
                }
            }

            args.config = validate_config_file(args.config);

            if let Some(suppress) = args.suppress {
                eprintln!("Suppressing reports from: {}", suppress.display());
                suppress_from_json(&suppress)?;
            }

            if args.show_vanilla {
                eprintln!(
                    "Showing warnings for base game files too. There will be many false positives in those."
                );
            }

            if args.show_mods {
                eprintln!("Showing warnings for other loaded mods too.");
            }

            if args.unused {
                eprintln!(
                    "Showing warnings for unused localization. There will be many false positives."
                );
            }

            if args.pod {
                eprintln!("Doing special checks for the Princes of Darkness mod.");
            }

            if args.no_color {
                // Disable colors both here and after reading the config, because reading the modfile and config may emit errors.
                disable_ansi_colors();
            }

            let mut everything;

            {
                if args.modpath.is_dir() {
                    args.modpath.push("descriptor.mod");
                }

                let modfile = ModFile::read(&args.modpath)?;
                let modpath = modfile.modpath();
                if !modpath.exists() {
                    eprintln!("Looking for mod in {}", modpath.display());
                    bail!("Cannot find mod directory. Please make sure the .mod file is correct.");
                }
                eprintln!("Using mod directory: {}", modpath.display());

                everything = Everything::new(
                    args.config.as_deref(),
                    args.game.as_deref(),
                    args.workshop.as_deref(),
                    args.paradox.as_deref(),
                    &modpath,
                    modfile.replace_paths(),
                )?;
                everything.check_dependencies(&modfile);
            }

            // Print a blank line between the preamble and the first report:
            eprintln!();

            everything.load_output_settings(true);
            everything.load_config_filtering_rules();

            let mut output = stdout();

            // Only reports about the config stop the run. Notes about the .mod file, like a
            // `replace_path` that is not needed, stay stored and print with the other reports.
            if args.format != Format::Json && config_reports_pending() {
                emit_reports(&mut output, false, args.consolidate, false);
                bail!("Invalid config");
            }

            // We must apply the --no-color flag AFTER loading and applying the config,
            // because we want it to override the config.
            if args.no_color {
                disable_ansi_colors();
            }
            // Same logic applies to showing vanilla and other mods
            if args.show_vanilla {
                set_show_vanilla(true);
            }
            if args.show_mods {
                set_show_loaded_mods(true);
            }
            // The command line narrows down whatever the config file lets through.
            let filter = CliFilter {
                min_severity: args.severity,
                only_keys: args.only_key.clone(),
                skip_keys: args.skip_key.clone(),
                paths: args.path.clone(),
            };
            if let Err(e) = restrict_reports(&filter) {
                Cli::command().error(ErrorKind::ValueValidation, e).exit();
            }

            everything.load_all();
            everything.validate_all();
            everything.check_rivers();

            if args.pod {
                everything.check_pod();
            }

            if args.unused {
                everything.check_unused();
            }

            let options = EmitOptions {
                format: match args.format {
                    Format::Full => OutputFormat::Full,
                    Format::Short => OutputFormat::Short,
                    Format::Json => OutputFormat::Json,
                    Format::Summary => OutputFormat::SummaryOnly,
                },
                consolidate: args.consolidate,
                summary_first: args.summary,
                summary_last: args.format == Format::Full,
            };
            let stats = emit_reports_with(&mut output, options);
            _ = output.flush();

            if args.format != Format::Json && stats.total() == 0 {
                eprintln!("No problems found.");
            }

            // Properly dropping `everything` takes a noticeable amount of time, and we're exiting anyway.
            forget(everything);
            if let Some(severity) = args.fail_on.severity() {
                let failing = stats.at_least(severity);
                if failing > 0 {
                    eprintln!(
                        "{failing} reports at {severity} or worse; exiting with status {EXIT_PROBLEMS}."
                    );
                    std::process::exit(EXIT_PROBLEMS);
                }
            }
            Ok(())
        }
    }
}
