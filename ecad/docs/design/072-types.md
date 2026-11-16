# Design 072: The typed object model

Status: **Draft** (designer, 2026-11-16; widened the same day by the owner from "types for keys,
units and device kinds" to the typed object model for all of ECAD). The spec below is the owner's
and is decided; this design turns it into types, lanes and steps. Waiting for: the owner's calls in
§16 and the overseer's batch approval of the format and op changes F13–F30 (§10). F1–F12 approved
2026-11-16 (F12 is superseded by F13, §10). Brief: `docs/briefs/072-types.md`.

**Goal (one sentence).** What every stored object is, and how it relates to other objects, is held
in its own typed struct, never worked out from names, symbols, strings or positions, so the
compiler finds every place a new object, field, kind or unit must be handled.

Sources: code review 2026-11 (`C:\dev\ecad-features\_tmp\code-review-2026-11.md`), findings 4, 6,
7 and 10 (lanes R4, R5, R8); the owner's answers and spec of 2026-11-16.

**Names.** "Device" is the person's word (052) for what the code calls `Component`; this design
proposes renaming the struct to `Device` and `Conductor` to `Wire` (F14, F20). That changes a
recorded decision (owner, 2026-11-08: code, op and command names stay until the next major format
version), so it is listed for the overseer; if it is refused, read `Device` as `Component` and
`Wire` as `Conductor` throughout, and nothing else changes. `Terminal` keeps its name either way. `ecad-cadcraft` becomes `ecad-engine` in lane 073, right after 070: read "Engine" as
that crate. Paths below are today's.

---

## Part I. Principles and rulings (decided)

**The owner's principles (2026-11-16), for every object type.**
1. **Data, not inference.** What something is, and how it relates to other things, is stored in
   its type. No predicate walks to a symbol, no name is matched, no cache keeps a guess.
2. **One struct per object type** with its own fields. Kind-specific data lives in enum variants,
   so a wrong combination can't be built.
3. **Shared behaviour is traits**, implemented by the types. Each type owns its calculations as
   methods (`psu.available_power()`, a protective device's trip check); checks, power generation
   and reports call them and never recalculate.
4. **Relationships are stored explicitly** as IDs, at the level they belong.
5. **Strict words.** An unknown word in a file is a read error and the project opens read-only
   with a message naming the word, the setting and where it is (the damaged-records path); the
   file is never changed. No `Unknown(String)`, no `Stored<E>`. Forward compatibility waits for
   the v1 format freeze (`docs/deferred-work.md`); other tools' files come in through a later
   explicit import step.
6. **No compatibility code.** Test files and fixtures are regenerated; no shims, no converting
   reader.

**Earlier rulings that stand.** strum (design 061) for `word()`/`parse()`. Booleans written
`yes`/`no`, read also as `true`/`1`/`on` (`false`/`0`/`off`), one rule in one place. One source
per value, shared vocabulary in ecad-model. Every guard ends the lane **at 0** as a plain ban. The
built-in symbol name list stays out of the core. A cable is not a device.

**Where the types live.** Every stored struct and enum is in **ecad-model** (it is what the file
holds). Calculations that need the electrical rules or tables (rating datasets, trip curves) are
trait methods defined and implemented in **ecad-electrical** on the model types (a local trait on a
foreign type is allowed), so `use ecad_electrical::Supply; psu.available_power(..)` reads as the
type's own method while Core stays layered. Pure accessors (a PSU's rated output) are inherent
methods in ecad-model.

---

## 0. What the code says (recounted at `d323677a`)

Counts are for product code (no `tests/`, no `#[cfg(test)]`), from grep and small scripts: ±10%.

### 0.1 The store

`Document` (`ecad-model/src/doc.rs:576`) holds: `pages`, `page_order`, `layers`, `items`
(drawing primitives and symbol instances), `components` (devices **and** junctions,
continuations, location boxes, BOM-only items), `terminals` (connection points), `representations`,
`conductors` (wires), `cables` (+ `CableMember`), `definitions` (symbols), `settings`
(`ProjectSettings`), `versions`, `properties` (the project's own), `folders`, `folder_order`,
`standard`, `styles` (style records). Derived and never stored: the reverse `index` and the nets
(`connectivity`).

Five of these are a typed shell around a **property bag** (`BTreeMap<String, String>`): `Page`
(title + bag), `Component` (bag + definition), `Conductor` (bag + ends), `Cable` (bag + members),
and the project (`Document.properties`). Style records are a second bag per catalogue
(`StyleRecord.fields`).

### 0.2 Strings and inference (the review's findings, recounted)

| Finding | Now |
| --- | --- |
| Property-key string constants | **~105** in 27 files; 42 literal `"tag"` keys; `description` 37, `part_number` 20, `maker` 12, `type` 12 literal uses |
| Keys built with `format!` / split by `starts_with`, `split_once` | **37** / **151** sites |
| Boolean readers | 3 word sets at ~13 sites |
| Digit checks | ~30 sites; 6 copies of one bounded-number reader |
| `Kind::Choice` lists | 56 uses, 31 literal lists, 25 of them on/off |
| String-literal `match` arms | **595** |
| Starter symbols / `match key` blocks | 113 / 11 (biggest 46 and 55 arms) |
| Style catalogues | 7, as `String` in 23 signatures |
| `Representation.role` | free text: 8 fixed words plus a symbol's starter or project key |
| Device predicates | **10**, 151 calls in ~45 files |
| Angles | stored radians, commands degrees, except symbol content `slots[].rotation` (radians) |
| Ratings parsers | 8 return bare `f64` |
| Page places | `Place { page: String, column: u32, row: u32 }` |

### 0.3 What is worked out today instead of stored

| # | Inferred today | From | Becomes |
| --- | --- | --- | --- |
| I1 | What a device is (strip, connector, rack, fuse terminal, item…) | ten predicates walking device → definition → starter key, or the `kind` property word | `Device.device_type` (§2) |
| I2 | A PLC card's type and whether it is a rack | its head symbol's starter key (`plc::rack::body_type`, `CardType::of_body`) | `PlcModule.function`, `PlcRack` (§3) |
| I3 | A connection point's side, channel, level and position | the terminal key text: `.s`/`.p`, `.<i>.<role>`, `<n>.<level>`, `.ni`/`.no` (`connectors.rs`, `plc/channels.rs:52`, `links.rs`) | `PointKey` enum on `Terminal` (§2.6) |
| I4 | A symbol's part in its device (head, contact, pole…) | `reps::rep_type` from the definition each time | `PartRole` on the representation (F3′) |
| I5 | A fuse bank's outputs | the digit-only labels of its starter (`symbols.rs:829`) | `Protection.outputs` (§2.3) |
| I6 | A protective device's rated current and curve | parsed from the free `rating` text (`rating/parse.rs::amps`) | typed `Protection` fields, the text kept only as what a person typed (§2.3) |
| I7 | A contact arrangement (3P, change-over) | `match` on starter keys (`symbols.rs:1377,1589`) | `Switching.contacts` (§2.4) |
| I8 | Junctions | a component with `kind = junction` | not stored: drawn where wires share a net (§6.1) |
| I9 | A continuation and its name | `kind = continuation` + `name` properties | `Aid::Continuation` (§6.2) |
| I10 | A device's location and function from boxes | proposed from box geometry when asked (`locations::context_in`) | stored on the device, set by the box on an explicit edit (§6.3) |
| I11 | What a page is | `generated` word + `strip` hex id + `page.type` properties | `Page.kind: PageKind` (§5.1) |
| I12 | A revision row and its columns | `rev.<n>.<field>` and `revisions.columns` text | `Project.revisions` (§5.3) |
| I13 | Function and location lists | `designation.<aspect>.<entry>.{code,description}` | `Project.designations` (§5.3) |
| I14 | A strip's jumpers | `link.<n>` range text, parsed on every read | `TerminalStrip.links` (§2.5) |
| I15 | A cable's ends, glands, rating inputs | `end.from.location`, `gland.to`, `rating.*` … | `Cable` fields (§4.1) |
| I16 | Part lines | `part.<n>.<field>` | `Device.parts: Vec<PartLine>` (§2.1) |
| I17 | Which device text a symbol slot places | `TextSlot.text` words `tag`, `rating.<label>` … | `SlotText` enum (§7.1) |
| I18 | Title block bindings and unknown items | `Binding` text, `BlockItem::Unknown`, `StyleBody::Raw` kept as read | strict typed elements (§5.4) |
| I19 | Style record fields | `StyleRecord.fields: BTreeMap<String, String>` per catalogue | one struct per catalogue (§8, lane G) |

---

## 1. The object map

Every stored object type, what it becomes, and its lane (§12). "Bag" = what leaves the property bag.

| Object | Struct (ecad-model) | Enums with data | Traits | Stored relationships | Bag → fields | Lane |
| --- | --- | --- | --- | --- | --- | --- |
| **Device** (`Component`) | `Device` (§2.1) | `DeviceType` (§2.2) | `Supply`, `Load`, `Protects`, `Switching`, `Connects`, `Addressed` (§2.4) | its symbols (representations, `Subject::Device`); its connection points (`Terminal.owner`); `PlcModule.rack`; `DeviceLink` siblings (§2.7) | common fields, part lines, user fields, all kind-only keys | B, C |
| **PLC rack / module / channel** | `PlcRack`, `PlcModule`, `ChannelGroup`, `Channel` (§3) | `ModuleFunction`, `ChannelFunction` | `Addressed`, `Load` | module → rack (`rack: DeviceId`), channel → module (inside it), channel → connection points (`PointKey::Channel`) | `plc.*` | C |
| **Wire** (`Conductor`) | `Wire` (§4.2) | `WireSize`, `WireColour` | `Circuit` | ends → connection points or another wire (§6.1); cable membership | `wire_number*`, `potential`, `cross_section`, `colour`, `wire.category`, `load`, `protected_by`, `rating.*` | D |
| **Net** | derived (`connectivity`), not stored | — | — | from wires and connection points | — | D |
| **Cable** | `Cable` (§4.1) | `Shield`, `CableEnd` | `Circuit` | cores → wires (`Core.wire`), ends → location/function entries | every `CableField`, `cores`, `unused.*`, `rating.*`, `load`, `protected_by` | D |
| **Page** | `Page` (§5.1) | `PageKind` | — | folder, page type, frame and title block styles | `number`, `folder`, `function`, `location`, `sheet`, `title_block`, `page.type`, `generated`, `strip`, `print.frame.style`, `description` | E |
| **Folder** | `Folder` (unchanged) | — | — | parent | `folder.<id>.start` (on the project) → `Folder.start` | E |
| **Project** (`Document.properties`) | `Project` (§5.3) | `RevisionColumn` | — | — | identity, revisions, designations, user field definitions, loose parts | E |
| **Title block** | per 055c (§5.4) | `Element`, `Binding` | — | inside a frame style | strictness only | E |
| **Drawing aid** | `Aid` (§6.2) | `Aid { Continuation, Box }` | — | its drawing (representation, `Subject::Aid`); a continuation's connection point | `kind`, `name`, `caption`, `location`, `function`, `rating.temperature` | F |
| **Loose part** (BOM-only item) | per the owner's call (§6.4) | — | — | — | `kind=bom_item`, `qty`, `unit`, … | F |
| **Connection point** (`Terminal`) | `Terminal { owner, label, key: PointKey }` | `PointKey`, `TerminalOwner` | — | owner device or aid | key text → `PointKey` | B (C, F extend) |
| **Representation** | unchanged + `role: Role` | `Role`, `PartRole` | — | subject, page, items | F3′ | B |
| **Symbol** (`Definition`) | `Definition` + `device_type`, typed slots and defaults (§7.1) | `SlotText`, `SymbolDefault` | — | origin, derived_from | slot words, default keys | B |
| **Item** (drawing primitive, symbol instance) | unchanged | `Primitive` | — | page, layer, definition | none | — |
| **Layer**, **version**, **standard header** | unchanged | — | — | — | none | — |
| **Style record** | one struct per catalogue (§8) | `StyleBody` | — | catalogue, layer | `fields` map | G (owner call) |
| **Project settings** | `ProjectSettings` + `cable_prefix` (F5) | existing | — | — | `tag_prefixes` keyed by `SymbolName` | A |

---

## 2. Device

### 2.1 The struct and its common fields

```rust
pub struct Device {
    pub definition: Option<DefinitionId>,          // the symbol it was made from (as today)
    pub tag: Option<Tag>,                           // `Tag` validates the tag rules (tag.rs)
    pub tag_fixed: bool,
    pub function: Option<EntryId>,                  // an entry of Project.designations (§5.3)
    pub location: Option<EntryId>,
    pub placed_in: Option<AidId>,                   // the box that set function/location (§6.3)
    pub description: Option<String>,
    pub part_number: Option<String>,                // line 0, the main part (047 §2.2)
    pub manufacturer: Option<String>,               // today's `maker`
    pub article_number: Option<String>,
    pub type_text: Option<String>,                  // today's free `type`
    pub rated_voltage: Option<RatedVoltage>,        // Volts + Ac | Dc
    pub parts: Vec<PartLine>,                       // 047's lines 1…99 (I16)
    pub user: BTreeMap<UserFieldId, UserValue>,     // project-defined fields (§2.8)
    pub device_type: DeviceType,                    // §2.2
    pub display: Properties,                        // what stays a property (§9.2)
}
pub struct PartLine { pub n: u8, pub part_number, pub manufacturer, pub description, pub article_number: Option<String>,
                      pub qty: Qty, pub unit: ItemUnit, pub kit: Option<String>, pub serves: Vec<String> }
```

Every device has these. A field that only some types have lives in the type (§2.3). Free text a
person types stays `String`; anything ECAD calculates with is typed (`RatedVoltage`, `Qty`).

### 2.2 `DeviceType`: the groups

`DeviceType` is fixed at compile time, one variant per group, each holding its own struct. Groups
start from IEC 81346-2's top-level classes, the letters ECAD's built-in tag prefixes already use
(`defaults/symbols.toml`), and merge classes that need the same data and behaviour; a class is split
only where its data or calculations differ. **General** covers the rest until it earns a type.

| # | Group (`DeviceType::`) | IEC 81346-2 classes it covers | Starters today | Data (its struct) | Traits |
| --- | --- | --- | --- | --- | --- |
| 1 | `Psu(Psu)` | G (generating: power supplies, batteries) and T (converting: transformers) | `psu`, `trafo` | input `RatedVoltage`, output `RatedVoltage`, output current `Amps` or power `Watts`, transformer ratio when T | `Supply`, `Load` |
| 2 | `Protection(Protection)` | F (protecting: fuses, surge) and Q's protective switches (QA/QB: MCB, RCD, RCBO, fuse switch) | `fuse`, `mcb1`, `mcb2`, `rcbo2`, `rcd2`, `rcd4`, `fuseterm`, `fusebank4…12` | `device: ProtectionKind` (Fuse, Mcb, Rcd, Rcbo, FuseBank), `rated_current: Option<Amps>`, `curve: Option<Curve>`, `residual: Option<MilliAmps>`, `breaking: Option<KiloAmps>`, `poles: u8`, `outputs: Vec<Output { label, rating }>` (fuse banks), `rating_text` (what was typed) | `Protects`, `Switching` (aux contacts) |
| 3 | `Relay(Relay)` | K (relays, contactors, SSR, timers) and QA contactors | `coil`, `power3`, `power4`, `co`, `ssrin`, `ssrout` | `coil: Option<Coil { voltage: RatedVoltage, power: Option<Watts> }>`, `contacts: ContactSet` (§2.4), `contact_table: TablePosition` | `Switching`, `Load` (coil) |
| 4 | `Switch(ManualSwitch)` | S (manual control: push buttons, selectors, switch-disconnectors operated by hand) | `pbno`, `pbnc`, `sel`, `swco` | `actuator: Actuator` (Push, Selector{positions}, Key, Mushroom…), `contacts: ContactSet` | `Switching` |
| 5 | `Sensor(Sensor)` | B (converting a variable to a signal) | `prox3`, `floatno`, `floatnc`, `rollerno`, `rollernc`, `interlockno`, `interlocknc`, `tc` | `sensing: Sensing` (Proximity, Float, Limit, Interlock, Thermocouple…), `output: SensorOutput` (contact, PNP, NPN, analogue range), supply `RatedVoltage` | `Switching` (contact sensors), `Load` |
| 6 | `Motor(Motor)` | M (providing mechanical energy: motors) | `motor1`, `motor3` | `phases`, `rated_power: Watts`, `rated_current: Option<Amps>`, `power_factor`, `starting: Starting` | `Load` (`full_load_current()`) |
| 7 | `Solenoid(Solenoid)` | MB (valves, solenoids, brakes: M sub-classes with a coil, not a shaft) | `valve`, `valve2` | `coil: Coil`, `positions: u8` | `Load` |
| 8 | `Indicator(Indicator)` | P and H (presenting information: lamps, beacons, meters) | `lamp` | `colour: Option<IecColour>`, `power: Option<Watts>` | `Load` |
| 9 | `Heater(Heater)` | E (providing heat or light as energy: heaters, lighting) | `heater` | `power: Watts`, `thermostat: bool` | `Load` |
| 10 | `TerminalStrip(TerminalStrip)` | X terminals (connecting, DIN-rail terminals) | `terminal`, `terminalpe`, `term2…`, `term3…`, `termlnpe…`, `termsensor…`, `termdisc`, `termfuse` | §2.5 | `Connects` |
| 11 | `Connector(Connector)` | X plugs, sockets, bulkheads | `socket*`, `plug*`, `dsub*`, `m12*`, `rj45`, `usba`, `usbb`, `pinpair`, `pins`, `pinp` | `family: ConnectorFamily`, `pins: u16`, `pin_names: Vec<String>`, `halves: BTreeMap<Side, HalfPart>`, `mating: bool` (a bulkhead is not), `ignored: BTreeSet<ConnectorCheck>` | `Connects` |
| 12 | `PlcRack(PlcRack)` | A/K: a PLC node (station) | (new: today the CPU card is the rack) | §3 | `Addressed` |
| 13 | `PlcModule(PlcModule)` | A/K: a module in a rack (CPU, I/O, power, communication) | `plccpu`, `plcps`, `plcdi`, `plcdq`, `plcai`, `plcao`, `plcrtd`, `plciol`, `plcim`, `plcpm`, `plccm`, `di`, `do` | §3 | `Addressed`, `Load` |
| 14 | `General(General)` | everything else (R, C, U, V, W-as-device, A not PLC…) | project symbols with no type, other converts | nothing kind-specific | — |

Notes.
- **QA and QB become Protection** (owner): fuses, MCBs, RCDs and fuse terminal devices share the
  rating, curve and trip data. A fuse *position* inside a terminal strip (`termfuse`) stays part of
  the strip (§2.5) with its own rating there.
- **Contactors are Relay**, not Protection: they switch but don't protect (the IEC 2019 letter QA
  covers both; ECAD splits by data, as the owner asked).
- **Catalogue products** (an IVP4 versus an IVP6) are part-library data, never variants.
- Fourteen groups; a new group is added only when a symbol needs data or a calculation that no
  group has.

### 2.3 Where a device's type comes from

- **The symbol declares it.** `Definition.device_type` (F8, renamed from `kind`) holds the type
  with its default data (`Protection { device: Mcb, curve: Some(C), .. }`). A starter's comes from
  its `STARTERS` row; a project symbol's (048) is chosen in the symbol builder, defaulting to its
  `derived_from` starter's, else `General`.
- **Placing a symbol as a new device** creates the device with that type; only values are filled in
  at runtime.
- **Adding a symbol to an existing device** (a contact to a relay, a channel to a module, a level
  to a strip) never changes the type. Whether it may join stays the join rule (`symbols::Kind`,
  renamed `JoinRule`).
- **Changing the type** is one explicit edit; swapping the symbol re-reads the type from the new
  symbol, keeping the fields both variants share. `setprop` on a key that moved (`plc.slot`,
  `rating`) keeps working for people and agents through 070's `PROP_RULES` (§13).

### 2.4 Traits and methods

```rust
// ecad-electrical/src/behaviour.rs (definitions); implemented in the group's module
pub trait Supply   { fn output(&self) -> RatedVoltage; fn available_power(&self) -> Option<Watts>; fn available_current(&self) -> Option<Amps>; }
pub trait Load     { fn demand(&self, at: RatedVoltage) -> Option<Demand>; }          // Demand { current: Amps, power: Watts }
pub trait Protects { fn rated_current(&self) -> Option<Amps>; fn trips(&self, current: Amps, for_s: Seconds) -> Trip;
                     fn protects(&self, circuit: &dyn Circuit) -> Verdict; }           // the 057 coordination check
pub trait Switching { fn contacts(&self) -> &ContactSet; fn contact_table(&self) -> TablePosition; }
pub trait Connects  { fn points(&self, d: &Device) -> Vec<PointKey>; }
pub trait Addressed { fn address_block(&self, doc: &Document) -> Option<AddressBlock>; }
pub trait Circuit   { fn load(&self) -> Option<Amps>; fn protected_by(&self) -> Option<DeviceId>;
                      fn rating(&self, doc: &Document) -> Rating; fn volt_drop(&self, doc: &Document) -> Option<Percent>; }
```

- `ContactSet` replaces I7: a list of `Contact { kind: Make | Break | ChangeOver | Power, label }`,
  declared by the symbol and extended by the offer text a person types (`3P+1NO+1NC`, today's
  `contacts` property), which is parsed once on input into the set.
- A type that does two things implements both (`Psu: Supply + Load`, `Relay: Switching + Load`).
- `DeviceType` offers `as_supply()`, `as_load()`, `as_protects()`, `as_switching()` (each an
  exhaustive `match`, no `_`), so a check asks `device.device_type.as_protects()` and never names a
  group it doesn't care about.
- Checks, the power summary and reports call these methods; the rating code in
  `ecad-electrical/src/rating/` becomes their implementation, not a parallel calculation.

### 2.5 Terminal strips

```rust
pub struct TerminalStrip {
    pub offer: Option<String>,                       // positions offered (FP1), as typed
    pub pe_part: Option<Part>,                       // part_number.pe, maker.pe
    pub levels: BTreeMap<(Position, Level), LevelData { label: Option<String>, fuse_rating: Option<String> }>,
    pub links: Vec<Jumper { from: PointRef, to: PointRef }>,   // today's `link.<n>` ranges, expanded once on input (I14)
    pub accessories: Accessories { start, end, after: BTreeMap<Position, String>, on: BTreeMap<Position, String>,
                                   marker_per_card: Option<u32>, jumper_bar: Option<u32> },
}
```

Parent/child: the strip is the parent; its terminals are its connection points
(`Terminal.owner = strip`, `PointKey::Level { position, level, side }`). Device-level data (tag,
part set) lives on the strip.

### 2.6 Connection points: `PointKey` (I3)

```rust
pub enum TerminalOwner { Device(DeviceId), Aid(AidId) }          // F moves continuations out (§6)
pub enum PointKey {
    Def(String),                                                 // a definition terminal's key (`A1`, `13`)
    Side(Side),                                                  // a connector pin's half
    Channel { index: u16, role: ChannelRole },                   // a PLC channel point
    Level { position: String, level: u8, io: Option<InOut> },    // a strip position level
}
```

Today's key text (`.s`, `.3.in`, `4.2`, `7.1o`) is parsed in five places; it becomes data.

### 2.7 Relationships

- **Parent → children, stored on the child or inside the parent:** device → its symbols
  (representation `subject`; coil → contacts are one device's representations with `PartRole`
  Head and Contact, so the contact table lives on the device); strip → terminals
  (`Terminal.owner`); rack → modules (`PlcModule.rack`); module → channels (inside the module, §3).
- **Typed sibling links**, stored once at the project level because they belong to the pair:
  `Document.device_links: BTreeMap<LinkId, DeviceLink { kind: LinkKind, a: DeviceId, b: DeviceId }>`,
  `LinkKind { MechanicalInterlock, AuxiliaryBlock, … }`. Today the interlock starters
  (`interlockno`/`interlocknc`) only draw; the link is new data (F22).
- **Schematic versus panel** is one device with two representations (symbol and, from 0.2.4,
  footprint): no relationship object.

### 2.8 Project-defined user fields

`Project.user_fields: Vec<UserFieldDef { id: UserFieldId, name: String, kind: UserFieldKind }>`,
`UserFieldKind { Text { max }, Number { unit: Option<Unit> }, Flag, Choice(Vec<String>) }`, like
the custom revision columns. A device stores values by id; a value of the wrong kind, or an id the
project doesn't define, is a read error. Today free keys on devices exist only by hand: they stay
in `display` until a project defines the field (§9.2).

---

## 3. PLC: three levels, all stored

```rust
pub struct PlcRack {                    // a node: the rack or station, its own device with its own tag
    pub network: Option<NetworkId>,     // name or address on the network (the owner's "network identity")
    pub address: Option<String>,        // IP or bus address, as typed
    pub address_style: AddressStyle,    // byte.bit, iec, percent, slot (today a project default)
}
pub struct PlcModule {
    pub rack: Option<DeviceId>,         // parent (today `plc.rack`)
    pub slot: Option<u16>,              // today `plc.slot`
    pub function: ModuleFunction,       // Cpu | Psu | Comm | Accessory | Io(Vec<ChannelGroup>)  (mixed = several groups)
    pub address: AddressPin { start: Option<String>, size: Option<u32> },   // plc.addr.*
    pub supply: Option<RatedVoltage>,   // today `plc.kind` words (`24 V`, `relay`) split into supply and output kind
    pub accessories: Vec<Part>,
}
pub struct ChannelGroup { pub function: ChannelFunction, pub count: u16, pub channels: BTreeMap<u16, Channel> }
pub enum ChannelFunction { Di, Dq, Ai, Aq, Rtd, IoLink, Supply24V, Ethernet, … }
pub struct Channel { pub text: Option<String>, pub address: Option<String>, pub tag: Option<String>, pub safe: Option<SafeState> }
```

- The 056 card fields move: rack, slot, ID (the tag), description, manufacturer, part number,
  accessories. ID, description, manufacturer and part number are the device's common fields; rack,
  slot and accessories are the module's.
- **Today's rack is the CPU or head module.** Each becomes a `PlcRack` device (its tag is today's
  card tag) plus a `PlcModule { function: Cpu }` in slot 0 or 1, both regenerated in fixtures.
  `plc::rack::is_rack`, `body_type` and `CardType::of_body` go.
