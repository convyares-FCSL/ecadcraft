# ecad-mcp: ECAD for agents

`ecad-mcp` lets an AI agent such as Claude Code read and change an ECAD project through the Model Context Protocol
(JSON-RPC on stdin and stdout; logs go to stderr). It runs CADCraft's MCP server with the ECAD commands installed
and one ECAD project open, and adds a write gate and agent authorship. An agent's edits are the same checked
transactions a person makes, recorded as the agent's.

It runs in one of two ways:

- **Headless:** no window. The agent works on a project file in a process of its own.
- **Live** (`--connect <port>`): the agent drives the running ECAD window while the person watches. The server is
  headless until the window answers on the port, and bridged once it does.

What the person sees and controls (the agent access setting, the banner, Undo session) is in
[../user/agent-access.md](../user/agent-access.md). The wire protocol of the live mode is in
[control-protocol.md](control-protocol.md).

## Run it

Build once, from `ecad/`: `cargo build --release -p ecad-mcp` (add `-p ecad-desktop` for the live mode). The
program is `target/release/ecad-mcp` (`ecad-mcp.exe` on Windows).

```
ecad-mcp [--write] [--agent <name>] [--connect <port>] [--launch] [--create] [<project.ecad>]
```

| Option | Does |
| --- | --- |
| `<project.ecad>` | the project to open headless; without one, a new project in memory that cannot be saved |
| `--write` | let the agent change the project through ECAD commands and save it; without it the server is read-only |
| `--create` | create `<project.ecad>` (it must not exist yet, and its folder must) and save it before serving |
| `--agent <name>` | record the agent's changes under this name instead of the client's own |
| `--connect <port>` | live: drive the ECAD window listening on `127.0.0.1:<port>` (1024–65535) |
| `--launch` | with `--connect`: open the ECAD window when nothing answers |
| `--help`, `--version` | usage; the version and the CADCraft commit |

Register it with Claude Code, headless on one file:

```sh
claude mcp add ecad -- <path to ecad-mcp> --write <project.ecad>
```

or live, for every folder, with agent access turned on in the window (default port 47823):

```sh
claude mcp add --scope user ecad -- <path to ecad-mcp> --connect 47823 --write
```

The same can go in a project's `.mcp.json`:
`{"mcpServers": {"ecad": {"command": "<path to ecad-mcp>", "args": ["--write", "<project.ecad>"]}}}`.

A headless server holds the project file's writer lock, so the app opens that file read-only meanwhile. A live
server opens its headless project only at the first headless call, and closes the file after every request until
headless has recorded something, so it does not lock a file the person is about to open in the window. Once
bridged, it stays bridged; if the window closes, calls answer "The ECAD window at 127.0.0.1:<port> is not
answering (closed?)…" until it is back.

## Tools

| Tool | Writes | What it does |
| --- | --- | --- |
| `execute` | with `--write` | runs one ECAD command with JSON parameters, through the gate |
| `list_commands` | no | the commands the gate offers, with their parameters, each marked `"agent": "read"` or `"write"`; `filter` narrows it; `all: true` lists every command, the refused ones marked `"agent": "refused"` and `"enabled": false` with the reason (see [commands.md](commands.md)). The `cadcraft://commands` resource is the offered list too |
| `inspect_drawing`, `query_entities`, `find_text`, `render` | no | CADCraft's drawing reads; `render` returns the image and refuses a `path` |
| `ecad_project` | no | name, file, whether writes are on, head and unsaved changes, pages, devices with their connection points, wires with their ends and cable, cables with their members, and nets as the app, wire numbers and checks see them (joined by wire ends, definition bridges, jumper bars and continuations of one name; each with its connection points, wires, continuation names and wire numbers); each list paged by `limit` (default 500) |
| `ecad_history` | no | the transactions in order: seq, id, label, kind, author (`user` or `agent <name>`), time, op count; paged by `offset` and `limit` (default the last 100) |
| `ecad_inspect` | no | `ecadinspect`'s read-only JSON snapshot of the project, live or at transaction `seq` |
| `describe_command` | no | one offered command (`id`): its parameters text (`help`), the JSON Schema of its parameters (`params`) and of its reply (`reply`; null while that command still answers untyped JSON), and `agent` (`read` or `write`) |
| `save` | with `--write` | saves the project to its own file; a `path` is refused |
| `ecad_connection` | no | where calls go: `mode` (`headless` or `bridged`), the port, the window's project and whether it lets this agent write, the agent name, the session and whether it is the window's writer |
| `screenshot` | no | a PNG of the ECAD window; listed by a `--connect` server, answered only when bridged (headless, use `render`) |

CADCraft's `command_line`, `script`, `cancel`, `new_drawing` and `open` are not offered: typed lines could reach
file commands and several commands per call. Asking for one gives "unknown tool `…`" with the reason.

## The gate

`ecad_engine::agent::policy` decides what `execute` may run; `ecad_engine::agent::gate` runs it. The same code
serves headless and live, so the same project state gives the same reply text in both.

