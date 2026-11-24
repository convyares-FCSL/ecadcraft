# 072 Typed object model: lane A1 (foundation types)

What changed for someone using the app:

- **A project holding a word this version does not know opens read-only.** A setting stored as
  text but meant to be one of a fixed set of words (for example `text.tag.show`, which is `on`,
  `off` or `first`) can hold something else if the file was edited by hand or made by another
  program. Before, ECAD quietly read such a value as the default. Now the project opens
  read-only, and the open message names the word, the setting and where it is (the project, a
  page, a device, a wire, a cable, the settings or the standard). The file is kept as it is.
  Save is refused; **Save As** still saves a copy.
- A yes/no property in a file (`numbering.provisional`, `wire_number_fixed`; a tag's state is part of the `tag` field: `{"state": "automatic"|"fixed"|"provisional", "tag": "-K1"}`) must hold
  exactly `yes` or `no`. Commands still accept `true`, `1` and `on` (and `false`, `0`, `off`) as
  you type them, and always write `yes` or `no`.
- Nothing else changes in how the app looks or behaves.

## Try it

There is no click-only way to make such a file: it needs a file edited outside ECAD. The headless
test `lanes::lane_072a::unknown_words_open_read_only` (ecad-engine) makes one and checks the
message, the refused save and the unchanged file. The inspector still shows such a file in its
Raw and Readable views (`lanes::lane_072a::a_project_with_an_unknown_word_is_still_shown`,
ecad-inspector).

Run them from `ecad/`:

```sh
cargo test -p ecad-engine --test all lane_072a
cargo test -p ecad-inspector --test all lane_072a
```

These are automated checks only; nobody has tried this by hand yet.

## Lane A2: switches are yes/no

Every on/off setting is now a yes/no switch, in the defaults registry, the page and frame styles
and the project file:

- The registry's on/off entries (text Shown, wire join, the cover blocks, PLC captions, frame and
  channel texts, the BOM article column) and the page and frame style fields `zones`, `checks`,
  `grid_labels` and `centre_marks` are stored as `yes` or `no`. A tag's Shown is `yes`, `no` or
  `first`.
- Commands and boxes still take `on`/`off`, `true`/`false` and `1`/`0` as you type them, and
  write `yes` or `no`. `defaults` and `textstyle` show `yes`/`no`; an entry's kind reads `flag`.
- A project file holding `on` or `off` for one of these settings opens read-only and names the
  setting, as for any unknown word. A style field holding `on`/`off` reads as unset (style
  bodies are checked strictly in lane G).

### Try it

1. Open a project, open the Text section of the project settings and set a kind's Shown to
   Hidden: it is hidden on the drawing as before.
2. In the inspector's Layout tab, click a symbol's tag Shown switch twice: hidden, then shown.
3. In Settings, Pages, switch a page style's zones off and on: the sheet preview follows.

These steps have been checked only by the automated tests (`text_ui`, `standards_ui`, `lane_054`);
nobody has tried them by hand yet.

## Lane A2: tag prefixes say which symbol they are for

The project's own tag prefixes are stored by symbol, `starter:coil` for a built-in symbol and
`project:valve-x` for one made in the project, and the cable prefix is a field of its own
(`cable_prefix`). A prefix is 1 to 10 upper-case letters.

- A project that gave one of its own symbols a prefix (`setdefault
  {"key": "symbol.<key>.prefix", "value": "BL"}`) now opens writable; before, it opened
  read-only.
- A file whose prefix key is neither form, names a built-in symbol that does not exist, or holds
  a prefix that is not letters opens read-only and names the setting.
- The inspector shows the new keys and the cable prefix in its Readable view.

### Try it

1. Open a project and make a symbol in the symbol builder with the key `float1`.
2. In the command line run `setdefault {"key": "symbol.float1.prefix", "value": "BL"}`, then place
   the symbol: its tag starts `-BL`.
3. Save, close and open the project again: it opens writable (no read-only notice).

The headless test `lanes::lane_072a::a_project_symbol_with_its_own_prefix_opens_writable`
(ecad-engine) runs these steps; nobody has tried them by hand yet.

## Lane B: devices have a type

What changed for someone using the app:

- **Every device stores what it is**: power supply, protection, switching, sensor, motor, load,
  indicator, terminal strip, connector, PLC station, PLC module or general. The symbol it was
  first placed from decides it; placing another symbol onto the same tag keeps it.
- In files, commands and the agent's tools a device is now a `device`: the ops are
  `CreateDevice`, `DeleteDevice`, `SetDeviceDefinition` and the new `ModifyDevice`; `setprop`,
  `setlocation`, `conflicts` and `partline` take several devices as `devices` (was
  `components`), and `ecad_project` lists them under `devices`. Projects saved by an earlier
  0.3.0-dev build do not open.
- Nothing changes in how the app looks or behaves yet.

### Try it

