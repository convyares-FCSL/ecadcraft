# ECAD's control port

The ECAD desktop app listens on `127.0.0.1:<port>` only while the person has turned **agent access** on (ECAD
Dev ▸ Agent access, Read only or Read and change), or for one run with `ecad-desktop --control <port>` (Read and
change). When access is off, nothing listens. The default port is 47823; ports below 1024 are refused.

`ecad-mcp --connect <port>` is the client; nothing else is supported. See [mcp.md](mcp.md) for how an agent uses
it.

## Transport

The transport is CADCraft's own control server, unchanged (`docs/control-protocol.md` at the CADCraft repository
root): one JSON object per line, one reply per line, loopback only, the per-session token from
`cadcraft-control-<port>.token` (an owner-only file) on every request, and the same bounds and `not-run` /
`may-have-run` replies.

## Methods

Every `ecad.*` request carries `session` (16 hex digits, chosen by the client) and the agent `name`, so
authorship never depends on the connection.

| Method | Params | Reply `result` |
|---|---|---|
| `ecad.hello` | `{session, name, write, client_version}` | `{writes, why?, project: {name, file}, writer, app_version}` |
| `ecad.execute` | `{session, name, line, command, params}` | `ecad-mcp`'s `execute` result, with `ecadTransactions` (the transactions the call recorded) |
| `ecad.tool` | `{session, name, line, tool, arguments}` | the tool's MCP result (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`); `{protocolError}` for invalid arguments or an unknown tool |
| `ecad.bye` | `{session}` | `{}` |

Answered by the window's own handler: `drawing.inspect`, `engine.commands`, `ui.render` and `ui.screenshot`
(both without `path`; the PNG comes back inline as `pngBase64`), `ping` (`"pong"`), and `control.methods`, which
lists the methods above. **Every other method** (`cmdline.*`, `app.*`, `ui.click`, `ui.key`, `engine.execute`…)
is refused without running:

```json
{"ok": false, "error": "not offered over ECAD's port: <why>", "state": "not-run"}
```

## Rules

- **Say hello first.** `ecad.execute` and `ecad.tool` from a session the window does not know (it never said
  hello, or the window restarted) get `say ecad.hello first` (`not-run`); the client says hello again and resends
  once.
- **One step per line.** `line` counts request lines in `ecad-mcp`; one line may record at most one transaction
  (batches included), as headless.
- **The same gate as headless.** The window runs each call between two frames, as the agent
  (`Author::Agent(<name>)`) for that call only, through the same gate as `ecad-mcp` headless
  (`ecad_engine::agent::gate`): the same refusals, the same cap and roll-back. The person is the author again
  after it, also on refusal, error or panic, and the author is checked again at the top of every frame.
- **The person comes first.** A call that may change the project (`ecad.execute`, `ecad.tool` `save`) waits up to
  5 s while the person is in the middle of a command, a dialog, a drag or typing, then is refused `not-run` with
  the reason. Reads are answered at once. Order is kept per session.
- **Write refusals** (`not-run`), checked in this order: access off; Read only; server started without
  `--write`; another session is the writer; paused by the designer; the project opened read-only. `ecad.hello`
  reports the first that applies as `why`.
- **One writer:** the first session whose hello may write, while access is Read and change. It is released by
  `ecad.bye`, 10 minutes without a call, access turned off, or Disconnect on the agent banner.
- **An agent undoes only its own steps:** `u` and `redo` act only on the session's own steps.
- **A burst cannot freeze the window:** at most 8 agent requests are answered per frame.
