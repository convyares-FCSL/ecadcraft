Historical record (2026-11-15). Not current; see [design/cables.md](../../design/cables.md).

# Design 059: the cable manager

Status: Approved (owner, 2026-11-15: §14 answered; overseer, 2026-11-15: FD1–FD5 approved). The
worker starts after #108 (057) merges.
Brief: `docs/briefs/059-cable-manager.md` (Approved). Roadmap 2.7. UI rev 4, board **2.06**
(Project ▸ Cables). The rev 4 handoff has no separate cable-manager board (2.07 is Power). 059 fills
in 2.06's own actions: its footer (**Change cable type…**, **Add a core**, the delete icon), the
editable Part and Ends sections, and the glands under the cable view.

Builds on 004 (slash, a wire in at most one cable), 026 (tags, `-W?1` provisional), 028 (core
offer, `cores`, `type` notation, `too-many-cores`, `no-core-count`), 032 (continue a cable on
another page), 052 (the device is the master; location and function are entry keys from the
project's list), and 057 (`feature/ratings`: the Cables tab, Details pane, splitter, cable view,
core table, Not used tick, `core-open`, the `cable` catalogue and `cable_type`). Follows `008.md`
§2: one commit is one JSON-equivalent transaction, Esc writes nothing, feedback in words, never
crash. Panels use only `ecad_ui` components (`components.md`).

## 0. Facts found in code (origin/dev `22c924d1`, origin/feature/ratings `047303b1`)

- `Cable { properties, members: Vec<CableMember> }` (`ecad-model/src/doc.rs`). Nothing requires a
  member: `CreateCable` with no members is a valid op, and `ecaddelete {cable}` already emits
  `DeleteCable` with an empty member list after removing the members.
- `cableadd {conductors, cable?}` adds wires to an existing cable and gives each the next spare
  core of its offer (`strips::cable_cores`). It works for a cable with no members today.
- `cableslash {from, to, cable?|tag?}` with an existing cable runs **continue mode**
  (`references::run_slash_continue`, 032 §3.3). Each crossed wire must continue a core through a
  continuation, so a cable with no members is refused ("does not continue a core"). **This is the
  one behaviour 059 changes** (§4.3).
- `styles::cable::open_cores` makes every offered core with no member a `core-open` error unless
  ticked Not used (057, owner rule). A built cable with a `4G1.5` type would show four errors
  before it is drawn. §6 changes that.
- The cable list rows come from `Outputs::cable_table` (the cable report). Its update map already
  patches one cable on `CreateCable`, `DeleteCable`, member ops and `SetProperty` on a cable
  (`outputs.rs` header). The worker confirms that a cable with no members gets a row, and adds it
  if not.
- Cable properties in use: `tag`, `type` (free text, `H07RN-F 4G1.5`), `cores` (offer), `length`,
  `cable_type` (057), `unused.<core>` (057). 057's `cable` catalogue has `cross_section`,
  `material`, `max_temperature`, `conductor_class`, `rated_current`, `rated_at`, `core_colours`,
  `part_number`, `maker`, `description`. 057 §4.4 reserves core notation, shield and pairs for 2.7.
- There is no `CmdError` yet (code review R1). Commands return `Result<_, String>` through `err`.

## 1. Words

- **Built cable**: one made in Project ▸ Cables. It is an ordinary cable. "Built" only says where
  it started.
- **Not drawn**: a cable with no members and no representation. It is shown as `not drawn`, never
  as an error.
- **Assign**: make wires the cores of an existing cable, by slashing them or with Add to cable.
- **Not assigned** (a core): a core in the cable's offer that no wire is yet. On a drawn cable this
  is 057's unwired `core-open`, with the wording "not assigned" (§6).
- **Planned end**: where each end of a cable is meant to be, as a location and a function
  (`=PUMP +CAB1` at the cabinet end, `=PUMP +FIELD` at the field end), set before it is drawn. A
  cable spans two places, which is how panel A connects to panel B: the two ends differing is normal
  (owner, Q4).
- **Gland**: the cable gland at each end, by its thread size (`M20`). It is a text value with a
  list of usual sizes (Q5).

Glossary additions (059b): *not drawn*, *assign*, *planned end*, *gland*.

## 2. Model