- **An allowlist:** an agent runs ECAD's own commands and a short, reviewed list of CADCraft drawing commands
  (`ecad_engine::agent::upstream`), each marked read or write. Every other command is refused in every mode
  (`Refused: <COMMAND> is not available to agents. …`). The CADCraft commands offered:
  - read: `entities`, `texts`, `shapes`, `usage`, `loops`, `drawing.inspect`, `getvar`, `document.switch`;
  - write: `line`, `pline`, `circle`, `arc`, `rectang`, `text`, `mtext`, `textedit`, `move`, `copy`, `rotate`,
    `mirror`, `scale`, `stretch`, `erase`, `oops`, `explode`, `join`, `trim`, `extend`, `-array`, `u`, `undo`,
    `redo`, `mredo`.
  A test fails when an upstream sync adds a command nobody has classified (offered, never, or not offered).
- **Never** (`NEVER_COMMANDS`): opening, creating, closing and saving under another name, import, export, plot,
  publish, scripts, LISP and APPLOAD, AUDIT, SETVAR, external references, inserting blocks, images, hatch patterns
  and line types from files, block libraries, and `agentaccess` (an agent never turns its own access on, widens it
  or moves the port). Every command CADCraft lists as touching files (`file_access::commands`) is here.
- **Writes off** (no `--write`, or the project opened read-only): only commands that record nothing run. A
  generic command says so in its declaration (`Access`); ECAD session commands that only read are listed
  (`READ_COMMANDS`: `openpage`, `cablegoto`, `refgoto`, `history`, `branches`, `printbook`, `prerelease`, …), and
  the CADCraft ones are marked read. Some commands only read in one form: `projectprops {}`, `pageprops {page}`,
  `numbering {}`, `textstyle {}`, `merge`/`renumber`/`finalise` with `preview: true`, `release {rev}` for an
  issued revision.
- **Writes on:** an offered command runs exactly as for a person, and ECAD refuses or records it.
- **Files:** behind the allowlist, a parameter that names a file (`path`, `file`, `patternFile`, `template`,
  `folder`, `output`, or a file key a command declares) is refused in every mode, except where the name is not a
  file (a page folder). `printbook` returns its PDF as base64 data instead.
- **One step per call:** one `execute` runs one command, which records at most one transaction. A request line
  (a JSON-RPC batch counts as one line) may record at most one; a call that records more is taken back and
  refused. `undo`, `redo` and `mredo` go one step per call (no count above 1, no `undo` back mode, no `mredo`
  all).
- **Caps:** a request line is at most 4 MiB (`MAX_REQUEST_BYTES`), a batch at most 64 messages (`MAX_BATCH`).
- **Live only:** an agent's `u` and `redo` act only on its own session's steps; writes wait for the person (see
  [control-protocol.md](control-protocol.md)).

## Authorship

Every transaction an agent makes (edits, undos and redos) is recorded with `Author::Agent(<name>)`, stored in the
file as `{"agent": "<name>"}`, and is one step of undo. The name comes from the client's first `initialize`
(later ones are ignored), or from `--agent`. Hidden characters (control, bidirectional, zero-width) are dropped,
spaces trimmed, and the name cut to 64 characters, so it cannot pose as another; an empty name is recorded as
`unnamed MCP client`. History, the debugger and `ecad_history` show it as `agent <name>`. Live, the agent is the
author for its call only; the person is the author again after it, also when the call is refused, fails or panics.

## Refusals

Every refusal says what happened and that nothing changed, in words an agent can act on:

| When | Reply |
| --- | --- |
| A change without `--write` | `Refused: this ECAD MCP server is read-only (started without --write). Nothing was changed. Restart it with --write to let the agent edit and save the project.` |
| A change to a project that opened read-only | `Refused: the project opened read-only (<why>). Nothing was changed.` |
| A command agents never run | `Refused: <COMMAND> is not available to agents. Opening, creating, closing and saving projects under another name, and reading or writing other files, stay with the person who started the server; …` |
| A command not offered to agents | `Refused: <COMMAND> is not available to agents. Agents run ECAD's commands and a short list of drawing commands; list_commands shows them, each marked read or write. Nothing was changed.` |
| `agentaccess` | `Refused: Agent access is set by the person in the ECAD window. Nothing was changed.` |
| A file parameter | ``Refused: `<key>` names a file; agents cannot read or write files through <COMMAND> …`` |
| Several undo steps | `Refused: UNDO takes one step per call here …` |
| The project is busy | `Refused: the project could not be checked just now (it is busy). Nothing was changed; try again.` |

Live, the window adds its own: Read only, paused by the designer, another agent is the writer, the designer is in
the middle of a command (after a 5 s wait), and an agent undoing a step that is not its own.

## Every command arrives with its agent access

- Every generic command that may edit has a row in `ecad/apps/ecad-mcp/tests/all/generic_commands_contracts.rs`:
  parameters that change a small project; with writes off it is refused in the gate's words and records nothing,
  with writes on it records exactly one transaction by `Author::Agent(<client name>)`. A new generic command fails
  `every_generic_edit_command_has_a_row` until its row is added.
- A new or changed session command, or a generic command's own edge (a form that only reads, a refusal in its own
  words), gets an MCP contract test that drives it through `execute` (as `mcp_contracts.rs` does) and checks the
  transaction, its author, and the refusal with writes off.

## Not offered

Several agents changing one project at once, proposals held outside the history until a person accepts them,
library tools, and anything beyond loopback on this machine.
