# ECAD (working title)

**A fast, modern, open-source electrical CAD. You draw; it works out the rest.**

Electrical CAD has been painful for too long. Today's tools make you build databases, manage projects, create parts and sync libraries before you can draw a single wire. This project aims to do for electrical schematics what Onshape did for 3D CAD: keep the drawing experience light and fast, and let the software infer the wires, cables, devices and cross-references in the background.

It is not an AutoCAD clone, and it is not trying to replace EPLAN for large teams. It is built for the person most ECAD serves badly: the **lone electrical designer**, or a small team of two or three, with no CAD administrator and no time for setup.

> **Status: pre-alpha, desktop-only.** The last release is `ecad-v0.2.0` (14 November 2026); this branch carries the next one, `0.3.0-dev`. ECAD runs natively on Windows and Linux. There is no web build. The ECAD code is in [`ecad/`](ecad/) and runs on the CADCraft drawing engine. The model and file format are still being shaped, so do not use it for production drawings yet. To build and try it, see [`ecad/docs/quick-start.md`](ecad/docs/quick-start.md).

## What works today

- **Projects:** one `.ecad` file per project. Open, Save and Save As, a lock against two writers, recovery from damaged files, and an undo history that is saved with the project, with branches of that history.
- **Devices, wires and cables:** tagged devices with several symbols each, wire categories and numbers, and cross-references between a device's symbols. A slash across wires makes a cable, with cores, traditional and fan views, and continuation across pages. Connectors have mating halves.
- **Terminal strips and PLC:** strip pages built from the drawing; PLC racks, cards, channels and an I/O list.
- **Checks and reports:** rule checks, wire and cable ratings, a bill of materials, and the PDF book.
- **Pages and sheets:** page types, title blocks and generated front pages.
- **The app:** a project panel, an inspector, a page well, and the Draw, Project, Library and Publish workspaces.
- **AI agents:** an MCP server (`ecad-mcp`) so an agent can read a project and, if allowed, draw in the running app. Its edits are recorded under its own name.
- **Tools for developers:** a read-only debugger panel, the `ecadinspect` tool for looking inside a `.ecad` file, and a load benchmark (`cargo xtask bench`).

Not yet: `.ecadlib` symbol libraries and a parts catalogue (0.4.0); panel layout, power, tests and network pages (0.2.4); DWG import and export.

---

## Principles

- **Infer, don't ask.** No database tables, project setup or library admin before drafting. Relationships come from what you draw.
- **Every inference is visible and overridable.** If you can't see what the software decided, you can't trust it.
- **Late binding.** Draw a generic "24 V PSU" or "7-core cable" now; choose the real part later.
- **The document is the truth.** A project is one file holding the record of your actions. Everything else is derived from it.
- **Local-first.** Works fully offline. Your file, on your machine, always.
- **Prove it for the user.** Old conventions stay only if they make the designer's day easier.
- **Classic 2D CAD feel.** Black canvas, crisp red/green/cyan vectors, utilitarian UI.

## What it fixes

| Pain in today's ECAD | How this project handles it |
| --- | --- |
| Cross-references tied to page/column coordinates break when pages are renumbered | **Identity by tag.** `K1` anywhere is the same device; cross-ref text is a live field; a hover lens previews the target page |
| Wires and cables managed separately; cable cores assigned one by one | **Cable slash.** Draw a slash across wires and a cable is inferred, cores in order, across pages |
| Project databases, server sync, locked projects, many sidecar files | **One-file project:** every action is recorded in the file, and you can read each one in the inspector. Email it; it opens anywhere |
| Parts and symbols must exist before you can draw | **Draw first, promote later.** A drawn box with connection points is a provisional type until you pick a real part |

## Architecture

You draw on the CADCraft engine. Every edit is a transaction recorded in the project file. Everything else (nets, cross-references, reports, the PDF book) is derived from that record.

