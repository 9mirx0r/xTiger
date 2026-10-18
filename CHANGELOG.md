# Changelog

## xTiger 1.6.10 alpha

- Correct the CK3 1.20 `on_faith_created` contract: the new faith is the root, with `founder`,
  `old_faith`, `old_rite` and `diverged` scopes. Conversion on-actions keep a character root.
- Report missing `trait_<group>` localization for trait groups and equivalence groups, at the
  group value in each trait. Group descriptions are not required.
- Add `observe_years` and `simulation_timeout` to `ck3_run` and `ck3_run_start`: run in observer
  mode at speed 5, wait for the game's completion log, and retain the raw logs beside the report.
  Observer tests preserve the player's rotating autosaves, including on cancellation.
  Restoration handles filename casing changes on Windows.
  Jobs expose the simulation phase; timeouts and early exits fail. Console input waits for the
  console to become visible before typing.

## xTiger 1.6.9 alpha

### Fixed

- **The app no longer hangs on "Press Enter to close this window".** Since 1.6.8 the validator
  waited for Enter when it took itself for a double-clicked program. The desktop app starts it
  without a window, which looks the same from the inside, so a check could stay on "Validating"
  until it was cancelled, and the real result or error stayed hidden. It now waits only when its
  console window is really on screen, and the app gives it no input to wait for.

## xTiger 1.6.8 alpha

### New

- **A full-screen display for `ck3-tiger-auto.exe`.** It now opens a real screen instead of
  scrolling text: the list of your mods on the left (type to search, or paste a folder or a
  `.mod` file) and what it found about your setup on the right; a screen that shows each step of
  the check as it runs; and a result you can move through. The result has three views (Causes,
  Files, Reports), a panel on the right that explains the selected cause and how to fix it, or
  shows the report with the line of code it is about, a filter by severity, and keys to open the
  full report or the file in your editor, check another mod, change the game folder or the folder
  of your files, or update. F1 lists the keys (so does `?` on the result). A mod with no problems gets a short "nothing to fix" screen. A window
  smaller than 64x18 asks to be made bigger.
  Where a full screen is not possible (output sent to a file, `TERM=dumb`), or when you ask for
  the plain menu with `--plain` or `XTIGER_ASCII=1`, the arrow-key menu is used as before.
  `ck3-tiger.exe` and its commands keep printing text.

### Fixes

- **The command-line programs no longer vanish.** Double-clicking `ck3-tiger.exe` or
  `ck3-tiger-auto.exe` opened a window that closed the moment the program ended, so an error, or
  the result of a check, could not be read. The window now waits for Enter when it was opened by a
  double-click. Started from a terminal, nothing changes.
- **A first-run screen for the command-line programs.** `ck3-tiger-auto.exe` (and `ck3-tiger.exe`
  when it is opened without a mod) now starts by looking for your files and shows what it found: the
  game, your Paradox user folder, the Workshop. When the game is not in a Steam library, for
  example a copy you downloaded or installed elsewhere, it says so and asks you to paste the
  folder, no command needed. It checks that the folder is really the game (a path to the `game`
  folder, to a file inside it, or to the folder around it also works), remembers it for next time,
  and goes on to the list of mods. A user folder moved to OneDrive is found too.
- **`ck3-tiger-auto.exe` stays open with a menu.** After the result it no longer says "Press Enter
  to close". It shows the result card and a menu you move through with the arrow keys (or a
  number): look at the causes and what each one means, read the errors page by page, open the full
  report, check another mod, change the game folder, look for a newer xTiger, or quit. Mods are
  also picked with the arrow keys.