1. Start the app, place a **Contactor / relay coil** and an **MCB**.
2. Open the inspector (`cargo run -p ecad-inspector -- <file>` after saving) and look at the
   project's devices: the coil's device shows `"device_type": {"switching": {}}`, the MCB's
   `{"protection": {}}`.

### Lane B, step 2: a device's text values are its own fields

A device's tag, description, part number, maker, article number and type text are fields of the
device, no longer entries in its list of properties. Commands and panels take the same keys as
before (`setprop {"key": "maker"}`); what changes:

- A part number, maker, article number or type text is at most 64 characters (was 200); a
  description at most 256. A longer value is refused with its limit.
- Each change records one `ModifyDevice` (the inspector shows the device before and after).
- A file whose device holds one of these keys in its properties opens read-only.

To try it: select a device, type a maker and a part number in the Properties panel, then undo
and redo: both come back as one step each. The headless tests below cover it; nobody has tried
it by hand yet.

### Lane B, step 2c: a device's location and function, and whether they are fixed

A device's location and function are fields that point at the project's lists, each with a
**fixed** flag (design 072 §6.3). What sets the value decides the flag:

- Typing a location or function (Properties panel, `setlocation`, `setfunction`, `setprop`) sets
  it **fixed**.
- Taking a box's, page's or the project's value (a proposal, or **Move** in the conflicts card)
  sets it **not fixed**.
- A file whose device holds `location` or `function` among its properties opens read-only.
- Nothing looks different yet; lane F's box rule (a box rewrites only unfixed devices) uses the
  flag.

To try it: type a location on a device, then draw a location box around another, unset device;
open the inspector's devices view: the first shows `"location_state": "manual"`, the second `"proposed"` (S6 #3; the readable view derives the word from the location's own state).
Nobody has tried it by hand yet.

### Lane B, step 3: a connector's own data

