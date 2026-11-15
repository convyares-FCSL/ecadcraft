# 061: Which standard crates ECAD adopts in 0.2.3 and 0.2.4

Status: Draft, for the owner's decision. Docs only; no code or `Cargo.toml` changes come with it.
Author: designer. Date: 2026-11-15. Lanes (R0 to R10) are those of `_tmp/code-review-2026-11.md`.

**Goal.** The refactor lanes need to know, before they start, which standard crates they may use and
where each goes, so no worker picks a dependency one lane at a time.

## 1. How the facts were gathered

- Versions, licences and who uses what: `cargo metadata --locked --offline` on `ecad/Cargo.lock`
  (no build). For crates not in the lock: `cargo info` (crates.io metadata, latest versions today).
- Licence allowlist (`AGENTS.md`, "Licence allowlist"): ISC, MIT, BSD-2-Clause, BSD-3-Clause,
  Apache-2.0, OFL-1.1, Ubuntu-font-1.0, Bitstream-Vera, CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0,
  LicenseRef-Public-Domain. An `A OR B` expression is allowed when every identifier is on the list,
  so `MIT OR Apache-2.0` passes. **Zlib is not on the list.**
- wasm: `cargo xtask wasm` builds `ecad-model`, `ecad-store`, `ecad-file`, `ecad-inspect`,
  `ecad-cadcraft`, `ecad-tokens` and `ecad-ui` for `wasm32-unknown-unknown` with warnings as errors
  (`xtask/src/main.rs:153`). The apps, `ecad-bench` and `xtask` are native only.
- Binary size and compile time are **estimates from the crate's dependency tree, not measurements**.
  The adopting lane measures (`cargo build --timings`, release size of `ecad-trial`) before merging
  and reports the numbers in its PR.
- Where a crate was only checked from crates.io, "not in the lock" means the lane must run
  `cargo tree` after adding it and check every new transitive licence.

## 2. Findings that change the picture

1. **`thiserror` is not unused.** `ecad-model` and `ecad-file` already depend on it
   (`thiserror = "2"` in the workspace table; 2.0.21 in the lock). CADCraft's root also says `"2"`, so
   there is one version. The lock also carries 1.0.69, but only for `calloop`, `ndk` and
   `smithay-client-toolkit`; ECAD never touches it.
2. **`slotmap` is under Zlib, which is not on the allowlist.** It also does not fit (section 4).
3. **ECAD already has a rayon-shaped seam.** `par_map` at `crates/ecad-file/src/v2/chunks.rs:459`
   (hand-rolled `std::thread::scope`, order kept, one thread on wasm) and the two-thread block at
   `crates/ecad-model/src/index.rs:126`. Both gate wasm with `cfg!(target_arch = "wasm32")`.
4. **CADCraft logs with `log`, not `tracing`.** `cadcraft-mcp` depends on `log` 0.4.34. `tracing` is in
   the lock only through winit, zbus and calloop.
5. **`rmcp` needs tokio**, and the MCP server ECAD uses is CADCraft's hand-written JSON-RPC
   (`../crates/mcp/src/server.rs`), outside `ecad/`.
6. **Hit-testing already uses `rstar`, in the engine.** `pick_object` (`../crates/engine/src/session/picking.rs:159`)
   goes through the engine's spatial index (`../crates/engine/src/spatial.rs`, `rstar` 0.13 with default
   features off). ECAD's own scans are page-sized.

## 3. Summary table

