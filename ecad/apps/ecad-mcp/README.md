# ecad-mcp

Layer: Programs

## Purpose
ECAD for agents over the Model Context Protocol. It runs CADCraft's MCP server with the ECAD
commands installed and one ECAD project open, and adds a write gate and agent authorship. It is
read-only by default; with `--write`, an agent's edits go through `execute`, so they are the same
checked transactions a person gets, recorded as `Author::Agent(<client name>)`.

## Contents
- `policy`: the write gate (what is read-only, what is never offered, one step per call).
- `gate`, `bridge`: the gate on the person's own session, and the bridged mode that sends calls to a running ECAD window (`--connect PORT`).
- `tools`: the ECAD tools (`ecad_project`, `ecad_history`, `ecad_inspect`, `save`).
- `args`, `launch`: command-line options, and starting the window (`--launch`).
- `demo`, `src/bin/ecad-demo.rs`: a demo session.
- `src/lib.rs`: `EcadMcp`, `Options`, and the caps (`MAX_REQUEST_BYTES`, `MAX_BATCH`, `MAX_TRANSACTIONS_PER_CALL`).

## Must not depend on
Programs rule: no `cadcraft-engine` directly (it uses `ecad-engine` and `cadcraft-mcp`), and no
Dev crate. Left out on purpose: several agents writing at once, proposals held outside the log,
library tools, and anything beyond loopback on this machine.

## Key types
`EcadMcp`, `Options`, `policy::check`, `agent_name`.

## How to test
```bash
cargo test -p ecad-mcp
```
Tests are in `tests/all/`, one file per area; `mcp_contracts.rs` drives commands through `execute`.
Every generic command that may edit has a row in `generic_commands_contracts.rs` (refused with writes
off, one transaction by the agent with writes on); a new or changed session command gets a test of
its own. To try it by hand, see [`docs/quick-start.md`](../../docs/quick-start.md).