- The address, the I/O list and the rack overview are methods: `PlcModule::address_block`,
  `PlcRack::modules(doc)` (through the index's `rack → modules` map), never a scan.
- A channel symbol is a representation of its module with `PartRole::Channel` and its points keyed
  `PointKey::Channel { index, role }`.

---

## 4. Cable, wire and net

### 4.1 Cable

```rust
pub struct Cable {
    pub tag: Option<Tag>, pub tag_fixed: bool,
    pub cable_type: Option<StyleRef>,              // a `cable` style (059)
    pub type_text: Option<String>, pub manufacturer, pub part_number, pub description: Option<String>,
    pub cores: Vec<Core { label: String, colour: Option<WireColour>, wire: Option<WireId>, unused: bool }>,
    pub shield: Shield,                            // None | Overall | Pairs | OverallPairs
    pub length: Option<Metres>,
    pub ends: [CableEnd { function: Option<EntryId>, location: Option<EntryId>, gland: Option<String> }; 2],
    pub rating: CircuitInputs,                     // basis, manual, method, ambient, grouped, voltage, system, vd_limit
    pub load: Option<Amps>, pub protected_by: Option<DeviceId>,
}
```

`members: Vec<CableMember>` folds into `cores` (a core names its wire). Methods (`Circuit`):
`cable.rating(doc)`, `cable.volt_drop(doc)`; the rating checks and the cable schedule call them.

### 4.2 Wire

```rust
pub struct Wire {                                   // today `Conductor`
    pub ends: [Option<Attach>; 2],                  // §6.1
    pub number: Option<WireNumber { value: String, fixed: bool }>,
    pub potential: Option<String>,
    pub size: Option<WireSize>,                     // Mm2(Mm2) | Awg(u8)
    pub colour: Option<WireColour>,                 // §16 call 5
    pub category: Option<StyleRef>,                 // a `wire` style
    pub rating: CircuitInputs, pub load: Option<Amps>, pub protected_by: Option<DeviceId>,
}
```

### 4.3 Net

A net is derived from the wires and connection points (graph structure, not inference): it is
never stored, as today (`connectivity`). Wire numbering stays typed (`WireNumber`, `Scheme`). The
junction component goes (§6.1).

---

## 5. Page, folder, project, title block

### 5.1 Page (I11)

```rust
pub struct Page {
    pub title: String,
    pub number: PageNo,
    pub folder: Option<FolderId>,
    pub kind: PageKind,
    pub page_type: Option<StyleRef>,       // a `page` style (054): sheet, frame, title block, zones presets
    pub sheet: Option<SheetSpec>, pub frame: Option<StyleRef>, pub title_block: Option<StyleRef>,
    pub function: Option<EntryId>, pub location: Option<EntryId>,
    pub description: Option<String>,
    pub display: Properties,
}
pub enum PageKind {
    Schematic,
    Panel,                                  // 0.2.4
    Generated(Generated { what: GenKind, strip: Option<DeviceId> }),   // cover, contents, revisions, BOM, terminals, cables, rack, I/O list
    StripPage { strip: DeviceId, form: StripPageForm },
    PlcDetail { module: DeviceId },
}
```

Position and numbering: the page's place in the book is `page_order` plus `folder`; its number is
`PageNo`; a point on it is `Place { page: PageNo, column: PageColumn, row: PageRow }`.

### 5.2 Folder

`Folder { name, parent, start: Option<PageNo> }` (today `folder.<id>.start` on the project).

### 5.3 Project (I12, I13)

```rust
pub struct Project {                                         // today `Document.properties`
    pub info: ProjectInfo { title, drawing_number, customer, site, status, notes… : Option<String> },
    pub revisions: Revisions { columns: Vec<RevisionColumn>, rows: Vec<Revision>, pending: RevisionMark },
    pub designations: Designations { functions: Vec<Entry>, locations: Vec<Entry> },   // Entry { id, code, description }
    pub user_fields: Vec<UserFieldDef>,
    pub loose_parts: …,                                      // §6.4, owner call
    pub display: Properties,                                 // registry project values (066) stay here
}
pub enum RevisionColumn { Rev, Date, Description, Drawn, Checked, Approved, Custom { name: String, width: Mm } }
pub struct Revision { pub values: BTreeMap<ColumnRef, String>, pub issued: Option<String>, pub released: Option<String> }
```

Device and page `function`/`location` become `EntryId`s into the designations list instead of
code text, so renaming an entry's code is one edit.

### 5.4 Title block

055c's element-built blocks are already typed (`title_block.rs`: `TitleBlockLayout`, `BlockItem`,
`Binding`, `TextLook`). What changes is strictness: `Binding` parses to an enum (`Project(Field)`,
`Page(Field)`, `Revision(ColumnRef)`, `Print(Field)`) instead of text kept as written;
`BlockItem::Unknown` and `StyleBody::Raw` go (an unknown element or body is a read error); revision
columns bind to `RevisionColumn` (§5.3). Whatever element types 055c has landed when lane E runs
are the ones typed; E adds none.

