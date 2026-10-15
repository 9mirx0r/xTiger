//! The tools the server offers: their descriptions for the assistant, and what they do.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::docs::{self, Lookup};
use crate::game::{self, LOGS, ReadLog, RunGame};
use crate::gap;
use crate::jobs;
use crate::journal::{self, ModRef, Tally};
use crate::ledger;
use crate::locate::Locations;
use crate::migrate;
use crate::migrate_apply;
use crate::mods::{self, ModInfo};
use crate::overrides;
use crate::projects;
use crate::requests::{self, Status};
use crate::runs::{self, Counts, Filter, GROUPS, Options, QueryResult, Row, SEVERITIES, Summary};
use crate::vanilla::{self, Search};
use crate::{playsets, sessions};

/// What a tool hands back to the assistant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// A PNG file, sent as an image.
    Image(PathBuf),
}

/// What a running tool can use besides its arguments.
pub struct Context<'a> {
    pub progress: &'a mut dyn FnMut(&str),
    pub cancelled: &'a dyn Fn() -> bool,
    /// Told which mod the call works on, as soon as that is known.
    pub working_on: &'a mut dyn FnMut(&ModRef),
    /// The work session the call belongs to.
    pub session: &'a str,
    /// The assistant, as it named itself when it connected.
    pub client: Option<&'a str>,
}

/// What a call did, in a few words, for the activity journal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Note {
    pub summary: String,
    pub mod_ref: Option<ModRef>,
    pub run_id: Option<String>,
    /// What a validation found.
    pub found: Option<Tally>,
}

/// A call that worked.
#[derive(Debug, PartialEq, Eq)]
pub struct Done {
    pub content: Vec<Content>,
    pub note: Note,
}

impl Done {
    fn new(content: Vec<Content>, summary: impl Into<String>) -> Self {
        Self { content, note: Note { summary: summary.into(), ..Note::default() } }
    }
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

/// Why a call did not work.
#[derive(Debug, PartialEq, Eq)]
pub enum CallError {
    /// No tool has this name. This is a protocol error.
    Unknown(String),
    /// The tool ran into a problem the assistant can act on.
    Failed(String),
}

impl From<String> for CallError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for CallError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}

const READ_ONLY: &str = "readOnly";
const LOCAL: &str = "local";
/// Rewrites files the user owns: the client should ask first.
const WRITE: &str = "write";
const GAME: &str = "game";

struct Tool {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// (name, JSON schema) of each argument, in the order they are shown.
    params: Vec<(&'static str, Value)>,
    required: &'static [&'static str],
    kind: &'static str,
}

