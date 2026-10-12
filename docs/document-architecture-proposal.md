# ECAD document architecture and integration proposal

> **Status:** revision 3. Revision 2 was approved as the basis for the bounded integration
> experiment (§17); the integration choices in Part B stay provisional until demonstrated. **Written:** 2026-10-12 against `dev`
> at `baee1b8` (CADCraft 0.6.2). **Reads with:**
> [`ecad-architecture-foundations.md`](ecad-architecture-foundations.md) (takes precedence),
> [`concept-spec.md`](concept-spec.md) and [`cadcraft-assessment.md`](cadcraft-assessment.md).

## Summary

The document has four parts so that settled direction is not confused with things still to
prove:

- **Part A, document principles.** The ECAD document is authoritative. Identity is stable and
  independent of names. Edits are project-wide transactions, and undo appends to history rather
  than rewriting it. Meaning is separate from drawing. History is never destroyed while
  compaction is undecided. The review supported these; they are written as principles.
- **Part B, provisional integration design.** How CADCraft is attached: a separate `ecad/`
  workspace, a projection of the document into CADCraft drawings, and translation of CADCraft's
  committed drawing changes into transactions. The translation is a **hypothesis** that still
  needs proof, together with the movement, copy and deletion policies built on it.
- **Part C, integration experiment.** The smallest experiment that would prove or disprove
  Part B. It is specified here but not run.
- **Part D, decisions,** split into four groups: direction already supported, revised
  recommendations needing approval, temporary limits that apply to the experiment only, and
  product behaviour deferred.

