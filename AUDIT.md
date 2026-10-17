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

## Open backlog from the vanilla self-check (1.20, error and fatal)

Known groups, none fixed yet unless marked. Counts are from the run of 2026-10-16.

- `common/succession_election`: fixed (issue #8).
- 134 `strict-scopes` for `scope:target` and 100+ for other scopes: scopes saved by the engine for a
  character interaction, on_action, scheme or activity that Tiger does not list.
- 107 `unknown-list`: `is_in_list` of lists that vanilla fills in a way Tiger does not see.
- 30 `dynamic script values are not allowed here` in script values.
- Rites (PAM): `clerical_region_holder`, `rite_has_doctrine`, `rite_can_choose_tenet`, scripted rule
  `can_edit_rite`, `faith = same_rite`, doctrine `divergence = 2.5` (accepts integers only).
- Smaller format gaps: `disable_regnal_numbers = never|always`, `royal_court = landed`,
  `attacker_wargoal_percentage = 1.5`, pool selector `gender = female|male`, `owner` values for great
  projects, `look_at` and light fields in portrait environments, `fog_color = hex{ ... }`,
  `adjacencies.csv` with 9 fields.