No new op, no new record field, no new `Document` map (FD1).

### 2.1 Cable properties (FD2)

| Key | Kind | Bounds | Notes |
| --- | --- | --- | --- |
| `tag` | text | 026 rules | Existing. Unique across devices and cables |
| `cable_type` | style ref | an entry of `cable` | Existing (057). Supplies the defaults below |
| `type` | text ≤ 64 | | Existing free notation. Kept; a cable type sets it when chosen (§4.2) |
| `cores` | core offer | 028's parser; ≤ `MAX_CABLE_MEMBERS` cores | Existing |
| `length` | number, m | 0.1–10 000 | Existing key; now validated on write |
| `gland.from`, `gland.to` | text ≤ 16 | from the gland list or typed | **New.** Cabinet end, field end (057's sides) |
| `shield` | choice `none`, `overall`, `pairs`, `overall+pairs` | | **New** |
| `maker`, `part_number`, `description` | text ≤ 64 | | **New on the cable**: override the cable type's (Q8) |
| `end.from.location`, `end.to.location` | location entry key | an entry of 052's list | **New.** Each end's planned location (Q4) |
| `end.from.function`, `end.to.function` | function entry key | an entry of 052's list | **New.** Each end's planned function (Q4, Q10) |

`from` is the cabinet end and `to` the field end, as 057's Ends section names them. There is no
cable-level location or function: a cable is a different thing from a device (Q10). Renaming an
entry in 052's lists renames it on every planned end, as on devices.

Every new key is read with a bound, and a value out of bounds counts as absent and raises 057's
`rating-bad-value`-style row. Here that is `cable-bad-value` (§6), so a file made elsewhere never
breaks the panel.

### 2.2 `cable` catalogue fields (FD3)

Added to 057's `cable` catalogue (the fields 057 §4.4 reserved for 2.7):

| Field | Kind | Built-in | Used for |
| --- | --- | --- | --- |
| `cores` | core offer (`4G`, `7G`, `BN,BK,GY,GNYE`) | empty | The cable's offer when it has no `cores` of its own |
| `shield` | as the property | `none` | Default for the cable |
| `gland` | text ≤ 16 | empty | Default gland at both ends (Q5, option B) |
| `outer_diameter` | number, mm, 1–200 | none | Shown in the Part card. Kept for computed glands later (Q5, C) |

**Resolution** (one function, `cable_value(doc, k, Field) -> Resolved<T>`, the same `Resolved`
with its source as 057 uses): the cable's own property → its cable type's field → the `type`
notation (cores, size only) → none. The source gives the hint: `set here`, `from type`,
`from notation`.

### 2.3 Typed keys (code review finding 4)

059a adds `enum CableField { Tag, CableType, Type, Cores, Length, Gland(Side), Shield,
Maker, PartNumber, Description, EndLocation(Side), EndFunction(Side) }` with `key() -> &'static str` and
`parse(&str)`, in `cable_cmds.rs`, with `enum Side { From, To }`, so each end's stored key is
built in one place. Every read and write in 059 goes through it. No new string
literal for a key outside that `impl`. `enum Shield` likewise, with serde words equal to the stored
words.

## 3. Ops each action records

| Action | Transaction name | Ops |
| --- | --- | --- |
| + Cable | `Create cable` | `CreateCable { cable: { properties: tag (+ given fields), members: [] } }` |
| Edit a field | `Edit cable` | `SetProperty { object: Cable(k), key, before, after }`; choosing a cable type also sets `type` from its notation when `type` was empty or came from the old type (one transaction) |
| Remove, not drawn | `Remove cable` | `DeleteCable` |
| Remove, drawn | refused (Q1) | none |
| Assign by slash | `Cable slash` | `AddCableMember`×n with core labels, `AddItem` + `LinkRepresentation` (role `slash`) |
| Assign by Add to cable | `Add to cable` | `AddCableMember`×n (existing `cableadd`) |
| Add a core (board footer) | `Edit cable` | `SetProperty cores` to the offer plus one core (`4G` → `5G`; a list gains the next number) |

## 4. Commands, JSON forms and MCP

New module `crates/ecad-cadcraft/src/cable_cmds.rs`, registered in `lanes.rs` as
`("059", crate::cable_cmds::specs)`. Parameters are serde structs with `deny_unknown_fields`,
parsed once (`serde_json::from_value`). Errors are one enum:

```rust
// design sketch, not production code
pub enum CableError {
    NoProject,
    ReadOnly,                                   // writes off, or a release (029)
    NoSuchCable,
    TagTaken { tag: String, by: String },       // 026: devices and cables share tags
    BadValue { field: CableField, reason: BadReason },
    Drawn { tag: String, pages: Vec<(PageId, String)> },               // the UI's Go to per page
    CoresInUse { tag: String, offer: usize, in_use: Vec<String> },     // Q6: the cores in use, by label
    MoreWiresThanCores { tag: String, free: Vec<String>, wires: usize }, // assign beyond the offer
    AlreadyInCable { wires: Vec<(String, String)> },                  // 004 O5
    SlashedHere { tag: String },                // 032: one slash per page
    TooMany,                                    // MAX_CABLE_MEMBERS
}
pub enum BadReason { Empty, TooLong(usize), NotANumber, OutOfRange { min: f64, max: f64 }, UnknownEntry, BadOffer }
```

`Display` gives the words of §9. `run` maps it with `.to_string()` into today's engine error until
R1's `CmdError` lands; then it becomes `CmdError::Refused(..)` with its kind. The UI branches on the
variant, not on the text (`Drawn` → the Go to links, §5.4).

### 4.1 `cablenew` (writes)

```rust
#[serde(deny_unknown_fields)]
struct NewCable { tag: Option<String>, cable_type: Option<String>, r#type: Option<String>,
  cores: Option<String>, length: Option<f64>, shield: Option<Shield>, maker: Option<String>,
  part_number: Option<String>, description: Option<String>,
  from: Option<EndPlan>, to: Option<EndPlan> }
#[serde(deny_unknown_fields)]
struct EndPlan { location: Option<String>, function: Option<String>, gland: Option<String> }
```

Every field is optional. The tag is the next free `-W` (`devices::new_cable_tag`, provisional
rules of 026) unless given. Returns `{cable, tag}`. Programmatic calls never prompt.

### 4.2 `cableedit` (writes)

`{cable: hex | tag, set: {<field>: value | null, …}}`. Several fields in one transaction; `null`
clears; the ends are `{from: {location, function, gland}, to: {…}}`. Validated all first: any
`BadValue` writes nothing. A `cores`, `type` or `cable_type` value whose offer has fewer cores than
the cable's members, or lacks a label a member uses, is refused with `CoresInUse`, naming the cores
in use (Q6). Choosing `cable_type` updates `type`
as §3 says. A tag edit goes through 026's rename (references and continuations follow, as Set
property on `tag` does today). Returns `{cable, changed: [field…]}`.

