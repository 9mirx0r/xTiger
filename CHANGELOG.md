# Changelog

## xTiger 1.5.7 alpha

Moving a mod to a newer game version gets more mechanical help, and the assistant can now write
the renames instead of only listing them.

### The MCP server

- `xtiger_migrate` reads the validator's own tables. Every effect or trigger that the tables mark
  as removed with one current key as its replacement is proposed as a rename, on top of the
  hand-written ones, which come first and do not change, for example `start_diarchy` to
  `try_start_diarchy`, `scheme_freeze_days` to `scheme_freeze` and `has_holy_site_flag` to
  `has_holy_site_parameter`. An entry counts only when its explanation is exactly "renamed to X",
  "replaced by X" or "replaced with X", X is a current entry of the same table and no other removed
  key names X (so `num_active_accolades` and `num_inactive_accolades`, which both point to
  `num_accolades`, are skipped). A derived rename rewrites only the key (`key =`), never a
  `scope:` or a saved scope name, and its note says to check the arguments.
- New tool `xtiger_migrate_apply` writes every edit `xtiger_migrate` lists into the mod. Each file
  that changes is first copied, as it was, to a new backup folder in the server's state folder
  (outside the mod, so a Workshop upload does not carry it), and a backup that exists is never
  overwritten. Only the lines a rule changes are rewritten: byte order mark, line endings,
  comments and quoted text stay. A file that is not UTF-8 is left alone and listed.

### The validator

- `tiger_lib::table_renames()` exposes those one-to-one renames. Nothing the validator reports
  changes.

## xTiger 1.5.6 alpha

A reliability pass on playing a mod in the real game (`ck3_run`), after an outside review of 1.5.5.
Each point was reproduced before it was fixed.

### The MCP server

- `xtiger_migrate` no longer proposes a rename for a saved scope name: `save_scope_as =
  every_character` and `scope:every_character` are the mod's own words and stay as they are. The
  two iterator renames (`every_character`, `is_created`) now apply only where the word is a key
  (`every_character = {`).
- A run that is cancelled or times out stops the script and the game together, and puts the
  player's `dlc_load.json` back. Before, only the script was stopped, so `ck3.exe` kept running and
  the mod list stayed changed. The original is also kept in `dlc_load.json.xtiger-backup` while a
  run has changed the file, and the next run restores it if the server itself was killed. A list the
  launcher or the player has changed since is never overwritten, nothing is restored while a CK3
  is running, and a run refuses to start (rather than overwrite the backup) if one is still there.
  Only one game run goes at a time, also across servers (Claude Code and the xTiger app each
  start their own): a lock file `xtiger-run.lock` in the game's user folder names the owner, and a
  second `ck3_run` meanwhile is an error. A lock whose owner is gone is taken over.
- The temporary `.mod` copies a run puts in the game's mod folder have a name of their own
  (`xtiger-run-<time>-<n>.mod`) and a marker line. A file that already exists is never
  overwritten, and only marked leftovers are ever deleted.
- A run that did not test anything is now an error (`isError`) instead of a result: the script
  failed or was stopped, the game never started, or a mod was not mounted by the game (exit code
  2). The first line of the message says which, so the app's activity list shows it; the report,
  and the path of the screenshot if there is one, follow.
- The key helper checks what `SendInput` returns. If Windows refuses the input (input blocked, a
  locked session) the command is retried and then fails with the Windows error code, instead of
  being lost without a word. A failure on the final Enter is not retried, since the command may
  already have run; it shows as `unconfirmed` if the game did not log it.
- Repeating the same console command in one run is confirmed per send: the second `help` needs
  its own line in `debug.log`, not the first one's.
- A saved validation run is written to a temporary file and renamed, so a crash or a full disk
  cannot leave half a run.
- A starting error of `ck3_run` (such as CK3 already running) shows its real message. A script-wide
  `trap` used to call a function that was not yet defined and hid it.
- `xtiger_game_gap` recognises paths with non-ASCII letters (`events/événement.txt`). Paths with
  spaces and absolute paths are still not recognised.

### The validator

- Nine effects that were accepted without looking at their arguments are now checked against the
  game's own documentation and the way vanilla uses them: `add_holy_site`, `add_eminent_holy_site`,
  `create_holy_site`, `create_clerical_region`, `split_clerical_region`, `create_dynamic_rite`,
  `detach_rite_to_new_faith`, `change_rite_divergence` and `create_domicile_title`. Missing and
  unknown fields, wrong scope types, domicile types that do not exist, and the scope each one saves
  with `save_scope_as` (a title, a rite or a faith) are now reported.
