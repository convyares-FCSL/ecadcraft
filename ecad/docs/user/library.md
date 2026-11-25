# Library

The symbols you place come from the library: the built-in IEC symbols, and the project's own
symbols that you draw in the symbol builder. A symbol is embedded in the project when it is
placed, so a project never depends on a library file.

## The Library workspace

**Library** in the top bar (Ctrl+3) opens the browser.

- **Tabs** across the top: **Symbols** (with its count), Snippets, Parts, Makers and Title blocks.
  Title blocks are described in [Pages](pages.md); the other tabs say what they will hold.
- **Search**: "Find a symbol by name, tag letter or pin". A family word finds a family:
  `protection`, `sensors`, `connectors`, `plc`.
- **Filters**: **Source** (All, Built-in, Project) works; Poles, Contact, Standard and Maker are
  greyed with their reason.
- **The category tree** at the left, with the sources and their counts at its foot (**IEC
  built-in**, read-only, and **This project**). Tiles are grouped under their category; hover a
  tile for its connection points and text slots.
- **The selected symbol** on the right: its picture, its source, what it is, how often it is used,
  then **Place on page N** and **Edit a copy** (a built-in) or **Edit symbol** (a project symbol).

There is no inspector in the Library.

## Built-in symbols

The built-in set covers a control panel without hand-drawn boxes: coils and contacts (NO, NC,
change-over, 3- and 4-pole power contacts), MCBs, RCBOs and RCDs, fuses, a fuse terminal and fuse
banks, power supplies, push buttons and switches (proximity, float, roller, interlock), lamps,
motors, a transformer, solenoid valves, a thermocouple and a heater, SSRs, terminal blocks,
connector halves (plain, D-sub, M12) and RJ45 and USB bulkheads, PLC card bodies and channels, and
the continuation arrow. The **Built-in** filter shows how many there are.

Each symbol brings its own tag prefix (a coil `K`, a fuse bank `F`). Change one in **Settings ▸
Pages and sheets ▸ Tags** or with `tagsettings` ([Devices](devices.md#tag-settings)).

`symbols {"query":"-Q"}` in the command window lists the symbols and devices a search finds.

## Place a symbol

- From the page: **Device ▸ Symbol** and its picker ([Devices](devices.md#place-a-device)).
- From the Library: select a tile and click **Place on page N**; then click on the page.
- From search: Ctrl+K and a symbol's name.
- Typed: `place {"symbol":"<key>","at":[x,y]}`. A project symbol's key places its newest version.

## Draw a new symbol

1. **Library ▸ + New symbol.** The builder opens on an empty sheet with a grid. The right panel's
   **Before it can be placed** list shows what is missing.
2. **Draw** (D): Line, Polyline, Rectangle, Circle, Arc. **Text** (T) adds fixed text such as a
   heading.
3. **Connection point** (C): click where a wire lands, usually a lead's end. A point goes on the
   5 mm grid; a click off it is refused at the cursor. Type a label (`L+`, `PE`) or press Enter
   for none.
4. **Text slot** (S): where a device text goes on each placed copy (tag, description, rating,
   part number, maker, function, location…). With a slot selected, **R** turns it 90° and **H**
   and **V** step its alignment.
5. **Origin** (O): the point the cursor holds when placing.
6. In Details, give a name, a key (`psu1`) and a tag prefix (`G`). When every row is done,
   **Save to project** writes it. The tile shows under **This project**, and **Place** places it.

The builder's strip has **Select** (V) and the tools above, with Snap, Grid and Ortho at its foot;
the builder always snaps to its grid. Ctrl+Z and Ctrl+Y undo and redo inside the draft.

**Drafts.** The builder keeps the draft while you work, also when you leave the Library or the
program stops. Drafts show in the browser under **Drafts**, with **Resume** and **Discard**. Save
is the only write to the project.

## Make a symbol from the page

Select plain lines and texts on a page (for example a box with leads and a text `-LS1`) and choose
**Make symbol** (the right slot of the wheel for several objects). The builder shows them on the
grid: free line ends are offered as connection points and tag-like text as the tag slot. Offers
are drawn dashed; nothing is applied until you accept them in Details.

**Accept all and save to project** replaces the drawing on the page with a device of the new
symbol; untick **Replace the drawing on page N with this symbol** to keep the lines.

A device drawn from plain lines ([Devices](devices.md#make-a-device-from-a-drawing)) has **Make
symbol** on its device wheel: saving keeps the device, its connection points and its wires.

## Edit a symbol

Right-click a placed copy ▸ **Edit symbol** (the device wheel's top-left slot), or **Edit symbol**
in the Library. Editing a built-in makes a project copy that remembers its source.

Saving an edit of a symbol with placed copies opens **Before saving** under the canvas: one row per
placed copy and what changes there. A copy where the change would lose a connection (a wired point
removed) is a **clash**: its row is red, names the wire, and is unticked. Choose which copies to
update and **Save**: the new version and the updates are one undo step.

A copy left on the older version shows **update available** in the inspector, with **Update** and
**Compare**. An update keeps every device, connection point and wire ID; a wire end on a moved
point follows it. A connection point label that was the old default takes the new one; a label set
by hand stays.

## Typed forms

| Command | Does |
| --- | --- |
| `symbolnew {name, key, graphics, points, slots?, defaults?}` | Saves version 1 of a project symbol |
| `symboledit {definition, …, update: all \| selected \| none \| new, copies?}` | A new version, and the chosen placed copies updated |
| `symbolimpact {definition, …}` | Read only: every placed copy, what changes and whether it clashes |
| `symbolupdate {copies \| lineage, to?}` | Brings copies to the newest (or `to`) version; refused while one clashes |
| `makesymbol {handles? \| component, name, key, …, replace?}` | A symbol from plain lines, arcs and texts, or from a drawn device |

## Limits

- No shared library files: **Open an .ecadlib** is greyed, and a symbol lives in the project that
  made it.
- Snippets, Parts and Makers are not built yet.
- The builder cannot yet set how many wires a connection point accepts (two).