### 4.3 `cableslash` and `cableadd`: the assign rule (FD5)

`cableslash {from, to, cable | tag}` with an existing cable:

- **Not drawn** (no members, no representation) → **assign**: the crossed wires become members in
  crossing order. Each takes the next spare core of the offer (as `cableadd` does). More wires than
  the offer's free cores is refused with `MoreWiresThanCores`, nothing written: the same rule as Q6,
  a cable never gains more wires than its cores. With no offer, labels are `1…n`. Plus the slash
  representation. One transaction, `Cable slash`.
- **Drawn** → **continue**, unchanged (032).

Old JSON that sent `cable` for a cable with no members was always refused, so no existing caller
changes behaviour. `cableadd` already assigns; onto a not-drawn cable it takes the same free-core
rule.

### 4.4 `cableremove` (writes)

`{cable}`. If the cable is not drawn it is deleted. If drawn it is refused with `CableError::Drawn`
(§9), whose pages the UI turns into Go to links (owner, Q1: no remove-but-keep-wires from the list).
`ecaddelete {cable}` stays as it is: deleting a cable on its page is unchanged.

### 4.5 Reads

No new read command. `report {kind: "cables"}` gains columns `Glands`, `Shield`, `Planned from`,
`Planned to` (each end's function and location), `Drawn` (`yes`/`no`), and the parameter
`include_not_drawn: bool` (default from the report setting, itself default `true`; Q3). MCP agents
list cables through it.

### 4.6 MCP contract tests (brief 011 rule)

`ecad-mcp/tests/all/lanes/lane_059.rs`: for `cablenew`, `cableedit` (with the `CoresInUse` refusal),
`cableremove` (not drawn, and the `Drawn` refusal) and `cableslash` in assign mode (with
`MoreWiresThanCores`): the recorded transaction's name and ops, `Author::Agent(<client>)`,
refusal with writes off, and `deny_unknown_fields` (an unknown key is refused, nothing written).

## 5. UI

### 5.1 Project ▸ Cables toolbar and list (059b)

Toolbar, left of the filter: **`+ Cable`** (`prim::Button` primary, small, with a chevron part;
`ecad.lists.cables.add`). Owner, Q9: it offers both ways.

- **Click**, or the menu's first item **New cable**: creates a cable at once (`cablenew {}`) with
  the next free `-W`, selects its row, and puts focus in the Details pane's **Tag** box with the
  tag selected, so typing renames it. No dialog.
- **Chevron**: a menu (`ecad.lists.cables.add.menu`) with **New cable** first, then a section
  header `Assign in the drawing` and the not-drawn cables
  (`-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`). Picking one runs **Assign in the drawing…** for it
  (§5.6).

List (057's columns; no new column):

| Column | Not-drawn cable shows |
| --- | --- |
| Cable | the tag, bold |
| Type, Cores, Length | as set; `—` read when empty, as today |
| From / To | each end's plan in `text_3` italic: `=PUMP +CAB1 (planned)`; `—` when none |
| Glands | `M20, M20` (059d); `—` |
| Cores used | `0 of 4` |
| Checks | badge `not drawn` (`Tone::Neutral`; `Tone::Warning` in the release check, Q2) |

Rows of not-drawn cables sort by tag with the others. The list footer counts them: `4 cables ·
1 not drawn`.

Keyboard: **Ins** = New cable, **Del** = Remove (with §5.4's rules), when the list has focus.

### 5.2 Details pane: editable (059b)

057's pane, now edited in place. Each value uses the component its kind calls for:

| Section | Row | Component | Element name |
| --- | --- | --- | --- |
| Identity | Tag | `TextEntry`, bold | `ecad.project.detail.cable.tag` |
| | Runs | read: drawn ends, else planned (`=PUMP +CAB1 → =PUMP +FIELD`) | `.cable.runs` |
| Part | Cable type | `Select` over `cable` entries, chevron; first item `None` | `.cable.cable_type` |
| | Type | `TextEntry` (the notation); hint `from type` / `set here` | `.cable.type` |
| | Cores | `TextEntry` (offer), hint `from type` / `from notation` / `set here`; under it read `4 · 3 assigned` | `.cable.cores` |
| | Shield | `Segmented` `None · Overall · Pairs · Both` | `.cable.shield.<word>` |
| | Maker, Part number | `TextEntry`, hint `from type` / `set here` (Q8) | `.cable.maker`, `.cable.part_number` |
| | Length | `TextEntry::number("m")` | `.cable.length` |
| Rating | 057's section, unchanged | | |
| Ends | Cabinet end | read: drawn end (`+CAB1 -X01`), else `—` | `.cable.end.from` |
| | Cabinet, planned function | `Select` over 052's function list, hint `planned` | `.cable.end.from.function` |
| | Cabinet, planned location | `Select` over 052's location list, hint `planned` | `.cable.end.from.location` |
| | Field end, its planned function and location | the same three rows, `end.to.*` | `.cable.end.to`, `.cable.end.to.function`, `.cable.end.to.location` |
| | Gland, cabinet | `Select` with typing (sizes list + free), hint `from type` / `set here` | `.cable.gland.from` |
| | Gland, field | same | `.cable.gland.to` |
| Where it is drawn | pages, `link` each (`→ 2/D1`); `Not drawn yet` + **Assign in the drawing…** link | `.cable.drawn`, `.cable.assign` |

Each box commits on Enter or focus loss as one `cableedit`. Esc restores. A bad value shows the
`TextEntry` invalid state and `BadValue`'s words under the row; nothing is written. Read-only (a
release, or writes off): boxes become read values and the footer buttons are disabled with 029's
tooltip.

The planned `Select`s stay editable after drawing: the plan is the cable schedule. If a drawn end
differs from **that same end's** plan, its read value carries a warning hint `planned +FIELD` (§6,
`cable-end-off-plan`). The two ends differing from each other is normal and never shown.

### 5.3 Board footer (059b)

Board 2.06's footer, left to right: **Change cable type…** (opens the Cable type `Select`, as
`.cable.cable_type`), **Add a core** (§3), and the danger icon **Remove** (`prim::Button` danger;
`ecad.lists.cables.remove`).