- `remove_barter_goods` is read as a script value. The docs give no syntax; the one vanilla use is
  `remove_barter_goods = scope:barter`.

### Known limits

- `multiply_focus_progress` and `set_focus_progress` are accepted without checking their arguments:
  the docs give no syntax and vanilla never uses them in script.
- `create_holy_site` is checked only for its field names and the scopes of `county`, `barony` and
  `actor`; `type` is not checked, and no vanilla script uses the effect.
- A console command the game does not log in `debug.log` shows as `unconfirmed` in a run report
  even if it ran; the run is still a result, not an error.

## xTiger 1.5.5 alpha

Found by play-testing a real mod in the game and setting the game's `error.log` against what the
validator had said. The MCP server grows to 18 tools, the validator learns six things the game
rejects or ignores, and a false report on vanilla is gone.

### The validator

- `entity` and `override` in a `dna_data` block are errors: the game's log shows a parse error
  for them.
- A history character with no faith, religion or rite is reported.
- A `custom_description` whose text key has no localization is reported.
- `faith_modifier` in a trait is reported as replaced by `rite_modifier`. The game drops the
  whole trait ("Unknown modifier type"), which shows as raw `trait_x` keys and a magenta icon.
- `mercenary_fallback` in a men-at-arms type is reported: 1.20 rejects the token (use
  `allowed_in_hired_troops` or `fallback_in_hired_troops_if_unlocked`).
- A men-at-arms type with both `icon` and `illustration` is reported: the game ignores `icon`.
- `selected_doctrines` is now known in tenets and may hold tenets, so it no longer gives a
  false report on a faith that picks them. On unmodded vanilla this removes 79 reports (71 `unknown list` in
  `tenet_types` and 8 `expects list to be doctrine` in `doctrine_types`) and adds none.
- `winter_severity_bias` accepts `0`, `0.0` and `0.00`.
- The `_past` perspectives of the localization functions are checked correctly (`global_past`,
  `first_past`), not as `global_part` and `first_part`.

### The MCP server

- New tool `xtiger_game_gap`: sets the game's `error.log` against a saved validation of the mod.
  It lists the log entries that point to a file of the mod, grouped by cause, and says whether
  Tiger reported anything in that file (`nothing` usually means a hole in the validator;
  `other lines` means it reported there, but not on the line the game named). Entries that
  name no file of the mod are left out. Some entries about a mod file are not validator holes
  and still show as `nothing` (see Known limits): read the groups, do not expect zero.
  Play-test first so the log is fresh, and validate so the run is current.
- `xtiger_migrate` has three more rules: `doctrine:tenet_...` to `tenet:tenet_...`, and the two
  `guardian_or_court_tutor` renames (`..._trigger_event` to `..._effect`, `..._trait` to
  `..._trait_trigger`). It now knows 7 renames.
- `ck3_run`: when another window takes focus for a moment, the game's window gets the focus back
  (even from a background process) and a command that was not typed completely is typed again,
  up to three times; one that was typed completely is never repeated. Each command is checked
  against the game's `debug.log` and marked `(confirmed)` or `(not logged yet)`; the game logs
  only some console commands, so the second is not a failure. The time allowed per
  command is longer.

### Known limits

- `xtiger_game_gap` cannot tell a validator hole from a log entry the validator has no way to
  know. A key that the mod defines again from the game's own localization is logged by
  the game as a duplicate, and the validator does not report it, since replacing a key is
  normal. Runtime script errors (for example a special building the game already gave to a
  province) are visible only in the game.
- Mods written for older game versions are reported for what 1.20 removed. This is correct, not
  a false report.

## xTiger 1.5 alpha

Found by checking xTiger against real mods (Regional Immersion and Cultural Enrichment, VIET
Events, Sons of Judah, Community Flavor Pack with Ethnicities and Portraits Expanded) and by
comparing its tables with the game's own documentation. The MCP server grows to 17 tools, the
validator learns what 1.20 removed and says what replaced it, and the app and the MCP server now
load a mod's dependencies the same way.

### The MCP server

- **Dependencies are loaded for you.** `xtiger_validate` reads the `dependencies` of the mod's
  descriptor, finds each one among your mods by name (a local copy comes before an added
  folder, which comes before a Workshop copy), and loads it with the mod. Mods already named in
  the mod's own `ck3-tiger.conf` are not loaded twice. A mod that needs another mod no longer reports hundreds of things as missing.
  The result lists `loaded_mods` and `unresolved_dependencies`, with the reason for each one
  that could not be found.
- `xtiger_validate` takes `with`, extra mods to load alongside, and `load_dependencies`
  (default true) to turn the automatic loading off.
