# ECADCraft roadmap

The plan, release by release, and the work that has been put off. Open work, each release's plan and each
piece of deferred work, is a brief in [briefs/](briefs/README.md).
Near releases are firm; later ones are headlines and will change as we learn. Background:
[road to v0.1](archive/plans/roadmap-v0.1.md) and the
[roadmap review](archive/discussions/roadmap-review-2026-11.md).

**How we work.** Each feature has its own long-lived `feature/<name>` branch, and its steps are commits on
that branch, not separate PRs. The whole feature reaches `dev` by one PR after one review and one fix round.
A release is a QA gate from `dev` to `main`: the full suite, the slow tests, the full bench and an owner
trial. `dev` carries the next version with `-dev` (now `0.3.0-dev`); the gate drops the suffix and the
`ecad-v…` tag follows. Every visible change is checked side by side against the UI design boards.

## What we aim for

A panel book an engineer would hand to a panel builder, done better than the tools it replaces:

- **Frame:** A3 pages with a lettered column grid and numbered rows; every reference names a page, a column
  and a row. Page numbers are chosen by the designer (`100`, `105`, `110`…), not counted.
- **Title block:** a revision table with status, function (`=`) and location (`+`), the project and drawing
  title, the drawing number, and this, next and total page.
- **Conductors:** each drawn as it is wired, labelled with its wire number, cross-section and colour
  (`L-01 2.5 Brown`); every conductor on one potential shares the number. Cable cores are labelled at their
  ends.
- **Devices:** IEC tags (`-K01`, `-Q03`), dashed location boxes, contact references under contacts, contact
  mirror tables under coils, PLC cards with channel labels.
- **Generated pages:** cover, revision list and contents, BOM by location, a graphical page per terminal
  strip (jumper bars, cables and cores), a cable-type list and a cable list.

## Released

### v0.2.0, 14 November 2026

Tagged `ecad-v0.2.0` on `main`. That build still reports `0.1.0-dev`; versions follow the tag from 0.3.0.

- **Engine:** a live MCP bridge, so an agent can draw in the running app; multi-level terminal blocks;
  connector devices with mating halves; PLC racks, cards, channels and an I/O list; graphical terminal-strip
  pages; a complete BOM; the symbol builder; continuation references; the style model with wire types.
- **UI:** a calm shell with one bar at each edge, panels that pin or float, inspector tabs, a page well,
  and the Draw, Project, Library and Publish workspaces.
- **Measured:** recreating a real 43-page drawing pack went from 12 to 34 recognisable pages.

## Next: v0.3.0, the functional core

The ladder to 0.3.0: 0.2.4 tidy-up → 0.2.5 features → 0.2.6 UI improvement → 0.2.7 the format release →
0.3.0, the functional core done well: good structure, a good build checked against the reference drawing
pack, and the solid foundation the `.ecadlib` libraries build on. Datasheet values (ratings, dissipation,
sizes) are typed on a device's part set until the parts catalogue arrives in 0.4.0. Each release from 0.2.4
has a release brief in [briefs/](briefs/README.md).

### 0.2.1: foundations (on `dev`)

- **Device-first model.** The device is the master: location, function, part number and every other
  device-level value are stored on it. Every symbol is an equal representation, typed by its definition
  (head, contact, pole, connection point, connector half), with derived parent, child and sibling relations.
  Location and function boxes *propose* a value and ask when it conflicts. The project keeps one list of
  functions and locations, with rename and merge.
- **Performance.** The BOM column change, strip build and PLC rack drop under their targets.
- **CI.** The full suite runs in parallel (nextest and sharded slow tests).

### 0.2.2: pages, title blocks, PLC signals, ratings (on `dev`)

- **Page types and Settings ▸ Pages and sheets.** Each type brings its sheet, frame, title block and zones.
  Pages number from 1; a folder can carry a start number, with a Renumber pages button.
- **Generated front pages:** a cover with notes, wire-colour and symbol legends, revision history and
  contents, all kept up to date.
- **Title block editor** (Library ▸ Title blocks), and a built-in house-style border.
- **PLC signal list:** a card editor, and one-way import and export of tags by CSV (no live sync).
- **Wire and cable rating checks**, and volt drop. Rating tables are supplied by the user.
- **Shared UI components**, a style guide and a guard against hand-rolled controls.
- **Cable manager:** build cables in Project ▸ Cables, then assign them in the drawing.

### 0.2.3: tidy-up and improvements (on `dev`)

- **Error handling:** typed error enums per library crate (`thiserror`), `anyhow` with context at the
  program edges; no `Result<_, String>` in the libraries.
- **Types instead of strings:** property keys, catalogue words, roles, check kinds and units (mm, A, V, °C)
  as enums and newtypes; typed command parameters.
- **One update pipeline:** a single classifier of what each edit touches, and one trait for every derived
  view.
- **Structure:** feature folders in every crate, the electrical domain apart from the engine adapter,
  developer tools under `dev/`, a README for every crate, the domain words everywhere (connection point,
  terminal block, jumper bar, references, reports).
- **Docs:** user guides by feature, developer and agent guides, one decision log, the history archived.
- Every change adds a guard (a ban at zero, an exhaustive match, a contract test) so it cannot regress.

### 0.2.4: bring upstream across, tidy up

Upstream CADCraft sync, typed commands end to end, the bugs and debt the 0.2.3 end gate and the engineering
reviews found, and the guard follow-ups. Brief: [release-0.2.4.md](briefs/release-0.2.4.md).

### 0.2.5: features

New features on the tidied framework: a recovery journal for unsaved work, an ECAD-domain agent tool set,
languages for the UI string table; the rest of the list is to come. Brief:
[release-0.2.5.md](briefs/release-0.2.5.md).

### 0.2.6: UI improvement

