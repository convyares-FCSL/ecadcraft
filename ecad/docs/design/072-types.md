# Design 072: Types for keys, units, kinds and device classes

Status: **Draft** (designer, 2026-11-16), waiting for the owner's answers in §14 and the
overseer's on the format list in §13. Brief: `docs/briefs/072-types.md`.

**Goal (one sentence).** Property keys, physical units, the words a file stores and the kind of a
device stop being free strings and bare numbers, so that the compiler finds every place a new key,
unit, catalogue or device kind must be handled.

Source: code review 2026-11 (`C:\dev\ecad-features\_tmp\code-review-2026-11.md`), findings 4
(property keys), 6 (strings that should be enums), 7 (a device class) and 10 (units and page
places): lanes R4, R5 and R8 of that review.

**Names.** This design uses today's crate names. Lane 073 renames `ecad-cadcraft` to
`ecad-engine` and moves `xtask` to `dev/xtask` before this lane's worker starts; read "Engine
(`ecad-cadcraft`)" as whatever the tree calls it then.

**Rulings already made (not reopened here).**
- strum is approved (design 061) for `word()` / `parse()` on enums.
- Booleans are written `yes` / `no`. Reading also accepts `true` / `1` / `on` (and `false` / `0` /
  `off`). One rule, in one place.
- An unknown word read from a file is kept as `Unknown(String)`; a command refuses it.
- Pre-v1: **no compatibility path and no legacy debt** (owner, 2026-11-16). Stored words may change
  when that makes them consistent. Fixtures are regenerated, never shimmed. Every on-disk word that
  changes is listed in §13.
- **One source per value** (owner). Shared vocabulary (property keys, units, `Noun`) goes in
  ecad-model, the lowest crate that needs it.
- Ratchets end the lane **at 0** and become plain bans (owner): no counts left counting down.

---

## 0. What the code says (recounted at `d323677a`)

Counts are for product code: `tests/` folders and `#[cfg(test)]` blocks are left out. They come
from grep and small scripts, so read them as ±10%. The review counted on `dev` at `c73f820b`,
before the crate split.

| Finding | Review | Now | Where now |
| --- | --- | --- | --- |
| Property-key string constants | ~50 in ~20 modules | **~105** in 27 files (the review counted only `*_KEY` names; many are called `PREFIX`, `CROSS_SECTION`, `RACK_KEY`, …) | ecad-model 8 files (`tag`, `book`, `sheet`, `wiring`, `connectivity`, `links`, `derive`, `builtins`), ecad-electrical 19 files |
| `TAG_KEY` uses / `"tag"` as a literal key | 80 / ~30 | 83 / **42** in 21 files | half in ecad-cadcraft (`cmd/*`, `interact/*`, `props.rs`, `look.rs`), 5 in ecad-app |
| Keys with no constant: `description`, `part_number`, `maker`, `type` | yes | 37 / 20 / 12 / 12 literal uses | ecad-electrical (`bom`, `devices`, `strips`, `connectors`, `outputs/kept`, `plc/*`, `styles/*`), Engine `cmd/bom`, `interact/bom_item` |
| Keys built with `format!` | — | **37** sites in 27 files | `designations`, `locations`, `connectors`, `accessories`, `plc/*`, `terminal_types`, `text_style`, `revisions`, `page_types`, … |
| `starts_with` / `strip_prefix` / `split_once` on keys | 74 in 33 files | **151** in 52 files | everywhere a key family is read |
| Boolean readers that disagree | 3 word sets | **3 word sets, 8 sites**: `"yes"\|"y"\|"true"` (`props.rs:1102`), `"yes"\|"true"` (`cmd/numbering.rs:793`), `== "yes"` (`connectivity.rs:109`, `numbering.rs:417,423`, `app shell/right/device.rs:167`), `== "on"` on defaults words (`plc/mod.rs:240,420`, `plc/look.rs:80`, `bom.rs:709`, `cmd/wire.rs:668`) | |
| Digit checks (`.all(is_ascii_digit)`) | 4 copies | **~30 sites**, of which **6** are the same "bounded whole number" reader (`plc/address.rs:70`, `plc/mod.rs:515`, `plc/rack.rs:200,206`, `rating/dataset.rs:89`, `rating/parse.rs:114`) | |
| `Kind::Choice` word lists in defaults | 3 examples | **56** uses, **31** literal lists, **25** of them on/off switches | `defaults/*.rs`, `styles/*.rs`, `rating/mod.rs` |
| `resolve(..).word() == "…"` | — | **10** | `plc/*`, `bom.rs`, `styles/frame.rs`, `front.rs`, `cmd/wire.rs` |
| String-literal `match` arms | 615 | **595** (electrical 230, cadcraft 131, app 122, file 28, inspector 28, mcp 20, bench 15, model 15) | |
| Starter symbols / `match key` blocks on them | 14 blocks | **113 starters**; **11 blocks**, biggest `symbols.rs:1377` (46 arms) and `:1589` (55); `symbols.rs:475,520` repeat the 6 channel keys; `devices.rs:353` (9) | ecad-electrical `symbols.rs`, `devices.rs`, `rating/trace.rs`, `connectors.rs`, `xref.rs` |
| `tag_prefixes` with a fake `"cable"` key | yes | yes: `devices.rs:318`; the map also holds **project symbol keys** (`cmd/symbol.rs:1358`), so a project symbol called `cable` collides | |
| Style catalogue words | 4 words, ~58 lines | **7 catalogues** (text, location, frame, wire, page, cable, ratingtable); `catalogue: &str`/`String` in 23 signatures; 41 literal catalogue words at call sites | `ecad-model/src/{op,style,merge}.rs`, `ecad-electrical/src/styles/*`, `text_style.rs`, `cmd/wire.rs`, Engine `cmd/text.rs`, `props.rs` |
| `Representation.role: String` | 7 words | 9 fixed words (`route`, `slash`, `reference`, `box`, `fan-anchor`, `termfuse`, `drawn`, `symbol`, plus a symbol's **starter key or project symbol key** as its role) in 18 files; read back as a starter key in `xref.rs:342` | |
| Connector side parsed | 3 times | 3 (`cmd/connector.rs:102`, `interact/connectors.rs:26,163`) though `symbols::Side` has `word()`; `locations.rs:457,485`, `proposals.rs:474` build `location.socket`/`plug` by hand | |
| Text kinds as strings | `styles/text.rs:97` | `styles/text.rs:94` `noun()` matches 7 words of `TextKind`; `props.rs:1037` matches `shown`/`on`/… by hand | |
| Device predicates | 9 | **10**: `strips::is_strip`, `connectors::is_connector`, `plc::rack::is_rack`, `locations::is_box`, `locations::is_device`, `continuations::is_continuation`, `bom::is_item`, `bom::is_fuse_terminal`, `strips::is_fuse_device` (two answers to one question), plus the `kind` word reader `wires::kind_of`. **151 calls in ~45 files**. `is_rack` walks the device's representations | |
| Angles | mixed | stored radians: `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation`; commands take **degrees**, except the symbol content's `slots[].rotation` (**radians**, `cmd/symbol.rs:204`); `to_radians`/`to_degrees` at **21** sites; 21 fields/params named `rotation`/`degrees`/`angle` typed `f64` | |
| Ratings parsers | 5 return `Option<f64>` | **8**: `current`, `amps`, `voltage`, `cross_section`, `temperature`, `ambient`, `percent`, `length_m` (`rating/parse.rs`) | |
| Page places | `Place`, `zone_text` | unchanged: `Place { page: String, column: u32, row: u32, … }` (`derive.rs:248`), `Grid::column_of/row_of -> u32` (`sheet.rs:648`), `page_number: String` in `derive.rs:288`, `device_list.rs:26`, app `library/*` | |

What is already right and stays: `Properties` stays `BTreeMap<String, String>` (the file does not
change shape); `Op::SetProperty { key: String }` stays a string (a person may add any key);
the defaults registry (066) already reads through `resolve(&Entry)`; `symbols::Side`,
`RepType`, `TextKind`, `Shown` and `CardType` exist and are used where this design says.

---

## 1. Where things live (one source per value)

| Piece | Crate / module | Why there |
| --- | --- | --- |
| `words`: the one boolean rule, whole-number and digit readers, the `Word` trait, `Stored<E>` | `ecad-model/src/words.rs` | every crate reads stored words |
| `PropKey`, `Area`, `KEYS`, `FAMILIES`, `PropsExt`, built-key constructors | `ecad-model/src/keys.rs` (one file per area under `keys/` if it passes ~600 lines) | keys are shared vocabulary; ecad-model already owns `tag`, `book`, `sheet`, `wiring` keys |
| Units: `Mm`, `Metres`, `Degrees`, `Radians`, `Amps`, `Volts`, `Mm2`, `Celsius`, `Percent` | `ecad-model/src/units.rs` | `Placement` (model) is the first user |
| `PageColumn`, `PageRow` | `ecad-model/src/sheet.rs` beside `Grid` | `Grid` makes them |
| `PageNo` | `ecad-model/src/book.rs` beside the page-number key | the book owns page numbers |
| `StarterKey`, `CardType` (the enum and its pure word and body-key methods), `STARTER_LIBRARY`, `PROJECT_SYMBOLS`, each starter's fixed `DefinitionId` | `ecad-model/src/starter.rs` | the device class (below) is kept by the model's index, which needs to know a starter from a definition |
| `SymbolKey`, `PrefixKey` | `ecad-model/src/starter.rs` | `ProjectSettings.tag_prefixes` (model) is keyed by them |
| `ComponentKind` (the `kind` property), `ComponentClass`, `DeviceClass` | `ecad-model/src/class.rs` | kept by `Index` |
| `StyleCatalogue` | `ecad-model/src/style.rs` | `Op::SetStyle` and `StyleCatalogues` are model types |
| `Role` | `ecad-model/src/doc.rs` beside `Representation` | stored field |
| `Side` | stays `ecad-electrical/src/symbols.rs` | only electrical and Engine use it |
| `TextKind`, `Shown` | stay `ecad-electrical/src/text_style.rs` | |
| The starter table (`Starter` rows: name, aliases, graphics, `Kind`, family, quiet) | stays `ecad-electrical/src/symbols.rs`, now keyed by `StarterKey` | what a symbol looks like is a rule, not vocabulary |
| The choice enums of the defaults and style fields (§4.8) | next to their entries in `ecad-electrical/src/defaults/*`, `styles/*`, `rating/*`; those ecad-model already has (`ReferenceStyle`, colour style, tag dash and case) stay there | |

`strum` becomes a workspace dependency of ecad-model and ecad-electrical. JSON Schema for the new
enums and units comes behind ecad-model's existing `schema` feature (070's pattern for ids), so
070's params structs can use them.

