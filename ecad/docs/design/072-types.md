# Design 072: The typed object model

Status: **Approved (owner, 2026-11-17).** Designer 2026-11-16, widened by the owner the same day,
revised 2026-11-17 with the owner's answers; every owner call is answered (§17). Every format and op
change is approved by the overseer (§11): F1–F42 and the revisions F15′, F19′, F25′, F27′, F28′
(F12 superseded by F13; F30 approved with the owner's answer on styles). Brief:
`docs/briefs/072-types.md`.

**Goal (one sentence).** What every stored object is, and how it relates to other objects, is held
in its own typed struct, never worked out from names, symbols, strings or positions, so the
compiler finds every place a new object, field, kind or unit must be handled.

Sources: code review 2026-11 (`C:\dev\ecad-features\_tmp\code-review-2026-11.md`), findings 4, 6,
7 and 10; the owner's spec and answers of 2026-11-16 and 2026-11-17.

**Names.** `Component` is renamed `Device` and `Conductor` is renamed `Wire`, with their ops (F14,
F20, approved; this supersedes the 2026-11-08 rule that code names stay until the next major format
version, `docs/decisions.md` 2026-11-16). Lanes B and D update `docs/glossary.md` in the commits
that rename. `Terminal` keeps its name. `ecad-cadcraft` becomes `ecad-engine` in lane 073.

---

## Part I. Principles and rulings (decided)

**The owner's principles, for every object type.**
1. **Data, not inference.** What something is, and how it relates to other things, is stored in
   its type. No predicate walks to a symbol, no name is matched, no cache keeps a guess.
2. **One struct per object type** with its own fields. Kind-specific data lives in enum variants,
   so a wrong combination can't be built.
3. **Shared behaviour is traits**, implemented by the types. Each type owns its calculations as
   methods; checks, power generation and reports call them and never recalculate.
4. **Relationships are stored explicitly** as IDs, at the level they belong.
5. **Strict words.** An unknown word or field in a file is a read error: the project opens read-only
   with a message naming the word, the setting and where it is (the damaged-records path); the file
   is never changed. No `Unknown(String)`, no `Stored<E>`.
6. **No free text as a raw `String`** (owner, 2026-11-17): every text a person types is a validated
   text type that refuses hidden and bidi characters (§9.4).
7. **No compatibility code.** Fixtures and test files are regenerated; no shims.
8. **No property bags left at 0.3.0** except display choices (owner, 2026-11-17: styles typed in
   this refactor, lane G).

**Earlier rulings that stand.** strum for `word()`/`parse()`; booleans `yes`/`no` (read also
`true`/`1`/`on`); one source per value, shared vocabulary in ecad-model; every guard ends at 0 as a
ban; the built-in symbol list stays out of the core; a cable is not a device.

**Where the types live.** Stored structs and enums in **ecad-model**. Calculations that need the
electrical rules or tables are trait methods defined and implemented in **ecad-electrical** on the
model types (a local trait on a foreign type), so `psu.available_power()` reads as the type's own
method while Core stays layered.

---

## 0. What the code says (recounted at `d323677a`)

Counts are for product code, ±10%.

### 0.1 The store

`Document` (`ecad-model/src/doc.rs:576`) holds `pages`, `page_order`, `layers`, `items`,
`components` (devices **and** junctions, continuations, location boxes, BOM-only items),
`terminals` (connection points), `representations`, `conductors` (wires), `cables` (+
`CableMember`), `definitions` (symbols), `settings`, `versions`, `properties` (the project's own),
`folders`, `folder_order`, `standard`, `styles` (style records). Never stored: the reverse `index`
and the nets.

Five objects are a typed shell around a property bag (`Page`, `Component`, `Conductor`, `Cable`,
the project); style records are a bag per catalogue. A drawn wire today **is** a conductor: each
drawn segment between two connection points (or a junction) is one `Conductor`, so the drawing's
shape decides the wires.

### 0.2 Strings (recounted)

~105 key constants in 27 files; 42 literal `"tag"` keys; 37 keys built with `format!`, 151 split by
hand; 3 boolean word sets at ~13 sites; ~30 digit checks; 31 literal choice lists (25 on/off); 595
string-literal match arms; 113 starters and 11 `match key` blocks; 7 style catalogues as `String`
in 23 signatures; free-text roles; 10 device predicates (151 calls); angles mixed (one command in
radians); 8 ratings parsers returning bare `f64`; page places as bare `u32`/`String`. One
hidden-character check exists (`cables::is_hidden`, the gland fix) and is used for cable fields
only.

### 0.3 What is worked out today instead of stored

| # | Inferred today | From | Becomes |
| --- | --- | --- | --- |
| I1 | What a device is | ten predicates, starter key names, the `kind` property | `Device.device_type` (§2) |
| I2 | A PLC card's type, whether it is a rack | its head symbol's starter key | `PlcRack.head`, `PlcModule.function` (§3) |
| I3 | A connection point's side, channel, level, position | the terminal key text | `PointKey` (§2.7) |
| I4 | A symbol's part in its device | `reps::rep_type` from the definition | `PartRole` (F3′) |
| I5 | A fuse bank's outputs | digit-only labels of its starter | `Protection.outputs` |
| I6 | A protective device's rated current and curve | parsed from free `rating` text | typed fields (§2.2) |
| I7 | A contact arrangement | `match` on starter keys | `Switching.contacts` |
| I8 | Which wires a net has | the drawing's shape: one conductor per drawn segment, junctions as components | an ordered list of conductors per net (§4) |
| I9 | A continuation and its name | `kind` + `name` properties | `Aid::Continuation` (§6) |
| I10 | A device's location and function from boxes | box geometry, when asked | stored on the device with `fixed` (§6.3) |
| I11 | What a page is | `generated`, `strip`, `page.type` words | `PageKind` (§5.1) |
| I12 | Revision rows and columns | `rev.<n>.<field>`, `revisions.columns` | `Project.revisions` (S4: typed `Revision` records, a `Rev` in `Project.rev_scheme`) |
| I13 | Function and location lists | `designation.<aspect>.<entry>.*` | `Project.locations`, `functions` (§5.3) |
| I14 | A strip's jumpers | `link.<n>` range text | `TerminalStrip.links` |
| I15 | A cable's ends, glands, rating inputs | `end.from.location`, `gland.to`, `rating.*` | `Cable` fields |
| I15a | A cable's tag, its fixed flag and its description (0.2.3 S1, #2) | `tag`, `description` | `Cable.tag: TagSlot` (state and bare tag in one value), `Cable.description: Option<Description>`, as a device's; written on every cable record; a cable record whose bag still holds one of the keys is a read error |
| I16 | Part lines | `part.<n>.<field>` | `Device.parts` |
| I17 | Which device text a symbol slot places | `TextSlot.text` words | `SlotText` (§7) |
| I18 | Title block bindings, unknown items | text kept as written | strict elements (§5.4) |
| I19 | Style record fields | a map per catalogue | one struct per catalogue (§8) |
| I20 | A wire's or core's colour | free text (`GNYE`) | `Vec<ConductorColour>` (§4.6) |

---

## 1. The object map

| Object | Struct | Enums with data | Traits | Stored relationships | Lane |
| --- | --- | --- | --- | --- | --- |
| **Device** | `Device` (§2.1) | `DeviceType` (§2.2) | `Bom`, `Schematic`, `Layout` (0.2.4), and per group `Supply`, `Load`, `Protects`, `Switching`, `Senses`, `Signals`, `Connects`, `Addressed` | its symbols; its connection points; `PlcModule.rack`; device links | B, C |
| **PLC station / module / channel** | `PlcRack`, `PlcModule`, `ChannelGroup`, `Channel` (§3) | `RackHead`, `ModuleFunction`, `ChannelFunction` | `Addressed`, `Load` | module → rack, channels inside the module, points `PointKey::Channel` | C |
| **Wire** (conductor) | `Wire` (§4.2) | — | `Circuit`, `Bom` | its two connection points, its place in its net's order, its wire type | D, F |
| **Route** (drawn connection line) | `Route` (§4.3) | `RouteEnd` | — | page, its ends (a point, another route) | F |
| **Net** | derived, never stored | — | — | from routes, links and continuations | F |
| **Cable** | `Cable` (§4.5) | `Shield` | `Bom`, `Schematic`, `Circuit` | cores → wires, ends → location/function entries, cable type | D |
| **Page** | `Page` (§5.1) | `PageKind` | — | folder, page type, frame, title block | E |
| **Folder** | `Folder` + `start` | — | — | parent | E |
| **Project** | `Project` (§5.3) | `RevisionColumn` | — | — | E |
| **Location / function list** | `Location`, `Function` entries (§5.3) | — | — | devices and pages refer by id | E |
| **Loose part** | `LoosePart` (§6.4) | — | `Bom` (then `Layout` in 0.2.4) | its location, function | F |
| **Title block** | per 055c (§5.4) | `Element`, `Binding` | — | inside a frame style | E |
| **Drawing aid** | `Aid` (§6.2) | `Aid { Continuation, Box }` | — | its drawing, a continuation's point | F |
| **Connection point** | `Terminal { owner, label, key }` | `PointKey`, `TerminalOwner` | — | owner device or aid | B |
| **Representation** | + `role: Role` | `Role`, `PartRole` | — | subject, page, items | B |
| **Symbol** | `Definition` + `device_type`, `part_role`, typed slots and defaults (§7) | `SlotText` | — | origin | B |
| **Style records** | one struct per catalogue (§8): text, wire type, cable type, page type, frame, location, rating table | `StyleBody` | — | catalogue, layer | G |
| **Item, layer, version, standard header** | unchanged | — | — | — | — |

---

## 2. Device

### 2.1 The struct

```rust
pub struct Device {
    pub definition: Option<DefinitionId>,
    pub tag: TagSlot,                      // None | Automatic(Tag) | Fixed(Tag) | Provisional(Tag): the state holds the tag (0.2.3 S6 #3)
    pub location: Place<LocationId>,       // §6.3: Unset | Manual(id) | Proposed(id): the state holds the entry
    pub function: Place<FunctionId>,
    pub description: Option<Text<256>>,
    pub part_number: Option<Text<64>>, pub manufacturer: Option<Text<64>>,
    pub article_number: Option<Text<64>>, pub type_text: Option<Text<64>>,
    pub rated_voltage: Option<RatedVoltage>,
    pub parts: Vec<PartLine>,                                          // 047's lines 1…99
    pub user: BTreeMap<UserFieldId, UserValue>,                        // project-defined fields
    pub device_type: DeviceType,
    pub display: Properties,                                           // display choices only (§9.2)
}
```

`Text<N>` is the validated text type (§9.4). Every device is `Bom` (part number, qty 1 for line 0
plus its part lines, unit) and `Schematic` (has symbols); `Layout` comes with footprints in 0.2.4.

### 2.2 `DeviceType`: twelve groups, each justified by its own fields and methods

Each group exists because it has fields and methods no other group has. Groups start from IEC
81346-2's top-level classes (the letters ECAD's built-in tag prefixes use). Merged against the
earlier list of fourteen: **Heater** has only a power and a demand, so it is a plain **Load**;
**Solenoid** likewise (its valve positions are a description, not a calculation); **Relay** and
**Switch** share contacts, the contact table and every switching method, and differ only in what
operates them, so they are one **Switching** group with an `operator` (owner, 2026-11-17).

| # | Group | IEC 81346-2 | Starters today | Own fields | Traits and methods |
| --- | --- | --- | --- | --- | --- |
| 1 | `PowerSupply` | G (generating: PSUs, batteries), T (converting: transformers) | `psu`, `trafo` | input and output `RatedVoltage`, output current `Amps` or power `Watts`, `ratio` for a transformer | `Supply::available_power()`, `available_current()`, `output()`; `Load::demand()` on its input (output ÷ efficiency) |
| 2 | `Protection` | F (protecting), Q's protective switches (MCB, RCD, RCBO, fuse switch) | `fuse`, `mcb1`, `mcb2`, `rcbo2`, `rcd2`, `rcd4`, `fuseterm`, `fusebank4…12` | `device` (Fuse, Mcb, Rcd, Rcbo, FuseBank), `rated_current`, `curve`, `residual` (mA), `breaking` (kA), `poles`, `outputs` (a fuse bank's per-output rating), `rating_text` | `Protects::rated_current()`, `trips(current, time)`, `protects(circuit)` (057's coordination), `Switching::contacts()` for auxiliaries |
| 3 | `Switching` | K (relays, contactors, SSR, timers), S (manual control: push buttons, selectors, switch-disconnectors) | `coil`, `power3`, `power4`, `co`, `ssrin`, `ssrout`, `pbno`, `pbnc`, `sel`, `swco` | `operator: Operator { Coil(Coil { voltage, power }), Manual(Actuator), Timer(Timer { function: OnDelay \| OffDelay \| …, coil: Coil }), … }`, `contacts: ContactSet`, `contact_table: TablePosition` | `Switching::contacts()`, `contact_table()`, `free_contacts(doc)` (offered minus drawn); `Load::demand()` when operated by a coil |
| 4 | `Sensor` | B (converting a variable to a signal) | `prox3`, `floatno/nc`, `rollerno/nc`, `interlockno/nc`, `tc` | `sensing` (Proximity, Float, Limit, Interlock, Temperature…), `output` (Contact, Pnp, Npn, Analogue { range }), supply `RatedVoltage` | `Senses::signal()` (what a PLC input must accept: the "sensor on the wrong channel type" check), `Load::demand()` |
| 5 | `Motor` | M (mechanical energy) | `motor1`, `motor3` | `phases`, `rated_power`, `rated_current`, `power_factor`, `efficiency`, `starting` (Direct, StarDelta, Soft, Vfd) | `Load::demand()`, `full_load_current()`, `starting_current()` (protection and cable sizing) |
| 6 | `Load` | E (heat, light as energy), MB (valves, solenoids, brakes), other consumers | `heater`, `valve`, `valve2` | `rated_power: Watts` | `Load::demand()` |
| 7 | `Indicator` | P (presenting information: lamps, beacons, buzzers, meters) | `lamp` | `colour: Option<IndicatorColour>`, `signal: SignalType` (Steady, Flashing, Rotating, Audible, Meter), `rated_voltage: RatedVoltage` (with AC/DC), `power: Watts` | `Signals::signal()`, `colour()`; `Load::demand()` |
| 8 | `TerminalStrip` | X (connecting: DIN-rail terminals) | `terminal`, `terminalpe`, `term2…`, `term3…`, `termlnpe…`, `termsensor…`, `termdisc`, `termfuse` | levels (label, fuse rating), jumpers, accessories, PE part (§2.5) | `Connects::points()`, `jumpered(position)`, `Bom` lines per terminal type and accessory |
| 9 | `Connector` | X (plugs, sockets, bulkheads) | `socket*`, `plug*`, `dsub*`, `m12*`, `rj45`, `usba`, `usbb`, `pinpair`, `pins`, `pinp` | `family`, `pins`, `pin_names`, `halves` (part per half), `mating`, ignored checks | `Connects::points()`, `mates_with(other)`, `pin_map()` |
| 10 | `PlcRack` | K (processing signals): a PLC station | today's CPU and head-module cards | §3 | `Addressed::address_space()`, `modules(doc)`, `io_count(doc)` |
| 11 | `PlcModule` | K: an I/O or power module in a station | `plcdi`, `plcdq`, `plcai`, `plcao`, `plcrtd`, `plciol`, `plcps`, `plcpm`, `plccm`, `di`, `do` | §3 | `Addressed::address_block()`, `channels()`; `Load::demand()` |
| 12 | `General` | everything else (R, C, U, V, A not PLC…) | project symbols with no type, converts | none | none |

- **QA and QB → Protection**; a fuse *position* in a strip (`termfuse`) stays the strip's.
- Contactors are `Switching` (they switch but don't protect).
- **Indicator (overseer rulings, 2026-11-18).** The signal is a starter default stored when the
  device is created (a lamp: `Steady`), never read from the symbol later; the colour starts
  empty. The rating reader reads colour (`red`, `RD`) and signal (`flashing`) into their fields.
  A lamp is bought by its voltage, so it has a typed `rated_voltage` (AC or DC) beside its power:
  `24 V DC 2 W` reads in full. Checking it against its circuit's voltage is later work. (A motor
  still refuses a voltage; listed for the end-gate review.)
- **PowerSupply (overseer rulings, 2026-11-18).** The kind is set by the starter (`psu` → `Psu`,
  `trafo` → `Transformer`) and each kind carries only its own fields: `Psu { input, output:
  Option<RatedVoltage>, output_current: Option<Amps>, output_power: Option<Watts> }`, and
  `Transformer { input, output: Option<RatedVoltage>, rated_power: Option<VoltAmps> }`. A
  transformer therefore cannot hold watts, and a DC voltage on one is invalid. Batteries come later.
  The ratio is never stored: `Transformer::ratio()` works it out from the two voltages. The reader
  takes `230 V AC / 24 V DC`, `230VAC/24VDC` and `in 230 V AC out 24 V DC` as input / output.
  `400/24 V` gives both sides the one unit, and on a transformer it reads as AC/AC. One DC voltage
  alone on a PSU is its output (a PSU's DC side is its output). One AC voltage alone on a PSU, one
  voltage alone on a transformer, and a PSU voltage without AC/DC ask "input or output?" (or "AC
  or DC?"). A PSU takes A and W together, with no consistency check yet. `VoltAmps` is its own unit
  (VA is not W): a transformer reads `250 VA`. W or a current on a transformer, DC on a
  transformer, and VA on a PSU are refused.
- Catalogue products (an IVP4 versus an IVP6) are part-library data, never variants.
- A group is added only when a symbol needs fields or a calculation no group has.
- `DeviceType` offers `as_supply()`, `as_load()`, `as_protects()`, `as_switching()`, … (exhaustive,
  no `_`), so a check asks for the trait it needs and never names a group it doesn't care about.

**Ratings (owner decisions, 2026-11-18; a deviation from the table's `rating_text`).** Today's free
`rating` text on every device becomes typed data:

- **Typed fields are the truth.** Each group with a rating holds it as several typed fields on its
  struct (a breaker: rated current, curve, breaking capacity, poles; an RCD: residual; a fuse bank:
  a rating per output; a contactor: AC-3 rating and coil voltage; a motor: rated power; a power
  supply: its output). There is no `rating_text` and no stored display string.
- **One common note.** `Device.rating_note: Option<Text<64>>` holds only text that does not read as
  the group's typed values ("see datasheet", a vendor code). It is a note, never a second copy of
  the rating.
- **Parsed at the edge, never guessed.** One reader per group, in `ecad-electrical`, used by the
  UI, the CLI, MCP and the symbol defaults (`DEFINITION_DEFAULT_KEYS`) alike. `setprop rating "B16"`
  on a breaker stores curve B and 16 A. Ambiguous input (a bare `16` on a breaker) is refused with a
  typed error naming what is missing ("curve?"). Text that is no rating at all goes to the note with
  the typed fields left empty, so nothing people type today is lost. Every way in shows what was
  read ("curve B, 16 A") in the command's result.
- **Shown from the fields.** Text slots and symbols show the typed fields when set, else the note.
- **Checks read only the typed fields.** A device whose group has rating fields, all empty, gets a
  "no rating" warning even when it has a note.
- **The part sets the rating (decided, waiting on a parts library).** When the part number's library
  entry carries a rating, it fills the fields; a value typed on the device is an override, and a
  check reports an override that differs from the part. Not built until a parts library with ratings
  exists (backlog); until then the typed fields are stored plainly.

### 2.3 Where a device's type comes from

The symbol declares it (`Definition.device_type`, with default data); placing a symbol as a new
device creates that type and only values are filled in; adding a symbol to an existing device never
changes the type; changing it is one edit (a symbol swap re-reads it, keeping shared fields).
`setprop` on a moved key still works through 070's `PROP_RULES`.

### 2.4 Traits

```rust
// ecad-model (data the store answers): implemented by Device, Cable, LoosePart, and later layout items
pub trait Bom       { fn bom_lines(&self) -> Vec<BomLine>; }          // part number, qty, unit, per line
pub trait Schematic { fn symbols<'a>(&self, doc: &'a Document) -> Vec<RepresentationId>; }
pub trait Layout    { fn footprint(&self) -> Option<…>; }              // 0.2.4
// ecad-electrical (calculations)
pub trait Supply    { fn output(&self) -> RatedVoltage; fn available_power(&self) -> Option<Watts>; fn available_current(&self) -> Option<Amps>; }
pub trait Load      { fn demand(&self, at: RatedVoltage) -> Option<Demand>; }
pub trait Protects  { fn rated_current(&self) -> Option<Amps>; fn trips(&self, i: Amps, t: Seconds) -> Trip; fn protects(&self, c: &dyn Circuit) -> Verdict; }
pub trait Switching { fn contacts(&self) -> &ContactSet; fn contact_table(&self) -> TablePosition; }
pub trait Senses    { fn signal(&self) -> SignalKind; }
pub trait Signals   { fn signal(&self) -> SignalType; fn colour(&self) -> Option<IndicatorColour>; }
pub trait Connects  { fn points(&self, d: &Device) -> Vec<PointKey>; }
pub trait Addressed { fn address_block(&self, doc: &Document) -> Option<AddressBlock>; }
pub trait Circuit   { fn load(&self) -> Option<Amps>; fn protected_by(&self) -> Option<DeviceId>;
                      fn rating(&self, doc: &Document) -> Rating; fn volt_drop(&self, doc: &Document) -> Option<Percent>; }
```

The BOM, the power summary, the rating checks and the reports call these; the code in
`ecad-electrical/src/{bom,rating}/` becomes their implementations, not parallel calculations.

### 2.5 Terminal strips

Levels (`BTreeMap<(Position, Level), LevelData { label, fuse_rating }>`), jumpers
(`Vec<Jumper { from, to }>`, today's `link.<n>` ranges expanded once on input), accessories (start,
end, after, on, marker card size, jumper bar), PE part. Parent of its connection points.

### 2.6 Relationships

Parent → children: device → its symbols (coil → contacts are one device's representations with
`PartRole` Head and Contact; the contact table lives on the device); strip → terminals; station →
modules (`PlcModule.rack`); module → channels (inside it). Typed sibling links at project level:
`device_links: BTreeMap<LinkId, DeviceLink { kind: MechanicalInterlock | AuxiliaryBlock | …, a, b }>`.
Schematic versus panel is one device with two representations, not a relationship.

### 2.7 Connection points

`Terminal { owner: TerminalOwner { Device | Aid }, label, key: PointKey, accepts: u8 }`,
`PointKey { Def(String), Side(Side), Channel { index, role }, Level { position, level, io } }`.
`accepts` is how many conductors the point takes, copied from `TerminalDef.accepts` in the symbol
(default 2; PE bars and earth studs more) when the point is made (§4.4).

### 2.8 Project-defined user fields

`Project.user_fields: Vec<UserFieldDef { id, name, kind: Text | Number { unit } | Flag | Choice }>`;
devices store values by id; a wrong kind or an unknown id is a read error.

**As built (lane B step 5, 2026-11-20).** `ecad_model::user`: `UserFieldDef { id: UserFieldId,
name: Text<32>, kind }`, `Choice { options }` (1 to 32, none twice ignoring case), at most 64
fields, names unique ignoring case; `Device.user: BTreeMap<UserFieldId, UserValue>` (`Text`,
`Number(f64)` finite, `Flag`, `Choice`), checked by invariant I24, so a bad value does not open.
Fields change through `ModifyProject` (`userfield`); a value through `ModifyDevice`
(`setprop user.<name>`, `user.` reserved on devices). Removing a field clears its values first, in
the same transaction. Links: `Document.device_links: BTreeMap<LinkId, DeviceLink>`, stored in the
`model:project` chunk; `LinkKind` is `mechanical_interlock` or `auxiliary_block` (more when a
check needs one); a pair is linked once per kind; `DeleteDevice` refuses a linked device, so every
delete removes the links first (a merge of two devices re-points them); `Effect::DeviceLink`;
commands `linkdevices`, `unlinkdevices`.

---

## 3. PLC: station, modules, channels

```rust
pub struct PlcRack {                          // a station on a network
    pub network: Option<Text<32>>,            // the network it is on
    pub address: Option<NetAddress>,          // its address there (IP, bus address), validated per network kind
    pub head: RackHead,
    pub address_style: AddressStyle,          // byte.bit, iec, percent, slot
}
pub enum RackHead {
    Cpu { family: Option<Text<32>> },         // usually one per project
    Coupler { protocol: Fieldbus },           // a fieldbus head: EtherCAT, Profinet, EtherNet/IP, Modbus TCP, …
}
pub struct PlcModule {
    pub rack: Option<DeviceId>,               // parent station
    pub slot: Option<u16>,
    pub function: ModuleFunction,
    pub attributes: ModuleAttributes,
    pub address: AddressPin { start: Option<Text<16>>, size: Option<u32> },
    pub supply: Option<RatedVoltage>,
}
/// What a module does (owner, 2026-11-17): generic names, never a vendor's. An enum with data.
pub enum ModuleFunction {
    DigitalIn(ChannelGroup), DigitalOut(ChannelGroup),
    AnalogIn(ChannelGroup),  AnalogOut(ChannelGroup),
    Mixed(Vec<ChannelGroup>),                                   // several groups on one module
    Position(Position { kind: Encoder | Ssi | Counter | …, channels: ChannelGroup }),
    Drive(Drive { kind: Stepper | Servo | DcMotor | …, axes: u8 }),
    Communication(Communication { protocol: Protocol, ports: Vec<Port>, role: Gateway | Serial | Fieldbus }),
    Measurement(Measurement { kind: PowerMeasurement | Vibration | Strain | …, channels: ChannelGroup }),   // DAQ
    SafetyLogic(SafetyLogic { channels: Option<ChannelGroup> }),
    System(SystemModule { kind: PowerFeed | PotentialDistribution | Fusing | EndCap | Shield | … }),
}
/// Properties of a module or its channels, not functions (owner): a safe DI is still `DigitalIn`.
pub struct ModuleAttributes { pub safety_rated: bool, pub intrinsically_safe: bool, pub calibration_certificate: bool }
pub struct ChannelGroup { pub signal: Signal, pub count: u16, pub channels: BTreeMap<u16, Channel> }
pub enum Signal { Dc24V, Ac230V, Relay, Current4_20, Voltage0_10, Rtd, Thermocouple, IoLink, … }
pub struct Channel { pub text: Option<Text<80>>, pub address: Option<Text<32>>, pub tag: Option<Tag>, pub safe: Option<SafeState> }
```

- **A rack is a station on a network with an address.** Its head is a typed choice, CPU or
  Coupler. Usually one station has the CPU; the others have couplers.
- **Today's CPU or head-module card becomes a station** whose head is a CPU (`plccpu`) or a Coupler
  (`plcim`), keeping its tag, description, manufacturer and part number (the head's part). No
  separate CPU module.
- **Modules** carry a `ModuleFunction` with its data. RTD, thermocouple and IO-Link are signal kinds
  of a channel group, not module functions. Today's starters map: `plcdi` → `DigitalIn`, `plcdq` →
  `DigitalOut`, `plcai` → `AnalogIn`, `plcao` → `AnalogOut`, `plcrtd` → `AnalogIn` (RTD signal),
  `plciol` → `Communication` (IO-Link master), `plcps` and `plcpm` → `System` (power feed), `plccm`
  → `Communication`.
- **Safety-rated, intrinsically safe (Ex i) and a calibration certificate are attributes**, never
  functions.
- **Communication modules appear in the station overview**, with protocol and ports.

**Sanity check: one fieldbus terminal range.** A broad modular I/O range (EtherCAT terminals,
public product categories) maps onto one function plus attributes each:

| Public category | Station head or function | Attributes |
| --- | --- | --- |
| Bus coupler | head `Coupler { protocol: EtherCAT }` | — |
| Digital input terminals | `DigitalIn` | — |
| Digital output terminals | `DigitalOut` | — |
| Analog input terminals (voltage, current, RTD, thermocouple) | `AnalogIn` (signal per group) | calibration certificate where offered |
| Analog output terminals | `AnalogOut` | — |
| Position measurement (encoder, SSI, counter) | `Position` | — |
| Communication terminals (serial, gateway, sub-bus, IO-Link master) | `Communication` | — |
| Drive terminals (stepper, servo, DC motor) | `Drive` | — |
| Multi-functional I/O | `Mixed` | — |
| System terminals (power feed, potential distribution, fusing, end cap, shield) | `System` | — |
| Measurement terminals (power measurement, condition monitoring) | `Measurement` | calibration certificate where offered |
| Safety terminals: safe inputs, safe outputs | `DigitalIn` / `DigitalOut` | safety-rated |
| Safety terminals: safety logic | `SafetyLogic` | safety-rated |
| Ex i terminals (intrinsically safe I/O) | the function of their signal (`DigitalIn`, `AnalogIn`, …) | intrinsically safe |

Every category lands on one function; nothing needs a vendor-specific variant.
- The 056 card fields move: rack, slot, accessories into the module; ID (tag), description,
  manufacturer, part number are common device fields.
- Addresses, the I/O list and the station overview are methods (`PlcModule::address_block`,
  `PlcRack::modules` through the index's `rack → modules` map).

---

## 4. Wires, nets and junctions (a design of its own)

### 4.1 What changes

Today a drawn line **is** a wire: every segment between two connection points or a junction is a
`Conductor`, and a T in the drawing makes a junction component. So the drawing's shape decides
which physical wires exist, and moving a line changes the wire list.

The new model, after the behaviour of established electrical CAD tools (clean room: behaviour only):
**a net breaks into conductors with a stored order.**

- **Routes** are what a person draws: lines on a page that connect connection points and each
  other. They decide *which points are on one net*, nothing more.
- **Wires** are the physical conductors: each runs between **two connection points** of the net,
  never to a T or a dot. A net with points P1…Pn has n−1 wires, in a **stored order**: by default a
  chain (1 = P1→P2, 2 = P2→P3, …), fanning out (a star) only at points that accept more than two
  conductors (§4.4).
- **ECAD proposes the order**; the person can **reorder by hand**; a button (**Show wire order**)
  draws the numbers 1, 2, 3 beside the wires on the page.
- **Junction dots and the T indicators** (a small triangle at a T showing which way the order runs)
  are **drawing aids**, like location boxes: drawn from the routes and the stored order, never
  stored.
- A **net** is still derived, never stored: from routes, strip jumpers and continuations.

### 4.2 Wire

```rust
pub struct Wire {
    pub net: NetId,                              // the net it belongs to (stored, stable)
    pub conductor: ConductorId,                  // this conductor's own number in the project (stable)
    pub net_order: u16,                          // its place in its net's order, 1-based
    pub ends: [TerminalId; 2],                   // two connection points of one net
    pub order_fixed: bool,                       // the person reordered this net by hand
    pub wire_type: Option<StyleRef>,             // §4.6
    pub size: Option<WireSize>,                  // overrides the type's: Mm2 | Awg
    pub colours: Option<Vec<ConductorColour>>,   // overrides the type's
    pub number: Option<WireNumber>,              // the shown number, per the project setting below
    pub potential: Option<Text<24>>,
    pub rating: CircuitInputs, pub load: Option<Amps>, pub protected_by: Option<DeviceId>,
}
```

Wires are listed by net and order everywhere a wire list is made (wire list, cable cores, BOM
lengths, the order numbers on the page).

- **`net` and `conductor` are both stored on every wire** (owner, 2026-11-17), so either numbering
  convention can be shown. A `NetId` is given when a net first appears and kept through edits;
  when two nets join, the one with more wires keeps its id (ties: the lower id); when a net splits,
  the part with more wires keeps it and the other gets a new one. A `ConductorId` is given when the
  wire is made and never reused.
- **The visible wire number is a project setting**, `wiring.number_by`: **per net** (potential
  numbering, today's behaviour: every wire of a net shows the net's number) or **per conductor**
  (each wire shows its own). The number's text still follows today's numbering schemes
  (`wiring::Scheme`); only what it counts changes.

### 4.3 Route

```rust
pub struct Route { pub page: PageId, pub points: Vec<Pt>, pub ends: [RouteEnd; 2] }
pub enum RouteEnd { Point(TerminalId), On { route: RouteId, at: Pt }, Free }
```

A route's items are its drawing; `Subject::Route(RouteId)` for its representation. Moving or
redrawing routes is an ordinary edit of routes (`AddRoute`, `RemoveRoute`, `ModifyRoute`).

### 4.4 The order: proposed, kept, reordered

- **The proposal is drawing order** (owner, 2026-11-17): each new connection becomes the next
  wire. A route ending on the middle of an existing route joins from the **nearest end** of the
  wire that route carries: draw A–B, then C onto the middle of A–B, and the wires are 1 = A–B,
  2 = B–C (B being the end nearer to where C's route lands). "Shortest total wire" is a later
  button, not this lane.
- **When a net's set of points changes** (a route added, removed or moved so that a point joins or
  leaves the net, two nets joined, one split), the same transaction rewrites that net's wires: the
  points still on the net keep their order, a point that left is cut out (its two neighbours become
  one wire), a new point is added by the drawing-order rule. Nets whose points didn't change keep
  their wires untouched, even when the routes are redrawn.
- **Reordering by hand** (a list of the net's points the person drags, or picking points in turn on
  the page) rewrites the net's wires and sets `order_fixed`; a fixed net keeps its order when it
  gains a point and only adds the new one.
- **Conductors accepted per point are data** (owner, 2026-11-17). Every connection point stores how
  many conductors it accepts, from its symbol or part: `TerminalDef.accepts` in the definition,
  copied to `Terminal.accepts` when the point is made (default 2; PE bars and earth studs N). The
  proposal chains by default and fans out (a star) only at points that accept more than two: the
  next wire starts from such a point instead of from the last one in the chain.
- **New check, an error:** a point with more conductors than it accepts ("`-X1:3` takes 2
  conductors; 3 are connected").
- **Continuations** are aids, not device points: a wire runs from a device point on one page to a
  device point on another when a continuation joins the net; the order spans the pages.
- **Junction dots** are drawn where three or more route ends meet or a route ends on another route;
  the **T indicator** where a route ends on another, pointing along the stored order. **Show wire
  order** draws each wire's `net_order` at the middle of its path along the routes.
- **Today's projects**: regenerated with one wire per consecutive pair of points, proposed order.

### 4.5 Cable

```rust
pub struct Cable {
    pub tag: TagSlot,
    pub cable_type: Option<StyleRef>,                  // a cable type (§8)
    pub type_text, pub manufacturer, pub part_number, pub description: Option<Text<64>>,
    pub cores: Vec<Core { label: Text<16>, colours: Vec<ConductorColour>, wire: Option<WireId>, unused: bool }>,
    pub shield: Shield,
    pub length: Option<Metres>,
    pub ends: [CableEnd { location: Option<LocationId>, function: Option<FunctionId>, gland: Option<Text<16>> }; 2],
    pub rating: CircuitInputs, pub load: Option<Amps>, pub protected_by: Option<DeviceId>,
}
```

`Bom`, `Schematic`, `Circuit` (`rating()`, `volt_drop()`). Core numbers are labels, not colours.

### 4.6 Colours and wire types

**One shared primitive for wires and cable cores: `ConductorColour`** (owner, 2026-11-17).

```rust
// ecad-model/src/colour.rs
/// An IEC 60757 base colour of a conductor. The variant is the identity; its forms are data.
pub enum ConductorColour { Black, Brown, Red, Orange, Yellow, Green, Blue, Violet, Grey, White, Pink, Gold, Turquoise, Silver }
pub struct ColourForms { pub name: &'static str, pub code2: &'static str, pub code3: &'static str, pub hex: Rgb }
impl ConductorColour {
    pub fn forms(self) -> &'static ColourForms;          // read once from the built-in TOML
    pub fn shown(self, words: ColourWords) -> &'static str;
}
pub enum ColourWords { Code2, Code3, Name }              // the project setting (today's `wiring.colour_style`)
```

- **Every form is data**, in `ecad-model/defaults/colours.toml` (embedded, 066's pattern), one table
  per colour, our own data:

  | Colour | Name | 2-letter | 3-letter | Display hex |
  | --- | --- | --- | --- | --- |
  | Black | Black | BK | BLK | `#000000` |
  | Brown | Brown | BN | BRN | `#8B4513` |
  | Red | Red | RD | RED | `#E00000` |
  | Orange | Orange | OG | ORG | `#FF8C00` |
  | Yellow | Yellow | YE | YEL | `#FFD700` |
  | Green | Green | GN | GRN | `#008000` |
  | Blue | Blue | BU | BLU | `#0000FF` |
  | Violet | Violet | VT | VIO | `#8A2BE2` |
  | Grey | Grey | GY | GRY | `#808080` |
  | White | White | WH | WHT | `#FFFFFF` |
  | Pink | Pink | PK | PNK | `#FF69B4` |
  | Gold | Gold | GD | GLD | `#D4AF37` |
  | Turquoise | Turquoise | TQ | TRQ | `#40E0D0` |
  | Silver | Silver | SR | SLV | `#C0C0C0` |

  The 2-letter codes are IEC 60757's; the 3-letter codes, names and hex values are our choice. The
  hex is for display only (swatches in the wire and cable panels, outlined so black and white show
  on either theme); drawings print the words, not the colour. A test checks every variant has one
  table and every table one variant, and that the codes are unique.
- **Stored word: the 2-letter code** (`BU`); reading also accepts the 3-letter code and the name,
  any case, so a person can type any form. The `wiring.colour_style` setting (`code2`, `code3`,
  `name`; today `iec` and `three-letter`) chooses which form drawings, labels and reports show.
- A wire's or a core's colour is an **ordered list** of base colours: green/yellow is
  `[Green, Yellow]`, written `GN/YE`, never a colour of its own. Wire types and cable types use the
  same list.
- An **indicator's** colour (a lamp, IEC 60073: red, yellow, green, blue, white) is a different
  thing and has its own small enum, `IndicatorColour`.
- **Wire types are a library with IDs**: today's `wire` style catalogue (050b's wire categories),
  renamed **wire types** and typed in lane G. The standard layer is the company library, the
  project layer the project's own; the built-ins (L, L1–L3, N, PE, 24 V, 0 V, signal, analogue,
  plus 230 V L/N) are editable defaults. Each has potential, size, colours, insulation (new),
  part number, manufacturer, description, allowance, rating data, line look. A wire refers to one
  and may override size and colours.
- **Cable types** (`cable` catalogue) use the same `ConductorColour` lists for their cores, as named
  core colour schemes (a scheme gives each core count its ordered list of core colours, or core
  numbers, which are labels). The default schemes are written as our own data in the defaults
  TOML; the owner's private reference informs the schemes and checks the codes (§12.4).

---

## 5. Page, folder, project, title block

### 5.1 Page

`Page { title: Text<80>, number: PageNo, folder, kind: PageKind, page_type, sheet, frame,
title_block, location: Option<LocationId>, function: Option<FunctionId>, description, display }`;
`PageKind { Schematic, Panel, Generated { what: GenKind, strip: Option<DeviceId> }, StripPage {
strip, form }, PlcDetail { module } }`. Place on a page: `Place { page: PageNo, column:
PageColumn, row: PageRow }`.

**Deviations in lane E (overseer, 2026-11-17).** The title is `Text<200>`, today's limit, not 80.
`PageKind` is `{ Schematic, Generated(GenKind), TerminalStrip { strip } }`: the one strip lives on
the variant that needs it; there is no `Panel` (the page type `panel` says that), no `StripPage`
(the graphic or table form stays the registry's `terminal.plan.style`, which has a project level)
and no `PlcDetail` (no such page exists yet). `page.type` (page level only) is
`Page.page_type`; `print.frame.style` (standard, project and page levels) stays a display value,
so there is no `Page.frame`.

### 5.2 Folder

`Folder { name, parent, start: Option<PageNo> }`.

### 5.3 Project, and the location and function lists

```rust
pub struct Project {
    pub info: ProjectInfo,                                   // title, drawing number, customer, site, status, notes
    pub revisions: Revisions { columns, rows, pending },
    pub locations: Vec<Location>, pub functions: Vec<Function>,
    pub user_fields: Vec<UserFieldDef>,
    pub loose_parts: Vec<LoosePart>,                         // §6.4
    pub display: Properties,
}
pub struct Location { pub id: LocationId, pub code: Text<16>, pub tag: Option<Text<16>>, pub descriptions: Vec<Text<128>> }
// Function: the same shape.
```

The project holds the lists (code `L1`, `F1`…, tag, description 1, description 2, …); devices,
pages, boxes and cable ends refer by id, so renaming a code is one edit.

**Deviations in lane E (overseer, 2026-11-17).** The project's default location and function are
`Project.location`/`Project.function` (ids), a holder the design did not list. `Revisions` holds
only the rows (`Project.revisions: Vec<Revision>`): the table's columns, marker and custom column
width stay registry values, because they have a standard level (as `print.frame.style`). The info
has no `notes` other than the cover's (`cover.notes`). The project-level settings bag stays outside
`Project`: `Project` holds typed project data and changes through `ModifyProject`; the bag holds
project-level setting overrides and changes through `SetProperty` (one writer each). It is renamed
`Document.properties` → `Document.display` with the 073 crate rename, when no lane is in flight
(overseer, 2026-11-18). The "Symbol tied to multiple
locations" check is lane F's (it needs boxes and the devices' fixed flags).

### 5.4 Title block

Typed per 055c's elements; strictness: `Binding` an enum, no `BlockItem::Unknown`, no
`StyleBody::Raw`; revision columns bind to `RevisionColumn`.

---

## 6. Not devices (lane F)

### 6.1 Junctions

Not stored; drawn (§4.4). Every `kind = junction` component goes.

### 6.2 Drawing aids

`Aid { Continuation { name, caption }, Box { location: Option<LocationId>, function:
Option<FunctionId>, panel_temperature: Option<Celsius> } }` in `Document.aids`, drawn by a
representation (`Subject::Aid`); a continuation's connection point has `TerminalOwner::Aid`.
Ops `CreateAid`, `DeleteAid`, `ModifyAid` (whole aid recorded; overseer, 2026-11-20); stored in
bucketed `model:aid:` chunks; invariant I25 (a box's panel temperature is a number).

### 6.3 Location and function: stored on the device, with `fixed`

- `Device { location: Place<LocationId> }` (`Unset`, `Manual(id)`, `Proposed(id)`: the state holds the entry, so a state without its value cannot be written; 0.2.3 S6 #3), and the same for function. **The
  action that sets the value decides `fixed`:** typing a location, or adding a location box around
  the device, sets `fixed = true`; inheriting from the page leaves `fixed = false`.
- **A box overwrites only when `fixed` is false.** Moving a device's symbol into a box, drawing a
  box, reshaping it or changing its value writes the location of every unfixed device inside it, in
  the same transaction.
- **A fixed device inside a box with a different location** is a check: the error marker goes on
  **every placed symbol of that device**, with one message, **"Symbol tied to multiple
  locations"**, and the person steps through the marked symbols. The same for function.
- **No box id is stored** on the device.

### 6.4 Loose parts

Panel-only items (trunking, DIN rail, ties, crimps) are not devices: `Project.loose_parts:
Vec<LoosePart { id, description: Text<200>, part_number, manufacturer, article_number, qty: Qty,
unit: ItemUnit, location, function }>`, implementing `Bom`, out of the device store. In 0.2.4 a
loose part becomes a layout item (`Bom` + `Layout`).

As built (F2a, overseer 2026-11-21): kept as aids are, in `Document.loose_parts: BTreeMap<LoosePartId,
LoosePart>` (the id is the key, not a field); ops `CreateLoosePart`, `DeleteLoosePart`,
`ModifyLoosePart` (whole part recorded); bucketed `model:loose_part:` chunks; invariant I26 (a
quantity more than 0 and at most 9 999, whole for pieces). `Qty` is thousandths written as a
decimal text (`"3.6"`); the description is `Text<200>` (`LooseDescription`: longer input is
refused, never cut). A device of kind `bom_item` is a read error. F2b: `bomitem` makes and edits
loose parts, the BOM reads them through `Bom` (`ecad_model::loose_part::Bom`, implemented by
`LoosePart` and by `Device` for its main part; a device's part lines are still properties), and
the BOM model keys its entries by `Owner::{Device, Loose}`.

---

## 7. Symbol and representation

`Definition` gains `device_type` and `part_role`; `TextSlot.text` → `SlotText`; `defaults` →
`SymbolDefaults`; `bridges` → `Vec<Vec<PointKey>>`. `Representation.role: Role` with
`Role::Part(PartRole)` (F3′); `Subject` gains `Aid` and `Route`.

---

## 8. Style records (lane G, now)

Each catalogue becomes a struct inside `StyleBody`, with "inherit" as `Option`: `TextStyle`,
`WireType` (§4.6), `CableType` (core colour schemes, shield, gland, outer diameter), `PageType`,
`FrameStyle`, `LocationStyle`, `RatingTable`. The `Field` tables in `styles/*` become the editor's
description of each struct's fields (labels, ranges), not the storage.

---

## 9. Foundation (lanes A1, A2)

### 9.1 Words and enums

`words::flag` (yes/true/1/on, no/false/0/off), `flag_word`, `is_digits`, `uint`, `uint_canonical`;
`trait Word` through strum, one macro for serde and schema; every choice list an enum, the on/off
switches `Kind::Flag` (F4); `StyleCatalogue`, `Side`, `TextKind`, `Shown`, `StarterKey` (stays in
electrical), `SymbolKey`, `SymbolName`, `cable_prefix` (F5).

### 9.2 What stays a property, and `PropKey`

Only display choices (`text.<kind>.style`, `text.<rep>.<kind>.show`) and defaults-registry values
stored per object. `keys.rs`: `PropKey`, `KEYS`, `FAMILIES`, `RESERVED` (every moved key: a bag
holding one is a read error), `PropsExt`, `Area` (imported by R9). During lanes B–G, `KEYS` also
holds the not-yet-moved keys; each lane moves its keys to `RESERVED`.

### 9.3 Units

`Mm`, `Metres`, `Radians`, `Degrees`, `Amps`, `MilliAmps`, `KiloAmps`, `Volts`, `Watts`, `Mm2`,
`Celsius`, `Percent`, `Seconds`; `RatedVoltage { volts, current: Ac | Dc }`; `PageColumn`,
`PageRow`, `PageNo`. Commands take degrees (F6); stored angles `Radians` (F7).

### 9.4 Validated text: `Text<N>`

```rust
/// Text a person typed: trimmed, 1..=N characters, one line, no control, hidden or bidi characters.
pub struct Text<const N: usize>(Box<str>);
impl<const N: usize> Text<N> { pub fn new(s: &str) -> Result<Self, TextError>; pub fn as_str(&self) -> &str; }
pub enum TextError { Empty, TooLong { max: usize }, Hidden { at: usize, char: char }, Lines }
pub struct LongText<const N: usize>(Box<str>);   // the same, line breaks allowed (notes, captions)
```

- The hidden set is today's `cables::is_hidden` (control characters, soft hyphen, the Arabic letter
  mark, zero-width and joiner characters, the bidi embeddings, overrides and isolates, word joiners,
  the byte-order mark, interlinear annotation marks), moved into `ecad_model::words` as the one
  rule.
- `Deserialize` validates: hidden characters in a file are a read error (strict). Commands refuse
  them with `TextError`'s words.
- Every free-text field in every stored struct is `Text<N>` or `LongText<N>` with its bound from
  today's checks (64 for parts, 24 for wire values, 16 for codes, …). A raw `String` field in a
  stored struct fails guard G13.

### 9.5 Strict reading (F11)

Typed fields with an unknown word, field or invalid text: the record is damaged, the project opens
read-only, the file is kept. Words in display values: `words_check` on open,
`ReadOnly::UnknownWords`. The inspector's Raw and Readable views show both (A15).

---

## 10. With lane 070 and with R9

**070.** Its params structs take the types: `Degrees`, `StyleCatalogue`, `SymbolKey`, `Side`,
`TextKind`, `Shown`, `Mm`, `Metres`, `Text<N>` for every text input, `LocationId`/`FunctionId` (or
the code, resolved to the id), `Vec<ConductorColour>`. Commands build `Modify*` ops. `setprop`'s key stays a
string and routes moved keys to fields.

**R9 (design 064).** `Effect::Device` (`Create`/`Delete`/`ModifyDevice`; a type change is a
`ModifyDevice`), `Effect::Wire`, `Effect::Cable` (no `Member`), `Effect::Route`, `Effect::Page`
from `ModifyPage` (no pair folding), `Effect::Project`, `Effect::Aid`, `Effect::DeviceLink`;
`Effect::Prop` only for display keys; `Effect::Style` with typed bodies. Views route by matching
fields and variants with no `_`.

---

## 11. Format and op changes

**Approved (overseer, 2026-11-16 and 2026-11-17): F1–F30** as written in the previous revision,
including the `Device`/`Wire` renames, the whole-struct `Modify*` ops, the removal of
`AddCableMember`/`RemoveCableMember`, `AddDeviceLink`/`RemoveDeviceLink`, and F30 (typed style
records, with the owner's answer on styles). F12 superseded by F13. Condition on every new op and
field: the inspector's Raw and Readable views show it (A15).

**Revised by the owner's answers: approved (overseer, 2026-11-17): F15′, F19′, F25′, F27′, F28′**,
carrying out the owner's answers (the twelve groups, the PLC station with a CPU or Coupler head,
ordered wires instead of junctions, the boxes' fixed flag, the loose parts list).

| # | Was | Now |
| --- | --- | --- |
| F15′ | 14 device groups | 12 groups (§2.2): Heater and Solenoid merged into `Load`, Relay and Switch into `Switching`, Lamp → `Indicator` |
| F19′ | a CPU card becomes a rack device plus a CPU module | a CPU or head-module card becomes a **station** whose `head` is `Cpu` or `Coupler`; no CPU module; `PlcRack { network, address, head, address_style }`; module functions per F41 (as refined by the owner's answer of 2026-11-17) |
| F25′ | `Attach::Wire` (a wire end on another wire) | **withdrawn**; replaced by F31–F32. Junction components are still removed |
| F27′ | boxes write location and `placed_in` | boxes write location and function only on devices with `fixed = false`; no box id stored (F33) |
| F28′ | loose parts "per the owner's call" | `Project.loose_parts`, implementing `Bom` (§6.4) |

**New: approved (overseer, 2026-11-17): F31–F37 as written**: routes as their own object with
`AddRoute`/`RemoveRoute`/`ModifyRoute`; ordered wires with `AttachEnd` removed; the location and
function lists with the fixed flags; `Text<N>`/`LongText<N>` validated on read; `ConductorColour`
with its forms in `ecad-model/defaults/colours.toml`; wire types; the default core colour schemes
as our own data. Condition (A15, hard rule): the inspector's Raw and Readable views show every new
op and record (`AddRoute`, `RemoveRoute`, `ModifyRoute`, the route, wire, location, function,
colour and wire-type records), on a normal project and on one opened read-only.

| # | Change | Op or on disk | Lane |
| --- | --- | --- | --- |
| F31 | **Routes**: `Document.routes: BTreeMap<RouteId, Route>`, `RouteEnd`, `Subject::Route`; ops **`AddRoute`, `RemoveRoute`, `ModifyRoute`**. The drawing of today's conductors moves to routes | new object, three new ops | F |
| F32 | **Ordered wires**: `Wire { net_order, ends: [TerminalId; 2], order_fixed, … }` between two connection points of one net; `AttachEnd` goes (ends are set by `CreateWire`/`ModifyWire`) | record shape; one op removed | F |
| F33 | `Device.location`, `function` (`Place<LocationId>`, `Place<FunctionId>`: `Unset`, `Manual(id)`, `Proposed(id)`); `Location`/`Function` entries with code, tag and descriptions; devices, pages, boxes and cable ends refer by `LocationId`/`FunctionId` | record shape | E (lists), B (device fields), F (boxes) |
| F34 | `Text<N>` / `LongText<N>` for every free-text field: a file with a hidden or bidi character, or an over-long text, is a read error | validation on read | A1 (type), each lane for its fields |
| F35 | `ConductorColour` (§4.6), one primitive for wires and cable cores, with every form (name, 2-letter, 3-letter, display hex) as built-in data in `ecad-model/defaults/colours.toml`; wire and core colours as ordered lists stored by 2-letter code (`GNYE` → `GN/YE`); `wiring.colour_style` words become `code2`, `code3`, `name` (were `iec`, `three-letter`); an indicator's colour is a separate `IndicatorColour` | **words change** (`GNYE` becomes a two-colour list; the setting's words) and built-in data | A1 (type and data), D, G |
| F36 | `Wire.wire_type` (today's `wire.category`), `size`, `colours` as overrides; wire types gain `insulation`; the `wire` catalogue's user-facing name becomes **wire types** | record shape, one new field | D, G |
| F37 | Default cable-type core colour schemes and the 230 V L/N wire types in the defaults TOML | built-in data | G |

**From the owner's answers to the four calls: approved (owner, 2026-11-17): F38–F42.** A15 applies
to each (the inspector shows every new field).

| # | Change | Op or on disk | Lane |
| --- | --- | --- | --- |
| F38 | **Conductors accepted per point**: `TerminalDef.accepts` in every symbol and `Terminal.accepts` on every connection point (default 2) | new field on definitions and connection points | B (field), F (the check and the star rule) |
| F39 | **`net: NetId` and `conductor: ConductorId` on every wire**, both stable (§4.2) | new fields | F |
| F40 | **Wire-number setting** `wiring.number_by`: `net` (per net, potential numbering, today's behaviour) or `conductor` | new settings field | F |
| F41 | **`ModuleFunction`** as an enum with data (§3): `DigitalIn`, `DigitalOut`, `AnalogIn`, `AnalogOut`, `Mixed`, `Position`, `Drive`, `Communication`, `Measurement`, `SafetyLogic`, `System`; station head `Cpu` or `Coupler`; `Signal` per channel group | record shape | C |
| F42 | **`ModuleAttributes`**: safety-rated, intrinsically safe (Ex i), calibration certificate | new fields | C |

---

## 12. Lanes, order, and what R9 needs

### 12.1 The lanes

| Lane | Content | Needs first | Tester |
| --- | --- | --- | --- |
| **A1** Foundation types | `words`, `units`, `Text<N>`, `ConductorColour` with its forms data, `keys` skeleton, `Word` + strum, `StyleCatalogue`, `Side`, page places, the strict read path, guards as ratchets | nothing: additive, **starts now**; its open-report step after 070's ecad-file batch | yes |
| **A2** Foundation adoption | callers onto words, keys, units, `Choice<E>`, `Kind::Flag`, typed `SetStyle`, `TextKind`/`Shown`, `StarterKey`, `SymbolKey`, `cable_prefix`, degrees, `Radians` | A1, **070 and 073** | no |
| **B** Device | device struct and fields, 12 groups (minus PLC), traits, `PointKey`, roles, symbol type, part lines, user fields, device links, predicates gone | A2 | yes |
| **C** PLC | station, modules, channels | B step 1 merged | yes |
| **D** Wire type data, cable | `Wire` fields (size, colours, type, rating), `Cable`, `Circuit`, colours | A2 | yes |
| **E** Page, project, lists, title block | `Page`, `PageKind`, `Project`, location and function lists, revisions, title block strictness | A2 | yes |
| **F** Routes, wire order, aids | routes, ordered wires, junction drawing, continuations and boxes as aids, `fixed` location, loose parts | B and D merged | yes (movement, `translate.rs`) |
| **G** Styles | one struct per catalogue, wire types, cable types and their default colour schemes | A2 | no |

### 12.2 Order

```
A1 (now) ─────────────┐
070 ──► 073 ──► A2 ───┼──► B ──┬──► F ──► end gate
                      │        └──► C (after B step 1)
                      ├──► D ──┘ (F also waits for D)
                      ├──► E
                      └──► G
```

B, D, E and G run in parallel after A2; their files don't overlap except `rating/` (split by
module first: B protection, D circuits) and `PROP_RULES` (each lane its own `cmd/props/<lane>.rs`).
D and G share the wire-type fields: G owns the `WireType` struct, D the `Wire` struct that refers to
it; D's step on overrides waits for G's wire-type step.

### 12.3 What R9 needs

R9 starts after **A1 + A2** (project split, `Effect` skeleton, `Area`). Its feeds wait for **B**;
`FrameView` and `PagesView` for **E**; `ConnectorsView` for **B**; `RefsView` for **D** and **F**
(continuations); `OutputsView` for **B**, **C**, **D**; `ChecksView` for **B**–**F**;
`GeneratedView` for **E**; style effects for **G**. R9's worker checks the lane table before each
view step and never builds a view on property routing a lane is removing.

### 12.4 The private colour reference

The default core colour schemes come from a third-party reference in the owner's private folder
(git-ignored; the overseer gives the worker its path). It is **never copied into the repo, quoted,
or named** (nor its tables) in any committed file. Lane G writes the schemes as our own data: the
colour sequences as facts in our TOML, in our own words, with no layout or text from the source.

---

## 13. Guards (all at 0, no allowlist)

| # | Guard | Lane |
| --- | --- | --- |
| G1 | a string literal in key position that is a registered or reserved key | A1 (B–G lower it) |
| G2 | boolean words compared outside `words.rs` | A1 |
| G3 | `.all(…is_ascii_digit…)` outside `words.rs` | A1 |
| G4 | a string-literal match or comparison on a word of any `Word` enum | A1 (all lanes lower it) |
| G5 | `#![deny(clippy::wildcard_enum_match_arm)]` on the type modules and `symbols.rs`, `behaviour.rs` | A1–G |
| G6 | the ten predicates, `body_type`, `of_body`, device `kind_of`, `rep_type` | B, C, F |
| G7 | angle conversions outside `units.rs`; unit-less angle, `_mm`, ratings `f64` | A2 |
| G8 | `Kind::Choice(&[`; `.word() == "` | A2 |
| G9 | `catalogue` or `role` as `&str`/`String` | A2, B |
| G10 | `Unknown(`, `Stored<`, raw bodies, lenient word deserialisers | A1, E |
| G11 | a bag read or `SetProperty` for a key not in `KEYS` | B–G |
| G12 | parsing a terminal key text | B, C |
| G13 | a `String` or `&str` field in a stored struct (except ids and `Text<N>`'s inside) | A1 (all lanes lower it) |
| G14 | `is_hidden` or a hand-written hidden-character set outside `words.rs` | A1 |

---

## 14. Acceptance

| # | Test | Lane |
| --- | --- | --- |
| A1 | word readers, `Text<N>` (every hidden and bidi character in the set refused; lengths; lines) | A1 |
| A2 | every `Word` enum and struct-enum round-trips; schema equals words | A1, each lane |
| A3 | keys registered, reserved keys refused | A1, each lane |
| A4 | unknown word, unknown field, invalid text → read-only, named, file unchanged | A1, each lane |
| A5 | commands refuse unknown words and invalid text; MCP contract tests (brief 011) | A2, each lane |
| A6 | degrees in, radians stored; ratings typed | A2 |
| A7 | groups match today's predicates on the test projects and the medium bench project, before the predicates go | B, C |
| A8 | the symbol declares the type; joining keeps it; a swap re-reads it | B, C |
| A9 | every new op in `Op::ALL`, inverse, atomic, invariant-checked; `setprop` on a moved key records the `Modify*` op | B–G |
| A10 | methods, not recalculation: 057 ratings, PLC address blocks, power demand unchanged through the traits | B, C, D |
| A11 | a CPU card becomes a station with a CPU head, a head module a station with a coupler; addresses, I/O list and overview unchanged | C |
| A12 | wire order: a net of n points has n−1 wires in order; adding, removing, joining and splitting keep the order of the points that stay; a hand order is kept; redrawing routes without changing the points changes no wire; **Show wire order** draws the numbers; dots and T indicators where expected | F |
| A13 | location `fixed`: typing or a box sets fixed; the page doesn't; a box overwrites only unfixed devices; a fixed device in a different box puts "Symbol tied to multiple locations" on every placed symbol of the device | F |
| A14 | regenerated fixtures have the §11 shapes; no shims | each lane |
| A15 | the inspector's Raw and Readable views show every new struct, field and op, and a read-only project's offending record with its reason (hard rule) | each lane |
| A16 | every guard at 0 | end of F and G |
| A17 | drawing order: A–B then C onto the middle of A–B gives 1 = A–B, 2 = B–C (nearest end); a point accepting N > 2 fans out; a point with more conductors than it accepts is an error with the point named | F |
| A18 | `net` and `conductor` ids: stable across unrelated edits; a join keeps the larger net's id, a split gives the smaller part a new one; `wiring.number_by` switches every shown number between per-net and per-conductor in one edit | F |
| A19 | modules: each of today's PLC starters gives its `ModuleFunction` (§3 mapping); safety and Ex i are attributes; a communication module shows in the station overview | C |

Bench as before: `views.setprop_tag`, `views.move_symbol` ≤ +10%; a route move with no change of
points rewrites no wire; open ≤ +5%; op log ≤ +10%.

---

## 15. Out of scope

`Pt` and drawing-unit coordinates; footprints and the panel layout (0.2.4); the import step for
other tools' files; R7 and R9 themselves; CADCraft.

---

## 16. Settled (owner, 2026-11-16 and 2026-11-17)

Everything in Part I; device groups justified by fields and methods; PLC as station (CPU or
Coupler head), modules, channels; wires as an ordered list per net with dots and T indicators as
drawing aids; location and function lists with `fixed`; `Bom`, `Schematic`, `Layout` traits; loose
parts on the project; styles in lane G now; validated text everywhere; colours as ordered lists of one shared
`ConductorColour` (wires and cores; every form as data) with a display setting; wire types and cable types as libraries; the lane order.

---

## 17. Owner decisions (all answered, 2026-11-17)

1. **Wire order: drawing order.** Each new connection becomes the next wire; a route joining the
   middle of an existing one links from the nearest end (A–B, then C onto A–B: 1 = A–B, 2 = B–C).
   The person can reorder; "shortest total wire" is a later button (§4.4).
2. **Chains versus stars: data.** Every connection point stores how many conductors it accepts,
   from its symbol or part (normally 2; PE bars and earth studs N). The order chains by default and
   fans out only at points that accept more than two. A point with more conductors than it accepts
   is an error. Every wire stores its net and its own conductor id, and a project setting chooses
   whether the visible wire number counts per net or per conductor (§4.2, §4.4).
3. **Switching:** one group, with the operator (coil, hand, timer, …) as a choice inside it (§2.2).
4. **PLC modules:** the function is an enum with data in generic names (digital in and out, analog
   in and out, mixed, position, drive, communication, measurement, safety logic, system); the
   station head is CPU or coupler; safety-rated, intrinsically safe and a calibration certificate
   are attributes; communication modules show in the station overview. A broad fieldbus terminal
   range maps onto one function plus attributes each (§3).