---

## 6. Not devices (lane F)

Moving these out of the device store is its own lane within the refactor, before the end gate.

### 6.1 Junctions are not stored (I8)

A junction today is a component with one connection point that wires end on. Without it, a T-joint
needs somewhere to attach. `Wire.ends: [Option<Attach>; 2]`:

```rust
pub enum Attach { Point(TerminalId), Wire { wire: WireId, at: Pt } }
```

A wire end on another wire joins its net; the junction dot is drawn where three or more wire ends
meet a wire or each other on one net (§16 call 1). `AttachEnd` carries `Attach` (F25).

### 6.2 Drawing aids (I9)

```rust
pub enum Aid {
    Continuation { name: String, caption: Option<String> },
    Box { function: Option<EntryId>, location: Option<EntryId>, panel_temperature: Option<Celsius> },
}
// Document.aids: BTreeMap<AidId, Aid>; Subject::Aid(AidId); TerminalOwner::Aid for a continuation's point
```

### 6.3 Boxes set location and function on the devices inside (I10)

- A box's location and function are **stored on each device inside it** (`Device.function`,
  `location`, `placed_in: Some(box)`), not worked out when drawn.
- Every edit that changes what is inside a box records the devices' new values in the **same
  transaction**: moving or placing a device's head symbol into or out of a box, moving, reshaping
  or deleting a box, or changing a box's location or function. The device's head symbol
  (`PartRole::Head`, else its first symbol) decides which box it is in; innermost box wins, as
  today.