A connector stores its family (from the symbol it was first placed from: a pin's is Plain), its
pin count, its typed pin names, each half's description, maker and part number, and the Checks
rows it ignores, as data of its type. The Properties panel and `setprop` keys are the same as
before (`pins`, `pin_names`, `half.s.maker`, `ignore.connector-family`); what changes:

- A half's field is one of `description`, `maker`, `part_number` (any other word is refused), and
  `ignore.` takes `connector-family` or `connector-size` only.
- Each change records one `ModifyDevice`; the inspector's log shows the connector's data.
- A file whose device holds one of these keys among its properties opens read-only.

To try it: place a D-sub 9 socket, set its Pins to 9 and a half's maker in the Properties panel,
then undo and redo. Nobody has tried it by hand yet.

The headless tests (automated, not tried by hand):

```sh
cargo test -p ecad-model --test all lane_072b
cargo test -p ecad-engine --test all lane_072b
cargo test -p ecad-inspector --test all lane_072b
cargo test -p ecad-mcp --test all lane_072b
```

### Lane B, step 3: ratings are typed (Protection first)

A device's `rating` is no longer free text. A protective device (fuse, MCB, RCD, RCBO, fuse bank)
stores its rating as typed fields: rated current, curve, breaking capacity, poles, residual
current, and a fuse bank one fuse rating per output. What you type is read into those fields and
shown back from them:

- `B16` on an MCB reads as **curve B, 16 A**; the Properties panel, the status line and the
  command's answer all say what was read.
- Ambiguous text is refused with a question, never guessed: a bare `16` on an MCB asks for the
  curve; a bare `30` on an RCD asks whether it is A or mA. A value the device does not have is
  refused (a curve on a fuse, a residual current on an MCB).
- Text that is no rating at all (`see datasheet`) is kept as the device's rating note, and the
  device gets the Checks warning **no-rating**, because the checks read only the typed fields.
- A fuse bank takes `rating.<n>` per fuse (`rating.3` `6.3 A T`); its own `rating` is refused.
- Other groups (power supplies) keep what you type
  as the note until their own step; a symbol's default rating is read the same way when it is
  placed, and a symbol whose default rating does not read for its group cannot be saved.
- A file whose device holds `rating` among its properties, or a protective device holding
  `rating.<n>`, opens read-only.
- From the nameplate, every kind also takes its **Rated voltage** with AC or DC (`230 V AC`,
  `24 V DC`; a voltage without either is asked about): a device rated for 230 V AC must not
  protect a 400 V or a DC circuit. A fuse (a fuse terminal, each fuse of a bank) takes its **Fuse
  size** (`5x20`, `6.3x32`, `10x38`, `14x51`, `22x58`, `NH000` to `NH4`), which picks the holder
  and the spare. An RCD or RCBO takes its **Residual type** after `type` (`type AC`, `A`, `F` or
  `B`: a drive or a DC load needs F or B). An RCD takes its **Conditional short-circuit current**
  after `Inc` (`Inc 10 kA`): it breaks no fault itself, so its fault rating is with the device
  before it. `40A 30mA type A Inc 10 kA 4P 400VAC` on an RCD reads as **40 A, 30 mA, type A, Inc
  10 kA, 4P, 400 V AC**. Each is also a row of its own (`rating.voltage`, `rating.size`,
  `rating.residual_type`, `rating.conditional_short_circuit`). Refused: a size on a breaker, a
  residual type on a fuse or an MCB, `Inc` on anything but an RCD.

To try it: place an MCB, type `B16` as its rating in the Properties panel (shown as
`curve B, 16 A`), then `16` (refused, asks for the curve), then `see datasheet` (kept as a note,
and Checks shows "no rating"). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector, and
`rating::reader` in ecad-electrical; the four nameplate fields above through
`devices::rating_fields` in ecad-engine (automated, not tried by hand).

#### The rating line on the drawing

The rating a symbol draws is made from the typed fields by its group's **rating line template**:
which fields show, and in what order. Each group has one (Standards ▸ Defaults,
`rating.template.<group>`, at the standard or the project level): a list of field words,
separated by commas. Built in, it shows the nameplate fields and the ones that say how the device
is used here (a motor's starting, a lamp's flashing); a field taken from a datasheet is kept but
shows only when a template adds it. A device with no typed rating draws its note.

| Group | Field words (built-in order) |
| --- | --- |
| `power_supply` | `voltages`, `current`, `power` (a power supply's, in W), `apparent_power` (a transformer's, in VA), `frequency` and `impedance_voltage` (a transformer's, `uk 4 %`) |
| `protection` | `curve`, `current`, `class`, `size`, `residual`, `residual_type`, `breaking`, `conditional_short_circuit`, `poles`, `voltage`, `outputs` |
| `switching` | `control`, `ac3_current`, `ac3_power`, `operational_voltage`, `thermal_current`, `insulation_voltage` |
| `sensor` | `supply` |
| `motor` | `power`, `voltage`, `current`, `frequency`, `speed`, `power_factor`, `efficiency`, `efficiency_class`, `connection`, `duty`, `insulation_class`, `ip_rating`, `starting` |
| `load` | `power`, `voltage`, `current`, `phases` |
| `indicator` | `colour`, `signal`, `voltage`, `power` |

Datasheet fields a template may add: an indicator's `current` and `ip_rating`; a power supply's
`input_current` and `efficiency` (shown `eff 90 %`); a sensor's `output_current`,
`no_load_current` and `ip_rating`.

`setdefault {"key": "rating.template.motor", "level": "project", "value": "current, power"}` makes
every motor draw `15.2 A, 7.5 kW`. Typing a rating (`7.5 kW, 15.2 A`) still fills the fields once;
the Properties panel, answers and reports keep every field. A word the group has no field for, or
a word given twice, is refused.

Where it shows: a project symbol with a rating text slot draws it there; a motor or indicator
drawn by a symbol without text slots (the built-in `motor3`, `motor1` and `lamp`) draws it to the
right of the symbol however the symbol is turned, and its tag block keeps clear of it. Changing a template
redraws the lines at once, and undo puts them back.

To try it: place a `motor3`, type `7.5 kW 400 V 15.2 A 50 Hz DOL` as its rating in Properties, then
set "Rating line: motor" in Standards ▸ Defaults to `current, power`.

Tests: `devices::rating_line` in ecad-engine, including the redraw on a template change
(automated, not tried by hand).

#### One rating field at a time

Each field in the table above can also be set by itself, without retyping the whole rating:

- The Device tab of one device shows a row per field its kind has (**Rated power**, **Rated
  voltage**, **Starting**…), the nameplate and design fields first, then the datasheet's. Type a
  value in a row: `7.5 kW` or `7500` (a bare number is in the field's own unit: W, V, A, mA, kA,
  Hz, %), `24 V DC` for a voltage that needs AC or DC, an IP code by its two numerals (`IP65`,
  `IPX4`; a code with extra letters such as `IP20B` is refused), a word or its label for a choice
  (`star_delta` or `star-delta`). Empty clears it.
- A datasheet field is set only this way: typing or clearing the whole rating changes the
  nameplate and design fields and keeps the datasheet ones.
- `setprop {"component": …, "key": "rating.<word>", "value": …}` does the same (`rating.voltage`,
  `rating.power_factor`). A value that is not of the field's type is refused with the field's
  name (`Rated voltage takes a voltage, such as 400 V.`), as is one outside the group's range or a
  field this kind of device does not have (a transformer has no output current).
- A field holding several values in one part (a power supply's `voltages`, a switching device's
  `control`, a fuse bank's `outputs`) is shown read-only and changed through the whole rating
  (or `rating.<n>` per fuse of a bank).
- `deviceprops` lists them under `rating_fields`: each field's key, label, source (`nameplate`,
  `design`, `datasheet`), type, unit, choices and value, so an agent can see what it may set.

To try it: select a `motor3`, type `400` in its **Rated voltage** row (shown as `400 V`), then
`15 A` (refused). Nobody has tried it by hand yet.

Tests: `devices::rating_fields` in ecad-engine, `rating_fields_contracts` in ecad-mcp and
`objects::rating_fields` in ecad-model (automated, not tried by hand).

### Lane B, step 3: switching devices' ratings are typed

A switching device (relay, contactor, SSR, push button, selector) now says what operates it, from
its symbol: a **coil** (`coil`, `no`, `nc`, `co`, `power3`, `power4`), a **solid-state** input
(`ssrin`, `ssrout`) or a **hand** (`pbno`, `pbnc`, `sel`, `swco`). Its rating is typed fields: the
coil's or input's control voltage (`24 V DC`) and power (`3 W`), and the AC-3 rating: rated
current (`AC-3 9 A`) and motor power (`4 kW`).

- `24VDC AC-3 9A` on a coil reads as **coil 24 V DC, AC-3 9 A**; an SSR shows `input 24 V DC`.
- Asked, never guessed: `24 V` (AC or DC?), `9 A` (is it the AC-3 current? write `AC-3 9 A`), a
  bare `24` (which unit?).
- A push button or selector has no coil: a voltage on it is refused; it takes only the AC-3
  rating. An unrated push button or selector gets no **no-rating** warning; an unrated coil or
  SSR does.
- `24 V AC/DC`, `5 VA` and other text that does not read are kept as the note.
- From the nameplate, each read after its letters: the **Operational voltage** the AC-3 rating is
  given at (`Ue 400 V`), the **Thermal current** (`Ith 20 A`: a push button, selector or relay has
  no AC-3 value, only this) and the **Insulation voltage** (`Ui 690 V`: the highest circuit
  voltage it may switch or sit beside). `24 V DC AC-3 9 A 4 kW Ue 400 V Ith 20 A Ui 690 V` on a
  coil reads as **coil 24 V DC, AC-3 9 A, 4 kW, Ue 400 V, Ith 20 A, Ui 690 V**. Each is also a
  row of its own (`rating.operational_voltage`, `rating.thermal_current`,
  `rating.insulation_voltage`), on a push button too.

To try it: place a coil (Checks shows "no rating"), type `24VDC AC-3 9A` as its rating (shown as
`coil 24 V DC, AC-3 9 A`, and the warning goes), then `24 V` (refused, asks AC or DC). Place a
push button and type `24 V DC` (refused: no coil). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…switching…`), and `rating::reader::switching` in ecad-electrical; the three nameplate fields
above through `devices::rating_fields` in ecad-engine (automated, not tried by hand).

### Lane B, step 3: sensors' ratings are typed

A sensor now says what it senses and what it outputs, from its symbol: `prox3` is a **proximity**
switch with a **PNP** output; `floatno`/`floatnc` (**float**), `rollerno`/`rollernc` (**limit**)
and `interlockno`/`interlocknc` (**interlock**) are volt-free **contacts**; `tc` is a
**temperature** sensor with a **thermocouple** output. Only a PNP or NPN sensor is powered: its
rating is its supply voltage, a typed field.

- `24VDC` on a proximity switch reads as **24 V DC**. Asked, never guessed: `24 V` (AC or DC?), a
  bare `24` (which unit?). `10-30 V DC` and other text that does not read are kept as the note.
- A contact or a thermocouple has no supply: what you type on it (`250 V AC 5 A`, `K`) is kept as
  the note, and an unrated one gets no **no-rating** warning. An unrated proximity switch does.
- From its datasheet, set one field at a time: the **Output current** its output may drive
  (`rating.output_current`, `0.2 A`; a contact's too), a powered sensor's **No-load current**
  (`rating.no_load_current`, `10 mA`: refused on a contact or a thermocouple, which draw none)
  and its **IP rating** (`rating.ip_rating`, `IP67`). They show in the Device tab after the
  supply, on the rating line only when `rating.template.sensor` adds them, and in a BOM rating
  column.

To try it: place a proximity switch (Checks shows "no rating"), type `24VDC` as its rating (shown
as `24 V DC`, and the warning goes), then `24 V` (refused, asks AC or DC). Place a float switch:
no warning, and `250 V AC 5 A` is kept as typed. Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…sensor…`), and `rating::reader::sensor` in ecad-electrical; the datasheet fields through
`devices::rating_fields` in ecad-engine (automated, not tried by hand).

### Lane B, step 3: a strip's fuse position ratings are typed

A fuse position's rating (`setprop rating.<position>`, or the Rating cell of the terminal list)
is now the strip's own typed field: a fuse's rated current, class and breaking capacity, read by
the same reader as a fuse. `6,3A T` reads as **6.3 A, T**.

- Text that is no fuse rating (`see datasheet`, `B16`) is refused: a position has no note.
- A position that is not a fuse position is refused; clearing always works.
- Renumbering or relabelling a position moves its rating; erasing the position's last terminal
  removes it (with a note, `U` brings it back). **Make strip position** moves a fuse terminal
  device's typed rating onto the position; a rating kept only as a note is listed as lost.
- A project whose strip still holds `rating.<p>` as a property opens read-only (pre-v1, no
  conversion).

To try it: draw a FUSE fence across two wires, open the terminal list, type `6,3A T` in a
position's Rating (shown as `6.3 A, T`; the **no-fuse-rating** warning goes when every fuse
position is rated), then `see datasheet` (refused). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…strip…`), and `levels_contracts::relabel_moves_every_position_keyed_property` (automated, not
tried by hand).

### Lane B, step 3: motors' ratings are typed

A motor now says its phases, from its symbol: `motor1` is **single-phase**, `motor3`
**three-phase**. Its rating is its nameplate, in typed fields: rated power (shown in kW), rated
current, power factor (`cos φ 0.85`, also `pf 0.85`), efficiency (`eff 91 %`) and how it starts
(`DOL`, `star-delta` or `Y-D`, `soft start`, `VFD`).

- `7,5kW 15.2A Y-D` reads as **7.5 kW, 15.2 A, star-delta**; `370 W` as **0.37 kW**.
- Asked, never guessed: a bare `7.5` (kW or A?), `91 %` without `eff`. Other text
  (`see datasheet`) is kept as the note.
- The rest of the nameplate: **Rated speed** (`1450 rpm` or `1450 1/min`), **Efficiency class**
  (`IE1` to `IE5`, the legal minimum), **Connection** of the windings (`Y`, `Δ`, or `Y/Δ` for six
  terminals; also `star`, `delta`: star-delta starting needs a motor rated delta at the supply
  voltage; `Y-D` still means star-delta *starting*), **Duty** (`S1` to `S10`: an S2 or S3 motor
  must not run continuously), **Insulation class** (`class B`, `F` or `H`: a drive or a hot room
  needs F or above) and **IP rating** (`IP55`). `7.5 kW 400 V 15.2 A 50 Hz 1460 rpm IE3 Δ S1 class
  F IP55` reads as **7.5 kW, 400 V, 15.2 A, 50 Hz, 1460 rpm, IE3, Δ, S1, class F, IP55**. Each is
  also a row of its own (`rating.speed`, `rating.efficiency_class`, `rating.connection`,
  `rating.duty`, `rating.insulation_class`, `rating.ip_rating`).
- An unrated motor now gets the Checks warning **no-rating** (the demo's `-M01` too).

To try it: place a 3-phase motor (Checks shows "no rating"), type `7,5kW 15.2A Y-D` as its rating
(shown as `7.5 kW, 15.2 A, star-delta`, and the warning goes), then `7.5` (refused, asks kW or A).
Nobody has tried it by hand yet.

#### A motor's rated voltage and frequency, and its supply

A motor's nameplate also holds its **rated voltage** and **rated frequency**: `7.5 kW 400 V 15.2 A
50 Hz` reads as **7.5 kW, 400 V, 15.2 A, 50 Hz**. A DC voltage (`24 VDC`) is refused.

What feeds a motor is the **supply** typed on a row of the potentials table: its voltages exactly
as typed, never one worked out from another. `400/230 V 3~ 50 Hz` is a three-phase supply with
both its line-to-line and line-to-neutral voltages; `400 V 3~` has only the line-to-line one;
`230 V 1~ 60 Hz` is single-phase; `24 V DC` is DC. The phases or DC must be said. Nothing is read
from the potential's name: a row with no supply is not known. The app has no potentials editor
yet, so the supply is set with `wiresettings` (as text or as fields), and `wiresettings {}` shows
it:

```
wiresettings {"potentials": [{"name": "L1", "prefix": "L1-", "numbered": true,
  "supply": "400/230 V 3~ 50 Hz"}, ...]}
