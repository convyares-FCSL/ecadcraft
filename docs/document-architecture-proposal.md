# ECAD document architecture and integration proposal

> **Status:** proposal for review, nothing implemented. **Written:** 2026-10-12 against `dev` at
> `baee1b8` (CADCraft 0.6.2). **Reads with:** [`ecad-architecture-foundations.md`](ecad-architecture-foundations.md)
> (12 October 2026, takes precedence), [`concept-spec.md`](concept-spec.md) and
> [`cadcraft-assessment.md`](cadcraft-assessment.md).

## Summary

- **One source of truth.** Every accepted change, whether ordinary geometry or electrical meaning,
  is a transaction in the ECAD log. Applying the log produces the ECAD document. CADCraft's
  `Drawing` becomes a *projection* of that document that CADCraft also uses as its editing
  surface. It is never saved as the project and never owns history.
- **CADCraft edits are proposals.** When a CADCraft command commits (LINE, MOVE, grip edits), an
  ECAD "document owner" compares the drawing with the last projection and turns the difference
  into a transaction. It then accepts that transaction, or rejects it and restores the drawing.
  ECAD commands (WIRE, CONNECT, CONVERTTOCOMPONENT, GROUPCABLE) build transactions directly.
- **Undo appends.** On ECAD pages, CADCraft's snapshot undo is switched off, and Undo/Redo are
  handed to the ledger, which appends inverse transactions. Plain CADCraft drawings in the same
  app behave exactly as today.
- **Separate folder, separate workspace.** All ECAD code goes in `ecad/`, a Cargo workspace of its
  own that depends on CADCraft's crates by path. A probe showed this builds and runs a CADCraft
  command with **no upstream file changes**.
- **Seams, revised.** Phase 0 proposed three engine seams. Reading further shows two are needed
  now: *extension commands* and a per-document *owner* (commit gate, undo, save/close). They touch
  three engine files. UI seams (dock panels, canvas overlays, a top-level menu) are deferred until
  a feature needs them.
- **Ambiguities are refused, not guessed.** For COPY, EXPLODE, BREAK and JOIN on geometry that
  depicts a component or conductor, this proposal makes CADCraft refuse the edit on ECAD pages
  until you decide what copying and splitting mean. Edits to plain geometry are unaffected.
