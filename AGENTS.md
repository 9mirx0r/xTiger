# Instructions for AI coding agents

There are no open tasks for agents in this repository. Do not start work that the maintainer has not asked for in the conversation.

## Project in one paragraph

xTiger is a fork of [Tiger](https://github.com/amtep/tiger), a mod validator for Paradox games. **xTiger supports Crusader Kings III (CK3) only.** Support for the other games (Victoria 3, Imperator, EU5, HoI4) has been dropped; a few leftover flags and comments remain in `src/`. Do not add support back or write code for them. The `xtiger-mcp/` crate is the MCP server that lets an AI assistant run the validator and drive the game, and `xtiger-app/` is the desktop app.

## Rules (no exceptions)

1. **Everything is in English**: code, comments, docs, CLI output, error messages.
2. **No personal data in the repo**: no user names, home paths, install paths, e-mail addresses, API keys, tokens or machine names. Paths come from environment variables or arguments. Before you finish, search your changes (`git diff`) for these.
3. **Never silence a real warning.** A check in `src/` or `tiger-tables/` may only be relaxed after the game itself confirms it: a throwaway mod uses the field once correctly and once with a typo, and the game's `error.log` decides.
4. **Validation output must stay deterministic.** The same mod must give the same reports on every run. After changes in `src/`, compare a `--show-vanilla --json` run on vanilla against the previous one.
5. **Do not use git to change anything** (commit, push, branch, merge, rebase, reset, stash, tags, releases) unless the maintainer asks. Reading git state is fine.
6. **Do not publish anything** unless the maintainer asks: no pushes, pull requests, issues, packages, or `ck3-tiger update`.
7. Keep the diff small and reviewable. Run `cargo fmt --all`. Do not reformat unrelated files.
8. Keep each file's existing line endings. Do not convert them.

## Checks

```bash
cargo fmt --all --check
cargo clippy -p ck3-tiger
cargo test -p tiger-lib --lib
cargo clippy -p xtiger-mcp --all-targets
cargo test -p xtiger-mcp
```
