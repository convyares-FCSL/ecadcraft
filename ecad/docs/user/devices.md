# Devices

A **device** is a real part with a **tag** (`-K01`), its properties (description, maker, part
number, function `=`, location `+`) and its connection points. It is drawn by one or more
**symbols**, on one page or several: a contactor's coil on page 2 and its contacts on pages 2 and
3 are all `-K01`. Where symbols come from is in [Library](library.md); wires between connection
points are in [Wires](wires.md).

## Place a device

1. Hover **Device** on the tool strip and choose **Symbol**. The picker opens at the top left of
   the page: **Symbol | Connector**, a search, and the symbols as tiles.
2. Type a few letters (`coil`, `cont`, `fuse`, `protection`) and click a tile. The picker then
   offers what the symbol can be placed as: **New device (-K02)** first, then the devices that can
   take it (a contact lists the devices with a coil first).
3. The picker folds to a chip ("as -K02"), and the intent line says what a click makes. Click on
   the page; each further click places another. Press **P**, or **Change** on the chip, to open
   the picker again. Esc ends the tool.

Ctrl+K finds symbols too. **Place on page N** in the Library places the selected symbol. A new
device takes the next free number of its prefix. Tags and connection point labels are drawn
beside every symbol.

Typed, in the command window:
`place {"symbol":"coil","at":[0,0]}` makes a new device;
`place {"symbol":"no","at":[40,0],"device":"<the device's id>"}` adds a contact to it;
`place {"symbol":"coil","at":[0,0],"tag":"-K05"}` names the new device, or adds the symbol to the
device that already has that tag. `"rotation"` (degrees) and `"mirror"` turn it.

## Symbols of one device

Every symbol of a device is equal: what it is for the device (head, contact, pole, channel, pin)
comes from the symbol, not from the order it was placed in. A device's **head** is its coil,
actuator, breaker or PLC card body.

- A device has one head. Joining a second coil is refused: `-K01 already has a coil.`
- A contact's reference text points at the head's page, and the contact table is drawn under the
  head, wherever the head was placed ([References](references.md)).
- Change-over contacts join as `11/12/14`, then `21/22/24`; a NO contact after them as `33/34`.
- Renumbering follows the head's place on the drawing.

## Tags

Tags are written with two digits by default (`-K01`, `-Q03`); `-K1` and `-K01` are the same tag.
Any letters are accepted (`-SR01`, `-TB01`): IEC 81346 is the default, never the law. Devices and
cables share one set of tags, so a device `-W01` and a cable `-W01` clash.

**Change a tag** with F2 (the selected device), the device wheel's **Edit tag**, or the Tag field
in the inspector or in Project ▸ Devices. Every symbol on every page follows; one undo puts it
back.

**A tag another device has** is never a dead end. Under the field ECAD says who holds it and
offers **Swap** (both tags change in one step), **Make it the same device** (merge; refused with
the reason when the result would have two heads) and **Use -K0n** (the next free one). Typed:
`setprop {"component":"<id>","key":"tag","value":"-K02","conflict":"swap"}` (or `"merge"`,
`"next"`); `setprop` without `conflict` refuses and lists the ways forward.

## Properties

The inspector's **Device** tab shows the selected device: **Identity** (tag, description,
function, location…), **Parts**, **Where it is drawn** (a click goes to that symbol),
**Connections**, and **Notes and attributes**. The device wheel's **Properties** opens it;
**Show row** opens its row in Project ▸ Devices. Fields are in [Properties](properties.md);
function and location, and how boxes propose them, are in [Locations](locations.md).

Typed: `setprop {"component":"<id>","key":"description","value":"Main contactor"}`.

## Find and list devices

- The **Devices** panel (tool strip): search, grouping by **None**, **Location**, **Function** or
  **Type**, and **Not on a page yet** first. A click goes to the device's first symbol.
