# xTiger MCP server

`xtiger-mcp` lets an AI assistant such as Claude use xTiger for you. The assistant can:

- validate your mod;
- dig through the reports;
- start CK3 and type console commands;
- look at screenshots of the game.

It's a single program with nothing else to install. It comes with the xTiger app and the release
archives, next to `ck3-tiger`.

## Connect an assistant

**With the app.** Open Settings → AI assistants and press **Connect** next to your assistant.

The app finds Claude Desktop, Claude Code, Cursor, VS Code and Windsurf, and adds xTiger to their
settings. It doesn't touch anything else in those files and keeps a backup next to each one.

For any other assistant, press **Copy settings** and paste the text into its MCP servers.

**By hand.** Register the program with its full path. For Claude Code:

```bash
claude mcp add xtiger -- "<xTiger folder>/xtiger-mcp.exe"
```

Most other assistants take a JSON entry like this one:

```json
{
  "mcpServers": {
    "xtiger": { "command": "<xTiger folder>/xtiger-mcp.exe" }
  }
}
```

Run `xtiger-mcp --status` to see what the server found and where each answer came from.

## What it finds on its own

The server doesn't need any settings. It looks again on every call, so a change in the app takes
effect right away. For each item, the first match wins.

| | Where it looks |
|---|---|
| Validator | `XTIGER_BIN`, then next to `xtiger-mcp`, then the installed app, then `PATH` |
| Game | `CK3_GAME_DIR`, then the folder picked in the xTiger app, then Steam |
| CK3 user folder | `CK3_USER_DIR`, then the folder picked in the app, then your Documents folder |
| Saved runs and the journal | `XTIGER_STATE_DIR`, then the app's data folder, then `~/.xtiger` |

A debug build of the server prefers the release build of the validator from the same checkout.

The variables only override the search. A variable that points to the wrong place is reported as
an error, never quietly ignored.

## Tools

