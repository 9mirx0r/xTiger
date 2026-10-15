<p align="center">
  <img src="assets/hero.svg" alt="xTiger: Qubis, a small blue pixel creature with a magnifying glass, watches a beam scan a mod's script and flag three reports" width="100%">
</p>

<p align="center">
  <a href="https://github.com/9mirx0r/xTiger/releases/latest"><b>Download</b></a> ·
  <a href="#the-app">The app</a> ·
  <a href="#with-an-ai-assistant">With an AI assistant</a> ·
  <a href="#the-command-line">Command line</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="ROADMAP.md">Roadmap</a>
</p>

xTiger finds the mistakes in your Crusader Kings III mod before the game does: typos, missing
localization, broken references, effects used in the wrong scope, and a lot more.

It's a fork of [Tiger](https://github.com/amtep/tiger) by amtep, brought up to date for CK3
1.20.0.4. On top of that it adds a desktop app, so you never need a terminal, and a way to let an
AI assistant check and test your mod for you. Qubis, the little reviewer, keeps an eye on things.

xTiger is for CK3 only. For other Paradox games, use upstream Tiger.

<p align="center">
  <img src="assets/features.svg" alt="What xTiger does: a validator you can trust, a desktop app with no terminal, an AI assistant that does the fixing, and following an update step by step" width="100%">
</p>

## The app

<p align="center">
  <img src="assets/app/results.png" alt="The results screen of the xTiger app: a severity bar, a summary of what was fixed and what is new, the list of reports and the broken line of the selected one" width="100%">
</p>

