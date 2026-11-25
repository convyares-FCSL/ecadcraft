# ECADCraft roadmap

The plan, release by release, and the work that has been put off with what is needed to pick it up again.
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

## Next: v0.3.0, the rev 4 UI

Rev 4 keeps the shell and adds the engineering a small or medium team needs. Datasheet values (ratings,
dissipation, sizes) are typed on a device's part set until the parts catalogue arrives in 0.4.0.

### 0.2.1: foundations

- **Device-first model.** The device is the master: location, function, part number and every other
  device-level value are stored on it. Every symbol is an equal representation, typed by its definition
  (head, contact, pole, connection point, connector half), with derived parent, child and sibling relations.
  Location and function boxes *propose* a value and ask when it conflicts. The project keeps one list of
  functions and locations, with rename and merge.
- **Performance.** The BOM column change, strip build and PLC rack drop under their targets.
- **CI.** The full suite runs in parallel (nextest and sharded slow tests).

### 0.2.2: pages, title blocks, PLC signals, ratings

- **Page types and Settings ▸ Pages and sheets.** Each type brings its sheet, frame, title block and zones.
  Pages number from 1; a folder can carry a start number, with a Renumber pages button.
- **Generated front pages:** a cover with notes, wire-colour and symbol legends, revision history and
  contents, all kept up to date.
- **Title block editor** (Library ▸ Title blocks), and a built-in house-style border.
- **PLC signal list:** a card editor, and one-way import and export of tags by CSV (no live sync).
- **Wire and cable rating checks**, and volt drop. Rating tables are supplied by the user.
- **Shared UI components**, a style guide and a guard against hand-rolled controls.
- **Cable manager:** build cables in Project ▸ Cables, then assign them in the drawing.

### 0.2.3: tidy-up and improvements

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

### 0.2.4: layout, power, tests, network, paged reports

- **Panel layout page:** plate and door, rails, trunking with fill %, cut-outs, dimensions, and devices
  placed from the schematic at real size. No routing, no 3D.
- **Power:** the load tree traced from the drawing, budgets per source with peak and diversity, and
  enclosure heat.
- **Tests and FAT:** tests generated from the PLC list, retest when something changes after a pass, and a
  printable FAT sheet.
- **Network:** ports, cables, VLANs and zones, with a generated topology drawing.
- **Paged reports:** every report is a run of real pages, each updated on its own.
- **Title blocks built from elements:** frames and title blocks as two libraries; blocks made from
  revision, field, notice and logo elements.
- The 0.2.4 items under [Deferred](#deferred) below.

Network and Tests may move to 0.3.1 if the week runs short.

### 0.3.0: release

- The rev 4 boards checked side by side, the QA gate, and a re-run of the drawing-pack test.
- **A new README** with screenshots of the app and diagrams.

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

### v0.6.0: the project file pipeline

- A simpler, leaner `.ecad` pipeline.
- Groundwork for the v1 format freeze.

Out of scope on purpose: hazardous areas, functional-safety calculations, 3D and automatic wire routing.

## Deferred

Work that has been put off, with what is needed to pick it up again. Nothing listed here is built. Add an
entry when work is deferred; remove it when the work is approved and started. The list as it stood before
0.2.3's docs restructure is in the [archive](archive/plans/deferred-work.md).

### 0.2.4: docs and names found in the 0.2.3 close-out

- **A command reference made from the schemas, and a `help <command>` command.** Today there is no `help`
  command; the reference an agent reads comes from MCP `list_commands` and the control channel's
  `engine.commands`, both built from each command's parameters struct (see
  [agents/commands.md](agents/commands.md)). Generate a reference page from the same schemas and add a
  `help` command that shows one command's entry.
- **Two constants named `SCHEMA_VERSION`.** `ecad_file::SCHEMA_VERSION` (4) is the file's record version,
  checked on open and pinned by the golden test ([reference/file-format.md](reference/file-format.md));
  `ecad_model::SCHEMA_VERSION` (1) is a different number in the model crate. Rename or remove the model's
  so one name means one thing.