| Crate | Licence | Lock today | wasm | Verdict | Lane |
|---|---|---|---|---|---|
| thiserror | MIT OR Apache-2.0 | 2.0.21, direct in model and file; matches CADCraft | yes | **adopt now** (extend) | R1 |
| anyhow | MIT OR Apache-2.0 | 1.0.104, only via `acadrust` | n/a (apps only) | **adopt now**, edges only | R1 |
| miette | Apache-2.0 | no | derive yes; `fancy` native only | **adopt later**, narrow | R1 follow-up |
| slotmap | **Zlib (not allowed)** | 1.1.1, only via `glow` | yes | **no** | none |
| rstar | MIT OR Apache-2.0 | 0.13.0 via `cadcraft-engine`; matches | yes | **adopt later**, on evidence | none now |
| petgraph | MIT OR Apache-2.0 | 0.8.3 via `tree_magic_mini` | yes | **no** (for now) | none |
| kurbo | Apache-2.0 OR MIT | 0.13.1 via `peniko`, `write-fonts` | yes | **no** | none |
| tokio | MIT | 1.53.2 (`rt`, `time`) via `wasm-bindgen-futures` | partial | **no** | none |
| rayon | MIT OR Apache-2.0 | 1.12.0 via `cadcraft-render`; CADCraft says `"1"` | **native only** | **adopt now**, native-only | new lane R11 |
| parking_lot | MIT OR Apache-2.0 | 0.12.5 via eframe, wgpu | yes | **no** | none |
| gix | MIT OR Apache-2.0 | no | no | **no** | none |
| rmcp | Apache-2.0 | no | no | **no** | none |
| svg | Apache-2.0 OR MIT | no | yes | **adopt later**, if SVG export is briefed | future export brief |
| usvg, svg2pdf | Apache-2.0 OR MIT; MIT OR Apache-2.0 | no | yes | **no** | none |
| tracing | MIT | 0.1.44 via winit, zbus | yes | **no** (use `log`); revisit with R9 | R9 |
| insta | Apache-2.0 | no | dev only | **adopt later**, dev-dependency | R2 follow-up |
| schemars | MIT | no | yes | **adopt now** | R6 |

## 4. Crate by crate

### thiserror. Adopt now.
- **Licence / lock.** MIT OR Apache-2.0; 2.0.21, workspace dependency; same major as CADCraft.
- **wasm.** Proc macro plus `std`; fine. **Cost.** Already compiled; zero added.
- **Replaces.** The 475 `Result<_, String>` (code-review table: ecad-cadcraft 255, ecad-bench 74,
  ecad-trial 53, ecad-mcp 44, ecad-file 39, ecad-model 8). Concretely: `ecad-file` v2 (49
  `Err(format!(..))`) onto its existing enums, e.g. `v2/chunks.rs:416` `components_of`,
  `v2/project.rs:333` `nearest_snapshot`; a `CmdError` in `ecad-cadcraft` (module by module);
  `ecad-model`'s public parsers; the string wrapper `try_lock` at `ecad-cadcraft/src/access.rs:56`.
- **Where.** `ecad-model`, `ecad-file`, `ecad-cadcraft` (add the dependency), `ecad-store`
  (add if it grows an error type). **Not** in `ecad-ui`, `ecad-tokens`, `ecad-inspect`: they have
  nothing to report. Per the owner decision, one error enum per library crate, no shared error crate.
- **Rule for lanes.** Messages stay word for word where tests or docs quote them; a `#[error("..")]`
  carries the same text the `format!` had, so R1 is behaviour-neutral and tests do not change.

### anyhow. Adopt now, edges only.
- **Licence / lock.** MIT OR Apache-2.0; 1.0.104, today only through `acadrust`. CADCraft's root
  does not list it, so ECAD would add the first direct dependency.
- **wasm.** Not needed: apps, CLI and xtask are native.
- **Replaces.** `Result<(), String>` plus `map_err(|e| format!(..))` at the edges:
  `xtask/src/main.rs:162` (`run`), `xtask` throughout (`wasm()` at `:573`), the `main` functions of
  `apps/ecad-cli`, `apps/ecad-mcp/src/main.rs` (10 `eprintln!` error exits), `ecad-trial`'s
  `main`, and `ecad-bench/src/main.rs` (26 prints, 74 `Result<_, String>` in the crate).
- **Where.** `apps/*`, `xtask`, `ecad-bench` binaries. **Never** in a library crate's public API:
  libraries return their own `thiserror` enum, so callers can match on kinds.
- **Cost.** Tiny (one crate, `std` feature only).

### miette. Adopt later, narrowly.
- **Licence / lock.** Apache-2.0 (allowed); 7.6.0 on crates.io; not in the lock.
- **wasm.** The `Diagnostic` derive is plain `std`. The `fancy` feature (terminal colours, graphical
  report) pulls terminal crates and must stay native: enable it only in `apps/ecad-cli`.