| Tool | What it does |
|---|---|
| `xtiger_status` | Shows what was found: the validator, the game and its version, and the user folder |
| `xtiger_mods` | Lists the mods: local, Workshop and folders added in the app |
| `xtiger_validate` | Validates a mod by name, folder or `.mod` file, loading the mods it depends on (or the mods of a `playset`, or the ones in `with`). Sums up the reports and says what is new or fixed since the last run |
| `xtiger_runs` | Lists the saved runs, newest first |
| `xtiger_reports` | Filters or groups the reports of a saved run by regex, file, severity or key |
| `xtiger_compare` | Compares two saved runs: the totals of each, and which reports are new or fixed, by key and one by one |
| `xtiger_overrides` | Shows what a mod overrides in the base game and how each copy differs from the game installed now, near copies first |
| `xtiger_project` | A durable record for updating one mod: baseline, working copy, runs pinned beyond the newest 20, stale-result detection, a ledger of removed or changed definitions (script, localization keys, gui templates and widgets, gfx sprites) that need a decision, notes on diagnostics that never hide them, an update status by stage (a clean static check is not a tested update), and an evidence export |
| `xtiger_migrate` | Dry run of the mechanical renames needed to move a mod to 1.20: file, line, and the line as it would read. Writes nothing |
| `xtiger_migrate_apply` | Writes the renames `xtiger_migrate` lists into the mod, after copying each changed file to a backup folder outside the mod |
| `xtiger_game_gap` | Sets the game's `error.log` against a saved run: log entries about the mod's files that Tiger did not report, grouped by cause. The holes in the validator |
| `xtiger_session` | Names the job at hand, or wraps it up, and sums up the session so far |
| `xtiger_pending_requests` | Lists what the user asked for in the app and is not done yet: reports to fix, or a mod to update |
| `xtiger_finish_request` | Closes a request as done or skipped, with a note the user sees in the app |
| `xtiger_playsets` | Lists the launcher's playsets, or the mods of one in load order |
| `ck3_run` | Starts CK3, loads a bookmark, types console commands, takes a screenshot, reports `error.log` and closes the game. With `playset`, the mods of that playset are loaded too |
| `ck3_run_start` | The same play-test as `ck3_run`, started as a job: it answers at once with a job id and the game keeps running in the server, for runs longer than the client is willing to wait (a long list of commands, `load_timeout` of 480) |
| `ck3_job` | Reads a job: `status` (state, phase, latest progress line, elapsed time, commands typed; `wait` holds the call up to 30 seconds), `result` (the report and screenshot), `cancel` or `list`. Jobs are kept on disk |
| `ck3_keys` | Sends keys or text to a running game, as one action or a list of `steps`, and returns a screenshot (also when another window covers the game; no keys means a screenshot only). A resident helper keeps each step to a fraction of a second, and `wait_for` ends a step as soon as a matching line reaches the game's log |
| `ck3_script` | Runs a list of effects on the player in an open game: writes them to the `run` folder, types the console's `run` for them and waits for the game to finish the file. Returns the script's `debug_log` lines, or the `error.log` lines when the game rejects it. `with` sets up scope fixtures first (see below) |
| `ck3_cases` | Runs up to 25 effect scripts one after the other in an open game and says for each what it printed and which new errors it caused. The game's noise (what it logs while it describes effects, warnings it repeats on every run) is left out, so what is left is what the case did wrong. Stops at the first case that does not finish |
| `ck3_events` | Fires events one after the other in an open game, by id or by sweeping the events a mod defines (`mod_path`, `filter`, `skip`, `max`). When the game says an event needs a scope that is not set, it makes one by the scope's name and fires the event again. Says per event: ok, silent (nothing logged), ERRORS or NEEDS SCOPES. The events really run in the open game: use a test game |
| `ck3_click` | Clicks the mouse in an open game at a point given as a fraction of the window (from a screenshot), for what keys cannot reach: an event option, a button |
| `ck3_logs` | Reads the start or end of a CK3 log, optionally filtered by a regex. `since` returns only what the game wrote after an earlier read (`"end"` sets the marker) |
| `ck3_vanilla` | Searches the base game's files: where a trigger, effect, event, decision, GUI type or localization key is defined, or the lines that match a regex |
| `ck3_docs` | Looks up the game's own script docs: triggers, effects, event targets, `on_actions`, modifiers, scopes and custom localization |