- `xtiger_validate` takes a `playset`: every enabled mod of that launcher playset is loaded with
  the mod that is checked, in the playset's order. A mod with no `.mod` file is read from the
  `descriptor.mod` in its folder; with neither, it is left out and named in the answer.
- `xtiger_compare` says when the two checks were made with different mods loaded, since that
  alone changes the reports.
- `xtiger_mods` shows each mod's dependencies, and says so when a `.mod` file points to a
  folder that does not exist.
- `xtiger_reports` with `group_by=file` or `folder` counts a report under every location it
  has, `limit` 0 is documented, a damaged run file is skipped and reported instead of breaking
  the tool, and two server processes no longer trip over the journal when it is rotated.
- `ck3_docs` suggests names that contain the words you asked for in order (`is_created` finds
  `is_title_created`), also when you give a plain identifier as `search`.
- `xtiger_reports` can group by `template`: the whole message with each `quoted` name replaced,
  so "unknown field `a`" and "unknown field `b`" are one group. `message` still cuts at 100
  characters.
- New tool `xtiger_overrides`: what a mod overrides in the base game, and how each copy differs
  from the game installed now. A mod file with the path of a game file replaces the whole file
  (the names the copy lacks are gone from the game); a definition with the name of a game
  definition in another file replaces that one. Each shows the lines only the game has (`- `)
  and only the mod has (`+ `), and near copies, the likeliest to be stale after a patch, come
  first.
- New tool `xtiger_migrate`: a dry run of the mechanical part of moving a mod to 1.20. It lists,
  file by file and line by line, the renames that are the same everywhere (`every_character` to
  `every_living_character`, `is_created` to `is_title_created`, `create_holy_order_effect` to
  `create_holy_order_accompanying_effect`, a tenet tested with `has_doctrine` to `has_tenet`),
  with the line as it would read. A line with several renames is one edit. Comments and quoted text are left alone and nothing is written.
- `ck3_run` answers with an error, not a result, when the game was not started (for example CK3
  was already running). The `play` parameter says it must be a title held at the bookmark date.

### The validator

- A `.mod` file whose `path` does not exist is reported as an error. Before, the validator
  looked at the wrong folder and reported nothing.
- A secondary mod (`load_mod` in `ck3-tiger.conf`) whose folder does not exist stops the check
  with a message that names the folder.
- Tables brought up to date with CK3 1.20.0.4 and checked against the game's documentation:
  20 `on_action`s that were missing (holy sites, rites, personal tenets, traits, holy orders,
  council, character creation and more), with the scopes the game sets for each, and the
  return types of `GetDoctrine` and `GetElectorFromCharacter`.
- Triggers, effects, iterators, data functions and modifiers were compared with the game's
  documentation and found complete.
- `munch-script-docs` no longer adds a second `// TODO: REMOVED` to a line that already has it.
- `history_override_priority` is understood in history characters: a character that carries it
  can redefine one defined elsewhere, without a duplicate-id report, an unknown-field report or
  a missing `name`. A duplicate without it is still reported, with a hint about the key.
- Keys that older versions accepted and 1.20 removed (`set_title_flag`, `trait_xp`,
  `every_character`, `is_created`, `create_holy_order_effect`, the `scholar` trait,
  `add_trait_track_xp`) are still reported as unknown, now with what to use instead. So are
  `GetFaithDoctrine` (use `GetDoctrine('<key>')`), `GetOwner` after an activity (use `GetHost`),
  and a tenet given to `has_doctrine` (use `has_tenet`).
- A faith icon is accepted when `gfx/interface/icons/faith/<name>.dds` exists, which is where the
  game takes faith icons from. On a mod written for an older version, 19 `has_icon` names that
  point at such a file, with no data file defining them, are no longer reported.
- New hints: `date = current_date` (a date takes year.month.day), an undefined `@constant`
  (constants only exist in the file that defines them), a law written in the old wrapper format
  (no `law_group_type`), and an on_action with an `effect` block in more than one file.
- A faith key that no longer exists but is now a rite (religions were reorganised in 1.20)
  is reported with that hint.
- A character template no longer checks its own `rite` when the `create_character` that calls
  it already gives one, as it already did for `culture` and `faith`.

### The xTiger app

- The xTiger app loads the mods a mod depends on, the same way the MCP server does, and saves
  which ones it loaded with the run. A check in the app and a check by an assistant now give the
  same reports, and comparing them no longer says the setup changed.

### Known limits

- Mods written for older game versions are reported for what 1.20 removed. This is correct, not
  a false report. The renames xTiger knows about come with a hint; any other removed name is
  reported without one.

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