- Moving out of every box sets the values the box had set back to none (`placed_in = None`), and
  leaves a value a person typed by hand (§16 call 2).
- Page and project values stay what a new device is offered, as 052 had it.

### 6.4 Parts-list-only items: what the list holds today (for the owner)

Today (brief 047 §3): an **item** is a tagless component with `kind = bom_item` and **no symbol on
any page**, counted in the BOM and nowhere else. It holds `description` (required),
`part_number`, `maker`, `article_number`, `qty` (a number ≤ 9 999, decimals only with metres),
`unit` (`pcs` or `m`), and a `location` and `function`; it can't take a tag, part lines or a
symbol. Examples from 047: trunking, DIN rail, cable ties, crimps bought loose. Accessories are
already attached to their owner (strip accessories, 030 §4; cable glands, 059); a device's extra
parts are its part lines. So the list holds only **panel items with no device to belong to**,
which the owner has set to wait for the 0.2.4 layout. §16 call 3 says where they go until then.

---

## 7. Symbol and representation

### 7.1 Symbol (`Definition`) (I17)

`Definition` gains `device_type: DeviceType` (F8) and `part_role: PartRole` (what its placements
are in a device). `TextSlot.text: String` → `SlotText { Tag, Description, Rating, RatingOf(String),
PartNumber, Manufacturer, ArticleNumber, Function, Location }`; `defaults: BTreeMap<String,
String>` keyed by `DEFINITION_DEFAULT_KEYS` → `SymbolDefaults { key, prefix, description, rating,
join, aliases, family }` as a struct; `bridges: Vec<Vec<String>>` → `Vec<Vec<PointKey>>`.