---

## 2. Property keys

### 2.1 `PropKey` and the registry

```rust
// ecad-model/src/keys.rs
/// A property key the code knows. `Copy`, compares with `&str`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PropKey(&'static str);
impl PropKey {
    pub const fn new(k: &'static str) -> Self;   // const: used in statics and match guards
    pub const fn as_str(self) -> &'static str;
}
impl PartialEq<str> for PropKey { /* … */ }
impl std::fmt::Display for PropKey { /* the key itself */ }

/// Which objects a key sits on.
pub enum Holder { Project, Page, Folder, Component, Conductor, Cable, Representation }

/// What a key's value is, for the typed readers, the debugger's key list and the inspector.
pub enum ValueSpec {
    Text { max_chars: usize },
    Flag,                                   // words::flag
    Uint { max: u32 },                      // words::uint
    Words(&'static [&'static str]),         // a Word enum's WORDS
    Id(Noun),                               // a hex id of a page, cable, …
    Length,                                 // a number in mm
}

pub struct KeyInfo { pub key: PropKey, pub area: Area, pub on: &'static [Holder], pub value: ValueSpec, pub label: &'static str }

/// A family of built keys: a prefix and the parts after it.
pub struct Family { pub prefix: &'static str, pub area: Area, pub parts: &'static [Part], pub on: &'static [Holder] }
pub enum Part { Uint { max: u32 }, Word(&'static [&'static str]), Name { max_chars: usize }, Hex }

pub static KEYS: &[KeyInfo];       // every fixed key outside the defaults registry
pub static FAMILIES: &[Family];    // every built-key family

pub fn info(key: &str) -> Option<&'static KeyInfo>;
pub fn family_of(key: &str) -> Option<(&'static Family, Parts<'_>)>;   // the one place a key is split
pub fn area_of(key: &str) -> Area;                                     // 064's `Area::of`
```