fn string(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn boolean(description: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": description, "default": default})
}

fn integer(description: &str, default: u64, minimum: u64) -> Value {
    json!({"type": "integer", "description": description, "default": default, "minimum": minimum})
}

fn choice(description: &str, values: &[&str]) -> Value {
    json!({"type": "string", "description": description, "enum": values})
}

fn strings(description: &str) -> Value {
    json!({"type": "array", "items": {"type": "string"}, "description": description})
}

/// Every tool takes this, so the user can follow what the assistant does in the xTiger app.
pub const REASON: &str = "reason";

fn reason() -> Value {
    string(
        "One short sentence on why you are making this call, such as \"Checking that the new event loads\". The user sees it in the xTiger app.",
    )
}

/// The arguments of `ck3_run` and `ck3_run_start`.
fn run_params() -> Vec<(&'static str, Value)> {
    vec![
        (
            "commands",
            strings(
                "Console commands, typed in order. Each must be one non-empty line of at most 255 UTF-16 code units.",
            ),
        ),
        ("screenshot", boolean("Take a screenshot of the game window at the end.", true)),
        (
            "bookmark",
            json!({"type": "string", "description": "An existing bookmark key, not a date; checked before launching.", "default": "bm_867_carolingians"}),
        ),
        (
            "play",
            json!({"type": "string", "description": "The title key of the ruler to play, such as k_france or c_jaffa. It must be a title that \
                                       is held at the bookmark's date; if it is not, the game stays on the lobby.", "default": "k_france"}),
        ),
        ("keep_open", boolean("Leave the game running, to continue with ck3_keys.", false)),
        ("mods", strings("Mods to load: names, folders or .mod files, as for xtiger_validate.")),
        (
            "playset",
            string(
                "A launcher playset whose enabled mods are loaded first, in its order, to test the mod \
                     together with others. xtiger_playsets lists them.",
            ),
        ),
        ("load_timeout", integer("Seconds to wait for the game to load.", 300, 30)),
        (
            "typing",
            json!({"type": "string", "enum": ["unicode", "scancode"], "default": "unicode",
                       "description": "unicode works on any keyboard layout. scancode is a fallback that \
                                       only types letters, digits, space, '.', '-' and '_'."}),
        ),
        (
            "background",
            boolean(
                "Do not take over the user's screen: nothing is typed, no screenshot is taken, the game \
                     starts minimized at idle priority and never takes the focus. It cannot send commands, so \
                     the mod must trigger what it tests itself, for example from an on_action such as \
                     on_game_start_after_lobby; read the result with ck3_logs. Always on when the user \
                     chose background game tests in the xTiger app's settings (the default), whatever \
                     is passed here.",
                false,
            ),
        ),
    ]
}

fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xtiger_status",
            title: "Check the setup",
            description: "Show which ck3-tiger, CK3 install and CK3 user folder the other tools use, where each \
                          was found, and whether it works. Call it first when a tool says something is missing.",
            params: vec![],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_mods",
            title: "List mods",
            description: "List the mods in the CK3 user folder (local and Workshop) and the folders added in the \
                          xTiger app: name, version, supported game version, .mod file and folder. Any of the \
                          names can be passed to xtiger_validate.",
            params: vec![(
                "filter",
                string("Keep only mods whose name contains this text, ignoring case."),
            )],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_validate",
            title: "Validate a mod",
            description: "Check a CK3 mod with ck3-tiger, the way the game will read it. Each run is saved \
                          under a run_id that xtiger_reports can query. Returns the totals by severity, the most \
                          common report keys and messages, what changed since the last run of the same mod (new \
                          and fixed reports, with the first new ones), which ck3-tiger ran, and its own \
                          messages (game version check, loading problems). The mods this one depends on are \
                          loaded too, as the game does, and listed in loaded_mods; a dependency that is not \
                          installed is in unresolved_dependencies, and what it defines is reported as missing. \
                          Takes from a few seconds to a few minutes.",
            params: vec![
                (
                    "mod_path",
                    string(
                        "The mod: its name as the launcher shows it (or a unique part of it), its folder, or \
                         its .mod file.",
                    ),
                ),
                (
                    "show_vanilla",
                    boolean("Also report problems in the base game's own files.", false),
                ),
                ("config", string("A ck3-tiger.conf file to use instead of the mod's own.")),
                (
                    "with",
                    strings(
                        "More mods to load with this one, such as a patch's base mod or the rest of a \
                         playset: names, folders or .mod files, as for mod_path. They load before the mod \
                         that is checked and cannot override it.",
                    ),
                ),
                (
                    "playset",
                    string(
                        "A launcher playset (xtiger_playsets lists them): every enabled mod in it is \
                         loaded with this one, in the playset's order, as `with` does. A mod of the \
                         playset with no .mod file is read from the descriptor.mod in its folder; if \
                         it has neither, it is left out and named in the answer.",
                    ),
                ),
                (
                    "load_dependencies",
                    boolean(
                        "Load the mods this one depends on (default true). Turn it off to check the mod \
                         alone, which reports everything those mods define as missing.",
                        true,
                    ),
                ),
            ],
            required: &["mod_path"],
            kind: LOCAL,
        },
        Tool {
            name: "xtiger_runs",
            title: "List saved runs",
            description: "List the saved xtiger_validate runs, newest first, with their run_id, mod, number of \
                          reports and which ck3-tiger ran. The newest 20 are kept.",
            params: vec![],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_reports",
            title: "Query reports",
            description: "Query the reports of a saved xtiger_validate run: the newest run of any mod, \
                          unless run_id or mod_path is given. \
                          Each report has its severity, key, message, extra info, where (relative path:line), \
                          file (the full path, to open or edit it), the line of code, and also (other places \
                          involved). Use group_by first to see the shape of a big run, then filter.",
            params: vec![
                ("pattern", string("Case-insensitive regex on the message.")),
                (
                    "path",
                    string(
                        "Part of a file path, such as events/ or my_faith.txt. Matches any place a report \
                         points to, not only the first.",
                    ),
                ),
                ("severity", choice("Only this severity.", &SEVERITIES)),
                ("key", string("Only this report key, such as missing-item.")),
                (
                    "group_by",
                    choice(
                        "Count the matching reports by this instead of listing them. By file or \
                         folder, a report that points to several places counts in each of them, \
                         so the counts can add up to more than the number of reports. \
                         message cuts the text at 100 characters; template keeps all of it but \
                         replaces each `quoted` name with `…`, so the same cause is one group.",
                        &GROUPS,
                    ),
                ),
                (
                    "limit",
                    integer(
                        "How many reports or groups to return. With group_by, total_groups says \
                         how many there are. 0 returns no rows, only the counts.",
                        50,
                        0,
                    ),
                ),
                ("offset", integer("How many to skip, to page through the results.", 0, 0)),
                ("run_id", string("Which saved run. The newest by default.")),
                (
                    "mod_path",
                    string(
                        "The mod (name, folder, workshop id or .mod file): its newest run. Ignored \
                         when run_id is given.",
                    ),
                ),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "ck3_run",
            title: "Play-test in CK3",
            description: "Launch CK3 in debug mode, start a game, type console commands, take a screenshot and \
                          report the logs. Commands are console commands typed in order, for example \"event \
                          my_mod.0001\" or \"effect add_gold = 100\". The game starts as `play` (a title key) \
                          from `bookmark`. Unless keep_open is set, the game is closed afterwards. Returns which \
                          commands were sent, the deduplicated error.log, relevant game.log lines and a \
                          screenshot. Every requested command must be confirmed in debug.log. A run that tested nothing (stopped, failed, or a mod the game did not \
                          load) is an error, with the report in its message. Earlier logs are moved to \
                          logs/xtiger-archive. Input stops if another \
                          window takes focus; use background to leave the user's screen alone. Windows only; takes a few minutes. The mod must be enabled in the \
                          launcher playset, or passed in mods or playset.",
            params: run_params(),
            required: &[],
            kind: GAME,
        },
        Tool {
            name: "ck3_run_start",
            title: "Start a play-test in CK3",
            description: "Start the same play-test as ck3_run, but as a job: it answers at once with a job id and the game \
                          keeps running in the xTiger server, so a client that gives up waiting after a few minutes \
                          has not stopped it. Use it for runs that may take longer than the client's limit, such as a \
                          long list of commands or a big mod (raise load_timeout, for example to 480). It takes the \
                          same arguments as ck3_run and is refused for the same reasons, before any game starts. Read the \
                          job with ck3_job: status (state, latest progress line, how many commands were typed), result (the \
                          report, once it is over) and cancel. The job remembers which files the mod had when it started, and \
                          ck3_job says whether they still do. A client timeout is not a game failure: the job is still there.",
            params: run_params(),
            required: &[],
            kind: GAME,
        },
        Tool {
            name: "ck3_job",
            title: "Read or stop a play-test job",
            description: "Look at a play-test started with ck3_run_start. action status (the default) gives the job's state \
                          (running, finished, failed, cancelled, interrupted), the phase (starting, loading, commands, \
                          finishing), the latest progress line, elapsed seconds and the commands typed; give wait to hold the \
                          call for up to 30 seconds until the job ends. result gives the report of a job that is over, with \
                          the screenshot: failed means the run tested nothing, finished only means it ran, and what the mod \
                          did is in the logs (ck3_logs). cancel stops the job and puts the mod list back. list shows the jobs \
                          on record, newest first. Without job_id the newest job is used. Jobs are kept after the server \
                          restarts; one whose server stopped is shown as interrupted.",
            params: vec![
                (
                    "action",
                    json!({"type": "string", "enum": ["status", "result", "cancel", "list"], "default": "status",
                           "description": "What to do with the job."}),
                ),
                ("job_id", string("The job to look at. The newest one when not given.")),
                (
                    "wait",
                    integer(
                        "Seconds to wait for the job to end, at most 30. Only for status.",
                        0,
                        0,
                    ),
                ),
            ],
            required: &[],
            kind: LOCAL,
        },
        Tool {
            name: "ck3_keys",
            title: "Send keys to CK3",
            description: "Send keys or text to a CK3 window that is already open, for example after ck3_run with \
                          keep_open. scancodes are sent first, then text is typed. To run a console command: \
                          scancodes \"29\" opens the console, text \"event my_mod.0001\", then scancodes \"1C\" \
                          in a second call presses Enter. Windows only.",
            params: vec![
                (
                    "scancodes",
                    string(
                        "Comma-separated hex hardware scancodes. Prefix one with S to hold Shift. Useful ones: \
                         29 console, 1C enter, 01 escape, 39 space.",
                    ),
                ),
                ("text", string("Text typed as Unicode characters, after the scancodes.")),
                ("screenshot", boolean("Take a screenshot of the game window afterwards.", true)),
            ],
            required: &[],
            kind: GAME,
        },
        Tool {
            name: "ck3_logs",
            title: "Read CK3 logs",
            description: "Read a CK3 log file from the user folder. With dedupe, timestamps are dropped and \
                          repeated lines are counted, most frequent first.",
            params: vec![
                (
                    "name",
                    json!({"type": "string", "enum": LOGS, "default": "error", "description": "Which log."}),
                ),
                ("max_lines", integer("The most lines to return.", 200, 0)),
                ("dedupe", boolean("Drop timestamps and count repeated lines.", true)),
                (
                    "tail",
                    boolean("Without dedupe, return the last lines instead of the first.", false),
                ),
                ("pattern", string("Keep only lines matching this case-insensitive regex.")),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: sessions::TOOL,
            title: "Name or wrap up the job",
            description: "Tell the user what you are working on, so the xTiger app can group your calls into \
                          one session with its own summary. At the start of a job, call it with a title. At \
                          the end, call it with wrap_up. A different title starts a new session. Returns the \
                          session so far: each validation of each mod, what was fixed, what is left, and \
                          which of the mod's files changed.",
            params: vec![
                (
                    "title",
                    string(
                        "What the job is, in a few words, such as \"Update Better Courts to 1.20\".",
                    ),
                ),
                (
                    "wrap_up",
                    string(
                        "At the end of the job: one or two sentences on what you did and what is left.",
                    ),
                ),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_pending_requests",
            title: "Pick up the user's requests",
            description: "List what the user asked for in the xTiger app and is not done yet, oldest first: fix \
                          these reports (kind fix, with the reports and the user's note), or bring a mod up to \
                          the current game version (kind update, with a brief of what the validator found). \
                          Calling it tells the user you have them. When one is done, or cannot be done, call \
                          xtiger_finish_request.",
            params: vec![],
            required: &[],
            kind: LOCAL,
        },
        Tool {
            name: "xtiger_finish_request",
            title: "Close a request",
            description: "Close a request from xtiger_pending_requests, with what you did or why you could not \
                          do it. The user sees it in the xTiger app.",
            params: vec![
                ("id", string("The request's id.")),
                (
                    "outcome",
                    choice(
                        "done, or skipped when you could not or should not do it.",
                        &["done", "skipped"],
                    ),
                ),
                ("note", string("One or two sentences: what you changed, or why it was skipped.")),
            ],
            required: &["id", "note"],
            kind: LOCAL,
        },
        Tool {
            name: "xtiger_compare",
            title: "Compare two checks",
            description: "Compare two saved xtiger_validate runs: the totals by severity of each, which reports \
                          are new and which were fixed, by key and one by one. By default the newest run is \
                          compared with the run of the same mod before it.",
            params: vec![
                ("run_id", string("The later run. The newest by default.")),
                (
                    "against",
                    string("The earlier run. By default the run of the same mod before run_id."),
                ),
                (
                    "mod_path",
                    string(
                        "Without run_id: compare the newest run of this mod, by name, folder or .mod file.",
                    ),
                ),
                ("limit", integer("How many new and fixed reports to list.", 30, 0)),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "ck3_vanilla",
            title: "Search the base game",
            description: "Search the base game's own files to see how CK3 does something. With name, find where \
                          it is defined: a scripted trigger or effect, an event id (my_events.0001), a decision, \
                          an on_action, a GUI type or a localization key; each hit has the file, the line and the \
                          whole definition. With pattern, list the lines that match a regex. Looks in common/, \
                          events/, gui/, history/, notifications/ and the English localization unless path says \
                          otherwise.",
            params: vec![
                (
                    "name",
                    string("The name of the definition, such as is_valid_for_feast or tgp.0101."),
                ),
                (
                    "pattern",
                    string(
                        "Case-insensitive regex on lines. With name, keeps only definitions that match it.",
                    ),
                ),
                (
                    "path",
                    string(
                        "Only files whose path under game/ contains this, such as common/decisions or \
                         localization/french.",
                    ),
                ),
                ("limit", integer("How many hits to return.", 10, 0)),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_overrides",
            title: "Compare a mod with the base game",
            description: "Find what a mod overrides in the base game and how it differs from the game installed now. A mod file with the path of a game file replaces the whole game file, so the names in missing_from_mod are gone from the game; a mod definition with the name of a game definition in another file replaces that one. For each, the lines the game has now and the mod lacks (`- `) and the lines only the mod has (`+ `). Near copies with a few differences are the likeliest to be stale after a game update. Looks at common/, events/ and history/.",
            params: vec![
                (
                    "mod_path",
                    string(
                        "The mod: its name, workshop id, folder or .mod file. xtiger_mods lists them.",
                    ),
                ),
                (
                    "path",
                    string("Only mod files whose path contains this, such as common/decisions."),
                ),
                ("diff_lines", integer("The most diff lines shown for one definition.", 20, 0)),
                ("limit", integer("How many files, definitions and names to list.", 15, 1)),
                (
                    "base",
                    string(
                        "A copy of the game version the mod was written for: the folder that holds game/. \
                         With it each difference says who made it, the mod or the game since, and a \
                         `rebase` verdict suggests what to do: keep_mod, take_game, combine or review. \
                         Without it, nothing can say which side made a difference.",
                    ),
                ),
            ],
            required: &["mod_path"],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_project",
            title: "Keep an update project for a mod",
            description: "A durable record for bringing one mod to a newer game version. It outlives the 20 saved runs, keeps its baseline when the work moves to a new folder, and says which results still belong to the files on disk. Status also has an `update` object with the stages process, static, runtime and review: a stage that did not run says so, and `update_complete` is true only when nothing blocks. Actions: create (with copy_to the work happens on a copy and the original is never touched; base is the old game folder, as for xtiger_overrides), list, status (what changed since the project began, which runs are stale, what still needs a decision), ledger (each definition of common/, events/ and history/ as preserved, changed, removed or added, with the lines and calls lost), decide (a removed definition or a dropped call needs an `intended` or `restore` decision with a note, bound to that exact difference), annotate (a note on diagnostics of the latest run: an intentional override, a test assertion, a defect still to fix or only formatting; the diagnostics stay in the run and in every count, and a note stops applying when its definition changes), pin or unpin a run (a pinned run is kept beyond the newest 20), baseline (choose the run to compare with) and export (the record, manifest, ledger and the runs that matter, into a new folder). The ledger compares text, not behaviour: it never says a mod plays the same.",
            params: vec![
                (
                    "action",
                    choice(
                        "What to do (default status).",
                        &[
                            "create", "list", "status", "ledger", "decide", "annotate", "pin",
                            "unpin", "baseline", "export",
                        ],
                    ),
                ),
                (
                    "project",
                    string(
                        "The project id, from list or create. Left out, the project of mod_path, or the only project.",
                    ),
                ),
                (
                    "mod_path",
                    string(
                        "create: the mod: its name, workshop id, folder or .mod file. xtiger_mods lists them.",
                    ),
                ),
                ("name", string("create: a name for the project. The mod's name when left out.")),
                (
                    "copy_to",
                    string(
                        "create: a new or empty folder. The mod is copied there and the project works on the copy.",
                    ),
                ),
                (
                    "base",
                    string(
                        "create: a copy of the game version the mod was written for: the folder that holds game/.",
                    ),
                ),
                (
                    "run_id",
                    string(
                        "pin, unpin, baseline, create: a saved run (xtiger_runs lists them). Pin and unpin default to the newest run of the project's mod; create defaults to the mod's newest run as baseline.",
                    ),
                ),
                (
                    "definition",
                    string(
                        "decide: the definition, as the ledger shows it (`folder: name`) or just its name.",
                    ),
                ),
                (
                    "decision",
                    choice(
                        "decide: `intended` if the removal or dropped call is wanted, `restore` if it must be put back.",
                        &["intended", "restore"],
                    ),
                ),
                ("note", string("decide, annotate: why. Kept with the project.")),
                (
                    "key",
                    string(
                        "annotate: the key of the diagnostics to write a note on, as xtiger_reports shows it.",
                    ),
                ),
                (
                    "path",
                    string(
                        "annotate: a part of the file path the diagnostics are in. Any file when left out.",
                    ),
                ),
                ("line", integer("annotate: only the diagnostics at this line. 0 for any.", 0, 0)),
                (
                    "message",
                    string(
                        "annotate: the message of one diagnostic. Then the note goes on that diagnostic alone, even when others share its key and line; path must be its whole path and line its line (xtiger_reports shows both). Left out, every diagnostic of the key, path and line gets the note.",
                    ),
                ),
                (
                    "kind",
                    choice(
                        "annotate: what the diagnostics are.",
                        &[
                            "intentional_override",
                            "test_assertion",
                            "unresolved_defect",
                            "advisory_formatting",
                        ],
                    ),
                ),
                ("to", string("export: a new or empty folder to write the evidence into.")),
                ("limit", integer("ledger: how many definitions to list in each group.", 25, 1)),
            ],
            required: &[],
            kind: LOCAL,
        },
        Tool {
            name: "xtiger_migrate",
            title: "Plan the renames for a newer game version",
            description: "A dry run: list the mechanical edits that bring a mod written for an older CK3 version in line with 1.20, such as every_character to every_living_character, is_created to is_title_created, create_holy_order_effect to create_holy_order_accompanying_effect, has_doctrine = tenet_x to has_tenet, and each removed effect or trigger that the validator names a single replacement for, such as start_diarchy to try_start_diarchy. Each edit has the file, the line, the line as it is and as it would read. Nothing is written: xtiger_migrate_apply writes them, or make the edits yourself, then xtiger_validate again. Renames that depend on the surrounding script are not here; the validator explains those.",
            params: vec![
                (
                    "mod_path",
                    string(
                        "The mod: its name, workshop id, folder or .mod file. xtiger_mods lists them.",
                    ),
                ),
                (
                    "path",
                    string("Only mod files whose path contains this, such as common/decisions."),
                ),
                ("limit", integer("How many edits to list.", 50, 1)),
            ],
            required: &["mod_path"],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_migrate_apply",
            title: "Write the renames for a newer game version",
            description: "Write into the mod files every edit xtiger_migrate lists (all of them, not only the ones it showed). Each file that changes is first copied, as it was, to a new backup folder outside the mod; the result names it. Only the changed lines are rewritten: line endings, comments and quoted text stay. Files that are not UTF-8 are left alone and listed. Run xtiger_migrate first and check its edits; a rename read from the validator's tables may need its arguments checked. Run xtiger_validate after.",
            params: vec![
                (
                    "mod_path",
                    string(
                        "The mod: its name, workshop id, folder or .mod file. xtiger_mods lists them.",
                    ),
                ),
                (
                    "path",
                    string("Only mod files whose path contains this, such as common/decisions."),
                ),
            ],
            required: &["mod_path"],
            kind: WRITE,
        },
        Tool {
            name: "xtiger_game_gap",
            title: "What the game said and Tiger did not",
            description: "Set the game's error.log against a saved validation of a mod: every log entry ends in one state (`states` adds up to `log_entries`): `matched` (Tiger reported about the same thing in that file, on the same line or within 3 lines, since the game names the line where the statement opens; near matches are counted in `matched_near_line` with examples; a tip or untidy report never matches), `same_location_other_cause`, `same_file_other_lines`, `no_tiger_report` (usually the validator is blind to that file, the likeliest source of a missing rule), `unattributed` (names no file of the mod, with the reason) or `unsupported_format` (a path it cannot place). Rows group entries by cause and keep the game's stack lines. The report says which saved run it used and whether that run still fits the files on disk (`run.files`). Play-test with ck3_run first so the log is fresh, and validate the mod so the run is current.",
            params: vec![
                (
                    "mod_path",
                    string(
                        "The mod: its name, workshop id, folder or .mod file. xtiger_mods lists them.",
                    ),
                ),
                (
                    "run_id",
                    string("Which saved run to compare with. The newest of the mod by default."),
                ),
                (
                    "name",
                    json!({"type": "string", "enum": LOGS, "default": "error", "description": "Which log."}),
                ),
                ("limit", integer("How many groups to list.", 30, 0)),
            ],
            required: &["mod_path"],
            kind: READ_ONLY,
        },
        Tool {
            name: "ck3_docs",
            title: "Look up script docs",
            description: "Look up the game's own documentation of its script: triggers, effects, event targets \
                          (scope links), on_actions, modifiers, scope types and custom localization. Each entry \
                          says what it does, its supported scopes and targets. The docs are written by the \
                          game's script_docs console command, which ck3_run can type.",
            params: vec![
                ("kind", choice("Only this kind.", &docs::KINDS.map(|(kind, _)| kind))),
                ("name", string("The exact name, such as has_trait or add_gold.")),
                (
                    "search",
                    string("Case-insensitive regex on the names and texts, such as \"opinion\"."),
                ),
                ("limit", integer("How many entries to return.", 20, 0)),
            ],
            required: &[],
            kind: READ_ONLY,
        },
        Tool {
            name: "xtiger_playsets",
            title: "List playsets",
            description: "List the playsets of the Paradox launcher, with the active one marked. With name, list \
                          the mods of that playset in load order: name, enabled, and the .mod file. Pass a \
                          playset to ck3_run to test a mod together with its mods.",
            params: vec![("name", string("A playset, to list its mods."))],
            required: &[],
            kind: READ_ONLY,
        },
    ]
}

/// Whether a tool has this name.
pub fn exists(name: &str) -> bool {
    tools().iter().any(|tool| tool.name == name)
}

/// The answer to `tools/list`.
pub fn list() -> Value {
    let tools: Vec<Value> = tools()
        .into_iter()
        .map(|tool| {
            let properties: Map<String, Value> = tool
                .params
                .into_iter()
                .chain(std::iter::once((REASON, reason())))
                .map(|(name, schema)| (name.to_owned(), schema))
                .collect();
            let annotations = match tool.kind {
                READ_ONLY => json!({"title": tool.title, "readOnlyHint": true, "openWorldHint": false}),
                LOCAL => json!({"title": tool.title, "readOnlyHint": false, "destructiveHint": false,
                                "idempotentHint": true, "openWorldHint": false}),
                WRITE => json!({"title": tool.title, "readOnlyHint": false, "destructiveHint": true,
                                "idempotentHint": true, "openWorldHint": false}),
                _ => json!({"title": tool.title, "readOnlyHint": false, "destructiveHint": false,
                            "idempotentHint": false, "openWorldHint": true}),
            };
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": {"type": "object", "properties": properties, "required": tool.required},
                "annotations": annotations,
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// The arguments of one call, with readable errors when one has the wrong type.
struct Args<'a>(&'a Map<String, Value>);

impl Args<'_> {
    fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name).filter(|value| !value.is_null())
    }

    fn string(&self, name: &str, default: &str) -> Result<String, String> {
        match self.get(name) {
            None => Ok(default.to_owned()),
            Some(Value::String(text)) => Ok(text.clone()),
            Some(_) => Err(format!("{name} must be a string")),
        }
    }

    fn boolean(&self, name: &str, default: bool) -> Result<bool, String> {
        match self.get(name) {
            None => Ok(default),
            Some(Value::Bool(value)) => Ok(*value),
            // Some clients send booleans as text.
            Some(Value::String(text)) if text == "true" || text == "false" => Ok(text == "true"),
            Some(_) => Err(format!("{name} must be true or false")),
        }
    }

    fn integer(&self, name: &str, default: u64) -> Result<u64, String> {
        match self.get(name) {
            None => Ok(default),
            Some(Value::Number(n)) => {
                n.as_u64().ok_or_else(|| format!("{name} must be a whole number, 0 or more"))
            }
            Some(Value::String(text)) => {
                text.trim().parse().map_err(|_| format!("{name} must be a whole number, 0 or more"))
            }
            Some(_) => Err(format!("{name} must be a whole number, 0 or more")),
        }
    }

    fn strings(&self, name: &str) -> Result<Vec<String>, String> {
        match self.get(name) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{name} must be a list of strings"))
                })
                .collect(),
            // A single string is taken as a list of one.
            Some(Value::String(text)) => Ok(vec![text.clone()]),
            Some(_) => Err(format!("{name} must be a list of strings")),
        }
    }
}

