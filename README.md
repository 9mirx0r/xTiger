<h1 align="center">
  <img src="assets/logo.png" alt="" width="72"><br>
  xTiger
</h1>

<p align="center">
  <b>Tiger for Crusader Kings III 1.20</b><br>
  A validator that finds the mistakes in your CK3 mod before the game does.
</p>

<p align="center">
  <a href="https://github.com/9mirx0r/xTiger/releases">Download</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#how-it-differs-from-tiger">What's different</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="https://github.com/9mirx0r/xTiger/issues">Issues</a>
</p>

xTiger is a fork of [Tiger](https://github.com/amtep/tiger) by amtep, updated for CK3 1.20. It is
the same command-line tool: the same options, the same `ck3-tiger.conf`, the same kind of reports.
It checks typos, missing localization, broken references, effects used in the wrong scope and a lot
more.

xTiger is for CK3 only. For the other Paradox games, use upstream Tiger.

xTiger comes in two forms, and both do the same validation:

- **The command line** is for people who prefer to work by hand and keep AI out of it. It is a plain
  program: no AI, no account, and it stays offline unless you ask it to check for updates. Since
  1.6.5 it is friendlier and easier to steer: it shows what it is doing, ends with a card that says
  what to fix first and which command to type next, lets you pick your mod from a list, and can narrow
  the reports down, summarise them or fail a script with a single option. See [Usage](#usage).
- **[The desktop app](docs/app.md)** does it from a window, with your real mods in front of you.
  The AI assistants below are optional extras.

<p align="center">
  <img src="assets/cli/validate.png" alt="The command line checking a real mod: Qubis, the game and the mod it found, one line per stage, the first reports and a closing card with what to look at first and the commands to try next" width="100%">
</p>
<p align="center"><sub><b>The command line.</b> A real run over a Workshop mod.</sub></p>

<p align="center">
  <img src="assets/app/results.png" alt="The results screen of the desktop app checking the same mod: a severity bar, the list of reports grouped by severity and the broken line of the selected one" width="100%">
</p>
<p align="center"><sub><b>The desktop app.</b> The same mod, with the reports grouped by severity and one click to open the line in VS Code.</sub></p>

## Download

Every [release](https://github.com/9mirx0r/xTiger/releases) has two things to pick from, and they do
the same validation. You need only one.

| You want | Download | Notes |
| --- | --- | --- |
| **A window** (the desktop app) | `xTiger_<version>_x64-setup.exe`, or `xTiger_<version>_x64_portable.zip` | Windows. The app updates itself. See [the app](docs/app.md). |
| **A terminal** (the command-line validator) | `ck3-tiger-windows-v<version>.zip` or `ck3-tiger-linux-v<version>.tar.gz` | Unzip it anywhere, nothing to install. `ck3-tiger update` keeps it up to date (see [Updating](#updating)). |

## Usage

On Windows you can double-click `ck3-tiger-auto.exe`. It finds your mods, asks which one to check
and writes a log to the CK3 `logs` folder.

From a terminal:

```bash
ck3-tiger path/to/your_mod/descriptor.mod
```

Add `--game "<CK3 install folder>"` if the game isn't where Steam usually puts it. Options worth
knowing:

- `--json` prints the reports as JSON.
- `--suppress old.json` hides everything already in an earlier `--json` output, so you only see
  what's new. xTiger gives the same reports for the same files every time, so this stays reliable.
- `--show-vanilla` also reports problems in the base game.
- `--summary` prints the counts by severity, key and file first. Start there on a mod with thousands
  of reports. `--format short` prints one line per report (`file:line: severity key: message`), easy to
  grep or to open from an editor.
- `--severity error`, `--only-key missing-item`, `--skip-key unknown-field` and `--path events` narrow
  the output without editing a config file. They combine.
- `--fail-on error` makes the exit status 1 when a report at that severity or worse is printed, for CI
  or a pre-commit check. Without it the status stays 0. Status 2 means the run could not be done.
- `ck3-tiger explain missing-item` says what a key means, how to fix it and how to silence it. Without
  a key it lists them all.

In a terminal xTiger-CK3 is made to be read by a person: Qubis and the game and mod it found, a line
per stage with how long it took, then the reports and a short card at the end (counts, what to look at
first, which files hold most of them and the commands to try next). Tips and untidy reports are folded
into a count; `--severity tips` shows everything. Run `ck3-tiger` with no mod and it lists the mods in
your mod folder to choose from. All of this is written to the terminal's messages channel (stderr), so
when that is redirected, or in CI, none of it appears and the output is the same as before. Set
`XTIGER_FORCE_TTY=1` to get the terminal view in a captured output.

`ck3-tiger --help` lists the rest.

<p align="center">
  <img src="example.png" alt="Sample output: reports with the file, the line and the broken code underlined" width="100%">
</p>

### Updating

`ck3-tiger update` downloads the newest release, checks its SHA-256, unpacks it and replaces the
program, showing each step as it goes and the release notes when it is done. Nothing is installed
until the download has been checked.

<p align="center">
  <img src="assets/cli/update.png" alt="Updating from the command line: the new-version line at the end of a validation, ck3-tiger update --check, and a full ck3-tiger update with its stages and what is new" width="86%">
</p>

- `ck3-tiger update --check` only says whether a newer version exists.
- `ck3-tiger update 1.6.5-alpha` installs a particular release, older ones included.
- The validator never goes online by itself. The first time you run it at a terminal it asks once
  whether to tell you when a new version is out. Say yes and it asks GitHub at most once a day and
  prints one line after a validation; say no and it stays offline. Change it at any time with
  `ck3-tiger update --notices on` or `--notices off`. In CI, scripts and redirected output it never
  asks and never prints the line. `XTIGER_NO_UPDATE_CHECK=1` turns it off for good.

To choose languages, load parent mods for a submod or silence reports you don't care about, put a
[`ck3-tiger.conf`](ck3-tiger.conf) in your mod folder. See also the [filter guide](filter.md) and
the [annotations guide](annotations.md).

## How it differs from Tiger

- It knows CK3 1.20.0.4. Upstream Tiger is made for an earlier version, so on 1.20 it reports
  things the game now accepts.
- It is checked against the game itself. Run over the unmodded base game, xTiger reports
  about **6,300** reports at warning level or above, fewer than **2,000** of them errors. Most of the rest
  are flaws in the game's own files; [AUDIT.md](AUDIT.md) lists what is left and how we look for gaps
  after a game update.
- Nothing gets relaxed on a hunch. Before xTiger accepts a field, a small test mod uses it once
  correctly and once with a typo, and the game's own `error.log` decides.
- The same mod gives the same reports on every run.
- It is not perfect. Some triggers and effects new in 1.20 are only partly checked. The
  [changelog](CHANGELOG.md) says what changed in each release.

## More ways to use it

- **[The desktop app](docs/app.md)** checks your mods from a window: it finds the game and your
  mods, groups the reports and opens the broken line in VS Code.
- **[AI assistants](docs/ai-assistants.md)** can use xTiger through an MCP server, to validate and
  test a mod in the game for you. It is optional and separate from the validator.
- unLomTrois's [VS Code extension](https://github.com/unLomTrois/ck3tiger-for-vscode-2) shows the
  reports in the Problems tab, and Bahmut's
  [GitHub Action](https://github.com/kaiser-chris/tiger-action-public) runs the validator in your
  workflows. Both were made for upstream Tiger and use the same `ck3-tiger` command.

## Building it yourself

You need Rust (stable).

```bash
git clone https://github.com/9mirx0r/xTiger.git
cd xTiger
cargo build --release -p ck3-tiger
```

The binary is `target/release/ck3-tiger`. To build the app and the installer too, see
[docs/app.md](docs/app.md#building-it).

## Contributing

Bug reports are welcome. If a problem also happens in upstream Tiger, report it there. Anything
about CK3 1.20, the app or the MCP server belongs in this repository's
[issues](https://github.com/9mirx0r/xTiger/issues). The most useful report is a false positive on
vanilla files, or a mistake the game rejects that xTiger accepts. See
[CONTRIBUTING.md](CONTRIBUTING.md). The plans are in the [roadmap](ROADMAP.md).

## Credits and license

xTiger stands on [Tiger](https://github.com/amtep/tiger) by amtep and its contributors. The
validation engine is their work, and the [contributors list](CONTRIBUTORS.md) keeps their names.
This fork updates the CK3 tables and validators and adds the app and the MCP server.

Everything is under the [GNU GPL v3](LICENSE). Copyright of the xTiger changes (C) 2026 Qubis; the
original copyright of Tiger stays with its authors. The git history records what changed.

xTiger is developed with the help of AI tools, under the direction of its maintainer. Every
validator rule comes with a test fixture, and the results are checked against the game's own files.