Download `xTiger_<version>_x64-setup.exe` from the [releases](https://github.com/9mirx0r/xTiger/releases)
and run it. No administrator rights needed. If you'd rather not install anything, grab the portable
zip, unzip it anywhere and run `xTiger.exe`.

- It finds CK3 and all your mods on its own, local and Workshop alike.
- Double-click a mod to check it.
- The results open with a summary: a severity bar you can click to filter, what was fixed and what
  is new since your last check, and the files with the most reports.
- Reports come grouped by file, kind or severity. Each one shows the broken line, and one click
  opens it in VS Code.
- Follow a mod's update: a strip under the summary says what still blocks it, and a note on any
  report records why it is fine or that it is still a defect. Notes never hide a report.

<table>
  <tr>
    <td width="50%"><img src="assets/app/home.png" alt="The Home screen, with the Crusader Kings III card and a Stellaris strip marked as coming soon"></td>
    <td width="50%"><img src="assets/app/mods.png" alt="The mods screen, with a card for each mod and its last result"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Home.</b> Pick a game and go.</sub></td>
    <td align="center"><sub><b>Your mods.</b> Local, Workshop and added by hand.</sub></td>
  </tr>
</table>

## With an AI assistant

xTiger comes with an MCP server that lets an assistant such as Claude validate your mod, read the
reports, start CK3, type console commands and look at screenshots of the game.

<p align="center">
  <img src="assets/flow.svg" alt="How a fix flows: validate, read the reports, ask your assistant, test it in the game, ship" width="100%">
</p>

So you can ask for a new event, and the assistant writes it, validates it, fixes what it broke,
fires the event in the game and checks the window looks right. You get back something that works.

To set it up, open the app's Settings and press **Connect** next to Claude Desktop, Claude Code,
Codex, Cursor, VS Code or Windsurf. The server finds the game, your mods and the validator on its
own. To set it up by hand, see [`xtiger-mcp/README.md`](xtiger-mcp/README.md).

<table>
  <tr>
    <td width="50%"><img src="assets/app/activity.png" alt="The AI activity screen: an assistant validating a mod right now, the requests waiting for an assistant and today's calls grouped by job"></td>
    <td width="50%"><img src="assets/app/settings.png" alt="The Settings screen with the AI assistants, each with its logo and a Connect or Disconnect button"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>AI activity.</b> What the assistant is doing, and why.</sub></td>
    <td align="center"><sub><b>AI assistants.</b> One click to connect.</sub></td>
  </tr>
</table>

What the assistant can do:

- Validate a mod, read the reports and see what is new since the last check, whoever ran it.
- Search the base game's files and look up the game's own docs of triggers and effects.
- Test your mod in the game with a launcher playset, **in the background** by default: the game
  starts minimized and never takes your focus, so you can keep playing something else.
- Follow a mod's update in a durable project record, with a ledger of what was preserved, changed,
  removed or added, and a status that never calls a clean static check a tested update.

From the app you can hand work over: **Ask AI to fix** on a report, or **Prepare update report**
on the results of a mod that is behind the game, leaves a request the assistant picks up the next
time you talk to it. The AI activity screen shows each request, who picked it up and how it ended.

## The command line

The release archives also include `ck3-tiger`, the validator itself. On Windows you can simply
double-click `ck3-tiger-auto.exe`: it finds your mods, asks which one to check, and writes a log to
the CK3 `logs` folder. From a terminal:

```bash
ck3-tiger path/to/your_mod/descriptor.mod
```

Add `--game "<CK3 install folder>"` if the game isn't where Steam usually puts it. A few options
worth knowing:

- `--json` prints the reports as JSON.
- `--suppress old.json` hides everything already in an earlier `--json` output, so you only see
  what's new. xTiger gives the same reports for the same files every time, so this stays reliable.
- `--show-vanilla` also reports problems in the base game.

`ck3-tiger --help` lists the rest, and `ck3-tiger update` fetches the latest release.

To choose languages, load parent mods for a submod or silence reports you don't care about, put a
[`ck3-tiger.conf`](ck3-tiger.conf) in your mod folder. See also the [filter guide](filter.md) and
the [annotations guide](annotations.md).

## How it differs from Tiger

<p align="center">
  <img src="assets/vanilla.svg" alt="Reports on the unmodded base game: about 144,000 with Tiger, about 15,000 with xTiger" width="100%">
</p>

| | Tiger | xTiger |
|---|---|---|
| CK3 version | 1.19 | 1.20.0.4 |
| Reports on unmodded vanilla | about 144,000 | about 15,000 |
| Same mod, same reports every run | not always | yes |
| Desktop app and AI integration | no | yes |
| Other Paradox games | yes | no |

The vanilla count matters more than it seems: when the base game alone makes thousands of
warnings, the real mistakes in your mod get lost in the noise.

Nothing gets relaxed on a hunch. Before xTiger accepts a field, a small test mod uses it once
correctly and once with a typo, and the game's own `error.log` decides. The real field passes, the
typo is still reported.

It isn't perfect yet. Some triggers and effects new in 1.20 are only partly checked, and fewer
reports doesn't automatically mean more accurate ones. The [changelog](CHANGELOG.md) has the
details.

## Roadmap

<p align="center">
  <img src="assets/roadmap.svg" alt="The xTiger roadmap: Frankokratia (released), Menorá (next), Vatican (later)" width="100%">
</p>

Next is **Menorá**, a mod updater that brings old mods, or a whole mod list, up to the current
game without cutting their content. After that comes **Vatican**, with much more help for mod
creators, by hand and through an AI assistant. The details are in [`ROADMAP.md`](ROADMAP.md).

## Building it yourself

You need Rust (stable).

```bash
git clone https://github.com/9mirx0r/xTiger.git
cd xTiger
cargo build --release -p ck3-tiger -p xtiger-mcp
```

For the app you also need the Tauri CLI (`cargo install tauri-cli --version "^2"`). Then
`xtiger-app/build-release.ps1` builds everything: the validator, the MCP server, the app, the
installer and the portable zip.

## Other tools

These were made for upstream Tiger and work with xTiger too:

- unLomTrois's [VS Code extension](https://github.com/unLomTrois/ck3tiger-for-vscode-2) shows the
  reports in the Problems tab.
- Bahmut's [GitHub Action](https://github.com/kaiser-chris/tiger-action-public) runs the validator
  in your workflows.

## Credits and license

xTiger stands on [Tiger](https://github.com/amtep/tiger) by amtep and its contributors. The
validation engine is their work. This fork updates the CK3 tables and validators and adds the app
and the MCP server. Copyright of the xTiger changes (C) 2026 Qubis and the original copyright
of Tiger stays with its authors. Everything is under the [GNU GPL v3](LICENSE), and the git
history records what changed. The app bundles the fonts Geist, Geist Mono and Pixelify Sans, under the SIL Open
Font License (see [`xtiger-app/ui/fonts`](xtiger-app/ui/fonts)).

If a bug also happens in upstream Tiger, report it there. Anything about CK3 1.20, the app or the
MCP server belongs in this repository's [issues](https://github.com/9mirx0r/xTiger/issues).