fn usize_of(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn json_text(value: &impl Serialize) -> Content {
    Content::Text(
        serde_json::to_string_pretty(value)
            .unwrap_or_else(|e| format!("cannot write the result: {e}")),
    )
}

fn all_mods(loc: &Locations) -> Vec<ModInfo> {
    mods::list(Some(&loc.user_dir), &loc.extra_mods)
}

fn mod_name(mods: &[ModInfo], mod_file: &std::path::Path) -> Option<String> {
    mods.iter().find(|info| info.mod_file == mod_file).map(|info| info.name.clone()).or_else(|| {
        let dir = mod_file.parent()?;
        (mod_file.file_name()? == "descriptor.mod")
            .then(|| mods::read_mod_folder(dir))?
            .map(|info| info.name)
    })
}

#[derive(Serialize)]
struct ModsResult<'a> {
    user_dir: &'a std::path::Path,
    count: usize,
    mods: Vec<&'a ModInfo>,
}

/// One side of `xtiger_compare`.
#[derive(Serialize)]
struct RunSide {
    run_id: String,
    #[serde(rename = "mod")]
    mod_file: PathBuf,
    mod_name: Option<String>,
    finished_at: u64,
    total: usize,
    by_severity: Counts,
}

impl RunSide {
    fn new(meta: runs::RunMeta, reports: &[Value]) -> Self {
        Self {
            run_id: meta.run_id,
            mod_file: meta.mod_file,
            mod_name: meta.mod_name,
            finished_at: meta.finished_at,
            total: reports.len(),
            by_severity: runs::summarize(reports, 0).by_severity,
        }
    }
}

