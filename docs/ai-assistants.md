# Using xTiger with an AI assistant

This is optional. The [validator](../README.md) and the [app](app.md) work without any assistant.

xTiger comes with an MCP server that lets an assistant such as Claude validate your mod, read the
reports, start CK3, type console commands and look at screenshots of the game.

<p align="center">
  <img src="../assets/flow.svg" alt="How a fix flows: validate, read the reports, ask your assistant, test it in the game, ship" width="100%">
</p>

So you can ask for a new event, and the assistant writes it, validates it, fixes what it broke,
fires the event in the game and checks the window looks right. You get back something that works.

To set it up, open the app's Settings and press **Connect** next to Claude Desktop, Claude Code,
Codex, Cursor, VS Code or Windsurf. The server finds the game, your mods and the validator on its
own. To set it up by hand, see [`xtiger-mcp/README.md`](../xtiger-mcp/README.md).

<table>
  <tr>
    <td width="50%"><img src="../assets/app/activity.png" alt="The AI activity screen: an assistant validating a mod right now, the requests waiting for an assistant and today's calls grouped by job"></td>
    <td width="50%"><img src="../assets/app/settings.png" alt="The Settings screen with the AI assistants, each with its logo and a Connect or Disconnect button"></td>
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