### 7.2 Representation

`role: Role` with `Role::Part(PartRole)` for a device's symbol (F3′, approved); the subject gains
`Subject::Aid(AidId)` (F) and `Subject::Device` is today's `Component` renamed.

---

## 8. Style records (found in the store, lane G)

`StyleRecord { name, fields: BTreeMap<String, String> }` is a property bag per catalogue whose
fields are described by each catalogue's `Field` table (`styles/*`). Under the principles each
catalogue gets its struct (`TextStyle { font, height: Mm, weight: Weight, … }`, `WireStyle`,
`CableType`, `PageType`, `FrameStyle`, `LocationStyle`, `RatingTable`) in a `StyleBody` enum keyed
by `StyleCatalogue`, with "inherit" as `Option`. It is the same pattern as devices; it is not in
the owner's list, so its timing is §16 call 4.

---

## 9. Foundation (lane A): words, keys, units, strict reading

### 9.1 Words and enums

- `ecad_model::words`: `flag(text) -> Option<bool>` (yes/true/1/on, no/false/0/off; `y` no longer
  read), `flag_word(bool)`, `is_digits`, `uint(text, max)`, `uint_canonical(text, max)`; every copy
  goes through them.
- `trait Word { const ALL; const WORDS; fn word(self); fn parse(text) -> Option<Self> }`, derived
  through strum (`IntoStaticStr`, `EnumString`, `VariantArray`, `serialize_all = "snake_case"`,
  `ascii_case_insensitive`); a non-snake word (`byte.bit`, `fan-anchor`, `0`, `90`) has its own
  `#[strum(serialize)]`. A small macro adds `Serialize`/`Deserialize` (as the word; an unknown word
  is an error naming it and the allowed words) and `JsonSchema` (behind ecad-model's `schema`
  feature) from the strum attribute, the only place a word is written.
- Every `Kind::Choice` list becomes an enum, the 25 on/off switches `Kind::Flag` (F4); typed
  `Choice<E>` entries replace `.word() == "…"`. `StyleCatalogue` (F1, F2), `Side`, `TextKind`,
  `Shown` (`yes`/`no`/`first`), `StarterKey` (stays in electrical, flat, 113 variants), `SymbolKey`,
  `SymbolName`, `cable_prefix` (F5).

### 9.2 What stays a property, and `PropKey`

After lanes B–F the property bags hold only: presentation choices (`text.<kind>.style`,
`text.<rep>.<kind>.show`), defaults-registry values stored per object (066, `ProjectSource::Property`
and object levels), and keys a person adds by hand. For these:

- `ecad-model/src/keys.rs`: `PropKey(&'static str)`, `KEYS` (with `Area`, holder, `ValueSpec`),
  `FAMILIES` (`text.<kind>.style` and similar, one `build`/`parse` pair each), `RESERVED` (every key
  that moved into a struct field: a bag holding one is a read error), `PropsExt` typed readers
  (`flag`, `uint`, `word::<E>`, `text`), `area_of(key)`.
- `Area` is defined here and imported by R9 (design 064 §3.3), trimmed to what stays a property:
  the 17 registry areas, `Text` display choices and `Free`.
- During lanes B–F, `KEYS` also holds each not-yet-moved key, so G1 can reach 0 for literals while
  the move is under way; each lane deletes its keys from `KEYS` and adds them to `RESERVED`.

### 9.3 Units

`ecad-model/src/units.rs`, each `#[serde(transparent)]`: `Mm`, `Metres`, `Radians`, `Degrees`,
`Amps`, `MilliAmps`, `KiloAmps`, `Volts`, `Watts`, `Mm2`, `Celsius`, `Percent`, `Seconds`.
Arithmetic only where it means something (`Mm ± Mm`, `Mm * f64`, `Degrees ⇄ Radians` by `From`, the
only `to_radians` left); the electrical ones get `PartialOrd`, `Display` (`10 A`) and no operators
(calculations are the traits' methods). `RatedVoltage { volts: Volts, current: Ac | Dc }`. A page
point (`Pt`) stays in drawing units; `Mm::to_units(LengthUnit)` is where they meet. Commands take
`Degrees`; the symbol content's `slots[].rotation` changes from radians to degrees (F6). Page
places: `PageColumn`, `PageRow` (`Grid`), `PageNo` (`book.rs`).

### 9.4 Strict reading (F11, approved)

- A typed field with an unknown word or shape (an enum, a `DeviceType` variant or field, a
  `PointKey`, a `Binding`) makes its record fail to decode: reported as damage with the word, the
  field and the record; the project opens read-only (`ReadOnly::Damaged`); the file is kept.
- A word inside a value that stays text (a defaults choice, `Shown`, a `tag_prefixes` key, a
  `RESERVED` key in a bag): `ecad_electrical::words_check::check(doc)` runs once on open; a fault
  opens read-only through `ReadOnly::UnknownWords`, naming the word, its setting and where it is.
- The inspector's Raw and Readable views keep working on such a project (hard rule, A15).

---

## 10. Format and op changes (for the overseer's batch approval)

Pre-v1: fixtures are regenerated, no shims, no format version change.

**Approved 2026-11-16: F1–F12.** F1 `SetStyle.catalogue` typed; F2 `StyleCatalogues` typed; F3′
`Representation.role` = part in the device; F4 on/off → yes/no; F5 `cable_prefix`; F6
`slots[].rotation` in degrees; F7 stored angles `Radians`; F8 `Definition.kind`; F9
`Component.kind`; F10 kind-only keys out of the bag; F11 unknown words are a read error; F12
`SetDeviceKind`.

**Changed by the widened spec:** F8 and F9 are renamed `device_type: DeviceType` with the §2.2
groups; F12 is **superseded** by F13: once every device field is typed, a type change is one field
edit like the others.

**For approval:**

| # | Change | Op or on disk | Lane |
| --- | --- | --- | --- |
| F13 | **`ModifyDevice { id, before: Device, after: Device }`** replaces `SetDeviceKind` (F12) and `SetProperty` for every typed device field. Same pattern as `ModifyItem`, `ModifyRepresentation`, `ModifyFolder`: the whole struct before and after, inverse by swap | new op; `SetDeviceKind` never ships | B |
| F14 | The device struct with the common fields (§2.1), `device_type`, `parts`, `user`, `display`. **Renaming** `Component` → `Device` and its ops (`CreateDevice`, `DeleteDevice`, `SetDeviceDefinition`) changes the 2026-11-08 rule that code and op names stay until the next major format version: approve the fields and the rename separately | record shape; op names if the rename is approved | B |
| F15 | `DeviceType` groups and their structs (§2.2, §2.5); the starters' and project symbols' `device_type` | new data | B |
| F16 | `Terminal { owner: TerminalOwner, label, key: PointKey }` (§2.6) | record shape | B |
| F17 | `Definition`: `part_role`, `SlotText`, `SymbolDefaults`, typed `bridges` (§7.1) | record shape | B |
| F18 | `Project.user_fields` and `Device.user` (§2.8) | new data | B |
| F19 | PLC: `PlcRack`, `PlcModule`, `ChannelGroup`, `Channel` (§3); today's CPU/head-module cards become a rack device plus a CPU module | new data; regenerated fixtures | C |
| F20 | The wire struct with typed fields (§4.2); **`ModifyWire { id, before, after }`** replaces `SetProperty` on wires. Renaming `Conductor` → `Wire` (and its ops) falls under the same rule as F14's rename | new op, record shape | D |
| F21 | `Cable` typed (§4.1), cores fold `members`; **`ModifyCable { id, before, after }`** replaces `SetProperty` and `AddCableMember`/`RemoveCableMember` | new op; two ops removed | D |
| F22 | `Document.device_links` and **`AddDeviceLink` / `RemoveDeviceLink`** (§2.7) | new ops, new data | B |
| F23 | `Page` typed with `PageKind` (§5.1); **`ModifyPage { id, before, after }`** for page edits and renames/moves (design 064 §13 item 2's wish, so `Effects::of` no longer folds `RemovePage` + `AddPage`) | new op, record shape | E |
| F24 | `Project` typed (§5.3): info, revisions, designations; **`ModifyProject { before, after }`** replaces `SetProperty` on the project for typed fields; `Folder.start`; function/location as `EntryId` on devices and pages | new op, record shape | E |
| F25 | `Wire.ends: [Option<Attach>; 2]` with `Attach::Wire` (§6.1); `AttachEnd` carries `Attach`; junction components removed | op shape, records removed | F |
| F26 | `Document.aids`, `Aid`, `Subject::Aid`, `TerminalOwner::Aid`; **`AddAid` / `RemoveAid` / `ModifyAid`** (§6.2) | new ops, new data | F |
| F27 | Boxes write `function`, `location`, `placed_in` on devices in the same transaction (§6.3): no new op, `ModifyDevice` | behaviour | F |
| F28 | Loose parts out of the device store (§6.4, per the owner's call) | new data | F |
| F29 | Title block strictness: `Binding` enum, no `BlockItem::Unknown`, no `StyleBody::Raw` (§5.4) | record shape | E |
| F30 | Typed style records per catalogue (§8), if the owner says now | record shape | G |

`SetProperty` stays, for the keys of §9.2 only. `Op::ALL` and the contract tests list every op;
`Effects::of` (064) maps every new op (§13).

---

## 11. Guards (all at 0, no allowlist, at the end of the last lane)

Each starts as a ratchet in the lane that adds it; the end gate checks them all at 0. Textual ones
in `cargo xtask layers`; those needing the registry or the enums' words are tests in
ecad-electrical. "Today" is an estimate; the lane records the exact start.

| # | Guard | Lane | Today |
| --- | --- | --- | --- |
| G1 | A string literal in key position that is a registered or reserved key | A (B–F lower it) | ~170 |
| G2 | Boolean words compared or matched outside `words.rs` | A | 13 |
| G3 | `.all(…is_ascii_digit…)` outside `words.rs` | A | ~30 |
| G4 | A string-literal match, `matches!` or `==` on a word of any `Word` enum | A (B–G lower it) | ~180 |
| G5 | `#![deny(clippy::wildcard_enum_match_arm)]` on the type modules (`words`, `keys`, `units`, `device`, `plc`, `wire`, `cable`, `page`, `aid`) and on `symbols.rs`, `behaviour.rs` | A–F | — |
| G6 | The ten predicates, `body_type`, `of_body`, `kind_of` on a device, `rep_type` defined anywhere | B, C, F | 13 |
| G7 | `to_radians`/`to_degrees` outside `units.rs`; `rotation`/`degrees`/`angle` typed `f64`; `_mm: f64`; `Option<f64>` from `rating/parse.rs` | A | 21/21/3/8 |
| G8 | `Kind::Choice(&[`; `.word() == "` | A | 31/10 |
| G9 | `catalogue`/`role` typed `&str`/`String` | A, B | 24 |
| G10 | `Unknown(`, `Stored<`, `Raw` bodies, lenient `_ =>` in a `Word` deserialiser | A, E | 3 (title block) |
| G11 | `properties.get(`/`.insert(`/`SetProperty` on a device, wire, cable, page or the project for a key that is not in `KEYS` (the bag holds only §9.2) | B–F | ~400 |
| G12 | Parsing a terminal key text (`strip_prefix('.')`, `split_once('.')` on `Terminal.key`) | B, C | ~12 |

---

## 12. Lanes, dependencies and order

### 12.1 The lanes

| Lane | Content | Main files | Needs first | Tester |
| --- | --- | --- | --- | --- |
| **A1** Foundation types | `words.rs`, `units.rs`, `keys.rs` skeleton, `Word` + strum, `StyleCatalogue`, `Side`, page places, `ReadOnly::UnknownWords` + the damaged-record path for typed words, guards G2, G3, G8, G10 as ratchets | ecad-model (new files), ecad-file open report, xtask | nothing: additive, **can start now**, before 070 | yes (the read path) |
| **A2** Foundation adoption | every caller onto `words`, `PropKey`, units, `Choice<E>`/`Kind::Flag` (F4), `StyleCatalogue` in ops (F1, F2), `TextKind`/`Shown`, `StarterKey`, `SymbolKey`, `cable_prefix` (F5), F6, F7 | every crate, `cmd/*` | A1, **070 and 073 landed** (rewrites the shims 070 just wrote) | no |
| **B** Device | `Device` and its common fields, `DeviceType` (groups 1–11, 14), traits, `PointKey`, `PartRole`/`Role` (F3′), symbol type and defaults, part lines, user fields, device links, the predicates gone; F13–F18, F22 | `doc.rs`, new `device/`, `symbols.rs`, `devices.rs`, `strips.rs`, `connectors.rs`, `bom.rs` (part lines), `xref.rs`, `rating/` (protection side), `cmd/device.rs`, `symbol.rs`, `terminal.rs`, `connector.rs`, `level.rs` | A2 | yes |
| **C** PLC | `PlcRack`, `PlcModule`, channels (F19) | `plc/*`, `cmd/plc/*`, app `lists/plc.rs`, `card_editor.rs` | B step 1 (the `Device`/`DeviceType` skeleton) | yes |
| **D** Wire, cable, net | `Wire`, `Cable`, `Circuit` (F20, F21) | `wires.rs`, `cables.rs`, `connectivity.rs`, `wiring.rs`, `rating/` (circuit side), `cmd/wire.rs`, `cmd/cable.rs` | A2 | yes |
| **E** Page, project, title block | `Page`/`PageKind`, `Folder.start`, `Project`, revisions, designations, title block strictness (F23, F24, F29) | `book.rs`, `sheet.rs`, `page_types.rs`, `generated.rs`, `revisions.rs`, `designations.rs`, `front.rs`, `title_block/*`, `cmd/sheet.rs` | A2 | yes |
| **F** Not devices out | junctions not stored, `Attach::Wire`, drawing aids, boxes set location, loose parts (F25–F28) | `connectivity.rs`, `wires.rs` (ends), `continuations.rs`, `locations.rs`, `bom.rs` (items), `translate.rs`, `cmd/wire.rs` (joins) | B and D landed (it changes `Wire.ends` and the device store) | yes (movement and `translate.rs`) |
| **G** Style records | one struct per catalogue (F30) | `style.rs`, `styles/*`, `defaults/styles.rs` | A2; owner call 4 | no |

### 12.2 Order and parallel work

```
070 ──► 073 ──► A2 ──┬──► B ──┬──► F ──► (end gate)
A1 (now) ────────────┘        ├──► C (after B step 1)
                     ├──► D ──┘ (F also waits for D)
                     ├──► E
                     └──► G (if now)
```

- **A1 starts now**, alongside 070: new files in ecad-model and the open report in ecad-file; it
  touches no command shim. Its one shared edit is ecad-file's open report, which 070's batch 8
  also changes: A1 takes that step after 070's ecad-file batch, or rebases on it.
- **A2 waits for 070 and 073**: it rewrites the command shims 070 just wrote, and 073 moves the
  crate they live in.
- **B, D and E run in parallel** after A2: their files don't overlap except `rating/` (B owns the
  protection side, D the circuit side; the split is by module before they start) and `cmd/mod.rs`'s
  setprop table (`PROP_RULES`): each lane adds its own rows in its own file
  (`cmd/props/<lane>.rs`), registered like `lanes.rs`.
- **C** starts once B's first step (the `Device` struct and the `DeviceType` skeleton with its
  variants empty) has merged into `refactor/0.2.3`.
- **F** waits for B and D (it changes `Wire.ends` and moves components out of the device store).
- Each lane merges into `refactor/0.2.3` when green (the 0.2.3 process: no lane PRs).

### 12.3 What R9 needs

R9's views read the typed fields and `DeviceType` directly, so each view can only be built once the
types it reads have landed. R9's own steps (design 064 §9) then map onto the lanes:

| R9 step | Needs |
| --- | --- |
| 1 split `project.rs`; 2 `Effect`, `Effects::of`, `Area`, the trait | **A1 + A2** |
| 3 feeds (tree, devices, props) | **B** |
| 4 `FrameView` | **E** |
| 5 `ConnectorsView`, `RefsView` | **B**, **D** (cables); continuation names follow **F** |
| 6 `PagesView` | **E** |
| 7 `OutputsView` (PLC, strips, strip pages) | **B**, **C**, **D** |
| 8 `ChecksView` | **B**, **C**, **D**, **E** (and **F** for boxes and continuations) |
| 9 `GeneratedView` | **E** |

So **R9 can start after A1 + A2**, build its skeleton and step 3 after B, and take each later view
as its lanes land. R9 must not build a view on property routing that a lane is about to remove:
its worker checks the lane table before each view step.

---

## 13. With lane 070 and with R9

**070.** Its params structs are where command inputs are typed; lanes A2–F change their fields:
`rotation: Degrees`; `catalogue: StyleCatalogue`; `symbol: SymbolKey`; connector `halves: Side`,
`family`, `layout`; `showtext` `kind: TextKind`, `show: Shown`; lengths `Mm`; cable `length:
Metres`; PLC commands build `ModifyDevice`; cable and wire commands build `ModifyCable` /
`ModifyWire`; page and project commands `ModifyPage` / `ModifyProject`. `setprop`'s `key` stays a
string, and `PROP_RULES` routes a moved key to its struct field and the `Modify*` op, so `setprop
{key: "plc.slot"}` still works for people and agents. 070's lenient JSON readers are left to the
end-gate review.

**R9 (design 064).** What changes in `Effect`:
- `Effect::Device { id, before: Option<&Device>, after: Option<&Device> }` (renamed from
  `Component`), produced by `CreateDevice`, `DeleteDevice` and `ModifyDevice`; a type change is an
  ordinary `ModifyDevice` (no `Effect::Kind`).
- `Effect::Wire`, `Effect::Cable` gain the `Modify*` ops; `Effect::Member` goes (cores are inside
  `Cable`).
- `Effect::Page` comes from `ModifyPage` directly; 064's pair folding for pages goes.
- New: `Effect::Project { before, after }`, `Effect::Aid { id, before, after }`,
  `Effect::DeviceLink { id, before, after }`.
- `Effect::Prop` remains only for §9.2's keys, with `Key { raw, area, known: Option<PropKey> }`.
- Views route by matching the struct fields and `DeviceType` variants (`before`/`after`) with no
  `_`; 064's `Area` routing shrinks to the display keys. 064 C1 (`every_op_has_an_effect`) covers
  the new ops.

---

## 14. Acceptance (each lane takes its rows; tester for every lane but A2 and G)

| # | Test | Lane |
| --- | --- | --- |
| A1 | `one_boolean_rule`, `whole_numbers`: the word readers on tables | A1 |
| A2 | `words_round_trip`: every `Word` enum and every struct-enum (`DeviceType`, `PageKind`, `Aid`, `PointKey`): `parse(word(x)) == x`, serde writes the word, schema equals `WORDS` | A1, each lane for its enums |
| A3 | `keys_are_registered`: `KEYS`, `FAMILIES`, `RESERVED` disjoint and classified | A1, then each lane |
| A4 | `unknown_words_open_read_only`: an unknown word or field in each typed record and in each text value opens read-only, names the word, setting and place; saving refused; bytes unchanged | A1, each lane for its records |
| A5 | `commands_refuse_unknown_words` and their MCP contract tests (brief 011) | A2, each lane for its commands |
| A6 | `degrees_in_radians_stored`, `ratings_typed` | A2 |
| A7 | `types_match_the_old_predicates`: before deleting the predicates, every device of the test projects and the medium bench project gets the group the predicates implied | B, C |
| A8 | `type_comes_from_the_symbol`: placing each starter gives its group; joining doesn't change it; a symbol swap re-reads it in one transaction | B, C |
| A9 | `modify_ops`: every new op is in `Op::ALL`, inverse, atomic, invariant-checked; `setprop` on a moved key records the `Modify*` op, also through MCP | B–F |
| A10 | `methods_not_recalculated`: the 057 ratings, the PLC address blocks, the power demand give today's results through the traits; no check computes them itself (a guard test greps the checks for the formulas' helpers) | B, C, D |
| A11 | `rack_module_channel`: a CPU card becomes rack + CPU module; addresses, I/O list and rack overview unchanged | C |
| A12 | `junctionless_nets`: a T-joint by `Attach::Wire` gives the same nets and wire numbers as the old junction; the dot is drawn where it was | F |
| A13 | `boxes_set_location`: placing, moving, reshaping and deleting a box or a device writes the devices' values in the same transaction; undo restores them | F |
| A14 | `format_shapes`: a project saved by each lane has its §10 shapes; fixtures regenerated, not shimmed | each lane |
| A15 | `inspector_shows_everything` (hard rule): the inspector's Raw and Readable views show every new struct, field and op, and a read-only project's offending record or value with its reason | each lane |
| A16 | guards at 0 | end of F (and G) |