```

- The Device tab of one motor shows **Rated voltage**. Empty, on a net whose potential has a
  supply, it offers **Use 400 V** (hover: `From L1: 400/230 V 3~ 50 Hz`): a three-phase motor is
  offered the line-to-line voltage, a single-phase motor the line-to-neutral one. Nothing is
  filled in until the link is clicked. The row is the motor's `rating.voltage` field (`400 V`
  or `400`; empty clears it), like every rating field.
- Checks warns **supply-mismatch** when a motor does not fit its supply: a rated voltage further
  from the supply's than the project's **Motor voltage tolerance** (Standards ▸ Defaults,
  `rating.supply_tolerance`, 5 % unless changed), a rated frequency other than the
  supply's (when both are known), a three-phase motor on a single-phase supply, a motor on DC,
  or a single-phase motor on a three-phase supply typed without its line-to-neutral voltage
  (`supply line-to-neutral voltage unknown`). When the supplied potentials on a motor's nets
  disagree, nothing is compared.
- Why 5 %: a motor is built to run continuously within about ±5 % of its rated voltage (zone A
  of IEC 60034-1). A project that accepts the wider ±10 % band, or wants a tighter check, changes
  the tolerance.

To try it: give L1 the supply above, place `motor3`, draw a wire from one of its points and set its
potential to L1, select the motor (the Device tab offers **Use 400 V**), then type `230` as its
Rated voltage (Checks shows **supply-mismatch**). Nobody has tried it by hand yet.

Tests: `devices::supply` in ecad-engine and ecad-electrical, `connectivity::supply` in ecad-model,
`supply_contracts` in ecad-mcp (automated, not tried by hand).

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…motor…`), and `rating::reader::motor` in ecad-electrical (automated, not tried by hand).

