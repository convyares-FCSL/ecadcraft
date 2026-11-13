# ECAD's control port (brief 035)

The ECAD trial listens on `127.0.0.1:<port>` only while the person has turned **agent access**
on (Developer ▸ Agent access, or `ecad-trial --control <port>` for one run). The transport is
CADCraft's own control server, unchanged (`docs/control-protocol.md` at the repository root):
one JSON object per line, one reply per line, the per-session token from
`cadcraft-control-<port>.token` on every request, and the same bounds and `not-run` /
`may-have-run` replies. `ecad-mcp --connect <port>` is the client; nothing else is supported.

## Methods

Every `ecad.*` request carries `session` (16 hex digits, chosen by the client) and the agent
`name`, so authorship never depends on the connection.

| Method | Params | Reply `result` |
|---|---|---|
| `ecad.hello` | `{session, name, write, client_version}` | `{writes, why?, project: {name, file}, writer, app_version}` |
| `ecad.execute` | `{session, name, line, command, params}` | `ecad-mcp`'s `execute` result, with `ecadTransactions` |
| `ecad.tool` | `{session, name, line, tool, arguments}` | the tool's MCP result (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`); `{protocolError}` for invalid arguments |
| `ecad.bye` | `{session}` | `{}` |

Forwarded unchanged to the window's own handler: `drawing.inspect`, `engine.commands`,
`ui.render` and `ui.screenshot` (both without `path`; the PNG comes back inline as
`pngBase64`), `ping`. `control.methods` lists this table. **Every other method** (`cmdline.*`,
`app.*`, `ui.click`, `ui.key`, `engine.execute`…) is refused without running:
`{"ok": false, "error": "not offered over ECAD's port: <why>", "state": "not-run"}`.

## Rules

- `line` counts request lines in `ecad-mcp`; one line may record at most one transaction
  (batches included), as headless.
- `ecad.execute` and `ecad.tool` from a session that never said hello: `say ecad.hello first`
  (`not-run`); the client says hello again and resends once.
- The window runs each call between two frames, as the agent (`Author::Agent(<name>)`) for that
  call only, through the same gate as headless (`ecad-mcp`'s `gate.rs`): the same refusals, the
  same cap and roll-back. The person is the author again after it, also on refusal, error or
  panic.
- A call that may change the project (`ecad.execute`, `ecad.tool` `save`) waits up to 5 s while
  the person is in the middle of a command, a dialog, a drag or typing, then is refused
  `not-run` with the reason. Reads are answered at once. Order is kept per session.
- Writes are refused (`not-run`), in this order: access off; Read only; server without
  `--write`; another session is the writer; paused by the designer; project read-only.
- One writer: the first session whose hello may write, while access is Read and change. Released
  by `ecad.bye`, 10 minutes without a call, access turned off, or Disconnect on the banner.
- An agent's `u` and `redo` act only on its own steps of the session.
- At most 8 agent requests are answered per frame.
