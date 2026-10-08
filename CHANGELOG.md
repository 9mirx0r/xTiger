# Changelog

## xTiger 1.0 alpha "Frankokratia"

The first release of xTiger, a fork of [Tiger](https://github.com/amtep/tiger) made for
Crusader Kings III 1.20 only. It comes in three parts:

- **The xTiger app**, new: a desktop window for checking your mods without a terminal.
- **The validator** (`ck3-tiger`), the command-line program that does the checking.
- **The MCP server**, which lets an AI assistant such as Claude run the validator and the game for you.

This is an alpha. Everything below works and was tested, but expect rough edges, and please
report them in the [issues](https://github.com/9mirx0r/xTiger/issues).

### At a glance: upstream Tiger 1.19 against xTiger 1.0 alpha

| | Tiger 1.19 (before) | xTiger 1.0 alpha (now) |
|---|---|---|
| Game | CK3 1.19 and four other Paradox games | CK3 1.20 only |
| Reports on unmodded vanilla 1.20 | about 144,000 | about 15,000 |
| Relaxed checks | by guess | each one confirmed in the game's own `error.log` |
| Same mod, same reports | scope reports could change between runs | always the same |
| Hide what you already know | no | `--suppress <file>` |
| Mod with missing dependencies | hundreds of unexplained reports | one warning per dependency saying why |
| Window to check mods | no, terminal only | the xTiger app, with installer and portable zip |
| AI assistants | no | MCP server: validate, read reports, run the game, read logs |
| Updates | manual | the app checks and installs new releases |

### At a glance: the earlier xTiger preview against this release

The earlier preview was the first public xTiger, made of the validator and a Python MCP server.

| | Earlier preview | xTiger 1.0 alpha |
|---|---|---|
| Game version | CK3 1.20.0.2 | CK3 1.20.0.4 |
| Desktop app | none | yes, with installer, portable zip and self-update |
| MCP server | Python, needed a Python setup and environment variables | one program, `xtiger-mcp`, nothing to install, finds the game and mods by itself |
| MCP tools | 6: validate, reports, runs, `ck3_run`, `ck3_logs`, `ck3_keys` | 15 tools, adding mods, playsets, compare, vanilla and docs lookup, sessions and requests |
| `ck3_run` and mods | mods passed to a run were ignored by the game, so the mod had to be enabled in the launcher first | loaded through the launcher's `dlc_load.json`, put back afterwards, and checked in the log |
| Console commands | the first one could be lost or broken on some layouts | the console gets time to open, stray characters are cleared, each command is confirmed in `debug.log` |
| Same mod, same reports | scope reports could differ between runs | always identical |
| Choosing a mod | the `.mod` file or its folder | by name, folder, workshop id or `.mod` file |
| Missing dependencies | hundreds of unexplained reports | one warning per dependency saying why |
| What the assistant did | not recorded | a journal, work sessions and a live activity screen in the app |
| Asking the assistant | not possible | "Ask AI to fix" and "Prepare update report" in the app |
| Known false reports | `scope:ruler` and `scope:character` in `court_scene` blocks | fixed |

### The xTiger app

A desktop app for Windows, with Qubis, the pixel-art reviewer, as your guide.

**Getting started**

- On first start it finds CK3 through Steam and your Paradox documents folder by itself, and
  shows what it found. Either folder can be changed by hand.
- If it cannot find the game, it says so and lets you pick the folder. Picking the `game`
  subfolder by mistake also works.

**Your mods**

- Every mod in your Paradox `mod` folder and every Workshop mod is shown as a card, with its
  picture, version, supported game version and the result of its last check.
- Filter by Local or Workshop, or search by name.
- Mods kept somewhere else can be added with "Add mod folder…" and removed again.
- Double-click a card, or select it and press Validate, to check it.

**Checking a mod**

- The validator runs in its own process, so the app stays responsive and a check can be
  cancelled at any time.
- A progress bar estimates the time left from how long the last check of that mod took, and
  the validator's own messages scroll by underneath.

**Results**

- Reports are grouped by file, by kind or by severity, and each group can be collapsed.
- Filter by severity (Errors, Warnings, Untidy, Tips), by text, or show only the reports that
  are new since the last check. New reports carry a NEW tag.
- Selecting a report shows the message, the hint, the line of code with the problem
  underlined, and every other place involved.
- Open the file at the right line in VS Code (or in your default program), show it in its
  folder, or copy the report, its location or its wiki link.
- Keyboard: ↑ ↓ or j k to move, Enter to open, Ctrl+F to search, Esc to clear.
- Export all reports as JSON or as text.
- A mod with no reports gets an "All clear" screen with the time taken and how many reports
  were fixed since the last check.

**AI assistants**

- Settings → AI assistants connects xTiger to Claude Desktop, Claude Code, Cursor, VS Code and
  Windsurf with one click, and shows which ones are connected.
- Only the xTiger entry in the assistant's settings file changes. The rest stays as it was, and
  a backup is kept next to the file. A file with comments is never rewritten: the app offers the
  text to paste by hand instead.
- An assistant still set up with another copy of xTiger is shown as such and can be switched to
  this one.
- The AI activity screen follows what the assistants do with xTiger, live: what is running now
  and its latest progress line, then every earlier call in plain words, with the assistant's
  reason, the outcome and the time. One click opens the results of a check the assistant ran.
  The screen only reads the journal while it is open and the window is visible.
- The calls are grouped in work sessions, such as "Update Better Courts to 1.20", each with the
  assistant, the time it took and whether it is still going. A small chart shows how the reports
  went down from one check to the next (41 → 18 → 9 → 0), and a summary says what was fixed,
  what is left to fix (one click opens it in the editor) and which files of the mod changed.
- **Ask AI to fix** on a report leaves a request for the assistant: that report, every report
  of the same kind, or all the reports shown, with an optional note.
- **Prepare update report** writes a brief of what it takes to bring a mod up to the current
  game version: the versions, the reports by kind with examples, and the files most affected.
  It can be copied, saved, or handed to the assistant as a request.
- The AI activity screen lists your requests as waiting, picked up, done or skipped, with the
  assistant's note. A waiting request can be taken back.
- The app's checks are saved with the assistant's, in one history: what is new is measured
  against the last check of the mod, whoever ran it, and the assistant can compare a check made
  in the app with its own.

**Look and feel**

- Dark and light themes, or follow the Windows setting.
- Smooth transitions between screens and small animations throughout: counters that count
  up, cards that lift on hover, a pill that slides between tabs, and Qubis reacting to the
  results. All motion is turned off when Windows is set to reduce animations.

**Updates**

- The app looks for a new xTiger release when it opens and every hour after. Nothing is shown
  while it is up to date; when a release appears, an "Update to …" button shows up in the
  title bar.
- It opens the notes of every release since your version. "Update now" downloads the new
  setup (checked against GitHub's checksum), installs it, and opens xTiger again by itself.
  Your settings and history are kept.
- After an update, a "What's new" window shows the notes of the new version once.
- "Skip this version" stops the reminders for that release, and checking can be turned off in
  Settings. A portable copy links to the release page instead of installing.
- Bundled fonts: Geist, Geist Mono and Pixelify Sans, all under the SIL Open Font License.
- Its own frameless window with the xTiger icon.

**Installing**

- `xTiger_<version>_x64-setup.exe`: xTiger's own installer, in the same style as the app,
  with Qubis showing the way. It needs no administrator rights, adds Start menu and desktop
  shortcuts, and also updates an older version in place. xTiger shows up in Windows' list of
  installed apps, and uninstalling it can keep or delete your settings. `--silent` installs
  without a window.
- `xTiger_<version>_x64_portable.zip`: unzip anywhere and run `xTiger.exe`. The portable copy
  keeps its settings and history in a `data` folder next to it, so the whole folder can be
  carried around. Deleting `portable.txt` makes it behave like the installed app.

### The validator

**CK3 only**

- The code for Victoria 3, Imperator, EU5 and Hearts of Iron IV has been removed. xTiger builds,
  tests and releases CK3 only. Use upstream Tiger for the other games.

**Updated for CK3 1.20**

- Tables, defines, triggers, effects and fields are updated for CK3 1.20.0.4, so the "please
  update" notice is gone.
- Defines synced with the game (+86 / -12), plus fixes to the trigger and effect tables,
  `compare_modifier` targets, `pay_short_term_treasury`, clergy trait modifiers, lease contracts,
  icon parsing and more.

**Fewer false reports, checked against the game**

- Every relaxed rule was checked in the game itself: a test mod uses the field once correctly
  and once with a typo, and the game's `error.log` decides. Typos are still reported, real
  fields are accepted.
- Newly accepted, all confirmed in game: gene `visible`, morph template `set_tags`, decal
  `required_tags`, DNA `portrait_info` `type` and `id`, pdxmesh `streaming`, epidemic intensity
  `notification` blocks, portrait override `colors`, and the GUI properties `wrap_basedon`,
  `overflowed_items_visibility` and `animated_progress_value`.
- Keys written in the wrong case are reported as untidy instead of as errors, because the game
  accepts them.
- `scope:ruler` and `scope:character` are no longer reported as unknown inside `court_scene`
  blocks, where the game sets them.
- `current_year` compared against a date now gives a warning, because it takes a year.
- Unmodded vanilla 1.20 gives about 15,000 reports, down from about 17,000. Upstream Tiger
  gives about 144,000. Unknown-field reports went from 40 to 9.

**Same mod, same reports**

- Some scope reports used to change from run to run. Now the same files always give the same
  reports, which makes comparing runs reliable.
- `--suppress <file>` hides the reports listed in an earlier `--json` output, so you see only
  what is new since then.

**Other**

- A mod that lists `dependencies` in its descriptor gets a warning (`packaging`) for each one that
  is not loaded in the check. Everything such a mod defines is reported as missing, and the
  warning now says why instead of leaving hundreds of reports unexplained.
- `ck3-tiger update` downloads releases from this repository instead of upstream Tiger.
- The release archives include the MCP server.

### The MCP server

Lets an AI assistant validate your mod, start CK3, type console commands and read the logs.

- One program, `xtiger-mcp`, with nothing to install. It ships with the app and the release
  archives.
- No settings needed. The server finds the validator, the game, your CK3 folder and the mods by
  itself, the same way the app does, and uses the folders picked in the app. `CK3_GAME_DIR`,
  `CK3_USER_DIR`, `XTIGER_BIN` and `XTIGER_STATE_DIR` still override the search, and a variable
  that points to the wrong place is reported instead of ignored. `xtiger-mcp --status` shows
  what was found and where.
- `xtiger_status` shows the same from the assistant. `xtiger_mods` lists the mods, and every
  tool takes a mod by name, folder, workshop id or `.mod` file.
- `xtiger_validate` reports which problems are new and which are fixed since the last run of
  the same mod.
- Long validations send progress updates and can be cancelled by the assistant.
- Every tool takes an optional `reason`, one sentence on why the assistant makes the call, and
  the server asks the assistant to always give it. Each call is written to a journal
  (`activity.jsonl` in the state folder) with its time, reason, mod, outcome and a short
  summary. The journal is capped at 512 KB plus one older file.
- `xtiger_session` names the job at hand and wraps it up at the end. It answers with a summary
  of the session so far: the checks, what was fixed, what is left and the files that changed.
  Calls are grouped in sessions even when the assistant never names one.
- Two prompts, `fix_mod` and `update_mod`, start the usual jobs in one step.
- `xtiger_pending_requests` hands the assistant what you asked for in the app, and
  `xtiger_finish_request` closes each one with a note. `xtiger_status` says how many are
  waiting.
- `xtiger_compare` compares two checks: the totals of each, and which reports are new or fixed.
- Runs share one history with the app's checks. `xtiger_runs` says who ran each one. Runs of
  one mod are matched by its `.mod` file, however the path was written, so "new" and "fixed"
  are worked out against the right earlier run.
- `xtiger_reports` takes a `mod_path` to read the newest run of one mod instead of the newest run
  of any mod. Grouped results say how many groups there are in all (`total_groups`), so a cut
  list can be told from a complete one.
- `xtiger_session` answers with the title and wrap-up it was just given, and marks the session as
  ended on a wrap-up.
- "Prepare update report" puts a "Start here" section ahead of the lists by key when files sit in
  a folder the game no longer reads: until they move, everything they define is reported as
  missing.
- `ck3_vanilla` searches the base game: where a trigger, effect, event, decision, GUI type or
  localization key is defined, with the whole definition, or the lines that match a regex.
- `ck3_docs` looks up the game's own docs of triggers, effects, event targets, `on_actions`,
  modifiers, scopes and custom localization.
- `xtiger_playsets` lists the launcher's playsets, and `ck3_run` takes a `playset` to test a
  mod together with its mods, in the launcher's order.

- `ck3_run` really loads the mods it is asked to test. It writes them to the launcher's
  `dlc_load.json`, puts the file back as it was when the game closes, and checks the log
  ("mods loaded: X of Y") before it reports a test as run.
- Console commands are typed as Unicode text, so any keyboard layout works, including
  characters such as `=`, `{`, `}` and quotes.
- The console is opened once, given two seconds to start taking text, and a few leftover
  characters are cleared before each command. A stray character used to break every command
  after the first on some keyboard layouts.
- The game counts as loaded when `game.log` says the bookmark screen is ready, instead of
  guessing from log sizes.
- Each command is checked against `debug.log`, waiting up to 15 seconds since the game writes
  that log late, and reported as run or unconfirmed.
- Input stops if CK3 loses focus, and the report says which commands were sent. Screenshots
  capture only the game window.
- Old logs are archived to `logs/xtiger-archive` instead of being deleted.
- Validation runs are saved under a run id (the newest 20). `xtiger_runs` lists them and
  `xtiger_reports` can search any of them, with paging. Results include the validator used,
  its exit code and its error output.
- `ck3_logs` reads the start or the end of a log and filters by regex. `ck3_keys` can send
  text as well as key codes.
- The game scripts are built into the program, and the server has its own test suite.

### Known limits

- Some triggers and effects that are new in 1.20 are only partly checked.
- The number of reports is not a measure of accuracy. Read the reports, not the count.
- MCP server: `xtiger_reports` with `group_by=file` counts a report under its first location
  only; a `limit` of 0 returns nothing; a damaged newest run file makes it fail until the file
  is deleted; and two server processes sharing one state folder can trip over the journal when
  it is rotated.
- The app's own crates (`xtiger-app/src-tauri` and its setup) are built by the release script
  and are not yet covered by the CI format, lint and test steps.
- The app is Windows only for now. The validator and the MCP server also run on Linux.
- The app and its installer need Microsoft Edge WebView2. Windows 11 always has it; on a
  Windows 10 without it, the installer says so and points to the download.
