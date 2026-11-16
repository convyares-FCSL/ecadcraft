# Design 072: Types for keys, units, words and device kinds

Status: **Draft** (designer, 2026-11-16; revised the same day after the owner's answers). Waiting
for the owner's calls in §14. Format and op changes §13 (F1–F12 with F3′) approved by the
overseer, 2026-11-16. Brief: `docs/briefs/072-types.md`.

**Goal (one sentence).** Property keys, physical units, the words a file stores and the kind of a
device stop being free strings and bare numbers, so that the compiler finds every place a new key,
unit, catalogue or device kind must be handled.

Source: code review 2026-11 (`C:\dev\ecad-features\_tmp\code-review-2026-11.md`), findings 4
(property keys), 6 (strings that should be enums), 7 (a device class) and 10 (units and page
places): lanes R4, R5 and R8 of that review.

**Names.** This design uses today's crate names. Lane 073 renames `ecad-cadcraft` to
`ecad-engine` and moves `xtask` to `dev/xtask` before this lane's worker starts; read "Engine
(`ecad-cadcraft`)" as whatever the tree calls it then. "Device" is the person's word (052) for what
the code calls `Component`; the struct keeps its code name (glossary rule for code names).

**Rulings (not reopened here).**
- strum is approved (design 061) for `word()` / `parse()` on enums.
- Booleans are written `yes` / `no`. Reading also accepts `true` / `1` / `on` (and `false` / `0` /
  `off`). One rule, in one place.
- **Unknown words are a read error** (owner, 2026-11-16, replacing "keep as `Unknown(String)`"):
  pre-v1, ECAD does not read files from newer builds or other tools. A file with a word this build
  doesn't know opens **read-only** with a message naming the word, the setting and where it is (the
  damaged-records path), and the file is never changed. Enums are strict: parse or fail. Files from
  other tools come in later through an explicit import step. Forward compatibility is revisited at
  the v1 format freeze (`docs/deferred-work.md`).
- **A device's kind is data** (owner, 2026-11-16): `Component.kind: DeviceKind`, saved in the
  file, set from the kind the library symbol declares; kind-specific fields live in the variant,
  not in the property bag. No derived class, no index cache, no update map for it, and the built-in
  symbol name list does **not** move into the core. A cable is not a device.
- Pre-v1: **no compatibility path and no legacy debt** (owner, 2026-11-16). Stored words may change
  when that makes them consistent. Fixtures are regenerated, never shimmed. Every on-disk change is
  in §13.
- **One source per value** (owner). Shared vocabulary goes in ecad-model, the lowest crate that
  needs it.
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
| Boolean readers that disagree | 3 word sets | **3 word sets, ~13 sites**: `"yes"\|"y"\|"true"` (`props.rs:1102`), `"yes"\|"true"` (`cmd/numbering.rs:793`), `== "yes"` (`connectivity.rs:109`, `numbering.rs:417,423`, app `shell/right/device.rs:167`), `== "on"` on defaults words (`plc/mod.rs:240,420`, `plc/look.rs:80`, `bom.rs:709`, `cmd/wire.rs:668`) | |
| Digit checks (`.all(is_ascii_digit)`) | 4 copies | **~30 sites**, of which **6** are the same "bounded whole number" reader (`plc/address.rs:70`, `plc/mod.rs:515`, `plc/rack.rs:200,206`, `rating/dataset.rs:89`, `rating/parse.rs:114`) | |
| `Kind::Choice` word lists in defaults | 3 examples | **56** uses, **31** literal lists, **25** of them on/off switches | `defaults/*.rs`, `styles/*.rs`, `rating/mod.rs` |
| `resolve(..).word() == "…"` | — | **10** | `plc/*`, `bom.rs`, `styles/frame.rs`, `front.rs`, `cmd/wire.rs` |
| String-literal `match` arms | 615 | **595** (electrical 230, cadcraft 131, app 122, file 28, inspector 28, mcp 20, bench 15, model 15) | |
| Starter symbols / `match key` blocks on them | 14 blocks | **113 starters**; **11 blocks**, biggest `symbols.rs:1377` (46 arms) and `:1589` (55); `symbols.rs:475,520` repeat the 6 channel keys; `devices.rs:353` (9) | ecad-electrical `symbols.rs`, `devices.rs`, `rating/trace.rs`, `connectors.rs`, `xref.rs` |
| `tag_prefixes` with a fake `"cable"` key | yes | yes: `devices.rs:318`; the map also holds **project symbol keys** (`cmd/symbol.rs:1358`), so a project symbol called `cable` collides | |
| Style catalogue words | 4 words, ~58 lines | **7 catalogues** (text, location, frame, wire, page, cable, ratingtable); `catalogue: &str`/`String` in 23 signatures; 41 literal catalogue words at call sites | `ecad-model/src/{op,style,merge}.rs`, `ecad-electrical/src/styles/*`, `text_style.rs`, `cmd/wire.rs`, Engine `cmd/text.rs`, `props.rs` |
| `Representation.role: String` | 7 words | 9 fixed words (`route`, `slash`, `reference`, `box`, `fan-anchor`, `termfuse`, `drawn`, `symbol`) plus a symbol's **starter key or project symbol key** as its role, in 18 files; read back as a starter key in `xref.rs:342` | |
| Connector side parsed | 3 times | 3 (`cmd/connector.rs:102`, `interact/connectors.rs:26,163`) though `symbols::Side` has `word()`; `locations.rs:457,485`, `proposals.rs:474` build `location.socket`/`plug` by hand | |
| Text kinds as strings | `styles/text.rs:97` | `styles/text.rs:94` `noun()` matches 7 words of `TextKind`; `props.rs:1037` matches `shown`/`on`/… by hand | |
| Device predicates | 9 | **10**: `strips::is_strip`, `connectors::is_connector`, `plc::rack::is_rack`, `locations::is_box`, `locations::is_device`, `continuations::is_continuation`, `bom::is_item`, `bom::is_fuse_terminal`, `strips::is_fuse_device` (two answers to one question), plus the `kind` word reader `wires::kind_of`. **151 calls in ~45 files**. `is_rack` walks the device's representations | |
| Kind-specific fields in the property bag | — | about 30 keys and 8 key families on components (§5.3) | `plc/*`, `strips.rs`, `terminal_types.rs`, `accessories.rs`, `connectors.rs`, `symbols.rs`, `xref.rs`, `continuations.rs`, `connectivity.rs`, `locations.rs`, `bom.rs`, `rating/mod.rs` |
| Angles | mixed | stored radians: `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation`; commands take **degrees**, except the symbol content's `slots[].rotation` (**radians**, `cmd/symbol.rs:204`); `to_radians`/`to_degrees` at **21** sites; 21 fields/params named `rotation`/`degrees`/`angle` typed `f64` | |
| Ratings parsers | 5 return `Option<f64>` | **8**: `current`, `amps`, `voltage`, `cross_section`, `temperature`, `ambient`, `percent`, `length_m` (`rating/parse.rs`) | |
| Page places | `Place`, `zone_text` | unchanged: `Place { page: String, column: u32, row: u32, … }` (`derive.rs:248`), `Grid::column_of/row_of -> u32` (`sheet.rs:648`), `page_number: String` in `derive.rs:288`, `device_list.rs:26`, app `library/*` | |

What is already right and stays: `Properties` stays `BTreeMap<String, String>` for the keys that
are not kind-specific; `Op::SetProperty { key: String }` stays a string (a person may add any key);
the defaults registry (066) already reads through `resolve(&Entry)`; the damaged-records open path
(`ecad_file::disk::ReadOnly::Damaged` and the open report) already opens a project read-only and
keeps the file as it is.

---

## 1. Where things live (one source per value)

