# Brief 021: container v2 redb

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 021 (container v2 on redb, EEP-0005), on `feature/container-v2` (built on
`dev`, brief 022 included), not yet in `dev`:** a `.ecad` file is now one **redb** database. Saving writes
one transaction in a few milliseconds and opening reads the latest snapshot and the changes after
it, so both stay fast as the history grows. A version 1 file is **offered** for conversion when
you open it (a new `<name>.v2.ecad`, the original untouched), or converted with `ecad upgrade`.
`ecadinspect` shows a version 2 file's tables, branches, log, versions and chunks, and exports the
history as version 1 JSON Lines. The debugger gains **Storage** and **Branches** views. Sections
7, 7B, 7C, 8 and "Measuring load time" describe it.
- *Automated checks:* `ecad/crates/ecad-file/tests/all/container_v2.rs` (reopen, saves and
  snapshots, versions and revisions, the append-only API, damage),
  `ecad/crates/ecad-file/tests/all/container_crash.rs` (power loss, torn writes and process kills,
  in the slow set), `ecad/crates/ecad-model/tests/all/ledger_base.rs` (a project opened from a
  snapshot undoes and rebuilds exactly as a replayed one), `ecad/apps/ecad-cli/tests/all/cli_contracts.rs`
  (`ecad upgrade`), `ecad/dev/ecad-inspector/tests/all/cli_contracts.rs` (every `ecadinspect`
  view, `export-jsonl` read back by version 1; moved there by brief 029), and the
  debugger checks in `ecad/crates/ecad-cadcraft/tests/all/debugger.rs` and
  `ecad/apps/ecad-trial/tests/all/debugger.rs`.
- *Agent-observed:* the worker ran the crash suite and the benchmark; no agent has clicked through
  the live window.
- *Owner-tried:* not yet.