- **A cleaner screen.** Each phase starts on a fresh screen, so the logo is no longer pushed out of
  view, and there is a single prompt where there were two. A note from the library ("Loading
  secondary mod ...") gets its own line instead of being written over the progress spinner.
- **`ck3_run` no longer looks stuck.** A play-test takes minutes, and some assistants (VS Code, for
  one) show a call that long as hanging. `ck3_run` now runs the game as a job and waits up to 150
  seconds; if the run is still going, it answers with the job id and `ck3_job` follows it. A run that
  ends sooner is answered exactly as before.

## xTiger 1.6.7 alpha

A hardening release: an outside audit of 1.6.6 found six problems, all of them real, and all are fixed
here. There is also a round of smaller fixes, and the pictures in the README were redone.

### Fixes from the audit

- **Migrator.** `xtiger_migrate_apply` changed text that came after an escaped quote inside a string.
  It now reads backslash escapes the way the game does.
- **Very deep files.** A file with thousands of nested blocks could overflow the stack and crash the
  validator. Nesting past 200 levels is now reported as an ordinary error and the rest of the mod is
  still checked.
- **Updaters.** Both updaters (`ck3-tiger update` and the app) used to carry on when GitHub published
  no checksum for a download. A checksum is now required, and a download without one is not offered.
- **Installer.** It removed every `*.old` file in the install folder. It now removes only the copies
  it moved aside itself, `<program>.<n>.old` for xTiger's own programs.
- **Releases.** A release could be published even if the tests failed. The release workflow now waits
  for the full test run, which also covers the app and the setup.
- **Dependencies.** `self_update` moved to 1.x and `zip` to 9. `cargo audit` reports no vulnerabilities
  in any of the three lockfiles.

### Smaller fixes

- Report columns count wide characters (Japanese, Chinese, Korean) as two columns, so a long path no
  longer breaks the layout.
- A file of the game and a file of the mod with the same path are labelled apart (`[CK3]` in front of
  the game's).
- `--path` ignores upper and lower case on Windows.
- Two xTiger programs starting at the same moment no longer race for the game lock.
- A byte-order mark at the start of a file is tolerated in more places.
- A file the migrator cannot read is reported and skipped instead of stopping the whole run.

### Documents

- The command-line pictures in the README are wider and shorter, with larger text, and show the same
  content as before.
- The AI activity and Settings pictures are captures of the real app instead of the old mock-ups.

## xTiger 1.6.6 alpha

`ck3-tiger update` works again, and it now looks like the rest of the validator: a line per step with
its time, a progress bar while it downloads, and the release notes at the end. It can also tell you,
if you want, when a new version is out.

### The updater

- The old updater asked GitHub for the "latest" release, and GitHub answers 404 while every release is
  marked as a pre-release, so `ck3-tiger update` said it was up to date or failed. The new one reads
  the list of releases itself, picks the newest that is not a draft, and compares versions properly.
  **If you are on 1.6.5 or older, download this release once by hand; from then on `ck3-tiger update`
  does the rest.**
- Steps are shown as they happen: looking for the newest version, downloading (with a bar, the size and
  the speed), checking the download, unpacking and replacing the program. The SHA-256 that GitHub
  publishes for each file is checked before anything is unpacked, and the archive is refused if it is
  not from the xTiger releases or is bigger than 200 MB.
- When it is done it shows what is new, taken from the release notes, and the link to the full notes.
- `ck3-tiger update --check` only says whether a newer version exists. `ck3-tiger update 1.6.5-alpha`
  installs a particular one, including an older one.
- Everything is plain text when the output is not a terminal, as in the rest of the validator.

### The "new version available" line

- Off until you say so. The first time you run the validator at a terminal it asks once. If you say
  yes, it asks GitHub at most once a day and prints one line after a validation when a newer version is
  out. If you say no, it never goes online by itself.
- `ck3-tiger update --notices on|off` changes it later. It never asks and never prints in CI, in
  scripts or with redirected output, and `XTIGER_NO_UPDATE_CHECK=1` turns it off for good.
- The choice is kept in the validator's own settings folder; `XTIGER_CONFIG_DIR` points it somewhere
  else.

### Documents

- The release page now has two downloads, the desktop app and the command-line validator. The separate
  `xtiger-mcp` archives are gone: the app already includes the MCP server, and it can still be built
  from source (see `xtiger-mcp/README.md`).
- The README shows the real command line and the real desktop app, with real mods, and explains the
  updater. The images of the app in `docs/app.md` were taken again from the current version.

## xTiger 1.6.5 alpha

The validator is now the front door: it is easier to read at a terminal, easier to use in a script or a
pre-commit check, and the repository and release files are organised around it. An audit of the whole
repository on top of that fixed a batch of smaller bugs in the validator, the MCP server and the app.

### The validator at a terminal

- The program calls itself xTiger-CK3 in its banner, `--help`, `--version` and usage errors. A banner
  with Qubis is shown at a terminal and one plain line otherwise. The executable keeps the name
  `ck3-tiger`, so `ck3-tiger update` and existing scripts keep working.
- The run shows its stages with their times: reading the game and the mod, understanding the scripts,
  checking everything. A card at the end says in plain language what was found, what to look at first,
  which files hold most of the reports and the commands to try next. Those commands keep the `--game`,
  `--paradox` and `--config` you gave.
- Locations are relative to the mod, and the short format is aligned and colored. Tips and untidy
  reports are folded into a count; `--severity tips` shows everything.
- Run it with no mod at a terminal and it lists the mods in your mod folder to choose from.
- All of this is written to stderr. With stderr redirected, and in CI, none of it appears and the
  reports are the same as before. `XTIGER_FORCE_TTY=1` forces the terminal view in a captured output.
  The colors follow `--no-color` and `NO_COLOR`.

### The validator in scripts and by hand

- `--format short|full|json|summary` and `--summary`: one line per report (`file:line: severity key:
  message`), and counts by severity, key and file before the reports. When the terminal folds reports,
  the summary says how many are not shown.
- `--severity`, `--only-key`, `--skip-key` and `--path` narrow the output without editing a config
  file. They apply on top of the `filter` block of `ck3-tiger.conf`, and they also filter the early
  reports, such as the packaging warnings. `--path` takes `events`, `./events`, a glob, or a bare file
  name such as `db_duel_events.txt`.
- `--fail-on <severity>` makes the exit status 1 when a report at that severity or worse is printed.
  Without it the status stays 0. Status 2 means the run could not be done: bad arguments, a mod that
  does not load, or a crash inside the validator.
- `ck3-tiger explain <key>` says what a report key means, how to fix it and how to silence it. Without
  a key it lists them all.
- A note about the `.mod` file, such as a `replace_path` that is not needed, no longer stops the run.
  Only reports about `ck3-tiger.conf` do.
- The validator has the xTiger icon and file properties on Windows.
- The `ck3-tiger.conf` template no longer contradicts itself: `check` and `skip` language lines are
  explained correctly, and the output colors use the right severity names.

### Release files and documents

- The README is written around the command-line validator. The desktop app and the AI assistants have
  their own pages: `docs/app.md` and `docs/ai-assistants.md`.
- The release has one archive for the validator (`ck3-tiger-windows-*.zip`, `ck3-tiger-linux-*.tar.gz`),
  which keeps its layout because `ck3-tiger update` depends on it, one for the MCP server
  (`xtiger-mcp-*`) and the app installer. The release notes start with a "Which file?" note, and the
  build now refuses to publish a release whose CHANGELOG section is still called "Unreleased" or whose
  app version does not match the tag.
- `update-versions.sh` also bumps the versions of the app, its installer and the MCP server.
- `CONTRIBUTING.md` and issue templates for bugs and false positives were added.

### The assistants (MCP server)

- The migrator (`xtiger_migrate`, `xtiger_migrate_apply`) renames `create_holy_order_effect`,
  `guardian_or_court_tutor_*` and `has_doctrine` only where they are keys, and never rewrites inside
  a string that spans several lines. The dry run and the real run share the same code.
- The fingerprint of a run includes `ck3-tiger.conf` and the images, and the project status says what
  the fingerprint does not cover.
- `recover_dlc_load` keeps its backup when writing the restored file fails.
- `ck3_events` counts separately the events that only pass with guessed scopes. Test scripts hold a
  lock (`xtiger_script.lock`) that the run lock shares.
- A validator that cannot start is reported as an error instead of an empty result, the validator's
  messages are no longer lost, typed keys cannot carry control characters or `|` into the helper's
  line protocol, a stale reply cannot be taken for the answer to the next request, and `load_timeout`
  is kept between 30 and 3600 seconds.

### The app

- A check cancelled while it starts no longer slips through, and a second check is refused while one is
  running, so a cancelled run cannot overwrite the next one.
- The estimate of the duration and the mod pictures no longer break the screen when they fail to load.

### Fixes from the audit

- Ctrl-C in the mod picker restores the cursor, and a key the terminal cannot read ends the picker
  instead of looping.
- A step that fails no longer leaves its spinner running over the error message.
- With only stdout redirected, the file keeps the closing count line.
- A run narrowed to nothing says "Nothing to show with these filters" instead of "No problems found.",
  and the card says "No errors or warnings" when only tips are hidden.
- A report whose occurrences are all silenced by `#tiger-ignore` is no longer counted.
- `explain` gives fewer false suggestions for very short inputs.
- Several wrong statements in the README and the filter guide were corrected, among them the claim
  that a narrowed run is faster (it is not: the filters only choose what is printed).

## xTiger 1.6.4 alpha

Elections written in the 1.20 shape are accepted, and the first run of Tiger over the game's own files
removed about 800 false reports. Thanks to shalola-author for issue #8.

### Succession elections (issue #8)

- `common/succession_election` follows the 1.20 shape. Candidates and electors are now built with
  `character_sets = { some_key = { type = ... limit = { ... } } }`, so a 1.20 election no longer reports
  `character_sets`, `priority` or the new `same_faith_clerical_electors` type as unknown. Added the
  `elector_vote_strength_value` and `candidate_score_value` fields.
- The old `add = holder` and `add = { type = ... limit = ... }` forms are reported as removed in 1.20,
  with the new form in the info text, instead of being accepted silently. Thanks to shalola-author.

### Vanilla self-check fixes (1.20)

The first run of the vanilla self-check found about 2,700 reports in the game's own files. These
false positives are fixed, 2,735 reports down to 2,180 so far, with no new group in any run:

- Enums and types the game accepts now: `disable_regnal_numbers = never|auto|always`,
  `royal_court = landed`, `attacker_wargoal_percentage` above 1, doctrine `divergence` with decimals,
  law `faith = same_rite`, pool `gender = male|female`, domicile `map_pin_anchor = left`,
  trait `inheritance_blocker = secular`, situation `window = christian_church`, great project
  `province_filter = clerical_region_theocracy_capitals` and `owner =
  province_clerical_region_or_theocracy_holder`, candidate types `clerical_region_clergy` and
  `clerical_region_clergy_councilor`, province filter `anointment_candidates`.
- `is_in_list = x` before the first `add_to_list = x` no longer reports an unknown list: the usual way to
  avoid duplicates, which the game treats as an empty list.
- Situation catalysts accept script values with calculations (dynamic values are fine since 1.20).
- New scripted rules for the Rites DLC (`can_edit_rite`, `can_create_title`, `can_destroy_title`,
  `is_grant_government_valid` and others), and lists of a scripted rule are now defined for its scripts.
- `random_traits_list` accepts `trait = { weight = 30 }` with a plain number, `culture != x`,
  `faith != x` and `government_type != x` are accepted in `any_ruler`-style lists next to the filter, and
  `hex { 50779b }` colors parse (fog colors of the map environment).
- No more false reports for `look_at`, `radius` and `falloff` on any light kind (tips now), the `effect`
  of tutorial lessons in any chain, an interaction category with `favorite_interactions` and no `desc`,
  `change_struggle_phase` without `with_transition`, `create_legend_seed` with a chronicle that has no
  properties, and `special = holder` on a flavorization title.
- Named scopes the game supplies are now known (strict-scopes reports 509 down to 332): the effects of a
  scheme type (`on_start`, `on_monthly`, `on_invalidated`, ...) see `scope:scheme`, `scope:owner`,
  `scope:target` and, for title schemes, `scope:target_title`, which also fixes the events they fire; the
  modifiers of a council task see `scope:councillor` and `scope:councillor_liege`; the DNA modifier values
  of a portrait modifier see `scope:weight_for_portrait`, `scope:prowess` and the rest; artifact visuals
  see `scope:owner`, and scripted animations and artifact visuals no longer insist on a fixed set of
  names because the caller supplies the others.
- More formats the game accepts (2,180 reports down to 1,940): `common/defines` lists written with commas
  (`{ "friend", "rival" }`), a plain number for the `x` and `y` of an accessory layout `offset`,
  `adjacencies.csv` lines with a tenth field, `rite_has_tenet`, `rite_can_choose_tenet` and
  `change_government` with a `scope:` reference, an empty `if` or `else_if` in a script value that an
  `else_if` or `else` follows (deliberate, it stops the later branches), and a chained call such as
  `scope.GetScriptValueBreakdown( 'x' )` in a gui file is no longer mistaken for the data binding macro of
  the same name. `rite_has_doctrine`, `rite_can_choose_doctrine` and `knows_doctrine` accept `scope:` and
  `doctrine:` references, `scope:province` is known in the local title holder pool selector, and
  `pause_game` is accepted in any tutorial lesson chain (the reactive advice chain uses it).
  `culture = { ... }` and `faith = { ... }` inside `every_ruler` and `random_ruler` are scope links again
  (they were taken for the filters of the same name), `domicile ?= { ... }` works in character history,
  an asset `attach = { name = locator root = entity }` is accepted, and a data binding macro that is only a
  typed literal (`'(CVector2f)0.0,0.04'`) is no longer a parse error.

### Audit tools

- `tools/audit_info.py` compares the structure files the game ships (`common/**/_*.info`) with what the
  validators accept, and lists names the validator does not know (possible false positives) and names it
  accepts that the game no longer documents or uses (possible false negatives).
- `tools/vanilla_selfcheck.py` runs Tiger over the game's own files and groups the reports. The game loads
  its own files, so each report is a Tiger gap or a flaw of the game. See AUDIT.md.

### Playsets

- The Delete button of the Playsets screen works: it asks "Delete this playset?" next to the buttons,
  with Delete and Keep (Escape also keeps it). The old second click on the same button was undone as
  soon as the screen repainted.
- A playset now shows what its mods still need. Mods that need something carry a "Needs ..." tag, and a
  short box lists the dependencies that are not in the playset (with Add and Add all), not installed,
  or loaded after the mod that needs them. Fix order puts every mod after the mods it needs and
  changes nothing else. When all is covered it says so in one line. The same analysis is in the
  `needs` and `suggested` fields the app reads.

## xTiger 1.6.3 alpha

A patch, or a mod built on other mods, can now be tested together with the mods it needs. Thanks to
shalola-author for the idea (issue #6).

### Load order

- xTiger used to load only the mod under test, so a patch checked on its own reported everything it
  patches as missing, and a play-test of it failed or proved nothing. Each mod now has a saved load
  order: the mods that load before it, in game order. The mod under test always loads last.
- New Load order screen in the app, reached from the Mods screen. Tick the mods a mod needs, drag them
  (or use the arrow keys on a handle) into the right order, and see below what the next test will load:
  the saved mods, the dependencies their descriptors name, and anything that cannot be found. The list
  can also start from a launcher playset.
- Dependencies are reported. The Load order screen has a Dependencies block that lists the mods the
  descriptors of the mod and of the listed mods name as needed: already in the list, installed and
  loaded on their own (with an Add to list button), or not installed, so you know what to get. The
  same list is the `dependencies` field of `xtiger_loadorder`.
- The same list is used by the assistants. New tool `xtiger_loadorder` (get, set, add, remove, move,
  clear, from_playset); `xtiger_validate`, `ck3_run` and `ck3_run_start` load it by default when one
  mod is tested, and say what they loaded. `load_order: false` leaves the saved order out (a check
  also needs `load_dependencies: false` to test the mod alone, as before). The Validate button of
  the app uses the saved order too.
- The order can be written into the mod's own `ck3-tiger.conf` as `load_mod` lines, for the plain
  validator and CI. The lines hold paths from your computer, so keep that file out of anything you
  publish; the app says so, keeps the old file next to it as `ck3-tiger.conf.xtiger-backup`, and takes
  the lines out again when the list is emptied.
- A dependency that the mod's own `ck3-tiger.conf` already loads is shown as loaded by the config, not
  as one that loads on its own (`configured` in `xtiger_loadorder`).

### Playsets of xTiger

- xTiger now keeps playsets of its own, next to the launcher's, which it still only reads. They are
  named lists of mods, in order, with a note on what they are for. The assistant puts there the mods it
  wants to try together, and you can open and change the same lists.
- New Playsets screen in the app: a bar with your playsets, an editor (name, note, duplicate, delete),
  the mods of the playset with a handle to drag them (or the arrow keys), a tick to leave one out
  without removing it, and the installed mods to pick from. A playset can start empty or as a copy of
  one of the launcher's.
- `xtiger_playsets` can now create and change them: set, add, remove, move, enable, disable, clear,
  copy, rename, delete. A playset of the launcher cannot be changed: copy it first. Pass the name as
  `playset` to `ck3_run`, `xtiger_validate` or `xtiger_loadorder` (from_playset); a name is looked up
  in xTiger's playsets first.

### The validator

- `add_trait_xp` without `track` is reported when the trait has several tracks (for example
  `tourney_participant`, `lifestyle_hunter`, `lifestyle_traveler` or `gallowsbait`): the game cannot tell
  which track gets the experience, so say it with `track = foot`. A trait with one track, or a macro
  or saved scope in place of the trait, is not reported. Thanks to shalola-author for the report
  (issue #7).

## xTiger 1.6.2 alpha

A small fix release.

### The validator

- `has_ethnicity` takes the key of an ethnicity from `common/ethnicities`, as it does in the game, and
  no longer reports every valid ethnicity as an "unknown token". An ethnicity that does not exist is now
  reported as a missing item instead. Thanks to shalola-author for the report (issue #5).

## xTiger 1.6.1 alpha "Qizilbash"

We are still working on speed, on what the validator can catch, and on making xTiger fit the way people
actually mod. This release is a step on that road.

- Faster: the tools that drive the game no longer wait fixed seconds, and a whole test set-up is one
  call instead of many. We keep measuring and trimming, in the validator as well as in the game control.
- A validator that finds more: we compare what the game logs when it loads a mod with what Tiger
  reports, and every error Tiger missed becomes a new check. Each check was run over all of vanilla
  first, so none of them reports anything in the base game.
- Modding without keeping xTiger on screen: we are building it so that xTiger can run in the background,
  watch a mod and hand you the result when you come back.
- Next, planned and not part of this release: our own commands for reviewing and validating mods, and a
  substantial improvement to updating deprecated mods to a newer game version.


### The assistants

- `ck3_keys` is much faster. A resident input helper now stays alive between calls, so a key press
  takes about half a second instead of three, and the fixed one-second waits are gone: the game gets
  `wait_ms` (300 by default) to react instead. If the helper cannot start, the old one-shot script is
  used as before.
- `ck3_keys` takes `steps`, a list of `{scancodes, text, wait_ms}` run in order in one call, so a
  console command (open the console, type, press Enter) no longer needs three round trips. The
  screenshot is taken after the last step. A failing step stops the call and says which one it was.
- `ck3_keys` can wait for the game instead of for a fixed time. Give a step `wait_for`, a regular
  expression, and it ends as soon as a matching line reaches the game's log (`debug` by default, `log`
  picks another), and says which line. `wait_ms` is then the longest it waits (5 s by default); if the
  line never comes, the call fails and still returns the screenshot. For a console command,
  `Running console command: effect` confirms that the game ran it. The game buffers its logs and
  writes them when it is busy, so a paused game that does nothing may hold the line back for a
  minute: use it for steps that make the game work, or check the screenshot.
- New tool `ck3_script` runs a list of effects on the player in an open game. It writes them to a file in the
  game's `run` folder, types the console's `run` command for it, and waits until the game has run to the end of
  the file (a `debug_log` marker it adds as the last line), so a test set-up (traits, gold, a title, then an event)
  is one call instead of one console line each. It returns the script's own `debug_log` lines, and when the
  game rejects the file, the `error.log` lines about it. The console's key is a toggle, so it looks at the
  game's picture to see whether the console is open and presses it only when closed (also with the window covered
  or minimized). The file is written with a BOM so the game does not warn on every run. A script is never sent
  twice, because the game writes `debug.log` late and a missing line does not prove that it did not run.
- New tool `ck3_cases` runs up to 25 effect scripts one after the other in the open game and reports, for each,
  what it printed and which new errors it caused. It leaves out the game's noise: what it logs while it
  describes effects (the tooltip pass) and the warnings it repeats on every run (an event target "used but
  never set"), so what is left is what that case did wrong.
- `ck3_script`, `ck3_cases` and the new `ck3_events` take `with`, scope fixtures such as `character:friend`,
  `child:kid=7`, `courtier:c`, `title:t`, `faith:f` or `value:n=3`, set up before the effects so an event that
  needs `scope:friend` gets one. The report notes how many lines of the file are fixtures.
- New tool `ck3_events` fires events one after the other in the open game, by id or every event a mod
  defines (`mod_path`, with `filter`, `skip` and `max` to page). When the game says an event target is
  undefined, it makes a scope of that name (a character, child, title... as the name suggests) and fires
  the event again. Each event comes back ok, silent (nothing logged, possibly a false trigger), ERRORS or
  NEEDS SCOPES.
- New tool `ck3_click` clicks the mouse in the open game at a fraction of the window's width and height,
  taken from a screenshot, for what keys cannot reach (number keys do not pick event options; a click
  does). It can wait for a log line like `ck3_keys`.
- The tools that take a screenshot accept `shot_region` and `shot_scale`, to keep part of the window and
  shrink it, so the picture costs the reader fewer tokens.
- Documented: a game started by `ck3_run` picks up edits to events, scripted effects and localization (also new
  files) within seconds, with no command, and logs syntax errors in the edited files at once.
- `ck3_logs` takes `since`: `"end"` returns no lines and reports an offset, and passing that offset
  back returns only what the game wrote in between (and a new offset). It reads only the new part of
  the file, so it is cheap on a large log, and starts again from the top if the game restarted.
- The `ck3_keys` screenshot no longer needs the game in front: it asks the window to paint itself, so it
  is right when another window covers the game and does not take the focus. A minimized game, or a
  picture that comes back black, still falls back to the old screen copy. A call with no keys at all
  takes just a screenshot of the game as it is.
- `ck3_run` checks the game's log for each console command it sent every quarter of a second instead
  of every second, and gives focus back to the game in less time. The wait for the loading screen
  stays fixed: no log line says when it is gone, and keys typed as soon as the map started logging
  were lost.

### The validator

Eight checks for errors the game logs at start-up that Tiger used to miss. Each was checked against
every vanilla definition first, so none of them reports anything in the base game.

- A culture without `head_determination` is an error: the game logs it, and all 193 vanilla cultures have it.
- A scheme without the localization `<scheme>_desc_general` is reported: all 37 vanilla schemes have it.
- `ai_check_interval` next to `ai_goal = yes` in a decision is a warning (it was untidy): the game logs an
  error and ignores the interval.
- A regiment (`men_at_arms_types`) `icon` whose .dds is not uncompressed B8G8R8A8 is an error: the game
  logs "Only B8G8R8A8_UNORM support so far" and shows no icon (all 81 vanilla icons are that format).
  This looked at DXT files only at first; it now checks the exact pixel format and masks.
- A portrait `dna_modifiers` `accessory` block with a `template` but none of `value`, `range` or
  `accessory` is an error: the game logs "Need a value set" and ignores it. Vanilla uses `range` in most of
  its 2200 blocks, which stays valid.
- A portrait `dna_modifiers` `accessory` block that names an `accessory` which is not an entry of the gene
  `template` is an error: the game logs "Can't find accessory 'X' in gene template Y". With a `type` only
  that body is searched (following `female = male` style references); `empty` does not count as an
  accessory. Checked against all 892 such blocks in vanilla: none is reported.
- A history character without any `birth` event is an error: the game logs "No birth date for character".
  For a `history_override_priority` override the info says that the date of birth is one of the base
  attributes the override replaces, so the birth has to be repeated. All 73450 vanilla characters have one.
- A `history_override_priority` character now needs `name` like any other: the override replaces all base
  attributes, and one without a name makes the game log `Missing loc for name ''` when it is instantiated.
  All vanilla overrides have one.
- The dds lookup used backslash paths on Windows, so any check that finds a texture by its script path
  (including the frame check for game concepts) silently found nothing there. Paths are now normalized.

## xTiger 1.6.0 alpha

A big step for the app and the assistants: a new Home screen, a results summary, following a mod's
update, notes on reports, game tests that no longer take over your screen, and a durable project
record for the assistants. It also fixes the missing trait-icon report from
[issue #4](https://github.com/9mirx0r/xTiger/issues/4) and adds the first checks for event
overrides and for booleans used as numbers.

### The validator

- Trait `tracks` now check that the level-track icon exists. A missing
  `gfx/interface/icons/trait_level_tracks/<track>.dds` is reported as a warning, as requested in
  [issue #4](https://github.com/9mirx0r/xTiger/issues/4). The vanilla game itself lacks 12 of these
  icons, so they appear only with `--show-vanilla`.
- Events that override another event with `id_override_priority` are now understood. The
  definition with the highest priority is the one validated, whichever file comes first, and
  `id_override_priority` is no longer an unknown field. Two definitions of one event with the
  same priority are reported as an error, as the game does, instead of the old "event is
  redefined" warning that also fired for deliberate overrides. Checked against the game itself.
- An event definition that loses against another definition of the same id is now checked for
  misspelled field names and a non-numeric `id_override_priority`, because the game still parses
  it and logs an error (for example for `id_overide_priority`). Its content is still not
  validated, since it never runs. Definitions from the base game are skipped.
- A literal `yes` or `no` where a number is required is now an error: effects such as `add_gold`
  and `add_prestige` (also as `{ value = yes }`) and numeric trigger comparisons such as
  `gold >= yes`. The game logs "Badly read script value" and ignores them. Booleans in boolean
  places are untouched. Checked against the game itself.
- A scope value saved as a boolean (`save_scope_value_as` or `save_temporary_scope_value_as` with
  `value = yes` or `no`, or a copy of such a value) and then used as a number is now an error, in
  the same places: `add_gold = scope:x`, `{ value = scope:x }`, `gold >= scope:x`, `scope:x >= 1`
  and `scope:x != 0`. Using it as a boolean (`scope:x = yes`) is fine. The check is deliberately
  cautious: it applies within one event or effect block, and stays silent when the same name is
  also saved as something else, or comes from a scripted effect, a script value or a triggering
  event.

### The app

- A new Home screen with the CK3 logo and a "Coming soon" strip for Stellaris, and a slow,
  smooth ASCII water background.
- The AI assistants screen shows each assistant with its logo and a small animation, tells chat
  apps, coding agents and editors apart, and can now connect and disconnect Codex
  (`config.toml`) as well.
- The results screen opens with a summary: a stacked severity bar that filters on click, counts
  of fixed and new reports since the last run, and the three files with the most reports. The
  validation progress bar fills up and turns green before the results appear.
- Dark theme fixes and a redesigned Settings screen.
- "Follow this update" on the results screen. Once a mod is followed, a strip under the summary
  shows how its update stands: validated, static check, game test and review, with what still
  blocks it. Details lists the blockers and how many reports are reviewed or still open. It reads
  the same project record the assistants use through `xtiger_project`.
- A note on any report: say why it is fine (on purpose, checked in the game, only formatting) or
  that it is still a defect, for one report or for all of its kind in the file. The note is kept
  with the update, shows as a NOTE mark in the list, and never hides the report from a list or a
  count.
- A note written for code that has since changed now says so on its own report: the list shows
  "Needs a new look" instead of NOTE, and the report offers "Still true, keep the note" to save
  the note again for the code as it is. The project record lists which reports those are
  (`needs_look`).
- A new Settings choice, "Game tests", for when an assistant tries a mod in the game. "In the
  background" (the default) starts the game minimized at low priority and never takes the focus,
  so you can keep playing something else; the mod starts its own events and the answer comes from
  the game's logs. "Take over the screen" brings the game to the front and types console commands
  into it, as before.
- `xtiger_overrides` says, for each event that overrides a game event, which priority wins.
- `xtiger_overrides` takes `base`, a copy of the game version the mod was written for. Each
  difference then says who made it, the mod or the game since, and a `rebase` verdict (keep_mod,
  take_game, combine, review) suggests what to do. Nothing is applied, and without a `base` the
  report says it cannot tell the two apart.
- New tool `xtiger_project`: a durable record for updating one mod. It keeps an explicit
  baseline (a run, and the mod's definitions when the project began) when the work moves to a new
  folder, can work on a copy so the original is never touched, pins runs so they survive beyond the
  newest 20, and exports its evidence. Every saved run now records a hash of the mod's files, so a
  result made before an edit is shown as stale.
- The project ledger lists each definition as preserved, changed, removed or added, with the lines
  and calls lost. Removing a block can lower the error count without fixing anything: a removed
  definition, or one that stopped naming another, needs an `intended` or `restore` decision with a
  note, bound to that exact difference. It compares text, not behaviour.
- The ledger now also compares localization (`localization/<language>/` keys and their text,
  `replace/` folders included), `gui/*.gui` (templates, `types` entries and named top-level
  widgets) and `gfx/*.gfx` (named entries such as sprites). Each entry has a `kind` (`script`,
  `localization`, `gui`, `gfx`) and the summary counts by kind. A removed localization key that a
  definition of common/, events/ or history/ still names lists it in `still_named_by` and needs a
  decision; other removed keys are listed but do not hold the review. Reordered keys, spacing and
  comments are not differences. Projects begun before this keep their old baseline, which only
  covers script, so the new kinds are skipped there. Images, models, `.asset` files and unnamed gui
  widgets are still not compared.
- Every validation now carries a `status` object with the stages process, static, runtime and
  review. Validation never starts the game or reviews anything, so those two say `not_run`.
  `xtiger_project` status adds an `update` object (`update_complete` and the list of what blocks
  it): a clean static check is never reported as a tested update, a game run older than the last
  edit does not count, and open decisions or unreviewed diagnostics keep it open.
- `xtiger_project` action `annotate` writes a note on diagnostics of the latest run (intentional
  override, test assertion, unresolved defect or advisory formatting). The diagnostics are not
  hidden or removed from any count; status groups them as reviewed by kind, unreviewed, or needing
  a new look because the definition they are in changed after the note was made.
- `xtiger_game_gap` now gives every `error.log` entry exactly one state, so `states` adds up to
  `log_entries`: `matched` (Tiger reported at that place about the same thing), `same_location_other_cause`,
  `same_file_other_lines`, `no_tiger_report` (usually the validator is blind to that file),
  `unattributed` (names no file of the mod, with the reason) and `unsupported_format` (a path it
  cannot place). The report also says which saved run it used and whether that run still fits the
  files on disk (`run.files`: current, stale or unknown). Cause matching is heuristic: it compares
  the place and the wording, not what the game meant.
- `xtiger_game_gap` tolerates a few lines of difference: the game often names the line where the
  enclosing statement or block opens, not the line Tiger blames. An entry whose file and cause
  agree with a Tiger report within 3 lines is `matched`; `matched_near_line` counts those and
  `near_line_examples` shows a few (`path:game line (Tiger: line N)`). The cause check is not
  loosened, and a tip or untidy report never matches.
- The update status no longer lumps decided and undecided definitions together: definitions
  without a decision and definitions decided to be restored but still missing from the mod are
  now two separate blocking messages.
- New tools `ck3_run_start` and `ck3_job` for play-tests that take longer than an assistant is
  willing to wait. `ck3_run_start` checks the run, answers at once with a job id and keeps the game
  running inside the xTiger server, so a client that gives up after five minutes has not stopped
  it. `ck3_job` gives the state (running, finished, failed, cancelled, interrupted), the phase, the
  latest progress line, the elapsed time and how many commands were typed and logged; `status`
  can wait up to 30 seconds for the end, `result` returns the report and the screenshot, `cancel`
  stops the game and puts the mod list back, and `list` shows the jobs on record. Jobs are kept on
  disk, a job whose server stopped is shown as interrupted, and each one remembers a hash of the
  mod's files from when it started and says whether they still match. A finished job counts as a
  game run for the update status, like `ck3_run`. `ck3_run` itself is unchanged for short runs,
  and a client timeout is not a game failure.
- Game runs report more honestly: the report opens with what the run confirmed (boot time, mods
  loaded, commands the game logged) and says that whether the mod behaved is for the logs to
  show. It is cut at 1500 lines with a note that says where the whole report is, and the script
  reads at most the last 100000 lines of a very large `error.log`, saying so.
- Fixes found by a final audit: the server stops the games it started when it exits, `ck3_run`
  checks that `play` is a valid list before it starts anything, a project copy is made beside the
  mod instead of inside it, the project export uses the runs that were really pinned, and the
  animated background no longer leaks observers when its screen goes away.
- Connecting Codex no longer writes a second `xtiger` entry when its `config.toml` already has
  one in a form xTiger does not edit (dotted or inline tables). It says so and leaves the file
  alone instead.
- The segmented buttons in Settings (such as Game tests) no longer wrap their text onto a second
  line.

### Testing

- Checked against 13 real mods with no regressions against 1.5.9: the new reports are the real
  ones (booleans used as numbers, the missing trait icons that the base game itself lacks, and a
  tie between two overriding events that is now an error).

### Thanks

Thank you to [shalola_author](https://github.com/shalola-author) for finding and
[reporting the missing trait track icon](https://github.com/9mirx0r/xTiger/issues/4). We
reproduced it, added the check and fixed it in this release.

## xTiger 1.5.9 alpha

Fixes all three doctrine and tenet validation problems reported in
[issue #3](https://github.com/9mirx0r/xTiger/issues/3).

### The validator

- `hostility_level_override` checks that each referenced doctrine or tenet exists and that
  its hostility level is `righteous`, `astray`, `hostile` or `evil`.
- `parameter_opinion_override` checks that each referenced parameter is defined by a
  doctrine or tenet and that its opinion value is numeric.
- Both override maps receive the same checks in doctrines and tenets. Malformed entries
  are reported at their source locations instead of silently accepted.
- Doctrine groups with `category = special` no longer require a group icon. Other categories
  retain the icon check. This removes 14 false-positive reports from the base game.

### Validation

- CK3 1.20.0.4 accepted valid doctrine and tenet references and the special group without an
  icon. Typo controls produced native game errors.
- All 146 library, integration and MCP tests passed, together with formatting and Clippy.
- Vanilla gained no diagnostics. Two repeated runs produced byte-identical JSON output.

### Thanks

Thank you to [shalola_author](https://github.com/shalola-author) for finding and
[reporting these bugs](https://github.com/9mirx0r/xTiger/issues/3), including the special-category
icon requirement. We reproduced the reported problems and fixed all three in this release.

## xTiger 1.5.8 alpha

Rite definitions in `common/religion/rite_types` now receive the missing checks for their
names, descriptions and icons. The report was reproduced and confirmed before the fix.
Six previously unparsed CK3 folders also gain validation, with their connected references checked
and the engine behavior verified through disposable game probes.

### The validator

- Simple localization keys and dynamic descriptions in `name` and `desc` are validated,
  including nested conditions in the documented rite scope. Missing localization, unknown
  triggers and wrong scope types are reported instead of silently accepted.
- When `name` is omitted, its default localization key is checked. Default descriptions are
  validated when present; an omitted description is allowed, as confirmed in the game.
- Explicit rite icons are checked against `NGameIcons|DYNAMIC_RITE_ICON_PATH`.
- Rite creation names, icon packs and doctrine categories are now loaded and validated.
  Their triggers, localization, icon files and category references are checked.
- Rite names also check the derived adjective, adherent and plural adherent localization keys.
  Faith details validate custom localization references and reject unknown fields, and clerical
  elector titles are checked against the title database.
- Triggered tenet icons validate their conditions and image references. Valid triggered blocks
  are accepted, and missing references and unknown fields are reported.
- `knows_tenet` accepts tenet scopes and saved tenet scopes. Static `tenet:` targets check the
  tenet database, and situation phase triggers check parameters declared on phases or in their
  participant modifier sets. Correct syntax and typo controls were verified in CK3.
- An explicit localization language selection is checked even when the mod has no localization
  files for that language. Automatic language selection still follows the mod's available files.
- Holy-order templates now validate required fields, government, title, coat of arms, tenet,
  men-at-arms, localization, triggers, weights and creation effects. `create_holy_order` checks
  explicit template references, and both type selectors accept the game-confirmed military and
  monastic choices. Post-creation effects expose `scope:new_holy_order`; rite and tenet setters
  require scope targets, with database checks for prefixed static targets.
- Morpheme strip rules check Unicode character matches, direction and nonnegative strip counts.
  Plain and uppercase localization formats validate their rule references in code and macros;
  empty rule names are reported. Numeric formatters retain their existing behavior.
- Modifier icon definitions check positive and negative image paths and boolean flags. Console
  groups validate their command-list structure. AI goal types are parsed, with advisory reports
  for contents outside the shipped key-only definition.
- Coat-of-arms atlas options validate integers, size pairs and all four texture-quality entries.
  The canonical `common/coat_of_arms/options/atlases.txt` filename is case insensitive, as confirmed
  in CK3. Other text filenames and nested files receive a filename diagnostic instead of being
  treated as active options. Comment-only readme documentation is allowed; active or malformed
  content in a readme still receives the filename diagnostic.
- Regression tests cover valid definitions and the errors that previously went unnoticed.
  Existing faith, tenet, doctrine, culture, founder, boolean and color checks remain in place.

### The MCP server

- Commands longer than CK3's 255 UTF-16 code unit console limit, empty commands and control
  characters are rejected before launch. Bookmark keys are checked against CK3 and the requested
  mods, so a date or missing bookmark fails before the loading screen. Compact and quoted
  bookmark definitions are accepted; comments, strings and nested keys cannot match a bookmark.
- A run fails when any requested command is unconfirmed or its report is missing. The failure
  includes the report and restores the user's mod list. Real CK3 runs verified both a confirmed
  command and an unknown command that the previous runner could report as successful.

### Validation

- All 144 library, integration and MCP tests passed, together with formatting, Clippy and
  PowerShell syntax checks. The game-backed rite probe detects all 13 invalid cases and leaves
  the valid control clean.
- Ten mods and working copies were checked in English and in all their available languages.
  Community Flavor Pack has zero diagnostics in both runs. Added and removed diagnostics were
  compared with the earlier audit and reviewed against the new checks.
- All six previously unparsed folders now have loaders and valid/error regression controls.
  The coverage probe diagnoses all 208 inventoried paths: 207 parser checks and the common-root
  packaging diagnostic. Semantic checks and CK3 probes establish the tested behavior separately.
- Two repeated vanilla runs produced byte-identical output. Changes from the previous validator
  were reviewed individually; the audit includes the complete comparison data.

### Thanks

Thank you to [shalola-author](https://github.com/shalola-author) for
[reporting the gap in rite validation](https://github.com/amtep/tiger/issues/323#issuecomment-6111062567).
We reproduced the problem, confirmed the missing checks and fixed them in this release.

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