| Piece | Crate / module | Why there |
| --- | --- | --- |
| `words`: the one boolean rule, whole-number and digit readers, the `Word` trait | `ecad-model/src/words.rs` | every crate reads stored words |
| `PropKey`, `Area`, `KEYS`, `FAMILIES`, `RESERVED`, `PropsExt`, built-key constructors | `ecad-model/src/keys.rs` (one file per area under `keys/` if it passes ~600 lines) | keys are shared vocabulary |
| Units: `Mm`, `Metres`, `Degrees`, `Radians`, `Amps`, `Volts`, `Mm2`, `Celsius`, `Percent` | `ecad-model/src/units.rs` | `Placement` (model) is the first user |
| `PageColumn`, `PageRow` | `ecad-model/src/sheet.rs` beside `Grid` | `Grid` makes them |
| `PageNo` | `ecad-model/src/book.rs` beside the page-number key | the book owns page numbers |
| `DeviceKind` and its variant data types; `CardType`, `Side`, `SafeState`, `ItemUnit` (stored in a variant) | `ecad-model/src/kind.rs` | stored on `Component` and `Definition` |
| `PartRole` (a symbol's role in its device) and `Role` | `ecad-model/src/doc.rs` beside `Representation` | stored field |
| `StyleCatalogue` | `ecad-model/src/style.rs` | `Op::SetStyle` and `StyleCatalogues` are model types |
| `SymbolName` (a symbol key's shape: a starter's word or a project symbol's key) | `ecad-model/src/doc.rs` | `ProjectSettings.tag_prefixes` is keyed by it |
| `StarterKey`, `SymbolKey` (`Starter(StarterKey) \| Project(String)`), the starter table | stay in `ecad-electrical/src/symbols.rs` | owner: the built-in symbol list stays out of the core |
| `TextKind`, `Shown` | stay `ecad-electrical/src/text_style.rs` | |
| The open-time word check (§4.1) | `ecad-electrical/src/words_check.rs` | it needs the electrical enums |
| The choice enums of the defaults and style fields (§4.8) | next to their entries in `ecad-electrical/src/defaults/*`, `styles/*`, `rating/*`; those ecad-model already has (`ReferenceStyle`, colour style, tag dash and case) stay there | |

`strum` becomes a workspace dependency of ecad-model and ecad-electrical. JSON Schema for the new
enums, units and `DeviceKind` comes behind ecad-model's existing `schema` feature (070's pattern
for ids), so 070's params structs can use them.

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
/// Keys that moved into a `DeviceKind` variant (§5.3): a component holding one is a read error.
pub static RESERVED: &[(&str, &str)];   // (key or family prefix, "the PLC card's slot")

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
    }
    // …
}
```

The constants are grouped by area (`keys::identity::TAG`) and re-exported flat (`keys::TAG`) when
the name is unique. The worker takes `max_chars` and ranges from today's checks, never inventing
new ones.

**`Area`** is the enum design 064 §3.3 drew, defined **here** (not in 064's `effect.rs`), so the
registry and the views share one list: the 17 registry areas (`Text`, `Wiring`, `Wire`, `Tags`,
`Revisions`, `Print`, `Symbol`, `Terminal`, `Connector`, `StripPage`, `Bom`, `Location`,
`Continuation`, `Plc`, `Page`, `Front`, `Rating`) and the stored areas outside the registry
(`Identity`, `BomItem`, `Designation`, `Book`, `RevisionRow`, `Link`, `TagHighWater`,
`CableCores`, `Generated`, `Connectivity` for `wire_number`/`potential`) and `Free` for a person's
own key. 064's `StripAccessory` and `ContinuationId` areas are not needed: those keys move into
device kinds (§5.3). `defaults::AREAS` becomes `Area::REGISTRY`'s words (064 step 2 then only
consumes it). `area_of` reads `KEYS`, then `FAMILIES`, then a registry area word before the first
`.`, else `Free`.

**What the registry does not hold.** The defaults registry's entries (`text.font`,
`plc.slot.first`, …) stay in `ecad-electrical/src/defaults/*` as today (066): they are the source
of their keys. `Entry.key` becomes a `PropKey` (built with `PropKey::new` in the entry), so a
defaults key is greppable and typed, but is not repeated in `KEYS`. Style **field** keys
(`grid_labels`, `line_type`) are a catalogue's fields and stay in `styles/*` (`Field.key`).

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

Every copy found in §0 goes through these: the boolean sites, the 6 bounded-number readers and the
remaining digit checks (`tag.rs`, `links.rs`, `bom.rs`, `channels.rs`, `strips.rs`, `symbols.rs`,
`trace.rs`, `tagtable.rs`, app `project_panel.rs`, `status_bar.rs`). `y` stops being read as yes
(`props.rs:1102`), which the ruling's word list leaves out. A prompt's "yes / cancel" answer
(`interact/devices.rs:29`) is a keyword typed at a prompt, not a stored word: it stays.

**Writing a flag.** A flag property that is false is removed when absence means false for that key
(`tag_fixed`, `wire_number_fixed`); a defaults entry level that overrides an inherited "yes" writes
`no`. The rule sits in `PropsExt::set_flag` and the defaults writer, nowhere else.

### 2.3 Typed readers

```rust
pub trait PropsExt {
    fn get_key(&self, k: PropKey) -> Option<&str>;
    fn has(&self, k: PropKey) -> bool;
    fn flag(&self, k: PropKey) -> bool;                     // absent = false
    fn uint(&self, k: PropKey) -> Option<u32>;              // max from the key's ValueSpec
    fn word<E: Word>(&self, k: PropKey) -> Option<E>;
    fn text(&self, k: PropKey) -> Option<&str>;             // trimmed, non-empty
}
impl PropsExt for Properties { /* … */ }

