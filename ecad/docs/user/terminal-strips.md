# Terminal strips

A terminal strip (`-X01`) is one tagged device: a run of terminal blocks side by side on a rail, with its
accessories. Each terminal block is a **position** (`-X01:1`); a multi-level block has several levels,
each with its own connection points (`-X01:1.L`, `-X01:1.N`, `-X01:1.PE`). The terminal plan, the
graphical strip pages and the BOM rows are read off the drawing every time; none of them is stored.

Words used here are in the [glossary](../reference/glossary.md).

## Place terminal blocks

The **Terminal** group in the tool strip holds the tools:

| Tool | Does |
| --- | --- |
| **Terminal** / **Terminals across wires** | Place Terminals: a click on a wire, or a line across several wires, puts one terminal block on each wire crossed |
| **Draw a jumper bar** | Draw Bridge: a wire between two terminal blocks of one strip (a bridge) |
| **Renumber…** | Renumber the positions in drawing order |
| **Terminal manager** (foot of the flyout) | Opens Project ▸ Terminals |

Place Terminals asks, in order: the strip (Enter for the last strip, or a new one), the **Type**, the
**Level** and the **Position** (Enter for the next free one); then click on a wire or draw a line across
wires. A terminal block placed on a wire splits it; both halves land on the block and keep one wire
number. Clicking a wire's end at a device is refused: click along the wire, away from its ends. Each
click or line is one step of undo.

Types: feed-through, PE, two-level, two-level bridged, three-level, L/N/PE, sensor, disconnect, fuse.
For a multi-level type, place each level with its own line: for example `LNPE` with level `L` across the
L wires, then `N`, then `PE`. One strip results, with labels `1.L`, `1.N`, `1.PE` and so on. The
separator between position and level is a project default (`terminal_block.level.separator`, `.`).

