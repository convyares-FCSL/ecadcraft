# Agent access: why it works the way it does

An agent (Claude Code or any MCP client) can drive ECAD in two ways: **headless**, where `ecad-mcp` opens a
project of its own and no one watches, and **live**, where `ecad-mcp` talks to the open ECAD window and the
person watches every change appear. This note explains the design of both and the reasons behind it. How to
turn access on and use it is in [`../user/agent-access.md`](../user/agent-access.md); the tools and the
command-line options are in [`../agents/mcp.md`](../agents/mcp.md); the port's methods are in
[`../agents/control-protocol.md`](../agents/control-protocol.md).

## The aim

The person asks an agent to draw and watches it happen. Every step the agent takes is recorded as the
agent's, and nothing the person does is ever recorded as the agent's. The person can stop the agent, and
take back what it did, without losing their own work.

## One gate, two hosts

The policy (what an agent may run with writes off or on), the per-call caps and their roll-back, the
agent-undo rule and the author all live in one place, the agent gate in `ecad-engine` (`agent/gate.rs`,
`agent/policy.rs`). The headless server and the live bridge in the desktop app call the same functions; only
the session and project they pass differ.

Why: the same project state must give the same reply, word for word, whichever way the agent is connected. A
refusal, a cap or a roll-back that behaved differently live would be a second policy to test and to trust.

- A command runs through the session's own `execute`, never the app's UI path, so commands that exist only
  in the user interface stay unknown to an agent in both modes.
- An agent runs only what is offered: ECAD's own commands and a short, reviewed list of CADCraft drawing
  commands, each marked read or write. Everything else is refused. Why: CADCraft adds commands with every
  upstream sync, and a denylist lets a new one that touches files (under a parameter name nobody listed) through
  silently; an allowlist fails closed, and a test fails until each new upstream command is classified.
- Some commands are never run by an agent, named so even if one were offered by mistake: opening, closing,
  Save As and other file and session commands, LISP, every command CADCraft classifies as touching files, and
  `agentaccess` itself. Parameters that name a file stay refused too, as a second fence. An agent can never turn its own access on, widen it or move the port; the
  refusal says the person sets it in the ECAD window.
- `save` is allowed with writes on, to the project's own file only. Save As and a never-saved project stay
  with the person.

## The author is set for one call only

For each agent call the gate records `Author::Agent(<name>)`, runs the call, and restores the person as the
author afterwards: on success, refusal, error and panic. If the author cannot be set, the call is refused and
nothing runs. At the top of every frame the window checks again that the author is the person and resets it
if not, before any of the person's input in that frame is handled.

Why: the person's input can only arrive between frames, so checking at the frame boundary makes it
impossible for a person's edit to be recorded under the agent's name, even after a failure inside a call.

## The live bridge

The window listens only when the person has turned **Agent access** on (Settings ▸ Agent access: Off, Read
only, Read and change, and a port; default port 47823). The preference is stored per user, outside any
project, and a damaged or unknown preference reads as Off. `--control PORT` on the desktop app's command line
turns access on as Read and change for one run and writes nothing. No environment variable turns it on: one
is too easy to inherit without noticing.

The transport is CADCraft's own control server, used unchanged: loopback only, token-checked, bounded. Its
requests never reach the drawing app directly. The window answers ECAD's own methods (`ecad.hello`,
`ecad.execute`, `ecad.tool`, `ecad.bye`), forwards a short list of reads (screenshot, render, inspect) and
refuses everything else without running it.

`ecad-mcp --connect <port>` starts undecided. Each call probes the port: when the window answers, the server
is **bridged** and stays bridged; when nothing answers, the call runs headless. Headless sticks once it has
recorded a transaction or saved, because the work would otherwise be split between two places. Until then a
later call probes again, so opening the window after the agent started still connects. `--launch` opens the
desktop app when nothing answers (Windows and Linux; on Linux only when there is a display).

## Calls run on the UI thread, between frames

Agent requests wait in a bounded queue. At the top of each frame the window takes up to 8 of them and runs
each one whole (author set, command, cap check, roll-back, author restored) before the person's input of that
frame. The rest wait for the next frame, which the window asks for at once.

Why: the project has one writer at a time, and the person's session is the one the agent edits. Running the
call between frames means no lock is shared with drawing, the person never sees half a call, and a burst of
calls cannot freeze the window.

### The busy rule

A call that may change the project waits while the person is in the middle of something: a running command,
an open dialog, a drag on the drawing, or unsent text in the command field. It is re-checked each frame; after
5 seconds it is refused unrun, with the reason ("the designer is in the middle of WIRE"). Every `execute` is
held, reads included, because starting any command would cancel the person's. The read tools answer at once.

Why: an agent must never take the drawing out from under the person's hands, and a clear refusal is better
than a change the person did not see coming.

## One writer

The first live session that asks to write, while access is Read and change, becomes the **writer**. Other
sessions are served read-only and told which agent is writing. The role is released by `ecad.bye`, by ten
minutes without a call, by turning access off, or by **Disconnect** on the banner.

Refusals for writes are checked in a fixed order: access off; Read only; the server started without
`--write`; not the writer; paused; the project read-only. Each is reported as not run, and history is
unchanged.

## The banner

While a live session is connected, a banner across the top of the drawing area says which agent is
connected, whether it is changing or only reading, its last change (a link that opens History at that step),
how many of its steps are still on the log, the last refusal, and how many other sessions are reading. Its
controls are **Undo session**, **Pause** / **Resume**, **Disconnect** and, once the session has finished,
**Dismiss**. The agent pill in the top bar hides and shows it.

The objects of the agent's last accepted change on the page shown get a dashed outline with a name chip. It is
painted over the page, never as a drawing entity, so it is never printed.

The banner is one value the app keeps up to date (`ecad-app` `agent/banner.rs`). It changes only on agent
events (hello, a call, the write flash ending, hold start and end, Pause, Disconnect, bye, the idle mark,
access off, Undo session); the step count is recounted on those and on any undo, redo or branch switch. A
person's ordinary edit changes nothing in it, and drawing it reads no project state.

## Undo: an agent undoes only its own steps

Over the bridge an agent's `u` is allowed only when the step it would take back is one this session
recorded, and `redo` only when the undo it would redo was this session's. Otherwise it is refused: "the last
step is the designer's; an agent undoes only its own steps". Headless is not limited, because only the agent
edits there.

**Undo session** on the banner is the person's action, recorded as the person's: ordinary undos, one per
step, using the usual step mapping (`u`, `undomerge`, `undoissue`, or a redo for an undo). No new op is
needed.

- When every step from the top of the log down to the session's oldest is the session's, they are taken back
  at once.
- When the person (or anyone else) recorded steps among or after them, a dialog names those steps and offers
  **Undo all**, **Undo the agent's steps after my last one** (when there are any) and **Cancel**.
- Only the branch shown is touched; steps on another branch are named, not undone.
- A step that fails to undo stops the run and says where the project now stands.

Why: the history is a single line of transactions, so "undo the agent's work" can only be a run of ordinary
undos from the top. Making the person confirm when their own steps are in the way keeps the rule simple and
honest: nothing of theirs disappears without them choosing it.

## Tests that hold this in place

Every generic command that may edit has a row in one contract table in `ecad-mcp`'s tests: with writes off it
is refused in the gate's words and records nothing; with writes on it records exactly one transaction by
`Author::Agent(<client name>)`. A new generic command fails the table until its row is added. Session
commands, and a generic command's own edges, get an MCP contract test that drives the command through
`execute`. See [`../developer/testing.md`](../developer/testing.md).
