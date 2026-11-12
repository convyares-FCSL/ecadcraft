# Brief 035: Live MCP bridge

Status: Draft
Branch: `feature/mcp-bridge`, from `origin/dev` `1e605e33`.
Designer: **yes** (`docs/design/035.md`) · Size: M
Roadmap: M1 item **1.0**, lane U, first (`docs/discussions/roadmap-review-2026-11.md` §3.2,
"1.0 Live MCP bridge, in detail"; owner questions 17 and 18, decided as recommended). It edits the
trial's frame before 1.5 (UI shell) does, so it runs first in lane U.

## Goal

An agent drives the **running** ECAD trial and the owner watches the drawing appear, as SolidCraft
does. One registered MCP server: headless when no app is open, bridged when one is. Every agent
change is recorded as the agent's, every person's change as the person's, per call.

## What exists today (`dev` `1e605e33`)

- `ecad-mcp` (brief 011) is headless only: one in-process CADCraft session, ECAD's write gate
  (`--write`, `policy.rs`), `Author::Agent(<client>)` set once for the whole process, the caps
  (`MAX_REQUEST_BYTES`, `MAX_BATCH`, `MAX_TRANSACTIONS_PER_CALL` with roll-back), the ECAD tools
  (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`) and six of CADCraft's tools. Its crate
  doc excludes "the bridged mode" (`apps/ecad-mcp/src/lib.rs:20`).
- The trial embeds CADCraft's `CadApp` and does not open a control channel
  (`apps/ecad-trial/src/lib.rs:37`); `CadApp::with_control(rx)` and `drain_control` exist upstream
  but are unused.
- Upstream, used as is: the loopback JSON-lines control server
  (`apps/cadcraft/src/control_server.rs`, bounded, token-checked), the per-session token
  (`crates/mcp/src/control_token.rs`), the `Remote` backend with reply caps, reconnect and
  `not-run`/`may-have-run` semantics, and `Remote::lazy`. Upstream's launch opens the app on macOS
  only.
- `deferred-work.md` "Bridged agent access": authorship per call, restored also when a call fails
  or panics.

## Scope

1. **Agent access preference** in the trial: Off (default), Read only, or Read and change, and a
   port (default 47823), saved once in the user's ECAD settings folder. `--control PORT` turns it
   on for one run. When off, nothing listens.
2. **ECAD's own tool set over the port**: the trial answers `ecad.*` methods (hello, execute,
   tool, bye) and forwards a short allowlist of CADCraft read methods (inspect, commands, render,
   screenshot, ping). Every other method, `cmdline.input` and `cmdline.script` included, is refused
   without running.
3. **The app-side write gate**: 011's gate and caps, moved into a library both sides share, run in
   the trial. `Author::Agent(<name>)` is set **per call** and restored to `Author::User` after it,
   also on refusal, error or panic, and checked again at the top of every frame.
4. **Calls on the UI thread between frames**: an agent call waits (up to 5 s) while the person is
   in the middle of a command, a dialog or a drag, and is then refused with the reason and
   `state: not-run`. It never interleaves with a person's half-finished command.
5. **Agent banner**: who is connected, writes on or off, the last change (label, number, time),
   **Undo session**, **Pause**. A data contract and element names that the UI redesign can place.
6. **Agent undo rules**: an agent's `u` and `redo` act only on its own steps of this session.
7. **`ecad-mcp --connect PORT [--launch]`**: one server that is headless until an app answers on
   the port and bridged once one does; `--launch` opens the trial on Windows and Linux; a new
   `ecad_connection` tool says which mode is in use.
8. **Registration** with `claude mcp add --scope user` for Windows and Linux in the quick start
   (new section 7B).
9. **MCP contract tests over the bridge**, matching brief 011's, and the trial's banner by element
   name.

## Later (not designed now)

- Several agents changing one project at once (035 allows one writer, others read).
- Proposals held outside the log (an agent's changes as a reviewable branch): brief 020's branches
  are the likely route.
- Agent access to a project in another window or machine; anything beyond loopback.
- A settings window: the preference is set from ECAD Dev until the UI shell (1.5) gives it a home.

## Format and ops

**None.** Authorship is already recorded per transaction (FILE-07). No op, no record field, no
property. "Undo session" records ordinary undo transactions by the person. The preference is a
per-user settings file (`agent-access.json`), never in a project. **FP items: none.**

## Acceptance criteria

1. With no preference and no `--control`, nothing listens on any port (connect is refused).
2. With the preference on Read and change, `claude mcp add` as in quick start 7B, and the trial
   open, *"draw three wires"* makes three wires appear on the open page; each is one transaction by
   `agent <client name>`; the banner shows the last one.
3. A person's edit between two agent calls is recorded as `user`; an agent call that fails, is
   refused, or panics (fault injection) leaves the next person's edit recorded as `user`.
4. A call that records more than its cap is rolled back and refused, as in brief 011, over the
   bridge; a JSON-RPC batch is one call.
5. While the person is in the middle of `wire`, an agent `execute` waits, then is refused with
   *"the designer is in the middle of WIRE"* and `not-run`; reads are answered at once.
6. `cmdline.input`, `cmdline.script`, `app.open`, `app.save`, `ui.click`, `ui.key` and `agentaccess`
   are refused over the port and change nothing.
7. Read only (preference) or no `--write` (server): every change and `save` is refused, history
   unchanged; reads work.
8. An agent `u` whose step is the person's is refused; its own step is undone.
9. **Undo session** takes back the session's agent steps; if the person's steps are on top, it asks
   first and names them.
10. **Pause** refuses agent writes with *"paused by the designer"* until resumed.
11. Mode: no app → headless; the app opened before the headless session records anything → the
    next call is bridged; after a headless change, the server stays headless. `ecad_connection`
    tells which.
12. `--launch` opens the trial with the port on Windows and Linux (Linux with a display), and its
    output never reaches the MCP stream.
13. The upstream token rule holds: a request without the token is refused.
14. Contract tests of design §12 pass on Windows and Linux CI; quick start 7B runs by hand.

## Needs owner check

- Design §15 questions.
- Quick start 7B on Windows with Claude Code (the tester cannot run Claude Code itself).
- The banner's look against the UI redesign spec when it is shared.