#[derive(Serialize)]
struct CompareResult {
    older: RunSide,
    newer: RunSide,
    new: usize,
    fixed: usize,
    new_by_key: Counts,
    fixed_by_key: Counts,
    new_reports: Vec<Row>,
    fixed_reports: Vec<Row>,
}

fn by_key(reports: &[&Value]) -> Counts {
    Counts(runs::most_common(
        reports
            .iter()
            .map(|report| report.get("key").and_then(Value::as_str).unwrap_or("").to_owned()),
    ))
}

fn compare_runs(args: &Args, loc: &Locations) -> Result<Done, CallError> {
    let run_id = args.string("run_id", "")?;
    let mod_path = args.string("mod_path", "")?;
    let newer = if run_id.is_empty() && !mod_path.trim().is_empty() {
        runs::newest_run_of(loc, &mods::resolve(&mod_path, &all_mods(loc))?)?
    } else {
        runs::load_run(loc, &run_id)?
    };
    let against = args.string("against", "")?;
    let older = if against.is_empty() {
        runs::run_before(loc, &newer.meta)?
    } else {
        runs::load_run(loc, &against)?
    };
    let mut skipped = newer.skipped;
    skipped.extend(older.skipped);
    let (newer, newer_reports) = (newer.meta, newer.reports);
    let (older, older_reports) = (older.meta, older.reports);
    let setup_note = runs::setup_changed(&older.loaded_mods, &newer.loaded_mods);
    let limit = usize_of(args.integer("limit", 30)?);
    let (new, fixed) = runs::diff(&older_reports, &newer_reports);
    let summary = match (new.len(), fixed.len()) {
        (0, 0) => "Nothing changed".to_owned(),
        (n, f) => format!("{n} new, {f} fixed"),
    };
    let mut summary = summary;
    let mut content = Vec::new();
    if let Some((warning, short)) = damaged_warning(&skipped) {
        summary = format!("{summary}, {short}");
        content.push(Content::Text(warning));
    }
    let note = Note {
        summary,
        mod_ref: Some(ModRef { name: newer.mod_name.clone(), file: newer.mod_file.clone() }),
        run_id: Some(newer.run_id.clone()),
        found: None,
    };
    let result = CompareResult {
        new: new.len(),
        fixed: fixed.len(),
        new_by_key: by_key(&new),
        fixed_by_key: by_key(&fixed),
        new_reports: new.iter().take(limit).map(|report| runs::row(report)).collect(),
        fixed_reports: fixed.iter().take(limit).map(|report| runs::row(report)).collect(),
        older: RunSide::new(older, &older_reports),
        newer: RunSide::new(newer, &newer_reports),
    };
    content.insert(0, json_text(&result));
    if let Some(text) = setup_note {
        content.push(Content::Text(text));
    }
    Ok(Done { content, note })
}

/// A warning for the assistant and a short phrase for the app, when saved runs could not be read.
fn damaged_warning(damaged: &[PathBuf]) -> Option<(String, String)> {
    if damaged.is_empty() {
        return None;
    }
    let names: Vec<String> = damaged.iter().map(|path| path.display().to_string()).collect();
    let warning = format!(
        "Skipped {}: damaged, so it cannot be read. Delete it, or run xtiger_validate again.",
        names.join(", ")
    );
    Some((warning, plural(damaged.len(), "damaged run skipped", "damaged runs skipped")))
}

