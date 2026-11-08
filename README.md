# ECAD (working title)

**A fast, modern, open-source electrical CAD. You draw; it works out the rest.**

Electrical CAD has been painful for too long. Today's tools make you build databases, manage projects, create parts and sync libraries before you can draw a single wire. This project aims to do for electrical schematics what Onshape did for 3D CAD: keep the drawing experience light and fast, and let the software infer the wires, cables, devices and cross-references in the background.

It is not an AutoCAD clone, and it is not trying to replace EPLAN for large teams. It is built for the person most ECAD serves badly: the **lone electrical designer**, or a small team of two or three, with no CAD administrator and no time for setup.

> **Status: pre-alpha, in a desktop trial.** The ECAD crates in [`ecad/`](ecad/) run on the CADCraft engine in a trial app, natively on Windows and Linux. Nothing here is usable for production drawings yet: the model and file format are still being shaped. What works today is listed below; how to try it is in [`ecad/docs/quick-start.md`](ecad/docs/quick-start.md).

## What works today

- **Projects:** one `.ecad` file per project, with every page inside it, saved and reopened on Windows and Linux. The file is an append-only log, checked on every open, with safe saving and a lock against two writers.
- **Drawing:** components drawn from lines and given terminals, wires between them, and multiple pages (new, rename, reorder). Undo and redo of everything, and connected wires follow when a component moves.
- **Cables:** a slash across wires makes a cable, with traditional and fan views of the same cable, core designation, and the cable continued on another page with live `/page.column` references, Go to Reference, a hover lens and orphan (`/?`) markers.
- **AI agents:** an MCP server (`ecad-mcp`, headless) that reads and edits projects through the same commands as the app.
- **Tools:** a read-only debugger panel, a read-only HTML inspector, a load benchmark (`cargo xtask bench`) and start-up timing.
- **Speed:** a drawing edit takes about 0.5 ms on a medium project and 1.5 ms on a 500-page one; showing another page about 1 ms (benchmark, 2026-11-08).

Not yet: symbol libraries (`.ecadlib`), component cross-references, the project panel, reports and exports, DWG import.

---

## Principles

- **Infer, don't ask.** No database tables, project setup or library admin before drafting. Relationships come from what you draw.
- **Every inference is visible and overridable.** If you can't see what the software decided, you can't trust it.
- **Late binding.** Draw a generic "24 V PSU" or "7-core cable" now; choose the real part later.
- **The document is the truth.** A project is one file holding a log of your actions. Everything else is derived from it.
- **Local-first.** Works fully offline. Your file, on your machine, always.
- **Prove it for the user.** Old conventions stay only if they make the designer's day easier.
- **Classic 2D CAD feel.** Black canvas, crisp red/green/cyan vectors, utilitarian UI.

## What it fixes

| Pain in today's ECAD | How this project handles it |
| --- | --- |
| Cross-references tied to page/column coordinates break when pages are renumbered | **Identity by tag.** `K1` anywhere is the same device; cross-ref text is a live field; a hover lens previews the target page |
| Wires and cables managed separately; cable cores assigned one by one | **Cable slash.** Draw a slash across wires and a cable is inferred, cores in order, across pages; a sidebar shows the fan-in/fan-out |
| Project databases, server sync, locked projects, many sidecar files | **One-file project** stored as an operation log. Email it; it opens anywhere |
| Parts and symbols must exist before you can draw | **Draw first, promote later.** A drawn box with pins is a provisional type until you pick a real part |

## Architecture

```
            DWG import (adopt) ──┐            ┌── Libraries (.ecadlib)
                                 ▼            ▼
   Draw (CADCraft engine) ──► Operation log (.ecad) ──► Inference engine
                                                              │
                                                              ▼
                                          Derived state (cached in checkpoints)
                                                              │
                                                              ▼
                                   Views and outputs: overlays, report pages,
                                   PDF / DWG / CSV / XLSX, MCP server
```

