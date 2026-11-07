# Brief 011: Agent access through MCP, built in from now on

Status: Approved (owner, 2026-11-07)
Branch: `ecad/agent-mcp`
Designer: no (no visible UI beyond a banner indicator) · Size: M
Depends on: 005 (branch from `ecad/cable-views` until PR #24 is merged)

## Goal

An AI agent can drive ECAD in a controlled way, as Fusion and CADCraft allow, and every feature from
now on arrives with its agent access instead of it being added at the end (owner, 2026-11-07).

## What already exists

- CADCraft ships an MCP server over stdio (`crates/mcp`, `docs/mcp.md`): headless
  (`cadcraft-cli mcp`) or bridged to a running app through its control channel
  (`--control PORT`, `--connect HOST:PORT`). Tools: `command_line`, `script`, `execute`,
  `list_commands`, `inspect_drawing`, `query_entities`, `render`, `screenshot`, and file tools.
- ECAD commands are `CommandSpec`s added through the extension interface, so `list_commands` and
  `execute` already reach their JSON forms (proposal Path A: `mcp/src/backend.rs:203`). JSON forms
  never prompt (brief 008), and the view never changes what commands do (design 005 rule 5).
- The ledger records an author on every transaction: `Author::User` or `Author::Agent(String)`
  (`ecad-model/src/ledger.rs:12`, FILE-07). So agent authorship needs no format change.
- The proposal's plan (§19): MCP makes the same transactions with an agent as author; proposals are
  transactions held outside the log until accepted; release is human-only.
- The ECAD trial excludes the control channel today (`apps/ecad-trial/src/lib.rs:29`).

## Scope

1. **ECAD over MCP, headless:** an `ecad-mcp` entry point (stdio) that opens or creates a `.ecad`
   project in-process and serves CADCraft's tools plus the ECAD tools below. Read-only by default;
   writes only with an explicit `--write` flag.
2. **Bridged to the running trial: deferred to a later brief (owner, 2026-11-07).** Later, the trial gains an opt-in control channel
   (`--control PORT`, localhost only, with CADCraft's control token), so the owner can watch an
   agent work. A banner shows when an agent is connected and when it last wrote.
3. **ECAD tools** (read): inspect the project (pages, components, terminals, conductors, cables
   and members, nets via `derive::nets`), the history with authors, and `ecadinspect`'s output.
   Write: ECAD commands through `execute` only, so every refusal, check and transaction is the same
   as for a person.
4. **Control:**
   - every agent transaction is recorded with `Author::Agent(<client name>)`, is one step of undo,
     and is visible in the debugger and history;
   - refused when the project is read-only, when the write flag is off, or when the agent targets
     anything that bypasses an ECAD command (raw CADCraft entity edits on ECAD pages are already
     refused by `refuse_start`; MCP tools must not get round it);
   - an agent may save (owner, 2026-11-07), only with `--write`; save-as and close stay explicit;
   - caps on request size and on transactions per call.
5. **Built in from now on (process change, needs the owner's approval):** add to `ecad/CLAUDE.md`
   that every brief with a new or changed ECAD command includes, in its acceptance criteria, an MCP
   contract test that drives the command through `execute` and checks the transaction and its
   author. The tester covers it like any other criterion.
6. Quick start: a section on connecting Claude Code to ECAD, headless and bridged.

**Stop and report (owner gate):** any change outside `ecad/` (for example in `crates/mcp`); it would
be an upstream patch with an `ECAD-SEAM` marker. Prefer composing CADCraft's server from ECAD's
side over patching it.

**Out of scope (later briefs):**
- Proposals held outside the log until a person accepts them (proposal §19): the next step once
  this works.
- Remote or networked access; anything beyond localhost.
- Library (`.ecadlib`) tools, until stage 3 is approved.

## Acceptance criteria

1. An MCP client lists the ECAD commands and their parameter docs, and the ECAD read tools.
2. Driving the brief 004 and 005 examples through MCP gives the same transactions as the JSON
   forms, each with `Author::Agent(...)`; `u` undoes each in one step.
3. With writes off, every write tool is refused with a clear message and the head is unchanged.
4. An agent cannot edit an ECAD page through raw CADCraft commands; the refusal matches the UI's.
5. Save through MCP works only with `--write`, and reopening gives the agent's edits with their
   author.
6. Hostile input (malformed JSON-RPC, huge payloads, unknown tools) never panics and gives
   JSON-RPC errors as `docs/mcp.md` describes.
7. Works on Windows and Linux; CI covers the headless server.

## Needs owner check

Connecting Claude Code to the running trial on Windows and watching it build the quick-start
cable example.

## Owner answers (2026-11-07)

1. Headless (windowless) only in this brief; bridged to the running trial later.
2. The process rule in scope item 5 is approved: every new or changed ECAD command ships with an MCP
   contract test.
3. An agent may save.
