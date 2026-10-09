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
| `xtiger_migrate` | Dry run of the mechanical renames needed to move a mod to 1.20: file, line, and the line as it would read. Writes nothing |
| `xtiger_game_gap` | Sets the game's `error.log` against a saved run: log entries about the mod's files that Tiger did not report, grouped by cause. The holes in the validator |
| `xtiger_session` | Names the job at hand, or wraps it up, and sums up the session so far |
| `xtiger_pending_requests` | Lists what the user asked for in the app and is not done yet: reports to fix, or a mod to update |
| `xtiger_finish_request` | Closes a request as done or skipped, with a note the user sees in the app |
| `xtiger_playsets` | Lists the launcher's playsets, or the mods of one in load order |
| `ck3_run` | Starts CK3, loads a bookmark, types console commands, takes a screenshot, reports `error.log` and closes the game. With `playset`, the mods of that playset are loaded too |
| `ck3_keys` | Sends keys or text to a running game and returns a screenshot |
| `ck3_logs` | Reads the start or end of a CK3 log, optionally filtered by a regex |
| `ck3_vanilla` | Searches the base game's files: where a trigger, effect, event, decision, GUI type or localization key is defined, or the lines that match a regex |
| `ck3_docs` | Looks up the game's own script docs: triggers, effects, event targets, `on_actions`, modifiers, scopes and custom localization |

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

If another window takes focus while `ck3_run` is typing, it stops and tells you what was sent. Old
logs are moved to `logs/xtiger-archive`, never deleted.

## Building

```bash
cargo build --release -p ck3-tiger -p xtiger-mcp
cargo test -p xtiger-mcp
```