- **Engine:** a Rust fork of [CADCraft](https://github.com/storytold/cadcraft) for geometry, snapping, commands and rendering.
- **Planned layout (below).** The crates built so far are `ecad-model` (the model and its operations), `ecad-store` and `ecad-file` (the project file), `ecad-cadcraft` (the bridge to the engine), `ecad-inspect`, `ecad-bench`, and the apps `ecad-trial` and `ecad-mcp`; see [`ecad/`](ecad/).
- **`ecad-ledger`:** the project file format: an append-only log of user operations (`AddWire`, `SetTag`, `SlashCable`, `PromotePart`, …), with checkpoints, named versions and compaction.
- **`ecad-topology`:** builds the connectivity graph from geometry and pins, with strict junction rules (crossing lines never connect; T-junctions need a dot).
- **`ecad-infer`:** tag identity, cables, off-page matching, orphan and open-end detection.
- **`ecad-mcp`:** an MCP server so AI agents can read and edit projects through the same API the app uses.

### File formats

- **`.ecad` project.** One file. An append-only operation log (JSON lines today; a binary codec may follow). Current state is produced by replaying the log; checkpoints only speed up opening and can always be regenerated. Saving only appends, so a crash can't corrupt earlier work. Named versions (Rev A, pre-FAT) are bookmarks in the log.
- **`.ecadlib` library.** Same format, holding symbols. Symbols are copied into projects but remember their origin, so projects always open on their own.
- **Determinism.** Your decisions ("not a connection", "K1 is part X") are stored as explicit operations. Inference only fills what you never stated, so an engine update can't silently change an old project.

Design lessons are borrowed openly from Onshape: store only the user's changes, derive everything else, reference items by stable internal ID, and make undo append the inverse change rather than rewrite history.

## First milestone: wires to cable, traditional ⇄ fan

The first visible win, chosen because it shows the whole idea in one gesture:

1. Draw a handful of parallel wires between two devices.
2. Draw a slash across them: a cable is inferred, cores numbered in order, spares tracked.
3. Toggle the view between **traditional** (every core drawn as its own wire, cable shown by the slash) and **fan** (the cores merge into a single cable line that fans out at each end), with one key or button.

Both views come from the same data, so switching is instant and nothing is redrawn by hand. The cable schedule updates as you go.

## Planned features

- Multi-page schematics with first-class pages (reorder and renumber freely)
- Live cross-references, coil/contact tables and hover lens
- Cable inference with fan-in/fan-out sidebar and spare-core tracking
- One-click switch between traditional and fan cable views
- Connectivity overlay: verified nets, inferred junctions, orphans and open ends
- Symbol libraries with one search box (name, tag prefix, function, pin labels, or a rough sketch)
- Report pages that are always up to date: BOM, wire list, cable schedule, terminal plan, labels
- One-button **Release**: pre-release check, named version, PDF + exports all stamped with the same revision
- **DWG import ("adopt")** of existing drawings and AutoCAD Electrical projects, with a confidence review
- **DWG export** (plain entities and attributes; opens cleanly in AutoCAD)
- **CSV / XLSX export** of BOM, wire list, cable schedule and labels
- **MCP server** for AI assistance; AI changes arrive as proposals you accept, and release is always human-only

## Roadmap

| Phase | Scope | Gate to next phase |
| --- | --- | --- |
| 0 Assess | Build CADCraft, map its crates, choose fork strategy | Fork plan approved |
| 1 Core | Op-log file, topology graph, tag inference, MCP spike; **first milestone: wires to cable with traditional ⇄ fan toggle** | Replay equals checkpoint; golden tests pass; milestone demo works |
| 2 Draft | Wires, cables, cross-refs, symbols and libraries | A real project drawn end to end |
| 3 Output | Report pages, Release, PDF / CSV / XLSX / DWG export | DWG export opens cleanly in AutoCAD |
| 4 Adopt | DWG and `.wdp` import with review step | A legacy project converts cleanly |

The full concept spec lives in [`docs/concept-spec.md`](docs/concept-spec.md). Design decisions are logged one per line in [`docs/decisions.md`](docs/decisions.md).

## Try the ECAD trial

The current ECAD work runs in a desktop trial app (an experiment): CADCraft's window with the ECAD commands, one project, and `.ecad` project files. **How to build, run and try it, and what has been tried so far:** [`ecad/docs/quick-start.md`](ecad/docs/quick-start.md), the single maintained guide.

## Building

Requires a recent stable Rust toolchain (the workspace declares Rust 1.90 as its minimum; it is built and tested with 1.97). A full debug build with debug info needs over 25 GB of disk; set `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` to keep it near 6 GB.

```sh
git clone https://github.com/convyares-FCSL/ecadcraft.git
cd ecadcraft
cargo build --release
cargo test --workspace
```

Run the inherited CADCraft drawing app with a sample drawing:

```sh
cargo run --release -p cadcraft -- --sample
```

On Linux the desktop app needs the usual windowing development packages (X11/Wayland, `libxkbcommon`).

## Development workflow

- `main` is the released baseline and `dev` is integration. Each feature is developed on its own long-lived branch cut from `dev` (`feature/<name>`, listed in [`ecad/docs/features.md`](ecad/docs/features.md)) and reaches `dev` by one pull request when it works. `dev` reaches `main` through a QA gate: the full suite, screenshots, the benchmark and an owner trial.
- Pull requests into `dev` run a quick check (`cargo fmt --check` and `cargo clippy --all-targets`); the full suite (`cargo xtask ci` with assets, crate layering and the WebAssembly build, plus `cargo test --workspace`, on Linux and Windows) runs on pull requests into `main`. Rules for contributors and agents: [`ecad/CLAUDE.md`](ecad/CLAUDE.md).
- File-format and topology rules are written test-first; ECAD crates carry unit tests next to the code, integration tests under `tests/`, and golden schematics for topology.
- Production code never panics: no `unwrap`, `expect`, `panic!` or `unsafe`; errors are `Result`s. See [`CLAUDE.md`](CLAUDE.md) for the full engine conventions inherited from CADCraft.

## Contributing

Contributions, ideas and real-world pain points are very welcome, especially from people who draw electrical schematics for a living.

- Open an issue to discuss a feature or report a problem before large changes.
- Keep pull requests small and focused, with tests for file-format and topology rules.
- Record design decisions in `docs/decisions.md` (one line each, with the reason).
- Dependencies must be MIT, Apache-2.0 or BSD compatible. No GPL.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

This is a non-commercial, community project. The aim is simply a good electrical CAD that anyone can use and improve.

## Acknowledgements

Built on [CADCraft](https://github.com/storytold/cadcraft) (MIT / Apache-2.0) by the ArtCraft Team and the CADCraft contributors. Its original licence and copyright notices are retained in [`NOTICE`](NOTICE), [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE). The ArtCraft name and logos in `docs/brand/` are trademarks of the ArtCraft Team and are not covered by the open-source licence. The CADCraft engine documentation (architecture, parity checklists, control protocol, MCP) remains under [`docs/`](docs/) for reference.

## Contact

Carl — ca99896@googlemail.com