### 5.4 Remove (059b)

- **Not drawn**: removed at once; the status line says `Removed -W03. Undo with Ctrl Z.` No dialog.
- **Drawn**: refused (owner, Q1). The button stays enabled. Pressing it shows the refusal as a
  status pill in the Details pane (`ecad.lists.cables.remove.refused`, `Tone::Warning`):
  > -W01 is drawn on **→ 2/D1** and **→ 3/C2**. Delete it on its page.

  Each page is a `control::link` (Go to: Draw, that page, the slash in view and selected). Nothing
  is written.

### 5.5 Cable view and core table for a not-drawn cable (059b)

- Cable view: the cable is drawn as usual, with glands, body, words over it and each core in its
  colour from `core_colours`. At each end, instead of points, the planned location in `text_3`
  (`=PUMP +CAB1 (planned)`). The cores are thin and in `text_3`, with no red open ends. The view's caption
  reads `-W03 · not drawn · H07RN-F 4G1.5 · 12 m`.
- Core table: one row per offered core; Cabinet end and Field end `—`; Status `not assigned` (read,
  `text_3`). The Not used tick works as on drawn cables.
- A drawn cable with spare cores: those cores' Status reads `not assigned` (was `open`). Their
  `core-open` row stays an error (057's owner rule), with the new wording (§6).

### 5.6 Assign in the drawing (059c)

**Slash tool.** The options bar (and the prompt) gains **Cable** before the first point:
`control::Select` (`ecad.draw.slash.cable`), items in this order:

1. `New cable` (default; Q7).
2. A section header `Not drawn` with not-drawn cables: `-W03  H07RN-F 4G1.5 · +CAB1 → +FIELD`. Ones
   whose planned end locations match the locations of the devices at the crossed wires' ends go
   first, marked `matches` (Q7; worked out when the slash is drawn, for not-drawn cables only).
3. A section header `Drawn` with drawn cables not yet slashed on this page (continue, 032).

The select types to filter by tag. At the command line the same choice is the `Cable` keyword:
`Slash first point or [Cable]:` → `Cable tag or [New] <New>:`. The preview colours the crossed wires
as today, and labels them with the cores they will get (`1`, `2`, `GNYE`), so the order is seen
before the commit. Esc writes nothing.

**New cable** takes the next free `-W` at once (Q9). After the commit the right panel shows the new
cable with its Tag box focused, ready to edit.

**Add to cable** (wire selection, right panel and wheel): its existing cable prompt becomes the same
`Select`, listing not-drawn cables first.

**From the Cables tab.** **Assign in the drawing…** (Details pane, and the `cable-not-drawn` fix)
switches to Draw on the first page whose location, or one of whose location boxes, matches the
planned cabinet end's location, else the active page, and starts the slash tool with that cable already picked. The status line reads
`Slash the wires of -W03 (4 cores).`

### 5.7 Element names (all)

`ecad.lists.cables.add`, `.add.menu`, `.remove`, `.remove.refused`,
`ecad.project.detail.cable.{tag,runs,cable_type,type,cores,shield.<word>,maker,part_number,length,
end.from,end.from.function,end.from.location,end.to,end.to.function,end.to.location,gland.from,
gland.to,drawn,assign,add_core,change_type}`, `ecad.reports.cables.include_not_drawn`,
`ecad.draw.slash.cable`, `ecad.draw.cableadd.cable`.

**Tokens:** none new. Not drawn uses `text_3` (and `ecad.check-warning` in the release check).
Not assigned uses `text_3`.

### 5.8 The report option (Q3)

The cable report's settings gain **Include not-drawn cables** (`control::Toggle`, default on;
`ecad.reports.cables.include_not_drawn`), stored as the report setting `include_not_drawn`.
Generating the report (Reports tab, Print Book, export) offers the same toggle beside Generate,
preset from the setting; that choice applies to that run only. Not-drawn rows show their planned
ends marked `(planned)`.

