# Brief 043: the agent banner and History authors (U4d)

Part of the [quick start](../quick-start.md). A section number refers to the quick start's own
sections; 7D is in [035-mcp-bridge.md](035-mcp-bridge.md).

## What this brief changed

**Branch `feature/ui-agent-banner` (brief 043)**, cut from `dev` at `8baf3de7`.

| Where | What is there now |
| --- | --- |
| Agent banner | 32 px under the top bar while an agent is connected: a bar in the state's colour (green `agent` while it works, amber while a write waits or writes are paused, grey when reading only or finished, red when access stopped), the agent's name, `connected since 14:02`, the state, the last write (`last: #57 Wire W12 · 14:02:11`, a go-to: a click opens History at it), the last refusal and other readers; on the right **Undo session (N)**, **Pause writes** (then **Resume**), **Disconnect**, and **Dismiss** once finished. Connecting no longer opens the Text panel |
| Top bar | The agent pill says `Agent <name>` in the state's colour while an agent is shown; a click hides the banner (the page gets the 32 px back) and a second click shows it. A new agent session shows its banner again. With no agent, the pill opens the Text panel as before (Agent access is there until Settings has it) |
| History ▸ Steps | Every step has its author: **You**, or the agent's name in green. An agent's consecutive steps are one row, `▸ [claude-code] 14 changes · 20:31–20:44`; a click opens it to its steps. While that agent is still connected the row has **Undo session**, the banner's own undo (same confirmation when your steps are among the agent's). Once the session ended, its steps undo one by one like any others |
| The page | The agent's last write has a dashed green outline with the agent's name on a chip, until its next write or until you dismiss the banner; your own edits in between keep it. A step selected in History's Steps list gets the same outline in blue with its author. Neither is part of the drawing: they are never plotted or printed, and they are not shown over Lists, Library or Publish |

No name is stored for a person: History says **You** (owner, 2026-11-13).

## Try it

1. Turn agent access on and register the server as in 7D, steps 1–3.
2. Ask Claude to draw three wires (7D step 4). Expected: the banner appears under the top bar
   with the agent's name, `connected since …`, `changing` and **Undo session (3)**; the last
   wire has a dashed green outline with the agent's name.
3. Draw a wire yourself. Expected: the outline stays on the agent's wire; the banner does not
   change.
4. Click the agent pill in the top bar. Expected: the banner hides; click again, it is back.
5. Click the banner's `last: #… Wire …`. Expected: History opens with the Steps list at that
   step; the page keeps its zoom.
6. In History ▸ Steps, find the row `claude-code · 3 changes · …` with **Undo session**; click
   the row to open it: three steps with the agent's chip; your wire says **You**. Click a step:
   its wire is outlined in blue.
7. Click **Undo session** (on the row or the banner). Expected: the agent's three wires go; your
   wire stays (or the confirmation names it, if it came between the agent's).
8. Ask Claude to say goodbye (or click **Disconnect**). Expected: the banner turns grey,
   `finished · 3 steps`, with **Dismiss**; History's row no longer offers Undo session.

## Checks

- *Automated (agent, native Windows 11, 2026-11-13):* `cargo test -p ecad-trial --test all
  agent_banner_ui` (4 tests: the states and controls by 035's element names; the slot, pill, go-to,
  Pause, Undo session and Dismiss over the live bridge; History's groups and chips; the outline,
  not an entity, absent over Lists, and no project read over 300 idle frames) and `cargo test -p
  ecad-cadcraft --test all history_sessions_contracts` (session grouping). 035's tests unchanged.
- *Agent-observed:* screenshots in the pull request, from the headless renderer.
- *Owner-tried:* not yet.
