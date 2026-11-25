# The Properties panel

The panel at the right of the Draw workspace shows what is selected and lets you change it. Its
title is **PROPERTIES**. Grey text is read from the drawing; an input box is something you set here.
Every edit is one undoable step and goes through the same commands an agent or a script uses
(`setprop`, `pageprops`, `projectprops`).

## Opening it

A rail of three buttons runs down the right edge: **Device**, **Layout** and **Checks**. Press one
to open the panel on that tab; press it again, press `]` or use the header's collapse button to
close it. The Checks button carries a dot when there are problems (see [checks.md](checks.md)).

The header names what is selected: for a device its tag, its description, which symbol and where
it is drawn, and a **Row** button that opens the device's row in **Project ▸ Devices**. Every titled
section folds to a one-line summary; click its title.

## A device

Select a symbol of a device. The **Device** tab shows:

- a status line with the device's own checks (`No checks on -K01` when there are none);
- **Identity**: Tag, Function, Location, Description, the symbol type and the symbol;
- **Parts**: Maker, Part number, and the device's part lines read only (see
  [reports.md](reports.md));
- **Where it is drawn**: every symbol of the device and its place (`2-A-5`); click a row to go
  there;
- **Connections**: each connection point, its wire and what else is on that wire;
- **Notes and attributes** (folded): the device's other values, such as its ratings and its user
  fields.

The **Layout** tab shows **Placement** (position, rotation and mirror, read from the drawing),
**Text on the sheet** (which texts are on, where, and their size as a text style), the **Contact
table** (Below, Right, Hidden) and **Line and colour**. See [styles.md](styles.md) for text styles
and [references.md](references.md) for contact tables.

A tag that another device already has is not simply refused: the field offers **Swap**, **Make it
the same** device, or **Use** the next free tag.

While Place Symbol runs, the Device and Layout tabs describe the device about to be placed: its
next tag, its symbol and what the click will change.

## A page

Click empty paper (nothing selected). The header names the page and the Device tab shows its
properties: **Number**, **Title**, **Folder** (read only), **Function**, **Location** and
**Sheet**. A page with no function, location or sheet of its own shows the project's value, marked
`(project)`. A page number that another page already has is refused under the field. Layout shows
the defaults that apply to the page.

On a generated page the panel shows the page's blocks with **Edit…** (on a cover), the generated
pages with their toggles, and the **Sheet** read from the page's type. See [pages.md](pages.md).

## Wires, cables and other objects

- **Wire**: its number and potential, **From** and **To**, the cables it is in, and its net.
- **Cable**: Tag, Description, **Cores** (its own, else read from its cable type, shown faint) and
  **Used / Spare**, with a button to show the cores. See [cables.md](cables.md).
- **Location box**, **continuation**, **reference**, **jumper bar**: their own fields; see the guide
  for each.
- Several objects of different kinds: a summary of the selection.

## The project's own values

Project values are not in the panel: they are in **Settings** (project menu, the project name at the
top left):

- **Project info** (project menu ▸ **Project info**): Name, Title, Drawing no., Customer, Status, the
  functions and locations list, the defaults for new pages, and **Other properties** (free name and
  value pairs), with a preview of the title block. **Save** writes, **Cancel** writes nothing.
- **Pages and sheets** (project menu ▸ **Project settings**): the sheet, references, tags,
  revisions, numbering and wires (see [styles.md](styles.md)).
- **Revisions** (project menu ▸ **Revisions**): see [releases.md](releases.md).

## User fields

A project can define its own **user fields**: each has a name (unique, ignoring case) and a kind:
text, a number with an optional unit, yes/no, or a choice from a fixed list. A device stores a value
per field. A value is read as the field's kind, so `twelve` in a number field or a word that is not
one of the choices is refused. An empty value clears it.

There is no panel for defining user fields yet; use the command line (or an agent):

- `userfield {"name":"Row","kind":"number","unit":"mm"}` adds a field;
  `userfield {"name":"Side","kind":"choice","options":["Left","Right"]}` a choice field;
  `userfield {"name":"Row","remove":true}` removes it and clears its values.
- `setprop {"component":"<id>","key":"user.Row","value":"12.5 mm"}` sets a device's value.

## Device links

Two devices can be **linked**: a mechanical interlock (a reversing pair) or an auxiliary block. A
link belongs to the project, not to either device; deleting a device removes its links in the same
step, and undo brings them back. Linking a device to itself, or the same link twice, is refused.

Typed: `linkdevices {"kind":"mechanical_interlock","devices":["<id>","<id>"]}` (answers the link's
ID), `unlinkdevices {"link":"<id>"}` removes it.

## Typed forms

- `setprop {"component":"<id>","key":"description","value":"Pump contactor"}`; several devices at
  once with `"components":[…]` (each value is checked as it would be on one device).
- `pageprops {"page":"<id>"}` reads a page's properties; add fields to set them.
- `projectprops {}` reads the project's; add fields to set them.

A device's tag, description, part number, maker, article number and type text are fields of the
device: a part number, maker, article number or type text is at most 64 characters, a description
at most 256.

## Limits

- No panel yet for user fields, device links or part lines: they are commands.
- **+ Accessory** and **From catalogue** under Parts, **Variant** and **Lock** under Placement, and the
  contact table's **Used only / All contacts** are shown but do nothing yet.
- A contact not drawn yet shows as "not used yet", with no button to place it.
- Nothing reads a device link yet.