- **Cost.** The derive alone is small; `fancy` adds roughly a dozen crates and some compile time.
  Measure on adoption.
- **What it is good for here.** Errors that point into a *text* source with a span:
  `.ecadlib` and `.ecad` v1 JSON validation messages ("line 40, `kind` must be one of ..."). It is the wrong
  tool for Checks: `checks.rs:838` `Check { kind, page, at, object, which, fix }` points at a drawing
  object and is drawn on the canvas, not on source text, and its 72 kinds are the R7 table.
- **Replaces.** Hand-formatted validation strings in `ecad-file` (v1 reader, library validation) and the
  CLI's printing in `apps/ecad-cli`. No file:line to quote yet because the library-validation entry
  point is not in one place today; R1 will name it.
- **Plan.** After R1 lands typed errors in `ecad-file`, add `#[derive(Diagnostic)]` to the validation enums
  (derive feature only, `default-features = false`), and use `fancy` in `ecad-cli`. Do not do it in R1
  itself: R1 stays a pure thiserror change.

### slotmap. No.
- **Licence.** Zlib. **Not on the allowlist**; adding it needs the owner to extend the list (the rules
  say a repository may not do so itself).
- **Fit.** ECAD's ids are 14 newtypes from `typed_id!` (`ecad-model/src/id.rs:10`): 128-bit,
  deterministic, serialised as hex and stable across sessions, merges and the op log. A slotmap key is
  a generational index into one map; it is not stable across runs or between two maps and could not be
  written to a `.ecad` file. Stored objects live in `BTreeMap<Id, T>`, which gives ordered, canonical
  iteration that the file format relies on.
- **Possible narrow use.** A purely in-memory arena (for example an undo stack node store in the UI).
  Nothing in the review asks for one. Revisit only if a lane shows a concrete need.

### rstar. Adopt later, on evidence.
- **Licence / lock.** MIT OR Apache-2.0; 0.13.0, same as CADCraft (`default-features = false` in
  `../crates/engine/Cargo.toml:25`).
- **wasm.** Yes. **Cost.** Already compiled for the engine; ECAD would add no new crate.
- **Where it would go.** The terminal capture scan `capture` in
  `ecad-cadcraft/src/interact/mod.rs:178` (loops over all `terminal_positions` of a page on every
  mouse move), `ecad-cadcraft/src/slash.rs:73` `hit` (segment tests) and
  `apps/ecad-trial/src/library/title_blocks/canvas.rs:173` `pick`. Object picking itself is already the
  engine's.
- **Why not now.** These scans are over one page; no bench or review item shows them slow. The
  derived-views rule (`ecad/CLAUDE.md`) says an index must be updated from the touched set, not
  rebuilt: an R-tree per page needs its own update map. Adopt when a medium-project bench shows a
  per-mouse-move cost, and then as a page-scoped derived view built on the R9 pipeline.

### petgraph. No, for now.
- **Licence / lock.** MIT OR Apache-2.0; 0.8.3, only via `tree_magic_mini` (transitive).
- **Fit.** Net building (`ecad-model/src/connectivity.rs`, 1,050 lines; hand-rolled union-find at `:773`
  with a loop guard, `union` at `:471`) is *incremental*: it is updated from each op's touched set and
  must never rescan. `petgraph::unionfind::UnionFind` and `connected_components` are whole-graph
  algorithms; using them means rebuilding, which the "no lazy default" rule forbids on a commit. They also
  index with `usize` and can panic on a bad index, so each call would need the same guard the code
  already has. ~50 lines of union-find are cheaper than the dependency in `ecad-model`, which must stay
  light and wasm-clean.
- **Revisit** for an analysis that is whole-graph by nature and runs on request (cable routing,
  "is the 24 V supply loop-free", derived-view dependency order in R9), in `ecad-cadcraft`, not
  `ecad-model`.

### kurbo. No.
- **Licence / lock.** Apache-2.0 OR MIT; 0.13.1, via `peniko`, `subsetter`, `write-fonts`
  (font and colour tooling), not the engine.