Reports as a mode of their own, paged reports, report tables, a home screen, opening off the UI thread, BOM
rating columns. Brief: [release-0.2.6.md](briefs/release-0.2.6.md).

### 0.2.7: the format release

The codec decision, migration, snapshots apart from the model, versioned built-in defaults, field-level ops
and the written freeze statement. Brief: [release-0.2.7.md](briefs/release-0.2.7.md).

### 0.3.0: release

- The functional core checked as a whole, the QA gate, and a re-run of the drawing-pack test against the
  reference pack.
- **The format freezes** (below).
- **A new README** with screenshots of the app and diagrams.

## Format freeze at 0.3.0

From 0.3.0 the `.ecad` format is frozen: a release can open every file an earlier frozen release wrote. Until
then older files need not open. The freeze is entered only when every item of
[the format release](briefs/release-0.2.7.md) is done:

- the codec decided and settled (`json/1` with its Zstandard setting, or `bin/1`);
- migration: an upcast layer, and a golden corpus of every released version that opens and verifies;
- snapshot chunks apart from the live model types, or declared a rebuildable cache with its own number;
- the invariant check of a loaded snapshot run in release builds, always or in the background;
- [versioned built-in defaults](briefs/versioned-defaults.md), and unparsable stored values shown as Checks
  rows;
- field-level modify ops as the stored form of typed fields; branches and merge frozen deliberately or marked
  experimental;
- [forward compatibility of stored words](briefs/forward-compatibility.md) decided;
- the salvage test: a file with damaged redb pages opens read-only at what verifies, or is refused, and never
  crashes;
- one written statement of what "frozen" covers: records and ops, chunks, meta and the `.ecadlib` seams.

## Not yet placed in a release

Features planned before the tidy-up releases, waiting for their place in the feature list:

- **Panel layout page:** plate and door, rails, trunking with fill %, cut-outs, dimensions, and devices
  placed from the schematic at real size. No routing, no 3D.
- **Power:** the load tree traced from the drawing, budgets per source with peak and diversity, and
  enclosure heat.
- **Tests and FAT:** tests generated from the PLC list, retest when something changes after a pass, and a
  printable FAT sheet.
- **Network:** ports, cables, VLANs and zones, with a generated topology drawing.
- **Title blocks built from elements:** frames and title blocks as two libraries; blocks made from revision,
  field, notice and logo elements.

## Later (headlines)

### v0.4.0: libraries, parts and standards as data

- **The `.ecadlib` library format:** readable text (a folder of small TOML files, CSV for large tables),
  easy for anyone to write and review, validated by a checker, versioned by content. Projects keep
  embedding what they use.
- **Layered libraries:** `ecadstd` (ECADCraft's own neutral base), then `IEC` and `ANSI`, each carrying its
  letter codes, colours, grid pitch and calculation method. A company library can sit on top. A project
  picks its standard.
- **Symbols reviewed group by group** against real drawings, keeping what engineers use. All symbols are
  independently authored; no standards database is reproduced.
- **Symbols on an abstract grid**, so inch and mm standards both get correctly proportioned symbols and text.
- **The parts catalogue**, which gives ratings, power, heat and panel layout real data.
- **Calculations from first principles** (volt drop, current rating) where possible, validated against
  licensed tables rather than shipping them.
- **Image logos** in title blocks.
- **A standards review:** what may be built in, what stays user-supplied, how to describe it.
- **House style and example projects.** A generic border in the house style, and public example projects
  in an `examples/` folder, from a small motor circuit to a full machine:
  - power distribution with drives;
  - a 24 V supply with UPS and electronic circuit protection;
  - a small PLC with fieldbuses;
  - a safety circuit;
  - a motor circuit with its terminal strip.

### v0.5.0: manufacturer data

- PLC exchange formats beyond CSV (for example TIA Portal and TwinCAT tag tables).
- Manufacturer part data and device templates.

Out of scope on purpose: hazardous areas, functional-safety calculations, 3D and automatic wire routing.

## Deferred

Work that has been put off, one line each; each has a draft brief in [briefs/](briefs/README.md) with what is
needed to pick it up again. Nothing listed here is built. Add a line and a draft brief when work is deferred;
remove the line when the work is approved and started. The list as it stood before 0.2.3's docs restructure
is in the [archive](archive/plans/deferred-work.md).

- **A potentials editor in the app**, not placed: [potentials-editor.md](briefs/potentials-editor.md).
- **Paged reports**, 0.2.6, with the open question of report sheets as pages of their own: the paged-reports
  brief and design in [briefs/](briefs/README.md).
- **Versioned built-in defaults**, 0.2.7, before the format freeze:
  [versioned-defaults.md](briefs/versioned-defaults.md).
- **Forward compatibility of stored words**, 0.2.7, before the format freeze:
  [forward-compatibility.md](briefs/forward-compatibility.md).
- **A compact binary encoding**, the seed of 0.2.7's codec decision:
  [eep-0003-binary-encoding.md](briefs/eep-0003-binary-encoding.md).
- **Upstream sync policy**, with the 0.2.4 sync: [upstream-sync-policy.md](briefs/upstream-sync-policy.md).
- **Ratings from parts**, with the parts catalogue: [ratings-from-parts.md](briefs/ratings-from-parts.md).
- **Company standard file and more defaults as data**, with libraries:
  [company-standard-file.md](briefs/company-standard-file.md).
- **Libraries** are planned for v0.4.0 (above). **Checkpoints and "Save a Copy As"** are on hold until
  libraries and the core domain features are settled: [checkpoints.md](briefs/checkpoints.md).
- **DXF export**, not placed: [dxf-export.md](briefs/dxf-export.md).
- **Small code follow-ups**, not placed: [code-follow-ups.md](briefs/code-follow-ups.md).