### Lane B, step 3: loads' ratings are typed

Heaters and valves are **loads**. A load's rating is its nameplate, in typed fields: its rated
power, shown in W below 1 kW (`5 W`) and in kW from it (`3 kW`); its rated voltage with AC or DC
(a heater or a valve coil is chosen for its supply); its rated current (for protection and cable
sizing without guessing a power factor); and its phases, `1~` or `3~` (a three-phase heater is
wired and protected differently).

- `3000 W` reads as **3 kW**, `0,5 kW` as **500 W**; `9 kW 400VAC 13A 3ph` as **9 kW, 400 V AC,
  13 A, 3~**. Each is also a row of its own (`rating.voltage`, `rating.current`, `rating.phases`).
- Asked, never guessed: a bare `3` (which unit?), a voltage without AC or DC (`24 V`). Other text
  (`see datasheet`) is kept as the note.
- An unrated load now gets the Checks warning **no-rating**.

To try it: place a heater (Checks shows "no rating"), type `3000 W` as its rating (shown as `3 kW`,
and the warning goes), then `3` (refused, asks W or kW). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector (`…load…`),
and `rating::reader::load` in ecad-electrical (automated, not tried by hand).

### Lane B, step 3: indicators' ratings are typed

A lamp is an **indicator**. It is created **steady** with no colour. Its rating is its colour
(red, yellow, green, blue or white, in any IEC 60757 form: `red`, `RD`, `RED`), how it signals
(`steady`, `flashing`, `rotating`, `audible`, `meter`), its rated voltage with AC or DC, and its
power.

