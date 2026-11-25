# Architecture

How ECAD is put together: the crates and the layers they sit in, what each layer may use, the document model
every crate shares, and how a change reaches the document as a transaction. The words are the glossary's
([`../reference/glossary.md`](../reference/glossary.md)). The binding decisions on names, crates and layers are in
[`../design/decisions.md`](../design/decisions.md); the source tree of each crate is in
[`../design/layout.md`](../design/layout.md).

## The principles

These hold for every crate. A change that would bend one goes to the owner first.

- **The ECAD document is the truth.** A project's state is what its transaction history gives when it is
  applied. CADCraft's drawing is a view of that state, for display and interaction. It is never saved as the
  project and never holds history.
- **Identity is stable and independent of names.** Every object has a typed ID that is never reused.
  References use IDs, never tags, labels or layer names, so a rename breaks nothing.
- **Edits are project-wide transactions.** A transaction is atomic, records resolved intent (outcomes, not
  gestures), names its author and carries what it replaced, so it can be inverted. One transaction may touch
  several pages and objects.
- **Undo and redo append.** They add transactions. History is never rewritten.
- **Stored, derived and temporary state are separate.** Only the user's intent and the drawing content are
  recorded. Derived results (checks, references, reports, generated pages) are worked out again, never read back
  as fact. UI state is not stored.
- **Meaning and drawing are separate.** A device, wire or cable may have any number of representations.
  Removing a representation never deletes its subject; deleting a subject is an explicit edit. Rerouting a wire
  keeps its identity and connections; reconnecting an end is a change of intent, and the recorded change says
  which it was. Switching a cable's view changes nothing in the project.
- **Incomplete designs are valid.** A property may be unset, and unset differs from any confirmed value and can
  be queried. Geometry may exist without electrical meaning: lines and shapes are valid drawing content, and
  Convert to Device gives existing geometry a device identity without redrawing it.
- **The project is self-contained.** It never needs a library, network or other file to open. A library change
  reaches a project only through an explicit edit.
- **History is preserved.** Nothing drops a transaction needed to rebuild the current state or a named version.
- **ECAD code is separate from CADCraft.** The document, its history and the electrical rules have no
  dependency on CADCraft. Only `ecad-engine` touches both.

## The layers

ECAD is its own Cargo workspace in `ecad/` (`crates/`, `apps/`, `dev/`). Every crate belongs to one of six
layers. The table, and the rules below it, are what `cargo xtask layers` enforces (`TABLE`, `CORE_ORDER`,
`UIKIT_ALLOWED` and the checks in `ecad/dev/xtask/src/layers.rs`). A crate that is not in the table fails the
guard.

| Layer | What it is | Crates |
| --- | --- | --- |
| **Core** | What a project is and means, with no screen and no drawing engine | `ecad-model`, `ecad-store`, `ecad-file`, `ecad-electrical`, `ecad-drawing-style` |
| **Engine** | The link to CADCraft: draws pages from the model, turns edits into transactions, runs commands | `ecad-engine` |
| **UI kit** | ECAD's widgets and screen tokens; knows nothing about projects | `ecad-widgets` |
| **App** | The ECAD user interface: shell, panels, lists, editors and UI state | `ecad-app` |
| **Programs** | Things a person or agent starts, one `main` each | `ecad-desktop`, `ecad-mcp` |
| **Dev** | Developer and test tools under `ecad/dev/`, never shipped | `ecad-bench`, `ecad-inspector`, `ecad-xtask` (`dev/xtask`) |

```text
Programs   ecad-desktop   ecad-mcp                  (may also use each other)
               |
App         ecad-app
             /      \
Engine  ecad-engine   UI kit: ecad-widgets          Engine and UI kit are siblings: neither uses the other
             \      /
Core    model -> store -> file -> electrical -> drawing-style
Dev     ecad-bench, ecad-inspector, xtask           (may use anything; no product crate uses them)
```

### The order inside Core

Core crates are ordered by what they know: `ecad-model`, `ecad-store`, `ecad-file`, `ecad-electrical`,
`ecad-drawing-style`. A Core crate depends only on Core crates before it in that list, so a cycle (such as
`ecad-file` → `ecad-electrical`) cannot be written.