**Bench** (medium while building, large once per lane): `views.setprop_tag`, `views.move_symbol`
no worse than `refactor/0.2.3` + 10%; a PLC slot edit and a box move no worse than + 10%; project
open no worse than + 5%; the op log of the medium tier no more than + 10% larger (whole-struct
`Modify*` ops).

---

## 15. Out of scope

`Pt` and drawing-unit coordinates; footprints and panel layout (0.2.4); the import step for other
tools' files; the check-kinds table (R7) and R9 itself; CADCraft (no file outside `ecad/`).

---

## 16. Owner decisions

### Answered (owner, 2026-11-16)

Everything in Part I and the spec: unknown words are a read error; data, not inference; one struct
per object with typed variants; traits and methods own the calculations; stored relationships;
device common fields and user fields; `DeviceType` from IEC 81346 classes (QA and QB → Protection);
the symbol carries the type; PLC in three levels; sibling links; junctions not stored; drawing aids
and boxes that set location on devices; cable, wire, page and title block typed; the lanes. This
settles the five earlier calls: fuse terminals are Protection; a rack is its own level above the
modules; junctions, continuations and boxes leave the device store; contact data lives in
`Switching` (Relay and Switch); empty kinds stay General until they earn a type.

### Going ahead unless you object

Every stored thing becomes its own typed object: devices, PLC racks and modules, wires, cables,
pages, the project's own data (title, revisions, function and location lists), folders and symbols.
Devices fall into fourteen groups (table in §2.2), from power supplies and protection through
relays, switches, sensors, motors, valves, lamps, heaters, terminal strips, connectors and the two
PLC levels, with General for the rest. Today's CPU or head-module card becomes a rack with its CPU
module in it. Function and location become picks from the project's lists, so renaming a code is
one edit. Each kind of object gets one "change" record in the history (a device, a wire, a cable, a
page, the project), so undo and agents see a type change like any other edit; a free-text `setprop`
still works. The work runs as seven lanes: A1 can start today, A2 after 070 and the rename, then
devices, wires and cables, and pages in parallel, PLC once devices have started, and the
not-devices lane last. The views lane (R9) starts after A1 and A2 and takes each view as the types
it reads land.

