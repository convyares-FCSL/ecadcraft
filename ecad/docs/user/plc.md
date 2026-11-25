# PLC racks, cards and signals

ECAD keeps a PLC as devices: a **rack** is its CPU or head module, a **card** sits in one of the rack's
numbered **slots**, and each card has **channels**. Every channel has an address, a pin per role, a
description, a PLC tag and a safe state. The rack overview, the I/O list and the PLC pages are read off
the project; nothing on them is stored.

Words used here are in the [glossary](../reference/glossary.md).

## Racks and cards

1. Place a CPU from the symbol picker (search `rack`, or `plccpu`). It is a rack (`-A01`). A head module
   (`plcim`) makes a remote I/O rack of its own, with a coupler head. The first rack in a project also
   adds the generated **PLC racks** page to the book, in the same step.
2. Place a card body (search `card`: `plcdi`, `plcdq`, `plcai`, `plcao`, `plcrtd`, `plciol`, `plcps`,
   `plcpm`, `plccm`). ECAD asks **Rack** (the rack of the last card placed, else the only rack; `None`
   for no rack) and **Slot** (the next free one): Enter, Enter takes both. A taken slot is refused with
   the card that is there and the next free slot.
3. A card in a slot has a caption under its body, `-A01 slot 2`.

Moving a card to another slot shifts the automatic addresses of the cards after it; the echo says how
many change. Deleting a rack keeps its cards and takes them out of it in the same step
(`Deleted -A01; 6 cards are now in no rack.`); undo puts both back.

A card's type is DI, DQ, AI, AO, RTD, IOL (IO-Link), PS, PM or CM. It can change among the I/O types
only (their bodies' points are the same). A new card takes the project's channel count for its type
(16 for DI and DQ, 4 for the others) when it is placed.

## Channels

Channels are drawn as cells: `chdi`, `chdq`, `chai`, `chao`, `chrtd`, `chiol`, or the compact `di` and
`do`. Cells of one card side by side read as the card: abutting cells form one box with the card's name
block bottom left and its type (`DI 16`) bottom right. A card body is one power cell with `L+` and `M`;
placed at the end of the cells it closes the box.

- **Channel…** (PLC group in the tool strip) places channels as a fence: a line across wires puts the
  card's next channels on them, one per wire. Wires closer together than a cell is wide (20 mm for DI and
  DQ) are refused before anything is written. The same tool draws a card's remaining channels as spares.
- Each cell shows its channel number, its address and its description written upwards under it; a
  drawn spare reads `Spare`.
- A channel point's label is its **pin** (`-A02:4`), given once and never renumbered: role by role, or
  channel by channel with the project default **Pin layout of a card**.
- A wire on a channel that goes on through the card to another point is kept, never cut, and listed in
  Checks.

## Addresses

Addresses are automatic by slot and channel, per rack, in the project's **Address style**:

| Style | Example |
| --- | --- |
| `byte.bit` (built in) | `I2.3`, `IW4` |
| `iec` | `%IX2.3` |
| `percent` | `%I0.0`, `%IW64` |
| `slot` | `I:3/04` |
| `none` | no address |

A typed address wins and shows `*` in lists. Fixing a whole rack's addresses keeps them where they are
when cards move. The first input and output byte, bytes per analogue channel and per IO-Link port are
project defaults (Settings ▸ Standards ▸ **PLC**).

## Project ▸ PLC

The Project workspace's **PLC** tab is the signal list.

- **The rack strip**: one tile per slot, the rack first and the next free slot last. A tile shows the
  slot, the tag, what the card is (`16 outputs`), the address range, one square per channel (in use,
  free, reserved, or with a check) and where it is drawn, or **Make its page** when it is not drawn yet.
  More than one rack shows one chip per rack.
- **The card header** of the selected tile: its channel counts and page, **Export tags**, a go-to for
  its page, and **…** ▸ **Make tags**.
- **The table**: **This card** or **All cards**. **Description**, **Address**, **Pins**, **Tag** and
  **Safe** are inputs; **Drives** shows the end device the channel reaches through terminal blocks and
  connectors; click it to go there.
- **The right panel** edits the card: **Type**, **ID**, **Description**, **Rack**, **Slot**,
  **Function**, **Location**; **Addresses** (**Start**, typed as `%Q4.0` or `4`, empty for automatic;
  **Rule**); **Detailed page** (go to it, or draw the missing channels); **Parts** (the main part's maker
  and part number, **+ Accessory**).