- `ecad-model` defines what things are: the project, pages, devices, wires, cables, title blocks and their
  fields, the kinds of text, the defaults registry's mechanism and the canvas palette.
- `ecad-store` is the storage under the project file: the redb database handle, the writer lock, atomic
  replacement, the clock and randomness.
- `ecad-file` is the `.ecad` format: records, the history chain, opening, saving, recovery and Verify.
- `ecad-electrical` decides what things mean electrically: devices, wires, cables, terminal strips, references,
  checks, reports, locations, revisions. It reads a `Document` and returns plain data or ops.
- `ecad-drawing-style` decides how things look: built-in title blocks and frames, text styles, fonts and text
  fitting. It is last in Core.

The electrical rules lay out their own content but own no look. They ask through the `Look` trait, which
`ecad-model` defines (`ecad_model::look`) and `ecad-drawing-style` implements: a text kind's style, whether a
text is shown, how wide a text is, and a page frame's text styles, lines and title block. The checks that read
the look stay in electrical and are given it.

The defaults registry works the same way. Its mechanism (keys, levels, resolution) is `ecad_model::registry`;
each crate contributes its own entries; `ecad-drawing-style` assembles them into one `REGISTRY`, and the engine
passes it to commands and views.

### What each layer may depend on

These rules apply to normal dependencies in `Cargo.toml`:

| Layer | May use | Must not use |
| --- | --- | --- |
| Core | Core crates before it in the order; crates.io crates | any `cadcraft-*` crate, any UI toolkit or `ecad-widgets`, Engine, App, Programs |
| Engine | Core; `cadcraft-engine` and `cadcraft-io` | any other `cadcraft-*` crate, any UI toolkit or `ecad-widgets`, App, Programs |
| UI kit | `egui`, `epaint`, `egui_extras` and `ecad-model` (the canvas palette), nothing else | every other crate, ECAD's or CADCraft's |
| App | Core, Engine, UI kit, CADCraft's window crates | `cadcraft-engine` directly, Programs |
| Programs | anything in the product, other Programs included | `cadcraft-engine` directly |
| Dev | anything | — |

The UI toolkits the guard knows are `cadcraft-ui-egui`, `craft-ui`, `egui`, `epaint`, `egui_extras`, `eframe`,
`egui-wgpu`, `wgpu`, `winit`, `rfd` and `ecad-widgets` itself (`UI_CRATES`). Core and Engine use none of them,
so the document and the engine never draw a screen, and the UI kit stays swappable and tests without wgpu.

More rules, in the same guard:

- **Dev-dependencies keep the layer order.** A test may use extra crates, but no crate depends on a layer above
  it, not even in its tests (Core, then Engine and UI kit, then App, then Programs). An App test that needs
  `ecad-mcp` or `ecad-desktop` lives in that program's own suite.