- **Decisions for you** are collected in [§14](#14-decisions-requiring-approval). Questions that
  are deliberately left open are in [§15](#15-deliberately-unresolved).

## Contents

1. [Scope](#1-scope)
2. [Evidence markers](#2-evidence-markers)
3. [Architecture in one picture](#3-architecture-in-one-picture)
4. [Repository layout and ownership](#4-repository-layout-and-ownership)
5. [Document model](#5-document-model)
6. [`.ecad` and `.ecadlib` contracts](#6-ecad-and-ecadlib-contracts)
7. [The transaction path](#7-the-transaction-path)
8. [Worked example](#8-worked-example)
9. [ECAD ⇄ CADCraft ID mapping lifecycle](#9-ecad--cadcraft-id-mapping-lifecycle)
10. [Integration seams and patch inventory](#10-integration-seams-and-patch-inventory)
11. [Incorporating upstream updates](#11-incorporating-upstream-updates)
12. [Optional DWG boundary](#12-optional-dwg-boundary)
13. [Room for later features](#13-room-for-later-features)
14. [Decisions requiring approval](#14-decisions-requiring-approval)
15. [Deliberately unresolved](#15-deliberately-unresolved)
16. [Proposed next steps](#16-proposed-next-steps)

## 1. Scope

**This proposal covers:** the minimal document objects and relationships; which state is stored,
which is derived and which is temporary; the `.ecad` and `.ecadlib` contracts (not their
encoding); how edits reach the log; how undo works alongside CADCraft; how ECAD IDs relate to
CADCraft handles; the repository layout; and how upstream updates are taken in.

**It does not cover:** the cable demonstration, numbering schemes, the completeness heat map, the
MCP server or library replacement UI. It also leaves out file encoding (CBOR or MessagePack),
framing, checkpoint format, product naming, default drawing standards and DWG licence acceptance.
Section 13 shows where each of these will fit.

No engine code was changed and the repository was not reorganised. The one experiment (§4.3) ran
in a temporary folder that was deleted afterwards.

## 2. Evidence markers

Integration claims carry one of three markers, in the spirit of `UNVALIDATED` in
`ros2-semantic-contracts`:

| Marker | Meaning |
| --- | --- |
| **[Shown]** | Demonstrated by building or running code in this session |
| **[Read]** | Read from the source at `baee1b8`, with file and line; not yet exercised |
| **[Assumed]** | Expected but not verified; each one appears in the verification list (§10.3) |

## 3. Architecture in one picture

```
                    ECAD application shell (ecad/apps/ecad)
                    owns the Project and a cadcraft_engine::Session
                                     │
        ┌────────────────────────────┼─────────────────────────────┐
        ▼                            ▼                             ▼
  ECAD commands              CADCraft commands             Undo / Redo / Save
  (WIRE, CONNECT, …)         (LINE, MOVE, grips, …)        (u, undo, redo, qsave)
        │                            │ edit the projection         │
        │                            ▼ as a scratch surface        │
        │                  CADCraft commit point                   │
        │                  (Session::execute / finish)             │
        │                            │ seam: owner.commit()        │ seam: owner.undo() …
        │                            ▼                             │
        │                  Adapter: diff → classify →              │
        │                  proposed Transaction                    │
        │                            │                             │
        └──────────────► Ledger.commit(Transaction) ◄──────────────┘
                         validate → append → apply
                                     │
                     ┌───────────────┴────────────────┐
                     ▼                                ▼
            ECAD Document                     Derived results
            (stored intent + drawing          (nets, completeness,
             content, materialised)            diagnostics, reports)
                     │
                     ▼
            Projector → CADCraft Drawing (per page) + handle map
                     │
                     ▼
            CADCraft canvas, snaps, grips, plotting, DXF/DWG export
```

The design keeps a pure core apart from an adapter: domain rules live in crates that know nothing
about CADCraft, much as `rosrustext_core` knows nothing about transports. The adapter is the only
code that touches both models.

## 4. Repository layout and ownership

### 4.1 Proposed tree

```
ecadcraft/                          fork of storytold/cadcraft
├── crates/                         UPSTREAM  CADCraft library crates; only marked seams change
├── apps/                           UPSTREAM  cadcraft, cadcraft-cli, cadcraft-web (kept building)
├── xtask/                          UPSTREAM  layering, assets, wasm and parity gates
├── assets/ packaging/ examples/    UPSTREAM
├── docs/                           UPSTREAM  CADCraft docs (ECAD docs move out, see §4.4)
├── Cargo.toml  Cargo.lock          UPSTREAM  root workspace, untouched
├── README.md                       OURS      repository front page (already rewritten)
├── CLAUDE.md  AGENTS.md            UPSTREAM today; rewrite pending your decision (§14 D11)
├── .github/workflows/ci.yml        OURS      new file; extended to build ecad/
└── ecad/                           OURS      everything ECAD, one Cargo workspace
    ├── Cargo.toml                  [workspace]; path deps on ../crates/*
    ├── Cargo.lock
    ├── rustfmt.toml  clippy.toml   our style (independent of upstream's)
    ├── CLAUDE.md                   ECAD agent instructions
    ├── UPSTREAM-PATCHES.md         every upstream file we change, and why
    ├── docs/                       concept spec, foundations, decisions, contracts, this proposal
    ├── crates/
    │   ├── ecad-model/             objects, typed IDs, ops, transactions, apply/invert, validation
    │   ├── ecad-store/             .ecad/.ecadlib container: framing, recovery, checkpoints, versions
    │   ├── ecad-library/           definitions, embedding, origin, "update available"
    │   ├── ecad-derive/            connectivity, nets, open ends, completeness, diagnostics
    │   ├── ecad-cadcraft/          THE ADAPTER: projector, handle map, commit gate, ECAD commands
    │   ├── ecad-ui/                egui panels and overlays for the ECAD shell
    │   └── ecad-testkit/           (dev only) document builders, golden logs, scenario runner
    ├── apps/
    │   ├── ecad/                   desktop shell wrapping cadcraft_ui_egui::CadApp
    │   └── ecad-cli/               headless replay, verify and export (later: MCP)
    └── xtask/                      ECAD gates: layering of ecad-* crates, upstream patch guard
```

Crate names follow responsibilities; the concept spec's `ecad-ledger`, `ecad-topology` and
`ecad-infer` correspond to `ecad-model`/`ecad-store` and `ecad-derive`. Splitting or merging
crates later is cheap; the boundaries below are what matter.

### 4.2 Where each part belongs

| Part | Location | Depends on | Must not depend on |
| --- | --- | --- | --- |
| Upstream CADCraft code | `crates/`, `apps/`, `xtask/` | upstream only | anything in `ecad/` |
| Application shell and UI extensions | `ecad/apps/ecad`, `ecad/crates/ecad-ui` | adapter, `cadcraft-ui-egui` | — |
| Document, ledger and library | `ecad-model`, `ecad-store`, `ecad-library`, `ecad-derive` | each other, small external crates | **any `cadcraft-*` crate** |
| Adapter between ECAD and CADCraft | `ecad/crates/ecad-cadcraft` | model crates, `cadcraft-engine`, `-doc`, `-geom` | egui and other UI crates |
| Tests | unit tests in each crate; contract tests in `ecad/crates/*/tests/*_contracts.rs`; Session-driven integration tests in `ecad-cadcraft/tests` | `ecad-testkit` (dev only) | — |
| Documentation | `ecad/docs/` (contracts, decisions, proposals); root `README.md` | — | — |

The model crates keep **no** CADCraft dependency, not even `cadcraft-geom`. They use their own
small value types (points, polyline vertices), and the adapter converts. An upstream merge then
cannot break the document model, the file format or replay; only `ecad-cadcraft` and `ecad-ui`
can be affected. ECAD layering is checked by `ecad/xtask`, because upstream's
`cargo xtask layers` rejects crates missing from its table (`xtask/src/layers.rs:36`, violation
`Unregistered` at line 150) **[Read]**.

### 4.3 Why a separate workspace

Two options:

| | A. Separate workspace in `ecad/` (recommended) | B. Add `ecad/*` to the root workspace |
| --- | --- | --- |
| Upstream files changed | none | root `Cargo.toml` `members`, `xtask/src/layers.rs` table |
| Style | own `rustfmt.toml` (yours uses `max_width = 120`; upstream 150) | upstream's formatter settings apply |
| Lock file | `ecad/Cargo.lock`; may resolve newer patch versions than upstream's | one lock |
| Gates | ECAD runs its own fmt/clippy/test/layer checks; upstream's `cargo xtask ci` still covers `crates/` | one `cargo xtask ci`, after patching it |
| Build output | second `target/` unless `CARGO_TARGET_DIR` is shared | one `target/` |

**Probe [Shown]:** a temporary nested workspace `ecad-probe/` with one crate depending on
`cadcraft-engine = { path = "../../../crates/engine" }` built on Rust 1.97, and its test ran
`Session::execute("line", …)` and found the line in model space (1 passed). Upstream crates'
`workspace = true` dependencies resolved against the root workspace. Afterwards
`cargo metadata` at the root still listed the same 16 packages without error. The probe was then
deleted.

Option A leaves the root workspace byte-for-byte upstream, which is the point of this layout. Its
costs are a second lock file and a CI job, both small. CI should share one `CARGO_TARGET_DIR` so
the upstream crates are compiled once (Phase 0 found a full debug build near 6 GB without debug
info).

### 4.4 What is not moved

- Upstream's tree stays where it is. Moving `crates/` under an `upstream/` folder would make
  every merge a rename conflict.
- Upstream apps keep building and their tests keep running. They are not our product, but a
  broken upstream app is an early warning that a merge disturbed something.
- ECAD documents now in `docs/` (`concept-spec.md`, `decisions.md`, `cadcraft-assessment.md`, this
  proposal) move to `ecad/docs/` when the layout is approved. Added files do not conflict with
  upstream, but keeping them in `ecad/` makes ownership obvious.
- The root `README.md` is ours and will conflict whenever upstream edits theirs; we resolve those
  conflicts by keeping ours.

### 4.5 Code and documentation conventions in `ecad/`

These are taken from your repositories (`rosrustext`, `ros2-semantic-contracts`,
`ros2-systems-operability`), plus CADCraft's never-crash rules, which still apply:

- `rustfmt.toml` with `max_width = 120` and `use_small_heuristics = "Max"`, as in `rosrustext`.
- A crate-level `//!` comment says what the crate is for and what it **intentionally does not
  contain**, as `ros2_semantics_core` does ("no ROS graph, middleware …").
- Public items have doc comments with `# Semantics`, `# Errors` (or `# Threading`), `# Example`
  and `# See also` sections, as in `rosrustext_core::parameters::ParameterStore`.
- A structured error type per crate (kind and domain, built with `thiserror`), as `CoreError`
  is, with no panics in non-test code (`unwrap`, `expect` and `panic!` denied; `unsafe`
  forbidden).
- Contracts in `ecad/docs/` use numbered, testable statements, as `SPEC_PC01` does, with
  unverified claims marked. This proposal's DOC-, FILE-, TX-, LIB-, UNDO- and PROJ- numbers are a
  first draft of those.
- Table-driven contract tests in `tests/<area>_contracts.rs`, one per contract area, as in
  `rosrustext_core/tests/lifecycle_contracts.rs`.

## 5. Document model

### 5.1 Four kinds of state

| Kind | What it is | Examples | Stored? |
| --- | --- | --- | --- |
| **Stored intent** | Electrical meaning and the user's explicit decisions | components, terminals, conductors and their end attachments, cables and membership, property values, embedded library definitions, "not connected" decisions | yes, as ops in the log |
| **Drawing content** | What is drawn and where, with or without meaning | lines, polylines, arcs, text, connection points, layers, representation links, deliberate annotation placement, pages | yes, as ops in the log |
| **Derived results** | Recomputable from the two above; never authoritative | nets, open ends, near misses, completeness, diagnostics, duplicate-tag warnings, BOM, wire list, the CADCraft `Drawing` projection and its handle map, spatial index | no; may be cached in checkpoints |
| **Temporary UI state** | This session's view of the work | selection, hover, zoom and pan, active page, running command and previews, panel layout, cable view toggle (concept spec: a view setting, not an op) | no |

Rules:

- **DOC-01** Only stored intent and drawing content enter the log. Nothing derived is ever read
  back as fact.
- **DOC-02** Derived results are recomputed from the document; a checkpoint may cache them, but
  deleting the cache must not change them.
- **DOC-03** Geometry may exist without meaning. A drawn line is drawing content and nothing else
  until an explicit command links it to a domain object.

### 5.2 Minimal objects

Every object has a typed, stable ID (§5.4). Every property is either unset or set (§5.5).

| Object | Kind | Holds | Relationships (by ID) |
| --- | --- | --- | --- |
| `Project` | intent | project settings (units), page order | owns pages |
| `Page` | content | title, sheet settings | items, representations and connection points placed on it |
| `Item` | content | one drawn primitive: kind (line, polyline, arc, circle, text, point), geometry, style (layer, colour, linetype, lineweight) | page; at most one representation (DOC-07) |
| `Layer` | content | name, colour, linetype, visibility, plot flag | items refer to it by ID |
| `Component` | intent | properties (tag, description, manufacturer, part number, …), all optional | terminals; zero or more representations; optional embedded definition |
| `Terminal` | intent | optional label; optional key into the component's definition | belongs to one component |
| `Representation` | content | role (for example main symbol, or another depiction such as a contact) | subject (component, conductor or cable); page; a set of items |
| `ConnectionPoint` | content | position on the page | one terminal; one component representation |
| `Conductor` | intent | properties (size, colour, type, identifiers), all optional | two ends, each with an optional attachment; zero or more route representations |
| `ConductorEnd` | intent | end A (first route vertex) or end B (last) | optional attachment: a terminal (junctions and bridges later, §15) |
| `Cable` | intent | properties (type, designation, …), all optional | ordered memberships |
| `CableMembership` | intent | position; optional core designation | one conductor |
| `EmbeddedDefinition` | intent | a project-local copy of a library definition (§6.3) | origin (library ID, definition ID, version, content hash) |

A cable has no "core" object that duplicates a conductor. "Core" is the conductor's role through
its membership, and a core designation lives on that membership. Nets are not stored objects:
they are derived from conductor attachments and (later) junctions and bridges.

### 5.3 Relationships

```
 Project ─┬─ Page ─┬─ Item ◄──────────────┐
          │        ├─ ConnectionPoint ─┐  │ items (each in at most one representation)
          │        └─ Representation ──┼──┘
          │              │ subject     │
          ├─ Component ◄─┤             │
          │    └─ Terminal ◄───────────┘
          │          ▲
          │          │ attachment (optional, per end)
          ├─ Conductor ─ ConductorEnd A/B
          │    ▲
          │    │ member (ordered, optional core designation)
          ├─ Cable ─ CableMembership
          │
          └─ EmbeddedDefinition ──origin──► (library ID, definition ID, version, hash)

 Derived, never stored:  Net = connected set of terminals and conductor ends
```

- **DOC-04** References use IDs, never names, tags or labels. Renaming a component, terminal or
  layer cannot break a reference.
- **DOC-05** Connectivity is stored intent: a conductor end is attached to a terminal by
  reference. Geometry being in the same place is not a connection; derived checks report when
  geometry and attachment disagree.
- **DOC-06** Grouping conductors into a cable adds memberships. It never changes conductor IDs,
  attachments or routes.
- **DOC-07** (proposed) An item belongs to at most one representation, so each drawn primitive
  has an unambiguous meaning.

Rust traits are used where behaviour is shared, such as "has properties", "has representations"
or "can be validated". Representation links, cable membership and attachments are relationship
records, not inheritance. Concrete types come from this model, not the other way round.

### 5.4 Identity

- One typed ID per object kind: `ComponentId`, `TerminalId`, `ConductorId`, `CableId`, `ItemId`,
  `RepresentationId`, `PageId`, `LayerId`, `DefinitionId`. The compiler keeps them from mixing.
- **Proposed encoding:** 128-bit random, allocated **when the edit is made** and written into the
  op. Replay never allocates IDs. Random IDs let content pasted from another project keep
  distinct IDs without renumbering. Users never see them; they see tags and labels, and
  diagnostics show a short form. A per-project counter is the alternative, but it needs
  remapping on every cross-project paste. Decision D5.
- **DOC-08** An ID is never reused, even after its object is deleted. Redo recreates an object
  with the same ID, because the redo op records it.
- CADCraft handles are not IDs. They are allocated by the projection and kept in a per-session
  map (§9).

### 5.5 Missing versus confirmed values

- Each property is `Unset` or `Set(value)`. "Not yet known" is `Unset`, never an empty string or
  zero. A query such as "conductors without a size" is a plain filter.
- Inferred values are derived results shown beside the stored value, never stored in it. A user
  who accepts an inferred value records it as an explicit `SetProperty`.
- A later "accepted as unresolved" marker (the concept spec's release step) is stored intent and
  fits this model without changing it.
- Completeness criteria, colours and heat-map UI are deferred. Completeness is only computed from
  explicit criteria over `Unset`/`Set`, and it is not a claim of electrical correctness.

## 6. `.ecad` and `.ecadlib` contracts

These contracts state what each file owns and promises. Encoding (CBOR or MessagePack), framing,
checksums and checkpoint layout come later and must satisfy them.

### 6.1 `.ecad` project file

**Owns:** the project's whole history as transactions (all stored intent and drawing content),
every embedded library definition the project uses, named versions as bookmarks, and optional
checkpoints as a cache.

**Promises:**

- **FILE-01 Self-contained.** Opening a project never reads a library, the network or any other
  file. Every definition it uses is embedded.
- **FILE-02 Deterministic.** Replaying the same log with the same software gives the same stored
  state. Replay applies recorded values and IDs; it never snaps, searches, infers attachments or
  allocates IDs.
- **FILE-03 Append-only saving.** Saving appends; a torn trailing write is ignored on open
  (concept spec).
- **FILE-04 Cache is optional.** Opening without checkpoints gives the same state, only more
  slowly. (After compaction this needs a decision; see §15.)
- **FILE-05 Versioned ops.** Every op type carries a version and old versions are upcast on
  replay.
- **FILE-06 Authorship.** Every transaction records its author (the user or a named agent) and a
  wall-clock time. The time is informational and never affects replay.

**Does not contain:** derived results as fact, temporary UI state, CADCraft handles, or a DXF/DWG
copy of the drawing.

### 6.2 `.ecadlib` library file

**Owns:** reusable definitions (symbol graphics, terminal definitions, default properties; later
perhaps parts), the library's own history and named versions, and its identity (a library ID
that survives renaming the file).

**Promises:** the same container rules as `.ecad` (FILE-02, -03, -05, -06). A definition has a
stable `DefinitionId` and each published change gives it a new version. Whether parts share this
container stays open (concept spec).

### 6.3 From library definition to project instance

```
 .ecadlib                          .ecad
 Definition D (v3) ──embed──► EmbeddedDefinition E   origin = (lib L, def D, v3, hash)
   terminal keys                   │  shared by every instance of D v3 in the project
   "A1","A2","13","14"             ▼
                              Component C (definition = E)
                                Terminal T1 (key "A1")   own IDs, own optional labels
                                Terminal T2 (key "A2")
                              Representation R (Insert of E's symbol, projected as a block)
```

- **LIB-01 Embed on first use.** Placing a definition embeds a copy in the same transaction
  (`EmbedDefinition` + `CreateComponent` + `AddTerminal` per terminal key + `LinkRepresentation`).
  Later instances reuse that copy.
- **LIB-02 Instances own their IDs.** Each instance terminal gets its own `TerminalId` and
  records the definition key it came from. Connections reference the instance terminal, never the
  definition.
- **LIB-03 No silent updates.** A library change makes an "update available" derived result. It
  never changes the project. Updating is an explicit transaction and is reviewable and undoable.
- **LIB-04 Local edits fork.** Editing an embedded definition inside the project makes a new
  local definition whose origin says "modified from (L, D, v3)". Other instances keep the
  original.
- **LIB-05 Same name, different content.** Two definitions with the same name and different
  origin or hash are both kept and never merged.
- **LIB-06 Harvest.** Turning project content into a library definition appends to the library's
  log; the project records nothing beyond, optionally, the new origin on its embedded copy.
- **Components need no definition.** Raw geometry converted to a component (§8 step 2) has no
  definition. A definition can be attached or harvested later.

**Open for later** (no behaviour chosen): terminal mapping when a definition is *replaced*,
including connected terminals that have no counterpart; whether replacement keeps the old
representation's placement; parts versus symbols in one container.

### 6.4 How projects stay self-contained across copy and paste

Paste between projects carries the embedded definitions the copied objects use, deduplicated by
origin and hash. Pasted objects always get new IDs. What a pasted component *means* (an
independent new component, or another depiction of an existing one) is the open copy question
(§15).

## 7. The transaction path

### 7.1 Transactions and ops

```
Transaction {
    id, author, time, label,
    kind: Edit | Undo { of: TransactionId } | Redo { of: TransactionId },
    ops: [Op],            // applied in order, all or nothing
}
```

Representative ops (enough for §8; final names come with the code):

| Area | Ops |
| --- | --- |
| Drawing content | `AddItem`, `ModifyItem { before, after }`, `RemoveItem { before }`, `AddLayer`, `ModifyLayer` |
| Representation | `LinkRepresentation`, `UnlinkRepresentation`, `SetConnectionPoint { before, after }` |
| Components | `CreateComponent`, `DeleteComponent { before }`, `AddTerminal`, `RemoveTerminal { before }` |
| Conductors | `CreateConductor`, `DeleteConductor { before }`, `AttachEnd { conductor, end, before, after }` |
| Cables | `CreateCable`, `DeleteCable { before }`, `AddCableMember`, `RemoveCableMember { before }` |
| Any object | `SetProperty { object, key, before, after }` |
| Library | `EmbedDefinition`, `UpdateDefinition { before, after }` |
| History | `NameVersion` (bookmark, not an edit) |

- **TX-01 Resolved intent.** Ops record outcomes (this terminal ID, these coordinates), not
  gestures ("clicked near (40, 15)"). A newer snapping algorithm cannot change an old project.
- **TX-02 Self-inverting.** Every op carries the values it replaces, so its inverse can be built
  without consulting later state.
- **TX-03 Atomic.** A transaction is checked against the document's invariants (references
  exist, DOC-07, no dangling attachments) and applied in full or not at all. The document uses
  copy-on-write, as CADCraft's `Drawing` does, so a failed apply leaves nothing half-updated.
- **TX-04 Replay checks preconditions.** If an op's `before` does not match during replay, the
  log is corrupt. The project opens read-only with a report; nothing is silently repaired.

### 7.2 Path A: ECAD commands

ECAD commands are CADCraft `CommandSpec`s registered through the extension seam (§10), so they
reach the command line, menus, scripts, the control channel and MCP like any other command
(every consumer reads `command_specs()`: `ui-egui/src/menus.rs:434`, `cmdline.rs:134`,
`control.rs:189`, `mcp/src/backend.rs:203`) **[Read]**. Interactive ones implement the public
`Interactive` trait (`engine/src/prompt.rs:173`, re-exported at `lib.rs:42`) **[Read]**, so they
get CADCraft prompts, snaps and rubber-band previews.

When an ECAD command finishes, it builds a transaction from resolved inputs and commits it to the
ledger. The projector then updates the page's `Drawing` and the owner records that drawing as the
new projection. The commit gate (Path B) therefore sees no unexplained difference.

### 7.3 Path B: CADCraft commands on an ECAD page

CADCraft commits in exactly two places: `Session::execute` (`engine/src/lib.rs:752-789`) and the
end of an interactive command (`finish`, `lib.rs:1092-1128`). Both run the post-command passes
(groups, constraints, associativity, `lib.rs:773-775` and `1111-1113`) and then push a
whole-drawing undo snapshot **[Read]**. The owner seam sits after those passes and before the
push:

1. **Diff** the drawing against the *last projection* the owner recorded, not against the
   command's own `before`. Changes made outside commit points are then caught too (§7.6). Entity
   stores hold `Arc<Entity>` in chunks (`doc/src/store.rs`), so unchanged entities compare by
   pointer **[Read]**; the cost is assumed small **[Assumed]**.
2. **Classify** each difference with the handle map and the rules in §9: new item, modified item,
   removed item, a change that is session-only (for example the current layer), or refused.
3. **Build** a transaction labelled with the command's name and authored by whoever ran it.
4. **Commit** it to the ledger, which validates it (TX-03).
5. **Accepted:** record the drawing as the new projection and push no CADCraft undo snapshot.
   **Refused:** restore the last projection and echo the reason on the command line. This reuses
   the engine's existing restore pattern for non-finite geometry (`engine/src/finite.rs:29`)
   **[Read]**.

Transparent commands (`'ZOOM` inside LINE) add their changes to the outer command's step
(`lib.rs:1114-1121`) **[Read]**, so the gate runs once, when the outer command commits.

**Projection fidelity (PROJ-01):** after an accepted CADCraft edit, projecting the new document
must reproduce the engine's drawing for every touched item. If it does not, the drawing content
schema lost information. The projection then wins, the item is redrawn from the document, and
tests fail. This invariant is the main integration test **[Assumed]**.

### 7.4 What the projector writes

- One CADCraft document (`DocState`) per ECAD page for Phase 1 (pages-as-layouts stays open, as
  recorded in `decisions.md`).
- Items become entities of matching kinds. Layers become layer-table entries.
- Each component representation also becomes an unnamed CADCraft group, so picking one primitive
  selects the whole symbol. `PICKSTYLE` bit 1 is on by default (`engine/src/lib.rs:361-418`)
  **[Read]**. The group is projection only: it is not logged, and group edits on ECAD pages are
  refused.
- Connection points become markers that object snaps can find. The candidate is a POINT entity on
  a non-plotting layer (CADCraft's `Defpoints` layer is created with `plot: false`,
  `doc/src/lib.rs` `bare()`) **[Read]**. An overlay with a custom snap is the alternative
  **[Assumed]**; decided in the first adapter spike.
- Derived markers (open ends, mismatched attachments) are **not** drawn as entities. They need
  the canvas overlay seam when that feature arrives (§10.2).

### 7.5 Undo and redo: one history

- **UNDO-01** On an ECAD page, CADCraft's snapshot stack stays empty. The owner tells the engine
  to skip `push_undo`.
- **UNDO-02** `u`, `undo`, `redo` and `mredo` all reach `Session::undo`/`redo`
  (`engine/src/cmd/edit.rs:15-23`, calls at `141`, `215`, `383`) **[Read]**. For an owned page
  these hand over to the owner, which appends `Undo { of }` or `Redo { of }` transactions holding
  the concrete inverse ops (TX-02). The projector updates the drawing.
- **UNDO-03** The undo/redo position is derived from the log, never stored. The rules are linear:
  undo targets the latest effective edit; a new edit after undo makes the undone edits no longer
  redoable, though they remain in the log.
- **UNDO-04** Undo covers the whole project, not one page, because one transaction can touch
  several pages (for example a cable shown on two sheets).
- In-command undo still works (LINE's "Undo" option, TRIM's undo, `modify.rs:2150`, `trimextend.rs:501`),
  because it rewinds uncommitted scratch state inside one command **[Read]**.
- UNDO's Mark, Back, BEgin/End and Control options report "not available on ECAD pages" until
  they are designed against the log.

Why not keep CADCraft's stack in step with the log? Its snapshots include `Drawing::handseed`
(`doc/src/lib.rs:94`, `new_handle` at `154-158`), so snapshot undo reissues handles that the map
may still hold **[Read]**. It would also let UNDO Back rewind past ledger transactions. Two
histories means two truths, so one of them has to go.

### 7.6 Changes outside commit points

| Source | Evidence | Treatment |
| --- | --- | --- |
| Pan/zoom inside a paper-space viewport edits the `Viewport` entity via `Arc::make_mut` | `engine/src/lib.rs:225-247` **[Read]** | Viewport view centre and height are UI state; the diff ignores them |
| `hidden_objects` (ISOLATEOBJECTS) | "Never written to a file", `doc/src/lib.rs` **[Read]** | UI state; ignored |
| Header variables and table edits made by commands (current layer, styles) | committed through the normal points **[Read]** | classified as logged, session-only or refused, from a census of all commands (V4) |
| AUDIT/RECOVER replacing the drawing | `cmd/audit.rs:85`, `115` **[Read]** | refused on ECAD pages in Phase 1 |
| Opening a DXF/DWG | creates a new `DocState` without an owner | stays a plain CADCraft drawing; adopting into a project is Phase 4 |

**DOC-09** An unclassified difference is refused, never silently dropped. If the census finds a
common command refused for no good reason, its class is added explicitly.

## 8. Worked example

Short IDs (`i1`, `c1`, `T1`) stand in for 128-bit IDs. Coordinates are millimetres on page `p1`.
Handles are illustrative.

**Step 1: draw raw geometry.** LINE draws a 40 × 20 rectangle. At LINE's `finish` the owner finds
four new Line entities (handles `2A`–`2D`).

```
T1  Edit "Line"  author=user
    AddItem i1 line (0,0)-(40,0)     AddItem i2 line (40,0)-(40,20)
    AddItem i3 line (40,20)-(0,20)   AddItem i4 line (0,20)-(0,0)
map: i1↔2A  i2↔2B  i3↔2C  i4↔2D
```

The items are plain drawing content (DOC-03).

**Step 2: convert to a component.** Select the four lines, run CONVERTTOCOMPONENT.

```
T2  Edit "Convert to component"
    CreateComponent c1                       all properties Unset
    LinkRepresentation r1 subject=c1 role=main items=[i1,i2,i3,i4]
```

Appearance and position are unchanged; the item IDs are unchanged. The projector adds an unnamed
group over `2A`–`2D`. Then ADDTERMINAL picks three points on the right-hand edge:

```
T3  Edit "Add terminals"
    AddTerminal t1 component=c1 label=Unset   SetConnectionPoint t1 rep=r1 at (40,15)
    AddTerminal t2 component=c1 label=Unset   SetConnectionPoint t2 rep=r1 at (40,10)
    AddTerminal t3 component=c1 label=Unset   SetConnectionPoint t3 rep=r1 at (40,5)
```

**Step 3: draw a loose conductor.** WIRE from (90,15) to (55,15), both ends in free space.

```
T4  Edit "Wire"
    CreateConductor w1                       ends A and B unattached, properties Unset
    AddItem i5 polyline [(90,15),(55,15)]
    LinkRepresentation r2 subject=w1 role=route items=[i5]
map: i5↔2E
```

A loose end is valid; derived results report it as an open end.

**Step 4: connect it.** CONNECT, pick end B of `w1`, pick terminal `t1`.

```
T5  Edit "Connect"
    ModifyItem i5 before=[(90,15),(55,15)] after=[(90,15),(40,15)]
    AttachEnd w1.B before=none after=t1
```

The terminal **ID** is recorded (TX-01). Replay applies `AttachEnd w1.B → t1` and never searches
for whatever is near (40,15).

Two more wires are then drawn straight onto `t2` and `t3`. WIRE resolved the snap to each
connection point when the wire was drawn, so each conductor is created already attached, in one
atomic transaction:

```
T6  Edit "Wire"   CreateConductor w2, AddItem i6 [(90,10),(40,10)], LinkRepresentation r3 w2 [i6],
                  AttachEnd w2.B before=none after=t2
T7  Edit "Wire"   CreateConductor w3, AddItem i7 [(90,5),(40,5)],   LinkRepresentation r4 w3 [i7],
                  AttachEnd w3.B before=none after=t3
map: i6↔2F  i7↔30
```

**Step 5: group into a cable.** Select the three routes, run GROUPCABLE.

```
T8  Edit "Group as cable"
    CreateCable k1                           properties Unset
    AddCableMember k1 w1 position=0 core=Unset
    AddCableMember k1 w2 position=1 core=Unset
    AddCableMember k1 w3 position=2 core=Unset
```

`w1`–`w3` keep their IDs, attachments and routes (DOC-06). Nothing is drawn yet; the cable's
representation (slash, bundle, fan) belongs to the cable demonstration.

**Step 6: reroute without changing connectivity.** On `i6`, add a vertex with CADCraft's polyline
grip and drag it to (65,25). At the grip command's commit the owner sees the same handle with new
vertices, and both end vertices unchanged:

```
T9  Edit "Grip edit"
    ModifyItem i6 before=[(90,10),(40,10)] after=[(90,10),(65,25),(40,10)]
```

No attachment op is produced. End B still sits on `t2`'s connection point, so no mismatch is
reported. The derived nets are the same before and after.

**Step 7: undo and redo.** Cmd+Z twice, then Cmd+Shift+Z.

```
T10 Undo of T9    ModifyItem i6 before=[(90,10),(65,25),(40,10)] after=[(90,10),(40,10)]
T11 Undo of T8    RemoveCableMember k1 w3 (pos 2)   RemoveCableMember k1 w2 (pos 1)
                  RemoveCableMember k1 w1 (pos 0)   DeleteCable k1 before={properties}
T12 Redo of T11   CreateCable k1   AddCableMember k1 w1 0   … w2 1   … w3 2
```

`T12` recreates the cable as **`k1`**, the same ID (DOC-08). `T9` stays undone; one more redo
would append `T13 Redo of T10`. Nothing is removed from the log. The projector updated `i6` in
place (handle `2F` kept), so selection and grips stay valid.

If the user had undone `T4`, item `i5` would have left the drawing. Redo would bring it back
under a **new** handle (say `35`), with `i5` unchanged and the map updated. Handles may change;
ECAD IDs never do.

**Replay.** Opening the file applies T1–T12 to an empty document. Each op is applied as recorded:
no snapping, no attachment inference, no ID allocation. The result equals the live state (FILE-02).
The undo position is derived again: next undo targets T12, next redo T10. A checkpoint after T12
plus an empty tail must give the same state; that is the ledger's main property test.

**What this example deliberately does not decide:** what COPY of the component should mean, how
the wires are numbered, what the cable looks like, and what happens when a connected end is
dragged off its terminal. These are in §9 and §15.

## 9. ECAD ⇄ CADCraft ID mapping lifecycle

The map `ItemId ⇄ Handle` (plus connection points and projection groups) lives only in the
running session. It is rebuilt whenever a page is projected and is never saved.

| Event | What CADCraft does to handles | ECAD effect | Status |
| --- | --- | --- | --- |
| Open project, project a page | new drawing, fresh handles | map built; IDs unchanged | defined |
| New geometry (LINE, PLINE, CIRCLE, TEXT) | new handles from `handseed` | `AddItem` with a new ID | defined |
| MOVE, ROTATE, SCALE (no copy) | same handle, entity changed in place (`modify_entity` in `transform_entities`, `modify.rs:120`) **[Read]** | `ModifyItem`; same ID | defined (end-vertex rule below) |
| STRETCH, grips, property edits | expected to keep the handle **[Assumed]** | `ModifyItem`; same ID | census V4 |
| ERASE of an unlinked item | handle removed | `RemoveItem` | defined |
| ERASE of an item in a representation | handle removed | **open**: see D6 | proposed default needs approval |
| COPY, MIRROR/ROTATE/SCALE with copy, ARRAY, COPYCLIP + PASTECLIP | new handles (`modify.rs:108`); the commit does not say which new entity came from which original (`run_copy`, `modify.rs:162`; paste's `adopt` replaces the handle, `clipboard.rs:341`) **[Read]** | unlinked items: `AddItem`, plain geometry. Linked items: **refused on ECAD pages** until copy semantics are decided | refusal is a holding position, not an answer |
| EXPLODE | removes the entity, adds parts with new handles (`modify.rs:1289-1309`) **[Read]** | unlinked: `RemoveItem` + `AddItem`s. Linked: **refused** | open |
| BREAK, BREAKATPOINT | removes the entity, adds all pieces with new handles (`modify.rs:1319-1407`) **[Read]** | unlinked: remove + add. On a route: **refused** (would it split the conductor?) | open |
| JOIN | removes sources, keeps or creates one (`modify.rs:1425`) **[Read]** | unlinked: fine. Across two routes: **refused** (would it merge conductors?) | open |
| TRIM, EXTEND, FILLET, LENGTHEN | per command; not yet traced **[Assumed]** | same handle → `ModifyItem`; replaced handle → as BREAK | census V4 |
| ECAD undo/redo | removed items get new handles when restored | IDs unchanged; map updated | defined |
| CADCraft snapshot undo | reverts `handseed`, may reissue handles **[Read]** | disabled on ECAD pages (UNDO-01) | defined |
| Cross-project paste | new handles in the target drawing | new IDs; definitions travel (§6.4); linked content refused until copy semantics decided | open |
| GROUP edits on projection groups, BEDIT/REFEDIT, AUDIT/RECOVER | various | refused on ECAD pages in Phase 1 | defined |
| Save, close, reopen | handles discarded | IDs persisted in the log | defined |
| DXF/DWG export | export file has its own handles | no ECAD IDs needed in the export | defined |

**Route edits and attachments.** Moving a route vertex that is not an attached end is a reroute
(T9). Moving an *attached* end with a generic CADCraft edit is ambiguous: the user might mean
"reroute" or "reconnect". Options: (a) refuse; (b) keep the attachment and report "end not at its
terminal"; (c) detach; (d) resolve whichever terminal it now touches, once, and record
`AttachEnd`. Recommendation: (b) for generic edits, with explicit CONNECT/DISCONNECT commands.
A later wire-end grip can offer (d) as a deliberate gesture. Decision D7.

**Moving a component with wires attached** raises the same question from the other side: should
attached route ends follow, recorded as explicit `ModifyItem` ops in the same transaction, or stay
put and be flagged? This is left open for the wire-tool spike (§15).

## 10. Integration seams and patch inventory

### 10.1 Engine seams needed now

| Seam | Upstream file(s) | Shape | Why it cannot be avoided |
| --- | --- | --- | --- |
| **E1 Extension commands** | `crates/engine/src/cmd/mod.rs` (`command_specs`, lines 165-202) | the embedder installs a `Vec<CommandSpec>` once, before first use, following the existing `set_io` pattern (`cmd/file.rs:31-38`); appended after built-ins | the registry is a closed `OnceLock` list **[Read]** |
| **E2 Document owner** | `crates/engine/src/lib.rs` (`DocState` field; `execute`; `finish`; `undo`/`redo`) | `DocState` gets an optional owner trait object. At both commit points the owner returns accept, skip-undo or refuse; `undo`/`redo` hand over when an owner is present | `Run` is a plain `fn` pointer, so ECAD commands need a way to reach the project; commits must be gated; undo must not be the snapshot stack **[Read]** |
| **E3 Save and close** | `crates/engine/src/cmd/file.rs` (`qsave`/`saveas` at line 146; `close` at 267); autosave in `engine/src/autosave.rs` and `ui-egui/src/autosave.rs` | for owned documents, SAVE goes to the owner (the `.ecad` file), close asks the owner, and drawing autosave is skipped | otherwise Cmd+S on an ECAD page writes a DXF of the projection and looks like a project save **[Read]**; exact call sites **[Assumed]** |

Phase 0's "domain state slot" becomes the owner in E2. Phase 0's "commit observer" becomes a
gate that can refuse. Undo, save and close are new findings. Dirty tracking needs no seam: it
compares `doc` with `saved` by pointer (`lib.rs:162-164`) **[Read]**, so the owner sets `saved`
after writing the project.

Plain CADCraft drawings have no owner, so every seam is a no-op for them. That is also why the
seams are worth offering upstream as general extension points (needs your approval, D3).

### 10.2 UI seams deferred until a feature needs them

| Seam | Upstream file | Needed for | Interim without a patch |
| --- | --- | --- | --- |
| U1 Extra dock panels | `ui-egui/src/docking.rs` (closed `Panel` enum, line 11) | project browser, properties, diagnostics docked like Layers | the shell draws egui side panels around `CadApp::ui` (`ui-egui/src/lib.rs:479`) without docking persistence **[Assumed]** |
| U2 Canvas overlay painter | `ui-egui/src/canvas.rs` (overlays hard-wired at 1140-1152) | open-end markers, connectivity overlay, heat map | none worth having; patch when the first overlay feature lands |
| U3 Top-level menu | `ui-egui/src/menus.rs` (`MENUS`, line 9) | an "Electrical" menu | ECAD commands use existing menus (for example Tools ▸ Electrical) |

### 10.3 Verification list

Each item becomes a test in `ecad-cadcraft/tests` before the seam it supports is relied on.

| # | Claim | Evidence now | Proof |
| --- | --- | --- | --- |
| V1 | A separate workspace builds against upstream crates | **[Shown]** probe | CI job on `ecad/` |
| V2 | Extension commands reach command line, menus, control channel and MCP | **[Read]** | drive each surface with an ECAD command |
| V3 | The gate sees every committed change, after the post-command passes | **[Read]** | instrumented commit count across all commands |
| V4 | No other path commits drawing content; every command's changes classify | **[Read]**, partial | census: run every `CommandSpec`'s JSON form on a sample ECAD page and record what changes |
| V5 | Viewport pans and `hidden_objects` are the only out-of-band changes | **[Read]** | census of UI and control methods |
| V6 | All undo entry points go through `Session::undo`/`redo` | **[Read]** | Cmd+Z, UNDO n, MREDO on an owned page append ledger transactions |
| V7 | Save, close and autosave can be routed for owned documents | **[Assumed]** | E3 tests, including close with unsaved changes |
| V8 | Projection fidelity (PROJ-01) | **[Assumed]** | property test: random supported edits round-trip |
| V9 | ECAD interactive commands get prompts, snaps and previews | **[Read]** (`Interactive` is public) | WIRE spike |
| V10 | Projected groups give whole-component picking | **[Read]** | pick test |
| V11 | The diff costs little on large pages | **[Assumed]** | benchmark at 10k items |
| V12 | ECAD crates build for `wasm32` | **[Assumed]** | `ecad/xtask wasm` |
| V13 | egui side panels outside `CadApp` lay out acceptably | **[Assumed]** | shell spike screenshot |

### 10.4 Patch inventory (proposed)

| File | Seam | Estimated size |
| --- | --- | --- |
| `crates/engine/src/cmd/mod.rs` | E1 | ~15 lines |
| `crates/engine/src/lib.rs` | E2 | ~60 lines (one field, two commit sites, undo/redo) |
| `crates/engine/src/cmd/file.rs`, autosave call sites | E3 | ~20-30 lines, after V7 |
| `crates/ui-egui/src/{docking,canvas,menus}.rs` | U1-U3 | later, only when needed |
| `crates/io` and/or `crates/dwg` | DWG feature gate (§12) | only if you choose that option |
| `README.md`; `CLAUDE.md`, `AGENTS.md` | not code | ours; agent files pending D11 |

Each patch is marked in the code with `// ECAD-SEAM(<name>):` and listed in
`ecad/UPSTREAM-PATCHES.md` with its reason and test. ECAD-specific semantics stay out of upstream
crates: the seams carry only generic notions (commands, owner, accept/refuse, undo, save) and say
nothing about components or wires.

## 11. Incorporating upstream updates

1. Add `upstream` = `https://github.com/storytold/cadcraft` as a fetch-only remote.
2. On demand, or about fortnightly, cut `upstream-sync/<date>` from `dev` and **merge**
   `upstream/main`. Never rebase: merges keep history readable and conflicts confined.
3. Conflicts are expected only in the patch inventory files and `README.md`. Resolve them keeping
   the `ECAD-SEAM` blocks.
4. Run upstream's gates on the root workspace, then the ECAD workspace: fmt, clippy, tests, the
   V-tests above and ECAD layering.
5. Run the **patch guard**: `git diff --name-only upstream/main...HEAD` limited to upstream
   paths must equal the inventory. Any other changed upstream file fails CI.
6. Open a PR into `dev` summarising upstream changes that touch the seams. It merges only after
   your approval; green CI is not approval.
7. Optionally offer E1-E3 upstream as general hooks. If accepted, our patch set shrinks to zero.
   This is outward-facing, so it needs your go-ahead (D3).

Upstream changes most likely to hurt are in `Session::execute`/`finish`, `DocState`, the command
registry, the entity kinds, and DXF writing. The V-tests guard each of those.

## 12. Optional DWG boundary

- ECAD exports build a plain CADCraft `Drawing` per page (or a multi-layout drawing) from the
  projection, then write DXF through `cadcraft-io`. DXF is always available.
- DWG export and adopt sit behind an ECAD cargo feature `dwg` in `ecad-cadcraft` and the apps, so
  a build without it is a configuration choice.
- **Current constraint [Read]:** `cadcraft-io` depends on `cadcraft-dwg` unconditionally
  (`crates/io/Cargo.toml:16`), and `cadcraft-engine` depends on `cadcraft-io`
  (`crates/engine/Cargo.toml:28`). Every native build therefore links `acadrust` (MPL-2.0)
  whatever ECAD's feature says. WASM builds already exclude it (`crates/dwg/Cargo.toml`
  target-specific dependency).
- So a truly DWG-free native build needs one of: (a) accept `acadrust` as an unmodified library
  and keep the feature for UI only; (b) a small upstream patch making `acadrust` optional in
  `cadcraft-dwg`/`cadcraft-io` (a reasonable upstream contribution); (c) substitute `cadcraft-dwg`
  at build time with a stub that reports "DWG not available" **[Assumed]**, untested whether Cargo
  allows this for a path dependency. Licence acceptance stays open, as you asked.

## 13. Room for later features

| Feature | Where it fits | What this proposal already provides |
| --- | --- | --- |
| Cable demonstration (slash, traditional ⇄ fan) | ECAD commands + cable representations; view toggle as UI state | cables group conductors by ID (DOC-06); representations allow several depictions of one cable |
| Numbering schemes, wire identification | `SetProperty` on conductors/ends; identifier records; derived suggestions | IDs independent of names (DOC-04); `Unset` is distinguishable; no single-number assumption |
| Completeness heat map | `ecad-derive` over `Unset`/`Set`; drawn through U2 | missing values are queryable (§5.5) |
| MCP | `ecad-mcp` over the same transactions; proposals are transactions held outside the log until accepted; release stays human-only | one transaction path for user, tools and agents; author on every transaction |
| Library replacement UI | `UpdateDefinition` + terminal remapping transaction | LIB-02/03/04; replacement rules deliberately open |
| Multi-page, cross-references | pages as projections; cross-refs derived | representations carry a page; undo is project-wide |
| DWG adopt | an import that emits ordinary transactions plus a review step | everything entering a project is a transaction |

## 14. Decisions requiring approval

| # | Decision | Recommendation | Alternatives | Blocks |
| --- | --- | --- | --- | --- |
| D1 | The log is the only truth; CADCraft's `Drawing` is a projection and editing surface; CADCraft commits are proposals the owner may refuse | approve | Drawing as truth with ECAD data in xdata (rejected in Phase 0) | everything |
| D2 | Layout: `ecad/` as a **separate Cargo workspace** with path dependencies | approve | add `ecad/*` to the root workspace (patches root `Cargo.toml` and `xtask/src/layers.rs`) | skeleton PR |
| D3 | Engine seams E1 (extension commands) and E2/E3 (document owner: commit gate, undo, save/close); UI seams only when needed; may we later offer them upstream? | approve E1-E3; offering upstream to wait until they have run here | keep Phase 0's three seams (they miss undo and save) | adapter PR |
| D4 | Drawing content uses an **ECAD-owned schema** for a listed subset of kinds (line, polyline, arc, circle, text, point); other kinds are refused on ECAD pages for now | approve | store CADCraft `Entity` serde as an opaque payload (complete, but couples our file format to upstream's internal structs) | model PR |
| D5 | IDs: 128-bit random, allocated at edit time, never reused, shown shortened | approve | per-project counters (remap on cross-project paste) | model PR |
| D6 | Erasing depicted geometry: a conductor whose last route item is erased is deleted in the same transaction (its attachments and memberships removed explicitly); a component whose last representation is erased **remains**, flagged "not shown" | approve as Phase 1 default | delete both; keep both; refuse | adapter PR |
| D7 | A generic edit that moves an attached conductor end keeps the attachment and reports "end not at terminal"; reconnection only by explicit command | approve | refuse; detach; resolve the new terminal at commit time | wire spike |
| D8 | Until copy semantics are decided, COPY/EXPLODE/BREAK/JOIN/paste of **linked** geometry is refused on ECAD pages; unlinked geometry behaves normally | approve as a holding position | pick a copy meaning now (the foundations note asks not to) | adapter PR |
| D9 | Undo is linear and project-wide, derived from the log; UNDO Mark/Back/BEgin/End unavailable on ECAD pages at first | approve | per-page undo | model PR |
| D10 | Any drawing change that is not classified is refused (DOC-09); a command census sets each class | approve | accept and silently not persist | adapter PR |
| D11 | Housekeeping: move ECAD docs to `ecad/docs/`; rewrite root `CLAUDE.md`/`AGENTS.md` for ECAD (keeping upstream's never-crash, clean-room and asset rules) with ECAD detail in `ecad/CLAUDE.md`; disable the inherited release and packaging workflows | approve | leave upstream agent files as they are (they tell agents to push to `main`) | skeleton PR |
| D12 | Phase 1 maps one ECAD page to one CADCraft document; the pages-as-layouts choice stays open for Phase 2 | approve for Phase 1 only | decide layouts now | adapter PR |

Not asked now, as you requested: DWG licence acceptance (§12 lists the options), product name,
default drawing standard.

## 15. Deliberately unresolved

Each item states the constraint this proposal preserves; none is decided by the example.

| Subject | Constraint preserved | Next step |
| --- | --- | --- |
| Copy and duplicate | an independent copy needs new component IDs; another depiction of the same contact is a new representation of the existing component; neither is hard-coded (D8 refuses until decided) | explicit ECAD copy commands for each meaning, once chosen |
| Tag scope and "same tag means same component" | tags are properties (DOC-04); duplicates are reported, never merged | decide scope (page, project, IEC 81346 aspects) before tag identity |
| Wire identification | identifiers are a property list, possibly per end, possibly hidden or repeated; manual or derived | design with numbering schemes |
| Net identity, junctions, terminal bridges | nets are derived; only attachments are stored | design before numbering by net |
| Inference versioning | stored intent never changes with rules; derived results can | candidates: record the rule version at each named version; upgrading rules is an explicit transaction showing differences |
| Checkpoint authority after compaction | today checkpoints are cache (FILE-04) | after compaction a checkpoint becomes the base of truth for the dropped prefix; decide what proves it correct |
| Named-revision preservation | named versions are bookmarks | decide whether compaction keeps their ops, a frozen state, or released outputs |
| Transaction recovery and op evolution | FILE-03, FILE-05 | settle with the encoding |
| Library replacement and terminal mapping | LIB-02/03/04 | design with the replacement UI |
| Parts container | concept spec open question | decide with library work |
| A conductor in more than one cable; an item in more than one representation | DOC-07 proposes "at most one representation per item"; cable membership not yet limited | confirm with the cable demonstration |
| Component moved with attached wires | either explicit route edits in the same transaction, or displaced ends flagged | wire-tool spike |
| Cable view toggle persistence | concept spec: a view setting, not an op | decide whether the choice is remembered per page outside the log |

## 16. Proposed next steps

After approval, each step arrives as a separate PR into `dev` and merges only with your approval:

1. **Skeleton and housekeeping** (D2, D11): create `ecad/` with its workspace, `ecad/xtask`
   (layering and patch guard), CI job, `UPSTREAM-PATCHES.md`; move ECAD docs. No behaviour.
2. **Model in memory** (D4, D5, D9): `ecad-model` objects, ops, transactions, apply/invert,
   validation and history. §8 becomes an executable scenario test: replay equals live state, and
   undo/redo IDs are preserved. No CADCraft dependency.
3. **Seams and adapter** (D3, D6, D7, D8, D10, D12): E1-E3 patches with their V-tests, projector,
   handle map, commit gate and command census; CONVERTTOCOMPONENT, ADDTERMINAL, WIRE, CONNECT and
   GROUPCABLE as minimal commands.
4. **Container** (`ecad-store`): encoding and framing chosen against §6, with recovery and
   checkpoint property tests.

The cable demonstration, numbering, heat map, MCP and library replacement follow only after these
foundations are approved.

## Corrections to the Phase 0 assessment

- Engine seams: E1-E3 (§10.1) replace the three proposed in Phase 0, which missed undo
  delegation and save/close routing.
- `EntityKind` has 30 variants (29 kinds plus `Unknown`, `doc/src/entity.rs:876`), not 25.
- A DWG cargo feature at the ECAD level does not keep `acadrust` out of native builds, because
  `cadcraft-io` depends on `cadcraft-dwg` unconditionally (§12).

## Revision history

| Date | Change |
| --- | --- |
| 2026-10-12 | First proposal, following the Phase 0 assessment and the architecture foundations note |
