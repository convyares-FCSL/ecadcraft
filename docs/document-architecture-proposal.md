# ECAD document architecture and integration proposal

> **Status:** revision 4 draft. Revision 2 was approved as the basis for the bounded integration
> experiment (§17); the integration choices in Part B stay provisional until demonstrated.
> Revision 4 adds **Part E** (persistence and the live debugger) **for review; nothing in it is
> implemented**. **Written:** 2026-10-12 against `dev`
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
- **Part E, persistence and live inspection** (revision 4 draft, for review). It sets out the
  `.ecad`/`.ecadlib` container, how embedded definitions are stored, durability, recovery,
  versioning and checkpoints, and a read-only live debugger panel, with acceptance criteria.
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

**Part E: persistence and live inspection (revision 4 draft, for review)**

20. [Persistence design](#20-persistence-design)
21. [Live debugger (developer panel)](#21-live-debugger-developer-panel)
22. [Plan and acceptance criteria](#22-plan-and-acceptance-criteria)
23. [Part E decisions for the owner](#23-part-e-decisions-for-the-owner)

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
  Replay never allocates. Users see tags and labels; diagnostics show a short form.
- **One copy policy:** every copy or paste, within a project or between projects, allocates new
  IDs for everything it creates (§11). No object keeps its ID across a copy. The benefit of
  random IDs is that IDs from independent sessions do not collide, not that pasted content keeps
  its IDs.
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
| Placement | implicit in the items' coordinates | explicit: origin (x, y), rotation (radians, stored), mirror (yes/no), uniform scale (> 0) |
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
where `M` negates x when mirrored, and `rotation` is in **radians**, counter-clockwise. These are
our own stored values.

**Units at the boundaries.** The document stores radians: the same unit as CADCraft's insert
rotation, so the round trip is exact (§4.3; the experiment showed that converting through degrees
would not be). People think in degrees, so the conversion happens only at the edges:

- commands that take an angle from people or scripts (PLACE `rotation`, and later rotate dialogs
  and property panels) accept **degrees** and convert once, when the edit is made;
- the converted value is what the op records, so replay never converts again (TX-01);
- displays and reports show degrees, derived from the stored radians;
- the stored value is never rounded to a "nice" number of degrees. They map onto CADCraft's
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

These contracts state what each file owns and promises. The encoding must meet them; Part E
(§20) proposes how.

### 5.1 `.ecad` project file

**Owns:** the project's transaction history, every embedded definition it uses, named versions
as bookmarks, and optional checkpoints as a cache.

- **FILE-01 Self-contained** (P8).
- **FILE-02 Deterministic replay.** Replay applies recorded values and IDs; it never snaps,
  searches, infers attachments or allocates IDs.
- **FILE-03 Durable append.** Saving appends whole transactions and then flushes them to disk
  (`fsync`). A torn trailing write is ignored on open.
- **FILE-04 Checkpoints are caches.** Deleting every checkpoint changes nothing but opening
  speed. This holds **unconditionally** in the initial implementation (P9). Part E proposes
  optional checkpoint records inside the file, validated before use and never authoritative
  (§20.8).
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
| L9 | A test-only store, not the `.ecad` encoding (Part E proposes the real one) |
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

# Part E: persistence and live inspection (proposal for review)

> **Status: revision 4 draft, for review. Nothing in Part E is implemented**, and no persistence
> or debugger code is written until the owner approves it (§23). Part E says how to meet the
> contracts of §5 and §6; it does not change them, except where §5 now points here.
> **Owner's direction (2026-10-12):**
> - Use JSON for the initial development implementation, because it is directly readable.
> - Keep encoding separate from document semantics so other formats can be evaluated later. Do
>   not benchmark alternatives or optimise storage at this stage.
> - "One file" does not mean one JSON object; JSON records do not imply ZIP.
> - Add a minimal live, read-only Rust debugger alongside persistence.

## 20. Persistence design

### 20.1 Three layers

```
ecad-model (memory)     Document, Ledger, Transaction, Op                         changes freely
     │ explicit conversion to and from versioned records (+ upcasts of old versions)
     ▼
1. semantic records     header, tx (with ops), checkpoint — schema version 1
     │ codec: record ⇄ bytes, and framing        JSON Lines now; another codec later
     ▼
2. encoded container    one frame per record, each with a frame checksum
     │ medium: append, flush, truncate, atomic replace, lock
     ▼
3. storage              file on disk; in-memory medium with fault injection for tests
```

- **The semantic records are the format.** They are separate, versioned types (in a new
  `ecad-file` crate) with explicit conversions from `ecad-model`. Today's test store serialises
  `ecad_model::Transaction` directly. That would let a model refactor silently change the file
  format. Explicit records make every format change a decision with a version number. Inspector
  snapshots keep using the model's own serialisation, because they are diagnostics, not
  projects.
- **The codec is replaceable.** It is a trait: encode a record, decode a record, frame it.
  JSON Lines is the only codec now. A conformance suite (golden files decode to identical
  records and identical history hashes) is the gate for any later codec. No benchmarking is
  planned in this stage.
- **The medium holds the durability rules,** in one place. The in-memory medium exists so every
  failure point can be injected in tests (as in today's test store).

### 20.2 Container layout: options

A single file is required (FILE-01, P8) and must hold three distinguishable kinds of record:
- transactions;
- embedded definitions, as content inside transactions or as records of their own (§20.3);
- disposable checkpoints.

| | **A. Typed record stream** (recommended) | B. ZIP-style package | C. Main file plus sidecar files |
| --- | --- | --- | --- |
| Shape | one file: a header record, then records each tagged `"type"`: `tx`, `checkpoint` (and `def` if separate definition records are chosen) | archive with members (`history.jsonl`, `checkpoints/…`, `defs/…`) | `.ecad` holds the history; checkpoints in `name.ecad-cache` |
| Append-only save (FILE-03) | natural: append records, flush | poor: the central directory must be rewritten or re-appended on every save; a torn write leaves an unreadable archive | natural for the main file |
| Torn-write recovery | the longest valid prefix of records | depends on archive tooling | main file as A; the cache is simply discarded |
| Readable with a text editor | yes, line by line | needs unzipping | yes |
| Self-contained | yes | yes | the history yes; checkpoints separate, which is acceptable for a cache but means two files to copy for full speed |
| Random access, compression | needs an offset index (built on open) | built in | as A |

**Recommendation: A.**
- **What it is:** a JSON Lines file of typed records.
- **Why not B:** ZIP solves random access and compression, which are not problems yet. It
  makes durable append and torn-write recovery harder, which are problems now.
- **C is rejected for now:** A can hold checkpoints as ignorable records (§20.8), so a second
  file is not needed.

The record framing (§20.11) leaves room to move to a binary framing later without changing the
records.

Records in container version 1:

| Type | Count | Purpose |
| --- | --- | --- |
| `header` | first, exactly once | `container` (`"ecad-project"` / `"ecad-library"`), `container_version`, `schema_version`, `codec` (`"jsonl/1"`), `project_id` / `library_id`, `created_ms`, `writer` (product and CADCraft commit) |
| `tx` | one per transaction, in order | `seq`, `id`, `kind` (`"edit"`, `{"undo": id}`, `{"redo": id}`), `author`, `time_ms?`, `label`, `ops: [{"op": name, "v": n, …}]`, `prev` (history hash, §20.10) |
| `checkpoint` | zero or more, never authoritative | `at_seq`, `history_hash` at that seq, `state_hash`, `document` (§20.8) |

A reader ignores nothing silently: any other `type` is refused unless a later container version
marks it as ignorable (§20.7).

### 20.3 Embedded library definitions

Requirements, all from the owner and LIB-01 to LIB-05:

1. **Embed once.** A definition is embedded once, and many component instances reference it.
2. **Independent identities.** Each instance (component and representation) and each of its
   terminals has its own ID.
3. **Versions coexist.** Different versions of a definition can coexist in one project.
4. **Introduced by history.** Introducing embedded content is recorded in the authoritative
   history.
5. **Old revisions keep their definitions.** The original definitions needed by historical
   revisions are preserved.
6. **No library needed.** The project opens without access to the external library.
7. **Nothing mutable outside history.** No symbol data can be changed except through history.

Two ways to store the content:

| | **I. Inline in the introducing transaction** (recommended) | II. Separate immutable definition records |
| --- | --- | --- |
| How | `EmbedDefinition { id, definition }` carries the full content inside the `tx` that introduces it (as the model does today) | a `def` record (content plus content hash) is written before the first `tx` that uses it; `EmbedDefinition { id, content_hash }` references it |
| 1. Embed once | yes: one `DefinitionId`, referenced by every `SymbolInstance` | yes |
| 2. Independent identities | yes: instances and terminals get their own IDs in the placing transaction (LIB-02) | same |
| 3. Versions coexist | yes: each embedded version is its own `DefinitionId` with its own `origin`; instances point to the one they were placed from | yes, plus identical content is shared automatically |
| 4. Introduced by history | yes: the `tx` *is* the introduction | the `tx` is the introduction; the `def` record is its payload |
| 5. Historical revisions | yes: no history is ever dropped (P9), so `state_at(n)` replays the content | yes, as long as `def` records are never dropped |
| 6. No library needed | yes | yes |
| 7. Nothing mutable outside history | yes: content exists only inside ops | yes, but only if `def` records are immutable and garbage is never collected; that adds a rule to enforce |
| Reader complexity | one record type | ordering rule (definition before use), presence check per reference, orphaned `def` records after a torn save |
| Size | content repeats when the embedding is undone and redone (inverse ops carry the full value) and in checkpoints | content stored once per file |

**Recommendation: I.** Separation's only concrete gain is size under undo/redo of an embedding
and in checkpoints. That is a storage optimisation the owner has asked us not to make yet, and
it costs a second record type with its own integrity rules. Revisit only if real projects show
the repetition matters. Moving to II later is a container version change, with conversion on
Save As.

Further rules for embedded definitions:
- **Identity of content.** A definition's identity is its `DefinitionId`. Its content hash
  (SHA-256 over the canonical form, §20.10) is computed, never trusted from the file. It is
  recorded in `origin` for libraries (§20.9).
- **No duplicate content.** Placing a definition whose origin and content hash are already
  embedded reuses that `DefinitionId` (LIB-01). New content, such as a newer library version,
  becomes a new `DefinitionId`.
- **Updating instances.** Moving instances to another version is an explicit, undoable
  transaction (LIB-03). It is a later design; the format needs nothing new for it.
- **Removing definitions.** `RemoveDefinition` is only valid while no item or component
  references the definition. The model's invariant check already enforces this. Removal is
  history like everything else, so earlier revisions still contain the content.

### 20.4 Transaction boundaries

- **One transaction per accepted command.** Every accepted command, ECAD command, undo or redo
  is exactly one `tx` record (§9.1, UNDO-01). A refused or cancelled command writes nothing
  (§9.2).
- **Atomic and validated before it exists.** A transaction is applied to a copy and checked
  (TX-03). Only then is it appended to the in-memory log and given its `seq`. Its record is
  written at the next save.
- **Pending versus durable.** The in-memory log is ahead of the file between saves. A
  transaction is **durable** only when its complete record is in the file and the file has
  been flushed. `persisted` is the highest durable `seq`.
- **A save is a batch.** Saving appends records `persisted+1 … head`, then flushes once. Each
  record stands alone and chains to the previous one, so any prefix of a batch is a valid
  history.

### 20.5 Durable saving

- **Save** (file already holds this project up to `persisted`):
  1. Check the file is unchanged since the last durable write: same length, same modification
     time.
  2. Truncate to the last durable length (removing any earlier torn tail).
  3. Append the pending records.
  4. Flush (`sync_data`).
  5. Then advance `persisted` (SAVE-04).

  If any step fails, `persisted` and the in-memory log are untouched, the error names the
  file and the step, and the project stays unsaved.
- **Save As, or first save:**
  1. Write the header and every transaction (FILE-05; checkpoints optional) to a temporary file
     in the target folder.
  2. Flush it and rename it over the target.
  3. Flush the folder where the platform allows.

  A failure never damages the existing target. `project_id` is kept: Save As is a new location,
  not a new project.
- **Writer lock.** Opening a project for writing takes an OS file lock, released when the
  process ends. A second opener gets the project **read-only** and is told why.
- **File changed externally** (length or modification time differs from the last durable
  write): Save refuses and offers Save As. Nothing is overwritten blindly.

### 20.6 Recovery from interrupted writes

On opening:
1. Check the header: container, versions, codec.
2. Read records in order, checking for each:
   - the frame checksum;
   - that `seq` is consecutive;
   - that `prev` links to the previous record (§20.10);
   - that every op decodes, upcasting old versions.
3. Replay through the ledger, which checks preconditions and invariants after every
   transaction (TX-03, TX-04).

| Finding | Result |
| --- | --- |
| Everything valid | opens writable; `persisted = head` |
| Last record incomplete, unterminated or failing its checksum (an interrupted save) | that record is ignored (FILE-03) and reported; opens writable at the valid prefix; the tail is truncated at the next save |
| A bad record, broken chain, failed precondition or failed invariant **before** the last record | opens **read-only** at the last good transaction, with a report naming the first bad `seq`; Save As can write the good history to a new file; nothing is written to the damaged file |
| Unknown or newer `container_version`, `schema_version`, `codec`, record type, op or op version | refuses to open, naming what this build supports (§20.7) |

A crash between saves loses the pending transactions. That is the reason for the "unsaved"
marker. The crash journal stays deferred (§18.4); it would be a separate design.

### 20.7 Format versioning

The four version axes are independent of each other and of the product version (A8):

| Axis | Covers | Rule |
| --- | --- | --- |
| `container_version` | record types, framing, header fields | a reader refuses newer values |
| `codec` | how records become bytes (`jsonl/1`) | a reader refuses codecs it lacks |
| `schema_version` | the meaning of record contents | a reader upcasts older versions and refuses newer ones |
| op `v` | one op's fields | a reader upcasts each older op version on read (FILE-06) |

- **Writing a different version:** the writer always writes the current versions. A file in an
  older schema version opens with its records upcast in memory. Appending records of a
  different version to it would mix versions in one file, so its **first save rewrites the
  whole file** in the current version. This uses the Save As procedure, keeps a copy of the
  original (`name.ecad.v1.bak`) and asks first. Every transaction is preserved, so this is a
  conversion, not compaction (E4).
- **Strict reading:** unknown fields, records, ops and op versions are refused, not ignored, so
  an older build never drops data silently (E5). Revisit before the first release that must
  read files from newer builds.

### 20.8 Checkpoints

- **What they are for:** disposable caches that only make opening faster (FILE-04, P9). They
  are **never written automatically in stages 1-3**. Replay is fast at development sizes, and
  that avoids premature optimisation.
- **Format:** a `checkpoint` record holds:
  - `at_seq`;
  - the history hash at that seq;
  - a state hash of the canonical document;
  - the document (all stored state, including embedded definitions).

  It always follows the `tx` record it covers.
- **Using one:**
  1. A reader may load the newest checkpoint whose `history_hash` matches the verified chain.
  2. Its `state_hash` must match its document.
  3. Only transactions after it are replayed.
  4. A checkpoint failing either check is **ignored**, with a diagnostic. It never blocks
     opening and never replaces history.
  5. The full-replay check stays available as an explicit "verify" action (§21).
- **Removing them:** Save As without checkpoints writes the same history with no checkpoints.
  Removing them changes only opening speed. History is never dropped (A7, FILE-05).

### 20.9 Library files (`.ecadlib`)

- **Container:** the same container and rules as projects, with `container: "ecad-library"`,
  a stable `library_id` that survives renaming the file, and library ops:
  - `AddDefinition`;
  - `ModifyDefinition { before, after }`;
  - `RemoveDefinition`;
  - `SetLibraryProperty`;
  - `NameVersion`.
- **Versions:** a definition's version is the `seq` of the library transaction that last
  changed it. Its content hash is computed from its content.
- **Placing into a project:**
  1. The project's `EmbedDefinition` records `origin { library_id, definition_id, library_seq,
     content_hash }` and the full content (§20.3).
  2. The project never refers to the library file again (P8).
  3. "Update available" is a derived comparison of hashes (P5).
- **Diverged copies:** two library files with the same `library_id` but diverging histories are
  told apart by their hash chains.
- **Still open from §5.3:** parts versus symbols, terminal mapping on replacement, library
  editing UI. None of these blocks the container.

### 20.10 Integrity

- **Frame checksum.** Encoding level: CRC-32C of a record's bytes. It catches torn and damaged
  records.
- **History hash chain.** Semantic level:
  - each `tx.prev` is the SHA-256 of the canonical form of the previous transaction;
  - transaction 1 chains to the header's `project_id`.

  It catches missing, reordered or substituted transactions, and diverging copies. It
  identifies named versions and checkpoints, and survives a later change of codec.
- **Canonical form.** A record's semantic content serialised by the JSON Canonicalization
  Scheme (RFC 8785), excluding the frame checksum. It is used only for hashing. Content hashes
  of definitions use the same form.
- **Dependencies:** both algorithms have small, permissively licensed Rust implementations, to
  be recorded in the ECAD dependency notes.

### 20.11 JSON Lines (`jsonl/1`)

- **Encoding:** UTF-8, one record per line, `\n` terminated, no byte-order mark.
- **Deterministic:** fixed key order, and floats in shortest round-trip form.
- **Frame checksum:** a trailing `"crc"` member, computed over the line without it.
- **Content rules:**
  - IDs are 32 hex digits;
  - angles are in radians;
  - lengths are in the project unit (§20.13);
  - only finite numbers are allowed;
  - an absent property means unset (P7).
- **Readable:** each line is readable as it stands. `ecad-cli dump` pretty-prints a file and
  `ecad-cli check` verifies it.
- **Hostile input:** reading treats the file as hostile, with caps applied **before**
  allocation on:
  - line length (initially 64 MiB);
  - transactions per file;
  - ops per transaction;
  - vertices per polyline;
  - string length.

  Tests include a fuzz target and golden files for every released schema version.

### 20.12 Code structure

| Crate | Layer | Role |
| --- | --- | --- |
| `ecad-file` (new) | Core | records, conversions and upcasts, canonical form and hashes, the codec trait and `jsonl/1`, reader and writer over a medium, caps; no I/O of its own; builds for wasm |
| `ecad-store` | Core | the `Medium` trait, the in-memory medium with fault injection, the file medium (native only; append, `sync_data`, atomic replace, lock); the current test store retires once `ecad-file` replaces it |
| `ecad-cadcraft` | Adapter | `Persistence::File` beside `TestStore` (tests) and `Unavailable` (the current trial); open, save, save as; read-only projects |
| `ecad-cli` (new, small) | UI | `dump`, `check` |
| `ecad-trial` | UI | Open, Save and Save As, the debugger panel (§21); its "no persistence" label changes only when stage 2 lands |

### 20.13 Model additions this needs

- `ProjectSettings { length_unit }`, recorded at creation by `SetProjectSetting` (architecture
  §3.2 lists units; the model lacks them).
- `NameVersion` (§6.1 lists it; the model lacks it).
- A typed definition `origin { library_id, definition_id, library_seq, content_hash }`, which
  replaces today's free-text `library`, `definition` and `version`.

## 21. Live debugger (developer panel)

**Purpose.** A developer can follow one action through four stages:
1. a drawing interaction (selected entities, CADCraft handles);
2. its electrical meaning (mapped ECAD IDs and relationships);
3. the transaction that accepted it (ops, before and after);
4. the bytes on disk (record offset, size, durable or pending).

The debugger is read-only and live. The static HTML inspector remains for exported diagnostics.

### 21.1 Where and how

- **Where:** a dockable developer panel in `ecad-trial`, written in Rust with egui, opened from
  the banner. It is not a product UI and is never shown by default.
- **Data sources** (all existing interfaces; no new model access paths):

  | Source | Provides |
  | --- | --- |
  | `ProjectHandle::read` | the project, briefly locked |
  | `ecad_inspect::snapshot` | objects, relationships, labels, history, checks |
  | `EventLog` | runtime diagnostics |
  | `HandleMap` | handle ⇄ ID |
  | `verify_page` | recovery-style check |
  | `ecad-file` writer status (new) | path, versions, sizes, record index, last write and flush results |

- **Coherent snapshots:**
  - The panel keeps one `DebugSnapshot` labelled with `seq`, built by copying what it shows
    under a short `try_lock`.
  - It is rebuilt only when `head`, `persisted`, the event count or the selection changes, or
    on Refresh. It is **never** rebuilt every frame.
  - Rendering reads only the snapshot and never holds the project lock.
  - If the project is busy, the previous snapshot stays on screen, marked "stale".
- **Expensive work is explicit.** Full replay (`replay_matches`), projection verification,
  re-reading the file and checking the hash chain run only from **Verify** buttons, and show
  their result with the `seq` and time they ran at.
- **History without side effects.** Choosing an earlier `seq` reconstructs that state with
  `Ledger::state_at` into a separate value, computed on request. The live document, head and
  pages are untouched. Historical views are labelled "history at seq n".

### 21.2 Views

| View | Shows |
| --- | --- |
| **Selection** | selected entities: CADCraft handle, entity kind and geometry summary; mapped ECAD ID (item or terminal marker); its representation, subject, terminals; readable JSON of the ECAD item |
| **Model** | components, terminals, conductors (ends and attachments), cables (members), representations (form, page, items, connection points); every ID links to its relations; search by ID, short ID or label |
| **Definitions** | embedded definitions with origin, version, content hash, and the instances (components, symbol items, pages) that reference each |
| **History** | transactions (seq, kind, author, label, time), each op with readable before and after JSON; **pending** (seq > persisted) and **durable** (seq ≤ persisted) shown distinctly |
| **Persistence** | file path, container, schema and codec versions, size on disk, last durable length, head versus persisted, pending count; per transaction: record offset and length once durable, and the record as stored (read from the file on request, pretty-printed); last write, flush, rename and lock results, including failures |
| **Diagnostics** | the runtime event log (accepted, refused, saved, save failed, recovery, …), kept visibly separate from design history, each correlated to command and seq |
| **Checks** | the Verify buttons and their results |

**The trail.** Selecting a transaction shows:
- the command and event that produced it;
- the ops and their before and after values;
- the objects touched (linked to Model and Selection);
- its record, with "pending (in memory only)" until a save makes it durable, and then the file
  offset and bytes.

### 21.3 Boundaries

- **Read-only.** The panel calls no command and no session method that edits. It gets no
  `&mut Project`; it uses `ProjectHandle::read` only.
- **No server and no network.** The live view is in-process. Exporting a snapshot for the HTML
  inspector stays as it is. The snapshot format gains persistence facts (a format bump to
  `ecad-inspect/2`) so exported files show durable versus pending too.
- **Bounded.** Large lists are paged, and record bytes are read on request with a size cap.

## 22. Plan and acceptance criteria

Each stage is one PR into `dev` and starts only after approval. Nothing in a later stage is
built early.

**Stage 1: records and JSON in memory** (`ecad-file`, model additions from §20.13)
- The round trip model → records → bytes → records → model reproduces the identical document,
  log, IDs and head, for golden projects and generated ones.
- The writer gives byte-identical output for identical records (determinism).
- The hash chain is verified on read. A tampered, reordered or missing transaction is reported
  with its seq.
- A torn last record opens writable at the valid prefix. Damage earlier opens read-only at the
  last good seq.
- Unknown or newer versions are refused with a clear message, and old op versions upcast (one
  synthetic v0 → v1 test).
- Embedded definitions:
  - two instances share one definition and their terminals have distinct IDs;
  - two versions of one definition coexist;
  - `state_at` before a version change still holds the original content;
  - removing a referenced definition is refused.
- Hostile-input caps and the fuzz target run in CI, and `cargo xtask ci` is green, including
  wasm for `ecad-file`.

**Stage 2: files, harness persistence, debugger panel**
- Save, quit, reopen: the same document, IDs, log and head, with persisted = head. The
  debugger shows no pending transactions.
- Fault injection at every step of Save and Save As (append, flush, rename, folder flush)
  leaves:
  - the existing file openable;
  - the in-memory project unsaved and intact;
  - the error reported.
- A crash simulated mid-batch leaves a file that opens at a valid prefix.
- A second instance opening the same file gets it read-only. A file changed externally makes
  Save refuse and offer Save As.
- Debugger:
  - selection → IDs → transaction → record offset matches the bytes in the file;
  - a new edit shows as pending until saved, then as durable with its offset;
  - historical views leave the live head unchanged;
  - rendering holds no project lock (tested by holding the lock during a frame: the panel
    shows its stale snapshot);
  - Verify is the only path to full replay.
- The trial's "no persistence" labelling is replaced by the real behaviour, and the harness
  README and manual trial are updated.
- Owner's WSL trial: create, save, close, reopen, edit, save, inspect with the debugger.

**Stage 3: libraries**
- A `.ecadlib` with two definitions is created by tests or a small CLI.
- Placing from it embeds once, and a second placement reuses the definition.
- The project opens and replays with the library file deleted.
- A library change makes "update available" appear, derived, with nothing changing in the
  project.
- Renaming the library file keeps its identity, and two versions coexist in one project.

**Stage 4: checkpoints** (only if opening becomes slow in practice)
- Opening with and without checkpoints gives identical state.
- A corrupted or mismatched checkpoint is ignored, with a diagnostic.
- Save As without checkpoints keeps the full history.

**Out of scope for all stages:**
- drafting features, numbering, MCP and a polished product UI;
- compaction;
- the crash journal;
- codec evaluation and benchmarks;
- DWG/DXF.

## 23. Part E decisions for the owner

| # | Decision | Recommendation |
| --- | --- | --- |
| E1 | Container A: one JSON Lines file of typed records (header, tx, checkpoint) | **Yes**. ZIP and sidecar files rejected for now (§20.2). |
| E2 | Embedded definitions inline in the introducing transaction (I) rather than separate immutable records (II) | **I**. Revisit only if repetition proves to matter (§20.3). |
| E3 | Separate versioned record types in `ecad-file`, not direct serialisation of `ecad-model` | **Yes** |
| E4 | An older-schema file is rewritten in the current version on first save, with a confirmation and a backup copy | **Yes** |
| E5 | Strict reading: unknown fields, records and versions are refused | **Yes** for development |
| E6 | Frame CRC-32C plus a SHA-256 history chain over the RFC 8785 canonical form | **Yes** |
| E7 | Checkpoints: in-file, optional, validated, never written automatically in stages 1-3 | **Yes** |
| E8 | Save refuses when the file changed externally; a second opener is read-only | **Yes** |
| E9 | Save As keeps `project_id`; "Save a Copy As" with a new ID deferred | **Keep the ID** |
| E10 | The debugger is an egui developer panel in `ecad-trial`; the HTML inspector stays for exports; snapshot format bump to `ecad-inspect/2` | **Yes** |
| E11 | Length unit in project settings, mm by default, fixed at creation for now | **Yes** |
| E12 | Stages and acceptance criteria as in §22 | Approve, or reorder |

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
| 2026-10-12 | Revision 3.1: placement rotation stored in radians, with degree conversion at command and UI boundaries (§4.2); a single copy-ID policy stated (§3.4) |
| 2026-10-12 | Revision 4 draft: Part E (§20-§23), persistence design (container options, inline versus separate definitions, transaction boundaries, durable saving, recovery, versioning, checkpoints, libraries, integrity, JSON Lines) and a live read-only debugger panel, with staged acceptance criteria and decisions E1-E12; §5 points to it. For review; not implemented |
