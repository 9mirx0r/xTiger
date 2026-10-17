# The xTiger desktop app

<p align="center">
  <img src="../assets/app/results.png" alt="The results screen of the xTiger app: a severity bar, a summary of what was fixed and what is new, the list of reports and the broken line of the selected one" width="100%">
</p>

The app is optional: the [validator](../README.md) works on its own. Download `xTiger_<version>_x64-setup.exe` from the [releases](https://github.com/9mirx0r/xTiger/releases)
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
    <td width="50%"><img src="../assets/app/home.png" alt="The Home screen, with the Crusader Kings III card and a Stellaris strip marked as coming soon"></td>
    <td width="50%"><img src="../assets/app/mods.png" alt="The mods screen, with a card for each mod and its last result"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Home.</b> Pick a game and go.</sub></td>
    <td align="center"><sub><b>Your mods.</b> Local, Workshop and added by hand.</sub></td>
  </tr>
</table>

## Building it

You need Rust (stable) and the Tauri CLI (`cargo install tauri-cli --version "^2"`). Then
`xtiger-app/build-release.ps1` builds everything: the validator, the MCP server, the app, the
installer and the portable zip.
