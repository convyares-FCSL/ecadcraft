# Brief 035: mcp bridge

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 035 (design `ecad/docs/design/035.md`), on `feature/mcp-bridge`, not yet in `dev`:**
an agent drives the running trial over MCP while you watch (section 7D). Agent access is off by
default and set in the Developer section (or `--control PORT` for one run); `ecad-mcp --connect
47823` is headless until the window answers and live once it does; each agent call is one
transaction recorded as the agent's, and you are the author again after it. A plain line in
the Developer section names the agent, its last change and its steps, with Undo session, Pause
and Disconnect; the banner's look and place come with the new UI shell.
- *Automated checks:* see section 7D.
- *Agent-observed:* none yet; no agent has watched the live window with Claude Code.

## 7D. Watch an agent draw (live, brief 035)

The same `ecad-mcp`, started with `--connect`, drives the **running** trial: you watch the
drawing appear, every agent change is recorded as `agent <name>` and every change of yours as
`user`. One registered server does both: it is headless while no ECAD window answers on the port
and live once one does (`ecad_connection` tells the agent which). Agent access is **off** until
you turn it on; when off, nothing listens.

1. **Build once** (from `ecad/`):

   ```sh
   cargo build --release -p ecad-mcp -p ecad-trial
   ```
2. **Turn agent access on in the trial.** Open the trial, unfold **Developer** at the bottom of
   the Project panel, and set **Agent access** to **Read and change** (port 47823). The status
   says `● Listening on 127.0.0.1:47823`. Once only: it is remembered in `agent-access.json` in
   your ECAD settings folder (never in a project). `ecad-trial --control 47823` turns it on for
   one run instead, without remembering it. **Read only** lets an agent read but not change or
   save.
3. **Register the server for every folder** (user scope). Native Windows:

   ```powershell
   claude mcp add --scope user ecad -- C:\dev\ecadcraft\ecad\target\release\ecad-mcp.exe --connect 47823 --write
   ```

   Linux or WSL:

   ```sh
   claude mcp add --scope user ecad -- ~/ecadcraft/ecad/target/release/ecad-mcp --connect 47823 --write
   ```

   Add `--launch` to let Claude open ECAD when it is closed (Windows, and Linux with a display;
   WSL needs WSLg). Leave out `--write` for read-only. A project path at the end chooses what the
   server opens headless when ECAD is closed (without one, headless works on an unsaved project).
4. **Ask:** *"With the ecad tools, draw three wires from 0,0 to 60,0, 0,5 to 60,5, 0,10 to
   60,10."* Expected: the wires appear one by one; the Developer section unfolds and shows a line
   with **Undo session**, **Pause**, **Disconnect** and `Agent claude-code changing last: #3 Wire …
   3 steps`; History shows each by
   `agent claude-code`; the command history has a line `Agent claude-code: WIRE  #… "Wire …"`
   per call.
5. **Start `wire` yourself and leave it half done**, then ask for another wire. Expected: the
   line says `waiting for you to finish WIRE`; after 5 s the agent reports *"Refused: the
   designer is in the middle of WIRE; nothing was changed. Try again when they finish."*. Finish
   or press Esc, then ask again.
6. **Pause** on that line refuses agent changes (*"paused by the designer"*) until **Resume**;
   reads go on. **Disconnect** ends this agent's session (restart the MCP server to connect
   again).
7. **Undo session** takes back the agent's steps; History has the undos by `user`. If you made
   changes after the agent's, it asks first and names them (**Undo all**, **Undo the agent's
   steps after my last one**, **Cancel**).
8. **Close ECAD and ask again:** the agent says *"The ECAD window at 127.0.0.1:47823 is not
   answering (closed?)…"* (once live, the server stays live). Reopen ECAD and it reconnects.

What the agent may do is 7A's: the same tools, checks, caps and refusals, one transaction per
call. Over the port it can also take a `screenshot` of the window. It can never turn agent
access on, widen it or move the port (`agentaccess` is refused to agents), never use your mouse,
keyboard or command line, and never open, save as or close files. An agent's `u` and `redo`
take back only its own steps. One agent changes the project at a time; others read.

The line is deliberately plain: the agent banner's look and place come with the new UI shell.

- *Automated checks:* `ecad/apps/ecad-trial/tests/all/bridge_contracts.rs` (a live trial and an
  `ecad-mcp --connect` server: authors per call, refusals byte-identical to headless, the person
  restored after refused, failing and panicking calls, the cap with batches, Read only and no
  `--write`, the busy rule, raw methods and the token, the agent-undo rule, one writer, modes,
  off by default), `agent_ui.rs` (the Developer row, the settings file, Pause, Disconnect, Undo
  session with and without your steps on top), `ecad/apps/ecad-mcp/tests/all/connect_contracts.rs`
  and `ecad/apps/ecad-mcp/src/launch.rs` (path order, arguments, no display).
- *Agent-observed:* none yet; registering with Claude Code and watching it draw (steps 3 to 8) is
  the owner's check on Windows.