- **Where the UI fonts live.** The interface and sheet fonts (IBM Plex Sans, Barlow Semi Condensed) are in
  `ecad/assets/fonts/`; the house rule says font files live in `storytold/craft-fonts`. Decide which
  stands, and move the fonts or record the exception in [design/decisions.md](design/decisions.md).

### Guard follow-ups: 0.2.4

The guards review of the 0.2.3 close-out kept only bans at zero and area entries with a reason; these are
the counts it dropped, each to be finished in 0.2.4.

- **Raw egui controls:** 13 app panels made before the components still use `egui::TextEdit` or `ComboBox`
  (`RAW_CONTROL_FILES` in xtask). Convert them to `ecad_widgets::control`; an entry goes when its file has
  none left, and at none the list is deleted.
- **Literal sizes:** about 260 lines in 39 app files write a space, a stroke width or a size as a number
  (`add_space(4.0)`, `Stroke::new(1.0, ..)`, `vec2(12.0, ..)`). After a token pass that gives each a token,
  the literal-size ban returns as a hard zero.
- **Stored text fields:** 11 fields in ecad-model's stored structs are still `String` (potential `name` and
  `prefix`, `unclassified_prefix`, `label_rule`; a definition's and two drawing names, `linetype`; two device
  labels; the ledger's `label`). Type them as `Text<N>` or a typed value (a format change: bump
  `SCHEMA_VERSION`); then the G13 entry is removed and G13 is a hard zero.
- **Typed errors in the engine and the app:** the engine still has about 145 `Result<_, String>` lines and
  35 `Err(format!(..))`, the app about 57 and 6. Move them to the crate's error type; then the text-error
  ban covers both crates like the others.

### A potentials editor in the app: 0.2.4

- **What:** a view of the potentials table (name, prefix, numbered, supply) in project settings. Today the
  table is set only with `wiresettings`; no settings sheet shows potentials, so the supply cannot simply be
  a column of an existing view.
- **Also look at:** the project defaults `rating.voltage` and `rating.system` (the volt-drop calculation's
  supply) overlap with the supply typed on a potential; decide whether the volt-drop calculation reads the
  circuit's potential instead.

### Rating fields: 0.2.4

- **A generic rating reader:** today each group's reader maps the words of a typed rating to its fields by
  hand; a test makes sure every nameplate field of a group's list is read. In 0.2.4 the reader takes each
  value to a field by the field's type, so a new field needs no reader code.
- **List fields by `setprop`:** a power supply's `voltages`, a switching device's `control` and a fuse
  bank's `outputs` are read only on the Device tab and set through the whole rating text (or `rating.<n>`
  per fuse). In 0.2.4 they get list-typed values of their own.
- **One BOM value only when all agree:** a BOM row's article number, and its rating columns, show the most
  common value of the row's devices. In 0.2.4 a row shows a value only when all its devices agree, and flags
  the devices that differ.

### Report sheets as pages of their own: an open question

- **Question:** should each sheet of a report page (BOM, terminal strip pages, cable schedule, I/O list) be
  a page in the book of its own, rather than a derived view of one stored page? Recorded only as a question,
  not as the plan: the engine would then write pages nobody made, against "derived, never stored".
- **Today:** a report page is one stored page; its sheets are derived. The page window shows and draws one
  sheet at a time (the Pages tree lists them under the page), and Print Book prints them all. On the large
  project that brought a BOM column toggle from 95 ms to 16 ms and a strip edit with a 252-sheet graphic
  strip page shown from 436 ms to 6 ms, so keeping parts per sheet between edits was not needed.

### Versioned built-in defaults: at the v1 format freeze

- **What:** while ECAD is pre-alpha there is one built-in defaults set, edited freely, and every project
  reads the current built-ins (a changed built-in moves every project that did not choose its own value).
  At the v1 format freeze that must stop:
  - numbered built-in sets that never change once released; a project names its set in its settings,
    recorded in the "New project" transaction;
  - a lock file of released values and a test that fails when one changes;
  - a `usedefaults` command and a Standards line "Built-in defaults: set *n* (newest: *m*)", to move a
    project to newer built-ins, with a **Keep how it looks** option;
  - a build that does not know a project's set refuses to open it.
