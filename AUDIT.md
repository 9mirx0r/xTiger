# Auditing the validators against a new game version

Issues like #8 (`common/succession_election` still on the pre-1.20 shape) come from one cause: the game
changed a file format and the validator still accepts the old one. Four checks find these before a mod
author does. Run them after every game update.

## 1. Vanilla self-check (finds false positives, highest yield)

The game loads its own files without errors, so every report Tiger makes about a vanilla file is a Tiger
gap or a flaw of the game.

    cargo build --package ck3-tiger --bin ck3-tiger
    python -I tools/vanilla_selfcheck.py target/debug/ck3-tiger.exe "<game root>" <out dir> error,fatal

`vanilla_groups.md` lists the reports grouped by key and message, biggest group first. Work through the
groups that are about a format (`choice`, `validation`, `unknown-field`, `structure`, `range`) first: they
are one-line fixes and the vanilla file itself proves the right answer. Re-run after a change: the group
must disappear and no new group may appear. Pass `warning` and a path prefix (`common`) to look at one area.

Things to remember when reading it: the DLC the player does not own can leave genuine `missing-item`
reports, and reports about history and bookmarks are mostly about the data, not about Tiger.

## 2. `.info` diff (finds both directions)

    python -I tools/audit_info.py "<game root>" out.md

Every item type has a `_*.info` structure document in the game. The script lists the names the document
mentions that the validator does not (NEW) and names the validator accepts that neither the document nor
any vanilla file uses (STALE). Both lists are suspects. It compares names, not paths, so it cannot see a
name that kept its place and changed meaning; check 3 catches those.

## 3. The game as oracle

For a suspect, write the smallest mod that uses it, run it (`ck3_run`, `ck3_cases`) and read the game's
`error.log`; `xtiger_game_gap` shows what the game rejected that Tiger did not. A rule that Tiger has and
the game does not enforce is a false positive; a game error that Tiger did not report is a missing check.

## 4. Real mods

Run `xtiger_validate` and then a play-test over installed workshop mods: the old-format ones (made for the
previous version) are the negative corpus, the updated ones the positive corpus. Reports that come up in an
updated mod that loads cleanly in the game are false positives.

## Backlog from the vanilla self-check (1.20, error and fatal)

Counts are from the runs of 2026-10-16: 2,735 error reports at the start, 2,180 after the fixes (the
changelog lists them). What is left:

- 332 `strict-scopes`, almost all in events (`events/dlc`, `events/activities`, ...): the scopes of an
  event depend on who fires it, and Tiger follows the call only through the paths it understands. These are
  inference limits, not gaps, except where a caller is still missing its own scopes: check a group by
  looking for the common caller (the 134 `scope:target` reports were scheme effects and are fixed).
- About 30 `unknown-list`: lists that vanilla fills in a way Tiger cannot follow (inference limits).
- 30 `dynamic script values are not allowed here` in some script values.
- Rites (PAM): missing items (`rite_has_doctrine`, `rite_can_choose_tenet` and the DLC's own items).
- Small format gaps still open: `look_at = { 0 0 }` with two numbers in the ep3 portrait environments,
  `adjacencies.csv` with 9 fields, `faith = scope:x.faith` as a filter inside a script value iterator,
  `value < script_value` inside a `piety = { }` trigger, `is_target_in_variable_list` without `target`,
  `domicile ?= { }` in character history, the tutorial and flavorization typos that vanilla has.
- Reports about history, bookmarks and map data are mostly flaws of the data, not of Tiger.