- **Fit.** ECAD's points and rectangles are model types that are serialised and guard `is_finite`
  (`ecad-model` `Pt`; `title_block.rs:83` `Rect`), and the engine already supplies `Vec2`. kurbo
  would be a third geometry vocabulary with its own conversions at every edge, to replace a few small
  functions (`interact/mod.rs:263` `nearest_on`, `slash.rs:73`). Its strength is Bezier curves, which
  ECAD does not draw (wires are orthogonal polylines).
- **Revisit** only if a brief introduces curved cable routes.

### tokio. No.
- **Licence / lock.** MIT; 1.53.2 with only `rt` and `time`, pulled by `wasm-bindgen-futures`.
- **Why.** The owner wants rayon, not tokio, to separate the UI from data exchange and checks. ECAD has no
  network or async I/O to run; the work is CPU-bound. An async runtime would add a second scheduler
  next to egui's frame loop and make wasm harder. Keep it out of every ECAD `Cargo.toml`.

### rayon. Adopt now, native only.
- **Licence / lock.** MIT OR Apache-2.0; 1.12.0 (rayon-core 1.13.0); CADCraft's root says `rayon = "1"`
  and `cadcraft-render` uses it (`pipeline.rs:148`, `viewports.rs:132`); `acadrust` also. Same version
  for ECAD, and it is already compiled, so no new crate in a normal build.
- **wasm. The risk.** `wasm32-unknown-unknown` has no threads by default: rayon compiles, but the
  global pool cannot start workers. Do not depend on its fallback behaviour. Follow what the repo
  already does with `zstd` (`ecad-file/Cargo.toml`, `[target.'cfg(not(target_arch = "wasm32"))']`):
  declare `rayon` only for non-wasm targets and keep a **sequential twin** behind one small function.
  `cargo xtask wasm` then stays green by construction and the lane proves it by running it.
- **Replaces.** `par_map` (`ecad-file/src/v2/chunks.rs:459-485`, about 30 lines of manual grouping,
  scoped threads and re-sorting) with `par_iter().map().collect()` (order is kept); the block at
  `ecad-model/src/index.rs:126-136`. Neither needs a pool the app owns.