Changes from revision 1 (details in [Revision 2 changes](#revision-2-changes)):

- Removing a representation is now distinct from deleting its subject.
- Moving connected equipment carries attached wire ends along.
- Moving a wire end is a deliberate reconnection.
- A plain copy of a whole component becomes an independent copy, and is part of the experiment.
- "Unsaved" is measured against the project's saved transaction position.
- Library symbols and drawn components are two explicit representation forms with their own
  placement data.
- Destructive compaction is deferred.
- The engine boundary is now a broader *generic extension interface* across about ten
  upstream files.

## Contents

1. [Scope and evidence markers](#1-scope-and-evidence-markers)

**Part A: document principles**

2. [Principles](#2-principles)
3. [Document model](#3-document-model)
4. [Representations and symbols](#4-representations-and-symbols)
5. [`.ecad` and `.ecadlib` contracts](#5-ecad-and-ecadlib-contracts)
6. [Transactions, history and unsaved state](#6-transactions-history-and-unsaved-state)
7. [Worked example](#7-worked-example)

**Part B: provisional integration design**

8. [Repository layout](#8-repository-layout)
9. [How edits reach the document](#9-how-edits-reach-the-document)
10. [Movement and connectivity](#10-movement-and-connectivity)
11. [Copy, paste and placement](#11-copy-paste-and-placement)
12. [Removing representations and deleting objects](#12-removing-representations-and-deleting-objects)
13. [ID mapping lifecycle](#13-id-mapping-lifecycle)
14. [Generic extension interface](#14-generic-extension-interface)
15. [Incorporating upstream updates](#15-incorporating-upstream-updates)
16. [Optional DWG boundary](#16-optional-dwg-boundary)

**Part C: integration experiment**

17. [Bounded integration experiment](#17-bounded-integration-experiment)

**Part D: decisions**

18. [Decision table](#18-decision-table)
19. [Room for later features](#19-room-for-later-features)

## 1. Scope and evidence markers

**Covered:**

- the document principles and objects, and what is stored versus derived;
- the representation of drawn components and library symbols;
- the file contracts and unsaved state;
- the transaction path, together with the movement, copy and deletion policies;
- how ECAD IDs relate to CADCraft handles;
- the engine extension interface and the upstream merge process;
- an integration experiment for later approval.

**Not covered:**

- the cable demonstration, numbering schemes, the completeness heat map, MCP and library
  replacement UI;
- file encoding, product naming and default drawing standards;
- DWG licence acceptance.

No code was changed for this revision.

Integration claims carry a marker:

| Marker | Meaning |
| --- | --- |
| **[Shown]** | Demonstrated by building or running code in this session |
| **[Read]** | Read from the source at `baee1b8`, with file and line; not yet exercised |
| **[Assumed]** | Expected but unverified; the experiment (§17) or a listed test must prove it |

---

# Part A: document principles

## 2. Principles

These are the architectural direction the review supports. Part B may change; these should not
change without a new review.

| # | Principle |
| --- | --- |
| **P1** | **The ECAD document is authoritative.** The project's state comes from applying its transaction history. CADCraft's drawing is a view of that state used for display and interaction; it is never saved as the project and never holds history. |
| **P2** | **Identity is stable and independent of names.** Every object has a typed internal ID, which is never reused. References use IDs, never tags, labels or layer names, so renaming breaks nothing. |
| **P3** | **Edits are project-wide transactions.** A transaction is atomic, records resolved intent (outcomes, not gestures), names its author and carries enough to be inverted. One transaction may touch several pages and objects. |
| **P4** | **Undo and redo append.** They add transactions; history is never rewritten. |
| **P5** | **Stored, derived and temporary state are separate.** Only stored intent and drawing content are recorded. Derived results (nets, completeness, reports) are recomputed, never read back as fact. UI state is not stored. |
| **P6** | **Meaning and drawing are separate.** A component, conductor or cable may have zero or more representations. Removing a representation never implies deleting its subject; deleting a subject is an explicit edit, which may be bundled with removing representations in one transaction. |
| **P7** | **Incomplete designs are valid.** Every property may be unset, and "unset" is distinguishable from any confirmed value and can be queried. Geometry may exist without meaning. |
| **P8** | **The project is self-contained.** It never needs a library, network or other file to open. Library changes reach a project only by an explicit edit. |
| **P9** | **History is preserved.** Until a preservation contract is approved, no transaction needed to rebuild the current state or any named version may be discarded. Checkpoints are disposable caches. |
| **P10** | **ECAD code is separate.** The document, history and library code have no dependency on CADCraft; only the adapter touches both. |

## 3. Document model

### 3.1 Four kinds of state

| Kind | What it is | Examples | Stored? |
| --- | --- | --- | --- |
| **Stored intent** | Electrical meaning and the user's explicit decisions | components, terminals, conductors and their end attachments, cables and membership, property values, embedded definitions | yes, as transactions |
| **Drawing content** | What is drawn and where, with or without meaning | primitives, symbol instances and their placement, connection points, layers, representation links, deliberate annotation placement, pages | yes, as transactions |
| **Derived results** | Recomputable from the two above | nets, open ends, completeness, diagnostics, duplicate-tag warnings, terminal positions of symbol instances, BOM, wire list, the CADCraft projection and handle map | no; may be cached |
| **Temporary UI state** | This session's view of the work | selection, hover, zoom and pan, active page, running command, previews, panel layout, cable view toggle | no |

### 3.2 Objects

| Object | Kind | Holds | Relationships (by ID) |
| --- | --- | --- | --- |
| `Project` | intent | settings (units), page order | pages |
| `Page` | content | title, sheet settings | items, representations |
| `Item` | content | one drawn primitive (line, polyline, arc, circle, text, point) **or** a symbol instance (§4); style (layer, colour, linetype, lineweight) | page; at most one representation |
| `Layer` | content | name, colour, linetype, visibility, plot flag | — |
| `Component` | intent | properties (tag, description, manufacturer, part number, …), all optional | terminals; zero or more representations; optional embedded definition |
| `Terminal` | intent | optional label; optional key into the component's definition | one component |
| `Representation` | content | form (**drawn** or **symbol**, §4), role | subject (component, conductor or cable); page; items; connection points (drawn form) |
| `ConnectionPoint` | content | page position (drawn form only) | one terminal; one representation |
| `Conductor` | intent | properties (size, colour, type, identifiers), all optional | ends A and B, each with an optional attachment; zero or more route representations |
| `Cable` | intent | properties, all optional | ordered memberships (conductor, optional core designation) |
| `EmbeddedDefinition` | intent | a project-local copy of a library definition (§4.2) | origin (library, definition, version, content hash) |

Nets are derived from attachments (and, later, junctions and bridges); they are not stored
objects. A cable has no "core" object duplicating a conductor; "core" is a conductor's role
through its membership.

### 3.3 Rules

- **DOC-01** Only stored intent and drawing content are recorded (P5).
- **DOC-02** References are by ID (P2).
- **DOC-03** Connectivity is stored intent: a conductor end is attached to a terminal by
  reference. Coincident geometry is not a connection; derived checks report when geometry and
  attachment disagree.
- **DOC-04** Grouping conductors into a cable adds memberships and changes nothing about the
  conductors.
- **DOC-05** (proposed) An item belongs to at most one representation.
- **DOC-06** A subject with no representation is valid ("not shown") and is reported, not
  deleted (P6).
- **DOC-07** An object can only be deleted when nothing references it. The deleting transaction
  removes the references explicitly (detach ends, remove memberships, unlink representations).

### 3.4 Identity and values

- One typed ID per object kind, so the compiler prevents mixing them.
- Proposed encoding: 128-bit random, allocated **when the edit is made** and recorded in the op.
  Replay never allocates. Users see tags and labels; diagnostics show a short form. A copy
  allocates new IDs for everything it creates (§11).
- Properties are `Unset` or `Set(value)`. Inferred values are derived and shown beside the
  stored value; accepting one records an explicit `SetProperty`.

## 4. Representations and symbols

Revision 1 contradicted itself here: it described library symbols as block inserts, left inserts
out of the drawing schema, and projected all components as groups of primitives. The resolution
is **two explicit representation forms**, each with its own stored data, both in the
ECAD-owned schema.

### 4.1 The two forms

| | **Drawn representation** | **Symbol representation** |
| --- | --- | --- |
| Comes from | Convert to component (raw geometry gains meaning) | placing an embedded definition (library, panel, paste) |
| Stored drawing content | the page items themselves, in page coordinates; one `ConnectionPoint` per terminal, in page coordinates | one `SymbolInstance` item: definition ID + placement |
| Placement | implicit in the items' coordinates | explicit: origin (x, y), rotation (degrees), mirror (yes/no), uniform scale (> 0) |
| Terminal page position | stored (`ConnectionPoint`) | derived: placement applied to the definition's local terminal position |
| Projected into CADCraft as | the primitives, an unnamed selectable group around them, and a point marker per connection point | one block per embedded definition (graphics + terminal point markers) and one `Insert` per instance |
| Moving it in CADCraft | the group is picked whole; items and markers move; the diff shows new coordinates | the `Insert` changes position, rotation or scale; the diff gives the new placement |

The forms are not alternatives to choose between. A drawn representation lets raw geometry
become a component without being redrawn, as the foundations require. A symbol representation
lets many instances share one definition and lets an explicit library update or replacement
find every instance. **Promoting** a drawn representation to a symbol (making a definition from
it) is a later explicit transaction that keeps component and terminal IDs. It is not designed
here.

Projection details, all **[Read]** and unverified in use:

- Whole-group picking relies on `PICKSTYLE` bit 1, which is on by default
  (`engine/src/lib.rs:361-418`).
- Snapping already walks into block contents (`block_leaves`, `engine/src/snap.rs:635`).
- Terminal markers can sit on a non-plotting layer, as CADCraft's `Defpoints` layer does.

### 4.2 Embedded definition content

```
EmbeddedDefinition {
    id, origin: Option<(library, definition, version, content hash)>,
    graphics:  [Primitive]                 // in the symbol's local frame, base point (0, 0);
                                           // nested symbols deferred
    terminals: [TerminalDef { key, default label, local position }],
    text slots: [ … ]                      // tag/label placement; deferred with annotation work
}
```

**Placement transform (normative):** `page = origin + R(rotation) · M(mirror) · scale · local`,
where `M` negates x when mirrored. These are our own stored values. They map onto CADCraft's
`Insert { insert, scale, rotation }` (`doc/src/entity.rs:382-399`) **[Read]** with
`scale = (±s, s, 1)`. Non-uniform scale, MINSERT rows/columns and oblique placement are outside
the schema; CADCraft edits that would produce them are refused (§18.3, L6).

### 4.3 Reproducing placement without CADCraft

The saved document holds primitives, placements and definitions in **our** schema. Opening a
project rebuilds CADCraft blocks and inserts from them, so nothing depends on CADCraft's
internal `Entity` serialisation. Terminal positions of symbol instances are recomputed from the
placement. Floating-point results may differ in the last digit across platforms, so position
comparisons (is this end on its terminal?) use the snap tolerance. Connectivity itself is by ID
and never depends on those comparisons (DOC-03).

The projector's round trip (document → CADCraft → diff → document) must be lossless for every
supported kind. That is hypothesis H2 (§9.4), proved by the experiment.

## 5. `.ecad` and `.ecadlib` contracts

These contracts state what each file owns and promises; the encoding comes later and must meet
them.

### 5.1 `.ecad` project file

**Owns:** the project's transaction history, every embedded definition it uses, named versions
as bookmarks, and optional checkpoints as a cache.

- **FILE-01 Self-contained** (P8).
- **FILE-02 Deterministic replay.** Replay applies recorded values and IDs; it never snaps,
  searches, infers attachments or allocates IDs.
- **FILE-03 Durable append.** Saving appends whole transactions and then flushes them to disk
  (`fsync`). A torn trailing write is ignored on open.
- **FILE-04 Checkpoints are caches.** Deleting every checkpoint changes nothing but opening
  speed. This holds **unconditionally** in the initial implementation (P9).
- **FILE-05 No destructive compaction.** Nothing rewrites the file in a way that drops
  transactions needed for the current state or any named version until a preservation contract
  is approved. Save As writes the full history.
- **FILE-06 Versioned ops.** Every op type has a version and old versions are upcast on replay.
- **FILE-07 Authorship.** Every transaction records its author and a wall-clock time; time
  never affects replay.

**Does not contain:** derived results as fact, UI state, CADCraft handles, or a DXF/DWG copy.

### 5.2 `.ecadlib` library file

**Owns:** reusable definitions (graphics, terminal definitions, default properties), the
library's history and named versions, and a library ID that survives renaming the file. It
follows FILE-02, -03, -05, -06 and -07. Whether parts share this container stays open.

### 5.3 From definition to instance

- **LIB-01** Placing a definition embeds a copy in the same transaction if the project lacks it
  (`EmbedDefinition` + `CreateComponent` + `AddTerminal` per key + `LinkRepresentation` with a
  `SymbolInstance`). Later instances reuse that copy.
- **LIB-02** Instance terminals have their own IDs and record their definition key; connections
  reference instance terminals.
- **LIB-03** A library change only produces an "update available" result; updating is an
  explicit, undoable transaction.
- **LIB-04** Editing an embedded definition in the project creates a new local definition
  ("modified from …"); other instances keep the original.
- **LIB-05** Definitions with the same name but different origin or content are both kept.
- Components need no definition (drawn form).

Open: terminal mapping when a definition is replaced (including connected terminals without a
counterpart), placement on replacement, and parts versus symbols in one container.

## 6. Transactions, history and unsaved state

### 6.1 Transactions

```
Transaction { id, seq, author, time, label,
              kind: Edit | Undo { of } | Redo { of },
              ops: [Op] }            // applied in order, all or nothing
```

| Area | Representative ops |
| --- | --- |
| Drawing content | `AddItem`, `ModifyItem { before, after }`, `RemoveItem { before }`, `AddLayer`, `ModifyLayer` |
| Representation | `LinkRepresentation`, `UnlinkRepresentation { before }`, `AddConnectionPoint`, `MoveConnectionPoint { before, after }`, `RemoveConnectionPoint { before }` |
| Components | `CreateComponent`, `DeleteComponent { before }`, `AddTerminal`, `RemoveTerminal { before }` |
| Conductors | `CreateConductor`, `DeleteConductor { before }`, `AttachEnd { conductor, end, before, after }` |
| Cables | `CreateCable`, `DeleteCable { before }`, `AddCableMember`, `RemoveCableMember { before }` |
| Any object | `SetProperty { object, key, before, after }` |
| Library | `EmbedDefinition`, `UpdateDefinition { before, after }` |
| History | `NameVersion` (bookmark) |

- **TX-01 Resolved intent.** Ops record outcomes (terminal `t2`, coordinates), not gestures.
- **TX-02 Self-inverting.** Each op carries the values it replaces.
- **TX-03 Atomic and validated.** Invariants (references exist, DOC-05, DOC-07) are checked;
  apply is all or nothing on a copy-on-write document.
- **TX-04 Replay checks preconditions.** A `before` that does not match means a corrupt log;
  the project opens read-only with a report.

### 6.2 Undo and redo

- **UNDO-01** Undo appends `Undo { of }` holding the concrete inverse ops; redo appends
  `Redo { of }`.
- **UNDO-02** The undo/redo position is derived from the log. Undo is linear: a new edit after
  undo makes the undone edits no longer redoable, though they remain in the log.
- **UNDO-03** Undo is project-wide, so it may change pages that are not on screen.

### 6.3 Unsaved state

Revision 1 proposed reusing CADCraft's dirty check, which compares the drawing pointer with the
saved one (`engine/src/lib.rs:162-164`) **[Read]**. That is wrong for a project. A property
edit, a cable membership change or an edit on another page can leave the active drawing
untouched.

- **SAVE-01** The project has a **head** (sequence number of the last applied transaction) and
  a **persisted position** (the last transaction known to be durably in the file). The project
  is **unsaved if and only if head ≠ persisted position**.
- **SAVE-02** Every transaction moves the head: geometry, properties, membership, other pages,
  undo and redo. Undoing back to the saved content still leaves the project unsaved, because
  the log has grown. That is correct for an append-only file.
- **SAVE-03** Temporary UI state (zoom, selection, panels, current layer if classed
  session-only) never moves the head.
- **SAVE-04 Save failure.** The persisted position advances only after the appended bytes are
  flushed. If writing or flushing fails, the position stays where it was, the project stays
  unsaved, the error is reported, and nothing in memory is lost. Before the next save, any torn
  trailing bytes are truncated back to the last good offset. A save to a new path (Save As,
  first save) writes a temporary file, flushes it and renames it into place, so a failure never
  damages an existing file.
- **SAVE-05 Pages versus project.** A page tab is a view of the project. Closing a page tab
  never prompts and never discards changes, because the project and its unsaved transactions
  remain open. Closing the **project** (Close Project, quitting the app, or closing its last
  page if the product chooses that, §18.4) prompts Save / Discard / Cancel when it is unsaved. If
  Save fails, the project stays open.
- **SAVE-06** Each page tab of an unsaved project shows the unsaved marker, so a non-graphical
  edit is visible wherever the project is shown.

Crash recovery between saves (a journal file next to the project) is deferred product behaviour.
CADCraft's drawing autosave is not used for ECAD pages (§14).

## 7. Worked example

Short IDs (`i1`, `c1`, `T1`) stand for 128-bit IDs. Coordinates are millimetres on page `p1`;
handles are illustrative. Steps 1-5 use the drawn form.

**1. Draw raw geometry.** LINE draws a 40 × 20 rectangle.
```
T1 Edit "Line"   AddItem i1 (0,0)-(40,0)   AddItem i2 (40,0)-(40,20)
                 AddItem i3 (40,20)-(0,20) AddItem i4 (0,20)-(0,0)
```

**2. Convert it to a component and add terminals.**
```
T2 Edit "Convert to component"   CreateComponent c1 (all Unset)
                                 LinkRepresentation r1 form=drawn subject=c1 items=[i1..i4]
T3 Edit "Add terminals"          AddTerminal t1 c1   AddConnectionPoint t1 r1 (40,15)
                                 AddTerminal t2 c1   AddConnectionPoint t2 r1 (40,10)
                                 AddTerminal t3 c1   AddConnectionPoint t3 r1 (40,5)
```

**3. Draw a loose conductor, then connect it.**
```
T4 Edit "Wire"      CreateConductor w1   AddItem i5 [(90,15),(55,15)]
                    LinkRepresentation r2 form=drawn subject=w1 role=route items=[i5]
T5 Edit "Connect"   ModifyItem i5 [(90,15),(55,15)] → [(90,15),(40,15)]
                    AttachEnd w1.B none → t1
T6/T7 Edit "Wire"   w2 to t2 and w3 to t3, each created already attached
                    (WIRE resolved the snap to the connection point when it was drawn)
```

**4. Group the conductors into a cable without changing their identities.**
```
T8 Edit "Group as cable"   CreateCable k1
                           AddCableMember k1 w1 0   AddCableMember k1 w2 1   AddCableMember k1 w3 2
```

**5. Reroute a wire without changing connectivity.** A grip adds a vertex to `i6` at (65,25);
both ends stay put.
```
T9 Edit "Grip edit"   ModifyItem i6 [(90,10),(40,10)] → [(90,10),(65,25),(40,10)]
```

**6. Move the connected component.** MOVE picks the component's group (items and connection
point markers) and moves it by (0,−5). The attached wire ends follow in the **same**
transaction (§10, rule M2):
```
T10 Edit "Move"   ModifyItem i1..i4 (each by (0,-5))
                  MoveConnectionPoint t1 (40,15)→(40,10)  t2 (40,10)→(40,5)  t3 (40,5)→(40,0)
                  ModifyItem i5 end B (40,15)→(40,10)   ModifyItem i6 end B (40,10)→(40,5)
                  ModifyItem i7 end B (40,5)→(40,0)
```
Attachments are unchanged, and drawing and intent still agree.

**7. Undo and redo.**
```
T11 Undo of T10   inverse of every T10 op (component and wire ends back)
T12 Undo of T9    ModifyItem i6 back to [(90,10),(40,10)]
T13 Undo of T8    RemoveCableMember ×3, DeleteCable k1
T14 Redo of T13   CreateCable k1 (same ID), AddCableMember ×3
```

**Replay.** Opening the file applies T1-T14 to an empty document, applying each op as recorded.
The result equals the live state. A checkpoint after T14 must give the same state, and deleting
it changes nothing (FILE-04). Saved after T14, the project is clean. A later `SetProperty c1 tag
"-K1"` changes no drawing but makes it unsaved (SAVE-01).

---

# Part B: provisional integration design

Everything in Part B is a proposal to be proved by the experiment (§17). The repository layout
is supported in direction; the rest are hypotheses.

## 8. Repository layout

```
ecadcraft/                          fork of storytold/cadcraft
├── crates/ apps/ xtask/            UPSTREAM  unchanged except the extension interface (§14)
├── assets/ packaging/ examples/    UPSTREAM
├── docs/                           UPSTREAM  (ECAD docs move to ecad/docs on approval)
├── Cargo.toml  Cargo.lock          UPSTREAM  root workspace, untouched
├── README.md                       OURS      repository front page
├── CLAUDE.md  AGENTS.md            UPSTREAM today; ECAD rewrite pending (§18.2)
├── .github/workflows/ci.yml        OURS      builds both workspaces
└── ecad/                           OURS      separate Cargo workspace
    ├── Cargo.toml  Cargo.lock      [workspace]; path deps on ../crates/*
    ├── rustfmt.toml  clippy.toml   our style
    ├── CLAUDE.md                   ECAD agent instructions
    ├── UPSTREAM-PATCHES.md         every upstream file we change, why, and its test
    ├── docs/                       contracts, decisions, proposals
    ├── crates/
    │   ├── ecad-model/             objects, IDs, ops, transactions, apply/invert, validation, history
    │   ├── ecad-store/             .ecad/.ecadlib container: framing, durability, recovery, checkpoints
    │   ├── ecad-library/           definitions, embedding, origin, "update available"
    │   ├── ecad-derive/            connectivity, nets, open ends, completeness, diagnostics
    │   ├── ecad-cadcraft/          adapter: projector, handle map, commit translation, owner, commands
    │   ├── ecad-ui/                panels and overlays
    │   └── ecad-testkit/           (dev only) builders, golden logs, scenario runner
    ├── apps/ecad, apps/ecad-cli    desktop shell around CadApp; headless replay/verify/export
    └── xtask/                      ECAD layering, upstream patch guard, wasm check
```

| Part | Location | Depends on | Must not depend on |
| --- | --- | --- | --- |
| Upstream CADCraft | `crates/`, `apps/`, `xtask/` | upstream | `ecad/` |
| Shell and UI extensions | `ecad/apps/ecad`, `ecad-ui` | adapter, `cadcraft-ui-egui` | — |
| Document, history, library | `ecad-model`, `ecad-store`, `ecad-library`, `ecad-derive` | each other, small external crates | **any `cadcraft-*` crate** (P10) |
| Adapter | `ecad-cadcraft` | model crates, `cadcraft-engine`, `-doc`, `-geom` | UI crates |
| Tests | unit tests per crate; `tests/<area>_contracts.rs`; Session-driven tests in `ecad-cadcraft/tests` | `ecad-testkit` | — |
| Documentation | `ecad/docs/`, root `README.md` | — | — |

**Separate workspace [Shown]:** a temporary nested workspace depending on
`cadcraft-engine` by path built on Rust 1.97 and ran `Session::execute("line", …)`; upstream
`workspace = true` dependencies resolved against the root, and the root workspace was unaffected.
It was deleted afterwards. Adding ECAD crates to the root workspace instead would need patches
to the root `Cargo.toml` and `xtask/src/layers.rs`, whose table rejects unregistered crates
(`Unregistered`, line 150) **[Read]**.

Conventions in `ecad/` follow your repositories:

- `rustfmt` at `max_width = 120` with `use_small_heuristics = "Max"`;
- a crate-level `//!` stating what the crate deliberately excludes;
- doc comments with `# Semantics`, `# Errors`, `# Example` and `# See also`;
- one structured error type per crate;
- numbered contract statements, with unverified claims marked;
- table-driven `tests/<area>_contracts.rs`.

CADCraft's never-crash rules also apply. Upstream's tree is not moved, because moving it would
turn every merge into rename conflicts.

### 8.1 Versioning (agreed)

| What | Version scheme | Where it is recorded |
| --- | --- | --- |
| ECAD product (the `ecad/` workspace) | `0.1.0-dev` during development | `ecad/Cargo.toml` `[workspace.package] version` |
| ECAD releases | tags `ecad-v<version>` (for example `ecad-v0.1.0`); upstream's `v…` tags are left alone | git tags |
| Incorporated CADCraft | tracked separately as the upstream commit merged in (today `00c7613`, CADCraft 0.6.2); updated by each upstream sync PR | `ecad/UPSTREAM.md` |
| Document formats (`.ecad`, `.ecadlib`, op schemas) | their own integer versions, independent of product versions; a product release never changes a format version unless the format changes | constants in `ecad-model` and `ecad-store` |

## 9. How edits reach the document

### 9.1 Two paths, one ledger

```
 ECAD command (WIRE, CONNECT, PLACE, …) ───────────────► Ledger.commit(tx)
                                                              ▲   │ apply
 CADCraft command (LINE, MOVE, grips, COPY, PASTECLIP)        │   ▼
   edits the projected drawing                                │  ECAD document
   │                                                          │   │
   ▼ commit point (Session::execute / finish)                 │   ▼ projector
 owner.commit(diff vs last projection, provenance) ──tx──────┘  CADCraft drawing(s)
   verdict: accept │ accept with amendments │ refuse
```

**Path A, ECAD commands** **[Read]**:

- ECAD commands are `CommandSpec`s added through the extension interface, so they reach the
  command line, menus, scripts, the control channel and MCP. All of these read
  `command_specs()`: `ui-egui/src/menus.rs:434`, `cmdline.rs:134`, `control.rs:189`,
  `mcp/src/backend.rs:203`.
- Interactive ones implement the public `Interactive` trait (`engine/src/prompt.rs:173`), so
  they get prompts, snaps and previews.
- Each builds a transaction from resolved inputs and commits it; the projector updates the
  affected drawings.

**Path B, CADCraft commands on ECAD pages:**

- CADCraft commits in two places: `Session::execute` (`engine/src/lib.rs:752-789`) and the end of
  an interactive command (`finish`, `1092-1128`). Both run the post-command passes first (groups,
  constraints, associativity) **[Read]**.
- At each commit point, the page's owner receives the command ID, the drawing and the
  provenance the engine recorded (§11). It compares the drawing with the **last projection** it
  wrote and classifies every difference.
- It then returns one of three verdicts:
  - **accept**: the drawing is the new projection;
  - **accept with amendments**: for example, attached wire ends follow moved equipment. The
    owner writes the amended projection;
  - **refuse**: the engine restores the last projection and echoes the reason.
- No CADCraft undo snapshot is pushed for owned pages.

### 9.2 Cancelled, refused and failed commands

| Case | What happens | Evidence |
| --- | --- | --- |
| Esc during a command | CADCraft's `finish(cancelled)` keeps what the command already made (LINE segments drawn before Esc), as AutoCAD does; the owner translates exactly that. A cancelled MOVE has changed nothing, so the diff is empty and no transaction is appended | `lib.rs:1092-1128` **[Read]** |
| Owner refuses (unsupported kind, refused operation, failed validation) | drawing reset to the last projection, selection cleared, ledger and map unchanged, reason echoed | restore pattern as in `engine/src/finite.rs:29` **[Read]** |
| Panic inside a CADCraft command | the engine restores `before` and returns an error **before** the commit point, so the owner is never asked and nothing is appended | `lib.rs:757-771`, interactive `1074-1081` **[Read]** |
| ECAD command fails validation | no transaction, no drawing change | ledger rule TX-03 |
| Ledger apply fails after a CADCraft commit | treated as a refusal | — |
| Failure **after** the ledger accepted a transaction but before projection completes (projector error or panic) | **roll forward**, never back. The accepted transaction stands and the document is authoritative. The engine's drawing and the handle map for every affected page are discarded and rebuilt from the document by a full projection. If that rebuild also fails, the page view is closed with an error; the project stays open, unsaved and saveable, and the page can be reopened. Restoring the previous drawing alone would leave the drawing behind the document | rule PROJ-02 |

**PROJ-02:** once a transaction is accepted, consistency is restored by projecting the document,
never by restoring an older drawing.

**TX-05 (proposed rule):** an ECAD transaction that changes a page's drawing is not applied
while a CADCraft command is in progress on that page, because the command holds its own copy of
the drawing state (`Running.before`, `lib.rs:436-441`). The user is asked to finish or cancel
first.

### 9.3 Changes outside commit points

| Source | Evidence | Treatment |
| --- | --- | --- |
| Pan/zoom inside a paper-space viewport edits the `Viewport` entity | `lib.rs:225-247` **[Read]** | UI state; ignored by the diff |
| `hidden_objects` (ISOLATEOBJECTS) | "never written to a file", `doc/src/lib.rs` **[Read]** | UI state; ignored |
| Header and table edits (current layer, styles) | committed normally **[Read]** | classified as recorded, session-only or refused, from a census of all commands |
| AUDIT/RECOVER replacing the drawing | `cmd/audit.rs:85`, `115` **[Read]** | refused on ECAD pages |

**DOC-08 (adapter rule):** a difference the owner cannot classify is refused, never silently
dropped.

### 9.4 Hypotheses the experiment must test

| # | Hypothesis | If it fails |
| --- | --- | --- |
| **H1** | Ordinary CADCraft commands on ECAD pages can be translated into correct transactions by diffing against the last projection plus engine-reported provenance, without ECAD code per command | add ECAD variants of the failing commands as extension commands, or move translation into the engine per command (a larger patch) |
| **H2** | The projection round trip is lossless for every supported kind, including symbol placement | narrow the supported kinds, or extend the schema |
| **H3** | Moving connected equipment feels normal: wire ends follow at commit and the drag preview shows them | ECAD MOVE variant with its own preview |
| **H4** | A plain COPY or COPYCLIP/PASTECLIP of a whole component maps to an independent copy with preserved internal relationships | ECAD copy commands |
| **H5** | Owner-delegated undo, save, unsaved state and close keep one history and correct prompts across pages and non-graphical edits | larger UI patch |
| **H6** | The extension interface survives upstream merges with small, local conflicts | measured over successive merges, not by the experiment |

## 10. Movement and connectivity

Revision 1 kept a connection while leaving a visibly detached wire end. That is no longer the
normal outcome. The initial policy separates three intents:

| Rule | Intent | Recognised when | Result, in one transaction |
| --- | --- | --- | --- |
| **M1** | **Reroute** | a route changes but both end vertices stay where they were | `ModifyItem` on the route; attachments unchanged |
| **M2** | **Move connected equipment** | a component's terminal positions move (drawn form: its connection points move with its group; symbol form: its placement changes) | the item and connection-point (or placement) ops, **plus** `ModifyItem` moving each attached route end to its terminal's new position; attachments unchanged |
| **M3** | **Deliberate reconnection** | a route end moves while its terminal does **not** (end grip; STRETCH over the end only; moving a wire without its equipment) | resolved **once**, at commit: on a connection point → attach there (`AttachEnd old → new`; nothing if it is the same terminal); in free space → detach (`AttachEnd old → none`); the end then shows as open |

Details:

- **M2 takes precedence** when a terminal moves: an end attached to a moved terminal follows it,
  even if the same command also moved the end some other way. If the wire was moved along with
  its equipment by the same transform (moving a whole circuit), the end is already in place and
  no amendment is needed.
- **Initial follow geometry:** only the end vertex moves; neighbouring vertices stay. Keeping
  orthogonal routes orthogonal (inserting or sliding a dogleg) is deferred product behaviour.
- **Preview:** during a MOVE drag, attached ends should rubber-band with the equipment. CADCraft
  already draws extra preview geometry returned by `Session::drag` (`Drag.extra`,
  `engine/src/prompt.rs:195-203`; drawn by `ui-egui/src/canvas.rs:1192`) **[Read]**. The
  extension interface lets the owner add the moved route ends there **[Assumed]** (H3).
- **Explicit commands:** CONNECT, DISCONNECT and RECONNECT produce the same ops for keyboard,
  scripts and agents.
- **Mismatch state** (attached, but the end not at the terminal) is reachable only through
  explicit ops kept for incomplete work, such as adopted DWG data or a future deliberate "keep
  connection" option. It is reported as a diagnostic. Normal movement never produces it, and
  the experiment checks that.
- An end landing on another conductor's route is a free end with a "possible junction" hint;
  junction rules are deferred.

## 11. Copy, paste and placement

Copy/paste and dragging from a panel are central workflows. The experiment must include copying
of electrical objects; the integration is not proved without it.

### 11.1 Proposed initial meaning

| Selection copied | Result |
| --- | --- |
| Plain items | plain items with new IDs |
| A **whole** component representation (drawn: all its items and connection points; symbol: its instance) | an **independent copy**: new component, terminal, representation, item and connection-point IDs; internal relationships preserved (each copied terminal belongs to the new component; each new connection point maps to the new terminal; properties copied) |
| Conductor routes copied with the equipment at both ends | new conductors, attached to the **copied** terminals |
| Conductor route copied without the equipment at an end | new conductor; that end resolved by M3 at its new position (normally free) |
| A conductor that is a cable member | copied without membership; cables are not copied implicitly |
| Part of a representation (some lines of a symbol) | plain items (an experiment limit, §18.3) |

Copying is never the place where a tag becomes an identity: a copied component keeps the same
tag text, and the duplicate is reported. Tag behaviour on copy is deferred product behaviour.

**Explicitly unresolved, not provided by plain copy:** "another contact of the same component"
(an additional terminal set or representation belonging to the existing component) and
"another depiction of the same contact" (an additional representation of existing terminals).
Each will be a distinct, named command once its semantics are decided.

### 11.2 How the adapter knows what was copied

The commit gives new handles but does not say which original each came from:

- COPY and the copy variants of MIRROR, ROTATE and SCALE create entities in
  `transform_entities` (`modify.rs:101-124`, new handle at 108);
- PASTECLIP gives each clipboard entity a new handle in `adopt` (`clipboard.rs:341`);
- the clipboard keeps copies of the entities, a source document ID and a serial number
  (`clipboard.rs:40-52`).

All of this is **[Read]**. The extension interface therefore adds **provenance** (§14, X6):

- copy-like operations record `new handle ← source handle` for the commit;
- COPYCLIP and CUTCLIP let the owner capture the copied objects' ECAD meaning, keyed by the
  clipboard serial, so a paste still works after the original is edited or deleted, and later
  into another page or project.

New entities that copy linked items but have no provenance are refused (DOC-08), never guessed.
Pairing by matching geometry is a fallback only if upstream rejects the provenance hook.

### 11.3 Placement from a panel

Dragging a symbol from a panel, or placing a library definition, is an ECAD command (Path A):
LIB-01 with a placement. Drag-and-drop from a panel onto the canvas needs a UI hook, deferred
with the panels (§14.2). Until then the experiment places definitions by command.

## 12. Removing representations and deleting objects

| Action | Ops | Subject afterwards |
| --- | --- | --- |
| Remove a representation (CADCraft ERASE of a whole drawn representation or a symbol instance; erasing a route) | `RemoveItem` for its items, `RemoveConnectionPoint`s, `UnlinkRepresentation` | **unchanged**: same ID, attachments, memberships and properties; reported "not shown" (DOC-06) |
| Remove part of a representation (one line of a drawn symbol) | `RemoveItem`, representation item list updated | unchanged |
| Delete a conductor (explicit command) | `AttachEnd … → none` for attached ends, `RemoveCableMember`, then removal of each representation, then `DeleteConductor` | gone |
| Delete a component (explicit command) | detach every conductor end attached to its terminals, remove representations, `RemoveTerminal`s, `DeleteComponent` | gone; conductors that were attached remain, now with open ends |

A future **Delete** interaction may deliberately perform "remove representation + delete
subject" as one transaction. What ERASE and the Delete key do by default on electrical objects
is deferred product behaviour. For the experiment only, ERASE removes representations and a
DELETE command deletes subjects, so both paths can be tested on their own.

## 13. ID mapping lifecycle

The map between ECAD IDs and CADCraft handles (for items, connection-point markers, projection
groups, blocks and inserts) lives only in the session. It is rebuilt whenever a page is projected
and never saved.

| Event | CADCraft handles | ECAD effect | Status |
| --- | --- | --- | --- |
| Open project, project a page | fresh | map built; IDs unchanged | defined |
| New geometry (LINE, PLINE, TEXT, …) | new | `AddItem`, new ID | defined |
| MOVE, ROTATE, SCALE, MIRROR (no copy) | same handle, changed in place (`modify_entity`, `modify.rs:120`) **[Read]** | `ModifyItem` / placement change; M1-M3 | defined |
| STRETCH, grips, property edits | expected to keep the handle **[Assumed]** | as above | census |
| COPY family, COPYCLIP + PASTECLIP | new handles; provenance via X6 | §11.1 | experiment |
| ERASE | handle removed | §12 | experiment uses "remove representation" |
| EXPLODE, BREAK, JOIN on linked items | remove and re-create handles (`modify.rs:1289`, `1319`, `1425`) **[Read]** | **refused for now** | experiment limit |
| TRIM, EXTEND, FILLET on routes | per command **[Assumed]** | same handle → M1/M3; replaced → refused for now | census |
| ECAD undo/redo | restored items may get new handles | IDs unchanged; map updated | defined |
| CADCraft snapshot undo | would restore `handseed` and reissue handles (`doc/src/lib.rs:94`, `154-158`) **[Read]** | not used on ECAD pages | defined |
| GROUP edits on projection groups, BEDIT/REFEDIT of ECAD blocks, AUDIT | various | refused | experiment limit |
| Save, close, reopen; DXF/DWG export | handles discarded or independent | IDs persisted only in the log | defined |

## 14. Generic extension interface

### 14.1 Review against the requirements

Revision 1 aimed for three engine files. The review's requirements are:

- normal copying and connected movement;
- correct unsaved state across pages and non-graphical edits;
- page-versus-project closing;
- safe restore on refusal.

These need more. The proposal is one **generic** interface. It says nothing about components or
wires: a CADCraft document can have an *owner*, and the engine reports to that owner at a few
well-defined moments. Plain drawings have no owner and behave exactly as today.

| # | Capability | Upstream files (expected) | Needed for |
| --- | --- | --- | --- |
| X1 | **Extension commands** installed once by the embedder (like `set_io`, `cmd/file.rs:31-38`) | `engine/src/cmd/mod.rs` | ECAD commands everywhere commands are listed |
| X2 | **Owners**: a session-level registry, and a binding from document to owner | `engine/src/lib.rs` | reaching the project from `fn` commands; owners with no open page |
| X3 | **Commit verdict** at both commit points: accept, accept with amendments, refuse; skip the undo snapshot | `engine/src/lib.rs` | §9, M2 |
| X4 | **History delegation**: `undo`/`redo` hand over to the owner, which may update several documents | `engine/src/lib.rs` (`1347-1368`); UNDO options in `cmd/edit.rs` report "not available" | P4, UNDO-03 |
| X5 | **Persistence delegation**: save/save-as to the owner; `is_dirty` answered by the owner; closing a document asks the owner whether it needs a prompt; quit asks every owner; drawing autosave skips owned documents | `engine/src/lib.rs` (`is_dirty`), `cmd/file.rs` (`qsave`, `saveas`, `close`), `engine/src/autosave.rs` (`184-189`), `ui-egui/src/closing.rs` (`71`, `177`) | SAVE-01..06 |
| X6 | **Provenance**: copy-like operations record `new ← source`; clipboard copy notifies the owner with the clipboard serial | `cmd/modify.rs` (`transform_entities`), `cmd/clipboard.rs` (`copy`, `adopt`), array and grip copy sites after census | §11 |
| X7 | **Drag preview contribution**: the owner adds rubber-band geometry to `Session::drag` | `engine/src/lib.rs` (`1195-1201`) | M2 preview |

**About ten upstream files.** The patch is larger than revision 1's estimate but has one shape,
one marker (`// ECAD-SEAM(owner):`) and one test suite. Each capability is generic enough to
offer upstream (for example, provenance also helps associativity). Closing and quitting
currently ask each document's `is_dirty` directly (`ui-egui/src/closing.rs:71`, `115`, `177`)
**[Read]**, so X5 must touch the UI crate. That is the only UI change the experiment needs.

ECAD semantics stay out of upstream crates: the interface passes drawings, handles, command IDs
and provenance; classification, policy and validation live in `ecad-cadcraft`.

### 14.2 Deferred UI hooks

| Hook | Upstream file | Needed for |
| --- | --- | --- |
| Extra dock panels | `ui-egui/src/docking.rs` (closed `Panel` enum, line 11) | project browser, properties, diagnostics |
| Canvas overlays | `ui-egui/src/canvas.rs` (hard-wired at 1140-1152) | open-end markers, heat map |
| Drop onto canvas | `ui-egui/src/canvas.rs` | drag from panel |
| Top-level menu | `ui-egui/src/menus.rs` (`MENUS`, line 9) | an "Electrical" menu |

### 14.3 Verification

| # | Claim | Now | Proof |
| --- | --- | --- | --- |
| V1 | Separate workspace builds against upstream | **[Shown]** | CI |
| V2 | Extension commands reach every command surface | **[Read]** | drive each surface |
| V3 | Commit points see every committed change after post-command passes | **[Read]** | instrumented count across all commands |
| V4 | Every command's changes on an ECAD page classify (census) | partial | run every `CommandSpec`'s JSON form on a sample page |
| V5 | All undo entry points reach `Session::undo`/`redo` (`cmd/edit.rs:15-23`) | **[Read]** | Cmd+Z, UNDO n, MREDO append ledger transactions |
| V6 | Provenance is complete for copy-like commands | **[Assumed]** | census of entity-creating commands that start from existing entities |
| V7 | Projection round trip (H2) | **[Assumed]** | property test |
| V8 | Diff cost on large pages (pointer comparison of `Arc<Entity>` chunks, `doc/src/store.rs`) | **[Assumed]** | benchmark at 10k items |
| V9 | ECAD crates build for `wasm32` | **[Assumed]** | `ecad/xtask wasm` |

## 15. Incorporating upstream updates

1. `upstream` = `https://github.com/storytold/cadcraft`, fetch only.
2. Cut `upstream-sync/<date>` from `dev` and **merge** `upstream/main` (never rebase).
3. Resolve conflicts in inventory files, keeping `ECAD-SEAM` blocks; keep our `README.md`.
4. Run upstream's gates on the root workspace, then the ECAD workspace, including V2-V8 and the
   experiment's scenario tests.
5. **Patch guard:** upstream paths changed relative to `upstream/main` must equal
   `ecad/UPSTREAM-PATCHES.md`.
6. PR into `dev`; merge only after your approval.
7. Offering the interface upstream is outward-facing and needs your go-ahead.

## 16. Optional DWG boundary

- ECAD exports build plain CADCraft drawings from the projection and write DXF through
  `cadcraft-io`. DWG export and adopt sit behind an ECAD `dwg` feature.
- **Constraint [Read]:**
  - `cadcraft-io` depends on `cadcraft-dwg` unconditionally (`crates/io/Cargo.toml:16`);
  - `cadcraft-engine` depends on `cadcraft-io` (`crates/engine/Cargo.toml:28`);
  - so every native build links `acadrust` (MPL-2.0). WASM excludes it already (`crates/dwg/Cargo.toml`).
- A DWG-free native build needs one of: (a) accept `acadrust` unmodified; (b) an upstream patch
  making it optional; (c) a build-time stub for `cadcraft-dwg` **[Assumed]**, untested.
  Licence acceptance stays open.

---

# Part C: integration experiment

## 17. Bounded integration experiment

Specified for approval; **not implemented or run**. Its purpose is to prove or disprove H1-H5
with the smallest code that exercises real CADCraft commands.

### 17.1 Scope

- `ecad/` skeleton.
- `ecad-model` in memory.
- A test-only store with an injectable write failure. This is not the `.ecad` encoding.
- The X1-X7 patches.
- `ecad-cadcraft`: projector, handle map, owner and commit translation.
- Minimal commands: CONVERTTOCOMPONENT, ADDTERMINAL, WIRE, CONNECT, DISCONNECT, DELETE, SETPROP,
  CABLEADD and PLACE (by command, from one embedded definition).
- Two pages in one project, driven headlessly through `Session` exactly as the UI and control channel drive it.
  Plus one manual desktop run, with screenshots, for the preview and closing prompts.

### 17.2 Scenario

One project with two pages, `p1` and `p2`. The drawn component `c1` has terminals `t1`-`t3`. The
symbol instance `c2` has terminals `s1` and `s2`.

| Step | Action (real CADCraft command where possible) | Expected |
| --- | --- | --- |
| S1 | On `p1`: LINE rectangle, TEXT label | `AddItem`s; plain geometry |
| S2 | CONVERTTOCOMPONENT, ADDTERMINAL ×3 | drawn representation of `c1`; connection points `t1`-`t3` |
| S3 | PLACE a symbol, rotated 90° | `EmbedDefinition`, instance `c2`, terminals `s1`/`s2` from definition keys; terminal positions match placement |
| S4 | WIRE `w1` from `t1` to `s1`; WIRE `w2` from `t2` to a free point; WIRE `w3` from `t3` to a free point | `w1` attached at both ends; `w2` and `w3` each with one open end |
| S5 | Grip a middle vertex of `w1` | M1: route only |
| S6 | MOVE `c1`; ROTATE `c2` | M2: attached ends follow in the same transaction; no mismatch; preview shows following ends |
| S7 | Grip `w3`'s free end onto `s2`; then grip it from `s2` into free space | M3: attach, then detach; drawing and intent agree. `w2` is not touched and stays attached to `t2` |
| S8 | COPY `c1` together with `w2`; then COPYCLIP/PASTECLIP `c2` | independent copies: all new IDs, internal relationships preserved, `w2`'s copy attached to `t2`'s copy; originals unchanged |
| S9 | ERASE `w1`'s route; then DELETE conductor `w2` | `w1` remains, "not shown", still attached; `w2` gone with its representation and attachments |
| S10 | NEWPAGE `p2`; on `p2` draw a LINE and a WIRE `w4`; CABLEADD `w3` and `w4` (members on two pages); SETPROP tag on `c1`'s copy (no drawing change) | `p2` projected as a second CADCraft document; the unsaved marker shows on **both** page tabs; head ≠ persisted |
| S11 | With `p1` active, UNDO twice | the SETPROP and the cable, which concern `p2` objects, are undone (project-wide history); `p2`'s drawing is updated though it is not active |
| S12 | Close the `p2` page tab; reopen it | no prompt; project still unsaved; reopened page equals the projection |
| S13 | Save; make a **new** unsaved edit (SETPROP); save with an injected write failure; save again | clean after the first save; unsaved after the edit; still unsaved after the failed save, with the error reported and the file still holding exactly the first save's transactions; clean after the third save |
| S14 | Undo everything back to an empty project, then redo everything | after each undo and redo, the **document state equals the state recorded for that point**, object IDs are identical and every page's drawing equals its projection. The log **grows** by one transaction per undo or redo; no transaction is removed |
| S15 | Reopen from the store, with and without checkpoints | equals the live state |
| S16 | Inject a projector failure into the next accepted transaction (a MOVE of `c1`); then inject failures into both the projection and the rebuild | first case: the transaction stays in the ledger, the document holds the move, and drawing and map are rebuilt from the document and consistent. Second case: the page view closes with an error; the project stays open, unsaved and saveable, and reopening the page shows the move |

### 17.3 Refused and cancelled cases

| Case | Expected |
| --- | --- |
| Esc during MOVE of the component | no transaction; drawing and map unchanged |
| Esc during LINE after two segments | the two segments recorded, as CADCraft keeps them |
| EXPLODE the symbol; BREAK a route; HATCH on the page | refused with a message; drawing restored to the last projection; ledger, map and unsaved state unchanged |
| CONNECT to a terminal deleted earlier in the same session | validation error; nothing appended |
| Injected panic in an ECAD command and in an owner callback | engine guard restores; nothing appended (or, after acceptance, PROJ-02 applies); next command works |
| A SETPROP while a CADCraft MOVE is running | refused per TX-05 |
| Closing a page tab with unsaved changes; then closing the project | tab closes without prompting; project close prompts; Save failure keeps it open |

### 17.4 Checked after every step

1. Projecting the document equals the engine drawing (H2).
2. Replaying the log equals the document.
3. The map is one-to-one and every mapped handle exists.
4. The CADCraft undo stack of the owned page is empty.
5. Unsaved state equals (head ≠ persisted position).
6. Derived connectivity equals stored attachments; no mismatch state unless deliberately created.

### 17.5 Exit criteria and report

The experiment passes when S1-S16 and §17.3 pass, with checks 1-6 after every step, on Linux,
and the ECAD crates build for `wasm32`. The report records:

- which hypotheses held;
- the final list of upstream files and lines changed (an outcome, not a target);
- every command the census refused, and why;
- the fallbacks needed for any hypothesis that failed.

---

# Part D: decisions

## 18. Decision table

### 18.1 Architectural direction supported by this review

Recorded as direction; no further approval needed.

| # | Direction |
| --- | --- |
| A1 | Separate `ecad/` folder and Cargo workspace, path dependencies on upstream crates |
| A2 | ECAD-owned authoritative document and history (P1, P4); CADCraft drawings are projections |
| A3 | Stable typed identity independent of names (P2) |
| A4 | Project-wide atomic transactions with resolved intent (P3) |
| A5 | An adapter around CADCraft; document, history and library crates free of CADCraft (P10) |
| A6 | Meaning separate from representation (P6); incomplete designs valid (P7) |
| A7 | History preserved; no destructive compaction; checkpoints disposable (P9, FILE-04, FILE-05) |
| A8 | Versioning as in §8.1: `0.1.0-dev`, `ecad-v…` release tags, CADCraft commit tracked separately, format versions independent |
| A9 | Revision 2 approved as the basis for the bounded experiment; Part B stays provisional until demonstrated |

### 18.2 Revised recommendations requiring approval

| # | Recommendation | Replaces |
| --- | --- | --- |
| R1 | Generic extension interface X1-X7 (§14), about ten upstream files, one marker, offered upstream later with your go-ahead | revision 1's "three seams" (D3) |
| R2 | Commit translation by diff against the last projection, plus provenance, as the **hypothesis to test** (H1), not a settled design | D1's second half |
| R3 | Two representation forms, drawn and symbol, in an ECAD-owned schema with explicit placement; projected as groups and as blocks with inserts (§4) | D4 |
| R4 | Removing a representation never deletes its subject; deletion is explicit and may be bundled (§12) | D6 |
| R5 | Movement policy M1 reroute / M2 connected equipment / M3 deliberate reconnection (§10) | D7 |
| R6 | A plain copy of a whole component is an independent copy with new IDs and preserved internal relationships (§11.1) | D8 |
| R7 | Unsaved = head ≠ persisted position; page tabs are views; the project closes and prompts; save failure keeps state (§6.3) | the pointer-based dirty check |
| R8 | IDs: 128-bit random, allocated at edit time, never reused | D5 (unchanged) |
| R9 | Linear, project-wide undo derived from the log | D9 (unchanged) |
| R10 | Unclassifiable differences are refused (DOC-08) | D10 (unchanged) |
| R11 | Housekeeping: ECAD docs to `ecad/docs/`; ECAD rewrite of root `CLAUDE.md`/`AGENTS.md`; disable inherited release and packaging workflows | D11 (unchanged) |
| R12 | One ECAD page = one CADCraft document for Phase 1 | D12 (unchanged) |
| R13 | Approve the experiment in §17 as the next implementation step (as PRs into `dev`) | new |

### 18.3 Temporary experiment limitations

These apply to the experiment only. Each is to be lifted or turned into a product decision
afterwards.

| # | Limitation |
| --- | --- |
| L1 | EXPLODE, BREAK, JOIN (and TRIM/EXTEND/FILLET that replace handles) are refused on linked items |
| L2 | Copying part of a representation produces plain items |
| L3 | Cables are not copied; copied members lose membership |
| L4 | Copy and paste within one page and project only; cross-project paste later |
| L5 | ERASE removes representations only; a separate DELETE command deletes subjects |
| L6 | Kinds outside the schema (hatch, dimensions, tables, arbitrary block inserts, MINSERT, non-uniform scale) are refused on ECAD pages |
| L7 | Two pages at most in the scenario; no autosave or crash journal; UNDO Mark/Back/BEgin/End unavailable |
| L8 | Placement by command; no drag-and-drop from panels |
| L9 | A test-only store, not the `.ecad` encoding |
| L10 | ECAD edits refused while a CADCraft command is running on the page (TX-05) |

### 18.4 Deferred product behaviour

Each item names the constraint to preserve. None is decided by this proposal.

| Subject | Constraint preserved |
| --- | --- |
| Another contact of the same component | a distinct command; never what plain copy does |
| Another depiction of the same contact | a distinct command; adds a representation of existing terminals |
| What ERASE and Delete do by default on electrical objects | representation removal and subject deletion stay distinct in the model |
| Tag on copy; tag scope; "same tag means same component" | tags are properties; duplicates reported, never merged |
| Wire identification and numbering | identifiers are properties, possibly per end, repeated or hidden |
| Net identity, junctions, terminal bridges | nets derived; only attachments stored |
| Orthogonal tidy-up when ends follow equipment | M2 records the resulting geometry explicitly |
| Whether closing the last page closes the project | SAVE-05 holds either way |
| Crash recovery journal | FILE-03 durability unaffected |
| Compaction, checkpoint authority, named-revision preservation | P9 until a preservation contract is approved |
| Inference versioning | stored intent never changes with rules |
| Library replacement and terminal mapping; parts container | LIB-02 to LIB-04 |
| Promoting a drawn representation to a symbol | component and terminal IDs kept |
| Cable view persistence; a conductor in several cables | cable membership changes no conductor |
| DWG licence; product name; default drawing standards | §16 lists the DWG options |

## 19. Room for later features

| Feature | Where it fits |
| --- | --- |
| Cable demonstration | cable representations (a slash or bundle is a representation of a cable); the view toggle is UI state |
| Numbering | `SetProperty` on conductors and ends, plus derived suggestions |
| Completeness heat map | `ecad-derive` over `Unset`/`Set`, drawn through the overlay hook |
| MCP | the same transactions with an agent as author; proposals are transactions held outside the log until accepted; release is human-only |
| Library replacement | `UpdateDefinition` + terminal remapping transaction |
| DWG adopt | an import that emits ordinary transactions, then a review |

## Revision 2 changes

| Review point | Change |
| --- | --- |
| 1. Principles versus integration choices | Parts A-D; principles P1-P10; hypotheses H1-H6 |
| 2. Representation versus subject | P6, DOC-06, DOC-07, §12; revision 1's D6 withdrawn |
| 3. Movement and connectivity | §10 rules M1-M3; mismatch only by explicit op; revision 1's D7 withdrawn |
| 4. Copy restrictions | §11 independent copy and provenance; remaining limits are L1-L4; two copy meanings left unresolved |
| 5. Dirty tracking | §6.3 SAVE-01 to SAVE-06; X5 |
| 6. Symbol representation | §4 drawn and symbol forms, placement, terminal positions, reproduction |
| 7. Compaction | P9, FILE-04, FILE-05 |
| 8. Experiment | §17 |
| Engine changes | §14 generic interface X1-X7 replaces three seams |

Corrections carried from revision 1:

- `EntityKind` has 30 variants (29 kinds plus `Unknown`, `doc/src/entity.rs:876`), not 25 as
  the Phase 0 assessment said;
- an ECAD-level DWG feature cannot keep `acadrust` out of native builds (§16).

## Revision history

| Date | Change |
| --- | --- |
| 2026-10-12 | Revision 1: first proposal |
| 2026-10-12 | Revision 2: review of PR #1; principles separated from hypotheses; representation/subject, movement, copy, unsaved state, symbols, compaction and experiment revised |
| 2026-10-12 | Revision 3: experiment corrections (S7/S8 conductors, two pages, undo/redo verifies state and IDs as the log grows, failure after acceptance PROJ-02, failed save after a new edit); versioning recorded (§8.1) |