- **Core:** the project model, the electrical rules (devices, wires, cables, strips, checks, reports) and the `.ecad` file. No screen and no drawing engine.
- **Engine:** `ecad-cadcraft`, the one link to [CADCraft](https://github.com/storytold/cadcraft) for drawing pages and running commands.
- **UI kit and App:** ECAD's widgets, and the application built from them (`ecad-app`).
- **Programs:** one thin `main` each: `ecad-desktop`, `ecad-cli` and `ecad-mcp`.
- **Dev:** the benchmark and the inspector, never shipped.

The layers, the rules between them and a README for every crate are in [`ecad/crates/README.md`](ecad/crates/README.md).

### File formats

- **`.ecad` project.** One file per project: a redb database holding its transactions, each checked when the file is opened. Current state is produced by replaying them. Saving is transactional, so a crash can't corrupt earlier work. Every transaction is readable in the inspector, in a raw view and a readable one, so the file is never a black box. Format versions are independent of the product version.
- **`.ecadlib` library (planned for 0.4.0).** Readable text: a folder of small TOML files, with CSV for large tables. Projects keep embedding what they use, so they always open on their own.
- **Determinism.** Your decisions ("not a connection", "K1 is part X") are stored as explicit transactions. Inference only fills what you never stated, so an update can't silently change an old project.

Design lessons are borrowed openly from Onshape: store only the user's changes, derive everything else, reference items by stable internal ID, and make undo append the inverse change rather than rewrite history.

## The cable idea

The clearest picture of the whole idea, and it works today:

1. Draw a handful of parallel wires between two devices.
2. Draw a slash across them: a cable is inferred, cores numbered in order, spares tracked.
3. Toggle the view between **traditional** (every core drawn as its own wire, cable shown by the slash) and **fan** (the cores merge into a single cable line that fans out at each end).

Both views come from the same data, so switching is instant and nothing is redrawn by hand.

## Planned features

Near releases are firm; later ones are headlines and will change.

- **0.3.0 (target: the week of 16 November):** a device-first model, page types and title blocks, a cable manager, PLC signal lists by CSV, wire and cable rating checks, then a panel layout page, power load tree, tests and FAT sheets, network pages and paged reports.
- **0.4.0:** `.ecadlib` libraries layered as `ecadstd`, then `IEC` and `ANSI`, with a parts catalogue, calculations from first principles, image logos and public example projects.
- **0.5.0 and 0.6.0:** manufacturer data and more PLC exchange formats, then a leaner project file pipeline ahead of the v1 format freeze.
- **Always:** AI changes arrive as proposals you accept, and release is always human-only.
- **Out of scope on purpose:** hazardous areas, functional-safety calculations, 3D and automatic wire routing.

## Roadmap

| Release | Scope |
| --- | --- |
| 0.2.0 (released) | Live MCP bridge, connectors, PLC racks and I/O list, strip pages, a complete BOM, the symbol builder, continuation cross-references, the style model, the rev 3 interface |
| 0.2.1 to 0.2.3 | Device-first model and speed; page types, title blocks, PLC signals, ratings and a cable manager; a code tidy-up (typed errors, one update pipeline, a crate split) |
| 0.2.4 | Panel layout, power, tests and FAT, network, paged reports |
| 0.3.0 | The QA gate for the above, and a new README with screenshots |
| 0.4.0 | `.ecadlib` libraries, parts catalogue, standards as data |
| 0.5.0 and 0.6.0 | Manufacturer data; a leaner project file pipeline |

The plan in full is [`ecad/docs/roadmap.md`](ecad/docs/roadmap.md), and [`ecad/docs/features.md`](ecad/docs/features.md) says which branch holds each feature. The original concept spec is [`docs/concept-spec.md`](docs/concept-spec.md) and design decisions are logged in [`docs/decisions.md`](docs/decisions.md). [`ROADMAP.md`](ROADMAP.md) is the roadmap of the CADCraft engine, not of ECAD.

## Try ECAD

The desktop program is `ecad-desktop`. **How to build, run and try it, and what has been tried so far:** [`ecad/docs/quick-start.md`](ecad/docs/quick-start.md), the single maintained guide.

## Building

Requires Rust. The ECAD workspace pins its toolchain in `ecad/rust-toolchain.toml` (1.99.1) and declares 1.95 as its minimum. A full debug build with debug info needs a lot of disk; set `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` to keep it down.

```sh
git clone https://github.com/convyares-FCSL/ecadcraft.git
cd ecadcraft/ecad
cargo run --release -p ecad-desktop
```

To open a saved project: `cargo run --release -p ecad-desktop -- path/to/project.ecad`. On Linux the desktop app needs the usual windowing development packages (X11/Wayland, `libxkbcommon`).

## Development workflow

- `main` is the released baseline and `dev` is integration. Each feature is developed on its own long-lived branch cut from `dev` (`feature/<name>`, listed in [`ecad/docs/features.md`](ecad/docs/features.md)) and reaches `dev` by one pull request when it works. `dev` reaches `main` through a QA gate: the full suite, screenshots, the benchmark and the owner's own click-through test.
- Pull requests into `dev` run a quick check (fmt, clippy with all targets, and the fast tests of the crates that changed); the full suite (`cargo xtask ci`, with the slow tests, on Linux and Windows) runs on pull requests into `main`. Rules for contributors and agents: [`ecad/CLAUDE.md`](ecad/CLAUDE.md).
- ECAD crates carry unit tests next to the code and one integration test executable per crate under `tests/all/`. Checks are scoped to what you changed (`cargo xtask test-changed`).
- Production code never panics: no `unwrap`, `expect`, `panic!` or `unsafe`; errors are `Result`s. See [`CLAUDE.md`](CLAUDE.md) for the full engine conventions inherited from CADCraft.

## Contributing

Contributions, ideas and real-world pain points are very welcome, especially from people who draw electrical schematics for a living.

- Open an issue to discuss a feature or report a problem before large changes.
- Keep pull requests small and focused, with tests.
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