### Needs your call

1. **T-joints without junctions.** Junctions are no longer stored, but a wire that tees into
   another still has to end somewhere. (a) A wire end can attach to another wire at a point; the
   dot is drawn where three or more meet. (b) Keep a tiny "node" object that wires end on, with no
   device data. *Recommended: (a)*: (b) is a junction under another name.
2. **A location typed by hand versus a box.** Boxes now set function and location on the devices
   inside them. If a person types a different location on a device inside a box: (a) the last edit
   wins: the typed value stays until the device next moves into or out of a box, which resets it to
   the box's; (b) a typed value always wins and the box never overwrites it, with a Checks row when
   they differ. *Recommended: (a)*: simple and visible; (b) needs a "set by hand" flag on every
   device.
3. **Parts-list-only items until 0.2.4.** The list holds only panel items with no device (trunking,
   DIN rail, cable ties, loose crimps: description, part number, manufacturer, quantity and unit,
   location). (a) Keep them as a plain list of loose parts on the project, out of the device store,
   until the 0.2.4 layout places them. (b) Drop the feature until 0.2.4. *Recommended: (a)*: the
   BOM keeps working and nothing is lost.
4. **Styles typed now or later.** Text, wire, cable, page and frame styles are still a free list of
   fields per style. (a) Type them in this refactor as lane G, in parallel. (b) After 0.3.0.
   *Recommended: (a)*, so 0.3.0 has no property bags left except display choices; it is a medium
   lane and touches only the style files.
5. **Wire and core colours.** Today a colour is free text up to 24 characters. (a) Only IEC 60757
   codes (`BU`, `BN`, `GNYE`, combinations like `BU/WH`); anything else is refused, and core
   *numbers* are core labels, not colours. (b) Keep free text. *Recommended: (a)*, which lets
   checks and reports read colours; a project with a non-IEC colour gets a read error and is
   regenerated (pre-v1).