- **Open questions then:** does a set freeze when it reaches `dev` or at a release? Is **Keep how it looks**
  on by default? See [design/defaults.md](design/defaults.md).

### Forward compatibility of stored words: at the v1 format freeze

- **What:** pre-v1, ECAD does not read files from newer builds or other tools. A word this build doesn't
  know (a style catalogue, a role, a device kind, a settings word) is a read error: the project opens
  read-only, the message names the word, the setting and where it is, and the file is never changed. Enums
  are strict; there is no "keep the unknown word" variant.
- **Revisit at the v1 format freeze:** whether a v1 build keeps and writes back words from a newer build
  (and how a person sees them), and the **explicit import step** for files from other tools, which is where
  foreign words are mapped or refused.

### Ratings from parts: with the parts library

- **A part sets a device's rating:** when a part number's library entry carries a rating, it fills the
  device's typed rating fields; a value typed on the device is an override, and a Checks row reports an
  override that differs from the part. Not built until a parts library with ratings exists: no stub and no
  override handling now. Until then the typed rating fields are stored plainly.
- **Catalogue-part ratings:** terminal block types (largest and smallest cross-section, rated current,
  rated voltage, pitch, connection screw/spring/push-in) and PLC channel groups (a digital output's output
  current, an analog channel's resolution in bits). These are ratings of a catalogue part, not of the
  device: one set per strip or per module would store wrong data for a strip or module that mixes them. The
  other fields of terminal strips' neighbours, PLC stations and modules (supply, backplane current supplied
  and drawn, power, power loss) are built.

### Company standard file and more defaults as data: with libraries

- **Company standard file:** loading a house standard from a TOML file into the project's standard, as one
  undoable edit with the differences shown first. Waits for libraries, where a standard also comes from a
  `.ecadlib`; it reuses the defaults loader.
- **More defaults as data**, each its own piece of work: the house title block's columns and rows, cover
  page sizes, the terminal strip drawing look, drawing line weights (with plot styles).

### Libraries and checkpoints

- Libraries (`.ecadlib`) and their UI are planned for v0.4.0 (above). Checkpoints (faster opening of long
  histories) are on hold until libraries and the core domain features are settled. On the benchmark's large
  tier, about 13 s to open 170,000 transactions is acceptable for now.
- "Save a Copy As" with a new project ID.

### Upstream sync policy

- **What:** how ECAD owns the root entry points, drops CADCraft clutter (its AutoCAD-parity planning
  documents, README screenshots, packaging, release and packaging-lint workflows) and stays mergeable with
  CADCraft: one policy file in three path categories (ECAD-owned, excluded upstream, upstream-maintained),
  an explicit `cargo xtask sync-upstream` run only on `upstream-sync/<date>` branches (no `merge=ours`, no
  merge drivers), and a CI check (`cargo xtask policy`). The proposal is in the
  [archive](archive/plans/upstream-sync-policy.md); branch `docs/upstream-sync-policy` keeps its commits.
- **Today** `ecad/UPSTREAM.md`, `ecad/UPSTREAM-PATCHES.md` and `cargo xtask patches` do part of its job.
- **Before resuming:** re-check the proposed exclusion and keep lists against the current tree, then get approval to
  implement it.

### Small code follow-ups

- Delete `apply_ops` and the `test-ops` feature of ecad-engine (`access.rs`). It was added so tests could
  build cable reference ends before `cablereference` existed; tests in nine files still apply ops through
  it. Make each of those cases with a command first.
- Consider moving the definition content hash (`definition_hash`, canonical JSON, today in
  `ecad-file/src/record.rs`) into ecad-model, so the electrical rules don't depend on the file format.
- Pass the branch seed in explicitly: `create_branch` (`ecad-file/src/project/branches.rs`) takes its
  entropy from `ecad_store::random_u128` inside it.
