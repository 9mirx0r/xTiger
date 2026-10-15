# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users
CK3 (Crusader Kings III) mod authors: people who make, port and maintain mods, including players on non-Steam copies who cannot use the Workshop. Their job is to find out what is wrong with a mod, fix it, and bring it to a new game version. Many work with an AI assistant (Claude, Codex, Cursor, VS Code) that drives xTiger over MCP while they watch the work happen in the app.

## Product Purpose
xTiger is a desktop app (Tauri 2, plain HTML/CSS/JS front end) plus an MCP server (`xtiger-mcp`) built on a fork of Tiger, the Paradox mod validator. It checks a CK3 mod against the game's own files, shows the reports sorted by what matters, compares runs (fixed vs new), prepares mod updates for a new game version, and lets an AI assistant validate, edit and play-test through the same tools. Success: a modder goes from "my mod breaks on the new patch" to a clean validation, with the user always able to see what the AI did.

## Positioning
- Validates and updates: renames, override diffs against the game, and migration with backup (Menorá milestone), plus comparing Tiger's reports with the game's real `error.log`.
- The AI drives, the user watches: 18 MCP tools (validate, run the real game, read logs), a live session journal in the app, and a request queue the user fills for the AI.
- CK3-only for now and deeper than upstream Tiger: other Paradox games were dropped to be exact on CK3 1.20. The user may add a Stellaris validator later, once the CK3 product is mature; the Home screen already shows it as "Coming soon". Do not build or imply Stellaris support before then.

## Operating Context
Windows desktop. Mods live in the Paradox mod folder, a local folder, or a playset; the CK3 install may be a non-Steam copy. Validation can take seconds to minutes, so the validating screen estimates time from earlier runs. The app sits next to the editor (VS Code, Cursor) and the game, and talks to assistants by writing their MCP configuration files (Claude Desktop, Claude Code, Codex, Cursor, VS Code, Windsurf).

## Capabilities and Constraints
- Reports by severity (fatal, error, warning, untidy, tips), grouped by file, key or severity, with detail, "open in editor", export, and "Prepare update report" for an AI assistant.
- Release flow: updates are announced by GitHub releases with a setup.exe asset and a bumped semver tag; a plain push does not trigger the in-app update notice.
- Validation output must stay deterministic: the same mod gives the same reports every run. A check may only be relaxed after the game itself confirms it.
- Everything in the repo is English: code, copy, docs, CLI and error messages.
- No personal data in the repo (names, home paths, tokens). Paths come from environment or arguments.
- CSP: no inline scripts, no external images or fonts; assets are bundled.
- Open decisions: Stellaris support timeline; whether the app will share one dependency/playset model with the MCP in every flow.

## Brand Commitments
Name: xTiger. Mascot: Qubis, a small blob shown in states (idle, worried). Logos of CK3, Claude, Codex, Cursor, VS Code and Windsurf are bundled only to identify the games and assistants; they belong to their owners. Tone: plain, direct, never blaming the modder.

## Evidence on Hand
Real use on test mods under `test-mods` (not in the repo): Sons of Judah, Community Flavor Pack, Elder Kings 2, RICE, VIET Events, with measured report counts and triage against the game's files. Public GitHub repo with tagged alpha releases (1.0.0-alpha, 1.5.0-alpha). No testimonials, user numbers or benchmarks exist; do not invent them.

## Product Principles
1. Honest signal over volume: never hide or soften a real problem; sort and group so the important ones come first.
2. The user must always see what the AI did: sessions, requests and results stay visible and interruptible.
3. Show change, not only state: fixed versus new since the last run is the main feedback loop.
4. Confirm against the game: the game's own files and `error.log` decide what is a bug.
5. Small and calm: the tool runs next to the game and the editor, so it stays quiet and responsive.

## Accessibility & Inclusion
Dark and light themes; motion honors `prefers-reduced-motion`; keyboard navigation through the report list (arrow keys, Enter, Ctrl+F). Users may read English as a second language: keep copy short and literal.