The constants and `KEYS` are made by one macro, so a key is written once (holders, lengths and
labels below are illustrative; the worker takes them from today's code):

```rust
prop_keys! {
    Identity {
        TAG = "tag", [Component, Cable], Text { max_chars: tag::MAX_CHARS }, "Tag";
        TAG_FIXED = "tag_fixed", [Component, Cable], Flag, "Tag fixed";
        DESCRIPTION = "description", [Component, Page, Cable], Text { max_chars: 256 }, "Description";
    }
    BomItem {
        PART_NUMBER = "part_number", [Component, Cable], Text { max_chars: 64 }, "Part number";
        MAKER = "maker", … ; ARTICLE_NUMBER = "article_number", … ; TYPE = "type", … ;
        QTY = "qty", [Component], Text { max_chars: 16 }, "Quantity"; UNIT = "unit", … ;
    }
    Plc { RACK = "plc.rack", … ; SLOT = "plc.slot", [Component], Uint { max: MAX_SLOT }, "Slot"; … }
    // …
}
```

The constants are grouped by area (`keys::identity::TAG`, `keys::plc::SLOT`) and re-exported flat
(`keys::TAG`) when the name is unique. The worker takes `max_chars` and ranges from today's
checks, never inventing new ones.

**`Area`** is the enum design 064 §3.3 drew, defined **here** (not in 064's `effect.rs`), so the
registry and the views share one list: the 17 registry areas (`Text`, `Wiring`, `Wire`, `Tags`,
`Revisions`, `Print`, `Symbol`, `Terminal`, `Connector`, `StripPage`, `Bom`, `Location`,
`Continuation`, `Plc`, `Page`, `Front`, `Rating`) and the stored areas outside the registry
(`Identity`, `BomItem`, `Designation`, `Book`, `RevisionRow`, `Link`, `StripAccessory`,
`TagHighWater`, `CableCores`, `ContinuationId`, `Generated`, plus the ones this inventory adds:
`Connectivity` for `wire_number`/`potential`/`junction`, `Rack`/`Channel` if the worker finds the
`plc.*` stored keys need their own, `Accessory` for `acc.*`, `ConnectorPins` for `pins`, `half.*`,
`ignore.*`) and `Free` for a person's own key. `defaults::AREAS` becomes `Area::REGISTRY`'s words
(064 step 2 then only consumes it). `area_of` reads `KEYS`, then `FAMILIES`, then a registry area
word before the first `.`, else `Free`.

**What the registry does not hold.** The defaults registry's entries (`text.font`,
`plc.slot.first`, …) stay in `ecad-electrical/src/defaults/*` as today (066): they are the source
of their keys. `Entry.key` becomes a `PropKey` (built with `PropKey::new` in the entry), so a
defaults key is greppable and typed, but is not repeated in `KEYS`. Style **field** keys
(`grid_labels`, `line_type`) are not property keys: they are a catalogue's fields and stay in
`styles/*` (`Field.key`).

### 2.2 One rule for words: `ecad_model::words`

```rust
pub const YES: &str = "yes";
pub const NO: &str = "no";
/// `yes|true|1|on` → true, `no|false|0|off` → false (trimmed, any case); anything else None.
pub fn flag(text: &str) -> Option<bool>;
pub fn flag_word(b: bool) -> &'static str;                    // YES / NO
/// ASCII digits only (no sign, no space).
pub fn is_digits(text: &str) -> bool;
/// A whole number 0..=max: digits only, at most as many digits as `max` has. Leading zeros allowed.
pub fn uint(text: &str, max: u32) -> Option<u32>;
/// The same without leading zeros (`07` refused): link core numbers, BOM sub-numbers, sheet steps.
pub fn uint_canonical(text: &str, max: u32) -> Option<u32>;
```

Every copy found in §0 goes through these: the 8 boolean sites, the 6 bounded-number readers and
the remaining digit checks (`tag.rs`, `links.rs`, `bom.rs`, `channels.rs`, `strips.rs`,
`symbols.rs`, `trace.rs`, `tagtable.rs`, app `project_panel.rs`, `status_bar.rs`). `y` stops being
read as yes (`props.rs:1102`), which the ruling's word list leaves out. A prompt's "yes / cancel"
answer (`interact/devices.rs:29`) is a keyword typed at a prompt, not a stored word: it stays.

**Writing a flag.** A flag property that is false is removed when absence means false for that key
(`tag_fixed`, `wire_number_fixed`); a defaults entry level that overrides an inherited "yes" writes
`no`. The rule sits in `PropsExt::set_flag` and the defaults writer, nowhere else.

### 2.3 Typed readers

```rust
pub trait PropsExt {
    fn get_key(&self, k: PropKey) -> Option<&str>;
    fn has(&self, k: PropKey) -> bool;
    fn flag(&self, k: PropKey) -> bool;                     // absent or unreadable = false
    fn flag_opt(&self, k: PropKey) -> Option<bool>;
    fn uint(&self, k: PropKey) -> Option<u32>;              // max from the key's ValueSpec
    fn word<E: Word>(&self, k: PropKey) -> Option<Stored<E>>;
    fn text(&self, k: PropKey) -> Option<&str>;             // trimmed, non-empty
}
impl PropsExt for Properties { /* … */ }

/// The op that sets `k` on `object` (value written through the key's ValueSpec rule).
pub fn set(object: ObjectRef, k: PropKey, before: Option<String>, after: Option<String>) -> Op;
```

`uint` takes its bound from the registry, so `plc::rack::parse_slot` and `parse_channels` stop
carrying their own `3` digits and `MAX_*`.

### 2.4 Built keys

Every `format!`-built key and every `strip_prefix` that reads one goes through its family. A
family's constructor takes typed parts:

| Family (prefix) | Parts | Constructor | Lives in |
| --- | --- | --- | --- |
| `rev.<n>.<column>` | `Uint`, `Word` | `keys::rev(n, col)` | model |
| `designation.<aspect>.<entry>.{code,description}` | `Word`, `Name`, `Word` | `keys::designation(..)` | model |
| `link.<…>` | as `links.rs` | `keys::link(..)` | model |
| `numbering.high.<prefix>` | `Name` | `keys::tag_high(prefix)` | model |
| `unused.<core>` | `Name` | `keys::unused_core(core)` | model |
| `plc.ch.<i>.<field>` | `Uint`, `Word` | `keys::plc_channel(i, field)` | model |
| `half.<side>.<…>`, `ignore.<…>`, `location.<side>` | `Word(Side::WORDS)`, … | `keys::half(..)`, `keys::location_side(side)` (side passed as its word by the electrical caller's `Side::word()`) | model |
| `acc.after.<…>`, `acc.on.<…>` | `Name` | `keys::acc_after(..)`, `keys::acc_on(..)` | model |
| `term_label.<label>`, `rating.<label>` | `Name` | `keys::term_label(..)`, `keys::rating_of(..)` | model |
| `folder.<id>.start` | `Hex` | `keys::folder_start(id)` | model |
| `text.<kind>.style`, `text.<kind>.show`, `text.<rep>.<kind>.show` | `Word(TextKind::WORDS)`, `Hex` | `TextKind::style_key(self)`, `show_key(self, rep: Option<RepresentationId>)` in electrical, through `keys::TEXT_STYLE.build(..)` | family in model, typed method in electrical |
| `symbol.<key>.<field>` | `Name`, `Word` | `defaults::symbols::family_key(SymbolKey, Field)` through `keys::SYMBOL_FAMILY` | same |

A family's `build` and `parse` are one pair, so a key is never split by a different rule than the
one that made it. `TextKind` and `Side` stay in electrical; the family records their words through
a `Part::Word` filled by a test (`families_name_their_words`) rather than by moving the enums.

### 2.5 Defaults entries

- `Entry.key: PropKey`.
- `Kind::Flag` for the 25 on/off switches (list in §13): `Value::Flag(bool)`; the Standards screen
  shows a `control::Toggle` for them (components rule, owner 2026-11-15); the TOML writes `yes`/`no`.
- `Kind::Choice(&'static [&'static str])` stays the registry's data shape but is built only from an
  enum: `Kind::choice::<E: Word>()` (a `const fn` over `E::WORDS`). A choice entry is a typed static,
  `pub static SLOT_FIRST: Choice<FirstNumber> = Choice::new(Entry { … })`, whose
  `resolve(doc, at) -> FirstNumber` replaces `.word() == "…"`. Every `Kind::Choice(&[…])` literal
  goes (31 now).

### 2.6 Inventory: what moves

| From | Keys | To |
| --- | --- | --- |
| `ecad-model/src/tag.rs` | `TAG_KEY`, `FIXED_KEY`, `PROVISIONAL_SETTING`, `HIGH_RESET_PREFIX`, `DASH_KEY`, `CASE_KEY` | `keys::{TAG, TAG_FIXED}`, `Area::Tags` entries stay registry; `HIGH_RESET_PREFIX` → family |
| `book.rs`, `sheet.rs` | `folder`, `number`, `function`, `location`, `sheet`, `title_block`, `page.type`, `generated`, `print.frame.style` | `keys::book::*`, `keys::page::*` |
| `connectivity.rs`, `wiring.rs` | `wire_number`, `wire_number_fixed`, `potential`, `kind`, `continuation`, `junction`, `name`, `cross_section`, `colour`, `wire.category` | `keys::connectivity::*`, `keys::wiring::*`; `kind` → `keys::KIND` read as `ComponentKind` (§4.7) |
| `links.rs`, `derive.rs`, `builtins.rs` | `link.` prefix; `REFERENCE_ROLE` (a role, §4.4); the 6 settings words in `builtins.rs` are TOML field names (066), not keys | family; `Role::Reference` |
| electrical `accessories`, `bom`, `connectors`, `continuations`, `designations`, `frames`, `front`, `generated`, `locations`, `page_types`, `plc/{address,channels,overview,rack,signals}`, `rating/mod`, `revisions`, `strips`, `styles/cable`, `symbols`, `terminal_types`, `xref` | ~75 constants (§0) | `keys::<area>::*` |
| every crate | the 42 `"tag"`, 37 `"description"`, 20 `"part_number"`, 12 `"maker"`, 12 `"type"`, 1 `"article_number"` literal keys | constants |
| `wires::kind_of`, `locations::KIND_VALUE`, `bom::ITEM_KIND` | `kind` words | `ComponentKind` (§4.7) |

Non-key strings with a `_KEY` look (`styles/*::WORD`, `ROLE`, `BUILTIN_FRAME`, `PLAIN_TYPE`,
`METRES`) are words of an enum and go with §4, not here.

---

## 3. Units

### 3.1 The types

```rust
// ecad-model/src/units.rs — each `#[serde(transparent)]`, so stored bytes do not change.
pub struct Mm(pub f64);       // a length defined in millimetres (styles, defaults, sheet margins, text heights)
pub struct Metres(pub f64);   // a cable or route length
pub struct Radians(pub f64);  // stored angles (counter-clockwise)
pub struct Degrees(pub f64);  // angles a person or agent types
pub struct Amps(pub f64);
pub struct Volts(pub f64);
pub struct Mm2(pub f64);      // cross-section
pub struct Celsius(pub f64);
pub struct Percent(pub f64);
```

- Arithmetic only where it means something: `Mm ± Mm`, `Mm * f64`, `Mm / f64`, `Mm / Mm -> f64`,
  `-Mm`, the same for `Metres`; `Degrees ⇄ Radians` by `From` (the only `to_radians` /
  `to_degrees` calls left in the workspace); `Amps`, `Volts`, `Mm2`, `Celsius`, `Percent` get
  `PartialOrd`, `get()`, `Display` (`10 A`, `2.5 mm²`, `35 °C`, `5 %`) and no operators: the ratings
  maths is written as functions of the standard (`volt_drop(mv_per_a_m, Amps, Metres) -> Volts`).
- `finite(x) -> Option<Self>` on each; the parsers use it.
- **Drawing units are not `Mm`.** A page point (`Pt`) is in the project's drawing unit (mm or inch,
  `Grid.k`). `Mm` is only for values defined in millimetres whatever the project unit. They meet
  at one function, `Mm::to_units(self, unit: LengthUnit) -> f64`. `Pt`, and the 80 `(f64, f64)`
  pairs, are out of scope (§12).
- `JsonSchema` behind ecad-model's `schema` feature: a number, with the unit in its description.

### 3.2 Where they enter first

| Type | Enters at (the boundary) | What the compiler then pulls in |
| --- | --- | --- |
| `Amps`, `Volts`, `Mm2`, `Celsius`, `Percent`, `Metres` | `rating/parse.rs`: `current`/`amps → Amps`, `voltage → Volts`, `cross_section → Mm2`, `temperature`/`ambient → Celsius`, `percent → Percent`, `length_m → Metres` | `rating/{mod,dataset,trace}.rs`, the rating checks rows, `lists/rating.rs` (app). The free text a person typed (`"10 A"`, `"18 AWG"`) stays text in the property: the types are what a parse gives, not what is stored |
| `Radians` | `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation` (all stored radians today; the worker confirms the last with a round-trip test before typing it) | `translate.rs` (Engine → CADCraft), `look.rs` `Text { rotation }`, the layout code in electrical |
| `Degrees` | every command parameter named `rotation` (070's params structs: `place`, connector place, `placesymbol`, channel place, symbol content `slots[].rotation`); interactive prompts; the plc text-angle choice `"90"`/`"0"` (§4.8) | `Radians::from(Degrees)` at the one place a command builds a `Placement` |
| `Mm` | `defaults::Value::Length` and `Resolved::length`, style `Length` fields, `TextLook.height`, sheet `Margins`, ecad-drawing-style line weights | electrical layout (`text_style`, `frames`, `title_block`, `plc/look`), the 186 bare `f64` signatures the review counted shrink as these flow in; only the 3 `_mm` names are a ban target (§6) |
| `PageColumn(u32)`, `PageRow(u32)` | `Grid::column_of`, `row_of`, `column_label`, `row_label`; `Place.column`, `Place.row` | `zone_text(column: &str, row: &str)` becomes `zone_text(&Place)`; xref, continuations, reference texts |
| `PageNo(String)` | `Place.page`, `CableReference.page_number` (`derive.rs:288`), `device_list::Row.page_number`, the book's page-number reader | sorting (one `Ord` for page numbers) and the app's `library/*` rows |

**Symbol content `slots[].rotation`** is the one command parameter that takes radians today. With
`Degrees` in its params struct it takes degrees like every other command. Listed in §13.

---

## 4. Enums

### 4.1 One pattern: `Word` and `Stored<E>`

```rust
// ecad-model/src/words.rs
pub trait Word: Copy + Eq + 'static {
    const ALL: &'static [Self];
    const WORDS: &'static [&'static str];
    fn word(self) -> &'static str;
    fn parse(text: &str) -> Option<Self>;   // trimmed, any case
}

/// A word read from a file: known, or kept as written (ruling: Unknown(String)).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Stored<E: Word> { Known(E), Unknown(String) }
// Serialize writes the word back exactly; Deserialize never fails on an unknown word.
```

- Each enum derives strum's `IntoStaticStr`, `EnumString`, `VariantArray` with
  `#[strum(serialize_all = "snake_case", ascii_case_insensitive)]`; a word that is not snake case
  (`byte.bit`, `fan-anchor`, `page-column`, `0`, `90`) gets its own `#[strum(serialize = "…")]`.
  The strum attribute is the only place a word is written. A small macro implements `Word`,
  `Serialize`/`Deserialize` (as the word) and `JsonSchema` (an enum of `WORDS`) from it, so serde
  and the schema can never disagree with `word()`.
- **Commands** read `E` (serde through 070's params structs): an unknown word is refused with
  070's uniform input error, which now lists the allowed words from the schema.
- **Files** read `Stored<E>`. An `Unknown` is written back unchanged, never acted on as if it were
  a known word, and listed (§14, call 1).
- Enums stay `Copy` and exhaustive because `Unknown` lives in `Stored`, not in each enum.

### 4.2 `StarterKey`, `SymbolKey`, `PrefixKey`

```rust
// ecad-model/src/starter.rs
pub enum StarterKey { Coil, No, Nc, Power3, Mcb1, Mcb2, Fuse, Psu, /* … all 113, flat … */ Chiol }
impl StarterKey {
    pub const fn definition(self) -> DefinitionId;     // def_id(1), cn(n), pc(n), mt(n): moved from symbols.rs
    pub fn of_definition(id: DefinitionId) -> Option<StarterKey>;
    pub fn card_type(self) -> Option<CardType>;        // plccpu…plccm bodies
    pub fn channel_type(self) -> Option<CardType>;     // chdi…chiol
    pub fn class(self) -> StarterClass;                // §5; exhaustive
}
pub enum SymbolKey { Starter(StarterKey), Project(String) }   // a project symbol's key (048)
pub enum PrefixKey { Symbol(SymbolKey) }                       // see "cable" below
```

- **Flat**, one variant per starter: the starter list is a flat catalogue, the words are what a
  person types, and families are answered by exhaustive methods (`card_type`, `channel_type`,
  `side`, `levels`). Nesting (`Channel(CardType)`, `Half { family, pins, side }`) would make
  `word()`/`parse()` hand-written again.
- `symbols.rs`'s `STARTERS` rows carry `key: StarterKey`; a test checks one row per key in
  `StarterKey::ALL` order. `by_key(&str)` becomes `StarterKey::parse` + `starter(k)`.
- The 11 `match key` blocks (`symbols.rs:79,475,520,973,1164,1273,1377,1589`, `devices.rs:353,
  1468`, `rating/trace.rs:67`, `connectors.rs:119`, `xref.rs:2163`) match on `StarterKey`, with no
  `_` (§6 G5): a new starter does not compile until each table answers for it. Grouped arms
  (`A | B | C =>`) keep them short.
- **The cable prefix.** `tag_prefixes["cable"]` is a fake symbol key, and a project symbol named
  `cable` would collide with it. It becomes its own setting, `ProjectSettings.cable_prefix:
  Option<String>` (absent = `W`, 066's "not set is stored as not set"). `tag_prefixes` is then
  `BTreeMap<Stored<SymbolKey>, String>`. Listed in §13.

### 4.3 `StyleCatalogue`

```rust
pub enum StyleCatalogue { Text, Location, Frame, Wire, Page, Cable, Ratingtable }
```

Includes 057's `cable` and `ratingtable` and the `page` catalogue. `Op::SetStyle.catalogue:
Stored<StyleCatalogue>`; `StyleCatalogues = BTreeMap<Stored<StyleCatalogue>, …>`; `styles::catalogue`
takes the enum and its `CATALOGUES` table is an exhaustive `match` (no `find` by word);
`defaults::Kind::Style(StyleCatalogue)`. The 23 `catalogue: &str` signatures and the 41 literal
words go. Words do not change.

### 4.4 `Role`

```rust
pub enum Role {
    Symbol(Option<SymbolKey>),   // a device's symbol; the key a drawn form needs (`xref.rs:342`)
    Route, Slash, Reference, LocationBox, FanAnchor, FuseTerm, Drawn,
}
// Representation.role: Stored<Role>
```

Today a symbol's role **is** its starter key (`coil`) or a project symbol's key, so a project
symbol called `route` or `slash` reads as that role. Words become `symbol` and `symbol.<key>`
(`symbol.coil`); the fixed words stay (`route`, `slash`, `reference`, `box`, `fan-anchor`,
`termfuse`, `drawn`). Listed in §13. `look::ANCHOR_ROLE`, `locations::ROLE`,
`derive::REFERENCE_ROLE` and every `r.role == "slash"` (12 sites) go.

### 4.5 Connector `Side`

`symbols::Side` gains `Word` (`socket`, `plug`, `both`). Its 3 hand parsers go; the short forms
`s`/`p` that two of them accept stay as strum aliases (`#[strum(serialize = "socket", serialize =
"s")]`, `word()` gives the first). `location.socket` / `location.plug` come from
`keys::location_side`. The defaults choice `Kind::Choice(&["socket", "plug", "both"])` becomes
`Choice<Side>`.

### 4.6 `TextKind`

`styles/text.rs::noun()` matches on `TextKind` from `keys::TEXT_STYLE.parse(key)` instead of 7
words; `props.rs:1037` reads `Shown` through `Shown::parse` (strum aliases `shown` / `hidden` /
`once per run` for what a person types in the Texts row). `TextKind` and `Shown` gain `Word`;
their hand-written `word()` / `from_word()` go.

### 4.7 `ComponentKind`

The stored `kind` property: `enum ComponentKind { Junction, Continuation, LocationBox, BomItem }`,
read as `Stored<ComponentKind>`. Words unchanged (`junction`, `continuation`, `location_box`,
`bom_item`). Used only by the class (§5).

### 4.8 Defaults and style-field choices

Every `Kind::Choice` gets an enum (or becomes `Kind::Flag`):

| Entry / field | Today's words | Becomes |
| --- | --- | --- |
| 25 switches (§13 F4) | `on`, `off` | `Kind::Flag`, `yes` / `no` |
| `text.tag.show*` (3) | `on`, `off`, `first` | `Shown { Yes, No, First }`, words `yes` / `no` / `first` (§13 F4) |
| `plc.slot.first`, `plc.channel.first` | `0`, `1` | `FirstNumber { Zero = "0", One = "1" }` |
| plc channel naming | `role`, `channel` | `ChannelName` |
| plc address style | `byte.bit`, `iec`, `percent`, `slot`, `none` | `AddressStyle` |
| plc text angle | `90`, `0` | `TextAngle { Upright = "0", Turned = "90" }` (→ `Degrees` when used) |
| plc output safe state | `off`, `on`, `hold` | `SafeState` (three states, not a flag: words stay) |
| connector side, family, pin names, layout | `socket…`, `plain…usbb`, `numbers…`, `row`/`pins` | `Side`, `ConnectorFamily`, `PinNames`, `PinLayout` |
| wiring label rule; wire style label rule | `net`/`each`; `project`/`net`/`each`/`none` | `LabelRule`; `StyleLabelRule` |
| line types (wire, location) | `continuous`, `dashed`, `dashdot`; `LINE_TYPES` | `LineType` (one enum if the two lists agree, else two) |
| text weight; frame block | `regular`/`bold`; `bottom_full`/`bottom_right` | `Weight`; `BlockPosition` |
| print colour; revision mark | `mono`/`colour`; `star`/`draft`/`off` | `PrintColour`; `RevisionMark` |
| strip page form; page `holds` | `table`/`graphic`; `HOLDS` | `StripPageForm`; `Holds` |
| BOM lines; item unit | `flat`/`nested`; `pcs`/`m` | `BomLines`; `ItemUnit` |
| rating basis, method, system, conductor class, material | `BASES`, `METHODS`, `ac1`/`ac3`/`dc`, `1/2/5/6`, `cu` | `RatingBasis`, `InstallMethod`, `SupplySystem`, `ConductorClass`, `Material` |
| one-word choices | `sequence`, `boxed`, `right_then_lift` | `ContinuationNumbers`, `MissingMark`, `ContactTableFallback` (one variant each, ready for the next) |
| model's own | `ReferenceStyle::WORDS`, `COLOUR_STYLE_WORDS`, `DASH_WORDS`, `CASE_WORDS` | `Word` impls on the existing types |

The worker may rename an enum; the words do not change except where §13 says.

---

## 5. `DeviceClass`

### 5.1 Shape

```rust
// ecad-model/src/class.rs
pub enum ComponentClass {
    Device(DeviceClass),
    Junction, Continuation, LocationBox, BomItem,   // from `kind` (§4.7)
    UnknownKind,                                    // a `kind` word this build doesn't know
}
pub enum DeviceClass {
    General,              // coil, contacts, breakers, motors, lamps, sensors, project symbols
    Strip,                // a terminal strip (single and multi-level terminal starters)
    FuseTerminal,         // 034's `fuseterm` device
    Connector,            // mating connector halves (044)
    PlcRack(CardType),    // CPU or head module
    PlcCard(CardType),    // every other card
}
impl ComponentClass {
    pub fn is_strip(self) -> bool; pub fn is_connector(self) -> bool; pub fn is_rack(self) -> bool;
    pub fn is_card(self) -> bool; pub fn is_fuse_terminal(self) -> bool; pub fn has_channels(self) -> bool;
    pub fn is_tagged_kind(self) -> bool;   // a Device: what `locations::is_device` means with a tag
    pub fn bom_line(self) -> BomLine;      // how the BOM counts it (part line, item, none)
}
pub enum StarterClass { Device(DeviceClass), Tagless, Part }   // what `StarterKey::class` gives
```

Every method is an exhaustive `match` (no `_`). The worker takes the exact class of each starter
from today's predicates and `symbols::Kind` (e.g. `rj45`/`usba`/`usbb` bulkheads: today
`is_connector` is false for them because they are not mating; they are `General` unless the
worker finds a caller that needs otherwise, which goes to the overseer).

### 5.2 How a class is decided

In this order, from the document alone:
1. The component's `kind` property → `Junction`, `Continuation`, `LocationBox`, `BomItem` or
   `UnknownKind`.
2. Its definition → starter: `StarterKey::of_definition` for a starter, the `derived_from` origin
   for a project symbol made from one, else `General`. A connector-half starter is `Connector` only
   when the definition mates (`.s` and `.p` in one bridge group, today's `is_mating`), else
   `General` (today's `kind_of_definition` HEAD fallback).
3. No definition: its head representation's starter (today's `plc::rack::body_type`).
4. Otherwise `General`.

### 5.3 Kept in the model's index

`Index` (ecad-model, kept by `Op::apply` for every edit, undo, redo, merge and replay) gains
`classes: BTreeMap<ComponentId, ComponentClass>`, and `Document::class_of(c) -> ComponentClass`
reads it (`O(log n)`, no walk). **Update map:**

| Op | What the class index does |
| --- | --- |
| `CreateComponent` / `DeleteComponent` | classify / remove that component |
| `SetComponentDefinition` | reclassify that component |
| `SetProperty` on a component, key `kind` | reclassify that component |
| `LinkRepresentation` / `ModifyRepresentation` / `UnlinkRepresentation` of a component subject | reclassify that component (rule 3 reads its reps) |
| `EmbedDefinition` / `RemoveDefinition` | reclassify the definition's users (`definition_components`, already indexed) |
| every other op | nothing |

`Index::build` classifies every component once (open, reload). The verify-mode full check already
compares the kept index with a rebuilt one, so a missed reclassification fails there.

### 5.4 The predicates go

The ten predicates (§0) are deleted, not kept as wrappers; their 151 callers call
`doc.class_of(c).is_strip()` and so on. `wires::kind_of(doc, c) -> Option<&str>` goes the same way.
`strips::is_fuse_device` and `bom::is_fuse_terminal` (one asks the definition, one the head
representation) become one `is_fuse_terminal()`; the worker first runs both over the test
projects and the medium bench project and reports any component where they differ.

### 5.5 Adding a device kind: three edits

1. A `StarterKey` variant with its word, in `starter.rs`. The compiler then asks for its
   `definition()` id and its `class()`, both in the same file.
2. Its `STARTERS` row (name, aliases, graphics) in `symbols.rs`. The exhaustive tables there
   (labels, graphics) ask for their arms.
3. Only for a **new class**: a `DeviceClass` variant and its arms in `class.rs`'s capability
   methods.

Its defaults (an area file and its TOML) and its checks are added only when the kind needs new
settings or checks, as with any feature. The review's list of 10–12 files (`devices.rs:391`,
`plc/channels.rs:104`, both setprop chains, `checks.rs`, `outputs.rs` `Scope`, `interact/kind.rs`,
`actions.rs`, `device_list.rs`, `bom.rs`) then reads the class and needs no edit.

---

## 6. Guards (all at 0 at the end of the lane)

Each guard starts as a ratchet at today's count in the step that adds it and is **at 0 with no
allowlist** by the lane's last step (owner: no ratchets left counting down). Textual guards go in
`cargo xtask layers`; guards that need the enums' word lists are tests in ecad-electrical
(`tests/all/lanes/lane_072.rs`, moved to `tests/all/` at the phase-end tidy-up), scanning the
workspace's product sources.

"Today" is this design's estimate; the step that adds a guard records its exact starting count.

| # | Guard | Kind | Today |
| --- | --- | --- | --- |
| G1 | A string literal in key position (`.get("…")`, `.contains_key("…")`, `.remove("…")`, `.insert("…"`, `key: "…"`, `key == "…"`) whose text is a registered key or family prefix, outside `keys.rs`, `defaults/*` entry declarations and the TOML | test (needs `KEYS`) | ~170 |
| G2 | Boolean words compared or matched (`== "yes"`, `"on" =>`, `Some("true"`, …) outside `words.rs` | xtask | 13 |
| G3 | `.all(…is_ascii_digit…)` outside `words.rs` | xtask | ~30 |
| G4 | A string-literal match arm, `matches!` or `==` on a word of `StarterKey`, `StyleCatalogue`, `Role`, `Side`, `TextKind`, `Shown`, `ComponentKind`, `CardType` outside the enum's strum attributes | test (needs `ALL`) | ~180 |
| G5 | `#![deny(clippy::wildcard_enum_match_arm)]` on `ecad-model/src/{words,keys,starter,class,units}.rs`, `ecad-electrical/src/symbols.rs`, `styles/mod.rs` | clippy | — |
| G6 | `fn is_(strip\|connector\|rack\|box\|device\|continuation\|item\|fuse_terminal\|fuse_device)` defined anywhere; the walk `components.get(..)…definition…of_definition` outside `class.rs` | xtask | 10 / ~12 |
| G7 | `to_radians()` / `to_degrees()` outside `units.rs`; a field or parameter named `rotation`, `degrees`, `radians` or `angle` typed `f64`; a name ending `_mm` typed `f64`; a `pub fn` in `rating/parse.rs` returning `Option<f64>` | xtask | 21 / 21 / 3 / 8 |
| G8 | `Kind::Choice(&[`; `.word() == "` / `.word() != "` | xtask | 31 / 10 |
| G9 | `catalogue: &str` / `catalogue: String` / `role: String` in a signature or field | xtask | 23 / 1 |

What the guards deliberately allow: CLI and MCP protocol words (`ui.*` command names, JSON-RPC
method names), search aliases (`actions.rs` keyword lists), UI element ids (`"plc.page"` section
ids), test code.

---

## 7. With lane 070 (typed command inputs)

070 lands first: both lanes change command signatures, and 070's params structs are where a
command's input types are declared. This lane changes those fields' types; the JSON a caller sends
stays the same except where §13 says.

| Params field (070's structs) | Today | Becomes |
| --- | --- | --- |
| `rotation` (`place`, connector place, `placesymbol`, channel place, level place) | `Option<f64>` degrees | `Option<Degrees>` |
| symbol content `slots[].rotation` | `f64` **radians** | `Degrees` (behaviour change, §13 F6) |
| `catalogue` (style commands, `styles`) | `String` / `Option<String>` | `StyleCatalogue` / `Option<StyleCatalogue>` (the schema lists the 7 words) |
| `symbol` / `key` of `place`, `setdefault symbol.*` | `String` | `SymbolKey` |
| `tag_prefixes` of `projectprops` | `BTreeMap<String, Option<String>>` | `BTreeMap<SymbolKey, Option<String>>`, plus `cable_prefix: Option<Option<String>>` (§4.2) |
| connector `halves`, `family`, `layout`, `pin_names` | `String` | `Side`, `ConnectorFamily`, `PinLayout`, `PinNames` |
| `showtext` `kind`, `show` | `String` | `TextKind`, `Shown` |
| lengths in mm (`label_height`, margins, sizes) | `f64` | `Mm` |
| cable `length` | `f64` | `Metres` |
| `setprop` `key` | `String` | stays `String`: a person may set any key. 070's `PROP_RULES` table is keyed by `PropKey` and family instead of strings |

070's lenient readers (`lenient`, a non-bool read as "not given") read JSON, not stored words,
and are outside this lane; the end-gate re-review decides them.

---

## 8. With R9 (design 064, views)

064 §2 already has this lane landing before it. What 064 then uses:

- `Effect::Style { catalogue: &'a Stored<StyleCatalogue>, … }` (064 drew `StyleCatalogue`; a file
  may hold an unknown catalogue, which every view ignores).
- `Effect::Prop { object, key: Key<'a>, … }` with `Key { raw: &'a str, area: Area, known:
  Option<PropKey> }`: `area` from `keys::area_of`, `known` from `keys::info`. A view matches
  `key.known == Some(keys::TAG)` or routes on `key.area`; it never compares `raw`.
- `Area`, `AreaSet` come from `keys.rs` (§2.1), not from 064's `effect.rs`; 064 step 2 imports them.
  064's test C3 (`areas_cover_the_keys`) walks `keys::KEYS`, `keys::FAMILIES` and the defaults
  registry.
- Views that care what a device is (Checks, Outputs for strips, PLC and connectors) read
  `doc.class_of(c)` before and after the commit, like any other document lookup (064 §4). The
  class is not an `Effect`: `Effects::of` needs no document, and the class does.
- 064 §13's "possible later op" 3 (`SetStyle.catalogue`) is done here; 064's "possible later
  typing" 4 (`SetProperty.key`) stays a string, as 064 recommended.

---

## 9. Step order

Every step builds, passes clippy and the tests of the crates it touches, and keeps each guard at
or under its count. The overseer may merge the lane into `refactor/0.2.3` after step 5 if R9 is
ready to start, since R9 needs `keys.rs`, `Area` and `StyleCatalogue`.

0. **Preconditions.** 070 has landed (typed params everywhere) and 073's renames have landed.
   Branch `feature/types` from `refactor/0.2.3`.
1. **Vocabulary, additive.** strum in the workspace. ecad-model `words.rs` (with `Word`,
   `Stored`), `units.rs`, `keys.rs` (`PropKey`, `Area`, `KEYS`, `FAMILIES`, `PropsExt`, the
   family constructors). Guards G2, G3, G8 as ratchets at today's counts. Nothing else changes yet.
2. **Property keys, area by area** (identity and tag; book and page; connectivity and wiring;
   BOM; PLC; connectors; rating; revisions and designations; strips and accessories; front; text
   and symbol families). Each commit moves an area's constants into `keys.rs`, puts its readers on
   `PropsExt` and `words`, and lowers G1–G3. G1 is added in the first of these commits. Ends with
   G1, G2, G3 at 0.
3. **Defaults.** `Entry.key: PropKey`; `Kind::Flag`; `Choice<E>` with the §4.8 enums; the TOML
   regenerated with `yes`/`no`; the Standards screen's toggles. G8 at 0.
4. **Units.** `rating/parse.rs` returns typed values; then `Placement`, `TextSlot`,
   `Primitive::Text` → `Radians`; 070's params → `Degrees`, `Mm`, `Metres`; `Value::Length` →
   `Mm`; `Grid`/`Place` → `PageColumn`/`PageRow`/`PageNo`. G7 at 0.
5. **Small enums.** `StyleCatalogue` (op and map, §13 F1–F2), `Side`, `TextKind` and `Shown`,
   `ComponentKind`. G9's catalogue part at 0.
6. **Starters.** `StarterKey` and `CardType` into ecad-model; `STARTERS` keyed by it; the 11 match
   blocks exhaustive; `SymbolKey`, `PrefixKey`, `cable_prefix` (§13 F5); then `Role` (§13 F3).
   G4, G9 at 0.
7. **Device class.** `class.rs`, the index, `Document::class_of`; an oracle test compares
   `class_of` with the old predicates on every starter and the test projects; then the predicates
   and their callers go. G6 at 0.
8. **Close.** Every guard is a ban with no allowlist; `docs/quick-start/072-types.md`; the
   design's status is set to Done.

---

## 10. Acceptance (tester required: op and format changes)

| # | Test | Checks |
| --- | --- | --- |
| A1 | `one_boolean_rule` | `words::flag` on a table: `yes`, `YES`, ` yes `, `true`, `1`, `on` → true; `no`, `false`, `0`, `off` → false; `y`, `ja`, `` → None. Every flag the app writes is `yes`/`no`. |
| A2 | `whole_numbers` | `uint` / `uint_canonical` at 0, max, max+1, leading zeros, signs, spaces, 20 digits. |
| A3 | `keys_are_registered` | `KEYS` unique; family prefixes unique and not prefixes of each other; every `KEYS` entry and defaults entry has `area_of` ≠ `Free`; each family's `build` then `parse` gives the parts back. |
| A4 | `words_round_trip` | For every `Word` enum: `parse(word(x)) == x`; serde writes `word()`; the JSON Schema's enum equals `WORDS`. |
| A5 | `unknown_words_survive` | A project file with an unknown catalogue, role, `kind` word and choice word opens, saves and reopens byte-equal for those words; nothing acts on them; they are listed (per §14 call 1). |
| A6 | `commands_refuse_unknown_words` | `styles {catalogue: "nope"}`, connector `halves: "x"`, `place {symbol: "nope"}` refused with the uniform input error naming the allowed words. Through `ecad-mcp`'s `execute` too (brief 011 rule): the transaction and `Author::Agent(..)` for one accepted call per changed command family, and refused with writes off. |
| A7 | `degrees_in_radians_stored` | `place {rotation: 90}` stores `Placement.rotation` = π/2; symbol content `slots[].rotation: 90` stores π/2 (was: 90 radians). A file's stored angles read and save byte-equal. |
| A8 | `ratings_typed` | `rating/parse.rs` table tests return the units; the 057 ratings tests are unchanged in outcome. |
| A9 | `class_matches_the_old_predicates` | (step 7, before the predicates go) for one component per `StarterKey`, every `kind` word, and every component of the test projects and the medium bench project: `class_of` agrees with each old predicate; disagreements of the two fuse predicates are reported. |
| A10 | `class_index_kept` | After every op of a corpus (the quick-start trial's log, the medium bench project's log, undo and redo of each, a merge), the kept class index equals a rebuilt one. |
| A11 | `format_words` | A project saved by this build has the §13 words: `yes`/`no` switches, `symbol.<key>` roles, `cable_prefix`. The inspector's Raw and Readable views show them (`.ecad` never a black box). Fixtures regenerated, not shimmed. |
| A12 | guards | `cargo xtask layers` passes with no allowlist entries for G2, G3, G6–G9; the G1/G4 tests pass at 0; clippy passes with G5's denies. |
| A13 | existing suites | fast tests of ecad-model, ecad-electrical, Engine, ecad-app, ecad-mcp, ecad-file unchanged in outcome except where a test asserted a changed word. |

**Bench** (medium while building, large once at the end): `devices.class_scan` (class of every
component, new row, < 1 ms medium); `views.setprop_tag` and `views.move_symbol` no worse than
`refactor/0.2.3` + 10%; project open (index build with classes) no worse than + 5%.

**Quick start.** `docs/quick-start/072-types.md`: what a person can notice. The Standards screen's
on/off settings are switches; where the project's tag prefixes are edited (`sheets_ui.rs`), the
cable prefix is its own line, "Cable tag prefix"; nothing else looks different.

---

## 11. Update maps

Only one derived thing is new: the class index (§5.3 has its map). Every existing view keeps its
own map; where it compared key strings it now compares `PropKey`s, with the same result.

---

## 12. Out of scope

- `Pt` and drawing-unit lengths; the 80 `(f64, f64)` pairs and 11 `[f64; 2]`; `history.rs`'s own
  `Point` (a later tidy-up, or R10).
- `Properties` storage, `Op::SetProperty.key`, and what a person may type as a key.
- The check-kinds table (R7) and the views pipeline (R9).
- Free-text values a person types (`"10 A"`, `"18 AWG"`, a colour): they stay text.
- CADCraft: no file outside `ecad/` changes.

---

## 13. Format and op changes (for the overseer)

Pre-v1, no compatibility path: fixtures are regenerated (ruling). No new op, no format version
change.

| # | Change | On disk |
| --- | --- | --- |
| F1 | `Op::SetStyle.catalogue`: `String` → `Stored<StyleCatalogue>` | same words (`text`, `location`, `frame`, `wire`, `page`, `cable`, `ratingtable`); an op type change in Rust only |
| F2 | `StyleCatalogues` keyed by `Stored<StyleCatalogue>` | same words |
| F3 | `Representation.role`: `String` → `Stored<Role>` | **symbol roles change**: `coil` → `symbol.coil`, a project symbol `xyz` → `symbol.xyz`, `symbol` stays; the fixed words stay |
| F4 | On/off switches → `yes` / `no`; text show → `yes` / `no` / `first` | **words change** in the defaults TOML, in project, page and component properties, and in style fields: `front.cover.{notes,wire_colours,symbol_legend,revisions,structure_codes,customer_site}`, `text.{point_label,description,part_number,continuation_name,reference,continuation_caption,location,wire_label}.show`, the 5 plc switches (`ON_OFF` in `defaults/plc.rs`), `wire.join`, `bom.column.article`, frame fields `grid_labels`, `centre_marks`, page fields `zones`, `checks`; `text.tag.show`, `.strip`, `.plc` and per-object `text.<rep>.<kind>.show` on/off/first |
| F5 | `ProjectSettings.cable_prefix: Option<String>`; `tag_prefixes` keyed by `Stored<SymbolKey>`, no `"cable"` entry | **new settings field**; `tag_prefixes["cable"]` moves to it |
| F6 | Symbol content command: `slots[].rotation` in degrees | command input only; stored stays radians |
| F7 | `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation` typed `Radians` | bytes unchanged (`serde(transparent)`) |

---

## 14. Owner decisions

### Going ahead unless you object

This follows your rulings: one yes/no rule for every switch (the about 25 on/off settings in the
Standards screen become switches and are saved as yes/no, and "y" stops counting as yes), unknown
words kept as written and refused in commands, no compatibility code, and each value defined once
in the lowest crate that needs it. Property keys get one list in the core with typed readers;
lengths, angles, currents, voltages, cross-sections, temperatures and percentages get their own
types where they are parsed, so they can't be mixed up; the text a person types (like "10 A")
stays as typed. Every command takes angles in degrees, so the one symbol command that took radians
changes to degrees. The cable tag prefix becomes its own setting instead of a fake symbol name, so
a project symbol called "cable" can't clash with it. Every guard ends at zero with no exceptions
left. We are **not** changing how points and page coordinates are stored, the check list (R7) or
the views (R9), and 070's lenient JSON readers wait for the end-gate review.

### Needs your call

1. **Words a file holds that this build doesn't know.** A file made by a newer build or another
   tool may hold a style catalogue, a symbol role or a setting word that this build doesn't know.
   As you ruled, ECAD keeps it as written and refuses it in commands. The question is whether a
   person sees it. (a) Show each one as a warning row in Checks ("Unknown word 'x' in setting y:
   kept, not used"), so nothing is silently ignored. (b) Keep them silently; only the developer
   inspector shows them. *Recommended: (a).* It costs one check kind and its row text.
2. **Knowing what kind of device each part is.** Today ten separate tests work out whether a part
   is a terminal strip, a connector, a PLC rack and so on, each by walking from the part to its
   symbol. The design works this out once per part and keeps it up to date on every edit, undo and
   merge, inside the core model. (a) Keep it up to date in the core: lists and checks get the
   answer instantly, and adding a device kind takes about three edits; the cost is that the list of
   built-in symbol names and PLC card types moves down into the core crate. (b) Work it out each
   time it is asked from one function, with nothing kept: the symbol list stays where it is, but
   large lists and checks ask thousands of times per update. *Recommended: (a).*