#[derive(Serialize)]
struct ReportsResult {
    run_id: String,
    #[serde(rename = "mod")]
    mod_file: PathBuf,
    #[serde(flatten)]
    result: QueryResult,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "No reports", or the counts by severity, worst first: "1 error, 3 warnings".
pub fn severity_phrase(by_severity: &Counts) -> String {
    let count = |name: &str| {
        by_severity.0.iter().find(|(severity, _)| severity == name).map_or(0, |(_, n)| *n)
    };
    let parts: Vec<String> = [
        ("fatal", "fatal", "fatal"),
        ("error", "error", "errors"),
        ("warning", "warning", "warnings"),
        ("untidy", "untidy", "untidy"),
        ("tips", "tip", "tips"),
    ]
    .iter()
    .filter(|(name, ..)| count(name) > 0)
    .map(|(name, one, many)| plural(count(name), one, many))
    .collect();
    if parts.is_empty() { "No reports".to_owned() } else { parts.join(", ") }
}

fn validation_phrase(summary: &Summary, since_last_run: Option<&runs::Comparison>) -> String {
    let change = match since_last_run.map(|c| (c.new, c.fixed)) {
        None => "first check".to_owned(),
        Some((0, 0)) => "nothing changed".to_owned(),
        Some((0, fixed)) => format!("{fixed} fixed"),
        Some((new, 0)) => format!("{new} new"),
        Some((new, fixed)) => format!("{new} new, {fixed} fixed"),
    };
    format!("{} · {change}", severity_phrase(&summary.by_severity))
}

fn status_phrase(status: &Value) -> String {
    let missing: Vec<&str> = [
        ("validator", "ck3-tiger"),
        ("game", "the CK3 install"),
        ("user_dir", "the CK3 user folder"),
    ]
    .iter()
    .filter(|(key, _)| status[key]["ok"] != true)
    .map(|(_, name)| *name)
    .collect();
    if missing.is_empty() {
        "Everything found".to_owned()
    } else {
        format!("Not found: {}", missing.join(", "))
    }
}

/// Find the project an `xtiger_project` call is about.
fn pick_project(args: &Args, loc: &Locations) -> Result<projects::Project, CallError> {
    let id = args.string("project", "")?;
    if !id.trim().is_empty() {
        return Ok(projects::load(loc, id.trim())?);
    }
    let all = projects::list(loc);
    let wanted = args.string("mod_path", "")?;
    if !wanted.trim().is_empty() {
        let mods = all_mods(loc);
        let file = mods::resolve(&wanted, &mods)?;
        let key = runs::mod_key(&file);
        let mut mine = all.into_iter().filter(|p| {
            runs::mod_key(&p.mod_file) == key
                || p.original_mod_file.as_deref().is_some_and(|o| runs::mod_key(o) == key)
        });
        return mine.next().ok_or_else(|| {
            CallError::from("That mod has no project. Use action create, or action list.")
        });
    }
    let mut all = all.into_iter();
    match (all.next(), all.next()) {
        (None, _) => Err("There are no projects. Use action create.".into()),
        (Some(only), None) => Ok(only),
        (Some(_), Some(_)) => {
            Err("There is more than one project: give `project`. Action list shows them.".into())
        }
    }
}

fn project_tool(args: &Args, loc: &Locations, ctx: &mut Context) -> Result<Done, CallError> {
    let action = args.string("action", "status")?;
    let by = ctx.client.unwrap_or("An AI assistant").to_owned();
    match action.as_str() {
        "list" => {
            let found = projects::list(loc);
            let summary = plural(found.len(), "project", "projects");
            let shown = json!({"count": found.len(), "projects": found});
            Ok(Done::new(vec![json_text(&shown)], summary))
        }
        "create" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            (ctx.working_on)(&mod_ref(&mods, &mod_file));
            let name = args.string("name", "")?;
            let copy_to = args.string("copy_to", "")?;
            let base = args.string("base", "")?;
            let run_id = args.string("run_id", "")?;
            let copy_to = (!copy_to.trim().is_empty()).then(|| PathBuf::from(copy_to.trim()));
            let base = (!base.trim().is_empty()).then(|| PathBuf::from(base.trim()));
            let project = projects::create(
                loc,
                &projects::Create {
                    mod_file: &mod_file,
                    name: (!name.trim().is_empty()).then(|| name.trim().to_owned()),
                    copy_to: copy_to.as_deref(),
                    old_game: base.as_deref(),
                    baseline_run: (!run_id.trim().is_empty()).then_some(run_id.trim()),
                },
            )?;
            let summary = format!("Project {} started", project.id);
            Ok(Done::new(vec![json_text(&project)], summary))
        }
        "status" => {
            let project = pick_project(args, loc)?;
            let status = projects::status(loc, &project)?;
            let summary = format!(
                "{}: {}",
                project.id,
                status.review.as_ref().map_or("no baseline", |r| r.state)
            );
            Ok(Done::new(vec![json_text(&status)], summary))
        }
        "ledger" => {
            let project = pick_project(args, loc)?;
            let entries = projects::ledger_of(loc, &project)?;
            let limit = usize_of(args.integer("limit", 25)?).max(1);
            let summary = ledger::summarize(&entries, &project.decisions, limit);
            let phrase = format!(
                "{} removed, {} changed, {} added",
                summary.counts.removed, summary.counts.changed, summary.counts.added
            );
            Ok(Done::new(vec![json_text(&summary)], phrase))
        }
        "decide" => {
            let project = pick_project(args, loc)?;
            let (project, review) = projects::decide(
                loc,
                &project.id,
                &args.string("definition", "")?,
                &args.string("decision", "")?,
                &args.string("note", "")?,
                &by,
            )?;
            let phrase = format!("{}: {}", project.id, review.state);
            Ok(Done::new(vec![json_text(&review)], phrase))
        }
        "annotate" => {
            let project = pick_project(args, loc)?;
            let message = args.string("message", "")?;
            let (project, done) = projects::annotate(
                loc,
                &project.id,
                &projects::Annotate {
                    key: &args.string("key", "")?,
                    path: &args.string("path", "")?,
                    line: usize_of(args.integer("line", 0)?),
                    message: (!message.is_empty()).then_some(message.as_str()),
                    kind: &args.string("kind", "")?,
                    note: &args.string("note", "")?,
                    by: &by,
                },
            )?;
            let phrase = format!(
                "{} noted in {}",
                plural(done.annotated, "diagnostic", "diagnostics"),
                project.id
            );
            Ok(Done::new(vec![json_text(&done)], phrase))
        }
        "pin" | "unpin" => {
            let project = pick_project(args, loc)?;
            let run_id = args.string("run_id", "")?;
            let run_id = if run_id.trim().is_empty() {
                runs::newest_in(&runs::runs_dir(loc), &project.mod_file)
                    .map(|(meta, _)| meta.run_id)
                    .ok_or("The project's mod has no saved run to pin.")?
            } else {
                run_id.trim().to_owned()
            };
            let pinned = action == "pin";
            projects::pin(loc, &project.id, &run_id, pinned)?;
            let phrase = format!("{run_id} {}", if pinned { "pinned" } else { "unpinned" });
            Ok(Done::new(vec![json_text(&json!({"run_id": run_id, "pinned": pinned}))], phrase))
        }
        "baseline" => {
            let project = pick_project(args, loc)?;
            let run_id = args.string("run_id", "")?;
            let project = projects::set_baseline(loc, &project.id, run_id.trim())?;
            let phrase = format!("{} baseline set", project.id);
            Ok(Done::new(vec![json_text(&project)], phrase))
        }
        "export" => {
            let project = pick_project(args, loc)?;
            let to = args.string("to", "")?;
            if to.trim().is_empty() {
                return Err("export needs `to`: a new or empty folder.".into());
            }
            let exported = projects::export(loc, &project, std::path::Path::new(to.trim()))?;
            let phrase = format!("{} written", plural(exported.files.len(), "file", "files"));
            Ok(Done::new(vec![json_text(&exported)], phrase))
        }
        other => Err(format!("Unknown action {other:?}.").into()),
    }
}

/// What a play-test call asks for, with the mods resolved.
struct PreparedRun {
    playset_note: String,
    commands: Vec<String>,
    play: String,
    bookmark: String,
    typing: String,
    mods: Vec<PathBuf>,
    screenshot: bool,
    keep_open: bool,
    load_timeout: u64,
    background: bool,
    /// The first mod asked for, and its folder.
    working: Option<ModRef>,
    mod_dir: Option<PathBuf>,
}

impl PreparedRun {
    fn run(&self) -> RunGame<'_> {
        RunGame {
            commands: &self.commands,
            screenshot: self.screenshot,
            bookmark: &self.bookmark,
            play: &self.play,
            keep_open: self.keep_open,
            mods: &self.mods,
            load_timeout: self.load_timeout,
            typing: &self.typing,
            background: self.background,
        }
    }
}

/// The arguments of `ck3_run` and `ck3_run_start`, checked and with the mods and the playset resolved.
fn prepare_run(args: &Args, loc: &Locations, ctx: &mut Context) -> Result<PreparedRun, CallError> {
    let mods_wanted = args.strings("mods")?;
    let known = if mods_wanted.is_empty() { Vec::new() } else { all_mods(loc) };
    let wanted: Vec<PathBuf> =
        mods_wanted.iter().map(|wanted| mods::resolve(wanted, &known)).collect::<Result<_, _>>()?;
    let working = wanted.first().map(|file| mod_ref(&known, file));
    let mod_dir = wanted.first().and_then(|file| {
        known
            .iter()
            .find(|info| info.mod_file == *file)
            .map(|info| info.dir.clone())
            .or_else(|| mods::describe(file).map(|info| info.dir))
    });
    let playset = args.string("playset", "")?;
    let mut mods = Vec::new();
    let mut playset_note = String::new();
    if !playset.trim().is_empty() {
        let found = playsets::find(&loc.user_dir, &playset)?;
        let (files, missing) = playsets::load_order(&found);
        playset_note =
            format!("Playset {}: {} loaded", found.name, plural(files.len(), "mod", "mods"));
        if !missing.is_empty() {
            let _ = write!(playset_note, "; left out, no .mod file: {}", missing.join(", "));
        }
        playset_note.push_str(".\n\n");
        mods = files;
    }
    for file in wanted {
        if !mods.contains(&file) {
            mods.push(file);
        }
    }
    if let Some(working) = &working {
        (ctx.working_on)(working);
    }
    Ok(PreparedRun {
        playset_note,
        commands: args.strings("commands")?,
        play: args.string("play", "k_france")?,
        bookmark: args.string("bookmark", "bm_867_carolingians")?,
        typing: args.string("typing", "unicode")?,
        mods,
        screenshot: args.boolean("screenshot", true)?,
        keep_open: args.boolean("keep_open", false)?,
        load_timeout: args.integer("load_timeout", 300)?.max(30),
        background: loc.game_tests_background || args.boolean("background", false)?,
        working,
        mod_dir,
    })
}

/// `ck3_job`: look at, collect or stop a play-test job.
fn job_tool(args: &Args, loc: &Locations, ctx: &mut Context) -> Result<Done, CallError> {
    let action = args.string("action", "status")?;
    let state_dir = &loc.state_dir;
    if action == "list" {
        let views: Vec<jobs::View> = jobs::list(state_dir)
            .into_iter()
            .map(|record| jobs::View::new(record, false))
            .collect();
        let summary = plural(views.len(), "job", "jobs");
        return Ok(Done::new(vec![json_text(&views)], summary));
    }
    if !["status", "result", "cancel"].contains(&action.as_str()) {
        return Err(format!("Unknown action {action:?}.").into());
    }
    let id = args.string("job_id", "")?;
    let id = if id.trim().is_empty() {
        jobs::list(state_dir)
            .into_iter()
            .next()
            .map(|record| record.id)
            .ok_or("There are no jobs on record. ck3_run_start starts one.")?
    } else {
        id.trim().to_owned()
    };
    let record = match action.as_str() {
        "cancel" => jobs::cancel(state_dir, &id)?,
        "status" => {
            let wait = Duration::from_secs(args.integer("wait", 0)?);
            (ctx.progress)(&format!("waiting for {id}"));
            jobs::status(state_dir, &id, wait, ctx.cancelled)?
        }
        _ => jobs::load(state_dir, &id).ok_or_else(|| {
            format!("No job {id:?}. ck3_job with action list shows the jobs on record.")
        })?,
    };
    let working = record.mod_ref.clone();
    let summary = format!("Job {id}: {:?}", record.state).to_lowercase();
    let mut content = Vec::new();
    let mut text = String::new();
    if action == "result" {
        if record.state == jobs::State::Running {
            text.push_str("The job is still running; there is no report yet. ");
            text.push_str("ck3_job with action status and wait follows it.\n\n");
        } else if let Some(report) = jobs::result_text(state_dir, &id) {
            let shot = record.screenshot.clone().filter(|path| path.is_file());
            let view = jobs::View::new(record, true);
            text =
                format!("{}\n\n{report}", serde_json::to_string_pretty(&view).unwrap_or_default());
            content.push(Content::Text(text));
            content.extend(shot.map(Content::Image));
            return Ok(Done {
                content,
                note: Note { summary, mod_ref: working, run_id: None, found: None },
            });
        } else {
            text.push_str("The job left no report.\n\n");
        }
    }
    let view = jobs::View::new(record, action != "status");
    text.push_str(&serde_json::to_string_pretty(&view).unwrap_or_default());
    content.push(Content::Text(text));
    Ok(Done { content, note: Note { summary, mod_ref: working, run_id: None, found: None } })
}