- **Developer tools stay apart.** A crate is under `ecad/dev/` exactly when it is Dev. A product crate may
  depend on a Dev crate only as an optional dependency that its own `dev` feature enables, and `dev` is not on
  by default (`ecad-app` is the one crate allowed to have it on by default, for the desktop app's debugger).
  The details are in [`debugger.md`](debugger.md#the-developer-tool-tree).
- **Source checks.** App and Programs sources never name `cadcraft_engine`: Cargo alone is not enough, because
  `cadcraft-ui-egui` and `cadcraft-mcp` can hand engine types through. `ecad-electrical`'s sources never name a
  `cadcraft_` path.
- **Core reaches no OS, except `ecad-store`.** Outside their test modules, the other Core crates do not use the
  file system, the environment, processes, the network, the clock or OS randomness (`std::fs`, `std::env`,
  `std::process`, `std::net`, `SystemTime`, `Instant::now`, `RandomState`, `chrono::Local`, `Utc::now`,
  `home_dir`, `canonicalize`, `File::`, `OpenOptions`), and depend on no crate that finds the user's folders
  (`dirs`, `directories` and their variants). Threads and rayon are allowed. The caller passes the time, seeds
  and switches in. This keeps Core pure data and rules: testable and portable.
- **Crate rules.** `anyhow` only in Programs, `xtask` and `ecad-bench` (a library returns its own error type, so
  a caller can tell failures apart). `tokio` nowhere, dev-dependencies included: commands run synchronously on
  the project's lock, and a second runtime beside CADCraft's event loop would split where work happens. `rayon`
  only in `ecad-model`, `ecad-file`, `ecad-electrical` and `ecad-engine`, as a plain dependency.
- **Docs and errors.** Every crate has a README with the standard sections; library crates return typed errors.
  These and the other source guards are in [`guards.md`](guards.md).

ECAD is desktop-only: no crate has to build for the web or wasm, and there is no wasm check. CADCraft's own wasm
build is not affected.

To change a layer rule, change `TABLE` (or the list concerned) in `ecad/dev/xtask/src/layers.rs` and this page
together. Layers are a recorded decision, so the owner agrees first.

## Every crate

| Crate | Layer | Purpose |
| --- | --- | --- |
| `ecad-model` | Core | The typed document and how it changes: objects, ops, transactions, history, merge, numbering, nets; definitions (title blocks, text kinds, the registry mechanism) and the canvas palette (`ecad_model::palette`) |
| `ecad-store` | Core | The only crate that touches the disk for projects: the redb handle, the writer lock, atomic replacement, clock and randomness |
| `ecad-file` | Core | The `.ecad` format: records, canonical JSON, the history chain, snapshots, branches, Verify, Save and Save As |
| `ecad-electrical` | Core | The electrical rules, one folder per feature, and the generic command declarations |
| `ecad-drawing-style` | Core | The look of a page: built-in title blocks and frames, text styles, fonts, text fitting, `REGISTRY` |
| `ecad-engine` | Engine | The session over CADCraft: drawing, interaction, the generic command runner, session commands, undo, save, export |
| `ecad-widgets` | UI kit | Generic controls and screen tokens (the `tokens` module); no ECAD knowledge |
| `ecad-app` | App | Screens, panels, lists, editors and UI state; turns user actions into commands and shows their results |
| `ecad-desktop` | Programs | The process, the window, OS integration, wiring the app to the engine |
| `ecad-mcp` | Programs | ECAD for agents over MCP, with the write gate; headless use goes through it |
| `ecad-bench` | Dev | The load benchmark (`cargo xtask bench`) |
| `ecad-inspector` | Dev | `ecadinspect`: look inside a `.ecad` file, as text or a local web page |
| `ecad-xtask` | Dev | The workspace's own commands (`cargo xtask …`) |

`ecad-app` holds no rules, no disk access, no OS access and no parsing of domain values: those belong to Core
and the engine. Each crate's README is its short card (purpose, contents, what it must not depend on, key
types, how to test) and its `//!` doc is the detailed reference.

## The CADCraft boundary

CADCraft is the upstream drawing platform. It lives in the parent directory and ECAD uses its crates by path.

- **Only `ecad-engine` touches CADCraft's engine.** It may use `cadcraft-engine` and `cadcraft-io`. No Core
  crate depends on any `cadcraft-*` crate.
- **App and Programs reach the engine through `ecad_engine::cad`.** That module is a list of plain re-exports
  of the CADCraft engine types and functions they may name. A new type the App needs is added there on purpose,
  in review. They may use CADCraft's window crates (for example `cadcraft-ui-egui`) but never `cadcraft-engine`
  directly, in `Cargo.toml` or in their sources.
- **The engine is the adapter.** It draws each page from the model (each feature's `draw` module and
  `convert.rs`), turns CADCraft commits into ECAD ops (`translate/`), runs the commands and owns undo, redo and
  save. It holds no electrical rules: a function that needs nothing the engine has belongs in a Core crate, and a
  guard checks it ([`guards.md`](guards.md#engine-rule-functions)).
- **Upstream stays clean.** Nothing ECAD-specific goes into CADCraft's crates. Any file changed outside `ecad/`
  is an upstream patch: it is listed in `ecad/UPSTREAM-PATCHES.md` and marked in code with
  `// ECAD-SEAM(owner):`. `cargo xtask patches` fails on a changed file that is not listed. The CADCraft commit
  ECAD builds on is in `ecad/UPSTREAM.md`, separate from the product version and the file format versions.

## The document model

### Four kinds of state

| Kind | What it is | Examples | Stored? |
| --- | --- | --- | --- |
| **Stored intent** | Electrical meaning and the user's explicit decisions | devices and their connection points, wires and the connection points they land on, cables and their cores, property values, embedded definitions, styles | yes, as transactions |
| **Drawing content** | What is drawn and where, with or without meaning | primitives, symbol instances and their placement, connection point positions, layers, representations, deliberately placed texts, pages and folders | yes, as transactions |
| **Derived results** | Worked out from the two above | checks, references, reports, generated pages, the CADCraft drawing of a page and its handle map | no; may be kept as a view |
| **Temporary UI state** | This session's view of the work | selection, hover, zoom and pan, the page in front, the running tool, previews, panel layout, the cable view switch | no |

### Objects

The ops in `ecad_model::document::op` name what the document holds: pages and folders, layers, items (one
drawn primitive or one symbol instance), devices and their connection points, embedded definitions,
representations, wires and their routes, drawing aids (continuation arrows, location boxes), loose parts,
cables, device links, project settings, styles and the project's standard, and named versions.

- **Representations.** A representation depicts one subject (a device, wire, cable, route or aid) on one page.
  It has one of two forms. The **drawn** form is page items in page coordinates, with each connection point's
  position stored. The **symbol** form is one symbol instance of an embedded definition, with a placement; its
  connection point positions are worked out from the placement.
- **Placement.** `origin`, `rotation`, `mirror` and a uniform `scale`. Rotation is stored in radians, the same
  unit as CADCraft's insert, so the round trip through the drawing is exact. People work in degrees: commands
  that take an angle convert once, when the edit is made, and the converted value is what the op records, so
  replay never converts again. Displays show degrees.
- **Wires and nets.** A wire runs between two connection points. Connectivity is stored intent: an end lands on
  a connection point by reference. Coincident geometry is not a connection; checks report where the drawing and
  the connections disagree. Nets are kept up to date by every op, and a net carries the wire number.
- **Not shown is valid.** A subject with no representation is valid and is reported, not deleted. An object can
  be deleted only when nothing references it, and the deleting transaction removes those references explicitly.

### Identity and values

- One typed ID per object kind, so the compiler keeps them apart. IDs are 128-bit values allocated **when the
  edit is made** and recorded in the op; replay never allocates. As text an ID is exactly 32 hex digits. Users
  see tags and labels, never IDs.
- Every copy or paste allocates new IDs for everything it creates.
- Properties are optional: a missing key is unset, which differs from an empty value. An inferred value is
  derived and shown beside the stored one; accepting it records an explicit edit.

## The project file

The `.ecad` file is one format, a redb database written through `ecad-file` and `ecad-store`. The full
description is [`../reference/file-format.md`](../reference/file-format.md). What every crate relies on:

- **Self-contained.** The file holds the whole history and every definition the project uses.
- **Deterministic replay.** Replay applies the recorded values and IDs. It never snaps, searches, infers a
  connection or allocates an ID.
- **History is kept.** Nothing rewrites the file to drop transactions needed for the current state or a named
  version. Save As writes the full history. Snapshots let a project open without replaying from the start;
  Verify checks every snapshot against the replayed history.
- **Authorship.** Every transaction records its author and, when known, a wall-clock time. The time never
  affects replay.
- **Not in the file:** derived results as fact, UI state, CADCraft handles, a DXF or DWG copy.
- **Versions.** There is no compatibility before v1: any format change bumps the file's schema version
  (`ecad_file::SCHEMA_VERSION`), and a file of another version opens read-only with "older format version" (or
  "newer format version"). An op of any version but the current
  one is refused.
- **Damage.** A file whose history does not replay opens read-only at its last good transaction, and the error
  names the first bad one.
- **Only `ecad-store` touches the disk.** `ecad-file` works on the handles and bytes that `ecad-store`
  supplies; the engine opens files a command declares. Where the OS cannot promise durability, the code says
  so instead of reporting success.

## Transactions

### What a transaction is

```text
Transaction { id, seq, author, label, kind, time_ms, ops }
  author: User | Agent(<client name>)
  kind:   Edit | Undo { of } | Redo { of } | Merge { source branch, source head, ancestor }
```

- **Resolved intent.** Ops record outcomes (this connection point, these coordinates), not gestures, so a newer
  snapping rule never reinterprets an old edit.
- **Self-inverting.** Each op carries the values it replaces, so its inverse comes from the op alone.
- **Atomic and checked.** The document's invariants are checked for what the transaction touched; apply is all
  or nothing, with rollback.
- **Replay checks preconditions.** A `before` that does not match means the history is damaged (see above).

### Undo, redo and merge

- Undo appends `Undo { of }` holding the inverse ops; redo appends `Redo { of }`. Undo is linear: a new edit
  after an undo makes the undone edits no longer redoable, though they stay in the history.
- Undo is project-wide, so it may change pages that are not on screen.
- A merge of another branch records ordinary ops and where they came from. Branches, versions and revisions are
  described in [`../user/history.md`](../user/history.md).

### How an edit reaches the document

- **Commands.** Every edit a person or an agent makes is a command. A command's JSON form builds one
  transaction from resolved inputs and commits it through `Project::apply_edit`, which records it, updates the
  undo history and the derived views, and redraws. Generic commands are declared in `ecad-electrical` and run
  by the engine's one runner; commands that need the session live in the engine. See
  [`commands.md`](commands.md).
- **Edits in the drawing.** When a person moves or edits drawn objects, CADCraft commits a drawing change and
  the engine's `translate/` turns it into ECAD ops in one transaction, so wires follow the devices they land on.
- **Programmatic calls never prompt.** The JSON form always runs with its parameters. Only a menu, tool button
  or bare command name starts a command's click-based form, and that form commits through the JSON form.

### Unsaved state

- The project has a **head** (the last applied transaction) and a **saved position** (the last transaction
  known to be durably in the file). It is unsaved exactly when they differ.
- Every transaction moves the head: geometry, properties, cables, other pages, undo and redo. Undoing back to
  the saved content still leaves the project unsaved, because the history has grown.
- UI state (zoom, selection, panels) never moves the head.
- The saved position moves only after the file confirms the write. A failed save leaves the transactions
  pending, reports the error and loses nothing in memory; Save retries them.

### Derived views

A view, list or index (checks, references, reports, strip plans, the device list) is reached through the
project's `Views`, its one way in. Each rebuilds from scratch after a commit unless a full rebuild costs more
than about 5 ms per edit on the large bench project; only then does it keep an **update map**, which says which
effects change what it shows, what it does for each, and that every other effect does nothing. Views stay live:
there are no refresh buttons. Only the page on screen redraws; other open pages are marked stale and redraw when
brought to the front. A project opens on demand, and opening does not grow with history.

## Never crash

A crash loses a person's work, so this outranks feature work.

- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or
  `unimplemented!`. Every production crate root carries
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo,
  clippy::unreachable)]`; `clippy.toml` allows them in tests.
- **No `unsafe`:** `unsafe_code = "forbid"` in `[workspace.lints.rust]`. Every crate manifest carries
  `[lints] workspace = true`.
- **Errors are `Result`** through each crate's own error type (a `thiserror` enum) and `?`. An unfinished
  feature returns an error or says it is not available yet.
- **Input is hostile.** Files, command parameters and MCP or control-channel requests are checked: `get()`
  rather than indexing, checked arithmetic, no casts of NaN, and caps on sizes and counts (for example
  `MAX_COORD` for coordinates and bounded lists in command parameters, refused in words).
- **Containment is tested.** The engine's `fault-injection` feature puts deliberate panics inside ECAD commands
  and project callbacks, so tests prove they are contained; it is never on in a normal build.

## Where to read more

- [`commands.md`](commands.md): generic and session commands, and how to add one.
- [`features.md`](features.md): the folders a new feature gets in each crate.
- [`guards.md`](guards.md): what each `cargo xtask` guard prevents.
- [`testing.md`](testing.md) and [`build.md`](build.md): tests, benches and build setup.
- [`../design/decisions.md`](../design/decisions.md) and [`../design/layout.md`](../design/layout.md): the binding
  decisions and the source layout.
