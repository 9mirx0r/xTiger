# Contributing to xTiger

Thanks for helping. The most useful contributions are reports and fixes about what the validator
gets wrong.

## Reporting a problem

Open an [issue](https://github.com/9mirx0r/xTiger/issues/new/choose). Please say which of these it is:

- **A false positive:** xTiger reports something the game accepts. Include the report, the few
  lines of script it points at, and what the game does with them.
- **A missed mistake:** the game logs an error (`error.log`) that xTiger did not report.
- **Something broken:** a crash, a wrong path, or a result that changes between runs.

Include the xTiger version (`ck3-tiger --version`) and the CK3 version. If the problem also
happens with upstream [Tiger](https://github.com/amtep/tiger), report it there too.

## Sending a change

1. Build and test: `cargo test -p tiger-lib`, then `cargo fmt --all` and `cargo clippy`.
2. A change to a validator rule needs a fixture in `tests/files/` and a test in `tests/test.rs`.
   The fixture uses the field once correctly and once with a typo.
3. Check it against the game's own files: [AUDIT.md](AUDIT.md) explains the vanilla self-check.
   A change may remove reports on vanilla, never add new ones.
4. Nothing gets relaxed on a hunch: the game's `error.log` decides whether a field is valid.
5. Keep the diff small and write everything in English.

Code and issues are under the [GNU GPL v3](LICENSE).