- **Project ▸ Devices**: one row per device with Tag, Description, Function, Location, Main part,
  Parts, Symbol and On page ("not yet" for a device on no page). Many devices can be edited at
  once ([Workspace](workspace.md#the-project-workspace)).
- The device wheel's **Go to contacts** goes to its symbol on another page; it is greyed when
  there is none.

## Delete

- The device wheel's **Delete device** deletes the device with every symbol on every page, in one
  undo step. The typed tool (`devicedelete`) asks first: `Delete -K01 and its 2 symbols on page 1?`
- Deleting one symbol removes only that symbol: `Removed -K01's NC contact 21/22 (page 2); -K01 is
  still shown on page 1.`
- Deleting a device's last symbol deletes the device: `Deleted -K01 with its last symbol; U brings
  it back.`

## Numbering and renumbering

**Renumber** (Ctrl+K **Renumber**, or **Renumber…** in the Terminal group) opens the Renumber
dialog: **What** (Devices, Cables, Wires, Terminal blocks, All), **Which** (All, the selection,
Provisional, a Prefix, a Folder), **Start**, and a preview of every row, old → new, in
**drawing order**: page in book order, then column, then row of the sheet grid. **Apply n** writes
it as one undo step.

- **Fixed.** Typing a tag fixes it: Renumber and Finalise never change it. The **Fixed** box beside
  Tag releases it (the tag stays). A fixed tag is marked `F` in the lists, never on the sheet.
- Typed: `renumber {"kind":"devices","scope":"all","preview":true}` previews;
  `renumber {"kind":"devices","scope":{"prefix":"-K"},"start":1,"digits":2}` applies.

### Provisional tags

**Settings ▸ Pages and sheets ▸ Numbering ▸ Provisional tags until Finalise** gives new devices
`-K?1`, cables `-W?1` and new nets `24V-?` until you finalise. Checks lists each provisional tag
and the provisional wire numbers, each with **Finalise**. **Finalise numbering** (Ctrl+K) numbers
them after the highest number in use, in drawing order. Issuing a revision with provisional items
asks first: **Finalise numbering now**, **Issue anyway** or **Cancel**.

Typed: `numbering {"provisional":true}`, `finalise {}`.

### Tag settings

**Settings ▸ Pages and sheets ▸ Tags**: **Digits**, **Case** (Upper case or As typed), **Leading -
on tags**, and each symbol's **Prefix**, with an example of how `k5` is written. Typed:
`tagsettings {"padding":3,"prefixes":{"coil":"KA"}}` (`null` for a symbol goes back to its
built-in prefix; `tagsettings {}` lists them).

## Make a device from a drawing

Plain lines can become a device of their own, with connection points you add. This is also the
fastest way to check the basics by typing. Type each line in the command window (Ctrl+/) and press
Enter or **Run**.

1. **Draw a rectangle.** `line 0,0 40,0 40,20 0,20 c` draws four lines.
2. **Make it a device.** Select the four lines with a window selection, then type
   `convertcomponent` (or Ctrl+K **Make component**). The reply is
   `Device <id> created from 4 objects.` Note the ID. The rectangle now selects as one.
3. **Give it a connection point.**
   `addconnectionpoint {"handle":"<handle of the right-hand line>","at":[40,10]}` puts connection
   point `1` at 40,10 on the right-hand edge. Or type `addconnectionpoint` alone, then `40,10`,
   Enter, and Enter again to finish; clicking on an edge works too, and `N` names the next point.
4. **Place a coil.** `place {"at":[100,0],"rotation":90}` places the coil turned 90°; its
   connection points land at 90,0 and 110,0.
5. **Wire them.** `wire {"points":[[40,10],[70,10],[70,0],[90,0]]}`. Both ends land on connection
   points, so the line is a wire.
6. **Move it.** Move the rectangle with its connection point 5 mm down (select it, then `move` and
   two points, or the wheel's **Move**). The wire's end follows to 40,5 and stays attached.
7. **Undo and redo.** `u` three times, then `redo` three times: the drawing steps back and
   forward exactly, with the same IDs. `undo {"mode":"mark"}` is refused as not supported.
8. **A second page.** `newpage {"title":"2"}`, then `wire {"points":[[0,0],[30,0]]}`. Its ends are
   free, so it is a line with no wire yet and no label. Switch to page 1 and back: page 2 still
   holds it.
9. **Save, close and reopen.** Save as `~/ecad-projects/demo` (on Windows
   `C:\dev\ecad-projects\demo`), close the project and open the file again. The drawing, the
   connections and the undo history are as they were.
10. **The ID survived.** `setprop {"component":"<the ID you noted>","key":"tag","value":"-K1"}` is
    accepted (an ID that does not exist is refused). `u` undoes it; `u` again steps back into the
    history from before the save; `redo` brings it forward.

A device made this way has no tag until you give it one. Its device wheel offers **Make symbol**,
which turns the drawing into a project symbol you can place again ([Library](library.md)).

## Limits

- Tag texts and connection point labels are placed automatically; move a text with the text
  wheel's **Move text**. A tag text cannot be edited on its own: change the tag.
- User fields and links between devices are set by command only (`userfield`, `linkdevices`,
  `unlinkdevices`); no panel shows them yet.
- Placing a device from the Devices panel's **Not on a page yet** is not available yet.
- The inspector's **+ Accessory** and **From catalogue** under Parts do nothing yet.
