# Brief 029: The HTML inspector opens `.ecad` v2 through wasm

Status: Draft (designer, 2026-11-12; design `docs/design/029.md`, owner questions §11, Q1–Q7)
Branch: `feature/inspector` (worktree `C:\dev\ecad-features\container-v2`, folder name
historical), from `dev` `dc2472c6`.
Designer: **yes** (`docs/design/029.md`) · Size: S
Roadmap: `docs/roadmap-v0.1.md`, lane C, after 026 and before the look-and-feel pass (owner,
2026-11-11). Follows brief 021, "Later": the inspector reads `.ecad` v2 directly through the wasm
reader, with a raw/readable toggle.

## Goal

Someone with only a browser can drop a `.ecad` file on one HTML page and see what is in it: the
project and pages, devices, wires and nets, history and branches, the stored tables and whether
the file verifies. No app, no server, no install. The page never changes the file.

## What exists today

- `ecad/tools/inspector/index.html` (owner decision, 2026-10-12): a static, read-only page for
  `ecad-inspect/1` and `/2` JSON snapshots exported from a running session.
- `ecad-file` builds for `wasm32-unknown-unknown` (`cargo xtask wasm`). `Store::from_bytes`
  opens a redb database held in memory, and `ruzstd` decodes rows on wasm (decision 2026-11-09).
- `ecad_file::v2::inspect` (`tables`, `branches`, `log`, `versions`, `chunk`, `export_jsonl`)
  and `v2::verify::verify` back `ecadinspect` on the command line.
- **Spike (2026-11-12, design §1):** a throwaway cdylib over `ecad-file` built for
  `wasm32-unknown-unknown`, ran in Node and opened the owner's `demo.v2.ecad` (323 KB) from a
  byte buffer: tables, branches, versions, Verify (`everything verifies`) and a full open (90
  transactions, 3 pages, 64 items) in 121 ms. On the large bench file (67 MB, 33 998
  transactions) the raw views took 69 ms and the full open 0.96 s, using 156 MiB. Full Verify
  did not finish in 25 minutes, even natively (design §1.3). The module is 1.97 MB, 570 KB
  gzipped.

## Scope

1. **A wasm entry crate**, `crates/ecad-inspector` (cdylib and rlib). Safe Rust over `ecad-file`
   and `ecad-model` only, never a `cadcraft-*` crate. One function, `request(json) -> json`,
   is tested natively. The few lines that export it to the page sit behind
   `cfg(target_arch = "wasm32")`. How they are exported is owner question Q1.
2. **The page.** `tools/inspector/index.html` stays the one page. It gains a `.ecad` drop
   target next to the snapshot loader. `cargo xtask inspector` writes one self-contained file,
   `target/inspector/ecad-inspector.html`, with the module gzipped and inlined. It opens from
   `file://` by double-click (design §3).
3. **Views** (design §4): Project and pages; Devices (tags, locations, connection points);
   Wires and nets; History (branches, transactions, versions and revisions); Tables (the raw
   rows, with a raw/readable toggle on each); Checks (the file's integrity: open report, a
   linear **Check chain** on load, model invariants, and full Verify on request with **Stop**).
   `verify_chain` is split out of `v2::verify::verify` (code, not format; design Q7).
4. **Mismatches** (design §5): every file the page cannot read fully gets a message in words
   and a way forward. The page always shows as much as it can: if the full open fails, the
   tables still show.
5. **Size budget and packaging** (design §6): the built page is at most 1.5 MB, and
   `cargo xtask inspector` fails above it. Nothing built is committed. CI is not changed in this
   brief.
6. **Quick start**: rewrite "Inspector snapshots" as "The inspector", covering both kinds of
   file.

Out of scope: drawing the pages (no geometry rendering), the drawing Checks tab's rules (022,
025, 026; they live in `ecad-cadcraft`), editing, a server or live connection, `.ecadlib`, and
version 1 files beyond saying how to upgrade them (Q4).

## Format and ops

None. The page reads what brief 021 writes and changes nothing. No FP items.

## Acceptance criteria

1. `cargo xtask inspector` builds `target/inspector/ecad-inspector.html` on Windows and Linux,
   prints its size, and fails if the size is over 1.5 MB.
2. Double-clicking the built page and dropping `demo.v2.ecad` shows all six views in under
   1 s. Dropping the large bench project (design §1) shows Project in under 3 s, and Check
   chain passes in under 5 s. Full Verify can be stopped, and the views stay.
3. Every row of the mismatch table (design §5) gives its message and way forward, tested
   natively and table-driven (`tests/all/inspector_contracts.rs`). No input panics, including
   every truncation of a small file and a flipped byte in each table.
4. A Node smoke check (`cargo xtask inspector --check`, skipped with a message when `node` is
   absent) loads the built module and opens three fixtures through it: the demo, a damaged
   file and a newer-version file.
5. Every value the page shows is escaped. A fixture whose tag and page name hold `<img src=x
   onerror=alert(1)>` shows the text and runs nothing (the tester checks this in a browser).
6. Opening a file never writes it. The page has no write path, and the Rust side gets a copy of
   the bytes.
7. `cargo xtask wasm` and `clippy -D warnings` stay clean. The spike's dead-code warning on
   wasm (`store.rs`, `Db::ReadOnly`) is fixed.
8. Quick start "The inspector" is updated, with agent-observed and owner-tried steps kept apart.

## Needs owner check

- Answer design §11 (Q1–Q7).
- A click-only trial: double-click the built page, drop `demo.v2.ecad`, open every view, and
  drop a `.ecad.lock` file and a v1 `.ecad` to see the messages.
