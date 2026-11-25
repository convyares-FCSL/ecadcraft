# Circuits: ratings, sizing and volt drop

ECAD checks that every panel wire and field cable with rating data is big enough for the device that
protects it, and that a cable's volt drop is within its limit. The results are rows in the Checks list,
with the usual counter, go-to and fixes. There is no calculator screen.

Words used here are in the [glossary](../reference/glossary.md).

**ECAD ships no IEC or BS 7671 table values.** Current-carrying capacities, correction factors and
conductor resistances come from a **rating table** that you enter or import, from a source you are
licensed to use. Every number in the examples below is made up.

## The rule

For each rated wire or cable: **Ib ≤ In ≤ Iz**.

- **Ib**, the design current: the subject's own **Load** (typed, in A).
- **In**, the rated current of the protective device that feeds it (see below).
- **Iz**, what the conductor carries here: its rating times the correction factors.

Checks rows:

| Check | When |
| --- | --- |
| Wire or cable rated below its protective device (error) | Iz < In: `… carries 18.8 A; -Q01 is 20 A` |
| Load above the protective device's rating | Ib > In |
| Conductor at or above its maximum temperature (error) | the panel or ambient temperature reaches the insulation's limit |
| Cable volt drop over its limit | `… drops 6.0 % at 0.5 A; limit 5 % (1 mm² would drop 4.5 %)` |

Where ECAD can work out a larger size that passes, the error offers it as the fix. Other rows say what
data is missing: no protective device upstream, two devices feeding one subject, a device rating with
no readable current, no rating data, no rating table chosen, no table value for the subject, panel wires
with no bundling factor (× 1.00 used), cables with no cable type (Cu, 70 °C, class 5 assumed), cables
with no load and no device (no volt drop), table rows not read or out of order.

A wire or cable with no rating data makes no rows, so a project that does not rate wires is not nagged.

## Where Iz comes from: the basis

Each subject uses one of three bases (**Basis** in its Rating section):

- **Datasheet**: the **Rated current (A)** and the temperature it is rated at, from the wire type (panel
  wires) or the cable type. Built in for panel wires.
- **Table**: the project's rating table, looked up by installation method, insulation, loaded
  conductors and size. Built in for field cables.
- **Manual**: a rated current you type.

The project defaults are Settings ▸ Standards ▸ **Defaults ▸ Rating**: **Panel wire basis**, **Field
cable basis**, **Panel temperature (°C)** (40), **Field ambient (°C)** (30), **Rating table** (none),
**Bundling**, **Installation method** (E), **Supply voltage (V)** (400), **Supply system** (ac1, ac3 or
dc), **Volt-drop limit (%)** (5) and **Motor voltage tolerance (%)** (5).

Correction factors:

- **Panel wires**: the panel temperature of the wire's location (each location box has a **Panel
  temperature (°C)** row; unset, the project's), and **Bundled**: a factor from 0.30 to 1.00, or a
  circuit count (`3 bunched`) read from the table. Unset bundling counts as × 1.00, with a warning.
- **Field cables**: the **Ambient** temperature and **Grouping** (circuits grouped, 1 to 20). Without
  ambient rows in the table, the temperature factor is computed from the insulation's maximum
  temperature.

Factors are rounded to two decimals before multiplying; comparisons use the unrounded product. A current
is shown to 0.1 A, with an extra digit when it is within 0.05 A of the value it is compared with, so a
pass never reads the same as a fail.

## Where In comes from: protection

ECAD traces from the subject's net (each core's net, for a cable) through the connectivity until it
reaches the **load side** of a protective device: a fuse, MCB, RCD, RCBO, a fuse of a fuse bank, or a
fuse position of a terminal strip. It passes through terminal blocks, connector pins, contacts (by their
pin pairs, `13`–`14`, `L1`–`T1`) and RCDs by pole; any other device stops it. If two devices are found
at the same depth, the larger In is used and Checks says so. The trace stops after 64 nets; set
**Protected by** by hand then (`-Q01`, or `-F01:3` for one fuse of a bank). A typed **Protected by**
always wins.