**Fixtures.** `ck3_script`, `ck3_cases` and `ck3_events` take `with`, a list of `kind:name` (or
`kind:name=value`) that set up scopes before the effects, so an event that needs `scope:friend` gets one:
`character:N` (a new character, age 30; `=age` sets it), `courtier:N` (employed by the player, age 30),
`child:N` (the player's child, age 5), `self:N` (the player), `capital:N`, `county:N`, `title:N`,
`house:N`, `dynasty:N`, `culture:N`, `faith:N` (the player's own) and `value:N=number` (a saved
number). Each run of a character fixture creates a character in the game, so use a test game. The
fixtures sit in the first lines of the file, so the line numbers the game reports are shifted by that
many; the report says so.

**Screenshots.** The tools that take a screenshot (`ck3_keys`, `ck3_click`, `ck3_script`, `ck3_cases`,
`ck3_events`) accept `shot_region` (`[left, top, right, bottom]`, fractions of the window) and
`shot_scale` (0.1 to 1) to keep only the part that matters and shrink it. At `shot_scale` 0.4 a whole
window is still readable, at a fraction of the cost.

**What a sweep cannot tell.** The game logs many errors once, so an event that fails the first time can come
back clean the second time in the same game: restart the game to repeat a check. An event whose
window opens is reported as silent unless it logs something: look at the window with `ck3_keys` and pick an
option with `ck3_click`. Real run on a flavour mod: 57 events in four calls, the scopes that its event texts
needed were made on their own.

**Editing a mod while the game runs.** With the game started by `ck3_run` (debug mode) and the mod loaded from its
folder, the game picks up edits by itself within a few seconds: events (including new events in an
existing file and new files), scripted effects and localization. No console command is needed (`reload`
only covers assets), and syntax errors in the edited files reach `error.log` within seconds. So the
loop is: edit, wait a moment, `ck3_events` or `ck3_script`, read the errors.

There are also two prompts:

- `fix_mod` validates a mod and fixes what it finds.
- `update_mod` brings a mod up to the current game version.

Long validations send progress updates, and the assistant can cancel them.

`ck3_docs` reads the logs that the game's `script_docs` console command writes. If they are
missing, it says so, and `ck3_run` with `commands: ["script_docs"]` writes them.

## Requests from the app

In the app, **Ask AI to fix** on a report, and **Prepare update report** on the results of a mod,
leave a request for the assistant: the reports to fix and a note, or a brief of what it takes to
bring the mod up to the current game version. Each request is one JSON file in `requests` in the state folder.

The assistant picks them up with `xtiger_pending_requests` and closes them with
`xtiger_finish_request`. `xtiger_status` says how many are waiting. The app shows each request
as waiting, picked up, done or skipped, with the assistant's note.

## One history

The app saves its checks in `runs` in the state folder too, next to the assistant's. So
`since_last_run`, `xtiger_compare` and the app's "new" marks all measure against the last check
of the mod, whoever ran it, and `xtiger_runs` says who ran each one: the assistant's name, or
`xTiger app`.

Every tool also takes an optional `reason`: one short sentence on why the assistant makes the
call. The server's instructions ask for it on every call.

## Activity journal

Each call is written as one JSON line to `activity.jsonl` in the state folder: when it started
and how long it took, the tool, the assistant, the reason, the mod, the arguments (long values
cut short), the outcome (`ok`, `error` or `cancelled`) and a one-line summary such as
`2 errors, 3 warnings · 1 new, 4 fixed`. Past 512 KB the file is moved to `activity.1.jsonl`,
replacing the one before, so the journal never takes more than about 1 MB.

A call that is still running has its own small file in `activity-running`, refreshed every two
seconds with its latest progress line and removed when the call ends. The app's AI activity
screen reads both.

Each entry also has a `session`: the calls of one server for one job. A new session starts with
the first call, after `xtiger_session` wraps one up, when it names a different job, or after 30
minutes without calls. A validation also keeps what it `found`, by severity, so the app can draw
how the reports went down from one check to the next. The session summary that `xtiger_session`
returns, and the app shows, comes from the journal, the saved runs and the files of the mod that
changed during the session.

The game tools need Windows. CK3 ignores ordinary key events, so the scripts in
[`scripts`](scripts):

- send hardware scancodes;
- type commands as Unicode text, which works with any keyboard layout.

If another window takes focus while `ck3_run` is typing, it stops and tells you what was sent.

The xTiger app's Settings has a "Game tests" choice. By default `ck3_run` runs in the background: the
game starts minimized at idle priority, nothing is typed, no screenshot is taken and the window never
takes the focus, so the user can keep playing something else. The mod under test has to start what it
checks itself (for example an `on_action` chained from `on_game_start_after_lobby`), and the result is
read with `ck3_logs`. Console commands need the other choice, "Take over the screen". Passing
`background` to `ck3_run` turns the quiet mode on even then. Old
logs are moved to `logs/xtiger-archive`; the runner keeps the ten most recent archives by default.

Each command must be one non-empty line of at most 255 UTF-16 code units. Longer commands and
control characters are rejected before CK3 starts. Use a bookmark key defined by the game or the
requested mods, such as `bm_867_carolingians`; a date is not a bookmark key. A run fails if any
requested command is unconfirmed in `debug.log`, or if its report is missing. Confirmation means
the console accepted the command; review the reported game errors to verify its script behavior.

A client that stops waiting (most give up after about five minutes) has not stopped the game. For a run that
may take longer, start it with `ck3_run_start` and read it with `ck3_job`: the job is recorded in the state
folder (`jobs`), survives the client's timeout, and says whether the mod's files are still the ones it was
started with. A job whose server stopped is shown as interrupted, and the next run puts the mod list back.
A timeout on the client's side is not a game failure; look at the job before running again.

## Building

```bash
cargo build --release -p ck3-tiger -p xtiger-mcp
cargo test -p xtiger-mcp
```