/// The op that sets `k` on `object`, the value written by the key's ValueSpec rule.
pub fn set(object: ObjectRef, k: PropKey, before: Option<String>, after: Option<String>) -> Op;
```

A stored value that doesn't read (a flag `maybe`, a word not in its enum) is caught when the file
is opened (§4.1), so the readers can treat it as absent without hiding anything.

### 2.4 Built keys

Every `format!`-built key and every `strip_prefix` that reads one goes through its family. A
family's constructor takes typed parts:

| Family (prefix) | Parts | Constructor | Lives in |
| --- | --- | --- | --- |
| `rev.<n>.<column>` | `Uint`, `Word` | `keys::rev(n, col)` | model |
| `designation.<aspect>.<entry>.{code,description}` | `Word`, `Name`, `Word` | `keys::designation(..)` | model |
| `link.<…>` | as `links.rs` | `keys::link(..)` | model |
| `numbering.high.<prefix>` | `Name` | `keys::tag_high(prefix)` | model |
| `unused.<core>` (cables) | `Name` | `keys::unused_core(core)` | model |
| `location.<side>` | `Word(Side::WORDS)` | `keys::location_side(side)` | model |
| `folder.<id>.start` | `Hex` | `keys::folder_start(id)` | model |
| `text.<kind>.style`, `text.<kind>.show`, `text.<rep>.<kind>.show` | `Word(TextKind::WORDS)`, `Hex` | `TextKind::style_key(self)`, `show_key(self, rep: Option<RepresentationId>)` in electrical, through `keys::TEXT_STYLE.build(..)` | family in model, typed method in electrical |
| `symbol.<key>.<field>` | `Name`, `Word` | `defaults::symbols::family_key(SymbolKey, Field)` through `keys::SYMBOL_FAMILY` | same |

The component families `plc.ch.<i>.*`, `half.*`, `ignore.*`, `acc.*`, `term_label.*`, `rating.<n>`
are not families any more: they move into device kinds (§5.3) and their prefixes go to
`RESERVED`. A family's `build` and `parse` are one pair, so a key is never split by a different
rule than the one that made it. `TextKind` stays in electrical; the family records its words through
a `Part::Word` checked by a test (`families_name_their_words`) rather than by moving the enum.

### 2.5 Defaults entries

- `Entry.key: PropKey`.
- `Kind::Flag` for the 25 on/off switches (§13 F4): `Value::Flag(bool)`; the Standards screen
  shows a `control::Toggle` for them (components rule, owner 2026-11-15); the TOML writes `yes`/`no`.
- `Kind::Choice(&'static [&'static str])` stays the registry's data shape but is built only from an
  enum: `Kind::choice::<E: Word>()` (a `const fn` over `E::WORDS`). A choice entry is a typed static,
  `pub static SLOT_FIRST: Choice<FirstNumber> = Choice::new(Entry { … })`, whose
  `resolve(doc, at) -> FirstNumber` replaces `.word() == "…"`. Every `Kind::Choice(&[…])` literal
  goes (31 now).

### 2.6 Inventory: what moves

| From | Keys | To |
| --- | --- | --- |
| `ecad-model/src/tag.rs` | `TAG_KEY`, `FIXED_KEY`, `PROVISIONAL_SETTING`, `HIGH_RESET_PREFIX`, `DASH_KEY`, `CASE_KEY` | `keys::{TAG, TAG_FIXED}`; `HIGH_RESET_PREFIX` → family; the registry ones stay entries |
| `book.rs`, `sheet.rs` | `folder`, `number`, `function`, `location`, `sheet`, `title_block`, `page.type`, `generated`, `print.frame.style` | `keys::book::*`, `keys::page::*` |
| `connectivity.rs`, `wiring.rs` | `wire_number`, `wire_number_fixed`, `potential`, `cross_section`, `colour`, `wire.category` | `keys::connectivity::*`, `keys::wiring::*` |
| `connectivity.rs` | `kind`, `continuation`, `junction`, `name` | into `DeviceKind` (§5.3); `kind`, `name` reserved on components |
| `links.rs`, `derive.rs`, `builtins.rs` | `link.` prefix; `REFERENCE_ROLE` (a role, §4.4); the 6 settings words in `builtins.rs` are TOML field names (066), not keys | family; `Role::Reference` |
| electrical `bom`, `designations`, `frames`, `front`, `generated`, `page_types`, `rating/mod` (circuit keys), `revisions`, `styles/cable` | their constants | `keys::<area>::*` |
| electrical `plc/*`, `strips`, `terminal_types`, `accessories`, `connectors`, `symbols` (`rating`), `xref` (`contacts`, `mirror`), `continuations` (`caption`), `locations`, `bom` (`qty`, `unit`) | kind-specific | into `DeviceKind` (§5.3) |
| every crate | the 42 `"tag"`, 37 `"description"`, 20 `"part_number"`, 12 `"maker"`, 12 `"type"`, 1 `"article_number"` literal keys | constants |

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
| `Amps`, `Volts`, `Mm2`, `Celsius`, `Percent`, `Metres` | `rating/parse.rs`: `current`/`amps → Amps`, `voltage → Volts`, `cross_section → Mm2`, `temperature`/`ambient → Celsius`, `percent → Percent`, `length_m → Metres` | `rating/{mod,dataset,trace}.rs`, the rating checks rows, `lists/rating.rs` (app). The free text a person typed (`"10 A"`, `"18 AWG"`) stays text: the types are what a parse gives, not what is stored |
| `Radians` | `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation` (all stored radians today; the worker confirms the last with a round-trip test before typing it) | `translate.rs` (Engine → CADCraft), `look.rs` `Text { rotation }`, the layout code in electrical |
| `Degrees` | every command parameter named `rotation` (070's params structs: `place`, connector place, `placesymbol`, channel place, symbol content `slots[].rotation`); interactive prompts; the plc text-angle choice `"90"`/`"0"` (§4.8) | `Radians::from(Degrees)` at the one place a command builds a `Placement` |
| `Mm` | `defaults::Value::Length` and `Resolved::length`, style `Length` fields, `TextLook.height`, sheet `Margins`, ecad-drawing-style line weights | electrical layout (`text_style`, `frames`, `title_block`, `plc/look`) |
| `PageColumn(u32)`, `PageRow(u32)` | `Grid::column_of`, `row_of`, `column_label`, `row_label`; `Place.column`, `Place.row` | `zone_text(column: &str, row: &str)` becomes `zone_text(&Place)`; xref, continuations, reference texts |
| `PageNo(String)` | `Place.page`, `CableReference.page_number` (`derive.rs:288`), `device_list::Row.page_number`, the book's page-number reader | sorting (one `Ord` for page numbers) and the app's `library/*` rows |

**Symbol content `slots[].rotation`** is the one command parameter that takes radians today. With
`Degrees` in its params struct it takes degrees like every other command (§13 F6).

---

## 4. Enums

### 4.1 One pattern: `Word`, strict, and the open-time word check

```rust
// ecad-model/src/words.rs
pub trait Word: Copy + Eq + 'static {
    const ALL: &'static [Self];
    const WORDS: &'static [&'static str];
    fn word(self) -> &'static str;
    fn parse(text: &str) -> Option<Self>;   // trimmed, any case
}
```

- Each enum derives strum's `IntoStaticStr`, `EnumString`, `VariantArray` with
  `#[strum(serialize_all = "snake_case", ascii_case_insensitive)]`; a word that is not snake case
  (`byte.bit`, `fan-anchor`, `0`, `90`) gets its own `#[strum(serialize = "…")]`. The strum
  attribute is the only place a word is written. A small macro implements `Word`,
  `Serialize`/`Deserialize` (as the word; an unknown word is a deserialise error naming the word
  and the allowed ones) and `JsonSchema` (an enum of `WORDS`) from it.
- There is **no `Unknown` variant and no `Stored<E>`**. Enums are exhaustive and `Copy`.
- **Commands** read `E` through 070's params structs: an unknown word is refused with 070's
  uniform input error, which lists the allowed words from the schema.
- **Files.** An unknown word is a read error, reported on the damaged-records path:
  1. *Typed fields* (a model enum in a record: `Op::SetStyle.catalogue`, a style catalogue map key,
     `Representation.role`, `Component.kind`, `Definition.kind`): the record does not decode. It is
     reported as damage with the word, the field and the record, exactly as a damaged record is
     today: the project opens read-only (`ReadOnly::Damaged`), the record is left out of the
     in-memory document, and the file is kept as it is.
  2. *Words inside text values* (a property or setting whose value is a word of an electrical
     enum: defaults choices, `Shown`, `tag_prefixes` keys, a component holding a `RESERVED` key):
     `ecad_electrical::words_check::check(doc) -> Vec<WordFault>` runs once when a project opens.
     A fault opens the project read-only through the same report, with a new reason
     `ReadOnly::UnknownWords` whose text names each word, its setting and where it is ("`plc.frame.show`
     on page 5 is `maybe`, which this build doesn't read"). Nothing is left out: the document is
     whole, only saving is refused.
- The open report lists both kinds in one list; the inspector shows them.

### 4.2 `StarterKey`, `SymbolKey`, the tag prefixes

```rust
// ecad-electrical/src/symbols.rs (the built-in list stays out of the core)
pub enum StarterKey { Coil, No, Nc, Power3, Mcb1, Mcb2, Fuse, Psu, /* … all 113, flat … */ Chiol }
pub enum SymbolKey { Starter(StarterKey), Project(String) }   // a project symbol's key (048)
```

- **Flat**, one variant per starter: the starter list is a flat catalogue, the words are what a
  person types, and families are answered by exhaustive methods (`card_type`, `channel_type`,
  `side`, `levels`). `STARTERS` rows carry `key: StarterKey` and `kind: DeviceKind` (the kind the
  starter declares, §5.2); a test checks one row per key in `StarterKey::ALL` order. `by_key(&str)`
  becomes `StarterKey::parse` + `starter(k)`.
- The 11 `match key` blocks match on `StarterKey`, with no `_` (§6 G5): a new starter does not
  compile until each table answers for it. Grouped arms (`A | B | C =>`) keep them short.
- **Tag prefixes.** `tag_prefixes` is keyed by `SymbolName` (model: a key's shape, 1–16
  characters); the open-time check makes a key that names neither a starter nor a project symbol a
  read error. The fake `"cable"` entry becomes its own setting, `ProjectSettings.cable_prefix:
  Option<String>` (absent = `W`), so a project symbol called `cable` can't collide (§13 F5).

### 4.3 `StyleCatalogue`

```rust
pub enum StyleCatalogue { Text, Location, Frame, Wire, Page, Cable, Ratingtable }
```

Includes 057's `cable` and `ratingtable` and the `page` catalogue. `Op::SetStyle.catalogue:
StyleCatalogue`; `StyleCatalogues = BTreeMap<StyleCatalogue, …>`; `styles::catalogue` takes the
enum and its `CATALOGUES` table is an exhaustive `match`; `defaults::Kind::Style(StyleCatalogue)`.
The 23 `catalogue: &str` signatures and the 41 literal words go. Words do not change.

### 4.4 `Role`: a symbol's role in its device

The owner's shape: a symbol on a page carries only its role in the device; what the device is lives
on the device (§5).

```rust
/// A symbol's part in its device: today's `reps::RepType`, now stored.
pub enum PartRole { Head, Contact, Pole, Terminal, Channel, Half, Pin, Other }
pub enum Role {
    Part(PartRole),      // a device's symbol: words `head`, `contact`, `pole`, `terminal`, …
    Route, Slash, Reference, LocationBox, FanAnchor, FuseTerm, Drawn,
}
```

Today a symbol's role is its starter key (`coil`) or a project symbol's key, and `reps::rep_type`
derives the part from the definition each time. `PartRole` is set when the symbol is placed (from
the declaring symbol: the starter row or the project symbol) and stored. Which symbol it is comes
from the definition, never from the role. The fixed words stay (`route`, `slash`, `reference`,
`box`, `fan-anchor`, `termfuse`, `drawn`). `look::ANCHOR_ROLE`, `locations::ROLE`,
`derive::REFERENCE_ROLE`, every `r.role == "slash"` (12 sites) and `reps::rep_type`'s derivation go.
**A drawn form** (`Form::Drawn`, a converted drawing) has no definition, and `xref.rs:342` reads its
starter from the role today to find its contact table. The worker checks whether any drawn form
needs that; if so it is an open question for the overseer, not a role word (§13 F3′).

### 4.5 Connector `Side`

`Side` (`socket`, `plug`, `both`) moves to `ecad-model/src/kind.rs`, because the connector variant
stores it (§5.3). Its 3 hand parsers go; the short forms `s`/`p` stay as strum aliases for typed
input (`word()` gives the full word, which is what is stored). `location.socket` / `location.plug`
come from `keys::location_side`. The defaults choice becomes `Choice<Side>`.

### 4.6 `TextKind`

`styles/text.rs::noun()` matches on `TextKind` from `keys::TEXT_STYLE.parse(key)` instead of 7
words; `props.rs:1037` reads `Shown` through `Shown::parse` (strum aliases `shown` / `hidden` /
`once per run` for what a person types in the Texts row). `TextKind` and `Shown` gain `Word`;
their hand-written `word()` / `from_word()` go.

### 4.7 The `kind` property

Goes: junctions, continuations, location boxes and items are device kinds (§5), and `kind` is a
`RESERVED` key on components.

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
| plc output safe state | `off`, `on`, `hold` | `SafeState` (three states, not a flag: words stay; model, §5.3) |
| connector side, family, pin names, layout | `socket…`, `plain…usbb`, `numbers…`, `row`/`pins` | `Side`, `ConnectorFamily`, `PinNames`, `PinLayout` |
| wiring label rule; wire style label rule | `net`/`each`; `project`/`net`/`each`/`none` | `LabelRule`; `StyleLabelRule` |
| line types (wire, location) | `continuous`, `dashed`, `dashdot`; `LINE_TYPES` | `LineType` (one enum if the two lists agree, else two) |
| text weight; frame block | `regular`/`bold`; `bottom_full`/`bottom_right` | `Weight`; `BlockPosition` |
| print colour; revision mark | `mono`/`colour`; `star`/`draft`/`off` | `PrintColour`; `RevisionMark` |
| strip page form; page `holds` | `table`/`graphic`; `HOLDS` | `StripPageForm`; `Holds` |
| BOM lines; item unit | `flat`/`nested`; `pcs`/`m` | `BomLines`; `ItemUnit` (model, §5.3) |
| rating basis, method, system, conductor class, material | `BASES`, `METHODS`, `ac1`/`ac3`/`dc`, `1/2/5/6`, `cu` | `RatingBasis`, `InstallMethod`, `SupplySystem`, `ConductorClass`, `Material` |
| one-word choices | `sequence`, `boxed`, `right_then_lift` | `ContinuationNumbers`, `MissingMark`, `ContactTableFallback` (one variant each, ready for the next) |
| model's own | `ReferenceStyle::WORDS`, `COLOUR_STYLE_WORDS`, `DASH_WORDS`, `CASE_WORDS` | `Word` impls on the existing types |

The worker may rename an enum; the words do not change except where §13 says.

---

## 5. Device kinds are data

### 5.1 The shape

Top-level objects stay their own structs: device (`Component`), cable, wire (`Conductor`), page,
folder. A cable is not a device and gets no kind.

```rust
// ecad-model/src/doc.rs
pub struct Component {
    pub properties: Properties,          // the generic fields only (§5.4)
    pub definition: Option<DefinitionId>,
    pub kind: DeviceKind,                // new, saved
}
pub struct Definition { /* … as today … */ pub kind: DeviceKind }   // the kind a new device from it gets