A protective device's rating is typed fields read from what you type: `B16` on an MCB reads as **curve
B, 16 A**. Ambiguous text is asked about, never guessed (a bare `16` on an MCB asks for the curve).
Text that is no rating (`see datasheet`) is kept as a note, and Checks warns **Device with no rating**.
More about device ratings: [Devices](devices.md).

## Volt drop

For a cable with a length, a size the table has a resistance for, and a current: the drop at the load
(or, with no load, at the protective device's In), single-phase or three-phase from the cable's supply
system, against the cable's limit (5 % unless the project or the cable says otherwise). The resistance is
the table's value at 20 °C corrected to the conductor's maximum temperature.

## The Rating section

**Project ▸ Wires** and **Project ▸ Cables** show a **Rating** section in the right panel for the
selected wire or cable: **Basis**, the rated or table value, the temperature and bundling or grouping
factors (an unset factor shows `not set: × 1.00 assumed`), **Carries** with the protective device and a
tick or a warning, **Load**, and for cables **Method** and **Volt drop**. Each value you type there is one
step of undo. A Checks row's **Show rating** fix selects the subject.

## Rating tables

Settings ▸ Standards ▸ **Rating tables**: make a table, enter rows, **Apply rows**, then choose it in
**Defaults ▸ Rating ▸ Rating table**. **Import CSV…** and **Export CSV** take a file path. The screen
reminds you: enter only values you are licensed to use.

One row per line, comma-separated; a header line starting `kind,` is skipped and `#` starts a comment:

| Kind | Columns |
| --- | --- |
| `capacity` | method, insulation, loaded conductors, size (mm²), amps |
| `ambient` | insulation, °C, factor |
| `grouping` | method, circuits, factor |
| `resistance` | conductor class, size (mm²), Ω/km at 20 °C |

Methods are `A1 A2 B1 B2 C D1 D2 E F G`; insulation is `pvc`, `xlpe` or a maximum temperature from 50 to
250 °C. A table holds at most 4 000 rows and 256 KiB. Made-up example:

```
capacity,E,pvc,3,1.5,20
grouping,E,3,0.90
resistance,5,1.5,10.0
```

## Motors and their supply

A motor's rated voltage and frequency are checked against the supply of the potential on its nets
(a supply such as `400/230 V 3~ 50 Hz`, set on the potential with `wiresettings`). Checks warns **Rating
differs from its supply** when the voltage is further off than the **Motor voltage tolerance** (5 %: a
motor is built to run within about ±5 % of its rated voltage), the frequency differs, or the phases do
not fit. An empty **Rated voltage** on a supplied net offers **Use 400 V**; nothing is filled in until
you click it.

## Command window

```
rating {"wire": "<conductor id or wire number>", "basis": "datasheet", "bundling": "0.8"}
rating {"cable": "-W01", "basis": "table", "method": "E", "ambient": 35, "grouped": 3, "load": 0.5}
rating {"cable": "-W01", "vd_limit": 7}
rating {"location": "+CAB1", "temperature": 40}
ratingtable {"name": "My table", "source": "<where the values come from>", "csv": "capacity,E,pvc,3,1.5,20"}
ratingtable {"import": "C:/tmp/table.csv"}
ratings {"cable": "-W01"}
setstyle {"catalogue": "wire", "id": "l", "fields": {"rated_current": "27", "rated_at": "30"}}
```

`rating` also takes `manual`, `protected_by`, `voltage`, `system`, `cable_type` and `many` (up to 500
subjects at once); each call is one transaction and `null` clears a value. `ratings` only reads.
`ratingtable` with `import` or `export` works at the command window only; an agent passes `csv`.

## Limits

- No fault-protection check (earth fault loop impedance, adiabatic k²S²).
- Volt drop is per cable, not summed from the origin of the installation.
- Loads are typed only; they are not worked out from the devices.
- The `rating` command has no prompt sequence at the command window; its JSON form works.