- **New work it enables (the owner's aim).** Run checks (`checks.rs`), `.ecadlib` and `.ecad`
  validation, v2 open and parse, and report generation (058) **off the UI thread**, on copies:
  `rayon::spawn` on a small pool that leaves one core for egui, with the result sent back over a
  channel and applied on the next frame. This fits the debugger rule: read through `ProjectHandle`
  under `try_lock`, copy, release the lock, then compute.
- **Rules for lanes.** (1) Results are deterministic and in input order (tests compare lists). (2) No
  `unwrap`/panic in a closure: a panic inside a rayon task propagates to the caller, so closures return
  `Result`. (3) Parallelise only per-page or per-chunk work, not per-object loops. (4) Each use is
  benched on the medium project.
- **Cost.** Negligible binary growth for ECAD (already linked); compile time unchanged.
- **Lane.** A new small lane, **R11: parallel work and the UI thread**, after R1 (errors) and before R9
  builds `DerivedView` (so R9 can mark views "computable off thread"). Its first step, replacing
  `par_map`, is a safe, tester-covered change (file format reader: tester territory per `ecad/CLAUDE.md`).

### parking_lot. No.
- **Licence / lock.** MIT OR Apache-2.0; 0.12.5 via eframe, wgpu, epaint.
- **Fit.** Its gain is no lock poisoning and faster locks. ECAD's one lock helper is
  `ecad-cadcraft/src/access.rs:56` (`try_lock` mapped to an error, a deliberate "busy is an error, never
  a deadlock" rule). `std::sync::Mutex` is already futex-based on the supported platforms, and
  poisoning is handled in that one function. Switching changes `Mutex<T>` types across 10 files for no
  visible gain and moves a never-crash decision (what to do with a poisoned project) out of sight.

### gix. No.
- **Licence / lock.** MIT OR Apache-2.0; 0.90.0; not in the lock.
- **Fit.** Dozens of crates and a long compile for one use: `ecad-bench/src/report.rs:469` runs
  `git` to record the commit in a report. A dev tool shelling out to `git` is fine. gix also cannot
  build for wasm.

### rmcp. No.
- **Licence / lock.** Apache-2.0; 3.10.0; not in the lock.
- **Why.** It is async on tokio (against the owner's choice above), and it would replace CADCraft's
  `cadcraft-mcp` server, which is outside `ecad/` and shared with the base app: an upstream patch with
  its own approval. ECAD's typed-schema goal is met more cheaply by schemars in R6 (below).

### svg, usvg, svg2pdf. `svg` later; the other two no.
- **Licences.** svg 0.18 Apache-2.0 OR MIT; usvg 0.50 Apache-2.0 OR MIT; svg2pdf 0.13 MIT OR
  Apache-2.0. None in the lock.
- **Fit.** There is no SVG code in ECAD today (no match in `crates` or `apps`). PDF output already goes
  through CADCraft's `cadcraft-io` (krilla, pinned and patched in the workspace `[patch.crates-io]`
  block), which is what Print Book (024) uses. `svg2pdf` brings its own PDF writer stack and risks a
  second, conflicting krilla/pdf-writer next to the patched one. `usvg` *parses* SVG; ECAD only
  *writes* it.
- **Plan.** If an SVG export is briefed, the tiny `svg` writer crate (or plain string building) is
  enough; decide in that brief. Not part of 0.2.3 or 0.2.4.

### tracing. No; revisit with R9.
- **Licence / lock.** MIT; 0.1.44, via winit, zbus, calloop. CADCraft itself uses `log`.
- **Fit.** ECAD prints with `eprintln!` in two binaries (`ecad-bench` 26, `ecad-mcp` 10) and nowhere in
  libraries; that is correct for a stdio MCP server. If a library ever needs diagnostics, use `log`
  (already in the graph, same as CADCraft). `tracing` spans would help profile the R9 update pipeline
  ("which view took 12 ms for this op"); decide that inside R9, as a dev-only feature, not before.

### insta. Adopt later, dev-dependency.
- **Licence / lock.** Apache-2.0; 1.49.0; not in the lock. Dev-dependency only, so no ship cost and
  `cargo xtask wasm` (which builds libraries) is unaffected.
- **Fit.** The test review (R2) wants fewer hand-copied fixtures. Snapshot tests suit large text
  outputs: the debugger report, `list_commands` text, generated schemas (below), reports and exports,
  and canonical `.ecad` JSON of a small project. It does not replace the egui screenshot tests
  (`egui_kittest`).
- **Cost to the process.** Snapshot files are reviewed in the PR; CI must set `INSTA_UPDATE=no`;
  the owner reviews `cargo insta review` output. Adopt after R2 and R6 give it things worth
  snapshotting, one crate at a time, starting in `ecad-cadcraft`.

### schemars. Adopt now.
- **Licence / lock.** MIT; 1.2.3; not in the lock. Its tree is small (serde, serde_json, dyn-clone,
  ref-cast, schemars_derive).
- **wasm.** Yes (pure derive and data).
- **Replaces.** The hand-written `.params("{handle: hex ..., at: [x,y], label?}")` strings in
  `ecad-cadcraft/src/commands.rs` (e.g. `:67`, `:71`, `:75`, `:81`) and their parallel hand parsing
  (about 505 `as_str`/`as_f64` calls, 124 `run_*` functions, 85 "is required" messages). R6's
  `typed::<P>()` already deserialises params into structs; `#[derive(Deserialize, JsonSchema)]` on the
  same struct makes the documentation and the MCP/agent schema come out of the code, so they cannot
  drift (the `setprop` drift is the example).
- **Where.** Params structs live in `ecad-cadcraft` next to each `*_cmds.rs`. Typed ids
  (`typed_id!`, hex strings of 128 bits) need a `JsonSchema` impl that says "string, hex": implement it
  in `ecad-model` behind an optional `schema` feature (off by default, on from `ecad-cadcraft`), so
  `ecad-file`, `ecad-store` and the wasm library builds that do not need it never compile it.
- **Compatibility.** Keep today's compact params text as a *rendering* of the schema (a small helper) so
  agents and `describe_command` do not change in 0.2.3; expose the full JSON Schema as an additional
  field. The MCP contract tests (`mcp_contracts.rs`) check both.
- **Lane.** R6, module by module (the owner's decision 3 in the review).

## 5. One more crate worth deciding (not on the owner's list)

- **strum** (MIT; not in the lock; wasm yes; derive only). R8's `StarterKey`, `StyleCatalogue`, `Role`,
  `Side` each need `word()`/`parse()` plus an `ALL` list that a contract test enumerates; strum's
  `Display`/`EnumString`/`EnumIter` derive those and make `ALL` impossible to forget. The alternative is
  a small local macro. Recommendation: **adopt later (R8)** if the owner is happy with one more proc
  macro; otherwise a `declare_words!` macro in `ecad-model`. `serde(rename_all)` stays for stored words.

## 6. Where each crate may appear (so lanes do not wander)

| Crate (ECAD) | thiserror | anyhow | rayon | schemars | miette | insta |
|---|---|---|---|---|---|---|
| ecad-model | yes | no | non-wasm only | `schema` feature (ids) | no | no |
| ecad-store | if it needs an error type | no | no | no | no | no |
| ecad-file | yes | no | non-wasm only | no | later (derive) | no |
| ecad-inspect, ecad-tokens, ecad-ui | no | no | no | no | no | ui screenshots stay `egui_kittest` |
| ecad-cadcraft | yes (`CmdError`) | no | non-wasm only | yes | later | dev-dep, later |
| ecad-bench, apps, xtask | no | yes | no | no | `fancy` in `ecad-cli` only, later | no |

`cargo xtask layers` should gain a one-line check per rule above (R0's "ratchets" are the place): no
`anyhow` in a library `Cargo.toml`, no `tokio`, no `rayon` outside a `cfg(not(wasm32))` table.

## 7. Updates to the lane plan

- **R1** adds `thiserror` to `ecad-cadcraft`; `anyhow` at the app, CLI and xtask edges.
- **R6** adds `schemars`.
- **R11 (new, small): parallel work and the UI thread.** `rayon` replaces `par_map` and the index
  threads; then checks and validation run off the UI thread on copies. Tester required for the
  `par_map` change (file reader).
- **R2** may add `insta` (dev) after R6.
- **R8** may add `strum` if approved.
- No lane adds `slotmap`, `petgraph`, `kurbo`, `tokio`, `parking_lot`, `gix`, `rmcp`, `usvg`,
  `svg2pdf` or `tracing`.

## 8. Decisions for the owner

1. Approve **thiserror** in `ecad-cadcraft` (and `ecad-store` if needed): yes/no?
2. Approve **anyhow** at the edges only (apps, `ecad-bench`, `xtask`), never in a library API?
3. Approve **schemars** (MIT) for R6, with typed-id schemas behind an optional `schema` feature in
   `ecad-model`, and today's compact params text kept as a rendering of the schema?
4. Approve **rayon** as native-only (`cfg(not(wasm32))`), starting with replacing `par_map` at
   `ecad-file/src/v2/chunks.rs:459`, and a new lane **R11** for running checks and validation off the UI
   thread?
5. Confirm **no tokio** and **no rmcp** anywhere in ECAD.
6. **miette**: later (derive in `ecad-file` validation after R1, `fancy` in the CLI only), or never?
7. **insta** as a dev-dependency after R2 and R6: approve in principle?
8. **slotmap**: confirm **no** (Zlib is off the allowlist and its keys are not stable ids). Do you
   also want to *keep* Zlib off the list?
9. **rstar**: confirm "later, only when a medium-project bench shows a hit-test cost"?
10. **petgraph, kurbo, parking_lot, gix, usvg, svg2pdf**: confirm **no**; `svg` decided in the brief that
    first needs SVG export.
11. **tracing**: confirm "no; use `log` if a library needs diagnostics; R9 may revisit tracing as a
    dev-only profiling feature"?
12. **strum** (not on your list): adopt later for R8, or use a local macro?
13. Add the "no anyhow in libraries, no tokio, rayon only under `cfg(not(wasm32))`" checks to
    `cargo xtask layers` in R0?
