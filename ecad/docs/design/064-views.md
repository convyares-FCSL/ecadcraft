# Design 064: one update pipeline for derived views (R9)

Status: Approved (owner, 2026-11-15: all ten decisions in §14 as recommended). Amended the same
day for the approved crate map (`docs/design/crates.md`, lane 065): `Views` and `derived/` live in
the new Core crate **ecad-electrical** from the start. ECAD is desktop-only (owner ruling: no
web build), so nothing here depends on wasm. Amended again the same day: every derived view has
one way in (§5.2, decision 11). Brief: `docs/briefs/064-views.md`.
Source: finding 1 and row R9 of the 2026-11 code review (not committed). This is an internal design
with no new controls. Its user-facing promise: nothing a person sees changes, except that views
can no longer go stale after an edit that a hand-written list forgot.

**Goal in one sentence.** Each derived view states its update map once, against one exhaustive
classification of the ops. All three commit paths run the same pipeline, so a new `Op` can't be
silently ignored and a new view takes one struct and one line.

Inventory taken on `dev` at `445787b8` (059 cable manager merged, #116). `Op` still has 33
variants; 059 added none. File paths in §1 are as on `dev` before the crate split. After the
split (065), every module the crate map marks **E** is in `ecad-electrical`, still re-exported
under its old `ecad_cadcraft::` path. That covers checks, outputs, xref, references, connectors,
plc, rating, generated, device_list, props and the rest. `project` stays in ecad-cadcraft.

---

## 1. What is there today

### 1.1 The three commit paths (`crates/ecad-cadcraft/src/project.rs`)

| Path | Where | Before the commit | After the commit |
|---|---|---|---|
| Command (Path A), merge, Restore | `apply_tx` `:2336` | `xref::Watch::take`, `pages_touched`, `devices_touched`, `props::touched`, `tree_touched`, `checks.scope`, `outputs.scope`, `note_frame_ops` | `finish_watch` (xref, connector index, `pages_for`), `numbered_pages`, `with_numbered`, `update_checks`, `update_outputs`, `generated_touched`, `record_change`, `redraw_touched` |
| Interactive (drawing) commit | `DocumentOwner::commit` `:2656` | the same, **without** `pages_touched` or `note_frame_ops`; plus `proposals::before_move` | `refresh_page` of the edited page **first**, then the same as above, with `page` added by hand; `move_follow_up` |
| Undo / redo | `history` `:2947` | the same as Path A, plus `pages_named` and the page order | the same, without the numbering helpers; `front_after_history` picks the page to bring forward |

Each copy calls about 12 updates in the same order. A new view has to be added to all three.
The copies have already drifted apart:

- **D1.** The interactive commit never calls `note_frame_ops`. Drawing edits seldom touch the
  title block, so this hasn't shown up yet.
- **D2.** The interactive commit never calls `pages_touched`. An `AttachEnd` made by a drag
  changes the conductor's other pages too, but those pages are not entered in the change log, so
  a preview of one of them can stay stale.
- **D3.** `with_numbered` and `numbered_pages` patch the numbering pass's extra ops into two
  feeds by hand. The other feeds get them only because they read the whole transaction after
  the commit.

### 1.2 "Which ops touch me" functions (ecad-cadcraft)

Measured with a brace-matching script, so treat the counts as ±1. "Variants" is how many `Op::`
variants the function names. "`_`" counts the wildcard arms.

| # | View (kept state) | Function(s) today | Variants / `_` | New `DerivedView` (§4) |
|---|---|---|---|---|
| 1 | Checks list (`checks::Checks`) | `checks.rs:1967 Scope::collect` (215 lines) | 33 / 1 | `ChecksView` |
| | ↳ pages with checks off | `page_types.rs:468 checks_changed`, `:485 touches_checks` | 4 / 1, 8 / 1 | note of `ChecksView` (`Scope::all`) |
| | ↳ style rows | `styles/checks.rs:42 touches` | 7 / 1 | note of `ChecksView` |
| | ↳ title-block rows | `title_block/checks.rs:33 touches` | 9 / 1 | note of `ChecksView` |
| | ↳ rating results (`rating::Kept`, inside Checks) | `rating/mod.rs:1009 Touch::collect`, `rating/load.rs:29 touched` | 21 / 1 | note of `ChecksView` (`rating::note`) |
| 2 | Report models (`outputs::Outputs`) | `outputs.rs:504 Scope::collect` (169 lines), `:831 moves_page_numbers` | 33 / 0, 4 / 1 | `OutputsView` |
| | ↳ PLC model | `plc/mod.rs:378 touched`, `:468 update` | 11 / 3, 2 / 1 | note of `OutputsView` |
| | ↳ terminal-plan pages | `strip_page.rs:352 update` | 2 / 1 | note of `OutputsView` |
| | ↳ rack places | `plc/sheets.rs:405 rack_places_touched` | 2 / 1 | note of `OutputsView` |
| 3 | Reference index (`xref::RefIndex`) | `xref.rs:1599 Scope::collect`, `Watch::take` / `finish` | 24 / 2 | `RefsView` |
| | ↳ cable references | `references.rs:523 cables_touched`, `:641 moved_cables` | 28 / 1 | note of `RefsView` |
| | ↳ continuation names | `continuations.rs:515 names_named` | 11 / 1 | note of `RefsView` |
| 4 | Connector index (`connectors::Index`) | `connectors.rs:1266 touched`, `:1355 pages_for` | 12 / 1, 1 / 0 | `ConnectorsView` |
| 5 | Drawn pages (change log, redraw) | `project.rs:3059 pages_touched` (147 lines), `plc/mod.rs:1075 pages_touched`, `styles/mod.rs:406 pages_touched`, `project.rs:3210 pages_named` | 19 / 1, –, 2 / 2, 6 / 1 | `PagesView` |
| 6 | Kept frame + pending Rev box | `frame.rs:256 touches`, `project.rs:1733 note_frame_ops`, `refresh_pending` (`revisions.rs:246 fills_pending_row`) | 11 / 2, –, 1 / 1 | `FrameView` |
| 7 | Generated pages | `generated.rs:355 touches`, `project.rs:2259 generated_touched` (list 1: `[Contents, Revisions]` + Cover), `project.rs:2465 update_outputs` → `rewritten` (list 2, **with `_ => false`**), `front.rs:433 touches`, `front.rs:543 Uses::update` | 8 / 1, –, –, 14 / 2, 8 / 2 | `GeneratedView` |
| 8 | Devices feed (Devices list, symbol search) | `device_list.rs:84 devices_touched` | 16 / 1 | `DevicesFeed` |
| 9 | Properties feed (Properties panel, Style catalogues) | `props.rs:86 touched` (inner `_` at `:537`) | 33 / 0 | `PropsFeed` |
| 10 | Pages tree feed | `project.rs:414 tree_touched` | 6 / 0 | `TreeFeed` |

That is 22 functions and about 330 variant arms, 21 of them wildcards. The trial
(`apps/ecad-trial`) has no `Op` match of its own: its lists read the kept models' `revision` /
`build_id` or the change log (`changes_since`, `device_changes_since`, `props_changes_since`,
`page_list_version`), which stay as they are (§6). It has two scans that break the owner's rule:

- **T1.** `checks_panel.rs:172 ChecksPanel::sync` clones every row and sorts the whole list
  (`checks::sort`) on every change to the Checks or report revision, then diffs the result.
- **T2.** `lists/cables.rs:444 sync_colours` (059) works out `open_cores` for **every** cable
  whenever the change count moves while the Cables tab is shown. `sync_checks` (`:496`) filters
  every Checks row for the selected cable on every change.

### 1.3 Not views (they stay `Op` matches)

These functions read ops to *make* or *judge* a command, not to keep derived state, so they are
out of scope. Each says so in a one-line comment so a later lane doesn't move it by mistake:
`generated::refusal`, `proposals::before_move`, `oops::note_accepted`, `merging::objects_of`,
`accessories::{renumber_ops, relabels}`, `bom::renumber_for_ops`, `revisions::{inverse, issue_in}`,
`symbol_cmds::superseded`, `project::unused_after` (moves into `PagesView`'s note as a helper),
`debug::{op_view, op_targets}` (the debugger describes ops; it stays read-only), and
`designations::Uses::apply` / `TagHigh::catch_up` (indexes that catch up from the log, which
works on any path today; owner decision 9).

---

## 2. Where things live (so nothing moves twice)

Two lanes land before this one: first the types lane (StyleCatalogue enum, property keys registry
`keys.rs`), then lane 065's crate split. The split creates **ecad-electrical** (Core: the
electrical rules, depending on `ecad-model` and `ecad-tokens` only). It leaves ecad-cadcraft
(Engine) with only the CADCraft link, the commands and the interaction forms. So each piece of
this lane goes straight to its final crate and never moves:

| Piece | Crate / module | Layer |
|---|---|---|
| `Effect`, `Effects::of`, `EffectKind`, `Area`, `Key` | `ecad-model/src/effect.rs` | Core |
| `DerivedView` trait, `Phase`, `NoteCx`, `Outcome`, `ViewScope` | `ecad-model/src/view.rs` (owner decision 1) | Core |
| `Views` (the set, stages, `begin`/`finish`, `verify`), `Pending`, and one adapter file per view (`derived/<view>.rs`) | `ecad-electrical/src/derived/` | Core. The crate already refuses `cadcraft_*`, so no extra layers rule is needed. |
| `Changes` (the engine-free record: pages, named pages, feeds, list, frame, echo) | `ecad-electrical/src/derived/changes.rs` | Core |
| `apply_changes`: change log, redraw, front page | `ecad-cadcraft/src/project/changes.rs` (after the `project.rs` split, §8) | Engine |
| `DocumentOwner` (interactive commit, undo, redo) | `ecad-cadcraft/src/project/owner.rs` | Engine |
| Trial consumers (§6) | `ecad-app` (ecad-trial's library after 065 step 9) | App |

`Project` (Engine) owns one `ecad_electrical::derived::Views` and calls it from its three paths,
so `Views`, `Pending` and `Changes` are `pub` in ecad-electrical. The views' own state (the frame
stale flag, the pending flag, the cover uses, the xref reuse) moves out of `Project` into the
views.

Several modules are split in the crate map: `frame`, `front`, `generated`, `device_list` and
`xref`. In each, the update-map half reads only the document and so sits in ecad-electrical.
`props` stays in ecad-cadcraft because it builds the palette for an engine selection. Its
`touched` reads only the document, though, so it moves into `derived/props_feed.rs`. No view's
`note` needs the engine.

The cable fan-view settings (`views::Settings`) stay in ecad-cadcraft under that name. To keep
the two words apart, the new module is called `derived`.

---

## 3. `Effect`: the one classifier (ecad-model)

### 3.1 Shape

`Effect` is computed **from the ops alone**. Every op carries what it changes: whole objects for
add and remove, and `before`/`after` for modify. So the classifier needs no document and never
fails. Views look things up in the document themselves, once before the commit and once after
(§4).

```rust
/// A placed object of an ordered list: its index and value.
pub struct Placed<'a, T> { pub index: usize, pub value: &'a T }

/// One thing a transaction changed. `before: None` = created, `after: None` = removed.
pub enum Effect<'a> {
    Page { id: PageId, before: Option<Placed<'a, Page>>, after: Option<Placed<'a, Page>> },
    Layer { id: LayerId, before: Option<&'a Layer>, after: Option<&'a Layer> },
    Item { id: ItemId, before: Option<&'a Item>, after: Option<&'a Item> },
    Component { id: ComponentId, before: Option<&'a Component>, after: Option<&'a Component> },
    ComponentDefinition { id: ComponentId, before: Option<DefinitionId>, after: Option<DefinitionId> },
    Terminal { id: TerminalId, before: Option<&'a Terminal>, after: Option<&'a Terminal> },
    Rep { id: RepresentationId, before: Option<&'a Representation>, after: Option<&'a Representation> },
    Conductor { id: ConductorId, before: Option<&'a Conductor>, after: Option<&'a Conductor> },
    End { conductor: ConductorId, end: End, before: Option<TerminalId>, after: Option<TerminalId> },
    Cable { id: CableId, before: Option<&'a Cable>, after: Option<&'a Cable> },
    Member { cable: CableId, index: usize, before: Option<&'a CableMember>, after: Option<&'a CableMember> },
    Prop { object: ObjectRef, key: Key<'a>, before: Option<&'a str>, after: Option<&'a str> },
    Definition { id: DefinitionId, before: Option<&'a Definition>, after: Option<&'a Definition> },
    Settings { before: &'a ProjectSettings, after: &'a ProjectSettings },
    Version { name: &'a str, at: u64, named: bool },
    Folder { id: FolderId, before: Option<Placed<'a, Folder>>, after: Option<Placed<'a, Folder>> },
    Style { layer: StyleLayer, catalogue: StyleCatalogue, id: &'a StyleRef,
            before: Option<&'a StyleRecord>, after: Option<&'a StyleRecord> },
    Standard { before: Option<&'a StandardHeader>, after: Option<&'a StandardHeader> },
}

/// A property key with its area, classified once (§3.3).
pub struct Key<'a> { pub raw: &'a str, pub area: Area }

/// The effects of a transaction's ops, in op order, with pairs folded (§3.2).
pub struct Effects<'a> { list: Vec<Effect<'a>> }
impl<'a> Effects<'a> {
    pub fn of(ops: &'a [Op]) -> Self;
    pub fn iter(&self) -> impl Iterator<Item = &Effect<'a>>;
    pub fn kinds(&self) -> EffectKindSet;          // a bitset, for cheap "anything of kind X?" questions
}

/// Field-less mirror for tests and the update-map matrix; `ALL` lists every kind.
pub enum EffectKind { Page, Layer, Item, /* … one per variant … */ Standard }
```

`StyleCatalogue` comes from the types lane. If it hasn't landed, `catalogue` is `&'a str` and
the brief's precondition fails (§9, step 0).

**Rules.**
- `Effects::of` has the **only** `match` on `Op` that views rely on. It has no `_` arm, and
  `#![deny(clippy::wildcard_enum_match_arm)]` on `effect.rs` keeps it that way. A new `Op` doesn't
  compile until someone maps it.
- `Touched::add` (`invariant.rs:94`) is rewritten over `Effects`, so the invariant check and the
  views share one classifier. Its result is unchanged, and its tests are the first proof that
  the classifier is right.

### 3.2 Op → Effect

| Op | Effect |
|---|---|
| `AddPage` / `RemovePage` | `Page` (after / before). **A `RemovePage` and an `AddPage` of one id in one transaction fold into one `Page` with both sides**: a rename keeps the index, a move changes it. This replaces the pair detection in `frame::touches` and `front_after_history`'s callers. |
| `AddLayer` / `RemoveLayer` | `Layer` |
| `AddItem` / `ModifyItem` / `RemoveItem` | `Item` |
| `CreateDevice` / `DeleteDevice` / `ModifyDevice` | `Device` (design 072: renamed from `Component`; a type change is a `ModifyDevice`) |
| `SetDeviceDefinition` | `DeviceDefinition` |
| `AddTerminal` / `RemoveTerminal` | `Terminal`. **A remove and an add of one id fold** (brief 028's relabel). |
| `LinkRepresentation` / `ModifyRepresentation` / `UnlinkRepresentation` | `Rep` |
| `CreateConductor` / `DeleteConductor` | `Conductor` |
| `AttachEnd` | `End` |
| `CreateCable` / `DeleteCable` | `Cable` |
| `AddCableMember` / `RemoveCableMember` | `Member` |
| `SetProperty` | `Prop`, with `Key { raw, area: Area::of(key) }` |
| `EmbedDefinition` / `RemoveDefinition` | `Definition` |
| `SetProjectSettings` | `Settings` |
| `NameVersion` / `UnnameVersion` | `Version { named: true / false }` |
| `AddFolder` / `RemoveFolder` / `ModifyFolder` | `Folder` (a remove and an add of one id fold, like pages) |
| `SetStyle` | `Style` |
| `SetStandard` | `Standard` |

Folding is the only cross-op logic. It keeps op order: the folded effect stands where the first
of the pair stood. A remove and add of one id that are separated by an op naming that id (which
the invariants don't allow today) aren't folded.

### 3.3 Property keys: `Area` (routing through `defaults::AREAS`)

`defaults::AREAS` (`defaults/mod.rs:36`) is a list of 17 words that a registry key may start
with. Today views route keys with 22 hand-written `starts_with("<area>.")` tests and many
`key == X_KEY` arms. The design turns the list into an enum in ecad-model. The words don't
change.

```rust
pub enum Area {
    // The registry's areas (today's AREAS, same words, same order):
    Text, Wiring, Wire, Tags, Revisions, Print, Symbol, Terminal, Connector, StripPage,
    Bom, Location, Continuation, Plc, Page, Front, Rating,
    // Stored keys outside the registry (from the types lane's keys.rs):
    Identity,      // tag, description
    BomItem,       // part.*, maker, article_number, type
    Designation,   // designation.<aspect>.<entry>.{code,description}; stored function / location
    Book,          // page number, folder, title
    RevisionRow,   // rev.<n>.*
    Link,          // link.*
    StripAccessory,// strip_* on the project
    TagHighWater,  // the tag high-water reset prefix
    CableCores,    // unused.<core>
    ContinuationId,// connectivity name / kind
    Generated,     // generated::KEY
    Free,          // anything else: a person's own property
}
impl Area {
    pub const REGISTRY: [Area; 17];                 // defaults::AREAS becomes REGISTRY.map(Area::word)
    pub fn of(key: &str) -> Area;                   // the one place a key's prefix is read
    pub fn word(self) -> &'static str;
}
pub struct AreaSet(u64);                            // const-buildable bitset
```

- `defaults::AREAS` becomes `Area::REGISTRY` words. The key-rule test in `defaults/mod.rs` keeps
  passing, and the doubled `strippage.` doc line (`:29-30`, an old `merge=union` leftover) goes.
- The non-registry rows above are a starting set. The worker takes the exact list from the types
  lane's `keys.rs`. A test checks that every key constant in `keys.rs` and every defaults entry
  classifies to something other than `Free`.
- **Routing.** Each view declares `const AREAS: AreaSet`. `Views` drops a `Prop` effect whose
  area isn't in the set before it calls that view's `note`, so a view never sees property writes
  it doesn't read. A view that reads people's own properties (the Properties feed, BOM
  attributes) includes `Area::Free`.
- In `derived/`, an `xtask layers` ratchet refuses `starts_with("` and `split_once('.')` on
  property keys, so routing goes through `Area`. A finer test within an area (one key) is
  written as `key.raw == keys::X`.

---

## 4. `DerivedView`: the trait

```rust
pub enum Phase { Before, After }

/// What `note` may read besides the effect.
pub struct NoteCx<'a> {
    pub doc: &'a Document,        // Before: the state before the ops; After: the state after
    pub phase: Phase,
    pub effects: &'a Effects<'a>, // the whole transaction (for pair-wide questions such as `unused_after`)
}

/// A view's scope: what it must redo. `Default` is "nothing".
pub trait ViewScope: Default + Send {
    fn is_empty(&self) -> bool;
    /// The scope asks for a whole rebuild (allowed only for the ops in the update-map allowlist, §7).
    fn is_all(&self) -> bool;
}

/// What a view's update reports to the pipeline.
#[derive(Default)]
pub struct Outcome {
    pub pages: PageSet,           // pages whose drawing changed: Some(set) or All
    pub frame_stale: bool,        // the kept frame must be drawn again
    pub list: bool,               // the page list / tree must be read again
    pub problems: Vec<String>,    // verify-mode disagreements (recorded as IndexRebuilt)
}

pub trait DerivedView: Send {
    type Scope: ViewScope;
    /// Its name in the update-map matrix, counters and the debugger.
    const NAME: &'static str;
    /// The property areas it reads (§3.3).
    const AREAS: AreaSet;
    /// Whether it is kept now. A view built on first show (Checks, report models) returns false
    /// until then, and the pipeline then skips it entirely.
    fn kept(&self) -> bool { true }
    /// Add to `scope` what `effect` could change, read in `cx.doc`. Called once per effect in the
    /// Before phase (on the document before the ops) and once in the After phase (after them).
    /// Reads `&self` only: kept state such as an index may be consulted, never changed.
    fn note(&self, cx: &NoteCx<'_>, effect: &Effect<'_>, scope: &mut Self::Scope);
    /// Bring the kept state in line with `doc` for `scope` only.
    fn update(&mut self, doc: &Document, scope: Self::Scope, inputs: &Inputs<'_>) -> Outcome;
    /// Verify mode: compare with a fresh build; `Some(why)` if they differ (the pipeline rebuilds).
    fn verify(&self, doc: &Document) -> Option<String>;
}
```

- `Inputs` carries what a later stage reads from an earlier one: the report models' outcome for
  `GeneratedView`, the kept PLC model for `PagesView`, and the connector index's named pages for
  `RefsView`'s outcome. Stage order makes those reads safe (§5).
- `note` is the view's update map. A view that matches on `Effect` lists every kind it ignores
  in an explicit arm (`Effect::Layer { .. } | Effect::Version { .. } => {}`). The `derived` module
  has `#![deny(clippy::wildcard_enum_match_arm)]`, so a new `Effect` kind sends the worker to
  every view.
- **Why the old protocol fits.** `checks.rs`, `outputs.rs` and `xref.rs` each already run
  "collect on the state before, collect on the state after, update". `DerivedView` is that
  protocol written once. The numbering pass's extra ops (and an undo's own transaction) reach
  `note` in the After phase, because the After phase reads the committed transaction's ops. That
  is why D3's `with_numbered` and `numbered_pages` disappear.

### 4.1 Each view on the trait

| View | Adapts | Scope (today's type, kept) | `note` (gist) | `update` → `Outcome` |
|---|---|---|---|---|
| `ChecksView` | `checks::Checks` (+ `rating::Kept`) | `checks::Scope` | today's `Scope::collect` over `Effect`, plus page types (`Prop` area `Page`, `Page` effects → `all`), styles (`Style`, `Standard`, `Prop` area `Text`/`Print`/`Wire`), title blocks, `rating::note` (`Prop` area `Rating`, `Wire`, `Location`; `End`; `Member`; `Style`). `AREAS`: Rating, Wire, Wiring, Location, Designation, Page, Print, Text, Front, Identity, Connector, Plc, Terminal, Continuation, ContinuationId, CableCores, Tags | `Checks::update`; also the cover rows (`set_cover`) when a cover `Page` effect is seen. `list` when the revision moved. |
| `OutputsView` | `outputs::Outputs` (rows, `strips`, `plc`, `strip_pages`) | `outputs::Scope` + `plc::Touch` | today's `Scope::collect`, `moves_page_numbers`, `plc::touched`, `rack_places_touched` | `Outputs::update`; `pages` = the generated pages whose kind's rows changed, by `GenKind` (§4.2); `frame_stale` on a sheet-count change or `numbers_moved` (today's `update_outputs` rule) |
| `RefsView` | `xref::RefIndex` (+ `references::Snapshot`, continuation names, the `xref_shown` reuse) | `xref::Watch` (made `Default`) | `xref::Scope::collect`, `cables_touched`, `names_named` over `Effect` | `Watch::finish`; `pages` = reference pages; the orphan line goes into `Outcome.problems`' sibling `echo` (§5) |
| `ConnectorsView` | `connectors::Index` | `BTreeSet<ComponentId>` + named keys | `Index::touched`, `pages_for` (`Prop` area `Connector` on `Project`) | `Index::update`; `pages` = `pages_for` |
| `PagesView` | nothing kept (a pure feed) | `PageSet` + `named: BTreeSet<PageId>` | `pages_touched` (with `plc::pages_touched`, `styles::pages_touched`, `unused_after` via `cx.effects`), `pages_named` | `pages`; `named` goes to `Changes::named` for `front_after_history` |
| `FrameView` | `Project.frame` / `frame_stale` / `pending` | `bool` | `frame::touches` over `Effect` (the folded `Page` effect replaces its pair search; `Settings` sheet/unit; `Prop` areas Text, Print, Revisions, RevisionRow, Designation, Book; `Style` Frame/Text/Page; `Standard`; `Folder`; `Version`) | `frame_stale`; `refresh_pending` runs here (O(1) as today) |
| `GeneratedView` | `cover_uses` (`front::Uses`) | per-`GenKind` flags | `Contents`: `Page`, `Folder`, `Prop` area `Book`; `Revisions`: `Prop` on `Project` area `Revisions`/`RevisionRow`, `Version`; `Cover`: `front::touches` + `Uses` | an **exhaustive `match GenKind`** over `GenKind::ALL` (§4.2); `pages` = the pages of each changed kind; `frame_stale` when the shown frame is such a page |
| `DevicesFeed` | the change log's `DeviceTouch` | `DeviceTouch` | `devices_touched` | a `DeviceTouch` in `Changes` |
| `PropsFeed` | the change log's `ObjectRef` set | `Option<BTreeSet<ObjectRef>>` | `props::touched` (`AREAS` = every area, `Free` included) | `Changes::props` |
| `TreeFeed` | `page_list_version` | `bool` | `Page`, `Folder`, `Prop` area `Book` (`tree_touched`) | `list` |

The three feeds keep nothing themselves. They stay views because their scope *is* the change
log's record. Making them views also puts D3's numbering ops into them through the After
phase.

### 4.2 Generated pages: one exhaustive list

Today two hand-kept lists decide which generated pages redraw: `generated_touched` (`:2264`,
Contents, Revisions, then Cover) and `update_outputs`' `rewritten` (`:2490`, five kinds and
`_ => false`). A new `GenKind` that is in neither list never redraws. The design replaces both
with one function in `derived/generated.rs`:

```rust
fn changed(kind: GenKind, s: &GenScope, outputs: &OutputsOutcome, page: PageId) -> bool {
    match kind {                                   // no `_`: a new GenKind does not compile
        GenKind::Cover => s.cover,
        GenKind::Contents => s.contents,
        GenKind::Revisions => s.revisions,
        GenKind::BomLocation => outputs.bom_rows_changed,
        GenKind::Terminals => outputs.terminals_rewritten(page),
        GenKind::Cables => outputs.cables_changed,
        GenKind::IoList => outputs.io_rows_changed,
        GenKind::PlcRack => outputs.rack_changed,
    }
}
```

The page list is read only when some kind changed, as it is today. A contract test runs every
`GenKind::ALL` member through a trigger op and checks that its page is in `Outcome.pages`.

---

## 5. `Views`: one pipeline, three paths

```rust
pub struct Views {
    // Stage 1: indexes that depend only on the document.
    pub refs: RefsView, pub connectors: ConnectorsView,
    // Stage 2: models.
    pub checks: Lazy<ChecksView>, pub outputs: Lazy<OutputsView>,
    // Stage 3: pages, which read stage 1 and 2 outcomes through `Inputs`.
    pub pages: PagesView, pub frame: FrameView, pub generated: GeneratedView,
    // Stage 4: feeds for the change log.
    pub devices: DevicesFeed, pub props: PropsFeed, pub tree: TreeFeed,
}

pub struct Pending { /* one Scope per kept view, from the Before phase */ }
pub struct Changes {
    pub pages: PageSet, pub named: BTreeSet<PageId>, pub devices: DeviceTouch,
    pub props: Option<BTreeSet<ObjectRef>>, pub list: bool, pub frame_stale: bool,
    pub echo: Vec<String>, pub problems: Vec<String>,
}

impl Views {
    /// Before phase: every kept view notes `ops` on the document before them.
    pub fn begin(&self, before: &Document, ops: &[Op]) -> Pending;
    /// After phase on the committed transaction's ops, then update every kept view, stage by stage.
    pub fn finish(&mut self, after: &Document, tx_ops: &[Op], pending: Pending) -> Changes;
    /// Verify mode: every kept view against a fresh build (today only xref, connectors, strips).
    pub fn verify(&mut self, doc: &Document) -> Vec<String>;
    /// Open, reload, branch switch: rebuild every kept view; first show builds a `Lazy` one.
    pub fn rebuild(&mut self, doc: &Document);
}
```

`Lazy<V>` holds `Option<V>`, built on first show as `Project::checks()` and `outputs()` are today.
A view that isn't built yet is skipped in both phases, at no cost.

**Every path, the same five lines** (inside ecad-cadcraft's `project/changes.rs`, calling
ecad-electrical's `Views`):

```text
let pending = self.derived.begin(before_doc, &ops);          // nothing changes yet
<path-specific commit>                                        // commit_extended | commit_merge | undo_at | redo_at
  └─ refused → drop `pending`, record Refused, return         // no view was touched: nothing to restore
<path-specific immediate drawing>                             // interactive: refresh_page(edited page) as today
let changes = self.derived.finish(after_doc, last_tx_ops, pending);
self.apply_changes(pg, command, changes, Path::{Command|Interactive{page}|History{order_before}});
```

`apply_changes` is the single place for what follows: the change log (`record_change`),
`page_list_version`, `frame_stale`, `goto_ring`, echoing orphan lines and notes, and then the
redraw. That's `redraw_touched` for Command, the edited page plus `redraw_touched` for
Interactive, and `front_after_history` with `Changes::named` for History. What stays outside
the pipeline is path-specific and isn't derived state: the rack-page ops added before a command
(`plc::page::rack_page_ops`), `generated::refusal`, clip capture, OOPS, `before_move` and
`move_follow_up`, and the issue and merge undo questions.

**Divergences closed.** D1: `FrameView` runs on every path. D2: `PagesView` runs on the
interactive path too, so the change log names the conductor's other pages. D3: the numbering
ops arrive in the After phase. These are the only behaviour changes, and each gets a test
(§10). Today `note_frame_ops` sets `frame_stale` before the commit and puts it back on a
refusal. In the new pipeline the flag is set only in `finish`, so nothing has to be put back.

### 5.1 Parallelism (the `parallel` feature)

The owner wants rayon for views, checks and parsing, and the approved crate map allows it in
ecad-electrical. ECAD is desktop-only, so every target can build it. The design makes parallel
updates **possible without making them necessary**:

- **Before and After phases:** `note` takes `&self` and `&Document` and writes only its own
  scope, so each view's notes are independent. Under the feature they run as one task per view.
- **`finish`:** views in one stage have disjoint fields (`&mut` each) and read only earlier
  stages through `Inputs`, so a stage's updates can run in parallel. The stages themselves run
  in order.
- **Determinism:** each view's result depends only on its scope and the document. `Changes` is
  merged in the fixed field order, so a run with the feature on equals a run with it off, and a
  test checks that (§10).
- **One switch.** `Views` iterates stages through one private helper (`for_each_view`). Without
  the feature it's a plain loop. With `--features parallel` (an ecad-electrical feature) it's
  `rayon::scope`. The feature is simply on or off. It stays **off by default until the bench shows
  it helps**: the `views.*` rows of §10, run both ways. Turning it on by default is then a
  one-line change for the overseer to approve.
- **Precondition:** `Document: Sync` and every view `Send`. Step 0 asserts both at compile time.
  If `Document` isn't `Sync` (a lazily filled cache, for example), the feature isn't offered, and
  the overseer is told which field blocks it.
- Checks' and reports' **internal** parallelism (per net, per strip) is a later lane's work
  inside one view's `update`. It doesn't depend on this pipeline.

### 5.2 One way in

*Approved amendment (owner, 2026-11-15, answering "should this be architected out?": yes, in R9).*

Today a derived view can be reached in three ways. Each one has to be threaded separately when
an engine-supplied input such as lane 067's `TextMetrics` is added:

1. **The kept view**, through the project: `Project::outputs()` and `checks()`, and the `xref`,
   `connectors` and `cover_uses` fields. That is about 127 call sites for outputs (the overseer's
   count). Mine: 45 `.outputs()`, 11 `.checks()`, and about 118 direct field reads of
   `.outputs` / `.checks` outside tests.
2. **A full rebuild from a document** (`X::build(doc)`), outside the kept view.
3. **Associated or free helpers** that work out part of a view on their own.

**Rule.** Every derived view has one way in: `Views`. The project owns one `Views`. It is built
once, with the engine's inputs, on open, reload and branch switch (or on first show for a `Lazy`
view), and after that it is updated only by the pipeline (§5). Callers read views through the
project (`project.views().outputs()`, …).

- **Engine inputs.** `Views::new(inputs: EngineInputs)`. `EngineInputs` is defined in
  ecad-electrical and carries `Arc<dyn TextMetrics>` (the trait as lane 067 defines it) and any
  later engine-supplied input. ecad-cadcraft fills it in. A view that needs metrics reads them
  from `Inputs` in `update`, never from a global and never from a parameter threaded through
  callers.
- **Another document state.** Releases and re-releases need the reports and checks of an *issued*
  or *finalised* state, not the current one. Today `release.rs` builds those with
  `Outputs::build(&then)` (`:1027`, `:1255`, `:1416`). They get one entry point,
  `Views::detached(&self, doc) -> Detached`. It builds only the views the caller asks for
  (outputs, checks) for that document, with the **same** `EngineInputs`. The result is not kept
  and not updated, and is dropped after use. The debugger's on-request checks (`verify.rs:163`)
  use it too.
- **The test oracle.** `same_as_built`: a full rebuild compared with the kept view. It exists
  only behind the `test-support` feature (lane 062) and in verify mode (`Views::verify`, §5). It
  is not public API for apps or the bench. **The bench measures the pipeline**: the edit rows of
  §10, and open / first show through `Views::rebuild`, which is the one legitimate whole build.
- **Helpers.** Table and CSV helpers are pure functions over a view's rows, needing no metrics,
  or methods on the view. `outputs::csv(&Table, bool)` and `rows_json(&Table, …)` already have
  that shape and stay. `Outputs::{bom_table, wire_table, cable_table}` (`outputs.rs:2380-2511`)
  are already methods on the view and stay. Free functions that take a `&Document` and work out
  rows on their own become methods on the view or `pub(crate)` rule functions under it.
- **Guard.** An `xtask layers` ratchet counts direct `::build(` calls on a view type (the list
  below) outside `tests/`, `Views` itself and the `test-support` oracle. The count is checked in,
  can only fall, and reaches **0** by the end of this lane (step 10, test C13).

**Inventory** (on `dev` at `445787b8`, outside `tests/`):

| View | (1) kept | (2) direct full builds outside the kept view | (3) side helpers | After 064 |
|---|---|---|---|---|
| `Outputs` (report models) | `Project.outputs` / `outputs()` | `generated.rs:1047` (`parts_of_pending` builds one whenever the caller passes `None` and a BOM page exists: every projection of such a page while Reports was never shown); `checks.rs:2291` (`checks::all`); `release.rs:1027, 1255, 1416` (issued and finalised states); bench `bom.rs:99` | `csv`, `rows_json` (pure, keep); `end_points`, `gap_pe`, `accessory_checks`, `accessory_fix_commands` (free fns over parts of the view) | `views.outputs()`. Projection gets the kept view through `Inputs`, and the `None` fallback goes. Release uses `detached`. Bench via the pipeline. Free fns become methods or `pub(crate)`. |
| `Checks` (+ `rating::Kept` inside) | `Project.checks` / `checks()` | `checks.rs:2296` (`all_with` **rebuilds Checks** every call, so the `checks` command (`wire_cmds.rs:679`) builds the whole list per request even though the project keeps one); bench `measure.rs:355, 398`, `ratings.rs:66` | `sort` (pure, keep); `all` / `all_with`; per-subject rule fns (`net_checks`, `conductor_checks`, …, `pub`) | `checks` command reads `views.checks()` plus outputs' rows. `all*` go, and `detached` covers release. Rule fns become `pub(crate)` (R7's `CheckSource` owns them). |
| `RefIndex` (references) | `Project.xref` | `project.rs:1176, 1273, 1411` (open ×3), `:1546` (branch switch); `xref.rs:1566, 1840, 1848` (verify and the wide case, internal); bench `measure.rs:737` | `references(doc, idx, …)`, `goto_target(doc, idx, …)` take the index (fine as methods) | Open and switch through `Views::rebuild`. Internal rebuilds become `ViewScope::is_all` / `verify`. Index-taking fns become methods. |
| `connectors::Index` | `Project.connectors` | `project.rs:1177, 1274, 1412, 1548` (open ×3, switch); `:2417-2418` (verify) | – | As `RefIndex`. |
| `Plc`, `Strips`, `StripPages` (sub-models of `Outputs`) | inside `Outputs` | `outputs.rs:804-806`, `:1084-1110` (inside the view: fine); bench `plc.rs:162`, `strips.rs:291, 294` | – | Reached only through `views.outputs().plc` / `.strips`. Bench via the pipeline. |
| `front::Uses` (cover) | `Project.cover_uses` | `project.rs:2273`, `front.rs:555` (inside the update) | – | Inside `GeneratedView`. |
| Device list, props, tree feeds | no kept model (change log) | – | – | Already one way in. |

So beyond outputs, **Checks** (the `checks` command's per-request rebuild), **RefIndex** and the
**connector index** (three open paths each, rebuilt by hand), and the **generated-page projection**
(`parts_of_pending`'s hidden rebuild) have more than one way in. The PLC and strip models are
reached around `Outputs` only by the bench.

---

## 6. Trial consumers

The trial's lists keep reading revisions and the change log. They gain nothing from a trait
because they live on the UI thread and copy from the kept models. Two fixes are in scope because
they break the owner's rule (owner decision 5):

- **T1, the Checks panel.** `Checks` keeps its rows in book order. The order changes only with
  `Scope::relocate` (page numbers or folders), which already exists. `Checks::update` returns a
  row delta (keys added, keys removed). `ChecksPanel::sync` patches its copy from the delta and
  sorts only on `relocate` or a new Show filter. The report models' location rows, merged into the
  panel today, give their own delta the same way. A counter `ChecksPanel::sorts` lets the tester
  check that an ordinary edit sorts nothing.
- **T2, the Cables tab.** `Checks` gains `rows_of(Object) -> &[Check]`, an index kept by the same
  update, which replaces `sync_checks`' filter. The open cores per cable come from the kept data
  (`OutputsView`'s cable rows carry `open: Vec<String>`, updated for the cables in its scope)
  instead of `open_cores` over every cable, so `sync_colours` reads at most the selected cable
  again.

`changes_since`, `device_changes_since` and `props_changes_since` keep their signatures, so the
preview, Devices list, Properties panel and Style catalogues don't change.

---

## 7. The update-map guarantee

Owner rule: derived views scale with what changed, and the whole project is never rebuilt on an
ordinary commit. Each view ships an update map. The design makes the map **checkable**, not just
documented.

1. **By construction.** A view's update map *is* its `note`. There's no other path into its
   scope. A whole rebuild happens only through `ViewScope::is_all()` or `Views::rebuild` (open,
   reload, branch switch, first show).
2. **The matrix.** A test builds one sample transaction per `Op` variant (on the `ProjectBuilder`
   fixture of lane 062 where it has landed, otherwise a local fixture). For each kept view it
   records one of `–` (empty scope), `scoped` or `ALL`. The result is compared with a checked-in
   table in `docs/design/064-update-maps.md`, which `cargo xtask update-maps` writes. The doc
   and the test can't disagree, in the same way the tokens doc-sync works. The reviewer reads
   this table once per feature to check the rule.
3. **The allowlist.** `ALL` is allowed only in a short list kept in the test, each entry with a
   reason. Today's real cases are: a page-type checks-off change (`page_types::checks_changed`), a
   folder move of a page (`pages_touched` returns `None`), a project-wide default that a style
   resolves to (`styles::pages_touched` returns `None`), a designation code rename that is the
   project's default, and `SetStandard` with a different standard. Any other `ALL` fails the test.
4. **Counters.** The existing counters (`frame_builds`, `projections`, `Outputs::build_id`,
   `sorts`, `continuation_builds`, the trial lists' `rebuilds`) are joined by one counter per view
   for `update` calls and whole rebuilds. These are cheap and are read by tests and the bench.

---

## 8. `project.rs` split

`project.rs` has 3,289 lines. The split is **step 1** of the brief: a pure move, so the pipeline
diffs that follow land in small files. Visibility doesn't widen: items become `pub(crate)` or
`pub(super)` where they were private.

| New file | Takes (today's lines) |
|---|---|
| `handle_map.rs` | `HandleMap`, `Mapped` and their impls (`:63-300`) |
| `page_tree.rs` | `PageView`, `PageRow`, `TreeRow`, `tree_rows` (`:305-400`). `tree_touched` goes in step 3 (it becomes `TreeFeed`). |
| `persistence.rs` | `Persistence`, `MemoryStore`, `Faults`, `ClipSnapshot`, and the save and branch plumbing: `save_now`, `save_pending`, `save_file`, `bind`, `adopt_branch`, `sync_saved` |
| `project/mod.rs` | `struct Project`, its accessors, the projection helpers (`project_page*`, `base_drawing`, `part_entity`…) |
| `project/open.rs` | `create`, `create_with`, `open_from`, `open_file`, `open_file_in`, `open_file_with` |
| `project/changes.rs` | `apply_edit*`, `apply_merge`, `apply_tx`, the change log (`Change`, `record_change`, `changes_since`, `device_changes_since`, `props_changes_since`, `page_list_version`), `refresh_page(s)`, `redraw_*`, and later `apply_changes` and the `derived` hookup |
| `project/owner.rs` | `impl DocumentOwner for Project` (commit, undo, redo, save, closed, drag_extra, overlay, recover) and `history` |

`pages_touched`, `pages_named`, `unused_after`, `generated_touched` and `update_outputs`/`update_checks`
are not moved by step 1. They're deleted when their view lands (§9). `front_after_history` and its
test move to `project/changes.rs`.

---

## 9. Step order (the brief has the detail)

Every step builds, passes clippy and passes the tests of the crates it touches. The old function
and the new view never both feed the change log: each view step deletes the old call in all three
paths in the same commit.

0. **Preconditions.** R0's `Op::ALL`/`Op::name` coverage has landed, along with the types lane's
   `StyleCatalogue` and `keys.rs`, and so has the crate split (065) with `ecad-electrical`. A
   compile-time assert for `Document: Sync`.
1. **Split `project.rs`** (§8). A pure move.
2. **ecad-model.** `effect.rs` (Effect, `Effects::of`, `Area`, `AreaSet`) and `view.rs` (the
   trait). `Touched::add` rewritten over `Effects`. `defaults::AREAS` derived from `Area`. In
   ecad-electrical, `derived/` with an empty `Views` and `Changes`. In ecad-cadcraft, all three
   paths call them already (`begin` / `finish` / `apply_changes`).
3. **Feeds:** `TreeFeed`, `DevicesFeed`, `PropsFeed`. `with_numbered` goes.
4. **`FrameView`** (D1 closed). `note_frame_ops` goes.
5. **`ConnectorsView`**, then **`RefsView`** (xref, cable references, continuation names, reuse).
6. **`PagesView`** (D2 closed). `pages_touched`, `pages_named`, `numbered_pages` go.
7. **`OutputsView`** (plc, strips, strip pages, rack places). `update_outputs` goes.
8. **`ChecksView`** (page types, styles, title blocks, rating). `update_checks` goes.
9. **`GeneratedView`** with the exhaustive `GenKind` match. `generated_touched`, `generated::touches`,
   and the `rewritten` list go.
10. **The matrix** (§7), `xtask update-maps`, the wildcard deny on `derived/`, the `starts_with`
    ratchet.
11. **Trial consumers** (§6): the Checks row delta and `rows_of`, the Cables open cores.
12. **Parallel seam** (§5.1): `for_each_view` and the `parallel` feature, off by default, with
    the bench run both ways.

---

## 10. Tester plan

A tester is required, because the lane touches the commit path, `translate.rs`'s caller and the
store's commit. Tests go in `tests/all/lanes/lane_064.rs` (ecad-model, ecad-electrical for the
views, ecad-cadcraft for the three paths) and in
the trial's lane module for §6.

| # | Test | Checks |
|---|---|---|
| C1 | `every_op_has_an_effect` | One sample per `Op::ALL` → `Effects::of` gives ≥ 1 effect of the expected `EffectKind`. Rename, move and relabel pairs fold. |
| C2 | `touched_is_unchanged` | `Touched::of(ops)` equals the old result for every sample and for a recorded corpus (the quick-start trial's log, the medium bench project's log). |
| C3 | `areas_cover_the_keys` | Every `keys.rs` constant and every defaults entry → `Area` ≠ `Free`. `Area::REGISTRY` words = the old `AREAS`. |
| C4 | `update_map_matrix` | `Op × view` → `–` / `scoped` / `ALL` equals `064-update-maps.md`. `ALL` only for the allowlist. |
| C5 | `kept_equals_built` | For each commit of the corpus (commands, drawing commits, merges, Restore, undo and redo of each), `Views::verify` finds no difference for any kept view. |
| C6 | `three_paths_agree` | The same edit as a command, as a drawing commit (headless), and as an undo followed by a redo gives the same `Changes.pages`, `devices`, `props` and `list` (modulo the edited page, which Interactive always draws). This covers D1, D2 and D3. |
| C7 | `refused_commit_leaves_views` | A refused command or drawing commit leaves every view's revision and counters as they were, and doesn't mark the frame. |
| C8 | `every_generated_kind_redraws` | For each `GenKind::ALL`, a trigger op puts its page in `Changes.pages`, and an unrelated op doesn't. |
| C9 | `checks_panel_patches` | Ordinary edits: `ChecksPanel::sorts` doesn't move, and rows are added or removed by delta. A page renumber sorts once. |
| C10 | `cables_tab_reads_one_cable` | With the Cables tab shown, an edit to cable A reads no other cable's open cores (counter). |
| C11 | `parallel_equals_serial` | Only with `--features parallel`: C5's corpus gives identical `Changes` and kept views. |
| C13 | `no_direct_builds` | The `xtask layers` ratchet (§5.2) counts direct `::build(` calls on view types outside tests, `Views` and the oracle: 0 at the end of the lane. A release of an issued revision gives the same report rows through `Views::detached` as before. |
| C12 | existing suites | ecad-model, ecad-electrical, ecad-cadcraft, ecad-mcp and ecad-app (ecad-trial) fast tests unchanged. The 059 and 058-adjacent update-map tests (`lists::*` counters) unchanged. |

No command is added or changed, so the MCP contract-test rule (brief 011) adds no new test. The
existing `ecad-mcp` contracts must still pass, because they drive the Path A pipeline.

**Bench rows** (`ecad-bench`, medium while building, large once at the end, with Checks and
reports shown, which is the expensive case):

| Row | Operation | Target |
|---|---|---|
| `views.move_symbol` | interactive move of one symbol | no worse than `dev` +10% |
| `views.setprop_tag` | `setprop` tag on one device | no worse than `dev` +10% |
| `views.paste_200` | paste of 200 objects | no worse than `dev` +10% |
| `views.undo_redo_paste` | undo then redo of the paste | no worse than `dev` +10% |
| `views.overhead` | `begin` + `finish` with every view kept and an empty-effect transaction (a `NameVersion`) | < 0.2 ms medium |
| `checks_panel.sync` | the panel sync after `views.setprop_tag` | < 1 ms large (today it sorts every row) |

---

## 11. How lane 058 (paged reports) builds on this

058's design (`docs/design/058.md` §6–§7, on `feature/paged-reports`) plans a `UnitDelta` per
report kind. It runs "inside `update_outputs` or with `generated::touches`", and both of those go
away here. After 064:

- **The report models report units.** `OutputsView`'s `Outcome` side-data (`OutputsOutcome`,
  §4.2) carries one `UnitDelta` per report kind instead of today's "page changed" bits.
  `GeneratedView::changed` reads `!delta.is_empty()`.
- **Layouts are views.** Each report layout (BOM, terminal plans, cable schedule, I/O list, rack,
  and also Contents and Revisions) is a stage-3 `DerivedView` named `ReportLayout<K>`. Its `Scope`
  is the `UnitDelta` it collects: from `Inputs` for model-backed kinds, and from its own `note` on
  `Page`, `Folder`, `Prop`(Book) and `Version` for Contents and Revisions. Its `update` runs
  `relayout_from` (058 §7.2) and returns the dirty sheets in `Outcome.pages`, with `frame_stale`
  on a sheet-count change. That is 058's own rule 3.
- **Sheet blocks stay presentation.** The LRU of built sheets (058 §6.1) isn't a view. It's
  invalidated by the dirty set in `Changes`, and built when shown.
- So 058 writes no `Scope` protocol of its own, and its update maps join the matrix (§7) for free.
  058's designer amends §6.1 and §7's opening paragraph once 064 is approved (owner decision 7).

---

## 12. Out of scope

- Any change to ops, the `.ecad` format, commands or what a person sees.
- Turning the trial lists into `DerivedView`s (they're UI copies, §6).
- Parallel internals of Checks or reports (§5.1).
- Tag high-water and designation uses, which catch up from the log (owner decision 9).
- Narrowing `Document`'s `pub` fields (R10, after this lane).

## 13. For the overseer (ops and format: not decided here)

1. **No op or format change is needed.** `Effect` is computed from today's 33 ops.
2. *Possible later op.* A page rename or move is recorded as `RemovePage` + `AddPage` of one id,
   and `Effects::of` folds the pair. A `ModifyPage { before, after }` op would make the fold
   unnecessary. It would be a new op and a format addition, so it's listed, not proposed.
3. *Possible later op.* `SetStyle.catalogue: String` becomes `StyleCatalogue` in the types lane,
   with the same serialised words. 064 only consumes it.
4. *Possible later typing.* `SetProperty.key: String` could carry a `PropKeyBuf`. That isn't
   needed: `Area::of` classifies the string once per effect.

## 14. Owner decisions

**Approved (owner, 2026-11-15): all ten as recommended.** That includes D1–D3 closed in this
lane, T1 and T2 fixed in step 11, and the bench targets in decision 10.

1. **Where the trait lives.** `Effect` and `DerivedView` both go in ecad-model (`effect.rs`,
   `view.rs`), and `Views` goes in `derived/`. *Recommended.* *Amended for the crate map:*
   `derived/` is in ecad-electrical from the start (no move), not in ecad-cadcraft. The alternative
   is the trait in `derived/`, which moves with it. That keeps "views" out of ecad-model but
   moves the trait once.
2. **Exhaustive notes.** `#![deny(clippy::wildcard_enum_match_arm)]` on `effect.rs` and
   `derived/`, so every view names every `Effect` kind it ignores. *Recommended.*
3. **Parallelism.** Build the seam now (§5.1), with rayon behind a `parallel` feature of
   ecad-electrical, off by default until the bench shows it helps. *Recommended.* *Amended:* ECAD
   is desktop-only, so the wasm condition is gone, and the crate map allows rayon.
4. **Split `project.rs` first**, as a pure-move step 1. *Recommended.* The alternative is to
   split it last, which makes the pipeline diffs larger but leaves `project.rs` stable longer.
5. **Fix T1 and T2 here** (the Checks panel's full sort and the Cables tab's scan of every cable),
   or in a follow-up lane. *Recommended: here (step 11).* They break the same rule this lane
   enforces, and the row delta comes naturally from `ChecksView`.
6. **The update-map matrix as a generated doc** (`064-update-maps.md`, written by
   `cargo xtask update-maps`, compared by a test) rather than a test-only table. *Recommended:*
   the reviewer reads it once per feature.
7. **058 builds on `DerivedView`** (§11), and its design is amended after 064's approval. 058a
   starts after 064 step 9. *Recommended* (the review's decision 1).
8. **Close D1–D3**, so interactive commits get the frame and page maps and the numbering ops
   reach every feed. This is a small behaviour change, each part with a test. *Recommended.*
9. **Leave tag high-water and designation uses out of `Views`.** They catch up from the log,
   which is already correct on every path. *Recommended.* The alternative is to fold them in
   as stage-1 views for uniformity.
10. **Bench targets** as in §10: no worse than `dev` +10% on the edit rows, and pipeline
    overhead under 0.2 ms on medium.
11. **One way in** (§5.2), *approved amendment, owner, 2026-11-15*. Views are reached only
    through `Views`, which is built with `EngineInputs`. `Views::detached` covers other document
    states. A full rebuild exists only as the test oracle, and a ratchet takes direct builds to
    0.