A channel with a wire is **used**; with no wire but a description, **reserved**; with neither,
**spare**.

**Tags** are unique within one PLC, ignoring case: a duplicate is refused
(`Tag "Valve_Inlet" is already -A03 channel 2 (%Q2.2).`). **Make tags** fills empty tags of used and
reserved channels from their descriptions (`Inlet valve open` → `Inlet_valve_open`), made unique with
`_2`, `_3`.

**Safe state**: a digital output defaults to `Off`, an analogue output to `Hold`; type `On`, `Off` or
`Hold`, or for an analogue output `Hold`, `Min`, `Max` or a value (`4 mA`). Inputs have none.

## Tags in and out

- **Export tags** writes a CSV tag table (`<project> PLC tags.csv`; **All racks** for every rack). The
  echo counts channels left out (those without a tag). Values a spreadsheet would run as a formula
  (starting `=`, `+`, `-` or `@`, as device tags do) are written with a leading `'`. Export changes
  nothing.
- **Import an I/O list** reads a CSV (comma, or `;` with a BOM). Each row's channel, found by address,
  takes the row's name as its tag and its comment as its description, in one step. Rows at an address no
  card has are named in the echo. If the result would give two channels of one PLC the same tag, nothing
  is imported.

This is a one-way exchange: nothing of an imported table is kept, and nothing is compared afterwards.

## PLC pages and lists

- **Make its page** (on a tile, or **Detail page…** in the PLC group): a new page after the rack's last
  card page, titled `-A03 DQ 16`, with every channel not yet drawn and the card body. Output cards are
  drawn upside down, points on the bottom edge.
- **PLC racks** (generated): per rack its tiles in ink and each card's place or `not drawn`.
- **I/O list** (generated): one ruled table per card with a heading
  (`-A01 slot 2 · -A03 · DQ 16 · <part number> · 9 used, 1 reserved, 6 spare`); a card longer than a
  sheet continues with `(continued)`.
- A release writes ` IO list.csv` and `PLC racks.csv` beside the other lists, or leaves them out when
  the project has no PLC cards. See [Releases](releases.md).

## Datasheet fields

A rack takes **Supply**, **Backplane current supplied** and **Power**; a card takes **Backplane current**
and **Power loss**. They show in the inspector's Device tab and a BOM rating column; there is no rating
line. A rack also takes a CPU family, a network and network address (`192.168.0.10`), and a card a
signal, a protocol (communication modules) and yes/no attributes: safety-rated, Ex i, calibration
certificate.

## Checks

Two cards in one slot · Card names a rack that is not in the project · Card in a slot its rack does not
have · PLC cards in no rack · Channel drawn twice · Channel beyond the card's count · Channel drawn as
another card type · Address blocks overlap · Address given twice · Address block past byte 65535 · Used
channels with no address · Wire runs through a PLC card · PLC texts wider than their cells · PLC tag
given twice in one PLC · PLC tag an export leaves out. Most offer a fix (**Move to slot n**, **Raise to
n channels**, **Clear the typed address**, …).

## Command window

```
plcslot {"card": "<id>", "rack": "-A01", "slot": 5}
plcracks {}
plcchannels {"card": "-A02", "from": [x0, y], "to": [x1, y]}
plcchannels {"card": "-A02", "rest": true}
plcchannel {"card": "-A02", "channel": 3, "text": "Pump 1 run", "tag": "Pump_1_run", "safe": "off"}
plcaddress {"rack": "-A01", "style": "iec"}
plcaddress {"rack": "-A01", "fix": true}
plccardtype {"card": "<id>", "type": "dq"}
plcmaketags {"card": "<id>"}
plcexport {"rack": "-A01", "path": "C:/tmp/tags.csv"}
plcimport {"path": "C:/tmp/tags.csv", "dry_run": true}
plcpage {"card": "<id>"}
iolist {"state": "spare"}
newpage {"generated": "iolist"}
setprop {"component": "<id>", "key": "plc.protocol", "value": "modbus_tcp"}
```

`plcracks`, `iolist`, `plcexport` and `plcimport` with `dry_run` only read. `plcimport` also takes
`text` instead of `path` and `what` (`both`, `tags` or `descriptions`).

## Limits

- Tag tables are generic CSV only.
- A tag table is read up to 16 MB and 20 000 rows; more is refused.
- The **Test** column and section show no tests yet.
- The **PLC racks** page stays when the last rack is deleted; it then says `No PLC racks.`
- A rack's supplied backplane current and its cards' draw are not compared.