- `RD flashing 24VDC 2W` reads as **red, flashing, 24 V DC, 2 W**. Without a signal word the lamp
  keeps its own.
- Asked, never guessed: `24 V` (AC or DC?) and a bare `24` (V or W?). Refused: a current (`0.1 A`:
  the rated current is set by itself, below), and a colour that no indicator has (`black`). Other text (`LED`, `amber`) is kept as the note.
- An unrated lamp now gets the Checks warning **no-rating** (the demo's lamps too).
- From its datasheet, set one field at a time: its **Rated current** in mA (`rating.current`,
  `20 mA` or `0.02 A`, up to 10 A: what it draws from a PLC output or a supply) and its **IP
  rating** (`rating.ip_rating`, `IP65`). They show in the Device tab after the nameplate fields,
  on the rating line only when `rating.template.indicator` adds them, and in a BOM rating column.

To try it: place an indicator lamp (Checks shows "no rating"), type `RD 24VDC 2W` as its rating
(shown as `red, 24 V DC, 2 W`, and the warning goes), then `24 V` (refused, asks AC or DC).
Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…indicator…`), and `rating::reader::indicator` in ecad-electrical; the datasheet fields:
`devices::rating_fields::a_datasheet_field_is_kept_through_the_rating_and_drawn_when_the_template_adds_it`
in ecad-engine, `objects::rating_fields::an_ip_code_reads_its_two_numerals_and_refuses_the_rest` in
ecad-model and `rating_fields_contracts::a_datasheet_field_by_the_agent_is_saved_and_read_back` in
ecad-mcp (automated, not tried by hand).

### Lane B, step 3: power supplies' and transformers' ratings are typed

A `psu` symbol creates a **power supply** and a `trafo` symbol a **transformer**; the symbol
decides which, the rating never does. A power supply's rating is its input and output voltage
(each AC or DC), its output current and its output power. A transformer's is its two voltages
(always AC) and its power in **VA**. A transformer's ratio comes from its two voltages.

- `230VAC/24VDC 5A` (or `in 230 V AC out 24 V DC 5 A`) reads as **230 V AC / 24 V DC, 5 A**.
  `24 V DC` alone is a power supply's output. On a transformer `400/24 V 250 VA` reads as
  **400 V AC / 24 V AC, 250 VA**.
- Asked, never guessed: one AC voltage on a power supply, or one voltage on a transformer (input
  or output?), and a power-supply voltage without AC or DC. Refused: W or a current on a
  transformer (use VA), DC on a transformer, and VA on a power supply. Other text
  (`see datasheet`) is kept as the note.
- An unrated power supply or transformer now gets the Checks warning **no-rating** (the demo's
  -G01 too).
- A transformer's nameplate also gives its **Rated frequency** (`50 Hz`) and its **Impedance
  voltage** (`uk 4 %`, the percentage that sets the fault current on its secondary):
  `400/24 V 250 VA 50 Hz uk 4 %` reads as **400 V AC / 24 V AC, 250 VA, 50 Hz, uk 4 %**. A
  percentage without `uk` is asked about.
- From a power supply's datasheet, set one field at a time: its **Input current**
  (`rating.input_current`, `2.1 A`: what sizes the protection and cable before it) and its
  **Efficiency** (`rating.efficiency`, `90 %`). They show in the Device tab after the nameplate
  fields, on the rating line only when `rating.template.power_supply` adds them, and in a BOM
  rating column. A power supply's typed rating with a frequency or a percentage is kept as the note.

To try it: place a power supply (Checks shows "no rating"), type `230VAC/24VDC 5A` as its rating
(the warning goes), then `230 V AC` (refused: input or output?). Place a transformer and type
`250 W` (refused: use VA). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-engine, ecad-mcp and ecad-inspector
(`…power_supply…`), and `rating::reader::power_supply` in ecad-electrical; the four fields above
through `devices::rating_fields` in ecad-engine (automated, not tried by hand).

### Lane B, step 5: user fields and device links

The project can define its own **user fields** (design 072 §2.8): each has a name (unique,
ignoring case) and a kind: text, number (with an optional unit), yes/no, or a choice from a fixed
list. A device stores its values by field. Two devices can be **linked** (§2.6): a mechanical
interlock (a reversing pair) or an auxiliary block. A link lives in the project, not in either
device; deleting a device removes its links in the same step, and undo brings them back.

There is no panel for either yet; they are commands (and MCP `execute`):

- `userfield {"name": "Row", "kind": "number", "unit": "mm"}` adds a field;
  `{"name": "Side", "kind": "choice", "options": ["Left", "Right"]}` a choice field;
  `{"name": "Row", "remove": true}` removes one and clears its values.
- `setprop {"component": <id>, "key": "user.Row", "value": "12.5 mm"}` sets a value; it is read
  as the field's kind, so `twelve` is refused, and so is `Up` for the choice field. An empty value
  clears it.
- `linkdevices {"kind": "mechanical_interlock", "devices": [<id>, <id>]}` answers the link id;
  the same link twice or a device linked to itself is refused. `unlinkdevices {"jumper_bar": <id>}`
  removes it.

The inspector's Raw and Readable views show `ModifyProject` with the field list, a device's
`user` values, and the `AddDeviceLink` / `RemoveDeviceLink` ops. Nothing reads a link yet (lane F's
interlock checks will). Nobody has tried it by hand yet.

Tests: `lanes::lane_072b` in ecad-model, ecad-file, ecad-engine, ecad-mcp and ecad-inspector
(`user_fields…`, `device_links…`) (automated, not tried by hand).

## Lane D: wires carry typed values

A conductor is now a **wire** in code and files (`Wire`, ops `CreateWire`, `DeleteWire` and the
new `ModifyWire`). A wire stores its values as fields, not free text: its size (`2.5` mm² or
`18 AWG`), its colours as an ordered list of IEC 60757 colours (green/yellow is `GN/YE`, never one
colour of its own), its wire number with its fixed flag, its potential, its rating inputs (basis,
manual Iz, bundling) and its load.

- Typing is lenient: `2.5 mm²`, `awg 18`, `GNYE`, `green/yellow` and `BLU` are all read. What is
  stored is one form only; a file holding any other form, or a value out of its bounds, opens
  read-only.
- `setprop` on a wire takes `size`, `colours` and the rating keys; any other key (`tag`, a free
  note) is refused: a wire has no free properties. Its number and potential still belong to its
  net (`wirenumber`, `potential`).
- Labels write two-colour wires with a slash: `GN/YE` (three-letter style `GRN/YEL`).
- The colour setting's words are `code2`, `code3` and `name` (were `iec`, `three-letter`):
  Project settings ▸ Wires ▸ Colours has a third choice, `Brown Blue Green/Yellow`.
- `rating {"wire":…, "protected_by":"-Q01"}` (or `-F01:3`) stores the device itself; a name that
  is no device is refused, also by setprop. Deleting the device clears it in the same transaction
  (the message says how many were cleared); U brings them back.
- Projects saved before this lane do not open (no compatibility before 1.0): regenerate them.

### Try it

1. Draw a wire between two connection points and select it.
2. In the wire's properties, set Cross-section to `2.5 mm²` and Colour to `GNYE`: the label shows
   `… 2.5 GN/YE`.
3. Type `thick` as the cross-section: it is refused with the forms it takes.
4. Open the project in the inspector: the wire's row shows its size, colours, potential and
   rating values, and the last transaction reads `ModifyWire`.

The headless tests `lanes::lane_072d` in ecad-model, ecad-mcp and ecad-inspector cover these
steps; nobody has tried them by hand yet.

## Lane E, step 1: typed pages (design 072 §5.1)

A page's title, number, folder, kind, page type, sheet and borrowed title block are fields of the
page now, not keys in its property bag. Renaming, renumbering, moving to another folder or
setting a page's sheet or type records one `ModifyPage`. A generated page's kind is fixed when it
is made: a terminal plan of every strip, or of one strip.

- Nothing should look different: page numbers, folders, the Pages panel, the title block and
  references follow every change as before.
- A file whose page holds one of the old keys (`number`, `folder`, `sheet`, `title_block`,
  `generated`, `strip`, `page.type`) in its property bag opens read-only and names the key.
- The inspector's Pages view shows each page's kind, page type, sheet and title block.

### Try it

1. Make a page, rename it from the Pages panel, set its number and sheet in its properties, and
   move it into a folder: each is one undo step, and the title block follows.
2. New page ▸ Terminal plan, once with no strip and once for one strip: both show their strips.
3. Open the project in the inspector: the Pages view lists the kinds.

Tests: `lanes::lane_072e` in ecad-model, ecad-file and ecad-mcp. Agent-observed only; nobody has
tried these steps by hand yet.

## Rotations (A2: F6, F7)

- Every command parameter named `rotation` takes degrees, now also the symbol builder's text
  slots (`symbolnew`/`symboledit` `slots[].rotation`), which took radians before. Files are
  unchanged: stored angles stay radians, now typed as such.
- The inspector's Layout tab now selects the right rotation segment for a turned symbol (it read
  the stored radians as degrees).

### Try it

1. Place a coil, turn it a quarter turn (the rotate key while placing, or `place {"at": [100, 0],
   "rotation": 90}`), select it: the Layout tab shows 90°.
2. In the symbol builder, add a text slot and turn it: it turns a quarter turn per click, as before.

The headless test `lanes::lane_072a::a_slot_rotation_is_given_in_degrees_and_stored_in_radians`
(ecad-engine) covers the slot input; nobody has tried these steps by hand yet.

## Lane G: wire types are typed records

What changed for someone using the app:

- **Wire categories are now wire types.** A wire type stores its values as fields, not free text:
  potential, size (`2.5` mm² or `18 AWG`), colours as an ordered list (`GN/YE`), **insulation**
  (new: `pvc`, `xlpe`, `epr`, `rubber`, `silicone`, `ptfe`, `lszh`), part number, maker,
  description, allowance, rating and the drawn line.
- In `setstyle` on the `wire` catalogue the fields are now `size` (was `cross_section`) and
  `colours` (was `colour`). Typing is lenient as for a wire (`6 mm²`, `green/yellow`, `XLPE`); what
  is stored is one form only.
- Two new built-in types, **230 V AC L** (`ac230_l`) and **230 V AC N** (`ac230_n`): brown and
  blue, 1.5 mm², no potential of their own.
- A style record in a file now holds a body of its own catalogue (`"body": {"wire": {…}}`); a
  field the type does not have, or a value in another form, makes the project open read-only.
  Projects saved before this lane do not open (no compatibility before 1.0): regenerate them.

- **Every style catalogue is typed** the same way: text styles, location styles, frames, page
  types, rating tables and cable types store their fields as values, not free text. A file whose
  style record holds an unknown field, or a value in another form, opens read-only.
- Cable types: `cross_section` is now `size`, and a cable type can name a **core colour scheme**
  (`core_scheme`): `harmonised` (power: brown, black, grey, blue, green/yellow; numbered black
  cores from six up), `numbered`, `din47100` (data cables) or `icea_k2` (control cables). A
  cable of that type takes its cores' colours from the scheme by its core offer (`5G`); the
  type's own `core_colours` list still wins.

### Try it

1. Open **Standards ▸ Wire types**: the list shows the twelve built-in types, with the two
   230 V ones.
2. Select **Protective earth**, set Insulation to `xlpe` and Size to `6`: a wire of potential PE
   now labels `… 6 GN/YE`.
3. Type `paper` as the insulation: it is refused with the words it takes.
4. In **Standards ▸ Cable types**, make a type with Cores `5G` and Core colour scheme
   `harmonised`; give a cable that type: its cores show brown, black, grey, blue and
   green/yellow.

The headless tests (automated, not tried by hand):

```sh
cargo test -p ecad-model --test all lane_072g
cargo test -p ecad-electrical styles::wire
cargo test -p ecad-electrical styles::cable
cargo test -p ecad-mcp --test all lane_072g
cargo test -p ecad-inspector --test all lane_072g
```