// ecad-model/src/kind.rs
pub enum DeviceKind {
    General,                               // coils, contacts, motors, lamps, sensors, valves, PSUs, …
    Relay(Relay),                          // see §14 call 4
    Protective(Protective),                // see §14 call 1
    TerminalStrip(TerminalStrip),
    PlcCard(PlcCard),                      // a rack is a card whose type is CPU or head module (§14 call 2)
    Connector(Connector),
    Continuation(Continuation),            // see §14 call 3 for these four
    Junction,
    LocationBox(LocationBox),
    Item(Item),
}
```

Every variant's struct holds only what that kind has (§5.3), so a terminal strip can't have a PLC
slot. `DeviceKind` has `Word` for its variant word (`general`, `relay`, `protective`,
`terminal_strip`, `plc_card`, `connector`, `continuation`, `junction`, `location_box`, `item`) via
strum's `EnumDiscriminants`, and serialises as `{ "kind": "<word>", …fields }`; unknown word or
unknown field → the record doesn't decode (§4.1). Default fields are left out of the file
(`skip_serializing_if` on each), so a plain device stays small.

Capability questions are `match`es on the kind, with no `_`: `kind.is_strip()`, `plc_card()`,
`connector()`… are one-line methods on `DeviceKind` in `kind.rs`, and checks and views read
`component.kind` directly. Nothing is derived and nothing is cached.

### 5.2 Where a kind comes from

- **The library symbol declares it.** `Definition.kind` holds the kind (with its default data, e.g.
  `PlcCard { card_type: Dq, .. }`) a device made from it gets. A starter's comes from its
  `STARTERS` row; a project symbol's (048) is chosen in the symbol builder, defaulting to its
  `derived_from` starter's kind, else `General`.
- **Placing a symbol as a new device** creates the component with `kind = definition.kind.clone()`.
- **Adding a symbol to an existing device** (a contact to a relay, a channel to a card, a level to
  a strip) never changes the kind. Whether the symbol may join that kind stays the join rule it is
  today (`symbols::Kind`, renamed `JoinRule` so the two words don't collide).
- **Changing the kind is one explicit edit**: `SetDeviceKind` (§5.5). Swapping a device's symbol
  (`plccardtype`, the symbol swap in the inspector) records `SetComponentDefinition` and
  `SetDeviceKind` in one transaction: the kind is re-read from the new symbol, keeping the fields the
  old and new variant share (a card type change keeps rack, slot, address and channel data; the
  existing "fewer channels than signals" refusal stays).
- `setprop` on a key that moved into a kind (`plc.slot`, `pins`, `rating`, …) keeps working for
  people and agents: 070's `PROP_RULES` routes it to an edit of the variant field and records
  `SetDeviceKind`. Command names and their JSON do not change.

### 5.3 What moves into each variant

Every component property today that only one kind has. Types are the stored types; free text a
person types stays `String`.

| Variant | Field | From today's key | Today's reader |
| --- | --- | --- | --- |
| **PlcCard** | `card_type: CardType` | (today derived from the head symbol, `plc::rack::body_type`) | `plc/rack.rs:188` |
| | `rack: Option<ComponentId>` | `plc.rack` | `plc/rack.rs:23` |
| | `slot: Option<u32>` | `plc.slot` | `plc/rack.rs:25` |
| | `channels: Option<u16>` | `plc.channels` | `plc/rack.rs:27` |
| | `address: AddressPin { start: Option<String>, size: Option<u32> }` | `plc.addr.start`, `plc.addr.size` | `plc/address.rs:24,26` |
| | `rating_words: Option<String>` (`24 V`, `relay`) | `plc.kind` | `plc/overview.rs:215` |
| | `channel_data: BTreeMap<u16, Channel { text, address, tag, safe: Option<SafeState> }>` | `plc.ch.<i>.text`, `.address`, `.tag`, `.safe` | `plc/channels.rs:30`, `plc/signals.rs:31` |
| **TerminalStrip** | `offer: Option<String>` (positions offered) | `terminals` | `strips.rs:55` |
| | `pe_part: Option<Part { part_number, maker }>` | `part_number.pe`, `maker.pe` | `strips.rs:59,60` |
| | `level_labels: BTreeMap<(String, u8), String>` | `term_label.<position>.<level>` | `terminal_types.rs:26` |
| | `fuse_ratings: BTreeMap<String, String>` (fuse positions) | `rating.<position>` | `terminal_types.rs:28` |
| | `accessories: Accessories { start, end, after: BTreeMap<String, String>, on: BTreeMap<String, String>, marker_per_card: Option<u32>, jumper_bar: Option<u32> }` | `acc.start`, `acc.end`, `acc.after.<p>`, `acc.on.<p>`, `marker.per_card`, `jumper.bar` | `accessories.rs:26–39` |
| **Connector** | `pins: Option<u16>` | `pins` | `connectors.rs:43` |
| | `pin_names: Option<String>` | `pin_names` | `connectors.rs:46` |
| | `halves: BTreeMap<Side, HalfPart { description, maker, part_number }>` | `half.<side>.{description,maker,part_number}` | `connectors.rs:48–51` |
| | `ignored: BTreeSet<ConnectorCheck>` | `ignore.<check>` | `connectors.rs:54` (only connector checks use it) |
| **Protective** | `rating: Option<String>` (`13A C 30mA`) | `rating` | `symbols.rs:836` |
| | `outputs: BTreeMap<String, String>` (a fuse bank's per-output rating) | `rating.<n>` | `symbols.rs:839` |
| **Relay** | `contacts: Option<String>` (`3P+1NO+1NC`) | `contacts` | `xref.rs:67` |
| | `mirror: Option<String>` (where its contact table goes) | `mirror` | `xref.rs:69` |
| **Continuation** | `name: String`, `caption: Option<String>` | `kind=continuation`, `name`, `caption` | `connectivity.rs:50–56`, `continuations.rs:40` |
| **Junction** | — | `kind=junction` | `connectivity.rs:54` |
| **LocationBox** | `panel_temperature: Option<String>` | `rating.temperature` | `rating/mod.rs:190` |
| **Item** | `qty: Option<String>`, `unit: ItemUnit` | `kind=bom_item`, `qty`, `unit` | `bom.rs:29–36` |

The worker confirms each holder against the code (some readers accept more than one holder) and
stops if a key turns out to sit on more than one kind (§9 step 7).

### 5.4 What stays generic on every device

`tag`, `tag_fixed`, `description`, `function`, `location`, `part_number`, `maker`,
`article_number`, `type`, the part lines `part.<n>.*` (devices, strips and items all have them),
the per-object text choices `text.<rep>.<kind>.show` and `text.<kind>.style`, `link.*`, and any
free key a person adds. Wire and cable keys (`wire_number`, `potential`, `cross_section`, `colour`,
`load`, `protected_by`, `rating.basis` and the other circuit `rating.*` keys, `cores`, `unused.*`)
stay on wires and cables: they are not devices.

### 5.5 The op

**One new op, for the overseer:**

```rust
Op::SetDeviceKind { id: ComponentId, before: DeviceKind, after: DeviceKind }
```

- Records a kind change and every edit of a variant field (a card's slot, a strip's accessory),
  carrying the whole kind before and after, the way `SetProjectSettings` carries the whole
  settings struct. Its inverse swaps them; it is atomic and checked like every op (it joins
  `Op::ALL`, the contract tests' list, and design 064's `Effects::of`).
- Why not an existing op: `SetProperty` holds a key and a text, which is what this change takes
  the fields away from; `SetComponentDefinition` changes the symbol, not the kind, and a kind field
  edit (a slot) changes no symbol. `CreateComponent` and `DeleteComponent` carry the kind inside
  the `Component` they already carry.
- Invariants it adds: a card's `rack` names a component whose kind is a rack card; the index keeps
  a `rack → cards` reverse map (like `definition_components`) so the invariant check and the rack
  views don't scan.

### 5.6 The predicates go

The ten predicates (§0) and `wires::kind_of` are deleted; their 151 callers read `component.kind`.
`strips::is_fuse_device` and `bom::is_fuse_terminal` (one asks the definition, one the head symbol)
become the one kind the owner picks (§14 call 1). The name-matching on starter keys that decided
what a device is (`devices.rs:353`, `rating/trace.rs:67`, `symbols.rs:851` `is_fuse_bank`,
`plc::rack::body_type`, `CardType::of_body`) goes: the kind says it.

### 5.7 Adding a device kind

1. A `DeviceKind` variant and its data struct in `kind.rs`; the compiler asks for its arms in the
   capability methods.
2. The starters (or project symbols) that declare it.
3. The rules that read its fields.

A new **symbol** of an existing kind is one `StarterKey` variant and its `STARTERS` row; the
exhaustive symbol tables ask for their arms.

### 5.8 Test files and fixtures

No shims and no converting reader: every fixture is regenerated by the code that made it.
- `crates/ecad-file/tests/golden/project.ecad`: re-made by the test that writes it (or a `child_`
  helper behind `ECAD_TEST_CHILD`, if none exists), committed with the regenerated file.
- Projects built in tests through `ProjectBuilder` (062) and commands get their kinds from the
  symbols they place: nothing to do beyond the builder setting `kind` from `Definition.kind`.
- Tests that build a `Component` literal by hand get `kind:` (usually `DeviceKind::General`); a
  test that wrote a moved key into the property bag writes the variant field instead.
- ecad-bench's generated tiers (`generate.rs` writes a role `coil` and the moved keys today) are
  generated with kinds; the quick-start's example projects are re-made by their scripts.
- `tests/fixtures/plc/*.csv` are tag-table imports, not projects: unchanged.

---

## 6. Guards (all at 0 at the end of the lane)

Each guard starts as a ratchet at today's count in the step that adds it and is **at 0 with no
allowlist** by the lane's last step. Textual guards go in `cargo xtask layers`; guards that need
the enums' word lists or the key registry are tests in ecad-electrical
(`tests/all/lanes/lane_072.rs`, moved to `tests/all/` at the phase-end tidy-up), scanning the
workspace's product sources. "Today" is this design's estimate; the step that adds a guard records
its exact starting count.

| # | Guard | Kind | Today |
| --- | --- | --- | --- |
| G1 | A string literal in key position (`.get("…")`, `.contains_key("…")`, `.remove("…")`, `.insert("…"`, `key: "…"`, `key == "…"`) whose text is a registered or reserved key or family prefix, outside `keys.rs`, `defaults/*` entry declarations and the TOML | test | ~170 |
| G2 | Boolean words compared or matched (`== "yes"`, `"on" =>`, `Some("true"`, …) outside `words.rs` | xtask | 13 |
| G3 | `.all(…is_ascii_digit…)` outside `words.rs` | xtask | ~30 |
| G4 | A string-literal match arm, `matches!` or `==` on a word of `StarterKey`, `StyleCatalogue`, `Role`, `PartRole`, `Side`, `TextKind`, `Shown`, `CardType`, `DeviceKind` outside the enum's strum attributes | test | ~180 |
| G5 | `#![deny(clippy::wildcard_enum_match_arm)]` on `ecad-model/src/{words,keys,kind,units}.rs`, `ecad-electrical/src/symbols.rs`, `styles/mod.rs` | clippy | — |
| G6 | `fn is_(strip\|connector\|rack\|box\|device\|continuation\|item\|fuse_terminal\|fuse_device)` defined anywhere; `body_type(`, `of_body(`, `kind_of(` on a component | xtask | 10 / ~12 |
| G7 | `to_radians()` / `to_degrees()` outside `units.rs`; a field or parameter named `rotation`, `degrees`, `radians` or `angle` typed `f64`; a name ending `_mm` typed `f64`; a `pub fn` in `rating/parse.rs` returning `Option<f64>` | xtask | 21 / 21 / 3 / 8 |
| G8 | `Kind::Choice(&[`; `.word() == "` / `.word() != "` | xtask | 31 / 10 |
| G9 | `catalogue: &str` / `catalogue: String` / `role: String` in a signature or field | xtask | 23 / 1 |
| G10 | `Unknown(` variants, `Stored<`, and `_ =>` arms in a `Word` deserialiser (no lenient reading of stored words) | xtask | 0 (keeps it there) |

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
| symbol content `slots[].rotation` | `f64` **radians** | `Degrees` (§13 F6) |
| `catalogue` (style commands, `styles`) | `String` / `Option<String>` | `StyleCatalogue` / `Option<StyleCatalogue>` |
| `symbol` / `key` of `place`, `setdefault symbol.*` | `String` | `SymbolKey` |
| `tag_prefixes` of `projectprops` | `BTreeMap<String, Option<String>>` | `BTreeMap<SymbolKey, Option<String>>`, plus `cable_prefix: Option<Option<String>>` |
| connector `halves`, `family`, `layout`, `pin_names` | `String` | `Side`, `ConnectorFamily`, `PinLayout`, `PinNames` |
| `plcslot`, `plcaddress`, `plccardtype`, `plcchannel`, connector pins, `partline` on strips, accessory commands | build `SetProperty` ops | build `SetDeviceKind` from the edited variant |
| symbol builder (048) | — | `kind` the new symbol declares (`DeviceKind` word plus its defaults) |
| `showtext` `kind`, `show` | `String` | `TextKind`, `Shown` |
| lengths in mm (`label_height`, margins, sizes) | `f64` | `Mm` |
| cable `length` | `f64` | `Metres` |
| `setprop` `key` | `String` | stays `String`: a person may set any key. `PROP_RULES` is keyed by `PropKey`, family, and the moved keys (routed to `SetDeviceKind`, §5.2) |

070's lenient readers (`lenient`, a non-bool read as "not given") read JSON input, not stored
words, and are outside this lane; the end-gate re-review decides them.

---

## 8. With R9 (design 064, views)

064 §2 already has this lane landing before it. Re-checked against the owner's answers:

- **Views read `device.kind`**, not a derived class. There is no class index and no update map for
  kinds; a kind is stored data like a property.
- **A kind change is an edit like any other.** `Effects::of` maps the new op:
  `SetDeviceKind` → `Effect::Kind { id: ComponentId, before: &'a DeviceKind, after: &'a DeviceKind }`
  (a new `EffectKind`). `CreateComponent` / `DeleteComponent` already carry the whole `Component`,
  kind included, in `Effect::Component`.
- **Routing by kind.** A view that cares about one kind (`OutputsView` for PLC cards and strips,
  `ConnectorsView`, `ChecksView`'s strip, connector and protective rows) matches the variant on
  `Effect::Kind` and `Effect::Component` (`before`/`after`), with no `_`. 064's `Prop` routing for
  the moved keys goes: `SetProperty(card, plc.channels | plc.addr.* | plc.ch.<i>.*)` rows in the
  PLC update map (the `SetProperty(card, …)` rows of `docs/design/045.md`'s update-map table)
  become `Kind` rows; the
  `StripAccessory`, `ContinuationId` and component `Connector` areas leave 064's `Area` list.
- `Effect::Style { catalogue: StyleCatalogue, … }` as 064 drew it (strict now: no unknown).
- `Effect::Prop { object, key: Key<'a>, … }` with `Key { raw: &'a str, area: Area, known:
  Option<PropKey> }`: `area` from `keys::area_of`, `known` from `keys::info`. A view matches
  `key.known == Some(keys::TAG)` or routes on `key.area`; it never compares `raw`.
- `Area`, `AreaSet` come from `keys.rs` (§2.1). 064's test C3 (`areas_cover_the_keys`) walks
  `keys::KEYS`, `keys::FAMILIES` and the defaults registry; C1 (`every_op_has_an_effect`) gains the
  `SetDeviceKind` sample.
- `Effect::Rep` carries `Representation`, whose `role` is now `Role` (§4.4); views that used
  `rep_type` read `role` directly.
- 064 §13 item 3 (`SetStyle.catalogue`) is done here; item 4 (`SetProperty.key`) stays a string.

---

## 9. Step order

Every step builds, passes clippy and the tests of the crates it touches, and keeps each guard at
or under its count. The overseer may merge the lane into `refactor/0.2.3` after step 5 if R9 is
ready to start, since R9 needs `keys.rs`, `Area` and `StyleCatalogue`; R9 then also waits for step
7's `SetDeviceKind` before its `Effects::of` is complete, or adds the arm itself when it merges.

0. **Preconditions.** 070 has landed (typed params everywhere) and 073's renames have landed.
   Branch `feature/types` from `refactor/0.2.3`.
1. **Vocabulary, additive.** strum in the workspace. ecad-model `words.rs`, `units.rs`, `keys.rs`
   (`PropKey`, `Area`, `KEYS`, `FAMILIES`, `PropsExt`, the family constructors). Guards G2, G3, G8,
   G10 as ratchets at today's counts. Nothing else changes yet.
2. **Property keys, area by area** (identity and tag; book and page; connectivity and wiring;
   BOM; rating circuit keys; revisions and designations; front; text and symbol families). The
   kind-specific keys are **not** moved here; step 7 takes them out of the bag. Each commit moves
   an area's constants into `keys.rs`, puts its readers on `PropsExt` / `words`, and lowers G1–G3.
   Ends with G1 (for the generic keys), G2, G3 at 0.
3. **Defaults.** `Entry.key: PropKey`; `Kind::Flag`; `Choice<E>` with the §4.8 enums; the TOML
   regenerated with `yes`/`no`; the Standards screen's switches. G8 at 0.
4. **Units.** `rating/parse.rs` typed; then `Placement`, `TextSlot`, `Primitive::Text` →
   `Radians`; 070's params → `Degrees`, `Mm`, `Metres`; `Value::Length` → `Mm`; `Grid`/`Place` →
   `PageColumn`/`PageRow`/`PageNo`. G7 at 0.
5. **Small enums and the open-time check.** `StyleCatalogue` (F1, F2), `Side` into the model,
   `TextKind` and `Shown`; `words_check` and `ReadOnly::UnknownWords` on the open path (F11).
6. **Starters and roles.** `StarterKey` (stays in electrical) with `STARTERS` keyed by it; the 11
   match blocks exhaustive (G5); `SymbolKey`, `SymbolName`, `cable_prefix` (F5); `PartRole` and
   `Role` (F3′). G4 (except `DeviceKind`), G9 at 0.
7. **Device kinds.** (a) `kind.rs`, `Component.kind` and `Definition.kind` (all `General` at
   first), `SetDeviceKind` with its contract tests, starters declaring their kinds, placement
   setting the kind (F8, F9). (b) One kind per commit, in this order: Junction and Continuation,
   LocationBox, Item, Relay, Protective, Connector, TerminalStrip, PlcCard: its fields move out of
   the bag into the variant (§5.3), its readers and commands follow, its keys join `RESERVED`, its
   predicate goes, fixtures regenerated (F10). Before the first, the oracle test A9 records today's
   predicate answers on the test projects and the medium bench project; each commit keeps A9
   green until its predicate is gone. (c) G6 and G4 (all words) at 0.
8. **Close.** Every guard is a ban with no allowlist entry; `docs/quick-start/072-types.md`;
   any remaining fixtures regenerated; the bench rows of §10 (medium while building, large once
   now).

---

## 10. Acceptance (tester required: op and format changes)

| # | Test | Checks |
| --- | --- | --- |
| A1 | `one_boolean_rule` | `words::flag` on a table: `yes`, `YES`, ` yes `, `true`, `1`, `on` → true; `no`, `false`, `0`, `off` → false; `y`, `ja`, `` → None. Every flag the app writes is `yes`/`no`. |
| A2 | `whole_numbers` | `uint` / `uint_canonical` at 0, max, max+1, leading zeros, signs, spaces, 20 digits. |
| A3 | `keys_are_registered` | `KEYS` unique; family prefixes unique and not prefixes of each other; `RESERVED` disjoint from `KEYS`; every `KEYS` entry and defaults entry has `area_of` ≠ `Free`; each family's `build` then `parse` gives the parts back. |
| A4 | `words_round_trip` | For every `Word` enum and `DeviceKind`: `parse(word(x)) == x`; serde writes `word()`; the JSON Schema's enum equals `WORDS`. |
| A5 | `unknown_words_open_read_only` | Files with (a) an unknown catalogue, role, kind or kind field in a record; (b) an unknown choice word, a `maybe` flag, a reserved key on a component, a tag prefix for no symbol. Each opens read-only with a message naming the word, the setting and where it is; saving is refused; the file's bytes are unchanged after open and close; in (a) the record is reported as damage. |
| A6 | `commands_refuse_unknown_words` | `styles {catalogue: "nope"}`, connector `halves: "x"`, `place {symbol: "nope"}` refused with the uniform input error naming the allowed words. Through `ecad-mcp`'s `execute` too (brief 011 rule): the transaction and `Author::Agent(..)` for one accepted call per changed command family, and refused with writes off. |
| A7 | `degrees_in_radians_stored` | `place {rotation: 90}` stores `Placement.rotation` = π/2; symbol content `slots[].rotation: 90` stores π/2. A file's stored angles read and save byte-equal. |
| A8 | `ratings_typed` | `rating/parse.rs` table tests return the units; the 057 ratings tests are unchanged in outcome. |
| A9 | `kinds_match_the_old_predicates` | (step 7, while the predicates still exist) for one device per starter, and every device of the test projects and the medium bench project: each kind agrees with the old predicates; where the two fuse predicates disagree, the test records which one the owner's answer (§14 call 1) follows. |
| A10 | `kind_comes_from_the_symbol` | Placing each starter as a new device gives its declared kind; adding a contact to a relay, a channel to a card, a level to a strip leaves the kind; `plccardtype` and a symbol swap re-read the kind in one transaction and keep the shared fields; undo and redo restore it. |
| A11 | `set_device_kind_op` | `SetDeviceKind` is in `Op::ALL`; inverse, atomic-on-failure and invariant contracts (a card's rack must be a rack card); `setprop {key: "plc.slot"}` records `SetDeviceKind`, also through MCP with `Author::Agent(..)`. |
| A12 | `format_words` | A project saved by this build has the §13 shapes: `yes`/`no` switches, part-role words, `cable_prefix`, `kind` on devices and definitions, no moved keys in any property bag. The inspector's Raw and Readable views show them (`.ecad` never a black box). Fixtures regenerated, not shimmed. |
| A13 | guards | `cargo xtask layers` passes with no allowlist entries for G2, G3, G6–G10; the G1/G4 tests pass at 0; clippy passes with G5's denies. |
| A14 | existing suites | fast tests of ecad-model, ecad-electrical, Engine, ecad-app, ecad-mcp, ecad-file unchanged in outcome except where a test asserted a changed word or a moved key. |
| A15 | `inspector_shows_kinds_and_words` | (hard rule, F11's condition) The inspector's Raw and Readable views show: a `SetDeviceKind` op with its before and after kinds; `Component.kind` and `Definition.kind` with every variant's fields (one device per variant); the part-role words; and, for a project opened read-only for an unknown word, the record or value with that word and the open report's reason. Neither view fails or hides a record because of a kind or an unknown word. |

**Bench** (medium while building, large once at the end): `views.setprop_tag` and
`views.move_symbol` no worse than `refactor/0.2.3` + 10%; `plc.slot_edit` (a slot change through
`SetDeviceKind`) no worse than today's `setprop` of `plc.slot` + 10%; project open (with the word
check) no worse than + 5%.

**Quick start.** `docs/quick-start/072-types.md`: what a person can notice. The Standards screen's
on/off settings are switches; where the project's tag prefixes are edited (`sheets_ui.rs`), the
cable prefix is its own line; the symbol builder asks what kind of device a new symbol makes; a
file with a word this build doesn't know opens read-only and says which word; nothing else looks
different.

---

## 11. Update maps

No new derived view. The kind is stored data, so there is no class index and no update map for it.
Views that read moved keys change their maps in the same commit as the key moves: their
`SetProperty(component, <moved key>)` rows become `SetDeviceKind(component)` rows with the same
reaction (045's PLC table, 044's connector rows, 036/057's strip and fuse rows, 049's continuation
rows). Every other row is unchanged.

---

## 12. Out of scope

- `Pt` and drawing-unit lengths; the 80 `(f64, f64)` pairs and 11 `[f64; 2]`; `history.rs`'s own
  `Point` (a later tidy-up, or R10).
- `Op::SetProperty.key`, and what a person may type as a key.
- Kinds for cables, wires, pages or folders.
- An import step for files from other tools (later, with the v1 format freeze).
- The check-kinds table (R7) and the views pipeline (R9).
- Free-text values a person types (`"10 A"`, `"18 AWG"`, a colour, a rating): they stay text.
- CADCraft: no file outside `ecad/` changes.

---

## 13. Format and op changes (for the overseer)

Pre-v1, no compatibility path: fixtures are regenerated and no shims are added (ruling). No format
version change.

**Approved (overseer, 2026-11-16): F1, F2, F4, F5, F6, F7 as written; then F3′ and F8–F12 as
written**, including the new op `SetDeviceKind` (a kind change and its variant edits need their
own undoable record). F3 was first approved as `symbol.<key>` roles; the owner's answer replaced
it with F3′. Condition on F11 (hard rule): the inspector's Raw and Readable views keep working for
the new op and fields, including on a project opened read-only for an unknown word (A15).

| # | Change | On disk | State |
| --- | --- | --- | --- |
| F1 | `Op::SetStyle.catalogue`: `String` → `StyleCatalogue` | same words (`text`, `location`, `frame`, `wire`, `page`, `cable`, `ratingtable`); an unknown one is now a damaged record | approved |
| F2 | `StyleCatalogues` keyed by `StyleCatalogue` | same words | approved |
| F3′ | `Representation.role`: `String` → `Role`; a symbol's role is its **part in the device** | **symbol roles change**: `coil`, `no`, a project symbol's key → `head`, `contact`, `pole`, `terminal`, `channel`, `half`, `pin`, `other`; the fixed words stay | approved |
| F4 | On/off switches → `yes` / `no`; text show → `yes` / `no` / `first` | words change in the defaults TOML, in project, page and component properties, and in style fields: `front.cover.{notes,wire_colours,symbol_legend,revisions,structure_codes,customer_site}`, `text.{point_label,description,part_number,continuation_name,reference,continuation_caption,location,wire_label}.show`, the 5 plc switches (`ON_OFF` in `defaults/plc.rs`), `wire.join`, `bom.column.article`, frame fields `grid_labels`, `centre_marks`, page fields `zones`, `checks`; `text.tag.show`, `.strip`, `.plc` and per-object `text.<rep>.<kind>.show` | approved |
| F5 | `ProjectSettings.cable_prefix: Option<String>`; `tag_prefixes` keyed by `SymbolName`, no `"cable"` entry | new settings field | approved |
| F6 | Symbol content command: `slots[].rotation` in degrees | command input only; stored stays radians | approved |
| F7 | `Placement.rotation`, `TextSlot.rotation`, `Primitive::Text.rotation` typed `Radians` | bytes unchanged (`serde(transparent)`) | approved |
| F8 | `Definition.kind: DeviceKind`: the kind a symbol declares, with its default data | **new field** on every definition record (starters' embedded copies and project symbols) | approved |
| F9 | `Component.kind: DeviceKind` with its variant data (§5.1, §5.3) | **new field** on every component record; `{ "kind": "<word>", …fields }`, default fields left out | approved |
| F10 | The kind-specific keys of §5.3 leave the property bag and are `RESERVED` on components | **keys removed**: `plc.rack`, `plc.slot`, `plc.channels`, `plc.addr.*`, `plc.kind`, `plc.ch.<i>.*`, `terminals`, `part_number.pe`, `maker.pe`, `term_label.*`, `acc.*`, `marker.per_card`, `jumper.bar`, `pins`, `pin_names`, `half.*`, `ignore.*`, `rating` and `rating.<n>` on devices, `contacts`, `mirror`, `kind`, `name` and `caption` on components, `rating.temperature` on boxes, `qty`, `unit` on items | approved |
| F11 | Unknown words are a read error: a damaged record for typed fields, `ReadOnly::UnknownWords` for words in values (§4.1) | no change to the bytes; a new read-only reason | approved |
| F12 | **New op `Op::SetDeviceKind { id, before, after }`** (§5.5): records a kind change and every variant-field edit | new op in the log | approved |

---

## 14. Owner decisions

### Answered (owner, 2026-11-16)

- **Unknown words** in a file are a read error: the project opens read-only with a message naming
  the word, the setting and where it is, and the file is never changed. No keep-and-warn. Forward
  compatibility waits for the v1 format freeze; files from other tools come in through a later
  import step.
- **Device kinds are data**, declared by the symbol and saved on the device, with kind-only fields
  in the kind. Not derived, not cached; the built-in symbol list stays out of the core.

### Going ahead unless you object

This follows your rulings: one yes/no rule for every switch (the about 25 on/off settings become
switches, saved as yes/no, and "y" stops counting as yes); strict words everywhere, with a file
holding one this build doesn't know opening read-only; no compatibility code; each value defined
once. Property keys get one list in the core with typed readers; lengths, angles, currents,
voltages, cross-sections, temperatures and percentages get their own types where they are parsed;
the text a person types (like "10 A") stays as typed. Every command takes angles in degrees. The
cable tag prefix becomes its own setting. A symbol on a page records its part in the device (head,
contact, terminal…) instead of the symbol's name. Each device records its kind, taken from the
symbol that created it; a card's rack and slot, a strip's accessories, a connector's pins and a
fuse's rating move off the free list of properties into that kind, so a terminal strip can't have a
PLC slot. Changing the kind is one edit (swapping the symbol does it), and `setprop` on those
settings still works. The symbol builder asks which kind of device a new symbol makes. Every guard
ends at zero. We are **not** giving cables or wires a kind, not changing how points are stored,
and not touching the check list (R7) or the views (R9) beyond what they read.

### Needs your call

Where it is unclear which kind a device is, or whether a field belongs to a kind:

1. **Fuses, breakers and fuse terminals.** Today a fuse, a breaker, an RCD, a fuse bank and the
   stand-alone fuse terminal all keep a free-text rating (`13A C 30mA`), and a fuse bank keeps one
   per output. (a) One **Protective** kind for all of them, holding the rating and the per-output
   ratings; the stand-alone fuse terminal is Protective too, while a fuse *position* in a terminal
   strip stays part of the strip. (b) Separate Fuse, Breaker, RCD and Fuse bank kinds, each with
   the same rating field. *Recommended: (a)*, because the ratings checks treat them alike; a later
   rule that only breakers have can split it then.
2. **PLC rack versus card.** Today a rack is just the CPU or head-module card that other cards
   point at. (a) One **PLC card** kind with its card type; a rack is a card whose type is CPU or
   head module. (b) A separate **PLC rack** kind. *Recommended: (a)*: changing a CPU to a head
   module stays one edit, and a card's "rack" can only point at a card of a rack type.
3. **Junctions, continuations, location boxes and BOM-only items.** Today they are stored with the
   devices but are not devices in your sense (no part on a page, no wiring role, or no symbol at
   all). (a) Keep them where they are, as kinds of their own (Junction, Continuation, Location
   box, Item). (b) Make each its own top-level object like cables. *Recommended: (a) now*, since
   (b) touches every place that walks devices and their symbols; (b) can follow as its own lane if
   you want them out of the device lists' code.
4. **Relays and contact tables.** A coil device keeps what contacts its part offers
   (`3P+1NO+1NC`) and where its contact table goes. (a) A **Relay** kind for coil devices
   (contactors and relays) holding both. (b) Keep both on every device, since a push button or a
   breaker with auxiliary contacts may want a table later. *Recommended: (a)*; the worker first
   checks that no non-coil device stores these today.
5. **Kinds with no data yet** (motor, lamp, sensor, valve…). (a) Add a kind only when it has its
   own fields or rules; these stay **General** (the symbol library's families still group them
   for finding). (b) Add them now as empty kinds. *Recommended: (a)*: an empty kind is a word to
   keep in step with nothing behind it.
