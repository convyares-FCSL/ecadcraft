# CADCraft assessment (Phase 0)

> **Written:** 2026-10-12 against `dev` at `0305243` (CADCraft 0.6.2). **Purpose:** decide how the
> ECAD domain layer attaches to the CADCraft engine before Phase 1 starts. Read with
> [`concept-spec.md`](concept-spec.md).

## Summary

- The fork builds and its test suite passes on stable Rust with no changes (details under
  [Build and tests](#build-and-tests)).
- CADCraft is a large, well-layered, actively developed AutoCAD-parity engine (~155k lines of
  Rust including tests, 12 library crates, 3 apps). It has everything the ECAD needs to draw:
  geometry, object snaps, a command system shared by UI/CLI/MCP, a GPU canvas, DXF I/O and a DWG
  bridge.
- Its document model is a classic drawing database with whole-drawing copy-on-write undo. It has
  **no operation log and no extension points** for outside crates: the command registry is a
  closed static list, the post-command hooks are hard-wired, and `Session` has no slot for
  domain state.
- Three small, clearly-marked seams in `cadcraft-engine` are enough to attach the ECAD layer
  *beside* the drawing database. Everything else (ledger, topology, inference) can live in new
  crates that do not depend on the engine at all.
- **Recommendation:** an upstream-friendly soft fork. Keep CADCraft's crates as they are, add
  `ecad-*` crates beside them, confine engine edits to the three seams, and merge upstream
  periodically. Revisit a hard fork only if the UI needs diverge further than panels and
  overlays.

## Build and tests

Built on Linux x86_64 with Rust 1.97.0 stable (`cargo 1.97.0`); the workspace declares
`rust-version = "1.90"` and edition 2024. No source changes were needed.

| Gate | Result |
| --- | --- |
| `cargo test --workspace` | **1,660 passed, 0 failed, 0 ignored** across 252 test binaries and doc-test runs; 10.5 min on 4 cores from clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all --check` | clean |
| `cargo xtask layers` | 16 crates, no layering violations |
| `cargo xtask assets` | all 28 asset files attributed |
| `cargo xtask wasm` | builds for `wasm32-unknown-unknown` (target needs `rustup target add`) |

Build problems found:

- **Disk.** A default debug build of all targets with debug info and incremental caches
  reached 27 GB and ran out of space. With `CARGO_PROFILE_DEV_DEBUG=0`,
  `CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_INCREMENTAL=0` the whole build plus tests is 5.8 GB.
  CI uses these settings; developers with less disk should too.
- **Network at build time.** `craft-ui` is a git dependency on `storytold/craft-libs` (pinned
  revision), so a first build needs access to GitHub as well as crates.io.
- **No CI for tests upstream.** The inherited workflows only lint packaging and build releases
  through ArtCraft's shared actions; `.github/workflows/ci.yml` now runs fmt, clippy and tests on
  `dev`.

## Crate map

Layering is enforced by `cargo xtask layers` (`xtask/src/layers.rs`): nothing below L5 may depend
on egui, eframe, winit, wgpu or rfd, so the engine runs headless.

| Layer | Crate | Lines (incl. tests) | Role | ECAD use |
| --- | --- | ---: | --- | --- |
| L0 | `geom` | 4.4k | f64 vectors, matrices, bounds, lines/arcs/splines, intersections, tessellation | Topology maths (endpoint distance, segment intersection) |
| L0 | `dxf` | 1.2k | DXF tag reader/writer (ASCII and binary) with hostile-input caps | Indirect, via `io` |
| L1 | `color` | 0.3k | ACI colours, true colour, ByLayer/ByBlock | Red/green/cyan overlay colours |
| L1 | `doc` | 8.3k | **The drawing database** (`Drawing`, `Entity`, `EntityStore`, tables, blocks, layouts) | Projection target for pages, symbols and wires |
| L2 | `fonts` | 4.6k | Own stroke font, TrueType outlines, TEXT/MTEXT layout | Tags, wire numbers, cross-ref text |
| L2 | `render` | 8.3k | Display lists, CPU rasteriser (PNG, plot preview) | Page rendering, PNG/SVG resources for MCP |
| L2 | `constraints` | 4.1k | Parametric solver | Not needed |
| L3 | `io` | 19.5k | DXF ⇄ document, PDF plotting, SVG/PNG export | PDF set, DXF/DWG export |
| L3 | `dwg` | 0.3k | DWG ⇄ DXF bridge over `acadrust` (MPL-2.0) | DWG export and adopt |
| L4 | `engine` | 78.9k | `Session`, 295 commands, prompts, snaps, selection, grips, undo, spatial index | Drawing interaction |
| L5 | `ui-egui` | 21.3k | egui front end, GPU canvas, palettes, control channel | Needs ECAD panels and overlays |
| L5 | `mcp` | 0.8k | MCP server (JSON-RPC over stdio) driving a `Session` | Drawing-level MCP tools |
| app | `cadcraft`, `cadcraft-cli`, `cadcraft-web` | 4.0k | Desktop, CLI and WASM shells | Become the ECAD apps |

## Where the drawing database lives

`cadcraft_doc::Drawing` (`crates/doc/src/lib.rs`) holds the header, symbol tables (layers,
linetypes, styles), block definitions, model space, paper-space layouts, groups and constraints.

- Entities are stored in `EntityStore` (`crates/doc/src/store.rs`): chunks of `Arc<Entity>` in
  draw order plus a sharded handle index. Cloning a store copies only pointers, so a snapshot of
  the whole drawing is cheap and an edit copies one chunk.
- An `Entity` is `{ handle, common (layer, colour, linetype, …), kind: EntityKind }` with 25
  kinds (Line, LwPolyline, Circle, Arc, Text, MText, Insert with attributes, Hatch, Table, …).
- Blocks and layouts each own an `EntityStore`. The drawing is `Arc<Drawing>` inside the engine's
  per-document `DocState`.

## How commands and undo work

- **Commands** are `CommandSpec` values (`crates/engine/src/cmd/mod.rs`): id, label, menu path,
  aliases, a JSON `run: fn(&mut Session, &Value) -> Result<Value>`, an `enabled` check and an
  optional interactive prompt machine. The command line, menus, scripts, CLI, control channel and
  MCP all reach the same specs. The registry is a closed `OnceLock<Vec<CommandSpec>>` built from
  a fixed list of modules; an outside crate cannot add commands today.
- **Commit points.** A command changes the drawing through `Arc::make_mut`. There are exactly two
  places where a change is committed: `Session::execute` (JSON form, `crates/engine/src/lib.rs`)
  and the end of an interactive command (same file, `finish` path). Both hold the drawing as it
  was before (`before: Arc<Drawing>`), run three hard-wired post-command hooks (groups,
  constraints, associativity), refuse non-finite geometry and push an undo step.
- **Undo** is a stack of whole-drawing snapshots (`DocState::undo`, `Snapshot`), at most 2000 steps
  (`crates/engine/src/undoctl.rs`). UNDO marks, groups and Control are layered on top. Undo
  replaces the current drawing with an older one: it rewrites state rather than appending an
  inverse change, which is the opposite of the ECAD rule. For ECAD documents the ledger must own
  undo; the engine's snapshot undo can remain for plain drawing commands inside a step.
- **Safety.** `execute` and every interactive step run under `catch_unwind` and restore the
  drawing if a panic escapes. Production crates deny `unwrap`/`expect`/`panic!` and forbid
  `unsafe`. ECAD crates adopt the same rules.

## How entities are identified

- `Handle(u64)` is the DXF group-5 handle, unique per drawing, allocated from `Drawing::handseed`.
- Entity handles are **written unchanged** to DXF (`dxf_write.rs` writes `e.handle`) and read
  back, and the next free handle is kept in a `CADCRAFT_HANDSEED` XRECORD, so handles survive a
  CADCraft save/open round trip. Table records and blocks are re-numbered on save.
- Handles do **not** survive DWG import from another program as stable identities, copy/paste
  between drawings, or block explode. They are a per-drawing database key, not a domain identity.
- **Consequence:** the ledger allocates its own stable IDs (devices, pins, wires, cables, pages)
  and keeps a projection map `ledger id ⇄ engine handle`. Tags such as `K1` are properties, never
  keys, as the spec requires.

## XDATA and attributes

- **Block attributes** are modelled: `Insert.attribs: Vec<Attrib>` with tag, text, invisible,
  constant and per-attribute properties; `AttDef` entities in block definitions. ATTEDIT,
  ATTSYNC, ATTDISP and data extraction exist. This covers the spec's DWG export requirement
  ("plain entities plus block attributes").
- **CADCraft's own data** travels in xdata under the registered application `CADCRAFT` and in
  `CADCRAFT_*` XRECORDs in the named object dictionary (`crates/io/src/dxf_ext.rs`).
- **Other programs' data** (xdata, extension dictionaries, reactors, unmodelled objects) is kept
  raw in `Drawing::foreign` (`crates/doc/src/foreign.rs`) and written back on DXF save with
  handles remapped. Commands never edit it.
- **Through DWG** some of that is lost: xdata on table records and blocks, reactors, colour
  books and custom-class objects do not survive `acadrust`'s conversion
  (`docs/file-format-parity.md`). Entity xdata does. This matters little because DWG is
  exchange-only here, and the ECAD exports deliberately avoid custom objects.

## DWG read/write versions actually supported

| DWG | Code | Read | Write | Evidence |
| --- | --- | --- | --- | --- |
| R1.0–R12 | ≤ AC1009 | no | no | `is_dwg` accepts only `AC10xx`; `acadrust` doesn't target pre-R13 |
| R13/R14 | AC1012/AC1014 | via bridge, untested | no | not in `cadcraft_dwg::VERSIONS` |
| 2000 | AC1015 | yes | yes | `dwg_at_every_version_reads_back` |
| 2004 | AC1018 | yes | yes | same, plus `dwg_roundtrip_keeps_extension_data` |
| 2007 | AC1021 | yes | yes | same |
| 2010 | AC1024 | yes | yes | same, plus `ac1024_dwg_with_many_entities_opens` |
| 2013 | AC1027 | yes | yes | same |
| 2018 | AC1032 | yes (default) | yes (default) | same; **synthetic files only** |

All DWG support is a byte-level DXF bridge (`crates/dwg/src/lib.rs`): DWG → `acadrust` → DXF
text → CADCraft's DXF reader, and the reverse on save. No test opens a DWG made by AutoCAD and no
CADCraft-written DWG has been checked in AutoCAD; that is the single largest risk for the
Phase 3 gate ("DWG export opens cleanly in AutoCAD"). DXF R2000–R2018 is the more mature path
and a valid fallback for export. DWG is native only (not in the WASM build). Large DWG reads are
slow (a 10 MB file took about two minutes in an upstream report).

The concept spec says CADCraft's documentation is inconsistent on write versions; on this
revision the code (`VERSIONS`) and `docs/file-format-parity.md` agree on 2000–2018.

## Cleanest place to attach the domain layer

The ECAD crates split into two kinds:

1. **Pure domain crates** with no engine dependency: `ecad-ledger` (op log, checkpoints,
   versions), `ecad-topology` (connectivity from plain segments, points and pins), `ecad-infer`.
   They take simple input types (or `cadcraft-geom` values) and are fully testable without a
   `Session`. This is where most Phase 1 work happens.
2. **A bridge crate** (working name `ecad-session`, layer L4/L5) that owns an ECAD project
   alongside a `cadcraft_engine::Session`: it turns committed drawing changes into ops, projects
   ledger state into `Drawing`s for display, and registers ECAD commands.

The bridge needs three seams in `cadcraft-engine`, each small and upstreamable:

| Seam | Why | Shape |
| --- | --- | --- |
| Extension commands | ECAD commands (`slash`, `settag`, `cableview`) must reach the command line, menus, scripts and MCP like any other | Let an embedder supply extra `CommandSpec`s before the registry is first built |
| Commit observer | The ledger must see every committed drawing change, from both commit points | One hook called with `(command id, before, after)` beside the existing `after_command` hooks |
| Domain state slot | `Run` is a plain `fn` pointer, so ECAD commands need somewhere on `Session`/`DocState` to reach the project | An `extension` field holding the embedder's state |

Alternatives considered:

- **Wrapper only (no engine edits).** Diff the drawing from outside after each call. It works for
  JSON calls but misses interactive commands, which commit inside the engine, and ECAD commands
  could not be typed at the command line. Rejected.
- **ECAD data inside `Drawing`** (xdata on entities). Makes the drawing database the truth, the
  opposite of "the op log is the only source of truth", and xdata partly drops through DWG.
  Rejected.

Pages: a CADCraft `Drawing` has one model space and any number of paper-space layouts. Pages can
be either one projected `Drawing` per page or one layout per page in a single `Drawing` (which
gives multi-layout DWG export for free). Phase 1 needs only one page; the choice is recorded as
open in `docs/decisions.md` and settled in Phase 2.

## Fork strategy

**Recommendation: upstream-friendly soft fork.**

Reasons:

- The engine is large and moving fast (0.6.x, many merged PRs per week). A hard fork freezes us
  on today's snapping, DWG and rendering quality and makes us maintain ~150k lines we didn't
  write. A soft fork keeps those improvements flowing.
- The ECAD layer needs only three narrow seams; everything else is additive (new crates, new
  panels). Merge conflicts stay confined to a few lines in `engine/src/lib.rs` and
  `engine/src/cmd/mod.rs`, which we can propose upstream as general extension points.
- The concept spec's model (op log beside the drawing, derived state) does not require changing
  how CADCraft stores drawings, only observing and projecting them.

Rules that keep it upstream-friendly:

- CADCraft crates keep their names and licences; ECAD code lives in `crates/ecad-*` and new apps.
- Engine edits are limited to the seams above, each marked with a short comment, and offered
  upstream.
- `upstream` remote tracks `storytold/cadcraft`; merges into `dev` happen on their own branch with
  the full test suite green.

When to reconsider: if the ECAD UI has to replace large parts of `ui-egui` (not just add panels
and overlays), or if upstream changes the engine in ways that break the seams repeatedly, switch
to a hard fork of `ui-egui` first, keeping the engine soft-forked.

## Risks and items for the owner

- **`acadrust` is MPL-2.0** (file-level weak copyleft, used unmodified as a separate crate). It is
  not GPL, but the spec asks for MIT/Apache/BSD-compatible dependencies. Options: accept it as an
  unmodified separate library (CADCraft's position, see `NOTICE`), or gate DWG behind a cargo
  feature so a pure MIT/Apache build is possible.
- **DWG output has never been opened in AutoCAD.** The Phase 3 gate depends on it; an automated
  check against a free reference viewer (e.g. ODA File Converter) should be added before Phase 3.
- **Inherited agent instructions and CI.** `CLAUDE.md`/`AGENTS.md` describe CADCraft's own
  workflow (push to `main`, local `plan/`, AutoCAD clean-room observation), and the `release` and
  `packaging-lint` workflows call ArtCraft's shared actions and signing secrets. They need an
  ECAD rewrite (or removal) once the owner confirms.
- **Branding.** The apps, window titles, MCP server name and `docs/brand/` are CADCraft/ArtCraft.
  The ArtCraft marks are trademarks and should be removed from any ECAD release; this waits on
  the product name.

## Revision history

| Date | Change |
| --- | --- |
| 2026-10-12 | First assessment, Phase 0 |
