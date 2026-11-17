# 072 Typed object model: lane A1 (foundation types)

What changed for someone using the app:

- **A project holding a word this version does not know opens read-only.** A setting stored as
  text but meant to be one of a fixed set of words (for example `text.tag.show`, which is `on`,
  `off` or `first`) can hold something else if the file was edited by hand or made by another
  program. Before, ECAD quietly read such a value as the default. Now the project opens
  read-only, and the open message names the word, the setting and where it is (the project, a
  page, a device, a wire, a cable, the settings or the standard). The file is kept as it is.
  Save is refused; **Save As** still saves a copy.
- A yes/no property in a file (`tag_fixed`, `numbering.provisional`, `wire_number_fixed`) must hold
  exactly `yes` or `no`. Commands still accept `true`, `1` and `on` (and `false`, `0`, `off`) as
  you type them, and always write `yes` or `no`.
- Nothing else changes in how the app looks or behaves.

## Try it

There is no click-only way to make such a file: it needs a file edited outside ECAD. The headless
test `lanes::lane_072a::unknown_words_open_read_only` (ecad-cadcraft) makes one and checks the
message, the refused save and the unchanged file. The inspector still shows such a file in its
Raw and Readable views (`lanes::lane_072a::a_project_with_an_unknown_word_is_still_shown`,
ecad-inspector).

Run them from `ecad/`:

```sh
cargo test -p ecad-cadcraft --test all lane_072a
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
(ecad-cadcraft) runs these steps; nobody has tried them by hand yet.

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

The headless tests (automated, not tried by hand):

```sh
cargo test -p ecad-model --test all lane_072b
cargo test -p ecad-cadcraft --test all lane_072b
cargo test -p ecad-inspector --test all lane_072b
cargo test -p ecad-mcp --test all lane_072b
```

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