## 6. Checks

| Kind | Severity | Message | Subject | Fix |
| --- | --- | --- | --- | --- |
| `cable-not-drawn` **new** | Info; **Warning in the release check** (029; Q2) | `-W03 is not in the drawing yet.` | cable | **Assign in the drawing…**; **Remove** |
| `core-open` changed | Error (057) | unwired: `-W01 core GNYE is not assigned: slash or add a wire, or tick Not used.`; open end: unchanged | cable | unchanged. **Not raised at all for a not-drawn cable** |
| `cable-end-off-plan` **new** | Warning | `-W01's field end is planned =PUMP +FIELD but drawn at +JB1.` One row per end off its plan. An end with no plan, or not drawn, raises nothing; the two ends differing from each other never does | cable (end) | **Use the drawing** (sets that end's plan to its drawn location and function, one `cableedit`); **Go to** that end |
| `no-gland` **new** (059d) | Info | `-W01 has no gland size at the field end.` | cable, drawn only | **Set gland** (focus the row) |
| `cable-bad-value` **new** | Warning | `-W01's length "12m3" is not a number of metres; it is ignored.` | cable | **Show** (focus the row) |
| `too-many-cores` reused | (028) | unchanged; 059's commands no longer cause it (Q6), but older files can | | add **Change cable type…** |
| `no-core-count` reused | (028) | unchanged; not raised for a not-drawn cable without a type (`cable-not-drawn` covers it) | | |

The kinds go in as rows of the check-kinds table once code review R4 has landed. Until then
they go in `checks.rs`'s usual places, and the overlap with other lanes is noted in §15.

## 7. States

| State | List | Details | Cable view |
| --- | --- | --- | --- |
| Not drawn | `not drawn` badge, planned ends italic | editable; Ends planned selects; Assign link | planned ends, grey cores |
| Partly assigned (spare cores) | `n errors` (core-open) | `4 · 3 assigned` | spare cores red `not assigned` (057 rule) |
| Drawn, all assigned | as 057 | as 057 + editable fields | as 057 + glands |
| Drawn, an end off its plan | `1 warning` | that end's read value with a `planned +FIELD` hint | as drawn |
| Read-only | as is | read values; footer disabled | as is |
| Empty list | `No cables yet. + Cable builds one; a slash across wires draws one.` with the + Cable button | | |

## 8. Update maps

| View | Ops that change it | What it does | Everything else |
| --- | --- | --- | --- |
| Cables list (report copy) | `CreateCable`, `DeleteCable`, `Add/RemoveCableMember`, `SetProperty` on `Cable(k)`, representation ops of `Cable(k)`, `SetStyle cable` (its users) | Patches row k (057's report map) | nothing |
| Not-drawn index `undrawn: BTreeSet<CableId>` (kept in the project's cable index) | `CreateCable` (insert if no members), `DeleteCable` (remove), first `AddCableMember` / `LinkRepresentation` of `Cable(k)` (remove), last `RemoveCableMember` with no reps left (insert) | O(1) per op | nothing |
| Slash and Add to cable pickers | read `undrawn` and the page's slashed cables when opened | built on open, O(cables) once; not per frame | nothing |
| Planned-ends match (picker order) | none (computed when the picker opens, for the not-drawn cables only) | | |
| Checks `Key::Cable(k)` | the cable's own ops above; for `cable-end-off-plan`, also `AttachEnd` on a member wire and a member's end device's `location` or `function` change (052's index) | re-checks cable k | nothing |
| Release check | reads the kept `cable-not-drawn` rows and raises their severity; no rescan | | nothing |
| Cable report, `include_not_drawn` | the setting's change; `undrawn` changes | filters the kept report rows, no rebuild | nothing |
| Details pane | the selected cable's ops | rebuilds the pane for k | nothing |
| Cable view | the selected cable's ops, its members' `AttachEnd` | redraws | nothing |

No full rescan on any ordinary commit. A full build only on open, reload or first show.

## 9. Words (the `Display` of `CableError`)

- `Drawn`: `-W01 is drawn on 2/D1 and 3/C2. Delete it on its page.` (the pages become Go to links)
- `CoresInUse`: `-W01 has wires on cores 1, 2, 3, 4 and GNYE; 3G1.5 has 3 cores. Remove wires on their page first, or choose a type with at least 5 cores.`
- `MoreWiresThanCores`: `-W03 has 2 free cores (3, GNYE); the slash crosses 4 wires. Slash fewer wires, or add cores first.`
- `TagTaken`: `-W01 is already the tag of -W01 (cable) / -K01 (device).`
- `BadValue`: `Length must be a number of metres from 0.1 to 10000.` / `Cores: "4X1.5" is not a core count (try 4G, 7X or BN,BK,GY).` / `No cable type "H07X" in this project's standards.`
- `SlashedHere`, `AlreadyInCable`, `TooMany`: today's `slash.rs` / `references.rs` words, unchanged.
- Echo after + Cable: `Created -W05 (not drawn). Fill it in on the right, then assign it in the drawing.`

## 10. Parts and files

| Part | Files | Overlaps |
| --- | --- | --- |
| 059a | `cable_cmds.rs` (new), `lanes.rs` (one line), `slash.rs`/`commands.rs` (`run_cableslash` branch), `styles/cable.rs` (`open_cores` skip, fields of §2.2), `checks.rs` (kinds), `outputs.rs` (report columns) | `checks.rs` with every lane; `styles/cable.rs` with 057's follow-ups |
| 059b | `lists/cables.rs`, `lists/detail.rs`, `lists/cable_view.rs`, `checks_panel.rs` (fix buttons) | 058b (`cablerow` paging) only in the report, not the list |
| 059c | `interact/` slash and cableadd prompt machines, the options bar | none known |
| 059d | `styles/cable.rs` (gland field), `lists/cable_view.rs` (gland text), `lists/cables.rs` (`GLANDS` column from `Placeholder` to read) | 059b |

## 11. Acceptance checks (tester for 059a and 059c)

1. `cablenew {}` on the demo → one transaction `Create cable`, one `CreateCable` with no members, tag `-W02`; Checks: one `cable-not-drawn` (Info; Warning in the release check), zero `core-open` for it, even with `cable_type` giving `4G`.
2. `cablenew {tag: "-K01"}` where `-K01` is a device → `TagTaken`; nothing written (head unchanged).
3. `cableedit {cable, set: {length: 12, to: {gland: "M20", location: "+FIELD"}, shield: "overall"}}` → one transaction, four `SetProperty`. `{length: "x"}` → `BadValue`, nothing written. `{colour: 1}` → refused (unknown field). On a cable with 4 wires, `{cores: "3G"}` or a 3-core `cable_type` → `CoresInUse` naming the cores in use, nothing written.
4. Choosing `cable_type` with notation fills `type` when empty; leaves a hand-typed `type` alone.
5. `cableremove` on a not-drawn cable → `DeleteCable`; undo restores it. On -W01 (drawn) → `Drawn` with `2/D1` in the words and its page id in the error; nothing written. `{cable, drawn: "unassign"}` → refused (unknown field).
6. Assign: a not-drawn `-W02` with `cores: "4G"`, slash across 3 wires → members in crossing order with labels `1, 2, 3`; `core-open` for `GNYE` reads `not assigned`; `cable-not-drawn` gone. Undo → not drawn again, row back.
7. Slash 5 wires onto a not-drawn `3G` cable → `MoreWiresThanCores`, nothing written.
8. `cableslash {cable}` on a drawn cable on another page → continue mode as 032 (regression).
9. A wire already in a cable → `AlreadyInCable`; nothing written.
10. Slash tool picker (059c, headless UI): lists `New cable`, then not-drawn, then drawn-not-on-this-page; a cable with a slash on this page is absent; Esc after picking writes nothing. New cable gives the next free `-W`.
11. `cable-end-off-plan`: plan the field end `+FIELD`, draw it to a device in `+JB1` → one warning, for the field end; the cabinet end drawn in `+CAB1` as planned raises nothing; **Use the drawing** sets `end.to.location` to `+JB1`. A cable planned and drawn `+CAB1 → +CAB2` raises nothing.
11a. + Cable's chevron menu lists New cable, then the not-drawn cables; picking one starts the slash tool with it picked.
11b. The cable report with `include_not_drawn` off omits not-drawn cables; on (the default) lists them with `(planned)` ends.
12. A file with `length = "abc"` and a 300-char `gland.to` opens, shows `—`, raises `cable-bad-value`, no panic (hostile input test in `panic_hunt` style).
13. MCP contract tests of §4.6.

## 12. Bench rows (medium while building; large once at the end)

| Row | Operation | Budget (large) |
| --- | --- | --- |
| `cablenew` | create one cable in a project with 500 cables, until the list row and Checks are current | < 2 ms update after commit |
| `cableedit` | set `length` on one cable | < 2 ms (one row patched, one cable re-checked) |
| `cableplanrename` | rename a location entry used by 200 planned ends | ≤ 052's location rename row |
| `cableassign` | slash 7 wires onto a not-drawn cable | ≤ today's `cableslash` row + 1 ms |
| `cablepicker` | open the slash picker with 500 cables, 100 not drawn | < 5 ms, once per open |

## 13. Format decisions (for the overseer)

**Overseer ruling (2026-11-15): FD1–FD5 approved.**

Pre-v1, and no op, record or type change. Old files open unchanged. A build without 059 ignores the
new keys, and shows a not-drawn cable as today's empty cable (its offered cores as `core-open`).

- **FD1. A cable with no members and no representation is a valid, saved state** ("not drawn").
  There is no model change. ecad-file v2 must round-trip it (a test in 059a). The merge (`merge.rs`
  `join_cables`) must not drop or join an empty cable; the worker checks this and adds a test.
- **FD2. Cable properties** `gland.from`, `gland.to`, `shield`, `maker`, `part_number`,
  `description`, `end.from.location`, `end.from.function`, `end.to.location`, `end.to.function`
  (§2.1). Existing `length` gains bounds. The cable report setting `include_not_drawn`.
- **FD3. `cable` catalogue fields** `cores`, `shield`, `gland`, `outer_diameter` (§2.2), on 057's
  FP4 catalogue.
- **FD4. No new op.** + Cable is `CreateCable`, edits are `SetProperty`, remove is `DeleteCable`.
- **FD5. `cableslash` assign mode**: a not-drawn cable passed as `cable`/`tag` is assigned, not
  continued (§4.3), and never beyond its free cores. It is a behaviour change of an existing command that was always refused before.

## 14. Owner answers (2026-11-15)

The questions were asked with options and costs; the answers are recorded here and applied above.

| Q | Question | Answer |
| --- | --- | --- |
| 1 | Removing a drawn cable | **Refused**, with a **Go to** for each page it is drawn on. No remove-but-keep-wires from the list (§4.4, §5.4) |
| 2 | How serious "not drawn" is | **Info** normally, **Warning in the release check** (§6) |
| 3 | Not-drawn cables in the cable report | **Included by default**, and offered as an option when the report is generated (§4.5, §5.8) |
| 4 | Planned ends | A cable spans two places; that is how panel A connects to panel B. **Each end has its own planned location and function**, as entry keys. The ends differing is normal and never a warning; `cable-end-off-plan` fires only when a drawn end differs from that same end's plan (§2.1, §6) |
| 5 | Glands | As recommended: **a default on the cable type, overridable per end** (computed glands later; `outer_diameter` is stored now) |
| 6 | Cores reduced below those assigned | **Refused**, naming the cores in use (`CoresInUse`). The assign rule follows: never more wires than free cores (§4.2, §4.3) |
| 7 | The slash picker's default | As recommended: **New cable**, not-drawn cables first, planned-end matches marked (§5.6) |
| 8 | Maker and part number | As recommended: **on the cable type, overridable on the cable** |
| 9 | + Cable and the picker | **New cable takes the next free `-W` at once, ready to edit**; the user can otherwise **assign an existing cable**, so both + Cable and the picker offer both (§5.1, §5.6) |
| 10 | A cable as a device | **Keep it a `Cable`**: it is a different thing from a device. Per-end planned location and function are entry keys (§2.1) |

## 15. Out of scope

Computed glands (Q5 C), cable import from CSV, routing, tray fill, pull lengths, ferrules,
pair/shield drawing on the schematic, BOM lines for cables (2.3 parts), and moving cables to
`Component` (Q10). Shared-file overlap: `checks.rs` and `styles/cable.rs` are edited by other
lanes too. The worker merges `dev` before starting each part.
