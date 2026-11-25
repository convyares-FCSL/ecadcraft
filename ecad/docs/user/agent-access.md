# Agent access

An AI agent, such as Claude Code, can read and change an ECAD project through `ecad-mcp`, ECAD's
Model Context Protocol server. It works either **headless** (on a project file, with no window) or
**live** (driving the ECAD window while you watch). Either way it goes through the same commands,
checks and refusals as you, and every change it makes is recorded with its name as the author. How
to set up the server, its tools and the protocol are in [../agents/mcp.md](../agents/mcp.md).

## What an agent may do

- **Read**, always: the project's pages, devices, wires, cables, nets and history, and every
  command that only reads (lists, reports, checks, a printed book returned as data).
- **Change**, only when writes are on: one command per call, as you would type it. Each change is
  one step in the history by `agent <name>` and one step of undo.
- **Never**: open, create, close or Save As a project, read or write other files, use your mouse,
  keyboard or command line, or change agent access itself. A parameter that names a file is
  refused.

An agent's `u` and redo take back only its own steps. One agent changes the project at a time;
others can read.

## Writes on or off

**Headless**: writes are off unless the server is started with `--write`, and off for a project
that opened read-only (for example, one open in another window). With writes off, every change and
`save` is refused and nothing is recorded.

**Live**: the ECAD window decides, in **Settings ▸ Agent access** (project menu ▸ Settings ▸ Agent
access):

- **Off** (the default): nothing listens.
- **Read only**: an agent may read but not change or save.
- **Read and change**: an agent may read and change the project.

The port is 47823 unless you change it, and only the computer itself can connect (`127.0.0.1`). The
status line says what listens (`● Listening on 127.0.0.1:47823`). The choice is remembered for you in
`agent-access.json` in your ECAD settings folder, never in a project.
`ecad-desktop --control <port>` turns on Read and change for one run without remembering it.

## While you work

An agent never interrupts you. A change it asks for while you are in the middle of a command, a
dialog, a drag or typing waits up to 5 seconds for you to finish, then is refused with nothing
changed (`the designer is in the middle of WIRE`); the agent can try again.

## The agent banner

While an agent is connected live, a banner runs under the top bar in the colour of its state:

- the agent's name and `connected since 14:02`;
- its state: `changing`, `waiting for you to finish WIRE`, `paused — agent changes are refused`,
  `reading only: …`, `finished · 3 steps` or `agent access stopped: …`;
- its **last write** (`last: #57 Wire W12 · 14:02:11`): click it to open History at that change;
- its last refusal, and how many other agents are reading;
- **Undo session (n)**, **Pause writes** (then **Resume**), **Disconnect**, and **Dismiss** once
  it has finished.

The top bar's agent icon names the agent and takes the state's colour. Click it to hide the banner,
and again to show it. A new agent session shows its banner again. With no agent connected, the icon
says `Agent off` or `Agent ready`.

- **Pause writes** refuses the agent's changes until **Resume**; it can still read.
- **Disconnect** ends this agent's session; restart its MCP server to connect again.

## Its last write on the page

The objects of the agent's last change on the page shown get a dashed green outline with the
agent's name on a chip, until its next write or until you dismiss the banner; your own edits in
between leave it. The outline is not part of the drawing: it is never printed, and it is not shown
over Library or Publish.

In History, every step says who made it: **You**, or the agent's name. An agent's run of steps
folds into one row; see [history.md](history.md). The History panel also shows a card for the
connected agent: who is writing on which branch, with Pause or Resume and disconnect. An agent
writes on the branch shown.

## Undo an agent session

**Undo session** (on the banner, or on the agent's row in History while it is still connected)
takes back the agent's steps in one click. The undos are recorded as yours.

- When only the agent's steps lie on top, they are taken back at once:
  `Undid 3 agent steps (#55–#57).`
- When you made steps among or after the agent's, the dialog **Undo agent session** names them and
  offers **Undo all** (yours too), **Undo the agent's steps after my last one**, and **Cancel**.
- Only the branch shown is undone; agent steps on another branch are named, not touched.
- Once the session has ended, its steps undo one by one like any others.

## Limits

- One agent changes a project at a time.
- Agent access is loopback only: no other computer can connect.
- `ecad-mcp --launch` opens the ECAD window on Windows and on Linux with a display (under WSL that
  needs WSLg), not on macOS.
- There is no ask-first mode and no chat: an agent's change is applied, and you take it back with
  undo or Undo session.