fn mod_ref(mods: &[ModInfo], mod_file: &std::path::Path) -> ModRef {
    ModRef { name: mod_name(mods, mod_file), file: mod_file.to_path_buf() }
}

/// Run a tool. `loc` is found again for every call, so changes in the app's settings apply at once.
pub fn call(
    name: &str,
    arguments: &Map<String, Value>,
    loc: &Locations,
    ctx: &mut Context,
) -> Result<Done, CallError> {
    let args = Args(arguments);
    match name {
        "xtiger_status" => {
            let mut status = loc.describe();
            status["xtiger_mcp"] = json!(env!("CARGO_PKG_VERSION"));
            let (waiting, open) = requests::counts(&loc.state_dir);
            status["requests"] = json!({"waiting": waiting, "open": open});
            let mut phrase = status_phrase(&status);
            if waiting > 0 {
                let _ =
                    write!(phrase, " · {}", plural(waiting, "request waiting", "requests waiting"));
            }
            Ok(Done::new(vec![json_text(&status)], phrase))
        }
        "xtiger_mods" => {
            let filter = args.string("filter", "")?.to_lowercase();
            let mods = all_mods(loc);
            let shown: Vec<&ModInfo> =
                mods.iter().filter(|info| info.name.to_lowercase().contains(&filter)).collect();
            let phrase = plural(shown.len(), "mod", "mods");
            let content = vec![json_text(&ModsResult {
                user_dir: &loc.user_dir,
                count: shown.len(),
                mods: shown,
            })];
            Ok(Done::new(content, phrase))
        }
        "xtiger_validate" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            let working = mod_ref(&mods, &mod_file);
            (ctx.working_on)(&working);
            let config = args.string("config", "")?;
            let config = (!config.trim().is_empty()).then(|| PathBuf::from(config.trim()));
            let mut with = Vec::new();
            let mut playset_note = String::new();
            let playset = args.string("playset", "")?;
            if !playset.trim().is_empty() {
                let found = playsets::find(&loc.user_dir, &playset)?;
                let (files, missing) = playsets::check_order(&found);
                // The mod that is checked is part of its own playset; it is not loaded twice.
                with = files.into_iter().filter(|file| mods::clean(file) != mod_file).collect();
                playset_note = format!(
                    "Playset {}: {} loaded with this one",
                    found.name,
                    plural(with.len(), "mod", "mods")
                );
                if !missing.is_empty() {
                    let _ =
                        write!(playset_note, "; left out, no .mod file: {}", missing.join(", "));
                }
                playset_note.push('.');
            }
            for wanted in args.strings("with")? {
                let file = mods::resolve(&wanted, &mods)?;
                if !with.contains(&file) {
                    with.push(file);
                }
            }
            let options = Options {
                show_vanilla: args.boolean("show_vanilla", false)?,
                config: config.as_deref(),
                by: Some(ctx.client.unwrap_or("An AI assistant")),
                load_dependencies: args.boolean("load_dependencies", true)?,
                with: &with,
                ..Options::default()
            };
            let result = runs::validate(
                loc,
                &mod_file,
                working.name.clone(),
                &options,
                ctx.progress,
                ctx.cancelled,
            )?;
            let note = Note {
                summary: validation_phrase(&result.summary, result.since_last_run.as_ref()),
                mod_ref: Some(working),
                run_id: Some(result.run_id.clone()),
                found: Some(Tally::from_pairs(
                    result.summary.by_severity.0.iter().map(|(name, n)| (name.as_str(), *n)),
                )),
            };
            let mut content = vec![json_text(&result)];
            if !playset_note.is_empty() {
                content.push(Content::Text(playset_note));
            }
            Ok(Done { content, note })
        }
        "xtiger_runs" => {
            let (listing, damaged) = runs::list_runs(loc);
            let mut phrase = plural(listing.len(), "saved run", "saved runs");
            let mut content = vec![json_text(&listing)];
            if let Some((warning, short)) = damaged_warning(&damaged) {
                phrase = format!("{phrase}, {short}");
                content.push(Content::Text(warning));
            }
            Ok(Done::new(content, phrase))
        }
        "xtiger_reports" => {
            let run_id = args.string("run_id", "")?;
            let mod_path = args.string("mod_path", "")?;
            let loaded = if run_id.is_empty() && !mod_path.trim().is_empty() {
                runs::newest_run_of(loc, &mods::resolve(&mod_path, &all_mods(loc))?)?
            } else {
                runs::load_run(loc, &run_id)?
            };
            let (meta, reports, skipped) = (loaded.meta, loaded.reports, loaded.skipped);
            let filter = Filter {
                pattern: args.string("pattern", "")?,
                path: args.string("path", "")?,
                severity: args.string("severity", "")?,
                key: args.string("key", "")?,
            };
            let group_by = args.string("group_by", "")?;
            let result = runs::query(
                &reports,
                &filter,
                &group_by,
                usize_of(args.integer("limit", 50)?),
                usize_of(args.integer("offset", 0)?),
            )?;
            let summary = match &result {
                QueryResult::Reports { matched, reports, .. } if reports.len() == *matched => {
                    plural(*matched, "report", "reports")
                }
                QueryResult::Reports { matched, reports, .. } => {
                    format!("{} of {}", reports.len(), plural(*matched, "report", "reports"))
                }
                QueryResult::Groups { matched, total_groups, .. } => format!(
                    "{} in {} by {group_by}",
                    plural(*matched, "report", "reports"),
                    plural(*total_groups, "group", "groups")
                ),
            };
            let mut summary = summary;
            let mut extra = Vec::new();
            if let Some((warning, short)) = damaged_warning(&skipped) {
                summary = format!("{summary}, {short}");
                extra.push(Content::Text(warning));
            }
            let note = Note {
                summary,
                mod_ref: Some(ModRef { name: meta.mod_name, file: meta.mod_file.clone() }),
                run_id: Some(meta.run_id.clone()),
                found: None,
            };
            let mut content = vec![json_text(&ReportsResult {
                run_id: meta.run_id,
                mod_file: meta.mod_file,
                result,
            })];
            content.append(&mut extra);
            Ok(Done { content, note })
        }
        "ck3_run" => {
            let prepared = prepare_run(&args, loc, ctx)?;
            let run = prepared.run();
            let mut progress = |line: &str| (ctx.progress)(line);
            let mut control = game::Control {
                cancelled: ctx.cancelled,
                progress: &mut progress,
                tick: &mut || {},
            };
            let (text, shot) = game::run_game(loc, &run, &mut control)?;
            let text = format!("{}{text}", prepared.playset_note);
            let mut summary = if run.background {
                format!("Played as {} in the background", prepared.play)
            } else {
                format!(
                    "Played as {}, {}",
                    prepared.play,
                    plural(prepared.commands.len(), "console command", "console commands")
                )
            };
            if shot.is_some() {
                summary.push_str(", screenshot taken");
            }
            let content =
                std::iter::once(Content::Text(text)).chain(shot.map(Content::Image)).collect();
            Ok(Done {
                content,
                note: Note { summary, mod_ref: prepared.working, run_id: None, found: None },
            })
        }
        "ck3_run_start" => {
            let prepared = prepare_run(&args, loc, ctx)?;
            let run = prepared.run();
            let meta = jobs::Meta {
                mod_ref: prepared.working.clone(),
                mod_dir: prepared.mod_dir.clone(),
                reason: journal::clean_reason(arguments.get(REASON)),
                client: ctx.client.map(str::to_owned),
                session: ctx.session.to_owned(),
                args: journal::brief_args(arguments),
            };
            let record = jobs::start(loc, &run, meta)?;
            let id = record.id.clone();
            let budget = record.budget_secs;
            let text = format!(
                "{}Job {id} started: {}, up to {} s. The game runs in this server; ck3_job with \
                 action status (wait up to 30 s) follows it, and action result gives the report \
                 once it is over. A client that times out in the meantime has not stopped it.\n\n{}",
                prepared.playset_note,
                if run.background {
                    "background run".to_owned()
                } else {
                    format!("{} console commands", run.commands.len())
                },
                budget,
                serde_json::to_string_pretty(&jobs::View::new(record, false)).unwrap_or_default()
            );
            Ok(Done {
                content: vec![Content::Text(text)],
                note: Note {
                    summary: format!("Job {id} started"),
                    mod_ref: prepared.working,
                    run_id: None,
                    found: None,
                },
            })
        }
        "ck3_job" => job_tool(&args, loc, ctx),
        "ck3_keys" => {
            let scancodes = args.string("scancodes", "")?;
            let text = args.string("text", "")?;
            if scancodes.trim().is_empty() && text.is_empty() {
                return Err("Give scancodes, text or both.".into());
            }
            let (out, shot) = game::send_keys(
                loc,
                &scancodes,
                &text,
                args.boolean("screenshot", true)?,
                ctx.cancelled,
            )?;
            let summary = if shot.is_some() { "Keys sent, screenshot taken" } else { "Keys sent" };
            Ok(Done::new(
                std::iter::once(Content::Text(out)).chain(shot.map(Content::Image)).collect(),
                summary,
            ))
        }
        "ck3_logs" => {
            let read = ReadLog {
                name: &args.string("name", "error")?,
                max_lines: usize_of(args.integer("max_lines", 200)?),
                dedupe: args.boolean("dedupe", true)?,
                tail: args.boolean("tail", false)?,
                pattern: &args.string("pattern", "")?,
            };
            let log = game::read_log(&loc.user_dir, &read)?;
            let first = log.lines().next().unwrap_or_default().to_owned();
            Ok(Done::new(vec![Content::Text(log)], first))
        }
        sessions::TOOL => {
            let title = journal::clean_line(arguments.get("title"), sessions::MAX_TITLE);
            let wrap_up = journal::clean_line(arguments.get("wrap_up"), sessions::MAX_WRAP_UP);
            let entries = sessions::entries_of(&loc.state_dir, ctx.session);
            let mut summary = sessions::summarize(
                &loc.state_dir,
                ctx.session,
                &entries,
                &all_mods(loc),
                journal::now_ms(),
            );
            // This call is not in the journal yet, so the summary does not know about it.
            if title.is_some() {
                summary.title.clone_from(&title);
            }
            if wrap_up.is_some() {
                summary.wrap_up.clone_from(&wrap_up);
                summary.ended = true;
            }
            let phrase = match (&wrap_up, &title) {
                (Some(wrap_up), _) => wrap_up.clone(),
                (None, Some(title)) => format!("Started: {title}"),
                (None, None) => "Summary so far".to_owned(),
            };
            let content = vec![json_text(&json!({
                "session": ctx.session,
                "title": summary.title.clone(),
                "so_far": summary,
            }))];
            Ok(Done::new(content, phrase))
        }
        "xtiger_pending_requests" => {
            let now = journal::now_ms();
            let open = requests::take(&loc.state_dir, ctx.client, ctx.session, now);
            let mod_ref = match open.as_slice() {
                [one] => Some(one.mod_ref.clone()),
                _ => None,
            };
            let summary = if open.is_empty() {
                "No requests".to_owned()
            } else {
                plural(open.len(), "open request", "open requests")
            };
            let pending: Vec<requests::Pending> =
                open.into_iter().map(|request| requests::pending(request, now)).collect();
            let content = vec![json_text(&json!({"count": pending.len(), "requests": pending}))];
            Ok(Done { content, note: Note { summary, mod_ref, ..Note::default() } })
        }
        "xtiger_finish_request" => {
            let id = args.string("id", "")?;
            let status = match args.string("outcome", "done")?.as_str() {
                "done" => Status::Done,
                "skipped" => Status::Skipped,
                _ => return Err("outcome must be done or skipped".into()),
            };
            let note = args.string("note", "")?;
            let request = requests::finish(
                &loc.state_dir,
                id.trim(),
                status,
                Some(&note),
                journal::now_ms(),
            )?;
            let summary = match status {
                Status::Skipped => {
                    format!("Skipped: {}", request.outcome.clone().unwrap_or_default())
                }
                _ => format!("Done: {}", request.outcome.clone().unwrap_or_default()),
            };
            let content = vec![Content::Text(format!(
                "Request {} is closed. The user sees it in the xTiger app.",
                request.id
            ))];
            Ok(Done {
                content,
                note: Note { summary, mod_ref: Some(request.mod_ref), ..Note::default() },
            })
        }
        "xtiger_compare" => compare_runs(&args, loc),
        "ck3_vanilla" => {
            let found = vanilla::search(
                loc.require_game()?,
                &Search {
                    name: &args.string("name", "")?,
                    pattern: &args.string("pattern", "")?,
                    path: &args.string("path", "")?,
                    limit: usize_of(args.integer("limit", 10)?),
                },
            )?;
            let summary = format!(
                "{} in {}",
                plural(found.matched, "hit", "hits"),
                plural(found.files_searched, "file", "files")
            );
            Ok(Done::new(vec![json_text(&found)], summary))
        }
        "xtiger_overrides" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            let info = mods::describe(&mod_file)
                .ok_or_else(|| format!("Cannot read {}.", mod_file.display()))?;
            let base = args.string("base", "")?;
            let base = (!base.trim().is_empty()).then(|| PathBuf::from(base.trim()));
            let report = overrides::compare_with_game(&overrides::Request {
                install: loc.require_game()?,
                mod_dir: &info.dir,
                path: &args.string("path", "")?,
                diff_lines: usize_of(args.integer("diff_lines", 20)?),
                limit: usize_of(args.integer("limit", 15)?).max(1),
                base: base.as_deref(),
            })?;
            let summary = format!(
                "{} replace a game file, {} replace a game definition",
                plural(report.same_path_files, "file", "files"),
                plural(report.same_key_definitions, "definition", "definitions")
            );
            Ok(Done::new(vec![json_text(&report)], summary))
        }
        "xtiger_project" => project_tool(&args, loc, ctx),
        "xtiger_migrate" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            let info = mods::describe(&mod_file)
                .ok_or_else(|| format!("Cannot read {}.", mod_file.display()))?;
            let report = migrate::plan(&migrate::Request {
                mod_dir: &info.dir,
                path: &args.string("path", "")?,
                limit: usize_of(args.integer("limit", 50)?).max(1),
            })?;
            let summary = format!(
                "{} to make in {}",
                plural(report.total_edits, "edit", "edits"),
                plural(report.files_scanned, "file", "files")
            );
            Ok(Done::new(vec![json_text(&report)], summary))
        }
        "xtiger_migrate_apply" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            let working = mod_ref(&mods, &mod_file);
            (ctx.working_on)(&working);
            let info = mods::describe(&mod_file)
                .ok_or_else(|| format!("Cannot read {}.", mod_file.display()))?;
            let report = migrate_apply::apply(&migrate_apply::Request {
                mod_dir: &info.dir,
                path: &args.string("path", "")?,
                backup_root: &loc.state_dir.join("migrate-backups"),
            })?;
            let mut summary = if report.changed.is_empty() {
                "Nothing written".to_owned()
            } else {
                format!(
                    "{} written to {}",
                    plural(report.total_edits, "edit", "edits"),
                    plural(report.changed.len(), "file", "files")
                )
            };
            if !report.skipped.is_empty() {
                let _ = write!(
                    summary,
                    ", {} left alone",
                    plural(report.skipped.len(), "file", "files")
                );
            }
            Ok(Done::new(vec![json_text(&report)], summary))
        }
        "xtiger_game_gap" => {
            let mods = all_mods(loc);
            let mod_file = mods::resolve(&args.string("mod_path", "")?, &mods)?;
            let info = mods::describe(&mod_file)
                .ok_or_else(|| format!("Cannot read {}.", mod_file.display()))?;
            let run_id = args.string("run_id", "")?;
            let run = if run_id.is_empty() {
                runs::newest_run_of(loc, &mod_file)?
            } else {
                runs::load_run(loc, &run_id)?
            };
            let name = args.string("name", "error")?;
            if !LOGS.contains(&name.as_str()) {
                return Err(format!("name must be one of {}", LOGS.join(", ")).into());
            }
            let log_path = loc.user_dir.join("logs").join(format!("{name}.log"));
            let bytes = std::fs::read(&log_path).map_err(|_| {
                format!("{} does not exist yet. Play-test with ck3_run first.", log_path.display())
            })?;
            let log = String::from_utf8_lossy(&bytes);
            let mut report = gap::compare(&gap::Request {
                log: &log,
                mod_dir: &info.dir,
                reports: &run.reports,
                limit: usize_of(args.integer("limit", 30)?),
            });
            let now = projects::manifest_hash(&info.dir);
            report.run = Some(gap::RunBinding {
                run_id: run.meta.run_id.clone(),
                files: match (run.meta.snapshot.as_deref(), now.as_deref()) {
                    (Some(then), Some(now)) if then == now => "current",
                    (Some(_), _) => "stale",
                    (None, _) => "unknown",
                },
            });
            let summary = format!(
                "{} about the mod, {} matched by Tiger, {} not ({} in files Tiger said nothing about)",
                report.about_the_mod,
                report.reported_by_tiger,
                report.gaps,
                report.files_without_any_report
            );
            let text = format!(
                "Compared with run {} of {}.
{}",
                run.meta.run_id,
                run.meta.mod_name.as_deref().unwrap_or("the mod"),
                serde_json::to_string_pretty(&report).unwrap_or_default()
            );
            Ok(Done::new(vec![Content::Text(text)], summary))
        }
        "ck3_docs" => {
            let found = docs::lookup(
                &loc.user_dir,
                &Lookup {
                    kind: &args.string("kind", "")?,
                    name: &args.string("name", "")?,
                    search: &args.string("search", "")?,
                    limit: usize_of(args.integer("limit", 20)?),
                },
            )?;
            let summary = plural(found.matched, "entry", "entries");
            Ok(Done::new(vec![json_text(&found)], summary))
        }
        "xtiger_playsets" => {
            let name = args.string("name", "")?;
            if name.trim().is_empty() {
                let list: Vec<Value> = playsets::list(&loc.user_dir)?
                    .iter()
                    .map(|p| json!({"name": p.name, "active": p.active, "mods": p.mods.len(), "enabled": p.enabled}))
                    .collect();
                let summary = plural(list.len(), "playset", "playsets");
                return Ok(Done::new(vec![json_text(&json!({"playsets": list}))], summary));
            }
            let playset = playsets::find(&loc.user_dir, &name)?;
            let summary = format!(
                "{}: {}",
                playset.name,
                plural(playset.enabled, "mod enabled", "mods enabled")
            );
            Ok(Done::new(vec![json_text(&playset)], summary))
        }
        _ => Err(CallError::Unknown(name.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::{AppSettings, Sources};
    use crate::testing::TempDir;
    use std::collections::HashMap;
    use std::fs;

    fn locations(tmp: &TempDir) -> Locations {
        let mut env = HashMap::new();
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

    fn run(name: &str, arguments: &Value, loc: &Locations) -> Result<Done, CallError> {
        let mut progress = |_: &str| {};
        let mut working_on = |_: &ModRef| {};
        let mut ctx = Context {
            progress: &mut progress,
            cancelled: &|| false,
            working_on: &mut working_on,
            session: "test",
            client: Some("Test Client"),
        };
        call(name, arguments.as_object().unwrap(), loc, &mut ctx)
    }

    fn text(result: Result<Done, CallError>) -> String {
        match result.unwrap().content.remove(0) {
            Content::Text(text) => text,
            Content::Image(_) => panic!(),
        }
    }

    #[test]
    fn every_tool_is_listed_with_a_schema() {
        let list = list();
        let tools = list["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 22);
        for tool in tools {
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["description"].as_str().unwrap().len() > 40);
            assert!(tool["annotations"]["title"].is_string());
        }
        assert_eq!(tools[2]["inputSchema"]["required"], json!(["mod_path"]));
    }

    #[test]
    fn unknown_tool_and_bad_arguments() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        assert_eq!(run("nope", &json!({}), &loc).unwrap_err(), CallError::Unknown("nope".into()));
        let err = run("ck3_logs", &json!({"max_lines": -1}), &loc).unwrap_err();
        assert_eq!(err, CallError::Failed("max_lines must be a whole number, 0 or more".into()));
        let err = run("ck3_logs", &json!({"dedupe": 3}), &loc).unwrap_err();
        assert_eq!(err, CallError::Failed("dedupe must be true or false".into()));
        assert!(
            text(run("ck3_logs", &json!({"max_lines": "5", "dedupe": "false"}), &loc))
                .contains("does not exist")
        );
    }

    #[test]
    fn mods_and_reports_without_runs() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        fs::create_dir_all(tmp.join("user/mod")).unwrap();
        fs::write(tmp.join("user/mod/silk.mod"), "name=\"Silk Road\"\npath=\"mod/silk\"\n")
            .unwrap();
        let mods: Value =
            serde_json::from_str(&text(run("xtiger_mods", &json!({"filter": "SILK"}), &loc)))
                .unwrap();
        assert_eq!(mods["count"], 1);
        assert_eq!(mods["mods"][0]["name"], "Silk Road");
        let err = run("xtiger_reports", &json!({}), &loc).unwrap_err();
        assert!(matches!(err, CallError::Failed(m) if m.contains("No results yet")));
        let err = run("xtiger_validate", &json!({"mod_path": "Nope"}), &loc).unwrap_err();
        assert!(matches!(err, CallError::Failed(m) if m.contains("No mod is called")));
        let err = run("xtiger_validate", &json!({"mod_path": "Silk", "playset": "Any"}), &loc)
            .unwrap_err();
        assert!(matches!(err, CallError::Failed(m) if m.contains("No launcher database")));
    }

    #[test]
    fn requests_are_picked_up_and_closed() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let new = requests::NewRequest {
            kind: requests::Kind::Fix,
            mod_file: PathBuf::from("/m/silk.mod"),
            mod_name: Some("Silk Road".to_owned()),
            game_version: None,
            note: Some("Keep the names".to_owned()),
            reports: vec![json!({"severity": "error", "key": "missing-item", "message": "x",
                                 "locations": [{"path": "events/a.txt", "linenr": 4}]})],
            brief: None,
        };
        let id = requests::add(&loc.state_dir, new, journal::now_ms()).unwrap().id;
        assert!(
            run("xtiger_status", &json!({}), &loc)
                .unwrap()
                .note
                .summary
                .ends_with("1 request waiting")
        );
        let done = run("xtiger_pending_requests", &json!({}), &loc).unwrap();
        assert_eq!(done.note.summary, "1 open request");
        assert_eq!(done.note.mod_ref.as_ref().unwrap().name.as_deref(), Some("Silk Road"));
        let listed: Value = serde_json::from_str(&text(Ok(done))).unwrap();
        assert_eq!(listed["requests"][0]["reports"][0]["where"], "events/a.txt:4");
        assert_eq!(requests::list(&loc.state_dir)[0].taken_by.as_deref(), Some("Test Client"));
        let err =
            run("xtiger_finish_request", &json!({"id": id, "outcome": "maybe", "note": "x"}), &loc);
        assert!(err.is_err());
        let done =
            run("xtiger_finish_request", &json!({"id": id, "note": "Renamed the item."}), &loc);
        assert_eq!(done.unwrap().note.summary, "Done: Renamed the item.");
        assert!(text(run("xtiger_pending_requests", &json!({}), &loc)).contains("\"count\": 0"));
    }

    #[test]
    fn runs_are_compared() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let report = |key: &str| json!({"severity": "error", "key": key, "message": key, "locations": [{"path": "a.txt"}]});
        let dir = runs::runs_dir(&loc);
        let silk = std::path::Path::new("/m/silk.mod");
        runs::save_test_run(&dir, "20260101-000000-silk", silk, &[report("a"), report("b")]);
        runs::save_test_run(&dir, "20260101-000001-other", std::path::Path::new("/m/o.mod"), &[]);
        runs::save_test_run(&dir, "20260101-000002-silk", silk, &[report("b"), report("c")]);
        let result: Value =
            serde_json::from_str(&text(run("xtiger_compare", &json!({}), &loc))).unwrap();
        assert_eq!(result["older"]["run_id"], "20260101-000000-silk");
        assert_eq!(result["newer"]["total"], 2);
        assert_eq!((result["new"].clone(), result["fixed"].clone()), (json!(1), json!(1)));
        assert_eq!(result["new_reports"][0]["key"], "c");
        assert_eq!(result["fixed_by_key"], json!({"a": 1}));
        let err =
            run("xtiger_compare", &json!({"run_id": "20260101-000000-silk"}), &loc).unwrap_err();
        assert!(matches!(err, CallError::Failed(m) if m.contains("oldest")));
        let other = json!({"run_id": "20260101-000002-silk", "against": "20260101-000001-other"});
        assert_eq!(run("xtiger_compare", &other, &loc).unwrap().note.summary, "2 new, 0 fixed");
    }

    #[test]
    fn migrate_apply_writes_and_backs_up() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        fs::create_dir_all(tmp.join("user/mod/silk/events")).unwrap();
        fs::write(tmp.join("user/mod/silk.mod"), "name=\"Silk Road\"\npath=\"mod/silk\"\n")
            .unwrap();
        fs::write(tmp.join("user/mod/silk/events/a.txt"), "e = { is_created = yes }\n").unwrap();
        let args = json!({"mod_path": "Silk Road"});
        let done = run("xtiger_migrate_apply", &args, &loc).unwrap();
        assert_eq!(done.note.summary, "1 edit written to 1 file");
        let result: Value = serde_json::from_str(&text(Ok(done))).unwrap();
        let backup = PathBuf::from(result["backup"].as_str().unwrap());
        assert!(backup.starts_with(loc.state_dir.join("migrate-backups")));
        assert_eq!(
            fs::read_to_string(backup.join("events/a.txt")).unwrap(),
            "e = { is_created = yes }\n"
        );
        assert_eq!(
            fs::read_to_string(tmp.join("user/mod/silk/events/a.txt")).unwrap(),
            "e = { is_title_created = yes }\n"
        );
        let again = run("xtiger_migrate_apply", &args, &loc).unwrap();
        assert_eq!(again.note.summary, "Nothing written");
    }

    #[test]
    fn status_reports_where_things_came_from() {
        let tmp = TempDir::new();
        let status: Value =
            serde_json::from_str(&text(run("xtiger_status", &json!({}), &locations(&tmp))))
                .unwrap();
        assert_eq!(status["user_dir"]["from"], "CK3_USER_DIR");
        assert_eq!(status["validator"]["ok"], false);
        assert_eq!(status["xtiger_mcp"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn jobs_are_read_by_id_or_as_the_newest_and_a_missing_one_is_said_so() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        let err = run("ck3_job", &json!({}), &loc).unwrap_err();
        assert_eq!(
            err,
            CallError::Failed("There are no jobs on record. ck3_run_start starts one.".into())
        );
        assert_eq!(text(run("ck3_job", &json!({"action": "list"}), &loc)), "[]");
        let err =
            run("ck3_job", &json!({"action": "status", "job_id": "job-1"}), &loc).unwrap_err();
        assert!(
            matches!(&err, CallError::Failed(message) if message.contains("No job")),
            "{err:?}"
        );
        let err = run("ck3_job", &json!({"action": "dance", "job_id": "job-1"}), &loc).unwrap_err();
        assert!(matches!(&err, CallError::Failed(message) if message.contains("Unknown action")));
    }

    #[test]
    fn a_job_that_cannot_start_is_refused_before_it_is_one() {
        let tmp = TempDir::new();
        let loc = locations(&tmp);
        // The same checks as ck3_run, so no job is left behind to find.
        let err = run("ck3_run_start", &json!({"commands": ["a\nb"]}), &loc).unwrap_err();
        assert!(matches!(err, CallError::Failed(_)));
        assert_eq!(text(run("ck3_job", &json!({"action": "list"}), &loc)), "[]");
    }
}