Hovering a terminal block shows its label, its type and level, and what it reaches on the inside (the
panel's devices) and the outside (the field, or a cable core).

- **Fuse and disconnect** positions break the net: each side has its own wire number. Give a fuse
  position its rating in the **Rating** cell of Project ▸ Terminals (`6,3A T` reads as **6.3 A, T**).
  Checks lists **Fuse positions without a rating** until each one has one.
- A fuse terminal placed as a device of its own shows in Checks (**Fuse terminals that are devices of
  their own**) with **Make strip position**: it becomes a fuse position of the nearest strip on the page,
  keeping its wires and rating. Also by search (Ctrl+K, **Make strip position**) or `toposition`.
- **Draw a jumper bar** draws a real wire. A jumper bar as a part is made with **Jumper** (below).

## The strip in the inspector

Select a terminal block or its strip:

- **Terminals offered** sets the positions the strip has, so spare positions count: `12`, `12+2PE`, or
  by type, `12+4x3`. Text that does not read is refused under the field. **Terminals** then reads, for
  example, `3 drawn, 6 spare`. Drawing more than the offer gives the Checks row **More terminals than
  the strip offers**, with **Offer drawn**.
- **PE part number**, **PE maker**, **Accessories placed**, and a part number and maker for each kind of
  accessory, **markers per card** and **jumper bar size**. Where the strip has none, the project's value
  shows faint.
- **Show plan** opens the strip's terminal plan.

A terminal block's wheel (right-click) has **Edit label**, **Jumper**, **Go to strip plan**, **Show row**
and **Accessory**.

## Project ▸ Terminals (the terminal manager)

The Project workspace's **Terminals** tab lists every position, drawn and spare, laid out as a terminal
plan: inside columns (**From**, **Wire**) towards the strip, the strip in the middle (**Terminal**,
**Label**, **Level**, **Type and part**, **Rating**, **Jumper bar**), outside columns (**Wire**,
**Cable**, **Core**, **To**) away from it, and **Page**. **Terminal** and **Label** rename a position or
one level's label; **Rating** sets a fuse position's rating. Everything else is read from the drawing.

The Reports window's **Terminals** tab shows the same plan per strip, with an **Accessories** switch and
the **Accessory ▾** menu for a strip's ends and markers.

## Accessories

End stops, end plates, partitions, strip labels and markers are parts you place where you want them.
ECAD adds nothing by a rule. They appear in the terminal plan in position order and on the BOM.

- **Strip accessory** (search with Ctrl+K, or **Accessory** on a terminal block's wheel): pick a
  position and choose what goes after it, for example a partition.
- The Terminals tab's **Accessory ▾** menu places items at the strip's start or end (**At start: end
  stop**, **At start: strip label**, …) and **Marker on every position**.
- Part numbers: on the strip in the inspector, or for the whole project as project properties named
  `strip_<item>_part_number` and `strip_<item>_maker` (for example `strip_end_stop_part_number`) in
  **Project menu ▸ Project info ▸ Other properties**. An accessory without a part number is counted in
  Checks (**Strip accessories without a part number**) and left off the BOM.

## Jumper bars

A jumper bar joins positions of one strip into one net. It is a part owned by the strip, not a wire,
and is drawn as a dashed line through its terminal blocks on every page.

1. Choose **Jumper** (terminal block wheel, or search) and click the positions; Enter ends. Consecutive
   picks take the positions between them.
2. Hovering the line shows its positions and the bars needed. Selecting it shows its **Positions** in the
   inspector; type `1-5` to extend it. Erase the line to delete the jumper bar.

On a multi-level strip a jumper bar joins one level across positions (the `+` levels of a sensor strip,
for example) or the levels of one position. The BOM counts jumper bars as bought: span divided by the
bar size (10 unless the strip or project says otherwise), rounded up.

Checks: **Link crosses a partition**, **Link names positions the strip lacks**, **Link names a level the
position lacks**.

## Strip pages

Add a generated page from **New page ▾** in the Book (see [Pages](pages.md#generated-pages)):

- **Terminal plan**: the plan as a table, every strip or one.
- **Terminal strips**: a picture of each strip as wired on the rail. One column per level, inside
  conductors up to their targets, outside conductors down, each with its wire number and core label; the
  terminal block's own place beside its label. PE levels are hatched, spare positions dashed, a fuse
  shows its break mark and rating. Jumper bars run in lanes with a dot on each linked level; accessories
  are columns of their own; markers are small boxes on top. Each cable has a bar under its cores and a
  box with its tag, type and maker. Long strips run onto further sheets, never splitting a position;
  each sheet is titled `Terminal strips: -X01 <description> (2/3)`.

A terminal plan page's style is `table` or `graphic`: Settings ▸ Standards ▸ **Terminal strip pages ▸
Terminal plan style**, for the project or one page. The same group holds the layout lengths. Text sizes
follow the text styles. Everything prints in ink; a missing value prints as a `?` box.

## Checks

Besides those above: **Terminal drawn twice**, **Terminal level drawn twice**, **Position mixes terminal
types**, **Accessory at a position the strip lacks**.

## Command window

```
terminalblocks {"from": [x, y], "to": [x, y]}
terminalblocks {"strip": "<id>", "type": "lnpe", "level": "N", "from": [x, y], "to": [x, y]}
terminalblocks {"strip": "<id>", "pe": true, "from": [x, y], "to": [x, y]}
bridge {"from": "<connection point id>", "to": "<connection point id>"}
setprop {"component": "<strip id>", "key": "terminal_blocks", "value": "12+2PE"}
setprop {"component": "<strip id>", "key": "rating.<position>", "value": "2A"}
terminalblocklabel {"connection_point": "<id>", "text": "2N"}
terminalblocklabel {"connection_point": "<id>", "position": "2.1"}
toposition {"device": "<fuse terminal id>"}
accessory {"strip": "<id>", "at": "start", "items": ["end_stop"]}
accessory {"strip": "<id>", "at": "all", "on": true, "items": ["marker"]}
accessory {"strip": "<id>", "at": "2", "remove": ["partition"]}
jumper {"connection_points": ["<id>", "<id>"]}
jumper {"strip": "<id>", "positions": "7, 9, 11", "skip": true}
jumper {"jumper_bar": {"strip": "<id>", "n": 1}, "add": "5"}
setprop {"component": "<strip id>", "key": "jumper.bar", "value": "4"}
stripplan {}
newpage {"generated": "terminal_blocks", "style": "graphic", "strip": "<strip id>"}
renumber {"kind": "terminal_blocks", "preview": true}
```

`terminalblocks` takes at most 200 crossings. `jumper` edits also take `remove`, `split_at` and
`delete`. `stripplan` only reads. Each edit is one transaction.

## Limits

- Accessories are placed by hand; there are no test plugs or rail parts, and marker text is not printed.
- **Swap top and bottom** and **Reorder the strip…** are greyed in the Terminal group: not available yet.
- A fuse position cannot be turned back into a device.
